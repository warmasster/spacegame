import * as THREE from 'three';
import { INTERPOLATION_DELAY_MS, SUIT_STRIPES } from '../../shared/constants';
import { StateFlags, type PlayerInfo, type PlayerState } from '../../shared/protocol';
import { Astronaut, type AstronautAsset } from '../player/astronaut';
import type { CrewSounds } from '../audio/crewSounds';
import type { Frames } from '../frames/frames';
import { bodyAt, tangentFrame } from '../../shared/space/body';

interface Sample {
  t: number;
  s: PlayerState;
}

const _a = new THREE.Vector3();
const _b = new THREE.Vector3();

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
 * Another astronaut: buffered snapshots rendered ~120 ms in the past, smoothly interpolated in its
 * own frame (a crew member walking a flying ship moves with the deck, not with the network).
 */
export class RemotePlayer {
  readonly astronaut: Astronaut;
  /** World position of the feet as drawn. */
  readonly position = new THREE.Vector3();
  /** Position and velocity in its frame. */
  readonly local = new THREE.Vector3();
  readonly velocity = new THREE.Vector3();
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
  private buffer: Sample[] = [];
  private hasState = false;

  constructor(
    readonly info: PlayerInfo,
    asset: AstronautAsset,
  ) {
    this.astronaut = new Astronaut(asset);
    this.astronaut.setStripeColor(SUIT_STRIPES[info.variant % SUIT_STRIPES.length]);
    this.astronaut.root.visible = false;
  }

  push(t: number, s: PlayerState) {
    const last = this.buffer[this.buffer.length - 1];
    if (last && t <= last.t) return;
    this.buffer.push({ t, s });
    if (this.buffer.length > 40) this.buffer.shift();
  }

  /** `eye`: the camera (world): far away the suit animates without arm IK. */
  update(dt: number, serverNow: number, frames: Frames, eye?: THREE.Vector3) {
    if (eye) {
      const d2 = this.astronaut.root.position.distanceToSquared(eye);
      this.astronaut.detail = d2 < REMOTE_IK_M * REMOTE_IK_M ? 1 : 0;
      this.astronaut.setLod(d2 < LOD1_M * LOD1_M ? 0 : d2 < LOD2_M * LOD2_M ? 1 : 2);
    }
    const buf = this.buffer;
    if (!buf.length) return;
    const renderT = serverNow - INTERPOLATION_DELAY_MS;
    while (buf.length > 2 && buf[1].t <= renderT) buf.shift();
    const a = buf[0];
    const b = buf[1];
    let s: PlayerState;
    if (b && renderT >= a.t && (a.s.fr ?? 0) === (b.s.fr ?? 0)) {
      const k = Math.min(1, (renderT - a.t) / Math.max(1, b.t - a.t));
      this.local.copy(_a.fromArray(a.s.p).lerp(_b.fromArray(b.s.p), k));
      this.velocity.copy(_a.fromArray(a.s.v).lerp(_b.fromArray(b.s.v), k));
      this.yaw = lerpAngle(a.s.yaw, b.s.yaw, k);
      this.pitch = a.s.pitch + (b.s.pitch - a.s.pitch) * k;
      s = k < 0.5 ? a.s : b.s;
    } else if (b && renderT >= a.t) {
      // it changed frame between the two samples: take the one we are closer to (no blend across frames)
      s = renderT - a.t < b.t - renderT ? a.s : b.s;
      this.local.fromArray(s.p);
      this.velocity.fromArray(s.v);
      this.yaw = s.yaw;
      this.pitch = s.pitch;
    } else {
      // starved: extrapolate briefly along the last velocity, then hold
      const last = buf[buf.length - 1];
      const ahead = Math.min(0.25, Math.max(0, (renderT - last.t) / 1000));
      this.local.fromArray(last.s.p).addScaledVector(_a.fromArray(last.s.v), ahead);
      this.velocity.fromArray(last.s.v);
      this.yaw = last.s.yaw;
      this.pitch = last.s.pitch;
      s = last.s;
    }
    this.frame = s.fr ?? 0;
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
    this.astronaut.setLamps(this.lamps);
    this.astronaut.update(dt, {
      velocity: this.velocity,
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

function lerpAngle(a: number, b: number, k: number) {
  let d = (b - a) % (Math.PI * 2);
  if (d > Math.PI) d -= Math.PI * 2;
  if (d < -Math.PI) d += Math.PI * 2;
  return a + d * k;
}
