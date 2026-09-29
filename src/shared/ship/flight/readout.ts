// What the displays and the helmet HUD show about a ship's flight, computed from what every client
// has (pose, state table, switches). The flight model's own telemetry exists only on the machine
// that flies the ship; this works on any of them.

import { partTag } from '../def.js';
import { qRotate, type V3 } from '../geom.js';
import type { ShipSim } from '../sim.js';
import { AP_KEY, apModes, apSelection, emptySetpoints, type ApContext, type ApSelection, type Setpoints } from './autopilot.js';
import { fcsPowered, throttleOf, tuningOf } from './fcs.js';
import { bearingTo } from './nav.js';
import { altitudeOf, baseFix, baseFrom, frameAt, gravityAt, homeDir, localBase, localFrame, bodyAt, orbitOf, type OrbitInfo, type Surfaces } from '../../space/body.js';

export interface FlightReadout {
  /** Height of the feet (gear down) or the hull above the ground (m). */
  agl: number;
  /** Vertical and ground speed (m/s), compass heading (rad), pitch and roll (rad, + nose up / right wing down). */
  vs: number;
  gs: number;
  heading: number;
  pitch: number;
  roll: number;
  landed: boolean;
  /** Gear travel 0 up … 1 down and locked. */
  gear: number;
  throttle: number;
  /** Thrust being made (N, from the published outputs) and the ship's lunar weight (N). */
  thrust: number;
  weight: number;
  /** No flight computer (dead avionics): direct control. */
  direct: boolean;
  /** Height above the body's sphere (m) and the orbit the ship is on. */
  alt: number;
  orbit: OrbitInfo;
  /** Orbital regime: flying round the body, not over the ground (see flight/model.ts). */
  orbital: boolean;
  /** The way home: distance to the base over the surface (m) and its compass bearing (rad). */
  base: { dist: number; bearing: number };
  apOn: boolean;
  /** Autopilot annunciations and what it asks for. */
  modes: string[];
  sp: Setpoints;
  sel: ApSelection | null;
  /** Bearing and distance to the selected waypoint. */
  wp: { name: string; bearing: number; dist: number } | null;
}

