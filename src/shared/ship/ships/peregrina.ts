// "Peregrina" long-haul passenger shuttle: single-pilot cockpit → passenger cabin (two seats, two
// bunks, galley, lavatory, life support and water recycling) → side airlock, with a micro-reactor,
// the main engine and fixed radiators outside at the tail, four VTOL lift pads under the belly and
// an inertial compensator under the deck. Built from catalog components only.
// Ship space: metres, +X starboard, +Y up, nose toward −Z, origin = deck level on the centreline.

import { part, prop, profilePoint, PROFILES } from '../catalog/index.js';
import {
  boardFrame,
  boxFace,
  buildConsoles,
  buildParts,
  buildProps,
  doorSensor,
  finishShip,
  outsidePoint,
  paintPanels,
  roofAt,
  ShipBuilder,
  supportBlock,
  wallAt,
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
  type ShipDef,
} from '../def.js';
import { clipPoly, norm, type V2, type V3 } from '../geom.js';
import { AP_SPACE_MANUAL, SPACE_MANUAL, autopilotConsole, breakerControls, circuits, decompressionManual, doorButtons, HELP as KIT, loadOf, priorityControls, type CircuitSpec } from './kit.js';

const T = 0.1; // hull skin
const FLOOR_T = 0.08;
const BULK_T = 0.08;

// Cross-sections (interior surface), x ascending along the top: a narrow cockpit, a taller and
// wider cabin whose walls reach door height so the side hatch cuts a single wall row.
const FWD: V2[] = [[-1.15, 0], [-1.15, 1.55], [-0.7, 2.1], [0.7, 2.1], [1.15, 1.55], [1.15, 0]];
const CAB: V2[] = [[-1.45, 0], [-1.45, 1.95], [-1.0, 2.35], [1.0, 2.35], [1.45, 1.95], [1.45, 0]];
const DOOR: V2[] = [[-0.45, 0], [-0.45, 1.95], [0.45, 1.95], [0.45, 0]];

const Z_NOSE = -5.2;
const Z_CK = -2.6; // cockpit | cabin bulkhead
const Z_LK = 3.4; // cabin | airlock bulkhead
const Z_TAIL = 4.8; // airlock aft wall; the machinery hangs behind it

const WS_BOTTOM = 0.95;
const WS_RAKE = 0.8 / 1.3;
const NOSE_CLIP = { origin: [0, WS_BOTTOM, Z_NOSE] as V3, n: norm([0, 0.8, -1.3]) };
const wsZ = (y: number) => Z_NOSE + (y - WS_BOTTOM) * WS_RAKE;

export const MODULES: ModuleDef[] = [
  { zone: 'cockpit', z0: Z_NOSE, z1: Z_CK, profile: FWD, cols: 2 },
  { zone: 'cabin', z0: Z_CK, z1: Z_LK, profile: CAB, cols: 5 },
  { zone: 'lock', z0: Z_LK, z1: Z_TAIL, profile: CAB, cols: 1 },
];
const MOD = (zone: string) => MODULES.find((m) => m.zone === zone)!;

/** Outer hatch of the airlock, in the starboard wall. */
const HATCH = { c: [1.45, 0, 4.1] as V3, n: [1, 0, 0] as V3, w: 0.9, h: 1.95 };

function buildPanels() {
  const B = new ShipBuilder();
  const [ck, cb, lk] = MODULES;
  // cockpit: wrap-around canopy (upper wall row + chamfers are glass)
  B.strip(ck, 'CK', ['L', 'LC', 'T', 'RC', 'R'], [2, 1, 1, 1, 2], (s, r) => ((s === 'L' || s === 'R') && r === 1) || s === 'LC' || s === 'RC' ? 'glass' : 'hull', T, NOSE_CLIP);
  B.cap('cockpit', 'CK-N', Z_NOSE, FWD, null, { outwardZ: -1, kind: 'hull', t: T, rowH: 1.2, splitX: [0], clip: NOSE_CLIP, face: 'inner' });
  const above = FWD.filter((p) => p[1] > WS_BOTTOM);
  const ws: V3[] = [[-1.15, WS_BOTTOM, wsZ(WS_BOTTOM)], ...above.map((p): V3 => [p[0], p[1], wsZ(p[1])]), [1.15, WS_BOTTOM, wsZ(WS_BOTTOM)]];
  B.addPoly(clipPoly(ws, [0, 0, 0], [1, 0, 0]), [0, 0.5, -1], { id: 'CK-W1', kind: 'glass', zone: 'cockpit', t: 0.05, face: 'inner' });
  B.addPoly(clipPoly(ws, [0, 0, 0], [-1, 0, 0]), [0, 0.5, -1], { id: 'CK-W2', kind: 'glass', zone: 'cockpit', t: 0.05, face: 'inner' });
  B.floor('cockpit', 'CK', -1.15, 1.15, Z_NOSE, Z_CK, 2, 2, FLOOR_T);
  B.cap('cockpit', 'BK1', Z_CK, FWD, DOOR, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.2, face: 'mid', other: 'cabin' });

  // cabin: portholes in the upper wall row beside the passenger seats (both sides) and the bunks
  const porthole = (s: string, r: number, c: number) => r === 1 && ((s === 'L' && (c === 1 || c === 3)) || (s === 'R' && c === 1));
  B.strip(cb, 'CB', ['L', 'LC', 'T', 'RC', 'R'], [2, 1, 1, 1, 2], (s, r, c) => (porthole(s, r, c) ? 'glass' : 'hull'), T);
  B.cap('cabin', 'CB-FW', Z_CK, CAB, FWD, { outwardZ: -1, kind: 'hull', t: T, rowH: 1.2, face: 'inner' });
  B.floor('cabin', 'CB', -1.45, 1.45, Z_CK, Z_LK, 3, 5, FLOOR_T);
  B.cap('cabin', 'BK2', Z_LK, CAB, DOOR, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.2, face: 'mid', other: 'lock' });

  // airlock: one bay; the outer hatch is cut out of its starboard wall
  B.strip(lk, 'LK', ['L', 'LC', 'T', 'RC', 'R'], [2, 1, 1, 1, 2], () => 'hull', T);
  B.cap('lock', 'LK-AFT', Z_TAIL, CAB, null, { outwardZ: 1, kind: 'hull', t: T, rowH: 1.2, splitX: [0], face: 'inner' });
  B.floor('lock', 'LK', -1.45, 1.45, Z_LK, Z_TAIL, 3, 1, FLOOR_T);
  B.cutOpening('lock', HATCH);

  // livery: charcoal lower row and nose cap, teal waterline along the upper row
  return paintPanels(B.panels, [
    { scheme: 1, when: (p) => /^(L|R)1$/.test(p.id.split('-')[1]) || p.id.startsWith('CK-N') },
    { scheme: 3, when: (p) => /^(L|R)2$/.test(p.id.split('-')[1]) },
  ]);
}

