// The world's kit, headless (part of npm run test:sim): places and levels of detail.
//
//   places        near() against brute force (hosts, moves, despawns)
//   conservation  aggregates promoted, demoted, touched, lost, saved and loaded at random: their
//                 ledger never changes, and their mixes add up to the start
//   cohorts       20 people drawn from 340 of whom 102 hate the governor: 6 ± 2 hate him
//   hysteresis    up within enter, nothing between enter and leave, down beyond leave; a host's
//                 aggregate is up for whoever is aboard
//   objects       the game's objects module: register, depots out and back, taking one for good
//
// Exit code 1 if anything fails. `--verbose` for the numbers.

import { digest, restore, Rng, snapshot, World, type SimModule } from '../../src/sim/index.js';
import { aggregate, Aggregate, ledger, lose, Member, membersOf, Mix, MIX_KEYS, observe, touch } from '../../src/sim/kit/lod.js';
import { near, place, Place } from '../../src/sim/kit/place.js';
import { objects, type ThingAt } from '../../src/sim/modules/objects.js';

const verbose = process.argv.includes('--verbose');
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}
type V3 = [number, number, number];
const kit: SimModule = { name: 'kit', components: [Place, Aggregate, Member, Mix] };

// --- places ---------------------------------------------------------------------------------------
{
  const w = World.create({ seed: 1, modules: [kit] });
  const r = new Rng(3);
  const ids: number[] = [];
  for (let i = 0; i < 3000; i++) {
    const id = w.spawn();
    ids.push(id);
    place(w, id, r.chance(0.2) ? 1 + r.int(3) : 0, [r.range(-3000, 3000), r.range(-50, 50), r.range(-3000, 3000)]);
  }
  for (let i = 0; i < 2000; i++) {
    const id = ids[r.int(ids.length)];
    if (!w.alive(id)) continue;
    if (r.chance(0.3)) w.despawn(id);
    else place(w, id, r.chance(0.2) ? 1 + r.int(3) : 0, [r.range(-3000, 3000), r.range(-50, 50), r.range(-3000, 3000)]);
  }
  let bad = 0;
  const t = w.table(Place);
  for (let q = 0; q < 300; q++) {
    const host = r.chance(0.3) ? 1 + r.int(3) : 0;
    const p: V3 = [r.range(-3000, 3000), 0, r.range(-3000, 3000)];
    const rad = r.range(10, 800);
    const want: number[] = [];
    for (let row = 0; row < t.count; row++) {
      if (t.c.host[row] !== host) continue;
      const d = Math.hypot(t.c.x[row] - p[0], t.c.y[row] - p[1], t.c.z[row] - p[2]);
      if (d <= rad) want.push(t.ids[row]);
    }
    if (want.sort((a, b) => a - b).join() !== near(w, host, p, rad).join()) bad++;
  }
  check('lugares: «qué hay cerca» = fuerza bruta (anfitriones, movimientos, bajas)', bad === 0, { bad });
}

