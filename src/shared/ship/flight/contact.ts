// Ship ↔ ground contact: the landing gear (a sprung, damped leg per foot, with travel and a bump
// stop, feet that grip until the side force beats friction) and the hull itself (probe points
// round its underside that only touch when something went wrong: gear up, a crash, a steep
// slope). Everything comes from the ship data: legs from `gear.legs`, the resting sag from
// `floorHeight`, probes from the lowest vertices of the hull, the parts and the fairings.
//
// Cost: the ground is a procedural function (a few µs a sample), and a ship has hundreds of probes
// evaluated four times a step near the ground. So every probe remembers where it last sampled the
// ground and how high it was there; the ground can't have risen more than SLOPE × the distance
// moved since, so a probe well clear of that bound is skipped without sampling anything. Only the
// points that are touching (or about to) are sampled exactly, every substep, as before.

import { keelModules, type ShipDef } from '../def.js';
import { add, rotY, type V3 } from '../geom.js';
import type { Ground } from '../../space/tangent.js';
import type { MassProps } from './mass.js';
import type { ShipPose } from './pose.js';

/** The ground in the contact model's frame (a tangent frame of the body: space/tangent.ts). */
export type { Ground } from '../../space/tangent.js';

export const GEAR = {
  /** Leg compression under the ship's own weight (m). */
  sag: 0.07,
  /** Usable travel before the bump stop (m). */
  travel: 0.4,
  damping: 0.85,
  /** Friction of the feet (static grip / sliding). */
  grip: 0.95,
  slide: 0.6,
};

/** Steepest ground the height caches trust (rise over run): a probe further above its last sample than this allows is not sampled. */
const SLOPE = 4;
/** A cached clearance (the AGL) may be off by this share of itself, plus 5 mm. */
const REL = 0.02;
/** Above this (m) the height above the ground is taken from a spread subset of the probes. */
const HIGH = 8;
/** Probes in that subset. */
const SUBSET = 24;
/** A contact's ground normal is reused within this distance of where it was measured (m). */
const NORMAL_REUSE = 0.02;

export interface ContactResult {
  /** World force and torque about the centre of mass. */
  F: V3;
  T: V3;
  /** Feet carrying weight, hull points touching, the deepest hull penetration (m). */
  feet: number;
  hull: number;
  deepest: number;
  /** Total normal force (N). */
  load: number;
  /** Hardest touchdown this step (m/s into the ground). */
  impact: number;
}

/** Terrain normal from central differences. */
export function groundNormal(g: Ground, x: number, z: number, e = 0.3): V3 {
  const hx = g.height(x + e, z) - g.height(x - e, z);
  const hz = g.height(x, z + e) - g.height(x, z - e);
  const n: V3 = [-hx / (2 * e), 1, -hz / (2 * e)];
  const l = Math.hypot(n[0], n[1], n[2]);
  return [n[0] / l, n[1] / l, n[2] / l];
}

/**
 * Ground heights (and normals) last sampled under a set of moving points: `bound` says how low the
 * ground under a point can be now without sampling it, `height` samples it exactly (and remembers).
 */
class GroundCache {
  readonly x: Float64Array;
  readonly z: Float64Array;
  readonly h: Float64Array;
  readonly ok: Uint8Array;
  private nx: Float64Array;
  private nz: Float64Array;
  private ny: Float64Array;
  private nAtX: Float64Array;
  private nAtZ: Float64Array;
  private nOk: Uint8Array;
  private ground: Ground | null = null;
  private version: number | undefined = undefined;

  constructor(n: number) {
    this.x = new Float64Array(n);
    this.z = new Float64Array(n);
    this.h = new Float64Array(n);
    this.ok = new Uint8Array(n);
    this.nx = new Float64Array(n);
    this.ny = new Float64Array(n);
    this.nz = new Float64Array(n);
    this.nAtX = new Float64Array(n);
    this.nAtZ = new Float64Array(n);
    this.nOk = new Uint8Array(n);
  }

  /** Forget everything if the ground is another one or has changed. */
  use(g: Ground) {
    if (g === this.ground && g.version === this.version) return;
    this.ground = g;
    this.version = g.version;
    this.ok.fill(0);
    this.nOk.fill(0);
  }

  /** Exact ground height under point i at (x, z), remembered. */
  height(i: number, x: number, z: number) {
    const h = this.ground!.height(x, z);
    this.x[i] = x;
    this.z[i] = z;
    this.h[i] = h;
    this.ok[i] = 1;
    return h;
  }

