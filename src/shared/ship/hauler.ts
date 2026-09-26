// "Selene" light cargo hauler: cockpit → systems corridor → cargo bay with rear ramp.
// Ship space: metres, +X starboard, +Y up, nose toward −Z, origin = deck level on the centreline.
// Everything here is data; the builder in def.ts turns it into panels, controls and routing.

import {
  boardFrame,
  hostPanel,
  onConsole,
  routeConduits,
  ShipBuilder,
  type ConsoleDef,
  type ControlDef,
  type ControlKind,
  type ModuleDef,
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
  B.cap('cockpit', 'BK1', Z_CK, FWD, DOOR, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.2, face: 'mid' });

  B.strip(cr, 'CR', ['L', 'LC', 'T', 'RC', 'R'], [2, 1, 1, 1, 2], () => 'hull', T);
  B.floor('corridor', 'CR', -1.6, 1.6, Z_CK, Z_CR, 2, 3, FLOOR_T);
  B.cap('corridor', 'BK2', Z_CR, FWD, DOOR, { outwardZ: -1, kind: 'bulkhead', t: BULK_T, rowH: 1.2, face: 'mid' });

  B.strip(cg, 'CG', ['L', 'LC', 'T', 'T', 'RC', 'R'], [2, 1, 1, 1, 1, 2], () => 'hull', T);
  // the two roof segments share a label; make ids unique
  let roof = 0;
  for (const p of B.panels) if (p.zone === 'cargo' && p.id.startsWith('CG-T-')) p.id = `CG-T${roof++ % 2 === 0 ? 'L' : 'R'}-${p.id.split('-')[2]}`;
  B.cap('cargo', 'CG-FW', Z_CR, CARGO, FWD, { outwardZ: -1, kind: 'hull', t: T, rowH: 1.5, face: 'inner' });
  B.cap('cargo', 'CG-AFT', Z_TAIL, CARGO, RAMP_OPENING, { outwardZ: 1, kind: 'hull', t: T, rowH: 1.5, face: 'inner' });
  B.floor('cargo', 'CG', -2.6, 2.6, Z_CR, Z_TAIL, 3, 5, FLOOR_T);
  return B.panels;
}

const SUBSYSTEMS: SubsystemDef[] = [
  {
    id: 'lights',
    label: 'ILUMINACIÓN',
    breaker: 'brk.lights',
    color: 0xffd36a,
    routes: [
      [[-1.5, 1.6, -4.1], [-1.25, 2.2, -4.1], [-0.35, 2.33, -4.1], [-0.35, 2.33, -8.6]],
      [[-0.35, 2.33, -4.1], [-0.35, 2.33, Z_CR], [-0.35, 2.93, Z_CR + 0.1], [-0.35, 2.93, 5.2]],
    ],
  },
  {
    id: 'ext',
    label: 'LUCES EXTERIORES',
    breaker: 'brk.ext',
    color: 0x7fd6ff,
    routes: [[[-1.5, 1.6, -4.3], [-1.25, 2.2, -4.3], [0.35, 2.33, -4.3], [0.35, 2.33, -8.6]]],
  },
  {
    id: 'doors',
    label: 'PUERTAS',
    breaker: 'brk.doors',
    color: 0x9cff8a,
    routes: [[[-1.5, 1.7, -4.5], [-1.3, 2.1, -4.5], [-1.3, 2.1, Z_CK + 0.05]], [[-1.3, 2.1, -4.5], [-1.3, 2.1, Z_CR - 0.05]]],
  },
  {
    id: 'hyd',
    label: 'HIDRÁULICA',
    breaker: 'brk.hyd',
    color: 0xff9a5c,
    routes: [[[-1.5, 1.0, -4.2], [-1.3, -0.16, -4.2], [0, -0.16, -4.2], [0, -0.16, Z_TAIL - 0.15]]],
  },
  {
    id: 'avionics',
    label: 'AVIÓNICA',
    breaker: 'brk.avionics',
    color: 0xb58cff,
    routes: [[[-1.5, 1.0, -4.4], [-1.3, -0.16, -4.4], [-0.55, -0.16, -4.4], [-0.55, -0.16, -9.2]]],
  },
  {
    id: 'shield',
    label: 'ESCUDO TÉRMICO',
    breaker: 'brk.shield',
    color: 0xff6b8a,
    routes: [[[-1.5, 1.0, -4.6], [-1.3, -0.16, -4.6], [0.55, -0.16, -4.6], [0.55, -0.16, -9.2]]],
  },
];

interface ControlSpec {
  key: string;
  kind: ControlKind;
  label: string;
  name: string;
  states?: [string, string];
  at: V2;
  requires?: SubsystemId;
  action?: 'toggle' | 'reset';
}

