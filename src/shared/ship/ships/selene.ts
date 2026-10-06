// "Selene" light cargo hauler: cockpit → systems corridor → cargo bay with rear ramp.
// Ship space: metres, +X starboard, +Y up, nose toward −Z, origin = deck level on the centreline.
// Everything here is data; the builder in def.ts turns it into panels, controls and routing.

import {
  boardFrame,
  buildConsoles,
  buildParts,
  buildProps,
  doorSensor,
  finishShip,
  outsidePoint,
  paintPanels,
  rampSensor,
  roofAt,
  ShipBuilder,
  shipManual,
  supportBlock,
  type CompartmentDef,
  type ConsoleSpec,
  type ControlSpec,
  type DoorDef,
  type FluidNetDef,
  type LifeSupportDef,
  type LoadDef,
  type ManualSection,
  type ModuleDef,
  type MoverDef,
  type OpeningDef,
  type PartSpec,
  type RampDef,
  type ShipDef,
  type SubsystemDef,
} from '../def.js';
import { clipPoly, norm, type V2, type V3 } from '../geom.js';
import { part, prop } from '../catalog/index.js';
import { AP_SPACE_MANUAL, SPACE_MANUAL, autopilotConsole, decompressionManual, loadOf } from './kit.js';

const T = 0.1; // hull skin
const FLOOR_T = 0.08;
const BULK_T = 0.08;

// Cross-sections (interior surface), x ascending along the top.
const CARGO: V2[] = [[-2.6, 0], [-2.6, 2.3], [-1.9, 3.0], [0, 3.0], [1.9, 3.0], [2.6, 2.3], [2.6, 0]];
const FWD: V2[] = [[-1.6, 0], [-1.6, 1.85], [-1.05, 2.4], [1.05, 2.4], [1.6, 1.85], [1.6, 0]];
const DOOR: V2[] = [[-0.6, 0], [-0.6, 2.05], [0.6, 2.05], [0.6, 0]];
const RAMP_OPENING: V2[] = [[-1.7, 0], [-1.7, 2.8], [1.7, 2.8], [1.7, 0]];

const Z_NOSE = -9.6;
const Z_CK = -6.4; // cockpit | corridor bulkhead
const Z_CR = -2.4; // corridor | cargo bulkhead
const Z_TAIL = 5.6;

// Windshield plane: from the dash (y 0.95 at the nose) raked back to the roof.
const WS_BOTTOM = 0.95;
const WS_RAKE = 0.8 / 1.45; // metres back per metre up
const NOSE_CLIP = { origin: [0, WS_BOTTOM, Z_NOSE] as V3, n: norm([0, 0.8, -1.45]) };
const wsZ = (y: number) => Z_NOSE + (y - WS_BOTTOM) * WS_RAKE;

export const MODULES: ModuleDef[] = [
  { zone: 'cockpit', z0: Z_NOSE, z1: Z_CK, profile: FWD, cols: 2 },
  { zone: 'corridor', z0: Z_CK, z1: Z_CR, profile: FWD, cols: 3 },
  { zone: 'cargo', z0: Z_CR, z1: Z_TAIL, profile: CARGO, cols: 5 },
];

function buildPanels() {
  const B = new ShipBuilder();
  const [ck, cr, cg] = MODULES;
  // cockpit: wrap-around canopy (upper wall row + chamfers are glass)
  B.strip(ck, 'CK', ['L', 'LC', 'T', 'RC', 'R'], [2, 1, 1, 1, 2], (s, r) => ((s === 'L' || s === 'R') && r === 1) || s === 'LC' || s === 'RC' ? 'glass' : 'hull', T, NOSE_CLIP);
  B.cap('cockpit', 'CK-N', Z_NOSE, FWD, null, { outwardZ: -1, kind: 'hull', t: T, rowH: 1.2, splitX: [0], clip: NOSE_CLIP, face: 'inner' });
  // windshield: the profile above the dash projected onto the raked plane, two panes
  const above = FWD.filter((p) => p[1] > WS_BOTTOM);
  const ws: V3[] = [[-1.6, WS_BOTTOM, wsZ(WS_BOTTOM)], ...above.map((p): V3 => [p[0], p[1], wsZ(p[1])]), [1.6, WS_BOTTOM, wsZ(WS_BOTTOM)]];
  const left = clipPoly(ws, [0, 0, 0], [1, 0, 0]);
  const right = clipPoly(ws, [0, 0, 0], [-1, 0, 0]);
  B.addPoly(left, [0, 0.5, -1], { id: 'CK-W1', kind: 'glass', zone: 'cockpit', t: 0.05, face: 'inner' });
  B.addPoly(right, [0, 0.5, -1], { id: 'CK-W2', kind: 'glass', zone: 'cockpit', t: 0.05, face: 'inner' });
  B.floor('cockpit', 'CK', -1.6, 1.6, Z_NOSE, Z_CK, 2, 2, FLOOR_T);
  B.cap('cockpit', 'BK1', Z_CK, FWD, DOOR, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.2, face: 'mid', other: 'corridor' });

  B.strip(cr, 'CR', ['L', 'LC', 'T', 'RC', 'R'], [2, 1, 1, 1, 2], () => 'hull', T);
  B.floor('corridor', 'CR', -1.6, 1.6, Z_CK, Z_CR, 2, 3, FLOOR_T);
  B.cap('corridor', 'BK2', Z_CR, FWD, DOOR, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.2, face: 'mid', other: 'cargo' });

  B.strip(cg, 'CG', ['L', 'LC', 'T', 'T', 'RC', 'R'], [2, 1, 1, 1, 1, 2], () => 'hull', T);
  // the two roof segments share a label; make ids unique
  let roof = 0;
  for (const p of B.panels) if (p.zone === 'cargo' && p.id.startsWith('CG-T-')) p.id = `CG-T${roof++ % 2 === 0 ? 'L' : 'R'}-${p.id.split('-')[2]}`;
  B.cap('cargo', 'CG-FW', Z_CR, CARGO, FWD, { outwardZ: -1, kind: 'hull', t: T, rowH: 1.5, face: 'inner' });
  B.cap('cargo', 'CG-AFT', Z_TAIL, CARGO, RAMP_OPENING, { outwardZ: 1, kind: 'hull', t: T, rowH: 1.5, face: 'inner' });
  B.floor('cargo', 'CG', -2.6, 2.6, Z_CR, Z_TAIL, 3, 5, FLOOR_T);
  // livery: charcoal lower wall row and nose cap, orange waterline on the second row, hazard
  // stripes around the ramp opening
  return paintPanels(B.panels, [
    { scheme: 1, when: (p) => /^(L|R)1$/.test(p.id.split('-')[1]) || p.id.startsWith('CK-N') },
    { scheme: 3, when: (p) => /^(L|R)2$/.test(p.id.split('-')[1]) },
    { scheme: 2, when: (p) => p.id.startsWith('CG-AFT') && p.c[1] < 1.6 },
  ]);
}

// Power circuits: breaker in the corridor cabinet, conduit runs to the loads (a blown panel over a
// run cuts the circuit), trip rating, standing load and default priority (see power.ts).
const CIRCUITS: Array<Omit<SubsystemDef, 'breaker' | 'priority'> & { pri: number }> = [
  {
    id: 'lights', desc: 'Luces interiores de cabina, pasillo y bodega.',
    label: 'ILUMINACIÓN',
    color: 0xffd36a,
    rating: 3,
    base: 0.1,
    pri: 1,
    routes: [
      [[-1.5, 1.6, -4.1], [-1.25, 2.2, -4.1], [-0.35, 2.33, -4.1], [-0.35, 2.33, -8.6]],
      [[-0.35, 2.33, -4.1], [-0.35, 2.33, Z_CR], [-0.35, 2.93, Z_CR + 0.1], [-0.35, 2.93, 5.2]],
    ],
  },
  { id: 'ext', desc: 'Luces de navegación, baliza anticolisión y focos de aterrizaje.', label: 'LUCES EXTERIORES', color: 0x7fd6ff, rating: 4, base: 0.05, pri: 2, routes: [[[-1.5, 1.6, -4.3], [-1.25, 2.2, -4.3], [0.35, 2.33, -4.3], [0.35, 2.33, -8.6]]] },
  { id: 'doors', desc: 'Motores de las dos puertas interiores.', label: 'PUERTAS', color: 0x9cff8a, rating: 5, base: 0.1, pri: 1, routes: [[[-1.5, 1.7, -4.5], [-1.3, 2.1, -4.5], [-1.3, 2.1, Z_CK + 0.05]], [[-1.3, 2.1, -4.5], [-1.3, 2.1, Z_CR - 0.05]]] },
  { id: 'hyd', desc: 'Bomba hidráulica: rampa de carga, tren de aterrizaje y góndolas.', label: 'HIDRÁULICA', color: 0xff9a5c, rating: 10, base: 0.2, pri: 1, routes: [[[-1.5, 1.0, -4.2], [-1.3, -0.16, -4.2], [0, -0.16, -4.2], [0, -0.16, Z_TAIL - 0.15]]] },
  { id: 'avionics', desc: 'Pantallas multifunción e instrumentos.', label: 'AVIÓNICA', color: 0xb58cff, rating: 4, base: 1.5, pri: 0, routes: [[[-1.5, 1.0, -4.4], [-1.3, -0.16, -4.4], [-0.55, -0.16, -4.4], [-0.55, -0.16, -9.2]]] },
  { id: 'shield', desc: 'Motores de las persianas del escudo térmico.', label: 'ESCUDO TÉRMICO', color: 0xff6b8a, rating: 5, base: 0.05, pri: 2, routes: [[[-1.5, 1.0, -4.6], [-1.3, -0.16, -4.6], [0.55, -0.16, -4.6], [0.55, -0.16, -9.2]]] },
  {
    id: 'prop', desc: 'Motores principales, APU, RCS, bombas de refuerzo y transferencia de propelente.',
    label: 'PROPULSIÓN',
    color: 0xff7a3a,
    rating: 12,
    base: 0.2,
    pri: 0,
    routes: [
      [[-1.5, 1.0, -4.0], [-1.3, -0.16, -4.0], [-0.9, -0.16, -4.0], [-0.9, -0.16, 1.5], [-2.5, -0.16, 1.5], [-2.5, 1.1, 1.5]],
      [[-0.9, -0.16, 1.5], [2.5, -0.16, 1.5], [2.5, 1.1, 1.5]],
    ],
  },
  { id: 'life', desc: 'Generador de O₂, depurador, ventiladores, calefacción, válvulas de gas y umbilicales de los asientos.', label: 'SOPORTE VITAL', color: 0x5cf2d2, rating: 14, base: 0.3, pri: 0, routes: [[[-1.5, 1.0, -3.8], [-1.3, -0.16, -3.8], [1.2, -0.16, -3.8], [1.2, -0.16, -1.5], [1.85, 0.2, -1.5]]] },
  { id: 'cool', desc: 'Bomba de refrigerante, radiadores y arranque del reactor.', label: 'REFRIGERACIÓN', color: 0x4f9dff, rating: 12, base: 0.1, pri: 0, routes: [[[-1.5, 1.0, -3.6], [-1.4, -0.16, -3.6], [-1.4, -0.16, -1.2], [-1.75, 0.2, -1.2]]] },
  { id: 'sensors', desc: 'Radar.', label: 'SENSORES', color: 0xe0e070, rating: 5, base: 0.2, pri: 1, routes: [[[-1.5, 1.6, -4.7], [-1.25, 2.2, -4.7], [-0.2, 2.33, -4.7], [-0.2, 2.33, -7.2], [0, 2.45, -7.2]]] },
  { id: 'weapons', desc: 'Torreta y cargador de misiles.', label: 'ARMAMENTO', color: 0xff4f6a, rating: 6, base: 0.1, pri: 2, routes: [[[-1.5, 1.6, -4.9], [-1.25, 2.2, -4.9], [0.2, 2.33, -4.9], [0.2, 2.33, -2.6], [0.2, 2.93, -2.3], [0, 2.95, -1.95]]] },
  { id: 'grav', desc: 'Compensador inercial.', label: 'COMPENSADOR INERCIAL', color: 0xc07fff, rating: 9, base: 0.1, pri: 1, routes: [[[-1.5, 1.6, -5.1], [-1.25, 2.2, -5.1], [0.5, 2.33, -5.1], [0.5, 2.33, -2.6], [0.5, 2.93, -2.3], [0.5, 2.93, 0.8]]] },
];
const SUBSYSTEMS: SubsystemDef[] = CIRCUITS.map(({ pri, ...c }) => ({ ...c, breaker: `brk.${c.id}`, priority: `pri.${c.id}` }));
const SHORT: Record<string, string> = {
  lights: 'ILUMIN.',
  ext: 'EXTERIOR',
  doors: 'PUERTAS',
  hyd: 'HIDRÁUL.',
  avionics: 'AVIÓNICA',
  shield: 'ESCUDO',
  prop: 'PROPULS.',
  life: 'SOP.VITAL',
  cool: 'REFRIG.',
  sensors: 'SENSORES',
  weapons: 'ARMAS',
  grav: 'GRAVEDAD',
};

