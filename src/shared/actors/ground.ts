// The grounds a walker can walk on (shared/actors/walker.ts): a body's surface anywhere on it.
// (A ship's deck will be another WalkGround: its +Y up, its floor under the feet.)

import { bodyAt, heightAboveGround, tangentFrame, type Surfaces } from '../space/body.js';
import type { V3, WalkGround } from './walker.js';

const q: number[] = [0, 0, 0, 1];

/** Whatever body is under the point: up away from its centre, the ground its surface (craters too). */
export function bodyGround(surfaces: Surfaces): WalkGround {
  return {
    axes(p, e: V3, u: V3, s: V3) {
      tangentFrame(bodyAt(p), p, q, e, u, s);
    },
    height(p) {
      const b = bodyAt(p);
      return heightAboveGround(b, p, surfaces(b));
    },
  };
}
