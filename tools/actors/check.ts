// People with a body, headless (npm run test:actors).
//
//   walker       on the Moon's real ground (craters and all): it gets there, feet on the ground at
//                every step, facing where it goes, never faster than it walks — at the base and
//                far from it; round an obstacle in the way without ever entering it
//   perception   sight (range, visor cone, the hull between) and hearing by what carries sound
//                (the same air, a structure without air, the ground; vacuum carries nothing)
//   minds        a crew comes out as people with names (the same ones every visit); over a game
//                day they work by day and rest by night, and their orders point at their posts
//
// Exit code 1 if anything fails. `--verbose` for the numbers.

import { bodyGround } from '../../src/shared/actors/ground.js';
import { hears, sees, SENSES, witnesses, type Eye, type Sense } from '../../src/shared/actors/perception.js';
import { GAIT, walk, type Obstacle, type V3, type WalkState } from '../../src/shared/actors/walker.js';
import { WORLD_SEED } from '../../src/shared/constants.js';
import { MOON_BODY, surfaceOf, tangentFrame, type CelestialBody } from '../../src/shared/space/body.js';
import { SITES, type BaseSiteDef } from '../../src/shared/space/sites.js';
import { siteCrews } from '../../src/shared/ship/spawn.js';
import { spawnPoint } from '../../src/shared/space/world.js';
import { World } from '../../src/sim/index.js';
import { ACTIVITIES, Person, people, type MadePerson, type NpcOrder } from '../../src/sim/modules/people.js';

const verbose = process.argv.includes('--verbose');
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}
const surfaces = (b: CelestialBody) => surfaceOf(b, WORLD_SEED);
const ground = bodyGround(surfaces);

/** A point `d` m east and `s` m south of `p` on the ground. */
function offset(p: V3, de: number, ds: number): V3 {
  const e: V3 = [0, 0, 0], u: V3 = [0, 0, 0], s: V3 = [0, 0, 0];
  ground.axes(p, e, u, s);
  const q: V3 = [p[0] + e[0] * de + s[0] * ds, p[1] + e[1] * de + s[1] * ds, p[2] + e[2] * de + s[2] * ds];
  const h = ground.height(q);
  ground.axes(q, e, u, s);
  return [q[0] - u[0] * h, q[1] - u[1] * h, q[2] - u[2] * h];
}

function stroll(from: V3, to: V3, obstacles: Obstacle[] = []) {
  const w: WalkState = { p: [...from], v: [0, 0, 0], yaw: 0 };
  const dt = 0.05;
  let t = 0, worstH = 0, worstFace = 0, fastest = 0, inside = Infinity;
  const e: V3 = [0, 0, 0], u: V3 = [0, 0, 0], s: V3 = [0, 0, 0];
  for (; t < 300; t += dt) {
    const left = walk(w, to, GAIT.walk, dt, ground, obstacles);
    worstH = Math.max(worstH, Math.abs(ground.height(w.p)));
    const sp = Math.hypot(w.v[0], w.v[1], w.v[2]);
    fastest = Math.max(fastest, sp);
    for (const o of obstacles) {
      ground.axes(w.p, e, u, s);
      const d = [w.p[0] - o.c[0], w.p[1] - o.c[1], w.p[2] - o.c[2]];
      const up = d[0] * u[0] + d[1] * u[1] + d[2] * u[2];
      inside = Math.min(inside, Math.hypot(d[0] - up * u[0], d[1] - up * u[1], d[2] - up * u[2]) - o.r);
    }
    // facing: forward = −sin·east − cos·south against the velocity, once walking steadily
    if (sp > GAIT.walk * 0.9) {
      ground.axes(w.p, e, u, s);
      const f = [-Math.sin(w.yaw) * e[0] - Math.cos(w.yaw) * s[0], -Math.sin(w.yaw) * e[1] - Math.cos(w.yaw) * s[1], -Math.sin(w.yaw) * e[2] - Math.cos(w.yaw) * s[2]];
      const c = (f[0] * w.v[0] + f[1] * w.v[1] + f[2] * w.v[2]) / sp;
      worstFace = Math.max(worstFace, Math.acos(Math.min(1, c)));
    }
    if (left === 0 && sp < 0.05) break;
  }
  const miss = Math.hypot(w.p[0] - to[0], w.p[1] - to[1], w.p[2] - to[2]);
  return { t, worstH, worstFace, fastest, inside, miss };
}

// --- walker ---------------------------------------------------------------------------------------
{
  const start = spawnPoint(0, surfaces);
  const to = offset(start, 45, -38);
  const a = stroll(start, to);
  check('andador en la base: llega, pies en el suelo en cada paso (< 3 cm), mirando adonde va, sin pasarse de velocidad', a.miss < GAIT.arrive + 0.1 && a.worstH < 0.03 && a.worstFace < 0.35 && a.fastest <= GAIT.walk + 1e-6 && a.t < 80, { ...a });
  // far from the base: 40° round the Moon (up is not +y there)
  const b = MOON_BODY;
  const dir: V3 = [Math.sin((40 * Math.PI) / 180), Math.cos((40 * Math.PI) / 180), 0];
  const far: V3 = [b.center[0] + dir[0] * b.radius, b.center[1] + dir[1] * b.radius, b.center[2] + dir[2] * b.radius];
  const h = ground.height(far);
  const e: V3 = [0, 0, 0], u: V3 = [0, 0, 0], s: V3 = [0, 0, 0];
  tangentFrame(b, far, [0, 0, 0, 1], e, u, s);
  const farStart: V3 = [far[0] - u[0] * h, far[1] - u[1] * h, far[2] - u[2] * h];
  const c = stroll(farStart, offset(farStart, -30, 25));
  check('andador lejos de la base (40° más allá, «arriba» no es +y): igual de bien', c.miss < GAIT.arrive + 0.1 && c.worstH < 0.03 && c.worstFace < 0.35, { ...c });
  // an obstacle right in the way
  const mid = offset(start, 22, -19);
  const d = stroll(start, to, [{ c: mid, r: 6 }]);
  check('rodea un obstáculo en el camino sin entrar nunca en él, y llega', d.miss < GAIT.arrive + 0.1 && d.inside > 0.3, { ...d });
}

