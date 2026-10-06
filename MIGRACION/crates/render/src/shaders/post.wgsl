// Post: bloom (downsample chain + tent upsample), ACES tone mapping with the dynamic-resolution
// upscale, FXAA. Self-contained (no common.wgsl).

struct PostU {
    src: vec4<f32>,      // uv scale of the rendered area (x, y), texel of the source (x, y)
    params: vec4<f32>,   // exposure, bloom strength, bloom on, -
    // the helmet's visor the picture is seen through (post.rs `Visor`; all ones and zeros: none)
    tint: vec4<f32>,     // what all of it lets through (rgb; 1 clear), breath on it (0..1)
    sun: vec4<f32>,      // what its sun visor lets through (rgb), how far down it is (0 up .. 1 down)
    visor: vec4<f32>,    // the glare left under the sun visor (x the bloom), the picture's width over its height, how hard its filter presses down what is bright, and its sun visor
};

@group(0) @binding(0) var<uniform> post: PostU;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;
@group(0) @binding(3) var bloom: texture_2d<f32>;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn fullscreen(@builtin(vertex_index) vid: u32) -> VOut {
    let uv = vec2<f32>(f32((vid << 1u) & 2u), f32(vid & 2u));
    var o: VOut;
    o.clip = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return o;
}

fn tap(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(src, smp, uv, 0.0).rgb;
}

// 13-tap downsample (Jimenez 2014), weights sum to 1
fn down13(uv: vec2<f32>, t: vec2<f32>) -> vec3<f32> {
    let a = tap(uv + t * vec2(-2.0, -2.0));
    let b = tap(uv + t * vec2(0.0, -2.0));
    let c = tap(uv + t * vec2(2.0, -2.0));
    let d = tap(uv + t * vec2(-2.0, 0.0));
    let e = tap(uv);
    let f = tap(uv + t * vec2(2.0, 0.0));
    let g = tap(uv + t * vec2(-2.0, 2.0));
    let h = tap(uv + t * vec2(0.0, 2.0));
    let i = tap(uv + t * vec2(2.0, 2.0));
    let j = tap(uv + t * vec2(-1.0, -1.0));
    let k = tap(uv + t * vec2(1.0, -1.0));
    let l = tap(uv + t * vec2(-1.0, 1.0));
    let m = tap(uv + t * vec2(1.0, 1.0));
    return e * 0.125 + (a + c + g + i) * 0.03125 + (b + d + f + h) * 0.0625 + (j + k + l + m) * 0.125;
}

/// What the visor lets through at `uv` (the whole picture, y down): its filter all over and its
/// sun visor from the top down to where it has been lowered (rgb), and how much of the sun visor
/// is over that point (a).
fn visor_shade(uv: vec2<f32>) -> vec4<f32> {
    var gain = post.tint.rgb;
    var under = 0.0;
    if post.sun.w > 0.0 {
        // its lower edge: a soft line a little lower in the middle, coming down from over the
        // picture to under it
        let x = uv.x - 0.5;
        let edge = post.sun.w * 1.3 - 0.12 - x * x * 0.22;
        under = 1.0 - smoothstep(edge - 0.03, edge + 0.03, uv.y);
        gain *= mix(vec3<f32>(1.0), post.sun.rgb, under);
    }
    return vec4<f32>(gain, under);
}

/// Light `c` through the visor's filter and, where it is down (`under`), its sun visor, which
/// press down what is bright more than what is dim: an arc is seen, not a white blot.
fn visor_pressed(c: vec3<f32>, under: f32) -> vec3<f32> {
    let k = post.visor.z + post.visor.w * under;
    return c / (1.0 + k * max(c.r, max(c.g, c.b)));
}

fn visor_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let h = vec4<f32>(dot(i, vec2<f32>(127.1, 311.7)), dot(i + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7)), dot(i + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7)), dot(i + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7)));
    let r = fract(sin(h) * 43758.5453);
    return mix(mix(r.x, r.y, u.x), mix(r.z, r.w, u.x), u.y);
}

