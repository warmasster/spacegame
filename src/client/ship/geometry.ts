import * as THREE from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import { PANEL_GAP, type PanelDef, type ShipDef } from '../../shared/ship/def';
import { insetPoly, type V2, type V3 } from '../../shared/ship/geom';

/** Which merged mesh each face of a panel goes to. */
export type PanelMatKey = 'hull' | 'lining' | 'deck' | 'under' | 'glass' | 'edge';

const FACES: Record<PanelDef['kind'], [PanelMatKey, PanelMatKey]> = {
  hull: ['hull', 'lining'],
  glass: ['glass', 'glass'],
  floor: ['deck', 'under'],
  bulkhead: ['lining', 'lining'],
};

class Buf {
  pos: number[] = [];
  nrm: number[] = [];
  uv: number[] = [];
  panel: number[] = [];
  heat: number[] = [];
  tan: number[] = [];
  bit: number[] = [];
  get count() {
    return this.pos.length / 3;
  }
  vert(p: V3, n: V3, uv: V2, P: [number, number, number, number], u: V3, v: V3) {
    this.pos.push(p[0], p[1], p[2]);
    this.nrm.push(n[0], n[1], n[2]);
    this.uv.push(uv[0], uv[1]);
    this.panel.push(P[0], P[1], P[2], P[3]);
    this.heat.push(0);
    this.tan.push(u[0], u[1], u[2]);
    this.bit.push(v[0], v[1], v[2]);
  }
  geometry() {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.Float32BufferAttribute(this.pos, 3));
    g.setAttribute('normal', new THREE.Float32BufferAttribute(this.nrm, 3));
    g.setAttribute('aUv', new THREE.Float32BufferAttribute(this.uv, 2));
    g.setAttribute('aPanel', new THREE.Float32BufferAttribute(this.panel, 4).setUsage(THREE.DynamicDrawUsage));
    g.setAttribute('aHeat', new THREE.Float32BufferAttribute(this.heat, 1).setUsage(THREE.DynamicDrawUsage));
    g.setAttribute('aTan', new THREE.Float32BufferAttribute(this.tan, 3));
    g.setAttribute('aBit', new THREE.Float32BufferAttribute(this.bit, 3));
    g.computeBoundingSphere();
    return g;
  }
}

export interface PanelRange {
  key: PanelMatKey;
  start: number;
  count: number;
}

const at = (p: PanelDef, x: number, y: number, z: number): V3 => [
  p.c[0] + p.u[0] * x + p.v[0] * y + p.n[0] * z,
  p.c[1] + p.u[1] * x + p.v[1] * y + p.n[1] * z,
  p.c[2] + p.u[2] * x + p.v[2] * y + p.n[2] * z,
];

export function panelSeed(i: number) {
  const s = Math.sin(i * 127.1 + 311.7) * 43758.5453;
  return (s - Math.floor(s)) * 0.998;
}

/**
 * Livery: light upper hull with an orange waterline, charcoal lower band and nose, hazard
 * stripes framing the ramp. Returns a PAINT index (see materials.ts).
 */
export function panelPaint(p: PanelDef) {
  if (p.kind !== 'hull') return 0;
  const [, row] = p.id.split('-');
  if (/^(L|R)1$/.test(row) || p.id.startsWith('CK-N')) return 1;
  if (/^(L|R)2$/.test(row)) return 3;
  if (p.id.startsWith('CG-AFT') && p.c[1] < 1.6) return 2;
  return 0;
}

/** Merged geometry per material of every solid panel, plus each panel's vertex ranges. */
export function buildPanels(def: ShipDef, solid: (i: number) => boolean, damage: (i: number) => number) {
  const bufs = new Map<PanelMatKey, Buf>();
  const ranges: PanelRange[][] = def.panels.map(() => []);
  const buf = (k: PanelMatKey) => {
    let b = bufs.get(k);
    if (!b) bufs.set(k, (b = new Buf()));
    return b;
  };
  for (const p of def.panels) {
    if (!solid(p.index)) continue;
    const xs = p.poly.map((q) => q[0]);
    const ys = p.poly.map((q) => q[1]);
    const minU = Math.min(...xs);
    const minV = Math.min(...ys);
    const P: [number, number, number, number] = [Math.max(...xs) - minU, Math.max(...ys) - minV, damage(p.index), panelPaint(p) + panelSeed(p.index)];
    const h = p.t / 2;
    const [outerKey, innerKey] = FACES[p.kind];
    const face = (key: PanelMatKey, side: 1 | -1) => {
      const b = buf(key);
      const start = b.count;
      const n: V3 = side > 0 ? p.n : [-p.n[0], -p.n[1], -p.n[2]];
      const poly = side > 0 ? p.poly : [...p.poly].reverse();
      for (let k = 1; k < poly.length - 1; k++) {
        for (const q of [poly[0], poly[k], poly[k + 1]]) b.vert(at(p, q[0], q[1], h * side), n, [q[0] - minU, q[1] - minV], P, p.u, p.v);
      }
      ranges[p.index].push({ key, start, count: b.count - start });
    };
    face(outerKey, 1);
    face(innerKey, -1);
    // edges: dark folded rim
    const eb = buf('edge');
    const start = eb.count;
    for (let k = 0; k < p.poly.length; k++) {
      const a = p.poly[k];
      const c = p.poly[(k + 1) % p.poly.length];
      const dx = c[0] - a[0];
      const dy = c[1] - a[1];
      const l = Math.hypot(dx, dy) || 1;
      const n: V3 = [(p.u[0] * dy - p.v[0] * dx) / l, (p.u[1] * dy - p.v[1] * dx) / l, (p.u[2] * dy - p.v[2] * dx) / l];
      const A = at(p, a[0], a[1], h);
      const B = at(p, c[0], c[1], h);
      const C = at(p, c[0], c[1], -h);
      const D = at(p, a[0], a[1], -h);
      for (const q of [A, D, C, A, C, B]) eb.vert(q, n, [0, 0], P, p.u, p.v);
    }
    ranges[p.index].push({ key: 'edge', start, count: eb.count - start });
  }
  const geos = new Map<PanelMatKey, THREE.BufferGeometry>();
  for (const [k, b] of bufs) geos.set(k, b.geometry());
  return { geos, ranges };
}

