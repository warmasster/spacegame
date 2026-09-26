import * as THREE from 'three';
import type { ScreenDef, ScreenPage } from '../../shared/ship/def';
import type { ShipSim } from '../../shared/ship/sim';

/** Animated state the displays report (door / ramp / shield travel, 0..1). */
export interface ShipAnimState {
  doors: Record<string, number>;
  ramp: number;
  shield: number;
}

const FONT = 'ui-monospace, Menlo, Consolas, monospace';
const C = {
  bg: '#030a10',
  grid: 'rgba(80,180,220,0.10)',
  dim: '#4f7d8f',
  txt: '#bfefff',
  ok: '#5cf29a',
  warn: '#ffc04a',
  bad: '#ff5a4a',
  cyan: '#4fd8f0',
};

/** Multi-function displays drawn into canvases (the ship's own diagnostics, readable in-world). */
export class ShipScreens {
  readonly meshes: THREE.Mesh[] = [];
  private items: Array<{ def: ScreenDef; mesh: THREE.Mesh; ctx: CanvasRenderingContext2D; tex: THREE.CanvasTexture; mat: THREE.MeshBasicMaterial; w: number; h: number }> = [];
  private last = -1;

  constructor(private sim: ShipSim) {
    for (const def of sim.def.screens) {
      const w = 512;
      const h = Math.round((w * def.h) / def.w);
      const canvas = document.createElement('canvas');
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext('2d')!;
      const tex = new THREE.CanvasTexture(canvas);
      tex.colorSpace = THREE.SRGBColorSpace;
      tex.anisotropy = 4;
      const mat = new THREE.MeshBasicMaterial({ map: tex, color: new THREE.Color(1.5, 1.5, 1.5) });
      const mesh = new THREE.Mesh(new THREE.PlaneGeometry(def.w, def.h), mat);
      mesh.matrixAutoUpdate = false;
      mesh.matrix.makeBasis(new THREE.Vector3(...def.u), new THREE.Vector3(...def.v), new THREE.Vector3(...def.n)).setPosition(...def.c);
      mesh.name = def.id;
      this.meshes.push(mesh);
      this.items.push({ def, mesh, ctx, tex, mat, w, h });
    }
  }

  /** Redraw at ~4 Hz (they are text, nobody needs 60). */
  update(time: number, anim: ShipAnimState, hidden: (host: number) => boolean) {
    const slot = Math.floor(time * 4);
    if (slot === this.last) return;
    this.last = slot;
    for (const it of this.items) {
      const off = hidden(it.def.host);
      it.mesh.visible = !off;
      if (off) continue;
      const on = it.def.page === 'power' ? true : this.sim.powered('avionics');
      it.mat.color.setScalar(on ? 1.5 : 0.02);
      if (!on) continue;
      this.draw(it.ctx, it.w, it.h, it.def.page, time, anim);
      it.tex.needsUpdate = true;
    }
  }

  private draw(g: CanvasRenderingContext2D, w: number, h: number, page: ScreenPage, time: number, anim: ShipAnimState) {
    g.fillStyle = C.bg;
    g.fillRect(0, 0, w, h);
    g.strokeStyle = C.grid;
    g.lineWidth = 1;
    for (let x = 0; x < w; x += 32) line(g, x, 0, x, h);
    for (let y = 0; y < h; y += 32) line(g, 0, y, w, y);
    if (page === 'status') this.status(g, w, h, time, anim);
    else if (page === 'hull') this.hull(g, w, h, time);
    else this.power(g, w, h, time);
    // bezel glow line
    g.strokeStyle = 'rgba(79,216,240,0.35)';
    g.lineWidth = 2;
    g.strokeRect(3, 3, w - 6, h - 6);
  }

  private header(g: CanvasRenderingContext2D, w: number, title: string, time: number) {
    const sim = this.sim;
    g.fillStyle = C.cyan;
    g.font = `700 19px ${FONT}`;
    g.textBaseline = 'top';
    g.fillText(title, 14, 10);
    g.textAlign = 'right';
    g.fillStyle = C.dim;
    g.font = `600 15px ${FONT}`;
    g.fillText(`${sim.def.name.toUpperCase()} · ${sim.def.registry}`, w - 14, 12);
    g.textAlign = 'left';
    if (sim.sw.caution === 1 && Math.floor(time * 2) % 2 === 0) {
      g.fillStyle = C.warn;
      g.fillRect(w / 2 - 70, 8, 140, 24);
      g.fillStyle = '#1a0f00';
      g.font = `700 16px ${FONT}`;
      g.textAlign = 'center';
      g.fillText('ALARMA GENERAL', w / 2, 12);
      g.textAlign = 'left';
    }
    g.strokeStyle = 'rgba(79,216,240,0.4)';
    line(g, 12, 38, w - 12, 38);
  }

