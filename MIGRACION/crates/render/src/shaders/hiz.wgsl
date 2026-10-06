// Depth pyramid for occlusion culling: level 0 copies the frame's depth (0 = far outside the
// rendered area), each next level keeps the farthest (smallest, reversed Z) of what it covers.

struct HizU {
    viewport: vec4<u32>,   // rendered width, height
};

@group(0) @binding(0) var<uniform> hu: HizU;
@group(0) @binding(1) var depth_in: texture_depth_2d;
@group(0) @binding(2) var src: texture_2d<f32>;
@group(0) @binding(3) var dst: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8, 1)
fn copy_depth(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(dst);
    if gid.x >= dims.x || gid.y >= dims.y {
        return;
    }
    var d = 0.0;
    if gid.x < hu.viewport.x && gid.y < hu.viewport.y {
        d = textureLoad(depth_in, vec2<u32>(gid.xy), 0);
    }
    textureStore(dst, gid.xy, vec4<f32>(d, 0.0, 0.0, 0.0));
}

@compute @workgroup_size(8, 8, 1)
fn downsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(dst);
    if gid.x >= dims.x || gid.y >= dims.y {
        return;
    }
    let s = vec2<i32>(textureDimensions(src));
    let b = vec2<i32>(gid.xy) * 2;
    var m = 1.0;
    // 3x3 when the source is odd so no texel is skipped
    for (var y = 0; y < 3; y++) {
        for (var x = 0; x < 3; x++) {
            let p = b + vec2<i32>(x, y);
            let edge = (x == 2 && (s.x & 1) == 0) || (y == 2 && (s.y & 1) == 0);
            if edge || p.x >= s.x || p.y >= s.y {
                continue;
            }
            m = min(m, textureLoad(src, p, 0).r);
        }
    }
    textureStore(dst, gid.xy, vec4<f32>(m, 0.0, 0.0, 0.0));
}
