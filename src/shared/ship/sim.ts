// Authoritative ship rules, shared by the server and by the client (which mirrors the server's
// state to predict denials and to show it). Only the server (or the offline client acting as its
// own server) calls the mutating methods and `tick`.
//
// State: `sw` = positions of every control (crew input), `hp` = hull panel integrity, `st` = the
// continuous simulation table (power, propellant, reactor, atmosphere, part integrity…, see
// systems.ts), replicated as quantised diffs.

import type { ControlDef, PanelDef, PartDef, ShipDef, SubsystemId } from './def.js';
import { add, closestInPoly, rotY, sub, toFrame, type V3 } from './geom.js';
import { HAULER } from './hauler.js';
import { quantize, VarTable } from './state.js';
import { ENG, panelArea, RX, ShipSystems, SYSTEMS_HZ, type SysContext, type SysEvent } from './systems.js';

export const SHIP_DEFS: Record<string, ShipDef> = { hauler: HAULER };

/** A panel below this integrity is a hole (damage below it blows the panel out completely). */
export const SOLID_HP = 30;
/** HP per second one astronaut restores with the repair tool. */
export const REPAIR_RATE = 22;
export const BLAST = { radius: 2.8, damage: 75 };
/** Max distances (m) from the astronaut's eye to act on a control / repair a panel. */
export const REACH = { control: 2.4, repair: 3.2 };
/** Pressure difference (kPa) across a door above which it refuses to open. */
export const DOOR_DP = 5;
export { SYSTEMS_HZ };

export interface ShipPlacement {
  x: number;
  y: number;
  z: number;
  yaw: number;
}

export interface ShipSnapshot {
  id: number;
  def: string;
  x: number;
  z: number;
  yaw: number;
  sw: Record<string, number>;
  hp: number[];
  /** Continuous state (full table, quantised). */
  st?: number[];
}

type Ground = { height(x: number, z: number): number };

/** World placement on its gear: floor at the mean ground height under the feet + clearance. */
export function placeShip(def: ShipDef, x: number, z: number, yaw: number, ground: Ground): ShipPlacement {
  let sum = 0;
  for (const leg of def.gear.legs) {
    const w = rotY(leg, yaw);
    sum += ground.height(x + w[0], z + w[2]);
  }
  return { x, y: sum / def.gear.legs.length + def.floorHeight, z, yaw };
}

export class ShipSim {
  readonly sw: Record<string, number>;
  readonly hp: number[];
  readonly vars = new VarTable();
  readonly sys: ShipSystems;
  st: Float64Array;
  /** Ramp rest angle below horizontal when lowered (rad), so its lip sits on the ground. */
  readonly rampAngle: number;
  /** Weight on wheels (always, until ships fly). */
  landed = true;
  /** Parked on a pad with a refuelling point. */
  onPad = true;

  constructor(
    readonly id: number,
    readonly def: ShipDef,
    readonly place: ShipPlacement,
    ground: Ground,
    snap?: { sw: Record<string, number>; hp: number[]; st?: number[] },
  ) {
    this.sw = { ...def.defaults, ...(snap?.sw ?? {}) };
    this.hp = def.panels.map((p, i) => snap?.hp[i] ?? p.maxHp);
    this.sys = new ShipSystems(def, this.vars);
    this.st = Float64Array.from(this.vars.initial);
    if (snap?.st && snap.st.length === this.st.length) this.st.set(snap.st);
    else this.sys.init(this.st);
    // settle the ramp lip on the terrain (two fixed-point passes are plenty)
    const r = def.ramp;
    let a = Math.asin(Math.min(0.95, def.floorHeight / r.length));
    for (let k = 0; k < 3; k++) {
      const tip = this.toWorld([r.hinge[0], r.hinge[1] - Math.sin(a) * r.length, r.hinge[2] + Math.cos(a) * r.length]);
      const drop = place.y - ground.height(tip[0], tip[2]);
      a = Math.asin(Math.max(0.1, Math.min(0.7, drop / r.length)));
    }
    this.rampAngle = a;
  }

  snapshot(): ShipSnapshot {
    const q = this.vars.quanta;
    return {
      id: this.id,
      def: this.def.id,
      x: this.place.x,
      z: this.place.z,
      yaw: this.place.yaw,
      sw: { ...this.sw },
      hp: this.hp.map((h) => Math.round(h * 10) / 10),
      st: Array.from(this.st, (v, i) => +quantize(v, Math.max(0, q[i])).toFixed(6)),
    };
  }

