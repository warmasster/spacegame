// `flyShip` moves a ship: main-engine thrust (gimballed if the part says so), an RCS mix, the pilot
// assists, lunar gravity and ground contact. Offline, the client is the authority and calls it
// itself. Online, the server calls it and broadcasts the pose (`shipPose`); the pilot's stick
// arrives as `fly`. See docs/SHIPS.md §5.

import type { PartDef, ShipDef } from './def.js';
import { add, cross, dot, qConj, qIntegrate, qMul, qNorm, qRotate, rotY, scale, sub, type Quat, type V3 } from './geom.js';
import { partTag } from './def.js';
import type { ShipSim } from './sim.js';

export const FLIGHT = {
  /**
   * The offline client sets this and steps `flyShip`. The server leaves it false, so a networked
   * ship stays where it was parked until flight is authoritative there too.
   */
  enabled: false,
  /** Standard gravity for specific impulse (m/s²). */
  g0: 9.80665,
  /** Lunar surface gravity (m/s²), the flight step. */
  g: 1.62,
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

/** Rodrigues rotation of `v` about a unit axis. */
function rotAbout(axis: V3, v: V3, ang: number): V3 {
  const c = Math.cos(ang);
  const s = Math.sin(ang);
  return add(add(scale(v, c), scale(cross(axis, v), s)), scale(axis, dot(axis, v) * (1 - c)));
}

/**
 * Every main engine and RCS block. Engines push along −z (toward the nose) from their aft end,
 * tilted by `p.gimbal` when that switch is on (0 = as built, 1 = fully tilted). `gimbalOf`
 * reads the switch (0..1). RCS blocks can fire along their six axes; the allocator picks which.
 */
export function thrusters(def: ShipDef, gimbalOf: (key: string) => number = () => 0): Thruster[] {
  const out: Thruster[] = [];
  for (const p of def.parts) {
    if (p.type === 'engine') {
      const axis = rotY([1, 0, 0], p.yaw);
      const ang = p.gimbal ? gimbalOf(p.gimbal.key) * p.gimbal.rad : 0;
      const off = rotAbout(axis, rotY([0, 0, p.half[2]], p.yaw), ang);
      out.push({ part: p.id, kind: 'main', at: add(p.c, off), dir: rotAbout(axis, rotY([0, 0, -1], p.yaw), ang), maxN: p.p.thrustN ?? 0, flowKg: p.p.flowKg ?? 0 });
    } else if (p.type === 'rcs') {
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

// ---------------------------------------------------------------------------------------------
// Pilot axes and the step that moves a ship
// ---------------------------------------------------------------------------------------------

/**
 * What the pilot is asking for, each axis −1..1, in pilot axes (not ship axes):
 * surge + toward the nose, sway + to the right, heave + up,
 * pitch + nose up, yaw + nose right, roll + right wing down.
 * The console throttle is not in here: it commands the main engines on its own.
 */
export interface FlightCommand {
  surge: number;
  sway: number;
  heave: number;
  pitch: number;
  yaw: number;
  roll: number;
}

export const FLIGHT_IDLE: FlightCommand = { surge: 0, sway: 0, heave: 0, pitch: 0, yaw: 0, roll: 0 };

/** Wire form of a stick command: surge, sway, heave, pitch, yaw, roll, each −1..1. */
export type FlightAxes = [number, number, number, number, number, number];

const axis = (v: unknown) => {
  const n = typeof v === 'number' ? v : 0;
  return Number.isFinite(n) ? Math.max(-1, Math.min(1, n)) : 0;
};

export function flightAxes(c: FlightCommand): FlightAxes {
  return [c.surge, c.sway, c.heave, c.pitch, c.yaw, c.roll];
}

/** A stick command from the network. Anything malformed becomes idle on that axis. */
export function flightCommand(a: readonly unknown[] | undefined): FlightCommand {
  const x = a ?? [];
  return { surge: axis(x[0]), sway: axis(x[1]), heave: axis(x[2]), pitch: axis(x[3]), yaw: axis(x[4]), roll: axis(x[5]) };
}

/** One line for the HUD when someone sits in the pilot seat. */
export const PILOT_KEYS = 'Piloto: W/S cabeceo, A/D guiñada, Z/C alabeo, flechas trasladan, R sube, F baja. HORIZ. y ALTURA, en el tablero de vuelo, mantienen el horizonte y la altura. El acelerador está en la consola.';

/** Seat that flies this ship, or null when it has no helm. */
export function helmSeat(def: ShipDef): string | null {
  if (!def.helm) return null;
  if (def.helm.seat) return def.helm.seat;
  return def.seats.find((s) => s.id.endsWith('pilot'))?.id ?? def.seats[0]?.id ?? null;
}

/** Yaw of a pose quaternion (radians, three.js convention). */
export function poseYaw(q: Quat): number {
  return Math.atan2(2 * (q[3] * q[1] + q[0] * q[2]), 1 - 2 * (q[1] * q[1] + q[2] * q[2]));
}

/** A ship-space point inside the hull bounds, with a margin for the stairs and the roof. */
export function aboard(def: ShipDef, local: V3, margin = 0.6): boolean {
  const b = def.bounds;
  return local[0] >= b.min[0] - margin && local[0] <= b.max[0] + margin && local[1] >= b.min[1] - margin && local[1] <= b.max[1] + margin && local[2] >= b.min[2] - margin && local[2] <= b.max[2] + margin;
}

/** World point carried rigidly from one pose to the next (same ship-space place). */
export function carried(prev: ShipPose, next: ShipPose, world: V3): V3 {
  const local = qRotate(qConj(prev.q), sub(world, prev.p));
  return add(next.p, qRotate(next.q, local));
}

/** World velocity of a point fixed on the ship (`local` is ship space). */
export function deckVelocity(pose: ShipPose, local: V3): V3 {
  return add(pose.v, cross(pose.w, qRotate(pose.q, local)));
}

/**
 * World velocity carried with the ship: the part of `vel` that was moving with the old pose
 * becomes the new pose's motion, and whatever was moving relative to the ship stays relative.
 */
export function carriedVelocity(prev: ShipPose, next: ShipPose, world: V3, vel: V3): V3 {
  const local = qRotate(qConj(prev.q), sub(world, prev.p));
  const rel = qRotate(qConj(prev.q), sub(vel, deckVelocity(prev, local)));
  return add(deckVelocity(next, local), qRotate(next.q, rel));
}

/** World orientation carried with the ship. */
export function carriedRotation(prev: ShipPose, next: ShipPose, q: Quat): Quat {
  return qNorm(qMul(next.q, qMul(qConj(prev.q), q)));
}

/**
 * RCS mix. Each entry matches `list` (main engines stay 0). A thruster fires in proportion to how
 * much of the asked force and torque (ship frame, newtons and N·m) its full thrust would cover.
 */
export function allocateRcs(list: Thruster[], com: V3, wantF: V3, wantT: V3): number[] {
  return list.map((t) => {
    if (t.kind !== 'rcs' || t.maxN <= 0) return 0;
    const F = scale(t.dir, t.maxN);
    const tau = cross(sub(t.at, com), F);
    const sf = dot(F, wantF) / (t.maxN * t.maxN);
    const t2 = dot(tau, tau);
    const st = t2 > 1 ? dot(tau, wantT) / t2 : 0;
    return Math.max(0, Math.min(1, sf + st));
  });
}

const clamp1 = (v: number) => Math.max(-1, Math.min(1, v));
const LIN = 0.55;
const HOLD_V = 6;
const HOLD_K = 1.4;
const ANG = 0.28;
const SAS_K = 2.2;

/**
 * One flight step on the authority sim. Updates `pose`, `landed` and `onPad`, and writes each
 * RCS block's `.use` (the propellant burn happens on the next systems tick). `cmd` is the pilot;
 * an idle command with the assists on just holds the ship on its gear.
 */
export function flyShip(sim: ShipSim, dt: number, cmd: FlightCommand, ground: (x: number, z: number) => number) {
  const pose = sim.pose;
  const m = sim.massNow();
  if (m.mass < 1 || dt <= 0) return;
  // parked, level and idle: do not integrate. A live spring on uneven ground is what makes the hull shiver.
  {
    const gk = sim.def.gear?.key;
    const gearDown = gk ? Math.max(0, Math.min(1, sim.mover(gk))) : 1;
    const up = qRotate(pose.q, [0, 1, 0]);
    const idle = Math.abs(cmd.surge) + Math.abs(cmd.sway) + Math.abs(cmd.heave) + Math.abs(cmd.pitch) + Math.abs(cmd.yaw) + Math.abs(cmd.roll) < 0.05;
    const quiet = Math.hypot(pose.v[0], pose.v[1], pose.v[2]) < 0.04 && Math.hypot(pose.w[0], pose.w[1], pose.w[2]) < 0.01;
    const frac = engineFractions(sim.def, (n) => sim.get(n));
    const pushing = thrusters(sim.def).some((t) => t.kind === 'main' && frac(t) > 0.08);
    if (gearDown > 0.9 && up[1] > 0.996 && idle && sim.apAlt == null && quiet && !pushing) {
      pose.v = [0, 0, 0];
      pose.w = [0, 0, 0];
      sim.landed = true;
      return;
    }
  }
  const list = thrusters(sim.def, (key) => sim.sw[key] ?? 0);
  const mains = thrust(list, m.com, engineFractions(sim.def, (n) => sim.get(n)));
  const qInv = qConj(pose.q);
  const vB = qRotate(qInv, pose.v);
  const wB = qRotate(qInv, pose.w);
  const hold = sim.sw['fa.hold'] === 1;
  const sas = sim.sw['fa.sas'] !== 0;
  const stick = (v: number) => Math.abs(v) > 0.12;
  let ax: number;
  let ay: number;
  let az: number;
  if (hold) {
    ax = (clamp1(cmd.sway) * HOLD_V - vB[0]) * HOLD_K;
    ay = (clamp1(cmd.heave) * HOLD_V - vB[1]) * HOLD_K;
    az = (-clamp1(cmd.surge) * HOLD_V - vB[2]) * HOLD_K;
  } else {
    ax = clamp1(cmd.sway) * LIN;
    ay = clamp1(cmd.heave) * LIN;
    az = -clamp1(cmd.surge) * LIN;
  }
  // the main engines already accelerate these axes; the RCS must not brake against them
  if (Math.abs(mains.F[0]) > 400) ax = clamp1(cmd.sway) * LIN;
  if (Math.abs(mains.F[1]) > 400) ay = clamp1(cmd.heave) * LIN;
  if (Math.abs(mains.F[2]) > 400) az = -clamp1(cmd.surge) * LIN;
  const damp = (onStick: boolean) => (sas ? (onStick ? 0.4 : SAS_K) : 0);
  const alpha: V3 = [
    clamp1(cmd.pitch) * ANG - wB[0] * damp(stick(cmd.pitch)),
    -clamp1(cmd.yaw) * ANG - wB[1] * damp(stick(cmd.yaw)),
    -clamp1(cmd.roll) * ANG - wB[2] * damp(stick(cmd.roll)),
  ];
  // horizon hold: ship up toward world up, unless the pilot is steering that axis
  if (sim.sw['ap.hor'] === 1) {
    const upB = qRotate(qInv, [0, 1, 0]);
    if (!stick(cmd.pitch)) alpha[0] += upB[2] * 2.4;
    if (!stick(cmd.roll)) alpha[2] += -upB[0] * 2.4;
  }
  if (sim.sw['ap.alt'] !== 1) sim.apAlt = null;
  else if (sim.apAlt == null || Math.abs(cmd.heave) > 0.12) sim.apAlt = pose.p[1];
  let wantF = scale([ax, ay, az], m.mass);
  const wantT: V3 = [alpha[0] * m.inertia[0], alpha[1] * m.inertia[1], alpha[2] * m.inertia[2]];
  const gearKey = sim.def.gear?.key;
  const down = gearKey ? Math.max(0, Math.min(1, sim.mover(gearKey))) : 1;
  const feet = (sim.def.gear?.legs ?? []).map((leg) => [leg[0], -sim.def.floorHeight * down, leg[2]] as V3);
  // landing assist: don't arrive faster than a walk once the feet are near the ground
  if (sim.sw['fa.land'] === 1 && down > 0.7 && feet.length) {
    let clearance = Infinity;
    for (const foot of feet) {
      const w = add(pose.p, qRotate(pose.q, foot));
      clearance = Math.min(clearance, w[1] - ground(w[0], w[2]));
    }
    if (clearance > 0.3 && clearance < 25 && pose.v[1] < 0) {
      const limit = clearance < 4 ? 0.7 : 2.4;
      if (pose.v[1] < -limit) {
        const up = qRotate(qInv, [0, m.mass * (-limit - pose.v[1]) * 1.6, 0]);
        wantF = add(wantF, up);
      }
    }
  }
  const rcsOn = sim.sw.rcs !== 0;
  const gated = list.map((t) => {
    if (t.kind !== 'rcs' || !rcsOn) return t.kind === 'rcs' ? { ...t, maxN: 0 } : t;
    const part = sim.def.parts.find((p) => p.id === t.part);
    if (part?.circuit && sim.sys.supply(sim.st, part.circuit) < 0.5) return { ...t, maxN: 0 };
    return t;
  });
  const mix = allocateRcs(gated, m.com, wantF, wantT);
  let F = mains.F;
  // Rear engines lifting the ship produce a huge nose-down moment the RCS cannot fight.
  // With the stabiliser on, that moment is trimmed out and attitude follows the stick.
  let T: V3 = sas || sim.sw['ap.hor'] === 1 ? wantT : mains.T;
  const burn = new Map<string, number>();
  gated.forEach((t, i) => {
    if (mix[i] <= 0) return;
    const f = scale(t.dir, t.maxN * mix[i]);
    F = add(F, f);
    T = add(T, cross(sub(t.at, m.com), f));
    burn.set(t.part, (burn.get(t.part) ?? 0) + mix[i]);
  });
  for (const p of sim.def.parts) if (p.type === 'rcs') sim.st[sim.vars.idx(`${partTag(p)}.use`)] = burn.get(p.id) ?? 0;
  // contact: the wheels while sitting level. The hull corners join in only when the ship is
  // tilted or the gear is up, so a level ship is not rocked by rough ground outside the pad.
  const b = sim.def.bounds;
  const upNow = qRotate(pose.q, [0, 1, 0]);
  const tilted = upNow[1] < 0.96;
  const footY = down > 0.35 ? -sim.def.floorHeight * down : b.min[1];
  const contacts: V3[] = down > 0.35 ? feet.slice() : [];
  if (tilted || down <= 0.35) for (const x of [b.min[0], b.max[0]]) for (const z of [b.min[2], b.max[2]]) contacts.push([x, footY, z]);
  const nContact = Math.max(1, contacts.length);
  const kSpring = (m.mass * FLIGHT.g) / (0.02 * nContact);
  const cSpring = (2.2 * Math.sqrt(kSpring * nContact * m.mass)) / nContact;
  let worldF: V3 = [0, -m.mass * FLIGHT.g, 0];
  let support = 0;
  let deepest = 0;
  for (const foot of contacts) {
    const w = add(pose.p, qRotate(pose.q, foot));
    const pen = ground(w[0], w[2]) - w[1];
    if (pen <= 0) continue;
    if (pen > deepest) deepest = pen;
    const rWorld = qRotate(pose.q, foot);
    const vFoot = add(pose.v, cross(pose.w, rWorld));
    const N = Math.max(0, kSpring * Math.min(pen, 0.15) - cSpring * vFoot[1]);
    if (N <= 0) continue;
    support += N;
    const Fw: V3 = [0, N, 0];
    const vh = Math.hypot(vFoot[0], vFoot[2]);
    if (vh > 0.02) {
      const f = Math.min(0.7 * N, ((vh / dt) * m.mass) / nContact);
      Fw[0] -= (vFoot[0] / vh) * f;
      Fw[2] -= (vFoot[2] / vh) * f;
    }
    worldF = add(worldF, Fw);
    T = add(T, cross(sub(foot, m.com), qRotate(qInv, Fw)));
  }
  // altitude hold: trim whatever the engines are doing so the ship stays at the captured height
  if (sim.apAlt != null) {
    const err = sim.apAlt - pose.p[1];
    const ay = Math.max(-2, Math.min(2, err * 0.45 - pose.v[1] * 1.5));
    const fUp = qRotate(pose.q, F)[1];
    const hold = m.mass * ay - fUp - worldF[1];
    const cap = m.mass * 12;
    worldF[1] += Math.max(-cap, Math.min(cap, hold));
  }
  integrate(pose, m, F, T, worldF, dt);
  // only a real dig is shoved out; the two-centimetre sag of the springs is what the gear rests on
  if (deepest > 0.08) {
    pose.p[1] += deepest - 0.02;
    if (pose.v[1] < 0) pose.v[1] = 0;
  }
  // parked and level, engines quiet: kill the leftover wobble so the hull does not shiver on the pad
  const up = qRotate(pose.q, [0, 1, 0]);
  const slow = Math.hypot(pose.v[0], pose.v[1], pose.v[2]) < 0.15 && Math.hypot(pose.w[0], pose.w[1], pose.w[2]) < 0.05;
  const driving = Math.hypot(mains.F[0], mains.F[1], mains.F[2]) > m.mass;
  if (!driving && down > 0.9 && up[1] > 0.98 && slow && deepest < 0.06) {
    pose.v = [0, 0, 0];
    pose.w = [0, 0, 0];
  }
  const supported = support > m.mass * FLIGHT.g * 0.35 && down > 0.75 && pose.v[1] < 1.2 && pose.v[1] > -2;
  if (supported) sim.landed = true;
  else if (support < m.mass * FLIGHT.g * 0.08 || down < 0.4 || pose.v[1] > 0.6) sim.landed = false;
  const dx = pose.p[0] - sim.place.x;
  const dz = pose.p[2] - sim.place.z;
  sim.onPad = sim.landed && dx * dx + dz * dz < 25 * 25;
}
