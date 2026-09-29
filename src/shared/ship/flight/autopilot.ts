// Autopilot modes. Each mode is a small rule that turns the ship's situation into setpoints for
// the flight computer (level the wings, hold a heading, a speed, a height above the ground, fly to
// a waypoint, take off, land). Modes stack in the order of AP_MODES: later ones override what
// earlier ones asked for. The pilot's stick always wins on the axis it moves.
//
// A mode is engaged by its switch `ap.<id>` (the autopilot panel), only while the master `ap.on`
// is on and the flight computer has power. What the knobs select (height, heading, speed,
// waypoint) are switches too, so everything replicates and the MFD shows it. Discrete automation
// (gear, finishing a landing, handing over to ALT after take-off) runs in the ship systems
// (modules/autopilot.ts), on the systems authority. A new mode = an entry here (+ its button in
// the ship's panel, see ships/kit.ts `autopilotConsole`).
//
// The panel is a drum with three faces (`ap.face`): SUPERFICIE (the modes above the ground),
// ÓRBITA (automatic burns: climb to orbit, circularise, come down to the base) and ESPACIO
// (pointing the nose along the orbit's directions). Each mode says which face its button is on.
// Surface modes make no sense in the orbital regime (they are cancelled there); the orbital and
// space modes work in it. Everything is in the local frame at the ship (x east, y up, z south).

import type { AutopilotDef } from '../def.js';
import type { V3 } from '../geom.js';
import { circularAt, type BaseFix, type CelestialBody, type OrbitInfo } from '../../space/body.js';
import { bearingTo, NAV_POINTS, type NavPoint } from './nav.js';
import type { ShipPose } from './pose.js';

/** What the knobs select. */
export interface ApSelection {
  /** Height above the ground (m). */
  alt: number;
  /** Compass heading (rad). */
  hdg: number;
  /** Speed (m/s). */
  spd: number;
  wp: NavPoint;
  /** Target orbit height above the sphere (m): SUBIR. */
  orb: number;
  /** Cruise speed (m/s) of the ÓRBITA and ESPACIO faces (CRUCERO, VELOC.). */
  crz: number;
}

/** Drum faces of the autopilot panel (`ap.face`). */
export const AP_FACE = { surface: 0, orbit: 1, space: 2 } as const;
export const AP_FACES = ['SUPERFICIE', 'ÓRBITA', 'ESPACIO'];
export const AP_FACE_KEY = 'ap.face';

export interface ApContext {
  pose: ShipPose;
  /** Where the ship is in the world (the waypoints are measured from here). */
  at: readonly number[];
  dt: number;
  /** Height of the gear feet (or hull bottom) above the ground (m). */
  agl: number;
  /** Compass heading now (rad). */
  heading: number;
  /** Horizontal velocity (world, y = 0). */
  vH: V3;
  sel: ApSelection;
  /** Horizontal deceleration the ship can count on (m/s²). */
  brake: number;
  landed: boolean;
  /** Gear travel 0 (up) … 1 (down and locked); 1 for ships without gear. */
  gear: number;
  /** Velocity (local frame), the nose's direction (local), height above the sphere (m). */
  v: V3;
  nose: V3;
  alt: number;
  /** Weight per kilogram the thrusters carry (gravity less the orbit's share, m/s²). */
  g: number;
  /** Acceleration the main engines give flat out (m/s²) and the vertical one the pads can hold (m/s²). */
  aMain: number;
  aVert: number;
  /** Upward acceleration the lift pads can give in all (m/s²): what of the weight they carry. */
  aLift: number;
  /** The orbit and the base (its direction in the local frame), when the flight computer has them. */
  orbit: OrbitInfo | null;
  base: BaseFix | null;
  /** The body the ship flies round (space/body.ts `bodyAt`): orbits, heights and the base are its. */
  body: CelestialBody;
}

/** What the modes ask of the flight computer (null = not driven by the autopilot). */
export interface Setpoints {
  level: boolean;
  /** Compass heading to hold (rad). */
  heading: number | null;
  /** Horizontal velocity to hold (world). */
  vH: V3 | null;
  /** Height above the ground to hold (m). */
  alt: number | null;
  /** Vertical speed to hold (m/s). */
  vz: number | null;
  /** Point the nose along this direction (local frame, unit), with the ship's top toward `pointUp`. */
  point: V3 | null;
  pointUp: V3 | null;
  /** Main engines: the autopilot's command (0..1), the lever doesn't count while it is set. */
  main: number | null;
  /** Annunciations for the displays. */
  show: string[];
}

