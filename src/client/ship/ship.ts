import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import type { Debris } from '../../engine/debris';
import { SHIP_DEFS, ShipSim, placeShip, type ShipSnapshot } from '../../shared/ship/sim';
import { rayBox, rayPrism, type V3 } from '../../shared/ship/geom';
import { SEAT_PICK, seatFrame, type SubsystemId } from '../../shared/ship/def';
import type { Particles } from '../fx/particles';
import type { Physics } from '../world/physics';
import { ShipCargo } from './cargo';
import { ShipPhysics } from './physics';
import type { ShipAnimState } from './screens';
import { ShipView } from './view';

/** Travel speeds (fraction per second) and the bus each mover needs. */
const MOVERS: Record<string, { rate: number; bus: SubsystemId }> = {
  door: { rate: 1 / 0.9, bus: 'doors' },
  ramp: { rate: 1 / 4.5, bus: 'hyd' },
  shield: { rate: 1 / 2.4, bus: 'shield' },
};

export interface ShipHit {
  kind: 'control' | 'panel' | 'seat';
  index: number;
  /** Panel is a hole (target for rebuilding). */
  hole: boolean;
  point: THREE.Vector3;
  normal: THREE.Vector3;
  dist: number;
}

export interface ShipDeps {
  physics: Physics;
  csm: CSM | null;
  ground: { height(x: number, z: number): number };
  particles: Particles;
  debris: Debris;
  gravity: number;
}

const _o = new THREE.Vector3();
const _d = new THREE.Vector3();

/**
 * Client side of one ship: mirrors the server's state (ShipSim), animates what moves, keeps the
 * physics in sync and turns state changes into effects (breach debris, sparks, heat).
 */
export class ShipClient {
  readonly sim: ShipSim;
  readonly view: ShipView;
  readonly physics: ShipPhysics;
  readonly anim: ShipAnimState;
  readonly cargo: ShipCargo;
  private time = 0;
  private sparkT = 0;
  private toShip = new THREE.Matrix4();

  constructor(
    snap: ShipSnapshot,
    private deps: ShipDeps,
  ) {
    const def = SHIP_DEFS[snap.def];
    if (!def) throw new Error(`unknown ship "${snap.def}"`);
    this.sim = new ShipSim(snap.id, def, placeShip(def, snap.x, snap.z, snap.yaw, deps.ground), deps.ground, snap);
    const ground = (x: number, z: number) => deps.ground.height(x, z);
    this.view = new ShipView(this.sim, deps.csm, ground);
    this.physics = new ShipPhysics(deps.physics, this.sim, ground);
    const m = this.view.mats;
    this.cargo = new ShipCargo(deps.physics, def.cargo, this.view.root.matrixWorld, snap.yaw, deps.gravity, { orange: m.crate, grey: m.crate2, strap: m.dark });
    const sw = this.sim.sw;
    // late joiners see things where they already are, no replayed travel
    this.anim = { doors: Object.fromEntries(def.doors.map((d) => [d.key, sw[d.key] ?? 0])), ramp: sw[def.ramp.key] ?? 0, shield: sw[def.shield.key] ?? 0 };
    this.toShip.copy(this.view.root.matrixWorld).invert();
  }

  get id() {
    return this.sim.id;
  }

  /** World → ship space. */
  local(p: THREE.Vector3, out = new THREE.Vector3()) {
    return out.copy(p).applyMatrix4(this.toShip);
  }

  /** Server update. Returns panels that were blown out (for toasts). */
  apply(sw?: Record<string, number>, hp?: Array<[number, number]>) {
    const before = hp?.map(([i]) => this.sim.hp[i]) ?? [];
    const flipped = this.sim.apply(sw, hp);
    const blown: number[] = [];
    hp?.forEach(([i, v], k) => {
      if (v < before[k] - 0.5) {
        this.view.heat[i] = Math.min(1, this.view.heat[i] + (before[k] - v) / 60);
        if (!this.sim.hole(i)) this.view.writePanel(i);
      } else if (!flipped.includes(i)) this.view.writePanel(i);
    });
    if (flipped.length) {
      this.physics.syncPanels();
      this.cargo.wake();
      this.view.rebuildPanels();
      for (const i of flipped) {
        if (!this.sim.hole(i)) continue;
        blown.push(i);
        this.breachFx(i);
      }
    }
    return blown;
  }

