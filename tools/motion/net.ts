// The real server, two headless clients (npm run test:motion:net). No browser: the server is
// started on its own port, one client flies a ship at 1600 m/s from a fixed-step loop on a jittery
// timer, the other watches the pose stream as a crew member aboard. Then the pilot leaves the helm
// and the server flies on. Checks:
//
//   - every pose is stamped with the time of the step that made it: consecutive stamps are whole
//     steps apart (± the clock's slew) and the positions between them agree with the velocities
//     (a stamp 1 ms off is 1.6 m at this speed);
//   - the server takes over from where the ship is now (its replica of the pilot's reports carried
//     to its present), not from the last report a trip old: no jump back.
//
// Exit code 1 if any check fails. `--verbose` for the numbers. PORT (default 3107).

import { spawn } from 'node:child_process';
import { WebSocket } from 'ws';
import { StepClock } from '../../src/shared/time/stepClock.js';
import { PROTOCOL_VERSION, type ServerMessage } from '../../src/shared/protocol.js';
import { decodeServer, encodeClient } from '../../src/shared/wire.js';
import type { V3 } from '../../src/shared/ship/geom.js';

const verbose = process.argv.includes('--verbose');
const PORT = Number(process.env.PORT) || 3107;
const URL = `ws://localhost:${PORT}/ws`;
const HMS = 1000 / 60;
const SPEED = 1600;
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown>) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const server = spawn('npx tsx src/server/index.ts', { env: { ...process.env, PORT: String(PORT), WORLD_DIR: 'none' }, shell: true, stdio: ['ignore', 'pipe', 'pipe'] });
let serverLog = '';
server.stdout.on('data', (d) => (serverLog += d));
server.stderr.on('data', (d) => (serverLog += d));
const stop = () => {
  if (process.platform === 'win32') spawn('taskkill', ['/pid', String(server.pid), '/T', '/F'], { stdio: 'ignore' });
  else server.kill();
};

interface Client {
  ws: WebSocket;
  welcome: Extract<ServerMessage, { type: 'welcome' }>;
  offset: number;
  inbox: ServerMessage[];
  send(msg: object): void;
  serverNow(): number;
}

async function connect(name: string): Promise<Client> {
  for (let tries = 0; ; tries++) {
    try {
      return await new Promise<Client>((resolve, reject) => {
        const ws = new WebSocket(URL);
        ws.binaryType = 'arraybuffer';
        const c: Partial<Client> & { inbox: ServerMessage[] } = { ws, inbox: [], offset: 0 };
        const read = (data: unknown): ServerMessage | null => {
          if (data instanceof ArrayBuffer) return decodeServer(new DataView(data));
          return JSON.parse(String(data)) as ServerMessage;
        };
        ws.on('message', (data) => {
          const m = read(data);
          if (!m) return;
          if (m.type === 'welcome') {
            c.welcome = m;
            c.offset = m.serverTime - performance.now();
            c.send = (msg) => ws.send((encodeClient(msg as never) ?? JSON.stringify(msg)) as never);
            c.serverNow = () => performance.now() + c.offset!;
            resolve(c as Client);
          } else if (m.type === 'pong') {
            const rtt = performance.now() - m.t;
            c.offset = m.serverTime + rtt / 2 - performance.now();
          } else if (m.type === 'reject') reject(new Error(m.reason));
          else c.inbox.push(m);
        });
        ws.on('open', () => ws.send(JSON.stringify({ type: 'hello', version: PROTOCOL_VERSION, name })));
        ws.on('error', reject);
      });
    } catch (e) {
      if (tries > 60) throw e;
      await sleep(500);
    }
  }
}

