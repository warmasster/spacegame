// Loose objects in the world (docs/MUNDO.md §10): the world is where they are kept. Every crate,
// spare part… is an entity with what it is (`Thing`) and where it lies (`Place`, reported by the
// engine when it comes to rest), so what you move stays moved after a restart. Depots are
// aggregates of things (kit/lod.ts): 40 crates are one number until someone comes close, then as
// many crates as the depot has slots; taking one makes it a thing of its own for good.
//
// The engine asks (requests below); the world answers. The engine never keeps its own list.

import type { SimModule, World } from '../core/world.js';
import { defineComponent } from '../core/store.js';
import { Aggregate, aggregate, Member, Mix, observe, touch, type ObserveInput } from '../kit/lod.js';
import { Place, place, placeOf, type Q4, type V3 } from '../kit/place.js';

/** What a loose object is (the engine's object catalog: kind, size, mass, paint). */
export const Thing = defineComponent('thing', { kind: 'sym', hx: 'f64', hy: 'f64', hz: 'f64', mass: 'f64', paint: 'u8' });
/** An aggregate of things somewhere (its Thing: what each of its units is). */
export const Depot = defineComponent('depot', { key: 'sym' });
/** One row (id 0): the engine's starting objects were registered in this world. */
const Meta = defineComponent('objects.meta', { registered: 'bool' });

const PAINTS = ['orange', 'grey'] as const;

export interface ThingSpec {
  kind?: string;
  half: V3;
  mass: number;
  paint: (typeof PAINTS)[number];
}

/** A thing and where it is (host 0: world coordinates; else that host's space). */
export interface ThingAt {
  id: number;
  spec: ThingSpec;
  host: number;
  p: V3;
  q: Q4;
}

export interface DepotDef {
  /** Stable name (site + depot): the same depot every time the engine registers it. */
  key: string;
  spec: ThingSpec;
  host: number;
  p: V3;
  count: number;
  slots: number;
  enter: number;
  leave: number;
}

/** A thing a depot put out: where in the depot's layout (`index`) the engine lays it. */
export interface MadeThing extends ThingAt {
  depot: string;
  index: number;
}

function setThing(w: World, id: number, s: ThingSpec): void {
  w.table(Thing).add(id, { kind: w.syms.id(s.kind ?? ''), hx: s.half[0], hy: s.half[1], hz: s.half[2], mass: s.mass, paint: Math.max(0, PAINTS.indexOf(s.paint)) });
}

function specOf(w: World, id: number): ThingSpec {
  const t = w.table(Thing);
  const r = t.row(id);
  const kind = w.syms.name(t.c.kind[r]);
  return { ...(kind ? { kind } : {}), half: [t.c.hx[r], t.c.hy[r], t.c.hz[r]], mass: t.c.mass[r], paint: PAINTS[t.c.paint[r]] ?? 'orange' };
}

function thingAt(w: World, id: number): ThingAt {
  const at = placeOf(w, id)!;
  return { id, spec: specOf(w, id), host: at.host, p: at.p, q: at.q };
}

/** Does this thing exist physically now (not a unit summed in a depot, not the depot itself)? */
function physical(w: World, id: number): boolean {
  if (w.table(Depot).has(id)) return false;
  const of = w.table(Member).get(id, 'of');
  return !(of > 0) || w.table(Aggregate).get(of, 'live') !== 0;
}

const isDepot = (w: World) => (agg: number) => w.table(Depot).has(agg);

function sumOverDepots(w: World, field: 'amount' | 'out'): number {
  const t = w.table(Depot);
  let sum = 0;
  for (let r = 0; r < t.count; r++) sum += w.table(Aggregate).get(t.ids[r], field);
  return sum;
}

