import * as THREE from 'three';
import { HELMET_LAYER, type Astronaut } from './astronaut';
import { origin } from '../render/origin';

export type CameraMode = 'first' | 'third';

export interface CameraView {
  feet: THREE.Vector3;
  frame: THREE.Quaternion;
  yaw: number;
  pitch: number;
  crouch: boolean;
}

const _up = new THREE.Vector3();
const _d = new THREE.Vector3();
const _q = new THREE.Quaternion();
const _X = new THREE.Vector3(1, 0, 0);
const _Y = new THREE.Vector3(0, 1, 0);
const _p = [0, 0, 0];

/** First person from inside the helmet (own body visible), or over-the-shoulder third person. */
export class CameraRig {
  mode: CameraMode = 'first';
  /** Free-look orbit around the astronaut in third person (Alt + mouse), radians. */
  orbit = 0;
  orbitPitch = 0;
  private eye = new THREE.Vector3();
  /** Smoothed eye height above the body. The body itself is not smoothed, or the jet and falls leave the camera behind. */
  private smoothBob = 0;
  private thirdPos = new THREE.Vector3();
  private initialized = false;
  zoom = 3.4;
  /** First-person optical zoom (wheel), ×1..×4. */
  fpZoom = 1;
  /** Right mouse held: extra ×3 on top, in either mode. */
  zoomHeld = false;
  /**
   * Wheel while looking at a selector (or with the manual open). Return true to keep the zoom
   * where it is. `dir` is +1 for scroll up.
   */
  wheelTo: ((dir: number) => boolean) | null = null;
  static readonly BASE_FOV = 72;
  /** Shake 0..1 (blasts, a decompression): fades by itself, felt as its square. */
  private trauma = 0;
  private shakeT = 0;

  /**
   * `ground`: height of a world point over the ground under it (m). The camera is placed in world
   * coordinates (it hangs from the render origin's root).
   */
  constructor(
    private camera: THREE.PerspectiveCamera,
    private ground: (p: readonly number[]) => number,
  ) {
    window.addEventListener('wheel', (e) => {
      const dir = -Math.sign(e.deltaY);
      if (dir && this.wheelTo?.(dir)) return;
      if (this.mode === 'third') this.zoom = THREE.MathUtils.clamp(this.zoom + Math.sign(e.deltaY) * 0.4, 1.6, 9);
      else this.fpZoom = THREE.MathUtils.clamp(this.fpZoom * (e.deltaY < 0 ? 1.25 : 0.8), 1, 4);
    });
  }

  toggle() {
    this.mode = this.mode === 'first' ? 'third' : 'first';
    this.initialized = false;
  }

  /** Shake the view (0..1 added to what is still shaking). */
  shake(amount: number) {
    this.trauma = Math.min(1, this.trauma + Math.max(0, amount));
  }

  /** Third person: `occlude(from, to)` returns the distance to the first solid surface (ship hulls). */
  /** Current magnification (for the HUD and mouse sensitivity). */
  get magnification() {
    return CameraRig.BASE_FOV / this.camera.fov;
  }

  /**
   * `view`: the astronaut as drawn — feet in the world, the orientation of its frame (identity on
   * the moon, the ship's as drawn aboard) and the look (yaw / pitch) inside that frame.
   */
  update(dt: number, view: CameraView, astronaut: Astronaut, occlude?: (from: THREE.Vector3, to: THREE.Vector3) => number | null) {
    const cam = this.camera;
    const up = _up.set(0, 1, 0).applyQuaternion(view.frame);
    astronaut.eyePosition(this.eye);
    const bob = _d.copy(this.eye).sub(view.feet).dot(up);
    if (!this.initialized) this.smoothBob = bob;
    // only the gait bob is softened; the body rise (jet, fall) stays with the body
    this.smoothBob += (bob - this.smoothBob) * Math.min(1, dt * 18);

    const third = this.mode === 'third';
    const pitch = third ? THREE.MathUtils.clamp(view.pitch + this.orbitPitch, -1.4, 1.3) : view.pitch;
    const orient = new THREE.Quaternion().copy(view.frame).multiply(_q.setFromAxisAngle(_Y, view.yaw + (third ? this.orbit : 0))).multiply(_q.setFromAxisAngle(_X, pitch));
    cam.up.copy(up);
    if (!third) {
      cam.layers.disable(HELMET_LAYER);
      cam.near = 0.05;
      cam.position.copy(this.eye).addScaledVector(up, this.smoothBob - bob);
      cam.quaternion.copy(orient);
    } else {
      cam.layers.enable(HELMET_LAYER);
      cam.near = 0.1;
      const target = view.feet.clone().addScaledVector(up, view.crouch ? 1.15 : 1.55);
      const back = new THREE.Vector3(0, 0, 1).applyQuaternion(orient);
      const right = new THREE.Vector3(1, 0, 0).applyQuaternion(orient);
      const desired = target.clone().addScaledVector(back, this.zoom).addScaledVector(right, 0.55).addScaledVector(up, 0.25);
      _p[0] = desired.x;
      _p[1] = desired.y;
      _p[2] = desired.z;
      const alt = this.ground(_p);
      if (alt < 0.4) desired.addScaledVector(up, 0.4 - alt);
      // don't look through walls: pull in in front of the first surface
      const pivot = target.clone().addScaledVector(right, 0.2);
      const hit = occlude?.(pivot, desired);
      if (hit !== null && hit !== undefined) desired.lerpVectors(pivot, desired, Math.max(0.05, (hit - 0.25) / pivot.distanceTo(desired)));
      if (!this.initialized) this.thirdPos.copy(desired);
      this.thirdPos.lerp(desired, Math.min(1, dt * 12));
      cam.position.copy(this.thirdPos);
      // lookAt compares with the camera's own matrix: render space
      cam.lookAt(origin.toRender(target.clone().addScaledVector(right, 0.55)));
    }
    // zoom = narrower field of view (eases in/out)
    const fov = CameraRig.BASE_FOV / ((this.mode === 'first' ? this.fpZoom : 1) * (this.zoomHeld ? 3 : 1));
    cam.fov += (fov - cam.fov) * Math.min(1, dt * 10);
    if (this.trauma > 0) {
      // smooth noise (a few incommensurate sines), stronger as the square of the trauma
      this.shakeT += dt;
      const k = this.trauma * this.trauma * 0.045;
      const t = this.shakeT * 31;
      cam.rotateX(k * (Math.sin(t * 1.13) + 0.5 * Math.sin(t * 2.71)));
      cam.rotateY(k * (Math.sin(t * 0.97 + 1.7) + 0.5 * Math.sin(t * 2.33 + 0.4)));
      cam.rotateZ(k * 0.6 * Math.sin(t * 1.41 + 2.9));
      this.trauma = Math.max(0, this.trauma - dt * 1.4);
    }
    cam.updateProjectionMatrix();
    cam.updateMatrixWorld();
    this.initialized = true;
  }
}
