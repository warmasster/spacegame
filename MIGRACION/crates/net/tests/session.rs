//! Sessions: clients on one server see each other (states within their quantisation, names,
//! arrivals and departures); who may not come in is told why; who vanishes is noticed.
mod common;

use common::{BUILD, SCENARIO, World, on_the_moon};
use glam::Vec3;
use lunar_net::game::POS_UNITS;
use lunar_net::proto::{Datagram, VERSION};
use lunar_net::{Conditions, Event, MAX_TELL, PlayerState, ServerConfig, ServerEvent, Status, Transport, flag, key};

/// Client `i`'s player at time `t`: each walks its own line at its own speed.
fn player(i: usize, t: f64) -> PlayerState {
    let speed = 1.0 + i as f64 * 0.25;
    PlayerState {
        pos: on_the_moon(i as f64 * 10.0) + glam::DVec3::new(0.0, speed * (t - 100.0), 0.0),
        vel: Vec3::new(0.0, speed as f32, 0.0),
        yaw: 0.1 * i as f32,
        pitch: -0.05 * i as f32,
        flags: flag::GROUNDED | if i % 2 == 1 { flag::LAMP } else { 0 },
        tool: i as u8,
        ..PlayerState::default()
    }
}

fn everyone_sees_everyone(n: usize, seed: u64) {
    let mut w = World::plain(seed);
    let mut ids = Vec::new();
    for i in 0..n {
        let c = w.join(&format!("Jugador {i}"));
        ids.push(w.settle(c));
    }
    assert_eq!(ids, (1..=n as u32).collect::<Vec<_>>(), "ids in the order they came");
    for _ in 0..180 {
        w.step_with(|i, c, t| c.set_player(&player(i, t)));
    }
    let mut seen = Vec::new();
    for i in 0..n {
        assert!(matches!(w.clients[i].status(), Status::Connected { id, players, .. } if id == ids[i] && players == n), "{:?}", w.clients[i].status());
        w.clients[i].players(w.now, &mut seen);
        assert_eq!(seen.len(), n - 1, "client {i} sees the others and not itself");
        // The others are drawn a little in the past: where each was at that moment, within the wire's precision.
        let drawn = w.now - w.clients[i].delay() as f64;
        for (id, got) in &seen {
            let j = ids.iter().position(|x| x == id).expect("a known id");
            assert_ne!(j, i);
            let want = player(j, drawn);
            assert!((got.pos - want.pos).length() < 0.005, "client {i} sees {j} {} m off", (got.pos - want.pos).length());
            assert!((got.pos - want.pos).x.abs() <= 0.5 / POS_UNITS + 1e-9);
            assert!((got.yaw - want.yaw).abs() < 1e-4 && (got.pitch - want.pitch).abs() < 1e-4);
            assert!((got.vel - want.vel).length() < 0.005);
            assert_eq!((got.flags, got.tool, got.ride, got.seat), (want.flags, want.tool, None, None));
            assert_eq!(w.clients[i].name(*id), Some(format!("Jugador {j}").as_str()));
        }
        assert_eq!(w.clients[i].name(ids[i]), Some(format!("Jugador {i}").as_str()), "and knows its own name");
        // Everyone learnt of everyone else exactly once: those who were here (before `Synced`) and those who came after.
        let events = w.events(i);
        let joined: Vec<u32> = events.iter().filter_map(|e| if let Event::Joined { id, .. } = e { Some(*id) } else { None }).collect();
        let mut want: Vec<u32> = ids.iter().copied().filter(|x| *x != ids[i]).collect();
        assert_eq!(joined, want, "client {i}");
        want.truncate(i);
        let synced = events.iter().position(|e| *e == Event::Synced).expect("synced");
        let before: Vec<u32> = events[..synced].iter().filter_map(|e| if let Event::Joined { id, .. } = e { Some(*id) } else { None }).collect();
        assert_eq!(before, want, "those already here are told as the past");
    }
    assert_eq!(w.log.iter().filter(|e| matches!(e, ServerEvent::Joined { .. })).count(), n);
    assert_eq!(w.server.player_count(), n);

    // One leaves with a goodbye: the rest are told at once, by name.
    w.clients[n - 1].close();
    w.run(0.3);
    assert_eq!(w.clients[n - 1].status(), Status::Failed("desconectado".to_string()));
    for i in 0..n - 1 {
        let left: Vec<Event> = w.events(i).into_iter().filter(|e| matches!(e, Event::Left { .. })).collect();
        assert_eq!(left, [Event::Left { id: ids[n - 1], name: format!("Jugador {}", n - 1) }]);
        w.clients[i].players(w.now, &mut seen);
        assert_eq!(seen.len(), n - 2);
        assert_eq!(w.clients[i].name(ids[n - 1]), None);
    }
    assert!(w.log.contains(&ServerEvent::Left { id: ids[n - 1], name: format!("Jugador {}", n - 1), reason: "se ha ido".to_string(), players: n - 1 }));
    assert_eq!(w.server.stats().garbled, 0, "nothing the clients sent was nonsense");
    assert_eq!(w.server.stats().foreign, 0);
}

