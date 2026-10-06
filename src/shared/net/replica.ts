// A thing another machine simulates (a ship its pilot or the server flies, another astronaut, a
// crate someone else throws), as this machine draws it. Its states arrive stamped with the time
// they belong to (the sender's step clock, shared/time/stepClock.ts) — never with when they
// arrived — and are turned into a pose at any time asked for:
//
//   - **In the world (frame 0) it is drawn in the present**: carried forward from the newest state
//     with its velocity and the pull it feels (gravity; a ship's thrust from its last two states).
//     Everything this machine simulates itself is in the present too, so a crate thrown out of a
//     ship at 1.6 km/s, the ship and an astronaut beside it all agree — drawn 120 ms in the past,
//     the ship would sit 190 m behind them.
//   - **Inside a host (a ship's space)** it is drawn `hostDelay` ms in the past, between two
//     states: slow, local numbers, and the host itself is in the present.
//   - When a new state disagrees with what was being drawn (the prediction missed a burn, a
//     frame changed), the difference is not jumped: it is blended away over `smooth` seconds.
//
// No allocation per call: ships sample it every fixed step.

import type { Quat, V3 } from '../ship/geom.js';
import { qMulInto, qSpinInto, slerpInto } from '../frames/track.js';

export interface ReplicaState {
  /** Frame (0 = the world, else a host id); p, v, q, w in it (w: world rate, rad/s). */
  fr: number;
  p: V3;
  v: V3;
  q: Quat;
  w: V3;
}

export interface ReplicaSample extends ReplicaState {
  /** Time the state belongs to (ms, server clock). */
  t: number;
  /** Whatever else the sender says about it (ground contact flags…), handed back untouched. */
  tag?: unknown;
}

export interface ReplicaOptions {
  /** In a host's frame it is drawn this far in the past (ms). */
  hostDelay: number;
  /** Time constant (s) of the blend that hides a correction. */
  smooth?: number;
  /** Carried forward at most this far past the newest state (ms); beyond, it holds. */
  maxAhead?: number;
  /** A correction bigger than this (m) is a jump, not an error: taken at once. */
  snap?: number;
  /**
   * Acceleration it keeps after the newest state (its frame). `measured`: what the last two
   * states' velocities say (a ship's burn and gravity alike) — the default. A walking astronaut
   * says zero (its steps are not a steady pull); a thing in free fall could say gravity.
   */
  accel?: (s: ReplicaSample, measured: V3, out: V3) => V3;
  /** States kept (ring). */
  keep?: number;
}

const MAX_ACCEL = 60;

export class Replica {
  /** The pose drawn at the last `sample` (read it right away: reused). */
  readonly out: ReplicaState & { tag: unknown } = { fr: 0, p: [0, 0, 0], v: [0, 0, 0], q: [0, 0, 0, 1], w: [0, 0, 0], tag: undefined };
  /** Largest correction blended away so far (m; diagnostics). */
  maxCorrection = 0;
  /** Corrections taken as jumps (diagnostics). */
  snaps = 0;
  private samples: ReplicaSample[] = [];
  private readonly opts: Required<Omit<ReplicaOptions, 'accel'>> & Pick<ReplicaOptions, 'accel'>;
  /** Correction still to blend away: position, velocity (frame) and orientation (world, left). */
  private eP: V3 = [0, 0, 0];
  private eV: V3 = [0, 0, 0];
  private eQ: Quat = [0, 0, 0, 1];
  /** Time and raw estimate at the last `sample` (what a new state is compared against). */
  private lastT = NaN;
  private raw: ReplicaState & { tag: unknown } = { fr: 0, p: [0, 0, 0], v: [0, 0, 0], q: [0, 0, 0, 1], w: [0, 0, 0], tag: undefined };
  private alt: ReplicaState & { tag: unknown } = { fr: 0, p: [0, 0, 0], v: [0, 0, 0], q: [0, 0, 0, 1], w: [0, 0, 0], tag: undefined };
  private a: V3 = [0, 0, 0];

  constructor(opts: ReplicaOptions) {
    this.opts = { smooth: 0.25, maxAhead: 1000, snap: 30, keep: 32, ...opts };
  }

  get empty() {
    return this.samples.length === 0;
  }

