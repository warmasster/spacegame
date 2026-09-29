import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import { doorAxis, hatchPlate, keelModules, moduleBase, partKey, SEAT_BOXES, nacellePylons, type ControlDef, type ControlKind, type PanelDef, type PartDef, type SeatBox, type SeatDef, type SubsystemId } from '../../shared/ship/def';
import { maker } from '../../shared/ship/catalog/makers';
import { LOCK } from '../../shared/ship/modules/airlock';
import { RX, type Reactor } from '../../shared/ship/modules/reactor';
import type { Engine } from '../../shared/ship/modules/engines';
import type { V2, V3 } from '../../shared/ship/geom';
import type { ShipSim } from '../../shared/ship/sim';
import { buildFrames, buildPanels, indexGeometry, Parts, strip, type PanelMatKey, type PanelRange } from './geometry';
import { patchInteriorLights, setInteriorLight } from './interiorLights';
import { GearFx } from './gear';
import { ThrusterFx } from './thrusterFx';
import type { ShipPose } from '../../shared/ship/flight';
import type { Surfaces } from '../../shared/space/body';
import { glassMaterial, LampMaterial, litMaterial, panelMaterial, sharpText } from './materials';
import { machineModel, propModel, type Slot } from './models';
import { ReasonHold } from '../../shared/ship/hold';
import { ShipScreens, type ShipAnimState } from './screens';
import { lights } from '../render/lightPool';
import { origin } from '../render/origin';

const lin = (r: number, g: number, b: number) => new THREE.Color().setRGB(r, g, b, THREE.LinearSRGBColorSpace);

const frameMatrix = (c: V3, u: V3, v: V3, n: V3) => new THREE.Matrix4().makeBasis(new THREE.Vector3(...u), new THREE.Vector3(...v), new THREE.Vector3(...n)).setPosition(...c);

const LANDING_ANGLE = THREE.MathUtils.degToRad(34);

/** Beyond this much past its bounding sphere (m), a ship draws no interior (LOD). */
const INTERIOR_REACH = 40;
/** Rotating drums turn this many faces a second. */
const DRUM_RATE = 1.6;
/** Beyond this, not its small exterior details either (pipes, chrome, lamp housings, rams, shutters): the silhouette stays. */
const DETAIL_REACH = 250;
/** Decor materials that make the silhouette (kept at any distance); the rest is detail. */
const SILHOUETTE = new Set(['paint', 'paintDark', 'hull', 'under']);
/** Meshes smaller than this (bounding radius, m) cast no sun shadow. */
const SMALL_CASTER = 0.1;

// scratch for the per-frame update (nothing is allocated per frame)
const _m = new THREE.Matrix4();
const _L = new THREE.Matrix4();
const _R = new THREE.Matrix4();
const _T = new THREE.Matrix4();
const _v = new THREE.Vector3();
const _v2 = new THREE.Vector3();
const _v3 = new THREE.Vector3();
const _s = new THREE.Vector3();
const _q = new THREE.Quaternion();
const _qI = new THREE.Quaternion();
const _col = new THREE.Color();
const _center = new THREE.Vector3();
const _up = new THREE.Vector3(0, 1, 0);
const blink = (time: number, hz: number, duty = 0.5) => (time * hz) % 1 < duty;

/**
 * Everything visible of a ship, in ship space under `root` (placed at the ship's world pose):
 * hull panels (merged per material, rebuilt when one is blown out or restored), structure,
 * props, consoles with animated controls and backlit labels, displays, doors, ramp, shutters,
 * gear, and every lamp (one draw call).
 */
export class ShipView {
  readonly root = new THREE.Group();
  readonly screens: ShipScreens;
  /** Per-panel heat after a blast (0..1, decays). */
  readonly heat: Float32Array;
  readonly mats: Record<string, THREE.Material>;
  private panelMeshes = new Map<PanelMatKey, THREE.Mesh>();
  private ranges: PanelRange[][] = [];
  private lampMat: LampMaterial;
  private lampIds = new Map<string, number>();
  private consoleMesh: THREE.Mesh | null = null;
  private labelMesh: THREE.Mesh | null = null;
  private labelMat: THREE.MeshBasicMaterial | null = null;
  /** Rotating drums (buildDrums) and the drum each control rides on (-1: none). */
  private drums: Array<{
    key: string;
    faces: number;
    step: number;
    /** Where it has turned to, in faces (0..faces). */
    theta: number;
    /** Drum-local → ship (x the axis, z out of face 0) and back. */
    A: THREE.Matrix4;
    Ainv: THREE.Matrix4;
    group: THREE.Group;
    faceGroups: THREE.Group[];
    /** Face k's place on the drum, and where it is now (ship space, with the turn). */
    slot: THREE.Matrix4[];
    faceM: THREE.Matrix4[];
    dirty: boolean;
  }> = [];
  private ctlDrum = new Int16Array(0);
  private hiddenHosts = '';
  private doorLeaves = new Map<string, [THREE.Mesh, THREE.Mesh]>();
  private hatchPlates = new Map<string, THREE.Mesh>();
  private ramp = new THREE.Group();
  private pistons: { barrel: THREE.InstancedMesh; rod: THREE.InstancedMesh; a: V3[]; b: V3[] } | null = null;
  /** Circuit that makes a switch actually do something (its LED goes amber when that circuit is dead). */
  private effect = new Map<string, SubsystemId>();
  /** Reactor run switches (their LED is green/red like a breaker). */
  private runKeys = new Set<string>();
  private shield: THREE.InstancedMesh;
  private parts: Record<'cap' | 'bat' | 'handle' | 'rocker' | 'knob' | 'lid', THREE.InstancedMesh>;
  private partSlot: Array<{ kind: 'cap' | 'bat' | 'handle' | 'rocker' | 'knob' | 'lid' | null; i: number }> = [];
  /**
   * Machines: the materials a deploying one tints with the welder, its animation, or `tex` = merged
   * (tinted through `partTex`).
   */
  private machines: Array<{ part: PartDef; mats: Array<{ mat: THREE.MeshStandardMaterial; base: THREE.Color }>; animate: ((t: number) => void) | null; mover: string | null; tex: boolean }> = [];
  /** Per part (3 texels from (index + 1)·3): maker colour, welder tint (a = on), welder glow. */
  private partData: Float32Array;
  private partTex: THREE.DataTexture;
  /** Body, trim and maker's colour of every merged machine (see `patchParts`). */
  private partMats: Record<'body' | 'trim' | 'accent', THREE.MeshStandardMaterial>;
  /** Machine integrity tint, only while the welder is in hand. */
  private welder = false;
  private shown: Float32Array;
  private press: Float32Array;
  private lightSlots: Array<{ zone: number; pos: THREE.Vector3 }> = [];
  /** Landing floodlight: where it sits and aims (ship space); the light comes from the pool. */
  private landing = { pos: new THREE.Vector3(), dir: new THREE.Vector3(0, -1, 0) };
  private labelAtlas: { tex: THREE.CanvasTexture; rects: Map<string, [number, number, number, number, number]> };
  private lastRamp = -1;
  private pistonsAt = -1;
  /** Refusals light the button only after they have held still (see ReasonHold). */
  private blocks = new ReasonHold(0.35);
  private refused = new Set<number>();
  /** Control marked from the manual: its LED blinks cyan. */
  private pointed = -1;
  private lastShield = -1;
  /** Number of static decor meshes (diagnostics). */
  decorCount = 0;
  /** Seats with someone in them (set by the game): their umbilical port lights up. */
  readonly seatOccupied: boolean[] = [];

  /** Landing gear legs and thruster exhausts (animated, ship space). */
  private gear!: GearFx;
  private exhaust: ThrusterFx;

  // ---- precomputed for the per-frame update ------------------------------------------------------
  /** Each control's frame (ship space) and its LED lamp. */
  private ctlFrame: THREE.Matrix4[] = [];
  private ledSlot: Int32Array;
  /** What each control's part was last drawn with: throw, press, hidden (only changes are written). */
  private drawnS: Float32Array;
  private drawnP: Float32Array;
  private drawnH: Int8Array;
  private zoneLamp: Int32Array;
  private indLamp: number[][] = [];
  private indReactor: Array<Reactor | undefined> = [];
  private annLamp: Int32Array;
  /** Per annunciator lamp: the alert variables that light it and which of them are warnings. */
  private annAlerts: Array<{ vars: Int32Array; hot: Uint8Array }> = [];
  private seatLamp: Int32Array;
  private lamp = { emergency: 0, navRed: 0, navGreen: 0, strobe: 0, beacon: 0, landing: 0, nozzle: 0, caution: 0 };
  private reactorMods: Reactor[];
  private engineMods: Engine[];
  private lockPhase: number;
  private doorAxes: V3[];
  private doorOpen: Float32Array;
  private hatchOpen: Float32Array;
  private shieldFrames: THREE.Matrix4[];
  /** Per zone, ship space: the box centre and half extents the cabin lights are clipped to. */
  private zoneBox: Array<{ center: THREE.Vector3; half: THREE.Vector3 }>;
  /** Pose last drawn (a parked ship leaves every matrix alone). */
  private lastPose = new Float64Array(7).fill(NaN);
  /** When the refusals were last worked out (they follow the state, ~20 Hz, not the frame rate). */
  private blockedAt = -Infinity;
  private blockedVer = -1;
  private consolesVer = -1;
  private machineState: Array<{ mover: number; ratio: number; welder: boolean }> = [];
  /** Interior-only objects: hidden when the camera is far outside (LOD). */
  private interior: THREE.Object3D[] = [];
  private near = true;
  /** Small exterior details: hidden from further away still (LOD). */
  private detail: THREE.Object3D[] = [];
  private detailed = true;
  /** The merged furniture and machines of each room (compartment index), for portal culling. */
  private roomMeshes = new Map<number, THREE.Object3D[]>();
  private roomsCulled = false;
  /** Machine groups that animate (their transforms change). */
  private animated = new Set<THREE.Object3D>();
  /** Ship-space bounding sphere (culling of the instanced parts, interior LOD). */
  readonly bounds = new THREE.Sphere();