// --- conservation ---------------------------------------------------------------------------------
{
  let w = World.create({ seed: 7, modules: [kit] });
  const r = new Rng(8);
  const aggs: number[] = [];
  const initial = new Map<number, { total: number; mix: number[] }>();
  for (let i = 0; i < 6; i++) {
    const id = w.spawn();
    place(w, id, i % 3 === 0 ? 5 : 0, [i * 400, 0, 0]);
    const mix = [30 + r.int(50), 10 + r.int(40), r.int(20)];
    const amount = mix.reduce((a, b) => a + b, 0);
    aggregate(w, id, { kind: 'people', amount, slots: 8 + r.int(12), enter: 150, leave: 250, mix });
    aggs.push(id);
    initial.set(id, { total: amount, mix });
  }
  // categories that left each aggregate with a member (touched or lost while in it)
  const left = new Map<number, number[]>(aggs.map((a) => [a, [0, 0, 0]]));
  let brokenLedger = 0, brokenMix = 0, cycles = 0;
  for (let step = 0; step < 600; step++) {
    const op = r.float();
    if (op < 0.5) {
      const obs = Array.from({ length: 1 + r.int(2) }, () => ({ host: r.chance(0.1) ? 5 : 0, p: [r.range(-300, 2300), 0, r.range(-200, 200)] as V3 }));
      const ch = observe(w, { observers: obs, hosts: [[5, r.range(-100, 100), 0, 0]] });
      cycles += ch.up.length + ch.down.length;
    } else if (op < 0.75) {
      const m = w.table(Member);
      if (m.count) {
        const id = m.ids[r.int(m.count)];
        const agg = m.get(id, 'of');
        const bucket = m.get(id, 'bucket');
        if (touch(w, id) && bucket !== 0xffff) left.get(agg)![bucket]++;
      }
    } else if (op < 0.9) {
      const m = w.table(Member);
      if (m.count) {
        const id = m.ids[r.int(m.count)];
        const agg = m.get(id, 'of');
        const bucket = m.get(id, 'bucket');
        if (agg && bucket !== 0xffff) left.get(agg)![bucket]++;
        lose(w, id);
      }
    } else {
      // save and load in the middle of it all
      const s = snapshot(w, { all: true });
      const back = restore(s.main, s.segments.map((x) => x.bytes), [kit]).world;
      if (digest(back) !== digest(w)) brokenLedger += 1000;
      w = back;
    }
    for (const agg of aggs) {
      const l = ledger(w, agg);
      if (Math.abs(l.total - initial.get(agg)!.total) > 1e-9) brokenLedger++;
      const mix = w.table(Mix);
      const inMembers = [0, 0, 0];
      for (const id of membersOf(w, agg)) inMembers[w.table(Member).get(id, 'bucket')]++;
      for (let k = 0; k < 3; k++) if (mix.get(agg, MIX_KEYS[k]) + inMembers[k] + left.get(agg)![k] !== initial.get(agg)!.mix[k]) brokenMix++;
    }
  }
  check('conservación: al azar (subir, bajar, tocar, perder, guardar y cargar) el libro de cada agregado nunca cambia y sus mezclas cuadran', brokenLedger === 0 && brokenMix === 0 && cycles > 50, { brokenLedger, brokenMix, cycles });
}

// --- cohorts --------------------------------------------------------------------------------------
{
  const w = World.create({ seed: 11, modules: [kit] });
  const counts: number[] = [];
  const cohorts: number[] = [];
  // 3000 stations, each with its own 340 workers
  for (let i = 0; i < 3000; i++) {
    const c = w.spawn();
    cohorts.push(c);
    place(w, c, 0, [i * 10_000, 0, 0]);
    aggregate(w, c, { kind: 'workers', amount: 340, slots: 20, enter: 100, leave: 200, mix: [102, 238] });
    observe(w, { observers: [{ host: 0, p: [i * 10_000, 0, 0] }] }, (a) => a === c);
    counts.push(membersOf(w, c).filter((id) => w.table(Member).get(id, 'bucket') === 0).length);
    observe(w, { observers: [] }, (a) => a === c);
  }
  const mean = counts.reduce((a, b) => a + b, 0) / counts.length;
  const sd = Math.sqrt(counts.reduce((a, b) => a + (b - mean) ** 2, 0) / counts.length);
  // coming back to the same station: the same people (the same categories slot by slot)
  const c = cohorts[0];
  const who = () => {
    observe(w, { observers: [{ host: 0, p: [0, 0, 0] }] }, (a) => a === c);
    const b = membersOf(w, c).map((id) => w.table(Member).get(id, 'bucket')).join('');
    observe(w, { observers: [] }, (a) => a === c);
    return b;
  };
  const first = who();
  const same = who() === first && who() === first;
  // one of them leaves for good: next time the draw is new (and still adds up)
  observe(w, { observers: [{ host: 0, p: [0, 0, 0] }] }, (a) => a === c);
  touch(w, membersOf(w, c)[0]);
  observe(w, { observers: [] }, (a) => a === c);
  const after = who();
  // hypergeometric: 20·0.3 = 6, sd √(20·0.3·0.7·320/339) ≈ 1.99
  check('cohortes: de 20 sacados de 340 (102 odian al gobernador) lo odian 6 ± 2; al volver salen los mismos; si uno se va para siempre, se sortea de nuevo', Math.abs(mean - 6) < 0.15 && Math.abs(sd - 1.99) < 0.15 && same && after !== first && ledger(w, c).total === 340, { mean: mean.toFixed(3), sd: sd.toFixed(3), first, after });
}

