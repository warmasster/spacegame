// Small vector / convex-polygon toolkit for ship definitions. Shared by client and server,
// so it has no three.js dependency (plain [x, y, z] tuples).

export type V3 = [number, number, number];
export type V2 = [number, number];

export const add = (a: V3, b: V3): V3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const scale = (a: V3, k: number): V3 => [a[0] * k, a[1] * k, a[2] * k];
export const madd = (a: V3, b: V3, k: number): V3 => [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k];
export const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const cross = (a: V3, b: V3): V3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
export const len = (a: V3) => Math.hypot(a[0], a[1], a[2]);
export const norm = (a: V3): V3 => scale(a, 1 / (len(a) || 1));
export const lerp3 = (a: V3, b: V3, t: number): V3 => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];

/** Rotation about +Y (yaw), same convention as three.js `rotation.y`. */
export function rotY(p: V3, yaw: number): V3 {
  const c = Math.cos(yaw);
  const s = Math.sin(yaw);
  return [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c];
}

/** Newell normal of a planar polygon (unit). */
export function polyNormal(pts: V3[]): V3 {
  let x = 0;
  let y = 0;
  let z = 0;
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i];
    const b = pts[(i + 1) % pts.length];
    x += (a[1] - b[1]) * (a[2] + b[2]);
    y += (a[2] - b[2]) * (a[0] + b[0]);
    z += (a[0] - b[0]) * (a[1] + b[1]);
  }
  return norm([x, y, z]);
}

export function centroid(pts: V3[]): V3 {
  const c: V3 = [0, 0, 0];
  for (const p of pts) {
    c[0] += p[0];
    c[1] += p[1];
    c[2] += p[2];
  }
  return scale(c, 1 / pts.length);
}

/** Sutherland–Hodgman: keep the part of a convex polygon with dot(p - origin, n) <= 0. */
export function clipPoly(pts: V3[], origin: V3, n: V3): V3[] {
  const out: V3[] = [];
  const side = (p: V3) => dot(sub(p, origin), n);
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i];
    const b = pts[(i + 1) % pts.length];
    const da = side(a);
    const db = side(b);
    if (da <= 0) out.push(a);
    if ((da < 0 && db > 0) || (da > 0 && db < 0)) out.push(lerp3(a, b, da / (da - db)));
  }
  return dedupe(out);
}

function dedupe(pts: V3[]) {
  return pts.filter((p, i) => len(sub(p, pts[(i + 1) % pts.length])) > 1e-4);
}

export function area2(poly: V2[]) {
  let a = 0;
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i];
    const q = poly[(i + 1) % poly.length];
    a += p[0] * q[1] - q[0] * p[1];
  }
  return a / 2;
}

/** Inset a convex CCW polygon by `d` (edge offset + intersection). */
export function insetPoly(poly: V2[], d: number): V2[] {
  const n = poly.length;
  const lines: Array<{ p: V2; dir: V2 }> = [];
  for (let i = 0; i < n; i++) {
    const a = poly[i];
    const b = poly[(i + 1) % n];
    const dx = b[0] - a[0];
    const dy = b[1] - a[1];
    const l = Math.hypot(dx, dy) || 1;
    // inward normal of a CCW polygon is the left normal
    lines.push({ p: [a[0] - (dy / l) * d, a[1] + (dx / l) * d], dir: [dx / l, dy / l] });
  }
  const out: V2[] = [];
  for (let i = 0; i < n; i++) {
    const l0 = lines[(i + n - 1) % n];
    const l1 = lines[i];
    const det = l0.dir[0] * l1.dir[1] - l0.dir[1] * l1.dir[0];
    if (Math.abs(det) < 1e-6) {
      out.push(l1.p);
      continue;
    }
    const t = ((l1.p[0] - l0.p[0]) * l1.dir[1] - (l1.p[1] - l0.p[1]) * l1.dir[0]) / det;
    out.push([l0.p[0] + l0.dir[0] * t, l0.p[1] + l0.dir[1] * t]);
  }
  return out;
}

