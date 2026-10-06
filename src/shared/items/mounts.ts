// Weapon mounts: a weapon of the weapon catalog bolted to something instead of carried — a
// ship's dorsal turret, a fixed nose gun, a turret on a base's wall, a rover's pintle. A mount
// kind says what it fires (a weapon of shared/items/weapons.ts: its projectile, rate and sounds)
// and how it moves (arcs, slew rate, barrels, magazine); a placement puts one on a host at a point
// of its space with its own "up" and "forward". Nothing here knows what the host is: a ship's
// system module (ship/modules/weapons.ts) is one adapter, a site or a vehicle would be another,
// all with the same aim, slew and muzzle math. See docs/EQUIPO.md.
//
// Angles: yaw about the placement's up (+ turns the muzzle toward its left, like three.js), pitch
// up from its horizon. Head space (barrels, looks): +Z the muzzle, +Y up, +X its left.

import { dot, type V3 } from '../ship/geom.js';
import { WEAPON_DEFS } from './weapons.js';

export interface MountKindDef {
  id: string;
  name: string;
  /** Weapon catalog id it fires (a `mounted` one). */
  weapon: string;
  /** Yaw arc (rad), null: all round. */
  yaw: [number, number] | null;
  /** Pitch arc (rad): [lowest, highest]. */
  pitch: [number, number];
  /** How fast it turns (rad/s, each axis). 0: fixed (it fires along its forward). */
  slew: number;
  /** Head pivot above the placement point (m, along its up). */
  pivot: number;
  /** Muzzles in head space (m): shots take turns. */
  barrels: V3[];
  /** Rounds it holds (0: no magazine, never runs dry). */
  magazine: number;
  /** Rounds fed back per second while it has power (a loader, a belt; 0: none). */
  feed: number;
  /** Most a shot may leave off the head's own axis (rad): the authority's check on the aim. */
  spread: number;
  /** Client look (client/fx/mountLooks.ts). */
  look: string;
}

/** A mount on a host: where (host space), its up and forward (unit), and its kind. */
export interface MountPlacement {
  id: string;
  kind: string;
  at: V3;
  up: V3;
  fwd: V3;
}

/** Where a mount's head points now (host space) and where it is asked to (the gunner's aim). */
export interface MountAim {
  yaw: number;
  pitch: number;
}

export const MOUNT_KINDS: Record<string, MountKindDef> = {};

export const mountKindById = (id: unknown): MountKindDef | undefined => (typeof id === 'string' && Object.hasOwn(MOUNT_KINDS, id) ? MOUNT_KINDS[id] : undefined);

export function defineMountKind(def: MountKindDef): MountKindDef {
  if (MOUNT_KINDS[def.id]) throw new Error(`mount kind "${def.id}" defined twice`);
  const w = WEAPON_DEFS[def.weapon];
  if (!w || w.action.kind !== 'fire') throw new Error(`mount kind "${def.id}": "${def.weapon}" is not a weapon that fires`);
  if (!w.mounted) throw new Error(`mount kind "${def.id}": weapon "${def.weapon}" is hand-held (give it \`mounted: true\`)`);
  if (!def.barrels.length) throw new Error(`mount kind "${def.id}": no barrels`);
  MOUNT_KINDS[def.id] = def;
  return def;
}

// ------------------------------------------------------------------------------------------------
// Math (host space; no allocation: `out` parameters)
// ------------------------------------------------------------------------------------------------

/** The placement's right (unit): forward × up. */
export function mountRight(m: MountPlacement, out: V3): V3 {
  const f = m.fwd, u = m.up;
  const x = f[1] * u[2] - f[2] * u[1];
  const y = f[2] * u[0] - f[0] * u[2];
  const z = f[0] * u[1] - f[1] * u[0];
  const n = Math.hypot(x, y, z) || 1;
  out[0] = x / n;
  out[1] = y / n;
  out[2] = z / n;
  return out;
}

const _r: V3 = [0, 0, 0];
const _d: V3 = [0, 0, 0];
const _u: V3 = [0, 0, 0];

/** Clamp an aim into the kind's arcs (yaw wrapped when it turns all round). */
export function clampAim(kind: MountKindDef, a: MountAim): MountAim {
  if (kind.yaw) a.yaw = Math.max(kind.yaw[0], Math.min(kind.yaw[1], a.yaw));
  else a.yaw = Math.atan2(Math.sin(a.yaw), Math.cos(a.yaw));
  a.pitch = Math.max(kind.pitch[0], Math.min(kind.pitch[1], a.pitch));
  if (kind.slew <= 0) a.yaw = a.pitch = 0;
  return a;
}

