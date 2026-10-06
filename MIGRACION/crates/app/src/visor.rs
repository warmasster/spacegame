//! The helmet's visor: what the picture is seen through from the player's own eyes, and how it
//! answers to what happens (`assets/defs/visor.jsonc`):
//!
//! - its **filter** for welding is lowered by hand with the welder: its right button (the same
//!   that shows the integrity of what is round) darkens it, and again clears it. Dark, the arc
//!   and its sparks are seen sharp instead of burning the picture white. It never darkens by
//!   itself;
//! - the **breath** mists it from its rim inward with effort — running, the pack's push, a jump
//!   landed, the suit's oxygen running out — and clears at rest.
//!
//! All of it is a few numbers handed to the tone mapping (`lunar_render::Visor`): no pass, no
//! texture, nothing allocated. Seen from outside (the third-person view, a camera of a script)
//! there is no visor in the picture; what it was doing goes on meanwhile.
use crate::{
    hud::{Hud, Level},
    pilot::{Input, Pilot},
};
use lunar_render::{Renderer, View};
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisorDef {
    pub filtro: FilterDef,
    pub aliento: BreathDef,
}

/// The welding filter (`visor.jsonc` says what each number is).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterDef {
    pub oscuro: f32,
    pub tinte: [f32; 3],
    pub rodilla: f32,
    pub oscurece: f32,
    pub aclara: f32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BreathDef {
    pub andar: f32,
    pub correr: f32,
    pub mochila: f32,
    pub salto: f32,
    pub sube: f32,
    pub baja: f32,
    pub desde: f32,
    pub tope: f32,
    pub oxigeno_bajo: f32,
    pub ahogo: f32,
}

impl VisorDef {
    pub fn check(&self) -> Result<(), String> {
        let (f, a) = (&self.filtro, &self.aliento);
        let share = |x: f32| (0.0..=1.0).contains(&x);
        if !(share(f.oscuro) && f.oscuro > 0.0 && f.rodilla >= 0.0 && f.oscurece > 0.0 && f.aclara > 0.0) {
            return Err("filtro: números fuera de lo que pueden ser".into());
        }
        if !(a.sube > 0.0 && a.baja > 0.0 && share(a.desde) && a.desde < 1.0 && share(a.tope) && share(a.oxigeno_bajo) && a.andar >= 0.0 && a.correr >= 0.0 && a.mochila >= 0.0 && a.salto >= 0.0 && a.ahogo >= 0.0) {
            return Err("aliento: números fuera de lo que pueden ser".into());
        }
        if f.tinte.iter().any(|c| !share(*c)) {
            return Err("tinte: cada color entre 0 y 1".into());
        }
        Ok(())
    }
}

/// What the visor answers to this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Senses {
    /// The picture is seen from the player's own eyes (else there is no visor in the picture).
    pub own_eyes: bool,
    /// The welding filter is asked down (the welder in hand, its right button).
    pub filter: bool,
    /// On foot and going, and at a run.
    pub walking: bool,
    pub running: bool,
    /// How hard the pack pushes (0..1).
    pub jet: f32,
    /// How fast the ground was met at the last step (m/s; 0: no landing then).
    pub landed: f32,
    /// What is left of the suit's oxygen (share of a full tank), if the suit keeps count of it.
    pub oxygen: Option<f32>,
}

impl Senses {
    /// What the player is doing, as the visor feels it: seen through `view`, the welder's
    /// filter asked down or not.
    pub fn of(pilot: &Pilot, input: &Input, filter: bool, _view: &View, own_eyes: bool) -> Senses {
        let walking = pilot.grounded && pilot.seat.is_none() && !pilot.flying && (input.forward != 0.0 || input.side != 0.0);
        // (the suit keeps no count of its oxygen yet: when it does, here)
        Senses { own_eyes, filter, walking, running: walking && input.run, jet: if pilot.pack_on { pilot.jet as f32 } else { 0.0 }, landed: pilot.landed as f32, oxygen: None }
    }
}

