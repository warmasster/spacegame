// Loose boulders anywhere on a body: a deterministic scatter over the cube-sphere cells
// (cubeSphere.ts), one candidate per cell of each size class, so every point of the surface has the
// same rocks for every client, worker and physics tile, and nothing depends on where anyone is.
// Denser on fresh bright ejecta, none where a modifier cleared the ground (pads, trenches: the
// `rocks` mask of a surface sample). Size classes are data (SurfaceDef.rocks).
//
// Rocks are streamed in tiles: cells of the coarsest class's level, each holding whole cells of the
// finer classes (a rock belongs to exactly one tile).

import { CellRandom3 } from '../noise.js';
import { cellArc, cubeArc, cubeDir } from './cubeSphere.js';
import { surfaceSample, type BodySurface } from './surface.js';

export interface RockLevelDef {
  /** Cell size (m): about one candidate per cell. */
  cell: number;
  /** Chance of a rock in a cell (on average ground). */
  p: number;
  /** Size range (m, about the radius). */
  sMin: number;
  sMax: number;
}

/** A size class on a body: its cube-sphere level and salt. */
export interface RockLevel extends RockLevelDef {
  level: number;
  salt: number;
}

export const ROCK_VARIANTS = 6;

/** Numbers per rock in `rocksIn`'s output. */
export const ROCK_STRIDE = 7;

const levels = new WeakMap<BodySurface, RockLevel[]>();

/** The size classes of a surface, coarsest first, with their levels (cached). */
export function rockLevels(surface: BodySurface): RockLevel[] {
  let l = levels.get(surface);
  if (l) return l;
  // the level whose cells are closest to the class's cell size
  const face = cubeArc(2, surface.radius);
  l = (surface.def.rocks ?? []).map((r, i) => ({ ...r, level: Math.max(0, Math.round(Math.log2(face / r.cell))), salt: surface.seed * 17 + 101 + i }));
  l.sort((a, b) => a.level - b.level);
  levels.set(surface, l);
  return l;
}

/** Level of the rock tiles: the coarsest class's (-1: no rocks on this body). */
export function rockTileLevel(surface: BodySurface) {
  const l = rockLevels(surface);
  return l.length ? l[0].level : -1;
}

/** Width of a rock tile (m, about). */
export function rockTileArc(surface: BodySurface) {
  const t = rockTileLevel(surface);
  return t < 0 ? Infinity : cellArc(t, surface.radius);
}

const rnd = new CellRandom3();
const _dir = [0, 0, 0];
const _smp = surfaceSample();

/**
 * The rock of cell (i, j) of `face` in size class `L`, if there is one and it is between `minSize`
 * and `maxSize`: pushed onto `out` as [dx, dy, dz (unit direction), h (ground height, m over the
 * sphere), size, rot (yaw, rad), variant + jitter/2 packed as variant + jitter·0.5] — see ROCK_STRIDE.
 */
export function rockAt(surface: BodySurface, L: RockLevel, face: number, i: number, j: number, minSize: number, maxSize: number, out: number[]) {
  rnd.reset(i, j, face, L.salt);
  const n = 2 ** L.level;
  const a = ((i + rnd.next()) / n) * 2 - 1;
  const b = ((j + rnd.next()) / n) * 2 - 1;
  const pr = rnd.next();
  const u = rnd.next();
  const size = L.sMin + (L.sMax - L.sMin) * u * u * u;
  if (size < minSize || size > maxSize) return;
  const rot = rnd.next() * Math.PI * 2;
  const variant = Math.floor(rnd.next() * ROCK_VARIANTS);
  const jitter = rnd.next();
  // more rocks on fresh ground (bright ejecta), none where the ground was cleared
  const d = cubeDir(face, a, b, _dir);
  // the relief at the class's own scale (its albedo), every modifier at full detail (a pad clears
  // its boulders whatever their size)
  const s = surface.natural(d[0], d[1], d[2], L.cell * 0.5, _smp);
  if (surface.mods.count) surface.mods.apply(d[0], d[1], d[2], 0, s);
  const p = L.p * (0.55 + Math.max(0, s.albedo - 0.95) * 3.2) * s.rocks;
  if (pr > p) return;
  const h = surface.height(d);
  out.push(d[0], d[1], d[2], h, size, rot, variant + jitter * 0.5);
}

/**
 * Every rock of tile (ti, tj) of `face` (tile level: `rockTileLevel`) sized `minSize`..`maxSize`,
 * appended to `out` (ROCK_STRIDE numbers each).
 */
export function rocksInTile(surface: BodySurface, face: number, ti: number, tj: number, minSize: number, maxSize: number, out: number[]) {
  const ls = rockLevels(surface);
  if (!ls.length) return out;
  const T = ls[0].level;
  for (const L of ls) {
    if (L.sMax < minSize || L.sMin > maxSize) continue;
    const span = 2 ** (L.level - T);
    for (let j = tj * span; j < (tj + 1) * span; j++) for (let i = ti * span; i < (ti + 1) * span; i++) rockAt(surface, L, face, i, j, minSize, maxSize, out);
  }
  return out;
}
