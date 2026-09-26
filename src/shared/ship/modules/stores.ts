// Stores for long trips: potable water tanks, the water recycler and the food lockers. Everyone
// aboard breathing cabin air drinks and eats; what they drink becomes grey water, which the
// recycler (switched on, powered, whole) turns back into potable water at its efficiency. A holed
// water tank leaks. Numbers are game scale (hours, not days).

import { partKey, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import { partsOf, type AlertDef, type ShipModule, type SystemFactory, type Tick } from './api.js';

/** Per person aboard (kg/s): water drunk, food eaten. Recycler throughput (kg/s) and efficiency. */
export const STORES = { water: 0.0004, food: 0.00012, recycleKgs: 0.004, eff: 0.9 };

interface Store {
  part: PartDef;
  kg: number;
  cap: number;
}

export class Stores implements ShipModule {
  readonly id = 'stores';
  readonly water: Store[];
  readonly food: Store[];
  readonly recyclers: PartDef[];
  readonly iWater: number;
  readonly iWaterCap: number;
  readonly iFood: number;
  readonly iFoodCap: number;
  readonly iGrey: number;
  readonly iRecycle: number;
  readonly iAboard: number;

  constructor(private sys: ShipSystems) {
    const v = sys.vars;
    const store = (p: PartDef): Store => ({ part: p, cap: p.p.cap, kg: v.define(`${p.id}.kg`, 0.1, p.p.fill ?? p.p.cap) });
    this.water = partsOf(sys, 'water').map(store);
    this.food = partsOf(sys, 'pantry').map(store);
    this.recyclers = partsOf(sys, 'recycler');
    this.iWater = v.define('stores.water', 0.1, this.water.reduce((a, s) => a + (s.part.p.fill ?? s.cap), 0));
    this.iWaterCap = v.define('stores.water.cap', 1, this.water.reduce((a, s) => a + s.cap, 0));
    this.iFood = v.define('stores.food', 0.05, this.food.reduce((a, s) => a + (s.part.p.fill ?? s.cap), 0));
    this.iFoodCap = v.define('stores.food.cap', 1, this.food.reduce((a, s) => a + s.cap, 0));
    this.iGrey = v.define('stores.grey', 0.05);
    this.iRecycle = v.define('stores.recycle', 0.0005);
    this.iAboard = v.define('stores.aboard', 1);
  }

  private take(st: Float64Array, list: Store[], kg: number) {
    const have = list.filter((s) => this.sys.health(st, s.part) > 0 && st[s.kg] > 0);
    const sum = have.reduce((a, s) => a + st[s.kg], 0);
    if (sum <= 0) return 0;
    const out = Math.min(kg, sum);
    for (const s of have) st[s.kg] -= out * (st[s.kg] / sum);
    return out;
  }

  private put(st: Float64Array, list: Store[], kg: number) {
    const into = list.filter((s) => this.sys.health(st, s.part) > 0);
    const room = into.reduce((a, s) => a + Math.max(0, s.cap - st[s.kg]), 0);
    if (room <= 0) return 0;
    const put = Math.min(kg, room);
    for (const s of into) st[s.kg] += put * (Math.max(0, s.cap - st[s.kg]) / room);
    return put;
  }

  /** Recycler running now: switched on, powered, not wrecked. */
  running(st: Float64Array, sw: Record<string, number>, r: PartDef) {
    return sw[partKey(r, 'run', r.id)] === 1 && this.sys.supply(st, r.circuit) >= 0.5 && this.sys.health(st, r) > 0.1;
  }

  step(t: Tick) {
    const { dt, st, sw, ctx } = t;
    const aboard = ctx.crew.reduce((a, n) => a + n, 0);
    st[this.iAboard] = aboard;
    const drunk = this.take(st, this.water, STORES.water * aboard * dt);
    st[this.iGrey] += drunk;
    this.take(st, this.food, STORES.food * aboard * dt);
    // recycling: grey water back into the tanks, the rest is brine
    let back = 0;
    for (const r of this.recyclers) {
      if (!this.running(st, sw, r)) continue;
      const kgs = (r.p.rate ?? STORES.recycleKgs) * this.sys.health(st, r);
      const done = Math.min(st[this.iGrey], kgs * dt);
      st[this.iGrey] -= done;
      back += this.put(st, this.water, done * (r.p.eff ?? STORES.eff));
    }
    st[this.iRecycle] = back / dt;
    // a holed tank leaks
    for (const s of this.water) {
      const h = this.sys.health(st, s.part);
      if (h < 0.5 && st[s.kg] > 0) st[s.kg] = Math.max(0, st[s.kg] - (h <= 0 ? 3 : 0.2 * (1 - h / 0.5)) * dt);
    }
    st[this.iWater] = this.water.reduce((a, s) => a + st[s.kg], 0);
    st[this.iFood] = this.food.reduce((a, s) => a + st[s.kg], 0);
  }

  alerts(): AlertDef[] {
    const out: AlertDef[] = [];
    if (this.water.length)
      out.push({ id: 'water', label: 'AGUA POTABLE BAJA', level: 1, lamp: 'AGUA', help: 'Queda menos del 15 % del agua. Enciende el reciclador (recupera casi toda el agua usada) y reposta agua en la base.', on: (st) => st[this.iWater] < st[this.iWaterCap] * 0.15 });
    if (this.recyclers.length)
      out.push({ id: 'recycler', label: 'RECICLADOR DE AGUA PARADO', level: 1, lamp: 'AGUA', help: 'Se acumula agua gris y el reciclador no la procesa (apagado, sin energía o dañado). Sin él el agua potable dura diez veces menos.', on: (st, sw) => st[this.iGrey] > 0.5 && !this.recyclers.some((r) => this.running(st, sw, r)) });
    if (this.food.length)
      out.push({ id: 'food', label: 'VÍVERES BAJOS', level: 1, lamp: 'VÍVERES', help: 'Queda menos del 15 % de las raciones de la despensa. Planifica la vuelta o reabastece en la base.', on: (st) => st[this.iFood] < st[this.iFoodCap] * 0.15 });
    return out;
  }
}

export const storesSystem: SystemFactory = {
  id: 'stores',
  parts: ['water', 'recycler', 'pantry'],
  make: (sys) => (partsOf(sys, 'water').length || partsOf(sys, 'pantry').length ? [new Stores(sys)] : []),
};
