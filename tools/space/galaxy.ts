// The galaxy and the jump between systems (npm run test:galaxy), headless.
//
//   galaxy    2000 systems, the same every time; Sol first; stars as common as round the Sun; one
//             name per system; a flattened disc
//   regions   each physical system is a slice of the world frame with its own body: there, that
//             body rules (gravity, "the body" for everything), the Moon's pull is nil, its ground is
//             its own, and a walker stays on it to the centimetre (float64 at 1e11 m)
//   rules     no jump landed, low, or to where you are; one otherwise
//   server    a pilot jumps: everyone is told where the ship is; late reports from the old system are
//             dropped; the server flies on from there when the pilot leaves
//
// Exit code 1 if anything fails. `--verbose` for the numbers. PORT (default 3112).

import { bodyGround } from '../../src/shared/actors/ground.js';
import { GAIT, walk, type V3, type WalkState } from '../../src/shared/actors/walker.js';
import type { ServerMessage } from '../../src/shared/protocol.js';
import { BODIES, bodyAt, gravityAt, MOON_BODY, surfaceOf, type CelestialBody } from '../../src/shared/space/body.js';
import { galaxy, GALAXY_SIZE, physicalRegions, regionAt, STAR_CLASSES, SYSTEM_SPACING } from '../../src/shared/space/galaxy.js';
import { arrivalPoint, JUMP, jumpRefusal } from '../../src/shared/space/jump.js';
import { connect, sleep, startServer } from '../net/harness.js';

const verbose = process.argv.includes('--verbose');
const PORT = Number(process.env.PORT) || 3112;
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}

// --- galaxy -----------------------------------------------------------------------------------------
{
  const g = galaxy();
  const shares: Record<string, number> = {};
  for (const s of g) shares[s.star.cls] = (shares[s.star.cls] ?? 0) + 1 / g.length;
  const m = shares['M'] ?? 0;
  const names = new Set(g.map((s) => s.name)).size;
  const inDisc = g.every((s) => Math.hypot(s.ly[0], s.ly[2]) <= 60 && Math.abs(s.ly[1]) <= 8);
  const again = galaxy();
  check('galaxia: 2000 sistemas, Sol primero, clases como alrededor del Sol, un nombre por sistema, en un disco', g.length === GALAXY_SIZE && g[0].name === 'Sol' && g[0].ly.every((v) => v === 0) && Math.abs(m - STAR_CLASSES[0].share) < 0.04 && names === g.length && inDisc && again === g, {
    shares: Object.fromEntries(Object.entries(shares).map(([k, v]) => [k, +v.toFixed(3)])),
    names,
  });
}

// --- regions ----------------------------------------------------------------------------------------
{
  const regions = physicalRegions();
  const bad: string[] = [];
  const surfaces = (b: CelestialBody) => surfaceOf(b, 1969);
  const ground = bodyGround(surfaces);
  let worstWalk = 0;
  const heights = new Set<string>();
  for (const r of regions) {
    const a = arrivalPoint(r);
    const body = r.body ? BODIES[r.body.def.id] : MOON_BODY;
    if (!body) bad.push(`${r.system.name}: sin cuerpo`);
    if (bodyAt(a) !== body) bad.push(`${r.system.name}: rige otro cuerpo`);
    if (regionAt(a).index !== r.index) bad.push(`${r.system.name}: región equivocada`);
    const gv: V3 = [0, 0, 0];
    gravityAt(body, a, gv);
    const gm = Math.hypot(...gv);
    // three radii from the centre: a ninth of the surface gravity
    if (Math.abs(gm - body.def.gravity / 9) > 1e-6) bad.push(`${r.system.name}: gravedad ${gm}`);
    // the Moon's pull there
    const mx = a[0] - MOON_BODY.center[0], my = a[1] - MOON_BODY.center[1], mz = a[2] - MOON_BODY.center[2];
    if (r.index > 0 && MOON_BODY.mu / (mx * mx + my * my + mz * mz) > 1e-9) bad.push(`${r.system.name}: la Luna aún tira`);
    // a walk on its ground, top of the body
    const top: V3 = [body.center[0], body.center[1] + body.radius, body.center[2]];
    const h = ground.height(top);
    const start: V3 = [top[0], top[1] - h, top[2]];
    heights.add(h.toFixed(3));
    const w: WalkState = { p: [...start], v: [0, 0, 0], yaw: 0 };
    const to: V3 = [start[0] + 30, start[1], start[2] - 20];
    for (let t = 0; t < 60; t += 0.05) {
      walk(w, to, GAIT.walk, 0.05, ground);
      worstWalk = Math.max(worstWalk, Math.abs(ground.height(w.p)));
    }
  }
  check(`regiones: ${regions.length} sistemas físicos, cada uno con su cuerpo rigiendo en su región (gravedad, «el cuerpo»), la Luna sin tirar, su propio suelo y un andador pegado a él`, bad.length === 0 && worstWalk < 0.03 && heights.size === regions.length && regions.length > 1, { bad, worstWalk, spacing: SYSTEM_SPACING });
}

