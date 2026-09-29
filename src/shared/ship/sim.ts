// Authoritative ship rules, shared by the server and by the client (which mirrors the server's
// state to predict denials and to show it). Only the server (or the offline client acting as its
// own server) calls the mutating methods and `tick`.
//
// State: `sw` = positions of every control (crew input), `hp` = hull panel integrity, `st` = the
// continuous simulation table (power, propellant, reactor, atmosphere, part integrity…, see
// systems.ts and modules/*.ts), replicated as quantised diffs.

import { SUN } from '../constants.js';
import { ventsOf, type Vent } from './airflow.js';
import type { ControlDef, PanelDef, PartDef, ShipDef, SubsystemId } from './def.js';
import { compassHeading, FlightModel, massProperties, type Lump, type MassProps, type ShipPose } from './flight/index.js';
import { add, closestInPoly, qConj, qRotate, qYaw, rotY, sub, toFrame, type Quat, type V3 } from './geom.js';
import type { SurfaceGround } from '../space/tangent.js';
import type { Engine } from './modules/engines.js';
import { quantize, VarTable } from './state.js';
import { panelArea, ShipSystems, SYSTEMS_HZ, type SysContext, type SysEvent } from './systems.js';
import type { HullView, InterlockEnv } from './modules/api.js';
import { bodyAt, inShadow, orbitalRegime, sunDirection } from '../space/body.js';

const NO_BODIES: V3[] = [];
const SUN_DIR = sunDirection(SUN.az, SUN.el);

/** Every ship the game knows, by id (ships/index.ts). */
export { SHIP_DEFS } from './ships/index.js';

/** A panel below this integrity is a hole (damage below it blows the panel out completely). */
export const SOLID_HP = 30;
/** HP per second one astronaut restores with the repair tool. */
export const REPAIR_RATE = 22;
export const BLAST = { radius: 2.8, damage: 75 };
/** Crew-scale radius of a rocket blast (m): other explosions scale the ship blast by radius / this. */
export const CREW_BLAST_RADIUS = 6;

/** Ship-scale blast of an explosion given in crew scale (rocket = 6 m, 75 dmg). Same on server and offline. */
export function shipBlast(radius = CREW_BLAST_RADIUS, damage = BLAST.damage) {
  return { radius: BLAST.radius * Math.max(1, radius / CREW_BLAST_RADIUS), damage };
}
/** Max distances (m) from the astronaut's eye to act on a control / repair a panel. */
export const REACH = { control: 2.4, repair: 3.2 };
export { DOOR_DP } from './modules/life.js';
export { SYSTEMS_HZ };

/** Where a ship was placed (world): on its gear on the ground, level on the local horizon. Its pad is there. */
export interface ShipPlacement {
  p: V3;
  q: Quat;
}

export interface ShipSnapshot {
  id: number;
  def: string;
  place: ShipPlacement;
  sw: Record<string, number>;
  hp: number[];
  /** Continuous state (full table, quantised). */
  st?: number[];
  /** World pose (where it is now; `place` is where it started). */
  pose?: ShipPose;
}

/** Height of a world point over the ground under it (m): space/body.ts `heightAboveGround`. */
export type GroundAlt = (p: readonly number[]) => number;

/** Nothing changed this tick (shared, never written). */
const NO_SW: Record<string, number> = Object.freeze({}) as Record<string, number>;
const NO_HP: Array<[number, number]> = Object.freeze([]) as unknown as Array<[number, number]>;

/**
 * Place a ship on its gear at (x, z) of a ground's tangent frame (space/tangent.ts: a site's, or
 * one laid anywhere), heading `yaw` in that frame: the floor at the mean height of the ground under
 * its feet plus its clearance, level on the frame's horizon. Works anywhere round any body.
 */
