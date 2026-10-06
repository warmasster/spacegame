// Blast scales, shared by the ship rules (sim.ts) and the projectile catalog: a leaf with no
// imports, so the catalogs never have to load the ship simulation.

export const BLAST = { radius: 2.8, damage: 75 };
/** Crew-scale radius of a rocket blast (m): other explosions scale the ship blast by radius / this. */
export const CREW_BLAST_RADIUS = 6;

/** Ship-scale blast of an explosion given in crew scale (rocket = 6 m, 75 dmg). Same on server and offline. */
export function shipBlast(radius = CREW_BLAST_RADIUS, damage = BLAST.damage) {
  return { radius: BLAST.radius * Math.max(1, radius / CREW_BLAST_RADIUS), damage };
}
