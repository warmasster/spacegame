// A ship as an acoustic host (acoustics.ts): one adapter between the ship's data and the generic
// sound system. It says what the audio needs to know about any host — its air spaces (the
// compartments) and their pressure, the ways between them (doors, hatches, the ramp, vents and
// ducts, blown panels), its footing on the ground — and plays what the ship's modules declare
// (`ShipSystems.soundCues()`, through the generic CuePlayer). What only ships have lives here too,
// all by data, nothing by name:
//
//   controls  a click by the control's kind (CONTROL_SOUNDS), the buzzer of a refusal, switches
//             moved by other crew or by the machinery itself
//   air       gas rushing through every opening that has a place (shared/ship/airflow vents),
//             panels groaning under pressure
//   hull      panels struck, blown out, welded shut; machines struck and wrecked; the touchdown

import * as THREE from 'three';
import { panelLoad, panelStrain, type Vent } from '../../shared/ship/airflow';
import { zoneAtPoint, type ControlKind } from '../../shared/ship/def';
import type { SoundCue } from '../../shared/sound';
import type { ShipClient } from '../ship/ship';
import { acousticHosts, AirWays, type AcousticHost } from './acoustics';
import { CuePlayer, type CueSpot, type Footing } from './cues';
import { sfx, type Loop } from './engine';
import { newPlace, type Place, type V3 } from './medium';

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
export const SIZE_PITCH: Record<string, number> = { XS: 1.25, S: 1.12, M: 1, L: 0.86 };

const _w3 = new THREE.Vector3();
const _p3 = new THREE.Vector3();

export class ShipSounds implements AcousticHost {
  readonly radius: number;
  private cues: CuePlayer;
  /** Controls by switch key. */
  private byKey = new Map<string, number[]>();
  /** Pressure of each compartment (kPa), this frame. */
  private press: Float64Array;
  private ways: AirWays;
  /** What each air passage is: a panel (open when blown out), a mover key (its travel), a valve key. */
  private passPanel: number[] = [];
  private passKey: string[] = [];
  private passValve: boolean[] = [];
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
  private agl = 0;
  private aglT = 0;
  private foot: Footing = { exterior: 0, interior: 0 };
  private shot: Place = newPlace();
  private centreL: V3;
  private bounds: { min: V3; max: V3 };

