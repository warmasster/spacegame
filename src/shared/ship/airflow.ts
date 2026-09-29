// Air on the move: where the gas of a holed or open compartment goes, how fast, and what it does to
// whatever is in its way. Pure functions of the replicated state (compartment pressures and
// temperatures, panel integrity, doors, valves), so the authority (the ship's reaction, the damage
// in modules/decomp.ts) and every client (the pull on the crew and the crates, the jets and the fog)
// see the same flow without it ever being sent.
//
//   in the leaking room: a sink — the air converges on the opening, sonic at its lip, and in a
//     narrow room the whole cross-section moves toward it (a corridor empties end to end);
//   past the opening: a jet — into the next room, or a plume into the vacuum that thins as it
//     spreads;
//   on the ship: the reaction of what leaves it, a thrust at the opening (a big breach kicks it).
//
// Panels carry the pressure difference across them; a damaged one holds less and tears (decomp.ts).

import type { PanelDef, PanelKind, ShipDef } from './def.js';
import { dot, norm, scale, sub, type V3 } from './geom.js';
import { orifice, R } from './modules/atmos.js';
import type { ShipSystems } from './systems.js';

/** Tuning of what the air does (1 = plain physics). */
export const AIR = {
  /**
   * Drag on bodies in the flow. Plain physics only yanks what stands right at a breach, for a
   * fraction of a second; this makes the crew feel it across the room, tears them off the deck a
   * few metres from a big hole and throws them at it close up.
   */
  drag: 9,
  /** Most the air can accelerate a body (m/s²): a plume outside a breach would fire people off like shells. */
  maxAccel: 40,
  /**
   * The first moments of an explosive decompression (a panel giving way under pressure): the pull
   * in the room it held is this many times stronger, fading over `burstS` seconds — the slug of air
   * that goes all at once before the flow settles.
   */
  burst: 2.5,
  burstS: 1.2,
  /**
   * Reaction thrust of the jets on the ship. Plain physics shoves a light ship parked on the Moon
   * (low weight, little friction) metres sideways; a jolt reads better than a slide.
   */
  thrust: 0.6,
  /**
   * Pressure difference (kPa) below which the air through an opening counts as still; the rush
   * fades in over the next three times that. Rooms settling through an open door (an airlock
   * cycle) or the last dregs of a breach neither pull nor show.
   */
  still: 1.5,
  /** Molar mass of cabin air (kg/mol). */
  M: 0.029,
  /**
   * Pressure difference a panel holds intact, by kind (kPa). A damaged panel holds
   * rating × (hp / maxHp)²: a hull plate below ~40 % or a window near its breaking point gives way
   * under a 70 kPa cabin.
   */
  rating: { hull: 400, glass: 190, floor: 400, bulkhead: 160 } satisfies Record<PanelKind, number>,
};

/** Drag area per mass (m²/kg): a suited astronaut (~180 kg) standing or crouched. Crates: `boxDrag`. */
export const DRAG = { standing: 0.9 / 180, crouched: 0.5 / 180 };

/** Drag area per mass of a box (mean projected area of a box, Cd ≈ 1). */
export const boxDrag = (half: V3, mass: number) => (2 * (half[0] * half[1] + half[1] * half[2] + half[0] * half[2])) / Math.max(1, mass);

/** Gas going through one opening right now. */
export interface Vent {
  /** Compartment it comes from, and where it goes (-1 = out of the ship). */
  up: number;
  down: number;
  /** Centre of the opening (ship space) and the way the gas goes through it. */
  at: V3;
  dir: V3;
  area: number;
  /** Radius of a round hole of that area (m). */
  r0: number;
  /** Mass flow (kg/s), upstream density (kg/m³), speed at the throat (m/s). */
  mdot: number;
  rho: number;
  speed: number;
  /** Push of the jet on the ship, against `dir` (N). */
  thrust: number;
  /** Panel it goes through (-1: a door, the ramp, a valve). */
  panel: number;
  /** 0..1: how much of a rush it is. Fades in above `AIR.still` so a trickle (two rooms settling through an open door, the dregs of a breach) moves nothing and shows nothing. */
  fade: number;
}

/** What the flow is computed from: ShipSim, or a tick's view of the same state. */
export interface AirState {
  readonly sys: ShipSystems;
  readonly st: Float64Array;
  readonly sw: Record<string, number>;
  hole(i: number): boolean;
  crack(i: number): number;
}


/**
 * Every opening with gas going through it now (the ones with a place: panels, doors, the ramp).
 * `out`: an array to refill instead of a new one (callers that ask every step keep their own).
 */
export function ventsOf(s: AirState, out: Vent[] = []): Vent[] {
  out.length = 0;
  const life = s.sys.life;
  if (!life) return out;
  const V = life.atmos.v;
  for (const g of life.paths(s.st, s.sw, s)) {
    if (!g.at || !g.n) continue;
    const pa = s.st[V[g.a].p] * 1000;
    const pb = g.b >= 0 ? s.st[V[g.b].p] * 1000 : 0;
    const fwd = pa >= pb;
    const up = fwd ? g.a : g.b;
    const Pu = Math.max(pa, pb);
    const Pd = Math.min(pa, pb);
    const still = AIR.still * 1000;
    if (up < 0 || Pu - Pd <= still) continue;
    const Tu = Math.max(150, s.st[V[up].t] + 273.15);
    const o = orifice(g.area, Pu, Tu, AIR.M, Pd);
    if (o.mdot <= 0) continue;
    out.push({
      up,
      down: fwd ? g.b : g.a,
      at: g.at,
      dir: fwd ? g.n : scale(g.n, -1),
      area: g.area,
      r0: Math.sqrt(g.area / Math.PI),
      mdot: o.mdot,
      rho: (Pu * AIR.M) / (R * Tu),
      speed: o.speed,
      thrust: o.thrust * AIR.thrust,
      panel: g.panel ?? -1,
      fade: Math.min(1, (Pu - Pd - still) / (3 * still)) ** 2,
    });
  }
  return out;
}

