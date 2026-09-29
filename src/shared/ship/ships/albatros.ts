// "Albatros" heavy two-deck transport. Upper deck (crew): bridge with a wrap-around canopy → mess
// (galley, tables, water) → crew quarters. Lower deck: engineering room (reactor, life support,
// batteries, gas; the crew airlock nested in its starboard aft corner, the stairwell up to the mess
// along its port wall) → a cargo hold that runs under the upper deck and opens to full height aft,
// with a rear ramp. Outside: engine pods and propellant pods on the flanks, six heavy VTOL lift
// pads under the belly, radiator wings and a solar wing on the roof. Built from catalog components.
//
// Ship space: metres, +X starboard, +Y up, nose toward −Z, origin = lower deck level on the
// centreline. The upper deck's plates top out at D2; its walls start on the lower deck's ceiling.

import { part, prop, profilePoint, PROFILES } from '../catalog/index.js';
import {
  boardFrame,
  boxFace,
  buildConsoles,
  buildParts,
  buildProps,
  bulkheadAt,
  doorSensor,
  finishShip,
  hatchSensor,
  outsidePoint,
  paintPanels,
  rampSensor,
  roofAt,
  ShipBuilder,
  supportBlock,
  wallAt,
  type CargoDef,
  type CompartmentDef,
  type ConsoleSpec,
  type ControlSpec,
  type DoorDef,
  type FluidNetDef,
  type HatchDef,
  type LifeSupportDef,
  type LoadDef,
  type ManualSection,
  type ModuleDef,
  type MoverDef,
  type OpeningDef,
  type PartSpec,
  type RampDef,
  type Rect,
  type ShipDef,
} from '../def.js';
import { clipPoly, norm, type V2, type V3 } from '../geom.js';
import { AP_SPACE_MANUAL, SPACE_MANUAL, autopilotConsole, breakerControls, circuits, decompressionManual, doorButtons, HELP as KIT, loadOf, priorityControls, type CircuitSpec } from './kit.js';

const T = 0.1; // hull skin
const FLOOR_T = 0.08;
const BULK_T = 0.08;

// Decks and cross-sections (interior surface, x ascending along the top)
const W = 3.4; // interior half-width
const D2 = 2.7; // upper deck: top of its plates
const CEIL = D2 - FLOOR_T; // lower deck ceiling = where the upper deck's walls start
const EAVE = 4.5;
const ROOF = 5.2;
/** Lower deck under the upper deck: walls only, the upper deck is its ceiling. */
const LOW: V2[] = [[-W, 0], [-W, CEIL], [W, CEIL], [W, 0]];
/** Upper deck: walls, roof chamfers and a two-plate roof. */
const UP: V2[] = [[-W, CEIL], [-W, EAVE], [-2.7, ROOF], [0, ROOF], [2.7, ROOF], [W, EAVE], [W, CEIL]];
/** The aft hold: both decks' height in one space. */
const FULL: V2[] = [[-W, 0], [-W, EAVE], [-2.7, ROOF], [0, ROOF], [2.7, ROOF], [W, EAVE], [W, 0]];
const DOOR_LOW: V2[] = [[-0.6, 0], [-0.6, 2.05], [0.6, 2.05], [0.6, 0]];
const DOOR_UP: V2[] = [[-0.6, CEIL], [-0.6, D2 + 2.05], [0.6, D2 + 2.05], [0.6, CEIL]];
const RAMP_OPENING: V2[] = [[-2.2, 0], [-2.2, 4.0], [2.2, 4.0], [2.2, 0]];
const UP_LABELS = ['L', 'LC', 'TL', 'TR', 'RC', 'R'];
const UP_ROWS = [2, 1, 1, 1, 1, 2];

// Stations along the ship
const Z_NOSE = -13; // bridge nose (the upper deck overhangs the lower one by a metre)
const Z_BOW = -12; // lower deck nose
const Z_BR = -9; // bridge | mess
const Z_EH = -2.5; // engineering | hold (lower deck)
const Z_MQ = 0; // mess | quarters
const Z_UA = 5; // upper deck aft bulkhead: the hold is full height behind it
const Z_TAIL = 12;

// The crew airlock: a room nested in the starboard aft corner of the engineering room
const LOCK = { x0: 1.2, z0: -5.2, z1: Z_EH };
const LOCK_Z = -3.85; // its two doors' centre line

// The stairwell: stairs along the port wall of engineering rising toward the nose into the mess;
// the hole in the upper deck starts where a suited head would hit the deck above
const STAIR = { x: -2.8, w: 1.2, zBottom: -3.5, zTop: -7.9 };
const WELL: Rect = { x0: -W, x1: STAIR.x + STAIR.w / 2, z0: -8.0, z1: -4.2 };

// Windshield plane: from the dash (0.95 m above the upper deck) raked back to the roof
const WS_BOTTOM = D2 + 0.95;
const WS_RAKE = 0.8 / 1.3;
const NOSE_CLIP = { origin: [0, WS_BOTTOM, Z_NOSE] as V3, n: norm([0, 0.8, -1.3]) };
const wsZ = (y: number) => Z_NOSE + (y - WS_BOTTOM) * WS_RAKE;

// Upper deck sections first (the nose section draws the shorter light strips), then the lower ones
export const MODULES: ModuleDef[] = [
  { zone: 'bridge', z0: Z_NOSE, z1: Z_BR, profile: UP, cols: 2 },
  { zone: 'mess', z0: Z_BR, z1: Z_MQ, profile: UP, cols: 4 },
  { zone: 'quarters', z0: Z_MQ, z1: Z_UA, profile: UP, cols: 2 },
  { zone: 'eng', z0: Z_BOW, z1: Z_EH, profile: LOW, cols: 4 },
  { zone: 'hold', z0: Z_EH, z1: Z_UA, profile: LOW, cols: 3 },
  { zone: 'hold', z0: Z_UA, z1: Z_TAIL, profile: FULL, cols: 3 },
];
const [M_BR, M_MS, M_QT, M_EN, , M_HA] = MODULES;

/** Outer hatch of the airlock, in the starboard wall. */
const HATCH = { c: [W, 0, LOCK_Z] as V3, n: [1, 0, 0] as V3, w: 1.0, h: 2.05 };

/** The compartment under a point of the upper deck. */
const below = (x: number, z: number): string | undefined => {
  if (z < Z_BOW) return undefined; // the bridge overhang: vacuum under it
  if (z < Z_EH) return x > LOCK.x0 && z > LOCK.z0 ? 'lock' : 'eng';
  return 'hold';
};

function buildPanels() {
  const B = new ShipBuilder();
  // ---- upper deck ---------------------------------------------------------------------------------
  // bridge: canopy windows in the forward column (upper wall row + chamfers), the raked windshield
  B.strip(M_BR, 'BR', UP_LABELS, UP_ROWS, (s, r, c) => (c === 0 && (((s === 'L' || s === 'R') && r === 1) || s === 'LC' || s === 'RC') ? 'glass' : 'hull'), T, NOSE_CLIP);
  B.cap('bridge', 'BR-N', Z_NOSE, UP, null, { outwardZ: -1, kind: 'hull', t: T, rowH: 1.3, splitX: [0], clip: NOSE_CLIP, face: 'inner' });
  const above = UP.filter((p) => p[1] > WS_BOTTOM);
  const ws: V3[] = [[-W, WS_BOTTOM, wsZ(WS_BOTTOM)], ...above.map((p): V3 => [p[0], p[1], wsZ(p[1])]), [W, WS_BOTTOM, wsZ(WS_BOTTOM)]];
  B.addPoly(clipPoly(ws, [0, 0, 0], [1, 0, 0]), [0, 0.5, -1], { id: 'BR-W1', kind: 'glass', zone: 'bridge', t: 0.05, face: 'inner' });
  B.addPoly(clipPoly(ws, [0, 0, 0], [-1, 0, 0]), [0, 0.5, -1], { id: 'BR-W2', kind: 'glass', zone: 'bridge', t: 0.05, face: 'inner' });
  B.cap('bridge', 'BK1', Z_BR, UP, DOOR_UP, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.3, face: 'mid', other: 'mess' });
  // mess: a window over the stairwell (port) and two by the tables (starboard)
  B.strip(M_MS, 'MS', UP_LABELS, UP_ROWS, (s, r, c) => (r === 1 && ((s === 'L' && c === 1) || (s === 'R' && (c === 1 || c === 2))) ? 'glass' : 'hull'), T);
  B.cap('mess', 'BK2', Z_MQ, UP, DOOR_UP, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.3, face: 'mid', other: 'quarters' });
  // quarters: a closed box; its aft wall looks over the full-height hold
  B.strip(M_QT, 'QT', UP_LABELS, UP_ROWS, () => 'hull', T);
  B.cap('quarters', 'BK3', Z_UA, UP, null, { outwardZ: 1, kind: 'bulkhead', t: BULK_T, rowH: 1.3, splitX: [0], face: 'mid', other: 'hold' });
  // the upper deck: every plate knows which compartment is under it; the stairwell is left open
  const upX = [-W, WELL.x1, -1.0, 0, LOCK.x0, 2.3, W];
  B.deck('bridge', 'BR', { y: D2, xs: upX, zs: [Z_NOSE, Z_BOW, -10.5, Z_BR], t: FLOOR_T, other: below });
  B.deck('mess', 'MS', { y: D2, xs: upX, zs: [Z_BR, WELL.z0, -6.6, LOCK.z0, WELL.z1, Z_EH, -1.2, Z_MQ], t: FLOOR_T, holes: [WELL], other: below });
  B.deck('quarters', 'QT', { y: D2, xs: upX, zs: [Z_MQ, 1.7, 3.4, Z_UA], t: FLOOR_T, other: below });

  // ---- lower deck ---------------------------------------------------------------------------------
  const lowWalls = (s: string) => (s === 'T' ? null : 'hull');
  B.strip(M_EN, 'EN', ['L', 'T', 'R'], [2, 1, 2], lowWalls, T);
  B.cap('eng', 'EN-N', Z_BOW, LOW, null, { outwardZ: -1, kind: 'hull', t: T, rowH: 1.31, splitX: [0], face: 'inner' });
  B.deck('eng', 'EN', { y: 0, xs: [-W, WELL.x1, -1.0, 0, LOCK.x0, 2.3, W], zs: [Z_BOW, -10.1, -8.2, -6.7, LOCK.z0, LOCK_Z, Z_EH], t: FLOOR_T });
  B.cap('eng', 'BK4', Z_EH, LOW, DOOR_LOW, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.31, splitX: [LOCK.x0], face: 'mid', other: 'hold' });
  B.strip(MODULES[4], 'HL', ['L', 'T', 'R'], [2, 1, 2], lowWalls, T);
  B.strip(M_HA, 'HA', UP_LABELS, [3, 1, 1, 1, 1, 3], () => 'hull', T);
  B.cap('hold', 'HA-AFT', Z_TAIL, FULL, RAMP_OPENING, { outwardZ: 1, kind: 'hull', t: T, rowH: 1.5, face: 'inner' });
  B.floor('hold', 'HD', -W, W, Z_EH, Z_TAIL, 4, 6, FLOOR_T);

  // the airlock, nested in engineering: two partitions, and everything of engineering's inside its
  // box (the starboard wall, the deck plates, the aft bulkhead's corner) becomes the lock's
  B.partition('lock', 'eng', 'LK-P', [LOCK.x0, LOCK.z0], [LOCK.x0, LOCK.z1], { y0: 0, y1: CEIL, t: BULK_T, cols: 2, rows: 2 });
  B.partition('lock', 'eng', 'LK-F', [LOCK.x0, LOCK.z0], [W, LOCK.z0], { y0: 0, y1: CEIL, t: BULK_T, cols: 1, rows: 2 });
  B.splitAt([0, 0, LOCK.z0], [0, 0, 1], (p) => p.zone === 'eng' && p.kind === 'hull' && p.c[0] > W - 0.2);
  const inLock = { min: [LOCK.x0 + 0.02, -0.5, LOCK.z0 + 0.02] as V3, max: [W + 0.5, CEIL + 0.02, LOCK.z1 + 0.1] as V3 };
  B.assign(inLock, { zone: 'lock' }, ['hull', 'floor']);
  B.assign({ min: [LOCK.x0 + 0.02, -0.5, Z_EH - 0.2], max: [W + 0.5, CEIL, Z_EH + 0.2] }, { zone: 'lock' }, ['bulkhead']);
  B.cutOpening('lock', { c: [LOCK.x0, 0, LOCK_Z], n: [1, 0, 0], w: 1.0, h: 2.05 });
  B.cutOpening('lock', HATCH);

  // livery: charcoal lower deck belt and nose caps, a blue band along the upper deck's lower row,
  // hazard stripes round the ramp
  return paintPanels(B.panels, [
    { scheme: 2, when: (p) => p.id.startsWith('HA-AFT') && p.c[1] < 1.6 },
    { scheme: 1, when: (p) => /^(EN|HL|HA|LK)-?/.test(p.id) && /-(L|R)1-/.test(p.id) },
    { scheme: 1, when: (p) => p.id.startsWith('BR-N') || p.id.startsWith('EN-N') },
    { scheme: 3, when: (p) => /^(BR|MS|QT)-(L|R)1-/.test(p.id) },
  ]);
}

