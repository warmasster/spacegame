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
  /**
   * What the crew can do with it. `throwSpeed`: how fast a throw sends it (m/s, relative to the
   * thrower); left out, from its mass (a shove of THROW_IMPULSE N·s, within THROW_SPEED).
   */
  handling: { grab: boolean; throw: boolean; throwSpeed?: number };
  /** How it behaves as a body (anything left out: DEFAULT_OBJECT_PHYSICS). */
  physics?: Partial<ObjectPhysics>;
  /** Client look (client/cargo/looks.ts). */
  look: string;
  /** Sound bank ids (the common ones when left out). */
  sounds?: { grab?: string; drop?: string; throw?: string };
}

/**
 * A loose body's contact and damping. Linear damping is 0 by default: out there is vacuum, and
 * damping is measured in whatever frame the body is simulated in — a moving one (a ship in flight,
 * the physics bubble in orbit) would brake it against that frame, not against anything real (a
 * crate thrown out of a ship at 250 m/s would fall behind at 12 m/s²). Air drag comes from the air
 * (client/cargo/crates.ts `airPull`), where there is air.
 */
export interface ObjectPhysics {
  friction: number;
  restitution: number;
  linearDamping: number;
  /** Tumbling fades (it keeps loose cargo from spinning forever on a deck). */
  angularDamping: number;
}

export const DEFAULT_OBJECT_PHYSICS: Readonly<ObjectPhysics> = { friction: 0.85, restitution: 0.05, linearDamping: 0, angularDamping: 0.3 };

/** A throw is about this shove (N·s) when the kind gives no speed of its own… */
export const THROW_IMPULSE = 275;
/** …within these speeds (m/s). */
export const THROW_SPEED: readonly [number, number] = [2.5, 9];

const physicsCache = new WeakMap<ObjectDef, ObjectPhysics>();

/** Contact and damping of a kind (its own values over the defaults). */
export function physicsOf(def: ObjectDef): ObjectPhysics {
  let p = physicsCache.get(def);
  if (!p) physicsCache.set(def, (p = { ...DEFAULT_OBJECT_PHYSICS, ...def.physics }));
  return p;
}

/** How fast a throw sends an object of this kind and mass (m/s). */
export function throwSpeedOf(def: ObjectDef, mass = def.mass): number {
  if (def.handling.throwSpeed !== undefined) return def.handling.throwSpeed;
  return Math.max(THROW_SPEED[0], Math.min(THROW_SPEED[1], THROW_IMPULSE / Math.max(1, mass)));
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
