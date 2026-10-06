//! Benchmark: a fixed, reproducible camera route (walk, look at the NPCs, look at the sky, fly
//! over the fleet) and per-segment statistics written as JSON.
use glam::DVec3;
use lunar_core::{body::BodyRegistry, scene::Site};
use std::sync::Arc;
use lunar_render::{Stats, View, timing::PASS_NAMES};
use std::fmt::Write;

pub const SEGMENTS: [(&str, f64); 4] = [("andar", 0.3), ("mirar_npc", 0.25), ("mirar_cielo", 0.15), ("volar_flota", 0.3)];
/// Seconds at the start of the route that are not recorded (streaming, shader warm-up).
pub const WARMUP: f64 = 3.0;

#[derive(Clone, Copy, Default)]
pub struct Sample {
    pub segment: u8,
    pub frame_ms: f32,
    pub cpu_ms: f32,
    pub sim_ms: f32,
    pub phases: [f32; 5],
    pub gpu: [f32; 7],
    pub draws: u32,
    pub triangles: u64,
}

pub struct Bench {
    pub seconds: f64,
    pub t: f64,
    pub samples: Vec<Sample>,
    pub label: String,
    site: Site,
    bodies: Arc<BodyRegistry>,
    look_npc: DVec3,
    fov_y: f32,
    near: f32,
}

impl Bench {
    /// `camera`: vertical field of view (rad) and near plane.
    pub fn new(seconds: f64, label: String, npc_homes: &[DVec3], site: Site, bodies: Arc<BodyRegistry>, camera: (f32, f32)) -> Bench {
        // the nearest dozen NPC homes: where the route looks
        let b = bodies.get(site.body);
        let origin = b.center + site.dir * b.radius;
        let mut near: Vec<DVec3> = npc_homes.to_vec();
        near.sort_by(|a, b| a.distance(origin).total_cmp(&b.distance(origin)));
        let look = if near.is_empty() { origin + site.north * 100.0 } else { near.iter().take(12).fold(DVec3::ZERO, |s, p| s + *p) / near.len().min(12) as f64 };
        Bench { seconds, t: 0.0, samples: Vec::with_capacity((seconds * 400.0) as usize + 64), label, site, bodies, look_npc: look, fov_y: camera.0, near: camera.1 }
    }

    pub fn done(&self) -> bool {
        self.t >= WARMUP + self.seconds
    }

    pub fn segment(&self) -> usize {
        let u = ((self.t - WARMUP).max(0.0) / self.seconds).clamp(0.0, 0.9999);
        let mut acc = 0.0;
        for (i, (_, f)) in SEGMENTS.iter().enumerate() {
            acc += f;
            if u < acc {
                return i;
            }
        }
        SEGMENTS.len() - 1
    }

    /// The camera at the current time of the route.
    pub fn view(&self) -> View {
        let seg = self.segment();
        let mut start = 0.0;
        for (_, f) in SEGMENTS.iter().take(seg) {
            start += f;
        }
        let local = (((self.t - WARMUP).max(0.0) / self.seconds - start) / SEGMENTS[seg].1).clamp(0.0, 1.0);
        let secs = local * SEGMENTS[seg].1 * self.seconds;
        let site = &self.site;
        let body = self.bodies.get(site.body);
        let eye_on_ground = |dir: DVec3, h: f64| body.above_ground(dir, h);
        let (eye, target) = match seg {
            0 => {
                let d = site.at(secs * 1.8, 0.0);
                let eye = eye_on_ground(d, 1.75);
                let ahead = eye_on_ground(site.at(secs * 1.8 + 30.0, 0.0), 0.0);
                (eye, ahead)
            }
            1 => {
                let eye = eye_on_ground(site.dir, 1.75);
                let pan = (local * std::f64::consts::TAU).sin() * 15.0;
                let to = self.look_npc - eye;
                let side = to.cross(body.up(eye)).normalize();
                (eye, self.look_npc + side * pan)
            }
            2 => {
                let eye = eye_on_ground(site.dir, 1.75);
                let up = body.up(eye);
                let yaw = local * 1.5;
                let dir = (site.east * yaw.cos() + site.north * yaw.sin()) * 0.5 + up;
                (eye, eye + dir * 100.0)
            }
            _ => {
                let a = local * std::f64::consts::TAU * 0.5;
                let d = site.at(a.cos() * 700.0, a.sin() * 700.0);
                let eye = eye_on_ground(d, 280.0);
                (eye, eye_on_ground(site.dir, 0.0))
            }
        };
        let up = body.up(eye);
        let forward = (target - eye).normalize();
        View { eye, forward, up, fov_y: self.fov_y, near: self.near }
    }

