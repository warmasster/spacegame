//! What a hand (or a pad, or the network) asks of a control, what the world allows, and what the
//! control answers: its new value, a pose for its model and an event for sound and the HUD.

/// Modifiers of an intent: Shift is coarse, Ctrl is fine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub coarse: bool,
    pub fine: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Intent {
    /// Press (on sub-element `elem`, 0 for the whole control: keypad keys, bezel buttons).
    Press { elem: u8 },
    Release,
    /// Kept pressed for `secs` (pull-to-move, hold-to-arm).
    Hold { secs: f32 },
    /// Wheel or pad: notches (signed) at `rate` notches per second.
    Turn { notches: f32, rate: f32, m: Mods },
    /// Drag (levers, big wheels): screen fraction moved on each axis.
    Drag { dx: f32, dy: f32, m: Mods },
    /// A character typed on a keypad.
    Type(char),
    Hover(bool),
    /// Set a value directly (network replay, bindings of a seat): SI.
    Set { value: f64 },
    /// Set one axis of a lever (seat bindings: the stick follows the keys): SI.
    Axis { axis: u8, value: f64 },
}

/// Who acts: the crew role and what they carry (keys).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Any,
    Crew,
    Officer,
}

/// What the world lets a control do now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gate {
    /// Share of the power it needs that it gets (1 if it needs none).
    pub supply: f32,
    /// Its part works (not destroyed, not jammed).
    pub working: bool,
    /// Not under a closed cover.
    pub uncovered: bool,
    pub role: Role,
    /// The actor carries the key this control asks for.
    pub has_key: bool,
    /// A machine refuses the change (interlock): its reason, as an index into the owner's list.
    pub veto: Option<u16>,
}

impl Default for Gate {
    fn default() -> Self {
        Gate { supply: 1.0, working: true, uncovered: true, role: Role::Any, has_key: true, veto: None }
    }
}

/// Why an intent did nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    Covered,
    NoPower,
    Broken,
    NoKey,
    /// Pull-to-move: needs a hold first.
    Pull,
    /// Lever gate: lift it to pass.
    Gate,
    Veto(u16),
    /// At its stop.
    Stop,
    /// A tripped breaker still too hot to reset.
    Hot,
}

/// Something worth a sound or a line in the HUD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A detent or a position reached.
    Click,
    /// Into a hard stop.
    Stop,
    /// Thrown by its spring back to rest.
    Spring,
    /// A breaker tripped.
    Trip,
    /// A cover's seal broke.
    Seal,
    Blocked(Blocked),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outcome {
    /// The value it writes changed.
    pub changed: bool,
    pub event: Option<Event>,
}

impl Outcome {
    pub fn blocked(b: Blocked) -> Outcome {
        Outcome { changed: false, event: Some(Event::Blocked(b)) }
    }
    pub fn click(changed: bool) -> Outcome {
        Outcome { changed, event: if changed { Some(Event::Click) } else { None } }
    }
}

/// How a control's model is posed: up to four element parameters (angles in rad or travels in m,
/// per kind; see `render` of each kind) and how lit it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub e: [f32; 4],
    /// Light of its own (illuminated buttons), 0..1, and its colour.
    pub lit: f32,
    pub color: [u8; 3],
}

/// The state of one control: plain numbers, so it is saved, cloned and sent with its part.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ControlState {
    /// Position: value, index or travel, per kind.
    pub x: f64,
    /// Second axis (two-axis levers) or a buffer (keypad entry).
    pub y: f64,
    /// Speed of the moving part (wheels with inertia, needles).
    pub v: f64,
    /// A timer (hold, pulses, breaker heat).
    pub t: f64,
    /// Bits: see `F_*`.
    pub flags: u32,
}

/// Pressed now.
pub const F_PRESSED: u32 = 1;
/// A lever's axis (first, second) is held off its centre by a key (`Intent::Axis`): its spring
/// waits until the key lets go.
pub const F_AXIS_X: u32 = 128;
pub const F_AXIS_Y: u32 = 256;
/// A one-tick pulse is out.
pub const F_PULSE: u32 = 2;
/// Held long enough to pass a pull-to-move position or a gate.
pub const F_PULLED: u32 = 4;
/// Breaker tripped.
pub const F_TRIPPED: u32 = 8;
/// Cover seal broken.
pub const F_SEAL: u32 = 16;
/// Fine mode on (wheel pressed into fine).
pub const F_FINE: u32 = 32;
/// Coarse mode toggled by a press.
pub const F_COARSE: u32 = 64;

impl ControlState {
    pub fn has(&self, f: u32) -> bool {
        self.flags & f != 0
    }
    pub fn set(&mut self, f: u32, on: bool) {
        if on {
            self.flags |= f;
        } else {
            self.flags &= !f;
        }
    }
}
