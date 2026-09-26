import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';
import type { Grip, WeaponDef } from '../fx/weapons';

/** Layer used for helmet meshes: hidden from the first-person camera, still casts shadows. */
export const HELMET_LAYER = 1;
const HELMET_MATERIALS = new Set(['Helmet', 'HelmetDark', 'Visor', 'HelmetInner', 'Lamp']);

// Rest-pose dimensions of the rig (metres, model space).
const THIGH = 0.4;
const SHIN = 0.397;
const HIP_Y = 0.92;
const ANKLE_Y = 0.125;
const EYE = new THREE.Vector3(0, 1.65, 0.085);

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
      mat.metalness = 0.85;
      mat.roughness = 0.14;
      mat.envMapIntensity = 5;
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
  isLocal = false;
  private lookPitch = 0;
  private armBlend = 0;
  private weaponDef: WeaponDef | null = null;
  private torsoAng = 0;
  private torsoVel = 0;
  private kick = 0;
  private kickVel = 0;

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
    this.addNeckSeal();
    this.calibrateHands();
  }

  /**
   * Measure each glove in its bone's local space from the mesh itself (no hand-tuned numbers):
   * the palm pad (GloveGrip material) centroid gives the palm contact point and palm normal, the
   * bone axis gives the finger direction. Grasp IK then places this frame onto any grip.
   */
  readonly palm: Record<'L' | 'R', { point: THREE.Vector3; normal: THREE.Vector3; finger: THREE.Vector3 }> = {
    L: { point: new THREE.Vector3(), normal: new THREE.Vector3(), finger: new THREE.Vector3(0, 1, 0) },
    R: { point: new THREE.Vector3(), normal: new THREE.Vector3(), finger: new THREE.Vector3(0, 1, 0) },
  };
  private calibrateHands() {
    this.model.updateMatrixWorld(true);
    const hw = { L: this.rig.handL.bone.getWorldPosition(new THREE.Vector3()), R: this.rig.handR.bone.getWorldPosition(new THREE.Vector3()) };
    const verts: Record<'L' | 'R', { pad: THREE.Vector3[]; all: THREE.Vector3[] }> = { L: { pad: [], all: [] }, R: { pad: [], all: [] } };
    this.model.traverse((o) => {
      const m = o as THREE.SkinnedMesh;
      const name = (m.material as THREE.Material | undefined)?.name;
      if (!m.isSkinnedMesh || (name !== 'GloveGrip' && name !== 'Glove')) return;
      m.skeleton.update();
      const pos = m.geometry.getAttribute('position');
      for (let i = 0; i < pos.count; i++) {
        const v = m.getVertexPosition(i, new THREE.Vector3()).applyMatrix4(m.matrixWorld);
        const side = handOfVertex(m, i, this.rig.handL.bone, this.rig.handR.bone);
        if (!side) continue;
        verts[side].all.push(v);
        if (name === 'GloveGrip') verts[side].pad.push(v);
      }
    });
    const mean = (a: THREE.Vector3[]) => a.reduce((s, v) => s.add(v), new THREE.Vector3()).divideScalar(Math.max(1, a.length));
    for (const side of ['L', 'R'] as const) {
      const vs = verts[side];
      if (!vs.all.length) continue;
      const wrist = hw[side];
      const bone = this.rig[`hand${side}`].bone;
      const inv = new THREE.Matrix4().copy(bone.matrixWorld).invert();
      const p = this.palm[side];
      const sorted = vs.all.slice().sort((a, b) => b.distanceTo(wrist) - a.distanceTo(wrist));
      const tips = mean(sorted.slice(0, Math.max(1, Math.floor(sorted.length * 0.15))));
      const pad = vs.pad.filter((v) => v.distanceTo(wrist) < 0.09);
      const pc = mean(pad.length ? pad : vs.pad);
      const core = mean(vs.all.filter((v) => v.distanceTo(wrist) < 0.09));
      // local frame measured from the mesh: finger axis, palm normal, palm contact point
      p.finger.copy(tips).applyMatrix4(inv).normalize();
      p.point.copy(pc).applyMatrix4(inv);
      p.normal.copy(pc).sub(core).transformDirection(inv);
      p.normal.addScaledVector(p.finger, -p.normal.dot(p.finger)).normalize();
    }
  }

  private addNeckSeal() {
    const chest = this.rig.chest.bone;
    this.model.updateMatrixWorld(true);
    const inv = new THREE.Matrix4().copy(chest.matrixWorld).invert().multiply(this.model.matrixWorld);
    const dark = new THREE.MeshStandardMaterial({ color: 0x151618, roughness: 0.6, metalness: 0.3 });
    void dark;
    const inner = new THREE.Mesh(
      new THREE.SphereGeometry(0.166, 40, 20, 0, Math.PI * 2, 0, Math.acos((1.506 - 1.622) / 0.166)),
      new THREE.MeshStandardMaterial({ color: 0x0b0b0c, roughness: 0.8, side: THREE.DoubleSide }),
    );
    inner.position.set(0, 1.622, 0.005);
    for (const m of [inner]) {
      m.castShadow = true;
      m.applyMatrix4(inv);
      chest.add(m);
      this.helmetMeshes.push(m as THREE.Mesh);
    }
  }

  /** Mount a weapon: carried on the right shoulder when armed, slung on the PLSS when holstered. */
  attachWeapon(def: WeaponDef) {
    const prop = def.build();
    this.weaponDef = def;
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

  /**
   * Firing: the weapon's momentum goes into two damped springs — the torso rocks back
   * (angular impulse about the hips, suit+body ≈ 45 kg·m²) and the weapon slides back
   * against the shoulder — so stronger weapons kick harder without per-weapon animation.
   */
  applyRecoil(momentum: number) {
    this.torsoVel += (momentum * 0.55) / 45;
    this.kickVel += momentum / 22;
  }

  /** World position of the weapon muzzle (null when no weapon). */
  muzzle(out: THREE.Vector3) {
    if (!this.weapon || !this.weaponDef) return null;
    this.weapon.updateWorldMatrix(true, true);
    return this.weapon.children[0].localToWorld(out.copy(this.weaponDef.muzzle));
  }

  /** Current recoil pitch (rad) for the camera. */
  get recoilPitch() {
    return this.torsoAng;
  }

  private placeWeapon() {
    if (!this.weapon || !this.weaponInv || !this.weaponDef) return;
    const w = smooth(this.armBlend);
    const def = this.weaponDef;
    // shoulder carry (like a real bazooka gunner): contact on the right shoulder, tube beside the helmet
    const aim = this.lookPitch * 0.74 - this.torsoAng * 0.6;
    _wq.setFromEuler(_we.set(aim, 0.02, 0));
    _wp.set(-0.21, 1.47, 0.06).addScaledVector(_fwd.set(0, 0, -1).applyQuaternion(_wq), this.kick);
    if (def.carry === 'hip') _wp.set(-0.2, 1.05, 0.25);
    _wp.lerp(def.holster.pos, 1 - w);
    _wq.slerp(_wq2.setFromEuler(def.holster.rot), 1 - w);
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
    this.isLocal = local;
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
  /** World position of a rig bone (diagnostics / camera framing). */
  partPosition(part: BoneName, out: THREE.Vector3) {
    return this.rig[part].bone.getWorldPosition(out);
  }

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
    this.pose('spine', this.lean * 0.5 + look * 0.16 - this.torsoAng * 0.5, -sway * 0.8, 0);
    this.pose('chest', this.lean * 0.3 + look * 0.26 + breathe * 2 - this.torsoAng, -sway * 0.6, Math.sin(this.phase) * 0.02 * g);

    // ---- weapon (placed first; arms reach for it after the body pose) ------------------------------
    this.armBlend += ((this.armed && !this.dead ? 1 : 0) - this.armBlend) * Math.min(1, dt * 5);
    // recoil springs (critically-ish damped)
    const sub = 4;
    for (let i = 0; i < sub; i++) {
      const h = dt / sub;
      this.torsoVel += (-55 * this.torsoAng - 9 * this.torsoVel) * h;
      this.torsoAng += this.torsoVel * h;
      this.kickVel += (-260 * this.kick - 22 * this.kickVel) * h;
      this.kick += this.kickVel * h;
    }
    this.lookPitch = THREE.MathUtils.clamp(-input.pitch, -0.9, 1.1);
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
      this.pose(`upperarm${A}` as BoneName, -shoulderFwd, 0, s * abduct);
      this.pose(`forearm${A}` as BoneName, -elbow, 0, 0);
      this.pose(`hand${A}` as BoneName, 0.1, 0, 0);
      if (this.deadBlend > 0.02) {
        this.pose(`upperarm${A}` as BoneName, -0.2 - this.deadBlend * 0.4, 0, s * (abduct + this.deadBlend * 0.9));
        this.pose(`forearm${A}` as BoneName, -0.35 - this.deadBlend * 0.3, 0, 0);
      }
    }
    this.armIK(smooth(this.armBlend) * (1 - this.deadBlend));
  }

  /**
   * Two-bone arm IK onto the weapon grips (right hand on the pistol grip, left under the tube),
   * so the arms follow every weapon motion — gait bob, aim pitch, recoil — like the legs do.
   */
  private armIK(w: number) {
    if (!this.weapon || !this.weaponDef || w < 0.01) return;
    this.model.updateMatrixWorld(true);
    const prop = this.weapon.children[0];
    const fwdW = _fw.set(0, 0, 1).transformDirection(prop.matrixWorld);
    const modelQ = this.model.getWorldQuaternion(_mq);
    const arms: Array<[BoneName, BoneName, BoneName, Grip, number]> = [
      ['upperarmR', 'forearmR', 'handR', this.weaponDef.rightGrip, -1],
      ['upperarmL', 'forearmL', 'handL', this.weaponDef.leftGrip, 1],
    ];
    for (const [ua, fa, ha, grip, side] of arms) {
      const U = this.rig[ua].bone;
      const F = this.rig[fa].bone;
      const H = this.rig[ha].bone;
      const palm = this.palm[side > 0 ? 'L' : 'R'];
      // grasp geometry in world space: handle axis, approach side
      const gc = prop.localToWorld(_t.copy(grip.pos));
      const axis = _ga.copy(grip.axis).transformDirection(prop.matrixWorld);
      const out = _gs.copy(grip.side).transformDirection(prop.matrixWorld);
      out.addScaledVector(axis, -out.dot(axis)).normalize();
      // target hand frame: fingers wrap across the handle (perpendicular to axis and to the
      // approach side), palm faces the handle. Wrap direction goes over the top of the handle.
      const fingerW = _hd.crossVectors(axis, out);
      if (fingerW.dot(fwdW) + fingerW.y * 0.1 < 0) fingerW.negate(); // knuckles toward the muzzle
      const palmW = _pw.copy(out).negate();
      const handQ = frameQuat(palm.finger, palm.normal, fingerW, palmW, _hq);
      // wrist target so the measured palm point sits on the handle surface
      const scale = H.getWorldScale(_sc).x;
      const T = _tw.copy(palm.point).multiplyScalar(scale).applyQuaternion(handQ).negate().add(gc).addScaledVector(out, grip.radius);
      const S = U.getWorldPosition(_s);
      const E0 = F.getWorldPosition(_e0);
      const H0 = H.getWorldPosition(_h0);
      const L1 = S.distanceTo(E0);
      const L2 = E0.distanceTo(H0);
      const toT = _d.subVectors(T, S);
      const dist = Math.min(toT.length(), L1 + L2 - 0.002);
      const dir = toT.normalize();
      const pole = _p.set(side > 0 ? 0.25 : -0.8, -1, side > 0 ? 0.1 : -0.2).applyQuaternion(modelQ).normalize();
      const perp = pole.addScaledVector(dir, -pole.dot(dir)).normalize();
      const a = (L1 * L1 - L2 * L2 + dist * dist) / (2 * dist);
      const h = Math.sqrt(Math.max(0, L1 * L1 - a * a));
      const E = _e.copy(S).addScaledVector(dir, a).addScaledVector(perp, h);
      const Hn = _hn.copy(S).addScaledVector(dir, dist);
      this.aimBone(U, E0, E, S, w);
      this.model.updateMatrixWorld(true);
      this.aimBone(F, H.getWorldPosition(_h0), Hn, F.getWorldPosition(_s2), w);
      this.model.updateMatrixWorld(true);
      // full hand orientation (direction + twist), not just pointing
      const parentQ = H.parent!.getWorldQuaternion(_q6).invert();
      H.quaternion.slerp(parentQ.multiply(handQ), w);
      this.model.updateMatrixWorld(true);
      this.grasp[side > 0 ? 'L' : 'R'] = { gc: gc.clone(), axis: axis.clone(), out: out.clone(), radius: grip.radius };
    }
  }

  /** Last grasp targets (world), for diagnostics: see graspReport(). */
  private grasp: Partial<Record<'L' | 'R', { gc: THREE.Vector3; axis: THREE.Vector3; out: THREE.Vector3; radius: number }>> = {};

  /**
   * Self-check of the weapon hold: for each hand, how far the palm is from the handle surface
   * (cm), how well it faces the handle and whether the fingers cross it. Used by diag tools.
   */
  graspReport() {
    // Independent of the IK's own assumptions: measure the skinned glove mesh as rendered.
    this.model.updateMatrixWorld(true);
    const r: Record<string, { gapCm: number; palmFacingDeg: number; fingerCrossDeg: number; ok: boolean }> = {};
    const verts: Record<'L' | 'R', { pad: THREE.Vector3[]; all: THREE.Vector3[] }> = { L: { pad: [], all: [] }, R: { pad: [], all: [] } };
    const hw = { L: this.rig.handL.bone.getWorldPosition(new THREE.Vector3()), R: this.rig.handR.bone.getWorldPosition(new THREE.Vector3()) };
    this.model.traverse((o) => {
      const m = o as THREE.SkinnedMesh;
      const name = (m.material as THREE.Material | undefined)?.name;
      if (!m.isSkinnedMesh || (name !== 'GloveGrip' && name !== 'Glove')) return;
      const pos = m.geometry.getAttribute('position');
      for (let i = 0; i < pos.count; i += 2) {
        const v = m.getVertexPosition(i, new THREE.Vector3()).applyMatrix4(m.matrixWorld);
        const side = handOfVertex(m, i, this.rig.handL.bone, this.rig.handR.bone);
        if (!side) continue;
        verts[side].all.push(v);
        if (name === 'GloveGrip') verts[side].pad.push(v);
      }
    });
    const mean = (a: THREE.Vector3[]) => a.reduce((s, v) => s.add(v), new THREE.Vector3()).divideScalar(Math.max(1, a.length));
    for (const side of ['L', 'R'] as const) {
      const g = this.grasp[side];
      if (!g || !verts[side].all.length) continue;
      const wrist = hw[side];
      // fingertips = the 15% of glove vertices farthest from the wrist
      const sorted = verts[side].all.slice().sort((a, b) => b.distanceTo(wrist) - a.distanceTo(wrist));
      const tips = mean(sorted.slice(0, Math.max(1, Math.floor(sorted.length * 0.15))));
      const finger = tips.clone().sub(wrist).normalize();
      // palm = pad vertices nearer the wrist than the tips
      const pad = verts[side].pad.filter((v) => v.distanceTo(wrist) < 0.09);
      const pc = mean(pad.length ? pad : verts[side].pad);
      const rel = pc.clone().sub(g.gc);
      const radial = rel.addScaledVector(g.axis, -rel.dot(g.axis));
      const gap = radial.length() - g.radius;
      // palm faces the handle: palm point sits on the handle side of the bone axis
      const handC = mean(verts[side].all.filter((v) => v.distanceTo(wrist) < 0.09));
      const palmDir = pc.clone().sub(handC).normalize();
      const facing = THREE.MathUtils.radToDeg(palmDir.angleTo(radial.clone().negate().normalize()));
      const cross = THREE.MathUtils.radToDeg(Math.asin(Math.min(1, Math.abs(finger.dot(g.axis)))));
      r[side] = { gapCm: +(gap * 100).toFixed(1), palmFacingDeg: +facing.toFixed(0), fingerCrossDeg: +cross.toFixed(0), ok: Math.abs(gap) < 0.035 && facing < 50 && cross < 30 };
    }
    return r;
  }


  /** Rotate `bone` (pivot P) so its child currently at `from` points toward `to`, blended by w. */
  private aimBone(bone: THREE.Bone, from: THREE.Vector3, to: THREE.Vector3, P: THREE.Vector3, w: number) {
    const a = _v1.subVectors(from, P).normalize();
    const b = _v2.subVectors(to, P).normalize();
    const delta = _q3.setFromUnitVectors(a, b);
    const worldQ = bone.getWorldQuaternion(_q4);
    const target = _q5.copy(delta).multiply(worldQ);
    const parentQ = bone.parent!.getWorldQuaternion(_q6).invert();
    const local = parentQ.multiply(target);
    bone.quaternion.slerp(local, w);
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
const _wq = new THREE.Quaternion();
const _wq2 = new THREE.Quaternion();
const _we = new THREE.Euler();
const _fwd = new THREE.Vector3();
const _fw = new THREE.Vector3();
const _ga = new THREE.Vector3();
const _gs = new THREE.Vector3();
const _hd = new THREE.Vector3();
const _tw = new THREE.Vector3();
const _mq = new THREE.Quaternion();
const _s = new THREE.Vector3();
const _s2 = new THREE.Vector3();
const _e0 = new THREE.Vector3();
const _h0 = new THREE.Vector3();
const _t = new THREE.Vector3();
const _d = new THREE.Vector3();
const _p = new THREE.Vector3();
const _e = new THREE.Vector3();
const _hn = new THREE.Vector3();
const _v1 = new THREE.Vector3();
const _v2 = new THREE.Vector3();
const _q3 = new THREE.Quaternion();
const _q4 = new THREE.Quaternion();
const _q5 = new THREE.Quaternion();
const _q6 = new THREE.Quaternion();
const _pw = new THREE.Vector3();
const _hq = new THREE.Quaternion();
const _sc = new THREE.Vector3();
const _b1 = new THREE.Matrix4();
const _b2 = new THREE.Matrix4();
const _bx = new THREE.Vector3();
const _by = new THREE.Vector3();
const _bz = new THREE.Vector3();

/** Which hand bone dominates a skinned vertex (by skin weight), if any. */
function handOfVertex(m: THREE.SkinnedMesh, i: number, hl: THREE.Bone, hr: THREE.Bone): 'L' | 'R' | null {
  const si = m.geometry.getAttribute('skinIndex');
  const sw = m.geometry.getAttribute('skinWeight');
  let best = -1;
  let bw = 0;
  for (let k = 0; k < 4; k++) {
    const w = sw.getComponent(i, k);
    if (w > bw) {
      bw = w;
      best = si.getComponent(i, k);
    }
  }
  const b = m.skeleton.bones[best];
  return b === hl ? 'L' : b === hr ? 'R' : null;
}

/** Rotation mapping the orthonormal pair (a1, b1) onto (a2, b2) (b's re-orthogonalised). */
function frameQuat(a1: THREE.Vector3, b1: THREE.Vector3, a2: THREE.Vector3, b2: THREE.Vector3, out: THREE.Quaternion) {
  const basis = (a: THREE.Vector3, b: THREE.Vector3, m: THREE.Matrix4) => {
    _bx.copy(a).normalize();
    _by.copy(b).addScaledVector(_bx, -b.dot(_bx)).normalize();
    _bz.crossVectors(_bx, _by);
    return m.makeBasis(_bx, _by, _bz);
  };
  basis(a1, b1, _b1);
  basis(a2, b2, _b2);
  return out.setFromRotationMatrix(_b2.multiply(_b1.transpose()));
}

const wrap = (a: number) => ((a % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2);
const smooth = (u: number) => u * u * (3 - 2 * u);
const clampCos = (c: number) => Math.max(-1, Math.min(1, c));

