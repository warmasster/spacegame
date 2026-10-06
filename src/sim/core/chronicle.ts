// The world's memory (docs/MUNDO.md §5): an append-only log of what mattered — what happened, who
// did it, to whom, when, and why. Each record points at the record that caused it (`cause`, and
// `also` when there is more than one), so any fact can be followed back to its roots: the game's
// black box, the debugger, and later what archives and chroniclers know.
//
// Records live in fixed-size chunks of typed arrays. A full chunk never changes again: it is written
// to disk once (a segment) and a save only adds what is new.

export const DEFAULT_CHUNK = 16384;

/** A record as tools see it (the log itself keeps columns). */
export interface LogRecord {
  id: number;
  time: number;
  kind: string;
  actor: number;
  subject: number;
  a: number;
  cause: number;
  also: number[];
  data: unknown;
}

export class LogChunk {
  /** Records filled. */
  n = 0;
  readonly time: Float64Array;
  readonly kind: Uint32Array;
  readonly actor: Uint32Array;
  readonly subject: Uint32Array;
  readonly a: Float64Array;
  readonly cause: Float64Array;
  /** Extra causes and payloads, by position in the chunk (few records have them). */
  also: Map<number, number[]> | null = null;
  data: Map<number, unknown> | null = null;

  constructor(size: number) {
    this.time = new Float64Array(size);
    this.kind = new Uint32Array(size);
    this.actor = new Uint32Array(size);
    this.subject = new Uint32Array(size);
    this.a = new Float64Array(size);
    this.cause = new Float64Array(size);
  }
}

export class Chronicle {
  /** Records so far; ids are 1…count (0 is "no record"). */
  count = 0;
  /** Chunks already written as segments. */
  saved = 0;
  readonly chunks: LogChunk[] = [];

  constructor(readonly chunkSize = DEFAULT_CHUNK) {
    if (!(chunkSize >= 16) || (chunkSize & (chunkSize - 1)) !== 0) throw new Error('chunkSize: potencia de dos ≥ 16');
  }

  /** Full chunks (they never change again). */
  get full(): number {
    return Math.floor(this.count / this.chunkSize);
  }

  append(time: number, kind: number, actor: number, subject: number, a: number, cause: number, also?: readonly number[], data?: unknown): number {
    const i = this.count;
    const ci = Math.floor(i / this.chunkSize);
    const li = i - ci * this.chunkSize;
    let c = this.chunks[ci];
    if (!c) this.chunks.push((c = new LogChunk(this.chunkSize)));
    c.time[li] = time;
    c.kind[li] = kind;
    c.actor[li] = actor;
    c.subject[li] = subject;
    c.a[li] = a;
    c.cause[li] = cause;
    if (also && also.length > 0) (c.also ??= new Map()).set(li, [...also]);
    if (data !== undefined) (c.data ??= new Map()).set(li, data);
    c.n = li + 1;
    return ++this.count;
  }

  has(id: number): boolean {
    return id >= 1 && id <= this.count && Number.isInteger(id);
  }

  time(id: number): number {
    return this.at(id, 'time');
  }

  kind(id: number): number {
    return this.at(id, 'kind');
  }

  actor(id: number): number {
    return this.at(id, 'actor');
  }

  subject(id: number): number {
    return this.at(id, 'subject');
  }

  a(id: number): number {
    return this.at(id, 'a');
  }

  cause(id: number): number {
    return this.at(id, 'cause');
  }

  also(id: number): number[] {
    const [c, li] = this.loc(id);
    return c.also?.get(li) ?? [];
  }

  data(id: number): unknown {
    const [c, li] = this.loc(id);
    return c.data?.get(li);
  }

  /** Every cause of a record (the main one first). */
  causes(id: number): number[] {
    const main = this.cause(id);
    return main ? [main, ...this.also(id)] : this.also(id);
  }

  /** A record and its main causes back to the root: [id, cause, cause of cause, …]. */
  chain(id: number, max = 256): number[] {
    const out: number[] = [];
    while (this.has(id) && out.length < max) {
      out.push(id);
      id = this.cause(id);
    }
    return out;
  }

  /** Records caused by `id` (a scan forward from it). */
  effects(id: number, limit = 100): number[] {
    const out: number[] = [];
    for (let r = id + 1; r <= this.count && out.length < limit; r++) if (this.cause(r) === id || this.also(r).includes(id)) out.push(r);
    return out;
  }

  /** Records where `entity` is the actor or the subject, newest first. */
  about(entity: number, limit = 50, before = this.count + 1): number[] {
    const out: number[] = [];
    for (let r = Math.min(before - 1, this.count); r >= 1 && out.length < limit; r--) if (this.actor(r) === entity || this.subject(r) === entity) out.push(r);
    return out;
  }

  private loc(id: number): [LogChunk, number] {
    if (!this.has(id)) throw new Error(`no hay registro ${id}`);
    const i = id - 1;
    const ci = Math.floor(i / this.chunkSize);
    return [this.chunks[ci], i - ci * this.chunkSize];
  }

  private at(id: number, col: 'time' | 'kind' | 'actor' | 'subject' | 'a' | 'cause'): number {
    const [c, li] = this.loc(id);
    return c[col][li];
  }
}