#[test]
fn two_clients_see_each_other() {
    everyone_sees_everyone(2, 1);
}

#[test]
fn eight_clients_see_each_other() {
    everyone_sees_everyone(8, 2);
}

#[test]
fn a_rough_network_still_gets_everyone_in_and_seen() {
    let mut w = World::plain(3);
    w.conditions(Conditions { loss: 0.2, duplicate: 0.1, delay: 0.03, jitter: 0.05 });
    for i in 0..4 {
        let c = w.join(&format!("J{i}"));
        w.settle(c);
    }
    for _ in 0..300 {
        w.step_with(|i, c, t| c.set_player(&player(i, t)));
    }
    let mut seen = Vec::new();
    for i in 0..4 {
        w.clients[i].players(w.now, &mut seen);
        assert_eq!(seen.len(), 3);
        let joined = w.events(i).iter().filter(|e| matches!(e, Event::Joined { .. })).count();
        assert_eq!(joined, 3, "each arrival told once, however many datagrams were lost or doubled");
    }
}

#[test]
fn a_wrong_build_a_wrong_scenario_and_a_wrong_protocol_are_refused_with_the_reason() {
    let mut w = World::plain(4);
    let first = w.join("Primera");
    w.settle(first);
    let build = w.join_as("Otra versión", "V35", SCENARIO);
    let ships = w.join_as("Otro escenario", BUILD, SCENARIO + 1);
    w.run(0.5);
    let Status::Failed(why) = w.clients[build].status() else { panic!("the wrong build got in") };
    assert_eq!(why, "versión del juego distinta: la partida es de la versión «V36» y la tuya es «V35»");
    let Status::Failed(why) = w.clients[ships].status() else { panic!("the wrong scenario got in") };
    assert_eq!(why, "escenario distinto: la partida empezó con otro mundo (el suyo es el 5ce0a210 y el tuyo el 5ce0a211); hay que entrar con el mismo escenario");
    assert!(w.events(build).is_empty() && w.events(ships).is_empty(), "who never got in gets no events");
    assert_eq!(w.server.player_count(), 1);
    assert_eq!(w.log.iter().filter(|e| matches!(e, ServerEvent::Refused { .. })).count(), 2, "each refusal logged once");
    // The first player never heard of them.
    assert!(!w.events(first).iter().any(|e| matches!(e, Event::Joined { .. })));

    // Another protocol version: its hello is answered with the reason, whatever else it says. A
    // game of V35 (protocol 1) is told to update; a later one, that the server is the old one.
    let mut stranger = w.net.endpoint();
    let mut buf = [0u8; 1200];
    for (version, salt, says) in [(1, 41, "protocolo de red distinto: el servidor habla la versión 2 y tu juego la 1 (tu juego es más antiguo que el servidor: actualiza el juego; la 1 es la del juego V35 y la 2 la del V36)"), (VERSION + 1, 42, "protocolo de red distinto: el servidor habla la versión 2 y tu juego la 3 (tu juego es más nuevo que el servidor: hay que actualizar el servidor; la 1 es la del juego V35 y la 2 la del V36)")] {
        let n = Datagram::Hello { version, salt, cookie: 0, scenario: SCENARIO, build: "V35", name: "Otro" }.encode(&mut buf);
        stranger.send(w.addr, &buf[..n]);
        w.run(0.1);
        let (_, n) = stranger.recv(&mut buf).expect("an answer");
        let Ok(Datagram::Refused { salt: back, reason }) = Datagram::decode(&buf[..n]) else { panic!("a refusal") };
        assert_eq!((back, reason), (salt, says));
    }
    assert_eq!(w.server.player_count(), 1);
}

