// Marks on loose ground (app::footprints): a boot's print, a landing pad's, the dent of a crate,
// the ground a jet swept. Each is a box round where it lies, drawn from one instance in the
// transparent pass; a pixel of it finds the ground under itself in the scene's depth, places it in
// the mark's own frame and says how the ground there differs from the ground round it: darker
// where it is pressed, lighter on the rim thrown up, lit by the sun as its slopes face it. It
// multiplies what the terrain drew, so the mark takes the ground's own light, shadows and lamps;
// it lies on whatever the terrain's mesh is at that distance (no z-fight, nothing floating) and
// nothing of it is a texture: the tread is a few lines of arithmetic. Needs common.wgsl.

struct PrintKind {
    a: vec4<f32>,        // shape (0 boot, 1 disc, 2 box, 3 swept), light of the pressed floor (x the ground's), of the rim, how deep (m)
    b: vec4<f32>,        // rim: how high (m), how wide (share of the half-width); tread: bars along it, how deep (share of the depth)
    c: vec4<f32>,        // how long it lasts (s; 0: for ever), the share of that it fades over, seen whole within (m), gone at (m)
    d: vec4<f32>,        // relief (x the slopes), grain (0..1), -, -
};

struct Print {
    a: vec4<f32>,        // its middle from the anchor (m), half its width (m)
    b: vec4<f32>,        // the ground's normal there (unit), half its length (m)
    c: vec4<f32>,        // the way it points along the ground (unit), when it was left (s, the frame's clock)
    d: vec4<f32>,        // kind, how hard it was pressed (0..2), seed, side (1 left, -1 right; swept ground: any, its streaks turn with it)
};

struct PrintsU {
    anchor: vec4<f32>,   // the anchor - the camera
};

@group(1) @binding(0) var<storage, read> prints: array<Print>;
@group(1) @binding(1) var<uniform> print_kinds: array<PrintKind, 16>;
@group(1) @binding(2) var<uniform> prints_u: PrintsU;
@group(1) @binding(3) var scene_depth: texture_depth_2d;

/// The frame's clock goes round at this (uniforms.rs).
const CLOCK: f32 = 100000.0;
/// How far past its edge a mark's box reaches, for its rim (shares of its half-sizes).
const MARGIN: f32 = 1.35;

// corners of a box (bit 0 x, bit 1 y, bit 2 z), its twelve triangles wound outward
const BOX: array<u32, 36> = array<u32, 36>(
    0u, 4u, 6u, 0u, 6u, 2u, 1u, 3u, 7u, 1u, 7u, 5u, 0u, 1u, 5u, 0u, 5u, 4u,
    2u, 6u, 7u, 2u, 7u, 3u, 0u, 2u, 3u, 0u, 3u, 1u, 4u, 5u, 7u, 4u, 7u, 6u);

struct PrintOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) @interpolate(flat) id: u32,
    // how much of it is left (age, distance), half the box's height (m), a pixel there (m)
    @location(2) @interpolate(flat) k: vec3<f32>,
    @location(3) @interpolate(flat) centre: vec3<f32>,
};

