//! The real thing, briefly: a server and two clients over UDP on this machine (a port the system
//! picks, so nothing clashes), for about a second of real time.
use lunar_net::{Client, Event, Server, ServerConfig, Status, Udp, now};
use std::time::Duration;

#[test]
fn a_server_and_two_clients_talk_over_udp() {
    let mut socket = Udp::bind("127.0.0.1:0").expect("a socket");
    let addr = socket.local().expect("an address").to_string();
    let mut server = Server::new(ServerConfig::default());
    let mut clients = [Client::connect(&addr, "Ana", "V36", 2).expect("a client"), Client::connect(&addr, "Luis", "V36", 2).expect("a client")];
    let (mut events, mut game) = ([Vec::new(), Vec::new()], Vec::new());
    let started = now();
    let mut said = false;
    while now() - started < 1.5 {
        let t = now();
        server.update(t, &mut socket);
        game.extend(server.take_game());
        for (i, c) in clients.iter_mut().enumerate() {
            // (what each step asks, again and again, and the newest is what counts)
            c.send_quick(&[i as u8, (t * 10.0) as u8]);
            c.update(t);
            events[i].extend(c.events());
        }
        if !said && clients.iter().all(|c| c.synced()) {
            said = true;
            clients[1].send_game(&[5, 0, 12, 1]);
            clients[0].chat("¿se me oye?");
            let to = clients[0].id().expect("connected");
            assert!(server.send_game(to, true, &[7, 7]));
            server.send_game(to, false, &[8]);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let ids: Vec<u32> = clients.iter().map(|c| c.id().expect("connected")).collect();
    for (i, c) in clients.iter().enumerate() {
        let Status::Connected { players: 2, ping_ms, .. } = c.status() else { panic!("client {i}: {:?}", c.status()) };
        assert!(ping_ms < 100.0, "on one machine: {ping_ms}");
        assert_eq!(c.name(ids[1 - i]), Some(["Luis", "Ana"][i]));
        assert!(events[i].contains(&Event::Chat { from: Some(ids[0]), text: "¿se me oye?".to_string() }));
    }
    // (what the games said reached the game in the server, unread, from whom it came)
    assert!(game.iter().any(|m| m.from == ids[1] && m.reliable && m.data == [5, 0, 12, 1]));
    assert!(game.iter().any(|m| m.from == ids[0] && !m.reliable && m.data[0] == 0) && game.iter().any(|m| m.from == ids[1] && !m.reliable && m.data[0] == 1));
    // (and what it said back, to the one it said it to alone)
    assert!(events[0].contains(&Event::Game { reliable: true, data: vec![7, 7] }) && events[0].contains(&Event::Game { reliable: false, data: vec![8] }));
    assert!(!events[1].iter().any(|e| matches!(e, Event::Game { .. })), "what is said to one is not said to the other");
    let stats = server.stats();
    println!("udp, 1.5 s: server took {} datagrams ({} bytes) and sent {} ({} bytes); ping {:?}", stats.datagrams_in, stats.bytes_in, stats.datagrams_out, stats.bytes_out, clients.iter().map(|c| c.status()).collect::<Vec<_>>());
    assert_eq!((stats.garbled, stats.strays), (0, 0));
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

/// Not a test of ours: connections for a server that is already running (the real program), to
/// see what they cost. `LUNA_SERVIDOR=127.0.0.1:47600 LUNA_SEGUNDOS=30 cargo test --test udp -- --ignored --nocapture load`
/// (they are not let into its game: another build; for players that play, `lunar_play::bots`).
#[test]
#[ignore]
fn load_for_a_running_server() {
    let Ok(addr) = std::env::var("LUNA_SERVIDOR") else { return };
    let seconds: f64 = std::env::var("LUNA_SEGUNDOS").ok().and_then(|s| s.parse().ok()).unwrap_or(20.0);
    let mut clients: Vec<Client> = (0..8).map(|i| Client::connect(&addr, &format!("Carga {i}"), "carga", 4).expect("a client")).collect();
    let started = now();
    while now() - started < seconds {
        let t = now();
        for c in clients.iter_mut() {
            c.send_quick(&[0; 64]);
            c.update(t);
            c.events().for_each(drop);
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    for (i, c) in clients.iter().enumerate() {
        let s = c.stats();
        println!("client {i}: {:?}; up {:.0} B/s, down {:.0} B/s (with IP+UDP)", c.status(), (s.sent_bytes + 28 * s.sent_datagrams) as f64 / seconds, (s.recv_bytes + 28 * s.recv_datagrams) as f64 / seconds);
    }
}
