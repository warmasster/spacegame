// The kinds of terrain modifier. Each one is a pure function: the ground at a point of the
// modifier's frame (height above the body's sphere, albedo, rock density, material) goes in and
// comes out changed. A new kind is an entry in MOD_KINDS; nothing else knows them by name.
//
// Frames: x east, z south, on the tangent plane at the modifier's centre (m). Kinds with a heading
// (`yaw`, from north toward east) run along it: forward = (sin yaw, −cos yaw), right = (cos yaw, sin yaw).

import { craterProfile, smoothstep } from '../../noise.js';
import type { SurfaceSample } from '../surface.js';
import type { ModPoint, TerrainMod } from './types.js';

export interface ModKind {
  /** How far from its centre it changes anything (m). */
  reach(m: TerrainMod): number;
  /** Width of its smallest detail (m): a sampling coarser than this leaves it out (level of detail). */
  feature(m: TerrainMod): number;
  /** Change the ground at a point of its frame, in place. */
  apply(m: TerrainMod, p: ModPoint, s: SurfaceSample): void;
  /** Fill the parameters left as NaN from the ground as it is (`ground(x, z)`: height at a point of its frame). */
  resolve?(m: TerrainMod, ground: (x: number, z: number) => number): void;
}

/** Parameter `i` of a modifier (its default when absent). */
function P(m: TerrainMod, i: number, def: number) {
  const v = m.params?.[i];
  return v === undefined ? def : v;
}

/** Set parameter `i` if it is NaN (or missing). */
function fill(m: TerrainMod, i: number, v: number) {
  const p = (m.params ??= []);
  while (p.length <= i) p.push(NaN);
  if (!Number.isFinite(p[i])) p[i] = v;
}

/**
 * flatten — a level circle blended into the ground round it: a landing field, a pad, the floor of a
 * settlement. params: [height (m over the sphere; NaN: the ground at its centre), inner (share of the
 * radius that is fully flat, 0.6), strength (1: level; less keeps some of the relief), clear (rocks
 * cleared, 0..1), mat (worked ground, 0..1)].
 */
const flatten: ModKind = {
  reach: (m) => m.radius,
  feature: (m) => Math.max(0.5, m.radius * (1 - P(m, 1, 0.6))),
  apply(m, p, s) {
    const w = 1 - smoothstep(m.radius * P(m, 1, 0.6), m.radius, p.d);
    if (w <= 0) return;
    s.height += (P(m, 0, s.height) - s.height) * w * P(m, 2, 1);
    s.rocks *= 1 - w * P(m, 3, 1);
    const mat = w * P(m, 4, 1);
    if (mat > s.mat) s.mat = mat;
  },
  resolve(m, ground) {
    fill(m, 0, ground(0, 0));
  },
};

/**
 * crater — a bowl with a raised rim (the shape of the procedural ones): a blast, or one placed by
 * hand. params: [depth (× the natural depth, 1), age (0 fresh … 1 worn), scorch (darkening at the
 * middle, 0.22), ejecta (bright ring at the rim, 0.12), wobble (irregular rim, 0..1, 1)]. The
 * defaults are a fresh blast's: the ones the game makes by the thousand go on the wire bare.
 */
