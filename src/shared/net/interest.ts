// Network interest (docs/RED.md): who has to know about what. Anything replicated (loose objects,
// NPCs…) is known by a watcher (a player) while it is near — within `enter`, and let go beyond
// `leave` (hysteresis: nothing flickers in and out at the edge) — or in the same host as it (a ship's
// contents, whatever the distance), or pinned to it (what it simulates). Coming into interest the
// watcher gets it whole (spawn); leaving, it forgets it (gone); in between, only its knowers hear
// its news. A grid over world positions keeps each update proportional to what is near, not to
// everything that exists.
//
// Pure: the server wires it to sockets (server/objects.ts, server/npcs.ts), tests to arrays.

import type { Vec3 } from '../protocol.js';

/** Buckets of ids by cell of a uniform grid over world positions (m). */
export class CellGrid {
  private readonly cells = new Map<number, Set<number>>();
  private readonly keyOf = new Map<number, number>();

  constructor(readonly cell: number) {}

  set(id: number, p: readonly number[]): void {
    const k = this.key(p[0], p[1], p[2]);
    const old = this.keyOf.get(id);
    if (old === k) return;
    if (old !== undefined) this.out(id, old);
    this.keyOf.set(id, k);
    let s = this.cells.get(k);
    if (!s) this.cells.set(k, (s = new Set()));
    s.add(id);
  }

  delete(id: number): void {
    const old = this.keyOf.get(id);
    if (old === undefined) return;
    this.out(id, old);
    this.keyOf.delete(id);
  }

  /** Ids in the cells that overlap the cube of half-size `r` round `p` (a superset of those within r). */
  near(p: readonly number[], r: number, out: number[] = []): number[] {
    const c = this.cell;
    const [x0, x1] = [Math.floor((p[0] - r) / c), Math.floor((p[0] + r) / c)];
    const [y0, y1] = [Math.floor((p[1] - r) / c), Math.floor((p[1] + r) / c)];
    const [z0, z1] = [Math.floor((p[2] - r) / c), Math.floor((p[2] + r) / c)];
    for (let x = x0; x <= x1; x++)
      for (let y = y0; y <= y1; y++)
        for (let z = z0; z <= z1; z++) {
          const s = this.cells.get(pack(x, y, z));
          if (s) for (const id of s) out.push(id);
        }
    return out;
  }

  private key(x: number, y: number, z: number): number {
    return pack(Math.floor(x / this.cell), Math.floor(y / this.cell), Math.floor(z / this.cell));
  }

  private out(id: number, k: number): void {
    const s = this.cells.get(k);
    if (!s) return;
    s.delete(id);
    if (s.size === 0) this.cells.delete(k);
  }
}

// cell coordinates packed in one exact number (±65 536 cells a side; beyond, the edge cells merge)
const OFF = 65536;
const SPAN = 131072;
const clampCell = (n: number) => (n < -OFF ? -OFF : n >= OFF ? OFF - 1 : n);
const pack = (x: number, y: number, z: number) => ((clampCell(x) + OFF) * SPAN + (clampCell(y) + OFF)) * SPAN + (clampCell(z) + OFF);

/** A watcher: where it is (world; null: nowhere yet) and the host it is in (0: none). */
export interface Watcher {
  readonly at: Vec3 | null;
  readonly fr: number;
}

export interface InterestRule {
  /** Comes into interest nearer than this (m)… */
  enter: number;
  /** …and leaves it beyond this (m). */
  leave: number;
}

export interface InterestChange<E> {
  enter: E[];
  leave: number[];
}

export interface ReplicationKind<E, W> {
  /** World position of a thing (into `out`; null: nowhere to be seen). */
  where(e: E, out: Vec3): Vec3 | null;
  /** The host it is in (0: none). */
  frame(e: E): number;
  /** Known by this watcher whatever the distance (what it simulates…). */
  pinned?(w: W, e: E): boolean;
}

/** The things of one kind and who knows each of them. */
export class Replication<E extends { readonly id: number }, W extends Watcher = Watcher> {
  private readonly all = new Map<number, E>();
  private readonly grid: CellGrid;
  private readonly byFrame = new Map<number, Set<number>>();
  private readonly frameOf = new Map<number, number>();
  private readonly known = new Map<W, Set<number>>();
  private readonly tmp: Vec3 = [0, 0, 0];
  private readonly ids: number[] = [];

  constructor(
    private readonly kind: ReplicationKind<E, W>,
    readonly rule: InterestRule,
  ) {
    if (!(rule.leave >= rule.enter)) throw new Error('interés: leave < enter');
    this.grid = new CellGrid(rule.leave);
  }

