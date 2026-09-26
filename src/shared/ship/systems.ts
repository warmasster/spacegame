// The ship's machinery kernel, stepped by the authority (server, or the offline client) at
// SYSTEMS_HZ. It knows no machine by name: it builds the variable table shared by every system
// (part integrity, mover travel), asks each system factory (modules/index.ts) for the modules this
// ship needs, and runs them in phases (see modules/api.ts):
//
//   input → loads → solve (power grid → propellant → atmosphere) → step → alerts
//
// Everything reads the switches (`sw`, set by crew at the controls) and writes the continuous
// state table (`st`, replicated as diffs). Another ship is another data file; another kind of
// machine is another module.

import { partTag, type ControlDef, type PartDef, type ShipDef, type SubsystemId } from './def.js';
import type { V3 } from './geom.js';
import { hpf, panelArea, type AlertDef, type InterlockEnv, type PowerSource, type ShipModule, type SysContext, type SysEvent, type Tick } from './modules/api.js';
import { SYSTEM_FACTORIES } from './modules/index.js';
import type { LifeSupport } from './modules/life.js';
import type { PowerGrid } from './modules/power.js';
import type { PropellantNet } from './modules/propellant.js';
import type { VarTable } from './state.js';

export type { SysContext, SysEvent } from './modules/api.js';
export { panelArea };

export const SYSTEMS_HZ = 20;

export interface Alert {
  id: string;
  label: string;
  level: 1 | 2;
  lamp: string;
  help?: string;
}

export class ShipSystems {
  readonly modules: ShipModule[] = [];
  readonly alerts: Alert[] = [];
  private alertTest: AlertDef['on'][] = [];
  private iHp = new Map<string, number>();
  private iMv = new Map<string, number>();
  private iAl = new Map<string, number>();
  private comp = new Map<string, number>();
  private services = new Map<string, unknown>();
  private solvers: ShipModule[];
  /** Previous alert states (latching the master caution on new ones). */
  private prevAl: Float64Array;

  constructor(
    readonly def: ShipDef,
    readonly vars: VarTable,
  ) {
    for (const p of def.parts) this.iHp.set(p.id, vars.define(`${p.id}.hp`, 0.1, p.maxHp));
    for (const m of def.movers) this.iMv.set(m.key, vars.define(`mv.${m.key}`, 0.01, def.defaults[m.key] ?? 0));
    def.compartments.forEach((c, i) => this.comp.set(c.id, i));
    // systems: every factory the ship asks for (or every one that finds something to drive)
    const known = new Set(SYSTEM_FACTORIES.map((f) => f.id));
    for (const id of def.systems ?? []) if (!known.has(id)) throw new Error(`ship ${def.id}: system "${id}" does not exist (modules/index.ts)`);
    const claimed = new Set<string>();
    for (const f of SYSTEM_FACTORIES) {
      if (def.systems && !def.systems.includes(f.id)) continue;
      for (const t of f.parts ?? []) claimed.add(t);
      this.modules.push(...f.make(this));
    }
    for (const p of def.parts) if (!claimed.has(p.type)) throw new Error(`ship ${def.id}: no system drives part type "${p.type}" (${p.id})`);
    this.solvers = this.modules.filter((m) => m.solve).sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
    // alerts: the hull's own, then every module's
    this.addAlert({
      id: 'breach',
      label: 'BRECHA EN EL CASCO',
      level: 2,
      lamp: 'CASCO',
      help: 'Un panel del casco (no un mamparo interior) ha reventado: ese compartimento está abierto al vacío. Suéldalo hasta cerrarlo; la página CASCO marca dónde.',
      on: () => this.holesNow,
    });
    for (const m of this.modules) for (const a of m.alerts?.() ?? []) this.addAlert(a);
    this.prevAl = new Float64Array(this.alerts.length);
  }

  private addAlert(a: AlertDef) {
    if (this.iAl.has(a.id)) throw new Error(`alert ${a.id} defined twice`);
    this.alerts.push({ id: a.id, label: a.label, level: a.level, lamp: a.lamp, help: a.help });
    this.alertTest.push(a.on);
    this.iAl.set(a.id, this.vars.define(`al.${a.id}`, 1));
  }

  // ------------------------------------------------------------------------------------------------
  // Services (networks other modules use) and module lookup
  // ------------------------------------------------------------------------------------------------

  provide(id: string, service: unknown) {
    this.services.set(id, service);
  }

  use<T>(id: string): T | undefined {
    return this.services.get(id) as T | undefined;
  }

  get power() {
    return this.use<PowerGrid>('power');
  }

  get fuel() {
    return this.use<PropellantNet>('propellant');
  }

  get life() {
    return this.use<LifeSupport>('life');
  }

  module<T extends ShipModule>(id: string): T | undefined {
    return this.modules.find((m) => m.id === id) as T | undefined;
  }

  // ------------------------------------------------------------------------------------------------
  // Queries (interlocks, displays, the crew rules)
  // ------------------------------------------------------------------------------------------------

  /** Index of a named variable. */
  idx(name: string) {
    return this.vars.idx(name);
  }

  hpIndex(part: string) {
    return this.iHp.get(part)!;
  }

  moverIndex(key: string) {
    return this.iMv.get(key);
  }

  /** Travel of a mover 0..1 (0 when the ship has no such mover). */
  mover(st: Float64Array, key: string) {
    const i = this.iMv.get(key);
    return i === undefined ? 0 : st[i];
  }

