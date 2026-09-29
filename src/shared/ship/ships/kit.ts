// Helpers every ship file uses: circuits with their breaker / priority switches, the breaker and
// priority boards, loads read from the machines' own catalog numbers, door push-button panels,
// and help texts written from a machine's parameters (so a bigger reactor explains itself with
// its own numbers).

import { AIR } from '../airflow.js';
import { boardFrame, onConsole, type AutopilotDef, type ConsoleSpec, type ControlSpec, type DoorDef, type LoadDef, type ManualBlock, type ManualSection, type PartSpec, type SubsystemDef } from '../def.js';
import { CABIN } from '../modules/atmos.js';
import type { Frame, V2, V3 } from '../geom.js';
import { AP_DEFAULTS, AP_FACE, AP_FACE_KEY, AP_FACES, AP_KEY, AP_MODES, HDG_STEPS } from '../flight/autopilot.js';
import { BOOST } from '../modules/engines.js';
import { NAV_POINTS } from '../flight/nav.js';

/**
 * Manual paragraphs on explosive decompression, for a ship's damage chapter. The integrity below
 * which each kind of panel gives way under a cabin's pressure comes from the ratings themselves.
 */
export function decompressionManual(): ManualBlock[] {
  const limit = (rating: number) => `${Math.round(Math.sqrt(CABIN.p / rating) * 100)} %`;
  return [
    'Una brecha en un compartimento presurizado es una descompresión explosiva: el aire se va en uno o dos segundos, lo que queda se enfría y se llena de niebla, y la corriente arrastra hacia el agujero la carga suelta y, a un metro o dos de él, también a ti. Agáchate (C) para ofrecerle menos cuerpo, o siéntate: el asiento te sujeta. Fuera, delante de la brecha, el chorro te lanza lejos.',
    'El choque daña las máquinas del compartimento, sobre todo las que están junto a la brecha: generadores de O₂, depuradores, recicladores, bombas y electrónica sufren; botellas, depósitos, motores y el reactor apenas lo notan. El chorro también empuja la nave: en vuelo, prepárate para corregir. Un venteo o una grieta vacían despacio y no hacen nada de esto.',
    {
      warn: `Un panel dañado aguanta menos presión: con ${CABIN.p} kPa detrás cede una ventana por debajo del ${limit(AIR.rating.glass)} de su integridad, un mamparo por debajo del ${limit(AIR.rating.bulkhead)} y una plancha del casco por debajo del ${limit(AIR.rating.hull)}. Cruje, salta la alarma PANEL CEDIENDO y en unos segundos revienta. Suéldalo por encima de ese límite antes de presurizar, o saca el aire.`,
    },
  ];
}

/** A circuit as a ship writes it: its priority tier (0 ALTA, 1 NORMAL, 2 BAJA) and short label. */
export type CircuitSpec = Omit<SubsystemDef, 'breaker' | 'priority'> & { pri: number; short: string };

/** Circuits → subsystems (breaker `brk.<id>`, priority `pri.<id>`) and their default switch positions. */
export function circuits(list: CircuitSpec[]) {
  const subsystems: SubsystemDef[] = list.map(({ pri: _p, short: _s, ...c }) => ({ ...c, breaker: `brk.${c.id}`, priority: `pri.${c.id}` }));
  const defaults: Record<string, number> = {};
  for (const c of list) {
    defaults[`brk.${c.id}`] = 1;
    defaults[`pri.${c.id}`] = c.pri;
  }
  const short = Object.fromEntries(list.map((c) => [c.id, c.short]));
  return { subsystems, defaults, short };
}

