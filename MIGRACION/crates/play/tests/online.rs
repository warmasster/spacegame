//! The game over a server that has it (`lunar_play::host`) and players' games that predict their
//! own (`lunar_play::online`), through the real network code over a network of their own that
//! loses, delays and reorders datagrams on purpose (`lunar_net::MemoryNet`, the same every run).
use glam::{DVec3, Quat};
use lunar_core::structure::schedule::coasted;
use lunar_net::{Conditions, Memory, MemoryNet, Server, ServerConfig, ServerEvent};
use lunar_play::{
    defs::Defs,
    game::{Game, Player, STEP},
    host::{Host, HostConfig},
    gear::ToolKind,
    online::Online,
    pilot::{Input, Summary},
};
use std::sync::OnceLock;

const BUILD: &str = "prueba+p2";

fn defs() -> &'static Defs {
    static DEFS: OnceLock<Defs> = OnceLock::new();
    DEFS.get_or_init(|| Defs::load(&lunar_play::root().join("assets/defs")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message)))
}

fn new_game() -> Game {
    Game::new_apart(defs(), &lunar_play::root().join("assets/defs"), 2000, |_| true).unwrap()
}

/// A player's game: its connection, its world, its body; and where the body was at each step
/// (in the world, and as it says it to the server).
struct Seat {
    /// Its address on the network.
    at: lunar_net::Addr,
    online: Online,
    game: Game,
    me: Player,
    path: Vec<(u64, DVec3)>,
    said: Vec<(u64, Summary)>,
}

/// A server and some players' games on a network of their own.
struct Table {
    net: MemoryNet,
    link: Memory,
    server: Server,
    host: Host,
    seats: Vec<Seat>,
    now: f64,
    due: f64,
    /// Where each player's body was at each step, as the server has it (by network id).
    truth: Vec<(u32, u64, DVec3)>,
    told: Vec<(u32, u64, Summary)>,
}

impl Table {
    fn new(players: usize, seed: u64, cond: Conditions) -> Table {
        let net = MemoryNet::new(seed);
        net.conditions(cond);
        let link = net.endpoint();
        let addr = link.addr();
        let config = ServerConfig { game: Some((BUILD.to_string(), defs().fingerprint)), ..ServerConfig::default() };
        let host = Host::new(new_game(), defs().scenario.player, HostConfig { cheats: true, ..HostConfig::default() });
        let seats = (0..players)
            .map(|k| {
                let end = net.endpoint();
                let at = end.addr();
                let client = lunar_net::Client::with_transport(Box::new(end), addr, &format!("jugador{k}"), BUILD, defs().fingerprint);
                let game = new_game();
                let me = Player::new(game.bodies.clone(), &game.site, defs().scenario.player);
                Seat { at, online: Online::new(client, defs().scenario.player), game, me, path: Vec::new(), said: Vec::new() }
            })
            .collect();
        let mut t = Table { net, link, server: Server::new(config), host, seats, now: 10.0, due: 0.0, truth: Vec::new(), told: Vec::new() };
        for _ in 0..600 {
            t.frame(1.0 / 60.0, |_, _, _| {});
            if t.seats.iter().all(|s| s.online.live()) {
                break;
            }
        }
        assert!(t.seats.iter().all(|s| s.online.live()), "{:?}", t.seats.iter().map(|s| s.online.status()).collect::<Vec<_>>());
        t
    }

    /// One more player comes in (until their game is in the game).
    fn join(&mut self, name: &str) -> usize {
        let addr = self.link.addr();
        let end = self.net.endpoint();
        let at = end.addr();
        let client = lunar_net::Client::with_transport(Box::new(end), addr, name, BUILD, defs().fingerprint);
        let game = new_game();
        let me = Player::new(game.bodies.clone(), &game.site, defs().scenario.player);
        self.seats.push(Seat { at, online: Online::new(client, defs().scenario.player), game, me, path: Vec::new(), said: Vec::new() });
        let k = self.seats.len() - 1;
        for _ in 0..600 {
            self.frame(1.0 / 60.0, |_, _, _| {});
            if self.seats[k].online.live() {
                break;
            }
        }
        assert!(self.seats[k].online.live(), "{name} never got in: {:?}", self.seats[k].online.status());
        k
    }

    /// One frame of `dt` s for everyone: the server's steps that are due, and each player's game
    /// with what `input(player, me, step)` asks of each of its steps.
    fn frame(&mut self, dt: f64, mut input: impl FnMut(usize, &mut Player, u64)) {
        self.now += dt;
        self.net.set_time(self.now);
        self.server.update(self.now, &mut self.link);
        for e in self.server.events() {
            match e {
                ServerEvent::Joined { id, .. } => self.host.join(id),
                ServerEvent::Left { id, .. } => self.host.leave(id),
                _ => {}
            }
        }
        for m in self.server.take_game().collect::<Vec<_>>() {
            self.host.take(m.from, &m.data);
        }
        self.due += dt;
        while self.due >= STEP {
            self.due -= STEP;
            self.host.step();
            let step = self.host.game.step;
            let ids: Vec<u32> = self.host.ids().collect();
            for id in ids {
                let p = self.host.player(id).unwrap();
                self.truth.push((id, step, p.pilot.position));
                self.told.push((id, step, p.pilot.summary()));
            }
            let server = &mut self.server;
            self.host.send(|to, reliable, bytes| server.send_game(to, reliable, bytes));
        }
        self.server.update(self.now, &mut self.link);
        for (k, s) in self.seats.iter_mut().enumerate() {
            s.online.receive(self.now, &mut s.game, &mut s.me);
            for _ in 0..s.online.steps(dt) {
                input(k, &mut s.me, s.game.step);
                s.online.step(&mut s.game, &mut s.me);
                s.path.push((s.game.step, s.me.pilot.position));
                s.said.push((s.game.step, s.me.pilot.summary()));
            }
        }
    }

    /// How far each player's game had its body from where the server had it in what carries it,
    /// at the same steps (the worst of the last `steps`; infinite if not in the same frame).
    fn off_aboard(&self, k: usize, steps: u64) -> f64 {
        let id = self.seats[k].online.you.unwrap();
        let newest = self.host.game.step;
        let mut worst = 0.0f64;
        for &(who, step, at) in &self.told {
            if who != id || step + steps < newest {
                continue;
            }
            if let Some(&(_, mine)) = self.seats[k].said.iter().rev().find(|p| p.0 == step) {
                worst = worst.max(if mine.ride == at.ride { mine.pos.distance(at.pos) } else { f64::INFINITY });
            }
        }
        worst
    }

    fn run(&mut self, seconds: f64, fps: f64, mut input: impl FnMut(usize, &mut Player, u64)) {
        for _ in 0..(seconds * fps).round() as usize {
            self.frame(1.0 / fps, &mut input);
        }
    }

    /// How far each player's game had its body from where the server had it, at the same steps
    /// (the worst of the last `steps` the server took).
    fn off(&self, k: usize, steps: u64) -> f64 {
        let id = self.seats[k].online.you.unwrap();
        let newest = self.host.game.step;
        let mut worst = 0.0f64;
        for &(who, step, at) in &self.truth {
            if who != id || step + steps < newest {
                continue;
            }
            if let Some(&(_, mine)) = self.seats[k].path.iter().rev().find(|p| p.0 == step) {
                worst = worst.max(mine.distance(at));
            }
        }
        worst
    }
}

/// A walk round the site: ahead, a turn, a jump, a dash to the side, a crouch.
fn walk(me: &mut Player, n: u64) {
    let mut i = Input::default();
    let n = n % 600;
    if (30..400).contains(&n) {
        i.forward = 1.0;
    }
    if (200..260).contains(&n) {
        i.side = -1.0;
        i.run = true;
    }
    if n == 120 || n == 330 {
        i.jump = true;
    }
    if (280..320).contains(&n) {
        i.crouch = true;
    }
    if n % 7 == 0 {
        me.pilot.look(5.0, 0.0);
    }
    me.input = i;
}

#[test]
fn walking_on_a_clean_network_the_body_is_never_put_right() {
    let cond = Conditions { delay: 0.04, ..Conditions::default() };
    let mut t = Table::new(2, 1, cond);
    t.run(12.0, 60.0, |k, me, n| {
        if k == 0 {
            walk(me, n);
        }
    });
    let fixes: Vec<u64> = t.seats.iter().map(|s| s.online.stats.corrections).collect();
    assert_eq!(fixes, vec![0, 0], "corrections on a clean network ({:?}; server {:?})", t.seats.iter().map(|s| s.online.stats).collect::<Vec<_>>(), t.host.stats);
    assert!(t.off(0, 240) < 1e-3, "the walker's own game had it {:.4} m off the server's", t.off(0, 240));
    // and it walked
    let me = &t.seats[0].me.pilot;
    let start = t.seats[0].path[0].1;
    assert!(me.position.distance(start) > 5.0);
    assert_eq!(t.host.stats.guessed, 0, "{:?}", t.host.stats);
}

#[test]
fn on_a_bad_network_corrections_are_rare_and_the_body_ends_where_the_server_has_it() {
    for (delay, jitter, loss, most) in [(0.04, 0.01, 0.03, 1), (0.15, 0.03, 0.10, 2)] {
        let cond = Conditions { delay, jitter, loss, ..Conditions::default() };
        let mut t = Table::new(1, 7, cond);
        t.run(15.0, 144.0, |_, me, n| walk(me, n));
        let st = t.seats[0].online.stats;
        assert!(st.corrections <= most, "{delay} s, {loss}: {st:?} (server {:?})", t.host.stats);
        // a few steps more standing, to let the last word come
        t.run(1.0, 144.0, |_, me, _| me.input = Input::default());
        assert!(t.off(0, 30) < 1e-3, "{delay} s: {:.4} m off", t.off(0, 30));
    }
}

#[test]
fn the_clock_settles_a_few_steps_early_whatever_the_network() {
    for delay in [0.01, 0.05, 0.15] {
        let cond = Conditions { delay, jitter: 0.03, ..Conditions::default() };
        let mut t = Table::new(1, 3, cond);
        t.run(3.0, 60.0, |_, _, _| {});
        let lead = t.seats[0].game.step as i64 - t.host.game.step as i64;
        let rtt_steps = (2.0 * delay / STEP) as i64;
        assert!(t.host.stats.guessed < 10, "{delay}: {:?}", t.host.stats);
        assert!(lead >= rtt_steps / 2 && lead <= rtt_steps / 2 + 12, "{delay} s: our game {lead} steps ahead of the server's");
    }
}

#[test]
fn a_push_only_the_server_knows_of_is_put_right_without_a_jump() {
    let cond = Conditions { delay: 0.04, ..Conditions::default() };
    let mut t = Table::new(1, 5, cond);
    t.run(1.0, 60.0, |_, _, _| {});
    // (the server alone moves the body a metre aside: a blast nobody predicted)
    let id = t.seats[0].online.you.unwrap();
    let bodies = t.host.game.bodies.clone();
    let b = bodies.get(t.host.game.site.body);
    let p = t.host.player_mut(id).unwrap();
    let (feet, up) = p.pilot.feet();
    let side = up.any_orthonormal_vector();
    let to = b.above_ground(b.up(feet + side), 0.0);
    p.pilot.put(to, b.up(to));
    let mut worst = 0.0f64;
    let mut last: Option<DVec3> = None;
    for _ in 0..90 {
        t.frame(1.0 / 60.0, |_, _, _| {});
        let s = &mut t.seats[0];
        s.game.present(s.online.alpha(), &mut [&mut s.me]);
        let eye = s.me.pilot.position;
        s.game.restore(&mut [&mut s.me]);
        if let Some(l) = last {
            worst = worst.max(eye.distance(l));
        }
        last = Some(eye);
    }
    assert!(t.seats[0].online.stats.corrections >= 1);
    assert!(t.off(0, 30) < 1e-3, "{:.4} m off", t.off(0, 30));
    assert!(worst < 0.2, "the eye jumped {worst:.3} m in a frame");
}

#[test]
fn put_in_the_air_aboard_a_ship_whose_gravity_comes_on_each_push_is_put_right_once() {
    // the Alcotán just made far from every body: its own gravity comes on over two seconds. One
    // aboard is put up in the air by what only the server knows, twice while it comes on: put
    // right each time and no more as they fall — their steps are done again against the ship as
    // it was at each (its gravity then, not as far on as it is now)
    let cond = Conditions { delay: 0.08, jitter: 0.005, ..Conditions::default() };
    let mut t = Table::new(1, 53, cond);
    let far = DVec3::new(2.0e6, 3.0e6, -1.0e6);
    t.host.game.watchers.push(far);
    let id = t.host.game.ships.spawn_free(&mut t.host.game.builds, "alcotan", far, Quat::IDENTITY).unwrap();
    t.run(0.2, 60.0, |_, _, _| {});
    let exit = {
        let g = &t.host.game;
        glam::Vec3::from_array(g.ships.list[g.ships.by_structure(id).unwrap()].kind.seats[0].def.salida)
    };
    let you = t.seats[0].online.you.unwrap();
    let put = |t: &mut Table, at: glam::Vec3| {
        let (game, p) = t.host.game_and_player(you).unwrap();
        p.pilot.put_on(&game.builds.set, id, at);
    };
    put(&mut t, exit);
    t.run(0.4, 60.0, |_, _, _| {});
    let on = |t: &Table| t.host.game.builds.set.get(id).unwrap().gravity.on;
    let first = t.seats[0].online.stats.corrections;
    let was = on(&t);
    for k in 0..2 {
        put(&mut t, exit + glam::Vec3::new(0.3 * (k + 1) as f32, 0.6, 0.0));
        t.run(0.7, 60.0, |_, _, _| {});
    }
    let fixes = t.seats[0].online.stats.corrections - first;
    println!("aboard as its gravity comes on ({was:.2} → {:.2}): {fixes} corrections for 2 pushes", on(&t));
    assert!(was < 0.5, "the gravity was not coming on: {was} → {}", on(&t));
    assert_eq!(fixes, 2, "put right {fixes} times for 2 pushes ({:?})", t.seats[0].online.stats);
}

