// A toy world that exercises every part of the core (tools only; the game's modules live in
// src/sim/modules): colonies whose food is a lazy quantity, a famine event computed from it and
// moved whenever its rate changes, births and deaths (entities come and go), raids from outside
// (facts), and a log whose causes chain: colony died ← famine ← raid.

import { defineComponent, LinearField, linearFields, type SimModule, type World } from '../../src/sim/index.js';

const DAY = 86400;
const YEAR = 365 * DAY;

export const Colony = defineComponent('colony', {
  pop: 'u32',
  ...linearFields('food'),
  /** The famine event (handle). */
  famine: 'f64',
  founded: 'time',
});

export const Person = defineComponent('person', {
  home: 'ref',
  born: 'time',
  /** Their death (handle). */
  death: 'f64',
});

/** Food eaten per person per second; grown per person per day of harvest, less as the colony crowds. */
const EAT = 1 / DAY;
const YIELD = 1.6;
const CROWD = 60;

const food = (w: World) => new LinearField(w.table(Colony), 'food', 0);

/** Sets the colony's consumption from its population and moves its famine to when food runs out. */
function replan(w: World, c: number): void {
  const t = w.table(Colony);
  const f = food(w);
  f.setRate(c, w.now, -t.get(c, 'pop') * EAT);
  const when = Math.ceil(f.when(c, w.now, 0));
  const h = t.get(c, 'famine');
  const next = w.pending(h) ? w.move(h, when) : w.schedule(when, 'colony.famine', c);
  t.set(c, 'famine', next);
}

function born(w: World, c: number): number {
  const p = w.spawn();
  const t = w.table(Person);
  t.add(p, { home: c, born: w.now });
  t.set(p, 'death', w.schedule(w.now + (20 + 60 * w.random(p)) * YEAR, 'person.death', p));
  w.table(Colony).set(c, 'pop', w.table(Colony).get(c, 'pop') + 1);
  return p;
}

function died(w: World, p: number): void {
  const t = w.table(Person);
  const c = t.get(p, 'home');
  w.cancel(t.get(p, 'death'));
  w.despawn(p);
  const colonies = w.table(Colony);
  if (colonies.has(c)) colonies.set(c, 'pop', colonies.get(c, 'pop') - 1);
}

function abandon(w: World, c: number): void {
  w.record('colony.died', 0, c);
  const people = w.table(Person);
  for (let r = people.count - 1; r >= 0; r--) if (people.c.home[r] === c) died(w, people.ids[r]);
  w.cancel(w.table(Colony).get(c, 'famine'));
  w.despawn(c);
}