export const emptySetpoints = (): Setpoints => ({ level: false, heading: null, vH: null, alt: null, vz: null, point: null, pointUp: null, main: null, show: [] });

export interface ApMode {
  id: string;
  /** Panel button label (≤ 8 characters) and HUD name. */
  label: string;
  name: string;
  help: string;
  /** Face of the drum its button is on (AP_FACE); surface modes are cancelled in the orbital regime. */
  face: number;
  /** Modes that switch off when this one engages (they want the same axes). */
  excludes: string[];
  apply(ctx: ApContext, sp: Setpoints): void;
}

/** Descent speed of an automatic landing by height above the ground. */
export function landingSink(agl: number) {
  return agl > 25 ? 3.5 : agl > 8 ? 2 : agl > 2.5 ? 1 : 0.45;
}

const forward = (hdg: number): V3 => [Math.sin(hdg), 0, -Math.cos(hdg)];
const UP: V3 = [0, 1, 0];
const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));
const km = (m: number) => (Math.abs(m) >= 100000 ? `${Math.round(m / 1000)} km` : `${(m / 1000).toFixed(1)} km`);

/** Horizontal velocity (local) and its direction (the nose's heading when too slow to have one). */
function horizontal(c: ApContext): { vh: number; dir: V3 } {
  const vh = Math.hypot(c.v[0], c.v[2]);
  return { vh, dir: vh > 5 ? [c.v[0] / vh, 0, c.v[2] / vh] : forward(c.heading) };
}

/** Main engines on a direction: only once the nose is on it (the pads and RCS can't hold a crooked burn). */
function burn(c: ApContext, dir: V3, want: number) {
  const cos = c.nose[0] * dir[0] + c.nose[1] * dir[1] + c.nose[2] * dir[2];
  return cos > 0.985 ? clamp(want, 0, 1) : 0;
}

/**
 * A main-engine burn that flies a velocity: `dvH` m/s still to gain along `hdir` (the horizon) and
 * a vertical speed `vzT` to reach. The vertical need comes first (gravity's share included, up to
 * 80 % of the engines), the horizontal gets the rest; returns the direction to point and the
 * throttle. Tilting the thrust is how the height is held: the pads alone can't push a climbing
 * ship back down.
 */
function allocate(c: ApContext, hdir: V3, dvH: number, vzT: number): { dir: V3; throttle: number } {
  return allocateA(c, (hdir[0] * dvH) / 10, (hdir[2] * dvH) / 10, vzT, hdir);
}

/** The same from a horizontal acceleration wanted (local x, z; m/s²). */
function allocateA(c: ApContext, ax: number, az: number, vzT: number, fallback: V3): { dir: V3; throttle: number } {
  const aM = Math.max(0.1, c.aMain);
  // the weight too: the main engines are twice as efficient as the lift pads (which then idle),
  // and the vertical error (the pads chase it as well)
  const av = clamp((vzT - c.v[1]) / 6 + c.g, -0.35 * aM, 0.8 * aM);
  const ahMax = Math.sqrt(Math.max(0, aM * aM - av * av));
  const want = Math.hypot(ax, az);
  const ah = Math.min(want, ahMax);
  const hx = want > 1e-6 ? ax / want : fallback[0];
  const hz = want > 1e-6 ? az / want : fallback[2];
  const l = Math.hypot(ah, av);
  if (l < 1e-4) return { dir: fallback, throttle: 0 };
  return { dir: [(hx * ah) / l, av / l, (hz * ah) / l], throttle: l / aM };
}

/** Speed to close a gap (m) no faster than `decel` can stop (see fcs.ts approach). */
function closing(err: number, vmax: number, decel: number, k: number) {
  const d = Math.abs(err);
  return Math.sign(err) * Math.min(vmax, k * d, Math.sqrt(2 * Math.max(0.05, decel) * 0.7 * d));
}

/** Point the nose along `dir` with the top toward `up`; nothing to point at without a direction. */
function pointAlong(sp: Setpoints, dir: V3 | null, up: V3, label: string) {
  if (!dir) {
    sp.show.push(`${label} · SIN VELOCIDAD`);
    return;
  }
  sp.point = dir;
  sp.pointUp = up;
  sp.show.push(label);
}

