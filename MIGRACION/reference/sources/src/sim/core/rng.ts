// Deterministic randomness for the world (docs/MUNDO.md §3). Integer mixing only (Math.imul and
// shifts): the same numbers on every engine, platform and run.
//
// Counter-based: a draw is hash(key, n) — no hidden state, a key and a counter. Any stream can be
// regenerated from any point (lazy updates; level-of-detail promotion: the same people come out of
// a cohort every time you come close), streams with different keys don't overlap, and the whole
// state of an entity's generator is one number in a table, saved with everything else.

const TWO32 = 4294967296;

/** Bijective 32-bit mix (Wellons' lowbias32, improved constants). */
export function mix32(x: number): number {
  x ^= x >>> 16;
  x = Math.imul(x, 0x21f0aaad);
  x ^= x >>> 15;
  x = Math.imul(x, 0x735a2d97);
  x ^= x >>> 15;
  return x >>> 0;
}

/** Hash of two 32-bit integers (order matters). */
export const hash2 = (a: number, b: number): number => mix32(mix32(a) ^ b);

/** Hash of three 32-bit integers (order matters). */
export const hash3 = (a: number, b: number, c: number): number => mix32(hash2(a, b) ^ c);

/** Seed of the `index`-th thing of a group: seed = hash(world seed, group, index) (the report's rule). */
export const seedOf = (worldSeed: number, group: number, index: number): number => hash3(worldSeed, group, index);

/** The `n`-th 32-bit draw of the stream `key` (n up to 2^53). */
export function draw(key: number, n: number): number {
  const hi = n < TWO32 ? 0 : Math.floor(n / TWO32);
  // the key goes in twice: two keys whose xor is a small number don't give the same stream shifted
  return mix32(mix32((n >>> 0) ^ key) + key + Math.imul(hi, 0x9e3779b9));
}

/** The `n`-th draw of the stream `key` in [0, 1). */
export const unit = (key: number, n: number): number => draw(key, n) / TWO32;

/** A phase in [0, period) that spreads periodic work of many things over the period. */
export const stagger = (key: number, period: number): number => unit(key, 0x5a5a) * period;

/** A stream: its key and its counter (`n` is its whole state; keep it to resume). */
export class Rng {
  constructor(
    readonly key: number,
    public n = 0,
  ) {}

  u32(): number {
    return draw(this.key, this.n++);
  }

  /** [0, 1) */
  float(): number {
    return this.u32() / TWO32;
  }

  /** [lo, hi) */
  range(lo: number, hi: number): number {
    return lo + (hi - lo) * this.float();
  }

  /** Integer in [0, n). */
  int(n: number): number {
    return Math.floor(this.float() * n);
  }

  chance(p: number): boolean {
    return this.float() < p;
  }

  pick<T>(list: readonly T[]): T {
    return list[this.int(list.length)];
  }

  /** Gaussian (Box–Muller). Uses Math.log/cos: reproducible on one engine, not bit-exact across engines. */
  normal(mean = 0, sd = 1): number {
    const u = 1 - this.float();
    const v = this.float();
    return mean + sd * Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
  }
}
