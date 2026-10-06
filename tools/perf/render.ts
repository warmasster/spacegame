// The render's automatic quality (npm run test:render), headless: the GPU detection that picks the
// default profile, and the governor that keeps the frame rate up (src/client/render/governor.ts),
// against a model of a frame: the GPU's time grows with the pixels (scale²), MSAA and bloom; the
// display waits for vsync (16.7 ms steps); the CPU's time is what it is.
//
//   detection   integrated (Intel UHD / Iris Xe, AMD APUs, Apple, phones) and software GPUs get the
//               low profile; discrete ones (NVIDIA, AMD RX, Intel Arc) the high one
//   GPU-bound   a slow GPU: it steps the image down until the frame fits, and stays there
//   CPU-bound   a slow CPU: it changes nothing (fewer pixels wouldn't help)
//   fast        a fast machine: nothing changes
//   no timer    without the GPU's timer: it still steps down, tries going back up, and backs off
//               (the tries don't turn into a see-saw)
//   weapons     merged into one mesh per material, same volume, same shadow
//   shadows     casters whose shadow can't land in the view leave the shadow pass (and come back)
//
// Exit code 1 if anything fails. `--verbose` for the numbers.

import './dom.js';
import { readFileSync } from 'node:fs';
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { Astronaut, AstronautAsset } from '../../src/client/player/astronaut.js';
import { classifyGpu, suggestedQuality } from '../../src/client/render/gpuTier.js';
import { GOVERNOR, NOTCHES, QualityGovernor, type Notch } from '../../src/client/render/governor.js';
import { ShadowCull } from '../../src/client/render/shadowCull.js';
import { WEAPON_LOOKS, WEAPON_ORDER } from '../../src/client/fx/weapons.js';
import { SpikeLog } from '../../src/engine/spikes.js';

const verbose = process.argv.includes('--verbose');
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}

// --- detection ---------------------------------------------------------------------------------------
{
  const cases: Array<[string, 'high' | 'low']> = [
    ['ANGLE (Intel, Intel(R) UHD Graphics 620 Direct3D11 vs_5_0 ps_5_0, D3D11)', 'low'],
    ['ANGLE (Intel, Intel(R) Iris(R) Xe Graphics (0x0000A7A0) Direct3D11 vs_5_0 ps_5_0, D3D11)', 'low'],
    ['ANGLE (Intel, Intel(R) Graphics (0x00007D55) Direct3D11 vs_5_0 ps_5_0, D3D11)', 'low'],
    ['ANGLE (AMD, AMD Radeon(TM) Graphics (0x00001681) Direct3D11 vs_5_0 ps_5_0, D3D11)', 'low'],
    ['ANGLE (AMD, AMD Radeon(TM) Vega 8 Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)', 'low'],
    ['ANGLE (Apple, ANGLE Metal Renderer: Apple M1, Unspecified Version)', 'low'],
    ['ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero) (0x0000C0DE)), SwiftShader driver)', 'low'],
    ['ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Laptop GPU Direct3D11 vs_5_0 ps_5_0, D3D11)', 'high'],
    ['ANGLE (AMD, AMD Radeon RX 6700 XT Direct3D11 vs_5_0 ps_5_0, D3D11)', 'high'],
    ['ANGLE (Intel, Intel(R) Arc(TM) A770 Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)', 'high'],
  ];
  const wrong = cases.filter(([r, q]) => suggestedQuality(classifyGpu(r), 12) !== q).map(([r]) => r);
  const fewCores = suggestedQuality(classifyGpu('ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 Direct3D11)'), 4);
  check('detección: integradas (Intel UHD/Iris/Graphics, AMD APU, Apple) y por software → BAJA; dedicadas (NVIDIA, AMD RX, Intel Arc) → ALTA; pocos núcleos → BAJA', wrong.length === 0 && fewCores === 'low', { wrong });
}

// --- the governor against a model of a frame ------------------------------------------------------
interface Machine {
  /** GPU ms at full resolution, no MSAA, no bloom. */
  gpu: number;
  cpu: number;
  timer: boolean;
}
const VSYNC = 1000 / 60;
function gpuTime(m: Machine, n: Notch) {
  return m.gpu * n.scale * n.scale * (n.msaa ? 1.4 : 1) + (n.bloom ? 1.2 : 0);
}
function run(m: Machine, seconds: number, start = 0) {
  const g = new QualityGovernor({ start });
  let t = 0;
  const changes: Array<{ t: number; notch: number }> = [];
  let lastFrame = 0;
  while (t < seconds * 1000) {
    const gpu = gpuTime(m, g.current);
    // the frame: the slower of CPU and GPU, rounded up to the display's refresh
    const frame = Math.ceil(Math.max(m.cpu, gpu) / VSYNC - 1e-9) * VSYNC;
    lastFrame = frame;
    t += frame;
    if (g.frame(frame, m.cpu, m.timer ? gpu : null)) changes.push({ t: t / 1000, notch: g.notch });
  }
  return { notch: g.notch, changes, lastFrame, fits: gpuTime(m, g.current) <= VSYNC && m.cpu <= VSYNC };
}

