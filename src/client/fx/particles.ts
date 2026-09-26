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

  /** Rocket impact: flash, sparks and a ballistic regolith curtain. */
  explosion(at: THREE.Vector3, floor: number, scale = 1) {
    const v = new THREE.Vector3();
    const p = new THREE.Vector3();
    for (let i = 0; i < 70 * scale; i++) {
      v.randomDirection().multiplyScalar(4 + Math.random() * 14);
      v.y = Math.abs(v.y) * 0.8 + 1;
      this.emit('glow', { pos: at, vel: v, color: [4, 2.2 + Math.random(), 0.7], life: 0.15 + Math.random() * 0.35, size: 0.12 + Math.random() * 0.2, gravity: 1.62 });
    }
    for (let i = 0; i < 6; i++) {
      this.emit('glow', { pos: at, vel: v.set(0, 0.5, 0), color: [7, 5, 3], life: 0.12 + i * 0.03, size: 2.5 + i * 0.8 });
    }
    for (let i = 0; i < 900 * scale; i++) {
      const a = Math.random() * Math.PI * 2;
      const up = 0.35 + Math.random() * 0.9;
      const sp = 2 + Math.random() * 11 * (Math.random() < 0.15 ? 1.6 : 1);
      v.set(Math.cos(a) * Math.cos(up), Math.sin(up), Math.sin(a) * Math.cos(up)).multiplyScalar(sp);
      p.copy(at).add(new THREE.Vector3(Math.cos(a), 0, Math.sin(a)).multiplyScalar(Math.random() * 1.4));
      const g = 0.16 + Math.random() * 0.1;
      this.emit('dust', { pos: p, vel: v, color: [g, g, g * 0.97], life: 4 + Math.random() * 5, size: 0.05 + Math.random() * 0.14, gravity: 1.62, floor });
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
