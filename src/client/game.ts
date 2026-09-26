import * as THREE from 'three';
import { MAX_PLAYERS, MOON, CLIENT_SEND_RATE, SUIT_STRIPES } from '../shared/constants';
import { StateFlags, type PlayerInfo } from '../shared/protocol';
import { LANDMARK, LunarTerrain } from '../shared/terrain';
import { NetClient, type Welcome } from './net/netClient';
import { RemotePlayer } from './net/remotePlayer';
import { Astronaut, AstronautAsset } from './player/astronaut';
import { CameraRig } from './player/cameraRig';
import { PlayerController } from './player/controller';
import { Input } from './player/input';
import { RenderPipeline } from './render/pipeline';
import { Hud } from './ui/hud';
import { dirFromAzEl, Lighting } from './world/lighting';
import { Physics } from './world/physics';
import { RockField } from './world/rocks';
import { Sky } from './world/sky';
import { TerrainSystem } from './world/terrain';
import { TerrainWorkerPool } from './world/workerPool';

export interface GameOptions {
  canvas: HTMLCanvasElement;
  ui: HTMLElement;
  name: string;
  quality: 'high' | 'low';
  onProgress: (text: string) => void;
}

// Landing site: ~20°N on the near side, local morning.
const SUN_AZ = 98;
const SUN_EL = 16;
const EARTH_AZ = 228;
const EARTH_EL = 52;
const SUN_ECLIPTIC_LONGITUDE = 150;

/** Top-level client: world, local astronaut, remote crew, network, HUD, frame loop. */
export class Game {
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(72, 1, 0.05, 60000);
  private pipeline: RenderPipeline;
  private input: Input;
  private net: NetClient;
  private hud!: Hud;
  private terrain!: LunarTerrain;
  private terrainSys!: TerrainSystem;
  private rocks!: RockField;
  private physics!: Physics;
  private lighting!: Lighting;
  private sky!: Sky;
  private asset!: AstronautAsset;
  private me!: Astronaut;
  private ctl!: PlayerController;
  private rig!: CameraRig;
  private remotes = new Map<number, RemotePlayer>();
  private welcome!: Welcome;
  private pool!: TerrainWorkerPool;
  private clock = new THREE.Timer();
  private sendAccum = 0;
  private evaTime = 0;
  private fps = 60;
  private spawned = false;
  private running = false;
  private lamps = false;
  private startTime = performance.now();
  /** Automation: keep a fixed third-person orbit (no easing back). */
  debugOrbit = false;

  constructor(private opts: GameOptions) {
    this.pipeline = new RenderPipeline(this.scene, this.camera, { canvas: opts.canvas, quality: opts.quality });
    this.input = new Input(opts.canvas);
    this.net = new NetClient({
      join: (p) => this.addRemote(p, true),
      leave: (id) => this.removeRemote(id),
      state: (id, t, s) => this.remotes.get(id)?.push(t, s),
      disconnect: (reason) => this.hud?.toast(reason),
    });
    window.addEventListener('resize', () => this.resize());
    this.resize();
  }

  async start() {
    const { onProgress } = this.opts;
    onProgress('Conectando con el servidor…');
    this.welcome = await this.net.connect(this.opts.name);

    onProgress('Preparando la superficie lunar…');
    const renderer = this.pipeline.renderer;
    const loader = new THREE.TextureLoader();
    this.terrain = new LunarTerrain(this.welcome.worldSeed);
    this.pool = new TerrainWorkerPool();
    this.physics = await Physics.create(this.pool, this.terrain);

    const sunDir = dirFromAzEl(SUN_AZ, SUN_EL);
    const earthDir = dirFromAzEl(EARTH_AZ, EARTH_EL);
    this.lighting = new Lighting(this.scene, this.camera, renderer, sunDir, earthDir, this.opts.quality);
    const csm = this.lighting.csm;

    const stars = new Float32Array(await (await fetch('/assets/sky/stars.bin')).arrayBuffer());
    this.sky = new Sky({ sunDir, earthDir, celestial: celestialMatrix(sunDir, SUN_ECLIPTIC_LONGITUDE) }, loader, stars);
    this.sky.setPixelRatio(renderer.getPixelRatio());
    this.scene.add(this.sky.group);

    const aniso = renderer.capabilities.getMaxAnisotropy();
    this.terrainSys = new TerrainSystem(this.pool, this.welcome.worldSeed, loader, sunDir, csm, Math.min(8, aniso));
    this.scene.add(this.terrainSys.group);
    this.rocks = new RockField(this.pool, this.welcome.worldSeed, RockField.material(loader, csm));
    this.scene.add(this.rocks.group);

    onProgress('Cargando traje EVA…');
    this.asset = await AstronautAsset.load('/assets/astronaut.glb', csm);
    this.me = new Astronaut(this.asset);
    this.me.setLocal(true);
    this.me.setStripeColor(SUIT_STRIPES[this.welcome.variant % SUIT_STRIPES.length]);
    this.scene.add(this.me.root);
    for (const p of this.welcome.players) this.addRemote(p, false);

    const [sx, , sz] = this.welcome.spawn;
    const spawn = new THREE.Vector3(sx, this.terrain.height(sx, sz) + 0.05, sz);
    this.ctl = PlayerController.forBody(this.physics, MOON, spawn);
    // face the landmark crater on arrival
    this.ctl.yaw = Math.atan2(-(LANDMARK.x - sx), -(LANDMARK.z - sz)) + 0.5;
    this.ctl.pitch = -0.05;
    this.rig = new CameraRig(this.camera, this.terrain);
    this.me.root.position.copy(spawn);

    this.hud = new Hud(this.opts.ui);
    this.hud.root.classList.add('hidden');

    // stream the world around the spawn before letting the player in
    onProgress('Generando terreno…');
    await this.warmUp(spawn);
    this.hud.root.classList.remove('hidden');
    this.spawned = true;
    this.running = true;
    // ?manual → no rAF loop; automation drives the simulation with step()
    if (!new URLSearchParams(location.search).has('manual')) this.frame();
  }

