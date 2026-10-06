import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import type RAPIER from '@dimforge/rapier3d-compat';
import type { Debris } from '../../engine/debris';
import { SHIP_DEFS, ShipSim, type ShipSnapshot } from '../../shared/ship/sim';
import { panelLoad } from '../../shared/ship/airflow';
import { qConj, qMul, rayBox, rayPrism, type V3 } from '../../shared/ship/geom';
import { SEAT_PICK, boxFrame, controlHit, doorAxis, facing, seatFrame, shutCovers } from '../../shared/ship/def';
import { zoneAt } from '../../shared/ship/crew';
import { bodyAt, type Surfaces } from '../../shared/space/body';
import { clonePose, copyPose, lerpPose, toLocal, type ShipPose } from '../../shared/ship/flight';
import { inShip, shipReach, type FrameHost } from '../../shared/frames';
import type { Particles } from '../fx/particles';
import { ShipSpace } from '../frames/shipSpace';
import type { Frames } from '../frames/frames';
import { PosePlayback } from '../net/posePlayback';
import type { Physics } from '../world/physics';
import { ShipSounds } from '../audio/shipSounds';
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
  /** The client's frames: the outer hull lives in the physics bubble's coordinates. */
  frames: Frames;
  /** The ground of each body (its surface with every modifier): under the legs, under the ramp. */
  surfaces: Surfaces;
  /** Height of a world point over the ground under it (m). */
  groundAlt: (p: readonly number[]) => number;
  csm: CSM | null;
  particles: Particles;
  debris: Debris;
}

/** Beyond this from its hull (m), a ship's outer colliders leave the lunar world: nothing simulated reaches that far from the player. */
const PHYSICS_REACH = 400;

const _o = new THREE.Vector3();
const _d = new THREE.Vector3();
const _wp = new THREE.Vector3();
const _wq = new THREE.Quaternion();
const _one = new THREE.Vector3(1, 1, 1);

/**
 * Client side of one ship: mirrors the server's state (ShipSim), animates what moves, keeps the
 * physics in sync and turns state changes into effects (breach debris, sparks, heat).
 *
 * Its pose has three versions: `sim.pose` (this fixed step: the flight model's if we fly it, the
 * network's played back otherwise), `prev` (the step before) and `render` (between the two, for the
 * frame being drawn). Everything drawn aboard — the hull, the crew, the crates — uses `render`.
 */
export class ShipClient implements FrameHost {
  readonly sim: ShipSim;
  readonly view: ShipView;
  readonly space: ShipSpace;
  readonly physics: ShipPhysics;
  readonly anim: ShipAnimState;
  readonly prev: ShipPose;
  readonly render: ShipPose;
  /** The network's pose stream (ships we don't fly). */
  readonly playback = new PosePlayback();
  /** Player flying it (0 = the server; offline: us or nobody). */
  pilot = 0;
  /** What it sounds like here (client/audio/shipSounds.ts). */
  readonly sounds: ShipSounds;
  private time = 0;
  private sparkT = 0;
  /**
   * Ship space ↔ world as drawn (the render pose), in world coordinates: the scene graph's matrices
   * are in render space (render/origin.ts), everything the game asks of a ship is in the world.
   */
  private toWorldM = new THREE.Matrix4();
  private toShip = new THREE.Matrix4();
  /** Authority's travel per mover and how long it has not changed (s). */
  private authMv = new Map<string, { v: number; still: number }>();
  /** Gear feet this step (ship space), for the colliders and the view. */
  private feet: V3[] = [];

