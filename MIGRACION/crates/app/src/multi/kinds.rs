//! The registry of what the games tell each other: one row per kind of thing told, saying how it
//! travels, who may say it and how whoever comes late gets what it changed. Everything `multi`
//! sends goes through `Multi::tell` with a kind of this table and comes in through
//! `Multi::told`, which looks its row up before doing anything: a new kind is a row here, the
//! bytes it carries (`tell.rs`: what is written and what is done with it) and nothing in the
//! server, which passes all of it on unread.
//!
//! The table is also what docs/MULTIJUGADOR.md lists (a test keeps the two in step).

/// How a kind travels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// To everyone else, reliably and in order.
    Sure,
    /// The same, and back to whoever told it in its place among what everyone told: for what
    /// must be done in one order everywhere, or by whoever holds the thing when it arrives.
    Echo,
    /// To everyone else, once: it may be lost (what is over in a moment anyway).
    Loose,
    /// To one player, reliably and in order.
    One,
}

/// Who may say it (anything else is dropped where it arrives).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    Anyone,
    /// Whoever holds the thing it names (who simulates it): its first field is that thing.
    Holder,
}

/// How whoever comes late gets what it changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Late {
    /// It is in the world's snapshot (the host's, on joining): what it did is there.
    World,
    /// It is over when it has been seen: nothing to catch up with.
    Nothing,
}

pub struct Kind {
    pub id: u8,
    /// Its name in the documents.
    pub name: &'static str,
    pub way: Way,
    pub by: By,
    pub late: Late,
}

pub const CONTROL: u8 = 1;
pub const ACT: u8 = 2;
pub const HIT: u8 = 3;
pub const DELTA: u8 = 4;
pub const MADE: u8 = 5;
pub const GONE: u8 = 6;
pub const HOLD: u8 = 7;
pub const DROP: u8 = 8;
pub const REST: u8 = 9;
pub const SEEN: u8 = 10;
pub const PART: u8 = 11;
pub const DIGEST: u8 = 12;
pub const ASK: u8 = 13;
pub const MARK: u8 = 14;
pub const WORLD: u8 = 15;
pub const SNAP: u8 = 16;

pub const KINDS: &[Kind] = &[
    // a control of a ship's panels left at a value by a hand (or a seat's keys)
    Kind { id: CONTROL, name: "mando", way: Way::Sure, by: By::Anyone, late: Late::World },
    // a hand on a door or on a clamp's lever
    Kind { id: ACT, name: "mano", way: Way::Sure, by: By::Anyone, late: Late::World },
    // a hit (a shot, a blast) on a structure, where it struck in the structure's own frame: done by whoever holds it
    Kind { id: HIT, name: "impacto", way: Way::Echo, by: By::Anyone, late: Late::World },
    // what is left of the parts and joints of a structure that changed
    Kind { id: DELTA, name: "daño", way: Way::Sure, by: By::Holder, late: Late::World },
    // a structure that was not there: a ship or a build set in play, a piece come off another
    Kind { id: MADE, name: "hecho", way: Way::Sure, by: By::Anyone, late: Late::World },
    // a structure that is no more
    Kind { id: GONE, name: "quitado", way: Way::Sure, by: By::Holder, late: Late::World },
    // a structure taken by another's clamp, magnet, crane or cradle (the first to take it has it)
    Kind { id: HOLD, name: "sujeto", way: Way::Echo, by: By::Anyone, late: Late::World },
    // ... and let go, with the speed it had
    Kind { id: DROP, name: "suelto", way: Way::Sure, by: By::Anyone, late: Late::World },
    // a structure come to rest: where
    Kind { id: REST, name: "reposo", way: Way::Sure, by: By::Holder, late: Late::World },
    // a round or a missile fired, an explosion gone off: to be seen (what it does is `impacto`)
    Kind { id: SEEN, name: "visto", way: Way::Sure, by: By::Anyone, late: Late::Nothing },
    // a tool on a part: mended, put back (done by whoever holds its structure)
    Kind { id: PART, name: "pieza", way: Way::Echo, by: By::Anyone, late: Late::World },
    // a digest of a structure (and its ship): whoever has another asks for the rest
    Kind { id: DIGEST, name: "resumen", way: Way::Sure, by: By::Holder, late: Late::Nothing },
    // "tell me": the world (on joining) or one structure (a digest that did not match)
    Kind { id: ASK, name: "petición", way: Way::Sure, by: By::Anyone, late: Late::Nothing },
    // a record of the world that is no structure, put or taken away (the sun, the clock, a beacon)
    Kind { id: MARK, name: "marca", way: Way::Sure, by: By::Anyone, late: Late::World },
    // the world as the host has it, in pieces, to whoever asked
    Kind { id: WORLD, name: "mundo", way: Way::One, by: By::Anyone, late: Late::Nothing },
    // a structure (and its ship) as whoever holds it has it, to whoever asked
    Kind { id: SNAP, name: "instantánea", way: Way::One, by: By::Anyone, late: Late::Nothing },
];

pub fn kind(id: u8) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.id == id)
}

/// The kinds of records of the world that are no structures (`Multi::put`): each a number here
/// and whatever bytes its owner writes. The server and `multi` read none of them.
pub mod mark {
    /// The sun as the menu left it: azimuth and elevation (degrees, two floats).
    pub const SUN: u8 = 1;
    /// The world's clock: what it said (s, a double) when the server's said (s, a double).
    pub const CLOCK: u8 = 2;
    /// From here on, the game's own (beacons, signs...): `multi` only keeps and passes them.
    pub const GAME: u8 = 16;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_one_row_and_the_document_lists_it() {
        for (n, k) in KINDS.iter().enumerate() {
            assert!(KINDS[..n].iter().all(|o| o.id != k.id && o.name != k.name), "{} dos veces", k.name);
            assert_eq!(kind(k.id).map(|x| x.name), Some(k.name));
            // what only its holder may say names its thing first; what goes to one player is not for all
            assert!(k.way != Way::One || k.late == Late::Nothing);
        }
        assert!(kind(0).is_none() && kind(200).is_none());
        let doc = std::fs::read_to_string(crate::root().join("docs/MULTIJUGADOR.md")).unwrap_or_default();
        for k in KINDS {
            assert!(doc.contains(&format!("`{}`", k.name)), "docs/MULTIJUGADOR.md no lista la clase «{}»", k.name);
        }
    }
}
