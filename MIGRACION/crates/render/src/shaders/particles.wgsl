// Particles (core::particles): camera-facing quads sorted far to near, premultiplied alpha, in the
// transparent pass (scene depth bound read-only). Puffs are lit as soft balls: a sphere normal per
// pixel, the sun wrapping round them, lunar dust scattering forward when backlit, the cascades'
// shadow (sampled at the quad's corners and interpolated: big dust overlaps many times, and a
// filtered shadow per pixel and layer was the costliest part of an explosion) and the explosion
// flashes, under a churning noisy density; they fade where they meet the scene (depth) and the
// ground. Fire and sparks add their own light. Needs common.wgsl.

struct Style {
    albedo0: vec4<f32>,
    albedo1: vec4<f32>,
    emission0: vec4<f32>,
    emission1: vec4<f32>,
    curve: vec4<f32>,    // opacity at birth, at death, fade-in share, fade-out share
    shape: vec4<f32>,    // stretch (sizes per m/s), puff (0 chunk, 1 cloud), glow (1: light only), glow reach (radii)
};

struct Part {
    pos: vec4<f32>,      // camera-relative centre, radius (m)
    vel: vec4<f32>,      // velocity (m/s), share of life gone
    misc: vec4<f32>,     // seed, style, centre's height over its ground (m), -
};

@group(1) @binding(0) var<storage, read> parts: array<Part>;
@group(1) @binding(1) var<uniform> styles: array<Style, 24>;
@group(1) @binding(2) var scene_depth: texture_depth_2d;

struct POut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) rel: vec3<f32>,
    @location(2) @interpolate(flat) id: u32,
    @location(3) @interpolate(flat) ax: vec3<f32>,
    @location(4) @interpolate(flat) ay: vec3<f32>,
    // the cascades' shadow at this corner
    @location(5) sun: f32,
    // glows: light kept when drawn bigger than they are (area ratio), and the radius drawn (m)
    @location(6) @interpolate(flat) glow: vec2<f32>,
};

/// A glow is drawn at its true size; under this radius on screen (px) it keeps the pixel and
/// spreads its light over it (area ratio), so it does not flicker. How far it is seen is its
/// style's reach in radii: the bigger the blast, the farther.
const GLOW_MIN_PX: f32 = 1.0;

