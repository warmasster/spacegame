// Regression checks at module boundaries: equipment identity on the wire and mount authority.
import assert from 'node:assert/strict';
import { PROJECTILES, WEAPON_DEFS, mountMuzzle, terrainImpact, defineProjectile } from '../../src/shared/items/index.js';
import { StateFlags, type PlayerState } from '../../src/shared/protocol.js';
import { decodeClient, decodeServer, encodeClient, encodeServer } from '../../src/shared/wire.js';
import { startShips } from '../../src/shared/ship/spawn.js';
import { VarSync } from '../../src/shared/ship/state.js';
import { MOON_BODY, surfaceOf } from '../../src/shared/space/body.js';
import { shipHost, launch, stepBallistic } from '../../src/shared/frames/index.js';
import type { BallisticEnv } from '../../src/shared/frames/index.js';
import { blastCrater, modTouches, TerrainMods } from '../../src/shared/space/terrainMods/index.js';

const names = [undefined, ...Object.keys(WEAPON_DEFS), 'tools/custom/修理-ñ'];
const states: PlayerState[] = names.map((w, i) => ({ p: [1e7 + i, -2.5, 3], v: [1, 2, 3], yaw: 0.5, pitch: -0.25, f: StateFlags.Armed | StateFlags.Welding, fr: i + 1, ...(w ? { w } : {}) }));
for (const s of states) {
  // the time of the step it belongs to travels with it (float64: 1 ms is 1.6 m in orbit)
  const t = 1.7e9 + 0.125;
  const data = encodeClient({ type: 'state', t, s })!;
  const result = decodeClient(new DataView(data));
  assert.deepEqual(result, { type: 'state', t, s });
  // DataView can point into a Node/WebSocket buffer with an unrelated prefix.
  const padded = new Uint8Array(data.byteLength + 11);
  padded.set(new Uint8Array(data), 7);
  assert.deepEqual(decodeClient(new DataView(padded.buffer, 7, data.byteLength)), result);
  assert.equal(decodeClient(new DataView(data, 0, data.byteLength - 1)), null);
}
const msg = { type: 'snapshot' as const, t: 42, states: states.map((s, i) => ({ id: i + 1, t: 41, s })) };
const data = encodeServer(msg)!;
assert.deepEqual(decodeServer(new DataView(data)), msg);
assert.equal(decodeServer(new DataView(data, 0, data.byteLength - 1)), null);
console.log('OK equipment ids survive binary uploads and mixed snapshots, including new UTF-8 ids');

const ships = startShips(b => surfaceOf(b, 1969));
const ship = ships.find(s => s.def.mounts.length)!;
const mounts = ship.mounts!;
const rt = mounts.list[0];
ship.sw[rt.key] = 1;
for (let i = 0; i < 20; i++) ship.tick(1 / 20);
assert.equal(ship.st[rt.iReady], 1);
mounts.aim(0, 0.5, 0.2);
for (let i = 0; i < 20; i++) ship.tick(1 / 20);
assert.ok(Math.abs(ship.st[rt.iYaw] - 0.5) < 0.01);
const ammo = ship.st[rt.iAmmo];
const sync = new VarSync(ship.vars, ship.st);
const barrel = mounts.tryFire(ship.st, 0, 1);
assert.equal(barrel, 0);
assert.equal(ship.st[rt.iAmmo], ammo - 1);
assert.equal(mounts.tryFire(ship.st, 0, 1.01), -1);
ship.tick(1 / 20);
assert.ok(ship.st[rt.iAmmo] < ammo - 0.9, 'a tick must not undo a shot');
const changes = sync.diff(ship.st);
let replicatedAmmo: number | undefined;
for (let i = 0; i < changes.length; i += 2) if (changes[i] === rt.iAmmo) replicatedAmmo = changes[i + 1];
assert.ok(replicatedAmmo !== undefined && replicatedAmmo < ammo - 0.9, 'fractional reload must not hide a spent round from replication');
ship.sw[rt.key] = 0;
ship.tick(1 / 20);
assert.equal(mounts.tryFire(ship.st, 0, 2), -1);
console.log('OK mount authority slews, spends ammo, reloads at its rate and refuses unpowered shots');