  compIndex(zone: string | null) {
    return zone === null ? -1 : this.comp.get(zone) ?? -1;
  }

  part(id: string) {
    return this.def.parts.find((p) => p.id === id);
  }

  tag(p: PartDef) {
    return partTag(p);
  }

  hp(st: Float64Array, id: string) {
    return st[this.iHp.get(id)!];
  }

  health(st: Float64Array, p: PartDef) {
    return hpf(st[this.iHp.get(p.id)!], p.maxHp);
  }

  /** Circuit supply fraction (0 = dead, 1 = fully fed). Unwired (undefined) = always fed. */
  supply(st: Float64Array, c: SubsystemId | undefined) {
    if (c === undefined) return 1;
    return this.power ? st[this.power.fIndex(c)] : 0;
  }

  /** Circuit energised as far as wiring goes: breaker closed, conduit not cut. */
  circuitLive(c: SubsystemId, sw: Record<string, number>, holes: (i: number) => boolean) {
    const s = this.def.subsystems.find((x) => x.id === c);
    if (!s || sw[s.breaker] !== 1) return false;
    for (const p of this.def.panels) if (p.conduits.includes(c) && holes(p.index)) return false;
    return true;
  }

  /** Pressure (kPa) of a compartment, 0 for outside or a ship without atmosphere. */
  pressure(st: Float64Array, zone: string | null) {
    return this.life?.pressure(st, zone) ?? 0;
  }

  breathable(st: Float64Array, zone: string | null) {
    return this.life?.breathable(st, zone) ?? false;
  }

  /** Why moving control `c` to `next` must be refused (the modules' interlocks), or null. */
  interlock(c: ControlDef, next: number, st: Float64Array, sw: Record<string, number>, env: InterlockEnv): string | null {
    for (const m of this.modules) {
      const why = m.interlock?.(c, next, st, sw, env);
      if (why) return why;
    }
    return null;
  }

  // ------------------------------------------------------------------------------------------------
  // Lifecycle
  // ------------------------------------------------------------------------------------------------

  /** Initial state (compartments filled if the definition says so, derived values published). */
  init(st: Float64Array) {
    for (const m of this.modules) m.init?.(st);
  }

  private holesNow = false;

  /**
   * Advance the machinery by dt. `sw` may be changed (tripped breakers, a reactor lever dropping
   * out, consumed pulses, the master caution latching): the changes are returned so the authority
   * can broadcast them, together with events (explosions...).
   */
  tick(dt: number, st: Float64Array, sw: Record<string, number>, ctx: SysContext, holes: (i: number) => boolean, crack: (i: number) => number): { events: SysEvent[]; sw: Record<string, number> } {
    const events: SysEvent[] = [];
    const swOut: Record<string, number> = {};
    const demand: Record<string, number> = {};
    for (const c of this.def.subsystems) demand[c.id] = c.base;
    const sources: PowerSource[] = [];
    const fuel: Record<string, number> = {};
    const holds = new Set<string>();
    const t: Tick = {
      dt,
      st,
      sw,
      ctx,
      hole: holes,
      crack,
      setSw: (k, v) => {
        if (sw[k] === v) return;
        sw[k] = v;
        swOut[k] = v;
      },
      emit: (e) => events.push(e),
      events,
      say: (text) => events.push({ type: 'say', text }),
      load: (c, kw) => {
        if (c !== undefined && c in demand) demand[c] += kw;
      },
      source: (s) => sources.push(s),
      burn: (id, kgs) => (fuel[id] = (fuel[id] ?? 0) + kgs),
      hold: (zone) => holds.add(zone),
      holds,
      demand,
      sources,
      fuel,
    };
    for (const m of this.modules) m.input?.(t);
    for (const m of this.modules) m.loads?.(t);
    for (const m of this.solvers) m.solve!(t);
    for (const m of this.modules) m.step?.(t);

    // alerts & master caution
    this.holesNow = this.def.panels.some((p) => p.kind !== 'bulkhead' && holes(p.index));
    let fresh = false;
    this.alerts.forEach((a, k) => {
      const on = this.alertTest[k](st, sw) ? 1 : 0;
      st[this.iAl.get(a.id)!] = on;
      if (on && !this.prevAl[k]) fresh = true;
      this.prevAl[k] = on;
    });
    if (fresh) t.setSw(this.def.caution, 1);
    return { events, sw: swOut };
  }

  /**
   * Damage to a part (blasts, overheating): emits `destroyed` when it reaches 0 and lets the
   * part's module react (a tank with propellant goes up).
   */
  damagePart(st: Float64Array, p: PartDef, amount: number, events: SysEvent[]) {
    const i = this.iHp.get(p.id)!;
    if (st[i] <= 0) return;
    st[i] = Math.max(0, st[i] - amount);
    if (st[i] > 0) return;
    events.push({ type: 'destroyed', part: p.id });
    for (const m of this.modules) m.destroyed?.(st, p, events);
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
  // Alerts
  // ------------------------------------------------------------------------------------------------

  /** Active alerts (for displays). */
  active(st: Float64Array) {
    return this.alerts.filter((a) => st[this.iAl.get(a.id)!] === 1);
  }

  alertIndex(id: string) {
    return this.iAl.get(id)!;
  }

  /** Annunciator lamp groups the alerts use (every one needs a lamp on the dash). */
  lampGroups() {
    return [...new Set(this.alerts.map((a) => a.lamp))];
  }
}
