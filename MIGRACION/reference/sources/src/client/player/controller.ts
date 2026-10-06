import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';
import type { BodyDef } from '../../shared/constants';
import type { Physics } from '../world/physics';
import type { Input } from './input';

// Suit capsule: 1.80 m tall, 0.68 m wide (PLSS included).
const RADIUS = 0.34;
const HALF_HEIGHT = 0.56;
const CROUCH_HALF_HEIGHT = 0.3;
const MAX_CLIMB = THREE.MathUtils.degToRad(36);
/** Contact normals steeper than this (along "up") are walls, not ground. */
const WALKABLE_NY = Math.cos(MAX_CLIMB) - 0.01;
/** Max horizontal force an astronaut can push loose objects with (N). */
const PUSH_FORCE = 380;
/** Pull along the deck (m/s²) at which the boots stop gripping at all. */
const WIND_GRIP = 7;
/** Below this apparent gravity (m/s²) "up" stays where it was: floating. */
const FLOAT_G = 0.05;

export const MOVE = {
  walk: 1.55,
  run: 3.3,
  crouch: 0.8,
  /** m/s² — low traction in 1/6 g: speeding up and braking take a moment. */
  accel: 3.4,
  brake: 2.6,
  airAccel: 0.35,
  jump: 2.05, // ≈ 1.3 m apex, ~2.5 s airtime under lunar gravity
  /** Jetpack: thrust acceleration (> lunar g so it climbs), burn time and ground recharge. */
  jetAccel: 3.3,
  jetBurn: 4.5,
  jetRecharge: 7,
  jetAirAccel: 4.5,
  /** Horizontal speed the jetpack can push you to. */
  jetSpeed: 7,
};

const _v = new THREE.Vector3();
const _fwd = new THREE.Vector3();
const _right = new THREE.Vector3();
const _wish = new THREE.Vector3();
const _vt = new THREE.Vector3();

/**
 * Kinematic character on Rapier: inertia-heavy acceleration in low gravity, auto-step onto small
 * rocks, slope limits, jetpack. It lives in one reference frame at a time (the moon, or a ship's
 * interior world) and everything here is in that frame's coordinates: position, velocity, the
 * look (yaw = body heading about the frame's +Y) and the gravity it feels, which sets "up" (on the
 * moon, straight up; aboard, the deck's up blended with the ship's motion — see ShipSpace).
 */
export class PlayerController {
  readonly position = new THREE.Vector3();
  /** Interpolated position for rendering (set by the game each frame), frame coordinates. */
  readonly renderPosition = new THREE.Vector3();
  readonly velocity = new THREE.Vector3();
  /** Gravity felt in the frame (m/s²); the game sets it every step. */
  readonly gravity: THREE.Vector3;
  /** "Up" for walking: against the gravity felt (kept while floating). */
  readonly up = new THREE.Vector3(0, 1, 0);
  /** Frame the astronaut is in: 0 = the moon, else a ship's id. */
  frame = 0;
  yaw = 0;
  pitch = 0;
  grounded = false;
  crouch = false;
  running = false;
  /** Jetpack firing this frame. */
  jetting = false;
  /** Jetpack propellant 0..1. */
  fuel = 1;
  /** Inputs ignored (dead). */
  disabled = false;
  /** Sitting: pinned to this spot (feet, frame coordinates), no walking; set/cleared by the game. */
  seat: { pos: THREE.Vector3; yaw: number } | null = null;
  /** Collider under the feet after the last step (its handle), or null. */
  groundHandle: number | null = null;
  /** Colliders the capsule ran into this step (handles), for the game (loose crates to take over). */
  readonly touched = new Set<number>();
  /**
   * Walkable surfaces the capsule leaned on this step (handles). The foot of a ramp or of the
   * boarding stairs holds the capsule up by its front before the feet are over it.
   */
  readonly steppedOn = new Set<number>();
  /** A collider the capsule must pass through (the crate in your hands). */
  ignore: number | null = null;
  /**
   * Steady pull in the frame (m/s²): the air rushing to a breach. Set by the game every step. On
   * the deck the boots' traction (the braking toward what you want) holds against a weak one.
   */
  readonly wind = new THREE.Vector3();
  private world: RAPIER.World;
  private body!: RAPIER.RigidBody;
  private collider!: RAPIER.Collider;
  private kcc!: RAPIER.KinematicCharacterController;
  private jumpLatch = 0;
  private airTime = 0;
  private filter = (c: RAPIER.Collider) => c.handle !== this.ignore;

