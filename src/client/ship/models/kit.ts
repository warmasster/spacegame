import * as THREE from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import type { V2, V3 } from '../../../shared/ship/geom';

/**
 * Material slots a model paints with. `body`, `trim` and `accent` are the machine's own (cloned
 * per part so the welder can tint it, accent = the maker's colour); the rest are the ship's shared
 * materials (see ShipView.mats).
 */
export type Slot = 'body' | 'trim' | 'accent' | 'dark' | 'chrome' | 'pipe' | 'cells' | 'glass' | 'seat' | 'console' | 'paint' | 'paintDark' | 'lining' | 'fabric';

type Axis = 'x' | 'y' | 'z';

/** Rotation taking a y-aligned primitive onto `axis`. */
const ALONG: Record<Axis, THREE.Quaternion> = {
  x: new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 0, 1), -Math.PI / 2),
  y: new THREE.Quaternion(),
  z: new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), Math.PI / 2),
};

/** Position + normal only, non-indexed (so heterogeneous pieces merge into one mesh per slot). */
function strip(g: THREE.BufferGeometry) {
  const n = g.index ? g.toNonIndexed() : g;
  for (const name of Object.keys(n.attributes)) if (name !== 'position' && name !== 'normal') n.deleteAttribute(name);
  if (!n.getAttribute('normal')) n.computeVertexNormals();
  return n;
}

/**
 * Geometry collector for one model, in metres around the box centre (x right, y up, z aft of the
 * part's own frame). Pieces are merged per material slot; `child` opens a separately moving node
 * (a hinged panel, a folding wing) that an animation can turn.
 */
export class Kit {
  private lists = new Map<Slot, THREE.BufferGeometry[]>();
  private xf = new THREE.Matrix4();
  readonly children = new Map<string, { kit: Kit; pivot: V3 }>();

  /** Everything added from now on is transformed by `m` (composed with what was set before). */
  push(m: THREE.Matrix4) {
    const prev = this.xf.clone();
    this.xf.multiply(m);
    return () => this.xf.copy(prev);
  }

  add(slot: Slot, g: THREE.BufferGeometry, m?: THREE.Matrix4) {
    if (m) g.applyMatrix4(m);
    g.applyMatrix4(this.xf);
    let list = this.lists.get(slot);
    if (!list) this.lists.set(slot, (list = []));
    list.push(strip(g));
    return this;
  }

  box(slot: Slot, sx: number, sy: number, sz: number, x = 0, y = 0, z = 0, rot?: THREE.Euler) {
    const m = new THREE.Matrix4().compose(new THREE.Vector3(x, y, z), new THREE.Quaternion().setFromEuler(rot ?? new THREE.Euler()), new THREE.Vector3(1, 1, 1));
    return this.add(slot, new THREE.BoxGeometry(Math.max(1e-3, sx), Math.max(1e-3, sy), Math.max(1e-3, sz)), m);
  }

  /** Cylinder (or cone: r2 at the + end) along an axis. */
  cyl(slot: Slot, axis: Axis, r: number, len: number, x = 0, y = 0, z = 0, seg = 20, r2 = r, open = false) {
    const m = new THREE.Matrix4().compose(new THREE.Vector3(x, y, z), ALONG[axis], new THREE.Vector3(1, 1, 1));
    // three's cylinder: radiusTop at +y
    return this.add(slot, new THREE.CylinderGeometry(r2, r, Math.max(1e-3, len), seg, 1, open), m);
  }

