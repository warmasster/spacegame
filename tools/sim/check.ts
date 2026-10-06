// The world core, headless (npm run test:sim). Every guarantee of docs/MUNDO.md, measured:
//
//   randomness   uniform, independent between draws and between entities, reproducible
//   timeline     the heap against a sorted reference (ties, cancels, moves), 200k random operations
//   tables       against a Map (swap-remove, growth, huge ids)
//   closed forms against stepping; calendar and clock
//   determinism  a world run in one go = the same world run in random slices with random budgets
//   save / load  continuing a loaded world = never having stopped (in memory, and on disk: gzip,
//                incremental segments, two slots); damaged saves fall back; old saves with other
//                fields / tables / events load
//   causes       colony died ← famine ← raid: every link
//   budget       advance() stops on time
//   thread       the runner in-process and on a real Node worker: facts, queries, autosave,
//                stop-and-resume, one process per world folder
//
// Exit code 1 if anything fails. `--verbose` for the numbers.

import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  approachAt,
  approachWhen,
  Calendar,
  defineComponent,
  digest,
  draw,
  EventQueue,
  hash3,
  LinearField,
  linearAt,
  linearFields,
  linearWhen,
  MemoryStorage,
  restore,
  Rng,
  SAVE_SLOTS,
  snapshot,
  Table,
  unit,
  World,
  WorldClock,
  WorldSaves,
  type SimModule,
} from '../../src/sim/index.js';
import type { SimOut, SimIn, RecordView, SimStatus } from '../../src/sim/host/protocol.js';
import { SimRunner } from '../../src/sim/host/runner.js';
import { FsStorage } from '../../src/sim/host/node/fsStorage.js';
import { launchWorld } from '../../src/sim/host/node/launch.js';
import { Colony, colonies, invariants } from './scenario.js';

const verbose = process.argv.includes('--verbose');
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const DAY = 86400;
const YEAR = 365 * DAY;
const tmp = mkdtempSync(join(tmpdir(), 'sim-'));

// --- randomness -------------------------------------------------------------------------------
{
  const key = hash3(1969, 7, 1);
  const N = 1_000_000;
  const bins = new Float64Array(100);
  let sx = 0, sxx = 0, sxy = 0, prev = unit(key, 0);
  for (let i = 1; i <= N; i++) {
    const u = unit(key, i);
    bins[Math.floor(u * 100)]++;
    sx += u;
    sxx += u * u;
    sxy += u * prev;
    prev = u;
  }
  const e = N / 100;
  const chi = bins.reduce((s, b) => s + ((b - e) * (b - e)) / e, 0);
  const mean = sx / N;
  const serial = (sxy / N - mean * mean) / (sxx / N - mean * mean);
  // neighbouring entities' first draws
  const M = 200_000;
  let ax = 0, ay = 0, axx = 0, ayy = 0, axy = 0;
  const firsts = new Float64Array(20);
  for (let id = 0; id < M; id++) {
    const x = unit(hash3(1969, id, 0x524e47), 0);
    const y = unit(hash3(1969, id + 1, 0x524e47), 0);
    firsts[Math.floor(x * 20)]++;
    ax += x; ay += y; axx += x * x; ayy += y * y; axy += x * y;
  }
  const cross = (axy / M - (ax / M) * (ay / M)) / Math.sqrt((axx / M - (ax / M) ** 2) * (ayy / M - (ay / M) ** 2));
  const fe = M / 20;
  const chiIds = firsts.reduce((s, b) => s + ((b - fe) * (b - fe)) / fe, 0);
  // keys a small xor apart must not be the same stream shifted
  let shifted = 0;
  for (let n = 0; n < 5000; n++) if (draw(key, n + 1) === draw(key ^ 1, n) || draw(key, n) === draw(key ^ 1, n + 1)) shifted++;
  check('azar: uniforme (χ² de 100 casillas < 150), sin correlación serie (|r| < 0,005)', chi < 150 && Math.abs(serial) < 0.005, { chi: chi.toFixed(1), serial: serial.toExponential(2) });
  check('azar: entidades vecinas independientes (|r| < 0,01, χ² de sus primeras tiradas < 45) y claves parecidas no se solapan', Math.abs(cross) < 0.01 && chiIds < 45 && shifted === 0, { cross: cross.toExponential(2), chiIds: chiIds.toFixed(1), shifted });
  const a = World.create({ seed: 5 });
  const b = World.create({ seed: 5 });
  const c = World.create({ seed: 6 });
  const seq = (w: World) => Array.from({ length: 8 }, () => w.random(42));
  const sa = seq(a), sb = seq(b), sc = seq(c);
  const r = new Rng(123);
  const n1 = Array.from({ length: 5 }, () => r.float());
  const resumed = new Rng(123, 2);
  check('azar: reproducible por semilla y entidad; un flujo se reanuda desde su contador', sa.join() === sb.join() && sa.join() !== sc.join() && resumed.float() === n1[2]);
}

