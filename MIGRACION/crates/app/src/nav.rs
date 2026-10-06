//! Which way is which: the references one finds one's way by, wherever one is, and what the
//! compass shows of them (`assets/defs/navegacion.jsonc`).
//!
//! There is no one reference that means something everywhere: north means something on the
//! ground of a body, the way one is going and the way out from the body mean something round
//! it, and with nothing pulling only a ship or the sun do. So the compass is not a heading: it
//! is a tape round one's own way up on which every reference that means something now is marked,
//! each as solid as its regime weighs — over the ground, in orbit, in free space, by body. The
//! weights are smooth in where one is and how one goes, so from one regime to another the marks
//! of the one fade as those of the other come in, and nothing ever jumps. Where a reference
//! stops meaning anything (north at a pole, a direction straight overhead) its marks fade too:
//! there is no place where a reading flips.
//!
//! Nothing here knows a body or a ship by name, nor asks what happened before: it is all worked
//! out from where one is, how one goes and how one is turned, now. What the regimes are, what
//! each marks and what it is called is data.
use glam::DVec3;
use lunar_core::{
    body::{Body, BodyRegistry},
    defs::{self, DefError},
    structure::state::Structure,
};
use serde::Deserialize;
use std::{fmt::Write, path::Path};

/// A direction one can find one's way by.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    /// North on the ground of the body: toward its pole. None at its poles.
    Norte,
    /// The way one goes round the body.
    Marcha,
    /// Out from the body's centre.
    Radial,
    /// Square to the plane one goes round the body in.
    Normal,
    /// Where the nose of the ship of reference points.
    Proa,
    /// Toward the ship of reference.
    Nave,
    /// Toward the sun.
    Sol,
}

/// What gives a regime its weight.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Weighs {
    /// A body's share of where one is, as far as one is not going round it.
    Superficie,
    /// The same, as far as one is.
    Orbita,
    /// What no body has of where one is.
    Espacio,
}

/// A mark on the tape: the way `hacia` (the other way with `contra`), turned `giro` degrees
/// round one's way up toward the right. `texto` may say `{nave}` (the ship of reference) and
/// `{distancia}` (how far it is). `mayor`: one of the main ones (bigger). `borde`: out of the
/// tape's sight it waits at its edge, on the side to turn to.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkDef {
    pub hacia: Dir,
    #[serde(default)]
    pub contra: bool,
    #[serde(default)]
    pub giro: f32,
    pub texto: String,
    #[serde(default)]
    pub mayor: bool,
    #[serde(default)]
    pub borde: bool,
}

/// A regime: what gives it weight, the direction its scale of degrees is counted from (if it
/// has one) and what those figures are called, and its marks.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegimeDef {
    pub pesa: Weighs,
    #[serde(default)]
    pub escala: Option<Dir>,
    pub nombre: String,
    pub marcas: Vec<MarkDef>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavDef {
    /// Going round a body rather than over its ground: going level at these shares of the speed
    /// of a circular orbit where one is (under the first: not at all; from the second: wholly).
    pub orbita: [f64; 2],
    /// Near a pole north means less and less: the sine of the angle to the body's axis under
    /// which there is no north, and from which it is whole.
    pub polo: [f64; 2],
    /// A direction nearly straight up or down is nowhere on the tape: how level it is (the
    /// cosine of its rise) under which its mark is gone, and from which it is whole.
    pub vertical: [f64; 2],
    /// The ship of reference with none carrying us and none kept beside: the nearest within
    /// this far (m).
    pub nave: f64,
    pub regimenes: Vec<RegimeDef>,
}

/// A mark as the compass shows it.
#[derive(Clone, Debug, Default)]
pub struct Mark {
    /// Degrees to the right of where one looks (-180..180), and over one's own level.
    pub bearing: f32,
    pub rise: f32,
    pub text: String,
    pub major: bool,
    pub edge: bool,
    /// How solid (0..1).
    pub alpha: f32,
}

/// How many degrees to each side of where one looks the compass shows, and how near straight
/// behind (degrees) a mark that waits at the edge begins to go: right behind there is no side
/// to turn to, and it must not flip from one edge to the other.
pub const SPAN: f32 = 62.0;
const BEHIND: f32 = 25.0;
/// How solid a mark that waits at the edge is there.
const WAITING: f32 = 0.75;

impl Mark {
    /// Where on the tape it is drawn (degrees to the right of its middle, within `SPAN`), how
    /// solid, and whether it is waiting at the edge. Smooth in where the mark is: turning,
    /// nothing on the tape appears, goes or changes side at a stroke.
    pub fn shown(&self) -> (f32, f32, bool) {
        let out = self.bearing.abs() > SPAN;
        let off = self.bearing.clamp(-SPAN, SPAN);
        let along = (off / SPAN) * (off / SPAN);
        let solid = if self.edge {
            let t = ((180.0 - self.bearing.abs()) / BEHIND).clamp(0.0, 1.0);
            (1.0 - along * (1.0 - WAITING)) * t * t * (3.0 - 2.0 * t)
        } else {
            1.0 - along
        };
        (off, solid * self.alpha, out)
    }
}