  private breachFx(i: number) {
    const p = this.sim.def.panels[i];
    const c = new THREE.Vector3(...p.c).applyMatrix4(this.view.root.matrixWorld);
    const n = new THREE.Vector3(...p.n).transformDirection(this.view.root.matrixWorld);
    const glass = p.kind === 'glass';
    this.deps.debris.burst(c, glass ? 10 : 7, { dir: n, speed: glass ? 0.35 : 0.5, size: glass ? 0.35 : 0.9, spread: 0.9 });
    for (let k = 0; k < 40; k++) {
      this.deps.particles.emit('glow', {
        pos: c.clone().add(new THREE.Vector3().randomDirection().multiplyScalar(0.4)),
        vel: new THREE.Vector3().randomDirection().multiplyScalar(2 + Math.random() * 5).addScaledVector(n, 3),
        color: glass ? [1.5, 2, 2.6] : [4, 2, 0.6],
        life: 0.2 + Math.random() * 0.6,
        size: 0.03 + Math.random() * 0.04,
        gravity: 1.62,
      });
    }
  }

  /** Obstruction: an astronaut standing where a door or the ramp would close. */
  private blocked(key: string, bodies: THREE.Vector3[]) {
    const def = this.sim.def;
    const l = new THREE.Vector3();
    for (const b of bodies) {
      this.local(b, l);
      if (key === def.ramp.key) {
        if (Math.abs(l.x) < def.ramp.w / 2 + 0.3 && l.z > def.ramp.hinge[2] - 0.45 && l.z < def.ramp.hinge[2] + def.ramp.length + 0.4 && l.y < 0.6) return true;
        continue;
      }
      const d = def.doors.find((x) => x.key === key);
      if (d && Math.abs(l.x - d.c[0]) < d.w / 2 + 0.3 && Math.abs(l.z - d.c[2]) < 0.55 && l.y > -0.5 && l.y < d.h) return true;
    }
    return false;
  }

  /** Fixed step: move doors / ramp / shutters toward their targets while powered. */
  fixed(dt: number, bodies: THREE.Vector3[]) {
    const sim = this.sim;
    const def = sim.def;
    const step = (cur: number, target: number, kind: keyof typeof MOVERS, key: string) => {
      if (cur === target || !sim.powered(MOVERS[kind].bus)) return cur;
      // obstruction sensor: hold the door open, stop the ramp
      if (target < cur && this.blocked(key, bodies)) return cur;
      const k = MOVERS[kind].rate * dt;
      return target > cur ? Math.min(target, cur + k) : Math.max(target, cur - k);
    };
    for (const d of def.doors) this.anim.doors[d.key] = step(this.anim.doors[d.key], sim.sw[d.key] ?? 0, 'door', d.key);
    this.anim.ramp = step(this.anim.ramp, sim.sw[def.ramp.key] ?? 0, 'ramp', def.ramp.key);
    this.anim.shield = step(this.anim.shield, sim.sw[def.shield.key] ?? 0, 'shield', def.shield.key);
    this.physics.update(this.anim, this.view.rampPhi(smooth(this.anim.ramp)));
  }

  /** Once per frame: visuals and ambient effects. */
  frame(dt: number) {
    this.time += dt;
    this.cargo.sync();
    this.view.update(dt, this.time, { ...this.anim, ramp: smooth(this.anim.ramp) });
    // damaged panels spit sparks now and then; cut conduits arc
    this.sparkT -= dt;
    if (this.sparkT > 0) return;
    this.sparkT = 0.08;
    const sim = this.sim;
    const M = this.view.root.matrixWorld;
    for (const p of sim.def.panels) {
      const r = sim.hp[p.index] / p.maxHp;
      const cut = sim.hole(p.index) && p.conduits.length && sim.sw.reactor === 1;
      if (!cut && (sim.hole(p.index) || r > 0.55)) continue;
      if (Math.random() > (cut ? 0.35 : 0.08)) continue;
      const q = p.poly[Math.floor(Math.random() * p.poly.length)];
      const t = Math.random();
      const e = p.poly[(p.poly.indexOf(q) + 1) % p.poly.length];
      const x = q[0] + (e[0] - q[0]) * t;
      const y = q[1] + (e[1] - q[1]) * t;
      const depth = cut ? -0.18 : 0;
      const at = new THREE.Vector3(p.c[0] + p.u[0] * x * 0.8 + p.v[0] * y * 0.8 + p.n[0] * depth, p.c[1] + p.u[1] * x * 0.8 + p.v[1] * y * 0.8 + p.n[1] * depth, p.c[2] + p.u[2] * x * 0.8 + p.v[2] * y * 0.8 + p.n[2] * depth).applyMatrix4(M);
      const n = new THREE.Vector3(...p.n).transformDirection(M);
      for (let k = 0; k < (cut ? 10 : 6); k++) {
        this.deps.particles.emit('glow', {
          pos: at,
          vel: new THREE.Vector3().randomDirection().multiplyScalar(0.8 + Math.random() * 2.5).addScaledVector(n, Math.random() < 0.5 ? 1 : -1),
          color: cut ? [2.2, 2.6, 4] : [4, 2.2, 0.7],
          life: 0.12 + Math.random() * 0.35,
          size: 0.018 + Math.random() * 0.02,
          gravity: 1.62,
        });
      }
    }
  }

