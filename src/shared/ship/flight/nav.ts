// Navigation points the autopilot can fly to (NAV mode, waypoint selector) and the helm shows on
// its map. They come from the sites (space/sites.ts: the base and its pads, the landmark crater…),
// as directions from their body's centre: they work anywhere round any body. A new place to fly to
// is a `nav` entry of a site.

import { bodyById, frameAt, localFrame, type CelestialBody } from '../../space/body.js';
import { SITES, siteDir } from '../../space/sites.js';
import { offsetDir } from '../../space/tangent.js';
import type { V3 } from '../geom.js';

export interface NavPoint {
  id: string;
  /** Short name on the selector and the map (capitals, ≤ 12 characters). */
  name: string;
  /** Body it is on and its unit direction from the body's centre. */
  body: string;
  dir: V3;
  /** Site it belongs to. */
  site: string;
  /** A pad: somewhere to land (the refuelling point is there). */
  pad?: boolean;
}

export const NAV_POINTS: NavPoint[] = SITES.flatMap((site) => {
  const b = bodyById(site.body);
  const at = siteDir(site, bodyById);
  return (site.nav ?? []).map((n): NavPoint => ({ id: n.id, name: n.name, body: site.body, site: site.id, pad: n.pad, dir: offsetDir(b.pole, at, n.x, n.z, b.radius, [0, 0, 0]) as V3 }));
});

/** A navigation point by id. */
export const navPoint = (id: string) => NAV_POINTS.find((p) => p.id === id);

/**
 * Compass bearing (0 = north, clockwise toward east) and distance over the surface (m, along the
 * great circle at the mean radius) from world point `p` to a point of body `b`.
 */
export function bearingTo(b: CelestialBody, p: readonly number[], wp: { dir: readonly number[] }) {
  const f = frameAt(b, p, _lf);
  const u = f.up;
  const t = wp.dir;
  const c = u[0] * t[0] + u[1] * t[1] + u[2] * t[2];
  // along the horizon toward it
  const hx = t[0] - u[0] * c;
  const hy = t[1] - u[1] * c;
  const hz = t[2] - u[2] * c;
  const s = Math.sqrt(hx * hx + hy * hy + hz * hz);
  const e = hx * f.east[0] + hy * f.east[1] + hz * f.east[2];
  const n = hx * f.north[0] + hy * f.north[1] + hz * f.north[2];
  return { bearing: (Math.atan2(e, n) + Math.PI * 2) % (Math.PI * 2), dist: Math.atan2(s, c) * b.radius };
}
const _lf = localFrame();

/** World point on the ground of a navigation point (`height`: of the ground there, m over the sphere). */
export function navWorld(wp: NavPoint, height: number, out: number[] = [0, 0, 0]): V3 {
  const b = bodyById(wp.body);
  const r = b.radius + height;
  out[0] = b.center[0] + wp.dir[0] * r;
  out[1] = b.center[1] + wp.dir[1] * r;
  out[2] = b.center[2] + wp.dir[2] * r;
  return out as V3;
}
