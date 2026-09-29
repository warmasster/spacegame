// The whole surface of a body: height above its mean sphere for any direction from its centre,
// procedural and deterministic (every client, worker and the server get the same ground without
// storing any of it), plus the terrain modifiers laid on it (terrainMods/: the base's flattened
// field and pads, craters, trenches…). One ground per body, the same quality everywhere: there is no
// separate terrain anywhere, a base is just a set of modifiers.
//
// Data per body (SurfaceDef): broad highlands, low dark maria, rolling relief down to about a metre
// and craters at every scale, the big ones flat-floored and shallow as real complex craters are;
// boulders (space/rocks.ts) are scattered from the same data. The world seed varies it all.
//
// Used by the terrain renderer and the physics tiles (in workers), the ships' ground contact
// (flight/model.ts through space/tangent.ts), the server (where craters form) and everything that
// asks how high something is over the ground (space/body.ts `heightAboveGround`).

import { CellRandom3, Noise3, clamp, craterProfile, hash2i, smoothstep } from '../noise.js';
import { CUBE_FACES, cubeArc, facePoint, paramsOn } from './cubeSphere.js';
import { resolveMod, TerrainMods, type ModBody } from './terrainMods/store.js';
import type { TerrainMod } from './terrainMods/types.js';
import type { RockLevelDef } from './rocks.js';

export interface CraterLevelDef {
  /** Cell size (m): about one crater of this size per cell. */
  cell: number;
  /** Chance a cell has one, and how much of that is left inside maria (lava flooded them). */
  p: number;
  mare: number;
  /** Radius range as a fraction of the cell. */
  rMin: number;
  rMax: number;
}

export interface SurfaceDef {
  seed: number;
  /** Height of the reference ground over the mean radius (m); 0 for a body whose relief averages out. */
  datum?: number;
  /** Rolling relief: amplitude (m) at the longest wavelength (m), octaves down from there. */
  relief: { amp: number; wavelength: number; octaves: number; gain: number; lacunarity: number };
  /** Broad highlands and lowlands (m, m). */
  highlands: { amp: number; wavelength: number };
  /** Maria: low flat dark plains where the large-scale noise passes `from`..`to`. */
  maria?: { depth: number; wavelength: number; from: number; to: number; smooth: number };
  craters: CraterLevelDef[];
  /** Craters above this radius (m) stop deepening in proportion (complex craters: flat floors, a few km deep). */
  complexFrom: number;
  /** Albedo multiplier of highlands and maria. */
  albedo: { highland: number; mare: number };
  /** Loose boulders, by size class (space/rocks.ts). None: bare ground. */
  rocks?: RockLevelDef[];
}

export interface SurfaceSample {
  /** Height above the mean sphere (m). */
  height: number;
  /** Albedo multiplier (≈ 0.6 .. 1.5). */
  albedo: number;
  /** Boulder density multiplier (1 natural, 0 cleared: pads, trenches). */
  rocks: number;
  /** Worked ground 0..1 (compacted, levelled: pads, roads): the renderer draws it smoother. */
  mat: number;
}

export const surfaceSample = (): SurfaceSample => ({ height: 0, albedo: 1, rocks: 1, mat: 0 });

const _s = surfaceSample();
const _fp = facePoint();

/** What a surface needs of its body. */
export interface SurfaceBody extends ModBody {
  def: { id: string };
}

export class BodySurface {
  private relief: Noise3;
  private broad: Noise3;
  private mareN: Noise3;
  private rnd = new CellRandom3();
  /** Cells per face side of each crater level (a candidate per cube-sphere cell). */
  private craterCells: number[];
  /** Scratch of a sample: the faces it can see and its parameters on each. */
  private ff = new Int32Array(6);
  private fa = new Float64Array(6);
  private fb = new Float64Array(6);
  /** The seed its noise runs on: the body's own mixed with the world's. */
  readonly seed: number;
  readonly radius: number;
  /** Modifiers laid on it (static: its sites; dynamic: craters of the game). */
  readonly mods: TerrainMods;
  /** Bounds of the height (m): what culling and collision may assume. */
  readonly lowest: number;
  readonly highest: number;
  /**
   * The world's reference level (m, added to every height): the ground at `level` lies on the mean
   * sphere, so a home site sits at its body's radius (the world origin, for the Moon's base) whatever
   * the seed made of the relief there.
   */
  readonly offset: number = 0;

