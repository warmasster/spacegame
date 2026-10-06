/**
 * Performance check without a browser: builds every ship's view, the rock field and the network
 * messages in Node and measures what they would cost — draw calls, shadow casters, materials,
 * triangles, CPU per call and garbage per call (V8's sampling heap profiler). Fails (exit 1) when
 * something goes past its budget (BUDGET below), like `test:ship` does for the rules.
 *
 *   npm run perf                  every section
 *   npm run perf -- ships rocks   some sections (ships, sim, rocks, terrain, world, net)
 *   npm run perf -- --json out.json
 *
 * GPU time is not measured here (no GPU): F3 in the game shows the real draw calls per frame.
 */
import './dom.js';
import { readFileSync, writeFileSync } from 'node:fs';
import * as THREE from 'three';
import { SHIP_SPAWNS } from '../../src/shared/constants.js';
import { SHIP_DEFS, ShipSim, placeShip } from '../../src/shared/ship/sim.js';
import { FLIGHT_IDLE } from '../../src/shared/ship/flight/index.js';
import { groundAltOn, startCrates } from '../../src/shared/ship/spawn.js';
import { MOON_BODY, sunDirection, surfaceOf, type CelestialBody } from '../../src/shared/space/body.js';
import { cellOf, cubeArc, cubeDir, faceOf, facePoint } from '../../src/shared/space/cubeSphere.js';
import { ROCK_STRIDE, ROCK_VARIANTS, rocksInTile, rockTileLevel } from '../../src/shared/space/rocks.js';
import { siteById } from '../../src/shared/space/sites.js';
import { BodySurface, surfaceSample } from '../../src/shared/space/surface.js';
import { offsetDir } from '../../src/shared/space/tangent.js';
import { blastCrater } from '../../src/shared/space/terrainMods/index.js';
import { siteGround, spawnPoint } from '../../src/shared/space/world.js';
import { VarSync } from '../../src/shared/ship/state.js';
import { crewContext } from '../../src/shared/ship/crew.js';
import { ShipView } from '../../src/client/ship/view.js';
import { ROCK_CASTERS, ROCK_LOD_TRIANGLES, ROCK_RINGS, rockLod } from '../../src/client/world/rocks.js';
import { TERRAIN_RES, TERRAIN_SHADOW_RANGE, TERRAIN_SPLIT } from '../../src/client/world/terrainGrid.js';
import { DETAIL_MATERIALS } from '../../src/client/player/astronaut.js';
import { ShadowCull } from '../../src/client/render/shadowCull.js';
import { CSM } from 'three/addons/csm/CSM.js';
import { SUN } from '../../src/shared/constants.js';
import { siteCrews, siteToWorld } from '../../src/shared/ship/spawn.js';
import { updateInteriorLights } from '../../src/client/ship/interiorLights.js';
import { allocIt, close, kb, ms, sceneStats, timeIt, type Alloc } from './measure.js';

const args = process.argv.slice(2);
const jsonOut = args.includes('--json') ? args[args.indexOf('--json') + 1] : null;
const wanted = args.filter((a) => !a.startsWith('--') && a !== jsonOut);
const on = (s: string) => !wanted.length || wanted.includes(s);

/** Budgets: past any of these the check fails. Per ship unless said otherwise. */
const BUDGET = {
  shipDraws: Infinity,
  shipShadowDraws: Infinity,
  shipMaterials: Infinity,
  shipUnculled: Infinity,
  viewUpdateMs: Infinity,
  viewUpdateBytes: Infinity,
  simTickBytes: Infinity,
  flightAwakeBytes: Infinity,
  flightSleepBytes: Infinity,
  flightSleepMs: Infinity,
  rockTrianglesPerFrame: Infinity,
};

const report: Record<string, unknown> = {};
const failures: string[] = [];
const check = (what: string, value: number, budget: number, fmt: (v: number) => string = String) => {
  if (value > budget) failures.push(`${what}: ${fmt(value)} > ${fmt(budget)}`);
};

function table(title: string, head: string[], rows: Array<Array<string | number>>) {
  console.log(`\n## ${title}\n`);
  const w = head.map((h, i) => Math.max(h.length, ...rows.map((r) => String(r[i]).length)));
  const line = (r: Array<string | number>) => '| ' + r.map((c, i) => String(c).padEnd(w[i])).join(' | ') + ' |';
  console.log(line(head));
  console.log('|' + w.map((n) => '-'.repeat(n + 2)).join('|') + '|');
  for (const r of rows) console.log(line(r));
}

const topLine = (a: Alloc) => a.top.slice(0, 3).map(([k, v]) => `${k} ${kb(v)}`).join(' · ');

const seed = 1969;
/** The world's ground (one surface per body, with its sites), as the game has it. */
const surfaces = (b: CelestialBody) => surfaceOf(b, seed);
const surface = surfaces(MOON_BODY)!;
const defs = SHIP_SPAWNS.map((s) => ({ spawn: s, def: SHIP_DEFS[s.def] }));

