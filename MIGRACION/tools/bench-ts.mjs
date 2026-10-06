import { readFileSync,writeFileSync } from 'node:fs';
import { BodySurface,MOON_SURFACE,MOON_BODY } from '../out/oracle.mjs';
const fixture=JSON.parse(readFileSync(new URL('../reference/golden/gpu-1000.json',import.meta.url),'utf8'));
const surface=new BodySurface(MOON_SURFACE,MOON_BODY,1969,[0,1,0]);
const points=fixture.points.map(p=>p.d);
let checksum=0;
function batch(){let sum=0;for(let i=0;i<points.length;i++)sum+=surface.height(points[i],0);return sum;}
let start=performance.now();while(performance.now()-start<300)checksum=batch();
const times=new Float64Array(100000);let count=0;start=performance.now();
while(performance.now()-start<2000 && count<times.length){const t=performance.now();checksum=batch();times[count++]=performance.now()-t;}
const elapsed=(performance.now()-start)/1000,sorted=times.subarray(0,count).sort();
const mean=sorted.reduce((s,n)=>s+n,0)/count;
const result={schema:1,milestone:'H0',scope:'scalar CPU natural-surface microbenchmark; NOT frame time',
  runtime:process.version,seed:1969,pointsPerBatch:1000,batches:count,warmupSeconds:.3,elapsedSeconds:elapsed,
  batchMs:{mean,p99:sorted[Math.ceil(count*.99)-1],max:sorted[count-1]},usPerSample:mean,checksum,gpuMs:null,frameMs:null};
writeFileSync(new URL('../evidence/h0-ts-surface.json',import.meta.url),JSON.stringify(result,null,2));
// Sensitivity of the existing f64 function to quantized input directions, NOT a WGSL emulation.
let max=0,fail=0;
for(const {d,height} of fixture.points){const q=d.map(Math.fround),len=Math.hypot(...q);const error=Math.abs(surface.height(q.map(x=>x/len),0)-height);max=Math.max(max,error);if(error>.01)fail++;}
writeFileSync(new URL('../evidence/f32-input-sensitivity.json',import.meta.url),JSON.stringify({scope:'Only input quantization then renormalization; height still evaluated by TS f64; not a GPU test',points:points.length,maxHeightErrorMeters:max,overOneCentimeter:fail},null,2));
console.log(JSON.stringify(result,null,2));
