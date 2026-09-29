import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import { INTERPOLATION_DELAY_MS } from '../../shared/constants';
import type { CrateSpec, CrateWire, Quat } from '../../shared/protocol';
import { boxDrag } from '../../shared/ship/airflow';
import { WORLD_FRAME, type Crate } from '../../shared/ship/crates';
import { slerp, type Lump } from '../../shared/ship/flight';
import { qConj, qMul, qRotate, qYaw, rayBox, type V3 } from '../../shared/ship/geom';
import { ABSOLUTE, type Frames } from '../frames/frames';
import { objectOf } from '../../shared/items';
import { OBJECT_LOOKS, type CargoPalette } from './looks';
import type { ShipClient } from '../ship/ship';
import { litMaterial } from '../ship/materials';
import { patchInteriorLights } from '../ship/interiorLights';
import { origin } from '../render/origin';

/** How the crates reach the others (null offline: we are everyone). */
export interface CrateLink {
  take(id: number): void;
  send(c: Omit<CrateWire, 'owner'>, rest: boolean): void;
}

/** Owner updates per second while a crate moves. */
const SEND_HZ = 15;
/** At rest this long (s) → handed back to nobody. */
const REST_S = 0.6;
/** Outside every compartment and touching nothing of the ship this long (s) → it left the ship. */
const LOOSE_S = 0.3;
/** Fastest a held crate swings toward the hands (m/s). */
const HOLD_VMAX = 7;
/** Pull (m/s²) that makes a resting crate worth simulating: about what it grips the deck with. */
const AIR_TAKE = 1.2;

interface Sample {
  t: number;
  fr: number;
  p: V3;
  q: Quat;
}

/** Where a crate is drawn: an instance of the mesh of its look (size × paint). */
interface CrateSlot {
  mesh: THREE.InstancedMesh;
  i: number;
}

/** One crate on this client. Poses are in its frame (the moon, or the ship it is in). */
export class CrateBody {
  body!: RAPIER.RigidBody;
  collider!: RAPIER.Collider;
  readonly slot: CrateSlot;
  /** Pose at the last two fixed steps (render interpolation). */
  readonly prevP: V3 = [0, 0, 0];
  readonly prevQ: Quat = [0, 0, 0, 1];
  readonly curP: V3 = [0, 0, 0];
  readonly curQ: Quat = [0, 0, 0, 1];
  /** Pose as drawn this frame (its frame's coordinates). */
  readonly drawP: V3 = [0, 0, 0];
  readonly drawQ: Quat = [0, 0, 0, 1];
  drawn = false;
  /** Someone else's crate: their states, played back a little in the past. */
  samples: Sample[] = [];
  still = 0;
  loose = 0;
  sendT = 0;
  /** Seconds to the next check that it has not gone under the ground (frame 0). */
  groundT = 0;
  asked = -10;
  held = false;
  /** The frame the last two poses were taken in (a frame change is not interpolated). */
  prevFr: number;
  /**
   * Lying still in frame 0 (nobody simulates it): where, in the world (float64). The bubble is
   * re-laid round the player; a crate left behind is put back from here, not from its float32
   * bubble coordinates (they lose millimetres per kilometre).
   */
  restW: { p: V3; q: Quat } | null = null;

  constructor(
    readonly id: number,
    readonly spec: CrateSpec,
    public fr: number,
    public owner: number,
    slot: CrateSlot,
  ) {
    this.slot = slot;
    this.prevFr = fr;
  }

  get mass() {
    return this.spec.mass;
  }
}

/**
 * Loose crates. Each one is a real Rapier box in the world of the frame it is in: the physics
 * bubble (frame 0, outside every ship), or a ship's interior world (ShipSpace) — so crates aboard
 * ride the ship perfectly still while the compensator is on, and slide when it is off and the ship
 * brakes or banks. On the network frame 0 is the world itself: states are converted on the way.
 *
 * One player simulates a crate at a time (its owner: whoever picked it up, pushed it or blew it
 * away); everyone else plays its states back; at rest it goes back to nobody and lies still on
 * every client. A crate changes frame on its own: into a ship when it is inside a compartment or
 * lying on the ship, out when it falls or slides off.
 */
