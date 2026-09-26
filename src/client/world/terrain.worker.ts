/// <reference lib="webworker" />
// Generates terrain chunk geometry, collision heightfields and rock scatter off the main thread.

import { LunarTerrain, rocksInTile, type TerrainSample } from '../../shared/terrain';

export type WorkerJob =
  | { kind: 'chunk'; id: number; seed: number; x0: number; z0: number; size: number; res: number; skirt: number; morphEnd: number }
  | { kind: 'heights'; id: number; seed: number; x0: number; z0: number; size: number; res: number }
  | { kind: 'rocks'; id: number; seed: number; x0: number; z0: number; size: number; minSize: number; maxSize: number };

export type WorkerResult =
  | { kind: 'chunk'; id: number; positions: Float32Array; normals: Float32Array; albedo: Float32Array; morph: Float32Array; minY: number; maxY: number }
  | { kind: 'heights'; id: number; heights: Float32Array }
  | { kind: 'rocks'; id: number; rocks: Float32Array };

let terrain: LunarTerrain | null = null;
const sample: TerrainSample = { height: 0, albedo: 1 };

function getTerrain(seed: number) {
  if (!terrain || terrain.seed !== seed) terrain = new LunarTerrain(seed);
  return terrain;
}

function buildChunk(job: Extract<WorkerJob, { kind: 'chunk' }>): WorkerResult {
  const t = getTerrain(job.seed);
  const { x0, z0, size, res, skirt, morphEnd } = job;
  const step = size / res;
  const n = res + 3; // one-sample border for normals
  const h = new Float32Array(n * n);
  const alb = new Float32Array(n * n);
  for (let j = 0; j < n; j++) {
    for (let i = 0; i < n; i++) {
      t.sample(x0 + (i - 1) * step, z0 + (j - 1) * step, step, sample);
      h[j * n + i] = sample.height;
      alb[j * n + i] = sample.albedo;
    }
  }
  // Parent-level heights on the even vertices (band-limited like the parent chunk), used as
  // CDLOD morph targets: far vertices blend into exactly the parent's grid → crack-free joins.
  const half = res / 2;
  const hp = new Float32Array((half + 1) * (half + 1));
  for (let j = 0; j <= half; j++)
    for (let i = 0; i <= half; i++) hp[j * (half + 1) + i] = t.sample(x0 + i * 2 * step, z0 + j * 2 * step, step * 2, sample).height;

  const vcount = (res + 1) * (res + 1) + 4 * (res + 1);
  const positions = new Float32Array(vcount * 3);
  const normals = new Float32Array(vcount * 3);
  const albedo = new Float32Array(vcount);
  const morph = new Float32Array(vcount * 4);
  let minY = Infinity;
  let maxY = -Infinity;
  let k = 0;
  for (let j = 0; j <= res; j++) {
    for (let i = 0; i <= res; i++) {
      const c = (j + 1) * n + (i + 1);
      const y = h[c];
      positions[k * 3] = i * step;
      positions[k * 3 + 1] = y;
      positions[k * 3 + 2] = j * step;
      const dx = (h[c + 1] - h[c - 1]) / (2 * step);
      const dz = (h[c + n] - h[c - n]) / (2 * step);
      const inv = 1 / Math.sqrt(dx * dx + 1 + dz * dz);
      normals[k * 3] = -dx * inv;
      normals[k * 3 + 1] = inv;
      normals[k * 3 + 2] = -dz * inv;
      albedo[k] = alb[c];
      // odd vertices slide onto their lower even neighbour (CDLOD)
      const ie = i - (i & 1);
      const je = j - (j & 1);
      const my = hp[(je / 2) * (half + 1) + ie / 2];
      morph[k * 4] = ie * step;
      morph[k * 4 + 1] = my;
      morph[k * 4 + 2] = je * step;
      morph[k * 4 + 3] = morphEnd;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
      if (my < minY) minY = my;
      if (my > maxY) maxY = my;
      k++;
    }
  }
  // skirts: copy of each edge, pushed down (backup for transient LOD jumps)
  const edge = (i: number, j: number) => {
    const src = j * (res + 1) + i;
    positions[k * 3] = positions[src * 3];
    positions[k * 3 + 1] = positions[src * 3 + 1] - skirt;
    positions[k * 3 + 2] = positions[src * 3 + 2];
    normals[k * 3] = normals[src * 3];
    normals[k * 3 + 1] = normals[src * 3 + 1];
    normals[k * 3 + 2] = normals[src * 3 + 2];
    albedo[k] = albedo[src];
    morph[k * 4] = morph[src * 4];
    morph[k * 4 + 1] = morph[src * 4 + 1] - skirt;
    morph[k * 4 + 2] = morph[src * 4 + 2];
    morph[k * 4 + 3] = morphEnd;
    k++;
  };
  for (let i = 0; i <= res; i++) edge(i, 0);
  for (let i = 0; i <= res; i++) edge(i, res);
  for (let j = 0; j <= res; j++) edge(0, j);
  for (let j = 0; j <= res; j++) edge(res, j);
  return { kind: 'chunk', id: job.id, positions, normals, albedo, morph, minY: minY - skirt, maxY };
}

function buildHeights(job: Extract<WorkerJob, { kind: 'heights' }>): WorkerResult {
  const t = getTerrain(job.seed);
  const { x0, z0, size, res } = job;
  const step = size / res;
  const heights = new Float32Array((res + 1) * (res + 1));
  // row-major: heights[j * (res + 1) + i] at (x0 + i*step, z0 + j*step)
  for (let j = 0; j <= res; j++)
    for (let i = 0; i <= res; i++) heights[j * (res + 1) + i] = t.sample(x0 + i * step, z0 + j * step, step, sample).height;
  return { kind: 'heights', id: job.id, heights };
}

function buildRocks(job: Extract<WorkerJob, { kind: 'rocks' }>): WorkerResult {
  const t = getTerrain(job.seed);
  const list = rocksInTile(t, job.x0, job.z0, job.size, job.minSize).filter((r) => r.size <= job.maxSize);
  const out = new Float32Array(list.length * 7);
  list.forEach((r, i) => {
    out.set([r.x, t.height(r.x, r.z, 0), r.z, r.size, r.rot, r.variant, r.jitter], i * 7);
  });
  return { kind: 'rocks', id: job.id, rocks: out };
}

self.onmessage = (e: MessageEvent<WorkerJob>) => {
  const job = e.data;
  let res: WorkerResult;
  if (job.kind === 'chunk') res = buildChunk(job);
  else if (job.kind === 'heights') res = buildHeights(job);
  else res = buildRocks(job);
  const transfer: Transferable[] =
    res.kind === 'chunk'
      ? [res.positions.buffer, res.normals.buffer, res.albedo.buffer, res.morph.buffer]
      : res.kind === 'heights'
        ? [res.heights.buffer]
        : [res.rocks.buffer];
  (self as unknown as Worker).postMessage(res, transfer);
};
