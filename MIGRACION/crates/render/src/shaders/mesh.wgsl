// Instanced meshes drawn from the culling's lists: rigid (ships, rocks), skinned (NPCs, baked
// animation texture), octahedral impostors. Hull plating (seams, plates, grime, windows) is drawn
// here from model space, so the fine detail costs no triangles. Needs common.wgsl.

struct Instance {
    pos: vec3<f32>,
    model: u32,
    rot: vec4<f32>,
    extra: vec3<f32>,    // anim phase, clip, scale
    flags: u32,
};

struct Clip {
    first: f32,
    frames: f32,
    joints: f32,
    loop_: f32,
};

@group(1) @binding(0) var<storage, read> instances: array<Instance>;
@group(1) @binding(1) var<storage, read> list: array<u32>;
@group(1) @binding(2) var anim: texture_2d<f32>;
@group(1) @binding(3) var<storage, read> clips: array<Clip>;
@group(1) @binding(4) var atlas: texture_2d_array<f32>;
struct Model {
    bounds: vec4<f32>,
    thresholds: array<f32, 8>,
    cmds: array<u32, 8>,
    regions: array<u32, 8>,
    info: vec4<u32>,     // lod count, impostor lod, atlas base layer, -
};
@group(1) @binding(5) var<storage, read> models: array<Model>;
@group(1) @binding(6) var<storage, read> glows: array<vec4<f32>>;

struct RigidIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>,
};

struct SkinnedIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) params: vec3<f32>,
    // LOD cross-fade: x = share drawn by the finer LOD, y = 1 on the coarser one
    @location(4) @interpolate(flat) fade: vec2<f32>,
    // model space: position (w: height over the model's bounds, 0 bottom .. 1 top) and normal
    @location(5) local: vec4<f32>,
    @location(6) lnrm: vec3<f32>,
    // Material::panel: plate size (dm) in the low 7 bits, windows bit
    @location(7) @interpolate(flat) panel: u32,
};

// List entries (twin of cull.wgsl): instance index in the low 24 bits, then the LOD faded from,
// "fading" and "lower LOD of the pair".
const FADE_BAND: f32 = 1.3;
const FADE_BIT: u32 = 0x40000000u;
const LOWER_BIT: u32 = 0x80000000u;

fn entry_instance(e: u32) -> Instance {
    return instances[e & 0xffffffu];
}

/// The cross-fade of a list entry: the same on-screen size the culling measured, mapped over the
/// band above the LOD's threshold (1 = fully the finer LOD).
fn entry_fade(e: u32, inst: Instance) -> vec2<f32> {
    if (e & FADE_BIT) == 0u {
        return vec2<f32>(1.0, 0.0);
    }
    let m = models[inst.model];
    let l = (e >> 24u) & 63u;
    let c = inst.pos + quat_rotate(inst.rot, m.bounds.xyz * inst.extra.z) + frame.origin.xyz;
    let r = m.bounds.w * inst.extra.z;
    let px = r / max(length(c), 1e-3) * frame.viewport.y / (2.0 * frame.cam_forward.w) * frame.params2.y;
    let t = m.thresholds[l];
    let f = clamp((px - t) / (t * (FADE_BAND - 1.0)), 0.0, 1.0);
    return vec2<f32>(f, select(0.0, 1.0, (e & LOWER_BIT) != 0u));
}

/// Ordered 4x4 dither in [0, 1).
fn bayer4(p: vec2<f32>) -> f32 {
    let i = vec2<u32>(p) & vec2<u32>(3u);
    let m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (m[i.y * 4u + i.x] + 0.5) / 16.0;
}

fn place(inst: Instance, p: vec3<f32>) -> vec3<f32> {
    return quat_rotate(inst.rot, p * inst.extra.z) + inst.pos + frame.origin.xyz;
}

fn finish(inst: Instance, local: vec3<f32>, n: vec3<f32>, color: vec4<f32>, params: vec4<f32>, fade: vec2<f32>) -> VOut {
    var o: VOut;
    o.fade = fade;
    let b = models[inst.model].bounds;
    o.local = vec4<f32>(local, (local.y - b.y + b.w) / max(2.0 * b.w, 1e-3));
    o.lnrm = n;
    o.rel = place(inst, local);
    o.clip = pass_u.view_proj * vec4<f32>(o.rel, 1.0);
    o.nrm = quat_rotate(inst.rot, n);
    o.color = srgb_to_linear(color.rgb);
    o.params = params.xyz;
    o.panel = u32(round(params.w * 255.0));
    return o;
}