  constructor(
    private sim: ShipSim,
    private csm: CSM | null,
    /** The ground of each body (the flight displays measure the height above it). */
    surfaces: Surfaces,
  ) {
    const def = sim.def;
    this.root.name = `ship-${sim.id}`;
    // everything of the ship lives under its pose (flight will move it; parked it never changes)
    this.root.position.set(...sim.pose.p);
    this.root.quaternion.set(...sim.pose.q);
    this.root.updateMatrixWorld(true);
    this.heat = new Float32Array(def.panels.length);
    this.shown = new Float32Array(def.controls.length).map((_, i) => sim.sw[def.controls[i].key] ?? 0);
    this.press = new Float32Array(def.controls.length);

    const inside = <T extends THREE.MeshStandardMaterial>(m: T) => patchInteriorLights(m);
    const livery = def.livery;
    const hullColor = lin(...livery.hull);
    this.mats = {
      hull: panelMaterial('hull', csm, { color: hullColor, roughness: 0.62, metalness: 0.15 }, livery.stripe),
      lining: inside(panelMaterial('lining', csm, { color: lin(0.2, 0.21, 0.22), roughness: 0.82, metalness: 0.05, envMapIntensity: 0.15 })),
      deck: inside(panelMaterial('deck', csm, { color: lin(0.11, 0.115, 0.12), roughness: 0.42, metalness: 0.85, envMapIntensity: 0.2 })),
      under: litMaterial(csm, { color: lin(0.05, 0.05, 0.05), roughness: 0.85 }),
      glass: glassMaterial(csm),
      edge: inside(litMaterial(csm, { color: lin(0.07, 0.072, 0.075), roughness: 0.55, metalness: 0.6, envMapIntensity: 0.5 })),
      frame: inside(litMaterial(csm, { color: lin(0.07, 0.075, 0.085), roughness: 0.5, metalness: 0.65, envMapIntensity: 0.5 })),
      paint: inside(litMaterial(csm, { color: hullColor, roughness: 0.62, metalness: 0.15 })),
      paintDark: inside(litMaterial(csm, { color: lin(0.06, 0.062, 0.066), roughness: 0.6, metalness: 0.2 })),
      accent: inside(litMaterial(csm, { color: lin(...livery.accent), roughness: 0.55, metalness: 0.1 })),
      dark: inside(litMaterial(csm, { color: lin(0.035, 0.037, 0.04), roughness: 0.45, metalness: 0.7, envMapIntensity: 0.6 })),
      strap: inside(litMaterial(csm, { color: lin(0.3, 0.12, 0.02), roughness: 0.85, envMapIntensity: 0.2 })),
      chrome: inside(litMaterial(csm, { color: lin(0.62, 0.62, 0.64), roughness: 0.16, metalness: 1 })),
      seat: inside(litMaterial(csm, { color: lin(0.045, 0.05, 0.058), roughness: 0.92, envMapIntensity: 0.2 })),
      crate: inside(litMaterial(csm, { color: lin(0.32, 0.2, 0.05), roughness: 0.72, metalness: 0.1, envMapIntensity: 0.3 })),
      crate2: inside(litMaterial(csm, { color: lin(0.12, 0.15, 0.17), roughness: 0.7, metalness: 0.2, envMapIntensity: 0.3 })),
      pipe: inside(litMaterial(csm, { color: lin(0.13, 0.13, 0.12), roughness: 0.5, metalness: 0.6, envMapIntensity: 0.3 })),
      console: inside(litMaterial(csm, { color: lin(0.03, 0.032, 0.036), roughness: 0.6, metalness: 0.4, envMapIntensity: 0.25 })),
      cap: inside(litMaterial(csm, { color: lin(0.55, 0.56, 0.58), roughness: 0.45, metalness: 0.1, envMapIntensity: 0.3 })),
      rocker: inside(litMaterial(csm, { color: lin(0.75, 0.75, 0.72), roughness: 0.4, envMapIntensity: 0.3 })),
      door: inside(litMaterial(csm, { map: doorTexture(), roughness: 0.6, metalness: 0.3, envMapIntensity: 0.3 })),
      // machine bodies: an industrial grey of their own, whatever the hull colour
      machine: inside(litMaterial(csm, { color: lin(0.24, 0.25, 0.26), roughness: 0.55, metalness: 0.35, envMapIntensity: 0.4 })),
      cells: inside(litMaterial(csm, { color: lin(0.015, 0.03, 0.08), roughness: 0.25, metalness: 0.8, envMapIntensity: 1.2 })),
      glassy: inside(litMaterial(csm, { color: lin(0.05, 0.16, 0.3), roughness: 0.1, metalness: 0.2, emissive: lin(0.01, 0.04, 0.08) })),
      fabric: inside(litMaterial(csm, { color: lin(0.1, 0.13, 0.17), roughness: 0.95, envMapIntensity: 0.1 })),
    };

    // ---- per-part colours and tint for the merged machines -----------------------------------------
    {
      const texels = (def.parts.length + 1) * 3;
      const h = Math.max(1, Math.ceil(texels / PART_TEX_W));
      this.partData = new Float32Array(PART_TEX_W * h * 4);
      this.partTex = new THREE.DataTexture(this.partData, PART_TEX_W, h, THREE.RGBAFormat, THREE.FloatType);
      this.partTex.minFilter = this.partTex.magFilter = THREE.NearestFilter;
      this.partTex.generateMipmaps = false;
      this.partMats = {
        body: patchParts(this.machineMaterial('body'), this.partTex, false),
        trim: patchParts(this.machineMaterial('trim'), this.partTex, false),
        accent: patchParts(this.machineMaterial('accent'), this.partTex, true),
      };
    }

    // ---- lamps: register every emitter first (the shader needs the count) -------------------------
    for (const l of def.loads) this.effect.set(l.key, l.circuit);
    for (const m of def.movers) this.effect.set(m.key, m.circuit);
    for (const r of this.reactors()) this.runKeys.add(r.keys.run);
    const lampNames = [...def.zones.map((z) => `zone:${z.id}`), 'emergency', 'nav-red', 'nav-green', 'strobe', 'beacon', 'landing', 'nozzle', 'caution-lens'];
    def.indicators.forEach((ind, i) => {
      if (ind.kind === 'airlock') for (let k = 0; k < 3; k++) lampNames.push(`ind:${i}:${k}`);
      else lampNames.push(`ind:${i}`);
    });
    def.annunciator?.lamps.forEach((_, i) => lampNames.push(`ann:${i}`));
    for (const st of def.seats) lampNames.push(`seat:${st.id}`);
    for (const c of def.controls) lampNames.push(`led:${c.index}`);
    lampNames.forEach((n, i) => this.lampIds.set(n, i));
    this.lampMat = new LampMaterial(lampNames.length);
    this.labelAtlas = buildLabelAtlas([...def.consoles.map((c) => c.title), ...def.controls.map((c) => c.label), ...(def.annunciator?.lamps ?? [])]);

    this.rebuildPanels();
    const frames = new THREE.Mesh(indexGeometry(buildFrames(def)), this.mats.frame);
    frames.castShadow = frames.receiveShadow = true;
    frames.name = 'frames';
    this.root.add(frames);
    this.buildDecor();
    this.gear = new GearFx(def, { dark: this.mats.dark, chrome: this.mats.chrome });
    this.exhaust = new ThrusterFx(sim);
    this.root.add(this.gear.group, this.exhaust.group);
    this.rebuildConsoles();
    this.parts = this.buildControlParts();
    this.screens = new ShipScreens(sim, surfaces);
    for (const m of this.screens.meshes) this.root.add(m);
    this.buildDoors();
    if (def.ramp) {
      this.buildRamp();
      this.pistons = this.buildPistons();
    }
    const plates = def.shield?.plates.length ?? 0;
    this.shield = new THREE.InstancedMesh(new THREE.BoxGeometry(1, 1, 1), this.mats.paint, Math.max(1, plates));
    this.shield.count = plates;
    this.shield.castShadow = this.shield.receiveShadow = true;
    this.shield.frustumCulled = false;
    this.root.add(this.shield);

    // cabin lights (shader lights, see interiorLights.ts: the nearest ones of every ship get drawn)
    def.zones.forEach((z, zi) => {
      for (const p of z.lights) this.lightSlots.push({ zone: zi, pos: new THREE.Vector3(...p) });
    });
    // landing floodlight: the one real light the ship adds (it has to light the terrain), taken
    // from the scene's light pool while it is switched on (render/lightPool.ts)
    const ll = def.extLights.filter((l) => l.kind === 'landing');
    const lp = ll.reduce((a, l) => a.add(new THREE.Vector3(...l.pos)), new THREE.Vector3()).multiplyScalar(1 / Math.max(1, ll.length));
    const ld = ll.reduce((a, l) => a.add(new THREE.Vector3(...l.dir!)), new THREE.Vector3());
    this.landing.pos.copy(lp);
    if (ld.lengthSq() > 1e-9) this.landing.dir.copy(ld).normalize();

    // ---- what the per-frame update looks up, once -------------------------------------------------
    const id = (n: string) => this.lampIds.get(n)!;
    this.ctlFrame = def.controls.map((c) => frameMatrix(c.c, c.u, c.v, c.n));
    this.ledSlot = Int32Array.from(def.controls, (c) => this.lampIds.get(`led:${c.index}`) ?? -1);
    this.drawnS = new Float32Array(def.controls.length).fill(NaN);
    this.drawnP = new Float32Array(def.controls.length).fill(NaN);
    this.drawnH = new Int8Array(def.controls.length).fill(-1);
    this.zoneLamp = Int32Array.from(def.zones, (z) => id(`zone:${z.id}`));
    this.lamp = { emergency: id('emergency'), navRed: id('nav-red'), navGreen: id('nav-green'), strobe: id('strobe'), beacon: id('beacon'), landing: id('landing'), nozzle: id('nozzle'), caution: id('caution-lens') };
    this.reactorMods = this.reactors();
    this.engineMods = sim.sys.modules.filter((m): m is Engine => m.id.startsWith('engine:'));
    def.indicators.forEach((ind, i) => {
      this.indLamp.push(ind.kind === 'airlock' ? [0, 1, 2].map((k) => id(`ind:${i}:${k}`)) : [id(`ind:${i}`)]);
      this.indReactor.push(ind.kind === 'reactor-core' ? this.reactorMods.find((x) => x.part.id === ind.ref) ?? this.reactorMods[0] : undefined);
    });
    const lamps = def.annunciator?.lamps ?? [];
    this.annLamp = Int32Array.from(lamps, (_, i) => id(`ann:${i}`));
    this.annAlerts = lamps.map((name) => {
      const hits = sim.sys.alerts.filter((a) => a.lamp === name);
      return { vars: Int32Array.from(hits, (a) => sim.sys.alertIndex(a.id)), hot: Uint8Array.from(hits, (a) => (a.level === 2 ? 1 : 0)) };
    });
    this.seatLamp = Int32Array.from(def.seats, (st) => id(`seat:${st.id}`));
    this.lockPhase = sim.vars.has('lock.phase') ? sim.vars.idx('lock.phase') : -1;
    this.doorAxes = def.doors.map((d) => doorAxis(d));
    this.doorOpen = new Float32Array(def.doors.length).fill(NaN);
    this.hatchOpen = new Float32Array(def.hatches.length).fill(NaN);
    this.shieldFrames = (def.shield?.plates ?? []).map((pl) => frameMatrix(pl.c, pl.u, pl.v, pl.n));
    this.zoneBox = def.zones.map((zn) => ({
      center: new THREE.Vector3((zn.min[0] + zn.max[0]) / 2, (zn.min[1] + zn.max[1]) / 2, (zn.min[2] + zn.max[2]) / 2),
      half: new THREE.Vector3((zn.max[0] - zn.min[0]) / 2 + 0.14, (zn.max[1] - zn.min[1]) / 2 + 0.14, (zn.max[2] - zn.min[2]) / 2 + 0.14),
    }));
    this.machineState = this.machines.map(() => ({ mover: NaN, ratio: NaN, welder: false }));
    this.buildDrums();

    // bounds (ship space): the hull, the parts, the ramp swung down
    const box = sim.localBox();
    const bb = new THREE.Box3(new THREE.Vector3(...box.min), new THREE.Vector3(...box.max));
    bb.getBoundingSphere(this.bounds);
    this.bounds.radius += (def.ramp?.length ?? 0) + 1;
    // instanced parts: cull them against the ship's sphere instead of never
    for (const m of [...Object.values(this.parts), this.shield, ...(this.pistons ? [this.pistons.barrel, this.pistons.rod] : [])]) {
      m.boundingSphere = this.bounds.clone();
      m.frustumCulled = true;
    }
    // the interior goes when the camera is far outside; control parts, screens
    for (const m of Object.values(this.parts)) this.interior.push(m);
    for (const m of this.screens.meshes) this.interior.push(m);
    // and further out the small exterior details
    this.detail.push(this.shield);
    if (this.pistons) this.detail.push(this.pistons.barrel, this.pistons.rod);
    // deck hatches are inside; door leaves stay (an outer door missing would open a hole in the hull)
    for (const p of this.hatchPlates.values()) this.interior.push(p);
    // static in ship space: their matrices are computed once (the root moves them all)
    this.root.matrixAutoUpdate = false;
    this.freezeStatic();
  }

  /**
   * Everything that never moves in ship space stops recomputing its local matrix every frame, and
   * the root only updates its world matrix when the ship moved: a parked ship costs the scene
   * graph nothing. Doors, hatches, the ramp, the gear, the exhausts and animated machines stay live.
   */
  private freezeStatic() {
    const live = new Set<THREE.Object3D>([this.gear.group, this.exhaust.group, this.ramp, ...this.animated]);
    for (const [a, b] of this.doorLeaves.values()) live.add(a).add(b);
    for (const p of this.hatchPlates.values()) live.add(p);
    const walk = (o: THREE.Object3D) => {
      for (const c of o.children) {
        if (live.has(c)) {
          if (c === this.ramp) walk(c);
          continue;
        }
        freeze(c);
        walk(c);
      }
    };
    walk(this.root);
  }

  // ------------------------------------------------------------------------------------------------
  // Panels
  // ------------------------------------------------------------------------------------------------

  /** Rebuild the merged hull (after panels were blown out or restored). */
  rebuildPanels() {
    const sim = this.sim;
    const { geos, ranges } = buildPanels(sim.def, (i) => !sim.hole(i), (i) => this.damageOf(i));
    this.ranges = ranges;
    for (const [key, mesh] of this.panelMeshes) {
      mesh.geometry.dispose();
      if (!geos.has(key)) {
        this.root.remove(mesh);
        this.panelMeshes.delete(key);
      }
    }
    for (const [key, geo] of geos) {
      let mesh = this.panelMeshes.get(key);
      if (!mesh) {
        mesh = new THREE.Mesh(geo, this.mats[key]);
        mesh.name = `panels-${key}`;
        mesh.castShadow = key !== 'glass';
        mesh.receiveShadow = true;
        if (key === 'glass') mesh.renderOrder = 5;
        this.panelMeshes.set(key, mesh);
        this.root.add(mesh);
        freeze(mesh);
      } else mesh.geometry = geo;
    }
    for (let i = 0; i < this.heat.length; i++) if (this.heat[i] > 0) this.writePanel(i);
  }

