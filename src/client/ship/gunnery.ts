import * as THREE from 'three';
import { aimAt, clampAim, mountMuzzle, slewAim, WEAPON_DEFS, type MountAim } from '../../shared/items';
import type { V3 } from '../../shared/ship/geom';
import { seatMounts } from '../../shared/ship/modules/weapons';
import type { ShipClient } from './ship';
import type { Input } from '../player/input';
import type { PipStatus } from '../render/pip';
import { CAMERA_ZOOM } from '../../shared/screens';

/** How far out the crosshair's aim converges (m): the mounts point at what is there. */
const CONVERGE = 300;
/** Aim updates sent while it changes (s between two), and the change worth sending (rad). */
const SEND_EVERY = 1 / 15;
const SEND_MIN = 0.002;
/** Reassert unchanged intent too: the first aim may reach the server before the seated state. */
const REASSERT_EVERY = 0.5;

export interface GunneryIO {
  /** Tell the authority where the gunner aims mount `m` (online: a message; offline: the sim itself). */
  aim(ship: ShipClient, m: number, yaw: number, pitch: number): void;
  /** A shot of mount `m` leaving `o` along `d` (ship space) with its weapon `w`: false if refused. */
  fire(ship: ShipClient, m: number, w: string, o: V3, d: V3): boolean;
  /** Point under the mount camera's sight, in world space; lock acquisition only, never per frame. */
  pick?(eye: THREE.Vector3, dir: THREE.Vector3, out: THREE.Vector3): boolean;
  refused?(reason: string): void;
  /** Align/release the observer on the station's display, without coupling this module to a camera rig. */
  focus?(ship: ShipClient, mount: string, on: boolean): void;
}

interface Manned {
  i: number;
  /** Where this client points it now (it leads the replicated head: the authority follows us). */
  lead: MountAim;
  target: MountAim;
  sent: MountAim;
  sentT: number;
  last: number;
  barrel: number;
}

const _eye = new THREE.Vector3();
const _pt: V3 = [0, 0, 0];
const _o: V3 = [0, 0, 0];
const _d: V3 = [0, 0, 0];
const _mouse: [number, number] = [0, 0];
const _worldEye = new THREE.Vector3();
const _worldDir = new THREE.Vector3();

/**
 * The seat of a gunner (`SeatDef.mounts`): while someone sits there, the mounts it lists point where
 * they look (without a monitor) or control a camera station at each mount's own slew rate, and the
 * trigger fires every one that is working and loaded, taking turns between its barrels. The heads
 * are drawn where this client points them; the authority slews its own toward the aim it is sent
 * and checks every shot against it (modules/weapons.ts).
 */
export class Gunnery {
  private ship: ShipClient | null = null;
  private manned: Manned[] = [];
  private consoleControl = false;
  aiming = false;
  zoom = 1;
  private zoomLevels = CAMERA_ZOOM;
  private zoomIndex = 0;
  private locked = false;
  private lockPoint = new THREE.Vector3();
  private textSlot = -1;
  private status: PipStatus = { title: '', detail: '', hint: '', warning: false, locked: false };
  private bar = { label: '', value: 0, state: 'reload' as 'ready' | 'reload' | 'fuel' };
  private actions = new Map<string, (mount: number) => string | null>();

  constructor(private io: GunneryIO) {
    this.registerAction('fire', i => this.trigger(performance.now() / 1000, false, i) ? null : this.failure(i));
    this.registerAction('aim', i => {
      this.aiming = !this.aiming; this.consoleControl = true; this.textSlot = -1;
      this.io.focus?.(this.ship!, this.ship!.sim.def.mounts[i].id, this.aiming);
      return null;
    });
    this.registerAction('zoom', () => { this.zoomIndex = (this.zoomIndex + 1) % this.zoomLevels.length; this.zoom = this.zoomLevels[this.zoomIndex]; this.textSlot = -1; return null; });
    this.registerAction('lock', () => this.toggleLock());
    this.registerAction('center', () => {
      this.locked = false;
      for (const m of this.manned) { m.target.yaw = m.target.pitch = 0; }
      this.textSlot = -1;
      return null;
    });
  }

  registerAction(id: string, handler: (mount: number) => string | null) {
    if (this.actions.has(id)) throw new Error(`gunner action already registered: ${id}`);
    this.actions.set(id, handler);
  }