  /** Highest the ground under point i can be at (x, z) from what it remembers (Infinity: nothing). */
  ceiling(i: number, x: number, z: number) {
    if (!this.ok[i]) return Infinity;
    const dx = x - this.x[i];
    const dz = z - this.z[i];
    return this.h[i] + SLOPE * Math.sqrt(dx * dx + dz * dz);
  }

  /**
   * Height for an AGL reading: the remembered one while it can't be off by more than REL of the
   * clearance `y` gives, else a fresh sample.
   */
  agl(i: number, x: number, z: number, y: number) {
    if (this.ok[i]) {
      const ex = x - this.x[i];
      const ez = z - this.z[i];
      const dd = Math.sqrt(ex * ex + ez * ez);
      if (SLOPE * dd <= 0.005 + REL * Math.max(0, y - this.h[i] - SLOPE * dd)) return this.h[i];
    }
    return this.height(i, x, z);
  }

  /** Ground normal under point i (measured again once the point moved NORMAL_REUSE away). */
  normal(i: number, x: number, z: number, out: V3) {
    if (!this.nOk[i] || Math.abs(x - this.nAtX[i]) + Math.abs(z - this.nAtZ[i]) > NORMAL_REUSE) {
      const n = groundNormal(this.ground!, x, z);
      this.nx[i] = n[0];
      this.ny[i] = n[1];
      this.nz[i] = n[2];
      this.nAtX[i] = x;
      this.nAtZ[i] = z;
      this.nOk[i] = 1;
    }
    out[0] = this.nx[i];
    out[1] = this.ny[i];
    out[2] = this.nz[i];
    return out;
  }
}

/** Hull probes of a definition (they never change): computed once per ship type. */
const probeCache = new WeakMap<ShipDef, V3[]>();

// scratch (single-threaded, one call at a time)
const _w: V3 = [0, 0, 0];
const _r: V3 = [0, 0, 0];
const _n: V3 = [0, 0, 0];

/** q ⊗ (x, y, z) into `out`. */
function rotate(q: readonly number[], x: number, y: number, z: number, out: V3) {
  const qx = q[0];
  const qy = q[1];
  const qz = q[2];
  const qw = q[3];
  const tx = 2 * (qy * z - qz * y);
  const ty = 2 * (qz * x - qx * z);
  const tz = 2 * (qx * y - qy * x);
  out[0] = x + qw * tx + (qy * tz - qz * ty);
  out[1] = y + qw * ty + (qz * tx - qx * tz);
  out[2] = z + qw * tz + (qx * ty - qy * tx);
  return out;
}

export class ContactModel {
  /** Hip (retracted foot) and fully extended foot of every leg, ship space. */
  readonly legs: Array<{ hip: V3; foot: V3 }>;
  /** Hull probe points, ship space. */
  readonly probes: V3[];
  /** Spring rate per leg for the ship's design mass. */
  private k = 0;
  private c = 0;
  private legCache: GroundCache;
  /** The drawn feet (the client passes its own ground: keep it apart from the flight's). */
  private drawCache: GroundCache;
  private probeCache: GroundCache;
  // the contact being summed (see touch)
  private cx = 0;
  private cy = 0;
  private cz = 0;
  private mEff = 0;
  private dt = 0;
  /** Probes the AGL is read from when high above the ground. */
  private subset: Int32Array;
  /** Last AGL reading was high (the subset is enough). */
  private high = false;
  private res: ContactResult = { F: [0, 0, 0], T: [0, 0, 0], feet: 0, hull: 0, deepest: 0, load: 0, impact: 0 };

  constructor(def: ShipDef, designMass: number) {
    const drop = def.floorHeight + GEAR.sag;
    this.legs = (def.gear?.legs ?? []).map((l) => ({ hip: [l[0], l[1], l[2]] as V3, foot: [l[0], -drop, l[2]] as V3 }));
    let probes = probeCache.get(def);
    if (!probes) probeCache.set(def, (probes = hullProbes(def)));
    this.probes = probes;
    this.legCache = new GroundCache(this.legs.length);
    this.drawCache = new GroundCache(this.legs.length);
    this.probeCache = new GroundCache(this.probes.length);
    const step = Math.max(1, Math.ceil(this.probes.length / SUBSET));
    this.subset = Int32Array.from({ length: Math.ceil(this.probes.length / step) }, (_, i) => i * step);
    this.setMass(designMass);
  }

  setMass(mass: number) {
    const n = Math.max(1, this.legs.length);
    this.k = (mass * 1.62) / n / GEAR.sag;
    this.c = 2 * GEAR.damping * Math.sqrt((this.k * mass) / n);
  }

