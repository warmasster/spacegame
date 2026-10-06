//! The component catalog (`assets/defs/components/*.jsonc`): reusable things a ship is fitted
//! with (seats, consoles, engines, tanks, pumps, lamps, vents, lockers...), each one or more convex
//! pieces with their materials and, if it works, a default machine and light. A ship places them
//! by id; a component with several pieces becomes several parts (`<id>.<piece>`), each breaking
//! on its own.
use crate::{
    def::{ComponentDef, LightDef, MachineDef},
    geom::{GenPart, Role, quad},
};
use glam::{Affine3A, EulerRot, Quat, Vec3};
use lunar_core::{
    mesh::{Material, Mesh, panel_code},
    structure::catalog::ShapeDef,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceDef {
    #[serde(default)]
    pub id: String,
    pub forma: ShapeDef,
    #[serde(default)]
    pub en: [f32; 3],
    #[serde(default)]
    pub rot: [f32; 3],
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub brillo: Option<u8>,
    #[serde(default)]
    pub chapa: Option<f32>,
    #[serde(default)]
    pub hueco: Option<f32>,
    /// Its finish (texture), else the component's, else its material's.
    #[serde(default)]
    pub acabado: Option<String>,
    /// A look of the catalog's models (by name), else its shape smooth- or flat-shaded.
    #[serde(default)]
    pub modelo: Option<String>,
    /// Round shapes shaded smooth.
    #[serde(default)]
    pub liso: bool,
    /// What it lets go of when it is destroyed (a drum of propellant, a bottle under pressure).
    #[serde(default)]
    pub estalla: Option<BurstPiece>,
}

/// A clamp's zone (`cargo`): the box over it (component frame: its centre and half extents, m)
/// where what is loose is taken when it shuts; the heaviest thing it takes (kg), all it holds
/// at once (kg: its rated load; no limit without it) and how many things (six by default; a
/// magnet, everything under it); and how it takes it — "asentar" (set down on the floor of the
/// zone as it lies: a deck clamp), "cuna" (set down in the middle of the zone, squared to it: a
/// cradle for a small ship) or "iman" (as it is, where it is: a magnet).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClampDef {
    pub centro: [f32; 3],
    pub zona: [f32; 3],
    pub masa: f32,
    #[serde(default)]
    pub carga: Option<f32>,
    #[serde(default)]
    pub cuantos: Option<u32>,
    #[serde(default)]
    pub modo: Option<String>,
}

/// A blast of `energia` J reaching `radio` m, shown as the explosion `efecto`
/// (`assets/defs/explosions`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BurstPiece {
    pub energia: f32,
    pub radio: f32,
    pub efecto: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentKind {
    pub nombre: String,
    pub piezas: Vec<PieceDef>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub maquina: Option<MachineDef>,
    #[serde(default)]
    pub luz: Option<LightDef>,
    /// Named points (component frame): "puerto" is where stub conduits plug in.
    #[serde(default)]
    pub anclajes: BTreeMap<String, [f32; 3]>,
    #[serde(default)]
    pub fabricante: Option<String>,
    /// Its decals (component frame): every one fitted carries them.
    #[serde(default)]
    pub calcas: Vec<crate::def::DecalDef>,
    /// Its labels (component frame): every one fitted carries them.
    #[serde(default)]
    pub rotulos: Vec<crate::def::LabelDef>,
    /// It is a clamp: it holds cargo, and takes what is loose in its zone when it shuts.
    #[serde(default)]
    pub anclaje: Option<ClampDef>,
    /// What it holds, as a scanner reads it. With `contenido` it is how that is told
    /// ("{cantidad} de {sustancia} en sacos": `lunar_core::structure::contents::text`; without
    /// it, "170 L de agua"); alone, the words themselves (something that is not counted).
    #[serde(default)]
    pub recurso: Option<String>,
    /// It is a container: what it holds, how much at most, how full as built (`contents`).
    #[serde(default)]
    pub contenido: Option<crate::contents::ContentsDef>,
}

pub type Components = BTreeMap<String, ComponentKind>;

pub fn euler(r: [f32; 3]) -> Quat {
    Quat::from_euler(EulerRot::XYZ, r[0].to_radians(), r[1].to_radians(), r[2].to_radians())
}

