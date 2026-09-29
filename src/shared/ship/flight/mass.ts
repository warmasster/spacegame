// Mass, centre of mass and inertia of a ship from its own data: hull plates by area and areal
// density, every machine and prop as a solid box, the consumables inside the tanks (live from the
// state table), and anything else aboard (crew, loose cargo) as extra lumps.

import type { PartDef, ShipDef } from '../def.js';
import type { V3 } from '../geom.js';

export const MASS = {
  /** Areal density of hull plating by kind (kg/m²). */
  plate: { hull: 38, glass: 30, floor: 32, bulkhead: 18 } as Record<string, number>,
  /** Mass of a suited astronaut (kg). */
  crewKg: 130,
};

/** Symmetric inertia tensor [Ixx, Iyy, Izz, Ixy, Ixz, Iyz] (kg·m², ship axes). */
export type Inertia = [number, number, number, number, number, number];

/** Row-major 3×3 matrix. */
export type M3 = [number, number, number, number, number, number, number, number, number];

export interface MassProps {
  mass: number;
  /** Centre of mass (ship space). */
  com: V3;
  /** About the centre of mass. */
  inertia: Inertia;
  /** Inverse of the inertia tensor (ship axes). */
  inv: M3;
  /** kg by group (MASA page, tests). */
  groups: Record<'estructura' | 'máquinas' | 'mobiliario' | 'propelente' | 'gases' | 'víveres' | 'tripulación' | 'carga', number>;
}

/** Something aboard that is not part of the ship data: a crew member, a crate (ship space). */
export interface Lump {
  m: number;
  c: V3;
  half?: V3;
  yaw?: number;
  group?: 'tripulación' | 'carga';
}

/** Live consumables by part (kg), read from a state table. */
export type Consumables = (part: PartDef) => number;

/** Part types whose content (propellant, gas, water, food) weighs. */
const HOLDS = new Set(['tank', 'gas', 'water', 'pantry']);

/** Consumable mass a part holds as defined (full or `fill`). */
export const initialContent: Consumables = (p) => (HOLDS.has(p.type) ? p.p.fill ?? p.p.cap ?? 0 : 0);

type Group = keyof MassProps['groups'];

/**
 * What never changes in a ship's mass, summed once per definition: the plates, the dry machines and
 * the furniture as moments about the ship-space origin (Σm, Σm·c, Σm·c⊗c) plus the boxes' own
 * inertia, and the parts that hold consumables with their box inertia per kilogram. A flight step
 * then only adds what is aboard right now (propellant, crew, crates): O(consumables + extras)
 * instead of re-adding the whole hull 60 times a second.
 */
interface MassBase {
  m: number;
  /** Σ m·c. */
  s1: V3;
  /** Σ m·c⊗c: xx, yy, zz, xy, xz, yz. */
  s2: Inertia;
  /** Σ of the boxes' inertia about their own centres (same layout as Inertia). */
  box: Inertia;
  groups: MassProps['groups'];
  /** Parts that hold something that weighs: their box inertia per kg and their mass group. */
  holds: Array<{ part: PartDef; box: Inertia; group: Group }>;
}

const bases = new WeakMap<ShipDef, MassBase>();

/** Inertia of a solid box of mass `m` about its own centre, turned by `yaw` about y (Inertia layout). */
function boxInertia(m: number, half: V3, yaw: number, out: Inertia, k = 1) {
  const a = 2 * half[0];
  const b = 2 * half[1];
  const c = 2 * half[2];
  const ix = (m * (b * b + c * c)) / 12;
  const iy = (m * (a * a + c * c)) / 12;
  const iz = (m * (a * a + b * b)) / 12;
  const cs = Math.cos(yaw);
  const sn = Math.sin(yaw);
  // yaw only mixes x and z
  out[0] += (ix * cs * cs + iz * sn * sn) * k;
  out[2] += (ix * sn * sn + iz * cs * cs) * k;
  out[1] += iy * k;
  out[4] += (ix - iz) * cs * sn * k;
}

