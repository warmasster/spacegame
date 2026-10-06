// A thing's pose over the last two fixed steps, in its frame: what the render draws between them.
// Everything drawn between steps (crates, debris, the astronaut, anything added later) keeps one
// of these instead of its own prev/cur arrays, so the rule is the same for all of them:
//
//   - `push` at the end of each step (the pose it just reached);
//   - `carry` when it goes to another frame (a door, a re-laid physics bubble): BOTH steps are
//     carried, each with the frame poses of its own step, so what is drawn goes on unbroken —
//     no frozen step, no jump of v·dt (4 m at 250 m/s, 27 m at 1.6 km/s);
//   - `snap` only for a real jump (teleport, respawn);
//   - `at(alpha)` for the pose drawn this frame, in its frame (compose it with the frame's pose as
//     drawn, `Frames.pose(fr, true)`).
//
// No allocation anywhere: it runs for every loose body every step.

import type { Quat, V3 } from '../ship/geom.js';
import type { FramePose } from './frame.js';

/** A frame's pose this step and the step before (a host, the physics bubble). */
export interface FramePair {
  readonly pose: FramePose;
  readonly prev: FramePose;
}

// ---------------------------------------------------------------------------------------------
// Allocation-free pose maths (also used by the network replicas)
// ---------------------------------------------------------------------------------------------

/** q ⊗ (x, y, z) into `out` (with `inv`: the conjugate's rotation). */
export function rotInto(q: readonly number[], x: number, y: number, z: number, out: number[], inv = false): number[] {
  const s = inv ? -1 : 1;
  const qx = q[0] * s, qy = q[1] * s, qz = q[2] * s, qw = q[3];
  const tx = 2 * (qy * z - qz * y), ty = 2 * (qz * x - qx * z), tz = 2 * (qx * y - qy * x);
  out[0] = x + qw * tx + (qy * tz - qz * ty);
  out[1] = y + qw * ty + (qz * tx - qx * tz);
  out[2] = z + qw * tz + (qx * ty - qy * tx);
  return out;
}

/** a ⊗ b into `out` (`ca`: conjugate of a). `out` may be a or b. */
export function qMulInto(a: readonly number[], b: readonly number[], out: number[], ca = false): number[] {
  const s = ca ? -1 : 1;
  const ax = a[0] * s, ay = a[1] * s, az = a[2] * s, aw = a[3];
  const bx = b[0], by = b[1], bz = b[2], bw = b[3];
  out[0] = aw * bx + ax * bw + ay * bz - az * by;
  out[1] = aw * by - ax * bz + ay * bw + az * bx;
  out[2] = aw * bz + ax * by - ay * bx + az * bw;
  out[3] = aw * bw - ax * bx - ay * by - az * bz;
  return out;
}

/** World point of a frame-local one (`pose`: the frame; null: the world). */
export function toWorldInto(pose: FramePose | null, l: readonly number[], out: number[]): number[] {
  if (!pose) {
    out[0] = l[0];
    out[1] = l[1];
    out[2] = l[2];
    return out;
  }
  rotInto(pose.q, l[0], l[1], l[2], out);
  out[0] += pose.p[0];
  out[1] += pose.p[1];
  out[2] += pose.p[2];
  return out;
}

/** Frame-local point of a world one. */
export function toLocalInto(pose: FramePose | null, w: readonly number[], out: number[]): number[] {
  if (!pose) {
    out[0] = w[0];
    out[1] = w[1];
    out[2] = w[2];
    return out;
  }
  return rotInto(pose.q, w[0] - pose.p[0], w[1] - pose.p[1], w[2] - pose.p[2], out, true);
}

/** Spin `q` by the world rate `w` for `dt` s (exact for a constant rate) into `out`. */
export function qSpinInto(q: readonly number[], w: readonly number[], dt: number, out: number[]): number[] {
  const ex = w[0] * dt, ey = w[1] * dt, ez = w[2] * dt;
  const ang = Math.sqrt(ex * ex + ey * ey + ez * ez);
  let dx: number, dy: number, dz: number, dw: number;
  if (ang < 1e-9) {
    dx = ex / 2;
    dy = ey / 2;
    dz = ez / 2;
    dw = 1;
  } else {
    const s = Math.sin(ang / 2) / ang;
    dx = ex * s;
    dy = ey * s;
    dz = ez * s;
    dw = Math.cos(ang / 2);
  }
  const qx = q[0], qy = q[1], qz = q[2], qw = q[3];
  const x = dw * qx + dx * qw + dy * qz - dz * qy;
  const y = dw * qy - dx * qz + dy * qw + dz * qx;
  const z = dw * qz + dx * qy - dy * qx + dz * qw;
  const ww = dw * qw - dx * qx - dy * qy - dz * qz;
  const l = Math.sqrt(x * x + y * y + z * z + ww * ww) || 1;
  out[0] = x / l;
  out[1] = y / l;
  out[2] = z / l;
  out[3] = ww / l;
  return out;
}