// --- timeline ---------------------------------------------------------------------------------
{
  const q = new EventQueue(4);
  const rng = new Rng(77);
  type Ref = { t: number; o: number; tg: number; s: number };
  const ref = new Map<number, Ref>();
  const live: number[] = [];
  const at = new Map<number, number>();
  const addLive = (h: number) => (at.set(h, live.length), live.push(h));
  const dropLive = (h: number) => {
    const i = at.get(h)!;
    const last = live.pop()!;
    if (last !== h) (live[i] = last), at.set(last, i);
    at.delete(h);
  };
  const dead: number[] = [];
  let seq = 0, wrong = 0, staleOk = 0, pops = 0;
  const out = { time: 0, kind: 0, target: 0, a: 0, b: 0, cause: 0, data: undefined as unknown, handle: 0 };
  const expectNext = () => {
    let best = -1, bv: Ref | null = null;
    for (const [h, v] of ref) if (!bv || v.t < bv.t || (v.t === bv.t && (v.o < bv.o || (v.o === bv.o && (v.tg < bv.tg || (v.tg === bv.tg && v.s < bv.s)))))) (best = h), (bv = v);
    return best;
  };
  for (let i = 0; i < 200_000; i++) {
    const op = rng.float();
    if (op < 0.45 || live.length === 0) {
      const v = { t: rng.int(60), o: rng.int(3), tg: rng.int(6), s: seq++ };
      const h = q.push(v.t, v.o, 0, v.tg, 0, 0, 0);
      ref.set(h, v);
      addLive(h);
    } else if (op < 0.6) {
      const h = live[rng.int(live.length)];
      if (!q.cancel(h)) wrong++;
      ref.delete(h);
      dropLive(h);
      dead.push(h);
    } else if (op < 0.75) {
      const h = live[rng.int(live.length)];
      const t = rng.int(60);
      if (!q.move(h, t, 0)) wrong++;
      ref.get(h)!.t = t;
    } else if (op < 0.78 && dead.length) {
      if (q.cancel(dead[rng.int(dead.length)]) || q.move(dead[rng.int(dead.length)], 1, 0)) staleOk++;
    } else if (ref.size < 400 || op < 0.99) {
      // pop only when small enough to check against the reference cheaply
      if (ref.size > 400) continue;
      const want = expectNext();
      q.pop(out);
      pops++;
      if (out.handle !== want || out.time !== ref.get(want)!.t) wrong++;
      ref.delete(want);
      dropLive(want);
      dead.push(want);
    }
  }
  while (ref.size) {
    const want = expectNext();
    q.pop(out);
    pops++;
    if (out.handle !== want) wrong++;
    ref.delete(want);
  }
  check('línea de tiempo: 200 000 operaciones contra la referencia ordenada (empates por orden, destino, programación)', wrong === 0 && staleOk === 0 && q.size === 0, { wrong, staleOk, pops });
}

// --- tables -----------------------------------------------------------------------------------
{
  const T = defineComponent('t', { x: 'f64', y: 'u8', z: { type: 'i32', default: -5 } });
  const t = new Table(T, 2);
  const ref = new Map<number, [number, number, number]>();
  const rng = new Rng(9);
  let wrong = 0;
  for (let i = 0; i < 100_000; i++) {
    const id = rng.chance(0.01) ? 3_000_000_000 + rng.int(1000) : 1 + rng.int(5000);
    const op = rng.float();
    if (op < 0.5) {
      const x = rng.float(), y = rng.int(256);
      const had = ref.get(id);
      t.add(id, { x, y });
      ref.set(id, [x, y, had ? had[2] : -5]);
    } else if (op < 0.8) {
      if (t.remove(id) !== ref.delete(id)) wrong++;
    } else if (ref.has(id)) {
      const z = rng.int(1000) - 500;
      t.set(id, 'z', z);
      ref.get(id)![2] = z;
    }
  }
  if (t.count !== ref.size) wrong++;
  for (const [id, [x, y, z]] of ref) if (t.get(id, 'x') !== x || t.get(id, 'y') !== y || t.get(id, 'z') !== z) wrong++;
  for (let r = 0; r < t.count; r++) if (t.row(t.ids[r]) !== r || !ref.has(t.ids[r])) wrong++;
  check('tablas: 100 000 altas/bajas/cambios contra un Map (crecimiento, ids enormes, valores por defecto)', wrong === 0, { rows: t.count, wrong });
}

