// Authoritative ship rules, shared by the server and by the client (which mirrors the server's
// state to predict denials and to evaluate power). Only the server (or the offline client acting
// as its own server) calls the mutating methods.

import type { ControlDef, PanelDef, ShipDef, SubsystemId } from './def.js';
import { add, closestInPoly, rotY, sub, toFrame, type V3 } from './geom.js';
import { HAULER } from './hauler.js';

export const SHIP_DEFS: Record<string, ShipDef> = { hauler: HAULER };

/** A panel below this integrity is a hole (damage below it blows the panel out completely). */
export const SOLID_HP = 30;
/** HP per second one astronaut restores with the repair tool. */
export const REPAIR_RATE = 22;
export const BLAST = { radius: 2.8, damage: 75 };
/** Max distances (m) from the astronaut's eye to act on a control / repair a panel. */
export const REACH = { control: 2.4, repair: 3.2 };

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
  /** Ramp rest angle below horizontal when lowered (rad), so its lip sits on the ground. */
  readonly rampAngle: number;
  /** Weight on wheels (always, until ships fly). */
  landed = true;

  constructor(
    readonly id: number,
    readonly def: ShipDef,
    readonly place: ShipPlacement,
    ground: Ground,
    snap?: { sw: Record<string, number>; hp: number[] },
  ) {
    this.sw = { ...def.defaults, ...(snap?.sw ?? {}) };
    this.hp = def.panels.map((p, i) => snap?.hp[i] ?? p.maxHp);
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
    return { id: this.id, def: this.def.id, x: this.place.x, z: this.place.z, yaw: this.place.yaw, sw: { ...this.sw }, hp: this.hp.map((h) => Math.round(h * 10) / 10) };
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

  /** First blown-out panel on a subsystem's conduit, if any. */
  conduitCut(sub: SubsystemId): PanelDef | null {
    for (const p of this.def.panels) if (p.conduits.includes(sub) && this.hole(p.index)) return p;
    return null;
  }

  powered(sub: SubsystemId) {
    const s = this.def.subsystems.find((x) => x.id === sub)!;
    return this.sw.reactor === 1 && this.sw[s.breaker] === 1 && !this.conduitCut(sub);
  }

  /** Why a control would do nothing right now (null = it works). */
  blocked(c: ControlDef): string | null {
    if (c.host >= 0 && this.hole(c.host)) return 'Mando destruido';
    if (c.requires && !this.powered(c.requires)) {
      const s = this.def.subsystems.find((x) => x.id === c.requires)!;
      if (this.sw.reactor !== 1) return 'Sin energía · reactor parado';
      if (this.sw[s.breaker] !== 1) return `Sin energía · disyuntor ${s.label} abierto`;
      return `Sin energía · conducto de ${s.label} cortado (${this.conduitCut(c.requires)!.id})`;
    }
    if (c.key === 'gear' && this.landed && this.sw.gear === 1) return 'Enclavamiento: peso sobre el tren';
    return null;
  }

  /** Operate a control. Returns the switches that changed, or the reason it did nothing. */
  interact(index: number): { changed: Record<string, number> } | { reason: string } {
    const c = this.def.controls[index];
    if (!c) return { reason: 'Mando desconocido' };
    const reason = this.blocked(c);
    if (reason) return { reason };
    const v = c.action === 'reset' ? 0 : this.sw[c.key] === 1 ? 0 : 1;
    if (this.sw[c.key] === v) return { changed: {} };
    this.sw[c.key] = v;
    return { changed: { [c.key]: v } };
  }

  /**
   * Blast at a world point: every panel within the radius loses integrity with distance to its
   * nearest point. Returns changed panels and switches (master caution latches on new holes).
   */
  explode(pWorld: V3, blast = BLAST): { hp: Array<[number, number]>; sw: Record<string, number> } {
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
    const sw: Record<string, number> = {};
    if (breach && this.sw.caution !== 1) sw.caution = this.sw.caution = 1;
    return { hp: changed, sw };
  }

  /** Restore integrity (repair tool). Returns the new HP or null when nothing changed. */
  repair(index: number, amount: number): number | null {
    const pn = this.def.panels[index];
    if (!pn || this.hp[index] >= pn.maxHp) return null;
    this.hp[index] = Math.min(pn.maxHp, this.hp[index] + amount);
    return Math.round(this.hp[index] * 10) / 10;
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
}
