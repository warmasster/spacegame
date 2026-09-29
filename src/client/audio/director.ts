// The game's side of the audio: fills the listener every frame (the camera's ears, the head's
// air and contacts), wakes every ship's sounds, and answers "where is this, acoustically" for
// anything placed in the world (a blast, a rocket, someone else's boots): in which ship's room, on
// which hull, on the ground. Run as the `audio` frame system (game.ts), after the camera is placed.

import * as THREE from 'three';
import { zoneAtPoint } from '../../shared/ship/def';
import type { ShipClient } from '../ship/ship';
import { origin } from '../render/origin';
import { CrewSounds, Helmet } from './crewSounds';
import { sfx } from './engine';
import { newPlace, type Place, type V3 } from './medium';
import { shipAcoustics } from './shipSounds';
import './sounds';

/** What the local astronaut is doing, as the ears need it (filled by the game every frame). */
export interface EarState {
  /** The head (world). */
  head: THREE.Vector3;
  /** Frame it is in (0: the outside, else a ship id). */
  frame: number;
  grounded: boolean;
  seated: boolean;
  dead: boolean;
}

const _cam = new THREE.Vector3();
const _dir = new THREE.Vector3();
const _q = new THREE.Quaternion();
const _up = new THREE.Vector3();
const _l = new THREE.Vector3();
const _p = new THREE.Vector3();
const _head: V3 = [0, 0, 0];

export class AudioDirector {
  /** The local astronaut's own sounds and its helmet. */
  readonly me = new CrewSounds(true);
  readonly helmet = new Helmet();

  constructor(
    private camera: THREE.Camera,
    private ships: () => readonly ShipClient[],
    private groundAlt: (p: readonly number[]) => number,
  ) {
    sfx.acoustics = shipAcoustics;
  }

  /** Every frame: the ears, every ship, then the engine. */
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
    // what the head is in: its ship's room first, then any ship's
    const ships = this.ships();
    let ship: ShipClient | null = null;
    let room = -1;
    for (let k = 0; k < ships.length; k++) {
      const s = ships[k];
      if (s.id !== ear.frame) continue;
      ship = s;
      room = roomOf(s, ear.head);
    }
    if (room < 0) {
      for (let k = 0; k < ships.length; k++) {
        const s = ships[k];
        if (s === ship || s.position.distanceToSquared(ear.head) > 60 * 60) continue;
        const r = roomOf(s, ear.head);
        if (r >= 0) {
          ship = s;
          room = r;
          break;
        }
      }
    }
    L.ship = ship?.id ?? 0;
    L.room = room;
    _head[0] = ear.head.x;
    _head[1] = ear.head.y;
    _head[2] = ear.head.z;
    L.air = shipAcoustics.air(L.ship, room, _head);
    const onIt = !!ship && ear.frame === ship.id;
    L.onShip = ear.seated && onIt ? 1 : onIt && ear.grounded ? 0.9 : onIt ? 0.2 : 0;
    L.onGround = ear.frame === 0 && ear.grounded ? 1 : onIt && ship!.sim.landed && (ear.grounded || ear.seated) ? 0.4 : 0;
    L.deaf = ear.dead;
    for (let k = 0; k < ships.length; k++) ships[k].sounds.update(dt);
    sfx.update(dt);
  }

  /**
   * Where a world point is, acoustically (into `out`): inside a ship's room or on its hull (then it
   * follows the ship), on or near the ground.
   */
  placeAt(p: readonly number[], out: Place = newPlace()): Place {
    out.p[0] = p[0];
    out.p[1] = p[1];
    out.p[2] = p[2];
    out.ship = 0;
    out.room = -1;
    out.own = 0;
    out.structural = false;
    const keep = out.local;
    out.local = null;
    const ships = this.ships();
    _p.set(p[0], p[1], p[2]);
    for (let k = 0; k < ships.length; k++) {
      const s = ships[k];
      const l = s.local(_p, _l);
      const b = s.sim.def.bounds;
      const m = 1.5;
      if (l.x < b.min[0] - m || l.x > b.max[0] + m || l.y < b.min[1] - m || l.y > b.max[1] + m || l.z < b.min[2] - m || l.z > b.max[2] + m) continue;
      out.ship = s.id;
      out.room = s.sim.sys.compIndex(zoneAtPoint(s.sim.def.zones, [l.x, l.y, l.z])?.id ?? null);
      out.local = keep ?? [0, 0, 0];
      out.local[0] = l.x;
      out.local[1] = l.y;
      out.local[2] = l.z;
      break;
    }
    const alt = this.groundAlt(p);
    out.ground = alt < 1.5 ? 1 : alt < 6 ? (6 - alt) / 4.5 : 0;
    return out;
  }

  /**
   * Where an astronaut's feet are, acoustically (into `out`): on a ship's deck (`frame`, its point
   * `local`) or out on the ground (world `p`). Keeps `out.own`.
   */
  feetAt(out: Place, frame: number, p: THREE.Vector3, local: THREE.Vector3, grounded: boolean) {
    out.p[0] = p.x;
    out.p[1] = p.y;
    out.p[2] = p.z;
    out.structural = false;
    const ships = this.ships();
    let ship: ShipClient | null = null;
    for (let k = 0; k < ships.length; k++) if (ships[k].id === frame) ship = ships[k];
    if (!ship) {
      out.ship = 0;
      out.room = -1;
      out.local = null;
      out.ground = grounded ? 1 : 0;
      return out;
    }
    out.ship = ship.id;
    const l = (out.local ??= [0, 0, 0]);
    l[0] = local.x;
    l[1] = local.y;
    l[2] = local.z;
    _head[0] = local.x;
    _head[1] = local.y + 1;
    _head[2] = local.z;
    out.room = ship.sim.sys.compIndex(zoneAtPoint(ship.sim.def.zones, _head)?.id ?? null);
    out.ground = ship.sim.landed && grounded ? 0.5 : 0;
    return out;
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
    const outside = pl.room < 0 && L.room < 0;
    const sameSpace = outside || (pl.ship !== 0 && pl.ship === L.ship && pl.room === L.room);
    if (!L.deaf && sameSpace && d < 16 * Math.sqrt(size)) {
      const k = 1 - d / (16 * Math.sqrt(size));
      sfx.ui('suit.pelt', k);
      sfx.stun(k * 0.9);
    }
  }
}

const _blast = newPlace();
const _shot = newPlace();

function roomOf(s: ShipClient, head: THREE.Vector3) {
  const z = s.zoneAt(head);
  return z ? s.sim.sys.compIndex(z.id) : -1;
}