// An exterior mount leaves a translating/rotating host with the inherited world velocity.
ship.pose.v = [1600, 0, 0];
ship.pose.w = [0, 0.3, 0];
const host = shipHost(ship);
const o: [number, number, number] = [0, 0, 0], d: [number, number, number] = [0, 0, 0];
mountMuzzle(rt.def, rt.kind, { yaw: 0.5, pitch: 0.2 }, barrel, o, d);
const body = launch(9, 'minimissile', ship.id, o, d, 140);
const env: BallisticEnv = { host: () => host, hosts: () => [host], hostGravity: (_, out) => { out.fill(0); return out; }, sweepHost: () => null, sweepWorld: () => null, groundAlt: () => 100, crew: [] };
assert.equal(stepBallistic(body, { gravity: 0.25, radius: 0.8 }, 1 / 60, env), null);
assert.equal(body.fr, 0);
assert.ok(body.v[0] > 1400, 'host momentum must survive exterior launch');
console.log('OK exterior mounts use the shared ballistic handover at orbital speed');

const surface = surfaceOf(MOON_BODY, 1969)!;
const surfaces = () => surface;
const before = surface.mods.count;
// Use the antipodes: a ground edit must follow the body's radial surface, not world Y.
const radial = [0, -1, 0];
const r = MOON_BODY.radius + surface.height(radial);
const p = MOON_BODY.center.map((x, i) => x + radial[i] * r);
const effect = PROJECTILES.rocket.impact.terrain!;
const mod = terrainImpact(effect, p, surfaces)!;
assert.deepEqual(mod.center, radial);
assert.equal(surface.mods.count, before, 'preparing an edit must not apply it twice');
assert.equal(terrainImpact(undefined, p, surfaces), undefined);
assert.equal(terrainImpact(effect, p.map((x, i) => x + radial[i] * 4), surfaces), undefined);
const original = surface.height(radial);
surface.addMod(mod);
assert.ok(surface.height(radial) < original - 0.5);
const flatten = { kind: 'flatten', radius: 4, maxHeight: 1.2, params: [NaN] };
const edit = terrainImpact(flatten, p, surfaces)!;
assert.ok(Number.isFinite(edit.params![0]));
assert.deepEqual(JSON.parse(JSON.stringify(edit)), edit, 'wire payload must contain resolved parameters');
assert.ok(Number.isNaN(flatten.params[0]), 'catalog data must remain reusable');
assert.throws(() => defineProjectile({ ...PROJECTILES.rocket, id: 'bad.terrain', impact: { ...PROJECTILES.rocket.impact, terrain: { ...effect, kind: 'missing' } } }), /unknown terrain/);
console.log('OK catalog surface effects are radial, height-limited, serializable and independent of ships');

