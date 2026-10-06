/// <reference lib="webworker" />
// Builds, off the main thread, everything that is sampled from a body's ground (its procedural
// surface with the terrain modifiers laid on it: space/surface.ts): the terrain's nodes, the
// physics bubble's collision tiles and the boulders. Every job carries the modifiers that can reach
// its piece of ground (the main thread keeps them), so the worker needs no other state.

import { bareSurface, BODIES } from '../../shared/space/body';
import { cellOf, cubeArc, cubeDir, faceOf, facePoint, CUBE_FACES } from '../../shared/space/cubeSphere';
import { rockAt, rockLevels, rocksInTile, ROCK_STRIDE } from '../../shared/space/rocks';
import { surfaceSample, type BodySurface } from '../../shared/space/surface';
import type { TerrainMod } from '../../shared/space/terrainMods/types';

export type WorkerJob =
  /**
   * A node of a body's terrain: a (res+1)² grid over face parameters [a0, a0+size] × [b0, b0+size],
   * with its CDLOD morph targets (the parent's grid), detail texture coordinates snapped to
   * `texPeriod` m, horizon shadows toward `sun` (world, unit; null: none) and skirts.
   */
  | { kind: 'sphere'; id: number; body: string; seed: number; mods: TerrainMod[]; face: number; a0: number; b0: number; size: number; res: number; skirt: number; morphEnd: number; sun: [number, number, number] | null; texPeriod: number }
  /**
   * Collision of the physics bubble (frames/bubble.ts): heights of the ground in a tangent frame
   * (origin `o`, axes east `e`, up `u`, south `s`, world) over a (res+1)² grid of frame [x0, x0+size] ×
   * [z0, z0+size], along `u` (row-major, rows along z), and the boulders from `minRock` m whose
   * centre is in the tile (frame x, y, z, size).
   */
  | { kind: 'tangent'; id: number; body: string; seed: number; mods: TerrainMod[]; o: number[]; e: number[]; u: number[]; s: number[]; x0: number; z0: number; size: number; res: number; minRock: number }
  /** The boulders of a rock tile (space/rocks.ts) sized minSize..maxSize. */
  | { kind: 'rocks'; id: number; body: string; seed: number; mods: TerrainMod[]; face: number; ti: number; tj: number; minSize: number; maxSize: number };

export type WorkerResult =
  | {
      kind: 'sphere';
      id: number;
      /** Vertices relative to `center` (world, the node's middle on the mean sphere); skirts after the grid. */
      positions: Float32Array;
      normals: Float32Array;
      /** Albedo, worked ground, sun visibility. */
      surf: Float32Array;
      /** Morph target (the parent's grid, same space) and the distance the morph ends at. */
      morph: Float32Array;
      /** The morph target's normal and sun visibility. */
      morphN: Float32Array;
      /** The parent's albedo and worked-ground value (same filtered sample as its geometry). */
      morphS: Float32Array;
      /** Detail texture coordinates (m) and the morph target's. */
      tex: Float32Array;
      /** The face's a axis (world): the detail normals' tangent. */
      faceU: Float32Array;
      center: [number, number, number];
      /** Farthest vertex from `center` (m), lowest and highest surface height (m). */
      radius: number;
      hMin: number;
      hMax: number;
    }
  | { kind: 'tangent'; id: number; heights: Float32Array; rocks: Float32Array }
  /** Per rock: x, y, z (relative to `center`), up (unit), size, yaw, variant + jitter/2. */
  | { kind: 'rocks'; id: number; center: [number, number, number]; rocks: Float32Array };

/** The worker's copy of each body's surface (no sites: every job brings its modifiers). */
const surfaces = new Map<string, BodySurface>();

function surfaceFor(body: string, seed: number, mods: TerrainMod[]) {
  const key = `${body}:${seed}`;
  let s = surfaces.get(key);
  if (!s) {
    s = bareSurface(BODIES[body], seed)!;
    surfaces.set(key, s);
  }
  s.mods.setStatic(mods);
  return s;
}

const smp = surfaceSample();
const _dir = [0, 0, 0];

/** Vertices between two evaluations of the far part of the sun march (the far horizon changes slowly). */
const FAR_EVERY = 4;
/** Where the far part of the march starts (vertex spacings). */
const FAR_FROM = 12;

