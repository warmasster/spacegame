// Code-only regression checks. No browser, game server, WebGL context or gameplay automation.
import assert from 'node:assert/strict';
import * as THREE from 'three';
import { cameraBudget, cameraFov } from '../../src/shared/screens.js';
import { startShips } from '../../src/shared/ship/spawn.js';
import { surfaceOf } from '../../src/shared/space/body.js';
import { checkShip } from '../../src/shared/ship/def.js';
import { PROJECTILES } from '../../src/shared/items/index.js';
import { CameraSources } from '../../src/client/render/cameraSources.js';
import { ConsoleCommands } from '../../src/client/ship/commands.js';
import { Gunnery } from '../../src/client/ship/gunnery.js';
import { PipDisplay, PipSystem, type PipSource } from '../../src/client/render/pip.js';
import { ImageMaterial } from '../../src/client/render/imageEffects.js';
import { ShipCameraFeed, shipCameraSources } from '../../src/client/ship/cameraScreens.js';
import { CameraRig } from '../../src/client/player/cameraRig.js';
import { origin } from '../../src/client/render/origin.js';
import type { ShipClient } from '../../src/client/ship/ship.js';
import type { Input } from '../../src/client/player/input.js';
import type { Astronaut } from '../../src/client/player/astronaut.js';

const canvasContext = { clearRect() {}, fillRect() {}, fillText() {}, beginPath() {}, moveTo() {}, lineTo() {}, stroke() {} };
Object.defineProperty(globalThis, 'document', { configurable: true, value: { createElement() { return { width: 0, height: 0, getContext: () => canvasContext }; } } });
Object.defineProperty(globalThis, 'window', { configurable: true, value: { addEventListener() {} } });

