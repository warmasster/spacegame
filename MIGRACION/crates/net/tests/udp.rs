//! The real thing, briefly: a server and two clients over UDP on this machine (a port the system
//! picks, so nothing clashes), for about a second of real time.
mod common;

use common::{ship, walker};
use lunar_net::{Client, Event, PlayerState, RigidState, Server, ServerConfig, Status, Udp, flag, key, now};
use std::time::Duration;

#[test]
fn a_server_and_two_clients_talk_over_udp() {
    let mut socket = Udp::bind("127.0.0.1:0").expect("a socket");
    let addr = socket.local().expect("an address").to_string();
    let mut server = Server::new(ServerConfig::default());
    let mut clients = [Client::connect(&addr, "Ana", "V36", 2).expect("a client"), Client::connect(&addr, "Luis", "V36", 2).expect("a client")];
    let (mut events, mut seen) = ([Vec::new(), Vec::new()], Vec::new());
    let started = now();
    let mut said = false;
    while now() - started < 1.5 {
        let t = now();
        server.update(t, &mut socket);
        for (i, c) in clients.iter_mut().enumerate() {
            c.set_player(&PlayerState { pos: walker(i as f64).pos + glam::DVec3::Y * (t - started), ..walker(i as f64) });
            // Ana came first: the ships are hers, and Luis's word about them is not even sent.
            c.set_rigid(&RigidState { vel: glam::Vec3::ZERO, spin: glam::Vec3::ZERO, ..ship(1, i as f64 * 100.0, 8) });
            c.update(t);
            events[i].extend(c.events());
        }
        if !said && clients.iter().all(|c| c.synced()) {
            said = true;
            clients[1].tell(&[5, 0, 12, 1]);
            clients[1].tell_to(1, &[9]);
            clients[1].claim(key::seat(1, 0));
            clients[0].chat("¿se me oye?");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let ids: Vec<u32> = clients.iter().map(|c| c.id().expect("connected")).collect();
    for (i, c) in clients.iter().enumerate() {
        let Status::Connected { players: 2, ping_ms, .. } = c.status() else { panic!("client {i}: {:?}", c.status()) };
        assert!(ping_ms < 100.0, "on one machine: {ping_ms}");
        c.players(now(), &mut seen);
        assert_eq!(seen.len(), 1);
        let (id, state) = &seen[0];
        assert_eq!(*id, ids[1 - i]);
        assert!((state.pos - walker((1 - i) as f64).pos).length() < 2.0 && state.flags == flag::GROUNDED, "the other one, somewhere along its walk");
        assert_eq!(c.name(ids[1 - i]), Some(["Luis", "Ana"][i]));
        assert!(events[i].contains(&Event::Chat { from: Some(ids[0]), text: "¿se me oye?".to_string() }));
    }
    assert!(events[0].contains(&Event::Told { by: ids[1], data: vec![5, 0, 12, 1] }) && events[0].contains(&Event::Direct { by: ids[1], data: vec![9] }));
    assert!(!events[1].iter().any(|e| matches!(e, Event::Told { .. } | Event::Direct { .. })), "what one tells is not echoed");
    assert!(events.iter().all(|e| e.contains(&Event::Owner { key: key::seat(1, 0), player: Some(ids[1]) })), "everyone knows who sits where");
    assert!(clients[0].owns_thing(1) && !clients[1].owns_thing(1));
    let mut got = RigidState::default();
    assert!(clients[1].rigid_into(1, now(), &mut got), "the holder's ship");
    assert!((got.pos - ship(1, 0.0, 8).pos).length() < 0.001 && got.joints.len() == 8);
    let stats = server.stats();
    println!("udp, 1.5 s: server took {} datagrams ({} bytes) and sent {} ({} bytes); ping {:?}", stats.datagrams_in, stats.bytes_in, stats.datagrams_out, stats.bytes_out, clients.iter().map(|c| c.status()).collect::<Vec<_>>());
    assert_eq!((stats.garbled, stats.strays, stats.foreign), (0, 0, 0));
    // A goodbye over the real network too.
    clients[1].close();
    let until = now() + 0.3;
    while now() < until {
        server.update(now(), &mut socket);
        clients[0].update(now());
        events[0].extend(clients[0].events());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(events[0].contains(&Event::Left { id: ids[1], name: "Luis".to_string() }));
    assert_eq!(server.player_count(), 1);
}

#[test]
fn connecting_to_nonsense_fails_at_once_and_to_nobody_fails_in_time() {
    assert!(Client::connect("esto no es una dirección", "Ana", "V36", 2).is_err());
    // A port of this machine where nobody listens: connecting starts, and gives up by itself.
    let free = Udp::bind("127.0.0.1:0").expect("a socket").local().expect("an address");
    let mut c = Client::connect(&free.to_string(), "Ana", "V36", 2).expect("a client");
    // Its own clock, fast-forwarded: no need to wait five real seconds.
    for step in 0..400 {
        c.update(1000.0 + step as f64 * 0.016);
    }
    assert_eq!(c.status(), Status::Failed("el servidor no responde".to_string()));
}

/// Not a test of ours: a load for a server that is already running (the real program), to see
/// what it costs. `LUNA_SERVIDOR=127.0.0.1:47600 LUNA_SEGUNDOS=30 cargo test --test udp -- --ignored --nocapture load`
#[test]
#[ignore]
fn load_for_a_running_server() {
    let Ok(addr) = std::env::var("LUNA_SERVIDOR") else { return };
    let seconds: f64 = std::env::var("LUNA_SEGUNDOS").ok().and_then(|s| s.parse().ok()).unwrap_or(20.0);
    let mut clients: Vec<Client> = (0..8).map(|i| Client::connect(&addr, &format!("Carga {i}"), "carga", 4).expect("a client")).collect();
    let started = now();
    while now() - started < seconds {
        let t = now();
        for (i, c) in clients.iter_mut().enumerate() {
            c.set_player(&PlayerState { pos: walker(i as f64).pos + glam::DVec3::new(0.3, 1.5, 0.1) * (t - started), yaw: (t * 0.7) as f32, ..walker(i as f64) });
            for s in 0..4 {
                let mut state = ship(s, s as f64 * 60.0, 20);
                state.pos += state.vel.as_dvec3() * (t - started);
                c.set_rigid(&state);
            }
            c.update(t);
            c.events().for_each(drop);
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    let mut seen = Vec::new();
    for (i, c) in clients.iter().enumerate() {
        c.players(now(), &mut seen);
        let s = c.stats();
        println!(
            "client {i}: {:?}, sees {} players and {} ships, drawn {:.0} ms ago; up {:.0} B/s, down {:.0} B/s (with IP+UDP)",
            c.status(),
            seen.len(),
            (0..4).filter(|s| c.rigid_into(*s, now(), &mut RigidState::default())).count(),
            c.delay() * 1000.0,
            (s.sent_bytes + 28 * s.sent_datagrams) as f64 / seconds,
            (s.recv_bytes + 28 * s.recv_datagrams) as f64 / seconds
        );
        assert!(matches!(c.status(), Status::Connected { players: 8, .. }));
        assert_eq!(seen.len(), 7);
    }
}
