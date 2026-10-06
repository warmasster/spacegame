// GPU twin of core::surface::Surface::sample, evaluated per texel of a quadtree node.
// Needs terrain_common.wgsl. Pass 1 (gen_heights) writes positions/heights/albedo of the node grid
// plus a border into scratch; with baked shadows, gen_horizon marches toward the sun far past the
// node (coarser relief) on a 9x9 subgrid; gen_finish derives normals and baked sun visibility (the
// near march over scratch, the far horizon interpolated) and stores the node's textures.

struct SurfaceU {
    a: vec4<f32>,        // radius, datum + level offset, highland amp, mare depth
    b: vec4<f32>,        // mare from, mare to, mountain amp, mountain gain
    c: vec4<f32>,        // relief amp, relief gain, mare smooth, complex from
    d: vec4<f32>,        // albedo highland, albedo mare, crater detail, crater odds fade
    e: vec4<u32>,        // crater salt (seed*31*golden), grid, border, baked shadows
    sun: vec4<f32>,      // toward the sun (world)
    layers: array<vec4<f32>, 24>, // per crater layer: (cells, cell m, margin, p), (mare, rmin, rmax, -)
};

struct Job {
    calls: array<vec4<f32>, 25>,  // phase fraction xyz, scale
    cells: array<vec4<i32>, 25>,  // phase integer cell (mod 256)
    node: Node,
    split: vec4<f32>,             // centre params a, b as hi + lo
    counts: vec4<u32>,            // relief octaves, mountain octaves, crater layers, parent's (packed)
    fades: vec4<f32>,             // min feature, broad fade, detail fade, -
    blasts: array<vec4<f32>, 128>, // blast craters (2 x MAX_NODE_CRATERS), oldest first: (direction - node's, radius), (depth, rim, level it wipes to, seed + 2 if the parent keeps it)
    blasts_n: vec4<u32>,          // x: blast count, y: the far horizon's octaves and layers (packed)
};

// Blast craters: twins of core::deform (REACH, WOBBLE, LOBES_GAIN, CREST, LOBES, PHASES and the
// functions).
const BLAST_REACH: f32 = 3.0;
const BLAST_WOBBLE: f32 = 0.07;
const BLAST_LOBES: f32 = 0.35;
const BLAST_CREST: f32 = 0.18;
const BLAST_ERASE_TO: f32 = 1.6;

/// How much of the old ground a crater wipes out (1 in the bowl, 0 from ERASE_TO radii).
fn blast_erase(rr: f32, wobble: f32) -> f32 {
    return 1.0 - smoothstep(1.0, BLAST_ERASE_TO, rr / (1.0 + BLAST_WOBBLE * wobble));
}

/// A crater's irregularity along unit `v` from its centre: (rim wobble -1..1, blanket lobes -1..1,
/// ray 0..1: colour only).
fn blast_pattern(v: vec3<f32>, seed: f32) -> vec3<f32> {
    let dirs = array<vec3<f32>, 5>(vec3(0.6, 0.48, 0.64), vec3(-0.8, 0.36, 0.48), vec3(0.0, -0.6, 0.8), vec3(0.48, 0.8, -0.36), vec3(-0.36, -0.48, -0.8));
    let phases = array<f32, 5>(12.9898, 78.233, 37.719, 4.581, 93.989);
    var ph: array<f32, 5>;
    var dv: array<f32, 5>;
    for (var i = 0u; i < 5u; i++) {
        ph[i] = fract(seed * phases[i]) * 6.2831853;
        dv[i] = dot(v, dirs[i]);
    }
    let wobble = (sin(ph[0] + 3.0 * dv[0]) + sin(ph[1] + 5.0 * dv[1]) + 0.5 * sin(ph[2] + 9.0 * dv[2])) / 2.5;
    let lobes = sin(ph[3] + 4.0 * dv[3]) * sin(ph[4] + 3.0 * dv[4]);
    let rays = max(sin(ph[3] + 23.0 * dv[3]) * sin(ph[4] + 17.0 * dv[4]), 0.0);
    return vec3<f32>(wobble, lobes, rays);
}

