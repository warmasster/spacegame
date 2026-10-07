//! Changes to a body's ground at run time (blast craters): a bounded list that every height query
//! and the GPU terrain add on top of the generated surface, plus a log of the regions that changed
//! so each terrain regenerates only the nodes they touch.
use glam::DVec3;

/// Craters a body keeps; the oldest goes when a new one does not fit.
pub const MAX_CRATERS: usize = 1024;
/// Craters one terrain node's generation sees (the biggest that touch it). Blasts piling up merge
/// (`add`), so a node rarely has more: past this the GPU would draw less than the collision.
pub const MAX_NODE_CRATERS: usize = 64;
/// A blast whose centre lands within this share of an earlier crater's radius, and of about its
/// size (the bigger under `MERGE_SIZE` times the smaller), widens and deepens that crater instead
/// of adding one: where blasts pile up the list stays short.
const MERGE_NEAR: f64 = 0.6;
const MERGE_SIZE: f64 = 2.5;
/// Out to where a crater changes the ground, in radii (its ejecta blanket).
pub const REACH: f64 = 3.0;
/// The rim's radius wobbles by this share round the crater (no two alike, never a circle).
const WOBBLE: f64 = 0.07;
/// How much the blanket's thickness varies round the crater (broad lobes; rays are only colour,
/// as on the Moon: as relief they read as straight ridges).
const LOBES_GAIN: f64 = 0.35;
/// Rounding of the rim's crest (depths): a smooth minimum of the wall and the blanket.
const CREST: f64 = 0.18;
/// The old ground is wiped out to here (radii): all of it in the bowl (it was thrown out), less
/// and less under the blanket by the rim, which buries what was smaller than itself.
const ERASE_TO: f64 = 1.6;
/// Fixed unit directions the pattern is read along, and each one's phase per crater seed.
const LOBES: [[f64; 3]; 5] = [[0.6, 0.48, 0.64], [-0.8, 0.36, 0.48], [0.0, -0.6, 0.8], [0.48, 0.8, -0.36], [-0.36, -0.48, -0.8]];
const PHASES: [f64; 5] = [12.9898, 78.233, 37.719, 4.581, 93.989];
/// Changes remembered for terrains that have not caught up yet.
const LOG: usize = 256;

/// A simple crater, as fresh lunar ones and explosion craters are: a bowl `depth` m deep (about a
/// fifth of its diameter, Pike 1977), a rim `rim` x depth over the old ground at one radius, and
/// the ejecta blanket outside thinning as (r/R)^-3 (McGetchin 1973) to nothing at `REACH` radii.
/// What the bowl lost is what the blanket holds: with that law the rim is a quarter of the depth.
/// The ground it is dug in is first levelled to `ground` (the height at its centre when it formed):
/// nothing that was there before shows in the bowl. `seed` makes its outline and rays its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crater {
    /// Unit direction of the centre from the body's centre.
    pub dir: DVec3,
    pub radius: f64,
    pub depth: f64,
    pub rim: f64,
    /// 0..1.
    pub seed: f64,
    /// Ground height over the datum at its centre when it formed (m): the level it wipes to.
    pub ground: f64,
}

