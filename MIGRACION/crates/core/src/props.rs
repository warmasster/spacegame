//! Props: what a structure shows besides its mesh, rebuilt every frame by its owner (panel
//! controls posed as they stand, gauge needles, lamps, screens, actuator rods) as instances of a
//! few primitive meshes, text as quads of the silkscreen font, and the lamps that light the scene.
//! Plain data for any renderer.
use glam::{DVec3, Quat, Vec3};

/// Primitive meshes: unit sized (a box of side 1, a cylinder of diameter 1 and height 1 along +Y,
/// a sphere of diameter 1, a cone from a base of diameter 1 at y = −0.5 to its tip at +0.5).
pub const BOX: u8 = 0;
pub const CYLINDER: u8 = 1;
pub const SPHERE: u8 = 2;
pub const CONE: u8 = 3;
/// How many primitives there are: a renderer numbers the models it is given from here on.
pub const PRIMITIVES: u8 = 4;

#[derive(Clone, Copy, Debug, Default)]
pub struct PropFrame {
    pub pos: DVec3,
    pub rot: Quat,
    /// What is drawn in it is inside a hull: lit by the lamps in there only.
    pub inside: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Prop {
    pub frame: u16,
    pub mesh: u8,
    /// In its frame: centre, turn and size (m).
    pub pos: Vec3,
    pub rot: Quat,
    pub size: Vec3,
    /// sRGB.
    pub color: [u8; 3],
    pub emissive: f32,
    pub rough: u8,
    pub metal: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphQuad {
    pub frame: u16,
    /// In its frame: centre and half extents along the text and up it.
    pub center: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub uv: [f32; 4],
    /// sRGB.
    pub color: [u8; 3],
    pub emissive: f32,
    /// Relief of the letters: 0 flat paint, > 0 raised (moulded, embossed) that much, < 0 cut in.
    pub relief: f32,
}

/// A decal (poster, sign, logo, plate): a quad of the decal atlas laid on a surface.
#[derive(Clone, Copy, Debug)]
pub struct DecalQuad {
    pub frame: u16,
    /// In its frame: centre and half extents across and up.
    pub center: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    /// Atlas rectangle (u0, v0 top, u1, v1).
    pub uv: [f32; 4],
    /// Multiplies its colours (sRGB); lit signs glow.
    pub tint: [u8; 3],
    pub emissive: f32,
}

/// One frame's props, filled by the app (reused: clear and refill).
#[derive(Default)]
pub struct PropScene {
    pub frames: Vec<PropFrame>,
    pub props: Vec<Prop>,
    pub glyphs: Vec<GlyphQuad>,
    pub decals: Vec<DecalQuad>,
}

impl PropScene {
    pub fn clear(&mut self) {
        self.frames.clear();
        self.props.clear();
        self.glyphs.clear();
        self.decals.clear();
    }
}

/// The decal atlas' contents (`assets/textures/calcas.json`, made by `tools/texturas/calcas.py`):
/// each decal's rectangle and its aspect (width / height).
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct DecalAtlas {
    pub size: u32,
    pub calcas: std::collections::BTreeMap<String, DecalEntry>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct DecalEntry {
    pub uv: [f32; 4],
    pub aspecto: f32,
}

impl DecalAtlas {
    pub fn parse(json: &str) -> Result<DecalAtlas, String> {
        serde_json::from_str(json).map_err(|e| format!("calcas: {e}"))
    }
}

/// A lamp: world position, colour × intensity (linear), range (m); a spot along `dir` when `cone`
/// (cosine of its half-angle) > 0; `inside` lamps light only structures, not the ground.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lamp {
    pub pos: DVec3,
    pub color: [f32; 3],
    pub range: f32,
    pub dir: Vec3,
    pub cone: f32,
    pub inside: bool,
}
