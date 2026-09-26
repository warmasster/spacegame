import * as THREE from 'three';
import { MAX_PLAYERS, MOON, CLIENT_SEND_RATE, SHIP_SPAWNS, SUIT_STRIPES, SUN } from '../shared/constants';
import { StateFlags, type PlayerInfo, type TerrainEdit, type Vec3 } from '../shared/protocol';
import { LANDMARK, LunarTerrain } from '../shared/terrain';
import { placeShip, REPAIR_RATE, SHIP_DEFS, shipBlast, ShipSim, SYSTEMS_HZ, type ShipSnapshot } from '../shared/ship/sim';
import { crewStep } from '../shared/ship/crew';
import type { SysEvent } from '../shared/ship/systems';
import { Interaction } from './ship/interaction';
import { updateInteriorLights } from './ship/interiorLights';
import { litMaterial } from './ship/materials';
import { ShipClient } from './ship/ship';
import { Particles } from './fx/particles';
import { Rockets } from './fx/rockets';
import { JointDiagnostics, JointGizmos } from './player/jointDiag';
import { Scheduler } from '../engine/systems';
import { FixedLoop } from '../engine/loop';
import { Debris } from '../engine/debris';
import { DebugOverlay } from '../engine/debug';
import { LAUNCHER, WELDER } from './fx/weapons';
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
  /** Rolled from the menu. Offline uses it; online asks the empty server to adopt it. */
  worldSeed?: number;
  onProgress: (text: string) => void;
}

