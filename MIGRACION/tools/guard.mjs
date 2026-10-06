// Read-only inventory of the existing project; all reports live in MIGRACION.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const migration = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const root = resolve(migration, '..');
const inventoryFile = resolve(migration, 'evidence/outside-before.json');
mkdirSync(dirname(inventoryFile), { recursive: true });
function inventory() {
  const paths = execFileSync('git', ['ls-files', '-z', '--cached', '--others', '--exclude-standard'],
    { cwd: root, encoding: 'utf8' }).split('\0').filter(p => p && !p.startsWith('MIGRACION/'));
  return Object.fromEntries([...new Set(paths)].sort().map(p => [p, existsSync(resolve(root, p))
    ? createHash('sha256').update(readFileSync(resolve(root, p))).digest('hex') : null]));
}
if (process.argv[2] === 'snapshot') {
  if (existsSync(inventoryFile)) throw new Error('Initial inventory already exists; do not overwrite it.');
  const files = inventory();
  writeFileSync(inventoryFile, JSON.stringify({ scope: 'tracked and non-ignored untracked project files', files }, null, 2));
  console.log(`Snapshot: ${Object.keys(files).length} project files outside MIGRACION.`);
} else if (process.argv[2] === 'check') {
  const before = JSON.parse(readFileSync(inventoryFile, 'utf8')).files;
  const after = inventory();
  const changed = [...new Set([...Object.keys(before), ...Object.keys(after)])]
    .filter(p => before[p] !== after[p]);
  const result = { scope: 'tracked and non-ignored untracked project files; excludes .git and ignored dependencies',
    files: Object.keys(after).length, changed, pass: changed.length === 0 };
  writeFileSync(resolve(migration, 'evidence/outside-after.json'), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
  if (changed.length) process.exitCode = 1;
} else throw new Error('Usage: node MIGRACION/tools/guard.mjs snapshot|check');
