// What a fresh world starts with: the ships parked on their sites' pads and the crates (their
// cargo, and the supply drops left on the ground). The server and the offline client build the same
// list, with the same ids, on the same global ground.

import { SHIP_SPAWNS } from '../constants.js';
import { bodyAt, bodyById, heightAboveGround, type Surfaces } from '../space/body.js';
import { SITES, siteById } from '../space/sites.js';
import { siteGround } from '../space/world.js';
import type { CrateSpec } from '../protocol.js';
import { initialCrates, type Crate } from './crates.js';
import { qYaw, type Quat, type V3 } from './geom.js';
import { placeShip, SHIP_DEFS, ShipSim } from './sim.js';

/**
 * Height over the ground of a world point, on whatever body rules there (for a ship's ramp and gear:
 * a ship that jumps to another system lands on that system's body). `body`: the one it starts on.
 */
export const groundAltOn = (surfaces: Surfaces, body: string) => {
  const home = bodyById(body);
  return (p: readonly number[]) => {
    const b = bodyAt(p);
    return heightAboveGround(b, p, surfaces(b === home ? home : b));
  };
};

/** The ships a fresh world has, parked on their pads. */
export function startShips(surfaces: Surfaces): ShipSim[] {
  return SHIP_SPAWNS.map((s) => {
    const site = siteById(s.site);
    if (!site) throw new Error(`ship ${s.id}: unknown site ${s.site}`);
    const g = siteGround(site, surfaces);
    if (!g) throw new Error(`ship ${s.id}: site ${s.site} has no ground`);
    const def = SHIP_DEFS[s.def];
    return new ShipSim(s.id, def, placeShip(def, g, s.x, s.z, s.yaw), groundAltOn(surfaces, site.body));
  });
}

/** The crates a fresh world has: the ships' cargo and every base's supply drops, on the ground. */
export function startCrates(ships: Array<{ id: number; def: ShipSim['def'] }>, surfaces: Surfaces): Crate[] {
  const drops: Parameters<typeof initialCrates>[1] = [];
  for (const site of SITES) {
    if (site.kind !== 'base' || !site.drops.length) continue;
    const g = siteGround(site, surfaces);
    if (!g) continue;
    for (const d of site.drops) {
      const p = g.toWorld(d.x, g.height(d.x, d.z) + d.half[1] + 0.02, d.z, [0, 0, 0]) as V3;
      const q = g.rotOut(qYaw(d.yaw), [0, 0, 0, 1]) as Quat;
      drops.push({ p, q, kind: d.kind, half: d.half, mass: d.mass, paint: d.paint });
    }
  }
  return initialCrates(ships, drops);
}

/** A site's depot laid on its ground: its centre and where each of its slots is (world). */
export interface SiteDepot {
  /** Stable name: site + depot. */
  key: string;
  spec: CrateSpec;
  center: V3;
  count: number;
  slots: number;
  enter: number;
  leave: number;
  slot(i: number): { p: V3; q: Quat };
}

/** Every base's depots, on the ground (the world keeps their contents: docs/MUNDO.md §10). */
export function siteDepots(surfaces: Surfaces): SiteDepot[] {
  const out: SiteDepot[] = [];
  for (const site of SITES) {
    if (site.kind !== 'base' || !site.depots?.length) continue;
    const g = siteGround(site, surfaces);
    if (!g) continue;
    for (const d of site.depots) {
      const rows = Math.ceil(d.slots / d.cols);
      const c = Math.cos(d.yaw);
      const sn = Math.sin(d.yaw);
      const q = g.rotOut(qYaw(d.yaw), [0, 0, 0, 1]) as Quat;
      out.push({
        key: `${site.id}.${d.id}`,
        spec: { kind: d.kind, half: d.half, mass: d.mass, paint: d.paint },
        center: g.toWorld(d.x, g.height(d.x, d.z), d.z, [0, 0, 0]) as V3,
        count: d.count,
        slots: d.slots,
        enter: d.enter,
        leave: d.leave,
        slot(i) {
          const lx = ((i % d.cols) - (d.cols - 1) / 2) * d.spacing;
          const lz = (Math.floor(i / d.cols) - (rows - 1) / 2) * d.spacing;
          const x = d.x + lx * c + lz * sn;
          const z = d.z - lx * sn + lz * c;
          return { p: g.toWorld(x, g.height(x, z) + d.half[1] + 0.02, z, [0, 0, 0]) as V3, q: [...q] as Quat };
        },
      });
    }
  }
  return out;
}

/** A site's crew (docs/MUNDO.md §11): where they gather, in the site's frame and on its ground. */
export interface SiteCrew {
  /** Stable name: site + crew. */
  key: string;
  site: string;
  x: number;
  z: number;
  center: V3;
  count: number;
  slots: number;
  enter: number;
  leave: number;
  mix: number[];
}

/** Every base's crews. */
export function siteCrews(surfaces: Surfaces): SiteCrew[] {
  const out: SiteCrew[] = [];
  for (const site of SITES) {
    if (site.kind !== 'base' || !site.crew?.length) continue;
    const g = siteGround(site, surfaces);
    if (!g) continue;
    for (const c of site.crew) {
      out.push({ key: `${site.id}.${c.id}`, site: site.id, x: c.x, z: c.z, center: g.toWorld(c.x, g.height(c.x, c.z), c.z, [0, 0, 0]) as V3, count: c.count, slots: c.slots, enter: c.enter, leave: c.leave, mix: [...c.mix] });
    }
  }
  return out;
}

/** A point of a site's frame on its ground (world); null: no such site or no ground. */
export function siteToWorld(surfaces: Surfaces, site: string, x: number, z: number): V3 | null {
  const s = siteById(site);
  const g = s ? siteGround(s, surfaces) : null;
  return g ? (g.toWorld(x, g.height(x, z), z, [0, 0, 0]) as V3) : null;
}
