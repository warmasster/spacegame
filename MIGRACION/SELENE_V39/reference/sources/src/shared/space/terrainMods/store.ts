// The modifiers of one body's surface and a spatial index over them, so that a ground sample only
// looks at the few that can reach it. Deterministic (the same list in the same order gives the same
// ground on every machine), allocation-free when sampling, usable in workers.
//
// Two layers, applied in this order:
// - static: what the world is built with (sites, space/sites.ts), rebuilt by every machine from the seed;
// - dynamic: what happens during the game (blast craters), kept by the server, bounded: repeated
//   blasts on the same spot deepen one crater instead of piling up, and past `max` the oldest go.
//
// Index: every modifier is filed in the cells of one cube-sphere level (cubeSphere.ts) whose cells
// are at least four times its reach: the cell of its centre and the eight round it, plus the same
// round any point of its rim that falls on another face. A sample looks up one cell per level in use.

import { cellOf, cubeArc, faceOf, facePoint } from '../cubeSphere.js';
import { angleBetween } from '../angle.js';
import type { SurfaceSample } from '../surface.js';
import { MOD_KINDS, type ModKind } from './kinds.js';
import type { ModPoint, TerrainMod } from './types.js';

/** The body a store lies on: what the index and the modifier frames need of it. */
export interface ModBody {
  radius: number;
  /** Rotation axis toward the north pole (unit): the modifiers' "north". */
  pole: readonly number[];
}

/** A modifier ready to apply: its frame at the centre, its reach, where it sits in the order. */
interface Live {
  mod: TerrainMod;
  kind: ModKind;
  order: number;
  dynamic: boolean;
  cx: number;
  cy: number;
  cz: number;
  ex: number;
  ey: number;
  ez: number;
  sx: number;
  sy: number;
  sz: number;
  reach: number;
  /** cos of the angle it reaches from its centre (a sample's quick rejection). */
  cosReach: number;
  feature: number;
  level: number;
}

/** Up to this many modifiers a sample just checks them all (cheaper than the index). */
const LINEAR = 16;
/** Deepest index level (cells of ~0.6 m on the Moon); smaller modifiers share its cells. */
const MAX_LEVEL = 22;

const _fp = facePoint();
const _fq = facePoint();
const _pt: ModPoint = { x: 0, z: 0, d: 0 };

export class TerrainMods {
  private live: Live[] = [];
  private statics = 0;
  private nextOrder = 0;
  /** Bumped on every change: whatever caches ground (flight contact, terrain nodes) compares it. */
  version = 0;
  // --- index: open addressing on (level·8 + face, i, j) → a chain of entries (modifier indices) ---------
  private hL = new Int32Array(0);
  private hI = new Int32Array(0);
  private hJ = new Int32Array(0);
  private hHead = new Int32Array(0);
  private hUsed = 0;
  private eMod = new Int32Array(64);
  private eNext = new Int32Array(64);
  private eUsed = 0;
  /** Levels with something filed in them (bit per level). */
  private levels = 0;
  private dirty = true;
  /** Scratch of a sample: the candidates, then sorted by order. */
  private hits = new Int32Array(64);
  /** Cells a modifier was filed in while filing it (no duplicates). */
  private filed: number[] = [];

  constructor(
    readonly body: ModBody,
    /** Most dynamic modifiers kept (a few more are allowed before a batch of the oldest goes). */
    readonly max = 4096,
  ) {}

  /** How many there are (both layers). */
  get count() {
    return this.live.length;
  }

  /** The static layer (what the world was built with). */
  get staticList(): TerrainMod[] {
    const out: TerrainMod[] = [];
    for (let i = 0; i < this.statics; i++) out.push(this.live[i].mod);
    return out;
  }

  /** The dynamic layer, oldest first (what the server sends a newcomer). */
  get dynamic(): TerrainMod[] {
    const out: TerrainMod[] = [];
    for (let i = this.statics; i < this.live.length; i++) out.push(this.live[i].mod);
    return out;
  }

