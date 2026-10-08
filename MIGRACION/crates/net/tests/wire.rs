//! The wire: every type goes and comes back, quantised ones within their stated error, and no
//! input whatever makes a decoder panic.
mod common;

use common::{Dice, angle_between, cargo, ship, walker};
use glam::{DVec3, Quat, Vec3};
use lunar_net::game::{JOINT_UNITS, LOCAL_UNITS, PLAYER_VEL_UNITS, POS_UNITS, PUSH_UNITS, RIGID_VEL_UNITS, SPIN_UNITS, mix_angle};
use lunar_net::proto::{Bundle, Datagram, Msg, VERSION};
use lunar_net::quant::ANGLE_STEP;
use lunar_net::wire::var_len;
use lunar_net::{Channel, Frame, Inbox, PlayerState, Reader, RigidState, WireError, Writer, flag};
use std::f32::consts::PI;

fn written(write: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut buf = vec![0u8; 4096];
    let mut w = Writer::new(&mut buf);
    write(&mut w);
    let n = w.finish().expect("fits");
    buf.truncate(n);
    buf
}

#[test]
fn plain_values_round_trip() {
    let bytes = written(|w| {
        w.u8(200);
        w.u16(54_321);
        w.u32(4_000_000_000);
        w.u64(u64::MAX - 5);
        w.f32(-1.5e-3);
        w.f64(1_737_400.123_456_789);
        w.str("Añil, 月, 🚀");
        w.str("");
        w.vec3(Vec3::new(1.0, -2.5, 3.25));
        w.bytes(&[9, 8, 7]);
    });
    let mut r = Reader::new(&bytes);
    assert_eq!(r.u8(), Ok(200));
    assert_eq!(r.u16(), Ok(54_321));
    assert_eq!(r.u32(), Ok(4_000_000_000));
    assert_eq!(r.u64(), Ok(u64::MAX - 5));
    assert_eq!(r.f32(), Ok(-1.5e-3));
    assert_eq!(r.f64(), Ok(1_737_400.123_456_789));
    assert_eq!(r.str(64), Ok("Añil, 月, 🚀"));
    assert_eq!(r.str(64), Ok(""));
    assert_eq!(r.vec3(), Ok(Vec3::new(1.0, -2.5, 3.25)));
    assert_eq!(r.bytes(3), Ok(&[9u8, 8, 7][..]));
    assert!(r.is_empty());
    assert_eq!(r.u8(), Err(WireError::Short));
}

#[test]
fn varints_round_trip_and_take_what_they_say() {
    let mut dice = Dice(7);
    let mut values = vec![0u64, 1, 127, 128, 16_383, 16_384, u32::MAX as u64, u64::MAX];
    values.extend((0..2000).map(|_| dice.next() >> dice.below(64)));
    for v in values {
        let bytes = written(|w| w.var(v));
        assert_eq!(bytes.len(), var_len(v), "length of {v}");
        assert_eq!(Reader::new(&bytes).var(), Ok(v));
        for s in [v as i64, -(v as i64), (v >> 1) as i64, i64::MIN, i64::MAX] {
            let bytes = written(|w| w.zig(s));
            assert_eq!(Reader::new(&bytes).zig(), Ok(s));
        }
    }
    // Small magnitudes of either sign are one byte.
    assert_eq!(written(|w| w.zig(-64)).len(), 1);
    assert_eq!(written(|w| w.zig(63)).len(), 1);
    // A varint that does not fit what is asked is refused, not cut.
    let big = written(|w| w.var(u32::MAX as u64 + 1));
    assert_eq!(Reader::new(&big).var32(), Err(WireError::Value));
    assert_eq!(Reader::new(&written(|w| w.var(70_000))).var16(), Err(WireError::Value));
    // Eleven bytes of "more follows" are no number.
    assert_eq!(Reader::new(&[0xff; 11]).var(), Err(WireError::Value));
}

#[test]
fn strings_are_capped_and_checked() {
    let bytes = written(|w| w.str("abcdef"));
    assert_eq!(Reader::new(&bytes).str(5), Err(WireError::Long));
    assert_eq!(Reader::new(&[2, 0xff, 0xfe]).str(8), Err(WireError::Value));
    assert_eq!(Reader::new(&[5, b'a']).str(8), Err(WireError::Short));
}

