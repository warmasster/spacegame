// The world's timeline (docs/MUNDO.md §2): what will happen at a known time, in order. A binary
// heap of slot numbers over slots kept in typed arrays (no object per event, nothing for the GC);
// O(log n) to schedule, cancel, move or pop. The order is total and doesn't depend on how the heap
// is laid out: time, then the kind's order, then the target's id, then scheduling order — the same
// history on every run.
//
// A handle names one scheduled event (slot + generation, never 0): keep it in a table to cancel or
// move the event when what it was computed from changes ("si puedes calcular cuándo va a pasar algo,
// no compruebes si ha pasado"). Slots, generations and the free list are saved as they are, so
// handles survive a save and a load, and the world after a load is the world that was saved.

const SLOTS = 2 ** 26;
const GENS = 2 ** 26;

/** An event taken off the queue (filled in place). */
export interface Popped {
  time: number;
  kind: number;
  target: number;
  a: number;
  b: number;
  cause: number;
  data: unknown;
  handle: number;
}

/** A queue as saved: the slot arrays up to the high-water mark, the heap and the free list. */
export interface QueueState {
  hw: number;
  size: number;
  freeCount: number;
  seq: number;
  time: Float64Array;
  order: Uint16Array;
  kind: Uint32Array;
  target: Uint32Array;
  seqs: Float64Array;
  a: Float64Array;
  b: Float64Array;
  cause: Float64Array;
  gen: Uint32Array;
  pos: Int32Array;
  heap: Int32Array;
  free: Int32Array;
  /** Payloads by slot, ascending. */
  data: Array<[number, unknown]>;
}

export class EventQueue {
  /** Events pending. */
  size = 0;
  /** Scheduling counter: the last tie-break. */
  seq = 0;
  private hw = 0;
  private freeCount = 0;
  private cap = 0;
  private t: Float64Array = new Float64Array(0);
  private ord: Uint16Array = new Uint16Array(0);
  private k: Uint32Array = new Uint32Array(0);
  private tg: Uint32Array = new Uint32Array(0);
  private sq: Float64Array = new Float64Array(0);
  private pa: Float64Array = new Float64Array(0);
  private pb: Float64Array = new Float64Array(0);
  private cz: Float64Array = new Float64Array(0);
  private gen: Uint32Array = new Uint32Array(0);
  private pos: Int32Array = new Int32Array(0);
  private heap: Int32Array = new Int32Array(0);
  /** Time of each heap position (the sift compares these, next to each other, not through the slot). */
  private ht: Float64Array = new Float64Array(0);
  private free: Int32Array = new Int32Array(0);
  private readonly data = new Map<number, unknown>();

  constructor(capacity = 64) {
    this.grow(capacity);
  }

  /** Schedules an event; returns its handle. */
  push(time: number, order: number, kind: number, target: number, a: number, b: number, cause: number, data?: unknown): number {
    let s: number;
    if (this.freeCount > 0) s = this.free[--this.freeCount];
    else {
      if (this.hw >= SLOTS) throw new Error('cola de eventos llena');
      if (this.hw === this.cap) this.grow(this.cap * 2);
      s = this.hw++;
      this.gen[s] = 1;
    }
    this.t[s] = time;
    this.ord[s] = order;
    this.k[s] = kind;
    this.tg[s] = target;
    this.sq[s] = this.seq++;
    this.pa[s] = a;
    this.pb[s] = b;
    this.cz[s] = cause;
    if (data !== undefined) this.data.set(s, data);
    const i = this.size++;
    this.heap[i] = s;
    this.ht[i] = time;
    this.pos[s] = i;
    this.up(i);
    return this.gen[s] * SLOTS + s;
  }

  /** Time of the next event (Infinity if none). */
  peekTime(): number {
    return this.size > 0 ? this.ht[0] : Infinity;
  }

  /** Takes the next event into `out`; false if there is none. Its handle is dead afterwards. */
  pop(out: Popped): boolean {
    if (this.size === 0) return false;
    const s = this.heap[0];
    out.time = this.t[s];
    out.kind = this.k[s];
    out.target = this.tg[s];
    out.a = this.pa[s];
    out.b = this.pb[s];
    out.cause = this.cz[s];
    out.data = this.data.size > 0 ? this.data.get(s) : undefined;
    out.handle = this.gen[s] * SLOTS + s;
    this.removeAt(0);
    this.release(s);
    return true;
  }

  /** Is this event still pending? */
  valid(handle: number): boolean {
    return this.slot(handle) >= 0;
  }

  /** When a pending event will happen (NaN if it isn't pending). */
  timeOf(handle: number): number {
    const s = this.slot(handle);
    return s < 0 ? NaN : this.t[s];
  }

  cancel(handle: number): boolean {
    const s = this.slot(handle);
    if (s < 0) return false;
    this.removeAt(this.pos[s]);
    this.release(s);
    return true;
  }

  /** Moves a pending event to another time (and a new cause: the reason it moved). */
  move(handle: number, time: number, cause: number): boolean {
    const s = this.slot(handle);
    if (s < 0) return false;
    const old = this.t[s];
    this.t[s] = time;
    this.cz[s] = cause;
    const i = this.pos[s];
    this.ht[i] = time;
    if (time < old) this.up(i);
    else this.down(i);
    return true;
  }

