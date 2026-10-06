//! Closures: anything that shuts an opening — ramps, doors, hatches, lids, locker doors — made one
//! way. The opening (a hull end, a bulkhead's doorway, a rectangle anywhere) gives the leaf its
//! outline: overlapping the frame round it (so it seals) or flush in it; the hinge or the slide
//! its pivot and axis from the edge it turns on or the way it runs. The leaf (one or two), its
//! rubber seal and whatever of the opening it leaves (the hull end under a ramp) are parts, the
//! joint goes to the kind with the compartments it opens between.
//!
//! In play (`ship.rs`): latches pull it the last stretch shut and hold it there until it is ordered
//! open; with no actuator on it a hand moves it (click on it). Every closure is closed at q = 0
//! and open at its upper limit; `<id>.abierta` says how open (0..1), `<id>.cerrada` whether it is
//! latched shut.
use crate::{
    def::{BulkheadDef, HullDef},
    geom::{self, GenPart, Role, fan, quad},
};
use glam::{Affine3A, Vec2, Vec3};
use lunar_core::mesh::{Material, Mesh, panel_code};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosureDef {
    pub id: String,
    #[serde(default)]
    pub nombre: Option<String>,
    pub abertura: ApertureDef,
    pub mueve: MoveDef,
    #[serde(default)]
    pub hoja: LeafDef,
    /// It starts open.
    #[serde(default)]
    pub abierta: bool,
    /// Compartments it opens between ("vacio" for space).
    #[serde(default)]
    pub entre: Option<[String; 2]>,
    /// Signal ordering it open (≥ 0.5) or shut; none: a hand works it (`<id>.mano`).
    #[serde(default)]
    pub orden: Option<String>,
    /// Latches pull it shut over the last stretch (`captura`) and hold it.
    #[serde(default = "yes")]
    pub pestillos: bool,
    /// Degrees (hinges) or metres (slides) from shut where the latches catch it.
    #[serde(default)]
    pub captura: Option<f32>,
    /// A rubber seal round the opening.
    #[serde(default = "yes")]
    pub junta: bool,
    #[serde(default)]
    pub amortiguamiento: Option<f32>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApertureDef {
    /// A bulkhead's doorway; the leaf sits on side `lado` (+1: toward +z, −1: toward −z).
    Mamparo { mamparo: String, lado: f32 },
    /// The hull's open end ("popa" or "proa") between two heights; what the leaf leaves of the
    /// end below `desde_y` is closed by a fixed plate.
    Casco {
        extremo: String,
        #[serde(default)]
        desde_y: Option<f32>,
        #[serde(default)]
        hasta_y: Option<f32>,
        /// What is under `desde_y` is closed by a fixed plate (the default); false: it is left
        /// to whatever else closes it (another leaf under this one: two doors on one end).
        #[serde(default = "yes")]
        fijo: bool,
    },
    /// A rectangle: its centre, the way out (normal), its up and its size (m).
    Rect {
        centro: [f32; 3],
        normal: [f32; 3],
        #[serde(default)]
        arriba: Option<[f32; 3]>,
        tamano: [f32; 2],
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum MoveDef {
    /// Hinged on an edge of the opening ("abajo", "arriba", "izq", "der"), opening outward this
    /// many degrees.
    Bisagra { borde: String, angulo: f32 },
    /// Sliding along `hacia` (ship frame); two leaves part from the middle.
    Corredera {
        hacia: [f32; 3],
        #[serde(default = "one_leaf")]
        hojas: u8,
        #[serde(default)]
        recorrido: Option<f32>,
    },
}

fn one_leaf() -> u8 {
    1
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeafDef {
    #[serde(default)]
    pub espesor: Option<f32>,
    #[serde(default)]
    pub material: Option<String>,
    /// Outside and inside colours (sRGB).
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub interior: Option<[u8; 3]>,
    /// Plate size of its outside (m).
    #[serde(default)]
    pub chapa: Option<f32>,
    /// Its inside is a floor (a ramp): tread.
    #[serde(default)]
    pub suelo: bool,
    /// Overlap over the frame round the opening (m).
    #[serde(default)]
    pub solape: Option<f32>,
    /// "fuera" (on the frame, overlapping) or "dentro" (flush in the opening).
    #[serde(default)]
    pub montaje: Option<String>,
    /// A stripe across the top of its outside (sRGB).
    #[serde(default)]
    pub franja: Option<[u8; 3]>,
}

/// A leaf as built: its joint and parts.
#[derive(Clone, Debug)]
pub struct Leaf {
    pub joint: String,
    pub hinge: bool,
    pub axis: Vec3,
    pub pivot: Vec3,
    /// Open position (rad or m): the joint runs 0 (shut) .. this.
    pub open: f32,
    pub parts: Vec<String>,
    /// Its share of the opening's area (m²).
    pub area: f32,
    /// The opening's centre and its outward normal (ship frame), and its half extents in its
    /// plane (across, up).
    pub at: Vec3,
    pub n: Vec3,
    pub span: [Vec3; 2],
}

pub struct Built {
    pub parts: Vec<GenPart>,
    pub leaves: Vec<Leaf>,
}

/// The opening: its outline (2D in its plane), the plane's frame (origin, u, v, outward n) and
/// how the leaf goes on it by default.
struct Opening {
    poly: Vec<Vec2>,
    origin: Vec3,
    u: Vec3,
    v: Vec3,
    n: Vec3,
    /// Thickness of the frame (the leaf sits past half of it when mounted outside).
    frame: f32,
    outside: bool,
    /// The bottom edge lies on a deck: no overlap below it.
    on_deck: bool,
    /// Fixed plates closing what the leaf does not (2D outlines in the same plane).
    rest: Vec<Vec<Vec2>>,
}

fn opening(d: &ClosureDef, hull: Option<&HullDef>, bulkheads: &[BulkheadDef]) -> Result<Opening, String> {
    match &d.abertura {
        ApertureDef::Mamparo { mamparo, lado } => {
            let b = bulkheads.iter().find(|b| b.id == *mamparo).ok_or_else(|| format!("cierre {}: no hay mamparo '{mamparo}'", d.id))?;
            let [x0, x1, top] = b.puerta.ok_or_else(|| format!("cierre {}: el mamparo '{mamparo}' no tiene puerta", d.id))?;
            let side = if *lado < 0.0 { -1.0 } else { 1.0 };
            let n = Vec3::new(0.0, 0.0, side);
            let v = Vec3::Y;
            let u = v.cross(n);
            let origin = Vec3::new(0.0, 0.0, b.z);
            // doorway corners in (u, v): u = side·x
            let (a, z) = ((x0 * side).min(x1 * side), (x0 * side).max(x1 * side));
            let poly = vec![Vec2::new(a, 0.0), Vec2::new(z, 0.0), Vec2::new(z, top), Vec2::new(a, top)];
            Ok(Opening { poly, origin, u, v, n, frame: b.espesor, outside: true, on_deck: true, rest: Vec::new() })
        }
        ApertureDef::Casco { extremo, desde_y, hasta_y, fijo } => {
            let h = hull.ok_or_else(|| format!("cierre {}: la nave no tiene casco", d.id))?;
            let st = &h.estaciones;
            let (z, side) = match extremo.as_str() {
                "popa" => (st[0].z, -1.0),
                "proa" => (st[st.len() - 1].z, 1.0),
                e => return Err(format!("cierre {}: extremo '{e}' (popa o proa)", d.id)),
            };
            let n = Vec3::new(0.0, 0.0, side);
            let v = Vec3::Y;
            let u = v.cross(n);
            let ring = geom::outline_at(h, z)?;
            // ring points (x, y) → (u, v) = (side·x... u = v × n: for n = −z, u = −x... careful)
            let to2 = |p: Vec2| Vec2::new(Vec3::new(p.x, p.y, 0.0).dot(u), p.y);
            let full: Vec<Vec2> = ring.iter().map(|p| to2(*p)).collect();
            let full = if ccw(&full) { full } else { full.into_iter().rev().collect() };
            let lo = desde_y.unwrap_or(f32::MIN);
            let hi = hasta_y.unwrap_or(f32::MAX);
            let poly = clip(&clip(&full, |p| p.y - lo), |p| hi - p.y);
            let mut rest = Vec::new();
            if lo > f32::MIN && *fijo {
                let below = clip(&full, |p| lo - p.y);
                if below.len() >= 3 {
                    rest.push(below);
                }
            }
            Ok(Opening { poly, origin: Vec3::new(0.0, 0.0, z), u, v, n, frame: h.espesor, outside: false, on_deck: desde_y.is_some(), rest })
        }
        ApertureDef::Rect { centro, normal, arriba, tamano } => {
            let n = Vec3::from_array(*normal).normalize_or(Vec3::Z);
            let up = Vec3::from_array(arriba.unwrap_or([0.0, 1.0, 0.0]));
            let mut v = (up - n * up.dot(n)).normalize_or(n.any_orthonormal_vector());
            if v.length_squared() < 0.5 {
                v = n.any_orthonormal_vector();
            }
            let u = v.cross(n);
            let [w, hh] = *tamano;
            let poly = vec![Vec2::new(-w * 0.5, -hh * 0.5), Vec2::new(w * 0.5, -hh * 0.5), Vec2::new(w * 0.5, hh * 0.5), Vec2::new(-w * 0.5, hh * 0.5)];
            Ok(Opening { poly, origin: Vec3::from_array(*centro), u, v, n, frame: 0.0, outside: true, on_deck: false, rest: Vec::new() })
        }
    }
}

fn ccw(o: &[Vec2]) -> bool {
    let mut a = 0.0;
    for i in 0..o.len() {
        let (p, q) = (o[i], o[(i + 1) % o.len()]);
        a += p.x * q.y - q.x * p.y;
    }
    a > 0.0
}

fn area(o: &[Vec2]) -> f32 {
    let mut a = 0.0;
    for i in 0..o.len() {
        let (p, q) = (o[i], o[(i + 1) % o.len()]);
        a += p.x * q.y - q.x * p.y;
    }
    a.abs() * 0.5
}

/// Clip a convex polygon to where `f(p) ≥ 0` (f linear).
fn clip(poly: &[Vec2], f: impl Fn(Vec2) -> f32) -> Vec<Vec2> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (fa, fb) = (f(a), f(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            out.push(a + (b - a) * (fa / (fa - fb)));
        }
    }
    out
}

/// A convex outline grown (`d` > 0) or shrunk outward, edge by edge (`keep_bottom`: the lowest
/// edge stays where it is).
fn grow(poly: &[Vec2], d: f32, keep_bottom: bool) -> Vec<Vec2> {
    let n = poly.len();
    let bottom = poly.iter().map(|p| p.y).fold(f32::MAX, f32::min);
    // each edge moved out along its normal, then neighbours intersected
    let lines: Vec<(Vec2, Vec2)> = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let e = (b - a).normalize_or(Vec2::X);
            let out = Vec2::new(e.y, -e.x);
            let flat = (a.y - bottom).abs() < 1e-4 && (b.y - bottom).abs() < 1e-4;
            let k = if keep_bottom && flat { 0.0 } else { d };
            (a + out * k, e)
        })
        .collect();
    (0..n)
        .map(|i| {
            let (p0, d0) = lines[(i + n - 1) % n];
            let (p1, d1) = lines[i];
            let den = d0.perp_dot(d1);
            if den.abs() < 1e-6 { p1 } else { p0 + d0 * ((p1 - p0).perp_dot(d1) / den) }
        })
        .collect()
}

fn mat(rgb: [u8; 3], rough: u8, metal: u8, panel: u8) -> Material {
    Material { albedo: rgb, rough, metal, emissive: 0, panel, finish: 0 }
}

/// A slab part: outline `poly` (2D in the frame), from `w0` to `w1` along n; faces coloured.
#[allow(clippy::too_many_arguments)]
fn slab(id: String, material: &str, role: Role, o: &Opening, poly: &[Vec2], w0: f32, w1: f32, outer: Material, inner: Material, edge: Material) -> Option<GenPart> {
    let at3 = |p: Vec2, w: f32| o.origin + o.u * p.x + o.v * p.y + o.n * w;
    let pts: Vec<Vec3> = poly.iter().flat_map(|p| [at3(*p, w0), at3(*p, w1)]).collect();
    let mut part = GenPart::from_points(id, material, role, &pts)?;
    let c = Vec3::from(part.at.translation);
    let far: Vec<Vec3> = poly.iter().map(|p| at3(*p, w0.max(w1)) - c).collect();
    let near: Vec<Vec3> = poly.iter().map(|p| at3(*p, w0.min(w1)) - c).collect();
    let mut m = Mesh::default();
    fan(&mut m, &far, o.n, outer);
    fan(&mut m, &near, -o.n, inner);
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        quad(&mut m, [near[i], near[j], far[j], far[i]], None, edge);
    }
    part.look = Some(m);
    Some(part)
}

