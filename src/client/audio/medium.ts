// How a sound gets from where it is made to the ear, whatever made it and wherever it is. Sound
// needs something to travel through, so every sound has a place (`Place`) that says what carries
// it — the air of a space, the structure of a host, the ground, the listener's own suit — and the
// listener (`Listener`) says what it is in and touching. `hear` walks the ways between the two:
//
//   air        both in air (a pressurised space, or a body with an atmosphere), through the open
//              ways between them; loud and full, thinner at low pressure
//   structure  a sound on a host reaches whoever stands, sits or breathes in that host through its
//              structure, as much as the sound shakes it (`body`): muffled, low
//   ground     a blast, a landing, a boot on the ground reaches whoever stands on the ground near
//              it: a dull thud that dies out in tens of metres
//   suit       the listener's own sounds (boots, jetpack, tools): always heard, through the suit —
//              muffled in vacuum, open in air
//
// In vacuum and touching nothing, nothing is heard. The `muffled` mode (settings) lets any sound
// through as a low rumble instead.
//
// Nothing here knows ships, bases or bodies: a host is an id and its spaces are numbers; the game
// answers `Acoustics` (acoustics.ts: the hosts' air and ways, the body's atmosphere outside).

export type V3 = [number, number, number];

/** Where a sound is, and what carries it. Reuse them (fill, don't allocate, every frame). */
export interface Place {
  /** World point (m). */
  p: V3;
  /** Host whose structure it is on (a frame id: a ship…; 0: none, out in the world). */
  host: number;
  /** Space of that host whose air it is in (-1: outside it). */
  space: number;
  /** How well the ground carries it (0..1): on the ground, a host standing on it, an exhaust hitting it. */
  ground: number;
  /** The listener's own: 0 no, 1 through the suit (boots, tools), 2 inside the helmet (radio, warnings). */
  own: 0 | 1 | 2;
  /** Only through the structure (no air path). */
  structural: boolean;
  /** A point of the host's space: the engine refreshes `p` from it while the sound plays (it follows the host). */
  local: V3 | null;
}

export const newPlace = (): Place => ({ p: [0, 0, 0], host: 0, space: -1, ground: 0, own: 0, structural: false, local: null });

export function copyPlace(to: Place, from: Place) {
  to.p[0] = from.p[0];
  to.p[1] = from.p[1];
  to.p[2] = from.p[2];
  to.host = from.host;
  to.space = from.space;
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
  /** Host whose space holds the head (0: out in the world). */
  host: number;
  space: number;
  /** Air around the helmet (kPa). */
  air: number;
  /** How well the listener feels a host's structure (0..1): seated 1, standing on it, floating in it. */
  onHost: number;
  /** Host it touches (its frame): the one whose structure it feels. */
  touching: number;
  /** How well the listener feels the ground (0..1): standing on it, or aboard a host standing on it. */
  onGround: number;
  /** Deaf (dead): only the helmet. */
  deaf: boolean;
}

export const newListener = (): Listener => ({ p: [0, 0, 0], fwd: [0, 0, -1], up: [0, 1, 0], host: 0, space: -1, air: 0, onHost: 0, touching: 0, onGround: 0, deaf: false });

/** What the game answers about air (acoustics.ts). */
export interface Acoustics {
  /** Air pressure (kPa) in a space of a host; host 0 or space -1: the outside at world point `p`. */
  air(host: number, space: number, p: V3): number;
  /** How open the air way is (0..1) from wherever the listener is to `space` of `host` (-1: the outside round it). */
  way(host: number, space: number): number;
  /** World point of a point of a host's space (into `out`); false if the host is gone. */
  toWorld(host: number, local: V3, out: V3): boolean;
}

/** How a sound is heard right now. */
export interface Heard {
  /** Loudness factor (0: inaudible). */
  gain: number;
  /** Low-pass cutoff (Hz) of what gets through. */
  cutoff: number;
  /** Reverb send (the space it is heard in). */
  wet: number;
  /** Seconds it takes to arrive (for one-shots). */
  delay: number;
}

export const newHeard = (): Heard => ({ gain: 0, cutoff: 20000, wet: 0, delay: 0 });

/** Numbers of the model. */
export const MEDIUM = {
  /** Pressure (kPa) below which air carries nothing worth hearing. */
  thin: 3,
  /** Structure path: loudness of a sound that shakes it fully, cutoff (Hz), distance (m) it halves over. */
  hull: { gain: 0.55, cutoff: 480, half: 9 },
  /** Ground path: loudness, cutoff, and how fast it dies out (e-folding m). */
  ground: { gain: 0.65, cutoff: 240, fade: 32 },
  /** Own suit: cutoff in vacuum (conducted through the suit and the body). */
  suit: { cutoff: 1400 },
  /** Muffled mode: what any sound in vacuum is allowed through as. */
  muffled: { gain: 0.35, cutoff: 700 },
  /** Speed of sound (m/s) in air, through the ground. */
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
 * How `pl` is heard by `L`. `ref`: the source's size (m, full loudness within it); `body`: how much
 * it shakes the structure it is on (0..1). Every way gives a loudness and a cutoff; they add as
 * powers, the cutoff is their loudness-weighted mean.
 */
export function hear(pl: Place, L: Listener, ac: Acoustics, ref: number, body: number, mode: VacuumMode, out: Heard): Heard {
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
    const srcAir = ac.air(pl.host, pl.space, pl.p);
    if (srcAir > MEDIUM.thin) {
      // a host knows the way from wherever the listener is to any of its spaces (or round it);
      // a sound out in the world reaches a listener inside only through the listener's host
      const open = pl.host !== 0 ? ac.way(pl.host, pl.space) : L.space < 0 ? 1 : L.host !== 0 ? ac.way(L.host, -1) : 0;
      if (open > 0) {
        // a thin atmosphere carries less and loses the highs first
        const k = Math.sqrt(Math.min(srcAir, earAir) / 101);
        const absorb = 20000 * (120 / (120 + d));
        way(open * Math.min(1, k * 1.15) * spread, Math.max(400, absorb * Math.min(1, 0.35 + 0.65 * k) * (0.25 + 0.75 * open)), d / MEDIUM.speed.air);
        if (L.space >= 0) wet = 0.22 * open;
      }
    }
  }
  // the structure of the host both are on (as much as the sound shakes it)
  if (pl.host !== 0 && body > 0) {
    // breathing its air, the listener hears its walls ring even floating
    const contact = Math.max(pl.host === L.touching ? L.onHost : 0, pl.host === L.host && L.space >= 0 && earAir > MEDIUM.thin ? 0.4 : 0);
    if (contact > 0) {
      const H = MEDIUM.hull;
      way((H.gain * body * contact) / (1 + d / H.half), H.cutoff * (0.8 + 0.4 * contact), 0);
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
