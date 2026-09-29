import { INTERPOLATION_DELAY_MS } from '../../shared/constants';
import { copyPose, extrapolatePose, hermitePose, type ShipPose } from '../../shared/ship/flight';

interface Sample {
  t: number;
  pose: ShipPose;
  landed: boolean;
  pad: boolean;
}

/** Longest a starved stream is carried forward on its last velocities (ms). */
const MAX_EXTRAPOLATE_MS = 300;

/**
 * A ship's pose as the network delivers it (the server or its pilot, ~30 Hz, server-time stamped),
 * played back a little in the past with cubic Hermite through the samples and their velocities, so
 * a turning ship draws smooth arcs instead of a polyline. Everyone aboard lives in the ship's own
 * frame, so what little error is left moves the whole ship, never its crew relative to the deck.
 */
export class PosePlayback {
  private samples: Sample[] = [];
  /** Playback clock (server ms), advanced by the fixed steps and eased toward now − delay. */
  private clock = -1;

  push(t: number, pose: ShipPose, landed: boolean, pad: boolean) {
    const last = this.samples[this.samples.length - 1];
    if (last && t <= last.t) return;
    this.samples.push({ t, pose, landed, pad });
    if (this.samples.length > 60) this.samples.shift();
  }

  get empty() {
    return this.samples.length === 0;
  }

  clear() {
    this.samples = [];
    this.clock = -1;
  }

  /**
   * One fixed step of `dt` seconds: the pose to use now (written into `out`), with the ground
   * contact flags. Null while nothing has arrived yet.
   */
  step(dt: number, serverNow: number, out: ShipPose): { landed: boolean; pad: boolean } | null {
    const s = this.samples;
    if (!s.length) return null;
    const target = serverNow - INTERPOLATION_DELAY_MS;
    if (this.clock < 0 || Math.abs(this.clock - target) > 250) this.clock = target;
    else this.clock += dt * 1000 + (target - this.clock) * 0.05;
    const t = this.clock;
    while (s.length > 2 && s[1].t <= t) s.shift();
    const a = s[0];
    const b = s[1];
    if (b && t >= a.t && t <= b.t) {
      const span = (b.t - a.t) / 1000;
      hermitePose(a.pose, b.pose, span > 0 ? (t - a.t) / (b.t - a.t) : 1, span, out);
      return t - a.t < b.t - t ? a : b;
    }
    const last = b && t > b.t ? b : a;
    if (t < last.t) {
      copyPose(out, last.pose);
      return last;
    }
    extrapolatePose(last.pose, Math.min(MAX_EXTRAPOLATE_MS, t - last.t) / 1000, out);
    return last;
  }
}