/** Central differences on a grid with a one-sample ring; shared by both LODs. */
function gridNormals(P: Float64Array, n: number, res: number): Float32Array {
  const out = new Float32Array((res + 1) * (res + 1) * 3);
  for (let j = 0; j <= res; j++) for (let i = 0; i <= res; i++) {
    const k = (j + 1) * n + i + 1;
    const ax = P[(k + 1) * 3] - P[(k - 1) * 3], ay = P[(k + 1) * 3 + 1] - P[(k - 1) * 3 + 1], az = P[(k + 1) * 3 + 2] - P[(k - 1) * 3 + 2];
    const bx = P[(k + n) * 3] - P[(k - n) * 3], by = P[(k + n) * 3 + 1] - P[(k - n) * 3 + 1], bz = P[(k + n) * 3 + 2] - P[(k - n) * 3 + 2];
    const nx = ay * bz - az * by, ny = az * bx - ax * bz, nz = ax * by - ay * bx;
    const len = Math.hypot(nx, ny, nz) || 1, v = (j * (res + 1) + i) * 3;
    out[v] = nx / len; out[v + 1] = ny / len; out[v + 2] = nz / len;
  }
  return out;
}

function buildSphere(job: Extract<WorkerJob, { kind: 'sphere' }>): WorkerResult {
  const body = BODIES[job.body];
  const surface = surfaceFor(job.body, job.seed, job.mods);
  const { face, a0, b0, size, res } = job;
  const R = body.radius;
  const c = body.center;
  const n = res + 3; // with the ring for the normals
  const step = size / res;
  const stepM = cubeArc(step, R);
  cubeDir(face, a0 + size / 2, b0 + size / 2, _dir);
  const center: [number, number, number] = [c[0] + _dir[0] * R, c[1] + _dir[1] * R, c[2] + _dir[2] * R];
  const ox = center[0] - c[0];
  const oy = center[1] - c[1];
  const oz = center[2] - c[2];
  // every sample (ring included), relative to the node's centre, in float64
  const P = new Float64Array(n * n * 3);
  const D = new Float64Array(n * n * 3);
  const H = new Float64Array(n * n);
  const alb = new Float32Array(n * n);
  const mat = new Float32Array(n * n);
  let hMin = Infinity;
  let hMax = -Infinity;
  for (let j = 0; j < n; j++) {
    for (let i = 0; i < n; i++) {
      const d = cubeDir(face, a0 + (i - 1) * step, b0 + (j - 1) * step, _dir);
      const s = surface.sample(d[0], d[1], d[2], stepM, smp);
      const r = R + s.height;
      const k = j * n + i;
      P[k * 3] = d[0] * r - ox;
      P[k * 3 + 1] = d[1] * r - oy;
      P[k * 3 + 2] = d[2] * r - oz;
      D[k * 3] = d[0];
      D[k * 3 + 1] = d[1];
      D[k * 3 + 2] = d[2];
      H[k] = s.height;
      alb[k] = s.albedo;
      mat[k] = s.mat;
      if (i > 0 && j > 0 && i < n - 1 && j < n - 1) {
        if (s.height < hMin) hMin = s.height;
        if (s.height > hMax) hMax = s.height;
      }
    }
  }
  // the parent's grid (every other vertex, sampled at the parent's detail): the CDLOD morph targets,
  // so far vertices blend into exactly the coarser neighbour's surface (crack-free joins)
  const half = res / 2;
  const pn = half + 3;
  const PP = new Float64Array(pn * pn * 3);
  const PD = new Float64Array(pn * pn * 3);
  const PH = new Float64Array(pn * pn);
  const PA = new Float32Array(pn * pn);
  const PM = new Float32Array(pn * pn);
  const parentMin = cubeArc(step * 2, R);
  for (let j = 0; j < pn; j++) {
    for (let i = 0; i < pn; i++) {
      const d = cubeDir(face, a0 + (i - 1) * 2 * step, b0 + (j - 1) * 2 * step, _dir);
      const sample = surface.sample(d[0], d[1], d[2], parentMin, smp), h = sample.height;
      const k = j * pn + i;
      for (let q = 0; q < 3; q++) { PP[k * 3 + q] = d[q] * (R + h) - (q === 0 ? ox : q === 1 ? oy : oz); PD[k * 3 + q] = d[q]; }
      PH[k] = h; PA[k] = sample.albedo; PM[k] = sample.mat;
      if (i > 0 && j > 0 && i < pn - 1 && j < pn - 1) {
        if (h < hMin) hMin = h;
        if (h > hMax) hMax = h;
      }
    }
  }
  const sun = job.sun ? sunGrid(job, surface, P, D, H, n, stepM, [ox, oy, oz]) : null;
  const parentSun = job.sun ? sunGrid({ ...job, res: half }, surface, PP, PD, PH, pn, parentMin, [ox, oy, oz]) : null;
  const PN = gridNormals(PP, pn, half);
  // detail texture coordinates: metres along the face, relative to a corner snapped to the texture
  // period (float32 keeps millimetres inside any node; neighbours agree modulo the period)
  const faceM = cubeArc(2, R) / 2;
  const P0 = job.texPeriod;
  const tu0 = Math.floor((a0 * faceM) / P0) * P0;
  const tv0 = Math.floor((b0 * faceM) / P0) * P0;
  const grid = (res + 1) * (res + 1);
  const total = grid + 4 * (res + 1);
  const positions = new Float32Array(total * 3);
  const normals = new Float32Array(total * 3);
  const surf = new Float32Array(total * 3);
  const morph = new Float32Array(total * 4);
  const morphN = new Float32Array(total * 4);
  const morphS = new Float32Array(total * 2);
  const tex = new Float32Array(total * 4);
  const faceU = new Float32Array(total * 3);
  const fu = CUBE_FACES[face].u;
  let radius = 0;
  const N = gridNormals(P, n, res);
  const put = (v: number, i: number, j: number, drop: number) => {
    const k = (j + 1) * n + (i + 1);
    const g = j * (res + 1) + i;
    const dx = D[k * 3];
    const dy = D[k * 3 + 1];
    const dz = D[k * 3 + 2];
    const x = P[k * 3] - dx * drop;
    const y = P[k * 3 + 1] - dy * drop;
    const z = P[k * 3 + 2] - dz * drop;
    positions[v * 3] = x;
    positions[v * 3 + 1] = y;
    positions[v * 3 + 2] = z;
    const rr = Math.sqrt(x * x + y * y + z * z);
    if (rr > radius) radius = rr;
    normals[v * 3] = N[g * 3];
    normals[v * 3 + 1] = N[g * 3 + 1];
    normals[v * 3 + 2] = N[g * 3 + 2];
    surf[v * 3] = alb[k];
    surf[v * 3 + 1] = mat[k];
    const sv = sun ? sun[g] : 1;
    surf[v * 3 + 2] = sv;
    // odd vertices slide onto their lower even neighbour (CDLOD)
    const ie = i - (i & 1);
    const je = j - (j & 1);
    const pk = (je / 2 + 1) * pn + ie / 2 + 1;
    morph[v * 4] = PP[pk * 3] - dx * drop;
    morph[v * 4 + 1] = PP[pk * 3 + 1] - dy * drop;
    morph[v * 4 + 2] = PP[pk * 3 + 2] - dz * drop;
    morph[v * 4 + 3] = job.morphEnd;
    const ge = je / 2 * (half + 1) + ie / 2;
    morphN[v * 4] = PN[ge * 3];
    morphN[v * 4 + 1] = PN[ge * 3 + 1];
    morphN[v * 4 + 2] = PN[ge * 3 + 2];
    morphN[v * 4 + 3] = parentSun ? parentSun[ge] : 1;
    morphS[v * 2] = PA[pk]; morphS[v * 2 + 1] = PM[pk];
    tex[v * 4] = (a0 + i * step) * faceM - tu0;
    tex[v * 4 + 1] = (b0 + j * step) * faceM - tv0;
    tex[v * 4 + 2] = (a0 + ie * step) * faceM - tu0;
    tex[v * 4 + 3] = (b0 + je * step) * faceM - tv0;
    faceU[v * 3] = fu[0];
    faceU[v * 3 + 1] = fu[1];
    faceU[v * 3 + 2] = fu[2];
  };
  let v = 0;
  for (let j = 0; j <= res; j++) for (let i = 0; i <= res; i++) put(v++, i, j, 0);
  // skirts: the four edges again, hanging toward the centre (order: b = 0, b = res, a = 0, a = res)
  for (let i = 0; i <= res; i++) put(v++, i, 0, job.skirt);
  for (let i = 0; i <= res; i++) put(v++, i, res, job.skirt);
  for (let j = 0; j <= res; j++) put(v++, 0, j, job.skirt);
  for (let j = 0; j <= res; j++) put(v++, res, j, job.skirt);
  return { kind: 'sphere', id: job.id, positions, normals, surf, morph, morphN, morphS, tex, faceU, center, radius, hMin, hMax };
}

