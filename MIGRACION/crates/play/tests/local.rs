//! A game of one's own over a server in the process (`lunar_play::local`), in real time: the
//! server's thread steps the world at 60 Hz, this one plays it through a network in memory, and
//! another comes in from the real network (UDP on this machine) as to a game hosted here.
use glam::DVec3;
use lunar_net::now;
use lunar_play::{
    defs::Defs,
    game::{Game, Player},
    host::HostConfig,
    local::{Local, LocalConfig},
    net::BUILD,
    online::Online,
    pilot::Input,
};
use std::time::Duration;

fn defs() -> Defs {
    Defs::load(&lunar_play::root().join("assets/defs")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message))
}

fn game(defs: &Defs) -> Game {
    Game::new_apart(defs, &lunar_play::root().join("assets/defs"), 500, |_| true).unwrap()
}

/// A player's game: frames of real time until `until` says so, or `most` seconds.
struct Seat {
    online: Online,
    game: Game,
    me: Player,
    last: f64,
}

impl Seat {
    fn frame(&mut self, mut input: impl FnMut(&mut Player, u64)) {
        let t = now();
        self.online.receive(t, &mut self.game, &mut self.me);
        for _ in 0..self.online.steps(t - self.last) {
            input(&mut self.me, self.game.step);
            self.online.step(&mut self.game, &mut self.me);
        }
        self.last = t;
    }
}

fn walk(me: &mut Player, n: u64) {
    let n = n % 300;
    me.input = Input { forward: if n < 200 { 1.0 } else { 0.0 }, run: n > 100, jump: n == 150, ..Input::default() };
    if n % 5 == 0 {
        me.pilot.look(4.0, 0.0);
    }
}

#[test]
fn a_game_of_ones_own_is_a_game_over_a_server_in_the_process_and_others_can_come_in() {
    let defs = defs();
    let config = LocalConfig { host: HostConfig { cheats: true, ..HostConfig::default() }, udp: Some(0), name: "Partida de prueba".to_string(), max_players: 4, build: BUILD.to_string(), fingerprint: defs.fingerprint };
    let mut local = Local::start(game(&defs), defs.scenario.player, config).unwrap_or_else(|e| panic!("{e}"));
    let port = local.port().expect("others may come in");
    let mine = game(&defs);
    let me = Player::new(mine.bodies.clone(), &mine.site, defs.scenario.player);
    let mut a = Seat { online: Online::new(local.client("yo"), defs.scenario.player), game: mine, me, last: now() };
    let began = now();
    while !a.online.live() {
        assert!(now() - began < 5.0, "never in: {:?}", a.online.status());
        a.frame(|_, _| {});
        std::thread::sleep(Duration::from_millis(2));
    }
    let in_after = now() - began;
    // (walking 4 s: what the server does is what our game did)
    let t0 = now();
    while now() - t0 < 4.0 {
        a.frame(walk);
        std::thread::sleep(Duration::from_millis(3));
    }
    assert_eq!(a.online.stats.corrections, 0, "{:?}", a.online.stats);
    // (another, from the real network)
    let other = game(&defs);
    let me = Player::new(other.bodies.clone(), &other.site, defs.scenario.player);
    let client = lunar_net::Client::connect(&format!("127.0.0.1:{port}"), "visita", BUILD, defs.fingerprint).unwrap();
    let mut b = Seat { online: Online::new(client, defs.scenario.player), game: other, me, last: now() };
    let began = now();
    while !b.online.live() || b.online.others.is_empty() {
        assert!(now() - began < 5.0, "the other never got in: {:?}", b.online.status());
        a.frame(|_, _| {});
        b.frame(|_, _| {});
        std::thread::sleep(Duration::from_millis(2));
    }
    let seen = b.online.others[0].1.pos;
    assert!(seen.distance(a.me.pilot.position) < 1.0, "the other sees us {:.2} m off", seen.distance(a.me.pilot.position));
    // (put somewhere else by whoever runs our game, as a menu does: the server puts us there too)
    let there = a.me.pilot.position + DVec3::new(30.0, 0.0, 30.0);
    let up = a.game.bodies.get(a.game.site.body).up(there);
    let feet = a.game.bodies.get(a.game.site.body).above_ground(up, 0.0);
    a.me.pilot.put(feet, up);
    a.online.put(&a.game, &a.me);
    let t0 = now();
    while now() - t0 < 2.0 {
        a.frame(|me, _| me.input = Input::default());
        b.frame(|_, _| {});
        std::thread::sleep(Duration::from_millis(3));
    }
    assert_eq!(a.online.stats.corrections, 0, "put there and not put back: {:?}", a.online.stats);
    let stats = *local.stats.lock().unwrap();
    while let Ok(n) = local.notes.try_recv() {
        println!("servidor: {n}");
    }
    let host = local.stop().expect("the game back");
    let you = a.online.you.unwrap();
    assert!(host.player(you).unwrap().pilot.position.distance(feet) < 2.0, "the server has us there");
    println!("in after {in_after:.2} s; server: {stats:?}");
    assert!(stats.mean_ms < 16.0 && stats.players == 2, "{stats:?}");
}

