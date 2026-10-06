// GPU culling: one thread per instance, every view at once (main + each cascade's static and
// moving casters). Frustum, Hi-Z occlusion (previous frame's depth pyramid), distance and LOD by
// screen size; survivors are appended to per-view draw lists and counted into indirect commands.
// Self-contained.

struct Instance {
    pos: vec3<f32>,
    model: u32,
    rot: vec4<f32>,
    extra: vec3<f32>,    // anim phase, clip, scale
    flags: u32,          // 1 static, 2 moving
};

struct Model {
    bounds: vec4<f32>,
    thresholds: array<f32, 8>,
    cmds: array<u32, 8>,
    regions: array<u32, 8>,
    info: vec4<u32>,     // lod count, impostor lod, atlas layer, -
};

struct DrawCmd {
    index_count: u32,
    instance_count: atomic<u32>,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
};

struct View {
    planes: array<vec4<f32>, 6>,
    shell: vec4<f32>,    // shadow views: near, far (m from the camera), -, -
    flags: vec4<u32>,    // x: instance mask, y: shadow lod shift, z: is shadow, w: occlusion
};

const VIEWS: u32 = 9u;

struct CullU {
    views: array<View, VIEWS>,
    main_planes: array<vec4<f32>, 6>,
    prev_vp: mat4x4<f32>,
    origin: vec4<f32>,       // origin - camera
    prev_origin: vec4<f32>,  // origin - previous camera
    shadow_dir: vec4<f32>,   // away from the sun; w: sweep base (m)
    lod: vec4<f32>,          // px per (radius/distance), draw distance, sweep per radius, near plane
    counts: vec4<u32>,       // instances, commands per view, views in use, list stride
    hiz: vec4<f32>,          // pyramid size (w, h) of the previous frame, mip count, enabled
    proj: vec4<f32>,         // P00, P11 of the previous frame
};

// List entries: instance index in the low 24 bits; main-view entries may carry the LOD they fade
// from (bits 24-29), "fading" (bit 30) and "the lower LOD of the pair" (bit 31). Twin in mesh.wgsl.
const FADE_BAND: f32 = 1.3;
const FADE_BIT: u32 = 0x40000000u;
const LOWER_BIT: u32 = 0x80000000u;

@group(0) @binding(0) var<uniform> cu: CullU;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(0) @binding(2) var<storage, read> models: array<Model>;
@group(0) @binding(3) var<storage, read_write> cmds: array<DrawCmd>;
@group(0) @binding(4) var<storage, read_write> list: array<u32>;
@group(0) @binding(5) var hiz: texture_2d<f32>;

fn quat_rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

fn sphere_in(planes: array<vec4<f32>, 6>, c: vec3<f32>, r: f32) -> bool {
    for (var i = 0; i < 6; i++) {
        if dot(planes[i].xyz, c) + planes[i].w < -r {
            return false;
        }
    }
    return true;
}

fn capsule_in(planes: array<vec4<f32>, 6>, c: vec3<f32>, e: vec3<f32>, r: f32) -> bool {
    for (var i = 0; i < 6; i++) {
        let p = planes[i];
        if max(dot(p.xyz, c), dot(p.xyz, e)) + p.w < -r {
            return false;
        }
    }
    return true;
}