  constructor(
    readonly def: SurfaceDef,
    readonly body: SurfaceBody,
    /** The world's seed (0: the body as it is). */
    readonly worldSeed = 0,
    /** Unit direction whose natural ground is the reference level (none: the mean radius). */
    level?: readonly number[],
  ) {
    this.radius = body.radius;
    this.seed = worldSeed ? hash2i(def.seed, worldSeed, 0x5eed) : def.seed;
    this.relief = new Noise3(this.seed);
    this.broad = new Noise3(this.seed * 7 + 3);
    this.mareN = new Noise3(this.seed * 13 + 5);
    this.mods = new TerrainMods(body);
    // the level whose cells are closest to each crater class's cell (the radius range must stay
    // under 0.45 of a cell: a crater reaches 2.2 radii and a point only looks one cell round it)
    this.craterCells = def.craters.map((c) => {
      if (c.rMax > 0.45) throw new Error(`crater level of ${c.cell} m: rMax ${c.rMax} > 0.45`);
      return 2 ** Math.max(0, Math.round(Math.log2(cubeArc(2, body.radius) / c.cell)));
    });
    if (level) this.offset = -this.natural(level[0], level[1], level[2], 0, _s).height;
    const craterDepth = Math.max(0, ...def.craters.map((c) => this.craterDepth(c.cell * c.rMax)));
    const datum = (def.datum ?? 0) + this.offset;
    this.lowest = datum - (def.highlands.amp * 1.45 + def.relief.amp * 1.7 + (def.maria?.depth ?? 0) + craterDepth * 1.1 + 500);
    this.highest = datum + def.highlands.amp * 1.45 + def.relief.amp * 1.7 + craterDepth * 0.3 + 2500;
  }

  /** Changes whenever the ground does (a modifier added): whoever caches heights compares it. */
  get version() {
    return this.mods.version;
  }

  /** Depth scale of a crater of this radius: simple bowls in proportion, big ones shallow. */
  private craterDepth(radius: number) {
    const k = radius < this.def.complexFrom ? 1 : (this.def.complexFrom / radius) ** 0.85;
    return 0.36 * radius * k;
  }

  /** Height above the mean sphere (m) toward unit direction `d`. `minFeature` (m) skips smaller detail. */
  height(d: readonly number[], minFeature = 0) {
    return this.sample(d[0], d[1], d[2], minFeature, _s).height;
  }

  /** The ground toward unit direction (dx, dy, dz): the procedural relief with every modifier on it. */
  sample(dx: number, dy: number, dz: number, minFeature: number, out: SurfaceSample): SurfaceSample {
    this.natural(dx, dy, dz, minFeature, out);
    if (this.mods.count) this.mods.apply(dx, dy, dz, minFeature, out);
    return out;
  }

  /**
   * Lay a modifier on the ground: its parameters left as NaN are measured from the ground as it is
   * now (the modifiers before it included), then it joins the dynamic layer. Returns the one that
   * holds the change (a crater on top of an older one deepens that one).
   */
  addMod(mod: TerrainMod): TerrainMod {
    resolveMod(mod, this.body, (d) => this.height(d));
    return this.mods.add(mod);
  }

  /** The static layer, laid in order (each one's NaN parameters measured over the ones before it). */
  setStatic(list: readonly TerrainMod[]) {
    this.mods.setStatic([]);
    const done: TerrainMod[] = [];
    for (const m of list) {
      resolveMod(m, this.body, (d) => this.height(d));
      done.push(m);
      this.mods.setStatic(done);
    }
  }