export class Crates {
  readonly list: CrateBody[] = [];
  readonly group = new THREE.Group();
  private mats: CargoPalette;
  private byHandle = new Map<number, CrateBody>();
  private byId = new Map<number, CrateBody>();
  /** One instanced mesh per look (size × paint): two draws for all the crates that share it. */
  private looks = new Map<string, { mesh: THREE.InstancedMesh; n: number }>();
  /** Per ship: the lumps its crates add to its mass (reused step after step). */
  private lumps = new Map<number, Lump[]>();
  /** The crate in the local astronaut's hands and where it should be (its frame's coordinates). */
  hold: { crate: CrateBody; target: V3; yaw: number } | null = null;

  constructor(
    private R: typeof RAPIER,
    private frames: Frames,
    private ships: () => ShipClient[],
    /** Height of a world point over the ground under it (m). */
    private groundAlt: (world: V3) => number,
    private link: CrateLink | null,
    private me: () => number,
    csm: CSM | null,
  ) {
    const lin = (r: number, g: number, b: number) => new THREE.Color().setRGB(r, g, b, THREE.SRGBColorSpace);
    this.mats = {
      orange: patchInteriorLights(litMaterial(csm, { color: lin(0.32, 0.2, 0.05), roughness: 0.72, metalness: 0.1, envMapIntensity: 0.3 })),
      grey: patchInteriorLights(litMaterial(csm, { color: lin(0.12, 0.15, 0.17), roughness: 0.7, metalness: 0.2, envMapIntensity: 0.3 })),
      strap: patchInteriorLights(litMaterial(csm, { color: lin(0.035, 0.037, 0.04), roughness: 0.45, metalness: 0.7, envMapIntensity: 0.6 })),
      metal: patchInteriorLights(litMaterial(csm, { color: lin(0.42, 0.43, 0.45), roughness: 0.35, metalness: 0.9, envMapIntensity: 0.8 })),
    };
  }

  load(crates: Crate[]) {
    // how many of each look, so each instanced mesh is made once with room for them all
    const need = new Map<string, number>();
    for (const c of crates) need.set(lookKey(c), (need.get(lookKey(c)) ?? 0) + 1);
    for (const c of crates) {
      const cb = new CrateBody(c.id, { kind: c.kind, half: c.half, mass: c.mass, paint: c.paint }, c.fr, c.owner, this.slotFor(c, need.get(lookKey(c)) ?? 1));
      this.list.push(cb);
      this.byId.set(cb.id, cb);
      const l = this.fromWire(c.fr, c.p, c.q, c.v, c.w);
      if (c.fr === WORLD_FRAME && c.owner === 0) cb.restW = { p: [...c.p], q: [...c.q] as Quat };
      this.build(cb, l.p, l.q, l.v, l.w);
      this.setPose(cb, l.p, l.q);
      this.setPose(cb, l.p, l.q);
    }
  }

  /** An instance for a crate of this look (the look's mesh grows if it has to). */
  private slotFor(c: CrateSpec, expected: number): CrateSlot {
    const key = lookKey(c);
    let look = this.looks.get(key);
    if (!look || look.n >= look.mesh.count) {
      const cap = Math.max(expected, (look?.mesh.count ?? 0) * 2, 4);
      const drawn = OBJECT_LOOKS[objectOf(c).look] ?? OBJECT_LOOKS.crate;
      const mesh = new THREE.InstancedMesh(drawn.geometry(c), drawn.materials(c, this.mats), cap);
      mesh.castShadow = mesh.receiveShadow = true;
      // crates move every step: culled as a whole would need a new sphere each frame
      mesh.frustumCulled = false;
      mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
      for (let i = 0; i < cap; i++) mesh.setMatrixAt(i, HIDDEN);
      if (look) {
        // grow: the crates already there keep their instance numbers
        for (let i = 0; i < look.n; i++) {
          look.mesh.getMatrixAt(i, _cm);
          mesh.setMatrixAt(i, _cm);
        }
        for (const cb of this.list) if (cb.slot.mesh === look.mesh) cb.slot.mesh = mesh;
        this.group.remove(look.mesh);
        look.mesh.dispose();
        look.mesh = mesh;
      } else {
        look = { mesh, n: 0 };
        this.looks.set(key, look);
      }
      this.group.add(mesh);
    }
    return { mesh: look.mesh, i: look.n++ };
  }

  /** Simulated here: ours, or everyone's offline. */
  mine(c: CrateBody) {
    return c.owner === this.me();
  }

