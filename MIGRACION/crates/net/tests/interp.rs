//! Snapshot interpolation: what a client draws of the others is where they were a moment ago,
//! smoothly, whatever the timing of the datagrams that brought it.
mod common;

use common::{Dice, FRAME, World, on_the_moon};
use glam::{DVec3, Quat, Vec3};
use lunar_net::snap::{EXTRAPOLATE, SnapBuffer};
use lunar_net::{Conditions, PlayerState, RigidState, flag};

const TICK: f64 = 0.05;

fn at(x: f64) -> PlayerState {
    PlayerState { pos: DVec3::new(x, 0.0, 0.0), vel: Vec3::new(2.0, 0.0, 0.0), ..PlayerState::default() }
}

fn sample(buf: &SnapBuffer<PlayerState>, t: f64) -> Option<PlayerState> {
    let mut out = PlayerState::default();
    buf.sample(t, TICK, &mut out).then_some(out)
}

#[test]
fn the_buffer_mixes_between_snapshots_carries_a_little_past_the_newest_and_then_holds() {
    let mut buf = SnapBuffer::<PlayerState>::default();
    assert!(sample(&buf, 1.0).is_none());
    for i in 0..5 {
        *buf.push(10.0 + i as f64 * TICK, false).expect("newer") = at(i as f64 * 0.1);
    }
    let x = |buf: &SnapBuffer<PlayerState>, t: f64| sample(buf, t).expect("a state").pos.x;
    // Between two: a straight line.
    assert!((x(&buf, 10.0) - 0.0).abs() < 1e-9 && (x(&buf, 10.025) - 0.05).abs() < 1e-6 && (x(&buf, 10.19) - 0.38).abs() < 1e-6);
    // Before the oldest: the oldest.
    assert_eq!(x(&buf, 3.0), 0.0);
    // Past the newest (10.2, at 0.4, going at 2 m/s): carried on for a quarter of a second, then held.
    assert!((x(&buf, 10.3) - 0.6).abs() < 1e-6);
    assert!((x(&buf, 10.2 + EXTRAPOLATE) - 0.9).abs() < 1e-6);
    assert!((x(&buf, 10.2 + EXTRAPOLATE + 0.1) - 0.9).abs() < 1e-6 && (x(&buf, 99.0) - 0.9).abs() < 1e-6);
    // An older snapshot arriving late is of no use and is not taken; nor the same one twice.
    assert!(buf.push(10.1, false).is_none() && buf.push(10.2, false).is_none());
    // It keeps the last sixteen.
    for i in 5..40 {
        *buf.push(10.0 + i as f64 * TICK, false).expect("newer") = at(i as f64 * 0.1);
    }
    assert!((x(&buf, 10.0 + 39.0 * TICK - 0.01) - 3.88).abs() < 1e-6);
    assert!((x(&buf, 0.0) - 2.4).abs() < 1e-9, "the oldest kept is the 24th");
    buf.clear();
    assert!(sample(&buf, 11.0).is_none());
}

#[test]
fn after_a_silence_the_change_is_not_stretched_over_it() {
    // Stood still at 0 (the sender went quiet), then one tick before 12.0 began to move.
    let mut buf = SnapBuffer::<PlayerState>::default();
    *buf.push(10.0, false).expect("newer") = PlayerState { vel: Vec3::ZERO, ..at(0.0) };
    *buf.push(12.0, true).expect("newer") = at(0.1);
    let x = |t: f64| sample(&buf, t).expect("a state").pos.x;
    assert_eq!(x(11.0), 0.0);
    assert_eq!(x(11.9), 0.0);
    assert!((x(11.975) - 0.05).abs() < 1e-6);
    // The same gap without the mark is a gap in the network: there, a straight line is the best guess.
    let mut buf = SnapBuffer::<PlayerState>::default();
    *buf.push(10.0, false).expect("newer") = at(0.0);
    *buf.push(12.0, false).expect("newer") = at(4.0);
    assert!((sample(&buf, 11.0).expect("a state").pos.x - 2.0).abs() < 1e-6);
}