// -----------------------------------------------------------------------------------------------
// Machinery (breakable, repairable parts) — positions in ship space
// -----------------------------------------------------------------------------------------------

const onRoof = (zone: string, x: number, z: number, rise: number): V3 => outsidePoint(roofAt(MODULES.find((m) => m.zone === zone)!, x, z), T, rise);

// Every machine is a catalog component (shared/ship/catalog): this list only says where it goes,
// what it is called on board and what it is wired to.
const PARTS: PartSpec[] = [
  // engineering corner of the cargo bay: reactor, its coolant pump
  part('reactor.fission.M', { id: 'reactor', c: [-1.75, 1.05, -1.25], zone: 'cargo', circuit: 'cool', tag: 'rx' }),
  part('coolpump.M', { id: 'coolpump', c: [-2.2, 0.32, -0.2], zone: 'cargo', circuit: 'cool' }),
  // life support rack (starboard) and the gas bottles behind it
  part('o2gen.M', { id: 'o2gen', c: [1.95, 0.5, -1.6], zone: 'cargo', circuit: 'life' }),
  part('scrubber.M', { id: 'scrubber', c: [1.95, 1.48, -1.6], zone: 'cargo', circuit: 'life', sw: { run: 'scrub' } }),
  part('gas.o2.M', { id: 'gas.O2', c: [2.3, 0.8, -0.35], zone: 'cargo', sw: { valve: 'v.gasO2' } }),
  part('gas.n2.M', { id: 'gas.N2', c: [2.3, 0.8, 0.28], zone: 'cargo', sw: { valve: 'v.gasN2' } }),
  part('grav.M', { id: 'grav', c: [0, 2.78, 0.8], zone: 'cargo', circuit: 'grav' }),
  part('loader.S', { id: 'loader', c: [0.95, 1.1, -2.26], zone: 'cargo', circuit: 'weapons' }),
  part('battery.M', { id: 'battery', c: [-1.36, 0.62, -5.6], zone: 'corridor', sw: { on: 'bat' } }),
  // outside: nacelles = propellant tank (front) + engine (rear), belly reserve tank, APU pod
  part('tank.cyl.M', { id: 'tank.L', name: 'Depósito izquierdo', c: [-3.45, 1.15, 0.05], zone: null, mount: 'nacelle.L', p: { fill: 1000 } }),
  part('tank.cyl.M', { id: 'tank.R', name: 'Depósito derecho', c: [3.45, 1.15, 0.05], zone: null, mount: 'nacelle.R', p: { fill: 1000 } }),
  part('tank.cyl.S', { id: 'tank.C', name: 'Depósito de reserva', c: [0, -0.64, 2.0], zone: null }),
  // VTOL: the nacelle lever ducts each engine's exhaust to a belly nozzle under the tank, beside the
  // centre of mass, so full thrust downward does not tip the ship
  part('engine.main.M', { id: 'eng.L', name: 'Motor izquierdo', c: [-3.45, 1.15, 4.75], zone: null, circuit: 'prop', feed: 'eng.L', mount: 'nacelle.L', lamp: 'MOTOR IZQ', gimbal: { key: 'nacelle', rad: Math.PI / 2, at: [-3.45, 0.36, -0.6] } }),
  part('engine.main.M', { id: 'eng.R', name: 'Motor derecho', c: [3.45, 1.15, 4.75], zone: null, circuit: 'prop', feed: 'eng.R', mount: 'nacelle.R', lamp: 'MOTOR DER', gimbal: { key: 'nacelle', rad: Math.PI / 2, at: [3.45, 0.36, -0.6] } }),
  // VTOL lift pads under the keel, around the centre of mass (front pair under the corridor, rear
  // pair under the bay): hover, climb and fine attitude without the main engines
  part('lift.M', { id: 'lift.FL', name: 'Sustentación delantera izq.', c: [-1.2, -0.62, -4.7], zone: null, circuit: 'prop', feed: 'lift.FL' }),
  part('lift.M', { id: 'lift.FR', name: 'Sustentación delantera der.', c: [1.2, -0.62, -4.7], zone: null, circuit: 'prop', feed: 'lift.FR' }),
  part('lift.M', { id: 'lift.RL', name: 'Sustentación trasera izq.', c: [-1.5, -0.62, 3.54], zone: null, circuit: 'prop', feed: 'lift.RL' }),
  part('lift.M', { id: 'lift.RR', name: 'Sustentación trasera der.', c: [1.5, -0.62, 3.54], zone: null, circuit: 'prop', feed: 'lift.RR' }),
  part('apu.M', { id: 'apu', c: [2.78, 0.76, 3.2], zone: null, circuit: 'prop', feed: 'apu', lamp: 'APU' }),
  part('rcs.M', { id: 'rcs.FL', name: 'RCS delantero izq.', c: [-1.72, 1.3, -8.7], zone: null, circuit: 'prop', feed: 'rcs.FL' }),
  part('rcs.M', { id: 'rcs.FR', name: 'RCS delantero der.', c: [1.72, 1.3, -8.7], zone: null, circuit: 'prop', feed: 'rcs.FR' }),
  part('rcs.M', { id: 'rcs.RL', name: 'RCS trasero izq.', c: [-2.78, 1.45, 4.62], half: [0.14, 0.12, 0.2], zone: null, circuit: 'prop', feed: 'rcs.RL' }),
  part('rcs.M', { id: 'rcs.RR', name: 'RCS trasero der.', c: [2.78, 1.45, 4.62], half: [0.14, 0.12, 0.2], zone: null, circuit: 'prop', feed: 'rcs.RR' }),
  // dorsal: radiator wings either side of the spine, turret, sensors
  part('radiator.wing.M', { id: 'rad.L', name: 'Radiador izquierdo', c: [-0.62, 3.22, 1.25], zone: null, circuit: 'cool', sw: { deploy: 'rad' } }),
  part('radiator.wing.M', { id: 'rad.R', name: 'Radiador derecho', c: [0.62, 3.22, 1.25], zone: null, circuit: 'cool', sw: { deploy: 'rad' } }),
  part('turret.M', { id: 'turret', c: onRoof('cargo', 0, -1.95, 0.32), zone: null, circuit: 'weapons' }),
  part('radar.dome.S', { id: 'radar', c: onRoof('cockpit', 0, -7.2, 0.14), zone: null, circuit: 'sensors' }),
  part('antenna.S', { id: 'antenna', c: onRoof('corridor', 0.7, -4.4, 0.3), zone: null, circuit: 'avionics' }),
];

const FLUID: FluidNetDef = {
  manifolds: ['mL', 'mC', 'mR', 'mRCS'],
  pipes: [
    { a: 'tank.L', b: 'mL', valve: 'v.tank.L' },
    { a: 'tank.C', b: 'mC', valve: 'v.tank.C' },
    { a: 'tank.R', b: 'mR', valve: 'v.tank.R' },
    { a: 'mL', b: 'mC', valve: 'v.xfeed.L' },
    { a: 'mR', b: 'mC', valve: 'v.xfeed.R' },
    { a: 'mL', b: 'eng.L', valve: 'v.eng.L' },
    { a: 'mR', b: 'eng.R', valve: 'v.eng.R' },
    { a: 'mC', b: 'apu', valve: 'v.apu' },
    { a: 'mC', b: 'mRCS', valve: 'v.rcs' },
    { a: 'mRCS', b: 'rcs.FL' },
    { a: 'mRCS', b: 'rcs.FR' },
    { a: 'mRCS', b: 'rcs.RL' },
    { a: 'mRCS', b: 'rcs.RR' },
    // lift pads drink from their own side's manifold
    { a: 'mL', b: 'lift.FL', valve: 'v.lift' },
    { a: 'mL', b: 'lift.RL', valve: 'v.lift' },
    { a: 'mR', b: 'lift.FR', valve: 'v.lift' },
    { a: 'mR', b: 'lift.RR', valve: 'v.lift' },
  ],
  circuit: 'prop',
  transfer: { key: 'xfer', modes: [null, ['tank.C', 'tank.L'], ['tank.C', 'tank.R'], ['tank.L', 'tank.R'], ['tank.R', 'tank.L']] },
  refuel: { key: 'refuel' },
  balance: [{ a: 'tank.L', b: 'tank.R', kg: 350 }],
};

const COMPARTMENTS: CompartmentDef[] = [
  { id: 'cockpit', label: 'CABINA', volume: 17 },
  { id: 'corridor', label: 'PASILLO', volume: 26 },
  { id: 'cargo', label: 'BODEGA', volume: 105 },
];

const OPENINGS: OpeningDef[] = [
  { id: 'door.cockpit', kind: 'door', a: 'cockpit', b: 'corridor', key: 'door.cockpit', area: 2.46 },
  { id: 'door.cargo', kind: 'door', a: 'corridor', b: 'cargo', key: 'door.cargo', area: 2.46 },
  { id: 'ramp', kind: 'ramp', a: 'cargo', b: null, key: 'ramp', area: 9.2 },
  { id: 'vent.cockpit', kind: 'vent', a: 'cockpit', b: null, key: 'vent.cockpit', area: 0.004 },
  { id: 'vent.corridor', kind: 'vent', a: 'corridor', b: null, key: 'vent.corridor', area: 0.005 },
  { id: 'vent.cargo', kind: 'vent', a: 'cargo', b: null, key: 'vent.cargo', area: 0.012 },
  { id: 'duct.cockpit', kind: 'duct', a: 'cockpit', b: 'corridor', key: 'duct.cockpit', area: 0.02 },
  { id: 'duct.cargo', kind: 'duct', a: 'corridor', b: 'cargo', key: 'duct.cargo', area: 0.02 },
];

