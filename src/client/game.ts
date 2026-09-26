import * as THREE from 'three';
import { MAX_PLAYERS, MOON, CLIENT_SEND_RATE, SUIT_STRIPES } from '../shared/constants';
import { StateFlags, type PlayerInfo, type TerrainEdit, type Vec3 } from '../shared/protocol';
import { LANDMARK, LunarTerrain } from '../shared/terrain';
import { Particles } from './fx/particles';
import { Rockets } from './fx/rockets';
import { Scheduler } from '../engine/systems';
import { FixedLoop } from '../engine/loop';
import { Debris } from '../engine/debris';
import { DebugOverlay } from '../engine/debug';
import { LAUNCHER } from './fx/weapons';
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
  me!: Astronaut;
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
  private particles!: Particles;
  private prevPos = new THREE.Vector3();
  /** Registered game systems (see src/engine/systems.ts); `game.systems.list()` from the console. */
  readonly systems = new Scheduler();
  private loop = new FixedLoop(1 / 60);
  private debris!: Debris;
  private diag!: DebugOverlay;
  private pendingHits: THREE.Vector3[] = [];
  private offline = false;
  private rockets!: Rockets;
  private hp = 100;
  private dead = false;
  private lastFire = -10;
  private fireQueued = false;
  private nozzles: [THREE.Vector3, THREE.Vector3] = [new THREE.Vector3(), new THREE.Vector3()];
  /** ?cam=x,z,yaw,pitch[,h] free inspection camera. */
  private inspectCam = new URLSearchParams(location.search).get('cam')?.split(',').map(Number) ?? null;
  /** Automation: keep a fixed third-person orbit (no easing back). */
  debugOrbit = false;
  /** Automation: frame a body part up close ({ part: 'handR', az, el, dist } relative to the body heading). */
  focusCam: { part: 'handL' | 'handR'; az: number; el: number; dist: number } | null = null;

  constructor(private opts: GameOptions) {
    this.pipeline = new RenderPipeline(this.scene, this.camera, { canvas: opts.canvas, quality: opts.quality });
    this.input = new Input(opts.canvas);
    this.net = new NetClient({
      join: (p) => this.addRemote(p, true),
      leave: (id) => this.removeRemote(id),
      state: (id, t, s) => this.remotes.get(id)?.push(t, s),
      disconnect: (reason) => this.hud?.toast(reason),
      fire: (id, o, d) => this.onFire(id, o, d),
      explode: (id, p, edit) => this.onExplode(id, p, edit),
      health: (id, hp, by, dead) => this.onHealth(id, hp, by, dead),
      respawn: (id, spawn) => this.onRespawn(id, spawn),
    });
    window.addEventListener('resize', () => this.resize());
    this.resize();
  }

  async start() {
    const { onProgress } = this.opts;
    onProgress('Conectando con el servidor…');
    const params = new URLSearchParams(location.search);
    // ?offline: no server — terrain/lighting inspection and solo testing
    this.offline = params.has('offline');
    this.welcome = this.offline
      ? { type: 'welcome', id: 1, variant: 0, players: [], spawn: [0, 0, 0], worldSeed: 1969, serverTime: 0, edits: [], health: [] }
      : await this.net.connect(this.opts.name);

    onProgress('Preparando la superficie lunar…');
    const renderer = this.pipeline.renderer;
    const loader = new THREE.TextureLoader();
    this.terrain = new LunarTerrain(this.welcome.worldSeed);
    this.terrain.edits = [...this.welcome.edits];
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
    this.terrainSys = new TerrainSystem(this.pool, this.terrain, loader, sunDir, csm, Math.min(8, aniso));
    this.scene.add(this.terrainSys.group);
    if (this.opts.quality === 'low') this.terrainSys.setBakedFade(75, 125);
    this.rocks = new RockField(this.pool, this.terrain, RockField.material(loader, csm));
    this.scene.add(this.rocks.group);

    onProgress('Cargando traje EVA…');
    this.asset = await AstronautAsset.load('/assets/astronaut.glb', csm);
    this.me = new Astronaut(this.asset);
    this.me.setLocal(true);
    this.me.setStripeColor(SUIT_STRIPES[this.welcome.variant % SUIT_STRIPES.length]);
    this.me.attachWeapon(LAUNCHER);
    this.me.setArmed(true);
    this.particles = new Particles(renderer.getPixelRatio());
    this.scene.add(this.particles.group);
    this.rockets = new Rockets(this.terrain, this.particles);
    this.scene.add(this.rockets.group);
    const debrisMat = RockField.material(loader, csm);
    this.debris = new Debris(this.physics.rapier, this.physics.world, MOON.gravity, debrisMat);
    this.scene.add(this.debris.mesh);
    this.diag = new DebugOverlay(this.opts.ui, this.scene);
    this.registerSystems();
    for (const h of this.welcome.health) this.pendingHealth.set(h.id, h.hp);
    window.addEventListener('mousedown', (e) => {
      if (e.button === 0 && this.input.locked) this.fireQueued = true;
    });
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

  /** Fixed-step simulation, in order. New gameplay systems register here (see AGENTS.md). */
  private registerSystems() {
    const S = this.systems;
    S.add({ name: 'physics', phase: 'fixed', order: 10, update: (h) => this.physics.step(h) });
    S.add({
      name: 'player',
      phase: 'fixed',
      order: 20,
      update: (h) => {
        this.prevPos.copy(this.ctl.position);
        this.ctl.update(h, this.input);
      },
    });
    S.add({ name: 'debris', phase: 'fixed', order: 30, update: (h) => this.debris.update(h) });
    S.add({
      name: 'rockets',
      phase: 'fixed',
      order: 40,
      update: (h) => {
        const up = new THREE.Vector3(0, 0.95, 0);
        const targets = [...this.remotes.values()].filter((r) => !r.dead).map((r) => ({ id: r.info.id, pos: r.position.clone().add(up) }));
        targets.push({ id: this.welcome.id, pos: this.ctl.position.clone().add(up) });
        this.pendingHits.push(...this.rockets.update(h, this.welcome.id, targets));
      },
    });
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

  private pendingHealth = new Map<number, number>();

  private addRemote(p: PlayerInfo, announce: boolean) {
    if (this.remotes.has(p.id) || !this.asset) return;
    const r = new RemotePlayer(p, this.asset);
    r.astronaut.attachWeapon(LAUNCHER);
    const hp = this.pendingHealth.get(p.id);
    if (hp !== undefined) {
      r.hp = hp;
      r.dead = hp <= 0;
    }
    this.remotes.set(p.id, r);
    this.scene.add(r.astronaut.root);
    if (announce) this.hud?.toast(`${p.name} se ha unido a la EVA`);
  }

  private nameOf(id: number) {
    if (id === this.welcome.id) return this.opts.name || 'Tú';
    return this.remotes.get(id)?.info.name ?? '???';
  }

  private onFire(id: number, o: Vec3, d: Vec3) {
    this.rockets?.spawn(id, new THREE.Vector3(...o), new THREE.Vector3(...d));
    this.remotes.get(id)?.astronaut.applyRecoil(LAUNCHER.recoil);
  }

  private onExplode(id: number, p: Vec3, edit: TerrainEdit) {
    const at = new THREE.Vector3(...p);
    this.terrain.edits.push(edit);
    this.terrainSys.invalidate(edit.x, edit.z, edit.r);
    this.physics.invalidate(edit.x, edit.z, edit.r);
    this.rocks.invalidate(edit.x, edit.z, edit.r);
    this.rockets.explode(id, at);
    if (this.physics.readyAt(at.x, at.z)) this.debris.burst(at, 8 + Math.floor(Math.random() * 6));
    // blast wave: push me away (the server decides damage)
    const c = this.ctl.position.clone().add(new THREE.Vector3(0, 0.9, 0));
    const dist = c.distanceTo(at);
    if (dist < 7 && !this.dead) {
      const k = (1 - dist / 7) * 7;
      this.ctl.impulse(c.sub(at).normalize().multiplyScalar(k).add(new THREE.Vector3(0, k * 0.5, 0)));
    }
  }

  private onHealth(id: number, hp: number, by?: number, dead?: boolean) {
    if (id === this.welcome.id) {
      if (hp < this.hp) this.hud.damage(this.hp - hp);
      this.hp = hp;
      if (dead) {
        this.dead = true;
        this.ctl.disabled = true;
        this.me.setDead(true);
        this.rig.mode === 'first' && this.rig.toggle();
      }
    } else {
      const r = this.remotes.get(id);
      if (r) {
        r.hp = hp;
        if (dead) r.dead = true;
      } else this.pendingHealth.set(id, hp);
    }
    if (dead && by !== undefined) {
      this.hud.toast(by === id ? `${this.nameOf(id)} se ha volado a sí mismo` : `${this.nameOf(by)} ha eliminado a ${this.nameOf(id)}`);
    }
  }

  private onRespawn(id: number, spawn: Vec3) {
    if (id === this.welcome.id) {
      this.dead = false;
      this.ctl.disabled = false;
      this.me.setDead(false);
      this.hp = 100;
      this.ctl.teleport(new THREE.Vector3(spawn[0], this.terrain.height(spawn[0], spawn[2]) + 0.1, spawn[2]));
      if (this.rig.mode === 'third') this.rig.toggle();
    } else {
      const r = this.remotes.get(id);
      if (r) {
        r.dead = false;
        r.hp = 100;
      }
    }
  }

  private tryFire() {
    const now = performance.now() / 1000;
    if (this.dead || !this.me.isArmed || now - this.lastFire < LAUNCHER.cooldown) return;
    this.lastFire = now;
    const dir = new THREE.Vector3();
    this.camera.getWorldDirection(dir);
    // launch from the shoulder tube, aimed at what the crosshair covers
    const origin = this.me.muzzle(new THREE.Vector3()) ?? this.ctl.position.clone().add(new THREE.Vector3(0, 1.5, 0));
    origin.addScaledVector(dir, 0.1);
    const target = this.camera.position.clone().addScaledVector(dir, 80);
    const aim = target.sub(origin).normalize();
    const o: Vec3 = [round(origin.x, 3), round(origin.y, 3), round(origin.z, 3)];
    const d: Vec3 = [round(aim.x, 4), round(aim.y, 4), round(aim.z, 4)];
    if (this.offline) this.onFire(this.welcome.id, o, d);
    else this.net.sendFire(o, d);
    // recoil kick
    // momentum conservation on a ~180 kg suited astronaut, plus the body/arm springs
    this.me.applyRecoil(LAUNCHER.recoil);
    this.ctl.impulse(aim.clone().multiplyScalar(-LAUNCHER.recoil / 180));
  }

  private emitJet(a: Astronaut) {
    a.nozzles(this.nozzles);
    for (const n of this.nozzles) {
      for (let i = 0; i < 3; i++) {
        _jetVel.set((Math.random() - 0.5) * 1.2, -7 - Math.random() * 4, (Math.random() - 0.5) * 1.2);
        this.particles.emit('glow', { pos: n, vel: _jetVel, color: [1.6, 1.9, 3.2], life: 0.08 + Math.random() * 0.1, size: 0.09 });
      }
    }
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
    if (input.consume('KeyX') || input.consume('Digit1')) {
      this.me.setArmed(!this.me.isArmed);
      this.hud.toast(this.me.isArmed ? 'Lanzacohetes en mano' : 'Lanzacohetes a la espalda');
    }
    if (this.fireQueued || input.consume('KeyF')) this.tryFire();
    this.fireQueued = false;
    this.me.setLamps(this.lamps);
    this.ctl.look(input, this.rig.mode === 'third' && !this.debugOrbit ? this.rig : undefined);
    this.physics.update(this.ctl.position.x, this.ctl.position.z);
    // fixed-step simulation: physics, character, debris, projectiles
    this.loop.advance(dt, (h) => {
      if (!this.physics.readyAt(this.ctl.position.x, this.ctl.position.z)) return;
      this.systems.run('fixed', h);
    });
    this.debris.sync();
    // safety net: never fall through the world
    const ground = this.terrain.height(this.ctl.position.x, this.ctl.position.z);
    if (this.ctl.position.y < ground - 2) this.ctl.teleport(new THREE.Vector3(this.ctl.position.x, ground + 0.3, this.ctl.position.z));

    // render between the last two sim states: no 60 Hz judder/smear on high refresh screens
    if (this.prevPos.distanceToSquared(this.ctl.position) > 25) this.prevPos.copy(this.ctl.position);
    this.ctl.renderPosition.lerpVectors(this.prevPos, this.ctl.position, this.loop.alpha);
    this.me.root.position.copy(this.ctl.renderPosition);
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
          (this.lamps ? StateFlags.Lamps : 0) |
          (this.ctl.jetting ? StateFlags.Jetpack : 0) |
          (this.dead ? StateFlags.Dead : 0) |
          (this.me.isArmed ? StateFlags.Armed : 0),
      });
    }
    const serverNow = this.net.serverNow();
    for (const r of this.remotes.values()) r.update(dt, serverNow);

    // --- combat & effects ------------------------------------------------------------------------
    for (const hit of this.pendingHits) {
      const p: Vec3 = [round(hit.x, 2), round(hit.y, 2), round(hit.z, 2)];
      // offline: act as our own server (crater, no damage bookkeeping)
      if (this.offline) this.onExplode(this.welcome.id, p, { x: p[0], z: p[2], r: 2.4, d: 1 });
      else this.net.sendHit(p);
    }
    this.pendingHits.length = 0;
    if (this.ctl.jetting) this.emitJet(this.me);
    for (const r of this.remotes.values()) if (r.jetting && !r.dead) this.emitJet(r.astronaut);
    this.particles.update(dt);

    // --- camera & world streaming -----------------------------------------------------------------
    this.rig.update(dt, this.ctl, this.me);
    if (this.inspectCam) {
      const [x, z, yaw, pitch, hgt = 1.8] = this.inspectCam;
      this.camera.position.set(x, this.terrain.height(x, z) + hgt, z);
      this.camera.quaternion.setFromEuler(new THREE.Euler(pitch, yaw, 0, 'YXZ'));
      this.camera.updateMatrixWorld();
    }
    if (this.focusCam) {
      const f = this.focusCam;
      const t = this.me.partPosition(f.part, new THREE.Vector3());
      const yaw = this.ctl.yaw + f.az;
      this.camera.position.set(t.x - Math.sin(yaw) * Math.cos(f.el) * f.dist, t.y + Math.sin(f.el) * f.dist, t.z - Math.cos(yaw) * Math.cos(f.el) * f.dist);
      this.camera.near = 0.02;
      this.camera.updateProjectionMatrix();
      this.camera.layers.enableAll();
      this.camera.lookAt(t);
      this.camera.updateMatrixWorld();
    }
    // recoil kicks the view up with the torso spring
    if (this.me.recoilPitch) {
      this.camera.rotateX(this.me.recoilPitch * 0.8);
      this.camera.updateMatrixWorld();
    }
    this.me.setEyeClip(this.rig.mode === 'first' ? this.camera.position : null);
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
      hp: this.hp,
      fuel: this.ctl.fuel,
      reload: this.me.isArmed ? Math.min(1, (performance.now() / 1000 - this.lastFire) / LAUNCHER.cooldown) : 0,
      dead: this.dead,
      markers,
    });

    this.diag.frame(dt * 1000);
    this.terrainSys.material.wireframe = this.diag.wireframe;
    this.diag.setPhysicsLines(this.diag.physicsLines ? this.physics.world.debugRender() : null);
    this.systems.endFrame();
    const sysMs: Record<string, string> = {};
    for (const [n, ms] of this.systems.timings) sysMs[`· ${n} ms`] = ms.toFixed(2);
    this.diag.update(this.pipeline.renderer, {
      ...sysMs,
      'sim steps/frame': this.loop.lastSteps,
      'terrain jobs': this.terrainSys.pendingJobs,
      'rigid bodies': this.physics.world.bodies.len(),
      colliders: this.physics.world.colliders.len(),
      debris: this.debris.count,
      'player pos': `${this.ctl.position.x.toFixed(1)}, ${this.ctl.position.y.toFixed(1)}, ${this.ctl.position.z.toFixed(1)}`,
      grounded: this.ctl.grounded ? 'sí' : 'no',
      'rtt ms': Math.round(this.net.rtt),
    });
    if (render) this.pipeline.render(dt);
    if (this.loop.lastSteps > 0) input.endFrame();
  }
}

const _jetVel = new THREE.Vector3();

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
