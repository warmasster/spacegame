// Celestial bodies as data, and the maths of flying around one: gravity toward its centre (μ/r²),
// the local frame (up away from the centre, north toward the pole on the horizon, east), height
// above its sphere and the orbit a state vector is on. The Moon is the first body; nothing here
// knows which one it is.
//
// World frame: the Moon's centre straight below the origin, the base (its home site, space/sites.ts)
// on that line, so there y is up, north −z and east +x. Nothing may rely on it: "up" is `frameAt`,
// "the ground" is `heightAboveGround` and "the body" is `bodyAt`, anywhere round any body.

import { MOON, type BodyDef } from '../constants.js';
import type { V3 } from '../ship/geom.js';
import { homeSite, siteDir, siteMods } from './sites.js';
import { BodySurface, MOON_SURFACE, type SurfaceDef } from './surface.js';
import { basisQuat, tangentAxes } from './tangent.js';

export { basisQuat } from './tangent.js';

export interface CelestialBody {
  def: BodyDef;
  /** Mean radius (m) and gravitational parameter μ = GM (m³/s²). */
  radius: number;
  mu: number;
  /** Centre in the world frame (m) and the rotation axis toward its north pole (unit). */
  center: V3;
  pole: V3;
  /** Its relief (null: a smooth sphere). The ground itself comes from `surfaceOf`. */
  surface: SurfaceDef | null;
}

/** Latitude of the landing site (deg): the pole stands this high over the base's north horizon. */
export const BASE_LATITUDE = 20;

function makeBody(def: BodyDef, latitudeDeg: number): CelestialBody {
  const lat = (latitudeDeg * Math.PI) / 180;
  return {
    def,
    radius: def.radius,
    // surface gravity is the game's number: μ from it, so the weight at the base is exactly MOON.gravity
    mu: def.gravity * def.radius * def.radius,
    center: [0, -def.radius, 0],
    pole: [0, Math.sin(lat), -Math.cos(lat)],
    surface: def.id === 'moon' ? MOON_SURFACE : null,
  };
}

export const MOON_BODY = makeBody(MOON, BASE_LATITUDE);

/** Every body by id (workers get an id, not the object). */
export const BODIES: Record<string, CelestialBody> = { moon: MOON_BODY };
const BODY_LIST: CelestialBody[] = Object.values(BODIES);

/**
 * The body whose pull rules at world point `p` (its sphere of influence): what "up", the ground,
 * the orbit and the flight computer's references are measured against there. With one body it is
 * always the Moon; nothing that asks here needs to change when there are more (the one whose
 * surface gravity-weighted pull is strongest wins, a stand-in for proper spheres of influence).
 */
export function bodyAt(p: readonly number[]): CelestialBody {
  if (BODY_LIST.length === 1) return BODY_LIST[0];
  let best = BODY_LIST[0];
  let bestPull = -1;
  for (const b of BODY_LIST) {
    const x = p[0] - b.center[0];
    const y = p[1] - b.center[1];
    const z = p[2] - b.center[2];
    const pull = b.mu / (x * x + y * y + z * z);
    if (pull > bestPull) {
      bestPull = pull;
      best = b;
    }
  }
  return best;
}

/** The surface of a body in the world being played, or null (a smooth sphere). */
export type Surfaces = (b: CelestialBody) => BodySurface | null;

const surfaces = new Map<CelestialBody, Map<number, BodySurface>>();

/**
 * The ground of a body in a world (its seed varies it all): cached, one per body and seed, with the
 * modifiers of its sites already laid (the static layer: space/sites.ts). Craters and whatever else
 * happens during the game go on its dynamic layer (`surface.addMod`, `surface.mods.setDynamic`).
 */
export function surfaceOf(b: CelestialBody, seed: number): BodySurface | null {
  if (!b.surface) return null;
  let bySeed = surfaces.get(b);
  if (!bySeed) surfaces.set(b, (bySeed = new Map()));
  let s = bySeed.get(seed);
  if (s) return s;
  s = bareSurface(b, seed)!;
  s.setStatic(siteMods(b.def.id, seed, (id) => BODIES[id]));
  bySeed.set(seed, s);
  return s;
}

/**
 * A body's ground in a world without any modifier (a worker's copy: its jobs bring theirs), levelled
 * like every one of that world: the natural ground of its home site on the mean sphere.
 */