/// A leaf from close by: a frame all round it, its panel set back inside the frame with ribs
/// across it flush with the frame — on both its faces, but a face that is a floor (a ramp's
/// inside), which stays flat to walk on. None for a leaf too small or too thin for that.
#[allow(clippy::too_many_arguments)]
fn leaf_look(o: &Opening, poly: &[Vec2], w0: f32, w1: f32, outer: Material, inner: Material, edge: Material, floor_inside: bool, centre: Vec3) -> Option<Mesh> {
    /// The frame's width, how far the panel is set back, how wide the slope down to it; a
    /// rib's width and how far apart ribs are (m).
    const FRAME: f32 = 0.07;
    const DEEP: f32 = 0.008;
    const SLOPE: f32 = 0.01;
    const RIB: f32 = 0.05;
    const RIB_EVERY: f32 = 0.55;
    let (near_w, far_w) = (w0.min(w1), w0.max(w1));
    let rim = grow(poly, -FRAME, false);
    let panel = grow(poly, -(FRAME + SLOPE), false);
    if far_w - near_w < 3.0 * DEEP || rim.len() != poly.len() || panel.len() != poly.len() || !ccw(&panel) || area(&panel) < 0.25 * area(poly) {
        return None;
    }
    let at3 = |p: Vec2, w: f32| o.origin + o.u * p.x + o.v * p.y + o.n * w - centre;
    let row = |q: &[Vec2], w: f32| -> Vec<Vec3> { q.iter().map(|p| at3(*p, w)).collect() };
    let n = poly.len();
    let mut m = Mesh::default();
    for (w, dir, material, flat) in [(far_w, 1.0, outer, false), (near_w, -1.0, inner, floor_inside)] {
        let facing = o.n * dir;
        if flat {
            fan(&mut m, &row(poly, w), facing, material);
            continue;
        }
        let back = w - dir * DEEP;
        for i in 0..n {
            let j = (i + 1) % n;
            geom::quad_facing(&mut m, [at3(poly[i], w), at3(poly[j], w), at3(rim[j], w), at3(rim[i], w)], facing, material);
            geom::quad_facing(&mut m, [at3(rim[i], w), at3(rim[j], w), at3(panel[j], back), at3(panel[i], back)], facing, material);
        }
        fan(&mut m, &row(&panel, back), facing, material);
        // ribs across the panel, level
        let (lo, hi) = panel.iter().fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p.y), h.max(p.y)));
        let ribs = ((hi - lo) / RIB_EVERY).floor() as usize;
        for k in 1..=ribs {
            let y = lo + (hi - lo) * k as f32 / (ribs + 1) as f32;
            let band = clip(&clip(&panel, |p| p.y - (y - RIB * 0.5)), |p| (y + RIB * 0.5) - p.y);
            if band.len() < 3 {
                continue;
            }
            fan(&mut m, &row(&band, w), facing, material);
            for i in 0..band.len() {
                let (a, b) = (band[i], band[(i + 1) % band.len()]);
                let e = (b - a).normalize_or(Vec2::X);
                // (only its long sides show: its ends are in the slope)
                if e.y.abs() < 0.5 {
                    geom::quad_facing(&mut m, [at3(a, w), at3(b, w), at3(b, back), at3(a, back)], o.u * e.y - o.v * e.x, material);
                }
            }
        }
    }
    let (near, far) = (row(poly, near_w), row(poly, far_w));
    for i in 0..n {
        let j = (i + 1) % n;
        quad(&mut m, [near[i], near[j], far[j], far[i]], None, edge);
    }
    Some(m)
}

