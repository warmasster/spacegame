/**
 * Fixed-timestep simulation clock. Physics, character movement and projectiles advance in
 * constant steps (deterministic, frame-rate independent); rendering runs once per frame.
 * `alpha` is the leftover fraction for optional render interpolation.
 */
export class FixedLoop {
  private acc = 0;
  alpha = 0;
  /** Steps executed in the last `advance` (diagnostics). */
  lastSteps = 0;

  constructor(
    readonly step = 1 / 60,
    private maxSteps = 6,
  ) {}

  advance(dt: number, fixed: (h: number) => void) {
    this.acc = Math.min(this.acc + dt, this.step * this.maxSteps);
    let n = 0;
    while (this.acc >= this.step) {
      fixed(this.step);
      this.acc -= this.step;
      n++;
    }
    this.lastSteps = n;
    this.alpha = this.acc / this.step;
  }
}
