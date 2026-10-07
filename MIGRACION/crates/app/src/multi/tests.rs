//! Several whole games through a real server over a network of their own that loses, delays and
//! reorders datagrams on purpose (`lunar_net::MemoryNet`, the same every run): what one does,
//! the others see, and see it right however fast it goes.
use super::*;
use crate::{blasts::Blasts, content::Defs, pilot::Seat};
use glam::Quat;
use lunar_core::scene::Site;
use lunar_net::{Conditions, Memory, MemoryNet, Server, ServerConfig};
use std::sync::{Arc, OnceLock};

/// The game's data, read once for all the tests.
fn defs() -> &'static Defs {
    static DEFS: OnceLock<Defs> = OnceLock::new();
    DEFS.get_or_init(|| Defs::load(&crate::root().join("assets/defs")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message)))
}

/// A game without its picture: the scenario round the site, its ships, a player, what flies
/// and strikes, its end of the network.
struct Game {
    ships: Ships,
    builds: Builds,
    pilot: Pilot,
    multi: Multi,
    blasts: Blasts,
}

fn game(client: Client) -> Game {
    let defs = defs();
    let bodies = defs.system.bodies.clone();
    let sc = &defs.scenario;
    let site = Site::from_def(&sc.site, &bodies).unwrap();
    let effects: Vec<&str> = defs.effects.explosions.iter().map(|(id, _)| id.as_str()).collect();
    let mut builds = Builds::new(defs.structures.clone(), sc, &site, &bodies, &effects).unwrap();
    let mut ships = Ships::new(defs.ships.clone(), defs.font.0.clone());
    for a in &sc.ships {
        ships.spawn(&mut builds, &bodies, &a.ship, site.body, site.at(a.east, a.north), a.yaw.to_radians()).unwrap();
    }
    let mut blasts = Blasts::new(&defs.effects, lunar_core::missiles::Missiles::new(defs.missiles.clone()), 20_000).unwrap();
    blasts.set_guided(lunar_core::guided::Flight::load(&crate::root().join("assets/defs")).unwrap()).unwrap();
    let multi = Multi::with(client, &ships, &builds);
    Game { ships, builds, pilot: Pilot::new(bodies.clone(), &site, sc.player), multi, blasts }
}

/// A server and some games on a network of their own.
struct Table {
    net: MemoryNet,
    link: Memory,
    server: Server,
    games: Vec<Game>,
    now: f64,
    bodies: Arc<BodyRegistry>,
}

impl Table {
    fn new(players: &[&str], seed: u64, cond: Conditions) -> Table {
        let net = MemoryNet::new(seed);
        net.conditions(cond);
        let link = net.endpoint();
        let addr = link.addr();
        let probe = game(Client::with_transport(Box::new(net.endpoint()), addr, "nadie", crate::BUILD, 0));
        let count = probe.ships.list.len() as u32;
        drop(probe);
        let games = players.iter().map(|name| game(Client::with_transport(Box::new(net.endpoint()), addr, name, crate::BUILD, count))).collect();
        let mut t = Table { net, link, server: Server::new(ServerConfig::default()), games, now: 50.0, bodies: defs().system.bodies.clone() };
        // (in, one after another: the first is the host; and every clock settled, as it is a
        // second or two after joining)
        for _ in 0..600 {
            t.frame(1.0 / 60.0, false, |_, _, _| {});
            if t.games.iter().all(|g| g.multi.client.synced() && g.multi.client.clock_settled()) {
                break;
            }
        }
        assert!(t.games.iter().all(|g| g.multi.connected()), "{:?}", t.games.iter().map(|g| g.multi.status()).collect::<Vec<_>>());
        t
    }

    /// One frame of every game, as `play` runs it: what came in done first, then (`physics`) the
    /// world stepped and what flies landed, then `after` (what a test does there: an owner puts its
    /// ships where they are now), then what is ours sent out.
    fn frame(&mut self, dt: f64, physics: bool, mut after: impl FnMut(usize, &mut Game, f64)) {
        self.now += dt;
        self.net.set_time(self.now);
        self.server.update(self.now, &mut self.link);
        let _ = self.server.events().count();
        let bodies = &self.bodies;
        for (i, g) in self.games.iter_mut().enumerate() {
            g.blasts.tell = g.multi.connected();
            g.multi.receive(self.now, &mut g.ships, &mut g.builds, bodies);
            for (by, seen, age) in g.multi.shown.drain(..) {
                g.blasts.show(by, &seen, age, bodies, &mut g.builds);
            }
            if physics {
                // (watched from where the player is, as in play: what is far runs at its own pace;
                // the ships run their systems first, as in play)
                let eye = g.pilot.position;
                let awake: Vec<u64> = g.pilot.ride.map(|r| r.id).into_iter().collect();
                g.ships.update(dt, &mut g.builds, bodies, &mut g.blasts.fx, DVec3::Y, &[eye], &awake, &[]);
                let (fx, mut flight) = g.blasts.flight();
                g.builds.update(dt, bodies, fx, &[eye], &mut [&mut g.pilot, &mut flight]);
                g.blasts.land_rounds(bodies, &mut g.builds);
                // (what flies by itself: missiles, decoys)
                let view = lunar_render::View { eye: g.pilot.position, forward: DVec3::Z, up: DVec3::Y, fov_y: 1.0, near: 0.1 };
                let motion = lunar_core::structure::motion::Motion { at: g.pilot.position, vel: DVec3::ZERO, spin: DVec3::ZERO };
                g.blasts.update(dt, bodies, view, motion, &mut g.builds);
            }
            after(i, g, self.now);
            g.multi.send(self.now, &g.pilot, 1.75, [0.0, 0.0], 0, false, false, &g.ships, &mut g.builds, &mut g.blasts.seen);
        }
    }

    fn run(&mut self, secs: f64, dt: f64, physics: bool, mut after: impl FnMut(usize, &mut Game, f64)) {
        for _ in 0..((secs / dt).round() as usize).max(1) {
            self.frame(dt, physics, &mut after);
        }
    }
}

/// Game `g` sits at the controls of its ship `k` (and so owns it).
fn sit_at_controls(g: &mut Game, k: usize) {
    let seat = g.ships.list[k].kind.seats.iter().position(|s| !s.def.mandos.is_empty()).expect("a seat with controls");
    let d = g.ships.list[k].kind.seats[seat].def.clone();
    let structure = g.ships.list[k].structure;
    g.pilot.sit(&g.builds.set, Seat { structure, index: seat, eyes: Vec3::from_array(d.ojos), heading: d.rumbo.to_radians(), exit: Vec3::from_array(d.salida) });
}

