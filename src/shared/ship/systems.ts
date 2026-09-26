// The ship's machinery, layer on layer, stepped by the authority (server, or the offline client)
// at SYSTEMS_HZ. Everything reads the switches (`sw`, set by crew at the controls) and the part
// integrities, and writes the continuous state table (`st`, replicated as diffs):
//
//   movers → power demand → power grid → propellant → reactor & coolant → engines/APU →
//   atmosphere & life support → hazards → alerts
//
// It is data driven: parts, circuits, pipes, compartments and openings come from the ship
// definition (hauler.ts), so another ship is another data file. A new kind of machine is a
// `PartType` plus its few lines below.

import { Atmosphere, BREATHABLE, type GasPath } from './atmos.js';
import type { PartDef, ShipDef, SubsystemId } from './def.js';
import { PropellantNet } from './fluid.js';
import type { V3 } from './geom.js';
import { PowerGrid } from './power.js';
import type { VarTable } from './state.js';

export const SYSTEMS_HZ = 20;

/** Reactor output selector positions (fraction of rated power). */
export const REACTOR_SET = [0, 0.25, 0.5, 0.75, 1, 1.1];
export const REACTOR = { kw: 60, heatKw: 100, startS: 15, startKw: 8, warnC: 600, scramC: 750, damageC: 820 };
export const ENGINE = { thrustN: 60000, flowKg: 2.0, spoolS: 3, idle: 0.05, kw: 3 };
export const APU = { kw: 15, startS: 8 };
/** Reactor states. */
export const RX = { off: 0, starting: 1, online: 2, scram: 3 } as const;
/** Engine / APU states. */
export const ENG = { off: 0, spool: 1, run: 2, fail: 3 } as const;

export interface Alert {
  id: string;
  label: string;
  /** 1 caution (amber), 2 warning (red). */
  level: 1 | 2;
  /** Annunciator lamp it lights on the dash. */
  lamp: string;
}

export interface SysContext {
  /** Crew in each compartment breathing cabin air (count per compartment index). */
  crew: number[];
  /** Docked suits drawing ship O2 through a seat umbilical. */
  docked: number;
  /** Astronaut positions in ship space (door / ramp obstruction sensors). */
  bodies: V3[];
  /** Weight on wheels. */
  landed: boolean;
  /** Parked on a pad with a refuelling point. */
  onPad: boolean;
  /** Random numbers (deterministic in tests). */
  rand: () => number;
}

export type SysEvent =
  | { type: 'explode'; at: V3; radius: number; damage: number; cause: string }
  | { type: 'trip'; circuit: SubsystemId }
  | { type: 'destroyed'; part: string }
  | { type: 'say'; text: string };

const hpf = (hp: number, max: number) => Math.max(0, Math.min(1, hp / max));

export class ShipSystems {
  readonly power: PowerGrid;
  readonly fuel: PropellantNet;
  readonly atmos: Atmosphere;
  readonly alerts: Alert[] = [];
  private iHp = new Map<string, number>();
  private iMv = new Map<string, number>();
  private iAl = new Map<string, number>();
  private v: Record<string, number> = {};
  private comp = new Map<string, number>();
  /** Previous alert states (latching the master caution on new ones). */
  private prevAl: Float64Array;

  constructor(
    readonly def: ShipDef,
    readonly vars: VarTable,
  ) {
    for (const p of def.parts) this.iHp.set(p.id, vars.define(`${p.id}.hp`, 0.1, p.maxHp));
    for (const m of def.movers) this.iMv.set(m.key, vars.define(`mv.${m.key}`, 0.01, def.defaults[m.key] ?? 0));
    this.power = new PowerGrid(def, vars);
    this.fuel = new PropellantNet(def.fluid, def.parts, vars);
    const gas = (g: number) => def.parts.find((p) => p.type === 'gas' && p.p.gas === g)!;
    this.atmos = new Atmosphere(def.compartments, vars, { o2: { id: gas(0).id, cap: gas(0).p.cap }, n2: { id: gas(1).id, cap: gas(1).p.cap } });
    def.compartments.forEach((c, i) => this.comp.set(c.id, i));
    const d = (name: string, q: number, init = 0) => (this.v[name] = vars.define(name, q, init));
    // reactor and coolant loop (starts online at its selected output, at equilibrium)
    d('rx.state', 1, RX.online);
    d('rx.out', 0.005, REACTOR_SET[def.defaults['rx.set'] ?? 2]);
    d('rx.temp', 1, 364);
    d('rx.start', 0.02);
    d('rx.cap', 0.01, 1.1);
    d('cool.temp', 1, 322);
    d('cool.flow', 0.02, 1);
    d('rad.area', 0.5, 8);
    d('rad.kw', 0.5);
    // APU
    d('apu.state', 1);
    d('apu.t', 0.1);
    d('apu.out', 0.01);
    // engines
    for (const e of def.parts.filter((p) => p.type === 'engine')) {
      d(`${e.id}.state`, 1);
      d(`${e.id}.n`, 0.01);
      d(`${e.id}.thr`, 0.01);
      d(`${e.id}.cmd`, 0.01);
      d(`${e.id}.temp`, 1, -20);
      d(`${e.id}.risk`, 0.01);
    }
    for (const r of def.parts.filter((p) => p.type === 'rcs')) d(`${r.id}.use`, 0.02);
    // life support
    d('ls.o2', 0.002);
    d('ls.scrub', 0.01);
    d('ls.leak', 1);
    this.defineAlerts();
    this.prevAl = new Float64Array(this.alerts.length);
  }