export function placeShip(def: ShipDef, ground: SurfaceGround, x: number, z: number, yaw: number): ShipPlacement {
  const legs: V3[] = def.gear?.legs.length ? def.gear.legs : [[0, 0, 0]];
  let sum = 0;
  for (const leg of legs) {
    const w = rotY(leg, yaw);
    sum += ground.height(x + w[0], z + w[2]);
  }
  const p = ground.toWorld(x, sum / legs.length + def.floorHeight, z, [0, 0, 0]) as V3;
  const q = ground.rotOut(qYaw(yaw), [0, 0, 0, 1]) as Quat;
  return { p, q };
}

export class ShipSim {
  readonly sw: Record<string, number>;
  readonly hp: number[];
  readonly vars = new VarTable();
  readonly sys: ShipSystems;
  st: Float64Array;
  /** Ramp rest angle below horizontal when lowered (rad), so its lip sits on the ground. */
  readonly rampAngle: number;
  /** Weight on the gear (or the hull resting on the ground). Set by the flight authority. */
  landed = true;
  /** Parked on a pad with a refuelling point. */
  onPad = true;
  /** Sunlight on the arrays (0..1): the landing site sits in the lunar day. */
  sun = SUN.el > 0 ? 1 : 0;
  /**
   * Where the ship is and how it moves (world). The flight authority integrates it
   * (`flight.step`), everyone else receives it. Every ship-space ↔ world conversion goes through it.
   */
  readonly pose: ShipPose;
  private flightModel: FlightModel | null = null;
  /** Reused every systems tick: the context, the hull as the tick sees it, panels it tore, the result. */
  private sysCtx: SysContext | null = null;
  private ilEnv: InterlockEnv = { landed: true, onPad: true, orbital: false };
  private noCrew: number[];
  private torn: number[] = [];
  private tickOut: { sw: Record<string, number>; hp: Array<[number, number]>; events: SysEvent[] } = { sw: NO_SW, hp: NO_HP, events: [] };
  private hullView: HullView = {
    hole: (i) => this.hole(i),
    crack: (i) => this.crack(i),
    hp: (i) => this.hp[i],
    damage: (i, amount) => {
      if (amount <= 0 || this.hp[i] <= 0) return;
      // below solid it blows out completely, like a blast
      this.hp[i] = this.hp[i] - amount < SOLID_HP ? 0 : this.hp[i] - amount;
      if (!this.torn.includes(i)) this.torn.push(i);
    },
  };
  /** Crack area per panel (m² at full crack), and the panels on each circuit's conduit. */
  private crackArea: Float64Array;

  constructor(
    readonly id: number,
    readonly def: ShipDef,
    readonly place: ShipPlacement,
    /** The ground where it stands (its ramp is laid on it). */
    groundAlt: GroundAlt,
    snap?: { sw: Record<string, number>; hp: number[]; st?: number[]; pose?: ShipPose },
  ) {
    this.pose = snap?.pose ? { p: [...snap.pose.p], q: [...snap.pose.q], v: [...snap.pose.v], w: [...snap.pose.w] } : { p: [...place.p], q: [...place.q], v: [0, 0, 0], w: [0, 0, 0] };
    this.sw = { ...def.defaults, ...(snap?.sw ?? {}) };
    this.hp = def.panels.map((p, i) => snap?.hp[i] ?? p.maxHp);
    this.crackArea = Float64Array.from(def.panels, (p) => panelArea(p.poly));
    this.noCrew = def.compartments.map(() => 0);
    this.sys = new ShipSystems(def, this.vars);
    this.st = Float64Array.from(this.vars.initial);
    if (snap?.st && snap.st.length === this.st.length) this.st.set(snap.st);
    else this.sys.init(this.st);
    // settle the ramp lip on the terrain (two fixed-point passes are plenty)
    const r = def.ramp;
    let a = 0;
    if (r) {
      a = Math.asin(Math.min(0.95, def.floorHeight / r.length));
      for (let k = 0; k < 3; k++) {
        // how far the ground under the tip is below the deck (the ship stands level)
        const drop = groundAlt(this.toWorld([r.hinge[0], 0, r.hinge[2] + Math.cos(a) * r.length]));
        a = Math.asin(Math.max(0.1, Math.min(0.7, drop / r.length)));
      }
    }
    this.rampAngle = a;
  }

