import * as THREE from 'three';
import { clearOfHull } from '../../shared/frames';
import { SEAT_PICK, boxFrame, coverHit, seatFrame, type ConsoleDef } from '../../shared/ship/def';
import { REACH, SOLID_HP } from '../../shared/ship/sim';
import type { Particles } from '../fx/particles';
import { origin } from '../render/origin';
import type { ShipClient, ShipHit } from './ship';

export interface InteractionSink {
  /** Operate a control (server request, or the offline authority). */
  interact(ship: ShipClient, ctl: number): void;
  /** Repair tool tick on a panel (dt seconds of welding). */
  repair(ship: ShipClient, panel: number, dt: number): void;
  /** Repair tool tick on a machine. */
  repairPart(ship: ShipClient, part: number, dt: number): void;
  /** Sit in a seat. */
  sit(ship: ShipClient, seat: number): void;
}

export interface PromptInfo {
  title: string;
  state: string;
  tone: 'on' | 'off' | 'warn' | 'bad';
  hint: string;
  hintTone?: 'bad';
  /** What the control actually does, one line. */
  detail?: string;
  bar?: number;
}

export type Target = ShipHit & { ship: ShipClient; inReach: boolean };

const _h = new THREE.Vector3();

const KIND: Record<string, string> = { hull: 'casco', glass: 'ventana', floor: 'cubierta', bulkhead: 'mamparo' };

/**
 * Look-and-click: the crosshair ray picks a ship control or panel, a highlight shows what you
 * would act on, a click / E operates controls, holding E welds panels back together.
 */
