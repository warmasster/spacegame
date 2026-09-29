// An astronaut's sounds — the same for the local one and every remote one; only the place
// differs (your own boots come through your suit; someone else's reach you through the deck, the
// ground or the air like anything else). And the helmet: what only the wearer hears.

import { sfx, type Loop } from './engine';
import { copyPlace, newPlace, type Place, type V3 } from './medium';
import { surface } from './surfaces';

export class CrewSounds {
  /** Where its feet are and what carries them (filled every frame by the game). */
  readonly feet: Place = newPlace();
  private jet: Loop;
  private weld: Loop;
  private shot: Place = newPlace();

  constructor(readonly own: boolean) {
    this.jet = sfx.loop('jet.hiss');
    this.weld = sfx.loop('weld.arc');
    if (own) this.feet.own = 1;
  }

  /** A foot comes down on `material` (strength 0..1: a stroll … a lope). */
  step(material: string, k: number) {
    const s = surface(material);
    const pl = this.onGround(s.ground);
    sfx.play(s.step, pl, s.gain * (0.45 + 0.55 * Math.min(1, k)));
  }

  /** Back on the ground after a jump or a fall (`speed`: m/s coming down). */
  land(material: string, speed: number) {
    if (speed < 0.6) return;
    const s = surface(material);
    sfx.play(s.land, this.onGround(s.ground), s.gain * Math.min(1.2, 0.35 + speed / 3.5));
  }

  /** A one-shot where the astronaut is (its hands: tools, crates, the launcher). */
  play(id: string, at: V3 | null = null, gain = 1) {
    const pl = copyPlace(this.shot, this.feet);
    if (at) {
      pl.p[0] = at[0];
      pl.p[1] = at[1];
      pl.p[2] = at[2];
      pl.local = null;
    }
    pl.ground = 0;
    sfx.play(id, pl, gain);
  }

  /** Every frame: the jetpack at its back, the welding arc at the tool's tip (null: not welding). */
  update(jetting: boolean, back: V3, weldAt: V3 | null) {
    this.jet.level = jetting ? 1 : 0;
    if (jetting) this.follow(this.jet.place, back);
    this.weld.level = weldAt ? 1 : 0;
    if (weldAt) this.follow(this.weld.place, weldAt);
  }

  dispose() {
    this.jet.release();
    this.weld.release();
  }

  private follow(pl: Place, p: V3) {
    copyPlace(pl, this.feet);
    pl.p[0] = p[0];
    pl.p[1] = p[1];
    pl.p[2] = p[2];
    pl.local = null;
    pl.ground = 0;
  }

  private onGround(carries: number) {
    const pl = copyPlace(this.shot, this.feet);
    pl.ground = this.feet.ground * carries;
    return pl;
  }
}

/** What only the wearer hears: breathing, the suit's fan, its warnings. */
export class Helmet {
  private breath = sfx.loop('suit.breath');
  private fan = sfx.loop('suit.fan');
  private effort = 0;
  private o2T = 0;
  private fuelLow = false;

  constructor() {
    this.breath.place.own = 2;
    this.fan.place.own = 2;
  }

  /** `work`: how hard the astronaut works now (0 idle … 1 running, jetting). */
  update(dt: number, s: { alive: boolean; work: number; o2: number; fuel: number; jetting: boolean }) {
    // breathing follows the effort (slowly), and gasps when the oxygen runs out
    const want = Math.max(s.work, s.o2 < 0.15 ? 1 - s.o2 / 0.15 : 0);
    this.effort += (want - this.effort) * Math.min(1, dt / (want > this.effort ? 2.5 : 6));
    this.breath.level = s.alive ? 0.55 + 0.45 * this.effort : 0;
    this.breath.pitch = 1 + 0.55 * this.effort;
    this.fan.level = s.alive ? 1 : 0;
    // low oxygen: beeps, faster as it drains
    this.o2T -= dt;
    if (s.alive && s.o2 < 0.25 && this.o2T <= 0) {
      sfx.ui('suit.warn');
      this.o2T = s.o2 < 0.1 ? 1.5 : 4;
    }
    // jetpack nearly dry: once, until it refills
    if (s.alive && s.jetting && s.fuel < 0.2 && !this.fuelLow) {
      this.fuelLow = true;
      sfx.ui('suit.fuel');
    }
    if (s.fuel > 0.5) this.fuelLow = false;
  }
}