@vertex
fn particle_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) ii: u32) -> POut {
    let p = parts[ii];
    let s = styles[u32(p.misc.y)];
    let corner = vec2<f32>(f32(vid & 1u) * 2.0 - 1.0, f32(vid >> 1u) * 2.0 - 1.0);
    let to_cam = normalize(-p.pos.xyz);
    let v = p.vel.xyz;
    let vs = v - to_cam * dot(v, to_cam);
    let speed = length(vs);
    var ax: vec3<f32>;
    var ay: vec3<f32>;
    var long = 1.0;
    if s.shape.x > 0.0 && speed > 1e-3 {
        // streaks along the motion seen on screen
        ax = vs / speed;
        ay = cross(to_cam, ax);
        long = 1.0 + s.shape.x * length(v);
    } else {
        // facing the camera, turned by the seed (no two puffs alike)
        let a = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(to_cam.y) > 0.9);
        let bx = normalize(cross(a, to_cam));
        let by = cross(to_cam, bx);
        let spin = p.misc.x * 6.2831853 + p.vel.w * (p.misc.x - 0.5) * 2.0;
        ax = bx * cos(spin) + by * sin(spin);
        ay = cross(to_cam, ax);
    }
    var r = p.pos.w;
    let dist = length(p.pos.xyz);
    var gain = 1.0;
    let glow = s.shape.z > 0.5;
    if glow {
        let reach = s.shape.w;
        if reach > 0.0 {
            gain = 1.0 - smoothstep(reach * 0.6, reach, dist / max(r, 1e-3));
        }
        let px = r / max(dist, 1e-3) * frame.viewport.y / (2.0 * frame.cam_forward.w);
        if px < GLOW_MIN_PX {
            let k = px / GLOW_MIN_PX;
            gain *= k * k;
            r = r / max(k, 1e-6);
        }
    }
    // a glow is pulled toward the camera by its radius (half the way at most): the ground it
    // sits on does not cut it with a hard line
    let centre = p.pos.xyz - p.pos.xyz / max(dist, 1e-3) * select(0.0, min(r, dist * 0.5), glow);
    let rel = centre + (ax * corner.x * long + ay * corner.y) * r;
    var o: POut;
    o.clip = frame.view_proj * vec4<f32>(rel, 1.0);
    o.uv = corner;
    o.rel = rel;
    o.id = ii;
    o.ax = ax;
    o.ay = ay;
    o.sun = sun_light(rel, to_cam, vec2<f32>(f32(ii), f32(vid)));
    o.glow = vec2<f32>(gain, r);
    return o;
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash12(i);
    let b = hash12(i + vec2<f32>(1.0, 0.0));
    let c = hash12(i + vec2<f32>(0.0, 1.0));
    let d = hash12(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

/// Opacity over life: birth -> death, faded in and out.
fn life_opacity(c: vec4<f32>, t: f32) -> f32 {
    var o = mix(c.x, c.y, t);
    if c.z > 0.0 {
        o *= smoothstep(0.0, c.z, t);
    }
    if c.w > 0.0 {
        o *= 1.0 - smoothstep(1.0 - c.w, 1.0, t);
    }
    return o;
}

@fragment
fn particle_fs(in: POut) -> @location(0) vec4<f32> {
    let p = parts[in.id];
    let s = styles[u32(p.misc.y)];
    let t = p.vel.w;
    let puff = s.shape.y;
    let d2 = dot(in.uv, in.uv);
    if d2 >= 1.0 {
        discard;
    }
    if s.shape.z > 0.5 {
        // a glow: light only (alpha 0 adds), bright at the heart and exactly nothing at the edge,
        // faded softly where the scene is in front of its true centre
        let fall = (1.0 - d2) * (1.0 - d2) * exp(-3.0 * d2);
        let near = frame.view_proj[3].z;
        let scene = near / max(textureLoad(scene_depth, vec2<i32>(in.clip.xy), 0), 1e-12);
        let centre = dot(p.pos.xyz, frame.cam_forward.xyz);
        let reach = in.glow.y;
        let seen = clamp((scene - centre + reach * 2.0) / (reach * 2.0), 0.0, 1.0);
        let e = mix(s.emission0.rgb, s.emission1.rgb, t) * (fall * life_opacity(s.curve, t) * seen * in.glow.x);
        return vec4<f32>(e, 0.0);
    }
    let to_cam = normalize(-p.pos.xyz);
    let z = sqrt(1.0 - d2);
    // the surface under this pixel: a ball for a chunk; for a puff, nearly flat toward the camera
    // (lit like a ball, dust reads as a heap of spheres)
    let bulge = mix(1.0, 0.3, puff);
    let n = normalize((in.ax * in.uv.x + in.ay * in.uv.y) * bulge + to_cam * z);
    // a churning cloud: two octaves drifting with age, thinner toward the rim
    let seed = p.misc.x * 37.0;
    let q = in.uv * 1.7 + vec2<f32>(seed, seed * 1.3);
    let cloud = vnoise(q + t * 0.9) * 0.65 + vnoise(q * 2.3 - t * 1.4) * 0.35;
    let rim = 1.0 - smoothstep(mix(0.9, 0.25, puff), 1.0, sqrt(d2));
    let density = rim * mix(1.0, clamp(cloud * 2.2 - 0.75 + 0.6 * z, 0.0, 1.0), puff);
    var alpha = density * life_opacity(s.curve, t);
    // soft where it meets the scene behind it (reversed infinite depth: view depth = near / d)
    let near = frame.view_proj[3].z;
    let scene = near / max(textureLoad(scene_depth, vec2<i32>(in.clip.xy), 0), 1e-12);
    let mine = dot(in.rel - to_cam * (z * p.pos.w * puff), frame.cam_forward.xyz);
    alpha *= clamp((scene - mine) / max(p.pos.w * 0.7, 0.02), 0.0, 1.0);
    // and where it dips under its ground (puffs)
    let up = normalize(p.pos.xyz - frame.body_center.xyz);
    let h = p.misc.z + dot(in.rel - p.pos.xyz, up);
    alpha *= mix(1.0, smoothstep(0.0, p.pos.w * 0.6, h), puff);
    if alpha < 0.002 {
        discard;
    }
    // light: the sun wrapping round a cloud (Lambert for a chunk), forward scattering of fine dust
    // looking toward the sun, the cascades' shadow, bounce light and the flashes
    let l = frame.sun_dir.xyz;
    let nl = dot(n, l);
    let wrap = mix(max(nl, 0.0), pow(nl * 0.5 + 0.5, 1.6), puff);
    let g = 0.55;
    let cv = dot(-to_cam, l);
    let hg = (1.0 - g * g) / pow(1.0 + g * g - 2.0 * g * cv, 1.5) * 0.25;
    let shadow = in.sun;
    let sun = frame.sun_color.rgb * frame.sun_dir.w * shadow * (wrap + hg * puff * 0.8);
    let albedo = srgb_to_linear(mix(s.albedo0.rgb, s.albedo1.rgb, t));
    let lit = albedo * (sun + ambient(n) + flash_light(in.rel, n));
    // emission: hotter in the core of a ball
    let emit = mix(s.emission0.rgb, s.emission1.rgb, t) * mix(1.0, 0.45 + 0.9 * z * cloud, puff);
    return vec4<f32>((lit + emit) * alpha, alpha);
}
