// A ship's sounds on this client, for any ship, from its data and the replicated state. Nothing
// here knows a machine by name:
//
//   cues      every module declares what its machines sound like (`ShipModule.sounds()`, gathered
//             by `ShipSystems.soundCues()`: the reactor, the engines, the movers, the master alarm…);
//             this plays them from where they are — loops as loud as their `level`, one-shots on
//             the edge of `on`, travel noise from `motion`.
//   physics   what any ship does the same way: a control's click by its kind (CONTROL_SOUNDS), the
//             buzzer of a refusal, gas rushing out of every opening (airflow vents), panels
//             creaking under pressure, struck, blown out, welded shut, machines hit and wrecked,
//             the touchdown.
//   air       its rooms' pressure and how open the way is from the listener to each of them, for
//             the medium (it answers `Acoustics` for every ship).
//
// Far from the listener a ship sleeps: no loops, no edges (they are re-read on waking, so what
// changed meanwhile doesn't play late).

import * as THREE from 'three';
import { panelLoad, panelStrain, type Vent } from '../../shared/ship/airflow';
import { zoneAtPoint, type ControlKind } from '../../shared/ship/def';
import type { SoundCue } from '../../shared/ship/modules/api';
import { bodyAt } from '../../shared/space/body';
import type { ShipClient } from '../ship/ship';
import { sfx, type Loop } from './engine';
import { newPlace, type Acoustics, type Place, type V3 } from './medium';

/** The voice of each kind of control (bank ids). A new kind of control is one entry. */
export const CONTROL_SOUNDS: Record<ControlKind, string> = {
  button: 'ctl.button',
  toggle: 'ctl.toggle',
  lever: 'ctl.lever',
  breaker: 'ctl.breaker',
  master: 'ctl.master',
  mushroom: 'ctl.mushroom',
  rotary: 'ctl.rotary',
  cover: 'ctl.cover',
  valve: 'ctl.valve',
  bezel: 'ctl.bezel',
};

/** Controls the machinery itself can move (a breaker tripping, a lever dropping out): heard when it does. */
const SELF_MOVING: ReadonlySet<ControlKind> = new Set<ControlKind>(['breaker', 'lever', 'toggle', 'rotary', 'valve']);

/** The same machine smaller sounds higher, bigger lower (catalog size class). */
const SIZE_PITCH: Record<string, number> = { XS: 1.25, S: 1.12, M: 1, L: 0.86 };

/** Beyond this (m, past the ship's own size) a ship sleeps. */
const WAKE = 450;

interface LiveCue {
  cue: SoundCue;
  sound: string;
  /** Ship-space point (refreshed by `at` when it is a function). */
  local: V3;
  atFn: ((st: Float64Array, sw: Record<string, number>, out: V3) => V3) | null;
  room: number;
  exterior: boolean;
  /** Integrity variable of its part (-1: none): a wreck is silent. */
  hp: number;
  pitch: number;
  gain: number;
  loop: Loop | null;
  /** Last value of `on` (-1: not read yet). */
  was: number;
  /** `motion`: last value and smoothed speed. */
  mv: number;
  speed: number;
}

const byId = new Map<number, ShipSounds>();
const _w3 = new THREE.Vector3();

export class ShipSounds {
  private cues: LiveCue[] = [];
  /** Controls by switch key. */
  private byKey = new Map<string, number[]>();
  /** Pressure of each room (kPa), this frame. */
  private press: Float64Array;
  /** How open the way is from the listener to each room (and, last, to the outside round the ship). */
  private way: Float64Array;
  /** Air ways between rooms (-1 = the outside): openings (their key) and panels (a hole). */
  private edges: Array<{ a: number; b: number; key: string | null; vent: boolean; panel: number }> = [];
  private vents: Vent[] = [];
  private ventKey: number[] = [];
  private ventLoop: Loop[] = [];
  private ventSeen: number[] = [];
  private stamp = 0;
  private creak: Float64Array;
  private hpWas: Float64Array;
  private hpIdx: Int32Array;
  private wasLanded: boolean;
  private lastSpeed = 0;
  private awake = false;
  private radius: number;
  private agl = 0;
  private aglT = 0;
  private shot: Place = newPlace();

