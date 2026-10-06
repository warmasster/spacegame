// Shadow casters that cast into nothing we see (docs/RENDIMIENTO.md). The sun's cascades are fitted
// round the near part of the view whatever it looks at, so everything round the player that casts
// a shadow is drawn into them again every frame — looking at the sky too, where no shadow lands
// on anything in view. A caster matters only if its shadow can: its bounding sphere swept along the
// light for `reach` metres must cross the view. Those that don't are left out of this frame's shadow
// pass (their `castShadow` is off during the render and back on after it).
//
// With several cascades (useCascades), the same test per cascade: each one covers a slice of the
// view (near to far), and a caster is drawn only into the cascades whose slice its shadow can reach
// (by layers: cascade i's shadow camera sees layer CASCADE_LAYER + i). Without it, three draws every
// caster near the player into every cascade: three times, for a shadow that lands in one.
//
// Pure (three.js maths only): tests build a scene and a camera and ask it.

import * as THREE from 'three';

/** Cascade i's shadow camera sees layer CASCADE_LAYER + i (and only that). */
export const CASCADE_LAYER = 20;
const MAX_CASCADES = 4;
const ALL_CASCADES = ((1 << MAX_CASCADES) - 1) << CASCADE_LAYER;
/** A cascade's slice of the view, widened this much of its depth each way (the cascades fade into each other). */
const SLICE_MARGIN = 0.15;

/** What the per-cascade test needs from three's CSM. */
export interface Cascades {
  lights: THREE.DirectionalLight[];
  breaks: number[];
  maxFar: number;
}

const _sphere = new THREE.Sphere();
const _slice = new THREE.PerspectiveCamera();
const _end = new THREE.Vector3();
const _m = new THREE.Matrix4();

/** Does a sphere swept from its centre along `dir` for `reach` m touch the frustum? (conservative) */
export function sweptSphereInFrustum(frustum: THREE.Frustum, s: THREE.Sphere, dir: THREE.Vector3, reach: number): boolean {
  _end.copy(s.center).addScaledVector(dir, reach);
  for (const p of frustum.planes) {
    // outside when both ends are wholly behind the same plane
    if (p.distanceToPoint(s.center) < -s.radius && p.distanceToPoint(_end) < -s.radius) return false;
  }
  return true;
}

export class ShadowCull {
  /** Casters left out this frame (turned back on by `after`). */
  private readonly off: THREE.Object3D[] = [];
  private readonly frustum = new THREE.Frustum();
  private readonly slices: THREE.Frustum[] = [];
  private cascades: Cascades | null = null;
  /** Casters looked at and left out in the last frame (diagnostics, F3). */
  seen = 0;
  culled = 0;
  /** Shadow draws left (casters × the cascades each is drawn into). */
  drawn = 0;

  constructor(
    private readonly scene: THREE.Object3D,
    private readonly camera: THREE.Camera,
    /** Which way the light travels (unit, world axes: from the sun toward the ground). */
    private readonly lightDir: () => THREE.Vector3,
    /** How far a shadow reaches from its caster (m). */
    private readonly reach = 300,
  ) {}

  /** Draw each caster only into the cascades it shadows (a CSM with 2+ cascades; null: into all, as three does). */
  useCascades(c: Cascades | null): void {
    this.cascades = c && c.lights.length > 1 ? c : null;
    if (!c) return;
    c.lights.forEach((l, i) => (this.cascades ? l.shadow.camera.layers.set(CASCADE_LAYER + i) : l.shadow.camera.layers.enableAll()));
  }

  /** Right before the frame is rendered. */
  before(): void {
    const cam = this.camera;
    cam.updateMatrixWorld();
    _m.multiplyMatrices(cam.projectionMatrix, cam.matrixWorldInverse);
    this.frustum.setFromProjectionMatrix(_m);
    const dir = this.lightDir();
    const n = this.cutSlices();
    this.seen = 0;
    this.drawn = 0;
    this.scene.traverseVisible((o) => {
      const m = o as THREE.Mesh;
      if (!m.castShadow) return;
      // instanced meshes with a sphere round their instances (culled as a whole) are tested like any
      // mesh; the rest (never culled, batched) go into every cascade
      const inst = m as unknown as THREE.InstancedMesh;
      if (!m.isMesh || (m as unknown as THREE.BatchedMesh).isBatchedMesh || (inst.isInstancedMesh && (!m.frustumCulled || !inst.boundingSphere))) {
        if (n) m.layers.mask |= ALL_CASCADES;
        return;
      }
      if (!m.geometry.boundingSphere) m.geometry.computeBoundingSphere();
      const bs = inst.isInstancedMesh ? inst.boundingSphere : (m as unknown as THREE.SkinnedMesh).isSkinnedMesh ? ((m as unknown as THREE.SkinnedMesh).boundingSphere ?? m.geometry.boundingSphere) : m.geometry.boundingSphere;
      if (!bs) return;
      this.seen++;
      _sphere.copy(bs).applyMatrix4(m.matrixWorld);
      if (sweptSphereInFrustum(this.frustum, _sphere, dir, this.reach)) {
        if (!n) return void this.drawn++;
        // the cascades whose slice its shadow reaches: about as far as it stands tall, and more for a low sun
        const reach = Math.min(this.reach, 4 * _sphere.radius + 20);
        let mask = 0;
        for (let i = 0; i < n; i++) if (sweptSphereInFrustum(this.slices[i], _sphere, dir, reach)) mask |= 1 << (CASCADE_LAYER + i);
        m.layers.mask = (m.layers.mask & ~ALL_CASCADES) | mask;
        if (mask) {
          for (let i = 0; i < n; i++) if (mask & (1 << (CASCADE_LAYER + i))) this.drawn++;
          return;
        }
      }
      m.castShadow = false;
      this.off.push(m);
    });
    this.culled = this.off.length;
  }

  /** Each cascade's slice of the view as a frustum (the CSM's breaks); 0 when not per cascade. */
  private cutSlices(): number {
    const c = this.cascades;
    const cam = this.camera as THREE.PerspectiveCamera;
    if (!c || !cam.isPerspectiveCamera) return 0;
    const n = Math.min(c.lights.length, c.breaks.length, MAX_CASCADES);
    const far = Math.min(cam.far, c.maxFar);
    _slice.copy(cam, false);
    for (let i = 0; i < n; i++) {
      const a = i === 0 ? cam.near : c.breaks[i - 1] * far;
      const b = c.breaks[i] * far;
      const pad = (b - a) * SLICE_MARGIN;
      _slice.near = Math.max(cam.near, a - pad);
      _slice.far = b + pad;
      _slice.updateProjectionMatrix();
      _m.multiplyMatrices(_slice.projectionMatrix, cam.matrixWorldInverse);
      (this.slices[i] ??= new THREE.Frustum()).setFromProjectionMatrix(_m);
    }
    return n;
  }

  /** Right after: every caster back as it was. */
  after(): void {
    for (const o of this.off) o.castShadow = true;
    this.off.length = 0;
  }
}