#[test]
fn the_first_client_says_what_game_it_is_and_an_empty_server_forgets() {
    let mut w = World::plain(5);
    let a = w.join_as("A", "V40", 9);
    let id = w.settle(a);
    assert_eq!((w.server.host(), w.clients[a].host(), w.clients[a].hosting()), (Some(id), Some(id), true));
    // what it holds and what it tells of is the game's
    w.clients[a].claim(key::seat(3, 0));
    for _ in 0..30 {
        w.step_with(|_, c, t| c.set_rigid(&common::ship(3, t, 4)));
    }
    assert_eq!((w.server.key_count(), w.server.thing_count()), (1, 1));
    w.clients[a].close();
    w.run(0.2);
    assert_eq!((w.server.player_count(), w.server.key_count(), w.server.thing_count(), w.server.host()), (0, 0, 0, None));
    assert_eq!(w.log.last(), Some(&ServerEvent::Empty));
    // Another game altogether is welcome now, and nothing of the last one is in it.
    let b = w.join_as("B", "V41", 2);
    let id = w.settle(b);
    assert_eq!((w.server.host(), w.server.owner(key::thing(3)), w.server.owner(key::seat(3, 0))), (Some(id), Some(id), None));
    let late = w.join_as("C", "V41", 9);
    w.run(0.5);
    assert!(matches!(w.clients[late].status(), Status::Failed(why) if why.starts_with("escenario distinto")));
}

#[test]
fn a_full_server_refuses_politely() {
    let mut w = World::new(7, ServerConfig { max_players: 3, ..ServerConfig::default() });
    for i in 0..3 {
        let c = w.join(&format!("J{i}"));
        w.settle(c);
    }
    let late = w.join("Tarde");
    w.run(0.5);
    assert_eq!(w.clients[late].status(), Status::Failed("el servidor está lleno (3 de 3 jugadores)".to_string()));
    assert_eq!(w.server.player_count(), 3);
    // Someone leaves: there is room again.
    w.clients[0].close();
    w.run(0.2);
    let again = w.join("Tarde");
    assert_eq!(w.settle(again), 4);
    assert_eq!(w.server.player_count(), 3);
}

#[test]
fn a_client_that_vanishes_is_dropped_and_the_others_are_told() {
    let mut w = World::new(8, ServerConfig { timeout: 3.0, ..ServerConfig::default() });
    let (a, b, c) = (w.join("A"), w.join("B"), w.join("C"));
    let ids = [w.settle(a), w.settle(b), w.settle(c)];
    for _ in 0..60 {
        w.step_with(|i, c, t| c.set_player(&player(i, t)));
    }
    w.events(a);
    w.events(c);
    w.vanish(b);
    w.run(2.5);
    assert_eq!(w.server.player_count(), 3, "not yet: it may only be a bad moment");
    w.run(1.0);
    assert_eq!(w.server.player_count(), 2);
    assert!(w.log.contains(&ServerEvent::Left { id: ids[1], name: "B".to_string(), reason: "dejó de dar señal".to_string(), players: 2 }));
    for i in [a, c] {
        assert_eq!(w.events(i), [Event::Left { id: ids[1], name: "B".to_string() }]);
        let mut seen = Vec::new();
        w.clients[i].players(w.now, &mut seen);
        assert_eq!(seen.len(), 1);
    }
    // And the one that vanished, hearing nothing, gives the server up by itself.
    w.run(8.0);
    assert_eq!(w.clients[b].status(), Status::Failed("se perdió la conexión con el servidor".to_string()));
    assert_eq!(w.events(b).last(), Some(&Event::Disconnected { reason: "se perdió la conexión con el servidor".to_string() }));
}

#[test]
fn a_server_that_is_not_there_fails_the_connection() {
    let mut w = World::plain(9);
    let lonely = w.join("Solo");
    w.net.cut(w.addr, true);
    w.run(1.0);
    assert_eq!(w.clients[lonely].status(), Status::Connecting);
    w.run(4.5);
    assert_eq!(w.clients[lonely].status(), Status::Failed("el servidor no responde".to_string()));
    assert_eq!(w.clients[lonely].id(), None);
}