  /** Index of a named variable of this module. */
  idx(name: string) {
    return this.v[name] ?? this.vars.idx(name);
  }

  hpIndex(part: string) {
    return this.iHp.get(part)!;
  }

  moverIndex(key: string) {
    return this.iMv.get(key);
  }

  compIndex(zone: string | null) {
    return zone === null ? -1 : this.comp.get(zone) ?? -1;
  }

  part(id: string) {
    return this.def.parts.find((p) => p.id === id);
  }

  /** Initial state: compartments filled if the definition says so. */
  init(st: Float64Array) {
    this.def.compartments.forEach((c, i) => {
      if ((this.def.defaults[`${c.id}.p0`] ?? 0) > 0) this.atmos.fill(st, i, this.def.defaults[`${c.id}.p0`]);
    });
    // publish derived values once
    this.atmos.step(0.001, st, [], { o2gen: null, scrub: null, mode: 2, manual: [], supplyO2: false, supplyN2: false, recover: -1, crew: [], heat: true });
  }

  // ------------------------------------------------------------------------------------------------
  // Queries (used by the sim's interlocks, the flight model and the client displays)
  // ------------------------------------------------------------------------------------------------

  hp(st: Float64Array, id: string) {
    return st[this.iHp.get(id)!];
  }

  health(st: Float64Array, p: PartDef) {
    return hpf(st[this.iHp.get(p.id)!], p.maxHp);
  }

  /** Circuit supply fraction (0 = dead, 1 = fully fed). */
  supply(st: Float64Array, c: SubsystemId) {
    return st[this.power.fIndex(c)];
  }

  /** Pressure (kPa) of a compartment, 0 for outside. */
  pressure(st: Float64Array, zone: string | null) {
    const i = this.compIndex(zone);
    return i < 0 ? 0 : st[this.atmos.v[i].p];
  }

  breathable(st: Float64Array, zone: string | null) {
    const i = this.compIndex(zone);
    if (i < 0) return false;
    const v = this.atmos.v[i];
    return st[v.p] > BREATHABLE.p && st[v.po2] > BREATHABLE.po2 && st[v.pco2] < BREATHABLE.pco2;
  }

  /** Coolant flow the loop's pump can give now (0..1). */
  private coolFlow(st: Float64Array, sw: Record<string, number>) {
    const pump = this.def.parts.find((p) => p.type === 'coolpump');
    if (!pump || sw.coolpump !== 1 || this.supply(st, 'cool') < 0.5) return 0;
    return this.health(st, pump) > 0 ? 0.35 + 0.65 * this.health(st, pump) : 0;
  }

  /** Why the reactor can't start now (null = it can). */
  reactorStartBlock(st: Float64Array, sw: Record<string, number>) {
    const rx = this.def.parts.find((p) => p.type === 'reactor')!;
    if (st[this.v['rx.state']] === RX.scram) return 'Reactor en SCRAM · rearme primero';
    if (this.hp(st, rx.id) < rx.maxHp * 0.1) return 'Reactor destruido';
    if (this.coolFlow(st, sw) < 0.5) return 'Enclavamiento: bomba de refrigerante parada';
    const bat = sw.bat === 1 && st[this.power.iSoc] > 0.03;
    const apu = st[this.v['apu.state']] === ENG.run;
    if (!bat && !apu) return 'Sin energía de arranque: conecta la batería o arranca la APU';
    return null;
  }

  /** Why an engine can't be started now (null = it can). */
  engineStartBlock(st: Float64Array, sw: Record<string, number>, id: string) {
    const e = this.part(id)!;
    const state = st[this.v[`${id}.state`]];
    if (state === ENG.spool || state === ENG.run) return 'Motor ya en marcha';
    if (sw[`${id}.arm`] !== 1) return 'Motor desarmado: arma primero (interruptor protegido)';
    if (this.supply(st, 'prop') < 0.5) return 'Sin energía · circuito de PROPULSIÓN';
    if (sw[`v.${id}`] !== 1) return 'Válvula de alimentación cerrada';
    if (!this.fuel.reachable(e.feed!, sw, (p) => this.hp(st, p.id)).some((t) => st[this.fuel.kgIndex(t.id)] > 1)) return 'Sin propelente en los depósitos conectados';
    return null;
  }

  /** Why the APU can't start (null = it can). */
  apuStartBlock(st: Float64Array, sw: Record<string, number>) {
    const apu = this.def.parts.find((p) => p.type === 'apu')!;
    if (this.hp(st, apu.id) <= 0) return 'APU destruida';
    if (!(sw.bat === 1 && st[this.power.iSoc] > 0.03) && st[this.v['rx.state']] !== RX.online) return 'Sin energía para el motor de arranque';
    if (sw[`v.${apu.id}`] !== 1) return 'Válvula de alimentación de la APU cerrada';
    if (!this.fuel.reachable(apu.feed!, sw, (p) => this.hp(st, p.id)).some((t) => st[this.fuel.kgIndex(t.id)] > 0.5)) return 'Sin propelente para la APU';
    return null;
  }

