// Propellant network: tanks and consumers are parts, joined through manifolds by pipes with
// valves. Each step every consumer (engine, APU, RCS) asks for a mass flow; it is served from the
// tanks it can reach through open valves, limited by what each tank can push (boost pump on and
// powered, or only its ullage pressure). Damaged tanks leak, a transfer pump moves propellant
// between tanks, and a refuelling connection on the pad fills them.

import type { FluidNetDef, PartDef } from './def.js';
import type { VarTable } from './state.js';

/** Tank outflow with the boost pump running / pressure-fed only (kg/s). */
const PUMPED = 6;
const UNPUMPED = 1.2;
const TRANSFER = 3;
const REFUEL = 10;

/** Transfer selector positions: [from, to] tank ids (null = off). */
export const TRANSFER_MODES: Array<[string, string] | null> = [null, ['tank.C', 'tank.L'], ['tank.C', 'tank.R'], ['tank.L', 'tank.R'], ['tank.R', 'tank.L']];

export interface FluidInput {
  /** Requested kg/s per consumer part id. */
  demand: Record<string, number>;
  /** Boost pump of a tank running (switch on + circuit powered + part working). */
  pumped: (tank: PartDef) => boolean;
  /** Transfer pump powered. */
  transferPower: boolean;
  /** Refuelling hose connected and flowing. */
  refuel: boolean;
  hp: (part: PartDef) => number;
}

export class PropellantNet {
  readonly tanks: PartDef[];
  private kg = new Map<string, number>();
  private leak = new Map<string, number>();
  private feed = new Map<string, number>();
  private adj = new Map<string, Array<{ to: string; valve?: string }>>();
  readonly iTotal: number;
  readonly iCap: number;
  readonly iXfer: number;

  constructor(
    net: FluidNetDef,
    private parts: PartDef[],
    vars: VarTable,
  ) {
    this.tanks = parts.filter((p) => p.type === 'tank');
    for (const t of this.tanks) {
      this.kg.set(t.id, vars.define(`${t.id}.kg`, 0.5, t.p.fill ?? t.p.cap));
      this.leak.set(t.id, vars.define(`${t.id}.leak`, 0.05));
    }
    for (const p of parts) if (p.feed) this.feed.set(p.id, vars.define(`${p.id}.feed`, 0.02, 0));
    this.iTotal = vars.define('fuel.kg', 0.5, this.tanks.reduce((a, t) => a + (t.p.fill ?? t.p.cap), 0));
    this.iCap = vars.define('fuel.cap', 1, this.tanks.reduce((a, t) => a + t.p.cap, 0));
    this.iXfer = vars.define('fuel.xfer', 0.05);
    for (const pipe of net.pipes) {
      for (const [a, b] of [[pipe.a, pipe.b], [pipe.b, pipe.a]]) {
        if (!this.adj.has(a)) this.adj.set(a, []);
        this.adj.get(a)!.push({ to: b, valve: pipe.valve });
      }
    }
  }

  kgIndex(tank: string) {
    return this.kg.get(tank)!;
  }

  feedIndex(consumer: string) {
    return this.feed.get(consumer)!;
  }

  /** Tanks reachable from a node through open valves. */
  reachable(from: string, sw: Record<string, number>, hp: (p: PartDef) => number): PartDef[] {
    const seen = new Set([from]);
    const queue = [from];
    const out: PartDef[] = [];
    while (queue.length) {
      const n = queue.shift()!;
      const tank = this.tanks.find((t) => t.id === n);
      // a destroyed tank still connects (empty) but gives nothing
      if (tank && hp(tank) > 0) out.push(tank);
      for (const e of this.adj.get(n) ?? []) {
        if (seen.has(e.to) || (e.valve && sw[e.valve] !== 1)) continue;
        seen.add(e.to);
        queue.push(e.to);
      }
    }
    return out;
  }

  step(dt: number, st: Float64Array, sw: Record<string, number>, input: FluidInput): { leaked: Record<string, number> } {
    const room = new Map<string, number>();
    for (const t of this.tanks) room.set(t.id, (input.pumped(t) ? PUMPED : UNPUMPED) * dt);
    // serve consumers, splitting each demand over its reachable tanks by content
    for (const p of this.parts) {
      if (!p.feed) continue;
      const want = (input.demand[p.id] ?? 0) * dt;
      const fi = this.feed.get(p.id)!;
      if (want <= 0) {
        // no draw: report whether it *could* be fed (for start interlocks / displays)
        st[fi] = this.reachable(p.feed, sw, input.hp).some((t) => st[this.kg.get(t.id)!] > 0.5) ? 1 : 0;
        continue;
      }
      let got = 0;
      for (let pass = 0; pass < 2 && got < want - 1e-9; pass++) {
        const src: PartDef[] = this.reachable(p.feed, sw, input.hp).filter((t) => st[this.kg.get(t.id)!] > 1e-6 && room.get(t.id)! > 1e-9);
        const sum = src.reduce((a: number, t: PartDef) => a + st[this.kg.get(t.id)!], 0);
        if (sum <= 0) break;
        const need = want - got;
        for (const t of src) {
          const i = this.kg.get(t.id)!;
          const take = Math.min(need * (st[i] / sum), st[i], room.get(t.id)!);
          st[i] -= take;
          room.set(t.id, room.get(t.id)! - take);
          got += take;
        }
      }
      st[fi] = Math.min(1, got / want);
    }
    // transfer pump
    const mode = TRANSFER_MODES[Math.round(sw['xfer'] ?? 0)] ?? null;
    st[this.iXfer] = 0;
    if (mode && input.transferPower) {
      const [from, to] = mode;
      const a = this.kg.get(from);
      const b = this.kg.get(to);
      const ta = this.tanks.find((t) => t.id === from);
      const tb = this.tanks.find((t) => t.id === to);
      if (a !== undefined && b !== undefined && ta && tb && input.hp(ta) > 0 && input.hp(tb) > 0 && sw[`v.${from}`] === 1 && sw[`v.${to}`] === 1) {
        const move = Math.min(TRANSFER * dt, st[a], tb.p.cap - st[b]);
        if (move > 0) {
          st[a] -= move;
          st[b] += move;
          st[this.iXfer] = move / dt;
        }
      }
    }
    // refuelling fills every tank whose outlet valve is open, evenly
    if (input.refuel) {
      const open = this.tanks.filter((t) => sw[`v.${t.id}`] === 1 && input.hp(t) > 0 && st[this.kg.get(t.id)!] < t.p.cap);
      for (const t of open) {
        const i = this.kg.get(t.id)!;
        st[i] = Math.min(t.p.cap, st[i] + (REFUEL * dt) / open.length);
      }
    }
    // leaks: a holed tank bleeds propellant, a destroyed one dumps it
    const leaked: Record<string, number> = {};
    for (const t of this.tanks) {
      const i = this.kg.get(t.id)!;
      const r = input.hp(t) / t.maxHp;
      const rate = r <= 0 ? 45 : r < 0.7 ? 4 * ((0.7 - r) / 0.7) ** 2 : 0;
      const lost = Math.min(st[i], rate * dt);
      st[i] -= lost;
      st[this.leak.get(t.id)!] = lost / dt;
      if (lost > 0) leaked[t.id] = lost;
    }
    st[this.iTotal] = this.tanks.reduce((a, t) => a + st[this.kg.get(t.id)!], 0);
    return { leaked };
  }
}
