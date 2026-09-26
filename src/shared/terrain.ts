// Deterministic lunar terrain. Every client (and later the server) evaluates the same
// function, so the world needs no download and physics/visuals agree everywhere.
//
// Layers: rolling regolith (fbm) + distant massifs (ridged noise) + a multi-scale crater
// field (hash-placed, size–frequency like real maria) + one landmark crater. The landing
// site around the origin is gently flattened (future base).

import type { TerrainEdit } from './protocol.js';
import { CellRandom, Simplex2, clamp, smax, smin, smoothstep } from './noise.js';

interface CraterLevel {
  cell: number;
  p: number;
  rMin: number;
  rMax: number;
}

const CRATER_LEVELS: CraterLevel[] = [
  { cell: 1400, p: 0.3, rMin: 0.14, rMax: 0.36 },
  { cell: 480, p: 0.42, rMin: 0.14, rMax: 0.4 },
  { cell: 170, p: 0.5, rMin: 0.12, rMax: 0.42 },
  { cell: 62, p: 0.55, rMin: 0.12, rMax: 0.42 },
  { cell: 23, p: 0.55, rMin: 0.12, rMax: 0.42 },
  { cell: 8.5, p: 0.5, rMin: 0.12, rMax: 0.4 },
  { cell: 3.2, p: 0.35, rMin: 0.1, rMax: 0.36 },
];

export interface TerrainSample {
  height: number;
  /** Albedo multiplier (~0.75 .. 1.4): fresh ejecta is brighter. */
  albedo: number;
}

/** Mean lunar radius (m). Heights include the curvature drop so the horizon is where it should be
 *  (~2.5 km for a standing astronaut); physics and rendering share it, so they always agree. */
export const BODY_RADIUS = 1_737_400;

/** Landmark crater visible from the landing site. */
export const LANDMARK = { x: 1150, z: -820, radius: 380 };
/** Radius of the flattened landing site (m). */
export const LANDING_RADIUS = 55;

export class LunarTerrain {
  private simplex: Simplex2;
  private ridge: Simplex2;
  private rnd = new CellRandom();
  readonly seed: number;
  /** Craters and other modifications on top of the procedural surface (deformable terrain). */
  edits: TerrainEdit[] = [];

  constructor(seed: number) {
    this.seed = seed;
    this.simplex = new Simplex2(seed);
    this.ridge = new Simplex2(seed * 7 + 13);
  }

  /** Height in metres. `minFeature` (m) skips detail smaller than the sampling step (LOD). */
  height(x: number, z: number, minFeature = 0): number {
    return this.sample(x, z, minFeature, _tmp).height;
  }

  sample(x: number, z: number, minFeature: number, out: TerrainSample): TerrainSample {
    const s = this.simplex;
    // --- rolling regolith -------------------------------------------------------------
    let h = 0;
    let amp = 26;
    let f = 1 / 2600;
    for (let i = 0; i < 9; i++) {
      const wavelength = 1 / f;
      if (wavelength < minFeature * 2) break;
      h += amp * s.noise(x * f + i * 17.3, z * f - i * 9.1);
      amp *= 0.4;
      f *= 2.55;
    }

    // --- distant massifs (Taurus–Littrow–like valley) ---------------------------------
    // Lunar mountains are billions of years of impact-smoothed domes: broad, rounded, no ridgelines.
    const d = Math.hypot(x, z);
    const mask = smoothstep(2400, 7200, d);
    if (mask > 0) {
      let m = 0;
      let a = 1;
      let fr = 1 / 10000;
      for (let i = 0; i < 5; i++) {
        if (1 / fr < minFeature * 2) break;
        m += a * this.ridge.noise(x * fr + 3.1 * i, z * fr - 1.7 * i);
        a *= 0.36;
        fr *= 2.2;
      }
      const dome = smoothstep(-0.55, 1.25, m);
      h += mask * (dome * dome * (3 - 2 * dome) * 1500 - 100);
    }

    // --- crater field -------------------------------------------------------------------
    let craterH = 0;
    let albedo = 1 + 0.06 * s.noise(x / 310, z / 310) + 0.04 * s.noise(x / 57, z / 57);
    const rnd = this.rnd;
    for (let li = 0; li < CRATER_LEVELS.length; li++) {
      const L = CRATER_LEVELS[li];
      if (L.cell * L.rMax * 2 < minFeature * 1.5) break;
      const cx = Math.floor(x / L.cell);
      const cz = Math.floor(z / L.cell);
      for (let oz = -1; oz <= 1; oz++) {
        for (let ox = -1; ox <= 1; ox++) {
          rnd.reset(cx + ox, cz + oz, this.seed * 31 + li);
          if (rnd.next() > L.p) continue;
          const px = (cx + ox + rnd.next()) * L.cell;
          const pz = (cz + oz + rnd.next()) * L.cell;
          const u = rnd.next();
          const radius = (L.rMin + (L.rMax - L.rMin) * u * u) * L.cell;
          const age = rnd.next();
          const dx = x - px;
          const dz = z - pz;
          const r = Math.sqrt(dx * dx + dz * dz) / radius;
          // influence must stay within one cell (rMax · 2.2 < 1) or the 3×3 search would clip it
          if (r > 2.2) continue;
          craterH += craterProfile(r, radius, age);
          if (age < 0.18) {
            // bright ejecta halo, faded smoothly to zero at the evaluation cut-off (no rings)
            const halo = Math.exp(-(r - 1) * (r - 1) * 1.6) * (0.6 + 0.4 * smoothstep(0.4, 0.8, r)) * (1 - smoothstep(1.6, 2.2, r));
            albedo += 0.35 * (1 - age / 0.18) * halo;
          }
        }
      }
    }
    h += craterH;

    // landmark crater
    {
      const dx = x - LANDMARK.x;
      const dz = z - LANDMARK.z;
      const r = Math.sqrt(dx * dx + dz * dz) / LANDMARK.radius;
      if (r < 3) {
        h += craterProfile(r, LANDMARK.radius, 0.05) * 0.9;
        albedo += 0.22 * Math.exp(-(r - 1.1) * (r - 1.1) * 0.9);
      }
    }

    // flatten the landing site
    const flat = 1 - smoothstep(LANDING_RADIUS * 0.45, LANDING_RADIUS, d);
    if (flat > 0) {
      const smooth = 26 * s.noise(x / 2600, z / 2600) + 10.4 * s.noise(x / 1020 + 17.3, z / 1020 - 9.1);
      h = h + (smooth + craterH * 0.12 - h) * flat * 0.92;
    }

    // player-made craters (explosions)
    for (let i = 0; i < this.edits.length; i++) {
      const e = this.edits[i];
      const dx = x - e.x;
      const dz = z - e.z;
      const d2 = dx * dx + dz * dz;
      const lim = e.r * 2.2;
      if (d2 > lim * lim) continue;
      const r = Math.sqrt(d2) / e.r;
      h += craterProfile(r, e.r, 0) * e.d;
      albedo *= 1 - 0.22 * Math.exp(-r * r * 1.5) + 0.12 * Math.exp(-(r - 1.2) * (r - 1.2) * 3);
    }

    // planetary curvature (paraboloid approximation of the sphere, exact to <1e-5 within ±20 km)
    h -= (x * x + z * z) / (2 * BODY_RADIUS);

    out.height = h;
    out.albedo = clamp(albedo, 0.7, 1.6);
    return out;
  }
}

