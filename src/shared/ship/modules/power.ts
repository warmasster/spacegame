// Electrical grid: sources (whatever modules offer: reactors, APUs…) and battery parts feed one
// main bus; every circuit (breaker + conduit + priority selector) takes what its loads ask for.
// When the sources can't cover the demand the lowest priority circuits are shed first, and a
// circuit drawing more than its breaker rating for two seconds trips it (someone has to go and
// reset it at the breaker cabinet). Sources are dispatched in their `order` (base load first), the
// batteries cover the rest and charge from the surplus.
//
// Also here: the switched loads of the ship (LoadDef: lights, pumps, radar…) and the short
// circuits of badly damaged machines that are switched on.

import { partKey, partTag, type PartDef, type SubsystemId } from '../def.js';
import type { ShipSystems } from '../systems.js';
import type { VarTable } from '../state.js';
import { partsOf, type AlertDef, type ShipModule, type SystemFactory, type Tick } from './api.js';

/** Battery bank defaults (game scale: minutes, not hours); a battery part overrides them in `p`. */
export const BATTERY = { kwh: 12, maxOut: 30, maxIn: 10, soc: 0.92 };
export const PRIORITY_LABELS = ['ALTA', 'NORMAL', 'BAJA'];
/** Seconds above the rating before a breaker trips. */
const TRIP_DELAY = 2;

interface Bat {
  part: PartDef;
  key: string;
  kwh: number;
  maxOut: number;
  maxIn: number;
  iSoc: number;
}

export class PowerGrid implements ShipModule {
  readonly id = 'power';
  readonly order = 10;
  private f: Record<string, number> = {};
  private kw: Record<string, number> = {};
  private over: Record<string, number> = {};
  private bats: Bat[];
  /** kW each source delivered in the last solve (read back by the source modules in `step`). */
  private delivered = new Map<string, number>();
  private offered = new Map<string, number>();
  readonly iGen: number;
  readonly iLoad: number;
  readonly iDemand: number;
  readonly iSoc: number;
  readonly iBatKw: number;
  readonly iQuality: number;
  readonly iShed: number;
  /** Any source up (reactor, APU or a charged battery connected): cut conduits arc. */
  readonly iLive: number;

  constructor(
    private sys: ShipSystems,
    vars: VarTable,
  ) {
    const def = sys.def;
    this.bats = partsOf(sys, 'battery').map((p) => ({
      part: p,
      key: partKey(p, 'on', `${p.id}.on`),
      kwh: p.p.kwh ?? BATTERY.kwh,
      maxOut: p.p.maxOut ?? BATTERY.maxOut,
      maxIn: p.p.maxIn ?? BATTERY.maxIn,
      iSoc: vars.define(`${partTag(p)}.soc`, 0.002, p.p.soc ?? BATTERY.soc),
    }));
    this.iGen = vars.define('pwr.gen', 0.1);
    this.iLoad = vars.define('pwr.load', 0.1);
    this.iDemand = vars.define('pwr.demand', 0.1);
    const kwh = this.bats.reduce((a, b) => a + b.kwh, 0);
    this.iSoc = vars.define('bat.soc', 0.002, kwh > 0 ? this.bats.reduce((a, b) => a + b.kwh * (b.part.p.soc ?? BATTERY.soc), 0) / kwh : 0);
    this.iBatKw = vars.define('bat.kw', 0.1);
    this.iQuality = vars.define('grid.q', 0.01, 1);
    this.iShed = vars.define('pwr.shed', 1);
    this.iLive = vars.define('pwr.live', 1, 1);
    for (const c of def.subsystems) {
      this.f[c.id] = vars.define(`ckt.${c.id}.f`, 0.02, 1);
      this.kw[c.id] = vars.define(`ckt.${c.id}.kw`, 0.05);
      this.over[c.id] = vars.define(`ckt.${c.id}.over`, -1);
    }
    sys.provide('power', this);
  }

  /** Supply fraction index of a circuit. */
  fIndex(id: SubsystemId) {
    return this.f[id];
  }

  /** kW a source delivered last tick. */
  deliveredBy(id: string) {
    return this.delivered.get(id) ?? 0;
  }

  get batteries(): readonly Bat[] {
    return this.bats;
  }

  private connected(b: Bat, st: Float64Array, sw: Record<string, number>) {
    return sw[b.key] === 1 && this.sys.health(st, b.part) > 0;
  }

  /**
   * Starting power for a machine that needs it (reactor, APU): a connected battery with charge, or
   * another source that was up last tick. `self` = the source asking (it doesn't count).
   */
  canStart(st: Float64Array, sw: Record<string, number>, self: string) {
    if (this.bats.some((b) => this.connected(b, st, sw) && st[b.iSoc] > 0.03)) return true;
    for (const [id, kw] of this.offered) if (id !== self && kw > 0.05) return true;
    return false;
  }

