// A world's places on its bodies: the frame of a site on its body's one surface (to place what it
// holds: ships on their pads, crates, astronauts) and where people start. Everything here is
// measured on the global ground, with the sites' own modifiers laid on it.

import type { V3 } from '../ship/geom.js';
import { bodyById, type Surfaces } from './body.js';
import { SITES, siteDir, type BaseSiteDef, type SiteDef } from './sites.js';
import { SurfaceGround } from './tangent.js';

/** The site where a world starts (the first home). */
export const START_SITE = SITES.find((s): s is BaseSiteDef => s.kind === 'base' && !!s.home) ?? null;

/** The ground of a site seen from its own tangent frame (null: its body has no surface data). */
export function siteGround(site: SiteDef, surfaces: Surfaces): SurfaceGround | null {
  const b = bodyById(site.body);
  const s = surfaces(b);
  if (!s) return null;
  return new SurfaceGround(b, s).layDir(siteDir(site, bodyById));
}

/** World point on the ground of spawn point `k` of the start site (a few cm above it). */
export function spawnPoint(k: number, surfaces: Surfaces, lift = 0.05): V3 {
  const site = START_SITE;
  if (!site) return [0, 0, 0];
  const g = siteGround(site, surfaces);
  const [x, z] = site.spawns[((k % site.spawns.length) + site.spawns.length) % site.spawns.length];
  if (!g) return [x, lift, z];
  return g.toWorld(x, g.height(x, z) + lift, z, [0, 0, 0]) as V3;
}
