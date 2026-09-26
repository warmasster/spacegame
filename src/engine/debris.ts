import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';

interface Chunk {
  body: RAPIER.RigidBody;
  age: number;
  life: number;
  scale: number;
}

const MAX = 160;

/**
 * Rigid-body debris: explosion fragments are real Rapier bodies (convex, lunar gravity,
 * bounce and roll on the terrain heightfield), drawn with one instanced mesh.
 */
export class Debris {
  readonly mesh: THREE.InstancedMesh;
  private chunks: Chunk[] = [];
  private m = new THREE.Matrix4();
  private q = new THREE.Quaternion();
  private p = new THREE.Vector3();
  private s = new THREE.Vector3();

  constructor(
    private R: typeof RAPIER,
    private world: RAPIER.World,
    private gravity: number,
    material: THREE.Material,
  ) {
    const g = new THREE.DodecahedronGeometry(1, 0);
    const pos = g.getAttribute('position');
    // lumpy, non-uniform fragments
    for (let i = 0; i < pos.count; i++) pos.setXYZ(i, pos.getX(i) * (0.8 + ((i * 7) % 5) * 0.08), pos.getY(i) * 0.7, pos.getZ(i));
    g.computeVertexNormals();
    const col = new Float32Array(pos.count * 3).fill(0.23);
    g.setAttribute('color', new THREE.BufferAttribute(col, 3));
    this.mesh = new THREE.InstancedMesh(g, material, MAX);
    this.mesh.count = 0;
    this.mesh.castShadow = true;
    this.mesh.receiveShadow = true;
    this.mesh.frustumCulled = false;
  }

  get count() {
    return this.chunks.length;
  }

  burst(at: THREE.Vector3, n = 10) {
    const R = this.R;
    for (let i = 0; i < n; i++) {
      if (this.chunks.length >= MAX) this.remove(0);
      const size = 0.05 + Math.random() ** 2 * 0.22;
      const a = Math.random() * Math.PI * 2;
      const up = 0.5 + Math.random() * 0.9;
      const sp = 3 + Math.random() * 8;
      const desc = R.RigidBodyDesc.dynamic()
        .setTranslation(at.x + Math.cos(a) * 0.4, at.y + 0.3, at.z + Math.sin(a) * 0.4)
        .setLinvel(Math.cos(a) * Math.cos(up) * sp, Math.sin(up) * sp, Math.sin(a) * Math.cos(up) * sp)
        .setAngvel({ x: (Math.random() - 0.5) * 12, y: (Math.random() - 0.5) * 12, z: (Math.random() - 0.5) * 12 })
        .setGravityScale(this.gravity / 9.81)
        .setCcdEnabled(true);
      const body = this.world.createRigidBody(desc);
      this.world.createCollider(R.ColliderDesc.ball(size * 0.85).setRestitution(0.25).setFriction(0.9).setDensity(2600), body);
      this.chunks.push({ body, age: 0, life: 25 + Math.random() * 15, scale: size });
    }
  }

  /** Called after each physics step. */
  update(dt: number) {
    for (let i = this.chunks.length - 1; i >= 0; i--) {
      const c = this.chunks[i];
      c.age += dt;
      const t = c.body.translation();
      if (c.age > c.life || t.y < -2000) this.remove(i);
    }
  }

  /** Once per frame: write instance transforms. */
  sync() {
    this.chunks.forEach((c, i) => {
      const t = c.body.translation();
      const r = c.body.rotation();
      const fade = Math.min(1, (c.life - c.age) / 2);
      this.m.compose(this.p.set(t.x, t.y, t.z), this.q.set(r.x, r.y, r.z, r.w), this.s.setScalar(c.scale * fade));
      this.mesh.setMatrixAt(i, this.m);
    });
    this.mesh.count = this.chunks.length;
    this.mesh.instanceMatrix.needsUpdate = true;
  }

  private remove(i: number) {
    this.world.removeRigidBody(this.chunks[i].body);
    this.chunks.splice(i, 1);
  }
}