/// A landing this fast (m/s) is a whole jump's worth of effort.
const HARD_LANDING: f32 = 3.0;

pub struct Visor {
    pub def: VisorDef,
    /// The filter: how dark (0 clear .. 1 dark), and whether a script holds it down.
    dark: f32,
    forced: bool,
    /// Effort (0..1), and whether the last step was a landing (each is felt once).
    effort: f32,
    landing: bool,
    /// Seen from one's own eyes this frame.
    worn: bool,
}

impl Visor {
    pub fn load(path: &Path) -> Result<Visor, String> {
        let def: VisorDef = lunar_core::defs::load(path).map_err(|e| format!("{}: {}", e.file, e.message))?;
        def.check().map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Visor::new(def))
    }

    pub fn new(def: VisorDef) -> Visor {
        Visor { def, dark: 0.0, forced: false, effort: 0.0, landing: false, worn: false }
    }

    /// A script's: the filter held down or let go, and the effort so far (0..1), each if said.
    pub fn set(&mut self, filter: Option<bool>, effort: Option<f32>) {
        if let Some(on) = filter {
            self.forced = on;
        }
        if let Some(e) = effort {
            self.effort = e.clamp(0.0, 1.0);
        }
    }

    /// How dark the filter is (0 clear .. 1) and how much mist is on it (0..`tope`).
    pub fn state(&self) -> (f32, f32) {
        let a = &self.def.aliento;
        let t = ((self.effort - a.desde) / (1.0 - a.desde)).clamp(0.0, 1.0);
        (self.dark, t * t * (3.0 - 2.0 * t) * a.tope)
    }

    /// `dt` s on: the filter and the breath answer to `s`. What the picture is seen through
    /// this frame (from outside: nothing).
    pub fn step(&mut self, dt: f32, s: &Senses) -> lunar_render::Visor {
        let (f, a) = (self.def.filtro, self.def.aliento);
        // the filter: down while it is asked, at its own pace each way
        self.dark = if s.filter || self.forced { (self.dark + dt / f.oscurece).min(1.0) } else { (self.dark - dt / f.aclara).max(0.0) };
        // the breath: effort goes toward what one is doing, sooner up than down
        let short = s.oxygen.map_or(0.0, |o| (1.0 - o / a.oxigeno_bajo.max(1e-3)).clamp(0.0, 1.0) * a.ahogo);
        let doing = (if s.running { a.correr } else if s.walking { a.andar } else { 0.0 }).max(s.jet.clamp(0.0, 1.0) * a.mochila) + short;
        let doing = doing.min(1.0);
        let pace = if doing > self.effort { a.sube } else { a.baja };
        self.effort += (doing - self.effort) * (1.0 - (-dt / pace).exp());
        if s.landed > 0.0 && !self.landing {
            self.effort = (self.effort + a.salto * (s.landed / HARD_LANDING).min(1.0)).min(1.0);
        }
        self.landing = s.landed > 0.0;
        self.worn = s.own_eyes;
        if !s.own_eyes {
            return lunar_render::Visor::default();
        }
        let (dark, mist) = self.state();
        let mix = |c: f32| 1.0 + (c * f.oscuro - 1.0) * dark;
        lunar_render::Visor { filter: [mix(f.tinte[0]), mix(f.tinte[1]), mix(f.tinte[2])], mist, knee: f.rodilla * dark, ..lunar_render::Visor::default() }
    }

    /// This frame's visor, handed to the renderer.
    pub fn frame(&mut self, dt: f32, s: &Senses, r: &mut Renderer) {
        let v = self.step(dt, s);
        r.set_visor(v);
    }

    /// Its mark on the HUD (where one is, top left): what it is doing, when it is doing something.
    pub fn hud(&self, hud: &mut Hud) {
        if self.worn && self.dark > 0.5 {
            hud.status.push(("Visor".into(), "FILTRO DE SOLDADURA".into(), Level::Caution));
        }
    }

    /// What it is doing, in a line (scripts).
    pub fn report(&self) -> String {
        let (dark, mist) = self.state();
        format!("visor: filtro {:.0} %, esfuerzo {:.0} %, vaho {:.0} %{}", dark * 100.0, self.effort * 100.0, mist * 100.0, if self.worn { "" } else { " (visto desde fuera: no se dibuja)" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn the_visor() -> Visor {
        Visor::load(&crate::root().join("assets/defs/visor.jsonc")).unwrap()
    }

    fn looking() -> Senses {
        Senses { own_eyes: true, ..Senses::default() }
    }

    /// `secs` of `s` at `fps` frames a second: the last frame's visor.
    fn run(v: &mut Visor, s: &Senses, secs: f32, fps: f32) -> lunar_render::Visor {
        let mut out = lunar_render::Visor::default();
        for _ in 0..(secs * fps).round() as usize {
            out = v.step(1.0 / fps, s);
        }
        out
    }

    #[test]
    fn the_data_is_sound_and_what_is_wrong_is_said() {
        let v = the_visor();
        v.def.check().unwrap();
        // it dims by half at the most (what is being welded is still seen through it): what it
        // does to the arc it does by pressing down what is bright
        let f = v.def.filtro;
        assert!(f.oscurece <= 0.3 && f.aclara <= 0.5 && (0.4..0.75).contains(&f.oscuro) && f.rodilla >= 1.5);
        let mut bad = v.def;
        bad.filtro.oscuro = 0.0;
        assert!(bad.check().unwrap_err().contains("filtro"));
        let mut bad = v.def;
        bad.aliento.baja = 0.0;
        assert!(bad.check().unwrap_err().contains("aliento"));
        // nothing happening: the picture is as it was
        let mut v = the_visor();
        let still = run(&mut v, &looking(), 2.0, 60.0);
        assert_eq!(still, lunar_render::Visor::default());
    }

    #[test]
    fn the_filter_is_down_only_while_it_is_asked() {
        for fps in [30.0f32, 60.0, 144.0] {
            let mut v = the_visor();
            let f = v.def.filtro;
            let s = looking();
            let asked = Senses { filter: true, ..s };
            // not asked: clear, however long
            run(&mut v, &s, 5.0, fps);
            assert_eq!(v.state().0, 0.0);
            // asked: dark within its time (and a frame), whatever the frame rate
            let dim = run(&mut v, &asked, f.oscurece + 1.0 / fps, fps);
            assert!(v.state().0 > 0.999, "a {fps} fps: {} tras {:.3} s", v.state().0, f.oscurece + 1.0 / fps);
            // dark: `oscuro` of the light, a little green, and what is bright pressed down
            assert!((dim.filter[1] - f.oscuro * f.tinte[1]).abs() < 1e-4 && dim.filter[0] < dim.filter[1] && dim.filter[2] < dim.filter[1], "{:?}", dim.filter);
            assert!((dim.knee - f.rodilla).abs() < 1e-4, "{}", dim.knee);
            // (through it something dim keeps most of its light and an arc a hundred times
            // over white is under white: a spot, not a blot)
            let through = |light: f32| light * dim.filter[1] / (1.0 + dim.knee * light * dim.filter[1]);
            assert!(through(0.2) > 0.2 * 0.4 && through(100.0) < 1.0, "{} / {}", through(0.2), through(100.0));
            // it stays dark while it is asked, and clears when it is not
            run(&mut v, &asked, 3.0, fps);
            assert_eq!(v.state().0, 1.0);
            let clear = run(&mut v, &s, f.aclara + 0.05, fps);
            assert!(v.state().0 == 0.0 && clear.filter == [1.0; 3], "a {fps} fps: {} tras soltarlo", v.state().0);
        }
    }

    #[test]
    fn from_outside_there_is_no_visor_in_the_picture() {
        let mut v = the_visor();
        let s = looking();
        let out = run(&mut v, &Senses { own_eyes: false, filter: true, running: true, walking: true, ..s }, 30.0, 60.0);
        assert_eq!(out, lunar_render::Visor::default());
        // (but it went on: back in one's eyes it is dark and misted)
        let back = v.step(1.0 / 60.0, &Senses { filter: true, ..s });
        assert!(back.filter[0] < 0.6 && back.knee > 1.0 && back.mist > 0.3, "{back:?}");
        // a script's
        let mut v = the_visor();
        v.set(Some(true), Some(0.9));
        run(&mut v, &s, 0.5, 60.0);
        assert!(v.state().0 == 1.0 && v.state().1 > 0.0);
    }

    #[test]
    fn breath_mists_the_visor_with_effort_and_clears_at_rest() {
        let mut v = the_visor();
        let a = v.def.aliento;
        let s = looking();
        // at rest and at a walk: none, however long
        assert_eq!(run(&mut v, &s, 120.0, 30.0).mist, 0.0);
        assert_eq!(run(&mut v, &Senses { walking: true, ..s }, 300.0, 30.0).mist, 0.0);
        // at a run: none at first, some within its time, never more than its most
        let running = Senses { walking: true, running: true, ..s };
        let mut v = the_visor();
        assert_eq!(run(&mut v, &running, 2.0, 60.0).mist, 0.0);
        let some = run(&mut v, &running, a.sube, 60.0).mist;
        let most = run(&mut v, &running, 120.0, 60.0).mist;
        assert!(some > 0.05 && most > some && most <= a.tope + 1e-6 && most > a.tope * 0.9, "{some} y luego {most}");
        // at rest it clears, more slowly than it came, all of it in the end
        let after = run(&mut v, &s, a.sube, 60.0).mist;
        assert!(after > 0.0 && after < most);
        assert_eq!(run(&mut v, &s, 6.0 * a.baja, 60.0).mist, 0.0);
        // the same at any frame rate
        let at = |fps: f32| {
            let mut v = the_visor();
            run(&mut v, &running, 12.0, fps).mist
        };
        assert!((at(30.0) - at(144.0)).abs() < 0.01, "{} a 30 fps, {} a 144", at(30.0), at(144.0));
        // the pack's push tires too, less; a landing is felt once, not every frame it is told
        let mut v = the_visor();
        let pack = run(&mut v, &Senses { jet: 1.0, ..s }, 120.0, 60.0).mist;
        assert!(pack > 0.0 && pack < most);
        let mut v = the_visor();
        run(&mut v, &Senses { landed: 4.0, ..s }, 0.5, 60.0);
        assert!((v.effort - a.salto).abs() < 0.02, "esfuerzo {} tras un salto", v.effort);
        // the suit short of oxygen: heavier breath even standing
        let mut v = the_visor();
        assert_eq!(run(&mut v, &Senses { oxygen: Some(0.9), ..s }, 120.0, 60.0).mist, 0.0);
        assert!(run(&mut v, &Senses { oxygen: Some(0.02), ..s }, 120.0, 60.0).mist > 0.1);
    }

    #[test]
    fn its_mark_is_on_the_hud_only_while_it_does_something() {
        let mut v = the_visor();
        let s = looking();
        let marks = |v: &Visor| {
            let mut hud = Hud::default();
            v.hud(&mut hud);
            hud.status
        };
        run(&mut v, &s, 0.1, 60.0);
        assert!(marks(&v).is_empty());
        run(&mut v, &Senses { filter: true, ..s }, 0.5, 60.0);
        assert!(matches!(marks(&v).as_slice(), [(name, what, Level::Caution)] if name == "Visor" && what.contains("FILTRO")));
        // from outside: none
        run(&mut v, &Senses { own_eyes: false, filter: true, ..s }, 0.1, 60.0);
        assert!(marks(&v).is_empty() && v.report().contains("filtro"));
    }
}
