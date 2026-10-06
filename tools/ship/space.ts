import assert from 'node:assert/strict';
import { localAt, type FrameHost } from '../../src/shared/frames/frame.js';
import { clonePose, lerpPose, qSpin, toLocal, toWorld, type ShipPose } from '../../src/shared/ship/flight/pose.js';
import { MOON_BODY, homeDir } from '../../src/shared/space/body.js';
import { offsetDir } from '../../src/shared/space/tangent.js';
import { BodySurface, MOON_SURFACE, surfaceSample } from '../../src/shared/space/surface.js';
import type { V3 } from '../../src/shared/ship/geom.js';

const prev: ShipPose = { p: [9e6, -4e6, 1e7], q: [0, 0, 0, 1], v: [1600, 0, 0], w: [0, 0.3, 0] };
const pose = clonePose(prev);
pose.p[0] += 1600 / 60;
pose.q = qSpin(prev.q, prev.w, 1 / 60);
const host: FrameHost = { id: 1, prev, pose, reach: 10, inside: () => false };
const scratch: V3 = [0, 0, 0];
let maxError = 0;
for (const angle of [0.005, 0.3, 1, 2.9]) {
  pose.q = qSpin(prev.q, [0.2, 0.7, -0.4], angle);
  for (let i = 0; i <= 16; i++) {
    const t = i / 16, at = lerpPose(prev, pose, t);
    const point = toWorld(at, [3, -2, 7]);
    const expected = toLocal(at, point);
    localAt(host, point, t, scratch);
    maxError = Math.max(maxError, Math.hypot(...scratch.map((n, i) => n - expected[i])));
  }
}
assert.ok(maxError < 1e-8, 'moving hull coordinates must agree with actual interpolated poses');
console.log('OK moving hull sweep coordinates at large world positions:', { maxError });

const mountain = new BodySurface(MOON_SURFACE, MOON_BODY, 1969);
const plain = new BodySurface({ ...MOON_SURFACE, mountains: undefined }, MOON_BODY, 1969);
const a = surfaceSample(), b = surfaceSample(), dir: V3 = [0, 0, 0];
const center = homeDir(MOON_BODY);
let minAdded = Infinity, maxAdded = -Infinity, best: V3 = [0, 0, 0];
for (let z = -50000; z <= 50000; z += 2000) for (let x = -50000; x <= 50000; x += 2000) {
  offsetDir(MOON_BODY.pole, center, x, z, MOON_BODY.radius, dir);
  mountain.sample(...dir, 100, a); plain.sample(...dir, 100, b);
  const added = a.height - b.height;
  minAdded = Math.min(minAdded, added);
  if (added > maxAdded) { maxAdded = added; best = [x, a.height, z]; }
  assert.ok(a.height <= mountain.highest && a.height >= mountain.lowest, 'terrain bounds must include the new relief');
}
console.log('OK deterministic mountain belts and conservative terrain bounds:', { minAdded, maxAdded, best });
assert.ok(maxAdded > 400 && maxAdded - minAdded > 300, 'mountains must add visible ridges, not just raise the datum');

let globalPeak = 0;
for (let i = 0; i < 1024; i++) {
  const y = 1 - 2 * (i + 0.5) / 1024, r = Math.sqrt(1 - y * y), angle = i * Math.PI * (3 - Math.sqrt(5));
  dir[0] = r * Math.cos(angle); dir[1] = y; dir[2] = r * Math.sin(angle);
  mountain.sample(...dir, 100, a); plain.sample(...dir, 100, b);
  globalPeak = Math.max(globalPeak, a.height - b.height);
  assert.ok(a.height <= mountain.highest && a.height >= mountain.lowest);
}
assert.ok(globalPeak > 2000, 'the lunar highlands must contain mountain ranges beyond the calmer home mare');
console.log('OK mountain ranges across the whole Moon:', { globalPeak });
