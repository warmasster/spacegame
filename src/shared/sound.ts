// What something sounds like, declared by whatever makes the sound — a ship's machine (its system
// module), and tomorrow a base's, a rover's, anything with state. The simulation never reads it:
// the client (client/audio/cues.ts) plays each cue from where it is in its host, through the air of
// its space, the host's structure and the ground. `sound` names an entry of the client's sound
// bank (client/audio/sounds/); a source can give a role its own voice (`sounds[role]`).
//
// Cues read the host's state (`st`: its number table, `sw`: its switches): only what reaches the
// clients (a ship's replicated variables).

import type { V3 } from './ship/geom.js';

/** The machine a cue comes from (a ship part fits: id, centre, room, size class, its own voices). */
export interface SoundSource {
  id: string;
  c: readonly number[];
  zone: string | null;
  /** Size class ('XS' … 'L'): the same machine bigger sounds deeper. */
  size?: string;
  /** Its own voice per role, replacing the cue's default. */
  sounds?: Record<string, string>;
}

export type SoundState = Float64Array;
export type SoundSwitches = Record<string, number>;

export interface SoundCue {
  /** Bank id (the default voice of this role). */
  sound: string;
  /** Role of the sound on its source ('run', 'start'…): `part.sounds[role]` replaces `sound`. */
  role?: string;
  /** The machine it comes from: its place and space, its size and its health (a wreck is silent). */
  part?: SoundSource;
  /** Point in the host's space (default: the part's centre, else its space's middle, else the host's). A function for one that moves: fill `out` and return it. */
  at?: V3 | ((st: SoundState, sw: SoundSwitches, out: V3) => V3);
  /** Space (a ship: compartment id) it sounds in (default: the part's; null = outside the host). */
  zone?: string | null;
  /** Continuous (a loop): how loud now, 0..1 (0 = silent). */
  level?: (st: SoundState, sw: SoundSwitches) => number;
  /** Continuous: playback rate (1 = as made): spool, rpm, flow. */
  pitch?: (st: SoundState, sw: SoundSwitches) => number;
  /** One-shot: plays each time this turns true (never on the first look). */
  on?: (st: SoundState, sw: SoundSwitches) => boolean;
  /**
   * Continuous, from motion: a value whose change makes the sound (a travel, a level). The loop
   * plays while it changes, as loud as its speed ÷ `rate` (change per second at full level).
   */
  motion?: { value: (st: SoundState, sw: SoundSwitches) => number; rate: number };
  /** Loudness scale (default 1). */
  gain?: number;
  /** Carried by the structure only (a machine bolted inside a wall): no air path. */
  structural?: boolean;
  /** A default any other cue of the same part and role replaces (switched loads' generic hum). */
  generic?: boolean;
}