/// A scale of degrees: how far the look is turned from its zero toward the right (0..360),
/// what its figures are called and how solid it is (0..1).
#[derive(Clone, Debug, Default)]
pub struct Scale {
    pub heading: f32,
    pub name: String,
    pub alpha: f32,
}

/// What the compass shows now. Filled anew every frame into the same store (`clear`, `mark`,
/// `scale`): nothing is allocated once it has held its most.
#[derive(Clone, Debug, Default)]
pub struct Compass {
    marks: Vec<Mark>,
    scales: Vec<Scale>,
    used: (usize, usize),
}

impl Compass {
    pub fn clear(&mut self) {
        self.used = (0, 0);
    }

    pub fn is_empty(&self) -> bool {
        self.used == (0, 0)
    }

    pub fn marks(&self) -> &[Mark] {
        &self.marks[..self.used.0]
    }

    pub fn scales(&self) -> &[Scale] {
        &self.scales[..self.used.1]
    }

    /// The scale that counts most (its figures are the ones shown), if any.
    pub fn main(&self) -> Option<&Scale> {
        self.scales().iter().max_by(|a, b| a.alpha.total_cmp(&b.alpha))
    }

    fn mark(&mut self) -> &mut Mark {
        if self.used.0 == self.marks.len() {
            self.marks.push(Mark::default());
        }
        self.used.0 += 1;
        let m = &mut self.marks[self.used.0 - 1];
        m.text.clear();
        m
    }

    fn scale(&mut self) -> &mut Scale {
        if self.used.1 == self.scales.len() {
            self.scales.push(Scale::default());
        }
        self.used.1 += 1;
        let s = &mut self.scales[self.used.1 - 1];
        s.name.clear();
        s
    }
}

/// Someone finding their way: where they are and how they go (world), their own way up and the
/// level way they look (unit, square to each other), the ship they go by if any, and the sun.
#[derive(Clone, Copy)]
pub struct Who<'a> {
    pub at: DVec3,
    pub vel: DVec3,
    pub up: DVec3,
    pub ahead: DVec3,
    pub ship: Option<&'a Structure>,
    pub sun: DVec3,
}

pub struct Nav {
    pub def: NavDef,
}

