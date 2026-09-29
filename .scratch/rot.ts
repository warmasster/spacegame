import { HAULER } from '../src/shared/ship/ships/index.js';
import { ShipSim, placeShip } from '../src/shared/ship/sim.js';
import { FLIGHT_IDLE } from '../src/shared/ship/flight/index.js';
const ground = { height: () => 0 };
const SKY = { height: () => -5000 };
const s = new ShipSim(1, HAULER, placeShip(HAULER, 0, 0, 0, ground), ground);
s.sw.gear = 0; const i = s.sys.moverIndex('gear'); if (i !== undefined) s.st[i] = 0;
s.landed = false; s.pose.p[1] += 60; s.flight.wake();
for (let k = 0; k < 120; k++) {
  s.flight.step(1 / 60, { ...FLIGHT_IDLE, surge: 1, yaw: 0.6 }, { ground: SKY });
  if (k % 20 === 0) console.log(k, 'w', s.pose.w.map((x) => x.toFixed(3)).join(','), 'v', s.pose.v.map((x) => x.toFixed(2)).join(','), 'avail', Array.from(s.flight.avail).map((x) => x.toFixed(1)).join(','), 'T', (s.flight as any).last.T.map((x: number) => x.toFixed(0)).join(','), 'direct', (s.flight as any).last.direct);
}
