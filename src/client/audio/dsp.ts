// Synthesis toolkit: every sound of the game is made here from numbers (no audio files). Mono
// Float32Array buffers at the context's sample rate; recipes live in ./sounds/ and use these
// pieces: noises, filters (fixed or swept), damped modes (struck metal, plastic, glass), tones,
// grains, envelopes, and the seam that turns a take into a seamless loop.
//
// Runs once per sound at start-up (bank.ts renders them in slices), never in a frame.

/** Seeded random numbers (xorshift32): the same take every run, another one per variant. */
export class Rng {
  private s: number;
  constructor(seed: number) {
    this.s = seed >>> 0 || 0x9e3779b9;
  }
  next(): number {
    let x = this.s;
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    this.s = x >>> 0;
    return this.s / 4294967296;
  }
  /** −1..1 */
  bi() {
    return this.next() * 2 - 1;
  }
  range(a: number, b: number) {
    return a + (b - a) * this.next();
  }
}

/** What a recipe gets: the sample rate, its random numbers and which take it is making. */
export interface Synth {
  sr: number;
  rng: Rng;
  variant: number;
}

export type Buf = Float32Array;
export type Curve = (t: number) => number;

export const buf = (s: Synth, seconds: number): Buf => new Float32Array(Math.max(1, Math.round(seconds * s.sr)));

// ------------------------------------------------------------------------------------------------
// Noises
// ------------------------------------------------------------------------------------------------

/** White noise into `b` (added), times `amp` or an envelope. */
export function noise(s: Synth, b: Buf, amp: number | Curve = 1, from = 0) {
  const r = s.rng;
  const i0 = Math.round(from * s.sr);
  for (let i = i0; i < b.length; i++) b[i] += (typeof amp === 'number' ? amp : amp((i - i0) / s.sr)) * r.bi();
  return b;
}

/** Pink noise (−3 dB/octave, Kellet's filter), added. */
export function pink(s: Synth, b: Buf, amp: number | Curve = 1) {
  const r = s.rng;
  let b0 = 0, b1 = 0, b2 = 0, b3 = 0, b4 = 0, b5 = 0, b6 = 0;
  for (let i = 0; i < b.length; i++) {
    const w = r.bi();
    b0 = 0.99886 * b0 + w * 0.0555179;
    b1 = 0.99332 * b1 + w * 0.0750759;
    b2 = 0.969 * b2 + w * 0.153852;
    b3 = 0.8665 * b3 + w * 0.3104856;
    b4 = 0.55 * b4 + w * 0.5329522;
    b5 = -0.7616 * b5 - w * 0.016898;
    const p = (b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362) * 0.11;
    b6 = w * 0.115926;
    b[i] += (typeof amp === 'number' ? amp : amp(i / s.sr)) * p;
  }
  return b;
}

/** Brown noise (−6 dB/octave: rumble), added. */
export function brown(s: Synth, b: Buf, amp: number | Curve = 1) {
  const r = s.rng;
  let y = 0;
  for (let i = 0; i < b.length; i++) {
    y = (y + 0.02 * r.bi()) / 1.02;
    b[i] += (typeof amp === 'number' ? amp : amp(i / s.sr)) * y * 3.5;
  }
  return b;
}

/**
 * Crackle: random clicks, `rate` per second, each a tiny decaying burst of noise (`tau` s) of random
 * size — fire, arcs, gravel, an engine running rough. Added.
 */
export function crackle(s: Synth, b: Buf, rate: number | Curve, amp: number | Curve = 1, tau = 0.002) {
  const r = s.rng;
  const k = Math.exp(-1 / (tau * s.sr));
  let e = 0;
  let sign = 1;
  for (let i = 0; i < b.length; i++) {
    const t = i / s.sr;
    const rt = typeof rate === 'number' ? rate : rate(t);
    if (r.next() < rt / s.sr) {
      e = Math.pow(r.next(), 2.2);
      sign = r.next() < 0.5 ? -1 : 1;
    }
    b[i] += (typeof amp === 'number' ? amp : amp(t)) * e * sign * (0.6 + 0.4 * r.bi());
    e *= k;
  }
  return b;
}

// ------------------------------------------------------------------------------------------------
// Filters (RBJ biquads)
// ------------------------------------------------------------------------------------------------

export type FilterType = 'lp' | 'hp' | 'bp' | 'peak' | 'notch' | 'ls' | 'hs';