/// In depths: -1 at the centre, a rounded crest about `rim` high (wobbled), the (r/R)^-3 blanket
/// varied by `lobes`, 0 from the reach on: the smooth minimum of the wall and the blanket.
fn blast_profile(rr_in: f32, rim: f32, wobble: f32, lobes: f32) -> f32 {
    let rr = rr_in / (1.0 + BLAST_WOBBLE * wobble);
    if rr >= BLAST_REACH {
        return 0.0;
    }
    let wall = -1.0 + (1.0 + rim) * rr * rr;
    let x = max(rr, 0.05);
    let q = (x - 1.0) / (BLAST_REACH - 1.0);
    let w = 1.0 - q * q;
    let lobed = 1.0 + BLAST_LOBES * lobes * smoothstep(1.0, 1.5, x);
    let blanket = rim * w * w * lobed / (x * x * x);
    let h = max(BLAST_CREST - abs(wall - blanket), 0.0) / BLAST_CREST;
    return min(wall, blanket) - h * h * BLAST_CREST * 0.25;
}

@group(0) @binding(0) var<uniform> su: SurfaceU;
@group(0) @binding(1) var<storage, read> perm: array<u32>;
@group(0) @binding(2) var<storage, read> jobs: array<Job>;
@group(0) @binding(3) var<storage, read_write> scratch_pos: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> scratch_alb: array<vec2<f32>>; // albedo, height the parent leaves out
@group(0) @binding(5) var height_out: texture_storage_2d_array<rg32uint, write>;
@group(0) @binding(6) var normal_out: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(7) var<storage, read_write> scratch_hor: array<f32>; // far horizon (sin), 9x9 per job

/// Far horizon subgrid side.
const HOR: u32 = 9u;

fn fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn grad(h: u32, x: f32, y: f32, z: f32) -> f32 {
    let g = h & 15u;
    let u = select(y, x, g < 8u);
    let v = select(select(z, x, g == 12u || g == 14u), y, g < 4u);
    return select(-u, u, (g & 1u) == 0u) + select(-v, v, (g & 2u) == 0u);
}

/// Improved Perlin noise of table `t` at integer cell `ci` + offset `x`.
fn perlin(t: u32, ci: vec3<i32>, x: vec3<f32>) -> f32 {
    let fl = floor(x);
    let c = vec3<u32>((ci + vec3<i32>(fl)) & vec3<i32>(255));
    let f = x - fl;
    let o = t * 512u;
    let u = fade(f.x);
    let v = fade(f.y);
    let w = fade(f.z);
    let a = perm[o + c.x] + c.y;
    let aa = perm[o + a] + c.z;
    let ab = perm[o + a + 1u] + c.z;
    let b = perm[o + c.x + 1u] + c.y;
    let ba = perm[o + b] + c.z;
    let bb = perm[o + b + 1u] + c.z;
    return mix(
        mix(mix(grad(perm[o + aa], f.x, f.y, f.z), grad(perm[o + ba], f.x - 1.0, f.y, f.z), u),
            mix(grad(perm[o + ab], f.x, f.y - 1.0, f.z), grad(perm[o + bb], f.x - 1.0, f.y - 1.0, f.z), u), v),
        mix(mix(grad(perm[o + aa + 1u], f.x, f.y, f.z - 1.0), grad(perm[o + ba + 1u], f.x - 1.0, f.y, f.z - 1.0), u),
            mix(grad(perm[o + ab + 1u], f.x, f.y - 1.0, f.z - 1.0), grad(perm[o + bb + 1u], f.x - 1.0, f.y - 1.0, f.z - 1.0), u), v),
        w);
}

fn call(jid: u32, t: u32, k: u32, local: vec3<f32>) -> f32 {
    let c = jobs[jid].calls[k];
    return perlin(t, jobs[jid].cells[k].xyz, c.xyz + local * c.w);
}

fn hash3i(x: u32, y: u32, z: u32, salt: u32) -> u32 {
    var h = (x * 0x27d4eb2du) ^ (y * 0x165667b1u) ^ (z * 0x1b873593u) ^ salt;
    h = (h ^ (h >> 15u)) * 0x85ebca6bu;
    h = (h ^ (h >> 13u)) * 0xc2b2ae35u;
    return h ^ (h >> 16u);
}

