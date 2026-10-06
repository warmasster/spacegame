// Keeps the frame rate up on whatever GPU (docs/RENDIMIENTO.md). It watches every frame — how long
// it took (the display's interval), how much of it the CPU spent, and the GPU's time (a timer query
// when the browser gives one; otherwise the interval minus the CPU's time) — and when the GPU is what
// holds the frame back for a while, it takes the image down one notch; when there is room again, back
// up one. A frame the CPU holds back gains nothing from fewer pixels: then it changes nothing.
//
// Notches, the least visible loss first: MSAA off, resolution 85 %, 70 %, bloom off, 60 %, 50 %.
// Pure: the render pipeline applies what it decides, a test feeds it frames.

export interface Notch {
  scale: number;
  msaa: boolean;
  bloom: boolean;
}

export const NOTCHES: readonly Notch[] = [
  { scale: 1, msaa: true, bloom: true },
  { scale: 1, msaa: false, bloom: true },
  { scale: 0.85, msaa: false, bloom: true },
  { scale: 0.7, msaa: false, bloom: true },
  { scale: 0.7, msaa: false, bloom: false },
  { scale: 0.6, msaa: false, bloom: false },
  { scale: 0.5, msaa: false, bloom: false },
];

export const GOVERNOR = {
  /** Seconds after start before it decides anything (shader compiles, terrain streaming). */
  warmup: 8,
  /** Seconds of frames each decision looks at. */
  window: 2,
  /** After a step down, seconds before it may step back up (no see-saw; each change reallocates buffers: a hitch). */
  holdUp: 15,
  /** Slow: the median frame over the target by this much. */
  slow: 1.15,
  /** Room to go up: the GPU's time under this share of the target. */
  room: 0.55,
  /** The GPU holds the frame back when its time is at least this share of the CPU's. */
  gpuShare: 0.8,
};

export interface GovernorOptions {
  /** Notch it may go down to (the lowest allowed: its index). */
  lowest?: number;
  /** Start at this notch (MSAA-less profiles start past the first). */
  start?: number;
}

/** The frame it aims at (ms): 60 per second (a faster display gains nothing from it lowering the image). */
const TARGET_MS = 1000 / 60;

export class QualityGovernor {
  notch: number;
  private readonly lowest: number;
  private frames: number[] = [];
  private cpu: number[] = [];
  private gpu: number[] = [];
  private sum = 0;
  private t = 0;
  private lastDown = -Infinity;
  /** Times going up to each notch didn't hold (each doubles the wait before trying it again). */
  private readonly fails = new Map<number, number>();
  private triedUp = { to: -1, at: -Infinity };
  private gpuKnown = false;

  constructor(o: GovernorOptions = {}) {
    this.notch = o.start ?? 0;
    this.lowest = Math.min(NOTCHES.length - 1, o.lowest ?? NOTCHES.length - 1);
  }

  get current(): Notch {
    return NOTCHES[this.notch];
  }

  /**
   * One frame: its interval (ms), the CPU's time in it (ms) and the GPU's if measured (ms, or null).
   * Returns the new notch when it changes, else null.
   */
  frame(intervalMs: number, cpuMs: number, gpuMs: number | null): Notch | null {
    this.t += intervalMs / 1000;
    // the first seconds are not the steady load (shaders compiling, the ground streaming in)
    if (this.t < GOVERNOR.warmup) return null;
    this.frames.push(intervalMs);
    this.cpu.push(cpuMs);
    if (gpuMs !== null) this.gpu.push(gpuMs);
    this.sum += intervalMs;
    if (this.sum < GOVERNOR.window * 1000) return null;
    const frame = median(this.frames);
    const cpu = median(this.cpu);
    this.gpuKnown = this.gpu.length > 0;
    const gpu = this.gpuKnown ? median(this.gpu) : Math.max(0, frame - cpu);
    this.frames = [];
    this.cpu = [];
    this.gpu = [];
    this.sum = 0;
    let next = this.notch;
    if (frame > TARGET_MS * GOVERNOR.slow && gpu >= cpu * GOVERNOR.gpuShare && this.notch < this.lowest) {
      next = this.notch + 1;
      // it had just gone up to here: that notch doesn't hold, wait longer before trying it again
      if (this.triedUp.to === this.notch && this.t - this.triedUp.at < GOVERNOR.window * 3) this.fails.set(this.notch, (this.fails.get(this.notch) ?? 0) + 1);
    } else if (this.notch > 0 && frame <= TARGET_MS * 1.05) {
      // room: measured when the GPU's time is known; otherwise tried (and undone if it doesn't hold)
      const room = !this.gpuKnown || gpu < TARGET_MS * GOVERNOR.room;
      const wait = GOVERNOR.holdUp * 2 ** (this.fails.get(this.notch - 1) ?? 0);
      if (room && this.t - this.lastDown >= wait && this.t - this.triedUp.at >= wait) {
        next = this.notch - 1;
        this.triedUp = { to: next, at: this.t };
      }
    }
    if (next === this.notch) return null;
    if (next > this.notch) this.lastDown = this.t;
    this.notch = next;
    return NOTCHES[next];
  }
}

function median(v: number[]): number {
  if (!v.length) return 0;
  const s = [...v].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}
