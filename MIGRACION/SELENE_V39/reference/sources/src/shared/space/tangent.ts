// Tangent frames on a body: a flat, y-up frame laid on its sphere at some point (x east, y up away
// from the centre, z south — at the base these are the world's own axes), and the body's ground
// seen from one of them as a height field. Whatever works in a flat y-up world (the ship contact
// model, the placement of ships and crates, sites and their pads, the physics bubble's tiles) works
// anywhere round the body through one of these.

import type { BodySurface } from './surface.js';

/** What a frame needs of its body. */
export interface FrameBody {
  center: readonly number[];
  radius: number;
  /** Rotation axis toward the north pole (unit). */
  pole: readonly number[];
}

/** A height field in some frame: the ground as the contact model and the placements see it. */
export interface Ground {
  height(x: number, z: number): number;
  /** Bumped whenever the ground changes (a crater, the frame laid elsewhere): cached heights go stale. */
  readonly version?: number;
}

type P3 = number[];
interface PoseLike {
  p: P3;
  q: P3;
  v: P3;
  w: P3;
}

export class TangentFrame {
  /** Origin (world, on the body's mean sphere unless laid otherwise) and axes (world, unit). */
  readonly o: P3 = [0, 0, 0];
  readonly e: P3 = [1, 0, 0];
  readonly u: P3 = [0, 1, 0];
  readonly s: P3 = [0, 0, 1];
  /** Bumped every time the frame is laid again. */
  laid = 0;

  constructor(readonly body: FrameBody) {}

  /** Lay the frame over unit direction `d` (from the centre), its origin `r` m from the centre (the mean sphere by default). */
  layDir(d: readonly number[], r = this.body.radius) {
    const c = this.body.center;
    const u = this.u;
    u[0] = d[0];
    u[1] = d[1];
    u[2] = d[2];
    this.o[0] = c[0] + u[0] * r;
    this.o[1] = c[1] + u[1] * r;
    this.o[2] = c[2] + u[2] * r;
    tangentAxes(this.body.pole, u, this.e, this.s);
    this.laid++;
    return this;
  }

  /** Lay the frame under world point `p` (origin on the mean sphere). */
  layAt(p: readonly number[]) {
    const c = this.body.center;
    const x = p[0] - c[0];
    const y = p[1] - c[1];
    const z = p[2] - c[2];
    const l = Math.sqrt(x * x + y * y + z * z) || 1;
    _d[0] = x / l;
    _d[1] = y / l;
    _d[2] = z / l;
    return this.layDir(_d);
  }

  /** Frame point → world (into `out`). */
  toWorld(x: number, y: number, z: number, out: P3): P3 {
    const o = this.o;
    const e = this.e;
    const u = this.u;
    const s = this.s;
    out[0] = o[0] + e[0] * x + u[0] * y + s[0] * z;
    out[1] = o[1] + e[1] * x + u[1] * y + s[1] * z;
    out[2] = o[2] + e[2] * x + u[2] * y + s[2] * z;
    return out;
  }

  /** World point → frame (into `out`). */
  toLocal(p: readonly number[], out: P3): P3 {
    const dx = p[0] - this.o[0];
    const dy = p[1] - this.o[1];
    const dz = p[2] - this.o[2];
    out[0] = dx * this.e[0] + dy * this.e[1] + dz * this.e[2];
    out[1] = dx * this.u[0] + dy * this.u[1] + dz * this.u[2];
    out[2] = dx * this.s[0] + dy * this.s[1] + dz * this.s[2];
    return out;
  }

  /** World vector → frame (into `out`). */
  vecIn(v: readonly number[], out: P3): P3 {
    const x = v[0] * this.e[0] + v[1] * this.e[1] + v[2] * this.e[2];
    const y = v[0] * this.u[0] + v[1] * this.u[1] + v[2] * this.u[2];
    const z = v[0] * this.s[0] + v[1] * this.s[1] + v[2] * this.s[2];
    out[0] = x;
    out[1] = y;
    out[2] = z;
    return out;
  }

  /** Frame vector → world (into `out`). */
  vecOut(v: readonly number[], out: P3): P3 {
    const x = this.e[0] * v[0] + this.u[0] * v[1] + this.s[0] * v[2];
    const y = this.e[1] * v[0] + this.u[1] * v[1] + this.s[1] * v[2];
    const z = this.e[2] * v[0] + this.u[2] * v[1] + this.s[2] * v[2];
    out[0] = x;
    out[1] = y;
    out[2] = z;
    return out;
  }