fn rnd(s: ptr<function, u32>) -> f32 {
    *s = *s + 0x6d2b79f5u;
    var t = *s;
    t = (t ^ (t >> 15u)) * (t | 1u);
    t = t ^ (t + (t ^ (t >> 7u)) * (t | 61u));
    return f32((t ^ (t >> 14u)) >> 8u) / 16777216.0;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return b + (a - b) * h - k * h * (1.0 - h);
}

fn crater_profile(r: f32, radius: f32, age: f32) -> f32 {
    let fl = -0.42 + 0.18 * age;
    let width = 0.55 + 0.3 * age;
    let k = 0.16 + 0.45 * age;
    let rim_x = min(r - 1.0 - width, 0.0);
    var shape = smin(r * r - 1.0, 0.42 * rim_x * rim_x, k);
    shape = -smin(-shape, -fl, min(k, -fl * 0.9));
    let blanket = select(1.0, exp(-(r - 1.0) * 2.2) * (1.0 - smoothstep(1.6, 2.2, r)), r > 1.0);
    shape += 0.03 * blanket * (1.0 - age);
    return shape * radius * 0.85 * (1.0 - 0.72 * age);
}

/// Height (m over the datum sphere) and albedo at a point of job `jid`'s node: `d` its direction
/// minus the centre's, (da, db) its parameter offsets.
/// Also returns (z) the part the parent node leaves out (octaves and crater layers finer than its
/// cell): the child blends it away as it morphs into the parent, so LOD borders meet exactly.
fn sample(jid: u32, d: vec3<f32>, da: f32, db: f32) -> vec3<f32> {
    return sample_n(jid, d, da, db, jobs[jid].counts.xyz);
}

