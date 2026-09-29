import * as THREE from 'three';
import type { CrateSpec } from '../../shared/protocol';

/** The materials loose objects are painted with (made once, lit like the cabins). */
export interface CargoPalette {
  orange: THREE.Material;
  grey: THREE.Material;
  strap: THREE.Material;
  metal: THREE.Material;
}

/**
 * How a kind of loose object is drawn (the object catalog's `look`, shared/items/objects.ts): one
 * geometry per size and paint, with material groups, filling the object's box (half extents
 * `spec.half`, centred). Every object of the same look, size and paint is one instanced mesh.
 */
export interface ObjectLook {
  geometry(spec: CrateSpec): THREE.BufferGeometry;
  materials(spec: CrateSpec, p: CargoPalette): THREE.Material[];
}

export const OBJECT_LOOKS: Record<string, ObjectLook> = {};

export function defineObjectLook(id: string, look: ObjectLook) {
  if (OBJECT_LOOKS[id]) throw new Error(`object look "${id}" defined twice`);
  OBJECT_LOOKS[id] = look;
}

/** Parts → one non-indexed geometry, a material group per part (`group`: its material index). */
function merge(parts: Array<[THREE.BufferGeometry, number]>) {
  const g = new THREE.BufferGeometry();
  const pos: number[] = [];
  const nrm: number[] = [];
  for (const [geo, group] of parts) {
    const x = geo.index ? geo.toNonIndexed() : geo;
    g.addGroup(pos.length / 3, x.getAttribute('position').count, group);
    pos.push(...(x.getAttribute('position').array as Float32Array));
    nrm.push(...(x.getAttribute('normal').array as Float32Array));
  }
  g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
  g.setAttribute('normal', new THREE.Float32BufferAttribute(nrm, 3));
  return g;
}

const paintOf = (spec: CrateSpec, p: CargoPalette) => (spec.paint === 'orange' ? p.orange : p.grey);

// a cargo crate with two straps
defineObjectLook('crate', {
  geometry(spec) {
    const [hx, hy, hz] = spec.half;
    const box = new THREE.BoxGeometry(hx * 2, hy * 2, hz * 2);
    const straps = [hy * 0.5, -hy * 0.5].map((y) => new THREE.BoxGeometry(hx * 2 + 0.012, 0.04, hz * 2 + 0.012).translate(0, y, 0));
    return merge([[box, 0], ...straps.map((s): [THREE.BufferGeometry, number] => [s, 1])]);
  },
  materials: (spec, p) => [paintOf(spec, p), p.strap],
});

// a machined spare: a flanged housing with a port on its side, the flanges in the owner's paint
defineObjectLook('spare', {
  geometry(spec) {
    const [hx, hy, hz] = spec.half;
    const r = Math.min(hx, hz);
    const body = new THREE.CylinderGeometry(r * 0.78, r * 0.78, hy * 1.5, 20);
    const flanges = [hy * 0.84, -hy * 0.84].map((y) => new THREE.CylinderGeometry(r, r, hy * 0.32, 20).translate(0, y, 0));
    const port = new THREE.CylinderGeometry(r * 0.28, r * 0.28, r * 0.5, 12).rotateZ(Math.PI / 2).translate(r * 0.9, 0, 0);
    const bolts = [0, 1, 2, 3].map((i) => new THREE.CylinderGeometry(r * 0.07, r * 0.07, hy * 0.06, 6).translate(Math.cos((i * Math.PI) / 2 + 0.4) * r * 0.86, hy, Math.sin((i * Math.PI) / 2 + 0.4) * r * 0.86));
    return merge([[body, 0], [port, 0], ...flanges.map((f): [THREE.BufferGeometry, number] => [f, 1]), ...bolts.map((b): [THREE.BufferGeometry, number] => [b, 2])]);
  },
  materials: (spec, p) => [p.metal, paintOf(spec, p), p.strap],
});
