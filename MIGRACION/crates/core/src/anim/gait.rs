//! Walking, step by step. Nothing here plays a walk cycle: each foot is ON the ground, at a
//! place of its own that does not move while the body goes on over it; when its turn comes it is
//! lifted and carried — in an arc, over what is in its way — to the place on the ground where it
//! will be needed next, found by asking the ground itself. So feet do not slide, they land on a
//! step or a slope where it is, they turn with the body when it turns on the spot, and they hang
//! and trail when there is no ground.
//!
//! The rhythm is the pendulum's: a leg swings in a time that grows as weight falls (about half
//! a second on the Moon against a third on Earth), a foot stays down while the body goes from
//! a stride behind it to a stride ahead, and when the swing takes longer than that — fast, or
//! with little weight — both feet are in the air between steps: the lope of the Apollo crews
//! comes out of it by itself.
//!
//! Everything is in "ground space": the frame of what is walked on (the world for the ground of
//! a body, a ship's own frame aboard it), so a foot on a deck goes with the deck. When what is
//! walked on changes, `rebase` takes the feet to the new frame.
use glam::DVec3;
use serde::Deserialize;

/// How a body walks (lengths in m, times in s).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct GaitDef {
    /// Half the distance between the feet.
    pub ancho: f64,
    /// Standing: how far from under its hip a foot may be before it is moved there, and how far
    /// (degrees) it may be turned from where the body faces.
    pub holgura: f64,
    pub giro: f64,
    /// Walking: how far ahead of under its hip a foot is set down (and how far behind it is
    /// lifted): at a standstill, and more per m/s; and the most it is.
    pub zancada: [f64; 2],
    pub zancada_max: f64,
    /// The length of a step (from one foot's print to the other's next): at a standstill, and
    /// more per m/s; and the most it is. With the speed it gives the rhythm: a cycle is two steps.
    pub paso: [f64; 2],
    pub paso_max: f64,
    /// The share of a cycle each foot is on the ground: at a walk (over a half: one foot is
    /// always down, and both for a moment) and at a run (under a half: both in the air between
    /// strides); and the speeds (m/s) between which it goes from the one to the other.
    pub apoyo: [f64; 2],
    pub correr: [f64; 2],
    /// Standing, how long a foot brought back to its place is in the air under 1 g; with less
    /// weight it is longer by (9.81/g)^`ingravidez`. And the limits of any foot's time in the
    /// air (a step's too).
    pub vuelo: f64,
    pub ingravidez: f64,
    pub vuelo_lim: [f64; 2],
    /// How high a foot is carried: at a standstill, and more per m/s; and the most.
    pub altura: [f64; 2],
    pub altura_max: f64,
    /// Slower than this (m/s) it is standing.
    pub quieto: f64,
    /// Off the ground: how far up from where they stand the feet hang, how far they trail behind
    /// the way one goes (s: times the speed; at most 0.3 m), and how fast they settle there
    /// (half-life).
    pub colgar: f64,
    pub arrastre: f64,
    pub muelle: f64,
}

impl Default for GaitDef {
    fn default() -> GaitDef {
        GaitDef { ancho: 0.11, holgura: 0.13, giro: 40.0, zancada: [0.2, 0.09], zancada_max: 0.46, paso: [0.34, 0.2], paso_max: 1.3, apoyo: [0.6, 0.4], correr: [2.6, 4.2], vuelo: 0.34, ingravidez: 0.25, vuelo_lim: [0.22, 0.62], altura: [0.07, 0.035], altura_max: 0.26, quieto: 0.15, colgar: 0.12, arrastre: 0.05, muelle: 0.12 }
    }
}