  /** (Re)create the body in the world of the crate's frame, of the kind its ownership asks for. */
  private build(c: CrateBody, p: V3, q: Quat, v: V3 = [0, 0, 0], w: V3 = [0, 0, 0]) {
    const R = this.R;
    const world = this.frames.world(c.fr);
    const desc = (this.mine(c) ? R.RigidBodyDesc.dynamic() : R.RigidBodyDesc.kinematicPositionBased())
      .setTranslation(p[0], p[1], p[2])
      .setRotation({ x: q[0], y: q[1], z: q[2], w: q[3] })
      .setLinvel(v[0], v[1], v[2])
      .setAngvel({ x: w[0], y: w[1], z: w[2] })
      .setLinearDamping(0.05)
      .setAngularDamping(0.3)
      .setCcdEnabled(true);
    c.body = world.createRigidBody(desc);
    const [hx, hy, hz] = c.spec.half;
    c.collider = world.createCollider(R.ColliderDesc.cuboid(hx, hy, hz).setMass(c.spec.mass).setFriction(0.85).setRestitution(0.05), c.body);
    this.byHandle.set(c.collider.handle, c);
    this.gravityScale(c);
  }

  /** Every world pulls with its own gravity (the bubble: the body's; a ship: what the crew feels). */
  private gravityScale(c: CrateBody) {
    if (!this.mine(c)) return;
    c.body.setGravityScale(c.held ? 0 : 1, true);
  }

  /** A state from the network (frame 0 = the world) in the crate's frame here. */
  private fromWire(fr: number, p: V3, q: Quat, v: V3 = [0, 0, 0], w: V3 = [0, 0, 0]): { p: V3; q: Quat; v: V3; w: V3 } {
    if (fr !== WORLD_FRAME || !this.frames.pose(WORLD_FRAME)) return { p, q, v, w };
    const l = this.frames.transfer(ABSOLUTE, WORLD_FRAME, p, v, q);
    return { p: l.p, q: l.q, v: l.v, w: this.frames.dirToLocal(WORLD_FRAME, w) };
  }

  /**
   * The bubble was re-laid: every crate in it carried into the new frame (the ones lying still put
   * back from where they lie in the world).
   */
  rebase() {
    for (const c of this.list) {
      if (c.fr !== WORLD_FRAME) continue;
      let p: V3;
      let q: Quat;
      if (!this.mine(c) && c.restW) {
        const l = this.fromWire(WORLD_FRAME, c.restW.p, c.restW.q);
        p = l.p;
        q = l.q;
        c.body.setTranslation({ x: p[0], y: p[1], z: p[2] }, false);
        c.body.setRotation({ x: q[0], y: q[1], z: q[2], w: q[3] }, false);
      } else {
        const t = c.body.translation();
        const r = c.body.rotation();
        const lv = c.body.linvel();
        const av = c.body.angvel();
        const n = this.frames.rebase([t.x, t.y, t.z], [lv.x, lv.y, lv.z], [r.x, r.y, r.z, r.w]);
        const b = this.frames.bubble;
        const wv = dirTurn(b.from.q, b.pose.q, [av.x, av.y, av.z]);
        p = n.p;
        q = n.q;
        c.body.setTranslation({ x: p[0], y: p[1], z: p[2] }, true);
        c.body.setRotation({ x: q[0], y: q[1], z: q[2], w: q[3] }, true);
        if (c.body.isDynamic()) {
          c.body.setLinvel({ x: n.v[0], y: n.v[1], z: n.v[2] }, true);
          c.body.setAngvel({ x: wv[0], y: wv[1], z: wv[2] }, true);
        } else {
          // played back: its next target comes from its samples (world); until then it stays put
          c.body.setNextKinematicTranslation({ x: p[0], y: p[1], z: p[2] });
          c.body.setNextKinematicRotation({ x: q[0], y: q[1], z: q[2], w: q[3] });
        }
      }
      // no interpolation across the re-laying
      this.setPose(c, p, q);
      this.setPose(c, p, q);
    }
    if (this.hold && this.hold.crate.fr === WORLD_FRAME) this.hold.target = [...this.hold.crate.curP] as V3;
  }

  private remove(c: CrateBody) {
    const world = this.frames.world(c.fr);
    this.byHandle.delete(c.collider.handle);
    world.removeRigidBody(c.body);
  }

