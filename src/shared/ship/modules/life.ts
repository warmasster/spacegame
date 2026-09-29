// Life support around the atmosphere network (atmos.ts): O2 generators, CO2 scrubbers and gas
// bottles are parts; pressure control mode, fans, heaters, manual repress valves and the
// recovery compressor are switches named in `def.life`. Gas paths come from the definition
// (doors / ramp open with their mover, vents with their valve, ducts with their damper) and from
// the hull (blown-out panels, cracks). Doors and ramps refuse to open across a pressure difference.
// Seat umbilicals feed docked suits from the O2 bottles.

import { facing, partKey, type ControlDef, type PartDef } from '../def.js';
import type { V3 } from '../geom.js';
import type { ShipSystems } from '../systems.js';
import { panelArea, partsOf, type AlertDef, type ShipModule, type SoundCue, type SystemFactory, type Tick } from './api.js';
import { Atmosphere, BREATHABLE, type GasPath, type GasStore, type LifeInput } from './atmos.js';

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

/**
 * The bottles as the atmosphere sees them this tick: only open, intact ones deliver. One per ship,
 * pointed at the tick's tables before each solve (no closures per tick).
 */
class BottleStore implements GasStore {
  st: Float64Array = new Float64Array(0);
  sw: Record<string, number> = {};
  powered = false;

  constructor(
    private sys: ShipSystems,
    private bottles: Bottle[],
  ) {}

  private usable(b: Bottle) {
    return this.powered && this.sw[b.valve] === 1 && this.sys.health(this.st, b.part) > 0 && this.st[b.kg] > 0;
  }

  take(gas: 'o2' | 'n2', kg: number) {
    const st = this.st;
    let have = 0;
    for (const b of this.bottles) if (b.gas === gas && this.usable(b)) have += st[b.kg];
    if (have <= 0 || kg <= 0) return 0;
    const out = Math.min(kg, have);
    for (const b of this.bottles) if (b.gas === gas && this.usable(b)) st[b.kg] -= out * (st[b.kg] / have);
    return out;
  }

  put(gas: 'o2' | 'n2', kg: number) {
    const st = this.st;
    let room = 0;
    for (const b of this.bottles) if (b.gas === gas && this.sys.health(st, b.part) > 0) room += Math.max(0, b.cap - st[b.kg]);
    if (room <= 0 || kg <= 0) return 0;
    const put = Math.min(kg, room);
    for (const b of this.bottles) if (b.gas === gas && this.sys.health(st, b.part) > 0) st[b.kg] += put * (Math.max(0, b.cap - st[b.kg]) / room);
    return put;
  }

