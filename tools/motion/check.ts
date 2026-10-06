// Motion continuity, without a browser (npm run test:motion). Everything that makes things "jump
// back and forth" at speed, measured the way the eye sees it: for each rendered frame, how far a
// thing is from where its last two frames said it would be, relative to a camera that moves
// smoothly with it (client/diag/motionProbe.ts measures the same in the game, F7).
//
//   1. Network ship stream (server with a jittery timer → network with jitter → client at 144 Hz
//      with hitches): the old pipeline (wall-clock stamps, 120 ms playback) against the new one
//      (step-clock stamps, present-time replica), at 250 and 1600 m/s along a curved path with a burn.
//   2. The pilot leaves the helm: the stream is seeded with our own last pose, the server takes over
//      from its replica of our reports (no freeze, no jump back).
//   3. A body leaving a ship through a door at 1.6 km/s (ship turning), and the physics bubble
//      re-laid under a body: the drawn history carried (PoseTrack) against snapping it.
//   4. Frame hand-over timing with Rapier: a crate handed from a ship to the bubble before the
//      bodies caught up with the frames (old) against at the end of the step (new).
//   5. The membership rule never flips a crate back and forth.
//   6. A shot heard late is flown forward to the present; the step times survive the codec.
//
// Exit code 1 if any check fails. `--verbose` prints the per-case numbers.

import assert from 'node:assert/strict';
import RAPIER from '@dimforge/rapier3d-compat';
import { StepClock } from '../../src/shared/time/stepClock.js';
import { Replica } from '../../src/shared/net/replica.js';
import { PosePlayback } from '../../src/client/net/posePlayback.js';
import { PoseTrack, carry, joinsHost, leavesHost, launch, stepBallistic, type BallisticEnv, type FramePair, type Presence } from '../../src/shared/frames/index.js';
import { clonePose, copyPose, hermitePose, extrapolatePose, lerpPose, qSpin, toWorld, toLocal, type ShipPose } from '../../src/shared/ship/flight/pose.js';
import { MOON_BODY, gravityAt } from '../../src/shared/space/body.js';
import { decodeClient, encodeClient } from '../../src/shared/wire.js';
import { StateFlags } from '../../src/shared/protocol.js';
import type { V3 } from '../../src/shared/ship/geom.js';

