//! What is behind the launcher: space, drawn from shapes (there are no picture files) — a dark
//! gradient, a field of stars, and the lit limb of the Moon in the lower left corner. Everything
//! is the same every time: the stars and the craters come out of a hash of their number. Nothing
//! of it moves, so it is made once for the window's size — its thousands of shapes turned into
//! one mesh — and painted from then on as that mesh (`Backdrop`): a frame costs what its
//! widgets cost, whatever is behind them.
use egui::{
    Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Vec2,
    epaint::{TessellationOptions, Tessellator},
    pos2, vec2,
};
use std::{f32::consts::TAU, sync::Arc};

/// A number in 0..1 out of a whole number, always the same.
fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / 16_777_216.0
}

/// Smooth noise in 0..1 over the plane (the hash at the whole points, eased between them).
fn noise(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let ease = |f: f32| f * f * (3.0 - 2.0 * f);
    let (sx, sy) = (ease(x - xi), ease(y - yi));
    let at = |i: f32, j: f32| hash((i as i32 as u32).wrapping_mul(73_856_093) ^ (j as i32 as u32).wrapping_mul(19_349_663));
    let (top, bottom) = (at(xi, yi) + (at(xi + 1.0, yi) - at(xi, yi)) * sx, at(xi, yi + 1.0) + (at(xi + 1.0, yi + 1.0) - at(xi, yi + 1.0)) * sx);
    top + (bottom - top) * sy
}

fn grey(r: f32, g: f32, b: f32) -> Color32 {
    let c = |v: f32| v.clamp(0.0, 255.0).round() as u8;
    Color32::from_rgb(c(r), c(g), c(b))
}

/// The dark of space: not one black, a little bluer and lighter toward the Moon's corner.
fn gradient(out: &mut Vec<Shape>, r: Rect) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(r.left_top(), Color32::from_rgb(6, 9, 15));
    mesh.colored_vertex(r.right_top(), Color32::from_rgb(3, 4, 8));
    mesh.colored_vertex(r.right_bottom(), Color32::from_rgb(5, 8, 13));
    mesh.colored_vertex(r.left_bottom(), Color32::from_rgb(11, 16, 25));
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    out.push(Shape::mesh(mesh));
}

/// How many stars there are.
const STARS: u32 = 260;

/// The stars: most of them faint and small, a few bright ones with a glow; each caught at its own
/// moment of its twinkle.
fn stars(out: &mut Vec<Shape>, r: Rect) {
    for i in 0..STARS {
        let at = pos2(r.left() + hash(i * 7) * r.width(), r.top() + hash(i * 7 + 1) * r.height());
        let bright = hash(i * 7 + 2).powi(3);
        let twinkle = 1.0 + 0.22 * (TAU * hash(i * 7 + 4)).sin();
        let alpha = ((0.22 + 0.78 * bright) * twinkle).clamp(0.0, 1.0);
        let tint = match (hash(i * 7 + 5) * 4.0) as u32 {
            0 => Color32::from_rgb(190, 212, 255),
            1 => Color32::from_rgb(255, 236, 214),
            _ => Color32::from_rgb(232, 238, 250),
        };
        let size = 0.5 + 1.15 * bright;
        if bright > 0.6 {
            // (the bright ones glow: two soft rings, not a disc)
            out.push(Shape::circle_filled(at, size * 4.4, tint.gamma_multiply(alpha * 0.03)));
            out.push(Shape::circle_filled(at, size * 2.2, tint.gamma_multiply(alpha * 0.08)));
        }
        out.push(Shape::circle_filled(at, size, tint.gamma_multiply(alpha)));
    }
}

/// The Moon as it is seen: a disc far larger than the window, its centre below and to the left of
/// the lower left corner, lit from above and behind so that only a band along its limb is in the
/// sun and the terminator fades inwards.
struct Moon {
    centre: Pos2,
    radius: f32,
    /// How far it has turned (radians).
    turn: f32,
}

/// Toward the sun, as seen on the screen (x right, y down, z to the eye).
const SUN: [f32; 3] = [-0.22, -0.66, -0.72];

