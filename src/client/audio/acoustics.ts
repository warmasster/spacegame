// Where sound can go, for the whole game, without knowing what anything is:
//
//   hosts        anything with spaces of air and a structure of its own registers here — a ship
//                today (shipSounds.ts), a base, a rover, a station tomorrow. Same idea as the frame
//                hosts (shared/frames): an id, its own space (y-up), what is inside it. It answers
//                the air in its spaces and how open the way is from the listener to each.
//   environment  the outside comes from the body you are near (its def): the air of its atmosphere
//                (thinner with height; none on the Moon) and the wind that moves it.
//
// `acoustics` answers the medium (medium.ts) from these; `locate` says where any world point is,
// acoustically (inside which host's space, on which host, on the ground).

import { altitudeOf, bodyAt } from '../../shared/space/body';
import { sfx, type Loop } from './engine';
import { MEDIUM, type Acoustics, type Listener, type Place, type V3 } from './medium';

/** Something with air and a structure of its own the sound can be inside or on. */
export interface AcousticHost {
  /** Its frame id (shared/frames), never 0. */
  readonly id: number;
  /** Beyond this (m) from its centre nothing of it matters (the listener closer wakes it). */
  readonly radius: number;
  /** Its centre in the world (into `out`). */
  centre(out: V3): V3;
  /** A world point in its space (into `out`); false when it is more than `margin` m outside it. */
  toLocal(p: V3, out: V3, margin: number): boolean;
  /** A point of its space in the world (into `out`). */
  toWorld(l: V3, out: V3): void;
  /** Which of its air spaces holds a point of its space (-1: none). Its space is y-up. */
  spaceAt(l: V3): number;
  /** Air pressure (kPa) in one of its spaces. */
  air(space: number): number;
  /** How open the air way is (0..1) from where the listener is to `space` (-1: the outside round it). */
  way(space: number): number;
  /** How much of its weight rests on the ground (0 flying … 1 standing there). */
  footing(): number;
  /** Every frame: its own sounds and its ways from the listener; `near`: the listener is close enough to hear it. */
  update(dt: number, near: boolean): void;
}

/** Beyond this (m, past a host's own radius) a host sleeps: no loops, no edges. */
export const WAKE = 450;

const byId = new Map<number, AcousticHost>();
const list: AcousticHost[] = [];

/** The hosts sound can be in. */
export const acousticHosts = {
  add(h: AcousticHost) {
    this.remove(h.id);
    byId.set(h.id, h);
    list.push(h);
  },
  remove(id: number) {
    const h = byId.get(id);
    if (!h) return;
    byId.delete(id);
    list.splice(list.indexOf(h), 1);
  },
  get(id: number) {
    return byId.get(id);
  },
  all(): readonly AcousticHost[] {
    return list;
  },
};

/** Air (kPa) outside at a world point: the atmosphere of the body there, thinner with height. */
export function outsideAir(p: V3): number {
  const b = bodyAt(p);
  const rho = b.def.atmosphereDensity;
  if (rho <= 0) return 0;
  const h = Math.max(0, altitudeOf(b, p));
  return (rho / 1.225) * 101.3 * Math.exp(-h / (b.def.scaleHeight ?? 8000));
}

/** The medium's questions, answered from the hosts and the environment. */
export const acoustics: Acoustics = {
  air(host, space, p) {
    if (host !== 0 && space >= 0) return byId.get(host)?.air(space) ?? 0;
    return outsideAir(p);
  },
  way(host, space) {
    return byId.get(host)?.way(space) ?? 0;
  },
  toWorld(host, local, out) {
    const h = byId.get(host);
    if (!h) return false;
    h.toWorld(local, out);
    return true;
  },
};

/**
 * The air ways of a host: its spaces (and the outside round it, the last node) joined by
 * openings, each as open as `open(e)` says now (0 shut … 1 wide). `from` a node, `solve` gives how
 * open the best way is to every other (the product of the openings along it): a crack lets some
 * sound through, a wide door almost all, a closed one none.
 */
export class AirWays {
  /** How open the way is to each node (spaces, then the outside), after `solve`. */
  readonly to: Float64Array;
  private a: number[] = [];
  private b: number[] = [];
  /** Kind of passage: 0 a door or a hole (sound goes through well), 1 a duct or a valve (it doesn't). */
  private narrow: number[] = [];

  constructor(readonly spaces: number) {
    this.to = new Float64Array(spaces + 1);
  }

  /** A passage between two spaces (-1: the outside); returns its index for `open`. */
  add(a: number, b: number, narrow = false) {
    this.a.push(a < 0 ? this.spaces : a);
    this.b.push(b < 0 ? this.spaces : b);
    this.narrow.push(narrow ? 1 : 0);
    return this.a.length - 1;
  }

  get count() {
    return this.a.length;
  }

