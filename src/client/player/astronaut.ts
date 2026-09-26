import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';

/** Layer used for helmet meshes: hidden from the first-person camera, still casts shadows. */
export const HELMET_LAYER = 1;
const HELMET_MATERIALS = new Set(['Helmet', 'HelmetDark', 'Visor', 'HelmetInner', 'Lamp']);

// Rest-pose dimensions of the rig (metres, model space).
const THIGH = 0.4;
const SHIN = 0.397;
const HIP_Y = 0.92;
const ANKLE_Y = 0.125;
const EYE = new THREE.Vector3(0, 1.7, 0.085);

/** Shared GLB template + material upgrades. */
export class AstronautAsset {
  private constructor(readonly template: THREE.Object3D) {}

  private csm: CSM | null = null;

  static async load(url: string, csm: CSM | null): Promise<AstronautAsset> {
    const gltf = await new GLTFLoader().loadAsync(url);
    const root = gltf.scene;
    root.traverse((o) => {
      const mesh = o as THREE.Mesh;
      if (!mesh.isMesh) return;
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      mesh.frustumCulled = false; // skinned bounds are unreliable
      const mat = mesh.material as THREE.MeshStandardMaterial;
      mat.userData.bakedAO = mat.vertexColors;
      mat.vertexColors = false;
      tuneMaterial(mat);
      if (HELMET_MATERIALS.has(mat.name)) mesh.layers.set(HELMET_LAYER);
    });
    const asset = new AstronautAsset(root);
    asset.csm = csm;
    return asset;
  }

  /** Per-instance material: clone + shader patches (baked AO, cascaded shadows). */
  instanceMaterial(src: THREE.MeshStandardMaterial) {
    const mat = src.clone();
    mat.userData.bakedAO = src.userData.bakedAO;
    patchSuitShader(mat, this.csm);
    return mat;
  }
}

function tuneMaterial(mat: THREE.MeshStandardMaterial) {
  switch (mat.name) {
    case 'Visor':
      // reflect the sunlit ground strongly so the gold reads instead of a black ball
      mat.roughness = 0.05;
      mat.envMapIntensity = 6;
      break;
    case 'Lamp':
      mat.emissiveIntensity = 0;
      break;
    case 'SuitBody':
    case 'SuitStripe':
    case 'SuitFabric':
    case 'Glove':
    case 'Boot':
      mat.envMapIntensity = 0.8;
      if (mat.normalMap) mat.normalScale.set(0.85, 0.85);
      break;
  }
}

/** Baked AO lives in COLOR_0: apply it to indirect light (and a little to direct light)
 *  instead of tinting the albedo. Every lit material must also be registered with CSM. */
function patchSuitShader(mat: THREE.MeshStandardMaterial, csm: CSM | null) {
  const hasAO = !!mat.userData.bakedAO;
  // first-person clip sphere around the eye (w = radius, 0 = off)
  const clip = { value: new THREE.Vector4(0, 0, 0, 0) };
  mat.userData.clip = clip;
  if (csm) csm.setupMaterial(mat);
  const csmHook = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    csmHook?.call(mat, shader, renderer);
    shader.uniforms.uEyeClip = clip;
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nvarying vec3 vSuitWorld;')
      .replace('#include <project_vertex>', '#include <project_vertex>\nvSuitWorld = (modelMatrix * vec4(transformed, 1.0)).xyz;');
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', '#include <common>\nuniform vec4 uEyeClip;\nvarying vec3 vSuitWorld;')
      .replace('void main() {', 'void main() {\n  if (uEyeClip.w > 0.0 && distance(vSuitWorld, uEyeClip.xyz) < uEyeClip.w) discard;');
    if (!hasAO) return;
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nattribute vec4 color;\nvarying float vSuitAO;')
      .replace('#include <begin_vertex>', '#include <begin_vertex>\nvSuitAO = color.r;');
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', '#include <common>\nvarying float vSuitAO;')
      .replace(
        '#include <aomap_fragment>',
        `#include <aomap_fragment>
        float suitAO = clamp(vSuitAO * 1.08, 0.0, 1.0);
        reflectedLight.indirectDiffuse *= suitAO;
        reflectedLight.indirectSpecular *= suitAO * suitAO;
        reflectedLight.directDiffuse *= mix(1.0, suitAO, 0.45);
        reflectedLight.directSpecular *= mix(1.0, suitAO, 0.6);`,
      );
  };
  mat.customProgramCacheKey = () => `suit2-${mat.name}-${hasAO}`;
  mat.needsUpdate = true;
}

