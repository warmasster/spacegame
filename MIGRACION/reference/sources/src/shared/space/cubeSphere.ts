// Cube-sphere parameterisation: six faces of a cube, each a square of parameters (a, b) in
// [-1, 1]², blown out onto the sphere. The tangent warp (tan(π/4 · a)) spreads the cells evenly
// (corner cells are only ~1.4× the centre ones instead of ~5×). Faces are wound so that u × v is
// the face's outward normal: a grid laid along a then b is counter-clockwise seen from outside.
//
// The same grid indexes everything that lives on a body's surface: terrain nodes (a quadtree per
// face), terrain modifiers (terrainMods/store.ts) and rock cells (space/rocks.ts). A cell of level
// L is one of 2^L × 2^L squares of a face.

export interface CubeFace {
  n: readonly [number, number, number];
  u: readonly [number, number, number];
  v: readonly [number, number, number];
}

export const CUBE_FACES: readonly CubeFace[] = [
  { n: [1, 0, 0], u: [0, 0, -1], v: [0, 1, 0] },
  { n: [-1, 0, 0], u: [0, 0, 1], v: [0, 1, 0] },
  { n: [0, 1, 0], u: [1, 0, 0], v: [0, 0, -1] },
  { n: [0, -1, 0], u: [1, 0, 0], v: [0, 0, 1] },
  { n: [0, 0, 1], u: [1, 0, 0], v: [0, 1, 0] },
  { n: [0, 0, -1], u: [-1, 0, 0], v: [0, 1, 0] },
];

/** Unit direction of face parameters (a, b) into `out`. */
export function cubeDir(face: number, a: number, b: number, out: number[]): number[] {
  const f = CUBE_FACES[face];
  const A = Math.tan(a * (Math.PI / 4));
  const B = Math.tan(b * (Math.PI / 4));
  const x = f.n[0] + A * f.u[0] + B * f.v[0];
  const y = f.n[1] + A * f.u[1] + B * f.v[1];
  const z = f.n[2] + A * f.u[2] + B * f.v[2];
  const l = Math.sqrt(x * x + y * y + z * z);
  out[0] = x / l;
  out[1] = y / l;
  out[2] = z / l;
  return out;
}

/** Where a direction falls on the cube: its face and parameters (a, b) in [-1, 1]. */
export interface FacePoint {
  face: number;
  a: number;
  b: number;
}

export const facePoint = (): FacePoint => ({ face: 0, a: 0, b: 0 });

/** Face and parameters of direction (x, y, z) (any length), into `out` (the inverse of `cubeDir`). */
export function faceOf(x: number, y: number, z: number, out: FacePoint): FacePoint {
  const ax = Math.abs(x);
  const ay = Math.abs(y);
  const az = Math.abs(z);
  let face: number;
  if (ax >= ay && ax >= az) face = x >= 0 ? 0 : 1;
  else if (ay >= az) face = y >= 0 ? 2 : 3;
  else face = z >= 0 ? 4 : 5;
  return paramsOn(face, x, y, z, out) ?? out;
}

/**
 * Parameters of direction (x, y, z) on a given face, into `out` (null: the direction points away from
 * that face). They may fall outside [-1, 1] when the direction belongs to a neighbouring face.
 */
export function paramsOn(face: number, x: number, y: number, z: number, out: FacePoint): FacePoint | null {
  const f = CUBE_FACES[face];
  const dn = x * f.n[0] + y * f.n[1] + z * f.n[2];
  if (dn <= 1e-12) return null;
  const A = (x * f.u[0] + y * f.u[1] + z * f.u[2]) / dn;
  const B = (x * f.v[0] + y * f.v[1] + z * f.v[2]) / dn;
  out.face = face;
  out.a = Math.atan(A) * (4 / Math.PI);
  out.b = Math.atan(B) * (4 / Math.PI);
  return out;
}

/** Arc length (m) of a span of `size` face parameters on a sphere of radius `r` (about: the warp keeps it even). */
export const cubeArc = (size: number, r: number) => (size / 2) * (Math.PI / 2) * r;

/** Width of a cell of level `level` (m, about) on a sphere of radius `r`. */
export const cellArc = (level: number, r: number) => cubeArc(2 / 2 ** level, r);

/** Deepest level whose cells are at least `m` metres wide on a sphere of radius `r` (0..maxLevel). */
export function levelFor(m: number, r: number, maxLevel = 24): number {
  const l = Math.floor(Math.log2(cubeArc(2, r) / Math.max(1e-6, m)));
  return l < 0 ? 0 : l > maxLevel ? maxLevel : l;
}

/** Cell index (0 .. 2^level − 1) of a face parameter at a level. */
export function cellOf(p: number, level: number): number {
  const n = 2 ** level;
  const i = Math.floor(((p + 1) / 2) * n);
  return i < 0 ? 0 : i >= n ? n - 1 : i;
}
