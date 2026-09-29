// Continuous ship state as one flat table of named numbers. Every system declares its variables
// once (deterministically, from the ship definition), so the server and every client agree on the
// index of each name and only changed values travel over the network, as [index, value] pairs.

export class VarTable {
  readonly names: string[] = [];
  /** Replication step: a value is resent once it moved this much since the last send (< 0: never sent). */
  readonly quanta: number[] = [];
  readonly initial: number[] = [];
  private index = new Map<string, number>();

  /** Declare a variable (idempotent). Returns its index. */
  define(name: string, quantum: number, initial = 0) {
    const had = this.index.get(name);
    if (had !== undefined) return had;
    const i = this.names.length;
    this.names.push(name);
    this.quanta.push(quantum);
    this.initial.push(initial);
    this.index.set(name, i);
    return i;
  }

  /** Index of a variable (throws on typos: a silent -1 would read garbage). */
  idx(name: string) {
    const i = this.index.get(name);
    if (i === undefined) throw new Error(`ship var "${name}" not defined`);
    return i;
  }

  has(name: string) {
    return this.index.has(name);
  }

  get size() {
    return this.names.length;
  }
}

/** Round to a variable's quantum (what goes on the wire). */
export function quantize(v: number, q: number) {
  return q > 0 ? Math.round(v / q) * q : v;
}

/** Drop the float noise a quantum leaves (0.30000000000000004 → 0.3) without a string round trip. */
export function tidy(v: number) {
  return Math.round(v * 1e6) / 1e6;
}

/**
 * Server-side change tracker: returns the flat [i, v, i, v…] list of variables that moved at least
 * one quantum since they were last sent (and remembers what it sent).
 */
export class VarSync {
  private sent: Float64Array;

  constructor(
    private table: VarTable,
    st: Float64Array,
  ) {
    this.sent = Float64Array.from(st);
  }

  diff(st: Float64Array): number[] {
    const out: number[] = [];
    const q = this.table.quanta;
    for (let i = 0; i < st.length; i++) {
      // negative quantum = server-private (internal integrator state, never sent)
      if (q[i] < 0) continue;
      const v = st[i];
      if (Math.abs(v - this.sent[i]) < q[i] * 0.999) continue;
      const r = quantize(v, q[i]);
      this.sent[i] = r;
      out.push(i, tidy(r));
    }
    return out;
  }

  /** Full state for a joining client. */
  full(st: Float64Array): number[] {
    const q = this.table.quanta;
    return Array.from(st, (v, i) => tidy(quantize(v, q[i])));
  }
}