  // ------------------------------------------------------------------------------------------------
  // The step
  // ------------------------------------------------------------------------------------------------

  /**
   * Advance the machinery by dt. `sw` may be changed (tripped breakers, a reactor lever dropping
   * out, consumed pulses, the master caution latching): the changes are returned so the authority
   * can broadcast them, together with events (explosions...).
   */
  tick(dt: number, st: Float64Array, sw: Record<string, number>, ctx: SysContext, holes: (i: number) => boolean, panelLeak: (i: number) => number): { events: SysEvent[]; sw: Record<string, number> } {
    const def = this.def;
    const events: SysEvent[] = [];
    const swOut: Record<string, number> = {};
    const setSw = (k: string, v: number) => {
      if (sw[k] === v) return;
      sw[k] = v;
      swOut[k] = v;
    };
    const V = this.v;
    const H = (p: PartDef) => this.health(st, p);
    const parts = (t: PartDef['type']) => def.parts.filter((p) => p.type === t);
    const rx = parts('reactor')[0];
    const apu = parts('apu')[0];

    // --- pulses (momentary buttons) ------------------------------------------------------------------
    if (sw['rx.scram'] === 1) {
      setSw('rx.scram', 0);
      if (st[V['rx.state']] === RX.online || st[V['rx.state']] === RX.starting) {
        st[V['rx.state']] = RX.scram;
        events.push({ type: 'say', text: 'SCRAM: reactor parado de emergencia' });
      }
    }
    if (sw['rx.reset'] === 1) {
      setSw('rx.reset', 0);
      if (st[V['rx.state']] === RX.scram && st[V['rx.temp']] < 300) {
        st[V['rx.state']] = RX.off;
        setSw('reactor', 0);
      }
    }
    for (const e of parts('engine')) {
      if (sw[`${e.id}.start`] !== 1) continue;
      setSw(`${e.id}.start`, 0);
      if (this.engineStartBlock(st, sw, e.id)) continue;
      st[V[`${e.id}.state`]] = ENG.spool;
      // a damaged engine may blow up the moment it lights
      const risk = this.engineRisk(st, sw, e);
      if (risk > 0 && ctx.rand() < risk * 6) this.blowEngine(st, sw, e, events, setSw);
    }

    // --- movers ----------------------------------------------------------------------------------------
    const moving = new Set<string>();
    for (const m of def.movers) {
      const i = this.iMv.get(m.key)!;
      const target = sw[m.key] ?? 0;
      if (Math.abs(st[i] - target) < 1e-6 || this.supply(st, m.circuit) < 0.5) continue;
      if (target < st[i] && this.obstructed(m.key, ctx.bodies)) continue;
      const k = m.rate * dt;
      st[i] = target > st[i] ? Math.min(target, st[i] + k) : Math.max(target, st[i] - k);
      moving.add(m.key);
    }

    // --- power demand per circuit ------------------------------------------------------------------------
    const demand: Record<string, number> = {};
    for (const c of def.subsystems) demand[c.id] = c.base;
    const load = (c: SubsystemId | undefined, kw: number) => {
      if (c) demand[c] += kw;
    };
    for (const z of def.zones) if (sw[z.lightKey] === 1) load('lights', 0.35);
    if (sw['light.nav'] === 1) load('ext', 0.15);
    if (sw['light.beacon'] === 1) load('ext', 0.1);
    if (sw['light.landing'] === 1) load('ext', 2);
    for (const m of def.movers) if (moving.has(m.key)) load(m.circuit, m.load);
    for (const t of this.fuel.tanks) if (sw[`pump.${t.id}`] === 1) load('prop', 0.8);
    if ((sw.xfer ?? 0) > 0) load('prop', 1);
    if (sw.rcs === 1) load('prop', 0.4);
    for (const e of parts('engine')) {
      const s = st[V[`${e.id}.state`]];
      if (s === ENG.spool || s === ENG.run) load('prop', ENGINE.kw);
    }
    if (st[V['apu.state']] === ENG.spool) load('prop', 3);
    if (st[V['rx.state']] === RX.starting) load('cool', REACTOR.startKw);
    if (sw.coolpump === 1) load('cool', 2.5);
    if (sw.o2gen === 1) load('life', 4);
    if (sw.scrub === 1) load('life', 1.5);
    if (sw.fans === 1) load('life', 1);
    if (sw.heat === 1) load('life', 0.6 * def.compartments.filter((_, i) => st[this.atmos.v[i].p] > 5).length);
    if (sw['ls.recover'] === 1) load('life', 5);
    const radarMode = Math.round(sw['radar.mode'] ?? 0);
    if (radarMode === 1) load('sensors', 0.5);
    if (radarMode === 2) load('sensors', 3);
    if (sw.turret === 1) load('weapons', 1.5);
    if (sw.grav === 1) load('grav', 6);
    // short circuits: a badly damaged machine that is switched on draws erratically
    for (const p of def.parts) {
      if (!p.circuit || p.p.kw === undefined) continue;
      const h = H(p);
      if (h > 0 && h < 0.25 && this.partOn(p, sw)) load(p.circuit, p.p.kw * (1.5 + 2.5 * ctx.rand()));
    }

    // --- power grid --------------------------------------------------------------------------------------
    const rxOut = st[V['rx.state']] === RX.online ? st[V['rx.out']] : 0;
    const apuOn = st[V['apu.state']] === ENG.run;
    const apuAvail = apuOn ? APU.kw * (0.4 + 0.6 * H(apu)) : 0;
    const bat = def.parts.find((p) => p.type === 'battery')!;
    const rxQ = Math.max(0.25, Math.min(1, (H(rx) - 0.2) / 0.6));
    const tripped = this.power.step(dt, st, sw, demand, (c) => this.circuitLive(c, sw, holes), {
      reactor: REACTOR.kw * rxOut,
      apu: apuAvail,
      battery: sw.bat === 1 && H(bat) > 0,
      batteryHealth: H(bat),
      reactorQuality: rxQ,
    });
    for (const c of tripped) {
      const s = def.subsystems.find((x) => x.id === c)!;
      swOut[s.breaker] = 0;
      events.push({ type: 'trip', circuit: c });
    }
    st[this.power.iRx] = REACTOR.kw * rxOut;
    const served = st[this.power.iLoad];
    st[V['apu.out']] = apuOn ? Math.min(1, Math.max(0, served - REACTOR.kw * rxOut) / APU.kw) : 0;
    st[this.power.iApu] = st[V['apu.out']] * APU.kw;
    // damaged reactor: output sags and flickers
    if (rxOut > 0 && H(rx) < 0.4) st[this.power.iQuality] *= 0.85 + 0.15 * ctx.rand();

    // --- propellant ----------------------------------------------------------------------------------------
    const fuelDemand: Record<string, number> = {};
    for (const e of parts('engine')) {
      const s = st[V[`${e.id}.state`]];
      if (s === ENG.spool || s === ENG.run) fuelDemand[e.id] = ENGINE.flowKg * Math.max(ENGINE.idle * 0.5, st[V[`${e.id}.thr`]]);
    }
    if (st[V['apu.state']] !== ENG.off) fuelDemand[apu.id] = 0.003 + 0.012 * st[V['apu.out']];
    for (const r of parts('rcs')) fuelDemand[r.id] = st[V[`${r.id}.use`]] * 0.3;
    const propOk = this.supply(st, 'prop') >= 0.5;
    this.fuel.step(dt, st, sw, {
      demand: fuelDemand,
      pumped: (t) => propOk && sw[`pump.${t.id}`] === 1 && H(t) > 0.2,
      transferPower: propOk,
      refuel: ctx.onPad && ctx.landed && sw.refuel === 1,
      hp: (p) => this.hp(st, p.id),
    });

    // --- reactor, coolant loop, radiators ------------------------------------------------------------------
    const rs = st[V['rx.state']];
    const flow = this.coolFlow(st, sw);
    st[V['cool.flow']] = flow;
    const cap = H(rx) >= 0.6 ? 1.1 : Math.max(0, (H(rx) / 0.6) * 1.1);
    st[V['rx.cap']] = cap;
    if (rs === RX.off && sw.reactor === 1) {
      const why = this.reactorStartBlock(st, sw);
      if (why) {
        setSw('reactor', 0);
        events.push({ type: 'say', text: `Arranque del reactor abortado: ${why}` });
      } else {
        st[V['rx.state']] = RX.starting;
        st[V['rx.start']] = 0;
      }
    } else if (rs === RX.starting) {
      const why = sw.reactor !== 1 ? 'palanca en PARADO' : flow < 0.5 ? 'sin refrigeración' : this.supply(st, 'cool') < 0.5 ? 'sin energía de arranque' : null;
      if (why) {
        st[V['rx.state']] = RX.off;
        setSw('reactor', 0);
        events.push({ type: 'say', text: `Arranque del reactor abortado: ${why}` });
      } else {
        st[V['rx.start']] += dt / REACTOR.startS;
        if (st[V['rx.start']] >= 1) {
          st[V['rx.state']] = RX.online;
          st[V['rx.start']] = 1;
          st[V['rx.out']] = 0;
        }
      }
    } else if (rs === RX.online) {
      const target = sw.reactor === 1 ? Math.min(cap, REACTOR_SET[Math.round(sw['rx.set'] ?? 2)] ?? 0.5) : 0;
      const o = st[V['rx.out']];
      st[V['rx.out']] = target > o ? Math.min(target, o + 0.05 * dt) : Math.max(target, o - 0.08 * dt);
      if (H(rx) < 0.4) st[V['rx.out']] = Math.max(0, st[V['rx.out']] * (1 - 0.08 * ctx.rand() * dt * 10));
      if (sw.reactor !== 1 && st[V['rx.out']] < 0.01) st[V['rx.state']] = RX.off;
      if (H(rx) < 0.15 && ctx.rand() < 0.04 * dt) {
        st[V['rx.state']] = RX.scram;
        events.push({ type: 'say', text: 'SCRAM automático: inestabilidad del núcleo (reactor dañado)' });
      }
    } else if (rs === RX.scram) st[V['rx.out']] = 0;
    if (st[V['rx.state']] !== RX.online) st[V['rx.out']] = Math.max(0, st[V['rx.out']] - 0.5 * dt);
    // heat: core → coolant (pumped) → radiators → space
    const q = st[V['rx.out']] * REACTOR.heatKw;
    const ua = 1.2 * Math.max(0.04, flow);
    const tCore = st[V['rx.temp']];
    const tCool = st[V['cool.temp']];
    const toCool = ua * (tCore - tCool);
    let area = 0;
    for (const r of parts('radiator')) area += (r.p.stowed + (r.p.deployed - r.p.stowed) * (st[this.iMv.get('rad') ?? -1] ?? 0)) * H(r);
    const K = tCool + 273.15;
    const radKw = (0.9 * 5.67e-8 * area * (K ** 4 - 230 ** 4)) / 1000;
    st[V['rad.area']] = area;
    st[V['rad.kw']] = radKw;
    st[V['rx.temp']] = tCore + ((q - toCool) / 60) * dt;
    st[V['cool.temp']] = Math.max(10, tCool + ((toCool + 1.5 - radKw) / 150) * dt);
    if (st[V['rx.temp']] > REACTOR.scramC && (st[V['rx.state']] === RX.online || st[V['rx.state']] === RX.starting)) {
      st[V['rx.state']] = RX.scram;
      events.push({ type: 'say', text: `SCRAM automático: núcleo a ${Math.round(st[V['rx.temp']])} °C` });
    }
    if (st[V['rx.temp']] > REACTOR.damageC) this.damagePart(st, rx, 2.5 * dt, events);

    // --- APU ------------------------------------------------------------------------------------------------
    const as = st[V['apu.state']];
    if (sw.apu !== 1) {
      st[V['apu.state']] = ENG.off;
      st[V['apu.t']] = 0;
    } else if (as === ENG.off || as === ENG.fail) {
      if (!this.apuStartBlock(st, sw)) {
        st[V['apu.state']] = ENG.spool;
        st[V['apu.t']] = 0;
      } else if (as === ENG.off) {
        st[V['apu.state']] = ENG.fail;
        events.push({ type: 'say', text: `APU: ${this.apuStartBlock(st, sw)}` });
      }
    } else if (as === ENG.spool) {
      st[V['apu.t']] += dt / APU.startS;
      const starved = st[this.fuel.feedIndex(apu.id)] < 0.3 && st[V['apu.t']] > 0.2;
      if (starved || (H(apu) < 0.3 && ctx.rand() < 0.3 * dt)) {
        st[V['apu.state']] = ENG.fail;
        events.push({ type: 'say', text: starved ? 'APU: sin alimentación de propelente' : 'APU: fallo de arranque (dañada)' });
      } else if (st[V['apu.t']] >= 1) st[V['apu.state']] = ENG.run;
    } else if (as === ENG.run && (st[this.fuel.feedIndex(apu.id)] < 0.3 || H(apu) <= 0)) {
      st[V['apu.state']] = ENG.fail;
      events.push({ type: 'say', text: 'APU: se ha apagado (alimentación / daños)' });
    }

    // --- main engines ---------------------------------------------------------------------------------------
    for (const e of parts('engine')) {
      const s = V[`${e.id}.state`];
      const armed = sw[`${e.id}.arm`] === 1;
      const fed = st[this.fuel.feedIndex(e.id)];
      const powered = this.supply(st, 'prop') >= 0.5;
      if (st[s] === ENG.spool || st[s] === ENG.run) {
        const why = !armed ? 'desarmado' : !powered ? 'sin energía' : H(e) <= 0 ? 'destruido' : fed < 0.3 && st[V[`${e.id}.n`]] > 0.3 ? 'sin propelente' : null;
        if (why) {
          st[s] = why === 'desarmado' ? ENG.off : ENG.fail;
          if (why !== 'desarmado') events.push({ type: 'say', text: `${e.name}: apagado (${why})` });
        }
      }
      const n = V[`${e.id}.n`];
      if (st[s] === ENG.spool) {
        st[n] = Math.min(1, st[n] + dt / ENGINE.spoolS);
        if (st[n] >= 1) st[s] = ENG.run;
      } else if (st[s] !== ENG.run) st[n] = Math.max(0, st[n] - dt / 2);
      // thrust follows the command with a spool lag, capped by health and by the feed
      const maxThr = st[s] === ENG.run ? Math.min(1, 0.25 + 0.75 * H(e)) * Math.min(1, fed / 0.9) : 0;
      const want = st[s] === ENG.run ? Math.max(ENGINE.idle, Math.min(maxThr, st[V[`${e.id}.cmd`]])) : 0;
      const thr = V[`${e.id}.thr`];
      st[thr] += (want - st[thr]) * Math.min(1, dt * 2.5);
      const temp = V[`${e.id}.temp`];
      const hot = st[s] === ENG.run || st[s] === ENG.spool ? -20 + 950 * Math.max(ENGINE.idle, st[thr]) : -20;
      st[temp] += (hot - st[temp]) * Math.min(1, dt / (hot > st[temp] ? 6 : 40));
      // the hazard the crew must isolate: a wrecked engine with power and propellant reaching it
      const risk = this.engineRisk(st, sw, e);
      st[V[`${e.id}.risk`]] = risk;
      if (risk > 0 && ctx.rand() < risk * dt) this.blowEngine(st, sw, e, events, setSw);
      void armed;
    }

    // --- atmosphere and life support --------------------------------------------------------------------------
    const lifeOk = this.supply(st, 'life') >= 0.5;
    const o2gen = parts('o2gen')[0];
    const scrub = parts('scrubber')[0];
    const q2 = st[this.power.iQuality] ** 2;
    const o2rate = o2gen && sw.o2gen === 1 && lifeOk ? 0.125 * H(o2gen) * q2 : 0;
    st[V['ls.o2']] = o2rate;
    const scrubEff = scrub && sw.scrub === 1 && lifeOk ? H(scrub) : 0;
    st[V['ls.scrub']] = scrubEff;
    const paths = this.gasPaths(st, sw, holes, panelLeak, lifeOk);
    const gasO2 = def.parts.find((p) => p.type === 'gas' && p.p.gas === 0)!;
    const gasN2 = def.parts.find((p) => p.type === 'gas' && p.p.gas === 1)!;
    // a holed bottle bleeds into its compartment
    for (const [b, key] of [[gasO2, this.atmos.iO2], [gasN2, this.atmos.iN2]] as const) {
      const h = H(b);
      if (h >= 0.4 || st[key] <= 0) continue;
      const kg = Math.min(st[key], (h <= 0 ? 2 : 0.25 * (1 - h / 0.4)) * dt);
      st[key] -= kg;
      const ci = this.compIndex(b.zone);
      if (ci >= 0) {
        const v = this.atmos.v[ci];
        if (b.p.gas === 0) st[v.o2] += kg / 0.032;
        else st[v.n2] += kg / 0.028;
      }
    }
    // docked suits drink from the O2 bottles through the seat umbilical
    st[this.atmos.iO2] = Math.max(0, st[this.atmos.iO2] - ctx.docked * 0.0004 * dt);
    this.atmos.step(dt, st, paths, {
      o2gen: o2gen ? { comp: this.compIndex(o2gen.zone), rate: o2rate } : null,
      scrub: scrub ? { comp: this.compIndex(scrub.zone), eff: scrubEff } : null,
      mode: Math.round(sw['ls.mode'] ?? 0),
      manual: def.compartments.map((c) => sw[`repress.${c.id}`] === 1),
      supplyO2: lifeOk && sw['v.gasO2'] === 1 && H(gasO2) > 0,
      supplyN2: lifeOk && sw['v.gasN2'] === 1 && H(gasN2) > 0,
      recover: sw['ls.recover'] === 1 && lifeOk ? this.compIndex(def.defaults['ls.recoverZone'] !== undefined ? def.compartments[def.defaults['ls.recoverZone']].id : 'cargo') : -1,
      crew: ctx.crew,
      heat: sw.heat === 1 && lifeOk,
    });
    st[V['ls.leak']] = def.compartments.some((_, i) => st[this.atmos.v[i].sealed] === 0 && st[this.atmos.v[i].p] > 2) ? 1 : 0;

    // --- alerts & master caution ---------------------------------------------------------------------------------
    let fresh = false;
    this.alerts.forEach((a, k) => {
      const on = this.alertOn(a.id, st, sw, holes) ? 1 : 0;
      st[this.iAl.get(a.id)!] = on;
      if (on && !this.prevAl[k]) fresh = true;
      this.prevAl[k] = on;
    });
    if (fresh) setSw('caution', 1);
    return { events, sw: swOut };
  }