    pub fn record(&mut self, frame_ms: f32, sim_ms: f32, st: &Stats) {
        if self.t < WARMUP {
            return;
        }
        let phases = [st.cpu_prepare_ms, st.cpu_terrain_ms, st.cpu_scene_ms, st.cpu_encode_ms, st.cpu_submit_ms];
        self.samples.push(Sample {
            segment: self.segment() as u8,
            frame_ms,
            cpu_ms: sim_ms + phases.iter().sum::<f32>(),
            sim_ms,
            phases,
            gpu: st.gpu_ms,
            draws: st.draws,
            triangles: st.triangles,
        });
    }
}

fn stats(v: &mut [f32]) -> (f32, f32, f32) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    v.sort_by(f32::total_cmp);
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    let p99 = v[((v.len() as f32 * 0.99).ceil() as usize).saturating_sub(1)];
    (mean, p99, v[v.len() - 1])
}

fn json_stats(out: &mut String, name: &str, v: &mut [f32]) {
    let (mean, p99, max) = stats(v);
    let _ = write!(out, "\"{name}\":{{\"mean\":{mean:.3},\"p99\":{p99:.3},\"max\":{max:.3}}}");
}

/// One bench run as a JSON object.
pub fn report(b: &Bench, adapter: &str, preset: &str, ships: usize, npcs: usize) -> String {
    let mut out = String::new();
    let _ = write!(out, "{{\"label\":\"{}\",\"adapter\":\"{}\",\"preset\":\"{}\",\"ships\":{ships},\"npcs\":{npcs},\"seconds\":{},\"frames\":{},\"segments\":[", b.label, adapter.replace('"', "'"), preset, b.seconds, b.samples.len());
    let mut all: Vec<Option<u8>> = (0..SEGMENTS.len() as u8).map(Some).collect();
    all.push(None);
    for (k, seg) in all.iter().enumerate() {
        let s: Vec<&Sample> = b.samples.iter().filter(|x| seg.is_none_or(|g| x.segment == g)).collect();
        let name = seg.map_or("total", |g| SEGMENTS[g as usize].0);
        let col = |f: &dyn Fn(&Sample) -> f32| s.iter().map(|x| f(x)).collect::<Vec<f32>>();
        if k > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"name\":\"{name}\",\"frames\":{},", s.len());
        json_stats(&mut out, "frame_ms", &mut col(&|x| x.frame_ms));
        out.push(',');
        json_stats(&mut out, "cpu_ms", &mut col(&|x| x.cpu_ms));
        let mean = |v: Vec<f32>| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
        let _ = write!(
            out,
            ",\"cpu_phases_ms\":{{\"sim\":{:.3},\"prepare\":{:.3},\"terrain\":{:.3},\"scene\":{:.3},\"encode\":{:.3},\"submit\":{:.3}}}",
            mean(col(&|x| x.sim_ms)),
            mean(col(&|x| x.phases[0])),
            mean(col(&|x| x.phases[1])),
            mean(col(&|x| x.phases[2])),
            mean(col(&|x| x.phases[3])),
            mean(col(&|x| x.phases[4]))
        );
        out.push_str(",\"gpu_ms\":{");
        for (p, name) in PASS_NAMES.iter().enumerate() {
            let _ = write!(out, "{}\"{}\":{:.3}", if p > 0 { "," } else { "" }, name, mean(col(&|x| x.gpu[p])));
        }
        let _ = write!(out, ",\"total\":{:.3}}}", mean(col(&|x| x.gpu.iter().sum())));
        let draws = col(&|x| x.draws as f32);
        let tris = col(&|x| x.triangles as f32);
        let _ = write!(
            out,
            ",\"draws\":{{\"mean\":{:.1},\"max\":{}}},\"triangles\":{{\"mean\":{:.0},\"max\":{}}}}}",
            mean(draws.clone()),
            draws.iter().fold(0.0f32, |a, b| a.max(*b)),
            mean(tris.clone()),
            tris.iter().fold(0.0f32, |a, b| a.max(*b))
        );
    }
    out.push_str("]}");
    out
}
