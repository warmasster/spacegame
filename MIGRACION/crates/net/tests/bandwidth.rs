//! What a game costs on the wire. Not a pass-or-fail of behaviour: it prints the numbers (they
//! are in docs/MULTIJUGADOR.md) and fails only if they grow past the budget written beside each.
mod common;

use common::{World, cargo, on_the_moon, ship};
use glam::{DVec3, Vec3};
use lunar_net::{Client, Frame, PlayerState, RigidState, flag};

const SHIPS: u64 = 4;

/// Bytes a second up and down for a client over `seconds`, counting the 28 bytes of IP and UDP of each datagram.
struct Rate {
    up: f64,
    down: f64,
    up_datagrams: f64,
    down_datagrams: f64,
}

fn measure(w: &mut World, seconds: f64, mut each: impl FnMut(usize, &mut Client, f64)) -> Vec<Rate> {
    let before: Vec<_> = w.clients.iter().map(|c| c.stats()).collect();
    for _ in 0..(seconds * 60.0) as usize {
        w.step_with(&mut each);
    }
    w.clients
        .iter()
        .zip(before)
        .map(|(c, b)| {
            let s = c.stats();
            let (ud, dd) = ((s.sent_datagrams - b.sent_datagrams) as f64, (s.recv_datagrams - b.recv_datagrams) as f64);
            Rate { up: ((s.sent_bytes - b.sent_bytes) as f64 + 28.0 * ud) / seconds, down: ((s.recv_bytes - b.recv_bytes) as f64 + 28.0 * dd) / seconds, up_datagrams: ud / seconds, down_datagrams: dd / seconds }
        })
        .collect()
}

/// Everyone walking and looking around: every state changes every tick.
fn moving(i: usize, t: f64) -> PlayerState {
    let speed = 1.5 + i as f64 * 0.1;
    PlayerState {
        pos: on_the_moon(i as f64 * 5.0) + DVec3::new(0.3, 0.1, speed) * (t - 100.0),
        vel: Vec3::new(0.3, 0.1, speed as f32),
        yaw: (t * 0.7 + i as f64) as f32,
        pitch: (t * 0.3).sin() as f32 * 0.3,
        flags: flag::GROUNDED,
        tool: 1,
        ..PlayerState::default()
    }
}

/// A ship in flight with 20 joints, all still (gear up, doors shut: the usual in flight).
fn flying(id: u64, t: f64) -> RigidState {
    let mut s = ship(id, id as f64 * 60.0, 20);
    s.pos += s.vel.as_dvec3() * (t - 100.0);
    s.rot = glam::Quat::from_rotation_y((t * 0.2) as f32 + id as f32);
    s
}

/// The same with some joint always moving (the gear coming down, a ramp opening): the worst case.
fn working(id: u64, t: f64) -> RigidState {
    let mut s = flying(id, t);
    s.joints[3] = (t * 0.5).sin() as f32;
    s.joints[7] = (t * 0.25).cos() as f32;
    s
}

