// One seed of a sweep (tools/sim/seeds.ts): a world run headless for some years, measured.
// Runs in a worker thread (or in-process): no engine, nobody observes — what the world does alone.

import { World, type SimModule } from '../../src/sim/index.js';
import { digest } from '../../src/sim/persist/snapshot.js';
import { readGauges } from '../../src/sim/tools/seismograph.js';

export interface SeedResult {
  seed: number;
  /** Every gauge at the end. */
  gauges: Record<string, number>;
  /** Each gauge at the end of each year. */
  yearly: Record<string, number[]>;
  /** How many records of each kind the log has (what happened, by kind). */
  kinds: Record<string, number>;
  events: number;
  errors: number;
  digest: string;
  ms: number;
}

export function runSeed(seed: number, years: number, modules: readonly SimModule[]): SeedResult {
  const t0 = performance.now();
  const w = World.create({ seed, modules });
  w.onError = () => undefined;
  const year = w.calendar.year;
  const yearly: Record<string, number[]> = {};
  for (let y = 1; y <= years; y++) {
    w.advance(y * year);
    for (const [k, v] of Object.entries(readGauges(w))) (yearly[k] ??= []).push(v);
  }
  const kinds: Record<string, number> = {};
  for (const c of w.log.chunks) for (let i = 0; i < c.n; i++) {
    const k = w.syms.name(c.kind[i]);
    kinds[k] = (kinds[k] ?? 0) + 1;
  }
  return { seed, gauges: readGauges(w), yearly, kinds, events: w.stats.events, errors: w.stats.errors, digest: digest(w), ms: performance.now() - t0 };
}