  /** Named state value (tests, displays). */
  get(name: string) {
    return this.st[this.vars.idx(name)];
  }

  toLocal(p: V3): V3 {
    return rotY(sub(p, [this.place.x, this.place.y, this.place.z]), -this.place.yaw);
  }

  toWorld(p: V3): V3 {
    return add(rotY(p, this.place.yaw), [this.place.x, this.place.y, this.place.z]);
  }

  dirToLocal(d: V3): V3 {
    return rotY(d, -this.place.yaw);
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
    return 0.004 * ((0.6 - r) / 0.6) ** 2 * panelArea(p.poly);
  }

  /** First blown-out panel on a subsystem's conduit, if any. */
  conduitCut(sub: SubsystemId): PanelDef | null {
    for (const p of this.def.panels) if (p.conduits.includes(sub) && this.hole(p.index)) return p;
    return null;
  }

  /** Circuit energised (breaker closed, conduit intact, enough supply). */
  powered(sub: SubsystemId) {
    return this.sys.supply(this.st, sub) >= 0.5;
  }

  partHp(p: PartDef | number) {
    const part = typeof p === 'number' ? this.def.parts[p] : p;
    return this.st[this.sys.hpIndex(part.id)];
  }

  /** Door or ramp position 0..1 as the authority simulates it. */
  mover(key: string) {
    const i = this.sys.moverIndex(key);
    return i === undefined ? 0 : this.st[i];
  }

  /** Why a control would do nothing right now (null = it works). */
  blocked(c: ControlDef): string | null {
    const sys = this.sys;
    const st = this.st;
    if (c.host >= 0 && this.hole(c.host)) return 'Mando destruido';
    if (c.guard && this.sw[c.guard] !== 1) return 'Tapa de seguridad cerrada';
    if (c.requires && !this.powered(c.requires)) {
      const s = this.def.subsystems.find((x) => x.id === c.requires)!;
      if (this.sw[s.breaker] !== 1) return `Sin energía · disyuntor ${s.label} abierto`;
      const cut = this.conduitCut(c.requires);
      if (cut) return `Sin energía · conducto de ${s.label} cortado (${cut.id})`;
      return `Sin energía · ${s.label} (${Math.round(sys.supply(st, c.requires) * 100)} %)`;
    }
    const v = this.sw[c.key] ?? 0;
    switch (c.key) {
      case 'gear':
        if (this.landed && v === 1) return 'Enclavamiento: peso sobre el tren';
        break;
      case 'reactor':
        if (v === 0) return sys.reactorStartBlock(st, this.sw);
        break;
      case 'apu':
        if (v === 0) return sys.apuStartBlock(st, this.sw);
        break;
      case 'rx.reset':
        if (st[sys.idx('rx.state')] !== RX.scram) return 'El reactor no está en SCRAM';
        if (st[sys.idx('rx.temp')] >= 300) return `Núcleo demasiado caliente para rearmar (${Math.round(st[sys.idx('rx.temp')])} °C)`;
        break;
      case 'refuel':
        if (v === 0 && !(this.landed && this.onPad)) return 'No hay toma de repostaje aquí (plataforma de la base)';
        break;
      case 'ramp':
        // the cargo bay must be vented before the ramp opens onto vacuum
        if (v === 0 && sys.pressure(st, 'cargo') > DOOR_DP) return `Enclavamiento: bodega presurizada (${sys.pressure(st, 'cargo').toFixed(0)} kPa) — ventéala o recupera el aire`;
        break;
    }
    if (c.key.endsWith('.start') && c.key.startsWith('eng.')) return sys.engineStartBlock(st, this.sw, c.key.slice(0, -6));
    const door = this.def.doors.find((d) => d.key === c.key);
    if (door && v === 0) {
      const o = this.def.openings.find((x) => x.key === door.key);
      if (o && o.b) {
        const dp = Math.abs(sys.pressure(st, o.a) - sys.pressure(st, o.b));
        if (dp > DOOR_DP) return `Enclavamiento: diferencia de presión ${dp.toFixed(0)} kPa`;
      }
    }
    return null;
  }

