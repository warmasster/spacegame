import * as THREE from 'three';
import type { LunarTerrain } from '../../shared/terrain';
import { HELMET_LAYER, type Astronaut } from './astronaut';
import type { PlayerController } from './controller';

export type CameraMode = 'first' | 'third';

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

  constructor(
    private camera: THREE.PerspectiveCamera,
    private terrain: LunarTerrain,
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

  /** Third person: `occlude(from, to)` returns the distance to the first solid surface (ship hulls). */
  /** Current magnification (for the HUD and mouse sensitivity). */
  get magnification() {
    return CameraRig.BASE_FOV / this.camera.fov;
  }

  update(dt: number, ctl: PlayerController, astronaut: Astronaut, occlude?: (from: THREE.Vector3, to: THREE.Vector3) => number | null) {
    const cam = this.camera;
    astronaut.eyePosition(this.eye);
    const bob = this.eye.y - ctl.renderPosition.y;
    if (!this.initialized) this.smoothBob = bob;
    // only the gait bob is softened; the body rise (jet, fall) stays with the body
    this.smoothBob += (bob - this.smoothBob) * Math.min(1, dt * 18);

    const rot = new THREE.Euler(ctl.pitch, ctl.yaw, 0, 'YXZ');
    if (this.mode === 'third') rot.set(THREE.MathUtils.clamp(ctl.pitch + this.orbitPitch, -1.4, 1.3), ctl.yaw + this.orbit, 0, 'YXZ');
    if (this.mode === 'first') {
      cam.layers.disable(HELMET_LAYER);
      cam.near = 0.05;
      cam.position.set(this.eye.x, ctl.renderPosition.y + this.smoothBob, this.eye.z);
      cam.quaternion.setFromEuler(rot);
    } else {
      cam.layers.enable(HELMET_LAYER);
      cam.near = 0.1;
      const target = new THREE.Vector3(ctl.renderPosition.x, ctl.renderPosition.y + (ctl.crouch ? 1.15 : 1.55), ctl.renderPosition.z);
      const back = new THREE.Vector3(0, 0, 1).applyEuler(rot);
      const right = new THREE.Vector3(1, 0, 0).applyEuler(rot);
      const desired = target.clone().addScaledVector(back, this.zoom).addScaledVector(right, 0.55).add(new THREE.Vector3(0, 0.25, 0));
      const ground = this.terrain.height(desired.x, desired.z) + 0.4;
      if (desired.y < ground) desired.y = ground;
      // don't look through walls: pull in in front of the first surface
      const pivot = target.clone().addScaledVector(right, 0.2);
      const hit = occlude?.(pivot, desired);
      if (hit !== null && hit !== undefined) desired.lerpVectors(pivot, desired, Math.max(0.05, (hit - 0.25) / pivot.distanceTo(desired)));
      if (!this.initialized) this.thirdPos.copy(desired);
      this.thirdPos.lerp(desired, Math.min(1, dt * 12));
      cam.position.copy(this.thirdPos);
      cam.lookAt(target.clone().addScaledVector(right, 0.55));
    }
    // zoom = narrower field of view (eases in/out)
    const fov = CameraRig.BASE_FOV / ((this.mode === 'first' ? this.fpZoom : 1) * (this.zoomHeld ? 3 : 1));
    cam.fov += (fov - cam.fov) * Math.min(1, dt * 10);
    cam.updateProjectionMatrix();
    cam.updateMatrixWorld();
    this.initialized = true;
  }
}
