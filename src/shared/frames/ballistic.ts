// Anything that flies free through the frames: rockets, bullets, and whatever the projectile
// catalog (shared/items/projectiles.ts) adds. The same code runs on every client (each simulates
// every shot: same start, same arc) and can run on the server (an NPC's or a turret's shot): what
// it flies through comes from a `BallisticEnv` each side provides.
//
// Fired inside a host (a ship's compartment) it flies in the host's space: it leaves exactly from
// where it was launched there, the cabin cannot slide past it however fast the host goes, it feels
// the cabin's apparent gravity and hits the host's own colliders. Out of the compartments (a door,
// a breach) it is handed to the world at a fixed step with the same world position and velocity;
// flying into a host's compartment it is handed to the host the same way. In the world it falls
// with the real radial gravity of the body it is near, and hits hulls and the ground.

import { bodyAt, gravityAt } from '../space/body.js';
import { dirToLocal, dirToWorld, toLocal, toWorld } from '../ship/flight/pose.js';
import type { V3 } from '../ship/geom.js';
import { carry, hostAt, WORLD_FRAME, type FrameHost } from './frame.js';

/** One body in flight. Position, the step before, velocity and nose are in its frame's coordinates. */
export interface Ballistic {
  /** Who launched it (a player id; an NPC or a ship: whatever id the game gives them). */
  owner: number;
  /** Its kind (projectile catalog id). */
  kind: string;
  /** The frame it flies in: a host's id, or WORLD_FRAME (the world itself). */
  fr: number;
  p: V3;
  prev: V3;
  v: V3;
  /** Where its nose points (unit; nothing turns it in flight). */
  nose: V3;
  age: number;
}

/** What it hit, and where. */
export interface Contact {
  /** The point in the world. */
  p: V3;
  /** A host (its id, the point in its space) or the world (the ground; `l` null). */
  fr: number;
  l: V3 | null;
  /** The crew member it hit (their id), if that is what it was. */
  crew?: number;
}

/** What a ballistic body flies through. The client builds it on its physics; the server on its own. */
export interface BallisticEnv {
  host(fr: number): FrameHost | undefined;
  hosts(): Iterable<FrameHost>;
  /** Gravity felt inside a host (its space: the cabin's apparent gravity), written to `out`. */
  hostGravity(host: FrameHost, out: V3): V3;
  /** First solid thing of a host along a → b (its space): the point there, or null. */
  sweepHost(host: FrameHost, a: V3, b: V3): V3 | null;
  /** First hull along a world segment: which host, where in its space and in the world. */
  sweepWorld(a: V3, b: V3): Contact | null;
  /** Height of a world point over the ground under it (≤ 0: in it). */
  groundAlt(pw: V3): number;
  /** Crew it can hit this step: their centres (world). */
  crew: ReadonlyArray<{ id: number; p: V3 }>;
}

/** How a kind flies (from its catalog entry). */
export interface BallisticSpec {
  /** Multiplier of the gravity it feels (1: like anything else). */
  gravity: number;
  /** How close (m) it must pass a crew member's centre to hit them. */
  radius: number;
}

/** Its host is gone from this machine (out of interest, removed): drop it. */
export const LOST = 'lost';

/** Its own launcher can't be hit by it this soon (s): it leaves from inside their reach. */
const OWNER_GRACE = 0.12;
/** Longest sub-step (m of travel in its frame): gravity and the ground test stay fine at any speed. */
const SUBSTEP_M = 6;
const MAX_SUBSTEPS = 8;

/**
 * A body leaving a launcher. `fr`: the launcher's frame (a host's id or WORLD_FRAME); `o`, `d` and
 * `v` in it (`v`: the launcher's own velocity there, carried); `speed` along `d`. `prev`: where the
 * launch point was the step before (so what is drawn between two steps leaves from it).
 */
export function launch(owner: number, kind: string, fr: number, o: V3, d: V3, speed: number, v?: V3, prev?: V3): Ballistic {
  const n = Math.hypot(d[0], d[1], d[2]) || 1;
  const nose: V3 = [d[0] / n, d[1] / n, d[2] / n];
  return {
    owner,
    kind,
    fr,
    p: [o[0], o[1], o[2]],
    prev: prev ? [prev[0], prev[1], prev[2]] : [o[0], o[1], o[2]],
    v: [nose[0] * speed + (v?.[0] ?? 0), nose[1] * speed + (v?.[1] ?? 0), nose[2] * speed + (v?.[2] ?? 0)],
    nose,
    age: 0,
  };
}

const _g: V3 = [0, 0, 0];

/**
 * One fixed step: gravity, travel, what it runs into, then (still flying) the frame it belongs to.
 * Returns the contact (it stopped there: `b.p` is the point, in its frame), LOST, or null.
 */
