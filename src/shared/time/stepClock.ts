// The time a fixed step's state belongs to, in the server's clock (ms). Whatever a machine
// simulates is stamped with this time — not when its timer fired, its frame was drawn or its
// message left — so a stream of states is exactly as regular as the simulation that made it, and
// every machine agrees on what "the state at time t" means. At 1.6 km/s one millisecond is 1.6 m:
// a stamp taken from a jittery timer is a ship that jumps back and forth (docs/MOVIMIENTO.md).
//
// The clock advances one step per step. It is kept on its reference (the server's wall clock; a
// client's estimate of it) by running a fraction of a percent fast or slow — a rate, spread over
// the steps — never by moving `t` between two steps: the poses already computed for those steps
// would no longer match the time drawn (2 cm at 250 m/s for every 0.08 ms). It jumps only when it
// is far off (the first sync, a long stall, another server).

export interface StepClockOptions {
  /** Largest rate change used to catch up with the reference (0.005 = 0.5 %: 5 ms per second). */
  slew?: number;
  /** Further off than this (ms) it jumps to the reference instead of slewing. */
  snap?: number;
  /** Errors smaller than this (ms) are left alone (the reference's own jitter). */
  deadband?: number;
  /** An error is worked off over about this long (ms), within `slew`. */
  settle?: number;
}

export class StepClock {
  /** Time (ms) of the state after the last step; NaN until the first `sync`. */
  t = NaN;
  /** Jumps so far (diagnostics: each one is a discontinuity of everything stamped with it). */
  jumps = 0;
  /** Last error seen by `sync` (ms, reference − clock). */
  error = 0;
  /** Current rate change (0.002 = running 0.2 % fast). */
  rate = 0;
  /** Length of the last step (ms, the rate included). */
  last: number;
  private readonly slew: number;
  private readonly snap: number;
  private readonly deadband: number;
  private readonly settle: number;

  constructor(
    /** Length of one fixed step (ms). */
    readonly stepMs: number,
    opts: StepClockOptions = {},
  ) {
    this.slew = opts.slew ?? 0.005;
    this.snap = opts.snap ?? 250;
    this.deadband = opts.deadband ?? 0.25;
    this.settle = opts.settle ?? 1000;
    this.last = stepMs;
  }

  /** One fixed step starts: the time its state will have. */
  step(): number {
    this.last = this.stepMs * (1 + this.rate);
    if (Number.isFinite(this.t)) this.t += this.last;
    return this.t;
  }

  /**
   * The reference says the state after the last step belongs to `ref` (ms). Sets the rate the next
   * steps run at; returns true when the clock had to jump instead (everything interpolated across
   * that step should be snapped).
   */
  sync(ref: number): boolean {
    if (!Number.isFinite(ref)) return false;
    const e = ref - this.t;
    this.error = Number.isFinite(e) ? e : 0;
    if (!Number.isFinite(this.t) || Math.abs(e) > this.snap) {
      this.t = ref;
      this.rate = 0;
      this.jumps++;
      return true;
    }
    const k = Math.abs(e) <= this.deadband ? 0 : e - Math.sign(e) * this.deadband;
    this.rate = Math.max(-this.slew, Math.min(this.slew, k / this.settle));
    return false;
  }

  /**
   * Time drawn this frame: `alpha` of the way from the step before the last to the last (the
   * render interpolates between them, so it shows the world one step late — everything alike).
   */
  renderTime(alpha: number): number {
    return this.t - (1 - alpha) * this.last;
  }
}
