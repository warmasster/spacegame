import * as THREE from 'three';
import { AIR, airAccel, airPush, panelLoad, panelStrain, type Vent } from '../../shared/ship/airflow';
import { WORLD_FRAME } from '../../shared/ship/crates';
import { facing, zoneAtPoint } from '../../shared/ship/def';
import { qRotate, type V3 } from '../../shared/ship/geom';
import type { Particles } from '../fx/particles';
import type { Frames } from '../frames/frames';
import type { ShipClient } from './ship';

/** Plumes of a ship reach this far (m): beyond it the moon's bodies are left alone. */
const PLUME_REACH = 30;
/** A breach with less than this across it (kPa) just opens; more, and it goes off. */
const BANG_KPA = 8;

const _p = new THREE.Vector3();
const _d = new THREE.Vector3();
const _v = new THREE.Vector3();
const _e1 = new THREE.Vector3();
const _e2 = new THREE.Vector3();

interface ShipAir {
  vents: Vent[];
  /** Which panels were holes on the last frame (a new one with air behind it goes off). */
  holes: boolean[];
  /** `<compartment>.shock` variable of each compartment (-1: none). */
  shock: number[];
  /** Emission left over from the last frame, per vent / per creaking panel. */
  carry: Map<string, number>;
  /** Explosive decompressions going on: the room that blew (compartment index) and when (s). */
  bursts: Array<{ comp: number; t0: number; k: number }>;
}

/**
 * The air of a decompressing ship on this client, from the replicated state (shared/ship/airflow.ts):
 *
 *   fixed step — what each ship's air is doing (its vents), the pull on a body anywhere (the local
 *                astronaut, the crates), the plumes outside that throw EVA crew around;
 *   frame      — what it looks like: vapour and ice pouring out of each breach, dust racing to it
 *                inside, the cabin fogging as the air left behind cools, a panel under too much
 *                pressure creaking and hissing, the bang when one goes, and the shake.
 */
export class AirFlow {
  private air = new Map<number, ShipAir>();

  constructor(
    private frames: Frames,
    private ships: () => ShipClient[],
    private particles: Particles,
  ) {}

  private state(ship: ShipClient): ShipAir {
    let a = this.air.get(ship.id);
    if (!a) {
      const sim = ship.sim;
      a = {
        vents: [],
        holes: sim.def.panels.map((p) => sim.hole(p.index)),
        shock: sim.def.compartments.map((c) => (sim.vars.has(`${c.id}.shock`) ? sim.vars.idx(`${c.id}.shock`) : -1)),
        carry: new Map(),
        bursts: [],
      };
      this.air.set(ship.id, a);
    }
    return a;
  }

  /** Fixed step, before anything is pushed: what the air of every ship is doing now. */
  step() {
    for (const ship of this.ships()) ship.sim.vents(this.state(ship).vents);
  }

  /**
   * Acceleration (m/s², frame coordinates) the air gives a body at `p` (frame coordinates) with drag
   * area per mass `cda`, or null where the air is still.
   */
  accel(fr: number, p: V3, cda: number): V3 | null {
    if (fr !== WORLD_FRAME) {
      const ship = this.frames.ship(fr);
      if (!ship) return null;
      return this.shipAccel(ship, p, cda);
    }
    // outside every ship (the physics bubble): the plumes of the ships around
    let out: V3 | null = null;
    let pw: V3 | null = null;
    for (const ship of this.ships()) {
      // no air moving on that ship (the usual case): nothing to transform
      if (!this.air.get(ship.id)?.vents.length) continue;
      pw ??= this.frames.toWorld(WORLD_FRAME, p);
      const l = ship.sim.toLocal(pw);
      if (Math.hypot(l[0], l[1], l[2]) > PLUME_REACH) continue;
      const a = this.shipAccel(ship, l, cda);
      if (!a) continue;
      const w = qRotate(ship.sim.pose.q, a);
      out = out ? [out[0] + w[0], out[1] + w[1], out[2] + w[2]] : w;
    }
    return out && this.frames.dirToLocal(WORLD_FRAME, out);
  }

