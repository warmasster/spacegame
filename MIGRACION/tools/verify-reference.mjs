import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const base=new URL('../reference/',import.meta.url);
const manifest=JSON.parse(readFileSync(new URL('manifest.json',base),'utf8'));
for(const f of manifest.files.filter(f=>f.exists)) {
  const hash=createHash('sha256').update(readFileSync(new URL('sources/'+f.path,base))).digest('hex');
  assert.equal(hash,f.sha256,`Frozen reference changed: ${f.path}`);
}
const {fixtures}=JSON.parse(readFileSync(new URL('fixture-manifest.json',base),'utf8'));
for(const f of fixtures)assert.equal(createHash('sha256').update(readFileSync(new URL('golden/'+f.path,base))).digest('hex'),f.sha256,`Fixture changed: ${f.path}`);
console.log(`Reference: ${manifest.files.filter(f=>f.exists).length} source files and ${fixtures.length} fixtures unchanged.`);