/// A ship's structure put where it is and going as fast (a test's owner doing it every frame).
fn put(g: &mut Game, k: usize, k_at: &Kin) {
    let id = g.ships.list[k].structure;
    let s = g.builds.set.list.iter_mut().find(|s| s.id == id).unwrap_or_else(|| panic!("ship {k} ({}) has no structure", g.ships.list[k].kind.id));
    (s.pos, s.vel, s.rot, s.spin, s.resting) = (k_at.pos, k_at.vel, k_at.rot, k_at.spin, false);
}

/// Where a ship is at the world's moment (one far off runs at its own pace, behind the world:
/// carried on to it).
fn kin_of(g: &Game, k: usize) -> Kin {
    let s = g.builds.set.get(g.ships.list[k].structure).unwrap();
    let behind = (g.builds.set.now - s.clock).max(0.0);
    Kin { pos: s.pos + s.vel * behind, vel: s.vel, rot: s.rot, spin: s.spin }
}

/// Two ships flying together at `speed` over the Moon, 100 km up: the first straight on, the
/// second round it at a fighter's pull (about 1 g each way) and rolling. Where each is at `t`.
#[derive(Clone, Copy)]
struct Pair {
    start: DVec3,
    ahead: DVec3,
    side: DVec3,
    up: DVec3,
    speed: f64,
}

impl Pair {
    fn new(bodies: &BodyRegistry, speed: f64) -> Pair {
        let moon = bodies.get(0);
        let up = DVec3::new(0.3, 0.9, 0.2).normalize();
        let start = moon.center + up * (moon.radius + 100_000.0);
        let ahead = up.any_orthonormal_vector();
        Pair { start, ahead, side: up.cross(ahead), up, speed }
    }
    fn lead(&self, t: f64) -> Kin {
        Kin { pos: self.start + self.ahead * (self.speed * t), vel: self.ahead * self.speed, rot: Quat::IDENTITY, spin: Vec3::ZERO }
    }
    /// Where the second is from the first.
    fn offset(&self, t: f64) -> (DVec3, DVec3) {
        let (s, u, a) = (20.0 * (0.5 * t).sin(), 8.0 * (0.9 * t).sin(), 30.0 * (0.3 * t).sin());
        let (ds, du, da) = (10.0 * (0.5 * t).cos(), 7.2 * (0.9 * t).cos(), 9.0 * (0.3 * t).cos());
        (self.side * (60.0 + s) + self.up * u + self.ahead * a, self.side * ds + self.up * du + self.ahead * da)
    }
    fn wing(&self, t: f64) -> Kin {
        let (o, v) = self.offset(t);
        let roll = 0.8;
        Kin { pos: self.lead(t).pos + o, vel: self.ahead * self.speed + v, rot: Quat::from_axis_angle(self.ahead.as_vec3(), (roll * t) as f32), spin: self.ahead.as_vec3() * roll as f32 }
    }
}

/// How one game sees the other's ship beside its own: the worst it is off where it is (m), and
/// the worst it moves from one frame to the next otherwise than it really does (m).
#[derive(Default, Debug)]
struct Seen2 {
    off: f64,
    jump: f64,
    turn: f32,
    /// How far apart the two games' clocks are (s): what no frame can take away (`follow`).
    clocks: f64,
}

#[test]
fn ships_are_seen_flying_where_they_are_and_smoothly_at_any_speed_and_frame_rate() {
    // (a network as from one end of a country to the other: 40 ms each way, up to 10 ms more,
    // 3 % lost, 1 % twice)
    let bad = Conditions { loss: 0.03, duplicate: 0.01, delay: 0.04, jitter: 0.01 };
    let mut report = Vec::new();
    for speed in [0.0, 300.0, 2000.0, 7800.0] {
        for fps in [30.0, 60.0, 144.0, 240.0] {
            let mut t = Table::new(&["Ana", "Berto"], 7 + speed as u64, bad);
            let pair = Pair::new(&t.bodies, speed);
            // Ana sits at the first ship's controls and has it; Berto at the second's
            sit_at_controls(&mut t.games[0], 0);
            sit_at_controls(&mut t.games[1], 1);
            let t0 = t.now;
            for g in &mut t.games {
                put(g, 0, &pair.lead(0.0));
                put(g, 1, &pair.wing(0.0));
            }
            let dt = 1.0 / fps;
            let fly = |i: usize, g: &mut Game, now: f64| {
                // each owner flies its own
                let at = now - t0;
                if i == 0 {
                    put(g, 0, &pair.lead(at));
                } else {
                    put(g, 1, &pair.wing(at));
                }
            };
            t.run(1.0, dt, true, fly);
            assert!(t.games[1].multi.owns(t.games[1].ships.list[1].structure) && t.games[0].multi.owns(t.games[0].ships.list[0].structure));
            // then watched for two seconds: each sees the other's beside its own
            let mut seen = [Seen2::default(), Seen2::default()];
            let mut was: [Option<DVec3>; 2] = [None, None];
            for _ in 0..(2.0 * fps) as usize {
                t.frame(dt, true, fly);
                let at = t.now - t0;
                let truth = pair.offset(at).0;
                let clock = |g: &Game| g.multi.client.server_time(t.now).unwrap_or(t.now) - t.now;
                let apart = (clock(&t.games[0]) - clock(&t.games[1])).abs();
                for (i, w) in seen.iter_mut().enumerate() {
                    w.clocks = w.clocks.max(apart);
                    let g = &t.games[i];
                    let (a, b) = (kin_of(g, 0), kin_of(g, 1));
                    let d = b.pos - a.pos;
                    w.off = w.off.max(d.distance(truth));
                    let rot_true = pair.wing(at).rot;
                    w.turn = w.turn.max(b.rot.angle_between(rot_true));
                    if let Some(prev) = was[i] {
                        let step_true = truth - pair.offset(at - dt).0;
                        w.jump = w.jump.max(((d - prev) - step_true).length());
                    }
                    was[i] = Some(d);
                }
            }
            report.push(format!(
                "{speed:>6} m/s {fps:>3} fps: Ana ve la de Berto a {:.3} m (salto {:.4} m, giro {:.2}°), Berto la de Ana a {:.3} m (salto {:.4} m); relojes a {:.2} ms",
                seen[0].off,
                seen[0].jump,
                seen[0].turn.to_degrees(),
                seen[1].off,
                seen[1].jump,
                seen[0].clocks * 1000.0
            ));
            // what the clocks allow: the way the other goes times how far apart the clocks are,
            // and how far either slid while the word came (its slide times a word's age, a quarter
            // of a second at most)
            let clocks = speed * (seen[0].clocks + lunar_net::clock::SLEW * 0.25);
            for (i, w) in seen.iter().enumerate() {
                assert!(w.clocks < 0.004, "{speed} m/s, {fps} fps: los relojes a {:.2} ms\n{}", w.clocks * 1000.0, report.join("\n"));
                assert!(w.off < 0.3 + clocks * 1.1, "{speed} m/s, {fps} fps, juego {i}: la otra nave a {:.3} m de donde está (los relojes dan {clocks:.3} m)\n{}", w.off, report.join("\n"));
                assert!(w.jump < 0.02 + speed * lunar_net::clock::SLEW / fps, "{speed} m/s, {fps} fps, juego {i}: salta {:.4} m de un fotograma a otro\n{}", w.jump, report.join("\n"));
                assert!(w.turn.to_degrees() < 2.0, "{speed} m/s, {fps} fps, juego {i}: girada {:.2}°\n{}", w.turn.to_degrees(), report.join("\n"));
            }
        }
    }
    eprintln!("{}", report.join("\n"));
}