// Landing site: ~20°N on the near side, local morning.
const SUN_AZ = SUN.az;
const SUN_EL = SUN.el;
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
  /** Joint angles / angular speeds of the local astronaut (F6 panel + axis gizmos). */
  joints!: JointDiagnostics;
  private jointGizmos!: JointGizmos;
  private jointPanel!: HTMLPreElement;
  /** Automation: sample joints every frame even with the panel closed. */
  jointsRecording = false;
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
  /** Ships in the world (client mirrors of the server's). `game.ships` from the console. */
  ships: ShipClient[] = [];
  /** Offline only: this client is its own ship authority. */
  private shipAuthority = new Map<number, ShipSim>();
  interaction!: Interaction;
  private scrap!: Debris;
  private envInside = 0;
  /** Seat the local astronaut sits in. */
  seat: { ship: ShipClient; index: number } | null = null;
  private welding = false;
  /** Suit oxygen reserve 0..1 (from the server, or local offline). */
  suitO2 = 1;
  private sysAcc = 0;
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
      ship: (ship, sw, hp, by) => this.onShip(ship, sw, hp, by),
      shipDenied: (ship, ctl) => this.ships.find((s) => s.id === ship)?.view.refuse(ctl),
      shipState: (ship, d) => this.ships.find((s) => s.id === ship)?.sim.applyState(d),
      say: (_ship, text) => this.hud?.toast(text),
      vitals: (o2) => (this.suitO2 = o2),
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
    const seed = this.opts.worldSeed ?? 1969;
    this.welcome = this.offline
      ? { type: 'welcome', id: 1, variant: 0, players: [], spawn: [0, 0, 0], worldSeed: seed, serverTime: 0, edits: [], health: [], ships: this.offlineShips(seed) }
      : await this.net.connect(this.opts.name, undefined, this.opts.worldSeed);

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
    this.me.attachWeapon(WELDER);
    this.me.setArmed(true);
    this.particles = new Particles(renderer.getPixelRatio());
    this.scene.add(this.particles.group);
    this.rockets = new Rockets(this.terrain, this.particles);
    this.scene.add(this.rockets.group);
    const debrisMat = RockField.material(loader, csm);
    this.debris = new Debris(this.physics.rapier, this.physics.world, MOON.gravity, debrisMat);
    this.scene.add(this.debris.mesh);

    onProgress('Preparando la nave…');
    // hull fragments: thin bent plates
    this.scrap = new Debris(this.physics.rapier, this.physics.world, MOON.gravity, litMaterial(csm, { color: new THREE.Color().setRGB(0.3, 0.3, 0.31), roughness: 0.5, metalness: 0.6 }), new THREE.BoxGeometry(1.4, 0.12, 1.0));
    this.scene.add(this.scrap.mesh);
    // ships stand on the pristine surface, like on the server
    const pristine = new LunarTerrain(this.welcome.worldSeed);
    for (const snap of this.welcome.ships ?? []) {
      const ship = new ShipClient(snap, { physics: this.physics, csm, ground: pristine, particles: this.particles, debris: this.scrap, gravity: MOON.gravity });
      this.ships.push(ship);
      this.scene.add(ship.view.root, ship.cargo.group);
    }
    this.interaction = new Interaction(this.scene, this.particles, {
      interact: (ship, ctl) => this.operate(ship, ctl),
      repair: (ship, panel, dt) => (this.offline ? this.localRepair(ship.id, panel, dt) : this.net.sendRepair(ship.id, panel)),
      repairPart: (ship, part, dt) => (this.offline ? this.localRepairPart(ship.id, part, dt) : this.net.sendRepairPart(ship.id, part)),
      sit: (ship, index) => this.sitDown(ship, index),
    });
    this.diag = new DebugOverlay(this.opts.ui, this.scene);
    this.registerSystems();
    this.joints = new JointDiagnostics(this.me);
    this.jointGizmos = new JointGizmos(this.me);
    this.scene.add(this.jointGizmos.group);
    this.jointPanel = document.createElement('pre');
    this.jointPanel.className = 'debug-overlay joints-panel hidden';
    this.opts.ui.appendChild(this.jointPanel);
    window.addEventListener('keydown', (e) => {
      if (e.code !== 'F6') return;
      e.preventDefault();
      const on = this.jointPanel.classList.toggle('hidden') === false;
      this.jointGizmos.group.visible = on;
      if (on) this.joints.reset();
    });
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
    // the manual (M) is the one of the ship you are in, or of the nearest one
    this.hud.beforeManual = () => this.bindManual();
    this.bindManual();
    // reading the manual frees the mouse; closing it takes it back (M / Esc / ✕ are user gestures)
    this.hud.onManual = (open) => {
      if (open) document.exitPointerLock?.();
      else this.lockPointer();
    };
    this.rig.wheelTo = (dir) => {
      // the manual has the mouse: the page scrolls by itself, the camera must not zoom
      if (this.hud.manualOpen) return true;
      const t = this.interaction.target;
      if (!t || t.kind !== 'control' || !t.inReach) return false;
      const c = t.ship.sim.def.controls[t.index];
      if (c.action !== 'cycle') return false;
      this.operate(t.ship, t.index, dir);
      return true;
    };

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
    S.add({
      name: 'debris',
      phase: 'fixed',
      order: 30,
      update: (h) => {
        this.debris.update(h);
        this.scrap.update(h);
      },
    });
    S.add({
      name: 'ships',
      phase: 'fixed',
      order: 30,
      update: (h) => {
        for (const ship of this.ships) ship.fixed(h);
        if (this.offline) this.offlineSystems(h);
      },
    });
    S.add({
      name: 'rockets',
      phase: 'fixed',
      order: 40,
      update: (h) => {
        const up = new THREE.Vector3(0, 0.95, 0);
        const targets = [...this.remotes.values()].filter((r) => !r.dead).map((r) => ({ id: r.info.id, pos: r.position.clone().add(up) }));
        targets.push({ id: this.welcome.id, pos: this.ctl.position.clone().add(up) });
        this.pendingHits.push(...this.rockets.update(h, this.welcome.id, targets, (a, b) => this.shipSweep(a, b)));
      },
    });
    // --- per rendered frame (after the camera is placed) ---
    S.add({
      name: 'interaction',
      phase: 'frame',
      order: 40,
      update: (dt) => {
        const eye = this.me.eyePosition(new THREE.Vector3());
        const prompt = this.interaction.update(dt, {
          camera: this.camera,
          eye,
          hand: this.me.muzzle(new THREE.Vector3()) ?? this.me.partPosition('handR', new THREE.Vector3()),
          ships: this.ships,
          tool: this.me.equipped === 'welder' && this.me.isArmed && !this.seat,
          holdRepair: this.input.down('Mouse0') && !this.dead,
          seated: !!this.seat,
          now: performance.now() / 1000,
          disabled: this.dead || !!this.inspectCam,
        });
        this.hud.setPrompt(prompt);
        this.welding = this.interaction.repairing;
      },
    });
    S.add({
      name: 'ship-view',
      phase: 'frame',
      order: 90,
      update: (dt) => {
        for (const ship of this.ships) {
          // docked packs light the seat's umbilical port
          ship.sim.def.seats.forEach((_, i) => {
            const at = ship.seatPose(i).pos;
            ship.view.seatOccupied[i] = (this.seat?.ship === ship && this.seat.index === i) || [...this.remotes.values()].some((r) => r.seated && r.position.distanceTo(at) < 0.35);
          });
          ship.frame(dt);
        }
        for (const r of this.remotes.values()) if (r.welding && !r.dead) this.remoteWeld(r, dt);
        updateInteriorLights(this.camera);
        // eyes adapt inside: the regolith glow of the environment map is outside
        const inside = this.ships.some((s) => s.zoneAt(this.camera.position));
        this.envInside += ((inside ? 1 : 0) - this.envInside) * Math.min(1, dt * 3);
        this.scene.environmentIntensity = 1 - 0.65 * this.envInside;
      },
    });
  }

  /** Another astronaut welding: sparks where its tool points (the panel HP comes from the server). */
  private remoteWeld(r: RemotePlayer, dt: number) {
    const from = r.astronaut.muzzle(new THREE.Vector3());
    if (!from || Math.random() > dt * 30) return;
    const dir = new THREE.Vector3(-Math.sin(r.yaw) * Math.cos(r.pitch), Math.sin(r.pitch), -Math.cos(r.yaw) * Math.cos(r.pitch));
    for (const ship of this.ships) {
      const h = ship.pick(from, dir, 3.5);
      if (!h || h.kind !== 'panel') continue;
      for (let k = 0; k < 4; k++) {
        this.particles.emit('glow', { pos: h.point, vel: new THREE.Vector3().randomDirection().multiplyScalar(0.6 + Math.random() * 2).addScaledVector(h.normal, 1), color: [2.4, 3, 4.5], life: 0.1 + Math.random() * 0.3, size: 0.015, gravity: 1.62 });
      }
      return;
    }
  }

  /** Sit in a ship seat: pinned to it, tool slung, head free to look around. */
  sitDown(ship: ShipClient, index: number) {
    if (this.dead || this.seat) return;
    const pose = ship.seatPose(index);
    // someone already there?
    for (const r of this.remotes.values()) if (r.seated && r.position.distanceTo(pose.pos) < 0.35) return this.hud.toast('Asiento ocupado');
    this.seat = { ship, index };
    if (this.ctl.crouch) this.input.setKey('KeyC', false);
    this.ctl.seat = { pos: pose.pos, yaw: pose.yaw };
    this.ctl.teleport(pose.pos);
    this.prevPos.copy(pose.pos);
    this.ctl.yaw = pose.yaw;
  }

  standUp() {
    if (!this.seat) return;
    const pose = this.seat.ship.seatPose(this.seat.index);
    this.seat = null;
    this.ctl.seat = null;
    this.ctl.teleport(pose.exit.add(new THREE.Vector3(0, 0.03, 0)));
    this.prevPos.copy(this.ctl.position);
  }

  /** Rocket sweep against every ship hull. */
  private shipSweep(a: THREE.Vector3, b: THREE.Vector3) {
    for (const ship of this.ships) {
      const h = ship.segmentHit(a, b);
      if (h) return h;
    }
    return null;
  }

  /** Default ship states for ?offline (what a fresh server would send). */
  private offlineShips(seed: number): ShipSnapshot[] {
    const ground = new LunarTerrain(seed);
    return SHIP_SPAWNS.map((s) => {
      const def = SHIP_DEFS[s.def];
      const sim = new ShipSim(s.id, def, placeShip(def, s.x, s.z, s.yaw, ground), ground);
      this.shipAuthority.set(s.id, sim);
      return sim.snapshot();
    });
  }

  /** Offline: this client runs the ship machinery and its own suit, like the server would. */
  private offlineSystems(h: number) {
    this.sysAcc += h;
    const step = 1 / SYSTEMS_HZ;
    while (this.sysAcc >= step) {
      this.sysAcc -= step;
      const p = this.ctl.position;
      // the same crew rules as the server (shared/ship/crew.ts)
      const sims = [...this.shipAuthority.values()];
      const r = crewStep(sims, [{ p: [p.x, p.y, p.z], seated: !!this.seat, o2: this.suitO2 }], step, (sim, ctx) => sim.tick(step, ctx));
      this.suitO2 = r.o2[0];
      sims.forEach((sim, k) => {
        const mirror = this.ships.find((s) => s.id === sim.id);
        if (mirror && mirror.sim !== sim) mirror.sim.st.set(sim.st);
        if (Object.keys(r.results[k].sw).length) this.onShip(sim.id, r.results[k].sw);
        this.offlineEvents(sim, r.results[k].events);
      });
    }
  }

  /** Offline: what the ship systems reported, resolved like the server does (room.ts). */
  private offlineEvents(sim: ShipSim, events: SysEvent[], depth = 0) {
    for (const e of events) {
      if (e.type === 'explode') {
        this.hud?.toast(e.cause);
        // bounded chain, like the server
        if (depth < 3) this.onExplode(0, sim.toWorld(e.at), undefined, { radius: e.radius, damage: e.damage, depth: depth + 1 });
      } else if (e.type === 'say') this.hud?.toast(e.text);
      else if (e.type === 'trip') this.hud?.toast(`Disyuntor saltado por sobrecarga: ${sim.def.subsystems.find((c) => c.id === e.circuit)?.label ?? e.circuit}`);
      else if (e.type === 'destroyed') this.hud?.toast(`${sim.def.parts.find((x) => x.id === e.part)?.name ?? e.part}: destruido`);
    }
  }

  /**
   * Operate a ship control. Offline we are the authority. Online the server decides, but an MFD
   * page button changes nothing but that screen and has no interlock beyond what the mirror already
   * checks, so it is applied locally at once (the server's echo confirms the same value).
   */
  private operate(ship: ShipClient, ctl: number, dir = 0) {
    if (this.offline) return this.localInteract(ship.id, ctl, dir);
    const c = ship.sim.def.controls[ctl];
    if (c?.kind === 'bezel' && !ship.sim.blocked(c, dir)) ship.apply({ [c.key]: ship.sim.next(c, dir) });
    this.net.sendInteract(ship.id, ctl, dir);
  }

  private localInteract(ship: number, ctl: number, dir = 0) {
    const sim = this.shipAuthority.get(ship);
    if (!sim) return;
    const r = sim.interact(ctl, dir);
    if ('reason' in r) this.ships.find((s) => s.id === ship)?.view.refuse(ctl);
    else this.onShip(ship, r.changed, undefined, this.welcome.id);
  }

  private localRepair(ship: number, panel: number, dt: number) {
    const hp = this.shipAuthority.get(ship)?.repair(panel, REPAIR_RATE * dt);
    if (hp !== null && hp !== undefined) this.onShip(ship, undefined, [[panel, hp]], this.welcome.id);
  }

  private localRepairPart(ship: number, part: number, dt: number) {
    const sim = this.shipAuthority.get(ship);
    const hp = sim?.repairPart(part, REPAIR_RATE * dt);
    if (hp == null || !sim) return;
    const mirror = this.ships.find((s) => s.id === ship);
    if (mirror && mirror.sim !== sim) mirror.sim.st[mirror.sim.sys.hpIndex(sim.def.parts[part].id)] = hp;
  }

  private onShip(id: number, sw?: Record<string, number>, hp?: Array<[number, number]>, by?: number) {
    const ship = this.ships.find((s) => s.id === id);
    if (!ship) return;
    ship.apply(sw, hp);
    void by;
  }

  /** Automation: operate a ship control by id (e.g. 'ck.main/ramp'), through the normal path. */
  shipControl(id: string, ship = 0) {
    const s = this.ships[ship];
    const c = s?.sim.def.controls.find((x) => x.id === id);
    if (!s || !c) throw new Error(`no control ${id}`);
    const reason = s.sim.blocked(c);
    if (reason) return reason;
    this.operate(s, c.index);
    return null;
  }

  /** Automation (offline): a rocket blast at a world point, as if the server confirmed it. */
  blast(p: THREE.Vector3) {
    const v: Vec3 = [round(p.x, 2), round(p.y, 2), round(p.z, 2)];
    this.onExplode(this.welcome.id, v, p.y - this.terrain.height(p.x, p.z) < 1.2 ? { x: v[0], z: v[2], r: 2.4, d: 1 } : undefined);
  }

  /** Advance `frames` frames of `dt`; renders only the last one (none with render = false). */
  step(frames = 1, dt = 1 / 30, render = true) {
    for (let i = 0; i < frames; i++) this.tick(dt, render && i === frames - 1);
  }

  private bookShip: ShipClient | null = null;

  /** Point the manual at the ship the astronaut is in (or the nearest one). */
  private bindManual() {
    const at = this.ctl.position;
    const head = at.clone().add(new THREE.Vector3(0, 1, 0));
    const ship = this.ships.find((s) => s.zoneAt(head)) ?? [...this.ships].sort((a, b) => a.position.distanceTo(at) - b.position.distanceTo(at))[0];
    if (!ship || ship === this.bookShip) return;
    if (this.bookShip) this.pointAt(null);
    this.bookShip = ship;
    this.hud.setManual(ship.sim, {
      point: (index) => this.pointAt(index === null ? null : { ship, index }),
      where: () => (ship.zoneAt(this.ctl.position.clone().add(new THREE.Vector3(0, 1, 0))) ? ship.sim.toLocal([this.ctl.position.x, this.ctl.position.y, this.ctl.position.z]) : null),
    });
  }

  /** Control marked from the manual ("Señalar"), cleared when you aim at it. */
  private pointed: { ship: ShipClient; index: number } | null = null;

  private pointAt(p: { ship: ShipClient; index: number } | null) {
    this.pointed?.ship.view.point(null);
    this.pointed = p;
    p?.ship.view.point(p.index);
    if (!p) {
      this.hud.removeTag(-1);
      this.hud.manualBook?.clearPoint();
    }
  }

  get manualOpen() {
    return this.hud?.manualOpen ?? false;
  }

  get pointerLocked() {
    return this.input.locked;
  }

  lockPointer() {
    this.input.lock();
  }

  /** Close the connection (failed start) so the server frees the slot. */
  dispose() {
    this.running = false;
    this.net.close();
  }

  /** Automation hook (tests / screenshots). */
  get debug() {
    return { input: this.input, controller: this.ctl, rig: this.rig, camera: this.camera, remotes: this.remotes, ships: this.ships, interaction: this.interaction, scene: this.scene, game: this };
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
    r.astronaut.attachWeapon(WELDER);
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

  private onExplode(id: number, p: Vec3, edit?: TerrainEdit, blast?: { radius: number; damage: number; depth: number }) {
    const at = new THREE.Vector3(...p);
    if (edit) {
      this.terrain.edits.push(edit);
      this.terrainSys.invalidate(edit.x, edit.z, edit.r);
      this.physics.invalidate(edit.x, edit.z, edit.r);
      this.rocks.invalidate(edit.x, edit.z, edit.r);
    }
    this.rockets.explode(id, at);
    for (const ship of this.ships) ship.cargo.blast(at);
    if (edit && this.physics.readyAt(at.x, at.z)) this.debris.burst(at, 8 + Math.floor(Math.random() * 6));
    // offline: we are the ship authority too
    if (this.offline) {
      for (const [sid, sim] of this.shipAuthority) {
        const r = sim.explode(p, shipBlast(blast?.radius, blast?.damage));
        if (r.hp.length) this.onShip(sid, Object.keys(r.sw).length ? r.sw : undefined, r.hp, id);
        this.offlineEvents(sim, r.events, blast?.depth ?? 0);
      }
    }
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
        this.standUp();
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
    if (this.dead || !this.me.isArmed || this.me.equipped !== 'launcher' || this.seat || now - this.lastFire < LAUNCHER.cooldown) return;
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
    // tools: 1 launcher, 2 welder (press again / X to put it away)
    for (const [code, tool] of [['Digit1', 'launcher'], ['Digit2', 'welder']] as const) {
      if (!input.consume(code)) continue;
      if (this.me.equipped === tool && this.me.isArmed) this.me.setArmed(false);
      else {
        this.me.equip(tool);
        this.me.setArmed(true);
      }
      this.hud.toast(this.me.isArmed ? `${tool === 'welder' ? WELDER.name : LAUNCHER.name} en mano` : 'Herramienta a la espalda');
    }
    if (input.consume('KeyX')) {
      this.me.setArmed(!this.me.isArmed);
      this.hud.toast(this.me.isArmed ? `${this.me.equipped === 'welder' ? WELDER.name : LAUNCHER.name} en mano` : 'Herramienta a la espalda');
    }
    // click: a ship control / seat under the crosshair takes it, otherwise the tool in hand acts
    if (this.fireQueued && !this.interaction.use()) this.tryFire();
    if (input.consume('KeyF')) this.tryFire();
    if (input.consume('KeyE') && !this.interaction.use() && this.seat) this.standUp();
    if (this.seat && input.consume('Space')) this.standUp();
    this.fireQueued = false;
    this.rig.zoomHeld = input.down('Mouse2');
    this.input.sensitivity = 0.0022 / this.rig.magnification;
    this.me.setLamps(this.lamps);
    this.ctl.look(input, this.rig.mode === 'third' && !this.debugOrbit ? this.rig : undefined);
    if (this.ctl.seat) {
      // seated: turn the head, not the seat
      let d = this.ctl.yaw - this.ctl.seat.yaw;
      d = Math.atan2(Math.sin(d), Math.cos(d));
      this.ctl.yaw = this.ctl.seat.yaw + THREE.MathUtils.clamp(d, -1.7, 1.7);
    }
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
    const bodyYaw = this.ctl.seat ? this.ctl.seat.yaw : this.ctl.yaw;
    this.me.root.rotation.y = bodyYaw;
    this.me.update(dt, {
      velocity: this.ctl.velocity,
      yaw: bodyYaw,
      pitch: this.ctl.pitch,
      grounded: this.ctl.grounded,
      crouch: this.ctl.crouch,
      seated: !!this.ctl.seat,
    });
    this.me.root.updateMatrixWorld(true);
    if (this.jointsRecording || this.jointGizmos.group.visible) {
      this.joints.sample(dt);
      this.jointGizmos.update();
      if (this.jointGizmos.group.visible) this.jointPanel.textContent = `ARTICULACIONES (F6) — grados vs reposo, ejes del modelo\n${this.joints.table()}`;
    }
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
        yaw: round(bodyYaw, 3),
        pitch: round(this.ctl.pitch, 3),
        f:
          (this.ctl.grounded ? StateFlags.Grounded : 0) |
          (this.ctl.running ? StateFlags.Running : 0) |
          (this.ctl.crouch ? StateFlags.Crouching : 0) |
          (this.lamps ? StateFlags.Lamps : 0) |
          (this.ctl.jetting ? StateFlags.Jetpack : 0) |
          (this.dead ? StateFlags.Dead : 0) |
          (this.me.isArmed ? StateFlags.Armed : 0) |
          (this.me.equipped === 'welder' ? StateFlags.Welder : 0) |
          (this.welding ? StateFlags.Welding : 0) |
          (this.ctl.seat ? StateFlags.Seated : 0),
      });
    }
    const serverNow = this.net.serverNow();
    for (const r of this.remotes.values()) r.update(dt, serverNow);

    // --- combat & effects ------------------------------------------------------------------------
    for (const hit of this.pendingHits) {
      const p: Vec3 = [round(hit.x, 2), round(hit.y, 2), round(hit.z, 2)];
      // offline: act as our own server (crater, no damage bookkeeping)
      if (this.offline) this.onExplode(this.welcome.id, p, hit.y - this.terrain.height(hit.x, hit.z) < 1.2 ? { x: p[0], z: p[2], r: 2.4, d: 1 } : undefined);
      else this.net.sendHit(p);
    }
    this.pendingHits.length = 0;
    if (this.ctl.jetting) this.emitJet(this.me);
    for (const r of this.remotes.values()) if (r.jetting && !r.dead) this.emitJet(r.astronaut);
    this.particles.update(dt);

    // --- camera & world streaming -----------------------------------------------------------------
    this.rig.update(dt, this.ctl, this.me, (a, b) => {
      let best: number | null = null;
      for (const ship of this.ships) {
        const t = ship.occlude(a, b);
        if (t !== null && (best === null || t < best)) best = t;
      }
      return best;
    });
    if (this.inspectCam) {
      const [x, z, yaw, pitch, hgt = 1.8] = this.inspectCam;
      this.camera.position.set(x, this.terrain.height(x, z) + hgt, z);
      this.camera.quaternion.setFromEuler(new THREE.Euler(pitch, yaw, 0, 'YXZ'));
      // an outside view: show our own helmet too (first person hides it on its layer)
      this.camera.layers.enableAll();
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
    this.me.setEyeClip(this.rig.mode === 'first' && !this.inspectCam && !this.focusCam ? this.camera.position : null);
    this.systems.run('frame', dt);
    this.terrainSys.update(this.camera.position, new THREE.Frustum());
    this.rocks.update(this.ctl.position);
    this.sky.update(this.camera, (performance.now() - this.startTime) / 1000 + 36000);
    this.lighting.update();

    // --- HUD ---------------------------------------------------------------------------------------
    this.hud.updateManual(performance.now() / 1000);
    const markers = [
      markerTo(this.ctl.position, new THREE.Vector3(0, 0, 0), 'Base', '#9fd3ff'),
      markerTo(this.ctl.position, new THREE.Vector3(LANDMARK.x, 0, LANDMARK.z), 'Cráter', '#e8d49c'),
      ...this.ships.map((s) => markerTo(this.ctl.position, s.position, s.sim.def.name, '#4fd8f0')),
    ];
    // a control pointed at from the manual: helmet tag + compass marker until you aim at it
    if (this.pointed) {
      const { ship, index } = this.pointed;
      const c = ship.sim.def.controls[index];
      const at = new THREE.Vector3(...ship.sim.controlWorld(index));
      const t = this.interaction.target;
      if (t?.kind === 'control' && t.ship === ship && ship.sim.def.controls[t.index].key === c.key && t.inReach) this.pointAt(null);
      else {
        markers.push(markerTo(this.ctl.position, at, c.name, '#ffb347'));
        this.hud.updateTag(-1, c.name, at, this.camera, '#ffb347');
      }
    }
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
      o2: this.suitO2,
      reload: !this.me.isArmed ? 0 : this.me.equipped === 'welder' ? 1 : Math.min(1, (performance.now() / 1000 - this.lastFire) / LAUNCHER.cooldown),
      tool: !this.me.isArmed ? 'none' : this.me.equipped === 'welder' ? (this.welding ? 'welding' : 'welder') : 'launcher',
      zoom: this.rig.magnification,
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
      debris: this.debris.count + this.scrap.count,
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