// --- closed forms, calendar, clock -------------------------------------------------------------
{
  let v = 100, worst = 0;
  for (let s = 1; s <= 1000; s++) {
    v += -0.07;
    worst = Math.max(worst, Math.abs(v - linearAt(100, -0.07, s)));
  }
  const tw = linearWhen(100, -0.07, 0);
  const aw = approachWhen(10, 2, 3600, 3);
  const T = defineComponent('stock', linearFields('iron'));
  const table = new Table(T);
  table.add(1);
  const iron = new LinearField(table as unknown as Table, 'iron', 0);
  iron.set(1, 1000, 50, -0.01);
  const when = iron.when(1, 1000, 0);
  check('perezoso: forma cerrada = paso a paso; «cuándo» cae donde el valor llega', worst < 1e-9 && Math.abs(linearAt(100, -0.07, tw)) < 1e-9 && Math.abs(approachAt(10, 2, 3600, aw) - 3) < 1e-9 && iron.at(1, when) === 0 && iron.at(1, when - 1) > 0 && iron.at(1, when + 1e6) === 0, { worst, when });
  const cal = new Calendar();
  const rng = new Rng(4);
  let bad = 0;
  for (let i = 0; i < 10_000; i++) {
    const t = Math.floor(rng.float() * 400 * YEAR);
    if (cal.at(cal.parts(t)) !== t) bad++;
  }
  const clock = new WorldClock(72);
  clock.anchor(0, 0);
  const day = clock.game(20 * 60 * 1000);
  clock.setScale(0, 1000);
  const paused = clock.game(1000) === clock.game(99999);
  check('calendario: fecha ↔ tiempo exacto en 400 años; reloj: un día en 20 min a ×72, sin saltos al cambiar de escala', bad === 0 && day === DAY && paused && cal.format(0) === 'año 1, día 1, 00:00', { bad, day, fmt: cal.format(3 * YEAR + 45 * DAY + 8 * 3600 + 5 * 60) });
}

// --- determinism ------------------------------------------------------------------------------
interface Raid { t: number; colony: number; amount: number }
const raids = (seed: number): Raid[] => {
  const r = new Rng(hash3(seed, 99, 0));
  return Array.from({ length: 6 }, (_, i) => ({ t: (i + 1) * 70 * DAY + r.int(DAY), colony: r.int(8), amount: r.range(50, 2000) }));
};
const raid = (w: World, f: Raid) => {
  const t = w.table(Colony);
  return t.count ? w.fact('raid', 999, t.ids[f.colony % t.count], f.amount) : 0;
};
const make = (seed: number) => World.create({ seed, modules: [colonies(8)], chunkSize: 64 });

/** Runs `w` to `to`, raiding on the way; in random slices with random budgets if `slicer`. */
function run(w: World, seed: number, to: number, slicer?: Rng): World {
  const stops: Array<Raid | { t: number }> = [...raids(seed).filter((f) => f.t > w.now && f.t <= to), { t: to }];
  for (const f of stops) {
    if (slicer) {
      while (w.now < f.t) {
        const step = Math.min(f.t, w.now + slicer.range(0, 20 * DAY));
        while (!w.advance(step, slicer.chance(0.5) ? slicer.range(0.001, 0.2) : Infinity));
      }
    } else w.advance(f.t);
    if ('colony' in f) raid(w, f);
  }
  return w;
}