/// Breath on the visor at `uv` (0 none .. 1): mist from its rim inward, most of it low (where
/// the breath goes), its edge ragged.
fn visor_mist(uv: vec2<f32>) -> f32 {
    let amount = post.tint.w;
    if amount <= 0.0 {
        return 0.0;
    }
    let p = (uv - vec2<f32>(0.5, 0.43)) * 2.0;
    let wide = vec2<f32>(post.visor.y, 1.0);
    let ragged = visor_noise(uv * wide * 5.0) * 0.16 + visor_noise(uv * wide * 23.0) * 0.05;
    let edge0 = mix(1.5, 0.82, amount);
    return smoothstep(edge0, edge0 + 0.45, length(p) + ragged) * sqrt(amount) * 0.8;
}

@fragment
fn bloom_first(in: VOut) -> @location(0) vec4<f32> {
    // from the HDR frame's rendered area, keeping only what is well above white (through the
    // visor: what it dims glares less)
    let shade = visor_shade(in.uv);
    let c = visor_pressed(down13(in.uv * post.src.xy, post.src.zw) * post.params.x * shade.rgb, shade.a);
    let l = max(c.r, max(c.g, c.b));
    let k = max(l - 1.0, 0.0) / max(l, 1e-4);
    return vec4<f32>(min(c * k, vec3<f32>(200.0)), 1.0);
}

@fragment
fn bloom_down(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(down13(in.uv, post.src.zw), 1.0);
}

@fragment
fn bloom_up(in: VOut) -> @location(0) vec4<f32> {
    let t = post.src.zw;
    var s = tap(in.uv) * 4.0;
    s += (tap(in.uv + vec2(-t.x, 0.0)) + tap(in.uv + vec2(t.x, 0.0)) + tap(in.uv + vec2(0.0, -t.y)) + tap(in.uv + vec2(0.0, t.y))) * 2.0;
    s += tap(in.uv - t) + tap(in.uv + t) + tap(in.uv + vec2(t.x, -t.y)) + tap(in.uv + vec2(-t.x, t.y));
    return vec4<f32>(s / 16.0, 1.0);
}