export interface AnimInput {
  /** World-space velocity (m/s). */
  velocity: THREE.Vector3;
  /** Body heading (rad); the astronaut faces (-sin, 0, -cos)·… i.e. -Z at yaw 0. */
  yaw: number;
  /** Look pitch (rad, + up). */
  pitch: number;
  grounded: boolean;
  crouch: boolean;
}

type BoneName =
  | 'pelvis' | 'spine' | 'chest' | 'neck'
  | 'clavicleL' | 'upperarmL' | 'forearmL' | 'handL'
  | 'clavicleR' | 'upperarmR' | 'forearmR' | 'handR'
  | 'thighL' | 'shinL' | 'footL' | 'toeL'
  | 'thighR' | 'shinR' | 'footR' | 'toeR';

interface BoneRig {
  bone: THREE.Bone;
  rest: THREE.Quaternion;
  restPos: THREE.Vector3;
  /** Model-space axes expressed in the bone's rest frame. */
  ax: THREE.Vector3;
  ay: THREE.Vector3;
  az: THREE.Vector3;
}

const _q = new THREE.Quaternion();
const _q2 = new THREE.Quaternion();
const _m = new THREE.Matrix4();

/**
 * One astronaut in the world: GLB instance + procedural animation (lunar gait with
 * two-bone leg IK, arm swing, torso lean/look, jumps and landings).
 */
export class Astronaut {
  readonly root = new THREE.Group();
  private model: THREE.Object3D;
  private rig = {} as Record<BoneName, BoneRig>;
  private materials: THREE.MeshStandardMaterial[] = [];
  private lamp: THREE.SpotLight;
  private lampTarget = new THREE.Object3D();
  private lampsOn = false;
  private eyeLocal = new THREE.Vector3();
  private helmetMeshes: THREE.Mesh[] = [];

  // animation state
  private phase = 0;
  private gaitBlend = 0; // 0 idle → 1 moving
  private runBlend = 0; // 0 walk → 1 lope
  private airBlend = 0;
  private crouchBlend = 0;
  private landing = 0; // spring compression after landing
  private landingVel = 0;
  private wasGrounded = true;
  private lastVy = 0;
  private time = Math.random() * 100;
  private smoothSpeed = 0;
  private lean = 0;
  private dead = false;
  private deadBlend = 0;
  private weapon: THREE.Object3D | null = null;
  private weaponInv: THREE.Matrix4 | null = null;
  private weaponInvQ: THREE.Quaternion | null = null;
  private armed = false;
  private armBlend = 0;
  private recoilT = 0;