#[test]
fn standing_in_the_hold_of_a_ship_still_bouncing_on_its_legs_is_put_right_only_while_it_bounces() {
    // a Cachalote put down by the server a moment ago, still settling on its legs, one put in its
    // hold at once: put right for being put there, a few times as the ship bounces under them
    // (their game's copy of it bounces a little otherwise: `PENDIENTES.md`), and never once it
    // has settled
    let cond = Conditions { delay: 0.04, jitter: 0.005, loss: 0.01, ..Conditions::default() };
    let mut t = Table::new(1, 61, cond);
    t.run(0.3, 60.0, |_, _, _| {});
    let (pos, rot) = {
        let g = &t.host.game;
        let b = g.bodies.get(g.site.body);
        let at = b.above_ground(g.site.at(60.0, 40.0), 0.3);
        (at, glam::Quat::IDENTITY)
    };
    let id = t.host.game.ships.spawn_free(&mut t.host.game.builds, "cachalote", pos, rot).unwrap();
    t.run(0.1, 60.0, |_, _, _| {});
    let you = t.seats[0].online.you.unwrap();
    {
        let (game, p) = t.host.game_and_player(you).unwrap();
        p.pilot.put_on(&game.builds.set, id, glam::Vec3::new(0.0, 0.05, -13.0));
    }
    for _ in 0..40 {
        t.run(0.25, 60.0, |_, _, _| {});
        if t.host.game.builds.set.get(id).unwrap().resting {
            break;
        }
    }
    assert!(t.host.game.builds.set.get(id).unwrap().resting, "the ship never settled");
    let settling = t.seats[0].online.stats.corrections;
    t.run(3.0, 60.0, |_, _, _| {});
    let after = t.seats[0].online.stats.corrections - settling;
    println!("in the hold of a Cachalote settling on its legs: {settling} corrections as it settled, {after} after");
    assert!(settling <= 6, "put right {settling} times as it settled");
    assert_eq!(after, 0, "put right {after} times once it had settled");
}

#[test]
fn every_weapon_of_the_data_fired_from_beside_a_ship_at_2_kms_ends_alike_in_every_game() {
    // far from every body, an Alcotán and a Cachalote side by side at 2 km/s, 150 m apart; one
    // floats beside the Alcotán, another is in the Cachalote's hold. The first lets fly every
    // weapon there is in the data (shots, missiles, guided missiles, decoys) one at a time at the
    // Cachalote, going as it goes: each starts once in each game (in the shooter's, as its own),
    // and when it is all over each structure there is the same in the three games, with the
    // server's name for each piece. The shooter is never put right; the one in the hold, no more
    // than once a weapon (the blows reach their game a moment after the server's: the ship jolts
    // under them there later)
    use lunar_play::blasts::{Launch, RAIL, What};
    let cond = Conditions { delay: 0.04, jitter: 0.005, loss: 0.01, ..Conditions::default() };
    let mut t = Table::new(2, 47, cond);
    (t.host.config.rule.near, t.host.config.rule.most) = (1.0e8, 1.0e8);
    let far = DVec3::new(2.0e6, 3.0e6, -1.0e6);
    let vel = DVec3::new(2000.0, 0.0, 0.0);
    t.host.game.watchers.push(far);
    let (alcotan, cachalote) = {
        let g = &mut t.host.game;
        let a = g.ships.spawn_free(&mut g.builds, "alcotan", far, Quat::IDENTITY).unwrap();
        let c = g.ships.spawn_free(&mut g.builds, "cachalote", far + DVec3::Z * 150.0, Quat::IDENTITY).unwrap();
        for id in [a, c] {
            let k = g.builds.set.index_of(id).unwrap();
            g.builds.set.list[k].vel = vel;
        }
        (a, c)
    };
    t.run(0.2, 60.0, |_, _, _| {});
    let (shooter, watcher) = (t.seats[0].online.you.unwrap(), t.seats[1].online.you.unwrap());
    {
        let (game, p) = t.host.game_and_player(shooter).unwrap();
        let s = game.builds.set.get(alcotan).unwrap();
        let at = s.to_world(s.center) + DVec3::Z * 40.0;
        p.pilot.put(at, DVec3::Y);
        p.pilot.still_to(&game.builds.set, alcotan);
        let (game, p) = t.host.game_and_player(watcher).unwrap();
        p.pilot.put_on(&game.builds.set, cachalote, glam::Vec3::new(0.0, 0.05, -13.0));
    }
    t.run(2.0, 60.0, |_, _, _| {});
    let fixed = [t.seats[0].online.stats.corrections, t.seats[1].online.stats.corrections];
    let every: Vec<(String, What)> = t.host.game.blasts.every().into_iter().filter(|(_, w)| !matches!(w, What::Boom(_))).collect();
    let mut fired = Vec::new();
    for (name, what) in &every {
        let l = {
            let s = &mut t.seats[0];
            let target = s.game.builds.set.get(cachalote).unwrap();
            let eye = s.me.pilot.position;
            let dir = (target.to_world(target.center) - eye).normalize();
            let from = eye + dir * 1.0;
            let vel = s.me.pilot.motion_in(&s.game.builds.set).velocity_at(from);
            match what {
                What::Shot(_) => s.game.blasts.shot(name, from, dir, vel, None).unwrap(),
                What::Missile(_) => Launch { what: *what, from, dir, speed: 120.0, vel, target: None, by: None },
                What::Guided(_) => Launch { what: *what, from, dir, speed: RAIL, vel, target: Some(cachalote), by: None },
                _ => Launch { what: *what, from, dir, speed: 15.0, vel, target: None, by: None },
            }
        };
        {
            let s = &mut t.seats[0];
            let bodies = s.game.bodies.clone();
            assert!(s.game.blasts.launch(l, &bodies, &mut s.game.builds), "{name} did not fire");
        }
        // (as long as it takes to get there, and a moment: what misses flies on in the void)
        let secs = t.host.game.blasts.shot_speed(name).map_or(6.0, |(v, _)| (150.0 / f64::from(v.max(1.0)) + 1.5).clamp(1.5, 9.0));
        t.run(secs, 60.0, |_, _, _| {});
        fired.push(name.clone());
    }
    t.run(2.0, 60.0, |_, _, _| {});
    // (seen once by each: the one who let it fly, as theirs; the other, as the server let it fly)
    let started = |k: usize| t.seats[k].game.blasts.started;
    println!("every weapon from beside a ship at 2 km/s: {}; started {} in the server, {} and {} in the games; struck {}", fired.join(", "), t.host.game.blasts.started, started(0), started(1), t.host.game.struck);
    assert_eq!((started(0), started(1)), (t.host.game.blasts.started, t.host.game.blasts.started), "what was let fly is not seen once by each");
    assert!(t.host.game.struck > 0, "nothing struck the Cachalote");
    let digest = |g: &Game, id: u64| {
        let s = g.builds.set.get(id)?;
        let sh = g.ships.by_structure(id).map(|n| &g.ships.list[n]);
        Some(lunar_ship::sync::Digest::of(sh, s).hash)
    };
    let site = t.host.game.builds.set.get(cachalote).map_or(far, |s| s.pos);
    let near: Vec<u64> = t.host.game.builds.set.list.iter().filter(|s| s.pos.distance(site) < 1000.0).map(|s| s.id).collect();
    for (k, s) in t.seats.iter().enumerate() {
        for &id in &near {
            assert_eq!(digest(&s.game, id), digest(&t.host.game, id), "player {k}: structure {id} is not the server's ({:?})", s.online.stats);
            assert_eq!(s.game.builds.set.get(id).map(|x| x.lineage), t.host.game.builds.set.get(id).map(|x| x.lineage), "player {k}: structure {id}");
        }
        for st in s.game.builds.set.list.iter().filter(|x| x.pos.distance(site) < 1000.0) {
            assert!(near.contains(&st.id), "player {k} has structure {} the server does not ({:?})", st.id, s.online.stats);
        }
    }
    let fixes = [t.seats[0].online.stats.corrections - fixed[0], t.seats[1].online.stats.corrections - fixed[1]];
    println!("put right meanwhile: the shooter {}, the one in the hold {}", fixes[0], fixes[1]);
    assert_eq!(fixes[0], 0, "the shooter was put right");
    assert!(fixes[1] <= every.len() as u64, "the one in the hold was put right {} times", fixes[1]);
}

#[test]
fn the_server_keeps_where_each_body_was_a_moment_ago() {
    // two walking: the server has where each body was at each of the last 300 ms, as it had them
    // then, and nothing older (what a shot at a person will be judged against)
    let mut t = Table::new(2, 19, Conditions { delay: 0.03, ..Conditions::default() });
    t.run(2.0, 60.0, |k, me, n| walk(me, n + k as u64 * 40));
    let now = t.host.game.step;
    for back in 0..lunar_play::host::REWIND as u64 {
        let step = now - back;
        let bodies = t.host.bodies_at(step).unwrap_or_else(|| panic!("nothing kept of {back} steps back"));
        assert_eq!(bodies.len(), 2);
        for b in bodies {
            let truth = t.truth.iter().find(|x| x.0 == b.id && x.1 == step).map(|x| x.2).unwrap();
            assert!(b.eye.distance(truth) < 1e-9, "{back} steps back, {} was {:.3} m off", b.id, b.eye.distance(truth));
        }
    }
    assert!(t.host.bodies_at(now - lunar_play::host::REWIND as u64).is_none(), "kept for longer than it says");
}

#[test]
fn the_others_are_seen_where_they_are() {
    let cond = Conditions { delay: 0.05, jitter: 0.01, loss: 0.02, ..Conditions::default() };
    let mut t = Table::new(2, 11, cond);
    t.run(8.0, 60.0, |k, me, n| {
        if k == 1 {
            walk(me, n);
        }
    });
    let walker = t.seats[1].online.you.unwrap();
    let seen = t.seats[0].online.others.iter().find(|o| o.0 == walker).expect("the walker is not among the others");
    let at = t.host.player(walker).unwrap().pilot.position;
    // (as of the snapshot's step: a few steps behind)
    let behind = (t.host.game.step - t.seats[0].online.others_step) as f64 * STEP;
    assert!(seen.1.pos.distance(at) < 0.5 + 6.0 * behind, "{:.3} m off ({behind:.3} s behind)", seen.1.pos.distance(at));
}

#[test]
fn what_flies_is_where_the_server_has_it_at_any_speed() {
    // a ship far from everything at 0, 300 and 7 800 m/s, made by the server: each game has it
    // where the server does at the same step, whatever it took to get there
    for speed in [0.0, 300.0, 7800.0] {
        let cond = Conditions { delay: 0.05, jitter: 0.01, loss: 0.02, ..Conditions::default() };
        let mut t = Table::new(1, 13, cond);
        // (known however far it goes in the test)
        t.host.config.rule.near = 100_000.0;
        let b = t.host.game.bodies.get(t.host.game.site.body);
        let far = b.above_ground(t.host.game.site.at(600.0, 0.0), 800.0);
        let id = t.host.game.ships.spawn_free(&mut t.host.game.builds, "alcotan", far, Quat::IDENTITY).unwrap();
        let k = t.host.game.builds.set.index_of(id).unwrap();
        t.host.game.builds.set.list[k].vel = DVec3::new(0.0, 0.0, speed);
        t.run(4.0, 60.0, |_, _, _| {});
        let s = &t.seats[0];
        let at = |set: &lunar_core::structure::set::Structures| {
            let x = set.get(id).expect("the ship never came to be known");
            coasted(x, t.host.game.bodies.field(x.pos).pull, set.now - x.clock)
        };
        let (mine, mine_vel, _) = at(&s.game.builds.set);
        let (theirs, ..) = at(&t.host.game.builds.set);
        // (ours is ahead by some steps: carried back to the server's)
        let ahead = (s.game.step - t.host.game.step) as f64 * STEP;
        let back = mine - mine_vel * ahead;
        assert!(back.distance(theirs) < 0.05 + 1e-4 * speed, "at {speed} m/s ours is {:.3} m off ({:?})", back.distance(theirs), s.online.stats);
    }
}

