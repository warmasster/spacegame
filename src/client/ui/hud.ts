import * as THREE from 'three';
import { origin } from '../render/origin';
import type { ShipSim } from '../../shared/ship/sim';
import { ShipManual, type ManualHooks } from './manual';
import type { PromptInfo } from '../ship/interaction';
import { WEAPON_LIST } from '../../shared/items';
import type { PipStatus } from '../render/pip';

export interface HudData {
  heading: number; // radians, 0 = north
  position: THREE.Vector3;
  speed: number;
  altitude: number;
  lamps: boolean;
  evaSeconds: number;
  cameraMode: 'first' | 'third';
  online: boolean;
  players: number;
  maxPlayers: number;
  rtt: number;
  fps: number;
  hp: number;
  fuel: number;
  /** Suit oxygen 0..1. */
  o2: number;
  /** The tool in hand (null: put away): its line, its bar (0 = just fired, 1 = ready) and how it shows. */
  tool: { label: string; value: number; state: 'ready' | 'reload' | 'fuel' } | null;
  /** Camera magnification (1 = none). */
  zoom: number;
  dead: boolean;
  station: PipStatus | null;
  markers: Array<{ label: string; bearing: number; distance: number; color: string }>;
}

/** Minimal helmet HUD, drawn in the DOM (crisp at any resolution, zero GPU cost). */
export class Hud {
  readonly root: HTMLDivElement;
  private tape: HTMLDivElement;
  private tapeInner: HTMLDivElement;
  private markerLayer: HTMLDivElement;
  private telemetry: HTMLDivElement;
  private net: HTMLDivElement;
  private help: HTMLDivElement;
  private toasts: HTMLDivElement;
  private tags = new Map<number, HTMLDivElement>();
  private tagLayer: HTMLDivElement;
  private vitals: HTMLDivElement;
  private station: HTMLDivElement;
  private stationTitle: HTMLElement;
  private stationDetail: HTMLElement;
  private stationHint: HTMLElement;
  private deathScreen: HTMLDivElement;
  private hitFlash: HTMLDivElement;
  private prompt: HTMLDivElement;
  private promptKey = '';
  private manual: HTMLDivElement;
  private book: ShipManual | null = null;
  /** The manual opened / closed (the game frees the mouse to read it and takes it back). */
  onManual?: (open: boolean) => void;
  /** True while the ship manual covers the view (the wheel scrolls it instead of zooming). */
  manualOpen = false;
  /**
   * Fixed nodes written with textContent / a style, only when their text changed: no innerHTML and
   * no layout reads per frame (the tape and root sizes are read on resize only).
   */
  private tel: Record<'eva' | 'hdg' | 'vel' | 'alt' | 'pos' | 'lamps' | 'cam', HTMLElement>;
  private bars: Array<{ box: HTMLDivElement; label: HTMLSpanElement; fill: HTMLElement; cls: string; text: string; pct: number }> = [];
  private netDot: HTMLElement;
  private netState: HTMLElement;
  private netCrew: HTMLElement;
  private netRtt: HTMLElement;
  private netFps: HTMLElement;
  private markerEls: Array<{ box: HTMLDivElement; span: HTMLSpanElement; left: number; text: string; color: string; edge: boolean }> = [];
  private tapeW = 0;
  private rootW = 0;
  private rootH = 0;
  private lastHdg = NaN;
  private lastText = -Infinity;
  private tagText = new Map<number, string>();

