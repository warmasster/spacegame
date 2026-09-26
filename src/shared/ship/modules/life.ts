// Life support around the atmosphere network (atmos.ts): O2 generators, CO2 scrubbers and gas
// bottles are parts; pressure control mode, fans, heaters, manual repress valves and the
// recovery compressor are switches named in `def.life`. Gas paths come from the definition
// (doors / ramp open with their mover, vents with their valve, ducts with their damper) and from
// the hull (blown-out panels, cracks). Doors and ramps refuse to open across a pressure difference.
// Seat umbilicals feed docked suits from the O2 bottles.

import { partKey, type ControlDef, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import { panelArea, partsOf, type AlertDef, type ShipModule, type SystemFactory, type Tick } from './api.js';
import { Atmosphere, BREATHABLE, type GasPath, type GasStore } from './atmos.js';

/** Pressure difference (kPa) across a door or ramp above which it refuses to open. */
export const DOOR_DP = 5;
/** O2 a docked suit draws from the ship through the seat umbilical (kg/s). */
const UMBILICAL_KGS = 0.0004;

interface Bottle {
  part: PartDef;
  gas: 'o2' | 'n2';
  valve: string;
  kg: number;
  cap: number;
}

export class LifeSupport implements ShipModule {
  readonly id = 'life';
  readonly order = 30;
  readonly atmos: Atmosphere;
  readonly bottles: Bottle[];
  private gens: PartDef[];
  private scrubs: PartDef[];
  private iO2: number;
  private iScrub: number;
  private iLeak: number;
  /** Seat umbilicals can deliver O2 (1) or not (0). */
  readonly iUmb: number;

  constructor(private sys: ShipSystems) {
    const def = sys.def;
    const vars = sys.vars;
    this.atmos = new Atmosphere(def.compartments, vars);
    this.gens = partsOf(sys, 'o2gen');
    this.scrubs = partsOf(sys, 'scrubber');
    this.bottles = partsOf(sys, 'gas').map((p) => ({
      part: p,
      gas: p.p.gas === 1 ? 'n2' : 'o2',
      valve: partKey(p, 'valve', `v.${p.id}`),
      kg: vars.define(`${p.id}.kg`, 0.1, p.p.fill ?? p.p.cap),
      cap: p.p.cap,
    }));
    this.iO2 = vars.define('ls.o2', 0.002);
    this.iScrub = vars.define('ls.scrub', 0.01);
    this.iLeak = vars.define('ls.leak', 1);
    this.iUmb = vars.define('ls.umb', 1);
    sys.provide('life', this);
  }

  private get cfg() {
    return this.sys.def.life;
  }

  /** Pressure (kPa) of a compartment, 0 for outside. */
  pressure(st: Float64Array, zone: string | null) {
    const i = this.sys.compIndex(zone);
    return i < 0 ? 0 : st[this.atmos.v[i].p];
  }

  breathable(st: Float64Array, zone: string | null) {
    const i = this.sys.compIndex(zone);
    if (i < 0) return false;
    const v = this.atmos.v[i];
    return st[v.p] > BREATHABLE.p && st[v.po2] > BREATHABLE.po2 && st[v.pco2] < BREATHABLE.pco2;
  }

  /** Total gas left in the bottles of one kind, and their capacity (kg). */
  stock(st: Float64Array, gas: 'o2' | 'n2') {
    let kg = 0;
    let cap = 0;
    for (const b of this.bottles) {
      if (b.gas !== gas) continue;
      kg += st[b.kg];
      cap += b.cap;
    }
    return { kg, cap };
  }

  private powered(st: Float64Array) {
    return this.sys.supply(st, this.cfg?.circuit) >= 0.5;
  }

  /** The bottles as the atmosphere sees them this tick: only open, intact ones deliver. */
  private store(st: Float64Array, sw: Record<string, number>, powered: boolean): GasStore {
    const usable = (b: Bottle) => powered && sw[b.valve] === 1 && this.sys.health(st, b.part) > 0 && st[b.kg] > 0;
    return {
      take: (gas, kg) => {
        const from = this.bottles.filter((b) => b.gas === gas && usable(b));
        const have = from.reduce((a, b) => a + st[b.kg], 0);
        if (have <= 0 || kg <= 0) return 0;
        const out = Math.min(kg, have);
        for (const b of from) st[b.kg] -= out * (st[b.kg] / have);
        return out;
      },
      put: (gas, kg) => {
        const into = this.bottles.filter((b) => b.gas === gas && this.sys.health(st, b.part) > 0);
        const room = into.reduce((a, b) => a + Math.max(0, b.cap - st[b.kg]), 0);
        if (room <= 0 || kg <= 0) return 0;
        const put = Math.min(kg, room);
        for (const b of into) st[b.kg] += put * (Math.max(0, b.cap - st[b.kg]) / room);
        return put;
      },
      room: () => this.bottles.reduce((a, b) => a + Math.max(0, b.cap - st[b.kg]), 0),
    };
  }

  init(st: Float64Array) {
    this.sys.def.compartments.forEach((c, i) => {
      const p0 = this.sys.def.defaults[`${c.id}.p0`] ?? 0;
      if (p0 > 0) this.atmos.fill(st, i, p0);
    });
    // publish derived values once
    const none: GasStore = { take: () => 0, put: () => 0, room: () => 0 };
    this.atmos.step(0.001, st, [], { o2gen: [], scrub: [], mode: 2, manual: [], gas: none, recover: -1, crew: [], heat: true });
  }

  loads(t: Tick) {
    const c = this.cfg;
    if (!c || t.sw[c.heat] !== 1) return;
    const warm = this.sys.def.compartments.filter((_, i) => t.st[this.atmos.v[i].p] > 5).length;
    t.load(c.circuit, (c.heatKw ?? 0.6) * warm);
  }

  solve(t: Tick) {
    const { dt, st, sw, ctx } = t;
    const sys = this.sys;
    const c = this.cfg;
    const lifeOk = !!c && this.powered(st);
    const q2 = sys.power ? st[sys.power.iQuality] ** 2 : 1;
    const run = (p: PartDef, fallback: string) => sw[partKey(p, 'run', fallback)] === 1 && sys.supply(st, p.circuit) >= 0.5;
    const o2gen = this.gens.map((g) => ({ comp: sys.compIndex(g.zone), rate: run(g, g.id) ? (g.p.rate ?? 0.125) * sys.health(st, g) * q2 : 0 }));
    st[this.iO2] = o2gen.reduce((a, g) => a + g.rate, 0);
    const scrub = this.scrubs.map((s) => ({ comp: sys.compIndex(s.zone), eff: run(s, s.id) ? sys.health(st, s) * (s.p.rate ?? 1) : 0 }));
    st[this.iScrub] = scrub.reduce((a, s) => Math.max(a, s.eff), 0);
    const paths = this.gasPaths(t, lifeOk && !!c && sw[c.fans] === 1);
    // a holed bottle bleeds into its compartment
    for (const b of this.bottles) {
      const h = sys.health(st, b.part);
      if (h >= 0.4 || st[b.kg] <= 0) continue;
      const kg = Math.min(st[b.kg], (h <= 0 ? 2 : 0.25 * (1 - h / 0.4)) * dt);
      st[b.kg] -= kg;
      const ci = sys.compIndex(b.part.zone);
      if (ci >= 0) {
        const v = this.atmos.v[ci];
        if (b.gas === 'o2') st[v.o2] += kg / 0.032;
        else st[v.n2] += kg / 0.028;
      }
    }
    const gas = this.store(st, sw, lifeOk);
    // seat umbilicals: docked suits drink from the O2 bottles (valve open, solenoids powered)
    const umb = lifeOk && this.bottles.some((b) => b.gas === 'o2' && sw[b.valve] === 1 && sys.health(st, b.part) > 0 && st[b.kg] > 0);
    st[this.iUmb] = umb ? 1 : 0;
    if (ctx.docked > 0) gas.take('o2', ctx.docked * UMBILICAL_KGS * dt);
    const recover = c?.recover && lifeOk && sw[c.recover.key] === 1 ? sys.compIndex(c.recover.zone) : -1;
    this.atmos.step(dt, st, paths, {
      o2gen,
      scrub,
      mode: c ? Math.round(sw[c.mode] ?? 0) : 2,
      manual: sys.def.compartments.map((comp) => !!c && lifeOk && sw[`${c.repress ?? 'repress.'}${comp.id}`] === 1),
      gas,
      recover,
      crew: ctx.crew,
      heat: !!c && sw[c.heat] === 1 && lifeOk,
      hold: sys.def.compartments.map((comp) => t.holds.has(comp.id)),
    });
    st[this.iLeak] = sys.def.compartments.some((_, i) => st[this.atmos.v[i].sealed] === 0 && st[this.atmos.v[i].p] > 2) ? 1 : 0;
  }

  /** Doors / ramp / vents / ducts from the definition, breaches and cracks from the panels. */
  private gasPaths(t: Tick, fans: boolean): GasPath[] {
    const sys = this.sys;
    const out: GasPath[] = [];
    for (const o of sys.def.openings) {
      const a = sys.compIndex(o.a);
      const b = o.b === null ? -1 : sys.compIndex(o.b);
      if (o.kind === 'duct') {
        if (t.sw[o.key] === 1) out.push({ a, b, area: o.area, mix: fans ? 0.4 : 0 });
        continue;
      }
      const open = o.kind === 'vent' ? (t.sw[o.key] === 1 ? 1 : 0) : sys.mover(t.st, o.key);
      if (open > 0) out.push({ a, b, area: o.area * open });
    }
    for (const p of sys.def.panels) {
      const a = sys.compIndex(p.zone);
      if (a < 0) continue;
      const b = p.kind === 'bulkhead' ? sys.compIndex(p.other ?? null) : -1;
      const area = t.hole(p.index) ? panelArea(p.poly) : t.crack(p.index);
      if (area > 0) out.push({ a, b, area });
    }
    return out;
  }

  /** Doors and ramps don't open across a pressure difference (vents and ducts are guarded instead). */
  interlock(c: ControlDef, next: number, st: Float64Array) {
    if (next !== 1) return null;
    for (const o of this.sys.def.openings) {
      if (o.key !== c.key || (o.kind !== 'door' && o.kind !== 'ramp')) continue;
      const pa = this.pressure(st, o.a);
      const pb = this.pressure(st, o.b);
      if (Math.abs(pa - pb) <= DOOR_DP) continue;
      if (o.b !== null) return `Enclavamiento: diferencia de presión ${Math.abs(pa - pb).toFixed(0)} kPa`;
      const label = this.sys.def.compartments.find((x) => x.id === o.a)?.label.toLowerCase() ?? o.a;
      return `Enclavamiento: ${label} presurizada (${pa.toFixed(0)} kPa) — ventéala o recupera el aire`;
    }
    return null;
  }

  alerts(): AlertDef[] {
    const out: AlertDef[] = [];
    const comp = (i: number) => this.atmos.v[i];
    this.sys.def.compartments.forEach((c, i) => {
      out.push({ id: `depress.${c.id}`, label: `DESCOMPRESIÓN · ${c.label}`, level: 2, lamp: 'DESCOMP', help: 'La presión cae deprisa: una brecha, una puerta o la rampa abierta al vacío, o un venteo abierto. Cierra la abertura o sella el compartimento (puertas, compuertas) y busca la brecha.', on: (st) => st[comp(i).dpdt] < -1.5 });
      out.push({ id: `o2.${c.id}`, label: `O₂ BAJO · ${c.label}`, level: 1, lamp: 'O2', help: 'El oxígeno de ese compartimento está por debajo de lo respirable. Enciende el generador de O₂ y los ventiladores con las compuertas abiertas, o repón gas de las botellas.', on: (st) => st[comp(i).p] > 5 && st[comp(i).po2] < 16 });
      out.push({ id: `co2.${c.id}`, label: `CO₂ ALTO · ${c.label}`, level: 1, lamp: 'CO2', help: 'Se acumula el CO₂ que exhala la tripulación. El depurador está en la bodega: enciéndelo y deja los ventiladores y las compuertas abiertos para que llegue el aire.', on: (st) => st[comp(i).pco2] > 1 });
    });
    const cfg = this.cfg;
    if (cfg) out.push({ id: 'leak', label: 'FUGA DE AIRE · REPRESURIZACIÓN SUSPENDIDA', level: 1, lamp: 'FUGA AIRE', help: 'El control automático no mete gas en un compartimento abierto al vacío (lo tiraría). Cierra la abertura o repara la brecha; si hace falta aire ya, usa el modo MANUAL y su válvula.', on: (st, sw) => st[this.iLeak] === 1 && (sw[cfg.mode] ?? 0) === 0 });
    if (this.bottles.length)
      out.push({
        id: 'gas',
        label: 'RESERVA DE O₂/N₂ BAJA',
        level: 1,
        lamp: 'GAS',
        help: 'Queda poco O₂ o N₂ en las botellas: la represurización y los umbilicales de los asientos dependen de ellas. Recupera el aire con el compresor en vez de ventearlo.',
        on: (st) => (['o2', 'n2'] as const).some((g) => {
          const s = this.stock(st, g);
          return s.cap > 0 && s.kg < s.cap * 0.15;
        }),
      });
    if (this.gens.length || this.scrubs.length) {
      const bad = (p: PartDef, sw: Record<string, number>, st: Float64Array) => sw[partKey(p, 'run', p.id)] !== 1 || this.sys.health(st, p) < 0.5;
      out.push({
        id: 'life',
        label: 'SOPORTE VITAL DEGRADADO',
        level: 1,
        lamp: 'SOP VITAL',
        help: 'El generador de O₂ o el depurador de CO₂ están apagados, sin energía o dañados. Enciéndelos, comprueba el circuito SOPORTE VITAL y suelda la máquina.',
        on: (st, sw) => this.gens.some((g) => bad(g, sw, st)) || (this.gens.length > 0 && st[this.iO2] < 0.06) || this.scrubs.some((s) => bad(s, sw, st)),
      });
    }
    return out;
  }
}

export const lifeSystem: SystemFactory = {
  id: 'life',
  parts: ['o2gen', 'scrubber', 'gas'],
  make: (sys) => (sys.def.compartments.length ? [new LifeSupport(sys)] : []),
};
