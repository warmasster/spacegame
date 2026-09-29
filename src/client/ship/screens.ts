import * as THREE from 'three';
import { activePage, pageLabel, type ScreenDef, type ScreenPage } from '../../shared/ship/def';
import type { Apu } from '../../shared/ship/modules/apu';
import { BOOST, ENG, type Engine } from '../../shared/ship/modules/engines';
import { AP_FACE_KEY, AP_FACES } from '../../shared/ship/flight/autopilot';
import { bodyAt, circularAt, type Surfaces } from '../../shared/space/body';
import { REACTOR_SET, RX, type Reactor } from '../../shared/ship/modules/reactor';
import { LOCK, LOCK_NAME } from '../../shared/ship/modules/airlock';
import type { SolarArray } from '../../shared/ship/modules/solar';
import { STORES, type Stores } from '../../shared/ship/modules/stores';
import { AP_KEY, apModes, bearingTo, flightReadout, mainUse, NAV_POINTS, performance, type FlightReadout } from '../../shared/ship/flight';
import { MOON } from '../../shared/constants';
import type { ShipSim } from '../../shared/ship/sim';
import { sharpText } from './materials';

/** Animated state the displays report: travel 0..1 of every mover (doors, ramp, shutters…) by key. */
export interface ShipAnimState {
  movers: Record<string, number>;
}

const FONT = 'ui-monospace, Menlo, Consolas, monospace';
const C = {
  bg: '#030a10',
  grid: 'rgba(80,180,220,0.10)',
  dim: '#4f7d8f',
  txt: '#bfefff',
  ok: '#5cf29a',
  warn: '#ffc04a',
  bad: '#ff5a4a',
  cyan: '#4fd8f0',
};

const RX_NAME = ['PARADO', 'ARRANCANDO', 'EN MARCHA', 'SCRAM'];
const ENG_NAME = ['PARADO', 'ARRANCANDO', 'EN MARCHA', 'FALLO'];

type Rows = { row(label: string, value: string, color: string): void; readonly y: number };

/** What a page drawing gets. */
interface PageCtx {
  g: CanvasRenderingContext2D;
  w: number;
  /** Body height (above the page tabs). */
  h: number;
  time: number;
  anim: ShipAnimState;
  sim: ShipSim;
  /** Flight state as the displays read it (height above the terrain, speeds, autopilot). */
  fl(): FlightReadout;
  /** Named state value. */
  n(name: string): number;
  header(title: string): void;
  rows(y0: number): Rows;
  /** A switch as the crew reads it: its control's name and the text of its position. */
  switchRow(r: Rows, key: string): void;
  note(text: string, y: number): void;
}

/**
 * Page drawings by page id. A new page = an entry in SCREEN_PAGES (shared/ship/def.ts, label and
 * help) + a function here; pages without one show a placeholder.
 */