// Serialized directions have a rounded length. At lunar scale that must neither attract
// separate impacts to an old crater nor exclude the crater from its own render/physics jobs.
const R = MOON_BODY.radius;
const modBody = { radius: R, pole: [0, 0, -1] };
for (const side of [1, -1]) {
  const craterAt = (x: number, z = 25) => blastCrater('moon', [0, 0, 0], [x, side * R, z], 2.4);
  const separated = new TerrainMods(modBody);
  separated.add(craterAt(35));
  separated.add(craterAt(47));
  assert.equal(separated.count, 2, 'impacts 12 m apart must dig separate craters');
  assert.equal(separated.dynamic[0].params, undefined, 'a distant hit must not deepen the first crater');
  const repeat = new TerrainMods(modBody);
  const first = repeat.add(craterAt(50));
  repeat.add(craterAt(50));
  assert.equal(repeat.count, 1, 'a second hit at the same point must merge even if rounded length is below one');
  assert.equal(first.params![0], 1.5);
  const m = craterAt(50);
  assert.ok(modTouches(m, modBody, m.center, 0), 'a crater must invalidate its own centre');
  assert.ok(modTouches(m, modBody, craterAt(54).center, 0), 'a point 4 m away is inside the crater reach');
  assert.ok(!modTouches(m, modBody, craterAt(62).center, 0), 'a point 12 m away is outside the crater reach');
  // Exercise the spatial index as well as the small-list path, using actual profile samples.
  const many = new TerrainMods(modBody);
  for (let i = 0; i < 24; i++) many.add(craterAt(35 + i * 12));
  assert.equal(many.count, 24);
  const saved = new TerrainMods(modBody);
  saved.setDynamic(JSON.parse(JSON.stringify(many.dynamic)));
  for (const current of many.dynamic) {
    const d = current.center, len = Math.hypot(...d);
    const sample = { height: 0, albedo: 1, rocks: 1, mat: 0 };
    saved.apply(d[0] / len, d[1] / len, d[2] / len, 0, sample);
    assert.ok(sample.height < -0.7, 'every separate serialized crater must deform its own ground');
    assert.equal(saved.near(d, 0, 0).length, 1, 'worker selection must find only the nearby edit');
  }
}
console.log('OK rounded lunar directions preserve separate craters, local rebuilding and persisted indexed terrain');

// Ground contacts must be on the swept surface, not the buried end of a fixed step.
const groundEnv: BallisticEnv = { host: () => undefined, hosts: () => [], hostGravity: (_, out) => out, sweepHost: () => null, sweepWorld: () => null, groundAlt: p => p[1], crew: [] };
for (const speed of [42, 140, 380, 1600]) {
  const shot = launch(9, 'test.ground', 0, [0, 0.1, 0], [0, -1, 0], speed);
  const hit = stepBallistic(shot, { gravity: 0, radius: 0.1 }, 1 / 60, groundEnv);
  assert.ok(hit && hit !== 'lost');
  assert.ok(hit.p[1] >= 0 && hit.p[1] < 0.001, 'contact must stay exposed, within a millimetre of the surface');
  assert.deepEqual(shot.p, hit.p);
}
const slopeEnv = { ...groundEnv, groundAlt: (p: [number, number, number]) => p[1] - 0.2 * p[0] };
const sloped = launch(9, 'test.ground', 0, [0, 1, 0], [1, -0.1, 0], 380);
const slopeHit = stepBallistic(sloped, { gravity: 0, radius: 0.1 }, 1 / 60, slopeEnv);
assert.ok(slopeHit && slopeHit !== 'lost');
assert.ok(Math.abs(slopeHit.p[0] - 10 / 3) < 0.001, 'refinement must follow the actual slope, preserving the impact position');
const blockedEnv = { ...groundEnv, sweepWorld: () => ({ p: [0, -1, 0] as [number, number, number], fr: 77, l: [0, 0, 0] as [number, number, number] }), crew: [{ id: 2, p: [0, -0.5, 0] as [number, number, number] }] };
const blocked = launch(9, 'test.ground', 0, [0, 0.1, 0], [0, -1, 0], 380);
const groundFirst = stepBallistic(blocked, { gravity: 0, radius: 0.1 }, 1 / 60, blockedEnv);
assert.ok(groundFirst && groundFirst !== 'lost');
assert.equal(groundFirst.fr, 0, 'ground must precede a hull or crew behind it');
for (const side of [1, -1]) {
  const radialEnv = { ...groundEnv, groundAlt: (p: [number, number, number]) => Math.hypot(p[0], p[1], p[2]) - R };
  const shot = launch(9, 'test.ground', 0, [side * (R + 0.1), 0, 0], [-side, 0, 0], 1600);
  const hit = stepBallistic(shot, { gravity: 0, radius: 0.1 }, 1 / 60, radialEnv);
  assert.ok(hit && hit !== 'lost');
  assert.ok(radialEnv.groundAlt(hit.p) >= 0 && radialEnv.groundAlt(hit.p) < 0.001, 'contact must work radially on either side of a body');
}
console.log('OK swept ground contacts stay exposed on slopes and radial surfaces, ahead of buried hulls and crew');