// -----------------------------------------------------------------------------------------------
// Power circuits: the breaker board is on engineering's starboard wall. Lower deck loads run under
// the deck; upper deck circuits climb the cable riser in the mess's forward starboard corner and
// run along the upper deck's ceiling (blow that corner out and half the ship goes dark).
// -----------------------------------------------------------------------------------------------

const BOARD_Z = -7.6;
const RISER = (x: number, z: number, to: V3[]): V3[] => [[3.3, 1.7, z], [x, 2.45, z], [x, 2.45, -8.45], [x, 4.95, -8.45], ...to];
const UNDER = (z: number, to: V3[]): V3[] => [[3.3, 1.0, z], [3.0, -0.16, z], ...to];
const CIRCUIT_LIST: CircuitSpec[] = [
  {
    id: 'lights',
    label: 'ILUMINACIÓN',
    short: 'ILUMIN.',
    desc: 'Luces de las seis salas.',
    color: 0xffd36a,
    rating: 5,
    base: 0.1,
    pri: 1,
    routes: [
      [[3.3, 1.7, -7.9], [3.1, 2.45, -7.9], [0.2, 2.45, -7.9], [0.2, 2.45, -11.6]],
      [[0.2, 2.45, -7.9], [0.2, 2.45, 4.9], [0.2, 5.05, 5.3], [0.2, 5.05, 11.5]],
      RISER(3.25, -7.9, [[0.2, 5.05, -8.45], [0.2, 5.05, -12.3]]),
      [[0.2, 5.05, -8.45], [0.2, 5.05, 4.7]],
    ],
  },
  { id: 'ext', label: 'LUCES EXTERIORES', short: 'EXTERIOR', desc: 'Luces de navegación, balizas y focos de aterrizaje.', color: 0x7fd6ff, rating: 5, base: 0.05, pri: 2, routes: [UNDER(-7.3, [[0.45, -0.16, -7.3], [0.45, -0.16, -11.9]]), RISER(3.2, -7.3, [[-0.2, 5.05, -8.45], [-0.2, 5.05, 11.5]])] },
  {
    id: 'doors',
    label: 'PUERTAS',
    short: 'PUERTAS',
    desc: 'Puertas, escotilla de la escalera, escotilla exterior y secuenciador de la esclusa.',
    color: 0x9cff8a,
    rating: 6,
    base: 0.1,
    pri: 1,
    routes: [
      [[3.3, 1.5, -7.2], [3.0, 2.45, -7.2], [1.4, 2.45, -5.0], [1.3, 2.2, -4.4]],
      [[3.0, 2.45, -7.2], [3.3, 2.3, -4.5]],
      [[1.4, 2.45, -5.0], [0.7, 2.45, -2.6]],
      [[1.4, 2.45, -5.0], [-2.1, 2.5, -6.1]],
      RISER(3.15, -7.2, [[0.8, 5.05, -8.45], [0.8, 5.05, -9.1]]),
      [[0.8, 5.05, -8.45], [0.8, 5.05, -0.1]],
    ],
  },
  {
    id: 'avionics',
    label: 'AVIÓNICA',
    short: 'AVIÓNICA',
    desc: 'Pantallas multifunción, instrumentos y ordenador de vuelo.',
    color: 0xb58cff,
    rating: 5,
    base: 1.8,
    pri: 0,
    routes: [RISER(3.1, -7.7, [[-0.6, 5.05, -8.45], [-0.6, 5.05, -12.3]]), [[-0.6, 5.05, -10.0], [-3.1, 4.3, -10.0]], [[-0.6, 5.05, -10.4], [3.1, 4.3, -10.4]]],
  },
  {
    id: 'prop',
    label: 'PROPULSIÓN',
    short: 'PROPULS.',
    desc: 'Motores, APU, sustentación VTOL, RCS, bombas de refuerzo y transferencia.',
    color: 0xff7a3a,
    rating: 14,
    base: 0.2,
    pri: 0,
    routes: [
      UNDER(-7.8, [[-0.9, -0.16, -7.8], [-0.9, -0.16, 11.6]]),
      [[-0.9, -0.16, 3.0], [-3.3, -0.16, 3.0], [-3.4, 1.1, 3.0]],
      [[-0.9, -0.16, 3.0], [3.3, -0.16, 3.0], [3.4, 1.1, 3.0]],
      [[-0.9, -0.16, -7.8], [-0.9, -0.16, -11.8]],
    ],
  },
  {
    id: 'life',
    label: 'SOPORTE VITAL',
    short: 'SOP.VITAL',
    desc: 'Generador de O₂, depurador, ventiladores, calefacción, compresor, válvulas de gas y umbilicales de los asientos.',
    color: 0x5cf2d2,
    rating: 16,
    base: 0.3,
    pri: 0,
    routes: [UNDER(-7.95, [[-2.6, -0.16, -8.7], [-2.85, 0.05, -8.7]]), [[-2.6, -0.16, -8.7], [-2.9, -0.16, -11.0], [-3.1, 0.1, -11.0]], RISER(3.05, -7.95, [[2.6, 5.05, -8.45], [2.6, 5.05, -10.4]])],
  },
  { id: 'cool', label: 'REFRIGERACIÓN', short: 'REFRIG.', desc: 'Bomba de refrigerante, radiadores y arranque del reactor.', color: 0x4f9dff, rating: 12, base: 0.1, pri: 0, routes: [UNDER(-8.05, [[0, -0.16, -8.05], [0, -0.16, -9.3], [0, 0.05, -9.3]]), RISER(3.0, -8.05, [[1.4, 5.05, -8.45], [1.4, 5.05, 5.3], [1.4, 5.3, 6.5]])] },
  { id: 'hab', label: 'HABITABILIDAD', short: 'HABITAB.', desc: 'Cocina, reciclador de agua y servicios de las cubiertas de tripulación.', color: 0xf0a0ff, rating: 6, base: 0.1, pri: 2, routes: [RISER(2.95, -7.0, [[2.0, 5.05, -8.45], [2.0, 5.05, -2.0], [2.9, 4.4, -2.0]]), [[2.0, 5.05, -2.0], [-2.9, 4.4, -2.0]]] },
  { id: 'grav', label: 'COMPENSADOR INERCIAL', short: 'GRAVEDAD', desc: 'Compensador inercial bajo la cubierta de la sala de máquinas.', color: 0xc07fff, rating: 10, base: 0.05, pri: 1, routes: [UNDER(-7.1, [[0.6, -0.16, -7.1], [0.4, -0.2, -6.5]])] },
  { id: 'sensors', label: 'SENSORES', short: 'SENSORES', desc: 'Radar de techo.', color: 0xe0e070, rating: 5, base: 0.2, pri: 1, routes: [RISER(2.9, -6.9, [[-1.0, 5.05, -8.45], [-1.0, 5.05, -10.6], [0, 5.3, -10.6]])] },
  { id: 'hyd', label: 'HIDRÁULICA', short: 'HIDRÁUL.', desc: 'Rampa de carga, tren de aterrizaje y ala solar.', color: 0xff9a5c, rating: 12, base: 0.2, pri: 1, routes: [UNDER(-7.2, [[1.0, -0.16, -7.2], [1.0, -0.16, 11.6]]), [[1.0, -0.16, -7.2], [1.0, -0.16, -11.5]], RISER(2.85, -7.2, [[-1.4, 5.05, -8.45], [-1.4, 5.05, 2.5], [0, 5.3, 2.5]])] },
];
const { subsystems: SUBSYSTEMS, defaults: CIRCUIT_DEFAULTS, short: SHORT } = circuits(CIRCUIT_LIST);

// -----------------------------------------------------------------------------------------------
// Machinery: catalog components, placed
// -----------------------------------------------------------------------------------------------

const onRoof = (m: ModuleDef, x: number, z: number, rise: number): V3 => outsidePoint(roofAt(m, x, z), T, rise);
const SIDEWAYS = Math.PI / 2; // against a side wall: the component's depth runs across the ship
const POD_X = W + T + 1.0 + 0.05; // flank pods: a radius off the skin

const LIFTS: Array<[string, string, V3]> = [
  ['lift.FL', 'Sustentación delantera izq.', [-1.9, -0.69, -8.5]],
  ['lift.FR', 'Sustentación delantera der.', [1.9, -0.69, -8.5]],
  ['lift.ML', 'Sustentación central izq.', [-1.9, -0.69, 0.5]],
  ['lift.MR', 'Sustentación central der.', [1.9, -0.69, 0.5]],
  ['lift.RL', 'Sustentación trasera izq.', [-1.9, -0.69, 8.5]],
  ['lift.RR', 'Sustentación trasera der.', [1.9, -0.69, 8.5]],
];
const RCS: Array<[string, string, V3]> = [
  ['rcs.FLL', 'RCS proa izq. bajo', [-(W + T + 0.12), 1.3, -11.0]],
  ['rcs.FRL', 'RCS proa der. bajo', [W + T + 0.12, 1.3, -11.0]],
  ['rcs.FLU', 'RCS proa izq. alto', [-(W + T + 0.12), 4.0, -10.2]],
  ['rcs.FRU', 'RCS proa der. alto', [W + T + 0.12, 4.0, -10.2]],
  ['rcs.MLU', 'RCS centro izq.', [-(W + T + 0.12), 4.0, 0.6]],
  ['rcs.MRU', 'RCS centro der.', [W + T + 0.12, 4.0, 0.6]],
  ['rcs.RLU', 'RCS popa izq.', [-(W + T + 0.12), 4.0, 11.2]],
  ['rcs.RRU', 'RCS popa der.', [W + T + 0.12, 4.0, 11.2]],
];

