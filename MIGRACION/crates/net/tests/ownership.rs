//! Who holds what: a thing is the host's while nobody asks for it, and of the first who asked
//! while someone does; a seat is nobody's until someone asks; and only the holder's word about a
//! thing is passed on. The server knows nothing of what a thing is.
mod common;

use common::{BUILD, SCENARIO, World, angle_between, cargo, ship, walker};
use glam::Vec3;
use lunar_net::proto::states::{Whose, begin_up, up_entry};
use lunar_net::proto::{Datagram, Msg, VERSION, lead};
use lunar_net::{Channel, Event, Frame, Inbox, PlayerState, RigidState, ServerConfig, Transport, Writer, key};

fn owners(events: &[Event]) -> Vec<(u64, Option<u32>)> {
    events.iter().filter_map(|e| if let Event::Owner { key, player } = e { Some((*key, *player)) } else { None }).collect()
}

/// Three players in, the events of joining already drained. Returns their ids.
fn three(seed: u64) -> (World, [u32; 3]) {
    let mut w = World::plain(seed);
    let (a, b, c) = (w.join("A"), w.join("B"), w.join("C"));
    let ids = [w.settle(a), w.settle(b), w.settle(c)];
    w.run(0.2);
    for i in 0..3 {
        w.events(i);
    }
    (w, ids)
}

#[test]
fn the_host_holds_the_things_nobody_asks_for() {
    let mut w = World::plain(1);
    let a = w.join("A");
    let ida = w.settle(a);
    let events = w.events(a);
    assert_eq!(events, [Event::Host { player: Some(ida) }, Event::Synced], "the first to come is told it is the host, and that is all there is to tell");
    let b = w.join("B");
    let idb = w.settle(b);
    assert_eq!(w.events(b), [Event::Joined { id: ida, name: "A".to_string() }, Event::Host { player: Some(ida) }, Event::Synced], "who comes later is told who is here and who the host is");
    assert_eq!(w.events(a), [Event::Joined { id: idb, name: "B".to_string() }], "and nothing changed for the host");
    for id in [0u64, 3, 77, (5 << 20) | 9] {
        let k = key::thing(id);
        assert!(w.clients[a].owns_thing(id) && !w.clients[b].owns_thing(id));
        assert_eq!((w.clients[a].owner(k), w.clients[b].owner(k), w.server.owner(k)), (Some(ida), Some(ida), Some(ida)));
        // a seat nobody asked for is nobody's
        let s = key::seat(id, 0);
        assert_eq!((w.clients[a].owner(s), w.clients[b].owner(s), w.server.owner(s)), (None, None, None));
    }
    assert!(w.clients[a].hosting() && !w.clients[b].hosting());
    assert_eq!((w.server.key_count(), w.server.host()), (0, Some(ida)), "nothing is kept for what nobody asks for");
}

#[test]
fn asking_for_a_thing_takes_it_and_letting_go_gives_it_back() {
    let (mut w, ids) = three(2);
    let k = key::thing(2);
    // B sits at the controls of ship 2: it asks for it.
    w.clients[1].claim(k);
    w.clients[1].claim(k);
    w.run(0.3);
    assert_eq!(w.server.owner(k), Some(ids[1]));
    for i in 0..3 {
        assert_eq!(owners(&w.events(i)), [(k, Some(ids[1]))], "everyone is told, once (asking twice is asking once)");
        assert_eq!(w.clients[i].owns_thing(2), i == 1);
    }
    // C asks for it too while B still has it: first come, first served. Only C is told where it stands.
    w.clients[2].claim(k);
    w.run(0.3);
    assert_eq!(w.server.owner(k), Some(ids[1]));
    assert_eq!(owners(&w.events(2)), [(k, Some(ids[1]))]);
    assert!(w.events(0).is_empty() && w.events(1).is_empty());
    assert!(w.clients[2].asked(k) && !w.clients[2].owns(k));
    // B lets it go: it passes to who was waiting for it, not to the host.
    w.clients[1].release(k);
    w.run(0.3);
    assert_eq!(w.server.owner(k), Some(ids[2]));
    assert!((0..3).all(|i| owners(&w.events(i)) == [(k, Some(ids[2]))]));
    // C lets it go too: nobody asks for it, so it is the host's again.
    w.clients[2].release(k);
    w.run(0.3);
    assert_eq!((w.server.owner(k), w.server.key_count()), (Some(ids[0]), 0));
    assert!((0..3).all(|i| owners(&w.events(i)) == [(k, None)]));
    assert!(w.clients[0].owns_thing(2) && (0..3).all(|i| w.clients[i].owner(k) == Some(ids[0])));
    // Letting go of what was never asked for is nothing.
    w.clients[1].release(k);
    w.run(0.3);
    assert!((0..3).all(|i| w.events(i).is_empty()));
    assert_eq!(w.server.stats().garbled, 0);
}

