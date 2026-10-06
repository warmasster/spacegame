// Headless server + clients for tests (no browser): the real server on its own port, clients that
// speak its protocol and keep everything they receive. Used by tools/net/*.ts.

import { spawn } from 'node:child_process';
import { WebSocket } from 'ws';
import { PROTOCOL_VERSION, type ClientMessage, type ServerMessage } from '../../src/shared/protocol.js';
import { decodeServer, encodeClient } from '../../src/shared/wire.js';

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export interface TestServer {
  readonly log: () => string;
  stop(): Promise<void>;
}

/** Starts the server (production mode: needs `npm run build` once) with test tools on. */
export async function startServer(port: number, env: Record<string, string> = {}): Promise<TestServer> {
  const child = spawn(process.execPath, ['--import', 'tsx', 'src/server/index.ts'], {
    env: { ...process.env, PORT: String(port), WORLD_DIR: 'none', DEV_TOOLS: '1', ...env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let log = '';
  child.stdout.on('data', (d) => (log += d));
  child.stderr.on('data', (d) => (log += d));
  const exited = new Promise<void>((r) => child.on('exit', () => r()));
  for (let i = 0; i < 300 && !/Servidor .*listo/.test(log); i++) {
    if (child.exitCode !== null) throw new Error(`el servidor salió:\n${log}`);
    await sleep(100);
  }
  if (!/Servidor .*listo/.test(log)) throw new Error(`el servidor no arrancó:\n${log}`);
  return {
    log: () => log,
    async stop() {
      child.kill();
      await Promise.race([exited, sleep(5000)]);
    },
  };
}

export interface TestClient {
  readonly welcome: Extract<ServerMessage, { type: 'welcome' }>;
  readonly inbox: ServerMessage[];
  send(msg: ClientMessage): void;
  /** Waits for a message that passes `pred` (among those after `from`); null on timeout. */
  until<T extends ServerMessage>(pred: (m: ServerMessage) => m is T, ms?: number, from?: number): Promise<T | null>;
  until(pred: (m: ServerMessage) => boolean, ms?: number, from?: number): Promise<ServerMessage | null>;
  close(): void;
}

export async function connect(port: number, name: string): Promise<TestClient> {
  const ws = new WebSocket(`ws://localhost:${port}/ws`);
  ws.binaryType = 'arraybuffer';
  const inbox: ServerMessage[] = [];
  const welcome = await new Promise<Extract<ServerMessage, { type: 'welcome' }>>((resolve, reject) => {
    ws.on('message', (data) => {
      const m = data instanceof ArrayBuffer ? decodeServer(new DataView(data)) : (JSON.parse(String(data)) as ServerMessage);
      if (!m) return;
      if (m.type === 'welcome') resolve(m);
      else if (m.type === 'reject') reject(new Error(m.reason));
      else inbox.push(m);
    });
    ws.on('open', () => ws.send(JSON.stringify({ type: 'hello', version: PROTOCOL_VERSION, name })));
    ws.on('error', reject);
  });
  const send = (msg: ClientMessage) => ws.send((encodeClient(msg as never) ?? JSON.stringify(msg)) as never);
  return {
    welcome,
    inbox,
    send,
    async until(pred: (m: ServerMessage) => boolean, ms = 3000, from = 0) {
      const end = performance.now() + ms;
      for (;;) {
        for (let i = from; i < inbox.length; i++) if (pred(inbox[i])) return inbox[i];
        if (performance.now() > end) return null;
        await sleep(20);
      }
    },
    close: () => ws.close(),
  } as TestClient;
}
