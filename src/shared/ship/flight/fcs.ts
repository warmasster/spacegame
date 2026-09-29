// Flight computer (FCS): the pilot's stick, the assists and the autopilot → the force and torque
// the ship should make this step, and the command for the main engines. It knows nothing about
// individual thrusters (that is the allocator's job) except the envelope of what they can give.
//
//   attitude  — the stick asks for turn rates; released, the stabiliser (ESTAB.) holds the
//               attitude, the level assist (NIVEL, landing) brings pitch and roll back to the
//               horizon, a heading hold keeps the nose where it was.
//   translation — coupled (ACOPLADO): the stick asks for velocities in the heading frame (forward,
//               right, up); released, the ship stops and holds its height. Decoupled: the stick is
//               raw acceleration and the ship keeps whatever velocity it had. Gravity is always
//               compensated while the flight computer has power.
//   main engines — coupled (and under the autopilot) the throttle lever caps how much of them the
//               flight computer may use to reach the speed it is asked for (W/S, or the autopilot's
//               VEL / NAV); decoupled it is the thrust itself. They answer slowly, the pads and RCS
//               cover the transients. Sitting on the ground (coupled) they stay quiet: lift off first.
//   no power on its circuit → DIRECTO: stick straight to torque and force, no gravity help.

import type { ShipDef } from '../def.js';
import { cross, dot, qConj, qRotate, type Quat, type V3 } from '../geom.js';
import type { ShipSim } from '../sim.js';
import { bodyAt, type BaseFix, type CelestialBody, type OrbitInfo } from '../../space/body.js';
import { AP_KEY, apModes, apSelection, emptySetpoints, type ApContext, type Setpoints } from './autopilot.js';
import { mulM3, tensor, type MassProps } from './mass.js';
import { compassHeading, qLog, type ShipPose } from './pose.js';

/**
 * What the pilot is asking for, each axis −1..1, in pilot axes (not ship axes):
 * surge + forward, sway + right, heave + up, pitch + nose up, yaw + nose right, roll + right wing down.
 */
export interface FlightCommand {
  surge: number;
  sway: number;
  heave: number;
  pitch: number;
  yaw: number;
  roll: number;
}

export const FLIGHT_IDLE: FlightCommand = Object.freeze({ surge: 0, sway: 0, heave: 0, pitch: 0, yaw: 0, roll: 0 });

/** Wire form of a stick command: surge, sway, heave, pitch, yaw, roll. */
export type FlightAxes = [number, number, number, number, number, number];

export const flightAxes = (c: FlightCommand): FlightAxes => [c.surge, c.sway, c.heave, c.pitch, c.yaw, c.roll];

const axis = (v: unknown) => {
  const n = typeof v === 'number' ? v : 0;
  return Number.isFinite(n) ? Math.max(-1, Math.min(1, n)) : 0;
};

/** A stick command from the network: anything malformed becomes idle on that axis. */
export function flightCommand(a: readonly unknown[] | undefined): FlightCommand {
  const x = a ?? [];
  return { surge: axis(x[0]), sway: axis(x[1]), heave: axis(x[2]), pitch: axis(x[3]), yaw: axis(x[4]), roll: axis(x[5]) };
}

const DEAD = 0.08;
const active = (v: number) => Math.abs(v) > DEAD;
export const isIdle = (c: FlightCommand) => !active(c.surge) && !active(c.sway) && !active(c.heave) && !active(c.pitch) && !active(c.yaw) && !active(c.roll);

/** Handling of a ship (data, `ShipDef.flight`), with defaults. */
export interface FlightTuning {
  /** Coupled flight: forward / sideways speed at full stick (m/s). */
  vmax: number;
  vside: number;
  /** Climb / descent speed at full stick (m/s). */
  vz: number;
  /** Turn rates at full stick: pitch, yaw, roll (rad/s). */
  rate: V3;
  /** Horizontal acceleration the flight computer uses at most (comfort, m/s²). */
  accel: number;
}

export const DEFAULT_TUNING: FlightTuning = { vmax: 25, vside: 8, vz: 5, rate: [0.4, 0.45, 0.55], accel: 3 };

export const tuningOf = (def: ShipDef): FlightTuning => ({ ...DEFAULT_TUNING, ...(def.flight ?? {}) });

