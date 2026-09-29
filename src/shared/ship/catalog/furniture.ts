// Furniture and structure (props): bunks, galley, lavatory, lockers, console supports, and the hull
// fairings (chin, fin, spine, tail cone) that used to be drawn by hand for one ship. The model
// builder fills the prop's box, so a stretched bunk is still a bunk.

import type { PropSpec } from '../def.js';
import { norm, type V2, type V3 } from '../geom.js';
import type { FurnitureDef } from './types.js';

/**
 * Side profiles of the hull fairings, normalised to their box: (z, y) in −1…1, counter-clockwise
 * seen from +x. The client extrudes them across x (models) and the physics takes their convex hull,
 * so lights and decals placed with `profilePoint` sit exactly on the drawn surface.
 */
export const PROFILES: Record<'chin' | 'fin', V2[]> = {
  // under-nose chin: flat top against the deck, raked front face (floodlights), flat bottom
  chin: [[1, 1], [0.613, 1], [-1, 0.289], [-0.484, -0.867], [1, -1]],
  // dorsal fin: long root on the hull, swept leading edge, flat tip
  fin: [[-1, -1], [1, -1], [1, 0.149], [0.66, 1], [0.064, 1]],
};

/**
 * A point on edge `edge` (from vertex edge to edge+1) of a prop's profile, `t` along it, at
 * `x` across, with the outward normal: where to put a floodlight on the chin.
 */
export function profilePoint(p: Pick<PropSpec, 'c' | 'half'>, profile: V2[], edge: number, t: number, x: number): { pos: V3; n: V3 } {
  const a = profile[edge];
  const b = profile[(edge + 1) % profile.length];
  const z = p.c[2] + (a[0] + (b[0] - a[0]) * t) * p.half[2];
  const y = p.c[1] + (a[1] + (b[1] - a[1]) * t) * p.half[1];
  const dz = (b[0] - a[0]) * p.half[2];
  const dy = (b[1] - a[1]) * p.half[1];
  // counter-clockwise in (z, y): the outward normal is the right-hand normal (dy, −dz)
  return { pos: [x, y, z], n: norm([0, -dz, dy]) };
}

export const FURNITURE_LIST: FurnitureDef[] = [
  // cabin
  { id: 'block', model: 'block', name: 'Pedestal', half: [0.3, 0.4, 0.3], mass: 60, collide: 'box' },
  { id: 'rack', model: 'rack', name: 'Estantería', half: [0.25, 0.9, 0.3], mass: 45, collide: 'box' },
  { id: 'bunk', model: 'bunk', name: 'Literas', half: [0.42, 0.95, 1.0], mass: 70, collide: 'box', look: { berths: 2 } },
  { id: 'galley', model: 'galley', name: 'Cocina', half: [0.3, 0.5, 0.5], mass: 90, collide: 'box' },
  { id: 'lavatory', model: 'lavatory', name: 'Aseo', half: [0.45, 1.05, 0.5], mass: 110, collide: 'box' },
  { id: 'locker', model: 'locker', name: 'Taquillas', half: [0.25, 1.0, 0.45], mass: 60, collide: 'box', look: { doors: 2 } },
  { id: 'table', model: 'table', name: 'Mesa plegable', half: [0.3, 0.37, 0.28], mass: 15, collide: 'box' },
  { id: 'handrail', model: 'handrail', name: 'Pasamanos', half: [0.03, 0.05, 0.55], mass: 3, collide: 'none' },
  { id: 'rail', model: 'rail', name: 'Raíl de amarre', half: [0.025, 0.025, 3.7], mass: 20, collide: 'none' },
  { id: 'stick', model: 'stick', name: 'Palanca', half: [0.03, 0.08, 0.07], mass: 1, collide: 'none' },
  // guard rail round a stairwell or along a catwalk: runs along local z, stops you walking off
  { id: 'railing', model: 'railing', name: 'Barandilla', half: [0.03, 0.55, 1.0], mass: 18, collide: 'box' },
  { id: 'step', model: 'step', name: 'Estribo', half: [0.45, 0.04, 0.22], mass: 25, collide: 'box' },
  // boarding stairs rising toward local −x (the hull): collider = the wedge under the treads
  { id: 'stairs', model: 'stairs', name: 'Escalerilla', half: [0.95, 0.6, 0.48], mass: 80, collide: 'hull', look: { steps: 5 } },
  // hull structure (outside)
  { id: 'chin', model: 'chin', name: 'Mentón de proa', half: [1.7, 0.225, 0.31], mass: 180, collide: 'hull' },
  { id: 'fin', model: 'fin', name: 'Aleta dorsal', half: [0.06, 0.435, 1.175], mass: 60, collide: 'hull' },
  { id: 'spine', model: 'spine', name: 'Lomo dorsal', half: [0.22, 0.13, 2.7], mass: 140, collide: 'box' },
  { id: 'fairing', model: 'fairing', name: 'Carenado', half: [1, 0.8, 1.2], mass: 150, collide: 'hull', look: { taper: 0.55 } },
];