const PARTS: PartSpec[] = [
  // engineering: reactor on the centreline with its coolant pump behind it, batteries in the
  // starboard bow corner, gas bottles along the port bow, life support stack by the stairs
  part('reactor.fission.M', { id: 'reactor', c: [0, 1.05, -10.4], zone: 'eng', circuit: 'cool', tag: 'rx' }),
  part('coolpump.M', { id: 'coolpump', c: [0, 0.32, -9.25], zone: 'eng', circuit: 'cool' }),
  part('battery.L', { id: 'battery', c: [2.9, 0.7, -11.3], zone: 'eng', sw: { on: 'bat' } }),
  part('gas.n2.M', { id: 'gas.N2a', name: 'Botellas de N₂ (1)', c: [-3.15, 0.8, -11.4], zone: 'eng', sw: { valve: 'v.gasN2' } }),
  part('gas.n2.M', { id: 'gas.N2b', name: 'Botellas de N₂ (2)', c: [-3.15, 0.8, -9.9], zone: 'eng', sw: { valve: 'v.gasN2' } }),
  part('gas.o2.M', { id: 'gas.O2a', name: 'Botella de O₂ (1)', c: [-3.15, 0.8, -10.65], zone: 'eng', sw: { valve: 'v.gasO2' } }),
  part('gas.o2.M', { id: 'gas.O2b', name: 'Botella de O₂ (2)', c: [-2.7, 0.8, -11.45], zone: 'eng', sw: { valve: 'v.gasO2' } }),
  part('o2gen.M', { id: 'o2gen', c: [-2.85, 0.5, -8.7], zone: 'eng', circuit: 'life' }),
  part('scrubber.M', { id: 'scrubber', c: [-2.85, 1.48, -8.7], zone: 'eng', circuit: 'life', sw: { run: 'scrub' } }),
  // under the engineering deck: the inertial compensator
  part('grav.M', { id: 'grav', c: [0, -0.27, -6.5], zone: null, circuit: 'grav' }),
  // mess: water tank and recycler along the starboard wall, the pantry over the galley
  part('water.M', { id: 'water', c: [2.95, D2 + 0.6, -3.3], zone: 'mess' }),
  part('recycler.S', { id: 'recycler', c: [3.1, D2 + 0.4, -2.2], yaw: SIDEWAYS, zone: 'mess', circuit: 'hab' }),
  part('pantry.S', { id: 'pantry', c: [-3.1, D2 + 1.47, -2.6], half: [0.3, 0.45, 0.25], yaw: SIDEWAYS, zone: 'mess' }),
  // flank pods: a propellant pod forward and an engine pod aft on each side, joined to the hull
  part('tank.cyl.L', { id: 'tank.L', name: 'Depósito izquierdo', c: [-POD_X, 1.1, 2.4], zone: null, mount: 'pod.tank.L' }),
  part('tank.cyl.L', { id: 'tank.R', name: 'Depósito derecho', c: [POD_X, 1.1, 2.4], zone: null, mount: 'pod.tank.R' }),
  part('engine.main.L', { id: 'eng.L', name: 'Motor izquierdo', c: [-POD_X, 1.3, 10.2], zone: null, circuit: 'prop', feed: 'eng.L', mount: 'pod.eng.L', lamp: 'MOTOR IZQ' }),
  part('engine.main.L', { id: 'eng.R', name: 'Motor derecho', c: [POD_X, 1.3, 10.2], zone: null, circuit: 'prop', feed: 'eng.R', mount: 'pod.eng.R', lamp: 'MOTOR DER' }),
  // belly: six heavy lift pads round the centre of mass, the APU on the keel
  ...LIFTS.map(([id, name, c]) => part('lift.L', { id, name, c, zone: null, circuit: 'prop', feed: id })),
  part('apu.M', { id: 'apu', c: [0, -0.67, 6.0], zone: null, circuit: 'prop', feed: 'apu', lamp: 'APU' }),
  ...RCS.map(([id, name, c]) => part('rcs.M', { id, name, c, zone: null, circuit: 'prop', feed: id })),
  // roof: radiator wings over the aft hold, the solar wing over the quarters, radar and antenna
  part('radiator.wing.M', { id: 'rad.L', name: 'Radiador izquierdo', c: onRoof(M_HA, -1.3, 8.4, 0.06), zone: null, circuit: 'cool', sw: { deploy: 'rad' } }),
  part('radiator.wing.M', { id: 'rad.R', name: 'Radiador derecho', c: onRoof(M_HA, 1.3, 8.4, 0.06), zone: null, circuit: 'cool', sw: { deploy: 'rad' } }),
  part('solar.wing.M', { id: 'solar', name: 'Ala solar', c: onRoof(M_QT, 0, 2.5, 0.06), zone: null, circuit: 'hyd', sw: { deploy: 'solar' } }),
  part('radar.dome.S', { id: 'radar', c: onRoof(M_BR, 0, -10.6, 0.14), zone: null, circuit: 'sensors' }),
  part('antenna.S', { id: 'antenna', c: onRoof(M_MS, 1.3, -6.0, 0.3), zone: null, circuit: 'avionics' }),
];
const P = (id: string) => PARTS.find((p) => p.id === id)!;

const FLUID: FluidNetDef = {
  manifolds: ['mL', 'mR', 'mC', 'mRCS'],
  pipes: [
    { a: 'tank.L', b: 'mL', valve: 'v.tank.L' },
    { a: 'tank.R', b: 'mR', valve: 'v.tank.R' },
    { a: 'mL', b: 'mC', valve: 'v.xfeed.L' },
    { a: 'mR', b: 'mC', valve: 'v.xfeed.R' },
    { a: 'mL', b: 'eng.L', valve: 'v.eng.L' },
    { a: 'mR', b: 'eng.R', valve: 'v.eng.R' },
    { a: 'mC', b: 'apu', valve: 'v.apu' },
    { a: 'mC', b: 'mRCS', valve: 'v.rcs' },
    ...RCS.map(([id]) => ({ a: 'mRCS', b: id })),
    // each lift pad drinks from its own side's manifold
    ...LIFTS.map(([id, , c]) => ({ a: c[0] < 0 ? 'mL' : 'mR', b: id, valve: 'v.lift' })),
  ],
  circuit: 'prop',
  transfer: { key: 'xfer', modes: [null, ['tank.L', 'tank.R'], ['tank.R', 'tank.L']] },
  refuel: { key: 'refuel' },
  balance: [{ a: 'tank.L', b: 'tank.R', kg: 500 }],
};

// -----------------------------------------------------------------------------------------------
// Compartments, openings, doors, the stairwell hatch, the ramp
// -----------------------------------------------------------------------------------------------

const ROOMS: Array<{ id: string; label: string; volume: number; vent: number }> = [
  { id: 'bridge', label: 'PUENTE', volume: 50, vent: 0.005 },
  { id: 'mess', label: 'COMEDOR', volume: 140, vent: 0.008 },
  { id: 'quarters', label: 'CAMAROTES', volume: 75, vent: 0.006 },
  { id: 'eng', label: 'SALA DE MÁQUINAS', volume: 140, vent: 0.01 },
  { id: 'lock', label: 'ESCLUSA', volume: 14, vent: 0.01 },
  { id: 'hold', label: 'BODEGA', volume: 360, vent: 0.05 },
];
const COMPARTMENTS: CompartmentDef[] = ROOMS.map(({ id, label, volume }) => ({ id, label, volume }));
const ROOM = (id: string) => ROOMS.find((r) => r.id === id)!;

/** Ventilation ducts: the fans move air along them while their damper is open. */
const DUCTS: Array<{ key: string; a: string; b: string; area: number }> = [
  { key: 'duct.bridge', a: 'bridge', b: 'mess', area: 0.02 },
  { key: 'duct.quarters', a: 'mess', b: 'quarters', area: 0.02 },
  { key: 'duct.eng', a: 'eng', b: 'mess', area: 0.03 },
  { key: 'duct.hold', a: 'eng', b: 'hold', area: 0.04 },
  { key: 'duct.lock', a: 'eng', b: 'lock', area: 0.015 },
];

const HATCH_STAIR: HatchDef = { key: 'hatch.stair', c: [(WELL.x0 + WELL.x1) / 2, D2, (WELL.z0 + WELL.z1) / 2], w: WELL.x1 - WELL.x0, l: WELL.z1 - WELL.z0, t: FLOOR_T, slide: [1, 0, 0] };

const DOORS: DoorDef[] = [
  { key: 'door.bridge', c: [0, D2, Z_BR], n: [0, 0, 1], w: 1.2, h: 2.05, offset: 0.075 },
  { key: 'door.quarters', c: [0, D2, Z_MQ], n: [0, 0, 1], w: 1.2, h: 2.05, offset: -0.075 },
  { key: 'door.hold', c: [0, 0, Z_EH], n: [0, 0, 1], w: 1.2, h: 2.05, offset: -0.075 },
  { key: 'door.lock', c: [LOCK.x0, 0, LOCK_Z], n: [1, 0, 0], w: 1.0, h: 2.05, offset: -0.06 },
  { key: 'door.ext', c: HATCH.c, n: HATCH.n, w: HATCH.w, h: HATCH.h, offset: 0.05 },
];

const RAMP: RampDef = {
  key: 'ramp',
  hinge: [0, 0, Z_TAIL],
  w: 4.3,
  length: 4.0,
  t: 0.14,
  pistons: [-1, 1].map((sx) => ({ hull: [sx * 2.08, 2.9, Z_TAIL - 0.14] as V3, ramp: [sx * 2.0, 1.4, 0] as V3 })),
};

const OPENINGS: OpeningDef[] = [
  { id: 'door.bridge', kind: 'door', a: 'bridge', b: 'mess', key: 'door.bridge', area: 2.46 },
  { id: 'door.quarters', kind: 'door', a: 'mess', b: 'quarters', key: 'door.quarters', area: 2.46 },
  { id: 'door.hold', kind: 'door', a: 'eng', b: 'hold', key: 'door.hold', area: 2.46 },
  { id: 'door.lock', kind: 'door', a: 'eng', b: 'lock', key: 'door.lock', area: 2.05 },
  { id: 'door.ext', kind: 'door', a: 'lock', b: null, key: 'door.ext', area: 2.05 },
  // the stairwell: a pressure door lying in the deck, between the engine room and the mess
  { id: 'hatch.stair', kind: 'door', a: 'eng', b: 'mess', key: HATCH_STAIR.key, area: HATCH_STAIR.w * HATCH_STAIR.l, at: HATCH_STAIR.c, n: [0, 1, 0] },
  { id: 'ramp', kind: 'ramp', a: 'hold', b: null, key: 'ramp', area: 17 },
  ...ROOMS.map((r): OpeningDef => ({ id: `vent.${r.id}`, kind: 'vent', a: r.id, b: null, key: `vent.${r.id}`, area: r.vent })),
  ...DUCTS.map((d): OpeningDef => ({ id: d.key, kind: 'duct', a: d.a, b: d.b, key: d.key, area: d.area })),
];

const MOVERS: MoverDef[] = [
  ...DOORS.map((d): MoverDef => ({ key: d.key, rate: 1 / 0.9, circuit: 'doors', load: 1.5, sensor: doorSensor(d) })),
  { key: HATCH_STAIR.key, rate: 1 / 2.5, circuit: 'doors', load: 1.5, sensor: hatchSensor(HATCH_STAIR) },
  { key: RAMP.key, rate: 1 / 6, circuit: 'hyd', load: 5, sensor: rampSensor(RAMP) },
  { key: 'gear', rate: 1 / 7, circuit: 'hyd', load: 4 },
  { key: 'rad', rate: 1 / 5, circuit: 'cool', load: 0.8 },
  { key: 'solar', rate: 1 / 6, circuit: 'hyd', load: 0.6 },
];

/** Every room's ceiling light switch and draw. */
const ROOM_LIGHTS: Array<[string, number]> = [
  ['bridge', 0.35],
  ['mess', 0.5],
  ['quarters', 0.3],
  ['eng', 0.5],
  ['lock', 0.2],
  ['hold', 0.9],
];

const LOADS: LoadDef[] = [
  ...ROOM_LIGHTS.map(([z, kw]): LoadDef => ({ key: `light.${z}`, circuit: 'lights', kw })),
  { key: 'light.nav', circuit: 'ext', kw: 0.15 },
  { key: 'light.beacon', circuit: 'ext', kw: 0.12 },
  { key: 'light.landing', circuit: 'ext', kw: 2.5 },
  { key: 'rcs', circuit: 'prop', kw: 0.5 },
  { key: 'lift', circuit: 'prop', kw: 1.2 },
  loadOf(PARTS, 'grav'),
  loadOf(PARTS, 'coolpump'),
  loadOf(PARTS, 'o2gen'),
  loadOf(PARTS, 'scrubber', 'scrub'),
  loadOf(PARTS, 'recycler'),
  loadOf(PARTS, 'radar', 'radar.mode'),
  { key: 'fans', circuit: 'life', kw: 1.4 },
  { key: 'ls.recover', circuit: 'life', kw: 4 },
  { key: 'galley', circuit: 'hab', kw: 2.2 },
];