  state(): QueueState {
    const hw = this.hw;
    return {
      hw,
      size: this.size,
      freeCount: this.freeCount,
      seq: this.seq,
      time: this.t.subarray(0, hw),
      order: this.ord.subarray(0, hw),
      kind: this.k.subarray(0, hw),
      target: this.tg.subarray(0, hw),
      seqs: this.sq.subarray(0, hw),
      a: this.pa.subarray(0, hw),
      b: this.pb.subarray(0, hw),
      cause: this.cz.subarray(0, hw),
      gen: this.gen.subarray(0, hw),
      pos: this.pos.subarray(0, hw),
      heap: this.heap.subarray(0, this.size),
      free: this.free.subarray(0, this.freeCount),
      data: [...this.data].sort((x, y) => x[0] - y[0]),
    };
  }

  load(s: QueueState): void {
    this.cap = 0;
    this.grow(Math.max(64, 2 ** Math.ceil(Math.log2(Math.max(1, s.hw)))));
    this.hw = s.hw;
    this.size = s.size;
    this.freeCount = s.freeCount;
    this.seq = s.seq;
    this.t.set(s.time);
    this.ord.set(s.order);
    this.k.set(s.kind);
    this.tg.set(s.target);
    this.sq.set(s.seqs);
    this.pa.set(s.a);
    this.pb.set(s.b);
    this.cz.set(s.cause);
    this.gen.set(s.gen);
    this.pos.set(s.pos);
    this.heap.set(s.heap);
    this.free.set(s.free);
    this.data.clear();
    for (const [slot, d] of s.data) this.data.set(slot, d);
    for (let i = 0; i < s.size; i++) this.ht[i] = this.t[this.heap[i]];
  }

  /** Renames kinds (after a load: saved kind index → this run's index). */
  remapKinds(map: ArrayLike<number>): void {
    for (let s = 0; s < this.hw; s++) if (this.pos[s] >= 0) this.k[s] = map[this.k[s]];
  }

  private slot(handle: number): number {
    if (!(handle > 0)) return -1;
    const s = handle % SLOTS;
    const g = (handle - s) / SLOTS;
    return s < this.hw && this.gen[s] === g && this.pos[s] >= 0 ? s : -1;
  }

  private release(s: number): void {
    this.pos[s] = -1;
    this.gen[s] = this.gen[s] + 1 >= GENS ? 1 : this.gen[s] + 1;
    if (this.data.size > 0) this.data.delete(s);
    this.free[this.freeCount++] = s;
  }

  /** Same time: x before y? */
  private tie(x: number, y: number): boolean {
    const ox = this.ord[x];
    const oy = this.ord[y];
    if (ox !== oy) return ox < oy;
    const gx = this.tg[x];
    const gy = this.tg[y];
    if (gx !== gy) return gx < gy;
    return this.sq[x] < this.sq[y];
  }

  private up(i: number): void {
    const h = this.heap;
    const ht = this.ht;
    const p = this.pos;
    const s = h[i];
    const ts = ht[i];
    while (i > 0) {
      const pi = (i - 1) >> 1;
      const ps = h[pi];
      const tp = ht[pi];
      if (ts > tp || (ts === tp && !this.tie(s, ps))) break;
      h[i] = ps;
      ht[i] = tp;
      p[ps] = i;
      i = pi;
    }
    h[i] = s;
    ht[i] = ts;
    p[s] = i;
  }

  private down(i: number): void {
    const h = this.heap;
    const ht = this.ht;
    const p = this.pos;
    const n = this.size;
    const s = h[i];
    const ts = ht[i];
    for (;;) {
      let c = 2 * i + 1;
      if (c >= n) break;
      let cs = h[c];
      let tc = ht[c];
      if (c + 1 < n) {
        const td = ht[c + 1];
        if (td < tc || (td === tc && this.tie(h[c + 1], cs))) {
          c++;
          cs = h[c];
          tc = td;
        }
      }
      if (tc > ts || (tc === ts && !this.tie(cs, s))) break;
      h[i] = cs;
      ht[i] = tc;
      p[cs] = i;
      i = c;
    }
    h[i] = s;
    ht[i] = ts;
    p[s] = i;
  }

  private removeAt(i: number): void {
    const last = --this.size;
    if (i === last) return;
    const s = this.heap[last];
    const ts = this.ht[last];
    this.heap[i] = s;
    this.ht[i] = ts;
    this.pos[s] = i;
    if (i > 0) {
      const pi = (i - 1) >> 1;
      const tp = this.ht[pi];
      if (ts < tp || (ts === tp && this.tie(s, this.heap[pi]))) return this.up(i);
    }
    this.down(i);
  }

  private grow(n: number): void {
    const old = this.cap;
    this.cap = n;
    const f64 = (a: Float64Array) => copy(new Float64Array(n), a, old);
    this.t = f64(this.t);
    this.sq = f64(this.sq);
    this.pa = f64(this.pa);
    this.pb = f64(this.pb);
    this.cz = f64(this.cz);
    this.ord = copy(new Uint16Array(n), this.ord, old);
    this.k = copy(new Uint32Array(n), this.k, old);
    this.tg = copy(new Uint32Array(n), this.tg, old);
    this.gen = copy(new Uint32Array(n), this.gen, old);
    this.pos = copy(new Int32Array(n), this.pos, old);
    this.heap = copy(new Int32Array(n), this.heap, old);
    this.ht = f64(this.ht);
    this.free = copy(new Int32Array(n), this.free, old);
  }
}

type Slots = Float64Array | Uint16Array | Uint32Array | Int32Array;

function copy<T extends Slots>(to: T, from: Slots, n: number): T {
  if (n > 0) to.set(from.subarray(0, Math.min(n, from.length)));
  return to;
}
