/**
 * Ordered system scheduler. Game logic is split into named systems that run either in the
 * fixed simulation step (`fixed`, deterministic 60 Hz) or once per rendered frame (`frame`).
 * Each system is timed, so F3 shows where the milliseconds go, and new features plug in by
 * registering a system instead of editing the main loop.
 *
 *   sched.add({ name: 'turrets', phase: 'fixed', order: 40, update: (h) => turrets.update(h) });
 */
export type Phase = 'fixed' | 'frame';

export interface System {
  name: string;
  phase: Phase;
  /** Lower runs first. Convention: 10 physics · 20 characters · 30 world objects · 40 gameplay · 90 fx. */
  order: number;
  update(dt: number): void;
  enabled?: boolean;
}

export class Scheduler {
  private systems: System[] = [];
  /** Smoothed milliseconds per system (per frame, summed over fixed steps). */
  readonly timings = new Map<string, number>();
  /** Milliseconds per system in the last frame (raw: what a hitch is made of). */
  readonly lastFrame = new Map<string, number>();
  private frameAcc = new Map<string, number>();

  add(s: System) {
    if (this.systems.some((o) => o.name === s.name)) throw new Error(`system "${s.name}" already registered`);
    this.systems.push(s);
    this.systems.sort((a, b) => a.order - b.order);
    return s;
  }

  remove(name: string) {
    this.systems = this.systems.filter((s) => s.name !== name);
    this.timings.delete(name);
  }

  get(name: string) {
    return this.systems.find((s) => s.name === name);
  }

  list() {
    return this.systems.map((s) => ({ name: s.name, phase: s.phase, order: s.order, enabled: s.enabled !== false }));
  }

  run(phase: Phase, dt: number) {
    for (const s of this.systems) {
      if (s.phase !== phase || s.enabled === false) continue;
      const t0 = performance.now();
      s.update(dt);
      this.frameAcc.set(s.name, (this.frameAcc.get(s.name) ?? 0) + performance.now() - t0);
    }
  }

  /** Call once per rendered frame after all phases ran. */
  endFrame() {
    for (const s of this.systems) {
      const ms = this.frameAcc.get(s.name) ?? 0;
      this.lastFrame.set(s.name, ms);
      this.timings.set(s.name, (this.timings.get(s.name) ?? ms) * 0.9 + ms * 0.1);
    }
    this.frameAcc.clear();
  }
}
