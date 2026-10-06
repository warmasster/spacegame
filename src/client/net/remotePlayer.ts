import * as THREE from 'three';
import { INTERPOLATION_DELAY_MS, SUIT_STRIPES } from '../../shared/constants';
import { StateFlags, type PlayerInfo, type PlayerState } from '../../shared/protocol';
import { Astronaut, type AstronautAsset } from '../player/astronaut';
import type { CrewSounds } from '../audio/crewSounds';
import type { Frames } from '../frames/frames';
import { bodyAt, tangentFrame } from '../../shared/space/body';
import { Replica } from '../../shared/net/replica';
import type { Quat, V3 } from '../../shared/ship/geom';

/** Its look (yaw, pitch) as one orientation, so the replica blends and carries it like any other. */
const _e = new THREE.Euler(0, 0, 0, 'YXZ');
const _lq = new THREE.Quaternion();
const _tq = new THREE.Quaternion();

const _yaw = new THREE.Quaternion();
const _q4 = [0, 0, 0, 1];
/** A reused world point as an array. */
const _w = {
  a: [0, 0, 0],
  set3(v: THREE.Vector3) {
    this.a[0] = v.x;
    this.a[1] = v.y;
    this.a[2] = v.z;
    return this.a;
  },
};
const _up = new THREE.Vector3(0, 1, 0);

/**
 * Another astronaut, from its states (each stamped with the time of its owner's step): in a ship
 * it is drawn ~120 ms in the past, between two states in the ship's space (a crew member walking a
 * flying ship moves with the deck, not with the network); in the world it is drawn in the present,
 * like everything else there (an astronaut floating beside a ship in orbit stays beside it). The
 * replica blends away whatever a new state corrects (shared/net/replica.ts).
 */
export class RemotePlayer {
  readonly astronaut: Astronaut;
  /** World position of the feet as drawn. */
  readonly position = new THREE.Vector3();
  /** Position and velocity in its frame (frame 0: the world — its velocity the world's too). */
  readonly local = new THREE.Vector3();
  readonly velocity = new THREE.Vector3();
  /** Velocity in the axes its body is drawn in (its frame's; in the world, its local horizon's). */
  private animVelocity = new THREE.Vector3();
  /** Frame it is in (0 = the moon, else a ship id). */
  frame = 0;
  yaw = 0;
  pitch = 0;
  grounded = true;
  crouch = false;
  lamps = false;
  jetting = false;
  dead = false;
  hp = 100;
  seated = false;
  welding = false;
  /** Its states (on foot: steps are not a steady pull — only in the air is its acceleration carried). */
  private replica = new Replica({
    hostDelay: INTERPOLATION_DELAY_MS,
    smooth: 0.18,
    accel: (s, measured, out) => {
      if (((s.tag as PlayerState).f & StateFlags.Grounded) !== 0) out[0] = out[1] = out[2] = 0;
      else if (out !== measured) for (let i = 0; i < 3; i++) out[i] = measured[i];
      return out;
    },
  });
  private hasState = false;

  constructor(
    readonly info: PlayerInfo,
    asset: AstronautAsset,
  ) {
    this.astronaut = new Astronaut(asset);
    this.astronaut.setStripeColor(SUIT_STRIPES[info.variant % SUIT_STRIPES.length]);
    this.astronaut.root.visible = false;
  }

  /** A state and the time of the owner's step it belongs to (ms, server clock). */
  push(t: number, s: PlayerState) {
    const n = this.replica.newest;
    if (n && t <= n.t) return;
    _lq.setFromEuler(_e.set(s.pitch, s.yaw, 0, 'YXZ'));
    const q: Quat = [_lq.x, _lq.y, _lq.z, _lq.w];
    this.replica.push({ t, fr: s.fr ?? 0, p: [s.p[0], s.p[1], s.p[2]], v: [s.v[0], s.v[1], s.v[2]] as V3, q, w: [0, 0, 0], tag: s });
  }

