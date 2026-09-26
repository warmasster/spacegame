import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';
import type { CargoDef } from '../../shared/ship/def';
import type { Physics } from '../world/physics';

/**
 * Loose cargo: real dynamic Rapier boxes (lunar gravity, friction on the deck). Astronauts push
 * them (PlayerController), blasts throw them, and a blown-out deck plate lets them drop.
 * Simulated per client for now (not replicated), like debris.
 */
export class ShipCargo {
  readonly group = new THREE.Group();
  readonly bodies: RAPIER.RigidBody[] = [];
  /** Collider handles of the boxes (rockets hit them). */
  readonly handles = new Set<number>();
  private meshes: THREE.Mesh[] = [];

  constructor(
    private physics: Physics,
    defs: CargoDef[],
    shipMatrix: THREE.Matrix4,
    shipYaw: number,
    gravity: number,
    mats: { orange: THREE.Material; grey: THREE.Material; strap: THREE.Material },
  ) {
    const R = physics.rapier;
    for (const c of defs) {
      const p = new THREE.Vector3(...c.pos).applyMatrix4(shipMatrix);
      const q = new THREE.Quaternion().setFromEuler(new THREE.Euler(0, shipYaw + c.yaw, 0));
      const body = physics.world.createRigidBody(
        R.RigidBodyDesc.dynamic()
          .setTranslation(p.x, p.y, p.z)
          .setRotation({ x: q.x, y: q.y, z: q.z, w: q.w })
          .setGravityScale(gravity / 9.81)
          .setLinearDamping(0.05)
          .setAngularDamping(0.2)
          .setCcdEnabled(true),
      );
      const [hx, hy, hz] = c.half;
      const col = physics.world.createCollider(R.ColliderDesc.cuboid(hx, hy, hz).setMass(c.mass).setFriction(0.7).setRestitution(0.05), body);
      this.handles.add(col.handle);
      this.bodies.push(body);
      // crate with two straps: one mesh, two material groups
      const box = new THREE.BoxGeometry(hx * 2, hy * 2, hz * 2);
      const straps = [hy * 0.5, -hy * 0.5].map((y) => new THREE.BoxGeometry(hx * 2 + 0.012, 0.04, hz * 2 + 0.012).translate(0, y, 0));
      const g = new THREE.BufferGeometry();
      const parts = [box, ...straps].map((x) => x.toNonIndexed());
      const pos: number[] = [];
      const nrm: number[] = [];
      parts.forEach((x, i) => {
        g.addGroup(pos.length / 3, x.getAttribute('position').count, i === 0 ? 0 : 1);
        pos.push(...(x.getAttribute('position').array as Float32Array));
        nrm.push(...(x.getAttribute('normal').array as Float32Array));
      });
      g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
      g.setAttribute('normal', new THREE.Float32BufferAttribute(nrm, 3));
      const mesh = new THREE.Mesh(g, [c.paint === 'orange' ? mats.orange : mats.grey, mats.strap]);
      mesh.castShadow = mesh.receiveShadow = true;
      this.meshes.push(mesh);
      this.group.add(mesh);
    }
    this.sync();
  }

  /** Once per frame: follow the bodies. */
  sync() {
    this.bodies.forEach((b, i) => {
      const t = b.translation();
      const r = b.rotation();
      this.meshes[i].position.set(t.x, t.y, t.z);
      this.meshes[i].quaternion.set(r.x, r.y, r.z, r.w);
    });
  }

  /** Something under them changed (a deck plate blew out): let them fall. */
  wake() {
    for (const b of this.bodies) b.wakeUp();
  }

  /** Blast wave: impulse away from the explosion, falling off with distance. */
  blast(at: THREE.Vector3, radius = 6, strength = 260) {
    const d = new THREE.Vector3();
    for (const b of this.bodies) {
      const t = b.translation();
      d.set(t.x - at.x, t.y - at.y, t.z - at.z);
      const dist = d.length();
      if (dist > radius) continue;
      const k = strength * (1 - dist / radius);
      d.normalize().multiplyScalar(k);
      d.y += k * 0.35;
      b.applyImpulse({ x: d.x, y: d.y, z: d.z }, true);
      b.applyTorqueImpulse({ x: (Math.random() - 0.5) * k * 0.3, y: (Math.random() - 0.5) * k * 0.3, z: (Math.random() - 0.5) * k * 0.3 }, true);
    }
  }

  dispose() {
    for (const b of this.bodies) this.physics.world.removeRigidBody(b);
  }
}