  snapshot(): ShipSnapshot {
    const q = this.vars.quanta;
    return {
      id: this.id,
      def: this.def.id,
      place: { p: [...this.place.p], q: [...this.place.q] },
      sw: { ...this.sw },
      hp: this.hp.map((h) => Math.round(h * 10) / 10),
      st: Array.from(this.st, (v, i) => +quantize(v, Math.max(0, q[i])).toFixed(6)),
      pose: { p: [...this.pose.p], q: [...this.pose.q], v: [...this.pose.v], w: [...this.pose.w] },
    };
  }

  /** Named state value (tests, displays). */
  get(name: string) {
    return this.st[this.vars.idx(name)];
  }

  toLocal(p: V3): V3 {
    return qRotate(qConj(this.pose.q), sub(p, this.pose.p));
  }

  toWorld(p: V3): V3 {
    return add(qRotate(this.pose.q, p), this.pose.p);
  }

  dirToLocal(d: V3): V3 {
    return qRotate(qConj(this.pose.q), d);
  }

  /** Flight model (thrusters, flight computer, contact): used by whoever flies the ship. */
  get flight(): FlightModel {
    return (this.flightModel ??= new FlightModel(this));
  }

  /** State index of what each part holds (kg), for the mass. */
  private kgIndex: Map<PartDef, number> | null = null;

  private contentKg = (p: PartDef) => {
    if (!this.kgIndex) {
      this.kgIndex = new Map();
      for (const part of this.def.parts) if (this.vars.has(`${part.id}.kg`)) this.kgIndex.set(part, this.vars.idx(`${part.id}.kg`));
    }
    const i = this.kgIndex.get(p);
    return i === undefined ? 0 : Math.max(0, this.st[i]);
  };

  /**
   * Mass properties with the consumables aboard right now and anything else aboard (crew, crates).
   * `out`: an object to fill instead of a new one (the flight model reuses its own every step).
   */
  massNow(extra: readonly Lump[] = [], out?: MassProps): MassProps {
    return massProperties(this.def, this.contentKg, extra, out);
  }

  /** Panel is blown out (or still being rebuilt). */
  hole(i: number) {
    return this.hp[i] < SOLID_HP;
  }

  /** Crack area (m²) of a damaged but standing panel: slow leaks you can hear. */
  crack(i: number) {
    const p = this.def.panels[i];
    const r = this.hp[i] / p.maxHp;
    if (this.hole(i) || r >= 0.6) return 0;
    return 0.004 * ((0.6 - r) / 0.6) ** 2 * this.crackArea[i];
  }

  private holeFn = (i: number) => this.hole(i);

  /** First blown-out panel on a subsystem's conduit, if any. */
  conduitCut(sub: SubsystemId): PanelDef | null {
    const i = this.sys.conduitCut(sub, this.holeFn);
    return i < 0 ? null : this.def.panels[i];
  }

  /** Circuit energised (breaker closed, conduit intact, enough supply). */
  powered(sub: SubsystemId) {
    return this.sys.supply(this.st, sub) >= 0.5;
  }

  partHp(p: PartDef | number) {
    const part = typeof p === 'number' ? this.def.parts[p] : p;
    return this.st[this.sys.hpIndex(part.id)];
  }

  /** Mover travel 0..1 (door, ramp, shutters, gear…) as the authority simulates it. */
  mover(key: string) {
    return this.sys.mover(this.st, key);
  }

