// Crew ↔ ship: which compartment each astronaut is in, whether it can breathe the cabin air
// (suit on "cabin" mode, topping up its reserve) or lives on its own oxygen, and suits docked to a
// seat umbilical. Shared by the server and the offline client so both apply the same rules.

import type { ShipDef, ZoneDef } from './def.js';
import type { V3 } from './geom.js';
import type { ShipSim } from './sim.js';

/** Suit oxygen (fraction of a full reserve per second): 45 min EVA, cabin top-up, choking damage. */
export const SUIT = { use: 1 / (45 * 60), refill: 0.012, dockRefill: 0.006, choke: 3 };

/** Compartment containing a ship-space point, if any. */
export function zoneAt(def: ShipDef, l: V3): ZoneDef | null {
  return def.zones.find((z) => l[0] >= z.min[0] && l[0] <= z.max[0] && l[1] >= z.min[1] - 0.2 && l[1] <= z.max[1] && l[2] >= z.min[2] && l[2] <= z.max[2]) ?? null;
}

export interface CrewPlace {
  inside: boolean;
  comp: number;
  breathable: boolean;
  docked: boolean;
}

/** Crew context for a ship tick from the astronauts' world positions (feet). */
export function crewContext(ship: ShipSim, crew: Array<{ p: V3; seated: boolean }>) {
  const counts = ship.def.compartments.map(() => 0);
  const bodies: V3[] = [];
  let docked = 0;
  const members: CrewPlace[] = crew.map(({ p, seated }) => {
    const feet = ship.toLocal(p);
    bodies.push(feet);
    const zone = zoneAt(ship.def, [feet[0], feet[1] + 1.0, feet[2]]);
    if (!zone) return { inside: false, comp: -1, breathable: false, docked: false };
    const comp = ship.sys.compIndex(zone.id);
    const breathable = ship.sys.breathable(ship.st, zone.id);
    if (breathable && comp >= 0) counts[comp]++;
    const isDocked = seated && ship.def.seats.some((s) => Math.hypot(feet[0] - s.root[0], feet[2] - s.root[2]) < 0.4);
    if (isDocked && !breathable) docked++;
    return { inside: true, comp, breathable, docked: isDocked };
  });
  return { counts, docked, bodies, members };
}
