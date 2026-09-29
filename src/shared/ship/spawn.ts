// What a fresh world starts with: the ships parked on their sites' pads and the crates (their
// cargo, and the supply drops left on the ground). The server and the offline client build the same
// list, with the same ids, on the same global ground.

import { SHIP_SPAWNS } from '../constants.js';
import { bodyById, heightAboveGround, type Surfaces } from '../space/body.js';
import { SITES, siteById } from '../space/sites.js';
import { siteGround } from '../space/world.js';
import { initialCrates, type Crate } from './crates.js';
import { qYaw, type Quat, type V3 } from './geom.js';
import { placeShip, SHIP_DEFS, ShipSim } from './sim.js';

/** Height over the ground of a world point, on whatever body it is near (for a ship's ramp). */
export const groundAltOn = (surfaces: Surfaces, body: string) => {
  const b = bodyById(body);
  return (p: readonly number[]) => heightAboveGround(b, p, surfaces(b));
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
