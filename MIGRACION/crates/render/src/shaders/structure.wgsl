// Modular structures (core::structure): every structure of a blueprint draws the same range of a
// shared vertex pool (instanced), placed by a per-structure transform (camera-relative origin +
// turn). Moving parts (doors, ramps, gear, nacelles) are posed by their bone: a 3x4 matrix per
// bone in a shared storage buffer, so a moving ramp never rebuilds a mesh. Each vertex carries its
// part's material and plating and says which part it is; what that part is like now (there or
// gone, how damaged, lit) is a word per part in a storage buffer: a part gone folds to nothing,
// a damaged one darkens (soot, more and more, until it is black), a lamp unfed goes dark.
// Plating is painted sheet steel: big aligned panels with fine recessed seams (bevelled in the
// normal, so the sun picks them out), rows of fasteners, paint that varies a little from panel
// to panel, chipped edges showing bare metal, a non-slip grit on rough floors. Glass comes last
// in each range and is drawn blended. Needs common.wgsl.

struct Xform {
    pos: vec4<f32>,      // origin, camera-relative; w: dither of a level cross-fade (0 none)
    rot: vec4<f32>,      // structure frame -> world (quaternion)
    info: vec4<u32>,     // x: first bone matrix of this structure (bone 1), y: bone count, z: 0 as it is, 1-6 level + 1 to tint by (tool), 100 its integrity, w: its first part word
};

@group(1) @binding(0) var<storage, read> xforms: array<Xform>;
// three rows per bone (3x4, structure frame at rest -> structure frame posed)
@group(1) @binding(1) var<storage, read> bones: array<vec4<f32>>;
// finishes (core::mesh::FINISHES from 1): rgba = albedo factor (0.5 = x1), roughness offset
// (0.5 = none), normal x, normal y of the tile
@group(1) @binding(2) var finishes: texture_2d_array<f32>;
@group(1) @binding(3) var finish_sampler: sampler;
// a word per part (core::structure::look::part_states): bits 0-7 damage, 8 there, 9 lit
@group(1) @binding(4) var<storage, read> part_words: array<u32>;

const WORD_ALIVE: u32 = 0x100u;
const WORD_LIT: u32 = 0x200u;
const WORD_AWAY: u32 = 0x400u;
const NO_PART: u32 = 0xffffu;
const MODE_INTEGRITY: u32 = 100u;

/// The word of part `part` of the structure drawn with `x` (a vertex of no one part: there,
/// sound, lit).
fn part_word(x: Xform, part: u32) -> u32 {
    if part == NO_PART {
        return WORD_ALIVE | WORD_LIT;
    }
    return part_words[x.info.w + part];
}

// per finish: tile size (m) and how strongly its relief shows
var<private> FINISH_TILE: array<f32, 17> = array<f32, 17>(1.0, 0.55, 0.45, 0.6, 0.28, 0.16, 0.07, 0.4, 0.22, 0.32, 0.5, 0.09, 1.6, 0.045, 0.3, 1.2, 0.45);
var<private> FINISH_BUMP: array<f32, 17> = array<f32, 17>(0.0, 0.35, 0.45, 0.9, 0.7, 0.55, 1.0, 1.1, 1.0, 1.0, 1.0, 0.5, 0.8, 1.0, 1.0, 0.7, 0.9);

struct SIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec4<f32>,
    @location(2) albedo: vec4<f32>,  // sRGB, damage
    @location(3) params: vec4<f32>,  // rough, metal, glow, plating code / 255
    @location(4) extra: vec4<u32>,   // bone, fragment, glass, finish
    @location(5) ids: vec2<u32>,     // its part, what its part is (bit 0: inside a hull)
};