  constructor(
    snap: ShipSnapshot,
    private deps: ShipDeps,
  ) {
    const def = SHIP_DEFS[snap.def];
    if (!def) throw new Error(`unknown ship "${snap.def}"`);
    this.sim = new ShipSim(snap.id, def, snap.place, deps.groundAlt, snap);
    this.prev = clonePose(this.sim.pose);
    this.render = clonePose(this.sim.pose);
    this.view = new ShipView(this.sim, deps.csm, deps.surfaces);
    this.space = new ShipSpace(deps.physics.rapier);
    this.physics = new ShipPhysics(deps.physics, this.space.world, this.sim, this.bubblePose());
    // late joiners see things where they already are (the snapshot carries the travel), no replay
    this.anim = { movers: Object.fromEntries(def.movers.map((m) => [m.key, this.sim.mover(m.key)])) };
    this.feet = this.sim.flight.feetAnywhere(this.sim.pose, deps.surfaces(bodyAt(this.sim.pose.p)), this.gearTravel());
    this.sounds = new ShipSounds(this);
    this.frame(0, 1);
  }

  get id() {
    return this.sim.id;
  }

  // --- a frame host (shared/frames): what is inside it lives in its space ---

  /** Pose this fixed step (`prev`: the step before). */
  get pose() {
    return this.sim.pose;
  }

  get reach() {
    return shipReach(this.sim.def);
  }

  /** Inside one of its compartments (ship space). */
  inside(l: readonly number[]) {
    return inShip(this.sim.def, l);
  }

  /** World → ship space, as drawn (the camera sees the rendered ship). */
  local(p: THREE.Vector3, out = new THREE.Vector3()) {
    return out.copy(p).applyMatrix4(this.toShip);
  }

  /** Ship space → world, as drawn. */
  world(p: V3, out = new THREE.Vector3()) {
    return out.set(p[0], p[1], p[2]).applyMatrix4(this.toWorldM);
  }

  /** Ship space → world as drawn (a matrix to read, not to keep). */
  get worldMatrix(): THREE.Matrix4 {
    return this.toWorldM;
  }

  /** Server update. Returns panels that were blown out (for toasts). */
  apply(sw?: Record<string, number>, hp?: Array<[number, number]>) {
    const before = hp?.map(([i]) => this.sim.hp[i]) ?? [];
    const flipped = this.sim.apply(sw, hp);
    const blown: number[] = [];
    hp?.forEach(([i, v], k) => {
      this.sounds.panel(i, before[k], v, flipped.includes(i));
      if (v < before[k] - 0.5) {
        this.view.heat[i] = Math.min(1, this.view.heat[i] + (before[k] - v) / 60);
        if (!this.sim.hole(i)) this.view.writePanel(i);
      } else if (!flipped.includes(i)) this.view.writePanel(i);
    });
    if (flipped.length) {
      this.physics.syncPanels();
      // whatever lay on a plate that just went has to notice
      this.space.world.bodies.forEach((b) => b.wakeUp());
      this.view.rebuildPanels();
      for (const i of flipped) {
        if (!this.sim.hole(i)) continue;
        blown.push(i);
        this.breachFx(i);
      }
    }
    return blown;
  }

  /** The authority refused a control: its light, and its buzzer. */
  refuse(ctl: number) {
    this.view.refuse(ctl);
    this.sounds.control(ctl, true);
  }

  /** Height over the ground under it (m): its exhausts reach the ground below this. */
  altitude() {
    return Math.max(0, this.deps.groundAlt(this.sim.pose.p) - this.sim.def.floorHeight);
  }

  /**
   * The whole state again (the server's snapshot when this ship comes back into our interest):
   * switches, panels and the state table, quietly — no breach effects for what happened far away.
   */
  sync(snap: ShipSnapshot) {
    this.sounds.resync();
    const hp = snap.hp.map((v, i): [number, number] => [i, v]);
    const flipped = this.sim.apply(snap.sw, hp);
    if (snap.st && snap.st.length === this.sim.st.length) {
      this.sim.st.set(snap.st);
      this.sim.version++;
    }
    for (let i = 0; i < hp.length; i++) if (!this.sim.hole(i)) this.view.writePanel(i);
    if (flipped.length) {
      this.physics.syncPanels();
      this.view.rebuildPanels();
    }
  }