{
  const T = 3 * YEAR;
  const straight = run(make(11), 11, T);
  const sliced = run(make(11), 11, T, new Rng(5));
  const bad = [...invariants(straight), ...invariants(sliced)];
  check('determinismo: de una vez = a trozos al azar con presupuestos al azar (mismo resumen)', digest(straight) === digest(sliced) && bad.length === 0 && straight.stats.errors === 0 && straight.store.entities.count > 0 && straight.log.full > 2, {
    digest: digest(straight),
    events: straight.stats.events,
    records: straight.log.count,
    entities: straight.store.entities.count,
    bad: bad.slice(0, 3),
  });

  const digests = new Set<string>();
  let mism = 0, broken = 0;
  for (let seed = 100; seed < 120; seed++) {
    const a = run(make(seed), seed, YEAR);
    const b = run(make(seed), seed, YEAR, new Rng(seed));
    if (digest(a) !== digest(b)) mism++;
    broken += invariants(a).length + a.stats.errors;
    digests.add(digest(a));
  }
  check('20 semillas: cada una idéntica a trozos, invariantes intactos, historias distintas entre semillas', mism === 0 && broken === 0 && digests.size === 20, { mism, broken, distinct: digests.size });
}

// --- save / load ------------------------------------------------------------------------------
{
  const T1 = 400 * DAY, T2 = 600 * DAY, T3 = 2 * YEAR;
  const whole = digest(run(make(21), 21, T3));

  // in memory, straight through the bytes
  const w = run(make(21), 21, T1);
  const snap = snapshot(w, { all: true });
  const back = restore(snap.main, snap.segments.map((s) => s.bytes), [colonies(8)]).world;
  const same = digest(back) === digest(w);
  check('guardar/cargar: el mundo cargado es el guardado; seguir desde él = no haber parado', same && digest(run(back, 21, T3)) === whole, { segments: snap.segments.length, bytes: snap.main.length });

  // on disk: two saves (the second only adds segments), gzip, load the newest
  const dir = join(tmp, 'saves');
  const storage = new FsStorage(dir);
  const saves = new WorldSaves(storage);
  const w2 = run(make(21), 21, T1);
  const s1 = await saves.save(w2);
  run(w2, 21, T2);
  const s2 = await saves.save(w2);
  const files = readdirSync(dir);
  const loaded = await new WorldSaves(new FsStorage(dir)).load([colonies(8)]);
  const cont = loaded ? digest(run(loaded.world, 21, T3)) : '';
  check('en disco: dos guardados (el segundo solo añade segmentos), gzip; cargar el último y seguir = no haber parado', !!loaded && !loaded.fallback && loaded.gen === 2 && cont === whole && s2.segments > s1.segments && files.includes(SAVE_SLOTS[0]) && files.includes(SAVE_SLOTS[1]), {
    first: s1,
    second: s2,
    files: files.length,
    gen: loaded?.gen,
    fallback: loaded?.fallback,
    problems: loaded?.problems,
    same: cont === whole,
  });

  // damaged newest slot / segment → the older save; both damaged → refuse; a new world never overwrites one
  const mem = new MemoryStorage();
  const ms = new WorldSaves(mem);
  const w3 = run(make(21), 21, T1);
  await ms.save(w3);
  const atT1 = digest(w3);
  run(w3, 21, T2);
  const g2 = await ms.save(w3);
  const newest = SAVE_SLOTS[1];
  const orig = mem.files.get(newest)!;
  const hurt = orig.slice();
  hurt[Math.floor(hurt.length / 2)] ^= 0xff;
  mem.files.set(newest, hurt);
  const fb = await new WorldSaves(mem).load([colonies(8)]);
  mem.files.set(newest, orig);
  const lastSeg = `log-${String(g2.segments - 1).padStart(6, '0')}.seg`;
  const seg = mem.files.get(lastSeg)!;
  mem.files.set(lastSeg, seg.slice(0, seg.length - 7));
  const fbSeg = await new WorldSaves(mem).load([colonies(8)]);
  mem.files.set(lastSeg, seg);
  mem.files.set(SAVE_SLOTS[0], hurt);
  mem.files.set(newest, hurt);
  let refused = false;
  try {
    await new WorldSaves(mem).load([colonies(8)]);
  } catch {
    refused = true;
  }
  let guarded = false;
  try {
    await new WorldSaves(mem).save(make(1));
  } catch {
    guarded = true;
  }
  check('partida dañada: se carga la anterior (principal o segmento dañado); las dos dañadas → error, nunca se pisa', !!fb && fb.fallback && digest(fb.world) === atT1 && !!fbSeg && fbSeg.fallback && digest(fbSeg.world) === atT1 && refused && guarded, { problems: fb?.problems });

  // schema changes between versions
  const V1 = defineComponent('thing', { a: 'f64', b: 'u8' });
  const Other = defineComponent('other', { x: 'i32' });
  const m1: SimModule = {
    name: 'v1',
    components: [V1, Other],
    events: [{ name: 'tick1', run: () => undefined }],
    create(w) {
      for (let i = 0; i < 5; i++) {
        const id = w.spawn();
        w.table(V1).add(id, { a: i + 0.5, b: i });
        w.table(Other).add(id, { x: -i });
        w.after(1000, 'tick1', id);
      }
    },
  };
  const V2 = defineComponent('thing', { b: 'f64', c: { type: 'f32', default: 7 } });
  const m2: SimModule = { name: 'v2', components: [V2] };
  const sv = new MemoryStorage();
  await new WorldSaves(sv).save(World.create({ seed: 1, modules: [m1] }));
  const l2 = (await new WorldSaves(sv).load([m2]))!;
  const t2 = l2.world.table(V2);
  const valuesOk = [1, 2, 3, 4, 5].every((id, i) => t2.get(id, 'b') === i && t2.get(id, 'c') === 7);
  l2.world.advance(2000);
  const saves2 = new WorldSaves(sv);
  await saves2.load([m2]);
  await saves2.save(l2.world);
  const l1 = (await new WorldSaves(sv).load([m1]))!;
  const kept = [1, 2, 3, 4, 5].every((id, i) => l1.world.table(Other).get(id, 'x') === -i);
  check('versiones: campos nuevos/quitados/cambiados, tablas y eventos sin módulo se conservan y vuelven', valuesOk && l2.report.orphans.includes('other') && l2.report.unknownKinds.includes('tick1') && l2.world.stats.unhandled === 5 && kept, {
    notes: l2.report.notes,
    orphans: l2.report.orphans,
    unknown: l2.report.unknownKinds,
  });
}

