//! Modular structures on the GPU, made for hundreds of them breaking at once.
//!
//! - **One look per blueprint.** Every structure built from a blueprint (every ship of a kind)
//!   draws the same vertices, meshed once (`Mesher`) with all its parts, live or not; its coarser
//!   levels (`LodBuilder`) are built once too, off the frame's thread. A structure that is no
//!   blueprint's any more (a part lost a chip, a loose piece) has a look of its own.
//! - **A word per part.** What changes while a structure lives — a part gone, how damaged it is,
//!   whether it is lit — is one word per part in a storage buffer the shader reads by the part
//!   index its vertices carry. A hit, a part destroyed, a lamp going out write that word and
//!   nothing else: no look is ever rebuilt for them. A ship broken in two is two structures
//!   drawing the same look, each with the other half's parts gone.
//! - **Few draws.** Each frame every structure picks its level by distance (cross-faded with the
//!   next by dithering inside a band); structures drawing the same level of the same look go in
//!   one instanced draw. Shadows use a level coarser, each cascade only what it holds. What the
//!   owner says is hidden (behind the ground, inside a closed hull) and what would cover less
//!   than a pixel is not drawn.
use crate::{
    graph::{self, Draws, PrepareCx, RenderSystem, Stage},
    shader,
    shadow::MAX_CASCADES,
};
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Quat, Vec3};
use lunar_core::structure::{
    Library,
    look::{self, DecimatedLods, FlatMesher, LodBuilder, Mesher, NO_PART, Vertex},
    state::Structure,
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        mpsc::{Receiver, Sender, channel},
    },
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct VertexGpu {
    pos: [f32; 3],
    nrm: [i8; 4],
    albedo: [u8; 4],
    params: [u8; 4],
    /// Bone, fragment, glass, finish.
    extra: [u8; 4],
    /// Its part (`NO_PART`: none), what its part is (`look::INNER`, `look::WIRING`).
    ids: [u16; 2],
}

