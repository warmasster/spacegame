import * as THREE from 'three';

/**
 * Minimal GPU-light particle pool: one Points draw call per blend mode, CPU simulation.
 * In vacuum there is no drag or smoke: dust flies on clean ballistic arcs and falls under
 * lunar gravity, fire is a brief flash. That alone reads as "Moon".
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
  dirty: boolean;
}

export interface Emit {
  pos: THREE.Vector3;
  vel: THREE.Vector3;
  color: [number, number, number];
  life: number;
  size: number;
  gravity?: number;
  /** Height where the particle settles (dust). */
  floor?: number;
}

export class Particles {
  readonly group = new THREE.Group();
  private pools: Record<'glow' | 'dust', Pool>;
  constructor(pixelRatio: number) {
    this.pools = {
      glow: this.makePool(1500, THREE.AdditiveBlending, pixelRatio, true),
      dust: this.makePool(5000, THREE.NormalBlending, pixelRatio, false),
    };
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
    const points = new THREE.Points(geo, mat);
    points.frustumCulled = false;
    points.renderOrder = glow ? 12 : 10;
    this.group.add(points);
    return {
      pos: posAttr, vel: new Float32Array(n * 3), col: colAttr, life: new Float32Array(n), maxLife: new Float32Array(n),
      size: new Float32Array(n), grav: new Float32Array(n), floor: new Float32Array(n), count: n, geo, alphaAttr, sizeAttr, colAttr, next: 0, dirty: false,
    };
  }

  emit(kind: 'glow' | 'dust', e: Emit) {
    const p = this.pools[kind];
    const i = p.next;
    p.next = (p.next + 1) % p.count;
    p.pos.set([e.pos.x, e.pos.y, e.pos.z], i * 3);
    p.vel.set([e.vel.x, e.vel.y, e.vel.z], i * 3);
    p.col.set(e.color, i * 3);
    p.life[i] = p.maxLife[i] = e.life;
    p.size[i] = e.size;
    p.grav[i] = e.gravity ?? 0;
    p.floor[i] = e.floor ?? -1e9;
  }

  /**
   * Rocket impact. Real lunar ejecta is not a smooth dome: it leaves in uneven rays and clumps,
   * so directions and speeds are drawn from a few random "jets" with noise, not uniformly.
   */
  explosion(at: THREE.Vector3, floor: number, scale = 1) {
    const v = new THREE.Vector3();
    const p = new THREE.Vector3();
    const rnd = Math.random;
    // flash core: several offset, uneven blobs instead of one perfect sphere
    for (let i = 0; i < 9; i++) {
      p.copy(at).add(v.randomDirection().multiplyScalar(rnd() * 0.9));
      p.y = Math.max(p.y, at.y + 0.1);
      this.emit('glow', { pos: p, vel: v.randomDirection().multiplyScalar(1 + rnd() * 2), color: [6, 3.5 + rnd() * 1.5, 1.5], life: 0.08 + rnd() * 0.18, size: 1.2 + rnd() * 2.4 });
    }
    // sparks, some fast streaks
    for (let i = 0; i < 90 * scale; i++) {
      v.randomDirection().multiplyScalar(3 + rnd() * rnd() * 22);
      v.y = Math.abs(v.y) * 0.9 + 0.5;
      this.emit('glow', { pos: at, vel: v, color: [4, 1.6 + rnd() * 1.4, 0.4], life: 0.1 + rnd() * rnd() * 0.9, size: 0.05 + rnd() * 0.12, gravity: 1.62 });
    }
    // glowing hot fragments that arc far and cool down
    for (let i = 0; i < 12 * scale; i++) {
      const a = rnd() * Math.PI * 2;
      v.set(Math.cos(a), 0.6 + rnd() * 1.2, Math.sin(a)).multiplyScalar(5 + rnd() * 9);
      this.emit('glow', { pos: at, vel: v, color: [2.2, 0.9, 0.3], life: 1.5 + rnd() * 2, size: 0.07, gravity: 1.62 });
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
      v.set(Math.cos(a) * Math.cos(up), Math.sin(up), Math.sin(a) * Math.cos(up)).multiplyScalar(sp);
      p.copy(at).add(new THREE.Vector3(Math.cos(a), 0, Math.sin(a)).multiplyScalar(rnd() * 1.2));
      const g = 0.11 + rnd() * 0.14 + (rnd() < 0.08 ? 0.12 : 0); // some fresher, brighter grains
      const clump = rnd() < 0.05;
      this.emit('dust', {
        pos: p,
        vel: v,
        color: [g, g * 0.99, g * 0.96],
        life: 3.5 + rnd() * 6,
        size: clump ? 0.18 + rnd() * 0.25 : 0.03 + rnd() * rnd() * 0.16,
        gravity: 1.62,
        floor: floor + (rnd() - 0.5) * 0.4,
      });
    }
  }

  update(dt: number) {
    for (const key of ['glow', 'dust'] as const) {
      const p = this.pools[key];
      let any = false;
      for (let i = 0; i < p.count; i++) {
        if (p.life[i] <= 0) {
          p.alphaAttr[i] = 0;
          continue;
        }
        any = true;
        p.life[i] -= dt;
        const j = i * 3;
        p.vel[j + 1] -= p.grav[i] * dt;
        p.pos[j] += p.vel[j] * dt;
        p.pos[j + 1] += p.vel[j + 1] * dt;
        p.pos[j + 2] += p.vel[j + 2] * dt;
        if (key === 'dust' && p.vel[j + 1] < 0) {
          if (p.pos[j + 1] < p.floor[i]) {
            // settle: regolith doesn't bounce
            p.pos[j + 1] = p.floor[i];
            p.vel[j] = p.vel[j + 1] = p.vel[j + 2] = 0;
            p.life[i] = Math.min(p.life[i], 0.6);
          }
        }
        const t = p.life[i] / p.maxLife[i];
        p.alphaAttr[i] = key === 'glow' ? t : Math.min(1, t * 3) * 0.9;
        p.sizeAttr[i] = p.size[i] * (key === 'glow' ? 0.6 + 0.4 * t : 1);
      }
      if (any || p.dirty) {
        p.dirty = any;
        for (const name of ['position', 'alpha', 'psize', 'pcolor']) (p.geo.getAttribute(name) as THREE.BufferAttribute).needsUpdate = true;
      }
    }
  }
}
