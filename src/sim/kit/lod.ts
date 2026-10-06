// Levels of detail with conservation (docs/MUNDO.md §10; the report's rule 1): an aggregate is a
// number of units somewhere (1 200 t of iron in a store, 340 workers in a station) that becomes
// concrete things when someone comes close (promote) and is summed back when they go (demote).
//
//   - Conservation: nothing is made or lost by changing level. amount + members + out + lost is
//     the same forever (`ledger`): promoting moves units into members, demoting moves them back.
//   - What is touched persists (the report's rule 3): a member someone handled leaves its aggregate
//     for good (`touch`): its units go to `out`, it stays a thing of its own and is never summed back.
//   - Mixes (the report's cohorts): an aggregate may count its units by category (`Mix`, up to 8:
//     102 of 340 hate the governor). Members are drawn without replacement (multivariate
//     hypergeometric) and put back in their category, so the people you meet always add up to the
//     cohort: of 20, 6 ± 2 hate him. The draw of slot i is keyed by (world, aggregate, slot, version):
//     come back and the same people come out (the report's rule 2) — until the aggregate really
//     changes (someone left it for good: `version` moves on).
//   - Hysteresis: it promotes when an observer comes within `enter` and demotes only when every
//     observer is beyond `leave` (> enter): nothing flickers at the edge.
//
// Generic: what a member *is* (a crate, a person) is its module's business (`LodHooks.made`).

import { hash3, unit } from '../core/rng.js';
import { defineComponent } from '../core/store.js';
import type { World } from '../core/world.js';
import { Place, place, type V3 } from './place.js';

export const Aggregate = defineComponent('$aggregate', {
  /** What its units are (a symbol its module understands). */
  kind: 'sym',
  /** Units in the aggregate now (not in members). */
  amount: 'f64',
  /** Units per member. */
  unit: { type: 'f64', default: 1 },
  /** Most members at once. */
  slots: 'u32',
  /** Promotes within this distance of an observer (m); demotes beyond `leave`. */
  enter: 'f32',
  leave: 'f32',
  /** Its members are out now. */
  live: 'bool',
  /** Units in members that went their own way (touched), still in the world. */
  out: 'f64',
  /** Units gone from the world (members destroyed, consumed). */
  lost: 'f64',
  /** Changes for good (members touched or lost): what comes out next time is drawn anew. */
  version: 'u32',
});

export const Member = defineComponent('$member', {
  /** Its aggregate while it belongs to it (0: it went its own way). */
  of: 'ref',
  /** The aggregate it came from (always). */
  origin: 'ref',
  /** Its slot in the aggregate (where the engine lays it). */
  index: 'u32',
  amount: 'f64',
  touched: 'bool',
  /** Its category in the aggregate's mix (0xffff: none). */
  bucket: { type: 'u16', default: 0xffff },
});

export const MIX_KEYS = ['b0', 'b1', 'b2', 'b3', 'b4', 'b5', 'b6', 'b7'] as const;
export const Mix = defineComponent('$mix', { b0: 'u32', b1: 'u32', b2: 'u32', b3: 'u32', b4: 'u32', b5: 'u32', b6: 'u32', b7: 'u32' });

export interface AggregateInit {
  kind: string;
  amount: number;
  unit?: number;
  slots: number;
  enter: number;
  leave: number;
  /** Units by category (sums to amount / unit). */
  mix?: number[];
}

export interface LodHooks {
  /** A member was made (give it what its kind needs: a spec, a name…). */
  made?(w: World, agg: number, member: number, index: number): void;
}

/** Makes entity `id` (which has a Place) an aggregate. */
export function aggregate(w: World, id: number, o: AggregateInit): void {
  if (!(o.leave > o.enter)) throw new Error('agregado: leave debe ser mayor que enter');
  w.table(Aggregate).add(id, { kind: w.syms.id(o.kind), amount: o.amount, unit: o.unit ?? 1, slots: o.slots, enter: o.enter, leave: o.leave });
  if (o.mix) {
    if (o.mix.length > MIX_KEYS.length) throw new Error('agregado: como mucho 8 categorías');
    const init: Partial<Record<(typeof MIX_KEYS)[number], number>> = {};
    o.mix.forEach((n, i) => (init[MIX_KEYS[i]] = n));
    w.table(Mix).add(id, init);
  }
}