impl VertexGpu {
    fn new(v: &Vertex) -> VertexGpu {
        let n = |x: f32| (x.clamp(-1.0, 1.0) * 127.0).round() as i8;
        VertexGpu {
            pos: v.pos,
            nrm: [n(v.nrm[0]), n(v.nrm[1]), n(v.nrm[2]), 0],
            albedo: [v.albedo[0], v.albedo[1], v.albedo[2], v.damage],
            params: [v.rough, v.metal, v.glow, v.panel],
            extra: [v.bone, v.fragment, v.glass, v.finish],
            ids: [v.part, u16::from(v.inner)],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct XformGpu {
    /// Origin (camera-relative); w: dither (0 none, f > 0 keep where bayer < f, f < 0 where ≥ -f).
    pos: [f32; 4],
    rot: [f32; 4],
    /// First bone matrix of the structure, bone count, how it is shown (`MODE_*`), first part word.
    info: [u32; 4],
}

/// `XformGpu::info[2]`: as it is; 1-6: tinted by its level (a tool); `MODE_INTEGRITY`: its
/// integrity (sound parts dim, damaged ones glowing red by how much, missing ones purple).
pub const MODE_INTEGRITY: u32 = 100;

/// Coarser levels of a look, built off-thread.
struct Lods {
    model: usize,
    serial: u64,
    levels: Vec<Vec<Vertex>>,
}

/// Whose look: every structure of a blueprint's, or one structure's own.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Shared(Arc<str>),
    Own(u64),
}

/// A look in the pool: its levels, the full one first.
struct Model {
    key: Key,
    /// The build it is (levels coming back from the worker are for it or for an older one).
    serial: u64,
    /// An own look: the shapes it was meshed from (`Structure::shapes`).
    stamp: u64,
    vertices: Vec<VertexGpu>,
    levels: Vec<(u32, u32)>,
    /// Levels whose vertices say which part they are.
    ids: Vec<bool>,
    /// Vertices of the full look before its glass tail, and of its level 1 when that is the
    /// look without its models (whole, with its glass; else the whole level).
    opaque: u32,
    opaque1: u32,
    /// Level 1 is the look without its models.
    models: bool,
    /// Part words a structure drawing it has.
    parts: u32,
    offset: u32,
    cap: u32,
    /// From how far (m) each level 1.. may be drawn.
    dist: Vec<f32>,
    used: u64,
}

struct Slot {
    id: u64,
    model: usize,
    /// It draws its blueprint's look (`Structure::model`).
    shared: bool,
    version: u64,
    /// Its part words in the state buffer: first, count.
    state: (u32, u32),
    /// Every part of its look is there: any level will do. Else only the ones that tell parts
    /// apart.
    whole: bool,
    pos: Vec3,
    rot: Quat,
    sphere: (Vec3, f32),
    seen: u64,
    /// Its bones this frame (3 rows each) and where they went in the bone buffer.
    bones: Vec<[f32; 4]>,
    bone_base: u32,
}

/// Structures drawing one level of one look: transforms `first..first + count`.
#[derive(Clone, Copy)]
struct Run {
    model: u32,
    level: u32,
    first: u32,
    count: u32,
}

pub struct StructuresGpu {
    main: wgpu::RenderPipeline,
    glass: wgpu::RenderPipeline,
    depth: wgpu::RenderPipeline,
    bones: wgpu::Buffer,
    bone_cap: usize,
    staged_bones: Vec<[f32; 4]>,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    finishes: Finishes,
    pool: wgpu::Buffer,
    capacity: u32,
    top: u32,
    xforms: wgpu::Buffer,
    xform_cap: usize,
    /// A word per part of every structure (`look::part_states`), and what is in it.
    states: wgpu::Buffer,
    state_cpu: Vec<u32>,
    state_cap: usize,
    state_waste: usize,
    models: Vec<Option<Model>>,
    shared: HashMap<Arc<str>, usize>,
    slots: Vec<Slot>,
    by_id: HashMap<u64, usize>,
    staged: Vec<XformGpu>,
    scratch: Vec<Vertex>,
    words: Vec<u32>,
    /// This frame's draws: the main view, its glass, each cascade's shadows.
    items: Vec<(u32, u32, u32, f32)>,
    runs: Vec<Run>,
    glass_runs: Vec<Run>,
    shadow_runs: [Vec<Run>; MAX_CASCADES],
    mesher: Box<dyn Mesher>,
    lods: Arc<dyn LodBuilder>,
    density: f32,
    jobs: Sender<Job>,
    done: Receiver<Lods>,
    frame: u64,
    serial: u64,
    /// Structures the owner says are not seen (ids, sorted): only their shadows are drawn.
    hidden: Vec<u64>,
    /// The level each structure was drawn with last frame (id, level), for tools.
    pub shown: Vec<(u64, u8)>,
    /// Tool: each structure tinted by the level it is drawn with (0 as it is, 1 green, 2 cyan,
    /// 3 yellow, 4 orange, 5 red).
    pub tint_levels: bool,
    /// Structures shown by their integrity (ids, sorted): the scanner's view.
    integrity: Vec<u64>,
    /// Vertices left in the pool by looks dropped or outgrown: repacked past a share.
    wasted: u32,
    /// Looks meshed and part words written since the start (for tools and tests of what a hit
    /// costs).
    pub meshed: u64,
    pub words_written: u64,
}

/// A coarse-level job: the look (and which build of it), its full level, the same without its
/// models if it has any, its far shape.
type Job = (usize, u64, Vec<Vertex>, Option<Vec<Vertex>>, Option<Arc<lunar_core::mesh::Mesh>>);

const STRIDE: u64 = std::mem::size_of::<VertexGpu>() as u64;
/// Width of the cross-fade band: how far over its triangle budget a finer level still fades.
const FADE: f32 = 0.6;
/// A structure covering less than this many pixels across is not drawn; nor its shadow under this.
const MIN_PIXELS: f32 = 0.8;
const MIN_SHADOW_PIXELS: f32 = 2.5;
/// Looks of fewer triangles than this have no coarser levels.
const MIN_LOD_TRIANGLES: usize = 300;
/// A blueprint's look nobody draws is kept this many frames.
const KEEP_FRAMES: u64 = 1800;

fn pool(device: &wgpu::Device, vertices: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("structure vertices"), size: u64::from(vertices) * STRIDE, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

fn storage_buffer(device: &wgpu::Device, label: &str, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: bytes.max(16) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

fn xform_buffer(device: &wgpu::Device, n: usize) -> wgpu::Buffer {
    storage_buffer(device, "structure transforms", std::mem::size_of::<XformGpu>() * n.max(1))
}

fn bone_buffer(device: &wgpu::Device, n: usize) -> wgpu::Buffer {
    storage_buffer(device, "structure bones", 16 * n.max(3))
}

fn state_buffer(device: &wgpu::Device, n: usize) -> wgpu::Buffer {
    storage_buffer(device, "structure part words", 4 * n.max(4))
}

/// The finishes (`mesh::FINISHES`): a texture array, one layer per finish, mipmapped.
struct Finishes {
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl Finishes {
    /// `layers` of `size`² RGBA8 (layer after layer); none: one neutral layer.
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, size: u32, layers: u32, data: &[u8]) -> Finishes {
        let (size, layers, base): (u32, u32, Vec<u8>) = if layers == 0 || data.len() < (size * size * 4 * layers) as usize {
            (1, 1, vec![128, 128, 128, 128])
        } else {
            (size, layers, data[..(size * size * 4 * layers) as usize].to_vec())
        };
        let mips = 32 - size.leading_zeros();
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("finishes"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // every level, every layer: each level the 2x2 average of the one above (wrapping)
        let mut level = base;
        let mut s = size;
        for mip in 0..mips {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: mip, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &level,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s * 4), rows_per_image: Some(s) },
                wgpu::Extent3d { width: s, height: s, depth_or_array_layers: layers },
            );
            if s == 1 {
                break;
            }
            let h = s / 2;
            let mut next = vec![0u8; (h * h * 4 * layers) as usize];
            for l in 0..layers as usize {
                let src = &level[l * (s * s * 4) as usize..(l + 1) * (s * s * 4) as usize];
                let dst = &mut next[l * (h * h * 4) as usize..(l + 1) * (h * h * 4) as usize];
                for y in 0..h as usize {
                    for x in 0..h as usize {
                        for c in 0..4 {
                            let at = |xx: usize, yy: usize| u32::from(src[(yy * s as usize + xx) * 4 + c]);
                            let sum = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
                            dst[(y * h as usize + x) * 4 + c] = ((sum + 2) / 4) as u8;
                        }
                    }
                }
            }
            level = next;
            s = h;
        }
        let view = tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("finishes"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        Finishes { _tex: tex, view, sampler }
    }
}

fn group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, xforms: &wgpu::Buffer, bones: &wgpu::Buffer, states: &wgpu::Buffer, finishes: &Finishes) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("structures"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: xforms.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: bones.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&finishes.view) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&finishes.sampler) },
            wgpu::BindGroupEntry { binding: 4, resource: states.as_entire_binding() },
        ],
    })
}

