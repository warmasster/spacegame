// One flight step of a ship, on whoever flies it (the pilot's client, the server when nobody is at
// the helm, the offline client). In order:
//
//   1. what every thruster can give (ship state) and the envelope it adds up to
//   2. the flight computer: stick + assists + autopilot → wanted force and torque, main engines
//   3. the allocator shares what the main engines don't cover among the pads, then the RCS
//   4. thrusters answer with their own lag; their real wrench is what moves the ship
//   5. gravity and ground contact (substeps near the ground), rigid-body integration
//   6. landed / on the pad / asleep, and each thruster's output written to the state table
//      (`<engine>.thr`, `<pad>.use`, `<rcs>.use`): the systems burn propellant from it and every
//      client draws the plumes from it.
//
// A parked ship that nothing pushes goes to sleep: its pose freezes, so it never shivers on its
// springs, and it wakes the moment the pilot, the autopilot or the ground under it asks. Asleep, a
// step costs a handful of comparisons (a server with fifty parked ships barely notices them).
//
// The step runs at 60 Hz for every ship its machine flies, so it keeps its working vectors in
// preallocated arrays: it must not leave garbage behind.

import type { V3 } from '../geom.js';
import type { ShipSim } from '../sim.js';
import type { Vent } from '../airflow.js';
import { Allocator } from './allocate.js';
import { integrateBody } from './body.js';
import { ContactModel, type Ground } from './contact.js';
import { fcsPowered, FlightComputer, isIdle, throttleOf, type Envelope, type FcsOut, type FlightCommand } from './fcs.js';
import { emptyMass, massProperties, type Lump, type MassProps } from './mass.js';
import { compassHeading, type ShipPose } from './pose.js';
import { ThrusterSet } from './thrusters.js';
import { altitudeOf, baseFix, baseFrom, bodyAt, frameAt, gravityAt, localBase, localFrame, orbitInfo, orbitInto, type CelestialBody, type LocalFrame } from '../../space/body.js';
import type { BodySurface } from '../../space/surface.js';
import { SurfaceGround } from '../../space/tangent.js';

export const FLIGHT = {
  /** Lunar surface gravity (m/s²). */
  g: 1.62,
  /** Standard gravity for specific impulse. */
  g0: 9.80665,
  /** A parked ship within this distance of where it was placed can refuel (m). */
  padRadius: 25,
};

export interface FlightEnv {
  /**
   * The ground: the whole surface of the body it flies round, with every modifier on it (the base's
   * pads, craters: space/body.ts `surfaceOf`). The same everywhere; without it, a smooth sphere.
   */
  surface?: BodySurface | null;
  /** Mass aboard that the ship data doesn't know: crew, crates (ship space). */
  extra?: readonly Lump[];
  /** The body it flies around (default: the one ruling where it is). */
  body?: CelestialBody;
}

/**
 * Orbital regime (with some hysteresis): above this horizontal speed (m/s) or height above the
 * sphere (m) the flight computer stops holding a position or a height (that would fight the orbit):
 * the throttle is the main engines, the stick is acceleration, the autopilot only levels and turns.
 */
const ORBITAL_IN_V = 150;
const ORBITAL_OUT_V = 100;
const ORBITAL_IN_ALT = 15000;
const ORBITAL_OUT_ALT = 12000;

/** No ground: contact forces away from it (high up, or a body without surface data). */
const NO_CONTACT = { F: [0, 0, 0] as V3, T: [0, 0, 0] as V3, feet: 0, hull: 0, load: 0, deepest: 0 };

/** Quaternion of the rotation whose matrix has rows a, b, c (three.js's algorithm) into `out`. */
function quatFromRows(a: V3, b: V3, c: V3, out: number[]) {
  const m00 = a[0];
  const m01 = a[1];
  const m02 = a[2];
  const m10 = b[0];
  const m11 = b[1];
  const m12 = b[2];
  const m20 = c[0];
  const m21 = c[1];
  const m22 = c[2];
  const tr = m00 + m11 + m22;
  if (tr > 0) {
    const s = 0.5 / Math.sqrt(tr + 1);
    out[3] = 0.25 / s;
    out[0] = (m21 - m12) * s;
    out[1] = (m02 - m20) * s;
    out[2] = (m10 - m01) * s;
  } else if (m00 > m11 && m00 > m22) {
    const s = 2 * Math.sqrt(1 + m00 - m11 - m22);
    out[3] = (m21 - m12) / s;
    out[0] = 0.25 * s;
    out[1] = (m01 + m10) / s;
    out[2] = (m02 + m20) / s;
  } else if (m11 > m22) {
    const s = 2 * Math.sqrt(1 + m11 - m00 - m22);
    out[3] = (m02 - m20) / s;
    out[0] = (m01 + m10) / s;
    out[1] = 0.25 * s;
    out[2] = (m12 + m21) / s;
  } else {
    const s = 2 * Math.sqrt(1 + m22 - m00 - m11);
    out[3] = (m10 - m01) / s;
    out[0] = (m02 + m20) / s;
    out[1] = (m12 + m21) / s;
    out[2] = 0.25 * s;
  }
}

export interface FlightTelemetry {
  /** Height of the feet / hull above the ground (m), vertical and ground speed (m/s), compass heading (rad). */
  agl: number;
  vs: number;
  gs: number;
  heading: number;
  direct: boolean;
  /** Autopilot annunciations. */
  modes: string[];
  /** Thrust being made (N) and the most the thrusters could make straight up (N). */
  thrust: number;
  lift: number;
  weight: number;
  throttle: number;
  targetV: V3 | null;
  targetVz: number | null;
  sleeping: boolean;
}

