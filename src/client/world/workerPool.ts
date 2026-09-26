import type { WorkerJob, WorkerResult } from './terrain.worker';

type JobInput = WorkerJob extends infer J ? (J extends { id: number } ? Omit<J, 'id'> : never) : never;

interface Pending {
  job: WorkerJob;
  priority: number;
  resolve: (r: WorkerResult) => void;
  reject: (e: unknown) => void;
  cancelled: boolean;
}

/** Small priority job queue over a few terrain workers (lower priority value = sooner). */
export class TerrainWorkerPool {
  private workers: Worker[] = [];
  private idle: Worker[] = [];
  private queue: Pending[] = [];
  private running = new Map<number, Pending>();
  private nextId = 1;

  constructor(count = Math.max(2, Math.min(4, (navigator.hardwareConcurrency || 4) - 1))) {
    for (let i = 0; i < count; i++) {
      const w = new Worker(new URL('./terrain.worker.ts', import.meta.url), { type: 'module' });
      w.onmessage = (e: MessageEvent<WorkerResult>) => {
        const p = this.running.get(e.data.id);
        this.running.delete(e.data.id);
        this.idle.push(w);
        if (p && !p.cancelled) p.resolve(e.data);
        this.pump();
      };
      w.onerror = (e) => console.error('terrain worker error', e);
      this.workers.push(w);
      this.idle.push(w);
    }
  }

  get busy() {
    return this.queue.length + this.running.size;
  }

  run(input: JobInput, priority: number): { promise: Promise<WorkerResult>; cancel: () => void; setPriority: (p: number) => void } {
    const job = { ...input, id: this.nextId++ } as WorkerJob;
    let pending!: Pending;
    const promise = new Promise<WorkerResult>((resolve, reject) => {
      pending = { job, priority, resolve, reject, cancelled: false };
    });
    this.queue.push(pending);
    this.pump();
    return {
      promise,
      cancel: () => {
        pending.cancelled = true;
        const i = this.queue.indexOf(pending);
        if (i >= 0) this.queue.splice(i, 1);
      },
      setPriority: (p: number) => {
        pending.priority = p;
      },
    };
  }

  private pump() {
    while (this.idle.length && this.queue.length) {
      let best = 0;
      for (let i = 1; i < this.queue.length; i++) if (this.queue[i].priority < this.queue[best].priority) best = i;
      const p = this.queue.splice(best, 1)[0];
      const w = this.idle.pop()!;
      this.running.set(p.job.id, p);
      w.postMessage(p.job);
    }
  }

  dispose() {
    for (const w of this.workers) w.terminate();
  }
}