/** One breaker per circuit on a grid (cols per row from `origin`, pitch dx, dy). */
export function breakerControls(subsystems: SubsystemDef[], short: Record<string, string>, g: { cols: number; origin: V2; dx: number; dy: number }): ControlSpec[] {
  return subsystems.map((c, i) => ({
    key: c.breaker,
    kind: 'breaker',
    label: short[c.id],
    name: `Disyuntor · ${c.label.toLowerCase()} (${c.rating} kW)`,
    help: `Corta o da corriente al circuito ${c.label}. ${c.desc ?? ''} Salta solo si pide más de ${c.rating} kW durante 2 s (una máquina muy dañada que hace cortocircuito, demasiadas cargas): quita la causa antes de subirlo.`,
    states: ['ABIERTO', 'CERRADO'],
    at: [g.origin[0] + (i % g.cols) * g.dx, g.origin[1] - Math.floor(i / g.cols) * g.dy],
  }));
}

/** One priority selector per circuit on a grid. */
export function priorityControls(subsystems: SubsystemDef[], short: Record<string, string>, g: { cols: number; origin: V2; dx: number; dy: number }): ControlSpec[] {
  return subsystems.map((c, i) => ({
    key: c.priority,
    kind: 'rotary',
    label: short[c.id],
    name: `Prioridad · ${c.label}`,
    help: `Decide cuándo se corta ${c.label.toLowerCase()} si falta energía: primero se quedan sin nada los circuitos en BAJA, luego los NORMAL; los ALTA son los últimos. ${c.desc ?? ''}`,
    states: ['ALTA', 'NORMAL', 'BAJA'],
    at: [g.origin[0] + (i % g.cols) * g.dx, g.origin[1] - Math.floor(i / g.cols) * g.dy],
    action: 'cycle',
  }));
}

/**
 * The switched load of a machine, from its catalog numbers: `kw` while its switch is on (radars:
 * off / passive / active positions). `key` defaults to the part id.
 */
export function loadOf(parts: PartSpec[], id: string, key = id): LoadDef {
  const p = parts.find((x) => x.id === id);
  if (!p) throw new Error(`carga: no hay máquina "${id}"`);
  if (!p.circuit) throw new Error(`carga: la máquina "${id}" no está conectada a ningún circuito`);
  const kw = p.p?.kwActive !== undefined ? [0, p.p.kwPassive ?? 0, p.p.kwActive] : p.p?.kw;
  if (kw === undefined) throw new Error(`carga: la máquina "${id}" no dice cuánto consume (p.kw)`);
  return { key, circuit: p.circuit, kw, part: id };
}

/** Small "open" push-button panels on both faces of a door's wall (`y` above the door's sill). */
export function doorButtons(d: DoorDef, o: { id: string; name: string; side: number; y?: number; t: number; requires: string; states?: string[] }): ConsoleSpec[] {
  const along: V3 = [d.n[2], 0, -d.n[0]];
  const out: ConsoleSpec[] = [];
  for (const face of [-1, 1] as const) {
    const c: V3 = [d.c[0] + along[0] * o.side * face + d.n[0] * face * (o.t / 2 + 0.01), d.c[1] + (o.y ?? 1.25), d.c[2] + along[2] * o.side * face + d.n[2] * face * (o.t / 2 + 0.01)];
    out.push({
      id: `${o.id}.${face < 0 ? 'a' : 'b'}`,
      title: 'PUERTA',
      frame: boardFrame(c, [d.n[0] * face, 0, d.n[2] * face]),
      w: 0.2,
      h: 0.26,
      depth: 0.03,
      controls: [{ key: d.key, kind: 'button', label: 'ABRIR', name: o.name, states: o.states ?? ['CERRADA', 'ABIERTA'], at: [0, -0.02], requires: o.requires }],
    });
  }
  return out;
}

const num = (v: number, d = 0) => v.toFixed(d).replace('.', ',');

