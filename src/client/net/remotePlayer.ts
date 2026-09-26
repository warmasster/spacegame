import * as THREE from 'three';
import { INTERPOLATION_DELAY_MS, SUIT_STRIPES } from '../../shared/constants';
import { StateFlags, type PlayerInfo, type PlayerState } from '../../shared/protocol';
import { Astronaut, type AstronautAsset } from '../player/astronaut';

interface Sample {
  t: number;
  s: PlayerState;
}

const _a = new THREE.Vector3();
const _b = new THREE.Vector3();

/** Another astronaut: buffered snapshots rendered ~120 ms in the past, smoothly interpolated. */
export class RemotePlayer {
  readonly astronaut: Astronaut;
  readonly position = new THREE.Vector3();
  readonly velocity = new THREE.Vector3();
  yaw = 0;
  pitch = 0;
  grounded = true;
  crouch = false;
  lamps = false;
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

  update(dt: number, serverNow: number) {
    const buf = this.buffer;
    if (!buf.length) return;
    const renderT = serverNow - INTERPOLATION_DELAY_MS;
    while (buf.length > 2 && buf[1].t <= renderT) buf.shift();
    const a = buf[0];
    const b = buf[1];
    let s: PlayerState;
    if (b && renderT >= a.t) {
      const k = Math.min(1, (renderT - a.t) / Math.max(1, b.t - a.t));
      this.position.copy(_a.fromArray(a.s.p).lerp(_b.fromArray(b.s.p), k));
      this.velocity.copy(_a.fromArray(a.s.v).lerp(_b.fromArray(b.s.v), k));
      this.yaw = lerpAngle(a.s.yaw, b.s.yaw, k);
      this.pitch = a.s.pitch + (b.s.pitch - a.s.pitch) * k;
      s = k < 0.5 ? a.s : b.s;
    } else {
      // starved: extrapolate briefly along the last velocity, then hold
      const last = buf[buf.length - 1];
      const ahead = Math.min(0.25, Math.max(0, (renderT - last.t) / 1000));
      this.position.fromArray(last.s.p).addScaledVector(_a.fromArray(last.s.v), ahead);
      this.velocity.fromArray(last.s.v);
      this.yaw = last.s.yaw;
      this.pitch = last.s.pitch;
      s = last.s;
    }
    this.grounded = (s.f & StateFlags.Grounded) !== 0;
    this.crouch = (s.f & StateFlags.Crouching) !== 0;
    this.lamps = (s.f & StateFlags.Lamps) !== 0;

    const root = this.astronaut.root;
    root.visible = true;
    if (!this.hasState) {
      root.position.copy(this.position);
      this.hasState = true;
    }
    root.position.copy(this.position);
    root.rotation.y = this.yaw;
    this.astronaut.setLamps(this.lamps);
    this.astronaut.update(dt, {
      velocity: this.velocity,
      yaw: this.yaw,
      pitch: this.pitch,
      grounded: this.grounded,
      crouch: this.crouch,
    });
  }

  dispose() {
    this.astronaut.dispose();
  }
}

function lerpAngle(a: number, b: number, k: number) {
  let d = (b - a) % (Math.PI * 2);
  if (d > Math.PI) d -= Math.PI * 2;
  if (d < -Math.PI) d += Math.PI * 2;
  return a + d * k;
}