/// What every game has of a structure's parts: whether each is there and how hurt (bit for bit).
fn wounds(g: &Game, structure: u64) -> Vec<(bool, u32)> {
    g.builds.set.get(structure).map(|s| s.parts.iter().map(|p| (p.alive, p.hp.to_bits())).collect()).unwrap_or_default()
}

#[test]
fn ships_shoot_each_other_at_orbital_speed_and_every_game_ends_with_the_same_damage() {
    // Ana flies the first ship and fires its cannon at the second, 150 m ahead, which Berto flies;
    // Carla only watches. All at 2 km/s, over a network that loses and reorders.
    let bad = Conditions { loss: 0.05, duplicate: 0.02, delay: 0.04, jitter: 0.02 };
    let mut t = Table::new(&["Ana", "Berto", "Carla"], 21, bad);
    let pair = Pair::new(&t.bodies, 2000.0);
    sit_at_controls(&mut t.games[0], 0);
    sit_at_controls(&mut t.games[1], 1);
    let ahead = Kin { pos: pair.ahead * 150.0 + pair.side * 2.0, ..Kin::default() };
    let target = move |at: f64| Kin { pos: pair.lead(at).pos + ahead.pos, rot: Quat::from_rotation_y(0.4), ..pair.lead(at) };
    let t0 = t.now;
    for g in &mut t.games {
        put(g, 0, &pair.lead(0.0));
        put(g, 1, &target(0.0));
    }
    let fly = move |i: usize, g: &mut Game, now: f64| match i {
        0 => put(g, 0, &pair.lead(now - t0)),
        1 => put(g, 1, &target(now - t0)),
        _ => {}
    };
    t.run(1.0, 1.0 / 60.0, true, fly);
    let b = |g: &Game| g.ships.list[1].structure;
    let whole: Vec<Vec<(bool, u32)>> = t.games.iter().map(|g| wounds(g, b(g))).collect();
    assert!(whole.iter().all(|w| *w == whole[0]) && !whole[0].is_empty());
    // a burst of 30 rounds, one a frame, from Ana's gun (the way any weapon lets fly: told once;
    // the others' games fly what they are told, from their copy of her ship)
    for g in &mut t.games {
        g.blasts.log_ends = true;
    }
    for _ in 0..30 {
        t.frame(1.0 / 60.0, true, |i, g, now| {
            fly(i, g, now);
            if i == 0 {
                let (a, target) = (kin_of(g, 0), kin_of(g, 1));
                let muzzle = a.pos + pair.ahead * 12.0;
                let dir = (target.pos - muzzle).normalize();
                let bodies = defs().system.bodies.clone();
                let by = Some(g.ships.list[0].structure);
                assert!(g.blasts.fire_from("canon_20", muzzle, dir, a.vel, by, &bodies, &mut g.builds));
            }
        });
    }
    // the rounds fly the 140 m in an eighth of a second; what they struck goes round and is done
    t.run(1.5, 1.0 / 60.0, true, fly);
    let ana = &t.games[0];
    assert!(ana.blasts.impact_count >= 20, "of 30 rounds, {} struck", ana.blasts.impact_count);
    assert_eq!(ana.blasts.last_impact.and_then(|i| i.surface).map(|s| s.id), Some(b(ana)), "the rounds struck something else");
    let after: Vec<Vec<(bool, u32)>> = t.games.iter().map(|g| wounds(g, b(g))).collect();
    let hurt = after[0].iter().zip(&whole[0]).filter(|(x, y)| x != y).count();
    eprintln!("30 disparos a 2 km/s: {} impactos en la nave de Berto, {hurt} de sus {} piezas tocadas", ana.blasts.impact_count, after[0].len());
    assert!(hurt > 0, "nothing was hurt");
    for (i, w) in after.iter().enumerate() {
        let differ = w.iter().zip(&after[0]).filter(|(x, y)| x != y).count();
        assert!(*w == after[0], "game {i} ends with {differ} parts unlike Ana's");
    }
    // (and the pieces that came off are the same in every game)
    let count: Vec<usize> = t.games.iter().map(|g| g.builds.set.list.len()).collect();
    assert!(count.iter().all(|c| *c == count[0]), "structures in each game: {count:?}");
    // every game saw every round go off, each where it did the damage, on its copy of the ship
    same_ends(&t, ana.blasts.impact_count as usize);
}

/// Every game saw `count` things go off, the same ones on the same structures in the same places
/// of them (to a centimetre): what every game sees go off is where the damage was done.
fn same_ends(t: &Table, count: usize) {
    let on = |g: &Game| -> Vec<(crate::blasts::What, Option<Vec3>)> { g.blasts.ended.iter().map(|(w, l)| (*w, l.map(|l| l.1))).collect() };
    let first = on(&t.games[0]);
    for (i, g) in t.games.iter().enumerate() {
        let e = on(g);
        assert_eq!(e.len(), count, "game {i} saw {} go off of {count}", e.len());
        let mut worst = 0.0f32;
        for (w, p) in &e {
            let near = first.iter().filter(|(x, _)| x == w).map(|(_, q)| match (p, q) {
                (Some(p), Some(q)) => p.distance(*q),
                (None, None) => 0.0,
                _ => f32::MAX,
            });
            worst = worst.max(near.fold(f32::MAX, f32::min));
        }
        assert!(worst < 0.01, "game {i} saw one go off {worst:.3} m from where it did");
    }
}