  private status(g: CanvasRenderingContext2D, w: number, h: number, time: number, anim: ShipAnimState) {
    const sim = this.sim;
    this.header(g, w, 'SISTEMAS', time);
    let y = 48;
    g.font = `600 15px ${FONT}`;
    const row = (label: string, value: string, color: string) => {
      g.fillStyle = C.dim;
      g.fillText(label, 16, y);
      g.fillStyle = color;
      g.fillText(value, 190, y);
      y += 21;
    };
    row('REACTOR', sim.sw.reactor ? 'EN MARCHA' : 'PARADO', sim.sw.reactor ? C.ok : C.bad);
    for (const s of sim.def.subsystems) {
      const cut = sim.conduitCut(s.id);
      const v = !sim.sw.reactor ? 'SIN ENERGÍA' : !sim.sw[s.breaker] ? 'DISYUNTOR ABIERTO' : cut ? `CONDUCTO ${cut.id}` : 'OK';
      row(s.label, v, v === 'OK' ? C.ok : cut ? C.bad : C.warn);
    }
    y += 4;
    g.strokeStyle = 'rgba(79,216,240,0.25)';
    line(g, 12, y - 6, w - 12, y - 6);
    // gear: three greens
    g.fillStyle = C.dim;
    g.fillText('TREN', 16, y);
    for (let i = 0; i < 3; i++) {
      g.fillStyle = sim.sw.gear ? C.ok : C.warn;
      g.fillRect(70 + i * 22, y + 2, 16, 14);
    }
    g.fillStyle = C.txt;
    g.fillText(sim.sw.gear ? 'ABAJO' : 'ARRIBA', 144, y);
    g.fillStyle = C.dim;
    g.fillText('RAMPA', 250, y);
    bar(g, 310, y + 3, 90, 12, anim.ramp, anim.ramp > 0.99 || anim.ramp < 0.01 ? C.cyan : C.warn);
    g.fillStyle = C.txt;
    g.fillText(`${Math.round(anim.ramp * 100)}%`, 410, y);
    y += 22;
    g.fillStyle = C.dim;
    g.fillText('ESCUDO', 16, y);
    bar(g, 90, y + 3, 90, 12, anim.shield, C.cyan);
    g.fillText('PUERTAS', 250, y);
    let x = 330;
    for (const d of sim.def.doors) {
      const v = anim.doors[d.key] ?? 0;
      g.fillStyle = v > 0.99 ? C.ok : v < 0.01 ? C.dim : C.warn;
      g.fillText(d.key === 'door.cockpit' ? 'CAB' : 'BOD', x, y);
      x += 52;
    }
    y += 24;
    const integ = sim.integrity();
    g.fillStyle = C.dim;
    g.fillText('CASCO', 16, y);
    bar(g, 90, y + 3, 300, 12, integ, integ > 0.85 ? C.ok : integ > 0.6 ? C.warn : C.bad);
    g.fillStyle = C.txt;
    g.fillText(`${Math.round(integ * 100)}%`, 400, y);
    void h;
  }

  private hull(g: CanvasRenderingContext2D, w: number, h: number, time: number) {
    const sim = this.sim;
    this.header(g, w, 'INTEGRIDAD DEL CASCO', time);
    // unwrap the hull around the ship's long axis: x = length (nose left), y = angle from the keel
    const cy = 1.3;
    const z0 = -10.2;
    const z1 = 6;
    const top = 48;
    const bottom = h - 34;
    const X = (z: number) => 16 + ((z - z0) / (z1 - z0)) * (w - 32);
    const Y = (a: number) => top + ((a + Math.PI) / (2 * Math.PI)) * (bottom - top);
    const blink = Math.floor(time * 3) % 2 === 0;
    let holes = 0;
    for (const p of sim.def.panels) {
      const pts = p.poly.map((q) => [p.c[0] + p.u[0] * q[0] + p.v[0] * q[1], p.c[1] + p.u[1] * q[0] + p.v[1] * q[1], p.c[2] + p.u[2] * q[0] + p.v[2] * q[1]]);
      const zs = pts.map((q) => q[2]);
      const angs = pts.map((q) => Math.atan2(q[0], -(q[1] - cy)));
      // panels straddling the roof centreline wrap around ±π
      if (Math.max(...angs) - Math.min(...angs) > Math.PI) for (let i = 0; i < angs.length; i++) if (angs[i] < 0) angs[i] += Math.PI * 2;
      let xa = X(Math.min(...zs));
      let xb = X(Math.max(...zs));
      if (xb - xa < 5) {
        const m = (xa + xb) / 2;
        xa = m - 3;
        xb = m + 3;
      }
      const ya = Y(Math.min(...angs));
      const yb = Y(Math.min(Math.PI, Math.max(...angs)));
      const r = sim.hp[p.index] / p.maxHp;
      const hole = sim.hole(p.index);
      if (hole) holes++;
      g.fillStyle = hole ? (sim.hp[p.index] > 0 ? 'rgba(79,216,240,0.35)' : '#000') : r > 0.8 ? 'rgba(92,242,154,0.55)' : r > 0.5 ? 'rgba(255,192,74,0.7)' : 'rgba(255,90,74,0.8)';
      if (p.kind === 'glass' && !hole && r > 0.8) g.fillStyle = 'rgba(79,170,240,0.55)';
      g.fillRect(xa + 1, ya + 1, xb - xa - 2, yb - ya - 2);
      if (hole && (blink || sim.hp[p.index] > 0)) {
        g.strokeStyle = sim.hp[p.index] > 0 ? C.cyan : C.bad;
        g.lineWidth = 2;
        g.strokeRect(xa + 1, ya + 1, xb - xa - 2, yb - ya - 2);
      }
    }
    g.fillStyle = C.dim;
    g.font = `600 13px ${FONT}`;
    g.fillText('PROA', 16, h - 26);
    g.textAlign = 'right';
    g.fillText('POPA', w - 16, h - 26);
    g.textAlign = 'center';
    g.fillStyle = holes ? C.bad : C.ok;
    g.font = `700 14px ${FONT}`;
    g.fillText(holes ? `${holes} BRECHA${holes > 1 ? 'S' : ''}` : 'CASCO ESTANCO', w / 2, h - 26);
    g.textAlign = 'left';
  }