/** Help lines written from a machine's own numbers. */
export const HELP = {
  battery: (p: PartSpec) => `Conecta el banco de baterías (${num(p.p!.kwh)} kWh) a la red. Cubre lo que los generadores no dan (hasta ${num(p.p!.maxOut)} kW) y se carga con lo que sobra. Hace falta para arrancar el reactor con todo parado.`,
  reactorLever: (p: PartSpec) => `Palanca del reactor. Arriba inicia el arranque (unos ${num(p.p!.startS)} s): exige la bomba de refrigerante en marcha y energía de arranque (batería). Abajo baja la potencia hasta pararlo.`,
  reactorSet: (p: PartSpec) => `Potencia pedida al reactor, de 0 a 110 % de ${num(p.p!.kw)} kW. Por encima de 100 % es sobrecarga: calienta más deprisa. Un reactor dañado no llega a lo pedido.`,
  scram: () => 'Parada de emergencia: mete las barras y el reactor se para al instante. Bajo tapa. También salta solo a 750 °C. Para volver a arrancar hace falta REARME.',
  reactorReset: () => 'Quita el SCRAM cuando el núcleo ha bajado de 300 °C; la palanca vuelve a PARADO y ya se puede arrancar de nuevo.',
  coolpump: (p: PartSpec) => `Bomba del circuito de refrigeración (${num(p.p!.kw, 1)} kW): lleva el calor del núcleo a los radiadores. Sin ella el reactor se calienta hasta el SCRAM y no se deja arrancar.`,
  o2gen: (p: PartSpec, where: string) => `Generador de oxígeno por electrólisis (${num(p.p!.kw, 1)} kW) ${where}. Repone el O₂ que respira la tripulación; los ventiladores lo reparten al resto de la nave.`,
  scrub: (p: PartSpec, where: string) => `Depurador de CO₂ (${num(p.p!.kw, 1)} kW) ${where}. Retira el CO₂ que exhala la tripulación; necesita los ventiladores para limpiar el aire de los demás compartimentos.`,
  gasValve: (p: PartSpec) => `Válvula de ${p.name.toLowerCase()} (${num(p.p!.cap)} kg). Cerrada no hay represurización con ${p.p!.gas === 1 ? 'nitrógeno (el gas de relleno del aire)' : 'oxígeno ni O₂ para los trajes acoplados a los asientos'}.`,
  engineArm: (name: string) => `Arma el ${name.toLowerCase()}: da tensión al encendido y a su control. Sin armar no arranca. Desarmarlo apaga el motor y, con la alimentación cerrada, lo deja seguro si está dañado.`,
  engineStart: (name: string, circuit: string) => `Arranca el ${name.toLowerCase()} (unos 3 s hasta régimen). Necesita el motor armado, el circuito ${circuit} con energía y propelente llegando por su válvula. Un motor muy dañado puede explotar al encenderse.`,
};

/**
 * Autopilot panel (glareshield board): a drum with three faces turned by GIRAR (`ap.face`), each
 * with the master switch and its modes' lit buttons:
 *   SUPERFICIE — the surface modes and the four selectors: height, heading (with SYNC), speed, waypoint;
 *   ÓRBITA     — the automatic burns (SUBIR, CIRCUL., BAJAR), the orbit selector and the engines'
 *                overdrive (SOBREPOT., under a cover);
 *   ESPACIO    — where the nose points: progrado, retrógrado, radial, nadir, normal, antinormal.
 * A ship places the board and says what its flight computer runs on; the help of every button comes
 * from the mode itself (shared/ship/flight/autopilot.ts), so a new mode shows up on its face
 * without touching the ships. Returns the faces (`consoles`; `console` is the surface one), the
 * ship's `autopilot` definition and the switches' starting positions.
 */
/** Rows of the autopilot drum's faces (console-local y, m): the title sits above TOP_ROW. */
const TOP_ROW = 0.025;
const LOW_ROW = -0.07;

