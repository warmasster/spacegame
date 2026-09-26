import * as THREE from 'three';
import type { Astronaut, BoneName } from './astronaut';

/**
 * Joint diagnostics for the astronaut rig.
 *
 * Every bone is measured RELATIVE TO ITS REST POSE, on the three model axes the animation code
 * poses with (see Astronaut.pose):
 *
 *   X  flex   — flexion / extension (pitch; bending forward/back: elbow, knee, nod)
 *   Y  twist  — rotation along the vertical/limb axis (yaw of the torso, pronation of forearm,
 *               roll of an arm hanging down)
 *   Z  side   — abduction / adduction, lateral bend (raising an arm sideways, leaning)
 *
 * The local delta  D = M^T · (rest⁻¹ · current) · M  (M = model axes in bone space, so every
 * bone reads the same regardless of how Blender oriented it) is split swing–twist:
 * twist = rotation of D about Y; swing = the rest, as a rotation vector whose X and Z components
 * are flex and side. Unlike Euler angles this never flips near 90°. Degrees.
 *
 * Speeds: `speed` is LOCAL (relative to the parent bone) — what a joint actually does, used for
 * limits; `worldSpeed` includes the whole body's motion (turning, falling).
 *
 * Also tracked per bone: angular speed in world space (°/s) and its peak, the direction of the
 * bone (world, unit) and violations of LIMITS — the range of motion of a pressurised EVA suit
 * (roughly NASA EMU mobility, generous). A violation means the pose code produced something a
 * suit (or a body) cannot do; an angular-speed spike means a pop/snap between frames.
 */
export interface JointLimit {
  /** Max |angle| per axis, degrees: [flex, twist, side]. */
  range: [number, number, number];
  /** Max plausible angular speed, °/s (above = visible pop). */
  maxSpeed: number;
}

export const LIMITS: Record<BoneName, JointLimit> = {
  pelvis: { range: [45, 70, 30], maxSpeed: 600 },
  spine: { range: [30, 25, 20], maxSpeed: 500 },
  chest: { range: [30, 25, 20], maxSpeed: 500 },
  neck: { range: [5, 5, 5], maxSpeed: 200 }, // helmet is fixed to the torso
  clavicleL: { range: [20, 20, 25], maxSpeed: 500 },
  clavicleR: { range: [20, 20, 25], maxSpeed: 500 },
  upperarmL: { range: [150, 90, 110], maxSpeed: 900 },
  upperarmR: { range: [150, 90, 110], maxSpeed: 900 },
  forearmL: { range: [145, 100, 20], maxSpeed: 900 },
  forearmR: { range: [145, 100, 20], maxSpeed: 900 },
  handL: { range: [75, 45, 45], maxSpeed: 1000 },
  handR: { range: [75, 45, 45], maxSpeed: 1000 },
  thighL: { range: [110, 40, 45], maxSpeed: 900 },
  thighR: { range: [110, 40, 45], maxSpeed: 900 },
  shinL: { range: [145, 12, 8], maxSpeed: 1000 },
  shinR: { range: [145, 12, 8], maxSpeed: 1000 },
  footL: { range: [50, 20, 30], maxSpeed: 1000 },
  footR: { range: [50, 20, 30], maxSpeed: 1000 },
  toeL: { range: [45, 8, 8], maxSpeed: 1000 },
  toeR: { range: [45, 8, 8], maxSpeed: 1000 },
};

export interface JointSample {
  bone: BoneName;
  /** Angles vs rest on model axes, degrees. */
  flex: number;
  twist: number;
  side: number;
  /** Local (vs parent) angular speed, °/s, this frame; peak since reset; world angular speed. */
  speed: number;
  worldSpeed: number;
  peakSpeed: number;
  /** Bone direction (head → child), world, unit. */
  dir: [number, number, number];
  /** Which limits are exceeded, e.g. ['flex', 'speed']. */
  violations: string[];
}

const AXES = ['flex', 'twist', 'side'] as const;

export class JointDiagnostics {
  private prevWorld = new Map<BoneName, THREE.Quaternion>();
  private prevLocal = new Map<BoneName, THREE.Quaternion>();
  private peak = new Map<BoneName, number>();
  /** Worst value per bone and axis since reset (for automated runs). */
  readonly worst = new Map<BoneName, { flex: number; twist: number; side: number; speed: number }>();
  last: JointSample[] = [];
  private m = new THREE.Matrix4();
  private mq = new THREE.Quaternion();
  private q = new THREE.Quaternion();

  constructor(private astro: Astronaut) {}

  reset() {
    this.prevWorld.clear();
    this.prevLocal.clear();
    this.peak.clear();
    this.worst.clear();
  }