  /** Largest correction blended in (m) and corrections taken as jumps (diagnostics). */
  get corrections() {
    return { max: this.replica.maxCorrection, snaps: this.replica.snaps };
  }

  /**
   * `time`: the time drawn this frame (ms, server clock: the step clock's render time). `eye`: the
   * camera (world): far away the suit animates without arm IK.
   */
  update(dt: number, time: number, frames: Frames, eye?: THREE.Vector3) {
    if (eye) {
      const d2 = this.astronaut.root.position.distanceToSquared(eye);
      this.astronaut.detail = d2 < REMOTE_IK_M * REMOTE_IK_M ? 1 : 0;
      this.astronaut.setLod(d2 < LOD1_M * LOD1_M ? 0 : d2 < LOD2_M * LOD2_M ? 1 : 2);
    }
    const r = this.replica.sample(time);
    if (!r) return;
    const s = r.tag as PlayerState;
    this.local.set(r.p[0], r.p[1], r.p[2]);
    this.velocity.set(r.v[0], r.v[1], r.v[2]);
    _e.setFromQuaternion(_lq.set(r.q[0], r.q[1], r.q[2], r.q[3]), 'YXZ');
    this.yaw = _e.y;
    this.pitch = _e.x;
    this.frame = r.fr;
    // frame 0 on the network is the world itself (each client has its own physics bubble)
    if (this.frame === 0) this.position.copy(this.local);
    else {
      const w = frames.toWorld(this.frame, [this.local.x, this.local.y, this.local.z], true);
      this.position.set(w[0], w[1], w[2]);
    }
    this.grounded = (s.f & StateFlags.Grounded) !== 0;
    this.crouch = (s.f & StateFlags.Crouching) !== 0;
    this.lamps = (s.f & StateFlags.Lamps) !== 0;
    this.jetting = (s.f & StateFlags.Jetpack) !== 0;
    this.astronaut.setDead(this.dead);
    if (s.w) this.astronaut.equip(s.w);
    this.astronaut.setArmed((s.f & StateFlags.Armed) !== 0);
    this.seated = (s.f & StateFlags.Seated) !== 0;
    this.welding = (s.f & StateFlags.Welding) !== 0;

    const root = this.astronaut.root;
    root.visible = true;
    if (!this.hasState) {
      root.position.copy(this.position);
      this.hasState = true;
    }
    root.position.copy(this.position);
    // on foot outside a ship: upright on the ground where it is (yaw from its tangent frame's north)
    const fq = this.frame === 0 ? tangentFrame(bodyAt(_w.set3(this.position)), _w.a, _q4) : frames.quat(this.frame, true);
    root.quaternion.set(fq[0], fq[1], fq[2], fq[3]).multiply(_yaw.setFromAxisAngle(_up, this.yaw));
    // the gait reads its velocity along its own horizon: in the world, turned into the tangent frame
    // where it stands (up is not +y away from the base)
    this.animVelocity.copy(this.velocity);
    if (this.frame === 0) this.animVelocity.applyQuaternion(_tq.set(fq[0], fq[1], fq[2], fq[3]).invert());
    this.astronaut.setLamps(this.lamps);
    this.astronaut.update(dt, {
      velocity: this.animVelocity,
      yaw: this.yaw,
      pitch: this.pitch,
      grounded: this.grounded,
      crouch: this.crouch,
      seated: this.seated,
    });
  }

  /** Its boots, pack and tools as heard here (client/audio). */
  sounds: CrewSounds | null = null;

  dispose() {
    this.astronaut.dispose();
    this.sounds?.dispose();
  }
}

/** Beyond this distance (m) a remote astronaut's hands no longer track the weapon grips. */
const REMOTE_IK_M = 25;
/** Mesh levels of detail of a remote suit: full up to LOD1_M, lighter up to LOD2_M, lightest beyond. */
const LOD1_M = 12;
const LOD2_M = 35;

