// Propellant network: tanks and consumers are parts, joined through manifolds by pipes with
// valves. Each step every consumer (engine, APU, RCS…) asks for a mass flow (`t.burn`); it is
// served from the tanks it can reach through open valves, limited by what each tank can push
// (boost pump on and powered, or only its ullage pressure). Damaged tanks leak, a destroyed full
// tank goes up, a transfer pump moves propellant between tanks and a refuelling connection on the
// pad fills them. Everything ship-specific (which valve, which pump, transfer positions) is data.

import { partKey, type PartDef } from '../def.js';
import type { V3 } from '../geom.js';
import type { ShipSystems } from '../systems.js';
import type { VarTable } from '../state.js';
import { partsOf, type AlertDef, type InterlockEnv, type ShipModule, type SoundCue, type SysEvent, type SystemFactory, type Tick } from './api.js';

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
  /** Integrity variable of the tank. */
  hp: number;
}

/** A consumer (engine, APU, RCS…): its part index, the node it draws from, its feed-fraction variable. */
interface Consumer {
  k: number;
  node: number;
  fi: number;
}

interface Link {
  to: number;
  valve: string | undefined;
}

export class PropellantNet implements ShipModule {
  readonly id = 'propellant';
  readonly order = 20;
  readonly tanks: PartDef[];
  private tk: Tank[];
  private kg = new Map<string, number>();
  private feed = new Map<string, number>();
  private tankIdx = new Map<string, number>();
  /** Pipe graph over integer nodes (tanks, manifolds, consumers). */
  private node = new Map<string, number>();
  private links: Link[][] = [];
  /** Per node: the tank there (index into `tk`) or -1. */
  private tankAt: number[] = [];
  /** Distinct valve keys on the pipes and the positions they had when `reach` was built. */
  private valves: string[] = [];
  private valveOpen: Int8Array;
  /** Per node: tanks reachable through open valves (index into `tk`, breadth-first), built on demand. */
  private reach: Array<Int32Array | null> = [];
  private mark: Int32Array;
  private queue: Int32Array;
  private stamp = 0;
  private consumers: Consumer[] = [];
  /** Scratch of one solve: outflow room per tank this tick, the tanks serving a consumer. */
  private room: Float64Array;
  private src: Int32Array;
  readonly iTotal: number;
  readonly iCap: number;
  readonly iXfer: number;

  constructor(
    private sys: ShipSystems,
    vars: VarTable,
  ) {
    const net = sys.def.fluid;
    this.tanks = partsOf(sys, 'tank');
    this.tk = this.tanks.map((t, i) => {
      const kg = vars.define(`${t.id}.kg`, 0.5, t.p.fill ?? t.p.cap);
      this.kg.set(t.id, kg);
      if (!this.tankIdx.has(t.id)) this.tankIdx.set(t.id, i);
      return { part: t, valve: partKey(t, 'valve', `v.${t.id}`), pump: partKey(t, 'pump', `pump.${t.id}`), kg, leak: vars.define(`${t.id}.leak`, 0.05), hp: sys.hpOf(t) };
    });
    for (const p of sys.def.parts) if (p.feed) this.feed.set(p.id, vars.define(`${p.id}.feed`, 0.02, 0));
    this.iTotal = vars.define('fuel.kg', 0.5, this.tanks.reduce((a, t) => a + (t.p.fill ?? t.p.cap), 0));
    this.iCap = vars.define('fuel.cap', 1, this.tanks.reduce((a, t) => a + t.p.cap, 0));
    this.iXfer = vars.define('fuel.xfer', 0.05);
    // the graph: every tank, pipe end and feed point is a node
    for (const t of this.tanks) this.nodeOf(t.id);
    for (const pipe of net.pipes) {
      const a = this.nodeOf(pipe.a);
      const b = this.nodeOf(pipe.b);
      this.links[a].push({ to: b, valve: pipe.valve });
      this.links[b].push({ to: a, valve: pipe.valve });
      if (pipe.valve && !this.valves.includes(pipe.valve)) this.valves.push(pipe.valve);
    }
    sys.def.parts.forEach((p, k) => {
      if (p.feed) this.consumers.push({ k, node: this.nodeOf(p.feed), fi: this.feed.get(p.id)! });
      if (p.feed || this.node.has(p.id)) this.nodeOf(p.id);
    });
    this.tankAt = Array.from({ length: this.links.length }, () => -1);
    this.tanks.forEach((t, i) => {
      const n = this.node.get(t.id)!;
      if (this.tankAt[n] < 0) this.tankAt[n] = i;
    });
    this.reach = this.links.map(() => null);
    this.valveOpen = new Int8Array(this.valves.length).fill(-1);
    this.mark = new Int32Array(this.links.length);
    this.queue = new Int32Array(this.links.length);
    this.room = new Float64Array(this.tk.length);
    this.src = new Int32Array(this.tk.length);
    sys.provide('propellant', this);
  }