  /** Replace the static layer (the dynamic one stays on top of it). */
  setStatic(list: readonly TerrainMod[]) {
    const dyn = this.live.slice(this.statics);
    this.live = [];
    this.nextOrder = 0;
    for (const m of list) this.live.push(this.compile(m, false));
    this.statics = this.live.length;
    for (const l of dyn) {
      l.order = this.nextOrder++;
      this.live.push(l);
    }
    this.changed(true);
  }

  /** Replace the dynamic layer (a newcomer's copy of the server's list; a worker's job). */
  setDynamic(list: readonly TerrainMod[]) {
    this.live.length = this.statics;
    this.nextOrder = this.statics;
    for (const m of list) this.live.push(this.compile(m, true));
    this.changed(true);
  }

  /** Drop the dynamic layer (a new world). */
  clearDynamic() {
    this.setDynamic([]);
  }

  /**
   * Add a dynamic modifier (its NaN parameters already filled). A crater on top of one of about the
   * same size deepens that one instead (the list stays short however often a spot is hit). Returns
   * the modifier that holds the change (the new one or the one it merged into).
   */
  add(mod: TerrainMod): TerrainMod {
    const merged = this.merge(mod);
    if (merged) {
      this.version++;
      return merged;
    }
    const l = this.compile(mod, true);
    this.live.push(l);
    const dyn = this.live.length - this.statics;
    if (dyn > this.max + (this.max >> 3)) {
      // a batch of the oldest go at once: the index is rebuilt once per batch, not per blast
      this.live.splice(this.statics, dyn - this.max);
      this.changed(true);
    } else if (!this.dirty && this.live.length - 1 > LINEAR) {
      // the index holds every other one already: file just this one
      this.file(this.live.length - 1);
      this.version++;
    } else this.changed(true);
    return mod;
  }

  /** Same kind, centre within 30 % of its radius, radius within 30 %: deepen the old one. */
  private merge(mod: TerrainMod): TerrainMod | null {
    if (mod.kind !== 'crater') return null;
    const R = this.body.radius;
    const c = mod.center;
    for (let i = this.live.length - 1; i >= this.statics; i--) {
      const l = this.live[i];
      const o = l.mod;
      if (o.kind !== mod.kind || o.body !== mod.body || Math.abs(o.radius - mod.radius) > 0.3 * mod.radius) continue;
      const d = angleBetween(c[0], c[1], c[2], l.cx, l.cy, l.cz) * R;
      if (d > 0.3 * mod.radius) continue;
      const p = (o.params ??= [1]);
      p[0] = Math.min(3, (p[0] ?? 1) + 0.5 * (mod.params?.[0] ?? 1));
      return o;
    }
    return null;
  }

  /**
   * Modifiers that can change the ground within `radius` m of direction `d` at a detail of
   * `minFeature` (a worker job's list), in order; plus, `margin` m further out, those at least
   * `marginFeature` m big (they still shade it).
   */
  near(d: readonly number[], radius: number, minFeature: number, out: TerrainMod[] = [], margin = 0, marginFeature = Infinity): TerrainMod[] {
    const R = this.body.radius;
    for (const l of this.live) {
      if (l.feature * 2 < minFeature * 1.5) continue;
      const dist = angleBetween(d[0], d[1], d[2], l.cx, l.cy, l.cz) * R - l.reach;
      if (dist <= radius || (dist <= radius + margin && l.feature >= marginFeature)) out.push(l.mod);
    }
    return out;
  }

  /**
   * Apply every modifier that reaches direction (dx, dy, dz) (unit) to a sample, in order. Details
   * smaller than `minFeature` are left out, like the procedural ones.
   */
  apply(dx: number, dy: number, dz: number, minFeature: number, out: SurfaceSample) {
    const n = this.gather(dx, dy, dz, minFeature);
    if (!n) return out;
    const R = this.body.radius;
    const hits = this.hits;
    const pt = _pt;
    for (let k = 0; k < n; k++) {
      const l = this.live[hits[k]];
      const dot = dx * l.cx + dy * l.cy + dz * l.cz;
      // gnomonic projection on the tangent plane at its centre (exact for its small size)
      const inv = R / dot;
      pt.x = (dx * l.ex + dy * l.ey + dz * l.ez) * inv;
      pt.z = (dx * l.sx + dy * l.sy + dz * l.sz) * inv;
      pt.d = Math.sqrt(pt.x * pt.x + pt.z * pt.z);
      if (pt.d > l.reach) continue;
      l.kind.apply(l.mod, pt, out);
    }
    return out;
  }

