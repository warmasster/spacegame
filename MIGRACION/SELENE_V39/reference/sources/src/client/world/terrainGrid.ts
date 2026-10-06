// The terrain quadtree's shape (world/sphereTerrain.ts, docs/RENDIMIENTO.md): cells per node side,
// and how close (in node widths) a node must be to split. The detail on screen is the angle of a cell
// seen from the camera, about 1 / (TERRAIN_RES · TERRAIN_SPLIT) rad: bigger nodes and an earlier
// stop give the same detail with fewer nodes (fewer draw calls; nodes go as SPLIT²), as long as a
// node's neighbours stay within one level of it (the morph between levels needs it: measured by
// tools/perf, averaged over eight headings).
//
//   32 × 2.2    422 nodes in view, 340 casting   (the first grid)
//   40 × 1.75   244 in view, 137 casting (low)
//   56 × 1.25   167 in view,  68 casting (low)   same cell angle; a third more triangles in view,
//               the same in the shadow pass; the finest cell 0.37 m (under sphereTerrain's 0.4 m)

export const TERRAIN_RES = 56;
export const TERRAIN_SPLIT = 1.25;

/**
 * Nodes this close (m) cast into the sun's shadow cascades, per graphics profile: a little past the
 * cascades' reach (lighting.ts: 220 m high, 90 m low); the baked horizon shadows do the rest.
 */
export const TERRAIN_SHADOW_RANGE = { high: 260, low: 110 } as const;
