import * as THREE from 'three';
import { bodyAt, gravityAt, tangentFrame } from '../../shared/space/body';
import { origin } from '../render/origin';

/**
 * Minimal GPU-light particle pool: one Points draw call per blend mode, CPU simulation.
 * In vacuum there is no drag or smoke: dust flies on clean ballistic arcs and falls under the
 * gravity of the body it is on (whatever body: `gravity` is a share of the local pull), fire is a
 * brief flash. That alone reads as "Moon" on the Moon.
 *
 * Particles live in a small local space of their own: centred on the render origin (float32 holds
 * them to the micrometre wherever that is) and turned to the body's tangent frame there, so "up"
 * (dust arcs, where it settles, which way gravity pulls) is +y anywhere round the body. Both move
 * with the render origin; the particles in flight are carried across. Emitters speak world.
 */
interface Pool {
  pos: Float32Array;
  vel: Float32Array;
  col: Float32Array; // rgb
  life: Float32Array; // remaining
  maxLife: Float32Array;
  size: Float32Array;
  grav: Float32Array;
  floor: Float32Array;
  count: number;
  geo: THREE.BufferGeometry;
  alphaAttr: Float32Array;
  sizeAttr: Float32Array;
  colAttr: Float32Array;
  next: number;
  /** Slots alive (dense list) and each slot's place in it (-1 = dead): the update walks only these. */
  alive: Int32Array;
  nAlive: number;
  slot: Int32Array;
  /** Slots written since the last upload (only that range goes to the GPU); colours apart (they change on emit only). */
  lo: number;
  hi: number;
  cLo: number;
  cHi: number;
  glow: boolean;
}

export interface Emit {
  /** World position and velocity (world axes, relative to `carry`). */
  pos: THREE.Vector3;
  vel: THREE.Vector3;
  /** Velocity of what it comes off (a ship in flight, a rocket): added to `vel`. */
  carry?: { x: number; y: number; z: number } | readonly number[];
  color: [number, number, number];
  life: number;
  size: number;
  /** How much of the local gravity pulls it (1: falls like anything else there; 0/absent: none). */
  gravity?: number;
  /** How far below `pos` (m, along the local vertical) the particle settles (dust). */
  floor?: number;
}

const _lp = new THREE.Vector3();
const _lv = new THREE.Vector3();
const _q = [0, 0, 0, 1];

export class Particles {
  readonly group = new THREE.Group();
  private pools: Record<'glow' | 'dust', Pool>;
  private list: Pool[];
  private mats: THREE.ShaderMaterial[] = [];
  /** World → particle space: the rotation's inverse (the group's orientation is the rotation). */
  private inv = new THREE.Quaternion();
  /** The body's pull where particle space is (m/s², along its −y): taken again whenever it moves. */
  private g = 0;
  constructor(pixelRatio: number) {
    this.pools = {
      glow: this.makePool(1500, THREE.AdditiveBlending, pixelRatio, true),
      dust: this.makePool(5000, THREE.NormalBlending, pixelRatio, false),
    };
    this.list = [this.pools.glow, this.pools.dust];
    this.group.matrixAutoUpdate = false;
    this.relay();
    origin.onMove((dx, dy, dz) => this.relay(dx, dy, dz));
  }

  /**
   * The render origin moved by (dx, dy, dz) world metres: particle space follows it, turned to the
   * tangent frame there; the particles in flight are carried across unchanged in the world.
   */
  private relay(dx = 0, dy = 0, dz = 0) {
    const O = [origin.x, origin.y, origin.z];
    const oldRot = this.group.quaternion.clone();
    const body = bodyAt(O);
    tangentFrame(body, O, _q);
    const gw = gravityAt(body, O, [0, 0, 0]);
    this.g = Math.hypot(gw[0], gw[1], gw[2]);
    this.group.quaternion.set(_q[0], _q[1], _q[2], _q[3]);
    this.group.updateMatrix();
    this.group.updateMatrixWorld(true);
    this.inv.copy(this.group.quaternion).invert();
    // old particle space → world offset → new particle space
    const turn = this.inv.clone().multiply(oldRot);
    const shift = _lp.set(-dx, -dy, -dz).applyQuaternion(this.inv);
    for (const p of this.list) {
      for (let k = 0; k < p.nAlive; k++) {
        const j = p.alive[k] * 3;
        _lv.set(p.pos[j], p.pos[j + 1], p.pos[j + 2]).applyQuaternion(turn).add(shift);
        const fy = p.floor[p.alive[k]] - p.pos[j + 1];
        p.pos[j] = _lv.x;
        p.pos[j + 1] = _lv.y;
        p.pos[j + 2] = _lv.z;
        p.floor[p.alive[k]] = _lv.y + fy;
        _lv.set(p.vel[j], p.vel[j + 1], p.vel[j + 2]).applyQuaternion(turn);
        p.vel[j] = _lv.x;
        p.vel[j + 1] = _lv.y;
        p.vel[j + 2] = _lv.z;
      }
      p.lo = 0;
      p.hi = p.count - 1;
    }
  }