  constructor(readonly ship: ShipClient) {
    const sim = ship.sim;
    const def = sim.def;
    const sys = sim.sys;
    for (const c of def.controls) {
      const list = this.byKey.get(c.key);
      if (list) list.push(c.index);
      else this.byKey.set(c.key, [c.index]);
    }
    this.press = new Float64Array(def.compartments.length);
    this.ways = new AirWays(def.compartments.length);
    for (const o of def.openings) this.pass(sys.compIndex(o.a), o.b === null ? -1 : sys.compIndex(o.b), -1, o.key, o.kind === 'vent' || o.kind === 'duct');
    for (const p of def.panels) {
      const a = sys.compIndex(p.zone);
      const b = p.other !== undefined ? sys.compIndex(p.other) : -1;
      if (a >= 0 || b >= 0) this.pass(a, b, p.index, '', false);
    }
    this.creak = new Float64Array(def.panels.length);
    this.hpIdx = Int32Array.from(def.parts, (p) => sys.hpIndex(p.id));
    this.hpWas = new Float64Array(def.parts.length);
    this.wasLanded = sim.landed;
    const b = def.bounds;
    this.bounds = { min: [...b.min] as V3, max: [...b.max] as V3 };
    this.radius = Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]) / 2;
    this.centreL = [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2];
    this.cues = new CuePlayer(ship.id, sys.soundCues(), (cue) => this.spot(cue));
    acousticHosts.add(this);
  }

  get id() {
    return this.ship.id;
  }

  private pass(a: number, b: number, panel: number, key: string, valve: boolean) {
    this.ways.add(a, b, valve);
    this.passPanel.push(panel);
    this.passKey.push(key);
    this.passValve.push(valve);
  }

  /** Where a declared cue sounds: its point, its machine, its compartment's middle, the ship's middle. */
  private spot(cue: SoundCue): CueSpot {
    const def = this.ship.sim.def;
    const sys = this.ship.sim.sys;
    const part = cue.part;
    let zone: string | null = cue.zone !== undefined ? cue.zone : part ? part.zone : null;
    const at = typeof cue.at === 'function' ? null : cue.at;
    let local: V3 = at ? [at[0], at[1], at[2]] : part ? [part.c[0], part.c[1], part.c[2]] : [...this.centreL];
    if (!at && !part && zone) {
      const z = def.zones.find((x) => x.id === zone);
      if (z) local = [(z.min[0] + z.max[0]) / 2, (z.min[1] + z.max[1]) / 2, (z.min[2] + z.max[2]) / 2];
    }
    if (cue.zone === undefined && !part && at) zone = zoneAtPoint(def.zones, at)?.id ?? null;
    return {
      sound: (part && cue.role && part.sounds?.[cue.role]) || cue.sound,
      local,
      space: sys.compIndex(zone),
      exterior: zone === null,
      pitch: part?.size ? SIZE_PITCH[part.size] ?? 1 : 1,
      hp: part && sys.part(part.id) ? sys.hpIndex(part.id) : -1,
    };
  }

  // --- the acoustic host (acoustics.ts) -----------------------------------------------------

  centre(out: V3) {
    const c = this.ship.position;
    out[0] = c.x;
    out[1] = c.y;
    out[2] = c.z;
    return out;
  }

  toLocal(p: V3, out: V3, margin: number) {
    const l = this.ship.local(_p3.set(p[0], p[1], p[2]), _w3);
    const b = this.bounds;
    if (l.x < b.min[0] - margin || l.x > b.max[0] + margin || l.y < b.min[1] - margin || l.y > b.max[1] + margin || l.z < b.min[2] - margin || l.z > b.max[2] + margin) return false;
    out[0] = l.x;
    out[1] = l.y;
    out[2] = l.z;
    return true;
  }

  toWorld(l: V3, out: V3) {
    const w = this.ship.world(l, _w3);
    out[0] = w.x;
    out[1] = w.y;
    out[2] = w.z;
  }

  spaceAt(l: V3) {
    return this.ship.sim.sys.compIndex(zoneAtPoint(this.ship.sim.def.zones, l)?.id ?? null);
  }

  air(space: number) {
    return space >= 0 && space < this.press.length ? this.press[space] : 0;
  }

  way(space: number) {
    return this.ways.way(space);
  }

  footing() {
    return this.ship.sim.landed ? 1 : 0;
  }

  private openness = (i: number) => {
    const sim = this.ship.sim;
    const panel = this.passPanel[i];
    if (panel >= 0) return sim.hole(panel) ? 1 : 0;
    const key = this.passKey[i];
    if (this.passValve[i]) return sim.sw[key] === 1 ? 1 : 0;
    return this.ship.anim.movers[key] ?? sim.mover(key);
  };

  /** Every frame (acoustics.updateHosts): its air, its declared sounds and what any ship does. */
  update(dt: number, near: boolean) {
    if (!near) {
      if (this.awake) this.sleep();
      return;
    }
    const L = sfx.listener;
    const ship = this.ship;
    const sim = ship.sim;
    const waking = !this.awake;
    this.awake = true;
    const st = sim.st;
    const def = sim.def;
    for (let i = 0; i < this.press.length; i++) this.press[i] = sim.sys.pressure(st, def.compartments[i].id);
    // the ways from the listener: its space here, or the outside round us
    this.ways.solve(L.host === ship.id && L.space >= 0 ? L.space : L.space < 0 ? -1 : -2, this.openness);
    // the ground under it carries what it does: all of it when it stands there, its exhausts when low
    this.aglT -= dt;
    if (this.aglT <= 0) {
      this.aglT = 0.25;
      this.agl = sim.landed ? 0 : ship.altitude();
    }
    this.foot.exterior = sim.landed ? 1 : Math.max(0, 1 - this.agl / 25);
    this.foot.interior = sim.landed ? 0.5 : 0;
    this.cues.update(dt, st, sim.sw, this.foot, waking);
    this.centre(_c);
    const d = Math.hypot(_c[0] - L.p[0], _c[1] - L.p[1], _c[2] - L.p[2]);
    const close = d < this.radius + 150;
    if (close) this.airflow(d, dt);
    this.parts(!waking && close);
    // touchdown: the gear takes the weight
    if (sim.landed && !this.wasLanded && !waking) this.touchdown();
    if (!sim.landed) this.lastSpeed = Math.hypot(sim.pose.v[0], sim.pose.v[1], sim.pose.v[2]);
    this.wasLanded = sim.landed;
  }

  /** A ship point as a place for a one-shot (its compartment from the point). */
  placeOf(local: readonly number[], out: Place = this.shot): Place {
    const landed = this.ship.sim.landed;
    out.host = this.ship.id;
    out.local ??= [0, 0, 0];
    out.local[0] = local[0];
    out.local[1] = local[1];
    out.local[2] = local[2];
    out.space = this.spaceAt(out.local);
    out.ground = landed ? (out.space >= 0 ? 0.5 : 1) : 0;
    out.own = 0;
    out.structural = false;
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
    pl.space = sim.sys.compIndex(p.zone);
    if (flipped && sim.hole(i)) {
      sfx.play(p.kind === 'glass' ? 'glass.break' : 'hull.breach', pl);
      // air behind it: the explosive decompression
      if (panelLoad(sim.sys, sim.st, p) > 8) sfx.play('decomp.bang', pl);
    } else if (flipped) sfx.play('hull.seal', pl);
    else if (before - after > 0.5) sfx.play('hull.hit', pl, Math.min(1, 0.3 + (before - after) / 40));
  }

  /** Gas through every opening with a place (breach, door, ramp): a rush as loud as its flow. Panels past what they hold groan. */
  private airflow(d: number, dt: number) {
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
      pl.host = this.ship.id;
      pl.space = v.up >= 0 ? v.up : v.down;
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
    pl.space = -1;
    sfx.play('ship.touchdown', pl, Math.min(1.2, 0.35 + this.lastSpeed / 3));
  }

  /** The whole state was replaced (a snapshot): what changed meanwhile doesn't play now. */
  resync() {
    if (this.awake) this.sleep();
  }

  private sleep() {
    this.awake = false;
    this.cues.sleep();
    for (const l of this.ventLoop) l.level = 0;
  }
}

const _c: V3 = [0, 0, 0];