  /** Circuit energised as far as wiring goes: breaker closed, conduit not cut. */
  circuitLive(c: SubsystemId, sw: Record<string, number>, holes: (i: number) => boolean) {
    const s = this.def.subsystems.find((x) => x.id === c)!;
    if (sw[s.breaker] !== 1) return false;
    for (const p of this.def.panels) if (p.conduits.includes(c) && holes(p.index)) return false;
    return true;
  }

  /** Is a machine switched on (for short-circuit loads)? */
  private partOn(p: PartDef, sw: Record<string, number>) {
    switch (p.type) {
      case 'coolpump':
        return sw.coolpump === 1;
      case 'o2gen':
        return sw.o2gen === 1;
      case 'scrubber':
        return sw.scrub === 1;
      case 'radar':
        return (sw['radar.mode'] ?? 0) > 0;
      case 'turret':
        return sw.turret === 1;
      case 'grav':
        return sw.grav === 1;
      default:
        return true;
    }
  }

  /** Explosion hazard per second of an engine: badly damaged + energised + propellant reaching it. */
  engineRisk(st: Float64Array, sw: Record<string, number>, e: PartDef) {
    const h = this.health(st, e);
    if (h >= 0.35) return 0;
    const energised = sw[`${e.id}.arm`] === 1 && this.supply(st, 'prop') >= 0.5;
    const fuel = sw[`v.${e.id}`] === 1 && this.fuel.reachable(e.feed!, sw, (p) => this.hp(st, p.id)).some((t) => st[this.fuel.kgIndex(t.id)] > 1);
    if (!energised || !fuel) return 0;
    const s = st[this.v[`${e.id}.state`]];
    if (h <= 0) return 0.25; // propellant pouring into a wrecked nacelle, sparks from live wiring
    return (1 - h / 0.35) * (s === ENG.run || s === ENG.spool ? 0.12 : 0.03);
  }