/// Mirror a name across the centreline: izq ↔ der.
pub fn mirror_name(s: &str) -> String {
    if s.contains("izq") {
        s.replace("izq", "der")
    } else if s.contains("der") {
        s.replace("der", "izq")
    } else {
        s.to_string()
    }
}

/// A placement mirrored across x = 0 (for shapes symmetric in their own x).
pub fn mirror(at: Affine3A) -> Affine3A {
    let (_, r, t) = at.to_scale_rotation_translation();
    let (rx, ry, rz) = r.to_euler(EulerRot::XYZ);
    Affine3A::from_rotation_translation(Quat::from_euler(EulerRot::XYZ, rx, -ry, -rz), Vec3::new(-t.x, t.y, t.z))
}

/// The look of a plain shape: smooth (round things) or flat, in one material, maybe plated.
pub fn shape_look(shape: &lunar_core::structure::convex::Convex, m: Material, smooth: bool) -> Mesh {
    let mut mesh = Mesh::default();
    let (c, _) = shape.sphere();
    for f in &shape.faces {
        let n = f.plane.n;
        // the sides of a prism (normals across the axis) round off; caps stay flat
        let round = smooth && n.y.abs() < 0.5;
        let base = mesh.pos.len() as u32;
        for v in &f.verts {
            let nrm = if round { Vec3::new(v.x - c.x, 0.0, v.z - c.z).normalize_or(n) } else { n };
            mesh.vertex(*v, nrm, m);
        }
        for k in 1..f.verts.len() as u32 - 1 {
            mesh.tri(base, base + k, base + k + 1);
        }
    }
    let _ = quad;
    mesh
}

/// A plain shape as it is seen from close by: a box with its edges broken, a round thing with
/// twice the sides, its rims broken and its sides smooth. (A wedge or a hull of points stays as
/// it is.) A real thing has no razor edges; one without them stops reading as a primitive.
pub fn fine_look(shape: &ShapeDef, m: Material) -> Option<Mesh> {
    // how much of an edge is taken: a share of the smallest side, between 3 mm and 12
    let chamfer = |least: f32| (least * 0.07).clamp(0.003, 0.012).min(least * 0.3);
    match *shape {
        ShapeDef::Box { size } => {
            let size = Vec3::from_array(size);
            Some(lunar_core::mesh::cuboid(size, chamfer(size.min_element()), m))
        }
        ShapeDef::Cylinder { radius, height, sides, taper } => {
            let (r0, r1) = (radius, (radius * taper).max(0.01));
            let c = chamfer(r0.min(r1).min(height * 0.5));
            let seg = (sides * 2).clamp(16, 32);
            let (y0, y1) = (-height * 0.5, height * 0.5);
            let mut o = Mesh::default();
            // bands from the bottom up: rim, side, rim; each smooth round the axis
            let rings = [(y0, r0 - c), (y0 + c, r0), (y1 - c, r1), (y1, r1 - c)];
            for w in rings.windows(2) {
                let ((ya, ra), (yb, rb)) = (w[0], w[1]);
                let slope = (ra - rb) / (yb - ya).max(1e-6);
                let base = o.pos.len() as u32;
                for s in 0..=seg {
                    let (sn, cs) = (s as f32 / seg as f32 * std::f32::consts::TAU).sin_cos();
                    let n = Vec3::new(cs, slope, sn);
                    o.vertex(Vec3::new(cs * ra, ya, sn * ra), n, m);
                    o.vertex(Vec3::new(cs * rb, yb, sn * rb), n, m);
                }
                for s in 0..seg {
                    let i = base + s * 2;
                    o.quad(i, i + 1, i + 3, i + 2);
                }
            }
            for (y, r, n) in [(y0, r0 - c, -Vec3::Y), (y1, r1 - c, Vec3::Y)] {
                let centre = o.vertex(Vec3::new(0.0, y, 0.0), n, m);
                let first = o.pos.len() as u32;
                for s in 0..seg {
                    let (sn, cs) = (s as f32 / seg as f32 * std::f32::consts::TAU).sin_cos();
                    o.vertex(Vec3::new(cs * r, y, sn * r), n, m);
                }
                for s in 0..seg {
                    let (a, b) = (first + s, first + (s + 1) % seg);
                    if n.y > 0.0 { o.tri(centre, b, a) } else { o.tri(centre, a, b) }
                }
            }
            Some(o)
        }
        _ => None,
    }
}

