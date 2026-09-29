import { HAULER, PEREGRINA } from '../src/shared/ship/ships/index.js';
import { ShipSim, placeShip, SYSTEMS_HZ } from '../src/shared/ship/sim.js';
import { FLIGHT_IDLE, NAV_POINTS, levelness } from '../src/shared/ship/flight/index.js';
import type { ShipDef } from '../src/shared/ship/def.js';
const ground = { height: (x: number, z: number) => 0.3 * Math.sin(x * 0.05) + 0.2 * Math.cos(z * 0.07) };
function press(s: ShipSim, key: string, v?: number) {
  const c = s.def.controls.find((x) => x.key === key && x.kind !== 'cover')!;
  if (v !== undefined) { s.sw[key] = v; return; }
  const r = s.interact(c.index);
  if ('reason' in r) console.log('  refused', key, r.reason);
}
function run(s: ShipSim, sec: number, every = 2, until?: () => boolean) {
  let sys = 0;
  for (let i = 0; i < Math.round(sec * 60); i++) {
    s.flight.step(1 / 60, FLIGHT_IDLE, { ground });
    sys += 1 / 60;
    while (sys >= 1 / SYSTEMS_HZ) {
      sys -= 1 / SYSTEMS_HZ;
      const r = s.tick(1 / SYSTEMS_HZ);
      for (const e of r.events) if (e.type === 'say') console.log('   say:', e.text);
    }
    if (i % (60 * every) === 0) {
      const t = s.flight.telemetry;
      console.log(`  t=${(i / 60).toFixed(0)} p=${s.pose.p.map((v) => v.toFixed(1)).join(',')} agl=${t.agl.toFixed(2)} vs=${t.vs.toFixed(2)} gs=${t.gs.toFixed(2)} hdg=${((t.heading * 180) / Math.PI).toFixed(0)} lvl=${levelness(s.pose.q).toFixed(3)} landed=${s.landed} gear=${s.mover('gear').toFixed(2)} modes=${t.modes.join('/')}`);
    }
    if (until?.()) return i / 60;
  }
  return sec;
}
for (const def of [PEREGRINA, HAULER] as ShipDef[]) {
  console.log(def.id);
  const s = new ShipSim(1, def, placeShip(def, 0, 0, 0.3, ground), ground);
  run(s, 1);
  const wp = NAV_POINTS.findIndex((p) => p.id === 'crater');
  s.sw['ap.wp'] = wp;
  s.sw['ap.alt.sel'] = 2; // 20 m
  press(s, 'ap.to');
  console.log(' takeoff');
  run(s, 30, 3, () => s.sw['ap.to'] === 0);
  console.log(' nav to', NAV_POINTS[wp].name, NAV_POINTS[wp].x, NAV_POINTS[wp].z);
  press(s, 'ap.nav');
  const t = run(s, 200, 10, () => s.sw['ap.nav'] === 0);
  console.log(' arrived after', t.toFixed(0), 's at', s.pose.p.map((v) => v.toFixed(1)).join(','));
  press(s, 'ap.land');
  run(s, 60, 3, () => s.sw['ap.land'] === 0);
  run(s, 3, 1);
  console.log(' landed', s.landed, 'sleep', s.flight.sleeping, 'lvl', levelness(s.pose.q).toFixed(3), 'ap.on', s.sw['ap.on'], 'fuel', s.massNow().groups.propelente.toFixed(0));
}