const PAGES: Record<ScreenPage, (p: PageCtx) => void> = {
  status(p) {
    const { g, w, sim, anim } = p;
    p.header('SISTEMAS');
    const r = p.rows(46);
    for (const rx of reactors(sim)) {
      const s = Math.round(p.n(`${rx.tag}.state`));
      r.row(rx.part.name.toUpperCase().slice(0, 14), `${RX_NAME[s] ?? '?'} · ${Math.round(p.n(`${rx.tag}.temp`))} °C`, s === RX.online ? C.ok : s === RX.scram ? C.bad : C.warn);
    }
    if (sim.sys.power) {
      r.row('RED', `${p.n('pwr.gen').toFixed(1)} / ${p.n('pwr.load').toFixed(1)} kW`, p.n('pwr.shed') ? C.warn : C.ok);
      if (sim.sys.power.batteries.length) r.row('BATERÍA', `${Math.round(p.n('bat.soc') * 100)} % · ${p.n('bat.kw').toFixed(1)} kW`, p.n('bat.soc') < 0.2 ? C.bad : C.txt);
    }
    for (const a of apus(sim)) {
      const s = Math.round(p.n(`${a.tag}.state`));
      r.row(a.part.name.toUpperCase(), ENG_NAME[s] ?? '?', s === ENG.run ? C.ok : C.dim);
    }
    let y = r.y + 2;
    g.strokeStyle = 'rgba(79,216,240,0.25)';
    line(g, 12, y, w - 12, y);
    y += 8;
    if (sim.def.compartments.length) {
      const open = sim.def.compartments.filter((c) => p.n(`${c.id}.p`) < 2).map((c) => c.label);
      g.fillStyle = open.length ? C.warn : C.ok;
      g.font = `600 13px ${FONT}`;
      g.fillText(open.length ? `SIN AIRE: ${open.join(', ')}` : 'TODOS LOS COMPARTIMENTOS CON AIRE', 16, y);
      y += 20;
    }
    const ramp = sim.def.ramp ? anim.movers[sim.def.ramp.key] ?? 0 : null;
    if (ramp !== null) {
      g.fillStyle = C.dim;
      g.fillText('RAMPA', 16, y);
      bar(g, 80, y + 2, 80, 11, ramp, ramp > 0.99 || ramp < 0.01 ? C.cyan : C.warn);
      g.fillStyle = C.txt;
      g.fillText(ramp > 0.99 ? 'ABAJO' : ramp < 0.01 ? 'CERRADA' : `${Math.round(ramp * 100)} %`, 170, y);
    }
    if (sim.def.shield) {
      g.fillStyle = C.dim;
      g.fillText('ESCUDO', 280, y);
      bar(g, 360, y + 2, 70, 11, anim.movers[sim.def.shield.key] ?? 0, C.cyan);
    }
  },

  hull(p) {
    const { g, w, h, sim, time } = p;
    p.header('INTEGRIDAD DEL CASCO');
    const b = sim.def.bounds;
    const cy = (b.min[1] + b.max[1]) / 2;
    const z0 = b.min[2];
    const z1 = b.max[2];
    const top = 44;
    const bottom = h - 22;
    const X = (z: number) => 16 + ((z - z0) / (z1 - z0)) * (w - 32);
    const Y = (a: number) => top + ((a + Math.PI) / (2 * Math.PI)) * (bottom - top);
    const blink = Math.floor(time * 3) % 2 === 0;
    let holes = 0;
    for (const pn of sim.def.panels) {
      const pts = pn.poly.map((q) => [pn.c[0] + pn.u[0] * q[0] + pn.v[0] * q[1], pn.c[1] + pn.u[1] * q[0] + pn.v[1] * q[1], pn.c[2] + pn.u[2] * q[0] + pn.v[2] * q[1]]);
      const zs = pts.map((q) => q[2]);
      const angs = pts.map((q) => Math.atan2(q[0], -(q[1] - cy)));
      if (Math.max(...angs) - Math.min(...angs) > Math.PI) for (let i = 0; i < angs.length; i++) if (angs[i] < 0) angs[i] += Math.PI * 2;
      let xa = X(Math.min(...zs));
      let xb = X(Math.max(...zs));
      if (xb - xa < 5) {
        const m = (xa + xb) / 2;
        xa = m - 3;
        xb = m + 3;
      }
      const ya = Y(Math.min(...angs));
      const yb = Y(Math.min(Math.PI, Math.max(...angs)));
      const ratio = sim.hp[pn.index] / pn.maxHp;
      const hole = sim.hole(pn.index);
      if (hole) holes++;
      g.fillStyle = hole ? (sim.hp[pn.index] > 0 ? 'rgba(79,216,240,0.35)' : '#000') : ratio > 0.8 ? 'rgba(92,242,154,0.55)' : ratio > 0.5 ? 'rgba(255,192,74,0.7)' : 'rgba(255,90,74,0.8)';
      if (pn.kind === 'glass' && !hole && ratio > 0.8) g.fillStyle = 'rgba(79,170,240,0.55)';
      g.fillRect(xa + 1, ya + 1, xb - xa - 2, yb - ya - 2);
      if (hole && (blink || sim.hp[pn.index] > 0)) {
        g.strokeStyle = sim.hp[pn.index] > 0 ? C.cyan : C.bad;
        g.lineWidth = 2;
        g.strokeRect(xa + 1, ya + 1, xb - xa - 2, yb - ya - 2);
      }
    }
    g.fillStyle = holes ? C.bad : C.ok;
    g.font = `700 13px ${FONT}`;
    g.textAlign = 'center';
    g.fillText(holes ? `${holes} BRECHA${holes > 1 ? 'S' : ''}` : 'CASCO ESTANCO', w / 2, h - 16);
    g.textAlign = 'left';
  },

  power(p) {
    const { g, h, sim } = p;
    p.header('DISTRIBUCIÓN');
    const rx = reactors(sim)[0];
    const s = rx ? Math.round(p.n(`${rx.tag}.state`)) : -1;
    const on = p.n('pwr.gen') > 0.05;
    g.fillStyle = on ? 'rgba(92,242,154,0.18)' : 'rgba(255,90,74,0.18)';
    g.fillRect(16, 48, 150, 52);
    g.strokeStyle = on ? C.ok : C.bad;
    g.lineWidth = 2;
    g.strokeRect(16, 48, 150, 52);
    g.fillStyle = on ? C.ok : C.bad;
    g.font = `700 14px ${FONT}`;
    g.fillText(rx ? RX_NAME[s] ?? 'REACTOR' : on ? 'GENERANDO' : 'SIN GENERACIÓN', 26, 56);
    g.font = `600 12px ${FONT}`;
    g.fillStyle = C.txt;
    g.fillText(`${rx ? `${p.n(`${rx.tag}.kw`).toFixed(1)} kW · ` : ''}red ${p.n('pwr.gen').toFixed(1)}`, 26, 76);
    const n = sim.def.subsystems.length;
    const y0 = 108;
    const dy = Math.min(22, (h - y0 - 8) / Math.max(1, n));
    sim.def.subsystems.forEach((c, i) => {
      const y = y0 + dy * i;
      const brk = sim.sw[c.breaker] === 1;
      const cut = sim.conduitCut(c.id);
      const live = p.n(`ckt.${c.id}.f`) >= 0.5 && !cut;
      const pri = ['ALTA', 'NORMAL', 'BAJA'][Math.round(sim.sw[c.priority] ?? 1)] ?? '';
      g.fillStyle = live ? C.ok : cut ? C.bad : C.warn;
      g.fillText(c.label, 16, y);
      g.fillStyle = C.txt;
      g.fillText(`${p.n(`ckt.${c.id}.kw`).toFixed(1)} kW`, 150, y);
      g.fillStyle = live ? C.ok : C.dim;
      g.fillText(live ? 'CON TENSIÓN' : !brk ? 'DISYUNTOR' : cut ? `CORTE ${cut.id}` : 'SIN ALIMENTAR', 230, y);
      g.fillStyle = pri === 'BAJA' ? C.warn : C.dim;
      g.fillText(pri, 400, y);
    });
  },

  fuel(p) {
    const { sim } = p;
    p.header('PROPELENTE');
    const fuel = sim.sys.fuel;
    if (!fuel) return p.note('Esta nave no lleva red de propelente.', 56);
    const r = p.rows(46);
    for (const t of fuel.tanks) {
      const kg = p.n(`${t.id}.kg`);
      const valve = sim.sw[fuel.valveOf(t.id)!] === 1;
      const pump = sim.sw[fuel.pumpOf(t.id)!] === 1;
      r.row(t.name.replace('Depósito ', '').toUpperCase().slice(0, 14), `${kg.toFixed(0)} / ${t.p.cap} kg · válv. ${valve ? 'ABIERTA' : 'CERRADA'} · bomba ${pump ? 'ON' : 'OFF'}`, kg < t.p.cap * 0.15 ? C.bad : valve ? C.ok : C.warn);
    }
    r.row('TOTAL', `${p.n('fuel.kg').toFixed(0)} / ${p.n('fuel.cap').toFixed(0)} kg`, C.txt);
    const x = sim.def.fluid.transfer;
    if (x) {
      const states = sim.def.controls.find((c) => c.key === x.key)?.states ?? [];
      r.row('TRANSFER.', `${states[Math.round(sim.sw[x.key] ?? 0)] ?? '?'} · ${p.n('fuel.xfer').toFixed(1)} kg/s`, p.n('fuel.xfer') > 0.1 ? C.ok : C.dim);
    }
    // valves between manifolds (cross-feeds)
    const manifolds = new Set(sim.def.fluid.manifolds);
    for (const pipe of sim.def.fluid.pipes) {
      if (!pipe.valve || !manifolds.has(pipe.a) || !manifolds.has(pipe.b)) continue;
      const c = sim.def.controls.find((k) => k.key === pipe.valve);
      const open = sim.sw[pipe.valve] === 1;
      r.row((c?.label ?? pipe.valve).toUpperCase(), open ? 'ABIERTA' : 'CERRADA', open ? C.ok : C.dim);
    }
  },

  atmos(p) {
    const { sim } = p;
    p.header('ATMÓSFERA');
    const life = sim.sys.life;
    if (!life) return p.note('Nave sin compartimentos presurizados.', 56);
    const r = p.rows(46);
    const cfg = sim.def.life;
    if (cfg) {
      const modes = sim.def.controls.find((c) => c.key === cfg.mode)?.states ?? ['AUTOMÁTICO', 'MANUAL', 'APAGADO'];
      r.row('CONTROL', (modes[Math.round(sim.sw[cfg.mode] ?? 0)] ?? '?') + (p.n('ls.leak') === 1 ? ' · FUGA, SUSPENDIDO' : ''), p.n('ls.leak') === 1 ? C.bad : C.ok);
    }
    r.row('GEN O₂', `${(p.n('ls.o2') * 1000).toFixed(1)} mmol/s`, p.n('ls.o2') > 0.01 ? C.ok : C.warn);
    r.row('DEPURADOR', p.n('ls.scrub') > 0.2 ? 'EN MARCHA' : 'PARADO', p.n('ls.scrub') > 0.2 ? C.ok : C.warn);
    for (const b of life.bottles) {
      const kg = p.n(`${b.part.id}.kg`);
      r.row(b.part.name.toUpperCase().slice(0, 14), `${kg.toFixed(0)} / ${b.cap} kg`, kg < b.cap * 0.15 ? C.bad : C.txt);
    }
    for (const c of sim.def.compartments) {
      const pr = p.n(`${c.id}.p`);
      const sealed = p.n(`${c.id}.sealed`) === 1;
      r.row(c.label, `${pr.toFixed(1)} kPa · O₂ ${p.n(`${c.id}.po2`).toFixed(1)} · CO₂ ${p.n(`${c.id}.pco2`).toFixed(2)} · ${Math.round(p.n(`${c.id}.t`))} °C · ${sealed ? 'ESTANCO' : 'ABIERTO'}`, pr > 50 && sealed ? C.ok : pr < 5 ? C.bad : C.warn);
    }
  },

  engines(p) {
    const { sim } = p;
    p.header('MOTORES');
    const r = p.rows(46);
    for (const e of engines(sim)) {
      const s = Math.round(p.n(`${e.tag}.state`));
      const risk = p.n(`${e.tag}.risk`);
      const hp = sim.partHp(e.part);
      r.row(e.part.name.toUpperCase(), `${ENG_NAME[s] ?? '?'} · ${Math.round(p.n(`${e.tag}.thr`) * 100)} % · ${Math.round(p.n(`${e.tag}.temp`))} °C · alim. ${p.n(`${e.part.id}.feed`).toFixed(2)}`, risk > 0.05 || hp < e.part.maxHp * 0.35 ? C.bad : s === ENG.run ? C.ok : C.dim);
    }
    for (const a of apus(sim)) {
      const s = Math.round(p.n(`${a.tag}.state`));
      r.row(a.part.name.toUpperCase(), `${ENG_NAME[s] ?? '?'} · ${(p.n(`${a.tag}.out`) * a.k.kw).toFixed(1)} kW`, s === ENG.fail ? C.bad : s === ENG.run ? C.ok : C.dim);
    }
    const use = mainUse(sim);
    if (use.count) {
      r.row('EMPUJE PPAL', `${Math.round(use.thr * 100)} % · tope ${Math.round(use.throttle * 100)} % · ${use.mode}`, use.thr > 0.02 ? C.ok : C.dim);
      r.row('', use.note, use.thr > 0.02 ? C.txt : C.warn);
    }
    for (const key of sim.def.readouts?.engines ?? []) p.switchRow(r, key);
  },

  reactor(p) {
    const { sim } = p;
    p.header('REACTOR');
    const rx = reactors(sim)[0];
    if (!rx) return p.note('Esta nave no lleva reactor.', 56);
    const r = p.rows(46);
    const T = rx.tag;
    const s = Math.round(p.n(`${T}.state`));
    const set = REACTOR_SET[Math.round(sim.sw[rx.keys.set] ?? 0)] ?? 0;
    const temp = p.n(`${T}.temp`);
    r.row('ESTADO', RX_NAME[s] ?? '?', s === RX.online ? C.ok : s === RX.scram ? C.bad : C.warn);
    r.row('SALIDA', `${Math.round(set * 100)} % pedido · ${(p.n(`${T}.out`) * 100).toFixed(0)} % real · tope ${(p.n(`${T}.cap`) * 100).toFixed(0)} %`, set > 1 ? C.warn : C.txt);
    r.row('NÚCLEO', `${Math.round(temp)} °C`, temp > rx.k.scramC ? C.bad : temp > rx.k.warnC ? C.warn : C.ok);
    r.row('REFRIG.', `${Math.round(p.n(`${T}.cool.temp`))} °C · flujo ${(p.n(`${T}.cool.flow`) * 100).toFixed(0)} %`, p.n(`${T}.cool.flow`) < 0.5 ? C.bad : C.ok);
    r.row('RADIADORES', `${p.n(`${T}.rad.area`).toFixed(1)} m² · ${p.n(`${T}.rad.kw`).toFixed(1)} kW`, C.txt);
    r.row('ARRANQUE', p.n(`${T}.start`) > 0.05 && s === RX.starting ? `${Math.round(p.n(`${T}.start`) * 100)} %` : '—', C.dim);
    const hp = sim.partHp(rx.part) / rx.part.maxHp;
    r.row('INTEGRIDAD', `${Math.round(hp * 100)} %`, hp < 0.5 ? C.bad : hp < 0.85 ? C.warn : C.ok);
  },

  alerts(p) {
    const { g, sim } = p;
    p.header('ALARMAS');
    const list = sim.sys.active(sim.st);
    g.font = `600 13px ${FONT}`;
    if (!list.length) {
      g.fillStyle = C.ok;
      g.fillText('SIN ALARMAS', 16, 52);
    }
    list.slice(0, 12).forEach((a, i) => {
      g.fillStyle = a.level === 2 ? C.bad : C.warn;
      g.fillText(`${a.level === 2 ? 'AVISO' : 'PRECAU'}  ${a.lamp}  ${a.label}`, 16, 50 + i * 18);
    });
    if (list.length > 12) {
      g.fillStyle = C.dim;
      g.fillText(`+${list.length - 12} más`, 16, 50 + 12 * 18);
    }
  },

  flight(p) {
    const { g, sim, anim } = p;
    p.header('VUELO');
    const f = p.fl();
    // attitude: the horizon moves against the ship's pitch and roll
    const cx = 86;
    const cy = 46 + (p.h - 46) / 2;
    const R = Math.min(70, (p.h - 56) / 2);
    g.save();
    g.beginPath();
    g.arc(cx, cy, R, 0, Math.PI * 2);
    g.clip();
    g.translate(cx, cy);
    g.rotate(f.roll);
    const off = (f.pitch * R) / 0.6;
    g.fillStyle = '#123a52';
    g.fillRect(-R * 2, -R * 2 + off, R * 4, R * 2);
    g.fillStyle = '#3a2a16';
    g.fillRect(-R * 2, off, R * 4, R * 2);
    g.strokeStyle = C.txt;
    g.lineWidth = 2;
    line(g, -R * 2, off, R * 2, off);
    g.lineWidth = 1;
    g.strokeStyle = 'rgba(191,239,255,0.6)';
    for (const deg of [-20, -10, 10, 20]) {
      const y = off - ((deg * Math.PI) / 180) * (R / 0.6);
      line(g, -R * 0.25, y, R * 0.25, y);
    }
    g.restore();
    g.strokeStyle = C.warn;
    g.lineWidth = 3;
    line(g, cx - R * 0.55, cy, cx - R * 0.15, cy);
    line(g, cx + R * 0.15, cy, cx + R * 0.55, cy);
    g.fillStyle = C.warn;
    g.fillRect(cx - 2, cy - 2, 4, 4);
    g.lineWidth = 2;
    g.strokeStyle = 'rgba(79,216,240,0.6)';
    g.beginPath();
    g.arc(cx, cy, R, 0, Math.PI * 2);
    g.stroke();
    g.lineWidth = 1;
    // numbers
    const x0 = 176;
    let y = 46;
    const row = (label: string, value: string, color: string) => {
      g.font = `600 13px ${FONT}`;
      g.fillStyle = C.dim;
      g.fillText(label, x0, y);
      g.font = `700 15px ${FONT}`;
      g.fillStyle = color;
      g.fillText(value, x0 + 104, y - 1);
      y += 21;
    };
    if (f.orbital || f.alt > 3000) {
      // high up or fast: the orbit instead of the ground
      const o = f.orbit;
      const km = (m: number) => (Number.isFinite(m) ? `${(m / 1000).toFixed(1)} km` : '∞');
      row('ALTITUD', km(o.altitude), C.txt);
      row('VELOCIDAD', `${Math.round(o.speed)} m/s`, o.speed >= o.circular * 0.99 ? C.ok : C.txt);
      row('CIRCULAR', `${Math.round(o.circular)} m/s`, C.dim);
      row('APOÁPSIDE', km(o.apoapsis), C.txt);
      row('PERIÁPSIDE', km(o.periapsis), o.periapsis < 0 ? C.bad : o.orbiting ? C.ok : C.warn);
      row('ÓRBITA', o.eccentricity >= 1 ? 'ESCAPE' : o.orbiting ? `ESTABLE · ${Math.floor(o.period / 60)} min` : 'SUBORBITAL', o.orbiting ? C.ok : C.warn);
      row('BASE', `${Math.round(f.base.dist / 1000)} km · ${deg3(f.base.bearing)}°`, C.cyan);
      const mode = f.direct ? 'DIRECTO' : f.orbital ? 'ORBITAL' : sim.sw['fa.hold'] !== 0 ? 'ACOPLADO' : 'DESACOPLADO';
      row('MANDO', mode, f.direct ? C.bad : C.ok);
      g.font = `700 12px ${FONT}`;
      g.fillStyle = f.apOn ? C.ok : C.dim;
      g.fillText(f.apOn ? `P.AUT  ${f.modes.join(' · ') || 'CONECTADO'}` : 'P.AUT  DESCONECTADO', 16, p.h - 8);
      return;
    }
    row('ALTURA', `${f.agl.toFixed(1)} m`, f.agl < 3 && !f.landed ? C.warn : C.txt);
    row('VERTICAL', `${f.vs >= 0 ? '+' : ''}${f.vs.toFixed(1)} m/s`, f.vs < -3 && f.agl < 20 ? C.bad : C.txt);
    row('VELOCIDAD', `${f.gs.toFixed(1)} m/s`, C.txt);
    row('RUMBO', `${deg3(f.heading)}°`, C.txt);
    const gear = sim.def.gear;
    if (gear) {
      const down = anim.movers[gear.key] ?? 0;
      row('TREN', down > 0.99 ? 'ABAJO' : down < 0.01 ? 'ARRIBA' : 'EN TRÁNSITO', down > 0.99 ? C.ok : down < 0.01 ? C.txt : C.warn);
    }
    const tw = f.weight > 0 ? f.thrust / f.weight : 0;
    row('EMPUJE', `${(f.thrust / 1000).toFixed(1)} kN · ${tw.toFixed(2)}×P`, C.txt);
    const mode = f.direct ? 'DIRECTO' : sim.sw['fa.hold'] !== 0 ? 'ACOPLADO' : 'DESACOPLADO';
    row('MANDO', mode, f.direct ? C.bad : C.ok);
    row('SUELO', f.landed ? 'EN TIERRA' : 'EN VUELO', f.landed ? C.ok : C.cyan);
    g.font = `700 12px ${FONT}`;
    g.fillStyle = f.apOn ? C.ok : C.dim;
    g.fillText(f.apOn ? `P.AUT  ${f.modes.join(' · ') || 'CONECTADO'}` : 'P.AUT  DESCONECTADO', 16, p.h - 8);
  },

  ap(p) {
    const { g, w, sim } = p;
    p.header('PILOTO AUTOMÁTICO');
    const ap = sim.def.autopilot;
    if (!ap) return p.note('Esta nave no tiene piloto automático.', 56);
    const f = p.fl();
    g.font = `700 16px ${FONT}`;
    g.fillStyle = f.direct ? C.bad : f.apOn ? C.ok : C.dim;
    g.fillText(f.direct ? 'SIN ORDENADOR DE VUELO' : f.apOn ? 'CONECTADO' : 'DESCONECTADO', 16, 44);
    // the drum's face out, and one lit box per mode on it (plus any engaged elsewhere)
    const face = sim.sw[AP_FACE_KEY] ?? 0;
    g.font = `700 13px ${FONT}`;
    g.fillStyle = C.cyan;
    g.fillText(`CARA ${AP_FACES[face] ?? face}`, w - 150, 44);
    const modes = apModes(ap).filter((m) => m.face === face || sim.sw[AP_KEY(m.id)] === 1);
    const cols = Math.max(1, Math.min(modes.length, 4));
    const bw = (w - 32) / cols;
    modes.forEach((m, i) => {
      const x = 16 + (i % cols) * bw;
      const y = 68 + Math.floor(i / cols) * 28;
      const on = sim.sw[AP_KEY(m.id)] === 1;
      g.fillStyle = on ? 'rgba(92,242,154,0.28)' : 'rgba(79,216,240,0.07)';
      g.fillRect(x, y, bw - 6, 22);
      g.fillStyle = on ? C.ok : C.dim;
      g.font = `700 13px ${FONT}`;
      g.fillText(m.label, x + 8, y + 4);
    });
    const r = p.rows(68 + Math.ceil(modes.length / cols) * 28 + 6);
    const sel = f.sel!;
    if (face === 0) {
      r.row('ALTURA SEL.', `${sel.alt} m (ahora ${f.agl.toFixed(0)})`, C.txt);
      r.row('RUMBO SEL.', `${deg3(sel.hdg)}° (ahora ${deg3(f.heading)}°)`, C.txt);
      r.row('VELOC. SEL.', `${sel.spd} m/s (ahora ${f.gs.toFixed(1)})`, C.txt);
      if (f.wp) r.row('PUNTO', `${f.wp.name} · ${fmtDist(f.wp.dist)} · ${deg3(f.wp.bearing)}°`, C.cyan);
    } else {
      const o = f.orbit;
      r.row('ÓRBITA SEL.', `${(sel.orb / 1000).toFixed(0)} km · ${circularAt(bodyAt(sim.pose.p), sel.orb).toFixed(0)} m/s`, C.txt);
      r.row('AHORA', `${(o.altitude / 1000).toFixed(1)} km · ${o.speed.toFixed(0)} m/s (circ. ${o.circular.toFixed(0)})`, C.txt);
      r.row('PE / AP', `${(o.periapsis / 1000).toFixed(1)} / ${Number.isFinite(o.apoapsis) ? (o.apoapsis / 1000).toFixed(1) : '∞'} km`, o.orbiting ? C.ok : C.warn);
      r.row('BASE', `${fmtDist(f.base.dist)} · ${deg3(f.base.bearing)}°`, C.cyan);
      r.row('SOBREPOT.', sim.sw[BOOST.key] === 1 ? 'CONECTADA' : 'NORMAL', sim.sw[BOOST.key] === 1 ? C.warn : C.dim);
    }
    if (f.modes.length) p.note(f.modes.join(' · '), Math.min(r.y + 4, p.h - 8));
  },

  nav(p) {
    const { g, w, sim } = p;
    p.header('NAVEGACIÓN');
    const f = p.fl();
    const body = bodyAt(sim.pose.p);
    const top = 42;
    const H = p.h - top;
    const cx = w / 2;
    const cy = top + H / 2;
    // scale: the selected point and the base in view, between 150 m and 3 km across
    const far = Math.max(150, Math.min(3000, Math.max(f.wp?.dist ?? 0, f.base.dist) * 1.25));
    const k = (Math.min(w, H) * 0.46) / far;
    g.strokeStyle = 'rgba(79,216,240,0.18)';
    for (const ring of [0.33, 0.66, 1]) {
      g.beginPath();
      g.arc(cx, cy, far * ring * k, 0, Math.PI * 2);
      g.stroke();
    }
    g.fillStyle = C.dim;
    g.font = `600 11px ${FONT}`;
    g.fillText(fmtDist(far), cx + far * k - 44, cy - 4);
    g.fillText('N', cx - 4, top + 2);
    // every point on the local horizon (north up), over the great circle from the ship
    let sx = cx;
    let sy = cy;
    for (const pt of NAV_POINTS) {
      if (pt.body !== body.def.id) continue;
      const to = bearingTo(body, sim.pose.p, pt);
      const x = cx + Math.sin(to.bearing) * to.dist * k;
      const y = cy - Math.cos(to.bearing) * to.dist * k;
      const selected = f.sel?.wp.id === pt.id;
      if (selected) {
        sx = x;
        sy = y;
      }
      if (x < 8 || x > w - 8 || y < top || y > p.h) continue;
      g.fillStyle = selected ? C.ok : pt.pad ? C.cyan : C.txt;
      g.fillRect(x - 3, y - 3, 6, 6);
      g.font = `700 11px ${FONT}`;
      g.fillText(pt.name, x + 6, y - 8);
    }
    if (f.sel && (sx !== cx || sy !== cy)) {
      g.strokeStyle = 'rgba(92,242,154,0.6)';
      g.setLineDash([6, 5]);
      line(g, cx, cy, sx, sy);
      g.setLineDash([]);
    }
    // the ship: a triangle along its heading (north up)
    g.save();
    g.translate(cx, cy);
    g.rotate(f.heading);
    g.fillStyle = C.warn;
    g.beginPath();
    g.moveTo(0, -9);
    g.lineTo(6, 7);
    g.lineTo(-6, 7);
    g.closePath();
    g.fill();
    g.restore();
    g.font = `700 12px ${FONT}`;
    g.fillStyle = C.txt;
    g.fillText(`RUMBO ${deg3(f.heading)}°  ${f.gs.toFixed(1)} m/s`, 12, p.h - 8);
    if (f.wp) {
      g.textAlign = 'right';
      g.fillStyle = C.ok;
      g.fillText(`${f.wp.name} ${fmtDist(f.wp.dist)}`, w - 12, p.h - 8);
      g.textAlign = 'left';
    }
  },

  mass(p) {
    const { sim } = p;
    p.header('MASA Y EMPUJE');
    const r = p.rows(46);
    const m = sim.flight.mp;
    const perf = performance(sim.def, m, MOON.gravity);
    r.row('MASA', `${(m.mass / 1000).toFixed(2)} t`, C.txt);
    const g = m.groups;
    r.row('  SECA', `${((g.estructura + g.máquinas + g.mobiliario) / 1000).toFixed(2)} t`, C.dim);
    r.row('  PROPELENTE', `${g.propelente.toFixed(0)} kg`, C.dim);
    r.row('  A BORDO', `${g.tripulación.toFixed(0)} kg gente · ${g.carga.toFixed(0)} kg carga`, C.dim);
    r.row('C. DE MASAS', `x ${m.com[0].toFixed(2)}  y ${m.com[1].toFixed(2)}  z ${m.com[2].toFixed(2)} m`, Math.abs(m.com[0]) > 0.2 ? C.warn : C.txt);
    r.row('SUSTENTACIÓN', `${(perf.liftN / 1000).toFixed(1)} kN · ${perf.liftTwr.toFixed(2)}× peso`, perf.liftTwr > 1.2 ? C.ok : C.warn);
    r.row('MOTORES', `${(perf.maxN / 1000).toFixed(1)} kN · Isp ${perf.isp.toFixed(0)} s`, C.txt);
    r.row('Δv', `${perf.dv.toFixed(0)} m/s`, perf.dv > 500 ? C.ok : C.warn);
    r.row('BRAZO EMPUJE', `${perf.lever.toFixed(2)} m del c. de masas`, perf.lever > 0.3 ? C.warn : C.dim);
  },

  stores(p) {
    const { sim } = p;
    p.header('VÍVERES Y AGUA');
    const st = sim.sys.module<Stores>('stores');
    const r = p.rows(46);
    if (st) {
      const aboard = Math.round(p.n('stores.aboard'));
      const water = p.n('stores.water');
      const food = p.n('stores.food');
      const recycling = st.recyclers.some((x) => st.running(sim.st, sim.sw, x));
      // hours left at the current crew, with / without the recycler
      const hours = (kg: number, perHead: number) => (aboard > 0 ? `${((kg / (perHead * aboard)) / 3600).toFixed(0)} h` : '—');
      r.row('A BORDO', `${aboard} persona${aboard === 1 ? '' : 's'}`, C.txt);
      r.row('AGUA', `${water.toFixed(1)} / ${p.n('stores.water.cap').toFixed(0)} kg · ${hours(water, STORES.water * (recycling ? 1 - STORES.eff : 1))}`, water < p.n('stores.water.cap') * 0.15 ? C.bad : C.ok);
      r.row('VÍVERES', `${food.toFixed(1)} / ${p.n('stores.food.cap').toFixed(0)} kg · ${hours(food, STORES.food)}`, food < p.n('stores.food.cap') * 0.15 ? C.bad : C.ok);
      r.row('RECICLADOR', recycling ? `EN MARCHA · ${(p.n('stores.recycle') * 3600).toFixed(2)} kg/h` : 'PARADO', recycling ? C.ok : C.warn);
      r.row('AGUA GRIS', `${p.n('stores.grey').toFixed(2)} kg`, p.n('stores.grey') > 0.5 ? C.warn : C.dim);
    }
    const arrays = sim.sys.modules.filter((m): m is SolarArray => m.id.startsWith('solar:'));
    for (const a of arrays) {
      r.row(a.part.name.toUpperCase().slice(0, 14), `${p.n(`${a.tag}.kw`).toFixed(2)} / ${a.k.kw} kW · ${Math.round(a.exposure(sim.st) * 100)} % abierta`, p.n(`${a.tag}.kw`) > 0.1 ? C.ok : C.dim);
    }
    if (!st && !arrays.length) p.note('Esta nave no lleva víveres para viajes largos ni paneles solares.', 56);
  },

  lock(p) {
    const { g, w, sim, anim } = p;
    p.header('ESCLUSA');
    const a = sim.def.airlock;
    if (!a) return p.note('Esta nave no tiene esclusa.', 56);
    const ph = Math.round(p.n('lock.phase'));
    const cyc = ph !== LOCK.in && ph !== LOCK.out;
    g.font = `700 20px ${FONT}`;
    g.fillStyle = cyc ? C.warn : ph === LOCK.in ? C.ok : C.bad;
    g.textAlign = 'center';
    g.fillText(LOCK_NAME[ph] ?? '?', w / 2, 48);
    g.textAlign = 'left';
    const r = p.rows(80);
    const inner = sim.def.openings.find((o) => o.key === a.inner)!;
    const cabin = inner.a === a.zone ? inner.b : inner.a;
    const pl = sim.sys.pressure(sim.st, a.zone);
    const pc = sim.sys.pressure(sim.st, cabin);
    r.row('ESCLUSA', `${pl.toFixed(1)} kPa`, pl < 3 ? C.bad : pl > 60 ? C.ok : C.warn);
    r.row('HABITÁCULO', `${pc.toFixed(1)} kPa`, pc > 60 ? C.ok : C.warn);
    r.row('P. INTERIOR', `${Math.round((anim.movers[a.inner] ?? 0) * 100)} % abierta`, C.txt);
    r.row('ESCOTILLA', `${Math.round((anim.movers[a.outer] ?? 0) * 100)} % abierta`, C.txt);
    const rec = sim.def.life?.recover?.key;
    if (rec) r.row('COMPRESOR', sim.sw[rec] === 1 ? 'RECUPERANDO' : 'PARADO', sim.sw[rec] === 1 ? C.warn : C.dim);
    if (a.duct) r.row('CONDUCTO', sim.sw[a.duct] === 1 ? 'ABIERTO' : 'CERRADO', C.dim);
  },

  radar(p) {
    const { sim } = p;
    p.header('RADAR');
    const r = p.rows(46);
    for (const key of sim.def.readouts?.radar ?? []) p.switchRow(r, key);
    for (const part of sim.def.parts.filter((x) => x.type === 'radar')) {
      const fed = part.circuit ? sim.powered(part.circuit) : true;
      const hp = sim.partHp(part) / part.maxHp;
      r.row('ALIMENTACIÓN', fed ? 'CON TENSIÓN' : 'SIN ENERGÍA', fed ? C.ok : C.bad);
      r.row(part.name.toUpperCase().slice(0, 14), `${Math.round(hp * 100)} %`, hp < 0.5 ? C.bad : C.ok);
    }
    p.note('El barrido todavía no genera contactos.', r.y + 8);
  },

  weapons(p) {
    const { sim } = p;
    p.header('ARMAMENTO');
    const r = p.rows(46);
    for (const key of sim.def.readouts?.weapons ?? []) p.switchRow(r, key);
    for (const part of sim.def.parts.filter((x) => x.type === 'turret')) {
      const fed = part.circuit ? sim.powered(part.circuit) : true;
      const hp = sim.partHp(part) / part.maxHp;
      r.row('CIRCUITO', fed ? 'CON TENSIÓN' : 'CORTE', fed ? C.ok : C.bad);
      r.row(part.name.toUpperCase().slice(0, 14), `${Math.round(hp * 100)} %`, hp < 0.5 ? C.bad : C.ok);
    }
    p.note('La torreta aún no dispara. El interruptor y el daño sí cuentan.', r.y + 8);
  },
};