/**
 * Structure the panels bolt onto: a beam along every panel edge (deduplicated), so seams read as
 * frames from inside and out, and a blown-out panel leaves its ribs standing.
 */
export function buildFrames(def: ShipDef) {
  const seen = new Set<string>();
  const parts: THREE.BufferGeometry[] = [];
  const key = (a: V3) => a.map((v) => Math.round(v * 50)).join(',');
  const m = new THREE.Matrix4();
  const q = new THREE.Quaternion();
  for (const p of def.panels) {
    const outline = insetPoly(p.poly, -PANEL_GAP / 2);
    for (let k = 0; k < outline.length; k++) {
      const a = at(p, outline[k][0], outline[k][1], 0);
      const b = at(p, outline[(k + 1) % outline.length][0], outline[(k + 1) % outline.length][1], 0);
      const id = [key(a), key(b)].sort().join('|');
      if (seen.has(id)) continue;
      seen.add(id);
      const A = new THREE.Vector3(...a);
      const B = new THREE.Vector3(...b);
      const L = A.distanceTo(B);
      if (L < 0.05) continue;
      const w = p.kind === 'glass' ? 0.085 : p.kind === 'floor' ? 0.05 : 0.065;
      const d = p.kind === 'floor' ? p.t + 0.02 : p.t + 0.035;
      const g = new THREE.BoxGeometry(w, d, L + w * 0.9);
      // box z along the edge, y along the panel normal
      const zAxis = B.clone().sub(A).normalize();
      const yAxis = new THREE.Vector3(...p.n);
      const xAxis = new THREE.Vector3().crossVectors(yAxis, zAxis).normalize();
      m.makeBasis(xAxis, yAxis, zAxis);
      q.setFromRotationMatrix(m);
      const mid = A.add(B).multiplyScalar(0.5);
      if (p.kind === 'floor') mid.addScaledVector(yAxis, 0.012);
      g.applyMatrix4(new THREE.Matrix4().compose(mid, q, new THREE.Vector3(1, 1, 1)));
      parts.push(strip(g));
    }
  }
  return mergeGeometries(parts)!;
}

/** Position + normal only, non-indexed (so heterogeneous parts merge). */
/**
 * Indexed copy of a non-indexed geometry: vertices equal in every attribute (to 1e-4) are shared,
 * so the GPU shades each once (its post-transform cache). Hard edges stay hard: their normals
 * differ. Groups and the draw range carry over (they count indices, which here are the old
 * vertices one to one). Numeric hashing: three's mergeVertices builds a string per vertex, far too
 * slow for a ship's quarter of a million.
 */