  /** The frame's rotation in the world (unit quaternion x, y, z, w into `out`): columns e, u, s. */
  quat(out: P3): P3 {
    return basisQuat(this.e, this.u, this.s, out);
  }

  /** A pose (world) in the frame, into `out`. */
  poseIn<T extends PoseLike>(pose: PoseLike, out: T): T {
    _t[0] = pose.p[0] - this.o[0];
    _t[1] = pose.p[1] - this.o[1];
    _t[2] = pose.p[2] - this.o[2];
    this.vecIn(_t, out.p);
    this.vecIn(pose.v, out.v);
    this.vecIn(pose.w, out.w);
    // q_frame = F⁻¹ ⊗ q
    const f = this.quat(_fq);
    const ax = -f[0];
    const ay = -f[1];
    const az = -f[2];
    const aw = f[3];
    const bx = pose.q[0];
    const by = pose.q[1];
    const bz = pose.q[2];
    const bw = pose.q[3];
    out.q[0] = aw * bx + ax * bw + ay * bz - az * by;
    out.q[1] = aw * by - ax * bz + ay * bw + az * bx;
    out.q[2] = aw * bz + ax * by - ay * bx + az * bw;
    out.q[3] = aw * bw - ax * bx - ay * by - az * bz;
    return out;
  }

  /** q_world = F ⊗ q (a rotation given in the frame, into `out`). */
  rotOut(q: readonly number[], out: P3): P3 {
    const f = this.quat(_fq);
    const ax = f[0];
    const ay = f[1];
    const az = f[2];
    const aw = f[3];
    const bx = q[0];
    const by = q[1];
    const bz = q[2];
    const bw = q[3];
    out[0] = aw * bx + ax * bw + ay * bz - az * by;
    out[1] = aw * by - ax * bz + ay * bw + az * bx;
    out[2] = aw * bz + ax * by - ay * bx + az * bw;
    out[3] = aw * bw - ax * bx - ay * by - az * bz;
    return out;
  }
}

/** Metres the frame of a SurfaceGround may be from what it serves before it is laid again. */
const REANCHOR = 1500;

/**
 * A body's ground (its surface with every modifier) seen from a tangent frame: a height field over
 * the frame's plane, like a flat world's. The contact model of the ships, the placement of ships and
 * crates and the sites' pads use it; `anchor` keeps the frame under whatever it serves.
 */
export class SurfaceGround extends TangentFrame implements Ground {
  private anchored = false;
  private _g: P3 = [0, 0, 0];

  constructor(
    body: FrameBody,
    readonly surface: BodySurface,
  ) {
    super(body);
  }

  /** Changes with the frame and with the surface's modifiers. */
  get version() {
    return this.laid * 2097152 + this.surface.version;
  }

  /** Lay the frame under world point `p` if it is not laid yet or `p` wandered off it. */
  anchor(p: readonly number[]) {
    if (this.anchored) {
      const dx = p[0] - this.o[0];
      const dy = p[1] - this.o[1];
      const dz = p[2] - this.o[2];
      const up = dx * this.u[0] + dy * this.u[1] + dz * this.u[2];
      if (dx * dx + dy * dy + dz * dz - up * up < REANCHOR * REANCHOR) return this;
    }
    this.anchored = true;
    this.layAt(p);
    return this;
  }

  /** Height of the ground over the frame's plane at frame (x, z), `minFeature` (m) its level of detail. */
  height(x: number, z: number, minFeature = 0): number {
    const d = this.dirAt(x, z, this._g);
    const r = this.body.radius + this.surface.height(d, minFeature);
    const c = this.body.center;
    // the ground point, and how high it stands over the plane
    return (c[0] + d[0] * r - this.o[0]) * this.u[0] + (c[1] + d[1] * r - this.o[1]) * this.u[1] + (c[2] + d[2] * r - this.o[2]) * this.u[2];
  }

  /** The ground's world point under frame (x, z) (straight toward the centre), into `out`. */
  point(x: number, z: number, out: P3): P3 {
    const d = this.dirAt(x, z, out);
    const r = this.body.radius + this.surface.height(d);
    const c = this.body.center;
    out[0] = c[0] + d[0] * r;
    out[1] = c[1] + d[1] * r;
    out[2] = c[2] + d[2] * r;
    return out;
  }