const reactors = (sim: ShipSim) => sim.sys.modules.filter((m): m is Reactor => m.id.startsWith('reactor:'));
const apus = (sim: ShipSim) => sim.sys.modules.filter((m): m is Apu => m.id.startsWith('apu:'));
const engines = (sim: ShipSim) => sim.sys.modules.filter((m): m is Engine => m.id.startsWith('engine:'));

/** Screens farther than this from the eye (m) keep their last picture: nobody can read them. */
const SCREEN_READ_M = 16;
const _sp = new THREE.Vector3();
const _sn = new THREE.Vector3();

/** The screen background (fill + grid), drawn once per size and blitted (one call, not 30 strokes). */
const backgrounds = new Map<string, HTMLCanvasElement>();
function background(w: number, h: number) {
  const key = `${w}x${h}`;
  let c = backgrounds.get(key);
  if (!c) {
    c = document.createElement('canvas');
    c.width = w;
    c.height = h;
    const g = c.getContext('2d')!;
    g.fillStyle = C.bg;
    g.fillRect(0, 0, w, h);
    g.strokeStyle = C.grid;
    g.lineWidth = 1;
    for (let x = 0; x < w; x += 32) line(g, x, 0, x, h);
    for (let y = 0; y < h; y += 32) line(g, 0, y, w, y);
    backgrounds.set(key, c);
  }
  return c;
}