  private blowEngine(st: Float64Array, sw: Record<string, number>, e: PartDef, events: SysEvent[], setSw: (k: string, v: number) => void) {
    st[this.iHp.get(e.id)!] = 0;
    st[this.v[`${e.id}.state`]] = ENG.fail;
    st[this.v[`${e.id}.thr`]] = 0;
    setSw(`${e.id}.arm`, 0);
    events.push({ type: 'explode', at: e.c, radius: 5, damage: 110, cause: `${e.name}: explosión` });
    events.push({ type: 'destroyed', part: e.id });
    void sw;
  }

  /** Blast / overheat damage to a part (tanks with propellant go up in a secondary explosion). */
  damagePart(st: Float64Array, p: PartDef, amount: number, events: SysEvent[]) {
    const i = this.iHp.get(p.id)!;
    if (st[i] <= 0) return;
    st[i] = Math.max(0, st[i] - amount);
    if (st[i] > 0) return;
    events.push({ type: 'destroyed', part: p.id });
    if (p.type === 'tank') {
      const kg = st[this.fuel.kgIndex(p.id)];
      if (kg > 60) events.push({ type: 'explode', at: p.c, radius: Math.min(9, 3 + kg / 250), damage: Math.min(160, 50 + kg / 12), cause: `${p.name}: rotura con propelente` });
    }
  }

