import { createReadStream, existsSync, statSync } from 'node:fs';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { networkInterfaces } from 'node:os';
import { extname, join, normalize, resolve } from 'node:path';
import { createGzip } from 'node:zlib';
import { WebSocketServer } from 'ws';
import { DEFAULT_PORT, MAX_PLAYERS } from '../shared/constants.js';
import { Room } from './room.js';

const dev = process.argv.includes('--dev');
const port = Number(process.env.PORT) || DEFAULT_PORT;
const root = resolve(import.meta.dirname, '../..');

const log = (msg: string) => console.log(`[${new Date().toLocaleTimeString()}] ${msg}`);

const MIME: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.wasm': 'application/wasm',
  '.glb': 'model/gltf-binary',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.webp': 'image/webp',
  '.ktx2': 'image/ktx2',
  '.bin': 'application/octet-stream',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
};

/** Minimal static file server for the production build. */
function serveStatic(dir: string) {
  return (req: IncomingMessage, res: ServerResponse) => {
    const url = new URL(req.url ?? '/', 'http://localhost');
    let file = normalize(join(dir, decodeURIComponent(url.pathname)));
    if (!file.startsWith(dir)) {
      res.writeHead(403).end();
      return;
    }
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html');
    if (!existsSync(file)) {
      res.writeHead(404).end('Not found');
      return;
    }
    const immutable = url.pathname.startsWith('/assets/');
    const type = MIME[extname(file)] ?? 'application/octet-stream';
    const compressible = /^(text\/|application\/(json|wasm|octet-stream)|model\/gltf)/.test(type);
    const gzip = compressible && /\bgzip\b/.test(String(req.headers['accept-encoding'] ?? ''));
    res.writeHead(200, {
      'Content-Type': type,
      'Cache-Control': immutable ? 'public, max-age=3600' : 'no-cache',
      ...(gzip ? { 'Content-Encoding': 'gzip', Vary: 'Accept-Encoding' } : {}),
    });
    const stream = createReadStream(file);
    if (gzip) stream.pipe(createGzip({ level: 6 })).pipe(res);
    else stream.pipe(res);
  };
}

const server = createServer();

if (dev) {
  const { createServer: createVite } = await import('vite');
  const vite = await createVite({
    root,
    server: { middlewareMode: true, hmr: { server } },
    appType: 'spa',
  });
  server.on('request', vite.middlewares);
} else {
  const dist = join(root, 'dist/client');
  if (!existsSync(join(dist, 'index.html'))) {
    console.error('No hay build del cliente. Ejecuta primero: npm run build');
    process.exit(1);
  }
  server.on('request', serveStatic(dist));
}

const room = new Room(log);
const wss = new WebSocketServer({ noServer: true, maxPayload: 64 * 1024 });
wss.on('connection', (socket) => room.connect(socket));
server.on('upgrade', (req, socket, head) => {
  if (new URL(req.url ?? '/', 'http://localhost').pathname !== '/ws') return; // e.g. Vite HMR
  wss.handleUpgrade(req, socket, head, (ws) => wss.emit('connection', ws, req));
});
setInterval(() => room.heartbeat(), 10_000);

server.listen(port, () => {
  const addrs = Object.values(networkInterfaces())
    .flat()
    .filter((a) => a && a.family === 'IPv4' && !a.internal)
    .map((a) => `http://${a!.address}:${port}`);
  log(`Servidor ${dev ? '(dev) ' : ''}listo — máx. ${MAX_PLAYERS} jugadores`);
  log(`  Local: http://localhost:${port}`);
  for (const a of addrs) log(`  Red:   ${a}`);
});
