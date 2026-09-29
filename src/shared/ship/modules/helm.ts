// Helm: the pilot's throttle lever. Its position (0 … last) as a fraction is published as
// `helm.throttle` for the displays; the flight computer reads the lever itself (shared/ship/flight):
// in coupled flight it caps how much of the main engines it may use, decoupled it is their thrust.

import type { ShipSystems } from '../systems.js';
import type { ShipModule, SystemFactory, Tick } from './api.js';

export class Helm implements ShipModule {
  readonly id = 'helm';
  private steps: number;
  readonly iThrottle: number;

  constructor(
    sys: ShipSystems,
    private key: string,
  ) {
    this.steps = Math.max(1, (sys.def.controls.find((c) => c.key === key)?.states.length ?? 2) - 1);
    this.iThrottle = sys.vars.define('helm.throttle', 0.01);
  }

  input(t: Tick) {
    t.st[this.iThrottle] = Math.max(0, Math.min(1, (t.sw[this.key] ?? 0) / this.steps));
  }
}

export const helmSystem: SystemFactory = {
  id: 'helm',
  make: (sys) => (sys.def.helm ? [new Helm(sys, sys.def.helm.throttle)] : []),
};
