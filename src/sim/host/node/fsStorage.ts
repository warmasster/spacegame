// Saves in a folder (Node). Writes go to a temporary file, flushed to disk, then renamed over the
// old one: a file is always either the old whole one or the new whole one.
//
// `lockFolder`: one process per world folder (two servers writing the same world would ruin it).

import { mkdirSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs';
import { open, readdir, readFile, rename, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import type { SaveStorage } from '../../persist/saves.js';

export class FsStorage implements SaveStorage {
  constructor(readonly dir: string) {
    mkdirSync(dir, { recursive: true });
  }

  async read(name: string): Promise<Uint8Array | null> {
    try {
      return new Uint8Array(await readFile(join(this.dir, name)));
    } catch (e) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') return null;
      throw e;
    }
  }

  async write(name: string, data: Uint8Array): Promise<void> {
    const file = join(this.dir, name);
    const tmp = `${file}.tmp`;
    const fh = await open(tmp, 'w');
    try {
      await fh.writeFile(data);
      await fh.sync();
    } finally {
      await fh.close();
    }
    // Windows: an antivirus or indexer may hold the old file for a moment
    for (let i = 0; ; i++) {
      try {
        await rename(tmp, file);
        return;
      } catch (e) {
        const code = (e as NodeJS.ErrnoException).code;
        if (i >= 20 || (code !== 'EPERM' && code !== 'EBUSY' && code !== 'EACCES')) throw e;
        await new Promise((r) => setTimeout(r, 50));
      }
    }
  }

  async remove(name: string): Promise<void> {
    await rm(join(this.dir, name), { force: true });
  }

  async list(): Promise<string[]> {
    return (await readdir(this.dir)).filter((f) => !f.endsWith('.tmp') && f !== LOCK);
  }
}

const LOCK = 'lock';
/** Folders this process holds (a second world on the same folder in this process is refused too). */
const held = new Set<string>();

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return (e as NodeJS.ErrnoException).code === 'EPERM';
  }
}

/** Takes the folder for this process; null if another live process has it. */
export function lockFolder(dir: string): { release(): void } | null {
  mkdirSync(dir, { recursive: true });
  const key = process.platform === 'win32' ? resolve(dir).toLowerCase() : resolve(dir);
  if (held.has(key)) return null;
  const file = join(dir, LOCK);
  const me = String(process.pid);
  for (let tries = 0; tries < 2; tries++) {
    try {
      writeFileSync(file, me, { flag: 'wx' });
      held.add(key);
      const release = () => {
        held.delete(key);
        try {
          if (readFileSync(file, 'utf8') === me) unlinkSync(file);
        } catch {
          // already gone
        }
      };
      process.once('exit', release);
      return { release };
    } catch (e) {
      if ((e as NodeJS.ErrnoException).code !== 'EEXIST') throw e;
      let pid = 0;
      try {
        pid = Number(readFileSync(file, 'utf8'));
      } catch {
        // removed meanwhile: try again
      }
      if (pid && pid !== process.pid && alive(pid)) return null;
      try {
        unlinkSync(file);
      } catch {
        // someone else took it: the next try says so
      }
    }
  }
  return null;
}