/** A ship where the world starts it (shared/ship/spawn.ts), on its own. */
function makeSim(i: number) {
  const { spawn, def } = defs[i];
  const site = siteById(spawn.site)!;
  return new ShipSim(spawn.id, def, placeShip(def, siteGround(site, surfaces)!, spawn.x, spawn.z, spawn.yaw), groundAltOn(surfaces, site.body));
}

// ---------------------------------------------------------------------------------------------------
// Ships: what the renderer gets, and what a frame costs
// ---------------------------------------------------------------------------------------------------

async function ships() {
  const rows: Array<Array<string | number>> = [];
  const cpu: Array<Array<string | number>> = [];
  const camera = new THREE.PerspectiveCamera(72, 16 / 9, 0.05, 60000);
  camera.updateMatrixWorld();
  const out: Record<string, unknown> = {};
  for (let i = 0; i < defs.length; i++) {
    const def = defs[i].def;
    const sim = makeSim(i);
    const t0 = performance.now();
    const view = new ShipView(sim, null, surfaces);
    const buildMs = performance.now() - t0;
    const anim = { movers: Object.fromEntries(def.movers.map((m) => [m.key, sim.mover(m.key)])) };
    const feet = sim.flight.feetAnywhere(sim.pose, surface, sim.flight.gear());
    let time = 0;
    const frame = () => {
      time += 1 / 60;
      view.update(1 / 60, time, anim, sim.pose, feet);
      updateInteriorLights(camera);
    };
    frame();
    const st = sceneStats(view.root);
    const updMs = timeIt(frame);
    const updAlloc = await allocIt(frame);
    const blockedMs = timeIt(() => {
      for (const c of def.controls) sim.blocked(c);
    });
    rows.push([def.name, def.panels.length, def.controls.length, def.screens.length, `${def.parts.length} + ${def.props.length}`, st.draws, st.shadowDraws, `${(st.triangles / 1000).toFixed(1)} K`, st.materials, st.programs, st.unculled, `${buildMs.toFixed(0)} ms`]);
    cpu.push([def.name, ms(updMs), kb(updAlloc.bytes), ms(blockedMs), topLine(updAlloc)]);
    out[def.id] = { draws: st.draws, shadowDraws: st.shadowDraws, triangles: st.triangles, materials: st.materials, programs: st.programs, unculled: st.unculled, buildMs, updateMs: updMs, updateBytes: updAlloc.bytes, blockedMs, byName: Object.fromEntries(st.byName) };
    check(`${def.name}: draws`, st.draws, BUDGET.shipDraws);
    check(`${def.name}: draws con sombra`, st.shadowDraws, BUDGET.shipShadowDraws);
    check(`${def.name}: materiales`, st.materials, BUDGET.shipMaterials);
    check(`${def.name}: sin frustum culling`, st.unculled, BUDGET.shipUnculled);
    check(`${def.name}: ShipView.update`, updMs, BUDGET.viewUpdateMs, ms);
    check(`${def.name}: basura de ShipView.update`, updAlloc.bytes, BUDGET.viewUpdateBytes, kb);
  }
  table('Naves: lo que recibe el renderizador (antes de culling)', ['Nave', 'Paneles', 'Mandos', 'Pantallas', 'Máquinas + muebles', 'Draws', 'Con sombra', 'Triángulos', 'Materiales', 'Programas', 'Sin culling', 'Construir'], rows);
  table('Naves: coste por frame en el cliente', ['Nave', 'ShipView.update', 'Basura/frame', 'blocked() ×todos', 'Quién ensucia'], cpu);
  report.ships = out;
}

// ---------------------------------------------------------------------------------------------------
// Ship simulation: systems tick and flight step (server and offline client)
// ---------------------------------------------------------------------------------------------------