#[test]
fn a_seat_is_one_players_at_a_time() {
    let (mut w, ids) = three(3);
    let seat = key::seat(2, 0);
    // B and C go for the same seat in the same frame.
    w.clients[1].claim(seat);
    w.clients[2].claim(seat);
    w.run(0.3);
    let held = w.server.owner(seat).expect("someone got it");
    assert!(held == ids[1] || held == ids[2]);
    let (won, lost) = if held == ids[1] { (1, 2) } else { (2, 1) };
    assert!(w.clients[won].owns(seat) && !w.clients[lost].owns(seat));
    // Everyone knows who sits there; the one who did not get it was told so, and gets up.
    assert!((0..3).all(|i| w.clients[i].owner(seat) == Some(held)));
    assert!(owners(&w.events(lost)).contains(&(seat, Some(held))));
    w.clients[lost].release(seat);
    w.run(0.3);
    assert_eq!(w.server.owner(seat), Some(held), "who gave up waiting changes nothing");
    (0..3).for_each(|i| drop(w.events(i)));
    // The one who sat gets up: the seat is nobody's (not the host's: a seat is not a thing).
    w.clients[won].release(seat);
    w.run(0.3);
    assert_eq!(w.server.owner(seat), None);
    assert!((0..3).all(|i| owners(&w.events(i)) == [(seat, None)] && w.clients[i].owner(seat).is_none()));
    // Another seat of the same ship is another key.
    w.clients[lost].claim(key::seat(2, 1));
    w.clients[won].claim(seat);
    w.run(0.3);
    assert_eq!((w.server.owner(key::seat(2, 1)), w.server.owner(seat)), (Some(ids[lost]), Some(ids[won])));
}

#[test]
fn a_holder_who_leaves_hands_it_on() {
    let (mut w, ids) = three(4);
    let (k0, k2) = (key::thing(0), key::thing(2));
    w.clients[1].claim(k2);
    w.clients[1].claim(key::seat(2, 0));
    w.clients[2].claim(k0);
    w.run(0.3);
    assert_eq!((w.server.owner(k0), w.server.owner(k2)), (Some(ids[2]), Some(ids[1])));
    (0..3).for_each(|i| drop(w.events(i)));
    // The pilot of ship 2 leaves: the host has it again, and its seat is free.
    w.clients[1].close();
    w.run(0.3);
    assert_eq!((w.server.owner(k2), w.server.owner(key::seat(2, 0))), (Some(ids[0]), None));
    for i in [0, 2] {
        let events = w.events(i);
        assert_eq!(events[0], Event::Left { id: ids[1], name: "B".to_string() });
        let mut changed = owners(&events);
        changed.sort_unstable();
        assert_eq!(changed, [(k2, None), (key::seat(2, 0), None)]);
        assert_eq!(w.clients[i].owner(k2), Some(ids[0]));
    }
    // The host vanishes without a word: once it is given up, the next oldest player is the host,
    // and has everything nobody asks for (what it asked for itself was its own already).
    w.vanish(0);
    w.run(11.0);
    assert_eq!((w.server.player_count(), w.server.host()), (1, Some(ids[2])));
    assert_eq!(w.events(2), [Event::Left { id: ids[0], name: "A".to_string() }, Event::Host { player: Some(ids[2]) }]);
    assert!((0..5).all(|id| w.clients[2].owns_thing(id) && w.server.owner(key::thing(id)) == Some(ids[2])));
}

