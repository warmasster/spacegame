// Loose objects (crates, spare parts… any kind of the object catalog, shared/items/objects.ts;
// "crate" is the machinery's name). Every ship's cargo list becomes objects in that ship's frame at start; after that a
// crate lives in whatever frame it is in (the moon, or any ship it was carried into) and is
// simulated by one player at a time (its owner) or by nobody (it lies still). The server and the
// offline client build the same list, with the same ids.

import type { CrateSpec, CrateWire, Quat, Vec3 } from '../protocol.js';
import { WORLD_FRAME } from '../frames/frame.js';
import type { ShipDef } from './def.js';
import { qYaw } from './geom.js';

export { WORLD_FRAME };

export type Crate = CrateWire & CrateSpec;

/**
 * Every crate a fresh world starts with: each ship's cargo list in that ship's frame, then the
 * crates on the ground (world poses: the sites' supply drops, ship/spawn.ts).
 */
export function initialCrates(ships: Array<{ id: number; def: ShipDef }>, ground: Array<{ p: Vec3; q: Quat } & CrateSpec>): Crate[] {
  const out: Crate[] = [];
  for (const s of ships) {
    for (const c of s.def.cargo) {
      out.push({ id: out.length + 1, fr: s.id, p: [...c.pos], q: qYaw(c.yaw) as Quat, v: [0, 0, 0], w: [0, 0, 0], owner: 0, kind: c.kind, half: [...c.half], mass: c.mass, paint: c.paint });
    }
  }
  for (const c of ground) {
    out.push({ id: out.length + 1, fr: WORLD_FRAME, p: [...c.p], q: [...c.q], v: [0, 0, 0], w: [0, 0, 0], owner: 0, kind: c.kind, half: [...c.half], mass: c.mass, paint: c.paint });
  }
  return out;
}

const finite = (v: unknown, n: number): boolean => Array.isArray(v) && v.length === n && v.every((x) => typeof x === 'number' && Number.isFinite(x));

/** A crate state from the network: well formed and not absurd. */
export function validCrate(c: unknown): c is Omit<CrateWire, 'owner'> {
  if (!c || typeof c !== 'object') return false;
  const o = c as Record<string, unknown>;
  if (!Number.isInteger(o.id) || !Number.isInteger(o.fr)) return false;
  if (!finite(o.p, 3) || !finite(o.q, 4) || !finite(o.v, 3) || !finite(o.w, 3)) return false;
  // frame 0 is the world (crates can be anywhere round the body, or in orbit)
  return (o.p as number[]).every((x) => Math.abs(x) < 1e10);
}
