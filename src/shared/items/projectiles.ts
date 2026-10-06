// Projectile catalog: what a weapon fires. Everything a kind needs to fly (shared/frames/
// ballistic.ts), to hurt (the server's impact) and to be drawn and heard (the client looks it up
// by `look` and the sound ids) is here. A new kind is one `defineProjectile` call.

import { BLAST, CREW_BLAST_RADIUS } from '../ship/blast.js';
import { MOD_KINDS } from '../space/terrainMods/index.js';
import type { TerrainImpact } from './impacts.js';

/** What happens where it stops. */
export interface ImpactDef {
  /** Crew within this distance of the point (m, to their centre) are hurt. */
  radius: number;
  /** Suit damage at the point. */
  crew: number;
  /** Damage falls off with distance (a blast) or is the same anywhere within the radius (a hit). */
  falloff: boolean;
  /** Hull and machinery (ship scale: radius m, integrity lost at the point). */
  hull: { radius: number; damage: number };
  /** Optional surface edit at contact, interpreted by the common impact module. */
  terrain?: TerrainImpact;
  /** How it shows: 'blast' (fireball, shock wave, loose objects thrown) or 'hit' (sparks, a clang). */
  fx: 'blast' | 'hit';
}

export interface ProjectileDef {
  id: string;
  name: string;
  /** Speed (m/s) along the aim, relative to the launcher. */
  speed: number;
  /** Seconds before it is gone if it hit nothing. */
  life: number;
  /** Multiplier of the gravity it feels (1: like anything else; a rocket's motor is not modelled). */
  gravity: number;
  /** How close (m) it must pass a crew member's centre to hit them. */
  radius: number;
  impact: ImpactDef;
  /** Client look (fx/projectileLooks.ts). */
  look: string;
  /** Sound bank ids: while it flies, where it stops (a 'blast' impact uses the explosion's). */
  sounds?: { flight?: string; impact?: string };
}

export const PROJECTILES: Record<string, ProjectileDef> = {};

/** A kind by id (anything from the network: only the catalog's own entries). */
export const projectileById = (id: unknown): ProjectileDef | undefined => (typeof id === 'string' && Object.hasOwn(PROJECTILES, id) ? PROJECTILES[id] : undefined);

export function defineProjectile(def: ProjectileDef): ProjectileDef {
  if (PROJECTILES[def.id]) throw new Error(`projectile "${def.id}" defined twice`);
  if (def.impact.terrain && !Object.hasOwn(MOD_KINDS, def.impact.terrain.kind)) throw new Error(`projectile "${def.id}": unknown terrain modifier "${def.impact.terrain.kind}"`);
  PROJECTILES[def.id] = def;
  return def;
}

// a slow unguided rocket: a blast that breaches hulls and digs craters
defineProjectile({
  id: 'rocket',
  name: 'Cohete',
  speed: 42,
  life: 9,
  gravity: 1,
  radius: 0.7,
  impact: { radius: CREW_BLAST_RADIUS, crew: 110, falloff: true, hull: { ...BLAST }, terrain: { kind: 'crater', radius: 2.4, maxHeight: 1.2 }, fx: 'blast' },
  look: 'rocket',
  sounds: { flight: 'rocket.motor' },
});

// a rifle bullet: fast and flat over short ranges, hurts what it hits and dents a panel
defineProjectile({
  id: 'bullet',
  name: 'Bala',
  speed: 380,
  life: 3,
  gravity: 1,
  radius: 0.45,
  impact: { radius: 0.5, crew: 34, falloff: false, hull: { radius: 0.3, damage: 6 }, fx: 'hit' },
  look: 'tracer',
  sounds: { impact: 'bullet.hit' },
});

// A light turret round: localized damage and a shallow crater (separate from the EVA rocket).
defineProjectile({
  id: 'minimissile',
  name: 'Minimisil',
  speed: 140,
  life: 7,
  gravity: 0.25,
  radius: 0.35,
  impact: { radius: 2, crew: 30, falloff: true, hull: { radius: 0.9, damage: 22 }, terrain: { kind: 'crater', radius: 1.1, maxHeight: 0.45 }, fx: 'blast' },
  look: 'minimissile',
  sounds: { flight: 'rocket.motor' },
});
