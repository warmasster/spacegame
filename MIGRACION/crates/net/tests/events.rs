//! What the game tells (a hand on a control, a hit, a ship made in play: bytes of its own that the
//! server passes on unread) reaches everyone exactly once and in the same order, through a rough
//! network too; what is sent to one player reaches only them, however long it is; and whoever
//! comes later is told by the server who is here and who holds what.
mod common;

use common::{World, walker};
use lunar_net::{Conditions, Event, MAX_HINT, MAX_TELL, Status, key};

const ROUGH: Conditions = Conditions { loss: 0.25, duplicate: 0.10, delay: 0.02, jitter: 0.08 };

/// Thing number `n` told by player `by`: each one different, so a repeat or a swap shows.
fn said(by: usize, n: u32) -> Vec<u8> {
    let mut data = vec![by as u8, (n % 251) as u8];
    data.extend_from_slice(&n.to_le_bytes());
    data.extend((0..n % 23).map(|k| (k * 3 + by as u32) as u8));
    data
}

fn told(events: &[Event]) -> Vec<(u32, Vec<u8>)> {
    events.iter().filter_map(|e| if let Event::Told { by, data } = e { Some((*by, data.clone())) } else { None }).collect()
}

fn four(seed: u64) -> (World, Vec<u32>) {
    let mut w = World::plain(seed);
    let ids: Vec<u32> = (0..4)
        .map(|i| {
            let c = w.join(&format!("J{i}"));
            w.settle(c)
        })
        .collect();
    (0..4).for_each(|i| drop(w.events(i)));
    (w, ids)
}

#[test]
fn what_is_told_reaches_everyone_else_once_and_in_order() {
    let (mut w, ids) = four(1);
    w.conditions(ROUGH);
    // Everyone tells things at once, one each frame.
    let per = 150u32;
    let mut seen: Vec<Vec<(u32, Vec<u8>)>> = vec![Vec::new(); 4];
    for n in 0..per {
        for i in 0..4 {
            assert!(w.clients[i].tell(&said(i, n)));
        }
        w.step();
        (0..4).for_each(|i| seen[i].extend(told(&w.events(i))));
    }
    for _ in 0..600 {
        w.step();
        (0..4).for_each(|i| seen[i].extend(told(&w.events(i))));
    }
    for (i, mine) in seen.iter().enumerate() {
        assert_eq!(mine.len(), 3 * per as usize, "client {i} got each of the others' once, and none of its own");
        for j in (0..4).filter(|j| *j != i) {
            let from_j: Vec<_> = mine.iter().filter(|e| e.0 == ids[j]).map(|e| e.1.clone()).collect();
            let sent: Vec<_> = (0..per).map(|n| said(j, n)).collect();
            assert_eq!(from_j, sent, "client {i} got those of {j} in the order they were told, byte for byte");
        }
    }
    // Everyone saw the events of any two others in the same relative order: one history for all.
    let between = |i: usize, x: u32, y: u32| seen[i].iter().filter(|e| e.0 == x || e.0 == y).cloned().collect::<Vec<_>>();
    assert_eq!(between(0, ids[2], ids[3]), between(1, ids[2], ids[3]));
    assert_eq!(between(2, ids[0], ids[1]), between(3, ids[0], ids[1]));
    assert_eq!(w.server.stats().garbled, 0);
}

#[test]
fn what_is_told_to_all_comes_back_to_its_teller_in_its_place() {
    let (mut w, ids) = four(2);
    w.conditions(ROUGH);
    // Two players tell things that must happen in one order everywhere (who took a thing first).
    let mut seen: Vec<Vec<(u32, Vec<u8>)>> = vec![Vec::new(); 4];
    for n in 0..80 {
        assert!(w.clients[1].tell_all(&said(1, n)) && w.clients[2].tell_all(&said(2, n)));
        w.step();
        (0..4).for_each(|i| seen[i].extend(told(&w.events(i))));
    }
    for _ in 0..600 {
        w.step();
        (0..4).for_each(|i| seen[i].extend(told(&w.events(i))));
    }
    assert_eq!(seen[0].len(), 160);
    assert!((1..4).all(|i| seen[i] == seen[0]), "the tellers too got everything, in the one order");
    assert_eq!(seen[0].iter().filter(|e| e.0 == ids[1]).count(), 80);
}

