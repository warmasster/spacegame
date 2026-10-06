// The galaxy (docs/ESPACIO.md, docs/MUNDO.md §14): star systems as data, the same on every machine
// (a fixed seed: it is the game's map). Coordinates by levels:
//
//   galaxy   where a system is, in light-years (float64) — Sol at the origin
//   system   where something is inside a system: the world frame, metres (float64)
//
// Only a few systems exist physically (a "region" each: their bodies, their ground, their gravity).
// A region is a slice of the one float64 world frame, SYSTEM_SPACING apart from the next: nothing of
// a region reaches another (gravity 10⁸ km away is nil), the physics, the frames, the network and
// interest work unchanged, and a point's region is where it is. Precision stays under a millimetre
// in every region (1e11 m: 1.5e-5 m steps). Travelling between them is a jump (shared/space/jump.ts).
// The rest of the galaxy lives in the world simulation (numbers: its people, its economy).

import { draw, hash2, Rng } from '../../sim/core/rng.js';
import type { BodyDef } from '../constants.js';

export type V3 = [number, number, number];

export const GALAXY_SEED = 0x5ea1;
/** Systems in the galaxy. */
export const GALAXY_SIZE = 2000;
/** Systems with a region (bodies you can fly to), besides Sol. */
export const PHYSICAL_SYSTEMS = 3;
/** Distance between regions of the world frame (m). */
export const SYSTEM_SPACING = 1e11;

export interface StarClass {
  cls: string;
  /** Share of stars of this class. */
  share: number;
  /** Surface temperature (K) and luminosity (Sun = 1): ranges. */
  temp: [number, number];
  lum: [number, number];
}

/** Main-sequence classes, roughly as common as in the sky round the Sun. */
export const STAR_CLASSES: StarClass[] = [
  { cls: 'M', share: 0.73, temp: [2400, 3700], lum: [0.0005, 0.07] },
  { cls: 'K', share: 0.13, temp: [3700, 5200], lum: [0.08, 0.6] },
  { cls: 'G', share: 0.08, temp: [5200, 6000], lum: [0.6, 1.5] },
  { cls: 'F', share: 0.04, temp: [6000, 7500], lum: [1.5, 5] },
  { cls: 'A', share: 0.015, temp: [7500, 10000], lum: [5, 25] },
  { cls: 'B', share: 0.004, temp: [10000, 30000], lum: [25, 30000] },
  { cls: 'O', share: 0.001, temp: [30000, 45000], lum: [30000, 500000] },
];

export interface StarSystem {
  id: number;
  name: string;
  /** Galactic position (light-years; Sol at the origin). */
  ly: V3;
  star: { cls: string; temp: number; lum: number; color: [number, number, number] };
  /** Planets it has (the world may make them more than a number). */
  planets: number;
}

/** A system that exists physically: its slice of the world frame and its body. */
export interface Region {
  index: number;
  system: StarSystem;
  /** Its slice of the world frame starts here (Sol's: the origin). */
  origin: V3;
  /** Its body (Sol's is the Moon, defined elsewhere: null here). */
  body: { def: BodyDef; center: V3 } | null;
}

const SYLLABLES = ['ka', 'lor', 've', 'ni', 'tan', 'ar', 'is', 'mo', 'ra', 'zen', 'ul', 'do', 'ri', 'sa', 'kel', 'vo', 'ter', 'an', 'yx', 'el', 'or', 'pa', 'thu', 'ne', 'qua', 'gi', 'bro', 'mi'];

function starName(key: number): string {
  const n = 2 + (draw(key, 10) % 2);
  let s = '';
  for (let i = 0; i < n; i++) s += SYLLABLES[draw(key, 11 + i) % SYLLABLES.length];
  const name = s[0].toUpperCase() + s.slice(1);
  return draw(key, 20) % 4 === 0 ? `${name} ${['Mayor', 'Menor', 'Prima', 'Austral'][draw(key, 21) % 4]}` : name;
}

