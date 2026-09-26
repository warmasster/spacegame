import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import type { Debris } from '../../engine/debris';
import { SHIP_DEFS, ShipSim, placeShip, type ShipSnapshot } from '../../shared/ship/sim';
import { rayBox, rayPrism, type V3 } from '../../shared/ship/geom';
import { SEAT_PICK, boxFrame, controlHit, seatFrame, shutCovers } from '../../shared/ship/def';
import { zoneAt } from '../../shared/ship/crew';
import type { Particles } from '../fx/particles';
import type { Physics } from '../world/physics';
import { ShipCargo } from './cargo';
import { ShipPhysics } from './physics';
import type { ShipAnimState } from './screens';
import { ShipView } from './view';

export interface ShipHit {
  kind: 'control' | 'panel' | 'seat' | 'part';
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
  /** Authority's travel per mover and how long it has not changed (s). */
  private authMv = new Map<string, { v: number; still: number }>();

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
    // late joiners see things where they already are (the snapshot carries the travel), no replay
    this.anim = { movers: Object.fromEntries(def.movers.map((m) => [m.key, this.sim.mover(m.key)])) };
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

  /**
   * Fixed step: doors / ramp / shutters / gear. The travel is predicted every fixed step with the
   * mover's own rule (toward its switch at its rate while its circuit is powered), so it is as
   * smooth as the physics. The authority's travel (replicated `mv.*`, a few updates a second and
   * always a little behind) only corrects it when they really disagree: the authority has stopped
   * somewhere else (an obstruction sensor held a door) or is far off.
   */
  fixed(dt: number) {
    const sim = this.sim;
    for (const m of sim.def.movers) {
      let cur = this.anim.movers[m.key] ?? 0;
      const target = sim.sw[m.key] ?? 0;
      if (sim.powered(m.circuit) && cur !== target) {
        const k = m.rate * dt;
        cur = target > cur ? Math.min(target, cur + k) : Math.max(target, cur - k);
      }
      // authority: how long has it been still?
      const auth = sim.mover(m.key);
      const t = this.authMv.get(m.key);
      if (!t || Math.abs(t.v - auth) > 1e-4) this.authMv.set(m.key, { v: auth, still: 0 });
      else t.still += dt;
      const still = this.authMv.get(m.key)!.still;
      // the authority always trails a little while moving: only a stop elsewhere or a big gap counts
      const err = auth - cur;
      if ((still > 0.35 && Math.abs(err) > 0.01) || Math.abs(err) > 0.5) cur += err * Math.min(1, dt * 6);
      this.anim.movers[m.key] = cur;
    }
    const ramp = sim.def.ramp;
    this.physics.update(this.anim, ramp ? this.view.rampPhi(smooth(this.anim.movers[ramp.key] ?? 0)) : 0);
  }

  /** Once per frame: visuals and ambient effects. */
  frame(dt: number) {
    this.time += dt;
    this.cargo.sync();
    const ramp = this.sim.def.ramp;
    this.view.update(dt, this.time, ramp ? { movers: { ...this.anim.movers, [ramp.key]: smooth(this.anim.movers[ramp.key] ?? 0) } } : this.anim);
    // damaged panels spit sparks now and then; cut conduits arc
    this.sparkT -= dt;
    if (this.sparkT > 0) return;
    this.sparkT = 0.08;
    const sim = this.sim;
    const M = this.view.root.matrixWorld;
    const grid = sim.sys.power;
    const live = !!grid && sim.st[grid.iLive] === 1;
    for (const p of sim.def.panels) {
      const r = sim.hp[p.index] / p.maxHp;
      // a cut conduit arcs while its breaker is closed and something feeds the grid
      const cut = sim.hole(p.index) && live && p.conduits.some((c) => sim.sw[sim.def.subsystems.find((s) => s.id === c)!.breaker] === 1);
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
  pick(origin: THREE.Vector3, dir: THREE.Vector3, max: number, ignoreSeats = false): ShipHit | null {
    const sim = this.sim;
    const def = sim.def;
    const occ = this.physics.castRay(origin, dir, max, undefined, ignoreSeats);
    const tOcc = occ ? occ.t : max;
    const o = this.local(origin, _o);
    const d = _d.copy(dir).transformDirection(this.toShip);
    const O: V3 = [o.x, o.y, o.z];
    const D: V3 = [d.x, d.y, d.z];
    let best: ShipHit | null = null;
    const M = this.view.root.matrixWorld;
    const world = (t: number) => origin.clone().addScaledVector(dir, t);
    const shut = shutCovers(def.controls, sim.sw);
    for (const c of def.controls) {
      if (c.host >= 0 && sim.hole(c.host)) continue;
      const hit = controlHit(c, sim.sw, shut);
      if (!hit) continue;
      const t = rayBox(hit.frame, hit.half, O, D, max);
      if (t < 0 || t > tOcc + 0.04 || (best && t >= best.dist)) continue;
      best = { kind: 'control', index: c.index, hole: false, point: world(t), normal: new THREE.Vector3(...c.n).transformDirection(M), dist: t };
    }
    for (let i = 0; i < (ignoreSeats ? 0 : def.seats.length); i++) {
      // hit box around the whole seat (pan, wings, headrest)
      const f = seatFrame(def.seats[i], SEAT_PICK);
      const t = rayBox(f, f.half, O, D, max);
      if (t < 0 || t > tOcc + 0.05 || (best && t >= best.dist)) continue;
      best = { kind: 'seat', index: i, hole: false, point: world(t), normal: new THREE.Vector3(0, 1, 0), dist: t };
    }
    for (const part of def.parts) {
      if (part.shape === 'none') continue;
      const t = rayBox(boxFrame(part), part.half, O, D, max);
      if (t < 0 || t > tOcc + 0.08 || (best && t >= best.dist)) continue;
      best = { kind: 'part', index: part.index, hole: false, point: world(t), normal: new THREE.Vector3(0, 1, 0), dist: t };
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
    return zoneAt(this.sim.def, [l.x, l.y, l.z]);
  }

  /** Centre of the ship in world space (compass marker). */
  get position() {
    return this.view.root.position;
  }
}

const smooth = (t: number) => t * t * (3 - 2 * t);