export function autopilotConsole(o: {
  frame: Frame;
  circuit: string;
  id?: string;
  modes?: string[];
  alts?: number[];
  speeds?: number[];
  orbits?: number[];
  w?: number;
  h?: number;
  depth?: number;
}): { console: ConsoleSpec; consoles: ConsoleSpec[]; autopilot: AutopilotDef; defaults: Record<string, number> } {
  const modes = o.modes ?? AP_MODES.map((m) => m.id);
  const alts = o.alts ?? AP_DEFAULTS.alts;
  const speeds = o.speeds ?? AP_DEFAULTS.speeds;
  const orbits = o.orbits ?? AP_DEFAULTS.orbits;
  const cruise = AP_DEFAULTS.cruise;
  // the cruise speed knob: on the ÓRBITA and ESPACIO faces (one switch, two knobs)
  const crzSel = (x: number): ControlSpec => ({
    key: 'ap.crz.sel',
    kind: 'rotary',
    label: 'CRUCERO',
    name: 'Piloto automático · velocidad de crucero',
    help: `Velocidad para CRUCERO (cara ÓRBITA: horizontal, a altura fija) y VELOC. (cara ESPACIO: respecto al cuerpo, en inercia): ${cruise.join(', ')} m/s. La orbital baja está en ~1.650 m/s.`,
    states: cruise.map((v) => `${v} m/s`),
    at: [x, LOW_ROW],
    action: 'cycle',
    wrap: false,
  });
  const w = o.w ?? 1.0;
  // tall enough for the title strip over the top row and the labels under the bottom one
  const h = Math.max(o.h ?? 0.27, 0.27);
  const id = o.id ?? 'ck.ap';
  const ON_OFF = ['APAGADO', 'CONECTADO'];
  const onFace = (face: number) =>
    modes.filter((mid) => {
      const m = AP_MODES.find((x) => x.id === mid);
      if (!m) throw new Error(`piloto automático: modo "${mid}" no existe`);
      return m.face === face;
    });
  /** Top row of a face: master, its modes, GIRAR at the right end. */
  const topRow = (face: number): ControlSpec[] => {
    const ids = onFace(face);
    const pitch = Math.min(0.115, (w - 0.08) / (ids.length + 2));
    const x0 = -((ids.length + 1) * pitch) / 2;
    return [
      {
        key: 'ap.on',
        kind: 'button',
        label: 'P.AUT',
        name: 'Piloto automático · general',
        help: 'Conecta o desconecta el piloto automático. Desconectarlo apaga todos los modos; pulsar cualquier modo lo conecta solo. Necesita energía en el ordenador de vuelo. Tecla P desde el asiento del piloto.',
        states: ON_OFF,
        at: [x0, TOP_ROW],
        requires: o.circuit,
      },
      ...ids.map((mid, i): ControlSpec => {
        const m = AP_MODES.find((x) => x.id === mid)!;
        return { key: AP_KEY(mid), kind: 'button', label: m.label, name: `Piloto automático · ${m.name.toLowerCase()}`, help: m.help, states: ['', 'ACTIVO'], at: [x0 + (i + 1) * pitch, TOP_ROW], requires: o.circuit };
      }),
      {
        key: AP_FACE_KEY,
        kind: 'button',
        label: 'GIRAR',
        name: 'Piloto automático · girar el tambor',
        help: `Gira el tambor del panel a la cara siguiente: ${AP_FACES.join(' → ')}. Solo se pueden pulsar los mandos de la cara que queda fuera; los modos conectados siguen funcionando aunque su cara quede dentro. En régimen orbital los modos de SUPERFICIE se bloquean (luz ámbar).`,
        states: AP_FACES,
        at: [x0 + (ids.length + 1) * pitch, TOP_ROW],
        action: 'cycle',
      },
    ];
  };
  const drum = (face: number) => ({ key: AP_FACE_KEY, face, faces: AP_FACES.length });
  const controls: ControlSpec[] = [
    ...topRow(AP_FACE.surface),
    {
      key: 'ap.alt.sel',
      kind: 'rotary',
      label: 'ALT',
      name: 'Piloto automático · altura seleccionada',
      help: `Altura sobre el terreno para ALTURA, NAV y el final del DESPEGUE: ${alts.join(', ')} m. Clic avanza, la rueda gira en los dos sentidos.`,
      states: alts.map((a) => `${a} m`),
      at: [-w * 0.34, LOW_ROW],
      action: 'cycle',
      wrap: false,
    },
    {
      key: 'ap.hdg.sel',
      kind: 'rotary',
      label: 'RUMBO',
      name: 'Piloto automático · rumbo seleccionado',
      help: 'Rumbo para el modo RUMBO, de 15 en 15 grados (000 = norte, 090 = este). SYNC lo pone en el rumbo que llevas ahora.',
      states: Array.from({ length: HDG_STEPS }, (_, i) => `${String((i * 360) / HDG_STEPS).padStart(3, '0')}°`),
      at: [-w * 0.14, LOW_ROW],
      action: 'cycle',
    },
    {
      key: 'ap.hdg.sync',
      kind: 'button',
      label: 'SYNC',
      name: 'Piloto automático · rumbo actual al selector',
      help: 'Copia en el selector de RUMBO el rumbo que lleva la nave ahora mismo, para mantenerlo con el modo RUMBO.',
      states: ['', 'COPIANDO'],
      at: [-w * 0.035, LOW_ROW],
      action: 'pulse',
      requires: o.circuit,
    },
    {
      key: 'ap.spd.sel',
      kind: 'rotary',
      label: 'VEL',
      name: 'Piloto automático · velocidad seleccionada',
      help: `Velocidad para el control de crucero (VELOC.) y máxima de NAV: ${speeds.join(', ')} m/s. En NAV, 0 = 25 m/s.`,
      states: speeds.map((v) => `${v} m/s`),
      at: [w * 0.13, LOW_ROW],
      action: 'cycle',
      wrap: false,
    },
    {
      key: 'ap.wp',
      kind: 'rotary',
      label: 'PUNTO',
      name: 'Piloto automático · punto de navegación',
      help: `Destino del modo NAV: ${NAV_POINTS.map((p) => p.name).join(', ')}. La página NAV lo marca en el mapa.`,
      states: NAV_POINTS.map((p) => p.name),
      at: [w * 0.34, LOW_ROW],
      action: 'cycle',
    },
  ];
  const orbitFace: ControlSpec[] = [
    ...topRow(AP_FACE.orbit),
    {
      key: 'ap.orb.sel',
      kind: 'rotary',
      label: 'ÓRBITA',
      name: 'Piloto automático · órbita seleccionada',
      help: `Altura de la órbita circular a la que SUBIR lleva la nave: ${orbits.join(', ')} km sobre el suelo medio. Más alta cuesta algo más de propelente y da una vuelta más larga.`,
      states: orbits.map((k) => `${k} km`),
      at: [-w * 0.3, LOW_ROW],
      action: 'cycle',
      wrap: false,
    },
    crzSel(-w * 0.02),
    {
      key: BOOST.key,
      kind: 'toggle',
      label: 'SOBREPOT.',
      name: 'Motores principales · sobrepotencia',
      help: `Inyección de masa: los motores mandan el doble de propelente por el chorro con la misma potencia. Empujan un ${Math.round((Math.sqrt(BOOST.flow) - 1) * 100)} % más a costa de un ${Math.round((1 - 1 / Math.sqrt(BOOST.flow)) * 100)} % menos de impulso específico: se sube antes y se gasta mucho más. Calienta la cámara: a ${BOOST.cutC} °C se corta sola y no vuelve a conectarse hasta bajar de ${BOOST.rearmC} °C.`,
      states: ['NORMAL', 'SOBREPOTENCIA'],
      at: [w * 0.26, LOW_ROW],
      guard: `${BOOST.key}.cov`,
    },
  ];
  const spaceFace = [...topRow(AP_FACE.space), crzSel(0)];
  const defaults: Record<string, number> = {
    'ap.on': 0,
    'ap.alt.sel': Math.min(2, alts.length - 1),
    'ap.hdg.sel': 0,
    'ap.hdg.sync': 0,
    'ap.spd.sel': Math.min(3, speeds.length - 1),
    'ap.wp': 0,
    [AP_FACE_KEY]: 0,
    'ap.orb.sel': Math.min(1, orbits.length - 1),
    'ap.crz.sel': Math.min(4, cruise.length - 1),
    [BOOST.key]: 0,
    [`${BOOST.key}.cov`]: 0,
  };
  for (const mid of modes) defaults[AP_KEY(mid)] = 0;
  const face = (fid: string, title: string, n: number, list: ControlSpec[]): ConsoleSpec => ({ id: fid, title, frame: o.frame, w, h, depth: o.depth ?? 0.06, free: true, drum: drum(n), controls: list });
  const surface = face(id, 'PILOTO AUTOMÁTICO · SUPERFICIE', AP_FACE.surface, controls);
  // cruise at hand whatever face the drum shows: a fixed strip under it
  const cruiseStrip: ConsoleSpec = {
    id: `${id}.crz`,
    title: 'CRUCERO',
    frame: onConsole(o.frame, 0, -(h / 2 + 0.075)),
    w: 0.34,
    h: 0.11,
    depth: 0.04,
    free: true,
    controls: [
      { ...crzSel(-0.07), at: [-0.07, -0.005] },
      {
        key: AP_KEY('ocrz'),
        kind: 'button',
        label: 'CRUCERO',
        name: 'Piloto automático · crucero',
        help: AP_MODES.find((m) => m.id === 'ocrz')!.help,
        states: ['', 'ACTIVO'],
        at: [0.08, 0.005],
        requires: o.circuit,
      },
    ],
  };
  return {
    console: surface,
    consoles: [surface, face(`${id}.orb`, 'PILOTO AUTOMÁTICO · ÓRBITA', AP_FACE.orbit, orbitFace), face(`${id}.esp`, 'PILOTO AUTOMÁTICO · ESPACIO', AP_FACE.space, spaceFace), ...(modes.includes('ocrz') ? [cruiseStrip] : [])],
    autopilot: { circuit: o.circuit, modes, alts, speeds, orbits, cruise },
    defaults,
  };
}

