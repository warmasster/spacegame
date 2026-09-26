// Solar arrays: free power while the sun shines on them. A wing gives its rated `kw` deployed
// (it tracks the sun on its gimbal) and `stowedFrac` of it folded against the hull; damage and
// shadow scale it down. It is offered to the grid ahead of every generator (order −1), so the
// reactor and the batteries only cover what the sun does not.

import { partKey, partTag, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import { partsOf, type ShipModule, type SystemFactory, type Tick } from './api.js';

export const SOLAR = { kw: 3.5, stowedFrac: 0.12 };

export class SolarArray implements ShipModule {
  readonly id: string;
  readonly tag: string;
  readonly deployKey: string;
  readonly k: typeof SOLAR;
  /** kW it can give right now (offered to the grid). */
  readonly iKw: number;

  constructor(
    private sys: ShipSystems,
    readonly part: PartDef,
  ) {
    this.id = `solar:${part.id}`;
    this.tag = partTag(part);
    this.deployKey = partKey(part, 'deploy', `${part.id}.deploy`);
    this.k = { ...SOLAR, ...part.p } as typeof SOLAR;
    this.iKw = sys.vars.define(`${this.tag}.kw`, 0.05);
  }

  /** Fraction of the rated output its position allows (0 → folded, 1 → deployed). */
  exposure(st: Float64Array) {
    const dep = this.sys.mover(st, this.deployKey);
    return this.k.stowedFrac + (1 - this.k.stowedFrac) * dep;
  }

  loads(t: Tick) {
    const kw = this.k.kw * this.exposure(t.st) * this.sys.health(t.st, this.part) * Math.max(0, Math.min(1, t.ctx.sun));
    t.st[this.iKw] = kw;
    if (kw > 0.01) t.source({ id: this.tag, kw, quality: 1, order: -1 });
  }
}

export const solarSystem: SystemFactory = {
  id: 'solar',
  parts: ['solar'],
  make: (sys) => partsOf(sys, 'solar').map((p) => new SolarArray(sys, p)),
};