/** What the thrusters can give along each ship axis right now (N, N·m). */
export interface Envelope {
  /** Force along +x, +y, +z / −x, −y, −z. */
  pos: V3;
  neg: V3;
  /** Torque about x, y, z (either way). */
  torque: V3;
}

export interface FcsOut {
  /** Force (N, ship axes, gravity compensation included) and torque (N·m about the centre of mass). */
  F: V3;
  T: V3;
  /** Main engines: fraction of their full thrust. */
  main: number;
  /**
   * The main engines' push is the pilot's (decoupled flight, direct control): the other thrusters
   * add to it instead of cancelling it. Coupled, the computer counts it toward `F`.
   */
  mainFree: boolean;
  direct: boolean;
  sp: Setpoints;
  /** Targets the computer is flying to (telemetry): horizontal velocity, vertical speed. */
  vH: V3 | null;
  vz: number | null;
  /** The pilot asked to climb (or the autopilot does): leave the ground. */
  climbing: boolean;
}

/** Lunar surface gravity (m/s²): what the flight computer carries unless told otherwise. */
const G_SURFACE = 1.62;
const K = { vel: 0.9, vz: 2.0, alt: 0.5, holdAlt: 0.5, rate: 3.2, level: 1.6, att: 1.5, hdg: 1.1, mainLp: 0.25 };
/** Most turn rate the autopilot uses to point the nose in the orbital regime (rad/s). */
const ORBIT_TURN = 0.08;
/** Share of the pads' lift the height loops may ask for: the rest stays for attitude. */
const LIFT_MARGIN = 0.88;
const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/**
 * Speed to close a gap `err` (m): proportional near the target, never faster than what `decel`
 * can stop in time, capped at `vmax`. Loops built on it arrive without overshoot.
 */
export function approach(err: number, vmax: number, decel: number, k: number) {
  const d = Math.abs(err);
  return Math.sign(err) * Math.min(vmax, k * d, Math.sqrt(2 * Math.max(0.05, decel) * 0.7 * d));
}
const wrapPi = (a: number) => Math.atan2(Math.sin(a), Math.cos(a));

/** Throttle lever position as a fraction (0..1). */
export function throttleOf(sim: ShipSim): number {
  const h = sim.def.helm;
  if (!h) return 0;
  const c = sim.def.controls.find((x) => x.key === h.throttle);
  const steps = Math.max(1, (c?.states.length ?? 2) - 1);
  return clamp((sim.sw[h.throttle] ?? 0) / steps, 0, 1);
}

/** Circuit the flight computer runs on (dead circuit → direct control). */
export function fcsPowered(sim: ShipSim): boolean {
  const c = sim.def.autopilot?.circuit;
  return c ? sim.sys.supply(sim.st, c) >= 0.5 : true;
}

export class FlightComputer {
  /** Attitude held by the stabiliser (pitch / roll), heading held, altitude held (world y). */
  holdQ: Quat | null = null;
  holdHdg: number | null = null;
  holdAlt: number | null = null;
  private mainLp = 0;

  reset() {
    this.holdQ = null;
    this.holdHdg = null;
    this.holdAlt = null;
    this.mainLp = 0;
  }