  torus(slot: Slot, axis: Axis, R: number, tube: number, x = 0, y = 0, z = 0, seg = 24) {
    // torus lies in xy with its axis on z: bring the axis onto `axis`
    const q = axis === 'z' ? new THREE.Quaternion() : axis === 'y' ? new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), Math.PI / 2) : new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), Math.PI / 2);
    const m = new THREE.Matrix4().compose(new THREE.Vector3(x, y, z), q, new THREE.Vector3(1, 1, 1));
    return this.add(slot, new THREE.TorusGeometry(R, tube, 6, seg), m);
  }

  sphere(slot: Slot, r: number, x = 0, y = 0, z = 0, scale: V3 = [1, 1, 1], seg = 16, half = false) {
    const g = new THREE.SphereGeometry(r, seg, Math.max(6, seg >> 1), 0, Math.PI * 2, 0, half ? Math.PI / 2 : Math.PI);
    return this.add(slot, g, new THREE.Matrix4().compose(new THREE.Vector3(x, y, z), new THREE.Quaternion(), new THREE.Vector3(...scale)));
  }

  /** Cylinder between two points. */
  rod(slot: Slot, a: V3, b: V3, r: number, seg = 8) {
    const A = new THREE.Vector3(...a);
    const B = new THREE.Vector3(...b);
    const L = A.distanceTo(B);
    if (L < 1e-4) return this;
    const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 1, 0), B.clone().sub(A).normalize());
    const mid = A.add(B).multiplyScalar(0.5);
    return this.add(slot, new THREE.CylinderGeometry(r, r, L, seg), new THREE.Matrix4().compose(mid, q, new THREE.Vector3(1, 1, 1)));
  }

  /** Bolt heads on a circle of radius R around `axis` through (x, y, z). */
  bolts(slot: Slot, axis: Axis, R: number, n: number, x = 0, y = 0, z = 0, size = 0.012) {
    for (let i = 0; i < n; i++) {
      const a = (i / n) * Math.PI * 2;
      const u = Math.cos(a) * R;
      const v = Math.sin(a) * R;
      const p: V3 = axis === 'y' ? [x + u, y, z + v] : axis === 'x' ? [x, y + u, z + v] : [x + u, y + v, z];
      this.cyl(slot, axis, size, size * 0.9, p[0], p[1], p[2], 6);
    }
    return this;
  }

  /** Ventilation slats on a face: `n` bars across (u) stacked along v, the face normal is `axis`. */
  grille(slot: Slot, axis: Axis, w: number, h: number, n: number, x = 0, y = 0, z = 0, depth = 0.012) {
    for (let i = 0; i < n; i++) {
      const t = n === 1 ? 0 : (i / (n - 1) - 0.5) * h * 0.9;
      if (axis === 'x') this.box(slot, depth, h / (n * 2.2), w, x, y + t, z);
      else if (axis === 'z') this.box(slot, w, h / (n * 2.2), depth, x, y + t, z);
      else this.box(slot, w, depth, h / (n * 2.2), x, y, z + t);
    }
    return this;
  }

  /** Extrude a (z, y) profile across x ∈ [x0, x1]. */
  extrudeX(slot: Slot, zy: V2[], x0: number, x1: number, bevel = 0) {
    const shape = new THREE.Shape(zy.map(([z, y]) => new THREE.Vector2(z, y)));
    const g = new THREE.ExtrudeGeometry(shape, { depth: Math.max(1e-3, x1 - x0 - bevel * 2), bevelEnabled: bevel > 0, bevelSize: bevel, bevelThickness: bevel, bevelSegments: 1 });
    // shape X → z, shape Y → y, extrusion Z → −x
    return this.add(slot, g, new THREE.Matrix4().set(0, 0, -1, x1 - bevel, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1));
  }

  /** Lathe an (r, y) profile around y. */
  lathe(slot: Slot, ry: V2[], x = 0, y = 0, z = 0, axis: Axis = 'y', seg = 24) {
    const m = new THREE.Matrix4().compose(new THREE.Vector3(x, y, z), ALONG[axis], new THREE.Vector3(1, 1, 1));
    return this.add(slot, new THREE.LatheGeometry(ry.map(([r, yy]) => new THREE.Vector2(r, yy)), seg), m);
  }

  /** A separately moving piece: its geometry is built around `pivot` (in this kit's frame). */
  child(id: string, pivot: V3): Kit {
    let c = this.children.get(id);
    if (!c) this.children.set(id, (c = { kit: new Kit(), pivot }));
    return c.kit;
  }

  build(): Map<Slot, THREE.BufferGeometry> {
    const out = new Map<Slot, THREE.BufferGeometry>();
    for (const [k, list] of this.lists) {
      const g = mergeGeometries(list);
      if (g) {
        g.computeBoundingSphere();
        out.set(k, g);
      }
    }
    return out;
  }
}

/** What a builder gets besides its box half extents. */
export interface ModelInfo {
  look: Record<string, number>;
  p: Record<string, number>;
  /** Side of the ship the part is on (−1 port, +1 starboard, 0 centre). */
  side: number;
  size?: string;
}

/** Animation of a model's moving children by a 0..1 value (deployment). */
export type ModelAnim = (nodes: Record<string, THREE.Object3D>, t: number) => void;

export type ModelBuilder = (k: Kit, h: V3, info: ModelInfo) => ModelAnim | void;
