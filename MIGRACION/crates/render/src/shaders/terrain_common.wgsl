// Cube-sphere node frames, shared by the generator and the terrain draw (twin of core::terrain_gen).
// A point of a node is rebuilt as an offset from the node's centre, never from absolute
// coordinates: every quantity stays small, so f32 keeps millimetres at 1737 km.

const FACE_N: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3(1.0, 0.0, 0.0), vec3(-1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0),
    vec3(0.0, -1.0, 0.0), vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, -1.0));
const FACE_U: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3(0.0, 0.0, -1.0), vec3(0.0, 0.0, 1.0), vec3(1.0, 0.0, 0.0),
    vec3(1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(-1.0, 0.0, 0.0));
const FACE_V: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3(0.0, 1.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, -1.0),
    vec3(0.0, 0.0, 1.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 1.0, 0.0));

const PI4: f32 = 0.78539816339745;

struct Node {
    frame0: vec4<f32>,   // alpha a, alpha b, tan a, tan b (centre)
    frame1: vec4<f32>,   // cos a, cos b, sin a, sin b
    dir: vec4<f32>,      // centre direction, s = |n + A u + B v|
    info: vec4<f32>,     // half extent (params), body radius, -, -
    tex: vec4<f32>,      // detail origin u, v (m, periodic), metres per param, skirt drop (m)
    ids: vec4<u32>,      // face, layer, level, grid
};

// Polynomials instead of hardware sin/cos: those have absolute errors near 1e-6, fatal for the
// tiny angles of deep nodes; these keep relative precision for |x| < 0.8.
fn sin_small(x: f32) -> f32 {
    let x2 = x * x;
    return x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))));
}

fn cos_small(x: f32) -> f32 {
    let x2 = x * x;
    return 1.0 - x2 / 2.0 * (1.0 - x2 / 12.0 * (1.0 - x2 / 30.0 * (1.0 - x2 / 56.0 * (1.0 - x2 / 90.0))));
}

struct Delta {
    d: vec3<f32>,        // direction minus the centre's direction
    ab: vec2<f32>,       // tan-warp coordinates (A, B) of the point
};

/// Unit direction at parameter offsets (da, db) from the node's centre, minus the centre's.
fn node_delta(face: u32, f0: vec4<f32>, f1: vec4<f32>, dir: vec4<f32>, da: f32, db: f32) -> Delta {
    let qa = da * PI4;
    let qb = db * PI4;
    let sa = sin_small(qa);
    let sb = sin_small(qb);
    let ca = f1.x * cos_small(qa) - f1.z * sa;
    let cb = f1.y * cos_small(qb) - f1.w * sb;
    let d_a = sa / (ca * f1.x);
    let d_b = sb / (cb * f1.y);
    let aa = f0.z + d_a;
    let bb = f0.w + d_b;
    let s = sqrt(1.0 + aa * aa + bb * bb);
    let ds = (2.0 * f0.z * d_a + d_a * d_a + 2.0 * f0.w * d_b + d_b * d_b) / (s + dir.w);
    var o: Delta;
    o.d = (FACE_U[face] * d_a + FACE_V[face] * d_b - dir.xyz * ds) / s;
    o.ab = vec2<f32>(aa, bb);
    return o;
}
