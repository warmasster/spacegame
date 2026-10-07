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
                let client = lunar_net::Client::with_transport(Box::new(net.endpoint()), addr, &format!("jugador{k}"), BUILD, defs().fingerprint);
                let game = new_game();
                let me = Player::new(game.bodies.clone(), &game.site, defs().scenario.player);
                Seat { online: Online::new(client, defs().scenario.player), game, me, path: Vec::new(), said: Vec::new() }
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
        let client = lunar_net::Client::with_transport(Box::new(self.net.endpoint()), addr, name, BUILD, defs().fingerprint);
        let game = new_game();
        let me = Player::new(game.bodies.clone(), &game.site, defs().scenario.player);
        self.seats.push(Seat { online: Online::new(client, defs().scenario.player), game, me, path: Vec::new(), said: Vec::new() });
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
        me.pilot.look_by(0.01, 0.0);
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