  constructor(
    private R: typeof RAPIER,
    world: RAPIER.World,
    gravity: number,
    spawn: THREE.Vector3,
  ) {
    this.world = world;
    this.gravity = new THREE.Vector3(0, -gravity, 0);
    this.position.copy(spawn);
    this.build();
  }

  static forBody(physics: Physics, body: BodyDef, spawn: THREE.Vector3) {
    return new PlayerController(physics.rapier, physics.world, body.gravity, spawn);
  }

  /** Body, capsule and character controller in the current world. */
  private build() {
    const R = this.R;
    const p = this.position;
    const half = this.crouch ? CROUCH_HALF_HEIGHT : HALF_HEIGHT;
    this.body = this.world.createRigidBody(R.RigidBodyDesc.kinematicPositionBased().setTranslation(p.x, p.y, p.z));
    this.collider = this.world.createCollider(R.ColliderDesc.capsule(half, RADIUS).setTranslation(0, half + RADIUS, 0), this.body);
    this.kcc = this.world.createCharacterController(0.03);
    this.kcc.setUp({ x: this.up.x, y: this.up.y, z: this.up.z });
    this.kcc.setMaxSlopeClimbAngle(MAX_CLIMB);
    this.kcc.setMinSlopeSlideAngle(THREE.MathUtils.degToRad(42));
    this.kcc.enableAutostep(0.32, 0.15, false);
    this.kcc.enableSnapToGround(0.35);
    this.kcc.setSlideEnabled(true);
  }

  /** The capsule's collider (to keep it out of other queries). */
  get handle() {
    return this.collider.handle;
  }

  /**
   * Move into another frame: its world, a position and a velocity there (the caller converts them
   * so the motion carries on unbroken), and the heading measured in that frame.
   */
  moveTo(frame: number, world: RAPIER.World, p: THREE.Vector3, v: THREE.Vector3, yaw: number) {
    this.world.removeCharacterController(this.kcc);
    this.world.removeRigidBody(this.body);
    this.world = world;
    this.frame = frame;
    this.position.copy(p);
    this.renderPosition.copy(p);
    this.velocity.copy(v);
    this.yaw = yaw;
    this.groundHandle = null;
    this.build();
  }

  /**
   * Just moved into this world (moveTo): lift the capsule clear of whatever it overlaps, up to
   * `max` m. Rapier's controller can't move a capsule that starts inside another shape — or
   * closer than its skin (0.03 m) to the ground — so arriving on the moon at the foot of the stairs
   * (still touching them, the feet at ground level) would leave it stuck until a jump.
   */
  unstick(max = 0.4, skin = 0.035) {
    const half = this.crouch ? CROUCH_HALF_HEIGHT : HALF_HEIGHT;
    // the capsule sunk by `skin`: clear means nothing within the controller's skin under the feet
    const shape = new this.R.Capsule(half, RADIUS);
    const up = this.up;
    const p = this.position;
    for (let lift = 0; lift <= max; lift += 0.01) {
      const k = half + RADIUS + lift - skin;
      const c = { x: p.x + up.x * k, y: p.y + up.y * k, z: p.z + up.z * k };
      if (this.world.intersectionWithShape(c, this.body.rotation(), shape, undefined, undefined, this.collider, undefined, this.filter)) continue;
      if (lift > 0) {
        p.addScaledVector(up, lift);
        this.renderPosition.copy(p);
        this.body.setTranslation({ x: p.x, y: p.y, z: p.z }, true);
        this.body.setNextKinematicTranslation({ x: p.x, y: p.y, z: p.z });
      }
      return lift;
    }
    return max;
  }