/** Multi-function displays drawn into canvases (the ship's own diagnostics, readable in-world). */
export class ShipScreens {
  readonly meshes: THREE.Mesh[] = [];
  private items: Array<{ def: ScreenDef; mesh: THREE.Mesh; ctx: CanvasRenderingContext2D; tex: THREE.CanvasTexture; mat: THREE.MeshBasicMaterial; w: number; h: number; slot: number; phase: number; page: ScreenPage | null }> = [];
  /** Page each screen last drew (a new page is drawn at once, not at the next 4 Hz slot). */
  private shown = new Map<string, ScreenPage>();

  constructor(
    private sim: ShipSim,
    /** The ground of each body (the flight displays measure the height above it). */
    private surfaces: Surfaces,
  ) {
    for (const def of sim.def.screens) {
      const w = 512;
      const h = Math.round((w * def.h) / def.w);
      const canvas = document.createElement('canvas');
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext('2d')!;
      const tex = new THREE.CanvasTexture(canvas);
      tex.colorSpace = THREE.SRGBColorSpace;
      tex.anisotropy = 4;
      const mat = sharpText(new THREE.MeshBasicMaterial({ map: tex, color: new THREE.Color(1.5, 1.5, 1.5) }), -0.6);
      const mesh = new THREE.Mesh(new THREE.PlaneGeometry(def.w, def.h), mat);
      mesh.matrixAutoUpdate = false;
      mesh.matrix.makeBasis(new THREE.Vector3(...def.u), new THREE.Vector3(...def.v), new THREE.Vector3(...def.n)).setPosition(...def.c);
      mesh.name = def.id;
      this.meshes.push(mesh);
      // each screen redraws on its own beat: they never all upload in the same frame
      this.items.push({ def, mesh, ctx, tex, mat, w, h, slot: -1, phase: this.items.length / Math.max(1, sim.def.screens.length), page: null });
    }
  }