  /** The pull of one ship's air at a ship-space point (ship axes). */
  private shipAccel(ship: ShipClient, l: V3, cda: number): V3 | null {
    const a = this.air.get(ship.id);
    const vents = a?.vents;
    if (!a || !vents?.length) return null;
    const def = ship.sim.def;
    const zone = zoneAtPoint(def.zones, l);
    const comp = zone ? ship.sim.sys.compIndex(zone.id) : -1;
    const q = airPush(def, vents, l, comp);
    if (q[0] === 0 && q[1] === 0 && q[2] === 0) return null;
    // the room that just blew: the slug of air goes all at once
    if (a.bursts.length) {
      const now = performance.now() / 1000;
      let gain = 1;
      for (let i = a.bursts.length - 1; i >= 0; i--) {
        const b = a.bursts[i];
        const f = 1 - (now - b.t0) / AIR.burstS;
        if (f <= 0) {
          a.bursts.splice(i, 1);
          continue;
        }
        if (b.comp === comp) gain = Math.max(gain, 1 + AIR.burst * b.k * f);
      }
      q[0] *= gain;
      q[1] *= gain;
      q[2] *= gain;
    }
    return airAccel(q, cda);
  }

  /**
   * Once per rendered frame: the effects. `eye` = the local astronaut's eye (world). Returns how
   * much the view should shake (0..1).
   */
  frame(dt: number, eye: THREE.Vector3): number {
    let shake = 0;
    for (const ship of this.ships()) {
      const a = this.state(ship);
      const sim = ship.sim;
      const M = ship.worldMatrix;
      const myZone = ship.zoneAt(eye);
      const myComp = myZone ? sim.sys.compIndex(myZone.id) : -1;
      // a panel that just went with air behind it goes off
      for (const p of sim.def.panels) {
        const hole = sim.hole(p.index);
        if (hole && !a.holes[p.index]) {
          const dp = panelLoad(sim.sys, sim.st, p);
          if (dp > BANG_KPA) shake += this.bang(ship, p.index, dp, eye);
        }
        a.holes[p.index] = hole;
      }
      for (const v of a.vents) this.jet(ship, a, v, dt);
      // the air left behind cools as it expands: the room fogs, and whoever is in it feels the blast
      for (let i = 0; i < a.shock.length; i++) {
        const k = a.shock[i] < 0 ? 0 : sim.st[a.shock[i]];
        if (k <= 0.02) continue;
        this.fog(ship, a, i, k, dt);
        if (i === myComp) shake += k * dt * 3;
      }
      // panels past what they hold creak and spit
      for (const p of sim.def.panels) {
        if (sim.hole(p.index)) continue;
        const strain = panelStrain(sim.sys, sim.st, p, sim.hp[p.index]);
        if (strain <= 0) continue;
        const key = `creak${p.index}`;
        const n = (a.carry.get(key) ?? 0) + dt * (6 + strain * 20);
        a.carry.set(key, n % 1);
        if (n < 1) continue;
        _p.set(p.c[0], p.c[1], p.c[2]).applyMatrix4(M);
        const d = _p.distanceTo(eye);
        if (d < 6) shake += (0.04 + 0.08 * Math.min(1, strain)) * (1 - d / 6);
        this.creak(ship, p.index);
      }
    }
    return Math.min(1, shake);
  }

