import * as THREE from 'three';

/**
 * Cabin lighting without real three.js lights: every material that can be inside a ship (hull
 * lining, decks, props, suits) evaluates up to MAX_SHIP_LIGHTS point lights in its own shader,
 * each masked to its compartment box. So cabin lights never leak through the hull onto the
 * terrain, and they cost nothing to the terrain/rock shaders (a real PointLight would be paid by
 * every lit pixel on screen, and has no shadows to stop it at the walls).
 */
export const MAX_SHIP_LIGHTS = 8;

const uniforms = {
  uShipLightPos: { value: Array.from({ length: MAX_SHIP_LIGHTS }, () => new THREE.Vector3()) },
  uShipLightCol: { value: Array.from({ length: MAX_SHIP_LIGHTS }, () => new THREE.Vector3()) },
  /** Compartment box centre in VIEW space (relative to the camera: exact however far from the origin). */
  uShipLightBox: { value: Array.from({ length: MAX_SHIP_LIGHTS }, () => new THREE.Vector3()) },
  /** View → compartment-box rotation (the camera's orientation, then the inverse of the ship's), as a quaternion. */
  uShipLightRot: { value: Array.from({ length: MAX_SHIP_LIGHTS }, () => new THREE.Vector4(0, 0, 0, 1)) },
  uShipLightHalf: { value: Array.from({ length: MAX_SHIP_LIGHTS }, () => new THREE.Vector3()) },
};

/** A cabin light asked for this frame (any ship). */
interface Light {
  world: THREE.Vector3;
  col: THREE.Vector3;
  box: THREE.Vector3;
  /** World → box rotation (inverse of the ship's orientation). */
  rot: THREE.Quaternion;
  half: THREE.Vector3;
  /** How much it matters to the camera now (smaller first). */
  score: number;
}

// a pool reused every frame (lights come and go with the ships and their switches)
const pool: Light[] = [];
let asked = 0;
const _rel = new THREE.Vector3();
const _cam = new THREE.Vector3();
const _q = new THREE.Quaternion();
const _camQ = new THREE.Quaternion();

/**
 * Ask for a cabin light this frame: world position, linear colour × intensity (candela) and the
 * compartment box (world centre, the ship's orientation, half extents in ship space) outside of
 * which it lights nothing. The box turns with the ship however it flies. Every ship asks for all
 * of its lights; `updateInteriorLights` keeps the MAX_SHIP_LIGHTS that matter most to the camera,
 * so a big ship (or three ships) never runs out of slots and leaves a room dark.
 */
export function setInteriorLight(pos: THREE.Vector3, color: THREE.Color, intensity: number, boxCenter: THREE.Vector3, shipQ: THREE.Quaternion, half: THREE.Vector3) {
  if (intensity <= 0) return;
  const l = (pool[asked] ??= { world: new THREE.Vector3(), col: new THREE.Vector3(), box: new THREE.Vector3(), rot: new THREE.Quaternion(), half: new THREE.Vector3(), score: 0 });
  asked++;
  l.world.copy(pos);
  l.col.set(color.r * intensity, color.g * intensity, color.b * intensity);
  l.box.copy(boxCenter);
  l.rot.set(-shipQ.x, -shipQ.y, -shipQ.z, shipQ.w);
  l.half.copy(half);
}

/**
 * Once per frame after the camera moved and every ship asked for its lights: the lights whose room
 * is nearest the camera (the one it is in first, then by distance to the light) take the slots, in
 * view space; the rest wait for next frame.
 */
export function updateInteriorLights(camera: THREE.Camera) {
  const cam = camera.getWorldPosition(_cam);
  for (let i = 0; i < asked; i++) {
    const l = pool[i];
    // camera in the box's own axes: how far outside the room it is
    _rel.copy(cam).sub(l.box).applyQuaternion(l.rot);
    const ox = Math.max(0, Math.abs(_rel.x) - l.half.x);
    const oy = Math.max(0, Math.abs(_rel.y) - l.half.y);
    const oz = Math.max(0, Math.abs(_rel.z) - l.half.z);
    l.score = Math.sqrt(ox * ox + oy * oy + oz * oz) * 4 + l.world.distanceTo(cam);
  }
  // the best MAX_SHIP_LIGHTS to the front of the pool (partial selection: no sort, no copy)
  const n = Math.min(asked, MAX_SHIP_LIGHTS);
  for (let i = 0; i < n; i++) {
    let best = i;
    for (let j = i + 1; j < asked; j++) if (pool[j].score < pool[best].score) best = j;
    if (best !== i) {
      const t = pool[i];
      pool[i] = pool[best];
      pool[best] = t;
    }
  }
  for (let i = 0; i < MAX_SHIP_LIGHTS; i++) {
    const l = i < n ? pool[i] : undefined;
    if (!l) {
      uniforms.uShipLightCol.value[i].set(0, 0, 0);
      continue;
    }
    uniforms.uShipLightPos.value[i].copy(l.world).applyMatrix4(camera.matrixWorldInverse);
    uniforms.uShipLightCol.value[i].copy(l.col);
    // everything relative to the camera, worked out here in double precision: a ship in orbit is
    // millions of metres from the origin, where float32 world positions in a shader are off by ~10 cm
    uniforms.uShipLightBox.value[i].copy(l.box).applyMatrix4(camera.matrixWorldInverse);
    _q.copy(l.rot).multiply(camera.getWorldQuaternion(_camQ));
    uniforms.uShipLightRot.value[i].set(_q.x, _q.y, _q.z, _q.w);
    uniforms.uShipLightHalf.value[i].copy(l.half);
  }
  asked = 0;
}

/** Add the cabin lights to a standard/physical material (chains existing onBeforeCompile hooks). */
export function patchInteriorLights<T extends THREE.MeshStandardMaterial>(mat: T): T {
  if (mat.userData.cabinLights) return mat; // shared materials get visited once per mesh
  mat.userData.cabinLights = true;
  const prev = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    prev?.call(mat, shader, renderer);
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <common>',
        `#include <common>
        uniform vec3 uShipLightPos[${MAX_SHIP_LIGHTS}];
        uniform vec3 uShipLightCol[${MAX_SHIP_LIGHTS}];
        uniform vec3 uShipLightBox[${MAX_SHIP_LIGHTS}];
        uniform vec4 uShipLightRot[${MAX_SHIP_LIGHTS}];
        uniform vec3 uShipLightHalf[${MAX_SHIP_LIGHTS}];`,
      )
      .replace(
        '#include <lights_fragment_begin>',
        `#include <lights_fragment_begin>
        for (int i = 0; i < ${MAX_SHIP_LIGHTS}; i++) {
          vec3 col = uShipLightCol[i];
          if (col.r + col.g + col.b <= 0.0) continue;
          vec3 rel = geometryPosition - uShipLightBox[i];
          vec4 r = uShipLightRot[i];
          vec3 loc = rel + 2.0 * cross(r.xyz, cross(r.xyz, rel) + r.w * rel);
          vec3 q = abs(loc) - uShipLightHalf[i];
          if (max(q.x, max(q.y, q.z)) > 0.0) continue;
          vec3 Lv = uShipLightPos[i] - geometryPosition;
          float d2 = max(dot(Lv, Lv), 0.2);
          IncidentLight il;
          il.direction = Lv * inversesqrt(d2);
          il.color = col / d2;
          il.visible = true;
          RE_Direct(il, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight);
        }`,
      );
  };
  const key = mat.customProgramCacheKey.bind(mat);
  mat.customProgramCacheKey = () => `${key()}+cabin2`;
  return mat;
}