  constructor(asset: AstronautAsset) {
    this.model = SkeletonUtils.clone(asset.template);
    this.model.rotation.y = Math.PI; // model faces +Z, gameplay forward is -Z
    this.root.add(this.model);
    // private materials so stripes/lamps can differ per player
    this.model.traverse((o) => {
      const mesh = o as THREE.Mesh;
      if (!mesh.isMesh) return;
      const mat = asset.instanceMaterial(mesh.material as THREE.MeshStandardMaterial);
      mesh.material = mat;
      this.materials.push(mat);
      if (HELMET_MATERIALS.has(mat.name)) {
        this.helmetMeshes.push(mesh);
        mesh.layers.set(0);
      }
      // the dark inner bubble peeked out under the shell like a gap at the neck
      if (mat.name === 'HelmetInner') mesh.visible = false;
    });
    this.model.updateMatrixWorld(true);
    const names: BoneName[] = [
      'pelvis', 'spine', 'chest', 'neck',
      'clavicleL', 'upperarmL', 'forearmL', 'handL', 'clavicleR', 'upperarmR', 'forearmR', 'handR',
      'thighL', 'shinL', 'footL', 'toeL', 'thighR', 'shinR', 'footR', 'toeR',
    ];
    const modelInv = new THREE.Matrix4().copy(this.model.matrixWorld).invert();
    for (const name of names) {
      const bone = findBone(this.model, name);
      if (!bone) throw new Error(`astronaut rig: bone ${name} missing`);
      _m.multiplyMatrices(modelInv, bone.matrixWorld);
      const restModel = new THREE.Quaternion().setFromRotationMatrix(_m);
      const inv = restModel.clone().invert();
      this.rig[name] = {
        bone,
        rest: bone.quaternion.clone(),
        restPos: bone.position.clone(),
        ax: new THREE.Vector3(1, 0, 0).applyQuaternion(inv),
        ay: new THREE.Vector3(0, 1, 0).applyQuaternion(inv),
        az: new THREE.Vector3(0, 0, 1).applyQuaternion(inv),
      };
    }

    // helmet lamps (one spot, no shadows) attached to the chest
    this.lamp = new THREE.SpotLight(0xfff1dc, 0, 40, THREE.MathUtils.degToRad(32), 0.55, 1.6);
    this.lamp.position.set(0, 1.78, 0.12);
    this.lampTarget.position.set(0, 1.2, 4);
    this.lamp.target = this.lampTarget;
    const chest = this.rig.chest.bone;
    const chestInv = new THREE.Matrix4().copy(chest.matrixWorld).invert().multiply(this.model.matrixWorld);
    this.lamp.position.applyMatrix4(chestInv);
    this.lampTarget.position.applyMatrix4(chestInv);
    chest.add(this.lamp, this.lampTarget);
    this.eyeLocal.copy(EYE).applyMatrix4(chestInv);
  }

  /** Mount a weapon: carried on the right shoulder when armed, slung on the PLSS when holstered. */
  attachWeapon(prop: THREE.Object3D) {
    const chest = this.rig.chest.bone;
    this.model.updateMatrixWorld(true);
    this.weaponInv = new THREE.Matrix4().copy(chest.matrixWorld).invert().multiply(this.model.matrixWorld);
    this.weaponInvQ = new THREE.Quaternion().setFromRotationMatrix(this.weaponInv);
    this.weapon = new THREE.Object3D();
    this.weapon.add(prop);
    chest.add(this.weapon);
    this.placeWeapon();
  }

  /** Draw / holster. */
  setArmed(armed: boolean) {
    this.armed = armed;
  }

  get isArmed() {
    return this.armed;
  }

  /** Firing kick (visual). */
  recoil() {
    this.recoilT = 1;
  }

  private placeWeapon() {
    if (!this.weapon || !this.weaponInv) return;
    const w = smooth(this.armBlend);
    // model space (character right = -X, forward = +Z)
    _wp.set(-0.23, 1.5, 0.05).lerp(_wp2.set(0.02, 1.33, -0.34), 1 - w);
    _wp.z -= this.recoilT * this.recoilT * 0.09 * w;
    _wq.setFromEuler(_we.set(-this.recoilT * 0.12 * w, 0, 0));
    _wq.slerp(_wq2.setFromEuler(_we.set(-1.2, 0, 0.55)), 1 - w);
    this.weapon.position.copy(_wp).applyMatrix4(this.weaponInv);
    this.weapon.quaternion.copy(this.weaponInvQ!).multiply(_wq);
  }

  /** Hide suit geometry within `radius` of `eye` (first-person body awareness without clipping). */
  setEyeClip(eye: THREE.Vector3 | null, radius = 0.24) {
    for (const m of this.materials) {
      const c = m.userData.clip as { value: THREE.Vector4 } | undefined;
      if (c) c.value.set(eye?.x ?? 0, eye?.y ?? 0, eye?.z ?? 0, eye ? radius : 0);
    }
  }