@vertex
fn print_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) ii: u32) -> PrintOut {
    let p = prints[ii];
    let kind = print_kinds[u32(p.d.x)];
    let centre = prints_u.anchor.xyz + p.a.xyz;
    let dist = length(centre);
    var left = 1.0 - smoothstep(kind.c.z, kind.c.w, dist);
    if kind.c.x > 0.0 {
        var age = frame.origin.w - p.c.w;
        if age < 0.0 {
            age += CLOCK;
        }
        left *= 1.0 - smoothstep(kind.c.x * (1.0 - kind.c.y), kind.c.x, age);
    }
    left *= min(p.d.y * 4.0, 1.0);
    let half = vec2<f32>(p.a.w, p.b.w) * MARGIN;
    // tall enough for the ground under it not to leave it: rough ground, and far away the
    // coarser mesh the terrain is drawn with
    let tall = clamp(0.35 * max(half.x, half.y), 0.06, 1.5) + dist * 0.01;
    let n = p.b.xyz;
    let f = p.c.xyz;
    let x = cross(n, f);
    let c = BOX[vid % 36u];
    let corner = vec3<f32>(f32(c & 1u), f32((c >> 1u) & 1u), f32((c >> 2u) & 1u)) * 2.0 - 1.0;
    let rel = centre + x * (corner.x * half.x) + n * (corner.y * tall) + f * (corner.z * half.y);
    var o: PrintOut;
    o.clip = frame.view_proj * vec4<f32>(rel, 1.0);
    o.rel = rel;
    o.id = ii;
    o.k = vec3<f32>(left, tall, dist * 2.0 * frame.cam_forward.w / frame.viewport.y);
    o.centre = centre;
    if left <= 0.0 {
        o.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return o;
}

fn print_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash12(i), hash12(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash12(i + vec2<f32>(0.0, 1.0)), hash12(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

/// How far out of a mark a point of it is (under 1: inside), by its shape. `q`: across and along
/// it, each -1..1 at its edges; `side`: 1 a left boot, -1 a right one.
fn print_out(shape: u32, q: vec2<f32>, side: f32) -> f32 {
    if shape == 0u {
        // a boot's sole: narrow at the heel, a waist, wide at the ball, round at both ends, its
        // toe turned toward the other foot
        let v = q.y;
        let mid = -side * 0.11 * smoothstep(-0.3, 1.0, v);
        var w = mix(0.74, 0.97, smoothstep(-0.25, 0.45, v)) - 0.1 * exp(-(v + 0.18) * (v + 0.18) * 14.0);
        let heel = clamp((-0.62 - v) / 0.38, 0.0, 1.0);
        let toe = clamp((v - 0.5) / 0.5, 0.0, 1.0);
        w *= sqrt(max(1.0 - heel * heel, 0.0)) * sqrt(max(1.0 - toe * toe, 0.0));
        return max(abs(q.x - mid) / max(w, 1e-3), abs(v));
    }
    if shape == 2u {
        // a box set down: square, its corners a little round
        let a = abs(q);
        let a4 = a * a * a * a;
        return sqrt(sqrt(sqrt(a4.x * a4.x + a4.y * a4.y)));
    }
    return length(q);
}

/// The ground of a mark at `q`: how high (m; under 0 pressed in) and how light (x the ground's).
fn print_ground(kind: PrintKind, q: vec2<f32>, side: f32, hard: f32, grain: f32) -> vec2<f32> {
    let shape = u32(kind.a.x);
    var d = print_out(shape, q, side);
    if shape == 3u {
        // swept by a jet: nothing pressed, the dust gone from the middle outward in streaks
        let streak = 0.55 + 0.45 * print_noise(vec2<f32>(atan2(q.y, q.x) * 5.0 + side * 7.0, d * 2.5));
        let swept = (1.0 - smoothstep(0.1, 1.0, d)) * streak * min(hard, 1.0);
        return vec2<f32>(-kind.a.w * swept, mix(1.0, kind.a.y, swept));
    }
    // (its edge is not a ruled line: the soil crumbles)
    d += (grain - 0.5) * 0.09 * kind.d.y;
    let inside = 1.0 - smoothstep(0.86, 1.0, d);
    let rw = max(kind.b.y, 0.02);
    let r = (d - 1.0 - rw) / rw;
    let rim = exp(-r * r * 1.6) * (1.0 - inside);
    // the tread: bars across the sole, each a groove in what it stood on
    var tread = 0.0;
    if kind.b.z > 0.0 {
        tread = kind.b.w * smoothstep(-0.35, 0.35, sin(q.y * 3.14159265 * kind.b.z));
    }
    let depth = kind.a.w * hard;
    let h = -depth * inside * (1.0 - tread) + kind.b.x * rim * min(hard, 1.5);
    let light = mix(1.0, kind.a.y, inside * min(hard, 1.0)) * mix(1.0, kind.a.z, rim);
    return vec2<f32>(h, light);
}

@fragment
fn print_fs(in: PrintOut) -> @location(0) vec4<f32> {
    let p = prints[in.id];
    let kind = print_kinds[u32(p.d.x)];
    // the ground under this pixel (camera-relative): the scene's depth along the pixel's ray
    let depth = textureLoad(scene_depth, vec2<i32>(in.clip.xy), 0);
    let pos = in.rel * (frame.view_proj[3].z / max(depth, 1e-12) / max(dot(in.rel, frame.cam_forward.xyz), 1e-6));
    // (what faces another way is not the ground: the side of a boot, a strut)
    var facing = normalize(cross(dpdx(pos), dpdy(pos)));
    facing *= -sign(dot(facing, pos));
    if depth <= 0.0 {
        discard;
    }
    let n = p.b.xyz;
    let f = p.c.xyz;
    let x = cross(n, f);
    let d = pos - in.centre;
    let local = vec3<f32>(dot(d, x), dot(d, n), dot(d, f));
    let half = vec2<f32>(p.a.w, p.b.w);
    let q = local.xz / half;
    if abs(local.y) > in.k.y || abs(q.x) > MARGIN || abs(q.y) > MARGIN {
        discard;
    }
    let seen = in.k.x * smoothstep(0.35, 0.65, dot(facing, n));
    if seen <= 0.0 {
        discard;
    }
    let side = p.d.w;
    let hard = p.d.y;
    let grain = print_noise(local.xz * 55.0 + p.d.z * 17.0);
    // its slopes, over no less than a pixel (far away its relief goes, its shade stays)
    let e = max(0.004, in.k.z);
    let g0 = print_ground(kind, q, side, hard, grain);
    let gx = print_ground(kind, q + vec2<f32>(e / half.x, 0.0), side, hard, grain);
    let gz = print_ground(kind, q + vec2<f32>(0.0, e / half.y), side, hard, grain);
    let slope = vec2<f32>(gx.x - g0.x, gz.x - g0.x) / e * kind.d.x;
    let n2 = normalize(n - x * slope.x - f * slope.y);
    // lit like the ground round it, as much more or less as its slopes face the sun; in shadow
    // only its shade is left
    let l = frame.sun_dir.xyz;
    let level = max(dot(n, l), 0.0);
    let sunlit = sun_light(pos, n, in.clip.xy) * smoothstep(0.0, 0.04, level);
    let ratio = clamp((max(dot(n2, l), 0.0) + 0.05) / (level + 0.05), 0.12, 3.0);
    let m = mix(1.0, g0.y * mix(1.0, ratio, sunlit), seen);
    return vec4<f32>(m, m, m, 1.0);
}