  /** The procedural ground alone, without modifiers. */
  natural(dx: number, dy: number, dz: number, minFeature: number, out: SurfaceSample): SurfaceSample {
    const D = this.def;
    const R = this.radius;
    const px = dx * R;
    const py = dy * R;
    const pz = dz * R;
    // broad highlands / lowlands
    let h = (D.datum ?? 0) + this.offset;
    {
      const f = 1 / D.highlands.wavelength;
      h += D.highlands.amp * (this.broad.noise(px * f, py * f, pz * f) + 0.45 * this.broad.noise(px * f * 2.3 + 11, py * f * 2.3, pz * f * 2.3 - 7));
    }
    // maria: flat dark basins
    let mare = 0;
    if (D.maria) {
      const f = 1 / D.maria.wavelength;
      const m = this.mareN.noise(px * f, py * f, pz * f) + 0.5 * this.mareN.noise(px * f * 2.1 - 5, py * f * 2.1 + 3, pz * f * 2.1);
      mare = smoothstep(D.maria.from, D.maria.to, m);
      h -= D.maria.depth * mare;
    }
    // rolling relief (calmer on the maria)
    {
      const r = D.relief;
      let amp = r.amp * (1 - (D.maria?.smooth ?? 0) * mare);
      let f = 1 / r.wavelength;
      for (let i = 0; i < r.octaves; i++) {
        if (1 / f < minFeature * 2) break;
        h += amp * this.relief.noise(px * f + i * 17.3, py * f - i * 9.1, pz * f + i * 3.7);
        amp *= r.gain;
        f *= r.lacunarity;
      }
    }
    // craters at every scale: one candidate per cube-sphere cell of the level (cubeSphere.ts). A
    // point looks at the 3×3 cells round it on its face, and on a neighbouring face too when it is
    // within a cell of that face's edge: every crater is whole wherever it is (no seams, no cut
    // bowls), and each is sized to its cell so its reach never passes the next one.
    let albedo = D.albedo.highland + (D.albedo.mare - D.albedo.highland) * mare;
    let nf = 0;
    for (let f = 0; f < 6; f++) {
      if (!paramsOn(f, dx, dy, dz, _fp)) continue;
      this.ff[nf] = f;
      this.fa[nf] = _fp.a;
      this.fb[nf] = _fp.b;
      nf++;
    }
    const rnd = this.rnd;
    const seed = this.seed;
    for (let li = 0; li < D.craters.length; li++) {
      const L = D.craters[li];
      if (L.cell * L.rMax * 2 < minFeature * 1.5) break;
      const p = L.p * (1 - (1 - L.mare) * mare);
      const n = this.craterCells[li];
      const cellM = (Math.PI / 2) * R / n;
      const margin = 2 / n;
      for (let k = 0; k < nf; k++) {
        const fa = this.fa[k];
        const fb = this.fb[k];
        if (fa < -1 - margin || fa > 1 + margin || fb < -1 - margin || fb > 1 + margin) continue;
        const face = this.ff[k];
        const F = CUBE_FACES[face];
        const ci = Math.floor((fa + 1) * 0.5 * n);
        const cj = Math.floor((fb + 1) * 0.5 * n);
        for (let j = cj - 1; j <= cj + 1; j++) {
          if (j < 0 || j >= n) continue;
          for (let i = ci - 1; i <= ci + 1; i++) {
            if (i < 0 || i >= n) continue;
            rnd.reset(i, j, face + li * 8, seed * 31);
            if (rnd.next() > p) continue;
            // its centre on the sphere (the face's tangent warp, as cubeDir)
            const A = Math.tan((((i + rnd.next()) / n) * 2 - 1) * (Math.PI / 4));
            const B = Math.tan((((j + rnd.next()) / n) * 2 - 1) * (Math.PI / 4));
            const s2 = 1 + A * A + B * B;
            const k2 = R / Math.sqrt(s2);
            const qx = (F.n[0] + A * F.u[0] + B * F.v[0]) * k2;
            const qy = (F.n[1] + A * F.u[1] + B * F.v[1]) * k2;
            const qz = (F.n[2] + A * F.u[2] + B * F.v[2]) * k2;
            // the cell's narrowest width there (edges and corners squeeze the cells to ~0.7)
            const local = Math.min((1 + A * A) * Math.sqrt(1 + B * B), (1 + B * B) * Math.sqrt(1 + A * A)) / s2;
            const u = rnd.next();
            const radius = (L.rMin + (L.rMax - L.rMin) * u * u) * cellM * local;
            const age = rnd.next();
            const ex = px - qx;
            const ey = py - qy;
            const ez = pz - qz;
            const rr = Math.sqrt(ex * ex + ey * ey + ez * ez) / radius;
            if (rr > 2.2) continue;
            // craterProfile digs 0.36·radius at full depth; big ones are scaled to their real depth
            h += (craterProfile(rr, radius, age) * this.craterDepth(radius)) / (0.36 * radius);
            if (age < 0.15) albedo += 0.3 * (1 - age / 0.15) * Math.exp(-(rr - 1) * (rr - 1) * 1.6) * (1 - smoothstep(1.6, 2.2, rr));
          }
        }
      }
    }
    out.height = h;
    out.albedo = clamp(albedo * (1 + 0.05 * this.relief.noise(px / 3100, py / 3100, pz / 3100) + 0.04 * this.relief.noise(px / 57, py / 57, pz / 57)), 0.5, 1.6);
    out.rocks = 1;
    out.mat = 0;
    return out;
  }
}