#[test]
fn eight_players_and_four_ships_at_twenty_a_second() {
    let mut w = World::plain(1);
    for i in 0..8 {
        let c = w.join(&format!("Jugador {i}"));
        w.settle(c);
    }
    // Everything moves: the host flies (holds and sends) the four ships.
    measure(&mut w, 2.0, |i, c, t| {
        c.set_player(&moving(i, t));
        (0..SHIPS).for_each(|s| c.set_rigid(&flying(s, t)));
    });
    let busy = measure(&mut w, 10.0, |i, c, t| {
        c.set_player(&moving(i, t));
        (0..SHIPS).for_each(|s| c.set_rigid(&flying(s, t)));
    });
    println!("8 players walking, 4 ships flying (20 joints each, still), 20 states a second (bytes include the 28 of IP+UDP per datagram):");
    println!("  the host (sends its player and the 4 ships): up {:.0} B/s in {:.0} datagrams/s, down {:.0} B/s in {:.0} datagrams/s", busy[0].up, busy[0].up_datagrams, busy[0].down, busy[0].down_datagrams);
    println!("  any other client (sends its player):          up {:.0} B/s in {:.0} datagrams/s, down {:.0} B/s in {:.0} datagrams/s", busy[1].up, busy[1].up_datagrams, busy[1].down, busy[1].down_datagrams);
    let server = busy.iter().map(|r| r.down).sum::<f64>();
    println!("  the server sends {:.1} kB/s in all ({:.0} kbit/s) and receives {:.1} kB/s", server / 1000.0, server * 8.0 / 1000.0, busy.iter().map(|r| r.up).sum::<f64>() / 1000.0);
    // A client hears 7 players and 4 ships 20 times a second in one datagram each time.
    assert!((19.0..23.0).contains(&busy[1].down_datagrams), "batched: one datagram a tick, not one per sender: {}", busy[1].down_datagrams);
    assert!(busy[1].down < 9_500.0 && busy[1].up < 2_000.0 && busy[0].up < 5_200.0, "{} {} {}", busy[1].down, busy[1].up, busy[0].up);

    // The worst case: in every ship some joint moves all the time, so the joints travel every tick too.
    let go = |i: usize, c: &mut Client, t: f64| {
        c.set_player(&moving(i, t));
        (0..SHIPS).for_each(|s| c.set_rigid(&working(s, t)));
    };
    measure(&mut w, 1.0, go);
    let worst = measure(&mut w, 10.0, go);
    println!("the same with a joint moving in every ship all the time (the joints travel every tick):");
    println!("  the host: up {:.0} B/s, down {:.0} B/s; any other client: up {:.0} B/s, down {:.0} B/s", worst[0].up, worst[0].down, worst[1].up, worst[1].down);
    assert!(worst[1].down < 13_500.0 && worst[0].up < 9_000.0);
    let mut got = RigidState::default();
    assert!(w.clients[3].rigid_into(2, w.now, &mut got), "a ship");
    let want = working(2, w.now - w.clients[3].delay() as f64);
    assert!((got.joints[3] - want.joints[3]).abs() < 0.002 && (got.joints[7] - want.joints[7]).abs() < 0.002, "and the moving joints are followed");

    // The same game with everyone standing still and the ships parked: nobody gives the ships any
    // more (a thing at rest is not told of), and each player goes once a second.
    let still = |i: usize| PlayerState { vel: Vec3::ZERO, ..moving(i, 100.0) };
    let parked = |s: u64| RigidState { vel: Vec3::ZERO, spin: Vec3::ZERO, ..flying(s, 100.0) };
    measure(&mut w, 2.0, |i, c, _| {
        c.set_player(&still(i));
        (0..SHIPS).for_each(|s| c.set_rigid(&parked(s)));
    });
    w.run(1.0);
    let idle = measure(&mut w, 10.0, |i, c, _| c.set_player(&still(i)));
    println!("the same with everyone still and the ships parked (the players once a second each, the ships not at all):");
    println!("  the host: up {:.0} B/s, down {:.0} B/s; any other client: up {:.0} B/s, down {:.0} B/s", idle[0].up, idle[0].down, idle[1].up, idle[1].down);
    assert!(idle[1].down < busy[1].down / 10.0 && idle[0].up < busy[0].up / 20.0);
    assert!((idle[0].up - idle[1].up).abs() < 5.0, "the host pays nothing for the ships it holds while they rest: {} and {}", idle[0].up, idle[1].up);
    // And what was sent is what the others have: still where they stood.
    let mut seen = Vec::new();
    w.clients[5].players(w.now, &mut seen);
    assert_eq!(seen.len(), 7);
    assert!(seen.iter().all(|(id, p)| (p.pos - still(*id as usize - 1).pos).length() < 0.001));
    assert!(w.clients[5].rigid_into(2, w.now, &mut got) && (got.pos - parked(2).pos).length() < 0.001);
}

#[test]
fn four_players_and_three_ships_at_rest() {
    // The budget of a quiet game: four players standing about, three ships parked.
    let mut w = World::plain(3);
    for i in 0..4 {
        let c = w.join(&format!("Jugador {i}"));
        w.settle(c);
    }
    let still = |i: usize| PlayerState { vel: Vec3::ZERO, ..moving(i, 100.0) };
    let parked = |s: u64| RigidState { vel: Vec3::ZERO, spin: Vec3::ZERO, ..flying(s, 100.0) };
    // (the ships settle, and then nobody gives them)
    measure(&mut w, 1.0, |i, c, _| {
        c.set_player(&still(i));
        (0..3).for_each(|s| c.set_rigid(&parked(s)));
    });
    w.run(2.0);
    let idle = measure(&mut w, 20.0, |i, c, _| c.set_player(&still(i)));
    println!("4 players standing, 3 ships at rest: the host up {:.0} B/s, down {:.0} B/s; any other client up {:.0} B/s, down {:.0} B/s; the server sends {:.0} B/s in all (with IP+UDP)", idle[0].up, idle[0].down, idle[1].up, idle[1].down, idle.iter().map(|r| r.down).sum::<f64>());
    for r in &idle {
        assert!(r.up < 130.0 && r.down < 260.0, "budget: 130 B/s up, 260 B/s down each: {:.0} up, {:.0} down", r.up, r.down);
    }
    assert_eq!(w.server.thing_count(), 0);
}

