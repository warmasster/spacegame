import * as THREE from 'three';
import { clonePose, copyPose, lerpPose, type ShipPose } from '../../shared/ship/flight';
import type { V3 } from '../../shared/ship/geom';
import { altitudeOf, bodyAt, gravityAt, tangentFrame, type CelestialBody } from '../../shared/space/body';
import type { BodySurface } from '../../shared/space/surface';

/**
 * How the physics bubble is laid:
 * - `ground`: near the surface, anywhere (the base is no different): a tangent frame on the ground
 *   under the player, still (the ground's collision is laid in it), re-laid every GROUND_REANCHOR.
 * - `space`: high or fast: a tangent frame at the player moving with it (a Galilean frame), so that
 *   a crate floating beside a ship in orbit, the ship's hull and the astronaut all move a few m/s in
 *   it, not 1.6 km/s — contacts, the character controller and float32 all work as on the ground.
 */
export type BubbleMode = 'ground' | 'space';

/** Metres the player may wander from the anchor (on the ground, horizontally) before it is re-laid. */
const GROUND_REANCHOR = 6000;
/** Metres (in the moving frame) before a new anchor in space. */
const SPACE_REANCHOR = 2000;
/** The player drifting this fast (m/s) off the bubble's own motion: a new anchor moving with it. */
const SPACE_DRIFT = 40;
/** Over the ground: below this height (m) and slower than this (m/s, relative to the ground). */
const GROUND_ALT = 5000;
const GROUND_SPEED = 250;
/** Hysteresis of the two (back to the ground only well inside them). */
const HYST = 0.8;

/**
 * The physics bubble: frame 0 of the client (Frames), where the lunar Rapier world lives — the
 * astronaut on foot, loose crates and debris outside every ship, the ships' outer hulls and the
 * ground's collision. Rapier is float32, so its coordinates must stay small wherever the player is:
 * the bubble follows the player round the body (and along its orbit) and every body in it is
 * carried across unchanged (same world position, velocity and spin) each time it is re-laid.
 *
 * Its axes are always the tangent frame of its anchor (x east, y up, z south). "Up" and gravity in
 * the bubble are (0, 1, 0) and (0, −g, 0) at the anchor; a few km away the vertical leans by a
 * fraction of a degree. It starts nowhere: the first `follow` lays it where the player is.
 */