struct SOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) local: vec3<f32>,
    @location(3) albedo: vec4<f32>,
    @location(4) params: vec4<f32>,
    @location(5) @interpolate(flat) fade: f32,
    @location(6) lnrm: vec3<f32>,
    // fragment (bit 1: a part that is gone, shown by the integrity view; bit 2: inside a hull),
    // glass, finish, mode
    @location(7) @interpolate(flat) flags: vec4<u32>,
    // world directions of the structure frame's axes (posed with the part's bone)
    @location(8) ax: vec3<f32>,
    @location(9) ay: vec3<f32>,
    @location(10) az: vec3<f32>,
};

/// The rest-pose point `p` (w = 1) or direction (w = 0) as bone `b` of `x` poses it.
fn posed(x: Xform, b: u32, p: vec4<f32>) -> vec3<f32> {
    if b == 0u || b > x.info.y {
        return p.xyz;
    }
    let k = (x.info.x + b - 1u) * 3u;
    return vec3<f32>(dot(bones[k], p), dot(bones[k + 1u], p), dot(bones[k + 2u], p));
}

@vertex
fn structure_vs(v: SIn, @builtin(instance_index) ii: u32) -> SOut {
    let x = xforms[ii];
    let b = v.extra.x;
    let word = part_word(x, v.ids.x);
    let gone = (word & WORD_ALIVE) == 0u;
    var o: SOut;
    // a part that is gone folds to a point off the screen (but the integrity view shows it,
    // unless it only left: cargo let go is not missing)
    if gone && (x.info.z != MODE_INTEGRITY || (word & WORD_AWAY) != 0u) {
        o.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return o;
    }
    let p = posed(x, b, vec4<f32>(v.pos, 1.0));
    let n = posed(x, b, vec4<f32>(v.nrm.xyz, 0.0));
    let rel = x.pos.xyz + quat_rotate(x.rot, p);
    o.clip = pass_u.view_proj * vec4<f32>(rel, 1.0);
    o.rel = rel;
    o.nrm = quat_rotate(x.rot, n);
    o.local = v.pos;
    o.lnrm = v.nrm.xyz;
    o.albedo = vec4<f32>(v.albedo.rgb, max(v.albedo.a, f32(word & 0xffu) / 255.0));
    o.params = vec4<f32>(v.params.xy, select(0.0, v.params.z, (word & WORD_LIT) != 0u), v.params.w);
    o.fade = x.pos.w;
    o.flags = vec4<u32>(v.extra.y | select(0u, 2u, gone) | ((v.ids.y & 1u) << 2u), v.extra.z, v.extra.w, x.info.z);
    o.ax = quat_rotate(x.rot, posed(x, b, vec4<f32>(1.0, 0.0, 0.0, 0.0)));
    o.ay = quat_rotate(x.rot, posed(x, b, vec4<f32>(0.0, 1.0, 0.0, 0.0)));
    o.az = quat_rotate(x.rot, posed(x, b, vec4<f32>(0.0, 0.0, 1.0, 0.0)));
    return o;
}

@vertex
fn structure_depth_vs(v: SIn, @builtin(instance_index) ii: u32) -> @builtin(position) vec4<f32> {
    let x = xforms[ii];
    if (part_word(x, v.ids.x) & WORD_ALIVE) == 0u {
        return vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    let p = posed(x, v.extra.x, vec4<f32>(v.pos, 1.0));
    return pass_u.view_proj * vec4<f32>(x.pos.xyz + quat_rotate(x.rot, p), 1.0);
}

fn bayer4(p: vec2<f32>) -> f32 {
    let i = vec2<u32>(p) & vec2<u32>(3u);
    let m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (m[i.y * 4u + i.x] + 0.5) / 16.0;
}

fn hash33(p: vec3<f32>) -> vec3<f32> {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.xxy + q.yxx) * q.zyx);
}