async function simulation() {
  const rows: Array<Array<string | number>> = [];
  const out: Record<string, unknown> = {};
  for (let i = 0; i < defs.length; i++) {
    const def = defs[i].def;
    // systems at 20 Hz with nobody aboard
    const s = makeSim(i);
    const ctx = crewContext(s, []);
    const tick = () => void s.tick(0.05, { crew: ctx.counts, docked: 0, bodies: [] });
    const tickMs = timeIt(tick, 200, 40);
    const tickAlloc = await allocIt(tick, 200, 20);
    // flight: parked and awake (the pilot just sat down), near the ground
    const a = makeSim(i);
    for (let k = 0; k < 30; k++) a.flight.step(1 / 60, FLIGHT_IDLE, { surface });
    const awake = () => {
      a.flight.wake();
      a.flight.step(1 / 60, FLIGHT_IDLE, { surface });
    };
    const awakeMs = timeIt(awake, 300, 30);
    const awakeAlloc = await allocIt(awake, 200, 20);
    // flight: hovering high above the ground (coupled flight holds it)
    const h = makeSim(i);
    h.pose.p[1] += 40;
    h.landed = false;
    h.flight.wake();
    for (let k = 0; k < 60; k++) h.flight.step(1 / 60, FLIGHT_IDLE, { surface });
    const hover = () => h.flight.step(1 / 60, FLIGHT_IDLE, { surface });
    const hoverMs = timeIt(hover, 300, 30);
    const hoverAlloc = await allocIt(hover, 200, 20);
    // flight: parked and asleep
    const z = makeSim(i);
    for (let k = 0; k < 600 && !z.flight.sleeping; k++) z.flight.step(1 / 60, FLIGHT_IDLE, { surface });
    const sleep = () => z.flight.step(1 / 60, FLIGHT_IDLE, { surface });
    const sleepMs = timeIt(sleep, 400, 30);
    const sleepAlloc = await allocIt(sleep, 300, 20);
    rows.push([def.name, ms(tickMs), kb(tickAlloc.bytes), ms(awakeMs), kb(awakeAlloc.bytes), ms(hoverMs), kb(hoverAlloc.bytes), z.flight.sleeping ? ms(sleepMs) : '(no duerme)', kb(sleepAlloc.bytes)]);
    out[def.id] = { tickMs, tickBytes: tickAlloc.bytes, tickTop: tickAlloc.top, awakeMs, awakeBytes: awakeAlloc.bytes, awakeTop: awakeAlloc.top, hoverMs, hoverBytes: hoverAlloc.bytes, sleepMs, sleepBytes: sleepAlloc.bytes, sleepTop: sleepAlloc.top, slept: z.flight.sleeping };
    check(`${def.name}: basura de ShipSim.tick`, tickAlloc.bytes, BUDGET.simTickBytes, kb);
    check(`${def.name}: basura de flight.step despierta`, awakeAlloc.bytes, BUDGET.flightAwakeBytes, kb);
    check(`${def.name}: basura de flight.step dormida`, sleepAlloc.bytes, BUDGET.flightSleepBytes, kb);
    check(`${def.name}: flight.step dormida`, sleepMs, BUDGET.flightSleepMs, ms);
    if (i === defs.length - 1) {
      console.log('\nQuién ensucia (última nave):');
      console.log('  tick      ', topLine(tickAlloc));
      console.log('  despierta ', topLine(awakeAlloc));
      console.log('  en vuelo  ', topLine(hoverAlloc));
      console.log('  dormida   ', topLine(sleepAlloc));
    }
  }
  table('Simulación de nave (servidor / offline)', ['Nave', 'tick 20 Hz', 'basura', 'vuelo despierta', 'basura', 'vuelo en el aire', 'basura', 'vuelo dormida', 'basura'], rows);
  report.sim = out;
}

// ---------------------------------------------------------------------------------------------------
// Rocks around the spawn
// ---------------------------------------------------------------------------------------------------

async function rocks() {
  // the rings of world/rocks.ts round the spawn, with its own rules: the LOD each rock gets at its
  // distance from the eye, the ones too small to see, and who casts a shadow (drawn once per cascade)
  const T = rockTileLevel(surface);
  const at = spawnPoint(0, surfaces);
  const c = MOON_BODY.center;
  const up = [at[0] - c[0], at[1] - c[1], at[2] - c[2]];
  const ul = Math.hypot(up[0], up[1], up[2]);
  const eye = up.map((v, i) => at[i] + (v / ul) * 1.7);
  const f = faceOf(at[0] - c[0], at[1] - c[1], at[2] - c[2], facePoint());
  const ti = cellOf(f.a, T);
  const tj = cellOf(f.b, T);
  let count = 0, drawn = 0, tiles = 0, main = 0, cast = 0;
  const list: number[] = [];
  ROCK_RINGS.forEach(([ring, minSize], ri) => {
    for (let dz = -ring; dz <= ring; dz++) {
      for (let dx = -ring; dx <= ring; dx++) {
        const inner = ROCK_RINGS.find(([r]) => Math.max(Math.abs(dx), Math.abs(dz)) <= r)!;
        if (inner[0] !== ring) continue;
        tiles++;
        list.length = 0;
        rocksInTile(surface, f.face, ti + dx, tj + dz, minSize, Infinity, list);
        for (let k = 0; k < list.length; k += ROCK_STRIDE) {
          count++;
          const r = MOON_BODY.radius + list[k + 3];
          const p = [c[0] + list[k] * r, c[1] + list[k + 1] * r, c[2] + list[k + 2] * r];
          const size = list[k + 4];
          const lod = rockLod(size, Math.hypot(p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]));
          if (lod < 0) continue;
          drawn++;
          main += ROCK_LOD_TRIANGLES[lod];
          if (size >= ROCK_CASTERS[ri]) cast += ROCK_LOD_TRIANGLES[lod];
        }
      }
    }
  });
  const rows = (['high', 'low'] as const).map((q) => {
    const cascades = q === 'high' ? 3 : 1;
    return [q === 'high' ? 'ALTA' : 'BAJA', tiles, count, drawn, `${(main / 1e3).toFixed(0)} K`, `${(cast / 1e3).toFixed(0)} K × ${cascades}`, `${((main + cast * cascades) / 1e6).toFixed(2)} M`];
  });
  table('Rocas alrededor del punto de aparición (sus reglas de LOD y sombra)', ['Perfil', 'Baldosas', 'Rocas', 'Dibujadas', 'Triángulos (vista)', 'Sombra (por cascada)', 'Triángulos por frame'], rows);
  report.rocks = { tiles, count, drawn, main, cast, trianglesPerFrame: main + cast * 3 };
  check('rocas: triángulos por frame', main + cast * 3, BUDGET.rockTrianglesPerFrame);
}

