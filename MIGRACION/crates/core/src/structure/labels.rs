//! What is written on parts: a stencil on a hull plate, a plate on a machine, what a drum holds.
//! A label is of its part's kind (`catalog::Label`, in the part's frame), so it goes wherever the
//! part goes — a drum keeps its lettering when it leaves the hold — and any structure may carry
//! them. `show` lays out those near enough to read as glyphs of the silkscreen font for the
//! props pass; nothing is kept between frames.
use super::{catalog::Catalog, state::Structure};
use crate::{
    font::{Font, Placed},
    props::{GlyphQuad, PropScene},
};
use glam::Vec3;

/// From how many letter heights away a label is drawn (a centimetre letter, a few metres; a
/// hull's stencil, from afar), and the least and the most that comes to (m).
pub const REACH: f32 = 350.0;
pub const NEAREST: f32 = 3.0;
pub const FARTHEST: f32 = 400.0;

/// Whether `s` has anything written on it.
pub fn any(s: &Structure, cat: &Catalog) -> bool {
    s.parts.iter().any(|p| p.alive && !p.fragment && !cat.parts[usize::from(p.kind)].def.labels.is_empty())
}

/// The labels of `s` that can be read from `eye` (in its frame) as glyphs in `out`, each in the
/// frame `frame` gives for where it is (the structure's own, or its inside's). `scratch` is
/// working room kept by the caller. Returns how many glyphs it added.
pub fn show(s: &Structure, cat: &Catalog, font: &Font, eye: Vec3, frame: impl Fn(Vec3) -> u16, scratch: &mut Vec<Placed>, out: &mut PropScene) -> usize {
    let before = out.glyphs.len();
    for p in &s.parts {
        let labels = &cat.parts[usize::from(p.kind)].def.labels;
        // (a broken piece has lost its lettering with its paint)
        if labels.is_empty() || !p.alive || p.fragment {
            continue;
        }
        for l in labels {
            let (at, u, v) = (p.local.transform_point3(Vec3::from_array(l.at)), p.local.transform_vector3(Vec3::from_array(l.u)), p.local.transform_vector3(Vec3::from_array(l.v)));
            let to_eye = eye - at;
            let h = u.length().max(1e-6);
            let reach = (h * REACH).clamp(NEAREST, FARTHEST);
            // read from the front (what goes round a drum is hidden by the drum where it faces away)
            if to_eye.length_squared() > reach * reach || (l.radius <= 0.0 && u.cross(v).dot(to_eye) <= 0.0) {
                continue;
            }
            let frame = frame(at);
            scratch.clear();
            font.layout(&l.text, l.align, scratch);
            if l.radius > 0.0 {
                // each letter where it falls round the cylinder, facing out of it
                let (across, n, r) = (u / h, u.cross(v).normalize_or(Vec3::Z), l.radius);
                for g in scratch.iter() {
                    let (sin, cos) = (g.center[0] * h / r).sin_cos();
                    let center = at - n * r + (n * cos + across * sin) * r + v * g.center[1];
                    out.glyphs.push(GlyphQuad { frame, center, u: (across * cos - n * sin) * (g.half[0] * h), v: v * g.half[1], uv: g.uv, color: l.color, emissive: l.emissive, relief: l.relief });
                }
            } else {
                for g in scratch.iter() {
                    out.glyphs.push(GlyphQuad { frame, center: at + u * g.center[0] + v * g.center[1], u: u * g.half[0], v: v * g.half[1], uv: g.uv, color: l.color, emissive: l.emissive, relief: l.relief });
                }
            }
        }
    }
    out.glyphs.len() - before
}