  /** Advance `frames` fixed steps; renders only the last one unless `renderAll`. */
  step(frames = 1, dt = 1 / 30) {
    for (let i = 0; i < frames; i++) this.tick(dt, i === frames - 1);
  }

  get pointerLocked() {
    return this.input.locked;
  }

  lockPointer() {
    this.input.lock();
  }

  /** Automation hook (tests / screenshots). */
  get debug() {
    return { input: this.input, controller: this.ctl, rig: this.rig, camera: this.camera, remotes: this.remotes, game: this };
  }

  private async warmUp(spawn: THREE.Vector3) {
    const t0 = performance.now();
    this.camera.position.set(spawn.x, spawn.y + 1.7, spawn.z);
    this.camera.updateMatrixWorld();
    const frustum = new THREE.Frustum();
    while (performance.now() - t0 < 25000) {
      this.terrainSys.update(this.camera.position, frustum);
      this.rocks.update(spawn);
      this.physics.update(spawn.x, spawn.z);
      const ready = this.terrainSys.readyAt(spawn.x, spawn.z) && this.physics.readyAt(spawn.x, spawn.z);
      const pending = this.terrainSys.pendingJobs;
      this.opts.onProgress(`Generando terreno… ${pending} bloques pendientes`);
      if (ready && pending < 6) break;
      await new Promise((r) => setTimeout(r, 50));
    }
    // physics needs a step to register the new colliders before the first character query
    this.physics.step(1 / 60);
  }

  private addRemote(p: PlayerInfo, announce: boolean) {
    if (this.remotes.has(p.id) || !this.asset) return;
    const r = new RemotePlayer(p, this.asset);
    this.remotes.set(p.id, r);
    this.scene.add(r.astronaut.root);
    if (announce) this.hud?.toast(`${p.name} se ha unido a la EVA`);
  }

  private removeRemote(id: number) {
    const r = this.remotes.get(id);
    if (!r) return;
    this.hud?.toast(`${r.info.name} ha abandonado la EVA`);
    this.hud?.removeTag(id);
    r.dispose();
    this.remotes.delete(id);
  }

  private resize() {
    this.pipeline.resize(window.innerWidth, window.innerHeight);
  }

  private frame = () => {
    if (!this.running) return;
    requestAnimationFrame(this.frame);
    this.clock.update();
    const dt = Math.min(this.clock.getDelta(), 1 / 20);
    this.fps += (1 / Math.max(dt, 1e-4) - this.fps) * 0.05;
    this.tick(dt);
  };

