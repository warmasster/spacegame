// Terrain modifiers: data (types.ts), their kinds (kinds.ts) and the per-surface store with its
// spatial index (store.ts). See the header of types.ts.

export type { ModPoint, TerrainMod } from './types.js';
export { MOD_KINDS, modFeature, modReach, type ModKind } from './kinds.js';
export { blastCrater, modTouches, resolveMod, TerrainMods, type ModBody } from './store.js';