  /** World positions of the two PLSS thruster nozzles. */
  nozzles(out: [THREE.Vector3, THREE.Vector3]) {
    const chest = this.rig.chest.bone;
    chest.updateWorldMatrix(true, false);
    const inv = this.nozzleLocal ?? (this.nozzleLocal = this.computeNozzles());
    out[0].copy(inv[0]).applyMatrix4(chest.matrixWorld);
    out[1].copy(inv[1]).applyMatrix4(chest.matrixWorld);
    return out;
  }
  private nozzleLocal: [THREE.Vector3, THREE.Vector3] | null = null;
  private computeNozzles(): [THREE.Vector3, THREE.Vector3] {
    const chest = this.rig.chest.bone;
    const inv = new THREE.Matrix4().copy(chest.matrixWorld).invert().multiply(this.model.matrixWorld);
    return [new THREE.Vector3(0.14, 0.92, -0.3).applyMatrix4(inv), new THREE.Vector3(-0.14, 0.92, -0.3).applyMatrix4(inv)];
  }

  setDead(dead: boolean) {
    this.dead = dead;
  }

  /** The local player's helmet goes to its own layer so the first-person camera can skip it. */
  setLocal(local: boolean) {
    for (const m of this.helmetMeshes) m.layers.set(local ? HELMET_LAYER : 0);
  }

  setStripeColor(hex: number) {
    for (const m of this.materials) if (m.name === 'SuitStripe') m.color.setHex(hex).convertSRGBToLinear();
  }

  setLamps(on: boolean) {
    if (on === this.lampsOn) return;
    this.lampsOn = on;
    this.lamp.intensity = on ? 22 : 0;
    for (const m of this.materials) {
      if (m.name !== 'Lamp') continue;
      m.emissive.setRGB(1, 0.95, 0.85);
      m.emissiveIntensity = on ? 18 : 0;
    }
  }

  get lampsEnabled() {
    return this.lampsOn;
  }

  /** Eye position in world space (inside the helmet), following the chest. */
  eyePosition(out: THREE.Vector3) {
    const chest = this.rig.chest.bone;
    chest.updateWorldMatrix(true, false);
    return out.copy(this.eyeLocal).applyMatrix4(chest.matrixWorld);
  }

