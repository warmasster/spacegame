import type RAPIER from '@dimforge/rapier3d-compat';
import type * as THREE from 'three';
import { carry, WORLD_FRAME } from '../../shared/frames';
import { dirToLocal, dirToWorld, pointVelocity, toLocal, toWorld, type ShipPose } from '../../shared/ship/flight';
import type { Quat, V3 } from '../../shared/ship/geom';
import type { ShipClient } from '../ship/ship';
import type { Physics } from '../world/physics';
import type { Bubble } from './bubble';

/** Not a frame: the world itself (for `transfer` to and from absolute coordinates). */
export const ABSOLUTE = -1;

/**
 * Reference frames of the client: the physics bubble (id 0, the Rapier world of the outside — a
 * tangent frame that follows the player round any body, frames/bubble.ts) and
 * every ship (its id, its interior world, ship space). Crew and crates live in exactly one frame;
 * these helpers convert positions, velocities and orientations between a frame and the world, and
 * carry a body's state from one frame to another without a jump (position, velocity and spin are
 * the same physical motion on both sides).
 *
 * On the network frame 0 means the world itself (every client has its own bubble): whoever sends
 * or receives a frame-0 state converts it (`Crates`, the player state, `RemotePlayer`).
 */
export class Frames {
  /** Where each ship id sat in the list last time (checked on every lookup, rebuilt when stale). */
  private index = new Map<number, number>();

  constructor(
    private physics: Physics,
    private ships: () => ShipClient[],
    readonly bubble: Bubble,
  ) {}

  /** Called dozens of times a step: O(1), no closures. */
  ship(fr: number): ShipClient | undefined {
    if (fr === WORLD_FRAME) return undefined;
    const list = this.ships();
    const i = this.index.get(fr);
    if (i !== undefined && list[i]?.id === fr) return list[i];
    this.index.clear();
    for (let k = 0; k < list.length; k++) this.index.set(list[k].id, k);
    const j = this.index.get(fr);
    return j === undefined ? undefined : list[j];
  }

  /** Rapier world of a frame (an unknown ship falls back to the bubble). */
  world(fr: number): RAPIER.World {
    return this.ship(fr)?.space.world ?? this.physics.world;
  }

  /** Gravity felt in a frame (frame coordinates). */
  gravity(fr: number): THREE.Vector3 {
    return this.ship(fr)?.space.gravity ?? this.bubble.gravity;
  }

  /** Pose of a frame this fixed step (null: none — an unknown ship). `render`: the interpolated one of this frame. */
  pose(fr: number, render = false): ShipPose | null {
    if (fr === WORLD_FRAME) {
      const b = this.bubble;
      return render ? b.render : b.pose;
    }
    const s = this.ship(fr);
    if (!s) return null;
    return render ? s.render : s.sim.pose;
  }

  toWorld(fr: number, p: V3, render = false): V3 {
    const pose = this.pose(fr, render);
    return pose ? toWorld(pose, p) : [p[0], p[1], p[2]];
  }

  toLocal(fr: number, p: V3, render = false): V3 {
    const pose = this.pose(fr, render);
    return pose ? toLocal(pose, p) : [p[0], p[1], p[2]];
  }

  dirToWorld(fr: number, d: V3, render = false): V3 {
    const pose = this.pose(fr, render);
    return pose ? dirToWorld(pose, d) : [d[0], d[1], d[2]];
  }

  dirToLocal(fr: number, d: V3, render = false): V3 {
    const pose = this.pose(fr, render);
    return pose ? dirToLocal(pose, d) : [d[0], d[1], d[2]];
  }

  /** Orientation of the frame in the world. */
  quat(fr: number, render = false): Quat {
    const pose = this.pose(fr, render);
    return pose ? pose.q : [0, 0, 0, 1];
  }

  /** World velocity of a point at rest in a frame (frame coordinates). */
  velocityAt(fr: number, p: V3): V3 {
    const pose = this.pose(fr);
    return pose ? pointVelocity(pose, p) : [0, 0, 0];
  }

  /**
   * A body's motion in `from` expressed in `to`: position, velocity relative to the new frame and
   * orientation. Nothing jumps: the world position and the world velocity are the same.
   */
  transfer(from: number, to: number, p: V3, v: V3, q?: Quat): { p: V3; v: V3; q: Quat } {
    return carry(this.pose(from), this.pose(to), p, v, q);
  }

  /** The same across a re-laying of the bubble: from its old pose to its new one. */
  rebase(p: V3, v: V3, q?: Quat): { p: V3; v: V3; q: Quat } {
    return carry(this.bubble.from, this.bubble.pose, p, v, q);
  }
}

/** Carry a body's motion from one pose to another (shared/frames). */
export { carry };
