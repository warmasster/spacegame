// Sites: the places of a world, as data. A site is not a terrain of its own: it lies on its body's
// one surface and emits terrain modifiers (terrainMods/), plus what the game finds there (pads to
// park ships on, spawn points, supply drops, navigation points, a compass marker). A new kind of
// site is an entry in SITE_KINDS; a procedural generator (settlements, outposts, roads) is a
// function in SITE_GENERATORS that makes sites from the world seed — every machine runs it and gets
// the same ones, so nothing of it goes over the network.
//
// Frames: a site's own frame is the tangent frame at its point (x east, y up, z south: space/tangent.ts).
// The Moon's base sits straight over the body's centre at the world origin, so its frame is the
// world's axes there; nothing depends on that.

import { SHIP_SPAWNS } from '../constants.js';
import type { CrateSpec } from '../protocol.js';
import { offsetDir, type FrameBody } from './tangent.js';
import type { TerrainMod, Vec3 } from './terrainMods/types.js';

/** Where a site is: a unit direction from its body's centre, or a point of another site's frame. */
export type SiteAt = { dir: Vec3 } | { site: string; x: number; z: number };

export interface NavDef {
  id: string;
  /** Short name on the selector and the map (capitals, ≤ 12 characters). */
  name: string;
  /** In the site's frame (m). */
  x: number;
  z: number;
  /** A pad: somewhere to land (the refuelling point is there). */
  pad?: boolean;
}

interface SiteCommon {
  id: string;
  body: string;
  at: SiteAt;
  name: string;
  /** The body's home: where BAJAR flies back to, "BASE" on the displays. */
  home?: boolean;
  /** Marker on the helmet's compass. */
  compass?: { label: string; color: string };
  /** Points the autopilot can fly to. */
  nav?: NavDef[];
}

/**
 * A depot: many objects of one kind kept as a number by the world (docs/MUNDO.md §10), laid out as
 * crates in a grid of `slots` (rows of `cols`, `spacing` m apart, turned `yaw`) when someone comes
 * within `enter` m, and summed back when everyone is beyond `leave`.
 */
export interface DepotDef extends CrateSpec {
  id: string;
  x: number;
  z: number;
  yaw: number;
  count: number;
  slots: number;
  cols: number;
  spacing: number;
  enter: number;
  leave: number;
}

/** What the people of a site do for a living (the order of `CrewDef.mix` and `CrewDef.work`). */
export const JOBS = ['técnico', 'estibador', 'guardia'] as const;

/** A spot of a site's frame (m), facing another if given. */
export interface SpotDef {
  x: number;
  z: number;
  fx?: number;
  fz?: number;
}

/**
 * The people of a site (docs/MUNDO.md §11): `count` of them (by job: `mix`) kept by the world as a
 * number; `slots` of them walk about when someone comes within `enter` m, and go back to being a
 * number when everyone is beyond `leave`. Where each job works, where they rest and stroll.
 */
export interface CrewDef {
  id: string;
  /** Where they gather (site frame, m). */
  x: number;
  z: number;
  count: number;
  slots: number;
  enter: number;
  leave: number;
  mix: number[];
  work: SpotDef[][];
  rest: SpotDef;
  stroll: { x: number; z: number; r: number };
}

/** A landing field: a levelled clearing with pads, where people start and ships park. */
export interface BaseSiteDef extends SiteCommon {
  kind: 'base';
  /** The levelled field (m): radius, share fully flat, how level (1 = flat), worked ground 0..1. */
  field: { r: number; inner: number; strength: number; mat: number };
  /** Pads (site frame, m): level circles free of boulders. `ship`: the ship parked there at start. */
  pads: Array<{ id: string; x: number; z: number; r: number }>;
  /** Where astronauts appear (site frame, m). */
  spawns: Array<[number, number]>;
  /** Crates left on the ground (site frame, m; yaw from the frame's axes). */
  drops: Array<{ x: number; z: number; yaw: number } & CrateSpec>;
  /** Depots (site frame, m). */
  depots?: DepotDef[];
  /** People (site frame, m). */
  crew?: CrewDef[];
}

/** A crater laid by hand (a landmark). */
export interface CraterSiteDef extends SiteCommon {
  kind: 'crater';
  crater: { r: number; depth: number; age: number; ejecta: number; wobble: number };
}

export type SiteDef = BaseSiteDef | CraterSiteDef;

/** What a kind of site does with its data: the modifiers it lays (in order), given its direction. */
export interface SiteKind {
  mods(site: SiteDef, dir: Vec3, body: FrameBody): TerrainMod[];
}

/** Kinds of site by name. */
export const SITE_KINDS: Record<string, SiteKind> = {
  base: {
    mods(site, dir, body) {
      const s = site as BaseSiteDef;
      const f = s.field;
      // the field, levelled to the ground at its middle; each pad levelled over it (NaN: measured when laid)
      const out: TerrainMod[] = [{ kind: 'flatten', body: s.body, center: dir, radius: f.r, params: [NaN, f.inner, f.strength, 0.9, f.mat] }];
      for (const p of s.pads) {
        out.push({ kind: 'flatten', body: s.body, center: offsetDir(body.pole, dir, p.x, p.z, body.radius, [0, 0, 0]) as Vec3, radius: p.r * 1.3, params: [NaN, 0.8 / 1.3, 1, 1, 1] });
      }
      return out;
    },
  },
  crater: {
    mods(site, dir) {
      const c = (site as CraterSiteDef).crater;
      return [{ kind: 'crater', body: site.body, center: dir, radius: c.r, params: [c.depth, c.age, 0, c.ejecta, c.wobble], seed: 7 }];
    },
  },
};

