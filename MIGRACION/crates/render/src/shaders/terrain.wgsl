// Terrain draw: one shared grid, one instance per visible quadtree node. Needs common.wgsl and
// terrain_common.wgsl. Vertices come from the node's height texture (GPU-generated), CDLOD morph
// toward the parent grid with distance, skirts hang below the borders.

struct Visible {
    center: vec3<f32>,   // node centre on the datum sphere, camera-relative
    slot: u32,
    morph: vec2<f32>,    // morph start, end (m from the camera), from this frame's split
    pad: vec2<f32>,
};

@group(1) @binding(0) var<storage, read> nodes: array<Node>;
@group(1) @binding(1) var<storage, read> visible: array<Visible>;
@group(1) @binding(2) var heights: texture_2d_array<u32>;
@group(1) @binding(3) var normals: texture_2d_array<f32>;
@group(1) @binding(4) var detail: texture_2d<f32>;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tex: vec2<f32>,
    @location(3) vis: f32,
    @location(4) @interpolate(flat) layer_face: vec2<u32>,
};

struct GridPoint {
    pos: vec3<f32>,
    vis: f32,
};

/// `k`: how far toward the parent's surface (0 this node's, 1 the parent's heights).
fn grid_point(node: Node, i: u32, j: u32, k: f32) -> GridPoint {
    let n = f32(node.ids.w);
    let half = node.info.x;
    let da = (f32(i) / n * 2.0 - 1.0) * half;
    let db = (f32(j) / n * 2.0 - 1.0) * half;
    let dl = node_delta(node.ids.x, node.frame0, node.frame1, node.dir, da, db);
    let raw = textureLoad(heights, vec2<u32>(i, j), node.ids.y, 0).xy;
    let vf = unpack2x16float(raw.y);
    let up = normalize(node.dir.xyz + dl.d);
    var g: GridPoint;
    g.pos = dl.d * node.info.y + up * (bitcast<f32>(raw.x) - vf.y * k);
    g.vis = vf.x;
    return g;
}

fn terrain_vertex(vid: u32, iid: u32) -> VOut {
    let v = visible[iid];
    let node = nodes[v.slot];
    let n = node.ids.w;
    let row = n + 1u;
    var i: u32;
    var j: u32;
    var skirt = false;
    if vid < row * row {
        i = vid % row;
        j = vid / row;
    } else {
        let k = vid - row * row;
        let t = k % row;
        skirt = true;
        switch k / row {
            case 0u: { i = t; j = 0u; }
            case 1u: { i = n; j = t; }
            case 2u: { i = n - t; j = n; }
            default: { i = 0u; j = n - t; }
        }
    }
    // CDLOD: with distance the node turns into its parent: odd vertices slide onto the parent's
    // edges and every height loses the detail the parent leaves out, so LOD borders meet exactly
    let k = clamp((length(v.center + grid_point(node, i, j, 0.0).pos) - v.morph.x) / (v.morph.y - v.morph.x), 0.0, 1.0);
    let g = grid_point(node, i, j, k);
    var pos = g.pos;
    var vis = g.vis;
    var cell = vec2<f32>(f32(i), f32(j));
    let odd = vec2<bool>((i & 1u) == 1u, (j & 1u) == 1u);
    if odd.x || odd.y {
        if k > 0.0 {
            var a: vec2<u32>;
            var b: vec2<u32>;
            if odd.x && odd.y {
                a = vec2<u32>(i + 1u, j - 1u);
                b = vec2<u32>(i - 1u, j + 1u);
            } else if odd.x {
                a = vec2<u32>(i - 1u, j);
                b = vec2<u32>(i + 1u, j);
            } else {
                a = vec2<u32>(i, j - 1u);
                b = vec2<u32>(i, j + 1u);
            }
            let ga = grid_point(node, a.x, a.y, 1.0);
            let gb = grid_point(node, b.x, b.y, 1.0);
            pos = mix(pos, (ga.pos + gb.pos) * 0.5, k);
            vis = mix(vis, (ga.vis + gb.vis) * 0.5, k);
            cell = mix(cell, (vec2<f32>(a) + vec2<f32>(b)) * 0.5, k);
        }
    }
    if skirt {
        pos -= normalize(node.dir.xyz * node.info.y + pos) * node.tex.w;
    }
    let rel = v.center + pos;
    var o: VOut;
    o.clip = pass_u.view_proj * vec4<f32>(rel, 1.0);
    o.rel = rel;
    o.uv = (cell + 0.5) / f32(row);
    o.tex = node.tex.xy + cell / f32(n) * (2.0 * node.info.x) * node.tex.z;
    o.vis = vis;
    o.layer_face = vec2<u32>(node.ids.y, node.ids.x);
    return o;
}