/** Where its units are: here, in its members, gone their own way, lost; `total` never changes. */
export function ledger(w: World, agg: number): { amount: number; members: number; out: number; lost: number; total: number } {
  const a = w.table(Aggregate);
  const m = w.table(Member);
  let members = 0;
  for (let r = 0; r < m.count; r++) if (m.c.of[r] === agg) members += m.c.amount[r];
  const amount = a.get(agg, 'amount');
  const out = a.get(agg, 'out');
  const lost = a.get(agg, 'lost');
  return { amount, members, out, lost, total: amount + members + out + lost };
}

/** Its members now (by id). */
export function membersOf(w: World, agg: number): number[] {
  const m = w.table(Member);
  const out: number[] = [];
  for (let r = 0; r < m.count; r++) if (m.c.of[r] === agg) out.push(m.ids[r]);
  return out.sort((a, b) => a - b);
}

/** Makes its members (as many as its slots and its units allow). Returns them. */
export function promote(w: World, agg: number, hooks?: LodHooks): number[] {
  const a = w.table(Aggregate);
  const r = a.row(agg);
  if (r < 0 || a.c.live[r]) return [];
  const unit = a.c.unit[r];
  const n = Math.min(a.c.slots[r], Math.floor(a.c.amount[r] / unit + 1e-9));
  const where = w.table(Place);
  const pr = where.row(agg);
  const host = pr >= 0 ? where.c.host[pr] : 0;
  const at: V3 = pr >= 0 ? [where.c.x[pr], where.c.y[pr], where.c.z[pr]] : [0, 0, 0];
  const made: number[] = [];
  for (let i = 0; i < n; i++) {
    const id = w.spawn();
    const bucket = draw(w, agg, i);
    w.table(Member).add(id, { of: agg, origin: agg, index: i, amount: unit, bucket });
    // the aggregate's row may have moved: re-read it
    a.c.amount[a.row(agg)] -= unit;
    place(w, id, host, at);
    hooks?.made?.(w, agg, id, i);
    made.push(id);
  }
  a.c.live[a.row(agg)] = 1;
  if (n > 0) w.record('lod.up', 0, agg, n, { keep: true });
  return made;
}

/** Sums its untouched members back and removes them. Returns the ones removed. */
export function demote(w: World, agg: number): number[] {
  const a = w.table(Aggregate);
  if (!a.has(agg) || !a.get(agg, 'live')) return [];
  const gone = membersOf(w, agg);
  const m = w.table(Member);
  for (const id of gone) {
    const amount = m.get(id, 'amount');
    const bucket = m.get(id, 'bucket');
    a.set(agg, 'amount', a.get(agg, 'amount') + amount);
    if (bucket !== 0xffff) putBack(w, agg, bucket);
    w.despawn(id);
  }
  a.set(agg, 'live', 0);
  if (gone.length) w.record('lod.down', 0, agg, gone.length, { keep: true });
  return gone;
}

/** Someone handled a member: it leaves its aggregate for good. False if it wasn't in one. */
export function touch(w: World, member: number): boolean {
  const m = w.table(Member);
  const r = m.row(member);
  if (r < 0) return false;
  m.c.touched[r] = 1;
  const agg = m.c.of[r];
  if (!agg) return false;
  m.c.of[r] = 0;
  const a = w.table(Aggregate);
  if (a.has(agg)) {
    a.set(agg, 'out', a.get(agg, 'out') + m.c.amount[r]);
    a.set(agg, 'version', a.get(agg, 'version') + 1);
  }
  return true;
}

