// Weapon mounts aboard a ship (shared/items/mounts.ts has the kinds and the aim math, for any
// host): each mount rides a machine (`ShipMountDef.part`, a turret) whose power switch, circuit
// and integrity decide whether it works. Working, its head slews toward the gunner's aim at the
// kind's rate and its loader feeds the magazine back; wrecked, unpowered or switched off it
// freezes where it is. The authority fires through `tryFire` (rate, rounds, working), and the head
// angles, rounds and readiness are replicated variables: every client draws the head where it is.
//
// The gunner's aim is not state: it comes from whoever sits in a seat that lists the mount
// (`SeatDef.mounts`), through the authority (`aim`).

import { aimAlong, clampAim, mountAxis, mountKindById, mountPivot, slewAim, WEAPON_DEFS, type MountAim, type MountKindDef } from '../../items/index.js';
import type { V3 } from '../geom.js';
import { partKey, seatAt, type PartDef, type ShipDef, type ShipMountDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import type { ShipModule, SoundCue, SystemFactory, Tick } from './api.js';

export interface MountRuntime {
  readonly def: ShipMountDef;
  readonly kind: MountKindDef;
  readonly part: PartDef;
  /** Its power switch (the part's `on` role, by convention the part's id). */
  readonly key: string;
  readonly iYaw: number;
  readonly iPitch: number;
  readonly iAmmo: number;
  /** 1 while it works: switched on, powered, not wrecked. */
  readonly iReady: number;
  /** Where the gunner asks it to point (authority only). */
  readonly target: MountAim;
  /** Authority clock of its last shot (s), and the barrel that fires next. */
  last: number;
  barrel: number;
}

const _cur: MountAim = { yaw: 0, pitch: 0 };

export class WeaponMounts implements ShipModule {
  readonly id = 'mounts';
  readonly list: MountRuntime[];

  constructor(
    private sys: ShipSystems,
    mounts: ShipMountDef[],
  ) {
    const def = sys.def;
    this.list = mounts.map((m) => {
      const kind = mountKindById(m.kind);
      if (!kind) throw new Error(`ship ${def.id}: mount ${m.id} of unknown kind "${m.kind}" (shared/items/mounts.ts)`);
      const part = sys.part(m.part)!;
      return {
        def: m,
        kind,
        part,
        key: partKey(part, 'on', part.id),
        iYaw: sys.vars.define(`${m.id}.yaw`, 0.004),
        iPitch: sys.vars.define(`${m.id}.pitch`, 0.004),
        // Reload is fractional. Quantum 1 would swallow a spent round: before replication the
        // loader has already added a fraction, so the difference from the old count is < 1.
        iAmmo: sys.vars.define(`${m.id}.ammo`, 0.01, kind.magazine),
        iReady: sys.vars.define(`${m.id}.ready`, 1),
        target: { yaw: 0, pitch: 0 },
        last: -Infinity,
        barrel: 0,
      };
    });
    sys.provide('mounts', this);
  }

  /** Index of a mount by id (-1: none). */
  index(id: string) {
    return this.list.findIndex((m) => m.def.id === id);
  }

  /** Its head now (from the replicated angles), into `out`. */
  current(st: Float64Array, i: number, out: MountAim): MountAim {
    const m = this.list[i];
    out.yaw = st[m.iYaw];
    out.pitch = st[m.iPitch];
    return out;
  }

  /** The gunner asks it to point along host-space direction `d` (clamped to its arcs). */
  aimAlong(i: number, d: [number, number, number]) {
    const m = this.list[i];
    if (m) aimAlong(m.def, m.kind, d, m.target);
  }

  /** The gunner asks for these angles (clamped to its arcs). */
  aim(i: number, yaw: number, pitch: number) {
    const m = this.list[i];
    if (!m || !Number.isFinite(yaw) || !Number.isFinite(pitch)) return;
    m.target.yaw = yaw;
    m.target.pitch = pitch;
    clampAim(m.kind, m.target);
  }

  /** It could fire now, going by the replicated state (the client's prediction; the rate is the caller's). */
  canFire(st: Float64Array, i: number) {
    const m = this.list[i];
    return !!m && st[m.iReady] === 1 && (m.kind.magazine === 0 || st[m.iAmmo] >= 1);
  }

  /** Shared service diagnostics, derived from this mount's data instead of a ship-specific switch. */
  unavailable(st: Float64Array, sw: Record<string, number>, i: number): string | null {
    const m = this.list[i];
    if (!m) return 'Montaje desconocido';
    if (sw[m.key] !== 1) return 'APAGADA · activa su alimentación';
    if (this.sys.health(st, m.part) <= 0) return 'MÁQUINA DESTRUIDA · necesita reparación';
    if (this.sys.supply(st, m.part.circuit) < 0.5) return 'SIN ENERGÍA · revisa disyuntor y alimentación';
    if (st[m.iReady] !== 1) return 'INICIANDO';
    if (m.kind.magazine > 0 && st[m.iAmmo] < 1) return 'SIN MUNICIÓN · cargador reponiendo';
    return null;
  }

  /**
   * The authority fires it at clock `now` (s): working, a round in it and past the weapon's rate.
   * Spends the round and returns the barrel it leaves from, or -1 (refused).
   */
  tryFire(st: Float64Array, i: number, now: number): number {
    const m = this.list[i];
    if (!m || !this.canFire(st, i)) return -1;
    // a little slack on the rate (the network bunches messages)
    if (now - m.last < WEAPON_DEFS[m.kind.weapon].cooldown * 0.75) return -1;
    m.last = now;
    if (m.kind.magazine > 0) st[m.iAmmo] = Math.max(0, st[m.iAmmo] - 1);
    const b = m.barrel;
    m.barrel = (b + 1) % m.kind.barrels.length;
    return b;
  }

  step(t: Tick) {
    const st = t.st;
    for (let i = 0; i < this.list.length; i++) {
      const m = this.list[i];
      const ready = t.sw[m.key] === 1 && this.sys.health(st, m.part) > 0 && this.sys.supply(st, m.part.circuit) >= 0.5;
      st[m.iReady] = ready ? 1 : 0;
      if (!ready) continue;
      this.current(st, i, _cur);
      slewAim(m.kind, _cur, m.target, t.dt);
      st[m.iYaw] = _cur.yaw;
      st[m.iPitch] = _cur.pitch;
      if (m.kind.magazine > 0 && st[m.iAmmo] < m.kind.magazine) st[m.iAmmo] = Math.min(m.kind.magazine, st[m.iAmmo] + m.kind.feed * t.dt);
    }
  }

  /** The traverse motors: they whine while the head turns (replacing the part's generic hum). */
  sounds(): SoundCue[] {
    return this.list.map(
      (m): SoundCue => ({
        sound: 'mach.motor',
        role: 'run',
        part: m.part,
        motion: { value: (st) => st[m.iYaw] * 1.7 + st[m.iPitch], rate: m.kind.slew * 1.2 },
      }),
    );
  }
}

/** Someone seated with their feet at `feet` (ship space) works mount `i` of the ship (its seat lists it). */
export function works(def: ShipDef, feet: readonly number[], i: number): boolean {
  const m = def.mounts[i];
  return !!m && !!seatAt(def, feet)?.mounts?.includes(m.id);
}

/** The mounts a seat works (indices in `def.mounts`). */
export function seatMounts(def: ShipDef, seat: number): number[] {
  const ids = def.seats[seat]?.mounts ?? [];
  const out: number[] = [];
  def.mounts.forEach((m, i) => {
    if (ids.includes(m.id)) out.push(i);
  });
  return out;
}

const _ax: V3 = [0, 0, 0];
const _pv: V3 = [0, 0, 0];

/**
 * A shot from mount `i` leaving at `o` along `d` (ship space) fits where its head is now: near the
 * pivot and within the kind's spread of its axis (the gunner's own head leads the replicated one).
 */
export function shotFits(mounts: WeaponMounts, st: Float64Array, i: number, o: V3, d: V3): boolean {
  const m = mounts.list[i];
  if (!m) return false;
  mountAxis(m.def, mounts.current(st, i, _cur), _ax);
  mountPivot(m.def, m.kind, _pv);
  const n = Math.hypot(d[0], d[1], d[2]) || 1;
  const cos = (d[0] * _ax[0] + d[1] * _ax[1] + d[2] * _ax[2]) / n;
  const far = Math.hypot(o[0] - _pv[0], o[1] - _pv[1], o[2] - _pv[2]);
  return cos >= Math.cos(m.kind.spread) && far < 3;
}

export const weaponsSystem: SystemFactory = {
  id: 'weapons',
  parts: ['turret'],
  make: (sys) => (sys.def.mounts.length ? [new WeaponMounts(sys, sys.def.mounts)] : []),
};
