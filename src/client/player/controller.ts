import type RAPIER from '@dimforge/rapier3d-compat';
import * as THREE from 'three';
import type { BodyDef } from '../../shared/constants';
import type { Physics } from '../world/physics';
import type { Input } from './input';

// Suit capsule: 1.80 m tall, 0.68 m wide (PLSS included).
const RADIUS = 0.34;
const HALF_HEIGHT = 0.56;
const CROUCH_HALF_HEIGHT = 0.3;

export const MOVE = {
  walk: 1.55,
  run: 3.3,
  crouch: 0.8,
  /** m/s² — low traction in 1/6 g: speeding up and braking take a moment. */
  accel: 3.4,
  brake: 2.6,
  airAccel: 0.35,
  jump: 2.05, // ≈ 1.3 m apex, ~2.5 s airtime under lunar gravity
};

/**
 * Kinematic character on Rapier: lunar gravity, inertia-heavy acceleration, auto-step onto
 * small rocks, slope limits. Mouse look is applied here too (yaw = body heading).
 */
export class PlayerController {
  readonly position = new THREE.Vector3();
  readonly velocity = new THREE.Vector3();
  yaw = 0;
  pitch = 0;
  grounded = false;
  crouch = false;
  running = false;
  private body: RAPIER.RigidBody;
  private collider: RAPIER.Collider;
  private kcc: RAPIER.KinematicCharacterController;
  private jumpLatch = 0;
  private airTime = 0;

  constructor(
    private physics: Physics,
    private gravity: number,
    spawn: THREE.Vector3,
  ) {
    const R = physics.rapier;
    const world = physics.world;
    this.body = world.createRigidBody(R.RigidBodyDesc.kinematicPositionBased().setTranslation(spawn.x, spawn.y, spawn.z));
    this.collider = world.createCollider(
      R.ColliderDesc.capsule(HALF_HEIGHT, RADIUS).setTranslation(0, HALF_HEIGHT + RADIUS, 0),
      this.body,
    );
    this.kcc = world.createCharacterController(0.03);
    this.kcc.setUp({ x: 0, y: 1, z: 0 });
    this.kcc.setMaxSlopeClimbAngle(THREE.MathUtils.degToRad(36));
    this.kcc.setMinSlopeSlideAngle(THREE.MathUtils.degToRad(42));
    this.kcc.enableAutostep(0.32, 0.15, false);
    this.kcc.enableSnapToGround(0.35);
    this.kcc.setSlideEnabled(true);
    this.position.copy(spawn);
  }

  static forBody(physics: Physics, body: BodyDef, spawn: THREE.Vector3) {
    return new PlayerController(physics, body.gravity, spawn);
  }

  teleport(p: THREE.Vector3) {
    this.position.copy(p);
    this.velocity.set(0, 0, 0);
    this.body.setTranslation({ x: p.x, y: p.y, z: p.z }, true);
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

  update(dt: number, input: Input) {
    // --- intent --------------------------------------------------------------------------
    let f = (input.down('KeyW') ? 1 : 0) - (input.down('KeyS') ? 1 : 0);
    let r = (input.down('KeyD') ? 1 : 0) - (input.down('KeyA') ? 1 : 0);
    const len = Math.hypot(f, r);
    if (len > 1) {
      f /= len;
      r /= len;
    }
    const wantCrouch = input.down('KeyC') || input.down('ControlLeft');
    if (wantCrouch !== this.crouch) this.setCrouch(wantCrouch);
    this.running = input.down('ShiftLeft') && f > 0 && !this.crouch;
    const speed = this.crouch ? MOVE.crouch : this.running ? MOVE.run : MOVE.walk;
    const sy = Math.sin(this.yaw);
    const cy = Math.cos(this.yaw);
    // forward = (-sin, 0, -cos), right = (cos, 0, -sin)
    const wx = (-sy * f + cy * r) * speed;
    const wz = (-cy * f - sy * r) * speed;

    // --- horizontal dynamics ---------------------------------------------------------------
    const v = this.velocity;
    const dvx = wx - v.x;
    const dvz = wz - v.z;
    const dvLen = Math.hypot(dvx, dvz);
    if (dvLen > 1e-5) {
      const speeding = wx * v.x + wz * v.z >= v.x * v.x + v.z * v.z - 1e-3 && (wx !== 0 || wz !== 0);
      const a = this.grounded ? (speeding ? MOVE.accel : MOVE.brake) : MOVE.airAccel;
      const k = Math.min(1, (a * dt) / dvLen);
      v.x += dvx * k;
      v.z += dvz * k;
    }

    // --- vertical ------------------------------------------------------------------------------
    if (input.consume('Space')) this.jumpLatch = 0.15;
    this.jumpLatch = Math.max(0, this.jumpLatch - dt);
    if (this.grounded && this.jumpLatch > 0 && !this.crouch) {
      v.y = MOVE.jump + (this.running ? 0.25 : 0);
      this.grounded = false;
      this.jumpLatch = 0;
      this.airTime = 0;
    }
    v.y -= this.gravity * dt;

    // --- collide & slide ---------------------------------------------------------------------------
    const desired = { x: v.x * dt, y: v.y * dt, z: v.z * dt };
    this.kcc.computeColliderMovement(this.collider, desired);
    const m = this.kcc.computedMovement();
    const wasGrounded = this.grounded;
    this.grounded = this.kcc.computedGrounded();
    this.position.x += m.x;
    this.position.y += m.y;
    this.position.z += m.z;
    this.body.setNextKinematicTranslation({ x: this.position.x, y: this.position.y, z: this.position.z });

    if (this.grounded && v.y < 0) v.y = 0;
    if (!this.grounded && desired.y > 0 && m.y < desired.y * 0.5) v.y = Math.min(v.y, 0); // bumped head
    // blocked horizontally → lose that velocity
    if (dt > 0) {
      const hx = m.x / dt;
      const hz = m.z / dt;
      if (Math.abs(hx) < Math.abs(v.x) - 0.05) v.x = hx;
      if (Math.abs(hz) < Math.abs(v.z) - 0.05) v.z = hz;
    }
    this.airTime = this.grounded ? 0 : this.airTime + dt;
    void wasGrounded;
  }

  private setCrouch(on: boolean) {
    const half = on ? CROUCH_HALF_HEIGHT : HALF_HEIGHT;
    if (!on) {
      // only stand up if there is room
      const R = this.physics.rapier;
      const shape = new R.Capsule(HALF_HEIGHT, RADIUS);
      const hit = this.physics.world.intersectionWithShape(
        { x: this.position.x, y: this.position.y + HALF_HEIGHT + RADIUS + 0.05, z: this.position.z },
        { x: 0, y: 0, z: 0, w: 1 },
        shape,
        undefined,
        undefined,
        this.collider,
      );
      if (hit) return;
    }
    this.crouch = on;
    this.collider.setShape(new this.physics.rapier.Capsule(half, RADIUS));
    this.collider.setTranslationWrtParent({ x: 0, y: half + RADIUS, z: 0 });
  }
}