/**
 * Soft self-shadowing of the ground toward the sun, per vertex: march the surface toward the sun's
 * azimuth on the vertex's own horizon (the curvature comes in by itself) and compare the steepest
 * horizon with the sun's elevation; the solar disc, widened with the vertex spacing, gives a soft
 * penumbra. Near samples come from the node's own grid while they fall inside it; the far ones are
 * sampled on every FAR_EVERY-th vertex and interpolated in between, corrected for the vertex's
 * height. Baked with the sun of the moment (the terrain rebuilds its nodes when it moves).
 */
function sunGrid(job: Extract<WorkerJob, { kind: 'sphere' }>, surface: BodySurface, P: Float64Array, D: Float64Array, H: Float64Array, n: number, stepM: number, o: number[]) {
  const sun = job.sun!;
  const res = job.res;
  const R = surface.radius;
  // grid steps per metre along a world tangent direction: the node's Jacobian at its middle
  const mid = (res / 2 + 1) * n + (res / 2 + 1);
  const Ta = [0, 0, 0];
  const Tb = [0, 0, 0];
  for (let q = 0; q < 3; q++) {
    Ta[q] = (P[(mid + 1) * 3 + q] - P[(mid - 1) * 3 + q]) / 2;
    Tb[q] = (P[(mid + n) * 3 + q] - P[(mid - n) * 3 + q]) / 2;
  }
  const g11 = Ta[0] * Ta[0] + Ta[1] * Ta[1] + Ta[2] * Ta[2];
  const g12 = Ta[0] * Tb[0] + Ta[1] * Tb[1] + Ta[2] * Tb[2];
  const g22 = Tb[0] * Tb[0] + Tb[1] * Tb[1] + Tb[2] * Tb[2];
  const det = g11 * g22 - g12 * g12 || 1;
  const ds: number[] = [];
  const maxRange = Math.min(0.12 * R, Math.max(4500, stepM * 400));
  for (let d = Math.max(stepM * 1.2, 0.8), i = 0; i < 22 && d < maxRange; i++, d *= 1.42) ds.push(d);
  let split = ds.findIndex((d) => d >= FAR_FROM * stepM);
  if (split < 0) split = ds.length;
  // Angular solar disc, independent of a patch's grid spacing. Growing this with stepM
  // dimmed even fully exposed ground in coarse patches and painted the quadtree as squares.
  const penumbra = 0.018;
  const t = [0, 0, 0];
  const q = [0, 0, 0];
  /** Toward the sun on the horizon of vertex k (unit, into t); tan of the sun's elevation (NaN: straight overhead). */
  const horizon = (k: number) => {
    const dx = D[k * 3];
    const dy = D[k * 3 + 1];
    const dz = D[k * 3 + 2];
    const sd = sun[0] * dx + sun[1] * dy + sun[2] * dz;
    t[0] = sun[0] - dx * sd;
    t[1] = sun[1] - dy * sd;
    t[2] = sun[2] - dz * sd;
    const tl = Math.sqrt(t[0] * t[0] + t[1] * t[1] + t[2] * t[2]);
    if (tl < 1e-6) return NaN;
    t[0] /= tl;
    t[1] /= tl;
    t[2] /= tl;
    return sd / tl;
  };
  /** Elevation (tan) of the ground `s` m toward the sun from vertex k, sampled on the surface. */
  const sampled = (k: number, s: number) => {
    const dx = D[k * 3];
    const dy = D[k * 3 + 1];
    const dz = D[k * 3 + 2];
    const a = s / R;
    const ca = Math.cos(a);
    const sa = Math.sin(a);
    q[0] = dx * ca + t[0] * sa;
    q[1] = dy * ca + t[1] * sa;
    q[2] = dz * ca + t[2] * sa;
    const r = R + surface.height(q, Math.max(stepM, s * 0.06));
    const ex = q[0] * r - o[0] - P[k * 3];
    const ey = q[1] * r - o[1] - P[k * 3 + 1];
    const ez = q[2] * r - o[2] - P[k * 3 + 2];
    return (ex * dx + ey * dy + ez * dz) / Math.max(1e-3, ex * t[0] + ey * t[1] + ez * t[2]);
  };
  // far horizon on the coarse grid
  const G = res / FAR_EVERY;
  const farTan = new Float32Array((G + 1) * (G + 1)).fill(-1);
  const farD = new Float32Array((G + 1) * (G + 1)).fill(1);
  if (split < ds.length) {
    for (let gj = 0; gj <= G; gj++) {
      for (let gi = 0; gi <= G; gi++) {
        const k = (gj * FAR_EVERY + 1) * n + (gi * FAR_EVERY + 1);
        if (Number.isNaN(horizon(k))) continue;
        let best = -1;
        let bestD = ds[split];
        for (let si = split; si < ds.length; si++) {
          const tn = sampled(k, ds[si]);
          if (tn > best) {
            best = tn;
            bestD = ds[si];
          }
        }
        farTan[gj * (G + 1) + gi] = best;
        farD[gj * (G + 1) + gi] = bestD;
      }
    }
  }
  const out = new Float32Array((res + 1) * (res + 1));
  for (let j = 0; j <= res; j++) {
    for (let i = 0; i <= res; i++) {
      const gi0 = i + 1;
      const gj0 = j + 1;
      const k = gj0 * n + gi0;
      const tanSun = horizon(k);
      if (Number.isNaN(tanSun)) {
        out[j * (res + 1) + i] = 1;
        continue;
      }
      // grid steps per metre along t
      const b1 = Ta[0] * t[0] + Ta[1] * t[1] + Ta[2] * t[2];
      const b2 = Tb[0] * t[0] + Tb[1] * t[1] + Tb[2] * t[2];
      const al = (g22 * b1 - g12 * b2) / det;
      const be = (g11 * b2 - g12 * b1) / det;
      let maxTan = -1;
      const px = P[k * 3];
      const py = P[k * 3 + 1];
      const pz = P[k * 3 + 2];
      const dx = D[k * 3];
      const dy = D[k * 3 + 1];
      const dz = D[k * 3 + 2];
      for (let si = 0; si < split; si++) {
        const s = ds[si];
        const gx = gi0 + al * s;
        const gy = gj0 + be * s;
        let tn: number;
        // At patch borders the same world sample must agree on both sides; a node-local
        // bilinear march/Jacobian gives different horizons depending on which node owns it.
        if (i > 0 && i < res && j > 0 && j < res && gx >= 0 && gy >= 0 && gx < n - 1 && gy < n - 1) {
          // inside the node's own grid: bilinear on its positions
          const x0 = Math.floor(gx);
          const y0 = Math.floor(gy);
          const fx = gx - x0;
          const fy = gy - y0;
          const a = (y0 * n + x0) * 3;
          const b = a + 3;
          const cc = a + n * 3;
          const d = cc + 3;
          const w0 = (1 - fx) * (1 - fy);
          const w1 = fx * (1 - fy);
          const w2 = (1 - fx) * fy;
          const w3 = fx * fy;
          const ex = P[a] * w0 + P[b] * w1 + P[cc] * w2 + P[d] * w3 - px;
          const ey = P[a + 1] * w0 + P[b + 1] * w1 + P[cc + 1] * w2 + P[d + 1] * w3 - py;
          const ez = P[a + 2] * w0 + P[b + 2] * w1 + P[cc + 2] * w2 + P[d + 2] * w3 - pz;
          tn = (ex * dx + ey * dy + ez * dz) / Math.max(1e-3, ex * t[0] + ey * t[1] + ez * t[2]);
        } else tn = sampled(k, s);
        if (tn > maxTan) maxTan = tn;
      }
      if (split < ds.length) {
        // far horizon: bilinear between the coarse grid's vertices, moved by this vertex's height above theirs
        const gi = Math.min(G - 1, Math.floor(i / FAR_EVERY));
        const gj = Math.min(G - 1, Math.floor(j / FAR_EVERY));
        const fx = i / FAR_EVERY - gi;
        const fz = j / FAR_EVERY - gj;
        const w = G + 1;
        const a = gj * w + gi;
        const lerp2 = (arr: Float32Array) => (arr[a] * (1 - fx) + arr[a + 1] * fx) * (1 - fz) + (arr[a + w] * (1 - fx) + arr[a + w + 1] * fx) * fz;
        const hAt = (gx: number, gy: number) => H[(gy * FAR_EVERY + 1) * n + (gx * FAR_EVERY + 1)];
        const hg = (hAt(gi, gj) * (1 - fx) + hAt(gi + 1, gj) * fx) * (1 - fz) + (hAt(gi, gj + 1) * (1 - fx) + hAt(gi + 1, gj + 1) * fx) * fz;
        const far = lerp2(farTan) + (hg - H[k]) / Math.max(1e-3, lerp2(farD));
        if (far > maxTan) maxTan = far;
      }
      const vis = (tanSun - maxTan) / penumbra + 0.5;
      out[j * (res + 1) + i] = vis < 0 ? 0 : vis > 1 ? 1 : vis;
    }
  }
  return out;
}