#[test]
fn floats_that_are_not_numbers_are_refused() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(Reader::new(&bad.to_le_bytes()).f32(), Err(WireError::Value));
        assert_eq!(Reader::new(&(bad as f64).to_le_bytes()).f64(), Err(WireError::Value));
    }
}

#[test]
fn a_writer_that_overflows_says_so_and_can_go_back() {
    let mut buf = [0u8; 4];
    let mut w = Writer::new(&mut buf);
    w.u16(1);
    let mark = w.mark();
    w.u32(2);
    assert!(!w.ok());
    assert_eq!(w.len(), 2, "what did not fit was not half written");
    w.rewind(mark);
    assert!(w.ok());
    w.u16(3);
    assert_eq!(w.finish(), Ok(4));
    let mut w = Writer::new(&mut buf);
    w.str("too long for four bytes");
    assert_eq!(w.finish(), Err(WireError::Long));
}

#[test]
fn angles_err_at_most_half_a_step() {
    let mut dice = Dice(11);
    let mut worst = 0.0f32;
    for i in 0..20_000 {
        let a = if i < 8 { [0.0, PI - 1e-6, -PI, PI / 2.0, -PI / 2.0, 6.5, -9.0, 100.0][i] } else { dice.signed() as f32 * PI };
        let bytes = written(|w| w.angle(a));
        assert_eq!(bytes.len(), 2);
        let back = Reader::new(&bytes).angle().expect("an angle");
        assert!((-std::f32::consts::PI..std::f32::consts::PI).contains(&back));
        worst = worst.max(back.sub_abs(a));
    }
    println!("angle: worst error {worst:.2e} rad (half a step is {:.2e})", ANGLE_STEP * 0.5);
    assert!(worst <= ANGLE_STEP * 0.5 + 2e-6, "{worst}");
    // NaN does not panic and is some angle.
    assert!(Reader::new(&written(|w| w.angle(f32::NAN))).angle().is_ok());
}

#[test]
fn fixed_point_errs_at_most_half_a_step() {
    let mut dice = Dice(13);
    for units in [POS_UNITS, LOCAL_UNITS, PLAYER_VEL_UNITS, RIGID_VEL_UNITS, SPIN_UNITS, JOINT_UNITS] {
        for scale in [1.0, 100.0, 1.7e6, 4.0e8, 1.5e11] {
            for _ in 0..500 {
                let v = dice.signed() * scale;
                let back = Reader::new(&written(|w| w.fixed(v, units))).fixed(units).expect("a number");
                // Half a step, plus what an f64 itself cannot tell apart that far out.
                assert!((back - v).abs() <= 0.5 / units + v.abs() * 1e-15, "{v} came back {back}");
            }
        }
    }
    // The Moon's surface is 5 bytes an axis; the Earth's distance, 6.
    assert_eq!(written(|w| w.fixed(1_737_400.0, POS_UNITS)).len(), 5);
    assert_eq!(written(|w| w.fixed(-384_400_000.0, POS_UNITS)).len(), 6);
    // Nothing a float can hold makes it panic.
    for odd in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e300, -1e300] {
        assert!(Reader::new(&written(|w| w.fixed(odd, POS_UNITS))).fixed(POS_UNITS).is_ok());
    }
    let v = DVec3::new(1.25, -7.5, 1e6);
    assert_eq!(Reader::new(&written(|w| w.fixed3(v, POS_UNITS))).fixed3(POS_UNITS), Ok(v));
}

#[test]
fn angles_mix_the_short_way() {
    let near = |a: f32, b: f32| a.sub_abs(b) < 1e-5;
    assert!(near(mix_angle(3.0, -3.0, 0.5), std::f32::consts::PI) || near(mix_angle(3.0, -3.0, 0.5), -std::f32::consts::PI));
    assert!(near(mix_angle(0.2, 0.6, 0.25), 0.3));
    assert!(near(mix_angle(-0.1, 0.1, 0.5), 0.0));
    assert!(near(mix_angle(1.0, 2.0, 0.0), 1.0) && near(mix_angle(1.0, 2.0, 1.0), 2.0));
}

