import * as THREE from 'three';
import { MAX_PLAYERS, CLIENT_SEND_RATE, SUIT_STRIPES, SUN } from '../shared/constants';
import { StateFlags, type EntityKind, type EntityWire, type PlayerInfo, type TerrainMod, type Vec3 } from '../shared/protocol';
import { REPAIR_RATE, shipBlast, ShipSim, SYSTEMS_HZ, type ShipSnapshot } from '../shared/ship/sim';
import { crewStep } from '../shared/ship/crew';
import { FLIGHT_IDLE, MASS, PILOT_KEYS, copyPose, flightReadout, helmSeat, pointVelocity, toWorld, type FlightCommand, type FlightEnv, type Lump } from '../shared/ship/flight';
import { WORLD_FRAME, type Crate } from '../shared/ship/crates';
import { startCrates, startShips } from '../shared/ship/spawn';
import { SITES, siteDir } from '../shared/space/sites';
import { siteGround, spawnPoint, START_SITE } from '../shared/space/world';
import { physicalRegions, regionAt } from '../shared/space/galaxy';
import { arrivalPoint, jumpRefusal, nextRegion } from '../shared/space/jump';
import { DRAG } from '../shared/ship/airflow';
import type { PoseWire } from '../shared/protocol';
import type { V3 } from '../shared/ship/geom';
import type { SysEvent } from '../shared/ship/systems';
import { Frames } from './frames/frames';
import { Bubble } from './frames/bubble';
import { origin } from './render/origin';
import { Crates } from './cargo/crates';
import { AirFlow } from './ship/decompression';
import { Carry, CARRY_REACH } from './cargo/carry';
import { Interaction, type PromptInfo } from './ship/interaction';
import { updateInteriorLights } from './ship/interiorLights';
import { lights } from './render/lightPool';
import { SphereTerrain } from './world/sphereTerrain';
import { altitudeOf, BODIES, bodyAt, bodyById, frameAt, heightAboveGround, inShadow, localFrame, sunDirection, surfaceOf, type CelestialBody, type LocalFrame, type OrbitInfo, type Surfaces } from '../shared/space/body';
import type { SurfaceGround } from '../shared/space/tangent';
import { litMaterial } from './ship/materials';
import { ShipClient } from './ship/ship';
import { Gunnery } from './ship/gunnery';
import { ConsoleCommands } from './ship/commands';
import { ShipCameraFeeds, shipCameraSources } from './ship/cameraScreens';
import { PipSystem } from './render/pip';
import { WorldCapture } from './render/worldCapture';
import { Particles } from './fx/particles';
import { Projectiles } from './fx/projectiles';
import { JointDiagnostics, JointGizmos } from './player/jointDiag';
import { Scheduler } from '../engine/systems';
import { FixedLoop } from '../engine/loop';
import { Debris } from '../engine/debris';
import { DebugOverlay } from '../engine/debug';
import { StepClock } from '../shared/time/stepClock';
import { carryPoint, clearOfHull, type PoseTrack } from '../shared/frames';
import { MotionProbe, type MotionSubject } from './diag/motionProbe';
import { WEAPON_ORDER, WEAPONS, type WeaponDef } from './fx/weapons';
import { objectOf, projectileById, projectileOf, terrainImpact, weaponById } from '../shared/items';
import type { Impact } from './fx/projectiles';
import { NetClient, type Welcome } from './net/netClient';
import { RemotePlayer } from './net/remotePlayer';
import { Astronaut, AstronautAsset } from './player/astronaut';
import { CameraRig } from './player/cameraRig';
import { PlayerController } from './player/controller';
import { Input } from './player/input';
import { RenderPipeline } from './render/pipeline';
import { Hud, type HudData } from './ui/hud';
import { dirFromAzEl, Lighting } from './world/lighting';
import { Physics } from './world/physics';
import { RockField } from './world/rocks';
import { Sky } from './world/sky';
import { TerrainWorkerPool } from './world/workerPool';
import { sfx } from './audio/engine';
import { AudioDirector, type EarState } from './audio/director';
import { CrewSounds } from './audio/crewSounds';
import { SpikeLog } from '../engine/spikes';
import { TERRAIN_SHADOW_RANGE } from './world/terrainGrid';
import { ShadowCull } from './render/shadowCull';

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
/** HUD tag keys of the world's people (clear of players' ids and the pointed control's −1). */
const NPC_TAG = -1_000_000;
/** Their name shows closer than this (m). */
const NPC_TAG_M = 14;
const SUN_ECLIPTIC_LONGITUDE = 150;

/** Top-level client: world, local astronaut, remote crew, network, HUD, frame loop. */
const _gunDir = new THREE.Vector3();
export class Game {
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(72, 1, 0.05, 60000);
  private pipeline: RenderPipeline;
  private input: Input;
  private net: NetClient;
  private hud!: Hud;
  /** Every body's terrain and boulders (one ground per body, the same everywhere). */
  private grounds: Array<{ body: CelestialBody; terrain: SphereTerrain; rocks: RockField }> = [];
  private physics!: Physics;
  private lighting!: Lighting;
  /** Leaves out of the shadow pass the casters whose shadow lands nowhere in view (render/shadowCull.ts). */
  private shadowCull: ShadowCull | null = null;
  private sky!: Sky;
  private asset!: AstronautAsset;
  me!: Astronaut;
  private ctl!: PlayerController;
  private rig!: CameraRig;
  private remotes = new Map<number, RemotePlayer>();
  /** The world's people near us, with a body (docs/MUNDO.md §11): drawn like other astronauts. */
  private npcs = new Map<number, RemotePlayer>();
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
  /**
   * Time of each fixed step's state (ms, server clock): exactly one step apart, kept on the
   * server's clock by slewing (shared/time/stepClock.ts). Everything we send is stamped with it and
   * everything others simulate is drawn at it (docs/MOVIMIENTO.md).
   */
  readonly stepClock = new StepClock(1000 / 60);
  /** Motion probe (F7): jumps of what is drawn, frame by frame (client/diag/motionProbe.ts). */
  motion!: MotionProbe;
  private debris!: Debris;
  private diag!: DebugOverlay;
  private pendingHits: Impact[] = [];
  private offline = false;
  private projectiles!: Projectiles;
  private hp = 100;
  private dead = false;
  private lastFire = -10;
  private fireQueued = false;
  /** The trigger went to the tool in hand (not to a control or a crate): an automatic weapon keeps firing while it is held. */
  private triggerHeld = false;
  /** The weapon mounts of the seat we sit in, if it is a gunner's (client/ship/gunnery.ts). */
  private gunnery = new Gunnery({
    aim: (ship, m, y, p) => {
      if (this.offline) this.shipAuthority.get(ship.id)?.mounts?.aim(m, y, p);
      else this.net.sendAim(ship.id, m, y, p);
    },
    fire: (ship, m, w, o, d) => this.fireMount(ship, m, w, o, d),
    refused: reason => { sfx.ui('ui.deny'); this.hud.toast(reason); },
    pick: (eye, dir, out) => this.pickSight(eye, dir, out),
    focus: (ship, mount, on) => {
      const screen = ship.view.cameraScreens.items.find(s => s.def.camera?.source.ref === mount);
      this.rig.focus = on ? screen?.display.surface ?? null : null;
      if (on) this.rig.fpZoom = 1;
    },
  });
  private consoleCommands = new ConsoleCommands<ShipClient>();
  private cameraFeeds: ShipCameraFeeds[] = [];
  private pip!: PipSystem;
  private sightRay = new THREE.Raycaster();
  private sightObjects: THREE.Object3D[] = [];
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
  /** Reference frames (the moon, every ship's interior). */
  frames!: Frames;
  /** Loose crates and the one in our hands. `game.crates.list` from the console. */
  crates!: Crates;
  private carry!: Carry;
  /** The air of decompressing ships: the pull on bodies and its effects. */
  private air!: AirFlow;
  /** Time the astronaut has been off every ship surface while in a ship frame (s). */
  private offShip = 0;
  /** Flight reports to the server while we pilot (s). */
  private flightAcc = 0;
  private flightHud!: HTMLDivElement;
  /** World-space rotation eased out after a frame change (the view settles into the new tilt). */
  private tilt = new THREE.Quaternion();
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
  /** Sound: the ears, our own sounds and the helmet (client/audio). */
  private audio!: AudioDirector;
  private ear: EarState = { head: new THREE.Vector3(), frame: 0, grounded: false, seated: false, dead: false };

