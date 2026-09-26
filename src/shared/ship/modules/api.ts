// The contract between the systems kernel (../systems.ts) and every system module.
//
// A ship's machinery is a list of modules. Each tick runs in phases, every module taking part in
// the phases it needs:
//
//   input  — react to the crew's switches: consume pulses, start/stop state machines
//   loads  — declare what it asks of the shared networks this tick: electrical loads per circuit,
//            power it offers as a source, propellant it wants to burn
//   solve  — networks only (power grid, propellant, atmosphere), in `order`: share out what was
//            declared (supply fractions, feed fractions, gas flows)
//   step   — react to what the networks gave: temperatures, spool-up, damage, failures
//
// then the kernel evaluates every module's alerts and latches the master caution. Modules also
// veto controls (`interlock`) and may publish a service other modules use (`sys.provide`).
// Adding a mechanic = a module file + one line in modules/index.ts. See docs/SHIPS.md.

import type { ControlDef, PartDef, SubsystemId } from '../def.js';
import type { V3 } from '../geom.js';
import type { ShipSystems } from '../systems.js';

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
  /** Sunlight on the hull, 0 (night / shadow) … 1 (full sun): solar arrays scale with it. */
  sun: number;
  /** Random numbers (deterministic in tests). */
  rand: () => number;
}

export type SysEvent =
  | { type: 'explode'; at: V3; radius: number; damage: number; cause: string }
  | { type: 'trip'; circuit: SubsystemId }
  | { type: 'destroyed'; part: string }
  | { type: 'say'; text: string };

/** A power source offered to the grid this tick (dispatched in ascending `order`). */
export interface PowerSource {
  id: string;
  kw: number;
  /** 0..1: power quality when it carries the load (a damaged reactor makes dirty power). */
  quality: number;
  order: number;
}

/** One tick, shared by every module. */
export interface Tick {
  readonly dt: number;
  readonly st: Float64Array;
  /** Switch positions (read; write through `setSw` so the change is broadcast). */
  readonly sw: Record<string, number>;
  readonly ctx: SysContext;
  /** Panel `i` is blown out / its crack area (m²). */
  hole(i: number): boolean;
  crack(i: number): number;
  /** Move a switch from the simulation (lever dropping out, pulse consumed…): broadcast to all. */
  setSw(key: string, v: number): void;
  emit(e: SysEvent): void;
  say(text: string): void;
  /** Events emitted so far this tick (append with `emit`). */
  readonly events: SysEvent[];
  /** Electrical load on a circuit (kW). `undefined` circuit = not wired, ignored. */
  load(circuit: SubsystemId | undefined, kw: number): void;
  /** Offer generation to the grid. */
  source(s: PowerSource): void;
  /** Propellant wanted by a consumer part this tick (kg/s). */
  burn(consumer: string, kgs: number): void;
  /** Keep the pressure control off a compartment this tick (an airlock pumping down). */
  hold(zone: string): void;
  readonly holds: ReadonlySet<string>;
  /** Declared this tick (read by the networks in `solve`). */
  readonly demand: Record<string, number>;
  readonly sources: PowerSource[];
  readonly fuel: Record<string, number>;
}

/** A condition shown on the MFDs and the annunciator. */
export interface AlertDef {
  id: string;
  label: string;
  /** 1 caution (amber), 2 warning (red). */
  level: 1 | 2;
  /** Annunciator lamp it lights. */
  lamp: string;
  /** What it means and what to do (manual). */
  help?: string;
  on(st: Float64Array, sw: Record<string, number>): boolean;
}

/** What an interlock knows beyond the state tables. */
export interface InterlockEnv {
  landed: boolean;
  onPad: boolean;
}

export interface ShipModule {
  readonly id: string;
  /** Networks: position in the `solve` phase (lower first). */
  readonly order?: number;
  /** Initial state (after the variable table is built). */
  init?(st: Float64Array): void;
  input?(t: Tick): void;
  loads?(t: Tick): void;
  solve?(t: Tick): void;
  step?(t: Tick): void;
  /**
   * Why operating `c` to position `next` must be refused right now (interlock), or null.
   * Called for every control; answer only for the keys the module owns.
   */
  interlock?(c: ControlDef, next: number, st: Float64Array, sw: Record<string, number>, env: InterlockEnv): string | null;
  alerts?(): AlertDef[];
  /** A part just reached 0 integrity (blast, fire): react to it (a full tank goes up…). */
  destroyed?(st: Float64Array, p: PartDef, events: SysEvent[]): void;
}

/**
 * A kind of system. `make` looks at the ship definition and returns the modules it needs (none if
 * the ship has nothing for it). `parts` = machine types it drives: a part whose type no system
 * claims is an error in the ship data.
 */
export interface SystemFactory {
  id: string;
  parts?: string[];
  make(sys: ShipSystems): ShipModule[];
}

/** Integrity fraction 0..1. */
export const hpf = (hp: number, max: number) => Math.max(0, Math.min(1, hp / max));

/** Parts of one type. */
export const partsOf = (sys: ShipSystems, type: string): PartDef[] => sys.def.parts.filter((p) => p.type === type);

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