#[test]
fn standing_in_a_ship_at_any_speed_the_body_is_never_put_right() {
    // a ship far from every body (nothing pulls) at 0, 300, 1 600 and 7 800 m/s, the player put
    // aboard it by the server (one correction: their game did not know), then standing in it
    // while it goes, seated at its controls, and up again: no correction more, and their game has
    // the body where the server has it in the ship
    for speed in [0.0, 300.0, 1600.0, 7800.0] {
        let cond = Conditions { delay: 0.04, jitter: 0.005, ..Conditions::default() };
        let mut t = Table::new(1, 17, cond);
        let far = DVec3::new(2.0e6, 3.0e6, -1.0e6);
        // (watched from there from the start: a structure nobody watches is stepped now and then,
        // and one put aboard it would be left behind when it catches up)
        t.host.game.watchers.push(far);
        let id = t.host.game.ships.spawn_free(&mut t.host.game.builds, "alcotan", far, Quat::IDENTITY).unwrap();
        let k = t.host.game.builds.set.index_of(id).unwrap();
        t.host.game.builds.set.list[k].vel = DVec3::new(speed, 0.0, 0.0);
        t.run(0.2, 60.0, |_, _, _| {});
        let you = t.seats[0].online.you.unwrap();
        let n = t.host.game.ships.by_structure(id).unwrap();
        let exit = glam::Vec3::from_array(t.host.game.ships.list[n].kind.seats[0].def.salida);
        let (game, p) = t.host.game_and_player(you).unwrap();
        p.pilot.put_on(&game.builds.set, id, exit);
        t.run(2.0, 60.0, |_, _, _| {});
        let first = t.seats[0].online.stats.corrections;
        assert!(t.seats[0].me.pilot.ride.is_some_and(|r| r.id == id), "at {speed} m/s our game does not have us aboard ({:?})", t.seats[0].online.stats);
        t.run(4.0, 144.0, |_, _, _| {});
        assert_eq!(t.seats[0].online.stats.corrections, first, "at {speed} m/s, standing: {:?} {:?}", t.seats[0].online.stats, t.host.stats);
        assert!(t.off_aboard(0, 120) < 1e-3, "at {speed} m/s our game has us {:.4} m off in the ship", t.off_aboard(0, 120));
    }
}

#[test]
fn what_a_player_fires_the_server_decides_and_every_game_ends_alike() {
    // a charge of 100 kg from the player's hands at a ship standing at the site, and a second player
    // watching: the server lets it fly and decides what it does; each game does the same strikes
    // with the same dice, so the ship ends the same in all three, and each piece that came off is
    // in each game with the server's name for it
    let cond = Conditions { delay: 0.04, jitter: 0.01, loss: 0.02, ..Conditions::default() };
    let mut t = Table::new(2, 23, cond);
    t.run(1.0, 60.0, |_, _, _| {});
    let target = {
        let s = &t.seats[0];
        let eye = s.me.pilot.position;
        let set = &s.game.builds.set;
        s.game.ships.list.iter().filter_map(|sh| set.get(sh.structure)).min_by(|a, b| a.pos.distance(eye).total_cmp(&b.pos.distance(eye))).map(|x| x.id).unwrap()
    };
    let before = t.host.game.builds.set.list.len();
    {
        let s = &mut t.seats[0];
        let set = &s.game.builds.set;
        let st = set.get(target).unwrap();
        let eye = s.me.pilot.position;
        let dir = (st.to_world(st.center) - eye).normalize();
        let bodies = s.game.bodies.clone();
        assert!(s.game.blasts.fire_from("personalizado", eye + dir, dir, DVec3::ZERO, None, &bodies, &mut s.game.builds));
    }
    t.run(4.0, 60.0, |_, _, _| {});
    assert!(t.host.game.struck > 0, "the server struck nothing");
    // (seen once by each: the one who fired it, as theirs; the other, as the server let it fly)
    let started = |k: usize| t.seats[k].game.blasts.started;
    assert_eq!((started(0), started(1)), (t.host.game.blasts.started, t.host.game.blasts.started), "what was let fly is seen once by each");
    let made = t.host.game.builds.set.list.len() - before;
    assert!(made > 0, "nothing came off");
    let site = t.host.game.builds.set.get(target).map_or(DVec3::ZERO, |s| s.pos);
    let site = if site == DVec3::ZERO { t.seats[0].me.pilot.position } else { site };
    // (what each structure near it is: what is there and works of it, each joint, what its
    // ship's systems keep that must be the same)
    let digest = |g: &Game, id: u64| {
        let s = g.builds.set.get(id)?;
        let sh = g.ships.by_structure(id).map(|n| &g.ships.list[n]);
        Some(lunar_ship::sync::Digest::of(sh, s).hash)
    };
    let near: Vec<u64> = t.host.game.builds.set.list.iter().filter(|s| s.pos.distance(site) < 300.0).map(|s| s.id).collect();
    for (k, s) in t.seats.iter().enumerate() {
        for &id in &near {
            assert_eq!(digest(&s.game, id), digest(&t.host.game, id), "player {k}: structure {id} is not the server's ({:?})", s.online.stats);
            assert_eq!(s.game.builds.set.get(id).map(|x| x.lineage), t.host.game.builds.set.get(id).map(|x| x.lineage));
        }
        // (and nothing near it the server does not have, ours unnamed least of all)
        for st in s.game.builds.set.list.iter().filter(|x| x.pos.distance(site) < 300.0) {
            assert!(near.contains(&st.id), "player {k} has structure {} the server does not ({:?})", st.id, s.online.stats);
        }
    }
    assert!(t.seats[0].online.stats.strikes > 0 && t.seats[1].online.stats.strikes > 0);
    // one who comes now sees each structure as it is and every piece where it lies, and the ground
    // as the blast left it
    let late = t.join("tarde");
    t.run(2.0, 60.0, |_, _, _| {});
    let s = &t.seats[late];
    for &id in &near {
        assert_eq!(digest(&s.game, id), digest(&t.host.game, id), "the one who came late has structure {id} otherwise ({:?})", s.online.stats);
    }
    let ground = |g: &Game| g.bodies.get(g.site.body).deform().craters().to_vec();
    assert!(!ground(&t.host.game).is_empty(), "the blast dug nothing");
    for (k, s) in t.seats.iter().enumerate() {
        assert_eq!(ground(&s.game), ground(&t.host.game), "player {k} has the ground otherwise");
    }
    let s = &t.seats[late];
    let mut seen = 0;
    for st in &t.host.game.builds.set.list {
        if st.id >= t.host.game.builds.scenario_end && st.pos.distance(site) < 300.0 {
            let mine = s.game.builds.set.get(st.id).unwrap_or_else(|| panic!("the one who came late has no piece {}", st.id));
            // (where its middle is: what is at rest, where it rests; what still rolls, within what
            // it rolls in the moment our game is ahead of the server's, and some)
            let ahead = (s.game.step - t.host.game.step) as f64 * STEP;
            let (a, b) = (mine.to_world(mine.center), st.to_world(st.center));
            let most = if st.resting { 0.002 } else { 0.05 + (st.vel.length() + f64::from(st.spin.length() * st.radius)) * (ahead + 0.3) };
            assert!(a.distance(b) < most, "piece {} is {:.3} m off ({most:.3} at most): server rest {} vel {:.2} spin {:.2}; ours rest {} ({:?})", st.id, a.distance(b), st.resting, st.vel.length(), st.spin.length(), mine.resting, s.online.stats);
            seen += 1;
        }
    }
    assert!(seen > 0);
}

#[test]
fn piloting_a_ship_it_goes_where_the_server_has_it_and_the_pilot_is_not_put_right() {
    // a ship far from every body, the player sat at its controls by their own game (the server
    // is told), its keys held: the ship goes as the keys say in the pilot's game at once and in
    // the server's the same, and a second player sees it there; nobody is put right
    let cond = Conditions { delay: 0.04, jitter: 0.005, loss: 0.01, ..Conditions::default() };
    let mut t = Table::new(2, 29, cond);
    // (the one who watches is at the site, thousands of kilometres off: known however far)
    (t.host.config.rule.near, t.host.config.rule.most) = (1.0e8, 1.0e8);
    let far = DVec3::new(2.0e6, 3.0e6, -1.0e6);
    t.host.game.watchers.push(far);
    let id = t.host.game.ships.spawn_free(&mut t.host.game.builds, "alcotan", far, Quat::IDENTITY).unwrap();
    t.run(0.2, 60.0, |_, _, _| {});
    let you = t.seats[0].online.you.unwrap();
    let n = t.host.game.ships.by_structure(id).unwrap();
    let exit = glam::Vec3::from_array(t.host.game.ships.list[n].kind.seats[0].def.salida);
    let (game, p) = t.host.game_and_player(you).unwrap();
    p.pilot.put_on(&game.builds.set, id, exit);
    t.run(2.0, 60.0, |_, _, _| {});
    // sat down by our own game, as the use key does
    {
        let s = &mut t.seats[0];
        let ships = &s.game.ships;
        lunar_play::seats::sit(&mut s.me.pilot, ships, &s.game.builds.set, id, 0, |_, _| false).unwrap();
    }
    t.run(0.5, 60.0, |_, _, _| {});
    let first = t.seats[0].online.stats.corrections;
    assert!(t.host.player(you).unwrap().pilot.seat.is_some_and(|s| s.structure == id), "the server did not sit us");
    // which of the seat's keys moves it ahead
    let k = {
        let g = &t.seats[0].game;
        let sh = &g.ships.list[g.ships.by_structure(id).unwrap()];
        let (keys, _) = lunar_ship::seat_keys::keys(sh, 0);
        keys.iter().position(|k| k.key == "avanzar").expect("a key to go ahead")
    };
    let start = t.host.game.builds.set.get(id).unwrap().pos;
    let mut track: Vec<(u64, DVec3)> = Vec::new();
    for frame in 0..(6 * 60) {
        t.seats[0].online.keys = if frame < 180 { 1 << k } else { 0 };
        t.frame(1.0 / 60.0, |_, _, _| {});
        let s = &t.seats[0];
        track.push((s.game.step, s.game.builds.set.get(id).unwrap().pos));
        let srv = (t.host.game.step, t.host.game.builds.set.get(id).unwrap().pos);
        if frame > 30
            && let Some(&(_, mine)) = track.iter().rev().find(|x| x.0 == srv.0)
        {
            assert!(mine.distance(srv.1) < 0.05, "frame {frame}: the pilot's game has the ship {:.3} m off the server's ({:?})", mine.distance(srv.1), s.online.stats);
        }
    }
    let srv = t.host.game.builds.set.get(id).unwrap();
    // (the manoeuvring jets alone: a few tenths of a metre a second in three seconds)
    assert!(srv.pos.distance(start) > 0.3 && srv.vel.length() > 0.08, "the ship did not go: {:.2} m, {:.2} m/s", srv.pos.distance(start), srv.vel.length());
    assert_eq!(t.seats[0].online.stats.corrections, first, "the pilot was put right ({:?})", t.seats[0].online.stats);
    // the one who watches has it where it is
    let w = &t.seats[1];
    let theirs = w.game.builds.set.get(id).expect("the watcher does not know the ship");
    let ahead = (w.game.step - t.host.game.step) as f64 * STEP;
    assert!((theirs.pos - theirs.vel * ahead).distance(srv.pos) < 0.05, "the watcher has the ship {:.3} m off", (theirs.pos - theirs.vel * ahead).distance(srv.pos));
}

