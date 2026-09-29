// Autopilot, systems side (runs on the systems authority): keeps the modes consistent — engaging
// one switches the master on and drops the modes that want the same axes, the master off clears
// them all — does what the pilot's hands would (gear down for an automatic landing, up after an
// automatic take-off), finishes modes (landed → all off, take-off height reached → ALTURA + NIVEL,
// destination reached → hover) and drops out when the flight computer loses power. The flying
// itself is shared/ship/flight (fcs.ts + autopilot.ts), on whoever has flight authority.

import { partTag, type AutopilotDef, type ControlDef } from '../def.js';
import { AP_FACE_KEY, AP_FACES, AP_KEY, AP_MODES, apModes, apSelection, HDG_STEPS } from '../flight/autopilot.js';
import { bearingTo, NAV_POINTS } from '../flight/nav.js';
import { baseFix, baseFrom, bodyAt, circularAt, frameAt, localFrame, orbitInfo, orbitInto } from '../../space/body.js';
import type { ShipSystems } from '../systems.js';
import type { InterlockEnv, ShipModule, SoundCue, SystemFactory, Tick } from './api.js';
import { ENG } from './apu.js';

/** Modes that fly the main engines themselves (the throttle lever is locked while one is on). */
const BURNS = ['sub', 'circ', 'baj', 'ocrz', 'bajq', 'crz'];

export class Autopilot implements ShipModule {
  readonly id = 'autopilot';
  private prev: Record<string, number> = {};
  private keys: string[];
  private landedFor = 0;
  /** The ship's modes with their switch keys and the keys they drop (built once). */
  private modes: Array<{ key: string; excludes: string[] }>;

  private orb = orbitInfo();
  private fix = baseFix();
  private lf = localFrame();
  /** Switch keys of the modes that burn, and of the surface ones (cancelled in the orbital regime). */
  private burnKeys: string[];
  private surfaceKeys: Set<string>;
  private spaceKeys: Set<string>;
  /** State index of every main engine's state (to warn when a climb is asked of cold engines). */
  private engineStates: number[];

  constructor(
    private sys: ShipSystems,
    private ap: AutopilotDef,
  ) {
    const modes = apModes(ap);
    this.modes = modes.map((m) => ({ key: AP_KEY(m.id), excludes: m.excludes.filter((x) => ap.modes.includes(x)).map(AP_KEY) }));
    this.keys = ['ap.on', AP_FACE_KEY, ...this.modes.map((m) => m.key)];
    for (const k of this.keys) this.prev[k] = sys.def.defaults[k] ?? 0;
    this.burnKeys = BURNS.filter((id) => ap.modes.includes(id)).map(AP_KEY);
    // levelling still makes sense in orbit (the flight computer keeps it): the rest of the surface face doesn't
    this.surfaceKeys = new Set(modes.filter((m) => m.face === 0 && m.id !== 'lvl').map((m) => AP_KEY(m.id)));
    this.spaceKeys = new Set(modes.filter((m) => m.face === 2).map((m) => AP_KEY(m.id)));
    this.engineStates = sys.def.parts.filter((p) => p.type === 'engine').flatMap((p) => {
      const name = `${partTag(p)}.state`;
      return sys.vars.has(name) ? [sys.vars.idx(name)] : [];
    });
  }

  private powered(st: Float64Array) {
    return this.sys.supply(st, this.ap.circuit) >= 0.5;
  }

  private allOff(t: Tick) {
    for (const m of this.modes) t.setSw(m.key, 0);
  }

  /** Engaging chimes; dropping out (by hand or by itself) sounds the disconnect warble at the panel. */
  sounds(): SoundCue[] {
    const c = this.sys.def.controls.find((x) => x.key === 'ap.on');
    if (!c) return [];
    return [
      { sound: 'ap.on', at: c.c, on: (_st, sw) => sw['ap.on'] === 1 },
      { sound: 'ap.off', at: c.c, on: (_st, sw) => sw['ap.on'] !== 1 },
    ];
  }