// Power circuits: breaker board on the cabin's forward port wall; conduits run under the deck and
// along the ceiling to their loads (a blown panel over a run cuts the circuit).
const BOARD_Z = -2.05;
const CIRCUIT_LIST: CircuitSpec[] = [
  {
    id: 'lights',
    label: 'ILUMINACIÓN',
    short: 'ILUMIN.',
    desc: 'Luces de techo de cabina, habitáculo y esclusa.',
    color: 0xffd36a,
    rating: 3,
    base: 0.05,
    pri: 1,
    routes: [
      [[-1.4, 1.6, BOARD_Z], [-1.2, 2.2, BOARD_Z], [-0.3, 2.3, BOARD_Z], [-0.3, 2.3, 4.6]],
      [[-0.3, 2.3, -2.3], [-0.3, 2.05, -2.7], [-0.3, 2.05, -4.3]],
    ],
  },
  { id: 'ext', label: 'LUCES EXTERIORES', short: 'EXTERIOR', desc: 'Luces de navegación, baliza y focos de aterrizaje.', color: 0x7fd6ff, rating: 3, base: 0.05, pri: 2, routes: [[[-1.4, 1.7, -2.3], [-1.2, 2.2, -2.3], [0.3, 2.3, -2.3], [0.3, 2.3, 4.6]]] },
  {
    id: 'doors',
    label: 'PUERTAS',
    short: 'PUERTAS',
    desc: 'Motores de las tres puertas y el secuenciador de la esclusa.',
    color: 0x9cff8a,
    rating: 5,
    base: 0.1,
    pri: 1,
    routes: [
      [[-1.4, 1.75, -2.4], [-1.3, 2.05, -2.4], [-0.55, 2.05, -2.55]],
      [[-1.3, 2.05, -2.4], [-1.3, 2.05, 3.3], [0.6, 2.3, 3.45], [1.35, 2.0, 3.7]],
    ],
  },
  {
    id: 'avionics',
    label: 'AVIÓNICA',
    short: 'AVIÓNICA',
    desc: 'Pantallas multifunción e instrumentos.',
    color: 0xb58cff,
    rating: 3,
    base: 1.0,
    pri: 0,
    routes: [
      [[-1.4, 1.0, -2.2], [-1.3, -0.16, -2.2], [-0.5, -0.16, -2.2], [-0.5, -0.16, -4.9]],
      [[-0.5, -0.16, -2.2], [1.3, -0.16, -2.2], [1.4, 1.0, -2.05]],
      [[-0.5, -0.16, -2.2], [-0.5, -0.16, 4.6], [0.7, 1.0, 4.75]],
    ],
  },
  {
    id: 'prop',
    label: 'PROPULSIÓN',
    short: 'PROPULS.',
    desc: 'Motor principal, sustentación VTOL, RCS, bomba de refuerzo del depósito.',
    color: 0xff7a3a,
    rating: 6,
    base: 0.1,
    pri: 0,
    routes: [
      [[-1.4, 1.0, -1.9], [-1.3, -0.16, -1.9], [-0.8, -0.16, -1.9], [-0.8, -0.16, 4.7], [0, 0.6, 5.0]],
      [[-0.8, -0.16, -1.9], [-0.8, -0.16, -4.2], [-1.1, 1.2, -4.0]],
    ],
  },
  {
    id: 'life',
    label: 'SOPORTE VITAL',
    short: 'SOP.VITAL',
    desc: 'Generador de O₂, depurador, ventiladores, calefacción, compresor, válvulas de gas y umbilicales de los asientos.',
    color: 0x5cf2d2,
    rating: 10,
    base: 0.2,
    pri: 0,
    routes: [
      [[-1.4, 1.0, -1.8], [-1.3, -0.16, -1.8], [1.0, -0.16, -1.8], [1.0, -0.16, 0.35], [1.15, 0.1, 0.35]],
      [[1.0, -0.16, 0.35], [1.0, -0.16, 4.2], [0.6, 0.8, 4.7]],
    ],
  },
  { id: 'cool', label: 'REFRIGERACIÓN', short: 'REFRIG.', desc: 'Bomba de refrigerante y arranque del reactor.', color: 0x4f9dff, rating: 6, base: 0.05, pri: 0, routes: [[[-1.4, 1.0, -1.7], [-1.35, -0.16, -1.7], [-1.35, -0.16, 4.75], [-0.95, 0.4, 5.1]]] },
  {
    id: 'hab',
    label: 'HABITABILIDAD',
    short: 'HABITAB.',
    desc: 'Cocina, reciclador de agua y servicios del habitáculo.',
    color: 0xf0a0ff,
    rating: 5,
    base: 0.1,
    pri: 2,
    routes: [
      [[-1.4, 1.65, -2.5], [-1.2, 2.2, -2.5], [-1.2, 2.2, 0.6], [-1.15, 1.05, 0.6]],
      [[-1.2, 2.2, 0.6], [1.2, 2.2, 0.97], [1.17, 1.7, 0.97]],
    ],
  },
  { id: 'grav', label: 'COMPENSADOR INERCIAL', short: 'GRAVEDAD', desc: 'Compensador inercial bajo la cubierta del habitáculo.', color: 0xc07fff, rating: 4, base: 0.05, pri: 1, routes: [[[-1.4, 1.0, -1.6], [-1.3, -0.16, -1.6], [0, -0.16, -1.6], [0, -0.2, -1.8]]] },
  { id: 'hyd', label: 'MECANISMOS', short: 'MECAN.', desc: 'Tren de aterrizaje y despliegue de las alas solares.', color: 0xff9a5c, rating: 5, base: 0.1, pri: 1, routes: [[[-1.4, 1.0, -2.0], [-1.3, -0.16, -2.0], [0.4, -0.16, -2.0], [0.4, -0.16, 3.2]], [[0.4, -0.16, -2.0], [0.4, -0.16, -3.6]], [[-1.2, 2.2, -2.0], [0, 2.32, 0.9]]] },
];
const { subsystems: SUBSYSTEMS, defaults: CIRCUIT_DEFAULTS, short: SHORT } = circuits(CIRCUIT_LIST);

// -----------------------------------------------------------------------------------------------
// Machinery: catalog components, placed
// -----------------------------------------------------------------------------------------------

const onRoof = (zone: string, x: number, z: number, rise: number): V3 => outsidePoint(roofAt(MOD(zone), x, z), T, rise);
const SIDEWAYS = Math.PI / 2; // against a side wall: the component's depth runs across the ship

const PARTS: PartSpec[] = [
  // tail, outside: the micro-reactor with its coolant pump under it, the main engine behind the airlock
  part('reactor.fission.XS', { id: 'reactor', c: [-0.95, 1.05, 5.42], zone: null, circuit: 'cool', tag: 'rx' }),
  part('coolpump.S', { id: 'coolpump', c: [-0.95, 0.26, 5.42], zone: null, circuit: 'cool' }),
  part('engine.main.S', { id: 'eng', name: 'Motor principal', c: [0, 0.95, 5.85], zone: null, circuit: 'prop', feed: 'eng', lamp: 'MOTOR' }),
  // roof: fixed radiators over the airlock, the folding solar wing over the cabin, the antenna
  part('radiator.panel.S', { id: 'rad.L', name: 'Radiador izquierdo', c: onRoof('cabin', -0.5, 3.3, 0.05), half: [0.45, 0.05, 1.0], zone: null, circuit: 'cool' }),
  part('radiator.panel.S', { id: 'rad.R', name: 'Radiador derecho', c: onRoof('cabin', 0.5, 3.3, 0.05), half: [0.45, 0.05, 1.0], zone: null, circuit: 'cool' }),
  part('solar.wing.S', { id: 'solar', name: 'Alas solares', c: onRoof('cabin', 0, 0.9, 0.05), zone: null, circuit: 'hyd', sw: { deploy: 'solar' } }),
  part('antenna.S', { id: 'antenna', c: onRoof('cockpit', 0.35, -3.2, 0.3), zone: null, circuit: 'avionics' }),
  // belly: the conformal propellant tank; four RCS blocks on the flanks
  part('tank.conformal.S', { id: 'tank', name: 'Depósito de propelente', c: [0, -0.62, 0.2], zone: null, p: { fill: 480 } }),
  // VTOL lift pads under the belly, round the centre of mass (front pair under the cabin's forward
  // end, rear pair under the airlock): they hold the shuttle in the air, the engine only runs it
  part('lift.S', { id: 'lift.FL', name: 'Sustentación delantera izq.', c: [-1.15, -0.62, -1.4], zone: null, circuit: 'prop', feed: 'lift.FL' }),
  part('lift.S', { id: 'lift.FR', name: 'Sustentación delantera der.', c: [1.15, -0.62, -1.4], zone: null, circuit: 'prop', feed: 'lift.FR' }),
  part('lift.S', { id: 'lift.RL', name: 'Sustentación trasera izq.', c: [-1.15, -0.62, 4.54], zone: null, circuit: 'prop', feed: 'lift.RL' }),
  part('lift.S', { id: 'lift.RR', name: 'Sustentación trasera der.', c: [1.15, -0.62, 4.54], zone: null, circuit: 'prop', feed: 'lift.RR' }),
  // under the cabin deck, forward: the inertial compensator
  part('grav.S', { id: 'grav', c: [0, -0.3, -1.8], zone: null, circuit: 'grav' }),
  part('rcs.S', { id: 'rcs.FL', name: 'RCS delantero izq.', c: [-1.35, 1.3, -4.0], zone: null, circuit: 'prop', feed: 'rcs.FL' }),
  part('rcs.S', { id: 'rcs.FR', name: 'RCS delantero der.', c: [1.35, 1.3, -4.0], zone: null, circuit: 'prop', feed: 'rcs.FR' }),
  part('rcs.S', { id: 'rcs.RL', name: 'RCS trasero izq.', c: [-1.65, 1.45, 3.2], zone: null, circuit: 'prop', feed: 'rcs.RL' }),
  part('rcs.S', { id: 'rcs.RR', name: 'RCS trasero der.', c: [1.65, 1.45, 3.2], zone: null, circuit: 'prop', feed: 'rcs.RR' }),
  // cockpit: the battery bank behind the pilot, port side
  part('battery.S', { id: 'battery', c: [-0.9, 0.45, -2.95], zone: 'cockpit', sw: { on: 'bat' } }),
  // cabin, starboard: life support stack, then water tank + recycler
  part('o2gen.S', { id: 'o2gen', c: [1.17, 0.38, 0.35], yaw: SIDEWAYS, zone: 'cabin', circuit: 'life' }),
  part('scrubber.S', { id: 'scrubber', c: [1.17, 1.12, 0.35], yaw: SIDEWAYS, zone: 'cabin', circuit: 'life', sw: { run: 'scrub' } }),
  part('water.S', { id: 'water', c: [1.17, 0.42, 0.97], yaw: SIDEWAYS, zone: 'cabin' }),
  part('recycler.S', { id: 'recycler', c: [1.17, 1.26, 0.97], yaw: SIDEWAYS, zone: 'cabin', circuit: 'hab' }),
  // cabin, port: the pantry above the galley counter
  part('pantry.S', { id: 'pantry', c: [-1.13, 1.47, 0.6], half: [0.3, 0.45, 0.25], yaw: SIDEWAYS, zone: 'cabin' }),
  // airlock, port: the gas bottles in front of the suit lockers
  part('gas.o2.S', { id: 'gas.O2', c: [-1.3, 0.55, 3.56], zone: 'lock', sw: { valve: 'v.gasO2' } }),
  part('gas.n2.S', { id: 'gas.N2', c: [-1.15, 0.55, 3.83], yaw: SIDEWAYS, zone: 'lock', sw: { valve: 'v.gasN2' } }),
];
const P = (id: string) => PARTS.find((p) => p.id === id)!;