  /** Vapour and ice crystals out of one opening (and, inside, dust racing to it). */
  private jet(ship: ShipClient, a: ShipAir, v: Vent, dt: number) {
    const M = ship.worldMatrix;
    const key = v.panel >= 0 ? `p${v.panel}` : `o${v.at.join(',')}`;
    const rate = Math.min(500, 40 * v.mdot ** 0.6) * v.fade;
    const n = (a.carry.get(key) ?? 0) + rate * dt;
    const count = Math.floor(n);
    a.carry.set(key, n - count);
    if (!count) return;
    const vac = v.down < 0;
    const at = _p.set(v.at[0], v.at[1], v.at[2]).applyMatrix4(M).clone();
    const dir = _d.set(v.dir[0], v.dir[1], v.dir[2]).transformDirection(M).clone();
    basis(dir);
    const vs = vac ? 4 + Math.min(30, v.speed * 0.08) : 1.5 + Math.min(10, v.speed * 0.03);
    const spread = vac ? 0.5 : 0.25;
    const size = Math.max(0.4, Math.min(1.4, v.r0 * 1.5));
    const rnd = Math.random;
    for (let k = 0; k < count; k++) {
      const ang = rnd() * Math.PI * 2;
      const rad = Math.sqrt(rnd()) * v.r0 * 0.8;
      const pos = at.clone().addScaledVector(_e1, Math.cos(ang) * rad).addScaledVector(_e2, Math.sin(ang) * rad).addScaledVector(dir, 0.05);
      const vel = _v
        .copy(dir)
        .addScaledVector(_e1, (rnd() - 0.5) * 2 * spread)
        .addScaledVector(_e2, (rnd() - 0.5) * 2 * spread)
        .multiplyScalar(vs * (0.6 + 0.8 * rnd()));
      if (rnd() < 0.15) {
        // ice crystals catching the light
        this.particles.emit('glow', { carry: ship.render.v, pos, vel, color: [0.55, 0.62, 0.75], life: 0.3 + rnd() * 0.8, size: 0.02 });
      } else {
        const g = 0.5 + rnd() * 0.25;
        this.particles.emit('dust', { carry: ship.render.v, pos, vel, color: [g, g * 1.03, g * 1.08], life: vac ? 0.5 + rnd() * 1.0 : 0.3 + rnd() * 0.6, size: (0.05 + rnd() * 0.22) * size });
      }
    }
    // inside: dust and scraps racing to the opening across the room
    if (v.mdot < 0.5) return;
    const inCount = Math.ceil(count * 0.35);
    const back = _v.copy(dir).negate().clone();
    for (let k = 0; k < inCount; k++) {
      const r = 0.4 + rnd() * 2.6;
      const from = at
        .clone()
        .addScaledVector(back, r)
        .addScaledVector(_e1, (rnd() - 0.5) * r * 1.2)
        .addScaledVector(_e2, (rnd() - 0.5) * r * 1.2);
      if (!ship.zoneAt(from)) continue;
      const to = at.clone().sub(from);
      const dist = to.length();
      const speed = Math.min(14, 2 + (v.speed * v.r0 * v.r0) / Math.max(0.2, dist * dist) * 0.25);
      const g = 0.28 + rnd() * 0.15;
      this.particles.emit('dust', { carry: ship.render.v, pos: from, vel: to.multiplyScalar(speed / Math.max(dist, 1e-3)), color: [g, g, g * 1.02], life: Math.min(1.5, dist / speed + 0.1), size: 0.015 + rnd() * 0.035 });
    }
  }

  /** Condensation fog through a room decompressing violently (k = its shock 0..1). */
  private fog(ship: ShipClient, a: ShipAir, comp: number, k: number, dt: number) {
    const sim = ship.sim;
    const c = sim.def.compartments[comp];
    const z = sim.def.zones.find((x) => x.id === c.id);
    if (!z) return;
    const key = `fog${comp}`;
    const n = (a.carry.get(key) ?? 0) + Math.min(400, 150 * k * Math.sqrt(c.volume / 20)) * dt;
    const count = Math.floor(n);
    a.carry.set(key, n - count);
    const M = ship.worldMatrix;
    // it drifts toward the openings the room is emptying through
    const out = a.vents.filter((v) => v.up === comp);
    const rnd = Math.random;
    for (let i = 0; i < count; i++) {
      const l: V3 = [z.min[0] + rnd() * (z.max[0] - z.min[0]), z.min[1] + rnd() * (z.max[1] - z.min[1]), z.min[2] + rnd() * (z.max[2] - z.min[2])];
      // zones nest (a lock inside the engine room, cabins over a double-height hold): only this room's air
      if (zoneAtPoint(sim.def.zones, l)?.id !== c.id) continue;
      const pos = new THREE.Vector3(l[0], l[1], l[2]).applyMatrix4(M);
      const vel = new THREE.Vector3((rnd() - 0.5) * 0.4, (rnd() - 0.5) * 0.3, (rnd() - 0.5) * 0.4);
      if (out.length) {
        const v = out[Math.floor(rnd() * out.length)];
        const d = new THREE.Vector3(v.at[0] - l[0], v.at[1] - l[1], v.at[2] - l[2]);
        const dist = d.length();
        vel.add(d.transformDirection(M).multiplyScalar(Math.min(6, 1 + 6 / Math.max(0.5, dist)) * (0.5 + rnd())));
      } else vel.transformDirection(M).multiplyScalar(0.4);
      const g = 0.42 + rnd() * 0.14;
      this.particles.emit('dust', { carry: ship.render.v, pos, vel, color: [g, g * 1.02, g * 1.05], life: 0.6 + rnd() * 1.0, size: 0.2 + rnd() * 0.4 });
    }
  }

