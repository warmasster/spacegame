// The people of the world (docs/MUNDO.md §11). A site's crew is a cohort (kit/lod.ts): a number of
// people by job until someone comes close; then some of them are people — a name, a face, a job,
// a mind — and walk about with a body the engine gives them. Their minds decide, once a game hour
// or so, what to do by the time of day and their job (work at their post, stroll, rest); the
// engine walks them there. Going back to being a number, they come out the same next time (unless
// the crew changed for good).
//
// Someone the world touched (a witness: modules/knowledge.ts) leaves the number for good: an
// individual with its own level of detail (a body near a player, only a record far away) and a
// mind that keeps its memories.
//
// The world tells the engine what they do on the channel 'npc.task' (site coordinates, or a world
// point) and what they say on 'npc.say'.

import { HOUR } from '../core/calendar.js';
import { hash3, seedOf, draw } from '../core/rng.js';
import { defineComponent } from '../core/store.js';
import type { SimModule, World } from '../core/world.js';
import { Aggregate, aggregate, Member, Mix, observe, type Observer, type ObserveInput } from '../kit/lod.js';
import { Place, place, placeOf, type V3 } from '../kit/place.js';
import { JOBS, SITES, type CrewDef } from '../../shared/space/sites.js';
import { nameOf } from './names.js';

export const Person = defineComponent('person', {
  name: 'sym',
  /** Suit (as a player's variant). */
  variant: 'u8',
  /** Index in JOBS. */
  job: 'u8',
  /** Its crew (the cohort it came from). */
  crew: 'ref',
  /** What it is doing (index in ACTIVITIES). */
  activity: 'u8',
  /** Its next decision (handle). */
  mind: 'f64',
  /** The engine has its body now. */
  body: 'bool',
});

/** A cohort that is a site's crew. */
export const Crew = defineComponent('crew', { key: 'sym' });

export const ACTIVITIES = ['work', 'stroll', 'rest'] as const;

/** What a person does, for the engine: a site's point (site frame, m) or a world point (`p`). */
export interface NpcOrder {
  id: number;
  site: string;
  kind: 'goto' | 'wander' | 'idle';
  x: number;
  z: number;
  r?: number;
  fx?: number;
  fz?: number;
  /** A world point instead of the site's (x, z). */
  p?: V3;
  run?: boolean;
}

export interface CrewAt {
  /** site.crew */
  key: string;
  /** World position of where they gather. */
  p: V3;
  count: number;
  slots: number;
  enter: number;
  leave: number;
  mix: number[];
}

/** Someone with a body now. */
export interface MadePerson {
  id: number;
  name: string;
  variant: number;
  job: string;
  crew: string;
  index: number;
  /** An individual (touched) coming back: where it was left (world). */
  p?: V3;
}

const PERSON_SALT = 0x504552;

/** A crew's definition (its site's data) from its key. */
export function crewOf(w: World, crew: number): { site: string; def: CrewDef } | null {
  const key = w.syms.name(w.table(Crew).get(crew, 'key'));
  const [site, id] = key.split('.');
  const def = SITES.find((s) => s.id === site && s.kind === 'base');
  const c = def && def.kind === 'base' ? def.crew?.find((x) => x.id === id) : undefined;
  return c ? { site, def: c } : null;
}

/** Makes a member of a crew a person: who they are comes from its slot's key (same slot, same person). */
function becomePerson(w: World, crew: number, member: number, index: number): void {
  const version = w.table(Aggregate).get(crew, 'version');
  const key = seedOf(w.seed ^ PERSON_SALT, crew, index + version * 1024);
  const bucket = w.table(Member).get(member, 'bucket');
  w.table(Person).add(member, { name: w.syms.id(nameOf(key)), variant: 1 + (draw(key, 2) % 3), job: bucket === 0xffff ? 0 : bucket, crew, body: 1 });
  // decides soon (spread: not all in the same instant)
  const h = w.after(30 + (draw(key, 3) % 600), 'person.decide', member);
  w.table(Person).set(member, 'mind', h);
}

/** Utilities of each activity now (by the hour and the job), and one drawn by them. */
function choose(w: World, id: number, job: number): number {
  const hour = w.calendar.parts(w.now).hour;
  const day = hour >= 7 && hour < 19;
  const guard = JOBS[job] === 'guardia';
  const u = day ? [guard ? 2 : 3, 1, 0.4] : [guard ? 2.5 : 0.3, 0.4, 3];
  let pick = w.random(id) * u.reduce((a, b) => a + b, 0);
  for (let i = 0; i < u.length; i++) {
    if (pick < u[i]) return i;
    pick -= u[i];
  }
  return u.length - 1;
}

/** Tells the engine what someone with a body does (nothing if it has none now). */
export function order(w: World, o: NpcOrder): void {
  if (w.table(Person).get(o.id, 'body') === 1) w.emit('npc.task', o);
}

/** Tells the engine what someone says (only with a body: someone there to hear it). */
export function say(w: World, id: number, text: string): void {
  if (w.table(Person).get(id, 'body') === 1) w.emit('npc.say', { id, text });
}

/** Distance from the nearest observer to a world point (0: one is in its host). */
function nearest(observers: readonly Observer[], p: V3): number {
  let d = Infinity;
  for (const o of observers) d = Math.min(d, Math.hypot(o.p[0] - p[0], o.p[1] - p[1], o.p[2] - p[2]));
  return d;
}