  /** Move a crate into another frame (same motion in the world), rebuilding its body there. */
  moveTo(c: CrateBody, fr: number) {
    const t = c.body.translation();
    const r = c.body.rotation();
    const lv = c.body.linvel();
    const av = c.body.angvel();
    const next = this.frames.transfer(c.fr, fr, [t.x, t.y, t.z], [lv.x, lv.y, lv.z], [r.x, r.y, r.z, r.w]);
    const wf = this.frames.dirToWorld(c.fr, [av.x, av.y, av.z]);
    const w = this.frames.dirToLocal(fr, wf);
    this.remove(c);
    c.fr = fr;
    this.build(c, next.p, next.q, next.v, w);
    this.setPose(c, next.p, next.q);
    c.prevFr = fr;
  }

  /** Switch between simulated here and played back (ownership changed). */
  private rebuild(c: CrateBody) {
    const t = c.body.translation();
    const r = c.body.rotation();
    const lv = c.body.linvel();
    const av = c.body.angvel();
    this.remove(c);
    this.build(c, [t.x, t.y, t.z], [r.x, r.y, r.z, r.w], [lv.x, lv.y, lv.z], [av.x, av.y, av.z]);
  }

  private setPose(c: CrateBody, p: V3, q: Quat) {
    this.setPose7(c, p[0], p[1], p[2], q[0], q[1], q[2], q[3]);
  }

  /** The same from scalars (the step reads Rapier's vectors: no arrays in between). */
  private setPose7(c: CrateBody, px: number, py: number, pz: number, qx: number, qy: number, qz: number, qw: number) {
    for (let i = 0; i < 3; i++) c.prevP[i] = c.curP[i];
    for (let i = 0; i < 4; i++) c.prevQ[i] = c.curQ[i];
    c.curP[0] = px;
    c.curP[1] = py;
    c.curP[2] = pz;
    c.curQ[0] = qx;
    c.curQ[1] = qy;
    c.curQ[2] = qz;
    c.curQ[3] = qw;
  }

  byCollider(handle: number) {
    return this.byHandle.get(handle);
  }

  // ---------------------------------------------------------------------------------------------
  // Ownership
  // ---------------------------------------------------------------------------------------------

  /** Start simulating a crate here (optimistic: the server confirms or says who has it). */
  take(c: CrateBody) {
    if (this.mine(c)) return;
    if (c.owner !== 0 && performance.now() / 1000 - c.asked < 1.5) return;
    c.asked = performance.now() / 1000;
    this.link?.take(c.id);
    c.owner = this.me();
    c.samples = [];
    c.still = 0;
    c.restW = null;
    this.rebuild(c);
  }

  /** The server's word on a crate (a state from its owner, a new owner, or at rest). */
  receive(w: CrateWire, rest: boolean, serverNow: number) {
    const c = this.byId.get(w.id);
    if (!c) return;
    const me = this.me();
    if (w.owner === me) {
      if (!this.mine(c)) {
        c.owner = me;
        this.rebuild(c);
      }
      return;
    }
    const was = this.mine(c);
    if (was && c.held) this.hold = null;
    c.held = false;
    c.owner = w.owner;
    const l = this.fromWire(w.fr, w.p, w.q);
    if (c.fr !== w.fr) {
      this.remove(c);
      c.fr = w.fr;
      this.build(c, l.p, l.q);
      c.prevFr = w.fr;
    } else if (was) this.rebuild(c);
    c.restW = null;
    if (rest || w.owner === 0) {
      c.samples = [];
      if (w.fr === WORLD_FRAME) c.restW = { p: [...w.p], q: [...w.q] as Quat };
      c.body.setTranslation({ x: l.p[0], y: l.p[1], z: l.p[2] }, true);
      c.body.setRotation({ x: l.q[0], y: l.q[1], z: l.q[2], w: l.q[3] }, true);
      this.setPose(c, l.p, l.q);
      this.setPose(c, l.p, l.q);
      return;
    }
    // samples stay as sent (frame 0: the world) and are brought into the bubble when played
    c.samples.push({ t: serverNow, fr: w.fr, p: [...w.p], q: [...w.q] as Quat });
    if (c.samples.length > 30) c.samples.shift();
  }

  // ---------------------------------------------------------------------------------------------
  // Fixed step
  // ---------------------------------------------------------------------------------------------

  /** Before the worlds step: hands, playback, frame changes. */
  beforeStep(dt: number, serverNow: number) {
    for (const c of this.list) {
      if (this.mine(c)) {
        if (c.held) this.steer(c, dt);
        else this.transitions(c, dt);
      } else if (c.samples.length) this.playback(c, serverNow - INTERPOLATION_DELAY_MS);
    }
  }

