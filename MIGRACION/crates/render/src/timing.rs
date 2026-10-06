//! GPU time per pass from timestamp queries, read back a few frames late without ever blocking.
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuPass {
    TerrainGen = 0,
    Cull = 1,
    Shadows = 2,
    Main = 3,
    HiZ = 4,
    Post = 5,
    Overlay = 6,
}
pub const PASSES: usize = 7;
pub const PASS_NAMES: [&str; PASSES] = ["terreno gen", "culling", "sombras", "principal", "hi-z", "post", "overlay"];

const RING: usize = 4;
const FREE: u8 = 0;
const MAPPING: u8 = 1;
const READY: u8 = 2;

pub struct GpuTimer {
    set: Option<wgpu::QuerySet>,
    resolve: Option<wgpu::Buffer>,
    ring: Vec<(wgpu::Buffer, Arc<AtomicU8>, u32)>,
    period_ns: f32,
    slot: usize,
    written: u32,
    /// Smoothed milliseconds per pass.
    pub ms: [f32; PASSES],
    pub last: [f32; PASSES],
}

impl GpuTimer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, enabled: bool) -> GpuTimer {
        let count = (PASSES * 2) as u32;
        let size = u64::from(count) * 8;
        let (set, resolve, ring) = if enabled {
            let set = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("timestamps"), ty: wgpu::QueryType::Timestamp, count });
            let resolve = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamp resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let ring = (0..RING)
                .map(|_| {
                    let b = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("timestamp readback"),
                        size,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    });
                    (b, Arc::new(AtomicU8::new(FREE)), 0)
                })
                .collect();
            (Some(set), Some(resolve), ring)
        } else {
            (None, None, Vec::new())
        };
        GpuTimer { set, resolve, ring, period_ns: queue.get_timestamp_period(), slot: 0, written: 0, ms: [0.0; PASSES], last: [0.0; PASSES] }
    }

    pub fn enabled(&self) -> bool {
        self.set.is_some()
    }

    /// Begin/end indices for `pass` this frame (None when timing is off or the slot is busy).
    pub fn pass(&mut self, pass: GpuPass) -> Option<(&wgpu::QuerySet, u32)> {
        let free = self.ring.get(self.slot).is_some_and(|r| r.1.load(Ordering::Acquire) == FREE);
        if !free {
            return None;
        }
        self.written |= 1 << pass as u32;
        self.set.as_ref().map(|s| (s, pass as u32 * 2))
    }

    pub fn compute(&mut self, pass: GpuPass) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        self.pass(pass).map(|(q, i)| wgpu::ComputePassTimestampWrites { query_set: q, beginning_of_pass_write_index: Some(i), end_of_pass_write_index: Some(i + 1) })
    }

    pub fn render(&mut self, pass: GpuPass) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.pass(pass).map(|(q, i)| wgpu::RenderPassTimestampWrites { query_set: q, beginning_of_pass_write_index: Some(i), end_of_pass_write_index: Some(i + 1) })
    }

    /// After the frame's passes: resolve into this frame's readback slot.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(set), Some(resolve)) = (&self.set, &self.resolve) else { return };
        if self.written == 0 {
            return;
        }
        let (buf, state, mask) = &mut self.ring[self.slot];
        if state.load(Ordering::Acquire) != FREE {
            return;
        }
        encoder.resolve_query_set(set, 0..(PASSES * 2) as u32, resolve, 0);
        encoder.copy_buffer_to_buffer(resolve, 0, buf, 0, resolve.size());
        *mask = self.written;
    }

    /// After submit: start mapping this frame's slot; harvest finished ones.
    pub fn after_submit(&mut self, device: &wgpu::Device) {
        if self.set.is_none() {
            return;
        }
        let slot = self.slot;
        if self.written != 0 && self.ring[slot].1.load(Ordering::Acquire) == FREE {
            let state = self.ring[slot].1.clone();
            state.store(MAPPING, Ordering::Release);
            self.ring[slot].0.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                state.store(if r.is_ok() { READY } else { FREE }, Ordering::Release);
            });
        }
        self.written = 0;
        self.slot = (self.slot + 1) % RING;
        let _ = device.poll(wgpu::PollType::Poll);
        for (buf, state, mask) in &mut self.ring {
            if state.load(Ordering::Acquire) != READY {
                continue;
            }
            if let Ok(data) = buf.slice(..).get_mapped_range() {
                let ts: &[u64] = bytemuck::cast_slice(&data);
                for p in 0..PASSES {
                    let v = if *mask & (1 << p) != 0 && ts[p * 2 + 1] >= ts[p * 2] {
                        (ts[p * 2 + 1] - ts[p * 2]) as f32 * self.period_ns / 1e6
                    } else {
                        0.0
                    };
                    self.last[p] = v;
                    self.ms[p] += (v - self.ms[p]) * 0.1;
                }
            }
            buf.unmap();
            state.store(FREE, Ordering::Release);
        }
    }

    pub fn total(&self) -> f32 {
        self.ms.iter().sum()
    }
}