  private makePool(n: number, blending: THREE.Blending, pr: number, glow: boolean): Pool {
    const geo = new THREE.BufferGeometry();
    const posAttr = new Float32Array(n * 3);
    const alphaAttr = new Float32Array(n);
    const sizeAttr = new Float32Array(n);
    const colAttr = new Float32Array(n * 3);
    geo.setAttribute('position', new THREE.BufferAttribute(posAttr, 3).setUsage(THREE.DynamicDrawUsage));
    geo.setAttribute('alpha', new THREE.BufferAttribute(alphaAttr, 1).setUsage(THREE.DynamicDrawUsage));
    geo.setAttribute('psize', new THREE.BufferAttribute(sizeAttr, 1).setUsage(THREE.DynamicDrawUsage));
    geo.setAttribute('pcolor', new THREE.BufferAttribute(colAttr, 3).setUsage(THREE.DynamicDrawUsage));
    const mat = new THREE.ShaderMaterial({
      uniforms: { uScale: { value: 900 * pr } },
      vertexShader: /* glsl */ `
        attribute float alpha; attribute float psize; attribute vec3 pcolor;
        uniform float uScale;
        varying float vA; varying vec3 vC;
        #include <common>
        #include <logdepthbuf_pars_vertex>
        void main() {
          vA = alpha; vC = pcolor;
          vec4 mv = modelViewMatrix * vec4(position, 1.0);
          gl_Position = projectionMatrix * mv;
          gl_PointSize = alpha <= 0.0 ? 0.0 : clamp(psize * uScale / -mv.z, 1.0, 256.0);
          #include <logdepthbuf_vertex>
        }`,
      fragmentShader: /* glsl */ `
        varying float vA; varying vec3 vC;
        #include <common>
        #include <logdepthbuf_pars_fragment>
        void main() {
          #include <logdepthbuf_fragment>
          vec2 c = gl_PointCoord * 2.0 - 1.0;
          float r2 = dot(c, c);
          if (r2 > 1.0) discard;
          float a = ${glow ? 'exp(-r2 * 3.0)' : '(1.0 - r2) * (1.0 - r2)'} * vA;
          gl_FragColor = vec4(vC * ${glow ? 'a' : '1.0'}, ${glow ? '1.0' : 'a'});
        }`,
      blending,
      transparent: true,
      depthWrite: false,
    });
    this.mats.push(mat);
    const points = new THREE.Points(geo, mat);
    points.frustumCulled = false;
    points.renderOrder = glow ? 12 : 10;
    this.group.add(points);
    return {
      pos: posAttr, vel: new Float32Array(n * 3), col: colAttr, life: new Float32Array(n), maxLife: new Float32Array(n),
      size: new Float32Array(n), grav: new Float32Array(n), floor: new Float32Array(n), count: n, geo, alphaAttr, sizeAttr, colAttr, next: 0,
      alive: new Int32Array(n), nAlive: 0, slot: new Int32Array(n).fill(-1), lo: n, hi: -1, cLo: n, cHi: -1, glow,
    };
  }

  /** The renderer's pixel ratio changed (dynamic resolution): sprites keep their size on screen. */
  setPixelRatio(pr: number) {
    for (const m of this.mats) m.uniforms.uScale.value = 900 * pr;
  }

  /** A particle from the world (see Emit). */
  emit(kind: 'glow' | 'dust', e: Emit) {
    const lp = _lp.set(e.pos.x - origin.x, e.pos.y - origin.y, e.pos.z - origin.z).applyQuaternion(this.inv);
    const lv = _lv.copy(e.vel);
    const c = e.carry;
    if (c) {
      if ('x' in c) lv.add(c as THREE.Vector3Like);
      else {
        lv.x += c[0];
        lv.y += c[1];
        lv.z += c[2];
      }
    }
    lv.applyQuaternion(this.inv);
    this.emitLocal(kind, lp, lv, e);
  }