/** The Moon's global relief; its base and other sites are modifiers on it (space/sites.ts). */
export const MOON_SURFACE: SurfaceDef = {
  seed: 7331,
  // octaves down to about a metre: the ground under the boots has its own undulation
  relief: { amp: 700, wavelength: 60000, octaves: 15, gain: 0.46, lacunarity: 2.2 },
  highlands: { amp: 1600, wavelength: 900000 },
  maria: { depth: 1300, wavelength: 1_300_000, from: 0.12, to: 0.32, smooth: 0.6 },
  craters: [
    { cell: 240000, p: 0.12, mare: 0.3, rMin: 0.12, rMax: 0.36 },
    { cell: 80000, p: 0.16, mare: 0.3, rMin: 0.12, rMax: 0.38 },
    { cell: 26000, p: 0.2, mare: 0.35, rMin: 0.12, rMax: 0.4 },
    { cell: 9000, p: 0.22, mare: 0.4, rMin: 0.12, rMax: 0.4 },
    { cell: 3000, p: 0.24, mare: 0.5, rMin: 0.12, rMax: 0.4 },
    { cell: 1000, p: 0.26, mare: 0.6, rMin: 0.12, rMax: 0.4 },
    { cell: 340, p: 0.28, mare: 0.7, rMin: 0.12, rMax: 0.4 },
    { cell: 115, p: 0.32, mare: 0.85, rMin: 0.12, rMax: 0.4 },
    // the small ones saturate the ground (the regolith is crater on crater down to the metre)
    { cell: 40, p: 0.4, mare: 0.95, rMin: 0.12, rMax: 0.4 },
    { cell: 14, p: 0.48, mare: 1, rMin: 0.12, rMax: 0.42 },
    { cell: 5, p: 0.5, mare: 1, rMin: 0.12, rMax: 0.4 },
    { cell: 2, p: 0.38, mare: 1, rMin: 0.1, rMax: 0.36 },
  ],
  complexFrom: 7000,
  albedo: { highland: 1.12, mare: 0.68 },
  rocks: [
    { cell: 2.2, p: 0.16, sMin: 0.04, sMax: 0.14 },
    { cell: 7, p: 0.3, sMin: 0.1, sMax: 0.35 },
    { cell: 21, p: 0.28, sMin: 0.3, sMax: 0.9 },
    { cell: 70, p: 0.22, sMin: 0.8, sMax: 2.2 },
  ],
};
