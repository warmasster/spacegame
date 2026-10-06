// Shared by every scene pipeline: the frame, the pass, shadows and lighting.
// Space: camera-relative (the camera sits at the origin), world axes. Depth: reversed Z, infinite far.

struct Frame {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    cascades: array<mat4x4<f32>, 4>,
    cascade_far: vec4<f32>,
    cam_forward: vec4<f32>,      // w: tan(fov/2)
    origin: vec4<f32>,           // render origin - camera; w: time (s)
    prev_origin: vec4<f32>,      // same, previous frame
    sun_dir: vec4<f32>,          // toward the sun; w: intensity
    sun_color: vec4<f32>,        // w: shadow sweep (m)
    backdrop: vec4<f32>,         // far planet in the sky: direction; w: angular radius (rad, 0 = none)
    up: vec4<f32>,               // local vertical at the camera; w: altitude over the datum
    body_center: vec4<f32>,      // body under the camera: centre - camera; w: radius
    viewport: vec4<f32>,         // w, h, 1/w, 1/h of the rendered area
    target_size: vec4<f32>,      // full target w, h; uv scale x, y
    params: vec4<f32>,           // exposure, shadow filter, cascades, shadow texel (world m at cascade 0)
    params2: vec4<f32>,          // frame index, lod bias, bloom, detail layers
    shadow: vec4<f32>,           // filter softness, cascade blend band (share of a cascade), flashes, -
    lights: array<vec4<f32>, 72>, // flashes and lamps: (camera-relative position, range m), (colour x intensity, 1 inside), (spot axis, cos of its cone; 0 point)
};

struct Pass {
    view_proj: mat4x4<f32>,
    info: vec4<u32>,             // x: 0 main, 1+ cascade index + 1
    light: vec4<f32>,            // shadow passes: depth bias (m), normal bias, -, -
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> pass_u: Pass;
@group(0) @binding(2) var shadow_map: texture_depth_2d_array;
@group(0) @binding(3) var shadow_cmp: sampler_comparison;
@group(0) @binding(4) var linear_repeat: sampler;
@group(0) @binding(5) var linear_clamp: sampler;

const PI: f32 = 3.14159265358979;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3<f32>(2.2));
}

fn quat_rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn oct_encode(n: vec3<f32>) -> vec2<f32> {
    let p = n.xy / (abs(n.x) + abs(n.y) + abs(n.z));
    if n.z >= 0.0 {
        return p;
    }
    return (1.0 - abs(p.yx)) * select(vec2<f32>(-1.0), vec2<f32>(1.0), p >= vec2<f32>(0.0));
}

fn oct_decode(e: vec2<f32>) -> vec3<f32> {
    var n = vec3<f32>(e, 1.0 - abs(e.x) - abs(e.y));
    let t = max(-n.z, 0.0);
    n.x += select(t, -t, n.x >= 0.0);
    n.y += select(t, -t, n.y >= 0.0);
    return normalize(n);
}

// ---- shadows ----

const POISSON: array<vec2<f32>, 12> = array<vec2<f32>, 12>(
    vec2(-0.326, -0.406), vec2(-0.840, -0.074), vec2(-0.696, 0.457), vec2(-0.203, 0.621),
    vec2(0.962, -0.195), vec2(0.473, -0.480), vec2(0.519, 0.767), vec2(0.185, -0.893),
    vec2(0.507, 0.064), vec2(0.896, 0.412), vec2(-0.322, -0.933), vec2(-0.792, -0.598)
);

fn cascade_sample(c: u32, p: vec3<f32>, spin: f32) -> f32 {
    let q = frame.cascades[c] * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
    let depth = q.z;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || depth > 1.0 {
        return -1.0;
    }
    let mode = u32(frame.params.y);
    let texel = 1.0 / f32(textureDimensions(shadow_map).x);
    let soft = frame.shadow.x;
    if mode == 0u {
        return textureSampleCompareLevel(shadow_map, shadow_cmp, uv, c, depth);
    }
    if mode == 1u {
        // 4 hardware-filtered taps
        var s = 0.0;
        for (var i = 0; i < 4; i++) {
            let o = vec2<f32>(f32(i & 1) - 0.5, f32(i >> 1u) - 0.5) * texel * soft;
            s += textureSampleCompareLevel(shadow_map, shadow_cmp, uv + o, c, depth);
        }
        return s * 0.25;
    }
    // rotated Poisson disc, wider on higher filters, scaled by the softness setting
    let taps = select(8, 12, mode >= 3u);
    let radius = texel * select(1.6, 2.4, mode >= 3u) * soft;
    let cs = vec2<f32>(cos(spin), sin(spin));
    var s = 0.0;
    for (var i = 0; i < taps; i++) {
        let d = POISSON[i];
        let o = vec2<f32>(d.x * cs.x - d.y * cs.y, d.x * cs.y + d.y * cs.x) * radius;
        s += textureSampleCompareLevel(shadow_map, shadow_cmp, uv + o, c, depth);
    }
    return s / f32(taps);
}

