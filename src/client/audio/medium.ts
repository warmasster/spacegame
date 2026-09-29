// How a sound gets from where it is made to the ear, whatever made it. Sound needs something to
// travel through, so every sound has a place (`Place`) that says what carries it — the air of a
// ship's room, the ship's structure, the ground, the listener's own suit — and the listener
// (`Listener`) says what it is in and touching. `hear` walks the ways between the two:
//
//   air        both in air (a pressurised room, or a body with an atmosphere), through the open
//              doors, hatches and breaches between them; loud and full, thinner at low pressure
//   structure  a sound on a ship reaches whoever stands, sits or breathes in that ship through
//              its hull: muffled, low, from where the machine is
//   ground     a blast, a landing, a boot on the regolith reaches whoever stands on the ground
//              near it: a dull thud that dies out in tens of metres
//   suit       the listener's own sounds (boots, jetpack, tools, breathing): always heard, through
//              the suit — muffled in vacuum, open in air
//
// In vacuum and touching nothing, nothing is heard. The `muffled` mode (settings) lets any sound
// through as a low rumble instead, for players who want to hear the fight outside.
//
// Nothing here knows about ships or bodies: the game answers `Acoustics` (the air in a room, how
// open the way to it is), so any new place that holds air (a base, a rover) only answers that.

export type V3 = [number, number, number];

/** Where a sound is, and what carries it. Reuse them (fill, don't allocate, every frame). */
export interface Place {
  /** World point (m). */
  p: V3;
  /** Ship whose structure it is on (0: none). */
  ship: number;
  /** Compartment index of that ship whose air it is in (-1: outside the hull). */
  room: number;
  /** How well the ground carries it (0..1): on the ground, a landed ship, an exhaust hitting it. */
  ground: number;
  /** The listener's own: 0 no, 1 through the suit (boots, tools), 2 inside the helmet (breath, radio). */
  own: 0 | 1 | 2;
  /** Only through the structure (no air path). */
  structural: boolean;
  /** A point of a ship (ship space): the engine refreshes `p` from it while the sound plays. */
  local: V3 | null;
}

export const newPlace = (): Place => ({ p: [0, 0, 0], ship: 0, room: -1, ground: 0, own: 0, structural: false, local: null });

export function copyPlace(to: Place, from: Place) {
  to.p[0] = from.p[0];
  to.p[1] = from.p[1];
  to.p[2] = from.p[2];
  to.ship = from.ship;
  to.room = from.room;
  to.ground = from.ground;
  to.own = from.own;
  to.structural = from.structural;
  if (from.local) {
    to.local ??= [0, 0, 0];
    to.local[0] = from.local[0];
    to.local[1] = from.local[1];
    to.local[2] = from.local[2];
  } else to.local = null;
  return to;
}

/** The ears: the camera for direction, the astronaut's head for what it is in. */
export interface Listener {
  p: V3;
  fwd: V3;
  up: V3;
  ship: number;
  room: number;
  /** Air around the helmet (kPa). */
  air: number;
  /** How well the listener feels a ship's structure (0..1): seated 1, standing on it, breathing its air. */
  onShip: number;
  /** How well the listener feels the ground (0..1): standing on it, or aboard a landed ship. */
  onGround: number;
  /** Deaf (dead): only the helmet. */
  deaf: boolean;
}

export const newListener = (): Listener => ({ p: [0, 0, 0], fwd: [0, 0, -1], up: [0, 1, 0], ship: 0, room: -1, air: 0, onShip: 0, onGround: 0, deaf: false });

/** What the game answers about air (implemented for ships in shipSounds.ts). */
export interface Acoustics {
  /** Air pressure (kPa) of a room of a ship; room -1 / ship 0: the outside at world point `p`. */
  air(ship: number, room: number, p: V3): number;
  /** How open the air way is (0..1) from wherever the listener is to `room` of `ship` (-1: the outside round it). */
  way(ship: number, room: number): number;
  /** World point of a ship-space point, as drawn (into `out`); false if the ship is gone. */
  toWorld(ship: number, local: V3, out: V3): boolean;
}

/** How a sound is heard right now. */
export interface Heard {
  /** Loudness factor (0: inaudible). */
  gain: number;
  /** Low-pass cutoff (Hz) of what gets through. */
  cutoff: number;
  /** Reverb send (the room it is heard in). */
  wet: number;
  /** Seconds it takes to arrive (for one-shots). */
  delay: number;
}

export const newHeard = (): Heard => ({ gain: 0, cutoff: 20000, wet: 0, delay: 0 });