  /** After the worlds step: poses for rendering, rest detection, what the others need to know. */
  afterStep(dt: number) {
    for (const c of this.list) {
      const t = c.body.translation();
      const r = c.body.rotation();
      if (c.fr !== c.prevFr) {
        c.prevFr = c.fr;
        this.setPose7(c, t.x, t.y, t.z, r.x, r.y, r.z, r.w);
      }
      this.setPose7(c, t.x, t.y, t.z, r.x, r.y, r.z, r.w);
      if (!this.mine(c)) continue;
      // outside the ships: where the ground has no collision here (away from the player, or seen
      // from orbit) a crate would sink through it — it is laid on the ground and left at rest
      if (c.fr === WORLD_FRAME && !c.held && (c.groundT -= dt) <= 0) {
        c.groundT = 0.25;
        if (this.settle(c)) continue;
      }
      const lv = c.body.linvel();
      const av = c.body.angvel();
      const quiet = Math.hypot(lv.x, lv.y, lv.z) < 0.05 && Math.hypot(av.x, av.y, av.z) < 0.08;
      c.still = quiet && !c.held ? c.still + dt : 0;
      if (c.still > REST_S) {
        // lies still: nobody's now, the same pose everywhere
        const w = this.wire(c);
        this.link?.send(w, true);
        if (c.fr === WORLD_FRAME) c.restW = { p: [...w.p], q: [...w.q] as Quat };
        c.owner = 0;
        c.still = 0;
        this.rebuild(c);
        continue;
      }
      c.sendT -= dt;
      if (c.sendT <= 0) {
        c.sendT = 1 / SEND_HZ;
        this.link?.send(this.wire(c), false);
      }
    }
  }

  /**
   * A crate of ours in frame 0 gone under the ground: back on top of it (along the local vertical)
   * at rest, for everyone. True when it was.
   */
  private settle(c: CrateBody): boolean {
    const t = c.body.translation();
    const w = this.frames.toWorld(WORLD_FRAME, [t.x, t.y, t.z]);
    const hy = c.spec.half[1];
    const alt = this.groundAlt(w);
    if (alt > -hy * 0.5) return false;
    // "up" where it lies: the bubble's (its vertical leans a fraction of a degree a few km away)
    const g = this.frames.bubble.gravity;
    const gl = g.length() || 1;
    const k = hy + 0.02 - alt;
    const p: V3 = [t.x - (g.x / gl) * k, t.y - (g.y / gl) * k, t.z - (g.z / gl) * k];
    c.body.setTranslation({ x: p[0], y: p[1], z: p[2] }, true);
    c.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    c.body.setAngvel({ x: 0, y: 0, z: 0 }, true);
    const r = c.body.rotation();
    this.setPose7(c, p[0], p[1], p[2], r.x, r.y, r.z, r.w);
    this.setPose7(c, p[0], p[1], p[2], r.x, r.y, r.z, r.w);
    const wire = this.wire(c);
    this.link?.send(wire, true);
    c.restW = { p: [...wire.p], q: [...wire.q] as Quat };
    c.owner = 0;
    c.still = 0;
    this.rebuild(c);
    return true;
  }

  /** The crate's state for the network (frame 0: in the world). */
  private wire(c: CrateBody): Omit<CrateWire, 'owner'> {
    const t = c.body.translation();
    const r = c.body.rotation();
    const lv = c.body.linvel();
    const av = c.body.angvel();
    let p: V3 = [t.x, t.y, t.z];
    let q: Quat = [r.x, r.y, r.z, r.w];
    let v: V3 = [lv.x, lv.y, lv.z];
    let w: V3 = [av.x, av.y, av.z];
    if (c.fr === WORLD_FRAME && this.frames.pose(WORLD_FRAME)) {
      const n = this.frames.transfer(WORLD_FRAME, ABSOLUTE, p, v, q);
      p = n.p;
      q = n.q;
      v = n.v;
      w = this.frames.dirToWorld(WORLD_FRAME, w);
    }
    const r3 = (n: number) => Math.round(n * 1000) / 1000;
    const r4 = (n: number) => Math.round(n * 10000) / 10000;
    return { id: c.id, fr: c.fr, p: [r3(p[0]), r3(p[1]), r3(p[2])], q: [r4(q[0]), r4(q[1]), r4(q[2]), r4(q[3])], v: [r3(v[0]), r3(v[1]), r3(v[2])], w: [r3(w[0]), r3(w[1]), r3(w[2])] };
  }