export class Biquad {
  private b0 = 1;
  private b1 = 0;
  private b2 = 0;
  private a1 = 0;
  private a2 = 0;
  private x1 = 0;
  private x2 = 0;
  private y1 = 0;
  private y2 = 0;

  constructor(
    private sr: number,
    readonly type: FilterType,
  ) {}

  set(f: number, q = 0.707, db = 0) {
    const w = (2 * Math.PI * Math.max(5, Math.min(f, this.sr * 0.49))) / this.sr;
    const cs = Math.cos(w);
    const sn = Math.sin(w);
    const al = sn / (2 * Math.max(0.05, q));
    const A = Math.pow(10, db / 40);
    let b0 = 1, b1 = 0, b2 = 0, a0 = 1, a1 = 0, a2 = 0;
    switch (this.type) {
      case 'lp':
        b0 = (1 - cs) / 2; b1 = 1 - cs; b2 = (1 - cs) / 2; a0 = 1 + al; a1 = -2 * cs; a2 = 1 - al;
        break;
      case 'hp':
        b0 = (1 + cs) / 2; b1 = -(1 + cs); b2 = (1 + cs) / 2; a0 = 1 + al; a1 = -2 * cs; a2 = 1 - al;
        break;
      case 'bp':
        b0 = al; b1 = 0; b2 = -al; a0 = 1 + al; a1 = -2 * cs; a2 = 1 - al;
        break;
      case 'notch':
        b0 = 1; b1 = -2 * cs; b2 = 1; a0 = 1 + al; a1 = -2 * cs; a2 = 1 - al;
        break;
      case 'peak':
        b0 = 1 + al * A; b1 = -2 * cs; b2 = 1 - al * A; a0 = 1 + al / A; a1 = -2 * cs; a2 = 1 - al / A;
        break;
      case 'ls': {
        const sq = 2 * Math.sqrt(A) * al;
        b0 = A * (A + 1 - (A - 1) * cs + sq); b1 = 2 * A * (A - 1 - (A + 1) * cs); b2 = A * (A + 1 - (A - 1) * cs - sq);
        a0 = A + 1 + (A - 1) * cs + sq; a1 = -2 * (A - 1 + (A + 1) * cs); a2 = A + 1 + (A - 1) * cs - sq;
        break;
      }
      case 'hs': {
        const sq = 2 * Math.sqrt(A) * al;
        b0 = A * (A + 1 + (A - 1) * cs + sq); b1 = -2 * A * (A - 1 + (A + 1) * cs); b2 = A * (A + 1 + (A - 1) * cs - sq);
        a0 = A + 1 - (A - 1) * cs + sq; a1 = 2 * (A - 1 - (A + 1) * cs); a2 = A + 1 - (A - 1) * cs - sq;
        break;
      }
    }
    this.b0 = b0 / a0;
    this.b1 = b1 / a0;
    this.b2 = b2 / a0;
    this.a1 = a1 / a0;
    this.a2 = a2 / a0;
    return this;
  }

  run(x: number) {
    const y = this.b0 * x + this.b1 * this.x1 + this.b2 * this.x2 - this.a1 * this.y1 - this.a2 * this.y2;
    this.x2 = this.x1;
    this.x1 = x;
    this.y2 = this.y1;
    this.y1 = y;
    return y;
  }
}

/** Filter `b` in place (fixed). */
export function filter(s: Synth, b: Buf, type: FilterType, f: number, q = 0.707, db = 0) {
  const bq = new Biquad(s.sr, type).set(f, q, db);
  for (let i = 0; i < b.length; i++) b[i] = bq.run(b[i]);
  return b;
}

/** Filter `b` in place with a moving cutoff `f(t)` (and resonance `q(t)`). */
export function sweep(s: Synth, b: Buf, type: FilterType, f: Curve, q: number | Curve = 0.707, db = 0) {
  const bq = new Biquad(s.sr, type);
  for (let i = 0; i < b.length; i++) {
    if ((i & 31) === 0) {
      const t = i / s.sr;
      bq.set(f(t), typeof q === 'number' ? q : q(t), db);
    }
    b[i] = bq.run(b[i]);
  }
  return b;
}

// ------------------------------------------------------------------------------------------------
// Tones and struck objects
// ------------------------------------------------------------------------------------------------

/** Waveshapes for `tone`, as harmonic series (band-limited additive). */
const SHAPES: Record<string, (k: number) => number> = {
  sine: (k) => (k === 1 ? 1 : 0),
  saw: (k) => 1 / k,
  square: (k) => (k % 2 === 1 ? 1 / k : 0),
  tri: (k) => (k % 2 === 1 ? ((((k - 1) / 2) % 2 === 0 ? 1 : -1) / (k * k)) : 0),
};

