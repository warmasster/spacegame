import * as THREE from 'three';
import { bodyAt } from '../../shared/space/body';
import { launch, localAt, LOST, stepBallistic, WORLD_FRAME, type Ballistic, type BallisticEnv, type Contact } from '../../shared/frames';
import { projectileById, type ProjectileDef } from '../../shared/items';
import { dirToWorld, pointVelocity, toWorld } from '../../shared/ship/flight';
import type { V3 } from '../../shared/ship/geom';
import type { Frames } from '../frames/frames';
import type { ShipClient } from '../ship/ship';
import type { Particles } from './particles';
import { PROJECTILE_LOOKS, type ProjectileLook } from './projectileLooks';
import { lights } from '../render/lightPool';
import { sfx, type Loop } from '../audio/engine';
import type { Place } from '../audio/medium';

interface Flying {
  /** Stable number (diagnostics: client/diag/motionProbe.ts). */
  id: number;
  b: Ballistic;
  def: ProjectileDef;
  look: ProjectileLook;
  mesh: THREE.Object3D;
  /** Its motor or its whistle while it flies (heard wherever something carries it), if it has one. */
  sound: Loop | null;
}

/** Where a projectile of ours stopped: its kind and the contact (reported to the server). */
export interface Impact {
  k: string;
  c: Contact;
}

const _sp: [number, number, number] = [0, 0, 0];
const _dir = new THREE.Vector3();
const _vw = new THREE.Vector3();
const _back = new THREE.Vector3();
const _Z = new THREE.Vector3(0, 0, 1);
const _q = new THREE.Vector3();
const _sweepA: V3 = [0, 0, 0];
const _sweepB: V3 = [0, 0, 0];

/**
 * Projectiles in flight, of every kind of the projectile catalog (shared/items/projectiles.ts):
 * the flight itself is the shared ballistic core (shared/frames/ballistic.ts: frames, gravity,
 * hand-over between a ship and the world), this is what it flies through on this client (the
 * ships' colliders, the ground, the crew) and how it is drawn and heard (fx/projectileLooks.ts).
 * Every client simulates every shot (same start = same arc); only the shooter reports where it
 * stopped, the server turns that into the impact for everyone.
 *
 * The group hangs from the render origin's root: meshes are placed in world coordinates.
 */
export class Projectiles {
  readonly group = new THREE.Group();
  private list: Flying[] = [];
  /** Where the last blast flashed (its light comes from the scene's pool while it fades). */
  private flashAt = new THREE.Vector3();
  private flash = 0;
  private flashPower = 400;
  /** Meshes out of use, per look, ready for the next shot. */
  private spare = new Map<string, THREE.Object3D[]>();
  /** Where between the last two steps the last frame was drawn. */
  private alpha = 0;
  /** Where a world point is, acoustically (client/audio/director.ts); unset: silent projectiles. */
  placeAt: ((p: readonly number[], out: Place) => Place) | null = null;
  /** What they fly through here (the crew is filled in every step). */
  private env: BallisticEnv & { crew: Array<{ id: number; p: V3 }> };
  private nextId = 1;
  /** Told when one changes frame (leaves a ship through a door, flies into one): diagnostics. */
  onFrame: ((id: number, kind: string, from: number, to: number) => void) | null = null;
  /** Length of a fixed step (s): a shot heard late is flown forward in these. */
  step = 1 / 60;

  constructor(
    private ground: (p: readonly number[]) => number,
    private particles: Particles,
    private frames: Frames,
    private ships: () => ShipClient[],
  ) {
    this.env = {
      host: (fr) => this.frames.ship(fr),
      hosts: () => this.ships(),
      hostGravity: (host, out) => {
        const g = this.frames.gravity(host.id);
        out[0] = g.x;
        out[1] = g.y;
        out[2] = g.z;
        return out;
      },
      sweepHost: (host, a, b) => this.frames.ship(host.id)?.localHit(a, b) ?? null,
      sweepWorld: (a, b, start, end) => this.sweepShips(a, b, start, end),
      groundAlt: (p) => this.ground(p),
      crew: [],
    };
  }