export class Bubble {
  mode: BubbleMode = 'space';
  /** The body it is laid on (until the first `follow`: the one ruling the world's origin). */
  body: CelestialBody = bodyAt([0, 0, 0]);
  /** Pose of the bubble in the world this fixed step, the step before, and as drawn. */
  readonly pose: ShipPose = { p: [0, 0, 0], q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0] };
  readonly prev: ShipPose = clonePose(this.pose);
  readonly render: ShipPose = clonePose(this.pose);
  /**
   * The poses it had before the last re-laying, this step and the step before: residents are
   * carried from `from` to `pose` and their step before from `fromPrev` to `prev`, so what is
   * drawn between the two steps goes on unbroken (shared/frames/track.ts).
   */
  readonly from: ShipPose = clonePose(this.pose);
  readonly fromPrev: ShipPose = clonePose(this.pose);
  /** Axes (world, unit): east, up, south. */
  readonly e: V3 = [1, 0, 0];
  readonly u: V3 = [0, 1, 0];
  readonly s: V3 = [0, 0, 1];
  /** Gravity in bubble coordinates (m/s²). */
  readonly gravity = new THREE.Vector3(0, -this.body.def.gravity, 0);
  /** Bumped on every re-laying (tile keys, caches). */
  version = 0;
  private _g: V3 = [0, 0, 0];

  constructor(
    private surface: (b: CelestialBody) => BodySurface | null,
    /** Length of a fixed step (s): where a moving bubble was the step before it was laid. */
    private stepS = 1 / 60,
  ) {}

  /** Start of a fixed step: a moving bubble advances with its velocity; gravity where it is. */
  step(dt: number) {
    copyPose(this.prev, this.pose);
    if (this.mode !== 'space') return;
    const p = this.pose.p;
    const v = this.pose.v;
    p[0] += v[0] * dt;
    p[1] += v[1] * dt;
    p[2] += v[2] * dt;
    this.updateGravity();
  }

  /** Once per rendered frame: the pose between the last two steps. */
  frame(alpha: number) {
    lerpPose(this.prev, this.pose, alpha, this.render);
  }

  /**
   * Keep the bubble round the player: world position `p` and velocity `v` (m/s) at the end of a
   * step (everything in the bubble at that same time). Returns true when it was re-laid — then
   * every resident has to be carried from `from` to `pose`, and its step before from `fromPrev`
   * to `prev` (`Frames.rebase`, `PoseTrack.carry`).
   */
  follow(p: V3, v: V3): boolean {
    const b = bodyAt(p);
    const surface = this.surface(b);
    const speed = Math.hypot(v[0], v[1], v[2]);
    const k = this.mode === 'space' ? HYST : 1;
    // the height over the ground only matters when it is not obviously high
    const alt = altitudeOf(b, p);
    const top = surface ? surface.highest : 0;
    let low = alt - top < GROUND_ALT * k;
    if (low && surface) {
      const d = _d;
      const r = alt + b.radius;
      d[0] = (p[0] - b.center[0]) / r;
      d[1] = (p[1] - b.center[1]) / r;
      d[2] = (p[2] - b.center[2]) / r;
      low = alt - surface.height(d, 50) < GROUND_ALT * k;
    }
    const ground = low && speed < GROUND_SPEED * k;
    const mode: BubbleMode = ground ? 'ground' : 'space';
    if (this.version > 0 && mode === this.mode && b === this.body && !this.wandered(p, v)) return false;
    copyPose(this.from, this.pose);
    copyPose(this.fromPrev, this.prev);
    this.mode = mode;
    this.body = b;
    const pose = this.pose;
    tangentFrame(b, p, pose.q, this.e, this.u, this.s);
    if (mode === 'ground') {
      // on the ground under the player: bubble heights are heights over the local ground
      const h = surface ? surface.height(this.u, 1) : 0;
      for (let i = 0; i < 3; i++) pose.p[i] = b.center[i] + this.u[i] * (b.radius + h);
      pose.v[0] = pose.v[1] = pose.v[2] = 0;
    } else {
      for (let i = 0; i < 3; i++) {
        pose.p[i] = p[i];
        pose.v[i] = v[i];
      }
    }
    this.updateGravity();
    // the new laying the step before: where it would have been (a moving one, one step back along
    // its velocity; one on the ground, where it is). The residents' step before goes there, so
    // the render interpolates across the re-laying like across any other step.
    copyPose(this.prev, pose);
    for (let i = 0; i < 3; i++) this.prev.p[i] -= pose.v[i] * this.stepS;
    copyPose(this.render, pose);
    this.version++;
    return true;
  }

  /** Far enough from the anchor (or drifting off a moving one) to lay a new one. */
  private wandered(p: V3, v: V3) {
    const o = this.pose.p;
    const dx = p[0] - o[0];
    const dy = p[1] - o[1];
    const dz = p[2] - o[2];
    if (this.mode === 'ground') {
      const up = dx * this.u[0] + dy * this.u[1] + dz * this.u[2];
      return dx * dx + dy * dy + dz * dz - up * up > GROUND_REANCHOR * GROUND_REANCHOR;
    }
    const bv = this.pose.v;
    const dv = Math.hypot(v[0] - bv[0], v[1] - bv[1], v[2] - bv[2]);
    return dx * dx + dy * dy + dz * dz > SPACE_REANCHOR * SPACE_REANCHOR || dv > SPACE_DRIFT;
  }

  /** Gravity at the anchor, in the bubble's axes. */
  private updateGravity() {
    const g = gravityAt(this.body, this.pose.p, this._g);
    this.gravity.set(g[0] * this.e[0] + g[1] * this.e[1] + g[2] * this.e[2], g[0] * this.u[0] + g[1] * this.u[1] + g[2] * this.u[2], g[0] * this.s[0] + g[1] * this.s[1] + g[2] * this.s[2]);
  }
}

const _d: V3 = [0, 0, 0];