/** Numbers of the model. */
export const MEDIUM = {
  /** Pressure (kPa) below which air carries nothing worth hearing. */
  thin: 3,
  /** Structure path: loudness, cutoff (Hz), and the distance (m) it halves over. */
  hull: { gain: 0.5, cutoff: 520, half: 9 },
  /** Ground path: loudness, cutoff, and how fast it dies out (e-folding m). */
  ground: { gain: 0.65, cutoff: 240, fade: 32 },
  /** Own suit: cutoff in vacuum (conducted through the suit and the body). */
  suit: { cutoff: 1400 },
  /** Muffled mode: what any sound in vacuum is allowed through as. */
  muffled: { gain: 0.35, cutoff: 700 },
  /** Speed of sound (m/s) in air, through regolith. */
  speed: { air: 343, ground: 180 },
};

export type VacuumMode = 'physical' | 'muffled';

const dist = (a: V3, b: V3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

// the ways found so far for one `hear` (module scratch: no closure, no garbage per call)
let g2 = 0;
let lc = 0;
let loud = 0;
let lag = 0;

function way(g: number, cutoff: number, delay: number) {
  if (g <= 1e-5) return;
  g2 += g * g;
  lc += g * g * Math.log(cutoff);
  if (g > loud) {
    loud = g;
    lag = delay;
  }
}

/**
 * How `pl` is heard by `L`. `ref`: the source's size (m, full loudness within it). Every way
 * gives a loudness and a cutoff; they add as powers, the cutoff is their loudness-weighted mean.
 */
export function hear(pl: Place, L: Listener, ac: Acoustics, ref: number, mode: VacuumMode, out: Heard): Heard {
  out.gain = 0;
  out.cutoff = 20000;
  out.wet = 0;
  out.delay = 0;
  if (L.deaf && pl.own !== 2) return out;
  // inside the helmet: straight to the ear
  if (pl.own === 2) {
    out.gain = 1;
    return out;
  }
  const d = dist(pl.p, L.p);
  const spread = ref / Math.max(ref, d);
  g2 = lc = loud = lag = 0;
  let wet = 0;
  // the suit: your own boots and tools, conducted (and in air, heard too)
  if (pl.own === 1) way(1, L.air > MEDIUM.thin ? 16000 : MEDIUM.suit.cutoff, 0);
  // air: both in air, and a way open between them
  const earAir = L.air;
  if (!pl.structural && pl.own === 0 && earAir > MEDIUM.thin) {
    const srcAir = ac.air(pl.ship, pl.room, pl.p);
    if (srcAir > MEDIUM.thin) {
      // a ship knows the way from wherever the listener is to any of its rooms (or round it);
      // a sound out in the open reaches a listener inside only through the listener's ship
      const open = pl.ship !== 0 ? ac.way(pl.ship, pl.room) : L.room < 0 ? 1 : L.ship !== 0 ? ac.way(L.ship, -1) : 0;
      if (open > 0) {
        // a thin atmosphere carries less and loses the highs first
        const k = Math.sqrt(Math.min(srcAir, earAir) / 101);
        const absorb = 20000 * (120 / (120 + d));
        way(open * Math.min(1, k * 1.15) * spread, Math.max(400, absorb * Math.min(1, 0.35 + 0.65 * k) * (0.25 + 0.75 * open)), d / MEDIUM.speed.air);
        if (L.room >= 0) wet = 0.22 * open;
      }
    }
  }
  // the structure of the ship both are on
  if (pl.ship !== 0 && pl.ship === L.ship) {
    // breathing its air, the listener hears the walls ring even floating
    const contact = Math.max(L.onShip, L.room >= 0 && earAir > MEDIUM.thin ? 0.55 : 0);
    if (contact > 0) {
      const H = MEDIUM.hull;
      way((H.gain * contact * Math.min(1, ref / 2 + 0.5)) / (1 + d / H.half), H.cutoff * (0.8 + 0.4 * contact), 0);
    }
  }
  // the ground both touch
  if (pl.ground > 0 && L.onGround > 0) {
    const G = MEDIUM.ground;
    way((G.gain * pl.ground * L.onGround * Math.exp(-d / (G.fade * Math.max(1, Math.sqrt(ref))))) / (1 + d / 12), G.cutoff, d / MEDIUM.speed.ground);
  }
  // muffled mode: nothing quite silent in vacuum
  if (mode === 'muffled' && g2 < 0.02 && pl.own === 0) way(MEDIUM.muffled.gain * spread, MEDIUM.muffled.cutoff, 0);
  if (g2 <= 0) return out;
  out.gain = Math.sqrt(g2);
  out.cutoff = Math.exp(lc / g2);
  out.wet = wet;
  out.delay = lag;
  return out;
}
