// Jumping between star systems (docs/ESPACIO.md): a ship leaves its region of the world frame
// (space/galaxy.ts) and appears in another, high over that region's body, at rest — and everything
// aboard with it, since it all lives in the ship's space. Only in flight, well clear of the ground
// (the drive needs clear space), and not too often. The same rules on the server (who decides) and
// on every client (what it shows and asks).

import type { V3 } from '../ship/geom.js';
import { altitudeOf, bodyAt, MOON_BODY } from './body.js';
import { physicalRegions, regionAt, type Region } from './galaxy.js';

export const JUMP = {
  /** Clear of the ground by this much (m) to jump. */
  minAltitude: 20_000,
  /** Seconds between two jumps of a ship. */
  cooldownS: 15,
  /** Arrival: this many body radii from its centre, over its top. */
  arrivalRadii: 3,
};

/** Why a ship at `p` can't jump to region `to` now (null: it can). */
export function jumpRefusal(p: readonly number[], landed: boolean, to: number): string | null {
  const regions = physicalRegions();
  if (!Number.isInteger(to) || to < 0 || to >= regions.length) return 'destino desconocido';
  if (regionAt(p).index === to) return 'ya estás en ese sistema';
  if (landed) return 'en tierra no';
  if (altitudeOf(bodyAt(p), p) < JUMP.minAltitude) return `demasiado cerca del suelo (hacen falta ${JUMP.minAltitude / 1000} km)`;
  return null;
}

/** Where a jump into a region ends: over its body's top, `arrivalRadii` from its centre. */
export function arrivalPoint(to: Region): V3 {
  const b = to.body ? { center: to.body.center, radius: to.body.def.radius } : { center: MOON_BODY.center, radius: MOON_BODY.radius };
  return [b.center[0], b.center[1] + b.radius * JUMP.arrivalRadii, b.center[2]];
}

/** The next region after the one `p` is in (the drive's destinations, in turn). */
export function nextRegion(p: readonly number[]): Region {
  const regions = physicalRegions();
  return regions[(regionAt(p).index + 1) % regions.length];
}

/** Moves a pose into region `to`: at its arrival point, at rest (its heading kept). */
export function jumpPose(pose: { p: number[]; v: number[]; w: number[] }, to: Region): void {
  const a = arrivalPoint(to);
  for (let i = 0; i < 3; i++) {
    pose.p[i] = a[i];
    pose.v[i] = 0;
    pose.w[i] = 0;
  }
}