#[test]
fn ships_mix_their_rotation_along_the_shortest_arc_and_their_joints_one_by_one() {
    let a = RigidState { id: 3, pos: DVec3::new(1e6, 0.0, 0.0), rot: Quat::from_rotation_y(-0.2), joints: vec![0.0, 1.0], ..RigidState::default() };
    let b = RigidState { id: 3, pos: DVec3::new(1e6 + 1.0, 2.0, 0.0), rot: -Quat::from_rotation_y(0.2), joints: vec![1.0, 1.0], spin: Vec3::Y, ..RigidState::default() };
    let m = RigidState::mix(&a, &b, 0.5);
    assert!((m.pos - DVec3::new(1e6 + 0.5, 1.0, 0.0)).length() < 1e-9, "f64 all the way: no precision lost a million metres out");
    assert!(common::angle_between(m.rot, Quat::IDENTITY) < 1e-6, "-q is the same rotation as q: the short way goes through the identity");
    assert_eq!(m.joints, [0.5, 1.0]);
    let mut c = b.clone();
    c.carry(0.1);
    assert!(common::angle_between(c.rot, Quat::from_rotation_y(0.3)) < 1e-6, "carried on by its spin");
    // A ship that changes its number of joints (a part lost) cannot be mixed joint by joint: the nearer state's are taken.
    let d = RigidState { joints: vec![9.0], ..b.clone() };
    assert_eq!(RigidState::mix(&a, &d, 0.4).joints, [0.0, 1.0]);
    assert_eq!(RigidState::mix(&a, &d, 0.6).joints, [9.0]);
    // Players: flags and seat from the nearer; another ship's frame is not mixed with this one's.
    let p = PlayerState { ride: Some(1), local: Vec3::X, yaw: 3.0, flags: flag::LAMP, ..PlayerState::default() };
    let q = PlayerState { ride: Some(2), local: Vec3::Y * 4.0, yaw: -3.0, flags: flag::CROUCHED, ..PlayerState::default() };
    let m = PlayerState::mix(&p, &q, 0.25);
    assert_eq!((m.ride, m.local, m.flags), (Some(1), Vec3::X, flag::LAMP));
    assert!(m.yaw > 3.0 || m.yaw < -3.0, "the short way round from 3 to -3 does not pass through 0: {}", m.yaw);
    assert_eq!(PlayerState::mix(&p, &q, 0.75).local, Vec3::Y * 4.0);
}

/// Client 0 moves as `path` says; client 1 watches, sampling at odd moments. Returns, for each
/// sample: the moment it shows, what it showed, and the time since the last sample.
fn watch(seed: u64, cond: Conditions, seconds: f64, path: impl Fn(f64) -> PlayerState) -> (Vec<(f64, PlayerState, f64)>, World) {
    let mut w = World::plain(seed);
    let (a, b) = (w.join("Corre"), w.join("Mira"));
    w.settle(a);
    w.settle(b);
    w.conditions(cond);
    let mut dice = Dice(seed);
    let (mut seen, mut samples, mut last) = (Vec::new(), Vec::new(), None);
    let started = w.now;
    while w.now < started + seconds {
        w.step_with(|i, c, t| {
            if i == 0 {
                c.set_player(&path(t));
            }
        });
        // Not on the frame: at any moment within it, as a renderer with its own pace would.
        let when = w.now + dice.unit() * FRAME * 0.9;
        w.clients[b].players(when, &mut seen);
        if let Some((_, state)) = seen.first() {
            // The moment drawn, on the test's own clock: the watcher's idea of the server's time, less
            // its delay, read back through the runner's idea of it (which is what stamped the states).
            let drawn = w.clients[b].server_time(when).expect("a clock") - w.clients[b].delay() as f64 - (w.clients[a].server_time(when).expect("a clock") - when);
            samples.push((drawn, *state, last.map_or(0.0, |l| when - l)));
            last = Some(when);
        }
    }
    (samples, w)
}

const JITTERY: Conditions = Conditions { loss: 0.03, duplicate: 0.02, delay: 0.03, jitter: 0.04 };

#[test]
fn a_player_at_constant_speed_is_reproduced_smoothly_through_jitter() {
    let (origin, vel) = (on_the_moon(0.0), DVec3::new(1.8, 0.3, -2.4));
    let speed = vel.length();
    let path = |t: f64| PlayerState { pos: origin + vel * (t - 100.0), vel: vel.as_vec3(), flags: flag::GROUNDED, ..PlayerState::default() };
    let (samples, w) = watch(41, JITTERY, 20.0, path);
    assert!(samples.len() > 1100, "seen almost from the start: {}", samples.len());
    let (mut worst, mut off_line, mut worst_step, mut back) = (0.0f64, 0.0f64, 0.0f64, f64::MAX);
    // The first seconds are the connection settling (its clock, its delay): look after them.
    let settled = &samples[180..];
    for (k, (drawn, state, dt)) in settled.iter().enumerate() {
        worst = worst.max((state.pos - path(*drawn).pos).length());
        let along = (state.pos - origin).dot(vel) / speed;
        off_line = off_line.max((state.pos - origin - vel / speed * along).length());
        if k > 0 {
            let moved = (state.pos - settled[k - 1].1.pos).dot(vel) / speed;
            worst_step = worst_step.max((moved - speed * dt).abs());
            back = back.min(moved);
        }
    }
    println!(
        "constant {speed:.1} m/s through 30-70 ms of delay each way and 3 % loss: worst error {:.1} mm, worst step error {:.2} mm, {:.2} mm off the line, drawn {:.0} ms in the past",
        worst * 1000.0,
        worst_step * 1000.0,
        off_line * 1000.0,
        w.clients[1].delay() * 1000.0
    );
    assert!(worst < 0.03, "within a few centimetres of where it really was at the moment drawn: {worst}");
    assert!(off_line < 0.001, "and on its line: {off_line}");
    // The delay is trimmed by at most a tenth of each frame (1.7 ms): at 3 m/s that is 5 mm of a 51 mm step.
    assert!(worst_step < 0.1 * speed * FRAME + 0.001, "no jumps: it advances what the time passed says: {worst_step}");
    assert!(back > -0.0005, "and never goes backwards: {back}");
    let delay = w.clients[1].delay();
    assert!((0.1..0.25).contains(&delay), "about a tenth of a second in the past, a bit more for the jitter: {delay}");
}

