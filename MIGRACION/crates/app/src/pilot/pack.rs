//! The suit's jet pack (`PlayerDef::mochila`): Space pushes up, the keys one walks with push
//! sideways once off the ground, Ctrl pushes down; it burns its gas doing so and fills up again
//! aboard a ship. Hands off, with its steadying on, it brakes you and holds your height — to
//! whatever near you goes most as you do (`Hold`): the ship you left or came up to, as it goes
//! now, or the ground you hover over. Beside a ship that falls you fall with it, burning nothing;
//! if it speeds up the pack goes after it as it can. With the steadying off nothing a ship does
//! is anything to you. Every number is the pack's data.
//!
//! Its jets push along the body's own frame (up, down, level), whatever that is turned to, and
//! it holds you against what you weigh, whatever that is: where nothing weighs it has nothing
//! to hold up and burns nothing standing still. And where there is no ground and nothing near
//! goes as you do, it has nothing to steady you to (`Hold::Free`): it lets you be.
use super::{Input, Pilot};
use glam::DVec3;
use lunar_core::structure::{set::Structures, state::Structure};

/// What the pack steadies us to when the keys are let go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hold {
    /// What our speeds are counted in: aboard, the ship that carries us; else the ground.
    Still,
    /// Nothing: no ground under us (past every body's reach) and nothing near that goes as we
    /// do. It brakes nothing.
    Free,
    /// A speed (world, m/s): that of what we went beside, as it was when it went out of reach.
    Speed(DVec3),
    /// A structure near us, as it goes now: how fast (world, m/s) and how that changes (m/s²,
    /// smoothed).
    Beside { id: u64, vel: DVec3, gains: DVec3 },
}

impl Pilot {
    /// Whether the suit has a jet pack at all.
    pub fn has_pack(&self) -> bool {
        self.def.mochila.is_some()
    }

    /// The pack's full push up (N), for what it does to the ground under it.
    pub fn pack_thrust(&self) -> f64 {
        self.def.mochila.as_ref().map_or(0.0, |m| m.empuje * self.def.cuerpo.masa)
    }

    /// The jet pack on or off; what it is now (None: the suit has none).
    pub fn toggle_pack(&mut self) -> Option<bool> {
        self.def.mochila?;
        self.pack_on = !self.pack_on;
        Some(self.pack_on)
    }

    /// The pack's steadying on or off; what it is now.
    pub fn toggle_steady(&mut self) -> bool {
        self.steady = !self.steady;
        self.steady
    }

    /// The structure the pack keeps us beside in the air, if any (not the one that carries us:
    /// `ride`).
    pub fn beside(&self) -> Option<u64> {
        match self.hold {
            Hold::Beside { id, .. } => Some(id),
            _ => None,
        }
    }

    /// How what the pack keeps us to goes (m/s, as our own speeds are counted) and how that
    /// speeds up (m/s²). None: it keeps us to nothing.
    fn kept_to(&self) -> Option<(DVec3, DVec3)> {
        match self.hold {
            Hold::Still => Some((DVec3::ZERO, DVec3::ZERO)),
            Hold::Free => None,
            Hold::Speed(v) => Some((v, DVec3::ZERO)),
            Hold::Beside { vel, gains, .. } => Some((vel, gains)),
        }
    }

    /// How fast we go sideways (level) from what we stand on or the pack keeps us to (from
    /// nothing: not at all).
    pub(super) fn aside(&self, up: DVec3) -> DVec3 {
        let Some((goes, _)) = self.kept_to() else { return DVec3::ZERO };
        // (what we go up or down at is along `up`: no part of it is level)
        let rel = self.drift - goes;
        rel - up * rel.dot(up)
    }

    /// In the air on our own: what the pack keeps us to, looked at again. Whatever near us goes
    /// most as we do — a structure (as it goes now) or the ground, where there is ground — and
    /// another than now only if it goes clearly more as we do (`mochila.junto`). What we kept
    /// beside and is out of reach: the speed it had. Nothing of all that: nothing.
    pub(super) fn refer(&mut self, set: &Structures, dt: f64) {
        let Some(j) = self.def.mochila.map(|m| m.junto) else { return };
        let ours = self.drift + self.up * self.vertical_velocity;
        let near = |s: &Structure| s.to_world(s.center).distance(self.position) < f64::from(s.radius) + j.alcance;
        // (the ground is something to keep to only where there is ground)
        let ground = self.ground.map(|_| DVec3::ZERO);
        let mut pick: (Option<&Structure>, Option<DVec3>) = match self.hold {
            Hold::Beside { id, vel, .. } => match set.get(id).filter(|s| near(s)) {
                Some(s) => (Some(s), Some(s.vel)),
                None => (None, Some(vel)),
            },
            Hold::Speed(v) => (None, Some(v)),
            Hold::Still => (None, ground),
            Hold::Free => (None, None),
        };
        let mut least = pick.1.map_or(f64::INFINITY, |v| (ours - v).length() * j.mejora - j.margen);
        if ground.is_some() && ours.length() < least {
            (pick, least) = ((None, ground), ours.length());
        }
        for s in &set.list {
            let apart = (ours - s.vel).length();
            if apart < least && !s.anchored && !s.resting && near(s) {
                (pick, least) = ((Some(s), Some(s.vel)), apart);
            }
        }
        self.hold = match pick {
            (Some(s), _) => {
                // how it speeds up: by how its speed changed since the last slice, smoothed
                let vel = s.vel;
                let gains = match self.hold {
                    Hold::Beside { id, vel: was, gains } if id == s.id => (gains + ((vel - was) / dt - gains) * (1.0 - (-dt / j.suavizado).exp())).clamp_length_max(j.aceleracion),
                    _ => DVec3::ZERO,
                };
                Hold::Beside { id: s.id, vel, gains }
            }
            (None, Some(v)) if v == DVec3::ZERO && ground.is_some() => Hold::Still,
            (None, Some(v)) => Hold::Speed(v),
            (None, None) => Hold::Free,
        };
    }