  /** Distance from a ship-space point to a part's box. */
  partDistance(p: PartDef, at: V3) {
    const c = Math.cos(p.yaw);
    const s = Math.sin(p.yaw);
    const dx = at[0] - p.c[0];
    const dz = at[2] - p.c[2];
    const lx = dx * c - dz * s;
    const lz = dx * s + dz * c;
    const qx = Math.max(0, Math.abs(lx) - p.half[0]);
    const qy = Math.max(0, Math.abs(at[1] - p.c[1]) - p.half[1]);
    const qz = Math.max(0, Math.abs(lz) - p.half[2]);
    return Math.hypot(qx, qy, qz);
  }

  // ------------------------------------------------------------------------------------------------
  // Gas paths: doors / ramp / vents / ducts from the definition, breaches and cracks from the panels
  // ------------------------------------------------------------------------------------------------

  private gasPaths(st: Float64Array, sw: Record<string, number>, holes: (i: number) => boolean, panelLeak: (i: number) => number, fans: boolean): GasPath[] {
    const out: GasPath[] = [];
    for (const o of this.def.openings) {
      const a = this.compIndex(o.a);
      const b = o.b === null ? -1 : this.compIndex(o.b);
      let open = 0;
      if (o.kind === 'door' || o.kind === 'ramp') open = st[this.iMv.get(o.key) ?? -1] ?? 0;
      else if (o.kind === 'vent') open = sw[o.key] === 1 ? 1 : 0;
      else if (o.kind === 'duct') {
        const on = sw[o.key] === 1;
        if (on) out.push({ a, b, area: o.area, mix: fans && sw.fans === 1 ? 0.4 : 0 });
        continue;
      }
      if (open > 0) out.push({ a, b, area: o.area * open });
    }
    for (const p of this.def.panels) {
      const a = this.compIndex(p.zone);
      if (a < 0) continue;
      const b = p.kind === 'bulkhead' ? this.compIndex(p.other ?? null) : -1;
      const area = holes(p.index) ? panelArea(p.poly) : panelLeak(p.index);
      if (area > 0) out.push({ a, b, area });
    }
    return out;
  }