/// How much of the old ground a crater wipes out at `rr` radii (1 in the bowl, 0 from `ERASE_TO`;
/// the rim's wobble moves it with the rim). Twin of `blast_erase` in terrain_gen.wgsl.
pub fn blast_erase(rr: f64, wobble: f64) -> f64 {
    let t = ((rr / (1.0 + WOBBLE * wobble) - 1.0) / (ERASE_TO - 1.0)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// A crater's irregularity seen from its centre along `v` (unit, across the ground): the rim's
/// wobble (-1..1), the blanket's broad lobes (-1..1) and how much ray there is (0..1, mostly
/// little; colour only). Twin of `blast_pattern` in terrain_gen.wgsl.
pub fn blast_pattern(v: DVec3, seed: f64) -> (f64, f64, f64) {
    let ph = |i: usize| (seed * PHASES[i]).fract() * std::f64::consts::TAU;
    let dv = |i: usize| v.dot(DVec3::from_array(LOBES[i]));
    let wobble = ((ph(0) + 3.0 * dv(0)).sin() + (ph(1) + 5.0 * dv(1)).sin() + 0.5 * (ph(2) + 9.0 * dv(2)).sin()) / 2.5;
    let lobes = (ph(3) + 4.0 * dv(3)).sin() * (ph(4) + 3.0 * dv(4)).sin();
    let rays = ((ph(3) + 23.0 * dv(3)).sin() * (ph(4) + 17.0 * dv(4)).sin()).max(0.0);
    (wobble, lobes, rays)
}

/// Shape of a blast crater at `rr` radii from the centre, in depths: -1 at the centre, a rounded
/// crest about `rim` high (its radius moved by `wobble`), the blanket in (r/R)^-3 varied by
/// `lobes`, 0 from `REACH` on with no step nor kink. The wall (a parabola) and the blanket are
/// both carried past the crest and the lower one is the ground, through a smooth minimum.
/// Twin of `blast_profile` in terrain_gen.wgsl.
pub fn blast_profile(rr: f64, rim: f64, wobble: f64, lobes: f64) -> f64 {
    let rr = rr / (1.0 + WOBBLE * wobble);
    if rr >= REACH {
        return 0.0;
    }
    let wall = -1.0 + (1.0 + rim) * rr * rr;
    let x = rr.max(0.05);
    let w = 1.0 - ((x - 1.0) / (REACH - 1.0)).powi(2);
    // the lobes grow out of the rim (none at the crest)
    let t = ((x - 1.0) / 0.5).clamp(0.0, 1.0);
    let lobed = 1.0 + LOBES_GAIN * lobes * t * t * (3.0 - 2.0 * t);
    let blanket = rim * w * w * lobed / (x * x * x);
    smooth_min(wall, blanket, CREST)
}

/// Polynomial smooth minimum: `min` with the corner rounded over `k`.
fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

impl Crater {
    /// The ground `h` (m over the datum) at unit direction `d` on a body of radius `r`, once this
    /// crater is dug in it: levelled where it wipes, then the bowl, rim and blanket.
    pub fn apply(&self, h: f64, d: DVec3, r: f64) -> f64 {
        let v = d - self.dir;
        let len = v.length();
        let rr = len * r / self.radius;
        if rr >= REACH * (1.0 + WOBBLE) {
            return h;
        }
        let (wobble, lobes, _) = blast_pattern(if len > 0.0 { v / len } else { DVec3::X }, self.seed);
        let e = blast_erase(rr, wobble);
        h + (self.ground - h) * e + self.depth * blast_profile(rr, self.rim, wobble, lobes)
    }

    /// Ground distance (m) out to where it changes anything.
    pub fn reach(&self) -> f64 {
        self.radius * REACH * (1.0 + WOBBLE)
    }

    /// One crater holding this one and `o`, shaped like the bigger: their volumes (r^3) add, so
    /// blasts piling up widen and deepen the hole instead of drilling a shaft.
    pub fn merged(&self, o: &Crater) -> Crater {
        let big = if self.radius >= o.radius { self } else { o };
        let (a, b) = (self.radius.powi(3), o.radius.powi(3));
        let radius = (a + b).cbrt();
        Crater { dir: (self.dir * a + o.dir * b).normalize(), radius, depth: big.depth * radius / big.radius, rim: big.rim, seed: big.seed, ground: big.ground }
    }
}

/// A body's craters and the regions changed so far (version, centre, reach m).
pub struct Deform {
    craters: Vec<Crater>,
    log: Vec<(u64, DVec3, f64)>,
    version: u64,
}

impl Default for Deform {
    fn default() -> Self {
        Deform { craters: Vec::with_capacity(MAX_CRATERS), log: Vec::with_capacity(LOG), version: 0 }
    }
}

impl Deform {
    /// Dig crater `c` on a body of radius `r` (m); merged into an earlier one where it lands in it.
    pub fn add(&mut self, c: Crater, r: f64) {
        let near = |e: &Crater| {
            let (big, small) = if e.radius >= c.radius { (e.radius, c.radius) } else { (c.radius, e.radius) };
            (e.dir - c.dir).length() * r < MERGE_NEAR * big && big < small * MERGE_SIZE
        };
        if let Some(i) = self.craters.iter().position(near) {
            let e = self.craters.remove(i);
            let m = e.merged(&c);
            self.craters.push(m);
            self.changed(e.dir, e.reach());
            self.changed(m.dir, m.reach());
            return;
        }
        if self.craters.len() == MAX_CRATERS {
            let old = self.craters.remove(0);
            self.changed(old.dir, old.reach());
        }
        self.craters.push(c);
        self.changed(c.dir, c.reach());
    }

    fn changed(&mut self, dir: DVec3, reach: f64) {
        self.version += 1;
        if self.log.len() == LOG {
            self.log.remove(0);
        }
        self.log.push((self.version, dir, reach));
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn craters(&self) -> &[Crater] {
        &self.craters
    }

    /// Every crater as `craters` says (oldest first), in place of what there was: the ground as
    /// another game has it (a player who comes in late is told the server's).
    pub fn replace(&mut self, craters: &[Crater]) {
        let old = std::mem::take(&mut self.craters);
        for c in &old {
            self.changed(c.dir, c.reach());
        }
        self.craters.extend(craters.iter().take(MAX_CRATERS).copied());
        for c in craters.iter().take(MAX_CRATERS) {
            self.changed(c.dir, c.reach());
        }
    }

    /// Regions changed after version `seen`; None when the log no longer reaches back that far
    /// (the caller then refreshes everything).
    pub fn changes_since(&self, seen: u64) -> Option<impl Iterator<Item = (DVec3, f64)> + '_> {
        let first = self.log.first().map_or(u64::MAX, |c| c.0);
        (seen >= self.version || first <= seen + 1).then(|| self.log.iter().filter(move |c| c.0 > seen).map(|c| (c.1, c.2)))
    }

    /// The ground `h` (m over the datum) at unit direction `d` on a body of radius `r` with every
    /// crater dug in it, oldest first: a newer crater wipes the older ones inside it.
    pub fn apply(&self, h: f64, d: DVec3, r: f64) -> f64 {
        self.craters.iter().fold(h, |h, c| c.apply(h, d, r))
    }

    /// The craters that touch a cap (centre `d`, ground radius `m`) into `out`, oldest first (the
    /// order they are dug in; past `MAX_NODE_CRATERS` the biggest); returns how many.
    pub fn touching(&self, d: DVec3, m: f64, r: f64, out: &mut [Crater; MAX_NODE_CRATERS]) -> usize {
        let mut age = [0usize; MAX_NODE_CRATERS];
        let mut n = 0;
        for (i, c) in self.craters.iter().enumerate() {
            if (d - c.dir).length() * r > m + c.reach() {
                continue;
            }
            if n < MAX_NODE_CRATERS {
                (out[n], age[n]) = (*c, i);
                n += 1;
            } else if let Some(small) = (0..n).min_by(|&a, &b| out[a].radius.total_cmp(&out[b].radius)) {
                if out[small].radius < c.radius {
                    (out[small], age[small]) = (*c, i);
                }
            }
        }
        // back in the order they formed (a replaced slot broke it): insertion sort, n is small
        for i in 1..n {
            let mut j = i;
            while j > 0 && age[j - 1] > age[j] {
                age.swap(j - 1, j);
                out.swap(j - 1, j);
                j -= 1;
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f64 = 1_737_400.0;

    fn crater(at: DVec3) -> Crater {
        Crater { dir: at, radius: 6.0, depth: 2.0, rim: 0.25, seed: 0.37, ground: 0.0 }
    }

    #[test]
    fn profile_is_a_bowl_with_a_rounded_rim_and_no_step() {
        for (wobble, lobes) in [(0.0, 0.0), (0.8, 0.9), (-1.0, -1.0)] {
            let crest = 1.0 + WOBBLE * wobble;
            let p = |x: f64| blast_profile(x * crest, 0.25, wobble, lobes);
            assert!((p(0.0) + 1.0).abs() < 1e-9);
            // the crest: a little under the rim (rounded), the highest point, with no corner
            let top = (0..400).map(|i| p(0.8 + f64::from(i) * 0.001)).fold(f64::MIN, f64::max);
            assert!(top < 0.25 && top > 0.25 - CREST * 0.3, "crest {top}");
            let slope = |x: f64| (p(x + 1e-5) - p(x - 1e-5)) / 2e-5;
            // a corner would jump the slope by 3.25 (wall 2.5 up, blanket 0.75 down)
            assert!((slope(1.0 - 1e-4) - slope(1.0 + 1e-4)).abs() < 0.05, "a corner at the crest");
            assert!(p(REACH - 1e-6).abs() < 1e-9);
            assert_eq!(p(REACH + 1e-3), 0.0);
            // the blanket only thins outward, and is never a trench
            let (a, b) = (p(1.5), p(2.5));
            assert!(a > b && b > 0.0, "{a} {b}");
        }
    }

    #[test]
    fn a_crater_wipes_the_ground_it_is_dug_in() {
        let at = DVec3::Y;
        let c = Crater { ground: 10.0, ..crater(at) };
        let off = |m: f64| (at + DVec3::X * (m / R)).normalize();
        // a bump in the bowl is gone: the bowl is the same over any old ground
        assert!((c.apply(10.0, off(2.0), R) - c.apply(14.0, off(2.0), R)).abs() < 1e-9);
        assert!((c.apply(3.0, at, R) - (10.0 - 2.0)).abs() < 1e-9);
        // out past the rim the old ground shows through, and from the reach on it is untouched
        let near = c.apply(14.0, off(8.0), R) - c.apply(10.0, off(8.0), R);
        assert!(near > 0.0 && near < 4.0, "{near}");
        assert_eq!(c.apply(7.5, off(30.0), R), 7.5);
        // a later, smaller crater in it shows; an earlier one inside a later, bigger one does not
        let mut d = Deform::default();
        let small = Crater { radius: 1.0, depth: 0.4, ground: 0.0, ..crater(off(3.0)) };
        d.add(small, R);
        d.add(Crater { radius: 20.0, depth: 6.0, ..crater(at) }, R);
        let big_only = Crater { radius: 20.0, depth: 6.0, ..crater(at) }.apply(0.0, off(3.0), R);
        assert!((d.apply(0.0, off(3.0), R) - big_only).abs() < 1e-9);
    }

    #[test]
    fn no_two_craters_alike_and_never_a_circle() {
        let rims: Vec<f64> = (0..36)
            .map(|k| {
                let a = f64::from(k) * std::f64::consts::TAU / 36.0;
                blast_pattern(DVec3::new(a.cos(), 0.0, a.sin()), 0.37).0
            })
            .collect();
        let (lo, hi) = rims.iter().fold((f64::MAX, f64::MIN), |(l, h), &x| (l.min(x), h.max(x)));
        assert!(hi - lo > 0.4, "a rim that barely wobbles: {lo}..{hi}");
        let v = DVec3::new(0.6, 0.0, 0.8);
        assert_ne!(blast_pattern(v, 0.37), blast_pattern(v, 0.71));
        let (_, lobes, rays) = blast_pattern(v, 0.37);
        assert!((-1.0..=1.0).contains(&lobes) && (0.0..=1.0).contains(&rays));
    }

    #[test]
    fn craters_dig_where_they_land_and_log_the_change() {
        let mut d = Deform::default();
        let at = DVec3::Y;
        d.add(crater(at), R);
        assert!((d.apply(0.0, at, R) + 2.0).abs() < 1e-9);
        let far = (at + DVec3::X * (100.0 / R)).normalize();
        assert_eq!(d.apply(0.0, far, R), 0.0);
        let regions: Vec<_> = d.changes_since(0).unwrap().collect();
        assert_eq!(regions.len(), 1);
        assert!(d.changes_since(d.version()).unwrap().next().is_none());
        // two blasts in the same place: one crater, wider and deeper (their volumes add)
        d.add(crater(at), R);
        assert_eq!(d.craters().len(), 1);
        assert!((d.apply(0.0, at, R) + 2.0 * 2f64.cbrt()).abs() < 1e-9);
        assert!((d.craters()[0].radius - 6.0 * 2f64.cbrt()).abs() < 1e-9);
    }

    #[test]
    fn full_list_drops_the_oldest_and_says_so() {
        let mut d = Deform::default();
        for i in 0..MAX_CRATERS + 10 {
            d.add(crater((DVec3::Y + DVec3::X * (i as f64 * 50.0 / R)).normalize()), R);
        }
        assert_eq!(d.craters().len(), MAX_CRATERS);
        // a terrain that saw nothing is too far behind: refresh everything
        assert!(d.changes_since(0).is_none());
        assert!(d.changes_since(d.version() - 3).unwrap().count() == 3);
    }

    #[test]
    fn nodes_get_the_biggest_craters_that_touch_them() {
        let mut d = Deform::default();
        // a hundred craters 100 m apart (too far apart to merge), radii 1..100 m
        for i in 0..100 {
            let (x, z) = (f64::from(i % 10) - 4.5, f64::from(i / 10) - 4.5);
            let at = (DVec3::Y + DVec3::new(x, 0.0, z) * (100.0 / R)).normalize();
            d.add(Crater { radius: 1.0 + f64::from(i), ..crater(at) }, R);
        }
        assert_eq!(d.craters().len(), 100);
        let mut out = [crater(DVec3::Y); MAX_NODE_CRATERS];
        let n = d.touching(DVec3::Y, 1000.0, R, &mut out);
        assert_eq!(n, MAX_NODE_CRATERS);
        // the biggest, in the order they formed (here, smallest first)
        assert_eq!(out[0].radius, 100.0 - (MAX_NODE_CRATERS - 1) as f64);
        assert_eq!(out[n - 1].radius, 100.0);
        assert!(out[..n].windows(2).all(|w| w[0].radius < w[1].radius));
        let away = (DVec3::Y + DVec3::X * (5000.0 / R)).normalize();
        assert_eq!(d.touching(away, 10.0, R, &mut out), 0);
    }

    /// Two hundred bombs within 25 m: a handful of craters (every node draws them all, as the
    /// collision does), the hole far deeper than one bomb's.
    #[test]
    fn blasts_piling_up_stay_few_craters() {
        let mut d = Deform::default();
        for i in 0..200 {
            let a = f64::from(i) * 2.4;
            let at = (DVec3::Y + DVec3::new(a.cos(), 0.0, a.sin()) * (f64::from(i % 7) * 4.0 / R)).normalize();
            // as a blast does: its level is the ground there now, already dug by the ones before
            let ground = d.apply(0.0, at, R);
            d.add(Crater { dir: at, radius: 45.0, depth: 12.0, rim: 0.2, seed: 0.5, ground }, R);
        }
        let mut out = [crater(DVec3::Y); MAX_NODE_CRATERS];
        let n = d.touching(DVec3::Y, 30.0, R, &mut out);
        assert_eq!(n, d.craters().len());
        assert!(n * 2 < MAX_NODE_CRATERS, "{n} craters");
        assert!(d.apply(0.0, DVec3::Y, R) < -36.0, "{}", d.apply(0.0, DVec3::Y, R));
    }
}