  private damageOf(i: number) {
    const p = this.sim.def.panels[i];
    return Math.max(0, Math.min(1, 1 - this.sim.hp[i] / p.maxHp));
  }

  /** Damage/heat changed on a solid panel: patch its vertices in place. */
  writePanel(i: number) {
    const dmg = this.damageOf(i);
    for (const r of this.ranges[i] ?? []) {
      const mesh = this.panelMeshes.get(r.key);
      if (!mesh) continue;
      const P = mesh.geometry.getAttribute('aPanel') as THREE.BufferAttribute;
      const H = mesh.geometry.getAttribute('aHeat') as THREE.BufferAttribute;
      for (let k = r.start; k < r.start + r.count; k++) {
        P.setZ(k, dmg);
        H.setX(k, this.heat[i]);
      }
      P.needsUpdate = true;
      H.needsUpdate = true;
    }
  }

  // ------------------------------------------------------------------------------------------------
  // Static decor
  // ------------------------------------------------------------------------------------------------

  private buildDecor() {
    const def = this.sim.def;
    const P = new Parts();
    const lamps: THREE.BufferGeometry[] = [];
    const lamp = (name: string, g: THREE.BufferGeometry, m?: THREE.Matrix4) => {
      if (m) g.applyMatrix4(m);
      const s = strip(g);
      s.setAttribute('aLamp', new THREE.Float32BufferAttribute(new Float32Array(s.getAttribute('position').count).fill(this.lampIds.get(name)!), 1));
      lamps.push(s);
    };
    const T = 0.1;

    // belly tubs: keel structure under the deck, open on top (conduits run in there); only under the
    // sections that stand on the keel (an upper deck section rests on the one below)
    for (const m of keelModules(def.modules)) {
      const hw = m.profile[m.profile.length - 1][0] + T;
      const tub: V2[] = [[-hw, 0], [-hw, -0.2], [-hw + 0.25, -0.45], [hw - 0.25, -0.45], [hw, -0.2], [hw, 0], [hw - T, 0], [hw - T, -0.3], [-hw + T, -0.3], [-hw + T, 0]];
      P.extrudeZ('paintDark', tub, m.z0, m.z1);
    }
    // fairings (chin, fin, spine…) are props with their own models; pylons are sized from the
    // nacelle part boxes; the tanks, engines and RCS are drawn from those boxes
    for (const py of nacellePylons(def.parts, def.modules)) P.box('paint', py.half[0] * 2, py.half[1] * 2, py.half[2] * 2, py.c);
    for (const e of def.parts.filter((p) => p.type === 'engine')) {
      lamp('nozzle', new THREE.CircleGeometry(Math.min(e.half[0], e.half[1]) * 0.72, 24), new THREE.Matrix4().makeTranslation(e.c[0], e.c[1], e.c[2] + e.half[2] * 0.92));
    }

    // landing gear bays (the legs themselves are animated: gear.ts)
    for (const [x, , z] of def.gear?.legs ?? []) {
      P.box('paint', 0.03, 0.34, 0.9, [x - 0.3, -0.62, z]);
      P.box('paint', 0.03, 0.34, 0.9, [x + 0.3, -0.62, z]);
    }
    // VTOL nozzles a tilting nacelle ducts its exhaust to, and the lift pads' own nozzles
    for (const e of def.parts) {
      const at = e.type === 'engine' ? e.gimbal?.at : e.type === 'lift' ? ([e.c[0], e.c[1] - e.half[1], e.c[2]] as V3) : undefined;
      if (!at) continue;
      const r = e.type === 'lift' ? e.half[0] * 0.62 : Math.min(e.half[0], e.half[1]) * 0.55;
      P.lathe('dark', [[r * 0.75, 0], [r, -0.02], [r * 1.05, 0.08], [r * 0.6, 0.12]], at, undefined, 18);
    }

    // ---- interior -----------------------------------------------------------------------------------
    // seats; furniture (dash supports, bunks, galley, handrails…) is props, drawn with the machines
    for (const st of def.seats) this.buildSeat(P, st, lamp);

    // conduits: cable runs of every subsystem (seen on the ceiling, under the deck through holes)
    for (const s of def.subsystems) for (const r of s.routes) for (let i = 0; i < r.length - 1; i++) P.rod('pipe', r[i], r[i + 1], 0.03, 8);

    // cabin light strips, emergency lights on the ribs near the deck
    // strips along both edges of the flat roof of each hull section (the nose section is shorter:
    // windshield); a room with no section of its own (nested in a bigger one) gets a pair centred on it
    const strips = (zone: string, cx: number, flat: number, top: number, z0: number, z1: number, nose: boolean) => {
      const x = flat - 0.085;
      const y = top - 0.075;
      const len = z1 - z0 - 0.6;
      const zc = (z1 + z0) / 2 + (nose ? 0.2 : 0);
      for (const sx of [-1, 1]) {
        P.box('dark', 0.1, 0.03, len + 0.04, [cx + sx * x, y + 0.02, zc]);
        lamp(`zone:${zone}`, new THREE.BoxGeometry(0.07, 0.012, nose ? len - 0.4 : len), new THREE.Matrix4().makeTranslation(cx + sx * x, y, zc));
      }
    };
    def.modules.forEach((m, mi) => {
      if (!def.zones.some((z) => z.id === m.zone)) return;
      const top = Math.max(...m.profile.map((q) => q[1]));
      const flat = Math.max(...m.profile.filter((q) => Math.abs(q[1] - top) < 1e-3).map((q) => q[0]));
      strips(m.zone, 0, flat, top, m.z0, m.z1, mi === 0);
    });
    for (const zn of def.zones) {
      if (def.modules.some((m) => m.zone === zn.id)) continue;
      strips(zn.id, (zn.min[0] + zn.max[0]) / 2, (zn.max[0] - zn.min[0]) * 0.3, zn.max[1], zn.min[2], zn.max[2], false);
    }
    for (const m of def.modules) {
      const hw = m.profile[m.profile.length - 1][0];
      const base = moduleBase(m);
      for (let c = 1; c < m.cols; c++) {
        const z = m.z0 + ((m.z1 - m.z0) * c) / m.cols;
        for (const sx of [-1, 1]) lamp('emergency', new THREE.BoxGeometry(0.025, 0.035, 0.1), new THREE.Matrix4().makeTranslation(sx * (hw - 0.03), base + 0.16, z));
      }
    }

    // exterior lights: every fixture stands on its surface point `pos` along the outward normal `n`
    const Y = new THREE.Vector3(0, 1, 0);
    for (const l of def.extLights) {
      const at = new THREE.Vector3(...l.pos);
      const n = new THREE.Vector3(...l.n);
      const qn = new THREE.Quaternion().setFromUnitVectors(Y, n);
      const on = (h: number) => at.clone().addScaledVector(n, h);
      if (l.kind === 'landing') {
        // recessed floodlight: bezel ring flush with the face, reflector cup behind the lens
        const d = new THREE.Vector3(...l.dir!);
        const qd = new THREE.Quaternion().setFromUnitVectors(Y, d);
        const bezel = on(0.012);
        P.add('dark', new THREE.CylinderGeometry(0.135, 0.145, 0.024, 20), [bezel.x, bezel.y, bezel.z], qn);
        const cup = on(-0.03);
        P.add('chrome', new THREE.CylinderGeometry(0.1, 0.06, 0.08, 16, 1, true), [cup.x, cup.y, cup.z], qd);
        lamp('landing', new THREE.CircleGeometry(0.1, 16).rotateX(-Math.PI / 2), new THREE.Matrix4().compose(on(0.022), qd, new THREE.Vector3(1, 1, 1)));
      } else if (l.kind === 'beacon') {
        // rotating beacon: flat base on the skin, red dome on top
        const base = on(0.02);
        P.add('dark', new THREE.CylinderGeometry(0.1, 0.11, 0.04, 16), [base.x, base.y, base.z], qn);
        lamp('beacon', new THREE.SphereGeometry(0.075, 14, 8, 0, Math.PI * 2, 0, Math.PI / 2), new THREE.Matrix4().compose(on(0.04), qn, new THREE.Vector3(1, 1, 1)));
      } else {
        // position / strobe light: streamlined blister, lens on its tip
        const base = on(0.015);
        P.add('dark', new THREE.CylinderGeometry(0.05, 0.065, 0.03, 14), [base.x, base.y, base.z], qn);
        lamp(l.kind, new THREE.SphereGeometry(0.048, 12, 8, 0, Math.PI * 2, 0, Math.PI / 2), new THREE.Matrix4().compose(on(0.028), qn, new THREE.Vector3(1, 1, 1.25)));
      }
    }
    // console indicators (reactor core window, gear greens…) from the definition
    def.indicators.forEach((ind, i) => {
      const F = frameMatrix(ind.c, ind.u, ind.v, ind.n);
      if (ind.kind === 'reactor-core') {
        const at = F.clone().multiply(new THREE.Matrix4().makeTranslation(0, 0, 0.002));
        lamp(`ind:${i}`, new THREE.CircleGeometry(0.055, 24), at);
        P.add('chrome', new THREE.TorusGeometry(0.062, 0.008, 6, 24).applyMatrix4(at));
      } else if (ind.kind === 'gear-greens') {
        for (let k = -1; k <= 1; k++) lamp(`ind:${i}`, new THREE.BoxGeometry(0.03, 0.02, 0.006), F.clone().multiply(new THREE.Matrix4().makeTranslation(k * 0.06, 0, 0)));
      } else if (ind.kind === 'airlock') {
        // three lamps: cabin side (green), cycling (amber), outside (red)
        for (let k = -1; k <= 1; k++) {
          lamp(`ind:${i}:${k + 1}`, new THREE.CircleGeometry(0.014, 12), F.clone().multiply(new THREE.Matrix4().makeTranslation(k * 0.045, 0, 0.004)));
          P.add('chrome', new THREE.TorusGeometry(0.017, 0.003, 5, 14).applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(k * 0.045, 0, 0.003))));
        }
      }
    });
    for (const con of def.consoles) {
      const F = frameMatrix(con.c, con.u, con.v, con.n);
      if (def.annunciator && con.id === def.annunciator.console) {
        const ann = def.annunciator;
        ann.lamps.forEach((_, i) => {
          const col = i % ann.cols;
          const row = Math.floor(i / ann.cols);
          const x = ann.at[0] + (col - (ann.cols - 1) / 2) * ann.cell[0];
          const y = ann.at[1] - row * ann.cell[1] * ANN_ROW;
          lamp(`ann:${i}`, new THREE.BoxGeometry(ann.cell[0] * 0.82, ann.cell[1] * ANN_LAMP, 0.006), F.clone().multiply(new THREE.Matrix4().makeTranslation(x, y, 0.008)));
        });
      }
    }
    // indicator LED above every control (the master caution button is its own lens); the drums'
    // turn with their faces (buildDrums)
    for (const c of def.controls) {
      if (c.drum) continue;
      const F = frameMatrix(c.c, c.u, c.v, c.n);
      if (c.kind === 'master') lamp('caution-lens', new THREE.BoxGeometry(0.084, 0.054, 0.012), F.clone().multiply(new THREE.Matrix4().makeTranslation(0, 0, 0.02)));
      else lamp(`led:${c.index}`, new THREE.BoxGeometry(0.03, 0.008, 0.005), F.clone().multiply(new THREE.Matrix4().makeTranslation(0, c.half[1] + 0.012, 0.003)));
    }
    // shutter housings above every canopy pane
    for (const pl of def.shield?.plates ?? []) {
      const F = frameMatrix(pl.c, pl.u, pl.v, pl.n);
      const g = new THREE.BoxGeometry(pl.w, 0.07, 0.07);
      g.applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(0, pl.h / 2 + 0.035, 0)));
      P.add('dark', g);
    }
    // registry: wrapped round the nacelles, or flat panels on the hull sides
    const decal = registryDecal(def.name, def.registry, def.livery.tagline || def.role.toUpperCase(), def.livery.accent, this.csm);
    if (def.livery.decals !== 'nacelles')
      for (const d of def.livery.decals) {
        const g = new THREE.PlaneGeometry(d.w, d.h);
        const n = new THREE.Vector3(...d.n);
        const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 0, 1), n);
        g.applyQuaternion(q).translate(d.c[0] + n.x * 0.006, d.c[1] + n.y * 0.006, d.c[2] + n.z * 0.006);
        const m = new THREE.Mesh(g, decal);
        m.receiveShadow = true;
        this.root.add(m);
      }
    for (const e of def.livery.decals === 'nacelles' ? def.parts.filter((p) => p.type === 'engine' && p.mount) : []) {
      // wrapped on the engine's combustion chamber (models/machines.ts `engine`: radius 0.72·r),
      // between its two trim rings, a little below the side so it clears the feed pipes
      const sx = Math.sign(e.c[0]) || 1;
      const hz = e.half[2];
      const R = Math.min(e.half[0], e.half[1]) * 0.72 + 0.006;
      const len = hz * 0.56;
      const span = len / 4 / R; // the decal is 4:1
      const mid = Math.PI / 2 - 0.55;
      const g = new THREE.CylinderGeometry(R, R, len, 24, 1, true, sx * mid - span / 2, span);
      const uv = g.getAttribute('uv') as THREE.BufferAttribute;
      for (let i = 0; i < uv.count; i++) {
        const u = uv.getX(i);
        const v = uv.getY(i);
        if (sx > 0) uv.setXY(i, 1 - v, u);
        else uv.setXY(i, v, 1 - u);
      }
      g.rotateX(Math.PI / 2);
      g.translate(e.c[0], e.c[1], e.c[2] - hz * 0.425);
      const m = new THREE.Mesh(g, decal);
      m.receiveShadow = true;
      this.root.add(m);
    }
    // machines: body, trim and the maker's colour are the part's own materials (the welder tints them)
    const shared: Partial<Record<Slot, THREE.Material>> = {
      dark: this.mats.dark,
      chrome: this.mats.chrome,
      pipe: this.mats.pipe,
      cells: this.mats.cells,
      glass: this.mats.glassy,
      seat: this.mats.seat,
      console: this.mats.console,
      paint: this.mats.paint,
      paintDark: this.mats.paintDark,
      lining: this.mats.paint,
      fabric: this.mats.fabric,
      accent: this.mats.accent,
    };
    // rooms the sun reaches (a window in them): what is inside the others casts no sun shadow
    const sunlit = new Set<string>();
    for (const pn of def.panels) {
      if (pn.kind !== 'glass') continue;
      if (pn.zone) sunlit.add(pn.zone);
      if (pn.other) sunlit.add(pn.other);
    }
    const shadows = (g: THREE.Object3D, zone: string | null) => {
      const dark = zone !== null && !sunlit.has(zone);
      g.traverse((o) => {
        const m = o as THREE.Mesh;
        if (!m.isMesh) return;
        if (dark) {
          m.castShadow = false;
          return;
        }
        if (!m.geometry.boundingSphere) m.geometry.computeBoundingSphere();
        const r = m.geometry.boundingSphere!.radius * Math.max(m.scale.x, m.scale.y, m.scale.z);
        if (r < SMALL_CASTER) m.castShadow = false;
      });
      if (zone !== null) this.interior.push(g);
    };
    // Machines and furniture that never move are baked into ship space and merged per
    // (room × material × casts a shadow): a few draws per room instead of one per material of every
    // piece. Their body, trim and maker's colour are three shared materials; the maker's colour and
    // the welder's damage tint come per part from `partTex` (`aPart` on every vertex).
    const buckets = new Map<string, { zone: string | null; mat: THREE.Material; cast: boolean; geos: THREE.BufferGeometry[] }>();
    const bake = (group: THREE.Group, zone: string | null, part: number) => {
      const dark = zone !== null && !sunlit.has(zone);
      group.updateMatrixWorld(true);
      group.traverse((o) => {
        const m = o as THREE.Mesh;
        if (!m.isMesh) return;
        const mat = m.material as THREE.Material;
        const g = m.geometry.clone().applyMatrix4(m.matrixWorld);
        g.computeBoundingSphere();
        const cast = m.castShadow && !dark && g.boundingSphere!.radius >= SMALL_CASTER;
        if (mat.userData.parts) g.setAttribute('aPart', new THREE.Float32BufferAttribute(new Float32Array(g.getAttribute('position').count).fill(part + 1), 1));
        const key = `${zone ?? ''}|${mat.uuid}|${cast ? 1 : 0}`;
        let b = buckets.get(key);
        if (!b) buckets.set(key, (b = { zone, mat, cast, geos: [] }));
        b.geos.push(g);
        m.geometry.dispose();
      });
    };
    const tinted = (slot: Slot): slot is 'body' | 'trim' | 'accent' => slot === 'body' || slot === 'trim' || slot === 'accent';
    for (const part of def.parts) {
      const trimColor = lin(...maker(part.maker).trim);
      this.partAccent(part.index, trimColor);
      const mover = part.type === 'radiator' || part.type === 'solar' ? partKey(part, 'deploy', `${part.id}.deploy`) : null;
      const live = mover && def.movers.some((m) => m.key === mover) ? mover : null;
      if (!live) {
        // never moves: shared materials, baked into the room's merged meshes
        const model = machineModel(part, (slot) => (tinted(slot) ? this.partMats[slot] : shared[slot] ?? this.mats.dark));
        model.animate?.(mover ? this.sim.mover(mover) : 1);
        bake(model.group, part.zone, part.index);
        this.machines.push({ part, mats: [], animate: null, mover: null, tex: true });
        continue;
      }
      // deploys (radiators, solar wings): its own group and materials
      const own = new Map<Slot, THREE.MeshStandardMaterial>();
      const mat = (slot: Slot): THREE.Material => {
        if (!tinted(slot)) return shared[slot] ?? this.mats.dark;
        let m = own.get(slot);
        if (!m) {
          m = this.machineMaterial(slot, slot === 'accent' ? trimColor : undefined);
          own.set(slot, m);
        }
        return m;
      };
      const model = machineModel(part, mat);
      this.root.add(model.group);
      this.machines.push({ part, mats: [...own.values()].map((m) => ({ mat: m, base: m.color.clone() })), animate: model.animate, mover: live, tex: false });
      model.animate?.(this.sim.mover(live));
      if (model.animate) this.animated.add(model.group);
      shadows(model.group, part.zone);
    }
    for (const prop of def.props) {
      const model = propModel(prop, (slot) => shared[slot] ?? (slot === 'body' ? this.mats.console : this.mats.dark));
      model.animate?.(1);
      bake(model.group, prop.zone ?? null, -1);
    }
    for (const b of buckets.values()) {
      const mesh = new THREE.Mesh(indexGeometry(mergeGeometries(b.geos)!), b.mat);
      mesh.name = `furnishing-${b.zone ?? 'outside'}`;
      mesh.castShadow = b.cast;
      mesh.receiveShadow = true;
      this.root.add(mesh);
      if (b.zone !== null) {
        this.interior.push(mesh);
        const room = this.sim.sys.compIndex(b.zone);
        if (room >= 0) {
          let list = this.roomMeshes.get(room);
          if (!list) this.roomMeshes.set(room, (list = []));
          list.push(mesh);
        }
      }
      for (const g of b.geos) g.dispose();
    }
    this.partTex.needsUpdate = true;

    for (const [key, geo] of P.build()) {
      const mesh = new THREE.Mesh(indexGeometry(geo), this.mats[key]);
      mesh.name = `decor-${key}`;
      mesh.castShadow = mesh.receiveShadow = true;
      this.root.add(mesh);
      if (!SILHOUETTE.has(key)) this.detail.push(mesh);
      this.decorCount++;
    }
    const lampMesh = new THREE.Mesh(indexGeometry(mergeGeometries(lamps)!), this.lampMat);
    lampMesh.name = 'lamps';
    // the lamp shader only collapses switched-off lamps: the geometry's own bounds hold
    this.root.add(lampMesh);
  }

  /**
   * Crew seat for a suited astronaut (dimensions: SEAT_BOXES): swivel pedestal on a floor track,
   * cushioned pan with thigh bolsters, two padded wings either side of the PLSS well, the dock
   * plate behind the pack with its umbilical port, the top bar and the helmet headrest, harness.
   */
  private buildSeat(P: Parts, st: SeatDef, lamp: (name: string, g: THREE.BufferGeometry, m?: THREE.Matrix4) => void) {
    const F = new THREE.Matrix4().compose(new THREE.Vector3(...st.root), new THREE.Quaternion().setFromEuler(new THREE.Euler(0, st.yaw, 0)), new THREE.Vector3(1, 1, 1));
    // seat space: x right, y up, z toward the backrest (+Z of the root frame)
    const put = (mat: string, g: THREE.BufferGeometry, x: number, y: number, z: number, rx = 0) => {
      if (rx) g.rotateX(rx);
      g.translate(x, y, z).applyMatrix4(F);
      P.add(mat, g);
    };
    const box = (mat: string, b: SeatBox, grow: V3 = [0, 0, 0], off: V3 = [0, 0, 0]) =>
      put(mat, new THREE.BoxGeometry((b.half[0] + grow[0]) * 2, (b.half[1] + grow[1]) * 2, (b.half[2] + grow[2]) * 2), b.c[0] + off[0], b.c[1] + off[1], b.c[2] + off[2]);
    const B = SEAT_BOXES;
    // floor track + pedestal
    put('dark', new THREE.BoxGeometry(0.42, 0.03, 0.9), 0, 0.015, 0.05);
    for (const x of [-0.16, 0.16]) put('chrome', new THREE.BoxGeometry(0.03, 0.02, 0.86), x, 0.035, 0.05);
    put('dark', new THREE.CylinderGeometry(0.075, 0.09, 0.36, 14), 0, 0.2, 0.05);
    put('dark', new THREE.BoxGeometry(0.5, 0.03, 0.7), 0, 0.37, 0.02);
    // pan: frame + cushion + bolsters
    box('dark', B.pan, [0.01, -0.02, 0.01], [0, -0.025, 0]);
    box('seat', B.pan, [-0.04, -0.015, -0.03], [0, 0.02, -0.01]);
    for (const x of [-0.27, 0.27]) put('seat', new THREE.BoxGeometry(0.07, 0.07, 0.72), x, 0.5, -0.06);
    // PLSS well: dock plate behind the pack, padded wings either side
    box('dark', B.dock);
    put('seat', new THREE.BoxGeometry(0.44, 0.62, 0.02), 0, 0.98, B.dock.c[2] - B.dock.half[2] - 0.01);
    box('dark', B.wingL, [0, 0, 0], [0, 0, 0]);
    box('dark', B.wingR);
    for (const b of [B.wingL, B.wingR]) put('seat', new THREE.BoxGeometry(0.02, b.half[1] * 1.8, b.half[2] * 1.7), b.c[0] - Math.sign(b.c[0]) * (b.half[0] + 0.01), b.c[1], b.c[2] - 0.01);
    // top bar + headrest (meets the back of the helmet above the pack)
    put('dark', new THREE.BoxGeometry(0.67, 0.07, 0.07), 0, B.head.c[1] - 0.02, B.dock.c[2]);
    box('seat', B.head);
    // umbilical port on the dock plate: lit when a pack is docked (see update)
    const port = new THREE.Vector3(0.12, 1.05, B.dock.c[2] - B.dock.half[2] - 0.006).applyMatrix4(F);
    P.add('chrome', new THREE.TorusGeometry(0.03, 0.007, 6, 16).applyQuaternion(new THREE.Quaternion().setFromEuler(new THREE.Euler(0, st.yaw, 0))), [port.x, port.y, port.z]);
    lamp(`seat:${st.id}`, new THREE.CircleGeometry(0.022, 12).rotateY(Math.PI), new THREE.Matrix4().compose(port, new THREE.Quaternion().setFromEuler(new THREE.Euler(0, st.yaw, 0)), new THREE.Vector3(1, 1, 1)));
    // harness: shoulder straps from the top bar down the wings, lap belt across the pan
    for (const x of [-0.2, 0.2]) put('strap', new THREE.BoxGeometry(0.05, 0.6, 0.012), x, 1.1, 0.2, -0.08);
    put('strap', new THREE.BoxGeometry(0.58, 0.05, 0.012), 0, 0.52, -0.2);
    put('chrome', new THREE.BoxGeometry(0.07, 0.06, 0.018), 0, 0.52, -0.21);
  }

  // ------------------------------------------------------------------------------------------------
  // Consoles, labels, control parts
  // ------------------------------------------------------------------------------------------------

  private hostHidden = (host: number) => host >= 0 && this.sim.hole(host);

  /** Console bodies, static control bases and labels of the consoles that still exist. */
  private rebuildConsoles() {
    const def = this.sim.def;
    const key = def.consoles.map((c) => (this.hostHidden(c.host) ? 1 : 0)).join('');
    if (key === this.hiddenHosts && this.consoleMesh) return;
    this.hiddenHosts = key;
    const P = new Parts();
    const labels: THREE.BufferGeometry[] = [];
    const label = (text: string, F: THREE.Matrix4, x: number, y: number, h: number) => {
      const r = this.labelAtlas.rects.get(text);
      if (!r) return;
      const [u0, v0, u1, v1, aspect] = r;
      const g = new THREE.PlaneGeometry(h * aspect, h);
      const uv = g.getAttribute('uv') as THREE.BufferAttribute;
      for (let i = 0; i < uv.count; i++) uv.setXY(i, uv.getX(i) ? u1 : u0, uv.getY(i) ? v1 : v0);
      g.applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(x, y, 0.0025)));
      labels.push(g);
    };
    for (const con of def.consoles) {
      if (this.hostHidden(con.host) || con.drum) continue;
      const F = frameMatrix(con.c, con.u, con.v, con.n);
      const body = new THREE.BoxGeometry(con.w, con.h, con.depth);
      body.applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(0, 0, -con.depth / 2)));
      P.add('console', body);
      const lip = new THREE.BoxGeometry(con.w + 0.03, con.h + 0.03, 0.012);
      lip.applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(0, 0, -con.depth + 0.004)));
      P.add('dark', lip);
      if (con.h > 0.2) label(con.title, F, 0, con.h / 2 - 0.028, 0.024);
      for (const c of def.controls) {
        if (c.console !== con.id) continue;
        const lx = (c.c[0] - con.c[0]) * con.u[0] + (c.c[1] - con.c[1]) * con.u[1] + (c.c[2] - con.c[2]) * con.u[2];
        const ly = (c.c[0] - con.c[0]) * con.v[0] + (c.c[1] - con.c[1]) * con.v[1] + (c.c[2] - con.c[2]) * con.v[2];
        const base = BASES[c.kind];
        const g = new THREE.BoxGeometry(base[0], base[1], base[2]);
        g.applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(lx, ly, base[2] / 2)));
        P.add('dark', g);
        if (c.label) label(c.label, F, lx, ly - c.half[1] - 0.022, 0.017);
      }
      if (def.annunciator && con.id === def.annunciator.console) {
        const ann = def.annunciator;
        ann.lamps.forEach((name, i) => {
          const col = i % ann.cols;
          const row = Math.floor(i / ann.cols);
          const x = ann.at[0] + (col - (ann.cols - 1) / 2) * ann.cell[0];
          // its name just under the lamp, clear of the next row's lamp
          const y = ann.at[1] - row * ann.cell[1] * ANN_ROW - ann.cell[1] * ANN_LAMP * 0.5 - 0.0075;
          label(name, F, x, y, 0.011);
        });
      }
    }
    // control LEDs live in the lamp mesh; hide the ones whose console is gone
    for (const c of def.controls) this.lampMat.setVisible(this.lampIds.get(`led:${c.index}`)!, !this.hostHidden(c.host));
    def.indicators.forEach((ind, i) => {
      const con = def.consoles.find((c) => c.id === ind.console)!;
      for (const name of ind.kind === 'airlock' ? [0, 1, 2].map((k) => `ind:${i}:${k}`) : [`ind:${i}`]) this.lampMat.setVisible(this.lampIds.get(name)!, !this.hostHidden(con.host));
    });
    for (const m of [this.consoleMesh, this.labelMesh]) {
      if (!m) continue;
      this.root.remove(m);
      m.geometry.dispose();
    }
    const built = P.build();
    const geos = [...built.entries()];
    // console + dark merged via groups
    const merged = indexGeometry(mergeGeometries(geos.map(([, g]) => g), true)!);
    const old = [this.consoleMesh, this.labelMesh];
    this.consoleMesh = new THREE.Mesh(merged, geos.map(([k]) => this.mats[k]));
    this.consoleMesh.name = 'consoles';
    this.consoleMesh.castShadow = this.consoleMesh.receiveShadow = true;
    this.consoleMesh.visible = this.near;
    this.root.add(this.consoleMesh);
    freeze(this.consoleMesh);
    this.labelMesh = null;
    if (labels.length) {
      this.labelMesh = new THREE.Mesh(mergeGeometries(labels)!, this.labelMaterial());
      this.labelMesh.name = 'labels';
      this.labelMesh.renderOrder = 3;
      this.labelMesh.visible = this.near;
      this.root.add(this.labelMesh);
      freeze(this.labelMesh);
    }
    // they come and go with the consoles: keep the interior list current
    this.interior = this.interior.filter((o) => !old.includes(o as THREE.Mesh));
    this.interior.push(this.consoleMesh);
    if (this.labelMesh) this.interior.push(this.labelMesh);
  }

  /** The backlit labels' material (shared by the consoles and the drums' faces). */
  private labelMaterial() {
    return (this.labelMat ??= sharpText(new THREE.MeshBasicMaterial({ map: this.labelAtlas.tex, transparent: true, depthWrite: false, color: new THREE.Color(0.9, 0.95, 1) })));
  }

  /**
   * Rotating drums (DrumFace): a prism turning about the face's u axis, one face per console, each
   * face with its plate, control bases, labels and LEDs in a group of its own. The drum's turn is
   * animated here (`ap.face` jumps, the drum takes DRUM_RATE faces a second to follow); the control
   * parts ride on their face (`drumFace`). Only the faces turned out, or turning, are drawn.
   */
  private buildDrums() {
    const def = this.sim.def;
    const byKey = new Map<string, typeof def.consoles>();
    for (const con of def.consoles) {
      if (!con.drum) continue;
      let list = byKey.get(con.drum.key);
      if (!list) byKey.set(con.drum.key, (list = []));
      list.push(con);
    }
    this.ctlDrum = new Int16Array(def.controls.length).fill(-1);
    for (const [key, cons] of byKey) {
      const c0 = cons[0];
      const faces = c0.drum!.faces;
      const step = (2 * Math.PI) / faces;
      // the prism: its faces are the consoles' plates, its axis a flat's distance behind the face
      const apothem = c0.h / (2 * Math.tan(Math.PI / faces));
      const A = new THREE.Matrix4().makeBasis(new THREE.Vector3(...c0.u), new THREE.Vector3(...c0.v), new THREE.Vector3(...c0.n));
      A.setPosition(c0.c[0] - c0.n[0] * apothem, c0.c[1] - c0.n[1] * apothem, c0.c[2] - c0.n[2] * apothem);
      const Ainv = A.clone().invert();
      const group = new THREE.Group();
      group.name = `drum-${key}`;
      group.matrixAutoUpdate = false;
      // the core: a prism along the axis (drum-local x), a flat facing +z; slightly inside the plates
      const core = new THREE.CylinderGeometry(1, 1, c0.w - 0.01, faces, 1, false, Math.PI / faces);
      core.rotateZ(-Math.PI / 2);
      const rc = (apothem - 0.004) / Math.cos(Math.PI / faces);
      core.scale(1, rc, rc);
      core.applyMatrix4(A);
      const P0 = new Parts();
      P0.add('dark', core);
      // end caps: a flange each side, a little larger, so the turning faces look held
      for (const s of [-1, 1]) {
        const cap = new THREE.CylinderGeometry(rc * 1.12, rc * 1.12, 0.012, faces * 2, 1, false, 0);
        cap.rotateZ(-Math.PI / 2);
        cap.translate(s * (c0.w / 2 + 0.006), 0, 0);
        cap.applyMatrix4(A);
        P0.add('dark', cap);
      }
      for (const [mk, g] of P0.build()) {
        const mesh = new THREE.Mesh(g, this.mats[mk] ?? this.mats.dark);
        mesh.castShadow = false;
        mesh.receiveShadow = true;
        group.add(mesh);
      }
      const faceGroups: THREE.Group[] = [];
      const slot: THREE.Matrix4[] = [];
      for (let k = 0; k < faces; k++) {
        const fg = new THREE.Group();
        fg.matrixAutoUpdate = false;
        // face k sits k steps round the drum
        const M = A.clone().multiply(new THREE.Matrix4().makeRotationX(k * step)).multiply(Ainv);
        fg.matrix.copy(M);
        slot.push(M);
        faceGroups.push(fg);
        group.add(fg);
        const con = cons.find((c) => c.drum!.face === k);
        if (!con) continue;
        const F = frameMatrix(con.c, con.u, con.v, con.n);
        const P = new Parts();
        const labels: THREE.BufferGeometry[] = [];
        const lamps: THREE.BufferGeometry[] = [];
        const label = (text: string, x: number, y: number, h: number) => {
          const r = this.labelAtlas.rects.get(text);
          if (!r) return;
          const [u0, v0, u1, v1, aspect] = r;
          const g = new THREE.PlaneGeometry(h * aspect, h);
          const uv = g.getAttribute('uv') as THREE.BufferAttribute;
          for (let i = 0; i < uv.count; i++) uv.setXY(i, uv.getX(i) ? u1 : u0, uv.getY(i) ? v1 : v0);
          g.applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(x, y, 0.0025)));
          labels.push(g);
        };
        // the face plate
        P.add('console', new THREE.BoxGeometry(con.w, con.h, 0.008).applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(0, 0, -0.004))));
        if (con.h > 0.2) label(con.title, 0, con.h / 2 - 0.028, 0.024);
        for (const c of def.controls) {
          if (c.console !== con.id) continue;
          this.ctlDrum[c.index] = this.drums.length;
          const lx = (c.c[0] - con.c[0]) * con.u[0] + (c.c[1] - con.c[1]) * con.u[1] + (c.c[2] - con.c[2]) * con.u[2];
          const ly = (c.c[0] - con.c[0]) * con.v[0] + (c.c[1] - con.c[1]) * con.v[1] + (c.c[2] - con.c[2]) * con.v[2];
          const base = BASES[c.kind];
          P.add('dark', new THREE.BoxGeometry(base[0], base[1], base[2]).applyMatrix4(F.clone().multiply(new THREE.Matrix4().makeTranslation(lx, ly, base[2] / 2))));
          if (c.label) label(c.label, lx, ly - c.half[1] - 0.022, 0.017);
          if (c.kind === 'master') continue;
          const CF = frameMatrix(c.c, c.u, c.v, c.n);
          const g = strip(new THREE.BoxGeometry(0.03, 0.008, 0.005).applyMatrix4(CF.multiply(new THREE.Matrix4().makeTranslation(0, c.half[1] + 0.012, 0.003))));
          g.setAttribute('aLamp', new THREE.Float32BufferAttribute(new Float32Array(g.getAttribute('position').count).fill(this.lampIds.get(`led:${c.index}`)!), 1));
          lamps.push(g);
        }
        const built = [...P.build().entries()];
        const body = new THREE.Mesh(mergeGeometries(built.map(([, g]) => g), true)!, built.map(([mk]) => this.mats[mk] ?? this.mats.dark));
        body.castShadow = false;
        body.receiveShadow = true;
        fg.add(body);
        if (labels.length) {
          const lm = new THREE.Mesh(mergeGeometries(labels)!, this.labelMaterial());
          lm.renderOrder = 3;
          fg.add(lm);
        }
        if (lamps.length) fg.add(new THREE.Mesh(mergeGeometries(lamps)!, this.lampMat));
        for (const o of fg.children) {
          o.matrixAutoUpdate = false;
          o.updateMatrix();
        }
      }
      this.root.add(group);
      this.interior.push(group);
      const theta = Math.max(0, Math.min(faces - 1, Math.round(this.sim.sw[key] ?? 0)));
      this.drums.push({ key, faces, step, theta, A, Ainv, group, faceGroups, slot, faceM: slot.map(() => new THREE.Matrix4()), dirty: true });
    }
    this.updateDrums(0);
  }

  /** Turn the drums toward their switches; true when one moved (its controls must be redrawn). */
  private updateDrums(dt: number) {
    let moved = false;
    for (const d of this.drums) {
      const target = Math.max(0, Math.min(d.faces - 1, Math.round(this.sim.sw[d.key] ?? 0)));
      // always forward (the way GIRAR turns it), the short way when it jumps several faces
      const ahead = (((target - d.theta) % d.faces) + d.faces) % d.faces;
      if (ahead > 1e-4) {
        d.theta = (d.theta + Math.min(ahead, dt * DRUM_RATE)) % d.faces;
        if (ahead <= dt * DRUM_RATE) d.theta = target;
        d.dirty = true;
      }
      if (!d.dirty) continue;
      d.dirty = false;
      moved = true;
      // the drum turned back by θ steps: face k is then (k − θ) steps from the front
      d.group.matrix.copy(d.A).multiply(_m.makeRotationX(-d.theta * d.step)).multiply(d.Ainv);
      d.group.updateMatrixWorld(true);
      for (let k = 0; k < d.faces; k++) {
        d.faceM[k].multiplyMatrices(d.group.matrix, d.slot[k]);
        let off = Math.abs(k - d.theta) % d.faces;
        off = Math.min(off, d.faces - off);
        d.faceGroups[k].visible = off < 0.999;
      }
    }
    if (moved) {
      for (let i = 0; i < this.ctlDrum.length; i++) if (this.ctlDrum[i] >= 0) this.drawnS[i] = NaN;
    }
    return moved;
  }

  /** A control on a drum can be reached: its face is out and the drum has stopped there. */
  drumReady(c: ControlDef) {
    const i = this.ctlDrum[c.index] ?? -1;
    if (i < 0 || !c.drum) return true;
    return Math.abs(this.drums[i].theta - c.drum.face) < 1e-3;
  }

  private buildControlParts() {
    const def = this.sim.def;
    const counts = { cap: 0, bat: 0, handle: 0, rocker: 0, knob: 0, lid: 0 };
    const kindPart: Record<ControlKind, keyof typeof counts | null> = { button: 'cap', toggle: 'bat', lever: 'handle', breaker: 'rocker', master: null, mushroom: 'cap', rotary: 'knob', cover: 'lid', valve: 'knob', bezel: 'cap' };
    for (const c of def.controls) {
      const k = kindPart[c.kind];
      this.partSlot.push({ kind: k, i: k ? counts[k]++ : -1 });
    }
    const bat = mergeGeometries([strip(new THREE.CylinderGeometry(0.0045, 0.006, 0.042, 8).rotateX(Math.PI / 2).translate(0, 0, 0.021)), strip(new THREE.SphereGeometry(0.009, 10, 8).translate(0, 0, 0.044))])!;
    const handle = mergeGeometries([strip(new THREE.CylinderGeometry(0.007, 0.007, 0.055, 8).rotateX(Math.PI / 2).translate(0, 0, 0.0275)), strip(new THREE.TorusGeometry(0.02, 0.008, 8, 16).translate(0, 0, 0.06))])!;
    const knob = mergeGeometries([strip(new THREE.CylinderGeometry(0.016, 0.018, 0.016, 14).rotateX(Math.PI / 2).translate(0, 0, 0.008)), strip(new THREE.BoxGeometry(0.006, 0.02, 0.006).translate(0, 0.012, 0.012))])!;
    const lid = strip(new THREE.BoxGeometry(0.07, 0.08, 0.006));
    const make = (g: THREE.BufferGeometry, mat: THREE.Material, n: number) => {
      const m = new THREE.InstancedMesh(g, mat, Math.max(1, n));
      m.count = n;
      // levers, caps and knobs are centimetres: no sun shadow worth a pass per cascade
      m.castShadow = false;
      m.receiveShadow = true;
      this.root.add(m);
      return m;
    };
    return {
      cap: make(new THREE.BoxGeometry(1, 1, 1), this.mats.cap, counts.cap),
      bat: make(bat, this.mats.chrome, counts.bat),
      handle: make(handle, this.mats.cap, counts.handle),
      rocker: make(new THREE.BoxGeometry(1, 1, 1), this.mats.rocker, counts.rocker),
      knob: make(knob, this.mats.cap, counts.knob),
      lid: make(lid, this.mats.dark, counts.lid),
    };
  }

  /** Local click feedback (the switch itself moves when the server confirms). */
  pressed(ctl: number) {
    this.press[ctl] = 1;
  }

  // ------------------------------------------------------------------------------------------------
  // Doors, ramp, shutters
  // ------------------------------------------------------------------------------------------------

  private buildDoors() {
    for (const d of this.sim.def.doors) {
      const leaves: THREE.Mesh[] = [];
      for (const side of [-1, 1]) {
        const g = new THREE.BoxGeometry(d.w / 2, d.h, 0.05);
        // the leaf's width runs along the wall, whichever way the door faces
        g.rotateY(Math.atan2(d.n[0], d.n[2]));
        if (side > 0) {
          const uv = g.getAttribute('uv') as THREE.BufferAttribute;
          for (let i = 0; i < uv.count; i++) uv.setX(i, 1 - uv.getX(i));
        }
        const m = new THREE.Mesh(g, this.mats.door);
        m.castShadow = m.receiveShadow = true;
        m.name = `${d.key}-${side}`;
        this.root.add(m);
        leaves.push(m);
      }
      this.doorLeaves.set(d.key, leaves as [THREE.Mesh, THREE.Mesh]);
    }
    // deck hatches: the sliding plate (moved every frame) and a hazard rim round the hole
    for (const h of this.sim.def.hatches) {
      const b = hatchPlate(h, 0);
      const plate = new THREE.Mesh(new THREE.BoxGeometry(b.half[0] * 2, b.half[1] * 2, b.half[2] * 2), this.mats.deck);
      plate.castShadow = plate.receiveShadow = true;
      plate.name = `hatch-${h.key}`;
      this.root.add(plate);
      this.hatchPlates.set(h.key, plate);
      const rim = 0.06;
      for (const [sx, sz, w, l] of [
        [-1, 0, rim, h.l + rim * 2],
        [1, 0, rim, h.l + rim * 2],
        [0, -1, h.w, rim],
        [0, 1, h.w, rim],
      ] as const) {
        const m = new THREE.Mesh(new THREE.BoxGeometry(w, 0.012, l), this.mats.accent);
        m.position.set(h.c[0] + sx * (h.w / 2 + rim / 2), h.c[1] + 0.004, h.c[2] + sz * (h.l / 2 + rim / 2));
        m.receiveShadow = true;
        this.root.add(m);
      }
    }
  }

  private buildRamp() {
    const r = this.sim.def.ramp!;
    // deck-plate walking face, hull paint outside: a pseudo panel through the panel builder
    const pseudo: PanelDef = {
      index: 0,
      id: 'RAMP',
      kind: 'hull',
      zone: 'cargo',
      c: [0, r.length / 2, r.t / 2],
      u: [-1, 0, 0],
      v: [0, 1, 0],
      n: [0, 0, -1],
      poly: [
        [-r.w / 2, -r.length / 2],
        [r.w / 2, -r.length / 2],
        [r.w / 2, r.length / 2],
        [-r.w / 2, r.length / 2],
      ],
      t: r.t,
      maxHp: 1,
      conduits: [],
    };
    const { geos } = buildPanels({ ...this.sim.def, panels: [pseudo] }, () => true, () => 0);
    const faces: Record<string, THREE.Material> = { hull: this.mats.deck, lining: this.mats.paint, edge: this.mats.edge };
    for (const [k, g] of geos) {
      const m = new THREE.Mesh(g, faces[k] ?? this.mats.edge);
      m.castShadow = m.receiveShadow = true;
      this.ramp.add(m);
    }
    // hazard lip at the end and side rails
    const lip = new THREE.Mesh(new THREE.BoxGeometry(r.w, 0.06, r.t + 0.02), this.mats.accent);
    lip.position.set(0, r.length - 0.03, r.t / 2);
    this.ramp.add(lip);
    for (const sx of [-1, 1]) {
      const rail = new THREE.Mesh(new THREE.BoxGeometry(0.06, r.length, 0.08), this.mats.dark);
      rail.position.set(sx * (r.w / 2 - 0.03), r.length / 2, -0.03);
      rail.castShadow = true;
      this.ramp.add(rail);
    }
    this.ramp.position.set(...r.hinge);
    this.ramp.name = 'ramp';
    this.root.add(this.ramp);
  }

  /** Hydraulic rams between the hull and the ramp (anchors from the ramp's data). */
  private buildPistons() {
    const list = this.sim.def.ramp!.pistons ?? [];
    if (!list.length) return null;
    const a: V3[] = list.map((p) => p.hull);
    const b: V3[] = list.map((p) => p.ramp);
    const barrel = new THREE.InstancedMesh(new THREE.CylinderGeometry(0.05, 0.05, 1, 12).translate(0, 0.5, 0), this.mats.dark, list.length);
    const rod = new THREE.InstancedMesh(new THREE.CylinderGeometry(0.026, 0.026, 1, 10).translate(0, 0.5, 0), this.mats.chrome, list.length);
    for (const m of [barrel, rod]) {
      m.castShadow = true;
      m.frustumCulled = false;
      this.root.add(m);
    }
    return { barrel, rod, a, b };
  }

  /** Current ramp rotation about its hinge: 0 = closed (vertical), 1 = resting on the ground. */
  rampPhi(open: number) {
    return open * (Math.PI / 2 + this.sim.rampAngle);
  }

  // ------------------------------------------------------------------------------------------------
  // Per frame
  // ------------------------------------------------------------------------------------------------

  /**
   * Per frame: `pose` = the ship as drawn this frame, `feet` = the gear feet (ship space), `eye` =
   * the camera in render space (like the matrices it is compared with, render/origin.ts): far
   * outside, the ship draws and updates no interior. Only what changed is
   * written; nothing is allocated.
   */
  update(dt: number, time: number, anim: ShipAnimState, pose: ShipPose, feet: V3[], eye?: THREE.Vector3) {
    const sim = this.sim;
    const def = sim.def;
    const sw = sim.sw;
    const lp = this.lastPose;
    const p = pose.p;
    const q = pose.q;
    if (p[0] !== lp[0] || p[1] !== lp[1] || p[2] !== lp[2] || q[0] !== lp[3] || q[1] !== lp[4] || q[2] !== lp[5] || q[3] !== lp[6]) {
      lp[0] = p[0];
      lp[1] = p[1];
      lp[2] = p[2];
      lp[3] = q[0];
      lp[4] = q[1];
      lp[5] = q[2];
      lp[6] = q[3];
      this.root.position.set(p[0], p[1], p[2]);
      this.root.quaternion.set(q[0], q[1], q[2], q[3]);
      this.root.updateMatrix();
      this.root.updateMatrixWorld(true);
    }
    // interior LOD: far outside the ship, none of the inside is drawn or animated; further still,
    // not the small exterior details either
    const dist = eye ? _v.copy(this.bounds.center).applyMatrix4(this.root.matrixWorld).distanceTo(eye) : 0;
    const near = dist < this.bounds.radius + INTERIOR_REACH;
    if (near !== this.near) {
      this.near = near;
      for (const o of this.interior) o.visible = near;
    }
    const detailed = dist < this.bounds.radius + DETAIL_REACH;
    if (detailed !== this.detailed) {
      this.detailed = detailed;
      for (const o of this.detail) o.visible = detailed;
      // the shutters show themselves again only when they are down
      if (detailed) this.shield.visible = this.lastShield > 0.005;
    }
    this.gear.update(feet);
    this.exhaust.update(dt, anim);
    if (sim.version !== this.consolesVer) {
      this.consolesVer = sim.version;
      this.rebuildConsoles();
    }

    // heat glow decays
    for (let i = 0; i < this.heat.length; i++) {
      if (this.heat[i] <= 0) continue;
      this.heat[i] = Math.max(0, this.heat[i] - dt * 0.45);
      this.writePanel(i);
    }

    // doors: the two leaves slide apart along the wall
    for (let k = 0; k < def.doors.length; k++) {
      const d = def.doors[k];
      const open = anim.movers[d.key] ?? 0;
      if (open === this.doorOpen[k]) continue;
      this.doorOpen[k] = open;
      const leaves = this.doorLeaves.get(d.key)!;
      const a = this.doorAxes[k];
      for (let j = 0; j < 2; j++) {
        const s = (j === 0 ? -1 : 1) * (d.w / 4 + open * (d.w / 2 - 0.03));
        leaves[j].position.set(d.c[0] + a[0] * s + d.n[0] * d.offset, d.c[1] + d.h / 2, d.c[2] + a[2] * s + d.n[2] * d.offset);
      }
    }
    for (let k = 0; k < def.hatches.length; k++) {
      const h = def.hatches[k];
      const open = anim.movers[h.key] ?? 0;
      if (open === this.hatchOpen[k]) continue;
      this.hatchOpen[k] = open;
      const b = hatchPlate(h, open);
      this.hatchPlates.get(h.key)!.position.set(b.c[0], b.c[1], b.c[2]);
    }
    // ramp + pistons
    const rampOpen = def.ramp ? anim.movers[def.ramp.key] ?? 0 : 0;
    if (def.ramp && rampOpen !== this.lastRamp) {
      this.lastRamp = rampOpen;
      this.ramp.rotation.x = this.rampPhi(rampOpen);
      this.ramp.updateMatrix();
    }
    if (this.pistons && this.pistonsAt !== rampOpen) {
      const pistons = this.pistons;
      this.pistonsAt = rampOpen;
      for (let i = 0; i < pistons.a.length; i++) {
        const a = pistons.a[i];
        const b = pistons.b[i];
        const A = _v.set(a[0], a[1], a[2]);
        const dir = _v2.set(b[0], b[1], b[2]).applyMatrix4(this.ramp.matrix).sub(A);
        const L = dir.length();
        _q.setFromUnitVectors(_up, dir.normalize());
        pistons.barrel.setMatrixAt(i, _m.compose(A, _q, _s.set(1, Math.min(1.0, L * 0.55), 1)));
        pistons.rod.setMatrixAt(i, _m.compose(_v3.copy(A).addScaledVector(dir, L * 0.2), _q, _s.set(1, L * 0.8, 1)));
      }
      pistons.barrel.instanceMatrix.needsUpdate = true;
      pistons.rod.instanceMatrix.needsUpdate = true;
    }
    // shutters roll down from their housings
    const shield = def.shield ? anim.movers[def.shield.key] ?? 0 : 0;
    if (def.shield && shield !== this.lastShield) {
      this.lastShield = shield;
      const plates = def.shield.plates;
      for (let i = 0; i < plates.length; i++) {
        const pl = plates[i];
        const h = Math.max(0.001, pl.h * shield);
        _m.copy(this.shieldFrames[i]).multiply(_L.compose(_v.set(0, pl.h / 2 - h / 2, 0), _qI, _s.set(pl.w, h, 0.035)));
        this.shield.setMatrixAt(i, _m);
      }
      this.shield.instanceMatrix.needsUpdate = true;
      this.shield.visible = shield > 0.005 && this.detailed;
    }

    // drums turning to the face asked for (their controls are redrawn on the way)
    if (this.drums.length) this.updateDrums(dt);
    // controls: smooth throws, click feedback (a part is rewritten only when its pose changed)
    const controls = def.controls;
    for (let k = 0; k < controls.length; k++) {
      const c = controls[k];
      const ci = c.index;
      const target = sw[c.key] ?? 0;
      let s = this.shown[ci];
      if (s !== target) {
        s += (target - s) * Math.min(1, dt * 14);
        if (Math.abs(target - s) < 1e-4) s = target;
        this.shown[ci] = s;
      }
      if (this.press[ci] > 0) this.press[ci] = Math.max(0, this.press[ci] - dt * 7);
      const slot = this.partSlot[ci];
      if (!slot.kind || !near) continue;
      const hidden = this.hostHidden(c.host) ? 1 : 0;
      const press = this.press[ci];
      if (s === this.drawnS[ci] && press === this.drawnP[ci] && hidden === this.drawnH[ci]) continue;
      this.drawnS[ci] = s;
      this.drawnP[ci] = press;
      this.drawnH[ci] = hidden;
      switch (slot.kind) {
        case 'cap':
          _L.compose(_v.set(0, 0, 0.019 - press * 0.007), _qI, _s.set(c.kind === 'bezel' ? 0.042 : 0.046, c.kind === 'bezel' ? 0.016 : 0.046, 0.016));
          break;
        case 'bat':
          _L.makeTranslation(0, 0, 0.012).multiply(_R.makeRotationX(-0.5 + (1 - s) * 1.0));
          break;
        case 'handle':
          _L.makeTranslation(0, (s - 0.5) * 0.11, 0.012);
          break;
        case 'knob': {
          const span = Math.max(1, c.states.length - 1);
          _L.makeRotationZ((s / span) * Math.PI * 1.35).multiply(_T.makeTranslation(0, 0, 0.012));
          break;
        }
        case 'lid':
          // hinge along the top edge: shut flat, open it flips up and off the button
          _L.makeTranslation(0, 0.04, 0).multiply(_R.makeRotationX(-s * 1.65)).multiply(_T.makeTranslation(0, -0.04, 0.012));
          break;
        default:
          _L.makeTranslation(0, 0, 0.02).multiply(_R.makeRotationX(s > 0.5 ? -0.24 : 0.24)).multiply(_T.makeScale(0.036, 0.05, 0.012));
      }
      if (hidden) _L.makeScale(0, 0, 0);
      const mesh = this.parts[slot.kind];
      const di = this.ctlDrum[ci];
      if (di >= 0) {
        // on a drum face: carried round with it, and not drawn while its face is inside
        const d = this.drums[di];
        const face = c.drum!.face;
        if (!d.faceGroups[face].visible) _L.makeScale(0, 0, 0);
        mesh.setMatrixAt(slot.i, _m.multiplyMatrices(d.faceM[face], this.ctlFrame[ci]).multiply(_L));
      } else mesh.setMatrixAt(slot.i, _m.multiplyMatrices(this.ctlFrame[ci], _L));
      mesh.instanceMatrix.needsUpdate = true;
    }

    // refusals follow the state (~20 Hz), not the frame rate; only worth it with the crew close
    if (near && (sim.version !== this.blockedVer || time - this.blockedAt > 0.05)) {
      this.blockedVer = sim.version;
      this.blockedAt = time;
      this.refused.clear();
      for (let k = 0; k < controls.length; k++) {
        const c = controls[k];
        if (c.host >= 0 && sim.hole(c.host)) {
          this.blocks.live(c.index, null, time);
          continue;
        }
        if (this.blocks.live(c.index, sim.blocked(c), time)) this.refused.add(c.index);
      }
    }

    // ---- lamps --------------------------------------------------------------------------------------
    const L = this.lampMat;
    const ids = this.lamp;
    const lk = def.lighting;
    const lightsOn = sim.powered(lk.cabin);
    const ext = sim.powered(lk.exterior);
    for (let zi = 0; zi < def.zones.length; zi++) {
      const on = lightsOn && sw[def.zones[zi].lightKey] === 1;
      L.set(this.zoneLamp[zi], on ? 7 : 0.02, on ? 6.6 : 0.02, on ? 6 : 0.02);
    }
    const emergency = !lightsOn;
    L.set(ids.emergency, emergency ? (blink(time, 0.5, 0.85) ? 6 : 2.5) : 0.04, emergency ? 0.25 : 0.01, emergency ? 0.1 : 0.01);
    const nav = ext && sw[lk.nav] === 1;
    L.set(ids.navRed, nav ? 8 : 0.05, nav ? 0.3 : 0, 0);
    L.set(ids.navGreen, 0, nav ? 7 : 0.05, nav ? 1.2 : 0);
    const strobe = nav && (time * 0.8) % 1 < 0.06;
    L.set(ids.strobe, strobe ? 40 : 0.1, strobe ? 40 : 0.1, strobe ? 40 : 0.1);
    const beacon = ext && sw[lk.beacon] === 1 ? Math.max(0, Math.sin(time * Math.PI * 1.2)) ** 8 : 0;
    L.set(ids.beacon, 0.1 + beacon * 25, beacon * 1.2, 0);
    const landing = ext && sw[lk.landing] === 1;
    L.set(ids.landing, landing ? 30 : 0.1, landing ? 29 : 0.1, landing ? 26 : 0.1);
    if (landing) {
      // the light pool takes world positions (the matrices are in render space)
      origin.toWorld(_v.copy(this.landing.pos).applyMatrix4(this.root.matrixWorld));
      _v2.copy(this.landing.dir).transformDirection(this.root.matrixWorld);
      lights.spot(_v, _v2, 0xfff4e6, 900, 60, LANDING_ANGLE, 0.5, 1.4);
    }
    const flick = 1 + Math.sin(time * 9.1) * 0.05 + Math.sin(time * 23.7) * 0.03;
    // engines are cold on the pad: a faint warm-up glow with a reactor online, brighter with thrust
    let thrust = 0;
    for (const e of this.engineMods) thrust = Math.max(thrust, e.thrust(sim.st));
    let anyOnline = false;
    for (const r of this.reactorMods) if (r.state(sim.st) === RX.online) anyOnline = true;
    const warm = (anyOnline ? 0.006 : 0) + thrust * 2;
    L.set(ids.nozzle, warm * flick, warm * 2 * flick, warm * 6.6 * flick);
    const caution = sw[def.caution] === 1 && blink(time, 2);
    L.set(ids.caution, caution ? 9 : 0.15, caution ? 5 : 0.08, 0);
    for (let k = 0; k < this.machines.length; k++) {
      const m = this.machines[k];
      const ms = this.machineState[k];
      if (m.animate && m.mover) {
        const v = anim.movers[m.mover] ?? 0;
        if (v !== ms.mover) {
          ms.mover = v;
          m.animate(v);
        }
      }
      if (!m.mats.length && !m.tex) continue;
      const ratio = sim.partHp(m.part) / m.part.maxHp;
      if (ratio === ms.ratio && this.welder === ms.welder) continue;
      ms.ratio = ratio;
      ms.welder = this.welder;
      if (m.tex) {
        this.partTint(m.part.index, this.welder, ratio);
        continue;
      }
      for (const x of m.mats) {
        if (!this.welder) {
          x.mat.color.copy(x.base);
          x.mat.emissive.setRGB(0, 0, 0);
          continue;
        }
        x.mat.color.setRGB(0.18 + 0.16 * ratio, 0.08 + 0.22 * ratio, 0.05 + 0.04 * ratio);
        x.mat.emissive.setRGB(ratio < 0.35 ? 0.35 * (1 - ratio) : 0, 0.02, 0);
      }
    }
    // far outside: the rest is interior (indicators, annunciator, seats, LEDs, cabin lights, screens)
    if (!near) return;
    const grid = sim.sys.power;
    const lit = !grid || sim.st[grid.iLive] === 1;
    for (let i = 0; i < def.indicators.length; i++) {
      const ind = def.indicators[i];
      const li = this.indLamp[i];
      if (ind.kind === 'reactor-core') {
        const r = this.indReactor[i];
        const on = !!r && r.state(sim.st) === RX.online;
        L.set(li[0], on ? 1.5 * flick : 0.02, on ? 4 * flick : 0.02, on ? 7 * flick : 0.03);
      } else if (ind.kind === 'gear-greens') {
        const down = lit && sim.mover(ind.ref ?? def.gear?.key ?? '') > 0.99;
        L.set(li[0], 0, down ? 5 : 0.03, down ? 1 : 0);
      } else if (ind.kind === 'airlock') {
        const ph = this.lockPhase >= 0 ? sim.st[this.lockPhase] : LOCK.in;
        const cyc = ph !== LOCK.in && ph !== LOCK.out;
        const b2 = blink(time, 2);
        L.set(li[0], 0, lit && ph === LOCK.in ? 5 : 0.03, lit && ph === LOCK.in ? 1 : 0);
        L.set(li[1], lit && cyc && b2 ? 5 : 0.05, lit && cyc && b2 ? 2.6 : 0.03, 0);
        L.set(li[2], lit && ph === LOCK.out ? 6 : 0.05, lit && ph === LOCK.out ? 0.3 : 0.01, 0);
      }
    }
    for (let i = 0; i < this.annLamp.length; i++) {
      const a = this.annAlerts[i];
      let any = false;
      let hot = false;
      for (let k = 0; k < a.vars.length; k++) {
        if (sim.st[a.vars[k]] !== 1) continue;
        any = true;
        if (a.hot[k]) hot = true;
      }
      const on = any && (!hot || blink(time, 2));
      L.set(this.annLamp[i], on ? (hot ? 7 : 5.5) : 0.12, on ? (hot ? 0.35 : 3.4) : 0.06, on ? 0.05 : 0.02);
    }
    // seat umbilical port: green while a pack is docked
    for (let i = 0; i < this.seatLamp.length; i++) {
      const docked = this.seatOccupied[i];
      L.set(this.seatLamp[i], docked ? 0 : 0.6, docked ? 4 : 0.35, docked ? 1 : 0);
    }
    for (let k = 0; k < controls.length; k++) this.setLed(controls[k], time, anim, this.refused.has(controls[k].index));

    // cabin lights (shader lights; emergency = dim red)
    const M = this.root.matrixWorld;
    for (const ls of this.lightSlots) {
      const zn = def.zones[ls.zone];
      const on = lightsOn && sw[zn.lightKey] === 1;
      const intensity = on ? zn.lux ?? 4.2 : emergency ? (blink(time, 0.5, 0.85) ? 0.9 : 0.35) : 0;
      if (intensity <= 0) continue;
      const zb = this.zoneBox[ls.zone];
      _center.copy(zb.center).applyMatrix4(M);
      if (on) _col.setRGB(1, 0.93, 0.84);
      else _col.setRGB(1, 0.06, 0.03);
      setInteriorLight(_v.copy(ls.pos).applyMatrix4(M), _col, intensity, _center, this.root.quaternion, zb.half);
    }

    this.screens.update(time, anim, this.hostHidden, eye);
  }

  private reactors() {
    return this.sim.sys.modules.filter((m): m is Reactor => m.id.startsWith('reactor:'));
  }

  /**
   * Portal culling (ShipClient.portalView): draw only the furniture and machines of the rooms the
   * camera can see into. `null` = every room again (as the distance LOD says).
   */
  showRooms(rooms: ReadonlySet<number> | null) {
    if (!rooms && !this.roomsCulled) return;
    this.roomsCulled = !!rooms;
    for (const [room, list] of this.roomMeshes) {
      const on = this.near && (!rooms || rooms.has(room));
      for (const o of list) o.visible = on;
    }
  }

  /**
   * A machine material made from scratch (a `clone()` loses the cascaded-shadow and cabin-light
   * shader hooks): body, trim or the maker's colour.
   */
  private machineMaterial(slot: 'body' | 'trim' | 'accent', color?: THREE.Color) {
    const look = MACHINE_LOOK[slot];
    const c = color ?? (slot === 'accent' ? lin(...this.sim.def.livery.accent) : lin(...look.color));
    return patchInteriorLights(litMaterial(this.csm, { color: c, roughness: look.roughness, metalness: look.metalness, envMapIntensity: look.env }));
  }

  /** A merged machine's maker colour (texel 0 of its three). */
  private partAccent(index: number, c: THREE.Color) {
    const o = (index + 1) * 3 * 4;
    this.partData[o] = c.r;
    this.partData[o + 1] = c.g;
    this.partData[o + 2] = c.b;
  }

  /** A merged machine's welder tint and glow (texels 1 and 2), from its integrity. */
  private partTint(index: number, welder: boolean, ratio: number) {
    const d = this.partData;
    const o = ((index + 1) * 3 + 1) * 4;
    if (welder) {
      d[o] = 0.18 + 0.16 * ratio;
      d[o + 1] = 0.08 + 0.22 * ratio;
      d[o + 2] = 0.05 + 0.04 * ratio;
      d[o + 3] = 1;
      d[o + 4] = ratio < 0.35 ? 0.35 * (1 - ratio) : 0;
      d[o + 5] = 0.02;
      d[o + 6] = 0;
    } else {
      d[o + 3] = 0;
      d[o + 4] = d[o + 5] = d[o + 6] = 0;
    }
    this.partTex.needsUpdate = true;
  }

  /** Welder in hand: machines show how damaged they are. */
  setWelder(on: boolean) {
    this.welder = on;
  }

  /** Mark a control from the manual (null = none). */
  point(index: number | null) {
    this.pointed = index ?? -1;
  }

  /** The authority refused this control: light it now, don't wait out the settle. */
  refuse(index: number) {
    this.blocks.pin(index);
    this.refused.add(index);
  }

  isRefused(index: number) {
    return this.refused.has(index);
  }

  private setLed(c: ControlDef, time: number, anim: ShipAnimState, refused: boolean) {
    const sim = this.sim;
    const i = this.ledSlot[c.index];
    if (i < 0) return;
    const L = this.lampMat;
    const vis = L.isVisible(i);
    if (refused) return L.set(i, 5.2, 2.6, 0.2, vis);
    if (c.index === this.pointed) return (time * 2.5) % 1 < 0.55 ? L.set(i, 0, 4, 6, vis) : L.set(i, 0, 0.3, 0.5, vis);
    const v = sim.sw[c.key] ?? 0;
    if (c.kind === 'master') return L.set(i, 0, 0, 0, vis);
    if (c.kind === 'bezel') return v === (c.value ?? 0) ? L.set(i, 0, 2.4, 4, vis) : L.set(i, 0.04, 0.08, 0.1, vis);
    if (c.kind === 'cover') return v ? L.set(i, 0.5, 0.35, 0.05, vis) : L.set(i, 0.04, 0.04, 0.04, vis);
    if (c.kind === 'breaker') return v ? L.set(i, 0, 4, 0.8, vis) : L.set(i, 5, 0.3, 0, vis);
    if (this.runKeys.has(c.key)) return v ? L.set(i, 0, 4, 0.8, vis) : L.set(i, 5, 0.3, 0, vis);
    // anything that travels (doors, ramp, gear, shutters, radiators): amber blink while on its way
    const moving = anim.movers[c.key];
    if (moving !== undefined) {
      const travelling = Math.abs(moving - v) > 0.01;
      if (travelling) return (time * 3) % 1 < 0.5 ? L.set(i, 5, 2.6, 0, vis) : L.set(i, 0.1, 0.05, 0, vis);
      const bus = c.requires ?? this.effect.get(c.key);
      if (bus && !sim.powered(bus)) return L.set(i, 0.6, 0.02, 0, vis);
      return v ? L.set(i, 0, 4, 0.8, vis) : L.set(i, 0.05, 0.05, 0.05, vis);
    }
    const eff = this.effect.get(c.key);
    if (!v) return L.set(i, 0.04, 0.04, 0.04, vis);
    return !eff || sim.powered(eff) ? L.set(i, 0, 4, 0.8, vis) : L.set(i, 5, 2.6, 0, vis);
  }
}

