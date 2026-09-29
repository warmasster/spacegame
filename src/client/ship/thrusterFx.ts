import * as THREE from 'three';
import { engineGeometry, ThrusterSet, type Thruster } from '../../shared/ship/flight';
import type { V3 } from '../../shared/ship/geom';
import type { ShipSim } from '../../shared/ship/sim';
import type { ShipAnimState } from './screens';

const _y = new THREE.Vector3(0, 1, 0);
const _d = new THREE.Vector3();

/**
 * An exhaust plume: an open flared tube from the nozzle (radius 1 at y = 0) spreading to `SPREAD`
 * at y = 1 (scaled to length / nozzle radius), fading along its length and toward its silhouette
 * so it reads as glowing gas, not a solid cone.
 */
const SPREAD = 2.2;

function plumeMaterial(color: THREE.Color) {
  return new THREE.ShaderMaterial({
    uniforms: { uColor: { value: color }, uPower: { value: 0 } },
    // every step guarded: on Windows the shader runs through Direct3D, where pow(0, y), a
    // normalize() of a zero vector or an overflow give NaN, and one NaN pixel blacks out whole
    // blocks of the screen through the bloom
    vertexShader: /* glsl */ `
      #include <common>
      #include <logdepthbuf_pars_vertex>
      varying float vT;
      varying float vFace;
      void main() {
        vT = position.y;
        vec4 mv = modelViewMatrix * vec4(position, 1.0);
        vec3 n = normalMatrix * normal;
        float ln = length(n);
        float lv = length(mv.xyz);
        vFace = (ln > 1e-6 && lv > 1e-6) ? clamp(abs(dot(n / ln, -mv.xyz / lv)), 0.0, 1.0) : 0.0;
        gl_Position = projectionMatrix * mv;
        #include <logdepthbuf_vertex>
      }`,
    fragmentShader: /* glsl */ `
      #include <common>
      #include <logdepthbuf_pars_fragment>
      uniform vec3 uColor;
      uniform float uPower;
      varying float vT;
      varying float vFace;
      void main() {
        #include <logdepthbuf_fragment>
        float t = clamp(vT, 0.0, 1.0);
        float f = clamp(vFace, 0.0, 1.0);
        // f^1.5 without pow()
        float a = clamp(exp(-3.2 * t) * (1.0 - t) * f * sqrt(f) * clamp(uPower, 0.0, 1.0), 0.0, 1.0);
        gl_FragColor = vec4(uColor * a, a);
      }`,
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
    side: THREE.DoubleSide,
    toneMapped: false,
  });
}

interface Plume {
  t: Thruster;
  /** State index of its output (-1: none). */
  out: number;
  outer: THREE.Mesh;
  inner: THREE.Mesh;
  mats: THREE.ShaderMaterial[];
  radius: number;
  /** Plume length at full output (m). */
  reach: number;
}

/**
 * Thruster exhausts drawn from what every client has: each thruster's output in the state table
 * (`<engine>.thr`, `<pad>.use`, `<rcs>.use`, written by whoever flies the ship) and, for tilting
 * nacelles, the nozzle and direction at the animated travel. Lives in ship space (child of the
 * ship's root).
 */
export class ThrusterFx {
  readonly group = new THREE.Group();
  private plumes: Plume[] = [];
  private set: ThrusterSet;
  private time = 0;

  constructor(private sim: ShipSim) {
    this.set = new ThrusterSet(sim.def);
    const cone = new THREE.CylinderGeometry(SPREAD, 1, 1, 20, 1, true).translate(0, 0.5, 0);
    for (const t of this.set.list) {
      const hot = t.kind === 'main' ? new THREE.Color(1.3, 0.95, 2.4) : t.kind === 'lift' ? new THREE.Color(0.7, 0.68, 1.5) : new THREE.Color(1.4, 1.45, 1.6);
      const core = t.kind === 'lift' ? new THREE.Color(1.4, 1.3, 2.0) : new THREE.Color(2.4, 2.2, 3.0);
      const mats = [plumeMaterial(hot), plumeMaterial(core)];
      const outer = new THREE.Mesh(cone, mats[0]);
      const inner = new THREE.Mesh(cone, mats[1]);
      for (const m of [outer, inner]) {
        // the plume is its scaled cone (the shader moves no vertex): its bounds hold for culling
        m.renderOrder = 6;
        m.visible = false;
        this.group.add(m);
      }
      const p = t.part;
      const radius = t.kind === 'main' ? Math.min(p.half[0], p.half[1]) * 0.6 : t.kind === 'lift' ? p.half[0] * 0.55 : 0.05;
      const reach = t.kind === 'main' ? 6 : t.kind === 'lift' ? 2.2 : 0.55;
      this.plumes.push({ t, out: sim.vars.has(t.outVar) ? sim.vars.idx(t.outVar) : -1, outer, inner, mats, radius, reach });
    }
  }

  private shape(m: THREE.Mesh, at: V3, k: number, r: number, radius: number, len: number, out: number) {
    m.position.set(at[0], at[1], at[2]);
    m.quaternion.setFromUnitVectors(_y, _d);
    m.scale.set(radius * r * (0.9 + 0.2 * out), len * k, radius * r * (0.9 + 0.2 * out));
  }

  update(dt: number, anim: ShipAnimState) {
    this.time += dt;
    const sim = this.sim;
    for (const pl of this.plumes) {
      const t = pl.t;
      const raw = pl.out >= 0 ? sim.st[pl.out] : 0;
      const out = Math.max(0, Math.min(1, t.kind === 'rcs' ? raw * 0.8 : raw));
      const on = out > 0.015;
      if (pl.outer.visible !== on) pl.outer.visible = pl.inner.visible = on;
      if (!on) continue;
      let at: V3 = t.at;
      let dir: V3 = t.dir;
      if (t.kind === 'main' && t.part.gimbal) {
        const key = t.part.gimbal.key;
        const g = engineGeometry(t.part, anim.movers[key] ?? sim.sw[key] ?? 0);
        at = g.at;
        dir = g.dir;
      }
      // RCS: a puff out of the block, away from the hull's middle (the wire carries only how much)
      if (t.kind === 'rcs') _d.set(Math.sign(t.at[0]) || 1, 0.35, 0);
      else _d.set(-dir[0], -dir[1], -dir[2]);
      const flicker = 0.88 + 0.12 * Math.sin(this.time * 61 + t.at[0] * 7) * Math.sin(this.time * 37 + t.at[2] * 5);
      const len = pl.reach * (0.25 + 0.75 * Math.sqrt(out)) * flicker;
      _d.normalize();
      this.shape(pl.outer, at, 1, 1, pl.radius, len, out);
      this.shape(pl.inner, at, 0.45, 0.45, pl.radius, len, out);
      pl.mats[0].uniforms.uPower.value = 0.35 + 0.65 * out;
      pl.mats[1].uniforms.uPower.value = 0.6 * out;
    }
  }

  /** Lift pads blowing now: nozzle (ship space) and output, for the dust they raise. */
  *blowing(): Generator<{ at: V3; out: number }> {
    for (const pl of this.plumes) {
      if (pl.t.kind !== 'lift' || !pl.outer.visible) continue;
      const v = this.sim.st[this.sim.vars.idx(pl.t.outVar)];
      yield { at: pl.t.at, out: v };
    }
  }
}