/// 0 inside a cell .. 1 on the border between two cells (3D Voronoi, F2 - F1).
fn cell_edge(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    var f1 = 8.0;
    var f2 = 8.0;
    for (var z = -1; z <= 1; z++) {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let o = vec3<f32>(f32(x), f32(y), f32(z));
                let d = length(o + hash33(i + o) - f);
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
    }
    return 1.0 - smoothstep(0.0, 0.12, f2 - f1);
}

// ---- plating: painted sheet steel ----

fn plate_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash12(i);
    let b = hash12(i + vec2<f32>(1.0, 0.0));
    let c = hash12(i + vec2<f32>(0.0, 1.0));
    let d = hash12(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

/// Two octaves of value noise, 0..1.
fn fbm2(p: vec2<f32>) -> f32 {
    return plate_noise(p) * 0.65 + plate_noise(p * 2.03 + 17.1) * 0.35;
}

/// The rest-frame axes the plating's (u, v) run along on a face facing `n` (box projection).
fn plate_axes(n: vec3<f32>) -> array<vec3<f32>, 2> {
    let a = abs(n);
    if a.x >= a.y && a.x >= a.z {
        return array<vec3<f32>, 2>(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 1.0, 0.0));
    }
    if a.y >= a.z {
        return array<vec3<f32>, 2>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    }
    return array<vec3<f32>, 2>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0));
}

/// Panel gap (m), the rounded edge either side of it, fastener head radius and inset (m).
const GAP: f32 = 0.0018;
const BEVEL: f32 = 0.007;
const SCREW: f32 = 0.0055;
const INSET: f32 = 0.024;

struct Surface {
    albedo: vec3<f32>,
    rough: f32,
    metal: f32,
    /// World normal (the plating's relief on it).
    n: vec3<f32>,
};

/// World direction of a structure-frame direction `d` on the part of `in`.
fn to_world(in: SOut, d: vec3<f32>) -> vec3<f32> {
    return in.ax * d.x + in.ay * d.y + in.az * d.z;
}

/// The part's finish: its texture from the three sides (blended by the face's turn), its light
/// and dark into the albedo, its sheen into the roughness, its relief into the normal. `dx`, `dy`:
/// screen derivatives of the rest-frame position (taken where every pixel runs).
fn finish(in: SOut, s: ptr<function, Surface>, dx: vec3<f32>, dy: vec3<f32>) {
    let f = in.flags.z;
    if f == 0u || f >= 17u {
        return;
    }
    let tile = FINISH_TILE[f];
    let layer = i32(f) - 1;
    var w = pow(abs(in.lnrm), vec3<f32>(4.0));
    w /= max(w.x + w.y + w.z, 1e-5);
    let p = in.local / tile;
    let gx = dx / tile;
    let gy = dy / tile;
    var albedo = 0.0;
    var rough = 0.0;
    var bump = vec3<f32>(0.0);
    if w.x > 0.02 {
        let t = textureSampleGrad(finishes, finish_sampler, p.zy, layer, gx.zy, gy.zy);
        albedo += w.x * t.r;
        rough += w.x * t.g;
        bump += w.x * ((t.b * 2.0 - 1.0) * in.az + (t.a * 2.0 - 1.0) * in.ay);
    }
    if w.y > 0.02 {
        let t = textureSampleGrad(finishes, finish_sampler, p.xz, layer, gx.xz, gy.xz);
        albedo += w.y * t.r;
        rough += w.y * t.g;
        bump += w.y * ((t.b * 2.0 - 1.0) * in.ax + (t.a * 2.0 - 1.0) * in.az);
    }
    if w.z > 0.02 {
        let t = textureSampleGrad(finishes, finish_sampler, p.xy, layer, gx.xy, gy.xy);
        albedo += w.z * t.r;
        rough += w.z * t.g;
        bump += w.z * ((t.b * 2.0 - 1.0) * in.ax + (t.a * 2.0 - 1.0) * in.ay);
    }
    let sum = max(select(0.0, w.x, w.x > 0.02) + select(0.0, w.y, w.y > 0.02) + select(0.0, w.z, w.z > 0.02), 1e-5);
    (*s).albedo *= albedo / sum * 2.0;
    (*s).rough = clamp((*s).rough + (rough / sum - 0.5) * 1.0, 0.03, 1.0);
    let b = bump / sum;
    (*s).n = normalize((*s).n + (b - (*s).n * dot(b, (*s).n)) * FINISH_BUMP[f] * 1.6);
}

