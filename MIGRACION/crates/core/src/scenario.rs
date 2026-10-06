//! A playable scenario as data (`scenario.jsonc`): where the player starts, how they move, and the
//! test crowd of ships and NPCs laid out round the site. The command line may override counts.
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteDef {
    /// Body id.
    pub body: String,
    /// Direction from the body's centre, then metres east and north of it.
    pub dir: [f64; 3],
    pub east: f64,
    pub north: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightDef {
    /// Base speed (m/s), changed with the wheel between `min_speed` and `max_speed`.
    pub speed: f64,
    pub min_speed: f64,
    pub max_speed: f64,
    /// Speed factor per wheel notch.
    pub wheel_step: f64,
    /// Shift multiplies the speed by this.
    pub run_factor: f64,
    /// Alt multiplies it again (default of the menu setting).
    pub boost_factor: f64,
    /// How fast (1/s) "up" follows the body under you in flight.
    pub up_follow: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerDef {
    /// The look at the start: radians from the north of the ground stood on toward east, and
    /// up from the horizon.
    pub yaw: f64,
    pub pitch: f64,
    pub eye_height: f64,
    pub walk_speed: f64,
    pub run_speed: f64,
    /// Jump apex (m) under the body's surface gravity.
    pub jump_height: f64,
    /// The highest step taken without jumping (m): a person steps up a kerb, a sill, a crate
    /// lid, and down one, without leaving the ground.
    pub escalon: f64,
    /// How the boots hold the ground: their friction on it (what of the weight on them they
    /// hold sideways) and what the legs brake with besides (m/s², stepping against the way
    /// one is going): what stops you when you come down with speed.
    pub agarre: f64,
    pub frenada: f64,
    /// The body as what walks among things and bumps into them.
    pub cuerpo: PersonDef,
    /// The helmet's lamps.
    pub linterna: HelmetLampDef,
    /// Vertical field of view (degrees) and near plane (m).
    pub fov: f32,
    pub near: f32,
    /// Radians per mouse count.
    pub mouse: f64,
    pub flight: FlightDef,
    /// The suit's jet pack, if it has one.
    #[serde(default)]
    pub mochila: Option<JetpackDef>,
    /// What bare hands can do with what is loose, if anything.
    #[serde(default)]
    pub manos: Option<HandsDef>,
}

/// A person on foot as what bumps into things: three spheres one over another (feet, hips, head)
/// that the parts of any structure push out, and how the legs and the head take what they meet.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonDef {
    /// A person in a suit with its pack (kg).
    pub masa: f64,
    /// Radius of the spheres (m) and their heights over the feet (their centres), standing.
    pub radio: f32,
    pub esferas: [f64; 3],
    pub agachado: CrouchDef,
    /// The steepest ground one stands on: the cosine of its slope (steeper is a wall).
    pub apoyo: f64,
    /// Parts too small to bump into (bounding radius, m).
    pub menudo: f32,
    /// Share of g the boots hold against the air sliding you over a deck (a gale: no bracing it).
    pub vendaval: f64,
    /// The fastest the air or the pack can take you from what you stand on or keep to (m/s).
    pub deriva: f64,
    /// A step taken (`escalon`): how far past the face of what is in the way the foot is set
    /// down (m, the nearest and the farthest tried), how fast the legs reach down to ground that
    /// drops away (m/s), and how fast the eyes catch up with a step taken at once (1/s).
    pub pisada: [f64; 2],
    pub piernas: f64,
    pub tras_escalon: f64,
    pub rodillas: KneesDef,
    pub cabeza: HeadDef,
    pub enderezar: UprightDef,
    /// Weighing less than this on what is underfoot (m/s²) the boots hold nothing: one does
    /// not walk, one floats. 0: the boots hold whatever deck they touch, weight or none.
    pub sin_peso: f64,
}

/// How the body rights itself to the way up its weight gives: toward it at `ritmo` 1/s of the
/// angle still to go, `giro` rad/s at most, and that only from a weight of `peso` m/s² on (with
/// less, as much slower): where hardly anything weighs hardly anything turns one, and where
/// nothing does, nothing.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UprightDef {
    pub ritmo: f64,
    pub giro: f64,
    pub peso: f64,
}

/// Crouched: the eye this high (m), the spheres there, the pace (m/s), and how fast one goes down
/// or up (m/s of eye height).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrouchDef {
    pub ojos: f64,
    pub esferas: [f64; 3],
    pub paso: f64,
    pub ritmo: f64,
}

/// A landing taken on the knees: how fast the eyes go down per m/s one comes down with
/// (`hundimiento`), the half-life they come up again with (s), and the most of each: the hardest
/// blow they take (m/s of the eyes) and the farthest they go down (m).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KneesDef {
    pub hundimiento: f64,
    pub vida: f64,
    pub golpe: f64,
    pub tope: f64,
}

/// How far the look goes up and down (rad), and seated how far the head turns to each side.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadDef {
    pub arriba: f64,
    pub sentado: f64,
}

/// The helmet's lamps: where from the eyes (m up, m ahead), their light, how far it reaches (m)
/// and how wide (cosine of the half angle of its cone).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelmetLampDef {
    pub en: [f64; 2],
    pub color: [f32; 3],
    pub alcance: f32,
    pub cono: f32,
}