@vertex
fn rigid_vs(v: RigidIn, @builtin(instance_index) ii: u32) -> VOut {
    let e = list[ii];
    let inst = entry_instance(e);
    return finish(inst, v.pos, v.nrm.xyz, v.color, v.params, entry_fade(e, inst));
}

@vertex
fn rigid_depth_vs(v: RigidIn, @builtin(instance_index) ii: u32) -> @builtin(position) vec4<f32> {
    let inst = entry_instance(list[ii]);
    return pass_u.view_proj * vec4<f32>(place(inst, v.pos), 1.0);
}

// ---- skinning: 3 rows of a 3x4 matrix per joint per frame ----

fn joint_row(j: u32, frame_i: u32, row: u32) -> vec4<f32> {
    return textureLoad(anim, vec2<u32>(j * 3u + row, frame_i), 0);
}

fn skin_matrix(j: u32, f0: u32, f1: u32, t: f32) -> mat3x4<f32> {
    return mat3x4<f32>(
        mix(joint_row(j, f0, 0u), joint_row(j, f1, 0u), t),
        mix(joint_row(j, f0, 1u), joint_row(j, f1, 1u), t),
        mix(joint_row(j, f0, 2u), joint_row(j, f1, 2u), t));
}

fn skin(inst: Instance, joints: vec4<u32>, weights: vec4<f32>) -> mat3x4<f32> {
    let clip = clips[u32(inst.extra.y)];
    let f = fract(inst.extra.x) * clip.frames;
    let a = u32(f) % u32(clip.frames);
    let b = (a + 1u) % u32(clip.frames);
    let t = fract(f);
    let f0 = u32(clip.first) + a;
    let f1 = u32(clip.first) + b;
    var m = skin_matrix(joints.x, f0, f1, t) * weights.x;
    if weights.y > 0.0 { m += skin_matrix(joints.y, f0, f1, t) * weights.y; }
    if weights.z > 0.0 { m += skin_matrix(joints.z, f0, f1, t) * weights.z; }
    if weights.w > 0.0 { m += skin_matrix(joints.w, f0, f1, t) * weights.w; }
    return m;
}

@vertex
fn skinned_vs(v: SkinnedIn, @builtin(instance_index) ii: u32) -> VOut {
    let e = list[ii];
    let inst = entry_instance(e);
    let m = skin(inst, v.joints, v.weights);
    let p = vec4<f32>(v.pos, 1.0) * m;
    let n = vec4<f32>(v.nrm.xyz, 0.0) * m;
    return finish(inst, p, normalize(n), v.color, v.params, entry_fade(e, inst));
}

@vertex
fn skinned_depth_vs(v: SkinnedIn, @builtin(instance_index) ii: u32) -> @builtin(position) vec4<f32> {
    let inst = entry_instance(list[ii]);
    let m = skin(inst, v.joints, v.weights);
    let p = vec4<f32>(v.pos, 1.0) * m;
    return pass_u.view_proj * vec4<f32>(place(inst, p), 1.0);
}

// ---- hull plating ----

/// Half-width of a seam between plates (m).
const SEAM: f32 = 0.02;

struct Plating {
    /// 0 on a plate .. 1 on a seam (antialiased, gone where the plates are too small on screen).
    seam: f32,
    /// One random number per plate.
    id: f32,
    /// Where in its plate, 0..1 each way, and one pixel in those units.
    inner: vec2<f32>,
    aa: vec2<f32>,
    /// 1 near; 0 once the plates shrink to a few pixels (no moiré, nothing to see).
    fade: f32,
};

/// Plates of `size` m (twice as long as tall, rows shifted like brickwork) on the model-space
/// plane facing `n`. `px`: metres per pixel here.
fn plating(p: vec3<f32>, n: vec3<f32>, size: f32, px: f32) -> Plating {
    let a = abs(n);
    var uv = p.xy;
    var face = 3.0;
    if a.x >= a.y && a.x >= a.z {
        uv = p.zy;
        face = 1.0;
    } else if a.y >= a.z {
        uv = p.xz;
        face = 2.0;
    }
    let dims = vec2<f32>(size, size * 0.5);
    let row = floor(uv.y / dims.y);
    let q = vec2<f32>(uv.x + hash12(vec2<f32>(row, face * 7.0)) * dims.x, uv.y) / dims;
    let cell = floor(q);
    let f = q - cell;
    let edge = min(min(f.x, 1.0 - f.x) * dims.x, min(f.y, 1.0 - f.y) * dims.y);
    var o: Plating;
    o.fade = 1.0 - smoothstep(size * 0.03, size * 0.12, px);
    o.seam = (1.0 - smoothstep(SEAM - px * 0.5, SEAM + px, edge)) * o.fade;
    o.id = hash12(cell + vec2<f32>(face * 31.7, face * 11.3));
    o.inner = f;
    o.aa = px / dims;
    return o;
}