export function stepBallistic(b: Ballistic, spec: BallisticSpec, dt: number, env: BallisticEnv): Contact | typeof LOST | null {
  const host = b.fr === WORLD_FRAME ? null : env.host(b.fr);
  if (b.fr !== WORLD_FRAME && !host) return LOST;
  b.age += dt;
  const p = b.p;
  const v = b.v;
  b.prev[0] = p[0];
  b.prev[1] = p[1];
  b.prev[2] = p[2];
  // aboard it moves a few m/s against the cabin: usually one sub-step
  const n = Math.min(MAX_SUBSTEPS, Math.max(1, Math.ceil((Math.hypot(v[0], v[1], v[2]) * dt) / SUBSTEP_M)));
  const h = dt / n;
  for (let s = 0; s < n; s++) {
    const g = host ? env.hostGravity(host, _g) : gravityAt(bodyAt(p), p, _g);
    for (let i = 0; i < 3; i++) v[i] += g[i] * spec.gravity * h;
    const from: V3 = [p[0], p[1], p[2]];
    for (let i = 0; i < 3; i++) p[i] += v[i] * h;
    let hit: Contact | null = null;
    if (host) {
      const l = env.sweepHost(host, from, p);
      if (l) hit = { p: toWorld(host.pose, l), fr: host.id, l };
    } else {
      hit = env.sweepWorld(from, p);
      if (!hit && env.groundAlt(p) <= 0) hit = { p: [p[0], p[1], p[2]], fr: WORLD_FRAME, l: null };
    }
    // crew along the segment it swept (world), up to the solid thing it met
    const end = hit ? (host ? hit.l! : hit.p) : p;
    const crew = crewHit(b, spec.radius, host ? toWorld(host.pose, from) : from, host ? toWorld(host.pose, end) : end, env.crew);
    if (crew) {
      const at: V3 = [crew.p[0], crew.p[1], crew.p[2]];
      hit = host ? { p: at, fr: host.id, l: toLocalOf(host, at), crew: crew.id } : { p: at, fr: WORLD_FRAME, l: null, crew: crew.id };
    }
    if (hit) {
      const l = host ? hit.l! : hit.p;
      p[0] = l[0];
      p[1] = l[1];
      p[2] = l[2];
      return hit;
    }
  }
  handOver(b, env, host);
  return null;
}

/** The first crew member within `radius` of segment a → b (world), and the closest point of it. */
function crewHit(b: Ballistic, radius: number, a: V3, e: V3, crew: BallisticEnv['crew']): { id: number; p: V3 } | null {
  let best: { id: number; p: V3 } | null = null;
  let bestT = Infinity;
  const d: V3 = [e[0] - a[0], e[1] - a[1], e[2] - a[2]];
  const dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
  for (const c of crew) {
    if (c.id === b.owner && b.age < OWNER_GRACE) continue;
    const t = dd > 1e-12 ? Math.max(0, Math.min(1, ((c.p[0] - a[0]) * d[0] + (c.p[1] - a[1]) * d[1] + (c.p[2] - a[2]) * d[2]) / dd)) : 0;
    const q: V3 = [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t];
    if (Math.hypot(q[0] - c.p[0], q[1] - c.p[1], q[2] - c.p[2]) < radius && t < bestT) {
      bestT = t;
      best = { id: c.id, p: q };
    }
  }
  return best;
}

const toLocalOf = (host: FrameHost, pw: V3): V3 => toLocal(host.pose, pw);

/**
 * End of a step, still flying: out of its host's compartments it goes to the world; in the world,
 * into a host's compartment it goes to the host. The step before is carried with the poses of the
 * step before, so what is drawn between the two doesn't jump.
 */
function handOver(b: Ballistic, env: BallisticEnv, host: FrameHost | null | undefined) {
  if (host) {
    if (host.inside(b.p)) return;
    move(b, host, null);
    return;
  }
  const into = hostAt(env.hosts(), b.p);
  if (into) move(b, null, into.host);
}

function move(b: Ballistic, from: FrameHost | null, to: FrameHost | null) {
  const now = carry(from?.pose ?? null, to?.pose ?? null, b.p, b.v);
  const before = carry(from?.prev ?? null, to?.prev ?? null, b.prev, [0, 0, 0]);
  let nose = b.nose;
  if (from) nose = dirToWorld(from.pose, nose);
  if (to) nose = dirToLocal(to.pose, nose);
  b.p = now.p;
  b.v = now.v;
  b.prev = before.p;
  b.nose = nose;
  b.fr = to ? to.id : WORLD_FRAME;
}

/** Where a body is in the world this step. */
export function ballisticWorld(b: Ballistic, env: Pick<BallisticEnv, 'host'>): V3 {
  const host = b.fr === WORLD_FRAME ? null : env.host(b.fr);
  return host ? toWorld(host.pose, b.p) : [b.p[0], b.p[1], b.p[2]];
}
