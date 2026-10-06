// Replication by interest (npm run test:net): what each player knows, measured.
//
//   grid          the cell grid finds everything within a radius (against brute force)
//   interest      random walkers and things (in hosts, pinned, coming and going): what each one
//                 knows after every update = the rule applied by hand (hysteresis included), and
//                 its spawn/gone lists are exactly the changes
//   real server   two headless clients: the objects near the base are known at once; a player
//                 who walks away forgets them and learns them again on its way back; an object
//                 made and removed at runtime reaches only who is near; an object's news go only
//                 to its knowers
//
// Exit code 1 if anything fails. `--verbose` for the numbers. PORT (default 3109).

import { CellGrid, Replication, type Watcher } from '../../src/shared/net/interest.js';
import type { EntityWire, ServerMessage, Vec3 } from '../../src/shared/protocol.js';
import { Rng } from '../../src/sim/index.js';
import { connect, sleep, startServer } from './harness.js';

const verbose = process.argv.includes('--verbose');
const PORT = Number(process.env.PORT) || 3109;
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}
const d2 = (a: readonly number[], b: readonly number[]) => (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2;

// --- grid ---------------------------------------------------------------------------------------
{
  const r = new Rng(1);
  const g = new CellGrid(500);
  const pts = new Map<number, Vec3>();
  for (let i = 0; i < 5000; i++) {
    const p: Vec3 = [r.range(-2e4, 2e4), r.range(-2e3, 2e3), r.range(-2e4, 2e4)];
    pts.set(i, p);
    g.set(i, p);
  }
  for (let i = 0; i < 2000; i++) {
    const id = r.int(5000);
    if (r.chance(0.3)) {
      g.delete(id);
      pts.delete(id);
    } else {
      const p: Vec3 = [r.range(-2e4, 2e4), r.range(-2e3, 2e3), r.range(-2e4, 2e4)];
      pts.set(id, p);
      g.set(id, p);
    }
  }
  let missed = 0, extra = 0;
  for (let q = 0; q < 300; q++) {
    const c: Vec3 = [r.range(-2e4, 2e4), r.range(-2e3, 2e3), r.range(-2e4, 2e4)];
    const rad = r.range(100, 3000);
    const found = new Set(g.near(c, rad));
    for (const [id, p] of pts) if (d2(p, c) < rad * rad && !found.has(id)) missed++;
    for (const id of found) if (!pts.has(id)) extra++;
  }
  check('rejilla: encuentra todo lo que hay dentro del radio (contra fuerza bruta), nada que ya no esté', missed === 0 && extra === 0, { missed, extra });
}

// --- interest -----------------------------------------------------------------------------------
{
  type Thing = { id: number; p: Vec3; fr: number; owner: number };
  type W = Watcher & { id: number; at: Vec3 | null; fr: number };
  const r = new Rng(2);
  const hosts = new Map<number, Vec3>([[7, [0, 0, 0]], [8, [5000, 0, 0]]]);
  const where = (t: Thing, out: Vec3): Vec3 => {
    const h = t.fr ? hosts.get(t.fr)! : [0, 0, 0];
    out[0] = h[0] + t.p[0];
    out[1] = h[1] + t.p[1];
    out[2] = h[2] + t.p[2];
    return out;
  };
  const rule = { enter: 1500, leave: 1800 };
  const rep = new Replication<Thing, W>({ where, frame: (t) => t.fr, pinned: (w, t) => t.owner === w.id }, rule);
  const things = new Map<number, Thing>();
  const watchers: W[] = [1, 2, 3].map((id) => ({ id, at: [r.range(-3000, 3000), 0, r.range(-3000, 3000)], fr: 0 }));
  const ref = new Map<W, Set<number>>(watchers.map((w) => [w, new Set()]));
  const tmp: Vec3 = [0, 0, 0];
  const sees = (w: W, t: Thing, known: boolean) => (t.fr !== 0 && t.fr === w.fr) || t.owner === w.id || (!!w.at && d2(where(t, tmp), w.at) < (known ? rule.leave : rule.enter) ** 2);
  let next = 1, wrong = 0, wrongDiff = 0, wrongAdd = 0, wrongRemove = 0, changes = 0;
  for (let step = 0; step < 400; step++) {
    // things come and go
    for (let k = 0; k < 5; k++) {
      const t: Thing = { id: next++, p: [r.range(-6000, 6000), 0, r.range(-6000, 6000)], fr: r.chance(0.2) ? (r.chance(0.5) ? 7 : 8) : 0, owner: r.chance(0.05) ? 1 + r.int(3) : 0 };
      if (t.fr) t.p = [r.range(-20, 20), 0, r.range(-20, 20)];
      things.set(t.id, t);
      const told = new Set(rep.add(t, watchers));
      for (const w of watchers) {
        const should = sees(w, t, false);
        if (should !== told.has(w)) wrongAdd++;
        if (should) ref.get(w)!.add(t.id);
      }
    }
    if (things.size > 200) {
      const ids = [...things.keys()];
      for (let k = 0; k < 4; k++) {
        const id = ids[r.int(ids.length)];
        if (!things.delete(id)) continue;
        const told = new Set(rep.remove(id));
        for (const w of watchers) {
          if (ref.get(w)!.has(id) !== told.has(w)) wrongRemove++;
          ref.get(w)!.delete(id);
        }
      }
    }
    // everything moves: walkers, things, the hosts (their contents with them), who is aboard what
    for (const w of watchers) {
      w.at = [w.at![0] + r.range(-400, 400), 0, w.at![2] + r.range(-400, 400)];
      if (r.chance(0.05)) w.fr = w.fr ? 0 : r.chance(0.5) ? 7 : 8;
    }
    for (const t of things.values()) if (!t.fr) t.p = [t.p[0] + r.range(-50, 50), 0, t.p[2] + r.range(-50, 50)];
    for (const [id, h] of hosts) hosts.set(id, [h[0] + r.range(-300, 300), 0, h[2] + r.range(-300, 300)]);
    const ch = rep.update(watchers);
    for (const w of watchers) {
      const known = ref.get(w)!;
      const now = new Set<number>();
      for (const t of things.values()) if (sees(w, t, known.has(t.id))) now.add(t.id);
      const enter = [...now].filter((id) => !known.has(id)).sort((a, b) => a - b);
      const leave = [...known].filter((id) => !now.has(id)).sort((a, b) => a - b);
      const c = ch.get(w);
      const gotEnter = (c?.enter ?? []).map((t) => t.id).sort((a, b) => a - b);
      const gotLeave = [...(c?.leave ?? [])].sort((a, b) => a - b);
      if (gotEnter.join() !== enter.join() || gotLeave.join() !== leave.join()) wrongDiff++;
      for (const t of things.values()) if (rep.knows(w, t.id) !== now.has(t.id)) wrong++;
      changes += enter.length + leave.length;
      ref.set(w, now);
    }
  }
  check('interés: lo que sabe cada uno tras cada actualización = la regla a mano (histéresis, anfitriones, fijados); altas y bajas avisan a quien toca', wrong === 0 && wrongDiff === 0 && wrongAdd === 0 && wrongRemove === 0 && changes > 100, { wrong, wrongDiff, wrongAdd, wrongRemove, changes, things: things.size });
}

// --- the real server -----------------------------------------------------------------------------
{
  let server: Awaited<ReturnType<typeof startServer>> | null = null;
  try {
    server = await startServer(PORT);
    const a = await connect(PORT, 'cerca');
    const b = await connect(PORT, 'lejos');
    const spawn = a.welcome.spawn;
    const initial = a.welcome.crates.length;
    const here = (c: typeof a, p: Vec3) => c.send({ type: 'state', t: performance.now(), s: { p, v: [0, 0, 0], yaw: 0, pitch: 0, f: 0 } });
    const away: Vec3 = [spawn[0] + 6000, spawn[1], spawn[2]];
    here(a, spawn);
    here(b, away);
    const isGone = (m: ServerMessage): m is Extract<ServerMessage, { type: 'gone' }> => m.type === 'gone' && m.k === 'obj';
    const isSpawn = (m: ServerMessage): m is Extract<ServerMessage, { type: 'spawn' }> => m.type === 'spawn';
    // what B knows: its welcome, plus what came, minus what went
    const knownBy = (c: typeof b) => {
      const k = new Set(c.welcome.crates.map((x) => x.id));
      for (const m of c.inbox) {
        if (m.type === 'spawn') for (const e of m.e) k.add(e.id);
        if (m.type === 'gone' && m.k === 'obj') for (const id of m.ids) k.delete(id);
      }
      return k;
    };
    const forgot = await b.until(isGone, 3000);
    await sleep(1500);
    check('al unirse sabe los objetos de alrededor; quien se aleja los olvida todos', initial > 0 && b.welcome.crates.length > 0 && !!forgot && knownBy(b).size === 0, { initial, welcomeB: b.welcome.crates.length, forgot: forgot?.ids.length, stillKnown: knownBy(b).size });

    // made at runtime near A: A learns it at once, B (far) never hears of it
    const fromA = a.inbox.length, fromB = b.inbox.length;
    a.send({ type: 'dev', cmd: 'obj.spawn', a: { p: [spawn[0] + 3, spawn[1] + 0.5, spawn[2]], kind: 'crate', paint: 'grey' } });
    const made = await a.until(isSpawn, 2000, fromA);
    const id = (made?.e[0] as EntityWire | undefined)?.id ?? -1;
    await sleep(1200);
    const bHeard = b.inbox.slice(fromB).some((m) => (m.type === 'spawn' && m.e.some((e) => e.id === id)) || (m.type === 'crate' && m.c.id === id));
    // A moves it: only who knows it hears (B doesn't)
    a.send({ type: 'crateTake', id });
    await sleep(100);
    a.send({ type: 'crate', c: { id, fr: 0, p: [spawn[0] + 4, spawn[1] + 0.5, spawn[2]], q: [0, 0, 0, 1], v: [1, 0, 0], w: [0, 0, 0], t: performance.now() } });
    await sleep(300);
    const bNews = b.inbox.slice(fromB).some((m) => m.type === 'crate' && m.c.id === id);
    // B comes back: learns everything, the new one too
    const back = b.inbox.length;
    here(b, [spawn[0] + 20, spawn[1], spawn[2]]);
    const relearnt = await b.until((m): m is Extract<ServerMessage, { type: 'spawn' }> => m.type === 'spawn' && m.e.length >= initial, 3000, back);
    await sleep(1500);
    const knowsAllOfA = [...knownBy(a)].every((x) => knownBy(b).has(x));
    // removed: both forget it
    const fa = a.inbox.length, fb = b.inbox.length;
    a.send({ type: 'dev', cmd: 'obj.despawn', a: { id } });
    const goneA = await a.until((m) => isGone(m) && m.ids.includes(id), 2000, fa);
    const goneB = await b.until((m) => isGone(m) && m.ids.includes(id), 2000, fb);
    check('objeto hecho en marcha: lo sabe quien está cerca al momento, el lejano ni se entera (ni de sus movimientos)', id > 0 && !bHeard && !bNews, { id, bHeard, bNews });
    check('quien vuelve lo aprende todo (también lo nuevo); al quitarlo lo olvidan todos los que lo sabían', !!relearnt && relearnt.e.some((e) => e.id === id) && knowsAllOfA && !!goneA && !!goneB, { relearnt: relearnt?.e.length, knowsAllOfA });
    a.close();
    b.close();
  } catch (e) {
    failures++;
    console.error('FAIL servidor', e instanceof Error ? e.message : e, server?.log().slice(-1500));
  } finally {
    await server?.stop();
  }
}

console.log(failures ? `\n${failures} fallo(s)` : '\nRéplica en orden.');
process.exit(failures ? 1 : 0);