/// A foot: where its sole is and how it is turned.
#[derive(Clone, Copy, Debug, Default)]
pub struct Foot {
    /// Its sole (the point under the ankle), and the ground's way up there.
    pub at: DVec3,
    pub normal: DVec3,
    /// The way it points (level, unit).
    pub dir: DVec3,
    /// On the ground (else carried, or hanging).
    pub planted: bool,
    /// Carried: how far along its step (0..1). Planted: 0.
    pub swing: f64,
    /// Carried: how high over the line from where it left to where it lands (m).
    pub lift: f64,
    /// Planted: how far behind under its hip it is, along the way the body goes (m; negative:
    /// ahead of it).
    pub lag: f64,
    from: DVec3,
    from_dir: DVec3,
    to: DVec3,
    to_normal: DVec3,
    time: f64,
    arc: f64,
    /// How far over the straight line from where it left to where it lands the ground is, at
    /// even steps along it (made as one curve that rises and falls once), and where it was
    /// going when that was looked at.
    path: [f64; PATH + 1],
    path_to: DVec3,
    /// Hanging: where it is from the body and how fast it moves from it.
    rel: DVec3,
    v: DVec3,
}

/// Who walks, now.
#[derive(Clone, Copy, Debug)]
pub struct Walker {
    /// The point under the middle of the body at the level of its feet, the way up, the way it
    /// faces (level, unit) and how fast it goes over what it walks on.
    pub at: DVec3,
    pub up: DVec3,
    pub ahead: DVec3,
    pub vel: DVec3,
    /// On its feet.
    pub grounded: bool,
    /// Its weight per kilo (m/s²).
    pub g: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Gait {
    /// Left, right.
    pub feet: [Foot; 2],
    /// Walking: 0..1 through two steps: the left foot leaves the ground at 0, the right at 0.5.
    pub phase: f64,
    /// 0 standing .. 1 walking, eased: for what goes with the steps (arms, hips, shoulders).
    pub walk: f64,
    /// How far ahead of its hip a foot is set down now (m): the stride.
    pub stride: f64,
    /// Off the ground.
    pub airborne: bool,
    /// The feet set down in the last update: bit 0 the left, bit 1 the right.
    pub landed: u8,
    last: usize,
    walking: bool,
    started: bool,
}

/// How many stretches the way of a foot in the air is looked at in.
const PATH: usize = 8;

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `v` without what of it is along `up`.
fn flat(v: DVec3, up: DVec3) -> DVec3 {
    v - up * v.dot(up)
}

impl Gait {
    /// Both feet where they stand now, on the ground (a new body, one put somewhere else).
    pub fn stand(&mut self, def: &GaitDef, w: &Walker, ground: &mut dyn FnMut(DVec3) -> Option<(DVec3, DVec3)>) {
        let left = w.up.cross(w.ahead).normalize_or(w.up.any_orthonormal_vector());
        for (i, f) in self.feet.iter_mut().enumerate() {
            let home = w.at + left * if i == 0 { def.ancho } else { -def.ancho };
            let (at, normal) = ground(home).unwrap_or((home, w.up));
            *f = Foot { at, normal, dir: w.ahead, planted: true, from: at, from_dir: w.ahead, to: at, to_normal: normal, ..Foot::default() };
        }
        (self.airborne, self.walking, self.started, self.landed) = (false, false, true, 0);
    }

    /// The feet taken to another frame (what is walked on changed): `point` and `dir` take a
    /// point and a direction of the old ground space to the new.
    pub fn rebase(&mut self, point: impl Fn(DVec3) -> DVec3, dir: impl Fn(DVec3) -> DVec3) {
        for f in &mut self.feet {
            (f.at, f.from, f.to) = (point(f.at), point(f.from), point(f.to));
            (f.normal, f.to_normal, f.dir, f.from_dir, f.v) = (dir(f.normal), dir(f.to_normal), dir(f.dir), dir(f.from_dir), dir(f.v));
        }
    }

