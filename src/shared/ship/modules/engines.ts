// Main engines and RCS thrusters. An engine is armed (guarded switch), started with a pulse,
// spools up on its circuit's power and propellant, and follows its throttle command (`.cmd`,
// for the flight model) capped by its health and its feed. A badly damaged engine with power and
// propellant reaching it can explode: the crew isolates it by closing its feed valve and
// disarming it. RCS thrusters burn propellant in proportion to their use (`.use`).

import { partKey, partTag, type ControlDef, type PartDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import { partsOf, type AlertDef, type ShipModule, type SystemFactory, type Tick } from './api.js';
import { ENG } from './apu.js';

export { ENG };
export const ENGINE = { thrustN: 60000, flowKg: 2.0, spoolS: 3, idle: 0.05, kw: 3 };

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
      ['cmd', 0.01, 0],
      ['temp', 1, -20],
      ['risk', 0.01, 0],
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
    if (this.part.feed) t.burn(this.part.id, this.k.flowKg * Math.max(this.k.idle * 0.5, st[this.v.thr]));
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
    // thrust follows the command with a spool lag, capped by health and by the feed
    const running = st[V.state] === ENG.run;
    const maxThr = running ? Math.min(1, 0.25 + 0.75 * H) * Math.min(1, fed / 0.9) : 0;
    const want = running ? Math.max(k.idle, Math.min(maxThr, st[V.cmd])) : 0;
    st[V.thr] += (want - st[V.thr]) * Math.min(1, dt * 2.5);
    const hot = running || st[V.state] === ENG.spool ? -20 + 950 * Math.max(k.idle, st[V.thr]) : -20;
    st[V.temp] += (hot - st[V.temp]) * Math.min(1, dt / (hot > st[V.temp] ? 6 : 40));
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
    return null;
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
}

export const enginesSystem: SystemFactory = {
  id: 'engines',
  parts: ['engine', 'rcs'],
  make: (sys) => [...partsOf(sys, 'engine').map((p) => new Engine(sys, p)), ...partsOf(sys, 'rcs').map((p) => new Rcs(sys, p))],
};