  update(dt: number, input: AnimInput) {
    this.time += dt;
    // death: topple onto the back (low g → slow fall), limbs splayed
    this.deadBlend += ((this.dead ? 1 : 0) - this.deadBlend) * Math.min(1, dt * (this.dead ? 1.6 : 6));
    this.model.rotation.x = -this.deadBlend * 1.45;
    this.model.position.y = this.deadBlend * 0.3;
    if (this.deadBlend > 0.02) input = { ...input, velocity: new THREE.Vector3(), grounded: true, crouch: false };
    const vel = input.velocity;
    const hSpeed = Math.hypot(vel.x, vel.z);
    this.smoothSpeed += (hSpeed - this.smoothSpeed) * Math.min(1, dt * 6);
    const speed = this.smoothSpeed;

    // local (body-frame) horizontal velocity: forward = -Z at yaw 0
    const sy = Math.sin(input.yaw), cy = Math.cos(input.yaw);
    const fwd = -(vel.x * sy + vel.z * cy); // along facing
    const side = vel.x * cy - vel.z * sy; // to the right

    const moving = speed > 0.12 && input.grounded ? 1 : 0;
    this.gaitBlend += (moving - this.gaitBlend) * Math.min(1, dt * 5);
    const run = speed > 2.3 ? 1 : 0;
    this.runBlend += (run - this.runBlend) * Math.min(1, dt * 3);
    this.airBlend += ((input.grounded ? 0 : 1) - this.airBlend) * Math.min(1, dt * (input.grounded ? 10 : 4));
    this.crouchBlend += ((input.crouch ? 1 : 0) - this.crouchBlend) * Math.min(1, dt * 6);

    // landing spring
    if (input.grounded && !this.wasGrounded) this.landingVel += Math.min(3.5, Math.max(0, -this.lastVy)) * 1.2;
    this.wasGrounded = input.grounded;
    this.lastVy = vel.y;
    this.landingVel += (-this.landing * 90 - this.landingVel * 11) * dt;
    this.landing = Math.max(-0.05, this.landing + this.landingVel * dt);

    // gait phase advances with distance so feet stay planted
    const cycle = THREE.MathUtils.lerp(1.45, 2.6, this.runBlend); // metres per full cycle (2 steps)
    if (input.grounded) this.phase += ((speed * dt) / cycle) * Math.PI * 2;
    else this.phase += dt * 1.2;

    const stride = cycle / 2;
    const stance = THREE.MathUtils.lerp(0.62, 0.42, this.runBlend);
    const dirF = speed > 0.05 ? fwd / Math.max(speed, 1e-3) : 1;
    const dirS = speed > 0.05 ? side / Math.max(speed, 1e-3) : 0;
    const g = this.gaitBlend;

    // pelvis height: walk dip, lope flight, crouch, landing
    const breathe = Math.sin(this.time * 1.7) * 0.004;
    const lopeLift = this.runBlend * g * 0.11 * Math.max(0, Math.sin(this.phase * 2 - 0.6));
    const walkDip = (1 - this.runBlend) * g * (0.028 + 0.012 * Math.cos(this.phase * 2));
    const pelvisDrop = walkDip + this.crouchBlend * 0.36 + this.landing * 0.55 + g * 0.02 - lopeLift - breathe;

    // ---- legs (two-bone IK in the body frame) ----------------------------------------------
    const legPhase = [0, THREE.MathUtils.lerp(Math.PI, Math.PI * 0.28, this.runBlend)];
    for (let li = 0; li < 2; li++) {
      const L = li === 0 ? 'L' : 'R';
      const p = wrap(this.phase + legPhase[li]) / (Math.PI * 2); // 0..1
      let off = 0; // foot offset along travel direction
      let lift = 0;
      if (p < stance) {
        off = stride * (0.5 - p / stance);
      } else {
        const u = (p - stance) / (1 - stance);
        off = stride * (-0.5 + smooth(u));
        lift = Math.sin(Math.PI * u) * THREE.MathUtils.lerp(0.1, 0.2, this.runBlend);
      }
      off *= g;
      lift *= g;
      // airborne: tuck, reach forward when falling
      const air = this.airBlend;
      const fz = off * dirF + air * (vel.y < 0 ? 0.16 : -0.08) + (li === 0 ? 0.02 : -0.02) * air;
      const fx = off * dirS;
      const footUp = lift + air * (vel.y > 0 ? 0.2 : 0.1) - 0; // relative to ground
      const hipY = HIP_Y - pelvisDrop;
      const down = hipY - (ANKLE_Y + footUp);
      const reach = Math.hypot(fz, down);
      const d = Math.min(reach, THIGH + SHIN - 0.004);
      const knee = Math.PI - Math.acos(clampCos((THIGH * THIGH + SHIN * SHIN - d * d) / (2 * THIGH * SHIN)));
      const hip = Math.atan2(fz, down) + Math.acos(clampCos((THIGH * THIGH + d * d - SHIN * SHIN) / (2 * THIGH * d)));
      const abduct = Math.atan2(fx, down) * (li === 0 ? 1 : 1);
      // swing forward = rotation about -X; knee flexion = +X; foot keeps level (+ toe-off)
      this.pose(`thigh${L}` as BoneName, -hip, 0, -abduct * 0.9);
      this.pose(`shin${L}` as BoneName, knee, 0, 0);
      const toeOff = p > stance - 0.12 && p < stance + 0.05 ? g * 0.35 * (1 - Math.abs((p - (stance - 0.035)) / 0.085)) : 0;
      this.pose(`foot${L}` as BoneName, hip - knee + Math.max(0, toeOff) * 0.6 - air * 0.25, 0, 0);
      this.pose(`toe${L}` as BoneName, -Math.max(0, toeOff), 0, 0);
    }

    // ---- pelvis & torso -----------------------------------------------------------------------
    const pelvis = this.rig.pelvis;
    pelvis.bone.position.copy(pelvis.restPos);
    // drop along model -Y expressed in the pelvis parent's frame (root bone: aligned with model)
    pelvis.bone.position.y -= pelvisDrop;
    const sway = Math.sin(this.phase) * 0.035 * g * (1 - this.runBlend);
    const targetLean = THREE.MathUtils.clamp(fwd * 0.07, -0.12, 0.22) + this.crouchBlend * 0.28 + this.airBlend * 0.05;
    this.lean += (targetLean - this.lean) * Math.min(1, dt * 4);
    this.pose('pelvis', this.lean * 0.4, sway * 0.5, 0);
    const look = THREE.MathUtils.clamp(-input.pitch, -0.9, 1.1);
    this.pose('spine', this.lean * 0.5 + look * 0.16, -sway * 0.8, 0);
    this.pose('chest', this.lean * 0.3 + look * 0.26 + breathe * 2, -sway * 0.6, Math.sin(this.phase) * 0.02 * g);

    // ---- weapon -----------------------------------------------------------------------------------
    this.armBlend += ((this.armed && !this.dead ? 1 : 0) - this.armBlend) * Math.min(1, dt * 5);
    this.recoilT = Math.max(0, this.recoilT - dt * 4);
    this.placeWeapon();

    // ---- arms ----------------------------------------------------------------------------------
    for (let ai = 0; ai < 2; ai++) {
      const A = ai === 0 ? 'L' : 'R';
      const s = ai === 0 ? 1 : -1;
      const opp = wrap(this.phase + legPhase[1 - ai]) / (Math.PI * 2);
      const swing = Math.cos(opp * Math.PI * 2) * g * THREE.MathUtils.lerp(0.28, 0.2, this.runBlend);
      const air = this.airBlend;
      const shoulderFwd = 0.12 + swing + air * 0.25 + this.crouchBlend * 0.25 + this.runBlend * g * 0.15;
      const abduct = 0.08 + air * 0.22 + Math.sin(this.time * 0.9 + ai) * 0.01;
      const elbow = 0.35 + g * 0.15 + this.runBlend * g * 0.35 + air * 0.25 + this.crouchBlend * 0.3 + Math.max(0, swing) * 0.4;
      // weapon hold: right hand on the grip, left hand steadying the tube
      const w = smooth(this.armBlend) * (1 - this.deadBlend);
      const kick = this.recoilT * this.recoilT * 0.25;
      const hold = ai === 1 ? [1.05 - kick, -0.22, 1.55] : [1.3 - kick, 0.42, 0.95];
      const aim = look * 0.5 * w;
      this.pose(
        `upperarm${A}` as BoneName,
        -THREE.MathUtils.lerp(shoulderFwd, hold[0] + aim, w),
        0,
        THREE.MathUtils.lerp(s * abduct, hold[1], w),
      );
      this.pose(`forearm${A}` as BoneName, -THREE.MathUtils.lerp(elbow, hold[2], w), 0, 0);
      this.pose(`hand${A}` as BoneName, 0.1, 0, 0);
      if (this.deadBlend > 0.02) {
        this.pose(`upperarm${A}` as BoneName, -0.2 - this.deadBlend * 0.4, 0, s * (abduct + this.deadBlend * 0.9));
        this.pose(`forearm${A}` as BoneName, -0.35 - this.deadBlend * 0.3, 0, 0);
      }
    }
  }

