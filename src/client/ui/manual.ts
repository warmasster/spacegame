import { deckOf, moduleBase, pageLabel, SCREEN_PAGES, type ControlDef, type ControlKind, type ManualBlock, type ManualFig, type ManualLive, type ShipDef } from '../../shared/ship/def';
import { zoneAt } from '../../shared/ship/crew';
import type { V3 } from '../../shared/ship/geom';
import { RX, type Reactor } from '../../shared/ship/modules/reactor';
import { LOCK, LOCK_NAME } from '../../shared/ship/modules/airlock';
import type { ShipSim } from '../../shared/ship/sim';
import { componentSheet, maker } from '../../shared/ship/catalog/index';
import { flightReadout, performance } from '../../shared/ship/flight';
import type { Surfaces } from '../../shared/space/body';
import { MOON } from '../../shared/constants';

/**
 * The ship manual (M): guide chapters written in the ship's data (`def.manual`), plus everything
 * that can be generated from the ship itself — a plan with every console, every control with its
 * help and its live position, the circuits, the alarms, the MFD pages. While open it refreshes a few
 * times a second, so it doubles as a status board. "Señalar" marks a control in the helmet.
 */
export interface ManualHooks {
  /** Mark a control in the world (null = stop). */
  point(index: number | null): void;
  /** Where the reader stands, in ship space (for the plan), or null outside. */
  where(): V3 | null;
  /** The ground of each body (height above it, for the live flight strip). */
  surfaces?: Surfaces;
}

const KIND: Record<ControlKind, string> = {
  button: 'Pulsador',
  toggle: 'Interruptor',
  lever: 'Palanca',
  breaker: 'Disyuntor',
  master: 'Pulsador luminoso',
  mushroom: 'Seta de emergencia',
  rotary: 'Selector · clic o rueda',
  cover: 'Tapa',
  valve: 'Válvula',
  bezel: 'Botón de página',
};

const RX_NAME = ['PARADO', 'ARRANCANDO', 'EN MARCHA', 'SCRAM'];

export class ShipManual {
  private readonly def: ShipDef;
  private body: HTMLElement;
  private nav: HTMLElement;
  private search: HTMLInputElement;
  private count: HTMLElement;
  private last = 0;
  private pointed: number | null = null;
  private liveEls: {
    ctl: Array<{ c: ControlDef; state: HTMLElement | null; why: HTMLElement | null; pos: HTMLElement[]; card: HTMLElement }>;
    [k: string]: unknown;
  } = { ctl: [] };

  constructor(
    private root: HTMLElement,
    private sim: ShipSim,
    private hooks: ManualHooks,
  ) {
    this.def = sim.def;
    root.innerHTML = `
      <header class="mn-head">
        <div class="mn-title"><small>MANUAL DE A BORDO</small><b>${esc(this.def.name)} · ${esc(this.def.registry)}</b></div>
        <label class="mn-search"><input type="search" placeholder="Buscar un mando, una alarma…" spellcheck="false"><span class="mn-count"></span></label>
        <button type="button" class="mn-close" data-act="close" title="Cerrar (M)">✕</button>
      </header>
      <div class="mn-wrap"><nav class="mn-nav"></nav><div class="mn-body"></div></div>`;
    this.body = root.querySelector('.mn-body')!;
    this.nav = root.querySelector('.mn-nav')!;
    this.search = root.querySelector('input')!;
    this.count = root.querySelector('.mn-count')!;
    this.build();
    root.addEventListener('click', (ev) => this.onClick(ev));
    this.search.addEventListener('input', () => {
      this.filter();
      this.body.scrollTop = 0;
    });
    this.body.addEventListener('scroll', () => this.spy(), { passive: true });
  }

  /** The wheel while open (pointer still locked): scroll the page. */
  scroll(dy: number) {
    this.body.scrollTop += dy;
  }

  focusSearch() {
    this.search.focus({ preventScroll: true });
  }

  // ------------------------------------------------------------------------------------------------
  // Build
  // ------------------------------------------------------------------------------------------------

  private build() {
    const def = this.def;
    const sections = def.manual;
    this.nav.innerHTML = sections
      .map((s, i) => `<button type="button" data-toc="${esc(s.id)}"><i>${String(i + 1).padStart(2, '0')}</i><span><b>${esc(s.title)}</b>${s.lead ? `<small>${esc(s.lead)}</small>` : ''}</span></button>`)
      .join('');
    this.body.innerHTML = sections
      .map(
        (s, i) => `<section class="mn-sec" id="mn-${esc(s.id)}" data-sec="${esc(s.id)}">
          <h2><i>${String(i + 1).padStart(2, '0')}</i>${esc(s.title)}</h2>${s.lead ? `<p class="mn-lead">${esc(s.lead)}</p>` : ''}
          ${s.body.map((b) => this.block(b)).join('')}
        </section>`,
      )
      .join('');
    // live element references
    this.liveEls.ctl = [];
    this.body.querySelectorAll<HTMLElement>('[data-card]').forEach((card) => {
      const c = def.controls[Number(card.dataset.card)];
      this.liveEls.ctl.push({ c, card, state: card.querySelector('[data-state]'), why: card.querySelector('[data-why]'), pos: [...card.querySelectorAll<HTMLElement>('[data-pos]')] });
    });
    this.spy();
  }

  private block(b: ManualBlock): string {
    if (typeof b === 'string') return `<p>${this.link(b)}</p>`;
    if ('steps' in b) return `<ol class="mn-steps">${b.steps.map((s) => `<li>${this.link(s)}</li>`).join('')}</ol>`;
    if ('note' in b) return `<p class="mn-note">${this.link(b.note)}</p>`;
    if ('warn' in b) return `<p class="mn-warn">${this.link(b.warn)}</p>`;
    if ('live' in b) return `<div class="mn-live" data-live="${b.live}"></div>`;
    if ('controls' in b) return `<div class="mn-cards">${b.controls.map((k) => this.keyCard(k)).join('')}</div>`;
    return this.figure(b.fig);
  }