  /** Someone can read it: close to the eye and facing it. */
  private readable(mesh: THREE.Mesh, eye: THREE.Vector3) {
    const M = mesh.matrixWorld;
    _sp.setFromMatrixPosition(M);
    const dx = eye.x - _sp.x;
    const dy = eye.y - _sp.y;
    const dz = eye.z - _sp.z;
    if (dx * dx + dy * dy + dz * dz > SCREEN_READ_M * SCREEN_READ_M) return false;
    _sn.setFromMatrixColumn(M, 2);
    return _sn.x * dx + _sn.y * dy + _sn.z * dz > 0;
  }

  /**
   * Redraw at ~4 Hz (they are text, nobody needs 60); flight instruments at 12 Hz. Only the screens
   * someone can read are redrawn (`eye`: the camera, world), each on its own beat; a new page is
   * drawn at once.
   */
  update(time: number, anim: ShipAnimState, hidden: (host: number) => boolean, eye?: THREE.Vector3) {
    const sim = this.sim;
    let battery: boolean | null = null;
    for (const it of this.items) {
      const off = hidden(it.def.host);
      it.mesh.visible = !off;
      if (off) continue;
      const page = activePage(it.def, sim.sw);
      const flipped = it.page !== page;
      const slot = Math.floor(time * (LIVE_PAGES.has(page) ? 12 : 4) + it.phase);
      if (slot === it.slot && !flipped) continue;
      if (!flipped && eye && !this.readable(it.mesh, eye)) continue;
      it.slot = slot;
      it.page = page;
      this.shown.set(it.def.id, page);
      const idx = it.def.pages.indexOf(page);
      // the power page stays readable on battery, so a dead avionics bus does not hide the grid
      if (page === 'power' && battery === null) {
        battery = false;
        for (const b of sim.sys.power?.batteries ?? []) if (sim.sw[b.key] === 1 && sim.st[b.iSoc] > 0.001) battery = true;
      }
      const on = sim.powered(it.def.circuit) || (page === 'power' && !!battery);
      it.mat.color.setScalar(on ? 1.5 : 0.02);
      if (!on) continue;
      this.draw(it.ctx, it.w, it.h, page, idx, it.def.pages, time, anim);
      it.tex.needsUpdate = true;
    }
  }