#[test]
fn rotations_err_under_a_hundredth_of_a_degree() {
    let mut dice = Dice(17);
    let mut worst = 0.0f64;
    let mut all: Vec<Quat> = vec![Quat::IDENTITY, Quat::from_xyzw(1.0, 0.0, 0.0, 0.0), Quat::from_xyzw(0.5, 0.5, 0.5, 0.5), Quat::from_xyzw(-0.5, 0.5, -0.5, -0.5), Quat::from_xyzw(0.707_106_77, 0.707_106_77, 0.0, 0.0)];
    all.extend((0..50_000).map(|_| dice.quat()));
    for q in all {
        let bytes = written(|w| w.quat(q));
        assert_eq!(bytes.len(), 6);
        let back = Reader::new(&bytes).quat().expect("a rotation");
        assert!((back.length() - 1.0).abs() < 1e-5);
        worst = worst.max(angle_between(q, back));
    }
    println!("rotation in 48 bits: worst error {worst:.2e} rad ({:.4} degrees)", worst.to_degrees());
    assert!(worst < 1.75e-4, "{worst}");
    // The identity and the right angles are exact: a ship that stands straight stays straight.
    for exact in [Quat::IDENTITY, Quat::from_xyzw(1.0, 0.0, 0.0, 0.0), Quat::from_xyzw(0.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2)] {
        let back = Reader::new(&written(|w| w.quat(exact))).quat().expect("a rotation");
        assert!(angle_between(exact, back) < 1e-7, "{exact} came back {back}");
    }
    // What is no rotation goes as the identity instead of poisoning the other side.
    for odd in [Quat::from_xyzw(0.0, 0.0, 0.0, 0.0), Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0), Quat::from_xyzw(f32::INFINITY, 1.0, 0.0, 0.0)] {
        assert_eq!(Reader::new(&written(|w| w.quat(odd))).quat(), Ok(Quat::IDENTITY));
    }
}

fn close(a: &PlayerState, b: &PlayerState) {
    assert!((a.pos - b.pos).abs().max_element() <= 0.5 / POS_UNITS + 1e-9, "pos {} {}", a.pos, b.pos);
    assert!((a.local - b.local).abs().max_element() <= 0.5 / LOCAL_UNITS as f32 + 1e-5, "local");
    assert!((a.vel - b.vel).abs().max_element() <= 0.5 / PLAYER_VEL_UNITS as f32 + 1e-5, "vel");
    assert!((a.push - b.push).abs().max_element() <= 0.5 / PUSH_UNITS + 1e-5, "push {} {}", a.push, b.push);
    for (x, y) in [(a.yaw, b.yaw), (a.pitch, b.pitch), (a.head[0], b.head[0]), (a.head[1], b.head[1])] {
        assert!(x.sub_abs(y) <= ANGLE_STEP * 0.5 + 2e-6, "angle {x} {y}");
    }
    assert!((a.eye_h - b.eye_h).abs() <= 0.005 + 1e-6);
    assert_eq!((a.body, a.ride, a.flags, a.seat, a.tool, a.gesture, a.work.map(|w| w.0)), (b.body, b.ride, b.flags, b.seat, b.tool, b.gesture, b.work.map(|w| w.0)));
    if let (Some(x), Some(y)) = (a.work, b.work) {
        assert!((x.1 - y.1).abs().max_element() <= 0.5 / LOCAL_UNITS as f32 + 1e-5, "work");
    }
}

