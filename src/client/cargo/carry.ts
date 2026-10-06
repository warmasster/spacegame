import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';
import type { Quat } from '../../shared/protocol';
import { qMul, qYaw, type V3 } from '../../shared/ship/geom';
import type { Frames } from '../frames/frames';
import { objectOf, throwSpeedOf } from '../../shared/items';
import type { CrateBody, Crates } from './crates';

/** How far a crate can be picked up / put down (m, from the eyes). */
export const CARRY_REACH = 2.4;
const PLACE_REACH = 3.2;
/** Ghost placement snaps its heading to this step (rad). */
const YAW_SNAP = Math.PI / 12;

/**
 * The crate in your hands: it floats in front of the eyes (a spring in the physics, so it bumps
 * into walls instead of passing through them), and a ghost shows where a click would set it down —
 * green if it fits there, red if something is in the way.
 */
export class Carry {
  private ghost: THREE.Mesh;
  private ghostMat: THREE.MeshBasicMaterial;
  /** Where a click would put the crate (the holder's frame), and whether it fits. */
  spot: { p: V3; q: Quat; fits: boolean } | null = null;

  constructor(
    private R: typeof RAPIER,
    /** Where the ghost hangs: it is placed in world coordinates (the render origin's root). */
    scene: THREE.Object3D,
    private crates: Crates,
    private frames: Frames,
  ) {
    this.ghostMat = new THREE.MeshBasicMaterial({ color: 0x5cf29a, transparent: true, opacity: 0.22, depthWrite: false, toneMapped: false });
    const edges = new THREE.LineSegments(new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1)), new THREE.LineBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.7, toneMapped: false }));
    this.ghost = new THREE.Mesh(new THREE.BoxGeometry(1, 1, 1), this.ghostMat);
    this.ghost.add(edges);
    this.ghost.visible = false;
    this.ghost.renderOrder = 21;
    scene.add(this.ghost);
  }

  get held(): CrateBody | null {
    return this.crates.hold?.crate ?? null;
  }

  /**
   * Every frame while holding: the hands' target (in front of the eyes, in the holder's frame) and
   * the placement ghost along the view. `eye` / `dir`: the camera in the holder's frame.
   */
  update(fr: number, eye: V3, dir: V3, yaw: number, player: number) {
    const h = this.crates.hold;
    this.spot = null;
    this.ghost.visible = false;
    if (!h) return;
    const c = h.crate;
    const size = Math.max(c.spec.half[0], c.spec.half[2]);
    const reach = 0.75 + size;
    h.target = [eye[0] + dir[0] * reach, eye[1] + dir[1] * reach - 0.15, eye[2] + dir[2] * reach];
    h.yaw = yaw;
    // where the view meets a floor-like surface within reach
    const world = this.frames.world(fr);
    const ray = new this.R.Ray({ x: eye[0], y: eye[1], z: eye[2] }, { x: dir[0], y: dir[1], z: dir[2] });
    const skip = (col: RAPIER.Collider) => col.handle !== c.collider.handle && col.handle !== player;
    const hit = world.castRayAndGetNormal(ray, PLACE_REACH, true, undefined, undefined, undefined, undefined, skip);
    if (!hit) return;
    const up = this.frames.gravity(fr).clone().negate().normalize();
    const n = hit.normal;
    if (n.x * up.x + n.y * up.y + n.z * up.z < 0.7) return;
    const t = hit.timeOfImpact;
    const hy = c.spec.half[1];
    const p: V3 = [eye[0] + dir[0] * t + up.x * (hy + 0.012), eye[1] + dir[1] * t + up.y * (hy + 0.012), eye[2] + dir[2] * t + up.z * (hy + 0.012)];
    const q = qYaw(Math.round(yaw / YAW_SNAP) * YAW_SNAP);
    // anything in the way (a slightly smaller box, so resting on the floor is not "in the way")
    const [hx, , hz] = c.spec.half;
    const shape = new this.R.Cuboid(hx - 0.01, hy - 0.01, hz - 0.01);
    const blocked = world.intersectionWithShape({ x: p[0], y: p[1], z: p[2] }, { x: q[0], y: q[1], z: q[2], w: q[3] }, shape, undefined, undefined, undefined, undefined, skip);
    this.spot = { p, q, fits: !blocked };
    // the ghost, drawn where it would be in the world
    const pose = this.frames.pose(fr, true);
    const wp = this.frames.toWorld(fr, p, true);
    const wq = pose ? qMul(pose.q, q) : q;
    this.ghost.position.set(wp[0], wp[1], wp[2]);
    this.ghost.quaternion.set(wq[0], wq[1], wq[2], wq[3]);
    this.ghost.scale.set(hx * 2, hy * 2, hz * 2);
    this.ghostMat.color.set(this.spot.fits ? 0x5cf29a : 0xff5a4a);
    this.ghost.visible = true;
  }

  /** E again: let go where it is. */
  drop() {
    this.crates.release();
  }

  /** Q: throw it along the view (frame coordinates), as fast as its kind and mass allow (object catalog). */
  throw(dir: V3) {
    const c = this.held;
    const k = c ? throwSpeedOf(objectOf(c.spec), c.spec.mass) : 0;
    this.crates.release([dir[0] * k, dir[1] * k, dir[2] * k]);
  }

  /** Click: set it down on the ghost if it fits. Returns false when it doesn't. */
  place(): boolean {
    const s = this.spot;
    if (!s || !s.fits) return false;
    this.crates.place(s.p, s.q);
    this.spot = null;
    this.ghost.visible = false;
    return true;
  }
}
