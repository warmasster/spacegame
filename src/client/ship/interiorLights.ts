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
  uShipLightBox: { value: Array.from({ length: MAX_SHIP_LIGHTS }, () => new THREE.Vector4()) },
  uShipLightHalf: { value: Array.from({ length: MAX_SHIP_LIGHTS }, () => new THREE.Vector3()) },
};

interface Slot {
  world: THREE.Vector3;
}

const slots: Slot[] = [];

/** Reserve a light slot. Returns its index or -1 when all slots are taken. */
export function allocInteriorLight() {
  if (slots.length >= MAX_SHIP_LIGHTS) return -1;
  slots.push({ world: new THREE.Vector3() });
  return slots.length - 1;
}

/**
 * Update a light: world position, linear colour × intensity (candela) and the compartment box
 * (world centre, ship yaw, half extents in ship space) outside of which it lights nothing.
 */
export function setInteriorLight(i: number, pos: THREE.Vector3, color: THREE.Color, intensity: number, boxCenter: THREE.Vector3, yaw: number, half: THREE.Vector3) {
  if (i < 0) return;
  slots[i].world.copy(pos);
  uniforms.uShipLightCol.value[i].set(color.r * intensity, color.g * intensity, color.b * intensity);
  uniforms.uShipLightBox.value[i].set(boxCenter.x, boxCenter.y, boxCenter.z, yaw);
  uniforms.uShipLightHalf.value[i].copy(half);
}

/** Once per frame after the camera moved: light positions → view space. */
export function updateInteriorLights(camera: THREE.Camera) {
  for (let i = 0; i < slots.length; i++) uniforms.uShipLightPos.value[i].copy(slots[i].world).applyMatrix4(camera.matrixWorldInverse);
}

/** Add the cabin lights to a standard/physical material (chains existing onBeforeCompile hooks). */
export function patchInteriorLights<T extends THREE.MeshStandardMaterial>(mat: T): T {
  if (mat.userData.cabinLights) return mat; // shared materials get visited once per mesh
  mat.userData.cabinLights = true;
  const prev = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    prev?.call(mat, shader, renderer);
    Object.assign(shader.uniforms, uniforms);
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nvarying vec3 vIntW;')
      .replace(
        '#include <project_vertex>',
        `#include <project_vertex>
        {
          vec4 iw = vec4(transformed, 1.0);
          #ifdef USE_INSTANCING
            iw = instanceMatrix * iw;
          #endif
          vIntW = (modelMatrix * iw).xyz;
        }`,
      );
    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <common>',
        `#include <common>
        varying vec3 vIntW;
        uniform vec3 uShipLightPos[${MAX_SHIP_LIGHTS}];
        uniform vec3 uShipLightCol[${MAX_SHIP_LIGHTS}];
        uniform vec4 uShipLightBox[${MAX_SHIP_LIGHTS}];
        uniform vec3 uShipLightHalf[${MAX_SHIP_LIGHTS}];`,
      )
      .replace(
        '#include <lights_fragment_begin>',
        `#include <lights_fragment_begin>
        for (int i = 0; i < ${MAX_SHIP_LIGHTS}; i++) {
          vec3 col = uShipLightCol[i];
          if (col.r + col.g + col.b <= 0.0) continue;
          vec4 bx = uShipLightBox[i];
          vec3 rel = vIntW - bx.xyz;
          float c = cos(bx.w);
          float s = sin(bx.w);
          vec3 loc = vec3(rel.x * c - rel.z * s, rel.y, rel.x * s + rel.z * c);
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
  mat.customProgramCacheKey = () => `${key()}+cabin`;
  return mat;
}