  /** Position a control would move its switch to (dir: mouse wheel step for rotary knobs). */
  next(c: ControlDef, dir = 0): number {
    const cur = this.sw[c.key] ?? 0;
    const n = c.states.length;
    switch (c.action) {
      case 'reset':
        return 0;
      case 'pulse':
        return 1;
      case 'set':
        return c.value ?? 0;
      case 'cycle': {
        // click steps forward; the wheel passes dir = ±1. Selectors wrap, scales stop at the end.
        const nxt = cur + (dir ? Math.sign(dir) : 1);
        return c.wrap ? ((nxt % n) + n) % n : Math.max(0, Math.min(n - 1, nxt));
      }
      default:
        return cur === 1 ? 0 : 1;
    }
  }

  /**
   * Why a control would do nothing right now (null = it works): what it is mounted on, its cover,
   * its circuit's wiring, then the interlocks of the systems (see modules/*.ts).
   */
  blocked(c: ControlDef, dir = 0): string | null {
    if (c.host >= 0 && this.hole(c.host)) return 'Mando destruido';
    if (c.hostPart >= 0 && this.partHp(c.hostPart) <= 0) return 'Mando destruido';
    if (c.drum && (this.sw[c.drum.key] ?? 0) !== c.drum.face) return 'Cara del tambor escondida: gíralo con GIRAR';
    if (c.guard && this.sw[c.guard] !== 1) return 'Tapa de seguridad cerrada';
    if (c.requires) {
      // a closed breaker and an intact cable accept the command; the motor waits if the bus is starved
      if (this.sw[this.sys.breakerOf(c.requires) ?? ''] !== 1) return `Sin energía · disyuntor ${this.circuitLabel(c.requires)} abierto`;
      const cut = this.conduitCut(c.requires);
      if (cut) return `Sin energía · conducto de ${this.circuitLabel(c.requires)} cortado (${cut.id})`;
    }
    const next = this.next(c, dir);
    if (next === (this.sw[c.key] ?? 0)) return null;
    const env = this.ilEnv;
    env.landed = this.landed;
    env.onPad = this.onPad;
    env.orbital = orbitalRegime(bodyAt(this.pose.p), this.pose.p, this.pose.v);
    return this.sys.interlock(c, next, this.st, this.sw, env);
  }

  /**
   * Operate a control (dir: mouse wheel step for rotary knobs). Returns the switches that changed,
   * or the reason it did nothing.
   */
  interact(index: number, dir = 0): { changed: Record<string, number> } | { reason: string } {
    const c = this.def.controls[index];
    if (!c) return { reason: 'Mando desconocido' };
    const reason = this.blocked(c, dir);
    if (reason) return { reason };
    const cur = this.sw[c.key] ?? 0;
    const v = this.next(c, dir);
    if (cur === v) return { changed: {} };
    this.sw[c.key] = v;
    this.version++;
    return { changed: { [c.key]: v } };
  }

  /**
   * One systems step (authority only). Returns switch changes and panels that changed (a panel
   * tearing under pressure) to broadcast, and events such as internal explosions (ship space) for
   * the authority to resolve.
   */
  /** Label of a circuit (messages). */
  private circuitLabel(c: SubsystemId) {
    const i = this.sys.circuitIndex(c);
    return i < 0 ? c : this.def.subsystems[i].label;
  }

