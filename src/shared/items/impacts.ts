// Terrain consequences of an impact, independent of launcher, host and authority transport.
// This prepares data only. The authority adds it once and broadcasts it; clients apply that
// event to their surface, terrain mesh, rocks and collision tiles. See docs/EQUIPO.md §2.1.
import { bodyAt, heightAboveGround, type Surfaces } from '../space/body.js';
import { resolveMod, terrainModAt, type TerrainMod } from '../space/terrainMods/index.js';

/** A catalog-declared surface edit. `kind` and `params` belong to MOD_KINDS, not to a weapon. */
export interface TerrainImpact {
  kind: string;
  radius: number;
  /** Impacts at or above this height over the real surface do not edit it (m). */
  maxHeight: number;
  params?: number[];
  yaw?: number;
}

export function terrainImpact(effect: TerrainImpact | undefined, p: readonly number[], surfaces: Surfaces): TerrainMod | undefined {
  if (!effect) return undefined;
  const body = bodyAt(p);
  const surface = surfaces(body);
  if (!surface || heightAboveGround(body, p, surface) >= effect.maxHeight) return undefined;
  const mod = terrainModAt(effect.kind, body.def.id, body.center, p, effect.radius);
  if (effect.params) mod.params = effect.params.slice();
  if (effect.yaw !== undefined) mod.yaw = effect.yaw;
  // Resolve NaN parameters before serialization; JSON would turn an unresolved NaN into null.
  return resolveMod(mod, body, d => surface.height(d));
}
