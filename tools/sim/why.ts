// The historian (npm run sim:why): reads a saved world and follows its causes.
//
//   npm run sim:why                      summary of data/world and its last records
//   npm run sim:why -- 1234              record 1234: its causes back to the root, and what it caused
//   npm run sim:why -- e57               what the log says about entity 57
//   npm run sim:why -- recent 50         the last 50 records
//   --dir <folder>                       another world (default: WORLD_DIR or data/world)
//
// Read-only: it never writes, so it can look at the world of a running server (its last save).

import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { WorldSaves } from '../../src/sim/index.js';
import { FsStorage } from '../../src/sim/host/node/fsStorage.js';
import type { RecordView } from '../../src/sim/host/protocol.js';
import { recordView } from '../../src/sim/host/runner.js';

const args = process.argv.slice(2);
const at = args.indexOf('--dir');
const dir = resolve(at >= 0 ? args.splice(at, 2)[1] : (process.env.WORLD_DIR ?? 'data/world'));
if (!existsSync(dir)) {
  console.error(`No hay mundo en ${dir}`);
  process.exit(1);
}
const loaded = await new WorldSaves(new FsStorage(dir)).load([]);
if (!loaded) {
  console.error(`No hay partida guardada en ${dir}`);
  process.exit(1);
}
const w = loaded.world;
const view = (id: number) => recordView(w, id);
// an entity of the world (†: no longer alive), or someone from outside it (a player, the engine)
const who = (id: number) => (!id ? '—' : id < w.store.nextId ? `#e${id}${w.alive(id) ? '' : '†'}` : `#${id}`);
const line = (r: RecordView, indent = '') => {
  const extra = [r.a ? `a=${+r.a.toFixed(3)}` : '', r.data !== undefined ? JSON.stringify(r.data) : ''].filter(Boolean).join(' ');
  const why = [r.cause, ...r.also].filter(Boolean).map((c) => `#${c}`).join(', ');
  return `${indent}#${r.id}  ${r.date}  ${r.kind.padEnd(18)} ${who(r.actor)} → ${who(r.subject)}${extra ? `  ${extra}` : ''}${why ? `  ← ${why}` : ''}`;
};

console.log(`Mundo en ${dir} (partida ${loaded.gen}${loaded.fallback ? ', la anterior: la última no se pudo leer' : ''})`);
console.log(`  ${w.date()} · ${w.store.entities.count} entidades vivas · ${w.queue.size} eventos pendientes · ${w.log.count} registros`);
const tables = w.store.tables().filter((t) => !t.name.startsWith('$')).map((t) => `${t.name} ${t.count}`);
const orphans = [...w.orphans.values()].map((t) => `${t.name} ${t.count}`);
if (tables.length || orphans.length) console.log(`  tablas: ${[...tables, ...orphans].join(' · ')}`);
console.log('');

const [what, n] = args;
if (!what || what === 'recent') {
  const count = Number(n) || 20;
  for (let id = Math.max(1, w.log.count - count + 1); id <= w.log.count; id++) console.log(line(view(id)));
} else if (/^e\d+$/.test(what)) {
  const id = Number(what.slice(1));
  const inTables = [...w.store.tables().filter((t) => !t.name.startsWith('$') && t.has(id)).map((t) => t.name), ...[...w.orphans.values()].filter((t) => t.ids.includes(id)).map((t) => t.name)];
  console.log(`Entidad ${id}${w.alive(id) ? '' : ' (ya no existe)'}: ${inTables.join(', ') || 'sin tablas'}`);
  for (const r of w.log.about(id, Number(n) || 50).reverse()) console.log(line(view(r)));
} else if (/^\d+$/.test(what) && w.log.has(Number(what))) {
  const id = Number(what);
  console.log('Por qué (hacia atrás):');
  w.log.chain(id).forEach((c, i) => console.log(line(view(c), '  '.repeat(i + 1))));
  const others = w.log.also(id);
  for (const o of others) console.log(`  también por: ${line(view(o))}`);
  const effects = w.log.effects(id, Number(n) || 20);
  if (effects.length) {
    console.log('\nQué causó (hacia delante):');
    for (const e of effects) console.log(line(view(e), '  '));
  }
} else {
  console.error(`No entiendo «${what}»: un número de registro, e<entidad> o recent`);
  process.exit(1);
}
