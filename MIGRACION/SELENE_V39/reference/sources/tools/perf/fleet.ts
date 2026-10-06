/**
 * Many ships and NPCs at once (npx tsx tools/perf/fleet.ts [10 30 60 100]), without a browser: N
 * ships of the three kinds parked round the base out to 1.5 km (a fifth of them flying, piloted), one NPC
 * wandering by each, the player at the spawn, the low profile's single cascade. Per N:
 *
 *   client   draws (main + shadow, the mean over 8 headings, with the shadow cull), triangles,
 *            ShipView.update of all of them, three's matrices, the lunar Rapier world's step
 *   server   systems tick (20 Hz), flight (60 Hz), NPC walking (20 Hz): CPU share of one core
 *   network  what one player at the spawn receives (bytes and messages per second), room rules
 *
 * No GPU here: the frame's WebGL submission is modelled at ~12 µs per draw (the game's own F3: ~1000
 * draws ≈ 12 ms on the Intel laptop). Terrain, sky and post passes are not in it (~250 draws).
 */

import './dom.js';
import { readFileSync } from 'node:fs';
import * as THREE from 'three';
import RAPIER from '@dimforge/rapier3d-compat';
import { CSM } from 'three/addons/csm/CSM.js';
import { SHIP_SPAWNS, SUN } from '../../src/shared/constants.js';
import { SHIP_DEFS, ShipSim, placeShip } from '../../src/shared/ship/sim.js';
import { FLIGHT_IDLE } from '../../src/shared/ship/flight/index.js';
import { groundAltOn, siteToWorld } from '../../src/shared/ship/spawn.js';
import { crewStep } from '../../src/shared/ship/crew.js';
import { MOON_BODY, sunDirection, surfaceOf, type CelestialBody } from '../../src/shared/space/body.js';
import { siteById } from '../../src/shared/space/sites.js';
import { siteGround, spawnPoint } from '../../src/shared/space/world.js';
import { bodyGround } from '../../src/shared/actors/ground.js';
import { encodeServer } from '../../src/shared/wire.js';
import { Npcs } from '../../src/server/npcs.js';
import type { Peer } from '../../src/server/peer.js';
import { ShipView } from '../../src/client/ship/view.js';
import { ShipPhysics } from '../../src/client/ship/physics.js';
import type { Physics } from '../../src/client/world/physics.js';
import { ShadowCull } from '../../src/client/render/shadowCull.js';
import { DETAIL_MATERIALS } from '../../src/client/player/astronaut.js';
import { timeIt } from './measure.js';

const SUBMIT_US = 12;
/** `--before`: every ship keeps its colliders in the lunar world (as before PHYSICS_REACH). */
const FAR_OFF = !process.argv.includes('--before');
const seed = 1969;
const surfaces = (b: CelestialBody) => surfaceOf(b, seed);
const surface = surfaces(MOON_BODY)!;
const site = SHIP_SPAWNS[0].site;
const ground = siteGround(siteById(site)!, surfaces)!;
const kinds = Object.values(SHIP_DEFS);
const sizes = process.argv.slice(2).filter((a) => !a.startsWith('--')).map(Number).filter(Boolean);
await RAPIER.init();

// the suit's primitives (as perf.ts counts them)
const glb = readFileSync(new URL('../../public/assets/astronaut.glb', import.meta.url));
const gj = JSON.parse(glb.subarray(20, 20 + glb.readUInt32LE(12)).toString()) as { nodes: Array<{ mesh?: number }>; meshes: Array<{ primitives: Array<{ material?: number }> }>; materials: Array<{ name: string }> };
const suit: string[] = [];
for (const n of gj.nodes) if (n.mesh !== undefined) for (const pr of gj.meshes[n.mesh].primitives) suit.push(gj.materials[pr.material ?? 0]?.name ?? '');

const spawnAt = spawnPoint(0, surfaces);
const up = new THREE.Vector3(spawnAt[0] - MOON_BODY.center[0], spawnAt[1] - MOON_BODY.center[1], spawnAt[2] - MOON_BODY.center[2]).normalize();
const eye = new THREE.Vector3(...spawnAt).addScaledVector(up, 1.7);

function layout(n: number) {
  const ships: ShipSim[] = [];
  const npcAt: Array<[number, number, number]> = [];
  for (let k = 0; k < n; k++) {
    const def = kinds[k % kinds.length];
    const r = 90 + 1400 * Math.sqrt((k + 0.5) / n);
    const a = k * 2.39996;
    const x = Math.cos(a) * r, z = Math.sin(a) * r;
    const sim = new ShipSim(1000 + k, def, placeShip(def, ground, x, z, a), groundAltOn(surfaces, siteById(site)!.body));
    if (k % 5 === 4) {
      const u = new THREE.Vector3(...sim.pose.p).sub(new THREE.Vector3(...MOON_BODY.center)).normalize().multiplyScalar(50 + (k % 7) * 50);
      sim.pose.p[0] += u.x;
      sim.pose.p[1] += u.y;
      sim.pose.p[2] += u.z;
      sim.landed = false;
      sim.flight.wake();
    }
    ships.push(sim);
    const p = siteToWorld(surfaces, site, x + 18, z + 6)!;
    npcAt.push([p[0], p[1], p[2]]);
  }
  return { ships, npcAt };
}

