import { build } from 'esbuild';
import { mkdirSync, readFileSync, writeFileSync, copyFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { cpus, platform, arch } from 'node:os';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const m = resolve(root, 'MIGRACION');
if (existsSync(resolve(m,'reference/manifest.json')) && !process.argv.includes('--refresh'))
  throw new Error('Reference is frozen. Use --refresh explicitly only when adopting a new TS reference.');
for (const dir of ['out', 'reference/golden', 'evidence', 'reference/sources']) mkdirSync(resolve(m, dir), { recursive:true });
const outfile = resolve(m, 'out/oracle.mjs');
const compiled = await build({ entryPoints:[resolve(m,'tools/oracle-entry.ts')], bundle:true,
  platform:'node', format:'esm', outfile, metafile:true, write:true, logLevel:'silent' });
const ts = await import(pathToFileURL(outfile).href);
const essential = ['src/shared/space/surface.ts', 'src/shared/noise.ts', 'src/shared/space/cubeSphere.ts',
  'src/shared/space/body.ts', 'src/shared/constants.ts', 'public/assets/astronaut.glb', 'docs/RENDIMIENTO.md'];
const useful = ['src/client/world/sphereTerrain.ts', 'src/client/world/terrainGrid.ts',
  'src/client/world/terrain.worker.ts', 'src/client/render/shadowCull.ts', 'src/client/world/lighting.ts', 'tools/perf/fleet.ts'];
const optional = ['src/client/player/controller.ts', 'src/shared/space/rocks.ts', 'src/client/world/rocks.ts', 'src/client/world/sky.ts'];
const extra = ['src/client/world/terrainShader.ts', 'src/client/player/astronaut.ts',
  'tools/blender/astronaut.py', 'package.json'];
const closure = Object.keys(compiled.metafile.inputs).map(p=>relative(root, resolve(p)).replaceAll('\\','/'))
  .filter(p=>p.startsWith('src/'));
const files = [...new Set([...essential, ...useful, ...optional, ...extra, ...closure])];
const manifest = files.map(p=> {
  const source = resolve(root,p);
  if (!existsSync(source)) return { path:p, exists:false };
  const data = readFileSync(source), target = resolve(m,'reference/sources',p);
  mkdirSync(dirname(target),{recursive:true}); copyFileSync(source,target);
  return { path:p, exists:true, bytes:data.length, sha256:createHash('sha256').update(data).digest('hex'),
    group:essential.includes(p)?'essential':useful.includes(p)?'useful':optional.includes(p)?'optional':'dependency' };
});
if (manifest.some(f=>essential.includes(f.path)&&!f.exists)) throw new Error('Required source missing');
writeFileSync(resolve(m,'reference/manifest.json'),JSON.stringify({version:1,files:manifest},null,2));

const rng = ts.mulberry32(0x1969);
const dirs = [];
for(let i=0;i<1000;i++) {
  const y=rng()*2-1, az=rng()*Math.PI*2, q=Math.sqrt(1-y*y);
  dirs.push([q*Math.cos(az),y,q*Math.sin(az)]);
}
// Face seams, corners, axes and base-local points; retain all coordinates as f64 in fixtures.
for(let face=0;face<6;face++) for(const a of [-1,-1+1e-9,0,1-1e-9,1])
  for(const b of [-1,0,1]) dirs.push([...ts.cubeDir(face,a,b,[0,0,0])]);
for(let i=0;i<110;i++) {
  const x=(rng()-.5)*3000,z=(rng()-.5)*3000, y=ts.MOON_BODY.radius;
  const len=Math.hypot(x,y,z); dirs.push([x/len,y/len,z/len]);
}
const rows=['case,seed,level,min_feature,dx,dy,dz,height,albedo'];
const point = ts.surfaceSample(); let caseId=0;
for(const seed of [0,1969,4294967295]) for(const level of [0,1]) {
  const s = new ts.BodySurface(ts.MOON_SURFACE,ts.MOON_BODY,seed,level?[0,1,0]:undefined);
  for(let i=0;i<dirs.length;i++) for(const feature of [0,0.37,28,1550]) {
    const d=dirs[i]; s.sample(...d,feature,point);
    rows.push([caseId++,seed,level,feature,...d,point.height,point.albedo].join(','));
  }
}
writeFileSync(resolve(m,'reference/golden/surface.csv'),rows.join('\n')+'\n');
const noise=['seed,x,y,z,noise,hash2,hash3,random0,crater'];
for(const seed of [0,7331,1969,4294967295]) {
  const n=new ts.Noise3(seed), r=ts.mulberry32(seed);
  for(let i=0;i<128;i++) {
    const x=(rng()-.5)*1000,y=(rng()-.5)*1000,z=(rng()-.5)*1000;
    noise.push([seed,x,y,z,n.noise(x,y,z),ts.hash2i(x,y,seed),ts.hash3i(x,y,z,seed),r(),ts.craterProfile(i/50,70,i/128)].join(','));
  }
}
writeFileSync(resolve(m,'reference/golden/noise.csv'),noise.join('\n')+'\n');
// Future GPU precision gate: untouched, full-detail f64 TS truth (not a rounded f32 oracle).
const gpu=dirs.slice(0,1000).map(d=>{const s=ts.bareSurface(ts.MOON_BODY,1969);return {d,height:s.height(d,0)};});
writeFileSync(resolve(m,'reference/golden/gpu-1000.json'),JSON.stringify({seed:1969,minFeature:0,toleranceMeters:0.01,points:gpu}));
const fixtures=['surface.csv','noise.csv','gpu-1000.json'].map(path=>({path,
  sha256:createHash('sha256').update(readFileSync(resolve(m,'reference/golden',path))).digest('hex')}));
writeFileSync(resolve(m,'reference/fixture-manifest.json'),JSON.stringify({fixtures},null,2));
const glb=readFileSync(resolve(root,'public/assets/astronaut.glb'));
const gltf=JSON.parse(glb.subarray(20,20+glb.readUInt32LE(12)).toString());
const asset={bytes:glb.length,meshes:gltf.meshes.length,primitives:gltf.meshes.reduce((n,m)=>n+m.primitives.length,0),
  nodes:gltf.nodes.length,materials:gltf.materials.length,skins:(gltf.skins??[]).map(s=>({joints:s.joints.length})),
  animations:(gltf.animations??[]).map(a=>({name:a.name??'',channels:a.channels.length})),
  triangles:gltf.meshes.reduce((n,m)=>n+m.primitives.reduce((n,p)=>n+(gltf.accessors[p.indices??p.attributes.POSITION].count/3),0),0)};
writeFileSync(resolve(m,'evidence/asset.json'),JSON.stringify(asset,null,2));
const report={scope:'TS natural surface with/without home offset, excludes modifiers',points:caseId,noiseCases:512,
  gpuGate:'pending: 1000 f64 truth samples saved',machine:{cpu:cpus()[0].model,threads:cpus().length,os:platform(),arch:arch(),node:process.version},asset};
writeFileSync(resolve(m,'evidence/reference.json'),JSON.stringify(report,null,2));
console.log(JSON.stringify(report,null,2));