#[test]
fn a_game_of_ones_own_is_kept_when_it_ends_and_taken_up_where_it_was() {
    let defs = defs();
    let dir = std::env::temp_dir().join(format!("luna-propia-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let keeping = || lunar_play::keep::Keeping { slots: lunar_play::keep::Slots::new(&dir.join("propia")), every: 0.0, fresh: false };
    let config = || LocalConfig { host: HostConfig { cheats: true, ..HostConfig::default() }, udp: None, name: "Partida propia".to_string(), max_players: 4, build: BUILD.to_string(), fingerprint: defs.fingerprint };
    let make = || Game::new_apart(&defs, &lunar_play::root().join("assets/defs"), 256, |_| true);
    // (played a while, walking off)
    let mut local = Local::start_kept(make, defs.scenario.player, config(), keeping()).unwrap();
    assert_eq!(local.back, None, "nothing kept yet");
    let mine = game(&defs);
    let me = Player::new(mine.bodies.clone(), &mine.site, defs.scenario.player);
    let mut a = Seat { online: Online::new(local.client("yo"), defs.scenario.player), game: mine, me, last: now() };
    let t0 = now();
    while now() - t0 < 3.0 {
        a.frame(walk);
        std::thread::sleep(Duration::from_millis(3));
    }
    let you = a.online.you.unwrap();
    let host = local.stop().expect("the game back");
    let was = host.player(you).unwrap().pilot.position;
    drop(local);
    assert!(dir.join("propia.a.bin").exists(), "kept when it ended");
    // (opened again: taken up, and back in that body)
    let mut local = Local::start_kept(make, defs.scenario.player, config(), keeping()).unwrap();
    let key = local.back.expect("a body waits");
    let mine = game(&defs);
    let me = Player::new(mine.bodies.clone(), &mine.site, defs.scenario.player);
    let mut b = Seat { online: Online::back(local.client("yo"), defs.scenario.player, key), game: mine, me, last: now() };
    let began = now();
    while !b.online.live() {
        assert!(now() - began < 5.0, "never back in: {:?}", b.online.status());
        b.frame(|_, _| {});
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(b.online.stats.back, "back in the body that waited");
    assert!(b.me.pilot.position.distance(was) < 0.5, "where it was: {:.2} m off", b.me.pilot.position.distance(was));
    let t0 = now();
    while now() - t0 < 1.5 {
        b.frame(walk);
        std::thread::sleep(Duration::from_millis(3));
    }
    assert_eq!(b.online.stats.corrections, 0, "{:?}", b.online.stats);
    while let Ok(n) = local.notes.try_recv() {
        println!("servidor: {n}");
    }
    local.stop();
    let _ = std::fs::remove_dir_all(&dir);
}