/// A weight under this shows nothing (it would not be seen).
const FAINT: f64 = 1e-3;

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Nav {
    pub fn load(path: &Path) -> Result<Nav, DefError> {
        let def: NavDef = defs::load(path)?;
        let bad = |m: &str| DefError::new("navegacion".to_string(), m.to_string());
        for pair in [def.orbita, def.polo, def.vertical] {
            if !(pair[0] >= 0.0 && pair[1] > pair[0]) {
                return Err(bad("orbita, polo y vertical: dos valores, el segundo mayor que el primero"));
            }
        }
        Ok(Nav { def })
    }

    /// The player as someone finding their way: where they are and how they go, their own way
    /// up (seated, the seat's) and the level way they look (`view`), and the ship they go by —
    /// the one that carries them, the one the pack keeps them beside, or the nearest.
    pub fn who<'a>(&self, p: &crate::pilot::Pilot, set: &'a lunar_core::structure::set::Structures, view: &lunar_render::View, sun: DVec3) -> Who<'a> {
        let up = match p.seat.and_then(|seat| set.get(seat.structure)) {
            Some(s) => (s.rot * glam::Vec3::Y).as_dvec3(),
            None => p.feet().1,
        };
        // (looking straight up or down: the way the top of the picture points, laid level)
        let flat = |v: DVec3| (v - up * v.dot(up)).try_normalize();
        let ahead = flat(view.forward).or_else(|| flat(view.up)).unwrap_or_else(|| up.any_orthonormal_vector());
        let ship = ship_of(set, p.ride.map(|r| r.id).or(p.beside()), p.position, self.def.nave);
        Who { at: p.position, vel: p.velocity_in(set), up, ahead, ship, sun }
    }

    /// How much someone at `alt` m over the datum of `b`, going `vel` (world), is going round it
    /// rather than over its ground (0..1).
    pub fn orbiting(&self, b: &Body, radial: DVec3, alt: f64, vel: DVec3) -> f64 {
        let level = (vel - radial * vel.dot(radial)).length();
        let round = (b.own_pull(alt) * (b.radius + alt)).sqrt();
        if round > 0.0 { smooth(self.def.orbita[0], self.def.orbita[1], level / round) } else { 1.0 }
    }

    /// What the compass shows `who` now.
    pub fn compass(&self, bodies: &BodyRegistry, who: &Who, out: &mut Compass) {
        out.clear();
        let mut held = 0.0;
        bodies.shares(who.at, |_, b, share, d, r| {
            held += share;
            let radial = d / r.max(f64::MIN_POSITIVE);
            let round = self.orbiting(b, radial, r - b.radius, who.vel);
            for regime in &self.def.regimenes {
                let weight = match regime.pesa {
                    Weighs::Superficie => share * (1.0 - round),
                    Weighs::Orbita => share * round,
                    Weighs::Espacio => 0.0,
                };
                self.show(regime, weight, who, Some((b, radial)), out);
            }
        });
        for regime in self.def.regimenes.iter().filter(|r| r.pesa == Weighs::Espacio) {
            self.show(regime, 1.0 - held, who, None, out);
        }
    }

    /// The way `dir` is for `who` (world, unit), and how much of a way it is (0..1).
    fn way(&self, dir: Dir, who: &Who, body: Option<(&Body, DVec3)>) -> Option<(DVec3, f64)> {
        match dir {
            Dir::Norte => body.map(|(b, radial)| b.north_at(radial)).filter(|n| n.1 > 0.0).map(|(n, much)| (n, smooth(self.def.polo[0], self.def.polo[1], much))),
            Dir::Marcha => who.vel.try_normalize().filter(|_| body.is_some()).map(|v| (v, 1.0)),
            Dir::Radial => body.map(|(_, radial)| (radial, 1.0)),
            Dir::Normal => body.and_then(|(_, radial)| radial.cross(who.vel).try_normalize()).map(|n| (n, 1.0)),
            Dir::Proa => who.ship.map(|s| ((s.rot * glam::Vec3::Z).as_dvec3(), 1.0)),
            Dir::Nave => who.ship.and_then(|s| {
                let to = s.to_world(s.center) - who.at;
                let r = f64::from(s.radius);
                // (in it, it is all round one: there is no way toward it)
                to.try_normalize().map(|d| (d, smooth(0.5 * r, r, to.length())))
            }),
            Dir::Sol => Some((who.sun, 1.0)),
        }
    }

    /// Degrees to the right of where `who` looks and over their level that `d` is, and how
    /// level it is (0..1).
    fn bearing(who: &Who, d: DVec3) -> (f64, f64, f64) {
        let rise = d.dot(who.up).clamp(-1.0, 1.0);
        let flat = d - who.up * rise;
        let right = who.ahead.cross(who.up);
        (flat.dot(right).atan2(flat.dot(who.ahead)).to_degrees(), rise.asin().to_degrees(), flat.length())
    }

    fn show(&self, regime: &RegimeDef, weight: f64, who: &Who, body: Option<(&Body, DVec3)>, out: &mut Compass) {
        if weight < FAINT {
            return;
        }
        let v = self.def.vertical;
        if let Some((d, much)) = regime.escala.and_then(|dir| self.way(dir, who, body)) {
            let (bearing, _, flat) = Nav::bearing(who, d);
            let alpha = weight * much * smooth(v[0], v[1], flat);
            if alpha >= FAINT {
                let s = out.scale();
                s.heading = (-bearing).rem_euclid(360.0) as f32;
                s.alpha = alpha as f32;
                s.name.push_str(&regime.nombre);
            }
        }
        for m in &regime.marcas {
            let Some((d, much)) = self.way(m.hacia, who, body) else { continue };
            let (bearing, rise, flat) = Nav::bearing(who, if m.contra { -d } else { d });
            let alpha = weight * much * smooth(v[0], v[1], flat);
            if alpha < FAINT {
                continue;
            }
            let mark = out.mark();
            mark.bearing = ((bearing + f64::from(m.giro) + 180.0).rem_euclid(360.0) - 180.0) as f32;
            mark.rise = rise as f32;
            (mark.major, mark.edge, mark.alpha) = (m.mayor, m.borde, alpha as f32);
            write_text(&mut mark.text, &m.texto, who);
        }
    }
}

/// `template` into `out`, `{nave}` and `{distancia}` filled from the ship of reference.
fn write_text(out: &mut String, template: &str, who: &Who) {
    let mut rest = template;
    while let Some(k) = rest.find('{') {
        out.push_str(&rest[..k]);
        let Some(end) = rest[k..].find('}') else { break };
        match (&rest[k + 1..k + end], who.ship) {
            ("nave", Some(s)) => out.push_str(&s.name),
            ("distancia", Some(s)) => {
                let d = (s.to_world(s.center).distance(who.at) - f64::from(s.radius)).max(0.0);
                let _ = if d < 1000.0 { write!(out, "{d:.0} m") } else { write!(out, "{:.1} km", d / 1000.0) };
            }
            _ => {}
        }
        rest = &rest[k + end + 1..];
    }
    out.push_str(rest);
}

/// The ship someone goes by: `first` (the one that carries them, or the one the pack keeps them
/// beside) if it is there; else the nearest ship within `within` m.
pub fn ship_of<'a>(set: &'a lunar_core::structure::set::Structures, first: Option<u64>, at: DVec3, within: f64) -> Option<&'a Structure> {
    first.and_then(|id| set.get(id)).or_else(|| {
        set.list
            .iter()
            .filter(|s| s.owner.is_some() && s.held.is_none())
            .map(|s| (s.to_world(s.center).distance(at) - f64::from(s.radius), s))
            .filter(|(d, _)| *d < within)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, s)| s)
    })
}

#[cfg(test)]
mod tests;