function client(ships: ShipSim[], npcAt: Array<[number, number, number]>) {
  const scene = new THREE.Scene();
  const views = ships.map((sim) => {
    const v = new ShipView(sim, null, surfaces);
    scene.add(v.root);
    return { v, sim, anim: { movers: Object.fromEntries(sim.def.movers.map((m) => [m.key, sim.mover(m.key)])) }, feet: sim.flight.feetAnywhere(sim.pose, surface, sim.flight.gear()) };
  });
  const bare = new THREE.MeshBasicMaterial();
  const g = new THREE.BufferGeometry();
  g.boundingSphere = new THREE.Sphere(new THREE.Vector3(), 1.2);
  for (const p of npcAt) {
    const near = eye.distanceTo(new THREE.Vector3(...p)) < 12;
    for (const name of suit) {
      if (name === 'HelmetInner' || (!near && DETAIL_MATERIALS.has(name))) continue;
      const m = new THREE.Mesh(g, bare);
      m.position.set(p[0], p[1] + 0.9, p[2]);
      m.castShadow = !DETAIL_MATERIALS.has(name);
      scene.add(m);
    }
  }
  let time = 0;
  const update = () => {
    time += 1 / 60;
    for (const s of views) s.v.update(1 / 60, time, s.anim, s.sim.pose, s.feet, eye);
  };
  // the first frames bake the far ships (a couple a frame): their worst frame
  let bakeMs = 0;
  for (let i = 0; i < ships.length; i++) {
    const t0 = performance.now();
    update();
    bakeMs = Math.max(bakeMs, performance.now() - t0);
  }
  const updateMs = timeIt(update, 60, 10);
  scene.updateMatrixWorld(true);
  const matrixMs = timeIt(() => scene.updateMatrixWorld(), 60, 10);
  // draws over 8 headings
  const cam = new THREE.PerspectiveCamera(72, 16 / 9, 0.05, 60000);
  cam.position.copy(eye);
  cam.up.copy(up);
  const sd = sunDirection(SUN.az, SUN.el);
  const csm = new CSM({ camera: cam, parent: scene, cascades: 1, maxFar: 90, mode: 'practical', shadowMapSize: 2048, lightDirection: new THREE.Vector3(-sd[0], -sd[1], -sd[2]), lightNear: 1, lightFar: 900, lightMargin: 160 });
  const cull = new ShadowCull(scene, cam, () => csm.lightDirection);
  let main = 0, shadow = 0, tris = 0;
  const H = 8;
  for (let h = 0; h < H; h++) {
    cam.lookAt(eye.clone().add(new THREE.Vector3(1, 0, 0).projectOnPlane(up).normalize().applyAxisAngle(up, (h / H) * Math.PI * 2)));
    cam.updateMatrixWorld(true);
    csm.updateFrustums();
    csm.update();
    scene.updateMatrixWorld(true);
    cull.before();
    const view = new THREE.Frustum().setFromProjectionMatrix(new THREE.Matrix4().multiplyMatrices(cam.projectionMatrix, cam.matrixWorldInverse));
    const sc = csm.lights[0].shadow.camera;
    sc.updateMatrixWorld(true);
    const box = new THREE.Frustum().setFromProjectionMatrix(new THREE.Matrix4().multiplyMatrices(sc.projectionMatrix, sc.matrixWorldInverse));
    scene.traverseVisible((o) => {
      const m = o as THREE.Mesh;
      if (!m.isMesh || !m.geometry) return;
      const multi = Array.isArray(m.material);
      const groups = multi ? Math.max(1, m.geometry.groups.length) : (m.material as THREE.Material).visible ? 1 : 0;
      if (!groups) return;
      if (!m.geometry.boundingSphere) m.geometry.computeBoundingSphere();
      if (!m.frustumCulled || view.intersectsObject(m)) {
        main += groups;
        const idx = m.geometry.index;
        tris += (idx ? idx.count : (m.geometry.attributes.position?.count ?? 0)) / 3;
      }
      if (m.castShadow && (!m.frustumCulled || box.intersectsObject(m))) shadow += groups;
    });
    cull.after();
  }
  // the lunar world: every ship's outer colliders (and its interior world, not stepped)
  const world = new RAPIER.World({ x: 0, y: 0, z: 0 });
  const phys = { rapier: RAPIER, world } as unknown as Physics;
  const bodies = ships.map((sim) => new ShipPhysics(phys, new RAPIER.World({ x: 0, y: -1.62, z: 0 }), sim, sim.pose));
  const reach = views.map((s) => s.v.bounds.radius + 400);
  const active = (i: number) => !FAR_OFF || eye.distanceTo(new THREE.Vector3(...ships[i].pose.p)) < reach[i];
  world.step();
  let k = 0;
  const physMs = timeIt(() => {
    k++;
    for (let i = 0; i < ships.length; i++) {
      const s = ships[i];
      if (!s.landed) s.pose.p[0] += 0.05 * Math.sin(k * 0.1);
      bodies[i].follow(s.pose, active(i));
    }
    world.step();
  }, 60, 10);
  return { draws: Math.round(main / H), shadow: Math.round(shadow / H), tris: Math.round(tris / H), updateMs, matrixMs, physMs, bakeMs };
}