export function bareSurface(b: CelestialBody, seed: number): BodySurface | null {
  if (!b.surface) return null;
  return new BodySurface(b.surface, b, seed, homeSite(b.def.id) ? homeDir(b) : undefined);
}

/** A body by id (throws on an unknown one: data error). */
export function bodyById(id: string): CelestialBody {
  const b = BODIES[id];
  if (!b) throw new Error(`unknown body "${id}"`);
  return b;
}

/** Direction toward the sun (world, unit) from its azimuth and elevation over the base (deg). */
export function sunDirection(azDeg: number, elDeg: number): V3 {
  const az = (azDeg * Math.PI) / 180;
  const el = (elDeg * Math.PI) / 180;
  return [Math.sin(az) * Math.cos(el), Math.sin(el), -Math.cos(az) * Math.cos(el)];
}

/** A point is in the body's shadow (behind it from the sun, within its radius of the axis). */
export function inShadow(b: CelestialBody, p: readonly number[], sun: readonly number[]) {
  const x = p[0] - b.center[0];
  const y = p[1] - b.center[1];
  const z = p[2] - b.center[2];
  const along = x * sun[0] + y * sun[1] + z * sun[2];
  if (along >= 0) return false;
  return x * x + y * y + z * z - along * along < b.radius * b.radius;
}

/** Distance from the centre (m). */
export function radiusOf(b: CelestialBody, p: readonly number[]) {
  const x = p[0] - b.center[0];
  const y = p[1] - b.center[1];
  const z = p[2] - b.center[2];
  return Math.sqrt(x * x + y * y + z * z);
}

/** Height above the body's mean sphere (m). */
export function altitudeOf(b: CelestialBody, p: readonly number[]) {
  return radiusOf(b, p) - b.radius;
}

/** Gravity at a world point (m/s², world axes) into `out`. */
export function gravityAt(b: CelestialBody, p: readonly number[], out: V3): V3 {
  const x = p[0] - b.center[0];
  const y = p[1] - b.center[1];
  const z = p[2] - b.center[2];
  const r2 = x * x + y * y + z * z;
  const r = Math.sqrt(r2);
  const k = -b.mu / (r2 * r);
  out[0] = x * k;
  out[1] = y * k;
  out[2] = z * k;
  return out;
}

/** Local frame at a point: up (away from the centre), north (toward the pole, on the horizon), east. */
export interface LocalFrame {
  up: V3;
  north: V3;
  east: V3;
}

export const localFrame = (): LocalFrame => ({ up: [0, 1, 0], north: [0, 0, -1], east: [1, 0, 0] });

/** Fill `f` with the local frame at world point `p`. At the poles north falls back to the base's. */
export function frameAt(b: CelestialBody, p: readonly number[], f: LocalFrame): LocalFrame {
  const r = radiusOf(b, p) || 1;
  const u = f.up;
  u[0] = (p[0] - b.center[0]) / r;
  u[1] = (p[1] - b.center[1]) / r;
  u[2] = (p[2] - b.center[2]) / r;
  const pole = b.pole;
  const d = pole[0] * u[0] + pole[1] * u[1] + pole[2] * u[2];
  let nx = pole[0] - u[0] * d;
  let ny = pole[1] - u[1] * d;
  let nz = pole[2] - u[2] * d;
  let nl = Math.sqrt(nx * nx + ny * ny + nz * nz);
  if (nl < 1e-6) {
    // at a pole: the base's north, flattened on this horizon
    const e = -u[2];
    nx = 0 - u[0] * e;
    ny = 0 - u[1] * e;
    nz = -1 - u[2] * e;
    nl = Math.sqrt(nx * nx + ny * ny + nz * nz) || 1;
  }
  const n = f.north;
  n[0] = nx / nl;
  n[1] = ny / nl;
  n[2] = nz / nl;
  // east = north × up
  const e = f.east;
  e[0] = n[1] * u[2] - n[2] * u[1];
  e[1] = n[2] * u[0] - n[0] * u[2];
  e[2] = n[0] * u[1] - n[1] * u[0];
  return f;
}

/** The orbit a state vector is on (conic around the body; altitudes above its sphere). */
export interface OrbitInfo {
  /** Speed (m/s) and height above the sphere (m). */
  speed: number;
  altitude: number;
  /** Highest and lowest points (m above the sphere); apoapsis is Infinity when it escapes. */
  apoapsis: number;
  periapsis: number;
  eccentricity: number;
  /** Seconds for one turn (Infinity when it escapes). */
  period: number;
  /** Circular orbit speed at this height (m/s). */
  circular: number;
  /** Clear of the surface all the way round (periapsis above the ground). */
  orbiting: boolean;
}

