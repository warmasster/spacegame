// Control allocation: the thruster outputs that best produce the force and torque the flight
// computer asks for. Every variable is a thruster output scaled to its full thrust; its column is
// the wrench (force, torque about the centre of mass, ship axes) it makes at full output. Bounds
// are per variable (0..available) or per vectorable pad (a cone around its axis).
//
//   minimise ½‖W(Bu − w)‖² + cᵀu + ½λ‖u‖²   subject to the bounds
//
// `c` is each variable's propellant price (so the pads carry the weight, not RCS nozzles fighting
// each other), `λ` keeps the problem well posed. Solved with FISTA (accelerated projected
// gradient), warm-started from the last step, Jacobi-preconditioned (a 12 kN pad and a 1.6 kN
// nozzle converge at the same pace) with the step size from a few power iterations. A ship with any thruster layout gets the best mix its thrusters allow; what they cannot
// give simply doesn't happen (the physics sees the real wrench, not the wish).

import type { V3 } from '../geom.js';

/** A bound on some variables: a box on u[j], or a pad's cone on u[j..j+2]. Reused step after step. */
interface Group {
  cone: boolean;
  j: number;
  hi: number;
  lo: number;
  c: number;
}

export class Allocator {
  private n = 0;
  /** Columns, 6 per variable: fx fy fz tx ty tz. */
  private B = new Float64Array(0);
  /** Current solution (warm start for the next step). */
  u = new Float64Array(0);
  private y = new Float64Array(0);
  private old = new Float64Array(0);
  private g = new Float64Array(0);
  private ev = new Float64Array(0);
  /** Linear cost per variable (propellant price of full output). */
  private price = new Float64Array(0);
  /** Jacobi preconditioner (per-variable step scale; equal within a pad's cone). */
  private pre = new Float64Array(0);
  private groups: Group[] = [];
  private nGroups = 0;
  /** Residual scratch (6 rows). */
  private r = new Float64Array(6);
  /** Wrench actually produced by the last solution (same units as the demand). */
  readonly got = new Float64Array(6);

  /** Start a new problem with `n` variables (the warm start survives while the size is the same). */
  begin(n: number) {
    if (n !== this.n) {
      this.n = n;
      this.B = new Float64Array(6 * n);
      this.u = new Float64Array(n);
      this.y = new Float64Array(n);
      this.old = new Float64Array(n);
      this.g = new Float64Array(n);
      this.ev = new Float64Array(n).fill(1);
      this.price = new Float64Array(n);
      this.pre = new Float64Array(n);
    }
    this.nGroups = 0;
  }

  private group(): Group {
    const g = (this.groups[this.nGroups] ??= { cone: false, j: 0, hi: 0, lo: 0, c: 0 });
    this.nGroups++;
    return g;
  }

  /** Column j: the wrench at full output; `price` its linear cost (default none). */
  column(j: number, f: V3, t: V3, price = 0) {
    this.column6(j, f[0], f[1], f[2], t[0], t[1], t[2], price);
  }

  /** Same, component by component (no arrays in the flight step's hot loop). */
  column6(j: number, fx: number, fy: number, fz: number, tx: number, ty: number, tz: number, price = 0) {
    this.price[j] = price;
    const o = j * 6;
    this.B[o] = fx;
    this.B[o + 1] = fy;
    this.B[o + 2] = fz;
    this.B[o + 3] = tx;
    this.B[o + 4] = ty;
    this.B[o + 5] = tz;
  }

  /** 0 ≤ u[j] ≤ hi (signed: −hi ≤ u[j] ≤ hi, a pair of opposed nozzles). */
  box(j: number, hi: number, signed = false) {
    const g = this.group();
    g.cone = false;
    g.j = j;
    g.hi = hi;
    g.lo = signed ? -hi : 0;
  }

  /** Vectorable pad: u[j] axial ≥ 0, |(u[j+1], u[j+2])| ≤ c·u[j], ‖u[j..j+2]‖ ≤ hi. */
  cone(j: number, hi: number, c: number) {
    const g = this.group();
    g.cone = true;
    g.j = j;
    g.hi = hi;
    g.c = c;
  }

  private project(u: Float64Array) {
    for (let k = 0; k < this.nGroups; k++) {
      const gr = this.groups[k];
      if (!gr.cone) {
        u[gr.j] = Math.max(gr.lo, Math.min(gr.hi, u[gr.j]));
        continue;
      }
      const j = gr.j;
      let a = u[j];
      let b = u[j + 1];
      let d = u[j + 2];
      const s = Math.hypot(b, d);
      if (s > gr.c * a) {
        // onto the second-order cone |h| ≤ c·a
        if (gr.c * s <= -a) {
          a = b = d = 0;
        } else {
          const na = (a + gr.c * s) / (1 + gr.c * gr.c);
          const k = s > 1e-12 ? (gr.c * na) / s : 0;
          a = na;
          b *= k;
          d *= k;
        }
      }
      if (a < 0) a = 0;
      const m = Math.hypot(a, b, d);
      if (m > gr.hi) {
        const k = gr.hi / m;
        a *= k;
        b *= k;
        d *= k;
      }
      u[j] = a;
      u[j + 1] = b;
      u[j + 2] = d;
    }
  }