// --- rules ------------------------------------------------------------------------------------------
{
  const r1 = physicalRegions()[1];
  const high: V3 = [0, 50_000, 0];
  const rules = [jumpRefusal(high, true, 1), jumpRefusal([0, 100, 0], false, 1), jumpRefusal(high, false, 0), jumpRefusal(high, false, 99), jumpRefusal(high, false, 1)];
  check('reglas del salto: en tierra no, cerca del suelo no, al mismo sistema no, a uno que no existe no; en vuelo alto, sí', rules.slice(0, 4).every((x) => typeof x === 'string') && rules[4] === null && !!r1, { rules });
}

// --- the real server --------------------------------------------------------------------------------
{
  const server = await startServer(PORT);
  try {
    const pilot = await connect(PORT, 'piloto');
    const crew = await connect(PORT, 'tripulante');
    const ship = pilot.welcome.ships[0];
    const id = ship.id;
    const start = ship.pose?.p ?? ship.place.p;
    const high: V3 = [start[0], start[1] + 30_000, start[2]];
    // the watcher aboard: it gets every pose
    const aboard = setInterval(() => crew.send({ type: 'state', t: performance.now(), s: { p: [0, 1, 0], v: [0, 0, 0], yaw: 0, pitch: 0, f: 0, fr: id } }), 50);
    pilot.send({ type: 'pilot', ship: id, on: true });
    await sleep(200);
    // landed still: refused
    pilot.send({ type: 'jump', ship: id, to: 1 });
    const refused = await pilot.until((m) => m.type === 'say' && /Salto/.test(m.text), 2000);
    // flying high: reports every 50 ms
    let flyingAt: V3 = high;
    const report = setInterval(() => pilot.send({ type: 'flight', ship: id, t: performance.now(), agl: 30_000, out: [], p: flyingAt, q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0], landed: false, pad: false }), 50);
    await sleep(600);
    const fromCrew = crew.inbox.length;
    pilot.send({ type: 'jump', ship: id, to: 1 });
    const jumped = await crew.until((m): m is Extract<ServerMessage, { type: 'jump' }> => m.type === 'jump', 3000, fromCrew);
    // the pilot's client hasn't heard yet: a few late reports from Sol
    await sleep(400);
    // it has now: it flies from the arrival point
    const arrival = jumped ? jumped.p : ([0, 0, 0] as V3);
    flyingAt = arrival;
    await sleep(600);
    clearInterval(report);
    const after = crew.inbox.slice(fromCrew).filter((m): m is Extract<ServerMessage, { type: 'shipPose' }> => m.type === 'shipPose' && m.ship === id);
    const backToSol = after.filter((m) => regionAt(m.p).index === 0).length;
    // the pilot leaves: the server flies on from there
    pilot.send({ type: 'pilot', ship: id, on: false });
    await sleep(1500);
    const served = crew.inbox.slice(fromCrew).filter((m): m is Extract<ServerMessage, { type: 'shipPose' }> => m.type === 'shipPose' && m.ship === id);
    const last = served[served.length - 1];
    const drift = last ? Math.hypot(last.p[0] - arrival[0], last.p[1] - arrival[1], last.p[2] - arrival[2]) : Infinity;
    clearInterval(aboard);
    check('servidor: en tierra lo niega; en vuelo salta y todos lo saben; los informes tardíos del Sol no lo devuelven; el servidor sigue volándola allí', !!refused && !!jumped && jumped.to === 1 && regionAt(jumped.p).index === 1 && backToSol === 0 && after.length > 5 && !!last && regionAt(last.p).index === 1 && drift < 200 && (JUMP.cooldownS > 0), {
      refused: refused && refused.type === 'say' ? refused.text : null,
      arrival: jumped?.p,
      poses: after.length,
      backToSol,
      drift: drift.toFixed(2),
      log: server.log().split('\n').filter((l) => /salta/.test(l)),
    });
    pilot.close();
    crew.close();
  } catch (e) {
    failures++;
    console.error('FAIL', e instanceof Error ? e.message : e, server.log().slice(-1500));
  } finally {
    await server.stop();
  }
}

console.log(failures ? `\n${failures} fallo(s)` : '\nGalaxia en orden.');
process.exit(failures ? 1 : 0);