export const orbitInfo = (): OrbitInfo => ({ speed: 0, altitude: 0, apoapsis: 0, periapsis: 0, eccentricity: 0, period: Infinity, circular: 0, orbiting: false });

/** Orbit of a body at world `p` moving at world `v`. */
export function orbitOf(b: CelestialBody, p: readonly number[], v: readonly number[]): OrbitInfo {
  return orbitInto(b, p, v, orbitInfo());
}

/**
 * Orbital regime of the flight computer (flight/model.ts has the same thresholds with hysteresis):
 * fast across the ground or high above it. Stateless, for the interlocks.
 */
export function orbitalRegime(b: CelestialBody, p: readonly number[], v: readonly number[]) {
  const x = p[0] - b.center[0];
  const y = p[1] - b.center[1];
  const z = p[2] - b.center[2];
  const r = Math.sqrt(x * x + y * y + z * z) || 1;
  const vr = (v[0] * x + v[1] * y + v[2] * z) / r;
  const vh2 = Math.max(0, v[0] * v[0] + v[1] * v[1] + v[2] * v[2] - vr * vr);
  return vh2 > 150 * 150 || r - b.radius > 15000;
}

/** Where the base is from a point: distance over the surface (m) and the direction to it on the local horizon (world, unit). */
export interface BaseFix {
  dist: number;
  dir: V3;
  /** Angle ahead along the orbit to the base (rad, −π..π: negative = behind) and how far off the orbit's track it is (m). */
  ahead: number;
  cross: number;
}

export const baseFix = (): BaseFix => ({ dist: 0, dir: [0, 0, -1], ahead: 0, cross: 0 });

/**
 * The body's home (its home site, space/sites.ts) as seen from `p` moving at `v`: great-circle
 * distance, the heading to it on the local horizon, and where it lies against the orbit (ahead /
 * across the ground track).
 */
export function baseFrom(b: CelestialBody, p: readonly number[], v: readonly number[], out: BaseFix): BaseFix {
  const rx = p[0] - b.center[0];
  const ry = p[1] - b.center[1];
  const rz = p[2] - b.center[2];
  const r = Math.sqrt(rx * rx + ry * ry + rz * rz) || 1;
  const ux = rx / r;
  const uy = ry / r;
  const uz = rz / r;
  // the home's direction from the centre
  const home = homeDir(b);
  const Bx = home[0];
  const By = home[1];
  const Bz = home[2];
  const c = Math.max(-1, Math.min(1, ux * Bx + uy * By + uz * Bz));
  out.dist = Math.acos(c) * b.radius;
  // on the horizon: the base's direction less its vertical part
  let tx = Bx - ux * c;
  let ty = By - uy * c;
  let tz = Bz - uz * c;
  const tl = Math.sqrt(tx * tx + ty * ty + tz * tz);
  if (tl > 1e-9) {
    tx /= tl;
    ty /= tl;
    tz /= tl;
    out.dir[0] = tx;
    out.dir[1] = ty;
    out.dir[2] = tz;
  }
  // the orbit's plane: h = r × v
  let hx = uy * v[2] - uz * v[1];
  let hy = uz * v[0] - ux * v[2];
  let hz = ux * v[1] - uy * v[0];
  const hl = Math.sqrt(hx * hx + hy * hy + hz * hz);
  if (hl < 1e-6) {
    out.ahead = 0;
    out.cross = 0;
    return out;
  }
  hx /= hl;
  hy /= hl;
  hz /= hl;
  // (r̂ × B̂) · ĥ: positive when the base is ahead along the motion
  const cx = uy * Bz - uz * By;
  const cy = uz * Bx - ux * Bz;
  const cz = ux * By - uy * Bx;
  out.ahead = Math.atan2(cx * hx + cy * hy + cz * hz, c);
  out.cross = Math.asin(Math.max(-1, Math.min(1, Bx * hx + By * hy + Bz * hz))) * b.radius;
  return out;
}

