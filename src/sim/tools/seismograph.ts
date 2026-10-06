// The seismograph (docs/MUNDO.md §13; the report's tool): every module's gauges sampled at a fixed
// game-time interval into bounded series, to see a system drift toward breaking before it breaks
// (prices, hunger, loyalty… today: crates in stores, people out, grudges). Derived data: not part of
// the world (never in its digest), kept beside its save.

import type { World } from '../core/world.js';

/** Every gauge of every module of a world, now. */
export function readGauges(w: World): Record<string, number> {
  const out: Record<string, number> = {};
  for (const m of w.modules) for (const [name, fn] of Object.entries(m.gauges ?? {})) out[name] = fn(w);
  return out;
}

export interface Series {
  /** Game times of the samples (s). */
  t: number[];
  /** Values by gauge, one per sample. */
  v: Record<string, number[]>;
}

export class Seismograph {
  private readonly t: number[] = [];
  private readonly v = new Map<string, number[]>();
  private next = -Infinity;

  constructor(
    /** Game seconds between samples. */
    readonly every: number,
    /** Most samples kept (the oldest go). */
    readonly keep = 2000,
  ) {}

  /** Samples if a sample is due (call it as often as you like). */
  tick(w: World): boolean {
    if (w.now < this.next) return false;
    this.next = Math.floor(w.now / this.every) * this.every + this.every;
    this.sample(w);
    return true;
  }

  sample(w: World): void {
    const g = readGauges(w);
    const n = this.t.length;
    this.t.push(w.now);
    for (const [name, value] of Object.entries(g)) {
      let s = this.v.get(name);
      // a gauge that appeared later: its past is unknown
      if (!s) this.v.set(name, (s = new Array<number>(n).fill(NaN)));
      s.push(value);
    }
    for (const [name, s] of this.v) if (!(name in g)) s.push(NaN);
    if (this.t.length > this.keep) {
      const drop = this.t.length - this.keep;
      this.t.splice(0, drop);
      for (const s of this.v.values()) s.splice(0, drop);
    }
  }

  series(): Series {
    return { t: [...this.t], v: Object.fromEntries([...this.v].map(([k, s]) => [k, [...s]])) };
  }

  load(s: Series): void {
    this.t.splice(0, this.t.length, ...s.t);
    this.v.clear();
    for (const [k, list] of Object.entries(s.v)) this.v.set(k, [...list]);
    this.next = this.t.length ? this.t[this.t.length - 1] + this.every : -Infinity;
  }
}