  /** The newest state (null: none yet). */
  get newest(): ReplicaSample | null {
    return this.samples[this.samples.length - 1] ?? null;
  }

  clear() {
    this.samples.length = 0;
    this.lastT = NaN;
    this.resetError();
  }

  /**
   * A state arrives (out of order ones are put in place; a repeated time replaces). What was
   * drawn at the last `sample` stays drawn: the new estimate's difference becomes a correction.
   */
  push(s: ReplicaSample) {
    const list = this.samples;
    let i = list.length;
    while (i > 0 && list[i - 1].t > s.t) i--;
    if (i > 0 && list[i - 1].t === s.t) list[i - 1] = s;
    else list.splice(i, 0, s);
    if (list.length > this.opts.keep) list.shift();
    if (!Number.isFinite(this.lastT)) return;
    // what was drawn at lastT, and what the states now say about lastT
    const was = this.raw;
    const now = this.estimate(this.lastT, this.alt);
    if (!now || now.fr !== was.fr) {
      this.resetError();
      return;
    }
    const ex = this.eP[0] + was.p[0] - now.p[0];
    const ey = this.eP[1] + was.p[1] - now.p[1];
    const ez = this.eP[2] + was.p[2] - now.p[2];
    const size = Math.sqrt(ex * ex + ey * ey + ez * ez);
    if (size > this.limit(now)) {
      this.snaps++;
      this.resetError();
    } else {
      this.eP[0] = ex;
      this.eP[1] = ey;
      this.eP[2] = ez;
      for (let k = 0; k < 3; k++) this.eV[k] += was.v[k] - now.v[k];
      // orientation drawn = eQ ⊗ raw.q: keep it → eQ' = eQ ⊗ was.q ⊗ now.q⁻¹
      qMulInto(this.eQ, was.q, this.eQ);
      const c = this.qc;
      c[0] = -now.q[0];
      c[1] = -now.q[1];
      c[2] = -now.q[2];
      c[3] = now.q[3];
      qMulInto(this.eQ, c, this.eQ);
      this.maxCorrection = Math.max(this.maxCorrection, size);
    }
    this.copyState(this.raw, now);
  }
  private qc: Quat = [0, 0, 0, 1];

  /**
   * Start from a known state without blending (who simulates it changed hands: the last pose we
   * drew ourselves becomes its first state).
   */
  seed(s: ReplicaSample) {
    this.clear();
    this.samples.push(s);
  }

  /**
   * The pose to draw at time `t` (ms, server clock) into `out`. A world-frame thing is estimated at
   * `t`; one in a host's frame at `t − hostDelay`. Null while nothing has arrived.
   */
  sample(t: number): (ReplicaState & { tag: unknown }) | null {
    const raw = this.estimate(t, this.alt);
    if (!raw) return null;
    if (raw.fr !== this.raw.fr && Number.isFinite(this.lastT)) this.resetError();
    // the correction fades with the time that went by
    if (Number.isFinite(this.lastT) && t > this.lastT) {
      const k = Math.exp(-(t - this.lastT) / 1000 / this.opts.smooth);
      for (let i = 0; i < 3; i++) {
        this.eP[i] *= k;
        this.eV[i] *= k;
      }
      slerpInto(IDENTITY, this.eQ, k, this.eQ);
    }
    this.lastT = t;
    this.copyState(this.raw, raw);
    const o = this.out;
    o.fr = raw.fr;
    o.tag = raw.tag;
    for (let i = 0; i < 3; i++) {
      o.p[i] = raw.p[i] + this.eP[i];
      o.v[i] = raw.v[i] + this.eV[i];
      o.w[i] = raw.w[i];
    }
    qMulInto(this.eQ, raw.q, o.q);
    return o;
  }

  /** Raw estimate (no correction) at time `t` into `out`. */
  private estimate(t: number, out: ReplicaState & { tag: unknown }): (ReplicaState & { tag: unknown }) | null {
    const list = this.samples;
    const n = list.length;
    if (!n) return null;
    const newest = list[n - 1];
    // inside a host, drawn a little in the past (where the frame of the newest state says)
    const at = newest.fr === 0 ? t : t - this.opts.hostDelay;
    if (at >= newest.t) return this.ahead(newest, n > 1 ? list[n - 2] : null, at - newest.t, out);
    if (at <= list[0].t) return this.copyState(out, list[0]);
    let j = n - 1;
    while (j > 0 && list[j - 1].t > at) j--;
    const a = list[j - 1];
    const b = list[j];
    if (a.fr !== b.fr) return this.copyState(out, at - a.t < b.t - at ? a : b);
    return hermite(a, b, (at - a.t) / (b.t - a.t), (b.t - a.t) / 1000, out);
  }