assert.deepEqual(cameraBudget({ source: { kind: 'test', ref: 'a' }, width: Infinity, height: -1, fps: 200, reach: NaN }), { width: 256, height: 36, fps: 15, reach: 10 });
assert.ok(Math.abs(Math.tan(cameraFov(55, 3) * Math.PI / 360) * 3 - Math.tan(55 * Math.PI / 360)) < 1e-12);
const material = new ImageMaterial({}, [{ kind: 'vhs', amount: 0.2 }], 384, 216);
const shader = { uniforms: {}, fragmentShader: '#include <common>\n#include <map_fragment>' } as any;
material.onBeforeCompile(shader, {} as THREE.WebGLRenderer);
material.time(12);
material.active(false);
assert.equal(shader.uniforms.imageActive.value, 0, 'VHS grain must turn off with the image');
material.active(true);
assert.equal(shader.uniforms.imageTime.value, 12);
assert.equal(shader.uniforms.imageSize.value.x, 384);
assert.match(shader.fragmentShader, /imageGrain0/);
assert.doesNotMatch(shader.fragmentShader, /#include <map_fragment>/);
assert.notEqual(material.customProgramCacheKey(), new ImageMaterial().customProgramCacheKey());
assert.throws(() => new ImageMaterial({}, [{ kind: 'absent' }]), /not registered/);
console.log('OK bounded capture budgets, optical zoom, reusable image recipes and shader cache identity');

const basicSource = (): PipSource => ({ camera: new THREE.PerspectiveCamera(60, 1, 0.1, 100), enabled: true, live: true, status: { title: 'TEST', detail: '', hint: '', locked: false, warning: false }, prepare() {} });
const providers = new CameraSources<number>();
providers.register('test', context => { const source = basicSource(); source.status.title = String(context); return source; });
assert.equal(providers.create('test', 23).status.title, '23');
assert.throws(() => providers.register('test', basicSource), /already registered/);
assert.throws(() => providers.create('absent', 1), /not registered/);
const commands = new ConsoleCommands<{ seated: boolean }>();
commands.register('test', host => host.seated ? null : 'seat required');
assert.equal(commands.execute({ seated: false }, { namespace: 'test', action: 'a', target: 'b' }), 'seat required');
assert.equal(commands.execute({ seated: true }, { namespace: 'test', action: 'a', target: 'b' }), null);
assert.throws(() => commands.register('test', () => null), /already registered/);
console.log('OK providers and command namespaces work with arbitrary host contexts');

const ships = startShips(b => surfaceOf(b, 1969));
for (const sim of ships) assert.deepEqual(checkShip(sim.def), []);
const sim = ships.find(s => s.def.mounts.length)!;
const mount = sim.mounts!.list[0], seat = sim.def.seats.findIndex(s => s.mounts?.includes(mount.def.id));
const screenDef = sim.def.screens.find(s => s.camera?.source.ref === mount.def.id)!;
const root = new THREE.Group();
const host = {
  sim, view: { root, mountLead: [null], seatOccupied: sim.def.seats.map(() => false) },
  world(p: [number, number, number], out = new THREE.Vector3()) { return out.set(...sim.toWorld(p)); },
  local(p: THREE.Vector3, out = new THREE.Vector3()) { return out.set(...sim.toLocal(p.toArray() as [number, number, number])); },
} as unknown as ShipClient;
root.quaternion.set(...sim.pose.q);
let fireClock = 10, shots = 0, focused = false, refusals = '';
const gun = new Gunnery({
  aim: (_, i, yaw, pitch) => sim.mounts!.aim(i, yaw, pitch),
  fire: (_, i) => { if (sim.mounts!.tryFire(sim.st, i, fireClock) < 0) return false; shots++; return true; },
  pick: (_eye, _dir, out) => { host.world([10, 3, -150], out); return true; },
  refused: text => { refusals = text; },
  focus: (_, __, on) => { focused = on; },
});
assert.match(gun.command(host, mount.def.id, 'fire')!, /asiento/);
assert.equal(gun.take(host, seat), true);
assert.match(gun.displayStatus(1)!.detail, /APAGADA/);
assert.equal(gun.trigger(1, true), false);
assert.match(refusals, /APAGADA/);
sim.sw[mount.key] = 1;
for (let i = 0; i < 20; i++) sim.tick(0.05);
assert.equal(sim.st[mount.iReady], 1);
assert.equal(gun.command(host, mount.def.id, 'aim'), null);
assert.equal(focused, true);
const input = { look(out: [number, number]) { out[0] = -0.3; out[1] = -0.1; return out; } } as Input;
assert.equal(gun.look(input), true);
gun.frame(2, new THREE.Vector3(), new THREE.Vector3(0, 0, -1));
for (let i = 0; i < 60; i++) { sim.tick(1 / 60); gun.fixed(1 / 60); }
assert.ok(Math.abs(sim.st[mount.iYaw] - 0.3) < 0.01, 'mouse aim reaches the authority');
for (const zoom of [1.5, 3, 1]) { gun.command(host, mount.def.id, 'zoom'); assert.equal(gun.zoom, zoom); }
const ammo = sim.st[mount.iAmmo];
assert.equal(gun.trigger(10), true);
assert.equal(shots, 1);
assert.equal(sim.st[mount.iAmmo], ammo - 1);
assert.equal(gun.trigger(10.01), false);
const pressed = new Set(['KeyT']);
const heldKeys = { consume(key: string) { return pressed.delete(key); }, down(key: string) { return key === 'KeyT'; } } as Input;
fireClock = 11; gun.keys(heldKeys, 11);
assert.equal(shots, 2, 'T must fire even with no helmet control target');
fireClock = 11.01; gun.keys(heldKeys, 11.01); assert.equal(shots, 2);
fireClock = 11.5; gun.keys(heldKeys, 11.5); assert.equal(shots, 3, 'held T repeats at the catalog cadence');
assert.equal(gun.command(host, mount.def.id, 'lock'), null);
gun.frame(3, new THREE.Vector3(100, 0, 0), new THREE.Vector3(1, 0, 0));
const targetYaw = mount.target.yaw;
gun.frame(4, new THREE.Vector3(-100, 0, 0), new THREE.Vector3(-1, 0, 0));
assert.equal(mount.target.yaw, targetYaw, 'fixed point ignores helmet look');
assert.equal(gun.displayStatus(4)!.locked, true);
gun.command(host, mount.def.id, 'center');
gun.frame(5, new THREE.Vector3(), new THREE.Vector3(1, 0, 0));
assert.equal(mount.target.yaw, 0);
assert.equal(gun.displayStatus(5)!.locked, false);
assert.match(sim.interact(sim.def.controls.find(c => c.command)!.index).reason!, /Mando del puesto/);
gun.leave();
assert.equal(focused, false);
assert.equal(gun.displayStatus(6), null);
assert.equal(host.view.mountLead[0], null);
assert.ok(PROJECTILES.minimissile.impact.hull.damage < 30 && PROJECTILES.minimissile.impact.terrain!.radius < 1.5);
console.log('OK seat permissions, refusal reasons, camera aim, zoom, lock, cadence, ammo and station cleanup');

const display = new PipDisplay(screenDef.w, screenDef.h, screenDef.camera!);
const screen = { def: screenDef, display, seat };
const context = { ship: host, screen, gunnery: gun };
const feed = new ShipCameraFeed(context, shipCameraSources().create('mount', context));
feed.update(); assert.equal(feed.enabled, false);
host.view.seatOccupied[seat] = true;
feed.update(); assert.equal(feed.enabled, true); assert.equal(feed.live, true);
feed.prepare();
const actual = feed.camera.getWorldPosition(new THREE.Vector3());
assert.ok(actual.distanceTo(new THREE.Vector3(...sim.pose.p)) < 30);
const offset = [1000000, -2000000, 3000000] as const;
origin.set(...offset);
feed.prepare();
assert.ok(origin.toWorld(feed.camera.getWorldPosition(new THREE.Vector3())).distanceTo(actual) < 1e-7, 'floating-origin rebasing must not move the camera');
sim.sw[sim.sys.breakerOf(screenDef.circuit)!] = 0;
sim.tick(0.05); feed.update(); assert.equal(feed.enabled, false);
sim.sw[sim.sys.breakerOf(screenDef.circuit)!] = 1;
sim.tick(0.05); feed.update();
host.view.seatOccupied[seat] = false; feed.update(); assert.equal(feed.enabled, false);
feed.dispose(); display.dispose(); origin.set(0, 0, 0);
console.log('OK unoccupied seats and power loss turn camera feeds off; floating origin remains exact');

class RendererStub {
  target: THREE.WebGLRenderTarget | null = null;
  viewport = new THREE.Vector4(11, 12, 640, 360);
  scissor = new THREE.Vector4(5, 6, 620, 340);
  scissorTest = true;
  autoClear = false;
  shadowMap = { autoUpdate: true, needsUpdate: true };
  toneMapping = THREE.NoToneMapping;
  renders = 0;
  onRender = () => {};
  getRenderTarget() { return this.target; }
  getActiveCubeFace() { return 0; }
  getActiveMipmapLevel() { return 0; }
  getViewport(out: THREE.Vector4) { return out.copy(this.viewport); }
  getScissor(out: THREE.Vector4) { return out.copy(this.scissor); }
  getScissorTest() { return this.scissorTest; }
  setRenderTarget(target: THREE.WebGLRenderTarget | null) { this.target = target; }
  setViewport(value: THREE.Vector4) { this.viewport.copy(value); }
  setScissor(value: THREE.Vector4) { this.scissor.copy(value); }
  setScissorTest(value: boolean) { this.scissorTest = value; }
  render() { this.renders++; assert.equal(this.shadowMap.autoUpdate, false); assert.equal(this.shadowMap.needsUpdate, false); this.onRender(); }
}
const renderer = new RendererStub(), scene = new THREE.Scene(), observer = new THREE.PerspectiveCamera(60, 1, 0.1, 100);
let begun = 0, ended = 0;
const pip = new PipSystem(renderer as unknown as THREE.WebGLRenderer, scene, { begin() { begun++; }, end() { ended++; } });
for (let i = 0; i < 4; i++) {
  const d = new PipDisplay(0.4, 0.3, { source: { kind: 'test', ref: String(i) } });
  d.root.position.set((i - 1.5) * 0.2, 0, -2);
  d.source = basicSource(); scene.add(d.root); pip.add(d);
}
renderer.onRender = () => { for (const d of pip.displays) assert.equal(d.root.visible, false, 'prevent recursive captures'); };
for (let i = 0; i < 4; i++) { pip.frame(i / 60, observer); assert.equal(renderer.renders, i + 1, 'at most one capture each frame'); }
assert.equal(new Set(pip.displays.map(d => d.surface.material.map)).size, 4);
pip.frame(0.055, observer); assert.equal(renderer.renders, 4, 'respect per-source cadence');
assert.equal(renderer.target, null); assert.equal(renderer.autoClear, false); assert.equal(renderer.scissorTest, true);
assert.equal(renderer.shadowMap.needsUpdate, true); assert.equal(renderer.shadowMap.autoUpdate, true);
assert.deepEqual(renderer.viewport.toArray(), [11, 12, 640, 360]);
assert.deepEqual(renderer.scissor.toArray(), [5, 6, 620, 340]);
for (const d of pip.displays) d.source!.enabled = false;
pip.frame(1, observer); assert.equal(renderer.renders, 4);
for (const d of pip.displays) assert.equal(d.surface.material.color.r, 0, 'off means black');
const first = pip.displays[0]; first.source!.enabled = true;
first.root.rotation.y = Math.PI;
pip.frame(2, observer); assert.equal(renderer.renders, 4, 'back-facing screens do not capture');
first.root.rotation.y = 0; first.root.position.z = -30;
pip.frame(3, observer); assert.equal(renderer.renders, 4, 'distant screens do not capture');
first.root.position.z = -2; first.root.visible = false;
pip.frame(4, observer); assert.equal(renderer.renders, 4, 'hidden screens do not capture');
first.root.visible = true;
renderer.onRender = () => { throw new Error('test render failure'); };
assert.throws(() => pip.frame(5, observer), /test render failure/);
assert.equal(first.root.visible, true); assert.equal(renderer.target, null); assert.equal(renderer.autoClear, false);
assert.equal(renderer.shadowMap.autoUpdate, true); assert.equal(renderer.scissorTest, true); assert.equal(begun, ended);
pip.dispose(); material.dispose();
console.log('OK capture cadence, visibility, bounded targets, no recursion/shadow pass and error-state restoration');

const camera = new THREE.PerspectiveCamera(), rig = new CameraRig(camera, () => 10), anchor = new THREE.Object3D();
origin.root.add(camera, anchor); origin.set(1000000, 0, -2000000);
camera.position.set(1000000, 1.3, -2000000);
anchor.position.set(1000000.2, 1.5, -2000001.2);
anchor.updateMatrixWorld(true);
rig.focus = anchor; rig.mode = 'third';
rig.update(1 / 30, { feet: new THREE.Vector3(1000000, 0, -2000000), frame: new THREE.Quaternion(), yaw: 0, pitch: 0, crouch: false }, { eyePosition(out: THREE.Vector3) { return out.copy(camera.position); } } as unknown as Astronaut);
assert.equal(rig.mode, 'first');
const direction = anchor.getWorldPosition(new THREE.Vector3()).sub(camera.getWorldPosition(new THREE.Vector3())).normalize();
assert.ok(camera.getWorldDirection(new THREE.Vector3()).dot(direction) > 0.999999, 'focus must align to the screen center after origin shift');
camera.removeFromParent(); anchor.removeFromParent(); origin.set(0, 0, 0);
console.log('OK generic gaze focus centers the observer on any object');