/** Room boxes per ship (size, gas volume) for the cross-section of the flow. */
const rooms = new WeakMap<ShipDef, Array<{ size: V3; volume: number }>>();

/** Cross-section of a compartment across direction `d` (m²): its gas volume over its length along d. */
function section(def: ShipDef, comp: number, d: V3) {
  let list = rooms.get(def);
  if (!list) {
    list = def.compartments.map((c) => {
      const z = def.zones.find((x) => x.id === c.id);
      return { size: z ? sub(z.max, z.min) : ([2, 2, 2] as V3), volume: c.volume };
    });
    rooms.set(def, list);
  }
  const r = list[comp];
  if (!r) return 4;
  const L = Math.abs(d[0]) * r.size[0] + Math.abs(d[1]) * r.size[1] + Math.abs(d[2]) * r.size[2];
  return Math.max(0.5, r.volume / Math.max(0.3, L));
}

/**
 * What the moving air does to a body at ship-space point `p` in compartment `comp` (-1: outside
 * the hull): its dynamic pressure along the flow (Pa, ship axes). Times a drag area it is a force.
 */
export function airPush(def: ShipDef, vents: readonly Vent[], p: V3, comp: number, out: V3 = [0, 0, 0]): V3 {
  out[0] = out[1] = out[2] = 0;
  const add = (d: V3, q: number) => {
    out[0] += d[0] * q;
    out[1] += d[1] * q;
    out[2] += d[2] * q;
  };
  for (const e of vents) {
    if (comp >= 0 && e.up === comp) {
      // sink: hemispherical near the opening, the whole section of the room further off
      const d = sub(e.at, p);
      const r = Math.max(0.05, Math.hypot(d[0], d[1], d[2]));
      const du: V3 = [d[0] / r, d[1] / r, d[2] / r];
      const Q = e.mdot / e.rho;
      const u = Math.min(e.speed, Q / Math.min(2 * Math.PI * r * r, section(def, comp, du)));
      // toward the opening, and out through it at its lip
      const k = e.r0 / r;
      add(norm([du[0] + e.dir[0] * k, du[1] + e.dir[1] * k, du[2] + e.dir[2] * k]), 0.5 * e.rho * u * u * e.fade);
    } else if (e.down === comp) {
      // jet: into the next room (it slows as it mixes), or a plume into vacuum (it thins as it spreads)
      const rel = sub(p, e.at);
      const s = dot(rel, e.dir);
      if (s <= 0) continue;
      const vac = e.down < 0;
      const b = e.r0 + (vac ? 0.6 : 0.2) * s;
      const lat2 = Math.max(0, dot(rel, rel) - s * s);
      if (lat2 > 9 * b * b) continue;
      const u = e.speed * (vac ? 1 : Math.min(1, (6 * e.r0) / s)) * Math.exp(-lat2 / (b * b));
      const rho = vac ? e.rho * (e.r0 / b) ** 2 : e.rho;
      add(e.dir, 0.5 * rho * u * u * e.fade);
    }
  }
  return out;
}

/** Acceleration (m/s², same axes) of a body with drag area per mass `cda` in the push `q` (Pa). */
export function airAccel(q: V3, cda: number, out: V3 = [0, 0, 0]): V3 {
  const k = AIR.drag * cda;
  out[0] = q[0] * k;
  out[1] = q[1] * k;
  out[2] = q[2] * k;
  const a = Math.hypot(out[0], out[1], out[2]);
  if (a > AIR.maxAccel) {
    const s = AIR.maxAccel / a;
    out[0] *= s;
    out[1] *= s;
    out[2] *= s;
  }
  return out;
}

/** Pressure difference a panel holds at this integrity (kPa). */
export function panelHolds(p: PanelDef, hp: number) {
  return (p.rating ?? AIR.rating[p.kind]) * Math.max(0, hp / p.maxHp) ** 2;
}

/** Pressure difference across a panel (kPa): its room against the other side (another room, or vacuum). */
export function panelLoad(sys: ShipSystems, st: Float64Array, p: PanelDef) {
  return Math.abs(sys.pressure(st, p.zone) - (p.other !== undefined ? sys.pressure(st, p.other) : 0));
}

/** How far a standing panel is past what it holds: 0 = it holds, 1 = twice its strength. */
export function panelStrain(sys: ShipSystems, st: Float64Array, p: PanelDef, hp: number) {
  return strainOf(p, panelLoad(sys, st, p), hp);
}

/** The same, from the pressure difference across it (kPa) when the caller already has it. */
export function strainOf(p: PanelDef, load: number, hp: number) {
  if (load < 1) return 0;
  const holds = panelHolds(p, hp);
  return holds <= 0 ? 2 : Math.max(0, load / holds - 1);
}
