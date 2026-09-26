// Cabin atmosphere, compartment by compartment: moles of O2, N2 and CO2 and a gas temperature per
// compartment; pressure from the ideal gas law. Gas moves through openings (doors, the ramp,
// blown-out panels, cracks in damaged ones, vent valves) with the compressible orifice equation —
// choked at large pressure ratios, so a 1 m² breach empties a cabin in about a second while a
// cracked plate hisses for minutes — and the expanding gas that stays behind cools. Fans mix the
// air between compartments through ducts with dampers. Life support adds and removes gas: O2
// generator, CO2 scrubber, make-up from the O2/N2 bottles (automatic pressure control or manual
// valves) and a compressor that recovers the air of a compartment into the bottles.

import type { CompartmentDef } from './def.js';
import type { VarTable } from './state.js';

const R = 8.314;
const M = { o2: 0.032, n2: 0.028, co2: 0.044 };
const CD = 0.65;
/** Cabin set point and O2 partial pressure (kPa) — a 10 psi atmosphere like a real spacecraft. */
export const CABIN = { p: 70, po2: 21 };
/** Breathable: pressure, O2 and CO2 limits for helmet-off / suit refill (kPa). */
export const BREATHABLE = { p: 50, po2: 16, pco2: 2 };

export interface GasPath {
  /** Compartment indices; b = -1 → vacuum. */
  a: number;
  b: number;
  area: number;
  /** Duct: fans force a mixing exchange (m³/s) on top of the pressure-driven flow. */
  mix?: number;
}

export interface LifeInput {
  /** Oxygen generator output into a compartment (mol/s). */
  o2gen: { comp: number; rate: number } | null;
  /** CO2 scrubber efficiency 0..1 in a compartment (air reaches it through the ducts). */
  scrub: { comp: number; eff: number } | null;
  /** 0 AUTO (pressure control), 1 MANUAL (per-compartment valves), 2 OFF. */
  mode: number;
  /** Manual repressurisation valve open, per compartment. */
  manual: boolean[];
  /** Bottles can deliver (valves open, solenoids powered). */
  supplyO2: boolean;
  supplyN2: boolean;
  /** Recovery compressor running on this compartment. */
  recover: number | -1;
  /** Crew members breathing cabin air, per compartment. */
  crew: number[];
  /** Cabin heaters powered. */
  heat: boolean;
}

interface CompVars {
  o2: number;
  n2: number;
  co2: number;
  tk: number;
  p: number;
  po2: number;
  pco2: number;
  t: number;
  dpdt: number;
  out: number;
  sealed: number;
  feed: number;
}

export class Atmosphere {
  readonly v: CompVars[];
  readonly iO2: number;
  readonly iN2: number;
  readonly o2Cap: number;
  readonly n2Cap: number;
  private prevP: number[];

  constructor(
    readonly comps: CompartmentDef[],
    vars: VarTable,
    bottles: { o2: { id: string; cap: number }; n2: { id: string; cap: number } },
  ) {
    this.v = comps.map((c) => ({
      // internal integrator state (server only)
      o2: vars.define(`${c.id}.nO2`, -1),
      n2: vars.define(`${c.id}.nN2`, -1),
      co2: vars.define(`${c.id}.nCO2`, -1),
      tk: vars.define(`${c.id}.tk`, -1, 290),
      // published
      p: vars.define(`${c.id}.p`, 0.1),
      po2: vars.define(`${c.id}.po2`, 0.1),
      pco2: vars.define(`${c.id}.pco2`, 0.02),
      t: vars.define(`${c.id}.t`, 0.2, 17),
      dpdt: vars.define(`${c.id}.dpdt`, 0.1),
      out: vars.define(`${c.id}.out`, 0.05),
      sealed: vars.define(`${c.id}.sealed`, 1, 1),
      feed: vars.define(`${c.id}.feed`, 0.02),
    }));
    this.iO2 = vars.define(`${bottles.o2.id}.kg`, 0.1, bottles.o2.cap);
    this.iN2 = vars.define(`${bottles.n2.id}.kg`, 0.1, bottles.n2.cap);
    this.o2Cap = bottles.o2.cap;
    this.n2Cap = bottles.n2.cap;
    this.prevP = comps.map(() => 0);
  }

