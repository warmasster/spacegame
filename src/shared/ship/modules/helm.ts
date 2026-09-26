// Helm: the pilot's throttle. Its position (0 … last) is the thrust fraction every main engine is
// commanded to (`<engine>.cmd`); the engines follow it with their own spool lag, capped by health
// and feed (engines.ts). The flight model reads the resulting thrust (flight.ts).

import { partTag } from '../def.js';
import type { ShipSystems } from '../systems.js';
import type { ShipModule, SystemFactory, Tick } from './api.js';

export class Helm implements ShipModule {
  readonly id = 'helm';
  private cmd: number[];
  private steps: number;
  readonly iThrottle: number;

  constructor(
    sys: ShipSystems,
    private key: string,
    engines: string[] | undefined,
  ) {
    const def = sys.def;
    const list = def.parts.filter((p) => p.type === 'engine' && (!engines || engines.includes(p.id)));
    // engine variables are declared by the engines module, which runs before this one
    this.cmd = list.map((p) => sys.idx(`${partTag(p)}.cmd`));
    this.steps = Math.max(1, (def.controls.find((c) => c.key === key)?.states.length ?? 2) - 1);
    this.iThrottle = sys.vars.define('helm.throttle', 0.01);
  }

  input(t: Tick) {
    const f = Math.max(0, Math.min(1, (t.sw[this.key] ?? 0) / this.steps));
    t.st[this.iThrottle] = f;
    for (const i of this.cmd) t.st[i] = f;
  }
}

export const helmSystem: SystemFactory = {
  id: 'helm',
  make: (sys) => (sys.def.helm ? [new Helm(sys, sys.def.helm.throttle, sys.def.helm.engines)] : []),
};