const verbose = process.argv.includes('--verbose');
const H = 1 / 60;
const HMS = 1000 / 60;
let failures = 0;
const results: Array<{ name: string; ok: boolean; info: Record<string, unknown> }> = [];
function check(name: string, ok: boolean, info: Record<string, unknown>) {
  results.push({ name, ok, info });
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}`, verbose || !ok ? info : '');
}

/** Deterministic noise. */
function rng(seed: number) {
  let s = seed >>> 0 || 1;
  return () => ((s = (s * 16807) % 2147483647) / 2147483647);
}

/** The eye's measure: distance of each point from the constant-velocity continuation of the two before (m). */
function jumps(r: V3[], dts: number[]): number[] {
  const out: number[] = [];
  for (let i = 2; i < r.length; i++) {
    const k = dts[i] / dts[i - 1];
    const px = r[i - 1][0] + (r[i - 1][0] - r[i - 2][0]) * k;
    const py = r[i - 1][1] + (r[i - 1][1] - r[i - 2][1]) * k;
    const pz = r[i - 1][2] + (r[i - 1][2] - r[i - 2][2]) * k;
    out.push(Math.hypot(r[i][0] - px, r[i][1] - py, r[i][2] - pz));
  }
  return out;
}
const max = (a: number[]) => a.reduce((m, x) => Math.max(m, x), 0);
const p99 = (a: number[]) => [...a].sort((x, y) => x - y)[Math.floor(a.length * 0.99)] ?? 0;
const mm = (m: number) => `${(m * 1000).toFixed(2)} mm`;

// ---------------------------------------------------------------------------------------------
// A ship's true path: curving round the Moon at `speed`, with a burn along its track.
// ---------------------------------------------------------------------------------------------

function truePath(speed: number, seconds: number) {
  const dt = 0.001;
  const n = Math.ceil(seconds / dt) + 2;
  const P = new Float64Array(n * 3);
  const Vv = new Float64Array(n * 3);
  const c = MOON_BODY.center;
  const r = MOON_BODY.radius + 15000;
  let p: V3 = [c[0] + r, c[1], c[2]];
  let v: V3 = [0, 0, speed];
  const g: V3 = [0, 0, 0];
  for (let i = 0; i < n; i++) {
    P.set(p, i * 3);
    Vv.set(v, i * 3);
    const t = i * dt;
    // holding the curve (as if flying it) plus a 6 m/s² burn from 4 s to 6 s
    const dx = p[0] - c[0], dy = p[1] - c[1], dz = p[2] - c[2];
    const rr = Math.hypot(dx, dy, dz);
    const vv = Math.hypot(...v);
    const ac = (vv * vv) / rr;
    gravityAt(MOON_BODY, p, g);
    const burn = t > 4 && t < 6 ? 6 : 0;
    for (let k = 0; k < 3; k++) v[k] += (-([dx, dy, dz][k] / rr) * ac + (v[k] / vv) * burn) * dt;
    p = [p[0] + v[0] * dt, p[1] + v[1] * dt, p[2] + v[2] * dt];
  }
  return (t: number): ShipPose => {
    const f = Math.max(0, Math.min(n - 2, t / dt));
    const i = Math.floor(f), a = f - i;
    const at = (k: number) => P[i * 3 + k] + (P[(i + 1) * 3 + k] - P[i * 3 + k]) * a;
    const vt = (k: number) => Vv[i * 3 + k] + (Vv[(i + 1) * 3 + k] - Vv[i * 3 + k]) * a;
    return { p: [at(0), at(1), at(2)], v: [vt(0), vt(1), vt(2)], q: [0, 0, 0, 1], w: [0, 0, 0] };
  };
}

// ---------------------------------------------------------------------------------------------
// The old stream, as it was: stamped with the server's wall clock, played back 120 ms late.
// ---------------------------------------------------------------------------------------------

class LegacyPlayback {
  private samples: Array<{ t: number; pose: ShipPose }> = [];
  private clock = -1;
  push(t: number, pose: ShipPose) {
    const last = this.samples[this.samples.length - 1];
    if (last && t <= last.t) return;
    this.samples.push({ t, pose });
    if (this.samples.length > 60) this.samples.shift();
  }
  step(dt: number, serverNow: number, out: ShipPose) {
    const s = this.samples;
    if (!s.length) return;
    const target = serverNow - 120;
    if (this.clock < 0 || Math.abs(this.clock - target) > 250) this.clock = target;
    else this.clock += dt * 1000 + (target - this.clock) * 0.05;
    const t = this.clock;
    while (s.length > 2 && s[1].t <= t) s.shift();
    const a = s[0], b = s[1];
    if (b && t >= a.t && t <= b.t) return void hermitePose(a.pose, b.pose, (t - a.t) / (b.t - a.t), (b.t - a.t) / 1000, out);
    const last = b && t > b.t ? b : a;
    if (t < last.t) return void copyPose(out, last.pose);
    extrapolatePose(last.pose, Math.min(300, t - last.t) / 1000, out);
  }
}

// ---------------------------------------------------------------------------------------------
// 1. Network ship stream
// ---------------------------------------------------------------------------------------------

interface StreamOpts {
  speed: number;
  legacy: boolean;
  seconds?: number;
  seed?: number;
}

/** A server flying a ship on a jittery timer, a jittery network, a client at 144 Hz with hitches. */
function streamRun(o: StreamOpts) {
  const seconds = o.seconds ?? 10;
  const truth = truePath(o.speed, seconds + 1);
  const rnd = rng(o.seed ?? 7);
  // --- server: setInterval on Windows (~15.6 ms ± 20 %), fixed 60 Hz steps, a pose every 2 ---
  const W0 = 5000; // wall time the server's simulation starts (ms)
  const packets: Array<{ arrive: number; t: number; pose: ShipPose }> = [];
  const clock = new StepClock(HMS);
  clock.sync(W0);
  let wall = W0, acc = 0, steps = 0;
  while (wall < W0 + seconds * 1000) {
    const prev = wall;
    wall += 15.6 * (0.8 + rnd() * 0.4);
    acc = Math.min(acc + (wall - prev) / 1000, 0.25);
    while (acc >= H) {
      acc -= H;
      steps++;
      clock.step();
      const pose = truth(steps * H);
      // old: the wall clock when the timer fired; new: the time of the step's state
      const t = o.legacy ? wall : clock.t;
      if (steps % 2 === 0) {
        const late = 20 + rnd() * 10 + (rnd() < 0.03 ? 40 : 0);
        packets.push({ arrive: wall + late, t, pose });
      }
    }
    clock.sync(wall - acc * 1000);
  }
  packets.sort((a, b) => a.arrive - b.arrive);
  // --- client: its estimate of the server's clock wanders a few ms every couple of seconds ---
  const legacy = new LegacyPlayback();
  const replica = new PosePlayback();
  const cclock = new StepClock(HMS);
  let offsetErr = 0;
  let cw = W0 + 200;
  cclock.sync(cw + offsetErr);
  let cacc = 0, k = 0;
  const pose = clonePose(truth(0));
  const prev = clonePose(pose);
  const drawn: V3[] = [];
  const dts: number[] = [];
  const errs: number[] = [];
  let nextWander = cw + 2000;
  while (cw < W0 + seconds * 1000 - 100) {
    // frames at 144 Hz ± 10 %, a 40 ms hitch every ~2 s
    const dt = rnd() < 0.004 ? 40 : (1000 / 144) * (0.9 + rnd() * 0.2);
    cw += dt;
    if (cw > nextWander) {
      offsetErr = (rnd() - 0.5) * 6;
      nextWander += 2000;
    }
    const serverNow = cw + offsetErr;
    while (k < packets.length && packets[k].arrive <= cw) {
      const pk = packets[k++];
      if (o.legacy) legacy.push(pk.t, pk.pose);
      else replica.push(pk.t, pk.pose, false, false);
    }
    cacc = Math.min(cacc + Math.min(dt, 50) / 1000, H * 6);
    while (cacc >= H) {
      cacc -= H;
      copyPose(prev, pose);
      cclock.step();
      if (o.legacy) legacy.step(H, serverNow, pose);
      else replica.step(cclock.t, pose);
    }
    const alpha = cacc / H;
    cclock.sync(serverNow - alpha * HMS);
    const x = lerpPose(prev, pose, alpha).p;
    // where it truly was at the time this frame draws (the old pipeline draws 120 ms back on purpose)
    const T = cclock.renderTime(alpha) - (o.legacy ? 120 : 0);
    const ideal = truth((T - W0) / 1000).p;
    if (cw > W0 + 1500) {
      // relative to a camera flying smoothly along the true path: what the eye sees
      drawn.push([x[0] - ideal[0], x[1] - ideal[1], x[2] - ideal[2]]);
      dts.push(dt);
      errs.push(Math.hypot(x[0] - ideal[0], x[1] - ideal[1], x[2] - ideal[2]));
    }
  }
  const j = jumps(drawn, dts);
  return { maxJump: max(j), p99Jump: p99(j), maxError: max(errs), frames: drawn.length, corrections: o.legacy ? null : replica.corrections };
}

for (const speed of [250, 1600]) {
  const old = streamRun({ speed, legacy: true });
  const now = streamRun({ speed, legacy: false });
  if (verbose) console.log(`  ${speed} m/s  antes: salto máx ${mm(old.maxJump)} (p99 ${mm(old.p99Jump)})   ahora: salto máx ${mm(now.maxJump)} (p99 ${mm(now.p99Jump)}), error de posición máx ${mm(now.maxError)}`);
  check(`nave por red a ${speed} m/s: sin vaivén (salto por fotograma < 5 mm)`, now.maxJump < 0.005, { antes: mm(old.maxJump), ahora: mm(now.maxJump), p99: mm(now.p99Jump), errorMax: mm(now.maxError), correcciones: now.corrections });
  check(`nave por red a ${speed} m/s: el sistema antiguo sí vibraba (la prueba ve el fallo)`, old.maxJump > 20 * now.maxJump, { antes: mm(old.maxJump), ahora: mm(now.maxJump) });
}

// ---------------------------------------------------------------------------------------------
// 2. The pilot leaves the helm
// ---------------------------------------------------------------------------------------------

{
  // we fly it (local, exact) at 1600 m/s; at 3 s we stand up: our stream is seeded with our own
  // pose, the server takes over from its replica of our reports (a trip old) carried to its
  // present, and from then on flies it on (whatever error the take-over had, it keeps)
  const truth = truePath(1600, 12);
  const rnd = rng(11);
  const W0 = 1000;
  const oneWay = 25;
  const reports = new Replica({ hostDelay: 0 });
  const ours = new PosePlayback();
  const cclock = new StepClock(HMS);
  cclock.sync(W0);
  const sclock = new StepClock(HMS);
  sclock.sync(W0);
  const pose = clonePose(truth(0));
  const prev = clonePose(pose);
  const offset: V3 = [0, 0, 0];
  const drawn: V3[] = [];
  const dts: number[] = [];
  const mail: Array<{ at: number; fn: () => void }> = [];
  const post = (at: number, fn: () => void) => mail.push({ at, fn });
  let cw = W0, cacc = 0, steps = 0, sSteps = 0, flying = true, serverFlies = false;
  while (cw < W0 + 8000) {
    const dt = (1000 / 144) * (0.9 + rnd() * 0.2);
    cw += dt;
    // the server: steps with the wall clock, flies it once our "leave" arrives
    while ((sSteps + 1) * HMS <= cw - W0) {
      sSteps++;
      sclock.step();
      const tp = truth(sSteps * H);
      if (!serverFlies && !flying && cw - W0 > 3000 + oneWay) {
        serverFlies = true;
        const s = reports.sample(sclock.t)!;
        for (let i = 0; i < 3; i++) offset[i] = s.p[i] - tp.p[i];
      }
      if (serverFlies && sSteps % 2 === 0) {
        const sp = clonePose(tp);
        for (let i = 0; i < 3; i++) sp.p[i] += offset[i];
        const t = sclock.t;
        post(cw + oneWay, () => ours.push(t, sp, false, false));
      }
    }
    for (let i = 0; i < mail.length; ) {
      if (mail[i].at <= cw) mail.splice(i, 1)[0].fn();
      else i++;
    }
    cacc += dt / 1000;
    while (cacc >= H) {
      cacc -= H;
      copyPose(prev, pose);
      cclock.step();
      steps++;
      if (flying) {
        copyPose(pose, truth(steps * H));
        if (steps % 2 === 0) {
          const rep = clonePose(pose);
          const t = cclock.t;
          post(cw + oneWay, () => reports.push({ t, fr: 0, p: rep.p, v: rep.v, q: rep.q, w: rep.w }));
        }
        if (cw - W0 > 3000) {
          flying = false;
          ours.seed(cclock.t, pose, false, false);
        }
      } else ours.step(cclock.t, pose);
    }
    const alpha = cacc / H;
    cclock.sync(cw - alpha * HMS);
    const x = lerpPose(prev, pose, alpha).p;
    const ideal = truth((cclock.renderTime(alpha) - W0) / 1000).p;
    if (cw > W0 + 1000) {
      drawn.push([x[0] - ideal[0], x[1] - ideal[1], x[2] - ideal[2]]);
      dts.push(dt);
    }
  }
  const j = jumps(drawn, dts);
  check('el piloto se levanta a 1600 m/s: la nave sigue sin congelarse ni saltar atrás (< 2 cm por fotograma)', max(j) < 0.02, { saltoMax: mm(max(j)), errorAlTomarElServidor: mm(Math.hypot(...offset)), correcciones: ours.corrections });
}

// ---------------------------------------------------------------------------------------------
// 3. Drawn history across a door and across a re-laid bubble
// ---------------------------------------------------------------------------------------------

{
  // a ship at 1.6 km/s turning at 0.3 rad/s; a body walks out of it at 5 m/s and is handed to a
  // Galilean frame (the physics bubble) moving 30 m/s off the ship's velocity
  const ship: FramePair & { pose: ShipPose; prev: ShipPose } = { pose: { p: [1e6, 2e5, -3e5], q: [0, 0, 0, 1], v: [1600, 0, 0], w: [0, 0.3, 0] }, prev: null! };
  ship.prev = clonePose(ship.pose);
  const bub: FramePair & { pose: ShipPose; prev: ShipPose } = { pose: { p: [1e6 + 3, 2e5, -3e5], q: [0, 0, 0, 1], v: [1570, 5, 0], w: [0, 0, 0] }, prev: null! };
  bub.prev = clonePose(bub.pose);
  const run = (snap: boolean) => {
    const sh = { pose: clonePose(ship.pose), prev: clonePose(ship.prev) };
    const bb = { pose: clonePose(bub.pose), prev: clonePose(bub.prev) };
    const track = new PoseTrack(1);
    let l: V3 = [0, 0, 0];
    let lv: V3 = [0, 0, 5];
    let fr = 1;
    track.snap(l, [0, 0, 0, 1]);
    const rnd = rng(3);
    const drawn: V3[] = [];
    const dts: number[] = [];
    let t = 0, acc = 0;
    // camera: fixed on the ship's deck (a crew member watching it go out)
    const eyeL: V3 = [0, 1.6, -3];
    for (let f = 0; f < 400; f++) {
      const dt = (1 / 144) * (0.9 + rnd() * 0.2);
      t += dt;
      acc += dt;
      while (acc >= H) {
        acc -= H;
        copyPose(sh.prev, sh.pose);
        copyPose(bb.prev, bb.pose);
        for (let i = 0; i < 3; i++) sh.pose.p[i] += sh.pose.v[i] * H;
        sh.pose.q = qSpin(sh.pose.q, sh.pose.w, H);
        for (let i = 0; i < 3; i++) bb.pose.p[i] += bb.pose.v[i] * H;
        // the body moves in its frame (no forces), then the frame rule
        for (let i = 0; i < 3; i++) l[i] += lv[i] * H;
        track.push(l[0], l[1], l[2]);
        if (fr === 1 && l[2] > 3) {
          const n = carry(sh.pose, bb.pose, l, lv);
          l = n.p;
          lv = n.v;
          if (snap) {
            track.fr = 0;
            track.snap(l);
          } else track.carry(sh, bb, 0);
          fr = 0;
        }
      }
      const a = acc / H;
      const shipR = lerpPose(sh.prev, sh.pose, a);
      const frameR = fr === 1 ? shipR : lerpPose(bb.prev, bb.pose, a);
      const x = toWorld(frameR, track.at(a, [0, 0, 0]) as V3);
      const eye = toWorld(shipR, eyeL);
      // from when it is already walking (its first step is a real start, not a hand-over)
      if (f < 20) continue;
      drawn.push([x[0] - eye[0], x[1] - eye[1], x[2] - eye[2]]);
      dts.push(dt);
    }
    return max(jumps(drawn, dts));
  };
  const before = run(true);
  const after = run(false);
  check('cuerpo sale por la puerta a 1600 m/s con la nave girando: la historia viaja con él (< 2 mm)', after < 0.002 && before > 50 * after, { antes: mm(before), ahora: mm(after) });
}

{
  // the physics bubble moving with the player in orbit (1.6 km/s); the player slowly accelerates
  // off it until it is re-laid round them (40 m/s of drift). A crate floating nearby, 8 m/s off
  // the player sideways, is carried across: both of its steps (new), or snapped with the bubble's
  // interpolation reset as it was (old)
  const { Bubble } = await import('../../src/client/frames/bubble.js');
  const run = (old: boolean) => {
    const b = new Bubble(() => null, H);
    const c = MOON_BODY.center;
    const r = MOON_BODY.radius + 90000;
    const p0: V3 = [c[0] + r, c[1], c[2]];
    b.follow(p0, [0, 0, 1600]);
    const pp: V3 = [...p0];
    const pv: V3 = [0, 0, 1600];
    const wp: V3 = [p0[0] + 2, p0[1], p0[2]];
    const wv: V3 = [8, 0, 1600];
    const track = new PoseTrack(0);
    track.snap(toLocal(b.pose, wp));
    const rnd = rng(5);
    const drawn: V3[] = [];
    const dts: number[] = [];
    const cam: V3 = [p0[0], p0[1], p0[2]];
    const camPrev: V3 = [...cam];
    let acc = 0, relays = 0;
    for (let f = 0; f < 1600; f++) {
      const dt = (1 / 144) * (0.9 + rnd() * 0.2);
      acc += dt;
      while (acc >= H) {
        acc -= H;
        b.step(H);
        pv[2] += 5 * H;
        for (let i = 0; i < 3; i++) pp[i] += pv[i] * H;
        for (let i = 0; i < 3; i++) wp[i] += wv[i] * H;
        for (let i = 0; i < 3; i++) camPrev[i] = cam[i];
        cam[2] += 1600 * H;
        const l = toLocal(b.pose, wp);
        track.push(l[0], l[1], l[2]);
        // re-laid round the player at the end of the step (the 'frames' system)
        if (b.follow(pp, pv)) {
          relays++;
          if (old) {
            copyPose(b.prev, b.pose);
            track.snap(toLocal(b.pose, wp));
          } else track.carry({ pose: b.from, prev: b.fromPrev }, b, 0);
        }
      }
      const a = acc / H;
      b.frame(a);
      const x = toWorld(b.render, track.at(a, [0, 0, 0]) as V3);
      if (f < 20) continue;
      drawn.push([x[0] - (camPrev[0] + (cam[0] - camPrev[0]) * a), x[1] - (camPrev[1] + (cam[1] - camPrev[1]) * a), x[2] - (camPrev[2] + (cam[2] - camPrev[2]) * a)]);
      dts.push(dt);
    }
    return { jump: max(jumps(drawn, dts)), relays };
  };
  const before = run(true);
  const after = run(false);
  check('burbuja de física recolocada (40 m/s de deriva): lo dibujado no salta (< 2 mm)', after.relays > 0 && after.jump < 0.002 && before.jump > 10 * after.jump, { recolocaciones: after.relays, antes: mm(before.jump), ahora: mm(after.jump) });
}

// ---------------------------------------------------------------------------------------------
// 4. Hand-over timing with Rapier: before the bodies caught up with the frames (old) vs after (new)
// ---------------------------------------------------------------------------------------------

await RAPIER.init();
{
  // the bubble lies still on the ground; the ship flies through it at 250 m/s; a crate inside the
  // ship drifts to the door at 2 m/s and leaves. Its world speed must stay 252 m/s through the door.
  const run = (late: boolean) => {
    const bubble = new RAPIER.World({ x: 0, y: 0, z: 0 });
    const inside = new RAPIER.World({ x: 0, y: 0, z: 0 });
    const shipPose: ShipPose = { p: [0, 0, 0], q: [0, 0, 0, 1], v: [250, 0, 0], w: [0, 0, 0] };
    const still: ShipPose = { p: [0, 0, 0], q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0] };
    let body = inside.createRigidBody(RAPIER.RigidBodyDesc.dynamic().setTranslation(0, 0, 0).setLinvel(2, 0, 0));
    inside.createCollider(RAPIER.ColliderDesc.cuboid(0.3, 0.3, 0.3), body);
    let fr = 1;
    const world: number[] = [];
    for (let s = 0; s < 120; s++) {
      // frames first (the ship moves)…
      for (let i = 0; i < 3; i++) shipPose.p[i] += shipPose.v[i] * H;
      const handOver = () => {
        const t = body.translation(), v = body.linvel();
        const n = carry(shipPose, still, [t.x, t.y, t.z], [v.x, v.y, v.z]);
        inside.removeRigidBody(body);
        body = bubble.createRigidBody(RAPIER.RigidBodyDesc.dynamic().setTranslation(...n.p).setLinvel(...n.v));
        bubble.createCollider(RAPIER.ColliderDesc.cuboid(0.3, 0.3, 0.3), body);
        fr = 0;
      };
      // …old: the crate changed frame here, its body still a step behind the ship
      if (late && fr === 1 && body.translation().x > 1) handOver();
      bubble.step();
      inside.step();
      // …new: after the worlds stepped, everything at the same time
      if (!late && fr === 1 && body.translation().x > 1) handOver();
      const t = body.translation();
      const w = fr === 1 ? toWorld(shipPose, [t.x, t.y, t.z]) : [t.x, t.y, t.z];
      world.push(w[0]);
    }
    // world positions step by step: every step should advance 252/60 m
    let worst = 0;
    for (let i = 1; i < world.length; i++) worst = Math.max(worst, Math.abs(world[i] - world[i - 1] - 252 * H));
    return worst;
  };
  const before = run(true);
  const after = run(false);
  check('caja sale de una nave a 250 m/s (Rapier): el paso de marco no la adelanta ni la retrasa (< 1 mm)', after < 0.001 && before > 1, { antes: `${before.toFixed(3)} m`, ahora: mm(after) });
}

// ---------------------------------------------------------------------------------------------
// 5. Membership: no back-and-forth
// ---------------------------------------------------------------------------------------------

{
  const p = (o: Partial<Presence>): Presence => ({ inside: false, supported: false, relSpeed: 0, grounded: false, ...o });
  // just thrown out of a door, grazing the hull at 5 m/s: stays in the world
  let fr = 0, flips = 0, age = 0;
  for (let s = 0; s < 120; s++) {
    age += H;
    const pres = p({ supported: s % 2 === 0, relSpeed: 5 });
    const next = fr === 0 ? (joinsHost(pres, age) ? 1 : 0) : leavesHost(pres) ? 0 : 1;
    if (next !== fr) {
      flips++;
      age = 0;
    }
    fr = next;
  }
  check('caja lanzada rozando el casco: no la recaptura la nave', flips === 0, { cambios: flips });
  // resting on the ramp outside: joins once, stays
  fr = 0;
  flips = 0;
  age = 0;
  for (let s = 0; s < 120; s++) {
    age += H;
    const pres = p({ supported: true, relSpeed: 0.1 });
    const next = fr === 0 ? (joinsHost(pres, age) ? 1 : 0) : leavesHost(pres) ? 0 : 1;
    if (next !== fr) {
      flips++;
      age = 0;
    }
    fr = next;
  }
  check('caja quieta sobre la rampa: entra en la nave una sola vez', flips === 1, { cambios: flips });
  // on the ground touching a landing pad: the ground has it (no flip-flop)
  fr = 0;
  flips = 0;
  for (let s = 0; s < 120; s++) {
    const pres = p({ supported: true, relSpeed: 0, grounded: true });
    const next = fr === 0 ? (joinsHost(pres, 1) ? 1 : 0) : leavesHost(pres) ? 0 : 1;
    if (next !== fr) flips++;
    fr = next;
  }
  check('caja en el suelo tocando una pata: se queda en el mundo', flips === 0, { cambios: flips });
}

// ---------------------------------------------------------------------------------------------
// 6. Late shots, codec
// ---------------------------------------------------------------------------------------------

{
  const env: BallisticEnv = {
    host: () => undefined,
    hosts: () => [],
    hostGravity: (_h, out) => out,
    sweepHost: () => null,
    sweepWorld: () => null,
    groundAlt: () => 1e5,
    crew: [],
  };
  const c = MOON_BODY.center;
  const o: V3 = [c[0] + MOON_BODY.radius + 20000, c[1], c[2]];
  const here = launch(1, 'rocket', 0, o, [0, 0, 1], 60, [0, 0, 1600]);
  const late = launch(1, 'rocket', 0, o, [0, 0, 1], 60, [0, 0, 1600]);
  const spec = { gravity: 1, radius: 0.5 };
  for (let i = 0; i < 6; i++) stepBallistic(here, spec, H, env);
  // heard 100 ms later: flown forward in the same steps
  for (let i = 0; i < 6; i++) stepBallistic(late, spec, H, env);
  const d = Math.hypot(here.p[0] - late.p[0], here.p[1] - late.p[1], here.p[2] - late.p[2]);
  check('disparo recibido tarde: adelantado al presente coincide con el del tirador', d < 1e-6, { diferencia: d });

  const s = { p: [1.7e6, -3, 2] as V3, v: [1600, 0, 0] as V3, yaw: 0.3, pitch: 0.1, f: StateFlags.Grounded };
  const data = encodeClient({ type: 'state', t: 123456.789, s })!;
  const back = decodeClient(new DataView(data));
  assert.ok(back && back.type === 'state');
  check('estado del astronauta: el tiempo de su paso viaja en binario', back.type === 'state' && back.t === 123456.789, { t: back.type === 'state' ? back.t : null });
}

console.log(failures ? `\n${failures} fallo(s)` : '\nTodo suave.');
process.exit(failures ? 1 : 0);