  constructor(private opts: GameOptions) {
    // the menu's click is the gesture browsers want before any sound
    sfx.init();
    // The scene never moves: its children only recompute their matrices when they move (the
    // static parts of the ships, the terrain chunks: never), not every frame.
    this.scene.matrixAutoUpdate = false;
    // Everything placed in world coordinates hangs from the render origin's root, the camera too
    // (render/origin.ts): the scene is drawn relative to a point near the camera.
    this.scene.add(origin.root);
    origin.root.add(this.camera);
    this.pipeline = new RenderPipeline(this.scene, this.camera, { canvas: opts.canvas, quality: opts.quality });
    this.input = new Input(opts.canvas);
    this.net = new NetClient({
      join: (p) => this.addRemote(p, true),
      leave: (id) => this.removeRemote(id),
      state: (id, t, s) => this.remotes.get(id)?.push(t, s),
      disconnect: (reason) => this.hud?.toast(reason),
      // our own shot left the tube when we fired it (tryFire): the echo is not another rocket
      fire: (id, w, o, d, v, fr, m, t) => id !== this.welcome.id && this.onFire(id, w, o, d, v, fr, undefined, m, t),
      explode: (id, p, mod, aboard, k) => this.onExplode(id, p, mod, undefined, aboard, k),
      health: (id, hp, by, dead) => this.onHealth(id, hp, by, dead),
      respawn: (id, spawn) => this.onRespawn(id, spawn),
      ship: (ship, sw, hp, by) => this.onShip(ship, sw, hp, by),
      shipDenied: (ship, ctl) => this.ships.find((s) => s.id === ship)?.refuse(ctl),
      shipState: (ship, d) => this.ships.find((s) => s.id === ship)?.sim.applyState(d),
      shipSync: (snap) => this.ships.find((s) => s.id === snap.id)?.sync(snap),
      shipPose: (ship, t, pose) => this.onShipPose(ship, t, pose),
      pilot: (ship, id) => this.onPilot(ship, id),
      jump: (ship, to, p) => this.onJump(ship, to, p),
      crate: (c, rest) => this.crates?.receive(c, rest),
      spawn: (list) => this.onSpawn(list),
      npcs: (states) => {
        for (const s of states) this.npcs.get(s.id)?.push(s.t, s.s);
      },
      gone: (k, ids) => this.onGone(k, ids),
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
    const offline = (b: CelestialBody) => surfaceOf(b, seed);
    this.welcome = this.offline
      ? { type: 'welcome', id: 1, variant: 0, players: [], spawn: spawnPoint(0, offline), worldSeed: seed, serverTime: 0, mods: [], health: [], ships: this.offlineShips(offline), pilots: [], crates: this.offlineCrates(offline) }
      : await this.net.connect(this.opts.name, undefined, this.opts.worldSeed);
    // the step clock starts on the server's clock (after this it only slews toward it)
    this.stepClock.sync(this.net.serverNow());

    onProgress('Preparando la superficie…');
    const renderer = this.pipeline.renderer;
    const loader = new THREE.TextureLoader();
    // the craters of the game so far, on each body's ground (its sites every machine builds itself)
    for (const b of Object.values(BODIES)) this.surfaces(b)?.mods.setDynamic(this.welcome.mods.filter((m) => m.body === b.def.id));
    const spawnW = this.welcome.spawn;
    this.pool = new TerrainWorkerPool();
    this.physics = await Physics.create(this.pool, this.welcome.worldSeed);
    // the outside's Rapier world is a bubble that follows the player (frames/bubble.ts), laid at the spawn
    this.bubble = new Bubble(this.surfaces, this.loop.step);
    this.bubble.follow(spawnW, [0, 0, 0]);
    this.physics.attach(this.bubble);
    this.frames = new Frames(this.physics, () => this.ships, this.bubble);

    const sunDir = dirFromAzEl(SUN_AZ, SUN_EL);
    const earthDir = dirFromAzEl(EARTH_AZ, EARTH_EL);
    this.lighting = new Lighting(this.scene, this.camera, renderer, sunDir, earthDir, this.opts.quality);
    // only casters whose shadow can land in the view go into the cascades (?noshcull: all of them)
    if (!new URLSearchParams(location.search).has('noshcull')) {
      this.shadowCull = new ShadowCull(this.scene, this.camera, () => this.lighting.csm.lightDirection);
      // each caster only into the cascades it shadows (?nocascull: into all of them, as three does)
      if (!new URLSearchParams(location.search).has('nocascull')) this.shadowCull.useCascades(this.lighting.csm);
    }
    const csm = this.lighting.csm;

    const stars = new Float32Array(await (await fetch('/assets/sky/stars.bin')).arrayBuffer());
    this.sky = new Sky({ sunDir, earthDir, celestial: celestialMatrix(sunDir, SUN_ECLIPTIC_LONGITUDE) }, loader, stars);
    this.sky.setPixelRatio(renderer.getPixelRatio());
    origin.root.add(this.sky.group);

    // every body's ground: its terrain and its boulders, anywhere on it (world/sphereTerrain.ts, world/rocks.ts)
    const aniso = Math.min(8, renderer.capabilities.getMaxAnisotropy());
    const rockMat = RockField.material(loader, csm);
    // a body's ground is made the first time the camera is in its star system (space/galaxy.ts):
    // Sol's now, another system's when a jump gets there
    this.makeGround = (body) => {
      if (!body.surface || this.grounds.some((g) => g.body === body)) return;
      const terrain = new SphereTerrain(this.pool, body, this.welcome.worldSeed, loader, sunDir, csm, aniso);
      if (this.opts.quality === 'low') {
        // its one cascade reaches 90 m: baked ground shadows take over before that, and ground
        // further out casts into nothing
        terrain.setBakedFade(50, 80);
        terrain.setShadowRange(TERRAIN_SHADOW_RANGE.low);
      }
      const rocks = new RockField(this.pool, body, this.welcome.worldSeed, rockMat);
      terrain.group.userData.cat = 'terreno';
      rocks.group.userData.cat = 'rocas';
      origin.root.add(terrain.group, rocks.group);
      this.grounds.push({ body, terrain, rocks });
    };
    for (const body of Object.values(BODIES)) if (regionAt(body.center).index === 0) this.makeGround(body);

    onProgress('Cargando traje EVA…');
    this.asset = await AstronautAsset.load('/assets/astronaut.glb', csm);
    this.me = new Astronaut(this.asset);
    this.me.setLocal(true);
    this.me.setStripeColor(SUIT_STRIPES[this.welcome.variant % SUIT_STRIPES.length]);
    for (const w of WEAPON_ORDER) this.me.attachWeapon(w);
    this.me.setArmed(true);
    this.particles = new Particles(renderer.getPixelRatio());
    // dynamic resolution: sprites and stars follow the pixel ratio
    this.pipeline.onPixelRatio = (pr) => {
      this.particles.setPixelRatio(pr);
      this.sky.setPixelRatio(pr);
    };
    // particles, crates and debris write render-space buffers themselves: they hang from the scene
    this.scene.add(this.particles.group);
    this.projectiles = new Projectiles(this.groundAlt, this.particles, this.frames, () => this.ships);
    origin.root.add(this.projectiles.group);
    const debrisMat = RockField.material(loader, csm);
    this.debris = new Debris(this.physics.rapier, this.physics.world, debrisMat);
    this.scene.add(this.debris.mesh);

    onProgress('Preparando la nave…');
    // hull fragments: thin bent plates
    this.scrap = new Debris(this.physics.rapier, this.physics.world, litMaterial(csm, { color: new THREE.Color().setRGB(0.3, 0.3, 0.31), roughness: 0.5, metalness: 0.6 }), new THREE.BoxGeometry(1.4, 0.12, 1.0));
    this.scene.add(this.scrap.mesh);
    // the ships where the server has them (placed on their pads on the global ground)
    for (const snap of this.welcome.ships ?? []) {
      const ship = new ShipClient(snap, { physics: this.physics, frames: this.frames, surfaces: this.surfaces, groundAlt: this.groundAlt, csm, particles: this.particles, debris: this.scrap });
      this.ships.push(ship);
      origin.root.add(ship.view.root);
    }
    for (const [ship, id] of this.welcome.pilots ?? []) {
      const s = this.ships.find((x) => x.id === ship);
      if (s) s.pilot = id;
    }
    const link = this.offline ? null : { take: (id: number) => this.net.sendCrateTake(id), send: (c: Parameters<NetClient['sendCrate']>[0], rest: boolean) => this.net.sendCrate(c, rest) };
    this.crates = new Crates(this.physics.rapier, this.frames, () => this.ships, this.groundAlt, link, () => this.welcome.id, csm, () => this.stepClock.t);
    this.crates.load(this.welcome.crates ?? []);
    this.air = new AirFlow(this.frames, () => this.ships, this.particles);
    this.scene.add(this.crates.group);
    this.carry = new Carry(this.physics.rapier, origin.root, this.crates, this.frames);
    this.interaction = new Interaction(this.scene, this.particles, {
      interact: (ship, ctl) => this.operate(ship, ctl),
      repair: (ship, panel, dt) => (this.offline ? this.localRepair(ship.id, panel, dt) : this.net.sendRepair(ship.id, panel)),
      repairPart: (ship, part, dt) => (this.offline ? this.localRepairPart(ship.id, part, dt) : this.net.sendRepairPart(ship.id, part)),
      sit: (ship, index) => this.sitDown(ship, index),
    });
    this.diag = new DebugOverlay(this.opts.ui, this.scene);
    this.motion = new MotionProbe(this.opts.ui);
    this.motion.info = () => this.motionInfo();
    this.crates.events.frame = (c, from, to, why) => this.motion.event(`caja ${c.id}`, `${frameName(from)} → ${frameName(to)} (${why})`);
    this.projectiles.onFrame = (id, kind, from, to) => this.motion.event(`${kind} ${id}`, `${frameName(from)} → ${frameName(to)}`);
    this.audio = new AudioDirector(this.camera, this.groundAlt);
    this.projectiles.placeAt = (p, out) => this.audio.placeAt(p, out);
    this.consoleCommands.register('gunnery', (ship, command) => this.gunnery.command(ship, command.target, command.action));
    this.pip = new PipSystem(this.pipeline.renderer, this.scene, new WorldCapture(this.camera, this.sky, this.grounds, this.ships));
    const providers = shipCameraSources();
    for (const ship of this.ships) {
      this.cameraFeeds.push(new ShipCameraFeeds(ship, this.gunnery, providers));
      this.sightObjects.push(ship.view.root);
      for (const screen of ship.view.cameraScreens.items) this.pip.add(screen.display);
    }
    for (const ground of this.grounds) this.sightObjects.push(ground.terrain.group);
    this.pipeline.beforeRender = () => this.pip.renderPending();
    this.me.onStep = (_foot, k) => this.audio.me.step(this.underfoot(), k);
    this.me.onLand = (v) => this.audio.me.land(this.underfoot(), v);
    this.registerSystems();
    this.joints = new JointDiagnostics(this.me);
    this.jointGizmos = new JointGizmos(this.me);
    this.scene.add(this.jointGizmos.group);
    this.jointPanel = document.createElement('pre');
    this.jointPanel.className = 'debug-overlay joints-panel hidden';
    this.opts.ui.appendChild(this.jointPanel);
    window.addEventListener('keydown', (e) => {
      if (e.code === 'KeyJ' && this.input.locked) this.askJump();
    });
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
    origin.root.add(this.me.root);
    origin.root.add(lights.group);
    // categories for the F3 draw-call breakdown
    this.sky.group.userData.cat = 'cielo';
    this.particles.group.userData.cat = 'partículas';
    this.projectiles.group.userData.cat = 'proyectiles';
    this.debris.mesh.userData.cat = 'escombros';
    this.scrap.mesh.userData.cat = 'escombros';
    this.crates.group.userData.cat = 'cajas';
    this.me.root.userData.cat = 'astronautas';
    for (const s of this.ships) s.view.root.userData.cat = 'naves';
    for (const p of this.welcome.players) this.addRemote(p, false);

    // the spawn is a world point on the ground: in the bubble, where the controller lives
    const sl = this.frames.toLocal(WORLD_FRAME, spawnW);
    const spawn = new THREE.Vector3(sl[0], sl[1], sl[2]);
    this.ctl = PlayerController.forBody(this.physics, bodyAt(spawnW).def, spawn);
    // face the landmark (the first site with a compass marker besides the start) on arrival
    this.siteMarkers = this.markersOfSites();
    const look = this.siteMarkers.find((m) => m.site !== START_SITE?.id);
    if (look) {
      const t = this.frames.toLocal(WORLD_FRAME, [look.p.x, look.p.y, look.p.z]);
      this.ctl.yaw = Math.atan2(-(t[0] - sl[0]), -(t[2] - sl[2])) + 0.5;
    }
    this.ctl.pitch = -0.05;
    this.rig = new CameraRig(this.camera, this.groundAlt);
    this.me.root.position.set(spawnW[0], spawnW[1], spawnW[2]);

    this.hud = new Hud(this.opts.ui);
    this.hud.onToast = () => sfx.ui('ui.msg');
    this.hud.root.classList.add('hidden');
    this.flightHud = document.createElement('div');
    this.flightHud.className = 'flight-hud';
    Object.assign(this.flightHud.style, {
      position: 'absolute',
      left: '50%',
      bottom: '11%',
      transform: 'translateX(-50%)',
      font: '12px/1.45 ui-monospace, Consolas, monospace',
      color: '#8ff0c8',
      textShadow: '0 0 4px #000',
      whiteSpace: 'pre',
      pointerEvents: 'none',
      display: 'none',
    });
    this.hud.root.appendChild(this.flightHud);
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
    await this.warmUp(spawnW, spawn);
    this.hud.root.classList.remove('hidden');
    this.spawned = true;
    this.running = true;
    // ?manual → no rAF loop; automation drives the simulation with step()
    if (!new URLSearchParams(location.search).has('manual')) this.frame();
  }

  /** Fixed-step simulation, in order. New gameplay systems register here (see AGENTS.md). */
  private registerSystems() {
    const S = this.systems;
    S.add({
      name: 'flight',
      phase: 'fixed',
      order: 8,
      update: (h) => {
        // the time this step's state will belong to (what we send is stamped with it, what others
        // simulate is drawn at it)
        this.stepClock.step();
        if (this.offline) this.offlineSystems(h);
        // the frames move first (the bubble, the ships); the bodies in them catch up in 'physics'
        this.bubble.step(h);
        this.flyShips(h);
        this.crates.beforeStep(h);
      },
    });
    S.add({
      name: 'physics',
      phase: 'fixed',
      order: 10,
      update: (h) => {
        this.physics.step(h);
        for (const ship of this.ships) if (ship.needsInteriorStep(this.ctl.frame === ship.id)) ship.space.step(h);
        this.crates.afterStep(h);
      },
    });
    S.add({
      name: 'air',
      phase: 'fixed',
      order: 15,
      update: (h) => {
        // air rushing to a breach: it drags the astronaut (strapped in a seat it can't) and the crates
        this.air.step();
        const ctl = this.ctl;
        const a = this.seat || this.dead ? null : this.air.accel(ctl.frame, [ctl.position.x + ctl.up.x * 0.9, ctl.position.y + ctl.up.y * 0.9, ctl.position.z + ctl.up.z * 0.9], ctl.crouch ? DRAG.crouched : DRAG.standing);
        if (a) ctl.wind.set(a[0], a[1], a[2]);
        else ctl.wind.set(0, 0, 0);
        this.crates.airPull(h, (fr, p, cda) => this.air.accel(fr, p, cda), this.firstAboard());
      },
    });
    S.add({
      name: 'player',
      phase: 'fixed',
      order: 20,
      update: (h) => {
        this.prevPos.copy(this.ctl.position);
        this.ctl.gravity.copy(this.frames.gravity(this.ctl.frame));
        this.ctl.ignore = this.carry.held?.collider.handle ?? null;
        this.ctl.update(h, this.input);
        // leaning into a loose crate: we simulate it now, so it can be pushed
        for (const handle of this.ctl.touched) {
          const c = this.crates.byCollider(handle);
          if (c && !this.crates.mine(c)) this.crates.take(c);
        }
        this.playerFrame(h);
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
      name: 'frames',
      phase: 'fixed',
      order: 35,
      // The physics bubble stays round the player, re-laid only here: at the end of the step's
      // motion, when the frames and every body in them are at the same time. Everything in it is
      // carried with both of its steps, so nothing drawn jumps (docs/MOVIMIENTO.md).
      update: () => this.followBubble(),
    });
    S.add({
      name: 'ships',
      phase: 'fixed',
      order: 30,
      update: (h) => {
        for (const ship of this.ships) ship.fixed(h);
      },
    });
    S.add({
      name: 'gunnery',
      phase: 'fixed',
      order: 39,
      update: (h) => this.gunnery.fixed(h),
    });
    S.add({
      name: 'station-controls',
      phase: 'fixed',
      order: 39,
      update: () => { if (!this.dead && this.input.locked) this.gunnery.keys(this.input, performance.now() / 1000); },
    });
    S.add({
      name: 'projectiles',
      phase: 'fixed',
      order: 40,
      update: (h) => {
        // centres of the astronauts: 0.95 m up from the feet ("up": the deck's aboard, away from the body outside)
        const crew: Array<{ id: number; p: V3 }> = [];
        for (const r of this.remotes.values()) if (!r.dead) crew.push({ id: r.info.id, p: this.crewCentre(r.frame, r.frame === WORLD_FRAME ? r.position : r.local) });
        if (!this.dead) crew.push({ id: this.welcome.id, p: this.crewCentre(this.ctl.frame, this.ctl.frame === WORLD_FRAME ? this.myWorld() : this.ctl.position) });
        this.pendingHits.push(...this.projectiles.update(h, this.welcome.id, crew));
      },
    });
    // --- per rendered frame (after the camera is placed) ---
    S.add({
      name: 'gunnery-aim',
      phase: 'frame',
      order: 38,
      update: () => this.gunnery.frame(performance.now() / 1000, this.camera.position, this.camera.getWorldDirection(_gunDir)),
    });
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
          tool: this.inHand()?.action.kind === 'weld' && !this.seat,
          holdRepair: this.input.down('Mouse0') && !this.dead,
          seated: !!this.seat,
          now: performance.now() / 1000,
          disabled: this.dead || !!this.inspectCam,
        });
        this.hud.setPrompt(prompt ?? this.objectPrompt());
        this.welding = this.interaction.repairing;
      },
    });
    S.add({
      name: 'flight-displays',
      phase: 'frame',
      order: 85,
      update: () => {
        this.camera.getWorldPosition(_eye);
        for (const ship of this.ships) ship.frameScreens(_eye);
      },
    });
    S.add({
      name: 'sky',
      phase: 'frame',
      order: 90,
      update: () => {
        this.sky.update(this.camera, (performance.now() - this.startTime) / 1000 + 36000);
        this.watchRegion();
      },
    });
    S.add({
      name: 'air-fx',
      phase: 'frame',
      order: 90,
      update: (dt) => this.rig.shake(this.air.frame(dt, this.me.eyePosition(new THREE.Vector3()))),
    });
    S.add({
      name: 'ship-view',
      phase: 'frame',
      order: 90,
      update: (dt) => {
        for (const r of this.remotes.values()) if (r.welding && !r.dead) this.remoteWeld(r, dt);
        updateInteriorLights(this.camera);
        // the real spots and points (helmet lamps, landing floodlights, blasts): a fixed pool
        this.projectiles.frame(this.loop.alpha);
        lights.update(this.camera);
        // portals: from the room the camera is in, the rooms it can see into and whether it sees out
        const eye = origin.worldOf(this.camera, _eye);
        this.camera.updateMatrixWorld();
        _pv.multiplyMatrices(this.camera.projectionMatrix, this.camera.matrixWorldInverse);
        let aboard: ShipClient | null = null;
        let outside = true;
        const now = performance.now() / 1000;
        for (const s of this.ships) {
          const seen = aboard ? null : s.portalView(eye, _pv, _rooms, now);
          if (!seen) {
            s.view.showRooms(null);
            continue;
          }
          aboard = s;
          outside = seen.outside;
          s.view.showRooms(_rooms);
        }
        this.sky.group.visible = outside;
        for (const s of this.ships) s.view.root.visible = s === aboard || outside;
        // every body's ground (nodes beyond its horizon cost nothing), the boulders only close by
        const body = bodyAt([eye.x, eye.y, eye.z]);
        for (const g of this.grounds) {
          g.terrain.group.visible = outside;
          g.rocks.group.visible = outside && this.nearGround && g.body === body;
          if (outside) g.terrain.update(this.camera);
        }
        // a view far enough to reach the body's limb
        const d = altitudeOf(body, [eye.x, eye.y, eye.z]) + body.radius;
        const limb = Math.sqrt(Math.max(0, d * d - body.radius * body.radius));
        const far = Math.max(60000, limb * 1.25);
        if (Math.abs(far - this.camera.far) > this.camera.far * 0.1) {
          this.camera.far = far;
          this.camera.updateProjectionMatrix();
        }
        // the body between the camera and the sun (the night side of an orbit): no sunlight, and
        // no sunlit regolith to fill the shadows
        const day = inShadow(body, [eye.x, eye.y, eye.z], SUN_WORLD) ? 0 : 1;
        this.lighting.setDaylight(day);
        // eyes adapt inside: the regolith glow of the environment map is outside
        const inside = aboard !== null;
        this.envInside += ((inside ? 1 : 0) - this.envInside) * Math.min(1, dt * 3);
        this.scene.environmentIntensity = (1 - 0.65 * this.envInside) * (0.03 + 0.97 * day);
      },
    });
    // sound: the ears where the camera is, every source that follows the game (client/audio)
    S.add({
      name: 'audio',
      phase: 'frame',
      order: 95,
      update: (dt) => this.audioFrame(dt),
    });
    S.add({
      name: 'camera-feeds',
      phase: 'frame',
      order: 96,
      update: () => {
        for (const feed of this.cameraFeeds) feed.frame();
        this.pip.request(performance.now() / 1000, this.camera);
      },
    });
  }

  /** What the local astronaut stands on (a surface material, client/audio/surfaces.ts). */
  private underfoot(): string {
    const h = this.ctl.groundHandle;
    if (h !== null) {
      if (this.crates.byCollider(h)) return 'crate';
      for (const s of this.ships) if (s.physics.ownsInner(h) || s.physics.ownsOuter(h)) return 'deck';
    }
    if (this.ctl.frame !== WORLD_FRAME) return 'deck';
    return bodyAt(this.me.root.position.toArray()).def.ground ?? 'regolith';
  }

  /** Every frame: the ears, our boots, pack, tool and helmet, everyone else's, then every ship and the engine. */
  private audioFrame(dt: number) {
    const a = this.audio;
    const ear = this.ear;
    this.me.eyePosition(ear.head);
    ear.frame = this.ctl.frame;
    ear.grounded = this.ctl.grounded;
    ear.seated = !!this.seat;
    ear.dead = this.dead;
    // ours: through the suit
    const ctl = this.ctl;
    a.feetAt(a.me.feet, ctl.frame, this.me.root.position, ctl.renderPosition, ctl.grounded);
    const back = this.me.partPosition('chest', _aBack);
    _aJet[0] = back.x;
    _aJet[1] = back.y;
    _aJet[2] = back.z;
    const t = this.interaction.target;
    const weld = this.welding && t ? t.point : null;
    if (weld) {
      _aWeld[0] = weld.x;
      _aWeld[1] = weld.y;
      _aWeld[2] = weld.z;
    }
    a.me.update(ctl.jetting && !this.dead, _aJet, weld ? _aWeld : null);
    // everyone else's: through whatever carries them here
    for (const r of this.remotes.values()) {
      const s = r.sounds;
      if (!s) continue;
      a.feetAt(s.feet, r.frame, r.position, r.local, r.grounded);
      const c = r.astronaut.partPosition('chest', _aBack);
      _aJet[0] = c.x;
      _aJet[1] = c.y;
      _aJet[2] = c.z;
      const m = r.welding && !r.dead ? r.astronaut.muzzle(_aTip) : null;
      if (m) {
        _aWeld[0] = m.x;
        _aWeld[1] = m.y;
        _aWeld[2] = m.z;
      }
      s.update(r.jetting && !r.dead, _aJet, m ? _aWeld : null);
    }
    // the helmet: breathing follows the work
    const speed = Math.hypot(ctl.velocity.x, ctl.velocity.z);
    const h = _aHelm;
    h.alive = !this.dead;
    h.work = ctl.jetting ? 1 : speed > 0.5 ? (ctl.running ? 0.8 : 0.35) : 0;
    h.o2 = this.suitO2;
    h.fuel = ctl.fuel;
    h.jetting = ctl.jetting;
    a.helmet.update(dt, h);
    a.update(dt, ear);
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
        this.particles.emit('glow', { pos: h.point, vel: new THREE.Vector3().randomDirection().multiplyScalar(0.6 + Math.random() * 2).addScaledVector(h.normal, 1), carry: ship.render.v, color: [2.4, 3, 4.5], life: 0.1 + Math.random() * 0.3, size: 0.015, gravity: 1 });
      }
      return;
    }
  }

  /** Sit in a ship seat: pinned to it (in the ship's frame), tool slung, head free to look around. */
  sitDown(ship: ShipClient, index: number) {
    if (this.dead || this.seat) return;
    const pose = ship.seatPose(index);
    const at = new THREE.Vector3(...pose.pos);
    // someone already there?
    for (const r of this.remotes.values()) if (r.seated && r.frame === ship.id && r.local.distanceTo(at) < 0.35) return this.hud.toast('Asiento ocupado');
    if (this.carry.held) this.carry.drop();
    this.setFrame(ship.id);
    this.seat = { ship, index };
    ship.sounds.playAt('seat.buckle', pose.pos);
    if (this.ctl.crouch) this.input.setKey('KeyC', false);
    this.ctl.seat = { pos: at, yaw: pose.yaw };
    this.ctl.teleport(at);
    this.prevPos.copy(at);
    this.ctl.yaw = pose.yaw;
    if (this.gunnery.take(ship, index)) this.hud.toast('ARTILLERO · consola delante · G apuntar · T disparar');
    if (this.atHelm()) {
      this.hud.toast(PILOT_KEYS);
      if (this.offline) this.onPilot(ship.id, this.welcome.id);
      else this.net.sendPilot(ship.id, true);
    }
  }

  standUp() {
    if (!this.seat) return;
    const helm = this.atHelm();
    const ship = this.seat.ship;
    const pose = ship.seatPose(this.seat.index);
    ship.sounds.playAt('seat.buckle', pose.pos, 0.8, 1.1);
    this.gunnery.leave();
    this.seat = null;
    this.ctl.seat = null;
    this.ctl.teleport(new THREE.Vector3(pose.exit[0], pose.exit[1] + 0.03, pose.exit[2]));
    this.prevPos.copy(this.ctl.position);
    if (!helm) return;
    if (this.offline) this.onPilot(ship.id, 0);
    else this.net.sendPilot(ship.id, false);
  }

  /**
   * This client answers for what nobody simulates (a crate the air starts to drag): offline, or
   * the lowest player id online — one client, the same on every client, no server round.
   */
  private firstAboard(): boolean {
    if (this.offline) return true;
    for (const id of this.remotes.keys()) if (id < this.welcome.id) return false;
    return true;
  }

  /** The star system the camera is in (space/galaxy.ts): the sky follows it, its ground is made, arriving is announced. */
  private region = 0;
  /** Makes a body's ground (its terrain and boulders) if it has none yet. */
  private makeGround: ((b: CelestialBody) => void) | null = null;
  private watchRegion() {
    const c = origin.toWorld(_v3.copy(this.camera.position));
    const r = regionAt([c.x, c.y, c.z]);
    if (r.index === this.region) return;
    this.region = r.index;
    this.sky.setHome(r.index === 0);
    for (const b of Object.values(BODIES)) if (regionAt(b.center).index === r.index) this.makeGround?.(b);
    this.hud?.toast(`Sistema ${r.system.name} · estrella ${r.system.star.cls}`);
  }

  /** J at the helm: jump to the next system (the server decides; offline, here). */
  private askJump() {
    const s = this.seat;
    if (!s || !this.atHelm()) return;
    const ship = s.ship;
    const pose = (this.shipAuthority.get(ship.id) ?? ship.sim).pose;
    const to = nextRegion(pose.p);
    const why = jumpRefusal(pose.p, ship.sim.landed, to.index);
    if (why) return this.hud.toast(`Salto: ${why}`);
    if (this.offline) this.onJump(ship.id, to.index, arrivalPoint(to));
    else this.net.sendJump(ship.id, to.index);
  }

  /**
   * A ship jumped (shared/space/jump.ts): it is at `p`, at rest, and everything aboard with it (their
   * places are the ship's space). We fly it: our flight goes on from there. The rest: its stream
   * starts again there. Nothing is drawn between the two systems.
   */
  private onJump(id: number, to: number, p: Vec3) {
    const ship = this.ships.find((s) => s.id === id);
    if (!ship) return;
    for (const pose of [this.shipAuthority.get(id)?.pose, ship.sim.pose]) {
      if (!pose) continue;
      for (let i = 0; i < 3; i++) {
        pose.p[i] = p[i];
        pose.v[i] = 0;
        pose.w[i] = 0;
      }
    }
    this.shipAuthority.get(id)?.flight.wake();
    ship.sim.flight.wake();
    ship.playback.clear();
    ship.beginStep();
    ship.rebase();
    if (this.seat?.ship === ship || this.ctl.frame === id) this.hud.toast(`Salto a ${physicalRegions()[to]?.system.name ?? '?'}`);
  }

  /** Sitting in the seat that flies the ship. */
  private atHelm(): boolean {
    const s = this.seat;
    return !!s && helmSeat(s.ship.sim.def) === s.ship.sim.def.seats[s.index]?.id;
  }

  /** An astronaut's centre in the world: 0.95 m up from its feet in its frame (`p`: frame coordinates; the world for frame 0). */
  private crewCentre(fr: number, p: THREE.Vector3): V3 {
    if (fr !== WORLD_FRAME && this.frames.ship(fr)) return this.frames.toWorld(fr, [p.x, p.y + 0.95, p.z]);
    const c = this.upFrom(p, 0.95);
    return [c.x, c.y, c.z];
  }

  /** The tool in hand (drawn), if any. */
  private inHand(): WeaponDef | null {
    return this.me.isArmed ? (WEAPONS[this.me.equipped] ?? null) : null;
  }

  /** What a loose object in the hands or in sight offers (the same for every kind, as its catalog allows). */
  private objectPrompt(): PromptInfo | null {
    if (this.dead || this.seat || this.inspectCam) return null;
    const held = this.carry.held;
    if (held) {
      const def = objectOf(held.spec);
      return { title: def.name, state: 'EN LAS MANOS', tone: 'on', hint: `[CLIC] dejar · [E] soltar${def.handling.throw ? ' · [Q] lanzar' : ''}` };
    }
    const c = this.crateInSight();
    if (!c) return null;
    const def = objectOf(c.spec);
    return { title: def.name, state: `${Math.round(c.spec.mass)} kg`, tone: 'on', hint: def.handling.grab ? '[E] coger' : 'No se puede coger' };
  }

  /** The helmet's tool bar: what is in hand and whether it is ready (its catalog's words). */
  private toolReadout(): HudData['tool'] {
    if (this.gunnery.active) return this.gunnery.readout();
    const w = this.inHand();
    if (!w) return null;
    if (w.action.kind !== 'fire') return { label: this.welding ? (w.hud.busy ?? w.hud.ready) : w.hud.ready, value: 1, state: 'fuel' };
    // a fast weapon is simply ready: its bar would only flicker
    const k = w.cooldown < 0.5 ? 1 : Math.min(1, (performance.now() / 1000 - this.lastFire) / w.cooldown);
    return k >= 1 ? { label: w.hud.ready, value: 1, state: 'ready' } : { label: 'RECARGANDO', value: k, state: 'reload' };
  }

  /** A tool drawn or put away: its sound and a line on the helmet. */
  private toolToast() {
    this.audio.me.play(this.me.isArmed ? 'tool.equip' : 'tool.stow');
    const w = WEAPONS[this.me.equipped];
    this.hud.toast(this.me.isArmed && w ? `${w.name} en mano` : 'Herramienta a la espalda');
  }

  /** Default ship states for ?offline (what a fresh server would send: shared/ship/spawn.ts). */
  private offlineShips(surfaces: Surfaces): ShipSnapshot[] {
    return startShips(surfaces).map((sim) => {
      this.shipAuthority.set(sim.id, sim);
      return sim.snapshot();
    });
  }

  /** The crates a fresh server would start with (same ids). */
  private offlineCrates(surfaces: Surfaces): Crate[] {
    return startCrates([...this.shipAuthority.values()], surfaces);
  }

  /** Offline: this client runs the ship machinery and its own suit, like the server would. */
  private offlineSystems(h: number) {
    this.sysAcc += h;
    const step = 1 / SYSTEMS_HZ;
    while (this.sysAcc >= step) {
      this.sysAcc -= step;
      const p = this.myWorld();
      // the same crew rules as the server (shared/ship/crew.ts)
      const sims = [...this.shipAuthority.values()];
      const r = crewStep(sims, [{ p: [p.x, p.y, p.z], seated: !!this.seat, o2: this.suitO2 }], step, (sim, ctx) => sim.tick(step, ctx));
      this.suitO2 = r.o2[0];
      sims.forEach((sim, k) => {
        const mirror = this.ships.find((s) => s.id === sim.id);
        if (mirror && mirror.sim !== sim) {
          mirror.sim.st.set(sim.st);
          mirror.sim.version++;
        }
        const { sw, hp } = r.results[k];
        let moved = false;
        for (const _ in sw) {
          moved = true;
          break;
        }
        if (moved || hp.length) this.onShip(sim.id, sw, hp.length ? hp : undefined);
        this.offlineEvents(sim, r.results[k].events);
      });
    }
  }

  /** A ship's pose from the network (the server, or its pilot through the server). */
  private onShipPose(id: number, t: number, w: PoseWire) {
    const ship = this.ships.find((s) => s.id === id);
    if (!ship || ship.pilot === this.welcome.id) return;
    ship.playback.push(t, { p: [...w.p], q: [...w.q], v: [...w.v], w: [...w.w] }, w.landed, w.pad);
  }

  /** Who flies a ship now. Taking it: the flight model starts from where the stream left it. */
  private onPilot(id: number, pilot: number) {
    const ship = this.ships.find((s) => s.id === id);
    if (!ship) return;
    const me = this.welcome.id;
    const was = ship.pilot;
    ship.pilot = pilot;
    if (pilot === me && was !== me) {
      (this.shipAuthority.get(id) ?? ship.sim).flight.resume();
      ship.playback.clear();
    }
    // we flew it until now: our last pose is where the stream starts (no freeze until the first
    // pose arrives, no jump back to where the network had it)
    if (was === me && pilot !== me && !this.shipAuthority.has(id)) ship.playback.seed(this.stepClock.t, ship.sim.pose, ship.sim.landed, ship.sim.onPad);
    if (pilot !== me && this.seat?.ship === ship && this.atHelm() && pilot !== 0) this.hud.toast('Otro tripulante lleva los mandos');
  }

  /** Frame 0: the outside's Rapier world, laid round the player (frames/bubble.ts). */
  private bubble!: Bubble;

  /** The ground of each body in this world: its surface with every modifier (the sites', the craters). */
  readonly surfaces: Surfaces = (b) => surfaceOf(b, this.welcome.worldSeed);

  /**
   * Height of a world point over the ground under it (m, along the local vertical), on whatever
   * body it is near. Whoever asks "is it on the ground" asks here, never `y` against a height map.
   */
  readonly groundAlt = (p: readonly number[]): number => {
    const b = bodyAt(p);
    return heightAboveGround(b, p, this.surfaces(b));
  };

  /** Automation: world y of the ground under world (x, z) (straight toward the centre of the body there). */
  groundY(x: number, z: number) {
    const b = bodyAt([x, 0, z]);
    const s = this.surfaces(b);
    const c = b.center;
    let y = 0;
    for (let k = 0; k < 3; k++) {
      const l = Math.hypot(x - c[0], y - c[1], z - c[2]);
      const d = [(x - c[0]) / l, (y - c[1]) / l, (z - c[2]) / l];
      y = c[1] + d[1] * (b.radius + (s ? s.height(d) : 0));
    }
    return y;
  }

  /** `d` metres up (away from the body) from world point `p` (a new vector). */
  private upFrom(p: THREE.Vector3, d: number) {
    const b = bodyAt([p.x, p.y, p.z]);
    const u = new THREE.Vector3(p.x - b.center[0], p.y - b.center[1], p.z - b.center[2]);
    return u.setLength(d).add(p);
  }

  /** The sites with a marker on the compass, where they are (their ground, world). */
  private siteMarkers: Array<{ site: string; label: string; color: string; p: THREE.Vector3 }> = [];

  private markersOfSites() {
    const out: Array<{ site: string; label: string; color: string; p: THREE.Vector3 }> = [];
    for (const site of SITES) {
      if (!site.compass) continue;
      const b = bodyById(site.body);
      const d = siteDir(site, bodyById);
      const r = b.radius + (this.surfaces(b)?.height(d) ?? 0);
      out.push({ site: site.id, label: site.compass.label, color: site.compass.color, p: new THREE.Vector3(b.center[0] + d[0] * r, b.center[1] + d[1] * r, b.center[2] + d[2] * r) });
    }
    return out;
  }

  /** The start site's frame for `?cam=` (x, z on its horizon). */
  private camSite: SurfaceGround | null = null;

  /** The player's world position and velocity (this step), for the bubble and the rockets. */
  private playerMotion(): { p: V3; v: V3 } {
    const ctl = this.ctl;
    const lp: V3 = [ctl.position.x, ctl.position.y, ctl.position.z];
    const p = this.frames.toWorld(ctl.frame, lp);
    const fv = this.frames.velocityAt(ctl.frame, lp);
    const dv = this.frames.dirToWorld(ctl.frame, [ctl.velocity.x, ctl.velocity.y, ctl.velocity.z]);
    return { p, v: [fv[0] + dv[0], fv[1] + dv[1], fv[2] + dv[2]] };
  }

  /**
   * Keep the physics bubble round the player; carry everything in it when it is re-laid — its
   * state now and what is drawn of the step before (at the end of a step: every body in the bubble
   * at the same time as the bubble; see the 'frames' system).
   */
  private followBubble(force?: { p: V3; v: V3 }) {
    const m = force ?? this.playerMotion();
    const mode = this.bubble.mode;
    if (!this.bubble.follow(m.p, m.v)) return;
    const frames = this.frames;
    const b = this.bubble;
    const ctl = this.ctl;
    this.motion?.event('burbuja', `${mode}→${b.mode} v ${Math.round(Math.hypot(b.pose.v[0], b.pose.v[1], b.pose.v[2]))} m/s`);
    if (ctl.frame === WORLD_FRAME) {
      const n = frames.rebase([ctl.position.x, ctl.position.y, ctl.position.z], [ctl.velocity.x, ctl.velocity.y, ctl.velocity.z]);
      const yaw = carryYaw(b.from.q, b.pose.q, ctl.yaw);
      this.easeTilt(b.from.q, ctl.yaw, b.pose.q, yaw);
      const was = ctl.seat;
      ctl.moveTo(WORLD_FRAME, this.physics.world, new THREE.Vector3(...n.p), new THREE.Vector3(...n.v), yaw);
      ctl.seat = was;
      ctl.gravity.copy(b.gravity);
      // the step before, with the bubble's poses of the step before: drawn unbroken
      _pp[0] = this.prevPos.x;
      _pp[1] = this.prevPos.y;
      _pp[2] = this.prevPos.z;
      if (force) this.prevPos.copy(ctl.position);
      else {
        frames.rebasePrev(_pp);
        this.prevPos.set(_pp[0], _pp[1], _pp[2]);
      }
    }
    this.crates.rebase();
    const carry = (p: V3, v: V3, q: [number, number, number, number]) => frames.rebase(p, v, q);
    // angular velocities: old axes → world → new axes (the bubble's poses)
    const turnW = (w: V3): number[] => {
      const wq = b.from.q;
      const nq = b.pose.q;
      const t = new THREE.Vector3(w[0], w[1], w[2]).applyQuaternion(new THREE.Quaternion(wq[0], wq[1], wq[2], wq[3])).applyQuaternion(new THREE.Quaternion(nq[0], nq[1], nq[2], nq[3]).invert());
      return [t.x, t.y, t.z];
    };
    const track = (t: PoseTrack) => frames.rebaseTrack(t);
    this.debris.rebase(carry, turnW, track);
    this.scrap.rebase(carry, turnW, track);
    this.physics.rebase(b.from, b.pose);
    for (const ship of this.ships) ship.rebase();
  }

  /** Where the bubble's coordinates sit in render space (debug lines), from its pose as drawn. */
  private bubbleMatrix(pose: { p: readonly number[]; q: readonly number[] } | null) {
    const p = pose?.p ?? [0, 0, 0];
    const q = pose?.q ?? [0, 0, 0, 1];
    return _bm.compose(_fb.set(p[0] - origin.x, p[1] - origin.y, p[2] - origin.z), _bq.set(q[0], q[1], q[2], q[3]), _one);
  }

  /** The view keeps the old tilt of a frame (`qa`, heading `ya`) and eases into the new one's (no lurch). */
  private easeTilt(qa: readonly number[], ya: number, qb: readonly number[], yb: number) {
    const Y = _up;
    const before = new THREE.Quaternion(qa[0], qa[1], qa[2], qa[3]).multiply(new THREE.Quaternion().setFromAxisAngle(Y, ya));
    const after = new THREE.Quaternion(qb[0], qb[1], qb[2], qb[3]).multiply(new THREE.Quaternion().setFromAxisAngle(Y, yb));
    this.tilt.copy(this.tilt.clone().multiply(before).multiply(after.invert()));
  }
  /** The camera is low over the ground (rocks worth scattering and drawing). */
  private nearGround = true;

  /** Per ship: the lumps aboard and the crew ones, reused every fixed step. */
  private aboardList = new Map<number, { out: Lump[]; crew: Array<{ m: number; c: [number, number, number]; group: 'tripulación' }> }>();

  /** Mass aboard a ship its data doesn't know: us, the crew in its frame and its crates (reused: read it right away). */
  private aboard(shipId: number): Lump[] {
    let a = this.aboardList.get(shipId);
    if (!a) this.aboardList.set(shipId, (a = { out: [], crew: [] }));
    const out = a.out;
    out.length = 0;
    this.crates.aboard(shipId, out);
    let k = 0;
    const crew = (x: number, y: number, z: number) => {
      const l = (a!.crew[k++] ??= { m: MASS.crewKg, c: [0, 0, 0], group: 'tripulación' });
      l.c[0] = x;
      l.c[1] = y;
      l.c[2] = z;
      out.push(l);
    };
    if (this.ctl.frame === shipId && !this.dead) crew(this.ctl.position.x, this.ctl.position.y, this.ctl.position.z);
    for (const r of this.remotes.values()) if (r.frame === shipId && !r.dead) crew(r.local.x, r.local.y, r.local.z);
    return out;
  }

  /** What the flight model is given each step (one object, refilled). */
  private flightEnv: FlightEnv & { extra: readonly Lump[] } = { extra: [], surface: null };

  /**
   * One fixed step of every ship's pose. We fly the ships we pilot (offline: all of them — the idle
   * stick holds the others where they are); the rest play the network's pose stream back. Then the
   * hull's lunar-world body sweeps to the pose and the interior feels the motion.
   */
  private flyShips(h: number) {
    const me = this.welcome.id;
    const cmd = this.pilotCommand();
    const env = this.flightEnv;
    const focus = this.myWorld(this.focusAt);
    for (const ship of this.ships) {
      ship.beginStep();
      // the ground of the body it flies round
      env.surface = this.surfaces(bodyAt(ship.sim.pose.p));
      const auth = this.shipAuthority.get(ship.id);
      const mine = ship.pilot === me && this.seat?.ship === ship && this.atHelm();
      if (auth) {
        // offline: the authority flies, the mirror shows it
        env.extra = this.aboard(ship.id);
        auth.flight.step(h, mine ? cmd : FLIGHT_IDLE, env);
        copyPose(ship.sim.pose, auth.pose);
        ship.sim.landed = auth.landed;
        ship.sim.onPad = auth.onPad;
        const f = ship.sim.flight;
        f.agl = auth.flight.agl;
        f.telemetry = auth.flight.telemetry;
        f.mp = auth.flight.mp;
      } else if (ship.pilot === me) {
        env.extra = this.aboard(ship.id);
        ship.sim.flight.step(h, mine ? cmd : FLIGHT_IDLE, env);
      } else {
        // drawn in the present like everything we simulate (net/posePlayback.ts)
        const r = ship.playback.step(this.stepClock.t, ship.sim.pose);
        if (r) {
          ship.sim.landed = r.landed;
          ship.sim.onPad = r.pad;
        }
      }
      ship.posed(h, focus);
    }
    // report the ships we fly (~30 Hz)
    this.flightAcc += h;
    if (this.offline || this.flightAcc < 1 / 30) return;
    this.flightAcc = 0;
    // the time of the step this pose belongs to, not of the frame that happened to run it
    const t = this.stepClock.t;
    for (const ship of this.ships) {
      if (ship.pilot !== me) continue;
      const s = ship.sim;
      const r4 = (n: number) => Math.round(n * 1e4) / 1e4;
      const r5 = (n: number) => Math.round(n * 1e5) / 1e5;
      this.net.sendFlight(ship.id, t, Math.round(s.flight.agl * 100) / 100, s.flight.outputs(), {
        p: s.pose.p.map(r4) as V3,
        q: s.pose.q.map(r5) as [number, number, number, number],
        v: s.pose.v.map(r4) as V3,
        w: s.pose.w.map(r5) as V3,
        landed: s.landed,
        pad: s.onPad,
      });
    }
  }

  /** Stick and translation keys, only from the helm seat (see PILOT_KEYS). */
  private pilotCommand(): FlightCommand {
    if (!this.seat || this.dead || !this.atHelm() || this.manualOpen) return FLIGHT_IDLE;
    const axis = (pos: string, neg: string) => (this.input.down(pos) ? 1 : 0) - (this.input.down(neg) ? 1 : 0);
    return {
      surge: axis('KeyW', 'KeyS'),
      yaw: axis('KeyD', 'KeyA'),
      heave: axis('KeyR', 'KeyF'),
      sway: axis('KeyC', 'KeyZ'),
      pitch: axis('ArrowDown', 'ArrowUp'),
      roll: axis('ArrowRight', 'ArrowLeft'),
    };
  }

  /** P at the helm: the autopilot master switch, through the normal control path. */
  private toggleAutopilot() {
    const ship = this.seat?.ship;
    // every face of the autopilot drum has its master: the one turned out
    const sw = ship?.sim.sw;
    const c = ship?.sim.def.controls.find((x) => x.key === 'ap.on' && (!x.drum || (sw?.[x.drum.key] ?? 0) === x.drum.face));
    if (!ship || !c) return this.hud.toast('Esta nave no tiene piloto automático');
    const reason = ship.sim.blocked(c);
    if (reason) return this.hud.toast(reason);
    this.operate(ship, c.index);
  }

  /**
   * The astronaut's frame follows where it is: into a ship when it stands on it (the bubble's copy
   * of the hull) or is inside one of its compartments; back to the bubble (frame 0) when it is
   * outside every compartment and nothing of the ship holds it up — on the ground anywhere, or
   * floating beside the ship in orbit. Position, velocity and heading carry over unbroken
   * (Frames.transfer).
   */
  private playerFrame(h: number) {
    const ctl = this.ctl;
    if (this.seat) return;
    const g = ctl.groundHandle;
    if (ctl.frame === WORLD_FRAME) {
      const p = this.frames.toWorld(WORLD_FRAME, [ctl.position.x, ctl.position.y, ctl.position.z]);
      let above: number | null = null;
      for (const ship of this.ships) {
        const l = ship.sim.toLocal(p);
        // well clear of its outside (its own bounds, whatever the ship's size): nothing to ask
        if (clearOfHull(ship.sim.def, l)) continue;
        const inside = ship.inside([l[0], l[1] + 1, l[2]]);
        // on the hull outside: only once clear of the ground (no flip-flop where the stairs meet it)
        above ??= this.groundAlt(p);
        const clear = above > 0.06;
        // at the foot of the stairs or the ramp the capsule leans on them by its front while the
        // feet are still over the ground: aboard now, or the ground holds it in the crease
        const leaning = above > 0.04 && [...ctl.steppedOn].some((h) => ship.physics.ownsOuter(h));
        if (inside || (g !== null && ship.physics.ownsOuter(g) && clear) || leaning) return this.setFrame(ship.id);
      }
      return;
    }
    const ship = this.frames.ship(ctl.frame);
    if (!ship) return this.setFrame(WORLD_FRAME);
    const p = ctl.position;
    const inside = ship.inside([p.x, p.y + 1, p.z]);
    const held = g !== null && (ship.physics.ownsInner(g) || !!this.crates.byCollider(g));
    this.offShip = inside || held ? 0 : this.offShip + h;
    const w = ship.sim.toWorld([p.x, p.y, p.z]);
    // the ground wherever the ship is (none worth asking about high up)
    const alt = inside ? Infinity : this.groundAlt(w);
    const onMoon = alt <= 0.03;
    // standing on the ship (its stairs, its ramp) keeps you aboard even at ground level
    // …unless it runs into the ground (the stairs go on below it): then the moon has you
    const sunk = alt < -0.04;
    if (this.offShip > 0.15 || (!inside && (sunk || (!held && onMoon)))) this.setFrame(WORLD_FRAME);
  }

  /** Move the astronaut (and the crate in its hands) into another frame without a jump. */
  private setFrame(to: number) {
    const ctl = this.ctl;
    const from = ctl.frame;
    if (from === to) return;
    const p = ctl.position;
    const v = ctl.velocity;
    const next = this.frames.transfer(from, to, [p.x, p.y, p.z], [v.x, v.y, v.z]);
    const at: V3 = [next.p[0], next.p[1], next.p[2]];
    const yaw = carryYaw(this.frames.quat(from), this.frames.quat(to), ctl.yaw);
    // the view keeps its old tilt and eases into the new frame's (no sudden lurch)
    this.easeTilt(this.frames.quat(from, true), ctl.yaw, this.frames.quat(to, true), yaw);
    // back on the ground off the stairs / the ramp: feet above the ground (they may have gone a
    // little under it on the part of the stairs that runs on below it), capsule clear of the hull
    if (to === WORLD_FRAME && this.bubble.mode !== 'space') {
      const alt = this.groundAlt(this.frames.toWorld(WORLD_FRAME, next.p));
      if (alt < 0.03) next.p[1] += 0.03 - alt;
    }
    // the step before goes with the frames' poses of the step before: the view does not jump
    const a = this.frames.pair(from);
    const b = this.frames.pair(to);
    _pp[0] = this.prevPos.x;
    _pp[1] = this.prevPos.y;
    _pp[2] = this.prevPos.z;
    carryPoint(a?.prev ?? null, b?.prev ?? null, _pp);
    ctl.moveTo(to, this.frames.world(to), new THREE.Vector3(...next.p), new THREE.Vector3(...next.v), yaw);
    ctl.gravity.copy(this.frames.gravity(to));
    if (to === WORLD_FRAME) ctl.unstick();
    // whatever lifted it clear of the ground or a hull lifts the step before alike
    this.prevPos.set(_pp[0] + ctl.position.x - at[0], _pp[1] + ctl.position.y - at[1], _pp[2] + ctl.position.z - at[2]);
    this.offShip = 0;
    this.motion?.event('tú', `${frameName(from)} → ${frameName(to)}`);
    const held = this.crates.hold;
    if (held && held.crate.fr !== to) {
      this.crates.moveTo(held.crate, to);
      held.target = [...held.crate.curP] as V3;
    }
  }

  /** Feet of the astronaut in the world (this fixed step). */
  private readonly focusAt = new THREE.Vector3();

  private myWorld(out = new THREE.Vector3()) {
    const w = this.frames.toWorld(this.ctl.frame, [this.ctl.position.x, this.ctl.position.y, this.ctl.position.z]);
    return out.set(w[0], w[1], w[2]);
  }

  /** The crate under the crosshair within reach (as drawn), if any. */
  private crateInSight() {
    // the camera hangs from the render origin's root: its position is in the world
    const o = this.camera.position;
    const d = this.camera.getWorldDirection(new THREE.Vector3());
    return this.crates.pick([o.x, o.y, o.z], [d.x, d.y, d.z], CARRY_REACH + 0.6)?.crate ?? null;
  }

  /** The camera in a frame's coordinates (as drawn): eye and view direction. */
  private viewIn(fr: number): { eye: V3; dir: V3 } {
    const o = this.camera.position;
    const d = this.camera.getWorldDirection(new THREE.Vector3());
    return { eye: this.frames.toLocal(fr, [o.x, o.y, o.z], true), dir: this.frames.dirToLocal(fr, [d.x, d.y, d.z], true) };
  }

  /** Offline: what the ship systems reported, resolved like the server does (room.ts). */
  private offlineEvents(sim: ShipSim, events: SysEvent[], depth = 0) {
    for (const e of events) {
      if (e.type === 'explode') {
        this.hud?.toast(e.cause);
        // bounded chain, like the server
        if (depth < 3) this.onExplode(0, sim.toWorld(e.at), undefined, { radius: e.radius, damage: e.damage, depth: depth + 1 }, { fr: sim.id, l: e.at });
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
    // the click is ours right away (a refusal buzzes when the authority answers)
    ship.sounds.control(ctl);
    const c = ship.sim.def.controls[ctl];
    if (c?.command) {
      const reason = ship.sim.blocked(c, dir) ?? this.consoleCommands.execute(ship, c.command);
      if (reason) { ship.sounds.control(ctl, true); this.hud.toast(reason); }
      return;
    }
    if (this.offline) return this.localInteract(ship.id, ctl, dir);
    if (c?.kind === 'bezel' && !ship.sim.blocked(c, dir)) ship.apply({ [c.key]: ship.sim.next(c, dir) });
    this.net.sendInteract(ship.id, ctl, dir);
  }

  private localInteract(ship: number, ctl: number, dir = 0) {
    const sim = this.shipAuthority.get(ship);
    if (!sim) return;
    const r = sim.interact(ctl, dir);
    if ('reason' in r) this.ships.find((s) => s.id === ship)?.refuse(ctl);
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
    if (!sw) return;
    // switches that moved elsewhere: someone else's click, or the machinery's own (our own clicks were heard already)
    if (by === undefined || hp) ship.sounds.switched(sw, 'system');
    else if (by !== this.welcome.id) {
      const r = this.remotes.get(by);
      ship.sounds.switched(sw, 'crew', r && r.frame === id ? [r.local.x, r.local.y + 1, r.local.z] : undefined);
    }
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
    this.onExplode(this.welcome.id, v, terrainImpact(projectileById('rocket')!.impact.terrain, v, this.surfaces));
  }

  /** Automation: put the astronaut at a world point (on the ground; it boards by itself if it is aboard). */
  teleport(p: THREE.Vector3) {
    this.standUp();
    this.setFrame(WORLD_FRAME);
    this.placeInBubble([p.x, p.y, p.z]);
    this.playerFrame(0);
  }

  /** Put the astronaut (frame 0) at a world point, the bubble laid there first. */
  private placeInBubble(pw: V3) {
    this.followBubble({ p: pw, v: [0, 0, 0] });
    const l = this.frames.toLocal(WORLD_FRAME, pw);
    const at = new THREE.Vector3(l[0], l[1], l[2]);
    this.ctl.teleport(at);
    this.prevPos.copy(at);
  }

  /** Automation: the astronaut's feet in the world and the frame it is in. */
  playerWorld() {
    return { p: this.myWorld(), frame: this.ctl.frame };
  }

  /** Automation: the helm seat index of a ship (-1 = none). */
  helmIndex(ship = 0) {
    const s = this.ships[ship];
    const id = s && helmSeat(s.sim.def);
    return id ? s.sim.def.seats.findIndex((x) => x.id === id) : -1;
  }

  /** Advance `frames` frames of `dt`; renders only the last one (none with render = false). */
  step(frames = 1, dt = 1 / 30, render = true) {
    for (let i = 0; i < frames; i++) this.tick(dt, render && i === frames - 1);
  }

  /** Flight instruments in the helmet while sitting at a helm (or riding a ship in the air). */
  private updateFlightHud() {
    const ship = this.frames.ship(this.ctl.frame);
    const el = this.flightHud;
    if (!ship || !ship.sim.def.helm || (!this.atHelm() && ship.sim.landed)) {
      el.style.display = 'none';
      return;
    }
    const r = flightReadout(ship.sim, this.surfaces);
    const deg = (x: number) => Math.round((x * 180) / Math.PI);
    const hdg = ((deg(r.heading) % 360) + 360) % 360;
    const f1 = (x: number) => x.toFixed(1);
    const pilot = ship.pilot === this.welcome.id ? 'TÚ' : ship.pilot ? `#${ship.pilot}` : 'AUTO';
    const lines = [
      `ALT ${f1(r.agl).padStart(6)} m   V/S ${f1(r.vs).padStart(6)} m/s   GS ${f1(r.gs).padStart(5)} m/s   RUMBO ${String(hdg).padStart(3, '0')}°`,
      `CAB ${String(deg(r.pitch)).padStart(4)}°  ALA ${String(deg(r.roll)).padStart(4)}°  EMPUJE ${r.weight > 0 ? Math.round((r.thrust / r.weight) * 100) : 0}% del peso  TREN ${r.gear >= 0.99 ? 'ABAJO' : r.gear <= 0.01 ? 'ARRIBA' : 'EN TRÁNSITO'}${r.landed ? '  EN TIERRA' : ''}`,
      `${r.direct ? 'DIRECTO (sin ordenador de vuelo)' : r.orbital ? 'ORBITAL (sin retención de posición)' : 'ASISTIDO'}   P.AUT ${r.apOn ? 'ON' : 'OFF'}${r.modes.length ? ' · ' + r.modes.join(' · ') : ''}${r.wp ? `   WP ${r.wp.name} ${String(((deg(r.wp.bearing) % 360) + 360) % 360).padStart(3, '0')}° ${Math.round(r.wp.dist)} m` : ''}   MANDOS: ${pilot}`,
    ];
    // high up or fast: the orbit the ship is on
    if (r.orbital || r.alt > 3000) lines.push(`${orbitLine(r.orbit)}   BASE ${(r.base.dist / 1000).toFixed(r.base.dist < 100000 ? 1 : 0)} km · ${String(((deg(r.base.bearing) % 360) + 360) % 360).padStart(3, '0')}°`);
    el.textContent = lines.join('\n');
    el.style.display = '';
  }

  private bookShip: ShipClient | null = null;

  /** Point the manual at the ship the astronaut is in (or the nearest one). */
  private bindManual() {
    const at = this.myWorld();
    const ship = this.frames.ship(this.ctl.frame) ?? [...this.ships].sort((a, b) => a.position.distanceTo(at) - b.position.distanceTo(at))[0];
    if (!ship || ship === this.bookShip) return;
    if (this.bookShip) this.pointAt(null);
    this.bookShip = ship;
    this.hud.setManual(ship.sim, {
      point: (index) => this.pointAt(index === null ? null : { ship, index }),
      where: () => (this.ctl.frame === ship.id ? [this.ctl.position.x, this.ctl.position.y, this.ctl.position.z] : null),
      surfaces: this.surfaces,
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
    for (const feed of this.cameraFeeds) feed.dispose();
    this.pip?.dispose();
    this.pipeline.beforeRender = null;
  }

  /** Lock acquisition is an event: raycast only the physical scene, excluding screen surfaces. */
  private pickSight(eye: THREE.Vector3, dir: THREE.Vector3, out: THREE.Vector3): boolean {
    this.sightRay.set(origin.toRender(_sight.copy(eye)), dir);
    this.sightRay.near = 0.15;
    this.sightRay.far = 3000;
    const hits = this.sightRay.intersectObjects(this.sightObjects, true);
    for (const hit of hits) {
      let screen = false;
      for (let object: THREE.Object3D | null = hit.object; object; object = object.parent) if (object.userData.cameraDisplay) { screen = true; break; }
      if (screen) continue;
      origin.toWorld(out.copy(hit.point));
      return true;
    }
    return false;
  }

  /** Automation hook (tests / screenshots). */
  get debug() {
    return { input: this.input, controller: this.ctl, rig: this.rig, camera: this.camera, remotes: this.remotes, ships: this.ships, interaction: this.interaction, scene: this.scene, game: this };
  }

  /** Stream the ground round the spawn (world `w`, bubble `local`) before letting the player in. */
  private async warmUp(w: readonly number[], local: THREE.Vector3) {
    const t0 = performance.now();
    this.camera.position.copy(this.upFrom(_fb.set(w[0], w[1], w[2]), 1.7));
    this.camera.updateMatrixWorld();
    const ground = this.grounds.find((g) => g.body === bodyAt(w));
    while (performance.now() - t0 < 25000) {
      ground?.terrain.update(this.camera);
      ground?.rocks.update(_fb.set(w[0], w[1], w[2]));
      this.physics.update(local.x, local.z);
      const ready = (!ground || ground.terrain.readyAt(w)) && this.physics.readyAt(local.x, local.z);
      const pending = this.pool.busy;
      this.opts.onProgress(`Generando terreno… ${pending} bloques pendientes`);
      if (ready && pending < 6) break;
      await new Promise((r) => setTimeout(r, 50));
    }
    // every shader the scene needs, compiled now (in parallel where the driver allows) instead of
    // stalling a frame the first time something comes into view
    this.opts.onProgress('Compilando shaders…');
    try {
      await this.pipeline.renderer.compileAsync(this.scene, this.camera);
    } catch {
      // not fatal: they compile on first use as before
    }
    // physics needs a step to register the new colliders before the first character query
    this.physics.step(1 / 60);
  }

  private pendingHealth = new Map<number, number>();

  private addRemote(p: PlayerInfo, announce: boolean) {
    if (this.remotes.has(p.id) || !this.asset) return;
    const r = new RemotePlayer(p, this.asset);
    for (const w of WEAPON_ORDER) r.astronaut.attachWeapon(w);
    const hp = this.pendingHealth.get(p.id);
    if (hp !== undefined) {
      r.hp = hp;
      r.dead = hp <= 0;
    }
    this.remotes.set(p.id, r);
    // their boots, pack and tools, heard through whatever carries them here
    const s = (r.sounds = new CrewSounds(false));
    r.astronaut.onStep = (_foot, k) => s.step(r.frame !== WORLD_FRAME ? 'deck' : (bodyAt(r.position.toArray()).def.ground ?? 'regolith'), k);
    r.astronaut.onLand = (v) => s.land(r.frame !== WORLD_FRAME ? 'deck' : (bodyAt(r.position.toArray()).def.ground ?? 'regolith'), v);
    // placed in world coordinates, like everything under the render origin's root
    origin.root.add(r.astronaut.root);
    r.astronaut.root.userData.cat = 'astronautas';
    if (announce) this.hud?.toast(`${p.name} se ha unido a la EVA`);
  }

  private nameOf(id: number) {
    if (id === this.welcome.id) return this.opts.name || 'Tú';
    return this.remotes.get(id)?.info.name ?? '???';
  }

  /**
   * A shot of weapon `w` (anyone's: its projectile flies on every client). `fr`: fired aboard that
   * ship (`o`, `d`, `v` in its space); `prev`: our own shot, the muzzle the step before (world).
   */
  /**
   * A shot leaves (ours now, or someone else's). `t`: the time of the shooter's step it left at —
   * someone else's is heard a network trip later and is flown forward to our present, where the
   * shooter and the ship it left are drawn (net/replica.ts).
   */
  private onFire(id: number, w: string, o: Vec3, d: Vec3, v?: Vec3, fr?: number, prev?: V3, m?: number, t?: number) {
    const ahead = t !== undefined && Number.isFinite(this.stepClock.t) ? Math.max(0, (this.stepClock.t - t) / 1000) : 0;
    if (m !== undefined) {
      // a weapon mount of a ship: it leaves its muzzle there, heard from the mount through the ship
      const mounted = weaponById(w);
      const kind = projectileOf(mounted);
      const ship = fr !== undefined ? this.frames.ship(fr) : undefined;
      if (!mounted?.mounted || !kind || !ship) return;
      this.projectiles?.spawn(id, kind.id, ship.id, o, d, v, undefined, ahead);
      if (mounted.sounds?.fire) ship.sounds.playAt(mounted.sounds.fire, o);
      return;
    }
    const weapon = WEAPONS[w];
    const kind = projectileOf(weapon);
    if (!weapon || !kind) return;
    this.projectiles?.spawn(id, kind.id, fr ?? WORLD_FRAME, o, d, v, prev, ahead);
    this.remotes.get(id)?.astronaut.applyRecoil(weapon.recoil);
    // the shot on the shooter's shoulder: ours through the suit, theirs through what carries it
    const crew = id === this.welcome.id ? this.audio?.me : this.remotes.get(id)?.sounds;
    const at = fr !== undefined ? this.frames.ship(fr)?.sim.toWorld(o) : o;
    if (at && weapon.sounds?.fire) crew?.play(weapon.sounds.fire, [at[0], at[1], at[2]]);
  }

  /**
   * An impact confirmed by the server (offline: by us). `k`: the projectile kind, which says how it
   * shows (a blast, or a hit); none: a ship's own blast (`blast`). `aboard`: it happened in or
   * against that ship, at `l` in its space (drawn there, wherever the ship is here).
   */
  private onExplode(id: number, p: Vec3, mod?: TerrainMod, blast?: { radius: number; damage: number; depth: number }, aboard?: { fr: number; l: Vec3 }, k?: string) {
    const def = k ? projectileById(k) : undefined;
    const host = aboard ? this.frames.ship(aboard.fr) : undefined;
    if (host && aboard) p = host.sim.toWorld(aboard.l) as Vec3;
    const at = new THREE.Vector3(...p);
    if (mod) {
      // the crater joins its body's ground, in the server's order (a repeat deepens the old one)
      const stored = this.surfaces(bodyById(mod.body))?.addMod(mod);
      if (stored) {
        for (const g of this.grounds) {
          g.terrain.invalidate(stored);
          g.rocks.invalidate(stored);
        }
        this.physics.invalidate(stored);
      }
    }
    // what it went off against moves with it (a ship in flight): the cloud moves with it too
    let carry: THREE.Vector3 | undefined;
    if (host && aboard) carry = new THREE.Vector3(...pointVelocity(host.sim.pose, aboard.l));
    else {
      // against the outside of a ship (anywhere within its own bounds): the cloud moves with it
      for (const s of this.ships) {
        const l = s.sim.toLocal(p);
        if (!clearOfHull(s.sim.def, l)) carry = new THREE.Vector3(...pointVelocity(s.sim.pose, l));
      }
    }
    this.projectiles.impact(id, def?.id, at, carry);
    // offline: we are the ship authority too
    const hull = def ? def.impact.hull : shipBlast(blast?.radius, blast?.damage);
    if (this.offline) {
      for (const [sid, sim] of this.shipAuthority) {
        if (mod && Math.hypot(sim.pose.p[0] - p[0], sim.pose.p[1] - p[1], sim.pose.p[2] - p[2]) < 20) sim.flight.wake();
        const r = sim.explode(p, hull);
        if (r.hp.length) this.onShip(sid, Object.keys(r.sw).length ? r.sw : undefined, r.hp, id);
        this.offlineEvents(sim, r.events, blast?.depth ?? 0);
      }
    }
    if (def && def.impact.fx === 'hit') {
      // a hit: its clang, and a nudge to a loose object right there
      this.audio.playAt(def.sounds?.impact ?? 'bullet.hit', [p[0], p[1], p[2]]);
      this.crates.blast(p, this.offline || id === this.welcome.id, 0.6, 10);
      return;
    }
    // the boom, through whatever carries it to us (a ship's own blasts are bigger)
    this.audio.explosion([p[0], p[1], p[2]], blast ? Math.max(0.7, blast.radius / 4) : 1);
    // loose crates: the shooter's client (or whoever simulates them) throws them
    this.crates.blast(p, this.offline || id === this.welcome.id);
    // rock fragments where the ground is solid here (the bubble's coordinates)
    if (this.bubble.mode !== 'space' && this.groundAlt(p) < 1.5) {
      const l = this.frames.toLocal(WORLD_FRAME, p);
      if (this.physics.readyAt(l[0], l[2])) this.debris.burst(new THREE.Vector3(l[0], l[1], l[2]), 8 + Math.floor(Math.random() * 6));
    }
    // blast wave: push me away (the server decides damage)
    const c = this.upFrom(this.myWorld(), 0.9);
    const dist = c.distanceTo(at);
    if (!this.dead) this.rig.shake(Math.max(0, 1 - dist / 25) * 0.8);
    if (dist < 7 && !this.dead && !this.seat) {
      const k = (1 - dist / 7) * 7;
      const up = this.upFrom(c, 1).sub(c);
      const dv = c.sub(at).normalize().multiplyScalar(k).addScaledVector(up, k * 0.5);
      this.ctl.impulse(new THREE.Vector3(...this.frames.dirToLocal(this.ctl.frame, [dv.x, dv.y, dv.z])));
    }
  }

  private onHealth(id: number, hp: number, by?: number, dead?: boolean) {
    if (id === this.welcome.id) {
      if (hp < this.hp) {
        this.hud.damage(this.hp - hp);
        sfx.ui('suit.hit', Math.min(1, 0.4 + (this.hp - hp) / 40));
      }
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
      this.standUp();
      if (this.carry.held) this.carry.drop();
      this.setFrame(WORLD_FRAME);
      // the spawn is on the ground already (world)
      this.placeInBubble([spawn[0], spawn[1], spawn[2]]);
      if (this.rig.mode === 'third') this.rig.toggle();
    } else {
      const r = this.remotes.get(id);
      if (r) {
        r.dead = false;
        r.hp = 100;
      }
    }
  }

  /** A shot of a ship's weapon mount (the gunner's: client/ship/gunnery.ts), ship space. Offline we are its authority. */
  private fireMount(ship: ShipClient, m: number, w: string, o: V3, d: V3): boolean {
    if (this.dead) return false;
    const mounts = ship.sim.mounts;
    if (!mounts) return false;
    if (this.offline) {
      const authority = this.shipAuthority.get(ship.id);
      if (!authority?.mounts || authority.mounts.tryFire(authority.st, m, performance.now() / 1000) < 0) return false;
      // Predict from the same authority that ticks and reloads; the next tick must not undo a shot.
      ship.sim.st.set(authority.st);
      ship.sim.version++;
    }
    const r3 = (a: readonly number[], k: number): Vec3 => [round(a[0], k), round(a[1], k), round(a[2], k)];
    const lo = r3(o, 3);
    const ld = r3(d, 4);
    if (!this.offline) this.net.sendFire(w, lo, ld, undefined, ship.id, m, this.stepClock.t);
    this.onFire(this.welcome.id, w, lo, ld, undefined, ship.id, undefined, m);
    return true;
  }

  /** The trigger of the weapon in hand, if it fires something (its rate from the catalog). */
  private tryFire() {
    const now = performance.now() / 1000;
    const weapon = this.inHand();
    if (this.dead || !weapon || !projectileOf(weapon) || this.seat || now - this.lastFire < weapon.cooldown) return;
    this.lastFire = now;
    const dir = new THREE.Vector3();
    this.camera.getWorldDirection(dir);
    // launch from the shoulder tube, aimed at what the crosshair covers (all as drawn)
    const origin = this.me.muzzle(new THREE.Vector3()) ?? this.myWorld().add(new THREE.Vector3(0, 1.5, 0));
    origin.addScaledVector(dir, 0.1);
    const target = this.camera.position.clone().addScaledVector(dir, 80);
    const aim = target.sub(origin).normalize();
    // the tube in the frame we stand in, as drawn: it is the same place in the frame at every step
    const fr = this.ctl.frame;
    const lo = this.frames.toLocal(fr, [origin.x, origin.y, origin.z], true);
    const ld = this.frames.dirToLocal(fr, [aim.x, aim.y, aim.z], true);
    const lv = this.ctl.velocity;
    const r3 = (a: readonly number[], k: number): Vec3 => [round(a[0], k), round(a[1], k), round(a[2], k)];
    let o: Vec3, d: Vec3, v: Vec3 | undefined, shipFr: number | undefined, prev: V3 | undefined;
    if (fr !== WORLD_FRAME) {
      // aboard: it leaves in the ship's space with our motion in it (the ship's own is the ship's)
      shipFr = fr;
      o = r3(lo, 3);
      d = r3(ld, 4);
      v = lv.lengthSq() > 0.0025 ? r3([lv.x, lv.y, lv.z], 3) : undefined;
    } else {
      // in the world: where the tube is at this step and was the step before (drawn between the two,
      // it leaves the tube as drawn), with the launcher's own motion (in orbit 1.6 km/s)
      const h = this.loop.step;
      const a = this.loop.alpha;
      o = r3(toWorld(this.bubble.pose, [lo[0] + lv.x * (1 - a) * h, lo[1] + lv.y * (1 - a) * h, lo[2] + lv.z * (1 - a) * h]), 3);
      prev = toWorld(this.bubble.prev, [lo[0] - lv.x * a * h, lo[1] - lv.y * a * h, lo[2] - lv.z * a * h]);
      d = r3(this.frames.dirToWorld(fr, ld), 4);
      const mv = this.playerMotion().v;
      v = Math.hypot(mv[0], mv[1], mv[2]) > 0.05 ? r3(mv, 3) : undefined;
    }
    // ours leaves now, from the muzzle as drawn (online, the server's echo is skipped)
    if (!this.offline) this.net.sendFire(weapon.id, o, d, v, shipFr, undefined, this.stepClock.t);
    this.onFire(this.welcome.id, weapon.id, o, d, v, shipFr, prev);
    // recoil kick
    // momentum conservation on a ~180 kg suited astronaut, plus the body/arm springs
    this.me.applyRecoil(weapon.recoil);
    const kick = aim.clone().multiplyScalar(-weapon.recoil / 180);
    this.ctl.impulse(new THREE.Vector3(...this.frames.dirToLocal(this.ctl.frame, [kick.x, kick.y, kick.z])));
  }

  /** Jetpack exhaust: down the astronaut's own axis, moving with it (`carry`: its world velocity). */
  private emitJet(a: Astronaut, carry?: V3) {
    a.nozzles(this.nozzles);
    for (const n of this.nozzles) {
      for (let i = 0; i < 3; i++) {
        _jetVel.set((Math.random() - 0.5) * 1.2, -7 - Math.random() * 4, (Math.random() - 0.5) * 1.2).applyQuaternion(a.root.quaternion);
        this.particles.emit('glow', { pos: n, vel: _jetVel, carry, color: [1.6, 1.9, 3.2], life: 0.08 + Math.random() * 0.1, size: 0.09 });
      }
    }
  }

  /** Things that came into our interest (docs/RED.md), by kind. */
  private onSpawn(list: EntityWire[]) {
    for (const e of list) {
      if (e.k === 'obj') this.crates?.spawn(e);
      else if (e.k === 'npc') this.addNpc(e);
    }
  }

  /** Things we can forget, by kind. */
  private onGone(k: EntityKind, ids: number[]) {
    if (k === 'obj') for (const id of ids) this.crates?.forget(id);
    else if (k === 'npc') for (const id of ids) this.removeNpc(id);
  }

  /** Someone of the world came near (docs/MUNDO.md §11): drawn like any other astronaut. */
  private addNpc(e: Extract<EntityWire, { k: 'npc' }>) {
    if (!this.asset) return;
    let n = this.npcs.get(e.id);
    if (!n) {
      n = new RemotePlayer({ id: e.id, name: e.name, variant: e.variant }, this.asset);
      this.npcs.set(e.id, n);
      const s = (n.sounds = new CrewSounds(false));
      const body = n;
      n.astronaut.onStep = (_foot, k) => s.step(body.frame !== WORLD_FRAME ? 'deck' : (bodyAt(body.position.toArray()).def.ground ?? 'regolith'), k);
      origin.root.add(n.astronaut.root);
      n.astronaut.root.userData.cat = 'personas';
    }
    n.push(e.t, e.s);
  }

  private removeNpc(id: number) {
    const n = this.npcs.get(id);
    if (!n) return;
    this.hud?.removeTag(NPC_TAG - id);
    n.dispose();
    this.npcs.delete(id);
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

  /** What the motion probe watches this frame (client/diag/motionProbe.ts): everything drawn that moves, as drawn. */
  private *motionSubjects(): Generator<MotionSubject> {
    const cam = this.camera.position;
    const near = (p: { x: number; y: number; z: number }) => (p.x - cam.x) ** 2 + (p.y - cam.y) ** 2 + (p.z - cam.z) ** 2 < 600 * 600;
    const at = (key: string, p: readonly number[]): MotionSubject => ({ key, x: p[0], y: p[1], z: p[2] });
    for (const ship of this.ships) {
      const p = ship.render.p;
      if (near({ x: p[0], y: p[1], z: p[2] })) yield at(`nave ${ship.id}`, p);
    }
    for (const c of this.crates.list) {
      const p = this.crates.worldPose(c).p;
      if (near({ x: p[0], y: p[1], z: p[2] })) yield at(`caja ${c.id}`, p);
    }
    const shots: MotionSubject[] = [];
    this.projectiles.forEachDrawn((id, p) => {
      if (near(p)) shots.push({ key: `proyectil ${id}`, x: p.x, y: p.y, z: p.z });
    });
    yield* shots;
    for (const r of this.remotes.values()) if (near(r.position)) yield { key: `astronauta ${r.info.id}`, x: r.position.x, y: r.position.y, z: r.position.z };
    if (this.rig.mode === 'third') yield { key: 'tú', x: this.me.root.position.x, y: this.me.root.position.y, z: this.me.root.position.z };
  }

  /** The motion probe's counters: clock, bubble, frames and how the network corrects the others. */
  private motionInfo(): Record<string, string | number> {
    const b = this.bubble;
    const out: Record<string, string | number> = {
      'pasos / fotograma': `${this.loop.lastSteps} · α ${this.loop.alpha.toFixed(2)}`,
      'reloj de pasos': `error ${this.stepClock.error.toFixed(1)} ms · saltos ${this.stepClock.jumps}`,
      burbuja: `${b.mode} · ${Math.round(Math.hypot(b.pose.v[0], b.pose.v[1], b.pose.v[2]))} m/s · v${b.version}`,
      'tu marco': frameName(this.ctl.frame),
    };
    for (const ship of this.ships) {
      const c = ship.playback.corrections;
      const speed = Math.round(Math.hypot(ship.sim.pose.v[0], ship.sim.pose.v[1], ship.sim.pose.v[2]));
      out[`nave ${ship.id}`] = `${speed} m/s · ${ship.pilot === this.welcome.id || this.shipAuthority.has(ship.id) ? 'simulada aquí' : `red: corr. ${c.max.toFixed(2)} m, saltos ${c.snaps}`}`;
    }
    for (const r of this.remotes.values()) {
      const c = r.corrections;
      out[`astronauta ${r.info.id}`] = `${frameName(r.frame)} · corr. ${c.max.toFixed(2)} m, saltos ${c.snaps}`;
    }
    return out;
  }

  private frame = () => {
    if (!this.running) return;
    requestAnimationFrame(this.frame);
    const t0 = performance.now();
    this.clock.update();
    const interval = this.clock.getDelta();
    const dt = Math.min(interval, 1 / 20);
    this.fps += (1 / Math.max(dt, 1e-4) - this.fps) * 0.05;
    this.tick(dt);
    const cpu = performance.now() - t0;
    // how the frame went (the display's interval, the CPU's share): the render governor keeps the rate up
    this.pipeline.frameStats(interval * 1000, cpu);
    // and the worst of the last seconds, with what took its time (F3)
    this.spikes.feed(interval * 1000, cpu, this.partsOfFrame);
  };

  /** Hitches (F3): the worst recent frame and its longest systems. */
  private readonly spikes = new SpikeLog();
  /** CPU time of the last frame's render call (ms). */
  private renderMs = 0;
  private readonly partsOfFrame = () => this.frameParts();
  private *frameParts(): Generator<[string, number]> {
    yield* this.systems.lastFrame;
    yield ['render', this.renderMs];
  }

  /** One simulation + render step (exposed for deterministic automation). */
  tick(dt: number, render = true) {
    const input = this.input;
    if (!this.spawned) return;
    // the render origin stays near the camera (render/origin.ts); first, before anything reads a matrix
    origin.follow(this.camera.position);

    // --- local player -------------------------------------------------------------------------
    if (input.consume('KeyV')) this.rig.toggle();
    if (input.consume('KeyH')) this.hud.toggleHelp();
    if (input.consume('KeyL')) {
      this.lamps = !this.lamps;
      sfx.ui('suit.click');
    }
    // tools: the number keys pick them in catalog order (press again / X to put it away)
    WEAPON_ORDER.forEach((w, i) => {
      if (i > 8 || !input.consume(`Digit${i + 1}`)) return;
      if (this.me.equipped === w.id && this.me.isArmed) this.me.setArmed(false);
      else {
        this.me.equip(w.id);
        this.me.setArmed(true);
      }
      this.toolToast();
    });
    if (input.consume('KeyX')) {
      this.me.setArmed(!this.me.isArmed);
      this.toolToast();
    }
    // click: an object in the hands is set down on its ghost; else a ship control / seat under the
    // crosshair takes it, otherwise the tool in hand acts (an automatic one while the button is held)
    if (this.fireQueued) {
      const held = this.carry.held;
      if (held) {
        const def = objectOf(held.spec);
        if (!this.carry.place()) {
          sfx.ui('ui.deny');
          this.hud.toast(`${def.name}: no cabe ahí`);
        } else this.audio.me.play(def.sounds?.drop ?? 'crate.drop', null, 0.7);
      } else if (this.gunnery.active && this.gunnery.aiming) {
        this.triggerHeld = true;
        this.gunnery.trigger(performance.now() / 1000, true);
      } else if (!this.interaction.use()) {
        this.triggerHeld = true;
        if (this.gunnery.active) this.gunnery.trigger(performance.now() / 1000, true);
        else this.tryFire();
      }
    }
    if (!input.down('Mouse0')) this.triggerHeld = false;
    else if (this.triggerHeld && !this.fireQueued && this.gunnery.active) this.gunnery.trigger(performance.now() / 1000);
    else if (this.triggerHeld && !this.fireQueued) {
      const w = this.inHand();
      if (w?.action.kind === 'fire' && w.action.auto) this.tryFire();
    }
    if (input.consume('KeyF') && !this.seat) this.tryFire();
    // E: let go of the object in the hands / pick up the one in sight / a control / stand up
    if (input.consume('KeyE')) {
      const crate = this.carry.held || this.seat || this.dead ? null : this.crateInSight();
      if (this.carry.held) {
        const def = objectOf(this.carry.held.spec);
        this.carry.drop();
        this.audio.me.play(def.sounds?.drop ?? 'crate.drop', null, 0.5);
      } else if (crate && objectOf(crate.spec).handling.grab) {
        this.crates.grab(crate, this.ctl.frame);
        this.audio.me.play(objectOf(crate.spec).sounds?.grab ?? 'crate.grab');
      } else if (!this.interaction.use() && this.seat) this.standUp();
    }
    // Q: throw it (if it can be thrown)
    if (input.consume('KeyQ') && this.carry.held && objectOf(this.carry.held.spec).handling.throw) {
      const def = objectOf(this.carry.held.spec);
      this.carry.throw(this.viewIn(this.ctl.frame).dir);
      this.audio.me.play(def.sounds?.throw ?? 'crate.throw');
    }
    if (this.seat && input.consume('KeyP') && this.atHelm()) this.toggleAutopilot();
    if (this.seat && input.consume('Space')) this.standUp();
    this.fireQueued = false;
    this.rig.zoomHeld = input.down('Mouse2');
    this.input.sensitivity = 0.0022 / this.rig.magnification;
    this.me.setLamps(this.lamps);
    if (!this.gunnery.look(input)) this.ctl.look(input, this.rig.mode === 'third' && !this.debugOrbit ? this.rig : undefined);
    if (this.ctl.seat) {
      // seated: turn the head, not the seat
      let d = this.ctl.yaw - this.ctl.seat.yaw;
      d = Math.atan2(Math.sin(d), Math.cos(d));
      this.ctl.yaw = this.ctl.seat.yaw + THREE.MathUtils.clamp(d, -1.7, 1.7);
    }
    // the collision of the ground streams round the astronaut, in the bubble (none up in space;
    // aboard a ship flying over the ground, only once it is low enough for it to matter)
    if (this.ctl.frame === WORLD_FRAME) this.physics.update(this.ctl.position.x, this.ctl.position.z);
    else if (this.bubble.mode !== 'space') {
      const w = this.myWorld().toArray();
      if (this.groundAlt(w) < 1500) {
        const fb = this.frames.toLocal(WORLD_FRAME, w);
        this.physics.update(fb[0], fb[2]);
      }
    } else this.physics.update(0, 0);
    // fixed-step simulation: ships, physics, character, crates, debris, projectiles
    this.loop.advance(dt, (h) => {
      if (this.ctl.frame === WORLD_FRAME && !this.physics.readyAt(this.ctl.position.x, this.ctl.position.z)) return;
      this.systems.run('fixed', h);
    });
    // the last step's state belongs to now minus what is still to be simulated (on the server's clock)
    if (this.stepClock.sync(this.net.serverNow() - this.loop.alpha * this.loop.step * 1000)) this.motion?.event('reloj', `salto ${Math.round(this.stepClock.error)} ms`);
    this.bubble.frame(this.loop.alpha);
    // debris and crates write their instances in render space from the bubble as drawn
    const bp = this.frames.pose(WORLD_FRAME, true);
    this.debris.sync(bp, origin, this.loop.alpha);
    this.scrap.sync(bp, origin, this.loop.alpha);
    // safety net: never fall through the ground
    if (this.ctl.frame === WORLD_FRAME && this.bubble.mode !== 'space') {
      const alt = this.groundAlt(this.myWorld().toArray());
      if (alt < -2) {
        const up = _fb.copy(this.bubble.gravity).normalize().negate();
        this.ctl.teleport(this.ctl.position.clone().addScaledVector(up, 0.3 - alt));
      }
    }

    // ships between their last two steps (everything aboard is drawn in their frame as drawn);
    // the views compare the eye with their matrices: render space
    const eye = this.camera.getWorldPosition(_eye);
    for (const ship of this.ships) {
      // docked packs light the seat's umbilical port
      const seats = ship.sim.def.seats;
      for (let i = 0; i < seats.length; i++) {
        const at = _seat.set(seats[i].root[0], seats[i].root[1], seats[i].root[2]);
        let taken = this.seat?.ship === ship && this.seat.index === i;
        if (!taken) {
          for (const r of this.remotes.values()) {
            if (!r.dead && r.seated && r.frame === ship.id && r.local.distanceTo(at) < 0.35) {
              taken = true;
              break;
            }
          }
        }
        ship.view.seatOccupied[i] = taken;
      }
      ship.frame(dt, this.loop.alpha, eye);
    }
    this.crates.sync(this.loop.alpha);

    // render between the last two sim states: no 60 Hz judder/smear on high refresh screens. The
    // step before is carried across every change of frame and re-laying (setFrame, followBubble)
    // and snapped only by a real jump (a seat, a teleport): no guessing from how far it moved,
    // which at orbital speed would switch the interpolation off for good.
    this.ctl.renderPosition.lerpVectors(this.prevPos, this.ctl.position, this.loop.alpha);
    const rp = this.ctl.renderPosition;
    const fr = this.ctl.frame;
    const rw = this.frames.toWorld(fr, [rp.x, rp.y, rp.z], true);
    const fq = this.frames.quat(fr, true);
    // tilt left over from the last frame change, fading out (~0.8 s)
    this.tilt.slerp(_identityQ, 1 - Math.exp(-dt * 4));
    const frameQ = _frameQ.copy(this.tilt).multiply(_fq.set(fq[0], fq[1], fq[2], fq[3]));
    this.me.root.position.set(rw[0], rw[1], rw[2]);
    const bodyYaw = this.ctl.seat ? this.ctl.seat.yaw : this.ctl.yaw;
    this.me.root.quaternion.copy(frameQ).multiply(_fq.setFromAxisAngle(_up, bodyYaw));
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
      // frame 0 goes out in the world (every client has its own bubble) — position and velocity,
      // the bubble's own motion included; ships' frames as they are
      const p = this.ctl.frame === WORLD_FRAME ? this.myWorld() : this.ctl.position;
      const v = this.ctl.frame === WORLD_FRAME ? _sv.fromArray(this.playerMotion().v) : this.ctl.velocity;
      this.net.sendState({
        p: [round(p.x, 3), round(p.y, 3), round(p.z, 3)],
        v: [round(v.x, 2), round(v.y, 2), round(v.z, 2)],
        yaw: round(bodyYaw, 3),
        pitch: round(this.ctl.pitch, 3),
        fr: this.ctl.frame || undefined,
        f:
          (this.ctl.grounded ? StateFlags.Grounded : 0) |
          (this.ctl.running ? StateFlags.Running : 0) |
          (this.ctl.crouch ? StateFlags.Crouching : 0) |
          (this.lamps ? StateFlags.Lamps : 0) |
          (this.ctl.jetting ? StateFlags.Jetpack : 0) |
          (this.dead ? StateFlags.Dead : 0) |
          (this.me.isArmed ? StateFlags.Armed : 0) |
          (this.welding ? StateFlags.Welding : 0) |
          (this.ctl.seat ? StateFlags.Seated : 0),
        w: this.me.equipped || undefined,
      }, this.stepClock.t);
    }
    // the others at the time drawn this frame (between the last two steps, like everything else)
    const drawnT = this.stepClock.renderTime(this.loop.alpha);
    const eyeNow = origin.worldOf(this.camera, _eye);
    for (const r of this.remotes.values()) r.update(dt, drawnT, this.frames, eyeNow);
    for (const n of this.npcs.values()) n.update(dt, drawnT, this.frames, eyeNow);

    // --- combat & effects ------------------------------------------------------------------------
    for (const { k, c } of this.pendingHits) {
      const p: Vec3 = [round(c.p[0], 2), round(c.p[1], 2), round(c.p[2], 2)];
      // in or against a ship: in its space (the ship is elsewhere on the server and on every client)
      const l: Vec3 | null = c.l ? [round(c.l[0], 3), round(c.l[1], 3), round(c.l[2], 3)] : null;
      // offline: act as our own server (crater, no damage bookkeeping)
      if (this.offline) this.onExplode(this.welcome.id, p, terrainImpact(projectileById(k)?.impact.terrain, p, this.surfaces), undefined, l ? { fr: c.fr, l } : undefined, k);
      else if (l) this.net.sendHit(k, l, c.fr);
      else this.net.sendHit(k, p);
    }
    this.pendingHits.length = 0;
    if (this.ctl.jetting) this.emitJet(this.me, this.playerMotion().v);
    for (const r of this.remotes.values()) if (r.jetting && !r.dead) this.emitJet(r.astronaut);
    this.particles.update(dt);

    // --- camera & world streaming -----------------------------------------------------------------
    this.rig.update(dt, { feet: this.me.root.position, frame: frameQ, yaw: this.ctl.yaw, pitch: this.ctl.pitch, crouch: this.ctl.crouch }, this.me, (a, b) => {
      let best: number | null = null;
      for (const ship of this.ships) {
        const t = ship.occlude(a, b);
        if (t !== null && (best === null || t < best)) best = t;
      }
      return best;
    });
    if (this.inspectCam && START_SITE) {
      // x, z in the start site's frame (on its horizon), h over its ground
      const [x, z, yaw, pitch, hgt = 1.8] = this.inspectCam;
      const g = (this.camSite ??= siteGround(START_SITE, this.surfaces));
      if (g) {
        const p = g.point(x, z, [0, 0, 0]);
        this.camera.position.set(p[0] + g.u[0] * hgt, p[1] + g.u[1] * hgt, p[2] + g.u[2] * hgt);
        const q = g.quat([0, 0, 0, 1]);
        this.camera.quaternion.set(q[0], q[1], q[2], q[3]).multiply(_fq.setFromEuler(new THREE.Euler(pitch, yaw, 0, 'YXZ')));
      }
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
      // lookAt compares with the camera's own matrix: render space
      this.camera.lookAt(origin.toRender(t));
      this.camera.updateMatrixWorld();
    }
    // recoil kicks the view up with the torso spring
    if (this.me.recoilPitch) {
      this.camera.rotateX(this.me.recoilPitch * 0.8);
      this.camera.updateMatrixWorld();
    }
    this.me.setEyeClip(this.rig.mode === 'first' && !this.inspectCam && !this.focusCam ? this.camera.position : null);
    this.systems.run('frame', dt);
    const view = this.viewIn(fr);
    this.carry.update(fr, view.eye, view.dir, this.ctl.yaw, this.ctl.handle);
    this.updateFlightHud();
    // rocks only matter close to the ground: flying high, nothing to scatter
    const cp = this.camera.position;
    const cb = bodyAt([cp.x, cp.y, cp.z]);
    this.nearGround = this.groundAlt([cp.x, cp.y, cp.z]) < 2500;
    if (this.nearGround) for (const g of this.grounds) if (g.body === cb) g.rocks.update(this.me.root.position);
    this.lighting.update();

    // --- HUD ---------------------------------------------------------------------------------------
    this.hud.updateManual(performance.now() / 1000);
    // the compass: bearings on the local horizon where the astronaut is (north toward the pole)
    const me = this.me.root.position;
    const lf = frameAt(bodyAt([me.x, me.y, me.z]), [me.x, me.y, me.z], _lf);
    const markers = [...this.siteMarkers.map((m) => markerTo(me, m.p, m.label, m.color, lf)), ...this.ships.map((s) => markerTo(me, s.position, s.sim.def.name, '#4fd8f0', lf))];
    // a control pointed at from the manual: helmet tag + compass marker until you aim at it
    if (this.pointed) {
      const { ship, index } = this.pointed;
      const c = ship.sim.def.controls[index];
      const at = new THREE.Vector3(...ship.sim.controlWorld(index));
      const t = this.interaction.target;
      if (t?.kind === 'control' && t.ship === ship && ship.sim.def.controls[t.index].key === c.key && t.inReach) this.pointAt(null);
      else {
        markers.push(markerTo(me, at, c.name, '#ffb347', lf));
        this.hud.updateTag(-1, c.name, at, this.camera, '#ffb347');
      }
    }
    for (const r of this.remotes.values()) {
      const color = cssColor(SUIT_STRIPES[r.info.variant % SUIT_STRIPES.length]);
      markers.push(markerTo(me, r.position, r.info.name, color, lf));
      const head = this.upFrom(r.position, 2.05);
      this.hud.updateTag(r.info.id, r.info.name, head, this.camera, color);
    }
    // the world's people: their name when close enough to read it
    for (const n of this.npcs.values()) {
      if (n.position.distanceTo(me) < NPC_TAG_M) this.hud.updateTag(NPC_TAG - n.info.id, n.info.name, this.upFrom(n.position, 2.05), this.camera, '#d8dde3');
      else this.hud.removeTag(NPC_TAG - n.info.id);
    }
    // where the astronaut faces, on the local horizon
    const fwd = this.me.root.getWorldDirection(_fwd).negate();
    this.hud.update({
      heading: Math.atan2(fwd.dot(_v3.fromArray(lf.east)), fwd.dot(_v4.fromArray(lf.north))),
      position: this.me.root.position,
      speed: Math.hypot(this.ctl.velocity.x, this.ctl.velocity.z),
      // elevation over the body's mean sphere (at the base: its datum, as always)
      altitude: altitudeOf(bodyAt(me.toArray()), me.toArray()),
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
      tool: this.toolReadout(),
      station: this.gunnery.displayStatus(performance.now() / 1000),
      zoom: this.rig.magnification,
      dead: this.dead,
      markers,
    });

    if (this.motion.enabled) this.motion.frame(dt, this.camera.position, this.motionSubjects());
    this.diag.frame(dt * 1000);
    for (const g of this.grounds) g.terrain.material.wireframe = this.diag.wireframe;
    this.diag.setPhysicsLines(this.diag.physicsLines ? this.physics.world.debugRender() : null, this.bubbleMatrix(bp));
    this.systems.endFrame();
    const sysMs: Record<string, string> = {};
    if (this.diag.visible) for (const [n, ms] of this.systems.timings) sysMs[`· ${n} ms`] = ms.toFixed(2);
    this.diag.update(this.pipeline.renderer, !this.diag.visible ? sysMs : {
      ...sysMs,
      'calidad automática': this.pipeline.governorState,
      'pico (5 s)': this.spikes.describe(),
      'sombras fuera': this.shadowCull ? `${this.shadowCull.culled} de ${this.shadowCull.seen} proyectores · ${this.shadowCull.drawn} en cascadas` : 'todas (?noshcull)',
      'sim steps/frame': this.loop.lastSteps,
      'terrain jobs': this.pool.busy,
      'rigid bodies': this.physics.world.bodies.len(),
      colliders: this.physics.world.colliders.len(),
      debris: this.debris.count + this.scrap.count,
      'player pos': `${this.me.root.position.x.toFixed(1)}, ${this.me.root.position.y.toFixed(1)}, ${this.me.root.position.z.toFixed(1)}`,
      grounded: this.ctl.grounded ? 'sí' : 'no',
      'rtt ms': Math.round(this.net.rtt),
      audio: audioStats(),
      'PiP capturas': this.pip.captures,
    });
    if (render) {
      const r0 = performance.now();
      this.shadowCull?.before();
      this.pipeline.render(dt);
      this.shadowCull?.after();
      this.renderMs = performance.now() - r0;
    }
    if (this.loop.lastSteps > 0) input.endFrame();
  }
}

const _jetVel = new THREE.Vector3();
const _sight = new THREE.Vector3();
const _fb = new THREE.Vector3();
// audio scratch (audioFrame)
const _aBack = new THREE.Vector3();
const _aTip = new THREE.Vector3();
const _aJet: V3 = [0, 0, 0];
const _aWeld: V3 = [0, 0, 0];
const _aHelm = { alive: true, work: 0, o2: 1, fuel: 1, jetting: false };
const _bm = new THREE.Matrix4();
const _bq = new THREE.Quaternion();
const _one = new THREE.Vector3(1, 1, 1);
const _fwd = new THREE.Vector3();
const _v3 = new THREE.Vector3();
const _v4 = new THREE.Vector3();
const _lf: LocalFrame = localFrame();

/**
 * A heading carried from a frame turned `qa` to one turned `qb` (world rotations): the same
 * direction on the ground, measured in the new frame's axes.
 */
function carryYaw(qa: readonly number[], qb: readonly number[], yaw: number) {
  const f = new THREE.Vector3(-Math.sin(yaw), 0, -Math.cos(yaw)).applyQuaternion(new THREE.Quaternion(qa[0], qa[1], qa[2], qa[3]));
  f.applyQuaternion(new THREE.Quaternion(qb[0], qb[1], qb[2], qb[3]).invert());
  return Math.atan2(-f.x, -f.z);
}
const _identityQ = new THREE.Quaternion();
const _eye = new THREE.Vector3();
const SUN_WORLD = sunDirection(SUN.az, SUN.el);
const _seat = new THREE.Vector3();
const _pv = new THREE.Matrix4();
const _rooms = new Set<number>();
const _frameQ = new THREE.Quaternion();
const _fq = new THREE.Quaternion();
const _up = new THREE.Vector3(0, 1, 0);

/** One line about an orbit for the helmet display. */
function orbitLine(o: OrbitInfo) {
  const km = (m: number) => (Number.isFinite(m) ? `${(m / 1000).toFixed(1)} km` : '∞');
  const state = o.eccentricity >= 1 ? 'ESCAPE' : o.orbiting ? 'EN ÓRBITA' : 'SUBORBITAL';
  const period = Number.isFinite(o.period) ? `   PERIODO ${Math.floor(o.period / 60)} min` : '';
  return `${state}   ALT ${km(o.altitude)}   AP ${km(o.apoapsis)}   PE ${km(o.periapsis)}   V ${Math.round(o.speed)} m/s (circular ${Math.round(o.circular)})${period}`;
}

/** Voices sounding (F3): one-shots + loops / loop sources, and the context's state. */
function audioStats() {
  const s = sfx.stats();
  return `${s.shots} + ${s.loops} / ${s.sources} · ${sfx.ctx?.state ?? 'sin audio'} · vacío ${sfx.mode === 'physical' ? 'físico' : 'amortiguado'}`;
}

function round(v: number, d: number) {
  const k = 10 ** d;
  return Math.round(v * k) / k;
}

/** A compass marker: bearing (0 = north, clockwise toward east) and distance, on the local horizon `f`. */
function markerTo(from: THREE.Vector3, to: THREE.Vector3, label: string, color: string, f: LocalFrame) {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const dz = to.z - from.z;
  const e = dx * f.east[0] + dy * f.east[1] + dz * f.east[2];
  const n = dx * f.north[0] + dy * f.north[1] + dz * f.north[2];
  return { label, bearing: Math.atan2(e, n), distance: Math.hypot(dx, dy, dz), color };
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

/** A frame's name for the diagnostics. */
const frameName = (fr: number) => (fr === WORLD_FRAME ? 'mundo' : `nave ${fr}`);
const _pp: V3 = [0, 0, 0];
const _sv = new THREE.Vector3();
