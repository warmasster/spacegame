// Reference frames, shared by the client and the server. There is the world itself (WORLD_FRAME)
// and every host: something with its own space that things can be inside of — a ship today; a
// station, a vehicle in a hold, anything with compartments tomorrow. Whatever moves (crew, loose
// objects, projectiles) lives in exactly one frame and is handed from one to another without a
// jump: the same world position and velocity on both sides.
//
// Why: inside a host everything is small, local numbers that don't care how fast the host goes
// (orbit: 1.6 km/s), where each client has it on the network, or float32; and one rule decides
// "which frame is this in" for everything that moves.

import { add, qConj, qMul, sub, type Quat, type V3 } from '../ship/geom.js';
import { dirToLocal, dirToWorld, pointVelocity, toLocal, toWorld, type ShipPose } from '../ship/flight/pose.js';

/** Frame id of the world itself (hosts use their own id, never 0). */
export const WORLD_FRAME = 0;

/** A frame's pose in the world: position, orientation, velocity of its origin and spin (world). */
export type FramePose = ShipPose;

/** Something with its own space that things can be inside of (a ship; later anything with rooms). */
export interface FrameHost {
  readonly id: number;
  /** Pose this step, and the step before (drawing between steps; the server may give the same one). */
  readonly pose: FramePose;
  readonly prev: FramePose;
  /** Farther than this from its origin (m, along any axis of its space) nothing is inside it. */
  readonly reach: number;
  /** Inside one of its compartments (a point of its space). */
  inside(l: V3): boolean;
}

/**
 * A body's motion carried from a frame with pose `a` to one with pose `b` (null: the world itself):
 * position, velocity relative to the new frame and orientation. Nothing jumps: the world position
 * and the world velocity are the same.
 */
export function carry(a: FramePose | null, b: FramePose | null, p: V3, v: V3, q?: Quat): { p: V3; v: V3; q: Quat } {
  // to the world
  const pw = a ? toWorld(a, p) : p;
  const vw = a ? add(pointVelocity(a, p), dirToWorld(a, v)) : v;
  const qw: Quat = q ? (a ? qMul(a.q, q) : q) : [0, 0, 0, 1];
  // into the new frame
  if (!b) return { p: [...pw] as V3, v: [...vw] as V3, q: qw };
  const pl = toLocal(b, pw);
  const vl = dirToLocal(b, sub(vw, pointVelocity(b, pl)));
  return { p: pl, v: vl, q: qMul(qConj(b.q), qw) };
}

/** A point of a host's space well away from it (cheap reject before asking about its rooms). */
export const outOfReach = (h: FrameHost, l: V3) => Math.abs(l[0]) > h.reach || Math.abs(l[1]) > h.reach || Math.abs(l[2]) > h.reach;

/** The host whose compartment holds a world point, and the point in its space (null: none). */
export function hostAt(hosts: Iterable<FrameHost>, pw: V3, skip?: number): { host: FrameHost; l: V3 } | null {
  for (const h of hosts) {
    if (h.id === skip) continue;
    const l = toLocal(h.pose, pw);
    if (outOfReach(h, l) || !h.inside(l)) continue;
    return { host: h, l };
  }
  return null;
}