  command(ship: ShipClient, target: string, action: string): string | null {
    const index = ship.sim.mounts?.index(target) ?? -1;
    if (!this.isManning(ship, index)) return 'Ocupa el asiento de artillero de este puesto';
    const handler = this.actions.get(action);
    return handler ? handler(index) : 'Mando de artillero desconocido';
  }

  isManning(ship: ShipClient, index: number): boolean {
    if (index < 0 || this.ship !== ship) return false;
    for (const m of this.manned) if (m.i === index) return true;
    return false;
  }

  aimOf(ship: ShipClient, index: number, out: MountAim): boolean {
    if (this.ship !== ship) return false;
    for (const m of this.manned) if (m.i === index) { out.yaw = m.lead.yaw; out.pitch = m.lead.pitch; return true; }
    return false;
  }

  /** Camera control consumes mouse motion; the astronaut keeps looking at the monitor. */
  look(input: Input): boolean {
    if (!this.active || !this.aiming) return false;
    input.look(_mouse);
    if (!this.locked) for (const m of this.manned) {
      m.target.yaw -= _mouse[0] / this.zoom;
      m.target.pitch -= _mouse[1] / this.zoom;
      clampAim(this.ship!.sim.mounts!.list[m.i].kind, m.target);
    }
    return true;
  }

  private toggleLock(): string | null {
    if (this.locked) { this.locked = false; this.textSlot = -1; return null; }
    const ship = this.ship, m = this.manned[0];
    if (!ship || !m) return 'Ocupa un asiento de artillero';
    const rt = ship.sim.mounts!.list[m.i];
    mountMuzzle(rt.def, rt.kind, m.lead, 0, _o, _d);
    for (let i = 0; i < 3; i++) _o[i] += _d[i] * 0.12;
    ship.world(_o, _worldEye);
    _worldDir.set(..._d).applyQuaternion(ship.view.root.quaternion);
    if (!this.io.pick?.(_worldEye, _worldDir, this.lockPoint)) return 'SIN BLANCO · apunta a una superficie visible';
    this.locked = true;
    this.consoleControl = true;
    this.textSlot = -1;
    return null;
  }

  get active() {
    return this.manned.length > 0;
  }

  /** Station key bindings. Called by a fixed gameplay system, independent of helmet control hits. */
  keys(input: Input, now: number) {
    if (input.consume('KeyT')) {
      if (this.active) this.trigger(now, true);
      else this.io.refused?.('T: ocupa un asiento de artillero para disparar');
    } else if (input.down('KeyT') && this.active) this.trigger(now);
    if (input.consume('KeyG') && this.active) this.actions.get('aim')!(this.manned[0].i);
  }

  /** Sat down in seat `seat` of a ship: true if it works mounts. */
  take(ship: ShipClient, seat: number): boolean {
    this.leave();
    const list = seatMounts(ship.sim.def, seat);
    const mounts = ship.sim.mounts;
    if (!list.length || !mounts) return false;
    this.ship = ship;
    this.manned = list.map((i) => {
      const lead = mounts.current(ship.sim.st, i, { yaw: 0, pitch: 0 });
      return { i, lead, target: { ...lead }, sent: { yaw: NaN, pitch: NaN }, sentT: 0, last: -Infinity, barrel: 0 };
    });
    for (const m of this.manned) ship.view.mountLead[m.i] = m.lead;
    const screen = ship.sim.def.screens.find(s => s.camera?.source.kind === 'mount' && list.some(i => ship.sim.def.mounts[i].id === s.camera!.source.ref));
    this.consoleControl = !!screen;
    this.zoomLevels = screen?.camera?.zoom ?? CAMERA_ZOOM;
    this.textSlot = -1;
    return true;
  }

  leave() {
    if (this.ship && this.aiming) this.io.focus?.(this.ship, this.ship.sim.def.mounts[this.manned[0].i].id, false);
    if (this.ship) for (const m of this.manned) this.ship.view.mountLead[m.i] = null;
    this.ship = null;
    this.manned = [];
    this.locked = this.aiming = this.consoleControl = false;
    this.zoom = 1;
    this.zoomIndex = 0;
    this.textSlot = -1;
  }

  /** Fixed simulation: head travel at the mount's rate, independent of rendered frame rate. */
  fixed(dt: number) {
    const mounts = this.ship?.sim.mounts;
    if (!mounts || !this.ship) return;
    const st = this.ship.sim.st;
    for (const m of this.manned) {
      const rt = mounts.list[m.i];
      if (st[rt.iReady] === 1) slewAim(rt.kind, m.lead, m.target, dt);
      else mounts.current(st, m.i, m.lead);
    }
  }