  /** `[[console/key]]` → a link to that control's card; `**text**` → bold. */
  private link(text: string) {
    return esc(text).replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>').replace(/\[\[([^\]]+)\]\]/g, (_, id: string) => {
      const c = this.def.controls.find((x) => x.id === id);
      return `<button type="button" class="mn-ref" data-jump="${esc(id)}">${esc(c?.name ?? id)}</button>`;
    });
  }

  private consoleOf(id: string) {
    return this.def.consoles.find((c) => c.id === id);
  }

  private place(p: V3) {
    return zoneAt(this.def, [p[0], p[1] - 0.8, p[2]])?.label ?? 'EXTERIOR';
  }

  /** One card for a switch key, with every console it can be operated from. */
  private keyCard(key: string) {
    const list = this.def.controls.filter((c) => c.key === key && c.kind !== 'cover');
    if (!list.length) return '';
    return this.card(list[0], list);
  }

  private card(c: ControlDef, places: ControlDef[] = [c]) {
    const def = this.def;
    const guard = c.guard ? `<span class="mn-tag warn">Bajo tapa</span>` : '';
    const needs = c.requires ? `<span class="mn-tag">Necesita ${esc(def.subsystems.find((s) => s.id === c.requires)?.label ?? c.requires)}</span>` : '';
    const momentary = c.action === 'pulse' || c.action === 'reset';
    const positions = momentary ? '' : `<div class="mn-pos">${c.states.map((s, i) => `<span data-pos="${i}">${esc(s || '—')}</span>`).join('')}</div>`;
    const where = places
      .map((p) => {
        const con = this.consoleOf(p.console);
        return `<button type="button" class="mn-point" data-point="${p.index}" title="Marcar en el casco">⌖ ${esc(con?.title ?? p.console)} <small>${esc(this.place(p.c))}</small></button>`;
      })
      .join('');
    return `<article class="mn-card" data-card="${c.index}" id="mc-${c.index}" data-search="${esc(`${c.name} ${c.label} ${c.help} ${c.key}`.toLowerCase())}">
      <header><span class="mn-label">${esc(c.label || '·')}</span><b>${esc(c.name)}</b><span class="mn-state" data-state></span></header>
      <p>${esc(c.help)}</p>
      ${positions}
      <p class="mn-why" data-why></p>
      <footer><span class="mn-tag kind">${KIND[c.kind]}${momentary ? ' · momentáneo' : ''}</span>${guard}${needs}<span class="mn-where">${where}</span></footer>
    </article>`;
  }

  // ------------------------------------------------------------------------------------------------
  // Figures
  // ------------------------------------------------------------------------------------------------

  private figure(f: ManualFig): string {
    switch (f) {
      case 'plan':
        return this.plan();
      case 'circuits':
        return this.circuits();
      case 'alerts':
        return this.alertList();
      case 'pages':
        return this.pages();
      case 'consoles':
        return this.consoles();
      case 'equipment':
        return this.equipment();
      case 'air':
        return this.airFigure();
      case 'power':
        return this.powerFigure();
      default:
        return FIGURES[f] ?? '';
    }
  }

  /**
   * Which deck a ship-space point is on: the deck of the room it stands in (a machine near the
   * ceiling of the lower deck is still on the lower deck), else by height.
   */
  private deckIndex(p: V3) {
    const zone = zoneAt(this.def, [p[0], p[1] - 0.8, p[2]]) ?? zoneAt(this.def, p);
    return deckOf(this.def.decks, zone ? zone.min[1] : p[1]);
  }

  /**
   * Top view of the ship, one map per deck (bottom deck last, as seen from above): hull sections,
   * rooms, machines, seats, deck hatches, and every console as a button.
   */
  private plan() {
    const def = this.def;
    const b = def.bounds;
    const W = 720;
    const pad = 16;
    const k = (W - pad * 2) / (b.max[2] - b.min[2]);
    const multi = def.decks.length > 1;
    const head = multi ? 18 : 0;
    const bandH = Math.round((b.max[0] - b.min[0]) * k + pad + head);
    const order = def.decks.map((_, i) => i).reverse();
    const H = bandH * def.decks.length + pad + 28;
    const X = (z: number) => pad + (z - b.min[2]) * k;
    const Y = (x: number, deck: number) => pad + head + order.indexOf(deck) * bandH + (x - b.min[0]) * k;
    const box = (c: V3, half: V3, yaw: number, deck: number, cls: string, title: string) => {
      const w = half[2] * 2 * k;
      const h = half[0] * 2 * k;
      return `<rect class="${cls}" x="${(X(c[2]) - w / 2).toFixed(1)}" y="${(Y(c[0], deck) - h / 2).toFixed(1)}" width="${w.toFixed(1)}" height="${h.toFixed(1)}" rx="2" transform="rotate(${((-yaw * 180) / Math.PI).toFixed(1)} ${X(c[2]).toFixed(1)} ${Y(c[0], deck).toFixed(1)})"><title>${esc(title)}</title></rect>`;
    };
    let svg = '';
    def.decks.forEach((d, di) => {
      if (multi) svg += `<text class="pl-deck" x="${pad}" y="${(Y(b.min[0], di) - 6).toFixed(1)}">${esc(d.label)}</text>`;
      const labelled = new Set<string>();
      for (const m of def.modules) {
        if (deckOf(def.decks, moduleBase(m) + 0.1) !== di) continue;
        const hw = Math.max(...m.profile.map((p) => Math.abs(p[0])));
        svg += `<rect class="pl-hull" x="${X(m.z0)}" y="${Y(-hw, di)}" width="${(m.z1 - m.z0) * k}" height="${2 * hw * k}" rx="4"/>`;
        const zone = def.zones.find((z) => z.id === m.zone);
        if (zone && !labelled.has(zone.id)) {
          labelled.add(zone.id);
          svg += `<text class="pl-zone" x="${X((zone.min[2] + zone.max[2]) / 2)}" y="${Y(hw, di) - 8}" text-anchor="middle">${esc(zone.label)}</text>`;
        }
      }
      // rooms with no hull section of their own (nested in a bigger one: an airlock in an engine room)
      for (const z of def.zones) {
        if (def.modules.some((m) => m.zone === z.id) || deckOf(def.decks, z.min[1]) !== di) continue;
        svg += `<rect class="pl-room" x="${X(z.min[2])}" y="${Y(z.min[0], di)}" width="${(z.max[2] - z.min[2]) * k}" height="${(z.max[0] - z.min[0]) * k}" rx="2"/>`;
        svg += `<text class="pl-zone" x="${X((z.min[2] + z.max[2]) / 2)}" y="${Y((z.min[0] + z.max[0]) / 2, di) + 4}" text-anchor="middle">${esc(z.label)}</text>`;
      }
    });
    for (const h of def.hatches) {
      // the hole shows on both decks it joins
      for (const di of new Set([deckOf(def.decks, h.c[1] - 0.5), deckOf(def.decks, h.c[1])])) svg += box(h.c, [h.w / 2, 0, h.l / 2], 0, di, 'pl-hatch', 'Escotilla de cubierta');
    }
    for (const p of def.parts) svg += box(p.c, p.half, p.yaw, this.deckIndex(p.c), p.zone ? 'pl-part in' : 'pl-part', p.name);
    for (const s of def.seats) svg += `<rect class="pl-seat" x="${X(s.root[2]) - 7}" y="${Y(s.root[0], this.deckIndex([s.root[0], s.root[1] + 1, s.root[2]])) - 7}" width="14" height="14" rx="3"><title>${esc(s.name)}</title></rect>`;
    for (const con of def.consoles) {
      svg += `<g class="pl-con" data-goto="${esc(con.id)}"><circle cx="${X(con.c[2]).toFixed(1)}" cy="${Y(con.c[0], this.deckIndex(con.c)).toFixed(1)}" r="7"/><title>${esc(con.title)}</title></g>`;
    }
    svg += `<g class="pl-me" data-me><circle r="6"/><circle r="11" class="ring"/></g>`;
    svg += `<text class="pl-axis" x="${pad}" y="${H - 8}">◀ PROA</text><text class="pl-axis" x="${W - pad}" y="${H - 8}" text-anchor="end">POPA ▶</text>`;
    this.planMap = { X, Y };
    return `<figure class="mn-fig mn-plan"><svg viewBox="0 0 ${W} ${H}">${svg}</svg>
      <figcaption><span class="dot con"></span>consola <span class="dot part"></span>máquina <span class="dot seat"></span>asiento <span class="dot me"></span>tú</figcaption></figure>`;
  }

  private planMap: { X: (z: number) => number; Y: (x: number, deck: number) => number } | null = null;

  /** Every machine aboard: its catalog component, size, maker and data sheet. */
  private equipment() {
    const rows = this.def.parts
      .map((p) => {
        const sheet = componentSheet(p)
          .map(([k, v]) => `<span class="mn-spec"><small>${esc(k)}</small> ${esc(v)}</span>`)
          .join('');
        return `<tr data-search="${esc(`${p.name} ${p.component ?? ''} ${maker(p.maker).name}`.toLowerCase())}"><td><b>${esc(p.name)}</b><small>${esc(p.component ?? p.type)}</small></td><td class="num">${esc(p.size ?? '—')}</td><td>${esc(maker(p.maker).name)}</td><td>${sheet}</td></tr>`;
      })
      .join('');
    return `<figure class="mn-fig"><table class="mn-table"><thead><tr><th>Máquina</th><th>Talla</th><th>Fabricante</th><th>Datos</th></tr></thead><tbody>${rows}</tbody></table></figure>`;
  }

  /** Side cut of the ship from its own hull sections: compartments, doors, the hatch or ramp. */
  private airFigure() {
    const def = this.def;
    const W = 560;
    const z0 = Math.min(...def.modules.map((m) => m.z0));
    const z1 = Math.max(...def.modules.map((m) => m.z1));
    const top = Math.max(...def.modules.map((m) => Math.max(...m.profile.map((p) => p[1]))));
    const k = (W - 60) / (z1 - z0);
    const X = (z: number) => 30 + (z - z0) * k;
    const floor = 130;
    const Y = (y: number) => floor - y * k;
    let svg = '';
    for (const m of def.modules) {
      const h = Math.max(...m.profile.map((p) => p[1]));
      const base = moduleBase(m);
      svg += `<rect class="hull" x="${X(m.z0).toFixed(1)}" y="${Y(h).toFixed(1)}" width="${((m.z1 - m.z0) * k).toFixed(1)}" height="${((h - base) * k).toFixed(1)}" rx="3"/>`;
    }
    for (const c of def.compartments) {
      const zs = def.modules.filter((m) => m.zone === c.id);
      if (!zs.length) continue;
      const zc = (Math.min(...zs.map((m) => m.z0)) + Math.max(...zs.map((m) => m.z1))) / 2;
      const base = Math.min(...zs.map(moduleBase));
      svg += `<text class="lbl" x="${X(zc).toFixed(1)}" y="${(Y(base) - 12).toFixed(1)}" text-anchor="middle">${esc(c.label)}</text>`;
    }
    for (const d of def.doors) {
      if (Math.abs(d.n[2]) > 0.5) svg += `<g class="door"><rect x="${(X(d.c[2]) - 4).toFixed(1)}" y="${Y(d.c[1] + d.h).toFixed(1)}" width="8" height="${(d.h * k).toFixed(1)}" rx="1"/></g>`;
      else svg += `<g class="door"><rect x="${(X(d.c[2] - d.w / 2)).toFixed(1)}" y="${(Y(d.c[1] + d.h) - 2).toFixed(1)}" width="${(d.w * k).toFixed(1)}" height="${(d.h * k).toFixed(1)}" rx="2" opacity="0.6"/></g>`;
    }
    // deck hatches: a slot in the deck between the two decks
    for (const h of def.hatches) svg += `<g class="door"><rect x="${X(h.c[2] - h.l / 2).toFixed(1)}" y="${(Y(h.c[1]) - 2).toFixed(1)}" width="${(h.l * k).toFixed(1)}" height="4" rx="1"/></g>`;
    const last = def.compartments.find((c) => def.openings.some((o) => o.a === c.id && o.b === null && (o.kind === 'ramp' || o.kind === 'door'))) ?? def.compartments[def.compartments.length - 1];
    const lz = def.modules.filter((m) => m.zone === last?.id);
    if (last && lz.length) svg += `<text class="kpa" x="${X((lz[0].z0 + lz[lz.length - 1].z1) / 2).toFixed(1)}" y="${(Y(top) + 30).toFixed(1)}" text-anchor="middle">0 kPa</text>`;
    if (def.ramp) {
      const zr = def.ramp.hinge[2];
      svg += `<g class="ramp-open"><line x1="${X(zr)}" y1="${floor}" x2="${X(zr) + 50}" y2="${floor + 30}"/></g><g class="ramp-shut"><line x1="${X(zr)}" y1="${Y(def.ramp.length)}" x2="${X(zr)}" y2="${floor}"/></g>`;
    }
    for (const leg of def.gear?.legs ?? []) if (leg[0] > 0) svg += `<g class="gear"><line x1="${X(leg[2])}" y1="${floor}" x2="${X(leg[2])}" y2="${floor + 22}"/><circle cx="${X(leg[2])}" cy="${floor + 27}" r="6"/></g>`;
    return `<figure class="mn-fig man-fig"><svg viewBox="0 0 ${W} ${floor + 40}" role="img" aria-label="Corte de la nave por compartimentos">${svg}</svg>
      <button type="button" class="act" data-act="air">Cerrar y presurizar</button></figure>`;
  }

  /** Sources on the left (whatever this ship carries), the bus, priorities on the right. */
  private powerFigure() {
    const sys = this.sim.sys;
    const names = [
      ...sys.modules.filter((m): m is Reactor => m.id.startsWith('reactor:')).map(() => 'REACTOR'),
      ...sys.modules.filter((m) => m.id.startsWith('solar:')).map(() => 'SOLAR'),
      ...sys.modules.filter((m) => m.id.startsWith('apu:')).map(() => 'APU'),
      ...(sys.power?.batteries.length ? ['BATERÍA'] : []),
    ].filter((n, i, a) => a.indexOf(n) === i);
    const src = names.map((n, i) => `<g class="src"><rect x="16" y="${18 + i * 40}" width="92" height="28" rx="4"/><text class="cap" x="62" y="${37 + i * 40}" text-anchor="middle">${n}</text></g>`).join('');
    const H = Math.max(150, 30 + names.length * 40);
    const mid = 18 + ((names.length - 1) * 40) / 2 + 14;
    const bus = names.map((_, i) => `M108 ${32 + i * 40} H150`).join(' ');
    return `<figure class="mn-fig man-fig"><svg viewBox="0 0 580 ${H}" role="img" aria-label="Fuentes de energía y prioridades">
      ${src}<path class="bus" d="${bus} M150 32 V${32 + (names.length - 1) * 40} M150 ${mid} H210"/>
      <rect class="busbar" x="210" y="${mid - 48}" width="16" height="96" rx="2"/>
      <g class="brk hi"><line x1="226" y1="${mid - 30}" x2="300" y2="${mid - 30}"/><rect x="300" y="${mid - 44}" width="70" height="28" rx="3"/><text class="cap" x="335" y="${mid - 26}" text-anchor="middle">ALTA</text></g>
      <g class="brk"><line x1="226" y1="${mid}" x2="300" y2="${mid}"/><rect x="300" y="${mid - 14}" width="70" height="28" rx="3"/><text class="cap" x="335" y="${mid + 4}" text-anchor="middle">NORMAL</text></g>
      <g class="brk lo"><line x1="226" y1="${mid + 30}" x2="300" y2="${mid + 30}"/><rect x="300" y="${mid + 16}" width="70" height="28" rx="3"/><text class="cap" x="335" y="${mid + 34}" text-anchor="middle">BAJA</text></g>
      <text class="cap dim" x="390" y="${mid - 4}">si falta energía,</text><text class="cap dim" x="390" y="${mid + 12}">se corta la BAJA primero</text>
    </svg></figure>`;
  }

  private circuits() {
    const def = this.def;
    if (!def.subsystems.length) return '';
    const rows = def.subsystems
      .map(
        (c) => `<tr data-ckt="${esc(c.id)}" data-search="${esc(`${c.label} ${c.desc ?? ''}`.toLowerCase())}">
        <td><i class="swatch" style="background:#${c.color.toString(16).padStart(6, '0')}"></i><b>${esc(c.label)}</b><small>${esc(c.desc ?? '')}</small></td>
        <td class="num">${c.rating} kW</td><td class="num" data-kw></td><td data-st></td><td data-pri></td></tr>`,
      )
      .join('');
    return `<figure class="mn-fig"><table class="mn-table"><thead><tr><th>Circuito</th><th>Límite</th><th>Ahora</th><th>Estado</th><th>Prioridad</th></tr></thead><tbody>${rows}</tbody></table></figure>`;
  }

  private alertList() {
    const alerts = this.sim.sys.alerts;
    const lamps = this.def.annunciator?.lamps ?? [...new Set(alerts.map((a) => a.lamp))];
    const groups = lamps.map((lamp) => ({ lamp, list: alerts.filter((a) => a.lamp === lamp) })).filter((g) => g.list.length);
    return `<div class="mn-alerts">${groups
      .map((g) => {
        // per-compartment / per-tank copies of one alarm share their help: show it once
        const byHelp = new Map<string, typeof g.list>();
        for (const a of g.list) byHelp.set(a.help ?? a.label, [...(byHelp.get(a.help ?? a.label) ?? []), a]);
        const items = [...byHelp.values()]
          .map((same) => {
            const a = same[0];
            const label = same.length > 1 ? `${a.label.split(' · ')[0]} · ${same.map((x) => x.label.split(' · ')[1] ?? '').filter(Boolean).join(', ')}` : a.label;
            return `<div class="mn-alert lv${a.level}"><b>${esc(label)}</b><p>${esc(a.help ?? '')}</p></div>`;
          })
          .join('');
        return `<div class="mn-lampgrp" data-lamp="${esc(g.lamp)}" data-search="${esc(`${g.lamp} ${g.list.map((a) => `${a.label} ${a.help ?? ''}`).join(' ')}`.toLowerCase())}">
          <span class="mn-lamp lv${Math.max(...g.list.map((a) => a.level))}">${esc(g.lamp)}</span><div>${items}</div></div>`;
      })
      .join('')}</div>`;
  }

  private pages() {
    const used = new Map<string, string[]>();
    for (const s of this.def.screens) {
      const con = this.consoleOf(this.def.controls.find((c) => c.key === s.id)?.console ?? '');
      for (const p of s.pages) used.set(p, [...(used.get(p) ?? []), con?.title ?? s.id]);
    }
    return `<div class="mn-pages">${[...used]
      .map(([p, where]) => `<div class="mn-page" data-search="${esc(`${pageLabel(p)} ${SCREEN_PAGES[p]?.help ?? ''}`.toLowerCase())}"><span class="mn-label">${esc(pageLabel(p))}</span><p>${esc(SCREEN_PAGES[p]?.help ?? '')}</p><small>${esc([...new Set(where)].join(' · '))}</small></div>`)
      .join('')}</div>`;
  }

  /** Every console, grouped by where it is, with all its controls. */
  private consoles() {
    const def = this.def;
    const groups = new Map<string, typeof def.consoles>();
    for (const con of def.consoles) {
      const z = this.place(con.c);
      groups.set(z, [...(groups.get(z) ?? []), con]);
    }
    let html = '';
    for (const [zone, list] of groups) {
      html += `<h3 class="mn-zone">${esc(zone)}</h3>`;
      for (const con of list) {
        const ctls = def.controls.filter((c) => c.console === con.id && c.kind !== 'cover' && (c.kind !== 'bezel' || c.command));
        const screens = def.screens.filter((s) => s.console === con.id || def.controls.some((c) => c.console === con.id && c.key === s.id));
        const scr = screens
          .map((s) => `<div class="mn-screen"><b>${s.camera ? 'Cámara' : 'Pantalla'}</b>${s.camera ? `<span>${esc(s.camera.source.kind)} · ${s.camera.width ?? 256}×${s.camera.height ?? 144}${s.seat ? ' · requiere asiento ocupado' : ''}</span>` : s.pages.map((p, i) => `<span data-screen="${esc(s.id)}" data-page="${i}" title="${esc(SCREEN_PAGES[p]?.help ?? '')}">${esc(pageLabel(p))}</span>`).join('')}</div>`)
          .join('');
        html += `<div class="mn-console" id="con-${esc(con.id)}"><header><b>${esc(con.title)}</b><small>${ctls.length} mandos${screens.length ? ` · ${screens.length} pantalla${screens.length > 1 ? 's' : ''}` : ''}</small></header>${scr}<div class="mn-cards">${ctls.map((c) => this.card(c)).join('')}</div></div>`;
      }
    }
    return html;
  }

  // ------------------------------------------------------------------------------------------------
  // Interaction
  // ------------------------------------------------------------------------------------------------

  private onClick(ev: MouseEvent) {
    const t = ev.target as HTMLElement;
    const act = t.closest('[data-act]') as HTMLElement | null;
    if (act?.dataset.act && act.dataset.act !== 'close') {
      const fig = act.closest('.mn-fig') as HTMLElement | null;
      if (fig) figureAction(fig, act);
      return;
    }
    const jump = t.closest('[data-jump]') as HTMLElement | null;
    if (jump?.dataset.jump) {
      const c = this.def.controls.find((x) => x.id === jump.dataset.jump);
      if (c) this.reveal(document.getElementById(`mc-${c.index}`));
      return;
    }
    const pt = t.closest('[data-point]') as HTMLElement | null;
    if (pt?.dataset.point) {
      const i = Number(pt.dataset.point);
      this.pointed = this.pointed === i ? null : i;
      this.hooks.point(this.pointed);
      this.markPointed();
      return;
    }
    const go = t.closest('[data-goto]') as HTMLElement | null;
    if (go?.dataset.goto) {
      this.reveal(document.getElementById(`con-${go.dataset.goto}`), 'start');
      return;
    }
    const toc = t.closest('[data-toc]') as HTMLElement | null;
    if (toc?.dataset.toc) {
      this.search.value = '';
      this.filter();
      document.getElementById(`mn-${toc.dataset.toc}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    }
  }

  /** The helmet reached the pointed control (or it was cleared from outside). */
  clearPoint() {
    this.pointed = null;
    this.markPointed();
  }

  private markPointed() {
    this.body.querySelectorAll<HTMLElement>('[data-point]').forEach((b) => b.classList.toggle('on', Number(b.dataset.point) === this.pointed));
  }

  private reveal(el: HTMLElement | null, block: ScrollLogicalPosition = 'center') {
    if (!el) return;
    if (this.search.value) {
      this.search.value = '';
      this.filter();
    }
    el.scrollIntoView({ block, behavior: 'smooth' });
    el.classList.remove('flash');
    void el.offsetWidth;
    el.classList.add('flash');
  }

  private filter() {
    const q = this.search.value.trim().toLowerCase();
    this.root.classList.toggle('searching', !!q);
    let n = 0;
    this.body.querySelectorAll<HTMLElement>('[data-search]').forEach((el) => {
      const hit = !q || q.split(/\s+/).every((w) => el.dataset.search!.includes(w));
      el.classList.toggle('miss', !hit);
      if (q && hit) n++;
    });
    // consoles / sections left empty by the search fold away
    this.body.querySelectorAll<HTMLElement>('.mn-console, .mn-sec').forEach((box) => {
      box.classList.toggle('empty', !!q && !box.querySelector('[data-search]:not(.miss)'));
    });
    this.count.textContent = q ? `${n} resultado${n === 1 ? '' : 's'}` : '';
  }

  /** Highlight the chapter being read in the index. */
  private spy() {
    const top = this.body.getBoundingClientRect().top + 60;
    let current = '';
    this.body.querySelectorAll<HTMLElement>('.mn-sec').forEach((s) => {
      if (s.getBoundingClientRect().top <= top) current = s.dataset.sec!;
    });
    if (!current) current = this.def.manual[0]?.id ?? '';
    this.nav.querySelectorAll<HTMLElement>('[data-toc]').forEach((b) => b.classList.toggle('on', b.dataset.toc === current));
  }

  // ------------------------------------------------------------------------------------------------
  // Live
  // ------------------------------------------------------------------------------------------------

  /** Refresh the live parts (call every frame while open; it throttles itself). */
  update(now: number) {
    if (now - this.last < 0.25) return;
    this.last = now;
    const sim = this.sim;
    const n = (name: string) => (sim.vars.has(name) ? sim.st[sim.vars.idx(name)] : 0);
    for (const { c, state, why, pos, card } of this.liveEls.ctl) {
      const v = Math.round(sim.sw[c.key] ?? 0);
      const lost = (c.host >= 0 && sim.hole(c.host)) || (c.hostPart >= 0 && sim.partHp(c.hostPart) <= 0);
      const reason = lost ? null : sim.blocked(c);
      const momentary = c.action === 'pulse' || c.action === 'reset';
      if (state) {
        state.textContent = lost ? 'DESTRUIDO' : momentary ? (c.key === this.def.caution ? c.states[v] : 'LISTO') : c.states[v] || '—';
        state.className = `mn-state ${lost ? 'bad' : reason ? 'warn' : v ? 'on' : 'off'}`;
      }
      if (why) why.textContent = lost ? 'La consola está destruida: suelda el panel donde iba montada.' : reason ? `Ahora mismo se negaría: ${reason}` : '';
      pos.forEach((p) => p.classList.toggle('on', Number(p.dataset.pos) === v));
      card.classList.toggle('lost', lost);
    }
    // screens: current page
    this.body.querySelectorAll<HTMLElement>('[data-screen]').forEach((el) => el.classList.toggle('on', Math.round(sim.sw[el.dataset.screen!] ?? 0) === Number(el.dataset.page)));
    // circuits
    this.body.querySelectorAll<HTMLElement>('[data-ckt]').forEach((row) => {
      const c = this.def.subsystems.find((x) => x.id === row.dataset.ckt)!;
      const cut = sim.conduitCut(c.id);
      const live = n(`ckt.${c.id}.f`) >= 0.5 && !cut;
      const st = row.querySelector<HTMLElement>('[data-st]')!;
      st.textContent = live ? 'CON TENSIÓN' : sim.sw[c.breaker] !== 1 ? 'DISYUNTOR ABIERTO' : cut ? `CORTE ${cut.id}` : 'SIN ALIMENTAR';
      st.className = live ? 'ok' : 'bad';
      row.querySelector<HTMLElement>('[data-kw]')!.textContent = `${n(`ckt.${c.id}.kw`).toFixed(1)} kW`;
      row.querySelector<HTMLElement>('[data-pri]')!.textContent = ['ALTA', 'NORMAL', 'BAJA'][Math.round(sim.sw[c.priority] ?? 1)] ?? '';
    });
    // alarms lit now
    const active = new Set(sim.sys.active(sim.st).map((a) => a.lamp));
    this.body.querySelectorAll<HTMLElement>('[data-lamp]').forEach((g) => g.classList.toggle('lit', active.has(g.dataset.lamp!)));
    // live strips
    this.body.querySelectorAll<HTMLElement>('[data-live]').forEach((el) => (el.innerHTML = this.live(el.dataset.live as ManualLive, n)));
    // you on the plan
    const me = this.body.querySelector<SVGGElement>('[data-me]');
    const at = this.hooks.where();
    if (me && this.planMap) {
      me.style.display = at ? '' : 'none';
      if (at) me.setAttribute('transform', `translate(${this.planMap.X(at[2]).toFixed(1)} ${this.planMap.Y(at[0], this.deckIndex(at)).toFixed(1)})`);
    }
  }

  private live(kind: ManualLive, n: (name: string) => number): string {
    const sim = this.sim;
    const chip = (label: string, value: string, tone = '') => `<span class="mn-chip ${tone}"><small>${esc(label)}</small><b>${esc(value)}</b></span>`;
    const bar = (label: string, v: number, text: string, tone = '') => `<span class="mn-bar ${tone}"><small>${esc(label)}</small><i><em style="width:${Math.round(Math.max(0, Math.min(1, v)) * 100)}%"></em></i><b>${esc(text)}</b></span>`;
    const tag = '<span class="mn-livetag">EN VIVO</span>';
    if (kind === 'power') {
      if (!sim.sys.power) return '';
      return `${tag}${chip('Generación', `${n('pwr.gen').toFixed(1)} kW`)}${chip('Consumo', `${n('pwr.load').toFixed(1)} kW`)}${bar('Batería', n('bat.soc'), `${Math.round(n('bat.soc') * 100)} %`, n('bat.soc') < 0.2 ? 'bad' : '')}${chip('Deslastre', n('pwr.shed') ? 'SÍ' : 'NO', n('pwr.shed') ? 'warn' : 'ok')}`;
    }
    if (kind === 'reactor') {
      const rx = sim.sys.modules.find((m): m is Reactor => m.id.startsWith('reactor:'));
      if (!rx) return '';
      const s = Math.round(n(`${rx.tag}.state`));
      const t = n(`${rx.tag}.temp`);
      return `${tag}${chip('Reactor', RX_NAME[s] ?? '?', s === RX.online ? 'ok' : s === RX.scram ? 'bad' : 'warn')}${bar('Núcleo', t / rx.k.damageC, `${Math.round(t)} °C`, t > rx.k.warnC ? 'bad' : '')}${chip('Salida', `${Math.round(n(`${rx.tag}.out`) * 100)} %`)}${chip('Refrigerante', `${Math.round(n(`${rx.tag}.cool.flow`) * 100)} %`, n(`${rx.tag}.cool.flow`) < 0.5 ? 'bad' : 'ok')}`;
    }
    if (kind === 'air') {
      return `${tag}${this.def.compartments.map((c) => bar(c.label, n(`${c.id}.p`) / 70, `${n(`${c.id}.p`).toFixed(0)} kPa`, n(`${c.id}.p`) < 50 ? 'warn' : 'ok')).join('')}`;
    }
    if (kind === 'stores') {
      if (!sim.vars.has('stores.water')) return '';
      const w = n('stores.water');
      const f = n('stores.food');
      return `${tag}${bar('Agua', w / Math.max(1, n('stores.water.cap')), `${w.toFixed(1)} kg`, w < n('stores.water.cap') * 0.15 ? 'bad' : '')}${bar('Víveres', f / Math.max(1, n('stores.food.cap')), `${f.toFixed(1)} kg`, f < n('stores.food.cap') * 0.15 ? 'bad' : '')}${chip('A bordo', `${Math.round(n('stores.aboard'))}`)}${chip('Reciclado', `${(n('stores.recycle') * 3600).toFixed(2)} kg/h`, n('stores.recycle') > 0 ? 'ok' : 'warn')}`;
    }
    if (kind === 'airlock') {
      const a = this.def.airlock;
      if (!a) return '';
      const ph = Math.round(n('lock.phase'));
      return `${tag}${chip('Esclusa', LOCK_NAME[ph] ?? '?', ph === LOCK.in ? 'ok' : ph === LOCK.out ? 'bad' : 'warn')}${bar('Presión', n(`${a.zone}.p`) / 70, `${n(`${a.zone}.p`).toFixed(0)} kPa`)}`;
    }
    if (kind === 'flight') {
      const m = sim.massNow();
      const perf = performance(this.def, m, MOON.gravity);
      const g = this.hooks.surfaces;
      if (g) {
        const r = flightReadout(sim, g);
        const deg = (x: number) => (((Math.round((x * 180) / Math.PI) % 360) + 360) % 360);
        const air = !r.landed;
        return `${tag}${chip('Estado', r.landed ? 'EN TIERRA' : 'EN VUELO', air ? 'warn' : 'ok')}${chip('Altura', `${r.agl.toFixed(1)} m`)}${chip('V/S', `${r.vs.toFixed(1)} m/s`, r.vs < -3 ? 'bad' : '')}${chip('Suelo', `${r.gs.toFixed(1)} m/s`)}${chip('Rumbo', `${String(deg(r.heading)).padStart(3, '0')}°`)}${chip('Mando', r.direct ? 'DIRECTO' : 'ASISTIDO', r.direct ? 'bad' : 'ok')}${chip('P. aut.', r.apOn ? (r.modes.join(' · ') || 'ON') : 'OFF', r.apOn ? 'ok' : '')}${bar('Empuje', r.weight > 0 ? r.thrust / r.weight / 2 : 0, `${r.weight > 0 ? Math.round((r.thrust / r.weight) * 100) : 0} % del peso`)}${chip('Masa', `${(m.mass / 1000).toFixed(2)} t`)}${chip('Empuje/peso', perf.twr.toFixed(2), perf.twr > 1 ? 'ok' : 'warn')}${chip('Δv', `${perf.dv.toFixed(0)} m/s`)}`;
      }
      return `${tag}${chip('Masa', `${(m.mass / 1000).toFixed(2)} t`)}${chip('Empuje/peso', perf.twr.toFixed(2), perf.twr > 1 ? 'ok' : 'warn')}${chip('Δv', `${perf.dv.toFixed(0)} m/s`)}`;
    }
    const fuel = sim.sys.fuel;
    if (!fuel) return '';
    return `${tag}${fuel.tanks.map((t) => bar(t.name.replace('Depósito ', ''), n(`${t.id}.kg`) / t.p.cap, `${Math.round(n(`${t.id}.kg`))} kg`, n(`${t.id}.leak`) > 0.05 ? 'bad' : '')).join('')}`;
  }
}

function figureAction(fig: HTMLElement, act: HTMLElement) {
  if (act.dataset.act === 'cover') {
    const on = fig.classList.toggle('open');
    act.textContent = on ? 'Cerrar la tapa' : 'Abrir la tapa';
  } else if (act.dataset.act === 'air') {
    const label = fig.querySelector('.kpa');
    const on = fig.classList.toggle('sealed');
    act.textContent = on ? 'Abrir al vacío' : 'Cerrar y presurizar';
    window.clearInterval(Number(fig.dataset.timer));
    if (!on) {
      if (label) label.textContent = '0 kPa';
      return;
    }
    let k = 0;
    fig.dataset.timer = String(
      window.setInterval(() => {
        k = Math.min(70, k + 5);
        if (label) label.textContent = `${k} kPa`;
        if (k >= 70) window.clearInterval(Number(fig.dataset.timer));
      }, 60),
    );
  } else if (act.dataset.act === 'drive') {
    const steps = fig.querySelectorAll('.step').length || 3;
    const step = ((Number(fig.dataset.step) || 0) % steps) + 1;
    fig.dataset.step = String(step);
    act.textContent = step === steps ? 'Desde el principio' : 'Siguiente paso';
  }
}

/** Hand-drawn diagrams (the generated ones are built from the ship above). */
const FIGURES: Partial<Record<ManualFig, string>> = {
  cover: `<figure class="mn-fig man-fig">
    <svg viewBox="0 0 420 150" role="img" aria-label="Tapa de seguridad sobre un interruptor">
      <rect class="plate" x="16" y="16" width="230" height="118" rx="8"/>
      <text class="cap" x="28" y="36">CONSOLA</text>
      <g class="sw"><rect class="btn" x="92" y="62" width="52" height="40" rx="4"/><text class="cap" x="118" y="86" text-anchor="middle">ARM</text></g>
      <g class="lid"><rect x="78" y="52" width="80" height="60" rx="3"/><text class="cap" x="118" y="86" text-anchor="middle">TAPA</text></g>
      <text class="cap note-shut" x="262" y="58">Cerrada: el clic</text>
      <text class="cap note-shut" x="262" y="76">cae en la tapa.</text>
      <text class="cap note-open" x="262" y="108">Abierta: el mando</text>
      <text class="cap note-open" x="262" y="126">de dentro queda libre.</text>
    </svg>
    <button type="button" class="act" data-act="cover">Abrir la tapa</button>
  </figure>`,
  leds: `<figure class="mn-fig mn-leds">
    <div><i class="led g"></i><b>Verde</b><span>encendido y funcionando</span></div>
    <div><i class="led a blink"></i><b>Ámbar parpadeando</b><span>en camino (puerta, rampa, tren…)</span></div>
    <div><i class="led a"></i><b>Ámbar fijo</b><span>encendido, pero su circuito no tiene energía</span></div>
    <div><i class="led r"></i><b>Rojo</b><span>disyuntor abierto o reactor parado</span></div>
    <div><i class="led o"></i><b>Naranja intenso</b><span>se ha negado: mira el motivo en el casco</span></div>
    <div><i class="led c blink"></i><b>Cian parpadeando</b><span>el mando que has señalado en el manual</span></div>
  </figure>`,
  airlock: `<figure class="mn-fig man-fig" data-step="0">
    <svg viewBox="0 0 560 130" role="img" aria-label="Ciclo de la esclusa">
      <g class="step s1"><circle cx="70" cy="46" r="22"/><text class="cap" x="70" y="51" text-anchor="middle">1</text><text class="lbl" x="70" y="92" text-anchor="middle">CIERRA PUERTA</text></g>
      <g class="step s2"><circle cx="210" cy="46" r="22"/><text class="cap" x="210" y="51" text-anchor="middle">2</text><text class="lbl" x="210" y="92" text-anchor="middle">VACÍA / LLENA</text></g>
      <g class="step s3"><circle cx="350" cy="46" r="22"/><text class="cap" x="350" y="51" text-anchor="middle">3</text><text class="lbl" x="350" y="92" text-anchor="middle">IGUALA PRESIÓN</text></g>
      <g class="step s4"><circle cx="490" cy="46" r="22"/><text class="cap" x="490" y="51" text-anchor="middle">4</text><text class="lbl" x="490" y="92" text-anchor="middle">ABRE LA OTRA</text></g>
      <path class="bus" d="M96 46 H184 M236 46 H324 M376 46 H464"/>
    </svg>
    <button type="button" class="act" data-act="drive">Siguiente paso</button>
  </figure>`,
  drive: `<figure class="mn-fig man-fig" data-step="0">
    <svg viewBox="0 0 520 120" role="img" aria-label="Secuencia de arranque de un motor">
      <g class="step s1"><circle cx="70" cy="46" r="22"/><text class="cap" x="70" y="51" text-anchor="middle">1</text><text class="lbl" x="70" y="92" text-anchor="middle">ABRIR TAPA</text></g>
      <g class="step s2"><circle cx="230" cy="46" r="22"/><text class="cap" x="230" y="51" text-anchor="middle">2</text><text class="lbl" x="230" y="92" text-anchor="middle">ARMAR</text></g>
      <g class="step s3"><circle cx="390" cy="46" r="22"/><text class="cap" x="390" y="51" text-anchor="middle">3</text><text class="lbl" x="390" y="92" text-anchor="middle">ARRANQUE</text></g>
      <path class="bus" d="M96 46 H200 M256 46 H360"/>
    </svg>
    <button type="button" class="act" data-act="drive">Siguiente paso</button>
  </figure>`,
};

function esc(s: string) {
  return s.replace(/[&<>"']/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[ch]!);
}
