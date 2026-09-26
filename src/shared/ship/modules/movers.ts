// Things that travel when a switch asks: doors, the ramp, shutters, the gear, radiator wings. Each
// one moves toward its switch position at its rate while its circuit is powered, draws extra power
// while it travels, and stops closing if its obstruction sensor sees someone. The travel (`mv.*`)
// is replicated, so every client draws the same position the gas flows and the colliders use.
//
// Landing gear: can't be raised with weight on the wheels.

import type { ShipSystems } from '../systems.js';
import type { ShipModule, SystemFactory, Tick } from './api.js';

export class Movers implements ShipModule {
  readonly id = 'movers';
  private moving = new Set<string>();

  constructor(private sys: ShipSystems) {}

  input(t: Tick) {
    this.moving.clear();
    for (const m of this.sys.def.movers) {
      const i = this.sys.moverIndex(m.key)!;
      const target = t.sw[m.key] ?? 0;
      const st = t.st;
      if (Math.abs(st[i] - target) < 1e-6 || this.sys.supply(st, m.circuit) < 0.5) continue;
      if (target < st[i] && m.sensor && t.ctx.bodies.some((b) => inside(b, m.sensor!))) continue;
      const k = m.rate * t.dt;
      st[i] = target > st[i] ? Math.min(target, st[i] + k) : Math.max(target, st[i] - k);
      this.moving.add(m.key);
    }
  }

  loads(t: Tick) {
    for (const m of this.sys.def.movers) if (this.moving.has(m.key)) t.load(m.circuit, m.load);
  }
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