  /** Candidates for a sample into `hits`, sorted by order; how many. */
  private gather(dx: number, dy: number, dz: number, minFeature: number): number {
    const live = this.live;
    const total = live.length;
    if (!total) return 0;
    let n = 0;
    if (total <= LINEAR) {
      for (let i = 0; i < total; i++) {
        const l = live[i];
        if (l.feature * 2 < minFeature * 1.5) continue;
        if (dx * l.cx + dy * l.cy + dz * l.cz < l.cosReach) continue;
        this.hits[n++] = i;
      }
      return n;
    }
    if (this.dirty) this.rebuild();
    const fp = faceOf(dx, dy, dz, _fp);
    const lf0 = fp.face;
    let levels = this.levels;
    for (let level = 0; levels; level++, levels >>>= 1) {
      if (!(levels & 1)) continue;
      const slot = this.find(level * 8 + lf0, cellOf(fp.a, level), cellOf(fp.b, level));
      if (slot < 0) continue;
      for (let e = this.hHead[slot]; e >= 0; e = this.eNext[e]) {
        const i = this.eMod[e];
        const l = live[i];
        if (l.feature * 2 < minFeature * 1.5) continue;
        if (dx * l.cx + dy * l.cy + dz * l.cz < l.cosReach) continue;
        if (n >= this.hits.length) {
          const h = new Int32Array(this.hits.length * 2);
          h.set(this.hits);
          this.hits = h;
        }
        this.hits[n++] = i;
      }
    }
    // in list order (static first, then by arrival): insertion sort, the lists are tiny
    const hits = this.hits;
    for (let a = 1; a < n; a++) {
      const v = hits[a];
      const o = live[v].order;
      let b = a - 1;
      while (b >= 0 && live[hits[b]].order > o) {
        hits[b + 1] = hits[b];
        b--;
      }
      hits[b + 1] = v;
    }
    return n;
  }

  private compile(mod: TerrainMod, dynamic: boolean): Live {
    const kind = MOD_KINDS[mod.kind];
    if (!kind) throw new Error(`terrain modifier of unknown kind "${mod.kind}"`);
    const R = this.body.radius;
    let [cx, cy, cz] = mod.center;
    const cl = Math.sqrt(cx * cx + cy * cy + cz * cz) || 1;
    cx /= cl;
    cy /= cl;
    cz /= cl;
    // east = pole × up; south = east × up (x east, y up, z south: the tangent frames of space/tangent.ts)
    const pl = this.body.pole;
    let ex = pl[1] * cz - pl[2] * cy;
    let ey = pl[2] * cx - pl[0] * cz;
    let ez = pl[0] * cy - pl[1] * cx;
    let el = Math.sqrt(ex * ex + ey * ey + ez * ez);
    if (el < 1e-9) {
      ex = cy;
      ey = -cx;
      ez = 0;
      el = Math.sqrt(ex * ex + ey * ey) || 1;
    }
    ex /= el;
    ey /= el;
    ez /= el;
    const reach = kind.reach(mod);
    const ang = Math.min(Math.PI, reach / R);
    return {
      mod,
      kind,
      order: this.nextOrder++,
      dynamic,
      cx,
      cy,
      cz,
      ex,
      ey,
      ez,
      sx: ey * cz - ez * cy,
      sy: ez * cx - ex * cz,
      sz: ex * cy - ey * cx,
      reach,
      cosReach: Math.cos(ang),
      feature: kind.feature(mod),
      level: Math.min(MAX_LEVEL, Math.max(0, Math.floor(Math.log2(cubeArc(2, R) / (4 * Math.max(1e-3, reach)))))),
    };
  }

  private changed(reindex: boolean) {
    if (reindex) this.dirty = true;
    this.version++;
  }

  // --- the index ------------------------------------------------------------------------------------