/** Air jets stronger than this (N, all breaches together) wake a parked ship: the blast shoves it on its gear. */
const JET_WAKE_N = 400;
/** Asleep, the air leaving through breaches is checked every this many steps (it is not free). */
const VENT_CHECK_STEPS = 6;

/** Weight of the torque rows against the force rows in the allocator (both as accelerations). */
const ATT_PRIORITY = 400;

const RCS_DIRS: V3[] = [
  [1, 0, 0],
  [-1, 0, 0],
  [0, 1, 0],
  [0, -1, 0],
  [0, 0, 1],
  [0, 0, -1],
];

/** Total push of the jets going out of the ship (downstream = vacuum), N. */
function outJets(vents: readonly Vent[]) {
  let n = 0;
  for (const j of vents) if (j.down < 0 && j.thrust > 1) n += j.thrust;
  return n;
}

// scratch vector (single-threaded)
const _q: V3 = [0, 0, 0];

/** q ⊗ (x, y, z) into `out`. */
function rot(q: readonly number[], x: number, y: number, z: number, out: V3) {
  const qx = q[0];
  const qy = q[1];
  const qz = q[2];
  const qw = q[3];
  const tx = 2 * (qy * z - qz * y);
  const ty = 2 * (qz * x - qx * z);
  const tz = 2 * (qx * y - qy * x);
  out[0] = x + qw * tx + (qy * tz - qz * ty);
  out[1] = y + qw * ty + (qz * tx - qx * tz);
  out[2] = z + qw * tz + (qx * ty - qy * tx);
  return out;
}

