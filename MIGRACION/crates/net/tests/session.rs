//! Sessions: clients on one server are told of each other (names, arrivals and departures) and
//! what each says reaches the game in the server, and what it says reaches each; who may not come
//! in is told why; who vanishes is noticed.
mod common;

use common::{BUILD, SCENARIO, World};
use lunar_net::proto::{Datagram, VERSION};
use lunar_net::{Conditions, Event, GameIn, MAX_TELL, ServerConfig, ServerEvent, Status, Transport};

/// What client `i`'s game says of a moment (as a game's commands do: often, the newest counts).
fn says(i: usize, t: f64) -> [u8; 3] {
    [i as u8, (t * 10.0) as u8, 0xAB]
}

fn everyone_sees_everyone(n: usize, seed: u64) {
    let mut w = World::plain(seed);
    let mut ids = Vec::new();
    for i in 0..n {
        let c = w.join(&format!("Jugador {i}"));
        ids.push(w.settle(c));
    }
    assert_eq!(ids, (1..=n as u32).collect::<Vec<_>>(), "ids in the order they came");
    let mut game: Vec<GameIn> = Vec::new();
    for k in 0..180 {
        w.step_with(|i, c, t| {
            c.send_quick(&says(i, t));
            if k == 10 {
                c.send_game(&[i as u8; 5]);
            }
        });
        game.extend(w.server.take_game());
        // (and the game in the server, to each its own)
        if k == 20 {
            for (i, id) in ids.iter().enumerate() {
                assert!(w.server.send_game(*id, true, &[0xC0, i as u8]));
            }
        }
    }
    for i in 0..n {
        assert!(matches!(w.clients[i].status(), Status::Connected { id, players, .. } if id == ids[i] && players == n), "{:?}", w.clients[i].status());
        assert_eq!(w.clients[i].peers().count(), n - 1, "client {i} knows of the others and not of itself");
        for (j, id) in ids.iter().enumerate().filter(|(j, _)| *j != i) {
            assert_eq!(w.clients[i].name(*id), Some(format!("Jugador {j}").as_str()));
        }
        // what its game said came to the game in the server, from it: reliably once, the rest as it went
        assert_eq!(game.iter().filter(|m| m.from == ids[i] && m.reliable).map(|m| m.data.clone()).collect::<Vec<_>>(), [vec![i as u8; 5]]);
        assert!(game.iter().filter(|m| m.from == ids[i] && !m.reliable).count() > 150, "nearly every quick one, on a clean network");
        assert!(game.iter().filter(|m| m.from == ids[i]).all(|m| m.data[0] == i as u8));
        assert_eq!(w.clients[i].name(ids[i]), Some(format!("Jugador {i}").as_str()), "and knows its own name");
        // Everyone learnt of everyone else exactly once: those who were here (before `Synced`) and those who came after.
        let events = w.events(i);
        assert_eq!(events.iter().filter(|e| matches!(e, Event::Game { .. })).cloned().collect::<Vec<_>>(), [Event::Game { reliable: true, data: vec![0xC0, i as u8] }], "what the game in the server said to it alone");
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
        assert_eq!(w.clients[i].peers().count(), n - 2);
        assert_eq!(w.clients[i].name(ids[n - 1]), None);
    }
    assert!(w.log.contains(&ServerEvent::Left { id: ids[n - 1], name: format!("Jugador {}", n - 1), reason: "se ha ido".to_string(), players: n - 1 }));
    assert_eq!(w.server.stats().garbled, 0, "nothing the clients sent was nonsense");
}

#[test]
fn two_clients_know_of_each_other() {
    everyone_sees_everyone(2, 1);
}

#[test]
fn eight_clients_know_of_each_other() {
    everyone_sees_everyone(8, 2);
}

