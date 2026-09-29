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
import { hpf, panelArea, type AlertDef, type HullView, type InterlockEnv, type PowerSource, type ShipModule, type SoundCue, type SysContext, type SysEvent, type Tick } from './modules/api.js';
import { SYSTEM_FACTORIES } from './modules/index.js';
import type { LifeSupport } from './modules/life.js';
import type { PowerGrid } from './modules/power.js';
import type { PropellantNet } from './modules/propellant.js';
import type { VarTable } from './state.js';

/** All alerts clear this long (s) → the master caution resets itself. */
const CAUTION_SELF_CLEAR_S = 3;

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

/** No switch moved this tick (shared, never written). */
const NO_SW: Record<string, number> = Object.freeze({}) as Record<string, number>;

/**
 * The tick every module sees. Each ship keeps one and refills it tick after tick: the containers
 * are emptied, never recreated, and the callbacks are bound once (a tick leaves no garbage).
 */
class TickState implements Tick {
  dt = 0;
  st: Float64Array = new Float64Array(0);
  sw: Record<string, number> = NO_SW;
  ctx!: SysContext;
  hull!: HullView;
  readonly events: SysEvent[] = [];
  /** Switches moved this tick (allocated only when one moves). */
  swOut: Record<string, number> | null = null;
  readonly held: Uint8Array;
  readonly demand: Float64Array;
  readonly sources: PowerSource[] = [];
  readonly fuel: Float64Array;

  constructor(private sys: ShipSystems) {
    this.held = new Uint8Array(sys.def.compartments.length);
    this.demand = new Float64Array(sys.def.subsystems.length);
    this.fuel = new Float64Array(sys.def.parts.length);
  }

  /** Empty everything for a new tick. */
  begin(dt: number, st: Float64Array, sw: Record<string, number>, ctx: SysContext, hull: HullView) {
    this.dt = dt;
    this.st = st;
    this.sw = sw;
    this.ctx = ctx;
    this.hull = hull;
    this.events.length = 0;
    this.swOut = null;
    this.held.fill(0);
    const subs = this.sys.def.subsystems;
    for (let k = 0; k < subs.length; k++) this.demand[k] = subs[k].base;
    this.sources.length = 0;
    this.fuel.fill(0);
  }

  hole = (i: number) => this.hull.hole(i);
  crack = (i: number) => this.hull.crack(i);
  panelHp = (i: number) => this.hull.hp(i);
  damagePanel = (i: number, amount: number) => this.hull.damage(i, amount);
  setSw = (k: string, v: number) => {
    if (this.sw[k] === v) return;
    this.sw[k] = v;
    (this.swOut ??= {})[k] = v;
  };
  emit = (e: SysEvent) => {
    this.events.push(e);
  };
  say = (text: string) => {
    this.events.push({ type: 'say', text });
  };
  load = (c: SubsystemId | undefined, kw: number) => {
    if (c === undefined) return;
    const k = this.sys.circuitIndex(c);
    if (k >= 0) this.demand[k] += kw;
  };
  source = (s: PowerSource) => {
    this.sources.push(s);
  };
  burn = (id: string, kgs: number) => {
    const k = this.sys.partIndex(id);
    if (k >= 0) this.fuel[k] += kgs;
  };
  hold = (zone: string) => {
    const k = this.sys.compIndex(zone);
    if (k >= 0) this.held[k] = 1;
  };
}

export class ShipSystems {
  readonly modules: ShipModule[] = [];
  readonly alerts: Alert[] = [];
  private alertTest: AlertDef['on'][] = [];
  /** State index of each alert's lamp variable (same order as `alerts`). */
  private alIdx: number[] = [];
  private iHp = new Map<string, number>();
  /** Integrity variable of each part, by part index. */
  private hpAt: Int32Array;
  /** Index of each circuit in `def.subsystems`, of each part in `def.parts`. */
  private iCirc = new Map<string, number>();
  private iPart = new Map<string, number>();
  /** Per circuit: its breaker and the panels its conduit runs through. */
  private wiring = new Map<string, { breaker: string; panels: Int32Array }>();
  /** Hull panels (no compartment on the other side): a hole in one is a breach. */
  private hullPanels: Int32Array;
  private tk: TickState;
  private result: { events: SysEvent[]; sw: Record<string, number> } = { events: [], sw: NO_SW };
  /** Services looked up once (`provide` resets it). */
  private svc: { power?: PowerGrid | null; fuel?: PropellantNet | null; life?: LifeSupport | null } = {};
  private iMv = new Map<string, number>();
  private iAl = new Map<string, number>();
  private comp = new Map<string, number>();
  private services = new Map<string, unknown>();
  private solvers: ShipModule[];
  /** Time every alert has been clear (s): the master caution goes out on its own. */
  private calm = 0;
  /** Previous alert states (latching the master caution on new ones). */
  private prevAl: Float64Array;

