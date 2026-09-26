// Propellant network: tanks and consumers are parts, joined through manifolds by pipes with
// valves. Each step every consumer (engine, APU, RCS…) asks for a mass flow (`t.burn`); it is
// served from the tanks it can reach through open valves, limited by what each tank can push
// (boost pump on and powered, or only its ullage pressure). Damaged tanks leak, a destroyed full
// tank goes up, a transfer pump moves propellant between tanks and a refuelling connection on the
// pad fills them. Everything ship-specific (which valve, which pump, transfer positions) is data.

import { partKey, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import type { VarTable } from '../state.js';
import { partsOf, type AlertDef, type InterlockEnv, type ShipModule, type SysEvent, type SystemFactory, type Tick } from './api.js';

/** Tank outflow with the boost pump running / pressure-fed only (kg/s), pump draw (kW). */
const PUMPED = 6;
const UNPUMPED = 1.2;
const PUMP_KW = 0.8;
const TRANSFER = { kgs: 3, kw: 1 };
const REFUEL = 10;

interface Tank {
  part: PartDef;
  valve: string;
  pump: string;
  kg: number;
  leak: number;
}

export class PropellantNet implements ShipModule {
  readonly id = 'propellant';
  readonly order = 20;
  readonly tanks: PartDef[];
  private tk: Tank[];
  private kg = new Map<string, number>();
  private feed = new Map<string, number>();
  private adj = new Map<string, Array<{ to: string; valve?: string }>>();
  readonly iTotal: number;
  readonly iCap: number;
  readonly iXfer: number;

  constructor(
    private sys: ShipSystems,
    vars: VarTable,
  ) {
    const net = sys.def.fluid;
    this.tanks = partsOf(sys, 'tank');
    this.tk = this.tanks.map((t) => {
      const kg = vars.define(`${t.id}.kg`, 0.5, t.p.fill ?? t.p.cap);
      this.kg.set(t.id, kg);
      return { part: t, valve: partKey(t, 'valve', `v.${t.id}`), pump: partKey(t, 'pump', `pump.${t.id}`), kg, leak: vars.define(`${t.id}.leak`, 0.05) };
    });
    for (const p of sys.def.parts) if (p.feed) this.feed.set(p.id, vars.define(`${p.id}.feed`, 0.02, 0));
    this.iTotal = vars.define('fuel.kg', 0.5, this.tanks.reduce((a, t) => a + (t.p.fill ?? t.p.cap), 0));
    this.iCap = vars.define('fuel.cap', 1, this.tanks.reduce((a, t) => a + t.p.cap, 0));
    this.iXfer = vars.define('fuel.xfer', 0.05);
    for (const pipe of net.pipes) {
      for (const [a, b] of [
        [pipe.a, pipe.b],
        [pipe.b, pipe.a],
      ]) {
        if (!this.adj.has(a)) this.adj.set(a, []);
        this.adj.get(a)!.push({ to: b, valve: pipe.valve });
      }
    }
    sys.provide('propellant', this);
  }

  kgIndex(tank: string) {
    return this.kg.get(tank)!;
  }

  feedIndex(consumer: string) {
    return this.feed.get(consumer)!;
  }

  /** Tank ids' outlet valve / boost pump switch keys. */
  valveOf(tank: string) {
    return this.tk.find((t) => t.part.id === tank)?.valve;
  }

  pumpOf(tank: string) {
    return this.tk.find((t) => t.part.id === tank)?.pump;
  }

  /** Tanks reachable from a node through open valves (a destroyed tank still connects, empty). */
  reachable(from: string, sw: Record<string, number>, st: Float64Array): PartDef[] {
    const seen = new Set([from]);
    const queue = [from];
    const out: PartDef[] = [];
    while (queue.length) {
      const n = queue.shift()!;
      const tank = this.tanks.find((t) => t.id === n);
      if (tank && this.sys.hp(st, tank.id) > 0) out.push(tank);
      for (const e of this.adj.get(n) ?? []) {
        if (seen.has(e.to) || (e.valve && sw[e.valve] !== 1)) continue;
        seen.add(e.to);
        queue.push(e.to);
      }
    }
    return out;
  }

  /** Some reachable tank holds more than `kg`. */
  canFeed(node: string, sw: Record<string, number>, st: Float64Array, kg = 1) {
    return this.reachable(node, sw, st).some((t) => st[this.kg.get(t.id)!] > kg);
  }

  /** Every pipe into this consumer has a valve and they are all shut (the crew isolated it). */
  isolated(consumer: string, sw: Record<string, number>) {
    const pipes = this.adj.get(consumer) ?? [];
    return pipes.length > 0 && pipes.every((e) => e.valve !== undefined && sw[e.valve] !== 1);
  }

  private powered(st: Float64Array) {
    return this.sys.supply(st, this.sys.def.fluid.circuit) >= 0.5;
  }

  loads(t: Tick) {
    const c = this.sys.def.fluid.circuit;
    for (const tk of this.tk) if (t.sw[tk.pump] === 1) t.load(c, tk.part.p.pumpKw ?? PUMP_KW);
    const x = this.sys.def.fluid.transfer;
    if (x && (t.sw[x.key] ?? 0) > 0) t.load(c, TRANSFER.kw);
  }

  solve(t: Tick) {
    const { dt, st, sw } = t;
    const sys = this.sys;
    const powered = this.powered(st);
    const room = new Map<string, number>();
    for (const tk of this.tk) room.set(tk.part.id, (powered && sw[tk.pump] === 1 && sys.health(st, tk.part) > 0.2 ? PUMPED : UNPUMPED) * dt);
    // serve consumers, splitting each demand over its reachable tanks by content
    for (const p of sys.def.parts) {
      if (!p.feed) continue;
      const want = (t.fuel[p.id] ?? 0) * dt;
      const fi = this.feed.get(p.id)!;
      if (want <= 0) {
        // no draw: report whether it *could* be fed (for start interlocks / displays)
        st[fi] = this.canFeed(p.feed, sw, st, 0.5) ? 1 : 0;
        continue;
      }
      let got = 0;
      for (let pass = 0; pass < 2 && got < want - 1e-9; pass++) {
        const src: PartDef[] = this.reachable(p.feed, sw, st).filter((tank) => st[this.kg.get(tank.id)!] > 1e-6 && room.get(tank.id)! > 1e-9);
        const sum = src.reduce((a: number, tank: PartDef) => a + st[this.kg.get(tank.id)!], 0);
        if (sum <= 0) break;
        const need = want - got;
        for (const tank of src) {
          const i = this.kg.get(tank.id)!;
          const take = Math.min(need * (st[i] / sum), st[i], room.get(tank.id)!);
          st[i] -= take;
          room.set(tank.id, room.get(tank.id)! - take);
          got += take;
        }
      }
      st[fi] = Math.min(1, got / want);
    }
    // transfer pump: needs both outlet valves open and both tanks whole
    const x = sys.def.fluid.transfer;
    const mode = x ? x.modes[Math.round(sw[x.key] ?? 0)] ?? null : null;
    st[this.iXfer] = 0;
    if (mode && powered) {
      const [from, to] = mode;
      const a = this.tk.find((k) => k.part.id === from);
      const b = this.tk.find((k) => k.part.id === to);
      if (a && b && sys.hp(st, from) > 0 && sys.hp(st, to) > 0 && sw[a.valve] === 1 && sw[b.valve] === 1) {
        const move = Math.min((x!.kgs ?? TRANSFER.kgs) * dt, st[a.kg], b.part.p.cap - st[b.kg]);
        if (move > 0) {
          st[a.kg] -= move;
          st[b.kg] += move;
          st[this.iXfer] = move / dt;
        }
      }
    }
    // refuelling fills every tank whose outlet valve is open, evenly
    const rf = sys.def.fluid.refuel;
    if (rf && t.ctx.onPad && t.ctx.landed && sw[rf.key] === 1) {
      const open = this.tk.filter((k) => sw[k.valve] === 1 && sys.hp(st, k.part.id) > 0 && st[k.kg] < k.part.p.cap);
      for (const k of open) st[k.kg] = Math.min(k.part.p.cap, st[k.kg] + ((rf.kgs ?? REFUEL) * dt) / open.length);
    }
    // leaks: a holed tank bleeds propellant, a destroyed one dumps it
    for (const k of this.tk) {
      const r = sys.hp(st, k.part.id) / k.part.maxHp;
      const rate = r <= 0 ? 45 : r < 0.7 ? 4 * ((0.7 - r) / 0.7) ** 2 : 0;
      const lost = Math.min(st[k.kg], rate * dt);
      st[k.kg] -= lost;
      st[k.leak] = lost / dt;
    }
    st[this.iTotal] = this.tk.reduce((a, k) => a + st[k.kg], 0);
  }

  /** A tank ruptured with propellant in it goes up in a secondary explosion. */
  destroyed(st: Float64Array, p: PartDef, events: SysEvent[]) {
    if (p.type !== 'tank') return;
    const kg = st[this.kg.get(p.id)!];
    if (kg > 60) events.push({ type: 'explode', at: p.c, radius: Math.min(9, 3 + kg / 250), damage: Math.min(160, 50 + kg / 12), cause: `${p.name}: rotura con propelente` });
  }

  interlock(c: { key: string }, next: number, _st: Float64Array, _sw: Record<string, number>, env: InterlockEnv) {
    const rf = this.sys.def.fluid.refuel;
    if (rf && c.key === rf.key && next === 1 && !(env.landed && env.onPad)) return 'No hay toma de repostaje aquí (plataforma de la base)';
    return null;
  }

  alerts(): AlertDef[] {
    if (!this.tk.length) return [];
    const out: AlertDef[] = [{ id: 'fuel', label: 'PROPELENTE BAJO', level: 1, lamp: 'COMBUST', help: 'Queda menos del 15 % del propelente total. Reposta en la plataforma de la base (toma exterior, válvulas de los depósitos abiertas).', on: (st) => st[this.iTotal] < st[this.iCap] * 0.15 }];
    for (const b of this.sys.def.fluid.balance ?? []) {
      const ia = this.kg.get(b.a)!;
      const ib = this.kg.get(b.b)!;
      out.push({ id: `imb.${b.a}.${b.b}`, label: 'DESEQUILIBRIO DE PROPELENTE', level: 1, lamp: 'DESEQUIL', help: 'Un depósito lateral tiene mucho más que el otro: la nave vuela cargada hacia un lado. Iguálalos con la transferencia o abriendo la alimentación cruzada.', on: (st) => Math.abs(st[ia] - st[ib]) > b.kg });
    }
    for (const k of this.tk) out.push({ id: `leak.${k.part.id}`, label: `FUGA DE PROPELENTE · ${k.part.name}`, level: 2, lamp: 'FUGA COMB', help: 'El depósito está dañado y pierde propelente. Pásalo a otro depósito con la transferencia antes de que se vacíe y suéldalo. Si revienta lleno, explota.', on: (st) => st[k.leak] > 0.05 });
    return out;
  }
}

export const propellantSystem: SystemFactory = {
  id: 'propellant',
  parts: ['tank'],
  make: (sys) => (partsOf(sys, 'tank').length ? [new PropellantNet(sys, sys.vars)] : []),
};
