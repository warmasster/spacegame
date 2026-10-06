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

const hullOf = new WeakMap<ShipDef, number>();

/**
 * Farthest its outside reaches from its origin along any axis (m): its whole bounds (wings, ramp,
 * gear) and a margin. Beyond this nothing can touch it or rest on it — the cheap test before
 * asking anything of its colliders (replaces "within 40 m", which a bigger ship would outgrow).
 */
export function shipHullReach(def: ShipDef, margin = 2): number {
  let r = hullOf.get(def);
  if (r === undefined) {
    r = shipReach(def);
    for (let i = 0; i < 3; i++) r = Math.max(r, Math.abs(def.bounds.min[i]), Math.abs(def.bounds.max[i]));
    hullOf.set(def, r);
  }
  return r + margin;
}

/** A point of the ship's space well clear of its outside (cheap reject). */
export const clearOfHull = (def: ShipDef, l: readonly number[], margin = 2) => {
  const r = shipHullReach(def, margin);
  return Math.abs(l[0]) > r || Math.abs(l[1]) > r || Math.abs(l[2]) > r;
};

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