/** Body, trim and maker's-colour looks of machines (the maker's colour itself comes per part). */
const MACHINE_LOOK = {
  body: { color: [0.24, 0.25, 0.26] as [number, number, number], roughness: 0.55, metalness: 0.35, env: 0.4 },
  trim: { color: [0.07, 0.072, 0.075] as [number, number, number], roughness: 0.55, metalness: 0.6, env: 0.5 },
  accent: { color: [0.5, 0.5, 0.5] as [number, number, number], roughness: 0.55, metalness: 0.1, env: 1 },
};

/** Parts per row of `partTex` (3 texels each). */
const PART_TEX_W = 256;

/**
 * Per-part colour on a material shared by many merged machines: `aPart` (part index + 1) picks the
 * part's texels in `tex` — the maker's colour (`accent` materials), the welder's damage tint when
 * on, and its glow. A clone per machine used to do this (and a draw call per machine and material).
 */
function patchParts(mat: THREE.MeshStandardMaterial, tex: THREE.DataTexture, accent: boolean) {
  mat.userData.parts = true;
  const prev = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    prev?.call(mat, shader, renderer);
    shader.uniforms.uPartTex = { value: tex };
    shader.uniforms.uPartAccent = { value: accent ? 1 : 0 };
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nattribute float aPart;\nvarying float vPart;')
      .replace('#include <begin_vertex>', '#include <begin_vertex>\nvPart = aPart;');
    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <common>',
        `#include <common>
        uniform highp sampler2D uPartTex;
        uniform float uPartAccent;
        varying float vPart;
        vec4 partTexel(int i) { return texelFetch(uPartTex, ivec2(i % ${PART_TEX_W}, i / ${PART_TEX_W}), 0); }`,
      )
      .replace(
        '#include <color_fragment>',
        `#include <color_fragment>
        {
          int pb = int(vPart + 0.5) * 3;
          if (pb > 0) {
            if (uPartAccent > 0.5) diffuseColor.rgb = partTexel(pb).rgb;
            vec4 tint = partTexel(pb + 1);
            if (tint.a > 0.5) diffuseColor.rgb = tint.rgb;
          }
        }`,
      )
      .replace(
        '#include <emissivemap_fragment>',
        `#include <emissivemap_fragment>
        {
          int pb = int(vPart + 0.5) * 3;
          if (pb > 0) totalEmissiveRadiance += partTexel(pb + 2).rgb;
        }`,
      );
  };
  const key = mat.customProgramCacheKey.bind(mat);
  mat.customProgramCacheKey = () => `${key()}+parts`;
  return mat;
}

