// Closed forms (docs/MUNDO.md §2, "cálculo perezoso"): what isn't looked at isn't stepped. A
// quantity keeps its value, its rate and the time they were set; reading it at any time is one
// formula, and the moment it will cross a threshold is another — schedule that moment instead of
// checking every tick, and move it when the rate changes.
//
// Pure functions, plus `LinearField` over three columns of a table.

import type { FieldType, Table } from './store.js';

/** Value `dt` seconds after it was `v0`, changing at `rate` per second, kept within [lo, hi]. */
export function linearAt(v0: number, rate: number, dt: number, lo = -Infinity, hi = Infinity): number {
  const v = v0 + rate * dt;
  return v < lo ? lo : v > hi ? hi : v;
}

/** Seconds until a linear quantity reaches `target` (0 if it is there, Infinity if it never gets there). */
export function linearWhen(v0: number, rate: number, target: number): number {
  const d = target - v0;
  if (d === 0) return 0;
  if (rate === 0 || d > 0 !== rate > 0) return Infinity;
  return d / rate;
}

/** Something that relaxes towards `goal`, halving the gap every `halfLife` s (a mood, a grudge, a memory). */
export function approachAt(v0: number, goal: number, halfLife: number, dt: number): number {
  return goal + (v0 - goal) * Math.pow(2, -dt / halfLife);
}

/** Seconds until something relaxing towards `goal` reaches `target` (Infinity if it never does). */
export function approachWhen(v0: number, goal: number, halfLife: number, target: number): number {
  const gap0 = v0 - goal;
  const gap = target - goal;
  if (gap0 === gap) return 0;
  if (gap === 0 || gap0 === 0 || gap0 > 0 !== gap > 0 || Math.abs(gap) > Math.abs(gap0)) return Infinity;
  return halfLife * Math.log2(gap0 / gap);
}

/** Compound growth: `rate` per `period` seconds (a debt at 5 % a year: rate 0.05, period a year). */
export function compoundAt(v0: number, rate: number, period: number, dt: number): number {
  return v0 * Math.pow(1 + rate, dt / period);
}

/** The three columns of a linear quantity: its value, its rate (per second) and when they were set. */
export function linearFields<N extends string>(name: N): Record<N | `${N}Rate` | `${N}At`, FieldType> {
  return { [name]: 'f64', [`${name}Rate`]: 'f64', [`${name}At`]: 'time' } as Record<N | `${N}Rate` | `${N}At`, FieldType>;
}

/**
 * A linear quantity kept in a table (`linearFields`): read at any time without writing, written
 * only at event times (so the world is the same however its time is sliced).
 */
export class LinearField {
  private readonly rateKey: string;
  private readonly atKey: string;

  constructor(
    private readonly table: Table,
    readonly name: string,
    readonly lo = -Infinity,
    readonly hi = Infinity,
  ) {
    this.rateKey = `${name}Rate`;
    this.atKey = `${name}At`;
  }

  /** Value at time `t` (NaN if `id` isn't in the table). */
  at(id: number, t: number): number {
    const r = this.table.row(id);
    if (r < 0) return NaN;
    return linearAt(this.table.column(this.name)[r], this.table.column(this.rateKey)[r], t - this.table.column(this.atKey)[r], this.lo, this.hi);
  }

  rate(id: number): number {
    return this.table.get(id, this.rateKey);
  }

  /** Sets the value at time `t` (and the rate, if given). */
  set(id: number, t: number, value: number, rate?: number): void {
    const r = this.table.row(id);
    if (r < 0) throw new Error(`${this.table.name}: no está ${id}`);
    this.table.column(this.name)[r] = value < this.lo ? this.lo : value > this.hi ? this.hi : value;
    this.table.column(this.atKey)[r] = t;
    if (rate !== undefined) this.table.column(this.rateKey)[r] = rate;
  }

  /** Adds to the value at time `t`. */
  add(id: number, t: number, dv: number): void {
    this.set(id, t, this.at(id, t) + dv);
  }

  /** Changes the rate from time `t` on (what it was until then is kept). */
  setRate(id: number, t: number, rate: number): void {
    this.set(id, t, this.at(id, t), rate);
  }

  /** Time (absolute) when it reaches `target` from time `t`; Infinity if never. */
  when(id: number, t: number, target: number): number {
    return t + linearWhen(this.at(id, t), this.rate(id), target);
  }
}