/// The model of piece `piece` of component kind `kind` (`lunar_core::structure::models`): the
/// mesh of that name in the kind's file, `cuerpo` for a piece without a name.
pub fn model_name(kind: &str, piece: &str) -> String {
    format!("{kind}/{}", if piece.is_empty() { lunar_core::structure::catalog::BODY } else { piece })
}

/// A shape's size as a name, to the millimetre: `b300x660x700` a box, `c85x1000` a cylinder
/// (radius, height; `t500` after it its taper in thousandths), `w100x1200x1600` a wedge. None
/// for a hull of points.
pub fn shape_key(shape: &ShapeDef) -> Option<String> {
    let mm = |v: f32| (v * 1000.0).round() as i64;
    match *shape {
        ShapeDef::Box { size } => Some(format!("b{}x{}x{}", mm(size[0]), mm(size[1]), mm(size[2]))),
        ShapeDef::Cylinder { radius, height, taper, .. } => Some(if (taper - 1.0).abs() < 5e-4 { format!("c{}x{}", mm(radius), mm(height)) } else { format!("c{}x{}t{}", mm(radius), mm(height), mm(taper)) }),
        ShapeDef::Wedge { size } => Some(format!("w{}x{}x{}", mm(size[0]), mm(size[1]), mm(size[2]))),
        // a hull of points: by its points to the millimetre, in their order (FNV-1a over them as
        // little-endian i32): `h` and eight hex digits
        ShapeDef::Hull { ref points } => {
            let mut h: u32 = 0x811c_9dc5;
            for p in points {
                for v in p {
                    for b in (mm(*v) as i32).to_le_bytes() {
                        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
                    }
                }
            }
            Some(format!("h{h:08x}"))
        }
    }
}

/// The model of a plain shape seen as style `style`: the mesh named after the shape's size in
/// the style's file (`tools/modelos` writes one for every size the ships use).
pub fn style_model(style: &str, shape: &ShapeDef) -> Option<String> {
    Some(format!("estilo_{style}/{}", shape_key(shape)?))
}

