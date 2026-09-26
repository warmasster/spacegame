// Fission reactor with its coolant loop: start-up sequence (needs coolant flow and starting power),
// output lever and set point, heat core → coolant (pumped) → radiators → space, automatic SCRAM on
// over-temperature or core instability, damage above the damage temperature. One module per
// reactor part; coolant pumps and radiators belong to the reactor they `link` to (default: the
// first reactor). Numbers come from the part's `p` (defaults in REACTOR).

import { partKey, partTag, type ControlDef, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import { partsOf, type AlertDef, type ShipModule, type SystemFactory, type Tick } from './api.js';

/** Output selector positions (fraction of rated power). */
export const REACTOR_SET = [0, 0.25, 0.5, 0.75, 1, 1.1];
/**
 * Defaults (the M reactor). Thermal model: core heat capacity `coreCap` (kJ/K), coolant `coolCap`,
 * core → coolant conductance `ua` (kW/K at full flow), pump heat into the loop `pumpHeat` (kW).
 * The catalog scales them with the rating so every size runs at the same temperatures.
 */
export const REACTOR = { kw: 60, heatKw: 100, startS: 15, startKw: 8, warnC: 600, scramC: 750, damageC: 820, resetC: 300, coreCap: 60, coolCap: 150, ua: 1.2, pumpHeat: 1.5 };
/** Reactor states. */
export const RX = { off: 0, starting: 1, online: 2, scram: 3 } as const;

export class Reactor implements ShipModule {
  readonly id: string;
  readonly tag: string;
  readonly keys: { run: string; set: string; scram: string; reset: string };
  readonly k: typeof REACTOR;
  readonly pumps: PartDef[];
  readonly radiators: PartDef[];
  private v: Record<string, number> = {};

  constructor(
    private sys: ShipSystems,
    readonly part: PartDef,
    first: boolean,
  ) {
    const tag = (this.tag = partTag(part));
    this.id = `reactor:${part.id}`;
    this.keys = { run: partKey(part, 'run', part.id), set: partKey(part, 'set', `${tag}.set`), scram: partKey(part, 'scram', `${tag}.scram`), reset: partKey(part, 'reset', `${tag}.reset`) };
    this.k = { ...REACTOR, ...part.p } as typeof REACTOR;
    const mine = (p: PartDef) => p.link === part.id || (!p.link && first);
    this.pumps = partsOf(sys, 'coolpump').filter(mine);
    this.radiators = partsOf(sys, 'radiator').filter(mine);
    const defaults = sys.def.defaults;
    const on = defaults[this.keys.run] === 1;
    const d = (name: string, q: number, init = 0) => (this.v[name] = sys.vars.define(`${tag}.${name}`, q, init));
    // starts at equilibrium: online at its selected output if the lever is up, cold otherwise
    const out0 = on ? REACTOR_SET[defaults[this.keys.set] ?? 2] ?? 0.5 : 0;
    const eq = this.equilibrium(out0, (r) => defaults[partKey(r, 'deploy', `${r.id}.deploy`)] ?? 0);
    d('state', 1, on ? RX.online : RX.off);
    d('out', 0.005, out0);
    d('temp', 1, on ? eq.core : 20);
    d('start', 0.02);
    d('cap', 0.01, 1.1);
    d('kw', 0.1);
    d('cool.temp', 1, on ? eq.cool : 20);
    d('cool.flow', 0.02, 1);
    d('rad.area', 0.5, 8);
    d('rad.kw', 0.5);
  }

  /**
   * Steady temperatures (°C) at output fraction `out` with full coolant flow and the radiators at
   * the given deployment: radiated heat = core heat + pump heat.
   */
  equilibrium(out: number, deployed: (r: PartDef) => number) {
    const k = this.k;
    const q = out * k.heatKw;
    const area = this.radiators.reduce((a, r) => a + r.p.stowed + (r.p.deployed - r.p.stowed) * deployed(r), 0);
    if (area <= 0) return { core: 20, cool: 20 };
    const K = (((q + k.pumpHeat) * 1000) / (0.9 * 5.67e-8 * area) + 230 ** 4) ** 0.25;
    const cool = K - 273.15;
    return { cool, core: cool + q / k.ua };
  }

  /** Variable index by local name ('state', 'temp', 'cool.flow'…). */
  vi(name: string) {
    return this.v[name];
  }

  state(st: Float64Array) {
    return st[this.v.state];
  }

  private H(st: Float64Array, p: PartDef = this.part) {
    return this.sys.health(st, p);
  }

  /** Coolant flow the loop's pumps can give now (0..1). */
  coolFlow(st: Float64Array, sw: Record<string, number>) {
    let flow = 0;
    for (const pump of this.pumps) {
      if (sw[partKey(pump, 'run', pump.id)] !== 1 || this.sys.supply(st, pump.circuit) < 0.5) continue;
      const h = this.H(st, pump);
      if (h > 0) flow += 0.35 + 0.65 * h;
    }
    return Math.min(1, flow);
  }

  /** Why the reactor can't start now (null = it can). */
  startBlock(st: Float64Array, sw: Record<string, number>) {
    if (st[this.v.state] === RX.scram) return 'Reactor en SCRAM · rearme primero';
    if (this.H(st) < 0.1) return 'Reactor destruido';
    if (this.coolFlow(st, sw) < 0.5) return 'Enclavamiento: bomba de refrigerante parada';
    const power = this.sys.power;
    if (power && !power.canStart(st, sw, this.tag)) return 'Sin energía de arranque: conecta la batería o arranca un generador (APU)';
    return null;
  }

  input(t: Tick) {
    const { st, sw } = t;
    const V = this.v;
    if (sw[this.keys.scram] === 1) {
      t.setSw(this.keys.scram, 0);
      if (st[V.state] === RX.online || st[V.state] === RX.starting) {
        st[V.state] = RX.scram;
        t.say(`SCRAM: ${this.part.name} parado de emergencia`);
      }
    }
    if (sw[this.keys.reset] === 1) {
      t.setSw(this.keys.reset, 0);
      if (st[V.state] === RX.scram && st[V.temp] < this.k.resetC) {
        st[V.state] = RX.off;
        t.setSw(this.keys.run, 0);
      }
    }
  }

  loads(t: Tick) {
    const { st } = t;
    const V = this.v;
    const kw = st[V.state] === RX.online ? this.k.kw * st[V.out] : 0;
    st[V.kw] = kw;
    if (kw > 0) {
      const h = this.H(st);
      let q = Math.max(0.25, Math.min(1, (h - 0.2) / 0.6));
      if (h < 0.4) q *= 0.85 + 0.15 * t.ctx.rand(); // damaged core: output sags and flickers
      t.source({ id: this.tag, kw, quality: q, order: 0 });
    }
    if (st[V.state] === RX.starting) t.load(this.part.circuit, this.k.startKw);
  }

  step(t: Tick) {
    const { dt, st, sw } = t;
    const V = this.v;
    const k = this.k;
    const H = this.H(st);
    const rs = st[V.state];
    const flow = this.coolFlow(st, sw);
    st[V['cool.flow']] = flow;
    const cap = H >= 0.6 ? 1.1 : Math.max(0, (H / 0.6) * 1.1);
    st[V.cap] = cap;
    const run = sw[this.keys.run] === 1;
    if (rs === RX.off && run) {
      const why = this.startBlock(st, sw);
      if (why) {
        t.setSw(this.keys.run, 0);
        t.say(`Arranque del reactor abortado: ${why}`);
      } else {
        st[V.state] = RX.starting;
        st[V.start] = 0;
      }
    } else if (rs === RX.starting) {
      const why = !run ? 'palanca en PARADO' : flow < 0.5 ? 'sin refrigeración' : this.sys.supply(st, this.part.circuit) < 0.5 ? 'sin energía de arranque' : null;
      if (why) {
        st[V.state] = RX.off;
        t.setSw(this.keys.run, 0);
        t.say(`Arranque del reactor abortado: ${why}`);
      } else {
        st[V.start] += dt / k.startS;
        if (st[V.start] >= 1) {
          st[V.state] = RX.online;
          st[V.start] = 1;
          st[V.out] = 0;
        }
      }
    } else if (rs === RX.online) {
      const target = run ? Math.min(cap, REACTOR_SET[Math.round(sw[this.keys.set] ?? 2)] ?? 0.5) : 0;
      const o = st[V.out];
      st[V.out] = target > o ? Math.min(target, o + 0.05 * dt) : Math.max(target, o - 0.08 * dt);
      if (H < 0.4) st[V.out] = Math.max(0, st[V.out] * (1 - 0.08 * t.ctx.rand() * dt * 10));
      if (!run && st[V.out] < 0.01) st[V.state] = RX.off;
      if (H < 0.15 && t.ctx.rand() < 0.04 * dt) {
        st[V.state] = RX.scram;
        t.say(`SCRAM automático: inestabilidad del núcleo (${this.part.name} dañado)`);
      }
    }
    if (st[V.state] !== RX.online) st[V.out] = Math.max(0, st[V.out] - 0.5 * dt);
    // heat: core → coolant (pumped) → radiators → space
    const q = st[V.out] * k.heatKw;
    const ua = k.ua * Math.max(0.04, flow);
    const tCore = st[V.temp];
    const tCool = st[V['cool.temp']];
    const toCool = ua * (tCore - tCool);
    let area = 0;
    for (const r of this.radiators) {
      const deployed = this.sys.mover(st, partKey(r, 'deploy', `${r.id}.deploy`));
      area += (r.p.stowed + (r.p.deployed - r.p.stowed) * deployed) * this.H(st, r);
    }
    const K = tCool + 273.15;
    const radKw = (0.9 * 5.67e-8 * area * (K ** 4 - 230 ** 4)) / 1000;
    st[V['rad.area']] = area;
    st[V['rad.kw']] = radKw;
    st[V.temp] = tCore + ((q - toCool) / k.coreCap) * dt;
    st[V['cool.temp']] = Math.max(10, tCool + ((toCool + k.pumpHeat - radKw) / k.coolCap) * dt);
    if (st[V.temp] > k.scramC && (st[V.state] === RX.online || st[V.state] === RX.starting)) {
      st[V.state] = RX.scram;
      t.say(`SCRAM automático: núcleo a ${Math.round(st[V.temp])} °C`);
    }
    if (st[V.temp] > k.damageC) this.sys.damagePart(st, this.part, 2.5 * dt, t.events);
  }

  interlock(c: ControlDef, next: number, st: Float64Array, sw: Record<string, number>) {
    if (c.key === this.keys.run && next === 1 && st[this.v.state] === RX.off) return this.startBlock(st, sw);
    if (c.key === this.keys.reset && next === 1) {
      if (st[this.v.state] !== RX.scram) return 'El reactor no está en SCRAM';
      if (st[this.v.temp] >= this.k.resetC) return `Núcleo demasiado caliente para rearmar (${Math.round(st[this.v.temp])} °C)`;
    }
    return null;
  }

  alerts(): AlertDef[] {
    const V = this.v;
    const T = this.tag;
    const k = this.k;
    const on = (st: Float64Array) => st[V.state] === RX.online || st[V.state] === RX.starting;
    const deploy = this.radiators.some((r) => r.p.deployed > r.p.stowed) ? ' y despliega los radiadores' : '';
    return [
      { id: `${T}.temp`, label: 'REACTOR: SOBRETEMPERATURA', level: 2, lamp: 'REACTOR', help: `El núcleo pasa de ${k.warnC} °C. Baja la salida y comprueba la bomba de refrigerante${deploy}. A ${k.scramC} °C salta el SCRAM solo; por encima de ${k.damageC} °C el núcleo se daña.`, on: (st) => st[V.temp] > this.k.warnC },
      { id: `${T}.scram`, label: 'REACTOR EN SCRAM', level: 2, lamp: 'SCRAM', help: `El reactor se ha parado de emergencia (botón, sobretemperatura o daño). Espera a que el núcleo baje de ${k.resetC} °C, pulsa REARME y vuelve a arrancarlo.`, on: (st) => st[V.state] === RX.scram },
      { id: `${T}.dmg`, label: 'REACTOR DAÑADO', level: 1, lamp: 'REACTOR', help: 'El reactor está por debajo del 50 %: da menos potencia, la red va sucia y puede hacer SCRAM solo. Suéldalo.', on: (st) => this.H(st) < 0.5 },
      { id: `${T}.cool`, label: 'SIN REFRIGERACIÓN DEL REACTOR', level: 2, lamp: 'REFRIG', help: 'El reactor funciona sin caudal de refrigerante: se calentará hasta el SCRAM. Enciende la bomba, rearma el disyuntor REFRIGERACIÓN o baja la salida.', on: (st) => on(st) && st[V['cool.flow']] < 0.5 },
      { id: `${T}.cool.temp`, label: 'REFRIGERANTE CALIENTE', level: 1, lamp: 'REFRIG', help: deploy ? 'Los radiadores no evacuan todo el calor. Despliégalos o baja la salida del reactor.' : 'Los radiadores no evacuan todo el calor (están dañados o la salida es demasiado alta). Baja la salida del reactor y suelda los radiadores.', on: (st) => st[V['cool.temp']] > 450 },
    ];
  }
}

export const reactorSystem: SystemFactory = {
  id: 'reactor',
  parts: ['reactor', 'coolpump', 'radiator'],
  make: (sys) => partsOf(sys, 'reactor').map((p, i) => new Reactor(sys, p, i === 0)),
};
