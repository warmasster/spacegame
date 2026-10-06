// Walking without physics (docs/MUNDO.md §11): someone on foot (an NPC the server moves) goes
// toward a point along the ground under it — any ground: a body's surface anywhere on it (up is away
// from its centre), a ship's deck (up is its +Y) — accelerating and turning like a person, round the
// obstacles in the way (vertical cylinders: ships, rocks), feet always on the ground.
//
// Pure: the same code can move a body on the server, predict it on a client or run in a test.

export type V3 = [number, number, number];

/** The ground under a walker, whatever it is. */
export interface WalkGround {
  /** Axes of the ground's plane at a point (unit): east, up, south — the frame its yaw is measured in. */
  axes(p: readonly number[], e: V3, u: V3, s: V3): void;
  /** Height of a point over the ground under it (m; negative: under it). */
  height(p: readonly number[]): number;
}

/** Something to walk round: a vertical cylinder of radius `r` through `c`. */
export interface Obstacle {
  c: V3;
  r: number;
}

export interface WalkState {
  /** Feet (in the ground's coordinates). */
  p: V3;
  v: V3;
  /** Heading in the ground's plane (forward = −sin·east − cos·south, as a player's). */
  yaw: number;
}

export const GAIT = {
  walk: 1.3,
  run: 2.8,
  /** m/s² */
  accel: 2.5,
  /** rad/s */
  turn: 3.5,
  /** Close enough (m). */
  arrive: 0.5,
  /** Clearance kept from obstacles (m). */
  clearance: 0.8,
};

const e: V3 = [0, 0, 0];
const u: V3 = [0, 0, 0];
const s: V3 = [0, 0, 0];

/**
 * One step toward `to` (null: stop where you are). Returns the distance still to go along the
 * ground (0 without a target).
 */
export function walk(w: WalkState, to: readonly number[] | null, speed: number, dt: number, g: WalkGround, obstacles: readonly Obstacle[] = []): number {
  g.axes(w.p, e, u, s);
  let dx = 0, dy = 0, dz = 0, dist = 0;
  if (to) {
    // the way along the ground: the offset with its vertical part taken off
    dx = to[0] - w.p[0];
    dy = to[1] - w.p[1];
    dz = to[2] - w.p[2];
    const up = dx * u[0] + dy * u[1] + dz * u[2];
    dx -= up * u[0];
    dy -= up * u[1];
    dz -= up * u[2];
    dist = Math.hypot(dx, dy, dz);
  }
  let wx = 0, wy = 0, wz = 0;
  let turning = false;
  if (to && dist > GAIT.arrive) {
    [dx, dy, dz] = around(w.p, [dx / dist, dy / dist, dz / dist], dist, obstacles);
    // turn toward the way first, and go slowly while the turn is big (a person doesn't walk sideways)
    const want = Math.atan2(-(dx * e[0] + dy * e[1] + dz * e[2]), -(dx * s[0] + dy * s[1] + dz * s[2]));
    turnTo(w, want, dt);
    turning = true;
    const off = Math.abs(Math.atan2(Math.sin(want - w.yaw), Math.cos(want - w.yaw)));
    const c = off < Math.PI / 2 ? Math.cos(off) : 0;
    const sp = Math.min(speed * c * c, dist * 1.5);
    wx = dx * sp;
    wy = dy * sp;
    wz = dz * sp;
  }
  // accelerate like a person, along the ground only
  const vu = w.v[0] * u[0] + w.v[1] * u[1] + w.v[2] * u[2];
  let ax = wx - (w.v[0] - vu * u[0]);
  let ay = wy - (w.v[1] - vu * u[1]);
  let az = wz - (w.v[2] - vu * u[2]);
  const a = Math.hypot(ax, ay, az);
  const most = GAIT.accel * dt;
  if (a > most) {
    ax *= most / a;
    ay *= most / a;
    az *= most / a;
  }
  w.v[0] += ax - vu * u[0];
  w.v[1] += ay - vu * u[1];
  w.v[2] += az - vu * u[2];
  w.p[0] += w.v[0] * dt;
  w.p[1] += w.v[1] * dt;
  w.p[2] += w.v[2] * dt;
  // feet on the ground
  const h = g.height(w.p);
  g.axes(w.p, e, u, s);
  w.p[0] -= u[0] * h;
  w.p[1] -= u[1] * h;
  w.p[2] -= u[2] * h;
  // slowing to a stop: facing where it was going
  if (!turning) {
    const ve = w.v[0] * e[0] + w.v[1] * e[1] + w.v[2] * e[2];
    const vs = w.v[0] * s[0] + w.v[1] * s[1] + w.v[2] * s[2];
    if (ve * ve + vs * vs > 0.04) turnTo(w, Math.atan2(-ve, -vs), dt);
  }
  return to && dist > GAIT.arrive ? dist : 0;
}

