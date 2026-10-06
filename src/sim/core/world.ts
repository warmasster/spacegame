// The world simulation (docs/MUNDO.md): no graphics, no physics, no platform. Entities and their
// tables, the timeline, the log of causes, deterministic randomness and game time — and the modules
// (economy, factions, cohorts…) that give them meaning. The 3D engine only asks it what is where
// and tells it what the player did.
//
// Determinism: with the same seed, modules and facts, the history is the same — however its time is
// sliced into `advance` calls, and across a save and a load. Module code keeps that by using only
// `now`, `random` and the tables (never Date, Math.random or state outside the world).

import { Calendar, type CalendarDef } from './calendar.js';
import { Chronicle } from './chronicle.js';
import { EventQueue, type Popped } from './queue.js';
import { draw, hash3 } from './rng.js';
import { defineComponent, Store, type ComponentDef, type Fields, type Table, type TableData } from './store.js';
import { Symbols } from './symbols.js';

/** An event being handled. One object reused for every event: don't keep it. */
export interface SimEvent {
  readonly kind: string;
  readonly time: number;
  readonly target: number;
  readonly a: number;
  readonly b: number;
  readonly data: unknown;
  /** The record that led to it (0: none). */
  readonly cause: number;
  readonly handle: number;
}

export interface EventDef {
  readonly name: string;
  /** Among events at the same time, lower orders go first (then by target id). */
  readonly order?: number;
  readonly run?: (w: World, e: SimEvent) => void;
}

/** A system of the world: its data, its events and how it seeds a new world. */
export interface SimModule {
  readonly name: string;
  readonly components?: ReadonlyArray<ComponentDef<any>>;
  readonly events?: readonly EventDef[];
  /** A new world: create its things and schedule their first events. */
  readonly create?: (w: World) => void;
  /** A loaded world (rebuild anything not kept in tables). */
  readonly loaded?: (w: World) => void;
  /**
   * What the engine can ask this module, by name (`w.request`): answers now, may change the world
   * (a request is an input, like a fact). Names are global: prefix them with the module's.
   */
  readonly requests?: Readonly<Record<string, (w: World, payload: any) => unknown>>;
  /** What an entity is, if it is one of this module's (for "what is here": plain data, or null). */
  readonly describe?: (w: World, id: number) => Record<string, unknown> | null;
  /**
   * Numbers that say how this part of the world is doing (the seismograph's series, the seed
   * sweep's distributions: docs/MUNDO.md §13). Read-only; names global: prefix them.
   */
  readonly gauges?: Readonly<Record<string, (w: World) => number>>;
}

export interface WorldInit {
  readonly seed: number;
  readonly modules?: readonly SimModule[];
  readonly calendar?: CalendarDef;
  /** Game time it starts at (s). */
  readonly start?: number;
  /** Records per log chunk (a power of two). */
  readonly chunkSize?: number;
}

export interface RecordOptions {
  /** Its cause (default: the current one — the record behind the event being handled). */
  readonly cause?: number;
  readonly also?: readonly number[];
  readonly data?: unknown;
  /** Don't make it the cause of what is done next in this event. */
  readonly keep?: boolean;
}

interface EventKind {
  readonly name: string;
  readonly order: number;
  run?: (w: World, e: SimEvent) => void;
}

const RNG = defineComponent('$rng', { n: 'f64' });
const RNG_SALT = 0x524e47;

export class World {
  readonly seed: number;
  readonly calendar: Calendar;
  readonly store = new Store();
  readonly queue = new EventQueue();
  readonly log: Chronicle;
  readonly syms = new Symbols();
  readonly modules: readonly SimModule[];
  /** Game time (s). */
  now: number;
  /** The record behind what is being done (what new records and events descend from). */
  cause = 0;
  readonly stats = { events: 0, unhandled: 0, errors: 0 };
  /** Tables of a save that no module of this run declares: kept as they are and saved again. */
  readonly orphans = new Map<string, TableData>();
  /**
   * What modules tell the engine (`emit`): taken by the host after each tick and sent on. Output
   * only: the world never reads it back (it isn't saved).
   */
  readonly outbox: Array<[channel: string, data: unknown]> = [];
  /** A module's event threw (default: rethrow). The world goes on with the next event. */
  onError: ((err: unknown, e: SimEvent) => void) | null = null;
  private readonly byDef = new Map<ComponentDef<any>, Table>();
  private readonly handlers = new Map<string, (w: World, payload: any) => unknown>();
  private readonly kinds: EventKind[] = [];
  private readonly kindIndex = new Map<string, number>();
  private readonly rng: Table<typeof RNG.fields>;
  private readonly popped: Popped = { time: 0, kind: 0, target: 0, a: 0, b: 0, cause: 0, data: undefined, handle: 0 };
  private readonly ev = { kind: '', time: 0, target: 0, a: 0, b: 0, data: undefined as unknown, cause: 0, handle: 0 };

