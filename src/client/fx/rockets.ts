import * as THREE from 'three';
import type { LunarTerrain } from '../../shared/terrain';
import type { Particles } from './particles';

export const ROCKET_SPEED = 42; // m/s
const ROCKET_LIFE = 9;
const GRAVITY = 1.62;

interface Rocket {
  owner: number;
  pos: THREE.Vector3;
  vel: THREE.Vector3;
  age: number;
  mesh: THREE.Object3D;
}

/** Lathe profile helper (keeps props from looking like primitives). */
function lathe(points: Array<[number, number]>, segs = 24) {
  const g = new THREE.LatheGeometry(points.map(([r, y]) => new THREE.Vector2(r, y)), segs);
  g.computeVertexNormals();
  return g;
}

/** Visual rocket: body, nose, fins. Points along +Z. */
function makeRocketMesh(mats: { body: THREE.Material; dark: THREE.Material }) {
  const g = new THREE.Group();
  const body = new THREE.Mesh(
    lathe([[0, -0.32], [0.035, -0.31], [0.042, -0.26], [0.042, 0.18], [0.036, 0.24], [0.018, 0.31], [0, 0.33]]),
    mats.body,
  );
  body.rotation.x = Math.PI / 2;
  g.add(body);
  for (let i = 0; i < 4; i++) {
    const fin = new THREE.Mesh(new THREE.BoxGeometry(0.004, 0.07, 0.11), mats.dark);
    fin.position.set(Math.cos((i * Math.PI) / 2) * 0.06, Math.sin((i * Math.PI) / 2) * 0.06, -0.24);
    fin.rotation.z = (i * Math.PI) / 2;
    g.add(fin);
  }
  return g;
}

/** Shoulder-carried launcher tube, local +Z forward (attached to the chest bone). */
export function makeLauncherMesh() {
  const olive = new THREE.MeshStandardMaterial({ color: 0x4d5243, roughness: 0.55, metalness: 0.35 });
  const dark = new THREE.MeshStandardMaterial({ color: 0x1b1c1e, roughness: 0.45, metalness: 0.7 });
  const g = new THREE.Group();
  const tube = new THREE.Mesh(
    lathe([[0.052, -0.55], [0.062, -0.54], [0.064, -0.5], [0.056, -0.47], [0.056, 0.4], [0.064, 0.43], [0.066, 0.5], [0.055, 0.52]], 28),
    olive,
  );
  tube.rotation.x = Math.PI / 2;
  const inner = new THREE.Mesh(new THREE.CylinderGeometry(0.05, 0.05, 1.04, 20, 1, true), dark);
  inner.material = dark.clone();
  (inner.material as THREE.MeshStandardMaterial).side = THREE.BackSide;
  inner.rotation.x = Math.PI / 2;
  const grip = new THREE.Mesh(new THREE.BoxGeometry(0.03, 0.12, 0.05), dark);
  grip.position.set(0, -0.09, 0.12);
  grip.rotation.x = 0.25;
  const sight = new THREE.Mesh(new THREE.BoxGeometry(0.035, 0.045, 0.1), dark);
  sight.position.set(-0.06, 0.05, 0.16);
  for (const m of [tube, inner, grip, sight]) {
    m.castShadow = true;
    g.add(m);
  }
  return g;
}

/**
 * Rockets in flight. Every client simulates every rocket (same start = same arc); only the
 * shooter reports the impact, the server turns it into an explosion + crater for everyone.
 */
export class Rockets {
  readonly group = new THREE.Group();
  private list: Rocket[] = [];
  private mats = {
    body: new THREE.MeshStandardMaterial({ color: 0xc9c6bb, roughness: 0.4, metalness: 0.3 }),
    dark: new THREE.MeshStandardMaterial({ color: 0x222222, roughness: 0.5, metalness: 0.5 }),
  };
  private light = new THREE.PointLight(0xffb070, 0, 18, 2);
  private flash = 0;
  private tmp = new THREE.Vector3();

  constructor(
    private terrain: LunarTerrain,
    private particles: Particles,
  ) {
    this.group.add(this.light);
  }

  spawn(owner: number, origin: THREE.Vector3, dir: THREE.Vector3) {
    const mesh = makeRocketMesh(this.mats);
    mesh.position.copy(origin);
    this.group.add(mesh);
    this.list.push({ owner, pos: origin.clone(), vel: dir.clone().normalize().multiplyScalar(ROCKET_SPEED), age: 0, mesh });
    for (let i = 0; i < 25; i++) {
      this.particles.emit('glow', {
        pos: origin,
        vel: this.tmp.randomDirection().multiplyScalar(3).addScaledVector(dir, -2),
        color: [3, 1.8, 0.8],
        life: 0.12 + Math.random() * 0.15,
        size: 0.1,
      });
    }
  }

  /** Explosion confirmed by the server: flash + remove the owner's nearest rocket. */
  explode(owner: number, at: THREE.Vector3) {
    let best = -1;
    let bestD = 20;
    this.list.forEach((r, i) => {
      const d = r.pos.distanceTo(at);
      if (r.owner === owner && d < bestD) {
        bestD = d;
        best = i;
      }
    });
    if (best >= 0) this.remove(best);
    this.particles.explosion(at, this.terrain.height(at.x, at.z) - 0.3);
    this.light.position.copy(at).y += 1.5;
    this.flash = 1;
  }

  /**
   * Advance rockets. `targets` = other astronauts (centre positions); returns impacts of
   * rockets owned by `me` that must be reported to the server.
   */
  update(dt: number, me: number, targets: Array<{ id: number; pos: THREE.Vector3 }>): THREE.Vector3[] {
    const impacts: THREE.Vector3[] = [];
    this.flash = Math.max(0, this.flash - dt * 5);
    this.light.intensity = this.flash * this.flash * 400;
    for (let i = this.list.length - 1; i >= 0; i--) {
      const r = this.list[i];
      r.age += dt;
      const steps = 4;
      let hit: THREE.Vector3 | null = null;
      for (let s = 0; s < steps && !hit; s++) {
        const h = dt / steps;
        r.vel.y -= GRAVITY * h;
        r.pos.addScaledVector(r.vel, h);
        if (r.pos.y <= this.terrain.height(r.pos.x, r.pos.z)) hit = r.pos.clone();
        for (const t of targets) {
          if (r.age < 0.12 && t.id === r.owner) continue;
          if (r.pos.distanceTo(t.pos) < 0.7) hit = r.pos.clone();
        }
      }
      r.mesh.position.copy(r.pos);
      r.mesh.lookAt(this.tmp.copy(r.pos).add(r.vel));
      // exhaust (vacuum: a short bright jet, no smoke trail)
      const back = this.tmp.copy(r.vel).normalize().multiplyScalar(-0.35).add(r.pos);
      for (let k = 0; k < 2; k++) {
        this.particles.emit('glow', {
          pos: back,
          vel: r.vel.clone().multiplyScalar(-0.15).add(new THREE.Vector3().randomDirection().multiplyScalar(0.8)),
          color: [3.5, 2.1, 1.0],
          life: 0.06 + Math.random() * 0.08,
          size: 0.14,
        });
      }
      if (hit || r.age > ROCKET_LIFE) {
        if (hit && r.owner === me) impacts.push(hit);
        // non-owners keep a short visual until the server's explosion arrives
        if (r.owner === me || r.age > ROCKET_LIFE || hit) this.remove(i);
      }
    }
    return impacts;
  }

  private remove(i: number) {
    const r = this.list[i];
    this.group.remove(r.mesh);
    this.list.splice(i, 1);
  }
}
