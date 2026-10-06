// A worker of the seed sweep: runs the seeds it is given, one message back per seed.

import { parentPort, workerData } from 'node:worker_threads';
import type { SimModule } from '../../src/sim/index.js';
import { runSeed } from './sweep.js';

const { modules: spec, years } = workerData as { modules: string; years: number };
const mod = (await import(spec)) as { modules?: readonly SimModule[]; WORLD_MODULES?: readonly SimModule[] };
const modules = mod.modules ?? mod.WORLD_MODULES ?? [];
parentPort!.on('message', (seed: number | null) => {
  if (seed === null) return process.exit(0);
  parentPort!.postMessage(runSeed(seed, years, modules));
});
parentPort!.postMessage('ready');