// ---------------------------------------------------------------------------------------------------
// World queries
// ---------------------------------------------------------------------------------------------------

async function world() {
  // a surface of its own (no sites): the same ground, bare and then with a thousand blast craters
  const t = new BodySurface(MOON_BODY.surface!, MOON_BODY, seed);
  const pole = MOON_BODY.pole;
  const R = MOON_BODY.radius;
  const pts = Array.from({ length: 2000 }, (_, i) => offsetDir(pole, [0, 1, 0], ((i * 7919) % 400) - 200, ((i * 104729) % 400) - 200, R, [0, 0, 0]));
  const smp = surfaceSample();
  let k = 0;
  const at = (mf: number) => () => {
    const d = pts[k++ % pts.length];
    t.sample(d[0], d[1], d[2], mf, smp);
  };
  const plain = timeIt(at(0), 20000, 2000);
  const coarse = timeIt(at(5), 20000, 2000);
  const c = MOON_BODY.center;
  for (let i = 0; i < 1000; i++) {
    const d = offsetDir(pole, [0, 1, 0], ((i * 37) % 800) - 400, ((i * 91) % 800) - 400, R, [0, 0, 0]);
    t.addMod(blastCrater('moon', c, [c[0] + d[0] * R, c[1] + d[1] * R, c[2] + d[2] * R], 2.4));
  }
  const edited = timeIt(at(0), 20000, 2000);
  table('Consultas del suelo (superficie global)', ['sample() a detalle completo', 'a 5 m', 'con 1.000 cráteres'], [[`${(plain * 1000).toFixed(2)} µs`, `${(coarse * 1000).toFixed(2)} µs`, `${(edited * 1000).toFixed(2)} µs`]]);
  report.world = { heightUs: plain * 1000, heightCoarseUs: coarse * 1000, heightEditedUs: edited * 1000, mods: t.mods.count };
}

// ---------------------------------------------------------------------------------------------------
// Network: message sizes as they go on the wire today
// ---------------------------------------------------------------------------------------------------

async function net() {
  const sims = defs.map((_, i) => makeSim(i));
  const welcome = (edits: number) =>
    JSON.stringify({
      type: 'welcome',
      id: 1,
      variant: 0,
      players: [],
      spawn: spawnPoint(0, surfaces),
      worldSeed: seed,
      serverTime: 12345.678,
      mods: Array.from({ length: edits }, (_, i) => {
        const d = offsetDir(MOON_BODY.pole, [0, 1, 0], (i % 400) - 200.25, ((i * 7) % 400) - 200.75, MOON_BODY.radius, [0, 0, 0]);
        const c = MOON_BODY.center;
        return blastCrater('moon', c, [c[0] + d[0] * MOON_BODY.radius, c[1] + d[1] * MOON_BODY.radius, c[2] + d[2] * MOON_BODY.radius], 2.4);
      }),
      health: [],
      ships: sims.map((s) => s.snapshot()),
      pilots: [],
      crates: startCrates(sims, surfaces),
    }).length;
  const snap = sims.map((s) => JSON.stringify(s.snapshot()).length);
  const r4 = (n: number) => Math.round(n * 1e4) / 1e4;
  const pose = JSON.stringify({ type: 'shipPose', ship: 1, t: 123456.789, p: sims[0].pose.p.map(r4), q: sims[0].pose.q.map(r4), v: [0.1234, -0.0012, 3.2101], w: [0.0012, 0.0, -0.0101], landed: false, pad: false }).length;
  const state = JSON.stringify({ type: 'snapshot', t: 123456.789, states: [{ id: 2, t: 123450.12, s: { p: [12.345, 3.21, -45.678], v: [1.23, 0, -0.45], yaw: 1.234, pitch: -0.123, f: 1, fr: 3 } }] }).length;
  // state diffs over 10 s of a ship doing something (reactor warm, doors cycling)
  const s = sims[0];
  const sync = new VarSync(s.vars, s.st);
  let bytes = 0;
  let msgs = 0;
  for (let k = 0; k < 200; k++) {
    if (k === 20) for (const m of s.def.movers.slice(0, 2)) s.sw[m.key] = 1 - (s.sw[m.key] ?? 0);
    s.tick(0.05);
    if (k % 2) continue;
    const d = sync.diff(s.st);
    if (!d.length) continue;
    msgs++;
    bytes += JSON.stringify({ type: 'shipSt', ship: s.id, d }).length;
  }
  table('Red (JSON tal y como se envía hoy)', ['welcome (3 naves)', 'welcome + 4.000 cráteres', 'instantánea de nave', 'pose de nave', 'estado de un jugador', 'diferencias shipSt'], [[kb(welcome(0)), kb(welcome(4000)), snap.map((b) => kb(b)).join(' · '), `${pose} B`, `${state} B`, `${msgs ? Math.round(bytes / msgs) : 0} B × ${msgs} en 10 s`]]);
  report.net = { welcome: welcome(0), welcome4000: welcome(4000), snapshots: snap, pose, state, shipStAvg: msgs ? bytes / msgs : 0, shipStMsgs: msgs };
}