  /** Door / ramp obstruction sensors (ship-space bodies). */
  private obstructed(key: string, bodies: V3[]) {
    const def = this.def;
    for (const l of bodies) {
      if (key === def.ramp.key) {
        if (Math.abs(l[0]) < def.ramp.w / 2 + 0.3 && l[2] > def.ramp.hinge[2] - 0.45 && l[2] < def.ramp.hinge[2] + def.ramp.length + 0.4 && l[1] < 0.6) return true;
        continue;
      }
      const d = def.doors.find((x) => x.key === key);
      if (d && Math.abs(l[0] - d.c[0]) < d.w / 2 + 0.3 && Math.abs(l[2] - d.c[2]) < 0.55 && l[1] > -0.5 && l[1] < d.h) return true;
    }
    return false;
  }

  // ------------------------------------------------------------------------------------------------
  // Alerts
  // ------------------------------------------------------------------------------------------------

  private defineAlerts() {
    const A = (id: string, label: string, level: 1 | 2, lamp: string) => {
      this.alerts.push({ id, label, level, lamp });
      this.iAl.set(id, this.vars.define(`al.${id}`, 1));
    };
    A('breach', 'BRECHA EN EL CASCO', 2, 'CASCO');
    for (const c of this.def.compartments) {
      A(`depress.${c.id}`, `DESCOMPRESIÓN · ${c.label}`, 2, 'DESCOMP');
      A(`o2.${c.id}`, `O₂ BAJO · ${c.label}`, 1, 'O2');
      A(`co2.${c.id}`, `CO₂ ALTO · ${c.label}`, 1, 'CO2');
    }
    A('leak', 'FUGA DE AIRE · REPRESURIZACIÓN SUSPENDIDA', 1, 'FUGA AIRE');
    A('gas', 'RESERVA DE O₂/N₂ BAJA', 1, 'GAS');
    A('rx.temp', 'REACTOR: SOBRETEMPERATURA', 2, 'REACTOR');
    A('rx.scram', 'REACTOR EN SCRAM', 2, 'SCRAM');
    A('rx.dmg', 'REACTOR DAÑADO', 1, 'REACTOR');
    A('cool', 'SIN REFRIGERACIÓN DEL REACTOR', 2, 'REFRIG');
    A('cool.temp', 'REFRIGERANTE CALIENTE', 1, 'REFRIG');
    A('bat', 'BATERÍA BAJA', 1, 'BATERÍA');
    A('onbat', 'EN BATERÍA', 1, 'BATERÍA');
    A('shed', 'DESLASTRE DE CARGAS', 1, 'DESLASTRE');
    A('trip', 'DISYUNTOR ABIERTO', 1, 'DISYUNTOR');
    A('fuel', 'PROPELENTE BAJO', 1, 'COMBUST');
    A('fuel.imb', 'DESEQUILIBRIO DE PROPELENTE', 1, 'DESEQUIL');
    for (const t of this.fuel.tanks) A(`leak.${t.id}`, `FUGA DE PROPELENTE · ${t.name}`, 2, 'FUGA COMB');
    for (const e of this.def.parts.filter((p) => p.type === 'engine')) {
      A(`dmg.${e.id}`, `${e.name.toUpperCase()} DAÑADO — AISLAR`, 2, e.id === 'eng.L' ? 'MOTOR IZQ' : 'MOTOR DER');
      A(`fail.${e.id}`, `${e.name.toUpperCase()}: FALLO`, 1, e.id === 'eng.L' ? 'MOTOR IZQ' : 'MOTOR DER');
    }
    A('life', 'SOPORTE VITAL DEGRADADO', 1, 'SOP VITAL');
    A('apu', 'APU: FALLO', 1, 'APU');
  }