#[test]
fn hits_from_two_shooters_at_once_are_done_in_one_order_everywhere() {
    // Ana and Carla strike Berto's ship in the same frames, again and again, over a network that
    // loses, duplicates and reorders: each game does them in the order the server passed them on,
    // its own too, and all three end the same, bit for bit
    let bad = Conditions { loss: 0.1, duplicate: 0.05, delay: 0.03, jitter: 0.04 };
    let mut t = Table::new(&["Ana", "Berto", "Carla"], 33, bad);
    sit_at_controls(&mut t.games[1], 1);
    t.run(0.5, 1.0 / 60.0, false, |_, _, _| {});
    let b = |g: &Game| g.ships.list[1].structure;
    let before = wounds(&t.games[0], b(&t.games[0]));
    for n in 0..40 {
        t.frame(1.0 / 60.0, false, |i, g, _| {
            if i == 1 {
                return;
            }
            let s = g.builds.set.get(b(g)).unwrap();
            // (each where it strikes, a different part each time)
            let p = s.parts[(n * 7 + i * 13) % s.parts.len()].center;
            let hit = lunar_core::structure::damage::Hit { point: p, dir: Vec3::new(0.3, -1.0, 0.2).normalize(), energy: 4e4, radius: 0.0, area: 3e-4 };
            let id = b(g);
            g.builds.hit(id, &hit);
            // (decided here, not done yet: it is done when it comes back)
            assert!(g.builds.set.get(id).unwrap().shared);
        });
    }
    t.run(2.0, 1.0 / 60.0, false, |_, _, _| {});
    let all: Vec<Vec<(bool, u32)>> = t.games.iter().map(|g| wounds(g, b(g))).collect();
    let hurt = all[0].iter().zip(&before).filter(|(x, y)| x != y).count();
    assert!(hurt > 5, "only {hurt} parts hurt");
    for (i, w) in all.iter().enumerate() {
        assert!(*w == all[0], "game {i} is not as Ana's");
    }
}

#[test]
fn what_a_player_fires_leaves_their_muzzle_in_every_game_however_fast_they_go() {
    // Ana, sat in her ship at orbital speed, fires the launcher's rocket ahead: Berto sees it leave
    // from beside his copy of her ship and fly as hers does
    let bad = Conditions { loss: 0.0, duplicate: 0.0, delay: 0.05, jitter: 0.01 };
    let mut t = Table::new(&["Ana", "Berto"], 44, bad);
    let pair = Pair::new(&t.bodies, 7800.0);
    sit_at_controls(&mut t.games[0], 0);
    let t0 = t.now;
    for g in &mut t.games {
        put(g, 0, &pair.lead(0.0));
    }
    let fly = move |i: usize, g: &mut Game, now: f64| {
        if i == 0 {
            put(g, 0, &pair.lead(now - t0));
        }
    };
    t.run(1.0, 1.0 / 60.0, true, fly);
    let bodies = t.bodies.clone();
    t.frame(1.0 / 60.0, true, |i, g, now| {
        fly(i, g, now);
        if i == 0 {
            let a = kin_of(g, 0);
            assert!(g.blasts.fire_from("cohete", a.pos + pair.ahead * 12.0 + pair.up * 2.0, pair.ahead, a.vel, None, &bodies, &mut g.builds));
        }
    });
    let Some(crate::blasts::What::Shot(rocket)) = t.games[0].blasts.what("cohete") else { panic!("no rocket") };
    let mut worst = 0.0f64;
    let mut seen = 0;
    for _ in 0..40 {
        t.frame(1.0 / 60.0, true, fly);
        // each game's rocket, from its own copy of Ana's ship
        let from_ship = |g: &Game| {
            let a = kin_of(g, 0);
            g.blasts.rounds.list.iter().find(|r| r.kind == rocket).map(|r| r.pos - a.pos)
        };
        if let (Some(hers), Some(his)) = (from_ship(&t.games[0]), from_ship(&t.games[1])) {
            worst = worst.max(hers.distance(his));
            seen += 1;
        }
    }
    eprintln!("el cohete de Ana visto por Berto a 7,8 km/s: a {worst:.3} m de donde ella lo tiene, junto a la nave ({seen} fotogramas)");
    assert!(seen > 25, "Berto saw it in {seen} frames");
    assert!(worst < 0.5, "Berto has her rocket {worst:.3} m off");
}

/// Which of a game's ships is of kind `kind`.
fn ship_of(g: &Game, kind: &str) -> usize {
    g.ships.list.iter().position(|sh| sh.kind.id == kind).unwrap_or_else(|| panic!("no {kind} in the scenario"))
}

#[test]
fn a_rocket_fired_out_of_an_open_hold_at_orbital_speed_strikes_another_ship_the_same_in_every_game() {
    // Ana flies the Alcotán at 7.8 km/s with its ramp down; Berto stands in its hold and fires
    // the launcher out of the back at the Abejorro, which Carla flies 160 m behind. Every game sees
    // the rocket leave Berto's muzzle, fly out of the hold without touching the Alcotán and go off
    // on the Abejorro where it did its damage, and every game ends with the same damage.
    let bad = Conditions { loss: 0.03, duplicate: 0.01, delay: 0.04, jitter: 0.01 };
    let mut t = Table::new(&["Ana", "Berto", "Carla"], 77, bad);
    let (a, b) = (ship_of(&t.games[0], "alcotan"), ship_of(&t.games[0], "abejorro"));
    let pair = Pair::new(&t.bodies, 7800.0);
    sit_at_controls(&mut t.games[0], a);
    sit_at_controls(&mut t.games[2], b);
    let behind = Vec3::new(0.0, 0.5, -160.0);
    let target = move |at: f64| {
        let lead = pair.lead(at);
        Kin { pos: lead.pos + (lead.rot * behind).as_dvec3(), ..lead }
    };
    let t0 = t.now;
    for g in &mut t.games {
        g.blasts.log_ends = true;
        put(g, a, &pair.lead(0.0));
        put(g, b, &target(0.0));
    }
    let fly = move |i: usize, g: &mut Game, now: f64| match i {
        0 => put(g, a, &pair.lead(now - t0)),
        2 => put(g, b, &target(now - t0)),
        _ => {}
    };
    // (Berto in the hold, on its floor, before the ramp)
    let berto = &mut t.games[1];
    let hold = berto.ships.list[a].structure;
    // (the room the ramp opens from: its floor, from the ship's data)
    let floor = berto.ships.list[a].kind.rooms.iter().find(|r| r[0].z < -7.0 && r[1].z > -7.0).expect("a hold at the back")[0].y;
    berto.pilot.put_on(&berto.builds.set, hold, Vec3::new(0.0, floor + 0.05, -7.0));
    for _ in 0..120 {
        t.frame(1.0 / 60.0, true, fly);
        if t.games[0].multi.owns(t.games[0].ships.list[a].structure) && t.games[2].multi.owns(t.games[2].ships.list[b].structure) {
            break;
        }
    }
    t.run(1.0, 1.0 / 60.0, true, fly);
    // (on its floor if the ship is up and gives weight; floating in it if not: in its frame
    // either way, by it however fast it goes)
    let in_hold = |g: &Game| g.builds.set.get(hold).unwrap().to_local(g.pilot.position);
    let at = in_hold(&t.games[1]);
    assert!(at.z < -3.4 && at.z > -9.3 && at.x.abs() < 2.0 && at.y > floor - 0.1 && at.y < floor + 3.0, "Berto is not in the hold: {at:?}");
    let whole: Vec<(Vec<(bool, u32)>, Vec<(bool, u32)>)> = t.games.iter().map(|g| (wounds(g, g.ships.list[a].structure), wounds(g, g.ships.list[b].structure))).collect();
    // the shot, from his eyes, at the Abejorro he sees out of the open ramp
    let bodies = t.bodies.clone();
    t.frame(1.0 / 60.0, true, |i, g, now| {
        fly(i, g, now);
        if i == 1 {
            let s = g.builds.set.get(hold).unwrap();
            let muzzle = s.to_world(Vec3::new(0.2, floor + 1.3, -7.6));
            let aim = (kin_of(g, b).pos - muzzle).normalize();
            assert!(g.blasts.fire_from("cohete", muzzle, aim, s.velocity_at(muzzle), None, &bodies, &mut g.builds));
        }
    });
    let Some(crate::blasts::What::Shot(rocket)) = t.games[0].blasts.what("cohete") else { panic!() };
    let mut worst = 0.0f64;
    let mut seen = 0;
    for _ in 0..150 {
        t.frame(1.0 / 60.0, true, fly);
        // each game's rocket, from its own copy of the Alcotán
        let from_hold = |g: &Game| g.blasts.rounds.list.iter().find(|r| r.kind == rocket).map(|r| r.pos - kin_of(g, a).pos);
        if let Some(his) = from_hold(&t.games[1]) {
            for g in [&t.games[0], &t.games[2]] {
                if let Some(theirs) = from_hold(g) {
                    worst = worst.max(theirs.distance(his));
                    seen += 1;
                }
            }
        }
    }
    t.run(1.0, 1.0 / 60.0, true, fly);
    let berto = &t.games[1];
    eprintln!("cohete desde la bodega abierta a 7,8 km/s: los demás lo ven a {worst:.3} m de donde Berto lo tiene ({seen} vistas); {} impacto(s)", berto.blasts.impact_count);
    assert_eq!(berto.blasts.impact_count, 1, "the rocket did not strike once");
    assert_eq!(berto.blasts.last_impact.and_then(|i| i.surface).map(|s| s.id), Some(berto.ships.list[b].structure), "it struck something else (the hold?)");
    assert!(seen > 100, "the others saw it in {seen} frames");
    assert!(worst < 0.5, "the others have it {worst:.3} m off");
    // the Alcotán untouched, the Abejorro hurt, the same in every game
    let after: Vec<(Vec<(bool, u32)>, Vec<(bool, u32)>)> = t.games.iter().map(|g| (wounds(g, g.ships.list[a].structure), wounds(g, g.ships.list[b].structure))).collect();
    assert_eq!(after[1].0, whole[1].0, "the hold was hurt");
    assert!(after[1].1 != whole[1].1, "the Abejorro was not hurt");
    for (i, w) in after.iter().enumerate() {
        assert!(*w == after[0], "game {i} ends unlike Ana's");
    }
    same_ends(&t, 1);
}

