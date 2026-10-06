// How fast the world core is (npm run bench:sim), on one core:
//
//   timeline   scheduling and popping a million events
//   events     a big toy world (thousands of colonies, tens of thousands of people) for years
//   sweep      a closed-form update over a million rows of one table (the prehistory's L0/L1 work)
//   saves      snapshot, gzip and load of that big world
//
// and what that means for the report's prehistory: 15,600 million updates (300 years).

import { digest, EventQueue, linearAt, linearFields, MemoryStorage, Rng, Table, defineComponent, World, WorldSaves } from '../../src/sim/index.js';
import { colonies } from './scenario.js';

const DAY = 86400;
const YEAR = 365 * DAY;
const fmt = (n: number) => n.toLocaleString('es-ES', { maximumFractionDigits: 0 });
const rate = (n: number, ms: number) => `${fmt((n / ms) * 1000)}/s`;

// timeline
{
  const q = new EventQueue();
  const r = new Rng(1);
  const N = 1_000_000;
  const out = { time: 0, kind: 0, target: 0, a: 0, b: 0, cause: 0, data: undefined as unknown, handle: 0 };
  const t0 = performance.now();
  for (let i = 0; i < N; i++) q.push(r.float() * YEAR, 0, 0, i, 0, 0, 0);
  const t1 = performance.now();
  while (q.pop(out));
  const t2 = performance.now();
  console.log(`cola:        ${fmt(N)} programados en ${(t1 - t0).toFixed(0)} ms (${rate(N, t1 - t0)}), sacados en ${(t2 - t1).toFixed(0)} ms (${rate(N, t2 - t1)})`);
}

// the core's own event rate: 100 000 things deciding once an hour (the report's L2 cadence)
{
  const Mind = defineComponent('mind', { ...linearFields('mood'), decisions: 'u32' });
  const N = 100_000;
  const w = World.create({
    seed: 3,
    modules: [
      {
        name: 'minds',
        components: [Mind],
        events: [
          {
            name: 'decide',
            run(w, e) {
              const t = w.table(Mind);
              const r = t.row(e.target);
              const c = t.c;
              c.mood[r] = linearAt(c.mood[r], c.moodRate[r], w.now - c.moodAt[r], -1, 1);
              c.moodAt[r] = w.now;
              c.moodRate[r] = (w.random(e.target) - 0.5) * 1e-4;
              c.decisions[r]++;
              w.after(3600, 'decide', e.target);
            },
          },
        ],
        create(w) {
          for (let i = 0; i < N; i++) {
            const id = w.spawn();
            w.table(Mind).add(id);
            w.schedule(w.random(id) * 3600, 'decide', id);
          }
        },
      },
    ],
  });
  const t0 = performance.now();
  w.advance(10 * DAY);
  const ms = performance.now() - t0;
  console.log(`núcleo:      ${fmt(N)} mentes decidiendo cada hora, 10 días: ${fmt(w.stats.events)} eventos en ${ms.toFixed(0)} ms (${rate(w.stats.events, ms)})`);
}

// events of a whole toy world (its famines walk every person: O(n) per famine, the toy's cost, not the core's)
let big: World;
{
  const t0 = performance.now();
  big = World.create({ seed: 1, modules: [colonies(3000, 12)] });
  const t1 = performance.now();
  big.advance(3 * YEAR);
  const t2 = performance.now();
  console.log(`mundo:       3000 colonias, ${fmt(big.store.entities.count)} entidades vivas; creado en ${(t1 - t0).toFixed(0)} ms`);
  console.log(`             3 años: ${fmt(big.stats.events)} eventos en ${(t2 - t1).toFixed(0)} ms (${rate(big.stats.events, t2 - t1)}), ${fmt(big.log.count)} registros`);
}

// sweep
let perSecond = 0;
{
  const N = 1_000_000;
  const T = defineComponent('cohort', { ...linearFields('grievance'), size: 'u32' });
  const t = new Table(T, N);
  const r = new Rng(2);
  for (let i = 1; i <= N; i++) t.add(i, { grievance: r.float(), grievanceRate: (r.float() - 0.5) * 1e-6, size: 100 });
  const v = t.c.grievance, k = t.c.grievanceRate, at = t.c.grievanceAt;
  const passes = 20;
  const t0 = performance.now();
  for (let p = 1; p <= passes; p++) {
    const now = p * 7 * DAY;
    for (let row = 0; row < t.count; row++) {
      v[row] = linearAt(v[row], k[row], now - at[row], 0, 1);
      at[row] = now;
    }
  }
  const ms = performance.now() - t0;
  perSecond = (N * passes * 1000) / ms;
  console.log(`barrido:     ${fmt(N)} filas × ${passes} pasadas en ${ms.toFixed(0)} ms → ${fmt(perSecond)} actualizaciones/s`);
  const pre = 15.6e9;
  console.log(`             prehistoria del informe (${fmt(pre)} actualizaciones): ${(pre / perSecond / 60).toFixed(1)} min en 1 núcleo, ${(pre / perSecond / 60 / 4).toFixed(1)} min en 4`);
}

// saves
{
  const storage = new MemoryStorage();
  const saves = new WorldSaves(storage);
  const t0 = performance.now();
  const info = await saves.save(big);
  const t1 = performance.now();
  const loaded = (await new WorldSaves(storage).load([colonies(3000, 12)]))!;
  const t2 = performance.now();
  const same = digest(loaded.world) === digest(big);
  console.log(`guardado:    ${fmt(info.bytes / 1024)} KB comprimido (${info.segments} segmentos) en ${(t1 - t0).toFixed(0)} ms; carga en ${(t2 - t1).toFixed(0)} ms; idéntico: ${same ? 'sí' : 'NO'}`);
}