  /**
   * Operate a control (dir: mouse wheel step for rotary knobs). Returns the switches that changed,
   * or the reason it did nothing.
   */
  interact(index: number, dir = 0): { changed: Record<string, number> } | { reason: string } {
    const c = this.def.controls[index];
    if (!c) return { reason: 'Mando desconocido' };
    const reason = this.blocked(c);
    if (reason) return { reason };
    const cur = this.sw[c.key] ?? 0;
    const n = c.states.length;
    let v: number;
    switch (c.action) {
      case 'reset':
        v = 0;
        break;
      case 'pulse':
        v = 1;
        break;
      case 'set':
        v = c.value ?? 0;
        break;
      case 'cycle':
        v = dir ? Math.max(0, Math.min(n - 1, cur + Math.sign(dir))) : (cur + 1) % n;
        break;
      default:
        v = cur === 1 ? 0 : 1;
    }
    if (cur === v) return { changed: {} };
    this.sw[c.key] = v;
    return { changed: { [c.key]: v } };
  }

  /**
   * One systems step (authority only). Returns switch changes to broadcast and events such as
   * internal explosions (ship space) for the authority to resolve.
   */
  tick(dt: number, ctx: Partial<SysContext> = {}): { sw: Record<string, number>; events: SysEvent[] } {
    const full: SysContext = {
      crew: ctx.crew ?? this.def.compartments.map(() => 0),
      docked: ctx.docked ?? 0,
      bodies: ctx.bodies ?? [],
      landed: ctx.landed ?? this.landed,
      onPad: ctx.onPad ?? this.onPad,
      rand: ctx.rand ?? Math.random,
    };
    const r = this.sys.tick(dt, this.st, this.sw, full, (i) => this.hole(i), (i) => this.crack(i));
    return { sw: r.sw, events: r.events };
  }

  /**
   * Blast at a world point: every panel and machine within the radius loses integrity with distance
   * to its nearest point. Returns changed panels, switches (master caution latches on new holes) and
   * follow-up events (tanks that rupture).
   */
  explode(pWorld: V3, blast = BLAST): { hp: Array<[number, number]>; sw: Record<string, number>; events: SysEvent[] } {
    const p = this.toLocal(pWorld);
    const changed: Array<[number, number]> = [];
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
    if (breach && this.sw.caution !== 1) sw.caution = this.sw.caution = 1;
    return { hp: changed, sw, events };
  }

  /** Restore integrity (repair tool). Returns the new HP or null when nothing changed. */
  repair(index: number, amount: number): number | null {
    const pn = this.def.panels[index];
    if (!pn || this.hp[index] >= pn.maxHp) return null;
    this.hp[index] = Math.min(pn.maxHp, this.hp[index] + amount);
    return Math.round(this.hp[index] * 10) / 10;
  }

  /** Repair a machine (welder). Machines take longer than plates. Returns the new HP or null. */
  repairPart(index: number, amount: number): number | null {
    const part = this.def.parts[index];
    if (!part) return null;
    const i = this.sys.hpIndex(part.id);
    if (this.st[i] >= part.maxHp) return null;
    this.st[i] = Math.min(part.maxHp, this.st[i] + amount * 0.6);
    return this.st[i];
  }

  /** Apply a server update (client mirror). Returns panels that switched between solid and hole. */
  apply(sw?: Record<string, number>, hp?: Array<[number, number]>) {
    if (sw) for (const [k, v] of Object.entries(sw)) this.sw[k] = v;
    const flipped: number[] = [];
    if (hp) {
      for (const [i, v] of hp) {
        if (i < 0 || i >= this.hp.length) continue;
        const was = this.hole(i);
        this.hp[i] = v;
        if (was !== this.hole(i)) flipped.push(i);
      }
    }
    return flipped;
  }

  /** Apply continuous state diffs ([index, value, …]) from the server. */
  applyState(d: number[]) {
    for (let k = 0; k + 1 < d.length; k += 2) {
      const i = d[k];
      if (i >= 0 && i < this.st.length) this.st[i] = d[k + 1];
    }
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

  /** Engine thrust fractions (for effects): id → 0..1 while running. */
  engineThrust(id: string) {
    const s = this.st[this.sys.idx(`${id}.state`)];
    return s === ENG.run || s === ENG.spool ? this.st[this.sys.idx(`${id}.thr`)] : 0;
  }
}