#[test]
fn a_ship_flying_with_two_loose_crates_in_its_hold() {
    // One player flies a ship; two crates slide about its hold (told in the ship's frame, by the
    // same player: they ride with it); three more players watch.
    let mut w = World::plain(4);
    for i in 0..4 {
        let c = w.join(&format!("Jugador {i}"));
        w.settle(c);
    }
    let crate_at = |k: u64, t: f64| RigidState { pos: DVec3::new(1.5 * k as f64, 0.4, -6.0 + (t * (0.4 + 0.1 * k as f64)).sin() * 0.8), vel: Vec3::new(0.0, 0.0, ((t * (0.4 + 0.1 * k as f64)).cos() * 0.8 * (0.4 + 0.1 * k as f64)) as f32), ..cargo(900 + k, 2, Vec3::ZERO) };
    let go = |i: usize, c: &mut Client, t: f64| {
        c.set_player(&moving(i, t));
        c.set_rigid(&flying(2, t));
        (0..2).for_each(|k| c.set_rigid(&crate_at(k, t)));
    };
    measure(&mut w, 2.0, go);
    let busy = measure(&mut w, 10.0, go);
    println!("4 players walking, one ship flying with 2 crates sliding in its hold: the host (flies it) up {:.0} B/s, down {:.0} B/s; any other client up {:.0} B/s, down {:.0} B/s (with IP+UDP)", busy[0].up, busy[0].down, busy[1].up, busy[1].down);
    assert!(busy[0].up < 3_600.0 && busy[1].up < 1_700.0, "budget: 3.6 kB/s up for who flies it, 1.7 kB/s for the rest: {:.0} and {:.0}", busy[0].up, busy[1].up);
    assert!(busy[1].down < 5_200.0 && busy[0].down < 3_300.0, "budget: 5.2 kB/s down: {:.0} and {:.0}", busy[1].down, busy[0].down);
    let mut got = RigidState::default();
    assert!(w.clients[3].rigid_into(901, w.now, &mut got) && got.frame == Frame::Aboard(2));
    let want = crate_at(1, w.now - w.clients[3].delay() as f64);
    assert!((got.pos - want.pos).length() < 0.01, "the crate is where it is in the hold: {} m off", (got.pos - want.pos).length());
    // The crates come to rest, the ship flies on: they cost nothing more.
    let on = |i: usize, c: &mut Client, t: f64| {
        c.set_player(&moving(i, t));
        c.set_rigid(&flying(2, t));
    };
    measure(&mut w, 1.0, on);
    let after = measure(&mut w, 10.0, on);
    println!("the same once the crates lie still: the host up {:.0} B/s; any other client down {:.0} B/s", after[0].up, after[1].down);
    assert!(after[0].up < busy[0].up - 500.0 && after[1].down < busy[1].down - 500.0);
}

#[test]
fn sixteen_players_and_forty_ships() {
    // A full server and a busy sky: the batch for each client no longer fits one datagram.
    let mut w = World::plain(2);
    for i in 0..16 {
        let c = w.join_as(&format!("Jugador {i}"), common::BUILD, 40);
        w.settle(c);
    }
    let go = |i: usize, c: &mut Client, t: f64| {
        c.set_player(&moving(i, t));
        (0..40).for_each(|s| c.set_rigid(&flying(s, t)));
    };
    measure(&mut w, 2.0, go);
    let busy = measure(&mut w, 5.0, go);
    println!(
        "16 players walking, 40 ships flying with 20 joints each: the host up {:.1} kB/s in {:.0} datagrams/s; any other client down {:.1} kB/s in {:.0} datagrams/s, up {:.0} B/s",
        busy[0].up / 1000.0,
        busy[0].up_datagrams,
        busy[1].down / 1000.0,
        busy[1].down_datagrams,
        busy[1].up
    );
    let mut seen = Vec::new();
    w.clients[9].players(w.now, &mut seen);
    assert_eq!(seen.len(), 15);
    let mut got = RigidState::default();
    assert!((0..40).all(|s| w.clients[9].rigid_into(s, w.now, &mut got)), "every ship arrives, in as many datagrams as it takes");
    assert!((35.0..50.0).contains(&busy[1].down_datagrams), "two datagrams a tick: {}", busy[1].down_datagrams);
}
