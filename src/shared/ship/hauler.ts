// "Selene" light cargo hauler: cockpit → systems corridor → cargo bay with rear ramp.
// Ship space: metres, +X starboard, +Y up, nose toward −Z, origin = deck level on the centreline.
// Everything here is data; the builder in def.ts turns it into panels, controls and routing.

import {
  boardFrame,
  hostPanel,
  onConsole,
  routeConduits,
  ShipBuilder,
  type CompartmentDef,
  type ConsoleDef,
  type ControlAction,
  type ControlDef,
  type ControlKind,
  type FluidNetDef,
  type ModuleDef,
  type MoverDef,
  type OpeningDef,
  type PartDef,
  type ScreenDef,
  type ScreenPage,
  type ShipDef,
  type SubsystemDef,
  type SubsystemId,
} from './def.js';
import { clipPoly, norm, type Frame, type V2, type V3 } from './geom.js';

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
  return B.panels;
}

// Power circuits: breaker in the corridor cabinet, conduit runs to the loads (a blown panel over a
// run cuts the circuit), trip rating, standing load and default priority (see power.ts).
const CIRCUITS: Array<Omit<SubsystemDef, 'breaker' | 'priority'> & { pri: number }> = [
  {
    id: 'lights',
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
  { id: 'ext', label: 'LUCES EXTERIORES', color: 0x7fd6ff, rating: 4, base: 0.05, pri: 2, routes: [[[-1.5, 1.6, -4.3], [-1.25, 2.2, -4.3], [0.35, 2.33, -4.3], [0.35, 2.33, -8.6]]] },
  { id: 'doors', label: 'PUERTAS', color: 0x9cff8a, rating: 5, base: 0.1, pri: 1, routes: [[[-1.5, 1.7, -4.5], [-1.3, 2.1, -4.5], [-1.3, 2.1, Z_CK + 0.05]], [[-1.3, 2.1, -4.5], [-1.3, 2.1, Z_CR - 0.05]]] },
  { id: 'hyd', label: 'HIDRÁULICA', color: 0xff9a5c, rating: 10, base: 0.2, pri: 1, routes: [[[-1.5, 1.0, -4.2], [-1.3, -0.16, -4.2], [0, -0.16, -4.2], [0, -0.16, Z_TAIL - 0.15]]] },
  { id: 'avionics', label: 'AVIÓNICA', color: 0xb58cff, rating: 4, base: 1.5, pri: 0, routes: [[[-1.5, 1.0, -4.4], [-1.3, -0.16, -4.4], [-0.55, -0.16, -4.4], [-0.55, -0.16, -9.2]]] },
  { id: 'shield', label: 'ESCUDO TÉRMICO', color: 0xff6b8a, rating: 5, base: 0.05, pri: 2, routes: [[[-1.5, 1.0, -4.6], [-1.3, -0.16, -4.6], [0.55, -0.16, -4.6], [0.55, -0.16, -9.2]]] },
  {
    id: 'prop',
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
  { id: 'life', label: 'SOPORTE VITAL', color: 0x5cf2d2, rating: 14, base: 0.3, pri: 0, routes: [[[-1.5, 1.0, -3.8], [-1.3, -0.16, -3.8], [1.2, -0.16, -3.8], [1.2, -0.16, -1.5], [1.85, 0.2, -1.5]]] },
  { id: 'cool', label: 'REFRIGERACIÓN', color: 0x4f9dff, rating: 12, base: 0.1, pri: 0, routes: [[[-1.5, 1.0, -3.6], [-1.4, -0.16, -3.6], [-1.4, -0.16, -1.2], [-1.75, 0.2, -1.2]]] },
  { id: 'sensors', label: 'SENSORES', color: 0xe0e070, rating: 5, base: 0.2, pri: 1, routes: [[[-1.5, 1.6, -4.7], [-1.25, 2.2, -4.7], [-0.2, 2.33, -4.7], [-0.2, 2.33, -7.2], [0, 2.45, -7.2]]] },
  { id: 'weapons', label: 'ARMAMENTO', color: 0xff4f6a, rating: 6, base: 0.1, pri: 2, routes: [[[-1.5, 1.6, -4.9], [-1.25, 2.2, -4.9], [0.2, 2.33, -4.9], [0.2, 2.33, -2.6], [0.2, 2.93, -2.3], [0, 2.95, -1.95]]] },
  { id: 'grav', label: 'COMPENSADOR INERCIAL', color: 0xc07fff, rating: 9, base: 0.1, pri: 1, routes: [[[-1.5, 1.6, -5.1], [-1.25, 2.2, -5.1], [0.5, 2.33, -5.1], [0.5, 2.33, -2.6], [0.5, 2.93, -2.3], [0.5, 2.93, 0.8]]] },
];
const SUBSYSTEMS: SubsystemDef[] = CIRCUITS.map(({ pri, ...c }) => ({ ...c, breaker: `brk.${c.id}`, priority: `pri.${c.id}` }));
const SHORT: Record<SubsystemId, string> = {
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

type PartSpec = Omit<PartDef, 'index' | 'yaw' | 'soft' | 'p'> & { yaw?: number; soft?: number; p?: Record<string, number> };
const PARTS: PartSpec[] = [
  // engineering corner of the cargo bay: reactor, its coolant pump
  { id: 'reactor', type: 'reactor', name: 'Reactor RX-1', c: [-1.75, 1.05, -1.25], half: [0.62, 1.05, 0.62], zone: 'cargo', maxHp: 150, circuit: 'cool', soft: 0.7 },
  { id: 'coolpump', type: 'coolpump', name: 'Bomba de refrigerante', c: [-2.2, 0.32, -0.2], half: [0.24, 0.32, 0.3], zone: 'cargo', maxHp: 60, circuit: 'cool', p: { kw: 2.5 } },
  // life support rack (starboard) and the gas bottles behind it
  { id: 'o2gen', type: 'o2gen', name: 'Generador de O₂ (electrólisis)', c: [1.95, 0.5, -1.6], half: [0.55, 0.5, 0.5], zone: 'cargo', maxHp: 70, circuit: 'life', p: { kw: 4 } },
  { id: 'scrubber', type: 'scrubber', name: 'Depurador de CO₂', c: [1.95, 1.48, -1.6], half: [0.55, 0.46, 0.5], zone: 'cargo', maxHp: 70, circuit: 'life', p: { kw: 1.5 } },
  { id: 'gas.O2', type: 'gas', name: 'Botella de O₂', c: [2.3, 0.8, -0.35], half: [0.17, 0.8, 0.17], zone: 'cargo', maxHp: 50, soft: 1.2, p: { gas: 0, cap: 80 } },
  { id: 'gas.N2', type: 'gas', name: 'Botellas de N₂', c: [2.3, 0.8, 0.28], half: [0.17, 0.8, 0.42], zone: 'cargo', maxHp: 60, p: { gas: 1, cap: 200 } },
  { id: 'grav', type: 'grav', name: 'Compensador inercial', c: [0, 2.78, 0.8], half: [0.45, 0.16, 0.45], zone: 'cargo', maxHp: 60, circuit: 'grav', p: { kw: 6 } },
  { id: 'loader', type: 'loader', name: 'Cargador de misiles', c: [0.95, 1.1, -2.26], half: [0.3, 0.3, 0.12], zone: 'cargo', maxHp: 50, circuit: 'weapons' },
  { id: 'battery', type: 'battery', name: 'Baterías principales', c: [-1.36, 0.62, -5.6], half: [0.22, 0.62, 0.42], zone: 'corridor', maxHp: 80 },
  // outside: nacelles = propellant tank (front) + engine (rear), belly reserve tank, APU pod
  { id: 'tank.L', type: 'tank', name: 'Depósito izquierdo', c: [-3.45, 1.15, 0.05], half: [0.74, 0.74, 1.95], zone: null, maxHp: 110, mount: 'nacelle.L', p: { cap: 1100, fill: 1000 } },
  { id: 'tank.R', type: 'tank', name: 'Depósito derecho', c: [3.45, 1.15, 0.05], half: [0.74, 0.74, 1.95], zone: null, maxHp: 110, mount: 'nacelle.R', p: { cap: 1100, fill: 1000 } },
  { id: 'tank.C', type: 'tank', name: 'Depósito de reserva', c: [0, -0.64, 2.0], half: [0.22, 0.22, 1.5], zone: null, maxHp: 90, p: { cap: 300, fill: 300 } },
  { id: 'eng.L', type: 'engine', name: 'Motor izquierdo', c: [-3.45, 1.15, 4.75], half: [0.74, 0.74, 1.6], zone: null, maxHp: 120, circuit: 'prop', feed: 'eng.L', mount: 'nacelle.L' },
  { id: 'eng.R', type: 'engine', name: 'Motor derecho', c: [3.45, 1.15, 4.75], half: [0.74, 0.74, 1.6], zone: null, maxHp: 120, circuit: 'prop', feed: 'eng.R', mount: 'nacelle.R' },
  { id: 'apu', type: 'apu', name: 'APU', c: [2.78, 0.76, 3.2], half: [0.2, 0.2, 0.62], zone: null, maxHp: 70, circuit: 'prop', feed: 'apu' },
  { id: 'rcs.FL', type: 'rcs', name: 'RCS delantero izq.', c: [-1.72, 1.3, -8.7], half: [0.12, 0.12, 0.18], zone: null, maxHp: 40, circuit: 'prop', feed: 'rcs.FL' },
  { id: 'rcs.FR', type: 'rcs', name: 'RCS delantero der.', c: [1.72, 1.3, -8.7], half: [0.12, 0.12, 0.18], zone: null, maxHp: 40, circuit: 'prop', feed: 'rcs.FR' },
  { id: 'rcs.RL', type: 'rcs', name: 'RCS trasero izq.', c: [-2.78, 1.45, 4.62], half: [0.14, 0.12, 0.2], zone: null, maxHp: 40, circuit: 'prop', feed: 'rcs.RL' },
  { id: 'rcs.RR', type: 'rcs', name: 'RCS trasero der.', c: [2.78, 1.45, 4.62], half: [0.14, 0.12, 0.2], zone: null, maxHp: 40, circuit: 'prop', feed: 'rcs.RR' },
  // dorsal: radiator wings either side of the spine, turret, sensors
  { id: 'rad.L', type: 'radiator', name: 'Radiador izquierdo', c: [-0.62, 3.22, 1.25], half: [0.42, 0.06, 2.25], zone: null, maxHp: 50, soft: 1.3, circuit: 'cool', p: { stowed: 4, deployed: 14 } },
  { id: 'rad.R', type: 'radiator', name: 'Radiador derecho', c: [0.62, 3.22, 1.25], half: [0.42, 0.06, 2.25], zone: null, maxHp: 50, soft: 1.3, circuit: 'cool', p: { stowed: 4, deployed: 14 } },
  { id: 'turret', type: 'turret', name: 'Torreta de minimisiles', c: [0, 3.42, -1.95], half: [0.45, 0.32, 0.45], zone: null, maxHp: 80, circuit: 'weapons', p: { kw: 1.5 } },
  { id: 'radar', type: 'radar', name: 'Radar (cúpula)', c: [0, 2.63, -7.2], half: [0.34, 0.14, 0.34], zone: null, maxHp: 40, circuit: 'sensors', p: { kw: 3 } },
  { id: 'antenna', type: 'antenna', name: 'Antena de comunicaciones', c: [1.05, 2.72, -4.4], half: [0.28, 0.3, 0.28], zone: null, maxHp: 30, circuit: 'avionics' },
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
  ],
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

const MOVERS: MoverDef[] = [
  { key: 'door.cockpit', rate: 1 / 0.9, circuit: 'doors', load: 1.5 },
  { key: 'door.cargo', rate: 1 / 0.9, circuit: 'doors', load: 1.5 },
  { key: 'ramp', rate: 1 / 4.5, circuit: 'hyd', load: 3.5 },
  { key: 'shield', rate: 1 / 2.4, circuit: 'shield', load: 2 },
  { key: 'gear', rate: 1 / 6, circuit: 'hyd', load: 3 },
  { key: 'rad', rate: 1 / 5, circuit: 'cool', load: 0.8 },
];

// -----------------------------------------------------------------------------------------------
// Consoles and their controls
// -----------------------------------------------------------------------------------------------

interface ControlSpec {
  key: string;
  kind: ControlKind;
  label: string;
  name: string;
  states?: string[];
  at: V2;
  requires?: SubsystemId;
  action?: ControlAction;
  value?: number;
  /** Flip cover over the control (a separate clickable cover is generated). */
  guard?: string;
}

interface ConsoleSpec {
  id: string;
  title: string;
  frame: Frame;
  w: number;
  h: number;
  depth: number;
  /** Free-standing (not mounted on a hull panel). */
  free?: boolean;
  controls: ControlSpec[];
  screens?: Array<{ id: string; pages: ScreenPage[]; at: V2; w: number; h: number }>;
}

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

export const PAGE_LABELS: Record<ScreenPage, string> = {
  status: 'SIST',
  hull: 'CASCO',
  power: 'ENERG',
  fuel: 'COMB',
  atmos: 'ATMOS',
  engines: 'MOTOR',
  reactor: 'REACT',
  alerts: 'ALARM',
  flight: 'VUELO',
  nav: 'NAV',
  radar: 'RADAR',
  weapons: 'ARMAS',
};

const CONSOLES: ConsoleSpec[] = [
  {
    id: 'ck.main',
    title: 'CONTROL DE VUELO',
    frame: boardFrame([0, 0.9, -9.25], tilt(40), [0, 0.2, -1]),
    w: 2.8,
    h: 0.6,
    depth: 0.12,
    free: true,
    screens: [
      { id: 'mfd.pilot', pages: ['flight', 'nav', 'engines', 'fuel', 'alerts'], at: [-0.56, 0.04], w: 0.54, h: 0.32 },
      { id: 'mfd.copilot', pages: ['status', 'power', 'fuel', 'atmos', 'hull', 'alerts'], at: [0.56, 0.04], w: 0.54, h: 0.32 },
    ],
    controls: [
      { key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga', states: ['CERRADA', 'BAJADA'], at: [-1.22, 0.12], requires: 'hyd' },
      { key: 'shield', kind: 'toggle', label: 'ESCUDO', name: 'Escudo térmico (persianas)', states: ['RETRAÍDO', 'DESPLEGADO'], at: [-1.0, 0.12], requires: 'shield' },
      { key: 'door.cockpit', kind: 'button', label: 'P. CABINA', name: 'Puerta de cabina', states: OPEN, at: [-1.22, -0.12], requires: 'doors' },
      { key: 'door.cargo', kind: 'button', label: 'P. BODEGA', name: 'Puerta de bodega', states: OPEN, at: [-1.0, -0.12], requires: 'doors' },
      { key: 'gear', kind: 'lever', label: 'TREN', name: 'Tren de aterrizaje', states: ['ARRIBA', 'ABAJO Y BLOCADO'], at: [1.2, 0.02], requires: 'hyd' },
      { key: 'caution', kind: 'master', label: 'ALARMA', name: 'Alarma general (reconocer)', states: ['SIN AVISOS', 'AVISO ACTIVO'], at: [0, -0.2], action: 'reset' },
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
      { key: 'v.eng.L', kind: 'toggle', label: 'AL.M.IZQ', name: 'Alimentación del motor izquierdo', states: VALVE, at: [-0.08, -0.05] },
      { key: 'v.eng.R', kind: 'toggle', label: 'AL.M.DER', name: 'Alimentación del motor derecho', states: VALVE, at: [0.08, -0.05] },
      { key: 'v.apu', kind: 'toggle', label: 'AL.APU', name: 'Alimentación de la APU', states: VALVE, at: [-0.12, -0.19] },
      { key: 'v.rcs', kind: 'toggle', label: 'AL.RCS', name: 'Alimentación de los RCS', states: VALVE, at: [0, -0.19] },
      { key: 'xfer', kind: 'rotary', label: 'TRANSF.', name: 'Transferencia de propelente', states: ['PARADA', 'CEN→IZQ', 'CEN→DER', 'IZQ→DER', 'DER→IZQ'], at: [0.12, -0.19], action: 'cycle', requires: 'prop' },
      { key: 'radar.mode', kind: 'rotary', label: 'RADAR', name: 'Radar · modo', states: ['APAGADO', 'PASIVO', 'ACTIVO'], at: [-0.12, -0.36], action: 'cycle' },
      { key: 'radar.range', kind: 'rotary', label: 'ALCANCE', name: 'Radar · alcance', states: ['250 m', '1 km', '5 km', '20 km', '100 km'], at: [0, -0.36], action: 'cycle' },
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
    controls: [
      { key: 'reactor', kind: 'lever', label: 'REACTOR', name: 'Reactor principal', states: ['PARADO', 'EN MARCHA'], at: [-0.42, 0.04] },
      { key: 'rx.set', kind: 'rotary', label: 'SALIDA', name: 'Reactor · potencia de salida', states: ['0 %', '25 %', '50 %', '75 %', '100 %', '110 % SOBRECARGA'], at: [-0.25, 0.17], action: 'cycle' },
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

const HALF: Record<ControlKind, V3> = {
  button: [0.035, 0.035, 0.03],
  toggle: [0.03, 0.045, 0.05],
  lever: [0.04, 0.09, 0.08],
  breaker: [0.04, 0.06, 0.04],
  master: [0.05, 0.04, 0.03],
  rotary: [0.035, 0.035, 0.04],
  cover: [0.045, 0.06, 0.06],
  valve: [0.06, 0.06, 0.05],
  bezel: [0.03, 0.014, 0.02],
};

/** Annunciator panel on the dash: one lamp per alert group (lit when any alert of the group is on). */
const ANNUNCIATOR = {
  console: 'ck.main',
  at: [0, 0.1] as V2,
  cols: 6,
  cell: [0.07, 0.036] as V2,
  lamps: ['CASCO', 'DESCOMP', 'O2', 'CO2', 'FUGA AIRE', 'GAS', 'REACTOR', 'SCRAM', 'REFRIG', 'BATERÍA', 'DESLASTRE', 'DISYUNTOR', 'COMBUST', 'FUGA COMB', 'DESEQUIL', 'MOTOR IZQ', 'MOTOR DER', 'SOP VITAL'],
};

function buildDef(): ShipDef {
  const panels = buildPanels();
  routeConduits(panels, SUBSYSTEMS);
  const consoles: ConsoleDef[] = [];
  const controls: ControlDef[] = [];
  const screens: ScreenDef[] = [];
  const add = (spec: ControlSpec, f: Frame, consoleId: string, host: number, extra: Partial<ControlDef> = {}) => {
    controls.push({
      ...onConsole(f, spec.at[0], spec.at[1]),
      index: controls.length,
      id: `${consoleId}/${spec.key}${spec.action === 'set' ? `=${spec.value}` : ''}`,
      key: spec.key,
      action: spec.action ?? 'toggle',
      value: spec.value,
      kind: spec.kind,
      label: spec.label,
      name: spec.name,
      states: spec.states ?? ON_OFF,
      requires: spec.requires,
      guard: spec.guard,
      console: consoleId,
      host,
      half: HALF[spec.kind],
      ...extra,
    });
  };
  for (const spec of CONSOLES) {
    const f = spec.frame;
    // mounted on the panel right behind the board (its back face), if any
    const back: V3 = [f.c[0] - f.n[0] * (spec.depth + 0.04), f.c[1] - f.n[1] * (spec.depth + 0.04), f.c[2] - f.n[2] * (spec.depth + 0.04)];
    const host = spec.free ? -1 : hostPanel(panels, back, 0.25);
    consoles.push({ ...f, id: spec.id, w: spec.w, h: spec.h, title: spec.title, host, depth: spec.depth });
    for (const s of spec.screens ?? []) {
      screens.push({ ...onConsole(f, s.at[0], s.at[1], 0.004), id: s.id, w: s.w, h: s.h, pages: s.pages, host });
      // bezel buttons under the display select its page
      s.pages.forEach((pg, i) => {
        const x = s.at[0] - s.w / 2 + ((i + 0.5) * s.w) / s.pages.length;
        add({ key: s.id, kind: 'bezel', label: PAGE_LABELS[pg], name: `Pantalla · ${PAGE_LABELS[pg]}`, states: s.pages.map((p) => PAGE_LABELS[p]), at: [x, s.at[1] - s.h / 2 - 0.028], action: 'set', value: i }, f, spec.id, host);
      });
    }
    for (const c of spec.controls) {
      // flip cover in front of a guarded control
      if (c.guard) add({ key: c.guard, kind: 'cover', label: '', name: `${c.name} · tapa de seguridad`, states: ['CERRADA', 'ABIERTA'], at: c.at }, f, spec.id, host);
      add(c, f, spec.id, host);
    }
  }

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

  const parts: PartDef[] = PARTS.map((p, index) => ({ ...p, index, yaw: p.yaw ?? 0, soft: p.soft ?? 1, p: p.p ?? {} }));
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
    'v.tank.C': 0,
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
    'eng.L.arm': 0,
    'eng.R.arm': 0,
    nacelle: 1,
    'fa.sas': 1,
    'fa.hold': 1,
    'fa.land': 1,
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

  return {
    id: 'hauler',
    name: 'Selene',
    registry: 'SLN-01',
    floorHeight: 1.15,
    panels,
    controls,
    consoles,
    screens,
    doors: [
      { key: 'door.cockpit', c: [0, 0, Z_CK], n: [0, 0, 1], w: 1.2, h: 2.05, offset: 0.075 },
      { key: 'door.cargo', c: [0, 0, Z_CR], n: [0, 0, 1], w: 1.2, h: 2.05, offset: -0.075 },
    ],
    ramp: { key: 'ramp', hinge: [0, 0, Z_TAIL], w: 3.3, length: 2.8, t: 0.12 },
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
      { id: 'cargo', label: 'BODEGA', min: [-2.6, 0, Z_CR], max: [2.6, 3.0, Z_TAIL], lights: [[0, 2.8, -0.4], [0, 2.8, 3.6]], lightKey: 'light.cargo' },
    ],
    seats: [
      { id: 'ck.pilot', name: 'Asiento del piloto', root: [-0.72, 0, -7.78], yaw: 0, exit: [-0.72, 0, -6.8] },
      { id: 'ck.copilot', name: 'Asiento del copiloto', root: [0.72, 0, -7.78], yaw: 0, exit: [0.72, 0, -6.8] },
    ],
    // the forward half of the bay is engineering (reactor, life support): cargo rides aft
    cargo: [
      { pos: [-1.95, 0.38, 1.4], half: [0.45, 0.375, 0.45], yaw: 0, mass: 70, paint: 'orange' },
      { pos: [-1.95, 0.38, 2.35], half: [0.45, 0.375, 0.45], yaw: 0.04, mass: 70, paint: 'orange' },
      { pos: [-1.95, 1.07, 1.45], half: [0.4, 0.3, 0.4], yaw: -0.08, mass: 45, paint: 'grey' },
      { pos: [1.9, 0.26, 3.2], half: [0.55, 0.25, 0.35], yaw: 0, mass: 55, paint: 'grey' },
      { pos: [2.0, 0.87, 3.2], half: [0.35, 0.35, 0.35], yaw: 0.1, mass: 35, paint: 'orange' },
      { pos: [-1.6, 0.3, 3.9], half: [0.3, 0.3, 0.3], yaw: 0.3, mass: 25, paint: 'grey' },
      { pos: [1.3, 0.3, 2.4], half: [0.3, 0.3, 0.3], yaw: -0.2, mass: 25, paint: 'orange' },
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
    parts,
    fluid: FLUID,
    compartments: COMPARTMENTS,
    openings: OPENINGS,
    movers: MOVERS,
    annunciator: ANNUNCIATOR,
    defaults,
    modules: MODULES,
    bounds: { min: [-4.4, -1.3, -10.4], max: [4.4, 4.1, 8.6] },
  };
}

export const HAULER = buildDef();