// --- perception -----------------------------------------------------------------------------------
{
  const eye = (p: V3, fwd: V3, host = 0, air = false): Eye => ({ host, air, p, fwd });
  const at = (p: V3, host = 0, air = false): Sense => ({ host, air, p });
  const e = eye([0, 0, 0], [0, 0, -1]);
  const sight = [
    sees(e, at([0, 0, -30])), // ahead
    sees(e, at([0, 0, 30])), // behind
    sees(e, at([0, 0, 1.5])), // behind but touching
    sees(e, at([0, 0, -200])), // too far
    sees(e, at([0, 0, -30], 7)), // aboard a ship (the hull between)
    sees(eye([0, 0, 0], [0, 0, -1], 7, true), at([0, 0, -10], 7, true)), // both aboard
  ];
  const shot = SENSES && 150;
  const hearing = [
    hears(at([0, 0, 0], 7, true), at([0, 0, -100], 7, true), shot), // same air
    hears(at([0, 0, 0], 7, false), at([0, 0, -100], 7, false), shot), // the hull without air: 60 m
    hears(at([0, 0, 0]), at([0, 0, -15]), shot), // through the ground: 18 m
    hears(at([0, 0, 0]), at([0, 0, -25]), shot), // too far through the ground
    hears(at([0, 0, 0]), at([0, 0, -5], 7, true), shot), // outside vs inside: vacuum between
  ];
  const who = witnesses({ ...at([0, 0, -20]), loud: 6, visible: true }, [
    { id: 1, eye: eye([0, 0, 0], [0, 0, -1]) },
    { id: 2, eye: eye([0, 0, 0], [0, 0, 1]) },
    { id: 3, eye: eye([0, 0, -18], [1, 0, 0]) },
  ]);
  check('percepción: vista (alcance, cono, casco en medio) y oído por lo que lleva el sonido (aire, estructura, suelo; el vacío nada)', sight.join() === 'true,false,true,false,false,true' && hearing.join() === 'true,false,true,false,false' && who.join() === '1,3', { sight, hearing, who });
}

// --- minds ----------------------------------------------------------------------------------------
{
  const base = SITES.find((s): s is BaseSiteDef => s.kind === 'base' && !!s.crew?.length)!;
  const crew = siteCrews(surfaces)[0];
  const w = World.create({ seed: 5, modules: [people] });
  const orders: NpcOrder[] = [];
  const drain = () => {
    for (const [ch, d] of w.outbox.splice(0)) if (ch === 'npc.task') orders.push(d as NpcOrder);
  };
  w.request('people.register', [{ key: crew.key, p: crew.center, count: crew.count, slots: crew.slots, enter: crew.enter, leave: crew.leave, mix: crew.mix }]);
  const near = { observers: [{ host: 0, p: crew.center }] };
  const away = { observers: [] };
  const first = (w.request('people.observe', near) as { up: MadePerson[] }).up;
  w.request('people.observe', away);
  const again = (w.request('people.observe', near) as { up: MadePerson[] }).up;
  const names = (l: MadePerson[]) => l.map((m) => `${m.name}/${m.job}`).join();
  // a whole game day with them out
  const byHour = new Map<string, number[]>([['day', [0, 0, 0]], ['night', [0, 0, 0]]]);
  const t = w.table(Person);
  for (let h = 0; h < 72; h++) {
    w.advance(w.now + 3600);
    drain();
    const hour = w.calendar.parts(w.now).hour;
    const bucket = byHour.get(hour >= 8 && hour < 18 ? 'day' : hour < 5 || hour >= 21 ? 'night' : '')!;
    if (bucket) for (let r = 0; r < t.count; r++) bucket[t.c.activity[r]]++;
  }
  const day = byHour.get('day')!;
  const night = byHour.get('night')!;
  const posts = new Set(base.crew![0].work.flat().map((p) => `${p.x},${p.z}`));
  const toPosts = orders.filter((o) => o.kind === 'goto' && posts.has(`${o.x},${o.z}`)).length;
  check('mentes: la dotación sale con nombres (los mismos al volver); de día trabajan, de noche descansan; sus órdenes van a sus puestos', first.length === crew.slots && names(first) === names(again) && day[0] > day[2] * 2 && night[2] > night[0] && toPosts > 10 && orders.every((o) => o.site === 'base'), {
    people: first.map((m) => `${m.name} (${m.job})`),
    day: Object.fromEntries(ACTIVITIES.map((a, i) => [a, day[i]])),
    night: Object.fromEntries(ACTIVITIES.map((a, i) => [a, night[i]])),
    orders: orders.length,
    toPosts,
  });
}

console.log(failures ? `\n${failures} fallo(s)` : '\nPersonas en orden.');
process.exit(failures ? 1 : 0);