  get size(): number {
    return this.all.size;
  }

  get(id: number): E | undefined {
    return this.all.get(id);
  }

  values(): IterableIterator<E> {
    return this.all.values();
  }

  /** Things in a host. */
  *inFrame(fr: number): Generator<E> {
    const s = this.byFrame.get(fr);
    if (s) for (const id of s) yield this.all.get(id)!;
  }

  /** A new thing: returns the watchers that know it right away (send them its spawn). */
  add(e: E, watchers: Iterable<W>): W[] {
    this.all.set(e.id, e);
    this.place(e);
    const out: W[] = [];
    for (const w of watchers) {
      if (!this.visible(w, e, false)) continue;
      this.knownBy(w).add(e.id);
      out.push(w);
    }
    return out;
  }

  /** It is gone: returns the watchers that knew it (tell them). */
  remove(id: number): W[] {
    if (!this.all.delete(id)) return [];
    this.grid.delete(id);
    const fr = this.frameOf.get(id);
    if (fr !== undefined) this.byFrame.get(fr)?.delete(id);
    this.frameOf.delete(id);
    const out: W[] = [];
    for (const [w, s] of this.known) if (s.delete(id)) out.push(w);
    return out;
  }

  /** It moved or changed host (its host and cell are kept up to date for queries between updates). */
  moved(e: E): void {
    this.place(e);
  }

  knows(w: W, id: number): boolean {
    return this.known.get(w)?.has(id) ?? false;
  }

  /** Watchers that know a thing (its news go to them). */
  watchers(id: number): W[] {
    const out: W[] = [];
    for (const [w, s] of this.known) if (s.has(id)) out.push(w);
    return out;
  }

  /** A watcher goes away. */
  drop(w: W): void {
    this.known.delete(w);
  }

  /**
   * Interest now: every thing re-placed (hosts move their contents), then for each watcher what it
   * must learn and what it may forget. It is what `known` becomes: send the changes.
   */
  update(watchers: Iterable<W>): Map<W, InterestChange<E>> {
    for (const e of this.all.values()) this.place(e);
    const out = new Map<W, InterestChange<E>>();
    for (const w of watchers) {
      const known = this.knownBy(w);
      const change: InterestChange<E> = { enter: [], leave: [] };
      const ids = this.ids;
      ids.length = 0;
      if (w.at) this.grid.near(w.at, this.rule.leave, ids);
      if (w.fr !== 0) {
        const s = this.byFrame.get(w.fr);
        if (s) for (const id of s) ids.push(id);
      }
      for (const id of ids) {
        if (known.has(id)) continue;
        const e = this.all.get(id)!;
        if (this.visible(w, e, false)) {
          known.add(id);
          change.enter.push(e);
        }
      }
      for (const id of known) {
        const e = this.all.get(id);
        if (e && this.visible(w, e, true)) continue;
        known.delete(id);
        if (e) change.leave.push(id);
      }
      // what it was pinned to but never saw
      if (this.kind.pinned) {
        for (const e of this.all.values()) {
          if (known.has(e.id) || !this.kind.pinned(w, e)) continue;
          known.add(e.id);
          change.enter.push(e);
        }
      }
      if (change.enter.length || change.leave.length) out.set(w, change);
    }
    return out;
  }

  private visible(w: W, e: E, knownAlready: boolean): boolean {
    const fr = this.kind.frame(e);
    if (fr !== 0 && fr === w.fr) return true;
    if (this.kind.pinned?.(w, e)) return true;
    if (!w.at) return false;
    const p = this.kind.where(e, this.tmp);
    if (!p) return false;
    const r = knownAlready ? this.rule.leave : this.rule.enter;
    const dx = p[0] - w.at[0];
    const dy = p[1] - w.at[1];
    const dz = p[2] - w.at[2];
    return dx * dx + dy * dy + dz * dz < r * r;
  }

  private place(e: E): void {
    const fr = this.kind.frame(e);
    const was = this.frameOf.get(e.id);
    if (was !== fr) {
      if (was !== undefined) this.byFrame.get(was)?.delete(e.id);
      let s = this.byFrame.get(fr);
      if (!s) this.byFrame.set(fr, (s = new Set()));
      s.add(e.id);
      this.frameOf.set(e.id, fr);
    }
    const p = this.kind.where(e, this.tmp);
    if (p) this.grid.set(e.id, p);
    else this.grid.delete(e.id);
  }

  private knownBy(w: W): Set<number> {
    let s = this.known.get(w);
    if (!s) this.known.set(w, (s = new Set()));
    return s;
  }
}
