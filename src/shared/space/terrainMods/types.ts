// Terrain modifiers: changes laid on a body's procedural surface (space/surface.ts). The flattened
// ground of a base and its pads, blast craters, hand-placed craters, trenches and ramps are all
// modifiers; so will be the clearings and roads a settlement generator emits from a seed. There is
// one ground per body: a modifier is not a world of its own, only a pure function of the ground
// under it (kinds.ts), so every client, worker and the server get exactly the same surface.
//
// A modifier is plain data (serialisable): the server keeps the dynamic ones (craters) and sends
// them; the static ones (sites, space/sites.ts) every machine rebuilds from the world seed.

export type Vec3 = [number, number, number];

export interface TerrainMod {
  /** Kind in the registry (kinds.ts `MOD_KINDS`): 'flatten', 'crater', 'trench', 'ramp'… */
  kind: string;
  /** Body it lies on (`BODIES` id). */
  body: string;
  /** Centre: radial unit direction, rounded for serialization (its length may differ slightly from 1). */
  center: Vec3;
  /** Size (m over the surface); what it measures is the kind's (its reach is a multiple of it). */
  radius: number;
  /** The kind's parameters. NaN = "the ground here as it is": filled in when it is laid (`BodySurface.addMod`). */
  params?: number[];
  /** Heading of a shape that has one (rad from north toward east). */
  yaw?: number;
  /** Salt of irregular shapes (a crater's rim). */
  seed?: number;
}

/** A point in a modifier's own frame: x east, z south (m, on the tangent plane at its centre) and its distance. */
export interface ModPoint {
  x: number;
  z: number;
  d: number;
}