/**
 * Annunciator rows: pitch (× the cell height) and lamp height (× the cell height). The lamp is
 * short and the rows spread out so each name reads between its lamp and the next row's.
 */
const ANN_ROW = 1.3;
const ANN_LAMP = 0.5;

/** Stop recomputing an object's local matrix every frame (it never moves in its parent). */
function freeze(o: THREE.Object3D) {
  // objects that already keep their own matrix (screens) are left alone
  if (!o.matrixAutoUpdate) return;
  o.updateMatrix();
  o.matrixAutoUpdate = false;
}

/** Static base under each control kind (w, h, depth). */
const BASES: Record<ControlKind, V3> = {
  button: [0.064, 0.064, 0.012],
  toggle: [0.036, 0.06, 0.01],
  lever: [0.05, 0.18, 0.012],
  breaker: [0.052, 0.08, 0.016],
  master: [0.1, 0.07, 0.014],
  mushroom: [0.07, 0.07, 0.012],
  rotary: [0.05, 0.05, 0.01],
  cover: [0.07, 0.09, 0.01],
  valve: [0.08, 0.08, 0.01],
  bezel: [0.05, 0.022, 0.008],
};

/** Label atlas: every console title / control label rendered once. */
function buildLabelAtlas(texts: string[]) {
  const unique = [...new Set(texts)];
  const canvas = document.createElement('canvas');
  const W = 1024;
  const H = 48;
  const ctx = canvas.getContext('2d')!;
  const font = '700 30px ui-monospace, Menlo, Consolas, monospace';
  ctx.font = font;
  const rows: Array<{ text: string; x: number; y: number; w: number }> = [];
  let x = 0;
  let y = 0;
  for (const t of unique) {
    const w = Math.ceil(ctx.measureText(t).width) + 12;
    if (x + w > W) {
      x = 0;
      y += H;
    }
    rows.push({ text: t, x, y, w });
    x += w;
  }
  canvas.width = W;
  canvas.height = THREE.MathUtils.ceilPowerOfTwo(y + H);
  ctx.font = font;
  ctx.fillStyle = '#fff';
  ctx.textBaseline = 'middle';
  const rects = new Map<string, [number, number, number, number, number]>();
  for (const r of rows) {
    ctx.fillText(r.text, r.x + 6, r.y + H / 2 + 1);
    rects.set(r.text, [r.x / W, 1 - (r.y + H) / canvas.height, (r.x + r.w) / W, 1 - r.y / canvas.height, r.w / H]);
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 8;
  return { tex, rects };
}

function doorTexture() {
  const c = document.createElement('canvas');
  c.width = 256;
  c.height = 512;
  const g = c.getContext('2d')!;
  g.fillStyle = '#5d6266';
  g.fillRect(0, 0, 256, 512);
  g.fillStyle = '#4a4e52';
  g.fillRect(0, 0, 256, 40);
  g.fillRect(0, 472, 256, 40);
  // hazard band on the meeting edge (right side of the texture)
  g.save();
  g.beginPath();
  g.rect(204, 0, 52, 512);
  g.clip();
  g.fillStyle = '#d8a318';
  g.fillRect(204, 0, 52, 512);
  g.fillStyle = '#151515';
  for (let y = -60; y < 560; y += 44) {
    g.beginPath();
    g.moveTo(204, y);
    g.lineTo(256, y - 30);
    g.lineTo(256, y - 8);
    g.lineTo(204, y + 22);
    g.fill();
  }
  g.restore();
  // window slot + seams
  g.fillStyle = '#101418';
  g.fillRect(70, 110, 90, 150);
  g.strokeStyle = '#2d3134';
  g.lineWidth = 6;
  g.strokeRect(66, 106, 98, 158);
  g.lineWidth = 3;
  for (const y of [300, 380]) {
    g.beginPath();
    g.moveTo(20, y);
    g.lineTo(190, y);
    g.stroke();
  }
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 4;
  return tex;
}

function registryDecal(name: string, reg: string, tagline: string, accent: [number, number, number], csm: CSM | null) {
  const c = document.createElement('canvas');
  c.width = 1024;
  c.height = 256;
  const g = c.getContext('2d')!;
  g.clearRect(0, 0, 1024, 256);
  g.fillStyle = 'rgba(20,24,30,0.92)';
  g.font = '800 120px ui-sans-serif, Helvetica, Arial, sans-serif';
  g.textBaseline = 'middle';
  g.fillText(reg, 40, 118);
  g.font = '700 54px ui-sans-serif, Helvetica, Arial, sans-serif';
  g.fillText(name.toUpperCase(), 560, 96);
  // the accent colour, linear → sRGB for the canvas
  const srgb = accent.map((c) => Math.round(255 * (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055)));
  g.fillStyle = `rgb(${srgb.join(',')})`;
  g.fillRect(560, 140, 420, 16);
  g.fillStyle = 'rgba(20,24,30,0.92)';
  g.font = '600 30px ui-sans-serif, Helvetica, Arial, sans-serif';
  g.fillText(tagline, 560, 196);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 8;
  return sharpText(litMaterial(csm, { map: tex, transparent: true, roughness: 0.6, metalness: 0.1, polygonOffset: true, polygonOffsetFactor: -2 }), -0.5);
}
