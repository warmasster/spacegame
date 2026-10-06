// Starts the world on a Node worker thread (docs/MUNDO.md §7): the server's main thread never pays
// for it. One process per world folder (a second one runs without saving, and says so).

import { Worker } from 'node:worker_threads';
import type { SaveInfo } from '../../persist/saves.js';
import { SimClient } from '../client.js';
import type { SimConfig, SimOut } from '../protocol.js';
import { lockFolder } from './fsStorage.js';

export interface WorkerInit {
  dir: string | null;
  config: SimConfig;
  /** URL of a module exporting `modules: SimModule[]` (default: the game's, src/sim/modules). */
  modules?: string;
}

export interface WorldLaunch extends WorkerInit {
  log?: (msg: string) => void;
  /** How long loading may take (ms). */
  timeoutMs?: number;
}

export interface WorldThread {
  readonly sim: SimClient;
  /** Where it is saved (null: this session doesn't save). */
  readonly dir: string | null;
  /** Stops it, saves it and ends the thread (at most `timeoutMs`). */
  stop(timeoutMs?: number): Promise<SaveInfo | null>;
}

export async function launchWorld(o: WorldLaunch): Promise<WorldThread> {
  let dir = o.dir;
  const lock = dir ? lockFolder(dir) : null;
  if (dir && !lock) {
    o.log?.(`Mundo: otro proceso está usando ${dir}; esta sesión no guarda`);
    dir = null;
  }
  const init: WorkerInit = { dir, config: o.config, modules: o.modules };
  const worker = new Worker(new URL('./worker.ts', import.meta.url), { workerData: init });
  const sim = new SimClient((m) => worker.postMessage(m));
  let exited = false;
  worker.on('message', (m: SimOut) => {
    if (m.type === 'error') o.log?.(`Mundo: ${m.message}`);
    sim.receive(m);
  });
  worker.on('error', (e) => {
    o.log?.(`Mundo: el hilo falló: ${e.message}`);
    sim.fail(e);
  });
  worker.on('exit', (code) => {
    exited = true;
    lock?.release();
    sim.fail(new Error(`el hilo del mundo terminó (${code})`));
  });

  const end = async () => {
    if (!exited) await worker.terminate();
    lock?.release();
  };
  try {
    await timeout(sim.ready, o.timeoutMs ?? 60_000, 'el mundo tarda demasiado en cargar');
  } catch (e) {
    await end();
    throw e;
  }
  for (const n of sim.notes) o.log?.(`Mundo: ${n}`);

  return {
    sim,
    dir,
    async stop(ms = 10_000) {
      try {
        return exited ? null : await timeout(sim.stop(), ms, 'el mundo no se guardó a tiempo');
      } finally {
        await end();
      }
    },
  };
}

function timeout<T>(p: Promise<T>, ms: number, why: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  return Promise.race([p, new Promise<never>((_, reject) => (timer = setTimeout(() => reject(new Error(why)), ms)))]).finally(() => clearTimeout(timer));
}