/// One cascade at camera-relative `p`, offset along the normal by a couple of its texels and
/// nudged toward the sun (no acne at grazing sun); -1 outside it.
fn cascade_at(c: u32, p: vec3<f32>, n: vec3<f32>, spin: f32) -> f32 {
    let texel = frame.params.w * frame.cascade_far[c] / frame.cascade_far[0];
    return cascade_sample(c, p + n * texel * 2.5 + frame.sun_dir.xyz * texel * 3.0, spin);
}

/// Sun visibility from the cascades at camera-relative `p`; -1 past their reach. The end of each
/// cascade cross-fades into the next (and the last one into full light): no seams, no pops.
fn sun_shadow(p: vec3<f32>, n: vec3<f32>, frag: vec2<f32>) -> f32 {
    let count = u32(frame.params.z);
    if count == 0u {
        return -1.0;
    }
    // cascades are spheres round the camera: chosen by distance
    let depth = length(p);
    let spin = hash12(frag) * 6.2831;
    for (var c = 0u; c < count; c++) {
        let far = frame.cascade_far[c];
        if depth < far {
            var s = cascade_at(c, p, n, spin);
            if s < 0.0 {
                continue;
            }
            let t = smoothstep(far * (1.0 - frame.shadow.y), far, depth);
            if t > 0.0 {
                var next = 1.0;
                if c + 1u < count {
                    next = cascade_at(c + 1u, p, n, spin);
                    if next < 0.0 {
                        next = s;
                    }
                }
                s = mix(s, next, t);
            }
            return s;
        }
    }
    return -1.0;
}

/// Sunlight the body under the camera leaves at camera-relative `p`: none on its night side (the
/// cascades only reach a few km, the planet itself casts no map), a narrow soft terminator. A
/// peak past the terminator keeps the last light, as on the real Moon.
fn body_light(p: vec3<f32>) -> f32 {
    let d = p - frame.body_center.xyz;
    let l = frame.sun_dir.xyz;
    let t = dot(d, l);
    if t >= 0.0 {
        return 1.0;
    }
    // closest approach of the ray toward the sun to the body's centre
    let miss = length(d - l * t);
    let r = frame.body_center.w;
    return smoothstep(r - 300.0, r + 300.0, miss);
}

/// Sun visibility for a lit surface at camera-relative `p`: the cascades (full light past them)
/// and the body's own shadow.
fn sun_light(p: vec3<f32>, n: vec3<f32>, frag: vec2<f32>) -> f32 {
    let b = body_light(p);
    if b <= 0.0 {
        return 0.0;
    }
    var s = sun_shadow(p, n, frag);
    if s < 0.0 {
        s = 1.0;
    }
    return s * b;
}

// ---- lighting ----

fn aces(c: vec3<f32>) -> vec3<f32> {
    return clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

/// Light bounced off the sunlit ground and the backdrop planet's glow, for surfaces facing `n`.
fn ambient(n: vec3<f32>) -> vec3<f32> {
    let up = frame.up.xyz;
    let sun = frame.sun_dir;
    // the ground around is lit at the sun's elevation: its glow reaches what faces down/sideways
    let lit_ground = max(dot(up, sun.xyz), 0.0) * sun.w * 0.12 * 0.45;
    let ground = lit_ground * clamp(0.5 - 0.5 * dot(n, up), 0.0, 1.0);
    // the backdrop planet's glow (none without one)
    let glow = vec3<f32>(0.10, 0.14, 0.22) * 0.02 * (0.6 + 0.4 * max(dot(n, frame.backdrop.xyz), 0.0)) * select(0.0, 1.0, frame.backdrop.w > 0.0);
    return vec3<f32>(ground) * vec3<f32>(1.0, 0.97, 0.92) + glow;
}

/// Light of one lamp or flash `i` on a surface at `p` (camera-relative) facing `n`: falls off with
/// distance, gone at its range; a spot only within its cone (soft edge).
fn lamp_light(i: u32, p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let l0 = frame.lights[i * 3u];
    let d = l0.xyz - p;
    let dist = length(d);
    if dist >= l0.w {
        return vec3<f32>(0.0);
    }
    let l = d / max(dist, 1e-3);
    let w = 1.0 - dist / l0.w;
    var k = max(dot(n, l), 0.0) * w * w / (1.0 + dist * dist * 0.25);
    let spot = frame.lights[i * 3u + 2u];
    if spot.w > 0.0 {
        k *= smoothstep(spot.w, mix(spot.w, 1.0, 0.35), dot(-l, spot.xyz));
    }
    return frame.lights[i * 3u + 1u].rgb * k;
}

/// Light the explosion flashes (core::effects) and the lamps of structures bring to a surface at
/// `p` (camera-relative) facing `n`.
fn flash_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var c = vec3<f32>(0.0);
    for (var i = 0u; i < u32(frame.shadow.z); i++) {
        c += lamp_light(i, p, n);
    }
    return c;
}