  compute(o: {
    sim: ShipSim;
    pose: ShipPose;
    mp: MassProps;
    env: Envelope;
    cmd: FlightCommand;
    dt: number;
    agl: number;
    landed: boolean;
    gear: number;
    /** Main engines: capacity (N) and mean push direction (ship axes) of those that can run. */
    mainCap: number;
    mainDir: V3;
    /**
     * Weight to carry per kilogram (m/s²): gravity less what flying round the body takes off it
     * (default the lunar surface's). `pose` is in the local frame then (y up the local vertical).
     */
    g?: number;
    /** Orbital regime: no position or height holds, the throttle is the engines, the autopilot only levels. */
    orbital?: boolean;
    /** The orientation in the world frame (attitude hold is inertial: a local frame turning under it doesn't move it). */
    qWorld?: Quat;
    /** Something of the ship (a foot, the hull) is on the ground. */
    touching?: boolean;
    /** Height above the body's sphere (m), the orbit and the base (in the local frame) for the orbital modes. */
    alt?: number;
    orbit?: OrbitInfo | null;
    base?: BaseFix | null;
    /** The body it flies round (default: the one ruling where it is). */
    body?: CelestialBody;
  }): FcsOut {
    const { sim, pose, mp, env, cmd, dt, agl, landed } = o;
    const G = o.g ?? G_SURFACE;
    const orbital = !!o.orbital;
    const qw = o.qWorld ?? pose.q;
    const sw = sim.sw;
    const def = sim.def;
    const tune = tuningOf(def);
    const m = mp.mass;
    const I = mp.inertia;
    const qi = qConj(pose.q);
    const wb = qRotate(qi, pose.w);
    const comW = qRotate(pose.q, mp.com);
    const vCom: V3 = [pose.v[0] + pose.w[1] * comW[2] - pose.w[2] * comW[1], pose.v[1] + pose.w[2] * comW[0] - pose.w[0] * comW[2], pose.v[2] + pose.w[0] * comW[1] - pose.w[1] * comW[0]];
    const heading = compassHeading(pose.q);
    const fwdH: V3 = [Math.sin(heading), 0, -Math.cos(heading)];
    const rightH: V3 = [Math.cos(heading), 0, Math.sin(heading)];
    const throttle = throttleOf(sim);
    const alpha: V3 = [0, 1, 2].map((k) => Math.max(0.02, (0.5 * env.torque[k]) / Math.max(1, I[k]))) as V3;

    // ---- direct control: no flight computer ----------------------------------------------------------
    if (!fcsPowered(sim)) {
      this.reset();
      const pick = (v: number, k: number) => (v >= 0 ? v * env.pos[k] : v * env.neg[k]);
      return {
        F: [pick(cmd.sway, 0), pick(cmd.heave, 1), pick(-cmd.surge, 2)],
        T: [cmd.pitch * env.torque[0] * 0.5, -cmd.yaw * env.torque[1] * 0.5, -cmd.roll * env.torque[2] * 0.5],
        main: throttle,
        mainFree: true,
        direct: true,
        sp: emptySetpoints(),
        vH: null,
        vz: null,
        climbing: cmd.heave > DEAD,
      };
    }
    const sas = sw['fa.sas'] !== 0;
    // coupled flight holds positions and heights: not in orbit (it would brake the orbit away)
    const coupled = sw['fa.hold'] !== 0 && !orbital;
    const landAssist = sw['fa.land'] !== 0;

    // ---- autopilot -------------------------------------------------------------------------------------
    const sp = emptySetpoints();
    const ap = def.autopilot;
    const apOn = !!ap && sw[AP_KEY('on')] === 1;
    const aMainMax = (o.mainCap * Math.max(0, -o.mainDir[2])) / m;
    if (ap && apOn) {
      const aBack = env.pos[2] / m;
      const aV = Math.max(0.2, Math.min(Math.max(0, (LIFT_MARGIN * env.pos[1]) / m - G), env.neg[1] / m + G));
      const ctx: ApContext = {
        pose,
        at: sim.pose.p,
        dt,
        agl,
        heading,
        vH: [vCom[0], 0, vCom[2]],
        sel: apSelection(ap, sw),
        brake: Math.max(0.3, Math.min(tune.accel, aBack) * 0.7),
        landed,
        gear: o.gear,
        v: vCom,
        nose: qRotate(pose.q, [0, 0, -1]),
        alt: o.alt ?? pose.p[1],
        g: G,
        aMain: aMainMax,
        aVert: aV,
        aLift: (LIFT_MARGIN * env.pos[1]) / m,
        orbit: o.orbit ?? null,
        base: o.base ?? null,
        body: o.body ?? bodyAt(sim.pose.p),
      };
      const modes = apModes(ap);
      for (const mode of modes) if (mode.face === 0 && sw[AP_KEY(mode.id)] === 1) mode.apply(ctx, sp);
      if (orbital) {
        // the surface modes mean nothing in orbit: speeds and heights over the ground are the orbit's
        if (sp.vH || sp.vz !== null || sp.alt !== null || sp.heading !== null) sp.show.push('SUPERFICIE CANCELADO');
        sp.vH = null;
        sp.vz = null;
        sp.alt = null;
        sp.heading = null;
        sp.show.push('ORBITAL');
      }
      // the orbit and space faces work anywhere
      for (const mode of modes) if (mode.face !== 0 && sw[AP_KEY(mode.id)] === 1) mode.apply(ctx, sp);
    }

    // ---- attitude --------------------------------------------------------------------------------------
    const inRot: V3 = [cmd.pitch, -cmd.yaw, -cmd.roll];
    const pr = active(cmd.pitch) || active(cmd.roll);
    // landing: close to the ground, descending, gear down → level it for the touchdown
    const landingLevel = landAssist && o.gear > 0.9 && agl < 4 && !landed && vCom[1] < 0.3;
    // touching down, going down: stop holding it level and let it settle on all its feet (on a
    // slope, holding it level stood it on one leg with the pads carrying the rest)
    const settling = !!o.touching && agl < 0.5 && !landed && (active(cmd.heave) ? cmd.heave < 0 : (sp.vz ?? -1) < 0.1) && !(cmd.heave > DEAD);
    const level = (sp.level || landingLevel) && !sp.point && !settling;
    // the autopilot points the nose: error to that orientation (body axes)
    const point = sp.point ? pointError(pose.q, sp.point, sp.pointUp ?? [0, 1, 0]) : null;
    // settling on its feet the attitude is the ground's: holding the one it touched with stood it on
    // two legs of a crater rim, the pads carrying half the weight
    if (pr || !sas || level || point || settling) this.holdQ = null;
    else if (!this.holdQ && Math.abs(wb[0]) < 0.05 && Math.abs(wb[2]) < 0.05) this.holdQ = [...qw];
    // the compass means nothing in orbit (it turns with the ground below): the attitude hold holds the nose
    if (orbital || point || active(cmd.yaw) || !sas || Math.abs(wb[1]) > 0.06) this.holdHdg = null;
    else if (this.holdHdg === null) this.holdHdg = heading;
    const upB = qRotate(qi, [0, 1, 0]);
    const tiltAng = Math.acos(clamp(upB[1], -1, 1));
    const tl = Math.hypot(upB[2], upB[0]) || 1;
    const toLevel: V3 = [(upB[2] / tl) * tiltAng, 0, (-upB[0] / tl) * tiltAng];
    const hold = this.holdQ ? qRotate(qConj(qw), qLog(qMulConj(this.holdQ, qw))) : null;
    const wT: V3 = [0, 0, 0];
    const ctl = [false, false, false];
    const hdgTarget = sp.heading ?? (sas ? this.holdHdg : null);
    for (let k = 0; k < 3; k++) {
      const r = tune.rate[k];
      if (active(inRot[k])) {
        wT[k] = inRot[k] * r;
        ctl[k] = sas;
      } else if (point) {
        // turn at the rate that still stops on the target (big ships turn slowly); in the orbital
        // regime gently: the torque the pads and RCS make also pushes a little, and in orbit every
        // stray push moves the orbit
        wT[k] = approach(point[k], orbital ? Math.min(r, ORBIT_TURN) : r, alpha[k], K.att);
        ctl[k] = true;
      } else if (k !== 1 && level) {
        wT[k] = clamp(K.level * toLevel[k], -r, r);
        ctl[k] = true;
      } else if (k !== 1 && settling) {
        // pitch and roll free: the feet decide the tilt
      } else if (k === 1 && hdgTarget !== null) {
        wT[1] = clamp(-K.hdg * wrapPi(hdgTarget - heading), -r, r);
        ctl[1] = true;
      } else if (sas) {
        wT[k] = hold && k !== 1 ? clamp(K.att * hold[k], -r, r) : 0;
        ctl[k] = true;
      }
    }
    const aAng: V3 = [0, 0, 0];
    for (let k = 0; k < 3; k++) {
      if (ctl[k]) aAng[k] = clamp(K.rate * (wT[k] - wb[k]), -alpha[k], alpha[k]);
      else if (active(inRot[k])) aAng[k] = inRot[k] * alpha[k];
    }
    const Iw = mulM3(tensor(I), wb);
    const Ia = mulM3(tensor(I), aAng);
    const gyro = cross(wb, Iw);
    const T: V3 = [Ia[0] + gyro[0], Ia[1] + gyro[1], Ia[2] + gyro[2]];

    // ---- translation -----------------------------------------------------------------------------------
    const aUp = Math.max(0, (LIFT_MARGIN * env.pos[1]) / m - G);
    const aDown = env.neg[1] / m + G;
    const aVert = Math.max(0.2, Math.min(aUp, aDown));
    // vertical speed target
    let vz: number | null = null;
    if (active(cmd.heave)) {
      vz = cmd.heave * tune.vz;
      this.holdAlt = null;
    } else if (sp.vz !== null) {
      vz = sp.vz;
      this.holdAlt = null;
    } else if (sp.alt !== null) {
      vz = approach(sp.alt - agl, tune.vz, aVert, K.alt);
      this.holdAlt = null;
    } else if (coupled) {
      // released: stop the climb or the descent first, then hold the height where it stopped
      if (this.holdAlt === null && Math.abs(vCom[1]) < 0.25) this.holdAlt = pose.p[1];
      vz = this.holdAlt === null ? 0 : approach(this.holdAlt - pose.p[1], tune.vz, aVert, K.holdAlt);
    } else this.holdAlt = null;
    if (vz !== null && landAssist && agl < 40) vz = Math.max(vz, -(0.35 + 0.3 * Math.max(0, agl)));
    // settling: unload the pads (the feet take the weight)
    if (settling && vz !== null && vz < 0.1) vz = -2;
    const climbing = (vz ?? 0) > 0.2 || (!coupled && cmd.heave > DEAD);
    // sitting on the gear and not climbing: no thrust, let it rest
    if (landed && !climbing) {
      this.holdAlt = null;
      this.mainLp = 0;
      return { F: [0, 0, 0], T: [0, 0, 0], main: coupled ? 0 : throttle, mainFree: !coupled, direct: false, sp, vH: null, vz: null, climbing: false };
    }
    // horizontal velocity target
    let vH: V3 | null = null;
    if (active(cmd.surge) || active(cmd.sway)) {
      if (coupled || sp.vH) vH = add2(scale3(fwdH, cmd.surge * tune.vmax), scale3(rightH, cmd.sway * tune.vside));
    } else if (sp.vH) vH = sp.vH;
    else if (coupled) vH = [0, 0, 0];
    const aW: V3 = [0, 0, 0];
    // the main engines may accelerate the ship as hard as the lever lets them
    const aMain = coupled ? (throttle * o.mainCap * Math.max(0, -o.mainDir[2])) / m : 0;
    const aFwd = Math.min(Math.max(tune.accel, aMain), env.neg[2] / m + aMain);
    const aBack = Math.min(tune.accel, env.pos[2] / m);
    const aSide = Math.min(tune.accel, Math.min(env.pos[0], env.neg[0]) / m);
    if (vH) {
      let f = K.vel * dot([vH[0] - vCom[0], 0, vH[2] - vCom[2]], fwdH);
      let r = K.vel * dot([vH[0] - vCom[0], 0, vH[2] - vCom[2]], rightH);
      f = clamp(f, -aBack, aFwd);
      r = clamp(r, -aSide, aSide);
      aW[0] = fwdH[0] * f + rightH[0] * r;
      aW[2] = fwdH[2] * f + rightH[2] * r;
    } else if (active(cmd.surge) || active(cmd.sway)) {
      const f = cmd.surge >= 0 ? cmd.surge * aFwd : cmd.surge * aBack;
      aW[0] = fwdH[0] * f + rightH[0] * cmd.sway * aSide;
      aW[2] = fwdH[2] * f + rightH[2] * cmd.sway * aSide;
    }
    if (vz !== null) aW[1] = clamp(K.vz * (vz - vCom[1]), -aDown, aUp);
    else if (active(cmd.heave)) aW[1] = cmd.heave >= 0 ? cmd.heave * aUp : cmd.heave * aDown;
    // gravity compensation: the thrusters carry the weight
    // gravity compensation: the thrusters carry the weight — except on a closed orbit with no
    // vertical speed asked for, where falling round the body is the orbit (carrying it would bend it)
    const carry = !(orbital && o.orbit?.orbiting && vz === null && !active(cmd.heave));
    const Fw: V3 = [m * aW[0], m * (aW[1] + (carry ? G : 0)), m * aW[2]];
    const F = qRotate(qi, Fw);

    // ---- main engines ------------------------------------------------------------------------------------
    // an orbital burn of the autopilot: it has the engines (the lever is locked, see modules/autopilot.ts)
    if (sp.main !== null) {
      this.mainLp = 0;
      return { F, T, main: sp.main, mainFree: true, direct: false, sp, vH, vz, climbing };
    }
    let main = throttle;
    // the computer sets the engines when it flies a speed or a height (coupled, or an autopilot
    // mode that does); an autopilot only levelling or turning leaves the lever to the pilot
    const apTranslates = apOn && (sp.vH !== null || sp.vz !== null || sp.alt !== null);
    const computed = (coupled || apTranslates) && !orbital;
    if (computed) {
      const along = o.mainCap > 0 ? Math.max(0, dot(F, o.mainDir)) / o.mainCap : 0;
      this.mainLp += (along - this.mainLp) * Math.min(1, dt / K.mainLp);
      main = Math.min(throttle, this.mainLp);
    }
    return { F, T, main, mainFree: !computed, direct: false, sp, vH, vz, climbing };
  }
}