const _tmp: TerrainSample = { height: 0, albedo: 1 };

/** Crater shape (after Lague): bowl + raised rim + flat floor, degraded with age. */
function craterProfile(r: number, radius: number, age: number): number {
  const floor = -0.42 + 0.18 * age;
  const rimWidth = 0.55 + 0.3 * age;
  const rimSteep = 0.42;
  const k = 0.16 + 0.45 * age;
  const cavity = r * r - 1;
  const rimX = Math.min(r - 1 - rimWidth, 0);
  const rim = rimSteep * rimX * rimX;
  let shape = smin(cavity, rim, k);
  // floor blend radius must stay below |floor| or the smooth-max leaks a constant offset
  // outside the crater (which then shows up as a cliff where the crater stops being evaluated)
  shape = smax(shape, floor, Math.min(k, -floor * 0.9));
  // faint ejecta blanket: continuous at the rim (constant inside) and faded to exactly zero
  // before the evaluation cut-off, so the surface has no steps anywhere
  const blanket = r > 1 ? Math.exp(-(r - 1) * 2.2) * (1 - smoothstep(1.6, 2.2, r)) : 1;
  shape += 0.03 * blanket * (1 - age);
  return shape * radius * 0.85 * (1 - 0.72 * age);
}

// --------------------------------------------------------------------------------------
// Rocks: deterministic scatter so every client (and physics) sees the same boulders.
// --------------------------------------------------------------------------------------

export interface Rock {
  x: number;
  z: number;
  /** Approximate radius (m). */
  size: number;
  /** Yaw rotation. */
  rot: number;
  /** Mesh variant index. */
  variant: number;
  /** Seed for tilt/scale jitter. */
  jitter: number;
}

const ROCK_LEVELS = [
  { cell: 2.2, p: 0.16, sMin: 0.04, sMax: 0.14, salt: 101 },
  { cell: 7, p: 0.3, sMin: 0.1, sMax: 0.35, salt: 102 },
  { cell: 21, p: 0.28, sMin: 0.3, sMax: 0.9, salt: 103 },
  { cell: 70, p: 0.22, sMin: 0.8, sMax: 2.2, salt: 104 },
];
export const ROCK_VARIANTS = 6;

/** Rocks whose centre lies in [x0, x0+size) × [z0, z0+size). */
export function rocksInTile(terrain: LunarTerrain, x0: number, z0: number, size: number, minSize = 0): Rock[] {
  const out: Rock[] = [];
  const rnd = new CellRandom();
  for (const L of ROCK_LEVELS) {
    if (L.sMax < minSize) continue;
    const c0x = Math.floor(x0 / L.cell);
    const c0z = Math.floor(z0 / L.cell);
    const c1x = Math.floor((x0 + size) / L.cell);
    const c1z = Math.floor((z0 + size) / L.cell);
    for (let cz = c0z; cz <= c1z; cz++) {
      for (let cx = c0x; cx <= c1x; cx++) {
        rnd.reset(cx, cz, terrain.seed * 17 + L.salt);
        // more rocks on fresh ground (bright ejecta) — sample albedo at the cell centre
        const px = (cx + rnd.next()) * L.cell;
        const pz = (cz + rnd.next()) * L.cell;
        if (px < x0 || px >= x0 + size || pz < z0 || pz >= z0 + size) continue;
        const a = terrain.sample(px, pz, L.cell * 0.5, _tmp).albedo;
        const p = L.p * (0.55 + Math.max(0, a - 0.95) * 3.2);
        if (rnd.next() > p) continue;
        const u = rnd.next();
        const s = L.sMin + (L.sMax - L.sMin) * u * u * u;
        if (s < minSize) continue;
        const dLanding = Math.hypot(px, pz);
        if (dLanding < 12 && s > 0.2) continue; // keep the spawn walkable
        out.push({ x: px, z: pz, size: s, rot: rnd.next() * Math.PI * 2, variant: Math.floor(rnd.next() * ROCK_VARIANTS), jitter: rnd.next() });
      }
    }
  }
  return out;
}