/** The aim that points the head along host-space direction `d` (clamped to the arcs), into `out`. */
export function aimAlong(m: MountPlacement, kind: MountKindDef, d: V3, out: MountAim): MountAim {
  mountRight(m, _r);
  const n = Math.hypot(d[0], d[1], d[2]) || 1;
  out.pitch = Math.asin(Math.max(-1, Math.min(1, dot(d, m.up) / n)));
  out.yaw = Math.atan2(-dot(d, _r), dot(d, m.fwd));
  return clampAim(kind, out);
}

/** The pivot of the head (host space), into `out`. */
export function mountPivot(m: MountPlacement, kind: MountKindDef, out: V3): V3 {
  for (let i = 0; i < 3; i++) out[i] = m.at[i] + m.up[i] * kind.pivot;
  return out;
}

/** The aim that points the head at host-space point `p` (clamped), into `out`. */
export function aimAt(m: MountPlacement, kind: MountKindDef, p: V3, out: MountAim): MountAim {
  mountPivot(m, kind, _u);
  _d[0] = p[0] - _u[0];
  _d[1] = p[1] - _u[1];
  _d[2] = p[2] - _u[2];
  return aimAlong(m, kind, _d, out);
}

/** The head's axis at an aim (host space, unit), into `out`. */
export function mountAxis(m: MountPlacement, a: MountAim, out: V3): V3 {
  mountRight(m, _r);
  const cp = Math.cos(a.pitch);
  const sp = Math.sin(a.pitch);
  const cy = Math.cos(a.yaw);
  const sy = Math.sin(a.yaw);
  for (let i = 0; i < 3; i++) out[i] = cp * (cy * m.fwd[i] - sy * _r[i]) + sp * m.up[i];
  return out;
}

/** Barrel `b`'s muzzle at an aim (host space), into `o`; the axis it fires along into `d`. */
export function mountMuzzle(m: MountPlacement, kind: MountKindDef, a: MountAim, b: number, o: V3, d: V3) {
  mountRight(m, _r);
  const cp = Math.cos(a.pitch);
  const sp = Math.sin(a.pitch);
  const cy = Math.cos(a.yaw);
  const sy = Math.sin(a.yaw);
  // head axes: forward d, up u, left l = u × d
  for (let i = 0; i < 3; i++) {
    const h = cy * m.fwd[i] - sy * _r[i];
    d[i] = cp * h + sp * m.up[i];
    _u[i] = -sp * h + cp * m.up[i];
  }
  const lx = _u[1] * d[2] - _u[2] * d[1];
  const ly = _u[2] * d[0] - _u[0] * d[2];
  const lz = _u[0] * d[1] - _u[1] * d[0];
  const bp = kind.barrels[((b % kind.barrels.length) + kind.barrels.length) % kind.barrels.length];
  mountPivot(m, kind, o);
  o[0] += lx * bp[0] + _u[0] * bp[1] + d[0] * bp[2];
  o[1] += ly * bp[0] + _u[1] * bp[1] + d[1] * bp[2];
  o[2] += lz * bp[0] + _u[2] * bp[1] + d[2] * bp[2];
}

/** Turn an aim toward a target at the kind's slew rate (shortest way round when it turns all round). */
export function slewAim(kind: MountKindDef, cur: MountAim, target: MountAim, dt: number): MountAim {
  const step = kind.slew * dt;
  let dy = target.yaw - cur.yaw;
  if (!kind.yaw) dy = Math.atan2(Math.sin(dy), Math.cos(dy));
  cur.yaw += Math.max(-step, Math.min(step, dy));
  cur.pitch += Math.max(-step, Math.min(step, target.pitch - cur.pitch));
  return clampAim(kind, cur);
}

// ------------------------------------------------------------------------------------------------
// Kinds
// ------------------------------------------------------------------------------------------------

// the Kestrel dorsal turret: two mini-missile tubes, all round, a loader feeds it from the hold
defineMountKind({
  id: 'turret.minimissile',
  name: 'Torreta de minimisiles',
  weapon: 'minimissile.tube',
  yaw: null,
  pitch: [-0.12, 1.35],
  slew: 1.6,
  pivot: 0.2,
  barrels: [
    [0.17, 0.02, 0.55],
    [-0.17, 0.02, 0.55],
  ],
  magazine: 12,
  feed: 0.25,
  spread: 0.45,
  look: 'turret.twin',
});
