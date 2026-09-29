// Main engines, VTOL lift pads and RCS thrusters: the machinery side of propulsion. An engine is
// armed (guarded switch), started with a pulse and spools up on its circuit's power and
// propellant; running, it publishes how much of its thrust it can give (`.cap`: health and feed).
// How much it actually gives (`.thr`) is decided by whoever flies the ship (shared/ship/flight):
// this module burns propellant and heats the engine from it. Pads (`.use`) and RCS blocks
// (`.use`, the sum of their six nozzles) work the same way without a start sequence. A badly
// damaged engine with power and propellant reaching it can explode: the crew isolates it by
// closing its feed valve and disarming it.

import { partKey, partTag, type ControlDef, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import { partsOf, type AlertDef, type ShipModule, type SoundCue, type SystemFactory, type Tick } from './api.js';
import { ENG } from './apu.js';

export { ENG };
export const ENGINE = { thrustN: 60000, flowKg: 1.0, spoolS: 3, idle: 0.05, kw: 3 };

/**
 * Overdrive (SOBREPOT.): mass injection. The engine's jet power is fixed (its reactor/heater), and
 * at fixed power thrust × exhaust speed is constant (P = F·vₑ/2). Pushing `flow`× the propellant
 * through it drops the exhaust speed by √flow and raises the thrust by √flow: twice the flow is
 * +41 % thrust at 71 % of the specific impulse — a quicker climb that costs a lot more propellant.
 * The cooler exhaust still heats the chamber harder; past `cutC` the controller drops it by itself
 * and it can be re-engaged below `rearmC`. One ship-wide switch (`key`) for every main engine.
 */
export const BOOST = { key: 'eng.boost', flow: 2, cutC: 1250, rearmC: 1000, heatTau: 110 };
export const BOOST_THRUST = Math.sqrt(BOOST.flow);

export class Engine implements ShipModule {
  readonly id: string;
  readonly tag: string;
  readonly keys: { arm: string; start: string };
  readonly k: typeof ENGINE;
  private v: Record<string, number> = {};

  constructor(
    private sys: ShipSystems,
    readonly part: PartDef,
  ) {
    this.id = `engine:${part.id}`;
    const tag = (this.tag = partTag(part));
    this.keys = { arm: partKey(part, 'arm', `${part.id}.arm`), start: partKey(part, 'start', `${part.id}.start`) };
    this.k = { ...ENGINE, ...part.p } as typeof ENGINE;
    for (const [name, q, init] of [
      ['state', 1, 0],
      ['n', 0.01, 0],
      ['thr', 0.01, 0],
      ['cap', 0.01, 0],
      ['temp', 1, -20],
      ['risk', 0.01, 0],
      ['boost', 1, 0],
    ] as const)
      this.v[name] = sys.vars.define(`${tag}.${name}`, q, init);
  }

  vi(name: string) {
    return this.v[name];
  }

  private H(st: Float64Array) {
    return this.sys.health(st, this.part);
  }

  private fed(st: Float64Array) {
    return this.part.feed && this.sys.fuel ? st[this.sys.fuel.feedIndex(this.part.id)] : 0;
  }

  private propellantReaches(st: Float64Array, sw: Record<string, number>) {
    const fuel = this.sys.fuel;
    return !!fuel && !!this.part.feed && !fuel.isolated(this.part.id, sw) && fuel.canFeed(this.part.feed, sw, st, 1);
  }

  /** Why the engine can't be started now (null = it can). */
  startBlock(st: Float64Array, sw: Record<string, number>) {
    const s = st[this.v.state];
    if (s === ENG.spool || s === ENG.run) return 'Motor ya en marcha';
    if (this.H(st) <= 0) return `${this.part.name} destruido`;
    if (sw[this.keys.arm] !== 1) return 'Motor desarmado: arma primero (interruptor protegido)';
    if (this.sys.supply(st, this.part.circuit) < 0.5) return `Sin energía · circuito ${this.sys.def.subsystems.find((c) => c.id === this.part.circuit)?.label ?? ''}`;
    if (this.sys.fuel?.isolated(this.part.id, sw)) return 'Válvula de alimentación cerrada';
    if (!this.propellantReaches(st, sw)) return 'Sin propelente en los depósitos conectados';
    return null;
  }

  /** Explosion hazard per second: badly damaged + energised + propellant reaching it. */
  risk(st: Float64Array, sw: Record<string, number>) {
    const h = this.H(st);
    if (h >= 0.35) return 0;
    const energised = sw[this.keys.arm] === 1 && this.sys.supply(st, this.part.circuit) >= 0.5;
    if (!energised || !this.propellantReaches(st, sw)) return 0;
    if (h <= 0) return 0.25; // propellant pouring into a wrecked nacelle, sparks from live wiring
    const s = st[this.v.state];
    return (1 - h / 0.35) * (s === ENG.run || s === ENG.spool ? 0.12 : 0.03);
  }

  private blow(t: Tick) {
    const { st } = t;
    const wreck = this.H(st) <= 0;
    st[this.sys.hpIndex(this.part.id)] = 0;
    st[this.v.state] = ENG.fail;
    st[this.v.thr] = 0;
    t.setSw(this.keys.arm, 0);
    t.emit({ type: 'explode', at: this.part.c, radius: 5, damage: 110, cause: `${this.part.name}: explosión` });
    if (!wreck) t.emit({ type: 'destroyed', part: this.part.id });
  }

  input(t: Tick) {
    const { st, sw } = t;
    if (sw[this.keys.start] !== 1) return;
    t.setSw(this.keys.start, 0);
    if (this.startBlock(st, sw)) return;
    st[this.v.state] = ENG.spool;
    // a damaged engine may blow up the moment it lights
    const risk = this.risk(st, sw);
    if (risk > 0 && t.ctx.rand() < risk * 6) this.blow(t);
  }

  loads(t: Tick) {
    const { st } = t;
    const s = st[this.v.state];
    if (s !== ENG.spool && s !== ENG.run) return;
    t.load(this.part.circuit, this.k.kw);
    // `thr` is a fraction of the rated thrust (above 1 in overdrive); overdrive burns √flow more per
    // newton. Lit and idle it only keeps its pilot flame (half a percent): a coast in orbit with the
    // engines lit costs next to nothing.
    const perN = st[this.v.boost] === 1 ? BOOST_THRUST : 1;
    if (this.part.feed) t.burn(this.part.id, this.k.flowKg * Math.max(0.005, st[this.v.thr]) * perN);
  }

  step(t: Tick) {
    const { dt, st, sw } = t;
    const V = this.v;
    const k = this.k;
    const H = this.H(st);
    const fed = this.fed(st);
    if (st[V.state] === ENG.spool || st[V.state] === ENG.run) {
      const armed = sw[this.keys.arm] === 1;
      const why = !armed ? 'desarmado' : this.sys.supply(st, this.part.circuit) < 0.5 ? 'sin energía' : H <= 0 ? 'destruido' : fed < 0.3 && st[V.n] > 0.3 ? 'sin propelente' : null;
      if (why) {
        st[V.state] = why === 'desarmado' ? ENG.off : ENG.fail;
        if (why !== 'desarmado') t.say(`${this.part.name}: apagado (${why})`);
      }
    }
    if (st[V.state] === ENG.spool) {
      st[V.n] = Math.min(1, st[V.n] + dt / k.spoolS);
      if (st[V.n] >= 1) st[V.state] = ENG.run;
    } else if (st[V.state] !== ENG.run) st[V.n] = Math.max(0, st[V.n] - dt / 2);
    // what it can give: running, health and the feed decide; the flight sets how much it gives
    const running = st[V.state] === ENG.run;
    // overdrive: asked for, running, and the chamber not past its limit (it cuts itself there)
    if (st[V.boost] === 1 && st[V.temp] >= BOOST.cutC) {
      if (sw[BOOST.key] === 1) {
        t.setSw(BOOST.key, 0);
        t.say(`SOBREPOTENCIA cortada: ${this.part.name.toLowerCase()} a ${Math.round(st[V.temp])} °C. Se puede volver a conectar por debajo de ${BOOST.rearmC} °C`);
      }
    }
    st[V.boost] = running && sw[BOOST.key] === 1 && st[V.temp] < BOOST.cutC ? 1 : 0;
    const boost = st[V.boost] === 1;
    st[V.cap] = running ? Math.min(1, 0.25 + 0.75 * H) * Math.min(1, fed / 0.9) * (boost ? BOOST_THRUST : 1) : 0;
    st[V.thr] = Math.max(0, Math.min(st[V.cap], st[V.thr]));
    const hot = running || st[V.state] === ENG.spool ? -20 + 950 * Math.max(k.idle, st[V.thr]) : -20;
    // overdrive heats more slowly toward a hotter chamber: about a minute from hot to the cut
    const tau = hot > st[V.temp] ? (boost ? BOOST.heatTau : 6) : 40;
    st[V.temp] += (hot - st[V.temp]) * Math.min(1, dt / tau);
    // the hazard the crew must isolate
    const risk = this.risk(st, sw);
    st[V.risk] = risk;
    if (risk > 0 && t.ctx.rand() < risk * dt) this.blow(t);
  }

  /** Thrust fraction while running (effects). */
  thrust(st: Float64Array) {
    const s = st[this.v.state];
    return s === ENG.run || s === ENG.spool ? st[this.v.thr] : 0;
  }

  interlock(c: ControlDef, next: number, st: Float64Array, sw: Record<string, number>) {
    if (c.key === this.keys.start && next === 1) return this.startBlock(st, sw);
    if (c.key === BOOST.key && next === 1 && st[this.v.temp] > BOOST.rearmC) return `${this.part.name} demasiado caliente (${Math.round(st[this.v.temp])} °C): espera a que baje de ${BOOST.rearmC} °C`;
    return null;
  }

  sounds(): SoundCue[] {
    const V = this.v;
    const part = this.part;
    const lit = (st: Float64Array) => st[V.state] === ENG.run || st[V.state] === ENG.spool;
    return [
      // the burn: from the pilot flame to full thrust (and past it in overdrive)
      { sound: 'eng.roar', role: 'run', part, level: (st) => (lit(st) ? Math.max(0.12 * st[V.n], Math.min(1, st[V.thr])) : 0), pitch: (st) => 0.8 + 0.3 * Math.min(1.4, st[V.thr]) },
      // the turbopumps winding up, then idling under the roar
      { sound: 'eng.spool', role: 'spool', part, level: (st) => (st[V.state] === ENG.spool ? 0.8 : lit(st) ? 0.2 : 0), pitch: (st) => 0.45 + 0.6 * st[V.n] },
      { sound: 'eng.crackle', role: 'boost', part, level: (st) => (st[V.boost] === 1 ? 0.35 + 0.65 * Math.min(1, st[V.thr]) : 0) },
      // wrecked with power and propellant reaching it: spraying and arcing (isolate it!)
      { sound: 'mach.sputter', role: 'hazard', part, level: (st) => (st[V.risk] > 0 ? 0.7 : 0) },
      { sound: 'eng.ignite', role: 'ignite', part, on: (st) => st[V.state] === ENG.spool },
      { sound: 'eng.cutoff', role: 'cutoff', part, on: (st) => !lit(st) },
    ];
  }

  alerts(): AlertDef[] {
    const p = this.part;
    const lamp = p.lamp ?? p.name.toUpperCase();
    return [
      { id: `dmg.${p.id}`, label: `${p.name.toUpperCase()} DAÑADO — AISLAR`, level: 2, lamp, help: 'El motor está por debajo del 35 %: con energía y propelente llegando puede explotar. Cierra su alimentación, desármalo y suéldalo.', on: (st) => this.H(st) < 0.35 },
      { id: `fail.${p.id}`, label: `${p.name.toUpperCase()}: FALLO`, level: 1, lamp, help: 'El motor se apagó solo: sin energía, sin propelente o destruido. Arregla la causa y vuelve a pulsar ARRANQUE.', on: (st) => st[this.v.state] === ENG.fail },
    ];
  }
}

/**
 * VTOL lift pad: a downward nozzle under the belly. No start sequence: it answers while its master
 * switch is on, its circuit has power and propellant reaches it. The flight computer drives it.
 */
export class Lift implements ShipModule {
  readonly id: string;
  private iUse: number;

  constructor(
    private sys: ShipSystems,
    readonly part: PartDef,
  ) {
    this.id = `lift:${part.id}`;
    this.iUse = sys.vars.define(`${partTag(part)}.use`, 0.02);
  }

  loads(t: Tick) {
    const use = t.st[this.iUse];
    if (use > 0 && this.part.feed) t.burn(this.part.id, use * (this.part.p.flowKg ?? 0.3));
  }

  sounds(): SoundCue[] {
    const i = this.iUse;
    return [{ sound: 'eng.lift', role: 'run', part: this.part, level: (st) => Math.min(1, st[i] * 1.2), pitch: (st) => 0.85 + 0.3 * Math.min(1, st[i]) }];
  }

  alerts(): AlertDef[] {
    const p = this.part;
    return [{ id: `dmg.${p.id}`, label: `${p.name.toUpperCase()} DAÑADO`, level: 1, lamp: p.lamp ?? 'VTOL', help: 'Un propulsor de sustentación está por debajo del 35 %: empuja menos y la nave tiene menos margen para quedarse en el aire. Suéldalo.', on: (st) => this.sys.health(st, p) < 0.35 }];
  }
}

export class Rcs implements ShipModule {
  readonly id: string;
  private iUse: number;

  constructor(
    sys: ShipSystems,
    readonly part: PartDef,
  ) {
    this.id = `rcs:${part.id}`;
    this.iUse = sys.vars.define(`${partTag(part)}.use`, 0.02);
  }

  loads(t: Tick) {
    const use = t.st[this.iUse];
    if (use > 0 && this.part.feed) t.burn(this.part.id, use * (this.part.p.flowKg ?? 0.3));
  }

  sounds(): SoundCue[] {
    const i = this.iUse;
    const part = this.part;
    return [
      // cold gas out of the nozzles, and the solenoid valves banging open on every pulse
      { sound: 'rcs.hiss', role: 'run', part, level: (st) => Math.min(1, st[i] * 1.5), pitch: (st) => 0.9 + 0.2 * Math.min(1, st[i]) },
      { sound: 'rcs.pop', role: 'pop', part, on: (st) => st[i] > 0.06 },
    ];
  }
}

export const enginesSystem: SystemFactory = {
  id: 'engines',
  parts: ['engine', 'lift', 'rcs'],
  make: (sys) => [...partsOf(sys, 'engine').map((p) => new Engine(sys, p)), ...partsOf(sys, 'lift').map((p) => new Lift(sys, p)), ...partsOf(sys, 'rcs').map((p) => new Rcs(sys, p))],
};
