// A world to bytes and back (docs/MUNDO.md §6). Everything that decides the future is saved as it
// is — tables, the queue's slots (handles stay valid), counters, the symbol and kind tables, the
// log — so a loaded world goes on exactly as the saved one would have.
//
//   main:     'SIMW' · format · header (JSON: schema and counters) · table columns · queue slots ·
//             the log's last, partial chunk · checksum
//   segments: one per full log chunk ('SIML' · index · columns · extras · checksum), written once.
//
// Loading matches tables and fields by name: fields added since get their default, removed ones are
// dropped, a table no module declares any more is kept as it is (and saved again), event kinds are
// renamed to this run's numbers. A save from a newer format is refused.

import type { CalendarDef } from '../core/calendar.js';
import { LogChunk, type Chronicle } from '../core/chronicle.js';
import type { QueueState } from '../core/queue.js';
import { mix32 } from '../core/rng.js';
import { COLUMN, type Column, type FieldType, type TableData } from '../core/store.js';
import { World, type SimModule } from '../core/world.js';
import { hashBytes, Reader, Writer } from './binary.js';

const WORLD_MAGIC = 0x5753494d; // "MISW"
const LOG_MAGIC = 0x4c53494d; // "MISL"
export const FORMAT = 1;

export interface Header {
  format: number;
  /** Save generation (which of the two save slots is newer). */
  gen: number;
  seed: number;
  now: number;
  calendar: CalendarDef;
  nextId: number;
  chunkSize: number;
  records: number;
  /** Full log chunks: segments 0…segments-1 belong to this save. */
  segments: number;
  syms: string[];
  kinds: string[];
  tables: Array<{ name: string; fields: Array<[string, FieldType]>; count: number }>;
  queue: { hw: number; size: number; freeCount: number; seq: number; data: Array<[number, unknown]> };
}

export interface Snapshot {
  header: Header;
  main: Uint8Array;
  /** Log segments not saved yet (all of them with `all`). */
  segments: Array<{ index: number; bytes: Uint8Array }>;
}

export interface RestoreReport {
  /** Fields that didn't match the save exactly. */
  notes: string[];
  /** Saved tables no module declares (kept). */
  orphans: string[];
  /** Pending events of kinds no module handles. */
  unknownKinds: string[];
}

const byName = (a: { name: string }, b: { name: string }) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);

export function snapshot(w: World, o: { gen?: number; all?: boolean } = {}): Snapshot {
  const tables: TableData[] = [...w.store.tables().map((t) => t.data()), ...w.orphans.values()].sort(byName);
  const q = w.queue.state();
  const log = w.log;
  const full = log.full;
  const header: Header = {
    format: FORMAT,
    gen: o.gen ?? 0,
    seed: w.seed,
    now: w.now,
    calendar: w.calendar.def,
    nextId: w.store.nextId,
    chunkSize: log.chunkSize,
    records: log.count,
    segments: full,
    syms: [...w.syms.list],
    kinds: w.kindNames(),
    tables: tables.map((t) => ({ name: t.name, fields: t.fields.map(([k, ty]) => [k, ty] as [string, FieldType]), count: t.count })),
    queue: { hw: q.hw, size: q.size, freeCount: q.freeCount, seq: q.seq, data: q.data },
  };
  const out = new Writer();
  out.u32(WORLD_MAGIC);
  out.u32(FORMAT);
  out.json(header);
  for (const t of tables) {
    out.column(t.ids);
    for (const c of t.cols) out.column(c);
  }
  for (const c of [q.time, q.order, q.kind, q.target, q.seqs, q.a, q.b, q.cause, q.gen, q.pos, q.heap, q.free]) out.column(c);
  out.bytes(encodeChunk(log, full));
  const segments: Snapshot['segments'] = [];
  for (let i = o.all ? 0 : log.saved; i < full; i++) segments.push({ index: i, bytes: encodeChunk(log, i) });
  return { header, main: out.finish(), segments };
}

/** The header of a save (checks it is whole and not from a newer format). */
export function readHeader(main: Uint8Array): Header {
  const r = new Reader(main);
  return header(r);
}