  private draw(g: CanvasRenderingContext2D, w: number, h: number, page: ScreenPage, index: number, pages: ScreenPage[], time: number, anim: ShipAnimState) {
    const sim = this.sim;
    g.drawImage(background(w, h), 0, 0);
    const n = (name: string) => (sim.vars.has(name) ? sim.st[sim.vars.idx(name)] : 0);
    let fl: FlightReadout | null = null;
    const ctx: PageCtx = {
      g,
      w,
      h: h - 34,
      time,
      anim,
      sim,
      fl: () => (fl ??= flightReadout(sim, this.surfaces)),
      n,
      header: (title) => this.header(g, w, title, time),
      rows: (y0) => rows(g, y0),
      switchRow: (r, key) => {
        const c = sim.def.controls.find((x) => x.key === key && x.kind !== 'cover' && x.kind !== 'bezel');
        if (!c) return;
        const v = Math.round(sim.sw[key] ?? 0);
        r.row((c.label || c.name).toUpperCase().slice(0, 14), c.states[v] ?? String(v), v ? C.txt : C.dim);
      },
      note: (text, y) => {
        g.fillStyle = C.dim;
        g.font = `600 13px ${FONT}`;
        g.fillText(text, 16, y);
      },
    };
    const draw = PAGES[page];
    if (draw) draw(ctx);
    else {
      this.header(g, w, pageLabel(page), time);
      ctx.note('Esta página todavía no tiene datos que mostrar.', 56);
    }
    this.tabs(g, w, h, pages, index);
    g.strokeStyle = 'rgba(79,216,240,0.35)';
    g.lineWidth = 2;
    g.strokeRect(3, 3, w - 6, h - 6);
  }