    /// What the keys do to how we go, `dt` on: on our feet (or with no pack on), walking at
    /// `pace` the way `wish` says (level, unit or none), and a jump; in the air with the pack
    /// on, its jets. `g`: what of our weight presses on what is under our feet (m/s²; none
    /// where nothing weighs).
    pub(super) fn legs_or_jets(&mut self, dt: f64, input: &Input, up: DVec3, g: f64, wish: DVec3, pace: f64) {
        // off the ground you keep the speed you had and the pack's jets change it
        let pack = self.def.mochila.filter(|_| self.pack_on && self.fuel > 0.0);
        let mut burn = 0.0;
        let mut jets = DVec3::ZERO;
        let flying = !self.grounded && self.aloft;
        if self.grounded || pack.is_none() {
            self.position += wish * pace * dt;
            self.walked = if self.grounded { wish * pace } else { DVec3::ZERO };
        } else if !flying {
            // a step off a sill with the pack on: still walking (what we walked at is kept for
            // when it does take over)
            self.position += wish * pace * dt;
        } else if let Some(j) = pack {
            // off an edge: with the speed we walked at
            self.drift += std::mem::take(&mut self.walked);
            if wish != DVec3::ZERO {
                self.drift += wish * j.lateral * dt;
                burn += j.gasto_lado;
                jets += wish * j.gasto_lado;
            } else if self.steady {
                // hands off: it steadies you to what it keeps you to
                let side = self.aside(up);
                let l = side.length();
                if l > j.quieto {
                    let share = j.gasto_freno * (j.frenada / j.lateral.max(1e-6)).max(1.0);
                    self.drift -= side / l * (j.frenada * dt).min(l);
                    burn += share;
                    jets -= side / l * share;
                }
            }
        }
        if input.jump && self.grounded && !self.crouched() {
            self.vertical_velocity = (2. * g * self.def.jump_height).sqrt();
            self.grounded = false;
            // off the ground with the speed you ran at
            if pack.is_some() {
                self.drift += std::mem::take(&mut self.walked);
                self.aloft = true;
            }
        }
        if let Some(j) = pack {
            if input.vertical > 0.0 {
                if self.grounded {
                    self.drift += std::mem::take(&mut self.walked);
                    self.grounded = false;
                }
                self.vertical_velocity += j.empuje * dt;
                burn += 1.0;
                jets += up;
                self.aloft = true;
            } else if input.vertical < 0.0 && flying {
                self.vertical_velocity -= j.empuje * j.abajo * dt;
                burn += j.abajo;
                jets -= up * j.abajo;
            } else if let (true, true, Some((goes, gains))) = (flying, self.steady, self.kept_to()) {
                // hands off: it holds your height too — level with what it keeps you to, as that
                // climbs, sinks or falls — pushing against your weight and what you still carry
                // (kept to nothing, there is nothing to hold you level with)
                let climb = (goes - self.drift).dot(up) - self.vertical_velocity;
                let push = (g + gains.dot(up) + climb / j.sosten).clamp(-j.empuje * j.abajo, j.empuje);
                self.vertical_velocity += push * dt;
                burn += push.abs() / j.empuje.max(1e-6);
                jets += up * (push / j.empuje.max(1e-6));
            }
            self.fuel = (self.fuel - burn * dt / j.autonomia.max(1e-3)).max(0.0);
        }
        self.jet = burn.min(1.0);
        self.push = jets;
        // aboard a ship (standing on it, or in it) the pack fills up again
        if let Some(j) = self.def.mochila
            && burn == 0.0
            && self.ride.is_some()
        {
            self.fuel = (self.fuel + dt / j.recarga.max(1e-3)).min(1.0);
        }
    }
}
