// Airlock cycle: one selector takes the crew out or in, doing the door and air sequence the crew
// would otherwise do by hand. Out: shut the inner door and the ventilation damper → pump the air
// back into the bottles (recovery compressor, pressure control held off) → open the outer door.
// In: shut the outer door → repressurise (the automatic pressure control, or the manual repress
// valve) → open the inner door and the damper once both sides match. Doors keep their own interlocks for anyone working them by
// hand; the sequencer only opens a door when the pressure across it is already equal.

import { DOOR_DP } from './life.js';
import type { AirlockDef, ControlDef } from '../def.js';
import type { ShipSystems } from '../systems.js';
import type { AlertDef, ShipModule, SoundCue, SystemFactory, Tick } from './api.js';

/** Sequencer phases (replicated as `lock.phase`). */
export const LOCK = { in: 0, closeIn: 1, pump: 2, openOut: 3, out: 4, closeOut: 5, fill: 6, openIn: 7 } as const;
export const LOCK_NAME = ['DENTRO', 'CERRANDO INTERIOR', 'VACIANDO', 'ABRIENDO EXTERIOR', 'FUERA', 'CERRANDO EXTERIOR', 'LLENANDO', 'ABRIENDO INTERIOR'];

/** Pressure (kPa) below which the outer door may open. */
const EMPTY = 3;

export class Airlock implements ShipModule {
  readonly id = 'airlock';
  readonly iPhase: number;
  private iWait: number;
  /** Compartment on the cabin side of the inner door. */
  private cabin: string | null;
  private repress: string;
  private recover: string;
  private said = -1;

  constructor(
    private sys: ShipSystems,
    readonly a: AirlockDef,
  ) {
    const def = sys.def;
    this.iPhase = sys.vars.define('lock.phase', 1, (def.defaults[a.key] ?? 0) === 1 ? LOCK.out : LOCK.in);
    this.iWait = sys.vars.define('lock.wait', -1);
    const inner = def.openings.find((o) => o.key === a.inner)!;
    this.cabin = inner.a === a.zone ? inner.b : inner.a;
    this.repress = `${def.life?.repress ?? 'repress.'}${a.zone}`;
    this.recover = def.life!.recover!.key;
  }

  private powered(st: Float64Array) {
    return this.sys.supply(st, this.a.circuit) >= 0.5;
  }

  /** Phases where the lock is (or is going) empty: the pressure control must not refill it. */
  private holding(phase: number) {
    return phase >= LOCK.closeIn && phase <= LOCK.closeOut;
  }

  /** Enter a phase (returns it, for the caller's local copy). */
  private go(st: Float64Array, next: number) {
    st[this.iPhase] = next;
    st[this.iWait] = 0;
    return next;
  }

