// Every source of thrust on a ship, read from its parts:
//
//   main  — `engine` parts: push toward the nose from their aft nozzle. A `gimbal` (tilting
//           nacelle) turns the push by `rad` × the mover's travel, and can duct it out of a
//           nozzle `at` near the centre of mass (VTOL). The pilot's throttle or the flight computer
//           commands them; they answer slowly (spool).
//   lift  — `lift` parts: VTOL pads under the belly pushing straight up, vectorable inside a cone
//           (`p.gimbalDeg`). The flight computer drives them (hover, climb, attitude, braking).
//   rcs   — `rcs` blocks: a nozzle along each of the six axes, fast and weak.
//
// What each one can give right now (0..1) comes from the ship state: running engine and its cap,
// health, power on its circuit, propellant reaching it and its master switch.

import { partKey, partTag, type PartDef, type ShipDef } from '../def.js';
import { add, cross, dot, lerp3, rotY, scale, type V3 } from '../geom.js';
import type { ShipSim } from '../sim.js';

export type ThrusterKind = 'main' | 'lift' | 'rcs';

export interface Thruster {
  part: PartDef;
  kind: ThrusterKind;
  /** Nozzle (ship space) and the direction it pushes the ship (unit), for the current gimbal travel. */
  at: V3;
  dir: V3;
  /** Newtons at full thrust (per axis for RCS blocks). */
  maxN: number;
  /** Propellant at full thrust (kg/s, per axis for RCS). */
  flowKg: number;
  /** Lift pads: tan of the vectoring cone half-angle (0 = fixed). */
  cone: number;
  /** First-order response time (s). */
  tau: number;
  /** State variable the flight writes its output to (fraction of full thrust 0..1). */
  outVar: string;
  /** Master switch that enables it, if any. */
  master?: string;
  /** Lift pads: two unit vectors across the axis (the pad vectors its push along them). */
  e1?: V3;
  e2?: V3;
}

/** State-table indices a set of thrusters reads and writes, resolved once per ship (-1: none). */
interface Bound {
  vars: ShipSim['vars'];
  out: Int32Array;
  cap: Int32Array;
  feed: Int32Array;
  hp: Int32Array;
}