// --- causes -----------------------------------------------------------------------------------
{
  const w = make(31);
  w.advance(40 * DAY);
  const target = w.table(Colony).ids[0];
  const raidId = w.fact('raid', 999, target, 1e9);
  w.advance(41 * DAY);
  const famine = w.log.about(target, 50).find((id) => id > raidId && w.syms.name(w.log.kind(id)) === 'famine');
  const hunger = famine ? w.log.effects(famine).filter((id) => w.syms.name(w.log.kind(id)) === 'death.hunger') : [];
  check('causas: hambruna ← saqueo, muertes de hambre ← hambruna (y hacia delante: efectos)', !!famine && w.log.chain(famine).includes(raidId) && w.log.effects(raidId).includes(famine) && hunger.length > 0 && hunger.every((h) => w.log.cause(h) === famine), {
    chain: famine ? w.log.chain(famine).map((id) => `${id}:${w.syms.name(w.log.kind(id))}`) : null,
    hunger: hunger.length,
  });
}

// --- budget -----------------------------------------------------------------------------------
{
  const w = World.create({ seed: 3, modules: [colonies(600)] });
  const times: number[] = [];
  let progress = true;
  for (let i = 0; i < 40; i++) {
    const before = w.now;
    const t0 = performance.now();
    w.advance(20 * YEAR, 2);
    times.push(performance.now() - t0);
    if (!(w.now > before)) progress = false;
  }
  times.sort((a, b) => a - b);
  const med = times[20];
  check('presupuesto: advance(…, 2 ms) para a tiempo (mediana < 2,5 ms) y siempre avanza', med < 2.5 && progress, { median: med.toFixed(3), max: times[39].toFixed(3), events: w.stats.events, date: w.date() });
}