/**
 * The ground under each point of a tangent-frame grid, straight along the frame's up: a radial
 * sample first, then one correction along `u` (away from the anchor a radial ray leans by x/R, and
 * over high ground that would put the sample metres off to the side). Plus the boulders in the tile.
 */
function buildTangent(job: Extract<WorkerJob, { kind: 'tangent' }>): WorkerResult {
  const body = BODIES[job.body];
  const surface = surfaceFor(job.body, job.seed, job.mods);
  const { o, e, u, s, x0, z0, size, res } = job;
  const c = body.center;
  const R = body.radius;
  const step = size / res;
  const heights = new Float32Array((res + 1) * (res + 1));
  for (let j = 0; j <= res; j++) {
    for (let i = 0; i <= res; i++) {
      const x = x0 + i * step;
      const z = z0 + j * step;
      // the plane point (relative to the centre)
      const px = o[0] + e[0] * x + s[0] * z - c[0];
      const py = o[1] + e[1] * x + s[1] * z - c[1];
      const pz = o[2] + e[2] * x + s[2] * z - c[2];
      let t = 0;
      for (let k = 0; k < 2; k++) {
        const qx = px + u[0] * t;
        const qy = py + u[1] * t;
        const qz = pz + u[2] * t;
        const L = Math.sqrt(qx * qx + qy * qy + qz * qz);
        _dir[0] = qx / L;
        _dir[1] = qy / L;
        _dir[2] = qz / L;
        // the first pass only finds where the ray meets the ground: coarse is enough
        const rs = R + surface.height(_dir, k === 0 ? Math.max(step, 16) : step);
        const cosA = _dir[0] * u[0] + _dir[1] * u[1] + _dir[2] * u[2];
        t += (rs - L) / Math.max(0.5, cosA);
      }
      heights[j * (res + 1) + i] = t;
    }
  }
  return { kind: 'tangent', id: job.id, heights, rocks: tileRocks(job, surface) };
}