/** Two unit vectors perpendicular to `d` (and to each other). */
export function basisOf(d: V3): [V3, V3] {
  const a: V3 = Math.abs(d[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
  const c = cross(a, d);
  const l = Math.hypot(c[0], c[1], c[2]) || 1;
  const e1: V3 = [c[0] / l, c[1] / l, c[2] / l];
  return [e1, cross(d, e1)];
}

/** Response times (s): engines spool, pads and RCS valves are quick. */
export const RESPONSE: Record<ThrusterKind, number> = { main: 0.45, lift: 0.12, rcs: 0.05 };

/** Rodrigues rotation of `v` about a unit axis. */
function rotAbout(axis: V3, v: V3, ang: number): V3 {
  const c = Math.cos(ang);
  const s = Math.sin(ang);
  return add(add(scale(v, c), scale(cross(axis, v), s)), scale(axis, dot(axis, v) * (1 - c)));
}

const smooth = (t: number) => t * t * (3 - 2 * t);

/** Nozzle and push direction of a main engine at gimbal travel 0..1. */
export function engineGeometry(p: PartDef, travel: number): { at: V3; dir: V3 } {
  const axis = rotY([1, 0, 0], p.yaw);
  const g = p.gimbal;
  const t = Math.max(0, Math.min(1, travel));
  const ang = g ? t * g.rad : 0;
  const aft = rotY([0, 0, p.half[2]], p.yaw);
  const dir = rotAbout(axis, rotY([0, 0, -1], p.yaw), ang);
  if (g?.at) return { at: lerp3(add(p.c, aft), g.at, smooth(t)), dir };
  return { at: add(p.c, rotAbout(axis, aft, ang)), dir };
}

export class ThrusterSet {
  readonly list: Thruster[] = [];
  readonly mains: Thruster[] = [];
  readonly lifts: Thruster[] = [];
  readonly rcs: Thruster[] = [];
  private bound: Bound | null = null;
  /** Gimbal travel each main engine's geometry was last built for. */
  private travelAt: Float64Array;

  constructor(readonly def: ShipDef) {
    for (const p of def.parts) {
      const tag = partTag(p);
      if (p.type === 'engine') {
        const g = engineGeometry(p, 0);
        const t: Thruster = { part: p, kind: 'main', at: g.at, dir: g.dir, maxN: p.p.thrustN ?? 0, flowKg: p.p.flowKg ?? 0, cone: 0, tau: RESPONSE.main, outVar: `${tag}.thr` };
        this.mains.push(t);
      } else if (p.type === 'lift') {
        const t: Thruster = {
          part: p,
          kind: 'lift',
          at: add(p.c, [0, -p.half[1], 0]),
          dir: [0, 1, 0],
          maxN: p.p.thrustN ?? 0,
          flowKg: p.p.flowKg ?? 0,
          cone: Math.tan(((p.p.gimbalDeg ?? 20) * Math.PI) / 180),
          tau: RESPONSE.lift,
          outVar: `${tag}.use`,
          master: partKey(p, 'on', 'lift'),
        };
        this.lifts.push(t);
      } else if (p.type === 'rcs') {
        const t: Thruster = { part: p, kind: 'rcs', at: p.c, dir: [0, 0, 0], maxN: p.p.thrustN ?? 0, flowKg: p.p.flowKg ?? 0, cone: 0, tau: RESPONSE.rcs, outVar: `${tag}.use`, master: partKey(p, 'on', 'rcs') };
        this.rcs.push(t);
      }
    }
    this.list.push(...this.mains, ...this.lifts, ...this.rcs);
    for (const t of this.lifts) [t.e1, t.e2] = basisOf(t.dir);
    this.travelAt = new Float64Array(this.mains.length).fill(-1);
  }

  /** State indices of this set's thrusters in `sim`'s table (resolved once; a set serves one ship). */
  private indices(sim: ShipSim): Bound {
    if (this.bound?.vars === sim.vars) return this.bound;
    const vars = sim.vars;
    const at = (name: string) => (vars.has(name) ? vars.idx(name) : -1);
    const n = this.list.length;
    const b: Bound = { vars, out: new Int32Array(n), cap: new Int32Array(n), feed: new Int32Array(n), hp: new Int32Array(n) };
    this.list.forEach((t, i) => {
      b.out[i] = at(t.outVar);
      b.cap[i] = t.kind === 'main' ? at(`${partTag(t.part)}.cap`) : -1;
      b.feed[i] = t.part.feed ? at(`${t.part.id}.feed`) : -1;
      b.hp[i] = sim.sys.hpIndex(t.part.id) ?? -1;
    });
    return (this.bound = b);
  }

  /** State index each thruster's output is written to (-1: none), same order as `list`. */
  outIndex(sim: ShipSim): Int32Array {
    return this.indices(sim).out;
  }

  /** Gimbal travel of an engine: its mover (animated tilt) or, without one, the switch itself. */
  static travel(sim: ShipSim, p: PartDef) {
    const key = p.gimbal?.key;
    if (!key) return 0;
    return sim.sys.moverIndex(key) !== undefined ? sim.mover(key) : sim.sw[key] ?? 0;
  }

  /** Follow the tilting nacelles (rebuilt only when their travel moved). */
  update(sim: ShipSim) {
    for (let i = 0; i < this.mains.length; i++) {
      const t = this.mains[i];
      if (!t.part.gimbal) continue;
      const travel = ThrusterSet.travel(sim, t.part);
      if (travel === this.travelAt[i]) continue;
      this.travelAt[i] = travel;
      const g = engineGeometry(t.part, travel);
      t.at = g.at;
      t.dir = g.dir;
    }
  }

  /** What each thruster can give right now, 0..1 (same order as `list`). */
  availability(sim: ShipSim, out: Float64Array) {
    const st = sim.st;
    const b = this.indices(sim);
    const list = this.list;
    for (let i = 0; i < list.length; i++) {
      const t = list[i];
      if (t.kind === 'main') {
        const c = b.cap[i];
        // above 1 in overdrive (modules/engines.ts BOOST)
        out[i] = c >= 0 ? Math.max(0, Math.min(2, st[c])) : 0;
        continue;
      }
      if (t.master && sim.sw[t.master] !== undefined && sim.sw[t.master] !== 1) {
        out[i] = 0;
        continue;
      }
      const h = b.hp[i] >= 0 ? Math.max(0, Math.min(1, st[b.hp[i]] / t.part.maxHp)) : 1;
      if (h <= 0) {
        out[i] = 0;
        continue;
      }
      const power = t.part.circuit ? Math.min(1, sim.sys.supply(st, t.part.circuit) * 2) : 1;
      const feed = b.feed[i] >= 0 ? Math.min(1, st[b.feed[i]] / 0.9) : t.part.feed ? 0 : 1;
      out[i] = (0.35 + 0.65 * h) * power * feed;
    }
  }
}