#[test]
fn names_are_cleaned_and_kept_apart() {
    let mut w = World::plain(10);
    let odd = w.join("  Ana\u{7}\n  María   de la O y de todos los Santos del cielo  ");
    let empty = w.join(" \t ");
    let same = w.join("ana maría de la o y de t");
    let ids = [w.settle(odd), w.settle(empty), w.settle(same)];
    assert_eq!(w.clients[odd].name(ids[0]), Some("Ana María de la O y de t"), "no control characters, single spaces, 24 characters");
    assert_eq!(w.clients[empty].name(ids[1]), Some("Jugador"));
    assert_eq!(w.clients[same].name(ids[2]), Some("ana maría de la o (3)"), "a name already taken gets its id");
    assert_eq!(w.clients[odd].name(ids[2]), Some("ana maría de la o (3)"));
}

#[test]
fn the_server_kicks_speaks_and_closes() {
    let mut w = World::plain(11);
    let (a, b) = (w.join("A"), w.join("B"));
    let ids = [w.settle(a), w.settle(b)];
    w.events(a);
    w.events(b);
    w.server.say("  Reinicio en 5 minutos \u{1b}[31m ");
    w.clients[a].chat("hola a todos");
    w.clients[b].chat(" \n ");
    w.run(0.3);
    let want = [Event::Chat { from: None, text: "Reinicio en 5 minutos [31m".to_string() }, Event::Chat { from: Some(ids[0]), text: "hola a todos".to_string() }];
    assert_eq!(w.events(a), want, "its own line comes back too, and the empty one went nowhere");
    assert_eq!(w.events(b), want);
    assert!(w.log.contains(&ServerEvent::Chat { id: ids[0], name: "A".to_string(), text: "hola a todos".to_string() }));

    assert!(!w.server.kick(99, ""));
    assert!(w.server.kick(ids[1], ""));
    w.run(0.3);
    assert_eq!(w.clients[b].status(), Status::Failed("expulsado por el servidor".to_string()));
    assert_eq!(w.events(b), [Event::Disconnected { reason: "expulsado por el servidor".to_string() }]);
    assert_eq!(w.events(a), [Event::Left { id: ids[1], name: "B".to_string() }]);

    w.server.close(&mut w.link);
    w.run(0.3);
    assert_eq!(w.clients[a].status(), Status::Failed("el servidor se ha cerrado".to_string()));
    assert_eq!(w.server.player_count(), 0);
}

#[test]
fn a_client_that_starts_over_from_the_same_address_replaces_its_old_self() {
    let mut w = World::plain(12);
    let (a, b) = (w.join("A"), w.join("B"));
    let ids = [w.settle(a), w.settle(b)];
    w.events(a);
    // The game of B crashes and is started again: same machine, same port, a new connection.
    // (Its cable is cut while it goes, so that not even its goodbye gets out.)
    w.vanish(b);
    let again = w.net.endpoint_at(w.addrs[b]).expect("the same address");
    w.clients[b] = lunar_net::Client::with_transport(Box::new(again), w.addr, "B", BUILD, SCENARIO);
    w.net.cut(w.addrs[b], false);
    assert_eq!(w.settle(b), 3);
    w.run(0.3);
    assert_eq!(w.server.player_count(), 2);
    assert_eq!(w.events(a), [Event::Left { id: ids[1], name: "B".to_string() }, Event::Joined { id: 3, name: "B".to_string() }]);
}