// ---------------------------------------------------------------------------------------------------
// Terrain: the quadtree's nodes round the spawn (draw calls: one per node, and per cascade it casts in)
// ---------------------------------------------------------------------------------------------------

/**
 * The selection of world/sphereTerrain.ts (split, horizon, view, shadow range), run here with every
 * node built (its heights sampled from the ground), for a camera at the spawn's eye looking along the
 * horizon: nodes drawn, nodes casting, and the largest level jump between neighbours (the morph between
 * levels needs it ≤ 1).
 */
/** The quadtree's leaves round the spawn, the camera at eye height looking level toward `yaw` (rad). */
function terrainNodes(res: number, split: number, shadowRange: number, yaw = 0) {
  const R = MOON_BODY.radius;
  const c = MOON_BODY.center;
  const maxLevel = Math.ceil(Math.log2(cubeArc(2, R) / (res * 0.4)));
  const at = spawnPoint(0, surfaces);
  const up = new THREE.Vector3(at[0] - c[0], at[1] - c[1], at[2] - c[2]).normalize();
  const eye = new THREE.Vector3(...at).addScaledVector(up, 1.7);
  const cam = new THREE.PerspectiveCamera(72, 16 / 9, 0.05, 1e7);
  cam.position.copy(eye);
  cam.up.copy(up);
  cam.lookAt(eye.clone().add(new THREE.Vector3(1, 0, 0).projectOnPlane(up).normalize().applyAxisAngle(up, yaw)));
  cam.updateMatrixWorld();
  const frustum = new THREE.Frustum().setFromProjectionMatrix(new THREE.Matrix4().multiplyMatrices(cam.projectionMatrix, cam.matrixWorldInverse));
  const camRel = eye.clone().sub(new THREE.Vector3(...c));
  const low = R + Math.max(surface.lowest, -3000);
  type N = { face: number; level: number; a0: number; b0: number; size: number; dir: number[]; arc: number; hMin: number; hMax: number };
  const node = (face: number, level: number, a0: number, b0: number, size: number): N => {
    const hs = [[0.5, 0.5], [0, 0], [1, 0], [0, 1], [1, 1], [0.25, 0.75], [0.75, 0.25]].map(([u, v]) => surface.height(cubeDir(face, a0 + u * size, b0 + v * size, [0, 0, 0])));
    return { face, level, a0, b0, size, dir: cubeDir(face, a0 + size / 2, b0 + size / 2, [0, 0, 0]), arc: cubeArc(size, R), hMin: Math.min(...hs), hMax: Math.max(...hs) };
  };
  const D = camRel.length();
  const hidden = (n: N) => {
    const g = Math.acos(Math.max(-1, Math.min(1, camRel.dot(new THREE.Vector3(...n.dir)) / D)));
    return g > Math.acos(Math.min(1, low / D)) + Math.acos(Math.min(1, low / (R + Math.max(n.hMax, 0)))) + (n.arc * 0.75) / R;
  };
  const dist = (n: N) => Math.max(0, new THREE.Vector3(...n.dir).multiplyScalar(R + (n.hMin + n.hMax) / 2).distanceTo(camRel) - n.arc * 0.75 - (n.hMax - n.hMin) / 2);
  let drawn = 0, casters = 0;
  const leaves: N[] = [];
  const spheres: Array<{ sphere: THREE.Sphere; caster: boolean }> = [];
  const walk = (n: N) => {
    if (hidden(n)) return;
    const d = dist(n);
    if (n.level < maxLevel && d < n.arc * split) {
      const h = n.size / 2, l = n.level + 1;
      for (const [da, db] of [[0, 0], [h, 0], [0, h], [h, h]]) walk(node(n.face, l, n.a0 + da, n.b0 + db, h));
      return;
    }
    leaves.push(n);
    const caster = d < shadowRange;
    const centre = new THREE.Vector3(...n.dir).multiplyScalar(R + (n.hMin + n.hMax) / 2).add(new THREE.Vector3(...c));
    const sphere = new THREE.Sphere(centre, n.arc * 0.75 + (n.hMax - n.hMin) / 2);
    spheres.push({ sphere, caster });
    const visible = frustum.intersectsSphere(sphere);
    if (visible) drawn++;
    if (caster) casters++;
  };
  for (let f = 0; f < 6; f++) walk(node(f, 0, -1, -1, 2));
  let jump = 0;
  for (const l of leaves) {
    const e = l.size * 1e-3;
    for (let k = 0; k < 4; k++)
      for (let t = 0.1; t < 1; t += 0.2) {
        const a = k === 0 ? l.a0 - e : k === 1 ? l.a0 + l.size + e : l.a0 + t * l.size;
        const b = k === 2 ? l.b0 - e : k === 3 ? l.b0 + l.size + e : l.b0 + t * l.size;
        const nb = leaves.find((o) => o.face === l.face && a >= o.a0 && a < o.a0 + o.size && b >= o.b0 && b < o.b0 + o.size);
        if (nb) jump = Math.max(jump, Math.abs(nb.level - l.level));
      }
  }
  return { drawn, casters, triangles: drawn * res * res * 2, jump, spheres, finest: cubeArc(2, R) / 2 ** maxLevel / res };
}