  /** A world with its modules registered but nothing created (`World.create` for a new one). */
  constructor(init: WorldInit) {
    this.seed = init.seed >>> 0;
    this.calendar = new Calendar(init.calendar);
    this.now = init.start ?? 0;
    this.log = new Chronicle(init.chunkSize);
    this.modules = init.modules ?? [];
    this.rng = this.table(RNG);
    for (const m of this.modules) {
      for (const c of m.components ?? []) this.table(c);
      for (const e of m.events ?? []) this.define(e);
      for (const [name, fn] of Object.entries(m.requests ?? {})) {
        if (this.handlers.has(name)) throw new Error(`petición ${name} definida dos veces`);
        this.handlers.set(name, fn);
      }
    }
  }

  /** A new world: every module creates its part, in order. */
  static create(init: WorldInit): World {
    const w = new World(init);
    for (const m of w.modules) {
      w.cause = 0;
      m.create?.(w);
    }
    w.cause = 0;
    return w;
  }

  // --- entities ---------------------------------------------------------------------------------

  spawn(): number {
    return this.store.spawn();
  }

  despawn(id: number): boolean {
    return this.store.despawn(id);
  }

  alive(id: number): boolean {
    return this.store.alive(id);
  }

  /** The table of a component (registered on first use; a saved one that was waiting is adopted). */
  table<F extends Fields>(def: ComponentDef<F>): Table<F> {
    const known = this.byDef.get(def);
    if (known) return known as unknown as Table<F>;
    const had = this.store.has(def.name);
    const t = this.store.table(def);
    if (!had) {
      const o = this.orphans.get(def.name);
      if (o) {
        this.orphans.delete(def.name);
        t.load(o);
      }
    }
    this.byDef.set(def, t as unknown as Table);
    return t;
  }

  // --- time -------------------------------------------------------------------------------------

  /** Registers an event kind (modules do it for theirs). */
  define(e: EventDef): number {
    const k = this.kindIndex.get(e.name);
    if (k !== undefined) {
      const kind = this.kinds[k];
      if (kind.run && e.run && kind.run !== e.run) throw new Error(`evento ${e.name} definido dos veces`);
      kind.run ??= e.run;
      return k;
    }
    this.kinds.push({ name: e.name, order: e.order ?? 0, run: e.run });
    this.kindIndex.set(e.name, this.kinds.length - 1);
    return this.kinds.length - 1;
  }

  /** Every kind, by index (as saved). */
  kindNames(): string[] {
    return this.kinds.map((k) => k.name);
  }

  /** Index of a kind by name, registering it (without a handler) if it is new. */
  kindOf(kind: string | EventDef): number {
    const name = typeof kind === 'string' ? kind : kind.name;
    return this.kindIndex.get(name) ?? this.define(typeof kind === 'string' ? { name } : kind);
  }

  /**
   * Schedules an event at game time `at` (not before now) for `target`; returns its handle (0 and
   * nothing scheduled if `at` is Infinity: "never"). Its cause is the current one.
   */
  schedule(at: number, kind: string | EventDef, target = 0, a = 0, b = 0, data?: unknown): number {
    if (at === Infinity) return 0;
    if (Number.isNaN(at)) throw new Error(`${typeof kind === 'string' ? kind : kind.name}: tiempo NaN`);
    const k = this.kindOf(kind);
    return this.queue.push(at > this.now ? at : this.now, this.kinds[k].order, k, target, a, b, this.cause, data);
  }

  /** Schedules an event `dt` seconds from now. */
  after(dt: number, kind: string | EventDef, target = 0, a = 0, b = 0, data?: unknown): number {
    return this.schedule(this.now + dt, kind, target, a, b, data);
  }

