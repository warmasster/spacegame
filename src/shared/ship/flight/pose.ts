// A ship's pose and the frame maths everything aboard needs: ship space ↔ world, the velocity of
// a point fixed on the ship, carrying points / velocities / orientations from one pose to the
// next, and interpolating poses (render between fixed steps, network playback).
//
// Conventions (same as three.js): quaternions [x, y, z, w]; ship space +X starboard, +Y up, nose
// toward −Z; `w` is the angular velocity in the WORLD frame (rad/s); `v` is the velocity of the
// ship-space origin (deck centre), not of the centre of mass.

import { add, cross, dot, qConj, qMul, qNorm, qRotate, scale, sub, type Quat, type V3 } from '../geom.js';

export interface ShipPose {
  p: V3;
  q: Quat;
  v: V3;
  w: V3;
}

export const clonePose = (s: ShipPose): ShipPose => ({ p: [...s.p], q: [...s.q], v: [...s.v], w: [...s.w] });

/** Copy in place (keeps the arrays other objects may hold). */
export function copyPose(dst: ShipPose, src: ShipPose) {
  for (let i = 0; i < 3; i++) {
    dst.p[i] = src.p[i];
    dst.v[i] = src.v[i];
    dst.w[i] = src.w[i];
  }
  for (let i = 0; i < 4; i++) dst.q[i] = src.q[i];
}

/** Heading of the nose projected on the horizon (three.js yaw: 0 = nose toward −Z, + = turning left). */
export function poseYaw(q: Quat): number {
  const n = qRotate(q, [0, 0, -1]);
  return Math.atan2(-n[0], -n[2]);
}