const _fp = facePoint();

/** Boulders from `minRock` m whose centre lies in a tangent tile: frame x, y (the ground), z and size. */
function tileRocks(job: Extract<WorkerJob, { kind: 'tangent' }>, surface: BodySurface): Float32Array {
  const levels = rockLevels(surface).filter((l) => l.sMax >= job.minRock);
  if (!levels.length) return new Float32Array(0);
  const body = BODIES[job.body];
  const { o, e, u, s, x0, z0, size } = job;
  const c = body.center;
  const R = body.radius;
  // the face-parameter box of the tile on each face it touches
  const box = new Map<number, number[]>();
  for (let j = 0; j <= 4; j++) {
    for (let i = 0; i <= 4; i++) {
      const x = x0 + (i / 4) * size;
      const z = z0 + (j / 4) * size;
      const f = faceOf(o[0] + e[0] * x + s[0] * z - c[0], o[1] + e[1] * x + s[1] * z - c[1], o[2] + e[2] * x + s[2] * z - c[2], _fp);
      const b = box.get(f.face);
      if (!b) box.set(f.face, [f.a, f.a, f.b, f.b]);
      else {
        b[0] = Math.min(b[0], f.a);
        b[1] = Math.max(b[1], f.a);
        b[2] = Math.min(b[2], f.b);
        b[3] = Math.max(b[3], f.b);
      }
    }
  }
  const found: number[] = [];
  const out: number[] = [];
  for (const L of levels) {
    const m = 2 / 2 ** L.level;
    for (const [face, b] of box) {
      const i0 = cellOf(b[0] - m, L.level);
      const i1 = cellOf(b[1] + m, L.level);
      const j0 = cellOf(b[2] - m, L.level);
      const j1 = cellOf(b[3] + m, L.level);
      for (let j = j0; j <= j1; j++) {
        for (let i = i0; i <= i1; i++) {
          found.length = 0;
          rockAt(surface, L, face, i, j, job.minRock, Infinity, found);
          if (!found.length) continue;
          // the rock's ground point, into the frame
          const r = R + found[3];
          const wx = c[0] + found[0] * r - o[0];
          const wy = c[1] + found[1] * r - o[1];
          const wz = c[2] + found[2] * r - o[2];
          const x = wx * e[0] + wy * e[1] + wz * e[2];
          const z = wx * s[0] + wy * s[1] + wz * s[2];
          if (x < x0 || x >= x0 + size || z < z0 || z >= z0 + size) continue;
          out.push(x, wx * u[0] + wy * u[1] + wz * u[2], z, found[4]);
        }
      }
    }
  }
  return new Float32Array(out);
}