#[test]
fn what_is_sent_to_one_player_reaches_only_them() {
    let (mut w, ids) = four(3);
    w.conditions(ROUGH);
    for n in 0..40 {
        assert!(w.clients[0].tell_to(ids[2], &said(0, n)));
        assert!(w.clients[3].tell_to(ids[2], &said(3, n)));
        // (to someone who is not there, and to oneself: one goes nowhere, the other comes back)
        assert!(w.clients[0].tell_to(999, &[1]));
        w.step();
    }
    assert!(w.clients[1].tell_to(ids[1], &[42]));
    w.run(6.0);
    let direct = |events: &[Event]| events.iter().filter_map(|e| if let Event::Direct { by, data } = e { Some((*by, data.clone())) } else { None }).collect::<Vec<_>>();
    let got = direct(&w.events(2));
    assert_eq!(got.len(), 80);
    for (who, id) in [(0usize, ids[0]), (3, ids[3])] {
        assert_eq!(got.iter().filter(|e| e.0 == id).map(|e| e.1.clone()).collect::<Vec<_>>(), (0..40).map(|n| said(who, n)).collect::<Vec<_>>());
    }
    assert!(w.events(0).is_empty() && w.events(3).is_empty());
    assert_eq!(w.events(1), [Event::Direct { by: ids[1], data: vec![42] }]);
}

#[test]
fn hints_reach_the_others_at_most_once_and_never_stale() {
    let (mut w, ids) = four(4);
    let hinted = |events: &[Event]| events.iter().filter_map(|e| if let Event::Hinted { by, data } = e { Some((*by, u32::from_le_bytes([data[0], data[1], data[2], data[3]]))) } else { None }).collect::<Vec<_>>();
    // On a good network every one arrives.
    for n in 0..50u32 {
        w.clients[0].hint(&n.to_le_bytes());
        w.step();
    }
    w.run(0.3);
    for i in 1..4 {
        assert_eq!(hinted(&w.events(i)), (0..50).map(|n| (ids[0], n)).collect::<Vec<_>>());
    }
    assert!(w.events(0).is_empty(), "not back to who hinted");
    // On a rough one some are lost; none comes twice, none after a newer one.
    w.conditions(ROUGH);
    let mut got = Vec::new();
    for n in 0..400u32 {
        w.clients[0].hint(&n.to_le_bytes());
        w.step();
        got.extend(hinted(&w.events(1)));
    }
    w.run(1.0);
    got.extend(hinted(&w.events(1)));
    assert!(got.len() > 120 && got.len() < 400, "{} of 400 arrived", got.len());
    assert!(got.windows(2).all(|p| p[0].1 < p[1].1), "in order, none twice");
    // One too long for a datagram is not sent at all.
    w.conditions(Conditions::default());
    w.clients[0].hint(&vec![0; MAX_HINT + 1]);
    w.clients[0].hint(&vec![7; MAX_HINT]);
    w.run(0.3);
    let events = w.events(2);
    assert_eq!(events.iter().filter(|e| matches!(e, Event::Hinted { data, .. } if data.len() != 4)).count(), 1);
    assert!(events.contains(&Event::Hinted { by: ids[0], data: vec![7; MAX_HINT] }));
}

