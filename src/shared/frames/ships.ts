// A ship as a frame host: its compartments (zones) are what is "inside" it. The client's ShipClient
// implements FrameHost with these; the server wraps a ShipSim with `shipHost`.

import { zoneAtPoint, type ShipDef } from '../ship/def.js';
import type { FramePose, FrameHost } from './frame.js';

const reachOf = new WeakMap<ShipDef, number>();

/** Farthest any compartment of the ship reaches from its origin, along any axis (m). */
export function shipReach(def: ShipDef): number {
  let r = reachOf.get(def);
  if (r === undefined) {
    r = 1;
    for (const z of def.zones) for (let i = 0; i < 3; i++) r = Math.max(r, Math.abs(z.min[i]), Math.abs(z.max[i]));
    r += 0.5;
    reachOf.set(def, r);
  }
  return r;
}

/** Inside one of the ship's compartments (ship space). */
export const inShip = (def: ShipDef, l: readonly number[]) => !!zoneAtPoint(def.zones, l as [number, number, number]);

/** A ship the server simulates, as a host (no step before: it is not drawn there). */
export function shipHost(ship: { id: number; def: ShipDef; pose: FramePose }): FrameHost {
  return {
    id: ship.id,
    get pose() {
      return ship.pose;
    },
    get prev() {
      return ship.pose;
    },
    reach: shipReach(ship.def),
    inside: (l) => inShip(ship.def, l),
  };
}