#[test]
fn garbage_thrown_at_a_server_changes_nothing() {
    let mut w = World::plain(13);
    let (a, b) = (w.join("A"), w.join("B"));
    let ids = [w.settle(a), w.settle(b)];
    w.events(a);
    w.events(b);
    let stranger = w.net.endpoint().addr();
    let mut dice = common::Dice(99);
    let mut buf = [0u8; 1400];
    for round in 0..20_000 {
        let n = 1 + dice.below(if round % 50 == 0 { 1400 } else { 60 }) as usize;
        dice.bytes(&mut buf[..n]);
        // Mostly with a first byte that names a kind of datagram, so it gets as far as it can.
        if round % 4 != 0 {
            buf[0] = 1 + dice.below(5) as u8;
        }
        // Some of them almost a hello: the magic is right and the rest is noise.
        if round % 7 == 0 && n > 8 {
            buf[..5].copy_from_slice(&[1, b'L', b'U', b'N', b'A']);
        }
        // And some a whole hello of our version from an address that is not the sender's own: it gets
        // a challenge it cannot answer, and nothing is kept for it.
        let n = if round % 11 == 0 { Datagram::Hello { version: VERSION, salt: round, cookie: dice.next(), scenario: SCENARIO, build: BUILD, name: "Nadie" }.encode(&mut buf) } else { n };
        w.net.inject(stranger, w.addr, &buf[..n]);
        if round % 200 == 0 {
            w.step_with(|i, c, t| c.set_player(&player(i, t)));
        }
    }
    // A goodbye in someone's name without their number is not theirs; nor are answers meant for clients.
    for (kind, from) in [(5u8, w.addrs[a]), (2, stranger), (3, stranger), (6, stranger)] {
        let n = match kind {
            5 => Datagram::Bye { salt: 0x0BAD_5A17, reason: "" }.encode(&mut buf),
            2 => Datagram::Welcome { salt: 1, id: 1, tick_hz: 20, name: "x", server: "y" }.encode(&mut buf),
            6 => Datagram::Challenge { salt: 1, cookie: 1 }.encode(&mut buf),
            _ => Datagram::Refused { salt: 1, reason: "no" }.encode(&mut buf),
        };
        w.net.inject(from, w.addr, &buf[..n]);
        // And the same at a client, as if from the server: a goodbye with the wrong number is nobody's.
        w.net.inject(w.addr, w.addrs[b], &buf[..n]);
    }
    for _ in 0..120 {
        w.step_with(|i, c, t| c.set_player(&player(i, t)));
    }
    let stats = w.server.stats();
    println!("20 000 garbage datagrams: {} taken for nonsense, {} from nobody we know; players still {}", stats.garbled, stats.strays, w.server.player_count());
    assert!(stats.garbled + stats.strays >= 18_000, "{stats:?}");
    // The hellos from addresses that were not the sender's own got a challenge each (13 bytes for their 300) and nothing more.
    assert!(stats.bytes_out < stats.bytes_in / 20, "what the server answers is far less than what it was sent: {stats:?}");
    assert!(w.log.iter().filter(|e| matches!(e, ServerEvent::Refused { .. })).count() < 200, "and its log is not filled with them");
    assert_eq!(w.server.player_count(), 2, "nobody came in by accident, nobody was thrown out");
    assert!(!w.log.iter().any(|e| matches!(e, ServerEvent::Left { .. })));
    let mut seen = Vec::new();
    for (i, other) in [(a, ids[1]), (b, ids[0])] {
        assert!(matches!(w.clients[i].status(), Status::Connected { players: 2, .. }), "{:?}", w.clients[i].status());
        w.clients[i].players(w.now, &mut seen);
        assert_eq!(seen.iter().map(|s| s.0).collect::<Vec<_>>(), [other]);
        assert!(w.events(i).is_empty(), "and the clients heard nothing of it");
    }
}

#[test]
fn texts_too_long_for_the_wire_are_cut_not_fatal() {
    let mut w = World::plain(14);
    // A build name and a player name far longer than the wire carries, with characters of several bytes.
    let long_build = "versión-ñandú-".repeat(20);
    let a = w.join_as(&"Ñ".repeat(500), &long_build, SCENARIO);
    let b = w.join_as("B", &long_build, SCENARIO);
    let ids = [w.settle(a), w.settle(b)];
    assert_eq!(w.clients[b].name(ids[0]), Some("Ñ".repeat(24).as_str()));
    w.clients[a].chat(&"é".repeat(5000));
    // What the game tells is not cut: the longest thing it may say arrives whole, and one longer is not taken.
    let long: Vec<u8> = (0..MAX_TELL).map(|k| (k * 7) as u8).collect();
    assert!(w.clients[a].tell(&long) && !w.clients[a].tell(&vec![0; MAX_TELL + 1]) && !w.clients[a].tell_to(ids[1], &vec![0; MAX_TELL + 1]));
    w.clients[a].hint(&vec![0; lunar_net::MAX_HINT + 1]);
    w.run(1.5);
    let events = w.events(b);
    assert!(events.contains(&Event::Chat { from: Some(ids[0]), text: "é".repeat(240) }));
    let Some(Event::Told { by, data }) = events.iter().find(|e| matches!(e, Event::Told { .. })) else { panic!("what was told") };
    assert!(*by == ids[0] && *data == long);
    assert!(!events.iter().any(|e| matches!(e, Event::Hinted { .. })));
    assert_eq!(w.server.stats().garbled, 0);
}
