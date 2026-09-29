import { HAULER } from '../src/shared/ship/ships/index.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../src/shared/ship/sim.js';
const ground = { height: () => 0 };
const s = new ShipSim(1, HAULER, placeShip(HAULER, 0, 0, 0, ground), ground);
const press = (key: string) => { const c = s.def.controls.find((x) => x.key === key && x.kind !== 'cover')!; const r = s.interact(c.index); console.log('press', key, c.id, JSON.stringify(r)); };
press('ap.hdg'); for (let i = 0; i < 2; i++) console.log(JSON.stringify(s.tick(1 / SYSTEMS_HZ).sw));
console.log('hdg', s.sw['ap.hdg'], 'on', s.sw['ap.on']);
press('ap.nav'); for (let i = 0; i < 2; i++) console.log(JSON.stringify(s.tick(1 / SYSTEMS_HZ).sw));
console.log('hdg', s.sw['ap.hdg'], 'nav', s.sw['ap.nav']);