/** Turn a fix's direction into the local frame `f` (x east, y up, z south), in place. */
export function localBase(fix: BaseFix, f: LocalFrame): BaseFix {
  const d = fix.dir;
  const x = d[0] * f.east[0] + d[1] * f.east[1] + d[2] * f.east[2];
  const y = d[0] * f.up[0] + d[1] * f.up[1] + d[2] * f.up[2];
  const z = -(d[0] * f.north[0] + d[1] * f.north[1] + d[2] * f.north[2]);
  d[0] = x;
  d[1] = y;
  d[2] = z;
  return fix;
}

/** The same as orbitOf, into `out` (no allocation: every step of a flight computer). */
export function orbitInto(b: CelestialBody, p: readonly number[], v: readonly number[], out: OrbitInfo): OrbitInfo {
  const rx = p[0] - b.center[0];
  const ry = p[1] - b.center[1];
  const rz = p[2] - b.center[2];
  const r = Math.sqrt(rx * rx + ry * ry + rz * rz);
  const v2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
  const mu = b.mu;
  const energy = v2 / 2 - mu / r;
  // angular momentum h = r × v
  const hx = ry * v[2] - rz * v[1];
  const hy = rz * v[0] - rx * v[2];
  const hz = rx * v[1] - ry * v[0];
  const h2 = hx * hx + hy * hy + hz * hz;
  const e = Math.sqrt(Math.max(0, 1 + (2 * energy * h2) / (mu * mu)));
  const pOrb = h2 / mu;
  const peri = pOrb / (1 + e);
  const apo = e < 1 ? pOrb / (1 - e) : Infinity;
  const a = energy < 0 ? -mu / (2 * energy) : Infinity;
  out.speed = Math.sqrt(v2);
  out.altitude = r - b.radius;
  out.apoapsis = apo - b.radius;
  out.periapsis = peri - b.radius;
  out.eccentricity = e;
  out.period = Number.isFinite(a) ? 2 * Math.PI * Math.sqrt((a * a * a) / mu) : Infinity;
  out.circular = Math.sqrt(mu / r);
  out.orbiting = peri - b.radius > 3000;
  return out;
}

/** Circular orbit speed at a height above the body's sphere (m/s). */
export const circularAt = (b: CelestialBody, alt: number) => Math.sqrt(b.mu / (b.radius + alt));

/**
 * The tangent frame at world point `p`: x east, y up (away from the centre), z south — the world's
 * axes at the base. Rotation (unit quaternion, into `q`) and the axes (world, unit) into `e`, `u`, `s`.
 */
export function tangentFrame(b: CelestialBody, p: readonly number[], q: number[], e: number[] = _te, u: number[] = _tu, s: number[] = _ts) {
  const c = b.center;
  const x = p[0] - c[0];
  const y = p[1] - c[1];
  const z = p[2] - c[2];
  const l = Math.sqrt(x * x + y * y + z * z) || 1;
  u[0] = x / l;
  u[1] = y / l;
  u[2] = z / l;
  tangentAxes(b.pole, u, e, s);
  return basisQuat(e, u, s, q);
}

const _te = [0, 0, 0];
const _tu = [0, 0, 0];
const _ts = [0, 0, 0];

/** Unit direction of a body's home site from its centre (the world origin's when it has none). */
export function homeDir(b: CelestialBody): readonly number[] {
  let d = homes.get(b);
  if (d) return d;
  const site = homeSite(b.def.id);
  if (site) d = siteDir(site, bodyById);
  else {
    const l = Math.hypot(b.center[0], b.center[1], b.center[2]) || 1;
    d = [-b.center[0] / l, -b.center[1] / l, -b.center[2] / l];
  }
  homes.set(b, d);
  return d;
}
const homes = new Map<CelestialBody, readonly number[]>();

/**
 * Height of world point `p` above the ground under it (m, along the local vertical; negative:
 * under it): the body's surface (with every modifier on it) toward the point, or its bare sphere.
 */
export function heightAboveGround(b: CelestialBody, p: readonly number[], surface: BodySurface | null, minFeature = 0): number {
  const x = p[0] - b.center[0];
  const y = p[1] - b.center[1];
  const z = p[2] - b.center[2];
  const r = Math.sqrt(x * x + y * y + z * z) || 1;
  _hd[0] = x / r;
  _hd[1] = y / r;
  _hd[2] = z / r;
  return r - b.radius - (surface ? surface.height(_hd, minFeature) : 0);
}
const _hd = [0, 0, 0];
