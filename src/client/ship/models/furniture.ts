import { PROFILES } from '../../../shared/ship/catalog/furniture';
import type { V2, V3 } from '../../../shared/ship/geom';
import type { ModelBuilder } from './kit';

/**
 * Furniture and hull-structure models by `PropDef.model`, in metres inside the prop's box. Their
 * front (the open side of a bunk, the door of a lavatory, the counter of a galley) faces local +x:
 * turn the prop with its yaw. Fairings extrude the shared profiles, the same ones the physics hull
 * and the light placement use.
 */

/** A shared (z, y) profile scaled to the box. */
const scaled = (p: V2[], h: V3): V2[] => p.map(([z, y]) => [z * h[2], y * h[1]]);

export const FURNITURE_MODELS: Record<string, ModelBuilder> = {
  block(k, h) {
    const [hx, hy, hz] = h;
    k.box('console', hx * 2, hy * 2 - 0.06, hz * 2, 0, 0.03, 0);
    k.box('dark', hx * 2 - 0.04, 0.06, hz * 2 - 0.04, 0, -hy + 0.03, 0);
  },

  rack(k, h) {
    const [hx, hy, hz] = h;
    for (const x of [-hx * 0.92, hx * 0.92]) for (const z of [-hz * 0.92, hz * 0.92]) k.box('dark', 0.05, hy * 2, 0.05, x, 0, z);
    for (const y of [-0.45, 0.15, 0.7]) k.box('dark', hx * 1.9, 0.03, hz * 1.9, 0, y * hy, 0);
    k.box('accent', 0.02, 0.05, hz * 1.9, hx * 0.95, hy * 0.7 + 0.04, 0);
  },

  bunk(k, h, info) {
    const [hx, hy, hz] = h;
    const berths = info.look.berths ?? 2;
    // back panel against the wall (−x), end panels, a berth every level with its mattress and rail
    k.box('console', 0.04, hy * 2, hz * 2, -hx + 0.02, 0, 0);
    for (const s of [-1, 1]) k.box('console', hx * 2, hy * 2, 0.04, 0, 0, s * (hz - 0.02));
    for (let i = 0; i < berths; i++) {
      const y = -hy + 0.3 + i * ((hy * 2 - 0.4) / berths);
      k.box('dark', hx * 2 - 0.04, 0.05, hz * 2 - 0.08, 0.02, y, 0);
      k.box('fabric', hx * 2 - 0.12, 0.1, hz * 2 - 0.14, 0.02, y + 0.075, 0);
      k.box('seat', hx * 0.7, 0.08, 0.35, -hx * 0.2, y + 0.16, -hz + 0.26);
      k.rod('chrome', [hx - 0.03, y + 0.24, -hz * 0.6], [hx - 0.03, y + 0.24, hz * 0.6], 0.012);
      for (const z of [-hz * 0.6, hz * 0.6]) k.rod('chrome', [hx - 0.03, y + 0.04, z], [hx - 0.03, y + 0.24, z], 0.01);
      // reading lamp and a privacy curtain rod
      k.box('accent', 0.04, 0.03, 0.1, -hx + 0.07, y + 0.5, hz * 0.7);
      k.rod('chrome', [hx - 0.01, y + 0.62, -hz + 0.05], [hx - 0.01, y + 0.62, hz - 0.05], 0.008);
    }
    k.box('console', hx * 2, 0.04, hz * 2, 0, hy - 0.02, 0);
  },

  galley(k, h) {
    const [hx, hy, hz] = h;
    const top = hy * 0.82;
    k.box('console', hx * 2, top + hy, hz * 2, 0, (top - hy) / 2, 0);
    k.box('dark', hx * 2 - 0.04, 0.06, hz * 2 - 0.04, -0.02, -hy + 0.03, 0);
    k.box('chrome', hx * 2 + 0.02, 0.03, hz * 2 + 0.02, 0.01, top + 0.015, 0);
    // sink and tap, induction plate, the oven door with its window on the front (+x)
    k.box('dark', hx * 0.9, 0.02, hz * 0.45, 0, top + 0.02, -hz * 0.45);
    k.rod('chrome', [-hx * 0.5, top, -hz * 0.45], [-hx * 0.5, top + 0.22, -hz * 0.45], 0.012);
    k.rod('chrome', [-hx * 0.5, top + 0.22, -hz * 0.45], [-hx * 0.2, top + 0.22, -hz * 0.45], 0.01);
    k.box('dark', hx * 1.1, 0.012, hz * 0.6, 0, top + 0.035, hz * 0.4);
    k.box('dark', 0.02, hy * 0.7, hz * 0.8, hx + 0.005, -hy * 0.1, hz * 0.4);
    k.box('glass', 0.012, hy * 0.35, hz * 0.55, hx + 0.017, -hy * 0.05, hz * 0.4);
    k.box('chrome', 0.03, 0.025, hz * 0.6, hx + 0.03, hy * 0.28, hz * 0.4);
    // cupboard doors
    k.box('trim', 0.012, hy * 0.9, 0.012, hx + 0.006, -hy * 0.05, -hz * 0.05);
    k.box('chrome', 0.025, 0.12, 0.02, hx + 0.02, hy * 0.2, -hz * 0.2);
  },

  lavatory(k, h) {
    const [hx, hy, hz] = h;
    // cubicle shell, a folding door on the front (+x) with a handle and an occupied light
    k.box('paint', hx * 2, hy * 2, 0.04, 0, 0, -hz + 0.02);
    k.box('paint', hx * 2, hy * 2, 0.04, 0, 0, hz - 0.02);
    k.box('paint', 0.04, hy * 2, hz * 2, -hx + 0.02, 0, 0);
    k.box('paint', hx * 2, 0.04, hz * 2, 0, hy - 0.02, 0);
    k.box('console', 0.04, hy * 2 - 0.1, hz * 2 - 0.12, hx - 0.02, -0.05, 0);
    for (const z of [-hz * 0.3, 0, hz * 0.3]) k.box('trim', 0.05, hy * 2 - 0.12, 0.01, hx - 0.01, -0.06, z);
    k.box('chrome', 0.04, 0.2, 0.03, hx + 0.02, 0, hz * 0.55);
    k.box('accent', 0.02, 0.05, 0.12, hx + 0.005, hy - 0.12, 0);
    k.grille('dark', 'x', hz * 0.6, 0.12, 4, hx + 0.005, hy * 0.7, 0);
  },

  locker(k, h, info) {
    const [hx, hy, hz] = h;
    const doors = info.look.doors ?? 2;
    k.box('console', hx * 2, hy * 2, hz * 2, 0, 0, 0);
    for (let i = 0; i < doors; i++) {
      const z = -hz + ((i + 0.5) * 2 * hz) / doors;
      k.box('trim', 0.012, hy * 1.9, (2 * hz) / doors - 0.03, hx + 0.004, 0, z);
      k.grille('dark', 'x', ((2 * hz) / doors) * 0.6, 0.15, 4, hx + 0.012, hy * 0.75, z);
      k.box('chrome', 0.03, 0.14, 0.025, hx + 0.02, 0, z + hz / doors - 0.06);
    }
    k.box('accent', 0.014, 0.05, hz * 1.8, hx + 0.008, -hy + 0.1, 0);
  },

  table(k, h) {
    const [hx, hy, hz] = h;
    k.box('console', hx * 2, 0.04, hz * 2, 0, hy - 0.02, 0);
    k.cyl('chrome', 'y', 0.035, hy * 2 - 0.05, 0, -0.02, 0, 10);
    k.cyl('dark', 'y', Math.min(hx, hz) * 0.5, 0.03, 0, -hy + 0.015, 0, 16);
  },

  handrail(k, h) {
    const [hx, , hz] = h;
    // rod along z, standing off the wall on the +x side
    k.rod('chrome', [-hx * 0.3, 0, -hz], [-hx * 0.3, 0, hz], 0.018);
    for (const z of [-hz * 0.85, hz * 0.85]) {
      k.rod('chrome', [-hx * 0.3, 0, z], [hx, 0, z], 0.012);
      k.cyl('dark', 'x', 0.03, 0.02, hx - 0.01, 0, z, 10);
    }
  },

  rail(k, h) {
    const [hx, hy, hz] = h;
    k.box('dark', hx * 2, hy * 2, hz * 2, 0, 0, 0);
    const n = Math.max(2, Math.floor((hz * 2) / 0.6));
    for (let i = 0; i <= n; i++) k.torus('chrome', 'z', hy * 0.9, hy * 0.25, 0, hy * 1.3, -hz + (i * 2 * hz) / n, 10);
  },

  stick(k, h) {
    const [hx, hy, hz] = h;
    k.box('dark', hx * 1.6, 0.01, hz * 1.6, 0, -hy + 0.005, 0);
    k.rod('chrome', [0, -hy, hz * 0.4], [0, hy * 0.7, -hz * 0.6], 0.012);
    k.box('dark', hx * 1.6, hy * 0.35, hz * 0.8, 0, hy * 0.8, -hz * 0.65);
  },

  step(k, h) {
    const [hx, hy, hz] = h;
    k.box('dark', hx * 2, hy * 2, hz * 2, 0, 0, 0);
    k.box('accent', hx * 2, hy * 2.02, 0.05, 0, 0, hz - 0.025);
  },

  stairs(k, h, info) {
    const [hx, hy, hz] = h;
    const n = info.look.steps ?? 5;
    // treads rising toward local −x (the hull), stringers along the sides, handrails on posts
    for (let i = 0; i < n; i++) {
      const x0 = hx - ((i + 1) * 2 * hx) / n;
      const x1 = hx - (i * 2 * hx) / n;
      const y = -hy + ((i + 1) * 2 * hy) / n;
      k.box('dark', x1 - x0, 0.04, hz * 2 - 0.08, (x0 + x1) / 2, y - 0.02, 0);
      k.box('accent', 0.04, 0.042, hz * 2 - 0.08, x0 + 0.02, y - 0.02, 0);
    }
    for (const s of [-1, 1]) {
      const z = s * (hz - 0.03);
      // stringer from the ground to the deck, handrail on a post at each end
      k.rod('paint', [hx, -hy + 0.05, z], [-hx, hy - 0.05, z], 0.035, 6);
      k.rod('chrome', [hx * 0.85, -hy + 0.95, z], [-hx, hy + 0.9, z], 0.02);
      k.rod('chrome', [hx * 0.85, -hy + 0.1, z], [hx * 0.85, -hy + 0.95, z], 0.018);
      k.rod('chrome', [-hx, hy, z], [-hx, hy + 0.9, z], 0.018);
    }
  },

  chin(k, h) {
    k.extrudeX('paintDark', scaled(PROFILES.chin, h), -h[0], h[0], 0.02);
  },

  fin(k, h) {
    k.extrudeX('paint', scaled(PROFILES.fin, h), -h[0], h[0], 0.01);
  },

  spine(k, h) {
    const [hx, hy, hz] = h;
    k.box('paintDark', hx * 2, hy * 1.6, hz * 2, 0, -hy * 0.2, 0);
    k.box('paint', hx * 1.36, hy * 0.48, hz * 1.93, 0, hy * 0.76, 0);
  },

  fairing(k, h, info) {
    const [hx, hy, hz] = h;
    const t = info.look.taper ?? 0.55;
    // frustum: full section at −z, shrunk by `taper` at +z
    k.extrudeX('paint', [[-hz, -hy], [hz, -hy * t], [hz, hy * t], [-hz, hy]], -hx, hx, 0.01);
  },
};

/**
 * Convex points (prop space, metres) for props that collide with their own outline (`hull`):
 * the wedge of the stairs, the fairings' extruded profiles.
 */
export function furnitureHull(model: string, h: V3, look: Record<string, number>): V3[] | null {
  const across = (p: V2[]): V3[] => p.flatMap(([z, y]) => [[-h[0], y, z] as V3, [h[0], y, z] as V3]);
  switch (model) {
    case 'chin':
      return across(scaled(PROFILES.chin, h));
    case 'fin':
      return across(scaled(PROFILES.fin, h));
    case 'fairing': {
      const t = look.taper ?? 0.55;
      return across([[-h[2], -h[1]], [h[2], -h[1] * t], [h[2], h[1] * t], [-h[2], h[1]]]);
    }
    case 'stairs':
      // wedge: deck-high at −x, on the ground at +x
      return [
        [-h[0], h[1], -h[2]],
        [-h[0], h[1], h[2]],
        [-h[0], -h[1], -h[2]],
        [-h[0], -h[1], h[2]],
        [h[0], -h[1], -h[2]],
        [h[0], -h[1], h[2]],
      ];
    default:
      return null;
  }
}