#[test]
fn a_crate_let_go_taken_and_dropped_ends_where_the_server_has_it() {
    // in the hold of the Cachalote: a clamp let go by a hand, one of the crates it held taken in
    // the bare hands, carried a moment and dropped; another player watches. Where it ends to rest
    // is the same in every game, exactly, and nobody's hands were put right
    let cond = Conditions { delay: 0.04, jitter: 0.005, loss: 0.01, ..Conditions::default() };
    let mut t = Table::new(2, 31, cond);
    // (the ship settled on its legs first, as it is when anyone comes to it)
    let cachalote = t.host.game.ships.list.iter().find(|sh| sh.kind.id == "cachalote").expect("the Cachalote").structure;
    for _ in 0..20 {
        t.run(0.5, 60.0, |_, _, _| {});
        if t.host.game.builds.set.get(cachalote).is_some_and(|s| s.resting) {
            break;
        }
    }
    // (the cargo is part of the ship until its clamp lets it go: then it is a body of its own)
    let (ship, clamp, zone) = {
        let g = &t.host.game;
        let n = g.ships.list.iter().position(|sh| sh.kind.id == "cachalote").expect("the Cachalote");
        let sh = &g.ships.list[n];
        let c = sh.kind.clamps.iter().position(|c| c.id == "anclaje_a4").expect("the clamp of the water drums of the fourth row");
        let s = g.builds.set.get(sh.structure).unwrap();
        let at = sh.kind.clamps[c].zone.map_or(glam::Vec3::ZERO, |z| z.centre);
        (sh.structure, c, s.to_world(at))
    };
    // (stood in the hold where the middle of the fifth row would be: it carries nothing)
    let you = t.seats[0].online.you.unwrap();
    let at = glam::Vec3::new(0.0, 0.05, -13.0);
    let (game, p) = t.host.game_and_player(you).unwrap();
    p.pilot.put_on(&game.builds.set, ship, at);
    t.run(2.0, 60.0, |_, _, _| {});
    let first = t.seats[0].online.stats.corrections;
    // (put in by the server, their game does not know: once, and no more after it — what their
    // game said of the steps ahead before it knew is not held against it)
    assert_eq!(first, 1, "put in the hold, put right {first} times");
    let made_from = t.host.game.builds.set.next_free();
    // the clamp let go, as a hand on it does (done here and said)
    {
        let s = &mut t.seats[0];
        lunar_play::controls::act(&mut s.game.ships, ship, lunar_play::controls::Act::Clamp(clamp as u16, true));
        s.online.act(s.game.step, &lunar_play::net::Act::Hand { ship, act: lunar_play::controls::Act::Clamp(clamp as u16, true) });
    }
    t.run(1.5, 60.0, |_, _, _| {});
    let cargo = t.host.game.builds.set.list.iter().filter(|s| s.id >= made_from && t.host.game.ships.by_structure(s.id).is_none()).min_by(|a, b| a.pos.distance(zone).total_cmp(&b.pos.distance(zone))).map(|s| s.id).expect("nothing came loose");
    for (k, g) in std::iter::once(&t.host.game).chain(t.seats.iter().map(|s| &s.game)).enumerate() {
        assert!(g.builds.set.get(cargo).is_some_and(|c| c.held.is_none()), "game {k}: the crate is not there loose");
    }
    // taken in the hands: looked at and grabbed, as a click does (what the look meets first of
    // what was let go)
    let cargo = {
        let s = &mut t.seats[0];
        let c = s.game.builds.set.get(cargo).unwrap();
        let target = c.to_world(c.center);
        s.me.pilot.look_at(target);
        let view = s.me.pilot.view_aboard(&s.game.builds.set).unwrap_or_else(|| s.me.pilot.view());
        let r = s.me.hands.reach(&s.game.builds.set, &s.game.ships, &view, Some(ship));
        assert!(r.as_ref().is_some_and(|r| r.0.no.is_none()), "nothing to take there: {:?} (eye {:.2} m from the crate)", r.map(|r| r.0), view.eye.distance(target));
        s.me.hands.grab(&s.game.builds.set, &s.game.ships, &view, Some(ship)).expect("it can be taken");
        s.me.hands.holding().expect("taken")
    };
    // (carried a moment, the look turned by the mouse as it goes)
    t.run(1.0, 60.0, |k, me, _| {
        if k == 0 {
            me.pilot.look(2.0, 0.0);
        }
    });
    assert_eq!(t.host.player(you).unwrap().hands.holding(), Some(cargo), "the server's hands do not hold it ({:?}, told {:?}; server {:?})", t.seats[0].online.stats, t.seats[0].online.said, t.host.stats);
    {
        let s = &mut t.seats[0];
        s.me.hands.release(&mut s.game.builds);
    }
    t.run(5.0, 60.0, |_, _, _| {});
    let truth = t.host.game.builds.set.get(cargo).unwrap();
    assert!(truth.resting && t.host.player(you).unwrap().hands.holding().is_none(), "the crate has not come to rest, or is still held");
    for (k, s) in t.seats.iter().enumerate() {
        let mine = s.game.builds.set.get(cargo).unwrap();
        assert!(mine.resting && mine.pos == truth.pos && mine.rot == truth.rot, "player {k} has the crate {:.4} m off (resting {})", mine.pos.distance(truth.pos), mine.resting);
    }
    assert_eq!(t.seats[0].online.stats.corrections, first, "the one who carried it was put right ({:?})", t.seats[0].online.stats);
}

#[test]
fn floating_with_the_pack_the_mouse_turns_the_body_and_nobody_puts_it_right() {
    // far from every body, the pack on, nothing weighs: the mouse turns the whole body (every
    // way, `Pilot::look`), the jets push where it looks. The server is told how the body is
    // turned with each step, so it goes the same there: never put right
    let cond = Conditions { delay: 0.05, jitter: 0.01, loss: 0.02, ..Conditions::default() };
    let mut t = Table::new(1, 37, cond);
    let far = DVec3::new(-2.0e6, 2.5e6, 1.5e6);
    let you = t.seats[0].online.you.unwrap();
    t.host.game.watchers.push(far);
    let (game, p) = t.host.game_and_player(you).unwrap();
    let _ = game;
    p.pilot.put(far, DVec3::Y);
    p.pilot.pack_on = true;
    t.run(2.0, 60.0, |_, me, _| me.pilot.pack_on = true);
    let first = t.seats[0].online.stats.corrections;
    assert!(t.seats[0].me.pilot.floating(), "not floating");
    t.run(6.0, 144.0, |_, me, n| {
        me.pilot.pack_on = true;
        me.pilot.look(if n % 120 < 60 { 3.0 } else { -2.0 }, if n % 90 < 30 { 1.5 } else { -0.5 });
        me.input = Input { forward: if n % 200 < 120 { 1.0 } else { 0.0 }, side: if n % 300 < 50 { 1.0 } else { 0.0 }, ..Input::default() };
    });
    assert!(t.seats[0].me.pilot.floating());
    assert_eq!(t.seats[0].online.stats.corrections, first, "floating, put right ({:?})", t.seats[0].online.stats);
    assert!(t.off(0, 120) < 1e-3, "{:.4} m off", t.off(0, 120));
    assert!(t.seats[0].me.pilot.position.distance(far) > 2.0, "the jets took us nowhere");
}

/// A new connection for player `k` coming back with `key` (their game and body kept as they were).
fn come_back(t: &mut Table, k: usize, key: u64) {
    let end = t.net.endpoint();
    t.seats[k].at = end.addr();
    let client = lunar_net::Client::with_transport(Box::new(end), t.link.addr(), &format!("jugador{k}"), BUILD, defs().fingerprint);
    t.seats[k].online = Online::back(client, defs().scenario.player, key);
    (t.seats[k].path.clear(), t.seats[k].said.clear());
    for _ in 0..600 {
        t.frame(1.0 / 60.0, |_, _, _| {});
        if t.seats[k].online.live() {
            break;
        }
    }
    assert!(t.seats[k].online.live(), "never came back: {:?}", t.seats[k].online.status());
}

#[test]
fn who_is_cut_off_comes_back_to_their_body_where_it_waited() {
    let mut t = Table::new(2, 47, Conditions::default());
    t.run(4.0, 60.0, |k, me, n| {
        if k == 0 {
            walk(me, n)
        }
    });
    let key = t.seats[0].online.key.expect("a key to come back with");
    let id = t.seats[0].online.you.unwrap();
    let at_cut = t.host.player(id).unwrap().pilot.position;
    // (20 s with no network: the server lets the connection go after its wait; the body waits)
    t.net.cut(t.seats[0].at, true);
    t.run(20.0, 60.0, |_, me, _| me.input = Input::default());
    assert!(t.host.player(id).is_none(), "the connection is gone");
    let waiting: Vec<u32> = t.host.waiting().collect();
    assert_eq!(waiting.len(), 1, "one body waits");
    let waited = t.host.player(waiting[0]).unwrap().pilot.position;
    // (it went on with what it last asked only a moment: then it stood)
    assert!(waited.distance(at_cut) < 3.0, "it walked off on its own: {:.2} m", waited.distance(at_cut));
    // (the other sees it standing where it is)
    let seen = t.seats[1].online.others.iter().find(|(o, _)| *o == waiting[0]).map(|(_, st)| st.pos);
    assert!(seen.is_some_and(|p| p.distance(waited) < 0.05), "the other does not see it: {seen:?}");
    come_back(&mut t, 0, key);
    let you = t.seats[0].online.you.unwrap();
    assert!(t.seats[0].online.stats.back, "came back to it");
    assert_eq!(t.seats[0].online.key, Some(key), "the same key holds");
    assert_eq!(t.host.waiting().count(), 0, "nothing waits any more");
    let back_at = t.host.player(you).unwrap().pilot.position;
    assert!(back_at.distance(waited) < 0.05, "back where it waited: {:.3} m off", back_at.distance(waited));
    assert!(t.seats[0].me.pilot.position.distance(back_at) < 0.05, "our game has it there too");
    let fixed = t.seats[0].online.stats.corrections;
    t.run(6.0, 60.0, |k, me, n| {
        if k == 0 {
            walk(me, n)
        }
    });
    let fixes = t.seats[0].online.stats.corrections - fixed;
    let off = t.off(0, 240);
    println!("back: {fixes} corrections, {off:.5} m off, the key held");
    assert!(fixes == 0, "{fixes} corrections after coming back");
    assert!(off < 1e-3, "{off} m off");
    // (a key that takes no body: in anew, at the start, with a key of its own)
    let k = t.join("jugador2");
    let _ = k;
    let end = t.net.endpoint();
    let client = lunar_net::Client::with_transport(Box::new(end), t.link.addr(), "jugador3", BUILD, defs().fingerprint);
    let game = new_game();
    let me = Player::new(game.bodies.clone(), &game.site, defs().scenario.player);
    t.seats.push(Seat { at: t.link.addr(), online: Online::back(client, defs().scenario.player, 0x1234), game, me, path: Vec::new(), said: Vec::new() });
    let n = t.seats.len() - 1;
    for _ in 0..600 {
        t.frame(1.0 / 60.0, |_, _, _| {});
        if t.seats[n].online.live() {
            break;
        }
    }
    assert!(t.seats[n].online.live());
    assert!(!t.seats[n].online.stats.back);
    assert!(t.seats[n].online.key.is_some_and(|k| k != 0x1234));
    // (and what nobody comes back for is gone after its while)
    t.host.config.keep = 2.0;
    t.net.cut(t.seats[1].at, true);
    t.run(14.0, 60.0, |_, _, _| {});
    assert_eq!(t.host.stats.forgotten, 1, "forgotten after its while");
    assert_eq!(t.host.waiting().count(), 0);
}

