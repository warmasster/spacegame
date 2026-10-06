//! What a sound comes through.
//!
//! Sound needs something to travel in. Through the air it is as loud as the air is dense: a
//! source moving so much pushes on thin air with less (the pressure it makes goes with the air's
//! ρ·c, and at one temperature that goes with its pressure), and the ear, which hears ratios,
//! takes a little of that back — so the level here goes with pressure to the 0.75, 1 at a
//! standard atmosphere and nothing at all in a vacuum. A third of an atmosphere (a spacecraft's
//! cabin) is 0.44 of it: thin, not silent.
//!
//! What the body touches still reaches it in a vacuum, through boots, gloves and the suit's own
//! shell: only its low end survives the trip (a second-order low-pass a few hundred hertz up),
//! which is why a hammer on a hull is a thud in the helmet.

/// A standard atmosphere (kPa).
pub const ATMOSPHERE: f32 = 101.325;

/// How much of a sound in the air reaches the ear at this pressure (kPa): 0 in a vacuum, 1 at a
/// standard atmosphere.
pub fn air_gain(kpa: f32) -> f32 {
    (kpa / ATMOSPHERE).clamp(0.0, 1.0).powf(0.75)
}

/// Where the body's own path stops passing (Hz): through a pressure suit, boots and gloves.
pub const CONTACT_HZ: f32 = 650.0;
/// How much of a sound in the air also comes through what the body touches, when it touches its
/// source (a switch under the finger).
pub const TOUCH_SHARE: f32 = 0.35;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_in_a_vacuum_all_of_it_at_sea_level_and_less_in_a_thin_cabin() {
        assert_eq!(air_gain(0.0), 0.0);
        assert!((air_gain(ATMOSPHERE) - 1.0).abs() < 1e-6);
        assert_eq!(air_gain(500.0), 1.0);
        let cabin = air_gain(34.0);
        assert!(cabin > 0.4 && cabin < 0.5, "{cabin}");
        // thinner is always quieter
        assert!(air_gain(5.0) < air_gain(20.0) && air_gain(20.0) < air_gain(70.0));
    }
}