/// Painted panels of `size` m (the short side; the long side runs along u) on the face of `in`.
fn plating(in: SOut, size: f32, s: ptr<function, Surface>) {
    let px = max(length(fwidth(in.local)), 1e-5);
    // past a few pixels per seam nothing of it shows: a plain painted surface
    let fade = 1.0 - smoothstep(size * 0.012, size * 0.05, px);
    let ax = plate_axes(in.lnrm);
    let uv = vec2<f32>(dot(in.local, ax[0]), dot(in.local, ax[1]));
    let face = dot(abs(in.lnrm), vec3<f32>(1.0, 2.0, 3.0));
    let dims = vec2<f32>(size * 1.6, size);
    let q = uv / dims;
    let cell = floor(q);
    let f = q - cell;
    let m = f * dims;
    // distance to the nearest edge and which way is inward
    let ex = min(m.x, dims.x - m.x);
    let ey = min(m.y, dims.y - m.y);
    let e = min(ex, ey);
    var inward = vec2<f32>(select(-1.0, 1.0, m.x < dims.x * 0.5), 0.0);
    if ey < ex {
        inward = vec2<f32>(0.0, select(-1.0, 1.0, m.y < dims.y * 0.5));
    }
    let id = hash12(cell + vec2<f32>(face * 31.7, face * 11.3));
    // paint: a batch tint per panel, broad mottling, smudges in the sheen
    let mottle = fbm2(uv * 1.7 + face * 5.3);
    let tint = 1.0 + (id - 0.5) * 0.045 + (mottle - 0.5) * 0.05;
    (*s).albedo *= mix(1.0, tint, fade);
    let smudge = fbm2(uv * 4.3 + 9.1);
    (*s).rough = clamp((*s).rough + (smudge - 0.5) * 0.16 * fade, 0.05, 1.0);
    // a non-slip grit on rough floors
    if (*s).rough > 0.6 && in.lnrm.y > 0.7 {
        let grit = hash12(floor(uv * 900.0));
        let g = (1.0 - smoothstep(0.0006, 0.002, px)) * fade;
        (*s).albedo *= 1.0 + (grit - 0.5) * 0.12 * g;
        (*s).rough = clamp((*s).rough + (grit - 0.5) * 0.2 * g, 0.05, 1.0);
    }
    // the recessed seam: the gap is dark, the rounded edges either side tilt toward it
    let sharp = 1.0 - smoothstep(BEVEL * 0.6, BEVEL * 2.0, px);
    let gap = 1.0 - smoothstep(GAP - px * 0.5, GAP + px * 0.5, e);
    let cover = clamp(GAP * 2.0 / px, 0.0, 1.0);
    (*s).albedo *= 1.0 - 0.75 * gap * mix(cover, 1.0, sharp) * fade;
    (*s).rough = mix((*s).rough, 0.9, gap * fade);
    var slope = vec2<f32>(0.0);
    if e < BEVEL {
        let k = 1.0 - e / BEVEL;
        slope = inward * (2.2 * k * k) * sharp * fade;
    }
    // a little grime gathers along the seams
    let grime = (1.0 - smoothstep(0.0, 0.035, e)) * (0.5 + 0.5 * fbm2(uv * 11.0));
    (*s).albedo *= 1.0 - 0.1 * grime * fade;
    // fasteners: rows along every edge, inset, a domed head with a dark ring
    let pitch = vec2<f32>(dims.x / max(round(dims.x / 0.16), 1.0), dims.y / max(round(dims.y / 0.16), 1.0));
    let row = vec2<f32>(m.x - (floor(m.x / pitch.x) + 0.5) * pitch.x, m.y - select(dims.y - INSET, INSET, m.y < dims.y * 0.5));
    let col = vec2<f32>(m.x - select(dims.x - INSET, INSET, m.x < dims.x * 0.5), m.y - (floor(m.y / pitch.y) + 0.5) * pitch.y);
    let sd = select(row, col, dot(col, col) < dot(row, row));
    let d = length(sd);
    let screws = (1.0 - smoothstep(SCREW * 0.5, SCREW * 1.6, px)) * fade;
    if d < SCREW * 1.35 && screws > 0.0 {
        let head = 1.0 - smoothstep(SCREW - px, SCREW + px, d);
        let ring = (1.0 - head) * (1.0 - smoothstep(SCREW * 1.05, SCREW * 1.35, d));
        (*s).albedo *= 1.0 - (0.12 * head + 0.45 * ring) * screws;
        (*s).rough = mix((*s).rough, (*s).rough * 0.7, head * screws);
        // the dome leans away from its centre
        slope -= sd / SCREW * 0.9 * head * screws;
    }
    // chipped paint on the edges: bare steel shows
    let chips = fbm2(uv * 38.0 + face * 3.1);
    let wear = 0.1 + 0.5 * in.albedo.a;
    let chip = step(1.0 - wear * 0.5, chips) * (1.0 - smoothstep(0.003, 0.016, e)) * (1.0 - smoothstep(0.002, 0.006, px)) * fade;
    (*s).albedo = mix((*s).albedo, vec3<f32>(0.42, 0.42, 0.43), chip);
    (*s).metal = mix((*s).metal, 0.9, chip);
    (*s).rough = mix((*s).rough, 0.35, chip);
    (*s).n = normalize((*s).n - (to_world(in, ax[0]) * slope.x + to_world(in, ax[1]) * slope.y));
}