/** Manual blocks for the autopilot drum's ÓRBITA and ESPACIO faces (after the surface ones). */
export const AP_SPACE_MANUAL: ManualBlock[] = [
  {
    note: 'El panel del piloto automático es un tambor de tres caras: [[ck.ap/ap.face]] lo gira a SUPERFICIE, ÓRBITA o ESPACIO. Solo se pulsa la cara que queda fuera; en régimen orbital (más de 150 m/s o 15 km de altura) los modos de superficie se bloquean con luz ámbar, y mientras una maniobra orbital lleva el motor el acelerador también.',
  },
  { controls: ['ap.face', 'ap.sub', 'ap.circ', 'ap.baj', 'ap.bajq', 'ap.ocrz', 'ap.crz.sel', 'ap.orb.sel', 'eng.boost'] },
  {
    note: 'A órbita: motores principales en marcha, elige la órbita ([[ck.ap.orb/ap.orb.sel]]) y pulsa SUBIR ([[ck.ap.orb/ap.sub]]); al llegar pasa solo a CIRCUL. Para volver, BAJAR ([[ck.ap.orb/ap.baj]]) espera su ventana, frena hacia la base, baja y te deja encima en vuelo estacionario con la cara SUPERFICIE: ATERRIZ. te posa.',
  },
  { controls: ['ap.pro', 'ap.retro', 'ap.rad', 'ap.nad', 'ap.nor', 'ap.anor', 'ap.crz'] },
  { note: 'Todo lo de la órbita y el espacio, explicado junto: sección «Órbita y espacio».' },
];