const DOORS: DoorDef[] = [
  { key: 'door.cockpit', c: [0, 0, Z_CK], n: [0, 0, 1], w: 1.2, h: 2.05, offset: 0.075 },
  { key: 'door.cargo', c: [0, 0, Z_CR], n: [0, 0, 1], w: 1.2, h: 2.05, offset: -0.075 },
];
const RAMP: RampDef = {
  key: 'ramp',
  hinge: [0, 0, Z_TAIL],
  w: 3.3,
  length: 2.8,
  t: 0.12,
  pistons: [-1, 1].map((sx) => ({ hull: [sx * 1.58, 1.95, Z_TAIL - 0.12] as V3, ramp: [sx * 1.52, 1.0, 0] as V3 })),
};

const MOVERS: MoverDef[] = [
  ...DOORS.map((d): MoverDef => ({ key: d.key, rate: 1 / 0.9, circuit: 'doors', load: 1.5, sensor: doorSensor(d) })),
  { key: RAMP.key, rate: 1 / 4.5, circuit: 'hyd', load: 3.5, sensor: rampSensor(RAMP) },
  { key: 'shield', rate: 1 / 2.4, circuit: 'shield', load: 2 },
  { key: 'gear', rate: 1 / 6, circuit: 'hyd', load: 3 },
  { key: 'nacelle', rate: 1 / 3.5, circuit: 'hyd', load: 2.5 },
  { key: 'rad', rate: 1 / 5, circuit: 'cool', load: 0.8 },
];

const ZONE_LIGHTS: Array<[string, string]> = [
  ['cockpit', 'light.cockpit'],
  ['corridor', 'light.corridor'],
  ['cargo', 'light.cargo'],
];

// Switched loads: what each switch draws from its circuit while on (per position for selectors).
const LOADS: LoadDef[] = [
  ...ZONE_LIGHTS.map(([, key]): LoadDef => ({ key, circuit: 'lights', kw: 0.35 })),
  { key: 'light.nav', circuit: 'ext', kw: 0.15 },
  { key: 'light.beacon', circuit: 'ext', kw: 0.1 },
  { key: 'light.landing', circuit: 'ext', kw: 2 },
  { key: 'rcs', circuit: 'prop', kw: 0.4 },
  { key: 'lift', circuit: 'prop', kw: 0.8 },
  // machines draw what their catalog component says
  loadOf(PARTS, 'coolpump'),
  loadOf(PARTS, 'o2gen'),
  loadOf(PARTS, 'scrubber', 'scrub'),
  { key: 'fans', circuit: 'life', kw: 1 },
  { key: 'ls.recover', circuit: 'life', kw: 5 },
  loadOf(PARTS, 'radar', 'radar.mode'),
  loadOf(PARTS, 'turret'),
  loadOf(PARTS, 'grav'),
];

const LIFE: LifeSupportDef = { circuit: 'life', mode: 'ls.mode', fans: 'fans', heat: 'heat', heatKw: 0.6, repress: 'repress.', recover: { key: 'ls.recover', zone: 'cargo' } };

// -----------------------------------------------------------------------------------------------
// Consoles and their controls
// -----------------------------------------------------------------------------------------------

// Help for every control, by switch key (helmet readout + manual). Controls that share a key share it.
const HELP: Record<string, string> = {
  ramp: 'Baja o sube la rampa trasera de la bodega (hidráulica, 4–5 s). No baja con la bodega presurizada: ventéala o recupera el aire primero. Si alguien está en su recorrido, se para al cerrar.',
  shield: 'Baja las persianas blindadas sobre todas las ventanas de la cabina (protección térmica y de impactos). Con ellas echadas no se ve fuera.',
  'door.cockpit': 'Abre o cierra la puerta entre cabina y pasillo. No abre con más de 5 kPa de diferencia entre los dos lados.',
  'door.cargo': 'Abre o cierra la puerta entre pasillo y bodega. No abre con más de 5 kPa de diferencia entre los dos lados.',
  gear: 'Sube o baja el tren de aterrizaje. Con peso sobre las ruedas no se deja subir. Las tres luces verdes del tablero indican abajo y blocado.',
  caution: 'Se enciende con cualquier alarma nueva. Púlsala para reconocerla: se apaga, pero la alarma sigue en el tablero y en la página ALARM hasta que desaparezca la causa.',
  'eng.L.arm': 'Arma el motor izquierdo: da tensión al encendido y a su control. Sin armar no arranca. Desarmarlo apaga el motor y, con la alimentación cerrada, lo deja seguro si está dañado.',
  'eng.R.arm': 'Arma el motor derecho: da tensión al encendido y a su control. Sin armar no arranca. Desarmarlo apaga el motor y, con la alimentación cerrada, lo deja seguro si está dañado.',
  'eng.L.start': 'Arranca el motor izquierdo (unos 3 s hasta régimen). Necesita el motor armado, el circuito PROPULSIÓN con energía y propelente llegando por su válvula. Un motor muy dañado puede explotar al encenderse.',
  'eng.R.start': 'Arranca el motor derecho (unos 3 s hasta régimen). Necesita el motor armado, el circuito PROPULSIÓN con energía y propelente llegando por su válvula. Un motor muy dañado puede explotar al encenderse.',
  'helm.throttle': 'Acelerador de los dos motores principales. En vuelo ACOPLADO y con el piloto automático es lo más que el ordenador de vuelo puede usar de ellos para alcanzar la velocidad pedida (W/S, o VEL / NAV del piloto automático; 0 % = solo sustentación y RCS). DESACOPLADO es su empuje directo (control manual). En tierra y ACOPLADO no empujan: despega primero. El casco muestra MOTOR: lo que empujan y por qué. Los motores tienen que estar armados y en marcha. Gasta propelente en proporción.',
  nacelle: 'Góndolas: CRUCERO empuja hacia la proa; VTOL desvía el chorro de los motores a las toberas ventrales, junto al centro de masas, para levantar cargas pesadas o subir deprisa (el ordenador de vuelo los usa como sustentación). Hidráulica, unos 3 s.',
  rcs: 'Activa los propulsores de maniobra (cuatro bloques de toberas en los seis ejes, 0,4 kW): giros finos y desplazamientos laterales. Beben propelente del colector central a través de su válvula AL.RCS.',
  lift: 'Activa los cuatro propulsores de sustentación VTOL bajo la quilla (12 kN cada uno, 0,8 kW): son los que mantienen la nave en el aire sin los motores principales. Beben de los colectores laterales por la válvula AL.VTOL.',
  'v.lift': 'Válvula de alimentación de los cuatro propulsores de sustentación desde los colectores izquierdo y derecho. Cerrada, la nave no puede quedarse en el aire con ellos.',
  'fa.sas': 'Estabilizador: con los mandos de giro sueltos, frena los giros y mantiene la actitud (y el rumbo). Sin él, las teclas de giro dan par directo y la nave sigue girando por inercia.',
  'fa.hold': 'ACOPLADO: los mandos de traslación piden una velocidad (adelante, lateral, subir) y al soltarlos la nave se para y mantiene la altura. DESACOPLADO: piden aceleración y la nave sigue en inercia; el ordenador solo compensa la gravedad.',
  'fa.land': 'Asistente de aterrizaje: cerca del suelo limita la velocidad de bajada según la altura y, con el tren abajo, nivela la nave para posarla.',
  'light.landing': 'Focos bajo el morro que iluminan el suelo delante de la nave (2 kW del circuito LUCES EXTERIORES).',
  'v.tank.L': 'Une el depósito izquierdo a su colector. Cerrada lo aísla: no alimenta nada, no recibe transferencia ni repostaje. Ciérrala si el depósito tiene una fuga.',
  'v.tank.R': 'Une el depósito derecho a su colector. Cerrada lo aísla: no alimenta nada, no recibe transferencia ni repostaje. Ciérrala si el depósito tiene una fuga.',
  'v.tank.C': 'Une el depósito de reserva al colector central, del que beben la APU y los RCS. Viene abierta: sin ella los RCS se quedan secos y la nave pierde los giros finos.',
  'pump.tank.L': 'Bomba de refuerzo del depósito izquierdo: sube el caudal que puede dar de 1,2 a 6 kg/s. Sin ella un motor a plena potencia se queda sin propelente. 0,8 kW de PROPULSIÓN.',
  'pump.tank.R': 'Bomba de refuerzo del depósito derecho: sube el caudal que puede dar de 1,2 a 6 kg/s. Sin ella un motor a plena potencia se queda sin propelente. 0,8 kW de PROPULSIÓN.',
  'pump.tank.C': 'Bomba de refuerzo del depósito de reserva: sube el caudal que puede dar de 1,2 a 6 kg/s. 0,8 kW de PROPULSIÓN.',
  'v.xfeed.L': 'Une el colector izquierdo con el central. Abierta, el lado izquierdo puede beber de la reserva o del otro lado (y la APU del depósito izquierdo).',
  'v.xfeed.R': 'Une el colector derecho con el central. Abierta, el lado derecho puede beber de la reserva o del otro lado (y la APU del depósito derecho).',
  'v.eng.L': 'Válvula de alimentación del motor izquierdo. Cerrarla le corta el propelente (se apaga) y es la forma de aislar un motor dañado que puede explotar.',
  'v.eng.R': 'Válvula de alimentación del motor derecho. Cerrarla le corta el propelente (se apaga) y es la forma de aislar un motor dañado que puede explotar.',
  'v.apu': 'Válvula de alimentación de la APU desde el colector central. Sin ella la APU no arranca.',
  'v.rcs': 'Válvula de alimentación de los cuatro RCS desde el colector central.',
  xfer: 'Bomba de transferencia: pasa propelente de un depósito a otro a 3 kg/s (CEN→IZQ = de la reserva al izquierdo…). Necesita las válvulas de salida de los dos depósitos abiertas y energía en PROPULSIÓN. Clic o rueda para girarlo.',
  'radar.mode': 'PASIVO solo escucha emisiones (0,5 kW). ACTIVO barre con su propia señal (3 kW): ve más, pero delata la nave. El radar aún no muestra contactos.',
  'radar.range': 'Escala del radar, de 250 m a 100 km. Más alcance, menos detalle. Aún sin contactos que mostrar.',
  'radar.lock': 'Fija el siguiente contacto del radar como blanco. Aún sin contactos.',
  'light.cockpit': 'Luces de techo de la cabina. Sin energía en ILUMINACIÓN se encienden las de emergencia, rojas.',
  'light.corridor': 'Luces de techo del pasillo. Sin energía en ILUMINACIÓN se encienden las de emergencia, rojas.',
  'light.cargo': 'Luces de techo de la bodega. Sin energía en ILUMINACIÓN se encienden las de emergencia, rojas.',
  'light.nav': 'Luces de posición (roja a babor, verde a estribor) y el estrobo blanco de la aleta.',
  'light.beacon': 'Baliza roja giratoria en el lomo y en la panza: avisa de que la nave está activa.',
  bat: 'Conecta el banco de baterías (12 kWh) a la red. Cubre lo que los generadores no dan (hasta 30 kW) y se carga con lo que sobra. Hace falta para arrancar el reactor o la APU con todo parado.',
  apu: 'Turbina auxiliar de 15 kW que quema propelente. Tarda unos 8 s en arrancar y necesita energía de arranque (batería o reactor), el circuito PROPULSIÓN y propelente en el colector central. Si falla, apágala y vuelve a encenderla.',
  rad: 'Despliega las alas de los radiadores sobre el lomo: de 8 a 28 m² para echar el calor del reactor al espacio. A potencias altas hace falta tenerlos fuera o el refrigerante se calienta.',
  grav: 'Compensador inercial (6 kW): a bordo el suelo sigue siendo «abajo» aunque la nave se incline, frene o acelere, así que la carga y la tripulación no resbalan. Apagado o sin energía se nota todo; en caída libre, se flota.',
  heat: 'Calefacción de los compartimentos presurizados (0,6 kW cada uno). Sin ella el aire se enfría poco a poco hacia −18 °C.',
  turret: 'Da energía a la torreta de minimisiles del lomo (1,5 kW de ARMAMENTO). El copiloto dispone de una consola con cámara: G/APUNTAR dirige la torreta con el ratón, T/DISPARO dispara; ZOOM amplía y FIJAR mantiene la mira sobre un punto del suelo. El cargador se repone mientras tiene energía.',
  'ls.mode': 'AUTOMÁTICO mantiene cada compartimento estanco a 70 kPa con gas de las botellas (se suspende si hay fuga). MANUAL solo mete gas donde abras la válvula de represurización. APAGADO no repone nada.',
  o2gen: 'Generador de oxígeno por electrólisis (4 kW) en la bodega. Repone el O₂ que respira la tripulación; los ventiladores lo reparten al resto de la nave.',
  scrub: 'Depurador de CO₂ (1,5 kW) en la bodega. Retira el CO₂ que exhala la tripulación; necesita los ventiladores para limpiar el aire de la cabina y el pasillo.',
  fans: 'Ventiladores (1 kW): mueven el aire entre compartimentos por los conductos con la compuerta abierta. Sin ellos el O₂ y el depurador de la bodega no llegan a la cabina.',
  'repress.cockpit': 'En modo MANUAL, abre las botellas de O₂/N₂ directamente a la cabina, aunque tenga una fuga (decisión tuya).',
  'repress.corridor': 'En modo MANUAL, abre las botellas de O₂/N₂ directamente al pasillo, aunque tenga una fuga (decisión tuya).',
  'repress.cargo': 'En modo MANUAL, abre las botellas de O₂/N₂ directamente a la bodega, aunque tenga una fuga (decisión tuya).',
  'vent.cockpit': 'Válvula de venteo: vacía el aire de la cabina al espacio, despacio. Bajo tapa porque tira el aire; para ahorrarlo usa el compresor.',
  'vent.corridor': 'Válvula de venteo: vacía el aire del pasillo al espacio, despacio. Bajo tapa porque tira el aire.',
  'vent.cargo': 'Válvula de venteo: vacía el aire de la bodega al espacio para poder bajar la rampa. Bajo tapa. Para no perder el aire usa antes el compresor.',
  'duct.cockpit': 'Compuerta del conducto de ventilación entre cabina y pasillo. Cerrada aísla la cabina (por ejemplo, de una brecha en el pasillo), pero corta el aire de los ventiladores.',
  'duct.cargo': 'Compuerta del conducto de ventilación entre pasillo y bodega. Cerrada aísla la bodega, pero corta el aire del generador de O₂ y del depurador al resto.',
  'ls.recover': 'Compresor (5 kW): bombea el aire de la bodega de vuelta a las botellas en vez de tirarlo. Úsalo antes de bajar la rampa y ciérralo cuando la bodega esté a 0.',
  reactor: 'Palanca del reactor. Arriba inicia el arranque (unos 15 s): exige la bomba de refrigerante en marcha y energía de arranque (batería o APU). Abajo baja la potencia hasta pararlo.',
  'rx.set': 'Potencia pedida al reactor, de 0 a 110 % de 60 kW. Por encima de 100 % es sobrecarga: calienta más deprisa. Un reactor dañado no llega a lo pedido.',
  'rx.scram': 'Parada de emergencia: mete las barras y el reactor se para al instante. Bajo tapa. También salta solo a 750 °C. Para volver a arrancar hace falta REARME.',
  coolpump: 'Bomba del circuito de refrigeración (2,5 kW): lleva el calor del núcleo a los radiadores. Sin ella el reactor se calienta hasta el SCRAM y no se deja arrancar.',
  'rx.reset': 'Quita el SCRAM cuando el núcleo ha bajado de 300 °C; la palanca vuelve a PARADO y ya se puede arrancar de nuevo.',
  refuel: 'Conecta la manguera de la plataforma de la base: llena a 10 kg/s los depósitos con la válvula de salida abierta. Solo funciona aterrizado en la plataforma.',
  'v.gasO2': 'Válvula de la botella de O₂ (80 kg). Cerrada no hay represurización con oxígeno ni O₂ para los trajes acoplados a los asientos.',
  'v.gasN2': 'Válvula de las botellas de N₂ (200 kg), el gas de relleno del aire. Cerrada, la represurización solo mete oxígeno.',
};

