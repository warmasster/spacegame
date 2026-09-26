// Flight groundwork: everything the flight model needs, computed from the ship definition and its
// live state, and a rigid-body integrator. Nothing here moves a ship yet (FLIGHT.enabled = false):
// the ships stay parked on their gear. What exists and is tested:
//
//   - mass properties: structure (panels by area), machines (catalog dry mass), furniture, and the
//     consumables that change in flight (propellant, gas, water, food) → mass, centre of mass,
//     inertia tensor about it
//   - thrusters: every main engine and RCS block with its position, thrust axis and rating
//   - forces: thrust of the running engines (their `thr` fraction) → net force and torque
//   - integrate: semi-implicit Euler on a pose (position, quaternion, velocity, spin)
//   - performance: thrust-to-weight on the Moon, specific impulse, Δv left
//
// Turning flight on = step `integrate` on the authority with `forces` + gravity + ground contact,
// replicate the pose, and move the ship's physics body kinematically (see docs/SHIPS.md §7).

import type { PartDef, ShipDef } from './def.js';
import { add, cross, qIntegrate, qRotate, rotY, scale, sub, type Quat, type V3 } from './geom.js';
import { partTag } from './def.js';

export const FLIGHT = {
  /** The flight model steps ships (false: parked, groundwork only). */
  enabled: false,
  /** Standard gravity for specific impulse (m/s²). */
  g0: 9.80665,
  /** Areal density of hull plating by kind (kg/m²). */
  plate: { hull: 38, glass: 30, floor: 32, bulkhead: 18 } as Record<string, number>,
  /** Mass of a suited astronaut (kg). */
  crewKg: 130,
};

/** Where a ship is and how it moves (world frame). */
export interface ShipPose {
  p: V3;
  q: Quat;
  v: V3;
  /** Angular velocity (world frame, rad/s). */
  w: V3;
}

export interface Thruster {
  part: string;
  kind: 'main' | 'rcs';
  /** Nozzle position (ship space). */
  at: V3;
  /** Direction the force pushes the ship (ship space, unit). */
  dir: V3;
  maxN: number;
  flowKg: number;
}

/** Symmetric inertia tensor [Ixx, Iyy, Izz, Ixy, Ixz, Iyz] (kg·m², ship axes). */
export type Inertia = [number, number, number, number, number, number];

export interface MassProps {
  mass: number;
  /** Centre of mass (ship space). */
  com: V3;
  /** About the centre of mass. */
  inertia: Inertia;
  /** kg by group (for the MASA page and the tests). */
  groups: Record<'estructura' | 'máquinas' | 'mobiliario' | 'propelente' | 'gases' | 'víveres' | 'tripulación', number>;
}

/** Every main engine and RCS block: engines push along −z (toward the nose) from their aft end. */
export function thrusters(def: ShipDef): Thruster[] {
  const out: Thruster[] = [];
  for (const p of def.parts) {
    if (p.type === 'engine') {
      out.push({ part: p.id, kind: 'main', at: add(p.c, rotY([0, 0, p.half[2]], p.yaw)), dir: rotY([0, 0, -1], p.yaw), maxN: p.p.thrustN ?? 0, flowKg: p.p.flowKg ?? 0 });
    } else if (p.type === 'rcs') {
      // a block fires along its six axes (the allocator picks which)
      for (const d of [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] as V3[]) out.push({ part: p.id, kind: 'rcs', at: p.c, dir: d, maxN: p.p.thrustN ?? 0, flowKg: p.p.flowKg ?? 0 });
    }
  }
  return out;
}

interface Lump {
  m: number;
  c: V3;
  /** Box half extents (solid box inertia) or none (point mass). */
  half?: V3;
  yaw?: number;
}

/** Live consumables by part (kg), read from a state table; empty = full tanks as defined. */
export type Consumables = (part: PartDef) => number;

/** Consumable mass a part holds as defined (full or `fill`). */
const initialContent: Consumables = (p) => (['tank', 'gas', 'water', 'pantry'].includes(p.type) ? p.p.fill ?? p.p.cap ?? 0 : 0);

/**
 * Mass, centre of mass and inertia of a ship with its consumables and `crew` astronauts (at
 * the given ship-space points).
 */