fn hull_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash12(i);
    let b = hash12(i + vec2<f32>(1.0, 0.0));
    let c = hash12(i + vec2<f32>(0.0, 1.0));
    let d = hash12(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

/// 1 inside the box [lo, hi] of a plate (u, v in 0..1), antialiased by `aa`.
fn inset(f: vec2<f32>, lo: vec2<f32>, hi: vec2<f32>, aa: vec2<f32>) -> f32 {
    let w = smoothstep(lo - aa, lo + aa, f) * (1.0 - smoothstep(hi - aa, hi + aa, f));
    return w.x * w.y;
}

@fragment
fn mesh_fs(in: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // metres per pixel, taken while every pixel of the quad is still alive
    let px = max(length(fwidth(in.local.xyz)), 1e-5);
    // LOD cross-fade: the finer LOD keeps the pixels under the fade, the coarser one the rest
    let d = bayer4(in.clip.xy);
    if (in.fade.y > 0.5) == (d < in.fade.x) {
        discard;
    }
    var n = normalize(in.nrm);
    if !front {
        n = -n;
    }
    var albedo = in.color;
    var rough = in.params.x;
    var emissive = in.params.z;
    let size = f32(in.panel & 0x7fu) * 0.1;
    if size > 0.0 {
        let pl = plating(in.local.xyz, in.lnrm, size, px);
        if (in.panel & 0x80u) != 0u {
            // windows: a pane in each plate, most lit (a warm spread), some dark
            let pane = inset(pl.inner, vec2<f32>(0.14, 0.22), vec2<f32>(0.86, 0.78), pl.aa) * pl.fade;
            let lit = step(0.2, pl.id) * (0.55 + 0.6 * fract(pl.id * 7.13));
            emissive *= mix(1.0, pane * lit, pl.fade);
            albedo = mix(albedo, vec3<f32>(0.035, 0.04, 0.045), pl.fade * (1.0 - pane * lit));
            rough = mix(rough, 0.08, pane);
        } else {
            // a hull of plates: each its own shade and sheen, a few dark service hatches,
            // dark seams, and grime streaking down, heavier low on the hull
            let shade = mix(0.84, 1.06, pl.id) * select(1.0, 0.68, pl.id > 0.93);
            albedo *= mix(1.0, shade, pl.fade);
            rough = clamp(rough + (pl.id - 0.5) * 0.14 * pl.fade, 0.03, 1.0);
            albedo *= 1.0 - 0.62 * pl.seam;
            rough = mix(rough, 0.95, pl.seam);
            let p = in.local.xyz;
            let streak = hull_noise(vec2<f32>(dot(p.xz, vec2<f32>(1.9, 1.3)) * 1.6, p.y * 0.35));
            let grime = smoothstep(0.45, 1.0, streak * 0.7 + (1.0 - in.local.w) * 0.55);
            albedo *= 1.0 - 0.22 * grime;
            rough = mix(rough, 1.0, 0.25 * grime);
        }
    }
    let v = normalize(-in.rel);
    let s = sun_light(in.rel, n, in.clip.xy);
    let c = shade_surface(albedo, max(rough, 0.03), in.params.y, emissive, n, v, s);
    return vec4<f32>(c + albedo * (1.0 - in.params.y) * flash_light(in.rel, n), 1.0);
}

// ---- octahedral impostors: a camera-facing quad showing the pre-rendered view closest to the
// direction the object is seen from (with per-texel normals and depth for lighting) ----

const IMP_GRID: f32 = 8.0;

struct ImpOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) cell: vec2<f32>,
    @location(3) @interpolate(flat) layer: u32,
    @location(4) @interpolate(flat) rot: vec4<f32>,
    @location(5) @interpolate(flat) radius: f32,
};

fn oct_cell(d: vec3<f32>) -> vec2<f32> {
    // hemisphere-free octahedral map of a local direction
    let e = oct_encode(d) * 0.5 + 0.5;
    return min(floor(e * IMP_GRID), vec2<f32>(IMP_GRID - 1.0));
}

fn oct_dir(cell: vec2<f32>) -> vec3<f32> {
    return oct_decode((cell + 0.5) / IMP_GRID * 2.0 - 1.0);
}