/// The worker that builds coarse levels.
fn worker(lods: Arc<dyn LodBuilder>) -> (Sender<Job>, Receiver<Lods>) {
    let (jobs, rx) = channel::<Job>();
    let (tx, done) = channel();
    std::thread::Builder::new()
        .name("structure lods".into())
        .spawn(move || {
            while let Ok((model, serial, full, basic, far)) = rx.recv() {
                let mut levels = Vec::new();
                lods.build(&full, basic.as_deref(), far.as_deref(), &mut levels);
                if tx.send(Lods { model, serial, levels }).is_err() {
                    break;
                }
            }
        })
        .expect("a thread for structure LODs");
    (jobs, done)
}

impl StructuresGpu {
    /// The finishes' textures: `layers` of `size`² RGBA8, in `mesh::FINISHES` order from 1.
    pub fn set_finishes(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: u32, layers: u32, data: &[u8]) {
        self.finishes = Finishes::new(device, queue, size, layers, data);
        self.regroup(device);
    }

    fn regroup(&mut self, device: &wgpu::Device) {
        self.group = group(device, &self.layout, &self.xforms, &self.bones, &self.states, &self.finishes);
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, globals: &wgpu::BindGroupLayout) -> StructuresGpu {
        let module = shader(device, "structures", &[include_str!("shaders/common.wgsl"), include_str!("shaders/structure.wgsl")]);
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let fragment = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("structures"),
            entries: &[
                storage(0),
                storage(1),
                fragment(2, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false }),
                fragment(3, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                storage(4),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("structures"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        const ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm8x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Uint8x4, 5 => Uint16x2];
        let vb = [Some(wgpu::VertexBufferLayout { array_stride: STRIDE, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS })];
        let xforms = xform_buffer(device, 64);
        let bones = bone_buffer(device, 3 * 64);
        let states = state_buffer(device, 4096);
        let capacity = 1 << 16;
        let lods: Arc<dyn LodBuilder> = Arc::new(DecimatedLods::default());
        let density = lods.density();
        let (jobs, done) = worker(lods.clone());
        let finishes = Finishes::new(device, queue, 1, 0, &[]);
        StructuresGpu {
            main: crate::pipes::scene(device, &pl, &module, "structure_vs", Some("structure_fs"), &vb, Some(wgpu::Face::Back)),
            glass: crate::pipes::blended(device, &pl, &module, "structure_vs", "glass_fs", &vb, None),
            depth: crate::pipes::shadow(device, &pl, &module, "structure_depth_vs", &vb, None),
            group: group(device, &layout, &xforms, &bones, &states, &finishes),
            finishes,
            bones,
            bone_cap: 3 * 64,
            staged_bones: Vec::new(),
            layout,
            pool: pool(device, capacity),
            capacity,
            top: 0,
            xforms,
            xform_cap: 64,
            states,
            state_cpu: Vec::new(),
            state_cap: 4096,
            state_waste: 0,
            models: Vec::new(),
            shared: HashMap::new(),
            slots: Vec::new(),
            by_id: HashMap::new(),
            staged: Vec::new(),
            scratch: Vec::new(),
            words: Vec::new(),
            items: Vec::new(),
            runs: Vec::new(),
            glass_runs: Vec::new(),
            shadow_runs: Default::default(),
            mesher: Box::new(FlatMesher),
            lods,
            density,
            jobs,
            done,
            frame: 0,
            serial: 0,
            hidden: Vec::new(),
            shown: Vec::new(),
            tint_levels: false,
            integrity: Vec::new(),
            wasted: 0,
            meshed: 0,
            words_written: 0,
        }
    }

    /// Structures not seen this frame (only their shadows are drawn).
    pub fn set_hidden(&mut self, ids: &[u64]) {
        self.hidden.clear();
        self.hidden.extend_from_slice(ids);
        self.hidden.sort_unstable();
    }

    /// Structures shown by their integrity this frame (the scanner's view): sound parts dim,
    /// damaged ones glowing red by how much, missing ones purple.
    pub fn set_integrity(&mut self, ids: &[u64]) {
        self.integrity.clear();
        self.integrity.extend_from_slice(ids);
        self.integrity.sort_unstable();
    }

    /// Vertices of every look in the pool, and of full looks among them (for tools).
    pub fn pool_use(&self) -> (u64, u64) {
        let all = self.models.iter().flatten().map(|m| m.vertices.len() as u64).sum();
        let full = self.models.iter().flatten().map(|m| u64::from(m.levels.first().map_or(0, |l| l.1))).sum();
        (all, full)
    }

    /// Looks in the pool: (shared by a blueprint's structures, a structure's own).
    pub fn looks(&self) -> (usize, usize) {
        let shared = self.models.iter().flatten().filter(|m| matches!(m.key, Key::Shared(_))).count();
        (shared, self.models.iter().flatten().count() - shared)
    }

    /// Another look, or other levels, for every structure (rebuilt on the next sync).
    pub fn set_look(&mut self, mesher: Box<dyn Mesher>, lods: Box<dyn LodBuilder>) {
        self.mesher = mesher;
        self.density = lods.density();
        self.lods = Arc::from(lods);
        (self.jobs, self.done) = worker(self.lods.clone());
        self.models.clear();
        self.shared.clear();
        self.slots.clear();
        self.by_id.clear();
        self.state_cpu.clear();
        self.state_waste = 0;
        self.top = 0;
        self.wasted = 0;
    }

    /// Write a look's vertices to the pool (a new range if they outgrew theirs).
    fn store(&mut self, k: usize, queue: &wgpu::Queue) -> bool {
        let Some(m) = self.models[k].as_mut() else { return true };
        let len = m.vertices.len() as u32;
        if len > m.cap {
            let cap = len + len / 4 + 36;
            self.wasted += m.cap;
            if self.top + cap > self.capacity {
                // (no room: it has no place until the pool is laid out again — and must not be
                // taken for having one if it is stored again before that)
                m.cap = 0;
                return false;
            }
            m.offset = self.top;
            m.cap = cap;
            self.top += cap;
        }
        if len > 0 {
            queue.write_buffer(&self.pool, u64::from(m.offset) * STRIDE, bytemuck::cast_slice(&m.vertices));
        }
        true
    }

    /// Lay out a look's levels: the full one, then the coarse ones.
    fn lay(m: &mut Model, full: &[Vertex], coarse: &[Vec<Vertex>]) {
        m.vertices.clear();
        m.vertices.extend(full.iter().map(VertexGpu::new));
        m.levels.clear();
        m.ids.clear();
        m.levels.push((0, full.len() as u32));
        m.ids.push(true);
        for l in coarse {
            m.levels.push((m.vertices.len() as u32, l.len() as u32));
            m.ids.push(l.first().is_some_and(|v| v.part != NO_PART));
            m.vertices.extend(l.iter().map(VertexGpu::new));
        }
    }

    /// The full look of `key` meshed from `s` (a blueprint's own structure, or the structure
    /// itself) into look `k`, its coarse levels asked for.
    fn mesh(&mut self, k: usize, s: &Structure, lib: &Library, queue: &wgpu::Queue) -> bool {
        self.mesher.mesh(s, &lib.catalog, &mut self.scratch);
        self.meshed += 1;
        self.serial += 1;
        let serial = self.serial;
        let Some(m) = self.models[k].as_mut() else { return true };
        m.serial = serial;
        m.stamp = s.shapes;
        m.parts = s.parts.len() as u32;
        m.opaque = look::opaque_len(&self.scratch) as u32;
        // (a look with models: the same without them is its level 1, and what the rest come from)
        let basic = look::detailed(s, &lib.catalog).then(|| {
            let mut b = Vec::new();
            self.mesher.basic(s, &lib.catalog, &mut b);
            b
        });
        m.models = basic.is_some();
        m.opaque1 = basic.as_ref().map_or(u32::MAX, |b| look::opaque_len(b) as u32);
        m.dist = self.lods.distances(s.radius, m.models);
        // the coarse levels it had stand in until the new ones come (an own look that lost a chip)
        let keep: Vec<(Vec<VertexGpu>, bool)> = m.levels.iter().zip(&m.ids).skip(1).map(|(&(o, l), &id)| (m.vertices[o as usize..(o + l) as usize].to_vec(), id)).collect();
        StructuresGpu::lay(m, &self.scratch, &[]);
        for (v, id) in keep {
            m.levels.push((m.vertices.len() as u32, v.len() as u32));
            m.ids.push(id);
            m.vertices.extend(v);
        }
        let far = match &m.key {
            Key::Shared(name) => lib.catalog.far_of(name),
            Key::Own(_) => None,
        };
        // a look of a few triangles (a loose piece) is its own only level
        if self.scratch.len() / 3 >= MIN_LOD_TRIANGLES {
            let _ = self.jobs.send((k, serial, std::mem::take(&mut self.scratch), basic, far));
        } else {
            m.dist.clear();
            m.models = false;
        }
        self.store(k, queue)
    }

    fn new_model(&mut self, key: Key) -> usize {
        let m = Model { key, serial: 0, stamp: u64::MAX, vertices: Vec::new(), levels: Vec::new(), ids: Vec::new(), opaque: 0, opaque1: u32::MAX, models: false, parts: 0, offset: 0, cap: 0, dist: Vec::new(), used: self.frame };
        match self.models.iter().position(Option::is_none) {
            Some(k) => {
                self.models[k] = Some(m);
                k
            }
            None => {
                self.models.push(Some(m));
                self.models.len() - 1
            }
        }
    }

    fn drop_model(&mut self, k: usize) {
        if let Some(m) = self.models[k].take() {
            self.wasted += m.cap;
            if let Key::Shared(name) = &m.key {
                self.shared.remove(name);
            }
        }
    }

    /// The part words of slot `k` written (its own range of the state buffer, a new one if it
    /// needs more).
    fn write_words(&mut self, k: usize, s: &Structure, queue: &wgpu::Queue) -> bool {
        let slot = &mut self.slots[k];
        let n = self.models[slot.model].as_ref().map_or(0, |m| m.parts as usize);
        look::part_states(s, n, &mut self.words);
        slot.whole = self.words.iter().all(|w| w & look::ALIVE != 0);
        if slot.state.1 as usize != n {
            self.state_waste += slot.state.1 as usize;
            slot.state = (self.state_cpu.len() as u32, n as u32);
            self.state_cpu.resize(self.state_cpu.len() + n, 0);
        }
        let (at, len) = (slot.state.0 as usize, slot.state.1 as usize);
        self.state_cpu[at..at + len].copy_from_slice(&self.words);
        self.words_written += len as u64;
        if self.state_cpu.len() > self.state_cap {
            return false;
        }
        if len > 0 {
            queue.write_buffer(&self.states, (at * 4) as u64, bytemuck::cast_slice(&self.words));
        }
        true
    }

    /// This frame's structures seen from `eye`: a look meshed for each blueprint (or structure of
    /// its own shapes) not seen before, the words of the parts that changed written, finished
    /// levels taken in, gone ones dropped.
    pub fn sync(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, list: &[Structure], lib: &Library, eye: DVec3) {
        self.frame += 1;
        let frame = self.frame;
        let (mut repack, mut regrow) = (false, false);
        for s in list {
            let k = match self.by_id.get(&s.id) {
                Some(&k) => k,
                None => {
                    self.slots.push(Slot { id: s.id, model: usize::MAX, shared: false, version: u64::MAX, state: (0, 0), whole: true, pos: Vec3::ZERO, rot: Quat::IDENTITY, sphere: (Vec3::ZERO, 0.0), seen: 0, bones: Vec::new(), bone_base: 0 });
                    self.by_id.insert(s.id, self.slots.len() - 1);
                    self.slots.len() - 1
                }
            };
            // whose look it draws: its blueprint's while it keeps its shapes, else its own
            if self.slots[k].model == usize::MAX || self.slots[k].shared != s.model {
                let old = self.slots[k].model;
                if old != usize::MAX && !self.slots[k].shared {
                    self.drop_model(old);
                }
                let bp = if s.model { lib.blueprint_named(&s.name) } else { None };
                let model = match bp {
                    Some(bp) => match self.shared.get(&s.name) {
                        Some(&m) => m,
                        None => {
                            let m = self.new_model(Key::Shared(s.name.clone()));
                            self.shared.insert(s.name.clone(), m);
                            let whole = Structure::new(0, bp, &lib.catalog, DVec3::ZERO, Quat::IDENTITY);
                            repack |= !self.mesh(m, &whole, lib, queue);
                            m
                        }
                    },
                    None => self.new_model(Key::Own(s.id)),
                };
                let slot = &mut self.slots[k];
                slot.model = model;
                slot.shared = bp.is_some();
                slot.version = u64::MAX;
            }
            let slot = &mut self.slots[k];
            slot.seen = frame;
            slot.pos = (s.pos - eye).as_vec3();
            slot.rot = s.rot;
            slot.sphere = (slot.pos + s.rot * s.center, s.radius);
            // the pose of its moving parts
            slot.bones.clear();
            for b in s.bones.iter().skip(1) {
                let m = b.matrix3;
                let t = b.translation;
                slot.bones.push([m.x_axis.x, m.y_axis.x, m.z_axis.x, t.x]);
                slot.bones.push([m.x_axis.y, m.y_axis.y, m.z_axis.y, t.y]);
                slot.bones.push([m.x_axis.z, m.y_axis.z, m.z_axis.z, t.z]);
            }
            let model = slot.model;
            if let Some(m) = self.models[model].as_mut() {
                m.used = frame;
                // its own look follows its shapes
                if matches!(m.key, Key::Own(_)) && m.stamp != s.shapes {
                    repack |= !self.mesh(model, s, lib, queue);
                    self.slots[k].version = u64::MAX;
                }
            }
            if self.slots[k].version != s.version {
                self.slots[k].version = s.version;
                regrow |= !self.write_words(k, s, queue);
            }
        }
        // coarse levels that came back
        while let Ok(l) = self.done.try_recv() {
            let Some(m) = self.models.get_mut(l.model).and_then(Option::as_mut).filter(|m| m.serial == l.serial) else { continue };
            let full: Vec<VertexGpu> = m.vertices[..m.levels[0].1 as usize].to_vec();
            m.vertices = full;
            m.levels.truncate(1);
            m.ids.truncate(1);
            for lv in &l.levels {
                m.levels.push((m.vertices.len() as u32, lv.len() as u32));
                m.ids.push(lv.first().is_some_and(|v| v.part != NO_PART));
                m.vertices.extend(lv.iter().map(VertexGpu::new));
            }
            repack |= !self.store(l.model, queue);
        }
        // structures gone, and the looks nobody draws any more
        if self.slots.iter().any(|s| s.seen != frame) {
            let mut gone = Vec::new();
            for s in self.slots.iter().filter(|s| s.seen != frame) {
                self.state_waste += s.state.1 as usize;
                if !s.shared {
                    gone.push(s.model);
                }
            }
            self.slots.retain(|s| s.seen == frame);
            self.by_id.clear();
            self.by_id.extend(self.slots.iter().enumerate().map(|(k, s)| (s.id, k)));
            for m in gone {
                self.drop_model(m);
            }
        }
        for k in 0..self.models.len() {
            if self.models[k].as_ref().is_some_and(|m| matches!(m.key, Key::Shared(_)) && frame - m.used > KEEP_FRAMES) {
                self.drop_model(k);
            }
        }
        // the part words packed again when holes are most of them, the buffer grown when they
        // do not fit
        if self.state_waste * 2 > self.state_cpu.len().max(4096) {
            let mut packed = Vec::with_capacity(self.state_cpu.len() - self.state_waste);
            for s in &mut self.slots {
                let (at, len) = (s.state.0 as usize, s.state.1 as usize);
                s.state.0 = packed.len() as u32;
                packed.extend_from_slice(&self.state_cpu[at..at + len]);
            }
            self.state_cpu = packed;
            self.state_waste = 0;
            regrow = true;
        }
        if regrow {
            if self.state_cpu.len() > self.state_cap {
                self.state_cap = (self.state_cpu.len() * 2).next_power_of_two();
                self.states = state_buffer(device, self.state_cap);
                self.regroup(device);
            }
            if !self.state_cpu.is_empty() {
                queue.write_buffer(&self.states, 0, bytemuck::cast_slice(&self.state_cpu));
            }
        }
        repack |= self.wasted > self.capacity / 4;
        if repack {
            self.repack(device, queue);
        }
    }

    /// Lay every look out again from the start (growing the pool if it must).
    fn repack(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let need: u32 = self.models.iter().flatten().map(|m| m.vertices.len() as u32 * 5 / 4 + 36).sum();
        if need > self.capacity {
            self.capacity = (need * 2).next_power_of_two();
            self.pool = pool(device, self.capacity);
        }
        self.top = 0;
        self.wasted = 0;
        for k in 0..self.models.len() {
            if let Some(m) = self.models[k].as_mut() {
                m.cap = 0;
            }
            self.store(k, queue);
        }
    }

    fn draw_runs(&self, pass: &mut wgpu::RenderPass, runs: &[Run], glass: bool) -> Draws {
        let mut d = Draws::default();
        if runs.is_empty() {
            return d;
        }
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, self.pool.slice(..));
        for r in runs {
            let Some(m) = self.models[r.model as usize].as_ref() else { continue };
            let (o, len) = m.levels[r.level as usize];
            // the full look without its glass (drawn apart, blended)
            let (from, to) = match (r.level, glass) {
                (0, true) => (m.opaque.min(len), len),
                (0, false) => (0, len.min(m.opaque)),
                (1, true) if m.models => (m.opaque1.min(len), len),
                (1, false) if m.models => (0, len.min(m.opaque1)),
                _ => (0, len),
            };
            if to > from {
                pass.draw(m.offset + o + from..m.offset + o + to, r.first..r.first + r.count);
                d.calls += 1;
                d.triangles += u64::from((to - from) / 3) * u64::from(r.count);
            }
        }
        d
    }

