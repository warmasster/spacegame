// Inertial compensator: while it runs (switch on, powered, not wrecked) the crew and anything loose
// aboard feel the ship's floor as "down" at lunar weight whatever the ship does — banking,
// braking, climbing. Off or broken, they feel the real thing: the ship's tilt and every
// acceleration (crates slide when it brakes hard; in free fall everything floats).
//
// It publishes `grav.k` (0 = off … 1 = full compensation), which ramps as the field spins up and
// down; the client's interior physics blends the two gravities with it (client/frames).

import { partKey, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import { partsOf, type AlertDef, type ShipModule, type SoundCue, type SystemFactory, type Tick } from './api.js';

/** Spin-up / spin-down rate of the field (fraction per second). */
const RAMP = 0.4;

export class InertialCompensator implements ShipModule {
  readonly id = 'grav';
  readonly iK: number;
  private key: string;

  constructor(
    private sys: ShipSystems,
    private parts: PartDef[],
  ) {
    this.key = partKey(parts[0], 'on', parts[0].id);
    this.iK = sys.vars.define('grav.k', 0.02, sys.def.defaults[this.key] ? 1 : 0);
  }

  /** Target compensation now: the best working unit, scaled by its health. */
  private target(t: Tick) {
    if (t.sw[this.key] !== 1) return 0;
    let best = 0;
    for (const p of this.parts) {
      const h = this.sys.health(t.st, p);
      if (h <= 0 || this.sys.supply(t.st, p.circuit) < 0.5) continue;
      best = Math.max(best, 0.3 + 0.7 * h);
    }
    return best;
  }

  step(t: Tick) {
    const k = this.target(t);
    const cur = t.st[this.iK];
    t.st[this.iK] = k > cur ? Math.min(k, cur + RAMP * t.dt) : Math.max(k, cur - RAMP * t.dt);
  }

  /** The field generator: a deep throb that spins up and down with the field. */
  sounds(): SoundCue[] {
    const i = this.iK;
    return this.parts.map((part): SoundCue => ({ sound: 'mach.grav', role: 'run', part, level: (st) => Math.min(1, st[i] * 1.2), pitch: (st) => 0.55 + 0.45 * st[i] }));
  }

  alerts(): AlertDef[] {
    return [
      {
        id: 'grav.off',
        label: 'COMPENSADOR INERCIAL SIN FUNCIONAR',
        level: 1,
        lamp: this.parts[0].lamp ?? 'GRAVEDAD',
        help: 'El compensador está encendido pero no compensa (sin energía en su circuito o destruido): a bordo se notan la inclinación y cada aceleración de la nave; la carga suelta puede deslizarse.',
        on: (st, sw) => {
          if (sw[this.key] !== 1) return false;
          for (const p of this.parts) if (this.sys.health(st, p) > 0 && this.sys.supply(st, p.circuit) >= 0.5) return false;
          return true;
        },
      },
    ];
  }
}

export const gravSystem: SystemFactory = {
  id: 'grav',
  parts: ['grav'],
  make: (sys) => {
    const parts = partsOf(sys, 'grav');
    return parts.length ? [new InertialCompensator(sys, parts)] : [];
  },
};
