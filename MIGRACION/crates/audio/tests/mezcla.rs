//! The mixer, rendered to a buffer (no sound card):
//! - every sound of the game compiles and sounds, and a blow frees its voice when it ends;
//! - a vacuum is silent but for what one touches (dull) and the suit;
//! - levels move without clicks and nothing leaves the mix over its ceiling;
//! - with every voice busy the oldest blow makes room;
//! - the alarm takes turns between its two tones;
//! - a full mixer costs a small share of one core.
use lunar_audio::{
    Cmd, LOOPS, Mixer, Sound, SoundDef, SoundDefs, VOICES,
    medium::{ATMOSPHERE, air_gain},
};
use std::{path::Path, time::Instant};

const RATE: f32 = 48_000.0;

fn game() -> SoundDefs {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/sounds.jsonc");
    lunar_core::defs::load(&path).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message))
}

fn parse(text: &str) -> SoundDef {
    lunar_core::defs::parse("prueba", text).unwrap_or_else(|e| panic!("{}", e.message))
}

struct Rig {
    mixer: Mixer,
    tx: rtrb::Producer<Cmd>,
    ids: Vec<String>,
}

impl Rig {
    fn new(defs: &SoundDefs) -> Rig {
        let sounds: Vec<Sound> = defs.iter().map(|(id, d)| Sound::new(d, RATE).unwrap_or_else(|e| panic!("{id}: {e}"))).collect();
        let (tx, rx) = rtrb::RingBuffer::new(256);
        let mut rig = Rig { mixer: Mixer::new(sounds.into(), RATE, rx), tx, ids: defs.keys().cloned().collect() };
        // (a sea-level cabin unless a test says otherwise; settled)
        rig.send(Cmd::Air(air_gain(ATMOSPHERE)));
        rig.render(0.1);
        rig
    }

    fn id(&self, name: &str) -> u16 {
        self.ids.iter().position(|i| i == name).unwrap_or_else(|| panic!("no hay sonido '{name}'")) as u16
    }

    fn send(&mut self, c: Cmd) {
        self.tx.push(c).unwrap();
    }

