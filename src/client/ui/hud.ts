import * as THREE from 'three';
import type { PromptInfo } from '../ship/interaction';

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
  /** 0 = just fired, 1 = ready. */
  reload: number;
  tool: 'none' | 'launcher' | 'welder' | 'welding';
  /** Camera magnification (1 = none). */
  zoom: number;
  dead: boolean;
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
  private deathScreen: HTMLDivElement;
  private hitFlash: HTMLDivElement;
  private prompt: HTMLDivElement;
  private promptKey = '';

  constructor(parent: HTMLElement) {
    this.root = el('div', 'hud', parent);
    this.tape = el('div', 'hud-compass', this.root);
    this.tapeInner = el('div', 'hud-compass-inner', this.tape);
    this.markerLayer = el('div', 'hud-compass-markers', this.tape);
    el('div', 'hud-compass-caret', this.tape);
    this.buildTape();
    this.telemetry = el('div', 'hud-telemetry', this.root);
    this.net = el('div', 'hud-net', this.root);
    el('div', 'hud-crosshair', this.root);
    this.prompt = el('div', 'hud-prompt hidden', this.root);
    this.help = el('div', 'hud-help', this.root);
    this.help.innerHTML = [
      ['W A S D', 'moverse'],
      ['Mayús', 'correr (trote lunar)'],
      ['Espacio', 'saltar'],
      ['C / Ctrl', 'agacharse'],
      ['Clic izq.', 'disparar / soldar · pulsar botón'],
      ['E', 'accionar · sentarse / levantarse'],
      ['1 / 2', 'lanzacohetes / soldadora'],
      ['Rueda · Clic der.', 'zoom'],
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

  /** What the crosshair is on (ship control or panel), or null. */
  setPrompt(p: PromptInfo | null) {
    const key = p ? `${p.title}|${p.state}|${p.tone}|${p.hint}|${p.hintTone}|${p.bar === undefined ? '' : Math.round(p.bar * 50)}` : '';
    if (key === this.promptKey) return;
    this.promptKey = key;
    this.prompt.classList.toggle('hidden', !p);
    if (!p) return;
    const bar = p.bar === undefined ? '' : `<i><em class="${p.tone}" style="width:${Math.round(Math.max(0, Math.min(1, p.bar)) * 100)}%"></em></i>`;
    this.prompt.innerHTML = `<b>${escapeHtml(p.title)}</b><span class="${p.tone}">${escapeHtml(p.state)}</span>${bar}<small class="${p.hintTone ?? ''}">${escapeHtml(p.hint)}</small>`;
  }

  toggleHelp() {
    this.help.classList.toggle('hidden');
  }

  toast(text: string) {
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

  update(d: HudData) {
    const hdg = (THREE.MathUtils.radToDeg(d.heading) + 360) % 360;
    const w = this.tape.clientWidth;
    this.tapeInner.style.transform = `translateX(${w / 2 - (360 + hdg) * PX_PER_DEG}px)`;
    this.markerLayer.innerHTML = d.markers
      .map((m) => {
        let rel = ((THREE.MathUtils.radToDeg(m.bearing) - hdg + 540) % 360) - 180;
        const clamped = Math.max(-58, Math.min(58, rel));
        const edge = rel !== clamped;
        rel = clamped;
        return `<div class="hud-marker${edge ? ' edge' : ''}" style="left:${w / 2 + rel * PX_PER_DEG}px;--c:${m.color}"><span>${m.label} · ${fmtDist(m.distance)}</span></div>`;
      })
      .join('');

    const t = Math.floor(d.evaSeconds);
    const clock = `${pad(Math.floor(t / 3600))}:${pad(Math.floor(t / 60) % 60)}:${pad(t % 60)}`;
    this.telemetry.innerHTML = `
      <div class="row"><span>EVA</span><b>${clock}</b></div>
      <div class="row"><span>RUMBO</span><b>${pad3(Math.round(hdg) % 360)}°</b></div>
      <div class="row"><span>VEL</span><b>${d.speed.toFixed(1)} m/s</b></div>
      <div class="row"><span>ALT</span><b>${d.altitude >= 0 ? '+' : ''}${d.altitude.toFixed(1)} m</b></div>
      <div class="row"><span>POS</span><b>${fmtCoord(d.position.x)} ${fmtCoord(-d.position.z)}</b></div>
      <div class="row"><span>LUCES</span><b class="${d.lamps ? 'on' : ''}">${d.lamps ? 'ON' : 'OFF'}</b></div>
      <div class="row dim"><span>CÁM</span><b>${d.cameraMode === 'first' ? '1ª persona' : '3ª persona'}${d.zoom > 1.05 ? ` · ×${d.zoom.toFixed(1)}` : ''}</b></div>`;
    const bar = (label: string, v: number, cls: string) =>
      `<div class="vital ${cls}"><span>${label}</span><i><em style="width:${Math.round(Math.max(0, Math.min(1, v)) * 100)}%"></em></i></div>`;
    this.vitals.innerHTML =
      bar('TRAJE', d.hp / 100, d.hp < 35 ? 'crit' : 'hp') +
      bar('O2', d.o2, d.o2 < 0.2 ? 'crit' : 'fuel') +
      bar('JET', d.fuel, 'fuel') +
      (d.tool === 'welder' || d.tool === 'welding'
        ? bar(d.tool === 'welding' ? 'SOLDANDO' : 'SOLDADORA LISTA', 1, 'fuel')
        : bar(d.tool === 'none' ? 'HERRAMIENTA GUARDADA' : d.reload >= 1 ? 'COHETE LISTO' : 'RECARGANDO', d.reload, d.reload >= 1 ? 'ready' : 'reload'));
    this.deathScreen.classList.toggle('hidden', !d.dead);
    this.net.innerHTML = `
      <div class="row"><i class="dot ${d.online ? 'ok' : 'bad'}"></i><b>${d.online ? 'EN LÍNEA' : 'SIN CONEXIÓN'}</b></div>
      <div class="row"><span>TRIPULACIÓN</span><b>${d.players}/${d.maxPlayers}</b></div>
      <div class="row"><span>RTT</span><b>${Math.round(d.rtt)} ms</b></div>
      <div class="row dim"><span>FPS</span><b>${Math.round(d.fps)}</b></div>`;
  }

  /** Floating name tags for other astronauts. */
  updateTag(id: number, name: string, world: THREE.Vector3, camera: THREE.Camera, color: string) {
    let tag = this.tags.get(id);
    if (!tag) {
      tag = el('div', 'hud-tag', this.tagLayer);
      this.tags.set(id, tag);
    }
    const p = world.clone().project(camera);
    const dist = world.distanceTo(camera.position);
    const visible = p.z < 1 && Math.abs(p.x) < 1.1 && Math.abs(p.y) < 1.1;
    tag.style.display = visible ? 'block' : 'none';
    if (!visible) return;
    const x = (p.x * 0.5 + 0.5) * this.root.clientWidth;
    const y = (-p.y * 0.5 + 0.5) * this.root.clientHeight;
    tag.style.transform = `translate(${x}px, ${y}px) translate(-50%, -100%)`;
    tag.style.setProperty('--c', color);
    tag.innerHTML = `<b>${escapeHtml(name)}</b><span>${fmtDist(dist)}</span>`;
  }

  removeTag(id: number) {
    this.tags.get(id)?.remove();
    this.tags.delete(id);
  }
}

const PX_PER_DEG = 4;

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