  /** World → particle space (a point). */
  local(p: THREE.Vector3, out: THREE.Vector3) {
    return out.set(p.x - origin.x, p.y - origin.y, p.z - origin.z).applyQuaternion(this.inv);
  }

  /** A particle already in particle space (y up); `e.pos` / `e.vel` are ignored. */
  private emitLocal(kind: 'glow' | 'dust', lp: THREE.Vector3, lv: THREE.Vector3, e: Omit<Emit, 'pos' | 'vel'>) {
    const p = this.pools[kind];
    const i = p.next;
    p.next = (p.next + 1) % p.count;
    const j = i * 3;
    p.pos[j] = lp.x;
    p.pos[j + 1] = lp.y;
    p.pos[j + 2] = lp.z;
    p.vel[j] = lv.x;
    p.vel[j + 1] = lv.y;
    p.vel[j + 2] = lv.z;
    p.col[j] = e.color[0];
    p.col[j + 1] = e.color[1];
    p.col[j + 2] = e.color[2];
    p.life[i] = p.maxLife[i] = e.life;
    p.size[i] = e.size;
    p.grav[i] = (e.gravity ?? 0) * this.g;
    p.floor[i] = e.floor !== undefined ? lp.y - e.floor : -1e9;
    if (p.slot[i] < 0) {
      p.slot[i] = p.nAlive;
      p.alive[p.nAlive++] = i;
    }
    if (i < p.cLo) p.cLo = i;
    if (i > p.cHi) p.cHi = i;
  }

  /**
   * Rocket impact at world point `atW`, `below` m over the ground (the ejecta settles there),
   * carried along at `carry` (world, m/s: the ship it hit). Real lunar ejecta is not a smooth dome:
   * it leaves in uneven rays and clumps, so directions and speeds are drawn from a few random
   * "jets" with noise, not uniformly. Built in particle space (y up).
   */
  explosion(atW: THREE.Vector3, below: number, scale = 1, carry?: THREE.Vector3) {
    const v = new THREE.Vector3();
    const p = new THREE.Vector3();
    const at = this.local(atW, new THREE.Vector3());
    const cv = carry ? carry.clone().applyQuaternion(this.inv) : new THREE.Vector3();
    const fx = (life: number, size: number, color: [number, number, number], gravity?: number, floor?: number) => ({ color, life, size, gravity, floor });
    const rnd = Math.random;
    // flash core: several offset, uneven blobs instead of one perfect sphere
    for (let i = 0; i < 9; i++) {
      p.copy(at).add(v.randomDirection().multiplyScalar(rnd() * 0.9));
      p.y = Math.max(p.y, at.y + 0.1);
      this.emitLocal('glow', p, v.randomDirection().multiplyScalar(1 + rnd() * 2).add(cv), fx(0.08 + rnd() * 0.18, 1.2 + rnd() * 2.4, [6, 3.5 + rnd() * 1.5, 1.5]));
    }
    // sparks, some fast streaks
    for (let i = 0; i < 90 * scale; i++) {
      v.randomDirection().multiplyScalar(3 + rnd() * rnd() * 22);
      v.y = Math.abs(v.y) * 0.9 + 0.5;
      this.emitLocal('glow', at, v.add(cv), fx(0.1 + rnd() * rnd() * 0.9, 0.05 + rnd() * 0.12, [4, 1.6 + rnd() * 1.4, 0.4], 1));
    }
    // glowing hot fragments that arc far and cool down
    for (let i = 0; i < 12 * scale; i++) {
      const a = rnd() * Math.PI * 2;
      v.set(Math.cos(a), 0.6 + rnd() * 1.2, Math.sin(a)).multiplyScalar(5 + rnd() * 9);
      this.emitLocal('glow', at, v.add(cv), fx(1.5 + rnd() * 2, 0.07, [2.2, 0.9, 0.3], 1));
    }
    // ejecta rays
    const rays = 7 + Math.floor(rnd() * 7);
    const rayDir: number[] = [];
    const rayW: number[] = [];
    for (let r = 0; r < rays; r++) {
      rayDir.push(rnd() * Math.PI * 2);
      rayW.push(0.4 + rnd() * rnd() * 1.6);
    }
    const total = 1100 * scale;
    for (let i = 0; i < total; i++) {
      let a: number;
      let sp: number;
      let up: number;
      if (rnd() < 0.7) {
        const r = Math.floor(rnd() * rays);
        a = rayDir[r] + (rnd() - 0.5) * 0.28 * (rnd() + 0.2);
        sp = (3 + rnd() * 10) * rayW[r];
        up = 0.4 + rnd() * 0.5;
      } else {
        // diffuse curtain + a low fast skirt
        a = rnd() * Math.PI * 2;
        const low = rnd() < 0.3;
        sp = low ? 6 + rnd() * 8 : 1.5 + rnd() * 6;
        up = low ? 0.12 + rnd() * 0.2 : 0.6 + rnd() * 0.8;
      }
      sp *= 0.75 + 0.5 * Math.sin(a * 5.3 + rays) ** 2; // lumpy azimuthal noise
      v.set(Math.cos(a) * Math.cos(up), Math.sin(up), Math.sin(a) * Math.cos(up)).multiplyScalar(sp).add(cv);
      p.copy(at).add(new THREE.Vector3(Math.cos(a), 0, Math.sin(a)).multiplyScalar(rnd() * 1.2));
      const g = 0.11 + rnd() * 0.14 + (rnd() < 0.08 ? 0.12 : 0); // some fresher, brighter grains
      const clump = rnd() < 0.05;
      this.emitLocal('dust', p, v, fx(3.5 + rnd() * 6, clump ? 0.18 + rnd() * 0.25 : 0.03 + rnd() * rnd() * 0.16, [g, g * 0.99, g * 0.96], 1, below + (rnd() - 0.5) * 0.4));
    }
  }