/// Build a closure: its parts and its leaves' joints.
pub fn build(d: &ClosureDef, hull: Option<&HullDef>, bulkheads: &[BulkheadDef]) -> Result<Built, String> {
    let o = opening(d, hull, bulkheads)?;
    if o.poly.len() < 3 {
        return Err(format!("cierre {}: la abertura no tiene forma", d.id));
    }
    let h = &d.hoja;
    let t = h.espesor.unwrap_or(0.06);
    let material = h.material.clone().unwrap_or_else(|| "panel_mamparo".into());
    let outside = h.montaje.as_deref().map_or(o.outside, |m| m != "dentro");
    let lap = h.solape.unwrap_or(0.04);
    // the leaf's outline: over the frame round the opening, or a hair inside it
    let leaf_poly = if outside { grow(&o.poly, lap, o.on_deck) } else { grow(&o.poly, -0.006, false) };
    // across the plane: outside past the frame's face, else flush in the opening going in
    let (w0, w1) = if outside { (o.frame * 0.5 + 0.004, o.frame * 0.5 + 0.004 + t) } else { (-t, 0.0) };
    let color = h.color.unwrap_or([196, 196, 190]);
    let outer = mat(color, 110, 0, panel_code(h.chapa.unwrap_or(0.6), false)).with_finish(geom::fin("pintura"));
    let inner = if h.suelo { mat(h.interior.unwrap_or([120, 124, 126]), 205, 0, panel_code(0.6, false)).with_finish(geom::fin("lagrimado")) } else { mat(h.interior.unwrap_or(color), 140, 0, panel_code(0.6, false)).with_finish(geom::fin("pintura")) };
    let edge = mat([70, 72, 76], 160, 60, 0).with_finish(geom::fin("cepillado"));
    let total = area(&o.poly);
    let (plo, phi) = o.poly.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(l, h), p| (l.min(*p), h.max(*p)));
    let mid = (plo + phi) * 0.5;
    let o_at = o.origin + o.u * mid.x + o.v * mid.y;
    let extent = [o.u * (phi.x - plo.x) * 0.5, o.v * (phi.y - plo.y) * 0.5];
    let mut parts = Vec::new();
    let mut leaves = Vec::new();
    let stripe = |id: &str, poly: &[Vec2], parts: &mut Vec<GenPart>| {
        let Some(c) = h.franja else { return };
        let top = poly.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        let band = clip(&clip(poly, |p| p.y - (top - 0.26)), |p| (top - 0.06) - p.y);
        if band.len() >= 3
            && let Some(mut s) = slab(format!("{id}.franja"), "compuesto", Role::Component, &o, &band, w1, w1 + 0.004, mat(c, 150, 0, 0), mat(c, 150, 0, 0), mat(c, 150, 0, 0))
        {
            s.color = Some(c);
            parts.push(s);
        }
    };
    match &d.mueve {
        MoveDef::Bisagra { borde, angulo } => {
            let id = format!("{}.hoja", d.id);
            let mut leaf = slab(id.clone(), &material, Role::Component, &o, &leaf_poly, w0, w1, outer, inner, edge).ok_or_else(|| format!("cierre {}: hoja sin forma", d.id))?;
            leaf.color = Some(color);
            leaf.fine = leaf_look(&o, &leaf_poly, w0, w1, outer, inner, edge, h.suelo, Vec3::from(leaf.at.translation));
            if let Some([a, b]) = &d.entre {
                leaf.bounds = vec![a.clone(), b.clone()];
            }
            // the hinge: the chosen edge of the leaf, at the middle of its thickness
            let pick = |f: &dyn Fn(Vec2) -> f32| -> (Vec2, Vec2) {
                let n = leaf_poly.len();
                (0..n).map(|i| (leaf_poly[i], leaf_poly[(i + 1) % n])).min_by(|a, b| (f(a.0) + f(a.1)).total_cmp(&(f(b.0) + f(b.1)))).unwrap_or((leaf_poly[0], leaf_poly[1]))
            };
            let (a, b) = match borde.as_str() {
                "abajo" => pick(&|p| p.y),
                "arriba" => pick(&|p| -p.y),
                "izq" => pick(&|p| p.x),
                "der" => pick(&|p| -p.x),
                e => {
                    return Err(format!("cierre {}: borde '{e}' (abajo, arriba, izq, der)", d.id));
                }
            };
            let wm = (w0 + w1) * 0.5;
            let at3 = |p: Vec2| o.origin + o.u * p.x + o.v * p.y + o.n * wm;
            let (pa, pb) = (at3(a), at3(b));
            let mut axis = (pb - pa).normalize_or(o.u);
            // opening turns the far side of the leaf outward
            let centre = leaf_poly.iter().copied().sum::<Vec2>() / leaf_poly.len() as f32;
            let arm = at3(centre) - pa;
            if axis.cross(arm).dot(o.n) < 0.0 {
                axis = -axis;
            }
            stripe(&d.id, &leaf_poly, &mut parts);
            let mut ids = vec![id];
            if h.franja.is_some() {
                ids.push(format!("{}.franja", d.id));
            }
            parts.push(leaf);
            leaves.push(Leaf { joint: d.id.clone(), hinge: true, axis, pivot: pa, open: angulo.to_radians(), parts: ids, area: total, at: o_at, n: o.n, span: extent });
        }
        MoveDef::Corredera { hacia, hojas, recorrido } => {
            let dir3 = Vec3::from_array(*hacia);
            let dir = Vec2::new(dir3.dot(o.u), dir3.dot(o.v)).normalize_or(Vec2::X);
            let along = |p: Vec2| p.dot(dir);
            let (lo, hi) = leaf_poly.iter().fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(along(*p)), h.max(along(*p))));
            let pieces: Vec<(String, String, Vec<Vec2>, f32)> = if *hojas >= 2 {
                let mid = (lo + hi) * 0.5;
                vec![(d.id.clone(), format!("{}.hoja", d.id), clip(&leaf_poly, |p| along(p) - mid), 1.0), (format!("{}_b", d.id), format!("{}.hoja_b", d.id), clip(&leaf_poly, |p| mid - along(p)), -1.0)]
            } else {
                vec![(d.id.clone(), format!("{}.hoja", d.id), leaf_poly.clone(), 1.0)]
            };
            let share = total / pieces.len() as f32;
            for (joint, id, poly, sign) in pieces {
                let span = poly.iter().fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(along(*p)), h.max(along(*p))));
                let travel = recorrido.unwrap_or(span.1 - span.0 + 0.02);
                let mut leaf = slab(id.clone(), &material, Role::Component, &o, &poly, w0, w1, outer, inner, edge).ok_or_else(|| format!("cierre {}: hoja sin forma", d.id))?;
                leaf.color = Some(color);
                leaf.fine = leaf_look(&o, &poly, w0, w1, outer, inner, edge, h.suelo, Vec3::from(leaf.at.translation));
                if let Some([a, b]) = &d.entre {
                    leaf.bounds = vec![a.clone(), b.clone()];
                }
                let axis = (o.u * dir.x + o.v * dir.y) * sign;
                parts.push(leaf);
                leaves.push(Leaf { joint, hinge: false, axis, pivot: Vec3::ZERO, open: travel, parts: vec![id], area: share, at: o_at, n: o.n, span: extent });
            }
        }
    }
    // the seal round the opening, on the frame where the leaf meets it
    if d.junta {
        let w = if outside { o.frame * 0.5 + 0.002 } else { -0.004 };
        let n = o.poly.len();
        let rubber = mat([24, 24, 26], 235, 0, 0).with_finish(geom::fin("goma"));
        for i in 0..n {
            let (a, b) = (o.poly[i], o.poly[(i + 1) % n]);
            // no seal along a deck (the threshold is the deck itself)
            if o.on_deck && (a.y - b.y).abs() < 1e-4 && a.y.min(b.y) <= o.poly.iter().map(|p| p.y).fold(f32::MAX, f32::min) + 1e-4 {
                continue;
            }
            let e = (b - a).normalize_or(Vec2::X);
            let out = Vec2::new(e.y, -e.x);
            let strip = [a, b, b + out * 0.03, a + out * 0.03];
            if let Some(mut s) = slab(format!("{}.junta.{i}", d.id), "goma", Role::Component, &o, &strip, w, w + if outside { 0.012 } else { -0.012 }, rubber, rubber, rubber) {
                s.color = Some([24, 24, 26]);
                parts.push(s);
            }
        }
    }
    // what the leaf leaves of the opening (the hull end under a ramp): fixed plates
    for (k, r) in o.rest.iter().enumerate() {
        let skin = mat(color, 110, 0, panel_code(0.6, false));
        if let Some(mut p) = slab(format!("{}.fijo{k}", d.id), &material, Role::Hull, &o, r, -t, 0.0, skin, skin, edge) {
            p.color = Some(color);
            parts.push(p);
        }
    }
    let _ = Affine3A::IDENTITY;
    Ok(Built { parts, leaves })
}