  /** One simulation + render step (exposed for deterministic automation). */
  tick(dt: number, render = true) {
    const input = this.input;
    if (!this.spawned) return;

    // --- local player -------------------------------------------------------------------------
    if (input.consume('KeyV')) this.rig.toggle();
    if (input.consume('KeyH')) this.hud.toggleHelp();
    if (input.consume('KeyL')) this.lamps = !this.lamps;
    this.me.setLamps(this.lamps);
    this.ctl.look(input, this.rig.mode === 'third' && !this.debugOrbit ? this.rig : undefined);
    this.physics.update(this.ctl.position.x, this.ctl.position.z);
    if (this.physics.readyAt(this.ctl.position.x, this.ctl.position.z)) {
      this.physics.step(dt);
      this.ctl.update(dt, input);
    }
    // safety net: never fall through the world
    const ground = this.terrain.height(this.ctl.position.x, this.ctl.position.z);
    if (this.ctl.position.y < ground - 2) this.ctl.teleport(new THREE.Vector3(this.ctl.position.x, ground + 0.3, this.ctl.position.z));

    this.me.root.position.copy(this.ctl.position);
    this.me.root.rotation.y = this.ctl.yaw;
    this.me.update(dt, {
      velocity: this.ctl.velocity,
      yaw: this.ctl.yaw,
      pitch: this.ctl.pitch,
      grounded: this.ctl.grounded,
      crouch: this.ctl.crouch,
    });
    this.me.root.updateMatrixWorld(true);
    this.evaTime += dt;

    // --- network -------------------------------------------------------------------------------
    this.sendAccum += dt;
    if (this.sendAccum >= 1 / CLIENT_SEND_RATE) {
      this.sendAccum = 0;
      const p = this.ctl.position;
      const v = this.ctl.velocity;
      this.net.sendState({
        p: [round(p.x, 3), round(p.y, 3), round(p.z, 3)],
        v: [round(v.x, 2), round(v.y, 2), round(v.z, 2)],
        yaw: round(this.ctl.yaw, 3),
        pitch: round(this.ctl.pitch, 3),
        f:
          (this.ctl.grounded ? StateFlags.Grounded : 0) |
          (this.ctl.running ? StateFlags.Running : 0) |
          (this.ctl.crouch ? StateFlags.Crouching : 0) |
          (this.lamps ? StateFlags.Lamps : 0),
      });
    }
    const serverNow = this.net.serverNow();
    for (const r of this.remotes.values()) r.update(dt, serverNow);

    // --- camera & world streaming -----------------------------------------------------------------
    this.rig.update(dt, this.ctl, this.me);
    this.terrainSys.update(this.camera.position, new THREE.Frustum());
    this.rocks.update(this.ctl.position);
    this.sky.update(this.camera, (performance.now() - this.startTime) / 1000 + 36000);
    this.lighting.update();

    // --- HUD ---------------------------------------------------------------------------------------
    const markers = [
      markerTo(this.ctl.position, new THREE.Vector3(0, 0, 0), 'Base', '#9fd3ff'),
      markerTo(this.ctl.position, new THREE.Vector3(LANDMARK.x, 0, LANDMARK.z), 'Cráter', '#e8d49c'),
    ];
    for (const r of this.remotes.values()) {
      const color = cssColor(SUIT_STRIPES[r.info.variant % SUIT_STRIPES.length]);
      markers.push(markerTo(this.ctl.position, r.position, r.info.name, color));
      const head = r.position.clone().add(new THREE.Vector3(0, 2.05, 0));
      this.hud.updateTag(r.info.id, r.info.name, head, this.camera, color);
    }
    this.hud.update({
      heading: -this.ctl.yaw,
      position: this.ctl.position,
      speed: Math.hypot(this.ctl.velocity.x, this.ctl.velocity.z),
      altitude: this.ctl.position.y,
      lamps: this.lamps,
      evaSeconds: this.evaTime,
      cameraMode: this.rig.mode,
      online: this.net.connected,
      players: this.remotes.size + 1,
      maxPlayers: MAX_PLAYERS,
      rtt: this.net.rtt,
      fps: this.fps,
      markers,
    });

    if (render) this.pipeline.render(dt);
    input.endFrame();
  }
}

function round(v: number, d: number) {
  const k = 10 ** d;
  return Math.round(v * k) / k;
}

function markerTo(from: THREE.Vector3, to: THREE.Vector3, label: string, color: string) {
  const dx = to.x - from.x;
  const dz = to.z - from.z;
  // bearing: 0 = north (-Z), clockwise toward east (+X)
  return { label, bearing: Math.atan2(dx, -dz), distance: Math.hypot(dx, dz), color };
}

function cssColor(hex: number) {
  return `#${hex.toString(16).padStart(6, '0')}`;
}

/**
 * Equatorial (J2000) → local frame for a site whose ecliptic pole stands 20° above the
 * northern horizon, with the Sun (given local direction) at the given ecliptic longitude.
 */
function celestialMatrix(sunDir: THREE.Vector3, sunLongitudeDeg: number) {
  const lat = THREE.MathUtils.degToRad(20);
  const Z = new THREE.Vector3(0, Math.sin(lat), -Math.cos(lat)); // ecliptic north pole
  const S = sunDir.clone().addScaledVector(Z, -sunDir.dot(Z)).normalize();
  const T = new THREE.Vector3().crossVectors(Z, S);
  const l = THREE.MathUtils.degToRad(sunLongitudeDeg);
  const X = S.clone().multiplyScalar(Math.cos(l)).addScaledVector(T, -Math.sin(l));
  const Y = new THREE.Vector3().crossVectors(Z, X);
  const eclToLocal = new THREE.Matrix3().set(X.x, Y.x, Z.x, X.y, Y.y, Z.y, X.z, Y.z, Z.z);
  const eps = THREE.MathUtils.degToRad(23.4393);
  const c = Math.cos(eps);
  const s = Math.sin(eps);
  // equatorial → ecliptic: rotate about X by -ε
  const eqToEcl = new THREE.Matrix3().set(1, 0, 0, 0, c, s, 0, -s, c);
  return eclToLocal.multiply(eqToEcl);
}
