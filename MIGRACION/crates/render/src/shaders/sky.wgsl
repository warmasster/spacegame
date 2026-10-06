// Sky: black space with the Milky Way's glow, the Earth (procedural, lit by the sun) and the sun's
// disc, in one full-screen triangle behind everything; stars as one instanced draw of tiny quads.
// Needs common.wgsl.

@group(1) @binding(0) var<storage, read> stars: array<vec4<f32>>;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn sky_vs(@builtin(vertex_index) vid: u32) -> SkyOut {
    let uv = vec2<f32>(f32((vid << 1u) & 2u), f32(vid & 2u));
    var o: SkyOut;
    o.clip = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.ndc = uv * 2.0 - 1.0;
    return o;
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}

fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(mix(hash31(i), hash31(i + vec3(1.0, 0.0, 0.0)), u.x), mix(hash31(i + vec3(0.0, 1.0, 0.0)), hash31(i + vec3(1.0, 1.0, 0.0)), u.x), u.y),
        mix(mix(hash31(i + vec3(0.0, 0.0, 1.0)), hash31(i + vec3(1.0, 0.0, 1.0)), u.x), mix(hash31(i + vec3(0.0, 1.0, 1.0)), hash31(i + vec3(1.0, 1.0, 1.0)), u.x), u.y),
        u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var s = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        s += a * vnoise(q);
        q = q * 2.03 + vec3<f32>(1.7, 9.2, 3.1);
        a *= 0.5;
    }
    return s;
}

const GALAXY_POLE: vec3<f32> = vec3<f32>(0.46, 0.31, 0.83);

// The system's backdrop planet (an Earth-like globe) painted in the sky.
fn backdrop_planet(d: vec3<f32>) -> vec4<f32> {
    let e = frame.backdrop.xyz;
    let ang = frame.backdrop.w;
    if ang <= 0.0 {
        return vec4<f32>(0.0);
    }
    let c = dot(d, e);
    let sin_r = sin(ang);
    let off = d - e * c;
    let r = length(off) / sin_r;
    if c < 0.0 || r > 1.08 {
        return vec4<f32>(0.0);
    }
    let t1 = normalize(cross(e, vec3<f32>(0.0, 1.0, 0.0)));
    let t2 = cross(t1, e);
    let o2 = vec2<f32>(dot(off, t1), dot(off, t2)) / sin_r;
    let sun = frame.sun_dir.xyz;
    if r > 1.0 {
        // thin atmosphere halo
        let lit = clamp(dot(normalize(off), sun) * 0.8 + 0.3, 0.0, 1.0);
        let halo = exp(-(r - 1.0) * 60.0) * lit;
        return vec4<f32>(vec3<f32>(0.25, 0.5, 1.0) * halo * 1.5, 0.0);
    }
    let z = sqrt(max(1.0 - r * r, 0.0));
    let n = normalize(t1 * o2.x + t2 * o2.y - e * z);
    // spin the globe slowly; procedural continents, ice caps and clouds
    let t = frame.origin.w * 0.002;
    let cs = vec2<f32>(cos(t), sin(t));
    let body = vec3<f32>(o2.x * cs.x - z * cs.y, o2.y, o2.x * cs.y + z * cs.x);
    let land = smoothstep(0.52, 0.56, fbm(body * 2.2 + vec3<f32>(3.0, 1.0, 0.0)));
    let ice = smoothstep(0.78, 0.86, abs(body.y));
    let cloud = smoothstep(0.5, 0.75, fbm(body * 4.5 + vec3<f32>(0.0, 7.0, t * 3.0)));
    var albedo = mix(vec3<f32>(0.02, 0.06, 0.2), mix(vec3<f32>(0.12, 0.2, 0.08), vec3<f32>(0.35, 0.3, 0.2), fbm(body * 9.0)), land);
    albedo = mix(albedo, vec3<f32>(0.85), max(ice, cloud * 0.9));
    let nl = dot(n, sun);
    let day = max(nl, 0.0);
    let spec = pow(max(dot(reflect(-sun, n), -d), 0.0), 40.0) * (1.0 - land) * (1.0 - cloud) * 0.6;
    let rim = pow(1.0 - z, 3.0) * vec3<f32>(0.3, 0.55, 1.0) * smoothstep(-0.2, 0.3, nl);
    let lights = vec3<f32>(1.0, 0.6, 0.25) * land * (1.0 - cloud) * step(0.93, hash31(floor(body * 180.0))) * smoothstep(0.05, -0.15, nl) * 0.05;
    let col = albedo * day * frame.sun_dir.w * 1.1 + spec * frame.sun_dir.w + rim * frame.sun_dir.w * 0.5 + lights;
    return vec4<f32>(col, 1.0);
}

@fragment
fn sky_fs(in: SkyOut) -> @location(0) vec4<f32> {
    let w = frame.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let d = normalize(w.xyz);
    var c = vec3<f32>(0.0);
    // Milky Way: a soft band with dust lanes
    let band = dot(d, GALAXY_POLE);
    let glow = exp(-band * band * 14.0) * (0.6 + 0.8 * fbm(d * 6.0)) * (1.0 - 0.7 * smoothstep(0.45, 0.7, fbm(d * 11.0 + 4.0)) * exp(-band * band * 60.0));
    c += vec3<f32>(0.8, 0.85, 1.0) * glow * 0.0035;
    let e = backdrop_planet(d);
    c = mix(c, e.rgb, e.a) + select(e.rgb, vec3<f32>(0.0), e.a > 0.0);
    // the sun: disc and a tight glare (bloom spreads it)
    let s = dot(d, frame.sun_dir.xyz);
    let disc = smoothstep(0.999985, 0.999992, s);
    let glare = pow(max(s, 0.0), 2000.0) * 0.4 + pow(max(s, 0.0), 200.0) * 0.02;
    c += frame.sun_color.rgb * (disc * 60.0 + glare) * frame.sun_dir.w;
    return vec4<f32>(c, 1.0);
}

struct StarOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn star_vs(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> StarOut {
    let a = stars[iid * 2u];
    let b = stars[iid * 2u + 1u];
    let corners = array<vec2<f32>, 6>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));
    let k = corners[vid];
    var o: StarOut;
    var clip = frame.view_proj * vec4<f32>(a.xyz, 0.0);
    // hidden behind the backdrop planet's disc
    let behind = frame.backdrop.w > 0.0 && dot(a.xyz, frame.backdrop.xyz) > cos(frame.backdrop.w * 1.05);
    let size = a.w * select(1.0, 0.0, behind);
    clip = vec4<f32>(clip.xy + k * size * 2.0 * frame.viewport.zw * clip.w, 0.0, clip.w);
    o.clip = clip;
    o.corner = k;
    o.color = b.rgb * b.w;
    return o;
}

@fragment
fn star_fs(in: StarOut) -> @location(0) vec4<f32> {
    let f = exp(-dot(in.corner, in.corner) * 3.0);
    return vec4<f32>(in.color * f, 1.0);
}
