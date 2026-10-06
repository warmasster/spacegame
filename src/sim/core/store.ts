// The world's data (docs/MUNDO.md §4): entities are ids, components are tables of typed-array
// columns (structure of arrays: 50,000 loyalties are one Float32Array walked in order, not 50,000
// objects). One table per component, dense rows with swap-remove, a paged sparse index from id to
// row. Declared as data (`defineComponent`), saved and loaded by field name, so adding, removing or
// retyping a field doesn't break an old save.
//
// Ids are never reused: the log talks about the dead too.

export type FieldType = 'f64' | 'f32' | 'i32' | 'u32' | 'u16' | 'u8' | 'i8' | 'bool' | 'ref' | 'sym' | 'time';
export type FieldSpec = FieldType | { readonly type: FieldType; readonly default?: number };
export type Fields = Readonly<Record<string, FieldSpec>>;
export type Column = Float64Array | Float32Array | Int32Array | Uint32Array | Uint16Array | Uint8Array | Int8Array;

export interface ComponentDef<F extends Fields = Fields> {
  readonly name: string;
  readonly fields: F;
}

/** Column type of each field type (`ref`: an entity id, `sym`: an interned string, `time`: game seconds). */
export const COLUMN = {
  f64: Float64Array,
  time: Float64Array,
  f32: Float32Array,
  i32: Int32Array,
  u32: Uint32Array,
  ref: Uint32Array,
  sym: Uint32Array,
  u16: Uint16Array,
  u8: Uint8Array,
  bool: Uint8Array,
  i8: Int8Array,
} as const satisfies Record<FieldType, new (n: number) => Column>;

export const typeOf = (s: FieldSpec): FieldType => (typeof s === 'string' ? s : s.type);
const defaultOf = (s: FieldSpec): number => (typeof s === 'string' ? 0 : (s.default ?? 0));

export function defineComponent<F extends Fields>(name: string, fields: F): ComponentDef<F> {
  for (const k of Object.keys(fields)) if (!(typeOf(fields[k]) in COLUMN)) throw new Error(`${name}.${k}: tipo desconocido`);
  return Object.freeze({ name, fields });
}

/** A table's rows as saved: its schema, ids and one column per field (the first `count` values). */
export interface TableData {
  readonly name: string;
  readonly fields: ReadonlyArray<readonly [string, FieldType]>;
  readonly count: number;
  readonly ids: Uint32Array;
  readonly cols: readonly Column[];
}

// id → row. Ids only grow, so a flat array indexed by id would grow forever: pages of 1024 ids exist
// while some id in them is in the table. Memory follows the live ids; a lookup is two loads.
const PAGE_BITS = 10;
const PAGE = 1 << PAGE_BITS;
const PAGE_MASK = PAGE - 1;

export class SparseIndex {
  private pages: Array<Int32Array | undefined> = [];
  private live: number[] = [];

  get(id: number): number {
    const p = this.pages[id >>> PAGE_BITS];
    return p === undefined ? -1 : p[id & PAGE_MASK];
  }

  set(id: number, row: number): void {
    const i = id >>> PAGE_BITS;
    let p = this.pages[i];
    if (p === undefined) {
      p = new Int32Array(PAGE).fill(-1);
      this.pages[i] = p;
      this.live[i] = 0;
    }
    if (p[id & PAGE_MASK] < 0) this.live[i]++;
    p[id & PAGE_MASK] = row;
  }

  delete(id: number): void {
    const i = id >>> PAGE_BITS;
    const p = this.pages[i];
    if (p === undefined || p[id & PAGE_MASK] < 0) return;
    p[id & PAGE_MASK] = -1;
    if (--this.live[i] === 0) this.pages[i] = undefined;
  }
}

export class Table<F extends Fields = Fields> {
  readonly name: string;
  readonly keys: ReadonlyArray<keyof F & string>;
  readonly types: readonly FieldType[];
  count = 0;
  /** Id of each row. */
  ids: Uint32Array;
  /**
   * Columns by field, dense by row (`row(id)`). Replaced when the table grows: read them after the
   * last `add` of a loop, not before.
   */
  c: { [K in keyof F]: Column };
  private cols: Column[];
  private readonly defaults: readonly number[];
  private readonly at = new Map<string, number>();
  private readonly index = new SparseIndex();

  constructor(
    readonly def: ComponentDef<F>,
    capacity = 16,
  ) {
    this.name = def.name;
    this.keys = Object.keys(def.fields) as Array<keyof F & string>;
    this.types = this.keys.map((k) => typeOf(def.fields[k]));
    this.defaults = this.keys.map((k) => defaultOf(def.fields[k]));
    this.keys.forEach((k, i) => this.at.set(k, i));
    this.ids = new Uint32Array(capacity);
    this.cols = this.types.map((t) => new COLUMN[t](capacity));
    this.c = this.byName();
  }

  has(id: number): boolean {
    return this.index.get(id) >= 0;
  }

  /** Row of `id`, -1 if it isn't in the table. */
  row(id: number): number {
    return this.index.get(id);
  }

  /** Adds `id` (defaults, then `init`); if it is already here, only applies `init`. Returns its row. */
  add(id: number, init?: Partial<Record<keyof F & string, number>>): number {
    let r = this.index.get(id);
    if (r < 0) {
      if (this.count === this.ids.length) this.grow(this.count * 2);
      r = this.count++;
      this.ids[r] = id;
      this.index.set(id, r);
      for (let k = 0; k < this.cols.length; k++) this.cols[k][r] = this.defaults[k];
    }
    if (init) {
      for (const key in init) {
        const v = init[key];
        if (v !== undefined) this.column(key)[r] = v;
      }
    }
    return r;
  }