  /** Fill a compartment to cabin conditions (used for the initial state of sealed ships). */
  fill(st: Float64Array, i: number, kPa = CABIN.p) {
    const c = this.comps[i];
    const v = this.v[i];
    const n = (kPa * 1000 * c.volume) / (R * 294);
    const xo2 = CABIN.po2 / CABIN.p;
    st[v.o2] = n * xo2;
    st[v.n2] = n * (1 - xo2);
    st[v.co2] = 0;
    st[v.tk] = 294;
  }

  pressure(st: Float64Array, i: number) {
    const v = this.v[i];
    return ((st[v.o2] + st[v.n2] + st[v.co2]) * R * st[v.tk]) / this.comps[i].volume;
  }

  step(dt: number, st: Float64Array, paths: GasPath[], life: LifeInput) {
    const C = this.comps;
    const V = this.v;
    const n = C.length;
    // --- groups of compartments joined by big openings, and their leaks to vacuum ------------------
    const root = C.map((_, i) => i);
    const find = (i: number): number => (root[i] === i ? i : (root[i] = find(root[i])));
    for (const g of paths) if (g.b >= 0 && g.area > 0.05) root[find(g.a)] = find(g.b);
    const vacuumArea = new Map<number, number>();
    for (const g of paths) if (g.b < 0) vacuumArea.set(find(g.a), (vacuumArea.get(find(g.a)) ?? 0) + g.area);
    const sealed = C.map((_, i) => (vacuumArea.get(find(i)) ?? 0) < 0.01);

    // --- life support sources and sinks --------------------------------------------------------------
    const add = (i: number, o2: number, n2: number, co2: number, tIn = 294) => {
      const v = V[i];
      const before = st[v.o2] + st[v.n2] + st[v.co2];
      st[v.o2] = Math.max(0, st[v.o2] + o2);
      st[v.n2] = Math.max(0, st[v.n2] + n2);
      st[v.co2] = Math.max(0, st[v.co2] + co2);
      const added = Math.max(0, o2) + Math.max(0, n2) + Math.max(0, co2);
      if (added > 0) st[v.tk] = (st[v.tk] * before + tIn * added) / Math.max(1e-9, before + added);
    };
    if (life.o2gen && life.o2gen.rate > 0 && this.pressure(st, life.o2gen.comp) < 101000) add(life.o2gen.comp, life.o2gen.rate * dt, 0, 0);
    if (life.scrub && life.scrub.eff > 0) {
      const v = V[life.scrub.comp];
      st[v.co2] -= Math.min(st[v.co2], Math.min(0.02 * st[v.co2] + 0.002, 0.08) * life.scrub.eff * dt);
    }
    for (let i = 0; i < n; i++) {
      const crew = life.crew[i] ?? 0;
      if (crew > 0) {
        const o2 = Math.min(st[V[i].o2], 0.012 * crew * dt);
        add(i, -o2, 0, o2 * 0.85);
      }
    }
    // make-up gas from the bottles: AUTO keeps sealed groups at the set point, MANUAL feeds where
    // the operator opened a valve (even into a leak — that's the operator's call)
    for (let i = 0; i < n; i++) st[V[i].feed] = 0;
    for (let i = 0; i < n; i++) {
      const pkPa = this.pressure(st, i) / 1000;
      const wantAuto = life.mode === 0 && sealed[i] && pkPa < CABIN.p - 2;
      const wantManual = life.mode === 1 && life.manual[i] && pkPa < 101;
      if (!wantAuto && !wantManual) continue;
      const v = V[i];
      const po2 = (st[v.o2] * R * st[v.tk]) / C[i].volume / 1000;
      const pn2 = (st[v.n2] * R * st[v.tk]) / C[i].volume / 1000;
      const needO2 = Math.max(0, CABIN.po2 - po2);
      const needN2 = Math.max(0, CABIN.p - CABIN.po2 - pn2);
      const sum = needO2 + needN2 || 1;
      const kgs = wantManual ? 0.6 : 1.0;
      const kO2 = life.supplyO2 ? Math.min(st[this.iO2], ((kgs * needO2) / sum) * dt) : 0;
      const kN2 = life.supplyN2 ? Math.min(st[this.iN2], ((kgs * needN2) / sum) * dt) : 0;
      st[this.iO2] -= kO2;
      st[this.iN2] -= kN2;
      add(i, kO2 / M.o2, kN2 / M.n2, 0, 282);
      st[v.feed] = (kO2 + kN2) / dt;
    }
    // recovery compressor: pumps a compartment's air back into the bottles
    if (life.recover >= 0) {
      const v = V[life.recover];
      const tot = st[v.o2] + st[v.n2] + st[v.co2];
      if (tot > 1e-6 && this.pressure(st, life.recover) > 800) {
        const molar = (st[v.o2] * M.o2 + st[v.n2] * M.n2 + st[v.co2] * M.co2) / tot;
        const k = Math.min(0.5, (0.4 * dt) / molar / tot);
        const room = Math.max(0, this.o2Cap - st[this.iO2]) + Math.max(0, this.n2Cap - st[this.iN2]);
        if (room > 0.01) {
          st[this.iO2] = Math.min(this.o2Cap, st[this.iO2] + st[v.o2] * k * M.o2);
          st[this.iN2] = Math.min(this.n2Cap, st[this.iN2] + st[v.n2] * k * M.n2);
          st[v.o2] *= 1 - k;
          st[v.n2] *= 1 - k;
          st[v.co2] *= 1 - k;
        }
      }
    }

    // --- flows through the openings (sub-stepped: a big breach empties a cabin in ~1 s) ----------------
    const out = C.map(() => 0);
    const live = paths.filter((g) => g.area > 1e-7 || (g.mix ?? 0) > 0);
    let fastest = 0;
    for (const g of live) {
      const pa = this.pressure(st, g.a);
      const pb = g.b >= 0 ? this.pressure(st, g.b) : 0;
      const up = pa >= pb ? g.a : g.b;
      if (up < 0) continue;
      const na = st[V[up].o2] + st[V[up].n2] + st[V[up].co2];
      if (na < 1e-9) continue;
      const rate = this.molarFlow(st, g, up, pa >= pb ? pb : pa) / na;
      fastest = Math.max(fastest, rate, (g.mix ?? 0) / C[g.a].volume);
    }
    const steps = Math.max(1, Math.min(48, Math.ceil((fastest * dt) / 0.18)));
    const h = dt / steps;
    for (let s = 0; s < steps; s++) {
      for (const g of live) {
        const pa = this.pressure(st, g.a);
        const pb = g.b >= 0 ? this.pressure(st, g.b) : 0;
        if (Math.abs(pa - pb) > 1) {
          const up = pa > pb ? g.a : g.b;
          const down = pa > pb ? g.b : g.a;
          if (up < 0) continue;
          const vu = V[up];
          const nu = st[vu.o2] + st[vu.n2] + st[vu.co2];
          if (nu > 1e-9) {
            let dn = this.molarFlow(st, g, up, Math.min(pa, pb)) * h;
            // never overshoot: at most half of what would equalise (or half the gas, into vacuum)
            if (down >= 0) {
              const eq = Math.abs(pa - pb) / (R * (st[vu.tk] / C[up].volume + st[V[down].tk] / C[down].volume));
              dn = Math.min(dn, eq * 0.5);
            } else dn = Math.min(dn, nu * 0.5);
            this.move(st, up, down, dn / nu, out);
          }
        }
        // fans: equal volumes each way through a duct
        if (g.mix && g.b >= 0) {
          const ka = Math.min(0.3, (g.mix * h) / C[g.a].volume);
          const kb = Math.min(0.3, (g.mix * h) / C[g.b].volume);
          this.exchange(st, g.a, g.b, ka, kb);
        }
      }
    }

    // --- temperature: heaters (or the cold hull) pull the gas back, publish ---------------------------
    for (let i = 0; i < n; i++) {
      const v = V[i];
      const tot = st[v.o2] + st[v.n2] + st[v.co2];
      if (tot < 1e-6) {
        st[v.o2] = st[v.n2] = st[v.co2] = 0;
        st[v.tk] = 250;
      } else {
        const target = life.heat ? 294 : 255;
        st[v.tk] += (target - st[v.tk]) * Math.min(1, dt / 90);
      }
      const P = this.pressure(st, i);
      st[v.p] = P / 1000;
      st[v.po2] = tot > 0 ? (P * st[v.o2]) / tot / 1000 : 0;
      st[v.pco2] = tot > 0 ? (P * st[v.co2]) / tot / 1000 : 0;
      st[v.t] = st[v.tk] - 273.15;
      const dp = (st[v.p] - this.prevP[i]) / dt;
      st[v.dpdt] += (dp - st[v.dpdt]) * Math.min(1, dt * 4);
      this.prevP[i] = st[v.p];
      st[v.out] = out[i] / dt;
      st[v.sealed] = sealed[i] ? 1 : 0;
    }
  }

