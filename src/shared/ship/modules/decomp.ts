// Explosive decompression. When a compartment loses its air fast — a panel blown out, a window
// giving way, a door opened onto vacuum — the violence of the drop (kPa per second) is a shock to
// everything in there: liquids boil, seals and cells burst, loose things are flung into the
// machinery by the breach. Every machine takes a share of its integrity set by its component
// (`decomp`: 0 sealed and rugged … 1 wrecked), more near the opening, a little luck either way.
// A slow leak or a vent valve does nothing of the kind: only a fast drop counts.
//
// Panels carry the pressure difference across them (../airflow.ts): a damaged one holds less and,
// overloaded, tears in a few seconds — it creaks (alarm, message), then goes, and the room it held
// decompresses in turn. Welding it back above its limit, or letting the air out, saves it.
//
// It publishes `<compartment>.shock` (0..1, how violent the decompression is right now) for the
// clients' fog and shake. The pull on the crew, the crates and the ship comes from the same flow
// (../airflow.ts), on whoever moves them.

import type { PartDef } from '../def.js';
import type { V3 } from '../geom.js';
import { strainOf } from '../airflow.js';
import type { ShipSystems } from '../systems.js';
import type { AlertDef, ShipModule, SoundCue, SystemFactory, Tick } from './api.js';
import { CABIN } from './atmos.js';

export const DECOMP = {
  /** Pressure drop rate (kPa/s) where a decompression starts to be violent, and where it is at its worst. */
  rate: [30, 150] as const,
  /** A violent drop of this much (kPa) is announced as an explosive decompression. */
  announce: 12,
  /** Machines near the opening take up to 1 + `near` times the shock, fading over `nearR` metres. */
  near: 1,
  nearR: 1.5,
  /** Share of a panel's integrity an overloaded panel loses per second, per unit of overload (capped at 2). */
  tear: 0.05,
  /** Time the published shock takes to fade (s). */
  fade: 1.5,
};

const smooth = (a: number, b: number, x: number) => {
  const t = Math.max(0, Math.min(1, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
};

export class Decompression implements ShipModule {
  readonly id = 'decomp';
  /** Published per compartment: how violent the decompression is (0..1). */
  readonly iShock: number[];
  private prev: number[];
  /** kPa lost in the violent drop going on (0 = calm), and whether it was announced. */
  private lost: number[];
  private told: boolean[];
  /** Machines in each compartment. */
  private inside: PartDef[][];
  /** Panels tearing under pressure right now (announced once each). */
  private yielding = new Set<number>();
  /** Compartment index on each side of every panel (-1: outside / none). */
  private pz: Int32Array;
  private po: Int32Array;

  constructor(private sys: ShipSystems) {
    const def = sys.def;
    this.iShock = def.compartments.map((c) => sys.vars.define(`${c.id}.shock`, 0.05));
    this.prev = def.compartments.map(() => -1);
    this.lost = def.compartments.map(() => 0);
    this.told = def.compartments.map(() => false);
    this.inside = def.compartments.map((c) => def.parts.filter((p) => p.zone === c.id));
    this.pz = Int32Array.from(def.panels, (p) => sys.compIndex(p.zone));
    this.po = Int32Array.from(def.panels, (p) => (p.other !== undefined ? sys.compIndex(p.other) : -1));
  }

  step(t: Tick) {
    const { dt, st } = t;
    const sys = this.sys;
    const life = sys.life;
    if (!life) return;
    const V = life.atmos.v;
    let places: Array<{ comp: number; at: V3 }> | null = null;
    const comps = sys.def.compartments;
    for (let i = 0; i < comps.length; i++) {
      const c = comps[i];
      const p = st[V[i].p];
      const before = this.prev[i] < 0 ? p : this.prev[i];
      this.prev[i] = p;
      const drop = Math.max(0, before - p);
      const rate = drop / dt;
      const v = smooth(DECOMP.rate[0], DECOMP.rate[1], rate);
      const iS = this.iShock[i];
      st[iS] = Math.max(0, st[iS] - dt / DECOMP.fade);
      if (v <= 0) {
        if (rate < DECOMP.rate[0] / 3) {
          this.lost[i] = 0;
          this.told[i] = false;
        }
        continue;
      }
      this.lost[i] += drop;
      if (this.lost[i] >= DECOMP.announce) {
        st[iS] = Math.max(st[iS], v);
        if (!this.told[i]) {
          this.told[i] = true;
          t.say(`¡DESCOMPRESIÓN EXPLOSIVA · ${c.label}!`);
        }
      }
      // the shock this tick: the share of a full cabin lost, weighted by how violently
      const dose = (drop / CABIN.p) * v;
      // where the air goes: the machines by the opening get the worst of it
      places ??= life
        .paths(st, t.sw, t)
        .filter((g) => g.at)
        .flatMap((g) => [g.a, g.b].filter((k) => k >= 0).map((k) => ({ comp: k, at: g.at! })));
      for (const part of this.inside[i]) {
        let d = Infinity;
        for (const g of places) if (g.comp === i) d = Math.min(d, sys.partDistance(part, g.at));
        const near = Number.isFinite(d) ? 1 + DECOMP.near * Math.exp(-d / DECOMP.nearR) : 1;
        const luck = 0.7 + 0.6 * t.ctx.rand();
        const dmg = part.maxHp * part.decomp * dose * near * luck;
        if (dmg > 0) sys.damagePart(st, part, dmg, t.events);
      }
    }
    // panels under a pressure they can no longer hold tear, faster the more they are overloaded
    const panels = sys.def.panels;
    for (let j = 0; j < panels.length; j++) {
      const pn = panels[j];
      if (t.hole(pn.index)) {
        if (this.yielding.size) this.yielding.delete(pn.index);
        continue;
      }
      const a = this.pz[j];
      const b = this.po[j];
      const load = Math.abs((a < 0 ? 0 : st[V[a].p]) - (b < 0 ? 0 : st[V[b].p]));
      const strain = load < 1 ? 0 : strainOf(pn, load, t.panelHp(pn.index));
      if (strain <= 0) {
        if (this.yielding.size) this.yielding.delete(pn.index);
        continue;
      }
      t.damagePanel(pn.index, pn.maxHp * DECOMP.tear * Math.min(2, strain) * dt);
      if (!this.yielding.has(pn.index) && !t.hole(pn.index)) {
        this.yielding.add(pn.index);
        t.say(`Panel ${pn.id} cediendo por la presión: suéldalo o saca el aire`);
      }
    }
  }

  /** The room that blows: the slug of air leaving all at once, the structure ringing. */
  sounds(): SoundCue[] {
    return this.sys.def.compartments.map((c, i): SoundCue => ({ sound: 'decomp.blast', zone: c.id, on: (st) => st[this.iShock[i]] > 0.3 }));
  }

  alerts(): AlertDef[] {
    return [
      {
        id: 'strain',
        label: 'PANEL CEDIENDO POR LA PRESIÓN',
        level: 2,
        lamp: 'CASCO',
        help: 'Un panel dañado no aguanta la diferencia de presión que soporta y se está rajando: en unos segundos revienta y el compartimento se descomprime de golpe. Suéldalo por encima de la mitad de su integridad, o saca el aire del compartimento (compresor, venteo) antes de que ceda; y aléjate de él.',
        on: () => this.yielding.size > 0,
      },
    ];
  }
}

export const decompSystem: SystemFactory = {
  id: 'decomp',
  make: (sys) => (sys.def.compartments.length ? [new Decompression(sys)] : []),
};
