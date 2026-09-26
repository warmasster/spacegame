import * as THREE from 'three';
import { REACH, SOLID_HP } from '../../shared/ship/sim';
import type { Particles } from '../fx/particles';
import type { ShipClient, ShipHit } from './ship';

export interface InteractionSink {
  /** Operate a control (server request, or the offline authority). */
  interact(ship: ShipClient, ctl: number): void;
  /** Repair tool tick on a panel (dt seconds of welding). */
  repair(ship: ShipClient, panel: number, dt: number): void;
}

export interface PromptInfo {
  title: string;
  state: string;
  tone: 'on' | 'off' | 'warn' | 'bad';
  hint: string;
  hintTone?: 'bad';
  bar?: number;
}

export type Target = ShipHit & { ship: ShipClient; inReach: boolean };

const KIND: Record<string, string> = { hull: 'casco', glass: 'ventana', floor: 'cubierta', bulkhead: 'mamparo' };

/**
 * Look-and-click: the crosshair ray picks a ship control or panel, a highlight shows what you
 * would act on, a click / E operates controls, holding E welds panels back together.
 */
export class Interaction {
  target: Target | null = null;
  repairing = false;
  private denied: { key: string; text: string; until: number } | null = null;
  private box: THREE.LineSegments;
  private outline: THREE.LineLoop;
  private ghost: THREE.Mesh;
  private beam: THREE.Mesh;
  private outlineKey = '';
  private tick = 0;
  private sparkT = 0;

