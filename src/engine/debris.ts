import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';
import { PoseTrack } from '../shared/frames/track';

interface Chunk {
  body: RAPIER.RigidBody;
  /** Its last two steps (drawn between them, like everything that moves). */
  track: PoseTrack;
  age: number;
  life: number;
  scale: number;
}

const MAX = 160;
/** Fragments this far (m) from the world's origin are gone (the physics bubble moved on). */
const FAR = 6000;

/** Where the Rapier world sits in the world (its frame's pose as drawn; null: the identity). */
export interface DebrisFrame {
  p: readonly number[];
  q: readonly number[];
}

/**
 * Rigid-body debris: explosion fragments are real Rapier bodies (convex, the world's own gravity,
 * bounce and roll on the terrain heightfield), drawn with one instanced mesh. Positions are in the
 * Rapier world's frame (the physics bubble); the instances are written relative to the render origin.
 */
export class Debris {
  readonly mesh: THREE.InstancedMesh;
  private chunks: Chunk[] = [];
  private m = new THREE.Matrix4();
  private q = new THREE.Quaternion();
  private p = new THREE.Vector3();
  private s = new THREE.Vector3();
  private fq = new THREE.Quaternion();

  constructor(
    private R: typeof RAPIER,
    private world: RAPIER.World,
    material: THREE.Material,
    /** Fragment shape (unit size); default: lumpy rock. */
    geometry?: THREE.BufferGeometry,
  ) {
    const g = geometry ?? new THREE.DodecahedronGeometry(1, 0);
    const pos = g.getAttribute('position');
    if (!geometry) {
      // lumpy, non-uniform fragments
      for (let i = 0; i < pos.count; i++) pos.setXYZ(i, pos.getX(i) * (0.8 + ((i * 7) % 5) * 0.08), pos.getY(i) * 0.7, pos.getZ(i));
      g.computeVertexNormals();
      const col = new Float32Array(pos.count * 3).fill(0.23);
      g.setAttribute('color', new THREE.BufferAttribute(col, 3));
    }
    this.mesh = new THREE.InstancedMesh(g, material, MAX);
    this.mesh.count = 0;
    this.mesh.castShadow = true;
    this.mesh.receiveShadow = true;
    this.mesh.frustumCulled = false;
  }

  get count() {
    return this.chunks.length;
  }

  /**
   * Throw `n` fragments from `at` (the Rapier world's coordinates): a ground blast by default ("up"
   * is +y there), or a cone along `dir` (hull breach); `base`: the velocity of what they came off.
   */
  burst(at: THREE.Vector3, n = 10, opts: { dir?: THREE.Vector3; speed?: number; size?: number; spread?: number; base?: THREE.Vector3 } = {}) {
    const R = this.R;
    for (let i = 0; i < n; i++) {
      if (this.chunks.length >= MAX) this.remove(0);
      const size = (0.05 + Math.random() ** 2 * 0.22) * (opts.size ?? 1);
      const a = Math.random() * Math.PI * 2;
      const up = 0.5 + Math.random() * 0.9;
      const sp = (3 + Math.random() * 8) * (opts.speed ?? 1);
      const v = new THREE.Vector3(Math.cos(a) * Math.cos(up), Math.sin(up), Math.sin(a) * Math.cos(up));
      const o = new THREE.Vector3(Math.cos(a) * 0.4, 0.3, Math.sin(a) * 0.4);
      if (opts.dir) {
        v.randomDirection().multiplyScalar(opts.spread ?? 0.6).add(opts.dir).normalize();
        o.randomDirection().multiplyScalar(0.35);
      }
      const desc = R.RigidBodyDesc.dynamic()
        .setTranslation(at.x + o.x, at.y + o.y, at.z + o.z)
        .setLinvel(v.x * sp + (opts.base?.x ?? 0), v.y * sp + (opts.base?.y ?? 0), v.z * sp + (opts.base?.z ?? 0))
        .setAngvel({ x: (Math.random() - 0.5) * 12, y: (Math.random() - 0.5) * 12, z: (Math.random() - 0.5) * 12 })
        .setCcdEnabled(true);
      const body = this.world.createRigidBody(desc);
      this.world.createCollider(R.ColliderDesc.ball(size * 0.85).setRestitution(0.25).setFriction(0.9).setDensity(2600), body);
      const track = new PoseTrack();
      const t = body.translation();
      track.snap([t.x, t.y, t.z], [0, 0, 0, 1]);
      this.chunks.push({ body, track, age: 0, life: 25 + Math.random() * 15, scale: size });
    }
  }