  private header(g: CanvasRenderingContext2D, w: number, title: string, time: number) {
    const sim = this.sim;
    g.fillStyle = C.cyan;
    g.font = `700 18px ${FONT}`;
    g.textBaseline = 'top';
    g.fillText(title, 14, 10);
    g.textAlign = 'right';
    g.fillStyle = C.dim;
    g.font = `600 14px ${FONT}`;
    g.fillText(`${sim.def.name.toUpperCase()} · ${sim.def.registry}`, w - 14, 12);
    g.textAlign = 'left';
    if (sim.sw[sim.def.caution] === 1 && Math.floor(time * 2) % 2 === 0) {
      g.fillStyle = C.warn;
      g.fillRect(w / 2 - 70, 8, 140, 22);
      g.fillStyle = '#1a0f00';
      g.font = `700 14px ${FONT}`;
      g.textAlign = 'center';
      g.fillText('ALARMA GENERAL', w / 2, 12);
      g.textAlign = 'left';
    }
    g.strokeStyle = 'rgba(79,216,240,0.4)';
    line(g, 12, 36, w - 12, 36);
  }

  private tabs(g: CanvasRenderingContext2D, w: number, h: number, pages: ScreenPage[], index: number) {
    const y = h - 26;
    const bw = (w - 24) / pages.length;
    g.font = `700 11px ${FONT}`;
    g.textBaseline = 'top';
    pages.forEach((p, i) => {
      const x = 12 + i * bw;
      g.fillStyle = i === index ? 'rgba(79,216,240,0.38)' : 'rgba(79,216,240,0.08)';
      g.fillRect(x, y, bw - 4, 16);
      g.fillStyle = i === index ? C.txt : C.dim;
      g.fillText(pageLabel(p), x + 4, y + 2);
    });
  }
}