const FLUID: FluidNetDef = {
  manifolds: ['m', 'mRCS'],
  pipes: [
    { a: 'tank', b: 'm', valve: 'v.tank' },
    { a: 'm', b: 'eng', valve: 'v.eng' },
    { a: 'm', b: 'mRCS', valve: 'v.rcs' },
    { a: 'mRCS', b: 'rcs.FL' },
    { a: 'mRCS', b: 'rcs.FR' },
    { a: 'mRCS', b: 'rcs.RL' },
    { a: 'mRCS', b: 'rcs.RR' },
    { a: 'm', b: 'lift.FL', valve: 'v.lift' },
    { a: 'm', b: 'lift.FR', valve: 'v.lift' },
    { a: 'm', b: 'lift.RL', valve: 'v.lift' },
    { a: 'm', b: 'lift.RR', valve: 'v.lift' },
  ],
  circuit: 'prop',
  refuel: { key: 'refuel' },
};

const COMPARTMENTS: CompartmentDef[] = [
  { id: 'cockpit', label: 'CABINA', volume: 9.5 },
  { id: 'cabin', label: 'HABITÁCULO', volume: 34 },
  { id: 'lock', label: 'ESCLUSA', volume: 8 },
];

const OPENINGS: OpeningDef[] = [
  { id: 'door.cockpit', kind: 'door', a: 'cockpit', b: 'cabin', key: 'door.cockpit', area: 1.75 },
  { id: 'door.lock', kind: 'door', a: 'cabin', b: 'lock', key: 'door.lock', area: 1.75 },
  { id: 'door.ext', kind: 'door', a: 'lock', b: null, key: 'door.ext', area: 1.75 },
  { id: 'vent.cockpit', kind: 'vent', a: 'cockpit', b: null, key: 'vent.cockpit', area: 0.003 },
  { id: 'vent.cabin', kind: 'vent', a: 'cabin', b: null, key: 'vent.cabin', area: 0.006 },
  { id: 'vent.lock', kind: 'vent', a: 'lock', b: null, key: 'vent.lock', area: 0.01 },
  { id: 'duct.cockpit', kind: 'duct', a: 'cockpit', b: 'cabin', key: 'duct.cockpit', area: 0.015 },
  { id: 'duct.lock', kind: 'duct', a: 'cabin', b: 'lock', key: 'duct.lock', area: 0.015 },
];

const DOORS: DoorDef[] = [
  { key: 'door.cockpit', c: [0, 0, Z_CK], n: [0, 0, 1], w: 0.9, h: 1.95, offset: 0.075 },
  { key: 'door.lock', c: [0, 0, Z_LK], n: [0, 0, 1], w: 0.9, h: 1.95, offset: -0.075 },
  { key: 'door.ext', c: HATCH.c, n: HATCH.n, w: HATCH.w, h: HATCH.h, offset: 0.05 },
];

const MOVERS: MoverDef[] = [
  ...DOORS.map((d): MoverDef => ({ key: d.key, rate: 1 / 0.9, circuit: 'doors', load: 1.2, sensor: doorSensor(d) })),
  { key: 'gear', rate: 1 / 5, circuit: 'hyd', load: 2 },
  { key: 'solar', rate: 1 / 6, circuit: 'hyd', load: 0.6 },
];

const LOADS: LoadDef[] = [
  { key: 'light.cockpit', circuit: 'lights', kw: 0.25 },
  { key: 'light.cabin', circuit: 'lights', kw: 0.4 },
  { key: 'light.lock', circuit: 'lights', kw: 0.2 },
  { key: 'light.nav', circuit: 'ext', kw: 0.12 },
  { key: 'light.beacon', circuit: 'ext', kw: 0.08 },
  { key: 'light.landing', circuit: 'ext', kw: 1.2 },
  { key: 'rcs', circuit: 'prop', kw: 0.25 },
  { key: 'lift', circuit: 'prop', kw: 0.5 },
  loadOf(PARTS, 'grav'),
  loadOf(PARTS, 'coolpump'),
  loadOf(PARTS, 'o2gen'),
  loadOf(PARTS, 'scrubber', 'scrub'),
  loadOf(PARTS, 'recycler'),
  { key: 'fans', circuit: 'life', kw: 0.6 },
  { key: 'ls.recover', circuit: 'life', kw: 3 },
  { key: 'galley', circuit: 'hab', kw: 2.2 },
];

const LIFE: LifeSupportDef = { circuit: 'life', mode: 'ls.mode', fans: 'fans', heat: 'heat', heatKw: 0.45, repress: 'repress.', recover: { key: 'ls.recover', zone: 'lock' } };

// -----------------------------------------------------------------------------------------------
// Consoles
// -----------------------------------------------------------------------------------------------

const ON_OFF = ['APAGADO', 'ENCENDIDO'];
const OPEN = ['CERRADA', 'ABIERTA'];
const VALVE = ['CERRADA', 'ABIERTA'];
const tilt = (deg: number): V3 => [0, Math.cos((deg * Math.PI) / 180), Math.sin((deg * Math.PI) / 180)];
const THROTTLE = Array.from({ length: 11 }, (_, i) => `${i * 10} %`);