    /// `dt` seconds on. `ground`: the ground at a point (given at about the level of the feet):
    /// the point of it over or under that one and its way up there, or None where there is none
    /// within a step.
    pub fn update(&mut self, def: &GaitDef, w: &Walker, dt: f64, ground: &mut dyn FnMut(DVec3) -> Option<(DVec3, DVec3)>) {
        self.landed = 0;
        let left = w.up.cross(w.ahead).normalize_or(w.up.any_orthonormal_vector());
        let level = flat(w.vel, w.up);
        let speed = level.length();
        let way = if speed > 1e-6 { level / speed } else { w.ahead };
        let home = |i: usize| w.at + left * if i == 0 { def.ancho } else { -def.ancho };
        // (not started, or taken somewhere else at once: on its feet where it is)
        if !self.started || self.feet.iter().enumerate().any(|(i, f)| f.at.distance_squared(home(i)) > 9.0) {
            self.stand(def, w, ground);
        }
        let moving = speed > def.quieto;
        self.walk += ((if moving && w.grounded { 1.0 } else { 0.0 }) - self.walk) * (1.0 - (-dt / 0.12).exp());

        // ---- off the ground: the feet hang under the hips and trail
        if !w.grounded {
            let k = def.muelle.max(1e-3);
            let y = 4.0 * std::f64::consts::LN_2 / k / 2.0;
            let e = (-y * dt).exp();
            for (i, f) in self.feet.iter_mut().enumerate() {
                if f.planted || !self.airborne {
                    // (leaving the ground: from where it was, going with the body)
                    (f.rel, f.v) = (f.at - w.at, DVec3::ZERO);
                }
                f.planted = false;
                // a little uneven (nobody hangs with the feet together), trailing what one goes
                let trail = (w.vel * def.arrastre).clamp_length_max(0.3);
                let goal = home(i) + w.up * (def.colgar + if i == 0 { 0.035 } else { 0.0 }) - trail + w.ahead * if i == 0 { 0.05 } else { -0.03 } - w.at;
                // (a spring on where it is from the body: the body may be going fast)
                let j0 = f.rel - goal;
                let j1 = f.v + j0 * y;
                f.rel = goal + (j0 + j1 * dt) * e;
                f.v = (f.v - j1 * y * dt) * e;
                f.at = w.at + f.rel;
                f.dir = (f.dir + (w.ahead - f.dir) * (1.0 - e)).normalize_or(w.ahead);
                f.normal = w.up;
                (f.swing, f.lift, f.lag) = (0.0, 0.0, 0.0);
            }
            (self.airborne, self.walking) = (true, false);
            return;
        }
        if self.airborne {
            // down: each foot on the ground under it
            for (i, f) in self.feet.iter_mut().enumerate() {
                let under = f.at - w.up * (f.at - w.at).dot(w.up);
                let (at, normal) = ground(under).unwrap_or((under, w.up));
                (f.at, f.normal, f.planted, f.v) = (at, normal, true, DVec3::ZERO);
                self.landed |= 1 << i;
            }
            self.airborne = false;
        }

        // ---- the rhythm: a cycle is two steps; each foot is down for its share of it (at a
        // walk more than half: one foot after the other, never both in the air) and in the air
        // for the rest
        let cycle = 2.0 * (def.paso[0] + def.paso[1] * speed).min(def.paso_max) / speed.max(def.quieto.max(1e-3));
        let run = smooth((speed - def.correr[0]) / (def.correr[1] - def.correr[0]).max(1e-3));
        let duty = def.apoyo[0] + (def.apoyo[1] - def.apoyo[0]) * run;
        let time = if moving { (1.0 - duty) * cycle } else { def.vuelo * (9.81 / w.g.max(0.05)).powf(def.ingravidez) }.clamp(def.vuelo_lim[0], def.vuelo_lim[1]);
        let stride = if moving { (def.zancada[0] + def.zancada[1] * speed).min(def.zancada_max) } else { 0.0 };
        self.stride = stride;
        let arc = (def.altura[0] + def.altura[1] * speed).min(def.altura_max);

        // ---- the feet in the air go on toward where they will be needed
        for i in 0..2 {
            if self.feet[i].planted {
                continue;
            }
            let f = &mut self.feet[i];
            f.swing = (f.swing + dt / f.time.max(1e-3)).min(1.0);
            let left_to_go = (1.0 - f.swing) * f.time;
            // under its hip when it lands, and a stride ahead of that
            let aim = home(i) + level * left_to_go + way * stride;
            let aim = aim - w.up * (aim - w.at).dot(w.up);
            let (to, normal) = ground(aim).unwrap_or((aim, w.up));
            (f.to, f.to_normal) = (to, normal);
            let s = f.swing;
            let across = flat(f.to - f.from, w.up);
            let rise = (f.to - f.from).dot(w.up);
            // what the ground does between the two, against the straight line from one to the
            // other: the foot is carried over all of it (a step's edge, a sill, a pipe on the
            // deck). Looked at when the foot leaves and again if where it is going moves much.
            if f.path_to.distance_squared(f.to) > 0.02 {
                // the ground's height over where the foot left, at even steps along its way
                let mut high = [0.0f64; PATH + 1];
                high[PATH] = rise;
                for (k, h) in high.iter_mut().enumerate().take(PATH).skip(1) {
                    let u = k as f64 / PATH as f64;
                    let on_line = f.from + across * u + w.up * (rise * u);
                    *h = ground(on_line).map_or(rise * u, |(p, _)| (p - f.from).dot(w.up));
                }
                // how far over the line the foot must be at each: over the highest ground of
                // the stretch before it and the one after (an edge is somewhere in a stretch).
                // At its ends: what it must rise before it goes anywhere, and come down once
                // it is there
                let mut over = [0.0f64; PATH + 1];
                for k in 1..PATH {
                    over[k] = (high[k - 1].max(high[k]).max(high[k + 1]) - rise * k as f64 / PATH as f64).max(0.0);
                }
                over[0] = (high[1] - high[0]).max(0.0);
                over[PATH] = (high[PATH - 1] - high[PATH]).max(0.0);
                // (one curve over them all that rises to its top and falls from it)
                let top = (0..=PATH).max_by(|&a, &b| over[a].total_cmp(&over[b])).unwrap_or(0);
                for k in 1..top {
                    over[k] = over[k].max(over[k - 1]);
                }
                for k in (top + 1..PATH).rev() {
                    over[k] = over[k].max(over[k + 1]);
                }
                (f.path, f.path_to) = (over, f.to);
            }
            // up against something higher it rises before it goes anywhere, and by the edge of
            // a drop it goes out over it before it comes down
            let wait = if f.path[0] > 0.02 { 0.22 } else { 0.0 };
            let drop = if f.path[PATH] > 0.02 { 0.18 } else { 0.0 };
            let along = smooth((s - wait) / (1.0 - wait - drop));
            let gate = if s < wait {
                smooth(s / wait)
            } else if s > 1.0 - drop {
                smooth((1.0 - s) / drop)
            } else {
                1.0
            };
            let x = along * PATH as f64;
            let k = (x.floor() as usize).min(PATH - 1);
            let over = (f.path[k] + (f.path[k + 1] - f.path[k]) * (x - k as f64)) * gate;
            // (it leaves the ground briskly and comes down on it softly)
            f.lift = over + f.arc * (std::f64::consts::PI * s.powf(0.8)).sin().max(0.0).powf(1.4);
            f.at = f.from + across * along + w.up * (rise * along + f.lift);
            f.dir = flat(f.from_dir + (w.ahead - f.from_dir) * smooth(s), w.up).normalize_or(w.ahead);
            f.normal = w.up;
            if f.swing >= 1.0 {
                (f.at, f.normal, f.dir, f.planted, f.swing, f.lift) = (f.to, f.to_normal, w.ahead, true, 0.0, 0.0);
                self.landed |= 1 << i;
            }
        }
        for i in 0..2 {
            self.feet[i].lag = if self.feet[i].planted { (home(i) - self.feet[i].at).dot(way) } else { 0.0 };
        }

        // ---- which foot leaves the ground now
        let mut lift: Option<usize> = None;
        if moving {
            if !self.walking {
                // setting off: the foot that is further behind goes first
                let first = match self.feet[0].lag - self.feet[1].lag {
                    d if d > 0.02 => 0,
                    d if d < -0.02 => 1,
                    _ => 1 - self.last,
                };
                self.phase = if first == 0 { 0.0 } else { 0.5 };
                self.walking = true;
                lift = Some(first);
            } else {
                // (the left foot leaves the ground as the cycle begins, the right at its half)
                let before = self.phase;
                self.phase = (self.phase + dt / cycle).fract();
                if self.phase < before {
                    lift = Some(0);
                } else if before < 0.5 && self.phase >= 0.5 {
                    lift = Some(1);
                }
                // (left too far behind — the pace changed —: it goes now)
                for i in 0..2 {
                    if lift.is_none() && self.feet[i].planted && self.feet[1 - i].planted && self.feet[i].lag > (speed * duty * cycle - stride) * 1.35 + 0.12 {
                        lift = Some(i);
                        self.phase = if i == 0 { 0.0 } else { 0.5 };
                    }
                }
            }
        } else {
            self.walking = false;
            // standing: a foot out of its place, or turned away from where the body faces, is
            // brought back, one at a time
            if self.feet.iter().all(|f| f.planted) {
                let off = |i: usize| {
                    let f = &self.feet[i];
                    let far = flat(home(i) - f.at, w.up).length() / def.holgura.max(1e-3);
                    let turned = f.dir.dot(w.ahead).clamp(-1.0, 1.0).acos().to_degrees() / def.giro.max(1e-3);
                    far.max(turned)
                };
                let (a, b) = (off(0), off(1));
                if a.max(b) > 1.0 {
                    lift = Some(if (a - b).abs() < 0.05 {
                        1 - self.last
                    } else if a > b {
                        0
                    } else {
                        1
                    });
                }
            }
        }
        if let Some(i) = lift.filter(|&i| self.feet[i].planted) {
            let f = &mut self.feet[i];
            (f.planted, f.swing, f.lift) = (false, 0.0, 0.0);
            (f.from, f.from_dir, f.to, f.to_normal) = (f.at, f.dir, f.at, f.normal);
            // (its way is looked at once it knows where it goes)
            f.path_to = DVec3::splat(f64::MAX);
            f.time = if moving { time } else { time * 0.7 };
            f.arc = if moving { arc } else { arc * 0.6 };
            self.last = i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOON: f64 = 1.62;

    fn flat_ground(p: DVec3) -> Option<(DVec3, DVec3)> {
        Some((DVec3::new(p.x, 0.0, p.z), DVec3::Y))
    }

    fn walker(at: DVec3, vel: DVec3) -> Walker {
        Walker { at, up: DVec3::Y, ahead: DVec3::Z, vel, grounded: true, g: MOON }
    }

    /// Walks `secs` at `vel` from `at`: for each update, (time, walker, gait).
    fn walk(g: &mut Gait, def: &GaitDef, at: &mut DVec3, vel: DVec3, secs: f64, ground: &mut dyn FnMut(DVec3) -> Option<(DVec3, DVec3)>, mut see: impl FnMut(f64, &Walker, &Gait)) {
        let dt = 1.0 / 120.0;
        let mut t = 0.0;
        while t < secs {
            *at += vel * dt;
            let w = walker(*at, vel);
            g.update(def, &w, dt, ground);
            t += dt;
            see(t, &w, g);
        }
    }

    #[test]
    fn standing_the_feet_are_under_the_hips_and_stay_there() {
        let def = GaitDef::default();
        let mut g = Gait::default();
        let mut at = DVec3::new(3.0, 0.0, -2.0);
        let mut moved = 0;
        let mut ground = flat_ground;
        walk(&mut g, &def, &mut at, DVec3::ZERO, 3.0, &mut ground, |_, _, g| moved += usize::from(!g.feet[0].planted || !g.feet[1].planted));
        assert_eq!(moved, 0, "standing still it shuffles");
        assert!((g.feet[0].at - DVec3::new(3.11, 0.0, -2.0)).length() < 1e-9 && (g.feet[1].at - DVec3::new(2.89, 0.0, -2.0)).length() < 1e-9, "{:?}", g.feet);
        assert!(g.walk < 1e-6);
    }

    #[test]
    fn walking_each_foot_stays_where_it_is_set_down_and_they_take_turns() {
        let def = GaitDef::default();
        let mut g = Gait::default();
        let mut at = DVec3::ZERO;
        let vel = DVec3::new(0.0, 0.0, 1.8);
        // per foot: where it was while planted, its landings (time, how far ahead of its hip),
        // its liftoffs (how far behind)
        let mut was: [Option<DVec3>; 2] = [None, None];
        let mut slid = 0.0f64;
        let mut landings: Vec<(usize, f64, f64)> = Vec::new();
        let mut lifts: Vec<(usize, f64)> = Vec::new();
        let (mut both_up, mut highest) = (0.0, 0.0f64);
        let mut down = [true, true];
        let mut ground = flat_ground;
        walk(&mut g, &def, &mut at, vel, 12.0, &mut ground, |t, w, g| {
            for i in 0..2 {
                let f = &g.feet[i];
                if f.planted {
                    if let Some(p) = was[i] {
                        slid = slid.max(p.distance(f.at));
                    }
                    was[i] = Some(f.at);
                    if !down[i] {
                        landings.push((i, t, (f.at - w.at).z));
                    }
                } else {
                    if let (true, Some(p)) = (down[i], was[i]) {
                        lifts.push((i, (w.at - p).z));
                    }
                    was[i] = None;
                    highest = highest.max(f.at.y);
                    assert!(f.at.y > -1e-9, "a foot under the ground: {}", f.at.y);
                }
                down[i] = f.planted;
            }
            if !g.feet[0].planted && !g.feet[1].planted {
                both_up += 1.0 / 120.0;
            }
        });
        assert!(slid < 1e-9, "a planted foot slides {slid} m");
        // they take turns
        assert!(landings.len() > 12, "{} steps in 12 s", landings.len());
        for w in landings.windows(2) {
            assert_ne!(w[0].0, w[1].0, "the same foot twice running: {landings:?}");
        }
        // set down a stride ahead; lifted as far behind as the body has gone over it meanwhile
        // (it is down its share of the cycle): within what a leg reaches
        let stride = def.zancada[0] + def.zancada[1] * 1.8;
        let step = def.paso[0] + def.paso[1] * 1.8;
        let behind_at_lift = def.apoyo[0] * 2.0 * step - stride;
        for &(_, _, ahead) in &landings[2..] {
            assert!((ahead - stride).abs() < 0.03, "set down {ahead:.3} m ahead (stride {stride:.3})");
        }
        for &(_, behind) in &lifts[2..] {
            assert!((behind - behind_at_lift).abs() < 0.06 && behind < 0.56, "lifted {behind:.3} m behind (expected {behind_at_lift:.3})");
        }
        // a walk's pace: two or three steps a second of two thirds of a metre, one foot after
        // the other (never both in the air), the feet carried a hand over the ground
        let rate = (landings.len() - 1) as f64 / (landings[landings.len() - 1].1 - landings[0].1);
        eprintln!("{rate:.2} pasos/s, {:.2} m por paso, {:.0} % del tiempo en el aire, pie a {highest:.2} m", 1.8 / rate, both_up / 12.0 * 100.0);
        assert!(rate > 1.8 && rate < 2.9 && (1.8 / rate - step).abs() < 0.03, "{rate} steps a second");
        assert!(both_up < 0.02, "at a walk both feet were in the air for {both_up} s");
        assert!(highest > 0.08 && highest < 0.3);
        assert!(g.walk > 0.99);
    }

    #[test]
    fn at_a_walk_one_foot_is_always_down_whatever_the_weight_and_at_a_run_both_are_in_the_air() {
        let def = GaitDef::default();
        let dt = 1.0 / 120.0;
        // (updates with both feet in the air, with both down, and steps taken, in 10 s)
        let go = |speed: f64, g_: f64| {
            let mut g = Gait::default();
            let mut at = DVec3::ZERO;
            let (mut both_up, mut both_down, mut steps) = (0, 0, 0);
            for k in 0..1200 {
                at += DVec3::new(0.0, 0.0, speed) * dt;
                g.update(&def, &Walker { at, up: DVec3::Y, ahead: DVec3::Z, vel: DVec3::new(0.0, 0.0, speed), grounded: true, g: g_ }, dt, &mut flat_ground);
                if k > 120 {
                    both_up += usize::from(!g.feet[0].planted && !g.feet[1].planted);
                    both_down += usize::from(g.feet[0].planted && g.feet[1].planted);
                    steps += g.landed.count_ones();
                }
            }
            (both_up, both_down, steps)
        };
        for g_ in [9.81, MOON, 0.3] {
            for speed in [0.8, 1.3, 1.8, 2.4] {
                let (up, down, steps) = go(speed, g_);
                assert_eq!(up, 0, "at {speed} m/s under {g_} m/s²: {up} updates with both feet in the air");
                // one foot after the other: both down for a moment between steps, and a
                // rhythm of a walk (not a shuffle, not a leap)
                let per_s = f64::from(steps) / 9.0;
                assert!(down > 100 && (1.0..=3.6).contains(&per_s), "at {speed} m/s under {g_}: {down} with both down, {per_s:.2} steps a second");
            }
        }
        let (up, _, _) = go(4.6, MOON);
        assert!(up > 100, "at a run there is a moment in the air between strides ({up})");
    }

    #[test]
    fn stopping_the_feet_come_to_rest_under_the_hips() {
        let def = GaitDef::default();
        let mut g = Gait::default();
        let mut at = DVec3::ZERO;
        let mut ground = flat_ground;
        walk(&mut g, &def, &mut at, DVec3::new(1.0, 0.0, 1.5), 4.0, &mut ground, |_, _, _| {});
        walk(&mut g, &def, &mut at, DVec3::ZERO, 3.0, &mut ground, |_, _, _| {});
        for (i, f) in g.feet.iter().enumerate() {
            let home = at + DVec3::X * if i == 0 { def.ancho } else { -def.ancho };
            assert!(f.planted && f.at.distance(home) <= def.holgura + 1e-6, "foot {i} rests {:.3} m from its place", f.at.distance(home));
        }
        assert!(g.walk < 0.01);
    }

    #[test]
    fn turning_on_the_spot_the_feet_turn_with_the_body() {
        let def = GaitDef::default();
        let mut g = Gait::default();
        let at = DVec3::ZERO;
        let dt = 1.0 / 120.0;
        let mut steps = 0;
        let mut yaw = 0.0f64;
        for k in 0..600 {
            if k < 240 {
                yaw += 1.6 * dt;
            }
            let ahead = DVec3::new(yaw.sin(), 0.0, yaw.cos());
            g.update(&def, &Walker { at, up: DVec3::Y, ahead, vel: DVec3::ZERO, grounded: true, g: MOON }, dt, &mut flat_ground);
            steps += g.landed.count_ones();
        }
        let ahead = DVec3::new(yaw.sin(), 0.0, yaw.cos());
        assert!(steps >= 4, "{steps} steps to turn {:.0} degrees", yaw.to_degrees());
        for f in &g.feet {
            assert!(f.planted && f.dir.dot(ahead) > (def.giro.to_radians()).cos() - 1e-6, "a foot left pointing {:.0} degrees off", f.dir.dot(ahead).acos().to_degrees());
        }
    }

    #[test]
    fn off_the_ground_the_feet_hang_and_they_come_down_on_it() {
        let def = GaitDef::default();
        let mut g = Gait::default();
        let mut at = DVec3::ZERO;
        let mut ground = flat_ground;
        walk(&mut g, &def, &mut at, DVec3::new(0.0, 0.0, 1.8), 2.0, &mut ground, |_, _, _| {});
        // up and along at speed
        let dt = 1.0 / 120.0;
        let vel = DVec3::new(0.0, 0.0, 6.0);
        for k in 0..240 {
            at += vel * dt + DVec3::Y * if k < 120 { 0.02 } else { 0.0 };
            g.update(&def, &Walker { at, up: DVec3::Y, ahead: DVec3::Z, vel, grounded: false, g: MOON }, dt, &mut ground);
            assert!(!g.feet[0].planted && !g.feet[1].planted && g.landed == 0);
        }
        assert!(g.airborne);
        for f in &g.feet {
            let rel = f.at - at;
            assert!(rel.y > 0.08 && rel.y < 0.2 && rel.z < -0.1 && rel.z > -0.4 && rel.x.abs() < 0.15, "hanging at {rel:.2?} from the body");
        }
        // down again: both on the ground at once, under where they hung
        at.y = 0.0;
        g.update(&def, &Walker { at, up: DVec3::Y, ahead: DVec3::Z, vel, grounded: true, g: MOON }, dt, &mut ground);
        assert_eq!(g.landed, 3);
        // (coming down with speed one of them is already off again for the first step)
        assert!(g.feet.iter().all(|f| f.at.y.abs() < 1e-9) && g.feet.iter().any(|f| f.planted) && !g.airborne);
    }

    #[test]
    fn a_step_in_the_way_is_stepped_on_and_the_foot_goes_over_its_edge() {
        let def = GaitDef::default();
        let mut g = Gait::default();
        // the ground is 0.3 m higher from z = 2 on
        let mut ground = |p: DVec3| Some((DVec3::new(p.x, if p.z >= 2.0 { 0.3 } else { 0.0 }, p.z), DVec3::Y));
        let mut at = DVec3::ZERO;
        let dt = 1.0 / 120.0;
        let vel = DVec3::new(0.0, 0.0, 1.8);
        let mut lowest_over_edge = f64::MAX;
        let mut on_top = [false; 2];
        let mut before = [DVec3::ZERO; 2];
        for _ in 0..360 {
            at += vel * dt;
            // (the body is on the step once its middle is over it, as the walker's physics has it)
            at.y = if at.z >= 2.0 { 0.3 } else { 0.0 };
            g.update(&def, &Walker { at, up: DVec3::Y, ahead: DVec3::Z, vel, grounded: true, g: MOON }, dt, &mut ground);
            for (i, f) in g.feet.iter().enumerate() {
                if f.planted {
                    let want = if f.at.z >= 2.0 { 0.3 } else { 0.0 };
                    assert!((f.at.y - want).abs() < 1e-9, "a foot planted at {:.2?}: not on the ground", f.at);
                    on_top[i] |= f.at.z >= 2.0;
                }
                // as it goes over the edge of the step it is over the step
                if before[i].z < 2.0 && f.at.z >= 2.0 {
                    lowest_over_edge = lowest_over_edge.min(before[i].y.min(f.at.y));
                }
                before[i] = f.at;
            }
        }
        assert!(on_top[0] && on_top[1], "the feet never got on the step");
        assert!(lowest_over_edge > 0.3, "a foot went through the edge of the step at {lowest_over_edge:.3} m");
    }

    #[test]
    fn up_a_slope_the_feet_follow_it_without_kicking_up_behind() {
        let def = GaitDef::default();
        let mut g = Gait::default();
        // a ramp of 1 in 3 from z = 1 on
        let h = |z: f64| ((z - 1.0) / 3.0).max(0.0);
        let mut ground = |p: DVec3| Some((DVec3::new(p.x, h(p.z), p.z), DVec3::Y));
        let mut at = DVec3::ZERO;
        let dt = 1.0 / 120.0;
        let vel = DVec3::new(0.0, 0.0, 1.8);
        let mut highest = 0.0f64;
        for _ in 0..480 {
            at += vel * dt;
            at.y = h(at.z);
            g.update(&def, &Walker { at, up: DVec3::Y, ahead: DVec3::Z, vel, grounded: true, g: MOON }, dt, &mut ground);
            for f in &g.feet {
                let over = f.at.y - h(f.at.z);
                assert!(over > -1e-6, "a foot under the ramp by {:.3} m", -over);
                highest = highest.max(over);
                if f.planted {
                    assert!(over.abs() < 1e-9, "a foot planted off the ramp");
                }
            }
        }
        assert!(at.y > 1.5, "it went up the ramp");
        assert!(highest < 0.3, "a foot was carried {highest:.2} m over the ramp: a kick, not a step");
    }

    #[test]
    fn taken_to_another_frame_the_feet_keep_their_places() {
        let def = GaitDef::default();
        let mut g = Gait::default();
        let mut at = DVec3::ZERO;
        let mut ground = flat_ground;
        walk(&mut g, &def, &mut at, DVec3::new(0.0, 0.0, 1.8), 1.3, &mut ground, |_, _, _| {});
        let before = g.feet;
        let shift = DVec3::new(100.0, 0.0, -40.0);
        g.rebase(|p| p + shift, |d| d);
        for (a, b) in before.iter().zip(&g.feet) {
            assert!((a.at + shift).distance(b.at) < 1e-12 && a.dir == b.dir && a.planted == b.planted);
        }
        // and walking goes on from there without a jump
        let mut at2 = at + shift;
        let mut ground2 = |p: DVec3| Some((DVec3::new(p.x, 0.0, p.z), DVec3::Y));
        let mut far = 0.0f64;
        walk(&mut g, &def, &mut at2, DVec3::new(0.0, 0.0, 1.8), 1.0, &mut ground2, |_, w, g| {
            for f in &g.feet {
                far = far.max(flat(f.at - w.at, DVec3::Y).length());
            }
        });
        assert!(far < 0.9, "after changing frame a foot is {far:.2} m from the body");
    }
}