#[test]
fn a_guided_missile_flies_the_same_in_every_game_and_goes_off_where_it_struck() {
    // Ana's ship fires a radar missile at Berto's Cachalote, 1.5 km off and weaving, all at 2 km/s; Carla
    // watches. Every game flies it by what its own sensors see of Berto's ship and is told now and
    // then where Ana's is: theirs stay by hers, and go off where hers struck, with the same damage.
    let bad = Conditions { loss: 0.03, duplicate: 0.01, delay: 0.04, jitter: 0.01 };
    let mut t = Table::new(&["Ana", "Berto", "Carla"], 88, bad);
    let pair = Pair::new(&t.bodies, 2000.0);
    let c = ship_of(&t.games[0], "cachalote");
    sit_at_controls(&mut t.games[0], 0);
    sit_at_controls(&mut t.games[1], c);
    let target = move |at: f64| {
        let lead = pair.lead(at);
        let (w, a) = (1.3, 40.0);
        Kin { pos: lead.pos + pair.ahead * 1500.0 + pair.side * (a * (w * at).sin()), vel: lead.vel + pair.side * (a * w * (w * at).cos()), ..lead }
    };
    let t0 = t.now;
    for g in &mut t.games {
        g.blasts.log_ends = true;
        put(g, 0, &pair.lead(0.0));
        put(g, c, &target(0.0));
    }
    // (each game's sensors: where its copy of Berto's ship is, bright)
    let fly = move |i: usize, g: &mut Game, now: f64| {
        match i {
            0 => put(g, 0, &pair.lead(now - t0)),
            1 => put(g, c, &target(now - t0)),
            _ => {}
        }
        let id = g.ships.list[c].structure;
        let k = kin_of(g, c);
        g.blasts.aims.clear();
        g.blasts.aims.push((id, lunar_core::guided::Aim { pos: k.pos, vel: k.vel, rcs: 60.0, heat: 2e7 }));
    };
    t.run(1.0, 1.0 / 60.0, true, fly);
    let whole: Vec<Vec<(bool, u32)>> = t.games.iter().map(|g| wounds(g, g.ships.list[c].structure)).collect();
    let bodies = t.bodies.clone();
    t.frame(1.0 / 60.0, true, |i, g, now| {
        fly(i, g, now);
        if i == 0 {
            let a = kin_of(g, 0);
            let Some(crate::blasts::What::Guided(kind)) = g.blasts.what("lanza") else { panic!("no lanza") };
            let (by, at) = (g.ships.list[0].structure, g.ships.list[c].structure);
            let from = a.pos + pair.ahead * 14.0;
            let l = crate::blasts::Launch { what: crate::blasts::What::Guided(kind), from, dir: pair.ahead, speed: crate::blasts::RAIL, vel: a.vel, target: Some(at), by: Some(by) };
            assert!(g.blasts.launch(l, &bodies, &mut g.builds));
        }
    });
    // how far each copy is from hers, against what no copy can do better than: how fast it
    // closes on the ship times how far apart the games' clocks are (`follow`), and half a
    // millisecond of its flight (each game steps it on its own fixed steps)
    let (mut worst, mut allowed, mut over, mut seen) = (0.0f64, 0.0f64, f64::MIN, 0);
    for _ in 0..(8.0 * 60.0) as usize {
        t.frame(1.0 / 60.0, true, fly);
        // each game's missile, from its own copy of Berto's ship
        let near = |g: &Game| g.blasts.guided.list.first().map(|m| (m.pos - kin_of(g, c).pos, m.vel - kin_of(g, c).vel));
        let clock = |g: &Game| g.multi.client.server_time(t.now).unwrap_or(t.now);
        if let Some((hers, closing)) = near(&t.games[0]) {
            for g in &t.games[1..] {
                if let Some((theirs, _)) = near(g) {
                    let apart = (clock(g) - clock(&t.games[0])).abs() + lunar_net::clock::SLEW * 0.25;
                    let off = theirs.distance(hers);
                    let may = 0.3 + closing.length() * (apart * 1.2 + 0.0005);
                    if off > worst {
                        (worst, allowed) = (off, may);
                    }
                    over = over.max(off - may);
                    seen += 1;
                }
            }
        }
    }
    let ana = &t.games[0];
    eprintln!("misil guiado a 2 km/s contra una nave que esquiva: los demás lo ven a {worst:.2} m del de Ana (los relojes permiten {allowed:.2} m; {seen} vistas); {} final(es)", ana.blasts.ends);
    assert!(ana.blasts.guided.list.is_empty() && t.games.iter().all(|g| g.blasts.guided.list.is_empty()), "a missile is still flying");
    assert!(seen > 100, "the others saw it in {seen} frames");
    assert!(over <= 0.0, "the others have it {over:.2} m further off than the clocks allow");
    let after: Vec<Vec<(bool, u32)>> = t.games.iter().map(|g| wounds(g, g.ships.list[c].structure)).collect();
    assert!(after[0] != whole[0], "Berto's ship was not hurt");
    for (i, w) in after.iter().enumerate() {
        assert!(*w == after[0], "game {i} ends unlike Ana's");
    }
    same_ends(&t, 1);
}

