// Exhaust plumes (core::plumes): each a cone of light from a nozzle's mouth, drawn as two quads
// from one instance — the cone seen from its side (a quad along its axis, turned about it to
// face the camera) and the glow at its mouth (a quad facing the camera, which is also what is
// left of it looking along the axis and from far away). Light only (alpha 0: it adds), in the
// transparent pass with the scene depth bound: soft where it meets the hull or the ground,
// the glow hidden when its nozzle is. No texture: the shape is a few lines of arithmetic.
// Needs common.wgsl.

struct PlumeStyle {
    core: vec4<f32>,     // light of the hot gas at the mouth (linear, HDR); how far along the cone the core reaches (share of its length)
    plume: vec4<f32>,    // light of the cone; how fast it thins as it opens (exponent)
    shape: vec4<f32>,    // core width (nozzle radii), flicker (0..1), how fast the streaks run (lengths/s), -
    extra: vec4<f32>,
};

struct Plume {
    a: vec4<f32>,        // camera-relative mouth, its radius (m)
    b: vec4<f32>,        // the way the gas leaves (unit), length of the cone (m)
    c: vec4<f32>,        // radius at its far end (m), light (0..1), style, seed
};

@group(1) @binding(0) var<storage, read> plumes: array<Plume>;
@group(1) @binding(1) var<uniform> plume_styles: array<PlumeStyle, 16>;
@group(1) @binding(2) var scene_depth: texture_depth_2d;

struct PlumeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,                     // the glow: -1..1 both ways
    @location(1) rel: vec3<f32>,
    @location(2) @interpolate(flat) id: u32,
    // x: 0 the cone, 1 the glow; y: light; z: glow, cos² of the view against the jet; w: glow, radius drawn (m)
    @location(3) @interpolate(flat) k: vec4<f32>,
    // the cone: the way across it, as seen (where a pixel is along and across it is worked out
    // from where it is: a quad this much narrower at one end has no honest corner coordinates);
    // the glow: the jet's way across the picture (x, y: its length the sine of its angle to the view)
    @location(4) @interpolate(flat) side: vec3<f32>,
};

/// View depth (m) of the scene under a pixel (reversed infinite depth: near / d).
fn scene_at(px: vec2<i32>) -> f32 {
    return frame.view_proj[3].z / max(textureLoad(scene_depth, px, 0), 1e-12);
}

@vertex
fn plume_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) ii: u32) -> PlumeOut {
    let p = plumes[ii];
    let s = plume_styles[u32(p.c.z)];
    let pos = p.a.xyz;
    let r0 = p.a.w;
    let dir = p.b.xyz;
    let len = p.b.w;
    let r1 = p.c.x;
    // a quad as two triangles: (0,0) (1,0) (0,1) · (0,1) (1,0) (1,1)
    let q = vid % 6u;
    let cx = f32(q == 1u || q == 4u || q == 5u);
    let cy = f32(q == 2u || q == 3u || q == 5u);
    let to_cam = normalize(-(pos + dir * (len * 0.5)));
    let c = dot(dir, to_cam);
    let s2 = max(1.0 - c * c, 0.0);
    let px_per_m = frame.viewport.y / (2.0 * frame.cam_forward.w) / max(length(pos), 1e-3);
    var o: PlumeOut;
    o.id = ii;
    o.side = vec3<f32>(0.0);
    if vid < 6u {
        // the cone from its side: nothing of it looking along it, nor once it is a hair wide
        let side = normalize(cross(dir, to_cam) + vec3<f32>(1e-6, 0.0, 0.0));
        let u = cx * 2.0 - 1.0;
        let rel = pos + dir * (cy * len) + side * (u * mix(r0, r1, cy));
        o.clip = frame.view_proj * vec4<f32>(rel, 1.0);
        o.uv = vec2<f32>(u, cy);
        o.rel = rel;
        o.side = side;
        let seen = smoothstep(0.02, 0.3, s2) * smoothstep(0.5, 1.6, r1 * px_per_m);
        o.k = vec4<f32>(0.0, p.c.y * seen, 1.0 - s2, 0.0);
        if seen <= 0.0 {
            o.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
        }
        return o;
    }
    // the glow at the mouth: the hot gas there from any side; looking along the jet, the whole
    // cone piled up on the line of sight
    let c2 = 1.0 - s2;
    let centre = pos + dir * (r0 * 0.2);
    var gain = p.c.y;
    var r = max(r0 * s.shape.x * 1.6 * mix(0.6, 1.0, min(gain, 1.0)), 0.5 * r1 * c2);
    // under a pixel it keeps the pixel and spreads its light over it: a far engine does not flicker
    let px = r * px_per_m;
    if px < 1.0 {
        gain *= px * px;
        r = r / max(px, 1e-4);
    }
    // hidden when its nozzle is: the scene in front of its centre (five pixels round it, softly)
    let cc = frame.view_proj * vec4<f32>(centre, 1.0);
    var shown = 1.0;
    if cc.w > 1e-4 {
        let ndc = cc.xy / cc.w;
        let at = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * frame.viewport.xy;
        if at.x >= 1.0 && at.y >= 1.0 && at.x < frame.viewport.x - 2.0 && at.y < frame.viewport.y - 2.0 {
            let mine = dot(centre, frame.cam_forward.xyz) - max(r0 * 0.5, 0.02);
            let base = vec2<i32>(at);
            shown = 0.0;
            shown += select(0.0, 0.2, scene_at(base) > mine);
            shown += select(0.0, 0.2, scene_at(base + vec2<i32>(1, 0)) > mine);
            shown += select(0.0, 0.2, scene_at(base + vec2<i32>(-1, 0)) > mine);
            shown += select(0.0, 0.2, scene_at(base + vec2<i32>(0, 1)) > mine);
            shown += select(0.0, 0.2, scene_at(base + vec2<i32>(0, -1)) > mine);
        }
    }
    // facing the camera, pulled toward it by its radius (what it sits by does not cut it)
    let at_cam = normalize(-centre);
    let a = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(at_cam.y) > 0.9);
    let bx = normalize(cross(a, at_cam));
    let by = cross(at_cam, bx);
    let corner = vec2<f32>(cx * 2.0 - 1.0, cy * 2.0 - 1.0);
    let rel = centre + at_cam * min(r, length(centre) * 0.5) + (bx * corner.x + by * corner.y) * r;
    o.clip = frame.view_proj * vec4<f32>(rel, 1.0);
    o.uv = corner;
    o.rel = rel;
    o.side = vec3<f32>(dot(dir, bx), dot(dir, by), 0.0);
    o.k = vec4<f32>(1.0, gain * shown, c2, r);
    if gain * shown <= 0.0 {
        o.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return o;
}

