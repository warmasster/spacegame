// Ship flight: pose maths, mass, thrusters, allocator, flight computer, autopilot, ground contact
// and the step that puts them together. See docs/SHIPS.md §5.
//
//   sim.flight.step(dt, stick, { surface })     ← whoever has flight authority, at 60 Hz
//
// Everyone else only reads the pose (and the thruster outputs in the state table).

import type { ShipDef } from '../def.js';
import { cross, sub, type V3 } from '../geom.js';
import type { MassProps } from './mass.js';
import { FLIGHT } from './model.js';
import { ThrusterSet } from './thrusters.js';

export * from './pose.js';
export * from './mass.js';
export * from './thrusters.js';
export * from './allocate.js';
export * from './contact.js';
export * from './body.js';
export * from './fcs.js';
export * from './autopilot.js';
export * from './nav.js';
export * from './model.js';
export * from './readout.js';

/** Performance summary: thrust-to-weight (main engines and lift pads), Isp, Δv with the propellant aboard. */
export function performance(def: ShipDef, m: MassProps, g = FLIGHT.g) {
  const set = new ThrusterSet(def);
  const maxN = set.mains.reduce((a, t) => a + t.maxN, 0);
  const flow = set.mains.reduce((a, t) => a + t.flowKg, 0);
  const liftN = set.lifts.reduce((a, t) => a + t.maxN, 0);
  const isp = flow > 0 ? maxN / (flow * FLIGHT.g0) : 0;
  const dry = m.mass - m.groups.propelente;
  const dv = dry > 0 && m.groups.propelente > 0 ? isp * FLIGHT.g0 * Math.log(m.mass / dry) : 0;
  // thrust line of the main engines (as built) against the centre of mass: what the RCS must trim
  let T: V3 = [0, 0, 0];
  for (const t of set.mains) {
    const f: V3 = [t.dir[0] * t.maxN, t.dir[1] * t.maxN, t.dir[2] * t.maxN];
    const tq = cross(sub(t.at, m.com), f);
    T = [T[0] + tq[0], T[1] + tq[1], T[2] + tq[2]];
  }
  const lever = maxN > 0 ? Math.hypot(T[0], T[1], T[2]) / maxN : 0;
  const weight = m.mass * g;
  return { maxN, flow, isp, dv, lever, twr: maxN / weight, liftN, liftTwr: liftN / weight };
}

/** Seat that flies this ship, or null when it has no helm. */
export function helmSeat(def: ShipDef): string | null {
  if (!def.helm) return null;
  if (def.helm.seat) return def.helm.seat;
  return def.seats.find((s) => s.id.endsWith('pilot'))?.id ?? def.seats[0]?.id ?? null;
}

/** Index of the helm seat in `def.seats` (-1 = none). */
export function helmSeatIndex(def: ShipDef): number {
  const id = helmSeat(def);
  return id ? def.seats.findIndex((s) => s.id === id) : -1;
}

/** One line for the HUD when someone sits in the pilot seat. */
export const PILOT_KEYS =
  'Piloto: W/S adelante/atrás · A/D guiñada · R/F subir/bajar · Z/C lateral · ↑/↓ cabeceo · ←/→ alabeo · P piloto automático · Espacio levantarse';
