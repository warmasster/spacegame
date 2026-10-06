import { INTERPOLATION_DELAY_MS } from '../../shared/constants';
import { copyPose, type ShipPose } from '../../shared/ship/flight';
import { Replica, type ReplicaSample } from '../../shared/net/replica';

interface Contact {
  landed: boolean;
  pad: boolean;
}

/**
 * A ship's pose as the network delivers it (the server or its pilot, ~30 Hz), stamped with the
 * time of the step that made it (shared/time/stepClock.ts). It is drawn in the present like
 * everything this client simulates (shared/net/replica.ts): carried forward from the newest pose
 * with its velocity and its acceleration (the last two poses: gravity and the burn alike), and a
 * pose that disagrees with the prediction is blended in, never jumped to. Everyone aboard lives in
 * the ship's own frame, so what little error is left moves the whole ship, never its crew
 * relative to the deck.
 */
export class PosePlayback {
  private replica = new Replica({ hostDelay: INTERPOLATION_DELAY_MS, smooth: 0.3 });

  push(t: number, pose: ShipPose, landed: boolean, pad: boolean) {
    this.replica.push(sample(t, pose, landed, pad));
  }

  /**
   * Nobody sent anything yet but the ship was being drawn (we flew it until a moment ago): its
   * last pose, at the time it belongs to, is where the stream starts — no freeze, no jump.
   */
  seed(t: number, pose: ShipPose, landed: boolean, pad: boolean) {
    this.replica.seed(sample(t, pose, landed, pad));
  }

  get empty() {
    return this.replica.empty;
  }

  clear() {
    this.replica.clear();
  }

  /** Largest correction blended in so far (m) and corrections taken as jumps (diagnostics). */
  get corrections() {
    return { max: this.replica.maxCorrection, snaps: this.replica.snaps };
  }

  /** The pose at time `t` (ms, server clock: the fixed step's), written into `out`; null while nothing has arrived. */
  step(t: number, out: ShipPose): Contact | null {
    const s = this.replica.sample(t);
    if (!s) return null;
    copyPose(out, s);
    return s.tag as Contact;
  }
}

function sample(t: number, pose: ShipPose, landed: boolean, pad: boolean): ReplicaSample {
  return { t, fr: 0, p: [pose.p[0], pose.p[1], pose.p[2]], v: [pose.v[0], pose.v[1], pose.v[2]], q: [pose.q[0], pose.q[1], pose.q[2], pose.q[3]], w: [pose.w[0], pose.w[1], pose.w[2]], tag: { landed, pad } };
}