  /** Compressible orifice flow (mol/s) out of `up` into pressure `pDown`. */
  private molarFlow(st: Float64Array, g: GasPath, up: number, pDown: number) {
    const v = this.v[up];
    const nt = st[v.o2] + st[v.n2] + st[v.co2];
    if (nt <= 0) return 0;
    const Mu = (st[v.o2] * M.o2 + st[v.n2] * M.n2 + st[v.co2] * M.co2) / nt;
    const Tu = Math.max(150, st[v.tk]);
    const Pu = (nt * R * Tu) / this.comps[up].volume;
    const r = Math.max(0, pDown / Pu);
    // γ = 1.4: choked below r* = 0.528; 2γ/(γ−1) = 7, 2/γ = 1.4286, (γ+1)/γ = 1.7143
    const psi = r <= 0.5283 ? 0.6847 : Math.sqrt(Math.max(0, 7 * (r ** 1.4286 - r ** 1.7143)));
    const mdot = CD * g.area * Pu * Math.sqrt(Mu / (R * Tu)) * psi;
    return mdot / Mu;
  }

  /** Move a fraction `k` of compartment `up`'s gas into `down` (or vacuum); the gas left behind expands and cools. */
  private move(st: Float64Array, up: number, down: number, k: number, out: number[]) {
    const vu = this.v[up];
    const o2 = st[vu.o2] * k;
    const n2 = st[vu.n2] * k;
    const co2 = st[vu.co2] * k;
    st[vu.o2] -= o2;
    st[vu.n2] -= n2;
    st[vu.co2] -= co2;
    const tUp = st[vu.tk];
    st[vu.tk] = Math.max(120, tUp * (1 - k) ** 0.4);
    if (down >= 0) {
      const vd = this.v[down];
      const before = st[vd.o2] + st[vd.n2] + st[vd.co2];
      st[vd.o2] += o2;
      st[vd.n2] += n2;
      st[vd.co2] += co2;
      const added = o2 + n2 + co2;
      st[vd.tk] = (st[vd.tk] * before + tUp * added) / Math.max(1e-9, before + added);
    } else out[up] += o2 * M.o2 + n2 * M.n2 + co2 * M.co2;
  }

  /** Mixing: fractions ka of a and kb of b swap places. */
  private exchange(st: Float64Array, a: number, b: number, ka: number, kb: number) {
    const va = this.v[a];
    const vb = this.v[b];
    for (const key of ['o2', 'n2', 'co2'] as const) {
      const fa = st[va[key]] * ka;
      const fb = st[vb[key]] * kb;
      st[va[key]] += fb - fa;
      st[vb[key]] += fa - fb;
    }
    const na = st[va.o2] + st[va.n2] + st[va.co2];
    const nb = st[vb.o2] + st[vb.n2] + st[vb.co2];
    if (na > 1e-6 && nb > 1e-6) {
      const ta = st[va.tk];
      const tb = st[vb.tk];
      st[va.tk] = ta * (1 - ka) + tb * ka;
      st[vb.tk] = tb * (1 - kb) + ta * kb;
    }
  }
}
