// Ship adapter: data defines display placement and occupancy; the PiP core knows neither.
import * as THREE from 'three';
import type { ScreenDef } from '../../shared/ship/def';
import type { ShipSim } from '../../shared/ship/sim';
import { mountMuzzle, type MountAim } from '../../shared/items';
import { PipDisplay, type PipSource, type PipStatus } from '../render/pip';
import { CameraSources } from '../render/cameraSources';
import { origin } from '../render/origin';
import type { ShipClient } from './ship';
import type { Gunnery } from './gunnery';
import { cameraFov } from '../../shared/screens';

export interface ShipCameraScreen { def: ScreenDef; display: PipDisplay; seat: number }
export class ShipCameraScreens {
  readonly items: ShipCameraScreen[] = [];
  constructor(sim: ShipSim) {
    for (const def of sim.def.screens) {
      if (!def.camera) continue;
      const display = new PipDisplay(def.w, def.h, def.camera);
      display.root.matrixAutoUpdate = false;
      display.root.matrix.makeBasis(new THREE.Vector3(...def.u), new THREE.Vector3(...def.v), new THREE.Vector3(...def.n)).setPosition(...def.c);
      display.root.name = def.id;
      this.items.push({ def, display, seat: sim.def.seats.findIndex(s => s.id === def.seat) });
    }
  }
}

export interface ShipCameraContext { ship: ShipClient; screen: ShipCameraScreen; gunnery: Gunnery }

class MountCamera implements PipSource {
  readonly camera: THREE.PerspectiveCamera;
  enabled = false;
  live = false;
  readonly status: PipStatus = { title: '', detail: '', hint: '', warning: false, locked: false };
  private aim: MountAim = { yaw: 0, pitch: 0 };
  private point: [number, number, number] = [0, 0, 0];
  private axis: [number, number, number] = [0, 0, 0];
  private dir = new THREE.Vector3();
  private up = new THREE.Vector3();
  private target = new THREE.Vector3();
  private index: number;
  private lastFov = -1;
  private textSlot = -1;

  constructor(private ctx: ShipCameraContext) {
    const { ship, screen } = ctx;
    this.index = ship.sim.mounts?.index(screen.def.camera!.source.ref) ?? -1;
    if (this.index < 0) throw new Error(`camera ${screen.def.id}: unknown mount ${screen.def.camera!.source.ref}`);
    this.camera = new THREE.PerspectiveCamera(55, screen.def.w / screen.def.h, 0.1, 10000);
    this.camera.matrixAutoUpdate = false;
    origin.root.add(this.camera);
  }

  update() {
    const { ship, gunnery } = this.ctx;
    const sim = ship.sim, mounts = sim.mounts!, rt = mounts.list[this.index];
    this.enabled = true;
    this.live = sim.st[rt.iReady] === 1;
    const now = performance.now() / 1000, slot = Math.floor(now * 4);
    if (slot === this.textSlot) return;
    this.textSlot = slot;
    const status = gunnery.isManning(ship, this.index) ? gunnery.displayStatus(now) : null;
    if (status) Object.assign(this.status, status);
    else {
      this.status.title = rt.kind.name.toUpperCase();
      this.status.detail = this.live ? `${Math.floor(sim.st[rt.iAmmo])}/${rt.kind.magazine} · EN SERVICIO` : 'SIN SERVICIO';
      this.status.hint = 'PUESTO DE ARTILLERO';
      this.status.warning = !this.live;
      this.status.locked = false;
    }
  }

  prepare() {
    const { ship, gunnery } = this.ctx, rt = ship.sim.mounts!.list[this.index];
    if (!gunnery.aimOf(ship, this.index, this.aim)) ship.sim.mounts!.current(ship.sim.st, this.index, this.aim);
    mountMuzzle(rt.def, rt.kind, this.aim, 0, this.point, this.axis);
    // Just in front of the tube: the camera cannot look through its own launcher.
    for (let i = 0; i < 3; i++) this.point[i] += this.axis[i] * 0.12;
    ship.world(this.point, this.camera.position);
    this.dir.set(...this.axis).applyQuaternion(ship.view.root.quaternion);
    this.up.set(...rt.def.up).applyQuaternion(ship.view.root.quaternion);
    this.camera.up.copy(this.up);
    this.target.copy(this.camera.position).add(this.dir);
    this.camera.updateMatrix();
    this.camera.updateMatrixWorld(true);
    this.camera.lookAt(origin.toRender(this.target));
    this.camera.updateMatrix();
    const zoom = gunnery.isManning(ship, this.index) ? gunnery.zoom : 1;
    const fov = cameraFov(55, zoom);
    if (fov !== this.lastFov) { this.lastFov = fov; this.camera.fov = fov; this.camera.updateProjectionMatrix(); }
    this.camera.updateMatrixWorld(true);
  }

  dispose() { this.camera.removeFromParent(); }
}

/** A new provider registers here. Display placement, visibility and renderer budgets stay generic. */
export function shipCameraSources(): CameraSources<ShipCameraContext> {
  const providers = new CameraSources<ShipCameraContext>();
  providers.register('mount', context => new MountCamera(context));
  return providers;
}

/** All ship camera kinds share seat, power and mounting integrity gates. Providers only describe their camera. */
export class ShipCameraFeed implements PipSource {
  enabled = false;
  live = false;
  constructor(private ctx: ShipCameraContext, private source: PipSource) {}
  get camera() { return this.source.camera; }
  get status() { return this.source.status; }
  update() {
    const { ship, screen } = this.ctx, sim = ship.sim, def = screen.def;
    const occupied = !def.seat || (screen.seat >= 0 && ship.view.seatOccupied[screen.seat]);
    const powered = occupied && sim.powered(def.circuit) && (def.host < 0 || !sim.hole(def.host)) && (def.hostPart < 0 || sim.partHp(def.hostPart) > 0);
    if (!powered) { this.enabled = this.live = false; return; }
    this.source.update?.();
    this.enabled = this.source.enabled;
    this.live = this.enabled && this.source.live;
  }
  prepare() { this.source.prepare(); }
  dispose() { this.source.dispose?.(); }
}

export class ShipCameraFeeds {
  private sources: PipSource[] = [];
  constructor(ship: ShipClient, gunnery: Gunnery, providers: CameraSources<ShipCameraContext>) {
    for (const screen of ship.view.cameraScreens.items) {
      const context = { ship, screen, gunnery };
      const source = new ShipCameraFeed(context, providers.create(screen.def.camera!.source.kind, context));
      screen.display.source = source;
      this.sources.push(source);
    }
  }
  frame() { for (const source of this.sources) source.update?.(); }
  dispose() { for (const source of this.sources) source.dispose?.(); }
}