/** What the displays show of a ship's flight; `surfaces`: the ground of each body in this world. */
export function flightReadout(sim: ShipSim, surfaces: Surfaces): FlightReadout {
  const pose = sim.pose;
  const fm = sim.flight;
  const gear = fm.gear();
  const body = bodyAt(sim.pose.p);
  const alt = altitudeOf(body, pose.p);
  // over the body's ground wherever the ship is (the flight model's own reading on the machine that flies it)
  const agl = fm.clearanceAnywhere(surfaces(body), gear, body);
  // the local frame (up, north, east) where the ship is: anywhere round the body
  const lf = frameAt(body, pose.p, localFrame());
  const dot = (a: readonly number[], b: readonly number[]) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const fwd = qRotate(pose.q, [0, 0, -1]);
  const right = qRotate(pose.q, [1, 0, 0]);
  const upShip = qRotate(pose.q, [0, 1, 0]);
  const heading = (Math.atan2(dot(fwd, lf.east), dot(fwd, lf.north)) + Math.PI * 2) % (Math.PI * 2);
  const vs = dot(pose.v, lf.up);
  const vh: V3 = [dot(pose.v, lf.east), 0, -dot(pose.v, lf.north)];
  const gs = Math.hypot(vh[0], vh[2]);
  const orbit = orbitOf(body, pose.p, pose.v);
  const orbital = gs > 150 || alt > 15000;
  // the way home: the body's home site over the great circle (space/body.ts `homeDir`)
  const toBase = bearingTo(body, pose.p, { dir: homeDir(body) });
  const gv = gravityAt(body, pose.p, [0, 0, 0]);
  let thrust = 0;
  for (const t of fm.thrusters.list) if (sim.vars.has(t.outVar)) thrust += Math.max(0, sim.st[sim.vars.idx(t.outVar)]) * t.maxN;
  const ap = sim.def.autopilot;
  const sw = sim.sw;
  const apOn = !!ap && sw[AP_KEY('on')] === 1;
  const sel = ap ? apSelection(ap, sw) : null;
  const sp = emptySetpoints();
  if (ap && sel && apOn) {
    // the modes' annunciations (what they would ask for now; the flight computer does the flying)
    const vL: V3 = [vh[0], vs, vh[2]];
    const ctx: ApContext = {
      pose,
      at: pose.p,
      dt: 0,
      agl,
      heading,
      vH: vh,
      sel,
      brake: tuningOf(sim.def).accel * 0.7,
      landed: sim.landed,
      gear,
      v: vL,
      nose: [dot(fwd, lf.east), dot(fwd, lf.up), -dot(fwd, lf.north)],
      alt,
      g: Math.max(0, Math.hypot(gv[0], gv[1], gv[2]) - (gs * gs) / (alt + body.radius)),
      aMain: 1,
      aVert: 1,
      aLift: 2,
      orbit,
      base: localBase(baseFrom(body, pose.p, pose.v, baseFix()), lf),
      body,
    };
    for (const mode of apModes(ap)) if (sw[AP_KEY(mode.id)] === 1 && !(orbital && mode.face === 0 && mode.id !== 'lvl')) mode.apply(ctx, sp);
  }
  const wp = sel && sel.wp.body === body.def.id ? { name: sel.wp.name, ...bearingTo(body, pose.p, sel.wp) } : null;
  return {
    agl,
    vs,
    gs,
    heading,
    pitch: Math.asin(Math.max(-1, Math.min(1, dot(fwd, lf.up)))),
    roll: Math.atan2(-dot(right, lf.up), dot(upShip, lf.up)),
    landed: sim.landed,
    gear,
    throttle: throttleOf(sim),
    thrust,
    weight: fm.mp.mass * Math.hypot(gv[0], gv[1], gv[2]),
    direct: !fcsPowered(sim),
    alt,
    orbit,
    orbital,
    base: toBase,
    apOn,
    modes: sp.show,
    sp,
    sel,
    wp,
  };
}

/** How the main engines are being used and why (the MOTORES page). */
export interface MainUse {
  /** Mean output of the running engines, the throttle lever (0..1), engines running / fitted. */
  thr: number;
  throttle: number;
  running: number;
  count: number;
  /** Who sets their thrust, and what the pilot should know. */
  mode: 'MANUAL' | 'ACOPLADO' | 'P.AUT' | 'DIRECTO';
  note: string;
}

export function mainUse(sim: ShipSim): MainUse {
  const mains = sim.flight.thrusters.mains;
  let thr = 0;
  let running = 0;
  for (const t of mains) {
    const cap = `${partTag(t.part)}.cap`;
    if (sim.vars.has(cap) && sim.st[sim.vars.idx(cap)] > 0) {
      running++;
      thr += sim.vars.has(t.outVar) ? sim.st[sim.vars.idx(t.outVar)] : 0;
    }
  }
  thr = running ? thr / running : 0;
  const throttle = throttleOf(sim);
  const direct = !fcsPowered(sim);
  const apOn = !!sim.def.autopilot && sim.sw[AP_KEY('on')] === 1;
  const coupled = sim.sw['fa.hold'] !== 0;
  const mode = direct ? 'DIRECTO' : apOn ? 'P.AUT' : coupled ? 'ACOPLADO' : 'MANUAL';
  const note = !running
    ? 'Motor parado: ármalo y arráncalo'
    : throttle < 0.01
      ? 'Acelerador a 0 %: el motor no empuja'
      : mode === 'MANUAL' || mode === 'DIRECTO'
        ? 'Empuje directo = acelerador'
        : sim.landed
          ? 'En tierra: despega para usar el motor'
          : thr < 0.02
            ? mode === 'P.AUT'
              ? 'P.AUT: sin velocidad que pedir al motor'
              : 'Pide velocidad con W (o VEL del P.AUT)'
            : `El ordenador usa hasta el ${Math.round(throttle * 100)} %`;
  return { thr, throttle, running, count: mains.length, mode, note };
}
