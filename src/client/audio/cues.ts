// Plays declared sounds (shared/sound.ts SoundCue) for any host with state: loops as loud as their
// `level`, travel noise from `motion`, one-shots on the edge of `on`. Where each cue sounds (its
// point, its space, its wreck test) is the host's to say (`spot`); this knows nothing else.

import type { SoundCue, SoundState, SoundSwitches } from '../../shared/sound';
import { sfx, type Loop } from './engine';
import { newPlace, type Place, type V3 } from './medium';

/** Where a cue sounds in its host, resolved once by the host. */
export interface CueSpot {
  /** Bank id (after the source's own voice for the role). */
  sound: string;
  /** Point of the host's space (refreshed by the cue's `at` when it is a function). */
  local: V3;
  /** Air space of the host (-1: outside it). */
  space: number;
  /** Outside the host's body (an exhaust, a pad): the ground under it carries it more. */
  exterior: boolean;
  /** Pitch of the source's size (bigger deeper). */
  pitch: number;
  /** State index whose value ≤ 0 means its machine is wrecked and silent (-1: none). */
  hp: number;
}

interface Live extends CueSpot {
  cue: SoundCue;
  atFn: ((st: SoundState, sw: SoundSwitches, out: V3) => V3) | null;
  gain: number;
  loop: Loop | null;
  /** Last value of `on` (-1: not read yet). */
  was: number;
  /** `motion`: last value and smoothed speed. */
  mv: number;
  speed: number;
}

/** How well the ground carries the host's sounds now: from outside it, from inside it. */
export interface Footing {
  exterior: number;
  interior: number;
}

export class CuePlayer {
  private live: Live[] = [];
  private shot: Place = newPlace();

  constructor(
    private host: number,
    cues: readonly SoundCue[],
    spot: (cue: SoundCue) => CueSpot,
  ) {
    for (const cue of cues) {
      const s = spot(cue);
      this.live.push({ ...s, cue, atFn: typeof cue.at === 'function' ? cue.at : null, gain: cue.gain ?? 1, loop: null, was: -1, mv: NaN, speed: 0 });
    }
  }

  /** Every frame the host is awake. `waking`: edges are read, not played (what changed meanwhile doesn't sound late). */
  update(dt: number, st: SoundState, sw: SoundSwitches, foot: Footing, waking: boolean) {
    for (let k = 0; k < this.live.length; k++) {
      const lc = this.live[k];
      const cue = lc.cue;
      if (lc.atFn) lc.atFn(st, sw, lc.local);
      const alive = lc.hp < 0 || st[lc.hp] > 0;
      if (cue.on) {
        const now = cue.on(st, sw) ? 1 : 0;
        if (lc.was === 0 && now === 1 && alive && !waking) sfx.play(lc.sound, this.fill(this.shot, lc, foot), lc.gain, lc.pitch);
        lc.was = now;
      }
      if (!cue.level && !cue.motion) continue;
      let level = 0;
      if (cue.motion) {
        const v = cue.motion.value(st, sw);
        if (lc.mv === lc.mv) lc.speed = Math.max(Math.abs(v - lc.mv) / Math.max(dt, 1e-3), lc.speed * Math.exp(-dt / 0.3));
        lc.mv = v;
        level = Math.min(1, lc.speed / cue.motion.rate);
        if (level < 0.05) level = 0;
      }
      if (cue.level) level = cue.motion ? level * cue.level(st, sw) : cue.level(st, sw);
      if (!alive) level = 0;
      if (level <= 0 && !lc.loop) continue;
      lc.loop ??= sfx.loop(lc.sound, lc.gain);
      lc.loop.level = level;
      lc.loop.pitch = lc.pitch * (cue.pitch ? cue.pitch(st, sw) : 1);
      this.fill(lc.loop.place, lc, foot);
    }
  }

  /** Far from the listener: everything quiet, edges forgotten. */
  sleep() {
    for (const lc of this.live) {
      lc.was = -1;
      lc.mv = NaN;
      lc.speed = 0;
      if (lc.loop) lc.loop.level = 0;
    }
  }

  private fill(pl: Place, lc: Live, foot: Footing) {
    pl.host = this.host;
    pl.space = lc.space;
    pl.ground = lc.exterior ? foot.exterior : foot.interior;
    pl.own = 0;
    pl.structural = !!lc.cue.structural;
    pl.local ??= [0, 0, 0];
    pl.local[0] = lc.local[0];
    pl.local[1] = lc.local[1];
    pl.local[2] = lc.local[2];
    return pl;
  }
}