  /** From node `from` (-1: the outside; -2: nowhere near: all shut), each passage as open as `open(i)`. */
  solve(from: number, open: (i: number) => number) {
    const to = this.to;
    to.fill(0);
    if (from === -2) return;
    to[from < 0 ? this.spaces : from] = 1;
    for (let pass = 0; pass <= this.spaces; pass++) {
      let changed = false;
      for (let i = 0; i < this.a.length; i++) {
        const o = open(i);
        if (o <= 0.005) continue;
        const t = this.narrow[i] ? o * 0.15 : 0.3 + 0.7 * Math.sqrt(o);
        const a = this.a[i];
        const b = this.b[i];
        if (to[a] * t > to[b] + 1e-6) {
          to[b] = to[a] * t;
          changed = true;
        }
        if (to[b] * t > to[a] + 1e-6) {
          to[a] = to[b] * t;
          changed = true;
        }
      }
      if (!changed) break;
    }
  }

  /** How open the way is to a space (-1: the outside). */
  way(space: number) {
    return this.to[space < 0 ? this.spaces : space] ?? 0;
  }
}

const _c: V3 = [0, 0, 0];
const _l: V3 = [0, 0, 0];

/** Wake the hosts near the listener and let them update; the far ones sleep. */
export function updateHosts(dt: number, L: Listener) {
  for (let k = 0; k < list.length; k++) {
    const h = list[k];
    h.centre(_c);
    const d = Math.hypot(_c[0] - L.p[0], _c[1] - L.p[1], _c[2] - L.p[2]);
    h.update(dt, d < h.radius + WAKE);
  }
}

/**
 * Where a world point is, acoustically (into `out`): in a host's space or on its structure (then
 * it follows the host), on or near the ground. `groundAlt`: height of a point over the ground.
 */
export function locate(p: readonly number[], groundAlt: (p: readonly number[]) => number, out: Place): Place {
  out.p[0] = p[0];
  out.p[1] = p[1];
  out.p[2] = p[2];
  out.host = 0;
  out.space = -1;
  out.own = 0;
  out.structural = false;
  const keep = out.local;
  out.local = null;
  let footing = 0;
  for (let k = 0; k < list.length; k++) {
    const h = list[k];
    if (!h.toLocal(out.p, _l, 1.5)) continue;
    out.host = h.id;
    out.space = h.spaceAt(_l);
    out.local = keep ?? [0, 0, 0];
    out.local[0] = _l[0];
    out.local[1] = _l[1];
    out.local[2] = _l[2];
    footing = h.footing() * (out.space < 0 ? 1 : 0.5);
    break;
  }
  const alt = groundAlt(p);
  out.ground = Math.max(footing, alt < 1.5 ? 1 : alt < 6 ? (6 - alt) / 4.5 : 0);
  return out;
}

/**
 * Where someone's feet are, acoustically (into `out`): on a host (`frame`, their point `local` in
 * its space) or out on the ground (world `p`). Keeps `out.own`.
 */
export function feetAt(out: Place, frame: number, p: V3, local: V3, grounded: boolean): Place {
  out.p[0] = p[0];
  out.p[1] = p[1];
  out.p[2] = p[2];
  out.structural = false;
  const h = frame !== 0 ? byId.get(frame) : undefined;
  if (!h) {
    out.host = 0;
    out.space = -1;
    out.local = null;
    out.ground = grounded ? 1 : 0;
    return out;
  }
  out.host = h.id;
  const l = (out.local ??= [0, 0, 0]);
  l[0] = local[0];
  l[1] = local[1];
  l[2] = local[2];
  // the space a body stands in: a metre above its feet
  _l[0] = local[0];
  _l[1] = local[1] + 1;
  _l[2] = local[2];
  out.space = h.spaceAt(_l);
  out.ground = grounded ? h.footing() * 0.5 : 0;
  return out;
}

/**
 * The outside as the body makes it sound: wind where its atmosphere moves (`BodyDef.wind`). It
 * sounds round the listener, out in the open: inside a closed host the medium keeps it out.
 */
export class Environment {
  private wind: Loop = sfx.loop('env.wind');

  update(L: Listener) {
    const b = bodyAt(L.p);
    const w = b.def.wind ?? 0;
    const air = outsideAir(L.p);
    const l = this.wind;
    l.level = w > 0 && air > MEDIUM.thin ? Math.min(1, w / 15) * Math.min(1, Math.sqrt(air / 101)) : 0;
    l.pitch = 0.7 + Math.min(0.6, w / 30);
    const pl = l.place;
    pl.host = 0;
    pl.space = -1;
    pl.ground = 0;
    pl.local = null;
    // all round you (a little above: it comes from the open, not from one point)
    pl.p[0] = L.p[0] + L.up[0] * 3;
    pl.p[1] = L.p[1] + L.up[1] * 3;
    pl.p[2] = L.p[2] + L.up[2] * 3;
  }
}
