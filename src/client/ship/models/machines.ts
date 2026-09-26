import * as THREE from 'three';
import type { V3 } from '../../../shared/ship/geom';
import type { Kit, ModelBuilder } from './kit';

/**
 * Machine models by `PartDef.model`, built in metres from the part's box (half extents `h`):
 * every size of a catalog component gets the same design at its own proportions, with bolts,
 * flanges and pipes of real size. Details sit on all four sides (the machine may face any way).
 */

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/** Cabinet: body, dark kick plinth, trim edges on top, a grille on each long face, status strip. */
function cabinet(k: Kit, h: V3, top = 0.9) {
  const [hx, hy, hz] = h;
  k.box('dark', hx * 2, hy * 0.12, hz * 2, 0, -hy + hy * 0.06);
  k.box('body', hx * 1.96, hy * 2 * top - hy * 0.12, hz * 1.96, 0, -hy + hy * 0.12 + (hy * 2 * top - hy * 0.12) / 2);
  const yTop = -hy + hy * 2 * top;
  k.box('trim', hx * 2, 0.02, hz * 2, 0, yTop);
  // grilles on the long faces, a maker band on the short ones
  const long: 'x' | 'z' = hx >= hz ? 'z' : 'x';
  for (const s of [-1, 1]) {
    if (long === 'z') k.grille('dark', 'z', hx * 1.3, hy * 0.5, 6, 0, -hy * 0.1, s * hz * 0.99);
    else k.grille('dark', 'x', hz * 1.3, hy * 0.5, 6, s * hx * 0.99, -hy * 0.1, 0);
  }
  if (long === 'z') for (const s of [-1, 1]) k.box('accent', 0.012, hy * 0.08, hz * 1.4, s * hx * 0.995, yTop - hy * 0.15, 0);
  else for (const s of [-1, 1]) k.box('accent', hx * 1.4, hy * 0.08, 0.012, 0, yTop - hy * 0.15, s * hz * 0.995);
  return yTop;
}

