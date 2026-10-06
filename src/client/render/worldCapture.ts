import * as THREE from 'three';
import type { CaptureScope } from './pip';
import type { SphereTerrain } from '../world/sphereTerrain';
import type { RockField } from '../world/rocks';
import type { Sky } from '../world/sky';
import type { ShipClient } from '../ship/ship';

/** World adapter: secondary views see outside even when the helmet's portal view hides it. */
export class WorldCapture implements CaptureScope {
  private objects: THREE.Object3D[] = [];
  private visible: boolean[] = [];
  private skyPosition = new THREE.Vector3();
  private interiors: Array<{ ship: ShipClient; visible: boolean[] }> = [];
  constructor(private main: THREE.Camera, private sky: Sky, private grounds: Array<{ terrain: SphereTerrain; rocks: RockField }>, ships: ShipClient[]) {
    this.objects.push(sky.group);
    for (const ground of grounds) this.objects.push(ground.terrain.group, ground.rocks.group);
    for (const ship of ships) {
      this.objects.push(ship.view.root);
      this.interiors.push({ ship, visible: [] });
    }
    for (let i = 0; i < this.objects.length; i++) this.visible.push(false);
  }
  begin(camera: THREE.PerspectiveCamera) {
    for (let i = 0; i < this.objects.length; i++) { this.visible[i] = this.objects[i].visible; this.objects[i].visible = true; }
    for (const it of this.interiors) {
      const objects = it.ship.view.captureInteriors();
      for (let i = 0; i < objects.length; i++) { it.visible[i] = objects[i].visible; objects[i].visible = false; }
    }
    this.skyPosition.copy(this.sky.group.position);
    this.sky.group.position.copy(camera.position);
    // Render existing nodes only: a monitor never allocates LOD children or schedules worker jobs.
    for (const ground of this.grounds) ground.terrain.update(camera, false);
  }
  end() {
    this.sky.group.position.copy(this.skyPosition);
    try { for (const ground of this.grounds) ground.terrain.update(this.main, false); }
    finally {
      for (const it of this.interiors) {
        const objects = it.ship.view.captureInteriors();
        for (let i = 0; i < objects.length; i++) objects[i].visible = it.visible[i];
      }
      for (let i = 0; i < this.objects.length; i++) this.objects[i].visible = this.visible[i];
    }
  }
}