export class Interaction {
  target: Target | null = null;
  repairing = false;
  private box: THREE.LineSegments;
  private outline: THREE.LineLoop;
  private ghost: THREE.Mesh;
  private beam: THREE.Mesh;
  /** Button consoles that were mounted on the blown panel under the crosshair. */
  private consoleGhosts: THREE.Group;
  private consoleMat: THREE.MeshBasicMaterial;
  private outlineKey = '';
  private consoleKey = '';
  private tick = 0;
  private sparkT = 0;
  private tool = false;
  private seated = false;

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
    this.consoleMat = new THREE.MeshBasicMaterial({ color: 0x9ad7ff, side: THREE.DoubleSide, transparent: true, opacity: 0.28, depthWrite: false, toneMapped: false });
    this.consoleGhosts = new THREE.Group();
    for (const o of [this.box, this.outline, this.ghost, this.beam, this.consoleGhosts]) {
      o.matrixAutoUpdate = false;
      o.visible = false;
      o.frustumCulled = false;
      o.renderOrder = 20;
      scene.add(o);
    }
  }

  /** Aim, highlight, weld. Returns what the helmet HUD should say about the target. */
  /**
   * `tool`: the welder is in hand (its tip is `hand`); `holdRepair`: its trigger is held.
   */
  update(dt: number, ctx: { camera: THREE.Camera; eye: THREE.Vector3; hand: THREE.Vector3; ships: ShipClient[]; tool: boolean; holdRepair: boolean; now: number; disabled: boolean; seated: boolean }): PromptInfo | null {
    this.tool = ctx.tool;
    this.seated = ctx.seated;
    for (const ship of ctx.ships) ship.view.setWelder(ctx.tool);
    const { camera, eye, ships } = ctx;
    // the game asks in world coordinates; the camera's matrices are in render space
    const from = origin.worldOf(camera, new THREE.Vector3());
    const dir = camera.getWorldDirection(new THREE.Vector3());
    // third person: the ray starts behind the astronaut; reach is still measured from the eye
    const max = from.distanceTo(eye) + REACH.repair + 0.5;
    let best: Target | null = null;
    if (!ctx.disabled) {
      for (const ship of ships) {
        // out of reach of anything on it (its own size, not a fixed radius)
        if (clearOfHull(ship.sim.def, ship.sim.toLocal([eye.x, eye.y, eye.z]), max)) continue;
        const h = ship.pick(from, dir, max, ctx.seated);
        if (h && (!best || h.dist < best.dist)) best = { ...h, ship, inReach: false };
      }
    }
    if (best) best.inReach = best.point.distanceTo(eye) <= (best.kind === 'panel' || best.kind === 'part' ? REACH.repair : REACH.control);
    this.target = best;

    // welding
    const t = best;
    const panelWeld = !!t && t.kind === 'panel' && t.inReach && ctx.tool && ctx.holdRepair && t.ship.sim.hp[t.index] < t.ship.sim.def.panels[t.index].maxHp;
    const part = t?.kind === 'part' ? t.ship.sim.def.parts[t.index] : null;
    const partWeld = !!t && !!part && t.inReach && ctx.tool && ctx.holdRepair && t.ship.sim.partHp(part) < part.maxHp;
    const canWeld = panelWeld || partWeld;
    this.repairing = canWeld;
    if (canWeld) {
      this.tick += dt;
      if (this.tick >= 0.1) {
        if (partWeld) this.sink.repairPart(t!.ship, t!.index, this.tick);
        else this.sink.repair(t!.ship, t!.index, this.tick);
        this.tick = 0;
      }
      this.weldFx(dt, ctx.hand, t!.point, t!.normal, t!.ship.render.v);
    } else this.tick = 0;
    this.beam.visible = canWeld;
    this.draw(t);
    return t ? this.prompt(t) : null;
  }

  /** Click / E. True when it acted on a control (so the click does not also fire). */
  use(): boolean {
    const t = this.target;
    if (!t || !t.inReach) return false;
    if (t.kind === 'seat') {
      if (this.seated) return false;
      this.sink.sit(t.ship, t.index);
      return true;
    }
    if (t.kind !== 'control') return false;
    // a settled refusal is the amber light (and its buzzer); the click does not also fire or print a line
    if (t.ship.view.isRefused(t.index)) {
      t.ship.sounds.control(t.index);
      t.ship.sounds.control(t.index, true);
      return true;
    }
    t.ship.view.pressed(t.index);
    this.sink.interact(t.ship, t.index);
    return true;
  }

  private weldFx(dt: number, hand: THREE.Vector3, at: THREE.Vector3, n: THREE.Vector3, carry: readonly number[]) {
    const d = at.clone().sub(hand);
    const L = d.length();
    const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 1, 0), d.normalize());
    const flicker = 0.6 + Math.random() * 0.8;
    // drawn straight in render space (the beam's matrixWorld is written by hand)
    this.beam.matrix.compose(origin.toRender(_h.copy(hand)), q, new THREE.Vector3(flicker, L, flicker));
    this.beam.matrixWorld.copy(this.beam.matrix);
    this.sparkT -= dt;
    if (this.sparkT > 0) return;
    this.sparkT = 0.03;
    for (let k = 0; k < 4; k++) {
      this.particles.emit('glow', {
        pos: at,
        vel: new THREE.Vector3().randomDirection().multiplyScalar(0.6 + Math.random() * 2.2).addScaledVector(n, 1.2),
        carry,
        color: Math.random() < 0.5 ? [2.4, 3, 4.5] : [4, 2.6, 1],
        life: 0.1 + Math.random() * 0.35,
        size: 0.012 + Math.random() * 0.018,
        gravity: 1,
      });
    }
    this.particles.emit('glow', { pos: at, vel: n.clone().multiplyScalar(0.05), carry, color: [3, 4, 6], life: 0.05, size: 0.12 + Math.random() * 0.1 });
  }

  private draw(t: Target | null) {
    this.box.visible = this.outline.visible = this.ghost.visible = this.consoleGhosts.visible = false;
    if (!t) return;
    if (!this.tool && (t.kind === 'panel' || t.kind === 'part')) return;
    const M = t.ship.view.root.matrixWorld;
    const refused = t.kind === 'control' && t.ship.view.isRefused(t.index);
    if (t.kind === 'seat') {
      const st = t.ship.sim.def.seats[t.index];
      const f = seatFrame(st, SEAT_PICK);
      const F = new THREE.Matrix4().compose(new THREE.Vector3(...f.c), new THREE.Quaternion().setFromEuler(new THREE.Euler(0, st.yaw, 0)), new THREE.Vector3(f.half[0] * 2, f.half[1] * 2, f.half[2] * 2));
      this.box.matrix.multiplyMatrices(M, F);
      this.box.matrixWorld.copy(this.box.matrix);
      (this.box.material as THREE.LineBasicMaterial).color.set(t.inReach ? 0x4fd8f0 : 0x2a6f80);
      this.box.visible = !this.seated;
      return;
    }
    if (t.kind === 'control') {
      const c = t.ship.sim.def.controls[t.index];
      const lid = c.kind === 'cover' ? coverHit(c, (t.ship.sim.sw[c.key] ?? 0) === 1) : null;
      const frame = lid ? lid.frame : { c: c.c, u: c.u, v: c.v, n: c.n };
      const F = new THREE.Matrix4().makeBasis(new THREE.Vector3(...frame.u), new THREE.Vector3(...frame.v), new THREE.Vector3(...frame.n)).setPosition(...frame.c);
      const size = lid ? [lid.half[0] * 2, lid.half[1] * 2, lid.half[2] * 2] : [c.half[0] * 2.5, c.half[1] * 2.5, c.half[2] * 2];
      const seat = lid ? new THREE.Vector3(0, 0, 0) : new THREE.Vector3(0, 0, c.half[2] * 0.6);
      F.multiply(new THREE.Matrix4().compose(seat, new THREE.Quaternion(), new THREE.Vector3(size[0], size[1], size[2])));
      this.box.matrix.multiplyMatrices(M, F);
      this.box.matrixWorld.copy(this.box.matrix);
      (this.box.material as THREE.LineBasicMaterial).color.set(refused ? 0xffc04a : t.inReach ? 0x4fd8f0 : 0x2a6f80);
      this.box.visible = true;
      return;
    }
    if (t.kind === 'part') {
      const part = t.ship.sim.def.parts[t.index];
      const f = boxFrame(part);
      const F = new THREE.Matrix4().makeBasis(new THREE.Vector3(...f.u), new THREE.Vector3(...f.v), new THREE.Vector3(...f.n)).setPosition(...f.c);
      F.multiply(new THREE.Matrix4().makeScale(part.half[0] * 2, part.half[1] * 2, part.half[2] * 2));
      this.box.matrix.multiplyMatrices(M, F);
      this.box.matrixWorld.copy(this.box.matrix);
      const ratio = t.ship.sim.partHp(part) / part.maxHp;
      (this.box.material as THREE.LineBasicMaterial).color.set(t.inReach ? (ratio > 0.8 ? 0x5cf29a : ratio > 0.5 ? 0xffc04a : 0xff5a4a) : 0x2a6f80);
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
      (this.ghost.material as THREE.MeshBasicMaterial).opacity = 0.22 + 0.35 * (sim.hp[p.index] / SOLID_HP) + (this.repairing ? 0.05 * Math.random() : 0);
      this.ghost.visible = true;
      this.consoleGhost(t.ship, t.index, M);
    }
  }

  /** Translucent copy of every button console that was bolted to this hull plate. */
  private consoleGhost(ship: Target['ship'], panel: number, M: THREE.Matrix4) {
    const key = `${ship.id}:${panel}`;
    if (key !== this.consoleKey) {
      this.consoleKey = key;
      for (const child of [...this.consoleGhosts.children]) {
        const mesh = child as THREE.Mesh;
        mesh.geometry.dispose();
        this.consoleGhosts.remove(mesh);
      }
      const add = (con: ConsoleDef) => {
        const geo = new THREE.BoxGeometry(con.w, con.h, Math.max(0.02, con.depth));
        const F = new THREE.Matrix4().makeBasis(new THREE.Vector3(...con.u), new THREE.Vector3(...con.v), new THREE.Vector3(...con.n)).setPosition(...con.c);
        geo.applyMatrix4(F.multiply(new THREE.Matrix4().makeTranslation(0, 0, -con.depth / 2)));
        this.consoleGhosts.add(new THREE.Mesh(geo, this.consoleMat));
      };
      for (const con of ship.sim.def.consoles) if (con.host === panel) add(con);
    }
    this.consoleGhosts.matrix.copy(M);
    this.consoleGhosts.updateMatrixWorld(true);
    this.consoleGhosts.visible = this.consoleGhosts.children.length > 0;
  }

  private prompt(t: Target): PromptInfo | null {
    const sim = t.ship.sim;
    if (t.kind === 'seat') {
      const st = sim.def.seats[t.index];
      return { title: st.name, state: this.seated ? 'SENTADO' : 'LIBRE', tone: 'on', hint: this.seated ? '[E] / [ESPACIO] levantarse' : t.inReach ? '[E] / [CLIC] sentarse' : 'Acércate para sentarte' };
    }
    if (t.kind === 'control') {
      const c = sim.def.controls[t.index];
      const v = sim.sw[c.key] ?? 0;
      const how = c.action === 'reset' ? '[CLIC] / [E] reconocer' : c.action === 'cycle' ? '[CLIC] avanza · [RUEDA] gira' : '[CLIC] / [E] accionar';
      const hint = !t.inReach ? 'Acércate para accionar' : how;
      const refused = t.ship.view.isRefused(c.index);
      const why = refused ? sim.blocked(c) : null;
      return { title: c.name, state: c.states[v] ?? '', tone: refused ? 'warn' : c.key === 'caution' ? (v ? 'warn' : 'off') : v ? 'on' : 'off', hint, detail: why ?? c.help };
    }
    if (t.kind === 'part' || t.kind === 'panel') {
      if (!this.tool) return null;
    }
    if (t.kind === 'part') {
      const part = sim.def.parts[t.index];
      const hp = sim.partHp(part);
      const ratio = hp / part.maxHp;
      const hint = !t.inReach ? 'Acércate para reparar' : hp >= part.maxHp ? 'Íntegra' : this.repairing ? 'Soldando…' : '[MANTÉN CLIC] soldar';
      return { title: part.name, state: `${Math.round(ratio * 100)} %`, tone: ratio > 0.8 ? 'on' : ratio > 0.5 ? 'warn' : 'bad', hint, detail: part.circuit ? `Circuito ${part.circuit}.` : 'Pieza mecánica, sin circuito.', bar: ratio };
    }
    const p = sim.def.panels[t.index];
    const hp = sim.hp[p.index];
    const r = hp / p.maxHp;
    const state = t.hole ? (hp > 0 ? `RECONSTRUYENDO ${Math.round((hp / SOLID_HP) * 100)}%` : 'DESTRUIDO') : `${Math.round(r * 100)}%`;
    const hint = !t.inReach ? 'Acércate para reparar' : hp >= p.maxHp ? 'Íntegro' : this.repairing ? 'Soldando…' : '[MANTÉN CLIC] soldar';
    return {
      title: `Panel ${p.id} · ${KIND[p.kind]}`,
      state,
      tone: t.hole ? 'bad' : r > 0.8 ? 'on' : r > 0.5 ? 'warn' : 'bad',
      hint,
      bar: t.hole ? hp / SOLID_HP : r,
    };
  }
}