/**
 * The directions of an orbit in the local frame: prograde is the velocity, radial is up (away from
 * the body), normal is r × v (with up = +y: (v_z, 0, −v_x)).
 */
function velDir(c: ApContext, s: number): V3 | null {
  const v = Math.hypot(c.v[0], c.v[1], c.v[2]);
  return v > 1 ? [(s * c.v[0]) / v, (s * c.v[1]) / v, (s * c.v[2]) / v] : null;
}
function normalDir(c: ApContext, s: number): V3 | null {
  const l = Math.hypot(c.v[0], c.v[2]);
  return l > 1 ? [(s * c.v[2]) / l, 0, (-s * c.v[0]) / l] : null;
}

/** Mode ids by kind, for the excludes lists. */
const SURFACE_IDS = ['lvl', 'alt', 'hdg', 'spd', 'nav', 'to', 'land'];
const POINT_IDS = ['pro', 'retro', 'rad', 'nad', 'nor', 'anor'];
const BURN_IDS = ['sub', 'circ', 'baj', 'ocrz', 'bajq', 'crz'];
const others = (ids: string[], self: string) => ids.filter((x) => x !== self);

export const AP_MODES: ApMode[] = [
  {
    id: 'lvl',
    face: 0,
    label: 'NIVEL',
    name: 'Nivelado',
    help: 'Mantiene las alas niveladas y el morro en el horizonte. Con los mandos de cabeceo y alabeo sueltos, la nave vuelve sola a nivel.',
    excludes: [],
    apply: (_c, sp) => {
      sp.level = true;
      sp.show.push('NIVEL');
    },
  },
  {
    id: 'alt',
    face: 0,
    label: 'ALTURA',
    name: 'Altura sobre el suelo',
    help: 'Mantiene la altura sobre el terreno elegida con el selector ALT (sigue el relieve). R y F siguen mandando mientras los pulsas.',
    excludes: ['to', 'land'],
    apply: (c, sp) => {
      sp.alt = c.sel.alt;
      sp.show.push(`ALT ${c.sel.alt} m`);
    },
  },
  {
    id: 'hdg',
    face: 0,
    label: 'RUMBO',
    name: 'Rumbo',
    help: 'Gira hasta el rumbo del selector RUMBO y lo mantiene. La guiñada manual manda mientras la pulsas.',
    excludes: ['nav'],
    apply: (c, sp) => {
      sp.heading = c.sel.hdg;
      sp.show.push(`RUMBO ${String(Math.round((c.sel.hdg * 180) / Math.PI) % 360).padStart(3, '0')}`);
    },
  },
  {
    id: 'spd',
    face: 0,
    label: 'VELOC.',
    name: 'Velocidad',
    help: 'Control de crucero: mantiene la velocidad del selector VEL hacia donde apunta el morro (o el rumbo del piloto automático). Frena sola al bajar el selector.',
    excludes: ['nav', 'land', 'to'],
    apply: (c, sp) => {
      sp.vH = forward(sp.heading ?? c.heading).map((v) => v * c.sel.spd) as V3;
      sp.show.push(`VEL ${c.sel.spd} m/s`);
    },
  },
  {
    id: 'nav',
    face: 0,
    label: 'NAV',
    name: 'Navegación a un punto',
    help: 'Vuela al punto del selector PUNTO: sube a la altura ALT, pone rumbo, va a la velocidad VEL (o 25 m/s) y frena a tiempo para quedarse en vuelo estacionario encima. Luego pulsa ATERRIZ.',
    excludes: ['hdg', 'spd', 'land', 'to'],
    apply: (c, sp) => {
      if (c.sel.wp.body !== c.body.def.id) {
        sp.show.push(`NAV ${c.sel.wp.name} · OTRO CUERPO`);
        return;
      }
      const { bearing, dist } = bearingTo(c.body, c.at, c.sel.wp);
      const cruise = c.sel.spd > 0 ? c.sel.spd : 25;
      // climb before running: slow while well below the selected height
      const low = c.agl < c.sel.alt - 6;
      let speed = Math.min(cruise, Math.sqrt(2 * Math.max(0.2, c.brake) * Math.max(0, dist - 2)));
      if (low) speed = Math.min(speed, 3);
      const dir: V3 = dist > 0.5 ? [Math.sin(bearing), 0, -Math.cos(bearing)] : [0, 0, 0];
      sp.vH = dir.map((v) => v * speed) as V3;
      if (dist > 20) sp.heading = bearing;
      sp.alt = c.sel.alt;
      sp.level = true;
      sp.show.push(`NAV ${c.sel.wp.name} ${dist < 1000 ? `${Math.round(dist)} m` : `${(dist / 1000).toFixed(1)} km`}`);
    },
  },
  {
    id: 'to',
    face: 0,
    label: 'DESPEG.',
    name: 'Despegue automático',
    help: 'Despega en vertical hasta la altura ALT, sube el tren a 8 m y al llegar pasa a mantener ALTURA y NIVEL.',
    excludes: ['land', 'nav', 'spd', 'alt'],
    apply: (c, sp) => {
      sp.level = true;
      sp.vH = [0, 0, 0];
      sp.vz = c.agl < 2 ? 1.2 : 2.5;
      sp.alt = null;
      sp.show.push('DESPEGUE');
    },
  },
  {
    id: 'land',
    face: 0,
    label: 'ATERRIZ.',
    name: 'Aterrizaje automático',
    help: 'Baja en vertical donde estés, cada vez más despacio, con el tren abajo (lo baja solo), y se posa. Al tocar tierra se desconecta.',
    excludes: ['to', 'nav', 'spd', 'alt'],
    apply: (c, sp) => {
      sp.level = true;
      sp.vH = [0, 0, 0];
      sp.alt = null;
      // wait for the gear before the last metres
      sp.vz = c.gear < 0.99 && c.agl < 6 ? 0 : -landingSink(c.agl);
      sp.show.push(c.landed ? 'EN TIERRA' : c.gear < 0.99 ? 'ATERRIZAJE · TREN' : 'ATERRIZAJE');
    },
  },
  // ---- ÓRBITA: automatic burns ------------------------------------------------------------------------
  {
    id: 'sub',
    face: 1,
    label: 'SUBIR',
    name: 'Subida a órbita',
    help: 'Sube a la órbita circular del selector ÓRBITA: despega con los propulsores de sustentación, sube el tren y, a partir de 150 m, quema el motor principal hacia el horizonte con el morro algo levantado hasta la velocidad orbital. Al llegar pasa a CIRCUL. Necesita los motores principales en marcha; con SOBREPOT. sube antes y gasta más.',
    excludes: [...SURFACE_IDS, ...POINT_IDS, ...others(BURN_IDS, 'sub')],
    apply: (c, sp) => {
      const altT = c.sel.orb;
      const vcT = circularAt(c.body, altT);
      const { vh, dir } = horizontal(c);
      // climb toward the orbit's height (as fast as the engines can still stop), gain the orbit's speed
      const vzT = closing(altT - c.alt, 150, 0.5 * Math.max(0.3, c.aMain), 0.03);
      sp.vz = c.agl < 150 ? Math.max(vzT, 4) : vzT;
      sp.pointUp = UP;
      if (c.agl < 150) {
        // off the pad on the pads, level: the engines light once clear of the ground
        sp.point = dir;
        sp.main = 0;
      } else {
        const a = allocate(c, dir, vcT - vh, vzT);
        sp.point = a.dir;
        sp.main = burn(c, a.dir, a.throttle);
      }
      sp.show.push(`SUBIR ${km(altT)} · ${Math.round(vh)}/${Math.round(vcT)} m/s`);
    },
  },
  {
    id: 'circ',
    face: 1,
    label: 'CIRCUL.',
    name: 'Circularizar',
    help: 'Deja la órbita circular a la altura a la que estás: acelera o frena en horizontal con el motor principal hasta la velocidad orbital de esa altura y anula la velocidad vertical con los propulsores. Se desconecta al conseguirlo.',
    excludes: [...SURFACE_IDS, ...POINT_IDS, ...others(BURN_IDS, 'circ')],
    apply: (c, sp) => {
      const vc = c.orbit?.circular ?? circularAt(c.body, c.alt);
      const { vh, dir } = horizontal(c);
      const dv = vc - vh;
      // the orbit's speed across, no speed up or down: gravity is the orbit's once it is circular
      const a = allocate(c, dir, dv, 0);
      sp.point = a.dir;
      sp.pointUp = UP;
      sp.main = burn(c, a.dir, a.throttle);
      sp.vz = 0;
      sp.show.push(`CIRCUL. ${km(c.alt)} · Δv ${dv >= 0 ? '+' : ''}${dv.toFixed(0)} · ↕ ${c.v[1].toFixed(0)} m/s`);
    },
  },
  {
    id: 'baj',
    face: 1,
    label: 'BAJAR',
    name: 'Bajada a la base',
    help: 'Vuelve de la órbita a la base: espera a que la base quede delante a la distancia justa de frenado, frena con el motor principal hacia ella y baja con los propulsores siguiendo un perfil de altura. Cerca de la base gira el tambor a SUPERFICIE y deja NAV a BASE en vuelo estacionario: luego ATERRIZ.',
    excludes: [...SURFACE_IDS, ...POINT_IDS, ...others(BURN_IDS, 'baj')],
    apply: (c, sp) => {
      const b = c.base;
      if (!b) return;
      const { vh, dir } = horizontal(c);
      const R = c.body.radius;
      const aBrake = Math.max(0.3, 0.5 * c.aMain);
      const dBrake = (vh * vh) / (2 * aBrake);
      // on a closed orbit, coast until the base is ahead just at the braking distance: where the
      // braking profile below meets the speed it has (the first metres per second of braking open
      // the orbit, and from then on it brakes)
      const along = b.ahead >= 0 ? b.ahead * R : b.ahead * R + 2 * Math.PI * R;
      const start = dBrake + 500;
      // the speed that still stops at the base from here
      const profile = Math.sqrt(2 * aBrake * Math.max(0, b.dist - 500));
      // near orbital speed there is nothing to hold: it coasts until the profile meets its speed
      const nearOrbit = vh > 0.8 * (c.orbit?.circular ?? circularAt(c.body, c.alt));
      if (nearOrbit && ((b.ahead < 0 && !!c.orbit?.orbiting) || profile > vh + 20)) {
        // coasting (the stabiliser holds the attitude for nothing); the last minutes the nose
        // turns backwards, ready for the braking burn — holding it longer costs RCS propellant
        const t = Math.max(0, along - start) / Math.max(1, vh);
        if (t < 180) {
          sp.point = [-dir[0], 0, -dir[2]];
          sp.pointUp = UP;
        }
        sp.main = 0;
        sp.show.push(`BAJAR · ESPERA ${t < 90 ? `${Math.round(t)} s` : `${Math.round(t / 60)} min`}${Math.abs(b.cross) > 20000 ? ` · PASA A ${km(Math.abs(b.cross))}` : ''}`);
        return;
      }
      // braking toward the base: the speed that still stops at it, along the way to it (a ship
      // brought down short of it may speed up again, to a few hundred m/s)
      const vT = Math.min(Math.max(vh, 400), profile);
      const vD: V3 = [b.dir[0] * vT, 0, b.dir[2] * vT];
      const ex = vD[0] - c.v[0];
      const ez = vD[2] - c.v[2];
      // on the profile it slows at aBrake by itself (feed-forward), the error on top of that
      const ff = profile <= Math.max(vh, 400) + 20 ? aBrake : 0;
      const ax = ex / 6 - b.dir[0] * ff;
      const az = ez / 6 - b.dir[2] * ff;
      
      // down a height profile that meets the ground near the base
      const altT = clamp(b.dist * 0.06, 300, Math.max(300, c.alt));
      let vzT = closing(altT - c.alt, 150, Math.max(c.aVert, 0.5 * c.aMain), 0.03);
      if (c.agl < 150) vzT = Math.max(vzT, 0);
      sp.vz = vzT;
      sp.vH = vD;
      sp.pointUp = UP;
      if (b.dist < 5000 && vh < 30) {
        // the last metres on the pads, level, nose to the base
        sp.point = b.dist > 30 ? [b.dir[0], 0, b.dir[2]] : dir;
        sp.main = 0;
      } else {
        const a = allocateA(c, ax, az, vzT, [-dir[0], 0, -dir[2]]);
        sp.point = a.dir;
        sp.main = burn(c, a.dir, a.throttle);
      }
      sp.show.push(`BAJAR · FRENADO · ${km(b.dist)} · ${Math.round(vh)} m/s`);
    },
  },
  {
    id: 'ocrz',
    face: 1,
    label: 'CRUCERO',
    name: 'Crucero a altura fija',
    help: 'Control de crucero rápido sobre la superficie: lleva la velocidad horizontal a la del selector CRUCERO y mantiene la altura a la que vas, con el motor principal. Por debajo de la velocidad orbital el motor también sostiene parte del peso (gasta más cuanto más lento vas); a la orbital ya no hace falta. Para cruzar cientos de km sin subir a órbita.',
    excludes: [...SURFACE_IDS, ...POINT_IDS, ...others(BURN_IDS, 'ocrz')],
    apply: (c, sp) => {
      const { vh, dir } = horizontal(c);
      const a = allocate(c, dir, c.sel.crz - vh, 0);
      sp.point = a.dir;
      sp.pointUp = UP;
      sp.main = burn(c, a.dir, a.throttle);
      sp.vz = 0;
      sp.show.push(`CRUCERO ${c.sel.crz} m/s · ${Math.round(vh)} m/s`);
    },
  },
  {
    id: 'bajq',
    face: 1,
    label: 'BAJ.AQUÍ',
    name: 'Frenar y bajar aquí',
    help: 'Baja donde estés, no a la base: frena toda la velocidad horizontal con el motor principal manteniendo la altura (desde órbita tarda minutos y la nave avanza unos v²/2a antes de pararse: a 1.600 m/s y 5 m/s², ~250 km), y luego baja con los propulsores hasta 150 m sobre el suelo. Te deja en vuelo estacionario con la cara SUPERFICIE: ATERRIZ. te posa.',
    excludes: [...SURFACE_IDS, ...POINT_IDS, ...others(BURN_IDS, 'bajq')],
    apply: (c, sp) => {
      const { vh, dir } = horizontal(c);
      sp.pointUp = UP;
      sp.vH = [0, 0, 0];
      if (vh > 30) {
        // braking first, the height held (never under 1 km over the ground while it brakes)
        const vzT = c.agl < 1000 ? closing(1000 - c.agl, 20, 1, 0.05) : 0;
        const a = allocate(c, dir, -vh, vzT);
        sp.point = a.dir;
        sp.main = burn(c, a.dir, a.throttle);
        sp.vz = vzT;
        sp.show.push(`BAJ.AQUÍ · FRENADO · ${Math.round(vh)} m/s`);
        return;
      }
      // slow: down on the pads to hover over the ground
      sp.point = dir;
      sp.main = 0;
      sp.vz = closing(150 - c.agl, 150, Math.max(c.aVert, 0.5 * c.aMain), 0.03);
      sp.show.push(`BAJ.AQUÍ · ${Math.round(c.agl)} m`);
    },
  },
  // ---- ESPACIO: where the nose points --------------------------------------------------------------------
  {
    id: 'pro',
    face: 2,
    label: 'PROGR.',
    name: 'Apuntar a progrado',
    help: 'Apunta el morro hacia donde va la nave (progrado): acelerar ahí sube el otro lado de la órbita. El acelerador sigue siendo tuyo.',
    excludes: ['lvl', 'hdg', ...others(POINT_IDS, 'pro'), ...BURN_IDS],
    apply: (c, sp) => pointAlong(sp, velDir(c, 1), UP, 'PROGRADO'),
  },
  {
    id: 'retro',
    face: 2,
    label: 'RETRÓG.',
    name: 'Apuntar a retrógrado',
    help: 'Apunta el morro contra la marcha (retrógrado): acelerar ahí frena y baja el otro lado de la órbita. Es la posición para frenar y para empezar a bajar.',
    excludes: ['lvl', 'hdg', ...others(POINT_IDS, 'retro'), ...BURN_IDS],
    apply: (c, sp) => pointAlong(sp, velDir(c, -1), UP, 'RETRÓGRADO'),
  },
  {
    id: 'rad',
    face: 2,
    label: 'RADIAL',
    name: 'Apuntar a radial (arriba)',
    help: 'Apunta el morro hacia arriba, lejos de la Luna (radial exterior): acelerar ahí gira la órbita, subiendo lo que tienes delante y bajando lo de detrás.',
    excludes: ['lvl', 'hdg', ...others(POINT_IDS, 'rad'), ...BURN_IDS],
    apply: (c, sp) => pointAlong(sp, UP, horizontal(c).dir, 'RADIAL'),
  },
  {
    id: 'nad',
    face: 2,
    label: 'NADIR',
    name: 'Apuntar al nadir (abajo)',
    help: 'Apunta el morro hacia el centro de la Luna (radial interior): para mirar abajo por el parabrisas o para bajar lo que tienes delante.',
    excludes: ['lvl', 'hdg', ...others(POINT_IDS, 'nad'), ...BURN_IDS],
    apply: (c, sp) => pointAlong(sp, [0, -1, 0], horizontal(c).dir, 'NADIR'),
  },
  {
    id: 'nor',
    face: 2,
    label: 'NORMAL',
    name: 'Apuntar a normal',
    help: 'Apunta el morro perpendicular al plano de la órbita (normal, a la izquierda de la marcha): acelerar ahí inclina la órbita sin cambiar su altura.',
    excludes: ['lvl', 'hdg', ...others(POINT_IDS, 'nor'), ...BURN_IDS],
    apply: (c, sp) => pointAlong(sp, normalDir(c, 1), UP, 'NORMAL'),
  },
  {
    id: 'anor',
    face: 2,
    label: 'ANTINOR.',
    name: 'Apuntar a antinormal',
    help: 'Apunta el morro al otro lado del plano de la órbita (antinormal, a la derecha de la marcha): inclina la órbita hacia el otro lado.',
    excludes: ['lvl', 'hdg', ...others(POINT_IDS, 'anor'), ...BURN_IDS],
    apply: (c, sp) => pointAlong(sp, normalDir(c, -1), UP, 'ANTINORMAL'),
  },
  {
    id: 'crz',
    face: 2,
    label: 'VELOC.',
    name: 'Velocidad respecto al cuerpo',
    help: 'Control de crucero del espacio: lleva la velocidad respecto al cuerpo que dominas (hoy la Luna) a la del selector CRUCERO, apuntando a progrado para acelerar o a retrógrado para frenar, y luego deja la nave en inercia (en el vacío no se pierde velocidad). Solo vuelve a quemar si se aparta más de 2 m/s. No sostiene la altura: en órbita cambia la órbita.',
    excludes: ['lvl', 'hdg', ...POINT_IDS, ...others(BURN_IDS, 'crz')],
    apply: (c, sp) => {
      const v = Math.hypot(c.v[0], c.v[1], c.v[2]);
      const dv = c.sel.crz - v;
      const dir = velDir(c, 1) ?? c.nose;
      const want: V3 = dv >= 0 ? dir : [-dir[0], -dir[1], -dir[2]];
      sp.point = want;
      sp.pointUp = UP;
      sp.main = Math.abs(dv) < 2 ? 0 : burn(c, want, clamp(Math.abs(dv) / (4 * Math.max(0.1, c.aMain)), 0.05, 1));
      sp.show.push(`VELOC. ${c.sel.crz} m/s · ${Math.round(v)}`);
    },
  },
];