try {
  const pilot = await connect('piloto');
  const crew = await connect('tripulante');
  for (const c of [pilot, crew]) c.send({ type: 'ping', t: performance.now() });
  await sleep(200);
  const snap = pilot.welcome.ships[0];
  const id = snap.id;
  const start = snap.pose?.p ?? snap.place.p;
  // high above the base, going fast along its horizon
  const P0: V3 = [start[0], start[1] + 20000, start[2]];
  // the watcher sits aboard it: it gets every pose
  const aboard = () => crew.send({ type: 'state', t: crew.serverNow(), s: { p: [0, 1, 0], v: [0, 0, 0], yaw: 0, pitch: 0, f: 0, fr: id } });
  aboard();
  pilot.send({ type: 'pilot', ship: id, on: true });
  await sleep(100);

  // the pilot's client: a fixed-step loop on a jittery timer, reports every other step stamped with its step clock
  const clock = new StepClock(HMS);
  clock.sync(pilot.serverNow());
  const t0 = clock.t;
  const where = (t: number): V3 => [P0[0] + (SPEED * (t - t0)) / 1000, P0[1], P0[2]];
  let acc = 0, last = performance.now(), steps = 0, flying = true;
  const loop = setInterval(() => {
    const now = performance.now();
    acc += now - last;
    last = now;
    while (acc >= HMS) {
      acc -= HMS;
      steps++;
      clock.step();
      if (flying && steps % 2 === 0) {
        pilot.send({ type: 'flight', ship: id, t: clock.t, agl: 20000, out: [], p: where(clock.t), q: [0, 0, 0, 1], v: [SPEED, 0, 0], w: [0, 0, 0], landed: false, pad: false });
      }
    }
    clock.sync(pilot.serverNow() - acc);
  }, 4);
  const keep = setInterval(aboard, 50);
  await sleep(3000);
  flying = false;
  pilot.send({ type: 'pilot', ship: id, on: false });
  const leftAt = clock.t;
  await sleep(2000);
  clearInterval(loop);
  clearInterval(keep);

  // --- the watcher's stream -------------------------------------------------------------------
  // who flew each pose: nobody yet (parked, before the pilot sat), the pilot, then the server
  const poses: Array<{ t: number; p: V3; v: V3; by: 'parked' | 'pilot' | 'server' }> = [];
  let by: 'parked' | 'pilot' | 'server' = 'parked';
  for (const m of crew.inbox) {
    if (m.type === 'pilot' && m.ship === id) by = m.id === 0 ? 'server' : 'pilot';
    if (m.type === 'shipPose' && m.ship === id) poses.push({ t: m.t, p: m.p, v: m.v, by });
  }
  const pilots = poses.filter((p) => p.by === 'pilot');
  const served = poses.filter((p) => p.by === 'server');
  // stamps whole steps apart, and positions that agree with the velocities between them
  const regular = (list: typeof poses) => {
    let off = 0, bad = 0;
    for (let i = 1; i < list.length; i++) {
      const dt = list[i].t - list[i - 1].t;
      const k = Math.max(1, Math.round(dt / HMS));
      off = Math.max(off, Math.abs(dt - k * HMS) / k);
      const vx = (list[i].p[0] - list[i - 1].p[0]) / (dt / 1000);
      const mean = (list[i].v[0] + list[i - 1].v[0]) / 2;
      bad = Math.max(bad, Math.abs(vx - mean));
    }
    return { stepOff: off, speedMismatch: bad, n: list.length };
  };
  if (verbose) {
    for (let i = 1; i < pilots.length; i++) {
      const dt = pilots[i].t - pilots[i - 1].t;
      const vx = (pilots[i].p[0] - pilots[i - 1].p[0]) / (dt / 1000);
      if (Math.abs(vx - SPEED) > 1) console.log('  muestra', i, { dt: dt.toFixed(3), vx: vx.toFixed(1), t: pilots[i].t.toFixed(2) });
    }
  }
  const a = regular(pilots);
  const b = regular(served);
  check('poses del piloto: selladas con su paso (vel. entre muestras = vel. declarada ±0,5 m/s a 1600 m/s)', a.n > 30 && a.speedMismatch < 0.5 && a.stepOff < 0.15, { muestras: a.n, desfaseDePaso_ms: a.stepOff.toFixed(3), desacuerdo_mps: a.speedMismatch.toFixed(3) });
  check('poses del servidor: selladas con su paso aunque su temporizador tiemble', b.n > 20 && b.stepOff < 0.15 && b.speedMismatch < 2, { muestras: b.n, desfaseDePaso_ms: b.stepOff.toFixed(3), desacuerdo_mps: b.speedMismatch.toFixed(3) });
  // the take-over: the server's first pose against where the pilot's ship would be at that time
  const first = served[0];
  const gap = first ? Math.hypot(first.p[0] - where(first.t)[0], first.p[1] - where(first.t)[1], first.p[2] - where(first.t)[2]) : Infinity;
  check('relevo del servidor a 1600 m/s: sigue desde donde está la nave ahora (< 2 m)', gap < 2, { salto_m: gap.toFixed(3), msTrasLevantarse: first ? (first.t - leftAt).toFixed(1) : null });
  for (const c of [pilot, crew]) c.ws.close();
} catch (e) {
  failures++;
  console.error('FAIL', e, serverLog.slice(-2000));
} finally {
  stop();
}
if (verbose) console.log(serverLog.trim().split('\n').slice(-5).join('\n'));
console.log(failures ? `\n${failures} fallo(s)` : '\nRed en orden.');
setTimeout(() => process.exit(failures ? 1 : 0), 300);