const LIFE: LifeSupportDef = { circuit: 'life', mode: 'ls.mode', fans: 'fans', heat: 'heat', heatKw: 0.8, repress: 'repress.', recover: { key: 'ls.recover', zone: 'lock' } };

// -----------------------------------------------------------------------------------------------
// Consoles
// -----------------------------------------------------------------------------------------------

const ON_OFF = ['APAGADO', 'ENCENDIDO'];
const OPEN = ['CERRADA', 'ABIERTA'];
const VALVE = ['CERRADA', 'ABIERTA'];
const tilt = (deg: number): V3 => [0, Math.cos((deg * Math.PI) / 180), Math.sin((deg * Math.PI) / 180)];
const THROTTLE = Array.from({ length: 11 }, (_, i) => `${i * 10} %`);

const roomHelp = (id: string) => {
  const r = ROOM(id);
  return {
    light: `Luces de techo de ${r.label.toLowerCase()}. Sin energía en ILUMINACIÓN se encienden las de emergencia, rojas.`,
    repress: `En modo MANUAL, abre las botellas de O₂/N₂ directamente a ${r.label.toLowerCase()}, aunque tenga una fuga (decisión tuya).`,
    vent: `Válvula de venteo: vacía el aire de ${r.label.toLowerCase()} al espacio${id === 'hold' ? ' (grande: la bodega se vacía en un minuto, para poder bajar la rampa)' : ', despacio'}. Bajo tapa porque tira el aire.`,
  };
};

const HELP: Record<string, string> = {
  'door.bridge': 'Puerta entre el puente y el comedor, en la cubierta superior. No abre con más de 5 kPa de diferencia entre los dos lados.',
  'door.quarters': 'Puerta entre el comedor y los camarotes. No abre con más de 5 kPa de diferencia entre los dos lados.',
  'door.hold': 'Puerta entre la sala de máquinas y la bodega, en la cubierta inferior. Con la bodega vacía (rampa abierta) no abre: cierra la rampa y deja que se presurice.',
  'door.lock': 'Puerta interior de la esclusa, desde la sala de máquinas. No abre con más de 5 kPa de diferencia: con la esclusa vacía, usa el ciclo ENTRAR.',
  'door.ext': 'Escotilla exterior de la esclusa, en el costado de estribor. Solo abre con la esclusa vacía: usa el ciclo SALIR, que la vacía primero.',
  'hatch.stair': 'Escotilla corredera de la escalera entre cubiertas: une la sala de máquinas con el comedor. Cerrada separa el aire de las dos cubiertas. No abre con más de 5 kPa de diferencia y no se cierra con alguien en la escalera.',
  ramp: 'Baja o sube la rampa de popa de la bodega (hidráulica, unos 6 s). No baja con la bodega presurizada: cierra su puerta y ventéala primero. Si alguien está en su recorrido, se para al cerrar.',
  gear: 'Sube o baja el tren de aterrizaje (seis patas). Con peso sobre las patas no se deja subir. Las luces verdes del salpicadero indican abajo y blocado.',
  caution: 'Se enciende con cualquier alarma nueva. Púlsala para reconocerla: se apaga, pero la alarma sigue en el anunciador y en la página ALARM hasta que desaparezca la causa.',
  'helm.throttle': 'Tope de los dos motores principales. En vuelo ACOPLADO y con el piloto automático es lo más que el ordenador de vuelo puede usar para la velocidad pedida; DESACOPLADO es su empuje directo. En tierra y ACOPLADO no empuja: despega primero. Los motores tienen que estar armados y en marcha.',
  'eng.L.arm': KIT.engineArm('motor izquierdo'),
  'eng.L.start': KIT.engineStart('motor izquierdo', 'PROPULSIÓN'),
  'eng.R.arm': KIT.engineArm('motor derecho'),
  'eng.R.start': KIT.engineStart('motor derecho', 'PROPULSIÓN'),
  'v.eng.L': 'Válvula de alimentación del motor izquierdo desde su colector. Cerrarla lo apaga y aísla un motor dañado.',
  'v.eng.R': 'Válvula de alimentación del motor derecho desde su colector. Cerrarla lo apaga y aísla un motor dañado.',
  'v.tank.L': 'Válvula de salida del depósito izquierdo (góndola de babor). Cerrada, ese depósito no da propelente ni se deja repostar. Ciérrala si pierde.',
  'v.tank.R': 'Válvula de salida del depósito derecho (góndola de estribor). Cerrada, ese depósito no da propelente ni se deja repostar. Ciérrala si pierde.',
  'pump.tank.L': 'Bomba de refuerzo del depósito izquierdo: sube el caudal que puede dar. Sin ella el lado izquierdo no llega a sostener tres propulsores de sustentación y un motor a la vez.',
  'pump.tank.R': 'Bomba de refuerzo del depósito derecho: sube el caudal que puede dar. Sin ella el lado derecho no llega a sostener tres propulsores de sustentación y un motor a la vez.',
  'v.xfeed.L': 'Alimentación cruzada izquierda: une el colector izquierdo con el central (APU y RCS). Con las dos abiertas, cualquier depósito alimenta a todo.',
  'v.xfeed.R': 'Alimentación cruzada derecha: une el colector derecho con el central (APU y RCS).',
  xfer: 'Bomba de transferencia: pasa propelente de un depósito al otro para equilibrar la nave. Necesita las dos válvulas de salida abiertas y energía en PROPULSIÓN.',
  'v.apu': 'Válvula de alimentación de la APU desde el colector central.',
  apu: 'Arranca o para la APU de la quilla (15 kW, quema propelente): energía de respaldo y de arranque del reactor si la batería está baja.',
  'v.rcs': 'Válvula de alimentación de los ocho bloques RCS desde el colector central.',
  rcs: 'Activa los propulsores de maniobra (ocho bloques en los costados, 0,5 kW): giros finos y desplazamientos laterales.',
  lift: 'Activa los seis propulsores de sustentación pesados bajo la panza (30 kN cada uno, 1,2 kW). Son los que mantienen el Albatros en el aire.',
  'v.lift': 'Válvula de alimentación de los seis propulsores de sustentación (los de cada lado beben de su colector). Cerrada, la nave no puede quedarse en el aire.',
  'fa.sas': 'Estabilizador: con los mandos de giro sueltos, frena los giros y mantiene la actitud y el rumbo.',
  'fa.hold': 'ACOPLADO: los mandos piden una velocidad y al soltarlos la nave se para y mantiene la altura. DESACOPLADO: piden aceleración y la nave sigue en inercia.',
  'fa.land': 'Asistente de aterrizaje: cerca del suelo limita la velocidad de bajada y, con el tren abajo, nivela la nave para posarla.',
  'light.landing': 'Focos bajo la proa que iluminan el suelo delante de la nave (2,5 kW del circuito LUCES EXTERIORES).',
  reactor: KIT.reactorLever(P('reactor')),
  'rx.set': KIT.reactorSet(P('reactor')),
  'rx.scram': KIT.scram(),
  'rx.reset': KIT.reactorReset(),
  coolpump: KIT.coolpump(P('coolpump')),
  bat: KIT.battery(P('battery')),
  rad: 'Despliega o pliega las alas radiadoras sobre la bodega (hidráulica del circuito REFRIGERACIÓN). Plegadas radian poco: el reactor a plena potencia se calienta hasta el SCRAM.',
  solar: 'Despliega el ala solar sobre los camarotes: hasta 8 kW con el sol de cara (plegada, apenas un 10 %). La energía solar se usa antes que la del reactor.',
  grav: 'Compensador inercial (6 kW, bajo la sala de máquinas): a bordo el suelo sigue siendo «abajo» aunque la nave se incline, frene o acelere. Apagado o sin energía se nota todo.',
  'radar.mode': 'Modo del radar de techo: PASIVO escucha (0,5 kW), ACTIVO barre y ve más lejos (3 kW).',
  'light.nav': 'Luces de posición (roja a babor, verde a estribor) y el estrobo blanco de la cola.',
  'light.beacon': 'Balizas rojas en el techo y en la panza: avisan de que la nave está activa.',
  fans: 'Ventiladores (1,4 kW): mueven el aire entre salas por los conductos con la compuerta abierta. El O₂ y el depurador están en la sala de máquinas: sin ventiladores no llegan a la cubierta superior.',
  heat: 'Calefacción de las salas presurizadas (0,8 kW cada una). Sin ella el aire se enfría poco a poco.',
  'ls.mode': 'AUTOMÁTICO mantiene cada sala estanca a 70 kPa con gas de las botellas (se suspende si hay fuga). MANUAL solo mete gas donde abras la válvula de represurización. APAGADO no repone nada.',
  o2gen: KIT.o2gen(P('o2gen'), 'en la sala de máquinas'),
  scrub: KIT.scrub(P('scrubber'), 'en la sala de máquinas'),
  recycler: 'Reciclador de agua (0,8 kW, circuito HABITABILIDAD), en el comedor: recupera el 90 % del agua usada.',
  galley: 'Cocina del comedor: horno y calentador de agua (2,2 kW del circuito HABITABILIDAD, prioridad BAJA). Apágala si falta energía.',
  'ls.recover': 'Compresor (4 kW): bombea el aire de la esclusa a las botellas en vez de tirarlo. El ciclo de la esclusa lo usa solo.',
  'v.gasO2': 'Válvula de las dos botellas de O₂ (160 kg). Cerrada no hay oxígeno para represurizar ni para los trajes acoplados a los asientos.',
  'v.gasN2': 'Válvula de las botellas de N₂ (400 kg), el gas de relleno del aire. Cerrada no se puede represurizar.',
  refuel: 'Conecta la manguera de la plataforma de la base: llena los dos depósitos con sus válvulas de salida abiertas. Solo funciona aterrizado en la plataforma.',
  'duct.bridge': 'Compuerta del conducto de ventilación entre el comedor y el puente. Cerrada aísla el puente, pero le corta el aire de los ventiladores.',
  'duct.quarters': 'Compuerta del conducto de ventilación entre el comedor y los camarotes.',
  'duct.eng': 'Compuerta del conducto vertical entre la sala de máquinas y el comedor: es el que sube el aire del soporte vital a la cubierta superior aunque la escotilla esté cerrada.',
  'duct.hold': 'Compuerta del conducto entre la sala de máquinas y la bodega. Ciérrala antes de ventear la bodega.',
  'duct.lock': 'Compuerta del conducto entre la sala de máquinas y la esclusa. El ciclo de la esclusa la cierra mientras está vacía.',
  ...Object.fromEntries(ROOMS.flatMap((r) => [
    [`light.${r.id}`, roomHelp(r.id).light],
    [`repress.${r.id}`, roomHelp(r.id).repress],
    [`vent.${r.id}`, roomHelp(r.id).vent],
  ])),
};

const cycle = (value: 0 | 1, at: V2, label = value ? 'SALIR' : 'ENTRAR'): ControlSpec => ({
  key: 'lock.cycle',
  kind: 'button',
  label,
  name: value ? 'Esclusa · ciclo de salida' : 'Esclusa · ciclo de entrada',
  help: value
    ? 'Ciclo de SALIDA: cierra la puerta interior, recupera el aire de la esclusa a las botellas y abre la escotilla exterior. Espera dentro de la esclusa.'
    : 'Ciclo de ENTRADA: cierra la escotilla exterior, llena la esclusa hasta la presión de la sala de máquinas y abre la puerta interior.',
  states: ['DENTRO', 'FUERA'],
  at,
  action: 'set',
  value,
  requires: 'doors',
});

const hatchButton = (at: V2): ControlSpec => ({ key: HATCH_STAIR.key, kind: 'button', label: 'ESCOTILLA', name: 'Escotilla de la escalera', states: OPEN, at, requires: 'doors' });

// autopilot board on the glareshield, under the windshield
const AP = autopilotConsole({ frame: boardFrame([0, D2 + 1.17, Z_NOSE + 0.25], norm([0, 0.35, 1])), circuit: 'avionics', w: 1.4, h: 0.22 });