  /** Someone else's crate: kinematic, following their states a little in the past. */
  private playback(c: CrateBody, t: number) {
    const s = c.samples;
    while (s.length > 2 && s[1].t <= t) s.shift();
    const a = s[0];
    const b = s[1];
    let p: V3 = a.p;
    let q: Quat = a.q;
    let fr = a.fr;
    if (b && t >= a.t) {
      if (b.fr !== a.fr) {
        if (t >= b.t) ({ p, q, fr } = b);
      } else {
        const k = Math.min(1, (t - a.t) / Math.max(1, b.t - a.t));
        p = [a.p[0] + (b.p[0] - a.p[0]) * k, a.p[1] + (b.p[1] - a.p[1]) * k, a.p[2] + (b.p[2] - a.p[2]) * k];
        q = slerp(a.q, b.q, k);
      }
    }
    if (fr === WORLD_FRAME) ({ p, q } = this.fromWire(fr, p, q));
    if (fr !== c.fr) {
      this.remove(c);
      c.fr = fr;
      this.build(c, p, q);
    }
    c.body.setNextKinematicTranslation({ x: p[0], y: p[1], z: p[2] });
    c.body.setNextKinematicRotation({ x: q[0], y: q[1], z: q[2], w: q[3] });
  }

  /** Our crate on its own: does it belong to another frame now? */
  private transitions(c: CrateBody, dt: number) {
    const t = c.body.translation();
    const p: V3 = [t.x, t.y, t.z];
    const hy = c.spec.half[1];
    if (c.fr === WORLD_FRAME) {
      const pw = this.frames.toWorld(WORLD_FRAME, p);
      for (const ship of this.ships()) {
        const l = ship.sim.toLocal(pw);
        if (Math.hypot(l[0], l[2]) > 40) continue;
        // inside a compartment, or lying on the ship (the ramp, the roof, the stairs)
        const g = this.frames.bubble.gravity;
        const gl = g.length() || 1;
        const ray = new this.R.Ray({ x: p[0], y: p[1], z: p[2] }, { x: g.x / gl, y: g.y / gl, z: g.z / gl });
        const hit = this.frames.world(WORLD_FRAME).castRay(ray, hy + 0.15, true, undefined, undefined, c.collider);
        if (ship.inside(l) || (hit && ship.physics.ownsOuter(hit.collider.handle))) {
          this.moveTo(c, ship.id);
          return;
        }
      }
      return;
    }
    const ship = this.frames.ship(c.fr);
    if (!ship) return;
    const inside = ship.inside(p);
    const g = ship.space.gravity;
    const gl = g.length() || 1;
    const ray = new this.R.Ray({ x: p[0], y: p[1], z: p[2] }, { x: g.x / gl, y: g.y / gl, z: g.z / gl });
    const supported = !!ship.space.world.castRay(ray, hy + 0.2, true, undefined, undefined, c.collider);
    c.loose = inside || supported ? 0 : c.loose + dt;
    const w = ship.sim.toWorld(p);
    const onMoon = this.groundAlt(w) - hy <= 0.03;
    if ((!inside && onMoon) || c.loose > LOOSE_S) {
      c.loose = 0;
      this.moveTo(c, WORLD_FRAME);
    }
  }

  // ---------------------------------------------------------------------------------------------
  // In the hands (Half-Life style: a spring to a point in front of the eyes, still colliding)
  // ---------------------------------------------------------------------------------------------

  /** Pick up (it becomes ours; `fr` = the holder's frame). */
  grab(c: CrateBody, fr: number) {
    this.take(c);
    if (c.fr !== fr) this.moveTo(c, fr);
    c.held = true;
    c.still = 0;
    this.gravityScale(c);
    this.hold = { crate: c, target: [...c.curP] as V3, yaw: 0 };
  }

  /** Let go: `v` (frame coordinates) is added to the crate's own motion (a throw). */
  release(v: V3 = [0, 0, 0]) {
    const h = this.hold;
    if (!h) return;
    this.hold = null;
    const c = h.crate;
    c.held = false;
    this.gravityScale(c);
    const lv = c.body.linvel();
    c.body.setLinvel({ x: lv.x + v[0], y: lv.y + v[1], z: lv.z + v[2] }, true);
  }

