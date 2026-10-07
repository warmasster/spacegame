//! What the server's step costs with many players (`docs/PLAN_AUTORITATIVO.md` fase 3): 16 bots
//! walking and flying with the pack round the site, through the real network code, for some
//! seconds of the game; the time of each of the server's steps measured.
use lunar_net::{Client, Conditions, MemoryNet, Server, ServerConfig, ServerEvent};
use lunar_play::{
    bots::{Bot, Script},
    defs::Defs,
    game::{Game, STEP},
    host::{Host, HostConfig},
};
use std::time::Instant;

#[test]
fn sixteen_players_in_the_scenario_the_step_stays_short() {
    let defs = Defs::load(&lunar_play::root().join("assets/defs")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    let game = Game::new_apart(&defs, &lunar_play::root().join("assets/defs"), 256, |_| true).unwrap();
    let mut host = Host::new(game, defs.scenario.player, HostConfig::default());
    let net = MemoryNet::new(3);
    net.conditions(Conditions { delay: 0.03, jitter: 0.01, loss: 0.01, ..Conditions::default() });
    let mut link = net.endpoint();
    let addr = link.addr();
    let mut server = Server::new(ServerConfig { game: Some((lunar_play::net::BUILD.to_string(), defs.fingerprint)), ..ServerConfig::default() });
    let scripts = [Script::Walk, Script::Jets, Script::Walk, Script::Stand];
    let mut bots: Vec<Bot> = (0..16).map(|k| Bot::new(Client::with_transport(Box::new(net.endpoint()), addr, &format!("bot{k}"), lunar_play::net::BUILD, defs.fingerprint), scripts[k % 4], k as u64)).collect();
    let (mut now, dt) = (10.0, 1.0 / 60.0);
    let mut times = Vec::new();
    let mut due = 0.0;
    for frame in 0..(12.0 / dt) as usize {
        now += dt;
        net.set_time(now);
        server.update(now, &mut link);
        for e in server.events() {
            match e {
                ServerEvent::Joined { id, .. } => host.join(id),
                ServerEvent::Left { id, .. } => host.leave(id),
                _ => {}
            }
        }
        for m in server.take_game().collect::<Vec<_>>() {
            host.take(m.from, &m.data);
        }
        due += dt;
        while due >= STEP {
            due -= STEP;
            let t = Instant::now();
            host.step();
            // (the first two seconds, everyone coming in, not counted)
            if frame > 120 {
                times.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            host.send(|to, reliable, bytes| server.send_game(to, reliable, bytes));
        }
        server.update(now, &mut link);
        for b in &mut bots {
            b.update(now, dt);
        }
    }
    assert_eq!(host.ids().count(), 16);
    times.sort_by(f64::total_cmp);
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    let s = host.stats;
    println!("16 bots: step median {:.2} ms, p95 {:.2} ms, max {:.2} ms; {:?}", at(0.5), at(0.95), at(1.0), s);
    // (built to be measured, the plan's bound; in the tests' build, a loose one)
    let bound = if cfg!(debug_assertions) { 40.0 } else { 8.0 };
    assert!(at(0.95) < bound, "p95 {:.2} ms", at(0.95));
    // every one of them was stepped with what it asked, nearly always in time
    assert!(s.guessed < s.steps * 16 / 50, "{s:?}");
}