{
  // an integrated GPU that needs about twice the time it has at full quality
  const slow = run({ gpu: 26, cpu: 9, timer: true }, 120);
  const late = slow.changes.filter((c) => c.t > 60);
  check('limitado por la GPU: baja la imagen escalón a escalón hasta que el frame cabe en 16,7 ms, y se queda ahí', slow.fits && slow.notch > 0 && late.length === 0 && slow.changes.every((c) => c.t >= GOVERNOR.warmup), { notch: slow.notch, state: NOTCHES[slow.notch], changes: slow.changes });
  const cpu = run({ gpu: 8, cpu: 34, timer: true }, 120);
  const cpuEstimate = run({ gpu: 8, cpu: 34, timer: false }, 120);
  check('limitado por la CPU: no toca la imagen (con y sin temporizador de GPU)', cpu.changes.length === 0 && cpuEstimate.changes.length === 0, { changes: cpu.changes.length + cpuEstimate.changes.length });
  const fast = run({ gpu: 6, cpu: 5, timer: true }, 120);
  const fastEstimate = run({ gpu: 6, cpu: 5, timer: false }, 120);
  check('equipo rápido: nada cambia', fast.changes.length === 0 && fastEstimate.changes.length === 0 && fast.notch === 0, {});
  // without a timer: it estimates, steps down; going back up is tried, and backed off when it fails
  const blind = run({ gpu: 26, cpu: 9, timer: false }, 600);
  const lastMinutes = blind.changes.filter((c) => c.t > 300).length;
  check('sin temporizador de GPU: baja hasta que cabe; los intentos de subir se espacian cada vez más (nada de vaivén)', blind.notch > 0 && blind.changes.length < 20 && lastMinutes <= 3, { notch: blind.notch, changes: blind.changes.length, lastMinutes, list: blind.changes.slice(0, 12) });
  // room again (a lighter scene): it goes back up
  const g = new QualityGovernor({ start: 4 });
  let t = 0, notch = 4;
  while (t < 120_000) {
    const gpu = 4 * g.current.scale ** 2;
    t += VSYNC;
    g.frame(VSYNC, 5, gpu);
    notch = g.notch;
  }
  check('con margen otra vez, vuelve a subir hasta arriba', notch === 0, { notch });
}

// --- hitches ---------------------------------------------------------------------------------------
{
  const log = new SpikeLog(5);
  const calm = (): Iterable<[string, number]> => [['physics', 2], ['terrain', 1]];
  for (let i = 0; i < 60; i++) log.feed(16.7, 6, calm);
  log.feed(48, 33, () => [['terrain', 22], ['physics', 3], ['render', 5], ['audio', 0.2]]);
  for (let i = 0; i < 60; i++) log.feed(16.7, 6, calm);
  const seen = log.spike;
  const line = log.describe();
  for (let i = 0; i < 400; i++) log.feed(16.7, 6, calm);
  const later = log.spike;
  check('picos: el peor frame reciente con lo que tardó (lo más lento primero, sin migajas) y olvidado pasada su ventana', !!seen && seen.ms === 48 && seen.parts[0][0] === 'terrain' && seen.parts.length === 3 && /terrain 22/.test(line) && !!later && later.ms < 20, { line, later: later?.ms });
}

// --- weapons merged by material ------------------------------------------------------------------
{
  const meshes = (o: THREE.Object3D) => {
    const out: THREE.Mesh[] = [];
    o.traverse((c) => void ((c as THREE.Mesh).isMesh && out.push(c as THREE.Mesh)));
    return out;
  };
  const box = (o: THREE.Object3D) => {
    o.updateMatrixWorld(true);
    return new THREE.Box3().setFromObject(o);
  };
  const rows: Record<string, string> = {};
  let ok = true;
  for (const w of WEAPON_ORDER) {
    const raw = WEAPON_LOOKS[w.id].build();
    const merged = w.build();
    const before = meshes(raw), after = meshes(merged);
    const mats = new Set(before.map((m) => m.material));
    const a = box(raw), b = box(merged);
    const same = a.min.distanceTo(b.min) < 1e-5 && a.max.distanceTo(b.max) < 1e-5;
    const shadow = before.some((m) => m.castShadow) === after.some((m) => m.castShadow);
    rows[w.id] = `${before.length} → ${after.length}`;
    ok &&= after.length === mats.size && after.length < before.length && same && shadow;
  }
  check('armas: una pieza por material, mismo volumen y misma sombra', ok, rows);
}