#[test]
fn the_game_kept_and_taken_up_again_is_as_it_was_and_each_comes_back_to_their_body() {
    // a shot that breaks a ship and digs the ground, a walk; the game kept, the server stopped
    // and started again from what it kept: every structure, piece and crater as it was, what was
    // flying flying on as it was, and the player, coming back with their key, in their body where
    // it was
    let mut t = Table::new(1, 61, Conditions::default());
    t.run(1.0, 60.0, |_, _, _| {});
    {
        let s = &mut t.seats[0];
        let eye = s.me.pilot.position;
        let set = &s.game.builds.set;
        let st = s.game.ships.list.iter().filter_map(|sh| set.get(sh.structure)).min_by(|a, b| a.pos.distance(eye).total_cmp(&b.pos.distance(eye))).unwrap();
        let dir = (st.to_world(st.center) - eye).normalize();
        let bodies = s.game.bodies.clone();
        assert!(s.game.blasts.fire_from("personalizado", eye + dir, dir, DVec3::ZERO, None, &bodies, &mut s.game.builds));
    }
    t.run(6.0, 60.0, |_, me, n| walk(me, n));
    assert!(t.host.game.struck > 0, "the shot struck nothing");
    let key = t.seats[0].online.key.unwrap();
    let id = t.seats[0].online.you.unwrap();
    let body = t.host.player(id).unwrap().pilot.summary();
    // (and in the air as it is kept: a rocket, a missile, a guided one and a decoy, going up)
    {
        let g = &mut t.host.game;
        let b = g.bodies.get(g.site.body);
        let up = g.site.at(0.0, 0.0);
        let from = b.above_ground(up, 50.0);
        let bodies = g.bodies.clone();
        assert!(g.blasts.fire_from("cohete", from, up, DVec3::ZERO, None, &bodies, &mut g.builds));
        g.blasts.missiles.fire(0, from + up * 5.0, up * 300.0, 7);
        assert!(g.blasts.guided.launch(0, from + up * 10.0, up * 40.0, None, 0));
        assert!(g.blasts.guided.release(0, from + up * 15.0, up * 20.0));
    }
    t.host.step();
    let mut kept = Vec::new();
    let began = std::time::Instant::now();
    t.host.save(defs().fingerprint, &mut kept);
    let save_ms = began.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(lunar_play::save::saves(&kept), Some(1));
    // (cut short, or a bit wrong, or of other data: not taken up)
    assert!(lunar_play::save::saves(&kept[..kept.len() - 1]).is_none());
    let mut bad = kept.clone();
    bad[kept.len() / 2] ^= 4;
    assert!(lunar_play::save::saves(&bad).is_none());
    let config = HostConfig { cheats: true, ..HostConfig::default() };
    assert!(Host::load(new_game(), defs().scenario.player, config.clone(), defs().fingerprint ^ 1, &kept).is_err());
    let fresh = new_game();
    let began = std::time::Instant::now();
    let host = Host::load(fresh, defs().scenario.player, config, defs().fingerprint, &kept).unwrap_or_else(|e| panic!("{e}"));
    let load_ms = began.elapsed().as_secs_f64() * 1000.0;
    let (a, b) = (&t.host.game, &host.game);
    assert_eq!(a.step, b.step);
    assert_eq!(a.struck, b.struck);
    assert_eq!(a.builds.set.next_free(), b.builds.set.next_free());
    let ids = |g: &Game| g.builds.set.list.iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(ids(a), ids(b), "the same structures");
    let digest = |g: &Game, id: u64| {
        let s = g.builds.set.get(id)?;
        let sh = g.ships.by_structure(id).map(|n| &g.ships.list[n]);
        Some(lunar_ship::sync::Digest::of(sh, s).hash)
    };
    for s in &a.builds.set.list {
        let o = b.builds.set.get(s.id).unwrap();
        assert_eq!(digest(a, s.id), digest(b, s.id), "structure {} is not as it was", s.id);
        assert_eq!((s.lineage, s.born, s.resting, s.held), (o.lineage, o.born, o.resting, o.held), "structure {}", s.id);
        assert!(s.pos.distance(o.pos) < 1e-9 && s.rot.angle_between(o.rot) < 1e-6, "structure {} moved: {:.2e} m", s.id, s.pos.distance(o.pos));
    }
    assert_eq!(a.ships.list.len(), b.ships.list.len());
    // (what flies, to the last bit, and flying on alike)
    let flying = |g: &Game| {
        let bl = &g.blasts;
        (
            bl.rounds.list.iter().map(|r| (r.pos, r.vel, r.left, r.tag)).collect::<Vec<_>>(),
            bl.missiles.list.iter().map(|m| (m.pos, m.vel, m.t, m.tag)).collect::<Vec<_>>(),
            bl.guided.list.iter().map(|m| (m.pos, m.vel, m.t, m.id)).collect::<Vec<_>>(),
            bl.guided.decoys.iter().map(|d| (d.pos, d.vel, d.age, d.id)).collect::<Vec<_>>(),
        )
    };
    let was = flying(a);
    assert!(!was.0.is_empty() && !was.1.is_empty() && !was.2.is_empty() && !was.3.is_empty(), "nothing flying to keep: {was:?}");
    assert_eq!(was, flying(b), "what flies is not as it was");
    let ground = |g: &Game| g.bodies.get(g.site.body).deform().craters().to_vec();
    assert!(!ground(a).is_empty(), "the shot dug nothing");
    assert_eq!(ground(a), ground(b), "the ground as it was");
    {
        // (another taken up from it, and the one kept, half a second on: alike)
        let config = HostConfig { cheats: true, ..HostConfig::default() };
        let mut other = Host::load(new_game(), defs().scenario.player, config, defs().fingerprint, &kept).unwrap();
        let mut kept_one = std::mem::replace(&mut t.host, Host::new(new_game(), defs().scenario.player, HostConfig::default()));
        for _ in 0..30 {
            kept_one.step();
            other.step();
        }
        assert_eq!(flying(&kept_one.game), flying(&other.game), "what flies does not fly on alike");
        t.host = kept_one;
    }
    let waiting: Vec<u32> = host.waiting().collect();
    assert_eq!(waiting.len(), 1, "the body waits");
    assert!(host.player(waiting[0]).unwrap().pilot.summary().near(&body), "the body as it was");
    // (the server stopped and started again: the player comes back)
    t.host = host;
    t.server = Server::new(ServerConfig { game: Some((BUILD.to_string(), defs().fingerprint)), ..ServerConfig::default() });
    come_back(&mut t, 0, key);
    assert!(t.seats[0].online.stats.back, "came back to it");
    let you = t.seats[0].online.you.unwrap();
    assert!(t.host.player(you).unwrap().pilot.summary().near(&body), "back in it where it was");
    let fixed = t.seats[0].online.stats.corrections;
    t.run(5.0, 60.0, |_, me, n| walk(me, n));
    let fixes = t.seats[0].online.stats.corrections - fixed;
    let off = t.off(0, 240);
    println!("kept: {} structures, {:.1} kB in {save_ms:.2} ms, taken up in {load_ms:.2} ms; back: {fixes} corrections, {off:.5} m off", t.host.game.builds.set.list.len(), kept.len() as f64 / 1000.0);
    assert!(fixes == 0, "{fixes} corrections after coming back");
    assert!(off < 1e-3, "{off} m off");
}

#[test]
fn a_late_comer_is_told_a_world_too_big_for_one_message() {
    // a thousand craters round the site and two dozen ships by it: more than one reliable message
    // carries. Whoever comes now is told all of it, in as many as it takes
    let mut t = Table::new(1, 71, Conditions { delay: 0.03, loss: 0.02, ..Conditions::default() });
    {
        let g = &mut t.host.game;
        let b = g.bodies.get(g.site.body);
        for k in 0..1000 {
            let (e, n) = ((k % 40) as f64 * 15.0 - 300.0, (k / 40) as f64 * 15.0 - 200.0);
            let c = lunar_core::deform::Crater { dir: g.site.at(e, n), radius: 2.0 + (k % 3) as f64, depth: 0.6, rim: 0.15, seed: (k as f64 * 0.37).fract(), ground: 0.0 };
            b.edit(|d| d.put_from(d.craters().len(), &[c]));
        }
        let kinds = ["alcotan", "abejorro", "azor"];
        for k in 0..24 {
            let a = k as f64 / 24.0 * std::f64::consts::TAU;
            let up = g.site.at(150.0 * a.cos(), 150.0 * a.sin());
            let pos = b.above_ground(up, 6.0);
            let rot = Quat::from_rotation_arc(glam::Vec3::Y, up.as_vec3());
            g.ships.spawn_free(&mut g.builds, kinds[k % 3], pos, rot).unwrap();
        }
    }
    t.run(1.0, 60.0, |_, _, _| {});
    let late = t.join("tarde");
    t.run(4.0, 60.0, |_, _, _| {});
    let ground = |g: &Game| g.bodies.get(g.site.body).deform().craters().to_vec();
    assert_eq!(ground(&t.host.game).len(), 1000);
    assert_eq!(ground(&t.seats[late].game), ground(&t.host.game), "the ground, every crater, in its order");
    let ships = |g: &Game| g.ships.list.iter().map(|sh| sh.structure).collect::<Vec<_>>();
    for id in ships(&t.host.game) {
        assert!(t.seats[late].game.builds.set.get(id).is_some(), "ship {id} not told");
    }
    assert_eq!(t.host.stats.too_big, 0);
    println!("told to who came late: {:.1} kB of events, {} structures", t.host.stats.event_bytes as f64 / 1000.0, t.seats[late].game.builds.set.list.len());
}

#[test]
fn a_cheating_client_gets_nothing_by_it_and_the_honest_one_plays_on() {
    // one plays honestly; another is a client made by hand that says whatever it likes: commands
    // by the hundred, a look no head can take, what only tests may, what is out of reach, the
    // same structure asked for again and again, garbage, and datagrams in the name of the honest
    // one. None of it does anything; the honest one is never put right
    use lunar_play::blasts::{Launch, What};
    use lunar_play::net::{self as wire, Act, Cmd};
    let mut t = Table::new(1, 83, Conditions::default());
    t.host.config.cheats = false;
    let mut cheat = lunar_net::Client::with_transport(Box::new(t.net.endpoint()), t.link.addr(), "tramposo", BUILD, defs().fingerprint);
    let mut out = Vec::new();
    let mut frames = 0;
    while t.host.ids().count() < 2 {
        t.frame(1.0 / 60.0, |_, me, n| walk(me, n));
        cheat.update(t.now);
        cheat.events().for_each(drop);
        frames += 1;
        assert!(frames < 600, "the cheat never got in");
    }
    let them = cheat.id().unwrap();
    let mut dice = 0x9E37_79B9_7F4A_7C15u64;
    let mut roll = move || {
        dice ^= dice << 13;
        dice ^= dice >> 7;
        dice ^= dice << 17;
        dice
    };
    let before = t.host.stats;
    for f in 0..240u64 {
        let step = t.host.game.step;
        // (commands by the hundred, each running, with a look straight through the head)
        for k in 0..40 {
            let c = Cmd { step: step + 2 + (k % 3), input: Input { forward: 1.0, run: true, ..Input::default() }, yaw: 3.0, pitch: 50.0, ..Cmd::default() };
            wire::write_cmds(&[c], None, &mut out);
            cheat.send_quick(&out);
        }
        // (what is not let be, and what is out of reach)
        let far = t.host.game.bodies.get(t.host.game.site.body).above_ground(t.host.game.site.at(3000.0, 0.0), 2.0);
        let acts = [
            Act::Body(vec![0; 40]),
            Act::Spawn { kind: "cachalote".into(), pos: far, rot: Quat::IDENTITY },
            Act::Launch(Launch { what: What::Boom(0), from: far, dir: DVec3::X, speed: 10.0, vel: DVec3::ZERO, target: None, by: None }, 1),
            Act::Launch(Launch { what: What::Shot(0), from: far, dir: DVec3::X, speed: 900.0, vel: DVec3::ZERO, target: None, by: None }, 2),
            Act::Mend { structure: 1, part: 0, hp: 1e9 },
            Act::Resync { id: 1 },
            Act::Resync { id: 2 },
        ];
        for a in &acts {
            wire::write_act(step, a, &mut out);
            cheat.send_game(&out);
        }
        // (garbage that says it is a command or an act)
        for _ in 0..20 {
            let n = 1 + (roll() % 60) as usize;
            let mut junk: Vec<u8> = (0..n).map(|_| roll() as u8).collect();
            junk[0] = if roll() % 2 == 0 { wire::CMDS } else { wire::ACT };
            cheat.send_quick(&junk);
        }
        // (datagrams in the honest one's name, from their address: not signed with their key)
        if f % 4 == 0 {
            let mut fake = vec![4u8];
            fake.extend((0..40).map(|_| roll() as u8));
            t.net.inject(t.seats[0].at, t.link.addr(), &fake);
        }
        t.frame(1.0 / 60.0, |_, me, n| walk(me, n));
        cheat.update(t.now);
        cheat.events().for_each(drop);
    }
    let s = t.host.stats;
    let body = &t.host.player(them).expect("the cheat is still there").pilot;
    println!(
        "cheat: {} flooded, {} denied, {} garbled; the server's net: {} forged; the honest one: {} corrections",
        s.flooded - before.flooded,
        s.denied - before.denied,
        s.garbled - before.garbled,
        t.server.stats().forged,
        t.seats[0].online.stats.corrections
    );
    assert!(body.pitch.abs() <= std::f64::consts::FRAC_PI_2 + 1e-9, "a look no head can take: {}", body.pitch);
    assert!(s.flooded - before.flooded > 1000, "commands by the hundred are dropped");
    assert!(s.denied - before.denied > 100, "what is not let be is refused");
    assert!(s.garbled > before.garbled, "garbage is counted");
    assert!(t.server.stats().forged > 0, "what is not signed by them is not theirs");
    assert_eq!(t.host.game.ships.list.iter().filter(|sh| t.host.game.builds.set.get(sh.structure).is_some_and(|st| st.pos.distance(DVec3::ZERO) > 0.0 && st.to_world(st.center).distance(t.host.game.site.at(3000.0, 0.0) * t.host.game.bodies.get(t.host.game.site.body).radius) < 500.0)).count(), 0, "no ship was put");
    // (the cheat's body went no faster than anyone's: one command a step)
    let walked = body.velocity_in(&t.host.game.builds.set).length();
    assert!(walked < 8.0 && !body.flying, "{walked} m/s, flying {}", body.flying);
    assert!(matches!(t.seats[0].online.status(), lunar_net::Status::Connected { .. }), "the honest one is still in");
    assert_eq!(t.seats[0].online.stats.corrections, 0, "and was never put right");
    assert!(t.off(0, 120) < 1e-3);
}