export function massProperties(def: ShipDef, content: Consumables = initialContent, crew: V3[] = []): MassProps {
  const lumps: Lump[] = [];
  const groups: MassProps['groups'] = { estructura: 0, máquinas: 0, mobiliario: 0, propelente: 0, gases: 0, víveres: 0, tripulación: 0 };
  for (const pn of def.panels) {
    let a = 0;
    for (let i = 0; i < pn.poly.length; i++) {
      const p = pn.poly[i];
      const q = pn.poly[(i + 1) % pn.poly.length];
      a += p[0] * q[1] - q[0] * p[1];
    }
    const m = (Math.abs(a) / 2) * (FLIGHT.plate[pn.kind] ?? 30);
    lumps.push({ m, c: pn.c });
    groups.estructura += m;
  }
  for (const p of def.parts) {
    lumps.push({ m: p.mass, c: p.c, half: p.half, yaw: p.yaw });
    groups.máquinas += p.mass;
    const kg = content(p);
    if (kg > 0) {
      lumps.push({ m: kg, c: p.c, half: p.half, yaw: p.yaw });
      const g = p.type === 'tank' ? 'propelente' : p.type === 'gas' ? 'gases' : 'víveres';
      groups[g] += kg;
    }
  }
  for (const p of def.props) {
    lumps.push({ m: p.mass, c: p.c, half: p.half, yaw: p.yaw });
    groups.mobiliario += p.mass;
  }
  for (const c of crew) {
    lumps.push({ m: FLIGHT.crewKg, c: [c[0], c[1] + 1, c[2]] });
    groups.tripulación += FLIGHT.crewKg;
  }
  const mass = lumps.reduce((a, l) => a + l.m, 0);
  const com = scale(
    lumps.reduce((a, l) => add(a, scale(l.c, l.m)), [0, 0, 0] as V3),
    1 / (mass || 1),
  );
  const I: Inertia = [0, 0, 0, 0, 0, 0];
  for (const l of lumps) {
    const r = sub(l.c, com);
    // parallel axis
    I[0] += l.m * (r[1] * r[1] + r[2] * r[2]);
    I[1] += l.m * (r[0] * r[0] + r[2] * r[2]);
    I[2] += l.m * (r[0] * r[0] + r[1] * r[1]);
    I[3] -= l.m * r[0] * r[1];
    I[4] -= l.m * r[0] * r[2];
    I[5] -= l.m * r[1] * r[2];
    if (l.half) {
      // solid box about its own centre (yaw only mixes x and z)
      const [a, b, c] = l.half.map((h) => 2 * h);
      const ix = (l.m * (b * b + c * c)) / 12;
      const iy = (l.m * (a * a + c * c)) / 12;
      const iz = (l.m * (a * a + b * b)) / 12;
      const cs = Math.cos(l.yaw ?? 0);
      const sn = Math.sin(l.yaw ?? 0);
      I[0] += ix * cs * cs + iz * sn * sn;
      I[2] += ix * sn * sn + iz * cs * cs;
      I[1] += iy;
      I[4] += (ix - iz) * cs * sn;
    }
  }
  return { mass, com, inertia: I, groups };
}

/** Net force and torque (about the centre of mass), ship space, from thrust fractions per thruster. */
export function thrust(list: Thruster[], com: V3, fraction: (t: Thruster) => number): { F: V3; T: V3 } {
  let F: V3 = [0, 0, 0];
  let T: V3 = [0, 0, 0];
  for (const t of list) {
    const k = fraction(t);
    if (k <= 0) continue;
    const f = scale(t.dir, t.maxN * k);
    F = add(F, f);
    T = add(T, cross(sub(t.at, com), f));
  }
  return { F, T };
}

/** Engine thrust fractions read from a ship's live state (`<engine>.thr` while running). */
export function engineFractions(def: ShipDef, get: (name: string) => number): (t: Thruster) => number {
  const byPart = new Map(def.parts.map((p) => [p.id, p]));
  return (t) => {
    if (t.kind !== 'main') return 0;
    const p = byPart.get(t.part)!;
    const s = get(`${partTag(p)}.state`);
    return s === 1 || s === 2 ? get(`${partTag(p)}.thr`) : 0;
  };
}

/** Principal moments (diagonal) of an inertia tensor, enough for the integrator's first version. */
const diag = (I: Inertia): V3 => [Math.max(1, I[0]), Math.max(1, I[1]), Math.max(1, I[2])];

/**
 * One semi-implicit Euler step: F and T in the ship frame (as `thrust` returns them), plus a world
 * force (gravity, contact). Updates the pose in place.
 */
export function integrate(pose: ShipPose, m: MassProps, F: V3, T: V3, worldF: V3, dt: number) {
  const Fw = add(qRotate(pose.q, F), worldF);
  pose.v = add(pose.v, scale(Fw, dt / m.mass));
  pose.p = add(pose.p, scale(pose.v, dt));
  // angular: body-frame torque over body inertia, spin kept in the world frame
  const Ib = diag(m.inertia);
  const alpha: V3 = [T[0] / Ib[0], T[1] / Ib[1], T[2] / Ib[2]];
  pose.w = add(pose.w, scale(qRotate(pose.q, alpha), dt));
  pose.q = qIntegrate(pose.q, pose.w, dt);
}

/** Performance summary: thrust-to-weight at gravity `g`, Isp, Δv with the propellant aboard. */
export function performance(def: ShipDef, m: MassProps, g: number) {
  const mains = thrusters(def).filter((t) => t.kind === 'main');
  const maxN = mains.reduce((a, t) => a + t.maxN, 0);
  const flow = mains.reduce((a, t) => a + t.flowKg, 0);
  const isp = flow > 0 ? maxN / (flow * FLIGHT.g0) : 0;
  const dry = m.mass - m.groups.propelente;
  const dv = dry > 0 && m.groups.propelente > 0 ? isp * FLIGHT.g0 * Math.log(m.mass / dry) : 0;
  // thrust line vs centre of mass: a lever that must be trimmed by the RCS
  const T = thrust(mains, m.com, () => 1).T;
  const lever = maxN > 0 ? Math.hypot(T[0], T[1], T[2]) / maxN : 0;
  return { maxN, twr: maxN / (m.mass * g), isp, dv, lever, flow };
}