  /** Apply rest * rotX(ax) * rotY(ay) * rotZ(az), angles about model-space axes. */
  private pose(name: BoneName, rx: number, ry: number, rz: number) {
    const r = this.rig[name];
    _q.copy(r.rest);
    if (rx) _q.multiply(_q2.setFromAxisAngle(r.ax, rx));
    if (ry) _q.multiply(_q2.setFromAxisAngle(r.ay, ry));
    if (rz) _q.multiply(_q2.setFromAxisAngle(r.az, rz));
    r.bone.quaternion.copy(_q);
  }

  dispose() {
    this.root.removeFromParent();
    for (const m of this.materials) m.dispose();
  }
}

function findBone(root: THREE.Object3D, name: string): THREE.Bone | null {
  let found: THREE.Bone | null = null;
  root.traverse((o) => {
    if (found || !(o as THREE.Bone).isBone) return;
    if (o.name.replace(/[._]/g, '') === name) found = o as THREE.Bone;
  });
  return found;
}

const _wp = new THREE.Vector3();
const _wp2 = new THREE.Vector3();
const _wq = new THREE.Quaternion();
const _wq2 = new THREE.Quaternion();
const _we = new THREE.Euler();

const wrap = (a: number) => ((a % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2);
const smooth = (u: number) => u * u * (3 - 2 * u);
const clampCos = (c: number) => Math.max(-1, Math.min(1, c));