// --- hysteresis -----------------------------------------------------------------------------------
{
  const w = World.create({ seed: 12, modules: [kit] });
  const a = w.spawn();
  place(w, a, 0, [0, 0, 0]);
  aggregate(w, a, { kind: 'x', amount: 10, slots: 4, enter: 100, leave: 150 });
  const h = w.spawn();
  place(w, h, 9, [2, 0, 3]);
  aggregate(w, h, { kind: 'x', amount: 10, slots: 4, enter: 100, leave: 150 });
  const at = (d: number, host = 0) => observe(w, { observers: [{ host, p: [d, 0, 0] }], hosts: [[9, 5000, 0, 0]] });
  const live = (id: number) => w.table(Aggregate).get(id, 'live') === 1;
  const steps = [
    [at(120), live(a)], // between: still down
    [at(90), live(a)], // within enter: up
    [at(140), live(a)], // between: still up
    [at(160), live(a)], // beyond leave: down
  ];
  at(99999, 9);
  const aboard = live(h) && !live(a);
  check('histéresis: sube dentro de enter, nada entre enter y leave, baja más allá de leave; a bordo del anfitrión, siempre arriba', steps.map((s) => s[1]).join() === 'false,true,true,false' && aboard, { steps: steps.map((s) => s[1]) });
}

// --- the objects module -------------------------------------------------------------------------------
{
  const w = World.create({ seed: 13, modules: [objects] });
  const spec = { half: [0.3, 0.3, 0.3] as V3, mass: 30, paint: 'grey' as const };
  const ask = <T>(name: string, payload?: unknown) => w.request(name, payload) as T;
  const ids = ask<number[]>('objects.register', [{ spec, host: 0, p: [1, 0, 1], q: [0, 0, 0, 1] }, { spec: { ...spec, kind: 'part' }, host: 3, p: [0, 1, 0], q: [0, 0, 0, 1] }]);
  ask('depots.register', [{ key: 'base.almacen', spec, host: 0, p: [100, 0, 0], count: 40, slots: 9, enter: 300, leave: 450 }]);
  ask('depots.register', [{ key: 'base.almacen', spec, host: 0, p: [100, 0, 0], count: 40, slots: 9, enter: 300, leave: 450 }]);
  const near1 = ask<{ up: Array<ThingAt & { index: number }>; down: number[] }>('objects.observe', { observers: [{ host: 0, p: [0, 0, 0] }] });
  const listed = ask<ThingAt[]>('objects.list');
  const taken = near1.up[4].id;
  const took = ask<{ detached: boolean }>('objects.taken', { id: taken, by: 77 });
  ask('objects.rest', [{ id: taken, host: 0, p: [5, 0, 5], q: [0, 0, 0, 1] }]);
  const far = ask<{ up: ThingAt[]; down: number[] }>('objects.observe', { observers: [{ host: 0, p: [2000, 0, 0] }] });
  const after = ask<ThingAt[]>('objects.list');
  const state = ask<Array<{ key: string; amount: number; live: number; out: number }>>('depots.state')[0];
  const back = ask<{ up: ThingAt[] }>('objects.observe', { observers: [{ host: 0, p: [0, 0, 0] }] });
  const moved = after.find((t) => t.id === taken);
  const fact = w.log.about(taken).map((id) => w.syms.name(w.log.kind(id)));
  check('objetos: registro, almacén que saca 9 y los recoge, el que se coge es suyo para siempre (y queda donde lo dejaron)',
    ids.length === 2 && near1.up.length === 9 && listed.length === 11 && took.detached && far.down.length === 8 && after.length === 3 && moved?.p[0] === 5 && state.amount === 39 && state.out === 1 && back.up.length === 9 && fact.includes('object.taken') && after.some((t) => t.spec.kind === 'part' && t.host === 3),
    { up: near1.up.length, listed: listed.length, down: far.down.length, after: after.length, state, back: back.up.length, fact });
}

console.log(failures ? `\n${failures} fallo(s)` : '\nKit del mundo en orden.');
process.exit(failures ? 1 : 0);