/** A saved world, with `modules` (saved things no module knows are kept, see RestoreReport). */
export function restore(main: Uint8Array, segments: readonly Uint8Array[], modules: readonly SimModule[]): { world: World; report: RestoreReport } {
  const r = new Reader(main);
  const h = header(r);
  if (segments.length !== h.segments) throw new Error(`faltan segmentos del registro (${segments.length}/${h.segments})`);
  const w = new World({ seed: h.seed, modules, calendar: h.calendar, start: h.now, chunkSize: h.chunkSize });
  const report: RestoreReport = { notes: [], orphans: [], unknownKinds: [] };
  w.syms.load(h.syms);
  w.store.nextId = h.nextId;

  for (const t of h.tables) {
    const ids = r.column(Uint32Array, t.count);
    const cols = t.fields.map(([k, ty]) => {
      const C = COLUMN[ty];
      if (!C) throw new Error(`${t.name}.${k}: tipo desconocido ${ty}`);
      return r.column(C as new (n: number) => Column, t.count);
    });
    const data: TableData = { name: t.name, fields: t.fields, count: t.count, ids, cols };
    const table = w.store.get(t.name);
    if (table) report.notes.push(...table.load(data));
    else {
      w.orphans.set(t.name, data);
      report.orphans.push(t.name);
    }
  }

  const q = h.queue;
  const state: QueueState = {
    hw: q.hw,
    size: q.size,
    freeCount: q.freeCount,
    seq: q.seq,
    time: r.column(Float64Array, q.hw),
    order: r.column(Uint16Array, q.hw),
    kind: r.column(Uint32Array, q.hw),
    target: r.column(Uint32Array, q.hw),
    seqs: r.column(Float64Array, q.hw),
    a: r.column(Float64Array, q.hw),
    b: r.column(Float64Array, q.hw),
    cause: r.column(Float64Array, q.hw),
    gen: r.column(Uint32Array, q.hw),
    pos: r.column(Int32Array, q.hw),
    heap: r.column(Int32Array, q.size),
    free: r.column(Int32Array, q.freeCount),
    data: q.data,
  };
  // every saved kind, in its saved order (a kind nobody registered yet keeps its place)
  const known = new Set(w.kindNames());
  const map = h.kinds.map((name) => w.kindOf(name));
  const waiting = new Set<number>();
  for (let s = 0; s < q.hw; s++) if (state.pos[s] >= 0) waiting.add(state.kind[s]);
  for (const k of waiting) if (!known.has(h.kinds[k])) report.unknownKinds.push(h.kinds[k]);
  w.queue.load(state);
  w.queue.remapKinds(map);

  const log = w.log;
  for (let i = 0; i < h.segments; i++) log.chunks.push(decodeChunk(segments[i], h.chunkSize, i));
  const tail = decodeChunk(r.bytes(), h.chunkSize, h.segments);
  if (tail.n > 0) log.chunks.push(tail);
  if (h.segments * h.chunkSize + tail.n !== h.records) throw new Error('registro incompleto');
  log.count = h.records;
  log.saved = h.segments;

  for (const m of modules) m.loaded?.(w);
  return { world: w, report };
}

/** A fingerprint of everything a world is (two worlds with the same digest have the same future). */
export function digest(w: World): string {
  const s = snapshot(w, { all: true });
  let a = hashBytes(s.main, 1);
  let b = hashBytes(s.main, 2);
  for (const seg of s.segments) {
    a = mix32(a ^ hashBytes(seg.bytes, 1));
    b = mix32(b ^ hashBytes(seg.bytes, 2));
  }
  return a.toString(16).padStart(8, '0') + b.toString(16).padStart(8, '0');
}

function header(r: Reader): Header {
  if (r.u32() !== WORLD_MAGIC) throw new Error('no es una partida del mundo');
  const format = r.u32();
  if (format > FORMAT) throw new Error(`partida de un formato más nuevo (${format} > ${FORMAT})`);
  return r.json<Header>();
}

function encodeChunk(log: Chronicle, index: number): Uint8Array {
  const c = log.chunks[index];
  const n = c ? c.n : 0;
  const out = new Writer();
  out.u32(LOG_MAGIC);
  out.u32(FORMAT);
  out.f64(index);
  out.u32(n);
  if (c) for (const col of [c.time, c.kind, c.actor, c.subject, c.a, c.cause]) out.column(col.subarray(0, n));
  out.json({ also: c?.also ? [...c.also] : [], data: c?.data ? [...c.data] : [] });
  return out.finish();
}

function decodeChunk(bytes: Uint8Array, size: number, index: number): LogChunk {
  const r = new Reader(bytes);
  if (r.u32() !== LOG_MAGIC) throw new Error(`segmento ${index}: no es un registro`);
  if (r.u32() > FORMAT) throw new Error(`segmento ${index}: formato más nuevo`);
  if (r.f64() !== index) throw new Error(`segmento ${index}: es de otro sitio`);
  const n = r.u32();
  if (n > size) throw new Error(`segmento ${index}: demasiados registros`);
  const c = new LogChunk(size);
  c.time.set(r.column(Float64Array, n));
  c.kind.set(r.column(Uint32Array, n));
  c.actor.set(r.column(Uint32Array, n));
  c.subject.set(r.column(Uint32Array, n));
  c.a.set(r.column(Float64Array, n));
  c.cause.set(r.column(Float64Array, n));
  c.n = n;
  const x = r.json<{ also: Array<[number, number[]]>; data: Array<[number, unknown]> }>();
  if (x.also.length > 0) c.also = new Map(x.also);
  if (x.data.length > 0) c.data = new Map(x.data);
  return c;
}