/** Compass heading (0 = north = −Z, clockwise toward east = +X), 0..2π. */
export function compassHeading(q: Quat): number {
  const h = -poseYaw(q);
  return ((h % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2);
}

export const toWorld = (pose: ShipPose, local: V3): V3 => add(pose.p, qRotate(pose.q, local));
export const toLocal = (pose: ShipPose, world: V3): V3 => qRotate(qConj(pose.q), sub(world, pose.p));
export const dirToWorld = (pose: ShipPose, d: V3): V3 => qRotate(pose.q, d);
export const dirToLocal = (pose: ShipPose, d: V3): V3 => qRotate(qConj(pose.q), d);

/** World velocity of a point fixed on the ship (`local` in ship space). */
export function pointVelocity(pose: ShipPose, local: V3): V3 {
  return add(pose.v, cross(pose.w, qRotate(pose.q, local)));
}

/** World point carried rigidly from one pose to the next (same ship-space place). */
export function carried(prev: ShipPose, next: ShipPose, world: V3): V3 {
  return toWorld(next, toLocal(prev, world));
}

/**
 * World velocity carried with the ship: what moved with the old deck becomes the new deck's
 * motion, what moved relative to the ship stays relative.
 */
export function carriedVelocity(prev: ShipPose, next: ShipPose, world: V3, vel: V3): V3 {
  const local = toLocal(prev, world);
  const rel = dirToLocal(prev, sub(vel, pointVelocity(prev, local)));
  return add(pointVelocity(next, local), dirToWorld(next, rel));
}

export function carriedRotation(prev: ShipPose, next: ShipPose, q: Quat): Quat {
  return qNorm(qMul(next.q, qMul(qConj(prev.q), q)));
}

// -----------------------------------------------------------------------------------------------
// Quaternion helpers the flight code needs
// -----------------------------------------------------------------------------------------------

export function qFromAxisAngle(axis: V3, ang: number): Quat {
  const s = Math.sin(ang / 2);
  return [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(ang / 2)];
}

/** Rotation vector (axis × angle, shortest way) of a unit quaternion. */
export function qLog(q: Quat): V3 {
  let [x, y, z, w] = q;
  if (w < 0) {
    x = -x;
    y = -y;
    z = -z;
    w = -w;
  }
  const s = Math.hypot(x, y, z);
  if (s < 1e-9) return [2 * x, 2 * y, 2 * z];
  const ang = 2 * Math.atan2(s, w);
  return [(x / s) * ang, (y / s) * ang, (z / s) * ang];
}

/** exp of a rotation vector. */
export function qExp(r: V3): Quat {
  const ang = Math.hypot(r[0], r[1], r[2]);
  if (ang < 1e-9) return qNorm([r[0] / 2, r[1] / 2, r[2] / 2, 1]);
  return qFromAxisAngle([r[0] / ang, r[1] / ang, r[2] / ang], ang);
}

export function slerp(a: Quat, b: Quat, t: number): Quat {
  let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
  let bb = b;
  if (d < 0) {
    d = -d;
    bb = [-b[0], -b[1], -b[2], -b[3]];
  }
  if (d > 0.9995) return qNorm([a[0] + (bb[0] - a[0]) * t, a[1] + (bb[1] - a[1]) * t, a[2] + (bb[2] - a[2]) * t, a[3] + (bb[3] - a[3]) * t]);
  const th = Math.acos(d);
  const s = Math.sin(th);
  const ka = Math.sin((1 - t) * th) / s;
  const kb = Math.sin(t * th) / s;
  return [a[0] * ka + bb[0] * kb, a[1] * ka + bb[1] * kb, a[2] * ka + bb[2] * kb, a[3] * ka + bb[3] * kb];
}

/** Orientation after spinning at world angular velocity `w` for dt (exact for constant w). */
export function qSpin(q: Quat, w: V3, dt: number): Quat {
  return qNorm(qMul(qExp(scale(w, dt)), q));
}

// -----------------------------------------------------------------------------------------------
// Interpolation
// -----------------------------------------------------------------------------------------------

/** Linear blend between two poses (render interpolation between fixed steps). */
export function lerpPose(a: ShipPose, b: ShipPose, t: number, out: ShipPose = clonePose(a)): ShipPose {
  for (let i = 0; i < 3; i++) {
    out.p[i] = a.p[i] + (b.p[i] - a.p[i]) * t;
    out.v[i] = a.v[i] + (b.v[i] - a.v[i]) * t;
    out.w[i] = a.w[i] + (b.w[i] - a.w[i]) * t;
  }
  const q = slerp(a.q, b.q, t);
  for (let i = 0; i < 4; i++) out.q[i] = q[i];
  return out;
}

/**
 * Cubic Hermite between two samples `span` seconds apart, using their velocities: a network
 * stream at 20–30 Hz plays back without the corners a linear blend puts at every sample.
 */
export function hermitePose(a: ShipPose, b: ShipPose, t: number, span: number, out: ShipPose = clonePose(a)): ShipPose {
  const t2 = t * t;
  const t3 = t2 * t;
  const h00 = 2 * t3 - 3 * t2 + 1;
  const h10 = t3 - 2 * t2 + t;
  const h01 = -2 * t3 + 3 * t2;
  const h11 = t3 - t2;
  for (let i = 0; i < 3; i++) {
    out.p[i] = h00 * a.p[i] + h10 * span * a.v[i] + h01 * b.p[i] + h11 * span * b.v[i];
    out.v[i] = a.v[i] + (b.v[i] - a.v[i]) * t;
    out.w[i] = a.w[i] + (b.w[i] - a.w[i]) * t;
  }
  const q = slerp(a.q, b.q, t);
  for (let i = 0; i < 4; i++) out.q[i] = q[i];
  return out;
}

/** A pose carried forward `dt` seconds at its own velocities (short gaps in a stream). */
export function extrapolatePose(a: ShipPose, dt: number, out: ShipPose = clonePose(a)): ShipPose {
  for (let i = 0; i < 3; i++) {
    out.p[i] = a.p[i] + a.v[i] * dt;
    out.v[i] = a.v[i];
    out.w[i] = a.w[i];
  }
  const q = qSpin(a.q, a.w, dt);
  for (let i = 0; i < 4; i++) out.q[i] = q[i];
  return out;
}

/** Up axis of a ship in the world, and how level it is (1 = level). */
export const shipUp = (q: Quat): V3 => qRotate(q, [0, 1, 0]);
export const levelness = (q: Quat) => dot(shipUp(q), [0, 1, 0]);