  /** g = Bᵀ W (B u − w) + λu; returns the residual cost. */
  private gradient(u: Float64Array, w: ArrayLike<number>, W: ArrayLike<number>, lambda: number, g: Float64Array) {
    const n = this.n;
    const B = this.B;
    const r = this.r.fill(0);
    for (let j = 0; j < n; j++) {
      const uj = u[j];
      if (uj === 0) continue;
      const o = j * 6;
      for (let k = 0; k < 6; k++) r[k] += B[o + k] * uj;
    }
    let cost = 0;
    for (let k = 0; k < 6; k++) {
      r[k] -= w[k];
      cost += W[k] * r[k] * r[k];
      r[k] *= W[k];
    }
    for (let j = 0; j < n; j++) {
      const o = j * 6;
      g[j] = B[o] * r[0] + B[o + 1] * r[1] + B[o + 2] * r[2] + B[o + 3] * r[3] + B[o + 4] * r[4] + B[o + 5] * r[5] + lambda * u[j] + this.price[j];
    }
    return cost;
  }

  /** Jacobi preconditioner: 1 / diag(BᵀWB + λ), the smallest of a cone group for all three. */
  private precondition(W: ArrayLike<number>, lambda: number) {
    const B = this.B;
    const d = this.pre;
    for (let j = 0; j < this.n; j++) {
      const o = j * 6;
      let a = lambda;
      for (let k = 0; k < 6; k++) a += W[k] * B[o + k] * B[o + k];
      d[j] = 1 / a;
    }
    for (let k = 0; k < this.nGroups; k++) {
      const gr = this.groups[k];
      if (!gr.cone) continue;
      const m = Math.min(d[gr.j], d[gr.j + 1], d[gr.j + 2]);
      d[gr.j] = d[gr.j + 1] = d[gr.j + 2] = m;
    }
  }

  /** Largest eigenvalue of D½ BᵀWB D½ (power iteration, warm-started). */
  private lipschitz(W: ArrayLike<number>) {
    const n = this.n;
    const B = this.B;
    const v = this.ev;
    const d = this.pre;
    let lam = 0;
    for (let it = 0; it < 6; it++) {
      const r = this.r.fill(0);
      for (let j = 0; j < n; j++) {
        const o = j * 6;
        const x = v[j] * Math.sqrt(d[j]);
        for (let k = 0; k < 6; k++) r[k] += B[o + k] * x;
      }
      for (let k = 0; k < 6; k++) r[k] *= W[k];
      let norm = 0;
      for (let j = 0; j < n; j++) {
        const o = j * 6;
        const x = (B[o] * r[0] + B[o + 1] * r[1] + B[o + 2] * r[2] + B[o + 3] * r[3] + B[o + 4] * r[4] + B[o + 5] * r[5]) * Math.sqrt(d[j]);
        v[j] = x;
        norm += x * x;
      }
      norm = Math.sqrt(norm);
      if (norm < 1e-12) {
        v.fill(1);
        return 1;
      }
      for (let j = 0; j < n; j++) v[j] /= norm;
      lam = norm;
    }
    return lam;
  }

  /**
   * Solve for the demand `w` (fx fy fz tx ty tz) with row weights `W`. Returns the outputs (the
   * array is reused: copy what you keep).
   */
  solve(w: ArrayLike<number>, W: ArrayLike<number>, lambda = 1e-3, iters = 40): Float64Array {
    const n = this.n;
    const u = this.u;
    if (n === 0) {
      this.got.fill(0);
      return u;
    }
    this.project(u);
    this.precondition(W, lambda);
    const pre = this.pre;
    let lmin = Infinity;
    for (let j = 0; j < n; j++) lmin = Math.min(lmin, pre[j]);
    const L = this.lipschitz(W) + lambda * lmin;
    const step = 1 / L;
    const y = this.y;
    const old = this.old;
    const g = this.g;
    for (let j = 0; j < n; j++) y[j] = u[j];
    let t = 1;
    for (let it = 0; it < iters; it++) {
      this.gradient(y, w, W, lambda, g);
      for (let j = 0; j < n; j++) {
        old[j] = u[j];
        u[j] = y[j] - step * pre[j] * g[j];
      }
      this.project(u);
      const tn = (1 + Math.sqrt(1 + 4 * t * t)) / 2;
      const k = (t - 1) / tn;
      // adaptive restart: momentum pointing uphill is dropped
      let dotp = 0;
      for (let j = 0; j < n; j++) dotp += (y[j] - u[j]) * (u[j] - old[j]);
      if (dotp > 0) {
        t = 1;
        for (let j = 0; j < n; j++) y[j] = u[j];
        continue;
      }
      for (let j = 0; j < n; j++) y[j] = u[j] + k * (u[j] - old[j]);
      t = tn;
    }
    this.got.fill(0);
    for (let j = 0; j < n; j++) {
      const o = j * 6;
      for (let k = 0; k < 6; k++) this.got[k] += this.B[o + k] * u[j];
    }
    return u;
  }
}
