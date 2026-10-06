// Runs a world on its own thread (docs/MUNDO.md §7), whatever the thread and the storage are: the
// Node worker and a browser Worker only give it a port and somewhere to save.
//
// Every tick it brings the world up to its clock within a CPU budget (the rest waits: the far
// world can run a little late without anyone noticing), sends its status now and then, saves
// every so often, and answers the engine: facts in, queries out.

import { HOUR, WorldClock } from '../core/calendar.js';
import { World, type SimModule } from '../core/world.js';
import { WorldSaves, type SaveInfo, type SaveStorage } from '../persist/saves.js';
import { digest } from '../persist/snapshot.js';
import { readGauges, Seismograph, type Series } from '../tools/seismograph.js';
import type { RecordView, SimConfig, SimIn, SimPort, SimQuery, SimStatus } from './protocol.js';

export const RUNNER_DEFAULTS: Required<Omit<SimConfig, 'seed'>> = {
  scale: 72,
  budgetMs: 4,
  tickMs: 1000 / 60,
  autosaveS: 60,
  statusS: 1,
  scopeS: HOUR,
};

/** Where the seismograph's series are kept, beside the save. */
export const SERIES_FILE = 'series.json';

const wallNow = () => performance.timeOrigin + performance.now();
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export class SimRunner {
  world: World | null = null;
  private cfg: Required<SimConfig> = { seed: 0, ...RUNNER_DEFAULTS };
  private readonly clock = new WorldClock();
  private readonly saves: WorldSaves | null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private saving: Promise<SaveInfo> | null = null;
  private lastSave: SaveInfo | null = null;
  private saveCount = 0;
  private savedMark = '';
  private lastSaveAt = 0;
  private lastStatusAt = 0;
  private lag = 0;
  private readonly tickMs = new Float64Array(128);
  private ticks = 0;
  /** The modules' gauges over game time (tools/seismograph.ts). */
  private scope = new Seismograph(HOUR);

  constructor(
    private readonly port: SimPort,
    private readonly modules: readonly SimModule[],
    private readonly storage: SaveStorage | null,
    private readonly realNow: () => number = wallNow,
  ) {
    this.saves = storage ? new WorldSaves(storage) : null;
    port.listen((m) => this.handle(m));
  }

  /** Loads the saved world (or creates one), then runs it. */
  async start(config: SimConfig): Promise<void> {
    try {
      this.cfg = { ...RUNNER_DEFAULTS, ...config };
      const loaded = this.saves ? await this.saves.load(this.modules) : null;
      const w = loaded ? loaded.world : World.create({ seed: this.cfg.seed, modules: this.modules });
      w.onError = (err, e) => this.port.post({ type: 'error', message: `${e.kind} (${w.date(e.time)}): ${message(err)}` });
      w.record(loaded ? 'sim.resumed' : 'sim.created', 0, 0, 0, { keep: true });
      this.world = w;
      this.scope = new Seismograph(this.cfg.scopeS);
      const kept = loaded && this.storage ? await this.storage.read(SERIES_FILE).catch(() => null) : null;
      if (kept) {
        try {
          this.scope.load(JSON.parse(new TextDecoder().decode(kept)) as Series);
        } catch {
          // an unreadable series is only lost history
        }
      }
      this.clock.scale = this.cfg.scale;
      this.clock.anchor(this.realNow(), w.now);
      if (this.saves && !loaded) await this.save();
      this.lastSaveAt = this.lastStatusAt = this.realNow();
      this.timer = setInterval(() => this.tick(), this.cfg.tickMs);
      const r = loaded?.report;
      const notes = [
        ...(loaded?.problems ?? []).map((p) => `no se pudo leer ${p}`),
        ...(loaded?.fallback ? ['cargada la partida anterior'] : []),
        ...(r?.notes ?? []),
        ...(r?.orphans.length ? [`tablas sin módulo (se conservan): ${r.orphans.join(', ')}`] : []),
        ...(r?.unknownKinds.length ? [`eventos sin módulo (se ignoran al llegar): ${r.unknownKinds.join(', ')}`] : []),
      ];
      this.port.post({ type: 'ready', status: this.status(), notes });
    } catch (e) {
      this.port.post({ type: 'failed', message: message(e) });
    }
  }

  status(): SimStatus {
    const w = this.world!;
    const n = Math.min(this.ticks, this.tickMs.length);
    const sorted = Array.from(this.tickMs.subarray(0, n)).sort((a, b) => a - b);
    return {
      now: w.now,
      date: w.date(),
      scale: this.clock.scale,
      lag: this.lag,
      entities: w.store.entities.count,
      pending: w.queue.size,
      records: w.log.count,
      events: w.stats.events,
      errors: w.stats.errors,
      tickP95: n ? sorted[Math.min(n - 1, Math.floor(n * 0.95))] : 0,
      budgetMs: this.cfg.budgetMs,
      persistent: this.saves !== null,
      saves: this.saveCount,
      lastSave: this.lastSave,
    };
  }

  save(): Promise<SaveInfo> {
    if (!this.saves || !this.world) return Promise.reject(new Error('este mundo no se guarda'));
    if (this.saving) return this.saving.then(() => this.save());
    const w = this.world;
    this.lastSaveAt = this.realNow();
    const mark = this.mark();
    const series = new TextEncoder().encode(JSON.stringify(this.scope.series()));
    const p = this.saves
      .save(w)
      .then(async (info) => {
        this.lastSave = info;
        this.saveCount++;
        this.savedMark = mark;
        // the seismograph's series beside it (history of the gauges, not the world)
        await this.storage?.write(SERIES_FILE, series).catch(() => undefined);
        return info;
      })
      .finally(() => (this.saving = null));
    this.saving = p;
    return p;
  }

  /** Stops the clock, brings the world up to now and saves it. */
  async stop(): Promise<SaveInfo | null> {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
    const w = this.world;
    if (!w) return null;
    w.advance(this.clock.game(this.realNow()), 250);
    this.clock.setScale(0, this.realNow());
    if (this.saving) await this.saving.catch(() => undefined);
    return this.saves ? this.save() : null;
  }

  /** One tick (the timer calls it; tests may too). */
  tick(): void {
    const w = this.world!;
    const t0 = performance.now();
    const target = this.clock.game(this.realNow());
    try {
      w.advance(target, this.cfg.budgetMs);
    } catch (e) {
      this.port.post({ type: 'error', message: message(e) });
    }
    this.lag = Math.max(0, target - w.now);
    this.flush();
    this.scope.tick(w);
    this.tickMs[this.ticks++ % this.tickMs.length] = performance.now() - t0;
    const real = this.realNow();
    if (real - this.lastStatusAt >= this.cfg.statusS * 1000) {
      this.lastStatusAt = real;
      this.port.post({ type: 'status', status: this.status() });
    }
    if (this.saves && this.cfg.autosaveS > 0 && !this.saving && real - this.lastSaveAt >= this.cfg.autosaveS * 1000 && this.mark() !== this.savedMark) {
      this.save().catch((e) => this.port.post({ type: 'error', message: `guardado: ${message(e)}` }));
    }
  }

  /** Sends on what the modules told the engine. */
  private flush(): void {
    const out = this.world?.outbox;
    if (!out?.length) return;
    this.port.post({ type: 'emit', list: out.splice(0) });
  }

  /** Changes when anything happens (time passing included). */
  private mark(): string {
    const w = this.world!;
    return `${w.now}|${w.log.count}|${w.stats.events}|${w.store.nextId}`;
  }

  private handle(m: SimIn): void {
    const w = this.world;
    switch (m.type) {
      case 'fact':
        if (w) {
          w.fact(m.kind, m.actor, m.subject, m.a, m.data);
          this.flush();
        }
        return;
      case 'scale':
        if (Number.isFinite(m.scale) && m.scale >= 0) this.clock.setScale(m.scale, this.realNow());
        return;
      case 'query':
        return this.answer(m.id, async () => this.query(m.q));
      case 'ask':
        return this.answer(m.id, async () => {
          if (!w) throw new Error('el mundo aún no está listo');
          try {
            return w.request(m.name, m.payload);
          } finally {
            this.flush();
          }
        });
      case 'save':
        return this.answer(m.id, () => this.save());
      case 'stop':
        return this.answer(m.id, () => this.stop());
    }
  }

  private answer(id: number, fn: () => Promise<unknown>): void {
    fn().then(
      (result) => this.port.post({ type: 'answer', id, ok: true, result }),
      (e) => this.port.post({ type: 'answer', id, ok: false, error: message(e) }),
    );
  }

  private query(q: SimQuery): unknown {
    const w = this.world;
    if (!w) throw new Error('el mundo aún no está listo');
    const view = (id: number) => recordView(w, id);
    switch (q.q) {
      case 'status':
        return this.status();
      case 'digest':
        return digest(w);
      case 'gauges':
        return readGauges(w);
      case 'series':
        return this.scope.series();
      case 'record':
        return w.log.has(q.id) ? view(q.id) : null;
      case 'chain':
        return w.log.chain(q.id, q.max).map(view);
      case 'about':
        return w.log.about(q.entity, q.limit).map(view);
      case 'recent': {
        const out: RecordView[] = [];
        for (let id = w.log.count; id >= 1 && out.length < (q.limit ?? 20); id--) out.push(view(id));
        return out;
      }
    }
  }
}

export function recordView(w: World, id: number): RecordView {
  const log = w.log;
  const time = log.time(id);
  return {
    id,
    time,
    date: w.date(time),
    kind: w.syms.name(log.kind(id)),
    actor: log.actor(id),
    subject: log.subject(id),
    a: log.a(id),
    cause: log.cause(id),
    also: log.also(id),
    data: log.data(id),
  };
}