const ON_OFF = ['APAGADO', 'ENCENDIDO'];
const OPEN = ['CERRADA', 'ABIERTA'];
const VALVE = ['CERRADA', 'ABIERTA'];
const BRK = ['ABIERTO', 'CERRADO'];
const ARMED = ['DESARMADO', 'ARMADO'];
const tilt = (deg: number): V3 => [0, Math.cos((deg * Math.PI) / 180), Math.sin((deg * Math.PI) / 180)];
const tank = (id: string, side: string, at: V2): ControlSpec => ({ key: `v.${id}`, kind: 'toggle', label: side, name: `Válvula de salida · ${PART_NAME(id)}`, states: VALVE, at });
const pump = (id: string, side: string, at: V2): ControlSpec => ({ key: `pump.${id}`, kind: 'toggle', label: side, name: `Bomba de refuerzo · ${PART_NAME(id)}`, states: ON_OFF, at, requires: 'prop' });
function PART_NAME(id: string) {
  return PARTS.find((p) => p.id === id)?.name ?? id;
}

// autopilot board on the glareshield, above the dash between the two displays
const AP = autopilotConsole({ frame: boardFrame([0, 1.19, -9.37], norm([0, 0.35, 1])), circuit: 'avionics', w: 1.0, h: 0.22 });