  input(t: Tick) {
    const { st, sw, dt } = t;
    const a = this.a;
    const sys = this.sys;
    let ph = st[this.iPhase];
    const want = sw[a.key] === 1 ? 1 : 0;
    // a new order turns the sequence around wherever it is
    if (want === 1 && (ph === LOCK.in || ph >= LOCK.closeOut)) ph = this.go(st, LOCK.closeIn);
    if (want === 0 && ph >= LOCK.closeIn && ph <= LOCK.out) ph = this.go(st, LOCK.closeOut);
    if (ph === LOCK.in || ph === LOCK.out) {
      if (ph === LOCK.out) t.hold(a.zone);
      return;
    }
    if (this.holding(ph)) t.hold(a.zone);
    st[this.iWait] += dt;
    if (!this.powered(st)) {
      if (st[this.iWait] > 3 && this.said !== ph) {
        this.said = ph;
        t.say(`Esclusa detenida: sin energía en ${sys.def.subsystems.find((c) => c.id === a.circuit)?.label ?? a.circuit}`);
      }
      return;
    }
    const p = sys.pressure(st, a.zone);
    const cabin = sys.pressure(st, this.cabin);
    switch (ph) {
      case LOCK.closeIn:
        t.setSw(a.inner, 0);
        t.setSw(a.outer, 0);
        if (a.duct) t.setSw(a.duct, 0);
        if (sys.mover(st, a.inner) < 0.02 && sys.mover(st, a.outer) < 0.02) ph = this.go(st, LOCK.pump);
        break;
      case LOCK.pump:
        t.setSw(a.inner, 0);
        t.setSw(this.recover, 1);
        if (p < EMPTY) {
          t.setSw(this.recover, 0);
          ph = this.go(st, LOCK.openOut);
        } else if (st[this.iWait] > 60 && this.said !== ph) {
          this.said = ph;
          t.say('Esclusa: el compresor no consigue vaciarla (¿puerta interior abierta, compresor sin energía?)');
        }
        break;
      case LOCK.openOut:
        t.setSw(a.outer, 1);
        if (sys.mover(st, a.outer) > 0.98) ph = this.go(st, LOCK.out);
        break;
      case LOCK.closeOut:
        t.setSw(this.recover, 0);
        t.setSw(a.outer, 0);
        if (sys.mover(st, a.outer) < 0.02) ph = this.go(st, LOCK.fill);
        break;
      case LOCK.fill: {
        // AUTO refills a sealed compartment by itself; otherwise open the manual valve meanwhile
        const auto = (sw[sys.def.life?.mode ?? ''] ?? 0) === 0;
        if (!auto) t.setSw(this.repress, 1);
        if (Math.abs(p - cabin) < DOOR_DP - 1) {
          if (!auto) t.setSw(this.repress, 0);
          ph = this.go(st, LOCK.openIn);
        } else if (st[this.iWait] > 60 && this.said !== ph) {
          this.said = ph;
          t.say('Esclusa: no llega a la presión de la cabina (¿botellas vacías o cerradas?)');
        }
        break;
      }
      case LOCK.openIn:
        t.setSw(a.inner, 1);
        if (a.duct) t.setSw(a.duct, 1);
        if (sys.mover(st, a.inner) > 0.98) ph = this.go(st, LOCK.in);
        break;
    }
    if (ph !== this.said) this.said = -1;
  }

  loads(t: Tick) {
    const ph = t.st[this.iPhase];
    if (ph !== LOCK.in && ph !== LOCK.out) t.load(this.a.circuit, 0.2);
  }

  interlock(c: ControlDef, _next: number, st: Float64Array) {
    if (c.key === this.a.key && !this.powered(st)) return `Esclusa sin energía · circuito ${this.sys.def.subsystems.find((x) => x.id === this.a.circuit)?.label ?? ''}`;
    return null;
  }

  /** A beep when a cycle starts, a chime when the lock is ready at either end. */
  sounds(): SoundCue[] {
    const i = this.iPhase;
    const zone = this.a.zone;
    const still = (st: Float64Array) => st[i] === LOCK.in || st[i] === LOCK.out;
    return [
      { sound: 'chime.cycle', zone, on: (st) => !still(st) },
      { sound: 'chime.ok', zone, on: still },
    ];
  }

  alerts(): AlertDef[] {
    return [
      {
        id: 'lock.stuck',
        label: 'ESCLUSA DETENIDA',
        level: 1,
        lamp: 'ESCLUSA',
        help: 'El ciclo de la esclusa lleva más de un minuto sin avanzar: falta energía, el compresor no vacía (una puerta abierta, botellas llenas) o no hay gas para llenarla. Mira la página ESCL y acaba el ciclo a mano con las puertas y las válvulas.',
        on: (st) => st[this.iPhase] !== LOCK.in && st[this.iPhase] !== LOCK.out && st[this.iWait] > 60,
      },
    ];
  }
}

export const airlockSystem: SystemFactory = {
  id: 'airlock',
  make: (sys) => (sys.def.airlock && sys.def.life?.recover ? [new Airlock(sys, sys.def.airlock)] : []),
};