const HELP: Record<string, string> = {
  'door.cockpit': 'Abre o cierra la puerta entre la cabina de pilotaje y el habitáculo. No abre con más de 5 kPa de diferencia entre los dos lados.',
  'door.lock': 'Puerta interior de la esclusa, entre el habitáculo y la esclusa. No abre con más de 5 kPa de diferencia: con la esclusa vacía, usa el ciclo ENTRAR.',
  'door.ext': 'Escotilla exterior de la esclusa, en el costado de estribor. Solo abre con la esclusa vacía (menos de 5 kPa): usa el ciclo SALIR, que la vacía primero.',
  gear: 'Sube o baja el tren de aterrizaje. Con peso sobre las patas no se deja subir. Las tres luces verdes del tablero indican abajo y blocado.',
  caution: 'Se enciende con cualquier alarma nueva. Púlsala para reconocerla: se apaga, pero la alarma sigue en el anunciador y en la página ALARM hasta que desaparezca la causa.',
  'helm.throttle': 'Tope del motor principal. En vuelo ACOPLADO y con el piloto automático es lo más que el ordenador de vuelo puede usar del motor para alcanzar la velocidad pedida (W/S, o VEL / NAV del piloto automático; 0 % = solo sustentación y RCS). DESACOPLADO es su empuje directo (control manual). En tierra y ACOPLADO no empuja: despega primero. El casco muestra MOTOR: lo que empuja y por qué. El motor tiene que estar armado y en marcha. Gasta propelente en proporción.',
  'eng.arm': KIT.engineArm('motor principal'),
  'eng.start': KIT.engineStart('motor principal', 'PROPULSIÓN'),
  'v.tank': 'Válvula de salida del depósito de propelente bajo la cubierta. Cerrada, nada recibe propelente (ni se puede repostar). Ciérrala si el depósito pierde.',
  'pump.tank': 'Bomba de refuerzo del depósito: sube el caudal que puede dar de 1,2 a 6 kg/s. El motor a plena potencia pide 1 kg/s y la sustentación en vuelo estacionario casi 0,5: sin la bomba no llegan los dos a la vez. 0,8 kW de PROPULSIÓN.',
  'v.eng': 'Válvula de alimentación del motor principal. Cerrarla le corta el propelente (se apaga) y es la forma de aislar un motor dañado que puede explotar.',
  'v.rcs': 'Válvula de alimentación de los cuatro bloques RCS desde el colector.',
  rcs: 'Activa los propulsores de maniobra (cuatro bloques de toberas en los seis ejes, 0,25 kW): giros finos y desplazamientos. Beben propelente del colector a través de su válvula AL.RCS.',
  lift: 'Activa los cuatro propulsores de sustentación VTOL bajo la panza (6 kN cada uno, 0,5 kW): son los que mantienen la lanzadera en el aire. Beben del colector por la válvula AL.VTOL.',
  'v.lift': 'Válvula de alimentación de los cuatro propulsores de sustentación desde el colector. Cerrada, la nave no puede quedarse en el aire.',
  'fa.sas': 'Estabilizador: con los mandos de giro sueltos, frena los giros y mantiene la actitud (y el rumbo). Sin él, las teclas de giro dan par directo y la nave sigue girando por inercia.',
  'fa.hold': 'ACOPLADO: los mandos de traslación piden una velocidad (adelante, lateral, subir) y al soltarlos la nave se para y mantiene la altura. DESACOPLADO: piden aceleración y la nave sigue en inercia; el ordenador solo compensa la gravedad.',
  'fa.land': 'Asistente de aterrizaje: cerca del suelo limita la velocidad de bajada según la altura y, con el tren abajo, nivela la nave para posarla.',
  grav: 'Compensador inercial (2,5 kW, bajo la cubierta): a bordo el suelo sigue siendo «abajo» aunque la nave se incline, frene o acelere, así que el equipaje y los pasajeros no resbalan. Apagado o sin energía se nota todo; en caída libre, se flota.',
  'light.landing': 'Focos bajo el morro que iluminan el suelo delante de la nave (1,2 kW del circuito LUCES EXTERIORES).',
  reactor: KIT.reactorLever(P('reactor')),
  'rx.set': KIT.reactorSet(P('reactor')),
  'rx.scram': KIT.scram(),
  'rx.reset': KIT.reactorReset(),
  coolpump: KIT.coolpump(P('coolpump')),
  bat: KIT.battery(P('battery')),
  solar: 'Despliega las alas solares del techo: dan hasta 3,5 kW con el sol de cara (plegadas, apenas un 12 %). La energía solar se usa antes que la del reactor y recarga la batería.',
  'light.cockpit': 'Luces de techo de la cabina de pilotaje. Sin energía en ILUMINACIÓN se encienden las de emergencia, rojas.',
  'light.cabin': 'Luces del habitáculo de pasajeros. Sin energía en ILUMINACIÓN se encienden las de emergencia, rojas.',
  'light.lock': 'Luz de techo de la esclusa (0,2 kW). Sin energía en ILUMINACIÓN se enciende la de emergencia, roja.',
  'light.nav': 'Luces de posición (roja a babor, verde a estribor) y el estrobo blanco sobre el motor.',
  'light.beacon': 'Baliza roja en el techo y en la panza: avisa de que la nave está activa.',
  fans: 'Ventiladores (0,6 kW): mueven el aire entre compartimentos por los conductos con la compuerta abierta. Sin ellos el O₂ y el depurador del habitáculo no llegan a la cabina.',
  heat: 'Calefacción de los compartimentos presurizados (0,45 kW cada uno). Sin ella el aire se enfría poco a poco hacia −18 °C.',
  'ls.mode': 'AUTOMÁTICO mantiene cada compartimento estanco a 70 kPa con gas de las botellas (se suspende si hay fuga). MANUAL solo mete gas donde abras la válvula de represurización. APAGADO no repone nada.',
  o2gen: KIT.o2gen(P('o2gen'), 'en el habitáculo'),
  scrub: KIT.scrub(P('scrubber'), 'en el habitáculo'),
  recycler: 'Reciclador de agua (0,8 kW, circuito HABITABILIDAD): convierte el agua usada otra vez en potable, recuperando el 90 %. Sin él el agua de un viaje largo dura diez veces menos.',
  galley: 'Cocina del habitáculo: horno y calentador de agua (2,2 kW del circuito HABITABILIDAD, prioridad BAJA por defecto). Apágala si falta energía.',
  'ls.recover': 'Compresor (3 kW): bombea el aire de la esclusa a las botellas en vez de tirarlo. El ciclo de la esclusa lo usa solo; a mano, ciérralo cuando la esclusa esté a 0.',
  'repress.cockpit': 'En modo MANUAL, abre las botellas de O₂/N₂ directamente a la cabina de pilotaje, aunque tenga una fuga (decisión tuya).',
  'repress.cabin': 'En modo MANUAL, abre las botellas de O₂/N₂ directamente al habitáculo, aunque tenga una fuga (decisión tuya).',
  'repress.lock': 'En modo MANUAL, llena la esclusa con gas de las botellas. En AUTOMÁTICO no hace falta: el control de presión la llena al cerrarse.',
  'vent.cockpit': 'Válvula de venteo: vacía el aire de la cabina de pilotaje al espacio, despacio. Bajo tapa porque tira el aire.',
  'vent.cabin': 'Válvula de venteo: vacía el aire del habitáculo al espacio, despacio. Bajo tapa porque tira el aire.',
  'vent.lock': 'Válvula de venteo de la esclusa: la vacía al espacio en lugar de recuperar el aire (más rápido si el compresor falla). Bajo tapa.',
  'duct.cockpit': 'Compuerta del conducto de ventilación entre cabina de pilotaje y habitáculo. Cerrada aísla la cabina, pero le corta el aire de los ventiladores.',
  'duct.lock': 'Compuerta del conducto entre habitáculo y esclusa. El ciclo de la esclusa la cierra mientras está vacía y la abre al volver a llenarse.',
  'v.gasO2': KIT.gasValve(P('gas.O2')),
  'v.gasN2': KIT.gasValve(P('gas.N2')),
  refuel: 'Conecta la manguera de la plataforma de la base: llena el depósito a 10 kg/s con su válvula de salida abierta. Solo funciona aterrizado en la plataforma.',
};

const cycle = (value: 0 | 1, at: V2, label = value ? 'SALIR' : 'ENTRAR'): ControlSpec => ({
  key: 'lock.cycle',
  kind: 'button',
  label,
  name: value ? 'Esclusa · ciclo de salida' : 'Esclusa · ciclo de entrada',
  help: value
    ? 'Ciclo de SALIDA: cierra la puerta interior, recupera el aire de la esclusa a las botellas y abre la escotilla exterior (unos 25 s). Espera dentro de la esclusa.'
    : 'Ciclo de ENTRADA: cierra la escotilla exterior, llena la esclusa hasta la presión del habitáculo y abre la puerta interior (unos 15 s).',
  states: ['DENTRO', 'FUERA'],
  at,
  action: 'set',
  value,
  requires: 'doors',
});

// autopilot board on the glareshield, between the dash and the windshield
const AP = autopilotConsole({ frame: boardFrame([0, 1.17, -5.0], norm([0, 0.35, 1])), circuit: 'avionics', w: 1.0, h: 0.22 });