  /** Every hull along a world segment: the first one it meets. */
  private sweepShips(a: V3, b: V3, start: number, end: number): Contact | null {
    const len = Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let best: Contact | null = null;
    let bestT = Infinity;
    for (const ship of this.ships()) {
      // nowhere near its hull (its whole bounds, wings and all): no ray
      const c = ship.sim.pose.p;
      const bd = ship.sim.def.bounds;
      const r = Math.hypot(Math.max(-bd.min[0], bd.max[0]), Math.max(-bd.min[1], bd.max[1]), Math.max(-bd.min[2], bd.max[2]));
      const travel = Math.hypot(c[0] - ship.prev.p[0], c[1] - ship.prev.p[1], c[2] - ship.prev.p[2]);
      if (Math.hypot(c[0] - a[0], c[1] - a[1], c[2] - a[2]) > r + len + travel + 1) continue;
      localAt(ship, a, start, _sweepA);
      localAt(ship, b, end, _sweepB);
      const l = ship.localHit(_sweepA, _sweepB);
      if (!l) continue;
      const dx = _sweepB[0] - _sweepA[0], dy = _sweepB[1] - _sweepA[1], dz = _sweepB[2] - _sweepA[2];
      const dd = dx * dx + dy * dy + dz * dz;
      const t = dd > 1e-12 ? ((l[0] - _sweepA[0]) * dx + (l[1] - _sweepA[1]) * dy + (l[2] - _sweepA[2]) * dz) / dd : 0;
      if (t < bestT) {
        bestT = t;
        best = { p: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t], fr: ship.id, l };
      }
    }
    return best;
  }

  /**
   * A projectile of kind `kind` leaves a launcher. `fr`: a ship's id — `o`, `d` and `v` in its
   * space, `v` the launcher's velocity relative to it — or WORLD_FRAME: the world (`v` its world
   * velocity; `prev`, where the muzzle was the step before, so the shooter sees it leave as drawn).
   * `ahead` (s): it left that long ago (someone else's shot, heard a network trip later) — it is
   * flown forward to the present, where the shooter is drawn too (shared/net/replica.ts), instead
   * of starting a trip behind them (80 m at 1.6 km/s).
   */
  spawn(owner: number, kind: string, fr: number, o: V3, d: V3, v?: V3, prev?: V3, ahead = 0) {
    const def = projectileById(kind);
    const look = def && PROJECTILE_LOOKS[def.look];
    // a kind or a ship this client doesn't have: nowhere to put it
    if (!def || !look || (fr !== WORLD_FRAME && !this.frames.ship(fr))) return;
    const mesh = this.spare.get(def.look)?.pop() ?? look.mesh();
    this.group.add(mesh);
    const f: Flying = { id: this.nextId++, b: launch(owner, kind, fr, o, d, def.speed, v, prev), def, look, mesh, sound: def.sounds?.flight ? sfx.loop(def.sounds.flight) : null };
    this.list.push(f);
    if (ahead > 0 && !this.catchUp(f, Math.min(ahead, def.life, 0.5))) return;
    this.place(f, this.alpha);
    // the flash at the muzzle as drawn, moving with what launched it
    const nose = this.noseWorld(f, _dir);
    const carry = this.velWorld(f, _vw).addScaledVector(nose, -def.speed);
    look.launch?.(this.particles, mesh.position, nose, carry);
  }

  /**
   * Fly a shot forward `s` seconds in fixed steps (it left that long ago). False when it stopped on
   * the way (the server's impact brings the effect) — it is gone.
   */
  private catchUp(f: Flying, s: number): boolean {
    for (let t = 0; t < s - 1e-6; t += this.step) {
      const r = stepBallistic(f.b, f.def, Math.min(this.step, s - t), this.env);
      if (r) {
        this.remove(this.list.indexOf(f));
        return false;
      }
    }
    return true;
  }

  /** Every projectile as drawn this frame (world): its number and where (diagnostics). */
  forEachDrawn(fn: (id: number, p: THREE.Vector3) => void) {
    for (const f of this.list) fn(f.id, f.mesh.position);
  }

  /**
   * Once per fixed step, after the ships' poses for this step are set. `crew`: the astronauts'
   * centres (world). Returns where projectiles owned by `me` stopped (to report).
   */
  update(dt: number, me: number, crew: Array<{ id: number; p: V3 }>): Impact[] {
    const out: Impact[] = [];
    this.flash = Math.max(0, this.flash - dt * 5);
    this.env.crew = crew;
    for (let i = this.list.length - 1; i >= 0; i--) {
      const f = this.list[i];
      const was = f.b.fr;
      const r = stepBallistic(f.b, f.def, dt, this.env);
      if (f.b.fr !== was) this.onFrame?.(f.id, f.b.kind, was, f.b.fr);
      if (r === LOST) {
        this.remove(i);
        continue;
      }
      if (r) {
        if (f.b.owner === me) out.push({ k: f.b.kind, c: r });
        // everyone's stops here; the server's impact brings the effect
        this.remove(i);
        continue;
      }
      if (f.b.age > f.def.life) this.remove(i);
    }
    return out;
  }

  /**
   * An impact confirmed by the server at a world point: the owner's nearest projectile of that
   * kind goes, and it shows as its kind says. No kind: a blast of a ship's own (a tank going up).
   * `carry`: the velocity of what it happened against (a ship in flight), for the debris cloud.
   */
  impact(owner: number, kind: string | undefined, at: THREE.Vector3, carry?: THREE.Vector3) {
    const def = kind ? projectileById(kind) : undefined;
    if (def) {
      let best = -1;
      let bestD = 40;
      for (let i = 0; i < this.list.length; i++) {
        const f = this.list[i];
        if (f.b.owner !== owner || f.b.kind !== def.id) continue;
        const d = this.worldOf(f, _q).distanceTo(at);
        if (d < bestD) {
          bestD = d;
          best = i;
        }
      }
      if (best >= 0) this.remove(best);
    }
    _sp[0] = at.x;
    _sp[1] = at.y;
    _sp[2] = at.z;
    if (!def || def.impact.fx === 'blast') {
      const below = Math.max(0, this.ground(_sp)) + 0.3;
      this.particles.explosion(at, below, 1, carry);
      this.flashUp(at, 1.5, 400);
      return;
    }
    // a hit: sparks off whatever it struck, a glint
    for (let k = 0; k < 10; k++) {
      this.particles.emit('glow', {
        pos: at,
        vel: _back.randomDirection().multiplyScalar(1 + Math.random() * 4),
        carry,
        color: [4, 2.6, 1.2],
        life: 0.05 + Math.random() * 0.2,
        size: 0.02 + Math.random() * 0.02,
        gravity: 1,
      });
    }
    this.flashUp(at, 0.2, 12);
  }

  /** The flash a little above the point ("above": away from the body), for the light pool. */
  private flashUp(at: THREE.Vector3, up: number, power: number) {
    const b = bodyAt(_sp);
    this.flashAt.set(at.x - b.center[0], at.y - b.center[1], at.z - b.center[2]).setLength(up).add(at);
    this.flash = 1;
    this.flashPower = power;
  }

  /** Where one is in the world this step (`out`). */
  private worldOf(f: Flying, out: THREE.Vector3) {
    const ship = f.b.fr === WORLD_FRAME ? null : this.frames.ship(f.b.fr);
    return ship ? out.set(...toWorld(ship.sim.pose, f.b.p)) : out.set(f.b.p[0], f.b.p[1], f.b.p[2]);
  }

  /** Its nose in the world, as drawn (`out`). */
  private noseWorld(f: Flying, out: THREE.Vector3) {
    const ship = f.b.fr === WORLD_FRAME ? null : this.frames.ship(f.b.fr);
    const n = ship ? dirToWorld(ship.render, f.b.nose) : f.b.nose;
    return out.set(n[0], n[1], n[2]);
  }

  /** Its world velocity, as drawn (`out`). */
  private velWorld(f: Flying, out: THREE.Vector3) {
    const b = f.b;
    const ship = b.fr === WORLD_FRAME ? null : this.frames.ship(b.fr);
    if (!ship) return out.set(b.v[0], b.v[1], b.v[2]);
    const pv = pointVelocity(ship.render, b.p);
    const dv = dirToWorld(ship.render, b.v);
    return out.set(pv[0] + dv[0], pv[1] + dv[1], pv[2] + dv[2]);
  }

  /** The mesh between its last two steps: through its ship as drawn, or in the world. */
  private place(f: Flying, alpha: number) {
    const b = f.b;
    const m = f.mesh.position;
    m.set(b.prev[0] + (b.p[0] - b.prev[0]) * alpha, b.prev[1] + (b.p[1] - b.prev[1]) * alpha, b.prev[2] + (b.p[2] - b.prev[2]) * alpha);
    if (b.fr === WORLD_FRAME) return;
    const ship = this.frames.ship(b.fr);
    if (ship) ship.world([m.x, m.y, m.z], m);
  }

  /**
   * Once per rendered frame: each one between its last two steps (like everything else that is
   * drawn), its trail and sound, and the last impact's flash for the light pool.
   */
  frame(alpha: number) {
    this.alpha = alpha;
    for (const f of this.list) {
      this.place(f, alpha);
      const at = f.mesh.position;
      if (this.placeAt && f.sound) {
        _sp[0] = at.x;
        _sp[1] = at.y;
        _sp[2] = at.z;
        this.placeAt(_sp, f.sound.place);
        f.sound.level = 1;
      }
      // the nose where it was aimed (nothing turns it); no lookAt: matrices are in render space, positions in the world
      const nose = this.noseWorld(f, _dir);
      f.mesh.quaternion.setFromUnitVectors(_Z, nose);
      f.look.trail?.(this.particles, _back.copy(nose).multiplyScalar(-f.look.tail).add(at), nose, this.velWorld(f, _vw));
    }
    if (this.flash > 0) lights.point(this.flashAt, 0xffb070, this.flash * this.flash * this.flashPower, 18, 2);
  }

  private remove(i: number) {
    const f = this.list[i];
    f.sound?.release();
    this.group.remove(f.mesh);
    let pool = this.spare.get(f.def.look);
    if (!pool) this.spare.set(f.def.look, (pool = []));
    pool.push(f.mesh);
    this.list.splice(i, 1);
  }
}