/// The game's astronaut, measured once (to draw the others).
fn source() -> Option<&'static BodySource> {
    static SOURCE: OnceLock<Option<BodySource>> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            let root = crate::root();
            let rig: crate::rig::RigDef = lunar_core::defs::load(&root.join("assets/defs/rigs/astronauta.jsonc")).ok()?;
            let model = crate::rig::Rigged::load(&root.join("assets/models").join(format!("{}.glb", rig.modelo))).ok()?;
            Some(BodySource { rig: Rig::new(rig, &model).ok()?, whole: None })
        })
        .as_ref()
}

#[test]
fn a_player_floating_by_a_ship_at_orbital_speed_is_drawn_beside_it_not_where_they_were() {
    // Berto floats 15 m off Ana's ship as it goes at 7.8 km/s: Ana draws him there, not 780 m
    // back where he was a tenth of a second ago
    let Some(source) = source() else { return };
    let bad = Conditions { loss: 0.03, duplicate: 0.01, delay: 0.04, jitter: 0.01 };
    let mut t = Table::new(&["Ana", "Berto"], 55, bad);
    let pair = Pair::new(&t.bodies, 7800.0);
    sit_at_controls(&mut t.games[0], 0);
    let t0 = t.now;
    let off = pair.side * 15.0 + pair.up * 2.0;
    for g in &mut t.games {
        put(g, 0, &pair.lead(0.0));
    }
    let fly = move |i: usize, g: &mut Game, now: f64| {
        let at = now - t0;
        if i == 0 {
            put(g, 0, &pair.lead(at));
        } else {
            g.pilot.position = pair.lead(at).pos + off;
        }
    };
    t.run(1.5, 1.0 / 60.0, true, fly);
    let mut scene = BodyScene::default();
    let (mut worst, mut clocks) = (0.0f64, 0.0f64);
    for _ in 0..120 {
        t.frame(1.0 / 60.0, true, fly);
        let now = t.now;
        let bodies = t.bodies.clone();
        let ana = &mut t.games[0];
        scene.clear();
        ana.multi.bodies(now, 1.0 / 60.0, source, &ana.builds.set, &bodies, &ana.ships, &mut scene);
        let (eye, name) = ana.multi.names().next().map(|(e, n)| (e, n.to_string())).expect("Berto is drawn");
        assert_eq!(name, "Berto");
        // (from Ana's own ship, which she has exactly)
        let truth = kin_of(ana, 0).pos + off;
        worst = worst.max(eye.distance(truth));
        let clock = |g: &Game| g.multi.client.server_time(now).unwrap_or(now) - now;
        clocks = clocks.max((clock(&t.games[0]) - clock(&t.games[1])).abs());
    }
    let allowed = 0.3 + 7800.0 * (clocks + lunar_net::clock::SLEW * 0.25);
    eprintln!("Berto flotando junto a la nave de Ana a 7,8 km/s: dibujado a {worst:.3} m de donde está (relojes a {:.2} ms: {allowed:.2} m)", clocks * 1000.0);
    assert!(worst < allowed, "Berto drawn {worst:.2} m from where he is");
}

#[test]
fn whoever_stands_aboard_a_ship_another_flies_at_orbital_speed_stays_where_they_stand() {
    // Carla stands in the hold of Berto's ship (the Cachalote), at 7.8 km/s: her copy of it is brought where Berto
    // has it every frame, and she goes with it, without sliding on its deck
    let bad = Conditions { loss: 0.03, duplicate: 0.01, delay: 0.04, jitter: 0.01 };
    let mut t = Table::new(&["Berto", "Carla"], 66, bad);
    let pair = Pair::new(&t.bodies, 7800.0);
    sit_at_controls(&mut t.games[0], 2);
    let t0 = t.now;
    for g in &mut t.games {
        put(g, 2, &pair.lead(0.0));
    }
    let fly = move |i: usize, g: &mut Game, now: f64| {
        if i == 0 {
            put(g, 2, &pair.lead(now - t0));
        }
    };
    // (aboard from the start, on the floor in the middle of its first room)
    let carla = &mut t.games[1];
    let ship = carla.ships.list[2].structure;
    let feet = {
        let [lo, hi] = carla.ships.list[2].kind.rooms[0];
        Vec3::new((lo.x + hi.x) * 0.5, lo.y + 0.05, (lo.z + hi.z) * 0.5)
    };
    carla.pilot.put_on(&carla.builds.set, ship, feet);
    // (once the ship is Berto's in both games and Carla's copy follows his)
    for _ in 0..120 {
        t.frame(1.0 / 60.0, true, fly);
        if t.games[0].multi.owns(t.games[0].ships.list[2].structure) && !t.games[1].multi.owns(t.games[1].ships.list[2].structure) {
            break;
        }
    }
    t.run(1.0, 1.0 / 60.0, true, fly);
    let local = |g: &Game| g.builds.set.get(ship).unwrap().to_local(g.pilot.position);
    let start = local(&t.games[1]);
    let (mut slid, mut jump) = (0.0f32, 0.0f32);
    let mut was = start;
    for _ in 0..180 {
        t.frame(1.0 / 60.0, true, fly);
        let here = local(&t.games[1]);
        slid = slid.max(here.distance(start));
        jump = jump.max(here.distance(was));
        was = here;
    }
    eprintln!("Carla de pie en la nave de Berto a 7,8 km/s: se mueve {:.1} mm sobre la cubierta en 3 s (como mucho {:.2} mm de un fotograma a otro)", slid * 1000.0, jump * 1000.0);
    assert!(t.games[1].pilot.ride.is_some_and(|r| r.id == ship), "she is no longer aboard");
    // (her feet settle on the floor once: they were put 5 cm over it)
    assert!(slid < 0.02 && jump < 0.01, "she slides {slid:.3} m (a frame: {jump:.4} m)");
}

