// Props: small things drawn as instances of a handful of primitive meshes (box, cylinder, sphere,
// cone, disc) and text drawn as quads of a signed-distance font atlas. Panel controls, gauges,
// lamps, screens, actuator rods, silkscreen. Each instance comes camera-relative from the CPU.
// Needs common.wgsl.

struct Prop {
    pos: vec4<f32>,      // camera-relative centre; w: emission (0..)
    rot: vec4<f32>,      // quaternion
    scale: vec4<f32>,    // xyz
    mat: vec4<f32>,      // linear albedo rgb, w: roughness
    params: vec4<f32>,   // x metalness, y: 1 inside a hull
};

@group(1) @binding(0) var<storage, read> props: array<Prop>;

struct PIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    // a model's own surface: sRGB colour; roughness, metalness, glow, and w: 1 they are its own
    @location(2) color: vec4<f32>,
    @location(3) own: vec4<f32>,
};

struct POut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) @interpolate(flat) id: u32,
    @location(3) color: vec3<f32>,
    @location(4) own: vec4<f32>,
};

@vertex
fn prop_vs(v: PIn, @builtin(instance_index) ii: u32) -> POut {
    let p = props[ii];
    let rel = p.pos.xyz + quat_rotate(p.rot, v.pos * p.scale.xyz);
    var o: POut;
    o.clip = pass_u.view_proj * vec4<f32>(rel, 1.0);
    o.rel = rel;
    o.nrm = quat_rotate(p.rot, v.nrm / max(p.scale.xyz, vec3<f32>(1e-6)));
    o.id = ii;
    o.color = srgb_to_linear(v.color.rgb);
    o.own = v.own;
    return o;
}

@fragment
fn prop_fs(in: POut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let p = props[in.id];
    var n = normalize(in.nrm);
    if !front {
        n = -n;
    }
    let v = normalize(-in.rel);
    let albedo = p.mat.rgb * in.color;
    let rough = mix(p.mat.w, in.own.x, in.own.w);
    let metal = mix(p.params.x, in.own.y, in.own.w);
    let glow = abs(p.pos.w) + in.own.z * in.own.w * 3.0;
    let s = sun_light(in.rel, n, in.clip.xy);
    // a negative emission is a display's: lit, but never into the bloom
    var c = shade_surface(albedo, max(rough, 0.04), metal, glow, n, v, s) + albedo * (1.0 - metal) * side_light(in.rel, n, p.params.y > 0.5);
    if p.pos.w < 0.0 {
        c = display_light(c);
    }
    return vec4<f32>(c, 1.0);
}

// ---- text ----

struct Glyph {
    center: vec4<f32>,   // camera-relative; w: emission
    u: vec4<f32>,        // half width along the text
    v: vec4<f32>,        // half height
    uv: vec4<f32>,       // atlas rectangle
    color: vec4<f32>,    // linear rgb, w: relief (0 flat, > 0 raised, < 0 cut in)
};

@group(1) @binding(1) var<storage, read> glyphs: array<Glyph>;
@group(1) @binding(2) var atlas: texture_2d<f32>;
@group(1) @binding(3) var atlas_sampler: sampler;
// posters, signs, logos (u.w = 1 marks a decal)
@group(1) @binding(4) var decal_atlas: texture_2d<f32>;

struct GOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) id: u32,
};

@vertex
fn glyph_vs(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> GOut {
    let g = glyphs[ii];
    let corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let c = corners[vi];
    let rel = g.center.xyz + g.u.xyz * c.x + g.v.xyz * c.y;
    var o: GOut;
    o.clip = pass_u.view_proj * vec4<f32>(rel, 1.0);
    o.rel = rel;
    // atlas rows grow downward
    o.uv = vec2<f32>(mix(g.uv.x, g.uv.z, c.x * 0.5 + 0.5), mix(g.uv.w, g.uv.y, c.y * 0.5 + 0.5));
    o.id = ii;
    return o;
}

/// Width of a raised letter's rounded edge, in the field's units (0.5 is the outline).
const BEVEL: f32 = 0.09;

@fragment
fn glyph_fs(in: GOut) -> @location(0) vec4<f32> {
    let g = glyphs[in.id];
    // both atlases sampled where every pixel runs (derivatives), one used
    let decal = textureSample(decal_atlas, atlas_sampler, in.uv);
    let d = textureSample(atlas, atlas_sampler, in.uv).r;
    if g.u.w > 0.5 {
        if decal.a < 0.02 {
            discard;
        }
        let n = normalize(cross(g.u.xyz, g.v.xyz));
        let s = sun_light(in.rel, n, in.clip.xy);
        let albedo = decal.rgb * g.color.rgb;
        let v = normalize(-in.rel);
        let lit = shade_surface(albedo, 0.45, 0.0, g.center.w, n, v, s) + albedo * side_light(in.rel, n, g.v.w > 0.5);
        return vec4<f32>(lit * decal.a, decal.a);
    }
    let w = max(fwidth(d), 1e-4) * 0.75;
    let a = smoothstep(0.5 - w, 0.5 + w, d);
    let relief = g.color.w;
    // raised letters cast a soft dark rim on the plate round them
    let rim = select(0.0, (1.0 - a) * smoothstep(0.5 - BEVEL * 1.6, 0.5, d) * 0.55 * clamp(abs(relief), 0.0, 1.0), relief != 0.0);
    if a < 0.01 && rim < 0.01 {
        discard;
    }
    var n = normalize(cross(g.u.xyz, g.v.xyz));
    if relief != 0.0 {
        // the slope of the letter's edge from the field's gradient (atlas rows grow downward)
        let dims = vec2<f32>(textureDimensions(atlas));
        let du = textureSampleLevel(atlas, atlas_sampler, in.uv + vec2<f32>(1.0 / dims.x, 0.0), 0.0).r - d;
        let dv = textureSampleLevel(atlas, atlas_sampler, in.uv + vec2<f32>(0.0, 1.0 / dims.y), 0.0).r - d;
        let gw = normalize(g.u.xyz) * du - normalize(g.v.xyz) * dv;
        let gl = length(gw);
        if gl > 1e-6 {
            let t = clamp((d - 0.5) / BEVEL, 0.0, 1.0);
            let slope = 4.0 * t * (1.0 - t) * 1.6 * sign(relief) * min(abs(relief), 1.5);
            n = normalize(n - gw / gl * slope);
        }
    }
    let s = sun_light(in.rel, n, in.clip.xy);
    let albedo = g.color.rgb;
    let v = normalize(-in.rel);
    var lit = shade_surface(albedo, select(0.7, 0.45, relief != 0.0), 0.0, g.center.w, n, v, s) + albedo * side_light(in.rel, n, g.v.w > 0.5);
    // lit letters are a display's: never into the bloom
    if g.center.w > 0.0 {
        lit = display_light(lit);
    }
    let alpha = a + (1.0 - a) * rim;
    return vec4<f32>(lit * a, alpha);
}