  constructor(readonly ship: ShipClient) {
    byId.set(ship.id, this);
    const sim = ship.sim;
    const def = sim.def;
    const sys = sim.sys;
    for (const c of def.controls) {
      const list = this.byKey.get(c.key);
      if (list) list.push(c.index);
      else this.byKey.set(c.key, [c.index]);
    }
    const n = def.compartments.length;
    this.press = new Float64Array(n);
    this.way = new Float64Array(n + 1);
    for (const o of def.openings) this.edges.push({ a: sys.compIndex(o.a), b: o.b === null ? -1 : sys.compIndex(o.b), key: o.key, vent: o.kind === 'vent' || o.kind === 'duct', panel: -1 });
    for (const p of def.panels) {
      const a = sys.compIndex(p.zone);
      const b = p.other !== undefined ? sys.compIndex(p.other) : -1;
      if (a >= 0 || b >= 0) this.edges.push({ a, b, key: null, vent: false, panel: p.index });
    }
    this.creak = new Float64Array(def.panels.length);
    this.hpIdx = Int32Array.from(def.parts, (p) => sys.hpIndex(p.id));
    this.hpWas = new Float64Array(def.parts.length);
    this.wasLanded = sim.landed;
    const b = def.bounds;
    this.radius = Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]) / 2;
    const centre: V3 = [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2];
    for (const cue of sys.soundCues()) {
      const part = cue.part;
      // where: its own point, its machine, its room's middle, the ship's middle
      let zone: string | null = cue.zone !== undefined ? cue.zone : part ? part.zone : null;
      const at = typeof cue.at === 'function' ? null : cue.at;
      let local: V3 = at ? [at[0], at[1], at[2]] : part ? [part.c[0], part.c[1], part.c[2]] : [...centre];
      if (!at && !part && zone) {
        const z = def.zones.find((x) => x.id === zone);
        if (z) local = [(z.min[0] + z.max[0]) / 2, (z.min[1] + z.max[1]) / 2, (z.min[2] + z.max[2]) / 2];
      }
      if (cue.zone === undefined && !part && at) zone = zoneAtPoint(def.zones, at)?.id ?? null;
      this.cues.push({
        cue,
        sound: (part && cue.role && part.sounds?.[cue.role]) || cue.sound,
        local,
        atFn: typeof cue.at === 'function' ? cue.at : null,
        room: sys.compIndex(zone),
        exterior: zone === null,
        hp: part ? sys.hpIndex(part.id) : -1,
        pitch: part?.size ? SIZE_PITCH[part.size] ?? 1 : 1,
        gain: cue.gain ?? 1,
        loop: null,
        was: -1,
        mv: NaN,
        speed: 0,
      });
    }
  }

  static of(id: number) {
    return byId.get(id) ?? null;
  }

  /** Pressure of a room (kPa), as of this frame. */
  pressure(room: number) {
    return room >= 0 && room < this.press.length ? this.press[room] : 0;
  }

  /** How open the air way is from the listener to a room (-1: the outside round the ship). */
  wayTo(room: number) {
    return room >= 0 ? this.way[room] ?? 0 : this.way[this.way.length - 1];
  }

  /** Every frame (the audio system): cues, air, and what any ship does. */
  update(dt: number) {
    const L = sfx.listener;
    const ship = this.ship;
    const sim = ship.sim;
    const c = ship.position;
    const d = Math.hypot(c.x - L.p[0], c.y - L.p[1], c.z - L.p[2]);
    if (d > this.radius + WAKE) {
      if (this.awake) this.sleep();
      return;
    }
    const waking = !this.awake;
    this.awake = true;
    const st = sim.st;
    const sw = sim.sw;
    const def = sim.def;
    for (let i = 0; i < this.press.length; i++) this.press[i] = sim.sys.pressure(st, def.compartments[i].id);
    this.buildWay();
    // the ground under it carries what it does: all of it when it stands there, its exhausts when low
    this.aglT -= dt;
    if (this.aglT <= 0) {
      this.aglT = 0.25;
      this.agl = sim.landed ? 0 : ship.altitude();
    }
    const gExt = sim.landed ? 1 : Math.max(0, 1 - this.agl / 25);
    const gIn = sim.landed ? 0.5 : 0;

    for (let k = 0; k < this.cues.length; k++) {
      const lc = this.cues[k];
      const cue = lc.cue;
      if (lc.atFn) lc.atFn(st, sw, lc.local);
      const alive = lc.hp < 0 || st[lc.hp] > 0;
      if (cue.on) {
        const now = cue.on(st, sw) ? 1 : 0;
        if (lc.was === 0 && now === 1 && alive && !waking) {
          const pl = this.fill(this.shot, lc, gExt, gIn);
          sfx.play(lc.sound, pl, lc.gain, lc.pitch);
        }
        lc.was = now;
      }
      if (!cue.level && !cue.motion) continue;
      let level = 0;
      if (cue.motion) {
        const v = cue.motion.value(st, sw);
        if (lc.mv === lc.mv) lc.speed = Math.max(Math.abs(v - lc.mv) / Math.max(dt, 1e-3), lc.speed * Math.exp(-dt / 0.3));
        lc.mv = v;
        level = Math.min(1, lc.speed / cue.motion.rate);
        if (level < 0.05) level = 0;
      }
      if (cue.level) level = cue.motion ? level * cue.level(st, sw) : cue.level(st, sw);
      if (!alive) level = 0;
      if (level <= 0 && !lc.loop) continue;
      lc.loop ??= sfx.loop(lc.sound, lc.gain);
      lc.loop.level = level;
      lc.loop.pitch = lc.pitch * (cue.pitch ? cue.pitch(st, sw) : 1);
      this.fill(lc.loop.place, lc, gExt, gIn);
    }
    const close = d < this.radius + 150;
    if (close) this.air(d, dt);
    this.parts(!waking && close);
    // touchdown: the gear takes the weight
    if (sim.landed && !this.wasLanded && !waking) this.touchdown();
    if (!sim.landed) this.lastSpeed = Math.hypot(sim.pose.v[0], sim.pose.v[1], sim.pose.v[2]);
    this.wasLanded = sim.landed;
  }

  /** Where a cue sounds (a ship point: the engine follows the ship while it plays). */
  private fill(pl: Place, lc: LiveCue, gExt: number, gIn: number) {
    pl.ship = this.ship.id;
    pl.room = lc.room;
    pl.ground = lc.exterior ? gExt : gIn;
    pl.own = 0;
    pl.structural = !!lc.cue.structural;
    pl.local ??= [0, 0, 0];
    pl.local[0] = lc.local[0];
    pl.local[1] = lc.local[1];
    pl.local[2] = lc.local[2];
    return pl;
  }

  /** A ship point as a place for a one-shot (its room from the point). */
  placeOf(local: readonly number[], out: Place = this.shot): Place {
    const def = this.ship.sim.def;
    const z = zoneAtPoint(def.zones, local as V3);
    const landed = this.ship.sim.landed;
    out.ship = this.ship.id;
    out.room = this.ship.sim.sys.compIndex(z?.id ?? null);
    out.ground = landed ? (z ? 0.5 : 1) : 0;
    out.own = 0;
    out.structural = false;
    out.local ??= [0, 0, 0];
    out.local[0] = local[0];
    out.local[1] = local[1];
    out.local[2] = local[2];
    return out;
  }

  /** A one-shot at a ship point. */
  playAt(id: string, local: readonly number[], gain = 1, pitch = 1) {
    sfx.play(id, this.placeOf(local), gain, pitch);
  }

  /** A control operated here and now: its click (by its kind); `deny`: the buzzer of a refusal. */
  control(index: number, deny = false) {
    const c = this.ship.sim.def.controls[index];
    if (!c) return;
    this.playAt(deny ? 'ctl.deny' : CONTROL_SOUNDS[c.kind] ?? 'ctl.button', c.c);
  }

  /**
   * Switches that moved elsewhere: by another crew member (their click, at the control nearest to
   * them: `near` in ship space) or by the machinery itself (a breaker tripping, a lever dropping).
   */
  switched(sw: Record<string, number>, cause: 'crew' | 'system', near?: V3) {
    const def = this.ship.sim.def;
    for (const key in sw) {
      const list = this.byKey.get(key);
      if (!list || key === def.caution) continue;
      let best = list[0];
      if (near && list.length > 1) {
        let bd = Infinity;
        for (const i of list) {
          const p = def.controls[i].c;
          const dd = (p[0] - near[0]) ** 2 + (p[1] - near[1]) ** 2 + (p[2] - near[2]) ** 2;
          if (dd < bd) {
            bd = dd;
            best = i;
          }
        }
      }
      const c = def.controls[best];
      if (cause === 'system' && (!SELF_MOVING.has(c.kind) || c.action === 'pulse')) continue;
      this.control(best);
    }
  }

  /** A panel's integrity changed (the authority's update): struck, blown out, or rebuilt. */
  panel(i: number, before: number, after: number, flipped: boolean) {
    const sim = this.ship.sim;
    const p = sim.def.panels[i];
    const pl = this.placeOf(p.c);
    pl.room = sim.sys.compIndex(p.zone);
    if (flipped && sim.hole(i)) {
      sfx.play(p.kind === 'glass' ? 'glass.break' : 'hull.breach', pl);
      // air behind it: the explosive decompression
      if (panelLoad(sim.sys, sim.st, p) > 8) sfx.play('decomp.bang', pl);
    } else if (flipped) sfx.play('hull.seal', pl);
    else if (before - after > 0.5) sfx.play('hull.hit', pl, Math.min(1, 0.3 + (before - after) / 40));
  }

  /** Gas through every opening (breach, door, ramp, valve): a rush as loud as its flow. Panels past what they hold groan. */
  private air(d: number, dt: number) {
    const sim = this.ship.sim;
    this.stamp++;
    sim.vents(this.vents);
    for (const v of this.vents) {
      if (v.fade <= 0.01) continue;
      const key = v.panel >= 0 ? v.panel : -1 - ((Math.round(v.at[0] * 10) + 5000) * 10000 + (Math.round(v.at[2] * 10) + 5000));
      let k = this.ventKey.indexOf(key);
      if (k < 0) {
        k = this.ventKey.length;
        this.ventKey.push(key);
        this.ventLoop.push(sfx.loop('air.rush'));
        this.ventSeen.push(0);
      }
      this.ventSeen[k] = this.stamp;
      const l = this.ventLoop[k];
      l.level = Math.min(1, 0.2 + Math.log10(1 + v.mdot * 8) / 2.2) * v.fade;
      // small holes whistle higher, fast jets roar higher
      l.pitch = 0.65 + Math.min(0.7, v.speed / 500) + 0.25 * Math.max(0, 1 - v.r0 / 0.25);
      const pl = l.place;
      pl.ship = this.ship.id;
      pl.room = v.up >= 0 ? v.up : v.down;
      pl.ground = 0;
      pl.local ??= [0, 0, 0];
      pl.local[0] = v.at[0];
      pl.local[1] = v.at[1];
      pl.local[2] = v.at[2];
    }
    for (let k = 0; k < this.ventKey.length; k++) if (this.ventSeen[k] !== this.stamp) this.ventLoop[k].level = 0;
    if (d > this.radius + 60) return;
    // panels past what they hold groan, faster the more they are strained
    for (const p of sim.def.panels) {
      if (sim.hole(p.index)) continue;
      const strain = panelStrain(sim.sys, sim.st, p, sim.hp[p.index]);
      if (strain <= 0) continue;
      this.creak[p.index] += dt * (0.7 + strain * 3) * (0.5 + Math.random());
      if (this.creak[p.index] < 1) continue;
      this.creak[p.index] = 0;
      this.playAt('metal.creak', p.c, 0.5 + 0.5 * Math.min(1, strain));
    }
  }

  /** Machines struck (a clunk and a spark) and wrecked (a crunch); `play`: close enough to bother. */
  private parts(play: boolean) {
    const sim = this.ship.sim;
    const st = sim.st;
    const parts = sim.def.parts;
    for (let i = 0; i < parts.length; i++) {
      const hp = st[this.hpIdx[i]];
      const was = this.hpWas[i];
      this.hpWas[i] = hp;
      if (!play || hp >= was) continue;
      const p = parts[i];
      if (hp <= 0 && was > 0) this.playAt('machine.break', p.c);
      else if (was - hp > Math.max(1, p.maxHp * 0.04)) this.playAt('machine.hit', p.c, Math.min(1, 0.4 + (was - hp) / p.maxHp));
    }
  }

  private touchdown() {
    const def = this.ship.sim.def;
    const legs = def.gear?.legs;
    const at: V3 = [0, def.bounds.min[1], 0];
    if (legs?.length) {
      at[1] = 0;
      for (const l of legs) for (let k = 0; k < 3; k++) at[k] += l[k] / legs.length;
    }
    const pl = this.placeOf(at);
    pl.ground = 1;
    pl.room = -1;
    sfx.play('ship.touchdown', pl, Math.min(1.2, 0.35 + this.lastSpeed / 3));
  }

  /** How open the air way is from the listener (in a room of this ship, or outside round it) to every room. */
  private buildWay() {
    const way = this.way;
    way.fill(0);
    const L = sfx.listener;
    const n = this.press.length;
    const from = L.ship === this.ship.id && L.room >= 0 ? L.room : L.room < 0 ? n : -2;
    if (from === -2) return;
    way[from] = 1;
    const sim = this.ship.sim;
    const anim = this.ship.anim.movers;
    // strongest way (product of openings) by relaxing the edges a few times: a handful of rooms
    for (let pass = 0; pass <= n; pass++) {
      let changed = false;
      for (const e of this.edges) {
        let open: number;
        if (e.panel >= 0) open = sim.hole(e.panel) ? 1 : 0;
        else if (e.vent) open = sim.sw[e.key!] === 1 ? 0.15 : 0;
        else open = anim[e.key!] ?? sim.mover(e.key!);
        if (open <= 0.005) continue;
        const t = e.vent ? open : 0.3 + 0.7 * Math.sqrt(open);
        const a = e.a < 0 ? n : e.a;
        const b = e.b < 0 ? n : e.b;
        if (way[a] * t > way[b] + 1e-6) {
          way[b] = way[a] * t;
          changed = true;
        }
        if (way[b] * t > way[a] + 1e-6) {
          way[a] = way[b] * t;
          changed = true;
        }
      }
      if (!changed) break;
    }
  }

  /** The whole state was replaced (a snapshot): what changed meanwhile doesn't play now. */
  resync() {
    if (this.awake) this.sleep();
  }

  private sleep() {
    this.awake = false;
    for (const lc of this.cues) {
      lc.was = -1;
      lc.mv = NaN;
      lc.speed = 0;
      if (lc.loop) lc.loop.level = 0;
    }
    for (const l of this.ventLoop) l.level = 0;
  }
}

/** Every ship's air for the medium (and the outside's, from the body's atmosphere). */
export const shipAcoustics: Acoustics = {
  air(ship, room, p) {
    if (ship !== 0 && room >= 0) return byId.get(ship)?.pressure(room) ?? 0;
    const rho = bodyAt(p).def.atmosphereDensity;
    return rho > 0 ? (rho / 1.225) * 101.3 : 0;
  },
  way(ship, room) {
    return byId.get(ship)?.wayTo(room) ?? 0;
  },
  toWorld(ship, local, out) {
    const s = byId.get(ship);
    if (!s) return false;
    const w = s.ship.world(local, _w3);
    out[0] = w.x;
    out[1] = w.y;
    out[2] = w.z;
    return true;
  },
};
