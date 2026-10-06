// The engine's side of the world thread (docs/MUNDO.md §7): send facts, ask questions, save, stop.
// Knows nothing of the transport: give it a `send`, feed it what arrives with `receive`.

import type { SaveInfo } from '../persist/saves.js';
import type { RecordView, SimIn, SimOut, SimQuery, SimStatus } from './protocol.js';

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

export class SimClient {
  /** Last status the world sent (null until it is ready). */
  status: SimStatus | null = null;
  /** Resolves when the world is loaded and running (rejects if it can't start). */
  readonly ready: Promise<SimStatus>;
  /** What the world noted while loading (fields that changed, tables kept without a module…). */
  notes: string[] = [];
  private settle!: Pending;
  private readonly pending = new Map<number, Pending>();
  private readonly listeners = new Set<(m: SimOut) => void>();
  private readonly channels = new Map<string, Set<(data: unknown) => void>>();
  private next = 1;
  private closed: Error | null = null;

  constructor(private readonly send: (m: SimIn) => void) {
    this.ready = new Promise<SimStatus>((resolve, reject) => (this.settle = { resolve: resolve as (v: unknown) => void, reject }));
    this.ready.catch(() => undefined);
  }

  receive(m: SimOut): void {
    if (m.type === 'ready' || m.type === 'status') this.status = m.status;
    if (m.type === 'ready') {
      this.notes = m.notes;
      this.settle.resolve(m.status);
    } else if (m.type === 'failed') this.fail(new Error(m.message));
    else if (m.type === 'answer') {
      const p = this.pending.get(m.id);
      this.pending.delete(m.id);
      if (m.ok) p?.resolve(m.result);
      else p?.reject(new Error(m.error));
    }
    else if (m.type === 'emit') for (const [ch, data] of m.list) for (const fn of this.channels.get(ch) ?? []) fn(data);
    for (const fn of this.listeners) fn(m);
  }

  /** The thread is gone: every pending request fails. */
  fail(e: Error): void {
    this.closed ??= e;
    this.settle.reject(e);
    for (const p of this.pending.values()) p.reject(e);
    this.pending.clear();
  }

  on(fn: (m: SimOut) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** What the world's modules say on a channel (World.emit). */
  onEmit(channel: string, fn: (data: unknown) => void): () => void {
    let s = this.channels.get(channel);
    if (!s) this.channels.set(channel, (s = new Set()));
    s.add(fn);
    return () => s.delete(fn);
  }

  /** A module's request (World.request): its answer. */
  ask<T = unknown>(name: string, payload?: unknown): Promise<T> {
    return this.request((id) => ({ type: 'ask', id, name, payload })) as Promise<T>;
  }

  /** Something that happened outside the world (see World.fact). */
  fact(kind: string, actor = 0, subject = 0, a = 0, data?: unknown): void {
    if (!this.closed) this.send({ type: 'fact', kind, actor, subject, a, data });
  }

  setScale(scale: number): void {
    if (!this.closed) this.send({ type: 'scale', scale });
  }

  query(q: { q: 'status' }): Promise<SimStatus>;
  query(q: { q: 'digest' }): Promise<string>;
  query(q: { q: 'record'; id: number }): Promise<RecordView | null>;
  query(q: SimQuery): Promise<RecordView[]>;
  query(q: SimQuery): Promise<unknown> {
    return this.request((id) => ({ type: 'query', id, q }));
  }

  save(): Promise<SaveInfo> {
    return this.request((id) => ({ type: 'save', id })) as Promise<SaveInfo>;
  }

  /** Stops the world and saves it. */
  stop(): Promise<SaveInfo | null> {
    return this.request((id) => ({ type: 'stop', id })) as Promise<SaveInfo | null>;
  }

  private request(make: (id: number) => SimIn): Promise<unknown> {
    if (this.closed) return Promise.reject(this.closed);
    const id = this.next++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.send(make(id));
    });
  }
}