// --- shadow casters that cast into nothing we see ------------------------------------------------
{
  const scene = new THREE.Scene();
  const cam = new THREE.PerspectiveCamera(70, 16 / 9, 0.05, 5000);
  cam.position.set(0, 1.7, 0);
  const unit = new THREE.BoxGeometry(1, 1, 1);
  const at = (x: number, z: number) => {
    const m = new THREE.Mesh(unit, new THREE.MeshBasicMaterial());
    m.position.set(x, 0.5, z);
    m.castShadow = true;
    scene.add(m);
    return m;
  };
  // a low sun from the right (the camera looks down -z)
  const light = new THREE.Vector3(-1, -0.3, 0).normalize();
  const behind = at(0, 40); // behind the camera: its shadow stays behind
  const ahead = at(0, -20); // in view
  const reaches = at(100, -40); // off to the right, out of view, but its shadow crosses the view
  scene.updateMatrixWorld(true);
  const cull = new ShadowCull(scene, cam, () => light);
  const flags = () => [behind, ahead, reaches].map((m) => m.castShadow).join();
  cam.lookAt(0, 1.7, -1);
  cull.before();
  const forward = flags();
  cull.after();
  cam.up.set(0, 0, -1);
  cam.lookAt(0, 100, 0); // straight up at the sky
  cull.before();
  const up = flags();
  cull.after();
  const restored = flags();
  check('recorte de sombras: fuera lo que no puede caer en la vista (mirando al frente y al cielo), y de vuelta después', forward === 'false,true,true' && up === 'false,false,false' && restored === 'true,true,true', { forward, up, restored });
}

// --- per cascade: each caster only into the cascades its shadow reaches ---------------------------
{
  const scene = new THREE.Scene();
  const cam = new THREE.PerspectiveCamera(70, 16 / 9, 0.05, 5000);
  cam.position.set(0, 1.7, 0);
  cam.lookAt(0, 1.7, -1);
  const unit = new THREE.BoxGeometry(1, 1, 1);
  const at = (z: number) => {
    const m = new THREE.Mesh(unit, new THREE.MeshBasicMaterial());
    m.position.set(0, 0.5, z);
    m.castShadow = true;
    scene.add(m);
    return m;
  };
  const near = at(-10), mid = at(-50), far = at(-150);
  scene.updateMatrixWorld(true);
  const lights = [0, 1, 2].map(() => new THREE.DirectionalLight());
  const cull = new ShadowCull(scene, cam, () => new THREE.Vector3(0.2, -1, 0).normalize());
  cull.useCascades({ lights, breaks: [0.17, 0.36, 1], maxFar: 220 });
  cull.before();
  const which = (m: THREE.Object3D) => lights.map((l) => (m.layers.test(l.shadow.camera.layers) ? 1 : 0)).join('');
  const got = [near, mid, far].map(which);
  cull.after();
  check('sombras por cascada: lo cercano solo en la primera, lo medio en la segunda, lo lejano en la última', got.join() === '100,010,001' && cull.drawn === 3, { got, drawn: cull.drawn });
}

// --- the far suit: past LOD 0 an astronaut is a few merged meshes ---------------------------------
{
  (globalThis as unknown as { self: unknown }).self = globalThis;
  (globalThis as unknown as { createImageBitmap: unknown }).createImageBitmap = () => undefined;
  // no images in Node: the loader gets a blank bitmap
  THREE.ImageBitmapLoader.prototype.load = function (_u: string, onLoad?: (b: ImageBitmap) => void) {
    const b = { width: 1, height: 1, close() {} } as unknown as ImageBitmap;
    setTimeout(() => onLoad?.(b));
    return b;
  };
  const buf = readFileSync(new URL('../../public/assets/astronaut.glb', import.meta.url));
  const gltf = await new GLTFLoader().parseAsync(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength), '');
  const a = new Astronaut(AstronautAsset.fromScene(gltf.scene, null));
  const count = (lod: number) => {
    a.setLod(lod);
    let draws = 0, casters = 0;
    a.root.traverseVisible((o) => void ((o as THREE.Mesh).isMesh && (draws++, (o as THREE.Mesh).castShadow && casters++)));
    return { draws, casters };
  };
  const near = count(0), far = count(1), further = count(2), back = count(0);
  check('astronauta lejano: pocas mallas fundidas (vista y sombra), y de vuelta al detalle', far.draws <= 5 && far.casters <= 5 && further.draws === far.draws && back.draws === near.draws && near.draws > 12, { near, far, further, back });
}

console.log(failures ? `\n${failures} fallo(s)` : '\nCalidad automática en orden.');
process.exit(failures ? 1 : 0);