/// Instance of a component (or plain shape) as parts, placed by `at` (ship frame); `side`: it is
/// the mirrored copy across the centreline. Returns the parts and the anchors (ship frame) by
/// name.
pub fn place(c: &ComponentDef, id: &str, at: Affine3A, side: bool, kinds: &Components, materials: &dyn Fn(&str) -> Option<([u8; 3], u8, u8, u8)>) -> Result<(Vec<GenPart>, BTreeMap<String, Vec3>), String> {
    let mut out = Vec::new();
    let mut anchors = BTreeMap::new();
    #[allow(clippy::too_many_arguments)]
    let piece = |pid: String, shape_def: &ShapeDef, local: Affine3A, material: &str, color: Option<[u8; 3]>, glow: u8, plate: Option<f32>, smooth: bool, hollow: Option<f32>, finish: Option<&str>| -> Result<GenPart, String> {
        let shape = shape_def.convex();
        let (albedo, rough, metal, mfinish) = materials(material).ok_or_else(|| format!("{pid}: material desconocido '{material}'"))?;
        let finish = match finish {
            Some(f) => lunar_core::mesh::finish(f).ok_or_else(|| format!("{pid}: acabado desconocido '{f}'"))?,
            None => mfinish,
        };
        let m = Material { albedo: color.unwrap_or(albedo), rough, metal, emissive: glow, panel: plate.map_or(0, |p| panel_code(p, false)), finish };
        let look = shape_look(&shape, m, smooth);
        Ok(GenPart {
            id: pid,
            material: material.to_string(),
            color,
            glow,
            hollow,
            shape,
            look: Some(look),
            at: local,
            role: Role::Component,
            bounds: Vec::new(),
            model: None,
            flags: 0,
            burst: None,
            surface: Some(m),
            fine: fine_look(shape_def, m),
            mirrored: false,
        })
    };
    if let Some(kind_id) = &c.tipo {
        let k = kinds.get(kind_id).ok_or_else(|| format!("componente {id}: tipo desconocido '{kind_id}'"))?;
        let single = k.piezas.len() == 1;
        for p in &k.piezas {
            let pid = if single || p.id.is_empty() { id.to_string() } else { format!("{id}.{}", p.id) };
            let local = at * Affine3A::from_rotation_translation(euler(p.rot), Vec3::from_array(p.en));
            let material = p.material.as_deref().or(c.material.as_deref()).or(k.material.as_deref()).unwrap_or("aluminio");
            let color = c.color.filter(|_| p.color.is_none()).or(p.color);
            let finish = p.acabado.as_deref().or(c.acabado.as_deref());
            let mut part = piece(pid, &p.forma, local, material, color, p.brillo.or(c.brillo).unwrap_or(0), p.chapa.or(c.chapa), p.liso, p.hueco.or(c.hueco), finish)?;
            // a detailed model replaces the plain look from close by (resolved by the kind builder):
            // the one its data names, else the one of its kind's file if there is such a file
            part.model = Some(p.modelo.clone().unwrap_or_else(|| model_name(kind_id, &p.id)));
            part.burst = p.estalla.as_ref().map(|b| lunar_core::structure::catalog::BurstDef { energy: b.energia, radius: b.radio, effect: b.efecto.clone() });
            out.push(part);
        }
        for (name, p) in &k.anclajes {
            anchors.insert(name.clone(), at.transform_point3(Vec3::from_array(*p)));
        }
    } else if let Some(f) = &c.forma {
        let shape: ShapeDef = serde_json::from_value(f.clone()).map_err(|e| format!("componente {id}: forma: {e}"))?;
        let material = c.material.as_deref().unwrap_or("aluminio");
        let smooth = matches!(shape, ShapeDef::Cylinder { .. });
        let mut part = piece(id.to_string(), &shape, at, material, c.color, c.brillo.unwrap_or(0), c.chapa, smooth, c.hueco, c.acabado.as_deref())?;
        // its style's model for a shape this size; the copy across the centreline wears it
        // mirrored (`mirror` turns the piece, it does not reflect it)
        part.model = c.estilo.as_deref().and_then(|s| style_model(s, &shape));
        part.mirrored = side;
        out.push(part);
    } else {
        return Err(format!("componente {id}: falta 'tipo' o 'forma'"));
    }
    anchors.entry("puerto".into()).or_insert_with(|| at.translation.into());
    Ok((out, anchors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_is_named_by_its_size_to_the_millimetre() {
        // (the names `tools/modelos/kit.py` gives the same shapes: `clave_forma`)
        assert_eq!(shape_key(&ShapeDef::Box { size: [0.3, 0.66, 0.7] }).as_deref(), Some("b300x660x700"));
        assert_eq!(shape_key(&ShapeDef::Cylinder { radius: 0.085, height: 1.0, sides: 10, taper: 1.0 }).as_deref(), Some("c85x1000"));
        assert_eq!(shape_key(&ShapeDef::Cylinder { radius: 0.055, height: 0.13, sides: 10, taper: 1.6 }).as_deref(), Some("c55x130t1600"));
        assert_eq!(shape_key(&ShapeDef::Wedge { size: [0.1, 1.2, 1.6] }).as_deref(), Some("w100x1200x1600"));
        assert_eq!(shape_key(&ShapeDef::Hull { points: vec![[0.0; 3]] }).as_deref(), Some("he23c62b5"));
        assert_eq!(shape_key(&ShapeDef::Hull { points: vec![[1.15, 0.11, 2.2], [-0.5, 0.0, -3.6]] }).as_deref(), Some("hb2c955bc"));
        assert_eq!(style_model("ala", &ShapeDef::Box { size: [2.5, 0.16, 2.4] }).as_deref(), Some("estilo_ala/b2500x160x2400"));
        assert_eq!(model_name("bidon_agua", ""), "bidon_agua/cuerpo");
        assert_eq!(model_name("asiento", "cojin"), "asiento/cojin");
    }
}
