// Little binary writer/reader for saves (docs/MUNDO.md §6): scalars little-endian, columns copied
// as they are in memory (every platform we run on is little-endian; checked), a checksum at the end.

import { mix32 } from '../core/rng.js';
import type { Column } from '../core/store.js';

if (new Uint8Array(new Uint32Array([1]).buffer)[0] !== 1) throw new Error('las partidas guardadas necesitan una máquina little-endian');

const enc = new TextEncoder();
const dec = new TextDecoder();

/** A 32-bit hash of bytes (corruption check; test digests). */
export function hashBytes(b: Uint8Array, seed = 0): number {
  const n = b.length;
  const n4 = n & ~3;
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  let h = (seed ^ n) >>> 0;
  for (let i = 0; i < n4; i += 4) {
    h = Math.imul(h ^ dv.getUint32(i, true), 0x9e3779b1);
    h ^= h >>> 15;
  }
  for (let i = n4; i < n; i++) {
    h = Math.imul(h ^ b[i], 0x9e3779b1);
    h ^= h >>> 15;
  }
  return mix32(h);
}

export class Writer {
  private buf = new Uint8Array(1 << 16);
  private view = new DataView(this.buf.buffer);
  private len = 0;

  u32(v: number): void {
    this.need(4);
    this.view.setUint32(this.len, v, true);
    this.len += 4;
  }

  f64(v: number): void {
    this.need(8);
    this.view.setFloat64(this.len, v, true);
    this.len += 8;
  }

  /** Length-prefixed bytes. */
  bytes(b: Uint8Array): void {
    this.u32(b.length);
    this.need(b.length);
    this.buf.set(b, this.len);
    this.len += b.length;
  }

  json(v: unknown): void {
    this.bytes(enc.encode(JSON.stringify(v)));
  }

  /** A column's raw bytes (its length is known to the reader from the header). */
  column(a: Column): void {
    this.need(a.byteLength);
    this.buf.set(new Uint8Array(a.buffer, a.byteOffset, a.byteLength), this.len);
    this.len += a.byteLength;
  }

  /** Everything written, with its checksum. */
  finish(): Uint8Array {
    const sum = hashBytes(this.buf.subarray(0, this.len));
    this.u32(sum);
    return this.buf.slice(0, this.len);
  }

  private need(n: number): void {
    if (this.len + n <= this.buf.length) return;
    let size = this.buf.length * 2;
    while (size < this.len + n) size *= 2;
    const next = new Uint8Array(size);
    next.set(this.buf.subarray(0, this.len));
    this.buf = next;
    this.view = new DataView(next.buffer);
  }
}

export class Reader {
  private readonly view: DataView;
  private o = 0;
  private readonly end: number;

  /** Checks the checksum (throws if the bytes are damaged or cut). */
  constructor(private readonly b: Uint8Array) {
    if (b.length < 4) throw new Error('archivo vacío o cortado');
    this.view = new DataView(b.buffer, b.byteOffset, b.byteLength);
    this.end = b.length - 4;
    if (hashBytes(b.subarray(0, this.end)) !== this.view.getUint32(this.end, true)) throw new Error('archivo dañado (checksum)');
  }

  u32(): number {
    this.room(4);
    const v = this.view.getUint32(this.o, true);
    this.o += 4;
    return v;
  }

  f64(): number {
    this.room(8);
    const v = this.view.getFloat64(this.o, true);
    this.o += 8;
    return v;
  }

  bytes(): Uint8Array {
    const n = this.u32();
    this.room(n);
    const out = this.b.slice(this.o, this.o + n);
    this.o += n;
    return out;
  }

  json<T>(): T {
    return JSON.parse(dec.decode(this.bytes())) as T;
  }

  /** A column of `n` values (a copy: the reader's bytes need not be aligned). */
  column<T extends Column>(C: new (n: number) => T, n: number): T {
    const out = new C(n);
    const size = out.byteLength;
    this.room(size);
    new Uint8Array(out.buffer).set(this.b.subarray(this.o, this.o + size));
    this.o += size;
    return out;
  }

  private room(n: number): void {
    if (this.o + n > this.end) throw new Error('archivo cortado');
  }
}
