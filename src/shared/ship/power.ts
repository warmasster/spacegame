// Electrical grid: sources (reactor, APU, battery) feed one main bus; every circuit (breaker +
// conduit + priority selector) takes what its loads ask for. When the sources can't cover the
// demand the lowest priority circuits are shed first, and a circuit drawing more than its breaker
// rating for two seconds trips it (someone has to go and reset it at the breaker cabinet).

import type { ShipDef, SubsystemId } from './def.js';
import type { VarTable } from './state.js';

/** Battery bank (game scale: minutes, not hours). */
export const BATTERY = { kwh: 12, maxOut: 30, maxIn: 10 };
export const PRIORITY_LABELS = ['ALTA', 'NORMAL', 'BAJA'];
/** Seconds above the rating before a breaker trips. */
const TRIP_DELAY = 2;

export interface PowerSources {
  /** Available now (kW). */
  reactor: number;
  apu: number;
  /** Battery connected (switch) and its health factor 0..1. */
  battery: boolean;
  batteryHealth: number;
  /** Reactor condition 0..1 (grid quality when it carries the load). */
  reactorQuality: number;
}

export class PowerGrid {
  private f: Record<string, number> = {};
  private kw: Record<string, number> = {};
  private over: Record<string, number> = {};
  readonly iGen: number;
  readonly iLoad: number;
  readonly iDemand: number;
  readonly iSoc: number;
  readonly iBatKw: number;
  readonly iQuality: number;
  readonly iShed: number;
  readonly iRx: number;
  readonly iApu: number;

  constructor(
    private def: ShipDef,
    vars: VarTable,
  ) {
    this.iGen = vars.define('pwr.gen', 0.1);
    this.iLoad = vars.define('pwr.load', 0.1);
    this.iDemand = vars.define('pwr.demand', 0.1);
    this.iSoc = vars.define('bat.soc', 0.002, 0.92);
    this.iBatKw = vars.define('bat.kw', 0.1);
    this.iQuality = vars.define('grid.q', 0.01, 1);
    this.iShed = vars.define('pwr.shed', 1);
    this.iRx = vars.define('pwr.rx', 0.1);
    this.iApu = vars.define('pwr.apu', 0.1);
    for (const c of def.subsystems) {
      this.f[c.id] = vars.define(`ckt.${c.id}.f`, 0.02, 1);
      this.kw[c.id] = vars.define(`ckt.${c.id}.kw`, 0.05);
      this.over[c.id] = vars.define(`ckt.${c.id}.over`, -1);
    }
  }

  /** Supply fraction index of a circuit. */
  fIndex(id: SubsystemId) {
    return this.f[id];
  }

  /**
   * One step. `demand` = kW each circuit's loads ask for; `live(c)` = breaker closed and conduit
   * intact. Writes supply fractions and battery state; returns the circuits that tripped.
   */
  step(dt: number, st: Float64Array, sw: Record<string, number>, demand: Record<string, number>, live: (id: SubsystemId) => boolean, src: PowerSources): SubsystemId[] {
    const tripped: SubsystemId[] = [];
    const circuits = this.def.subsystems;
    // overload protection first: a breaker that trips sheds its load this very step
    for (const c of circuits) {
      const want = live(c.id) ? demand[c.id] ?? 0 : 0;
      const o = this.over[c.id];
      st[o] = want > c.rating ? st[o] + dt : Math.max(0, st[o] - dt);
      if (st[o] > TRIP_DELAY && sw[c.breaker] === 1) {
        sw[c.breaker] = 0;
        st[o] = 0;
        tripped.push(c.id);
      }
    }
    // demand per priority tier
    const tiers: SubsystemId[][] = [[], [], []];
    let total = 0;
    for (const c of circuits) {
      if (!live(c.id)) continue;
      const tier = Math.max(0, Math.min(2, Math.round(sw[c.priority] ?? 1)));
      tiers[tier].push(c.id);
      total += demand[c.id] ?? 0;
    }
    const gen = src.reactor + src.apu;
    const soc = st[this.iSoc];
    const batCap = src.battery && soc > 0.001 ? BATTERY.maxOut * src.batteryHealth : 0;
    let avail = gen + Math.min(batCap, Math.max(0, total - gen));
    // serve ALTA, then NORMAL, then BAJA
    let shed = 0;
    for (const c of circuits) st[this.f[c.id]] = 0;
    for (const tier of tiers) {
      const want = tier.reduce((a, id) => a + (demand[id] ?? 0), 0);
      const k = want <= 1e-6 ? 1 : Math.max(0, Math.min(1, avail / want));
      if (k < 0.999) shed++;
      for (const id of tier) st[this.f[id]] = k;
      avail = Math.max(0, avail - want * k);
    }
    // a live circuit with no demand still counts as energised if any source is up
    const anySource = gen > 0.05 || batCap > 0;
    let served = 0;
    for (const c of circuits) {
      if (!live(c.id) || !anySource) st[this.f[c.id]] = 0;
      else if ((demand[c.id] ?? 0) <= 1e-6) st[this.f[c.id]] = 1;
      st[this.kw[c.id]] = (demand[c.id] ?? 0) * st[this.f[c.id]];
      served += st[this.kw[c.id]];
    }
    // battery: covers the deficit, charges from the surplus
    const batKw = Math.max(-BATTERY.maxIn, Math.min(batCap, served - gen));
    const charge = src.battery && batKw < 0 && soc < 1 ? batKw : Math.max(0, batKw);
    st[this.iSoc] = Math.max(0, Math.min(1, soc - (charge > 0 ? charge : charge * 0.9) * (dt / 3600) / (BATTERY.kwh * Math.max(0.2, src.batteryHealth))));
    st[this.iBatKw] = charge;
    st[this.iGen] = gen;
    st[this.iLoad] = served;
    st[this.iDemand] = total;
    st[this.iShed] = shed > 0 && total > 0 ? 1 : 0;
    // grid quality: a damaged reactor carrying the load makes dirty power (ripple, sags)
    const share = served > 0 ? Math.min(1, src.reactor / Math.max(served, 1e-6)) : 0;
    st[this.iQuality] = 1 - share * (1 - src.reactorQuality);
    return tripped;
  }
}