/**
 * A tone whose frequency follows `f(t)` (Hz) and amplitude `amp(t)`, added from `from` s. Shapes
 * other than sine are summed harmonics up to `maxHz` (no aliasing).
 */
export function tone(s: Synth, b: Buf, f: number | Curve, amp: number | Curve, shape: keyof typeof SHAPES = 'sine', from = 0, maxHz = 8000, dur = Infinity) {
  const i0 = Math.round(from * s.sr);
  const i1 = Math.min(b.length, i0 + Math.round(Math.min(dur, 1e4) * s.sr));
  const h = SHAPES[shape];
  let ph = s.rng.next();
  for (let i = i0; i < i1; i++) {
    const t = (i - i0) / s.sr;
    const fr = typeof f === 'number' ? f : f(t);
    ph += fr / s.sr;
    if (ph > 1) ph -= Math.floor(ph);
    let v = 0;
    const top = shape === 'sine' ? 1 : Math.min(40, Math.floor(maxHz / Math.max(fr, 1)));
    for (let k = 1; k <= top; k++) {
      const a = h(k);
      if (a !== 0) v += a * Math.sin(2 * Math.PI * k * ph);
    }
    b[i] += (typeof amp === 'number' ? amp : amp(t)) * v;
  }
  return b;
}

/**
 * A struck object: damped sines `[Hz, decay s, amplitude]` starting at `at` s — metal rings long
 * and inharmonic, plastic short and dull. Added.
 */
export function modes(s: Synth, b: Buf, list: Array<[number, number, number]>, at = 0, gain = 1) {
  const i0 = Math.round(at * s.sr);
  for (const [f, tau, a] of list) {
    const w = (2 * Math.PI * f) / s.sr;
    const k = Math.exp(-1 / (Math.max(1e-4, tau) * s.sr));
    let e = a * gain;
    const ph = s.rng.next() * 0.3;
    // a click of attack: the first samples ramp in over 0.3 ms
    const ramp = Math.max(1, Math.round(0.0003 * s.sr));
    for (let i = i0, n = 0; i < b.length && e > 1e-5; i++, n++) {
      b[i] += e * Math.sin(w * n + ph) * (n < ramp ? n / ramp : 1);
      e *= k;
    }
  }
  return b;
}

/** A short burst of filtered noise (the contact of a strike), added at `at` s. */
export function tick(s: Synth, b: Buf, at: number, dur: number, f: number, amp = 1, type: FilterType = 'bp', q = 1.2) {
  const t = buf(s, dur);
  noise(s, t, (x) => Math.exp(-x / (dur * 0.25)));
  filter(s, t, type, f, q);
  return add(s, b, t, amp, at);
}

/** A thump: a sine dropping in pitch (`f0` → `f1`) with a quick decay — boots, impacts, blasts. Added. */
export function thump(s: Synth, b: Buf, at: number, f0: number, f1: number, tau: number, amp = 1) {
  const i0 = Math.round(at * s.sr);
  let ph = 0;
  for (let i = i0; i < b.length; i++) {
    const t = (i - i0) / s.sr;
    const e = Math.exp(-t / tau);
    if (e < 1e-4) break;
    const f = f1 + (f0 - f1) * Math.exp(-t / (tau * 0.6));
    ph += (2 * Math.PI * f) / s.sr;
    b[i] += amp * e * Math.sin(ph) * Math.min(1, t * s.sr / 24);
  }
  return b;
}

// ------------------------------------------------------------------------------------------------
// Envelopes and mixing
// ------------------------------------------------------------------------------------------------

/** Multiply `b` by `f(t)`. */
export function shape(s: Synth, b: Buf, f: Curve) {
  for (let i = 0; i < b.length; i++) b[i] *= f(i / s.sr);
  return b;
}

/** Attack then exponential decay (time constant `tau`). */
export const ad = (attack: number, tau: number, hold = 0): Curve => (t) => (t < attack ? t / attack : t < attack + hold ? 1 : Math.exp(-(t - attack - hold) / tau));

/** Smooth window: rises over `a`, falls over the last `r` of `len` seconds. */
export const win = (len: number, a: number, r: number): Curve => (t) => (t < a ? 0.5 - 0.5 * Math.cos((Math.PI * t) / a) : t > len - r ? Math.max(0, 0.5 + 0.5 * Math.cos((Math.PI * (t - (len - r))) / r)) : 1);