fn surface(in: SOut, n: vec3<f32>, dx: vec3<f32>, dy: vec3<f32>) -> Surface {
    var s: Surface;
    s.albedo = srgb_to_linear(in.albedo.rgb);
    s.rough = max(in.params.x, 0.04);
    s.metal = in.params.y;
    s.n = n;
    finish(in, &s, dx, dy);
    let damage = in.albedo.a;
    let code = u32(round(in.params.w * 255.0));
    let size = f32(code & 0x7fu) * 0.1;
    if size > 0.0 {
        plating(in, size, &s);
    }
    // damage: soot. A little dirt at the first hits, then blotches of black that spread until
    // the whole part is charred; dull and no longer metal under it
    if damage > 0.0 {
        let blot = fbm2(vec2<f32>(dot(in.local.xz, vec2<f32>(1.3, 0.9)), in.local.y) * 2.6);
        let soot = clamp(damage * 1.25 + (blot - 0.5) * 0.6 * (1.0 - damage), 0.0, 1.0);
        let char = smoothstep(0.08, 0.95, soot);
        s.albedo = mix(s.albedo * (1.0 - 0.3 * damage), vec3<f32>(0.012, 0.011, 0.010), char);
        s.rough = mix(s.rough, 0.96, char);
        s.metal = mix(s.metal, 0.0, char);
    }
    // the bare cut of a broken piece: the material without paint, darker
    if (in.flags.x & 1u) != 0u {
        s.albedo *= 0.75;
    }
    return s;
}

