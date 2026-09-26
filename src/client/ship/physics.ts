import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';
import { doorAxis, SEAT_BOXES, nacellePylons, seatFrame } from '../../shared/ship/def';
import type { V3 } from '../../shared/ship/geom';
import type { ShipSim } from '../../shared/ship/sim';
import type { Physics } from '../world/physics';
import { propHull } from './models';
import type { ShipAnimState } from './screens';

type Owner = { kind: 'panel'; index: number } | { kind: 'solid'; seat?: boolean };

const q = (e: THREE.Euler) => {
  const t = new THREE.Quaternion().setFromEuler(e);
  return { x: t.x, y: t.y, z: t.z, w: t.w };
};

/**
 * Rapier side of a ship: one fixed body at the ship pose, a convex hull per solid panel (removed
 * when it is blown out, re-created when repaired), static props, and colliders that follow the
 * doors, the ramp and the canopy shutters.
 */
export class ShipPhysics {
  private body: RAPIER.RigidBody;
  private panels: Array<RAPIER.Collider | null>;
  private owners = new Map<number, Owner>();
  private doors = new Map<string, RAPIER.Collider[]>();
  private ramp: RAPIER.Collider | null = null;
  private shutters: RAPIER.Collider[] = [];
  private mounted: Array<{ host: number; collider: RAPIER.Collider }> = [];
  private R: typeof RAPIER;
  private world: RAPIER.World;

  constructor(
    physics: Physics,
    private sim: ShipSim,
    ground: (x: number, z: number) => number,
  ) {
    const R = (this.R = physics.rapier);
    const world = (this.world = physics.world);
    // parked ships are fixed bodies at their pose; flight will make this kinematic (docs/SHIPS.md §7)
    const pose = sim.pose;
    this.body = world.createRigidBody(R.RigidBodyDesc.fixed().setTranslation(...pose.p).setRotation({ x: pose.q[0], y: pose.q[1], z: pose.q[2], w: pose.q[3] }));
    const p = sim.place;
    this.panels = sim.def.panels.map(() => null);
    this.syncPanels();

    const def = sim.def;
    let seat = false;
    const solid = (d: RAPIER.ColliderDesc) => {
      const c = world.createCollider(d.setFriction(0.8), this.body);
      this.owners.set(c.handle, { kind: 'solid', seat });
      return c;
    };
    const box = (hx: number, hy: number, hz: number, pos: V3, e?: THREE.Euler) => {
      const d = R.ColliderDesc.cuboid(hx, hy, hz).setTranslation(...pos);
      if (e) d.setRotation(q(e));
      return solid(d);
    };
    const T = 0.1;
    for (const m of def.modules) {
      const hw = m.profile[m.profile.length - 1][0] + T;
      const zc = (m.z0 + m.z1) / 2;
      const hz = (m.z1 - m.z0) / 2;
      box(hw, 0.075, hz, [0, -0.375, zc]);
      for (const sx of [-1, 1]) box(T / 2, 0.225, hz, [sx * (hw - T / 2), -0.225, zc]);
    }
    for (const py of nacellePylons(def.parts, def.modules)) box(py.half[0], py.half[1], py.half[2], py.c);
    for (const part of def.parts) {
      if (part.shape === 'none') continue;
      box(part.half[0], part.half[1], part.half[2], part.c, part.yaw ? new THREE.Euler(0, part.yaw, 0) : undefined);
    }
    for (const prop of def.props) {
      if (prop.collide === 'box') box(prop.half[0], prop.half[1], prop.half[2], prop.c, prop.yaw ? new THREE.Euler(0, prop.yaw, 0) : undefined);
      else if (prop.collide === 'hull') {
        // wedges and fairings: the convex hull of the model's own outline (chin, fin, stairs…)
        const pts = propHull(prop);
        const d = pts && R.ColliderDesc.convexHull(new Float32Array(pts.flat()));
        if (d) solid(d);
      }
    }
    // gear
    const m = new THREE.Matrix4().compose(new THREE.Vector3(p.x, p.y, p.z), new THREE.Quaternion().setFromEuler(new THREE.Euler(0, p.yaw, 0)), new THREE.Vector3(1, 1, 1));
    const inv = m.clone().invert();
    for (const leg of def.gear?.legs ?? []) {
      const w = new THREE.Vector3(...leg).applyMatrix4(m);
      const footY = new THREE.Vector3(w.x, ground(w.x, w.z), w.z).applyMatrix4(inv).y;
      const hh = (-0.45 - footY) / 2;
      if (hh > 0.05) solid(R.ColliderDesc.cylinder(hh, 0.12).setTranslation(leg[0], footY + hh, leg[2]));
      solid(R.ColliderDesc.cylinder(0.065, 0.34).setTranslation(leg[0], footY + 0.065, leg[2]));
    }
    seat = true;
    for (const st of def.seats) {
      for (const b of Object.values(SEAT_BOXES)) {
        const f = seatFrame(st, b);
        box(f.half[0], f.half[1], f.half[2], f.c, new THREE.Euler(0, st.yaw, 0));
      }
    }
    seat = false;
    for (const con of def.consoles) {
      const basis = new THREE.Matrix4().makeBasis(new THREE.Vector3(...con.u), new THREE.Vector3(...con.v), new THREE.Vector3(...con.n));
      const qq = new THREE.Quaternion().setFromRotationMatrix(basis);
      const c: V3 = [con.c[0] - con.n[0] * con.depth * 0.5, con.c[1] - con.n[1] * con.depth * 0.5, con.c[2] - con.n[2] * con.depth * 0.5];
      const col = solid(R.ColliderDesc.cuboid(con.w / 2, con.h / 2, con.depth / 2).setTranslation(...c).setRotation({ x: qq.x, y: qq.y, z: qq.z, w: qq.w }));
      // a console on a hull panel goes with it (the view hides it too)
      if (con.host >= 0) this.mounted.push({ host: con.host, collider: col });
    }
    this.syncMounted();

    // movers
    for (const d of def.doors) {
      const rot = q(new THREE.Euler(0, Math.atan2(d.n[0], d.n[2]), 0));
      // the two leaves overlap a little where they meet, so a shut door has no seam to slip through
      const leaves = [-1, 1].map(() => solid(R.ColliderDesc.cuboid(d.w / 4 + 0.01, d.h / 2, 0.025).setTranslation(d.c[0], d.c[1] + d.h / 2, d.c[2] + d.offset).setRotation(rot)));
      this.doors.set(d.key, leaves);
    }
    const r = def.ramp;
    if (r) this.ramp = solid(R.ColliderDesc.cuboid(r.w / 2, r.length / 2, r.t / 2).setTranslation(r.hinge[0], r.hinge[1] + r.length / 2, r.hinge[2] + r.t / 2));
    for (const pl of def.shield?.plates ?? []) {
      const basis = new THREE.Matrix4().makeBasis(new THREE.Vector3(...pl.u), new THREE.Vector3(...pl.v), new THREE.Vector3(...pl.n));
      const qq = new THREE.Quaternion().setFromRotationMatrix(basis);
      const c = solid(R.ColliderDesc.cuboid(pl.w / 2, pl.h / 2, 0.02).setTranslation(...pl.c).setRotation({ x: qq.x, y: qq.y, z: qq.z, w: qq.w }));
      c.setEnabled(false);
      this.shutters.push(c);
    }
  }