async function crew() {
  // the suit as the GLB has it: one primitive per draw (materials: the model's parts)
  const glb = readFileSync(new URL('../../public/assets/astronaut.glb', import.meta.url));
  const json = JSON.parse(glb.subarray(20, 20 + glb.readUInt32LE(12)).toString()) as { nodes: Array<{ mesh?: number }>; meshes: Array<{ primitives: Array<{ material?: number }> }>; materials: Array<{ name: string }> };
  let all = 0, detail = 0;
  for (const n of json.nodes) {
    if (n.mesh === undefined) continue;
    for (const p of json.meshes[n.mesh].primitives) {
      // the bubble inside the helmet is never drawn
      const name = json.materials[p.material ?? 0]?.name ?? '';
      if (name === 'HelmetInner') continue;
      all++;
      if (DETAIL_MATERIALS.has(name)) detail++;
    }
  }
  const rows = [
    ['antes', all, all, all, `${all} × 2`, all + all * 2],
    ['ahora', all, all - detail, all - detail, `${all - detail} × 1`, all - detail + (all - detail)],
  ];
  table('Astronautas: draws de cada uno (jugadores, NPC)', ['', 'Cerca (< 12 m)', 'Lejos', 'Con sombra', 'Sombra × cascadas (BAJA)', 'Total lejos (BAJA)'], rows);
  report.crew = { all, detail };
}

async function terrain() {
  const rows: Array<Array<string | number>> = [];
  const cases: Array<[string, number, number, number, number]> = [
    ['antes (32 × 2,2), ALTA', 32, 2.2, 260, 3],
    ['antes (32 × 2,2), BAJA', 32, 2.2, 260, 2],
    [`ahora (${TERRAIN_RES} × ${TERRAIN_SPLIT}), ALTA`, TERRAIN_RES, TERRAIN_SPLIT, TERRAIN_SHADOW_RANGE.high, 3],
    [`ahora (${TERRAIN_RES} × ${TERRAIN_SPLIT}), BAJA`, TERRAIN_RES, TERRAIN_SPLIT, TERRAIN_SHADOW_RANGE.low, 1],
    ...(process.env.GRIDS ?? '').split(',').filter(Boolean).map((g): [string, number, number, number, number] => {
      const [r, sp] = g.split('x').map(Number);
      return [`${r} × ${sp}, BAJA`, r, sp, TERRAIN_SHADOW_RANGE.low, 1];
    }),
  ];
  // the view turned round (8 headings): nodes in view vary with where one looks
  const YAWS = 8;
  for (const [name, res, split, range, cascades] of cases) {
    const all = Array.from({ length: YAWS }, (_, k) => terrainNodes(res, split, range, (k / YAWS) * Math.PI * 2));
    const t = all[0];
    const drawn = all.map((a) => a.drawn);
    const mean = Math.round(drawn.reduce((a, b) => a + b, 0) / YAWS);
    const most = Math.max(...drawn);
    const jump = Math.max(...all.map((a) => a.jump));
    rows.push([name, `${mean} (${Math.min(...drawn)}-${most})`, `${t.casters} × ${cascades}`, most + t.casters * cascades, `${((mean * res * res * 2) / 1e3).toFixed(0)} K`, `${(1 / (res * split)).toFixed(4)} rad`, `${t.finest.toFixed(2)} m`, jump]);
    if (res === TERRAIN_RES) check(`terreno ${name}: salto de nivel entre vecinos`, jump, 1);
  }
  table('Terreno alrededor del punto de aparición (nodos = draws; los cercanos, además, una vez por cascada; 8 direcciones)', ['Rejilla y perfil', 'Nodos a la vista: media (mín-máx)', 'Con sombra × cascadas', 'Draws (máx.)', 'Triángulos a la vista (media)', 'Celda vista', 'Celda más fina', 'Salto de nivel'], rows);
}