  /** Measure all joints. Call after the astronaut's update (pose + IK) for this frame. */
  sample(dt: number): JointSample[] {
    const out: JointSample[] = [];
    for (const [name, r] of Object.entries(this.astro.rig) as Array<[BoneName, (typeof this.astro.rig)[BoneName]]>) {
      // local delta from rest, re-expressed on the model axes
      this.m.makeBasis(r.ax, r.ay, r.az);
      this.mq.setFromRotationMatrix(this.m);
      this.q.copy(r.rest).invert().multiply(r.bone.quaternion);
      this.q.premultiply(this.mq.clone().invert()).multiply(this.mq);
      const deg = THREE.MathUtils.radToDeg;
      const q = this.q;
      if (q.w < 0) q.set(-q.x, -q.y, -q.z, -q.w);
      const tw = new THREE.Quaternion(0, q.y, 0, q.w).normalize();
      const sw = q.clone().multiply(tw.clone().invert());
      const swAng = 2 * Math.acos(Math.min(1, Math.abs(sw.w)));
      const sn = Math.sqrt(1 - sw.w * sw.w) || 1;
      const sgn = sw.w < 0 ? -1 : 1;
      const ang = {
        flex: deg((sw.x / sn) * swAng * sgn),
        twist: deg(2 * Math.atan2(tw.y, tw.w)),
        side: deg((sw.z / sn) * swAng * sgn),
      };
      // angular speeds: local (joint motion) and world
      const wq = r.bone.getWorldQuaternion(new THREE.Quaternion());
      const prev = this.prevWorld.get(name);
      const worldSpeed = prev && dt > 0 ? deg(prev.angleTo(wq)) / dt : 0;
      this.prevWorld.set(name, wq);
      const lq = r.bone.quaternion.clone();
      const prevL = this.prevLocal.get(name);
      const speed = prevL && dt > 0 ? deg(prevL.angleTo(lq)) / dt : 0;
      this.prevLocal.set(name, lq);
      const peak = Math.max(this.peak.get(name) ?? 0, speed);
      this.peak.set(name, peak);
      const lim = LIMITS[name];
      const violations: string[] = [];
      AXES.forEach((a, i) => {
        if (Math.abs(ang[a]) > lim.range[i]) violations.push(a);
      });
      if (speed > lim.maxSpeed) violations.push('speed');
      const child = r.bone.children.find((c) => (c as THREE.Bone).isBone);
      const head = r.bone.getWorldPosition(new THREE.Vector3());
      const dir = child ? child.getWorldPosition(new THREE.Vector3()).sub(head).normalize() : new THREE.Vector3(0, 1, 0).applyQuaternion(wq);
      const w = this.worst.get(name) ?? { flex: 0, twist: 0, side: 0, speed: 0 };
      this.worst.set(name, {
        flex: Math.max(w.flex, Math.abs(ang.flex)),
        twist: Math.max(w.twist, Math.abs(ang.twist)),
        side: Math.max(w.side, Math.abs(ang.side)),
        speed: Math.max(w.speed, speed),
      });
      out.push({
        bone: name,
        flex: +ang.flex.toFixed(1),
        twist: +ang.twist.toFixed(1),
        side: +ang.side.toFixed(1),
        speed: Math.round(speed),
        worldSpeed: Math.round(worldSpeed),
        peakSpeed: Math.round(peak),
        dir: [+dir.x.toFixed(3), +dir.y.toFixed(3), +dir.z.toFixed(3)],
        violations,
      });
    }
    this.last = out;
    return out;
  }

  /** Worst-case summary since reset, with limit violations, for automated checks. */
  summary() {
    return [...this.worst.entries()].map(([bone, w]) => {
      const lim = LIMITS[bone];
      const bad: string[] = [];
      AXES.forEach((a, i) => {
        if (w[a] > lim.range[i]) bad.push(`${a} ${w[a].toFixed(0)}° > ${lim.range[i]}°`);
      });
      if (w.speed > lim.maxSpeed) bad.push(`speed ${w.speed.toFixed(0)}°/s > ${lim.maxSpeed}`);
      return { bone, flex: Math.round(w.flex), twist: Math.round(w.twist), side: Math.round(w.side), speed: Math.round(w.speed), ok: !bad.length, bad };
    });
  }

  /** Text table for the F6 overlay. */
  table() {
    const pad = (v: number | string, n: number) => String(v).padStart(n);
    const rows = this.last.map((s) => {
      const flag = s.violations.length ? ` ⚠ ${s.violations.join(',')}` : '';
      return `${s.bone.padEnd(10)}${pad(s.flex, 7)}${pad(s.twist, 7)}${pad(s.side, 7)}${pad(s.speed, 6)}${pad(s.peakSpeed, 6)}${flag}`;
    });
    return [`${'hueso'.padEnd(10)}${pad('flex', 7)}${pad('twist', 7)}${pad('side', 7)}${pad('°/s', 6)}${pad('pico', 6)}`, ...rows].join('\n');
  }
}

/**
 * Per-bone RGB axis gizmos (X red = flex axis, Y green = twist, Z blue = side), drawn on top.
 * Shows the MODEL axes each joint is measured on, at each bone's current position.
 */
export class JointGizmos {
  readonly group = new THREE.Group();
  private helpers = new Map<BoneName, THREE.AxesHelper>();

  constructor(private astro: Astronaut, size = 0.08) {
    for (const name of Object.keys(astro.rig) as BoneName[]) {
      const h = new THREE.AxesHelper(size);
      const mat = h.material as THREE.Material;
      mat.depthTest = false;
      h.renderOrder = 999;
      this.helpers.set(name, h);
      this.group.add(h);
    }
    this.group.visible = false;
  }

  update() {
    if (!this.group.visible) return;
    const mq = new THREE.Quaternion();
    for (const [name, h] of this.helpers) {
      const r = this.astro.rig[name];
      r.bone.getWorldPosition(h.position);
      // model axes carried by the bone: world = boneWorld · (rest⁻¹ basis)
      r.bone.getWorldQuaternion(h.quaternion);
      mq.setFromRotationMatrix(new THREE.Matrix4().makeBasis(r.ax, r.ay, r.az));
      h.quaternion.multiply(mq);
    }
  }
}