trait SubAbs {
    fn sub_abs(self, other: f32) -> f32;
}
impl SubAbs for f32 {
    /// The distance between two angles, the short way.
    fn sub_abs(self, other: f32) -> f32 {
        ((self - other + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).abs()
    }
}

#[test]
fn player_states_round_trip_within_their_quantisation() {
    let mut dice = Dice(19);
    for i in 0..5000 {
        let v3 = |dice: &mut Dice| Vec3::new(dice.signed() as f32, dice.signed() as f32, dice.signed() as f32);
        let mut p = PlayerState {
            body: if i % 3 == 0 { dice.below(5) as u8 } else { 0 },
            pos: DVec3::new(dice.signed(), dice.signed(), dice.signed()) * 1_800_000.0,
            ride: (i % 4 == 0).then(|| dice.below(1 << 40)),
            local: v3(&mut dice) * 60.0,
            yaw: dice.signed() as f32 * PI,
            pitch: (dice.signed() * 1.5) as f32,
            head: if i % 5 == 0 { [dice.signed() as f32, dice.signed() as f32 * 0.5] } else { [0.0; 2] },
            vel: if i % 2 == 0 { v3(&mut dice) * 30.0 } else { Vec3::ZERO },
            flags: dice.below(4096) as u16,
            seat: (i % 7 == 0).then(|| (dice.below(1 << 40), dice.below(8) as u8)),
            tool: if i % 6 == 0 { 1 + dice.below(20) as u8 } else { 0 },
            eye_h: 1.0 + dice.unit() as f32 * 0.8,
            push: if i % 8 == 0 { v3(&mut dice) } else { Vec3::ZERO },
            work: (i % 9 == 0).then(|| (dice.below(1 << 30), v3(&mut dice) * 40.0)),
            gesture: if i % 11 == 0 { 1 + dice.below(255) as u8 } else { 0 },
        };
        if p.ride.is_none() {
            p.local = Vec3::ZERO;
        }
        let bytes = written(|w| p.encode(w));
        let mut r = Reader::new(&bytes);
        let back = PlayerState::decode(&mut r).expect("a player");
        assert!(r.is_empty());
        close(&p, &back);
        // What came back is exactly representable: it survives another trip unchanged.
        let again = written(|w| back.encode(w));
        assert_eq!(bytes, again);
    }
}

#[test]
fn rigid_states_round_trip_within_their_quantisation() {
    let mut dice = Dice(23);
    let mut scratch = RigidState::default();
    for i in 0..3000 {
        let frame = match i % 5 {
            0 => Frame::Beside(dice.below(1 << 30)),
            1 => Frame::Aboard(dice.below(1 << 30)),
            _ => Frame::World,
        };
        let s = RigidState {
            id: dice.below(1 << 44),
            body: (i % 4 == 0) as u8 * 2,
            frame,
            pos: DVec3::new(dice.signed(), dice.signed(), dice.signed()) * if frame == Frame::World { 1_900_000.0 } else { 300.0 },
            rot: dice.quat(),
            vel: if i % 3 == 0 { Vec3::ZERO } else { Vec3::new(dice.signed() as f32, dice.signed() as f32, dice.signed() as f32) * 1700.0 },
            spin: if i % 3 == 0 { Vec3::ZERO } else { Vec3::new(dice.signed() as f32, dice.signed() as f32, dice.signed() as f32) * 2.0 },
            joints: (0..dice.below(41)).map(|_| (dice.signed() * 3.2) as f32).collect(),
            resting: i % 7 == 0 && i % 3 == 0,
        };
        let bytes = written(|w| s.encode(w));
        let b = RigidState::decode(&mut Reader::new(&bytes)).expect("a thing");
        assert_eq!((s.id, s.body, s.frame, s.joints.len()), (b.id, b.body, b.frame, b.joints.len()));
        assert!((s.pos - b.pos).abs().max_element() <= 0.5 / POS_UNITS + 1e-9);
        assert!(angle_between(s.rot, b.rot) < 1.75e-4);
        assert!((s.vel - b.vel).abs().max_element() <= 0.5 / RIGID_VEL_UNITS as f32 + 1e-4);
        assert!((s.spin - b.spin).abs().max_element() <= 0.5 / SPIN_UNITS as f32 + 1e-6);
        for (x, y) in s.joints.iter().zip(&b.joints) {
            assert!((x - y).abs() <= 0.5 / JOINT_UNITS as f32 + 1e-6);
        }
        // As it goes every tick: its hull and its joints apart, read over a state that keeps its id.
        let (hull, joints) = (written(|w| s.encode_rigid(w)), written(|w| s.encode_joints(w)));
        scratch.id = s.id;
        let mut r = Reader::new(&hull);
        scratch.decode_rigid(&mut r).expect("a hull");
        assert!(r.is_empty() && scratch.joints.is_empty());
        RigidState::decode_joints(&mut Reader::new(&joints), &mut scratch.joints).expect("joints");
        assert_eq!(scratch, b);
    }
    // More joints than the wire carries: the first ones go, and it still reads.
    let long = RigidState { joints: vec![0.5; 200], ..RigidState::default() };
    let back = RigidState::decode(&mut Reader::new(&written(|w| long.encode(w)))).expect("a thing");
    assert_eq!(back.joints.len(), lunar_net::game::MAX_JOINTS);
}

#[test]
fn sizes_on_the_wire() {
    let size = |p: &PlayerState| written(|w| p.encode(w)).len();
    let still = PlayerState { vel: Vec3::ZERO, ..walker(10.0) };
    let seated = PlayerState { ride: Some(2), local: Vec3::new(1.2, 2.4, -6.0), seat: Some((2, 0)), flags: flag::AT_CONTROLS | flag::GROUNDED, tool: 0, ..walker(10.0) };
    let busy = PlayerState { body: 1, tool: 3, head: [0.4, -0.2], flags: 0xfff, push: Vec3::new(0.2, 1.0, -0.4), work: Some((2, Vec3::new(1.0, 2.0, -3.0))), gesture: 4, ..seated };
    // (a gesture, a visor, a wrist computer raised: a byte or two more, and nothing when there is none)
    let waving = PlayerState { gesture: 2, flags: flag::GROUNDED | flag::VISOR | flag::WRIST, ..still };
    assert_eq!(size(&waving), size(&still) + 3);
    let flying = PlayerState { flags: flag::PACK | flag::THRUSTING, push: Vec3::Y, ..walker(10.0) };
    println!("PlayerState: walking on the Moon {} bytes, standing still {}, seated at the controls of a ship {}, on the pack {}, everything at once {}", size(&walker(10.0)), size(&still), size(&seated), size(&flying), size(&busy));
    let size = |s: &RigidState| written(|w| s.encode(w)).len();
    let orbit = RigidState { vel: Vec3::new(1200.0, 300.0, -1100.0), ..ship(3, 50.0, 20) };
    let parked = RigidState { vel: Vec3::ZERO, spin: Vec3::ZERO, joints: vec![], ..ship(3, 50.0, 0) };
    println!(
        "RigidState: flying low with 20 joints {} bytes, at orbital speed with 20 joints {}, with 40 joints {}, without joints {}, parked without joints {}",
        size(&ship(3, 50.0, 20)),
        size(&orbit),
        size(&ship(3, 50.0, 40)),
        size(&ship(3, 50.0, 0)),
        size(&parked)
    );
    let rigid = |s: &RigidState| written(|w| s.encode_rigid(w)).len();
    // (beside another ship at orbital speed: what is left is how they move against each other)
    let beside = RigidState { frame: Frame::Beside(2), pos: DVec3::new(40.0, -3.0, 120.0), vel: Vec3::new(0.4, 0.0, -1.5), ..ship(3, 50.0, 0) };
    let sliding = RigidState { vel: Vec3::new(0.2, 0.0, 0.1), ..cargo(900, 2, Vec3::new(1.5, 0.4, -6.0)) };
    println!(
        "RigidState without its joints, as it goes every tick: flying low {} bytes, at orbital speed {}, beside another ship {}, parked {}, a crate sliding in a hold {}, lying in it {}; the joints apart, when they change: 20 joints {} bytes",
        rigid(&ship(3, 50.0, 20)),
        rigid(&orbit),
        rigid(&beside),
        rigid(&parked),
        rigid(&sliding),
        rigid(&cargo(900, 2, Vec3::new(1.5, 0.4, -6.0))),
        written(|w| ship(3, 50.0, 20).encode_joints(w)).len()
    );
    println!("datagram header {} bytes (plus 28 of IP and UDP)", lunar_net::channel::HEADER);
    assert_eq!(size(&ship(3, 50.0, 20)), 74);
    assert_eq!(rigid(&ship(3, 50.0, 20)), 33);
    assert!(rigid(&beside) <= 26 && rigid(&cargo(900, 2, Vec3::new(1.5, 0.4, -6.0))) <= 16, "a state told in a ship's frame is small numbers");
    assert_eq!(written(|w| walker(10.0).encode(w)).len(), 26);
    assert_eq!(written(|w| still.encode(w)).len(), 22);
    // what the game says costs what it says plus this
    for (what, m) in [("said reliably", Msg::Game(&[0; 12])), ("said quickly", Msg::Quick(&[0; 12]))] {
        println!("{what}: {} bytes as a message", written(|w| m.encode(w)).len());
    }
}

#[test]
fn datagrams_and_messages_round_trip() {
    let mut buf = [0u8; 1200];
    for d in [
        Datagram::Hello { version: VERSION, salt: 0xDEAD_BEEF, cookie: 0, key: [0x33; 32], scenario: 7, build: "V36", name: "Añil" },
        Datagram::Hello { version: VERSION, salt: 1, cookie: 0xFEED_FACE_CAFE_F00D, key: [0x33; 32], scenario: 0xffff_ffff, build: "", name: "" },
        Datagram::Challenge { salt: 0xDEAD_BEEF, cookie: 0xFEED_FACE_CAFE_F00D },
        Datagram::Welcome { salt: 5, id: 3, key: [0x44; 32], name: "Añil", server: "Servidor de Selene" },
        Datagram::Refused { salt: 5, reason: "el servidor está lleno (16 de 16 jugadores)" },
        Datagram::Bye { salt: 9, reason: "" },
        Datagram::Data(&[1, 2, 3]),
    ] {
        let n = d.encode(&mut buf);
        assert!(n > 0);
        assert_eq!(Datagram::decode(&buf[..n]), Ok(d));
    }
    // A hello is always the same size, larger than any answer to it; a shorter one is not a hello.
    let n = Datagram::Hello { version: VERSION, salt: 77, cookie: 5, key: [0x33; 32], scenario: 7, build: "V36", name: "x" }.encode(&mut buf);
    assert_eq!(n, lunar_net::proto::HELLO_SIZE);
    assert_eq!(Datagram::decode(&buf[..n - 1]), Err(WireError::Short));
    // A hello of another version (V35's was 1) is recognised as such, whatever follows it.
    for other in [1, VERSION + 1] {
        let n = Datagram::Hello { version: other, salt: 77, cookie: 5, key: [0x33; 32], scenario: 7, build: "V35", name: "x" }.encode(&mut buf);
        buf[11..40].fill(0xff);
        assert_eq!(Datagram::decode(&buf[..n]), Ok(Datagram::Hello { version: other, salt: 77, cookie: 0, key: [0; 32], scenario: 0, build: "", name: "" }));
    }
    assert_eq!(VERSION, 4);
    // the refusal of a game of another version fits the answer to a hello, and says which is which
    let why = lunar_net::text::version(VERSION, 1);
    assert!(why.contains("V35") && why.contains("actualiza el juego"), "{why}");
    let n = Datagram::Refused { salt: 1, reason: &why }.encode(&mut buf[..lunar_net::proto::HELLO_SIZE]);
    assert!(n > 0, "the refusal fits a hello's size");

    let data = [7u8, 0, 255, 3, 9];
    for m in [
        Msg::Ping { t: 123_456_789 },
        Msg::Pong { t: 123_456_789, server: 987_654_321_000 },
        Msg::Game(&data),
        Msg::Game(&[]),
        Msg::Quick(&data),
        Msg::Chat { text: "hola" },
        Msg::Joined { id: 9, name: "Añil" },
        Msg::Left { id: 9 },
        Msg::Said { from: None, text: "aviso" },
        Msg::Said { from: Some(4), text: "hola" },
        Msg::Synced,
        Msg::Bundle(&[1, 14]),
    ] {
        let bytes = written(|w| m.encode(w));
        assert_eq!(Msg::decode(&bytes), Ok(m));
    }
    // Bytes left over after a message make it no message.
    let mut bytes = written(|w| Msg::Left { id: 9 }.encode(w));
    bytes.push(0);
    assert_eq!(Msg::decode(&bytes), Err(WireError::Long));
}

#[test]
fn bundles_hold_messages_in_order() {
    let msgs = [written(|w| Msg::Joined { id: 1, name: "a" }.encode(w)), written(|w| Msg::Left { id: 1 }.encode(w)), written(|w| Msg::Synced.encode(w))];
    let mut bundle = Bundle::begin();
    msgs.iter().for_each(|m| Bundle::push(&mut bundle, m));
    let Ok(Msg::Bundle(body)) = Msg::decode(&bundle) else { panic!("a bundle") };
    let back: Vec<&[u8]> = Bundle::new(body).map(|m| m.expect("whole")).collect();
    assert_eq!(back, msgs.iter().map(|m| &m[..]).collect::<Vec<_>>());
    // Cut short, it gives what is whole and then one error, and ends.
    let cut: Vec<_> = Bundle::new(&body[..body.len() - 1]).collect();
    assert_eq!(cut.len(), 3);
    assert!(cut[0].is_ok() && cut[1].is_ok() && cut[2].is_err());
}

/// Every decoder there is, fed `bytes`. None may panic; what they return does not matter.
fn feed_everything(bytes: &[u8], channel: &mut Channel, inbox: &mut Inbox, scratch: &mut RigidState) {
    let mut r = Reader::new(bytes);
    let _ = (r.u8(), r.u16(), r.u32(), r.u64(), r.var(), r.var32(), r.var16(), r.zig(), r.f32(), r.f64(), r.str(16), r.vec3(), r.angle(), r.fixed(POS_UNITS), r.fixed3(POS_UNITS), r.quat());
    let _ = Reader::new(bytes).str(1000);
    let _ = Reader::new(bytes).quat();
    let _ = PlayerState::decode(&mut Reader::new(bytes));
    let _ = RigidState::decode(&mut Reader::new(bytes));
    let _ = scratch.decode_rigid(&mut Reader::new(bytes));
    let _ = RigidState::decode_joints(&mut Reader::new(bytes), &mut scratch.joints);
    let _ = Datagram::decode(bytes);
    if let Ok(Msg::Bundle(body)) = Msg::decode(bytes) {
        Bundle::new(body).for_each(drop);
    }
    Bundle::new(bytes).for_each(drop);
    inbox.clear();
    let _ = channel.receive(bytes, 1.0, inbox);
    for (_, msg) in inbox.iter() {
        let _ = Msg::decode(msg);
    }
}

#[test]
fn garbage_never_panics() {
    let mut dice = Dice(29);
    let mut buf = [0u8; 300];
    let (mut inbox, mut scratch) = (Inbox::new(), RigidState::default());
    let mut channel = Channel::new(0.0, 4);
    for round in 0..200_000 {
        let n = dice.below(if round % 10 == 0 { 300 } else { 40 }) as usize;
        dice.bytes(&mut buf[..n]);
        // Random first bytes rarely name a real message: steer some rounds into each decoder.
        if n > 0 && round % 3 == 0 {
            buf[0] = dice.below(22) as u8;
        }
        if round % 5000 == 0 {
            channel = Channel::new(0.0, 4);
        }
        feed_everything(&buf[..n], &mut channel, &mut inbox, &mut scratch);
    }
}

#[test]
fn real_messages_cut_or_flipped_never_panic() {
    let mut dice = Dice(31);
    let state = written(|w| ship(4, 1.0, 30).encode(w));
    let (rigid, joints) = (written(|w| cargo(4, 9, Vec3::ONE).encode_rigid(w)), written(|w| ship(4, 1.0, 30).encode_joints(w)));
    let player = written(|w| PlayerState { ride: Some(1), seat: Some((1, 0)), tool: 2, head: [0.1, 0.2], body: 3, push: Vec3::Y, work: Some((4, Vec3::ONE)), gesture: 3, ..walker(1.0) }.encode(w));
    let mut hello = [0u8; 400];
    let n = Datagram::Hello { version: VERSION, salt: 1, cookie: 9, key: [0x33; 32], scenario: 4, build: "V36", name: "Añil" }.encode(&mut hello);
    let samples: Vec<Vec<u8>> = vec![
        state.clone(),
        joints.clone(),
        rigid.clone(),
        player.clone(),
        hello[..n].to_vec(),
        written(|w| Msg::Game(&state).encode(w)),
        written(|w| Msg::Said { from: Some(2), text: "hola" }.encode(w)),
        written(|w| Msg::Joined { id: 3, name: "Añil" }.encode(w)),
    ];
    let (mut inbox, mut scratch) = (Inbox::new(), RigidState::default());
    let mut channel = Channel::new(0.0, 4);
    for sample in &samples {
        for cut in 0..=sample.len() {
            feed_everything(&sample[..cut], &mut channel, &mut inbox, &mut scratch);
            // The same without its tag byte, as the batch readers take it.
            feed_everything(&sample[cut.min(1)..], &mut channel, &mut inbox, &mut scratch);
        }
        for _ in 0..3000 {
            let mut flipped = sample.clone();
            for _ in 0..1 + dice.below(3) {
                let at = dice.below(flipped.len() as u64) as usize;
                flipped[at] ^= 1 << dice.below(8);
            }
            feed_everything(&flipped, &mut channel, &mut inbox, &mut scratch);
            feed_everything(&flipped[1..], &mut channel, &mut inbox, &mut scratch);
        }
    }
}
