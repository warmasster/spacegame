//! Exhaust in play: the plumes of whatever fires — every thruster of every ship (engines and
//! reaction control alike, through `lunar_ship::exhaust`) and the nozzles of the suit's pack —
//! handed each frame to `lunar_core::plumes` (how they look is data: `assets/defs/chorros.jsonc`)
//! and from there to the renderer. Nothing here knows a ship or an engine by name.
//!
//! What it costs (docs/CHORROS.md): a ship at rest is passed over at once (`Ship::busy`); of a
//! ship that fires, only its thrusters are looked at, and only those that give thrust or are
//! still fading are worked out; ships farther than their plumes can be seen are not looked at.
//! Nothing is allocated once a ship has fired for the first time. The dust the jets raise from
//! the ground is `dust`'s, not this module's.
use crate::{
    body::{Body, Stance},
    pilot::Pilot,
    ships::Ships,
};
use glam::{DVec3, Vec3};
use lunar_core::{
    body::BodyRegistry,
    effects::Effects,
    plumes::{Exhaust, Eye, FIRING, Memory, Nozzle, PlumeDefs, Styles},
    structure::{set::Structures, state::Structure},
};
use lunar_render::{Light, Renderer, View};
use lunar_ship::{Ship, ShipKind, exhaust};
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

/// A ship's memory is dropped when it has not fired for this many frames.
const FORGET: u32 = 1800;

/// A thruster of a kind of ship: its machine, its style, its rated thrust (N), its own seed.
#[derive(Clone, Copy, Debug)]
struct Thruster {
    machine: u32,
    style: u8,
    rated: f32,
    seed: f32,
}

/// What is known of a kind of ship once: its thrusters, and how far the plume seen from
/// farthest is seen (m, for an eye of one pixel per radian).
struct KindFx {
    /// (not kept alive by this: a kind the editor has rebuilt goes when its ships do)
    kind: Weak<ShipKind>,
    thrusters: Vec<Thruster>,
    reach: f32,
}

/// A ship that has fired: its kind and what each of its thrusters' plumes remembers.
struct ShipFx {
    kind: usize,
    memory: Vec<Memory>,
    seen: u32,
    /// Something of it is still shown.
    live: bool,
}

/// The suit's pack: its style and its nozzles (the suit's points named so), found the first
/// time it pushes.
struct Pack {
    style: u8,
    radius: f32,
    cos: f32,
    prefix: String,
    nozzles: Option<Vec<String>>,
    memory: Vec<Memory>,
    /// This frame: where each nozzle is and the way its jet leaves (world).
    ways: Vec<(DVec3, Vec3)>,
    live: bool,
}

pub struct Plumes {
    exhaust: Exhaust,
    kinds: Vec<KindFx>,
    ships: HashMap<u64, ShipFx>,
    pack: Option<Pack>,
    lights: Vec<Light>,
    frame: u32,
}

/// The thrusters of `kind` and their styles (a machine that pushes and has no style shows
/// nothing).
fn kind_fx(kind: &Arc<ShipKind>, styles: &Styles) -> KindFx {
    let mut thrusters = Vec::new();
    let mut reach = 0.0f32;
    for (m, jet) in exhaust::thrusters(kind) {
        let Some(style) = styles.of(jet.style.as_deref(), &kind.machines[m].def.modelo) else { continue };
        reach = reach.max(styles.list[usize::from(style)].reach(jet.rated, jet.radius, 1.0));
        thrusters.push(Thruster { machine: m as u32, style, rated: jet.rated, seed: (m as f32 * 0.618_034).fract() });
    }
    KindFx { kind: Arc::downgrade(kind), thrusters, reach }
}

impl Plumes {
    /// The styles of `assets/defs/chorros.jsonc`, told to the renderer.
    pub fn new(fx: &Effects, r: &Renderer) -> Result<Plumes, String> {
        let plumes = Plumes::load(fx)?;
        r.set_plume_styles(&plumes.exhaust.styles);
        Ok(plumes)
    }