  room() {
    let r = 0;
    for (const b of this.bottles) r += Math.max(0, b.cap - this.st[b.kg]);
    return r;
  }
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
  /** Each panel's normal turned the way its gas goes when it breaks: out of its zone (into `other`, or out of the ship). */
  private out: V3[];
  /** Reused every solve: the bottles, what the atmosphere is given, run switches, repress valve keys. */
  private store: BottleStore;
  private lifeIn: LifeInput;
  private genRun: string[];
  private scrubRun: string[];
  private repressKey: string[];
  /** Compartment index of each opening's sides and of each panel's zone / other side. */
  private opA: Int32Array;
  private opB: Int32Array;
  private pnA: Int32Array;
  private pnB: Int32Array;
  private pnArea: Float64Array;
  /** Gas paths handed out by `paths` (reused: the result is valid until the next call). */
  private pool: GasPath[] = [];
  private pathOut: GasPath[] = [];

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
    this.out = def.panels.map((p) => facing(def.zones, p.zone, p.other ?? null, p.c, p.n));
    this.store = new BottleStore(sys, this.bottles);
    this.genRun = this.gens.map((g) => partKey(g, 'run', g.id));
    this.scrubRun = this.scrubs.map((s) => partKey(s, 'run', s.id));
    this.repressKey = def.compartments.map((comp) => `${def.life?.repress ?? 'repress.'}${comp.id}`);
    this.lifeIn = {
      o2gen: this.gens.map((g) => ({ comp: sys.compIndex(g.zone), rate: 0 })),
      scrub: this.scrubs.map((s) => ({ comp: sys.compIndex(s.zone), eff: 0 })),
      mode: 2,
      manual: def.compartments.map(() => false),
      gas: this.store,
      recover: -1,
      crew: [],
      heat: false,
      hold: def.compartments.map(() => false),
    };
    this.opA = Int32Array.from(def.openings, (o) => sys.compIndex(o.a));
    this.opB = Int32Array.from(def.openings, (o) => (o.b === null ? -1 : sys.compIndex(o.b)));
    this.pnA = Int32Array.from(def.panels, (p) => sys.compIndex(p.zone));
    this.pnB = Int32Array.from(def.panels, (p) => (p.other !== undefined ? sys.compIndex(p.other) : -1));
    this.pnArea = Float64Array.from(def.panels, (p) => panelArea(p.poly));
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
    let warm = 0;
    for (let i = 0; i < this.atmos.v.length; i++) if (t.st[this.atmos.v[i].p] > 5) warm++;
    t.load(c.circuit, (c.heatKw ?? 0.6) * warm);
  }

  solve(t: Tick) {
    const { dt, st, sw, ctx } = t;
    const sys = this.sys;
    const c = this.cfg;
    const L = this.lifeIn;
    const lifeOk = !!c && this.powered(st);
    const q2 = sys.power ? st[sys.power.iQuality] ** 2 : 1;
    let o2 = 0;
    for (let k = 0; k < this.gens.length; k++) {
      const g = this.gens[k];
      const e = L.o2gen[k];
      e.rate = sw[this.genRun[k]] === 1 && sys.supply(st, g.circuit) >= 0.5 ? (g.p.rate ?? 0.125) * sys.health(st, g) * q2 : 0;
      o2 += e.rate;
    }
    st[this.iO2] = o2;
    let best = 0;
    for (let k = 0; k < this.scrubs.length; k++) {
      const s = this.scrubs[k];
      const e = L.scrub[k];
      e.eff = sw[this.scrubRun[k]] === 1 && sys.supply(st, s.circuit) >= 0.5 ? sys.health(st, s) * (s.p.rate ?? 1) : 0;
      best = Math.max(best, e.eff);
    }
    st[this.iScrub] = best;
    const paths = this.paths(st, sw, t, lifeOk && !!c && sw[c.fans] === 1);
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
    const gas = this.store;
    gas.st = st;
    gas.sw = sw;
    gas.powered = lifeOk;
    // seat umbilicals: docked suits drink from the O2 bottles (valve open, solenoids powered)
    let umb = false;
    if (lifeOk) {
      for (const b of this.bottles) {
        if (b.gas === 'o2' && sw[b.valve] === 1 && sys.health(st, b.part) > 0 && st[b.kg] > 0) {
          umb = true;
          break;
        }
      }
    }
    st[this.iUmb] = umb ? 1 : 0;
    if (ctx.docked > 0) gas.take('o2', ctx.docked * UMBILICAL_KGS * dt);
    L.recover = c?.recover && lifeOk && sw[c.recover.key] === 1 ? sys.compIndex(c.recover.zone) : -1;
    L.mode = c ? Math.round(sw[c.mode] ?? 0) : 2;
    const n = this.atmos.v.length;
    for (let i = 0; i < n; i++) {
      L.manual[i] = !!c && lifeOk && sw[this.repressKey[i]] === 1;
      L.hold![i] = t.held[i] === 1;
    }
    L.crew = ctx.crew;
    L.heat = !!c && sw[c.heat] === 1 && lifeOk;
    this.atmos.step(dt, st, paths, L);
    let leak = 0;
    for (let i = 0; i < n; i++) {
      const v = this.atmos.v[i];
      if (st[v.sealed] === 0 && st[v.p] > 2) {
        leak = 1;
        break;
      }
    }
    st[this.iLeak] = leak;
  }

  /** A path from the pool (every field set, so they all share one shape). */
  private path(k: number, a: number, b: number, area: number, mix: number | undefined, at: V3 | undefined, n: V3 | undefined, panel: number | undefined) {
    let g = this.pool[k];
    if (!g) {
      g = { a: 0, b: 0, area: 0, mix: undefined, at: undefined, n: undefined, panel: undefined };
      this.pool[k] = g;
    }
    g.a = a;
    g.b = b;
    g.area = area;
    g.mix = mix;
    g.at = at;
    g.n = n;
    g.panel = panel;
    this.pathOut.push(g);
  }

  /**
   * Gas paths open right now: doors / ramp / vents / ducts from the definition (with their place,
   * when they have one), breaches and cracks from the panels. Also read by ../airflow.ts. The array
   * and its paths are reused by the next call: read them right away.
   */
  paths(st: Float64Array, sw: Record<string, number>, hull: { hole(i: number): boolean; crack(i: number): number }, fans = false): GasPath[] {
    const sys = this.sys;
    const def = sys.def;
    this.pathOut.length = 0;
    let k = 0;
    for (let j = 0; j < def.openings.length; j++) {
      const o = def.openings[j];
      const a = this.opA[j];
      const b = this.opB[j];
      if (o.kind === 'duct') {
        if (sw[o.key] === 1) this.path(k++, a, b, o.area, fans ? 0.4 : 0, undefined, undefined, undefined);
        continue;
      }
      const open = o.kind === 'vent' ? (sw[o.key] === 1 ? 1 : 0) : sys.mover(st, o.key);
      if (open > 0) this.path(k++, a, b, o.area * open, undefined, o.at, o.n, undefined);
    }
    const panels = def.panels;
    for (let j = 0; j < panels.length; j++) {
      const a = this.pnA[j];
      if (a < 0) continue;
      const i = panels[j].index;
      const area = hull.hole(i) ? this.pnArea[j] : hull.crack(i);
      if (area > 0) this.path(k++, a, this.pnB[j], area, undefined, panels[j].c, this.out[i], i);
    }
    return this.pathOut;
  }

  /** Last refusal text per opening and the whole kPa it was written for (the displays ask ~20 times a second). */
  private refusal = new Map<string, { kpa: number; text: string }>();

  /** Doors and ramps don't open across a pressure difference (vents and ducts are guarded instead). */
  interlock(c: ControlDef, next: number, st: Float64Array) {
    if (next !== 1) return null;
    for (const o of this.sys.def.openings) {
      if (o.key !== c.key || (o.kind !== 'door' && o.kind !== 'ramp')) continue;
      const pa = this.pressure(st, o.a);
      const pb = this.pressure(st, o.b);
      if (Math.abs(pa - pb) <= DOOR_DP) continue;
      const kpa = Math.round(o.b !== null ? Math.abs(pa - pb) : pa);
      const hit = this.refusal.get(o.id);
      if (hit && hit.kpa === kpa) return hit.text;
      let text: string;
      if (o.b !== null) text = `Enclavamiento: diferencia de presión ${kpa} kPa`;
      else {
        const label = this.sys.def.compartments.find((x) => x.id === o.a)?.label.toLowerCase() ?? o.a;
        text = `Enclavamiento: ${label} presurizada (${kpa} kPa) — ventéala o recupera el aire`;
      }
      this.refusal.set(o.id, { kpa, text });
      return text;
    }
    return null;
  }

  /** The fans in every room with air, gas hissing in where it is fed, the recovery compressor. */
  sounds(): SoundCue[] {
    const sys = this.sys;
    const c = sys.def.life;
    if (!c) return [];
    const out: SoundCue[] = [];
    const powered = (st: Float64Array) => sys.supply(st, c.circuit) >= 0.5;
    for (const comp of sys.def.compartments) {
      out.push({ sound: 'mach.fan', role: 'fans', zone: comp.id, level: (st, sw) => (sw[c.fans] === 1 && powered(st) && sys.pressure(st, comp.id) > 5 ? 1 : 0) });
      const feed = `${comp.id}.feed`;
      if (sys.vars.has(feed)) {
        const i = sys.vars.idx(feed);
        out.push({ sound: 'air.hiss', role: 'feed', zone: comp.id, gain: 0.6, level: (st) => Math.min(1, st[i] * 0.5) });
      }
    }
    const r = c.recover;
    if (r) out.push({ sound: 'mach.compressor', role: 'recover', zone: r.zone, level: (st, sw) => (sw[r.key] === 1 && powered(st) ? 1 : 0) });
    // vent valves and ducts with no place of their own (the airflow jets sound the ones that have
    // one): gas roaring through the valve's pipe, as loud as the pressure across it
    for (const o of sys.def.openings) {
      if ((o.kind !== 'vent' && o.kind !== 'duct') || o.at) continue;
      const drop = (st: Float64Array) => Math.abs(sys.pressure(st, o.a) - (o.b ? sys.pressure(st, o.b) : 0));
      out.push({
        sound: o.kind === 'vent' ? 'air.vent' : 'air.rush',
        role: o.id,
        zone: o.a,
        level: (st, sw) => (sw[o.key] === 1 && drop(st) > 0.5 ? Math.min(1, 0.25 + Math.sqrt(drop(st) / 101)) : 0),
        pitch: (st) => 0.8 + 0.6 * Math.sqrt(Math.min(1, drop(st) / 101)),
      });
    }
    return out;
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
    if (this.bottles.length) {
      const low = (st: Float64Array, gas: 'o2' | 'n2') => {
        let kg = 0;
        let cap = 0;
        for (const b of this.bottles) {
          if (b.gas !== gas) continue;
          kg += st[b.kg];
          cap += b.cap;
        }
        return cap > 0 && kg < cap * 0.15;
      };
      out.push({
        id: 'gas',
        label: 'RESERVA DE O₂/N₂ BAJA',
        level: 1,
        lamp: 'GAS',
        help: 'Queda poco O₂ o N₂ en las botellas: la represurización y los umbilicales de los asientos dependen de ellas. Recupera el aire con el compresor en vez de ventearlo.',
        on: (st) => low(st, 'o2') || low(st, 'n2'),
      });
    }
    if (this.gens.length || this.scrubs.length) {
      const bad = (p: PartDef, key: string, sw: Record<string, number>, st: Float64Array) => sw[key] !== 1 || this.sys.health(st, p) < 0.5;
      out.push({
        id: 'life',
        label: 'SOPORTE VITAL DEGRADADO',
        level: 1,
        lamp: 'SOP VITAL',
        help: 'El generador de O₂ o el depurador de CO₂ están apagados, sin energía o dañados. Enciéndelos, comprueba el circuito SOPORTE VITAL y suelda la máquina.',
        on: (st, sw) => {
          for (let k = 0; k < this.gens.length; k++) if (bad(this.gens[k], this.genRun[k], sw, st)) return true;
          if (this.gens.length > 0 && st[this.iO2] < 0.06) return true;
          for (let k = 0; k < this.scrubs.length; k++) if (bad(this.scrubs[k], this.scrubRun[k], sw, st)) return true;
          return false;
        },
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