/// `sample` with `counts` (relief octaves, mountain octaves, crater layers): fewer for coarse relief.
fn sample_n(jid: u32, d: vec3<f32>, da: f32, db: f32, counts: vec3<u32>) -> vec3<f32> {
    let node = jobs[jid].node;
    let r = su.a.x;
    let local = d * r;
    let pn = normalize(node.dir.xyz + d);
    var h = su.a.y;
    h += su.a.z * (call(jid, 1u, 0u, local) + 0.45 * call(jid, 1u, 1u, local));
    let m = call(jid, 2u, 2u, local) + 0.5 * call(jid, 2u, 3u, local);
    let mare = smoothstep(su.b.x, su.b.y, m);
    h -= su.a.w * mare;
    let belt = smoothstep(-0.25, 0.55, call(jid, 1u, 4u, local));
    var amp = su.b.z * belt * (1.0 - 0.85 * mare);
    let pc = jobs[jid].counts.w;
    let parent = vec3<u32>(pc & 255u, (pc >> 8u) & 255u, pc >> 16u);
    var fine = 0.0;
    for (var i = 0u; i < counts.y; i++) {
        let ridge = 1.0 - abs(call(jid, 0u, 5u + i, local));
        let t = amp * ridge * ridge * ridge;
        h += t;
        fine += select(0.0, t, i >= parent.y);
        amp *= su.b.w;
    }
    amp = su.c.x * (1.0 - su.c.z * mare);
    for (var i = 0u; i < counts.x; i++) {
        let t = amp * call(jid, 0u, 8u + i, local);
        h += t;
        fine += select(0.0, t, i >= parent.x);
        amp *= su.c.y;
    }
    var albedo = su.d.x + (su.d.y - su.d.x) * mare;
    // craters, every scale: the cells around the point on each face it can touch
    let face_own = node.ids.x;
    let split = jobs[jid].split;
    let fa_own = (split.x + da) + split.y;
    let fb_own = (split.z + db) + split.w;
    let salt = su.e.x;
    for (var li = 0u; li < counts.z; li++) {
        let l0 = su.layers[li * 2u];
        let l1 = su.layers[li * 2u + 1u];
        // l1.w: the layer's depth scale (0 = switched off)
        if l1.w <= 0.0 {
            continue;
        }
        let cells = l0.x;
        let n = i32(cells);
        let p = l0.w * (1.0 - (1.0 - l1.x) * mare);
        let band = su.d.w * l0.w;
        for (var face = 0u; face < 6u; face++) {
            var fa = fa_own;
            var fb = fb_own;
            if face != face_own {
                let dn = dot(pn, FACE_N[face]);
                if dn <= 1e-6 {
                    continue;
                }
                fa = atan(dot(pn, FACE_U[face]) / dn) / PI4;
                fb = atan(dot(pn, FACE_V[face]) / dn) / PI4;
            }
            let margin = l0.z;
            if fa < -1.0 - margin || fa > 1.0 + margin || fb < -1.0 - margin || fb > 1.0 + margin {
                continue;
            }
            let ci = i32(floor((fa + 1.0) * 0.5 * cells));
            let cj = i32(floor((fb + 1.0) * 0.5 * cells));
            for (var j = max(cj - 1, 0); j <= min(cj + 1, n - 1); j++) {
                for (var i = max(ci - 1, 0); i <= min(ci + 1, n - 1); i++) {
                    var st = hash3i(u32(i), u32(j), face + li * 8u, salt);
                    // twin of core::surface::crater_presence: fade in, never a line of cliffs
                    let x = rnd(&st);
                    let presence = select(select(1.0, 0.0, x > p), clamp((p - x) / band, 0.0, 1.0), band > 0.0);
                    if presence <= 0.0 {
                        continue;
                    }
                    let ra = rnd(&st);
                    let rb = rnd(&st);
                    var e: vec3<f32>;
                    var ta: f32;
                    var tb: f32;
                    if face == face_own {
                        // crater centre relative to the node's centre: no absolute coordinates
                        let qa = ((f32(i) * 2.0 / cells - 1.0) - split.x) - split.y + ra * 2.0 / cells;
                        let qb = ((f32(j) * 2.0 / cells - 1.0) - split.z) - split.w + rb * 2.0 / cells;
                        let dq = node_delta(face, node.frame0, node.frame1, node.dir, qa, qb);
                        e = (d - dq.d) * r;
                        ta = dq.ab.x;
                        tb = dq.ab.y;
                    } else {
                        ta = tan(((f32(i) + ra) / cells * 2.0 - 1.0) * PI4);
                        tb = tan(((f32(j) + rb) / cells * 2.0 - 1.0) * PI4);
                        let q = normalize(FACE_N[face] + ta * FACE_U[face] + tb * FACE_V[face]);
                        e = (pn - q) * r;
                    }
                    let s2 = 1.0 + ta * ta + tb * tb;
                    let lf = min((1.0 + ta * ta) * sqrt(1.0 + tb * tb), (1.0 + tb * tb) * sqrt(1.0 + ta * ta)) / s2;
                    let u = rnd(&st);
                    let radius = (l1.y + (l1.z - l1.y) * u * u) * l0.y * lf;
                    let age = rnd(&st);
                    let w1 = rnd(&st);
                    let w2 = rnd(&st);
                    let pk = rnd(&st);
                    let dl = length(e);
                    let amp = su.d.z * (0.03 + 0.05 * w1) * (1.0 - 0.75 * smoothstep(1500.0, 15000.0, radius));
                    if dl / radius > 2.2 * (1.0 + 1.5 * amp) {
                        continue;
                    }
                    let nn = select(vec3<f32>(0.0), e / dl, dl > 0.0);
                    let wob = amp * (sin(nn.x * 4.1 + nn.y * 5.3 - nn.z * 3.7 + w2 * 6.283) + 0.5 * sin(-nn.x * 7.9 + nn.y * 6.1 + nn.z * 8.3 + w1 * 6.283));
                    let rr = dl / radius * (1.0 + wob);
                    if rr > 2.2 {
                        continue;
                    }
                    let k = select(pow(su.c.w / radius, 0.85), 1.0, radius < su.c.w);
                    let cf = su.c.w;
                    let hp = min(0.6 * pow(radius * 2e-3, 1.97), 0.5 * 0.36 * radius * k);
                    let q = rr / 0.2;
                    let peak = select(0.0, su.d.z * smoothstep(0.5 * cf, cf, radius) * hp * (1.0 - 0.6 * age) * exp(-q * q), pk < 0.85 && radius > 0.5 * cf);
                    let t = (crater_profile(rr, radius, age) * k + peak) * l1.w * presence;
                    h += t;
                    fine += select(0.0, t, li >= parent.z);
                    if age < 0.15 {
                        albedo += l1.w * presence * 0.3 * (1.0 - age / 0.15) * exp(-(rr - 1.0) * (rr - 1.0) * 1.6) * (1.0 - smoothstep(1.6, 2.2, rr));
                    }
                }
            }
        }
    }
    // blast craters dug at run time (core::deform), on top of everything
    for (var bi = 0u; bi < jobs[jid].blasts_n.x; bi++) {
        let b0 = jobs[jid].blasts[bi * 2u];
        let b1 = jobs[jid].blasts[bi * 2u + 1u];
        let v = d - b0.xyz;
        let len = length(v);
        let rr = len * r / b0.w;
        if rr < BLAST_REACH * (1.0 + BLAST_WOBBLE) {
            let kept = b1.w >= 1.5;
            let pat = blast_pattern(v / max(len, 1e-12), fract(b1.w));
            let e = blast_erase(rr, pat.x);
            let before = h;
            // the old ground levelled where it wipes, then the bowl, rim and blanket
            h = mix(h, b1.z, e) + b1.x * blast_profile(rr, b1.y, pat.x, pat.y);
            // what the parent leaves out: if it draws this crater too, only what is left of the
            // detail it lacks; if not, all this crater changed
            fine = select(fine + (h - before), fine * (1.0 - e), kept);
            // fresh regolith thrown out is brighter than the weathered surface, most along its rays
            let fresh = 1.0 - smoothstep(0.7, BLAST_REACH, rr);
            let rayed = pat.z * smoothstep(1.0, 1.3, rr) * (1.0 - smoothstep(2.0, BLAST_REACH, rr));
            albedo += 0.12 * fresh + 0.22 * rayed;
        }
    }
    let fades = jobs[jid].fades;
    albedo = clamp(albedo * (1.0 + 0.05 * fades.y * call(jid, 0u, 23u, local) + 0.025 * fades.z * call(jid, 0u, 24u, local)), 0.5, 1.6);
    return vec3<f32>(h, albedo, fine);
}