#[test]
fn whoever_comes_later_is_told_who_is_here_and_who_holds_what() {
    let mut w = World::plain(5);
    let (a, b) = (w.join("A"), w.join("B"));
    let (ida, idb) = (w.settle(a), w.settle(b));
    // A game goes on: things are told, B flies a ship and sits in it, a crate moves.
    w.clients[b].claim(key::thing(5));
    w.clients[b].claim(key::seat(5, 0));
    w.clients[a].claim(key::thing(900));
    for n in 0..200u32 {
        w.clients[n as usize % 2].tell(&said(n as usize % 2, n));
        w.step_with(|i, c, t| {
            c.set_player(&walker(i as f64));
            c.set_rigid(&common::ship(if i == 1 { 5 } else { 900 }, t, 0));
        });
    }
    w.run(0.5);

    // C arrives, through a bad connection.
    w.conditions(ROUGH);
    let c = w.join("C");
    let idc = w.settle(c);
    let events = w.events(c);
    let synced = events.iter().position(|e| *e == Event::Synced).expect("synced");
    let mut holders: Vec<(u64, Option<u32>)> = events[..synced].iter().filter_map(|e| if let Event::Owner { key, player } = e { Some((*key, *player)) } else { None }).collect();
    holders.sort_unstable();
    assert_eq!(events[..2], [Event::Joined { id: ida, name: "A".to_string() }, Event::Joined { id: idb, name: "B".to_string() }]);
    assert_eq!(events[2], Event::Host { player: Some(ida) });
    assert_eq!(holders, [(key::thing(5), Some(idb)), (key::thing(900), Some(ida)), (key::seat(5, 0), Some(idb))]);
    assert_eq!(synced, 6, "who is here, the host, who holds what: nothing else is the server's to tell");
    assert!(told(&events).is_empty(), "what was told before it came is not said again: the game asks for the world as it is now");
    assert!(matches!(w.clients[c].status(), Status::Connected { players: 3, .. }));
    assert_eq!(idc, 3);
    assert!(!w.clients[c].owns_thing(5) && w.clients[c].owner(key::thing(5)) == Some(idb) && w.clients[c].owner(key::thing(6)) == Some(ida));
    // What happens from now on reaches C live, once.
    w.clients[a].tell(&[9, 9, 9]);
    w.run(2.0);
    assert_eq!(told(&w.events(c)), [(ida, vec![9, 9, 9])]);
    // And the states of what moves: the players, the ship B flies, the crate.
    w.conditions(Conditions::default());
    for _ in 0..60 {
        w.step_with(|i, c, t| {
            c.set_player(&walker(i as f64));
            c.set_rigid(&common::ship(if i == 1 { 5 } else { 900 }, t, 0));
        });
    }
    let mut seen = Vec::new();
    w.clients[c].players(w.now, &mut seen);
    assert_eq!(seen.len(), 2);
    let mut got = lunar_net::RigidState::default();
    assert!(w.clients[c].rigid_into(5, w.now, &mut got) && w.clients[c].rigid_into(900, w.now, &mut got));
}

#[test]
fn a_long_snapshot_reaches_one_player_whole() {
    // What a newcomer is sent by those who hold the world: hundreds of kilobytes, in pieces of the
    // longest thing that can be told, through a network that loses one datagram in ten.
    let mut w = World::plain(6);
    let (a, b) = (w.join("A"), w.join("B"));
    let (ida, idb) = (w.settle(a), w.settle(b));
    w.events(b);
    w.conditions(Conditions { loss: 0.1, duplicate: 0.0, delay: 0.02, jitter: 0.02 });
    let pieces: Vec<Vec<u8>> = (0..6u8).map(|k| (0..MAX_TELL - 100 * k as usize).map(|n| (n as u8).wrapping_mul(31).wrapping_add(k)).collect()).collect();
    let total: usize = pieces.iter().map(Vec::len).sum();
    let (sent_before, started) = (w.server.stats().bytes_out, w.now);
    for p in &pieces {
        assert!(w.clients[a].tell_to(idb, p));
    }
    let mut got = Vec::new();
    while got.len() < pieces.len() && w.now - started < 30.0 {
        w.step();
        got.extend(w.events(b).into_iter().filter_map(|e| if let Event::Direct { by, data } = e { Some((by, data)) } else { None }));
    }
    let resent: u64 = w.server.players().map(|p| p.channel.resent).sum::<u64>() + w.clients[a].stats().resent;
    println!(
        "{} kB sent to one player in {} pieces through 10 % loss: whole in {:.2} s; the server passed on {} kB in {} datagrams to them ({} pieces sent again on the way)",
        total / 1000,
        pieces.len(),
        w.now - started,
        (w.server.stats().bytes_out - sent_before) / 1000,
        w.clients[b].stats().recv_datagrams,
        resent
    );
    assert_eq!(got.len(), pieces.len(), "all of it arrived");
    assert!(got.iter().zip(&pieces).all(|(g, p)| g.0 == ida && g.1 == *p), "in order and byte for byte");
    assert!(w.now - started < 15.0);
    assert!((w.server.stats().bytes_out - sent_before) < total as u64 * 2, "little of it had to go twice");
    assert_eq!(w.clients[a].backlog(), 0);
}
