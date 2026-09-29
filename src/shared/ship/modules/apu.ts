// Auxiliary power unit: a small turbine that burns propellant and backs up the grid. Start-up
// needs starting power (battery or another generator), the circuit its starter runs on and
// propellant reaching it. A failure latches: switch it off and on again to retry.

import { partKey, partTag, type ControlDef, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import { partsOf, type AlertDef, type PowerSource, type ShipModule, type SoundCue, type SystemFactory, type Tick } from './api.js';

export const APU = { kw: 15, startS: 8, spoolKw: 3 };
/** Engine / APU states. */
export const ENG = { off: 0, spool: 1, run: 2, fail: 3 } as const;

export class Apu implements ShipModule {
  readonly id: string;
  readonly tag: string;
  readonly key: string;
  readonly k: typeof APU;
  private v: Record<string, number> = {};
  /** What it offers the grid (one object, refilled every tick). */
  private offer: PowerSource;

  constructor(
    private sys: ShipSystems,
    readonly part: PartDef,
  ) {
    this.id = `apu:${part.id}`;
    this.tag = partTag(part);
    this.key = partKey(part, 'run', part.id);
    this.k = { ...APU, ...part.p } as typeof APU;
    this.offer = { id: this.tag, kw: 0, quality: 1, order: 1 };
    for (const [name, q] of [
      ['state', 1],
      ['t', 0.1],
      ['out', 0.01],
    ] as const)
      this.v[name] = sys.vars.define(`${this.tag}.${name}`, q);
  }

  vi(name: string) {
    return this.v[name];
  }

  /** Why the APU can't start (null = it can). */
  startBlock(st: Float64Array, sw: Record<string, number>) {
    const sys = this.sys;
    const p = this.part;
    if (sys.hp(st, p.id) <= 0) return `${p.name} destruida`;
    if (sys.power && !sys.power.canStart(st, sw, this.tag)) return 'Sin energía para el motor de arranque';
    if (sys.supply(st, p.circuit) < 0.5) return `Sin energía · circuito ${this.circuitLabel()}`;
    const fuel = sys.fuel;
    if (fuel && p.feed) {
      if (fuel.isolated(p.id, sw)) return `Válvula de alimentación de la ${p.name} cerrada`;
      if (!fuel.canFeed(p.feed, sw, st, 0.5)) return `Sin propelente para la ${p.name}`;
    }
    return null;
  }

  private circuitLabel() {
    return this.sys.def.subsystems.find((c) => c.id === this.part.circuit)?.label ?? this.part.circuit ?? '';
  }

  loads(t: Tick) {
    const { st } = t;
    const s = st[this.v.state];
    if (s === ENG.spool) t.load(this.part.circuit, this.k.spoolKw);
    if (s === ENG.run) {
      this.offer.kw = this.k.kw * (0.4 + 0.6 * this.sys.health(st, this.part));
      t.source(this.offer);
    }
    // a failed or stopped unit burns nothing
    if ((s === ENG.spool || s === ENG.run) && this.part.feed) t.burn(this.part.id, 0.003 + 0.012 * st[this.v.out]);
  }

  step(t: Tick) {
    const { dt, st, sw } = t;
    const V = this.v;
    const H = this.sys.health(st, this.part);
    const fed = this.part.feed && this.sys.fuel ? st[this.sys.fuel.feedIndex(this.part.id)] : 1;
    const s = st[V.state];
    st[V.out] = s === ENG.run ? Math.min(1, (this.sys.power?.deliveredBy(this.tag) ?? 0) / this.k.kw) : 0;
    if (sw[this.key] !== 1) {
      st[V.state] = ENG.off;
      st[V.t] = 0;
    } else if (s === ENG.off) {
      const why = this.startBlock(st, sw);
      if (!why) {
        st[V.state] = ENG.spool;
        st[V.t] = 0;
      } else {
        st[V.state] = ENG.fail;
        t.say(`${this.part.name}: ${why}`);
      }
    } else if (s === ENG.spool) {
      st[V.t] += dt / this.k.startS;
      const starved = fed < 0.3 && st[V.t] > 0.2;
      if (starved || (H < 0.3 && t.ctx.rand() < 0.3 * dt)) {
        st[V.state] = ENG.fail;
        t.say(starved ? `${this.part.name}: sin alimentación de propelente` : `${this.part.name}: fallo de arranque (dañada)`);
      } else if (st[V.t] >= 1) st[V.state] = ENG.run;
    } else if (s === ENG.run && (fed < 0.3 || H <= 0)) {
      st[V.state] = ENG.fail;
      t.say(`${this.part.name}: se ha apagado (alimentación / daños)`);
    }
    // ENG.fail stays latched until the switch is cycled
  }

  interlock(c: ControlDef, next: number, st: Float64Array, sw: Record<string, number>) {
    if (c.key === this.key && next === 1) return this.startBlock(st, sw);
    return null;
  }

  sounds(): SoundCue[] {
    const V = this.v;
    const part = this.part;
    return [
      // the turbine: the starter winds it up, running it whines and roars with the load
      { sound: 'mach.turbine', role: 'run', part, level: (st) => (st[V.state] === ENG.run ? 0.7 + 0.3 * st[V.out] : st[V.state] === ENG.spool ? 0.15 + 0.6 * st[V.t] : 0), pitch: (st) => (st[V.state] === ENG.run ? 1 + 0.08 * st[V.out] : 0.3 + 0.7 * st[V.t]) },
      { sound: 'turbine.start', role: 'start', part, on: (st) => st[V.state] === ENG.spool },
      { sound: 'turbine.down', role: 'stop', part, on: (st) => st[V.state] !== ENG.run && st[V.state] !== ENG.spool },
      { sound: 'turbine.fail', role: 'fail', part, on: (st) => st[V.state] === ENG.fail },
    ];
  }

  alerts(): AlertDef[] {
    return [{ id: `fail.${this.part.id}`, label: `${this.part.name.toUpperCase()}: FALLO`, level: 1, lamp: this.part.lamp ?? this.part.name.toUpperCase(), help: 'La APU no arrancó o se paró (sin energía de arranque, sin propelente o dañada). El fallo se queda: apágala, arregla la causa y vuelve a encenderla.', on: (st) => st[this.v.state] === ENG.fail }];
  }
}

export const apuSystem: SystemFactory = {
  id: 'apu',
  parts: ['apu'],
  make: (sys) => partsOf(sys, 'apu').map((p) => new Apu(sys, p)),
};