/** Shortest-path blend of two unit quaternions into `out` (`out` may be a). */
export function slerpInto(a: readonly number[], b: readonly number[], t: number, out: number[]): number[] {
  let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
  const sign = d < 0 ? -1 : 1;
  d = Math.min(1, Math.abs(d));
  let ka = 1 - t, kb = t * sign;
  if (d < 0.9995) {
    const th = Math.acos(d), s = Math.sin(th);
    ka = Math.sin((1 - t) * th) / s;
    kb = (Math.sin(t * th) / s) * sign;
  }
  const x = a[0] * ka + b[0] * kb, y = a[1] * ka + b[1] * kb, z = a[2] * ka + b[2] * kb, w = a[3] * ka + b[3] * kb;
  const l = Math.sqrt(x * x + y * y + z * z + w * w) || 1;
  out[0] = x / l;
  out[1] = y / l;
  out[2] = z / l;
  out[3] = w / l;
  return out;
}

// ---------------------------------------------------------------------------------------------
// The track
// ---------------------------------------------------------------------------------------------

const _w: number[] = [0, 0, 0];
const _q: number[] = [0, 0, 0, 1];

export class PoseTrack {
  /** Pose at the step before and at the last step (its frame's coordinates). */
  readonly prevP: V3 = [0, 0, 0];
  readonly prevQ: Quat = [0, 0, 0, 1];
  readonly curP: V3 = [0, 0, 0];
  readonly curQ: Quat = [0, 0, 0, 1];
  /** Carried to another frame since the last `push` (diagnostics). */
  carried = 0;

  constructor(
    /** The frame the two poses are in. */
    public fr = 0,
  ) {}

  /** End of a step: the pose it reached (the one before becomes `prev`). */
  push(px: number, py: number, pz: number, qx = 0, qy = 0, qz = 0, qw = 1) {
    const a = this.prevP, b = this.curP, qa = this.prevQ, qb = this.curQ;
    a[0] = b[0];
    a[1] = b[1];
    a[2] = b[2];
    qa[0] = qb[0];
    qa[1] = qb[1];
    qa[2] = qb[2];
    qa[3] = qb[3];
    b[0] = px;
    b[1] = py;
    b[2] = pz;
    qb[0] = qx;
    qb[1] = qy;
    qb[2] = qz;
    qb[3] = qw;
    this.carried = 0;
  }

  /** A real jump (teleport, respawn, a new owner's state): nothing is drawn in between. */
  snap(p: readonly number[], q?: readonly number[]) {
    for (let i = 0; i < 3; i++) this.prevP[i] = this.curP[i] = p[i];
    if (q) for (let i = 0; i < 4; i++) this.prevQ[i] = this.curQ[i] = q[i];
  }

  /**
   * Into another frame (`from`/`to`: the frames' two poses; null: the world itself). The pose now
   * goes with the frames' poses now, the one before with theirs before: world positions of both
   * steps are unchanged, so the motion drawn between them is too.
   */
  carry(from: FramePair | null, to: FramePair | null, toFr: number) {
    carryPoint(from?.pose ?? null, to?.pose ?? null, this.curP);
    carryPoint(from?.prev ?? null, to?.prev ?? null, this.prevP);
    carryQuat(from?.pose ?? null, to?.pose ?? null, this.curQ);
    carryQuat(from?.prev ?? null, to?.prev ?? null, this.prevQ);
    this.fr = toFr;
    this.carried++;
  }

  /** Only the step before, into the current frame (the current pose was set there already). */
  carryPrev(from: FramePose | null, to: FramePose | null) {
    carryPoint(from, to, this.prevP);
    carryQuat(from, to, this.prevQ);
  }

  /** Position between the two steps (`alpha` 0..1), in its frame. */
  at(alpha: number, out: number[]): number[] {
    const a = this.prevP, b = this.curP;
    out[0] = a[0] + (b[0] - a[0]) * alpha;
    out[1] = a[1] + (b[1] - a[1]) * alpha;
    out[2] = a[2] + (b[2] - a[2]) * alpha;
    return out;
  }

  /** Orientation between the two steps, in its frame. */
  quatAt(alpha: number, out: number[]): number[] {
    return slerpInto(this.prevQ, this.curQ, alpha, out);
  }
}

/** A point from frame pose `a` to frame pose `b` (null: the world), in place. */
export function carryPoint(a: FramePose | null, b: FramePose | null, p: number[]) {
  toWorldInto(a, p, _w);
  toLocalInto(b, _w, p);
}

/** An orientation from frame pose `a` to `b`, in place. */
export function carryQuat(a: FramePose | null, b: FramePose | null, q: number[]) {
  if (a) qMulInto(a.q, q, q);
  if (b) {
    qMulInto(b.q, q, _q, true);
    q[0] = _q[0];
    q[1] = _q[1];
    q[2] = _q[2];
    q[3] = _q[3];
  }
}