  /** Removes `id` (the last row takes its place). */
  remove(id: number): boolean {
    const r = this.index.get(id);
    if (r < 0) return false;
    const last = --this.count;
    if (r !== last) {
      const moved = this.ids[last];
      this.ids[r] = moved;
      for (const col of this.cols) col[r] = col[last];
      this.index.set(moved, r);
    }
    this.index.delete(id);
    return true;
  }

  /** Value of a field (NaN if `id` isn't in the table). */
  get(id: number, key: keyof F & string): number {
    const r = this.index.get(id);
    return r < 0 ? NaN : this.column(key)[r];
  }

  /** Sets a field; false if `id` isn't in the table. */
  set(id: number, key: keyof F & string, v: number): boolean {
    const r = this.index.get(id);
    if (r < 0) return false;
    this.column(key)[r] = v;
    return true;
  }

  /** Column of a field by name (for code that only knows the name). */
  column(key: string): Column {
    const i = this.at.get(key);
    if (i === undefined) throw new Error(`${this.name}: no hay campo ${key}`);
    return this.cols[i];
  }

  /** The rows as saved (views: copy them if they must outlive the next change). */
  data(): TableData {
    return {
      name: this.name,
      fields: this.keys.map((k, i) => [k, this.types[i]] as const),
      count: this.count,
      ids: this.ids.subarray(0, this.count),
      cols: this.cols.map((c) => c.subarray(0, this.count)),
    };
  }

  /**
   * Replaces the rows with saved ones, matching fields by name: a field the save doesn't have gets
   * its default, one this table doesn't have is dropped, a different type is converted. Returns what
   * didn't match exactly.
   */
  load(d: TableData): string[] {
    const notes: string[] = [];
    for (let r = 0; r < this.count; r++) this.index.delete(this.ids[r]);
    this.count = 0;
    if (this.ids.length < d.count) this.grow(Math.max(16, d.count));
    this.ids.set(d.ids.subarray(0, d.count));
    const saved = new Map(d.fields.map(([k, t], i) => [k, { t, col: d.cols[i] }]));
    this.keys.forEach((k, i) => {
      const s = saved.get(k);
      if (!s) {
        this.cols[i].fill(this.defaults[i], 0, d.count);
        notes.push(`${this.name}.${k}: nuevo (valor por defecto)`);
        return;
      }
      if (s.t !== this.types[i]) notes.push(`${this.name}.${k}: ${s.t} → ${this.types[i]}`);
      this.cols[i].set(s.col.subarray(0, d.count));
    });
    for (const [k] of saved) if (!this.at.has(k)) notes.push(`${this.name}.${k}: ya no existe (descartado)`);
    this.count = d.count;
    for (let r = 0; r < d.count; r++) this.index.set(this.ids[r], r);
    return notes;
  }

  private grow(n: number): void {
    const ids = new Uint32Array(n);
    ids.set(this.ids.subarray(0, this.count));
    this.ids = ids;
    this.cols = this.cols.map((c, i) => {
      const next = new COLUMN[this.types[i]](n);
      next.set(c.subarray(0, this.count));
      return next;
    });
    this.c = this.byName();
  }

  private byName(): { [K in keyof F]: Column } {
    const c: Record<string, Column> = {};
    this.keys.forEach((k, i) => (c[k] = this.cols[i]));
    return c as { [K in keyof F]: Column };
  }
}

const ENTITY = defineComponent('$entity', {});

export class Store {
  /** Next id to give (saved: ids are never reused). */
  nextId = 1;
  /** Every live entity. */
  readonly entities: Table<Record<string, never>>;
  private readonly byName = new Map<string, Table>();

  constructor() {
    this.entities = this.table(ENTITY);
  }

  has(name: string): boolean {
    return this.byName.has(name);
  }

  /** A registered table by name. */
  get(name: string): Table | undefined {
    return this.byName.get(name);
  }

  /** The table of a component (created on first use). */
  table<F extends Fields>(def: ComponentDef<F>): Table<F> {
    const t = this.byName.get(def.name);
    if (t) {
      if (t.def !== (def as ComponentDef) && !sameFields(t.def, def)) throw new Error(`${def.name}: dos componentes con el mismo nombre`);
      return t as unknown as Table<F>;
    }
    const made = new Table(def);
    this.byName.set(def.name, made as unknown as Table);
    return made;
  }

  /** Every table, by name (a stable order for saving and hashing). */
  tables(): Table[] {
    return [...this.byName.values()].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  }

  spawn(): number {
    if (this.nextId > 0xffffffff) throw new Error('ids de entidad agotados');
    const id = this.nextId++;
    this.entities.add(id);
    return id;
  }

  /** Removes the entity from every table. */
  despawn(id: number): boolean {
    if (!this.entities.has(id)) return false;
    for (const t of this.byName.values()) t.remove(id);
    return true;
  }

  alive(id: number): boolean {
    return this.entities.has(id);
  }
}

function sameFields(a: ComponentDef, b: ComponentDef): boolean {
  const ka = Object.keys(a.fields);
  const kb = Object.keys(b.fields);
  return ka.length === kb.length && ka.every((k, i) => k === kb[i] && typeOf(a.fields[k]) === typeOf(b.fields[k]));
}
