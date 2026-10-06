// The world keeps the loose objects (npm run test:world): the real server with its world in a
// temporary folder, a headless client, a restart.
//
//   - the room's objects are the world's (the same ids after a restart);
//   - near the base's depot, its crates come out (9 slots of 40); going away, they go back in;
//   - a crate taken from the depot is its own for good: it stays where it was left, the depot has
//     one fewer — and after a save and a restart it is still there;
//   - nothing of this waits in a game step (the world answers on its own thread).
//
// Exit code 1 if anything fails. `--verbose` for the numbers. PORT (default 3110).

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { ServerMessage, Vec3 } from '../../src/shared/protocol.js';
import type { ThingAt } from '../../src/sim/modules/objects.js';
import { connect, sleep, startServer, type TestClient } from './harness.js';

const verbose = process.argv.includes('--verbose');
const PORT = Number(process.env.PORT) || 3110;
const dir = mkdtempSync(join(tmpdir(), 'world-objects-'));
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}

const here = (c: TestClient, p: Vec3) => c.send({ type: 'state', t: performance.now(), s: { p, v: [0, 0, 0], yaw: 0, pitch: 0, f: 0 } });
async function ask<T>(c: TestClient, name: string, payload?: unknown): Promise<T> {
  const from = c.inbox.length;
  c.send({ type: 'dev', cmd: 'world.ask', a: { name, payload } });
  const r = await c.until((m) => m.type === 'devReply' && m.cmd === 'world.ask', 5000, from);
  if (!r || r.type !== 'devReply') throw new Error(`sin respuesta a ${name}`);
  return r.data as T;
}
type Depot = { key: string; amount: number; live: number; out: number };
const depotOf = (c: TestClient) => ask<Depot[]>(c, 'depots.state').then((l) => l[0]);
const spawned = (c: TestClient, from = 0) => c.inbox.slice(from).flatMap((m: ServerMessage) => (m.type === 'spawn' ? m.e.map((e) => e.id) : []));

try {
  // --- first run --------------------------------------------------------------------------------
  let server = await startServer(PORT, { WORLD_DIR: dir });
  for (let i = 0; i < 100 && !/objetos sueltos/.test(server.log()); i++) await sleep(100);
  const a = await connect(PORT, 'a');
  const spawn = a.welcome.spawn;
  here(a, spawn);
  const startIds = a.welcome.crates.map((c) => c.id).sort((x, y) => x - y);
  // the depot comes out within a second or two
  let depot = await depotOf(a);
  for (let i = 0; i < 30 && depot.live === 0; i++) {
    await sleep(200);
    depot = await depotOf(a);
  }
  await sleep(700);
  const out = (await ask<ThingAt[]>(a, 'objects.list')).filter((t) => t.spec.paint === 'grey' && t.spec.half[0] === 0.35);
  const known = new Set([...startIds, ...spawned(a)]);
  check('cerca del almacén salen sus cajas (9 de 40) y el cliente las recibe', depot.live === 9 && depot.amount === 31 && out.length === 9 && out.every((t) => known.has(t.id)), { depot, out: out.length });

  // take one and leave it somewhere else
  const taken = out[0].id;
  a.send({ type: 'crateTake', id: taken });
  await sleep(200);
  const dropAt: Vec3 = [spawn[0] + 3, spawn[1] + 0.3, spawn[2] + 2];
  a.send({ type: 'crate', c: { id: taken, fr: 0, p: dropAt, q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0], t: performance.now() }, rest: true });
  await sleep(1500);
  // walk away: the depot takes its crates back, the taken one stays
  here(a, [spawn[0] + 2500, spawn[1], spawn[2]]);
  await sleep(2500);
  depot = await depotOf(a);
  const list = await ask<ThingAt[]>(a, 'objects.list');
  const kept = list.find((t) => t.id === taken);
  check('al irse vuelven al almacén (39 dentro, 1 fuera para siempre); la cogida sigue donde la dejaron', depot.live === 0 && depot.amount === 39 && depot.out === 1 && !!kept && Math.abs(kept.p[0] - dropAt[0]) < 1e-6 && list.filter((t) => t.spec.half[0] === 0.35 && t.spec.paint === 'grey').length === 1, { depot, kept: kept?.p });
  const saved = await ask<unknown>(a, 'objects.state');
  a.send({ type: 'dev', cmd: 'world.save' });
  const ok = await a.until((m) => m.type === 'devReply' && m.cmd === 'world.save', 5000);
  a.close();
  await server.stop();

  // --- after a restart -------------------------------------------------------------------------
  server = await startServer(PORT, { WORLD_DIR: dir });
  for (let i = 0; i < 100 && !/objetos sueltos/.test(server.log()); i++) await sleep(100);
  const b = await connect(PORT, 'b');
  here(b, spawn);
  await sleep(2500);
  const ids = new Set([...b.welcome.crates.map((c) => c.id), ...spawned(b)]);
  const again = await depotOf(b);
  const list2 = await ask<ThingAt[]>(b, 'objects.list');
  const stillThere = list2.find((t) => t.id === taken);
  check('tras guardar y reiniciar: mismos ids, la caja cogida donde estaba, el almacén con 39 (y vuelve a sacar 9)', !!ok && startIds.every((id) => ids.has(id)) && ids.has(taken) && !!stillThere && Math.abs(stillThere.p[2] - dropAt[2]) < 1e-6 && again.amount + again.live === 39 && again.live === 9, { saved, again, ids: ids.size });
  b.close();
  await server.stop();
  if (verbose) console.log(server.log().trim().split('\n').slice(-6).join('\n'));
} catch (e) {
  failures++;
  console.error('FAIL', e instanceof Error ? e.message : e);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(failures ? `\n${failures} fallo(s)` : '\nObjetos del mundo en orden.');
process.exit(failures ? 1 : 0);