#[test]
fn two_games_through_a_server_agree_on_players_controls_ships_made_and_owners() {
    let Some(source) = source() else { return };
    let mut t = Table::new(&["Ana", "Berto"], 11, Conditions::default());
    let bodies = t.bodies.clone();
    let mut scene = BodyScene::default();
    let draw = |t: &mut Table, scene: &mut BodyScene| {
        let now = t.now;
        for g in &mut t.games {
            scene.clear();
            g.multi.bodies(now, 1.0 / 60.0, source, &g.builds.set, &bodies, &g.ships, scene);
        }
    };
    // ---- each sees the other where they are, by their name
    let ana = &mut t.games[0];
    let body = bodies.get(ana.pilot.body);
    let up = body.up(ana.pilot.position);
    let east = up.any_orthonormal_vector();
    let feet = body.above_ground(body.up(ana.pilot.position + east * 7.0), 0.0);
    ana.pilot.put(feet, body.up(feet));
    t.run(2.0, 1.0 / 60.0, false, |_, _, _| {});
    draw(&mut t, &mut scene);
    assert_eq!((t.games[0].multi.others(), t.games[1].multi.others()), (1, 1));
    let (eye, name) = t.games[1].multi.names().next().map(|(e, n)| (e, n.to_string())).unwrap();
    assert_eq!(name, "Ana");
    assert!(eye.distance(t.games[0].pilot.position) < 0.02, "Berto has Ana {:.3} m from where she is", eye.distance(t.games[0].pilot.position));
    assert!(!scene.bodies.is_empty(), "the other is drawn");
    // ---- a control worked on one copy is set the same on the other
    let a = &mut t.games[0];
    let structure = a.ships.list[0].structure;
    let (k, value) = {
        let (sh, s) = (&mut a.ships.list[0], a.builds.set.get(structure).unwrap());
        let kind = sh.kind.clone();
        let k = (0..sh.panels.controls.len()).find(|&k| sh.panels.intent(k, &lunar_controls::Intent::Press { elem: 0 }, s, &kind, &sh.store).changed).expect("a control that a press changes");
        let c = &sh.panels.controls[k];
        (k, c.mech.value(&c.st))
    };
    a.multi.control(structure, k as u16, value);
    t.run(1.0, 1.0 / 60.0, false, |_, _, _| {});
    let theirs = {
        let c = &t.games[1].ships.list[0].panels.controls[k];
        c.mech.value(&c.st)
    };
    assert!((theirs - value).abs() < 1e-9, "control {k}: {value} on one copy, {theirs} on the other");
    // ---- a door pushed open by a hand is open on the other copy too
    let a = &mut t.games[0];
    let n = (0..a.ships.list.len()).find(|&n| !a.ships.list[n].kind.closures.is_empty()).expect("a ship with a door");
    let order = crate::aboard::closure_order(&a.ships.list[n], 0).unwrap();
    assert_eq!(t.games[1].ships.list[n].signal(&order).unwrap_or(0.0), 0.0);
    let a = &mut t.games[0];
    a.multi.act(a.ships.list[n].structure, crate::aboard::Act::Closure(0, true));
    t.run(1.0, 1.0 / 60.0, false, |_, _, _| {});
    assert_eq!(t.games[1].ships.list[n].signal(&order), Some(1.0), "the door of {}", t.games[1].ships.list[n].kind.id);
    // ---- a ship made in one game is made in the other, once in each
    let count = t.games[0].ships.list.len();
    let a = &mut t.games[0];
    let made = a.ships.spawn_free(&mut a.builds, "abejorro", a.pilot.position + up * 40.0, Quat::IDENTITY).unwrap();
    a.multi.made("abejorro", made, &a.builds.set, &a.ships);
    t.run(1.0, 1.0 / 60.0, false, |_, _, _| {});
    assert_eq!((t.games[0].ships.list.len(), t.games[1].ships.list.len()), (count + 1, count + 1));
    // ---- who sits at the controls of a ship owns it, and where they take it the other copy goes
    sit_at_controls(&mut t.games[1], 0);
    t.run(1.0, 1.0 / 60.0, false, |_, _, _| {});
    let flown = t.games[1].ships.list[0].structure;
    assert!(t.games[1].multi.owns(flown) && !t.games[0].multi.owns(structure), "whoever sits at the controls owns the ship");
    let before = t.games[0].builds.set.get(structure).unwrap().pos;
    let lift = up * 6.0;
    t.run(1.5, 1.0 / 60.0, false, |i, g, _| {
        if i == 1 {
            // (held there by its owner: that game is what says where it is)
            put(g, 0, &Kin { pos: before + lift, ..kin_of(g, 0) });
            let s = g.builds.set.list.iter_mut().find(|s| s.id == flown).unwrap();
            (s.vel, s.spin) = (DVec3::ZERO, Vec3::ZERO);
        }
    });
    let ours = t.games[0].builds.set.get(structure).unwrap().pos;
    assert!((ours - before - lift).length() < 0.05, "the owner has it 6 m up, the other copy is {:.3} m from there", (ours - before - lift).length());
    // (the copy of a ship someone else owns is theirs to say what its machines do, and what is
    // done to it is done by every game in one order)
    let s = t.games[0].builds.set.get(structure).unwrap();
    assert!(s.remote && s.shared);
    let s = t.games[1].builds.set.get(flown).unwrap();
    assert!(!s.remote && s.shared);
    // (and the seated player is seen, sat in it)
    draw(&mut t, &mut scene);
    assert_eq!(t.games[0].multi.others(), 1);
}

#[test]
fn a_body_faces_where_its_player_does() {
    // (as the player's own: with north -z where up is +y, east is +x)
    let up = DVec3::Y;
    assert!((facing(DVec3::NEG_Z, up, 0.0) - DVec3::NEG_Z).length() < 1e-12);
    assert!((facing(DVec3::NEG_Z, up, std::f64::consts::FRAC_PI_2) - DVec3::X).length() < 1e-12);
    // level wherever up is
    let up = DVec3::new(0.4, 0.5, -0.3).normalize();
    let north = up.any_orthonormal_vector();
    for yaw in [0.0, 1.0, 2.5, -2.0] {
        let f = facing(north, up, yaw);
        assert!(f.dot(up).abs() < 1e-12 && (f.length() - 1.0).abs() < 1e-12);
    }
}
/// What every game has, as each has it: each shared structure (by the name every game knows it
/// by: pieces come off too) and its parts, bit for bit; and how many structures there are in all.
fn shared_state(g: &Game) -> (Vec<(u64, Vec<(bool, u32)>)>, usize) {
    let mut out: Vec<(u64, Vec<(bool, u32)>)> = g.builds.set.list.iter().filter(|s| s.shared).map(|s| (s.lineage, s.parts.iter().map(|p| (p.alive, p.hp.to_bits())).collect())).collect();
    out.sort_by_key(|s| s.0);
    (out, g.builds.set.list.len())
}