/** Turns toward a point without walking. */
export function face(w: WalkState, at: readonly number[], dt: number, g: WalkGround): void {
  g.axes(w.p, e, u, s);
  const d = [at[0] - w.p[0], at[1] - w.p[1], at[2] - w.p[2]];
  const de = d[0] * e[0] + d[1] * e[1] + d[2] * e[2];
  const ds = d[0] * s[0] + d[1] * s[1] + d[2] * s[2];
  if (de * de + ds * ds > 1e-4) turnTo(w, Math.atan2(-de, -ds), dt);
}

function turnTo(w: WalkState, yaw: number, dt: number): void {
  let d = yaw - w.yaw;
  d = Math.atan2(Math.sin(d), Math.cos(d));
  const most = GAIT.turn * dt;
  w.yaw += Math.max(-most, Math.min(most, d));
  w.yaw = Math.atan2(Math.sin(w.yaw), Math.cos(w.yaw));
}

/**
 * The direction to walk in: straight on, or past the nearest obstacle that blocks the way (to the
 * side of it we are already on), aiming at the point beside it.
 */
function around(p: V3, dir: V3, dist: number, obstacles: readonly Obstacle[]): V3 {
  let best: Obstacle | null = null;
  let bestAlong = Infinity;
  for (const o of obstacles) {
    const cx = o.c[0] - p[0];
    const cy = o.c[1] - p[1];
    const cz = o.c[2] - p[2];
    const along = cx * dir[0] + cy * dir[1] + cz * dir[2];
    const reach = o.r + GAIT.clearance;
    if (along < -reach || along > dist + reach) continue;
    // closest approach of the straight way to its axis (along the ground)
    const ox = cx - along * dir[0] - dot3(cx, cy, cz, u) * u[0];
    const oy = cy - along * dir[1] - dot3(cx, cy, cz, u) * u[1];
    const oz = cz - along * dir[2] - dot3(cx, cy, cz, u) * u[2];
    if (Math.hypot(ox, oy, oz) >= reach) continue;
    if (along < bestAlong) {
      bestAlong = along;
      best = o;
    }
  }
  if (!best) return dir;
  // beside it, on our side of the way: the waypoint round its edge
  const cx = best.c[0] - p[0];
  const cy = best.c[1] - p[1];
  const cz = best.c[2] - p[2];
  // side: perpendicular to the way in the ground's plane
  const sx = u[1] * dir[2] - u[2] * dir[1];
  const sy = u[2] * dir[0] - u[0] * dir[2];
  const sz = u[0] * dir[1] - u[1] * dir[0];
  const side = cx * sx + cy * sy + cz * sz > 0 ? -1 : 1;
  const reach = best.r + GAIT.clearance * 1.5;
  const tx = cx + side * sx * reach;
  const ty = cy + side * sy * reach;
  const tz = cz + side * sz * reach;
  const up = dot3(tx, ty, tz, u);
  const hx = tx - up * u[0];
  const hy = ty - up * u[1];
  const hz = tz - up * u[2];
  const l = Math.hypot(hx, hy, hz) || 1;
  return [hx / l, hy / l, hz / l];
}

const dot3 = (x: number, y: number, z: number, a: V3) => x * a[0] + y * a[1] + z * a[2];