  solve(t: Tick) {
    const { dt, st, sw, demand } = t;
    const circuits = this.sys.def.subsystems;
    const live = (id: SubsystemId) => this.sys.circuitLive(id, sw, t.hole);
    // overload protection first: a breaker that trips sheds its load this very step
    for (const c of circuits) {
      const want = live(c.id) ? demand[c.id] ?? 0 : 0;
      const o = this.over[c.id];
      st[o] = want > c.rating ? st[o] + dt : Math.max(0, st[o] - dt);
      if (st[o] > TRIP_DELAY && sw[c.breaker] === 1) {
        t.setSw(c.breaker, 0);
        st[o] = 0;
        t.emit({ type: 'trip', circuit: c.id });
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
    const sources = [...t.sources].sort((a, b) => a.order - b.order);
    this.offered.clear();
    for (const s of sources) this.offered.set(s.id, s.kw);
    const gen = sources.reduce((a, s) => a + s.kw, 0);
    const cap = this.bats.map((b) => (this.connected(b, st, sw) && st[b.iSoc] > 0.001 ? b.maxOut * this.sys.health(st, b.part) : 0));
    const batCap = cap.reduce((a, c) => a + c, 0);
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
    // dispatch: base-load sources first, what they don't cover comes from the batteries
    let rest = served;
    let dirty = 0;
    this.delivered.clear();
    for (const s of sources) {
      const d = Math.min(s.kw, rest);
      rest -= d;
      this.delivered.set(s.id, d);
      if (served > 0) dirty += (d / served) * (1 - s.quality);
    }
    // batteries: cover the deficit in proportion to what each can give, charge from the surplus
    const chargeable = this.bats.map((b) => (this.connected(b, st, sw) && st[b.iSoc] < 1 ? b.maxIn : 0));
    const inCap = chargeable.reduce((a, c) => a + c, 0);
    const batKw = Math.max(-inCap, Math.min(batCap, served - gen));
    let kwhSum = 0;
    let socSum = 0;
    this.bats.forEach((b, i) => {
      const share = batKw > 0 ? (batCap > 0 ? (batKw * cap[i]) / batCap : 0) : inCap > 0 ? (batKw * chargeable[i]) / inCap : 0;
      const h = Math.max(0.2, this.sys.health(st, b.part));
      st[b.iSoc] = Math.max(0, Math.min(1, st[b.iSoc] - (share > 0 ? share : share * 0.9) * (dt / 3600) / (b.kwh * h)));
      kwhSum += b.kwh;
      socSum += b.kwh * st[b.iSoc];
    });
    st[this.iSoc] = kwhSum > 0 ? socSum / kwhSum : 0;
    st[this.iBatKw] = batKw;
    st[this.iGen] = gen;
    st[this.iLoad] = served;
    st[this.iDemand] = total;
    st[this.iShed] = shed > 0 && total > 0 ? 1 : 0;
    st[this.iLive] = anySource ? 1 : 0;
    // grid quality: a damaged reactor carrying the load makes dirty power (ripple, sags)
    st[this.iQuality] = 1 - Math.min(1, dirty);
  }

  alerts(): AlertDef[] {
    const out: AlertDef[] = [];
    if (this.bats.length) {
      out.push({ id: 'bat', label: 'BATERÍA BAJA', level: 1, lamp: 'BATERÍA', help: 'Queda menos del 20 % de carga. Sin batería no podrás volver a arrancar el reactor ni la APU si se paran. Déjala conectada con el reactor en marcha: se recarga con lo que sobre.', on: (st) => st[this.iSoc] < 0.2 });
      out.push({ id: 'onbat', label: 'EN BATERÍA', level: 1, lamp: 'BATERÍA', help: 'La batería está cubriendo consumo: los generadores no llegan (reactor parado o al mínimo, APU apagada) o hay un pico. Sube la salida del reactor, arranca la APU o apaga cargas.', on: (st) => st[this.iBatKw] > 0.5 });
    }
    if (this.sys.def.subsystems.length) {
      out.push({ id: 'shed', label: 'DESLASTRE DE CARGAS', level: 1, lamp: 'DESLASTRE', help: 'No hay energía para todo: los circuitos de prioridad BAJA (y luego NORMAL) reciben menos de lo que piden y sus máquinas se paran. Genera más o apaga lo que no necesites; cambia prioridades en el pedestal.', on: (st) => st[this.iShed] === 1 });
      out.push({ id: 'trip', label: 'DISYUNTOR ABIERTO', level: 1, lamp: 'DISYUNTOR', help: 'Algún disyuntor del armario del pasillo está abierto (saltó por sobrecarga o alguien lo bajó). La página ENERG dice cuál. Quita la causa y súbelo.', on: (_, sw) => this.sys.def.subsystems.some((c) => sw[c.breaker] !== 1) });
    }
    return out;
  }
}

/** Switched loads from the definition, plus short circuits of wrecked machines left on. */
export class SwitchedLoads implements ShipModule {
  readonly id = 'loads';

  constructor(private sys: ShipSystems) {}

  loads(t: Tick) {
    for (const l of this.sys.def.loads) {
      const v = t.sw[l.key] ?? 0;
      const kw = Array.isArray(l.kw) ? l.kw[Math.round(v)] ?? 0 : v > 0 ? l.kw : 0;
      if (kw <= 0) continue;
      t.load(l.circuit, kw);
      // a badly damaged machine that is switched on draws erratically (and may trip its breaker)
      const part = l.part ? this.sys.part(l.part) : undefined;
      if (!part) continue;
      const h = this.sys.health(t.st, part);
      if (h > 0 && h < 0.25) t.load(l.circuit, kw * (1.5 + 2.5 * t.ctx.rand()));
    }
  }
}

export const powerSystem: SystemFactory = {
  id: 'power',
  parts: ['battery'],
  make: (sys) => (sys.def.subsystems.length || partsOf(sys, 'battery').length ? [new PowerGrid(sys, sys.vars), new SwitchedLoads(sys)] : []),
};