  /** After placing the camera: acquire aim in world space and send targets, never simulate travel. */
  frame(now: number, eye: THREE.Vector3, dir: THREE.Vector3) {
    const ship = this.ship;
    const mounts = ship?.sim.mounts;
    if (!ship || !mounts) return;
    if (this.locked || !this.consoleControl) {
      ship.local(this.locked ? _eye.copy(this.lockPoint) : _eye.copy(eye).addScaledVector(dir, CONVERGE), _eye);
      _pt[0] = _eye.x;
      _pt[1] = _eye.y;
      _pt[2] = _eye.z;
    }
    for (const m of this.manned) {
      const rt = mounts.list[m.i];
      if (this.locked || !this.consoleControl) aimAt(rt.def, rt.kind, _pt, m.target);
      if (now - m.sentT >= SEND_EVERY && (now - m.sentT >= REASSERT_EVERY || Math.abs(m.target.yaw - m.sent.yaw) > SEND_MIN || Math.abs(m.target.pitch - m.sent.pitch) > SEND_MIN || Number.isNaN(m.sent.yaw))) {
        m.sent.yaw = m.target.yaw;
        m.sent.pitch = m.target.pitch;
        m.sentT = now;
        this.io.aim(ship, m.i, m.target.yaw, m.target.pitch);
      }
    }
  }

  /** The trigger (held: at each weapon's rate). True if anything fired. */
  trigger(now: number, feedback = false, only = -1): boolean {
    const ship = this.ship;
    const mounts = ship?.sim.mounts;
    if (!ship || !mounts) return false;
    let any = false;
    for (const m of this.manned) {
      if (only >= 0 && m.i !== only) continue;
      const rt = mounts.list[m.i];
      const w = WEAPON_DEFS[rt.kind.weapon];
      if (now - m.last < w.cooldown || !mounts.canFire(ship.sim.st, m.i)) continue;
      mountMuzzle(rt.def, rt.kind, m.lead, m.barrel, _o, _d);
      if (!this.io.fire(ship, m.i, w.id, [_o[0], _o[1], _o[2]], [_d[0], _d[1], _d[2]])) continue;
      m.last = now;
      m.barrel = (m.barrel + 1) % rt.kind.barrels.length;
      any = true;
    }
    if (!any && feedback) this.io.refused?.(this.failure(only >= 0 ? only : this.manned[0]?.i ?? -1));
    return any;
  }

  private failure(index: number): string {
    const ship = this.ship;
    if (!ship) return 'Ocupa un asiento de artillero';
    return ship.sim.mounts?.unavailable(ship.sim.st, ship.sim.sw, index) ?? 'Espera a que termine la cadencia de disparo';
  }

  displayStatus(now: number): PipStatus | null {
    const ship = this.ship, mounts = ship?.sim.mounts, m = this.manned[0];
    if (!ship || !mounts || !m) return null;
    const slot = Math.floor(now * 4);
    if (slot === this.textSlot) return this.status;
    this.textSlot = slot;
    const rt = mounts.list[m.i], reason = mounts.unavailable(ship.sim.st, ship.sim.sw, m.i);
    this.status.title = rt.kind.name.toUpperCase();
    this.status.detail = `${Math.floor(ship.sim.st[rt.iAmmo])}/${rt.kind.magazine} · ${reason ?? (this.locked ? 'PUNTO FIJADO' : 'LISTA')} · ×${this.zoom}`;
    this.status.hint = this.aiming ? 'RATÓN: APUNTAR · T/CLIC: DISPARAR · G: CABINA' : 'G: APUNTAR EN PANTALLA · T: DISPARAR';
    this.status.warning = !!reason;
    this.status.locked = this.locked;
    return this.status;
  }

  /** The helmet's bar: the first manned mount's rounds and whether it works. */
  readout(): { label: string; value: number; state: 'ready' | 'reload' | 'fuel' } | null {
    const ship = this.ship;
    const mounts = ship?.sim.mounts;
    const m = this.manned[0];
    if (!ship || !mounts || !m) return null;
    const rt = mounts.list[m.i];
    const st = ship.sim.st;
    const status = this.displayStatus(performance.now() / 1000)!;
    this.bar.label = status.detail;
    this.bar.value = rt.kind.magazine === 0 ? 1 : st[rt.iAmmo] / rt.kind.magazine;
    this.bar.state = status.warning ? 'reload' : 'ready';
    return this.bar;
  }
}
