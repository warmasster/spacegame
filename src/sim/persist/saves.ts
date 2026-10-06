// Where a world is kept (docs/MUNDO.md §6): two save slots written in turn, plus the log segments.
//
// A save writes the new log segments first, then the main file into the older slot. A crash at
// any point leaves the other slot whole, and everything it refers to on disk: segments only grow,
// and a full chunk's segment never changes. Loading takes the newest slot that reads whole (checksum,
// every segment) and falls back to the other. Files are gzip-compressed (CompressionStream: the
// same code in Node and in a browser).

import type { World, SimModule } from '../core/world.js';
import { readHeader, restore, snapshot, type RestoreReport } from './snapshot.js';

/** Somewhere to keep files (a folder, IndexedDB, memory). `write` must replace atomically. */
export interface SaveStorage {
  read(name: string): Promise<Uint8Array | null>;
  write(name: string, data: Uint8Array): Promise<void>;
  remove(name: string): Promise<void>;
  list(): Promise<string[]>;
}

export class MemoryStorage implements SaveStorage {
  readonly files = new Map<string, Uint8Array>();

  async read(name: string): Promise<Uint8Array | null> {
    const f = this.files.get(name);
    return f ? f.slice() : null;
  }

  async write(name: string, data: Uint8Array): Promise<void> {
    this.files.set(name, data.slice());
  }

  async remove(name: string): Promise<void> {
    this.files.delete(name);
  }

  async list(): Promise<string[]> {
    return [...this.files.keys()];
  }
}

export const SAVE_SLOTS = ['world-a.sim', 'world-b.sim'] as const;
export const segmentName = (i: number) => `log-${String(i).padStart(6, '0')}.seg`;

export interface SaveInfo {
  gen: number;
  /** Bytes written (compressed). */
  bytes: number;
  segments: number;
  ms: number;
}

export interface Loaded {
  world: World;
  report: RestoreReport;
  gen: number;
  /** Loaded from the older slot (the newer one was damaged). */
  fallback: boolean;
  /** What couldn't be read. */
  problems: string[];
}

export class WorldSaves {
  private gen = 0;
  /** Slot holding the save the world came from (the next save goes to the other). */
  private current: string | null = null;

  constructor(
    readonly storage: SaveStorage,
    readonly compress = true,
  ) {}

  /** The newest whole save, or null if there is none. Throws if saves exist but none can be read. */
  async load(modules: readonly SimModule[]): Promise<Loaded | null> {
    const problems: string[] = [];
    const found: Array<{ slot: string; gen: number; main: Uint8Array; segments: number }> = [];
    let any = false;
    for (const slot of SAVE_SLOTS) {
      const raw = await this.storage.read(slot);
      if (!raw) continue;
      any = true;
      try {
        const main = await unpack(raw);
        const h = readHeader(main);
        found.push({ slot, gen: h.gen, main, segments: h.segments });
      } catch (e) {
        problems.push(`${slot}: ${message(e)}`);
      }
    }
    if (!any) return null;
    found.sort((a, b) => b.gen - a.gen);
    this.gen = Math.max(0, ...found.map((f) => f.gen));
    for (const f of found) {
      try {
        const segments: Uint8Array[] = [];
        for (let i = 0; i < f.segments; i++) {
          const raw = await this.storage.read(segmentName(i));
          if (!raw) throw new Error(`falta ${segmentName(i)}`);
          segments.push(await unpack(raw));
        }
        const { world, report } = restore(f.main, segments, modules);
        this.current = f.slot;
        return { world, report, gen: f.gen, fallback: f !== found[0] || problems.length > 0, problems };
      } catch (e) {
        problems.push(`${f.slot}: ${message(e)}`);
      }
    }
    throw new Error(`no se puede cargar el mundo: ${problems.join('; ')}`);
  }

  /** Saves the world as it is now (the bytes are taken at once; the world may go on meanwhile). */
  async save(w: World): Promise<SaveInfo> {
    const t0 = performance.now();
    if (this.current === null) {
      // a new world: never over someone else's save without a `clear`
      const files = await this.storage.list();
      if (SAVE_SLOTS.some((s) => files.includes(s))) throw new Error('ya hay un mundo guardado aquí: cárgalo o bórralo (clear)');
    }
    const gen = this.gen + 1;
    const snap = snapshot(w, { gen });
    let bytes = 0;
    for (const s of snap.segments) {
      const packed = await pack(s.bytes, this.compress);
      await this.storage.write(segmentName(s.index), packed);
      bytes += packed.length;
    }
    // what is on disk now, whatever the world did meanwhile
    w.log.saved = Math.max(w.log.saved, snap.header.segments);
    const slot = this.current === SAVE_SLOTS[0] ? SAVE_SLOTS[1] : SAVE_SLOTS[0];
    const main = await pack(snap.main, this.compress);
    await this.storage.write(slot, main);
    bytes += main.length;
    this.gen = gen;
    this.current = slot;
    return { gen, bytes, segments: snap.header.segments, ms: performance.now() - t0 };
  }

  /** Deletes the saved world (slots and segments). */
  async clear(): Promise<void> {
    for (const f of await this.storage.list()) if ((SAVE_SLOTS as readonly string[]).includes(f) || /^log-\d+\.seg$/.test(f)) await this.storage.remove(f);
    this.gen = 0;
    this.current = null;
  }
}

const message = (e: unknown) => (e instanceof Error ? e.message || e.name : String(e)) || 'ilegible';

async function pack(b: Uint8Array, compress: boolean): Promise<Uint8Array> {
  if (!compress) return b;
  const stream = new Blob([b as Uint8Array<ArrayBuffer>]).stream().pipeThrough(new CompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function unpack(b: Uint8Array): Promise<Uint8Array> {
  if (b.length < 2 || b[0] !== 0x1f || b[1] !== 0x8b) return b;
  const stream = new Blob([b as Uint8Array<ArrayBuffer>]).stream().pipeThrough(new DecompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}