  private nodeOf(id: string) {
    let n = this.node.get(id);
    if (n === undefined) {
      n = this.links.length;
      this.node.set(id, n);
      this.links.push([]);
    }
    return n;
  }

  /** Forget the reach lists when a valve moved since they were built. */
  private check(sw: Record<string, number>) {
    let moved = false;
    for (let k = 0; k < this.valves.length; k++) {
      const open = sw[this.valves[k]] === 1 ? 1 : 0;
      if (this.valveOpen[k] !== open) {
        this.valveOpen[k] = open;
        moved = true;
      }
    }
    if (moved) this.reach.fill(null);
  }

  /** Tanks reachable from node `from` through open valves, whole or not (cached until a valve moves). */
  private reachOf(from: number, sw: Record<string, number>): Int32Array {
    this.check(sw);
    const hit = this.reach[from];
    if (hit) return hit;
    const stamp = ++this.stamp;
    const mark = this.mark;
    const queue = this.queue;
    let head = 0;
    let tail = 0;
    mark[from] = stamp;
    queue[tail++] = from;
    const out: number[] = [];
    while (head < tail) {
      const n = queue[head++];
      if (this.tankAt[n] >= 0) out.push(this.tankAt[n]);
      for (const e of this.links[n]) {
        if (mark[e.to] === stamp || (e.valve && sw[e.valve] !== 1)) continue;
        mark[e.to] = stamp;
        queue[tail++] = e.to;
      }
    }
    const list = Int32Array.from(out);
    this.reach[from] = list;
    return list;
  }

  kgIndex(tank: string) {
    return this.kg.get(tank)!;
  }

  feedIndex(consumer: string) {
    return this.feed.get(consumer)!;
  }

  /** Tank ids' outlet valve / boost pump switch keys. */
  valveOf(tank: string) {
    const i = this.tankIdx.get(tank);
    return i === undefined ? undefined : this.tk[i].valve;
  }

  pumpOf(tank: string) {
    const i = this.tankIdx.get(tank);
    return i === undefined ? undefined : this.tk[i].pump;
  }

  /** Tanks reachable from a node through open valves (a destroyed tank still connects, empty). */
  reachable(from: string, sw: Record<string, number>, st: Float64Array): PartDef[] {
    const out: PartDef[] = [];
    const n = this.node.get(from);
    if (n === undefined) return out;
    const r = this.reachOf(n, sw);
    for (let j = 0; j < r.length; j++) if (st[this.tk[r[j]].hp] > 0) out.push(this.tk[r[j]].part);
    return out;
  }

  /** Some reachable tank holds more than `kg`. */
  canFeed(node: string, sw: Record<string, number>, st: Float64Array, kg = 1) {
    const n = this.node.get(node);
    return n !== undefined && this.feeds(n, sw, st, kg);
  }

  private feeds(n: number, sw: Record<string, number>, st: Float64Array, kg: number) {
    const r = this.reachOf(n, sw);
    for (let j = 0; j < r.length; j++) {
      const t = this.tk[r[j]];
      if (st[t.hp] > 0 && st[t.kg] > kg) return true;
    }
    return false;
  }

