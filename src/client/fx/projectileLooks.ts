import * as THREE from 'three';
import type { Particles } from './particles';

/**
 * How each projectile look is drawn (the catalog's `look`, shared/items/projectiles.ts): the mesh
 * of one in flight (+Z = its nose), what it leaves behind every frame, and the flash at the muzzle.
 * Everything in world coordinates; `carry` is a world velocity the particles move with.
 */
export interface ProjectileLook {
  /** A new mesh (the rockets keep their spares and reuse them). */
  mesh(): THREE.Object3D;
  /** Once per rendered frame while it flies: `at` its tail as drawn, `nose` unit, `vel` its world velocity. */
  trail?(particles: Particles, at: THREE.Vector3, nose: THREE.Vector3, vel: THREE.Vector3): void;
  /** When it leaves the muzzle (`at`), moving with the launcher (`carry`). */
  launch?(particles: Particles, at: THREE.Vector3, nose: THREE.Vector3, carry: THREE.Vector3): void;
  /** Half its length along the nose (m): the trail starts behind it. */
  tail: number;
}

export const PROJECTILE_LOOKS: Record<string, ProjectileLook> = {};

export function defineProjectileLook(id: string, look: ProjectileLook) {
  if (PROJECTILE_LOOKS[id]) throw new Error(`projectile look "${id}" defined twice`);
  PROJECTILE_LOOKS[id] = look;
}

const _v = new THREE.Vector3();
const _w = new THREE.Vector3();

/** Lathe profile helper (keeps props from looking like primitives). */
function lathe(points: Array<[number, number]>, segs = 24) {
  const g = new THREE.LatheGeometry(points.map(([r, y]) => new THREE.Vector2(r, y)), segs);
  g.computeVertexNormals();
  return g;
}

// --- rocket: body, nose and fins; a short bright exhaust (vacuum: no smoke trail) ---

let rocketParts: { body: THREE.BufferGeometry; fin: THREE.BufferGeometry; mats: { body: THREE.Material; dark: THREE.Material } } | null = null;

defineProjectileLook('rocket', {
  tail: 0.35,
  mesh() {
    rocketParts ??= {
      body: lathe([[0, -0.32], [0.035, -0.31], [0.042, -0.26], [0.042, 0.18], [0.036, 0.24], [0.018, 0.31], [0, 0.33]]),
      fin: new THREE.BoxGeometry(0.004, 0.07, 0.11),
      mats: {
        body: new THREE.MeshStandardMaterial({ color: 0xc9c6bb, roughness: 0.4, metalness: 0.3 }),
        dark: new THREE.MeshStandardMaterial({ color: 0x222222, roughness: 0.5, metalness: 0.5 }),
      },
    };
    const g = new THREE.Group();
    const body = new THREE.Mesh(rocketParts.body, rocketParts.mats.body);
    body.rotation.x = Math.PI / 2;
    g.add(body);
    for (let i = 0; i < 4; i++) {
      const fin = new THREE.Mesh(rocketParts.fin, rocketParts.mats.dark);
      fin.position.set(Math.cos((i * Math.PI) / 2) * 0.06, Math.sin((i * Math.PI) / 2) * 0.06, -0.24);
      fin.rotation.z = (i * Math.PI) / 2;
      g.add(fin);
    }
    return g;
  },
  trail(particles, at, nose, vel) {
    for (let k = 0; k < 2; k++) {
      particles.emit('glow', {
        pos: at,
        // a few m/s back from the rocket itself, whatever it carries from the launcher
        vel: _v.randomDirection().multiplyScalar(0.8).addScaledVector(nose, -6),
        carry: vel,
        color: [3.5, 2.1, 1.0],
        life: 0.06 + Math.random() * 0.08,
        size: 0.14,
      });
    }
  },
  launch(particles, at, nose, carry) {
    for (let i = 0; i < 25; i++) {
      particles.emit('glow', {
        pos: at,
        vel: _v.randomDirection().multiplyScalar(3).addScaledVector(nose, -2),
        carry,
        color: [3, 1.8, 0.8],
        life: 0.12 + Math.random() * 0.15,
        size: 0.1,
      });
    }
  },
});

/** A visual variant reuses the geometry/material pool, scaled at creation (never per frame). */
export function scaledProjectileLook(base: ProjectileLook, scale: number): ProjectileLook {
  return { tail: base.tail * scale, mesh() { const mesh = base.mesh(); mesh.scale.multiplyScalar(scale); return mesh; }, trail: base.trail, launch: base.launch };
}
defineProjectileLook('minimissile', scaledProjectileLook(PROJECTILE_LOOKS.rocket, 0.55));

// --- tracer: a bright streak (a bullet is too small and fast to see; its tracer is what shows) ---

let tracerParts: { geo: THREE.BufferGeometry; mat: THREE.Material } | null = null;

defineProjectileLook('tracer', {
  tail: 0.6,
  mesh() {
    tracerParts ??= {
      geo: new THREE.BoxGeometry(0.018, 0.018, 1.2),
      mat: new THREE.MeshBasicMaterial({ color: new THREE.Color(4, 2.6, 1.2), toneMapped: false }),
    };
    return new THREE.Mesh(tracerParts.geo, tracerParts.mat);
  },
  trail(particles, at, _nose, vel) {
    if (Math.random() > 0.5) return;
    particles.emit('glow', { pos: at, vel: _w.set(0, 0, 0), carry: vel, color: [2.4, 1.5, 0.7], life: 0.03, size: 0.05 });
  },
  launch(particles, at, nose, carry) {
    for (let i = 0; i < 8; i++) {
      particles.emit('glow', {
        pos: at,
        vel: _v.randomDirection().multiplyScalar(1.5).addScaledVector(nose, 4),
        carry,
        color: [3.2, 2.2, 1.1],
        life: 0.03 + Math.random() * 0.04,
        size: 0.06,
      });
    }
  },
});
