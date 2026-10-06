// What the people of the world know and feel (docs/MUNDO.md §12). The engine tells the world what
// a player did and who noticed it (a fact with its witnesses: shared/actors/perception.ts decides
// who saw or heard). Each witness remembers it (a memory that points at the fact: the chain of
// causes goes on through them), and holds it against whoever did it (a grudge, a closed form that
// fades by itself: half of it gone in two game days). Seeing it makes a witness someone of its own
// (it leaves its crew's number for good: kit/lod.ts `touch`). A witness with a body whose grudge
// crosses a line reacts: says something and goes over there.
//
// Which facts matter and how much is data (FACTS): a new kind of fact is a line.

import { HOUR } from '../core/calendar.js';
import { approachAt } from '../core/lazy.js';
import { defineComponent } from '../core/store.js';
import type { EventDef, SimModule, World } from '../core/world.js';
import { Member, touch } from '../kit/lod.js';
import { Place } from '../kit/place.js';
import { order, Person, say } from './people.js';
import type { V3 } from '../kit/place.js';

/** How much each kind of fact weighs on a witness, and what it says if it reacts. */
export const FACTS: Record<string, { weight: number; say?: string }> = {
  'object.taken': { weight: 0.5, say: '¡Eh! ¡Eso es del almacén!' },
  hurt: { weight: 0.9, say: '¡Alto! ¡Deja de disparar a la gente!' },
  shot: { weight: 0.2, say: '¿Quién está disparando?' },
  explosion: { weight: 0.15, say: '¿Qué ha sido eso?' },
};

/** A grudge above this makes a witness with a body react. */
const REACT_AT = 0.35;
/** Half of a grudge fades in this long (s of game time). */
const GRUDGE_HALF = 48 * HOUR;

/** Something someone remembers: a fact (its record) about someone. */
export const Memory = defineComponent('memory', { holder: 'ref', about: 'ref', kind: 'sym', record: 'f64', time: 'time' });
/** What someone holds against someone (fades by itself: value at `at`). */
export const Grudge = defineComponent('grudge', { holder: 'ref', about: 'ref', value: 'f64', at: 'time' });

/** The data every witnessed fact carries (from the engine). */
export interface Witnessed {
  witnesses?: number[];
  /** Where it happened (world). */
  at?: V3;
}

/** A grudge now (0 if none). */
export function grudge(w: World, holder: number, about: number): number {
  const t = w.table(Grudge);
  for (let r = 0; r < t.count; r++) if (t.c.holder[r] === holder && t.c.about[r] === about) return approachAt(t.c.value[r], 0, GRUDGE_HALF, w.now - t.c.at[r]);
  return 0;
}

function addGrudge(w: World, holder: number, about: number, dv: number): number {
  const t = w.table(Grudge);
  for (let r = 0; r < t.count; r++) {
    if (t.c.holder[r] !== holder || t.c.about[r] !== about) continue;
    const v = approachAt(t.c.value[r], 0, GRUDGE_HALF, w.now - t.c.at[r]) + dv;
    t.c.value[r] = v;
    t.c.at[r] = w.now;
    return v;
  }
  const id = w.spawn();
  t.add(id, { holder, about, value: dv, at: w.now });
  return dv;
}

/** A fact the engine reported (World.fact made its record; `e.cause` is it). */
function witnessed(kind: string): EventDef {
  return {
    name: kind,
    run(w, e) {
      const data = (e.data ?? {}) as Witnessed;
      const def = FACTS[kind];
      const actor = w.log.actor(e.cause);
      for (const id of data.witnesses ?? []) {
        if (!w.alive(id) || !w.table(Person).has(id)) continue;
        // it saw it: someone of its own from now on
        if (w.table(Member).get(id, 'of') > 0) touch(w, id);
        const mem = w.spawn();
        w.table(Memory).add(mem, { holder: id, about: actor, kind: w.syms.id(kind), record: e.cause, time: w.now });
        const g = actor ? addGrudge(w, id, actor, def.weight) : 0;
        const rec = w.record('witnessed', id, actor, g, { keep: true, data: { kind } });
        if (g >= REACT_AT && def.say && data.at) {
          w.record('reacted', id, actor, g, { cause: rec, keep: true });
          say(w, id, def.say);
          order(w, { id, site: '', kind: 'goto', x: 0, z: 0, p: data.at, run: g > 0.8 });
        }
      }
    },
  };
}

/** Every grudge now. */
function grudges(w: World): number[] {
  const t = w.table(Grudge);
  const out: number[] = [];
  for (let r = 0; r < t.count; r++) out.push(approachAt(t.c.value[r], 0, GRUDGE_HALF, w.now - t.c.at[r]));
  return out;
}

export const knowledge: SimModule = {
  name: 'knowledge',
  components: [Memory, Grudge, Place],
  events: Object.keys(FACTS).map(witnessed),
  gauges: {
    'knowledge.memories': (w) => w.table(Memory).count,
    'knowledge.grudge_mean': (w) => {
      const g = grudges(w);
      return g.length ? g.reduce((a, b) => a + b, 0) / g.length : 0;
    },
    'knowledge.grudge_max': (w) => Math.max(0, ...grudges(w)),
  },
  requests: {
    /** What someone remembers (newest first) and what it holds against whom. */
    'knowledge.of': (w, o: { id: number }) => {
      const m = w.table(Memory);
      const memories: Array<{ about: number; kind: string; record: number; time: number }> = [];
      for (let r = 0; r < m.count; r++) if (m.c.holder[r] === o.id) memories.push({ about: m.c.about[r], kind: w.syms.name(m.c.kind[r]), record: m.c.record[r], time: m.c.time[r] });
      const g = w.table(Grudge);
      const grudges: Array<{ about: number; value: number }> = [];
      for (let r = 0; r < g.count; r++) if (g.c.holder[r] === o.id) grudges.push({ about: g.c.about[r], value: grudge(w, o.id, g.c.about[r]) });
      return { memories: memories.sort((a, b) => b.time - a.time), grudges };
    },
  },
};