  private alertOn(id: string, st: Float64Array, sw: Record<string, number>, holes: (i: number) => boolean): boolean {
    const def = this.def;
    const V = this.v;
    const comp = (z: string) => this.atmos.v[this.compIndex(z)];
    const tanks = (a: string, b: string) => {
      const L = this.fuel.tanks.find((t) => t.id === a);
      const R = this.fuel.tanks.find((t) => t.id === b);
      return L && R ? Math.abs(st[this.fuel.kgIndex(L.id)] - st[this.fuel.kgIndex(R.id)]) : 0;
    };
    const after = (prefix: string) => id.slice(prefix.length);
    if (id === 'breach') return def.panels.some((p) => p.kind !== 'bulkhead' && holes(p.index));
    if (id.startsWith('depress.')) return st[comp(after('depress.')).dpdt] < -1.5;
    if (id.startsWith('o2.')) {
      const v = comp(after('o2.'));
      return st[v.p] > 5 && st[v.po2] < 16;
    }
    if (id.startsWith('co2.')) return st[comp(after('co2.')).pco2] > 1;
    if (id === 'leak') return st[V['ls.leak']] === 1 && (sw['ls.mode'] ?? 0) === 0;
    if (id === 'gas') return st[this.atmos.iO2] < this.atmos.o2Cap * 0.15 || st[this.atmos.iN2] < this.atmos.n2Cap * 0.15;
    if (id === 'rx.temp') return st[V['rx.temp']] > REACTOR.warnC;
    if (id === 'rx.scram') return st[V['rx.state']] === RX.scram;
    if (id === 'rx.dmg') return this.health(st, def.parts.find((p) => p.type === 'reactor')!) < 0.5;
    if (id === 'cool') return (st[V['rx.state']] === RX.online || st[V['rx.state']] === RX.starting) && st[V['cool.flow']] < 0.5;
    if (id === 'cool.temp') return st[V['cool.temp']] > 450;
    if (id === 'bat') return st[this.power.iSoc] < 0.2;
    if (id === 'onbat') return st[this.power.iBatKw] > 0.5 && st[V['rx.state']] !== RX.online;
    if (id === 'shed') return st[this.power.iShed] === 1;
    if (id === 'trip') return def.subsystems.some((c) => sw[c.breaker] !== 1);
    if (id === 'fuel') return st[this.fuel.iTotal] < st[this.fuel.iCap] * 0.15;
    if (id === 'fuel.imb') return tanks('tank.L', 'tank.R') > 350;
    if (id.startsWith('leak.')) return st[this.vars.idx(`${after('leak.')}.leak`)] > 0.05;
    if (id.startsWith('dmg.')) return this.health(st, this.part(after('dmg.'))!) < 0.35;
    if (id.startsWith('fail.')) return st[V[`${after('fail.')}.state`]] === ENG.fail;
    if (id === 'life') {
      const o2 = def.parts.find((p) => p.type === 'o2gen');
      const sc = def.parts.find((p) => p.type === 'scrubber');
      return (!!o2 && (sw.o2gen !== 1 || this.health(st, o2) < 0.5 || st[V['ls.o2']] < 0.06)) || (!!sc && (sw.scrub !== 1 || this.health(st, sc) < 0.5));
    }
    if (id === 'apu') return st[V['apu.state']] === ENG.fail;
    return false;
  }

  /** Active alerts (for displays). */
  active(st: Float64Array) {
    return this.alerts.filter((a) => st[this.iAl.get(a.id)!] === 1);
  }

  alertIndex(id: string) {
    return this.iAl.get(id)!;
  }
}

/** Area of a convex polygon (m²). */
export function panelArea(poly: Array<[number, number]>) {
  let a = 0;
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i];
    const q = poly[(i + 1) % poly.length];
    a += p[0] * q[1] - q[0] * p[1];
  }
  return Math.abs(a) / 2;
}