  /** Put it down exactly there (frame coordinates), at rest. */
  place(p: V3, q: Quat) {
    const h = this.hold;
    if (!h) return;
    this.hold = null;
    const c = h.crate;
    c.held = false;
    c.body.setTranslation({ x: p[0], y: p[1], z: p[2] }, true);
    c.body.setRotation({ x: q[0], y: q[1], z: q[2], w: q[3] }, true);
    c.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    c.body.setAngvel({ x: 0, y: 0, z: 0 }, true);
    this.gravityScale(c);
    this.setPose(c, p, q);
    this.setPose(c, p, q);
  }

  /** The held crate follows its target in the holder's frame; too far (stuck behind something) → dropped. */
  private steer(c: CrateBody, dt: number) {
    const h = this.hold;
    if (!h || h.crate !== c) {
      c.held = false;
      this.gravityScale(c);
      return;
    }
    const t = c.body.translation();
    const e: V3 = [h.target[0] - t.x, h.target[1] - t.y, h.target[2] - t.z];
    const d = Math.hypot(...e);
    if (d > 1.4) {
      this.release();
      return;
    }
    // close a third of the gap every step, never faster than a brisk swing
    const v: V3 = e.map((x) => (x / Math.max(dt, 1e-3)) * 0.35) as V3;
    const vl = Math.hypot(...v);
    const s = vl > HOLD_VMAX ? HOLD_VMAX / vl : 1;
    c.body.setLinvel({ x: v[0] * s, y: v[1] * s, z: v[2] * s }, true);
    // keep it upright, turned with the view
    const r = c.body.rotation();
    const want = qYaw(h.yaw);
    const dq = qMul(want, qConj([r.x, r.y, r.z, r.w]));
    const sgn = dq[3] < 0 ? -1 : 1;
    const wv: V3 = [dq[0] * sgn * 2 * 6, dq[1] * sgn * 2 * 6, dq[2] * sgn * 2 * 6];
    c.body.setAngvel({ x: wv[0], y: wv[1], z: wv[2] }, true);
  }

  // ---------------------------------------------------------------------------------------------
  // Queries and effects
  // ---------------------------------------------------------------------------------------------

  /** World pose of a crate as drawn (render-interpolated in its frame). */
  worldPose(c: CrateBody): { p: V3; q: Quat } {
    const pose = this.frames.pose(c.fr, true);
    const lp = c.drawn ? c.drawP : c.curP;
    const lq = c.drawn ? c.drawQ : c.curQ;
    if (!pose) return { p: [...lp] as V3, q: [...lq] as Quat };
    return { p: this.frames.toWorld(c.fr, lp, true), q: qMul(pose.q, lq) };
  }

  /** First crate along a world ray (as drawn), within `max`. */
  pick(o: V3, d: V3, max: number, skip?: CrateBody): { crate: CrateBody; t: number } | null {
    let best: { crate: CrateBody; t: number } | null = null;
    for (const c of this.list) {
      if (c === skip) continue;
      const { p, q } = this.worldPose(c);
      const f = { c: p, u: qRotate(q, [1, 0, 0]), v: qRotate(q, [0, 1, 0]), n: qRotate(q, [0, 0, 1]) };
      const t = rayBox(f, c.spec.half, o, d, max);
      if (t >= 0 && (!best || t < best.t)) best = { crate: c, t };
    }
    return best;
  }

  /** Blast wave: crates we simulate (or nobody does, if we answer for this blast) are thrown. */
  blast(at: V3, responsible: boolean, radius = 6, strength = 260) {
    for (const c of this.list) {
      const w = this.worldPose(c).p;
      const d: V3 = [w[0] - at[0], w[1] - at[1], w[2] - at[2]];
      const dist = Math.hypot(...d);
      if (dist > radius) continue;
      if (!this.mine(c)) {
        if (c.owner !== 0 || !responsible) continue;
        this.take(c);
      }
      const k = (strength * (1 - dist / radius)) / Math.max(dist, 1e-3);
      const jw: V3 = [d[0] * k, d[1] * k + strength * (1 - dist / radius) * 0.35, d[2] * k];
      const j = this.frames.dirToLocal(c.fr, jw);
      c.body.applyImpulse({ x: j[0], y: j[1], z: j[2] }, true);
      const s = strength * (1 - dist / radius) * 0.3;
      c.body.applyTorqueImpulse({ x: (Math.random() - 0.5) * s, y: (Math.random() - 0.5) * s, z: (Math.random() - 0.5) * s }, true);
    }
  }

