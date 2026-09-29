// Rigid-body step of a ship: the centre of mass moves with the net force, the body turns with
// the torque about the centre of mass (full inertia tensor, gyroscopic term included), and the
// pose (ship-space origin, not the centre of mass) is rebuilt from both. Plain scalar maths: it
// runs up to four times a flight step for every ship, and must not leave garbage behind.

import type { V3 } from '../geom.js';
import type { MassProps } from './mass.js';
import type { ShipPose } from './pose.js';

const _a: V3 = [0, 0, 0];

/** q ⊗ (x, y, z) (or its conjugate with `inv`) into `out`. */
function rot(q: readonly number[], x: number, y: number, z: number, out: V3, inv = false) {
  const s = inv ? -1 : 1;
  const qx = q[0] * s;
  const qy = q[1] * s;
  const qz = q[2] * s;
  const qw = q[3];
  const tx = 2 * (qy * z - qz * y);
  const ty = 2 * (qz * x - qx * z);
  const tz = 2 * (qx * y - qy * x);
  out[0] = x + qw * tx + (qy * tz - qz * ty);
  out[1] = y + qw * ty + (qz * tx - qx * tz);
  out[2] = z + qw * tz + (qx * ty - qy * tx);
  return out;
}

/** Semi-implicit Euler step: `F` world force (N), `T` world torque about the centre of mass (N·m). */
export function integrateBody(pose: ShipPose, mp: MassProps, F: readonly number[], T: readonly number[], dt: number) {
  const q = pose.q;
  const p = pose.p;
  const v = pose.v;
  const w = pose.w;
  // centre of mass: position and velocity
  rot(q, mp.com[0], mp.com[1], mp.com[2], _a);
  const k = dt / mp.mass;
  const vcx = v[0] + (w[1] * _a[2] - w[2] * _a[1]) + F[0] * k;
  const vcy = v[1] + (w[2] * _a[0] - w[0] * _a[2]) + F[1] * k;
  const vcz = v[2] + (w[0] * _a[1] - w[1] * _a[0]) + F[2] * k;
  const pcx = p[0] + _a[0] + vcx * dt;
  const pcy = p[1] + _a[1] + vcy * dt;
  const pcz = p[2] + _a[2] + vcz * dt;
  // angular, in body axes: I·dω/dt = T − ω × Iω
  rot(q, w[0], w[1], w[2], _a, true);
  let bx = _a[0];
  let by = _a[1];
  let bz = _a[2];
  rot(q, T[0], T[1], T[2], _a, true);
  const I = mp.inertia;
  // I·ω (symmetric tensor: xx yy zz xy xz yz)
  const ix = I[0] * bx + I[3] * by + I[4] * bz;
  const iy = I[3] * bx + I[1] * by + I[5] * bz;
  const iz = I[4] * bx + I[5] * by + I[2] * bz;
  const rx = _a[0] - (by * iz - bz * iy);
  const ry = _a[1] - (bz * ix - bx * iz);
  const rz = _a[2] - (bx * iy - by * ix);
  const m = mp.inv;
  bx += (m[0] * rx + m[1] * ry + m[2] * rz) * dt;
  by += (m[3] * rx + m[4] * ry + m[5] * rz) * dt;
  bz += (m[6] * rx + m[7] * ry + m[8] * rz) * dt;
  // spin the orientation at the new world rate (exact for a constant rate over dt)
  rot(q, bx, by, bz, _a);
  const ex = _a[0] * dt;
  const ey = _a[1] * dt;
  const ez = _a[2] * dt;
  const ang = Math.sqrt(ex * ex + ey * ey + ez * ez);
  let dx: number;
  let dy: number;
  let dz: number;
  let dw: number;
  if (ang < 1e-9) {
    const l = Math.sqrt((ex * ex + ey * ey + ez * ez) / 4 + 1);
    dx = ex / 2 / l;
    dy = ey / 2 / l;
    dz = ez / 2 / l;
    dw = 1 / l;
  } else {
    const s = Math.sin(ang / 2) / ang;
    dx = ex * s;
    dy = ey * s;
    dz = ez * s;
    dw = Math.cos(ang / 2);
  }
  // q' = normalize(d · q)
  const qx = dw * q[0] + dx * q[3] + dy * q[2] - dz * q[1];
  const qy = dw * q[1] - dx * q[2] + dy * q[3] + dz * q[0];
  const qz = dw * q[2] + dx * q[1] - dy * q[0] + dz * q[3];
  const qw = dw * q[3] - dx * q[0] - dy * q[1] - dz * q[2];
  const ql = Math.sqrt(qx * qx + qy * qy + qz * qz + qw * qw) || 1;
  q[0] = qx / ql;
  q[1] = qy / ql;
  q[2] = qz / ql;
  q[3] = qw / ql;
  // world rate and the ship-space origin from the new orientation
  rot(q, bx, by, bz, _a);
  w[0] = _a[0];
  w[1] = _a[1];
  w[2] = _a[2];
  rot(q, mp.com[0], mp.com[1], mp.com[2], _a);
  p[0] = pcx - _a[0];
  p[1] = pcy - _a[1];
  p[2] = pcz - _a[2];
  v[0] = vcx - (w[1] * _a[2] - w[2] * _a[1]);
  v[1] = vcy - (w[2] * _a[0] - w[0] * _a[2]);
  v[2] = vcz - (w[0] * _a[1] - w[1] * _a[0]);
}