/** A member is gone from the world (destroyed, eaten…): its units count as lost. */
export function lose(w: World, member: number): void {
  const m = w.table(Member);
  const r = m.row(member);
  if (r >= 0) {
    const agg = m.c.origin[r];
    const amount = m.c.amount[r];
    const attached = m.c.of[r] !== 0;
    const a = w.table(Aggregate);
    if (a.has(agg)) {
      if (!attached) a.set(agg, 'out', a.get(agg, 'out') - amount);
      a.set(agg, 'lost', a.get(agg, 'lost') + amount);
      a.set(agg, 'version', a.get(agg, 'version') + 1);
    }
  }
  w.despawn(member);
}

export interface Observer {
  /** The host it is in (0: none). */
  host: number;
  /** World position. */
  p: V3;
}

export interface ObserveInput {
  observers: Observer[];
  /** World position of each host with aggregates in it. */
  hosts?: Array<[id: number, x: number, y: number, z: number]>;
}

export interface LodChange {
  up: Array<{ agg: number; members: number[] }>;
  down: Array<{ agg: number; members: number[] }>;
}

/**
 * Where the observers are now: aggregates promote or demote (those `which` accepts, in table
 * order). A host's aggregates are measured from the host's position (plus their offset in it:
 * the host's turn is ignored, metres against LOD distances of hundreds).
 */
export function observe(w: World, input: ObserveInput, which: (agg: number) => boolean = () => true, hooks?: LodHooks): LodChange {
  const out: LodChange = { up: [], down: [] };
  const hosts = new Map((input.hosts ?? []).map(([id, x, y, z]) => [id, [x, y, z] as V3]));
  const a = w.table(Aggregate);
  const where = w.table(Place);
  // ids first: promoting adds rows elsewhere, but keep the walk independent of it
  const ids = Array.from(a.ids.subarray(0, a.count));
  for (const agg of ids) {
    if (!which(agg)) continue;
    const pr = where.row(agg);
    if (pr < 0) continue;
    const host = where.c.host[pr];
    const base = host ? hosts.get(host) : ([0, 0, 0] as V3);
    let d = Infinity;
    for (const o of input.observers) {
      if (host !== 0 && o.host === host) {
        d = 0;
        break;
      }
      if (!base) continue;
      const dx = o.p[0] - (base[0] + where.c.x[pr]);
      const dy = o.p[1] - (base[1] + where.c.y[pr]);
      const dz = o.p[2] - (base[2] + where.c.z[pr]);
      d = Math.min(d, Math.sqrt(dx * dx + dy * dy + dz * dz));
    }
    const live = a.get(agg, 'live') !== 0;
    if (!live && d < a.get(agg, 'enter')) {
      const members = promote(w, agg, hooks);
      out.up.push({ agg, members });
    } else if (live && d > a.get(agg, 'leave')) out.down.push({ agg, members: demote(w, agg) });
  }
  return out;
}

const DRAW_SALT = 0x4c4f44;

/** A category for slot `i`: drawn from what is left in the mix (without replacement), keyed (see top). */
function draw(w: World, agg: number, i: number): number {
  const mix = w.table(Mix);
  const r = mix.row(agg);
  if (r < 0) return 0xffff;
  let total = 0;
  for (const k of MIX_KEYS) total += mix.c[k][r];
  if (total <= 0) return 0xffff;
  const version = w.table(Aggregate).get(agg, 'version');
  let pick = Math.floor(unit(hash3(w.seed ^ DRAW_SALT, agg, version), i) * total);
  for (let i = 0; i < MIX_KEYS.length; i++) {
    const n = mix.c[MIX_KEYS[i]][r];
    if (pick < n) {
      mix.c[MIX_KEYS[i]][r] = n - 1;
      return i;
    }
    pick -= n;
  }
  return 0xffff;
}

function putBack(w: World, agg: number, bucket: number): void {
  const mix = w.table(Mix);
  const r = mix.row(agg);
  if (r >= 0 && bucket < MIX_KEYS.length) mix.c[MIX_KEYS[bucket]][r]++;
}