  constructor(
    readonly def: ShipDef,
    readonly vars: VarTable,
  ) {
    for (const p of def.parts) this.iHp.set(p.id, vars.define(`${p.id}.hp`, 0.1, p.maxHp));
    this.hpAt = Int32Array.from(def.parts, (p) => this.iHp.get(p.id)!);
    def.parts.forEach((p, i) => {
      if (!this.iPart.has(p.id)) this.iPart.set(p.id, i);
    });
    def.subsystems.forEach((c, i) => {
      if (this.iCirc.has(c.id)) return;
      this.iCirc.set(c.id, i);
      this.wiring.set(c.id, { breaker: c.breaker, panels: Int32Array.from(def.panels.filter((p) => p.conduits.includes(c.id)), (p) => p.index) });
    });
    this.hullPanels = Int32Array.from(def.panels.filter((p) => p.other === undefined), (p) => p.index);
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
    this.tk = new TickState(this);
  }

  /** Worst alert on now (replicated lamps): 0 none, 1 caution, 2 warning. */
  worstAlert(st: Float64Array): number {
    let w = 0;
    for (let k = 0; k < this.alIdx.length; k++) if (st[this.alIdx[k]] === 1 && this.alerts[k].level > w) w = this.alerts[k].level;
    return w;
  }

  /**
   * Everything the ship sounds like (client/audio): every module's cues, then the master alarm —
   * a speaker in the ceiling of every room sounding the worst alert while the caution is latched
   * (acknowledging it silences them). Built on demand.
   */
  soundCues(): SoundCue[] {
    const all: SoundCue[] = [];
    for (const m of this.modules) {
      const cues = m.sounds?.();
      if (cues) for (const c of cues) all.push(c);
    }
    // a machine's own module speaks for it: the generic cue of the same part and role goes
    const own = new Set(all.filter((c) => c.part && !c.generic).map((c) => `${c.part!.id}|${c.role ?? ''}`));
    const out = all.filter((c) => !c.generic || !c.part || !own.has(`${c.part.id}|${c.role ?? ''}`));
    const key = this.def.caution;
    const worst = this.alIdx.length ? (st: Float64Array, sw: Record<string, number>) => (sw[key] === 1 ? this.worstAlert(st) : 0) : null;
    if (worst) {
      for (const z of this.def.zones) {
        const at: V3 = z.lights[0] ?? [(z.min[0] + z.max[0]) / 2, z.max[1] - 0.1, (z.min[2] + z.max[2]) / 2];
        out.push({ sound: 'alarm.warning', at, zone: z.id, level: (st, sw) => (worst(st, sw) === 2 ? 1 : 0) });
        out.push({ sound: 'alarm.caution', at, zone: z.id, level: (st, sw) => (worst(st, sw) === 1 ? 1 : 0) });
      }
    }
    return out;
  }

  private addAlert(a: AlertDef) {
    if (this.iAl.has(a.id)) throw new Error(`alert ${a.id} defined twice`);
    this.alerts.push({ id: a.id, label: a.label, level: a.level, lamp: a.lamp, help: a.help });
    this.alertTest.push(a.on);
    const i = this.vars.define(`al.${a.id}`, 1);
    this.iAl.set(a.id, i);
    this.alIdx.push(i);
  }

  // ------------------------------------------------------------------------------------------------
  // Services (networks other modules use) and module lookup
  // ------------------------------------------------------------------------------------------------

  provide(id: string, service: unknown) {
    this.services.set(id, service);
    this.svc = {};
  }

  use<T>(id: string): T | undefined {
    return this.services.get(id) as T | undefined;
  }

  get power(): PowerGrid | undefined {
    if (this.svc.power === undefined) this.svc.power = this.use<PowerGrid>('power') ?? null;
    return this.svc.power ?? undefined;
  }

  get fuel(): PropellantNet | undefined {
    if (this.svc.fuel === undefined) this.svc.fuel = this.use<PropellantNet>('propellant') ?? null;
    return this.svc.fuel ?? undefined;
  }