impl Moon {
    /// How lit the point at `angle` round the centre and `rho` (0..1) of the radius out is: 0 in
    /// the night, 1 at the brightest of the limb.
    fn light(&self, angle: f32, rho: f32) -> f32 {
        let (x, y) = (angle.cos() * rho, angle.sin() * rho);
        let z = (1.0 - rho * rho).max(0.0).sqrt();
        let lit = (x * SUN[0] + y * SUN[1] + z * SUN[2]) / 0.69;
        // (a soft terminator: the sun is not a point)
        let lit = (lit + 0.06).clamp(0.0, 1.0);
        lit * lit * (3.0 - 2.0 * lit)
    }

    /// How light the ground is there (the seas are darker), on the Moon as it turns.
    fn albedo(&self, angle: f32, rho: f32) -> f32 {
        let (x, y) = ((angle - self.turn).cos() * rho, (angle - self.turn).sin() * rho);
        let seas = noise(x * 5.0 + 11.0, y * 5.0 + 3.0);
        let fine = noise(x * 21.0 + 40.0, y * 21.0 + 17.0) * 0.6 + noise(x * 67.0, y * 67.0 + 90.0) * 0.4;
        0.52 + 0.38 * seas + 0.16 * (fine - 0.5)
    }

    fn colour(&self, angle: f32, rho: f32, gain: f32) -> Color32 {
        let v = self.light(angle, rho) * self.albedo(angle, rho) * gain;
        // (the night side is not quite black: it hides the stars and no more)
        grey(4.0 + 222.0 * v, 6.0 + 224.0 * v, 9.0 + 228.0 * v)
    }

    fn point(&self, angle: f32, rho: f32) -> Pos2 {
        self.centre + vec2(angle.cos(), angle.sin()) * (rho * self.radius)
    }

    /// The part of the disc that falls in `r`, as a fan of rings: the angles its corners are seen
    /// under from the centre, and from the nearest of it out to the limb.
    fn draw(&self, out: &mut Vec<Shape>, r: Rect) {
        let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
        let angles = corners.map(|c| (c.y - self.centre.y).atan2(c.x - self.centre.x));
        let (from, to) = (angles.iter().copied().fold(f32::MAX, f32::min), angles.iter().copied().fold(f32::MIN, f32::max));
        let nearest = r.distance_to_pos(self.centre) / self.radius;
        if nearest >= 1.0 {
            return;
        }
        // (the centre is below the window: past where the limb goes under its bottom edge there is nothing to see)
        let to = to.min(-((self.centre.y - r.bottom()) / self.radius).clamp(0.0, 1.0).asin());
        const AROUND: u32 = 120;
        const OUT: u32 = 34;
        let mut mesh = Mesh::default();
        for j in 0..=OUT + 2 {
            for i in 0..=AROUND {
                let angle = from + (to - from) * i as f32 / AROUND as f32;
                // (the last two rings are the limb's edge: it fades out over a pixel, then a faint glow)
                let (rho, colour) = match j.checked_sub(OUT) {
                    None | Some(0) => {
                        let rho = nearest + (1.0 - nearest) * j as f32 / OUT as f32;
                        (rho, self.colour(angle, rho, 1.0))
                    }
                    Some(1) => (1.0 + 1.2 / self.radius, self.colour(angle, 1.0, 1.0).gamma_multiply(0.16)),
                    Some(_) => (1.0 + 9.0 / self.radius, Color32::TRANSPARENT),
                };
                mesh.colored_vertex(self.point(angle, rho), colour);
            }
        }
        let row = AROUND + 1;
        for j in 0..OUT + 2 {
            for i in 0..AROUND {
                let a = j * row + i;
                mesh.add_triangle(a, a + 1, a + row);
                mesh.add_triangle(a + 1, a + row + 1, a + row);
            }
        }
        out.push(Shape::mesh(mesh));
        self.craters(out, from, to, nearest);
    }