#[test]
fn the_holders_state_of_a_thing_reaches_the_others_and_nobody_elses_does() {
    let (mut w, ids) = three(5);
    // B flies ship 2; the host has the rest. Everyone *tries* to speak for every thing.
    w.clients[1].claim(key::thing(2));
    w.run(0.3);
    const SHIPS: u64 = 4;
    for _ in 0..120 {
        w.step_with(|i, c, t| {
            c.set_player(&walker(i as f64));
            for s in 0..SHIPS {
                // Each client's copy of a ship is a little different: only the holder's must travel.
                let mut state = ship(s, s as f64 * 30.0 + i as f64, 12);
                state.pos += (state.vel * (t - 100.0) as f32).as_dvec3();
                c.set_rigid(&state);
            }
        });
    }
    let drawn = |w: &World, i: usize| w.now - w.clients[i].delay() as f64;
    let mut got = RigidState::default();
    for i in 0..3 {
        for s in 0..SHIPS {
            let owner = if s == 2 { 1 } else { 0 };
            if i == owner {
                assert!(!w.clients[i].rigid_into(s, w.now, &mut got), "a holder is not sent its own thing");
                continue;
            }
            assert!(w.clients[i].rigid_into(s, w.now, &mut got), "the holder's state");
            let mut want = ship(s, s as f64 * 30.0 + owner as f64, 12);
            want.pos += (want.vel.as_dvec3()) * (drawn(&w, i) - 100.0);
            assert_eq!((got.id, got.frame), (s, Frame::World));
            assert!((got.pos - want.pos).length() < 0.02, "client {i} sees ship {s} {} m off the holder's", (got.pos - want.pos).length());
            // The rotation went on turning at its spin; the holder sends it as it has it.
            assert!(angle_between(got.rot, want.rot) < 2e-4);
            assert!((got.vel - want.vel).length() < 0.002 && (got.spin - want.spin).length() < 0.001);
            assert_eq!(got.joints.len(), 12);
            assert!(got.joints.iter().zip(&want.joints).all(|(x, y)| (x - y).abs() < 0.0005));
            // And carried to the present, it is where the holder has it now.
            let mut now_state = RigidState::default();
            let age = w.clients[i].rigid_now(s, w.now, &mut now_state).expect("a state");
            let mut present = ship(s, s as f64 * 30.0 + owner as f64, 12);
            present.pos += present.vel.as_dvec3() * (w.now - 100.0);
            assert!(age < 0.15 && (now_state.pos - present.pos).length() < 0.05, "age {age}, off {}", (now_state.pos - present.pos).length());
        }
    }
    assert_eq!(w.server.stats().foreign, 0, "a well-behaved client does not even send what is not its own");
    // A thing told in another's frame arrives in it: a crate in the hold of the ship B flies.
    for _ in 0..40 {
        w.step_with(|i, c, _| {
            c.set_player(&walker(i as f64));
            c.set_rigid(&cargo(900, 2, Vec3::new(1.5, 0.4, -6.0)));
        });
    }
    assert!(w.clients[1].rigid_into(900, w.now, &mut got), "the host's crate");
    assert_eq!(got.frame, Frame::Aboard(2));
    assert!((got.pos - glam::DVec3::new(1.5, 0.4, -6.0)).length() < 0.001 && got.joints.is_empty());
    let _ = ids;
}

#[test]
fn a_thing_at_rest_costs_nothing_and_is_forgotten_by_the_server() {
    let (mut w, _) = three(6);
    // The host's crate moves for a second, then lies still: nobody gives it any more.
    for n in 0..60 {
        w.step_with(|i, c, _| {
            c.set_player(&walker(i as f64));
            if i == 0 {
                c.set_rigid(&RigidState { pos: ship(50, 0.0, 0).pos + glam::DVec3::X * n as f64 * 0.1, vel: Vec3::ZERO, spin: Vec3::ZERO, ..ship(50, 0.0, 0) });
            }
        });
    }
    let last = ship(50, 0.0, 0).pos + glam::DVec3::X * 5.9;
    assert_eq!(w.server.thing_count(), 1);
    w.run(1.0);
    let mut got = RigidState::default();
    assert!(w.clients[1].rigid_into(50, w.now, &mut got) && (got.pos - last).length() < 0.001, "where it was last told (said twice more, so one lost datagram does not lose it)");
    let (up, down) = (w.clients[0].stats().sent_bytes, w.clients[1].stats().recv_bytes);
    let (ups, downs) = (w.clients[0].stats().sent_datagrams, w.clients[1].stats().recv_datagrams);
    for _ in 0..600 {
        w.step_with(|i, c, _| c.set_player(&PlayerState { vel: Vec3::ZERO, ..walker(i as f64) }));
    }
    // Ten seconds on: only the players' own once a second, the pings and the acks went by.
    let (up, down) = ((w.clients[0].stats().sent_bytes - up) as f64 / 10.0, (w.clients[1].stats().recv_bytes - down) as f64 / 10.0);
    println!("a crate at rest: its holder sends {up:.0} B/s in all ({} datagrams in 10 s), another client receives {down:.0} B/s ({} datagrams)", w.clients[0].stats().sent_datagrams - ups, w.clients[1].stats().recv_datagrams - downs);
    assert!(up < 80.0 && down < 120.0, "{up} B/s up, {down} B/s down");
    assert_eq!(w.server.thing_count(), 0, "the server keeps nothing of what is at rest");
    // Whoever comes now is not told of it by the server: the game tells where things lie.
    let d = w.join("D");
    w.settle(d);
    w.run(0.5);
    assert!(!w.clients[d].rigid_into(50, w.now, &mut got));
    // It moves again: everyone hears, the newcomer too, and knows it stood still until now.
    for n in 0..30 {
        w.step_with(|i, c, _| {
            c.set_player(&walker(i as f64));
            if i == 0 {
                c.set_rigid(&RigidState { pos: last + glam::DVec3::X * n as f64 * 0.1, vel: Vec3::ZERO, spin: Vec3::ZERO, ..ship(50, 0.0, 0) });
            }
        });
    }
    assert!(w.clients[d].rigid_into(50, w.now, &mut got) && w.clients[1].rigid_into(50, w.now, &mut got));
    assert!(got.pos.distance(last) > 1.0);
}