export function pointInPoly(poly: V2[], x: number, y: number) {
  // convex CCW: left of every edge
  for (let i = 0; i < poly.length; i++) {
    const a = poly[i];
    const b = poly[(i + 1) % poly.length];
    if ((b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]) < -1e-6) return false;
  }
  return true;
}

/** Closest point of a convex polygon to (x, y) (the point itself when inside). */
export function closestInPoly(poly: V2[], x: number, y: number): V2 {
  if (pointInPoly(poly, x, y)) return [x, y];
  let best: V2 = poly[0];
  let bestD = Infinity;
  for (let i = 0; i < poly.length; i++) {
    const a = poly[i];
    const b = poly[(i + 1) % poly.length];
    const ex = b[0] - a[0];
    const ey = b[1] - a[1];
    const t = Math.max(0, Math.min(1, ((x - a[0]) * ex + (y - a[1]) * ey) / (ex * ex + ey * ey || 1)));
    const px = a[0] + ex * t;
    const py = a[1] + ey * t;
    const d = (px - x) ** 2 + (py - y) ** 2;
    if (d < bestD) {
      bestD = d;
      best = [px, py];
    }
  }
  return best;
}

/** An oriented flat part: centre, orthonormal frame (u × v = n) and extents. */
export interface Frame {
  c: V3;
  u: V3;
  v: V3;
  n: V3;
}

export function toFrame(f: Frame, p: V3): V3 {
  const d = sub(p, f.c);
  return [dot(d, f.u), dot(d, f.v), dot(d, f.n)];
}

export function fromFrame(f: Frame, x: number, y: number, z = 0): V3 {
  return [
    f.c[0] + f.u[0] * x + f.v[0] * y + f.n[0] * z,
    f.c[1] + f.u[1] * x + f.v[1] * y + f.n[1] * z,
    f.c[2] + f.u[2] * x + f.v[2] * y + f.n[2] * z,
  ];
}

/**
 * Ray against a flat convex prism (polygon in the frame's u,v plane, ±halfT along n).
 * Returns the distance along the (unit) ray or -1.
 */
export function rayPrism(f: Frame, poly: V2[], halfT: number, o: V3, d: V3, maxT: number): number {
  // slab test on n, then point-in-polygon on the entry point (good enough for thin plates)
  const lo = toFrame(f, o);
  const ld: V3 = [dot(d, f.u), dot(d, f.v), dot(d, f.n)];
  let t0 = 0;
  let t1 = maxT;
  if (Math.abs(ld[2]) < 1e-8) {
    if (Math.abs(lo[2]) > halfT) return -1;
  } else {
    let a = (-halfT - lo[2]) / ld[2];
    let b = (halfT - lo[2]) / ld[2];
    if (a > b) [a, b] = [b, a];
    t0 = Math.max(t0, a);
    t1 = Math.min(t1, b);
    if (t0 > t1) return -1;
  }
  // test a few points through the slab (grazing rays cross the polygon inside the plate)
  for (const k of [0, 0.5, 1]) {
    const t = t0 + (t1 - t0) * k;
    if (pointInPoly(poly, lo[0] + ld[0] * t, lo[1] + ld[1] * t)) return t;
  }
  return -1;
}

/** Ray against an oriented box (half extents along u, v, n). Distance or -1. */
export function rayBox(f: Frame, half: V3, o: V3, d: V3, maxT: number): number {
  const lo = toFrame(f, o);
  const ld: V3 = [dot(d, f.u), dot(d, f.v), dot(d, f.n)];
  let t0 = 0;
  let t1 = maxT;
  for (let i = 0; i < 3; i++) {
    if (Math.abs(ld[i]) < 1e-8) {
      if (Math.abs(lo[i]) > half[i]) return -1;
      continue;
    }
    let a = (-half[i] - lo[i]) / ld[i];
    let b = (half[i] - lo[i]) / ld[i];
    if (a > b) [a, b] = [b, a];
    t0 = Math.max(t0, a);
    t1 = Math.min(t1, b);
    if (t0 > t1) return -1;
  }
  return t0;
}