  private breachFx(i: number) {
    const p = this.sim.def.panels[i];
    const c = new THREE.Vector3(...p.c).applyMatrix4(this.toWorldM);
    const n = new THREE.Vector3(...p.n).transformDirection(this.toWorldM);
    const glass = p.kind === 'glass';
    // air behind it blows the pieces out of the room it held (the jets and fog: decompression.ts)
    const dp = panelLoad(this.sim.sys, this.sim.st, p);
    if (dp > 8) {
      const out = facing(this.sim.def.zones, p.zone, p.other ?? null, p.c, p.n);
      n.set(out[0], out[1], out[2]).transformDirection(this.toWorldM);
    }
    const blow = 1 + dp / 25;
    // the pieces go into the physics bubble (its coordinates), leaving with the ship's own motion
    const fr = this.deps.frames;
    const cl = fr.toLocal(0, [c.x, c.y, c.z]);
    const nl = fr.dirToLocal(0, [n.x, n.y, n.z]);
    const bv = fr.pose(0)?.v ?? ZERO3;
    const vl = fr.dirToLocal(0, [this.sim.pose.v[0] - bv[0], this.sim.pose.v[1] - bv[1], this.sim.pose.v[2] - bv[2]]);
    this.deps.debris.burst(new THREE.Vector3(cl[0], cl[1], cl[2]), glass ? 10 : 7, { dir: new THREE.Vector3(nl[0], nl[1], nl[2]), speed: (glass ? 0.35 : 0.5) * blow, size: glass ? 0.35 : 0.9, spread: dp > 8 ? 0.5 : 0.9, base: new THREE.Vector3(vl[0], vl[1], vl[2]) });
    for (let k = 0; k < 40; k++) {
      this.deps.particles.emit('glow', {
        pos: c.clone().add(new THREE.Vector3().randomDirection().multiplyScalar(0.4)),
        vel: new THREE.Vector3().randomDirection().multiplyScalar(2 + Math.random() * 5).addScaledVector(n, 3),
        carry: this.render.v,
        color: glass ? [1.5, 2, 2.6] : [4, 2, 0.6],
        life: 0.2 + Math.random() * 0.6,
        size: 0.03 + Math.random() * 0.04,
        gravity: 1,
      });
    }
  }

  /** Start of a fixed step: the pose so far becomes the previous one. */
  beginStep() {
    copyPose(this.prev, this.sim.pose);
  }

  /**
   * The pose for this step is set (flown here or played back): the lunar-world body sweeps to it
   * and the interior feels the motion (apparent gravity, blended by the compensator). `focus`: the
   * player (world); a ship far from it keeps its hull out of the lunar world (PHYSICS_REACH).
   */
  posed(dt: number, focus?: THREE.Vector3) {
    const sim = this.sim;
    const p = sim.pose.p;
    const active = !focus || Math.hypot(p[0] - focus.x, p[1] - focus.y, p[2] - focus.z) < this.view.bounds.radius + PHYSICS_REACH;
    this.physics.follow(this.bubblePose(), active);
    const k = sim.vars.has('grav.k') ? sim.get('grav.k') : 0;
    this.space.update(dt, sim.pose, k);
  }

