import { HAULER, PEREGRINA } from '../src/shared/ship/ships/index.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../src/shared/ship/sim.js';
import { FLIGHT_IDLE, type FlightCommand } from '../src/shared/ship/flight/index.js';
const ground = { height: () => 0 };
for (const def of [PEREGRINA, HAULER]) {
  const s = new ShipSim(1, def, placeShip(def, 0, 0, 0, ground), ground);
  const run = (sec: number, cmd: FlightCommand) => { let sys = 0; for (let i = 0; i < Math.round(sec * 60); i++) { s.flight.step(1 / 60, cmd, { ground }); sys += 1/60; while (sys >= 1/SYSTEMS_HZ) { sys -= 1/SYSTEMS_HZ; s.tick(1/SYSTEMS_HZ); } } };
  run(3, { ...FLIGHT_IDLE, heave: 1 });
  run(8, FLIGHT_IDLE);
  const fm = s.flight as any;
  console.log(def.id, 'lift', Array.from(fm.liftVec).map((x: number) => x.toFixed(3)).join(','));
  console.log(' rcs', Array.from(fm.rcsOut).map((x: number) => x.toFixed(3)).join(','));
  console.log(' want', fm.last.F.map((x: number) => x.toFixed(0)).join(','), fm.last.T.map((x: number) => x.toFixed(0)).join(','), 'got', Array.from(fm.alloc.got).map((x: number) => x.toFixed(0)).join(','));
}
{
  const s = new ShipSim(1, PEREGRINA, placeShip(PEREGRINA, 0, 0, 0, ground), ground);
  const run = (sec: number, cmd: FlightCommand) => { let sys = 0; for (let i = 0; i < Math.round(sec * 60); i++) { s.flight.step(1 / 60, cmd, { ground }); sys += 1/60; while (sys >= 1/SYSTEMS_HZ) { sys -= 1/SYSTEMS_HZ; s.tick(1/SYSTEMS_HZ); } } };
  run(3, { ...FLIGHT_IDLE, heave: 1 });
  run(8, FLIGHT_IDLE);
  const fm = s.flight as any;
  const al = fm.alloc;
  const want = [...fm.last.F, ...fm.last.T];
  const m2 = fm.mp.mass ** 2; const I = fm.mp.inertia;
  const W = [1 / m2, 3 / m2, 1 / m2, 400 / I[0] ** 2, 400 / I[1] ** 2, 400 / I[2] ** 2];
  const cost = () => { const r=[0,0,0,0,0,0]; for (let j=0;j<al.n;j++) for (let k=0;k<6;k++) r[k]+=al.B[j*6+k]*al.u[j]; let c=0; for (let k=0;k<6;k++) c+=0.5*W[k]*(r[k]-want[k])**2; for (let j=0;j<al.n;j++) c+=al.price[j]*al.u[j]+0.5e-4*al.u[j]**2; return c; };
  console.log('cost before', cost(), Array.from(al.u).map((x: number) => x.toFixed(2)).join(','));
  for (let i = 0; i < 200; i++) al.solve(want, W, 1e-4, 100);
  console.log('cost after ', cost(), Array.from(al.u).map((x: number) => x.toFixed(2)).join(','));
  console.log('price', Array.from(al.price).map((x: number) => x.toExponential(1)).join(','));
  console.log('pre', Array.from(al.pre).map((x: number) => x.toExponential(1)).join(','));
}