  /** Unit direction from the centre through the plane's point (x, z), into `out`. */
  dirAt(x: number, z: number, out: P3): P3 {
    const c = this.body.center;
    const px = this.o[0] + this.e[0] * x + this.s[0] * z - c[0];
    const py = this.o[1] + this.e[1] * x + this.s[1] * z - c[1];
    const pz = this.o[2] + this.e[2] * x + this.s[2] * z - c[2];
    const l = Math.sqrt(px * px + py * py + pz * pz);
    out[0] = px / l;
    out[1] = py / l;
    out[2] = pz / l;
    return out;
  }
}

/** East and south on the horizon of up direction `u` (unit), into `e` and `s`: east = pole × up, south = east × up. */
export function tangentAxes(pole: readonly number[], u: readonly number[], e: P3, s: P3) {
  let ex = pole[1] * u[2] - pole[2] * u[1];
  let ey = pole[2] * u[0] - pole[0] * u[2];
  let ez = pole[0] * u[1] - pole[1] * u[0];
  let el = Math.sqrt(ex * ex + ey * ey + ez * ez);
  if (el < 1e-9) {
    // at a pole any horizontal axis will do
    ex = u[1];
    ey = -u[0];
    ez = 0;
    el = Math.sqrt(ex * ex + ey * ey) || 1;
    if (el < 1e-9) {
      ex = 1;
      el = 1;
    }
  }
  e[0] = ex / el;
  e[1] = ey / el;
  e[2] = ez / el;
  s[0] = e[1] * u[2] - e[2] * u[1];
  s[1] = e[2] * u[0] - e[0] * u[2];
  s[2] = e[0] * u[1] - e[1] * u[0];
}

/**
 * Unit direction of a point `x` m east and `z` m south of unit direction `c` on a sphere of radius
 * `r` (a point of the tangent plane there, taken straight toward the centre), into `out`.
 */
export function offsetDir(pole: readonly number[], c: readonly number[], x: number, z: number, r: number, out: P3): P3 {
  tangentAxes(pole, c, _e, _s);
  const px = c[0] * r + _e[0] * x + _s[0] * z;
  const py = c[1] * r + _e[1] * x + _s[1] * z;
  const pz = c[2] * r + _e[2] * x + _s[2] * z;
  const l = Math.sqrt(px * px + py * py + pz * pz);
  out[0] = px / l;
  out[1] = py / l;
  out[2] = pz / l;
  return out;
}

/** Quaternion (x, y, z, w into `out`) of the rotation whose columns are the unit axes x, y, z. */
export function basisQuat(x: readonly number[], y: readonly number[], z: readonly number[], out: P3) {
  const m00 = x[0];
  const m10 = x[1];
  const m20 = x[2];
  const m01 = y[0];
  const m11 = y[1];
  const m21 = y[2];
  const m02 = z[0];
  const m12 = z[1];
  const m22 = z[2];
  const tr = m00 + m11 + m22;
  if (tr > 0) {
    const k = 0.5 / Math.sqrt(tr + 1);
    out[0] = (m21 - m12) * k;
    out[1] = (m02 - m20) * k;
    out[2] = (m10 - m01) * k;
    out[3] = 0.25 / k;
  } else if (m00 > m11 && m00 > m22) {
    const k = 2 * Math.sqrt(1 + m00 - m11 - m22);
    out[0] = 0.25 * k;
    out[1] = (m01 + m10) / k;
    out[2] = (m02 + m20) / k;
    out[3] = (m21 - m12) / k;
  } else if (m11 > m22) {
    const k = 2 * Math.sqrt(1 + m11 - m00 - m22);
    out[0] = (m01 + m10) / k;
    out[1] = 0.25 * k;
    out[2] = (m12 + m21) / k;
    out[3] = (m02 - m20) / k;
  } else {
    const k = 2 * Math.sqrt(1 + m22 - m00 - m11);
    out[0] = (m02 + m20) / k;
    out[1] = (m12 + m21) / k;
    out[2] = 0.25 * k;
    out[3] = (m10 - m01) / k;
  }
  return out;
}

const _d: P3 = [0, 0, 0];
const _t: P3 = [0, 0, 0];
const _fq: P3 = [0, 0, 0, 1];
const _e: P3 = [0, 0, 0];
const _s: P3 = [0, 0, 0];