/** Procedural generators of sites: (body id, world seed) → sites. Empty today; settlements go here. */
export const SITE_GENERATORS: Array<(body: string, seed: number) => SiteDef[]> = [];

/** The sites every world has. */
export const SITES: SiteDef[] = [
  {
    id: 'base',
    kind: 'base',
    body: 'moon',
    at: { dir: [0, 1, 0] },
    name: 'BASE',
    home: true,
    compass: { label: 'Base', color: '#9fd3ff' },
    field: { r: 55, inner: 0.45, strength: 0.92, mat: 0.35 },
    // one pad under each ship that starts here
    pads: SHIP_SPAWNS.filter((s) => s.site === 'base').map((s) => ({ id: `pad.${s.id}`, x: s.x, z: s.z, r: s.r ?? 14 })),
    spawns: [
      [0, 0],
      [2.2, 1.4],
      [-2.0, 1.8],
      [0.6, -2.4],
    ],
    drops: [
      { x: 5.2, z: -5.6, yaw: 0.3, half: [0.35, 0.3, 0.35], mass: 40, paint: 'orange' },
      { x: 6.1, z: -4.9, yaw: -0.2, half: [0.3, 0.25, 0.3], mass: 28, paint: 'grey' },
      { x: 5.6, z: -4.3, yaw: 0.9, half: [0.45, 0.3, 0.3], mass: 45, paint: 'orange' },
    ],
    // the base's stores: 40 crates of supplies, 9 out at a time, clear of the pads and the spawn
    depots: [{ id: 'almacen', x: 8, z: 14, yaw: 0.2, count: 40, slots: 9, cols: 3, spacing: 1.3, enter: 300, leave: 450, half: [0.35, 0.3, 0.35], mass: 40, paint: 'grey' }],
    // the base's crew: 11 people, 6 about at a time; their posts clear of the pads
    crew: [
      {
        id: 'dotacion',
        x: -4,
        z: 12,
        count: 11,
        slots: 6,
        enter: 350,
        leave: 500,
        mix: [4, 4, 3],
        work: [
          // technicians: by the ships
          [{ x: 18, z: -24, fx: 3.6, fz: -23.7 }, { x: -10, z: 7, fx: -10, fz: -8 }, { x: 6, z: -9, fx: 30, fz: -8 }],
          // stevedores: round the stores
          [{ x: 6, z: 16.5, fx: 8, fz: 14 }, { x: 10.5, z: 12, fx: 8, fz: 14 }, { x: 9.5, z: 17, fx: 8, fz: 14 }, { x: 5, z: 12.5, fx: 8, fz: 14 }],
          // guards: posts at the field's edge, looking out
          [{ x: 0, z: 42, fx: 0, fz: 80 }, { x: -38, z: 22, fx: -80, fz: 40 }, { x: 36, z: 26, fx: 80, fz: 50 }],
        ],
        rest: { x: -18, z: 22 },
        stroll: { x: 0, z: 12, r: 18 },
      },
    ],
    nav: [{ id: 'base', name: 'BASE', x: 0, z: 0 }, ...SHIP_SPAWNS.filter((s) => s.site === 'base').map((s, i) => ({ id: `pad.${s.id}`, name: `PISTA ${i + 1}`, x: s.x, z: s.z, pad: true }))],
  },
  {
    id: 'landmark',
    kind: 'crater',
    body: 'moon',
    at: { site: 'base', x: 1150, z: -820 },
    name: 'CRÁTER',
    compass: { label: 'Cráter', color: '#e8d49c' },
    crater: { r: 380, depth: 0.9, age: 0.05, ejecta: 0.2, wobble: 0.3 },
    nav: [
      { id: 'crater', name: 'CRÁTER', x: 0, z: 380 * 1.15 },
      { id: 'rim', name: 'BORDE N', x: 0, z: -380 * 1.1 },
    ],
  },
];

/** A site by id. */
export const siteById = (id: string) => SITES.find((s) => s.id === id);

/** Unit direction of a site from its body's centre (`bodyOf`: the body of a site). */
export function siteDir(site: SiteDef, bodyOf: (id: string) => FrameBody, out: number[] = [0, 0, 0]): Vec3 {
  const at = site.at;
  if ('dir' in at) {
    const l = Math.hypot(at.dir[0], at.dir[1], at.dir[2]) || 1;
    out[0] = at.dir[0] / l;
    out[1] = at.dir[1] / l;
    out[2] = at.dir[2] / l;
    return out as Vec3;
  }
  const parent = siteById(at.site);
  if (!parent) throw new Error(`site ${site.id}: unknown site ${at.site}`);
  const b = bodyOf(site.body);
  return offsetDir(b.pole, siteDir(parent, bodyOf), at.x, at.z, b.radius, out) as Vec3;
}

/** Every site of a body in a world (the fixed ones, then the generated ones). */
export function sitesOf(body: string, seed: number): SiteDef[] {
  const out = SITES.filter((s) => s.body === body);
  for (const g of SITE_GENERATORS) for (const s of g(body, seed)) if (s.body === body) out.push(s);
  return out;
}

/** The terrain modifiers of a body's sites, in order (the static layer of its surface). */
export function siteMods(body: string, seed: number, bodyOf: (id: string) => FrameBody): TerrainMod[] {
  const out: TerrainMod[] = [];
  for (const s of sitesOf(body, seed)) {
    const kind = SITE_KINDS[s.kind];
    if (!kind) throw new Error(`site ${s.id}: unknown kind ${s.kind}`);
    out.push(...kind.mods(s, siteDir(s, bodyOf), bodyOf(s.body)));
  }
  return out;
}

/** The home site of a body (null: none). */
export const homeSite = (body: string): SiteDef | null => SITES.find((s) => s.body === body && s.home) ?? null;
