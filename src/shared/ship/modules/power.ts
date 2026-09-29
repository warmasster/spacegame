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
import { partsOf, type AlertDef, type PowerSource, type ShipModule, type SoundCue, type SystemFactory, type Tick } from './api.js';

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
  /** The same variables by circuit index (order of `def.subsystems`). */
  private cF: Int32Array;
  private cKw: Int32Array;
  private cOver: Int32Array;
  /** Scratch of one solve: circuit live, its priority tier (-1: dead), battery limits, sorted sources. */
  private live: Uint8Array;
  private tierOf: Int8Array;
  private cap: Float64Array;
  private chg: Float64Array;
  private src: PowerSource[] = [];
  private bats: Bat[];
  /** Sources offered in the last solve (dispatch order), what each offered and what it delivered (kW). */
  private offId: string[] = [];
  private offKw: number[] = [];
  private delKw: number[] = [];
  private nOff = 0;
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
    const n = def.subsystems.length;
    this.cF = Int32Array.from(def.subsystems, (c) => this.f[c.id]);
    this.cKw = Int32Array.from(def.subsystems, (c) => this.kw[c.id]);
    this.cOver = Int32Array.from(def.subsystems, (c) => this.over[c.id]);
    this.live = new Uint8Array(n);
    this.tierOf = new Int8Array(n);
    this.cap = new Float64Array(this.bats.length);
    this.chg = new Float64Array(this.bats.length);
    sys.provide('power', this);
  }

  /** Supply fraction index of a circuit. */
  fIndex(id: SubsystemId) {
    return this.f[id];
  }

  /** kW a source delivered last tick. */
  deliveredBy(id: string) {
    for (let i = 0; i < this.nOff; i++) if (this.offId[i] === id) return this.delKw[i];
    return 0;
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
    for (const b of this.bats) if (this.connected(b, st, sw) && st[b.iSoc] > 0.03) return true;
    for (let i = 0; i < this.nOff; i++) if (this.offId[i] !== self && this.offKw[i] > 0.05) return true;
    return false;
  }

  solve(t: Tick) {
    const { dt, st, sw, demand } = t;
    const sys = this.sys;
    const circuits = sys.def.subsystems;
    const n = circuits.length;
    // overload protection first: a breaker that trips sheds its load this very step
    for (let k = 0; k < n; k++) {
      const c = circuits[k];
      const want = sys.circuitLive(c.id, sw, t.hole) ? demand[k] : 0;
      const o = this.cOver[k];
      st[o] = want > c.rating ? st[o] + dt : Math.max(0, st[o] - dt);
      if (st[o] > TRIP_DELAY && sw[c.breaker] === 1) {
        t.setSw(c.breaker, 0);
        st[o] = 0;
        t.emit({ type: 'trip', circuit: c.id });
      }
    }
    // demand per priority tier
    const live = this.live;
    const tierOf = this.tierOf;
    let total = 0;
    for (let k = 0; k < n; k++) {
      const c = circuits[k];
      if (!sys.circuitLive(c.id, sw, t.hole)) {
        live[k] = 0;
        tierOf[k] = -1;
        continue;
      }
      live[k] = 1;
      tierOf[k] = Math.max(0, Math.min(2, Math.round(sw[c.priority] ?? 1)));
      total += demand[k];
    }
    // sources in dispatch order (insertion sort: stable, and no garbage)
    const src = this.src;
    src.length = 0;
    for (const s of t.sources) {
      let j = src.length;
      src.push(s);
      while (j > 0 && src[j - 1].order > s.order) {
        src[j] = src[j - 1];
        j--;
      }
      src[j] = s;
    }
    let gen = 0;
    this.nOff = src.length;
    for (let i = 0; i < src.length; i++) {
      this.offId[i] = src[i].id;
      this.offKw[i] = src[i].kw;
      gen += src[i].kw;
    }
    const cap = this.cap;
    let batCap = 0;
    for (let i = 0; i < this.bats.length; i++) {
      const b = this.bats[i];
      cap[i] = this.connected(b, st, sw) && st[b.iSoc] > 0.001 ? b.maxOut * sys.health(st, b.part) : 0;
      batCap += cap[i];
    }
    let avail = gen + Math.min(batCap, Math.max(0, total - gen));
    // serve ALTA, then NORMAL, then BAJA
    let shed = 0;
    for (let k = 0; k < n; k++) st[this.cF[k]] = 0;
    for (let tier = 0; tier < 3; tier++) {
      let want = 0;
      for (let k = 0; k < n; k++) if (tierOf[k] === tier) want += demand[k];
      const f = want <= 1e-6 ? 1 : Math.max(0, Math.min(1, avail / want));
      if (f < 0.999) shed++;
      for (let k = 0; k < n; k++) if (tierOf[k] === tier) st[this.cF[k]] = f;
      avail = Math.max(0, avail - want * f);
    }
    // a live circuit with no demand still counts as energised if any source is up
    const anySource = gen > 0.05 || batCap > 0;
    let served = 0;
    for (let k = 0; k < n; k++) {
      const f = this.cF[k];
      if (!live[k] || !anySource) st[f] = 0;
      else if (demand[k] <= 1e-6) st[f] = 1;
      st[this.cKw[k]] = demand[k] * st[f];
      served += st[this.cKw[k]];
    }
    // dispatch: base-load sources first, what they don't cover comes from the batteries
    let rest = served;
    let dirty = 0;
    for (let i = 0; i < src.length; i++) {
      const s = src[i];
      const d = Math.min(s.kw, rest);
      rest -= d;
      this.delKw[i] = d;
      if (served > 0) dirty += (d / served) * (1 - s.quality);
    }
    // batteries: cover the deficit in proportion to what each can give, charge from the surplus
    const chg = this.chg;
    let inCap = 0;
    for (let i = 0; i < this.bats.length; i++) {
      const b = this.bats[i];
      chg[i] = this.connected(b, st, sw) && st[b.iSoc] < 1 ? b.maxIn : 0;
      inCap += chg[i];
    }
    const batKw = Math.max(-inCap, Math.min(batCap, served - gen));
    let kwhSum = 0;
    let socSum = 0;
    for (let i = 0; i < this.bats.length; i++) {
      const b = this.bats[i];
      const share = batKw > 0 ? (batCap > 0 ? (batKw * cap[i]) / batCap : 0) : inCap > 0 ? (batKw * chg[i]) / inCap : 0;
      const h = Math.max(0.2, sys.health(st, b.part));
      st[b.iSoc] = Math.max(0, Math.min(1, st[b.iSoc] - (share > 0 ? share : share * 0.9) * (dt / 3600) / (b.kwh * h)));
      kwhSum += b.kwh;
      socSum += b.kwh * st[b.iSoc];
    }
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

  /** The main bus at every battery: relays dropping out when the ship goes dark, closing when it comes back, shedding loads. */
  sounds(): SoundCue[] {
    const out: SoundCue[] = [];
    const live = this.iLive;
    const shed = this.iShed;
    for (const b of this.bats) {
      const part = b.part;
      out.push({ sound: 'power.down', role: 'down', part, on: (st) => st[live] !== 1 });
      out.push({ sound: 'power.up', role: 'up', part, on: (st) => st[live] === 1 });
      out.push({ sound: 'relay.clack', role: 'shed', part, on: (st) => st[shed] === 1 });
      // a battery delivering hard buzzes
      const soc = b.iSoc;
      out.push({ sound: 'mach.hum', role: 'run', part, gain: 0.35, level: (st) => (st[live] === 1 && st[soc] > 0.01 ? 0.3 + Math.min(0.7, Math.abs(st[this.iBatKw]) / Math.max(1, b.maxOut)) : 0), pitch: () => 1.5 });
    }
    return out;
  }

  alerts(): AlertDef[] {
    const out: AlertDef[] = [];
    if (this.bats.length) {
      out.push({ id: 'bat', label: 'BATERÍA BAJA', level: 1, lamp: 'BATERÍA', help: 'Queda menos del 20 % de carga. Sin batería no podrás volver a arrancar el reactor ni la APU si se paran. Déjala conectada con el reactor en marcha: se recarga con lo que sobre.', on: (st) => st[this.iSoc] < 0.2 });
      out.push({ id: 'onbat', label: 'EN BATERÍA', level: 1, lamp: 'BATERÍA', help: 'La batería está cubriendo consumo: los generadores no llegan (reactor parado o al mínimo, APU apagada) o hay un pico. Sube la salida del reactor, arranca la APU o apaga cargas.', on: (st) => st[this.iBatKw] > 0.5 });
    }
    if (this.sys.def.subsystems.length) {
      out.push({ id: 'shed', label: 'DESLASTRE DE CARGAS', level: 1, lamp: 'DESLASTRE', help: 'No hay energía para todo: los circuitos de prioridad BAJA (y luego NORMAL) reciben menos de lo que piden y sus máquinas se paran. Genera más o apaga lo que no necesites; cambia prioridades en el pedestal.', on: (st) => st[this.iShed] === 1 });
      out.push({
        id: 'trip',
        label: 'DISYUNTOR ABIERTO',
        level: 1,
        lamp: 'DISYUNTOR',
        help: 'Algún disyuntor del armario del pasillo está abierto (saltó por sobrecarga o alguien lo bajó). La página ENERG dice cuál. Quita la causa y súbelo.',
        on: (_, sw) => {
          for (const c of this.sys.def.subsystems) if (sw[c.breaker] !== 1) return true;
          return false;
        },
      });
    }
    return out;
  }
}