fn plume_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash12(i), hash12(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash12(i + vec2<f32>(0.0, 1.0)), hash12(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

@fragment
fn plume_fs(in: PlumeOut) -> @location(0) vec4<f32> {
    let p = plumes[in.id];
    let s = plume_styles[u32(p.c.z)];
    let r0 = p.a.w;
    let len = p.b.w;
    let r1 = p.c.x;
    let time = frame.origin.w;
    let seed = p.c.w * 61.0;
    // (a little of the whole jet's light comes and goes)
    let breath = 1.0 + s.shape.y * 0.5 * (plume_noise(vec2<f32>(time * 11.0 + seed, seed)) - 0.5);
    var light: vec3<f32>;
    if in.k.x < 0.5 {
        let d = in.rel - p.a.xyz;
        let t = clamp(dot(d, p.b.xyz) / max(len, 1e-4), 0.0, 1.0);
        let r = mix(r0, r1, t);
        let x = dot(d, in.side) / max(r, 1e-4);
        let x2 = x * x;
        if x2 >= 1.0 {
            discard;
        }
        // the gas thins as it opens: what a line of sight crosses of it goes with r0 / r
        let thin = pow(r0 / max(r, 1e-4), s.plume.w);
        let edge = pow(1.0 - x2, 1.5);
        let tail = (1.0 - t) * (1.0 - t);
        // streaks running out along it
        let run = plume_noise(vec2<f32>(x * 2.5 + seed, t * 5.0 - time * s.shape.z * 5.0));
        let streak = 1.0 - s.shape.y + s.shape.y * 2.0 * run;
        // the core: a narrow bright spike out of the mouth, as wide as the nozzle wherever it is
        let xr = x * r / max(r0 * s.shape.x, 1e-4);
        let core = exp(-xr * xr * 2.0) * exp(-t / max(s.core.w, 1e-3));
        light = s.plume.rgb * (edge * thin * tail * streak) + s.core.rgb * core;
        // soft where it meets the scene
        let scene = scene_at(vec2<i32>(in.clip.xy));
        let mine = dot(in.rel, frame.cam_forward.xyz);
        light *= clamp((scene - mine) / max(r * 0.5, 0.03), 0.0, 1.0);
    } else {
        let d2 = dot(in.uv, in.uv);
        if d2 >= 1.0 {
            discard;
        }
        // the hot gas across the mouth: a disc, seen as flat as the view makes it (from the side
        // a bright line across the nozzle, not a ball of light)
        let way = in.side.xy / max(length(in.side.xy), 1e-3);
        let along = dot(in.uv, way) * in.k.w;
        let across = dot(in.uv, vec2<f32>(-way.y, way.x)) * in.k.w;
        let rc = max(r0 * s.shape.x, 1e-4);
        let flat = rc * mix(0.4, 1.0, in.k.z);
        let hot = exp(-1.2 * (across * across / (rc * rc) + along * along / (flat * flat)));
        let pile = (1.0 - d2) * in.k.z * min(len / max(r0 + r1, 1e-3), 4.0) * 0.6;
        light = (s.core.rgb * hot * (0.25 + 0.75 * in.k.z) + s.plume.rgb * pile) * (1.0 - d2);
    }
    // (the eye inside it: no wall of light)
    light *= in.k.y * breath * smoothstep(0.05, 0.5, length(in.rel));
    return vec4<f32>(light, 0.0);
}