  teleport(p: THREE.Vector3) {
    this.position.copy(p);
    this.velocity.set(0, 0, 0);
    this.body.setTranslation({ x: p.x, y: p.y, z: p.z }, true);
    this.body.setNextKinematicTranslation({ x: p.x, y: p.y, z: p.z });
  }

  look(input: Input, freeLook?: { orbit: number; orbitPitch: number }) {
    const [dx, dy] = input.look();
    if (freeLook && input.down('AltLeft')) {
      freeLook.orbit -= dx;
      freeLook.orbitPitch = THREE.MathUtils.clamp(freeLook.orbitPitch - dy, -1.2, 1.2);
      return;
    }
    if (freeLook) {
      // ease the free-look camera back behind the astronaut
      freeLook.orbit *= 0.9;
      freeLook.orbitPitch *= 0.9;
    }
    this.yaw -= dx;
    this.pitch = THREE.MathUtils.clamp(this.pitch - dy, -1.5, 1.45);
  }

  /** External velocity change in the frame (explosion knockback). */
  impulse(dv: THREE.Vector3) {
    this.velocity.add(dv);
    if (dv.dot(this.up) > 0.3) this.grounded = false;
  }

  update(dt: number, input: Input) {
    this.touched.clear();
    this.steppedOn.clear();
    if (this.seat) {
      this.position.copy(this.seat.pos);
      this.velocity.set(0, 0, 0);
      this.body.setTranslation({ x: this.position.x, y: this.position.y, z: this.position.z }, true);
      this.body.setNextKinematicTranslation({ x: this.position.x, y: this.position.y, z: this.position.z });
      this.grounded = true;
      this.jetting = this.running = false;
      this.fuel = Math.min(1, this.fuel + dt / MOVE.jetRecharge);
      return;
    }
    const g = this.gravity.length();
    if (g > FLOAT_G) this.up.copy(this.gravity).multiplyScalar(-1 / g);
    const up = this.up;
    this.kcc.setUp({ x: up.x, y: up.y, z: up.z });
    const off = this.disabled;
    // --- intent --------------------------------------------------------------------------
    let f = off ? 0 : (input.down('KeyW') ? 1 : 0) - (input.down('KeyS') ? 1 : 0);
    let r = off ? 0 : (input.down('KeyD') ? 1 : 0) - (input.down('KeyA') ? 1 : 0);
    const len = Math.hypot(f, r);
    if (len > 1) {
      f /= len;
      r /= len;
    }
    const wantCrouch = input.down('KeyC') || input.down('ControlLeft');
    if (wantCrouch !== this.crouch) this.setCrouch(wantCrouch);
    this.running = input.down('ShiftLeft') && f > 0 && !this.crouch;
    const speed = this.jetting ? MOVE.jetSpeed : this.crouch ? MOVE.crouch : this.running ? MOVE.run : MOVE.walk;
    // heading in the frame, laid on the ground plane: forward = (-sin, 0, -cos), right = (cos, 0, -sin)
    const sy = Math.sin(this.yaw);
    const cy = Math.cos(this.yaw);
    _fwd.set(-sy, 0, -cy).addScaledVector(up, -(-sy * up.x - cy * up.z)).normalize();
    _right.set(cy, 0, -sy).addScaledVector(up, -(cy * up.x - sy * up.z)).normalize();
    _wish.copy(_fwd).multiplyScalar(f * speed).addScaledVector(_right, r * speed);

    // --- along the ground ------------------------------------------------------------------
    const v = this.velocity;
    let vn = v.dot(up);
    const vUp0 = vn;
    _vt.copy(v).addScaledVector(up, -vn);
    _v.copy(_wish).sub(_vt);
    const dvLen = _v.length();
    if (this.grounded && dvLen > 1e-5) {
      const speeding = _wish.dot(_vt) >= _vt.lengthSq() - 1e-3 && (f !== 0 || r !== 0);
      let a = speeding ? MOVE.accel : MOVE.brake;
      // a gale along the deck: boots in 1/6 g grip little, and less the harder it pulls
      if (this.wind.lengthSq() > 1e-6) {
        const wn = this.wind.dot(up);
        const wh = Math.sqrt(Math.max(0, this.wind.lengthSq() - wn * wn));
        a *= Math.max(0.1, 1 - wh / WIND_GRIP);
      }
      _vt.addScaledVector(_v, Math.min(1, (a * dt) / dvLen));
    } else if (!this.grounded && (f !== 0 || r !== 0)) {
      // in the air there is nothing to push against but the pack: a push along the wish, up to
      // its speed — never a brake on the motion already there. That motion is measured in this
      // frame, and the frame may be anything (a ship, the ground under a ship flying past at
      // 250 m/s): braking toward it would be a force out of nowhere.
      const ws = _wish.length();
      if (ws > 1e-5) {
        const dir = _v.copy(_wish).divideScalar(ws);
        const along = _vt.dot(dir);
        const a = this.jetting ? MOVE.jetAirAccel : MOVE.airAccel;
        if (along < ws) _vt.addScaledVector(dir, Math.min(ws - along, a * dt));
      }
    }

    // --- along "up" ----------------------------------------------------------------------------
    if (input.consume('Space') && !off) this.jumpLatch = 0.15;
    this.jumpLatch = Math.max(0, this.jumpLatch - dt);
    if (this.grounded && this.jumpLatch > 0 && !this.crouch) {
      vn = MOVE.jump + (this.running ? 0.25 : 0);
      this.grounded = false;
      this.jumpLatch = 0;
      this.airTime = 0;
    }
    // jetpack: hold Space while airborne
    this.jetting = !off && !this.grounded && input.down('Space') && this.fuel > 0 && this.airTime > 0.22;
    if (this.jetting) {
      vn += MOVE.jetAccel * dt;
      this.fuel = Math.max(0, this.fuel - dt / MOVE.jetBurn);
    } else if (this.grounded) this.fuel = Math.min(1, this.fuel + dt / MOVE.jetRecharge);
    vn -= g * dt;
    // the pack climbs to 9 m/s at most; what the body already had (leaving a climbing ship) it keeps
    if (this.jetting) vn = Math.min(vn, Math.max(9, vUp0));
    v.copy(_vt).addScaledVector(up, vn);
    // the air: next step the traction takes back what it can (up to the brake), the rest drags you
    if (this.wind.lengthSq() > 1e-8) {
      v.addScaledVector(this.wind, dt);
      if (this.wind.dot(up) > g) this.grounded = false;
    }

    // --- collide & slide ---------------------------------------------------------------------------
    const desired = { x: v.x * dt, y: v.y * dt, z: v.z * dt };
    this.kcc.computeColliderMovement(this.collider, desired, undefined, undefined, this.filter);
    const m = this.kcc.computedMovement();
    this.grounded = this.kcc.computedGrounded();
    this.position.x += m.x;
    this.position.y += m.y;
    this.position.z += m.z;
    this.body.setNextKinematicTranslation({ x: this.position.x, y: this.position.y, z: this.position.z });

    const vUp = v.dot(up);
    if (this.grounded && vUp < 0) v.addScaledVector(up, -vUp);
    const dUp = desired.x * up.x + desired.y * up.y + desired.z * up.z;
    const mUp = m.x * up.x + m.y * up.y + m.z * up.z;
    if (!this.grounded && dUp > 0 && mUp < dUp * 0.5) v.addScaledVector(up, -Math.max(0, v.dot(up))); // bumped head
    // blocked by a wall → lose the velocity going into it (keep the slide along it). Walkable
    // slopes (a ship's ramp) keep it: clamping to the achieved motion compounded every step on a
    // slope, and clamping per axis skewed the heading on slopes not aligned with the axes.
    for (let i = 0; i < this.kcc.numComputedCollisions(); i++) {
      const hit = this.kcc.computedCollision(i);
      const n = hit?.normal1;
      if (!hit || !n) continue;
      if (hit.collider) this.touched.add(hit.collider.handle);
      const nUp = n.x * up.x + n.y * up.y + n.z * up.z;
      if (nUp > WALKABLE_NY) {
        if (hit.collider) this.steppedOn.add(hit.collider.handle);
        continue;
      }
      // loose objects (cargo): shove them instead of stopping dead
      const other = hit.collider?.parent();
      if (other && other.isDynamic()) {
        this.push(other, n, dt);
        continue;
      }
      _v.set(n.x - up.x * nUp, n.y - up.y * nUp, n.z - up.z * nUp);
      const hl = _v.length();
      if (hl < 1e-4) continue;
      _v.divideScalar(hl);
      const into = v.dot(_v);
      if (into < 0) v.addScaledVector(_v, -into);
    }
    this.airTime = this.grounded ? 0 : this.airTime + dt;
    this.groundHandle = this.grounded ? this.probeGround() : null;
  }

