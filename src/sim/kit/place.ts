// Where things are (docs/MUNDO.md §10): a point in the world (host 0: world coordinates, float64)
// or in a host (a ship, a station: its own space, which moves with it), and a grid to find what is
// near a point without walking every thing. The grid is derived (rebuilt from the table on first
// use, kept by `place`): not state, never saved; answers are sorted by id (deterministic).

import { defineComponent, type Table } from '../core/store.js';
import type { World } from '../core/world.js';

export const Place = defineComponent('$place', {
  host: 'u32',
  x: 'f64',
  y: 'f64',
  z: 'f64',
  qx: 'f32',
  qy: 'f32',
  qz: 'f32',
  qw: { type: 'f32', default: 1 },
});

export type V3 = [number, number, number];
export type Q4 = [number, number, number, number];

const CELL = 256;

class PlaceIndex {
  private readonly cells = new Map<string, Set<number>>();
  private readonly keyOf = new Map<number, string>();

  constructor(t: Table<typeof Place.fields>) {
    for (let r = 0; r < t.count; r++) this.set(t.ids[r], t.c.host[r], t.c.x[r], t.c.y[r], t.c.z[r]);
  }

  set(id: number, host: number, x: number, y: number, z: number): void {
    const k = key(host, Math.floor(x / CELL), Math.floor(y / CELL), Math.floor(z / CELL));
    const old = this.keyOf.get(id);
    if (old === k) return;
    if (old !== undefined) this.cells.get(old)?.delete(id);
    this.keyOf.set(id, k);
    let s = this.cells.get(k);
    if (!s) this.cells.set(k, (s = new Set()));
    s.add(id);
  }

  candidates(host: number, p: readonly number[], r: number, out: number[]): void {
    const [x0, x1] = [Math.floor((p[0] - r) / CELL), Math.floor((p[0] + r) / CELL)];
    const [y0, y1] = [Math.floor((p[1] - r) / CELL), Math.floor((p[1] + r) / CELL)];
    const [z0, z1] = [Math.floor((p[2] - r) / CELL), Math.floor((p[2] + r) / CELL)];
    for (let x = x0; x <= x1; x++)
      for (let y = y0; y <= y1; y++)
        for (let z = z0; z <= z1; z++) {
          const s = this.cells.get(key(host, x, y, z));
          if (s) for (const id of s) out.push(id);
        }
  }

  forget(id: number): void {
    const k = this.keyOf.get(id);
    if (k === undefined) return;
    this.cells.get(k)?.delete(id);
    this.keyOf.delete(id);
  }
}

const key = (host: number, x: number, y: number, z: number) => `${host}:${x},${y},${z}`;
const indexes = new WeakMap<World, PlaceIndex>();

function index(w: World): PlaceIndex {
  let i = indexes.get(w);
  if (!i) indexes.set(w, (i = new PlaceIndex(w.table(Place))));
  return i;
}

/** Puts a thing somewhere (a point of host `host`'s space; 0: the world) with an orientation. */
export function place(w: World, id: number, host: number, p: readonly number[], q?: readonly number[]): void {
  const t = w.table(Place);
  const r = t.add(id);
  const c = t.c;
  c.host[r] = host;
  c.x[r] = p[0];
  c.y[r] = p[1];
  c.z[r] = p[2];
  if (q) {
    c.qx[r] = q[0];
    c.qy[r] = q[1];
    c.qz[r] = q[2];
    c.qw[r] = q[3];
  }
  index(w).set(id, host, p[0], p[1], p[2]);
}

/** Where a thing is (null: nowhere). */
export function placeOf(w: World, id: number): { host: number; p: V3; q: Q4 } | null {
  const t = w.table(Place);
  const r = t.row(id);
  if (r < 0) return null;
  const c = t.c;
  return { host: c.host[r], p: [c.x[r], c.y[r], c.z[r]], q: [c.qx[r], c.qy[r], c.qz[r], c.qw[r]] };
}

/** Things within `r` m of point `p` of host `host`'s space (0: the world), by id. */
export function near(w: World, host: number, p: readonly number[], r: number): number[] {
  const t = w.table(Place);
  const found: number[] = [];
  const idx = index(w);
  idx.candidates(host, p, r, found);
  const out: number[] = [];
  const c = t.c;
  for (const id of found) {
    const row = t.row(id);
    // gone since (despawned): the grid forgets it
    if (row < 0) {
      idx.forget(id);
      continue;
    }
    if (c.host[row] !== host) continue;
    const dx = c.x[row] - p[0];
    const dy = c.y[row] - p[1];
    const dz = c.z[row] - p[2];
    if (dx * dx + dy * dy + dz * dz <= r * r) out.push(id);
  }
  return out.sort((a, b) => a - b);
}
