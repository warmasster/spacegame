import * as THREE from 'three';
import type { PartDef, PropDef } from '../../../shared/ship/def';
import { rotY, type V3 } from '../../../shared/ship/geom';
import { Kit, type ModelAnim, type ModelBuilder, type Slot } from './kit';
import { DEFAULT_MACHINE, MACHINES } from './machines';
import { FURNITURE_MODELS, furnitureHull } from './furniture';

/**
 * Model library: machines (`PartDef.model`) and furniture / structure (`PropDef.model`) drawn from
 * their data box, in metres, merged into one mesh per material slot. A new model = a builder in
 * machines.ts or furniture.ts under its name; a part without one gets a generic cabinet.
 * See docs/SHIPS.md §6.
 */

export type { Slot } from './kit';

/** Registered builder names (tests, the catalog tool). */
export const MODEL_NAMES = { machines: Object.keys(MACHINES), furniture: Object.keys(FURNITURE_MODELS) };

/** A built model: its group, and its deployment animation if it has moving pieces. */
export interface BuiltModel {
  group: THREE.Group;
  animate: ((t: number) => void) | null;
  /** Slots the model uses (the view clones the per-machine ones). */
  slots: Slot[];
}

function build(builder: ModelBuilder, half: V3, info: Parameters<ModelBuilder>[2], mat: (slot: Slot) => THREE.Material, name: string): BuiltModel {
  const kit = new Kit();
  const anim = builder(kit, half, info) as ModelAnim | undefined;
  const group = new THREE.Group();
  group.name = name;
  const slots = new Set<Slot>();
  const meshes = (k: Kit, parent: THREE.Object3D) => {
    for (const [slot, geo] of k.build()) {
      const m = new THREE.Mesh(geo, mat(slot));
      m.castShadow = m.receiveShadow = true;
      parent.add(m);
      slots.add(slot);
    }
  };
  meshes(kit, group);
  const nodes: Record<string, THREE.Object3D> = {};
  for (const [id, c] of kit.children) {
    const node = new THREE.Group();
    node.name = id;
    node.position.set(...c.pivot);
    meshes(c.kit, node);
    group.add(node);
    nodes[id] = node;
  }
  return { group, animate: anim ? (t) => anim(nodes, t) : null, slots: [...slots] };
}

/** A machine at its place in the ship (group positioned and turned by its box). */
export function machineModel(part: PartDef, mat: (slot: Slot) => THREE.Material): BuiltModel {
  const builder = MACHINES[part.model] ?? DEFAULT_MACHINE;
  const out = build(builder, part.half, { look: part.look, p: part.p, side: Math.sign(Math.round(part.c[0] * 10)), size: part.size }, mat, `part:${part.id}`);
  out.group.position.set(part.c[0], part.c[1], part.c[2]);
  out.group.rotation.y = part.yaw;
  return out;
}

export function propModel(prop: PropDef, mat: (slot: Slot) => THREE.Material): BuiltModel {
  const builder = FURNITURE_MODELS[prop.model] ?? FURNITURE_MODELS.block;
  const out = build(builder, prop.half, { look: prop.look, p: prop.p, side: Math.sign(Math.round(prop.c[0] * 10)) }, mat, `prop:${prop.id}`);
  out.group.position.set(prop.c[0], prop.c[1], prop.c[2]);
  out.group.rotation.y = prop.yaw;
  return out;
}

/** Ship-space convex points of a `hull`-collider prop (its outline), or null. */
export function propHull(prop: PropDef): V3[] | null {
  const pts = furnitureHull(prop.model, prop.half, prop.look);
  if (!pts) return null;
  return pts.map((p) => {
    const r = rotY(p, prop.yaw);
    return [r[0] + prop.c[0], r[1] + prop.c[1], r[2] + prop.c[2]] as V3;
  });
}