  input(t: Tick) {
    const sw = t.sw;
    // heading knob sync: a pulse copies the heading the ship has now
    if (sw['ap.hdg.sync'] === 1) {
      t.setSw('ap.hdg.sync', 0);
      t.setSw('ap.hdg.sel', Math.round((t.ctx.heading / (2 * Math.PI)) * HDG_STEPS) % HDG_STEPS);
    }
    // the drum turned: which face is out now
    const face = sw[AP_FACE_KEY] ?? 0;
    if (face !== (this.prev[AP_FACE_KEY] ?? 0)) t.say(`Tambor del piloto automático: cara ${AP_FACES[face] ?? face}`);
    // the master switched off wins over anything engaged in the same tick
    if (sw['ap.on'] !== 1 && this.prev['ap.on'] === 1) this.allOff(t);
    else {
      for (const m of this.modes) {
        if (sw[m.key] === 1 && this.prev[m.key] !== 1) {
          t.setSw('ap.on', 1);
          for (const x of m.excludes) t.setSw(x, 0);
          if (this.burnKeys.includes(m.key) && !this.enginesRunning(t.st)) t.say('Motores principales parados: arráncalos para que el piloto automático pueda quemar');
        }
      }
    }
    if (sw['ap.on'] === 1 && !this.powered(t.st)) {
      t.setSw('ap.on', 0);
      this.allOff(t);
      t.say('Piloto automático desconectado: el ordenador de vuelo no tiene energía');
    }
    for (const k of this.keys) this.prev[k] = sw[k] ?? 0;
  }

  step(t: Tick) {
    const { sw, ctx } = t;
    if (sw['ap.on'] !== 1) {
      this.landedFor = 0;
      return;
    }
    const gear = this.sys.def.gear?.key;
    const sel = apSelection(this.ap, sw);
    if (sw['ap.land'] === 1) {
      if (gear && sw[gear] !== 1 && ctx.agl < 40) {
        t.setSw(gear, 1);
        t.say('Aterrizaje automático: tren abajo');
      }
      this.landedFor = ctx.landed ? this.landedFor + t.dt : 0;
      if (this.landedFor > 1.5) {
        // on the ground the autopilot has nothing left to do: everything off
        t.setSw('ap.on', 0);
        this.allOff(t);
        t.say('Aterrizaje completado: piloto automático desconectado');
      }
    } else this.landedFor = 0;
    if (sw['ap.to'] === 1) {
      if (gear && sw[gear] === 1 && !ctx.landed && ctx.agl > 8) {
        t.setSw(gear, 0);
        t.say('Despegue automático: tren arriba');
      }
      if (ctx.agl >= sel.alt - 1) {
        t.setSw('ap.to', 0);
        t.setSw('ap.alt', 1);
        t.setSw('ap.lvl', 1);
        t.say(`Despegue completado: manteniendo ${sel.alt} m`);
      }
    }
    // ---- the orbital burns end by themselves ----------------------------------------------------------
    // the body it flies round, wherever that is
    const body = bodyAt(ctx.pos);
    const burning = sw['ap.sub'] === 1 || sw['ap.circ'] === 1 || sw['ap.baj'] === 1 || sw['ap.bajq'] === 1;
    if (burning) {
      const o = orbitInto(body, ctx.pos, ctx.vel, this.orb);
      const r = body.radius + o.altitude;
      // speed across the ground and up (m/s)
      const x = ctx.pos[0] - body.center[0];
      const y = ctx.pos[1] - body.center[1];
      const z = ctx.pos[2] - body.center[2];
      const vUp = (ctx.vel[0] * x + ctx.vel[1] * y + ctx.vel[2] * z) / r;
      const vh = Math.sqrt(Math.max(0, o.speed * o.speed - vUp * vUp));
      if (sw['ap.sub'] === 1) {
        if (gear && sw[gear] === 1 && !ctx.landed && ctx.agl > 8) {
          t.setSw(gear, 0);
          t.say('Subida a órbita: tren arriba');
        }
        if (vh >= circularAt(body, sel.orb) - 3 && o.periapsis > sel.orb * 0.8) {
          t.setSw('ap.sub', 0);
          t.setSw('ap.circ', 1);
          t.say(`Velocidad orbital a ${(o.altitude / 1000).toFixed(1)} km: CIRCUL. redondea la órbita`);
        }
      } else if (sw['ap.circ'] === 1) {
        if (Math.abs(o.circular - vh) < 1.5 && Math.abs(vUp) < 1.5) {
          t.setSw('ap.circ', 0);
          t.say(`Órbita circular a ${(o.altitude / 1000).toFixed(1)} km: una vuelta cada ${Math.round(o.period / 60)} min`);
        }
      } else if (sw['ap.bajq'] === 1) {
        if (vh < 3 && Math.abs(ctx.agl - 150) < 30) {
          // stopped over the ground: the surface face takes over, hovering where it is
          t.setSw('ap.bajq', 0);
          t.setSw(AP_FACE_KEY, 0);
          const alt = this.ap.alts.findIndex((a) => a >= 100);
          t.setSw('ap.alt.sel', alt >= 0 ? alt : this.ap.alts.length - 1);
          for (const k of ['ap.alt', 'ap.lvl']) if (this.keys.includes(k)) t.setSw(k, 1);
          t.say('Parada sobre el suelo: tambor a SUPERFICIE en vuelo estacionario. Pulsa ATERRIZ. para posarte');
        }
      } else if (sw['ap.baj'] === 1) {
        const b = baseFrom(body, ctx.pos, ctx.vel, this.fix);
        if (b.dist < 1500 && vh < 25) {
          // over the base: the surface face takes over, hovering at the base
          t.setSw('ap.baj', 0);
          t.setSw(AP_FACE_KEY, 0);
          const base = NAV_POINTS.findIndex((p) => p.id === 'base');
          t.setSw('ap.wp', Math.max(0, base));
          const alt = this.ap.alts.findIndex((a) => a >= 100);
          t.setSw('ap.alt.sel', alt >= 0 ? alt : this.ap.alts.length - 1);
          for (const k of ['ap.nav', 'ap.alt', 'ap.lvl']) if (this.keys.includes(k)) t.setSw(k, 1);
          t.say('Sobre la base: tambor a SUPERFICIE, NAV a BASE en vuelo estacionario. Pulsa ATERRIZ. para posarte');
        }
      }
    }
    if (sw['ap.nav'] === 1 && sel.wp.body === body.def.id) {
      const d = bearingTo(body, ctx.pos, sel.wp).dist;
      // over it and nearly still across the ground (the local horizon's share of the velocity)
      const f = frameAt(body, ctx.pos, this.lf);
      const v = ctx.vel;
      const vu = v[0] * f.up[0] + v[1] * f.up[1] + v[2] * f.up[2];
      const vh = Math.sqrt(Math.max(0, v[0] * v[0] + v[1] * v[1] + v[2] * v[2] - vu * vu));
      if (d < 6 && vh < 1.5) {
        t.setSw('ap.nav', 0);
        t.setSw('ap.alt', 1);
        t.setSw('ap.lvl', 1);
        t.say(`Llegada a ${sel.wp.name}: vuelo estacionario. ATERRIZ. para posarse`);
      }
    }
    for (const k of this.keys) this.prev[k] = sw[k] ?? 0;
  }