function addPoint(b: { m: number; s1: V3; s2: Inertia }, m: number, c: readonly number[]) {
  b.m += m;
  b.s1[0] += m * c[0];
  b.s1[1] += m * c[1];
  b.s1[2] += m * c[2];
  b.s2[0] += m * c[0] * c[0];
  b.s2[1] += m * c[1] * c[1];
  b.s2[2] += m * c[2] * c[2];
  b.s2[3] += m * c[0] * c[1];
  b.s2[4] += m * c[0] * c[2];
  b.s2[5] += m * c[1] * c[2];
}

function massBase(def: ShipDef): MassBase {
  let b = bases.get(def);
  if (b) return b;
  b = { m: 0, s1: [0, 0, 0], s2: [0, 0, 0, 0, 0, 0], box: [0, 0, 0, 0, 0, 0], groups: { estructura: 0, máquinas: 0, mobiliario: 0, propelente: 0, gases: 0, víveres: 0, tripulación: 0, carga: 0 }, holds: [] };
  for (const pn of def.panels) {
    let a = 0;
    for (let i = 0; i < pn.poly.length; i++) {
      const p = pn.poly[i];
      const q = pn.poly[(i + 1) % pn.poly.length];
      a += p[0] * q[1] - q[0] * p[1];
    }
    const m = (Math.abs(a) / 2) * (MASS.plate[pn.kind] ?? 30);
    addPoint(b, m, pn.c);
    b.groups.estructura += m;
  }
  for (const p of def.parts) {
    addPoint(b, p.mass, p.c);
    boxInertia(p.mass, p.half, p.yaw, b.box);
    b.groups.máquinas += p.mass;
    if (!HOLDS.has(p.type)) continue;
    const unit: Inertia = [0, 0, 0, 0, 0, 0];
    boxInertia(1, p.half, p.yaw, unit);
    b.holds.push({ part: p, box: unit, group: p.type === 'tank' ? 'propelente' : p.type === 'gas' ? 'gases' : 'víveres' });
  }
  for (const p of def.props) {
    addPoint(b, p.mass, p.c);
    boxInertia(p.mass, p.half, p.yaw, b.box);
    b.groups.mobiliario += p.mass;
  }
  bases.set(def, b);
  return b;
}

/** A fresh MassProps to fill (reuse one per ship: `massProperties(…, out)`). */
export function emptyMass(): MassProps {
  return { mass: 0, com: [0, 0, 0], inertia: [0, 0, 0, 0, 0, 0], inv: [1, 0, 0, 0, 1, 0, 0, 0, 1], groups: { estructura: 0, máquinas: 0, mobiliario: 0, propelente: 0, gases: 0, víveres: 0, tripulación: 0, carga: 0 } };
}

// scratch sums (one call at a time: single-threaded)
const _acc = { m: 0, s1: [0, 0, 0] as V3, s2: [0, 0, 0, 0, 0, 0] as Inertia };
const _box: Inertia = [0, 0, 0, 0, 0, 0];

/**
 * Mass, centre of mass and inertia (about the centre of mass) with `content` kg in each part that
 * holds something and `extra` lumps aboard. `out` is filled and returned (a new one if omitted).
 */