    /// `secs` of stereo, interleaved.
    fn render(&mut self, secs: f32) -> Vec<f32> {
        let mut out = vec![0.0; (secs * RATE) as usize * 2];
        // (in the sizes a sound card asks for)
        for chunk in out.chunks_mut(2 * 441) {
            self.mixer.fill(chunk, 2);
        }
        out
    }
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// How bright a signal is: the energy of its sample-to-sample change over its own (0: a steady
/// level, 4: the highest pitch there is).
fn brightness(x: &[f32]) -> f32 {
    let left: Vec<f32> = x.iter().step_by(2).copied().collect();
    let d: f32 = left.windows(2).map(|w| (w[1] - w[0]) * (w[1] - w[0])).sum();
    d / left.iter().map(|v| v * v).sum::<f32>().max(1e-12)
}

#[test]
fn every_sound_of_the_game_compiles_and_sounds() {
    let defs = game();
    assert!(defs.len() >= 16, "{} sonidos", defs.len());
    let mut rig = Rig::new(&defs);
    for (k, (id, d)) in defs.iter().enumerate() {
        let out = match d.dura {
            Some(t) => {
                rig.send(Cmd::Play { sound: k as u16, gain: 1.0, pitch: 1.0, pan: 0.0, touch: true });
                let out = rig.render(t + 0.05);
                assert_eq!(rig.mixer.busy, 0, "{id}: acabado y sigue ocupando una voz");
                out
            }
            None => {
                rig.send(Cmd::Loop { slot: 0, sound: k as u16, gain: 1.0, pitch: 1.0, touch: true });
                let out = rig.render(0.6);
                assert_eq!(rig.mixer.busy, 1, "{id}: un bucle sostenido no suena");
                rig.send(Cmd::Loop { slot: 0, sound: k as u16, gain: 0.0, pitch: 1.0, touch: true });
                rig.render(0.3);
                assert_eq!(rig.mixer.busy, 0, "{id}: soltado y sigue sonando");
                out
            }
        };
        let (level, peak) = (rms(&out), out.iter().fold(0.0f32, |m, v| m.max(v.abs())));
        eprintln!("{id}: {} nivel {level:.4}, pico {peak:.3}", if d.dura.is_some() { "golpe" } else { "bucle" });
        assert!(out.iter().all(|v| v.is_finite()), "{id}: no es un número");
        assert!(level > 0.002, "{id}: no se oye ({level})");
        assert!(peak < 0.95, "{id}: satura él solo ({peak})");
    }
}

#[test]
fn a_vacuum_is_silent_but_for_what_you_touch_and_the_suit() {
    let defs = game();
    let mut rig = Rig::new(&defs);
    let (click, fan) = (rig.id("interruptor"), rig.id("ventilador"));
    let play = |rig: &mut Rig, touch: bool| {
        rig.send(Cmd::Play { sound: click, gain: 1.0, pitch: 1.0, pan: 0.0, touch });
        rig.render(0.12)
    };
    // in a cabin: heard as it is
    let cabin = play(&mut rig, false);
    assert!(rms(&cabin) > 0.005);
    // in a vacuum, across the room: nothing at all
    rig.send(Cmd::Air(air_gain(0.0)));
    rig.render(0.3);
    let far = play(&mut rig, false);
    assert_eq!(rms(&far), 0.0, "un chasquido se oye en el vacío sin tocarlo");
    // under the finger: there, and dull
    let touched = play(&mut rig, true);
    assert!(rms(&touched) > 0.0005, "lo que se toca no llega ({})", rms(&touched));
    assert!(rms(&touched) < rms(&cabin), "tocado en el vacío suena más que en el aire");
    assert!(brightness(&touched) < brightness(&cabin) * 0.25, "lo que llega por el guante no es sordo: {} contra {}", brightness(&touched), brightness(&cabin));
    // the suit's own: the same with air or without
    let hold = |rig: &mut Rig| {
        rig.send(Cmd::Loop { slot: 1, sound: fan, gain: 1.0, pitch: 1.0, touch: false });
        rig.render(0.5);
        rms(&rig.render(1.0))
    };
    let in_vacuum = hold(&mut rig);
    rig.send(Cmd::Air(air_gain(ATMOSPHERE)));
    rig.render(0.3);
    let in_air = hold(&mut rig);
    assert!(in_vacuum > 0.002 && (in_vacuum / in_air - 1.0).abs() < 0.2, "el ventilador del traje: {in_vacuum} en el vacío, {in_air} en el aire");
    // and a thin cabin is quieter than a thick one
    assert!(air_gain(34.0) < 0.5 && air_gain(34.0) > 0.4);
}

#[test]
fn levels_move_without_clicks_and_nothing_leaves_over_the_ceiling() {
    let mut defs = SoundDefs::new();
    defs.insert("tono".into(), parse(r#"{ "via": "traje", "capas": [ { "onda": "seno", "hz": 200 } ] }"#));
    defs.insert("golpe".into(), parse(r#"{ "via": "traje", "dura": 0.3, "capas": [ { "onda": "cuadrada", "hz": 150 } ] }"#));
    let mut rig = Rig::new(&defs);
    let (blow, tone) = (rig.id("golpe"), rig.id("tono"));
    // a tone switched full on, then off: the steepest step is hardly more than the tone's own
    rig.send(Cmd::Loop { slot: 0, sound: tone, gain: 0.8, pitch: 1.0, touch: false });
    let mut out = rig.render(0.2);
    rig.send(Cmd::Loop { slot: 0, sound: tone, gain: 0.0, pitch: 1.0, touch: false });
    out.extend(rig.render(0.2));
    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    let step = left.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let own = 0.8 * 0.707 * std::f32::consts::TAU * 200.0 / RATE;
    assert!(step < own * 1.6, "un salto de {step} (el tono solo da {own})");
    assert!(rms(&rig.render(0.1)) < 1e-4, "apagado y sigue sonando");
    // twenty loud blows at once: over nothing
    for _ in 0..20 {
        rig.send(Cmd::Play { sound: blow, gain: 1.5, pitch: 1.0, pan: 0.0, touch: false });
    }
    let loud = rig.render(0.3);
    let peak = loud.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak <= 1.0 && peak > 0.9, "pico {peak}");
}

#[test]
fn with_every_voice_busy_the_oldest_blow_makes_room() {
    let mut defs = SoundDefs::new();
    defs.insert("largo".into(), parse(r#"{ "via": "traje", "dura": 2.0, "ganancia": 0.02, "capas": [ { "onda": "seno", "hz": 300 } ] }"#));
    let mut rig = Rig::new(&defs);
    for k in 0..(VOICES + 10) {
        rig.send(Cmd::Play { sound: 0, gain: 1.0, pitch: 1.0 + k as f32 * 0.01, pan: 0.0, touch: false });
        rig.render(0.01);
    }
    assert_eq!(rig.mixer.busy, VOICES - LOOPS);
    // a loop asked of a slot that is not one, a sound that is not there: nothing happens
    rig.send(Cmd::Loop { slot: 200, sound: 0, gain: 1.0, pitch: 1.0, touch: false });
    rig.send(Cmd::Play { sound: 999, gain: 1.0, pitch: 1.0, pan: 0.0, touch: false });
    rig.render(0.05);
    assert_eq!(rig.mixer.busy, VOICES - LOOPS);
    rig.render(2.1);
    assert_eq!(rig.mixer.busy, 0);
}

#[test]
fn the_alarm_takes_turns_between_its_two_tones() {
    let defs = game();
    let mut rig = Rig::new(&defs);
    let alarm = rig.id("alarma");
    rig.send(Cmd::Loop { slot: 0, sound: alarm, gain: 1.0, pitch: 1.0, touch: false });
    rig.render(0.2);
    let out = rig.render(1.6);
    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    // pitch in windows of a twentieth of a second, by its zero crossings
    let w = (RATE * 0.05) as usize;
    let tones: Vec<f32> = left.chunks(w).filter(|c| c.len() == w).map(|c| c.windows(2).filter(|p| p[0] <= 0.0 && p[1] > 0.0).count() as f32 / 0.05).collect();
    let (low, high) = (tones.iter().filter(|t| (**t - 375.0).abs() < 60.0).count(), tones.iter().filter(|t| (**t - 1000.0).abs() < 100.0).count());
    let turns = tones.windows(2).filter(|p| (p[1] - p[0]).abs() > 400.0).count();
    eprintln!("alarma: {low} ventanas a 375 Hz, {high} a 1000 Hz, {turns} cambios en 1,6 s: {tones:?}");
    assert!(low >= 10 && high >= 10, "{low} graves y {high} agudas");
    assert!((6..=10).contains(&turns), "{turns} cambios (2,5 ciclos por segundo: 8 en 1,6 s)");
}

#[test]
fn a_full_mixer_costs_a_small_share_of_one_core() {
    let defs = game();
    let mut rig = Rig::new(&defs);
    let loops: Vec<u16> = defs.iter().enumerate().filter(|(_, (_, d))| d.dura.is_none()).map(|(k, _)| k as u16).collect();
    let blows: Vec<u16> = defs.iter().enumerate().filter(|(_, (_, d))| d.dura.is_some()).map(|(k, _)| k as u16).collect();
    let mut spent = 0.0;
    let secs = 4.0;
    for round in 0..(secs / 0.05) as usize {
        // every loop held, and blows faster than they end
        for slot in 0..LOOPS {
            rig.send(Cmd::Loop { slot: slot as u8, sound: loops[slot % loops.len()], gain: 0.2, pitch: 1.0, touch: true });
        }
        for k in 0..6 {
            rig.send(Cmd::Play { sound: blows[(round + k) % blows.len()], gain: 0.2, pitch: 1.0, pan: 0.0, touch: true });
        }
        let t = Instant::now();
        rig.render(0.05);
        spent += t.elapsed().as_secs_f64();
    }
    let share = spent / f64::from(secs);
    eprintln!("mezclador lleno ({} voces): {:.2} % de un núcleo", rig.mixer.busy, share * 100.0);
    assert!(rig.mixer.busy >= VOICES - 4, "{} voces", rig.mixer.busy);
    assert!(share < 0.05, "{:.1} % de un núcleo", share * 100.0);
}