  /** Foot position of leg i at gear travel 0 (up) … 1 (down), ship space. */
  foot(i: number, gear: number, out: V3 = [0, 0, 0]): V3 {
    const l = this.legs[i];
    const t = Math.max(0, Math.min(1, gear));
    out[0] = l.hip[0] + (l.foot[0] - l.hip[0]) * t;
    out[1] = l.hip[1] + (l.foot[1] - l.hip[1]) * t;
    out[2] = l.hip[2] + (l.foot[2] - l.hip[2]) * t;
    return out;
  }

  /**
   * Where each foot is drawn and collides (ship space): at its travel, pushed up the strut by the
   * ground under it (up to the bump stop) — a parked ship's legs stand on the terrain. `out` is
   * filled (and grown) in place when given.
   */
  feet(pose: ShipPose, ground: Ground, gear: number, out: V3[] = []): V3[] {
    const q = pose.q;
    const upY = 1 - 2 * (q[0] * q[0] + q[2] * q[2]);
    const cache = this.drawCache;
    cache.use(ground);
    for (let i = 0; i < this.legs.length; i++) {
      const f = (out[i] ??= [0, 0, 0]);
      this.foot(i, gear, f);
      const w = rotate(q, f[0], f[1], f[2], _w);
      const wx = w[0] + pose.p[0];
      const wy = w[1] + pose.p[1];
      const wz = w[2] + pose.p[2];
      // drawn feet only need millimetres: the last sample holds while the foot stays put
      const h = cache.agl(i, wx, wz, wy);
      // how far below the ground the free foot would be, along the ship's up
      const pen = (h - wy) * Math.max(0.2, upY);
      const rise = gear > 0.35 ? Math.max(0, Math.min(GEAR.travel, pen)) : 0;
      f[1] += rise;
    }
    out.length = this.legs.length;
    return out;
  }

  /**
   * Lowest clearance of the feet (gear down) or the hull above the ground: the height the displays
   * call AGL. High above the ground it is read from a spread subset of the hull probes (a few
   * percent off at worst, and nothing can touch up there).
   */
  clearance(pose: ShipPose, ground: Ground, gear: number): number {
    let best = Infinity;
    const q = pose.q;
    const p = pose.p;
    if (gear > 0.35) {
      const cache = this.legCache;
      cache.use(ground);
      for (let i = 0; i < this.legs.length; i++) {
        const f = this.foot(i, gear, _r);
        const w = rotate(q, f[0], f[1], f[2], _w);
        const wy = w[1] + p[1];
        const h = cache.agl(i, w[0] + p[0], w[2] + p[2], wy);
        if (wy - h < best) best = wy - h;
      }
    } else {
      const cache = this.probeCache;
      cache.use(ground);
      const probes = this.probes;
      const n = this.high ? this.subset.length : probes.length;
      for (let k = 0; k < n; k++) {
        const i = this.high ? this.subset[k] : k;
        const pr = probes[i];
        const w = rotate(q, pr[0], pr[1], pr[2], _w);
        const wy = w[1] + p[1];
        const h = cache.agl(i, w[0] + p[0], w[2] + p[2], wy);
        if (wy - h < best) best = wy - h;
      }
      // high enough: next time the subset will do; coming down, every probe again
      this.high = best > HIGH;
      if (!this.high && n < probes.length) return this.clearance(pose, ground, gear);
    }
    return Number.isFinite(best) ? best : p[1] - ground.height(p[0], p[2]);
  }