#[test]
fn garbage_said_to_the_game_breaks_nothing() {
    // what a game says, cut short and with bits changed, and noise that says it is a command or an
    // act, fifty thousand times, from one who is in the game: nothing panics, nothing it says that
    // makes no sense is taken, and the honest player beside it plays on
    use lunar_play::net::{self as wire, Act, Check, Cmd};
    let mut t = Table::new(1, 89, Conditions::default());
    t.host.config.cheats = false;
    let noise = 999_999;
    t.host.join(noise);
    let mut real: Vec<Vec<u8>> = Vec::new();
    let mut out = Vec::new();
    let cmds: Vec<Cmd> = (0..4).map(|k| Cmd { step: 100 + k, input: Input { forward: 0.5, jump: k == 2, ..Input::default() }, yaw: 0.3 * k as f64, aim: Some(glam::Vec3::Y), frame: Some([glam::Vec3::Y, glam::Vec3::X]), keys: 5, ..Cmd::default() }).collect();
    wire::write_cmds(&cmds, Some(Check { step: 99, body: Summary::default(), fixes: 3 }), &mut out);
    real.push(out.clone());
    for a in [Act::Grab, Act::Release, Act::Wheel(2.0), Act::Sit { ship: 5, seat: 1 }, Act::Stand, Act::Resync { id: 3 }, Act::Back { key: 7 }, Act::Body(vec![1; 90]), Act::Mend { structure: 2, part: 3, hp: 4.0 }] {
        wire::write_act(100, &a, &mut out);
        real.push(out.clone());
    }
    let mut dice = 0x2545_F491_4F6C_DD1Du64;
    let mut roll = move || {
        dice ^= dice << 13;
        dice ^= dice >> 7;
        dice ^= dice << 17;
        dice
    };
    let before = t.host.stats.garbled;
    for round in 0..50_000u64 {
        let mut m = match round % 3 {
            0 => {
                let r = &real[(roll() % real.len() as u64) as usize];
                r[..1 + (roll() as usize % r.len())].to_vec()
            }
            1 => {
                let mut r = real[(roll() % real.len() as u64) as usize].clone();
                for _ in 0..1 + roll() % 3 {
                    let at = (roll() % r.len() as u64) as usize;
                    r[at] ^= 1 << (roll() % 8);
                }
                r
            }
            _ => (0..1 + roll() % 80).map(|_| roll() as u8).collect(),
        };
        if round % 7 == 0 && !m.is_empty() {
            m[0] = if roll() % 2 == 0 { wire::CMDS } else { wire::ACT };
        }
        t.host.take(noise, &m);
        if round % 500 == 0 {
            t.frame(1.0 / 60.0, |_, me, n| walk(me, n));
        }
    }
    t.run(2.0, 60.0, |_, me, n| walk(me, n));
    println!("garbage: {} taken for nonsense of 50 000 (the rest made sense, or came too fast)", t.host.stats.garbled - before);
    assert!(t.host.stats.garbled - before > 10_000);
    assert_eq!(t.seats[0].online.stats.corrections, 0);
    assert!(t.off(0, 120) < 1e-3);
}

#[test]
fn a_ships_gun_fired_from_its_seat_fires_in_the_server_and_everyone_sees_it() {
    // the Azor far from every body, its pilot sat by their own game, the master armed, the
    // trigger key held: its gun fires in the server (the seat's keys drive its controls there as
    // here), and the pilot and the one who watches see each round once
    let mut t = Table::new(2, 97, Conditions { delay: 0.03, loss: 0.01, ..Conditions::default() });
    (t.host.config.rule.near, t.host.config.rule.most) = (1.0e8, 1.0e8);
    let far = DVec3::new(-2.0e6, 3.0e6, 1.5e6);
    t.host.game.watchers.push(far);
    let id = t.host.game.ships.spawn_free(&mut t.host.game.builds, "azor", far, Quat::IDENTITY).unwrap();
    t.run(0.3, 60.0, |_, _, _| {});
    let you = t.seats[0].online.you.unwrap();
    let n = t.host.game.ships.by_structure(id).unwrap();
    let exit = glam::Vec3::from_array(t.host.game.ships.list[n].kind.seats[0].def.salida);
    let (game, p) = t.host.game_and_player(you).unwrap();
    p.pilot.put_on(&game.builds.set, id, exit);
    t.run(2.0, 60.0, |_, _, _| {});
    {
        let s = &mut t.seats[0];
        lunar_play::seats::sit(&mut s.me.pilot, &s.game.ships, &s.game.builds.set, id, 0, |_, _| false).unwrap();
    }
    t.run(0.5, 60.0, |_, _, _| {});
    assert!(t.host.player(you).unwrap().pilot.seat.is_some_and(|s| s.structure == id), "the server did not sit us");
    // (armed by the pilot's hand: the cover up, the master to ARMADO; done in their game and said)
    for want in ["combate/armas_tapa", "combate/armas_maestro"] {
        let s = &mut t.seats[0];
        let n = s.game.ships.by_structure(id).unwrap();
        let control = s.game.ships.list[n].panels.controls.iter().position(|c| c.id == want).expect("the control");
        assert!(lunar_play::controls::set(&mut s.game.ships, &s.game.builds.set, id, control, 1.0));
        let step = s.game.step;
        s.online.act(step, &lunar_play::net::Act::Control { ship: id, control: control as u16, value: 1.0 });
        t.run(0.2, 60.0, |_, _, _| {});
    }
    let armed = |g: &Game| g.ships.list[g.ships.by_structure(id).unwrap()].signal("armas.maestro");
    // (the one who watches is thousands of kilometres off: its copy runs coarse, and what its
    // switches say is worked out when it runs in full)
    assert_eq!((armed(&t.host.game), armed(&t.seats[0].game)), (Some(1.0), Some(1.0)), "armed in the server and in the pilot's game");
    let k = {
        let g = &t.seats[0].game;
        let (keys, _) = lunar_ship::seat_keys::keys(&g.ships.list[g.ships.by_structure(id).unwrap()], 0);
        keys.iter().position(|k| k.key == "disparar").expect("a key to fire")
    };
    let before = (t.host.game.blasts.started, t.seats[0].game.blasts.started, t.seats[1].game.blasts.started);
    let sent = t.host.stats.snap_bytes + t.host.stats.event_bytes;
    for frame in 0..120 {
        t.seats[0].online.keys = if frame < 60 { 1 << k } else { 0 };
        t.frame(1.0 / 60.0, |_, _, _| {});
    }
    // (what the fight costs on the wire: a second of fire and one after, to each of the two)
    let per_player = (t.host.stats.snap_bytes + t.host.stats.event_bytes - sent) as f64 / 2.0 / 2.0;
    println!("in the fight, each player is sent {:.1} kB/s", per_player / 1000.0);
    assert!(per_player < 64_000.0, "{per_player} B/s each");
    t.run(1.0, 60.0, |_, _, _| {});
    let fired = t.host.game.blasts.started - before.0;
    let (pilot, watcher) = (t.seats[0].game.blasts.started - before.1, t.seats[1].game.blasts.started - before.2);
    println!("the Azor's gun from its seat: {fired} let fly by the server; the pilot saw {pilot}, the one who watches {watcher}");
    assert!(fired > 0, "the gun did not fire in the server");
    assert_eq!((pilot, watcher), (fired, fired), "each sees each once");
}

#[test]
fn two_who_reach_for_one_crate_one_has_it_and_a_weld_is_the_same_for_all() {
    // in the hold of the Cachalote two reach for the same crate: the first has it, the other's
    // game lets it go (`Event::Unheld`). Then a part of the ship hurt, welded by one of them: it
    // mends no faster than a welder can, and the three games have it alike
    let cond = Conditions { delay: 0.03, jitter: 0.005, loss: 0.01, ..Conditions::default() };
    let mut t = Table::new(3, 37, cond);
    let ship = t.host.game.ships.list.iter().find(|sh| sh.kind.id == "cachalote").expect("the Cachalote").structure;
    for _ in 0..20 {
        t.run(0.5, 60.0, |_, _, _| {});
        if t.host.game.builds.set.get(ship).is_some_and(|s| s.resting) {
            break;
        }
    }
    let (clamp, zone) = {
        let g = &t.host.game;
        let sh = &g.ships.list[g.ships.by_structure(ship).unwrap()];
        let c = sh.kind.clamps.iter().position(|c| c.id == "anclaje_a4").expect("the clamp");
        let s = g.builds.set.get(ship).unwrap();
        (c, s.to_world(sh.kind.clamps[c].zone.map_or(glam::Vec3::ZERO, |z| z.centre)))
    };
    let at = glam::Vec3::new(0.0, 0.05, -13.0);
    for (k, off) in [(0usize, 0.0f32), (1, 0.9)] {
        let id = t.seats[k].online.you.unwrap();
        let (game, p) = t.host.game_and_player(id).unwrap();
        p.pilot.put_on(&game.builds.set, ship, at + glam::Vec3::X * off);
    }
    t.run(2.0, 60.0, |_, _, _| {});
    let made_from = t.host.game.builds.set.next_free();
    {
        let s = &mut t.seats[0];
        lunar_play::controls::act(&mut s.game.ships, ship, lunar_play::controls::Act::Clamp(clamp as u16, true));
        s.online.act(s.game.step, &lunar_play::net::Act::Hand { ship, act: lunar_play::controls::Act::Clamp(clamp as u16, true) });
    }
    t.run(1.5, 60.0, |_, _, _| {});
    let cargo = t.host.game.builds.set.list.iter().filter(|s| s.id >= made_from && t.host.game.ships.by_structure(s.id).is_none()).min_by(|a, b| a.pos.distance(zone).total_cmp(&b.pos.distance(zone))).map(|s| s.id).expect("nothing came loose");
    // (each looks at it and takes it, the first a moment before the other)
    let take = |t: &mut Table, k: usize| {
        let s = &mut t.seats[k];
        let c = s.game.builds.set.get(cargo).unwrap();
        s.me.pilot.look_at(c.to_world(c.center));
        let view = s.me.pilot.view_aboard(&s.game.builds.set).unwrap_or_else(|| s.me.pilot.view());
        s.me.hands.grab(&s.game.builds.set, &s.game.ships, &view, Some(ship)).expect("it can be taken here");
    };
    take(&mut t, 0);
    t.run(0.5, 60.0, |_, _, _| {});
    take(&mut t, 1);
    t.run(1.0, 60.0, |_, _, _| {});
    let (a, b) = (t.seats[0].online.you.unwrap(), t.seats[1].online.you.unwrap());
    assert_eq!(t.host.player(a).unwrap().hands.holding(), Some(cargo), "the first has it");
    assert_eq!(t.host.player(b).unwrap().hands.holding(), None, "the other does not");
    assert_eq!(t.seats[1].me.hands.holding(), None, "and the other's game let it go");
    {
        let s = &mut t.seats[0];
        s.me.hands.release(&mut s.game.builds);
    }
    t.run(1.0, 60.0, |_, _, _| {});
    // (a part of the ship near the second, hurt: as a blow would leave it, told to all)
    let (part, max) = {
        let g = &t.host.game;
        let eye = t.host.player(b).unwrap().pilot.position;
        let s = g.builds.set.get(ship).unwrap();
        let k = (0..s.parts.len())
            .filter(|&k| s.parts[k].alive && s.parts[k].max_hp > 0.0)
            .min_by(|&i, &j| s.to_world(s.parts[i].local.translation.into()).distance(eye).total_cmp(&s.to_world(s.parts[j].local.translation.into()).distance(eye)))
            .unwrap();
        (k, s.parts[k].max_hp)
    };
    {
        let k = t.host.game.builds.set.index_of(ship).unwrap();
        let st = &mut t.host.game.builds.set.list[k];
        st.parts[part].hp = max * 0.01;
        st.version += 1;
    }
    t.run(0.5, 60.0, |_, _, _| {});
    let hp = |g: &Game| g.builds.set.get(ship).unwrap().parts[part].hp;
    for (k, s) in t.seats.iter().enumerate() {
        assert_eq!(hp(&s.game), hp(&t.host.game), "player {k} does not have the part as hurt");
    }
    // (welded: asked for all of it in each step, for half a second: first with the hands free,
    // which mends nothing where tests are not let be; then with the welder, which gives what it
    // gives a second and a little more, no more)
    t.host.config.cheats = false;
    let (welder, rate) = t.host.game.gear.tools.iter().enumerate().find_map(|(k, x)| match x.kind {
        ToolKind::Soldador { ritmo, .. } => Some((k as u8 + 1, ritmo)),
        ToolKind::Lanzador { .. } => None,
    }).expect("a welder");
    let weld = |t: &mut Table| {
        for _ in 0..30 {
            let s = &mut t.seats[1];
            let step = s.game.step;
            s.online.act(step, &lunar_play::net::Act::Mend { structure: ship, part: part as u32, hp: max });
            t.frame(1.0 / 60.0, |_, _, _| {});
        }
        t.run(0.2, 60.0, |_, _, _| {});
    };
    let start = hp(&t.host.game);
    weld(&mut t);
    assert_eq!(hp(&t.host.game), start, "it mended with the hands free");
    t.seats[1].online.tool = welder;
    t.run(0.2, 60.0, |_, _, _| {});
    weld(&mut t);
    let mended = hp(&t.host.game);
    println!("welded: {:.0} of {max:.0} hp ({start:.0} at the start; a welder, {:.0} a second)", mended, rate * max);
    assert!(mended - start > max * rate * 0.5 * 0.9, "the weld did little: {mended} of {max}");
    assert!(mended - start < max * rate * 1.5, "it mended faster than a welder can: {mended} of {max}");
    t.run(0.5, 60.0, |_, _, _| {});
    let mended = hp(&t.host.game);
    for (k, s) in t.seats.iter().enumerate() {
        assert_eq!(hp(&s.game), mended, "player {k} does not have the weld");
    }
}

