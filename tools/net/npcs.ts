// The world's people on the real server (npm run test:npcs): a headless client at the base.
//
//   bodies     the crew comes out (6 people with names) when a player is near; their states come
//              stamped with the server's steps (every 50 ms), feet on the ground; they walk to
//              what their minds decide
//   facts      a player fires next to them: who heard it remembers it (a memory, a grudge), becomes
//              someone of its own, reacts (says so over the radio, goes over), and the log chains
//              reacted ← witnessed ← shot
//   here       "what is here" answers with the people and their crew
//   leaving    far away, their bodies go (the crew back to a number; the witness stays, without one)
//
// Exit code 1 if anything fails. `--verbose` for the numbers. PORT (default 3111).

import { bodyGround } from '../../src/shared/actors/ground.js';
import type { PlayerState, ServerMessage, Vec3 } from '../../src/shared/protocol.js';
import { surfaceOf, type CelestialBody } from '../../src/shared/space/body.js';
import { siteCrews } from '../../src/shared/ship/spawn.js';
import { connect, sleep, startServer, type TestClient } from './harness.js';

const verbose = process.argv.includes('--verbose');
const PORT = Number(process.env.PORT) || 3111;
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}
const here = (c: TestClient, p: Vec3) => c.send({ type: 'state', t: performance.now(), s: { p, v: [0, 0, 0], yaw: 0, pitch: 0, f: 1 } });
async function ask<T>(c: TestClient, name: string, payload?: unknown): Promise<T> {
  const from = c.inbox.length;
  c.send({ type: 'dev', cmd: 'world.ask', a: { name, payload } });
  const r = await c.until((m) => m.type === 'devReply' && m.cmd === 'world.ask', 5000, from);
  if (!r || r.type !== 'devReply') throw new Error(`sin respuesta a ${name}`);
  return r.data as T;
}