  /** What the feet stand on: a short ray down from inside the capsule's bottom. */
  private probeGround(): number | null {
    const up = this.up;
    const o = { x: this.position.x + up.x * 0.3, y: this.position.y + up.y * 0.3, z: this.position.z + up.z * 0.3 };
    const ray = new this.R.Ray(o, { x: -up.x, y: -up.y, z: -up.z });
    const hit = this.world.castRay(ray, 0.7, true, undefined, undefined, this.collider, undefined, this.filter);
    return hit ? hit.collider.handle : null;
  }

  /** A suited astronaut (~180 kg, boots on regolith) leaning into a loose body: bounded force. */
  private push(body: RAPIER.RigidBody, n: { x: number; y: number; z: number }, dt: number) {
    const up = this.up;
    const nUp = n.x * up.x + n.y * up.y + n.z * up.z;
    _v.set(-(n.x - up.x * nUp), -(n.y - up.y * nUp), -(n.z - up.z * nUp));
    const hl = _v.length();
    if (hl < 1e-4) return;
    _v.divideScalar(hl);
    const want = Math.max(0, this.velocity.dot(_v));
    if (want <= 0) return;
    const bv = body.linvel();
    const gap = want - (bv.x * _v.x + bv.y * _v.y + bv.z * _v.z);
    if (gap <= 0) return;
    const j = Math.min(gap * body.mass() * 0.5, PUSH_FORCE * dt);
    body.applyImpulse({ x: _v.x * j, y: _v.y * j, z: _v.z * j }, true);
  }

  private setCrouch(on: boolean) {
    const half = on ? CROUCH_HALF_HEIGHT : HALF_HEIGHT;
    if (!on) {
      // only stand up if there is room
      const shape = new this.R.Capsule(HALF_HEIGHT, RADIUS);
      const hit = this.world.intersectionWithShape(
        { x: this.position.x, y: this.position.y + HALF_HEIGHT + RADIUS + 0.05, z: this.position.z },
        { x: 0, y: 0, z: 0, w: 1 },
        shape,
        undefined,
        undefined,
        this.collider,
        undefined,
        this.filter,
      );
      if (hit) return;
    }
    this.crouch = on;
    this.collider.setShape(new this.R.Capsule(half, RADIUS));
    this.collider.setTranslationWrtParent({ x: 0, y: half + RADIUS, z: 0 });
  }
}