  get life(): LifeSupport | undefined {
    if (this.svc.life === undefined) this.svc.life = this.use<LifeSupport>('life') ?? null;
    return this.svc.life ?? undefined;
  }

  /** Index of a circuit in `def.subsystems` (-1: none). */
  circuitIndex(c: SubsystemId) {
    return this.iCirc.get(c) ?? -1;
  }

  /** Index of a part in `def.parts` (-1: none). */
  partIndex(id: string) {
    return this.iPart.get(id) ?? -1;
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

  part(id: string): PartDef | undefined {
    const i = this.iPart.get(id);
    return i === undefined ? undefined : this.def.parts[i];
  }

  tag(p: PartDef) {
    return partTag(p);
  }

  hp(st: Float64Array, id: string) {
    return st[this.iHp.get(id)!];
  }

  health(st: Float64Array, p: PartDef) {
    return hpf(st[this.hpAt[p.index]], p.maxHp);
  }

  /** State index of a part's integrity. */
  hpOf(p: PartDef) {
    return this.hpAt[p.index];
  }

  /** Circuit supply fraction (0 = dead, 1 = fully fed). Unwired (undefined) = always fed. */
  supply(st: Float64Array, c: SubsystemId | undefined) {
    if (c === undefined) return 1;
    return this.power ? st[this.power.fIndex(c)] : 0;
  }

  /** Circuit energised as far as wiring goes: breaker closed, conduit not cut. */
  circuitLive(c: SubsystemId, sw: Record<string, number>, holes: (i: number) => boolean) {
    const w = this.wiring.get(c);
    if (!w || sw[w.breaker] !== 1) return false;
    const panels = w.panels;
    for (let k = 0; k < panels.length; k++) if (holes(panels[k])) return false;
    return true;
  }

  /** First blown-out panel on a circuit's conduit (panel index), or -1. */
  conduitCut(c: SubsystemId, holes: (i: number) => boolean) {
    const w = this.wiring.get(c);
    if (!w) return -1;
    const panels = w.panels;
    for (let k = 0; k < panels.length; k++) if (holes(panels[k])) return panels[k];
    return -1;
  }

  /** Breaker switch of a circuit. */
  breakerOf(c: SubsystemId) {
    return this.wiring.get(c)?.breaker;
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
  tick(dt: number, st: Float64Array, sw: Record<string, number>, ctx: SysContext, hull: HullView): { events: SysEvent[]; sw: Record<string, number> } {
    const t = this.tk;
    t.begin(dt, st, sw, ctx, hull);
    const mods = this.modules;
    for (let k = 0; k < mods.length; k++) mods[k].input?.(t);
    for (let k = 0; k < mods.length; k++) mods[k].loads?.(t);
    for (let k = 0; k < this.solvers.length; k++) this.solvers[k].solve!(t);
    for (let k = 0; k < mods.length; k++) mods[k].step?.(t);

    // alerts & master caution
    let holes = false;
    for (let k = 0; k < this.hullPanels.length; k++) {
      if (hull.hole(this.hullPanels[k])) {
        holes = true;
        break;
      }
    }
    this.holesNow = holes;
    let fresh = false;
    let any = false;
    const tests = this.alertTest;
    for (let k = 0; k < tests.length; k++) {
      const on = tests[k](st, sw) ? 1 : 0;
      st[this.alIdx[k]] = on;
      if (on && !this.prevAl[k]) fresh = true;
      if (on) any = true;
      this.prevAl[k] = on;
    }
    if (fresh) t.setSw(this.def.caution, 1);
    // self-check: the master caution goes out by itself once every alert has been clear for a while
    this.calm = any || fresh ? 0 : this.calm + dt;
    if (this.calm >= CAUTION_SELF_CLEAR_S && sw[this.def.caution] === 1) t.setSw(this.def.caution, 0);
    // reused tick after tick: the caller reads it right away
    this.result.events = t.events;
    this.result.sw = t.swOut ?? NO_SW;
    return this.result;
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
    return this.alerts.filter((_, k) => st[this.alIdx[k]] === 1);
  }

  alertIndex(id: string) {
    return this.iAl.get(id)!;
  }

  /** Annunciator lamp groups the alerts use (every one needs a lamp on the dash). */
  lampGroups() {
    return [...new Set(this.alerts.map((a) => a.lamp))];
  }
}