export const colonies = (n: number, pop = 12): SimModule => ({
  name: 'colonies',
  components: [Colony, Person],
  events: [
    {
      name: 'colony.harvest',
      run(w, e) {
        const c = e.target;
        if (!w.alive(c)) return;
        const t = w.table(Colony);
        const pop = t.get(c, 'pop');
        const grown = pop * YIELD * (0.4 + w.random(c)) * (CROWD / (CROWD + pop));
        food(w).add(c, w.now, grown * 30);
        if (w.random(c) < 0.02) w.record('harvest.great', 0, c, grown);
        replan(w, c);
        w.after(30 * DAY, 'colony.harvest', c);
      },
    },
    {
      name: 'colony.famine',
      order: 1,
      run(w, e) {
        const c = e.target;
        if (!w.alive(c)) return;
        const t = w.table(Colony);
        t.set(c, 'famine', 0);
        const pop = t.get(c, 'pop');
        w.record('famine', 0, c, pop);
        const dead = Math.max(1, Math.ceil(pop * 0.25));
        const people = w.table(Person);
        let killed = 0;
        for (let r = people.count - 1; r >= 0 && killed < dead; r--) {
          if (people.c.home[r] !== c) continue;
          const p = people.ids[r];
          w.record('death.hunger', 0, p, 0, { keep: true });
          died(w, p);
          killed++;
        }
        if (t.get(c, 'pop') === 0) return abandon(w, c);
        food(w).set(c, w.now, t.get(c, 'pop') * 10);
        replan(w, c);
      },
    },
    {
      name: 'person.birth',
      run(w, e) {
        const c = e.target;
        if (!w.alive(c)) return;
        const p = born(w, c);
        w.record('birth', c, p, 0, { keep: true });
        replan(w, c);
        w.after(((0.1 + w.random(c)) * YEAR) / Math.max(1, w.table(Colony).get(c, 'pop')), 'person.birth', c);
      },
    },
    {
      name: 'person.death',
      run(w, e) {
        if (!w.alive(e.target)) return;
        const c = w.table(Person).get(e.target, 'home');
        w.record('death.age', 0, e.target, 0, { keep: true });
        died(w, e.target);
        if (!w.alive(c)) return;
        if (w.table(Colony).get(c, 'pop') === 0) abandon(w, c);
        else replan(w, c);
      },
    },
    {
      // a fact: someone took `a` food from colony `target`
      name: 'raid',
      run(w, e) {
        const c = e.target;
        if (!w.alive(c)) return;
        food(w).add(c, w.now, -e.a);
        replan(w, c);
      },
    },
  ],
  create(w) {
    for (let i = 0; i < n; i++) {
      const c = w.spawn();
      w.table(Colony).add(c, { founded: w.now });
      food(w).set(c, w.now, pop * 10 * (0.5 + w.random()), 0);
      for (let k = 0; k < pop; k++) born(w, c);
      w.record('colony.founded', 0, c, pop, { keep: true });
      replan(w, c);
      w.schedule(w.now + w.random(c) * 30 * DAY, 'colony.harvest', c);
      w.schedule(w.now + w.random(c) * YEAR, 'person.birth', c);
    }
  },
  gauges: {
    'colonies.alive': (w) => w.table(Colony).count,
    'colonies.population': (w) => w.table(Person).count,
    'colonies.food_mean': (w) => {
      const t = w.table(Colony);
      if (!t.count) return 0;
      const f = food(w);
      let sum = 0;
      for (let r = 0; r < t.count; r++) sum += f.at(t.ids[r], w.now);
      return sum / t.count;
    },
  },
});

/** The default scenario (worker tests import this module and use `modules`). */
export const modules: SimModule[] = [colonies(8)];

/** What must always hold; returns what doesn't. */
export function invariants(w: World): string[] {
  const bad: string[] = [];
  const cols = w.table(Colony);
  const people = w.table(Person);
  const f = food(w);
  const count = new Map<number, number>();
  for (let r = 0; r < people.count; r++) {
    const home = people.c.home[r];
    count.set(home, (count.get(home) ?? 0) + 1);
    if (!w.alive(people.ids[r])) bad.push(`persona ${people.ids[r]} en la tabla pero muerta`);
    if (!w.pending(people.c.death[r])) bad.push(`persona ${people.ids[r]} sin muerte programada`);
  }
  for (let r = 0; r < cols.count; r++) {
    const c = cols.ids[r];
    const pop = cols.c.pop[r];
    if ((count.get(c) ?? 0) !== pop) bad.push(`colonia ${c}: pop ${pop} ≠ ${count.get(c) ?? 0} personas`);
    const v = f.at(c, w.now);
    if (!(v >= 0)) bad.push(`colonia ${c}: comida ${v}`);
    const h = cols.c.famine[r];
    if (f.rate(c) < 0 && !w.pending(h)) bad.push(`colonia ${c}: consume y no tiene hambruna programada`);
    if (w.pending(h) && w.when(h) < w.now) bad.push(`colonia ${c}: hambruna en el pasado`);
  }
  for (const [home] of count) if (!cols.has(home)) bad.push(`personas de la colonia ${home}, que no existe`);
  return bad;
}
