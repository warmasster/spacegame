// Things that travel when a switch asks: doors, the ramp, shutters, the gear, radiator wings. Each
// one moves toward its switch position at its rate while its circuit is powered, draws extra power
// while it travels, and stops closing if its obstruction sensor sees someone. The travel (`mv.*`)
// is replicated, so every client draws the same position the gas flows and the colliders use.
//
// Landing gear: can't be raised with weight on the wheels.

import type { ShipDef } from '../def.js';
import type { V3 } from '../geom.js';
import type { ShipSystems } from '../systems.js';
import type { ShipModule, SoundCue, SystemFactory, Tick } from './api.js';

/**
 * How each kind of mechanism sounds (bank ids, client/audio): its drive while it travels and the
 * stop at the end of the travel. A mover may bring its own (`MoverDef.sounds`).
 */
export const MOVER_SOUNDS: Record<MoverKind, { run: string; stop: string }> = {
  door: { run: 'mover.slide', stop: 'mover.stop' },
  ramp: { run: 'mover.hydraulic', stop: 'mover.lock' },
  gear: { run: 'mover.hydraulic', stop: 'mover.lock' },
  shutter: { run: 'mover.roller', stop: 'mover.stop' },
  servo: { run: 'mover.motor', stop: 'mover.stop' },
};

export type MoverKind = 'door' | 'ramp' | 'gear' | 'shutter' | 'servo';

/** What a mover's key drives, read from the definition (a door or hatch, the ramp, the gear, the shutters, or a machine's actuator). */
export function moverKind(def: ShipDef, key: string): MoverKind {
  if (def.doors.some((d) => d.key === key) || def.hatches.some((h) => h.key === key)) return 'door';
  if (def.ramp?.key === key) return 'ramp';
  if (def.gear?.key === key) return 'gear';
  if (def.shield?.key === key) return 'shutter';
  return 'servo';
}

/** Where a mover's drives are (ship space, at most `max`): each door, the ramp's hinge, every gear leg, the shutters, the machines it deploys or tilts. */
export function moverAnchors(def: ShipDef, key: string, max = 4): V3[] {
  const out: V3[] = [];
  for (const d of def.doors) if (d.key === key) out.push([d.c[0], d.c[1] + d.h / 2, d.c[2]]);
  for (const h of def.hatches) if (h.key === key) out.push(h.c);
  if (def.ramp?.key === key) out.push(def.ramp.hinge);
  if (def.gear?.key === key) out.push(...def.gear.legs);
  const plates = def.shield?.key === key ? def.shield.plates : [];
  if (plates.length) {
    const c: V3 = [0, 0, 0];
    for (const p of plates) for (let i = 0; i < 3; i++) c[i] += p.c[i] / plates.length;
    out.push(c);
  }
  for (const p of def.parts) {
    if (p.gimbal?.key === key) out.push(p.c);
    else if (p.sw) for (const role in p.sw) if (p.sw[role] === key && role !== 'run' && role !== 'on') out.push(p.c);
  }
  if (!out.length) out.push([(def.bounds.min[0] + def.bounds.max[0]) / 2, (def.bounds.min[1] + def.bounds.max[1]) / 2, (def.bounds.min[2] + def.bounds.max[2]) / 2]);
  return out.slice(0, max);
}

export class Movers implements ShipModule {
  readonly id = 'movers';
  /** Travelling this tick, per mover (order of `def.movers`), and each one's state index. */
  private moving: Uint8Array;
  private idx: Int32Array;

  constructor(private sys: ShipSystems) {
    this.moving = new Uint8Array(sys.def.movers.length);
    this.idx = Int32Array.from(sys.def.movers, (m) => sys.moverIndex(m.key)!);
  }

  input(t: Tick) {
    const movers = this.sys.def.movers;
    const st = t.st;
    for (let j = 0; j < movers.length; j++) {
      const m = movers[j];
      this.moving[j] = 0;
      const i = this.idx[j];
      const target = t.sw[m.key] ?? 0;
      if (Math.abs(st[i] - target) < 1e-6 || this.sys.supply(st, m.circuit) < 0.5) continue;
      if (target < st[i] && m.sensor && blocked(t.ctx.bodies, m.sensor)) continue;
      const k = m.rate * t.dt;
      st[i] = target > st[i] ? Math.min(target, st[i] + k) : Math.max(target, st[i] - k);
      this.moving[j] = 1;
    }
  }

  loads(t: Tick) {
    const movers = this.sys.def.movers;
    for (let j = 0; j < movers.length; j++) if (this.moving[j]) t.load(movers[j].circuit, movers[j].load);
  }

  /** Every drive whirs while its travel moves and knocks at the end of it (the clients see the replicated travel). */
  sounds(): SoundCue[] {
    const def = this.sys.def;
    const out: SoundCue[] = [];
    def.movers.forEach((m, j) => {
      const i = this.idx[j];
      const voice = { ...MOVER_SOUNDS[moverKind(def, m.key)], ...(m.sounds ?? {}) };
      const anchors = moverAnchors(def, m.key);
      // several drives (gear legs) share the loudness of one
      const gain = 1 / Math.sqrt(anchors.length);
      for (const at of anchors) {
        out.push({ sound: voice.run, at, gain, motion: { value: (st) => st[i], rate: m.rate } });
        out.push({ sound: voice.stop, at, gain, on: (st, sw) => Math.abs((sw[m.key] ?? 0) - st[i]) < 0.015 });
      }
    });
    return out;
  }
}

/** Someone stands in an obstruction sensor's box. */
function blocked(bodies: readonly (readonly number[])[], box: { min: readonly number[]; max: readonly number[] }) {
  for (const b of bodies) if (inside(b, box)) return true;
  return false;
}

const inside = (p: readonly number[], b: { min: readonly number[]; max: readonly number[] }) =>
  p[0] >= b.min[0] && p[0] <= b.max[0] && p[1] >= b.min[1] && p[1] <= b.max[1] && p[2] >= b.min[2] && p[2] <= b.max[2];

export class LandingGear implements ShipModule {
  readonly id = 'gear';

  constructor(private key: string) {}

  interlock(c: { key: string }, next: number, _st: Float64Array, _sw: Record<string, number>, env: { landed: boolean }) {
    if (c.key === this.key && next === 0 && env.landed) return 'Enclavamiento: peso sobre el tren';
    return null;
  }
}

export const moversSystem: SystemFactory = {
  id: 'movers',
  make: (sys) => {
    const out: ShipModule[] = [];
    if (sys.def.movers.length) out.push(new Movers(sys));
    if (sys.def.gear) out.push(new LandingGear(sys.def.gear.key));
    return out;
  },
};