#[test]
fn a_rough_network_still_gets_everyone_in_and_heard() {
    let mut w = World::plain(3);
    w.conditions(Conditions { loss: 0.2, duplicate: 0.1, delay: 0.03, jitter: 0.05 });
    for i in 0..4 {
        let c = w.join(&format!("J{i}"));
        w.settle(c);
    }
    let mut game: Vec<GameIn> = Vec::new();
    for k in 0..300 {
        w.step_with(|i, c, t| {
            c.send_quick(&says(i, t));
            if k % 30 == 0 {
                c.send_game(&[i as u8, (k / 30) as u8]);
            }
        });
        game.extend(w.server.take_game());
    }
    w.run(2.0);
    game.extend(w.server.take_game());
    for i in 0..4 {
        assert_eq!(w.clients[i].peers().count(), 3);
        // (what goes reliably, all of it and in order, whatever was lost or doubled)
        let id = w.clients[i].id().unwrap();
        let sure: Vec<u8> = game.iter().filter(|m| m.from == id && m.reliable).map(|m| m.data[1]).collect();
        assert_eq!(sure, (0..10).collect::<Vec<u8>>(), "client {i}");
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
    let tail = "la 1 es la del juego V35, la 2 la del V36, la 3 la del V41, la del servidor que tiene la partida, y la 4 la de la conexión cifrada";
    for (version, salt, says) in [(1, 41, format!("protocolo de red distinto: el servidor habla la versión 4 y tu juego la 1 (tu juego es más antiguo que el servidor: actualiza el juego; {tail})")), (VERSION + 1, 42, format!("protocolo de red distinto: el servidor habla la versión 4 y tu juego la 5 (tu juego es más nuevo que el servidor: hay que actualizar el servidor; {tail})"))] {
        let n = Datagram::Hello { version, salt, cookie: 0, key: [0x33; 32], scenario: SCENARIO, build: "V35", name: "Otro" }.encode(&mut buf);
        stranger.send(w.addr, &buf[..n]);
        w.run(0.1);
        let (_, n) = stranger.recv(&mut buf).expect("an answer");
        let Ok(Datagram::Refused { salt: back, reason }) = Datagram::decode(&buf[..n]) else { panic!("a refusal") };
        assert_eq!((back, reason), (salt, says.as_str()));
    }
    assert_eq!(w.server.player_count(), 1);
}

#[test]
fn the_first_client_says_what_game_it_is_and_an_empty_server_forgets() {
    let mut w = World::plain(5);
    let a = w.join_as("A", "V40", 9);
    w.settle(a);
    w.clients[a].close();
    w.run(0.2);
    assert_eq!(w.server.player_count(), 0);
    assert_eq!(w.log.last(), Some(&ServerEvent::Empty));
    // Another game altogether is welcome now (a server that does not say which game it has).
    let b = w.join_as("B", "V41", 2);
    w.settle(b);
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
        w.step_with(|i, c, t| c.send_quick(&says(i, t)));
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
        assert_eq!(w.clients[i].peers().count(), 1);
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
        let n = if round % 11 == 0 { Datagram::Hello { version: VERSION, salt: round, cookie: dice.next(), key: [0x33; 32], scenario: SCENARIO, build: BUILD, name: "Nadie" }.encode(&mut buf) } else { n };
        w.net.inject(stranger, w.addr, &buf[..n]);
        if round % 200 == 0 {
            w.step_with(|i, c, t| c.send_quick(&says(i, t)));
        }
    }
    // A goodbye in someone's name without their number is not theirs; nor are answers meant for clients.
    for (kind, from) in [(5u8, w.addrs[a]), (2, stranger), (3, stranger), (6, stranger)] {
        let n = match kind {
            5 => Datagram::Bye { salt: 0x0BAD_5A17, reason: "" }.encode(&mut buf),
            2 => Datagram::Welcome { salt: 1, id: 1, key: [0x44; 32], name: "x", server: "y" }.encode(&mut buf),
            6 => Datagram::Challenge { salt: 1, cookie: 1 }.encode(&mut buf),
            _ => Datagram::Refused { salt: 1, reason: "no" }.encode(&mut buf),
        };
        w.net.inject(from, w.addr, &buf[..n]);
        // And the same at a client, as if from the server: a goodbye with the wrong number is nobody's.
        w.net.inject(w.addr, w.addrs[b], &buf[..n]);
    }
    for _ in 0..120 {
        w.step_with(|i, c, t| c.send_quick(&says(i, t)));
    }
    let stats = w.server.stats();
    println!("20 000 garbage datagrams: {} taken for nonsense, {} from nobody we know; players still {}", stats.garbled, stats.strays, w.server.player_count());
    assert!(stats.garbled + stats.strays >= 18_000, "{stats:?}");
    // The hellos from addresses that were not the sender's own got a challenge each (13 bytes for their 300) and nothing more.
    assert!(stats.bytes_out < stats.bytes_in / 20, "what the server answers is far less than what it was sent: {stats:?}");
    assert!(w.log.iter().filter(|e| matches!(e, ServerEvent::Refused { .. })).count() < 200, "and its log is not filled with them");
    assert_eq!(w.server.player_count(), 2, "nobody came in by accident, nobody was thrown out");
    assert!(!w.log.iter().any(|e| matches!(e, ServerEvent::Left { .. })));
    for (i, other) in [(a, ids[1]), (b, ids[0])] {
        assert!(matches!(w.clients[i].status(), Status::Connected { players: 2, .. }), "{:?}", w.clients[i].status());
        assert_eq!(w.clients[i].peers().collect::<Vec<_>>(), [other]);
        assert!(w.events(i).is_empty(), "and the clients heard nothing of it");
    }
    // (what the games said still came, and only from them)
    assert!(w.server.take_game().all(|m| ids.contains(&m.from)));
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
    // What the game says is not cut: the longest thing it may say arrives whole, and one longer is not taken.
    let long: Vec<u8> = (0..MAX_TELL).map(|k| (k * 7) as u8).collect();
    assert!(w.clients[a].send_game(&long) && !w.clients[a].send_game(&vec![0; MAX_TELL + 1]));
    w.clients[a].send_quick(&vec![0; lunar_net::MAX_HINT + 1]);
    assert!(w.server.send_game(ids[1], true, &long) && !w.server.send_game(ids[1], true, &vec![0; MAX_TELL + 1]));
    w.run(1.5);
    let events = w.events(b);
    assert!(events.contains(&Event::Chat { from: Some(ids[0]), text: "é".repeat(240) }));
    assert!(events.contains(&Event::Game { reliable: true, data: long.clone() }), "the longest the server may say, whole");
    let game: Vec<GameIn> = w.server.take_game().collect();
    assert_eq!(game, [GameIn { from: ids[0], reliable: true, data: long }], "the longest a game may say, whole, and nothing longer");
    assert_eq!(w.server.stats().garbled, 0);
}