export const AP_KEY = (id: string) => `ap.${id}`;

/** Default knob positions (a ship may give its own lists). Orbits: km above the sphere. */
export const AP_DEFAULTS = { alts: [5, 10, 20, 35, 60, 100, 200, 400], speeds: [0, 5, 10, 20, 35, 50, 80], orbits: [15, 20, 30, 50, 80, 120], cruise: [0, 50, 100, 200, 400, 800, 1200, 1600, 2000, 2500] };

/** Heading knob: 24 positions of 15°. */
export const HDG_STEPS = 24;

/** What the knobs select right now. */
export function apSelection(ap: AutopilotDef, sw: Record<string, number>): ApSelection {
  const pick = <T>(list: T[], key: string) => list[Math.max(0, Math.min(list.length - 1, Math.round(sw[key] ?? 0)))];
  return {
    alt: pick(ap.alts, 'ap.alt.sel'),
    hdg: ((Math.round(sw['ap.hdg.sel'] ?? 0) % HDG_STEPS) * 2 * Math.PI) / HDG_STEPS,
    spd: pick(ap.speeds, 'ap.spd.sel'),
    wp: pick(NAV_POINTS, 'ap.wp'),
    orb: pick(ap.orbits ?? AP_DEFAULTS.orbits, 'ap.orb.sel') * 1000,
    crz: pick(ap.cruise ?? AP_DEFAULTS.cruise, 'ap.crz.sel'),
  };
}

/** Modes a ship's autopilot offers, in stacking order. */
export const apModes = (ap: AutopilotDef) => AP_MODES.filter((m) => ap.modes.includes(m.id));