  /** Carried forward `ms` past the newest state with its velocity and pull. */
  private ahead(s: ReplicaSample, before: ReplicaSample | null, ms: number, out: ReplicaState & { tag: unknown }) {
    const tau = Math.min(ms, this.opts.maxAhead) / 1000;
    const a = this.accelOf(s, before);
    out.fr = s.fr;
    out.tag = s.tag;
    for (let i = 0; i < 3; i++) {
      out.p[i] = s.p[i] + s.v[i] * tau + 0.5 * a[i] * tau * tau;
      out.v[i] = s.v[i] + a[i] * tau;
      out.w[i] = s.w[i];
    }
    qSpinInto(s.q, s.w, tau, out.q);
    return out;
  }

  private accelOf(s: ReplicaSample, before: ReplicaSample | null): V3 {
    const a = this.a;
    a[0] = a[1] = a[2] = 0;
    const dt = before && before.fr === s.fr ? (s.t - before.t) / 1000 : 0;
    if (before && dt >= 0.004 && dt <= 0.5) {
      let m = 0;
      for (let i = 0; i < 3; i++) {
        a[i] = (s.v[i] - before.v[i]) / dt;
        m += a[i] * a[i];
      }
      m = Math.sqrt(m);
      if (m > MAX_ACCEL) for (let i = 0; i < 3; i++) a[i] *= MAX_ACCEL / m;
    }
    return this.opts.accel ? this.opts.accel(s, a, a) : a;
  }

  /** How big a correction may be before it counts as a jump: more for faster things. */
  private limit(s: ReplicaState) {
    const v = Math.sqrt(s.v[0] * s.v[0] + s.v[1] * s.v[1] + s.v[2] * s.v[2]);
    return Math.max(this.opts.snap, v * 0.25);
  }

  private resetError() {
    this.eP[0] = this.eP[1] = this.eP[2] = 0;
    this.eV[0] = this.eV[1] = this.eV[2] = 0;
    this.eQ[0] = this.eQ[1] = this.eQ[2] = 0;
    this.eQ[3] = 1;
  }

  private copyState(out: ReplicaState & { tag: unknown }, s: ReplicaState & { tag?: unknown }) {
    out.fr = s.fr;
    out.tag = s.tag;
    for (let i = 0; i < 3; i++) {
      out.p[i] = s.p[i];
      out.v[i] = s.v[i];
      out.w[i] = s.w[i];
    }
    for (let i = 0; i < 4; i++) out.q[i] = s.q[i];
    return out;
  }
}

const IDENTITY: Quat = [0, 0, 0, 1];

/** Cubic Hermite between two states `span` s apart (positions through their velocities). */
function hermite(a: ReplicaSample, b: ReplicaSample, t: number, span: number, out: ReplicaState & { tag: unknown }) {
  const t2 = t * t, t3 = t2 * t;
  const h00 = 2 * t3 - 3 * t2 + 1, h10 = t3 - 2 * t2 + t, h01 = -2 * t3 + 3 * t2, h11 = t3 - t2;
  // derivative (per unit t) for the velocity: continuous with the carried-forward estimate at b
  const d00 = 6 * t2 - 6 * t, d10 = 3 * t2 - 4 * t + 1, d01 = -6 * t2 + 6 * t, d11 = 3 * t2 - 2 * t;
  out.fr = a.fr;
  out.tag = t < 0.5 ? a.tag : b.tag;
  for (let i = 0; i < 3; i++) {
    out.p[i] = h00 * a.p[i] + h10 * span * a.v[i] + h01 * b.p[i] + h11 * span * b.v[i];
    out.v[i] = span > 0 ? (d00 * a.p[i] + d01 * b.p[i]) / span + d10 * a.v[i] + d11 * b.v[i] : b.v[i];
    out.w[i] = a.w[i] + (b.w[i] - a.w[i]) * t;
  }
  slerpInto(a.q, b.q, t, out.q);
  return out;
}
