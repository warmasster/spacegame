// What things sound like underfoot, by material. A body names its ground (`BodyDef.ground`),
// ship decks are 'deck', crates 'crate'; a new material (ice, sand, a grating) is one entry here
// and its sounds in the bank.

export interface SurfaceSound {
  /** A step. */
  step: string;
  /** Landing from a jump or a fall. */
  land: string;
  /** Loudness of both. */
  gain: number;
  /** Whether it carries sound along the ground (loose regolith does, poorly; a ship's deck is the ship's structure). */
  ground: number;
}

export const SURFACES: Record<string, SurfaceSound> = {
  regolith: { step: 'step.regolith', land: 'land.regolith', gain: 1, ground: 1 },
  deck: { step: 'step.deck', land: 'land.deck', gain: 1, ground: 0 },
  crate: { step: 'step.crate', land: 'land.deck', gain: 0.9, ground: 0 },
};

/** The sounds of a material (regolith when it has none). */
export function surface(material: string | undefined): SurfaceSound {
  return (material && SURFACES[material]) || SURFACES.regolith;
}
