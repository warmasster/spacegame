import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import { doorAxis, partKey, SEAT_BOXES, nacellePylons, type ControlDef, type ControlKind, type PanelDef, type PartDef, type SeatBox, type SeatDef, type SubsystemId } from '../../shared/ship/def';
import { maker } from '../../shared/ship/catalog/makers';
import { LOCK } from '../../shared/ship/modules/airlock';
import { RX, type Reactor } from '../../shared/ship/modules/reactor';
import type { Engine } from '../../shared/ship/modules/engines';
import type { V2, V3 } from '../../shared/ship/geom';
import type { ShipSim } from '../../shared/ship/sim';
import { buildFrames, buildPanels, Parts, strip, type PanelMatKey, type PanelRange } from './geometry';
import { allocInteriorLight, patchInteriorLights, setInteriorLight } from './interiorLights';
import { glassMaterial, LampMaterial, litMaterial, panelMaterial } from './materials';
import { machineModel, propModel, type Slot } from './models';
import { ReasonHold } from '../../shared/ship/hold';
import { ShipScreens, type ShipAnimState } from './screens';

const lin = (r: number, g: number, b: number) => new THREE.Color().setRGB(r, g, b, THREE.LinearSRGBColorSpace);

const frameMatrix = (c: V3, u: V3, v: V3, n: V3) => new THREE.Matrix4().makeBasis(new THREE.Vector3(...u), new THREE.Vector3(...v), new THREE.Vector3(...n)).setPosition(...c);

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
  private hiddenHosts = '';
  private doorLeaves = new Map<string, [THREE.Mesh, THREE.Mesh]>();
  private ramp = new THREE.Group();
  private pistons: { barrel: THREE.InstancedMesh; rod: THREE.InstancedMesh; a: V3[]; b: V3[] } | null = null;
  /** Circuit that makes a switch actually do something (its LED goes amber when that circuit is dead). */
  private effect = new Map<string, SubsystemId>();
  /** Reactor run switches (their LED is green/red like a breaker). */
  private runKeys = new Set<string>();
  private shield: THREE.InstancedMesh;
  private parts: Record<'cap' | 'bat' | 'handle' | 'rocker' | 'knob' | 'lid', THREE.InstancedMesh>;
  private partSlot: Array<{ kind: 'cap' | 'bat' | 'handle' | 'rocker' | 'knob' | 'lid' | null; i: number }> = [];
  /** Per-machine materials (welder tint) and the machine's deployment animation. */
  private machines: Array<{ part: PartDef; mats: Array<{ mat: THREE.MeshStandardMaterial; base: THREE.Color }>; animate: ((t: number) => void) | null; mover: string | null }> = [];
  /** Machine integrity tint, only while the welder is in hand. */
  private welder = false;
  private shown: Float32Array;
  private press: Float32Array;
  private lightSlots: Array<{ slot: number; zone: number; pos: THREE.Vector3 }> = [];
  private landing: THREE.SpotLight;
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

  constructor(
    private sim: ShipSim,
    private csm: CSM | null,
    /** Ground height in world space (for gear feet). */
    ground: (x: number, z: number) => number,
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
    const frames = new THREE.Mesh(buildFrames(def), this.mats.frame);
    frames.castShadow = frames.receiveShadow = true;
    frames.name = 'frames';
    this.root.add(frames);
    this.buildDecor(ground);
    this.rebuildConsoles();
    this.parts = this.buildControlParts();
    this.screens = new ShipScreens(sim);
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

    // cabin light slots (shader lights, see interiorLights.ts)
    def.zones.forEach((z, zi) => {
      for (const p of z.lights) this.lightSlots.push({ slot: allocInteriorLight(), zone: zi, pos: new THREE.Vector3(...p) });
    });
    // landing floodlight: the one real light the ship adds (it has to light the terrain)
    const ll = def.extLights.filter((l) => l.kind === 'landing');
    const lp = ll.reduce((a, l) => a.add(new THREE.Vector3(...l.pos)), new THREE.Vector3()).multiplyScalar(1 / Math.max(1, ll.length));
    const ld = ll.reduce((a, l) => a.add(new THREE.Vector3(...l.dir!)), new THREE.Vector3()).normalize();
    this.landing = new THREE.SpotLight(0xfff4e6, 0, 60, THREE.MathUtils.degToRad(34), 0.5, 1.4);
    this.landing.position.copy(lp);
    this.landing.target.position.copy(lp.clone().addScaledVector(ld, 10));
    this.root.add(this.landing, this.landing.target);
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

  private buildDecor(ground: (x: number, z: number) => number) {
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

    // belly tubs: keel structure under the deck, open on top (conduits run in there)
    for (const m of def.modules) {
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

    // landing gear: struts reach the actual terrain under each foot
    const inv = new THREE.Matrix4().copy(this.root.matrixWorld).invert();
    for (const leg of def.gear?.legs ?? []) {
      const w = new THREE.Vector3(...leg).applyMatrix4(this.root.matrixWorld);
      const footY = new THREE.Vector3(w.x, ground(w.x, w.z), w.z).applyMatrix4(inv).y;
      const [x, , z] = leg;
      const mid = (-0.45 + footY) / 2;
      P.rod('dark', [x, -0.4, z], [x, mid, z], 0.11, 14);
      P.rod('chrome', [x, mid + 0.1, z], [x, footY + 0.14, z], 0.075, 14);
      P.rod('dark', [x, -0.42, z - Math.sign(z) * 0.9], [x, mid, z], 0.05, 8);
      P.lathe('dark', [[0, 0], [0.34, 0], [0.36, 0.04], [0.3, 0.1], [0.12, 0.13], [0, 0.13]], [x, footY + 0.005, z], undefined, 20);
      P.box('paint', 0.03, 0.34, 0.9, [x - 0.3, -0.62, z]);
      P.box('paint', 0.03, 0.34, 0.9, [x + 0.3, -0.62, z]);
    }

    // ---- interior -----------------------------------------------------------------------------------
    // seats; furniture (dash supports, bunks, galley, handrails…) is props, drawn with the machines
    for (const st of def.seats) this.buildSeat(P, st, lamp);

    // conduits: cable runs of every subsystem (seen on the ceiling, under the deck through holes)
    for (const s of def.subsystems) for (const r of s.routes) for (let i = 0; i < r.length - 1; i++) P.rod('pipe', r[i], r[i + 1], 0.03, 8);

    // cabin light strips, emergency lights on the ribs near the deck
    // strips along both edges of the flat roof of each zone's module (the nose module is shorter: windshield)
    def.zones.forEach((zn) => {
      const mi = def.modules.findIndex((m) => m.zone === zn.id);
      const mod = def.modules[mi];
      const top = mod ? Math.max(...mod.profile.map((q) => q[1])) : zn.max[1];
      const flat = mod ? Math.max(...mod.profile.filter((q) => Math.abs(q[1] - top) < 1e-3).map((q) => q[0])) : (zn.max[0] - zn.min[0]) * 0.3;
      const x = flat - 0.085;
      const y = top - 0.075;
      const nose = mi === 0;
      const len = zn.max[2] - zn.min[2] - 0.6;
      const zc = (zn.max[2] + zn.min[2]) / 2 + (nose ? 0.2 : 0);
      for (const sx of [-1, 1]) {
        P.box('dark', 0.1, 0.03, len + 0.04, [sx * x, y + 0.02, zc]);
        lamp(`zone:${zn.id}`, new THREE.BoxGeometry(0.07, 0.012, nose ? len - 0.4 : len), new THREE.Matrix4().makeTranslation(sx * x, y, zc));
      }
    });
    for (const m of def.modules) {
      const hw = m.profile[m.profile.length - 1][0];
      for (let c = 1; c < m.cols; c++) {
        const z = m.z0 + ((m.z1 - m.z0) * c) / m.cols;
        for (const sx of [-1, 1]) lamp('emergency', new THREE.BoxGeometry(0.025, 0.035, 0.1), new THREE.Matrix4().makeTranslation(sx * (hw - 0.03), 0.16, z));
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
          const y = ann.at[1] - row * ann.cell[1];
          lamp(`ann:${i}`, new THREE.BoxGeometry(ann.cell[0] * 0.82, ann.cell[1] * 0.62, 0.006), F.clone().multiply(new THREE.Matrix4().makeTranslation(x, y, 0.008)));
        });
      }
    }
    // indicator LED above every control (the master caution button is its own lens)
    for (const c of def.controls) {
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
      const sx = Math.sign(e.c[0]) || 1;
      const g = new THREE.CylinderGeometry(e.half[1] * 1.02, e.half[1] * 1.02, e.half[2] * 1.4, 16, 1, true, sx > 0 ? Math.PI / 2 - 0.34 : -Math.PI / 2 - 0.34, 0.68);
      const uv = g.getAttribute('uv') as THREE.BufferAttribute;
      for (let i = 0; i < uv.count; i++) {
        const u = uv.getX(i);
        const v = uv.getY(i);
        if (sx > 0) uv.setXY(i, 1 - v, u);
        else uv.setXY(i, v, 1 - u);
      }
      g.rotateX(Math.PI / 2);
      g.translate(e.c[0], e.c[1], e.c[2]);
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
    for (const part of def.parts) {
      const own = new Map<Slot, THREE.MeshStandardMaterial>();
      const trimColor = lin(...maker(part.maker).trim);
      const mat = (slot: Slot): THREE.Material => {
        if (slot !== 'body' && slot !== 'trim' && slot !== 'accent') return shared[slot] ?? this.mats.dark;
        let m = own.get(slot);
        if (!m) {
          m = ((slot === 'body' ? this.mats.machine : slot === 'trim' ? this.mats.edge : this.mats.accent) as THREE.MeshStandardMaterial).clone();
          if (slot === 'accent') m.color.copy(trimColor);
          own.set(slot, m);
        }
        return m;
      };
      const model = machineModel(part, mat);
      this.root.add(model.group);
      const mover = part.type === 'radiator' || part.type === 'solar' ? partKey(part, 'deploy', `${part.id}.deploy`) : null;
      this.machines.push({ part, mats: [...own.values()].map((m) => ({ mat: m, base: m.color.clone() })), animate: model.animate, mover: mover && def.movers.some((m) => m.key === mover) ? mover : null });
      model.animate?.(mover ? this.sim.mover(mover) : 1);
    }
    for (const prop of def.props) this.root.add(propModel(prop, (slot) => shared[slot] ?? (slot === 'body' ? this.mats.console : this.mats.dark)).group);

    for (const [key, geo] of P.build()) {
      const mesh = new THREE.Mesh(geo, this.mats[key]);
      mesh.name = `decor-${key}`;
      mesh.castShadow = mesh.receiveShadow = true;
      this.root.add(mesh);
      this.decorCount++;
    }
    const lampMesh = new THREE.Mesh(mergeGeometries(lamps)!, this.lampMat);
    lampMesh.name = 'lamps';
    lampMesh.frustumCulled = false;
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
      if (this.hostHidden(con.host)) continue;
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
          const y = ann.at[1] - row * ann.cell[1] - ann.cell[1] * 0.55;
          label(name, F, x, y, 0.011);
        });
      }
    }
    // control LEDs live in the lamp mesh; hide the ones whose console is gone
    for (const c of def.controls) this.lampMat.levels[this.lampIds.get(`led:${c.index}`)!].w = this.hostHidden(c.host) ? 0 : 1;
    def.indicators.forEach((ind, i) => {
      const con = def.consoles.find((c) => c.id === ind.console)!;
      for (const name of ind.kind === 'airlock' ? [0, 1, 2].map((k) => `ind:${i}:${k}`) : [`ind:${i}`]) this.lampMat.levels[this.lampIds.get(name)!].w = this.hostHidden(con.host) ? 0 : 1;
    });
    for (const m of [this.consoleMesh, this.labelMesh]) {
      if (!m) continue;
      this.root.remove(m);
      m.geometry.dispose();
    }
    const built = P.build();
    const geos = [...built.entries()];
    // console + dark merged via groups
    const merged = mergeGeometries(geos.map(([, g]) => g), true)!;
    this.consoleMesh = new THREE.Mesh(merged, geos.map(([k]) => this.mats[k]));
    this.consoleMesh.name = 'consoles';
    this.consoleMesh.castShadow = this.consoleMesh.receiveShadow = true;
    this.root.add(this.consoleMesh);
    if (labels.length) {
      this.labelMesh = new THREE.Mesh(mergeGeometries(labels)!, new THREE.MeshBasicMaterial({ map: this.labelAtlas.tex, transparent: true, depthWrite: false, color: new THREE.Color(0.9, 0.95, 1) }));
      this.labelMesh.name = 'labels';
      this.labelMesh.renderOrder = 3;
      this.root.add(this.labelMesh);
    }
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
      m.castShadow = true;
      m.frustumCulled = false;
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

  update(dt: number, time: number, anim: ShipAnimState) {
    const sim = this.sim;
    const def = sim.def;
    const sw = sim.sw;
    this.rebuildConsoles();

    // heat glow decays
    for (let i = 0; i < this.heat.length; i++) {
      if (this.heat[i] <= 0) continue;
      this.heat[i] = Math.max(0, this.heat[i] - dt * 0.45);
      this.writePanel(i);
    }

    // doors: the two leaves slide apart along the wall
    for (const d of def.doors) {
      const leaves = this.doorLeaves.get(d.key)!;
      const open = anim.movers[d.key] ?? 0;
      const a = doorAxis(d);
      leaves.forEach((m, k) => {
        const side = k === 0 ? -1 : 1;
        const s = side * (d.w / 4 + open * (d.w / 2 - 0.03));
        m.position.set(d.c[0] + a[0] * s + d.n[0] * d.offset, d.c[1] + d.h / 2, d.c[2] + a[2] * s + d.n[2] * d.offset);
      });
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
      const m = new THREE.Matrix4();
      const q = new THREE.Quaternion();
      const up = new THREE.Vector3(0, 1, 0);
      for (let i = 0; i < pistons.a.length; i++) {
        const A = new THREE.Vector3(...pistons.a[i]);
        const B = new THREE.Vector3(pistons.b[i][0], pistons.b[i][1], pistons.b[i][2]).applyMatrix4(this.ramp.matrix);
        const dir = B.clone().sub(A);
        const L = dir.length();
        q.setFromUnitVectors(up, dir.normalize());
        pistons.barrel.setMatrixAt(i, m.compose(A, q, new THREE.Vector3(1, Math.min(1.0, L * 0.55), 1)));
        pistons.rod.setMatrixAt(i, m.compose(A.clone().addScaledVector(dir, L * 0.2), q, new THREE.Vector3(1, L * 0.8, 1)));
      }
      pistons.barrel.instanceMatrix.needsUpdate = true;
      pistons.rod.instanceMatrix.needsUpdate = true;
    }
    // shutters roll down from their housings
    const shield = def.shield ? anim.movers[def.shield.key] ?? 0 : 0;
    if (def.shield && shield !== this.lastShield) {
      this.lastShield = shield;
      const m = new THREE.Matrix4();
      def.shield.plates.forEach((pl, i) => {
        const h = Math.max(0.001, pl.h * shield);
        const F = frameMatrix(pl.c, pl.u, pl.v, pl.n);
        m.copy(F).multiply(new THREE.Matrix4().compose(new THREE.Vector3(0, pl.h / 2 - h / 2, 0), new THREE.Quaternion(), new THREE.Vector3(pl.w, h, 0.035)));
        this.shield.setMatrixAt(i, m);
      });
      this.shield.instanceMatrix.needsUpdate = true;
      this.shield.visible = shield > 0.005;
    }

    // controls: smooth throws, click feedback
    const tmp = new THREE.Matrix4();
    for (const c of def.controls) {
      const slot = this.partSlot[c.index];
      const target = sw[c.key] ?? 0;
      this.shown[c.index] += (target - this.shown[c.index]) * Math.min(1, dt * 14);
      this.press[c.index] = Math.max(0, this.press[c.index] - dt * 7);
      if (!slot.kind) continue;
      const F = frameMatrix(c.c, c.u, c.v, c.n);
      const s = this.shown[c.index];
      const hidden = this.hostHidden(c.host);
      let L: THREE.Matrix4;
      switch (slot.kind) {
        case 'cap':
          L = new THREE.Matrix4().compose(new THREE.Vector3(0, 0, 0.019 - this.press[c.index] * 0.007), new THREE.Quaternion(), new THREE.Vector3(c.kind === 'bezel' ? 0.042 : 0.046, c.kind === 'bezel' ? 0.016 : 0.046, 0.016));
          break;
        case 'bat':
          L = new THREE.Matrix4().makeTranslation(0, 0, 0.012).multiply(new THREE.Matrix4().makeRotationX(-0.5 + (1 - s) * 1.0));
          break;
        case 'handle':
          L = new THREE.Matrix4().makeTranslation(0, (s - 0.5) * 0.11, 0.012);
          break;
        case 'knob': {
          const span = Math.max(1, c.states.length - 1);
          L = new THREE.Matrix4().makeRotationZ((s / span) * Math.PI * 1.35).multiply(new THREE.Matrix4().makeTranslation(0, 0, 0.012));
          break;
        }
        case 'lid':
          // hinge along the top edge: shut flat, open it flips up and off the button
          L = new THREE.Matrix4().makeTranslation(0, 0.04, 0).multiply(new THREE.Matrix4().makeRotationX(-s * 1.65)).multiply(new THREE.Matrix4().makeTranslation(0, -0.04, 0.012));
          break;
        default:
          L = new THREE.Matrix4().makeTranslation(0, 0, 0.02).multiply(new THREE.Matrix4().makeRotationX(s > 0.5 ? -0.24 : 0.24)).multiply(new THREE.Matrix4().makeScale(0.036, 0.05, 0.012));
      }
      if (hidden) L.makeScale(0, 0, 0);
      this.parts[slot.kind].setMatrixAt(slot.i, tmp.multiplyMatrices(F, L));
    }
    for (const m of Object.values(this.parts)) m.instanceMatrix.needsUpdate = true;

    this.refused.clear();
    for (const c of def.controls) {
      if (c.host >= 0 && sim.hole(c.host)) {
        this.blocks.live(String(c.index), null, time);
        continue;
      }
      if (this.blocks.live(String(c.index), sim.blocked(c), time)) this.refused.add(c.index);
    }

    // ---- lamps --------------------------------------------------------------------------------------
    const L = this.lampMat;
    const id = (n: string) => this.lampIds.get(n)!;
    const blink = (hz: number, duty = 0.5) => (time * hz) % 1 < duty;
    const lk = def.lighting;
    const lightsOn = sim.powered(lk.cabin);
    const ext = sim.powered(lk.exterior);
    def.zones.forEach((zn) => {
      const on = lightsOn && sw[zn.lightKey] === 1;
      L.set(id(`zone:${zn.id}`), on ? 7 : 0.02, on ? 6.6 : 0.02, on ? 6 : 0.02);
    });
    const emergency = !lightsOn;
    L.set(id('emergency'), emergency ? (blink(0.5, 0.85) ? 6 : 2.5) : 0.04, emergency ? 0.25 : 0.01, emergency ? 0.1 : 0.01);
    const nav = ext && sw[lk.nav] === 1;
    L.set(id('nav-red'), nav ? 8 : 0.05, nav ? 0.3 : 0, 0);
    L.set(id('nav-green'), 0, nav ? 7 : 0.05, nav ? 1.2 : 0);
    const strobe = nav && (time * 0.8) % 1 < 0.06;
    L.set(id('strobe'), strobe ? 40 : 0.1, strobe ? 40 : 0.1, strobe ? 40 : 0.1);
    const beacon = ext && sw[lk.beacon] === 1 ? Math.max(0, Math.sin(time * Math.PI * 1.2)) ** 8 : 0;
    L.set(id('beacon'), 0.1 + beacon * 25, beacon * 1.2, 0);
    const landing = ext && sw[lk.landing] === 1;
    L.set(id('landing'), landing ? 30 : 0.1, landing ? 29 : 0.1, landing ? 26 : 0.1);
    this.landing.intensity = landing ? 900 : 0;
    const reactors = this.reactors();
    const online = (r: Reactor | undefined) => !!r && r.state(sim.st) === RX.online;
    const flick = 1 + Math.sin(time * 9.1) * 0.05 + Math.sin(time * 23.7) * 0.03;
    // engines are cold on the pad: a faint warm-up glow with a reactor online, brighter with thrust
    const thrust = Math.max(0, ...sim.sys.modules.filter((m): m is Engine => m.id.startsWith('engine:')).map((e) => e.thrust(sim.st)));
    const warm = (reactors.some(online) ? 0.006 : 0) + thrust * 2;
    L.set(id('nozzle'), warm * flick, warm * 2 * flick, warm * 6.6 * flick);
    const grid = sim.sys.power;
    const lit = !grid || sim.st[grid.iLive] === 1;
    def.indicators.forEach((ind, i) => {
      if (ind.kind === 'reactor-core') {
        const r = reactors.find((x) => x.part.id === ind.ref) ?? reactors[0];
        const on = online(r);
        L.set(id(`ind:${i}`), on ? 1.5 * flick : 0.02, on ? 4 * flick : 0.02, on ? 7 * flick : 0.03);
      } else if (ind.kind === 'gear-greens') {
        const down = lit && sim.mover(ind.ref ?? def.gear?.key ?? '') > 0.99;
        L.set(id(`ind:${i}`), 0, down ? 5 : 0.03, down ? 1 : 0);
      } else if (ind.kind === 'airlock') {
        const ph = sim.vars.has('lock.phase') ? sim.get('lock.phase') : LOCK.in;
        const cyc = ph !== LOCK.in && ph !== LOCK.out;
        L.set(id(`ind:${i}:0`), 0, lit && ph === LOCK.in ? 5 : 0.03, lit && ph === LOCK.in ? 1 : 0);
        L.set(id(`ind:${i}:1`), lit && cyc && blink(2) ? 5 : 0.05, lit && cyc && blink(2) ? 2.6 : 0.03, 0);
        L.set(id(`ind:${i}:2`), lit && ph === LOCK.out ? 6 : 0.05, lit && ph === LOCK.out ? 0.3 : 0.01, 0);
      }
    });
    const caution = sw[def.caution] === 1 && blink(2);
    L.set(id('caution-lens'), caution ? 9 : 0.15, caution ? 5 : 0.08, 0);
    const active = sim.sys.active(sim.st);
    def.annunciator?.lamps.forEach((name, i) => {
      const hit = active.filter((a) => a.lamp === name);
      const hot = hit.some((a) => a.level === 2);
      const on = hit.length > 0 && (!hot || blink(2));
      L.set(id(`ann:${i}`), on ? (hot ? 7 : 5.5) : 0.12, on ? (hot ? 0.35 : 3.4) : 0.06, on ? 0.05 : 0.02);
    });
    for (const m of this.machines) {
      if (m.animate && m.mover) m.animate(anim.movers[m.mover] ?? 0);
      const ratio = sim.partHp(m.part) / m.part.maxHp;
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
    // seat umbilical port: green while a pack is docked
    def.seats.forEach((st, i) => {
      const docked = this.seatOccupied[i];
      L.set(id(`seat:${st.id}`), docked ? 0 : 0.6, docked ? 4 : 0.35, docked ? 1 : 0);
    });
    for (const c of def.controls) this.setLed(c, time, anim, this.refused.has(c.index));

    // cabin lights (shader lights; emergency = dim red)
    const center = new THREE.Vector3();
    const half = new THREE.Vector3();
    const col = new THREE.Color();
    for (const ls of this.lightSlots) {
      const zn = def.zones[ls.zone];
      const on = lightsOn && sw[zn.lightKey] === 1;
      center.set((zn.min[0] + zn.max[0]) / 2, (zn.min[1] + zn.max[1]) / 2, (zn.min[2] + zn.max[2]) / 2);
      half.set((zn.max[0] - zn.min[0]) / 2 + 0.14, (zn.max[1] - zn.min[1]) / 2 + 0.14, (zn.max[2] - zn.min[2]) / 2 + 0.14);
      center.applyMatrix4(this.root.matrixWorld);
      const intensity = on ? zn.lux ?? 4.2 : emergency ? (blink(0.5, 0.85) ? 0.9 : 0.35) : 0;
      if (on) col.setRGB(1, 0.93, 0.84);
      else col.setRGB(1, 0.06, 0.03);
      setInteriorLight(ls.slot, ls.pos.clone().applyMatrix4(this.root.matrixWorld), col, intensity, center, sim.place.yaw, half);
    }

    this.screens.update(time, anim, this.hostHidden);
  }

  private reactors() {
    return this.sim.sys.modules.filter((m): m is Reactor => m.id.startsWith('reactor:'));
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
    this.blocks.pin(String(index));
    this.refused.add(index);
  }

  isRefused(index: number) {
    return this.refused.has(index);
  }

  private setLed(c: ControlDef, time: number, anim: ShipAnimState, refused: boolean) {
    const sim = this.sim;
    const i = this.lampIds.get(`led:${c.index}`)!;
    const L = this.lampMat;
    const vis = L.levels[i].w > 0.5;
    const set = (r: number, g: number, b: number) => L.set(i, r, g, b, vis);
    if (refused) return set(5.2, 2.6, 0.2);
    if (c.index === this.pointed) return (time * 2.5) % 1 < 0.55 ? set(0, 4, 6) : set(0, 0.3, 0.5);
    const v = sim.sw[c.key] ?? 0;
    if (c.kind === 'master') return set(0, 0, 0);
    if (c.kind === 'bezel') return (sim.sw[c.key] ?? 0) === (c.value ?? 0) ? set(0, 2.4, 4) : set(0.04, 0.08, 0.1);
    if (c.kind === 'cover') return v ? set(0.5, 0.35, 0.05) : set(0.04, 0.04, 0.04);
    if (c.kind === 'breaker') return v ? set(0, 4, 0.8) : set(5, 0.3, 0);
    if (this.runKeys.has(c.key)) return v ? set(0, 4, 0.8) : set(5, 0.3, 0);
    // anything that travels (doors, ramp, gear, shutters, radiators): amber blink while on its way
    const moving = anim.movers[c.key];
    if (moving !== undefined) {
      const travelling = Math.abs(moving - v) > 0.01;
      if (travelling) return (time * 3) % 1 < 0.5 ? set(5, 2.6, 0) : set(0.1, 0.05, 0);
      const bus = c.requires ?? this.effect.get(c.key);
      if (bus && !sim.powered(bus)) return set(0.6, 0.02, 0);
      return v ? set(0, 4, 0.8) : set(0.05, 0.05, 0.05);
    }
    const eff = this.effect.get(c.key);
    if (!v) return set(0.04, 0.04, 0.04);
    return !eff || sim.powered(eff) ? set(0, 4, 0.8) : set(5, 2.6, 0);
  }
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
  return litMaterial(csm, { map: tex, transparent: true, roughness: 0.6, metalness: 0.1, polygonOffset: true, polygonOffsetFactor: -2 });
}