fn aces(c: vec3<f32>) -> vec3<f32> {
    return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn graded(uv: vec2<f32>) -> vec3<f32> {
    var c = textureSampleLevel(src, smp, uv * post.src.xy, 0.0).rgb;
    let mist = visor_mist(uv);
    if mist > 0.002 {
        // through the mist: what is round it, spread, and a veil of the light that falls on it
        let at = uv * post.src.xy;
        let t = post.src.zw * 11.0;
        let top = post.src.xy - post.src.zw;
        let spread = (tap(min(at + vec2<f32>(t.x, 0.0), top)) + tap(max(at - vec2<f32>(t.x, 0.0), vec2<f32>(0.0))) + tap(min(at + vec2<f32>(0.0, t.y), top)) + tap(max(at - vec2<f32>(0.0, t.y), vec2<f32>(0.0)))) * 0.25;
        c = mix(c, spread, mist * 0.8) + (dot(spread, vec3<f32>(0.3, 0.5, 0.2)) * 0.35 + 0.0015) * mist;
    }
    let shade = visor_shade(uv);
    c = visor_pressed(c * post.params.x * shade.rgb, shade.a);
    if post.params.z > 0.0 {
        c += textureSampleLevel(bloom, smp, uv, 0.0).rgb * post.params.y * mix(1.0, post.visor.x, shade.a);
    }
    return aces(c);
}

@fragment
fn tonemap_direct(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(graded(in.uv), 1.0);
}

@fragment
fn tonemap_ldr(in: VOut) -> @location(0) vec4<f32> {
    let g = sqrt(graded(in.uv));
    return vec4<f32>(g, dot(g, vec3<f32>(0.299, 0.587, 0.114)));
}

// FXAA (quality preset ~12) on the gamma image; luma in alpha. Writes linear to an sRGB target.
@fragment
fn fxaa(in: VOut) -> @location(0) vec4<f32> {
    let t = post.src.zw;
    let uv = in.uv;
    let m = textureSampleLevel(src, smp, uv, 0.0);
    let n = textureSampleLevel(src, smp, uv + vec2(0.0, -t.y), 0.0).a;
    let s = textureSampleLevel(src, smp, uv + vec2(0.0, t.y), 0.0).a;
    let e = textureSampleLevel(src, smp, uv + vec2(t.x, 0.0), 0.0).a;
    let w = textureSampleLevel(src, smp, uv + vec2(-t.x, 0.0), 0.0).a;
    let lmin = min(m.a, min(min(n, s), min(e, w)));
    let lmax = max(m.a, max(max(n, s), max(e, w)));
    let range = lmax - lmin;
    if range < max(0.0312, lmax * 0.125) {
        return vec4<f32>(m.rgb * m.rgb, 1.0);
    }
    let nw = textureSampleLevel(src, smp, uv + vec2(-t.x, -t.y), 0.0).a;
    let ne = textureSampleLevel(src, smp, uv + vec2(t.x, -t.y), 0.0).a;
    let sw = textureSampleLevel(src, smp, uv + vec2(-t.x, t.y), 0.0).a;
    let se = textureSampleLevel(src, smp, uv + vec2(t.x, t.y), 0.0).a;
    let horizontal = abs(n + s - 2.0 * m.a) * 2.0 + abs(ne + se - 2.0 * e) + abs(nw + sw - 2.0 * w)
        >= abs(e + w - 2.0 * m.a) * 2.0 + abs(ne + nw - 2.0 * n) + abs(se + sw - 2.0 * s);
    let p1 = select(e, s, horizontal);
    let n1 = select(w, n, horizontal);
    let g1 = abs(p1 - m.a);
    let g2 = abs(n1 - m.a);
    var step = select(t.x, t.y, horizontal);
    var grad = g1;
    var edge_luma = (p1 + m.a) * 0.5;
    if g2 > g1 {
        step = -step;
        grad = g2;
        edge_luma = (n1 + m.a) * 0.5;
    }
    let along = select(vec2<f32>(0.0, t.y), vec2<f32>(t.x, 0.0), horizontal);
    var uv_edge = uv + select(vec2<f32>(step * 0.5, 0.0), vec2<f32>(0.0, step * 0.5), horizontal);
    let scaled = grad * 0.25;
    var up = uv_edge + along;
    var dn = uv_edge - along;
    var lu = textureSampleLevel(src, smp, up, 0.0).a - edge_luma;
    var ld = textureSampleLevel(src, smp, dn, 0.0).a - edge_luma;
    var done_u = abs(lu) >= scaled;
    var done_d = abs(ld) >= scaled;
    let steps = array<f32, 8>(1.0, 1.5, 2.0, 2.0, 2.0, 4.0, 8.0, 8.0);
    for (var i = 0; i < 8; i++) {
        if done_u && done_d {
            break;
        }
        if !done_u {
            up += along * steps[i];
            lu = textureSampleLevel(src, smp, up, 0.0).a - edge_luma;
            done_u = abs(lu) >= scaled;
        }
        if !done_d {
            dn -= along * steps[i];
            ld = textureSampleLevel(src, smp, dn, 0.0).a - edge_luma;
            done_d = abs(ld) >= scaled;
        }
    }
    let du = select(up.y - uv.y, up.x - uv.x, horizontal);
    let dd = select(uv.y - dn.y, uv.x - dn.x, horizontal);
    let closer_up = du < dd;
    let span = du + dd;
    let d = min(du, dd);
    let below = m.a < edge_luma;
    let good = select((ld < 0.0) != below, (lu < 0.0) != below, closer_up);
    var offset = select(0.0, 0.5 - d / span, good);
    // sub-pixel smoothing
    let avg = (2.0 * (n + s + e + w) + nw + ne + sw + se) / 12.0;
    let sub = clamp(abs(avg - m.a) / range, 0.0, 1.0);
    let sub2 = (-2.0 * sub + 3.0) * sub * sub;
    offset = max(offset, sub2 * sub2 * 0.75);
    let final_uv = uv + select(vec2<f32>(offset * step, 0.0), vec2<f32>(0.0, offset * step), horizontal);
    let c = textureSampleLevel(src, smp, final_uv, 0.0).rgb;
    return vec4<f32>(c * c, 1.0);
}