  /** Some main engine is running (or spooling up). */
  private enginesRunning(st: Float64Array) {
    if (!this.engineStates.length) return true;
    for (const i of this.engineStates) if (st[i] === ENG.run || st[i] === ENG.spool) return true;
    return false;
  }

  interlock(c: ControlDef, next: number, st: Float64Array, sw: Record<string, number>, env: InterlockEnv) {
    // an orbital burn has the engines: the lever waits (amber)
    const helm = this.sys.def.helm;
    if (helm && c.key === helm.throttle && sw['ap.on'] === 1) {
      for (const k of this.burnKeys) if (sw[k] === 1) return `El piloto automático lleva el motor (${AP_MODES.find((m) => AP_KEY(m.id) === k)?.label ?? k}): desconéctalo para usar el acelerador`;
    }
    if (!c.key.startsWith('ap.') || next !== 1 || c.key.endsWith('.sel') || c.key === 'ap.wp' || c.key === AP_FACE_KEY) return null;
    if (!this.powered(st)) return 'Sin energía en el ordenador de vuelo';
    if (c.key === 'ap.land' && env.landed) return 'Ya estás en tierra';
    if (env.orbital && this.surfaceKeys.has(c.key)) return 'En régimen orbital (más de 150 m/s o 15 km): usa las caras ÓRBITA y ESPACIO del tambor';
    if (!env.orbital && (c.key === 'ap.circ' || c.key === 'ap.baj')) return 'Solo en régimen orbital (más de 150 m/s o 15 km de altura)';
    if (env.landed && this.spaceKeys.has(c.key)) return 'En tierra: nada hacia donde apuntar';
    if (env.landed && (c.key === 'ap.bajq' || c.key === 'ap.ocrz')) return 'Ya estás en tierra: despega primero';
    return null;
  }
}

export const autopilotSystem: SystemFactory = {
  id: 'autopilot',
  make: (sys) => (sys.def.autopilot ? [new Autopilot(sys, sys.def.autopilot)] : []),
};