  /**
   * What the crosshair ray hits on this ship: a control (within its hit box, in front of any
   * surface), a solid panel, or a blown-out panel's footprint (for rebuilding).
   */
  pick(origin: THREE.Vector3, dir: THREE.Vector3, max: number): ShipHit | null {
    const sim = this.sim;
    const def = sim.def;
    const occ = this.physics.castRay(origin, dir, max);
    const tOcc = occ ? occ.t : max;
    const o = this.local(origin, _o);
    const d = _d.copy(dir).transformDirection(this.toShip);
    const O: V3 = [o.x, o.y, o.z];
    const D: V3 = [d.x, d.y, d.z];
    let best: ShipHit | null = null;
    const M = this.view.root.matrixWorld;
    const world = (t: number) => origin.clone().addScaledVector(dir, t);
    for (const c of def.controls) {
      if (c.host >= 0 && sim.hole(c.host)) continue;
      const half: V3 = [c.half[0] * 1.5 + 0.01, c.half[1] * 1.5 + 0.01, c.half[2] + 0.02];
      const t = rayBox({ c: [c.c[0] + c.n[0] * c.half[2] * 0.5, c.c[1] + c.n[1] * c.half[2] * 0.5, c.c[2] + c.n[2] * c.half[2] * 0.5], u: c.u, v: c.v, n: c.n }, half, O, D, max);
      if (t < 0 || t > tOcc + 0.04 || (best && t >= best.dist)) continue;
      best = { kind: 'control', index: c.index, hole: false, point: world(t), normal: new THREE.Vector3(...c.n).transformDirection(M), dist: t };
    }
    for (let i = 0; i < def.seats.length; i++) {
      // hit box around the whole seat (pan, wings, headrest)
      const f = seatFrame(def.seats[i], SEAT_PICK);
      const t = rayBox(f, f.half, O, D, max);
      if (t < 0 || t > tOcc + 0.05 || (best && t >= best.dist)) continue;
      best = { kind: 'seat', index: i, hole: false, point: world(t), normal: new THREE.Vector3(0, 1, 0), dist: t };
    }
    if (best) return best;
    if (occ && occ.panel >= 0) {
      const p = def.panels[occ.panel];
      const n = new THREE.Vector3(...p.n).transformDirection(M);
      if (n.dot(dir) > 0) n.negate();
      best = { kind: 'panel', index: occ.panel, hole: false, point: world(occ.t), normal: n, dist: occ.t };
    }
    for (const p of def.panels) {
      if (!sim.hole(p.index)) continue;
      const t = rayPrism(p, p.poly, p.t / 2 + 0.03, O, D, max);
      if (t < 0 || t > tOcc || (best && t >= best.dist)) continue;
      const n = new THREE.Vector3(...p.n).transformDirection(M);
      if (n.dot(dir) > 0) n.negate();
      best = { kind: 'panel', index: p.index, hole: true, point: world(t), normal: n, dist: t };
    }
    return best;
  }

  /** Rocket sweep a → b against the ship (world). */
  segmentHit(a: THREE.Vector3, b: THREE.Vector3): THREE.Vector3 | null {
    const d = b.clone().sub(a);
    const len = d.length();
    if (len < 1e-6) return null;
    d.divideScalar(len);
    const h = this.physics.castRay(a, d, len, (handle) => this.cargo.handles.has(handle));
    return h ? a.clone().addScaledVector(d, h.t) : null;
  }

  /** Third-person camera: distance along from→to before a ship surface (or null). */
  occlude(from: THREE.Vector3, to: THREE.Vector3): number | null {
    const d = to.clone().sub(from);
    const len = d.length();
    if (len < 1e-4) return null;
    const h = this.physics.castRay(from, d.divideScalar(len), len);
    return h ? h.t : null;
  }

  /** Seat pose in the world: root position (feet) and body yaw. */
  seatPose(i: number) {
    const st = this.sim.def.seats[i];
    return { pos: new THREE.Vector3(...st.root).applyMatrix4(this.view.root.matrixWorld), yaw: this.sim.place.yaw + st.yaw, exit: new THREE.Vector3(...st.exit).applyMatrix4(this.view.root.matrixWorld) };
  }

  /** Compartment containing a world point, if any. */
  zoneAt(p: THREE.Vector3) {
    const l = this.local(p);
    return this.sim.def.zones.find((z) => l.x >= z.min[0] && l.x <= z.max[0] && l.y >= z.min[1] - 0.2 && l.y <= z.max[1] && l.z >= z.min[2] && l.z <= z.max[2]) ?? null;
  }

  /** Centre of the ship in world space (compass marker). */
  get position() {
    return this.view.root.position;
  }
}

const smooth = (t: number) => t * t * (3 - 2 * t);
