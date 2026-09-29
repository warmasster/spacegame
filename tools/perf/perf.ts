/**
 * Performance check without a browser: builds every ship's view, the rock field and the network
 * messages in Node and measures what they would cost — draw calls, shadow casters, materials,
 * triangles, CPU per call and garbage per call (V8's sampling heap profiler). Fails (exit 1) when
 * something goes past its budget (BUDGET below), like `test:ship` does for the rules.
 *
 *   npm run perf                  every section
 *   npm run perf -- ships rocks   some sections (ships, sim, rocks, world, net)
 *   npm run perf -- --json out.json
 *
 * GPU time is not measured here (no GPU): F3 in the game shows the real draw calls per frame.
 */
import './dom.js';
import { writeFileSync } from 'node:fs';
import * as THREE from 'three';
import { SHIP_SPAWNS } from '../../src/shared/constants.js';
import { SHIP_DEFS, ShipSim, placeShip } from '../../src/shared/ship/sim.js';
import { FLIGHT_IDLE } from '../../src/shared/ship/flight/index.js';
import { groundAltOn, startCrates } from '../../src/shared/ship/spawn.js';
import { MOON_BODY, surfaceOf, type CelestialBody } from '../../src/shared/space/body.js';
import { cellOf, faceOf, facePoint } from '../../src/shared/space/cubeSphere.js';
import { ROCK_STRIDE, ROCK_VARIANTS, rocksInTile, rockTileLevel } from '../../src/shared/space/rocks.js';
import { siteById } from '../../src/shared/space/sites.js';
import { BodySurface, surfaceSample } from '../../src/shared/space/surface.js';
import { offsetDir } from '../../src/shared/space/tangent.js';
import { blastCrater } from '../../src/shared/space/terrainMods/index.js';
import { siteGround, spawnPoint } from '../../src/shared/space/world.js';
import { VarSync } from '../../src/shared/ship/state.js';
import { crewContext } from '../../src/shared/ship/crew.js';
import { ShipView } from '../../src/client/ship/view.js';
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
  // the rings of world/rocks.ts, in rock tiles (cube-sphere cells) round the spawn
  const RINGS: Array<[number, number]> = [
    [1, 0],
    [3, 0.28],
    [6, 0.75],
  ];
  const T = rockTileLevel(surface);
  const at = spawnPoint(0, surfaces);
  const c = MOON_BODY.center;
  const f = faceOf(at[0] - c[0], at[1] - c[1], at[2] - c[2], facePoint());
  const ti = cellOf(f.a, T);
  const tj = cellOf(f.b, T);
  let count = 0;
  let tiles = 0;
  const list: number[] = [];
  for (const [ring, minSize] of RINGS) {
    for (let dz = -ring; dz <= ring; dz++) {
      for (let dx = -ring; dx <= ring; dx++) {
        const inner = RINGS.find(([r]) => Math.max(Math.abs(dx), Math.abs(dz)) <= r)!;
        if (inner[0] !== ring) continue;
        tiles++;
        list.length = 0;
        count += rocksInTile(surface, f.face, ti + dx, tj + dz, minSize, Infinity, list).length / ROCK_STRIDE;
      }
    }
  }
  const perPass = count * 720;
  table('Rocas alrededor del punto de aparición', ['Baldosas', 'Rocas', 'Variantes', 'Triángulos por pasada', 'Pasadas', 'Triángulos por frame'], [[tiles, count, ROCK_VARIANTS, `${(perPass / 1e6).toFixed(2)} M`, 4, `${((perPass * 4) / 1e6).toFixed(2)} M`]]);
  report.rocks = { tiles, count, trianglesPerFrame: perPass * 4 };
  check('rocas: triángulos por frame', perPass * 4, BUDGET.rockTrianglesPerFrame);
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

const sections: Array<[string, () => Promise<void>]> = [
  ['ships', ships],
  ['sim', simulation],
  ['rocks', rocks],
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
