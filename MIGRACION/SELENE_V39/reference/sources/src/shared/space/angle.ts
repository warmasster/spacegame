/**
 * Angle between nonzero radial directions (radians, 0..π), without allocations.
 * Serialized directions are rounded and need not have length exactly one. atan2(|a×b|, a·b)
 * cancels their lengths and preserves metre-scale separation on planetary radii; acos(a·b)
 * can instead turn a tiny length error into tens of metres or clamp distinct points to zero.
 */
export function angleBetween(ax: number, ay: number, az: number, bx: number, by: number, bz: number): number {
  const x = ay * bz - az * by;
  const y = az * bx - ax * bz;
  const z = ax * by - ay * bx;
  return Math.atan2(Math.sqrt(x * x + y * y + z * z), ax * bx + ay * by + az * bz);
}
