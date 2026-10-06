// Compare actual worker buffers along adjoining patches. No browser, renderer or copied generator.
import assert from 'node:assert/strict';
import { buildTerrainJob, type WorkerJob } from '../../src/client/world/terrain.worker.js';
import { patchTerrainDepth } from '../../src/client/world/terrainShader.js';
import * as THREE from 'three';
import { TERRAIN_RES } from '../../src/client/world/terrainGrid.js';

type SphereJob = Extract<WorkerJob, { kind: 'sphere' }>;
const sphere = (job: SphereJob) => { const out = buildTerrainJob(job); assert.equal(out.kind, 'sphere'); return out as Extract<ReturnType<typeof buildTerrainJob>, { kind: 'sphere' }>; };

// the old grid and the game's (world/terrainGrid.ts)
for (const RES of [...new Set([32, TERRAIN_RES])]) {
const W = RES + 1;
console.log(`— celdas por nodo: ${RES}`);
const job: SphereJob = { kind: 'sphere', id: 1, body: 'moon', seed: 1969, mods: [], face: 2, a0: 0.001, b0: 0.001, size: 2 / 2 ** 17, res: RES, skirt: 2, morphEnd: 80, sun: null, texPeriod: 2560 };

const a = sphere(job), b = sphere({ ...job, a0: job.a0 + job.size });
const coarse = sphere({ ...job, a0: job.a0 + job.size, size: job.size * 2 });
let ownPosition = 0, ownNormal = 0, lodPosition = 0, lodNormal = 0;
const angle = (a: Float32Array, ai: number, b: Float32Array, bi: number) => {
  const ax = a[ai], ay = a[ai + 1], az = a[ai + 2], bx = b[bi], by = b[bi + 1], bz = b[bi + 2];
  return Math.atan2(Math.hypot(ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx), ax * bx + ay * by + az * bz);
};
for (let j = 0; j <= job.res; j++) {
  const ia = j * W + RES, ib = j * W;
  let d = 0;
  for (let q = 0; q < 3; q++) d += (a.positions[ia * 3 + q] + a.center[q] - b.positions[ib * 3 + q] - b.center[q]) ** 2;
  ownPosition = Math.max(ownPosition, Math.sqrt(d)); ownNormal = Math.max(ownNormal, angle(a.normals, ia * 3, b.normals, ib * 3));
  if (j % 2) continue;
  const ic = (j / 2) * W;
  d = 0;
  for (let q = 0; q < 3; q++) d += (a.morph[ia * 4 + q] + a.center[q] - coarse.positions[ic * 3 + q] - coarse.center[q]) ** 2;
  lodPosition = Math.max(lodPosition, Math.sqrt(d)); lodNormal = Math.max(lodNormal, angle(a.morphN, ia * 4, coarse.normals, ic * 3));
  assert.equal(a.morphS[ia * 2], coarse.surf[ic * 3], 'albedo must use parent detail');
  assert.equal(a.morphS[ia * 2 + 1], coarse.surf[ic * 3 + 1], 'worked ground must use parent detail');
}
assert.ok(ownPosition < 0.0001 && lodPosition < 0.0001, 'shared edges must agree within 0.1 mm');
assert.ok(ownNormal < 1e-6 && lodNormal < 1e-6, 'LOD transitions must agree on shading, not just geometry');
console.log('OK worker edges match positions and normals:', JSON.stringify({ ownPosition, ownNormalDegrees: ownNormal * 180 / Math.PI, lodPosition, lodNormalDegrees: lodNormal * 180 / Math.PI }));

for (const size of [job.size, job.size * 4]) {
  const withSun = { ...job, size, sun: [0.8, 0.15, 0.58] as [number, number, number] };
  const l = sphere(withSun), r = sphere({ ...withSun, a0: job.a0 + size });
  const parent = sphere({ ...withSun, a0: job.a0 + size, size: size * 2 });
  for (let j = 0; j <= job.res; j++) {
    const i = j * W + RES, ri = j * W;
    assert.ok(Math.abs(l.surf[i * 3 + 2] - r.surf[ri * 3 + 2]) < 1e-6, 'baked shadow must not depend on patch ownership');
    if (j % 2 === 0) assert.ok(Math.abs(l.morphN[i * 4 + 3] - parent.surf[(j / 2) * W * 3 + 2]) < 1e-6, 'baked shadow must converge to the coarser neighbor');
  }
}
const shader = { uniforms: {}, vertexShader: THREE.ShaderLib.depth.vertexShader, fragmentShader: THREE.ShaderLib.depth.fragmentShader };
patchTerrainDepth(shader, { value: new THREE.Vector3() }, W * W);
assert.match(shader.vertexShader, new RegExp(`vSkirtDepth = float\\(gl_VertexID >= ${W * W}\\)`));
assert.match(shader.fragmentShader, /if \(vSkirtDepth > 0.0\) discard/);
console.log('OK baked shadow continuity and actual depth shader exclusion of artificial skirts');

// A sunlit coarse patch must stay sunlit: angular penumbra cannot grow with its cell size.
// At 500+ m/cell the old blur darkened an entire LOD rectangle even with a 45° Sun.
const exposed = sphere({ ...job, a0: -0.004, b0: -0.004, size: 2 / 2 ** 8, sun: [Math.SQRT1_2, Math.SQRT1_2, 0] });
let dimmed = 0;
for (let i = 0; i < W * W; i++) if (exposed.surf[i * 3 + 2] < 0.99) dimmed++;
assert.ok(dimmed < W * W * 0.03, 'exposed coarse ground must not be darkened by LOD-dependent penumbra');
console.log('OK exposed coarse terrain remains illuminated:', { dimmed, vertices: W * W });
}