/// A client written by hand, that says whatever it is told to.
struct Rogue {
    end: lunar_net::Memory,
    channel: Channel,
    inbox: Inbox,
    id: u32,
}

impl Rogue {
    fn join(w: &mut World) -> Rogue {
        let mut end = w.net.endpoint();
        let mut buf = [0u8; 1200];
        // The hello, the server's challenge, the hello again with what it said, the welcome.
        let mut cookie = 0;
        loop {
            let n = Datagram::Hello { version: VERSION, salt: 7, cookie, scenario: SCENARIO, build: BUILD, name: "Tramposo" }.encode(&mut buf);
            end.send(w.addr, &buf[..n]);
            w.run(0.1);
            while let Some((_, n)) = end.recv(&mut buf) {
                match Datagram::decode(&buf[..n]) {
                    Ok(Datagram::Challenge { cookie: said, .. }) => cookie = said,
                    Ok(Datagram::Welcome { id, .. }) => return Rogue { end, channel: Channel::new(w.now, lead::DATA), inbox: Inbox::new(), id },
                    other => panic!("neither a challenge nor a welcome: {other:?}"),
                }
            }
        }
    }
    fn listen(&mut self, w: &World) {
        let mut buf = [0u8; 1200];
        while let Some((_, n)) = self.end.recv(&mut buf) {
            self.inbox.clear();
            let _ = self.channel.receive(&buf[1..n], w.now, &mut self.inbox);
        }
    }
    /// Sends a batch with its own player and what it claims are the states of `things`, and keeps its channel alive.
    fn say(&mut self, w: &World, me: &PlayerState, things: &[RigidState]) {
        self.listen(w);
        let mut msg = [0u8; 1100];
        let mut m = Writer::new(&mut msg);
        begin_up(&mut m, (w.now * 1e6) as u64);
        let mut raw = [0u8; 300];
        let mut r = Writer::new(&mut raw);
        me.encode(&mut r);
        let n = r.finish().expect("fits");
        up_entry(&mut m, Whose::Own, 0, false, &raw[..n]);
        for s in things {
            let whose = Whose::Thing(key::thing(s.id));
            let mut r = Writer::new(&mut raw);
            s.encode_joints(&mut r);
            let n = r.finish().expect("fits");
            up_entry(&mut m, whose, 1, false, &raw[..n]);
            let mut r = Writer::new(&mut raw);
            s.encode_rigid(&mut r);
            let n = r.finish().expect("fits");
            up_entry(&mut m, whose, 0, false, &raw[..n]);
        }
        let n = m.finish().expect("fits");
        self.channel.send_unreliable(&msg[..n]);
        self.channel.flush(w.now, w.addr, &mut self.end);
    }
    fn ask(&mut self, w: &World, msg: &Msg) {
        let mut buf = [0u8; 64];
        let mut m = Writer::new(&mut buf);
        msg.encode(&mut m);
        let n = m.finish().expect("fits");
        self.channel.send_reliable(&buf[..n]);
        self.channel.flush(w.now, w.addr, &mut self.end);
    }
}

