//! What flies and goes off, as it is drawn: every round, missile and guided missile in flight as
//! a particle of its style, the effects' particles, and the flashes as lights. What they do is
//! the game's (`lunar_play::blasts`).
use glam::DVec3;
use lunar_core::{body::BodyRegistry, particles::Particle};
use lunar_play::blasts::Blasts;
use lunar_render::{Light, Renderer};
use winit::keyboard::KeyCode;

/// This frame's particles and lights to the renderer, seen from `eye`: where the picture is
/// taken from in the end (the player's eyes, or a camera outside: the particles are sent
/// relative to it, so the eye of `update` will not do). The picture is `lag` s behind the
/// game's last step (drawn between steps): what flies is drawn where it was then.
pub fn draw(b: &mut Blasts, lights: &mut Vec<Light>, r: &mut Renderer, eye: DVec3, bodies: &BodyRegistry, lag: f64) {
    lights.clear();
    lights.extend(b.fx.flashes().iter().map(|f| {
        let i = f.now();
        Light { pos: f.pos - f.vel * lag, color: [f.color.x * i, f.color.y * i, f.color.z * i], range: f.range, ..Light::default() }
    }));
    b.rounds.looks(&mut b.looks);
    for m in b.missiles.list.iter().chain(&b.guests.list) {
        if let Some((style, size)) = b.missile_looks[m.kind] {
            b.looks.push(Particle { pos: m.pos, drift: DVec3::ZERO, vel: m.vel.as_vec3(), age: 0.0, life: 1.0, size, seed: 0.5, ground: 0.0, height: 1e6, body: bodies.dominant(m.pos), style });
        }
    }
    for m in &b.guided.list {
        if let Some((style, size)) = b.guided_looks[usize::from(m.kind)] {
            b.looks.push(Particle { pos: m.pos, drift: DVec3::ZERO, vel: m.vel.as_vec3(), age: 0.0, life: 1.0, size, seed: 0.5, ground: 0.0, height: 1e6, body: bodies.dominant(m.pos), style });
        }
    }
    r.set_effects(&b.fx.particles, &b.looks, lights, eye, lag);
}

const LETTERS: [KeyCode; 26] = {
    use KeyCode::*;
    [KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ]
};
const DIGITS: [KeyCode; 10] = {
    use KeyCode::*;
    [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9]
};

/// The test key a key code is (`lunar_play::blasts::key_name`): its letter or digit.
pub fn key_char(key: KeyCode) -> Option<char> {
    if let Some(i) = LETTERS.iter().position(|k| *k == key) {
        return Some((b'A' + i as u8) as char);
    }
    DIGITS.iter().position(|k| *k == key).map(|i| (b'0' + i as u8) as char)
}

/// The key code of a test key (`key_char` the other way).
pub fn key_code(c: char) -> Option<KeyCode> {
    match c {
        'A'..='Z' => Some(LETTERS[c as usize - 'A' as usize]),
        '0'..='9' => Some(DIGITS[c as usize - '0' as usize]),
        _ => None,
    }
}

/// None of the test keys is one of the player's own (moving, crouching, the lamp...).
pub fn check_keys(b: &Blasts) -> Result<(), String> {
    for c in b.keys() {
        if key_code(c).is_some_and(crate::input::taken) {
            return Err(format!("la tecla {c} ya es del jugador (mover, agacharse, linterna...)"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keys_go_both_ways() {
        for c in ('A'..='Z').chain('0'..='9') {
            assert_eq!(key_char(key_code(c).unwrap()), Some(c));
        }
        assert_eq!(key_char(KeyCode::F1), None);
    }

    #[test]
    fn no_test_key_is_the_players() {
        let d = crate::content::Defs::load(&crate::root().join("assets/defs")).unwrap_or_else(|e| panic!("{e}"));
        let b = Blasts::new(&d.effects, lunar_core::missiles::Missiles::new(d.missiles.clone()), 1000).unwrap_or_else(|e| panic!("{e}"));
        check_keys(&b).unwrap();
    }
}