/** Switched loads from the definition, plus short circuits of wrecked machines left on. */
export class SwitchedLoads implements ShipModule {
  readonly id = 'loads';
  /** The machine behind each load (looked up once). */
  private parts: Array<PartDef | undefined>;

  constructor(private sys: ShipSystems) {
    this.parts = sys.def.loads.map((l) => (l.part ? sys.part(l.part) : undefined));
  }

  loads(t: Tick) {
    const loads = this.sys.def.loads;
    for (let k = 0; k < loads.length; k++) {
      const l = loads[k];
      const v = t.sw[l.key] ?? 0;
      const kw = Array.isArray(l.kw) ? l.kw[Math.round(v)] ?? 0 : v > 0 ? l.kw : 0;
      if (kw <= 0) continue;
      t.load(l.circuit, kw);
      // a badly damaged machine that is switched on draws erratically (and may trip its breaker)
      const part = this.parts[k];
      if (!part) continue;
      const h = this.sys.health(t.st, part);
      if (h > 0 && h < 0.25) t.load(l.circuit, kw * (1.5 + 2.5 * t.ctx.rand()));
    }
  }

  /**
   * Every machine behind a switched load runs audibly while it is on and powered: its component's
   * `run` voice (a pump, a fan…), a hum by default. A module with its own `run` cue for that
   * machine replaces this one.
   */
  sounds(): SoundCue[] {
    const out: SoundCue[] = [];
    const sys = this.sys;
    const loads = sys.def.loads;
    for (let k = 0; k < loads.length; k++) {
      const part = this.parts[k];
      if (!part) continue;
      const l = loads[k];
      const top = Array.isArray(l.kw) ? Math.max(1, l.kw.length - 1) : 1;
      out.push({ sound: 'mach.hum', role: 'run', part, generic: true, level: (st, sw) => {
        const v = sw[l.key] ?? 0;
        return v > 0 && sys.supply(st, l.circuit) >= 0.5 ? 0.5 + 0.5 * Math.min(1, v / top) : 0;
      } });
    }
    return out;
  }
}

export const powerSystem: SystemFactory = {
  id: 'power',
  parts: ['battery'],
  make: (sys) => (sys.def.subsystems.length || partsOf(sys, 'battery').length ? [new PowerGrid(sys, sys.vars), new SwitchedLoads(sys)] : []),
};