/// A machine's way out whose address can change under it (a router that gives it another one).
struct Movable(std::sync::Arc<std::sync::Mutex<lunar_net::Memory>>);

impl Transport for Movable {
    fn send(&mut self, to: lunar_net::Addr, data: &[u8]) {
        self.0.lock().unwrap().send(to, data);
    }
    fn recv(&mut self, buf: &mut [u8]) -> Option<(lunar_net::Addr, usize)> {
        self.0.lock().unwrap().recv(buf)
    }
}

#[test]
fn a_session_whose_address_changes_goes_on_from_the_new_one_and_nobody_else_can_take_it() {
    let mut w = World::plain(15);
    let a = w.join("A");
    let ida = w.settle(a);
    // B's way out, to be moved
    let way = std::sync::Arc::new(std::sync::Mutex::new(w.net.endpoint()));
    w.clients.push(lunar_net::Client::with_transport(Box::new(Movable(way.clone())), w.addr, "B", BUILD, SCENARIO));
    w.addrs.push(way.lock().unwrap().addr());
    let b = w.clients.len() - 1;
    let idb = w.settle(b);
    w.events(a);
    // (the router gives B another address: what it sends comes from there now, what is sent to the
    // old one is lost)
    let new = w.net.endpoint();
    let (old, moved) = (way.lock().unwrap().addr(), new.addr());
    *way.lock().unwrap() = new;
    w.net.cut(old, true);
    for k in 0..120 {
        w.step_with(|i, c, t| {
            c.send_quick(&says(i, t));
            if i == 1 && k == 10 {
                c.send_game(&[0xB0]);
            }
        });
    }
    assert!(w.log.contains(&ServerEvent::Moved { id: idb, name: "B".to_string(), addr: moved }), "{:?}", w.log);
    assert!(matches!(w.clients[b].status(), Status::Connected { .. }), "B is still in: {:?}", w.clients[b].status());
    assert!(w.server.take_game().any(|m| m.from == idb && m.data == [0xB0]), "and what it says reaches the game");
    assert_eq!(w.server.player_count(), 2);
    assert!(w.events(a).iter().all(|e| !matches!(e, Event::Left { .. })), "nobody left");
    // (and someone else at yet another address, saying it is A, is nobody)
    let stranger = w.net.endpoint().addr();
    let mut buf = [0u8; 64];
    for k in 0..40 {
        buf.iter_mut().enumerate().for_each(|(i, x)| *x = (i * 31 + k) as u8);
        buf[0] = 4;
        w.net.inject(stranger, w.addr, &buf);
    }
    w.run(0.5);
    assert!(!w.log.iter().any(|e| matches!(e, ServerEvent::Moved { id, .. } if *id == ida)), "A stays where it is");
    assert!(matches!(w.clients[a].status(), Status::Connected { .. }));
}

#[test]
fn one_who_floods_does_not_crowd_out_the_others() {
    let mut w = World::plain(16);
    let (a, b) = (w.join("A"), w.join("B"));
    let ids = [w.settle(a), w.settle(b)];
    w.server.take_game().for_each(drop);
    let mut honest = 0;
    for k in 0..60 {
        // (B says thousands of things a frame; A, one, as a game does)
        w.step_with(|i, c, _| {
            if i == 1 {
                for _ in 0..3000 {
                    c.send_quick(&[0xEE; 8]);
                }
            } else {
                c.send_game(&[0xA0, k]);
            }
        });
        let got: Vec<GameIn> = w.server.take_game().collect();
        assert!(got.iter().filter(|m| m.from == ids[1]).count() <= 512, "what one may put in at once");
        honest += got.iter().filter(|m| m.from == ids[0]).count();
    }
    w.run(0.5);
    honest += w.server.take_game().filter(|m| m.from == ids[0]).count();
    assert_eq!(honest, 60, "everything the honest one said came");
    assert!(w.server.stats().flooded > 0);
    assert_eq!(w.server.player_count(), 2);
}
