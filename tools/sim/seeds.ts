// Thousands of seeds, headless (npm run sim:seeds): the report's test — emergent systems break in
// boring ways (an empire eats everything, factions dissolve), so run the world alone over many
// seeds and look at the distributions, not the one world you watch.
//
//   npm run sim:seeds                                  200 seeds × 5 years of the test scenario
//   npm run sim:seeds -- --n 1000 --years 20           more
//   npm run sim:seeds -- --modules src/sim/modules/index.ts   the game's world (alone, nobody near)
//   --workers 8  --from 1
//
// For each gauge of each module (SimModule.gauges): its distribution at the end and its p10/median/
// p90 year by year; for each kind of record (what happened): how often per century and in how many
// seeds; how many different histories there were; errors. Console summary, JSON and HTML in
// tools/sim/out/.

import { mkdirSync, writeFileSync } from 'node:fs';
import { cpus } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { Worker } from 'node:worker_threads';
import { OUT_DIR, histogram, lines, quantile, stats, table, writePage, fmt } from './report.js';
import type { SeedResult } from './sweep.js';

const args = process.argv.slice(2);
const opt = (name: string, d: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : d;
};
const n = Number(opt('n', '200'));
const years = Number(opt('years', '5'));
const from = Number(opt('from', '1'));
const workers = Math.max(1, Math.min(n, Number(opt('workers', String(Math.max(1, cpus().length - 1))))));
const modulesPath = resolve(opt('modules', 'tools/sim/scenario.ts'));
const quiet = args.includes('--quiet');

export async function sweep(): Promise<SeedResult[]> {
  const results: SeedResult[] = [];
  let next = from;
  const end = from + n;
  const t0 = performance.now();
  await Promise.all(
    Array.from({ length: workers }, () => {
      const w = new Worker(new URL('./sweepWorker.ts', import.meta.url), { workerData: { modules: pathToFileURL(modulesPath).href, years } });
      return new Promise<void>((done, fail) => {
        const give = () => w.postMessage(next < end ? next++ : null);
        w.on('message', (m: SeedResult | 'ready') => {
          if (m !== 'ready') {
            results.push(m);
            if (!quiet && results.length % Math.max(1, Math.floor(n / 10)) === 0) process.stdout.write(`\r${results.length}/${n} semillas (${((performance.now() - t0) / 1000).toFixed(1)} s)`);
          }
          give();
        });
        w.on('error', fail);
        w.on('exit', () => done());
      });
    }),
  );
  if (!quiet) process.stdout.write('\n');
  return results.sort((a, b) => a.seed - b.seed);
}

export function summarize(results: SeedResult[]) {
  const gauges = [...new Set(results.flatMap((r) => Object.keys(r.gauges)))].sort();
  const kinds = [...new Set(results.flatMap((r) => Object.keys(r.kinds)))].sort();
  const g = gauges.map((name) => {
    const end = results.map((r) => r.gauges[name]);
    const yearly = Array.from({ length: years }, (_, y) => results.map((r) => r.yearly[name]?.[y]).filter(Number.isFinite).sort((a, b) => a - b));
    return { name, end, st: stats(end), zero: end.filter((v) => v === 0).length / results.length, band: [yearly.map((s) => quantile(s, 0.1)), yearly.map((s) => quantile(s, 0.5)), yearly.map((s) => quantile(s, 0.9))] };
  });
  const k = kinds.map((name) => {
    const counts = results.map((r) => r.kinds[name] ?? 0);
    return { name, perCentury: (counts.reduce((a, b) => a + b, 0) / results.length / years) * 100, seeds: counts.filter((c) => c > 0).length / results.length };
  });
  const distinct = new Set(results.map((r) => r.digest)).size;
  const errors = results.reduce((a, r) => a + r.errors, 0);
  const ms = results.reduce((a, r) => a + r.ms, 0);
  return { gauges: g, kinds: k, distinct, errors, ms };
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  console.log(`${n} semillas × ${years} años · ${workers} hilos · ${modulesPath}`);
  const results = await sweep();
  const s = summarize(results);
  console.log(`\nHistorias distintas: ${s.distinct}/${results.length} · errores: ${s.errors} · ${fmt(s.ms / results.length)} ms por semilla`);
  console.log('\nIndicador'.padEnd(34) + 'mín'.padStart(10) + 'p10'.padStart(10) + 'mediana'.padStart(10) + 'p90'.padStart(10) + 'máx'.padStart(10) + '  a cero');
  for (const x of s.gauges) console.log(x.name.padEnd(33) + [x.st.min, x.st.p10, x.st.median, x.st.p90, x.st.max].map((v) => fmt(v).padStart(10)).join('') + `  ${(x.zero * 100).toFixed(0)} %`);
  console.log('\nQué pasa'.padEnd(34) + 'por siglo'.padStart(12) + '  en semillas');
  for (const x of s.kinds) console.log(x.name.padEnd(33) + fmt(x.perCentury).padStart(12) + `  ${(x.seeds * 100).toFixed(0)} %`);
  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  mkdirSync(OUT_DIR, { recursive: true });
  writeFileSync(join(OUT_DIR, `seeds-${stamp}.json`), JSON.stringify({ n, years, modules: modulesPath, summary: s, results }, null, 1));
  const page = writePage(
    'seeds.html',
    `Barrido de semillas · ${results.length} × ${years} años`,
    `${modulesPath} · ${s.distinct} historias distintas · ${s.errors} errores · generado ${new Date().toLocaleString('es-ES')}`,
    [
      { title: 'Indicadores al final (distribución entre semillas)', cards: s.gauges.map((x) => ({ title: x.name, svg: histogram(x.end), facts: [['mediana', x.st.median], ['p10 – p90', `${fmt(x.st.p10)} – ${fmt(x.st.p90)}`], ['a cero', `${(x.zero * 100).toFixed(0)} %`]] })) },
      { title: 'Año a año (p10 · mediana · p90)', cards: s.gauges.map((x) => ({ title: x.name, svg: lines(Array.from({ length: years }, (_, i) => i + 1), x.band, { band: true, xlabel: (v) => `año ${v}` }) })) },
    ],
    [table(['Qué pasa', 'por siglo', 'en semillas'], s.kinds.map((x) => [x.name, x.perCentury, `${(x.seeds * 100).toFixed(0)} %`]))],
  );
  console.log(`\nInforme: ${page}`);
  process.exit(s.errors ? 1 : 0);
}