  update(dt: number) {
    for (const p of this.list) {
      const glow = p.glow;
      let lo = p.lo;
      let hi = p.hi;
      let k = 0;
      while (k < p.nAlive) {
        const i = p.alive[k];
        if (i < lo) lo = i;
        if (i > hi) hi = i;
        p.life[i] -= dt;
        if (p.life[i] <= 0) {
          // dead: hidden, out of the list (the last one takes its place)
          p.alphaAttr[i] = 0;
          const last = p.alive[--p.nAlive];
          p.alive[k] = last;
          p.slot[last] = k;
          p.slot[i] = -1;
          continue;
        }
        const j = i * 3;
        p.vel[j + 1] -= p.grav[i] * dt;
        p.pos[j] += p.vel[j] * dt;
        p.pos[j + 1] += p.vel[j + 1] * dt;
        p.pos[j + 2] += p.vel[j + 2] * dt;
        if (!glow && p.vel[j + 1] < 0) {
          if (p.pos[j + 1] < p.floor[i]) {
            // settle: regolith doesn't bounce
            p.pos[j + 1] = p.floor[i];
            p.vel[j] = p.vel[j + 1] = p.vel[j + 2] = 0;
            p.life[i] = Math.min(p.life[i], 0.6);
          }
        }
        const t = p.life[i] / p.maxLife[i];
        p.alphaAttr[i] = glow ? t : Math.min(1, t * 3) * 0.9;
        p.sizeAttr[i] = p.size[i] * (glow ? 0.6 + 0.4 * t : 1);
        k++;
      }
      // upload only the slots touched this frame
      if (hi >= lo) {
        const n = hi - lo + 1;
        const pos = p.geo.getAttribute('position') as THREE.BufferAttribute;
        const alpha = p.geo.getAttribute('alpha') as THREE.BufferAttribute;
        const size = p.geo.getAttribute('psize') as THREE.BufferAttribute;
        pos.addUpdateRange(lo * 3, n * 3);
        alpha.addUpdateRange(lo, n);
        size.addUpdateRange(lo, n);
        pos.needsUpdate = alpha.needsUpdate = size.needsUpdate = true;
      }
      if (p.cHi >= p.cLo) {
        const col = p.geo.getAttribute('pcolor') as THREE.BufferAttribute;
        col.addUpdateRange(p.cLo * 3, (p.cHi - p.cLo + 1) * 3);
        col.needsUpdate = true;
      }
      p.lo = p.cLo = p.count;
      p.hi = p.cHi = -1;
    }
  }
}