/// Bare hands on a loose thing: a click held takes hold of it and it follows the look, pulled
/// with at most `fuerza` N (what that cannot lift it drags; what weighs over `masa` kg it does
/// not move at all), from at most `alcance` m, as stiff as a spring of `frecuencia` Hz.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandsDef {
    pub fuerza: f32,
    pub masa: f32,
    pub alcance: f64,
    pub frecuencia: f32,
}

/// A jet pack: Space pushes up, the keys you walk with push sideways once off the ground (let go
/// and it steadies you), Ctrl pushes down. It burns its gas doing so and fills up again aboard a
/// ship.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JetpackDef {
    /// Push up and sideways (m/s²).
    pub empuje: f64,
    pub lateral: f64,
    /// Hands off with its steadying on: how hard it brakes what you still carry sideways
    /// (m/s²; all its jets against the way you go: more than they push you with one way).
    pub frenada: f64,
    /// Seconds of full push up a full pack gives.
    pub autonomia: f64,
    /// Seconds to fill it aboard a ship.
    pub recarga: f64,
    /// It pushes down this much of what it pushes up.
    pub abajo: f64,
    /// What of a full push up its side jets and its steadying burn (share).
    pub gasto_lado: f64,
    pub gasto_freno: f64,
    /// Seconds off the ground before it takes that for flying (a ramp's sill, a step).
    pub despegue: f64,
    /// Holding your height: the time it takes it to null the speed up or down (s).
    pub sosten: f64,
    /// Steadying: under this speed sideways it leaves you be (m/s).
    pub quieto: f64,
    pub junto: BesideDef,
    /// Floating where nothing weighs, how fast its jets roll you (rad/s, Q and E).
    #[serde(default = "roll_rate")]
    pub alabeo: f64,
}

fn roll_rate() -> f64 {
    1.2
}

/// What the pack steadies you to in the air: whatever near you goes most as you do — a ship you
/// left or came up to, as it goes now; the ground you hover over. Something counts as near within
/// `alcance` m of its bounding sphere. Another is taken only if it goes clearly more as you do:
/// if you go less than `mejora` of what you go from the one you keep to, less `margen` m/s. How a
/// ship beside you speeds up is smoothed over `suavizado` s and taken to `aceleracion` m/s² at
/// most (it falls: you fall with it).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BesideDef {
    pub alcance: f64,
    pub mejora: f64,
    pub margen: f64,
    pub suavizado: f64,
    pub aceleracion: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Things are spread on a sunflower spiral from `inner` to `radius` m round the site.
    pub radius: f64,
    pub inner: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CircuitDef {
    /// [min, max] of each random pick.
    pub radius: [f64; 2],
    pub altitude: [f64; 2],
    /// rad/s
    pub omega: [f64; 2],
    pub figure_eight: f64,
    /// Bank into turns (rad per unit of turn) and its limit.
    pub bank: f64,
    pub bank_max: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetDef {
    /// Model ids, used in turn.
    pub models: Vec<String>,
    pub count: usize,
    /// Share of the ships flying circuits.
    pub flying_share: f64,
    pub circuit: CircuitDef,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrowdDef {
    pub model: String,
    pub count: usize,
    /// Offset (east, north m) of the crowd's spiral from the fleet's.
    pub offset: [f64; 2],
    pub walk_speed: f64,
    /// Metres per walk cycle; idle cycles per second.
    pub stride: f64,
    pub idle_rate: f64,
    /// Ground resampling step (m) and how far from home they may wander (m).
    pub resample: f64,
    pub leash: f64,
    /// Seconds of each walk and each rest: [min, max].
    pub walk_time: [f64; 2],
    pub idle_time: [f64; 2],
    /// Wander turn rate (rad/s at most) and the rate turning back home.
    pub wander: f64,
    pub home_turn: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDef {
    pub site: SiteDef,
    pub player: PlayerDef,
    pub layout: Layout,
    pub fleet: FleetDef,
    pub crowd: CrowdDef,
    /// Structures (`structures/builds`) set round the site.
    #[serde(default)]
    pub structures: Vec<StructureAtDef>,
    /// Ships (`ships/<id>`) standing round the site, systems and all.
    #[serde(default)]
    pub ships: Vec<ShipAtDef>,
    /// Traffic: ships going from field to field round the site (`traffic`), drawn as the
    /// fleet's models.
    #[serde(default)]
    pub trafico: Option<crate::traffic::TrafficDef>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShipAtDef {
    pub ship: String,
    /// Metres east and north of the site.
    pub east: f64,
    pub north: f64,
    /// Nose from north toward east (deg).
    #[serde(default)]
    pub yaw: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureAtDef {
    pub build: String,
    /// Metres east and north of the site.
    pub east: f64,
    pub north: f64,
    /// Turn from north toward east (deg).
    #[serde(default)]
    pub yaw: f64,
    /// Over the lowest ground under it (m).
    #[serde(default)]
    pub lift: f64,
}

/// Uniform pick in [min, max] from a unit random number.
pub fn lerp_range(r: [f64; 2], u: f64) -> f64 {
    r[0] + (r[1] - r[0]) * u
}
