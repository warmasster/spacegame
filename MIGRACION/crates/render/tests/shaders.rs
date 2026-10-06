//! Every shader combination parses and validates (fast, no GPU).
const COMMON: &str = include_str!("../src/shaders/common.wgsl");
const TERRAIN_COMMON: &str = include_str!("../src/shaders/terrain_common.wgsl");

#[test]
fn shaders_validate() {
    let sets: [(&str, Vec<&str>); 11] = [
        ("terrain", vec![COMMON, TERRAIN_COMMON, include_str!("../src/shaders/terrain.wgsl")]),
        ("terrain gen", vec![TERRAIN_COMMON, include_str!("../src/shaders/terrain_gen.wgsl")]),
        ("sky", vec![COMMON, include_str!("../src/shaders/sky.wgsl")]),
        ("post", vec![include_str!("../src/shaders/post.wgsl")]),
        ("mesh", vec![COMMON, include_str!("../src/shaders/mesh.wgsl")]),
        ("cull", vec![include_str!("../src/shaders/cull.wgsl")]),
        ("hiz", vec![include_str!("../src/shaders/hiz.wgsl")]),
        ("particles", vec![COMMON, include_str!("../src/shaders/particles.wgsl")]),
        ("plumes", vec![COMMON, include_str!("../src/shaders/plumes.wgsl")]),
        ("structures", vec![COMMON, include_str!("../src/shaders/structure.wgsl")]),
        ("props", vec![COMMON, include_str!("../src/shaders/props.wgsl")]),
    ];
    let mut failed = Vec::new();
    for (name, parts) in sets {
        let src = parts.concat();
        match naga::front::wgsl::parse_str(&src) {
            Err(e) => failed.push(format!("{name}: {}", e.emit_to_string(&src))),
            Ok(module) => {
                let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
                if let Err(e) = v.validate(&module) {
                    failed.push(format!("{name}: {}", e.emit_to_string(&src)));
                }
            }
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