export class FlightModel {
  readonly thrusters: ThrusterSet;
  readonly contact: ContactModel;
  readonly fcs = new FlightComputer();
  private alloc = new Allocator();
  private allocRcs = new Allocator();
  /** What each thruster can give, 0..1 (order of `thrusters.list`). */
  readonly avail: Float64Array;
  /** Output of each thruster as a fraction of full thrust (RCS: sum of its six nozzles). */
  readonly out: Float64Array;
  /** RCS nozzles (6 per block: +x −x +y −y +z −z) and pad vectors (axial, side a, side b). */
  readonly rcsOut: Float64Array;
  readonly liftVec: Float64Array;
  private rcsCmd: Float64Array;
  private liftCmd: Float64Array;
  sleeping = false;
  private still = 0;
  private sleepClear = 0;
  private sleepCheck = 0;
  private ventCheck = 0;
  /** Contacts touched last step (substeps near the ground). */
  private touching = false;
  agl = 0;
  /** World acceleration of the centre of mass during the last step (m/s²). */
  readonly accel: V3 = [0, 0, 0];
  /** Gravity at the centre of mass (world, m/s²), its size, the local frame there, height above the sphere. */
  readonly gravity: V3 = [0, -FLIGHT.g, 0];
  gMag = FLIGHT.g;
  readonly frame: LocalFrame = localFrame();
  alt = 0;
  /** Orbital regime (see ORBITAL_IN_V): no position or height holds. */
  orbital = false;
  /** The pose in the local frame (up = +y, north = −z) the flight computer flies in; reused. */
  private lpose: ShipPose = { p: [0, 0, 0], q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0] };
  private qL = [0, 0, 0, 1];
  private _negN: V3 = [0, 0, 1];
  private _pc: V3 = [0, 0, 0];
  /** The orbit and the base as the orbital autopilot modes see them (refilled while the autopilot is on). */
  private orb = orbitInfo();
  /** The ground: the body's surface in a tangent frame under the ship (the contact model works in it). */
  private sg: SurfaceGround | null = null;
  private poseL: ShipPose = { p: [0, 0, 0], q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0] };
  private cF: V3 = [0, 0, 0];
  private cT: V3 = [0, 0, 0];
  /** Pose at the start of the step (position, orientation): the safety net's. */
  private saved = new Float64Array(7);
  private bfix = baseFix();
  last: FcsOut | null = null;
  telemetry: FlightTelemetry = { agl: 0, vs: 0, gs: 0, heading: 0, direct: false, modes: [], thrust: 0, lift: 0, weight: 0, throttle: 0, targetV: null, targetVz: null, sleeping: false };
  /** The last mass properties (read by the displays). Filled in place every step. */
  mp: MassProps;
  // working arrays of the step
  private envl: Envelope = { pos: [0, 0, 0], neg: [0, 0, 0], torque: [0, 0, 0] };
  private mainDir: V3 = [0, 0, -1];
  private want = new Float64Array(6);
  private rest = new Float64Array(6);
  private W = new Float64Array(6);
  private Fb: V3 = [0, 0, 0];
  private Tb: V3 = [0, 0, 0];
  private Fw: V3 = [0, 0, 0];
  private Tw: V3 = [0, 0, 0];
  private Ft: V3 = [0, 0, 0];
  private Tt: V3 = [0, 0, 0];
  private com: V3 = [0, 0, 0];

  /** Jets through the openings, refilled every step. */
  private ventBuf: Vent[] = [];

  constructor(private sim: ShipSim) {
    this.thrusters = new ThrusterSet(sim.def);
    this.mp = massProperties(sim.def, undefined, undefined, emptyMass());
    this.contact = new ContactModel(sim.def, this.mp.mass);
    const n = this.thrusters.list.length;
    this.avail = new Float64Array(n);
    this.out = new Float64Array(n);
    this.rcsOut = new Float64Array(this.thrusters.rcs.length * 6);
    this.rcsCmd = new Float64Array(this.thrusters.rcs.length * 6);
    this.liftVec = new Float64Array(this.thrusters.lifts.length * 3);
    this.liftCmd = new Float64Array(this.thrusters.lifts.length * 3);
  }

  /** Gear travel (0 up … 1 down); ships without gear rest on the hull. */
  gear() {
    const k = this.sim.def.gear?.key;
    return k ? this.sim.mover(k) : 0;
  }

  /** Something outside the flight moved the ship or the ground (a blast, a crater): wake up. */
  wake() {
    this.sleeping = false;
    this.still = 0;
  }

  /**
   * Asleep: does it stay asleep this step? Cheap checks first; true = nothing to do. False when
   * something woke it (it is awake now) or when only the full step can tell (the autopilot or the
   * throttle might push: the full step decides and may put it straight back to sleep).
   */
  private stillAsleep(dt: number, cmd: FlightCommand, env: FlightEnv): boolean {
    const sim = this.sim;
    const gear = this.gear();
    // the ground under it moved (a crater): measured twice a second
    this.sleepCheck -= dt;
    if (this.sleepCheck <= 0) {
      this.sleepCheck = 0.5;
      this.agl = this.clearanceNow(env, gear);
      if (Math.abs(this.agl - this.sleepClear) > 0.02) {
        this.wake();
        return false;
      }
      this.refreshTelemetry();
    }
    // air rushing out of a breach shoves it
    if (--this.ventCheck <= 0) {
      this.ventCheck = VENT_CHECK_STEPS;
      if (outJets(sim.vents(this.ventBuf)) > JET_WAKE_N) {
        this.wake();
        return false;
      }
    }
    if (!isIdle(cmd) || !(gear > 0.95)) {
      this.wake();
      return false;
    }
    const sw = sim.sw;
    // only the flight computer knows whether the autopilot wants to climb, or the lever to push
    if (sim.def.autopilot && sw['ap.on'] === 1) return false;
    if (throttleOf(sim) > 0.02 && (sw['fa.hold'] === 0 || !fcsPowered(sim))) return false;
    return true;
  }

  step(dt: number, cmd: FlightCommand, env: FlightEnv) {
    if (this.sleeping && this.stillAsleep(dt, cmd, env)) return;
    const sim = this.sim;
    const pose = sim.pose;
    const saved = this.saved;
    for (let i = 0; i < 3; i++) saved[i] = pose.p[i];
    for (let i = 0; i < 4; i++) saved[3 + i] = pose.q[i];
    const T = this.thrusters;
    T.update(sim);
    T.availability(sim, this.avail);
    const mp = sim.massNow(env.extra, this.mp);
    this.contact.setMass(mp.mass);
    const gear = this.gear();
    // ---- where it is: the body's pull and the local frame at the centre of mass -------------------------
    const body = env.body ?? bodyAt(pose.p);
    const pc = rot(pose.q, mp.com[0], mp.com[1], mp.com[2], this._pc);
    pc[0] += pose.p[0];
    pc[1] += pose.p[1];
    pc[2] += pose.p[2];
    const g = gravityAt(body, pc, this.gravity);
    this.gMag = Math.sqrt(g[0] * g[0] + g[1] * g[1] + g[2] * g[2]);
    const frame = frameAt(body, pc, this.frame);
    this.alt = altitudeOf(body, pc);
    // the body's surface (or its bare sphere), seen from a tangent frame under the ship
    const sg = this.sphereGround(env.surface, body);
    this.agl = this.clearanceNow(env, gear);
    const up = frame.up;
    const vUp = pose.v[0] * up[0] + pose.v[1] * up[1] + pose.v[2] * up[2];
    const vh2 = Math.max(0, pose.v[0] * pose.v[0] + pose.v[1] * pose.v[1] + pose.v[2] * pose.v[2] - vUp * vUp);
    const vh = Math.sqrt(vh2);
    if (this.orbital) {
      if (vh < ORBITAL_OUT_V && this.alt < ORBITAL_OUT_ALT) this.orbital = false;
    } else if (vh > ORBITAL_IN_V || this.alt > ORBITAL_IN_ALT) this.orbital = true;
    // the weight the thrusters carry: gravity less what flying round the body takes off it
    const gEff = Math.max(0, this.gMag - vh2 / (this.alt + body.radius));
    const lp = this.localPose(pose, body);

    // ---- envelope ------------------------------------------------------------------------------------
    const com = mp.com;
    const envl = this.envl;
    envl.pos[0] = envl.pos[1] = envl.pos[2] = 0;
    envl.neg[0] = envl.neg[1] = envl.neg[2] = 0;
    envl.torque[0] = envl.torque[1] = envl.torque[2] = 0;
    const addForce = this.addForce;
    const nm = T.mains.length;
    const nl = T.lifts.length;
    let mainCap = 0;
    let mdx = 0;
    let mdy = 0;
    let mdz = 0;
    for (let i = 0; i < nm; i++) {
      const t = T.mains[i];
      const c = this.avail[i] * t.maxN;
      mainCap += c;
      mdx += t.dir[0] * c;
      mdy += t.dir[1] * c;
      mdz += t.dir[2] * c;
    }
    const md = this.mainDir;
    if (mainCap > 0) {
      const l = Math.sqrt(mdx * mdx + mdy * mdy + mdz * mdz) || 1;
      md[0] = mdx / l;
      md[1] = mdy / l;
      md[2] = mdz / l;
    } else {
      md[0] = 0;
      md[1] = 0;
      md[2] = -1;
    }
    for (let i = 0; i < nl; i++) {
      const t = T.lifts[i];
      const c = this.avail[nm + i] * t.maxN;
      if (c <= 0) continue;
      const side = ((c * t.cone) / Math.sqrt(1 + t.cone * t.cone)) * 0.5;
      const e1 = t.e1!;
      const e2 = t.e2!;
      addForce(envl, com, t.dir[0] * c, t.dir[1] * c, t.dir[2] * c, t.at);
      addForce(envl, com, e1[0] * side, e1[1] * side, e1[2] * side, t.at);
      addForce(envl, com, -e1[0] * side, -e1[1] * side, -e1[2] * side, t.at);
      addForce(envl, com, e2[0] * side, e2[1] * side, e2[2] * side, t.at);
      addForce(envl, com, -e2[0] * side, -e2[1] * side, -e2[2] * side, t.at);
    }
    for (let i = 0; i < T.rcs.length; i++) {
      const t = T.rcs[i];
      const c = this.avail[nm + nl + i] * t.maxN;
      if (c <= 0) continue;
      for (const d of RCS_DIRS) addForce(envl, com, d[0] * c, d[1] * c, d[2] * c, t.at);
    }

    // ---- flight computer --------------------------------------------------------------------------------
    const landedBefore = sim.landed;
    const apOn = !!sim.def.autopilot && sim.sw['ap.on'] === 1;
    orbitInto(body, pc, pose.v, this.orb);
    if (apOn) localBase(baseFrom(body, pc, pose.v, this.bfix), frame);
    const fc = this.fcs.compute({
      sim,
      pose: lp,
      mp,
      env: envl,
      cmd,
      dt,
      agl: this.agl,
      landed: landedBefore,
      gear,
      mainCap,
      mainDir: md,
      g: gEff,
      orbital: this.orbital,
      qWorld: pose.q,
      touching: this.touching,
      alt: this.alt,
      orbit: this.orb,
      base: apOn ? this.bfix : null,
      body,
    });
    this.last = fc;

    // ---- air leaving through breaches and open doors: a thrust at each opening (../airflow.ts) ----------
    const vents = sim.vents(this.ventBuf);
    const jetN = outJets(vents);

    // ---- asleep on the pad (only the autopilot or the lever could be pushing: see stillAsleep) ---------
    if (this.sleeping) {
      const pushing = fc.climbing || (fc.main > 0.02 && mainCap > 0) || !isIdle(cmd) || jetN > JET_WAKE_N;
      if (!pushing && gear > 0.95) {
        this.out.fill(0);
        this.rcsOut.fill(0);
        this.liftVec.fill(0);
        this.publish(fc, mp);
        return;
      }
      this.wake();
    }

    // ---- main engines: what they do now, the fast thrusters cover the rest ------------------------------
    let fmx = 0;
    let fmy = 0;
    let fmz = 0;
    let tmx = 0;
    let tmy = 0;
    let tmz = 0;
    for (let i = 0; i < nm; i++) {
      const t = T.mains[i];
      const k = this.out[i] * t.maxN;
      const fx = t.dir[0] * k;
      const fy = t.dir[1] * k;
      const fz = t.dir[2] * k;
      const rx = t.at[0] - com[0];
      const ry = t.at[1] - com[1];
      const rz = t.at[2] - com[2];
      fmx += fx;
      fmy += fy;
      fmz += fz;
      tmx += ry * fz - rz * fy;
      tmy += rz * fx - rx * fz;
      tmz += rx * fy - ry * fx;
    }
    // two stages: the pads (efficient, strong) take the whole demand; the RCS covers what they
    // cannot give (horizontal force beyond their cones, torque when they are saturated or out)
    // the main engines' force counts toward the demand unless it is the pilot's own (decoupled);
    // their torque is always trimmed out, except with no flight computer at all
    const fm = fc.mainFree ? 0 : 1;
    const tm = fc.direct ? 0 : 1;
    const want = this.want;
    want[0] = fc.F[0] - fmx * fm;
    want[1] = fc.F[1] - fmy * fm;
    want[2] = fc.F[2] - fmz * fm;
    want[3] = fc.T[0] - tmx * tm;
    want[4] = fc.T[1] - tmy * tm;
    want[5] = fc.T[2] - tmz * tm;
    // rows in acceleration units; attitude first (a ship that cannot hold its attitude cannot do
    // anything else), then height, then the horizontal
    const m2 = mp.mass * mp.mass;
    const W = this.W;
    W[0] = 1 / m2;
    W[1] = 3 / m2;
    W[2] = 1 / m2;
    W[3] = ATT_PRIORITY / (mp.inertia[0] * mp.inertia[0]);
    W[4] = ATT_PRIORITY / (mp.inertia[1] * mp.inertia[1]);
    W[5] = ATT_PRIORITY / (mp.inertia[2] * mp.inertia[2]);
    const al = this.alloc;
    al.begin(nl * 3);
    for (let i = 0; i < nl; i++) {
      const t = T.lifts[i];
      const rx = t.at[0] - com[0];
      const ry = t.at[1] - com[1];
      const rz = t.at[2] - com[2];
      for (let k = 0; k < 3; k++) {
        const d = k === 0 ? t.dir : k === 1 ? t.e1! : t.e2!;
        const fx = d[0] * t.maxN;
        const fy = d[1] * t.maxN;
        const fz = d[2] * t.maxN;
        al.column6(i * 3 + k, fx, fy, fz, ry * fz - rz * fy, rz * fx - rx * fz, rx * fy - ry * fx);
      }
      al.cone(i * 3, this.avail[nm + i], t.cone);
    }
    const u = al.solve(want, W, 1e-3, 40);
    for (let i = 0; i < nl * 3; i++) this.liftCmd[i] = u[i];
    // RCS: one signed variable per block axis (the + or the − nozzle fires, never both)
    const ar = this.allocRcs;
    ar.begin(T.rcs.length * 3);
    for (let i = 0; i < T.rcs.length; i++) {
      const t = T.rcs[i];
      const rx = t.at[0] - com[0];
      const ry = t.at[1] - com[1];
      const rz = t.at[2] - com[2];
      const f = t.maxN;
      // unit force along axis k at `at`: torque r × (f e_k)
      ar.column6(i * 3, f, 0, 0, 0, rz * f, -ry * f);
      ar.column6(i * 3 + 1, 0, f, 0, -rz * f, 0, rx * f);
      ar.column6(i * 3 + 2, 0, 0, f, ry * f, -rx * f, 0);
      for (let k = 0; k < 3; k++) ar.box(i * 3 + k, this.avail[nm + nl + i], true);
    }
    const rest = this.rest;
    for (let k = 0; k < 6; k++) rest[k] = want[k] - al.got[k];
    const ur = ar.solve(rest, W, 3e-3, 40);
    for (let i = 0; i < T.rcs.length; i++) {
      for (let k = 0; k < 3; k++) {
        const v = ur[i * 3 + k];
        this.rcsCmd[i * 6 + k * 2] = Math.max(0, v);
        this.rcsCmd[i * 6 + k * 2 + 1] = Math.max(0, -v);
      }
    }

    // ---- thruster response ----------------------------------------------------------------------------
    for (let i = 0; i < nm; i++) {
      const t = T.mains[i];
      // the lever is a share of what the engine gives now (overdrive: above its rating)
      const a = this.avail[i];
      const target = a > 1 ? fc.main * a : Math.min(fc.main, a);
      this.out[i] += (target - this.out[i]) * (1 - Math.exp(-dt / Math.max(1e-3, t.tau)));
      if (this.avail[i] <= 0) this.out[i] = 0;
    }
    for (let i = 0; i < nl; i++) {
      const t = T.lifts[i];
      const k = 1 - Math.exp(-dt / Math.max(1e-3, t.tau));
      for (let c = 0; c < 3; c++) this.liftVec[i * 3 + c] += (this.liftCmd[i * 3 + c] - this.liftVec[i * 3 + c]) * k;
      if (this.avail[nm + i] <= 0) this.liftVec.fill(0, i * 3, i * 3 + 3);
      const a = this.liftVec[i * 3];
      const b = this.liftVec[i * 3 + 1];
      const c = this.liftVec[i * 3 + 2];
      this.out[nm + i] = Math.sqrt(a * a + b * b + c * c);
    }
    for (let i = 0; i < T.rcs.length; i++) {
      const t = T.rcs[i];
      const k = 1 - Math.exp(-dt / Math.max(1e-3, t.tau));
      let sum = 0;
      for (let c = 0; c < 6; c++) {
        const o = i * 6 + c;
        this.rcsOut[o] += (this.rcsCmd[o] - this.rcsOut[o]) * k;
        if (this.avail[nm + nl + i] <= 0) this.rcsOut[o] = 0;
        sum += this.rcsOut[o];
      }
      this.out[nm + nl + i] = sum;
    }

    // ---- the real wrench (ship axes → world) -----------------------------------------------------------------
    const Fb = this.Fb;
    const Tb = this.Tb;
    Fb[0] = fmx;
    Fb[1] = fmy;
    Fb[2] = fmz;
    Tb[0] = tmx;
    Tb[1] = tmy;
    Tb[2] = tmz;
    const push = this.push;
    for (let i = 0; i < nl; i++) {
      const t = T.lifts[i];
      const e1 = t.e1!;
      const e2 = t.e2!;
      const a = this.liftVec[i * 3] * t.maxN;
      const b = this.liftVec[i * 3 + 1] * t.maxN;
      const c = this.liftVec[i * 3 + 2] * t.maxN;
      push(Fb, Tb, com, t.dir[0] * a + e1[0] * b + e2[0] * c, t.dir[1] * a + e1[1] * b + e2[1] * c, t.dir[2] * a + e1[2] * b + e2[2] * c, t.at);
    }
    for (let i = 0; i < T.rcs.length; i++) {
      const t = T.rcs[i];
      for (let c = 0; c < 6; c++) {
        const o = this.rcsOut[i * 6 + c] * t.maxN;
        if (o > 0) push(Fb, Tb, com, RCS_DIRS[c][0] * o, RCS_DIRS[c][1] * o, RCS_DIRS[c][2] * o, t.at);
      }
    }
    for (const j of vents) if (j.down < 0 && j.thrust > 1) push(Fb, Tb, com, -j.dir[0] * j.thrust, -j.dir[1] * j.thrust, -j.dir[2] * j.thrust, j.at);
    const Fw = rot(pose.q, Fb[0], Fb[1], Fb[2], this.Fw);
    const Tw = rot(pose.q, Tb[0], Tb[1], Tb[2], this.Tw);

    // ---- gravity, ground, integration --------------------------------------------------------------------------
    const comW0 = rot(pose.q, com[0], com[1], com[2], this.com);
    const w = pose.w;
    const v0x = pose.v[0] + w[1] * comW0[2] - w[2] * comW0[1];
    const v0y = pose.v[1] + w[2] * comW0[0] - w[0] * comW0[2];
    const v0z = pose.v[2] + w[0] * comW0[1] - w[1] * comW0[0];
    const near = !!sg && (this.touching || this.agl < 2.5);
    const nSub = near ? 4 : 1;
    const h = dt / nSub;
    const brake = landedBefore && isIdle(cmd) && !fc.climbing;
    let feet = 0;
    let hull = 0;
    let load = 0;
    let deepest = 0;
    const Ft = this.Ft;
    const Tt = this.Tt;
    for (let s = 0; s < nSub; s++) {
      let cF: readonly number[] = NO_CONTACT.F;
      let cT: readonly number[] = NO_CONTACT.T;
      let cr: { feet: number; hull: number; load: number; deepest: number } = NO_CONTACT;
      if (sg && near) {
        // the same contact model in the tangent frame under the ship, its forces back to the world
        const r = this.contact.forces(sg.poseIn(pose, this.poseL), mp, sg, gear, h, brake);
        cr = r;
        cF = sg.vecOut(r.F, this.cF);
        cT = sg.vecOut(r.T, this.cT);
      }
      feet = cr.feet;
      hull = cr.hull;
      load = cr.load;
      deepest = cr.deepest;
      Ft[0] = Fw[0] + cF[0] + mp.mass * g[0];
      Ft[1] = Fw[1] + cF[1] + mp.mass * g[1];
      Ft[2] = Fw[2] + cF[2] + mp.mass * g[2];
      Tt[0] = Tw[0] + cT[0];
      Tt[1] = Tw[1] + cT[1];
      Tt[2] = Tw[2] + cT[2];
      integrateBody(pose, mp, Ft, Tt, h);
    }
    // safety net: a step that blew up (a crash far past what the contact model holds) puts the ship
    // back where it was, stopped, instead of losing it to NaN
    const v2 = pose.v[0] * pose.v[0] + pose.v[1] * pose.v[1] + pose.v[2] * pose.v[2];
    const w2 = pose.w[0] * pose.w[0] + pose.w[1] * pose.w[1] + pose.w[2] * pose.w[2];
    if (!Number.isFinite(pose.p[0] + pose.p[1] + pose.p[2] + pose.q[0] + pose.q[1] + pose.q[2] + pose.q[3] + v2 + w2) || v2 > 2e4 * 2e4 || w2 > 50 * 50) {
      for (let i = 0; i < 3; i++) pose.p[i] = saved[i];
      for (let i = 0; i < 4; i++) pose.q[i] = saved[3 + i];
      pose.v.fill(0);
      pose.w.fill(0);
    }
    this.touching = feet + hull > 0;
    // a real dig (a crash, a crater opening under it) is pushed out rather than left inside
    if (deepest > 0.35) {
      // out along the local vertical (the world's y at the base)
      const k = deepest - 0.35;
      pose.p[0] += up[0] * k;
      pose.p[1] += up[1] * k;
      pose.p[2] += up[2] * k;
      const vr = pose.v[0] * up[0] + pose.v[1] * up[1] + pose.v[2] * up[2];
      if (vr < 0) {
        pose.v[0] -= up[0] * vr;
        pose.v[1] -= up[1] * vr;
        pose.v[2] -= up[2] * vr;
      }
    }
    // a body with no surface data is its smooth sphere: the ship rests on it (feet at the radius),
    // never sinks into it, and slides to a stop on it
    let sphereRest = false;
    if (!sg) {
      const below = sim.def.floorHeight - altitudeOf(body, pose.p);
      if (below > 0) {
        pose.p[0] += up[0] * below;
        pose.p[1] += up[1] * below;
        pose.p[2] += up[2] * below;
        const vr = pose.v[0] * up[0] + pose.v[1] * up[1] + pose.v[2] * up[2];
        if (vr < 0) {
          pose.v[0] -= up[0] * vr;
          pose.v[1] -= up[1] * vr;
          pose.v[2] -= up[2] * vr;
        }
        const k = Math.exp(-dt * 2);
        for (let i = 0; i < 3; i++) {
          pose.v[i] *= k;
          pose.w[i] *= k;
        }
        sphereRest = vr > -1.5;
      }
    }
    const comW1 = rot(pose.q, com[0], com[1], com[2], _q);
    this.accel[0] = (pose.v[0] + w[1] * comW1[2] - w[2] * comW1[1] - v0x) / dt;
    this.accel[1] = (pose.v[1] + w[2] * comW1[0] - w[0] * comW1[2] - v0y) / dt;
    this.accel[2] = (pose.v[2] + w[0] * comW1[1] - w[1] * comW1[0] - v0z) / dt;

    // ---- landed, pad, sleep ---------------------------------------------------------------------------------------
    const weight = mp.mass * this.gMag;
    const legs = this.contact.legs.length;
    const onFeet = legs > 0 && feet >= Math.max(2, legs - 1) && gear > 0.9;
    const vUpNow = pose.v[0] * up[0] + pose.v[1] * up[1] + pose.v[2] * up[2];
    // resting: on its feet (or its belly), or at rest on uneven ground with two feet or more taking most of the weight
    const vv0 = pose.v[0] * pose.v[0] + pose.v[1] * pose.v[1] + pose.v[2] * pose.v[2];
    const resting = feet >= 2 && load > weight * 0.6 && vv0 < 0.15 * 0.15 && gear > 0.9;
    if (sphereRest) sim.landed = true;
    else if (((onFeet || hull >= 3) && load > weight * 0.4 && Math.abs(vUpNow) < 1.5) || resting) sim.landed = true;
    else if (load < weight * 0.1) sim.landed = false;
    const dx = pose.p[0] - sim.place.p[0];
    const dy = pose.p[1] - sim.place.p[1];
    const dz = pose.p[2] - sim.place.p[2];
    sim.onPad = sim.landed && dx * dx + dy * dy + dz * dz < FLIGHT.padRadius * FLIGHT.padRadius;
    const vv = pose.v[0] * pose.v[0] + pose.v[1] * pose.v[1] + pose.v[2] * pose.v[2];
    const ww = w[0] * w[0] + w[1] * w[1] + w[2] * w[2];
    const quiet = vv < 0.04 * 0.04 && ww < 0.012 * 0.012;
    let idleThrust = true;
    for (let i = 0; i < this.out.length; i++) if (!(this.out[i] < 0.02)) idleThrust = false;
    if (sim.landed && quiet && idleThrust && isIdle(cmd) && !fc.climbing) this.still += dt;
    else this.still = 0;
    if (this.still > 0.8) {
      this.sleeping = true;
      this.still = 0;
      pose.v.fill(0);
      pose.w.fill(0);
      this.sleepClear = this.clearanceNow(env, gear);
      this.sleepCheck = 0.5;
      this.ventCheck = VENT_CHECK_STEPS;
      this.out.fill(0);
      this.rcsOut.fill(0);
      this.liftVec.fill(0);
    }
    this.publish(fc, mp);
  }

  /** The body's surface as a contact ground (one per surface; null without surface data). */
  private sphereGround(surface: BodySurface | null | undefined, body: CelestialBody): SurfaceGround | null {
    if (!surface) return null;
    if (!this.sg || this.sg.surface !== surface || this.sg.body !== body) this.sg = new SurfaceGround(body, surface);
    return this.sg;
  }

  /**
   * The gear feet as drawn (ship space), on the ground under the ship at `pose` (the body's surface,
   * wherever it is): a landed ship stands on its compressed legs anywhere.
   */
  feetAnywhere(pose: ShipPose, surface: BodySurface | null, gear: number, out: V3[] = []): V3[] {
    const body = bodyAt(pose.p);
    const sg = this.sphereGround(surface, body);
    const alt = altitudeOf(body, pose.p);
    if (!sg || alt > sg.surface.highest + 200) return this.contact.feet(pose, NO_GROUND, gear, out);
    sg.anchor(pose.p);
    return this.contact.feet(sg.poseIn(pose, this.poseF), sg, gear, out);
  }
  private poseF: ShipPose = { p: [0, 0, 0], q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0] };

  /** Height of the gear (or hull bottom) over whatever ground is under the ship now (m). */
  private clearanceNow(env: FlightEnv, gear: number): number {
    return this.clearanceAnywhere(env.surface ?? null, gear, env.body);
  }

  /**
   * Height of the gear feet (or the hull bottom) over the ground under the ship now (m), on any
   * machine (the displays of a ship someone else flies ask here too).
   */
  clearanceAnywhere(surface: BodySurface | null, gear = this.gear(), b?: CelestialBody): number {
    const pose = this.sim.pose;
    const body = b ?? bodyAt(pose.p);
    const sg = this.sphereGround(surface, body);
    if (!sg) return altitudeOf(body, pose.p) - this.sim.def.floorHeight;
    // high above it the contact model's probes aren't worth it: the surface right below will do
    const alt = altitudeOf(body, pose.p);
    if (alt - this.sim.def.floorHeight > sg.surface.highest + 500) {
      const c = body.center;
      const l = Math.hypot(pose.p[0] - c[0], pose.p[1] - c[1], pose.p[2] - c[2]);
      const d = this.poseL.p;
      d[0] = (pose.p[0] - c[0]) / l;
      d[1] = (pose.p[1] - c[1]) / l;
      d[2] = (pose.p[2] - c[2]) / l;
      return alt - sg.surface.height(d, 50) - this.sim.def.floorHeight;
    }
    sg.anchor(pose.p);
    return this.contact.clearance(sg.poseIn(pose, this.poseL), sg, gear);
  }

  /**
   * The pose in the local frame at the centre of mass (x east, y up, z south): what the flight
   * computer flies in, so "up", "level" and the compass work anywhere round the body. Its y is the
   * height above the sphere (x and z are the world's and mean nothing; the waypoints are measured
   * from the world position).
   */
  private localPose(pose: ShipPose, body: CelestialBody): ShipPose {
    const f = this.frame;
    const n = this._negN;
    n[0] = -f.north[0];
    n[1] = -f.north[1];
    n[2] = -f.north[2];
    const qL = this.qL;
    quatFromRows(f.east, f.up, n, qL);
    const lp = this.lpose;
    // q_local = qL ⊗ q
    const ax = qL[0];
    const ay = qL[1];
    const az = qL[2];
    const aw = qL[3];
    const bx = pose.q[0];
    const by = pose.q[1];
    const bz = pose.q[2];
    const bw = pose.q[3];
    lp.q[0] = aw * bx + ax * bw + ay * bz - az * by;
    lp.q[1] = aw * by - ax * bz + ay * bw + az * bx;
    lp.q[2] = aw * bz + ax * by - ay * bx + az * bw;
    lp.q[3] = aw * bw - ax * bx - ay * by - az * bz;
    const v = pose.v;
    const w = pose.w;
    lp.v[0] = v[0] * f.east[0] + v[1] * f.east[1] + v[2] * f.east[2];
    lp.v[1] = v[0] * f.up[0] + v[1] * f.up[1] + v[2] * f.up[2];
    lp.v[2] = v[0] * n[0] + v[1] * n[1] + v[2] * n[2];
    lp.w[0] = w[0] * f.east[0] + w[1] * f.east[1] + w[2] * f.east[2];
    lp.w[1] = w[0] * f.up[0] + w[1] * f.up[1] + w[2] * f.up[2];
    lp.w[2] = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
    lp.p[0] = pose.p[0];
    lp.p[1] = altitudeOf(body, pose.p);
    lp.p[2] = pose.p[2];
    return lp;
  }

  /** Add a force (ship axes) at `at` to the envelope: per-axis push each way, torque magnitude per axis. */
  private addForce = (env: Envelope, com: V3, fx: number, fy: number, fz: number, at: V3) => {
    if (fx > 0) env.pos[0] += fx;
    else env.neg[0] -= fx;
    if (fy > 0) env.pos[1] += fy;
    else env.neg[1] -= fy;
    if (fz > 0) env.pos[2] += fz;
    else env.neg[2] -= fz;
    const rx = at[0] - com[0];
    const ry = at[1] - com[1];
    const rz = at[2] - com[2];
    env.torque[0] += Math.abs(ry * fz - rz * fy);
    env.torque[1] += Math.abs(rz * fx - rx * fz);
    env.torque[2] += Math.abs(rx * fy - ry * fx);
  };

  /** Add a force (ship axes) at `at` to a wrench (force, torque about the centre of mass). */
  private push = (F: V3, T: V3, com: V3, fx: number, fy: number, fz: number, at: V3) => {
    const rx = at[0] - com[0];
    const ry = at[1] - com[1];
    const rz = at[2] - com[2];
    F[0] += fx;
    F[1] += fy;
    F[2] += fz;
    T[0] += ry * fz - rz * fy;
    T[1] += rz * fx - rx * fz;
    T[2] += rx * fy - ry * fx;
  };

  /**
   * Take over a ship someone else was flying (the pilot sat down, or stood up and the server
   * flies it now): the holds start from here, the thrusters from what they were giving.
   */
  resume() {
    this.fcs.reset();
    this.wake();
    const sim = this.sim;
    const idx = this.thrusters.outIndex(sim);
    for (let i = 0; i < idx.length; i++) this.out[i] = idx[i] >= 0 ? sim.st[idx[i]] : 0;
  }

  /** Thruster outputs as the network carries them (order of `thrusters.list`). */
  outputs(): number[] {
    return Array.from(this.out, (v) => Math.round(v * 1000) / 1000);
  }

  /**
   * The flight of a ship someone else flies (server: the pilot's client reports it): pose, ground
   * contact and thruster outputs, so the systems burn propellant and everyone draws the plumes.
   */
  adopt(o: { p: V3; q: [number, number, number, number]; v: V3; w: V3; landed: boolean; pad: boolean; agl: number; out: readonly number[] }) {
    const sim = this.sim;
    const pose = sim.pose;
    for (let i = 0; i < 3; i++) {
      pose.p[i] = o.p[i];
      pose.v[i] = o.v[i];
      pose.w[i] = o.w[i];
    }
    for (let i = 0; i < 4; i++) pose.q[i] = o.q[i];
    sim.landed = o.landed;
    sim.onPad = o.pad;
    this.agl = o.agl;
    this.sleeping = false;
    const idx = this.thrusters.outIndex(sim);
    this.thrusters.list.forEach((t, i) => {
      const v = Math.max(0, Math.min(t.kind === 'rcs' ? 6 : 1, o.out[i] ?? 0));
      this.out[i] = v;
      if (idx[i] >= 0) sim.st[idx[i]] = v;
    });
  }

  /** Outputs → state table (propellant burn, plumes everywhere) and telemetry. */
  private publish(fc: FcsOut, mp: MassProps) {
    const sim = this.sim;
    const idx = this.thrusters.outIndex(sim);
    for (let i = 0; i < idx.length; i++) if (idx[i] >= 0) sim.st[idx[i]] = this.out[i];
    this.telemetryFrom(fc, mp);
  }

  private telemetryFrom(fc: FcsOut, mp: MassProps) {
    const T = this.thrusters;
    const pose = this.sim.pose;
    const q = pose.q;
    let thrust = 0;
    let lift = 0;
    for (let i = 0; i < T.list.length; i++) {
      const t = T.list[i];
      thrust += this.out[i] * t.maxN;
      if (t.kind === 'lift') lift += this.avail[i] * t.maxN;
      else if (t.kind === 'main') lift += Math.max(0, rot(q, t.dir[0], t.dir[1], t.dir[2], _q)[1]) * this.avail[i] * t.maxN;
    }
    const tl = this.telemetry;
    const up = this.frame.up;
    const vs = pose.v[0] * up[0] + pose.v[1] * up[1] + pose.v[2] * up[2];
    tl.agl = this.agl;
    tl.vs = vs;
    tl.gs = Math.sqrt(Math.max(0, pose.v[0] * pose.v[0] + pose.v[1] * pose.v[1] + pose.v[2] * pose.v[2] - vs * vs));
    tl.heading = compassHeading(this.lpose.q);
    tl.direct = fc.direct;
    tl.modes = fc.sp.show;
    tl.thrust = thrust;
    tl.lift = lift;
    tl.weight = mp.mass * this.gMag;
    tl.throttle = throttleOf(this.sim);
    tl.targetV = fc.vH;
    tl.targetVz = fc.vz;
    tl.sleeping = this.sleeping;
  }

  /** Asleep: the readings the displays show, now and then. */
  private refreshTelemetry() {
    if (this.last) this.telemetryFrom(this.last, this.mp);
  }
}

/** No ground at all (high above the surface: the legs hang free). */
const NO_GROUND: Ground = { height: () => -1e9 };