    /// The craters round the whole disc; those in sight are drawn as bowls: the wall on the side
    /// of the sun in its own shadow (a dark crescent), the floor nearly as light as the ground
    /// round it, the far wall catching the sun (a bright edge). Near the limb they are seen edge
    /// on, flattened.
    fn craters(&self, shapes: &mut Vec<Shape>, from: f32, to: f32, nearest: f32) {
        const CRATERS: u32 = 420;
        const STEPS: usize = 24;
        let sun = vec2(SUN[0], SUN[1]).normalized();
        for i in 0..CRATERS {
            // (the angles in sight are between -TAU/2 and 0: the centre of the disc is below the window)
            let angle = (hash(i * 5 + 1000) * TAU + self.turn).rem_euclid(TAU) - TAU;
            let rho = 1.0 - 0.30 * hash(i * 5 + 1001).powf(1.4) - 0.004;
            let size = 3.0 + 22.0 * hash(i * 5 + 1002).powi(4);
            let reach = size / self.radius;
            if angle < from - reach || angle > to + reach || rho < nearest - reach || self.light(angle, rho) < 0.03 {
                continue;
            }
            let flat = (1.0 - rho * rho).max(0.0).sqrt().max(0.16);
            let centre = self.point(angle, rho);
            let (out, along) = (vec2(angle.cos(), angle.sin()), vec2(-angle.sin(), angle.cos()));
            // a point of the rim of the bowl, `scale` of its size, at `a` round it
            let rim = |scale: f32, a: f32| out * (a.cos() * size * flat * scale) + along * (a.sin() * size * scale);
            let ring = |scale: f32, shift: Vec2| -> Vec<Pos2> { (0..STEPS).map(|k| centre + shift + rim(scale, TAU * k as f32 / STEPS as f32)).collect() };
            // the floor is seen past the shadow of the near wall: set back from the sun, flattened as the bowl is
            let back = -sun * (size * 0.2);
            let back = out * (back.dot(out) * flat) + along * back.dot(along);
            shapes.push(Shape::convex_polygon(ring(1.0, Vec2::ZERO), self.colour(angle, rho, 0.5), Stroke::NONE));
            shapes.push(Shape::convex_polygon(ring(0.8, back), self.colour(angle, rho, 0.94), Stroke::NONE));
            // the far wall: an arc of the rim round its point farthest from the sun
            let far = (-along.dot(sun)).atan2(-out.dot(sun) * flat);
            let arc: Vec<Pos2> = (0..=8).map(|k| centre + rim(1.0, far + (k as f32 / 4.0 - 1.0) * 1.05)).collect();
            shapes.push(Shape::line(arc, Stroke::new(1.0, self.colour(angle, rho, 1.3))));
        }
    }
}

/// The corners a little darker, so that the eye stays in the middle.
fn vignette(out: &mut Vec<Shape>, r: Rect) {
    let (mid, rim) = (Color32::TRANSPARENT, Color32::from_black_alpha(96));
    let mut mesh = Mesh::default();
    let c = r.center();
    mesh.colored_vertex(c, mid);
    for p in [r.left_top(), pos2(c.x, r.top()), r.right_top(), pos2(r.right(), c.y), r.right_bottom(), pos2(c.x, r.bottom()), r.left_bottom(), pos2(r.left(), c.y)] {
        mesh.colored_vertex(p, rim);
    }
    for i in 0..8u32 {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 8);
    }
    out.push(Shape::mesh(mesh));
}

/// The whole background of the window `r`, as the shapes it is drawn with.
fn shapes(r: Rect) -> Vec<Shape> {
    let mut out = Vec::new();
    gradient(&mut out, r);
    stars(&mut out, r);
    // (placed by the window's height: the limb crosses the lower left corner whatever its size)
    let k = r.height() / 640.0;
    let moon = Moon { centre: r.left_bottom() + vec2(-40.0, 548.0) * k, radius: 760.0 * k, turn: 0.0 };
    moon.draw(&mut out, r);
    vignette(&mut out, r);
    out
}