export function indexGeometry(g: THREE.BufferGeometry): THREE.BufferGeometry {
  if (g.index) return g;
  const names = Object.keys(g.attributes);
  const attrs = names.map((n) => g.getAttribute(n) as THREE.BufferAttribute);
  const count = attrs[0]?.count ?? 0;
  if (!count) return g;
  const q = (v: number) => Math.round(v * 1e4);
  const same = (a: number, b: number) => {
    for (const at of attrs) for (let c = 0; c < at.itemSize; c++) if (q(at.array[a * at.itemSize + c]) !== q(at.array[b * at.itemSize + c])) return false;
    return true;
  };
  const heads = new Map<number, number>();
  const next = new Int32Array(count).fill(-1);
  const first = new Int32Array(count);
  const index = new Uint32Array(count);
  let unique = 0;
  for (let v = 0; v < count; v++) {
    let h = 0x811c9dc5;
    for (const at of attrs) for (let c = 0; c < at.itemSize; c++) h = Math.imul(h ^ q(at.array[v * at.itemSize + c]), 16777619);
    const head = heads.get(h) ?? -1;
    let u = head;
    while (u >= 0 && !same(first[u], v)) u = next[u];
    if (u < 0) {
      u = unique++;
      first[u] = v;
      next[u] = head;
      heads.set(h, u);
    }
    index[v] = u;
  }
  if (unique > count * 0.9) return g; // nothing to share: keep it as it is
  const out = new THREE.BufferGeometry();
  names.forEach((name, k) => {
    const at = attrs[k];
    const size = at.itemSize;
    const arr = new (at.array.constructor as Float32ArrayConstructor)(unique * size);
    for (let u = 0; u < unique; u++) for (let c = 0; c < size; c++) arr[u * size + c] = at.array[first[u] * size + c];
    out.setAttribute(name, new THREE.BufferAttribute(arr, size, at.normalized));
  });
  out.setIndex(new THREE.BufferAttribute(unique < 65536 ? Uint16Array.from(index) : index, 1));
  for (const gr of g.groups) out.addGroup(gr.start, gr.count, gr.materialIndex);
  out.setDrawRange(g.drawRange.start, g.drawRange.count);
  out.boundingBox = g.boundingBox?.clone() ?? null;
  out.boundingSphere = g.boundingSphere?.clone() ?? null;
  g.dispose();
  return out;
}

export function strip(g: THREE.BufferGeometry) {
  const n = g.index ? g.toNonIndexed() : g;
  for (const name of Object.keys(n.attributes)) if (name !== 'position' && name !== 'normal') n.deleteAttribute(name);
  if (!n.getAttribute('normal')) n.computeVertexNormals();
  return n;
}

/** Collects decor geometry per material and merges it (one draw call per material). */
export class Parts {
  private lists = new Map<string, THREE.BufferGeometry[]>();
  private m = new THREE.Matrix4();
  private q = new THREE.Quaternion();
  private s = new THREE.Vector3(1, 1, 1);

  add(mat: string, g: THREE.BufferGeometry, pos: V3 = [0, 0, 0], rot?: THREE.Quaternion | THREE.Euler, keepUv = false) {
    const q = rot instanceof THREE.Euler ? this.q.setFromEuler(rot) : rot ?? this.q.identity();
    g.applyMatrix4(this.m.compose(new THREE.Vector3(...pos), q, this.s));
    let list = this.lists.get(mat);
    if (!list) this.lists.set(mat, (list = []));
    list.push(keepUv ? (g.index ? g.toNonIndexed() : g) : strip(g));
    return g;
  }

  box(mat: string, w: number, h: number, d: number, pos: V3, rot?: THREE.Quaternion | THREE.Euler) {
    return this.add(mat, new THREE.BoxGeometry(w, h, d), pos, rot);
  }

  /** Cylinder between two points. */
  rod(mat: string, a: V3, b: V3, r: number, seg = 12, r2 = r) {
    const A = new THREE.Vector3(...a);
    const B = new THREE.Vector3(...b);
    const g = new THREE.CylinderGeometry(r2, r, A.distanceTo(B), seg);
    const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 1, 0), B.clone().sub(A).normalize());
    const mid = A.add(B).multiplyScalar(0.5);
    return this.add(mat, g, [mid.x, mid.y, mid.z], q);
  }

  /** Extrude a (z, y) profile across x ∈ [x0, x1]. */
  extrudeX(mat: string, zy: V2[], x0: number, x1: number, bevel = 0) {
    const shape = new THREE.Shape(zy.map(([z, y]) => new THREE.Vector2(z, y)));
    const g = new THREE.ExtrudeGeometry(shape, { depth: x1 - x0 - bevel * 2, bevelEnabled: bevel > 0, bevelSize: bevel, bevelThickness: bevel, bevelSegments: 1 });
    // shape X → ship z, shape Y → y, extrusion Z → −x (proper rotation, keeps winding)
    g.applyMatrix4(new THREE.Matrix4().set(0, 0, -1, x1 - bevel, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1));
    return this.add(mat, g);
  }

  /** Extrude an (x, y) profile along z ∈ [z0, z1]. */
  extrudeZ(mat: string, xy: V2[], z0: number, z1: number, holes: V2[][] = []) {
    const shape = new THREE.Shape(xy.map(([x, y]) => new THREE.Vector2(x, y)));
    for (const h of holes) shape.holes.push(new THREE.Path(h.map(([x, y]) => new THREE.Vector2(x, y))));
    const g = new THREE.ExtrudeGeometry(shape, { depth: z1 - z0, bevelEnabled: false });
    return this.add(mat, g, [0, 0, z0]);
  }

  lathe(mat: string, rz: V2[], pos: V3, rot?: THREE.Quaternion | THREE.Euler, seg = 28) {
    const g = new THREE.LatheGeometry(rz.map(([r, y]) => new THREE.Vector2(r, y)), seg);
    return this.add(mat, g, pos, rot);
  }

  build() {
    const out = new Map<string, THREE.BufferGeometry>();
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