#[test]
fn parts_put_back_by_a_welder_are_back_in_every_game_one_at_a_time() {
    // two parts of the Cachalote's hold gone, one aboard with the welder in hand: put back, each
    // is back in every game (as the server put it); the second asked at once is refused (a welder
    // puts back one at a time, as long as it takes) and put back once that is over
    let cond = Conditions { delay: 0.03, jitter: 0.005, loss: 0.01, ..Conditions::default() };
    let mut t = Table::new(2, 43, cond);
    let ship = t.host.game.ships.list.iter().find(|sh| sh.kind.id == "cachalote").expect("the Cachalote").structure;
    for _ in 0..20 {
        t.run(0.5, 60.0, |_, _, _| {});
        if t.host.game.builds.set.get(ship).is_some_and(|s| s.resting) {
            break;
        }
    }
    let you = t.seats[0].online.you.unwrap();
    {
        let (game, p) = t.host.game_and_player(you).unwrap();
        p.pilot.put_on(&game.builds.set, ship, glam::Vec3::new(0.0, 0.05, -13.0));
    }
    t.run(2.0, 60.0, |_, _, _| {});
    // (the two nearest parts that hang on one other alone: gone, nothing else comes off)
    let parts: Vec<usize> = {
        let g = &t.host.game;
        let eye = t.host.player(you).unwrap().pilot.position;
        let s = g.builds.set.get(ship).unwrap();
        let joints = |k: usize| s.joints.iter().filter(|j| j.alive && (j.a as usize == k || j.b as usize == k)).count();
        let mut near: Vec<usize> = (0..s.parts.len()).filter(|&k| s.parts[k].alive && !s.parts[k].fragment && s.parts[k].max_hp > 0.0 && s.parts[k].bone == 0 && joints(k) == 1).collect();
        near.sort_by(|&i, &j| s.to_world(s.parts[i].local.translation.into()).distance(eye).total_cmp(&s.to_world(s.parts[j].local.translation.into()).distance(eye)));
        near.into_iter().take(2).collect()
    };
    assert_eq!(parts.len(), 2, "no parts to take off");
    let count = t.host.game.builds.set.list.len();
    {
        let k = t.host.game.builds.set.index_of(ship).unwrap();
        let st = &mut t.host.game.builds.set.list[k];
        for &p in &parts {
            (st.parts[p].alive, st.parts[p].hp) = (false, 0.0);
        }
        st.version += 1;
    }
    t.run(0.5, 60.0, |_, _, _| {});
    assert_eq!(t.host.game.builds.set.list.len(), count, "something came off");
    let alive = |g: &Game, p: usize| g.builds.set.get(ship).unwrap().parts[p].alive;
    for (k, s) in t.seats.iter().enumerate() {
        assert!(parts.iter().all(|&p| !alive(&s.game, p)), "player {k} still has the parts");
    }
    // (where tests are not let be, with the welder in hand)
    t.host.config.cheats = false;
    let (welder, takes) = t.host.game.gear.tools.iter().enumerate().find_map(|(k, x)| match x.kind {
        ToolKind::Soldador { reconstruir, .. } => Some((k as u8 + 1, f64::from(reconstruir))),
        ToolKind::Lanzador { .. } => None,
    }).expect("a welder");
    t.seats[0].online.tool = welder;
    t.run(0.2, 60.0, |_, _, _| {});
    let denied = t.host.stats.denied;
    let rebuild = |t: &mut Table, p: usize| {
        let s = &mut t.seats[0];
        let step = s.game.step;
        s.online.act(step, &lunar_play::net::Act::Rebuild { structure: ship, part: p as u32 });
        t.run(0.5, 60.0, |_, _, _| {});
    };
    rebuild(&mut t, parts[0]);
    rebuild(&mut t, parts[1]);
    assert!(alive(&t.host.game, parts[0]) && !alive(&t.host.game, parts[1]), "the first is back, the second not yet");
    assert_eq!(t.host.stats.denied - denied, 1);
    t.run(takes, 60.0, |_, _, _| {});
    rebuild(&mut t, parts[1]);
    t.run(0.5, 60.0, |_, _, _| {});
    let hp = |g: &Game, p: usize| g.builds.set.get(ship).unwrap().parts[p].hp;
    for (k, s) in t.seats.iter().enumerate() {
        for &p in &parts {
            assert!(alive(&s.game, p), "player {k} does not have part {p} back");
            assert_eq!(hp(&s.game, p), hp(&t.host.game, p), "player {k} has part {p} otherwise");
        }
    }
    assert_eq!(t.host.game.builds.set.list.len(), count);
}

#[test]
fn a_ship_nobody_flies_in_orbit_is_where_the_server_has_it_in_every_game() {
    // the Alcotán in a circular orbit 18 km over the Moon, nobody aboard: one watches it from the
    // site, another comes in half a minute late; each one's copy is where the server has it, on
    // its orbit, for a minute and a half (150 km of it). Then the first is put aboard where the
    // ship is by now (not where it was last stepped, asleep, a second back: `Pilot::put_on`); it
    // is stepped from then on, they go round with it and nobody is put right for it
    let cond = Conditions { delay: 0.05, jitter: 0.005, loss: 0.02, ..Conditions::default() };
    let mut t = Table::new(1, 71, cond);
    // (known however far: what is tried here is where it is, not who knows it)
    (t.host.config.rule.near, t.host.config.rule.most) = (1.0e8, 1.0e8);
    let (pos, vel) = {
        let g = &t.host.game;
        let b = g.bodies.get(g.site.body);
        let dir = g.site.at(0.0, 0.0);
        // (within its reach: what pulls beyond it is nothing, `bodies/luna.jsonc`)
        let p = b.center + dir * (b.radius + b.reach * 0.6);
        let (pull, r) = (g.bodies.field(p).pull.length(), (p - b.center).length());
        (p, dir.any_orthonormal_vector() * (pull * r).sqrt())
    };
    let id = t.host.game.ships.spawn_free(&mut t.host.game.builds, "alcotan", pos, Quat::IDENTITY).unwrap();
    {
        let k = t.host.game.builds.set.index_of(id).unwrap();
        t.host.game.builds.set.list[k].vel = vel;
    }
    // (where a game has it at the server's step: as it is at its own moment, taken back along its
    // orbit by how far ahead that game is)
    let at = |g: &Game, step: u64| {
        let s = g.builds.set.get(id)?;
        let (p, v, _) = coasted(s, g.bodies.field(s.pos).pull, g.builds.set.now - s.clock);
        let back = (g.step as f64 - step as f64) * STEP;
        Some(p - v * back + g.bodies.field(p).pull * 0.5 * back * back)
    };
    let mut worst = [0.0f64; 2];
    let mut late = None;
    for second in 0..90 {
        if second == 30 {
            late = Some(t.join("tarde"));
        }
        t.run(1.0, 60.0, |_, _, _| {});
        let step = t.host.game.step;
        let truth = at(&t.host.game, step).unwrap();
        for (k, s) in t.seats.iter().enumerate() {
            // (the late one from a few seconds after it came)
            if late == Some(k) && second < 35 {
                continue;
            }
            let mine = at(&s.game, step).unwrap_or_else(|| panic!("player {k} does not know the ship at {second} s"));
            worst[k.min(1)] = worst[k.min(1)].max(mine.distance(truth));
        }
    }
    // (a ship nobody is near is not stepped: where it is follows from its orbit, `coasted`)
    let travelled = at(&t.host.game, t.host.game.step).unwrap().distance(pos);
    println!("a ship in orbit nobody flies, {:.0} km on: the watcher's copy at most {:.3} m off, the late one's {:.3} m", travelled / 1000.0, worst[0], worst[1]);
    assert!(travelled > 100_000.0, "it did not go round: {travelled:.0} m");
    assert!(worst[0] < 0.1 && worst[1] < 0.1, "copies off: {worst:?}");
    // (one aboard, in its rooms: they go round with it)
    let you = t.seats[0].online.you.unwrap();
    let exit = {
        let g = &t.host.game;
        glam::Vec3::from_array(g.ships.list[g.ships.by_structure(id).unwrap()].kind.seats[0].def.salida)
    };
    {
        let (game, p) = t.host.game_and_player(you).unwrap();
        p.pilot.put_on(&game.builds.set, id, exit);
    }
    t.run(2.0, 60.0, |_, _, _| {});
    let first = t.seats[0].online.stats.corrections;
    let mut aboard = [0.0f64; 2];
    for _ in 0..10 {
        t.run(1.0, 60.0, |_, _, _| {});
        let step = t.host.game.step;
        let truth = at(&t.host.game, step).unwrap();
        for (k, s) in t.seats.iter().enumerate() {
            aboard[k.min(1)] = aboard[k.min(1)].max(at(&s.game, step).unwrap().distance(truth));
        }
    }
    let p = &t.host.player(you).unwrap().pilot;
    println!("aboard in orbit: {} corrections after being put there, copies at most {aboard:?} m off", t.seats[0].online.stats.corrections - first);
    assert!(p.ride.is_some_and(|r| r.id == id), "they did not go round with it");
    assert_eq!(t.seats[0].online.stats.corrections, first, "put right aboard in orbit ({:?})", t.seats[0].online.stats);
    assert!(aboard[0] < 0.1 && aboard[1] < 0.1, "copies off with one aboard: {aboard:?}");
}

#[test]
fn ships_too_far_to_know_in_full_are_known_from_afar_where_they_are() {
    // an Alcotán set down 40 km from the site and an Azor flying 5 km up 30 km off, past what is
    // known of them in full: the one at the site knows them from afar (`Online::far`), each of
    // its kind and where the server has it, a few times a second; put by the Alcotán, it is known
    // in full and not from afar
    let cond = Conditions { delay: 0.05, jitter: 0.005, loss: 0.02, ..Conditions::default() };
    let mut t = Table::new(1, 77, cond);
    let (parked, flying) = {
        let g = &mut t.host.game;
        let b = g.bodies.get(g.site.body);
        let (a, c) = (g.site.at(40_000.0, 0.0), g.site.at(-30_000.0, 0.0));
        let rot = |up: DVec3| Quat::from_rotation_arc(glam::Vec3::Y, up.as_vec3());
        let parked = g.ships.spawn_free(&mut g.builds, "alcotan", b.above_ground(a, 3.0), rot(a)).unwrap();
        let flying = g.ships.spawn_free(&mut g.builds, "azor", b.above_ground(c, 5000.0), rot(c)).unwrap();
        let k = g.builds.set.index_of(flying).unwrap();
        g.builds.set.list[k].vel = c.any_orthonormal_vector() * 600.0;
        (parked, flying)
    };
    t.run(1.0, 60.0, |_, _, _| {});
    let at = |g: &Game, id: u64| {
        let s = g.builds.set.get(id).unwrap();
        coasted(s, g.bodies.field(s.pos).pull, g.builds.set.now - s.clock).0
    };
    let mut worst = 0.0f64;
    for _ in 0..5 {
        t.run(1.0, 60.0, |_, _, _| {});
        let (o, g) = (&t.seats[0].online, &t.seats[0].game);
        let step = t.host.game.step;
        for (id, kind) in [(parked, "alcotan"), (flying, "azor")] {
            assert!(g.builds.set.get(id).is_none(), "{kind} is known in full at {:.0} km", at(&t.host.game, id).distance(t.host.player(o.you.unwrap()).unwrap().pilot.position) / 1000.0);
            let (_, pos) = o.far_now(step).find(|(w, _)| matches!(w, lunar_play::net::FarWhat::Ship { id: i, .. } if *i == id)).unwrap_or_else(|| panic!("{kind} not known from afar ({:?})", o.far));
            let k = o.far.iter().find_map(|f| match f.what {
                lunar_play::net::FarWhat::Ship { id: i, kind } if i == id => Some(kind),
                _ => None,
            });
            assert_eq!(k.map(|k| g.ships.kinds[usize::from(k)].id.as_str()), Some(kind));
            worst = worst.max(pos.distance(at(&t.host.game, id)));
        }
    }
    println!("known from afar: a ship set down 40 km off and one flying at 600 m/s 30 km off, at most {worst:.3} m from where the server has them");
    assert!(worst < 1.0, "{worst:.3} m off");
    // (beside the Alcotán: known in full, and not from afar)
    let you = t.seats[0].online.you.unwrap();
    {
        let (game, p) = t.host.game_and_player(you).unwrap();
        let s = game.builds.set.get(parked).unwrap();
        let beside = s.to_world(s.center) + (s.rot * glam::Vec3::X).as_dvec3() * 60.0;
        let b = game.bodies.get(game.site.body);
        let ground = b.above_ground(b.up(beside), 0.0);
        p.pilot.put(ground, b.up(ground));
    }
    t.run(3.0, 60.0, |_, _, _| {});
    let o = &t.seats[0].online;
    assert!(t.seats[0].game.builds.set.get(parked).is_some(), "the Alcotán is not known in full beside it");
    assert!(o.far.iter().all(|f| !matches!(f.what, lunar_play::net::FarWhat::Ship { id, .. } if id == parked)), "and still from afar");
}

