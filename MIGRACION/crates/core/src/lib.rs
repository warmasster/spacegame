//! Graphics-free core: bodies and their surfaces (the lunar surface is an f64 port of the TS
//! terrain), star systems, content definitions (models, hulls, palette, scenario) and their
//! loaders, the quadtree/GPU generator tables, frusta, procedural meshes, graphics settings and the
//! stress-test scene (SoA).
pub mod anim;
pub mod body;
pub mod cube_sphere;
pub mod deform;
pub mod defs;
pub mod detonation;
pub mod effect_defs;
pub mod effects;
pub mod font;
pub mod frustum;
pub mod guided;
pub mod hull;
pub mod mesh;
pub mod missiles;
pub mod model;
pub mod noise;
pub mod palette;
pub mod particles;
pub mod plumes;
pub mod props;
pub mod quadtree;
pub mod quality;
pub mod rounds;
pub mod scenario;
pub mod scene;
pub mod simplify;
pub mod structure;
pub mod surface;
pub mod system;
pub mod terrain_gen;
pub mod traffic;
