// The game's side of the audio, knowing nothing of ships or bodies by name: every frame it fills
// the listener (the camera's ears; the head's air and what it touches, from the acoustic hosts and
// the environment), wakes the hosts near it, sounds the outside the body makes (wind where there
// is air), and places anything put in the world (a blast, a projectile, someone's boots) through
// acoustics.ts. Run as the `audio` frame system (game.ts), after the camera is placed.

import * as THREE from 'three';
import { origin } from '../render/origin';
import { acoustics, acousticHosts, Environment, feetAt, locate, updateHosts } from './acoustics';
import { CrewSounds, Helmet } from './crewSounds';
import { sfx } from './engine';
import { newPlace, type Place, type V3 } from './medium';
import './sounds';

/** What the local astronaut is doing, as the ears need it (filled by the game every frame). */
export interface EarState {
  /** The head (world). */
  head: THREE.Vector3;
  /** Frame it is in (0: the world, else a host's id). */
  frame: number;
  grounded: boolean;
  seated: boolean;
  dead: boolean;
}

const _cam = new THREE.Vector3();
const _dir = new THREE.Vector3();
const _q = new THREE.Quaternion();
const _up = new THREE.Vector3();
const _head: V3 = [0, 0, 0];
const _l: V3 = [0, 0, 0];
const _w: V3 = [0, 0, 0];
const _lv: V3 = [0, 0, 0];

export class AudioDirector {
  /** The local astronaut's own sounds and its helmet. */
  readonly me = new CrewSounds(true);
  readonly helmet = new Helmet();
  private env = new Environment();

  constructor(
    private camera: THREE.Camera,
    private groundAlt: (p: readonly number[]) => number,
  ) {
    sfx.acoustics = acoustics;
  }

  /** Every frame: the ears, the hosts round them, the outside, then the engine. */
  update(dt: number, ear: EarState) {
    const L = sfx.listener;
    const cam = origin.worldOf(this.camera, _cam);
    L.p[0] = cam.x;
    L.p[1] = cam.y;
    L.p[2] = cam.z;
    // the render origin only translates: the camera's rotation is the world's
    this.camera.getWorldDirection(_dir);
    this.camera.getWorldQuaternion(_q);
    _up.set(0, 1, 0).applyQuaternion(_q);
    L.fwd[0] = _dir.x;
    L.fwd[1] = _dir.y;
    L.fwd[2] = _dir.z;
    L.up[0] = _up.x;
    L.up[1] = _up.y;
    L.up[2] = _up.z;
    // what the head is in: a space of the host it is in first, then of any host round it
    _head[0] = ear.head.x;
    _head[1] = ear.head.y;
    _head[2] = ear.head.z;
    L.host = 0;
    L.space = -1;
    const mine = ear.frame !== 0 ? acousticHosts.get(ear.frame) : undefined;
    if (mine && mine.toLocal(_head, _l, 0.5)) {
      L.host = mine.id;
      L.space = mine.spaceAt(_l);
    }
    if (L.space < 0) {
      const all = acousticHosts.all();
      for (let k = 0; k < all.length; k++) {
        const h = all[k];
        if (h === mine || !h.toLocal(_head, _l, 0)) continue;
        const s = h.spaceAt(_l);
        if (s < 0) continue;
        L.host = h.id;
        L.space = s;
        break;
      }
    }
    L.air = acoustics.air(L.host, L.space, _head);
    // what it touches: the host it stands in (its structure), the ground (directly, or through the host's footing)
    const on = !!mine;
    L.touching = mine?.id ?? 0;
    L.onHost = on ? (ear.seated ? 1 : ear.grounded ? 0.9 : 0.2) : 0;
    L.onGround = ear.frame === 0 && ear.grounded ? 1 : on && (ear.grounded || ear.seated) ? 0.4 * mine!.footing() : 0;
    L.deaf = ear.dead;
    updateHosts(dt, L);
    this.env.update(L);
    sfx.update(dt);
  }

  /** Where a world point is, acoustically (into `out`). */
  placeAt(p: readonly number[], out: Place = newPlace()): Place {
    return locate(p, this.groundAlt, out);
  }

  /** Where someone's feet are (a host's frame and their point in it, or the world), into `out`. */
  feetAt(out: Place, frame: number, p: THREE.Vector3, local: THREE.Vector3, grounded: boolean) {
    _w[0] = p.x;
    _w[1] = p.y;
    _w[2] = p.z;
    _lv[0] = local.x;
    _lv[1] = local.y;
    _lv[2] = local.z;
    return feetAt(out, frame, _w, _lv, grounded);
  }

  /** A one-shot at a world point, through whatever carries it from there. */
  playAt(id: string, p: V3, gain = 1) {
    sfx.play(id, this.placeAt(p, _shot), gain);
  }

  /**
   * A blast at a world point (`size` 1 = a rocket): the boom through whatever carries it, the
   * grit of it pelting the suit and the ears ringing when close.
   */
  explosion(p: V3, size = 1, carry = true) {
    const pl = this.placeAt(p, _blast);
    sfx.play(size >= 1 ? 'boom.big' : 'boom.small', pl, Math.min(1.6, 0.6 + 0.5 * size));
    if (!carry) return;
    const L = sfx.listener;
    const d = Math.hypot(p[0] - L.p[0], p[1] - L.p[1], p[2] - L.p[2]);
    // debris flying past and into the suit (the blast in the same space as you, no wall between)
    const outside = pl.space < 0 && L.space < 0;
    const sameSpace = outside || (pl.host !== 0 && pl.host === L.host && pl.space === L.space);
    if (!L.deaf && sameSpace && d < 16 * Math.sqrt(size)) {
      const k = 1 - d / (16 * Math.sqrt(size));
      sfx.ui('suit.pelt', k);
      sfx.stun(k * 0.9);
    }
  }
}

const _blast = newPlace();
const _shot = newPlace();