// ---------------------------------------------------------------------------------------------------
// The whole frame at the base: what the main pass and the shadow cascade draw, looking up and at a ship
// ---------------------------------------------------------------------------------------------------

/**
 * The base's scene in Node — the three ships (their real views, where they park), the terrain nodes
 * the quadtree picks (as spheres), the crew and the player (as their suit's meshes), the low profile's
 * real cascade (three's CSM) — and a camera at the spawn's eye. Counted as three.js does: the main
 * pass draws what is visible and in the view (or never culled), the cascade what casts and is in its
 * box. With and without render/shadowCull.ts. Sky, particles, crates, rocks and the post passes (a
 * few dozen draws, always) are not in it.
 */
async function frame() {
  const scene = new THREE.Scene();
  // ships where they park
  for (let i = 0; i < defs.length; i++) {
    const sim = makeSim(i);
    const view = new ShipView(sim, null, surfaces);
    const anim = { movers: Object.fromEntries(defs[i].def.movers.map((m) => [m.key, sim.mover(m.key)])) };
    view.update(1 / 60, 0, anim, sim.pose, sim.flight.feetAnywhere(sim.pose, surface, sim.flight.gear()));
    scene.add(view.root);
  }
  // the terrain the quadtree picks, as spheres (one draw each; the near ones cast)
  const bare = new THREE.MeshBasicMaterial();
  const sphereMesh = (center: THREE.Vector3, radius: number, cast: boolean, name: string) => {
    const g = new THREE.BufferGeometry();
    g.boundingSphere = new THREE.Sphere(new THREE.Vector3(), radius);
    const m = new THREE.Mesh(g, bare);
    m.position.copy(center);
    m.castShadow = cast;
    m.name = name;
    return m;
  };
  for (const n of terrainNodes(TERRAIN_RES, TERRAIN_SPLIT, TERRAIN_SHADOW_RANGE.low).spheres) scene.add(sphereMesh(n.sphere.center, n.sphere.radius, n.caster, 'terreno'));
  // the crew round where they gather and the player: a suit each (its draws; the detail parts cast nothing)
  const glb = readFileSync(new URL('../../public/assets/astronaut.glb', import.meta.url));
  const json = JSON.parse(glb.subarray(20, 20 + glb.readUInt32LE(12)).toString()) as { nodes: Array<{ mesh?: number }>; meshes: Array<{ primitives: Array<{ material?: number }> }>; materials: Array<{ name: string }> };
  const parts: string[] = [];
  for (const n of json.nodes) if (n.mesh !== undefined) for (const pr of json.meshes[n.mesh].primitives) parts.push(json.materials[pr.material ?? 0]?.name ?? '');
  const crew = siteCrews(surfaces)[0];
  const spawnAt = spawnPoint(0, surfaces);
  const people = [spawnAt, ...Array.from({ length: 6 }, (_, k) => siteToWorld(surfaces, crew.site, crew.x + Math.cos(k) * 2.5, crew.z + Math.sin(k) * 2.5)!)];
  for (const p of people) for (const name of parts) if (name !== 'HelmetInner') scene.add(sphereMesh(new THREE.Vector3(p[0], p[1] + 0.9, p[2]), 1.2, !DETAIL_MATERIALS.has(name), 'astronautas'));
  scene.updateMatrixWorld(true);
  // the camera at the spawn's eye, and the low profile's cascade
  const up = new THREE.Vector3(spawnAt[0] - MOON_BODY.center[0], spawnAt[1] - MOON_BODY.center[1], spawnAt[2] - MOON_BODY.center[2]).normalize();
  const eye = new THREE.Vector3(spawnAt[0], spawnAt[1], spawnAt[2]).addScaledVector(up, 1.7);
  const cam = new THREE.PerspectiveCamera(72, 16 / 9, 0.05, 60000);
  cam.position.copy(eye);
  cam.up.copy(up);
  const sd = sunDirection(SUN.az, SUN.el);
  const sun = new THREE.Vector3(sd[0], sd[1], sd[2]);
  const csm = new CSM({ camera: cam, parent: scene, cascades: 1, maxFar: 90, mode: 'practical', shadowMapSize: 2048, lightDirection: sun.clone().negate(), lightNear: 1, lightFar: 900, lightMargin: 160 });
  const count = (from: THREE.Vector3, look: THREE.Vector3, cull: boolean) => {
    cam.position.copy(from);
    cam.lookAt(look);
    cam.updateMatrixWorld(true);
    csm.updateFrustums();
    csm.update();
    scene.updateMatrixWorld(true);
    const culler = new ShadowCull(scene, cam, () => csm.lightDirection);
    if (cull) culler.before();
    const view = new THREE.Frustum().setFromProjectionMatrix(new THREE.Matrix4().multiplyMatrices(cam.projectionMatrix, cam.matrixWorldInverse));
    const sc = csm.lights[0].shadow.camera;
    sc.updateMatrixWorld(true);
    const box = new THREE.Frustum().setFromProjectionMatrix(new THREE.Matrix4().multiplyMatrices(sc.projectionMatrix, sc.matrixWorldInverse));
    const by = new Map<string, [number, number]>();
    let main = 0;
    let shadow = 0;
    scene.traverseVisible((o) => {
      const m = o as THREE.Mesh;
      if (!m.isMesh || !m.geometry) return;
      if ((m as unknown as THREE.InstancedMesh).isInstancedMesh && (m as unknown as THREE.InstancedMesh).count === 0) return;
      const list = Array.isArray(m.material) ? m.material : [m.material];
      const groups = Array.isArray(m.material) ? Math.max(1, m.geometry.groups.length) : list[0].visible ? 1 : 0;
      if (!groups) return;
      if (!m.geometry.boundingSphere) m.geometry.computeBoundingSphere();
      const inView = !m.frustumCulled || view.intersectsObject(m);
      const inBox = m.castShadow && (!m.frustumCulled || box.intersectsObject(m));
      const key = m.name === 'terreno' || m.name === 'astronautas' ? m.name : 'naves';
      const e = by.get(key) ?? [0, 0];
      if (inView) {
        e[0] += groups;
        main += groups;
      }
      if (inBox) {
        e[1] += groups;
        shadow += groups;
      }
      by.set(key, e);
    });
    if (cull) culler.after();
    return { main, shadow, total: main + shadow, by: Object.fromEntries(by) };
  };
  const sky = eye.clone().addScaledVector(up, 100).add(new THREE.Vector3(1, 0, 0).projectOnPlane(up).multiplyScalar(30));
  const poses = defs.map((_, i) => makeSim(i).pose.p);
  poses.sort((a, b) => Math.hypot(a[0] - eye.x, a[2] - eye.z) - Math.hypot(b[0] - eye.x, b[2] - eye.z));
  const ship = new THREE.Vector3(poses[0][0], poses[0][1], poses[0][2]);
  const rows: Array<Array<string | number>> = [];
  const out: Record<string, unknown> = {};
  const fmt = (r: { by: Record<string, [number, number]> }) =>
    Object.entries(r.by)
      .map(([k, [m, sh]]) => `${k} ${m}+${sh}`)
      .join(' · ');
  // 12 m from the nearest ship's centre, toward the spawn, at eye height
  const close = ship.clone().add(eye.clone().sub(ship).projectOnPlane(up).setLength(12));
  close.addScaledVector(up, eye.clone().sub(close).dot(up));
  const skyClose = close.clone().addScaledVector(up, 100).add(new THREE.Vector3(1, 0, 0).projectOnPlane(up).multiplyScalar(30));
  for (const [name, from, look] of [
    ['al cielo', eye, sky],
    ['a la nave', eye, ship],
    ['junto a la nave, al cielo', close, skyClose],
    ['junto a la nave, a ella', close, ship],
  ] as const) {
    const a = count(from, look, false);
    const b = count(from, look, true);
    rows.push([`${name}, antes`, a.main, a.shadow, a.total, fmt(a)]);
    rows.push([`${name}, con recorte de sombras`, b.main, b.shadow, b.total, fmt(b)]);
    out[name] = { before: a, after: b };
  }
  table('El frame en la base (BAJA): pasada principal + cascada de sombra', ['Mirando', 'Principal', 'Sombra', 'Total', 'Por grupo (principal+sombra)'], rows);
  report.frame = out;
}

const sections: Array<[string, () => Promise<void>]> = [
  ['ships', ships],
  ['sim', simulation],
  ['rocks', rocks],
  ['terrain', terrain],
  ['crew', crew],
  ['frame', frame],
  ['world', world],
  ['net', net],
];
for (const [name, run] of sections) if (on(name)) await run();
close();
if (jsonOut) writeFileSync(jsonOut, JSON.stringify(report, null, 1));
if (failures.length) {
  console.log(`\n✗ ${failures.length} fuera de presupuesto:\n  ${failures.join('\n  ')}`);
  process.exit(1);
}
console.log('\n✓ todo dentro de presupuesto');