/// The background of the window `r` as one mesh, its edges smoothed for a screen of
/// `pixels_per_point`.
fn mesh(r: Rect, pixels_per_point: f32) -> Mesh {
    // (no picture of a disc from the type's atlas: the mesh is plain colour, whatever the atlas holds)
    let options = TessellationOptions { prerasterized_discs: false, coarse_tessellation_culling: false, ..Default::default() };
    let mut tessellator = Tessellator::new(pixels_per_point, options, [1, 1], Vec::new());
    let mut mesh = Mesh::default();
    for shape in shapes(r) {
        tessellator.tessellate_shape(shape, &mut mesh);
    }
    mesh
}

/// The background, kept from one frame to the next.
#[derive(Default)]
pub struct Backdrop {
    /// What the mesh was made for: the window and the screen's scale.
    made: Option<(Rect, f32)>,
    mesh: Arc<Mesh>,
}

impl Backdrop {
    /// Paint the background of the window `r`: made if this is its first frame or its size has
    /// changed, the same mesh as last time otherwise.
    pub fn paint(&mut self, painter: &Painter, r: Rect, pixels_per_point: f32) {
        if self.made != Some((r, pixels_per_point)) {
            self.mesh = Arc::new(mesh(r, pixels_per_point));
            self.made = Some((r, pixels_per_point));
        }
        painter.add(Shape::mesh(self.mesh.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_spread_and_always_the_same() {
        let values: Vec<f32> = (0..1000).map(hash).collect();
        assert!(values.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = values.iter().sum::<f32>() / 1000.0;
        assert!((mean - 0.5).abs() < 0.05, "{mean}");
        assert_eq!(hash(77), hash(77));
        assert_ne!(hash(77), hash(78));
    }

    #[test]
    fn the_noise_is_smooth_and_in_range() {
        for i in 0..200 {
            let (x, y) = (i as f32 * 0.173 - 9.0, i as f32 * 0.071 + 2.0);
            let (a, b) = (noise(x, y), noise(x + 0.01, y));
            assert!((0.0..=1.0).contains(&a));
            assert!((a - b).abs() < 0.05, "{a} {b}");
        }
    }

    #[test]
    fn the_background_is_one_mesh_and_always_the_same() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 720.0));
        let made = mesh(r, 1.0);
        assert!(made.vertices.len() > 5000 && made.indices.len() % 3 == 0, "{} {}", made.vertices.len(), made.indices.len());
        assert!(made.indices.iter().all(|i| (*i as usize) < made.vertices.len()));
        assert!(made.vertices.iter().all(|v| v.pos.x.is_finite() && v.pos.y.is_finite()));
        // (plain colour: every corner reads the atlas's white dot)
        assert!(made.vertices.iter().all(|v| v.uv == egui::epaint::WHITE_UV));
        assert_eq!(made.texture_id, egui::TextureId::default());
        let again = mesh(r, 1.0);
        assert!(made.indices == again.indices && made.vertices.len() == again.vertices.len() && made.vertices.iter().zip(&again.vertices).all(|(a, b)| a.pos == b.pos && a.color == b.color));
        // the corner where the Moon is has more of it than space has stars
        let moon = made.vertices.iter().filter(|v| v.pos.x < 400.0 && v.pos.y > 420.0).count();
        assert!(moon > made.vertices.len() / 3, "{moon} of {}", made.vertices.len());
        // another size is another mesh
        assert_ne!(mesh(Rect::from_min_size(pos2(0.0, 0.0), vec2(1040.0, 640.0)), 1.0).vertices.len(), 0);
    }

    #[test]
    fn the_limb_is_lit_and_the_inside_is_night() {
        let moon = Moon { centre: pos2(0.0, 0.0), radius: 760.0, turn: 0.0 };
        let up = -TAU / 4.0;
        assert!(moon.light(up, 1.0) > 0.9);
        assert!(moon.light(up, 0.95) > moon.light(up, 0.85));
        assert_eq!(moon.light(up, 0.6), 0.0);
        // (and it is the upper limb that is lit, not the lower)
        assert_eq!(moon.light(-up, 1.0), 0.0);
    }
}