@vertex
fn terrain_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VOut {
    return terrain_vertex(vid, iid);
}

@vertex
fn terrain_depth_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> @builtin(position) vec4<f32> {
    return terrain_vertex(vid, iid).clip;
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn vnoise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash2(i), hash2(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash2(i + vec2<f32>(0.0, 1.0)), hash2(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

// No visible repetition: a slow noise picks one of eight offsets of the tile per region and the
// neighbouring pick is blended in (Quilez's texture-repetition trick); gradients stay continuous.
fn detail_layer(uv: vec2<f32>) -> vec4<f32> {
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let k = vnoise2(uv * 0.35) * 8.0;
    let ia = floor(k);
    let f = fract(k);
    let oa = sin(vec2<f32>(3.0, 7.0) * ia);
    let ob = sin(vec2<f32>(3.0, 7.0) * (ia + 1.0));
    let a = textureSampleGrad(detail, linear_repeat, uv + oa, dx, dy);
    let b = textureSampleGrad(detail, linear_repeat, uv + ob, dx, dy);
    let t = mix(a, b, smoothstep(0.2, 0.8, f - 0.1 * (a.z - b.z)));
    return vec4<f32>(t.xy * 2.0 - 1.0, t.z * 2.0, t.w);
}

@fragment
fn terrain_fs(in: VOut) -> @location(0) vec4<f32> {
    let nt = textureSample(normals, linear_clamp, in.uv, in.layer_face.x);
    var n = normalize(nt.xyz);
    let v = normalize(-in.rel);
    let dist = length(in.rel);
    var albedo = nt.w;
    var cavity = 1.0;
    let layers = u32(frame.params2.w);
    if layers > 0u {
        // detail textures on face metres: 4 m tiles near, 32 m tiles everywhere
        let face = in.layer_face.y;
        let t = normalize(FACE_U[face] - n * dot(FACE_U[face], n));
        let b = cross(n, t);
        let coarse = detail_layer(in.tex / 32.0 + vec2<f32>(0.37, 0.11));
        var d = coarse.xy * 0.21;
        albedo *= mix(1.0, coarse.z, 0.3);
        cavity = mix(1.0, coarse.w, 0.3);
        if layers > 1u {
            let near = 1.0 - smoothstep(30.0, 90.0, dist);
            let fine = detail_layer(in.tex / 4.0);
            d += fine.xy * near * 0.36;
            albedo *= mix(1.0, fine.z, 0.36 * near);
            cavity *= mix(1.0, fine.w, 0.6 * near);
        }
        n = normalize(n + t * d.x + b * d.y);
    }
    var s = in.vis;
    let csm = sun_shadow(in.rel, n, in.clip.xy);
    if csm >= 0.0 {
        // near the camera the cascades hold the ground's own shadows sharply; the coarse baked
        // visibility only adds far casters, faded in toward the end of the cascades
        let reach = frame.cascade_far[u32(frame.params.z) - 1u];
        let trust = 1.0 - smoothstep(0.25 * reach, 0.9 * reach, length(in.rel));
        s = min(mix(s, 1.0, trust), csm);
    }
    let base = vec3<f32>(0.135, 0.132, 0.128) * albedo;
    let c = regolith(base, n, v, s * cavity * body_light(in.rel)) + base * outside_light(in.rel, n) * cavity;
    return vec4<f32>(c, 1.0);
}
