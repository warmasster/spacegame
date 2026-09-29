import * as THREE from 'three';
import type { V3 } from '../../shared/ship/geom';
import type { ShipDef } from '../../shared/ship/def';

const _a = new THREE.Vector3();
const _b = new THREE.Vector3();
const _y = new THREE.Vector3(0, 1, 0);

interface Leg {
  hip: V3;
  strut: THREE.Mesh;
  piston: THREE.Mesh;
  pad: THREE.Mesh;
}

/**
 * Landing gear, animated: each leg's piston runs from its strut to the foot wherever the flight's
 * contact model says the foot is (retracted into the belly, hanging in the air, or pushed up the
 * strut by the ground under it). Ship space (child of the ship's root).
 */
export class GearFx {
  readonly group = new THREE.Group();
  private legs: Leg[] = [];

  constructor(def: ShipDef, mats: { dark: THREE.Material; chrome: THREE.Material }) {
    const rod = new THREE.CylinderGeometry(1, 1, 1, 14).translate(0, 0.5, 0);
    const pad = new THREE.LatheGeometry(
      [[0, 0], [0.34, 0], [0.36, 0.04], [0.3, 0.1], [0.12, 0.13], [0, 0.13]].map(([x, y]) => new THREE.Vector2(x, y)),
      20,
    );
    for (const hip of def.gear?.legs ?? []) {
      const strut = new THREE.Mesh(rod, mats.dark);
      const piston = new THREE.Mesh(rod, mats.chrome);
      const foot = new THREE.Mesh(pad, mats.dark);
      for (const m of [strut, piston, foot]) {
        m.castShadow = m.receiveShadow = true;
        this.group.add(m);
      }
      // the upper strut hangs from the hip, fixed
      strut.position.set(hip[0], hip[1] - 0.45, hip[2]);
      strut.scale.set(0.11, 0.4, 0.11);
      this.legs.push({ hip, strut, piston, pad: foot });
    }
  }

  /** Feet (ship space) this frame. */
  update(feet: V3[]) {
    this.legs.forEach((leg, i) => {
      const f = feet[i];
      if (!f) return;
      _a.set(leg.hip[0], leg.hip[1] - 0.1, leg.hip[2]);
      _b.set(f[0], f[1] + 0.13, f[2]);
      const len = _a.distanceTo(_b);
      leg.piston.visible = len > 0.02;
      leg.piston.position.copy(_b);
      leg.piston.quaternion.setFromUnitVectors(_y, _a.sub(_b).normalize());
      leg.piston.scale.set(0.075, len, 0.075);
      leg.pad.position.set(f[0], f[1] + 0.005, f[2]);
      // retracted: the strut folds away with it
      leg.strut.visible = f[1] < leg.hip[1] - 0.2;
    });
  }
}
