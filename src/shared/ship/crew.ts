// Crew ↔ ship: which compartment each astronaut is in, whether it can breathe the cabin air
// (suit on "cabin" mode, topping up its reserve) or lives on its own oxygen, and suits docked to a
// seat umbilical. Shared by the server and the offline client so both apply the same rules.

import { zoneAtPoint, type ShipDef, type ZoneDef } from './def.js';
import type { V3 } from './geom.js';
import type { ShipSim } from './sim.js';

/** Suit oxygen (fraction of a full reserve per second): 45 min EVA, cabin top-up, choking damage. */
export const SUIT = { use: 1 / (45 * 60), refill: 0.012, dockRefill: 0.006, choke: 3 };

/** Compartment containing a ship-space point, if any. */
export function zoneAt(def: ShipDef, l: V3): ZoneDef | null {
  return zoneAtPoint(def.zones, l);
}

export interface CrewPlace {
  inside: boolean;
  comp: number;
  breathable: boolean;
  /** Seated with the pack docked to a seat umbilical that is delivering O2. */
  docked: boolean;
}

/** Crew context for a ship tick from the astronauts' world positions (feet). */
export function crewContext(ship: ShipSim, crew: Array<{ p: V3; seated: boolean }>) {
  const counts = ship.def.compartments.map(() => 0);
  const bodies: V3[] = [];
  let docked = 0;
  const life = ship.sys.life;
  const umbilical = !!life && ship.st[life.iUmb] === 1;
  const members: CrewPlace[] = crew.map(({ p, seated }) => {
    const feet = ship.toLocal(p);
    bodies.push(feet);
    const zone = zoneAt(ship.def, [feet[0], feet[1] + 1.0, feet[2]]);
    if (!zone) return { inside: false, comp: -1, breathable: false, docked: false };
    const comp = ship.sys.compIndex(zone.id);
    const breathable = ship.sys.breathable(ship.st, zone.id);
    if (breathable && comp >= 0) counts[comp]++;
    const inSeat = seated && ship.def.seats.some((s) => Math.hypot(feet[0] - s.root[0], feet[2] - s.root[2]) < 0.4);
    // the umbilical only helps a suit that isn't breathing cabin air, and only while it delivers
    const isDocked = inSeat && !breathable && umbilical;
    if (isDocked) docked++;
    return { inside: true, comp, breathable, docked: isDocked };
  });
  return { counts, docked, bodies, members };
}

/**
 * One step of every astronaut's suit oxygen against every ship: cabin air tops it up, a delivering
 * seat umbilical tops it up slower, otherwise it spends its reserve. Runs the ships' ticks too
 * (they need the crew positions). Returns the new O2 levels, whether each one breathes cabin air,
 * and each ship's tick result.
 */
export function crewStep<T>(
  ships: ShipSim[],
  crew: Array<{ p: V3; seated: boolean; o2: number }>,
  dt: number,
  tick: (ship: ShipSim, ctx: { crew: number[]; docked: number; bodies: V3[] }) => T,
): { o2: number[]; cabin: boolean[]; results: T[] } {
  const o2 = crew.map((m) => m.o2);
  const cabin = crew.map(() => false);
  const fed = crew.map(() => false);
  const results: T[] = [];
  for (const ship of ships) {
    const c = crewContext(ship, crew);
    results.push(tick(ship, { crew: c.counts, docked: c.docked, bodies: c.bodies }));
    c.members.forEach((m, i) => {
      if (m.breathable) cabin[i] = true;
      if (m.breathable) o2[i] = Math.min(1, o2[i] + SUIT.refill * dt);
      else if (m.docked) o2[i] = Math.min(1, o2[i] + SUIT.dockRefill * dt);
      if (m.breathable || m.docked) fed[i] = true;
    });
  }
  for (let i = 0; i < crew.length; i++) if (!fed[i]) o2[i] = Math.max(0, o2[i] - SUIT.use * dt);
  return { o2, cabin, results };
}