const CONSOLES: ConsoleSpec[] = [
  ...AP.consoles,
  {
    id: 'ck.main',
    title: 'CONTROL DE VUELO',
    frame: boardFrame([0, 0.9, Z_NOSE + 0.35], tilt(40), [0, 0.2, -1]),
    w: 2.0,
    h: 0.6,
    depth: 0.12,
    free: true,
    screens: [
      { id: 'mfd.pilot', pages: ['flight', 'ap', 'nav', 'mass', 'engines', 'fuel', 'alerts'], at: [-0.45, 0.04], w: 0.5, h: 0.3 },
      { id: 'mfd.aux', pages: ['status', 'power', 'atmos', 'stores', 'lock', 'hull'], at: [0.45, 0.04], w: 0.5, h: 0.3 },
    ],
    indicators: [{ kind: 'gear-greens', at: [0.85, 0.22], ref: 'gear' }],
    controls: [
      { key: 'door.cockpit', kind: 'button', label: 'P. CABINA', name: 'Puerta de cabina', states: OPEN, at: [-0.85, 0.12], requires: 'doors' },
      cycle(1, [-0.92, -0.12]),
      cycle(0, [-0.78, -0.12]),
      { key: 'gear', kind: 'lever', label: 'TREN', name: 'Tren de aterrizaje', states: ['ARRIBA', 'ABAJO Y BLOCADO'], at: [0.85, 0.02], requires: 'hyd' },
      { key: 'helm.throttle', kind: 'rotary', label: 'ACELER.', name: 'Acelerador del motor principal', states: THROTTLE, at: [0.85, -0.2], action: 'cycle', wrap: false },
      { key: 'caution', kind: 'master', label: 'ALARMA', name: 'Alarma general (reconocer)', states: ['SIN AVISOS', 'AVISO ACTIVO'], at: [0, -0.2], action: 'reset' },
    ],
  },
  {
    id: 'ck.l',
    title: 'PROPULSIÓN',
    frame: boardFrame([-0.98, 0.8, -3.82], norm([1, 1.25, 0]), [0, 0, -1]),
    w: 0.4,
    h: 0.95,
    depth: 0.1,
    controls: [
      { key: 'eng.arm', kind: 'toggle', label: 'ARMAR', name: 'Motor principal · armado', states: ['DESARMADO', 'ARMADO'], at: [-0.1, 0.34], guard: 'guard.eng' },
      { key: 'eng.start', kind: 'button', label: 'ARRANQUE', name: 'Motor principal · arranque', states: ['', 'ARRANCANDO'], at: [0.08, 0.34], action: 'pulse' },
      { key: 'v.tank', kind: 'toggle', label: 'V.DEP', name: 'Válvula de salida del depósito', states: VALVE, at: [-0.12, 0.16] },
      { key: 'pump.tank', kind: 'toggle', label: 'BOMBA', name: 'Bomba de refuerzo del depósito', states: ON_OFF, at: [0, 0.16], requires: 'prop' },
      { key: 'v.eng', kind: 'toggle', label: 'AL.MOTOR', name: 'Alimentación del motor principal', states: VALVE, at: [0.12, 0.16] },
      { key: 'v.rcs', kind: 'toggle', label: 'AL.RCS', name: 'Alimentación de los RCS', states: VALVE, at: [-0.12, -0.02] },
      { key: 'rcs', kind: 'toggle', label: 'RCS', name: 'Propulsores de maniobra (RCS)', states: ON_OFF, at: [0.02, -0.02] },
      { key: 'fa.sas', kind: 'toggle', label: 'ESTAB.', name: 'Estabilizador (amortigua giros)', states: ON_OFF, at: [-0.12, -0.2] },
      { key: 'fa.hold', kind: 'toggle', label: 'ACOPLADO', name: 'Vuelo acoplado (mantiene velocidad)', states: ['DESACOPLADO', 'ACOPLADO'], at: [0, -0.2] },
      { key: 'fa.land', kind: 'toggle', label: 'ATERRIZ.', name: 'Asistente de aterrizaje', states: ON_OFF, at: [0.12, -0.2] },
      { key: 'light.landing', kind: 'toggle', label: 'FOCOS', name: 'Focos de aterrizaje', states: ON_OFF, at: [-0.1, -0.38] },
      { key: 'lift', kind: 'toggle', label: 'VTOL', name: 'Propulsores de sustentación (VTOL)', states: ON_OFF, at: [0.02, -0.38] },
      { key: 'v.lift', kind: 'toggle', label: 'AL.VTOL', name: 'Alimentación de la sustentación VTOL', states: VALVE, at: [0.14, -0.38] },
    ],
  },
  {
    id: 'ck.r',
    title: 'REACTOR RX-20',
    frame: boardFrame([0.98, 0.8, -3.82], norm([-1, 1.25, 0]), [0, 0, -1]),
    w: 0.4,
    h: 0.95,
    depth: 0.1,
    indicators: [{ kind: 'reactor-core', at: [-0.08, 0.36], ref: 'reactor' }],
    controls: [
      { key: 'reactor', kind: 'lever', label: 'REACTOR', name: 'Reactor', states: ['PARADO', 'EN MARCHA'], at: [0.1, 0.3] },
      { key: 'rx.set', kind: 'rotary', label: 'SALIDA', name: 'Reactor · potencia de salida', states: ['0 %', '25 %', '50 %', '75 %', '100 %', '110 % SOBRECARGA'], at: [-0.1, 0.14], action: 'cycle', wrap: false },
      { key: 'rx.scram', kind: 'master', label: 'SCRAM', name: 'SCRAM · parada de emergencia', states: ['', 'SCRAM'], at: [0.1, 0.12], action: 'pulse', guard: 'guard.scram' },
      { key: 'coolpump', kind: 'toggle', label: 'B.REFRIG', name: 'Bomba de refrigerante', states: ON_OFF, at: [-0.1, -0.05], requires: 'cool' },
      { key: 'rx.reset', kind: 'button', label: 'REARME', name: 'Reactor · rearme tras SCRAM (núcleo < 300 °C)', states: ['', 'REARMANDO'], at: [0.1, -0.05], action: 'pulse' },
      { key: 'bat', kind: 'toggle', label: 'BATERÍA', name: 'Batería', states: ['DESCONECTADA', 'CONECTADA'], at: [-0.1, -0.24] },
      { key: 'solar', kind: 'toggle', label: 'SOLAR', name: 'Alas solares', states: ['PLEGADAS', 'DESPLEGADAS'], at: [0.1, -0.24], requires: 'hyd' },
      { key: 'grav', kind: 'toggle', label: 'GRAVEDAD', name: 'Compensador inercial', states: ON_OFF, at: [0, -0.4], requires: 'grav' },
    ],
  },
  {
    id: 'ck.over',
    title: 'ILUMINACIÓN',
    frame: boardFrame([0, 2.06, -3.95], [0, -1, 0], [0, 0, 1]),
    w: 1.2,
    h: 0.36,
    depth: 0.05,
    controls: [
      { key: 'light.cockpit', kind: 'toggle', label: 'CABINA', name: 'Luces de cabina', states: ON_OFF, at: [-0.5, 0.0] },
      { key: 'light.cabin', kind: 'toggle', label: 'HABITÁC.', name: 'Luces del habitáculo', states: ON_OFF, at: [-0.3, 0.0] },
      { key: 'light.lock', kind: 'toggle', label: 'ESCLUSA', name: 'Luz de la esclusa', states: ON_OFF, at: [-0.1, 0.0] },
      { key: 'light.nav', kind: 'toggle', label: 'NAVEG.', name: 'Luces de navegación', states: ON_OFF, at: [0.1, 0.0] },
      { key: 'light.beacon', kind: 'toggle', label: 'BALIZA', name: 'Baliza anticolisión', states: ON_OFF, at: [0.3, 0.0] },
      { key: 'light.landing', kind: 'toggle', label: 'FOCOS', name: 'Focos de aterrizaje', states: ON_OFF, at: [0.5, 0.0] },
    ],
  },
  {
    id: 'ck.bat',
    title: 'BATERÍA',
    on: boxFace({ c: P('battery').c, half: P('battery').half, yaw: 0 }, '+x', [0, 0.18]),
    part: 'battery',
    w: 0.26,
    h: 0.2,
    depth: 0.02,
    controls: [{ key: 'bat', kind: 'rotary', label: 'AISLADOR', name: 'Aislador de batería', states: ['DESCONECTADA', 'CONECTADA'], at: [0, -0.02], action: 'cycle' }],
  },
  ...doorButtons(DOORS[0], { id: 'bk1', name: 'Puerta de cabina', side: 0.75, t: BULK_T, requires: 'doors' }),
  {
    id: 'cb.pwr',
    title: 'CUADRO ELÉCTRICO',
    on: wallAt(MOD('cabin'), 'L', BOARD_Z, 1.3),
    w: 0.95,
    h: 0.8,
    depth: 0.08,
    controls: [...breakerControls(SUBSYSTEMS, SHORT, { cols: 5, origin: [-0.36, 0.26], dx: 0.18, dy: 0.2 }), ...priorityControls(SUBSYSTEMS, SHORT, { cols: 5, origin: [-0.36, -0.12], dx: 0.18, dy: 0.17 })],
  },
  {
    id: 'cb.ls',
    title: 'SOPORTE VITAL',
    on: wallAt(MOD('cabin'), 'R', BOARD_Z, 1.3),
    w: 1.1,
    h: 0.8,
    depth: 0.08,
    screens: [{ id: 'mfd.life', pages: ['atmos', 'stores', 'lock', 'alerts'], at: [-0.27, 0.14], w: 0.48, h: 0.3 }],
    controls: [
      { key: 'ls.mode', kind: 'rotary', label: 'MODO', name: 'Control de presión', states: ['AUTOMÁTICO', 'MANUAL', 'APAGADO'], at: [0.08, 0.28], action: 'cycle' },
      { key: 'o2gen', kind: 'toggle', label: 'GEN O2', name: 'Generador de O₂', states: ON_OFF, at: [0.2, 0.28] },
      { key: 'scrub', kind: 'toggle', label: 'DEPUR.', name: 'Depurador de CO₂', states: ON_OFF, at: [0.32, 0.28] },
      { key: 'fans', kind: 'toggle', label: 'VENTIL.', name: 'Ventiladores', states: ON_OFF, at: [0.44, 0.28] },
      { key: 'heat', kind: 'toggle', label: 'CALEF.', name: 'Calefacción', states: ON_OFF, at: [0.44, 0.12] },
      ...(['cockpit', 'cabin', 'lock'] as const).flatMap((z, row): ControlSpec[] => {
        const y = 0.1 - row * 0.15;
        const label = { cockpit: 'CABINA', cabin: 'HABITÁCULO', lock: 'ESCLUSA' }[z];
        const out: ControlSpec[] = [
          { key: `repress.${z}`, kind: 'toggle', label: `REP.${label.slice(0, 3)}`, name: `Represurización manual · ${label.toLowerCase()}`, states: VALVE, at: [0.08, y], requires: 'life' },
          { key: `vent.${z}`, kind: 'toggle', label: `VENT.${label.slice(0, 3)}`, name: `Venteo al vacío · ${label.toLowerCase()}`, states: VALVE, at: [0.32, y], guard: `guard.vent.${z}` },
        ];
        if (z !== 'cabin') out.push({ key: `duct.${z}`, kind: 'toggle', label: 'CONDUC.', name: `Compuerta de ventilación · ${label.toLowerCase()}`, states: OPEN, at: [0.2, y] });
        return out;
      }),
      { key: 'ls.recover', kind: 'toggle', label: 'RECUPERAR', name: 'Compresor: recuperar el aire de la esclusa', states: ON_OFF, at: [-0.4, -0.28], requires: 'life' },
    ],
  },
  {
    id: 'cb.hab',
    title: 'HABITÁCULO',
    on: wallAt(MOD('cabin'), 'R', 1.75, 1.3),
    w: 0.7,
    h: 0.3,
    depth: 0.05,
    controls: [
      { key: 'light.cabin', kind: 'toggle', label: 'LUCES', name: 'Luces del habitáculo', states: ON_OFF, at: [-0.24, -0.03] },
      { key: 'galley', kind: 'toggle', label: 'COCINA', name: 'Cocina', states: ON_OFF, at: [-0.08, -0.03], requires: 'hab' },
      { key: 'recycler', kind: 'toggle', label: 'RECICL.', name: 'Reciclador de agua', states: ON_OFF, at: [0.08, -0.03] },
      { key: 'heat', kind: 'toggle', label: 'CALEF.', name: 'Calefacción', states: ON_OFF, at: [0.24, -0.03] },
    ],
  },
  {
    id: 'cb.lock',
    title: 'ESCLUSA',
    frame: boardFrame([0, 2.13, Z_LK - BULK_T / 2 - 0.01], [0, 0, -1]),
    w: 0.56,
    h: 0.26,
    depth: 0.03,
    indicators: [{ kind: 'airlock', at: [0, 0.06] }],
    controls: [cycle(1, [-0.19, -0.04]), cycle(0, [0.19, -0.04]), { key: 'door.lock', kind: 'button', label: 'PUERTA', name: 'Puerta interior de la esclusa', states: OPEN, at: [0, -0.06], requires: 'doors' }],
  },
  {
    id: 'lk.ctl',
    title: 'ESCLUSA',
    on: boardFrame([0.68, 1.3, Z_TAIL], [0, 0, -1]),
    w: 1.0,
    h: 0.55,
    depth: 0.06,
    screens: [{ id: 'mfd.lock', pages: ['lock', 'atmos'], at: [-0.26, 0.05], w: 0.4, h: 0.26 }],
    indicators: [{ kind: 'airlock', at: [0.2, 0.19] }],
    controls: [
      cycle(1, [0.08, 0.06]),
      cycle(0, [0.32, 0.06]),
      { key: 'door.lock', kind: 'button', label: 'P.INT', name: 'Puerta interior de la esclusa', states: OPEN, at: [0.08, -0.12], requires: 'doors' },
      { key: 'door.ext', kind: 'button', label: 'P.EXT', name: 'Escotilla exterior', states: OPEN, at: [0.32, -0.12], requires: 'doors' },
      { key: 'light.lock', kind: 'toggle', label: 'LUZ', name: 'Luz de la esclusa', states: ON_OFF, at: [-0.4, -0.18] },
      { key: 'repress.lock', kind: 'toggle', label: 'REPRES.', name: 'Represurización manual · esclusa', states: VALVE, at: [-0.26, -0.18], requires: 'life' },
      { key: 'ls.recover', kind: 'toggle', label: 'RECUP.', name: 'Compresor: recuperar el aire de la esclusa', states: ON_OFF, at: [-0.12, -0.18], requires: 'life' },
      { key: 'vent.lock', kind: 'toggle', label: 'VENTEO', name: 'Venteo al vacío · esclusa', states: VALVE, at: [0.44, -0.12], guard: 'guard.vent.lock' },
    ],
  },
  {
    id: 'lk.gas',
    title: 'O2 · N2',
    on: wallAt(MOD('lock'), 'L', 3.7, 1.38),
    w: 0.44,
    h: 0.18,
    depth: 0.05,
    free: true,
    controls: [
      { key: 'v.gasO2', kind: 'valve', label: 'O2', name: 'Válvula de la botella de O₂', states: VALVE, at: [-0.11, -0.01] },
      { key: 'v.gasN2', kind: 'valve', label: 'N2', name: 'Válvula de las botellas de N₂', states: VALVE, at: [0.11, -0.01] },
    ],
  },
  {
    id: 'ext.lock',
    title: 'ESCLUSA',
    frame: boardFrame([1.55 + 0.07, 1.25, 4.7], [1, 0, 0]),
    w: 0.24,
    h: 0.36,
    depth: 0.06,
    indicators: [{ kind: 'airlock', at: [0, 0.12] }],
    controls: [cycle(1, [0, 0.02], 'ABRIR'), cycle(0, [0, -0.12], 'CERRAR')],
  },
  {
    id: 'ext.fuel',
    title: 'REPOSTAJE',
    frame: boardFrame([-1.55 - 0.07, 0.7, 4.4], [-1, 0, 0]),
    w: 0.3,
    h: 0.3,
    depth: 0.06,
    controls: [{ key: 'refuel', kind: 'toggle', label: 'TOMA', name: 'Toma de repostaje (en la plataforma de la base)', states: ['DESCONECTADA', 'CONECTADA'], at: [0, -0.03] }],
  },
];