export const people: SimModule = {
  name: 'people',
  components: [Person, Crew, Place, Aggregate, Member, Mix],
  events: [
    {
      name: 'person.decide',
      run(w, e) {
        const id = e.target;
        const t = w.table(Person);
        // back to being a number since: nothing to decide
        if (!w.alive(id) || !t.has(id)) return;
        const crew = crewOf(w, t.get(id, 'crew'));
        if (crew) {
          const job = t.get(id, 'job');
          const act = choose(w, id, job);
          t.set(id, 'activity', act);
          const d = crew.def;
          if (ACTIVITIES[act] === 'work') {
            const posts = d.work[job] ?? d.work[0];
            const post = posts[draw(hash3(w.seed, id, 0x574f524b), 0) % posts.length];
            order(w, { id, site: crew.site, kind: 'goto', x: post.x, z: post.z, fx: post.fx, fz: post.fz });
          } else if (ACTIVITIES[act] === 'stroll') order(w, { id, site: crew.site, kind: 'wander', x: d.stroll.x, z: d.stroll.z, r: d.stroll.r });
          else order(w, { id, site: crew.site, kind: 'goto', x: d.rest.x + (w.random(id) - 0.5) * 4, z: d.rest.z + (w.random(id) - 0.5) * 4 });
        }
        t.set(id, 'mind', w.after(HOUR * (0.5 + w.random(id)), 'person.decide', id));
      },
    },
  ],
  requests: {
    /** The crews of the engine's sites (those it doesn't have yet are made). */
    'people.register': (w, list: CrewAt[]) => {
      const known = new Set<string>();
      const t = w.table(Crew);
      for (let r = 0; r < t.count; r++) known.add(w.syms.name(t.c.key[r]));
      let made = 0;
      for (const c of list) {
        if (known.has(c.key)) continue;
        const id = w.spawn();
        w.table(Crew).add(id, { key: w.syms.id(c.key) });
        place(w, id, 0, c.p);
        aggregate(w, id, { kind: 'person', amount: c.count, slots: c.slots, enter: c.enter, leave: c.leave, mix: c.mix });
        w.record('crew.founded', 0, id, c.count, { keep: true });
        made++;
      }
      return made;
    },

    /**
     * Where the players are: crews come out as people or go back to being a number; individuals
     * (touched) get a body near a player and lose it far away (they stay).
     */
    'people.observe': (w, input: ObserveInput) => {
      const isCrew = (agg: number) => w.table(Crew).has(agg);
      const change = observe(w, input, isCrew, { made: (w, crew, member, index) => becomePerson(w, crew, member, index) });
      const up: MadePerson[] = [];
      const down: number[] = [];
      const t = w.table(Person);
      const made = (id: number, crew: number, index: number, p?: V3): MadePerson => ({ id, name: w.syms.name(t.get(id, 'name')), variant: t.get(id, 'variant'), job: JOBS[t.get(id, 'job')] ?? JOBS[0], crew: w.syms.name(w.table(Crew).get(crew, 'key')), index, ...(p ? { p } : {}) });
      for (const u of change.up) for (const id of u.members) up.push(made(id, u.agg, w.table(Member).get(id, 'index')));
      for (const d of change.down) down.push(...d.members);
      // individuals: by their own distance, with their crew's radii
      const m = w.table(Member);
      const a = w.table(Aggregate);
      for (let r = 0; r < t.count; r++) {
        const id = t.ids[r];
        if (m.get(id, 'of') > 0) continue;
        const crew = t.c.crew[r];
        const at = placeOf(w, id);
        if (!at) continue;
        const d = nearest(input.observers, at.p);
        if (!t.c.body[r] && d < a.get(crew, 'enter')) {
          t.c.body[r] = 1;
          up.push(made(id, crew, m.get(id, 'index'), at.p));
        } else if (t.c.body[r] && d > a.get(crew, 'leave')) {
          t.c.body[r] = 0;
          down.push(id);
        }
      }
      return { up, down };
    },

    /** Where the people with a body are now (world). */
    'people.at': (w, list: Array<{ id: number; p: V3 }>) => {
      for (const o of list) if (w.table(Person).has(o.id)) place(w, o.id, 0, o.p);
    },

    /** Every crew: people in it (a number), out now, gone their own way. */
    'people.state': (w) => {
      const t = w.table(Crew);
      const out: Array<{ key: string; amount: number; live: number; out: number }> = [];
      for (let r = 0; r < t.count; r++) {
        const id = t.ids[r];
        const m = w.table(Member);
        let live = 0;
        for (let k = 0; k < m.count; k++) if (m.c.of[k] === id) live++;
        out.push({ key: w.syms.name(t.c.key[r]), amount: w.table(Aggregate).get(id, 'amount'), live, out: w.table(Aggregate).get(id, 'out') });
      }
      return out;
    },
  },
  gauges: {
    /** People kept as numbers in the crews. */
    'people.in_crews': (w) => {
      const t = w.table(Crew);
      let sum = 0;
      for (let r = 0; r < t.count; r++) sum += w.table(Aggregate).get(t.ids[r], 'amount');
      return sum;
    },
    /** People with a body now. */
    'people.embodied': (w) => {
      const t = w.table(Person);
      let n = 0;
      for (let r = 0; r < t.count; r++) n += t.c.body[r];
      return n;
    },
    /** People of their own (touched: they left their crew for good). */
    'people.individuals': (w) => {
      const t = w.table(Person);
      let n = 0;
      for (let r = 0; r < t.count; r++) if (!(w.table(Member).get(t.ids[r], 'of') > 0)) n++;
      return n;
    },
  },
  describe: (w, id) => {
    const t = w.table(Person);
    if (t.has(id)) return { what: 'persona', name: w.syms.name(t.get(id, 'name')), job: JOBS[t.get(id, 'job')], individual: w.table(Member).get(id, 'of') === 0 };
    if (w.table(Crew).has(id)) return { what: 'dotación', key: w.syms.name(w.table(Crew).get(id, 'key')), people: w.table(Aggregate).get(id, 'amount') };
    return null;
  },
};