  /**
   * Contact forces for the current pose. `dt` is the substep (feet grip within a step), `brake`
   * locks the feet harder (parked). The result is reused: read it before the next call.
   */
  forces(pose: ShipPose, mp: MassProps, ground: Ground, gear: number, dt: number, brake: boolean): ContactResult {
    const res = this.res;
    const F = res.F;
    const T = res.T;
    F[0] = F[1] = F[2] = T[0] = T[1] = T[2] = 0;
    const q = pose.q;
    const p = pose.p;
    const v = pose.v;
    const wv = pose.w;
    const com = rotate(q, mp.com[0], mp.com[1], mp.com[2], _r);
    this.cx = p[0] + com[0];
    this.cy = p[1] + com[1];
    this.cz = p[2] + com[2];
    this.mEff = mp.mass / Math.max(1, this.legs.length);
    this.dt = dt;
    let feet = 0;
    let hull = 0;
    let deepest = 0;
    let load = 0;
    let impact = 0;
    const touch = this.touch;
    // gear legs: only when the legs are out
    if (gear > 0.35) {
      const cache = this.legCache;
      cache.use(ground);
      for (let i = 0; i < this.legs.length; i++) {
        const local = this.foot(i, gear, _r);
        const lw = rotate(q, local[0], local[1], local[2], _w);
        const wx = p[0] + lw[0];
        const wy = p[1] + lw[1];
        const wz = p[2] + lw[2];
        if (wy >= cache.ceiling(i, wx, wz)) continue;
        const h = cache.height(i, wx, wz);
        const pen = h - wy;
        if (pen <= 0) continue;
        const nrm = cache.normal(i, wx, wz, _n);
        // velocity of the foot: v + ω × (q ⊗ local)
        const vx = v[0] + wv[1] * lw[2] - wv[2] * lw[1];
        const vy = v[1] + wv[2] * lw[0] - wv[0] * lw[2];
        const vz = v[2] + wv[0] * lw[1] - wv[1] * lw[0];
        const vn = vx * nrm[0] + vy * nrm[1] + vz * nrm[2];
        impact = Math.max(impact, -vn);
        // spring with a bump stop at the end of the travel, damper on the closing speed
        const comp = pen * gear;
        const stop = Math.max(0, comp - GEAR.travel) * this.k * 25;
        const N = Math.max(0, this.k * Math.min(comp, GEAR.travel) + stop - this.c * vn);
        if (N <= 0) continue;
        feet++;
        load += N;
        touch(wx, h, wz, nrm, N, vx, vy, vz, vn, brake ? GEAR.grip * 1.3 : GEAR.grip, GEAR.slide);
      }
    }
    // hull probes: stiff and well damped, rough
    const kh = this.k * 30;
    const ch = this.c * 6;
    const cache = this.probeCache;
    cache.use(ground);
    const probes = this.probes;
    const cOk = cache.ok;
    const cX = cache.x;
    const cZ = cache.z;
    const cH = cache.h;
    const qx = q[0];
    const qy = q[1];
    const qz = q[2];
    const qw = q[3];
    for (let i = 0; i < probes.length; i++) {
      const local = probes[i];
      // q ⊗ local, inline: this loop runs for hundreds of probes four times a step
      const lx = local[0];
      const ly = local[1];
      const lz = local[2];
      const tx = 2 * (qy * lz - qz * ly);
      const ty = 2 * (qz * lx - qx * lz);
      const tz = 2 * (qx * ly - qy * lx);
      const wy = p[1] + ly + qw * ty + (qz * tx - qx * tz);
      const ox = lx + qw * tx + (qy * tz - qz * ty);
      const oz = lz + qw * tz + (qx * ty - qy * tx);
      const wx = p[0] + ox;
      const wz = p[2] + oz;
      if (cOk[i]) {
        // the ground can't be higher than its last sample plus SLOPE × the distance moved since
        const dx = wx - cX[i];
        const dz = wz - cZ[i];
        if (wy >= cH[i] + SLOPE * Math.sqrt(dx * dx + dz * dz)) continue;
      }
      const lw = rotate(q, lx, ly, lz, _w);
      const h = cache.height(i, wx, wz);
      const pen = h - wy;
      if (pen <= 0) continue;
      const nrm = cache.normal(i, wx, wz, _n);
      const vx = v[0] + wv[1] * lw[2] - wv[2] * lw[1];
      const vy = v[1] + wv[2] * lw[0] - wv[0] * lw[2];
      const vz = v[2] + wv[0] * lw[1] - wv[1] * lw[0];
      const vn = vx * nrm[0] + vy * nrm[1] + vz * nrm[2];
      impact = Math.max(impact, -vn);
      const N = Math.max(0, kh * pen - ch * vn);
      deepest = Math.max(deepest, pen);
      if (N <= 0) continue;
      hull++;
      load += N;
      touch(wx, h, wz, nrm, N, vx, vy, vz, vn, 0.7, 0.5);
    }
    res.feet = feet;
    res.hull = hull;
    res.deepest = deepest;
    res.load = load;
    res.impact = impact;
    return res;
  }