  /**
   * One systems step. The result is reused by the next tick: read it right away (the server and the
   * offline client broadcast and apply it on the spot).
   */
  tick(dt: number, ctx: Partial<SysContext> = {}): { sw: Record<string, number>; hp: Array<[number, number]>; events: SysEvent[] } {
    const full = (this.sysCtx ??= { crew: this.noCrew, docked: 0, bodies: [], landed: true, onPad: true, sun: 0, agl: 0, pos: this.pose.p, vel: this.pose.v, heading: 0, rand: Math.random });
    full.crew = ctx.crew ?? this.noCrew;
    full.docked = ctx.docked ?? 0;
    full.bodies = ctx.bodies ?? NO_BODIES;
    full.landed = ctx.landed ?? this.landed;
    full.onPad = ctx.onPad ?? this.onPad;
    // the arrays see the sun unless the Moon is in the way (the night side of an orbit)
    full.sun = ctx.sun ?? (inShadow(bodyAt(this.pose.p), this.pose.p, SUN_DIR) ? 0 : this.sun);
    full.agl = ctx.agl ?? this.flight.agl;
    full.pos = ctx.pos ?? this.pose.p;
    full.vel = ctx.vel ?? this.pose.v;
    full.heading = ctx.heading ?? compassHeading(this.pose.q);
    full.rand = ctx.rand ?? Math.random;
    this.torn.length = 0;
    const r = this.sys.tick(dt, this.st, this.sw, full, this.hullView);
    this.version++;
    const out = this.tickOut;
    out.sw = r.sw;
    out.events = r.events;
    out.hp = this.torn.length ? this.torn.map((i): [number, number] => [i, Math.round(this.hp[i] * 10) / 10]) : NO_HP;
    return out;
  }

  /**
   * Gas leaving through breaches, cracks and open doors right now, with its place and push
   * (airflow.ts). `out`: an array to refill (callers asking every step keep their own).
   */
  vents(out?: Vent[]): Vent[] {
    return ventsOf(this, out);
  }

  /**
   * Blast at a world point: every panel and machine within the radius loses integrity with distance
   * to its nearest point. Returns changed panels, switches (master caution latches on new holes) and
   * follow-up events (tanks that rupture).
   */
  explode(pWorld: V3, blast = BLAST): { hp: Array<[number, number]>; sw: Record<string, number>; events: SysEvent[] } {
    const p = this.toLocal(pWorld);
    const changed: Array<[number, number]> = [];
    // nothing of the ship within reach: skip the per-panel and per-part work
    if (this.outOfReach(p, blast.radius * 1.15)) return { hp: changed, sw: {}, events: [] };
    let breach = false;
    for (const pn of this.def.panels) {
      if (this.hp[pn.index] <= 0) continue;
      const l = toFrame(pn, p);
      const [cu, cv] = closestInPoly(pn.poly, l[0], l[1]);
      const d = Math.hypot(cu - l[0], cv - l[1], Math.max(0, Math.abs(l[2]) - pn.t / 2));
      if (d >= blast.radius) continue;
      const k = pn.kind === 'glass' ? 1.5 : pn.kind === 'floor' ? 0.8 : 1;
      // quadratic falloff: the struck plate takes the blow, neighbours get dented
      let hp = this.hp[pn.index] - blast.damage * k * (1 - d / blast.radius) ** 2;
      if (hp < SOLID_HP) {
        hp = 0;
        breach = true;
      }
      this.hp[pn.index] = hp;
      changed.push([pn.index, Math.round(hp * 10) / 10]);
    }
    // machinery: same falloff, scaled by how exposed each part is
    const events: SysEvent[] = [];
    for (const part of this.def.parts) {
      const d = this.sys.partDistance(part, p);
      const r = blast.radius * 1.15;
      if (d >= r) continue;
      this.sys.damagePart(this.st, part, blast.damage * part.soft * (1 - d / r) ** 2, events);
    }
    const sw: Record<string, number> = {};
    const caution = this.def.caution;
    if (breach && this.sw[caution] !== 1) sw[caution] = this.sw[caution] = 1;
    this.version++;
    return { hp: changed, sw, events };
  }

  /** Ship-space box around every panel and part (m), for the quick rejections. */
  private box: { min: V3; max: V3 } | null = null;

  /** A ship-space point is farther than `r` from anything of the ship. */
  outOfReach(p: V3, r: number) {
    const b = this.localBox();
    const dx = Math.max(b.min[0] - p[0], 0, p[0] - b.max[0]);
    const dy = Math.max(b.min[1] - p[1], 0, p[1] - b.max[1]);
    const dz = Math.max(b.min[2] - p[2], 0, p[2] - b.max[2]);
    return dx * dx + dy * dy + dz * dz > r * r;
  }