#[test]
fn states_of_a_thing_from_someone_who_does_not_hold_it_are_ignored() {
    let mut w = World::plain(7);
    let (a, b) = (w.join("A"), w.join("B"));
    let (ida, _idb) = (w.settle(a), w.settle(b));
    let mut rogue = Rogue::join(&mut w);
    assert_eq!(w.server.player_count(), 3);
    const SHIPS: u64 = 4;
    let truth = |s: u64, n: u32| RigidState { pos: ship(s, s as f64 * 30.0, 4).pos + glam::DVec3::Y * (n as f64 * 0.01), vel: Vec3::ZERO, spin: Vec3::ZERO, ..ship(s, s as f64 * 30.0, 4) };
    let lie = |s: u64, n: u32| RigidState { pos: truth(s, n).pos + glam::DVec3::splat(5000.0), ..truth(s, n) };
    let mut got = RigidState::default();
    for n in 0..120 {
        // The host says where its ships are; the rogue says they are somewhere else, and so is a thing 77.
        rogue.say(&w, &walker(50.0), &[lie(0, n), lie(1, n), lie(3, n), lie(77, n)]);
        w.step_with(|i, c, _| {
            c.set_player(&walker(i as f64));
            if i == 0 {
                (0..SHIPS).for_each(|s| c.set_rigid(&truth(s, n)));
            }
        });
    }
    assert!(w.server.stats().foreign >= 8 * 100, "each lie was counted (four things, and their joints): {}", w.server.stats().foreign);
    assert_eq!(w.server.stats().garbled, 0);
    for s in 0..SHIPS {
        assert_eq!(w.server.owner(key::thing(s)), Some(ida));
        assert!(w.clients[b].rigid_into(s, w.now, &mut got), "the holder's state");
        assert!((got.pos - truth(s, 119).pos).length() < 0.15, "B sees ship {s} where its holder has it (a moment ago)");
    }
    assert!(!w.clients[b].rigid_into(77, w.now, &mut got) || got.pos.distance(lie(77, 0).pos) > 1000.0, "what the host never told of, the rogue cannot make up");
    // The rogue's own player, though, is its own to tell.
    let mut seen = Vec::new();
    w.clients[b].players(w.now, &mut seen);
    assert!(seen.iter().any(|(id, p)| *id == rogue.id && (p.pos - walker(50.0).pos).length() < 0.001));
    // When the rogue asks for ship 3 and gets it, its word about ship 3 (and only 3) counts.
    rogue.ask(&w, &Msg::Claim { key: key::thing(3) });
    for n in 120..180 {
        rogue.say(&w, &walker(50.0), &[lie(3, n), lie(1, n)]);
        w.step_with(|i, c, _| {
            c.set_player(&walker(i as f64));
            if i == 0 {
                (0..SHIPS).for_each(|s| c.set_rigid(&truth(s, n)));
            }
        });
    }
    assert_eq!(w.server.owner(key::thing(3)), Some(rogue.id));
    assert!(w.clients[b].rigid_into(3, w.now, &mut got));
    assert!((got.pos - lie(3, 179).pos).length() < 0.15, "ship 3 is where its new holder says");
    assert!(w.clients[b].rigid_into(1, w.now, &mut got));
    assert!((got.pos - truth(1, 179).pos).length() < 0.15, "ship 1 is still where the host says");
    assert!(w.clients[a].rigid_into(3, w.now, &mut got), "the host, no longer its holder, now receives ship 3");
}

#[test]
fn a_player_may_ask_for_only_so_many_keys() {
    let mut w = World::new(8, ServerConfig { max_keys: 10, ..ServerConfig::default() });
    let (a, b) = (w.join("A"), w.join("B"));
    let (_, idb) = (w.settle(a), w.settle(b));
    (0..2).for_each(|i| drop(w.events(i)));
    for id in 0..40u64 {
        w.clients[b].claim(key::thing(id));
    }
    w.run(0.5);
    assert_eq!(w.server.key_count(), 10);
    assert_eq!(owners(&w.events(a)), (0..10).map(|id| (key::thing(id), Some(idb))).collect::<Vec<_>>());
    // Letting some go makes room for others.
    (0..5).for_each(|id| w.clients[b].release(key::thing(id)));
    (100..110).for_each(|id| w.clients[b].claim(key::thing(id)));
    w.run(0.5);
    assert_eq!(w.server.key_count(), 10);
    assert!((100..105).all(|id| w.server.owner(key::thing(id)) == Some(idb)) && w.server.owner(key::thing(105)) != Some(idb));
}