  cancel(handle: number): boolean {
    return this.queue.cancel(handle);
  }

  /**
   * Moves a pending event to `at` (Infinity cancels it); its cause becomes the current one. Returns
   * the handle to keep: the same one, or 0 if it was cancelled or wasn't pending.
   */
  move(handle: number, at: number): number {
    if (at === Infinity) {
      this.queue.cancel(handle);
      return 0;
    }
    return this.queue.move(handle, at > this.now ? at : this.now, this.cause) ? handle : 0;
  }

  pending(handle: number): boolean {
    return this.queue.valid(handle);
  }

  /** When a pending event will happen (NaN if it isn't pending). */
  when(handle: number): number {
    return this.queue.timeOf(handle);
  }

  /**
   * Runs every event up to game time `to`, in order. With a budget (ms) it stops early when the
   * budget is spent (after at least 32 events) and returns false: call again to go on. The history
   * doesn't depend on where it stopped.
   */
  advance(to: number, budgetMs = Infinity): boolean {
    const q = this.queue;
    const out = this.popped;
    const ev = this.ev;
    const timed = budgetMs < Infinity;
    const t0 = timed ? performance.now() : 0;
    let n = 0;
    while (q.size > 0 && q.peekTime() <= to) {
      if (timed && (++n & 31) === 0 && performance.now() - t0 >= budgetMs) return false;
      q.pop(out);
      this.now = out.time;
      const kind = this.kinds[out.kind];
      if (!kind.run) {
        this.stats.unhandled++;
        continue;
      }
      ev.kind = kind.name;
      ev.time = out.time;
      ev.target = out.target;
      ev.a = out.a;
      ev.b = out.b;
      ev.data = out.data;
      ev.cause = out.cause;
      ev.handle = out.handle;
      this.cause = out.cause;
      this.stats.events++;
      try {
        kind.run(this, ev);
      } catch (err) {
        this.stats.errors++;
        if (!this.onError) throw err;
        this.onError(err, ev);
      } finally {
        this.cause = 0;
      }
    }
    if (to > this.now) this.now = to;
    return true;
  }

  date(t = this.now): string {
    return this.calendar.format(t);
  }

  // --- memory -----------------------------------------------------------------------------------

  /**
   * Writes a record in the log (at `now`); returns its id. Unless `keep`, it becomes the current
   * cause: what this event does next descends from it.
   */
  record(kind: string, actor = 0, subject = 0, a = 0, o?: RecordOptions): number {
    const id = this.log.append(this.now, this.syms.id(kind), actor, subject, a, o?.cause ?? this.cause, o?.also, o?.data);
    if (!o?.keep) this.cause = id;
    return id;
  }

  /**
   * Something from outside the world (the player, the engine): a record with no cause, and — if a
   * module handles this kind — an event now for `subject`, caused by that record.
   */
  fact(kind: string, actor = 0, subject = 0, a = 0, data?: unknown): number {
    const prev = this.cause;
    const id = this.record(kind, actor, subject, a, { cause: 0, data });
    const k = this.kindIndex.get(kind);
    if (k !== undefined && this.kinds[k].run) this.schedule(this.now, kind, subject, a, 0, data);
    this.cause = prev;
    return id;
  }

  // --- the engine ---------------------------------------------------------------------------------

  /** Answers an engine's request with the module that handles it (the world is at `now`). */
  request(name: string, payload: unknown): unknown {
    const fn = this.handlers.get(name);
    if (!fn) throw new Error(`nadie atiende la petición ${name}`);
    const prev = this.cause;
    this.cause = 0;
    try {
      return fn(this, payload);
    } finally {
      this.cause = prev;
    }
  }

  /** Tells the engine something (see `outbox`). */
  emit(channel: string, data: unknown): void {
    this.outbox.push([channel, data]);
  }

  // --- randomness -------------------------------------------------------------------------------

  /** Next number in [0, 1) of the stream of `id` (0: the world's own). Its counter is saved with the world. */
  random(id = 0): number {
    const t = this.rng;
    let r = t.row(id);
    if (r < 0) r = t.add(id);
    const col = t.c.n;
    const n = col[r];
    col[r] = n + 1;
    return draw(hash3(this.seed, id, RNG_SALT), n) / 4294967296;
  }
}