#[test]
fn on_a_clean_network_the_others_are_drawn_a_tenth_of_a_second_ago_and_exactly() {
    let (origin, vel) = (on_the_moon(0.0), DVec3::new(0.0, 1.5, 0.0));
    let path = |t: f64| PlayerState { pos: origin + vel * (t - 100.0), vel: vel.as_vec3(), ..PlayerState::default() };
    let (samples, w) = watch(42, Conditions::default(), 6.0, path);
    let worst = samples[30..].iter().map(|(drawn, state, _)| (state.pos - path(*drawn).pos).length()).fold(0.0, f64::max);
    println!("clean network: worst error {:.2} mm, drawn {:.0} ms in the past", worst * 1000.0, w.clients[1].delay() * 1000.0);
    assert!(worst < 0.001, "{worst}");
    assert!((0.1..0.13).contains(&w.clients[1].delay()), "{}", w.clients[1].delay());
}

#[test]
fn a_player_who_stops_and_starts_is_followed_without_pops() {
    // Still for 3 s, walks 3 s at 2 m/s, still again: while still nothing is sent but a heartbeat.
    let origin = on_the_moon(0.0);
    let x = |t: f64| ((t - 104.0).clamp(0.0, 3.0)) * 2.0;
    let path = |t: f64| PlayerState { pos: origin + DVec3::X * x(t), vel: if (104.0..107.0).contains(&t) { Vec3::X * 2.0 } else { Vec3::ZERO }, ..PlayerState::default() };
    let (samples, w) = watch(43, Conditions { loss: 0.0, duplicate: 0.0, delay: 0.02, jitter: 0.02 }, 10.0, path);
    let settled = &samples[60..];
    let (mut worst, mut worst_step) = (0.0f64, 0.0f64);
    for (k, (drawn, state, dt)) in settled.iter().enumerate() {
        worst = worst.max((state.pos - path(*drawn).pos).length());
        if k > 0 {
            // It never moves faster than it walks (plus the slack of a clock being trimmed).
            worst_step = worst_step.max((state.pos - settled[k - 1].1.pos).length() - 2.0 * dt * 1.3);
        }
    }
    println!("stop and go: worst error {:.1} mm, worst excess step {:.1} mm", worst * 1000.0, worst_step * 1000.0);
    // Starting and stopping at once is a corner between two snapshots, and a straight line cuts it:
    // a quarter of what is walked between two snapshots (which can be two ticks apart) at worst.
    assert!(worst < 0.055, "{worst}");
    // It does not begin to move before the walker did (less one tick: the corner again), however long the silence before.
    assert!(settled.iter().filter(|s| s.0 < 104.0 - TICK - 0.005).all(|s| (s.1.pos - origin).length() < 0.0005), "no creeping during the silence");
    assert!(worst_step < 0.01, "{worst_step}");
    assert!((settled.last().expect("samples").1.pos - (origin + DVec3::X * 6.0)).length() < 0.001, "and it ends where the walker stopped");
    // Standing still costs almost nothing: the watcher got far fewer bytes than at 20 states a second.
    let stats = w.clients[0].stats();
    println!("the walker sent {} bytes in {} datagrams in 10 s (3 of them walking)", stats.sent_bytes, stats.sent_datagrams);
    assert!(stats.sent_bytes < 5000, "{}", stats.sent_bytes);
}

#[test]
fn a_gap_in_the_network_is_bridged_and_then_held() {
    let (origin, vel) = (on_the_moon(0.0), DVec3::new(3.0, 0.0, 0.0));
    let path = |t: f64| PlayerState { pos: origin + vel * (t - 100.0), vel: vel.as_vec3(), ..PlayerState::default() };
    let mut w = World::plain(44);
    let (a, b) = (w.join("Corre"), w.join("Mira"));
    w.settle(a);
    w.settle(b);
    for _ in 0..120 {
        w.step_with(|i, c, t| {
            if i == 0 {
                c.set_player(&path(t));
            }
        });
    }
    // The runner's connection stalls for 150 ms: the watcher carries it on, and nothing shows.
    let mut seen = Vec::new();
    w.net.cut(w.addrs[a], true);
    let mut worst = 0.0f64;
    for _ in 0..9 {
        w.step_with(|i, c, t| {
            if i == 0 {
                c.set_player(&path(t));
            }
        });
        w.clients[b].players(w.now, &mut seen);
        let drawn = w.now - w.clients[b].delay() as f64;
        worst = worst.max((seen[0].1.pos - path(drawn).pos).length());
    }
    assert!(worst < 0.005, "carried on by its velocity: {worst}");
    // It never comes back: after a quarter of a second past its last word, it is held where it would be.
    for _ in 0..60 {
        w.step();
    }
    w.clients[b].players(w.now, &mut seen);
    let frozen = seen[0].1.pos;
    w.run(1.0);
    w.clients[b].players(w.now, &mut seen);
    assert_eq!(seen[0].1.pos, frozen, "held, not flying off for ever");
}