const crater: ModKind = {
  reach: (m) => m.radius * 2.2 * (1 + 0.2 * P(m, 4, 1)),
  feature: (m) => m.radius,
  apply(m, p, s) {
    let r = p.d / m.radius;
    const wobble = P(m, 4, 1);
    if (wobble > 0 && r < 2.7) {
      // the rim wanders with the azimuth: not a perfect bowl
      const a = Math.atan2(p.z, p.x);
      const k = (m.seed ?? 0) * 0.7548776662;
      r /= 1 + wobble * (0.09 * Math.sin(a * 3 + k) + 0.06 * Math.sin(a * 7 - k * 0.5) + 0.04 * Math.sin(a * 13 + k * 2.3));
    }
    if (r >= 2.2) return;
    const age = P(m, 1, 0);
    s.height += craterProfile(r, m.radius, age) * P(m, 0, 1);
    s.albedo *= 1 - P(m, 2, 0.22) * Math.exp(-r * r * 1.5) + P(m, 3, 0.12) * Math.exp(-(r - 1.2) * (r - 1.2) * 3);
    // the bowl buries the small stones; the ejecta blanket throws new ones round the rim
    const fresh = 1 - age;
    s.rocks *= (1 - fresh * 0.75 * (1 - smoothstep(0.5, 1.05, r))) * (1 + fresh * 1.2 * Math.exp(-(r - 1.3) * (r - 1.3) * 5));
  },
};

/** Along and across the heading of a modifier (forward = (sin yaw, −cos yaw), right = (cos yaw, sin yaw)). */
function along(m: TerrainMod, p: ModPoint) {
  const y = m.yaw ?? 0;
  return p.x * Math.sin(y) - p.z * Math.cos(y);
}
function across(m: TerrainMod, p: ModPoint) {
  const y = m.yaw ?? 0;
  return p.x * Math.cos(y) + p.z * Math.sin(y);
}

/**
 * trench — a straight channel `2·radius` long along its heading, rounded walls. params: [half width
 * (m, 2), depth (m, 1)].
 */
const trench: ModKind = {
  reach: (m) => m.radius + P(m, 0, 2),
  feature: (m) => P(m, 0, 2),
  apply(m, p, s) {
    const w = P(m, 0, 2);
    const k = (1 - smoothstep(w * 0.45, w, Math.abs(across(m, p)))) * (1 - smoothstep(m.radius - w, m.radius, Math.abs(along(m, p))));
    if (k <= 0) return;
    s.height -= P(m, 1, 1) * k;
    s.rocks *= 1 - k;
    if (k * 0.5 > s.mat) s.mat = k * 0.5;
  },
};

/**
 * ramp — a straight even slope `2·radius` long along its heading, from height A (behind) to B (ahead).
 * params: [half width (m, 3), A, B (m over the sphere; NaN: the ground at that end)].
 */
const ramp: ModKind = {
  reach: (m) => Math.hypot(m.radius + P(m, 0, 3), P(m, 0, 3)),
  feature: (m) => Math.max(0.5, P(m, 0, 3) * 0.3),
  apply(m, p, s) {
    const w = P(m, 0, 3);
    const t = along(m, p);
    const k = (1 - smoothstep(w * 0.6, w, Math.abs(across(m, p)))) * (1 - smoothstep(m.radius, m.radius + w * 0.6, Math.abs(t)));
    if (k <= 0) return;
    const f = Math.min(1, Math.max(0, (t + m.radius) / (2 * m.radius)));
    const target = P(m, 1, s.height) + (P(m, 2, s.height) - P(m, 1, s.height)) * f;
    s.height += (target - s.height) * k;
    s.rocks *= 1 - k;
    if (k > s.mat) s.mat = k;
  },
  resolve(m, ground) {
    const y = m.yaw ?? 0;
    const fx = Math.sin(y);
    const fz = -Math.cos(y);
    fill(m, 0, 3);
    fill(m, 1, ground(-fx * m.radius, -fz * m.radius));
    fill(m, 2, ground(fx * m.radius, fz * m.radius));
  },
};

/** Every kind by name. */
export const MOD_KINDS: Record<string, ModKind> = { flatten, crater, trench, ramp };

/** How far from its centre a modifier changes the ground (m). */
export const modReach = (m: TerrainMod) => MOD_KINDS[m.kind]?.reach(m) ?? m.radius;

/** Width of a modifier's smallest detail (m): samplings coarser than ~1.3× this leave it out. */
export const modFeature = (m: TerrainMod) => MOD_KINDS[m.kind]?.feature(m) ?? m.radius;
