import * as THREE from 'three';
import { origin } from './origin';

/** A spot light asked for this frame (world space). */
interface SpotAsk {
  pos: THREE.Vector3;
  dir: THREE.Vector3;
  color: THREE.Color;
  intensity: number;
  distance: number;
  angle: number;
  penumbra: number;
  decay: number;
  score: number;
}

/** A point light asked for this frame (world space). */
interface PointAsk {
  pos: THREE.Vector3;
  color: THREE.Color;
  intensity: number;
  distance: number;
  decay: number;
  score: number;
}

const _cam = new THREE.Vector3();

/**
 * The scene's real (three.js) spot and point lights: a fixed number, made once. Every emitter —
 * helmet lamps, a ship's landing floodlight, a rocket's flash — asks for a light each frame, and
 * the ones that matter most to the camera (bright and close) get the slots; the rest wait.
 *
 * In three.js every light enters the light loop of every lit material (the terrain included) and a
 * change in their number recompiles every shader. With the pool the number never changes — a
 * player joining or a ship arriving costs nothing — and each lit pixel pays for a bounded set.
 * Cabin lights are a separate, shader-only system (ship/interiorLights.ts).
 */
export class LightPool {
  readonly group = new THREE.Group();
  private spots: THREE.SpotLight[] = [];
  private points: THREE.PointLight[] = [];
  private spotAsk: SpotAsk[] = [];
  private pointAsk: PointAsk[] = [];
  private nSpot = 0;
  private nPoint = 0;

  constructor(spots = 4, points = 2) {
    this.group.name = 'light-pool';
    for (let i = 0; i < spots; i++) {
      const l = new THREE.SpotLight(0xffffff, 0, 40, 0.5, 0.5, 1.6);
      l.castShadow = false;
      this.group.add(l, l.target);
      this.spots.push(l);
    }
    for (let i = 0; i < points; i++) {
      const l = new THREE.PointLight(0xffffff, 0, 18, 2);
      l.castShadow = false;
      this.group.add(l);
      this.points.push(l);
    }
  }

  /** Ask for a spot light this frame: world position and direction, three.js SpotLight parameters. */
  spot(pos: THREE.Vector3, dir: THREE.Vector3, color: THREE.ColorRepresentation, intensity: number, distance: number, angle: number, penumbra: number, decay: number) {
    if (intensity <= 0) return;
    const a = (this.spotAsk[this.nSpot] ??= { pos: new THREE.Vector3(), dir: new THREE.Vector3(), color: new THREE.Color(), intensity: 0, distance: 0, angle: 0, penumbra: 0, decay: 0, score: 0 });
    this.nSpot++;
    a.pos.copy(pos);
    a.dir.copy(dir);
    a.color.set(color);
    a.intensity = intensity;
    a.distance = distance;
    a.angle = angle;
    a.penumbra = penumbra;
    a.decay = decay;
  }

  /** Ask for a point light this frame (world position). */
  point(pos: THREE.Vector3, color: THREE.ColorRepresentation, intensity: number, distance: number, decay: number) {
    if (intensity <= 0) return;
    const a = (this.pointAsk[this.nPoint] ??= { pos: new THREE.Vector3(), color: new THREE.Color(), intensity: 0, distance: 0, decay: 0, score: 0 });
    this.nPoint++;
    a.pos.copy(pos);
    a.color.set(color);
    a.intensity = intensity;
    a.distance = distance;
    a.decay = decay;
  }

  /**
   * Once per frame, after every emitter asked and before rendering: hand out the slots. The pool
   * lives under the render origin's root (world positions, like every ask).
   */
  update(camera: THREE.Camera) {
    const cam = origin.worldOf(camera, _cam);
    const nS = choose(this.spotAsk, this.nSpot, this.spots.length, cam);
    for (let i = 0; i < this.spots.length; i++) {
      const l = this.spots[i];
      if (i >= nS) {
        l.intensity = 0;
        continue;
      }
      const a = this.spotAsk[i];
      l.position.copy(a.pos);
      l.target.position.copy(a.pos).add(a.dir);
      l.color.copy(a.color);
      l.intensity = a.intensity;
      l.distance = a.distance;
      l.angle = a.angle;
      l.penumbra = a.penumbra;
      l.decay = a.decay;
    }
    const nP = choose(this.pointAsk, this.nPoint, this.points.length, cam);
    for (let i = 0; i < this.points.length; i++) {
      const l = this.points[i];
      if (i >= nP) {
        l.intensity = 0;
        continue;
      }
      const a = this.pointAsk[i];
      l.position.copy(a.pos);
      l.color.copy(a.color);
      l.intensity = a.intensity;
      l.distance = a.distance;
      l.decay = a.decay;
    }
    this.nSpot = 0;
    this.nPoint = 0;
  }
}

/**
 * The `slots` asks that matter most (bright and close to the camera; beyond a light's reach it
 * matters little) moved to the front of `list`, by partial selection (no sort, no garbage).
 * Returns how many there are.
 */
function choose<T extends { pos: THREE.Vector3; intensity: number; distance: number; score: number }>(list: T[], n: number, slots: number, cam: THREE.Vector3) {
  for (let i = 0; i < n; i++) {
    const a = list[i];
    const d = a.pos.distanceTo(cam);
    const beyond = a.distance > 0 ? Math.max(0, d - a.distance) : 0;
    a.score = (d + beyond * 4) / Math.sqrt(a.intensity);
  }
  const k = Math.min(n, slots);
  for (let i = 0; i < k; i++) {
    let best = i;
    for (let j = i + 1; j < n; j++) if (list[j].score < list[best].score) best = j;
    if (best !== i) {
      const t = list[i];
      list[i] = list[best];
      list[best] = t;
    }
  }
  return k;
}

/** The game's pool (one scene, one pool). */
export const lights = new LightPool();