@vertex
fn impostor_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) ii: u32) -> ImpOut {
    let inst = entry_instance(list[ii]);
    let m = models[inst.model];
    let bounds = m.bounds;
    let r = bounds.w * inst.extra.z;
    let center = place(inst, bounds.xyz);
    // the direction toward the camera, in the object's frame
    let to_cam = normalize(-center);
    let inv = vec4<f32>(-inst.rot.xyz, inst.rot.w);
    let local = quat_rotate(inv, to_cam);
    let cell = oct_cell(local);
    // the quad faces the baked view direction (so the picture lines up), in world space
    let view_dir = quat_rotate(inst.rot, oct_dir(cell));
    var up = quat_rotate(inst.rot, vec3<f32>(0.0, 1.0, 0.0));
    if abs(dot(up, view_dir)) > 0.99 {
        up = quat_rotate(inst.rot, vec3<f32>(0.0, 0.0, 1.0));
    }
    let right = normalize(cross(up, view_dir));
    let upv = cross(view_dir, right);
    let corners = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
    let k = corners[vid];
    let p = center + (right * k.x + upv * k.y) * r;
    var o: ImpOut;
    o.rel = p;
    o.clip = pass_u.view_proj * vec4<f32>(p, 1.0);
    o.uv = vec2<f32>(k.x * 0.5 + 0.5, 0.5 - k.y * 0.5);
    o.cell = cell;
    o.layer = m.info.z;
    o.rot = inst.rot;
    o.radius = r;
    return o;
}

@fragment
fn impostor_fs(in: ImpOut) -> @location(0) vec4<f32> {
    let uv = (in.cell + in.uv) / IMP_GRID;
    // atlas layer pair: albedo+rough, normal (object space, oct) + emission
    let a = textureSampleLevel(atlas, linear_clamp, uv, in.layer, 0.0);
    if a.a < 0.5 {
        discard;
    }
    let nb = textureSampleLevel(atlas, linear_clamp, uv, in.layer + 1u, 0.0);
    let n = quat_rotate(in.rot, oct_decode(nb.xy * 2.0 - 1.0));
    let v = normalize(-in.rel);
    let s = sun_light(in.rel, n, in.clip.xy);
    let c = shade_surface(a.rgb, 0.5, 0.0, nb.z, n, v, s);
    return vec4<f32>(c + a.rgb * flash_light(in.rel, n), 1.0);
}

// ---- glows: engines and beacons as sprites that never shrink below a few pixels ----

struct GlowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn glow_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) ii: u32) -> GlowOut {
    var o: GlowOut;
    o.clip = vec4<f32>(0.0);
    let inst = instances[ii / 16u];
    let k = ii % 16u;
    let m = models[inst.model];
    let count = m.info.w & 15u;
    if k >= count {
        return o;
    }
    let at = (m.info.w >> 4u) + k;
    let a = glows[at * 2u];
    let b = glows[at * 2u + 1u];
    let p = place(inst, a.xyz);
    let dist = length(p);
    // pulled toward the camera so the ship's own nozzle does not hide it
    let center = p - p / max(dist, 1e-3) * min(a.w * 1.5, dist * 0.5);
    let clip = pass_u.view_proj * vec4<f32>(center, 1.0);
    let px_scale = frame.viewport.y / (2.0 * frame.cam_forward.w);
    let phys = a.w * inst.extra.z / max(dist, 1e-3) * px_scale;
    let px = clamp(phys, 1.5, 16.0);
    // up close the geometry shows the glow; only far away (a few pixels) does the sprite take
    // over, fading in smoothly. A light smaller than its sprite spreads its light over it (the
    // area ratio): it dims with the square of the distance, as a real light, instead of a crowd
    // of far ships summing into one bright blot
    let shrink = min(phys / px, 1.0);
    let far_gain = shrink * shrink;
    let near_fade = 1.0 - smoothstep(4.0, 13.0, phys);
    let corners = array<vec2<f32>, 6>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));
    let c = corners[vid];
    o.clip = vec4<f32>(clip.xy + c * px * 2.0 * frame.viewport.zw * clip.w, clip.z, clip.w);
    o.corner = c;
    o.color = b.rgb * b.w * far_gain * near_fade;
    return o;
}

@fragment
fn glow_fs(in: GlowOut) -> @location(0) vec4<f32> {
    let r2 = dot(in.corner, in.corner);
    let f = exp(-r2 * 5.0) + exp(-r2 * 1.5) * 0.15;
    return vec4<f32>(in.color * f, 1.0);
}