  constructor(parent: HTMLElement) {
    this.root = el('div', 'hud', parent);
    this.tape = el('div', 'hud-compass', this.root);
    this.tapeInner = el('div', 'hud-compass-inner', this.tape);
    this.markerLayer = el('div', 'hud-compass-markers', this.tape);
    el('div', 'hud-compass-caret', this.tape);
    this.buildTape();
    this.telemetry = el('div', 'hud-telemetry', this.root);
    const row = (parent: HTMLElement, label: string, cls = 'row') => {
      const r = el('div', cls, parent);
      el('span', '', r).textContent = label;
      return el('b', '', r);
    };
    this.tel = {
      eva: row(this.telemetry, 'EVA'),
      hdg: row(this.telemetry, 'RUMBO'),
      vel: row(this.telemetry, 'VEL'),
      alt: row(this.telemetry, 'ALT'),
      pos: row(this.telemetry, 'POS'),
      lamps: row(this.telemetry, 'LUCES'),
      cam: row(this.telemetry, 'CÁM', 'row dim'),
    };
    this.net = el('div', 'hud-net', this.root);
    const r0 = el('div', 'row', this.net);
    this.netDot = el('i', 'dot', r0);
    this.netState = el('b', '', r0);
    this.netCrew = row(this.net, 'TRIPULACIÓN');
    this.netRtt = row(this.net, 'RTT');
    this.netFps = row(this.net, 'FPS', 'row dim');
    const measure = () => {
      this.tapeW = this.tape.clientWidth;
      this.rootW = this.root.clientWidth;
      this.rootH = this.root.clientHeight;
      this.lastHdg = NaN;
    };
    window.addEventListener('resize', measure);
    requestAnimationFrame(measure);
    el('div', 'hud-crosshair', this.root);
    this.prompt = el('div', 'hud-prompt hidden', this.root);
    this.manual = el('div', 'mn hidden', this.root);
    this.manual.addEventListener('click', (ev) => {
      if ((ev.target as HTMLElement).closest('[data-act="close"]')) this.toggleManual(false);
    });
    window.addEventListener('keydown', (e) => {
      if (e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
      const tag = (e.target as HTMLElement | null)?.tagName;
      const typing = tag === 'INPUT' || tag === 'TEXTAREA';
      if (e.code === 'Escape' && this.manualOpen) {
        e.preventDefault();
        this.toggleManual(false);
      } else if (e.code === 'KeyM' && !typing) {
        e.preventDefault();
        this.toggleManual();
      } else if (e.code === 'Slash' && this.manualOpen && !typing) {
        e.preventDefault();
        this.book?.focusSearch();
      }
    });
    this.help = el('div', 'hud-help', this.root);
    this.help.innerHTML = [
      ['W A S D', 'moverse'],
      ['Mayús', 'correr (trote lunar)'],
      ['Espacio', 'saltar'],
      ['C / Ctrl', 'agacharse'],
      ['Clic izq.', 'disparar / soldar · pulsar botón'],
      ['G / T (artillero)', 'apuntar en pantalla / disparar'],
      ['E', 'accionar · sentarse / levantarse · coger / soltar objeto'],
      ['Q', 'lanzar el objeto que llevas'],
      [WEAPON_LIST.map((_, i) => i + 1).join(' / '), WEAPON_LIST.map((w) => w.name.toLowerCase()).join(' / ')],
      ['Rueda sobre un selector', 'girarlo'],
      ['Rueda · Clic der.', 'zoom'],
      ['M', 'manual de la nave'],
      ['X', 'sacar / guardar herramienta'],
      ['Espacio (aire)', 'jetpack'],
      ['L', 'luces del casco'],
      ['V', 'primera / tercera persona'],
      ['Rueda', 'distancia de cámara (3ª)'],
      ['Alt + ratón', 'mirar alrededor (3ª)'],
      ['H', 'ocultar ayuda'],
      ['Esc', 'liberar ratón'],
    ]
      .map(([k, v]) => `<div><kbd>${k}</kbd><span>${v}</span></div>`)
      .join('');
    this.toasts = el('div', 'hud-toasts', this.root);
    this.vitals = el('div', 'hud-vitals', this.root);
    this.station = el('div', 'hud-station hidden', this.root);
    this.stationTitle = el('b', '', this.station);
    this.stationDetail = el('span', '', this.station);
    this.stationHint = el('small', '', this.station);
    for (let k = 0; k < 4; k++) {
      const box = el('div', 'vital', this.vitals);
      const label = el('span', '', box);
      const fill = el('em', '', el('i', '', box));
      this.bars.push({ box, label, fill, cls: '', text: '', pct: -1 });
    }
    this.hitFlash = el('div', 'hud-hitflash', this.root);
    this.deathScreen = el('div', 'hud-death hidden', this.root);
    this.deathScreen.innerHTML = '<b>TRAJE COMPROMETIDO</b><span>Reapareciendo…</span>';
    this.tagLayer = el('div', 'hud-tags', this.root);
  }

  /** Red pulse when taking damage. */
  damage(amount: number) {
    this.hitFlash.style.opacity = String(Math.min(0.85, 0.25 + amount / 80));
    setTimeout(() => (this.hitFlash.style.opacity = '0'), 120);
  }

  /** Ship manual (M), built from the ship itself (see manual.ts). */
  setManual(sim: ShipSim, hooks: ManualHooks) {
    this.book = new ShipManual(this.manual, sim, hooks);
  }

  get manualBook() {
    return this.book;
  }

  /** Called right before the manual opens (the game picks which ship's manual to show). */
  beforeManual?: () => void;

  toggleManual(open = !this.manualOpen) {
    if (open === this.manualOpen) return;
    if (open) this.beforeManual?.();
    this.manualOpen = open;
    this.manual.classList.toggle('hidden', !open);
    if (open) this.book?.update(Infinity);
    this.onManual?.(open);
  }

  /** Every frame: the manual refreshes its live parts while open. */
  updateManual(time: number) {
    if (this.manualOpen) this.book?.update(time);
  }

  /** Wheel while the manual is open: scroll the page instead of the camera. */
  scrollManual(dy: number) {
    this.book?.scroll(dy);
  }

  /** What the crosshair is on (ship control or panel), or null. */
  setPrompt(p: PromptInfo | null) {
    const key = p ? `${p.title}|${p.state}|${p.tone}|${p.hint}|${p.hintTone}|${p.detail ?? ''}|${p.bar === undefined ? '' : Math.round(p.bar * 50)}` : '';
    if (key === this.promptKey) return;
    this.promptKey = key;
    this.prompt.classList.toggle('hidden', !p);
    if (!p) return;
    const bar = p.bar === undefined ? '' : `<i><em class="${p.tone}" style="width:${Math.round(Math.max(0, Math.min(1, p.bar)) * 100)}%"></em></i>`;
    const detail = p.detail ? `<span class="detail">${escapeHtml(p.detail)}</span>` : '';
    this.prompt.innerHTML = `<b>${escapeHtml(p.title)}</b><span class="${p.tone}">${escapeHtml(p.state)}</span>${bar}<small class="${p.hintTone ?? ''}">${escapeHtml(p.hint)}</small>${detail}`;
  }

  toggleHelp() {
    this.help.classList.toggle('hidden');
  }

  /** A line came in (the radio's chirp). */
  onToast: (() => void) | null = null;

  toast(text: string) {
    this.onToast?.();
    const t = el('div', 'hud-toast', this.toasts);
    t.textContent = text;
    setTimeout(() => t.classList.add('out'), 3800);
    setTimeout(() => t.remove(), 4600);
  }

  private buildTape() {
    // 0..360 repeated three times so the strip can scroll seamlessly
    const labels: Record<number, string> = { 0: 'N', 45: 'NE', 90: 'E', 135: 'SE', 180: 'S', 225: 'SO', 270: 'O', 315: 'NO' };
    let html = '';
    for (let rep = 0; rep < 3; rep++) {
      for (let d = 0; d < 360; d += 5) {
        const major = d % 45 === 0;
        const mid = d % 15 === 0;
        html += `<span class="tick ${major ? 'major' : mid ? 'mid' : ''}" style="left:${(rep * 360 + d) * PX_PER_DEG}px">${major ? `<b>${labels[d]}</b>` : mid ? `<i>${d}</i>` : ''}</span>`;
      }
    }
    this.tapeInner.innerHTML = html;
  }

  private bar(k: number, label: string, v: number, cls: string) {
    const b = this.bars[k];
    if (b.cls !== cls) {
      b.cls = cls;
      b.box.className = `vital ${cls}`;
    }
    if (b.text !== label) {
      b.text = label;
      b.label.textContent = label;
    }
    const pct = Math.round(Math.max(0, Math.min(1, v)) * 100);
    if (b.pct !== pct) {
      b.pct = pct;
      b.fill.style.width = `${pct}%`;
    }
  }

  update(d: HudData) {
    const hdg = (THREE.MathUtils.radToDeg(d.heading) + 360) % 360;
    // measured on resize; until the HUD is laid out (width 0) keep asking
    if (!this.tapeW) {
      this.tapeW = this.tape.clientWidth;
      this.rootW = this.root.clientWidth;
      this.rootH = this.root.clientHeight;
    }
    const w = this.tapeW;
    if (Math.abs(hdg - this.lastHdg) > 0.02 || Number.isNaN(this.lastHdg)) {
      this.lastHdg = hdg;
      this.tapeInner.style.transform = `translateX(${(w / 2 - (360 + hdg) * PX_PER_DEG).toFixed(1)}px)`;
    }
    // compass markers: one node each, moved and relabelled only when that changed
    const ms = d.markers;
    while (this.markerEls.length < ms.length) {
      const box = el('div', 'hud-marker', this.markerLayer);
      this.markerEls.push({ box, span: el('span', '', box), left: NaN, text: '', color: '', edge: false });
    }
    for (let k = 0; k < this.markerEls.length; k++) {
      const e = this.markerEls[k];
      const m = ms[k];
      e.box.style.display = m ? '' : 'none';
      if (!m) continue;
      let rel = ((THREE.MathUtils.radToDeg(m.bearing) - hdg + 540) % 360) - 180;
      const clamped = Math.max(-58, Math.min(58, rel));
      const edge = rel !== clamped;
      rel = clamped;
      const left = Math.round(w / 2 + rel * PX_PER_DEG);
      if (left !== e.left) {
        e.left = left;
        e.box.style.left = `${left}px`;
      }
      if (edge !== e.edge) {
        e.edge = edge;
        e.box.classList.toggle('edge', edge);
      }
      if (m.color !== e.color) {
        e.color = m.color;
        e.box.style.setProperty('--c', m.color);
      }
      const text = `${m.label} · ${fmtDist(m.distance)}`;
      if (text !== e.text) {
        e.text = text;
        e.span.textContent = text;
      }
    }
    this.deathScreen.classList.toggle('hidden', !d.dead);

    // text and bars: 10 Hz is plenty
    const now = performance.now();
    if (now - this.lastText < 100) return;
    this.lastText = now;
    this.station.classList.toggle('hidden', !d.station || d.dead);
    if (d.station) {
      setText(this.stationTitle, d.station.title);
      setText(this.stationDetail, d.station.detail);
      setText(this.stationHint, d.station.hint);
      this.station.classList.toggle('warning', d.station.warning);
    }
    const t = Math.floor(d.evaSeconds);
    setText(this.tel.eva, `${pad(Math.floor(t / 3600))}:${pad(Math.floor(t / 60) % 60)}:${pad(t % 60)}`);
    setText(this.tel.hdg, `${pad3(Math.round(hdg) % 360)}°`);
    setText(this.tel.vel, `${d.speed.toFixed(1)} m/s`);
    setText(this.tel.alt, `${d.altitude >= 0 ? '+' : ''}${d.altitude.toFixed(1)} m`);
    setText(this.tel.pos, `${fmtCoord(d.position.x)} ${fmtCoord(-d.position.z)}`);
    setText(this.tel.lamps, d.lamps ? 'ON' : 'OFF');
    this.tel.lamps.classList.toggle('on', d.lamps);
    setText(this.tel.cam, `${d.cameraMode === 'first' ? '1ª persona' : '3ª persona'}${d.zoom > 1.05 ? ` · ×${d.zoom.toFixed(1)}` : ''}`);
    this.bar(0, 'TRAJE', d.hp / 100, d.hp < 35 ? 'crit' : 'hp');
    this.bar(1, 'O2', d.o2, d.o2 < 0.2 ? 'crit' : 'fuel');
    this.bar(2, 'JET', d.fuel, 'fuel');
    if (d.tool) this.bar(3, d.tool.label, d.tool.value, d.tool.state);
    else this.bar(3, 'HERRAMIENTA GUARDADA', 0, 'reload');
    this.netDot.className = `dot ${d.online ? 'ok' : 'bad'}`;
    setText(this.netState, d.online ? 'EN LÍNEA' : 'SIN CONEXIÓN');
    setText(this.netCrew, `${d.players}/${d.maxPlayers}`);
    setText(this.netRtt, `${Math.round(d.rtt)} ms`);
    setText(this.netFps, `${Math.round(d.fps)}`);
  }

  /** Floating name tags for other astronauts. */
  updateTag(id: number, name: string, world: THREE.Vector3, camera: THREE.Camera, color: string) {
    let tag = this.tags.get(id);
    if (!tag) {
      tag = el('div', 'hud-tag', this.tagLayer);
      this.tags.set(id, tag);
    }
    // `world` is in the world, the camera's matrices in render space (render/origin.ts)
    const p = origin.toRender(_tag.copy(world)).project(camera);
    const dist = world.distanceTo(camera.position);
    const visible = p.z < 1 && Math.abs(p.x) < 1.1 && Math.abs(p.y) < 1.1;
    tag.style.display = visible ? 'block' : 'none';
    if (!visible) return;
    const x = (p.x * 0.5 + 0.5) * this.rootW;
    const y = (-p.y * 0.5 + 0.5) * this.rootH;
    tag.style.transform = `translate(${x.toFixed(0)}px, ${y.toFixed(0)}px) translate(-50%, -100%)`;
    const html = `<b>${escapeHtml(name)}</b><span>${fmtDist(dist)}</span>`;
    if (this.tagText.get(id) !== html) {
      this.tagText.set(id, html);
      tag.style.setProperty('--c', color);
      tag.innerHTML = html;
    }
  }

  removeTag(id: number) {
    this.tags.get(id)?.remove();
    this.tags.delete(id);
    this.tagText.delete(id);
  }
}

const PX_PER_DEG = 4;
const _tag = new THREE.Vector3();

function setText(node: HTMLElement, text: string) {
  if (node.textContent !== text) node.textContent = text;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, parent: HTMLElement) {
  const e = document.createElement(tag);
  e.className = cls;
  parent.appendChild(e);
  return e as HTMLElementTagNameMap[K] & HTMLDivElement;
}

const pad = (n: number) => String(n).padStart(2, '0');
const pad3 = (n: number) => String(n).padStart(3, '0');
const fmtCoord = (v: number) => `${v >= 0 ? '+' : '−'}${Math.abs(v).toFixed(0).padStart(4, '0')}`;
const fmtDist = (m: number) => (m < 1000 ? `${Math.round(m)} m` : `${(m / 1000).toFixed(2)} km`);
const escapeHtml = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
