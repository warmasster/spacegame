import { HAULER } from '../src/shared/ship/ships/index.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../src/shared/ship/sim.js';
import { FLIGHT_IDLE, levelness, type FlightCommand } from '../src/shared/ship/flight/index.js';
import { qConj, qRotate } from '../src/shared/ship/geom.js';

const ground = { height: () => 0 };
const s = new ShipSim(1, HAULER, placeShip(HAULER, 0, 0, 0, ground), ground);
const f = (v: number[]) => v.map((x) => x.toFixed(0)).join(',');
function run(sec: number, cmd: FlightCommand) {
  let sys = 0;
  for (let i = 0; i < Math.round(sec * 60); i++) {
    s.flight.step(1 / 60, cmd, { ground });
    sys += 1 / 60;
    while (sys >= 1 / SYSTEMS_HZ) { sys -= 1 / SYSTEMS_HZ; s.tick(1 / SYSTEMS_HZ); }
    if (i % 20 === 0) {
      const fm = s.flight as any;
      const fc = fm.last;
      const wb = qRotate(qConj(s.pose.q), s.pose.w);
      console.log(`t=${(i/60).toFixed(2)} lvl=${levelness(s.pose.q).toFixed(3)} wb=${wb.map((x) => x.toFixed(3)).join(',')} Fwant=${f(fc.F)} Twant=${f(fc.T)} got=${f(Array.from(fm.alloc.got))} lift=${Array.from(fm.liftVec).map((x: number) => x.toFixed(2)).join(',')} avail=${Array.from(fm.avail).map((x: number) => x.toFixed(2)).join(',')}`);
    }
  }
}
run(3, { ...FLIGHT_IDLE, heave: 1 });