  /** Add / remove panel hulls to match the integrity state. Returns how many changed. */
  /** Consoles mounted on a panel that is blown out lose their collider with it. */
  private syncMounted() {
    for (const m of this.mounted) {
      const on = !this.sim.hole(m.host);
      if (m.collider.isEnabled() !== on) m.collider.setEnabled(on);
    }
  }

  syncPanels() {
    let n = 0;
    for (const p of this.sim.def.panels) {
      const want = !this.sim.hole(p.index);
      const have = this.panels[p.index];
      if (want && !have) {
        const pts: number[] = [];
        for (const [x, y] of p.poly) {
          for (const s of [-1, 1]) {
            const z = (s * p.t) / 2;
            pts.push(p.c[0] + p.u[0] * x + p.v[0] * y + p.n[0] * z, p.c[1] + p.u[1] * x + p.v[1] * y + p.n[1] * z, p.c[2] + p.u[2] * x + p.v[2] * y + p.n[2] * z);
          }
        }
        const desc = this.R.ColliderDesc.convexHull(new Float32Array(pts));
        if (!desc) continue;
        const c = this.world.createCollider(desc.setFriction(p.kind === 'floor' ? 0.9 : 0.6), this.body);
        this.panels[p.index] = c;
        this.owners.set(c.handle, { kind: 'panel', index: p.index });
        n++;
      } else if (!want && have) {
        this.owners.delete(have.handle);
        this.world.removeCollider(have, true);
        this.panels[p.index] = null;
        n++;
      }
    }
    this.syncMounted();
    return n;
  }

  hasPanel(i: number) {
    return !!this.panels[i];
  }

  /** Follow the animated parts (fixed step). */
  update(anim: ShipAnimState, rampPhi: number) {
    const def = this.sim.def;
    for (const d of def.doors) {
      const open = anim.movers[d.key] ?? 0;
      const a = doorAxis(d);
      this.doors.get(d.key)!.forEach((c, k) => {
        const side = k === 0 ? -1 : 1;
        const s = side * (d.w / 4 + open * (d.w / 2 - 0.03));
        c.setTranslationWrtParent({ x: d.c[0] + a[0] * s + d.n[0] * d.offset, y: d.c[1] + d.h / 2, z: d.c[2] + a[2] * s + d.n[2] * d.offset });
      });
    }
    const r = def.ramp;
    if (r && this.ramp) {
      const cy = Math.cos(rampPhi);
      const sy = Math.sin(rampPhi);
      // centre of the slab (0, L/2, t/2) rotated about the hinge's x axis
      const ly = r.length / 2;
      const lz = r.t / 2;
      this.ramp.setTranslationWrtParent({ x: r.hinge[0], y: r.hinge[1] + ly * cy - lz * sy, z: r.hinge[2] + ly * sy + lz * cy });
      this.ramp.setRotationWrtParent(q(new THREE.Euler(rampPhi, 0, 0)));
    }
    const shut = !!def.shield && (anim.movers[def.shield.key] ?? 0) > 0.95;
    for (const c of this.shutters) if (c.isEnabled() !== shut) c.setEnabled(shut);
  }

  /** Closest ship surface along a world ray (panel index or -1 for other parts). */
  castRay(o: THREE.Vector3, d: THREE.Vector3, max: number, also?: (handle: number) => boolean, skipSeats = false): { t: number; panel: number } | null {
    const ray = new this.R.Ray({ x: o.x, y: o.y, z: o.z }, { x: d.x, y: d.y, z: d.z });
    const hit = this.world.castRay(ray, max, true, undefined, undefined, undefined, undefined, (c) => {
      const own = this.owners.get(c.handle);
      if (own) return !(skipSeats && own.kind === 'solid' && own.seat);
      return !!also?.(c.handle);
    });
    if (!hit) return null;
    const own = this.owners.get(hit.collider.handle);
    return { t: hit.timeOfImpact, panel: own?.kind === 'panel' ? own.index : -1 };
  }
}