/**
 * Manual section every ship with an autopilot carries (after "Vuelo"): how the regimes, the drum,
 * the cruise modes, coming down anywhere and the engines fit together.
 */
export const SPACE_MANUAL: ManualSection = {
  id: 'space',
  title: 'Órbita y espacio',
  lead: 'Cómo encaja todo: regímenes, crucero en cada cara, bajar donde quieras, motores',
  body: [
    'La nave vuela en uno de dos regímenes y todo lo demás cuelga de eso. **Superficie**: despacio y cerca del suelo, el ordenador de vuelo sostiene el peso con la sustentación, retiene posición y altura al soltar los mandos, y el acelerador pide velocidad. **Orbital** (más de 150 m/s en horizontal o más de 15 km de altura): la nave cae alrededor del cuerpo en vez de apoyarse en él; el ordenador ya no retiene nada (frenaría la órbita), el acelerador manda en el motor principal y la actitud se mantiene fija en el espacio, no respecto al suelo.',
    'El tambor del piloto automático tiene una cara para cada cosa: **SUPERFICIE** (volar sobre el suelo: altura, rumbo, velocidad, NAV, despegar, aterrizar; se bloquea en régimen orbital), **ÓRBITA** (maniobras con el motor principal: subir, redondear la órbita, crucero rápido, bajar) y **ESPACIO** (hacia dónde apunta el morro y la velocidad respecto al cuerpo). Los modos conectados siguen funcionando aunque gires el tambor; solo pulsas la cara que queda fuera.',
    { warn: 'Los modos de ÓRBITA y ESPACIO que queman (SUBIR, CIRCUL., BAJAR, BAJ.AQUÍ, CRUCERO, VELOC.) llevan el motor principal: el acelerador queda en ámbar mientras están conectados, y solo empujan con el morro ya alineado. Arranca los motores antes.' },
    'Velocidad de crucero, una por cara:',
    {
      steps: [
        'SUPERFICIE · VELOC. ([[ck.ap/ap.spd]], selector [[ck.ap/ap.spd.sel]]): velocidad sobre el suelo, unas decenas de m/s, con la sustentación. Para moverse por la zona.',
        'ÓRBITA · CRUCERO ([[ck.ap.orb/ap.ocrz]], selector [[ck.ap.orb/ap.crz.sel]]): velocidad horizontal a la altura a la que vas, con el motor principal. Para cruzar cientos de km sin subir a órbita; por debajo de la velocidad orbital el motor también sostiene parte del peso y gasta. Está también en la tira CRUCERO bajo el tambor ([[ck.ap.crz/ap.ocrz]]), a mano sea cual sea la cara que muestre.',
        'ESPACIO · VELOC. ([[ck.ap.esp/ap.crz]], mismo selector [[ck.ap.esp/ap.crz.sel]]): velocidad respecto al cuerpo, en inercia: acelera o frena y apaga; en el vacío no se pierde nada. No sostiene la altura: en órbita cambia la órbita.',
      ],
    },
    'Volver al suelo desde arriba, tres maneras:',
    {
      steps: [
        'A la base: BAJAR ([[ck.ap.orb/ap.baj]]). Espera a que la base quede delante, frena hacia ella y te deja encima en estacionario con NAV. Luego ATERRIZ.',
        'Donde estés: BAJ.AQUÍ ([[ck.ap.orb/ap.bajq]]). Frena toda la velocidad manteniendo la altura y baja a 150 m sobre el suelo. Frenar desde órbita lleva minutos y la nave avanza entretanto unos v²/2a (a 1.600 m/s, cientos de km): no para justo debajo de donde lo pulsas. Luego ATERRIZ.',
        'A mano: RETRÓG. ([[ck.ap.esp/ap.retro]]) para girar la nave contra la marcha, acelerador hasta quedarte casi sin velocidad horizontal, y baja. Al bajar de 150 m/s y 15 km el régimen vuelve a superficie y todo lo de siempre funciona (NIVEL, ALTURA, ATERRIZ.).',
      ],
    },
    'Se puede posar en cualquier punto del cuerpo: la superficie entera tiene relieve y colisión. Fuera de la base no hay rocas sueltas.',
    { warn: 'Motores desiguales: cada motor principal empuja desde su propia tobera. Si uno está en marcha y otro parado, sin propelente, dañado o en otro régimen (uno en sobrepotencia y el otro no), el empuje ya no pasa por el centro de masas y hace girar la nave hacia el lado del motor parado. El ordenador de vuelo lo contrarresta con los RCS y la sustentación; si no le llega, recorta el empuje o la nave se va de lado. En DIRECTO (sin ordenador) gira sin remedio. Arranca todos los motores y comprueba su estado antes de una quema larga.' },
    'Fuera de la influencia de cualquier cuerpo no hay órbita: ahí solo tienen sentido el apuntado y la velocidad respecto a una referencia (pendiente: hoy la referencia es siempre el cuerpo dominante).',
  ],
};
