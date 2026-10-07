// Bodies: meshes that bend with a skeleton posed on the CPU every frame (the player's own body,
// whoever is near enough to be worth it). Each vertex follows up to four bones of its body's
// palette; the body comes camera-relative from the CPU like a prop. A bone's part of it may be
// faded (one's own arm in front of what one aims at): drawn as a screen of dots, what is behind
// shows through. Needs common.wgsl.

struct Body {
    pos: vec4<f32>,      // camera-relative origin; w: scale
    rot: vec4<f32>,      // quaternion
    params: vec4<f32>,   // x: 1 inside a hull, y: first bone of its palette
};

// a bone: the rows of the 3x4 matrix that takes a vertex at rest to where the bone has it
struct Bone {
    r0: vec4<f32>,
    r1: vec4<f32>,
    r2: vec4<f32>,
};

@group(1) @binding(0) var<storage, read> bodies: array<Body>;
@group(1) @binding(1) var<storage, read> bones: array<Bone>;
// how faded each bone's part is (0 solid .. 1 gone)
@group(1) @binding(2) var<storage, read> fades: array<f32>;

struct BIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    // sRGB colour; roughness, metalness, glow
    @location(2) color: vec4<f32>,
    @location(3) own: vec4<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
};

struct BOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) own: vec3<f32>,
    @location(4) @interpolate(flat) inside: f32,
    @location(5) fade: f32,
};

fn bone_point(b: Bone, p: vec3<f32>) -> vec3<f32> {
    let q = vec4<f32>(p, 1.0);
    return vec3<f32>(dot(b.r0, q), dot(b.r1, q), dot(b.r2, q));
}

fn bone_dir(b: Bone, n: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(b.r0.xyz, n), dot(b.r1.xyz, n), dot(b.r2.xyz, n));
}

/// The vertex where its bones have it, in its body's frame.
fn skinned(v: BIn, first: u32) -> vec3<f32> {
    var p = vec3<f32>(0.0);
    for (var k = 0; k < 4; k++) {
        let w = v.weights[k];
        if w > 0.0 {
            p += w * bone_point(bones[first + v.joints[k]], v.pos);
        }
    }
    return p;
}

@vertex
fn body_vs(v: BIn, @builtin(instance_index) ii: u32) -> BOut {
    let b = bodies[ii];
    let first = u32(b.params.y);
    var n = vec3<f32>(0.0);
    var fade = 0.0;
    for (var k = 0; k < 4; k++) {
        let w = v.weights[k];
        if w > 0.0 {
            n += w * bone_dir(bones[first + v.joints[k]], v.nrm);
            fade += w * fades[first + v.joints[k]];
        }
    }
    let rel = b.pos.xyz + quat_rotate(b.rot, skinned(v, first) * b.pos.w);
    var o: BOut;
    o.clip = pass_u.view_proj * vec4<f32>(rel, 1.0);
    o.rel = rel;
    o.nrm = quat_rotate(b.rot, n);
    o.color = srgb_to_linear(v.color.rgb);
    o.own = v.own.xyz;
    o.inside = b.params.x;
    o.fade = fade;
    return o;
}

@vertex
fn body_depth_vs(v: BIn, @builtin(instance_index) ii: u32) -> @builtin(position) vec4<f32> {
    let b = bodies[ii];
    return pass_u.view_proj * vec4<f32>(b.pos.xyz + quat_rotate(b.rot, skinned(v, u32(b.params.y)) * b.pos.w), 1.0);
}

// a 4x4 ordered screen: each pixel's threshold (0..1), so that a faded part keeps as many dots
// as it is solid, spread evenly
fn screen(p: vec2<f32>) -> f32 {
    let i = vec2<u32>(p) % vec2<u32>(4u);
    var m = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);
    return (f32(m[i.y * 4u + i.x]) + 0.5) / 16.0;
}

@fragment
fn body_fs(in: BOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    if in.fade > 0.003 && screen(in.clip.xy) < in.fade {
        discard;
    }
    var n = normalize(in.nrm);
    if !front {
        n = -n;
    }
    let v = normalize(-in.rel);
    let s = sun_light(in.rel, n, in.clip.xy);
    let c = shade_surface(in.color, max(in.own.x, 0.04), in.own.y, in.own.z * 3.0, n, v, s) + in.color * (1.0 - in.own.y) * side_light(in.rel, n, in.inside > 0.5);
    return vec4<f32>(c, 1.0);
}