  /** The pose in the physics bubble's coordinates (this step's; reused). */
  private bubblePose(): ShipPose {
    const b = this.deps.frames.pose(0);
    const pose = this.sim.pose;
    if (!b) return pose;
    const out = this.local0;
    const l = toLocal(b, pose.p);
    const q = qMul(qConj(b.q), pose.q);
    for (let i = 0; i < 3; i++) out.p[i] = l[i];
    for (let i = 0; i < 4; i++) out.q[i] = q[i];
    return out;
  }
  private local0: ShipPose = { p: [0, 0, 0], q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0] };

  /** The physics bubble was re-laid: the outer hull jumps to where the ship is in it now. */
  rebase() {
    this.physics.teleport(this.bubblePose());
  }

  /** Gear travel as animated here (0 up … 1 down). */
  private gearTravel() {
    const key = this.sim.def.gear?.key;
    return key ? this.anim?.movers[key] ?? this.sim.mover(key) : 0;
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
    // the legs fold on the ground under them, wherever the ship stands
    this.feet = sim.flight.feetAnywhere(sim.pose, this.deps.surfaces(bodyAt(sim.pose.p)), this.gearTravel(), this.feet);
    this.physics.update(this.anim, ramp ? this.view.rampPhi(smooth(this.anim.movers[ramp.key] ?? 0)) : 0, this.feet);
  }

  /** What the view draws: the movers as animated, the ramp eased (refilled every frame). */
  private drawAnim: ShipAnimState = { movers: {} };

  /**
   * Every room's portals (built once): the openings (doors, hatches, the ramp) and the panels you
   * can see through when glass or blown out, each with its outline in ship space (null: an
   * opening with no known shape, never narrowed) and the room on the other side (-1: outside).
   */
  private portals: Portal[][] | null = null;
  /** When each room / the outside was last seen through the portals (s, `now` of portalView). */
  private roomSeen: Float64Array | null = null;
  private outsideSeen = -1;

  private buildPortals(): Portal[][] {
    const sim = this.sim;
    const def = sim.def;
    const rooms: Portal[][] = def.compartments.map(() => []);
    const rect = (c: V3, a: V3, b: V3): V3[] => [
      [c[0] - a[0] - b[0], c[1] - a[1] - b[1], c[2] - a[2] - b[2]],
      [c[0] + a[0] - b[0], c[1] + a[1] - b[1], c[2] + a[2] - b[2]],
      [c[0] + a[0] + b[0], c[1] + a[1] + b[1], c[2] + a[2] + b[2]],
      [c[0] - a[0] + b[0], c[1] - a[1] + b[1], c[2] - a[2] + b[2]],
    ];
    const outline = (key: string): V3[] | null => {
      const d = def.doors.find((x) => x.key === key);
      if (d) {
        const ax = doorAxis(d);
        return rect([d.c[0], d.c[1] + d.h / 2, d.c[2]], [ax[0] * d.w / 2, 0, ax[2] * d.w / 2], [0, d.h / 2, 0]);
      }
      const h = def.hatches.find((x) => x.key === key);
      if (h) return rect(h.c, [h.w / 2, 0, 0], [0, 0, h.l / 2]);
      const r = def.ramp;
      if (r && r.key === key) return rect([r.hinge[0], r.hinge[1] + r.length / 2, r.hinge[2]], [r.w / 2, 0, 0], [0, r.length / 2, 0]);
      return null;
    };
    for (const o of def.openings) {
      if (o.kind !== 'door' && o.kind !== 'ramp') continue;
      const a = sim.sys.compIndex(o.a);
      const b = o.b === null ? -1 : sim.sys.compIndex(o.b);
      const pts = outline(o.key);
      const s = sphereOf(pts);
      if (a >= 0) rooms[a].push({ to: b, key: o.key, panel: -1, pts, ...s });
      if (b >= 0) rooms[b].push({ to: a, key: o.key, panel: -1, pts, ...s });
    }
    for (const p of def.panels) {
      const a = sim.sys.compIndex(p.zone);
      if (a < 0) continue;
      const pts = p.poly.map(([x, y]): V3 => [p.c[0] + p.u[0] * x + p.v[0] * y, p.c[1] + p.u[1] * x + p.v[1] * y, p.c[2] + p.u[2] * x + p.v[2] * y]);
      const s = sphereOf(pts);
      const b = p.other !== undefined ? sim.sys.compIndex(p.other) : -1;
      rooms[a].push({ to: b, key: null, panel: p.index, pts, ...s });
      if (b >= 0) rooms[b].push({ to: a, key: null, panel: p.index, pts, ...s });
    }
    return rooms;
  }

  /**
   * Portal culling from inside the ship. From the camera's room, with the whole screen, every open
   * portal (a door, hatch or the ramp open, a window, a glass bulkhead, a blown-out panel) narrows
   * the view to its outline's rectangle on screen; the rooms reached go in `rooms`, and the outside
   * is seen only if a portal to it is on screen. A portal crossing the camera's near plane isn't
   * narrowed (safe side). Returns null when the camera is not in this ship.
   *
   * What was seen stays drawn for PORTAL_HOLD seconds (`now`, s): a portal test that misses for a
   * frame (a fast turn, a zoom, the eye crossing a doorway) never blinks a room or the outside out.
   */
  portalView(eye: THREE.Vector3, viewProj: THREE.Matrix4, rooms: Set<number>, now: number): { outside: boolean } | null {
    // `eye`: the camera in the world; `viewProj`: the camera's, in render space (as the root's matrix)
    const zone = this.zoneAt(eye);
    if (!zone) return null;
    const start = this.sim.sys.compIndex(zone.id);
    if (start < 0) return null;
    this.portals ??= this.buildPortals();
    const seen = (this.roomSeen ??= new Float64Array(this.portals.length).fill(-1e9));
    _mvp.multiplyMatrices(viewProj, this.view.root.matrixWorld);
    const l = this.local(eye);
    _eyeL[0] = l.x;
    _eyeL[1] = l.y;
    _eyeL[2] = l.z;
    rooms.clear();
    const out = _portalOut;
    out.outside = false;
    this.visitRoom(start, -1, -1, -1, 1, 1, 0, rooms, out);
    for (let r = 0; r < seen.length; r++) {
      if (rooms.has(r)) seen[r] = now;
      else if (now - seen[r] < PORTAL_HOLD) rooms.add(r);
    }
    if (out.outside) this.outsideSeen = now;
    else if (now - this.outsideSeen < PORTAL_HOLD) out.outside = true;
    return out;
  }

  private visitRoom(room: number, from: number, x0: number, y0: number, x1: number, y1: number, depth: number, rooms: Set<number>, out: { outside: boolean }) {
    rooms.add(room);
    const sim = this.sim;
    for (const p of this.portals![room]) {
      if (p.to === from && p.to >= 0) continue;
      const open = p.key !== null ? (this.anim.movers[p.key] ?? sim.mover(p.key)) > 0.01 : sim.def.panels[p.panel].kind === 'glass' || sim.hole(p.panel);
      if (!open) continue;
      if (p.to < 0 && out.outside) continue;
      // the portal's rectangle on screen, cut by the view that reached it
      let a0 = x0;
      let b0 = y0;
      let a1 = x1;
      let b1 = y1;
      // an eye at the portal (standing in a doorway) sees through it whatever its outline does on
      // screen; otherwise: wholly behind the eye → not seen, across its near plane → not narrowed
      const dx = _eyeL[0] - p.c[0];
      const dy = _eyeL[1] - p.c[1];
      const dz = _eyeL[2] - p.c[2];
      const close = dx * dx + dy * dy + dz * dz < (p.r + PORTAL_NEAR) * (p.r + PORTAL_NEAR);
      if (p.pts && !close) {
        const got = projectRect(p.pts, _rect);
        if (got === 0) continue;
        if (got === 1) {
          a0 = Math.max(x0, _rect[0] - PORTAL_PAD);
          b0 = Math.max(y0, _rect[1] - PORTAL_PAD);
          a1 = Math.min(x1, _rect[2] + PORTAL_PAD);
          b1 = Math.min(y1, _rect[3] + PORTAL_PAD);
          if (a0 >= a1 || b0 >= b1) continue;
        }
      }
      if (p.to < 0) {
        out.outside = true;
        continue;
      }
      if (depth < PORTAL_DEPTH) this.visitRoom(p.to, room, a0, b0, a1, b1, depth + 1, rooms, out);
    }
  }

  /** Steps the interior world still has to take after something in it moved (the broad phase catches up). */
  private interiorSteps = 3;
  private moverWas: Float64Array | null = null;
  private feetWas: number[] = [];
  private awake = false;
  private onActive = (b: RAPIER.RigidBody) => {
    if (!b.isFixed()) this.awake = true;
  };

  /**
   * The interior world only has to step when something in it can move: someone aboard (`aboard`:
   * the local astronaut is in this ship's frame), an awake body (a crate, a remote crew proxy), or
   * a door, hatch, ramp or gear leg that moved this step. A parked, empty ship costs nothing.
   */
  needsInteriorStep(aboard: boolean) {
    const movers = this.sim.def.movers;
    const was = (this.moverWas ??= new Float64Array(movers.length).fill(NaN));
    let moved = false;
    for (let i = 0; i < movers.length; i++) {
      const v = this.anim.movers[movers[i].key] ?? 0;
      if (v !== was[i]) {
        was[i] = v;
        moved = true;
      }
    }
    let k = 0;
    for (const f of this.feet) {
      for (let j = 0; j < 3; j++, k++) {
        if (this.feetWas[k] !== f[j]) {
          this.feetWas[k] = f[j];
          moved = true;
        }
      }
    }
    if (moved) this.interiorSteps = 3;
    if (aboard || this.interiorSteps > 0) {
      this.interiorSteps = Math.max(0, this.interiorSteps - 1);
      return true;
    }
    this.awake = false;
    this.space.world.forEachActiveRigidBody(this.onActive);
    return this.awake;
  }

  /** Once per frame: the pose between the last two steps, visuals and ambient effects. `eye`: the camera (render space). */
  frame(dt: number, alpha: number, eye?: THREE.Vector3) {
    this.time += dt;
    lerpPose(this.prev, this.sim.pose, alpha, this.render);
    const ramp = this.sim.def.ramp;
    let anim = this.anim;
    if (ramp) {
      const m = this.drawAnim.movers;
      for (const k in this.anim.movers) m[k] = this.anim.movers[k];
      m[ramp.key] = smooth(this.anim.movers[ramp.key] ?? 0);
      anim = this.drawAnim;
    }
    this.view.update(dt, this.time, anim, this.render, this.feet, eye);
    const r = this.render;
    this.toWorldM.compose(_wp.set(r.p[0], r.p[1], r.p[2]), _wq.set(r.q[0], r.q[1], r.q[2], r.q[3]), _one);
    this.toShip.copy(this.toWorldM).invert();
    this.sparks(dt);
  }

  frameScreens(eye: THREE.Vector3) {
    this.view.updateScreens(this.time, this.sim.def.ramp ? this.drawAnim : this.anim, eye);
  }

  /** Damaged panels spit sparks now and then; cut conduits arc. */
  private sparks(dt: number) {
    this.sparkT -= dt;
    if (this.sparkT > 0) return;
    this.sparkT = 0.08;
    const sim = this.sim;
    const M = this.toWorldM;
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
      const local: V3 = [p.c[0] + p.u[0] * x * 0.8 + p.v[0] * y * 0.8 + p.n[0] * depth, p.c[1] + p.u[1] * x * 0.8 + p.v[1] * y * 0.8 + p.n[1] * depth, p.c[2] + p.u[2] * x * 0.8 + p.v[2] * y * 0.8 + p.n[2] * depth];
      const at = new THREE.Vector3(local[0], local[1], local[2]).applyMatrix4(M);
      this.sounds.playAt('spark', local, cut ? 1 : 0.6);
      const n = new THREE.Vector3(...p.n).transformDirection(M);
      for (let k = 0; k < (cut ? 10 : 6); k++) {
        this.deps.particles.emit('glow', {
          pos: at,
          vel: new THREE.Vector3().randomDirection().multiplyScalar(0.8 + Math.random() * 2.5).addScaledVector(n, Math.random() < 0.5 ? 1 : -1),
          carry: this.render.v,
          color: cut ? [2.2, 2.6, 4] : [4, 2.2, 0.7],
          life: 0.12 + Math.random() * 0.35,
          size: 0.018 + Math.random() * 0.02,
          gravity: 1,
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
    const o = this.local(origin, _o);
    const d = _d.copy(dir).transformDirection(this.toShip);
    const O: V3 = [o.x, o.y, o.z];
    const D: V3 = [d.x, d.y, d.z];
    const occ = this.physics.castRay(O, D, max, undefined, ignoreSeats);
    const tOcc = occ ? occ.t : max;
    let best: ShipHit | null = null;
    const M = this.toWorldM;
    const world = (t: number) => origin.clone().addScaledVector(dir, t);
    let shut: Set<string> | null = null;
    for (const c of def.controls) {
      // quick reject: the ray passes nowhere near the control (most of them, every frame)
      if (farFromRay(c.c, (c.half[0] + c.half[1] + c.half[2]) * 1.6 + 0.12, O, D, Math.min(max, tOcc + 0.04))) continue;
      if (c.host >= 0 && sim.hole(c.host)) continue;
      // on a drum face turned inside (or still turning)
      if (c.drum && !this.view.drumReady(c)) continue;
      shut ??= shutCovers(def.controls, sim.sw);
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
      best = { kind: 'seat', index: i, hole: false, point: world(t), normal: new THREE.Vector3(0, 1, 0).transformDirection(M), dist: t };
    }
    for (const part of def.parts) {
      if (part.shape === 'none') continue;
      if (farFromRay(part.c, Math.hypot(part.half[0], part.half[1], part.half[2]), O, D, Math.min(max, tOcc + 0.08))) continue;
      const t = rayBox(boxFrame(part), part.half, O, D, max);
      if (t < 0 || t > tOcc + 0.08 || (best && t >= best.dist)) continue;
      best = { kind: 'part', index: part.index, hole: false, point: world(t), normal: new THREE.Vector3(0, 1, 0).transformDirection(M), dist: t };
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

  /** Rocket sweep a → b in ship space: where it hits the ship (ship space), or null. */
  localHit(a: V3, b: V3): V3 | null {
    const d: V3 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    const len = Math.hypot(d[0], d[1], d[2]);
    if (len < 1e-6) return null;
    const h = this.physics.castRay(a, [d[0] / len, d[1] / len, d[2] / len], len);
    if (!h) return null;
    const k = h.t / len;
    return [a[0] + d[0] * k, a[1] + d[1] * k, a[2] + d[2] * k];
  }

  /** Projectile sweep a → b against the ship (world, this step's pose): the hit in the world and in ship space. */
  segmentHit(a: V3, b: V3): { p: V3; l: V3 } | null {
    const l = this.localHit(this.sim.toLocal(a), this.sim.toLocal(b));
    return l ? { p: this.sim.toWorld(l), l } : null;
  }

  /** Third-person camera: distance along from→to before a ship surface (or null). */
  occlude(from: THREE.Vector3, to: THREE.Vector3): number | null {
    const o = this.local(from, _o);
    const e = this.local(to, _d);
    const d: V3 = [e.x - o.x, e.y - o.y, e.z - o.z];
    const len = Math.hypot(d[0], d[1], d[2]);
    if (len < 1e-4) return null;
    const h = this.physics.castRay([o.x, o.y, o.z], [d[0] / len, d[1] / len, d[2] / len], len);
    return h ? h.t : null;
  }

  /** Seat in ship space: feet, the way the body faces, and where standing up puts you. */
  seatPose(i: number): { pos: V3; yaw: number; exit: V3 } {
    const st = this.sim.def.seats[i];
    return { pos: [...st.root] as V3, yaw: st.yaw, exit: [...st.exit] as V3 };
  }

  /** Compartment containing a world point (as drawn), if any. */
  zoneAt(p: THREE.Vector3) {
    const l = this.local(p);
    return zoneAt(this.sim.def, [l.x, l.y, l.z]);
  }

  /** Centre of the ship in world space as drawn (compass marker). */
  get position() {
    return this.view.root.position;
  }
}

const smooth = (t: number) => t * t * (3 - 2 * t);
const ZERO3: V3 = [0, 0, 0];

/** A way to see from one room into another (or out): see `ShipClient.portalView`. */
interface Portal {
  to: number;
  key: string | null;
  panel: number;
  pts: V3[] | null;
  /** Bounding sphere of the outline (ship space): an eye this close is never narrowed. */
  c: V3;
  r: number;
}

/**
 * Portal rectangles are grown by this much (normalised screen units), followed this many rooms
 * deep, and never narrowed with the eye within PORTAL_NEAR (m) of their outline.
 */
const PORTAL_PAD = 0.06;
const PORTAL_DEPTH = 6;
const PORTAL_NEAR = 0.8;
/** Seconds a room or the outside stays drawn after the portals last showed it. */
const PORTAL_HOLD = 0.3;
const _portalOut = { outside: false };
const _mvp = new THREE.Matrix4();
const _rect = new Float64Array(4);
const _eyeL: V3 = [0, 0, 0];

function sphereOf(pts: V3[] | null): { c: V3; r: number } {
  if (!pts || !pts.length) return { c: [0, 0, 0], r: Infinity };
  const c: V3 = [0, 0, 0];
  for (const p of pts) for (let i = 0; i < 3; i++) c[i] += p[i] / pts.length;
  let r = 0;
  for (const p of pts) r = Math.max(r, Math.hypot(p[0] - c[0], p[1] - c[1], p[2] - c[2]));
  return { c, r };
}

/**
 * Screen rectangle (normalised device coordinates) of ship-space points through `_mvp` into `r`
 * (x0, y0, x1, y1). Returns 1 when every point is in front of the camera, 0 when every point is
 * behind it (not seen), 2 when the outline crosses the near plane (not to be trusted: the portal
 * is taken as wide as the view that reached it).
 */
function projectRect(pts: V3[], r: Float64Array): 0 | 1 | 2 {
  const e = _mvp.elements;
  r[0] = r[1] = Infinity;
  r[2] = r[3] = -Infinity;
  let behind = 0;
  for (const p of pts) {
    const w = e[3] * p[0] + e[7] * p[1] + e[11] * p[2] + e[15];
    if (w <= 0.05) {
      behind++;
      continue;
    }
    const x = (e[0] * p[0] + e[4] * p[1] + e[8] * p[2] + e[12]) / w;
    const y = (e[1] * p[0] + e[5] * p[1] + e[9] * p[2] + e[13]) / w;
    if (x < r[0]) r[0] = x;
    if (y < r[1]) r[1] = y;
    if (x > r[2]) r[2] = x;
    if (y > r[3]) r[3] = y;
  }
  return behind === pts.length ? 0 : behind ? 2 : 1;
}

/** A sphere (centre, radius) the ray o + t·d (unit d, 0 ≤ t ≤ max) cannot touch. */
function farFromRay(c: V3, r: number, o: V3, d: V3, max: number) {
  const ex = c[0] - o[0];
  const ey = c[1] - o[1];
  const ez = c[2] - o[2];
  const t = ex * d[0] + ey * d[1] + ez * d[2];
  if (t < -r || t > max + r) return true;
  const tc = Math.max(0, Math.min(max, t));
  const qx = ex - d[0] * tc;
  const qy = ey - d[1] * tc;
  const qz = ez - d[2] * tc;
  return qx * qx + qy * qy + qz * qz > r * r;
}