interface ConsoleSpec {
  id: string;
  title: string;
  frame: Frame;
  w: number;
  h: number;
  depth: number;
  controls: ControlSpec[];
  screens?: Array<{ id: string; page: ScreenPage; at: V2; w: number; h: number }>;
}

const ON_OFF: [string, string] = ['APAGADO', 'ENCENDIDO'];
const OPEN: [string, string] = ['CERRADA', 'ABIERTA'];
const BRK: [string, string] = ['ABIERTO', 'CERRADO'];
const tilt = (deg: number): V3 => [0, Math.cos((deg * Math.PI) / 180), Math.sin((deg * Math.PI) / 180)];

const CONSOLES: ConsoleSpec[] = [
  {
    id: 'ck.main',
    title: 'CONTROL DE VUELO',
    frame: boardFrame([0, 0.9, -9.25], tilt(40), [0, 0.2, -1]),
    w: 2.8,
    h: 0.6,
    depth: 0.12,
    screens: [
      { id: 'mfd.status', page: 'status', at: [-0.56, 0.02], w: 0.54, h: 0.34 },
      { id: 'mfd.hull', page: 'hull', at: [0.56, 0.02], w: 0.54, h: 0.34 },
    ],
    controls: [
      { key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga', states: ['CERRADA', 'BAJADA'], at: [-1.22, 0.12], requires: 'hyd' },
      { key: 'shield', kind: 'toggle', label: 'ESCUDO', name: 'Escudo térmico', states: ['RETRAÍDO', 'DESPLEGADO'], at: [-1.0, 0.12], requires: 'shield' },
      { key: 'door.cockpit', kind: 'button', label: 'P. CABINA', name: 'Puerta de cabina', states: OPEN, at: [-1.22, -0.12], requires: 'doors' },
      { key: 'door.cargo', kind: 'button', label: 'P. BODEGA', name: 'Puerta de bodega', states: OPEN, at: [-1.0, -0.12], requires: 'doors' },
      { key: 'gear', kind: 'lever', label: 'TREN', name: 'Tren de aterrizaje', states: ['ARRIBA', 'ABAJO Y BLOCADO'], at: [1.2, 0.02], requires: 'hyd' },
      { key: 'caution', kind: 'master', label: 'ALARMA', name: 'Alarma general (reconocer)', states: ['SIN AVISOS', 'AVISO ACTIVO'], at: [0.97, -0.02], action: 'reset' },
    ],
  },
  {
    id: 'ck.over',
    title: 'ILUMINACIÓN',
    frame: boardFrame([0, 2.34, -7.7], [0, -1, 0], [0, 0, 1]),
    w: 1.1,
    h: 0.5,
    depth: 0.06,
    controls: [
      { key: 'light.cockpit', kind: 'toggle', label: 'CABINA', name: 'Luces de cabina', states: ON_OFF, at: [-0.36, 0.1] },
      { key: 'light.corridor', kind: 'toggle', label: 'PASILLO', name: 'Luces del pasillo', states: ON_OFF, at: [-0.12, 0.1] },
      { key: 'light.cargo', kind: 'toggle', label: 'BODEGA', name: 'Luces de bodega', states: ON_OFF, at: [0.12, 0.1] },
      { key: 'light.nav', kind: 'toggle', label: 'NAVEG.', name: 'Luces de navegación', states: ON_OFF, at: [-0.36, -0.12] },
      { key: 'light.beacon', kind: 'toggle', label: 'BALIZA', name: 'Baliza anticolisión', states: ON_OFF, at: [-0.12, -0.12] },
      { key: 'light.landing', kind: 'toggle', label: 'FOCOS', name: 'Focos de aterrizaje', states: ON_OFF, at: [0.12, -0.12] },
    ],
  },
  {
    id: 'cr.brk',
    title: 'DISYUNTORES',
    frame: boardFrame([-1.5, 1.35, -4.4], [1, 0, 0]),
    w: 0.95,
    h: 0.7,
    depth: 0.1,
    controls: [
      { key: 'brk.lights', kind: 'breaker', label: 'ILUMIN.', name: 'Disyuntor · iluminación', states: BRK, at: [-0.3, 0.12] },
      { key: 'brk.ext', kind: 'breaker', label: 'EXTERIOR', name: 'Disyuntor · luces exteriores', states: BRK, at: [0, 0.12] },
      { key: 'brk.doors', kind: 'breaker', label: 'PUERTAS', name: 'Disyuntor · puertas', states: BRK, at: [0.3, 0.12] },
      { key: 'brk.hyd', kind: 'breaker', label: 'HIDRÁUL.', name: 'Disyuntor · hidráulica (rampa, tren)', states: BRK, at: [-0.3, -0.16] },
      { key: 'brk.avionics', kind: 'breaker', label: 'AVIÓNICA', name: 'Disyuntor · aviónica', states: BRK, at: [0, -0.16] },
      { key: 'brk.shield', kind: 'breaker', label: 'ESCUDO', name: 'Disyuntor · escudo térmico', states: BRK, at: [0.3, -0.16] },
    ],
  },
  {
    id: 'cr.rct',
    title: 'REACTOR',
    frame: boardFrame([1.5, 1.35, -4.4], [-1, 0, 0]),
    w: 1.05,
    h: 0.7,
    depth: 0.1,
    screens: [{ id: 'mfd.power', page: 'power', at: [0.18, 0.04], w: 0.56, h: 0.36 }],
    controls: [{ key: 'reactor', kind: 'lever', label: 'REACTOR', name: 'Reactor principal', states: ['PARADO', 'EN MARCHA'], at: [-0.34, 0.02] }],
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
    title: 'BODEGA',
    frame: boardFrame([2.55, 1.3, 4.7], [-1, 0, 0]),
    w: 0.46,
    h: 0.3,
    depth: 0.05,
    controls: [
      { key: 'ramp', kind: 'button', label: 'RAMPA', name: 'Rampa de carga', states: ['CERRADA', 'BAJADA'], at: [-0.1, -0.02], requires: 'hyd' },
      { key: 'light.cargo', kind: 'toggle', label: 'LUCES', name: 'Luces de bodega', states: ON_OFF, at: [0.11, -0.02] },
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
];

const HALF: Record<ControlKind, V3> = {
  button: [0.035, 0.035, 0.03],
  toggle: [0.03, 0.045, 0.05],
  lever: [0.04, 0.09, 0.08],
  breaker: [0.04, 0.06, 0.04],
  master: [0.05, 0.04, 0.03],
};

function buildDef(): ShipDef {
  const panels = buildPanels();
  routeConduits(panels, SUBSYSTEMS);
  const consoles: ConsoleDef[] = [];
  const controls: ControlDef[] = [];
  const screens: ScreenDef[] = [];
  for (const spec of CONSOLES) {
    const f = spec.frame;
    // mounted on the panel right behind the board (its back face), if any
    const back: V3 = [f.c[0] - f.n[0] * (spec.depth + 0.04), f.c[1] - f.n[1] * (spec.depth + 0.04), f.c[2] - f.n[2] * (spec.depth + 0.04)];
    const host = spec.id === 'ck.main' ? -1 : hostPanel(panels, back, 0.25);
    consoles.push({ ...f, id: spec.id, w: spec.w, h: spec.h, title: spec.title, host, depth: spec.depth });
    for (const s of spec.screens ?? []) screens.push({ ...onConsole(f, s.at[0], s.at[1], 0.004), id: s.id, w: s.w, h: s.h, page: s.page, host });
    for (const c of spec.controls) {
      controls.push({
        ...onConsole(f, c.at[0], c.at[1]),
        index: controls.length,
        id: `${spec.id}/${c.key}`,
        key: c.key,
        action: c.action ?? 'toggle',
        kind: c.kind,
        label: c.label,
        name: c.name,
        states: c.states ?? ON_OFF,
        requires: c.requires,
        console: spec.id,
        host,
        half: HALF[c.kind],
      });
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
    extLights: [
      { kind: 'nav-red', pos: [-4.2, 1.2, -1.9] },
      { kind: 'nav-green', pos: [4.2, 1.2, -1.9] },
      { kind: 'strobe', pos: [0, 3.95, 5.3] },
      { kind: 'beacon', pos: [0, 2.56, -6.9] },
      { kind: 'beacon', pos: [0, -0.5, -1.0] },
      { kind: 'landing', pos: [-0.9, -0.42, -8.9], dir: norm([-0.1, -0.55, -1]) },
      { kind: 'landing', pos: [0.9, -0.42, -8.9], dir: norm([0.1, -0.55, -1]) },
    ],
    subsystems: SUBSYSTEMS,
    defaults: {
      reactor: 1,
      'brk.lights': 1,
      'brk.ext': 1,
      'brk.doors': 1,
      'brk.hyd': 1,
      'brk.avionics': 1,
      'brk.shield': 1,
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
    },
    modules: MODULES,
    bounds: { min: [-4.4, -1.3, -10.4], max: [4.4, 4.1, 8.6] },
  };
}

export const HAULER = buildDef();