const sideFrame = (x: number, y: number, z: number, sx: 1 | -1) => boardFrame([x, y, z], norm([-sx, 1.25, 0]), [0, 0, -1]);

const CONSOLES: ConsoleSpec[] = [
  ...AP.consoles,
  {
    id: 'br.main',
    title: 'CONTROL DE VUELO',
    frame: boardFrame([0, D2 + 0.9, Z_NOSE + 0.45], tilt(40), [0, 0.2, -1]),
    w: 2.8,
    h: 0.6,
    depth: 0.12,
    free: true,
    screens: [
      { id: 'mfd.pilot', pages: ['flight', 'ap', 'nav', 'mass', 'engines', 'fuel', 'alerts'], at: [-0.8, 0.04], w: 0.5, h: 0.3 },
      { id: 'mfd.copilot', pages: ['status', 'power', 'atmos', 'reactor', 'hull', 'lock', 'alerts'], at: [0.8, 0.04], w: 0.5, h: 0.3 },
    ],
    indicators: [{ kind: 'gear-greens', at: [1.25, 0.22], ref: 'gear' }],
    controls: [
      { key: 'door.bridge', kind: 'button', label: 'P. PUENTE', name: 'Puerta del puente', states: OPEN, at: [-1.25, 0.14], requires: 'doors' },
      { key: 'helm.throttle', kind: 'rotary', label: 'ACELER.', name: 'Acelerador de los motores', states: THROTTLE, at: [-1.25, -0.16], action: 'cycle', wrap: false },
      { key: 'gear', kind: 'lever', label: 'TREN', name: 'Tren de aterrizaje', states: ['ARRIBA', 'ABAJO Y BLOCADO'], at: [1.25, 0.02], requires: 'hyd' },
      { key: 'light.landing', kind: 'toggle', label: 'FOCOS', name: 'Focos de aterrizaje', states: ON_OFF, at: [1.25, -0.2] },
      { key: 'caution', kind: 'master', label: 'ALARMA', name: 'Alarma general (reconocer)', states: ['SIN AVISOS', 'AVISO ACTIVO'], at: [0, -0.2], action: 'reset' },
    ],
  },
  {
    id: 'br.l',
    title: 'PROPULSIÓN',
    frame: sideFrame(-1.75, D2 + 0.8, -12.0, -1),
    w: 0.46,
    h: 1.0,
    depth: 0.1,
    free: true,
    controls: [
      { key: 'eng.L.arm', kind: 'toggle', label: 'ARM IZQ', name: 'Motor izquierdo · armado', states: ['DESARMADO', 'ARMADO'], at: [-0.13, 0.38], guard: 'guard.eng.L' },
      { key: 'eng.L.start', kind: 'button', label: 'ARR IZQ', name: 'Motor izquierdo · arranque', states: ['', 'ARRANCANDO'], at: [-0.13, 0.22], action: 'pulse' },
      { key: 'eng.R.arm', kind: 'toggle', label: 'ARM DER', name: 'Motor derecho · armado', states: ['DESARMADO', 'ARMADO'], at: [0.13, 0.38], guard: 'guard.eng.R' },
      { key: 'eng.R.start', kind: 'button', label: 'ARR DER', name: 'Motor derecho · arranque', states: ['', 'ARRANCANDO'], at: [0.13, 0.22], action: 'pulse' },
      { key: 'v.eng.L', kind: 'toggle', label: 'AL.IZQ', name: 'Alimentación del motor izquierdo', states: VALVE, at: [-0.13, 0.06] },
      { key: 'v.eng.R', kind: 'toggle', label: 'AL.DER', name: 'Alimentación del motor derecho', states: VALVE, at: [0.13, 0.06] },
      { key: 'lift', kind: 'toggle', label: 'VTOL', name: 'Propulsores de sustentación (VTOL)', states: ON_OFF, at: [-0.13, -0.1] },
      { key: 'v.lift', kind: 'toggle', label: 'AL.VTOL', name: 'Alimentación de la sustentación VTOL', states: VALVE, at: [0.13, -0.1] },
      { key: 'rcs', kind: 'toggle', label: 'RCS', name: 'Propulsores de maniobra (RCS)', states: ON_OFF, at: [-0.13, -0.26] },
      { key: 'v.rcs', kind: 'toggle', label: 'AL.RCS', name: 'Alimentación de los RCS', states: VALVE, at: [0.13, -0.26] },
      { key: 'fa.sas', kind: 'toggle', label: 'ESTAB.', name: 'Estabilizador (amortigua giros)', states: ON_OFF, at: [-0.15, -0.42] },
      { key: 'fa.hold', kind: 'toggle', label: 'ACOPLADO', name: 'Vuelo acoplado (mantiene velocidad)', states: ['DESACOPLADO', 'ACOPLADO'], at: [0, -0.42] },
      { key: 'fa.land', kind: 'toggle', label: 'ATERRIZ.', name: 'Asistente de aterrizaje', states: ON_OFF, at: [0.15, -0.42] },
    ],
  },
  {
    id: 'br.r',
    title: 'REACTOR RX-1',
    frame: sideFrame(1.75, D2 + 0.8, -12.0, 1),
    w: 0.46,
    h: 1.0,
    depth: 0.1,
    free: true,
    indicators: [{ kind: 'reactor-core', at: [-0.1, 0.38], ref: 'reactor' }],
    controls: [
      { key: 'reactor', kind: 'lever', label: 'REACTOR', name: 'Reactor', states: ['PARADO', 'EN MARCHA'], at: [0.12, 0.32] },
      { key: 'rx.set', kind: 'rotary', label: 'SALIDA', name: 'Reactor · potencia de salida', states: ['0 %', '25 %', '50 %', '75 %', '100 %', '110 % SOBRECARGA'], at: [-0.12, 0.16], action: 'cycle', wrap: false },
      { key: 'rx.scram', kind: 'master', label: 'SCRAM', name: 'SCRAM · parada de emergencia', states: ['', 'SCRAM'], at: [0.12, 0.14], action: 'pulse', guard: 'guard.scram' },
      { key: 'coolpump', kind: 'toggle', label: 'B.REFRIG', name: 'Bomba de refrigerante', states: ON_OFF, at: [-0.12, -0.02], requires: 'cool' },
      { key: 'rx.reset', kind: 'button', label: 'REARME', name: 'Reactor · rearme tras SCRAM (núcleo < 300 °C)', states: ['', 'REARMANDO'], at: [0.12, -0.03], action: 'pulse' },
      { key: 'rad', kind: 'toggle', label: 'RADIAD.', name: 'Alas radiadoras', states: ['PLEGADAS', 'DESPLEGADAS'], at: [-0.12, -0.2], requires: 'cool' },
      { key: 'bat', kind: 'toggle', label: 'BATERÍA', name: 'Baterías', states: ['DESCONECTADA', 'CONECTADA'], at: [0.12, -0.2] },
      { key: 'solar', kind: 'toggle', label: 'SOLAR', name: 'Ala solar', states: ['PLEGADA', 'DESPLEGADA'], at: [-0.12, -0.38], requires: 'hyd' },
      { key: 'grav', kind: 'toggle', label: 'GRAVEDAD', name: 'Compensador inercial', states: ON_OFF, at: [0.12, -0.38], requires: 'grav' },
    ],
  },
  // engineer's station on the port wall, ops station on the starboard wall (each faces its seat)
  {
    id: 'br.eng',
    title: 'INGENIERÍA',
    on: wallAt(M_BR, 'L', -10.0, D2 + 1.2),
    w: 1.25,
    h: 0.7,
    depth: 0.08,
    screens: [{ id: 'mfd.eng', pages: ['power', 'reactor', 'fuel', 'engines', 'atmos', 'status'], at: [-0.3, 0.08], w: 0.5, h: 0.3 }],
    controls: [
      { key: 'v.tank.L', kind: 'toggle', label: 'V.DEP I', name: 'Válvula del depósito izquierdo', states: VALVE, at: [0.12, 0.24] },
      { key: 'v.tank.R', kind: 'toggle', label: 'V.DEP D', name: 'Válvula del depósito derecho', states: VALVE, at: [0.26, 0.24] },
      { key: 'pump.tank.L', kind: 'toggle', label: 'BOMBA I', name: 'Bomba de refuerzo izquierda', states: ON_OFF, at: [0.12, 0.08], requires: 'prop' },
      { key: 'pump.tank.R', kind: 'toggle', label: 'BOMBA D', name: 'Bomba de refuerzo derecha', states: ON_OFF, at: [0.26, 0.08], requires: 'prop' },
      { key: 'v.xfeed.L', kind: 'toggle', label: 'CRUZ I', name: 'Alimentación cruzada izquierda', states: VALVE, at: [0.4, 0.24] },
      { key: 'v.xfeed.R', kind: 'toggle', label: 'CRUZ D', name: 'Alimentación cruzada derecha', states: VALVE, at: [0.54, 0.24] },
      { key: 'xfer', kind: 'rotary', label: 'TRANSF.', name: 'Transferencia de propelente', states: ['PARADA', 'IZQ→DER', 'DER→IZQ'], at: [0.47, 0.08], action: 'cycle', requires: 'prop' },
      { key: 'v.apu', kind: 'toggle', label: 'AL.APU', name: 'Alimentación de la APU', states: VALVE, at: [0.12, -0.1] },
      { key: 'apu', kind: 'toggle', label: 'APU', name: 'APU', states: ON_OFF, at: [0.26, -0.1], requires: 'prop' },
      { key: 'coolpump', kind: 'toggle', label: 'B.REFRIG', name: 'Bomba de refrigerante', states: ON_OFF, at: [0.4, -0.1], requires: 'cool' },
      { key: 'rad', kind: 'toggle', label: 'RADIAD.', name: 'Alas radiadoras', states: ['PLEGADAS', 'DESPLEGADAS'], at: [0.54, -0.1], requires: 'cool' },
      { key: 'fans', kind: 'toggle', label: 'VENTIL.', name: 'Ventiladores', states: ON_OFF, at: [-0.46, -0.24] },
      { key: 'ls.mode', kind: 'rotary', label: 'PRESIÓN', name: 'Control de presión', states: ['AUTOMÁTICO', 'MANUAL', 'APAGADO'], at: [-0.3, -0.24], action: 'cycle' },
      { key: 'grav', kind: 'toggle', label: 'GRAVEDAD', name: 'Compensador inercial', states: ON_OFF, at: [-0.14, -0.24], requires: 'grav' },
      { key: 'caution', kind: 'master', label: 'ALARMA', name: 'Alarma general (reconocer)', states: ['SIN AVISOS', 'AVISO ACTIVO'], at: [0.33, -0.26], action: 'reset' },
    ],
  },
  {
    id: 'br.ops',
    title: 'OPERACIONES',
    on: wallAt(M_BR, 'R', -10.0, D2 + 1.2),
    w: 1.25,
    h: 0.7,
    depth: 0.08,
    screens: [{ id: 'mfd.ops', pages: ['nav', 'radar', 'hull', 'atmos', 'lock', 'alerts'], at: [0.3, 0.08], w: 0.5, h: 0.3 }],
    controls: [
      { key: 'radar.mode', kind: 'rotary', label: 'RADAR', name: 'Radar · modo', states: ['APAGADO', 'PASIVO', 'ACTIVO'], at: [-0.5, 0.24], action: 'cycle' },
      ...ROOM_LIGHTS.map(([z], i): ControlSpec => ({ key: `light.${z}`, kind: 'toggle', label: ROOM(z).label.split(' ')[0].slice(0, 8), name: `Luces · ${ROOM(z).label.toLowerCase()}`, states: ON_OFF, at: [-0.52 + (i % 3) * 0.14, 0.06 - Math.floor(i / 3) * 0.16] })),
      { key: 'light.nav', kind: 'toggle', label: 'NAVEG.', name: 'Luces de navegación', states: ON_OFF, at: [-0.1, 0.06] },
      { key: 'light.beacon', kind: 'toggle', label: 'BALIZA', name: 'Balizas anticolisión', states: ON_OFF, at: [-0.1, -0.1] },
      { key: HATCH_STAIR.key, kind: 'button', label: 'ESCOT.', name: 'Escotilla de la escalera', states: OPEN, at: [0.1, -0.24], requires: 'doors' },
      { key: 'door.quarters', kind: 'button', label: 'P.CAMAR.', name: 'Puerta de los camarotes', states: OPEN, at: [0.26, -0.24], requires: 'doors' },
      { key: 'door.hold', kind: 'button', label: 'P.BODEGA', name: 'Puerta de la bodega', states: OPEN, at: [0.42, -0.24], requires: 'doors' },
      { key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga', states: ['CERRADA', 'BAJADA'], at: [0.56, -0.24], requires: 'hyd' },
    ],
  },
  ...doorButtons(DOORS[0], { id: 'bk1', name: 'Puerta del puente', side: 0.9, t: BULK_T, requires: 'doors' }),
  ...doorButtons(DOORS[1], { id: 'bk2', name: 'Puerta de los camarotes', side: 0.9, t: BULK_T, requires: 'doors' }),
  ...doorButtons(DOORS[2], { id: 'bk4', name: 'Puerta de la bodega', side: 0.9, t: BULK_T, requires: 'doors' }),
  // the stairwell hatch: a panel at the top landing (mess side of the bridge bulkhead) and at the
  // foot of the stairs (engine room side of the hold bulkhead)
  { id: 'ms.hatch', title: 'ESCALERA', on: bulkheadAt(Z_BR, BULK_T, STAIR.x, D2 + 1.25, 1), w: 0.3, h: 0.22, depth: 0.03, controls: [hatchButton([0, -0.02])] },
  { id: 'en.hatch', title: 'ESCALERA', on: bulkheadAt(Z_EH, BULK_T, STAIR.x, 1.25, -1), w: 0.3, h: 0.22, depth: 0.03, controls: [hatchButton([0, -0.02])] },
  {
    id: 'ms.hab',
    title: 'COMEDOR',
    on: bulkheadAt(Z_MQ, BULK_T, -2.1, D2 + 1.3, -1),
    w: 0.7,
    h: 0.3,
    depth: 0.05,
    controls: [
      { key: 'light.mess', kind: 'toggle', label: 'LUCES', name: 'Luces del comedor', states: ON_OFF, at: [-0.24, -0.03] },
      { key: 'galley', kind: 'toggle', label: 'COCINA', name: 'Cocina', states: ON_OFF, at: [-0.08, -0.03], requires: 'hab' },
      { key: 'recycler', kind: 'toggle', label: 'RECICL.', name: 'Reciclador de agua', states: ON_OFF, at: [0.08, -0.03] },
      { key: 'heat', kind: 'toggle', label: 'CALEF.', name: 'Calefacción', states: ON_OFF, at: [0.24, -0.03] },
    ],
  },
  {
    id: 'qt.panel',
    title: 'CAMAROTES',
    on: bulkheadAt(Z_UA, BULK_T, 1.9, D2 + 1.3, -1),
    w: 0.4,
    h: 0.24,
    depth: 0.04,
    controls: [
      { key: 'light.quarters', kind: 'toggle', label: 'LUCES', name: 'Luces de los camarotes', states: ON_OFF, at: [-0.08, -0.03] },
      { key: 'heat', kind: 'toggle', label: 'CALEF.', name: 'Calefacción', states: ON_OFF, at: [0.08, -0.03] },
    ],
  },
  // engineering: breaker board and life support on the starboard wall, gas valves by the bottles
  {
    id: 'en.pwr',
    title: 'CUADRO ELÉCTRICO',
    on: wallAt(M_EN, 'R', BOARD_Z, 1.3),
    w: 1.25,
    h: 0.85,
    depth: 0.08,
    controls: [...breakerControls(SUBSYSTEMS, SHORT, { cols: 6, origin: [-0.5, 0.3], dx: 0.2, dy: 0.19 }), ...priorityControls(SUBSYSTEMS, SHORT, { cols: 6, origin: [-0.5, -0.1], dx: 0.2, dy: 0.17 })],
  },
  {
    id: 'en.ls',
    title: 'SOPORTE VITAL',
    on: wallAt(M_EN, 'R', -9.6, 1.35),
    w: 1.3,
    h: 1.0,
    depth: 0.08,
    screens: [{ id: 'mfd.life', pages: ['atmos', 'stores', 'lock', 'alerts'], at: [-0.35, 0.24], w: 0.48, h: 0.3 }],
    controls: [
      { key: 'ls.mode', kind: 'rotary', label: 'MODO', name: 'Control de presión', states: ['AUTOMÁTICO', 'MANUAL', 'APAGADO'], at: [0.06, 0.38], action: 'cycle' },
      { key: 'o2gen', kind: 'toggle', label: 'GEN O2', name: 'Generador de O₂', states: ON_OFF, at: [0.2, 0.38] },
      { key: 'scrub', kind: 'toggle', label: 'DEPUR.', name: 'Depurador de CO₂', states: ON_OFF, at: [0.34, 0.38] },
      { key: 'fans', kind: 'toggle', label: 'VENTIL.', name: 'Ventiladores', states: ON_OFF, at: [0.48, 0.38] },
      { key: 'heat', kind: 'toggle', label: 'CALEF.', name: 'Calefacción', states: ON_OFF, at: [0.48, 0.22] },
      { key: 'ls.recover', kind: 'toggle', label: 'RECUPERAR', name: 'Compresor: recuperar el aire de la esclusa', states: ON_OFF, at: [0.2, 0.22], requires: 'life' },
      // one row per room: manual repressurisation, its duct damper (if it has one), vent to vacuum
      ...ROOMS.flatMap((r, row): ControlSpec[] => {
        const y = 0.04 - row * 0.085;
        const short = r.label.slice(0, 3);
        const out: ControlSpec[] = [
          { key: `repress.${r.id}`, kind: 'toggle', label: `REP.${short}`, name: `Represurización manual · ${r.label.toLowerCase()}`, states: VALVE, at: [-0.55 + 0.001, y], requires: 'life' },
          { key: `vent.${r.id}`, kind: 'toggle', label: `VENT.${short}`, name: `Venteo al vacío · ${r.label.toLowerCase()}`, states: VALVE, at: [-0.25, y], guard: `guard.vent.${r.id}` },
        ];
        const duct = DUCTS.find((d) => d.key === `duct.${r.id}`);
        if (duct) out.push({ key: duct.key, kind: 'toggle', label: 'CONDUC.', name: `Compuerta de ventilación · ${r.label.toLowerCase()}`, states: OPEN, at: [-0.4, y] });
        return out;
      }),
      { key: 'v.gasO2', kind: 'valve', label: 'O2', name: 'Válvula de las botellas de O₂', states: VALVE, at: [0.2, -0.3] },
      { key: 'v.gasN2', kind: 'valve', label: 'N2', name: 'Válvula de las botellas de N₂', states: VALVE, at: [0.44, -0.3] },
    ],
  },
  {
    id: 'en.bat',
    title: 'BATERÍAS',
    on: boxFace({ c: P('battery').c, half: P('battery').half, yaw: 0 }, '-x', [0, 0.3]),
    part: 'battery',
    w: 0.26,
    h: 0.2,
    depth: 0.02,
    controls: [{ key: 'bat', kind: 'rotary', label: 'AISLADOR', name: 'Aislador de baterías', states: ['DESCONECTADA', 'CONECTADA'], at: [0, -0.02], action: 'cycle' }],
  },
  // the airlock: beside its inner door (engine room side), inside it, and outside by the hatch
  {
    id: 'en.lock',
    title: 'ESCLUSA',
    frame: boardFrame([LOCK.x0 - BULK_T / 2 - 0.035, 1.3, LOCK.z0 + 0.45], [-1, 0, 0]),
    w: 0.56,
    h: 0.26,
    depth: 0.03,
    indicators: [{ kind: 'airlock', at: [0, 0.06] }],
    controls: [cycle(1, [-0.19, -0.04]), cycle(0, [0.19, -0.04]), { key: 'door.lock', kind: 'button', label: 'PUERTA', name: 'Puerta interior de la esclusa', states: OPEN, at: [0, -0.06], requires: 'doors' }],
  },
  {
    id: 'lk.ctl',
    title: 'ESCLUSA',
    on: bulkheadAt(Z_EH, BULK_T, 2.3, 1.3, -1),
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
    id: 'ext.lock',
    title: 'ESCLUSA',
    frame: boardFrame([W + T + 0.07, 1.25, LOCK_Z + 0.95], [1, 0, 0]),
    w: 0.24,
    h: 0.36,
    depth: 0.06,
    indicators: [{ kind: 'airlock', at: [0, 0.12] }],
    controls: [cycle(1, [0, 0.02], 'ABRIR'), cycle(0, [0, -0.12], 'CERRAR')],
  },
  // the hold: ramp panel on the starboard wall by the ramp, and one outside
  {
    id: 'hd.ramp',
    title: 'BODEGA',
    on: wallAt(M_HA, 'R', 10.6, 1.35),
    w: 0.7,
    h: 0.4,
    depth: 0.06,
    controls: [
      { key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga', states: ['CERRADA', 'BAJADA'], at: [-0.24, 0.04], requires: 'hyd' },
      { key: 'light.hold', kind: 'toggle', label: 'LUCES', name: 'Luces de la bodega', states: ON_OFF, at: [-0.08, 0.04] },
      { key: 'repress.hold', kind: 'toggle', label: 'REPRES.', name: 'Represurización manual · bodega', states: VALVE, at: [0.08, 0.04], requires: 'life' },
      { key: 'vent.hold', kind: 'toggle', label: 'VENTEO', name: 'Venteo al vacío · bodega', states: VALVE, at: [0.24, 0.04], guard: 'guard.vent.hold' },
      { key: 'duct.hold', kind: 'toggle', label: 'CONDUC.', name: 'Compuerta de ventilación · bodega', states: OPEN, at: [-0.08, -0.12] },
      { key: 'door.hold', kind: 'button', label: 'P.BODEGA', name: 'Puerta de la bodega', states: OPEN, at: [0.08, -0.12], requires: 'doors' },
    ],
  },
  {
    id: 'ext.ramp',
    title: 'RAMPA',
    frame: boardFrame([2.85, 1.1, Z_TAIL + T + 0.07], [0, 0, 1]),
    w: 0.3,
    h: 0.3,
    depth: 0.06,
    controls: [{ key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga (exterior)', states: ['CERRADA', 'BAJADA'], at: [0, -0.03], requires: 'hyd' }],
  },
  {
    id: 'ext.fuel',
    title: 'REPOSTAJE',
    frame: boardFrame([-(W + T + 0.07), 0.8, -1.0], [-1, 0, 0]),
    w: 0.3,
    h: 0.3,
    depth: 0.06,
    controls: [{ key: 'refuel', kind: 'toggle', label: 'TOMA', name: 'Toma de repostaje (en la plataforma de la base)', states: ['DESCONECTADA', 'CONECTADA'], at: [0, -0.03] }],
  },
];

const ANNUNCIATOR = {
  console: 'br.main',
  at: [0, 0.2] as V2,
  cols: 6,
  cell: [0.075, 0.034] as V2,
  lamps: ['CASCO', 'DESCOMP', 'O2', 'CO2', 'FUGA AIRE', 'GAS', 'SOP VITAL', 'REACTOR', 'SCRAM', 'REFRIG', 'BATERÍA', 'DESLASTRE', 'DISYUNTOR', 'APU', 'COMBUST', 'FUGA COMB', 'DESEQUIL', 'MOTOR IZQ', 'MOTOR DER', 'VTOL', 'GRAVEDAD', 'AGUA', 'VÍVERES', 'ESCLUSA'],
};

// the chin under the lower deck's nose carries the floodlights on its raked face
const CHIN = prop('chin', { id: 'chin', c: [0, -0.2, Z_BOW - 0.3], half: [2.6, 0.2, 0.3] });
const flood = (x: number) => profilePoint(CHIN, PROFILES.chin, 2, 0.5, x);

/** Loose cargo in the hold: two rows of crates along the tie-down rails. */
const CARGO: CargoDef[] = [
  { pos: [-1.6, 0.35, 1.0], half: [0.5, 0.35, 0.45], yaw: 0, mass: 180, paint: 'orange' },
  { pos: [-1.6, 0.35, 2.2], half: [0.5, 0.35, 0.45], yaw: 0.05, mass: 160, paint: 'grey' },
  { pos: [-1.6, 0.95, 1.05], half: [0.4, 0.25, 0.35], yaw: -0.1, mass: 70, paint: 'grey' },
  { pos: [1.6, 0.45, 6.5], half: [0.6, 0.45, 0.6], yaw: 0, mass: 260, paint: 'orange' },
  { pos: [1.6, 0.3, 8.0], half: [0.45, 0.3, 0.4], yaw: 0.2, mass: 90, paint: 'grey' },
  { pos: [-1.8, 0.3, 7.5], half: [0.45, 0.3, 0.4], yaw: -0.15, mass: 90, paint: 'orange' },
];

function buildDef(): ShipDef {
  const panels = buildPanels();
  const parts = buildParts(PARTS);
  const specs = CONSOLES.map((con) => ({ ...con, controls: con.controls.map((c) => ({ ...c, help: c.help ?? HELP[c.key] })) }));
  const { consoles, controls, screens, indicators } = buildConsoles(specs, panels, parts, []);
  const dash = consoles.find((c) => c.id === 'br.main')!;
  const rail = (id: string, c: V3, len: number, yaw = 0) => prop('railing', { id, c, half: [0.03, 0.55, len / 2], yaw, zone: 'mess' });
  const props = buildProps([
    supportBlock('dash', dash, 'bridge', D2),
    // the stairs between the decks, rising toward the nose; guard rails round the well upstairs
    prop('stairs', { id: 'stairs.in', c: [STAIR.x, D2 / 2, (STAIR.zBottom + STAIR.zTop) / 2], half: [(STAIR.zBottom - STAIR.zTop) / 2, D2 / 2, STAIR.w / 2], yaw: -Math.PI / 2, zone: 'eng', look: { steps: 15 } }),
    rail('rail.well.side', [WELL.x1 + 0.04, D2 + 0.55, (WELL.z0 + WELL.z1) / 2], WELL.z1 - WELL.z0),
    rail('rail.well.aft', [(WELL.x0 + WELL.x1) / 2, D2 + 0.55, WELL.z1 + 0.04], WELL.x1 - WELL.x0, Math.PI / 2),
    // the cable riser cabinet in the mess's forward starboard corner (the circuits climb inside)
    prop('block', { id: 'riser', c: [3.15, D2 + 1.1, -8.45], half: [0.25, 1.1, 0.3], zone: 'mess' }),
    // mess: galley under the pantry, two tables
    prop('galley', { id: 'galley', c: [-3.1, D2 + 0.5, -2.6], half: [0.3, 0.5, 0.55], zone: 'mess' }),
    prop('table', { id: 'table.1', c: [0.6, D2 + 0.37, -6.0], zone: 'mess' }),
    prop('table', { id: 'table.2', c: [0.6, D2 + 0.37, -3.2], zone: 'mess' }),
    // quarters: four double bunks, the lavatory, lockers against the aft bulkhead
    prop('bunk', { id: 'bunk.1', c: [-2.97, D2 + 0.95, 1.2], zone: 'quarters' }),
    prop('bunk', { id: 'bunk.2', c: [-2.97, D2 + 0.95, 3.4], zone: 'quarters' }),
    prop('bunk', { id: 'bunk.3', c: [2.97, D2 + 0.95, 1.2], yaw: Math.PI, zone: 'quarters' }),
    prop('lavatory', { id: 'lavatory', c: [2.9, D2 + 1.05, 4.3], yaw: Math.PI, zone: 'quarters' }),
    prop('locker', { id: 'lockers.qt', c: [-0.9, D2 + 1.0, Z_UA - 0.3], yaw: Math.PI / 2, zone: 'quarters' }),
    // airlock: suit lockers against its forward partition; outside a handhold by the hatch
    prop('locker', { id: 'lockers.lk', c: [2.4, 1.0, LOCK.z0 + 0.3], yaw: -Math.PI / 2, zone: 'lock' }),
    prop('handrail', { id: 'rail.ext', c: [W + T + 0.08, 1.1, LOCK_Z - 0.75], yaw: Math.PI, half: [0.03, 0.05, 0.12] }),
    // boarding stairs under the hatch: 32° that run on below the deck-to-ground height
    prop('stairs', { id: 'stairs', c: [W + T + 1.92, -1.2, LOCK_Z], half: [1.92, 1.2, 0.55], look: { steps: 10 } }),
    // hold: tie-down rails on the deck, racks along the walls
    prop('rail', { id: 'rail.hold.L', c: [-1.6, 0.03, 4.8], half: [0.025, 0.025, 7.0] }),
    prop('rail', { id: 'rail.hold.R', c: [1.6, 0.03, 4.8], half: [0.025, 0.025, 7.0] }),
    prop('rack', { id: 'rack.1', c: [-3.1, 0.9, -1.3], zone: 'hold' }),
    prop('rack', { id: 'rack.2', c: [3.1, 0.9, -1.3], yaw: Math.PI, zone: 'hold' }),
    prop('rack', { id: 'rack.3', c: [3.1, 0.9, 3.5], yaw: Math.PI, zone: 'hold' }),
    CHIN,
  ]);
  const defaults: Record<string, number> = {
    ...CIRCUIT_DEFAULTS,
    reactor: 1,
    'rx.set': 2,
    coolpump: 1,
    rad: 1,
    bat: 1,
    solar: 1,
    apu: 0,
    'v.apu': 1,
    grav: 1,
    ...Object.fromEntries(ROOM_LIGHTS.map(([z]) => [`light.${z}`, 1])),
    'light.nav': 1,
    'light.beacon': 1,
    'light.landing': 0,
    'radar.mode': 1,
    // parked with the crew hatch open and the lock empty; everything else sealed and breathable,
    // the stairwell hatch and the inner doors open, the ramp shut
    'door.bridge': 1,
    'door.quarters': 1,
    'door.hold': 1,
    'hatch.stair': 1,
    'door.lock': 0,
    'door.ext': 1,
    'lock.cycle': 1,
    ramp: 0,
    gear: 1,
    'v.tank.L': 1,
    'v.tank.R': 1,
    'pump.tank.L': 1,
    'pump.tank.R': 1,
    'v.xfeed.L': 1,
    'v.xfeed.R': 1,
    xfer: 0,
    'v.eng.L': 1,
    'v.eng.R': 1,
    'eng.L.arm': 0,
    'eng.R.arm': 0,
    'v.rcs': 1,
    rcs: 1,
    lift: 1,
    'v.lift': 1,
    'helm.throttle': 0,
    'fa.sas': 1,
    'fa.hold': 1,
    'fa.land': 1,
    ...AP.defaults,
    refuel: 0,
    'ls.mode': 0,
    o2gen: 1,
    scrub: 1,
    fans: 1,
    heat: 1,
    'v.gasO2': 1,
    'v.gasN2': 1,
    'ls.recover': 0,
    ...Object.fromEntries(DUCTS.map((d) => [d.key, d.key === 'duct.lock' ? 0 : 1])),
    recycler: 1,
    galley: 0,
    ...Object.fromEntries(ROOMS.filter((r) => r.id !== 'lock').map((r) => [`${r.id}.p0`, 70])),
    'mfd.pilot': 0,
    'mfd.copilot': 0,
    'mfd.eng': 0,
    'mfd.ops': 0,
    'mfd.life': 0,
    'mfd.lock': 0,
  };

  return finishShip({
    id: 'albatros',
    name: 'Albatros',
    registry: 'ALB-7',
    role: 'Transporte pesado de dos cubiertas',
    floorHeight: 1.6,
    decks: [
      { id: 'lower', label: 'CUBIERTA INFERIOR', y: 0 },
      { id: 'upper', label: 'CUBIERTA SUPERIOR', y: D2 },
    ],
    hatches: [HATCH_STAIR],
    panels,
    controls,
    consoles,
    screens,
    doors: DOORS,
    ramp: RAMP,
    gear: {
      key: 'gear',
      legs: [
        [-2.9, -0.3, -9.6],
        [2.9, -0.3, -9.6],
        [-3.0, -0.3, 0.8],
        [3.0, -0.3, 0.8],
        [-3.0, -0.3, 9.4],
        [3.0, -0.3, 9.4],
      ],
    },
    // nested rooms first: the first zone that contains a point is the one it is in
    zones: [
      { id: 'lock', label: 'ESCLUSA', min: [LOCK.x0, 0, LOCK.z0], max: [W, CEIL, LOCK.z1], lights: [[2.3, 2.45, LOCK_Z]], lightKey: 'light.lock' },
      { id: 'bridge', label: 'PUENTE', min: [-W, D2, Z_NOSE], max: [W, ROOF, Z_BR], lights: [[0, ROOF - 0.15, -11.0], [0, ROOF - 0.15, -9.8]], lightKey: 'light.bridge' },
      { id: 'mess', label: 'COMEDOR', min: [-W, D2, Z_BR], max: [W, ROOF, Z_MQ], lights: [[0, ROOF - 0.15, -7.0], [0, ROOF - 0.15, -2.2]], lightKey: 'light.mess', lux: 4.6 },
      { id: 'quarters', label: 'CAMAROTES', min: [-W, D2, Z_MQ], max: [W, ROOF, Z_UA], lights: [[0, ROOF - 0.15, 2.5]], lightKey: 'light.quarters', lux: 3.6 },
      { id: 'eng', label: 'SALA DE MÁQUINAS', min: [-W, 0, Z_BOW], max: [W, CEIL, Z_EH], lights: [[0, 2.45, -10.4], [-0.5, 2.45, -6.6], [-2.8, 2.45, -3.2]], lightKey: 'light.eng' },
      { id: 'hold', label: 'BODEGA', min: [-W, 0, Z_EH], max: [W, ROOF, Z_TAIL], lights: [[0, 2.45, -0.2], [0, 2.45, 3.4], [0, ROOF - 0.15, 7.2], [0, ROOF - 0.15, 10.4]], lightKey: 'light.hold', lux: 5 },
    ],
    seats: [
      { id: 'br.pilot', name: 'Asiento del piloto', root: [-0.8, D2, -11.35], yaw: 0, exit: [-0.05, D2, -10.9] },
      { id: 'br.copilot', name: 'Asiento del copiloto', root: [0.8, D2, -11.35], yaw: 0, exit: [0.05, D2, -10.5] },
      { id: 'br.eng', name: 'Puesto de ingeniería', root: [-2.4, D2, -10.0], yaw: Math.PI / 2, exit: [-1.5, D2, -9.9] },
      { id: 'br.ops', name: 'Puesto de operaciones', root: [2.4, D2, -10.0], yaw: -Math.PI / 2, exit: [1.5, D2, -9.9] },
    ],
    cargo: CARGO,
    extLights: [
      { kind: 'nav-red', pos: [-(W + T), 4.0, -5.0], n: [-1, 0, 0] },
      { kind: 'nav-green', pos: [W + T, 4.0, -5.0], n: [1, 0, 0] },
      { kind: 'strobe', pos: [0, ROOF + T, 11.6], n: [0, 1, 0] },
      { kind: 'beacon', pos: [0, ROOF + T, -3.0], n: [0, 1, 0] },
      { kind: 'beacon', pos: [0, -0.45, -3.0], n: [0, -1, 0] },
      { kind: 'landing', ...flood(-1.3), dir: norm([-0.1, -0.55, -1]) },
      { kind: 'landing', ...flood(1.3), dir: norm([0.1, -0.55, -1]) },
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
    helm: { throttle: 'helm.throttle', seat: 'br.pilot' },
    caution: 'caution',
    readouts: { flight: ['fa.sas', 'fa.hold', 'fa.land', 'helm.throttle'], engines: ['rcs', 'lift', 'helm.throttle'], radar: ['radar.mode'] },
    autopilot: AP.autopilot,
    // a heavy transport: slow and steady
    flight: { vmax: 22, vside: 6, vz: 4, rate: [0.22, 0.28, 0.3], accel: 2.2 },
    annunciator: ANNUNCIATOR,
    defaults,
    modules: MODULES,
    bounds: { min: [-5.8, -2.6, -13.4], max: [7.5, 5.8, 12.6] },
    indicators,
    livery: {
      hull: [0.42, 0.43, 0.45],
      stripe: [0.03, 0.12, 0.3],
      accent: [0.55, 0.36, 0.03],
      tagline: 'TRANSPORTE PESADO · DOS CUBIERTAS',
      decals: [
        { c: [-(W + T + 0.01), 4.0, -2.5], n: [-1, 0, 0], w: 4.2, h: 0.9 },
        { c: [W + T + 0.01, 4.0, -2.5], n: [1, 0, 0], w: 4.2, h: 0.9 },
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
      'Todo en el Albatros es físico: mira un mando hasta que se ilumine y pulsa clic izquierdo o E. El casco te dice su nombre, su posición y, si se niega, por qué.',
      'Los selectores (ruedas) avanzan una posición con cada clic; la rueda del ratón los gira en los dos sentidos.',
      { fig: 'cover' },
      { fig: 'leds' },
      { note: 'En este manual, los nombres en azul llevan a la ficha del mando. En cada ficha, «Señalar» pone una marca en tu casco que te lleva hasta él.' },
    ],
  },
  {
    id: 'ship',
    title: 'La nave',
    lead: 'Dos cubiertas, seis salas y dónde está cada consola',
    body: [
      'El Albatros es un transporte pesado de 25 m con dos cubiertas. Arriba vive la tripulación: el puente (piloto, copiloto, ingeniería y operaciones), el comedor y los camarotes. Abajo trabaja la nave: la sala de máquinas (reactor, soporte vital, baterías y gases), la esclusa de tripulación en su esquina de estribor y la bodega, que a popa ocupa las dos alturas y se abre con una rampa.',
      'Por fuera: una góndola de propelente y una de motor a cada costado, seis propulsores de sustentación pesados bajo la panza, la APU en la quilla, las alas radiadoras sobre la bodega y el ala solar sobre los camarotes.',
      { fig: 'plan' },
      { note: 'El plano dibuja cada cubierta por separado; tu punto verde está en la que pisas. Pulsa una consola para ir a sus mandos.' },
    ],
  },
  {
    id: 'decks',
    title: 'Cubiertas y escalera',
    lead: 'Subir y bajar sin mezclar el aire de las dos cubiertas',
    body: [
      'Las dos cubiertas se unen por la escalera de la sala de máquinas, pegada a la pared de babor. Arriba desemboca en el comedor, junto a la puerta del puente, por un hueco con barandilla.',
      'El hueco se cierra con una escotilla corredera ([[ms.hatch/hatch.stair]] arriba, [[en.hatch/hatch.stair]] al pie de la escalera y desde el puesto de operaciones). Cerrada, cada cubierta guarda su aire: si la bodega o la sala de máquinas pierden la presión, la cubierta de la tripulación no.',
      { warn: 'Como cualquier puerta estanca, la escotilla no abre con más de 5 kPa de diferencia entre las dos cubiertas, y no se cierra mientras haya alguien en la escalera.' },
      'El suelo de la cubierta superior es a la vez el techo de la de abajo: una placa reventada entre las dos une sus salas, no las abre al vacío.',
    ],
  },
  {
    id: 'lock',
    title: 'La esclusa',
    lead: 'Entrar y salir sin perder el aire',
    body: [
      'La nave se aparca con la escotilla abierta y la esclusa vacía; el resto conserva su aire. Para entrar: sube la escalerilla de estribor, entra en la esclusa y pulsa ENTRAR ([[lk.ctl/lock.cycle=0]]). La escotilla se cierra, la esclusa se llena y la puerta interior se abre a la sala de máquinas.',
      { fig: 'airlock' },
      { live: 'airlock' },
      {
        steps: [
          'Salir: desde la sala de máquinas ([[en.lock/lock.cycle=1]]) o desde dentro de la esclusa ([[lk.ctl/lock.cycle=1]]).',
          'Desde fuera, junto a la escotilla, ABRIR y CERRAR ([[ext.lock/lock.cycle=1]], [[ext.lock/lock.cycle=0]]).',
        ],
      },
      { warn: 'El ciclo necesita energía en PUERTAS y SOPORTE VITAL. Si se detiene (alarma ESCLUSA), termínalo a mano con la consola de la esclusa.' },
    ],
  },
  {
    id: 'hold',
    title: 'La bodega y la rampa',
    lead: 'Cargar y descargar',
    body: [
      'La bodega es presurizable pero enorme (360 m³): la rampa solo baja con ella vacía. Para abrirla:',
      {
        steps: [
          'Cierra la puerta de la bodega ([[hd.ramp/door.hold]]) y su conducto ([[hd.ramp/duct.hold]]), con todo el mundo dentro con el traje cerrado o fuera.',
          'Ventéala ([[hd.ramp/vent.hold]], bajo tapa): tarda cerca de un minuto.',
          'Baja la rampa ([[hd.ramp/ramp]], o desde fuera [[ext.ramp/ramp]]).',
        ],
      },
      'Para volver a presurizarla, sube la rampa y cierra el venteo: en AUTOMÁTICO el soporte vital la llena con las botellas (gasta mucho gas: vigila la reserva de N₂).',
    ],
  },
  {
    id: 'cold',
    title: 'Arranque en frío',
    lead: 'De nave apagada a todo en marcha',
    body: [
      {
        steps: [
          'Conecta las baterías ([[br.r/bat]] o el aislador sobre ellas en la sala de máquinas, [[en.bat/bat]]). Comprueba los disyuntores en el cuadro eléctrico ([[en.pwr/brk.cool]]).',
          'Enciende la bomba de refrigerante ([[br.r/coolpump]]) y despliega los radiadores ([[br.r/rad]]).',
          'Sube la palanca del reactor ([[br.r/reactor]]) y elige la salida ([[br.r/rx.set]]). Unos 15 s hasta EN MARCHA.',
          'Si la batería está baja, arranca antes la APU ([[br.eng/apu]]).',
        ],
      },
      { live: 'reactor' },
    ],
  },
  {
    id: 'air',
    title: 'Aire y presión',
    lead: 'Presurizar, ventilar, respirar',
    body: [
      'El soporte vital está en la sala de máquinas: generador de O₂ y depurador junto a la escalera, botellas en la proa de babor. Los ventiladores suben el aire al comedor por su conducto vertical ([[en.ls/duct.eng]]) y desde allí al puente y a los camarotes.',
      { fig: 'air' },
      { live: 'air' },
      'En AUTOMÁTICO ([[en.ls/ls.mode]]) cada sala estanca se mantiene a 70 kPa con gas de las botellas ([[en.ls/v.gasO2]], [[en.ls/v.gasN2]]). El automático no mete gas donde hay una fuga: sella primero.',
      { controls: ['ls.mode', 'o2gen', 'scrub', 'fans', 'heat', 'duct.eng', 'duct.bridge', 'duct.quarters', 'duct.hold', 'duct.lock', 'ls.recover'] },
    ],
  },
  {
    id: 'stores',
    title: 'Víveres y agua',
    lead: 'Agua, comida y reciclado',
    body: ['El depósito de agua y el reciclador ([[ms.hab/recycler]]) están en el comedor, la despensa sobre la cocina ([[ms.hab/galley]]).', { live: 'stores' }],
  },
  {
    id: 'power',
    title: 'Energía',
    lead: 'Reactor, sol, APU, baterías, prioridades y disyuntores',
    body: [
      'Fuentes: el reactor RX-1 (60 kW), el ala solar (8 kW al sol), la APU (15 kW, quema propelente) y las baterías (30 kWh).',
      { fig: 'power' },
      { live: 'power' },
      'El cuadro eléctrico está en la pared de estribor de la sala de máquinas. Los circuitos de la cubierta superior suben por el armario de cables del rincón de proa-estribor del comedor: una brecha allí deja a oscuras media nave.',
      { fig: 'circuits' },
    ],
  },
  {
    id: 'reactor',
    title: 'Reactor',
    lead: 'Arranque, potencia, temperatura y SCRAM',
    body: ['El RX-1 está en la proa de la sala de máquinas, con su bomba de refrigerante detrás. Se maneja desde la consola derecha del puente.', { live: 'reactor' }, { controls: ['reactor', 'rx.set', 'coolpump', 'rad', 'rx.scram', 'rx.reset'] }],
  },
  {
    id: 'drive',
    title: 'Motores y propelente',
    lead: 'Armar, arrancar, alimentar',
    body: [
      { fig: 'drive' },
      {
        steps: [
          'Abre las tapas y arma los motores ([[br.l/eng.L.arm]], [[br.l/eng.R.arm]]); pulsa ARRANQUE en cada uno ([[br.l/eng.L.start]], [[br.l/eng.R.start]]).',
          'Cada góndola de propelente alimenta su lado; las alimentaciones cruzadas ([[br.eng/v.xfeed.L]], [[br.eng/v.xfeed.R]]) llevan propelente a la APU y a los RCS. La transferencia ([[br.eng/xfer]]) equilibra los dos depósitos.',
        ],
      },
      { live: 'fuel' },
    ],
  },
  {
    id: 'flight',
    title: 'Vuelo',
    lead: 'Despegar, volar, aterrizar y el piloto automático',
    body: [
      'El Albatros se sostiene con seis propulsores de sustentación pesados, corre con los dos motores y afina los giros con los RCS. Es lento de reflejos: anticípate.',
      {
        steps: [
          'Siéntate en el asiento del piloto (el izquierdo). La sustentación viene encendida ([[br.l/lift]], [[br.l/v.lift]]).',
          'R sube, F baja. W/S adelante y atrás, A/D gira el morro, Z/C desplaza de lado. Suéltalo todo y la nave frena y se queda a esa altura (vuelo ACOPLADO, [[br.l/fa.hold]]).',
          'Tren arriba en vuelo ([[br.main/gear]]) y abajo antes de posarte.',
        ],
      },
      { live: 'flight' },
      { controls: ['ap.on', 'ap.lvl', 'ap.alt', 'ap.hdg', 'ap.spd', 'ap.nav', 'ap.to', 'ap.land', 'ap.alt.sel', 'ap.hdg.sel', 'ap.spd.sel', 'ap.wp'] },
      ...AP_SPACE_MANUAL,
      { warn: 'Sin energía en AVIÓNICA se apaga el ordenador de vuelo: mando DIRECTO, sin estabilizador ni piloto automático.' },
    ],
  },
  SPACE_MANUAL,
  {
    id: 'damage',
    title: 'Daños y reparación',
    lead: 'Brechas, ventanas y máquinas',
    body: [
      'Las explosiones dañan los paneles del casco y las máquinas cercanas. Las ventanas del puente y del comedor son de vidrio. Cierra las puertas y la escotilla de la escalera para salvar el resto de la nave.',
      'La soldadora (tecla 2) repara paneles y máquinas.',
      ...decompressionManual(),
      { warn: 'Los depósitos de las góndolas y un motor dañado pueden explotar. Aíslalos antes de soldar.' },
    ],
  },
  { id: 'equipment', title: 'Equipo', lead: 'Cada máquina de a bordo, del catálogo estándar', body: [{ fig: 'equipment' }] },
  { id: 'alerts', title: 'Alarmas', lead: 'Qué significa cada lámpara y qué hacer', body: ['Cualquier alarma nueva enciende la alarma general ([[br.main/caution]]).', { fig: 'alerts' }] },
  { id: 'screens', title: 'Pantallas', lead: 'Qué muestra cada página', body: [{ fig: 'pages' }] },
  { id: 'consoles', title: 'Todos los mandos', lead: 'Consola por consola', body: [{ fig: 'consoles' }] },
];

export const ALBATROS = buildDef();
