// Loose object catalog: anything that lies around as a real physical body — cargo crates, spare
// parts, and whatever comes next (food, tools, canisters…). They all share the same machinery
// (client/cargo/crates.ts: one simulating owner, frames, network) and the same handling: E picks
// one up or lets it go, Q throws it, a click sets it down. A kind adds its size, mass, look,
// sounds and whatever it allows; a placement (a ship's cargo list, a site's drops) may resize it.

import type { V3 } from '../ship/geom.js';

export interface ObjectDef {
  id: string;
  name: string;
  /** Default size (half extents, m) and mass (kg). */
  half: V3;
  mass: number;
  /** What the crew can do with it. */
  handling: { grab: boolean; throw: boolean };
  /** Client look (client/cargo/looks.ts). */
  look: string;
  /** Sound bank ids (the common ones when left out). */
  sounds?: { grab?: string; drop?: string; throw?: string };
}

export const OBJECTS: Record<string, ObjectDef> = {};

export function defineObject(def: ObjectDef): ObjectDef {
  if (OBJECTS[def.id]) throw new Error(`object "${def.id}" defined twice`);
  OBJECTS[def.id] = def;
  return def;
}

/** The catalog entry of a loose object (its `kind`; crates when it has none). */
export const objectOf = (spec: { kind?: string }): ObjectDef => (spec.kind && Object.hasOwn(OBJECTS, spec.kind) ? OBJECTS[spec.kind] : OBJECTS.crate);

defineObject({
  id: 'crate',
  name: 'Caja de carga',
  half: [0.4, 0.3, 0.4],
  mass: 50,
  handling: { grab: true, throw: true },
  look: 'crate',
  sounds: { grab: 'crate.grab', drop: 'crate.drop', throw: 'crate.throw' },
});

// a machined spare (a pump housing, a valve body): small, dense, easy to pass to a crewmate
defineObject({
  id: 'spare',
  name: 'Pieza de repuesto',
  half: [0.16, 0.11, 0.16],
  mass: 9,
  handling: { grab: true, throw: true },
  look: 'spare',
  sounds: { grab: 'crate.grab', drop: 'crate.drop', throw: 'crate.throw' },
});
