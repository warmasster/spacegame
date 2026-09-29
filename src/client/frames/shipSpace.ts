import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';
import { MOON } from '../../shared/constants';
import type { ShipPose } from '../../shared/ship/flight';
import { qConj, qRotate, type V3 } from '../../shared/ship/geom';
import { bodyAt, gravityAt } from '../../shared/space/body';

/** Smoothing of the ship's measured acceleration (s): a landing jolts, it doesn't teleport. */
const ACCEL_TAU = 0.12;

/**
 * The interior of one ship as its own Rapier world, in ship space: the ship's colliders at rest,
 * and the crew and the cargo aboard. However the ship flies, nothing aboard is ever carried by
 * teleporting it after the hull moved (the source of every "clipping through the floor" bug): the
 * deck simply never moves in this world. What the ship's motion does to them comes in through the
 * apparent gravity:
 *
 *   g_aboard = k · (lunar g, straight down the deck) + (1 − k) · Rᵀ (g_world − a_ship)
 *
 * with k = `grav.k` of the inertial compensator (modules/grav.ts): on, the floor is "down" at
 * lunar weight whatever the ship does; off, the crew and the crates feel the tilt, the braking,
 * the climb — and float in free fall.
 */
export class ShipSpace {
  readonly world: RAPIER.World;
  /** Apparent gravity in ship space (m/s²). */
  readonly gravity = new THREE.Vector3(0, -MOON.gravity, 0);
  /** Smoothed world acceleration of the ship (m/s²). */
  readonly accel: V3 = [0, 0, 0];
  private lastV: V3 | null = null;
  private g: V3 = [0, -MOON.gravity, 0];

  constructor(R: typeof RAPIER) {
    this.world = new R.World({ x: 0, y: -MOON.gravity, z: 0 });
  }

  /** Once per fixed step, after the ship's pose for this step is known. `k`: compensator 0..1. */
  update(dt: number, pose: ShipPose, k: number) {
    if (this.lastV) {
      const a = Math.min(1, dt / ACCEL_TAU);
      for (let i = 0; i < 3; i++) this.accel[i] += ((pose.v[i] - this.lastV[i]) / dt - this.accel[i]) * a;
    }
    this.lastV = [pose.v[0], pose.v[1], pose.v[2]];
    // the body's real pull where the ship is (radial; weaker high up): in free fall — in orbit —
    // it equals the ship's acceleration and nothing aboard weighs anything
    const g = gravityAt(bodyAt(pose.p), pose.p, this.g);
    const felt = qRotate(qConj(pose.q), [g[0] - this.accel[0], g[1] - this.accel[1], g[2] - this.accel[2]]);
    const c = Math.max(0, Math.min(1, k));
    this.gravity.set(felt[0] * (1 - c), -MOON.gravity * c + felt[1] * (1 - c), felt[2] * (1 - c));
    this.world.gravity = { x: this.gravity.x, y: this.gravity.y, z: this.gravity.z };
  }

  /** A jump of the pose (teleport, first sample): forget the old velocity. */
  reset() {
    this.lastV = null;
    this.accel.fill(0);
  }

  step(dt: number) {
    this.world.timestep = dt;
    this.world.step();
  }
}