/// Where structure `id` is at the world's moment (none: gone), as `kin_of`.
fn kin_at(g: &Game, id: u64) -> Option<(Kin, DVec3)> {
    let s = g.builds.set.get(id)?;
    let behind = (g.builds.set.now - s.clock).max(0.0);
    let carried = s.vel * behind;
    Some((Kin { pos: s.pos + carried, vel: s.vel, rot: s.rot, spin: s.spin }, s.to_world(s.com) + carried))
}

#[test]
fn every_weapon_in_the_data_is_seen_and_does_the_same_in_every_game() {
    // Everything the definitions say can be let fly or set off (every shot, missile, guided
    // missile, decoy and explosion: a weapon added to the data is tried here without a line
    // more), each from Ana's ship at 2 km/s at an Abejorro she puts beside it for it (made in
    // every game: what is made in play is too); Berto and Carla watch. Each is seen in every
    // game, goes off in the same place in each, and every game ends each with the same damage,
    // bit for bit — the pieces that come off included.
    let bad = Conditions { loss: 0.03, duplicate: 0.01, delay: 0.04, jitter: 0.01 };
    let mut t = Table::new(&["Ana", "Berto", "Carla"], 99, bad);
    let pair = Pair::new(&t.bodies, 2000.0);
    sit_at_controls(&mut t.games[0], 0);
    let beside = move |at: f64| {
        let lead = pair.lead(at);
        Kin { pos: lead.pos + pair.ahead * 60.0 + pair.side * 3.0, ..lead }
    };
    let t0 = t.now;
    for g in &mut t.games {
        g.blasts.log_ends = true;
        put(g, 0, &pair.lead(0.0));
    }
    // (Ana flies hers; the newest ship is where it goes in the game of whoever owns it — a ship
    // nobody sits in is the host's, whoever made it)
    let fly = move |i: usize, g: &mut Game, now: f64| {
        let at = now - t0;
        if i == 0 {
            put(g, 0, &pair.lead(at));
        }
        // (while there is anything of it: a weapon may leave nothing)
        let k = g.ships.list.len() - 1;
        let id = g.ships.list[k].structure;
        if k > 0 && g.multi.owns(id) && g.builds.set.get(id).is_some() {
            put(g, k, &beside(at));
        }
        if let Some((k, mass)) = g.ships.list.last().and_then(|sh| kin_at(g, sh.structure)).map(|(k, m)| (k, m)) {
            let id = g.ships.list.last().unwrap().structure;
            g.blasts.aims.clear();
            g.blasts.aims.push((id, lunar_core::guided::Aim { pos: mass, vel: k.vel, rcs: 60.0, heat: 2e7 }));
        }
    };
    let dt = std::env::var("FPS").ok().and_then(|f| f.parse::<f64>().ok()).map_or(1.0 / 30.0, |f| 1.0 / f);
    t.run(0.5, dt, true, fly);
    let bodies = t.bodies.clone();
    let every = t.games[0].blasts.every();
    assert!(every.len() >= 20, "{every:?}");
    let mut report = Vec::new();
    for (name, what) in every {
        // a fresh target, in every game
        let count: Vec<usize> = t.games.iter().map(|g| g.ships.list.len()).collect();
        let ana = &mut t.games[0];
        let at = beside(t.now - t0);
        let made = ana.ships.spawn_free(&mut ana.builds, "abejorro", at.pos, at.rot).unwrap();
        ana.multi.made("abejorro", made, &ana.builds.set, &ana.ships);
        for _ in 0..60 {
            t.frame(dt, true, fly);
            if t.games.iter().zip(&count).all(|(g, n)| g.ships.list.len() == n + 1) {
                break;
            }
        }
        assert!(t.games.iter().zip(&count).all(|(g, n)| g.ships.list.len() == n + 1), "{name}: the target was not made in every game");
        t.run(0.3, dt, true, fly);
        let before: Vec<u64> = t.games.iter().map(|g| g.blasts.ends).collect();
        let started: Vec<u64> = t.games.iter().map(|g| g.blasts.started).collect();
        t.frame(dt, true, |i, g, now| {
            fly(i, g, now);
            if i != 0 {
                return;
            }
            let a = kin_of(g, 0);
            let (by, target) = (g.ships.list[0].structure, g.ships.list.last().unwrap().structure);
            let from = a.pos + pair.ahead * 14.0;
            let (_, mass) = kin_at(g, target).expect("the target is there");
            let aim = (mass - from).normalize();
            let l = match what {
                crate::blasts::What::Shot(_) => g.blasts.shot(&name, from, aim, a.vel, Some(by)).unwrap(),
                crate::blasts::What::Missile(_) => crate::blasts::Launch { what, from, dir: aim, speed: 300.0, vel: a.vel, target: None, by: Some(by) },
                crate::blasts::What::Guided(_) => crate::blasts::Launch { what, from, dir: aim, speed: crate::blasts::RAIL, vel: a.vel, target: Some(target), by: Some(by) },
                crate::blasts::What::Decoy(_) => crate::blasts::Launch { what, from, dir: pair.up, speed: 30.0, vel: a.vel, target: None, by: Some(by) },
                // (an explosion set off beside the target's hull)
                crate::blasts::What::Boom(_) => crate::blasts::Launch { what, from: mass - aim * 4.0, dir: aim, speed: 0.0, vel: a.vel, target: None, by: Some(by) },
            };
            assert!(g.blasts.launch(l, &bodies, &mut g.builds), "{name} could not be let fly");
        });
        // until nothing of it flies in Ana's game, and a little more for the word to go round
        let mut left = 0.6;
        for _ in 0..(3.0 / dt) as usize {
            t.frame(dt, true, fly);
            if !t.games[0].blasts.flying() {
                left -= dt;
                if left <= 0.0 {
                    break;
                }
            }
        }
        // (seen: started in every game, once — Ana's own, the others' copy of it)
        let seen: Vec<u64> = t.games.iter().zip(&started).map(|(g, b)| g.blasts.started - b).collect();
        let ends: Vec<u64> = t.games.iter().zip(&before).map(|(g, b)| g.blasts.ends - b).collect();
        let states: Vec<_> = t.games.iter().map(shared_state).collect();
        report.push(format!("{name:>22} ({what:?}): visto en {seen:?}, finales {ends:?}, estructuras {}", t.games[0].builds.set.list.len()));
        let say = || report.join("\n");
        assert!(seen.iter().all(|s| *s == 1), "{name}: started {seen:?} times\n{}", say());
        assert!(ends.iter().all(|e| *e == ends[0]), "{name}: ended {ends:?} times\n{}", say());
        for (i, st) in states.iter().enumerate() {
            assert!(st.1 == states[0].1, "{name}: game {i} has {} structures, Ana's {}\n{}", st.1, states[0].1, say());
            assert!(st.0 == states[0].0, "{name}: game {i} ends unlike Ana's\n{}", say());
        }
    }
    same_ends(&t, t.games[0].blasts.ended.len());
    eprintln!("{}", report.join("\n"));
}