export const objects: SimModule = {
  name: 'objects',
  components: [Thing, Depot, Meta, Place, Aggregate, Member, Mix],
  describe: (w, id) => {
    if (w.table(Depot).has(id)) return { what: 'almacén', key: w.syms.name(w.table(Depot).get(id, 'key')), units: w.table(Aggregate).get(id, 'amount') };
    if (w.table(Thing).has(id) && physical(w, id)) return { what: 'objeto', ...specOf(w, id) };
    return null;
  },
  gauges: {
    /** Objects that exist as bodies now. */
    'objects.physical': (w) => {
      const t = w.table(Thing);
      let n = 0;
      for (let r = 0; r < t.count; r++) if (physical(w, t.ids[r])) n++;
      return n;
    },
    /** Units kept as numbers in the depots. */
    'objects.in_depots': (w) => sumOverDepots(w, 'amount'),
    /** Units taken out of the depots for good. */
    'objects.taken': (w) => sumOverDepots(w, 'out'),
  },
  requests: {
    /** { registered, count }: has this world got the engine's starting objects? */
    'objects.state': (w) => ({ registered: w.table(Meta).get(0, 'registered') === 1, count: w.table(Thing).count }),

    /** The engine's starting objects (a fresh world): their ids. */
    'objects.register': (w, list: Array<Omit<ThingAt, 'id'>>) => {
      const ids = list.map((o) => {
        const id = w.spawn();
        setThing(w, id, o.spec);
        place(w, id, o.host, o.p, o.q);
        return id;
      });
      w.table(Meta).add(0, { registered: 1 });
      return ids;
    },

    /** Every object that exists physically now, where it lies. */
    'objects.list': (w): ThingAt[] => {
      const t = w.table(Thing);
      const out: ThingAt[] = [];
      for (let r = 0; r < t.count; r++) if (physical(w, t.ids[r])) out.push(thingAt(w, t.ids[r]));
      return out.sort((a, b) => a.id - b.id);
    },

    /** Where things came to rest (or were laid). */
    'objects.rest': (w, list: Array<{ id: number; host: number; p: V3; q: Q4 }>) => {
      for (const o of list) if (w.table(Thing).has(o.id)) place(w, o.id, o.host, o.p, o.q);
    },

    /**
     * Someone took a thing (started handling it): if it was a depot's, it is its own now — the
     * depot's units are fewer for good. A fact of the world (who, what, from where).
     */
    'objects.taken': (w, o: { id: number; by: number; witnesses?: number[]; at?: V3 }) => {
      if (!w.table(Thing).has(o.id)) return { detached: false };
      const from = w.table(Member).get(o.id, 'of');
      const detached = touch(w, o.id);
      if (detached) w.fact('object.taken', o.by, o.id, 0, { depot: w.syms.name(w.table(Depot).get(from, 'key')), witnesses: o.witnesses ?? [], at: o.at });
      return { detached };
    },

    /** A new terrain (the room was reseeded): every object and depot goes; register again. */
    'objects.reset': (w) => {
      for (const t of [w.table(Thing), w.table(Depot)]) for (let r = t.count - 1; r >= 0; r--) w.despawn(t.ids[r]);
      w.table(Meta).remove(0);
    },

    /** The depots of the engine's sites (those it doesn't have yet are made, full). */
    'depots.register': (w, list: DepotDef[]) => {
      const known = new Set<string>();
      const t = w.table(Depot);
      for (let r = 0; r < t.count; r++) known.add(w.syms.name(t.c.key[r]));
      let made = 0;
      for (const d of list) {
        if (known.has(d.key)) continue;
        const id = w.spawn();
        w.table(Depot).add(id, { key: w.syms.id(d.key) });
        setThing(w, id, d.spec);
        place(w, id, d.host, d.p);
        aggregate(w, id, { kind: 'thing', amount: d.count, slots: d.slots, enter: d.enter, leave: d.leave });
        w.record('depot.founded', 0, id, d.count, { keep: true });
        made++;
      }
      return made;
    },

    /** Every depot: units in it, crates out now, units gone their own way. */
    'depots.state': (w) => {
      const t = w.table(Depot);
      const out: Array<{ key: string; amount: number; live: number; out: number }> = [];
      for (let r = 0; r < t.count; r++) {
        const key = w.syms.name(t.c.key[r]);
        out.push({ key, ...depotCount(w, key)! });
      }
      return out;
    },

    /** Where the players are: depots put their things out or take them back. */
    'objects.observe': (w, input: ObserveInput) => {
      const up: MadeThing[] = [];
      const down: number[] = [];
      const hooks = { made: (w: World, agg: number, member: number) => setThing(w, member, specOf(w, agg)) };
      const change = observe(w, input, isDepot(w), hooks);
      for (const u of change.up) {
        const key = w.syms.name(w.table(Depot).get(u.agg, 'key'));
        for (const id of u.members) up.push({ ...thingAt(w, id), depot: key, index: w.table(Member).get(id, 'index') });
      }
      for (const d of change.down) down.push(...d.members);
      return { up, down };
    },
  },
};

/** Units of a depot: in it, out as crates, gone their own way (tests, tools). */
export function depotCount(w: World, key: string): { amount: number; live: number; out: number } | null {
  const t = w.table(Depot);
  for (let r = 0; r < t.count; r++) {
    if (w.syms.name(t.c.key[r]) !== key) continue;
    const id = t.ids[r];
    const m = w.table(Member);
    let live = 0;
    for (let k = 0; k < m.count; k++) if (m.c.of[k] === id) live++;
    return { amount: w.table(Aggregate).get(id, 'amount'), live, out: w.table(Aggregate).get(id, 'out') };
  }
  return null;
}

export { Place };