  constructor(
    scene: THREE.Scene,
    private particles: Particles,
    private sink: InteractionSink,
  ) {
    const noDepth = { transparent: true, depthWrite: false, toneMapped: false };
    this.box = new THREE.LineSegments(new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1)), new THREE.LineBasicMaterial({ color: 0x4fd8f0, ...noDepth }));
    this.outline = new THREE.LineLoop(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({ color: 0x4fd8f0, ...noDepth }));
    this.ghost = new THREE.Mesh(new THREE.BufferGeometry(), new THREE.MeshBasicMaterial({ color: 0x4fd8f0, side: THREE.DoubleSide, opacity: 0.15, ...noDepth }));
    this.beam = new THREE.Mesh(new THREE.CylinderGeometry(0.004, 0.004, 1, 6).translate(0, 0.5, 0), new THREE.MeshBasicMaterial({ color: new THREE.Color(2.5, 3.5, 5), transparent: true, blending: THREE.AdditiveBlending, depthWrite: false }));
    for (const o of [this.box, this.outline, this.ghost, this.beam]) {
      o.matrixAutoUpdate = false;
      o.visible = false;
      o.frustumCulled = false;
      o.renderOrder = 20;
      scene.add(o);
    }
  }

  /** Aim, highlight, weld. Returns what the helmet HUD should say about the target. */
  update(dt: number, ctx: { camera: THREE.Camera; eye: THREE.Vector3; hand: THREE.Vector3; ships: ShipClient[]; holdRepair: boolean; now: number; disabled: boolean }): PromptInfo | null {
    const { camera, eye, ships } = ctx;
    const origin = camera.getWorldPosition(new THREE.Vector3());
    const dir = camera.getWorldDirection(new THREE.Vector3());
    // third person: the ray starts behind the astronaut; reach is still measured from the eye
    const max = origin.distanceTo(eye) + REACH.repair + 0.5;
    let best: Target | null = null;
    if (!ctx.disabled) {
      for (const ship of ships) {
        if (ship.position.distanceTo(eye) > 40) continue;
        const h = ship.pick(origin, dir, max);
        if (h && (!best || h.dist < best.dist)) best = { ...h, ship, inReach: false };
      }
    }
    if (best) best.inReach = best.point.distanceTo(eye) <= (best.kind === 'control' ? REACH.control : REACH.repair);
    this.target = best;

    // welding
    const t = best;
    const canWeld = !!t && t.kind === 'panel' && t.inReach && ctx.holdRepair && t.ship.sim.hp[t.index] < t.ship.sim.def.panels[t.index].maxHp;
    this.repairing = canWeld;
    if (canWeld) {
      this.tick += dt;
      if (this.tick >= 0.1) {
        this.sink.repair(t!.ship, t!.index, this.tick);
        this.tick = 0;
      }
      this.weldFx(dt, ctx.hand, t!.point, t!.normal);
    } else this.tick = 0;
    this.beam.visible = canWeld;
    this.draw(t, ctx.now);
    return t ? this.prompt(t, ctx.now) : null;
  }

  /** Click / E. True when it acted on a control (so the click does not also fire). */
  use(now: number): boolean {
    const t = this.target;
    if (!t || t.kind !== 'control' || !t.inReach) return false;
    const c = t.ship.sim.def.controls[t.index];
    const reason = t.ship.sim.blocked(c);
    t.ship.view.pressed(t.index);
    if (reason) {
      this.denied = { key: `${t.ship.id}:${t.index}`, text: reason, until: now + 1.8 };
      return true;
    }
    this.sink.interact(t.ship, t.index);
    return true;
  }

  /** Server refused (rare: the client normally knows first). */
  deny(ship: number, ctl: number, text: string, now: number) {
    this.denied = { key: `${ship}:${ctl}`, text, until: now + 1.8 };
  }

  private weldFx(dt: number, hand: THREE.Vector3, at: THREE.Vector3, n: THREE.Vector3) {
    const d = at.clone().sub(hand);
    const L = d.length();
    const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 1, 0), d.normalize());
    const flicker = 0.6 + Math.random() * 0.8;
    this.beam.matrix.compose(hand, q, new THREE.Vector3(flicker, L, flicker));
    this.beam.matrixWorld.copy(this.beam.matrix);
    this.sparkT -= dt;
    if (this.sparkT > 0) return;
    this.sparkT = 0.03;
    for (let k = 0; k < 4; k++) {
      this.particles.emit('glow', {
        pos: at,
        vel: new THREE.Vector3().randomDirection().multiplyScalar(0.6 + Math.random() * 2.2).addScaledVector(n, 1.2),
        color: Math.random() < 0.5 ? [2.4, 3, 4.5] : [4, 2.6, 1],
        life: 0.1 + Math.random() * 0.35,
        size: 0.012 + Math.random() * 0.018,
        gravity: 1.62,
      });
    }
    this.particles.emit('glow', { pos: at, vel: n.clone().multiplyScalar(0.05), color: [3, 4, 6], life: 0.05, size: 0.12 + Math.random() * 0.1 });
  }

  private draw(t: Target | null, now: number) {
    this.box.visible = this.outline.visible = this.ghost.visible = false;
    if (!t) return;
    const M = t.ship.view.root.matrixWorld;
    const deniedHere = this.denied && this.denied.until > now && this.denied.key === `${t.ship.id}:${t.index}`;
    if (t.kind === 'control') {
      const c = t.ship.sim.def.controls[t.index];
      const F = new THREE.Matrix4().makeBasis(new THREE.Vector3(...c.u), new THREE.Vector3(...c.v), new THREE.Vector3(...c.n)).setPosition(...c.c);
      F.multiply(new THREE.Matrix4().compose(new THREE.Vector3(0, 0, c.half[2] * 0.6), new THREE.Quaternion(), new THREE.Vector3(c.half[0] * 2.5, c.half[1] * 2.5, c.half[2] * 2)));
      this.box.matrix.multiplyMatrices(M, F);
      this.box.matrixWorld.copy(this.box.matrix);
      (this.box.material as THREE.LineBasicMaterial).color.set(deniedHere ? 0xff5a4a : t.inReach ? 0x4fd8f0 : 0x2a6f80);
      this.box.visible = true;
      return;
    }
    const sim = t.ship.sim;
    const p = sim.def.panels[t.index];
    const r = sim.hp[p.index] / p.maxHp;
    const side = new THREE.Vector3(...p.n).transformDirection(M).dot(t.normal) > 0 ? 1 : -1;
    const key = `${t.ship.id}:${t.index}:${side}`;
    if (key !== this.outlineKey) {
      this.outlineKey = key;
      const lift = side * (p.t / 2 + 0.006);
      const pts = p.poly.map(([x, y]) => new THREE.Vector3(p.c[0] + p.u[0] * x + p.v[0] * y + p.n[0] * lift, p.c[1] + p.u[1] * x + p.v[1] * y + p.n[1] * lift, p.c[2] + p.u[2] * x + p.v[2] * y + p.n[2] * lift));
      this.outline.geometry.dispose();
      this.outline.geometry = new THREE.BufferGeometry().setFromPoints(pts);
      const shape = new THREE.Shape(p.poly.map(([x, y]) => new THREE.Vector2(x, y)));
      const g = new THREE.ShapeGeometry(shape);
      g.applyMatrix4(new THREE.Matrix4().makeBasis(new THREE.Vector3(...p.u), new THREE.Vector3(...p.v), new THREE.Vector3(...p.n)).setPosition(...p.c));
      this.ghost.geometry.dispose();
      this.ghost.geometry = g;
    }
    this.outline.matrix.copy(M);
    this.outline.matrixWorld.copy(M);
    const col = t.hole ? 0x4fd8f0 : r > 0.8 ? 0x5cf29a : r > 0.5 ? 0xffc04a : 0xff5a4a;
    (this.outline.material as THREE.LineBasicMaterial).color.setHex(t.inReach ? col : 0x2a6f80);
    this.outline.visible = true;
    if (t.hole) {
      this.ghost.matrix.copy(M);
      this.ghost.matrixWorld.copy(M);
      (this.ghost.material as THREE.MeshBasicMaterial).opacity = 0.08 + 0.4 * (sim.hp[p.index] / SOLID_HP) + (this.repairing ? 0.05 * Math.random() : 0);
      this.ghost.visible = true;
    }
  }

  private prompt(t: Target, now: number): PromptInfo {
    const sim = t.ship.sim;
    const denied = this.denied && this.denied.until > now && this.denied.key === `${t.ship.id}:${t.index}` ? this.denied.text : null;
    if (t.kind === 'control') {
      const c = sim.def.controls[t.index];
      const v = sim.sw[c.key] ?? 0;
      const blocked = sim.blocked(c);
      const hint = !t.inReach ? 'Acércate para accionar' : denied ?? (c.action === 'reset' ? '[CLIC] / [E] reconocer' : '[CLIC] / [E] accionar');
      return { title: c.name, state: c.states[v] ?? '', tone: c.key === 'caution' ? (v ? 'warn' : 'off') : v ? 'on' : 'off', hint: blocked && t.inReach && !denied ? `${hint} · ${blocked}` : hint, hintTone: denied || (blocked && t.inReach) ? 'bad' : undefined };
    }
    const p = sim.def.panels[t.index];
    const hp = sim.hp[p.index];
    const r = hp / p.maxHp;
    const conduits = p.conduits.map((id) => sim.def.subsystems.find((s) => s.id === id)!.label).join(', ');
    const state = t.hole ? (hp > 0 ? `RECONSTRUYENDO ${Math.round((hp / SOLID_HP) * 100)}%` : 'DESTRUIDO') : `${Math.round(r * 100)}%`;
    const hint = !t.inReach ? 'Acércate para reparar' : hp >= p.maxHp ? 'Íntegro' : this.repairing ? 'Soldando…' : '[MANTÉN E] reparar';
    return {
      title: `Panel ${p.id} · ${KIND[p.kind]}${conduits ? ` · conducto ${conduits}` : ''}`,
      state,
      tone: t.hole ? 'bad' : r > 0.8 ? 'on' : r > 0.5 ? 'warn' : 'bad',
      hint,
      bar: t.hole ? hp / SOLID_HP : r,
    };
  }
}
