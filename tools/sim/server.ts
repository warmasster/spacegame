// The real server with its world (npm run test:sim:server). No browser: the server is started on
// its own port with its world in a temporary folder, twice.
//
//   - it starts the world thread and says so (date, entities, records, folder);
//   - a new world is on disk from its first moment;
//   - killed and started again, it takes the folder back (the dead one's lock) and resumes that
//     world (one more record: its own resumption), not a new one;
//   - Ctrl+C / SIGTERM saves it before exiting (not on Windows: there the console sends it, a
//     test can only kill the process).
//
// Exit code 1 if any check fails. `--verbose` for the logs. PORT (default 3108).

import { spawn } from 'node:child_process';
import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const verbose = process.argv.includes('--verbose');
const PORT = Number(process.env.PORT) || 3108;
const dir = mkdtempSync(join(tmpdir(), 'world-'));
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}

interface Run {
  line: string;
  log: string;
  stop(signal?: NodeJS.Signals): Promise<string>;
}

function start(): Promise<Run> {
  const child = spawn(process.execPath, ['--import', 'tsx', 'src/server/index.ts'], { env: { ...process.env, PORT: String(PORT), WORLD_DIR: dir }, stdio: ['ignore', 'pipe', 'pipe'] });
  let log = '';
  const exited = new Promise<void>((r) => child.on('exit', () => r()));
  const stop = async (signal: NodeJS.Signals = 'SIGKILL') => {
    child.kill(signal);
    await Promise.race([exited, new Promise((r) => setTimeout(r, 15_000))]);
    return log;
  };
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`el servidor no arrancó el mundo:\n${log}`)), 60_000);
    const read = (d: Buffer) => {
      log += d;
      const m = /Mundo: (año .*)/.exec(log);
      if (m) {
        clearTimeout(timer);
        resolve({ line: m[1], get log() { return log; }, stop });
      }
    };
    child.stdout.on('data', read);
    child.stderr.on('data', read);
    child.on('exit', (code) => reject(new Error(`el servidor salió (${code}):\n${log}`)));
  });
}

const records = (line: string) => Number(/(\d+) registros/.exec(line)?.[1] ?? NaN);

try {
  const first = await start();
  const files = readdirSync(dir);
  check('arranca el hilo del mundo y lo anuncia; un mundo nuevo está en disco desde el primer momento', files.includes('world-a.sim') && files.includes('lock') && first.line.includes(dir), { line: first.line, files });
  await first.stop();
  const second = await start();
  check('rearrancado tras morir de golpe: recupera la carpeta (candado huérfano) y reanuda ese mundo (un registro más: su reanudación)', records(second.line) === records(first.line) + 1 && second.line.includes(dir) && !/otro proceso/.test(second.log), { first: first.line, second: second.line });
  if (process.platform === 'win32') {
    await second.stop();
    console.log('—    Ctrl+C guarda al salir: en Windows lo manda la consola (una prueba solo puede matar el proceso)');
  } else {
    const log = await second.stop('SIGINT');
    check('Ctrl+C: guarda el mundo antes de salir', /Mundo guardado/.test(log) && !readdirSync(dir).includes('lock'), { tail: log.trim().split('\n').slice(-3) });
  }
  if (verbose) console.log(second.log.trim().split('\n').slice(-6).join('\n'));
} catch (e) {
  failures++;
  console.error('FAIL', e instanceof Error ? e.message : e);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(failures ? `\n${failures} fallo(s)` : '\nServidor y mundo en orden.');
process.exit(failures ? 1 : 0);