/** Colour of a black body at `t` K (sRGB-ish, 0…1), Tanner Helland's fit. */
export function starColor(t: number): [number, number, number] {
  const k = t / 100;
  const r = k <= 66 ? 255 : 329.7 * Math.pow(k - 60, -0.1332);
  const g = k <= 66 ? 99.47 * Math.log(k) - 161.12 : 288.12 * Math.pow(k - 60, -0.0755);
  const b = k >= 66 ? 255 : k <= 19 ? 0 : 138.52 * Math.log(k - 10) - 305.04;
  const c = (v: number) => Math.min(1, Math.max(0, v / 255));
  return [c(r), c(g), c(b)];
}

let systems: StarSystem[] | null = null;

/** Every system of the galaxy (made once; the same everywhere). Sol is the first. */
export function galaxy(): readonly StarSystem[] {
  if (systems) return systems;
  const out: StarSystem[] = [{ id: 0, name: 'Sol', ly: [0, 0, 0], star: { cls: 'G', temp: 5778, lum: 1, color: starColor(5778) }, planets: 8 }];
  const r = new Rng(hash2(GALAXY_SEED, 1));
  for (let id = 1; id < GALAXY_SIZE; id++) {
    // a flattened disc round the Sun, uniform in area: about the stars within 60 ly of us
    const a = r.float() * Math.PI * 2;
    const d = 60 * Math.sqrt(r.float());
    const ly: V3 = [Math.cos(a) * d, (r.float() - 0.5) * 16, Math.sin(a) * d];
    let pick = r.float();
    let c = STAR_CLASSES[0];
    for (const k of STAR_CLASSES) {
      if (pick < k.share) {
        c = k;
        break;
      }
      pick -= k.share;
    }
    const temp = c.temp[0] + (c.temp[1] - c.temp[0]) * r.float();
    const lum = c.lum[0] * Math.pow(c.lum[1] / c.lum[0], r.float());
    out.push({ id, name: starName(hash2(GALAXY_SEED, id)), ly, star: { cls: c.cls, temp, lum, color: starColor(temp) }, planets: r.int(9) });
  }
  // one name, one system: repeats get a catalogue number
  const seen = new Map<string, number>();
  for (const s of out) {
    const n = (seen.get(s.name) ?? 0) + 1;
    seen.set(s.name, n);
    if (n > 1) s.name = `${s.name} ${n}`;
  }
  systems = out;
  return out;
}

export const systemById = (id: number): StarSystem | undefined => galaxy()[id];

/** Light-years between two systems. */
export function lyBetween(a: StarSystem, b: StarSystem): number {
  return Math.hypot(a.ly[0] - b.ly[0], a.ly[1] - b.ly[1], a.ly[2] - b.ly[2]);
}

/** Systems within `ly` of one, nearest first. */
export function systemsNear(from: StarSystem, ly: number): StarSystem[] {
  return galaxy()
    .filter((s) => s !== from && lyBetween(from, s) <= ly)
    .sort((a, b) => lyBetween(from, a) - lyBetween(from, b));
}

let regions: Region[] | null = null;

/** The systems that exist physically: Sol, then the nearest ones, each with a rocky body. */
export function physicalRegions(): readonly Region[] {
  if (regions) return regions;
  const sol = galaxy()[0];
  const near = systemsNear(sol, Infinity).slice(0, PHYSICAL_SYSTEMS);
  regions = [{ index: 0, system: sol, origin: [0, 0, 0], body: null }];
  near.forEach((system, i) => {
    const index = i + 1;
    const origin: V3 = [0, 0, -index * SYSTEM_SPACING];
    const key = hash2(GALAXY_SEED ^ 0xb0d7, system.id);
    const radius = 700e3 + (draw(key, 0) / 4294967296) * 1700e3;
    const gravity = 0.9 + (draw(key, 1) / 4294967296) * 1.7;
    const def: BodyDef = { id: `sys${system.id}`, name: `${system.name} I`, radius, gravity, atmosphereDensity: 0, ground: 'regolith' };
    // like the Moon at Sol: the body's top at the region's origin
    regions!.push({ index, system, origin, body: { def, center: [origin[0], origin[1] - radius, origin[2]] } });
  });
  return regions;
}

/** The region a world point is in (0: Sol). */
export function regionAt(p: readonly number[]): Region {
  const r = physicalRegions();
  const i = Math.round(-p[2] / SYSTEM_SPACING);
  return r[Math.max(0, Math.min(r.length - 1, i))];
}
