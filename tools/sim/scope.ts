// The seismograph (npm run sim:scope): every module's gauges over game time, to see a system drift
// toward breaking before it breaks.
//
//   npm run sim:scope                               the server's world (data/world/series.json)
//   npm run sim:scope -- --dir <folder>             another world
//   npm run sim:scope -- --run 3 [--seed 7] [--every 24] [--modules tools/sim/scenario.ts]
//                                                   a world run headless for 3 years, sampled every 24 game hours
//
// Terminal: last value, range and a sparkline per gauge. HTML: tools/sim/out/scope.html.

import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { HOUR } from '../../src/sim/core/calendar.js';
import { World, type SimModule } from '../../src/sim/index.js';
import { SERIES_FILE } from '../../src/sim/host/runner.js';
import { Seismograph, type Series } from '../../src/sim/tools/seismograph.js';
import { fmt, lines, spark, writePage } from './report.js';

const args = process.argv.slice(2);
const opt = (name: string, d: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : d;
};

/** A headless world run for `years`, its gauges sampled every `every` game seconds. */
export async function record(years: number, seed: number, every: number, modulesPath: string): Promise<Series> {
  const mod = (await import(pathToFileURL(resolve(modulesPath)).href)) as { modules?: readonly SimModule[]; WORLD_MODULES?: readonly SimModule[] };
  const w = World.create({ seed, modules: mod.modules ?? mod.WORLD_MODULES ?? [] });
  const scope = new Seismograph(every, 100_000);
  const end = years * w.calendar.year;
  scope.sample(w);
  for (let t = every; t <= end; t += every) {
    w.advance(t);
    scope.sample(w);
  }
  return scope.series();
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  let series: Series;
  let source: string;
  const run = args.indexOf('--run');
  if (run >= 0) {
    const years = Number(args[run + 1] ?? 1);
    const every = Number(opt('every', '24')) * HOUR;
    const modules = opt('modules', 'tools/sim/scenario.ts');
    series = await record(years, Number(opt('seed', '1')), every, modules);
    source = `${modules} · semilla ${opt('seed', '1')} · ${years} años`;
  } else {
    const dir = resolve(opt('dir', process.env.WORLD_DIR ?? 'data/world'));
    const file = resolve(dir, SERIES_FILE);
    if (!existsSync(file)) {
      console.error(`No hay series en ${file} (el servidor las escribe al guardar; o usa --run)`);
      process.exit(1);
    }
    series = JSON.parse(readFileSync(file, 'utf8')) as Series;
    source = file;
  }
  const days = series.t.map((t) => t / 86400);
  console.log(`${source} · ${series.t.length} muestras · día ${fmt(days[0] ?? 0)} → ${fmt(days[days.length - 1] ?? 0)}\n`);
  for (const [name, v] of Object.entries(series.v)) {
    const finite = v.filter(Number.isFinite);
    console.log(`${name.padEnd(30)} ${fmt(finite[finite.length - 1] ?? NaN).padStart(10)}  [${fmt(Math.min(...finite))} … ${fmt(Math.max(...finite))}]  ${spark(v)}`);
  }
  const page = writePage('scope.html', 'Sismógrafo del mundo', `${source} · ${series.t.length} muestras`, [
    { title: 'Indicadores en el tiempo de juego', cards: Object.entries(series.v).map(([name, v]) => ({ title: name, svg: lines(days, [v], { xlabel: (d) => `día ${fmt(d)}` }) })) },
  ]);
  console.log(`\nInforme: ${page}`);
}