function server(ships: ShipSim[], npcAt: Array<[number, number, number]>) {
  const tickMs = timeIt(() => void crewStep(ships, [], 0.05, (s, ctx) => s.tick(0.05, ctx)), 40, 10);
  // the room's sim LOD: parked, nobody within 500 m: it rests
  const flightMs = timeIt(() => {
    for (const s of ships) {
      // the ones in the air have pilots: their clients fly them, the server relays their poses
      if (!s.landed) continue;
      const env = { surface };
      if (FAR_OFF && !s.flight.sleeping && s.landed && Math.hypot(...s.pose.v) < 0.5 && Math.hypot(s.pose.p[0] - spawnAt[0], s.pose.p[1] - spawnAt[1], s.pose.p[2] - spawnAt[2]) > 500) s.flight.settle(env);
      s.flight.step(1 / 60, FLIGHT_IDLE, env);
    }
  }, 120, 20);
  // one player at the spawn, counting what it gets
  let bytes = 0, msgs = 0;
  const me: Peer = { id: 1, dead: false, at: [spawnAt[0], spawnAt[1], spawnAt[2]], fr: 0, send: (d) => void ((bytes += typeof d === 'string' ? d.length : d.byteLength), msgs++) };
  const walkGround = bodyGround(surfaces);
  const npcs = new Npcs({ ground: () => walkGround, obstacles: (p, r) => ships.filter((s) => Math.hypot(s.pose.p[0] - p[0], s.pose.p[1] - p[1], s.pose.p[2] - p[2]) < r + 20).map((s) => ({ c: [s.pose.p[0], s.pose.p[1], s.pose.p[2]] as [number, number, number], r: 15 })) }, () => [me]);
  npcAt.forEach((p, i) => npcs.spawn({ id: 5000 + i, name: `n${i}`, variant: i % 4, p, task: { kind: 'wander', c: p, r: 25 } }, 0));
  npcs.interest();
  let t = 0;
  const npcMs = timeIt(() => npcs.step(0.05, (t += 50)), 40, 10);
  bytes = msgs = 0;
  for (let i = 0; i < 20; i++) npcs.broadcast((t += 50));
  const npcBytes = bytes, npcMsgs = msgs;
  // ship poses, as the room sends them (near < 3 km: awake 30/s, asleep 1/s; far: 2/s, 1/s)
  let poseBytes = 0, poseMsgs = 0;
  for (const s of ships) {
    const p = s.pose;
    const b = (encodeServer({ type: 'shipPose', ship: s.id, t: 1e6, p: p.p, q: p.q, v: p.v, w: p.w, landed: s.landed, pad: false }) as ArrayBuffer).byteLength;
    const near = Math.hypot(p.p[0] - spawnAt[0], p.p[1] - spawnAt[1], p.p[2] - spawnAt[2]) < 3000;
    const rate = s.flight.sleeping ? 1 : near ? 30 : 2;
    poseBytes += b * rate;
    poseMsgs += rate;
  }
  const cpu = tickMs * 20 + flightMs * 60 + npcMs * 20;
  return { tickMs, flightMs, npcMs, cpuShare: cpu / 1000, kbps: ((npcBytes + poseBytes) * 8) / 1000, msgs: npcMsgs + poseMsgs, awake: ships.filter((s) => !s.flight.sleeping).length };
}

const f = (n: number, d = 2) => n.toFixed(d);
console.log('| N naves + N NPC | draws (princ.+sombra) | triáng. vista | CPU cliente ms (vistas · matrices · física · envío≈) | peor frame horneando | CPU servidor (tick · vuelo · NPC ms) | % de un núcleo | red al jugador |');
console.log('|---|---|---|---|---|---|---|---|');
for (const n of sizes.length ? sizes : [10, 30, 60, 100]) {
  const { ships, npcAt } = layout(n);
  // 30 s parked: the ships settle and go to sleep
  for (let k = 0; k < 1800; k++) for (const s of ships) s.flight.step(1 / 60, FLIGHT_IDLE, { surface });
  const c = client(ships, npcAt);
  const s = server(ships, npcAt);
  const submit = ((c.draws + c.shadow) * SUBMIT_US) / 1000;
  const total = c.updateMs + c.matrixMs + c.physMs + submit;
  console.log(`| ${n} (${s.awake} despiertas) | ${c.draws} + ${c.shadow} | ${Math.round(c.tris / 1000)} K | ${f(total, 1)} = ${f(c.updateMs)} · ${f(c.matrixMs)} · ${f(c.physMs)} · ${f(submit, 1)} | ${f(c.bakeMs, 1)} ms | ${f(s.tickMs)} · ${f(s.flightMs)} · ${f(s.npcMs)} | ${f(s.cpuShare * 100, 0)} % | ${f(s.kbps, 0)} kbit/s, ${s.msgs} msg/s |`);
}
process.exit(0);