/// The integrity view (the scanner's): what is sound is a dim grey shape, what is damaged glows
/// red, the more the worse, and what is gone shows purple where it should be.
fn integrity(in: SOut, n: vec3<f32>, v: vec3<f32>) -> vec3<f32> {
    let rim = pow(1.0 - clamp(dot(n, v), 0.0, 1.0), 2.0);
    if (in.flags.x & 2u) != 0u {
        let beat = 0.75 + 0.25 * sin(frame.origin.w * 4.0);
        return vec3<f32>(0.45, 0.08, 0.95) * (0.5 + 1.6 * rim) * beat * 1.2;
    }
    let d = in.albedo.a;
    let shape = vec3<f32>(0.05, 0.06, 0.07) * (0.5 + 1.5 * rim);
    let hurt = smoothstep(0.02, 1.0, d);
    return mix(shape, vec3<f32>(1.0, 0.05, 0.02) * (0.25 + 2.2 * hurt * hurt + rim * hurt), hurt);
}

@fragment
fn structure_fs(in: SOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // level cross-fade: the incoming level keeps the dither cells under `fade`, the outgoing the rest
    if in.fade != 0.0 {
        let b = bayer4(in.clip.xy);
        if (in.fade > 0.0 && b >= in.fade) || (in.fade < 0.0 && b < -in.fade) {
            discard;
        }
    }
    var n = normalize(in.nrm);
    if !front {
        n = -n;
    }
    let v = normalize(-in.rel);
    if in.flags.w == MODE_INTEGRITY {
        return vec4<f32>(integrity(in, n, v), 1.0);
    }
    let sf = surface(in, n, dpdx(in.local), dpdy(in.local));
    // shadows from the geometric normal, light from the relief
    let s = sun_light(in.rel, n, in.clip.xy);
    var c = shade_surface(sf.albedo, sf.rough, sf.metal, in.params.z, sf.n, v, s) + sf.albedo * (1.0 - sf.metal) * side_light(in.rel, sf.n, (in.flags.x & 4u) != 0u);
    // tool: tinted by its level of detail
    if in.flags.w > 1u {
        let tints = array<vec3<f32>, 6>(vec3<f32>(1.0), vec3<f32>(0.2, 1.0, 0.2), vec3<f32>(0.1, 0.9, 1.0), vec3<f32>(1.0, 0.95, 0.1), vec3<f32>(1.0, 0.5, 0.05), vec3<f32>(1.0, 0.1, 0.1));
        let t = tints[min(in.flags.w - 1u, 5u)];
        c = mix(c, t * max(dot(c, vec3<f32>(0.33)), 0.05) * 2.0 + t * 0.02, 0.75);
    }
    return vec4<f32>(c, 1.0);
}

/// Glass: tinted, mostly see-through, reflecting more at grazing angles (Fresnel), lit by the sun
/// and the lights round it. Premultiplied alpha.
@fragment
fn glass_fs(in: SOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.nrm);
    if !front {
        n = -n;
    }
    let v = normalize(-in.rel);
    if in.flags.w == MODE_INTEGRITY {
        return vec4<f32>(integrity(in, n, v), 0.85);
    }
    let tint = srgb_to_linear(in.albedo.rgb);
    let damage = in.albedo.a;
    let nv = clamp(dot(n, v), 0.0, 1.0);
    let fresnel = 0.04 + 0.96 * pow(1.0 - nv, 5.0);
    let s = sun_light(in.rel, n, in.clip.xy);
    let l = frame.sun_dir.xyz;
    let h = normalize(v + l);
    let spec = pow(max(dot(n, h), 0.0), 900.0) * 40.0 * s * frame.sun_dir.w;
    // a hurt pane clouds over, darker the worse it is
    let frost = smoothstep(0.1, 0.95, damage) * 0.8;
    let alpha = clamp(0.12 + fresnel * 0.55 + frost * 0.7, 0.0, 0.95);
    let lit = tint * (0.02 + 0.3 * frost) * (frame.sun_dir.w * s * 0.2 + 0.05) + flash_light(in.rel, n) * 0.05;
    let c = lit + vec3<f32>(spec) + tint * fresnel * 0.08;
    return vec4<f32>(c * alpha + vec3<f32>(spec) * (1.0 - alpha), alpha);
}