/** Add `src` into `dst` at `at` seconds, times `gain`. */
export function add(s: Synth, dst: Buf, src: Buf, gain = 1, at = 0) {
  const i0 = Math.round(at * s.sr);
  const n = Math.min(src.length, dst.length - i0);
  for (let i = 0; i < n; i++) dst[i0 + i] += src[i] * gain;
  return dst;
}

/** Soft saturation (tanh), `drive` > 1 squashes harder. */
export function saturate(b: Buf, drive: number) {
  const k = Math.tanh(drive);
  for (let i = 0; i < b.length; i++) b[i] = Math.tanh(b[i] * drive) / k;
  return b;
}

/** Remove any DC offset (one-pole high-pass at ~10 Hz). */
export function dc(s: Synth, b: Buf) {
  const R = 1 - (2 * Math.PI * 10) / s.sr;
  let x1 = 0;
  let y1 = 0;
  for (let i = 0; i < b.length; i++) {
    const y = b[i] - x1 + R * y1;
    x1 = b[i];
    y1 = y;
    b[i] = y;
  }
  return b;
}

/** Scale to a peak of `peak`. */
export function normalize(b: Buf, peak = 0.9) {
  let m = 0;
  for (let i = 0; i < b.length; i++) m = Math.max(m, Math.abs(b[i]));
  if (m > 1e-9) for (let i = 0; i < b.length; i++) b[i] *= peak / m;
  return b;
}

/** Fade the ends (ms-scale: no clicks at start and stop of one-shots). */
export function edges(s: Synth, b: Buf, fadeIn = 0.001, fadeOut = 0.01) {
  const a = Math.max(1, Math.round(fadeIn * s.sr));
  const r = Math.max(1, Math.round(fadeOut * s.sr));
  for (let i = 0; i < Math.min(a, b.length); i++) b[i] *= i / a;
  for (let i = 0; i < Math.min(r, b.length); i++) b[b.length - 1 - i] *= i / r;
  return b;
}

/**
 * A seamless loop: the last `xfade` seconds are blended into the start (equal power) and cut
 * off, so the end runs straight into the beginning.
 */
export function loopify(s: Synth, b: Buf, xfade = 0.25): Buf {
  const x = Math.min(Math.round(xfade * s.sr), Math.floor(b.length / 3));
  const n = b.length - x;
  const out = b.slice(0, n);
  for (let i = 0; i < x; i++) {
    const w = i / x;
    out[i] = b[i] * Math.sin((w * Math.PI) / 2) + b[n + i] * Math.cos((w * Math.PI) / 2);
  }
  return out;
}

/** A frequency nudged to a whole number of cycles in `L` s: a tone that loops without a seam. */
export const fit = (f: number, L: number) => Math.max(1, Math.round(f * L)) / L;

/**
 * A loop of exactly `L` seconds: `noisy` fills the noise layers (given L + `seam` seconds, blended
 * at the seam), `tonal` adds tones fitted to L (`fit`: exactly periodic, no blend). Normalised.
 */
export function loopOf(s: Synth, L: number, noisy: ((b: Buf) => void) | null, tonal: ((b: Buf) => void) | null, peak = 0.85, seam = 0.3): Buf {
  const n = Math.round(L * s.sr);
  const out = new Float32Array(n);
  if (noisy) {
    const b = new Float32Array(n + Math.round(seam * s.sr));
    noisy(b);
    const l = loopify(s, b, seam);
    for (let i = 0; i < n && i < l.length; i++) out[i] += l[i];
  }
  if (tonal) tonal(out);
  return normalize(out, peak);
}

/** Slow random wobble 0..1 (smoothed noise at about `hz`), for flutter and turbulence. */
export function wobble(s: Synth, seconds: number, hz: number): Curve {
  const n = Math.max(4, Math.ceil(seconds * hz) + 3);
  const pts = Array.from({ length: n }, () => s.rng.next());
  // periodic over `seconds` (loops stay seamless): the last points repeat the first
  pts[n - 1] = pts[0];
  return (t) => {
    const u = ((t / seconds) * (n - 1)) % (n - 1);
    const i = Math.floor(u);
    const f = u - i;
    const a = pts[i];
    const c = pts[Math.min(n - 1, i + 1)];
    return a + (c - a) * (0.5 - 0.5 * Math.cos(Math.PI * f));
  };
}
