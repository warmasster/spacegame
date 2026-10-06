// Hitches, caught in the act (F3, docs/RENDIMIENTO.md): the worst frame of the last few seconds
// and what took its time — the systems that ran longest in it and the render — so a 50 ms frame
// says whether it was the ground streaming in, the physics, a burst of garbage (no system slow: the
// CPU time is not the frame's) or the GPU (the CPU's share small).

export interface Spike {
  /** The frame (ms, the display's interval). */
  ms: number;
  /** The CPU's time in it (ms). */
  cpu: number;
  /** The longest parts of it (name, ms), longest first. */
  parts: Array<[string, number]>;
  /** Seconds ago. */
  age: number;
}

export class SpikeLog {
  private worst: { ms: number; cpu: number; parts: Array<[string, number]>; t: number } | null = null;
  private t = 0;

  constructor(
    /** Seconds a spike is remembered. */
    readonly window = 5,
  ) {}

  /** A frame: its interval and the CPU's time (ms); `parts` is read only when it is the new worst. */
  feed(intervalMs: number, cpuMs: number, parts: () => Iterable<[string, number]>): void {
    this.t += intervalMs / 1000;
    if (this.worst && this.t - this.worst.t > this.window) this.worst = null;
    if (this.worst && intervalMs <= this.worst.ms) return;
    const list: Array<[string, number]> = [];
    for (const [name, ms] of parts()) if (ms >= 0.5) list.push([name, ms]);
    list.sort((a, b) => b[1] - a[1]);
    this.worst = { ms: intervalMs, cpu: cpuMs, parts: list.slice(0, 4), t: this.t };
  }

  get spike(): Spike | null {
    const w = this.worst;
    return w ? { ms: w.ms, cpu: w.cpu, parts: w.parts, age: this.t - w.t } : null;
  }

  /** One line for F3: "48 ms (CPU 31: terrain 18 · physics 6 · render 4) hace 2 s". */
  describe(): string {
    const s = this.spike;
    if (!s) return '—';
    const parts = s.parts.map(([n, ms]) => `${n} ${ms.toFixed(1)}`).join(' · ');
    return `${s.ms.toFixed(0)} ms (CPU ${s.cpu.toFixed(0)}${parts ? `: ${parts}` : ''}) hace ${s.age.toFixed(0)} s`;
  }
}