  private rebuild() {
    this.dirty = false;
    const cap = Math.max(64, 1 << Math.ceil(Math.log2(this.live.length * 24 + 1)));
    if (this.hL.length !== cap) {
      this.hL = new Int32Array(cap);
      this.hI = new Int32Array(cap);
      this.hJ = new Int32Array(cap);
      this.hHead = new Int32Array(cap);
    }
    this.hL.fill(-1);
    this.hUsed = 0;
    this.eUsed = 0;
    this.levels = 0;
    if (this.live.length <= LINEAR) return;
    for (let i = 0; i < this.live.length; i++) this.file(i);
  }

  /** File modifier `i` in the cells round its centre and round its rim where it crosses a face edge. */
  private file(i: number) {
    const l = this.live[i];
    this.levels |= 1 << l.level;
    const filed = this.filed;
    filed.length = 0;
    const cf = faceOf(l.cx, l.cy, l.cz, _fp);
    const face0 = cf.face;
    this.block(i, l.level, face0, cf.a, cf.b);
    // a rim that crosses onto another face: the same block round it there
    const R = this.body.radius;
    const t = Math.tan(Math.min(1.4, (l.reach * 1.05) / R));
    for (let k = 0; k < 16; k++) {
      const a = (k / 16) * Math.PI * 2;
      const ca = Math.cos(a) * t;
      const sa = Math.sin(a) * t;
      const x = l.cx + l.ex * ca + l.sx * sa;
      const y = l.cy + l.ey * ca + l.sy * sa;
      const z = l.cz + l.ez * ca + l.sz * sa;
      const f = faceOf(x, y, z, _fq);
      if (f.face === face0) continue;
      this.block(i, l.level, f.face, f.a, f.b);
    }
  }

  /** File modifier `i` in the 3×3 cells of `level` round face point (a, b). */
  private block(i: number, level: number, face: number, a: number, b: number) {
    const n = 2 ** level;
    const ci = cellOf(a, level);
    const cj = cellOf(b, level);
    const lf = level * 8 + face;
    for (let dj = -1; dj <= 1; dj++) {
      const j = cj + dj;
      if (j < 0 || j >= n) continue;
      for (let di = -1; di <= 1; di++) {
        const ii = ci + di;
        if (ii < 0 || ii >= n) continue;
        // once per cell (two blocks of one modifier can overlap)
        let seen = false;
        const f = this.filed;
        for (let k = 0; k < f.length; k += 3) {
          if (f[k] === lf && f[k + 1] === ii && f[k + 2] === j) {
            seen = true;
            break;
          }
        }
        if (seen) continue;
        f.push(lf, ii, j);
        this.insert(lf, ii, j, i);
      }
    }
  }

  private hash(lf: number, i: number, j: number) {
    let h = Math.imul(i, 0x9e3779b1) ^ Math.imul(j, 0x85ebca77) ^ Math.imul(lf, 0xc2b2ae3d);
    h = Math.imul(h ^ (h >>> 15), 0x2c1b3c6d);
    return (h ^ (h >>> 13)) >>> 0;
  }

  private find(lf: number, i: number, j: number): number {
    const mask = this.hL.length - 1;
    if (mask < 0) return -1;
    for (let s = this.hash(lf, i, j) & mask; ; s = (s + 1) & mask) {
      const k = this.hL[s];
      if (k === -1) return -1;
      if (k === lf && this.hI[s] === i && this.hJ[s] === j) return s;
    }
  }

  private insert(lf: number, i: number, j: number, mod: number) {
    if ((this.hUsed + 1) * 2 > this.hL.length) {
      this.grow();
    }
    const mask = this.hL.length - 1;
    let s = this.hash(lf, i, j) & mask;
    for (; ; s = (s + 1) & mask) {
      const k = this.hL[s];
      if (k === -1) {
        this.hL[s] = lf;
        this.hI[s] = i;
        this.hJ[s] = j;
        this.hHead[s] = -1;
        this.hUsed++;
        break;
      }
      if (k === lf && this.hI[s] === i && this.hJ[s] === j) break;
    }
    if (this.eUsed >= this.eMod.length) {
      const m = new Int32Array(this.eMod.length * 2);
      m.set(this.eMod);
      this.eMod = m;
      const nx = new Int32Array(this.eNext.length * 2);
      nx.set(this.eNext);
      this.eNext = nx;
    }
    const e = this.eUsed++;
    this.eMod[e] = mod;
    this.eNext[e] = this.hHead[s];
    this.hHead[s] = e;
  }