  /** Every pipe into this consumer has a valve and they are all shut (the crew isolated it). */
  isolated(consumer: string, sw: Record<string, number>) {
    const n = this.node.get(consumer);
    if (n === undefined) return false;
    const pipes = this.links[n];
    if (!pipes.length) return false;
    for (const e of pipes) if (e.valve === undefined || sw[e.valve] === 1) return false;
    return true;
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
    const tk = this.tk;
    const powered = this.powered(st);
    const room = this.room;
    for (let k = 0; k < tk.length; k++) {
      const x = tk[k];
      room[k] = (powered && sw[x.pump] === 1 && sys.health(st, x.part) > 0.2 ? PUMPED : UNPUMPED) * dt;
    }
    // serve consumers, splitting each demand over its reachable tanks by content
    const src = this.src;
    for (const c of this.consumers) {
      const want = t.fuel[c.k] * dt;
      if (want <= 0) {
        // no draw: report whether it *could* be fed (for start interlocks / displays)
        st[c.fi] = this.feeds(c.node, sw, st, 0.5) ? 1 : 0;
        continue;
      }
      const reach = this.reachOf(c.node, sw);
      let got = 0;
      for (let pass = 0; pass < 2 && got < want - 1e-9; pass++) {
        let n = 0;
        let sum = 0;
        for (let j = 0; j < reach.length; j++) {
          const k = reach[j];
          const x = tk[k];
          if (st[x.hp] > 0 && st[x.kg] > 1e-6 && room[k] > 1e-9) {
            src[n++] = k;
            sum += st[x.kg];
          }
        }
        if (sum <= 0) break;
        const need = want - got;
        for (let j = 0; j < n; j++) {
          const k = src[j];
          const i = tk[k].kg;
          const take = Math.min(need * (st[i] / sum), st[i], room[k]);
          st[i] -= take;
          room[k] -= take;
          got += take;
        }
      }
      st[c.fi] = Math.min(1, got / want);
    }
    // transfer pump: needs both outlet valves open and both tanks whole
    const x = sys.def.fluid.transfer;
    const mode = x ? x.modes[Math.round(sw[x.key] ?? 0)] ?? null : null;
    st[this.iXfer] = 0;
    if (mode && powered) {
      const ai = this.tankIdx.get(mode[0]);
      const bi = this.tankIdx.get(mode[1]);
      const a = ai === undefined ? undefined : tk[ai];
      const b = bi === undefined ? undefined : tk[bi];
      if (a && b && st[a.hp] > 0 && st[b.hp] > 0 && sw[a.valve] === 1 && sw[b.valve] === 1) {
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
      let open = 0;
      for (const k of tk) if (sw[k.valve] === 1 && st[k.hp] > 0 && st[k.kg] < k.part.p.cap) open++;
      if (open) {
        for (const k of tk) {
          if (sw[k.valve] === 1 && st[k.hp] > 0 && st[k.kg] < k.part.p.cap) st[k.kg] = Math.min(k.part.p.cap, st[k.kg] + ((rf.kgs ?? REFUEL) * dt) / open);
        }
      }
    }
    // leaks: a holed tank bleeds propellant, a destroyed one dumps it
    let total = 0;
    for (const k of tk) {
      const r = st[k.hp] / k.part.maxHp;
      const rate = r <= 0 ? 45 : r < 0.7 ? 4 * ((0.7 - r) / 0.7) ** 2 : 0;
      const lost = Math.min(st[k.kg], rate * dt);
      st[k.kg] -= lost;
      st[k.leak] = lost / dt;
    }
    for (const k of tk) total += st[k.kg];
    st[this.iTotal] = total;
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

  /** Boost pumps at their tanks, the transfer pump, propellant spraying out of a damaged tank. */
  sounds(): SoundCue[] {
    const out: SoundCue[] = [];
    const powered = (st: Float64Array) => this.powered(st);
    for (const tk of this.tk) {
      const part = tk.part;
      out.push({ sound: 'mach.pump', role: 'pump', part, gain: 0.7, level: (st, sw) => (sw[tk.pump] === 1 && powered(st) ? 1 : 0) });
      out.push({ sound: 'air.hiss', role: 'leak', part, level: (st) => (st[tk.leak] > 0.01 ? Math.min(1, 0.4 + st[tk.leak] * 2) : 0) });
    }
    const x = this.sys.def.fluid.transfer;
    if (x && this.tk.length) {
      const at: V3 = [0, 0, 0];
      for (const tk of this.tk) for (let k = 0; k < 3; k++) at[k] += tk.part.c[k] / this.tk.length;
      out.push({ sound: 'mach.pump', role: 'transfer', at, zone: null, level: (st, sw) => ((sw[x.key] ?? 0) > 0 && powered(st) ? 1 : 0), pitch: () => 1.2 });
    }
    return out;
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