  /** Called after each physics step: where each fragment got to (what is drawn), its age. */
  update(dt: number) {
    for (let i = this.chunks.length - 1; i >= 0; i--) {
      const c = this.chunks[i];
      c.age += dt;
      const t = c.body.translation();
      const r = c.body.rotation();
      c.track.push(t.x, t.y, t.z, r.x, r.y, r.z, r.w);
      if (c.age > c.life || t.x * t.x + t.y * t.y + t.z * t.z > FAR * FAR) this.remove(i);
    }
  }

  /**
   * Once per frame: write instance transforms, between the last two steps (`alpha`), from the
   * world's frame (`frame`, as drawn; null = the identity) into render space (`origin`: the world
   * position of render-space zero).
   */
  sync(frame: DebrisFrame | null, origin: { x: number; y: number; z: number }, alpha = 1) {
    const fq = frame ? this.fq.set(frame.q[0], frame.q[1], frame.q[2], frame.q[3]) : null;
    for (let i = 0; i < this.chunks.length; i++) {
      const c = this.chunks[i];
      const fade = Math.min(1, (c.life - c.age) / 2);
      c.track.at(alpha, _p);
      c.track.quatAt(alpha, _q);
      this.p.set(_p[0], _p[1], _p[2]);
      this.q.set(_q[0], _q[1], _q[2], _q[3]);
      if (frame && fq) {
        this.p.applyQuaternion(fq);
        this.p.x += frame.p[0];
        this.p.y += frame.p[1];
        this.p.z += frame.p[2];
        this.q.premultiply(fq);
      }
      this.p.x -= origin.x;
      this.p.y -= origin.y;
      this.p.z -= origin.z;
      this.m.compose(this.p, this.q, this.s.setScalar(c.scale * fade));
      this.mesh.setMatrixAt(i, this.m);
    }
    this.mesh.count = this.chunks.length;
    this.mesh.instanceMatrix.needsUpdate = true;
  }

  /**
   * The world was re-laid: every fragment carried into the new frame (`carry` maps p, v, q; `track`
   * carries both drawn steps, shared/frames/track.ts).
   */
  rebase(
    carry: (p: [number, number, number], v: [number, number, number], q: [number, number, number, number]) => { p: number[]; v: number[]; q: number[] },
    turn: (w: [number, number, number]) => number[],
    track?: (t: PoseTrack) => void,
  ) {
    for (const c of this.chunks) {
      track?.(c.track);
      const t = c.body.translation();
      const r = c.body.rotation();
      const lv = c.body.linvel();
      const av = c.body.angvel();
      const n = carry([t.x, t.y, t.z], [lv.x, lv.y, lv.z], [r.x, r.y, r.z, r.w]);
      const w = turn([av.x, av.y, av.z]);
      c.body.setTranslation({ x: n.p[0], y: n.p[1], z: n.p[2] }, true);
      c.body.setRotation({ x: n.q[0], y: n.q[1], z: n.q[2], w: n.q[3] }, true);
      c.body.setLinvel({ x: n.v[0], y: n.v[1], z: n.v[2] }, true);
      c.body.setAngvel({ x: w[0], y: w[1], z: w[2] }, true);
    }
  }

  private remove(i: number) {
    this.world.removeRigidBody(this.chunks[i].body);
    this.chunks.splice(i, 1);
  }
}

const _p = [0, 0, 0];
const _q = [0, 0, 0, 1];