/**
 * Rotation (body axes, rad: pitch x, yaw y, roll z) from orientation `q` to the one with the nose
 * (−z) along `fwd` and the top (+y) as close to `up` as it can be. All in the same frame.
 */
function pointError(q: Quat, fwd: V3, up: V3): V3 {
  // target basis: x = fwd × up, y = x × fwd, z = −fwd
  let x = cross(fwd, up);
  let xl = Math.hypot(x[0], x[1], x[2]);
  if (xl < 1e-3) {
    // nose straight along `up`: any top will do, keep the current one's side
    const side = qRotate(q, [1, 0, 0]);
    x = [side[0], side[1], side[2]];
    const d = dot(x, fwd);
    x = [x[0] - fwd[0] * d, x[1] - fwd[1] * d, x[2] - fwd[2] * d];
    xl = Math.hypot(x[0], x[1], x[2]) || 1;
  }
  x = [x[0] / xl, x[1] / xl, x[2] / xl];
  const y = cross(x, fwd);
  const z: V3 = [-fwd[0], -fwd[1], -fwd[2]];
  const target = quatFromColumns(x, y, z);
  return qRotate(qConj(q), qLog(qMulConj(target, q)));
}

/** Quaternion of the rotation whose matrix has columns x, y, z. */
function quatFromColumns(x: V3, y: V3, z: V3): Quat {
  const m00 = x[0];
  const m10 = x[1];
  const m20 = x[2];
  const m01 = y[0];
  const m11 = y[1];
  const m21 = y[2];
  const m02 = z[0];
  const m12 = z[1];
  const m22 = z[2];
  const tr = m00 + m11 + m22;
  if (tr > 0) {
    const s = 0.5 / Math.sqrt(tr + 1);
    return [(m21 - m12) * s, (m02 - m20) * s, (m10 - m01) * s, 0.25 / s];
  }
  if (m00 > m11 && m00 > m22) {
    const s = 2 * Math.sqrt(1 + m00 - m11 - m22);
    return [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s];
  }
  if (m11 > m22) {
    const s = 2 * Math.sqrt(1 + m11 - m00 - m22);
    return [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s];
  }
  const s = 2 * Math.sqrt(1 + m22 - m00 - m11);
  return [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s];
}

function qMulConj(a: Quat, b: Quat): Quat {
  // a · conj(b)
  const [bx, by, bz, bw] = [-b[0], -b[1], -b[2], b[3]];
  return [a[3] * bx + a[0] * bw + a[1] * bz - a[2] * by, a[3] * by - a[0] * bz + a[1] * bw + a[2] * bx, a[3] * bz + a[0] * by - a[1] * bx + a[2] * bw, a[3] * bw - a[0] * bx - a[1] * by - a[2] * bz];
}

const add2 = (a: V3, b: V3): V3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const scale3 = (a: V3, k: number): V3 => [a[0] * k, a[1] * k, a[2] * k];