#[test]
fn what_is_let_fly_is_what_the_hands_carry_and_what_is_not_is_gone_from_the_game_that_fired_it() {
    // where tests are not let be, a rocket fired with the hands free, before the launcher is
    // loaded again, or a test key's gun with the launcher in hand, is not let fly: the game that
    // fired it is told (`Event::Unfired`) and it is gone there as if it never was. With the
    // launcher in hand, at its pace, it flies in both
    let mut t = Table::new(1, 91, Conditions { delay: 0.03, jitter: 0.005, ..Conditions::default() });
    t.host.config.cheats = false;
    let (launcher, reload) = t.host.game.gear.tools.iter().enumerate().find_map(|(k, x)| match x.kind {
        ToolKind::Lanzador { recarga, .. } => Some((k as u8 + 1, recarga)),
        ToolKind::Soldador { .. } => None,
    }).expect("a launcher");
    let rocket = match &t.host.game.gear.tools[usize::from(launcher) - 1].kind {
        ToolKind::Lanzador { tiro, .. } => tiro.clone(),
        ToolKind::Soldador { .. } => unreachable!(),
    };
    t.run(1.0, 60.0, |_, _, _| {});
    // (fired as the window fires it: from before the eye, straight up, going as the body goes)
    let fire = |t: &mut Table, id: &str| {
        let s = &mut t.seats[0];
        let view = s.me.pilot.view();
        let from = view.eye + view.up * 0.5;
        let vel = s.me.pilot.motion_in(&s.game.builds.set).velocity_at(from);
        let bodies = s.game.bodies.clone();
        assert!(s.game.blasts.fire_from(id, from, view.up, vel, None, &bodies, &mut s.game.builds), "{id} did not fire");
    };
    let flying = |g: &Game| g.blasts.rounds.list.iter().filter(|r| r.tag != 0 && r.tag < lunar_play::blasts::OWN).count();
    let denied = t.host.stats.denied;
    // (the hands free)
    fire(&mut t, &rocket);
    assert_eq!(flying(&t.seats[0].game), 1);
    t.run(0.5, 60.0, |_, _, _| {});
    assert_eq!((flying(&t.seats[0].game), flying(&t.host.game)), (0, 0), "a rocket with the hands free");
    // (the launcher in hand: it flies; once more at once: not loaded yet)
    t.seats[0].online.tool = launcher;
    t.run(0.3, 60.0, |_, _, _| {});
    fire(&mut t, &rocket);
    t.run(0.5, 60.0, |_, _, _| {});
    assert_eq!((flying(&t.seats[0].game), flying(&t.host.game)), (1, 1), "the rocket of the launcher in hand");
    fire(&mut t, &rocket);
    t.run(0.5, 60.0, |_, _, _| {});
    assert_eq!((flying(&t.seats[0].game), flying(&t.host.game)), (1, 1), "a rocket before the launcher is loaded");
    // (loaded again: it flies; a test key's gun: not with this in hand)
    t.run(reload, 60.0, |_, _, _| {});
    fire(&mut t, &rocket);
    fire(&mut t, "metralleta");
    t.run(0.5, 60.0, |_, _, _| {});
    assert_eq!((flying(&t.seats[0].game), flying(&t.host.game)), (2, 2), "loaded again, the launcher's alone");
    assert_eq!(t.host.stats.denied - denied, 3, "what was refused");
    assert_eq!(t.seats[0].online.stats.corrections, 0);
}

#[test]
fn the_pilot_hands_over_at_orbital_speed_and_the_ship_goes_on_without_a_jerk() {
    // the Alcotán at 7.8 km/s far from every body, two aboard: one flies it a while, gets up, the
    // other sits and flies on. The ship goes on as the keys say, without a jump, the same in the
    // server and in the games; nobody is put right for it
    let cond = Conditions { delay: 0.04, jitter: 0.005, loss: 0.01, ..Conditions::default() };
    let mut t = Table::new(2, 41, cond);
    let far = DVec3::new(2.0e6, 3.0e6, -1.0e6);
    t.host.game.watchers.push(far);
    let id = t.host.game.ships.spawn_free(&mut t.host.game.builds, "alcotan", far, Quat::IDENTITY).unwrap();
    {
        let k = t.host.game.builds.set.index_of(id).unwrap();
        t.host.game.builds.set.list[k].vel = DVec3::new(7800.0, 0.0, 0.0);
    }
    // (a few steps for it to be whole, not so many that it is out of sight of where it was made:
    // a ship nobody is aboard nor near is stepped now and then, and one put aboard it then is
    // left where it was)
    t.run(0.2, 60.0, |_, _, _| {});
    // (where to stand: where one gets off the seat, and beside it, within reach of the seat)
    let spots: Vec<glam::Vec3> = {
        let g = &t.host.game;
        let n = g.ships.by_structure(id).unwrap();
        let exit = glam::Vec3::from_array(g.ships.list[n].kind.seats[0].def.salida);
        vec![exit, exit + glam::Vec3::X * 0.6]
    };
    // (the first aboard now; the second where one gets off, once the first sits)
    let put = |t: &mut Table, k: usize| {
        let you = t.seats[k].online.you.unwrap();
        let (game, p) = t.host.game_and_player(you).unwrap();
        p.pilot.put_on(&game.builds.set, id, spots[0]);
        t.run(2.0, 60.0, |_, _, _| {});
    };
    put(&mut t, 0);
    for k in 0..1 {
        let you = t.seats[k].online.you.unwrap();
        let srv = &t.host.player(you).unwrap().pilot;
        let mine = &t.seats[k].me.pilot;
        assert!(srv.ride.is_some_and(|r| r.id == id) && mine.ride.is_some_and(|r| r.id == id), "player {k} is not aboard: server {:?}, theirs {:?}", srv.ride.map(|r| r.local), mine.ride.map(|r| r.local));
    }
    let key = |t: &Table, name: &str| {
        let g = &t.seats[0].game;
        let (keys, _) = lunar_ship::seat_keys::keys(&g.ships.list[g.ships.by_structure(id).unwrap()], 0);
        keys.iter().position(|k| k.key == name).expect("the key")
    };
    let (ahead, up) = (key(&t, "avanzar"), key(&t, "subir"));
    let mut fixed = [t.seats[0].online.stats.corrections, t.seats[1].online.stats.corrections];
    let mut server: Vec<(u64, DVec3, DVec3)> = Vec::new();
    let mut worst_off = 0.0f64;
    // (who sits, what they hold, and for how long)
    let turns = [(0usize, ahead, 90), (1usize, up, 90)];
    for (who, k, frames) in turns {
        {
            let s = &mut t.seats[who];
            lunar_play::seats::sit(&mut s.me.pilot, &s.game.ships, &s.game.builds.set, id, 0, |_, _| false).unwrap();
        }
        t.run(0.3, 60.0, |_, _, _| {});
        let you = t.seats[who].online.you.unwrap();
        assert!(t.host.player(you).unwrap().pilot.seat.is_some_and(|s| s.structure == id), "the server did not sit player {who}");
        if who == 0 {
            put(&mut t, 1);
            let other = t.seats[1].online.you.unwrap();
            assert!(t.host.player(other).unwrap().pilot.ride.is_some_and(|r| r.id == id), "the second is not aboard");
            // (put aboard by the server, their game did not know: from here on, nothing)
            fixed = [t.seats[0].online.stats.corrections, t.seats[1].online.stats.corrections];
        }
        let phase = [t.seats[0].online.stats.corrections, t.seats[1].online.stats.corrections];
        for _ in 0..frames {
            t.seats[who].online.keys = 1 << k;
            t.frame(1.0 / 60.0, |_, _, _| {});
            let s = t.host.game.builds.set.get(id).unwrap();
            server.push((t.host.game.step, s.pos, s.vel));
            // (the pilot's game has the ship where the server has it, step by step)
            let mine = &t.seats[who].game;
            let ahead = (mine.step - t.host.game.step) as f64 * STEP;
            let m = mine.builds.set.get(id).unwrap();
            worst_off = worst_off.max((m.pos - m.vel * ahead).distance(s.pos));
        }
        t.seats[who].online.keys = 0;
        let flown = [t.seats[0].online.stats.corrections - phase[0], t.seats[1].online.stats.corrections - phase[1]];
        {
            let s = &mut t.seats[who];
            lunar_play::seats::stand(&mut s.me.pilot, &s.game.ships, &s.game.builds.set);
        }
        t.run(0.3, 60.0, |_, _, _| {});
        println!("player {who} flew: corrections while flying {flown:?}, then up: {:?}", [t.seats[0].online.stats.corrections - phase[0], t.seats[1].online.stats.corrections - phase[1]]);
        assert_eq!(flown, [0, 0], "put right while player {who} flew");
    }
    // (no jump from one step to the next: where it is follows from where it was and how it went)
    let mut worst_jump = 0.0f64;
    for w in server.windows(2) {
        let ((s0, p0, v0), (s1, p1, v1)) = (w[0], w[1]);
        let dt = (s1 - s0) as f64 * STEP;
        worst_jump = worst_jump.max((p1 - (p0 + (v0 + v1) * 0.5 * dt)).length());
    }
    let fixes = [t.seats[0].online.stats.corrections - fixed[0], t.seats[1].online.stats.corrections - fixed[1]];
    println!("handover at 7.8 km/s: worst jump {worst_jump:.4} m, the pilot's copy at most {worst_off:.3} m off, corrections {fixes:?}");
    assert!(worst_jump < 0.01, "the ship jumped {worst_jump:.4} m");
    assert!(worst_off < 0.3, "the pilot's game had the ship {worst_off:.3} m off");
    // (getting up where the other stands: each game puts them by where it has the other, a
    // moment old; a few, and only then)
    assert!(fixes.iter().all(|f| *f <= 5), "put right {fixes:?}");
}

#[test]
fn one_who_comes_into_a_busy_game_on_a_bad_network_plays_at_once_and_soon_has_it_all() {
    // ten playing, six ships and a hundred structures loose round the site, ten in a hundred
    // datagrams lost: one more comes in. They are in within two seconds, and have everything near
    // them as the server has it soon after; told they have it all (`Event::Ready`: the loading
    // screen comes off) when they do, not before
    let cond = Conditions { delay: 0.05, jitter: 0.01, loss: 0.1, ..Conditions::default() };
    let mut t = Table::new(10, 101, cond);
    {
        let g = &mut t.host.game;
        let b = g.bodies.get(g.site.body);
        for k in 0..6 {
            let a = k as f64 / 6.0 * std::f64::consts::TAU;
            let up = g.site.at(220.0 * a.cos(), 220.0 * a.sin());
            g.ships.spawn_free(&mut g.builds, ["alcotan", "abejorro", "azor"][k % 3], b.above_ground(up, 6.0), Quat::from_rotation_arc(glam::Vec3::Y, up.as_vec3())).unwrap();
        }
        for k in 0..100 {
            let (e, n) = ((k % 10) as f64 * 60.0 - 270.0, (k / 10) as f64 * 60.0 - 270.0);
            let up = g.site.at(e, n);
            let id = g.builds.set.spawn("torre", b.above_ground(up, 0.0), Quat::from_rotation_arc(glam::Vec3::Y, up.as_vec3())).unwrap();
            let i = g.builds.set.index_of(id).unwrap();
            g.builds.set.list[i].anchored = true;
        }
    }
    t.run(2.0, 60.0, |k, me, n| {
        if k % 2 == 0 {
            walk(me, n + k as u64 * 50)
        }
    });
    let began = t.now;
    let late = t.join("tarde");
    let playing = t.now - began;
    // (everything near them, as the server has it)
    let near: Vec<u64> = {
        let you = t.seats[late].online.you.unwrap();
        let eye = t.host.player(you).unwrap().pilot.position;
        let rule = t.host.config.rule;
        t.host.game.builds.set.list.iter().filter(|s| s.id >= t.host.game.builds.scenario_end && s.to_world(s.center).distance(eye) < rule.reach(f64::from(s.radius))).map(|s| s.id).collect()
    };
    let mut all = None;
    let mut ready = None;
    for f in 0..600 {
        t.frame(1.0 / 60.0, |k, me, n| {
            if k % 2 == 0 {
                walk(me, n + k as u64 * 50)
            }
        });
        let has = near.iter().filter(|id| t.seats[late].game.builds.set.get(**id).is_some()).count();
        if ready.is_none() && t.seats[late].online.ready {
            ready = Some((f as f64 / 60.0, has));
        }
        if has == near.len() && ready.is_some() {
            all = Some(f as f64 / 60.0);
            break;
        }
    }
    let all = all.unwrap_or_else(|| panic!("the one who came never had it all: {} of {} ({:?})", near.iter().filter(|id| t.seats[late].game.builds.set.get(**id).is_some()).count(), near.len(), t.seats[late].online.stats));
    let (ready, had) = ready.expect("never told it had it all");
    println!("came into a game of 10 with {} structures near (10 % lost): in at {playing:.2} s, everything near {all:.2} s later, told so at {ready:.2} s with {had}", near.len());
    assert_eq!(had, near.len(), "told it had it all with {had} of {}", near.len());
    assert!(near.len() >= 100, "the scene is wrong: {} near", near.len());
    assert!(playing < 2.0, "it took {playing:.2} s to play");
    assert!(all < 2.0, "and {all:.2} s more to have everything near");
}