@compute @workgroup_size(8, 8, 1)
fn gen_heights(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = su.e.y;
    let b = su.e.z;
    let w = n + 1u + 2u * b;
    if gid.x >= w || gid.y >= w {
        return;
    }
    let jid = gid.z;
    let node = jobs[jid].node;
    let half = node.info.x;
    let da = ((f32(i32(gid.x) - i32(b)) / f32(n)) * 2.0 - 1.0) * half;
    let db = ((f32(i32(gid.y) - i32(b)) / f32(n)) * 2.0 - 1.0) * half;
    let dl = node_delta(node.ids.x, node.frame0, node.frame1, node.dir, da, db);
    let s = sample(jid, dl.d, da, db);
    let up = normalize(node.dir.xyz + dl.d);
    let at = jid * w * w + gid.y * w + gid.x;
    scratch_pos[at] = vec4<f32>(dl.d * su.a.x + up * s.x, s.x);
    scratch_alb[at] = s.yz;
}

@compute @workgroup_size(8, 8, 1)
fn gen_finish(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = su.e.y;
    let b = su.e.z;
    let w = n + 1u + 2u * b;
    if gid.x > n || gid.y > n {
        return;
    }
    let jid = gid.z;
    let node = jobs[jid].node;
    let base = jid * w * w;
    let x = gid.x + b;
    let y = gid.y + b;
    let at = base + y * w + x;
    let c = scratch_pos[at];
    let tx = scratch_pos[at + 1u].xyz - scratch_pos[at - 1u].xyz;
    let ty = scratch_pos[at + w].xyz - scratch_pos[at - w].xyz;
    let nrm = normalize(cross(tx, ty));
    var vis = 1.0;
    if su.e.w != 0u {
        // the far horizon (gen_horizon), interpolated between its subgrid points
        let hs = vec2<f32>(gid.xy) * f32(HOR - 1u) / f32(n);
        let h0 = min(vec2<u32>(hs), vec2<u32>(HOR - 2u));
        let hf = hs - vec2<f32>(h0);
        let hb = jid * HOR * HOR + h0.y * HOR + h0.x;
        let far = mix(mix(scratch_hor[hb], scratch_hor[hb + 1u], hf.x), mix(scratch_hor[hb + HOR], scratch_hor[hb + HOR + 1u], hf.x), hf.y);
        // march toward the sun over this node's grid (and its border): the highest elevation seen
        let up = normalize(node.dir.xyz * su.a.x + c.xyz);
        let sun = su.sun.xyz;
        let st = sun - up * dot(sun, up);
        var g = vec2<f32>(dot(st, tx) / max(dot(tx, tx), 1e-12), dot(st, ty) / max(dot(ty, ty), 1e-12));
        if dot(g, g) > 1e-12 {
            g = normalize(g);
            var best = far;
            var dist = 1.0;
            for (var k = 0; k < 24; k++) {
                let q = vec2<f32>(f32(x), f32(y)) + g * dist;
                if any(q < vec2<f32>(0.0)) || any(q > vec2<f32>(f32(w - 1u))) {
                    break;
                }
                let s = scratch_pos[base + u32(q.y + 0.5) * w + u32(q.x + 0.5)].xyz;
                let dv = s - c.xyz;
                best = max(best, dot(dv, up) / max(length(dv), 1e-3));
                dist = dist * 1.3 + 1.0;
            }
            vis = smoothstep(-0.02, 0.02, dot(sun, up) - best);
        }
    }
    let layer = node.ids.y;
    let af = scratch_alb[at];
    textureStore(height_out, vec2<u32>(gid.xy), layer, vec4<u32>(bitcast<u32>(c.w), pack2x16float(vec2<f32>(vis, af.y)), 0u, 0u));
    textureStore(normal_out, vec2<u32>(gid.xy), layer, vec4<f32>(nrm, af.x));
}

