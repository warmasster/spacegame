// Messages between the engine and the thread that runs the world (docs/MUNDO.md §7). Plain
// structured-clone data: the same over a Node worker, a browser Worker or in-process.

import type { SaveInfo } from '../persist/saves.js';

export interface SimConfig {
  /** Seed of a new world (a loaded one keeps its own). */
  seed: number;
  /** Game seconds per real second (72: a game day in 20 real minutes). */
  scale?: number;
  /** Most CPU time per tick (ms); what doesn't fit waits for the next tick. */
  budgetMs?: number;
  /** Real time between ticks (ms). */
  tickMs?: number;
  /** Real seconds between automatic saves (0: only when asked and when stopping). */
  autosaveS?: number;
  /** Real seconds between status messages. */
  statusS?: number;
  /** Game seconds between the seismograph's samples of the modules' gauges. */
  scopeS?: number;
}

export interface SimStatus {
  /** Game time (s) and its date. */
  now: number;
  date: string;
  scale: number;
  /** How far behind its clock the world is (game s): > 0 while it catches up. */
  lag: number;
  entities: number;
  pending: number;
  records: number;
  events: number;
  errors: number;
  /** 95th percentile of the time a tick took (ms), against its budget. */
  tickP95: number;
  budgetMs: number;
  persistent: boolean;
  saves: number;
  lastSave: SaveInfo | null;
}

/** A log record as the engine and the tools see it. */
export interface RecordView {
  id: number;
  time: number;
  date: string;
  kind: string;
  actor: number;
  subject: number;
  a: number;
  cause: number;
  also: number[];
  data: unknown;
}

export type SimQuery =
  | { q: 'status' }
  | { q: 'digest' }
  /** Every module's gauges now. */
  | { q: 'gauges' }
  /** The seismograph's series (tools/seismograph.ts). */
  | { q: 'series' }
  | { q: 'record'; id: number }
  | { q: 'chain'; id: number; max?: number }
  | { q: 'about'; entity: number; limit?: number }
  | { q: 'recent'; limit?: number };

/** Engine → world. */
export type SimIn =
  | { type: 'fact'; kind: string; actor?: number; subject?: number; a?: number; data?: unknown }
  | { type: 'scale'; scale: number }
  | { type: 'query'; id: number; q: SimQuery }
  /** A module's request (World.request). */
  | { type: 'ask'; id: number; name: string; payload?: unknown }
  | { type: 'save'; id: number }
  | { type: 'stop'; id: number };

/** World → engine. */
export type SimOut =
  | { type: 'ready'; status: SimStatus; notes: string[] }
  | { type: 'status'; status: SimStatus }
  | { type: 'answer'; id: number; ok: boolean; result?: unknown; error?: string }
  | { type: 'error'; message: string }
  /** What modules told the engine (World.emit), in order. */
  | { type: 'emit'; list: Array<[string, unknown]> }
  | { type: 'failed'; message: string };

export interface SimPort {
  post(msg: SimOut): void;
  listen(fn: (msg: SimIn) => void): void;
}