function buildRocks(job: Extract<WorkerJob, { kind: 'rocks' }>): WorkerResult {
  const body = BODIES[job.body];
  const surface = surfaceFor(job.body, job.seed, job.mods);
  const R = body.radius;
  const c = body.center;
  const list: number[] = [];
  rocksInTile(surface, job.face, job.ti, job.tj, job.minSize, job.maxSize, list);
  const T = 2 ** rockLevels(surface)[0].level;
  const d = cubeDir(job.face, ((job.ti + 0.5) / T) * 2 - 1, ((job.tj + 0.5) / T) * 2 - 1, _dir);
  const center: [number, number, number] = [c[0] + d[0] * R, c[1] + d[1] * R, c[2] + d[2] * R];
  const count = list.length / ROCK_STRIDE;
  const rocks = new Float32Array(count * 9);
  for (let k = 0; k < count; k++) {
    const a = k * ROCK_STRIDE;
    const r = R + list[a + 3];
    const b = k * 9;
    rocks[b] = c[0] + list[a] * r - center[0];
    rocks[b + 1] = c[1] + list[a + 1] * r - center[1];
    rocks[b + 2] = c[2] + list[a + 2] * r - center[2];
    rocks[b + 3] = list[a];
    rocks[b + 4] = list[a + 1];
    rocks[b + 5] = list[a + 2];
    rocks[b + 6] = list[a + 4];
    rocks[b + 7] = list[a + 5];
    rocks[b + 8] = list[a + 6];
  }
  return { kind: 'rocks', id: job.id, center, rocks };
}

/** Same deterministic builder in workers and code-only diagnostics; no DOM or renderer required. */
export function buildTerrainJob(job: WorkerJob): WorkerResult {
  if (job.kind === 'sphere') return buildSphere(job);
  if (job.kind === 'tangent') return buildTangent(job);
  return buildRocks(job);
}

if (typeof self !== 'undefined') self.onmessage = (e: MessageEvent<WorkerJob>) => {
  const res = buildTerrainJob(e.data);
  const transfer: Transferable[] =
    res.kind === 'sphere'
      ? [res.positions.buffer, res.normals.buffer, res.surf.buffer, res.morph.buffer, res.morphN.buffer, res.morphS.buffer, res.tex.buffer, res.faceU.buffer]
      : res.kind === 'tangent'
        ? [res.heights.buffer, res.rocks.buffer]
        : [res.rocks.buffer];
  (self as unknown as Worker).postMessage(res, transfer);
};
