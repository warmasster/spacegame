import { HAULER, PEREGRINA } from '../src/shared/ship/ships/index.js';
import { massProperties, performance, thrusters, thrust } from '../src/shared/ship/flight.js';
for (const def of [HAULER, PEREGRINA]) {
  const m = massProperties(def);
  const perf = performance(def, m, 1.62);
  console.log(def.id, 'mass', m.mass.toFixed(0), 'com', m.com.map(v=>v.toFixed(2)).join(','), 'I', m.inertia.map(v=>v.toFixed(0)).join(','));
  console.log('  groups', JSON.stringify(Object.fromEntries(Object.entries(m.groups).map(([k,v])=>[k, Math.round(v)]))));
  console.log('  perf', JSON.stringify(Object.fromEntries(Object.entries(perf).map(([k,v])=>[k, +v.toFixed(2)]))));
  console.log('  bounds', JSON.stringify(def.bounds), 'floorH', def.floorHeight, 'gear', JSON.stringify(def.gear?.legs));
  for (const p of def.parts.filter(p=>p.type==='engine'||p.type==='rcs')) console.log('   ', p.id, p.type, p.c, p.half, p.p.thrustN, p.mass);
  const dry = massProperties(def, () => 0);
  console.log('  dry mass', dry.mass.toFixed(0), 'com', dry.com.map(v=>v.toFixed(2)).join(','));
}
