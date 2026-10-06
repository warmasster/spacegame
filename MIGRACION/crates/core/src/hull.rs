//! Procedural hulls (ships, stations, props): a list of primitive parts declared in data, built at
//! any LOD. The finest LODs are filled with surface detail ("greebles") up to a triangle budget.
//! A new ship is one more definition file; nothing here changes.
use crate::{
    mesh::{Glow, Material, Mesh, cone, cuboid, sphere, wedge},
    model::{BuiltModel, ShapeBuilder},
    noise::Random,
    palette::Palette,
};
use glam::{Affine3A, EulerRot, Quat, Vec3};
use serde::Deserialize;

#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Box(Vec3),
    /// Radius, length (along the part's +Y).
    Cyl(f32, f32),
    Cone(f32, f32, f32),
    Wedge(Vec3),
    Dome(f32),
    Ball(f32),
}

#[derive(Clone, Copy, Debug)]
pub struct Part {
    pub shape: Shape,
    pub at: Affine3A,
    pub mat: Material,
    /// Included for LODs up to this one.
    pub lod_max: u8,
    /// Surface to scatter detail on.
    pub hull: bool,
}

// ---- data ----

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HullLod {
    /// Smallest on-screen radius (px) this LOD is drawn at.
    pub min_px: f32,
    /// Triangle budget the greebles fill up to.
    pub budget: usize,
    /// Segment count factor of round shapes, and its floor.
    pub segments: f32,
    pub min_segments: u32,
    /// Largest greeble (m); 0 = no greebles.
    #[serde(default)]
    pub greeble: f32,
    /// Chamfered boxes.
    #[serde(default)]
    pub bevel: bool,
    /// Some greebles are pipes.
    #[serde(default)]
    pub pipes: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Greebles {
    pub dark: String,
    pub metal: String,
    /// A roll under `dark_below` is dark, under `metal_below` metal, else the part's material.
    pub dark_below: f64,
    pub metal_below: f64,
    pub pipe_chance: f64,
    pub attempts: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlowLook {
    /// Sprite size per part size.
    pub scale: f32,
    pub intensity: f32,
}

/// Which parts become far-visible glow sprites and how they look.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlowRule {
    pub min_emissive: u8,
    /// Parts bigger than this (m) are engines, the rest beacons.
    pub engine_size: f32,
    pub engine: GlowLook,
    pub beacon: GlowLook,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartDef {
    #[serde(rename = "box", default)]
    cuboid: Option<[f32; 3]>,
    #[serde(default)]
    cyl: Option<[f32; 2]>,
    #[serde(default)]
    cone: Option<[f32; 3]>,
    #[serde(default)]
    wedge: Option<[f32; 3]>,
    #[serde(default)]
    dome: Option<f32>,
    #[serde(default)]
    ball: Option<f32>,
    at: [f32; 3],
    /// Euler XYZ (degrees).
    #[serde(default)]
    rot: [f32; 3],
    mat: String,
    /// Material of the mirrored copy (inside a `mirror` group).
    #[serde(default)]
    mirror_mat: Option<String>,
    #[serde(default)]
    hull: bool,
    #[serde(default)]
    lod_max: Option<u8>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum PartEntry {
    Part(PartDef),
    /// Parts written for +X; the group is emitted mirrored (x → -x) first, then as written.
    Mirror {
        mirror: Vec<PartDef>,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HullDef {
    pub seed: u32,
    pub lods: Vec<HullLod>,
    pub greebles: Greebles,
    pub glows: GlowRule,
    pub parts: Vec<PartEntry>,
}

impl HullDef {
    /// Resolve materials and mirrors into a buildable hull.
    pub fn resolve(&self, palette: &Palette) -> Result<Hull, String> {
        let mut parts = Vec::new();
        for e in &self.parts {
            match e {
                PartEntry::Part(p) => parts.push(part(p, palette, false)?),
                PartEntry::Mirror { mirror } => {
                    for flip in [true, false] {
                        for p in mirror {
                            parts.push(part(p, palette, flip)?);
                        }
                    }
                }
            }
        }
        if self.lods.is_empty() {
            return Err("a hull needs at least one LOD".into());
        }
        Ok(Hull { seed: self.seed, lods: self.lods.clone(), dark: palette.get(&self.greebles.dark)?, metal: palette.get(&self.greebles.metal)?, greebles: self.greebles.clone(), glows: self.glows, parts })
    }
}

fn part(d: &PartDef, palette: &Palette, flip: bool) -> Result<Part, String> {
    let v = Vec3::from_array;
    let shapes = [d.cuboid.map(|s| Shape::Box(v(s))), d.cyl.map(|[r, l]| Shape::Cyl(r, l)), d.cone.map(|[a, b, l]| Shape::Cone(a, b, l)), d.wedge.map(|s| Shape::Wedge(v(s))), d.dome.map(Shape::Dome), d.ball.map(Shape::Ball)];
    let mut found = shapes.iter().flatten();
    let shape = *found.next().ok_or("a part needs one shape: box, cyl, cone, wedge, dome or ball")?;
    if found.next().is_some() {
        return Err("a part has more than one shape".into());
    }
    let [x, y, z] = d.rot.map(f32::to_radians);
    let mut rot = Quat::from_euler(EulerRot::XYZ, x, y, z);
    let mut pos = v(d.at);
    let mut mat = &d.mat;
    if flip {
        // reflection across the YZ plane
        pos.x = -pos.x;
        rot = Quat::from_xyzw(rot.x, -rot.y, -rot.z, rot.w);
        mat = d.mirror_mat.as_ref().unwrap_or(&d.mat);
    }
    Ok(Part { shape, at: Affine3A::from_rotation_translation(rot, pos), mat: palette.get(mat)?, lod_max: d.lod_max.unwrap_or(u8::MAX), hull: d.hull })
}

// ---- building ----

pub struct Hull {
    pub seed: u32,
    pub lods: Vec<HullLod>,
    pub parts: Vec<Part>,
    greebles: Greebles,
    dark: Material,
    metal: Material,
    glows: GlowRule,
}

/// Segments of a full turn for round shapes at LOD factor 1.
const ROUND_SEGMENTS: [f32; 3] = [40.0, 28.0, 24.0];

impl Hull {
    /// Every emissive part as a far-visible glow.
    pub fn glows(&self) -> Vec<Glow> {
        let rule = &self.glows;
        self.parts
            .iter()
            .filter(|p| p.mat.emissive >= rule.min_emissive)
            .map(|p| {
                let size = match p.shape {
                    Shape::Cyl(r, _) | Shape::Cone(r, _, _) => r,
                    Shape::Box(s) | Shape::Wedge(s) => s.max_element() * 0.5,
                    Shape::Dome(r) | Shape::Ball(r) => r,
                };
                let c = p.mat.albedo.map(|v| (f32::from(v) / 255.0).powf(2.2));
                let look = if size > rule.engine_size { rule.engine } else { rule.beacon };
                Glow { pos: p.at.translation.to_array(), size: size * look.scale, color: c, intensity: look.intensity }
            })
            .collect()
    }

    fn shape_mesh(&self, shape: Shape, l: &HullLod, m: Material) -> Mesh {
        let seg = |base: f32, r: f32| ((base * l.segments * (0.6 + r.sqrt() * 0.4)) as u32).max(l.min_segments);
        let [cyl, dome, ball] = ROUND_SEGMENTS;
        match shape {
            Shape::Box(s) => cuboid(s, if l.bevel { s.min_element().min(0.4) * 0.12 } else { 0.0 }, m),
            Shape::Cyl(r, len) => cone(r, r, len, seg(cyl, r), true, m),
            Shape::Cone(r0, r1, len) => cone(r0, r1, len, seg(cyl, r0.max(r1)), true, m),
            Shape::Wedge(s) => wedge(s, m),
            Shape::Dome(r) => sphere(r, seg(dome, r), true, m),
            Shape::Ball(r) => sphere(r, seg(ball, r), false, m),
        }
    }

    /// One LOD (0 finest).
    pub fn build(&self, lod: usize) -> Mesh {
        let l = &self.lods[lod];
        let mut mesh = Mesh::default();
        for part in self.parts.iter().filter(|p| lod <= usize::from(p.lod_max)) {
            mesh.append(&self.shape_mesh(part.shape, l, part.mat), part.at);
        }
        if l.greeble <= 0.0 {
            return mesh;
        }
        // surface detail until the budget: panels, boxes and pipes on the hull parts
        let g = &self.greebles;
        let hulls: Vec<&Part> = self.parts.iter().filter(|p| p.hull).collect();
        let mut rng = Random(self.seed.wrapping_add(lod as u32));
        let big = l.greeble;
        let mut guard = 0;
        while mesh.tris() < l.budget && guard < g.attempts && !hulls.is_empty() {
            guard += 1;
            let part = hulls[(rng.next_f64() * hulls.len() as f64) as usize % hulls.len()];
            let (at, n) = surface_point(part, &mut rng);
            let size = Vec3::new(0.15 + rng.next_f64() as f32 * big, 0.02 + rng.next_f64() as f32 * big * 0.1, 0.15 + rng.next_f64() as f32 * big);
            let spin = (rng.next_f64() * 4.0).floor() as f32 * std::f32::consts::FRAC_PI_2;
            let rot = Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(spin);
            let shade = rng.next_f64();
            let m = if shade < g.dark_below {
                self.dark
            } else if shade < g.metal_below {
                self.metal
            } else {
                part.mat
            };
            let pipe = l.pipes && rng.next_f64() < g.pipe_chance;
            let piece = if pipe { cone(size.x * 0.25, size.x * 0.25, size.z * 1.5, 10, true, m) } else { cuboid(size, if l.bevel { size.min_element() * 0.3 } else { 0.0 }, m) };
            let lift = if pipe { size.x * 0.25 } else { size.y * 0.5 };
            let mut t = Affine3A::from_rotation_translation(rot, at + n * lift);
            if pipe {
                t = t * Affine3A::from_rotation_z(std::f32::consts::FRAC_PI_2);
            }
            mesh.append(&piece, t);
        }
        mesh
    }
}

impl ShapeBuilder for Hull {
    fn build(&self) -> Result<BuiltModel, String> {
        Ok(BuiltModel { lods: (0..self.lods.len()).map(|l| (Hull::build(self, l), self.lods[l].min_px)).collect(), skinned: false, glows: self.glows(), animation: None })
    }
}

/// A random point on a part's surface: position, outward normal (part space → hull space).
fn surface_point(part: &Part, rng: &mut Random) -> (Vec3, Vec3) {
    let mut f = || rng.next_f64() as f32;
    let (lp, ln) = match part.shape {
        Shape::Box(s) => {
            // any face but the bottom, weighted by area
            let faces = [(Vec3::X, s.y * s.z), (Vec3::NEG_X, s.y * s.z), (Vec3::Y, s.x * s.z), (Vec3::Z, s.x * s.y), (Vec3::NEG_Z, s.x * s.y)];
            let total: f32 = faces.iter().map(|f| f.1).sum();
            let mut pick = f() * total;
            let mut n = Vec3::Y;
            for (fnrm, a) in faces {
                n = fnrm;
                if pick < a {
                    break;
                }
                pick -= a;
            }
            let (u, v) = (f() - 0.5, f() - 0.5);
            let h = s * 0.5;
            let p = if n.x != 0. {
                Vec3::new(h.x * n.x, u * s.y * 0.9, v * s.z * 0.9)
            } else if n.y != 0. {
                Vec3::new(u * s.x * 0.9, h.y, v * s.z * 0.9)
            } else {
                Vec3::new(u * s.x * 0.9, v * s.y * 0.9, h.z * n.z)
            };
            (p, n)
        }
        Shape::Cyl(r, l) | Shape::Cone(r, _, l) => {
            let a = f() * std::f32::consts::TAU;
            let n = Vec3::new(a.cos(), 0., a.sin());
            (n * r + Vec3::Y * (f() - 0.5) * l * 0.9, n)
        }
        _ => (Vec3::ZERO, Vec3::Y),
    };
    (part.at.transform_point3(lp), part.at.transform_vector3(ln).normalize())
}

#[cfg(test)]
mod tests {
    use crate::{model::ModelDef, palette::Palette};
    #[test]
    fn budgets_and_sizes() {
        let palette = Palette::parse("palette", include_str!("../../../assets/defs/palette.jsonc")).unwrap();
        for (name, text) in [
            ("nave_pequena", include_str!("../../../assets/defs/models/nave_pequena.jsonc")),
            ("nave_mediana", include_str!("../../../assets/defs/models/nave_mediana.jsonc")),
            ("nave_grande", include_str!("../../../assets/defs/models/nave_grande.jsonc")),
        ] {
            let def: ModelDef = crate::defs::parse(name, text).unwrap();
            let crate::model::ModelSource::Hull(h) = &def.source else { panic!("{name} is not a hull") };
            let hull = h.resolve(&palette).unwrap();
            let lods: Vec<_> = (0..hull.lods.len()).map(|l| hull.build(l)).collect();
            let tris: Vec<_> = lods.iter().map(|m| m.tris()).collect();
            let (_, r) = lods[0].bounds();
            println!("{name}: tris {tris:?}, radius {r:.1} m, glows {}", hull.glows().len());
            // the fine detail is the shader's plating: triangles only for shape and big relief
            assert!(tris[0] > 6_000 && tris[0] < 12_000);
            assert!(tris[1] > 1_200 && tris[1] < 2_800);
            assert!(tris[2] > 100 && tris[2] < 900);
            assert!(!hull.glows().is_empty());
        }
    }
}