/** Pages with moving instruments (redrawn faster). */
const LIVE_PAGES = new Set<ScreenPage>(['flight', 'ap', 'nav']);

/** Compass heading as three digits (000 = north). */
function deg3(rad: number) {
  return String(((Math.round((rad * 180) / Math.PI) % 360) + 360) % 360).padStart(3, '0');
}

function fmtDist(m: number) {
  return m < 1000 ? `${Math.round(m)} m` : `${(m / 1000).toFixed(1)} km`;
}

function rows(g: CanvasRenderingContext2D, y0: number): Rows {
  let y = y0;
  g.font = `600 14px ${FONT}`;
  return {
    row(label, value, color) {
      g.font = `600 14px ${FONT}`;
      g.fillStyle = C.dim;
      g.fillText(label, 16, y);
      g.fillStyle = color;
      g.fillText(value, 168, y);
      y += 20;
    },
    get y() {
      return y;
    },
  };
}

function line(g: CanvasRenderingContext2D, x0: number, y0: number, x1: number, y1: number) {
  g.beginPath();
  g.moveTo(x0, y0);
  g.lineTo(x1, y1);
  g.stroke();
}

function bar(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, v: number, color: string) {
  g.strokeStyle = 'rgba(79,216,240,0.4)';
  g.lineWidth = 1;
  g.strokeRect(x, y, w, h);
  g.fillStyle = color;
  g.fillRect(x + 2, y + 2, Math.max(0, Math.min(1, v)) * (w - 4), h - 4);
}