const ANNUNCIATOR = {
  console: 'ck.main',
  at: [0, 0.2] as V2,
  cols: 5,
  cell: [0.075, 0.034] as V2,
  lamps: ['CASCO', 'DESCOMP', 'O2', 'CO2', 'FUGA AIRE', 'GAS', 'SOP VITAL', 'REACTOR', 'SCRAM', 'REFRIG', 'BATERÍA', 'DESLASTRE', 'DISYUNTOR', 'COMBUST', 'FUGA COMB', 'MOTOR', 'VTOL', 'GRAVEDAD', 'AGUA', 'VÍVERES', 'ESCLUSA'],
};

// the chin under the nose carries the floodlights on its raked face
const CHIN = prop('chin', { id: 'chin', c: [0, -0.2, Z_NOSE - 0.25], half: [1.25, 0.2, 0.25] });
const flood = (x: number) => profilePoint(CHIN, PROFILES.chin, 2, 0.5, x);

function buildDef(): ShipDef {
  const panels = buildPanels();
  const parts = buildParts(PARTS);
  const specs = CONSOLES.map((con) => ({ ...con, controls: con.controls.map((c) => ({ ...c, help: c.help ?? HELP[c.key] })) }));
  const { consoles, controls, screens, indicators } = buildConsoles(specs, panels, parts, []);
  const dash = consoles.find((c) => c.id === 'ck.main')!;
  const props = buildProps([
    supportBlock('dash', dash, 'cockpit'),
    // cabin: galley counter under the pantry, bunks along the port wall, lavatory aft starboard
    prop('galley', { id: 'galley', c: [-1.15, 0.5, 0.6], half: [0.3, 0.5, 0.55], zone: 'cabin' }),
    prop('bunk', { id: 'bunks', c: [-1.03, 0.9, 2.35], half: [0.42, 0.9, 1.0], zone: 'cabin' }),
    prop('lavatory', { id: 'lavatory', c: [0.95, 0.9, 2.8], half: [0.5, 0.9, 0.55], yaw: Math.PI, zone: 'cabin' }),
    // airlock: suit lockers, a handhold on the aft wall; outside a handhold by the hatch
    prop('locker', { id: 'lockers', c: [-1.2, 0.9, 4.35], half: [0.25, 0.9, 0.4], zone: 'lock' }),
    prop('handrail', { id: 'rail.lk', c: [-0.4, 1.05, Z_TAIL - 0.08], yaw: -Math.PI / 2, zone: 'lock' }),
    prop('handrail', { id: 'rail.ext', c: [1.58, 1.1, 3.5], yaw: Math.PI, half: [0.03, 0.05, 0.12] }),
    // boarding stairs under the hatch: a 32° slope (the suit's step-up takes each tread) that runs
    // on below the deck-to-ground height, so uneven ground still meets it
    prop('stairs', { id: 'stairs', c: [1.55 + 1.6, -1.0, 4.1], half: [1.6, 1.0, 0.48], look: { steps: 8 } }),
    // structure: reactor and engine mounts on the aft wall, the chin under the nose
    prop('block', { id: 'rx.mount', c: [-0.95, 0.9, 4.98], half: [0.3, 0.6, 0.08] }),
    prop('block', { id: 'eng.mount', c: [0, 0.95, 4.93], half: [0.5, 0.5, 0.03] }),
    CHIN,
  ]);
  const defaults: Record<string, number> = {
    ...CIRCUIT_DEFAULTS,
    reactor: 1,
    'rx.set': 2,
    coolpump: 1,
    bat: 1,
    solar: 1,
    'light.cockpit': 1,
    'light.cabin': 1,
    'light.lock': 1,
    'light.nav': 1,
    'light.beacon': 1,
    'light.landing': 0,
    // parked with the hatch open for the crew: the airlock empty, the cabin sealed and breathable
    'door.cockpit': 1,
    'door.lock': 0,
    'door.ext': 1,
    'lock.cycle': 1,
    gear: 1,
    'v.tank': 1,
    'pump.tank': 1,
    'v.eng': 1,
    'v.rcs': 1,
    rcs: 1,
    lift: 1,
    'v.lift': 1,
    'eng.arm': 0,
    'helm.throttle': 0,
    'fa.sas': 1,
    'fa.hold': 1,
    'fa.land': 1,
    ...AP.defaults,
    grav: 1,
    refuel: 0,
    'ls.mode': 0,
    o2gen: 1,
    scrub: 1,
    fans: 1,
    heat: 1,
    'v.gasO2': 1,
    'v.gasN2': 1,
    'ls.recover': 0,
    'duct.cockpit': 1,
    'duct.lock': 0,
    recycler: 1,
    galley: 0,
    'cockpit.p0': 70,
    'cabin.p0': 70,
    'mfd.pilot': 0,
    'mfd.aux': 0,
    'mfd.life': 0,
    'mfd.lock': 0,
  };

  return finishShip({
    id: 'peregrina',
    name: 'Peregrina',
    registry: 'PRG-2',
    role: 'Lanzadera de pasaje de largo recorrido',
    floorHeight: 1.2,
    panels,
    controls,
    consoles,
    screens,
    doors: DOORS,
    gear: {
      key: 'gear',
      legs: [
        [-1.1, -0.3, -3.6],
        [1.1, -0.3, -3.6],
        [-1.3, -0.3, 3.0],
        [1.3, -0.3, 3.0],
      ],
    },
    zones: [
      { id: 'cockpit', label: 'CABINA', min: [-1.15, 0, Z_NOSE], max: [1.15, 2.1, Z_CK], lights: [[0, 1.95, -3.4]], lightKey: 'light.cockpit' },
      { id: 'cabin', label: 'HABITÁCULO', min: [-1.45, 0, Z_CK], max: [1.45, 2.35, Z_LK], lights: [[0, 2.2, -1.2], [0, 2.2, 1.8]], lightKey: 'light.cabin', lux: 4.6 },
      { id: 'lock', label: 'ESCLUSA', min: [-1.45, 0, Z_LK], max: [1.45, 2.35, Z_TAIL], lights: [[0, 2.2, 4.1]], lightKey: 'light.lock' },
    ],
    seats: [
      { id: 'ck.pilot', name: 'Asiento del piloto', root: [0, 0, -3.55], yaw: 0, exit: [0.72, 0, -3.2] },
      { id: 'cb.p1', name: 'Asiento de pasajero izquierdo', root: [-0.75, 0, -1.45], yaw: 0, exit: [-0.15, 0, -1.4] },
      { id: 'cb.p2', name: 'Asiento de pasajero derecho', root: [0.75, 0, -1.45], yaw: 0, exit: [0.15, 0, -1.4] },
    ],
    // passengers' luggage between the water recycler and the lavatory
    cargo: [
      { pos: [0.95, 0.2, 1.75], half: [0.25, 0.2, 0.22], yaw: 0, mass: 22, paint: 'grey' },
      { pos: [0.95, 0.57, 1.72], half: [0.2, 0.16, 0.18], yaw: 0.15, mass: 14, paint: 'orange' },
    ],
    extLights: [
      { kind: 'nav-red', pos: [-1.55, 1.2, -0.5], n: [-1, 0, 0] },
      { kind: 'nav-green', pos: [1.55, 1.2, -0.5], n: [1, 0, 0] },
      { kind: 'strobe', pos: [0, 1.4, 6.3], n: [0, 1, 0] },
      { kind: 'beacon', pos: [0, 2.45, -1.2], n: [0, 1, 0] },
      { kind: 'beacon', pos: [0, -0.45, -2.0], n: [0, -1, 0] },
      { kind: 'landing', ...flood(-0.6), dir: norm([-0.1, -0.55, -1]) },
      { kind: 'landing', ...flood(0.6), dir: norm([0.1, -0.55, -1]) },
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
    airlock: { zone: 'lock', inner: 'door.lock', outer: 'door.ext', key: 'lock.cycle', circuit: 'doors', duct: 'duct.lock' },
    helm: { throttle: 'helm.throttle', seat: 'ck.pilot' },
    caution: 'caution',
    readouts: { flight: ['fa.sas', 'fa.hold', 'fa.land', 'helm.throttle'], engines: ['rcs', 'lift', 'helm.throttle'] },
    autopilot: AP.autopilot,
    // a light shuttle: quicker and more nimble than the freighter
    flight: { vmax: 28, vside: 9, vz: 5.5, rate: [0.45, 0.55, 0.65], accel: 3.2 },
    annunciator: ANNUNCIATOR,
    defaults,
    modules: MODULES,
    bounds: { min: [-1.9, -2.0, -5.8], max: [4.8, 3.1, 6.9] },
    indicators,
    livery: {
      hull: [0.55, 0.56, 0.58],
      stripe: [0.02, 0.2, 0.24],
      accent: [0.55, 0.38, 0.03],
      tagline: 'PASAJE · LARGO RECORRIDO',
      decals: [
        { c: [-1.56, 1.35, 0.9], n: [-1, 0, 0], w: 2.2, h: 0.55 },
        { c: [1.56, 1.35, -0.2], n: [1, 0, 0], w: 2.2, h: 0.55 },
      ],
    },
    manual: MANUAL,
  });
}

const MANUAL: ManualSection[] = [
  {
    id: 'start',
    title: 'Primeros pasos',
    lead: 'Cómo se usa un mando y qué dicen sus luces',
    body: [
      'Todo en la Peregrina es físico: mira un mando hasta que se ilumine y pulsa clic izquierdo o E. El casco te dice su nombre, su posición y, si se niega, por qué.',
      'Los selectores (ruedas) avanzan una posición con cada clic; la rueda del ratón los gira en los dos sentidos. Las escalas (potencia, acelerador) se paran en el extremo.',
      { fig: 'cover' },
      { fig: 'leds' },
      { note: 'En este manual, los nombres en azul llevan a la ficha del mando. En cada ficha, «Señalar» pone una marca en tu casco que te lleva hasta él.' },
    ],
  },
  {
    id: 'ship',
    title: 'La nave',
    lead: 'Compartimentos, equipo y dónde está cada consola',
    body: [
      'La Peregrina es una lanzadera de pasaje para viajes largos: un piloto y dos pasajeros. Tres compartimentos presurizados: la cabina de pilotaje, el habitáculo (dos asientos, dos literas, cocina, aseo, soporte vital y reciclado de agua) y la esclusa, con la escotilla en el costado de estribor.',
      'Por fuera, en la cola: el micro-reactor RX-20 con su bomba de refrigerante y el motor principal. En el techo: los radiadores fijos y las alas solares plegables. Bajo la cubierta, el depósito de propelente y el compensador inercial; bajo la panza, los cuatro propulsores de sustentación VTOL.',
      { fig: 'plan' },
      { note: 'Pulsa una consola del plano para ir a sus mandos.' },
    ],
  },
  {
    id: 'lock',
    title: 'La esclusa',
    lead: 'Entrar y salir sin perder el aire',
    body: [
      'La nave se aparca con la escotilla abierta y la esclusa vacía; la cabina y el habitáculo conservan su aire. Para entrar: sube la escalerilla, entra en la esclusa y pulsa ENTRAR ([[lk.ctl/lock.cycle=0]]). La escotilla se cierra, la esclusa se llena y la puerta interior se abre.',
      { fig: 'airlock' },
      { live: 'airlock' },
      {
        steps: [
          'Salir: desde el habitáculo ([[cb.lock/lock.cycle=1]]) o desde dentro de la esclusa ([[lk.ctl/lock.cycle=1]]).',
          'El ciclo cierra la puerta interior, recupera el aire con el compresor (unos 20 s) y abre la escotilla.',
          'Desde fuera, junto a la escotilla, ABRIR y CERRAR ([[ext.lock/lock.cycle=1]], [[ext.lock/lock.cycle=0]]) hacen lo mismo.',
        ],
      },
      { warn: 'El ciclo necesita energía en PUERTAS y SOPORTE VITAL. Si se detiene (alarma ESCLUSA), termínalo a mano: puertas, compresor y válvulas están en la consola de la esclusa.' },
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
          'Conecta la batería ([[ck.r/bat]] o el aislador sobre ella, [[ck.bat/bat]]). Comprueba los disyuntores en el cuadro eléctrico ([[cb.pwr/brk.cool]]).',
          'Enciende la bomba de refrigerante ([[ck.r/coolpump]]).',
          'Sube la palanca del reactor ([[ck.r/reactor]]) y elige la salida ([[ck.r/rx.set]]). Unos 10 s hasta EN MARCHA.',
          'Despliega las alas solares ([[ck.r/solar]]): con sol, cubren el consumo básico y ahorran reactor.',
        ],
      },
      { live: 'reactor' },
      { warn: 'Sin APU a bordo: si la batería se agota con el reactor parado, solo las alas solares pueden devolver energía, y despacio. No dejes que la batería se vacíe.' },
    ],
  },
  {
    id: 'air',
    title: 'Aire y presión',
    lead: 'Presurizar, ventilar, respirar',
    body: [
      'El soporte vital está en el habitáculo: generador de O₂ y depurador de CO₂ en el costado de estribor, botellas de O₂ y N₂ en la esclusa. Los ventiladores llevan el aire a la cabina por su conducto.',
      { fig: 'air' },
      { live: 'air' },
      'En AUTOMÁTICO ([[cb.ls/ls.mode]]) cada compartimento estanco se mantiene a 70 kPa con gas de las botellas ([[lk.gas/v.gasO2]], [[lk.gas/v.gasN2]]). El automático no mete gas donde hay una fuga: sella primero.',
      { controls: ['ls.mode', 'o2gen', 'scrub', 'fans', 'heat', 'duct.cockpit', 'duct.lock', 'ls.recover'] },
    ],
  },
  {
    id: 'stores',
    title: 'Víveres y agua',
    lead: 'Viajes largos: agua, comida y reciclado',
    body: [
      'Cada persona a bordo bebe y come. El agua usada pasa al reciclador ([[cb.hab/recycler]]), que devuelve el 90 % al depósito; sin él el agua dura diez veces menos. La despensa está sobre la cocina.',
      { live: 'stores' },
      'La página VÍVER de las pantallas lleva la cuenta: agua, raciones, reciclado y la energía de las alas solares.',
      { controls: ['recycler', 'galley'] },
    ],
  },
  {
    id: 'power',
    title: 'Energía',
    lead: 'Reactor, sol, batería, prioridades y disyuntores',
    body: [
      'Fuentes: el micro-reactor RX-20 (hasta 22 kW), las alas solares (3,5 kW desplegadas al sol) y la batería (6 kWh). La energía solar se usa primero; la batería cubre lo que falta y se carga con lo que sobra.',
      { fig: 'power' },
      { live: 'power' },
      'El cuadro eléctrico está en la pared de babor del habitáculo, junto a la puerta de la cabina: un disyuntor y un selector de prioridad por circuito. La cocina va en prioridad BAJA: es lo primero que se corta.',
      { fig: 'circuits' },
    ],
  },
  {
    id: 'reactor',
    title: 'Reactor',
    lead: 'Arranque, potencia, temperatura y SCRAM',
    body: [
      'El RX-20 va fuera del casco presurizado, en la cola, a babor del motor; se maneja desde la consola derecha de la cabina. Da 20 kW al 100 % y sus radiadores fijos del techo evacuan el calor sin mecanismos.',
      { live: 'reactor' },
      { controls: ['reactor', 'rx.set', 'coolpump', 'rx.scram', 'rx.reset'] },
      { warn: 'Para soldarlo hay que salir: está en la cola, sobre su bomba de refrigerante.' },
    ],
  },
  {
    id: 'drive',
    title: 'Motor y propelente',
    lead: 'Armar, arrancar, acelerar',
    body: [
      { fig: 'drive' },
      {
        steps: [
          'Abre la tapa y arma el motor ([[ck.l/eng.arm]]). Válvulas del depósito y del motor abiertas ([[ck.l/v.tank]], [[ck.l/v.eng]]).',
          'Pulsa ARRANQUE ([[ck.l/eng.start]]). Unos 3 s hasta régimen.',
          'El acelerador ([[ck.main/helm.throttle]]) es el tope del motor que puede usar el ordenador de vuelo (ACOPLADO y piloto automático) o cuánto empuja (DESACOPLADO). La página MASA dice cuánto empuja, cuánto pesa la nave y el Δv que queda.',
        ],
      },
      { live: 'fuel' },
      'El motor no sostiene la nave: eso lo hacen los propulsores de sustentación. Para volar mira el capítulo «Vuelo».',
    ],
  },
  {
    id: 'flight',
    title: 'Vuelo',
    lead: 'Despegar, volar, aterrizar y el piloto automático',
    body: [
      'La Peregrina se sostiene con cuatro propulsores de sustentación bajo la panza (VTOL), corre con el motor principal y afina los giros con los RCS. El ordenador de vuelo lo reparte todo: desde el asiento del piloto tú pides velocidades y giros, no empujes.',
      {
        steps: [
          'Siéntate en el asiento del piloto. La sustentación y su alimentación vienen encendidas ([[ck.l/lift]], [[ck.l/v.lift]]).',
          'R sube, F baja. W/S adelante y atrás, A/D gira el morro, Z/C desplaza de lado; las flechas cabecean y alabean. Suéltalo todo y la nave frena y se queda a esa altura (vuelo ACOPLADO, [[ck.l/fa.hold]]).',
          'Para correr: arma y arranca el motor ([[ck.l/eng.arm]], [[ck.l/eng.start]]), despega y sube el acelerador ([[ck.main/helm.throttle]]). ACOPLADO, W pide velocidad y el motor ayuda hasta el tope del acelerador; para una velocidad fija usa VEL del piloto automático; DESACOPLADO, el acelerador es el empuje directo.',
          'Tren arriba en vuelo ([[ck.main/gear]]) y abajo antes de posarte. Cerca del suelo el asistente ([[ck.l/fa.land]]) frena la bajada y nivela la nave.',
        ],
      },
      { live: 'flight' },
      'Piloto automático, en el tablero sobre el salpicadero: [[ck.ap/ap.on]] lo conecta (tecla P desde el asiento). Cada modo tiene su botón; cualquier mando tuyo manda en su eje mientras lo pulsas.',
      { controls: ['ap.on', 'ap.lvl', 'ap.alt', 'ap.hdg', 'ap.spd', 'ap.nav', 'ap.to', 'ap.land', 'ap.alt.sel', 'ap.hdg.sel', 'ap.spd.sel', 'ap.wp'] },
      ...AP_SPACE_MANUAL,
      { note: 'Ir a un sitio: elige el punto ([[ck.ap/ap.wp]]) y la altura ([[ck.ap/ap.alt.sel]]), pulsa DESPEG. y luego NAV. Al llegar se queda en vuelo estacionario encima; ATERRIZ. la posa y se desconecta.' },
      { warn: 'Sin energía en AVIÓNICA se apaga el ordenador de vuelo: mando DIRECTO, sin estabilizador, sin piloto automático y sin compensar la gravedad. Baja despacio.' },
      'El compensador inercial ([[ck.r/grav]]) mantiene el suelo como «abajo» a bordo: con él, los pasajeros caminan y el equipaje se queda donde está aunque la nave se incline o frene. Apagado, se nota todo.',
    ],
  },
  SPACE_MANUAL,
  {
    id: 'damage',
    title: 'Daños y reparación',
    lead: 'Brechas, ojos de buey y máquinas',
    body: [
      'Las explosiones dañan los paneles del casco y las máquinas cercanas. Los ojos de buey del habitáculo son de vidrio: si revientan, el habitáculo se vacía en segundos. Cierra las puertas para salvar la cabina y la esclusa.',
      'La soldadora (tecla 2) repara paneles y máquinas. Con ella en la mano las máquinas se tiñen según su daño.',
      ...decompressionManual(),
      { warn: 'El depósito de propelente bajo la cubierta y el motor dañado pueden explotar. Aíslalos antes de soldar.' },
    ],
  },
  { id: 'equipment', title: 'Equipo', lead: 'Cada máquina de a bordo, del catálogo estándar', body: ['Todas las máquinas son componentes del catálogo estándar: tamaño, fabricante y números.', { fig: 'equipment' }] },
  {
    id: 'alerts',
    title: 'Alarmas',
    lead: 'Qué significa cada lámpara y qué hacer',
    body: ['Cualquier alarma nueva enciende la alarma general ([[ck.main/caution]]): púlsala para reconocerla. Ámbar es precaución, rojo parpadeante es aviso.', { fig: 'alerts' }],
  },
  { id: 'screens', title: 'Pantallas', lead: 'Qué muestra cada página', body: ['Las pantallas multifunción cambian de página con los botones de debajo. Van con el circuito AVIÓNICA.', { fig: 'pages' }] },
  { id: 'consoles', title: 'Todos los mandos', lead: 'Consola por consola', body: [{ fig: 'consoles' }] },
];

export const PEREGRINA = buildDef();