  /** A panel tearing under pressure: flakes and a hiss from its edge, now and then a spark. */
  private creak(ship: ShipClient, index: number) {
    const p = ship.sim.def.panels[index];
    const M = ship.worldMatrix;
    const rnd = Math.random;
    const q = p.poly[Math.floor(rnd() * p.poly.length)];
    const e = p.poly[(p.poly.indexOf(q) + 1) % p.poly.length];
    const t = rnd();
    const x = (q[0] + (e[0] - q[0]) * t) * 0.9;
    const y = (q[1] + (e[1] - q[1]) * t) * 0.9;
    const at = new THREE.Vector3(p.c[0] + p.u[0] * x + p.v[0] * y, p.c[1] + p.u[1] * x + p.v[1] * y, p.c[2] + p.u[2] * x + p.v[2] * y).applyMatrix4(M);
    const n = new THREE.Vector3(p.n[0], p.n[1], p.n[2]).transformDirection(M);
    for (let k = 0; k < 4; k++) {
      const g = 0.4 + rnd() * 0.2;
      this.particles.emit('dust', { carry: ship.render.v, pos: at, vel: n.clone().multiplyScalar(0.5 + rnd() * 2).add(new THREE.Vector3().randomDirection().multiplyScalar(0.6)), color: [g, g, g], life: 0.2 + rnd() * 0.4, size: 0.02 + rnd() * 0.05, gravity: 1 });
    }
    if (rnd() < 0.35) {
      for (let k = 0; k < 3; k++) {
        this.particles.emit('glow', { carry: ship.render.v, pos: at, vel: new THREE.Vector3().randomDirection().multiplyScalar(1 + rnd() * 2), color: [4, 2.2, 0.7], life: 0.1 + rnd() * 0.25, size: 0.015, gravity: 1 });
      }
    }
  }

  /** A panel blown out by the pressure behind it: a burst of vapour out of the hole. Returns the shake. */
  private bang(ship: ShipClient, index: number, dp: number, eye: THREE.Vector3): number {
    const p = ship.sim.def.panels[index];
    const vent = this.air.get(ship.id)?.vents.find((v) => v.panel === index);
    const M = ship.worldMatrix;
    const at = new THREE.Vector3(p.c[0], p.c[1], p.c[2]).applyMatrix4(M);
    // out of the room it held (the panel's normal, the way its gas goes)
    const dirL = vent?.dir ?? facing(ship.sim.def.zones, p.zone, p.other ?? null, p.c, p.n);
    const dir = new THREE.Vector3(dirL[0], dirL[1], dirL[2]).transformDirection(M);
    basis(dir);
    const k = Math.min(1.5, dp / 70);
    // the room it held gets the burst (the flow's upstream side)
    const comp = vent ? vent.up : ship.sim.sys.compIndex(p.zone);
    if (comp >= 0) this.state(ship).bursts.push({ comp, t0: performance.now() / 1000, k: Math.min(1, k) });
    const rnd = Math.random;
    for (let i = 0; i < 160 * k; i++) {
      const vel = dir
        .clone()
        .addScaledVector(_e1, (rnd() - 0.5) * 1.6)
        .addScaledVector(_e2, (rnd() - 0.5) * 1.6)
        .multiplyScalar((6 + rnd() * 30) * k);
      const g = 0.55 + rnd() * 0.3;
      this.particles.emit('dust', { carry: ship.render.v, pos: at.clone().addScaledVector(_e1, (rnd() - 0.5) * 0.8).addScaledVector(_e2, (rnd() - 0.5) * 0.8), vel, color: [g, g * 1.03, g * 1.08], life: 0.4 + rnd() * 1.2, size: 0.1 + rnd() * 0.45 });
    }
    return Math.max(0, 1 - at.distanceTo(eye) / 14) * k;
  }
}

/** Two unit vectors across `d` (into _e1, _e2). */
function basis(d: THREE.Vector3) {
  _e1.set(0, 1, 0);
  if (Math.abs(d.y) > 0.9) _e1.set(1, 0, 0);
  _e1.cross(d).normalize();
  _e2.crossVectors(d, _e1).normalize();
}