  /** Twice the slots, every cell moved over (its chain of entries goes with it). */
  private grow() {
    const oL = this.hL;
    const oI = this.hI;
    const oJ = this.hJ;
    const oH = this.hHead;
    const cap = Math.max(64, oL.length * 2);
    this.hL = new Int32Array(cap).fill(-1);
    this.hI = new Int32Array(cap);
    this.hJ = new Int32Array(cap);
    this.hHead = new Int32Array(cap);
    const mask = cap - 1;
    for (let s = 0; s < oL.length; s++) {
      if (oL[s] === -1) continue;
      let t = this.hash(oL[s], oI[s], oJ[s]) & mask;
      while (this.hL[t] !== -1) t = (t + 1) & mask;
      this.hL[t] = oL[s];
      this.hI[t] = oI[s];
      this.hJ[t] = oJ[s];
      this.hHead[t] = oH[s];
    }
  }
}

/**
 * The ground a modifier's NaN parameters are measured from, at a point of its frame: `sample(d)`
 * gives the height above the sphere toward unit direction d.
 */
export function resolveMod(mod: TerrainMod, body: ModBody, sample: (d: readonly number[]) => number) {
  const kind = MOD_KINDS[mod.kind];
  if (!kind?.resolve) return mod;
  const c = mod.center;
  const pl = body.pole;
  let ex = pl[1] * c[2] - pl[2] * c[1];
  let ey = pl[2] * c[0] - pl[0] * c[2];
  let ez = pl[0] * c[1] - pl[1] * c[0];
  let el = Math.sqrt(ex * ex + ey * ey + ez * ez);
  if (el < 1e-9) {
    ex = c[1];
    ey = -c[0];
    ez = 0;
    el = Math.sqrt(ex * ex + ey * ey) || 1;
  }
  ex /= el;
  ey /= el;
  ez /= el;
  const sx = ey * c[2] - ez * c[1];
  const sy = ez * c[0] - ex * c[2];
  const sz = ex * c[1] - ey * c[0];
  const R = body.radius;
  const d = [0, 0, 0];
  kind.resolve(mod, (x, z) => {
    d[0] = c[0] + (ex * x + sx * z) / R;
    d[1] = c[1] + (ey * x + sy * z) / R;
    d[2] = c[2] + (ez * x + sz * z) / R;
    const l = Math.hypot(d[0], d[1], d[2]);
    d[0] /= l;
    d[1] /= l;
    d[2] /= l;
    return sample(d);
  });
  return mod;
}

/** Any modifier centred at world point `p` on a body's surface; no preferred world axis. */
export function terrainModAt(kind: string, body: string, center: readonly number[], p: readonly number[], radius: number): TerrainMod {
  const x = p[0] - center[0];
  const y = p[1] - center[1];
  const z = p[2] - center[2];
  const l = Math.sqrt(x * x + y * y + z * z) || 1;
  // a unit direction to 1e-9 (under 2 mm on the Moon): the same number on every machine
  const r9 = (v: number) => Math.round(v * 1e9) / 1e9;
  return { kind, body, center: [r9(x / l), r9(y / l), r9(z / l)], radius, seed: Math.round(x * 17 + z * 31) % 100000 };
}

/** A crater modifier where a blast hit world point `p` of a body (centre `center`). */
export function blastCrater(body: string, center: readonly number[], p: readonly number[], radius: number, depth = 1): TerrainMod {
  const m = terrainModAt('crater', body, center, p, radius);
  // a fresh blast is what the kind's defaults are: only a different depth is carried
  if (depth !== 1) m.params = [depth];
  return m;
}

/** The same test the index makes, for anyone that has a modifier and a direction (unit). */
export function modTouches(mod: TerrainMod, body: ModBody, d: readonly number[], radius: number): boolean {
  const k = MOD_KINDS[mod.kind];
  const reach = k ? k.reach(mod) : mod.radius;
  const c = mod.center;
  return angleBetween(d[0], d[1], d[2], c[0], c[1], c[2]) * body.radius <= reach + radius;
}
