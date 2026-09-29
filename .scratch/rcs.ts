import { HAULER, PEREGRINA } from '../src/shared/ship/ships/index.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../src/shared/ship/sim.js';
const ground = { height: () => 0 };
for (const def of [HAULER, PEREGRINA]) {
  const s = new ShipSim(1, def, placeShip(def, 0, 0, 0, ground), ground);
  for (let i = 0; i < 40; i++) s.tick(1 / SYSTEMS_HZ);
  const out: string[] = [];
  for (const name of s.vars.names ?? []) if (/rcs|lift|feed/.test(name)) out.push(`${name}=${s.get(name).toFixed(2)}`);
  console.log(def.id, 'sw rcs', s.sw['rcs'], 'v.rcs', s.sw['v.rcs'], out.join(' '));
  s.flight.thrusters.availability(s, s.flight.avail);
  console.log(' avail', Array.from(s.flight.avail).map((x) => x.toFixed(2)).join(','), s.flight.thrusters.list.map((t) => t.part.id + ':' + t.master).join(' '));
}