export const MACHINES: Record<string, ModelBuilder> = {
  reactor(k, h, info) {
    const [hx, H, hz] = h;
    const r = Math.min(hx, hz);
    const b = clamp(r * 0.03, 0.008, 0.02);
    k.box('dark', hx * 2, H * 0.12, hz * 2, 0, -H + H * 0.06);
    k.cyl('trim', 'y', r * 0.96, H * 0.06, 0, -H + H * 0.15, 0, 32);
    k.bolts('chrome', 'y', r * 0.88, 16, 0, -H + H * 0.185, 0, b);
    const y0 = -H + H * 0.18;
    const y1 = H * 0.62;
    k.cyl('body', 'y', r * 0.8, y1 - y0, 0, (y0 + y1) / 2, 0, 32);
    k.sphere('body', r * 0.8, 0, y1, 0, [1, 0.45, 1], 28, true);
    const rings = info.look.rings ?? 2;
    for (let i = 1; i <= rings; i++) k.torus('trim', 'y', r * 0.81, r * 0.025, 0, y0 + ((y1 - y0) * i) / (rings + 1), 0, 32);
    k.cyl('accent', 'y', r * 0.812, H * 0.05, 0, y1 - H * 0.06, 0, 32);
    // control-rod drives through the head
    for (let i = 0; i < 4; i++) {
      const a = (i / 4) * Math.PI * 2 + Math.PI / 4;
      k.cyl('chrome', 'y', r * 0.07, H * 0.14, Math.cos(a) * r * 0.32, y1 + r * 0.2, Math.sin(a) * r * 0.32, 10);
    }
    // primary loop nozzles and the pipes riser around the vessel
    for (const s of [-1, 1]) {
      k.cyl('pipe', 'x', r * 0.12, hx * 0.35, s * (r * 0.8 + hx * 0.12), -H * 0.2, 0, 12);
      k.cyl('trim', 'x', r * 0.17, 0.03, s * (r * 0.8 + 0.03), -H * 0.2, 0, 14);
    }
    for (const a of [Math.PI / 2, (7 * Math.PI) / 6, (11 * Math.PI) / 6]) k.rod('pipe', [Math.cos(a) * r * 0.9, -H * 0.75, Math.sin(a) * r * 0.9], [Math.cos(a) * r * 0.9, y1 - H * 0.12, Math.sin(a) * r * 0.9], r * 0.045);
    // warning placard on both faces along z
    for (const s of [-1, 1]) k.box('accent', r * 0.5, r * 0.5, 0.01, 0, -H * 0.35, s * (r * 0.8 + 0.004));
  },

  coolpump(k, h) {
    const [hx, hy, hz] = h;
    k.box('dark', hx * 2, hy * 0.18, hz * 2, 0, -hy + hy * 0.09);
    const r = Math.min(hx, hy) * 0.62;
    // motor along z, pump volute at its front
    k.cyl('body', 'z', r, hz * 1.2, 0, hy * 0.05, hz * 0.3, 18);
    for (let i = -2; i <= 2; i++) k.torus('trim', 'z', r * 1.02, r * 0.05, 0, hy * 0.05, hz * 0.3 + i * hz * 0.2, 18);
    k.cyl('trim', 'z', r * 1.15, hz * 0.4, 0, hy * 0.05, -hz * 0.6, 20);
    k.bolts('chrome', 'z', r * 1.05, 10, 0, hy * 0.05, -hz * 0.39, 0.009);
    // suction and discharge
    k.cyl('pipe', 'y', r * 0.3, hy * 0.7, 0, hy * 0.6, -hz * 0.6, 12);
    k.cyl('pipe', 'x', r * 0.28, hx * 0.8, hx * 0.5, hy * 0.05, -hz * 0.6, 12);
    k.box('accent', 0.01, r * 0.6, r * 0.9, r + 0.006, hy * 0.05, hz * 0.3);
  },

  radiator(k, h, info) {
    // built flat (thin in y); a vertical fin (thin in x) is the same panel stood on its edge
    const thinX = h[0] < h[1];
    const undo = thinX ? k.push(new THREE.Matrix4().makeRotationZ(Math.PI / 2)) : null;
    const [hx, hy, hz] = thinX ? [h[1], h[0], h[2]] : h;
    const fins = info.look.fins ?? 6;
    k.box('dark', hx * 2, hy * 0.5, hz * 2, 0, -hy * 0.5);
    if (info.look.wing) {
      // folding wing: hinged along its outboard edge, lifts up and out when deployed
      const s = info.side || 1;
      const w = k.child('wing', [s * hx, hy * 0.2, 0]);
      w.box('body', hx * 1.9, hy * 0.35, hz * 1.96, -s * hx * 0.95, 0, 0);
      for (let i = 0; i < fins; i++) w.box('pipe', hx * 1.8, hy * 0.5, 0.025, -s * hx * 0.95, hy * 0.3, -hz + ((i + 0.5) * 2 * hz) / fins);
      w.box('trim', 0.04, hy * 0.6, hz * 2, 0, 0, 0);
      return (nodes, t) => {
        const n = nodes.wing;
        if (n) n.rotation.z = -s * t * 1.25;
      };
    }
    k.box('body', hx * 1.96, hy * 0.5, hz * 1.96, 0, hy * 0.2);
    // heat pipes along the length, header manifold at both ends
    for (let i = 0; i < fins; i++) k.box('pipe', 0.028, hy * 0.5, hz * 1.9, -hx + ((i + 0.5) * 2 * hx) / fins, hy * 0.55, 0);
    for (const s of [-1, 1]) k.cyl('trim', 'x', hy * 0.45, hx * 2, 0, hy * 0.45, s * hz * 0.97, 10);
    undo?.();
  },

  battery(k, h) {
    const [hx, hy, hz] = h;
    const top = cabinet(k, h, 0.94);
    // cell modules visible as horizontal ribs on every face, terminals and cable conduit on top
    for (let i = 1; i < 4; i++) {
      const y = -hy + (i / 4) * (top + hy);
      k.box('trim', hx * 2.02, 0.018, hz * 2.02, 0, y);
    }
    for (const s of [-1, 1]) {
      k.cyl('chrome', 'y', 0.02, 0.04, s * hx * 0.5, top + 0.02, 0, 8);
      k.box(s > 0 ? 'accent' : 'dark', 0.05, 0.012, 0.05, s * hx * 0.5, top + 0.045, 0);
    }
    k.rod('pipe', [-hx * 0.5, top + 0.05, 0], [-hx * 0.5, top + 0.05, hz * 0.95], 0.018);
  },

  apu(k, h) {
    const [hx, hy, hz] = h;
    const r = Math.min(hx, hy) * 0.85;
    k.cyl('body', 'z', r, hz * 1.3, 0, 0, -hz * 0.1, 20);
    k.lathe('dark', [[0, 0], [r * 1.02, 0], [r * 0.9, hz * 0.2], [r * 0.6, hz * 0.28]], 0, 0, -hz * 0.78, 'z');
    k.lathe('chrome', [[r * 0.7, 0], [r * 0.95, hz * 0.3], [r * 0.9, hz * 0.32]], 0, 0, hz * 0.55, 'z');
    for (const z of [-0.5, 0, 0.45]) k.torus('trim', 'z', r * 1.01, r * 0.06, 0, 0, z * hz, 20);
    k.bolts('chrome', 'z', r * 0.85, 12, 0, 0, -hz * 0.76, 0.008);
    for (const s of [-1, 1]) k.box('dark', hx * 0.3, hy * 0.25, hz * 1.5, s * hx * 0.75, -hy * 0.7, 0);
  },

  solar(k, h, info) {
    const [hx, hy, hz] = h;
    // hub on the roof: gimbal base and the folded stack; each wing unfolds outward from its edge
    k.box('dark', hx * 2, hy * 0.8, hz * 2, 0, -hy * 0.6);
    k.cyl('trim', 'y', Math.min(hx, hz) * 0.25, hy * 0.8, 0, 0, 0, 16);
    const cells = info.look.cells ?? 6;
    const L = hx * 0.98;
    const wing = (s: number) => {
      const w = k.child(s < 0 ? 'wingL' : 'wingR', [s * hx, hy * 0.4, 0]);
      w.box('trim', L, hy * 0.35, hz * 2, (s * L) / 2, 0, 0);
      for (let i = 0; i < cells; i++) w.box('cells', (L / cells) * 0.92, hy * 0.2, hz * 1.9, s * ((i + 0.5) * L) / cells, hy * 0.2, 0);
      w.box('chrome', L, hy * 0.25, 0.03, (s * L) / 2, 0, hz * 0.98);
      w.box('chrome', L, hy * 0.25, 0.03, (s * L) / 2, 0, -hz * 0.98);
    };
    wing(-1);
    wing(1);
    return (nodes, t) => {
      // folded = flipped over onto the hub (π), deployed = flat out (0)
      if (nodes.wingL) nodes.wingL.rotation.z = -(1 - t) * Math.PI;
      if (nodes.wingR) nodes.wingR.rotation.z = (1 - t) * Math.PI;
    };
  },

  tank(k, h, info) {
    const [hx, hy, hz] = h;
    const form = info.look.form ?? 0;
    if (form === 1) {
      // conformal: flat pillow tank with ribs across it
      k.box('body', hx * 1.9, hy * 1.8, hz * 1.9, 0, 0, 0);
      k.box('body', hx * 1.7, hy * 2, hz * 1.7, 0, 0, 0);
      const n = Math.max(2, Math.round((hz * 2) / 0.45));
      for (let i = 0; i <= n; i++) k.box('trim', hx * 2, hy * 1.2, 0.03, 0, 0, -hz + (i * 2 * hz) / n);
      k.cyl('pipe', 'x', hy * 0.35, hx * 0.5, hx * 0.8, -hy * 0.2, -hz * 0.6, 10);
      k.box('accent', hx * 0.6, 0.01, hz * 0.3, 0, -hy - 0.004, 0);
      return;
    }
    if (form === 2) {
      const r = Math.min(hx, hy, hz) * 0.94;
      k.sphere('body', r, 0, hy * 0.05, 0, [1, 1, 1], 24);
      k.torus('trim', 'y', r, r * 0.05, 0, hy * 0.05, 0, 28);
      for (let i = 0; i < 4; i++) {
        const a = (i / 4) * Math.PI * 2 + Math.PI / 4;
        k.rod('dark', [Math.cos(a) * r * 0.6, 0, Math.sin(a) * r * 0.6], [Math.cos(a) * hx * 0.9, -hy, Math.sin(a) * hz * 0.9], r * 0.05);
      }
      return;
    }
    // cylinder along z with domed ends
    const r = Math.min(hx, hy) * 0.96;
    const dome = r * 0.42;
    const body = hz * 2 - dome * 2;
    k.cyl('body', 'z', r, body, 0, 0, 0, 28);
    for (const s of [-1, 1]) {
      k.sphere('body', r, 0, 0, s * (body / 2), [1, 1, dome / r], 24, false);
      k.torus('trim', 'z', r * 1.005, r * 0.04, 0, 0, s * (body / 2 - r * 0.06), 28);
      // mounting bands
      k.torus('dark', 'z', r * 1.01, r * 0.025, 0, 0, s * body * 0.28, 28);
    }
    k.box('accent', r * 0.04, r * 0.5, body * 0.4, r * 0.99, 0, 0);
    k.cyl('pipe', 'y', r * 0.1, r * 0.25, 0, -r * 1.05, body * 0.3, 10);
  },

  engine(k, h) {
    const [hx, hy, hz] = h;
    const r = Math.min(hx, hy);
    // combustion chamber and turbopump up front, gimbal ring, bell nozzle at +z
    k.cyl('body', 'z', r * 0.72, hz * 0.95, 0, 0, -hz * 0.45, 24);
    k.sphere('body', r * 0.72, 0, 0, -hz * 0.92, [1, 1, 0.35], 20);
    k.torus('trim', 'z', r * 0.76, r * 0.06, 0, 0, -hz * 0.1, 24);
    k.torus('trim', 'z', r * 0.76, r * 0.06, 0, 0, -hz * 0.75, 24);
    k.bolts('chrome', 'z', r * 0.68, 12, 0, 0, -hz * 0.02, clamp(r * 0.02, 0.008, 0.02));
    k.lathe('dark', [[r * 0.34, 0], [r * 0.4, hz * 0.18], [r * 0.62, hz * 0.55], [r * 0.9, hz * 0.95], [r * 0.93, hz * 0.99]], 0, 0, hz * 0.0, 'z', 32);
    k.lathe('chrome', [[r * 0.3, 0], [r * 0.36, hz * 0.18], [r * 0.58, hz * 0.55], [r * 0.86, hz * 0.95]], 0, 0, hz * 0.0, 'z', 32);
    for (const s of [-1, 1]) {
      k.cyl('pipe', 'z', r * 0.12, hz * 0.6, s * r * 0.8, r * 0.3, -hz * 0.5, 10);
      k.cyl('trim', 'y', r * 0.16, r * 0.3, s * r * 0.8, r * 0.55, -hz * 0.7, 12);
    }
    k.box('accent', r * 0.06, r * 0.3, hz * 0.4, r * 0.73, 0, -hz * 0.45);
  },

  rcs(k, h) {
    const [hx, hy, hz] = h;
    k.box('trim', hx * 1.4, hy * 1.1, hz * 1.4, 0, 0, 0);
    k.box('accent', hx * 1.42, hy * 0.2, hz * 1.42, 0, hy * 0.3, 0);
    const r = Math.min(hx, hy, hz) * 0.24;
    const L = Math.min(hx, hy, hz) * 0.45;
    k.cyl('dark', 'y', r * 0.6, L, 0, hy * 0.55 + L / 2, 0, 10, r);
    for (const s of [-1, 1]) k.cyl('dark', 'x', r, L, s * (hx * 0.7 + L / 2), 0, 0, 10, r * 0.6);
    k.cyl('dark', 'z', r * 0.6, L, 0, 0, hz * 0.7 + L / 2, 10, r);
    k.cyl('dark', 'z', r, L, 0, 0, -(hz * 0.7 + L / 2), 10, r * 0.6);
  },

  gas(k, h, info) {
    const [hx, hy, hz] = h;
    // as many bottles as fit side by side along the long horizontal side
    const along: 'x' | 'z' = hx >= hz ? 'x' : 'z';
    const r = Math.min(hx, hz) * 0.9;
    const n = info.look.n ?? Math.max(1, Math.round(Math.max(hx, hz) / Math.min(hx, hz)));
    const span = Math.max(hx, hz) * 2;
    for (let i = 0; i < n; i++) {
      const o = -span / 2 + ((i + 0.5) * span) / n;
      const x = along === 'x' ? o : 0;
      const z = along === 'z' ? o : 0;
      const top = hy * 0.72;
      k.cyl('body', 'y', r, hy * 2 * 0.82 - r, x, -hy + (hy * 2 * 0.82 - r) / 2, z, 16);
      k.sphere('body', r, x, top - r * 0.2, z, [1, 0.7, 1], 14, true);
      k.cyl('accent', 'y', r * 1.01, hy * 0.12, x, top - r * 0.6, z, 16);
      k.cyl('chrome', 'y', r * 0.2, hy * 0.12, x, top + r * 0.35, z, 8);
      k.cyl('dark', 'x', r * 0.07, r * 0.5, x + r * 0.2, top + r * 0.4, z, 6);
    }
    // wall straps
    for (const y of [-hy * 0.4, hy * 0.35]) k.box('dark', along === 'x' ? span : r * 2.1, 0.03, along === 'z' ? span : r * 2.1, 0, y, 0);
  },

  o2gen(k, h) {
    const [hx, hy, hz] = h;
    const top = cabinet(k, h, 0.8);
    // electrolysis stack: a column on top with its gas outlet
    const r = Math.min(hx, hz) * 0.42;
    k.cyl('trim', 'y', r, hy * 0.36, 0, top + hy * 0.18, 0, 16);
    for (let i = 0; i < 4; i++) k.torus('chrome', 'y', r * 1.02, r * 0.04, 0, top + hy * (0.05 + i * 0.09), 0, 16);
    k.rod('pipe', [0, top + hy * 0.36, 0], [hx * 0.6, top + hy * 0.36, 0], r * 0.12);
  },

  scrubber(k, h) {
    const [hx, hy, hz] = h;
    const top = cabinet(k, h, 0.85);
    // twin amine beds with the swing valve between them
    const r = Math.min(hx, hz) * 0.3;
    for (const s of [-1, 1]) k.cyl('trim', 'y', r, hy * 0.28, s * hx * 0.45, top + hy * 0.14, 0, 14);
    k.box('chrome', hx * 0.4, hy * 0.1, r * 0.8, 0, top + hy * 0.1, 0);
  },

  water(k, h) {
    const [hx, hy, hz] = h;
    const r = Math.min(hx, hz) * 0.88;
    k.box('dark', hx * 2, hy * 0.08, hz * 2, 0, -hy + hy * 0.04);
    k.cyl('body', 'y', r, hy * 1.7, 0, -hy * 0.02, 0, 20);
    k.sphere('body', r, 0, hy * 0.83, 0, [1, 0.25, 1], 16, true);
    for (const y of [-0.55, 0.1, 0.65]) k.torus('trim', 'y', r * 1.01, r * 0.035, 0, hy * y, 0, 20);
    // sight glass with the level, drawn blue; frame corners
    k.box('glass', 0.02, hy * 1.2, r * 0.25, r * 0.98, -hy * 0.05, 0);
    for (const sx of [-1, 1]) for (const sz of [-1, 1]) k.box('dark', 0.04, hy * 2, 0.04, sx * hx * 0.92, 0, sz * hz * 0.92);
    k.cyl('pipe', 'x', r * 0.08, hx * 0.5, hx * 0.5, -hy * 0.8, 0, 8);
  },

  recycler(k, h) {
    const [hx, hy, hz] = h;
    const top = cabinet(k, h, 0.92);
    // round access port on the long faces, distillation column behind
    for (const s of [-1, 1]) {
      if (hx >= hz) k.torus('chrome', 'z', Math.min(hx, hy) * 0.3, 0.015, 0, hy * 0.2, s * hz * 1.0, 20);
      else k.torus('chrome', 'x', Math.min(hz, hy) * 0.3, 0.015, s * hx * 1.0, hy * 0.2, 0, 20);
    }
    k.cyl('pipe', 'y', Math.min(hx, hz) * 0.12, hy * 0.2, -hx * 0.4, top + hy * 0.1, 0, 10);
    k.cyl('pipe', 'y', Math.min(hx, hz) * 0.12, hy * 0.2, hx * 0.4, top + hy * 0.1, 0, 10);
  },

  pantry(k, h) {
    const [hx, hy, hz] = h;
    k.box('body', hx * 2, hy * 2, hz * 2, 0, 0, 0);
    // drawers on both long faces with handles
    const n = 4;
    for (let i = 0; i < n; i++) {
      const y = -hy + ((i + 0.5) * 2 * hy) / n;
      for (const s of [-1, 1]) {
        if (hx >= hz) {
          k.box('trim', hx * 1.9, 0.012, 0.012, 0, y + hy / n - 0.01, s * hz);
          k.box('chrome', hx * 0.5, 0.02, 0.02, 0, y, s * (hz + 0.012));
        } else {
          k.box('trim', 0.012, 0.012, hz * 1.9, s * hx, y + hy / n - 0.01, 0);
          k.box('chrome', 0.02, 0.02, hz * 0.5, s * (hx + 0.012), y, 0);
        }
      }
    }
    k.box('accent', hx * 2.01, hy * 0.06, hz * 2.01, 0, hy * 0.9, 0);
  },

  radar(k, h) {
    const [hx, hy, hz] = h;
    const r = Math.min(hx, hz);
    k.cyl('dark', 'y', r * 1.02, hy * 0.25, 0, -hy + hy * 0.125, 0, 24);
    k.sphere('body', r * 0.97, 0, -hy + hy * 0.25, 0, [1, (hy * 1.7) / r, 1], 24, true);
    k.torus('trim', 'y', r * 0.98, r * 0.03, 0, -hy + hy * 0.25, 0, 24);
  },

  antenna(k, h) {
    const [hx, hy, hz] = h;
    const r = Math.min(hx, hz);
    k.cyl('dark', 'y', r * 0.3, hy * 0.2, 0, -hy + hy * 0.1, 0, 12);
    k.cyl('trim', 'y', r * 0.12, hy * 1.0, 0, -hy * 0.4, 0, 10);
    // dish tilted toward the sky, feed horn on struts
    const dish = new THREE.Matrix4().compose(new THREE.Vector3(0, hy * 0.35, r * 0.1), new THREE.Quaternion().setFromEuler(new THREE.Euler(0.6, 0, 0)), new THREE.Vector3(1, 1, 1));
    const undo = k.push(dish);
    k.lathe('body', [[0, -r * 0.2], [r * 0.4, -r * 0.16], [r * 0.75, -r * 0.05], [r * 0.9, r * 0.06]], 0, 0, 0, 'y', 24);
    k.rod('chrome', [0, -r * 0.1, 0], [0, r * 0.55, 0], r * 0.03);
    k.cyl('dark', 'y', r * 0.08, r * 0.12, 0, r * 0.6, 0, 8);
    undo();
  },

  turret(k, h) {
    const [hx, hy, hz] = h;
    k.cyl('dark', 'y', Math.min(hx, hz) * 0.95, hy * 0.35, 0, -hy + hy * 0.175, 0, 20);
    k.cyl('body', 'y', Math.min(hx, hz) * 0.72, hy * 0.7, 0, -hy * 0.3, 0, 16, Math.min(hx, hz) * 0.6);
    k.torus('trim', 'y', Math.min(hx, hz) * 0.73, 0.02, 0, -hy * 0.6, 0, 20);
    for (const s of [-1, 1]) {
      k.box('trim', hx * 0.35, hy * 0.5, hz * 1.4, s * hx * 0.38, hy * 0.35, hz * 0.1);
      k.cyl('dark', 'z', hy * 0.14, hz * 0.3, s * hx * 0.38, hy * 0.35, -hz * 0.75, 8);
    }
    k.box('accent', hx * 0.2, hy * 0.2, hz * 0.2, 0, hy * 0.2, -hz * 0.2);
  },

  grav(k, h) {
    const [hx, hy, hz] = h;
    const r = Math.min(hx, hz);
    k.cyl('body', 'y', r, hy * 1.4, 0, 0, 0, 28);
    k.torus('trim', 'y', r * 0.6, hy * 0.25, 0, hy * 0.1, 0, 28);
    k.torus('accent', 'y', r * 1.0, hy * 0.08, 0, -hy * 0.5, 0, 28);
    k.bolts('chrome', 'y', r * 0.85, 12, 0, hy * 0.72, 0, 0.012);
  },

  loader(k, h) {
    const [hx, hy, hz] = h;
    k.box('body', hx * 1.8, hy * 1.6, hz * 1.4, 0, 0, 0);
    k.box('trim', hx * 0.4, hy * 0.4, hz * 0.8, 0, hy * 0.2, hz * 0.9);
    k.grille('dark', 'z', hx * 1.2, hy * 0.8, 5, 0, -hy * 0.2, -hz * 0.71);
  },
};

/** Fallback for a model nobody wrote yet: a plain cabinet (still shows its maker band). */
export const DEFAULT_MACHINE: ModelBuilder = (k, h) => {
  cabinet(k, h, 1);
};
