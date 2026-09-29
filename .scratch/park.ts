import { HAULER } from '../src/shared/ship/ships/index.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../src/shared/ship/sim.js';
import { FLIGHT_IDLE, levelness } from '../src/shared/ship/flight/index.js';
const ground = { height: (x: number, z: number) => 0.3 * Math.sin(x * 0.05) + 0.2 * Math.cos(z * 0.07) };
const yaw = Number(process.argv[2] ?? 0.7);
const s = new ShipSim(1, HAULER, placeShip(HAULER, 0, 0, yaw, ground), ground);
const p0 = [...s.pose.p];
let acc = 0;
for (let i = 0; i < 600; i++) {
  s.flight.step(1 / 60, FLIGHT_IDLE, { ground });
  acc += 1 / 60;
  while (acc >= 1 / SYSTEMS_HZ) { acc -= 1 / SYSTEMS_HZ; s.tick(1 / SYSTEMS_HZ); }
  if (i % 15 === 0 || i < 10) console.log(i, s.pose.p.map((v, k) => (v - p0[k]).toFixed(3)).join(','), 'v', s.pose.v.map((v) => v.toFixed(3)).join(','), 'w', s.pose.w.map((v) => v.toFixed(3)).join(','), 'lvl', levelness(s.pose.q).toFixed(5), 'landed', s.landed, 'sleep', s.flight.sleeping, 'out', Array.from(s.flight.out).map((x) => x.toFixed(2)).join(','));
}