/// Hidden behind last frame's depth (reversed Z: the pyramid keeps the farthest = smallest depth).
fn occluded(c_rel_prev: vec3<f32>, r: f32) -> bool {
    let clip = cu.prev_vp * vec4<f32>(c_rel_prev, 1.0);
    if clip.w - r < cu.lod.w * 2.0 {
        return false;
    }
    let ndc = clip.xy / clip.w;
    let ext = vec2<f32>(r * cu.proj.x, r * cu.proj.y) / (clip.w - r);
    let lo = clamp(ndc - ext, vec2<f32>(-1.0), vec2<f32>(1.0));
    let hi = clamp(ndc + ext, vec2<f32>(-1.0), vec2<f32>(1.0));
    if any(lo >= hi) {
        return false;
    }
    let size = cu.hiz.xy;
    let uv_lo = vec2<f32>(lo.x * 0.5 + 0.5, 0.5 - hi.y * 0.5) * size;
    let uv_hi = vec2<f32>(hi.x * 0.5 + 0.5, 0.5 - lo.y * 0.5) * size;
    let span = max(uv_hi.x - uv_lo.x, uv_hi.y - uv_lo.y);
    let mip = min(u32(ceil(log2(max(span, 1.0)))), u32(cu.hiz.z) - 1u);
    let dims = vec2<i32>(textureDimensions(hiz, mip));
    let scale = 1.0 / f32(1u << mip);
    let a = clamp(vec2<i32>(floor(uv_lo * scale)), vec2<i32>(0), dims - 1);
    let b = clamp(vec2<i32>(floor(uv_hi * scale)), vec2<i32>(0), dims - 1);
    let far = min(min(textureLoad(hiz, a, mip).r, textureLoad(hiz, vec2<i32>(b.x, a.y), mip).r),
                  min(textureLoad(hiz, vec2<i32>(a.x, b.y), mip).r, textureLoad(hiz, b, mip).r));
    // nearest depth of the sphere (reversed, infinite: near / w)
    let nearest = cu.lod.w / (clip.w - r);
    return nearest < far;
}

@compute @workgroup_size(64)
fn cull(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= cu.counts.x {
        return;
    }
    let inst = instances[i];
    let m = models[inst.model];
    let scale = inst.extra.z;
    let offset = quat_rotate(inst.rot, m.bounds.xyz * scale);
    let c = inst.pos + offset + cu.origin.xyz;
    let r = m.bounds.w * scale;
    let dist = length(c);
    if dist - r > cu.lod.y {
        return;
    }
    // LOD by on-screen radius (px); past the last threshold: too small to draw
    let px = r / max(dist, 1e-3) * cu.lod.x;
    let lods = m.info.x;
    var lod = 0u;
    while lod < lods && px < m.thresholds[lod] {
        lod += 1u;
    }
    if lod >= lods {
        return;
    }
    let sweep = cu.shadow_dir.w + r * cu.lod.z;
    let end = c + cu.shadow_dir.xyz * sweep;
    for (var v = 0u; v < cu.counts.z; v++) {
        let view = cu.views[v];
        if (view.flags.x & inst.flags) == 0u {
            continue;
        }
        var l = lod;
        if view.flags.z == 0u {
            if !sphere_in(view.planes, c, r) {
                continue;
            }
            if view.flags.w != 0u && occluded(inst.pos + offset + cu.prev_origin.xyz, r) {
                continue;
            }
        } else {
            // a caster counts for a cascade only if it is in the light's box and its shadow,
            // swept away from the sun, reaches the part of the view this cascade covers
            if !sphere_in(view.planes, c, r) || !capsule_in(cu.main_planes, c, end, r) {
                continue;
            }
            let seg = end - c;
            let t = clamp(-dot(c, seg) / max(dot(seg, seg), 1e-6), 0.0, 1.0);
            let closest = length(c + seg * t);
            if max(length(c), length(end)) < view.shell.x - r || closest > view.shell.y + r {
                continue;
            }
            l = min(lod + view.flags.y, lods - 1u);
            if l >= m.info.y {
                // impostors cast no shadow: their mesh LOD before, or nothing
                if m.info.y == 0u {
                    continue;
                }
                l = m.info.y - 1u;
            }
        }
        var entry = i;
        let next = l + 1u;
        let next_mesh = next < lods && next != m.info.y;
        if view.flags.z == 0u && (next_mesh || next == lods) {
            // main view: LOD transitions (and the fade-out past the last LOD) are dithered
            // cross-fades over a band of on-screen sizes; the vertex shader recomputes the fade
            entry = i | (l << 24u) | FADE_BIT;
        }
        let cmd = v * cu.counts.y + m.cmds[l];
        let slot = atomicAdd(&cmds[cmd].instance_count, 1u);
        list[v * cu.counts.w + m.regions[l] + slot] = entry;
        if view.flags.z == 0u && next_mesh && px < m.thresholds[l] * FADE_BAND * 1.02 {
            // inside the band the next LOD fills the dither holes
            let cmd2 = v * cu.counts.y + m.cmds[next];
            let slot2 = atomicAdd(&cmds[cmd2].instance_count, 1u);
            list[v * cu.counts.w + m.regions[next] + slot2] = entry | LOWER_BIT;
        }
    }
}