  /** Ship-space box around every panel and part (m). */
  localBox(): { min: V3; max: V3 } {
    if (!this.box) {
      const min: V3 = [Infinity, Infinity, Infinity];
      const max: V3 = [-Infinity, -Infinity, -Infinity];
      const grow = (x: number, y: number, z: number, e: number) => {
        min[0] = Math.min(min[0], x - e);
        min[1] = Math.min(min[1], y - e);
        min[2] = Math.min(min[2], z - e);
        max[0] = Math.max(max[0], x + e);
        max[1] = Math.max(max[1], y + e);
        max[2] = Math.max(max[2], z + e);
      };
      for (const pn of this.def.panels) {
        let e = 0;
        for (const [u, v] of pn.poly) e = Math.max(e, Math.hypot(u, v));
        grow(pn.c[0], pn.c[1], pn.c[2], e + pn.t);
      }
      for (const part of this.def.parts) grow(part.c[0], part.c[1], part.c[2], Math.hypot(part.half[0], part.half[1], part.half[2]));
      this.box = { min, max };
    }
    return this.box;
  }

  /** Restore integrity (repair tool). Returns the new HP or null when nothing changed. */
  repair(index: number, amount: number): number | null {
    const pn = this.def.panels[index];
    if (!pn || this.hp[index] >= pn.maxHp) return null;
    this.hp[index] = Math.min(pn.maxHp, this.hp[index] + amount);
    this.version++;
    return Math.round(this.hp[index] * 10) / 10;
  }

  /** Repair a machine (welder). Machines take longer than plates. Returns the new HP or null. */
  repairPart(index: number, amount: number): number | null {
    const part = this.def.parts[index];
    if (!part) return null;
    const i = this.sys.hpIndex(part.id);
    if (this.st[i] >= part.maxHp) return null;
    this.st[i] = Math.min(part.maxHp, this.st[i] + amount * 0.6);
    this.version++;
    return this.st[i];
  }

  /**
   * Bumped whenever the switches, the panels or the continuous state change through `apply` /
   * `applyState` / `tick` (views redraw only what changed).
   */
  version = 0;

  /** Apply a server update (client mirror). Returns panels that switched between solid and hole. */
  apply(sw?: Record<string, number>, hp?: Array<[number, number]>) {
    if (sw) for (const k in sw) this.sw[k] = sw[k];
    const flipped: number[] = [];
    if (hp) {
      for (const [i, v] of hp) {
        if (i < 0 || i >= this.hp.length) continue;
        const was = this.hole(i);
        this.hp[i] = v;
        if (was !== this.hole(i)) flipped.push(i);
      }
    }
    this.version++;
    return flipped;
  }

  /** Apply continuous state diffs ([index, value, …]) from the server. */
  applyState(d: number[]) {
    for (let k = 0; k + 1 < d.length; k += 2) {
      const i = d[k];
      if (i >= 0 && i < this.st.length) this.st[i] = d[k + 1];
    }
    this.version++;
  }

  /** Overall hull integrity 0..1. */
  integrity() {
    let a = 0;
    let b = 0;
    for (const p of this.def.panels) {
      a += this.hp[p.index];
      b += p.maxHp;
    }
    return a / b;
  }

  controlWorld(i: number): V3 {
    return this.toWorld(this.def.controls[i].c);
  }

  panelWorld(i: number): V3 {
    return this.toWorld(this.def.panels[i].c);
  }

  partWorld(i: number): V3 {
    return this.toWorld(this.def.parts[i].c);
  }

  /** Engine thrust fraction (for effects): 0..1 while running. */
  engineThrust(id: string) {
    return this.sys.module<Engine>(`engine:${id}`)?.thrust(this.st) ?? 0;
  }
}