    fn load(fx: &Effects) -> Result<Plumes, String> {
        let defs = PlumeDefs::load(&crate::root().join("assets/defs")).map_err(|e| e.to_string())?;
        let styles = Styles::new(&defs, |name| fx.style(name))?;
        let pack = styles.pack.as_ref().map(|(style, def)| Pack { style: *style, radius: def.radio, cos: def.cono.to_radians().cos(), prefix: def.toberas.clone(), nozzles: None, memory: Vec::new(), ways: Vec::new(), live: false });
        Ok(Plumes { exhaust: Exhaust::new(styles), kinds: Vec::new(), ships: HashMap::new(), pack, lights: Vec::with_capacity(lunar_core::plumes::MAX_GLOWS), frame: 0 })
    }

    /// This frame's exhaust: of every ship that fires near enough to be seen from `view`, of the
    /// pack of `pilot` (its body `body`, if it has one this frame); its plumes and their lights to
    /// the renderer (after the explosions' flashes, before the ships' lamps), its gas to `fx`.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, dt: f32, fx: &mut Effects, bodies: &BodyRegistry, pilot: &Pilot, body: Option<(&Body, &Stance)>, ships: &Ships, set: &Structures, r: &mut Renderer, view: &View, sun: DVec3) {
        let height = match r.stats.render_size.1 {
            0 => 1080.0,
            h => h as f32,
        };
        let eye = Eye { at: view.eye, px_per_rad: height / (2.0 * (view.fov_y * 0.5).tan()), sun };
        self.begin(dt, fx);
        for sh in &ships.list {
            // a ship at rest fires nothing: not even its structure is looked for
            if sh.busy()
                && let Some(s) = set.get(sh.structure)
            {
                self.ship(dt, sh, s, &eye, fx, bodies);
            }
        }
        self.pack(dt, pilot, body, set, &eye, fx, bodies);
        r.set_plumes(&self.exhaust.plumes, view.eye);
        if !self.exhaust.glows.is_empty() {
            self.lights.clear();
            self.lights.extend(self.exhaust.glows.iter().map(|g| Light { pos: g.pos, color: g.color, range: g.range, ..Light::default() }));
            r.add_flashes(&self.lights);
        }
    }

    fn begin(&mut self, dt: f32, fx: &Effects) {
        self.frame = self.frame.wrapping_add(1);
        self.exhaust.begin(fx, dt);
        if self.frame % FORGET == 0 {
            let now = self.frame;
            self.ships.retain(|_, e| now.wrapping_sub(e.seen) < FORGET);
            // kinds that are gone (a ship rebuilt in the editor): forgotten, and with them what
            // was remembered by their place in the list
            if self.kinds.iter().any(|k| k.kind.strong_count() == 0) {
                self.kinds.retain(|k| k.kind.strong_count() > 0);
                self.ships.clear();
            }
        }
    }

    /// The plumes of ship `sh` on structure `s`: of the thrusters that push now, and of those
    /// still fading.
    fn ship(&mut self, dt: f32, sh: &Ship, s: &Structure, eye: &Eye, fx: &mut Effects, bodies: &BodyRegistry) {
        if s.force == Vec3::ZERO && s.torque == Vec3::ZERO && !self.ships.get(&sh.structure).is_some_and(|e| e.live) {
            return;
        }
        let k = match self.kinds.iter().position(|k| std::ptr::eq(k.kind.as_ptr(), Arc::as_ptr(&sh.kind))) {
            Some(k) => k,
            None => {
                self.kinds.push(kind_fx(&sh.kind, &self.exhaust.styles));
                self.kinds.len() - 1
            }
        };
        let kind = &self.kinds[k];
        // farther than the plume seen from farthest is seen: nothing to do
        let d = (s.to_world(s.center).distance(eye.at) - f64::from(s.radius)) as f32;
        if kind.thrusters.is_empty() || d > kind.reach * eye.px_per_rad {
            return;
        }
        let n = kind.thrusters.len();
        let e = self.ships.entry(sh.structure).or_insert_with(|| ShipFx { kind: k, memory: vec![Memory::default(); n], seen: 0, live: false });
        if e.kind != k || e.memory.len() != n {
            (e.kind, e.memory) = (k, vec![Memory::default(); n]);
        }
        e.seen = self.frame;
        e.live = false;
        let vel = s.vel.as_vec3();
        // (whose ground its gas is drawn over: the nearest)
        let body = bodies.dominant(s.pos);
        for (t, m) in kind.thrusters.iter().zip(&mut e.memory) {
            let m_ix = t.machine as usize;
            if sh.machines[m_ix].m.thrust() <= 0.0 && m.idle() {
                continue;
            }
            let Some(f) = exhaust::thruster(sh, s, m_ix) else { continue };
            let nozzle = Nozzle { at: s.to_world(f.at), dir: s.rot * f.dir, radius: f.radius, rated: t.rated, level: f.level, style: t.style, vel, body, seed: t.seed };
            self.exhaust.add(&nozzle, m, dt, eye, fx, bodies);
            e.live |= !m.idle();
        }
    }

    /// The pack's jets: each nozzle whose jet leaves against the push gives its share of it.
    fn pack(&mut self, dt: f32, pilot: &Pilot, body: Option<(&Body, &Stance)>, set: &Structures, eye: &Eye, fx: &mut Effects, bodies: &BodyRegistry) {
        let Some(p) = &mut self.pack else { return };
        let on = pilot.pack_on && !pilot.flying && pilot.seat.is_none();
        let push = if on { pilot.push } else { DVec3::ZERO };
        if push == DVec3::ZERO && !p.live {
            return;
        }
        let Some((body, stance)) = body else { return };
        let nozzles = p.nozzles.get_or_insert_with(|| body.rig.points.keys().filter(|k| k.starts_with(&p.prefix)).cloned().collect());
        if p.memory.len() != nozzles.len() {
            p.memory = vec![Memory::default(); nozzles.len()];
        }
        p.ways.clear();
        p.ways.extend(nozzles.iter().map(|name| body.point(name, stance).map_or((stance.eye, Vec3::ZERO), |(at, way, _)| (at, way))));
        // the way the gas must go, and what each nozzle that looks that way gives of it: two
        // nozzles full on are the pack's whole push
        let strength = push.length() as f32;
        let against = if strength > 1e-4 { (-push / push.length()).as_vec3() } else { Vec3::ZERO };
        let answer = |way: Vec3| ((way.dot(against) - p.cos) / (1.0 - p.cos)).max(0.0);
        let sum: f32 = p.ways.iter().map(|(_, way)| answer(*way) * way.dot(against)).sum();
        let gain = if sum > 1e-4 { 2.0 * strength / sum } else { 0.0 };
        let rated = (pilot.pack_thrust() * 0.5) as f32;
        // (the gas leaves with the speed we have in the world: aboard, the ship's too)
        let vel = pilot.velocity_in(set).as_vec3();
        p.live = false;
        for (k, ((at, way), m)) in p.ways.iter().zip(&mut p.memory).enumerate() {
            let level = (gain * answer(*way)).min(1.2);
            if level < FIRING && m.idle() {
                continue;
            }
            let nozzle = Nozzle { at: *at, dir: *way, radius: p.radius, rated, level, style: p.style, vel, body: pilot.body, seed: (k as f32 * 0.137).fract() };
            self.exhaust.add(&nozzle, m, dt, eye, fx, bodies);
            p.live |= !m.idle();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::{Rig, RigDef, Rigged};
    use lunar_controls::Intent;
    use lunar_core::{
        body::{Body as Planet, BodyDef},
        defs,
        effects::EffectDefs,
        scenario::ScenarioDef,
        scene::Site,
    };
    use lunar_ship::{ShipLibrary, World, ship::TICK};

    fn root() -> std::path::PathBuf {
        crate::root().join("assets/defs")
    }

    fn effects() -> Effects {
        Effects::new(&EffectDefs::load(&root()).unwrap_or_else(|e| panic!("{e}")), 16_000)
    }

    fn moon() -> Arc<BodyRegistry> {
        let moon: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        Arc::new(BodyRegistry::new(vec![Planet::from_def("luna", &moon).unwrap()]))
    }

    fn site(bodies: &BodyRegistry) -> (Site, ScenarioDef) {
        let sc: ScenarioDef = defs::parse("scenario", include_str!("../../../assets/defs/scenario.jsonc")).unwrap();
        (Site::from_def(&sc.site, bodies).unwrap(), sc)
    }

    /// The Moon and every kind of ship.
    fn library() -> (Structures, Arc<BodyRegistry>, ShipLibrary) {
        let mut lib = lunar_core::structure::Library::load(&root().join("structures")).unwrap();
        let (ships, bps) = ShipLibrary::load(&root(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        lib.blueprints.extend(bps);
        (Structures::new(Arc::new(lib)), moon(), ships)
    }

    /// A ship of `kind` standing on the scenario's site.
    fn standing(set: &mut Structures, bodies: &BodyRegistry, ships: &ShipLibrary, kind: &str) -> (Ship, u64) {
        let (site, _) = site(bodies);
        let kind = ships.get(kind).unwrap().clone();
        let id = set.place(&kind.blueprint, bodies, 0, site.at(40.0, 40.0), 0.0, f64::from(kind.lift)).unwrap();
        let mut ship = Ship::new(kind, id, 7).unwrap_or_else(|e| panic!("{e}"));
        let k = set.index_of(id).unwrap();
        ship.update(&mut set.list[k], &World::default(), 0.0);
        set.rest_on_ground(id, bodies);
        (ship, id)
    }

    fn run(ship: &mut Ship, s: &mut Structure, secs: f64) {
        for _ in 0..(secs / TICK).round() as usize {
            ship.update(s, &World::default(), TICK);
        }
    }

    fn press(ship: &mut Ship, s: &mut Structure, id: &str) {
        let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"));
        let kind = ship.kind.clone();
        ship.panels.intent(k, &Intent::Press { elem: 0 }, s, &kind, &ship.store);
        run(ship, s, 0.25);
        ship.panels.intent(k, &Intent::Release, s, &kind, &ship.store);
        run(ship, s, 0.1);
    }

    /// An eye `d` m to the side of `at` and a little over it, on a 1080 px screen of 60°.
    fn eye_by(bodies: &BodyRegistry, at: DVec3, d: f64) -> Eye {
        let up = bodies.get(0).up(at);
        Eye { at: at + up.any_orthonormal_vector() * d + up * 2.0, px_per_rad: 935.0, sun: up }
    }

    #[test]
    fn every_thruster_of_every_ship_has_a_style_and_this_is_what_they_cost() {
        let fx = effects();
        let plumes = Plumes::load(&fx).unwrap_or_else(|e| panic!("{e}"));
        let styles = &plumes.exhaust.styles;
        let (_, _, ships) = library();
        for kind in &ships.kinds {
            let k = kind_fx(kind, styles);
            assert_eq!(k.thrusters.len(), exhaust::thrusters(kind).count(), "{}: propulsores sin estilo", kind.id);
            assert!(!k.thrusters.is_empty() && k.reach > 0.0);
            // by style: how many, how long their plume, what gas they throw
            let mut by: std::collections::BTreeMap<&str, (usize, f32, f32, u32)> = std::collections::BTreeMap::new();
            for t in &k.thrusters {
                let s = &styles.list[usize::from(t.style)];
                let e = by.entry(styles.names[usize::from(t.style)].as_str()).or_default();
                let (start, stop) = s.bursts();
                *e = (e.0 + 1, s.length(t.rated), e.2 + s.stream_rate(), e.3 + start + stop);
            }
            let stream: f32 = by.values().map(|v| v.2).sum();
            eprintln!("{}: {} propulsores, se ven hasta {:.0} m (1080 px, 60°); todos a tope a la vez: {} conos (48 bytes cada uno), {stream:.0} partículas/s de chorro", kind.id, k.thrusters.len(), k.reach * 935.0, k.thrusters.len());
            for (name, (n, len, rate, bursts)) in &by {
                eprintln!("    {n} × {name}: cono de {len:.1} m; {rate:.0} partículas/s entre todos; {bursts} en ráfagas al encender y apagar");
            }
        }
        let style_of = |ship: &str, machine: &str| {
            let kind = ships.get(ship).unwrap();
            let m = kind.machines.iter().position(|m| m.id == machine).unwrap();
            let k = kind_fx(kind, styles);
            styles.names[usize::from(k.thrusters.iter().find(|t| t.machine as usize == m).unwrap().style)].clone()
        };
        assert_eq!(style_of("alcotan", "gondola_izq"), "motor");
        assert_eq!(style_of("alcotan", "rcs_popa_der_fuera"), "rcs");
        assert_eq!(style_of("cachalote", "gondola_popa_der"), "motor");
        assert_eq!(style_of("cachalote", "rcs_proa_izq_abajo"), "rcs");
        assert_eq!(style_of("abejorro", "motor_popa_izq"), "motor");
        assert_eq!(style_of("abejorro", "rcs_proa_izq_arriba"), "gas_frio");
    }

    #[test]
    fn a_ship_at_rest_costs_nothing_and_one_that_fires_shows_its_plumes_where_its_nozzles_are() {
        let mut fx = effects();
        let mut plumes = Plumes::load(&fx).unwrap_or_else(|e| panic!("{e}"));
        let (mut set, bodies, ships) = library();
        let (mut ship, id) = standing(&mut set, &bodies, &ships, "abejorro");
        let k = set.index_of(id).unwrap();
        let dt = 1.0 / 60.0;
        // standing with everything off, and long enough for it to be at rest
        run(&mut ship, &mut set.list[k], 10.0);
        let eye = eye_by(&bodies, set.list[k].pos, 12.0);
        assert!(!ship.busy(), "una nave posada y apagada sigue ocupada");
        for _ in 0..10 {
            plumes.begin(dt, &fx);
            plumes.ship(dt, &ship, &set.list[k], &eye, &mut fx, &bodies);
        }
        assert!(plumes.exhaust.plumes.is_empty() && plumes.exhaust.walked == 0 && plumes.ships.is_empty() && plumes.kinds.is_empty() && fx.particles.is_empty());
        // its four engines lit, at a third of their thrust (it stays down)
        for c in ["consola/tapa_arm", "consola/arm", "consola/arr", "consola/arr"] {
            press(&mut ship, &mut set.list[k], c);
        }
        run(&mut ship, &mut set.list[k], 3.0);
        let c = ship.panels.controls.iter().position(|c| c.id == "consola/acelerador").unwrap();
        let kind = ship.kind.clone();
        ship.panels.intent(c, &Intent::Set { value: 0.1 }, &set.list[k], &kind, &ship.store);
        run(&mut ship, &mut set.list[k], 2.0);
        assert!(ship.busy());
        plumes.begin(dt, &fx);
        plumes.ship(dt, &ship, &set.list[k], &eye, &mut fx, &bodies);
        let s = &set.list[k];
        let up = (s.rot * Vec3::Y).as_dvec3();
        assert_eq!(plumes.exhaust.plumes.len(), 4, "{:?}", plumes.exhaust.plumes);
        assert_eq!(plumes.exhaust.walked, 4, "solo se miran los propulsores que dan empuje");
        for p in &plumes.exhaust.plumes {
            // out of the bell under each engine, downward, a short dim cone
            let local = s.to_local(p.pos);
            assert!(p.dir.as_dvec3().dot(-up) > 0.99 && local.y < -0.5 && local.x.abs() > 1.5, "{p:?} en {local:?}");
            assert!((p.radius - 0.238).abs() < 0.01 && p.length > 0.3 && p.length < 2.6 && p.spread > p.radius && p.gain > 0.05 && p.gain < 0.8, "{p:?}");
        }
        assert!(!plumes.exhaust.glows.is_empty() && fx.particles.len() >= 4, "{} luces, {} partículas al encender", plumes.exhaust.glows.len(), fx.particles.len());
        // from too far to see them: nothing is worked out
        let far = eye_by(&bodies, s.pos, 30_000.0);
        plumes.begin(dt, &fx);
        plumes.ship(dt, &ship, &set.list[k], &far, &mut fx, &bodies);
        assert!(plumes.exhaust.plumes.is_empty() && plumes.exhaust.walked == 0);
        // full throttle: longer and brighter
        plumes.begin(dt, &fx);
        plumes.ship(dt, &ship, &set.list[k], &eye, &mut fx, &bodies);
        let dim = plumes.exhaust.plumes[0];
        ship.panels.intent(c, &Intent::Set { value: 1.0 }, &set.list[k], &kind, &ship.store);
        run(&mut ship, &mut set.list[k], 2.0);
        plumes.begin(dt, &fx);
        plumes.ship(dt, &ship, &set.list[k], &eye, &mut fx, &bodies);
        let bright = plumes.exhaust.plumes[0];
        assert!(bright.length > dim.length * 1.5 && bright.gain > dim.gain * 1.3, "{dim:?} → {bright:?}");
        // engines stopped: the plumes fade and the ship is forgotten about until it fires again
        press(&mut ship, &mut set.list[k], "consola/arm");
        let mut frames = 0;
        loop {
            run(&mut ship, &mut set.list[k], f64::from(dt));
            plumes.begin(dt, &fx);
            plumes.ship(dt, &ship, &set.list[k], &eye, &mut fx, &bodies);
            frames += 1;
            if plumes.exhaust.plumes.is_empty() || frames > 1200 {
                break;
            }
        }
        assert!(frames < 600, "los chorros siguen a los {frames} fotogramas de parar");
        // (the last of their thrust dies away: until then they are still looked at)
        for _ in 0..180 {
            run(&mut ship, &mut set.list[k], f64::from(dt));
            plumes.begin(dt, &fx);
            plumes.ship(dt, &ship, &set.list[k], &eye, &mut fx, &bodies);
        }
        assert!(!plumes.ships[&id].live && plumes.exhaust.plumes.is_empty());
        assert_eq!(plumes.exhaust.walked, 0);
    }

    #[test]
    fn the_pack_answers_each_push_with_the_jets_against_it() {
        let path = crate::root();
        let def: RigDef = defs::load(&path.join("assets/defs/rigs/astronauta.jsonc")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        let Ok(model) = Rigged::load(&path.join("assets/models").join(format!("{}.glb", def.modelo))) else {
            eprintln!("(sin el modelo del astronauta: nada que comprobar)");
            return;
        };
        let mut body = Body::new(Rig::new(def, model.skeleton).unwrap(), None, None);
        let bodies = moon();
        let (place, sc) = site(&bodies);
        let mut pilot = Pilot::new(bodies.clone(), &place, sc.player);
        let set = Structures::new(Arc::new(lunar_core::structure::Library::load(&root().join("structures")).unwrap()));
        // aloft two metres over the ground, facing some way
        let b = bodies.get(0);
        let up = b.up(pilot.position);
        let ahead = up.any_orthonormal_vector();
        let left = up.cross(ahead);
        let eye_h = 1.75;
        let stance = Stance { eye: b.above_ground(up, eye_h + 2.0), up, ahead, eye_h, vel: DVec3::ZERO, grounded: false, g: 1.62, ride: None, seated: false, inside: false, own_eyes: false };
        for _ in 0..20 {
            body.update(1.0 / 60.0, &stance, &set, &bodies, [None, None]);
        }
        let mut fx = effects();
        let mut plumes = Plumes::load(&fx).unwrap_or_else(|e| panic!("{e}"));
        let eye = Eye { at: stance.eye - ahead * 3.0, px_per_rad: 935.0, sun: up };
        let dt = 1.0 / 60.0;
        let jets = |plumes: &mut Plumes, pilot: &Pilot, fx: &mut Effects| {
            plumes.begin(dt, fx);
            plumes.pack(dt, pilot, Some((&body, &stance)), &set, &eye, fx, &bodies);
            plumes.exhaust.plumes.clone()
        };
        // the pack off, or on and not pushing: nothing, and nothing looked at
        pilot.push = up;
        assert!(jets(&mut plumes, &pilot, &mut fx).is_empty());
        pilot.pack_on = true;
        pilot.push = DVec3::ZERO;
        assert!(jets(&mut plumes, &pilot, &mut fx).is_empty() && plumes.exhaust.walked == 0);
        // every push: jets, all of them against it, out of the pack (behind the back, between
        // the hips and the top of the head)
        let nozzles = body.rig.points.keys().filter(|k| k.starts_with("tobera_")).count();
        assert!(nozzles >= 6, "la mochila tiene {nozzles} toberas");
        let feet = stance.eye - up * eye_h;
        for (name, push) in [("arriba", up), ("abajo", -up * 0.6), ("izquierda", left * 0.35), ("derecha", -left * 0.35), ("adelante", ahead * 0.35), ("atrás", -ahead * 0.35), ("arriba y a un lado", up * 0.4 + left * 0.35)] {
            // (what the push before left has faded)
            pilot.push = DVec3::ZERO;
            for _ in 0..15 {
                jets(&mut plumes, &pilot, &mut fx);
            }
            assert!(jets(&mut plumes, &pilot, &mut fx).is_empty(), "antes de empujar hacia {name} quedan chorros");
            pilot.push = push;
            let seen = jets(&mut plumes, &pilot, &mut fx);
            assert!(!seen.is_empty() && seen.len() <= 4, "empujando hacia {name}: {} chorros", seen.len());
            let against = (-push.normalize()).as_vec3();
            let mut gas = Vec3::ZERO;
            for p in &seen {
                assert!(p.dir.dot(against) > 0.45, "empujando hacia {name}, un chorro hacia {:?}", p.dir);
                let at = p.pos - feet;
                let (h, back, side) = (at.dot(up), at.dot(-ahead), at.dot(left));
                assert!(h > 0.8 && h < 1.9 && back > 0.1 && back < 0.7 && side.abs() < 0.35, "empujando hacia {name}, un chorro sale de {h:.2} m de alto, {back:.2} m detrás, {side:.2} m a un lado");
                assert!(p.length > 0.1 && p.length < 1.5 && p.radius < 0.05, "{p:?}");
                gas += p.dir * p.length * p.length;
            }
            // (and together they push the way asked: no turning the suit round)
            assert!(gas.normalize().dot(against) > 0.9, "empujando hacia {name}, el gas sale hacia {:?}", gas.normalize());
            eprintln!("empujando hacia {name}: {} chorros de {:.2} m", seen.len(), seen[0].length);
        }
        // harder, longer
        pilot.push = DVec3::ZERO;
        for _ in 0..15 {
            jets(&mut plumes, &pilot, &mut fx);
        }
        pilot.push = up;
        let full = jets(&mut plumes, &pilot, &mut fx)[0].length;
        pilot.push = up * 0.39;
        // (it follows the push down in a moment)
        for _ in 0..30 {
            jets(&mut plumes, &pilot, &mut fx);
        }
        let hover = jets(&mut plumes, &pilot, &mut fx)[0].length;
        assert!(hover < full * 0.75 && hover > full * 0.4, "sosteniéndose {hover:.2} m, a tope {full:.2} m");
        assert!(fx.particles.len() >= 4, "{} bocanadas", fx.particles.len());
        // let go: they fade and it is quiet again
        pilot.push = DVec3::ZERO;
        let mut frames = 0;
        while !jets(&mut plumes, &pilot, &mut fx).is_empty() {
            frames += 1;
            assert!(frames < 120);
        }
        assert!(jets(&mut plumes, &pilot, &mut fx).is_empty() && plumes.exhaust.walked == 0);
    }
}