const server = await startServer(PORT);
try {
  for (let i = 0; i < 100 && !/dotación/.test(server.log()); i++) await sleep(100);
  const a = await connect(PORT, 'visitante');
  const surfaces = (b: CelestialBody) => surfaceOf(b, a.welcome.worldSeed);
  const ground = bodyGround(surfaces);
  const crew = siteCrews(surfaces)[0];
  // stand where the crew gathers
  const at: Vec3 = [crew.center[0], crew.center[1] + 0.05, crew.center[2]];
  const keep = setInterval(() => here(a, at), 100);
  here(a, at);
  await sleep(3000);
  const npcs = new Map<number, string>();
  for (const m of a.inbox) if (m.type === 'spawn') for (const e of m.e) if (e.k === 'npc') npcs.set(e.id, e.name);
  // their states for a while
  const from = a.inbox.length;
  await sleep(6000);
  const states = a.inbox.slice(from).filter((m): m is Extract<ServerMessage, { type: 'npcs' }> => m.type === 'npcs');
  const track = new Map<number, Array<{ t: number; s: PlayerState }>>();
  for (const m of states) for (const s of m.states) (track.get(s.id) ?? track.set(s.id, []).get(s.id)!).push({ t: s.t, s: s.s });
  let worstH = 0, moved = 0;
  const gaps: number[] = [];
  for (const list of track.values()) {
    for (let i = 0; i < list.length; i++) {
      worstH = Math.max(worstH, Math.abs(ground.height(list[i].s.p)));
      if (i) gaps.push(list[i].t - list[i - 1].t);
    }
    const f = list[0].s.p, l = list[list.length - 1].s.p;
    if (Math.hypot(l[0] - f[0], l[1] - f[1], l[2] - f[2]) > 2) moved++;
  }
  gaps.sort((x, y) => x - y);
  const median = gaps[Math.floor(gaps.length / 2)] ?? NaN;
  check('cerca de la base sale la dotación: 6 personas con nombre, estados cada 50 ms de paso, pies en el suelo, andando a lo que deciden', npcs.size === 6 && [...npcs.values()].every((n) => / /.test(n)) && Math.abs(median - 50) < 1 && worstH < 0.05 && moved >= 2, { names: [...npcs.values()], median, worstH, moved, states: states.length });

  // two shots right next to them: whoever heard it remembers, holds it, and reacts
  const said = a.inbox.length;
  a.send({ type: 'fire', w: 'rifle', o: [at[0], at[1] + 1.5, at[2]], d: [0, 1, 0] });
  await sleep(400);
  a.send({ type: 'fire', w: 'rifle', o: [at[0], at[1] + 1.5, at[2]], d: [0, 1, 0] });
  const voice = await a.until((m) => m.type === 'say' && /disparando/.test(m.text), 6000, said);
  // the witnesses: people who remember a shot
  const knows: Array<{ id: number; memories: Array<{ kind: string; record: number }>; grudges: Array<{ value: number }> }> = [];
  for (const id of npcs.keys()) {
    const k = await ask<{ memories: Array<{ kind: string; record: number }>; grudges: Array<{ value: number }> }>(a, 'knowledge.of', { id });
    if (k.memories.length) knows.push({ id, ...k });
  }
  const crewState = (await ask<Array<{ out: number }>>(a, 'people.state'))[0];
  check('un disparo junto a ellos: quien lo oye lo recuerda y se la guarda, pasa a tener ficha propia, y reacciona (lo dice por radio)', knows.length > 0 && knows.every((k) => k.memories.some((m) => m.kind === 'shot') && k.grudges[0]?.value > 0.3) && crewState.out >= knows.length && !!voice, {
    witnesses: knows.length,
    voice: voice && voice.type === 'say' ? voice.text : null,
    out: crewState.out,
  });
  // the chain of causes of a reaction, through the historian's query
  a.send({ type: 'dev', cmd: 'world.query', a: { q: { q: 'recent', limit: 60 } } });
  const log = await a.until((m) => m.type === 'devReply' && m.cmd === 'world.query', 5000);
  const records = (log && log.type === 'devReply' ? (log.data as Array<{ id: number; kind: string; cause: number }>) : []) ?? [];
  const byId = new Map(records.map((r) => [r.id, r]));
  const reacted = records.find((r) => r.kind === 'reacted');
  const chain: string[] = [];
  for (let r = reacted; r; r = byId.get(r.cause)) chain.push(r.kind);
  check('el registro encadena reaccionó ← presenció ← disparo', chain.join(' ← ') === 'reacted ← witnessed ← shot', { chain });

  // what is here
  const items = await ask<Array<{ what: string; name?: string }>>(a, 'world.here', { p: crew.center, r: 40 });
  check('«qué hay aquí» responde con las personas (y su dotación)', items.filter((i) => i.what === 'persona').length >= 5 && items.some((i) => i.what === 'dotación'), { items: items.map((i) => `${i.what}${i.name ? ` ${i.name}` : ''}`) });

  // far away: the bodies go
  clearInterval(keep);
  const gone = a.inbox.length;
  here(a, [at[0] + 3000, at[1], at[2]]);
  await sleep(3000);
  const forgot = new Set(a.inbox.slice(gone).flatMap((m) => (m.type === 'gone' && m.k === 'npc' ? m.ids : [])));
  const after = (await ask<Array<{ live: number; out: number }>>(a, 'people.state'))[0];
  check('lejos, se van sus cuerpos: la dotación vuelve a ser un número, el testigo sigue (sin cuerpo)', [...npcs.keys()].every((id) => forgot.has(id)) && after.live === 0 && after.out >= 1, { forgot: forgot.size, after });
  a.close();
} catch (e) {
  failures++;
  console.error('FAIL', e instanceof Error ? e.message : e, server.log().slice(-1500));
} finally {
  await server.stop();
}
console.log(failures ? `\n${failures} fallo(s)` : '\nGente del mundo en orden.');
process.exit(failures ? 1 : 0);