// --- the runner, in-process ---------------------------------------------------------------------
{
  let real = 1_000_000;
  const storage = new MemoryStorage();
  const start = async () => {
    const sent: SimOut[] = [];
    let deliver: (m: SimIn) => void = () => undefined;
    const r = new SimRunner({ post: (m) => sent.push(m), listen: (fn) => (deliver = fn) }, [colonies(4)], storage, () => real);
    await r.start({ seed: 7, scale: DAY, tickMs: 1e9, autosaveS: 0 });
    const ask = async (m: SimIn) => {
      deliver(m);
      for (let i = 0; i < 50; i++) {
        await sleep(1);
        const a = sent.find((s) => s.type === 'answer' && s.id === (m as { id: number }).id);
        if (a && a.type === 'answer') return a;
      }
      return null;
    };
    return { r, sent, send: (m: SimIn) => deliver(m), ask };
  };
  const a = await start();
  const created = a.sent.find((m) => m.type === 'ready') as Extract<SimOut, { type: 'ready' }>;
  real += 10_000;
  a.r.tick();
  const after10 = a.r.world!.now;
  a.send({ type: 'fact', kind: 'raid', actor: 999, subject: a.r.world!.table(Colony).ids[0], a: 5 });
  const recent = await a.ask({ type: 'query', id: 1, q: { q: 'recent', limit: 3 } });
  const stopped = await a.ask({ type: 'stop', id: 2 });
  const nowAtStop = a.r.world!.now;
  const records = a.r.world!.log.count;
  real += 3_600_000; // an hour off: the world doesn't live it
  const b = await start();
  const resumed = b.r.world!;
  const resumedNow = resumed.now;
  const resumedRecords = resumed.log.count;
  real += 5_000;
  b.r.tick();
  const recentB = (await b.ask({ type: 'query', id: 1, q: { q: 'recent', limit: 1 } }))?.result as RecordView[] | undefined;
  check('runner: crea y guarda, sigue su reloj, hechos y consultas, parar guarda, reanuda donde lo dejó (sin vivir el tiempo apagado)', !!created && Math.abs(after10 - 10 * DAY) < 1 && !!recent?.ok && (recent.result as RecordView[])[0].kind === 'raid' && !!stopped?.ok && resumedNow === nowAtStop && resumedRecords === records + 1 && Math.abs(resumed.now - nowAtStop - 5 * DAY) < 1 && recentB?.[0]?.kind !== undefined, {
    after10: after10 / DAY,
    stoppedAt: resumed.date(nowAtStop),
    resumedAt: resumed.date(resumedNow),
    fiveDaysLater: resumed.date(),
    files: [...storage.files.keys()],
  });
  await b.ask({ type: 'stop', id: 9 });
}

// --- the runner on a Node worker thread -----------------------------------------------------------
{
  const dir = join(tmp, 'thread');
  const modules = new URL('./scenario.ts', import.meta.url).href;
  const config = { seed: 3, scale: 30 * DAY, tickMs: 10, budgetMs: 2, autosaveS: 0.2, statusS: 0.1 };
  const logs: string[] = [];
  const log = (m: string) => logs.push(m);
  try {
    const one = await launchWorld({ dir, config, modules, log });
    const second = await launchWorld({ dir, config, modules, log });
    await sleep(700);
    const s1 = (await one.sim.query({ q: 'status' })) as SimStatus;
    one.sim.fact('raid', 999, 1, 10);
    await sleep(50);
    const recent = await one.sim.query({ q: 'recent', limit: 5 });
    const locked = second.dir === null && second.sim.status?.persistent === false;
    await second.stop();
    const info = await one.stop();
    const files = readdirSync(dir);
    const again = await launchWorld({ dir, config, modules, log });
    const s2 = again.sim.status!;
    await again.stop();
    check('hilo de Node: corre a su reloj con presupuesto, hechos, consultas, autoguardado, un proceso por carpeta, parar y reanudar', s1.now > 5 * DAY && s1.saves >= 2 && recent.some((r) => r.kind === 'raid') && locked && !!info && !files.includes('lock') && s2.now >= s1.now && s2.records > s1.records && s1.errors === 0, {
      firstStatus: { date: s1.date, events: s1.events, records: s1.records, tickP95: s1.tickP95.toFixed(2), saves: s1.saves, lag: s1.lag.toFixed(0), errors: s1.errors },
      flags: { raid: recent.map((r) => r.kind), locked, info: !!info, noLock: !files.includes('lock'), later: s2.now >= s1.now, more: s2.records > s1.records },
      resumed: s2.date,
      files,
      logs,
    });
  } catch (e) {
    check('hilo de Node', false, { error: String(e), logs });
  }
}

rmSync(tmp, { recursive: true, force: true });
console.log(failures ? `\n${failures} fallo(s)` : '\nNúcleo del mundo en orden.');
process.exit(failures ? 1 : 0);
