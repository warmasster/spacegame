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
  private smoothEye = new THREE.Vector3();
  private thirdPos = new THREE.Vector3();
  private initialized = false;
  zoom = 3.4;

  constructor(
    private camera: THREE.PerspectiveCamera,
    private terrain: LunarTerrain,
  ) {
    window.addEventListener('wheel', (e) => {
      if (this.mode === 'third') this.zoom = THREE.MathUtils.clamp(this.zoom + Math.sign(e.deltaY) * 0.4, 1.6, 9);
    });
  }

  toggle() {
    this.mode = this.mode === 'first' ? 'third' : 'first';
    this.initialized = false;
  }

  update(dt: number, ctl: PlayerController, astronaut: Astronaut) {
    const cam = this.camera;
    astronaut.eyePosition(this.eye);
    if (!this.initialized) {
      this.smoothEye.copy(this.eye);
    }
    // keep horizontal exact, soften vertical gait bounce a little
    this.smoothEye.x = this.eye.x;
    this.smoothEye.z = this.eye.z;
    this.smoothEye.y += (this.eye.y - this.smoothEye.y) * Math.min(1, dt * 18);

    const rot = new THREE.Euler(ctl.pitch, ctl.yaw, 0, 'YXZ');
    if (this.mode === 'third') rot.set(THREE.MathUtils.clamp(ctl.pitch + this.orbitPitch, -1.4, 1.3), ctl.yaw + this.orbit, 0, 'YXZ');
    if (this.mode === 'first') {
      cam.layers.disable(HELMET_LAYER);
      cam.near = 0.05;
      cam.position.copy(this.smoothEye);
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
      if (!this.initialized) this.thirdPos.copy(desired);
      this.thirdPos.lerp(desired, Math.min(1, dt * 12));
      cam.position.copy(this.thirdPos);
      cam.lookAt(target.clone().addScaledVector(right, 0.55));
    }
    cam.updateProjectionMatrix();
    cam.updateMatrixWorld();
    this.initialized = true;
  }
}
