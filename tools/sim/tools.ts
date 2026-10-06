// The world's tools, checked (part of npm run test:sim): the seed sweep and the seismograph.
//
//   sweep        one seed run twice gives the same history; different seeds, different ones; no errors
//   seismograph  samples on its game-time grid, keeps a bounded history, and the runner writes it
//                beside the save and picks it up again after a restart
//
// Exit code 1 if anything fails. `--verbose` for the numbers.

import { HOUR } from '../../src/sim/core/calendar.js';
import { MemoryStorage, World } from '../../src/sim/index.js';
import type { SimIn, SimOut } from '../../src/sim/host/protocol.js';
import { SERIES_FILE, SimRunner } from '../../src/sim/host/runner.js';
import { readGauges, Seismograph, type Series } from '../../src/sim/tools/seismograph.js';
import { colonies, modules } from './scenario.js';
import { runSeed } from './sweep.js';
import { record } from './scope.js';

const verbose = process.argv.includes('--verbose');
let failures = 0;
function check(name: string, ok: boolean, info: Record<string, unknown> = {}) {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}

{
  const a = runSeed(5, 2, modules);
  const b = runSeed(5, 2, modules);
  const many = Array.from({ length: 12 }, (_, i) => runSeed(100 + i, 2, modules));
  const distinct = new Set(many.map((r) => r.digest)).size;
  check('barrido: una semilla dos veces = la misma historia; 12 semillas, 12 historias; sin errores; indicadores y tipos de hecho recogidos', a.digest === b.digest && JSON.stringify(a.gauges) === JSON.stringify(b.gauges) && distinct === 12 && many.every((r) => r.errors === 0) && 'colonies.population' in a.gauges && a.yearly['colonies.population'].length === 2 && (a.kinds['birth'] ?? 0) > 0, {
    gauges: a.gauges,
    kinds: a.kinds,
    ms: a.ms.toFixed(0),
  });
}

{
  const s = await record(1, 3, 24 * HOUR, 'tools/sim/scenario.ts');
  const scope = new Seismograph(HOUR, 10);
  const w = World.create({ seed: 1, modules: [colonies(2)] });
  let samples = 0;
  for (let t = 0; t < 30 * HOUR; t += 600) {
    w.advance(t);
    if (scope.tick(w)) samples++;
  }
  const kept = scope.series();
  check('sismógrafo: una muestra por intervalo de juego (366 en un año a 24 h), historia acotada, todos los indicadores', s.t.length === 366 && Object.keys(s.v).length === 3 && samples === 30 && kept.t.length === 10 && kept.t[9] === 29 * HOUR && JSON.stringify(Object.keys(readGauges(w))) === JSON.stringify(Object.keys(kept.v)), { samples, kept: kept.t.length });
}

{
  let real = 0;
  const storage = new MemoryStorage();
  const start = async () => {
    const sent: SimOut[] = [];
    let deliver: (m: SimIn) => void = () => undefined;
    const r = new SimRunner({ post: (m) => sent.push(m), listen: (fn) => (deliver = fn) }, modules, storage, () => real);
    await r.start({ seed: 9, scale: 86400, tickMs: 1e9, autosaveS: 0, scopeS: 6 * HOUR });
    return { r, sent, deliver: (m: SimIn) => deliver(m) };
  };
  const one = await start();
  for (let i = 0; i < 20; i++) {
    real += 500;
    one.r.tick();
  }
  await one.r.stop();
  const file = storage.files.get(SERIES_FILE);
  const saved = file ? (JSON.parse(new TextDecoder().decode(file)) as Series) : null;
  const two = await start();
  real += 2000;
  two.r.tick();
  two.deliver({ type: 'query', id: 1, q: { q: 'series' } });
  await new Promise((r) => setTimeout(r, 20));
  const ans = two.sent.find((m) => m.type === 'answer' && m.id === 1);
  const after = ans && ans.type === 'answer' ? (ans.result as Series) : null;
  await two.r.stop();
  check('el runner escribe las series junto a la partida y las recupera al reanudar (siguen donde iban)', !!saved && saved.t.length >= 20 && !!after && after.t.length > saved.t.length && after.t[saved.t.length - 1] === saved.t[saved.t.length - 1], { saved: saved?.t.length, after: after?.t.length });
}

console.log(failures ? `\n${failures} fallo(s)` : '\nHerramientas del mundo en orden.');
process.exit(failures ? 1 : 0);