  private power(g: CanvasRenderingContext2D, w: number, h: number, time: number) {
    const sim = this.sim;
    this.header(g, w, 'DISTRIBUCIÓN', time);
    const on = sim.sw.reactor === 1;
    // reactor block
    g.fillStyle = on ? 'rgba(92,242,154,0.18)' : 'rgba(255,90,74,0.18)';
    g.fillRect(16, 60, 110, 70);
    g.strokeStyle = on ? C.ok : C.bad;
    g.lineWidth = 2;
    g.strokeRect(16, 60, 110, 70);
    g.fillStyle = on ? C.ok : C.bad;
    g.font = `700 16px ${FONT}`;
    g.fillText('REACTOR', 28, 72);
    g.font = `600 13px ${FONT}`;
    g.fillText(on ? `${(96 + Math.sin(time * 1.3) * 0.8).toFixed(1)}% · 42 kW` : 'PARADO', 28, 98);
    // bus
    g.strokeStyle = on ? C.ok : C.dim;
    g.lineWidth = 3;
    line(g, 126, 95, 160, 95);
    const n = sim.def.subsystems.length;
    const y0 = 52;
    const dy = (h - 70) / n;
    line(g, 160, y0 + dy * 0.5, 160, y0 + dy * (n - 0.5));
    g.font = `600 14px ${FONT}`;
    sim.def.subsystems.forEach((s, i) => {
      const y = y0 + dy * (i + 0.5);
      const brk = sim.sw[s.breaker] === 1;
      const cut = sim.conduitCut(s.id);
      const live = on && brk && !cut;
      g.strokeStyle = on ? C.ok : C.dim;
      line(g, 160, y, 200, y);
      // breaker symbol
      g.strokeStyle = on ? (brk ? C.ok : C.warn) : C.dim;
      if (brk) line(g, 200, y, 236, y);
      else line(g, 200, y, 232, y - 14);
      g.fillStyle = g.strokeStyle as string;
      g.fillRect(196, y - 3, 6, 6);
      g.fillRect(234, y - 3, 6, 6);
      g.strokeStyle = live ? C.ok : cut && on && brk ? C.bad : C.dim;
      if (cut && on && brk) {
        line(g, 240, y, 262, y);
        g.fillStyle = C.bad;
        g.fillText('✕', 266, y - 9);
        line(g, 282, y, 300, y);
      } else line(g, 240, y, 300, y);
      g.fillStyle = live ? C.ok : C.dim;
      g.fillText(s.label, 306, y - 16);
      g.fillStyle = live ? C.txt : cut ? C.bad : C.warn;
      g.font = `600 12px ${FONT}`;
      g.fillText(live ? 'CON TENSIÓN' : !on ? 'SIN ENERGÍA' : !brk ? 'DISYUNTOR ABIERTO' : `CORTE EN ${cut!.id}`, 306, y + 1);
      g.font = `600 14px ${FONT}`;
    });
  }
}

function line(g: CanvasRenderingContext2D, x0: number, y0: number, x1: number, y1: number) {
  g.beginPath();
  g.moveTo(x0, y0);
  g.lineTo(x1, y1);
  g.stroke();
}

function bar(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, v: number, color: string) {
  g.strokeStyle = 'rgba(79,216,240,0.4)';
  g.lineWidth = 1;
  g.strokeRect(x, y, w, h);
  g.fillStyle = color;
  g.fillRect(x + 2, y + 2, Math.max(0, Math.min(1, v)) * (w - 4), h - 4);
}
