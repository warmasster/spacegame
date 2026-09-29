import { HAULER, PEREGRINA } from '../src/shared/ship/ships/index.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../src/shared/ship/sim.js';
import { FLIGHT_IDLE, performance as perf, levelness, type FlightCommand } from '../src/shared/ship/flight/index.js';
import type { ShipDef } from '../src/shared/ship/def.js';

const ground = { height: (x: number, z: number) => 0.3 * Math.sin(x * 0.05) + 0.2 * Math.cos(z * 0.07) };
function make(def: ShipDef) {
  return new ShipSim(1, def, placeShip(def, 0, 0, 0.3, ground), ground);
}
function run(s: ShipSim, sec: number, cmd: FlightCommand = FLIGHT_IDLE, log = false) {
  let sys = 0;
  for (let i = 0; i < Math.round(sec * 60); i++) {
    s.flight.step(1 / 60, cmd, { ground });
    sys += 1 / 60;
    while (sys >= 1 / SYSTEMS_HZ) {
      sys -= 1 / SYSTEMS_HZ;
      s.tick(1 / SYSTEMS_HZ);
    }
    if (log && i % 60 === 0) {
      const t = s.flight.telemetry;
      console.log(`  t=${(i / 60).toFixed(0)} agl=${t.agl.toFixed(2)} vs=${t.vs.toFixed(2)} gs=${t.gs.toFixed(2)} lvl=${levelness(s.pose.q).toFixed(3)} landed=${s.landed} sleep=${t.sleeping} modes=${t.modes.join('/')} thr=${(t.thrust / 1000).toFixed(1)}kN W=${(t.weight / 1000).toFixed(1)}kN`);
    }
  }
}
for (const def of [PEREGRINA, HAULER]) {
  const s = make(def);
  const m = s.massNow();
  const p = perf(def, m);
  console.log(def.id, 'mass', m.mass.toFixed(0), 'com', m.com.map((v) => v.toFixed(2)).join(','), 'liftTwr', p.liftTwr.toFixed(2), 'twr', p.twr.toFixed(2));
  const y0 = s.pose.p[1];
  run(s, 5);
  console.log(' parked dy', (s.pose.p[1] - y0).toFixed(3), 'dxz', Math.hypot(s.pose.p[0], s.pose.p[2]).toFixed(3), 'landed', s.landed, 'sleep', s.flight.sleeping);
  console.log(' climb R');
  run(s, 6, { ...FLIGHT_IDLE, heave: 1 }, true);
  console.log(' release (hover)');
  run(s, 6, FLIGHT_IDLE, true);
  console.log(' forward');
  run(s, 5, { ...FLIGHT_IDLE, surge: 1 }, true);
  console.log(' release');
  run(s, 8, FLIGHT_IDLE, true);
  console.log(' pos', s.pose.p.map((v) => v.toFixed(1)).join(','));
}