/// Far horizon toward the sun from a 9x9 subgrid of the node: marches far past the node's border
/// over coarser relief (a few octaves and crater layers less), so a crater rim kilometres away
/// still shades the ground whatever the node's size; gen_finish takes the higher of this and its
/// own near march.
@compute @workgroup_size(9, 9, 1)
fn gen_horizon(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = su.e.y;
    let b = su.e.z;
    let w = n + 1u + 2u * b;
    let jid = gid.z;
    let node = jobs[jid].node;
    let gx = gid.x * n / (HOR - 1u);
    let gy = gid.y * n / (HOR - 1u);
    let base = jid * w * w;
    let at = base + (gy + b) * w + gx + b;
    let c = scratch_pos[at];
    let tx = scratch_pos[at + 1u].xyz - scratch_pos[at - 1u].xyz;
    let ty = scratch_pos[at + w].xyz - scratch_pos[at - w].xyz;
    let up = normalize(node.dir.xyz * su.a.x + c.xyz);
    let sun = su.sun.xyz;
    let st = sun - up * dot(sun, up);
    var best = -1.0;
    var g = vec2<f32>(dot(st, tx) / max(dot(tx, tx), 1e-12), dot(st, ty) / max(dot(ty, ty), 1e-12));
    if dot(g, g) > 1e-12 && dot(sun, up) > -0.2 {
        g = normalize(g);
        let half = node.info.x;
        let cell = 2.0 * half / f32(n);
        let da0 = (f32(gx) / f32(n) * 2.0 - 1.0) * half;
        let db0 = (f32(gy) / f32(n) * 2.0 - 1.0) * half;
        let fc = jobs[jid].blasts_n.y;
        let counts = vec3<u32>(fc & 255u, (fc >> 8u) & 255u, fc >> 16u);
        let split = jobs[jid].split;
        var dist = 6.0;
        for (var k = 0; k < 12; k++) {
            let da = da0 + g.x * dist * cell;
            let db = db0 + g.y * dist * cell;
            // past ~67 degrees off the face's centre the face's warp runs away: stop there
            if abs((split.x + da) + split.y) > 1.5 || abs((split.z + db) + split.w) > 1.5 {
                break;
            }
            let dl = node_delta(node.ids.x, node.frame0, node.frame1, node.dir, da, db);
            let s = sample_n(jid, dl.d, da, db, counts);
            let p = dl.d * su.a.x + normalize(node.dir.xyz + dl.d) * s.x;
            let dv = p - c.xyz;
            best = max(best, dot(dv, up) / max(length(dv), 1e-3));
            dist *= 1.55;
        }
    }
    scratch_hor[jid * HOR * HOR + gid.y * HOR + gid.x] = best;
}