/// The same for the ground: lamps inside structures do not reach it through their floors (a
/// lamp carried about, w = 2, lights everything).
fn outside_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var c = vec3<f32>(0.0);
    for (var i = 0u; i < u32(frame.shadow.z); i++) {
        let side = frame.lights[i * 3u + 1u].w;
        if side < 0.5 || side > 1.5 {
            c += lamp_light(i, p, n);
        }
    }
    return c;
}

/// The same for a surface that may be inside a hull: what is in there is lit only by the lamps in
/// there (and the one carried about): the hull stands between it and the lamps and flashes
/// outside. What is outside takes every light (a plate's inner face turns its back on them).
fn side_light(p: vec3<f32>, n: vec3<f32>, inside: bool) -> vec3<f32> {
    var c = vec3<f32>(0.0);
    for (var i = 0u; i < u32(frame.shadow.z); i++) {
        if !inside || frame.lights[i * 3u + 1u].w > 0.5 {
            c += lamp_light(i, p, n);
        }
    }
    return c;
}

/// Lunar-Lambert (McEwen) regolith: limb-darkened Lommel-Seeliger blended with Lambert by phase.
fn regolith(albedo: vec3<f32>, n: vec3<f32>, v: vec3<f32>, shadow: f32) -> vec3<f32> {
    let l = frame.sun_dir.xyz;
    let mu0 = max(dot(n, l), 0.0);
    let mu = max(dot(n, v), 0.02);
    let g = acos(clamp(dot(l, v), -1.0, 1.0)) * 57.2958;
    let ll = clamp(1.0 - 0.019 * g + 2.42e-4 * g * g - 1.46e-6 * g * g * g, 0.0, 1.0);
    let surge = 1.0 + 0.35 * exp(-g / 5.0);
    let r = (2.0 * ll * mu0 / (mu0 + mu) + (1.0 - ll) * mu0) * surge;
    return albedo * (frame.sun_color.rgb * frame.sun_dir.w * r * shadow + ambient(n));
}

fn ggx(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, rough: f32, f0: vec3<f32>) -> vec3<f32> {
    let h = normalize(v + l);
    let a = max(rough * rough, 0.002);
    let nh = max(dot(n, h), 0.0);
    let nl = max(dot(n, l), 0.0);
    let nv = max(dot(n, v), 1e-3);
    let d = a * a / (PI * pow(nh * nh * (a * a - 1.0) + 1.0, 2.0));
    let k = a * 0.5;
    let g = nl / (nl * (1.0 - k) + k) * nv / (nv * (1.0 - k) + k);
    let f = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    return d * g * f / (4.0 * nv);
}

/// Hard surfaces (ships, suits): Lambert + GGX under the sun, bounce ambient, emission.
/// A display's light (screens, lit text, seven segments): as bright as a display gets, never past
/// what the bloom picks up (`post.wgsl` keeps only what the exposure takes over white).
fn display_light(c: vec3<f32>) -> vec3<f32> {
    let m = max(c.r, max(c.g, c.b)) * frame.params.x;
    return select(c, c * (0.8 / m), m > 0.8);
}

fn shade_surface(albedo: vec3<f32>, rough: f32, metal: f32, emissive: f32, n: vec3<f32>, v: vec3<f32>, shadow: f32) -> vec3<f32> {
    let l = frame.sun_dir.xyz;
    let nl = max(dot(n, l), 0.0);
    let f0 = mix(vec3<f32>(0.04), albedo, metal);
    let diffuse = albedo * (1.0 - metal);
    let sun = frame.sun_color.rgb * frame.sun_dir.w * shadow;
    let spec = ggx(n, v, l, rough, f0) * PI;
    // a crude environment term: metals reflect the lit ground below the horizon
    let r = reflect(-v, n);
    let env = mix(0.004, 0.06, clamp(-dot(r, frame.up.xyz) * 2.0 + 0.2, 0.0, 1.0)) * frame.sun_dir.w;
    let env_spec = f0 * env * (1.0 - rough * 0.7);
    return (diffuse * nl + spec * nl) * sun + (diffuse + metal * albedo) * ambient(n) + env_spec + albedo * emissive * 8.0;
}