  /**
   * A steady pull on every crate (the air rushing to a breach): `accel(frame, centre, drag area per
   * kg)` in the crate's frame (m/s²). Crates we simulate take it; one nobody simulates is taken by
   * the responsible client once the pull could move it (more than it grips the floor).
   */
  airPull(dt: number, accel: (fr: number, p: V3, cda: number) => V3 | null, responsible: boolean) {
    for (const c of this.list) {
      if (c.held) continue;
      const t = c.body.translation();
      const a = accel(c.fr, [t.x, t.y, t.z], boxDrag(c.spec.half, c.spec.mass));
      if (!a) continue;
      if (!this.mine(c)) {
        if (c.owner !== 0 || !responsible || Math.hypot(a[0], a[1], a[2]) < AIR_TAKE) continue;
        this.take(c);
      }
      const m = c.spec.mass * dt;
      c.body.applyImpulse({ x: a[0] * m, y: a[1] * m, z: a[2] * m }, true);
      c.still = 0;
    }
  }

  /**
   * Crates a ship carries (ship space), for its mass, appended to `out`. The lumps are reused
   * step after step (the flight model reads them right away).
   */
  aboard(shipId: number, out: Lump[] = []) {
    let pool = this.lumps.get(shipId);
    if (!pool) this.lumps.set(shipId, (pool = []));
    let k = 0;
    for (const c of this.list) {
      if (c.fr !== shipId) continue;
      const l = (pool[k] ??= { m: 0, c: [0, 0, 0], half: c.spec.half, group: 'carga' }) as Lump & { c: V3 };
      k++;
      l.m = c.spec.mass;
      l.c[0] = c.curP[0];
      l.c[1] = c.curP[1];
      l.c[2] = c.curP[2];
      l.half = c.spec.half;
      out.push(l);
    }
    return out;
  }

  /** Once per frame: instances where the crates are, between the last two steps, in their frame as drawn. */
  sync(alpha: number) {
    for (const c of this.list) {
      const lp = c.drawP;
      lp[0] = c.prevP[0] + (c.curP[0] - c.prevP[0]) * alpha;
      lp[1] = c.prevP[1] + (c.curP[1] - c.prevP[1]) * alpha;
      lp[2] = c.prevP[2] + (c.curP[2] - c.prevP[2]) * alpha;
      _qa.set(c.prevQ[0], c.prevQ[1], c.prevQ[2], c.prevQ[3]);
      _qb.set(c.curQ[0], c.curQ[1], c.curQ[2], c.curQ[3]);
      _qa.slerp(_qb, alpha);
      const lq = c.drawQ;
      lq[0] = _qa.x;
      lq[1] = _qa.y;
      lq[2] = _qa.z;
      lq[3] = _qa.w;
      c.drawn = true;
      const pose = this.frames.pose(c.fr, true);
      _cp.set(lp[0], lp[1], lp[2]);
      if (pose) {
        _qb.set(pose.q[0], pose.q[1], pose.q[2], pose.q[3]);
        _cp.applyQuaternion(_qb).add(_cv.set(pose.p[0], pose.p[1], pose.p[2]));
        _qa.premultiply(_qb);
      }
      // instances in render space (the meshes hang from the scene, not from the origin's root)
      origin.toRender(_cp);
      c.slot.mesh.setMatrixAt(c.slot.i, _cm.compose(_cp, _qa, ONE));
      c.slot.mesh.instanceMatrix.needsUpdate = true;
    }
  }
}

/** An angular velocity in the axes of rotation `a` turned into the axes of rotation `b` (both world rotations). */
function dirTurn(a: Quat, b: Quat, w: V3): V3 {
  return qRotate(qConj(b), qRotate(a, w));
}

const lookKey = (c: CrateSpec) => `${objectOf(c).look}|${c.half[0]},${c.half[1]},${c.half[2]}|${c.paint}`;
const HIDDEN = new THREE.Matrix4().makeScale(0, 0, 0);
const ONE = new THREE.Vector3(1, 1, 1);
const _cm = new THREE.Matrix4();
const _cp = new THREE.Vector3();
const _cv = new THREE.Vector3();
const _qa = new THREE.Quaternion();
const _qb = new THREE.Quaternion();