const CONSOLES: ConsoleSpec[] = [
  ...AP.consoles,
  {
    id: 'ck.main',
    title: 'CONTROL DE VUELO',
    frame: boardFrame([0, 0.9, -9.25], tilt(40), [0, 0.2, -1]),
    w: 2.8,
    h: 0.6,
    depth: 0.12,
    free: true,
    screens: [
      { id: 'mfd.pilot', pages: ['flight', 'ap', 'nav', 'mass', 'engines', 'fuel', 'alerts'], at: [-0.56, 0.04], w: 0.54, h: 0.32 },
      { id: 'mfd.copilot', pages: ['status', 'power', 'fuel', 'atmos', 'hull', 'alerts'], at: [0.56, 0.04], w: 0.54, h: 0.32 },
    ],
    indicators: [{ kind: 'gear-greens', at: [1.2, 0.22], ref: 'gear' }],
    controls: [
      { key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga', states: ['CERRADA', 'BAJADA'], at: [-1.22, 0.12], requires: 'hyd' },
      { key: 'shield', kind: 'toggle', label: 'ESCUDO', name: 'Escudo térmico (persianas)', states: ['RETRAÍDO', 'DESPLEGADO'], at: [-1.0, 0.12], requires: 'shield' },
      { key: 'door.cockpit', kind: 'button', label: 'P. CABINA', name: 'Puerta de cabina', states: OPEN, at: [-1.22, -0.12], requires: 'doors' },
      { key: 'door.cargo', kind: 'button', label: 'P. BODEGA', name: 'Puerta de bodega', states: OPEN, at: [-1.0, -0.12], requires: 'doors' },
      { key: 'gear', kind: 'lever', label: 'TREN', name: 'Tren de aterrizaje', states: ['ARRIBA', 'ABAJO Y BLOCADO'], at: [1.2, 0.02], requires: 'hyd' },
      { key: 'helm.throttle', kind: 'rotary', label: 'ACELER.', name: 'Acelerador de los motores', states: Array.from({ length: 11 }, (_, i) => `${i * 10} %`), at: [0.98, -0.12], action: 'cycle', wrap: false },
      { key: 'caution', kind: 'master', label: 'ALARMA', name: 'Alarma general (reconocer)', states: ['SIN AVISOS', 'AVISO ACTIVO'], at: [0, -0.2], action: 'reset' },
    ],
  },
  {
    id: 'ck.gun',
    title: 'ARTILLERÍA · CÁMARA',
    frame: boardFrame([0.83, 1.46, -8.98], norm([0, 0.12, 1])),
    w: 0.78,
    h: 0.52,
    depth: 0.045,
    free: true,
    screens: [{ id: 'camera.turret', camera: { source: { kind: 'mount', ref: 'turret' }, width: 384, height: 216, fps: 12, zoom: [1, 1.5, 3], effects: [{ kind: 'vhs', amount: 0.2 }] }, seat: 'ck.copilot', at: [-0.02, 0.045], w: 0.56, h: 0.315, circuit: 'avionics' }],
    controls: [
      { key: 'turret', kind: 'toggle', label: 'ON', name: 'Alimentación de la torreta', help: HELP.turret, states: ON_OFF, at: [0.33, 0.045], requires: 'weapons' },
      ...([
        ['aim', 'APUNTAR', 'Apuntar con la cámara', 'G alterna entre dirigir la torreta con el ratón y mirar la cabina. En APUNTAR, clic y T disparan.'],
        ['lock', 'FIJAR', 'Fijar o liberar un punto', 'Mantiene la mira sobre el punto de la superficie bajo la cruz de la cámara. No sigue objetivos móviles ni guía los misiles. Pulsa otra vez para liberar.'],
        ['zoom', 'ZOOM', 'Zoom de la cámara', 'Alterna la cámara entre 1×, 1,5× y 3×. Reduce la velocidad del ratón para apuntar con precisión.'],
        ['center', 'CENTRO', 'Centrar la torreta', 'Libera el punto fijado y devuelve la torreta al centro de su recorrido.'],
        ['fire', 'DISPARO', 'Disparar un minimisil', 'Dispara desde la torreta. También T, o clic en modo APUNTAR. Requiere copiloto sentado, torreta encendida, energía, munición y cadencia cumplida.'],
      ] as const).map(([action, label, name, help], i): ControlSpec => ({ key: `gun.${action}`, kind: 'bezel', label, name, help, at: [-0.29 + i * 0.145, -0.205], action: 'pulse', command: { namespace: 'gunnery', target: 'turret', action } })),
    ],
  },
  {
    id: 'ck.pl',
    title: 'PILOTO · PROPULSIÓN',
    frame: boardFrame([-1.44, 0.8, -8.05], norm([1, 1.25, 0]), [0, 0, -1]),
    w: 0.4,
    h: 0.95,
    depth: 0.1,
    controls: [
      { key: 'eng.L.arm', kind: 'toggle', label: 'ARM IZQ', name: 'Motor izquierdo · armado', states: ARMED, at: [-0.1, 0.34], guard: 'guard.eng.L' },
      { key: 'eng.L.start', kind: 'button', label: 'ARRANQUE', name: 'Motor izquierdo · arranque', states: ['', 'ARRANCANDO'], at: [0.08, 0.34], action: 'pulse' },
      { key: 'eng.R.arm', kind: 'toggle', label: 'ARM DER', name: 'Motor derecho · armado', states: ARMED, at: [-0.1, 0.16], guard: 'guard.eng.R' },
      { key: 'eng.R.start', kind: 'button', label: 'ARRANQUE', name: 'Motor derecho · arranque', states: ['', 'ARRANCANDO'], at: [0.08, 0.16], action: 'pulse' },
      { key: 'nacelle', kind: 'lever', label: 'GÓNDOLAS', name: 'Góndolas (crucero / VTOL)', states: ['CRUCERO', 'VTOL'], at: [-0.08, -0.03], requires: 'hyd' },
      { key: 'rcs', kind: 'toggle', label: 'RCS', name: 'Propulsores de maniobra (RCS)', states: ON_OFF, at: [0.1, -0.03] },
      { key: 'fa.sas', kind: 'toggle', label: 'ESTAB.', name: 'Estabilizador (amortigua giros)', states: ON_OFF, at: [-0.12, -0.22] },
      { key: 'fa.hold', kind: 'toggle', label: 'ACOPLADO', name: 'Vuelo acoplado (mantiene velocidad)', states: ['DESACOPLADO', 'ACOPLADO'], at: [0, -0.22] },
      { key: 'fa.land', kind: 'toggle', label: 'ATERRIZ.', name: 'Asistente de aterrizaje', states: ON_OFF, at: [0.12, -0.22] },
      { key: 'light.landing', kind: 'toggle', label: 'FOCOS', name: 'Focos de aterrizaje', states: ON_OFF, at: [-0.1, -0.38] },
      { key: 'lift', kind: 'toggle', label: 'VTOL', name: 'Propulsores de sustentación (VTOL)', states: ON_OFF, at: [0.1, -0.38] },
    ],
  },
  {
    id: 'ck.cp',
    title: 'COPILOTO · PROPELENTE',
    frame: boardFrame([1.44, 0.8, -8.05], norm([-1, 1.25, 0]), [0, 0, -1]),
    w: 0.4,
    h: 0.95,
    depth: 0.1,
    controls: [
      tank('tank.L', 'V.IZQ', [-0.12, 0.37]),
      tank('tank.C', 'V.CEN', [0, 0.37]),
      tank('tank.R', 'V.DER', [0.12, 0.37]),
      pump('tank.L', 'B.IZQ', [-0.12, 0.23]),
      pump('tank.C', 'B.CEN', [0, 0.23]),
      pump('tank.R', 'B.DER', [0.12, 0.23]),
      { key: 'v.xfeed.L', kind: 'toggle', label: 'XF IZQ', name: 'Alimentación cruzada izquierda', states: VALVE, at: [-0.08, 0.09] },
      { key: 'v.xfeed.R', kind: 'toggle', label: 'XF DER', name: 'Alimentación cruzada derecha', states: VALVE, at: [0.08, 0.09] },
      { key: 'v.lift', kind: 'toggle', label: 'AL.VTOL', name: 'Alimentación de la sustentación VTOL', states: VALVE, at: [0, 0.09] },
      { key: 'v.eng.L', kind: 'toggle', label: 'AL.M.IZQ', name: 'Alimentación del motor izquierdo', states: VALVE, at: [-0.08, -0.05] },
      { key: 'v.eng.R', kind: 'toggle', label: 'AL.M.DER', name: 'Alimentación del motor derecho', states: VALVE, at: [0.08, -0.05] },
      { key: 'v.apu', kind: 'toggle', label: 'AL.APU', name: 'Alimentación de la APU', states: VALVE, at: [-0.12, -0.19] },
      { key: 'v.rcs', kind: 'toggle', label: 'AL.RCS', name: 'Alimentación de los RCS', states: VALVE, at: [0, -0.19] },
      { key: 'xfer', kind: 'rotary', label: 'TRANSF.', name: 'Transferencia de propelente', states: ['PARADA', 'CEN→IZQ', 'CEN→DER', 'IZQ→DER', 'DER→IZQ'], at: [0.12, -0.19], action: 'cycle', requires: 'prop' },
      { key: 'radar.mode', kind: 'rotary', label: 'RADAR', name: 'Radar · modo', states: ['APAGADO', 'PASIVO', 'ACTIVO'], at: [-0.12, -0.36], action: 'cycle' },
      { key: 'radar.range', kind: 'rotary', label: 'ALCANCE', name: 'Radar · alcance', states: ['250 m', '1 km', '5 km', '20 km', '100 km'], at: [0, -0.36], action: 'cycle', wrap: false },
      { key: 'radar.lock', kind: 'button', label: 'BLANCO', name: 'Radar · fijar siguiente blanco', states: ['', 'FIJANDO'], at: [0.12, -0.36], action: 'pulse', requires: 'sensors' },
    ],
  },
  {
    id: 'ck.ped',
    title: 'PRIORIDAD ENERGÍA',
    frame: boardFrame([0, 0.705, -8.35], norm([0, 1, 0.15]), [0, 0, -1]),
    w: 0.46,
    h: 0.78,
    depth: 0.04,
    free: true,
    controls: SUBSYSTEMS.map((c, i): ControlSpec => ({
      key: c.priority,
      kind: 'rotary',
      label: SHORT[c.id],
      name: `Prioridad · ${c.label}`,
      help: `Decide cuándo se corta ${c.label.toLowerCase()} si falta energía: primero se quedan sin nada los circuitos en BAJA, luego los NORMAL; los ALTA son los últimos. ${c.desc ?? ''}`,
      states: ['ALTA', 'NORMAL', 'BAJA'],
      at: [i % 2 === 0 ? -0.11 : 0.11, 0.3 - Math.floor(i / 2) * 0.12],
      action: 'cycle',
    })),
  },
  {
    id: 'ck.over',
    title: 'ILUMINACIÓN · ENERGÍA',
    frame: boardFrame([0, 2.34, -7.7], [0, -1, 0], [0, 0, 1]),
    w: 1.5,
    h: 0.56,
    depth: 0.06,
    controls: [
      { key: 'light.cockpit', kind: 'toggle', label: 'CABINA', name: 'Luces de cabina', states: ON_OFF, at: [-0.55, 0.13] },
      { key: 'light.corridor', kind: 'toggle', label: 'PASILLO', name: 'Luces del pasillo', states: ON_OFF, at: [-0.33, 0.13] },
      { key: 'light.cargo', kind: 'toggle', label: 'BODEGA', name: 'Luces de bodega', states: ON_OFF, at: [-0.11, 0.13] },
      { key: 'light.nav', kind: 'toggle', label: 'NAVEG.', name: 'Luces de navegación', states: ON_OFF, at: [0.11, 0.13] },
      { key: 'light.beacon', kind: 'toggle', label: 'BALIZA', name: 'Baliza anticolisión', states: ON_OFF, at: [0.33, 0.13] },
      { key: 'light.landing', kind: 'toggle', label: 'FOCOS', name: 'Focos de aterrizaje', states: ON_OFF, at: [0.55, 0.13] },
      { key: 'bat', kind: 'toggle', label: 'BATERÍA', name: 'Batería principal', states: ['DESCONECTADA', 'CONECTADA'], at: [-0.55, -0.11] },
      { key: 'apu', kind: 'toggle', label: 'APU', name: 'Unidad auxiliar de potencia (APU)', states: ['APAGADA', 'EN MARCHA'], at: [-0.33, -0.11] },
      { key: 'rad', kind: 'toggle', label: 'RADIAD.', name: 'Radiadores', states: ['PLEGADOS', 'DESPLEGADOS'], at: [-0.11, -0.11], requires: 'cool' },
      { key: 'grav', kind: 'toggle', label: 'GRAVEDAD', name: 'Compensador inercial', states: ON_OFF, at: [0.11, -0.11], requires: 'grav' },
      { key: 'heat', kind: 'toggle', label: 'CALEF.', name: 'Calefacción de cabina', states: ON_OFF, at: [0.33, -0.11] },
      { key: 'turret', kind: 'toggle', label: 'TORRETA', name: 'Torreta · alimentación', states: ON_OFF, at: [0.55, -0.11], requires: 'weapons' },
    ],
  },
  {
    id: 'cr.brk',
    title: 'DISYUNTORES',
    frame: boardFrame([-1.5, 1.35, -4.4], [1, 0, 0]),
    w: 1.1,
    h: 0.74,
    depth: 0.1,
    controls: SUBSYSTEMS.map((c, i): ControlSpec => ({
      key: c.breaker,
      kind: 'breaker',
      label: SHORT[c.id],
      name: `Disyuntor · ${c.label.toLowerCase()} (${c.rating} kW)`,
      help: `Corta o da corriente al circuito ${c.label}. ${c.desc ?? ''} Salta solo si pide más de ${c.rating} kW durante 2 s (una máquina muy dañada que hace cortocircuito, demasiadas cargas): quita la causa antes de subirlo.`,
      states: BRK,
      at: [-0.39 + (i % 4) * 0.26, 0.18 - Math.floor(i / 4) * 0.22],
    })),
  },
  {
    id: 'cr.ls',
    title: 'SOPORTE VITAL',
    frame: boardFrame([1.5, 1.35, -4.4], [-1, 0, 0]),
    w: 1.34,
    h: 0.76,
    depth: 0.1,
    screens: [{ id: 'mfd.life', pages: ['atmos', 'alerts'], at: [-0.36, 0.08], w: 0.54, h: 0.34 }],
    controls: [
      { key: 'ls.mode', kind: 'rotary', label: 'MODO', name: 'Control de presión', states: ['AUTOMÁTICO', 'MANUAL', 'APAGADO'], at: [0.06, 0.26], action: 'cycle' },
      { key: 'o2gen', kind: 'toggle', label: 'GEN O2', name: 'Generador de O₂', states: ON_OFF, at: [0.2, 0.26] },
      { key: 'scrub', kind: 'toggle', label: 'DEPUR.', name: 'Depurador de CO₂', states: ON_OFF, at: [0.32, 0.26] },
      { key: 'fans', kind: 'toggle', label: 'VENTIL.', name: 'Ventiladores', states: ON_OFF, at: [0.44, 0.26] },
      { key: 'heat', kind: 'toggle', label: 'CALEF.', name: 'Calefacción de cabina', states: ON_OFF, at: [0.56, 0.26] },
      ...(['cockpit', 'corridor', 'cargo'] as const).flatMap((z, row): ControlSpec[] => {
        const y = 0.09 - row * 0.16;
        const label = { cockpit: 'CABINA', corridor: 'PASILLO', cargo: 'BODEGA' }[z];
        const out: ControlSpec[] = [
          { key: `repress.${z}`, kind: 'toggle', label: `REP.${label.slice(0, 3)}`, name: `Represurización manual · ${label.toLowerCase()}`, states: VALVE, at: [0.12, y], requires: 'life' },
          { key: `vent.${z}`, kind: 'toggle', label: `VENT.${label.slice(0, 3)}`, name: `Venteo al vacío · ${label.toLowerCase()}`, states: VALVE, at: [0.46, y], guard: `guard.vent.${z}` },
        ];
        if (z !== 'corridor') out.push({ key: `duct.${z}`, kind: 'toggle', label: `CONDUC.`, name: `Compuerta de ventilación · ${label.toLowerCase()}`, states: OPEN, at: [0.29, y] });
        return out;
      }),
      { key: 'ls.recover', kind: 'toggle', label: 'RECUPERAR', name: 'Compresor: recuperar el aire de la bodega', states: ON_OFF, at: [-0.52, -0.27], requires: 'life' },
    ],
  },
  {
    id: 'cg.rct',
    title: 'REACTOR RX-1',
    frame: boardFrame([-2.5, 1.35, 0.45], [1, 0, 0]),
    w: 1.1,
    h: 0.7,
    depth: 0.1,
    screens: [{ id: 'mfd.reactor', pages: ['reactor', 'power'], at: [0.2, 0.05], w: 0.56, h: 0.34 }],
    indicators: [{ kind: 'reactor-core', at: [-0.34, 0.22], ref: 'reactor' }],
    controls: [
      { key: 'reactor', kind: 'lever', label: 'REACTOR', name: 'Reactor principal', states: ['PARADO', 'EN MARCHA'], at: [-0.42, 0.04] },
      { key: 'rx.set', kind: 'rotary', label: 'SALIDA', name: 'Reactor · potencia de salida', states: ['0 %', '25 %', '50 %', '75 %', '100 %', '110 % SOBRECARGA'], at: [-0.25, 0.17], action: 'cycle', wrap: false },
      { key: 'rx.scram', kind: 'master', label: 'SCRAM', name: 'SCRAM · parada de emergencia', states: ['', 'SCRAM'], at: [-0.1, 0.17], action: 'pulse', guard: 'guard.scram' },
      { key: 'coolpump', kind: 'toggle', label: 'B.REFRIG', name: 'Bomba de refrigerante', states: ON_OFF, at: [-0.25, -0.02], requires: 'cool' },
      { key: 'rad', kind: 'toggle', label: 'RADIAD.', name: 'Radiadores', states: ['PLEGADOS', 'DESPLEGADOS'], at: [-0.11, -0.02], requires: 'cool' },
      { key: 'rx.reset', kind: 'button', label: 'REARME', name: 'Reactor · rearme tras SCRAM (núcleo < 300 °C)', states: ['', 'REARMANDO'], at: [-0.25, -0.2], action: 'pulse' },
    ],
  },
  {
    id: 'bk1.a',
    title: 'PUERTA',
    frame: boardFrame([-1.36, 1.25, Z_CK - BULK_T / 2 - 0.01], [0, 0, -1]),
    w: 0.2,
    h: 0.26,
    depth: 0.03,
    controls: [{ key: 'door.cockpit', kind: 'button', label: 'ABRIR', name: 'Puerta de cabina', states: OPEN, at: [0, -0.02], requires: 'doors' }],
  },
  {
    id: 'bk1.b',
    title: 'PUERTA',
    frame: boardFrame([1.36, 1.25, Z_CK + BULK_T / 2 + 0.01], [0, 0, 1]),
    w: 0.2,
    h: 0.26,
    depth: 0.03,
    controls: [{ key: 'door.cockpit', kind: 'button', label: 'ABRIR', name: 'Puerta de cabina', states: OPEN, at: [0, -0.02], requires: 'doors' }],
  },
  {
    id: 'bk2.a',
    title: 'PUERTA',
    frame: boardFrame([-1.36, 1.25, Z_CR - BULK_T / 2 - 0.01], [0, 0, -1]),
    w: 0.2,
    h: 0.26,
    depth: 0.03,
    controls: [{ key: 'door.cargo', kind: 'button', label: 'ABRIR', name: 'Puerta de bodega', states: OPEN, at: [0, -0.02], requires: 'doors' }],
  },
  {
    id: 'bk2.b',
    title: 'PUERTA',
    frame: boardFrame([1.36, 1.25, Z_CR + BULK_T / 2 + 0.01], [0, 0, 1]),
    w: 0.2,
    h: 0.26,
    depth: 0.03,
    controls: [{ key: 'door.cargo', kind: 'button', label: 'ABRIR', name: 'Puerta de bodega', states: OPEN, at: [0, -0.02], requires: 'doors' }],
  },
  {
    id: 'cg.ramp',
    title: 'BODEGA · ESCLUSA',
    frame: boardFrame([2.55, 1.3, 4.6], [-1, 0, 0]),
    w: 0.74,
    h: 0.34,
    depth: 0.05,
    controls: [
      { key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga', states: ['CERRADA', 'BAJADA'], at: [-0.27, -0.03], requires: 'hyd' },
      { key: 'light.cargo', kind: 'toggle', label: 'LUCES', name: 'Luces de bodega', states: ON_OFF, at: [-0.13, -0.03] },
      { key: 'repress.cargo', kind: 'toggle', label: 'REPRES.', name: 'Represurización manual · bodega', states: VALVE, at: [0.01, -0.03], requires: 'life' },
      { key: 'ls.recover', kind: 'toggle', label: 'RECUP.', name: 'Compresor: recuperar el aire de la bodega', states: ON_OFF, at: [0.14, -0.03], requires: 'life' },
      { key: 'vent.cargo', kind: 'toggle', label: 'VENTEO', name: 'Venteo al vacío · bodega', states: VALVE, at: [0.27, -0.03], guard: 'guard.vent.cargo' },
    ],
  },
  {
    id: 'ext.ramp',
    title: 'RAMPA',
    frame: boardFrame([2.15, 0.55, Z_TAIL + T + 0.02], [0, 0, 1]),
    w: 0.3,
    h: 0.3,
    depth: 0.06,
    controls: [{ key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga (exterior)', states: ['CERRADA', 'BAJADA'], at: [0, -0.03], requires: 'hyd' }],
  },
  {
    id: 'ext.fuel',
    title: 'REPOSTAJE',
    frame: boardFrame([-2.15, 0.55, Z_TAIL + T + 0.02], [0, 0, 1]),
    w: 0.3,
    h: 0.3,
    depth: 0.06,
    controls: [{ key: 'refuel', kind: 'toggle', label: 'TOMA', name: 'Toma de repostaje (en la plataforma de la base)', states: ['DESCONECTADA', 'CONECTADA'], at: [0, -0.03] }],
  },
  {
    id: 'cg.gas',
    title: 'O2 · N2',
    frame: boardFrame([2.1, 1.66, -0.05], [-1, 0, 0]),
    w: 0.56,
    h: 0.18,
    depth: 0.05,
    free: true,
    controls: [
      { key: 'v.gasO2', kind: 'valve', label: 'O2', name: 'Válvula de la botella de O₂', states: VALVE, at: [-0.14, -0.01] },
      { key: 'v.gasN2', kind: 'valve', label: 'N2', name: 'Válvula de las botellas de N₂', states: VALVE, at: [0.14, -0.01] },
    ],
  },
  {
    id: 'cr.bat',
    title: 'BATERÍA',
    frame: boardFrame([-1.13, 1.0, -5.6], [1, 0, 0]),
    w: 0.3,
    h: 0.24,
    depth: 0.02,
    free: true,
    controls: [{ key: 'bat', kind: 'rotary', label: 'AISLADOR', name: 'Aislador de batería', states: ['DESCONECTADA', 'CONECTADA'], at: [0, -0.02], action: 'cycle' }],
  },
];

/** Annunciator panel on the dash: one lamp per alert group (lit when any alert of the group is on). */
const ANNUNCIATOR = {
  console: 'ck.main',
  at: [0, 0.17] as V2,
  cols: 7,
  cell: [0.07, 0.036] as V2,
  lamps: ['CASCO', 'DESCOMP', 'O2', 'CO2', 'FUGA AIRE', 'GAS', 'SOP VITAL', 'REACTOR', 'SCRAM', 'REFRIG', 'BATERÍA', 'DESLASTRE', 'DISYUNTOR', 'APU', 'COMBUST', 'FUGA COMB', 'DESEQUIL', 'MOTOR IZQ', 'MOTOR DER', 'VTOL', 'GRAVEDAD'],
};

function buildDef(): ShipDef {
  const panels = buildPanels();
  const parts = buildParts(PARTS);
  // every control gets its help line from HELP unless its spec wrote one
  const specs = CONSOLES.map((con) => ({ ...con, controls: con.controls.map((c) => ({ ...c, help: c.help ?? HELP[c.key] })) }));
  const { consoles, controls, screens, indicators } = buildConsoles(specs, panels, parts, []);

  // heat shield: armoured roller shutters that come down over every canopy pane
  const plates = panels
    .filter((p) => p.kind === 'glass')
    .map((p) => {
      const xs = p.poly.map((q) => q[0]);
      const ys = p.poly.map((q) => q[1]);
      const cu = (Math.max(...xs) + Math.min(...xs)) / 2;
      const cv = (Math.max(...ys) + Math.min(...ys)) / 2;
      const c: V3 = [p.c[0] + p.u[0] * cu + p.v[0] * cv + p.n[0] * 0.09, p.c[1] + p.u[1] * cu + p.v[1] * cv + p.n[1] * 0.09, p.c[2] + p.u[2] * cu + p.v[2] * cv + p.n[2] * 0.09];
      return { c, u: p.u, v: p.v, n: p.n, w: Math.max(...xs) - Math.min(...xs) + 0.06, h: Math.max(...ys) - Math.min(...ys) + 0.06 };
    });

  const dash = consoles.find((c) => c.id === 'ck.main')!;
  const ped = consoles.find((c) => c.id === 'ck.ped')!;
  const props = buildProps([
    supportBlock('dash', dash, 'cockpit'),
    supportBlock('pedestal', ped, 'cockpit'),
    prop('rack', { id: 'rack', c: [2.25, 0.9, 0.4], zone: 'cargo' }),
    prop('stick', { id: 'stick', c: [0.06, 0.765, -8.405], zone: 'cockpit' }),
    // corridor handholds, cargo tie-down rails on the ribs
    prop('handrail', { id: 'rail.cr.L', c: [-1.52, 1.05, -5.55], yaw: Math.PI, zone: 'corridor' }),
    prop('handrail', { id: 'rail.cr.R', c: [1.52, 1.05, -5.55], zone: 'corridor' }),
    prop('rail', { id: 'tiedown.L', c: [-2.56, 1.15, 1.6], zone: 'cargo' }),
    prop('rail', { id: 'tiedown.R', c: [2.56, 1.15, 1.6], zone: 'cargo' }),
    // hull fairings: chin under the dash, dorsal spine the radiators sit on, the fin with the strobe
    prop('chin', { id: 'chin', c: [0, -0.225, Z_NOSE - 0.31], half: [1.7, 0.225, 0.31], look: { profile: 0 } }),
    prop('spine', { id: 'spine', c: [0, 3.205, Z_CR + 3.3], half: [0.22, 0.125, 2.7] }),
    prop('fin', { id: 'fin', c: [0, 3.485, Z_TAIL - 1.125], half: [0.06, 0.435, 1.175] }),
  ]);
  const defaults: Record<string, number> = {
    reactor: 1,
    'rx.set': 2,
    coolpump: 1,
    rad: 0,
    bat: 1,
    apu: 0,
    'light.cockpit': 1,
    'light.corridor': 1,
    'light.cargo': 1,
    'light.nav': 1,
    'light.beacon': 1,
    'light.landing': 0,
    'door.cockpit': 1,
    'door.cargo': 1,
    ramp: 1,
    gear: 1,
    shield: 0,
    caution: 0,
    // propellant: each side feeds its own engine; the reserve stays isolated
    'v.tank.L': 1,
    'v.tank.R': 1,
    'v.tank.C': 1,
    'pump.tank.L': 1,
    'pump.tank.R': 1,
    'pump.tank.C': 0,
    'v.xfeed.L': 0,
    'v.xfeed.R': 0,
    'v.eng.L': 1,
    'v.eng.R': 1,
    'v.apu': 1,
    'v.rcs': 1,
    xfer: 0,
    rcs: 1,
    'helm.throttle': 0,
    'eng.L.arm': 0,
    'eng.R.arm': 0,
    nacelle: 0,
    lift: 1,
    'v.lift': 1,
    'fa.sas': 1,
    'fa.hold': 1,
    'fa.land': 1,
    ...AP.defaults,
    refuel: 0,
    // life support: automatic pressure control, generator, scrubber, fans, heaters on
    'ls.mode': 0,
    o2gen: 1,
    scrub: 1,
    fans: 1,
    heat: 1,
    'v.gasO2': 1,
    'v.gasN2': 1,
    'ls.recover': 0,
    'duct.cockpit': 1,
    'duct.cargo': 1,
    'radar.mode': 1,
    'radar.range': 2,
    turret: 0,
    grav: 1,
    'mfd.pilot': 0,
    'mfd.copilot': 0,
    'mfd.life': 0,
    'mfd.reactor': 0,
  };
  for (const c of SUBSYSTEMS) defaults[c.breaker] = 1;
  CIRCUITS.forEach((c) => (defaults[`pri.${c.id}`] = c.pri));

  return finishShip({
    id: 'hauler',
    name: 'Selene',
    registry: 'SLN-01',
    role: 'Carguero ligero',
    livery: { tagline: 'CARGA LIGERA · LUNA', decals: 'nacelles' },
    floorHeight: 1.15,
    panels,
    controls,
    consoles,
    screens,
    doors: DOORS,
    ramp: RAMP,
    shield: { key: 'shield', plates },
    gear: {
      key: 'gear',
      legs: [
        [-1.75, -0.3, -5.2],
        [1.75, -0.3, -5.2],
        [-2.3, -0.3, 3.6],
        [2.3, -0.3, 3.6],
      ],
    },
    zones: [
      { id: 'cockpit', label: 'CABINA', min: [-1.6, 0, Z_NOSE], max: [1.6, 2.4, Z_CK], lights: [[0, 2.2, -8.1]], lightKey: 'light.cockpit' },
      { id: 'corridor', label: 'PASILLO', min: [-1.6, 0, Z_CK], max: [1.6, 2.4, Z_CR], lights: [[0, 2.2, -4.4]], lightKey: 'light.corridor' },
      { id: 'cargo', label: 'BODEGA', min: [-2.6, 0, Z_CR], max: [2.6, 3.0, Z_TAIL], lights: [[0, 2.8, -0.4], [0, 2.8, 3.6]], lightKey: 'light.cargo', lux: 7 },
    ],
    seats: [
      { id: 'ck.pilot', name: 'Asiento del piloto', root: [-0.72, 0, -7.78], yaw: 0, exit: [-0.72, 0, -6.8] },
      { id: 'ck.copilot', name: 'Asiento del copiloto', root: [0.72, 0, -7.78], yaw: 0, exit: [0.72, 0, -6.8], mounts: ['turret'] },
    ],
    // the dorsal turret, on its machine: the copilot aims it with the head and fires it
    mounts: [{ id: 'turret', kind: 'turret.minimissile', part: 'turret' }],
    // the forward half of the bay is engineering (reactor, life support): cargo rides aft
    cargo: [
      { pos: [-1.95, 0.38, 1.4], half: [0.45, 0.375, 0.45], yaw: 0, mass: 70, paint: 'orange' },
      { pos: [-1.95, 0.38, 2.35], half: [0.45, 0.375, 0.45], yaw: 0.04, mass: 70, paint: 'orange' },
      { pos: [-1.95, 1.07, 1.45], half: [0.4, 0.3, 0.4], yaw: -0.08, mass: 45, paint: 'grey' },
      { pos: [1.9, 0.26, 3.2], half: [0.55, 0.25, 0.35], yaw: 0, mass: 55, paint: 'grey' },
      { pos: [2.0, 0.87, 3.2], half: [0.35, 0.35, 0.35], yaw: 0.1, mass: 35, paint: 'orange' },
      { pos: [-1.6, 0.3, 3.9], half: [0.3, 0.3, 0.3], yaw: 0.3, mass: 25, paint: 'grey' },
      { pos: [1.3, 0.3, 2.4], half: [0.3, 0.3, 0.3], yaw: -0.2, mass: 25, paint: 'orange' },
      // spare parts on top of the small crates
      { kind: 'spare', pos: [1.3, 0.72, 2.4], half: [0.16, 0.11, 0.16], yaw: 0.5, mass: 9, paint: 'grey' },
      { kind: 'spare', pos: [-1.6, 0.72, 3.9], half: [0.16, 0.11, 0.16], yaw: -0.4, mass: 9, paint: 'orange' },
    ],
    // every fixture sits on a surface (checked by diag:ship): nav lights on the outboard flank of the
    // nacelles, strobe on the fin top, beacons on the dorsal spine and the keel, floodlights in the
    // raked front face of the chin
    extLights: [
      { kind: 'nav-red', pos: [-4.19, 1.15, -0.3], n: [-1, 0, 0] },
      { kind: 'nav-green', pos: [4.19, 1.15, -0.3], n: [1, 0, 0] },
      { kind: 'strobe', pos: [0, 3.92, 4.9], n: [0, 1, 0] },
      { kind: 'beacon', pos: [0, 3.33, 3.0], n: [0, 1, 0] },
      { kind: 'beacon', pos: [0, -0.45, -1.0], n: [0, -1, 0] },
      { kind: 'landing', pos: [-0.9, -0.29, -10.14], n: norm([0, -0.524, -0.852]), dir: norm([-0.1, -0.55, -1]) },
      { kind: 'landing', pos: [0.9, -0.29, -10.14], n: norm([0, -0.524, -0.852]), dir: norm([0.1, -0.55, -1]) },
    ],
    subsystems: SUBSYSTEMS,
    loads: LOADS,
    parts,
    props,
    fluid: FLUID,
    compartments: COMPARTMENTS,
    openings: OPENINGS,
    movers: MOVERS,
    life: LIFE,
    caution: 'caution',
    readouts: { flight: ['fa.sas', 'fa.hold', 'fa.land', 'nacelle', 'helm.throttle'], engines: ['rcs', 'lift', 'nacelle', 'helm.throttle'], radar: ['radar.mode', 'radar.range'], weapons: ['turret'] },
    helm: { throttle: 'helm.throttle', seat: 'ck.pilot' },
    autopilot: AP.autopilot,
    // a heavy freighter: slower and steadier than the shuttle
    flight: { vmax: 22, vside: 6, vz: 4.5, rate: [0.3, 0.35, 0.4], accel: 2.5 },
    annunciator: ANNUNCIATOR,
    defaults,
    modules: MODULES,
    bounds: { min: [-4.4, -1.3, -10.4], max: [4.4, 4.1, 8.6] },
    indicators,
    manual: shipManual(MANUAL),
  });
}

const MANUAL: ManualSection[] = [
  {
    id: 'start',
    title: 'Primeros pasos',
    lead: 'Cómo se usa un mando y qué dicen sus luces',
    body: [
      'Todo en la Selene es físico: mira un mando hasta que se ilumine y pulsa clic izquierdo o E. El casco te dice su nombre, su posición y, si se niega, por qué.',
      'Los selectores (ruedas) avanzan una posición con cada clic; la rueda del ratón los gira en los dos sentidos. Las escalas (potencia, alcance) se paran en el extremo; los selectores de modo dan la vuelta.',
      { fig: 'cover' },
      'Los mandos peligrosos (armado de motores, SCRAM, venteos) están bajo una tapa de seguridad: primero se abre la tapa y después se acciona lo de dentro.',
      { fig: 'leds' },
      { note: 'En este manual, los nombres en azul llevan a la ficha del mando. En cada ficha, «Señalar» pone una marca en tu casco que te lleva hasta él.' },
    ],
  },
  {
    id: 'ship',
    title: 'La nave',
    lead: 'Compartimentos y dónde está cada consola',
    body: [
      'La Selene es un carguero ligero de tres compartimentos presurizados: cabina (vuelo), pasillo (disyuntores y soporte vital) y bodega (reactor, soporte vital, gas y la carga). Por fuera: dos góndolas con depósito y motor, la APU, el depósito de reserva bajo la panza y los radiadores en el lomo.',
      { fig: 'plan' },
      { note: 'Pulsa una consola del plano para ir a sus mandos.' },
    ],
  },
  {
    id: 'cold',
    title: 'Arranque en frío',
    lead: 'De nave apagada a todo en marcha',
    body: [
      'Si todo está parado (reactor en SCRAM o apagado, batería desconectada), este es el orden:',
      {
        steps: [
          'Conecta la batería ([[ck.over/bat]] o el aislador junto a ella, [[cr.bat/bat]]). Comprueba en el armario que los disyuntores están subidos ([[cr.brk/brk.cool]]).',
          'Enciende la bomba de refrigerante ([[cg.rct/coolpump]]). Sin caudal el reactor no se deja arrancar.',
          'Sube la palanca del reactor ([[cg.rct/reactor]]) y elige la salida ([[cg.rct/rx.set]]). Tarda unos 15 s en llegar a EN MARCHA.',
          'Si vas a pedir mucha potencia, despliega los radiadores ([[cg.rct/rad]]).',
          'Con el reactor dando energía, la batería se recarga sola.',
        ],
      },
      { live: 'reactor' },
      { warn: 'Si la batería está vacía y el reactor parado, la única forma de volver a tener energía es la APU, que también necesita algo de batería para su motor de arranque. No dejes que la batería se agote.' },
    ],
  },
  {
    id: 'air',
    title: 'Aire y presión',
    lead: 'Presurizar, ventilar, respirar',
    body: [
      'En la plataforma la nave aparece con la rampa y las puertas abiertas: el interior está en vacío y respiras de tu traje (unos 45 min). El traje se recarga en un compartimento con aire, o sentado en un asiento con el umbilical dando O₂.',
      { fig: 'air' },
      {
        steps: [
          'Cierra la rampa ([[ext.ramp/ramp]] desde fuera o [[ck.main/ramp]]) y las dos puertas ([[bk1.a/door.cockpit]], [[bk2.a/door.cargo]]).',
          'Soporte vital en AUTOMÁTICO ([[cr.ls/ls.mode]]) con generador de O₂, depurador y ventiladores encendidos ([[cr.ls/o2gen]], [[cr.ls/scrub]], [[cr.ls/fans]]).',
          'Válvulas de las botellas abiertas ([[cg.gas/v.gasO2]], [[cg.gas/v.gasN2]]). La página ATMOS sube hacia 70 kPa.',
        ],
      },
      { live: 'air' },
      'Para salir por la rampa hay que vaciar la bodega: recupera el aire con el compresor ([[cg.ramp/ls.recover]]) o ventéala ([[cg.ramp/vent.cargo]], bajo tapa). Una puerta interior no abre con más de 5 kPa de diferencia entre sus dos lados.',
      'El automático no mete gas en un compartimento abierto al vacío (lo tiraría): cierra la abertura o suelda la brecha. Si necesitas aire en uno concreto ya, pon MANUAL y abre su válvula de represurización.',
      { controls: ['ls.mode', 'o2gen', 'scrub', 'fans', 'heat', 'duct.cockpit', 'duct.cargo', 'ls.recover'] },
    ],
  },
  {
    id: 'power',
    title: 'Energía',
    lead: 'Fuentes, batería, prioridades y disyuntores',
    body: [
      'Fuentes: el reactor (hasta 66 kW), la APU (15 kW) y la batería (12 kWh). La batería cubre lo que los generadores no dan y se carga con lo que sobra.',
      { fig: 'power' },
      { live: 'power' },
      'Cada circuito tiene su disyuntor en el armario del pasillo y un selector de prioridad en el pedestal de la cabina. Si falta energía se cortan primero los de prioridad BAJA, luego los NORMAL.',
      'Un disyuntor salta si su circuito pide más de su límite durante 2 s; hay que subirlo a mano. Un panel reventado sobre un cable corta ese circuito hasta que se suelda. Una máquina muy dañada y encendida hace cortocircuito: consume a tirones y puede hacer saltar su disyuntor.',
      { fig: 'circuits' },
    ],
  },
  {
    id: 'reactor',
    title: 'Reactor',
    lead: 'Arranque, potencia, temperatura y SCRAM',
    body: [
      'El reactor RX-1 está en la esquina delantera izquierda de la bodega, con su consola al lado. Da 60 kW al 100 % y calienta el núcleo; la bomba lleva ese calor a los radiadores del lomo.',
      { live: 'reactor' },
      {
        steps: [
          'Arranque: bomba de refrigerante en marcha, batería o APU dando energía, palanca arriba. Unos 15 s.',
          'Potencia: el selector de salida pide de 0 a 110 %. La salida sube despacio hacia lo pedido.',
          'Calor: por encima de 600 °C salta la alarma; a 750 °C el SCRAM automático; por encima de 820 °C el núcleo se daña.',
          'Tras un SCRAM: espera a que el núcleo baje de 300 °C, pulsa REARME y vuelve a arrancarlo.',
        ],
      },
      { controls: ['reactor', 'rx.set', 'coolpump', 'rad', 'rx.scram', 'rx.reset'] },
    ],
  },
  {
    id: 'fuel',
    title: 'Propelente',
    lead: 'Depósitos, válvulas, bombas y repostaje',
    body: [
      'Tres depósitos: izquierdo y derecho en la proa de las góndolas (1100 kg cada uno) y la reserva bajo la panza (300 kg). Cada uno tiene válvula de salida hacia su colector y bomba de refuerzo. Las alimentaciones cruzadas unen los colectores laterales con el central, del que beben la APU y los RCS.',
      { live: 'fuel' },
      { warn: 'Si cierras la reserva con las cruzadas cerradas, el colector central se queda sin propelente: la APU no arranca y los RCS no empujan. Abre la reserva o una cruzada.' },
      'Un depósito dañado pierde propelente (alarma FUGA COMB): pásalo a otro con la transferencia y suéldalo. Si revienta con más de 60 kg dentro, explota.',
      'Repostaje: en la plataforma de la base, conecta la toma exterior ([[ext.fuel/refuel]]). Llena los depósitos con la válvula de salida abierta.',
      { controls: ['v.tank.L', 'v.tank.R', 'v.tank.C', 'pump.tank.L', 'v.xfeed.L', 'v.xfeed.R', 'xfer'] },
    ],
  },
  {
    id: 'engines',
    title: 'Motores, APU y RCS',
    lead: 'Armar, arrancar y aislar',
    body: [
      { fig: 'drive' },
      {
        steps: [
          'Abre la tapa y arma el motor ([[ck.pl/eng.L.arm]]).',
          'Comprueba su alimentación abierta ([[ck.cp/v.eng.L]]) y el circuito PROPULSIÓN con energía.',
          'Pulsa ARRANQUE ([[ck.pl/eng.L.start]]). Unos 3 s hasta régimen.',
        ],
      },
      { warn: 'Un motor por debajo del 35 % de integridad, con energía y propelente llegando, puede explotar, más aún si está en marcha. Ciérrale la alimentación, desármalo y suéldalo.' },
      'La APU ([[ck.over/apu]]) es un generador de respaldo de 15 kW que quema propelente del colector central. Si falla (sin energía de arranque, sin propelente, dañada) se queda en FALLO: apágala, arregla la causa y enciéndela otra vez.',
      'La APU se puede arrancar con la nave en vuelo; los motores principales, igual. Para volar mira el capítulo «Vuelo».',
    ],
  },
  {
    id: 'flight',
    title: 'Vuelo',
    lead: 'Despegar, volar, aterrizar y el piloto automático',
    body: [
      'La Selene se sostiene en el aire con cuatro propulsores de sustentación bajo la quilla (VTOL), corre con los dos motores principales y afina los giros con los RCS. El ordenador de vuelo lo reparte todo: desde el asiento del piloto tú pides velocidades y giros, no empujes.',
      {
        steps: [
          'Siéntate en el asiento del piloto. La sustentación y su alimentación vienen encendidas ([[ck.pl/lift]], [[ck.cp/v.lift]]).',
          'R sube, F baja. W/S adelante y atrás, A/D gira el morro, Z/C desplaza de lado; las flechas cabecean y alabean. Suéltalo todo y la nave frena y se queda a esa altura (vuelo ACOPLADO, [[ck.pl/fa.hold]]).',
          'Para correr: arma y arranca los motores ([[ck.pl/eng.L.arm]], [[ck.pl/eng.L.start]]) despega y sube el acelerador ([[ck.main/helm.throttle]]): ACOPLADO es el tope de motor que el ordenador puede usar (W o VEL del piloto automático piden la velocidad); DESACOPLADO es el empuje directo.',
          'Tren arriba en vuelo ([[ck.main/gear]]) y abajo antes de posarte. Cerca del suelo el asistente ([[ck.pl/fa.land]]) frena la bajada y nivela la nave.',
          'Góndolas en VTOL ([[ck.pl/nacelle]]): el chorro de los motores sale por las toberas ventrales, junto al centro de masas; sirve para subir cargada o deprisa.',
        ],
      },
      { live: 'flight' },
      'Piloto automático, en el tablero sobre el salpicadero: [[ck.ap/ap.on]] lo conecta (tecla P desde el asiento). Cada modo tiene su botón; cualquier mando tuyo manda en su eje mientras lo pulsas.',
      { controls: ['ap.on', 'ap.lvl', 'ap.alt', 'ap.hdg', 'ap.spd', 'ap.nav', 'ap.to', 'ap.land', 'ap.alt.sel', 'ap.hdg.sel', 'ap.spd.sel', 'ap.wp'] },
      ...AP_SPACE_MANUAL,
      { note: 'Ir a un sitio: elige el punto ([[ck.ap/ap.wp]]) y la altura ([[ck.ap/ap.alt.sel]]), pulsa DESPEG. y luego NAV. Al llegar se queda en vuelo estacionario encima; ATERRIZ. la posa y se desconecta.' },
      { warn: 'Sin energía en AVIÓNICA se apaga el ordenador de vuelo: mando DIRECTO, sin estabilizador, sin piloto automático y sin compensar la gravedad. Baja despacio.' },
      'El compensador inercial ([[ck.over/grav]]) mantiene el suelo como «abajo» a bordo: con él, la tripulación camina y la carga se queda donde está aunque la nave se incline o frene. Apagado, se nota todo.',
    ],
  },
  SPACE_MANUAL,
  {
    id: 'damage',
    title: 'Daños y reparación',
    lead: 'Brechas, grietas, máquinas y consolas',
    body: [
      'Las explosiones dañan los paneles del casco y las máquinas cercanas. Un panel por debajo del 60 % tiene grietas y pierde aire despacio; por debajo de 30 es un agujero. Una ventana reventada vacía la cabina en un par de segundos.',
      'La soldadora (tecla 2) repara: mantenla apuntada a un panel o a una máquina con el clic pulsado. Con ella en la mano las máquinas se tiñen según su daño (rojo = grave). Un agujero se rellena hasta cerrarse.',
      'Una consola montada en un panel que revienta desaparece con él, y con ella sus mandos, hasta que se suelda el panel. Un cable que pasa por detrás de un agujero deja su circuito sin energía.',
      ...decompressionManual(),
      { warn: 'Depósitos llenos y motores dañados pueden explotar y encadenar más daños. Aíslalos antes de soldar.' },
    ],
  },
  {
    id: 'alerts',
    title: 'Alarmas',
    lead: 'Qué significa cada lámpara y qué hacer',
    body: [
      'Cualquier alarma nueva enciende la alarma general ([[ck.main/caution]]): púlsala para reconocerla. Las lámparas del tablero, sobre el panel de vuelo, se agrupan por sistema: ámbar es precaución, rojo parpadeante es aviso. Las que están activas ahora salen marcadas.',
      { fig: 'alerts' },
    ],
  },
  {
    id: 'screens',
    title: 'Pantallas',
    lead: 'Qué muestra cada página',
    body: ['Las pantallas multifunción cambian de página con los botones de debajo. Van con el circuito AVIÓNICA; la de energía se sigue viendo con la batería conectada.', { fig: 'pages' }],
  },
  {
    id: 'consoles',
    title: 'Todos los mandos',
    lead: 'Consola por consola',
    body: [{ fig: 'consoles' }],
  },
];

export const HAULER = buildDef();