export function massProperties(def: ShipDef, content: Consumables = initialContent, extra: readonly Lump[] = [], out: MassProps = emptyMass()): MassProps {
  const b = massBase(def);
  const acc = _acc;
  acc.m = b.m;
  for (let k = 0; k < 3; k++) acc.s1[k] = b.s1[k];
  for (let k = 0; k < 6; k++) {
    acc.s2[k] = b.s2[k];
    _box[k] = b.box[k];
  }
  const g = out.groups;
  g.estructura = b.groups.estructura;
  g.máquinas = b.groups.máquinas;
  g.mobiliario = b.groups.mobiliario;
  g.propelente = g.gases = g.víveres = g.tripulación = g.carga = 0;
  for (const h of b.holds) {
    const kg = content(h.part);
    if (!(kg > 0)) continue;
    addPoint(acc, kg, h.part.c);
    for (let k = 0; k < 6; k++) _box[k] += h.box[k] * kg;
    g[h.group] += kg;
  }
  for (const l of extra) {
    addPoint(acc, l.m, l.c);
    if (l.half) boxInertia(l.m, l.half, l.yaw ?? 0, _box);
    g[l.group ?? 'carga'] += l.m;
  }
  const mass = acc.m;
  const k = 1 / (mass || 1);
  const cx = acc.s1[0] * k;
  const cy = acc.s1[1] * k;
  const cz = acc.s1[2] * k;
  // parallel axis: Σ m·r_a·r_b about the centre of mass = Σ m·c_a·c_b − M·com_a·com_b
  const xx = acc.s2[0] - mass * cx * cx;
  const yy = acc.s2[1] - mass * cy * cy;
  const zz = acc.s2[2] - mass * cz * cz;
  const xy = acc.s2[3] - mass * cx * cy;
  const xz = acc.s2[4] - mass * cx * cz;
  const yz = acc.s2[5] - mass * cy * cz;
  const I = out.inertia;
  I[0] = Math.max(yy + zz + _box[0], 1);
  I[1] = Math.max(xx + zz + _box[1], 1);
  I[2] = Math.max(xx + yy + _box[2], 1);
  I[3] = -xy + _box[3];
  I[4] = -xz + _box[4];
  I[5] = -yz + _box[5];
  out.mass = mass;
  out.com[0] = cx;
  out.com[1] = cy;
  out.com[2] = cz;
  invertSym(I, out.inv);
  return out;
}

/** Inverse of a symmetric inertia tensor into `out` (row-major 3×3). */
function invertSym(I: Inertia, out: M3) {
  const a = I[0];
  const b = I[3];
  const c = I[4];
  const e = I[1];
  const f = I[5];
  const i = I[2];
  // matrix [a b c; b e f; c f i]
  const A = e * i - f * f;
  const B = -(b * i - f * c);
  const C = b * f - e * c;
  const det = a * A + b * B + c * C;
  if (Math.abs(det) < 1e-12) {
    out.fill(0);
    out[0] = 1 / (a || 1);
    out[4] = 1 / (e || 1);
    out[8] = 1 / (i || 1);
    return;
  }
  const k = 1 / det;
  out[0] = A * k;
  out[1] = -(b * i - c * f) * k;
  out[2] = (b * f - c * e) * k;
  out[3] = B * k;
  out[4] = (a * i - c * c) * k;
  out[5] = -(a * f - c * b) * k;
  out[6] = C * k;
  out[7] = -(a * f - b * c) * k;
  out[8] = (a * e - b * b) * k;
}

/** Inertia tensor as a full matrix. */
export function tensor(I: Inertia): M3 {
  return [I[0], I[3], I[4], I[3], I[1], I[5], I[4], I[5], I[2]];
}

export function mulM3(m: M3, v: V3): V3 {
  return [m[0] * v[0] + m[1] * v[1] + m[2] * v[2], m[3] * v[0] + m[4] * v[1] + m[5] * v[2], m[6] * v[0] + m[7] * v[1] + m[8] * v[2]];
}

export function invert3(m: M3): M3 {
  const [a, b, c, d, e, f, g, h, i] = m;
  const A = e * i - f * h;
  const B = -(d * i - f * g);
  const C = d * h - e * g;
  const det = a * A + b * B + c * C;
  if (Math.abs(det) < 1e-12) return [1 / (a || 1), 0, 0, 0, 1 / (e || 1), 0, 0, 0, 1 / (i || 1)];
  const k = 1 / det;
  return [A * k, -(b * i - c * h) * k, (b * f - c * e) * k, B * k, (a * i - c * g) * k, -(a * f - c * d) * k, C * k, -(a * h - b * g) * k, (a * e - b * d) * k];
}