    /// `items` (look, level, slot, fade) staged as transforms, those of one level of one look
    /// together: one run each.
    fn stage(items: &mut Vec<(u32, u32, u32, f32)>, slots: &[Slot], mode: impl Fn(&Slot, u32) -> u32, staged: &mut Vec<XformGpu>, runs: &mut Vec<Run>) {
        items.sort_unstable_by_key(|i| (i.0, i.1));
        for &(model, level, k, fade) in items.iter() {
            let s = &slots[k as usize];
            staged.push(XformGpu { pos: [s.pos.x, s.pos.y, s.pos.z, fade], rot: s.rot.to_array(), info: [s.bone_base, (s.bones.len() / 3) as u32, mode(s, level), s.state.0] });
            let at = staged.len() as u32 - 1;
            match runs.last_mut() {
                Some(r) if r.model == model && r.level == level && r.first + r.count == at => r.count += 1,
                _ => runs.push(Run { model, level, first: at, count: 1 }),
            }
        }
        items.clear();
    }
}

impl RenderSystem for StructuresGpu {
    fn stages(&self) -> u8 {
        if self.slots.is_empty() { 0 } else { graph::OPAQUE | graph::SHADOW | graph::TRANSPARENT }
    }

    /// Level per structure by its distance; transforms for this frame's draws.
    fn prepare(&mut self, cx: &PrepareCx) {
        self.staged.clear();
        self.runs.clear();
        self.glass_runs.clear();
        for r in &mut self.shadow_runs {
            r.clear();
        }
        // every slot's bones into the shared buffer
        self.staged_bones.clear();
        for s in &mut self.slots {
            s.bone_base = (self.staged_bones.len() / 3) as u32;
            self.staged_bones.extend_from_slice(&s.bones);
        }
        let px_per = cx.viewport.1 as f32 / (2.0 * (cx.view.fov_y * 0.5).tan()) * cx.settings.lod_bias;
        let density = self.density;
        let models = &self.models;
        // pixels across
        let pixels = |s: &Slot| 2.0 * s.sphere.1 / s.sphere.0.length().max(1e-3) * px_per;
        // how many times over the budget a level is (≤ 1: it may be drawn)
        let load = |s: &Slot, m: &Model, level: usize| {
            let px = pixels(s) * 0.5;
            (m.levels[level].1 / 3) as f32 / (density * std::f32::consts::PI * px * px).max(1e-6)
        };
        // the level its distance asks, of the ones it has
        let by_distance = |s: &Slot, m: &Model| {
            let d = s.sphere.0.length();
            m.dist.iter().filter(|&&x| d >= x).count().min(m.levels.len() - 1)
        };
        // by distance when its levels say from where, else by density
        let pick = |s: &Slot, m: &Model| if m.dist.is_empty() { (0..m.levels.len()).find(|&l| load(s, m, l) <= 1.0).unwrap_or(m.levels.len() - 1) } else { by_distance(s, m) };
        // one that has lost parts: no coarser than the last level that tells them apart
        let apart = |s: &Slot, m: &Model, mut l: usize| {
            while !s.whole && l > 0 && !m.ids[l] {
                l -= 1;
            }
            l
        };
        let (tint, integrity) = (self.tint_levels, &self.integrity);
        let mode = |s: &Slot, level: u32| {
            if integrity.binary_search(&s.id).is_ok() {
                MODE_INTEGRITY
            } else if tint {
                level + 1
            } else {
                0
            }
        };
        self.shown.clear();
        let mut glass: Vec<(u32, u32, u32, f32)> = Vec::new();
        for (k, s) in self.slots.iter().enumerate() {
            let Some(m) = models[s.model].as_ref().filter(|m| !m.levels.is_empty()) else { continue };
            if !cx.main.sphere(s.sphere.0, s.sphere.1) || pixels(s) < MIN_PIXELS || self.hidden.binary_search(&s.id).is_ok() {
                continue;
            }
            let level = apart(s, m, pick(s, m));
            self.shown.push((s.id, level as u8));
            let gate = apart(s, m, by_distance(s, m));
            // a little over the finer level's budget: it fades in over this one (0 → 1 at budget);
            // past a distance step, the same over a tenth of it
            let f = if level == 0 {
                -1.0
            } else if level == gate && m.levels[level - 1].1 > 0 && !m.dist.is_empty() {
                let th = m.dist[level - 1];
                1.0 - (s.sphere.0.length() - th) / (th * 0.1)
            } else if level > gate {
                1.0 - (load(s, m, level - 1) - 1.0) / FADE
            } else {
                -1.0
            };
            let (model, k) = (s.model as u32, k as u32);
            if level > 0 && f > 0.0 && f < 1.0 {
                self.items.push((model, level as u32, k, -f.max(1e-3)));
                self.items.push((model, level as u32 - 1, k, f.max(1e-3)));
            } else {
                self.items.push((model, level as u32, k, 0.0));
            }
            // its glass, while the full look shows (with its models or without them)
            if level == 0 && m.opaque < m.levels[0].1 {
                glass.push((model, 0, k, 0.0));
            } else if level == 1 && m.models && m.opaque1 < m.levels[1].1 {
                glass.push((model, 1, k, 0.0));
            }
        }
        StructuresGpu::stage(&mut self.items, &self.slots, mode, &mut self.staged, &mut self.runs);
        StructuresGpu::stage(&mut glass, &self.slots, mode, &mut self.staged, &mut self.glass_runs);
        // shadows: per cascade what it holds, a level coarser than it shows (the full one without
        // its glass, so the sun comes in through the windows, while it shows full)
        for c in 0..cx.cascades.count.min(MAX_CASCADES) {
            for (k, s) in self.slots.iter().enumerate() {
                let Some(m) = models[s.model].as_ref().filter(|m| !m.levels.is_empty()) else { continue };
                if pixels(s) < MIN_SHADOW_PIXELS || !cx.cascades.frusta[c].sphere(s.sphere.0, s.sphere.1) {
                    continue;
                }
                let shown = pick(s, m);
                let level = apart(s, m, if shown == 0 { 0 } else { (shown + 1).min(m.levels.len() - 1) });
                self.items.push((s.model as u32, level as u32, k as u32, 0.0));
            }
            StructuresGpu::stage(&mut self.items, &self.slots, |_, _| 0, &mut self.staged, &mut self.shadow_runs[c]);
        }
        let mut regroup = false;
        if self.staged.len() > self.xform_cap {
            self.xform_cap = self.staged.len().next_power_of_two();
            self.xforms = xform_buffer(cx.device, self.xform_cap);
            regroup = true;
        }
        if self.staged_bones.len() > self.bone_cap {
            self.bone_cap = self.staged_bones.len().next_power_of_two();
            self.bones = bone_buffer(cx.device, self.bone_cap);
            regroup = true;
        }
        if regroup {
            self.regroup(cx.device);
        }
        if !self.staged.is_empty() {
            cx.queue.write_buffer(&self.xforms, 0, bytemuck::cast_slice(&self.staged));
        }
        if !self.staged_bones.is_empty() {
            cx.queue.write_buffer(&self.bones, 0, bytemuck::cast_slice(&self.staged_bones));
        }
    }

    fn draw(&mut self, pass: &mut wgpu::RenderPass, stage: Stage) -> Draws {
        match stage {
            Stage::Opaque => {
                pass.set_pipeline(&self.main);
                self.draw_runs(pass, &self.runs, false)
            }
            // they move and break: redrawn with the moving casters every frame
            Stage::Shadow { cascade, moving: true, .. } if cascade < MAX_CASCADES => {
                pass.set_pipeline(&self.depth);
                self.draw_runs(pass, &self.shadow_runs[cascade], false)
            }
            Stage::Transparent => {
                pass.set_pipeline(&self.glass);
                self.draw_runs(pass, &self.glass_runs, true)
            }
            _ => Draws::default(),
        }
    }

    fn gpu_bytes(&self) -> u64 {
        self.pool.size() + self.xforms.size() + self.bones.size() + self.states.size()
    }
}