  /** One contact: normal force N along the ground normal plus friction on the slip, at (ax, h, az). */
  private touch = (ax: number, h: number, az: number, nrm: V3, N: number, vx: number, vy: number, vz: number, vn: number, grip: number, slide: number) => {
    const F = this.res.F;
    const T = this.res.T;
    let fx = nrm[0] * N;
    let fy = nrm[1] * N;
    let fz = nrm[2] * N;
    // friction: cancel the slip within the substep, up to static friction; beyond it, slide
    const tx = vx - nrm[0] * vn;
    const ty = vy - nrm[1] * vn;
    const tz = vz - nrm[2] * vn;
    const s = Math.sqrt(tx * tx + ty * ty + tz * tz);
    if (s >= 1e-6) {
      const want = (this.mEff * s) / this.dt;
      const f = want <= grip * N ? want : slide * N;
      const k = -f / s;
      fx += tx * k;
      fy += ty * k;
      fz += tz * k;
    }
    F[0] += fx;
    F[1] += fy;
    F[2] += fz;
    const rx = ax - this.cx;
    const ry = h - this.cy;
    const rz = az - this.cz;
    T[0] += ry * fz - rz * fy;
    T[1] += rz * fx - rx * fz;
    T[2] += rx * fy - ry * fx;
  };
}

/**
 * Probe points round the underside of the ship: the lowest vertices of the hull plates and the
 * bottom corners of every part and hull-colliding prop (nacelles, tanks, pods, the chin), thinned
 * on a 0.7 m grid. The boarding stairs and anything meant to reach the ground are left out.
 */
export function hullProbes(def: ShipDef): V3[] {
  const pts: V3[] = [];
  for (const pn of def.panels) {
    if (pn.kind === 'bulkhead') continue;
    for (const [x, y] of pn.poly) pts.push([pn.c[0] + pn.u[0] * x + pn.v[0] * y - pn.n[0] * pn.t * 0.5, pn.c[1] + pn.u[1] * x + pn.v[1] * y - pn.n[1] * pn.t * 0.5, pn.c[2] + pn.u[2] * x + pn.v[2] * y - pn.n[2] * pn.t * 0.5]);
  }
  // the keel tubs under every hull section (client/ship/view.ts draws them down to −0.45 m)
  for (const m of keelModules(def.modules)) {
    const hw = m.profile[m.profile.length - 1][0] + 0.1;
    const n = Math.max(1, Math.ceil((m.z1 - m.z0) / 0.7));
    for (let i = 0; i <= n; i++) for (const x of [-(hw - 0.25), 0, hw - 0.25]) pts.push([x, -0.45, m.z0 + ((m.z1 - m.z0) * i) / n]);
  }
  const floor = -def.floorHeight;
  const boxes =[...def.parts.filter((p) => p.shape !== 'none'), ...def.props.filter((p) => p.collide !== 'none' && p.c[1] - p.half[1] > floor + 0.05)];
  for (const b of boxes) for (const sx of [-1, 1]) for (const sz of [-1, 1]) pts.push(add(b.c, rotY([sx * b.half[0], -b.half[1], sz * b.half[2]], b.yaw)));
  // keep the lower part of the ship, one point per grid cell (the lowest)
  const minY = Math.min(...pts.map((p) => p[1]));
  const cells = new Map<string, V3>();
  for (const p of pts) {
    if (p[1] > minY + 1.4) continue;
    const key = `${Math.round(p[0] / 0.7)}:${Math.round(p[2] / 0.7)}`;
    const had = cells.get(key);
    if (!had || p[1] < had[1]) cells.set(key, p);
  }
  // the whole bounding box too (corners, edge midpoints, face centres): a ship on its side or upside
  // down still rests on its roof and flanks instead of sinking into the ground
  const lo: V3 = [Infinity, Infinity, Infinity];
  const hi: V3 = [-Infinity, -Infinity, -Infinity];
  for (const p of pts) for (let i = 0; i < 3; i++) {
    lo[i] = Math.min(lo[i], p[i]);
    hi[i] = Math.max(hi[i], p[i]);
  }
  const box: V3[] = [];
  const zs = Math.max(2, Math.ceil((hi[2] - lo[2]) / 1.5));
  for (let a = 0; a <= 2; a++) for (let b = 0; b <= 2; b++) for (let k = 0; k <= zs; k++) {
    // only the surface of the box
    if (a !== 0 && a !== 2 && b !== 0 && b !== 2 && k !== 0 && k !== zs) continue;
    box.push([lo[0] + ((hi[0] - lo[0]) * a) / 2, lo[1] + ((hi[1] - lo[1]) * b) / 2, lo[2] + ((hi[2] - lo[2]) * k) / zs]);
  }
  // the bottom face is already covered by the finer probes above
  return [...cells.values(), ...box.filter((p) => p[1] > lo[1] + 0.01)];
}
