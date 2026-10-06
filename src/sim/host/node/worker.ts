// Entry of the Node worker thread that runs the world (started by `launchWorld`).

import { parentPort, workerData } from 'node:worker_threads';
import type { SimModule } from '../../core/world.js';
import type { SimIn, SimOut } from '../protocol.js';
import { SimRunner } from '../runner.js';
import { FsStorage } from './fsStorage.js';
import type { WorkerInit } from './launch.js';

const init = workerData as WorkerInit;
const port = parentPort!;
try {
  const modules: readonly SimModule[] = init.modules
    ? ((await import(init.modules)) as { modules: readonly SimModule[] }).modules
    : (await import('../../modules/index.js')).WORLD_MODULES;
  const runner = new SimRunner(
    { post: (m: SimOut) => port.postMessage(m), listen: (fn: (m: SimIn) => void) => port.on('message', fn) },
    modules,
    init.dir ? new FsStorage(init.dir) : null,
  );
  await runner.start(init.config);
} catch (e) {
  port.postMessage({ type: 'failed', message: e instanceof Error ? (e.stack ?? e.message) : String(e) } satisfies SimOut);
}
