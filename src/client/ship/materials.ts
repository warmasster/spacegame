import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';

/**
 * Ship surfaces. Panels carry their own detail procedurally (no textures): per-panel attributes
 * give size, damage and a seed, so rivet rows follow every panel's edges, each plate has its own
 * tone, and damage shows as dents (normal perturbation) and scorching, glowing hot for a moment
 * after a blast.
 *
 * Panel vertex attributes: aUv (panel-local metres), aPanel (sizeU, sizeV, damage 0..1,
 * paint scheme + seed: integer part = PAINT index, fraction = per-panel random), aHeat (0..1),
 * aTan / aBit (panel u / v axes).
 */
export type PanelStyle = 'hull' | 'lining' | 'deck';

/** Hull paint schemes (integer part of aPanel.w). */
export const PAINT = { light: 0, dark: 1, hazard: 2, stripe: 3 } as const;

const STYLE_ID: Record<PanelStyle, number> = { hull: 0, lining: 1, deck: 2 };

const NOISE = /* glsl */ `
float shHash(vec2 p) { p = fract(p * vec2(123.34, 456.21)); p += dot(p, p + 45.32); return fract(p.x * p.y); }
float shNoise(vec2 p) {
  vec2 i = floor(p); vec2 f = fract(p); vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(shHash(i), shHash(i + vec2(1.0, 0.0)), u.x), mix(shHash(i + vec2(0.0, 1.0)), shHash(i + vec2(1.0, 1.0)), u.x), u.y);
}
float shFbm(vec2 p) { float s = 0.0; float a = 0.5; for (int i = 0; i < 4; i++) { s += a * shNoise(p); p = p * 2.03 + 7.1; a *= 0.5; } return s; }
`;

const HEIGHT = /* glsl */ `
float shRivets(vec2 uv, vec2 size, float inset, float pitch) {
  vec2 e2 = min(uv, size - uv);
  float ax = (fract(uv.x / pitch) - 0.5) * pitch;
  float ay = (fract(uv.y / pitch) - 0.5) * pitch;
  float r = 1.0 - smoothstep(0.0042, 0.0082, length(vec2(ax, e2.y - inset)));
  r = max(r, 1.0 - smoothstep(0.0042, 0.0082, length(vec2(ay, e2.x - inset))));
  return r;
}
float shHeight(vec2 uv, vec4 P, float aa) {
  vec2 size = P.xy;
  vec2 e2 = min(uv, size - uv);
  float edge = min(e2.x, e2.y);
  float h = -0.006 * (1.0 - smoothstep(0.0, 0.022, edge));
#if SHIP_STYLE == 0
  h += 0.0026 * shRivets(uv, size, 0.034, 0.11) * aa;
  if (fract(fract(P.w) * 7.13) > 0.55) {
    // access hatch outline
    vec2 hc = abs(uv - size * vec2(0.5, 0.55)) - size * vec2(0.2, 0.15);
    float sd = length(max(hc, 0.0)) + min(max(hc.x, hc.y), 0.0);
    h -= 0.003 * (1.0 - smoothstep(0.0015, 0.0045, abs(sd))) * aa;
  }
#elif SHIP_STYLE == 1
  // quilted insulation pads between seams
  vec2 cell = vec2(0.36, 0.26);
  vec2 f = (0.5 - abs(fract(uv / cell) - 0.5)) * cell;
  float pad = smoothstep(0.0, 0.035, min(f.x, f.y));
  h += 0.009 * sqrt(pad) * aa;
  h += 0.002 * shRivets(uv, size, 0.03, 0.14) * aa;
#elif SHIP_STYLE == 2
  // diamond tread plate
  vec2 q = uv * 20.0;
  vec2 c = floor(q);
  vec2 f = fract(q) - 0.5;
  float flip = mod(c.x + c.y, 2.0) < 1.0 ? 1.0 : -1.0;
  vec2 r = vec2(f.x + f.y * flip, f.y - f.x * flip) * 0.7071;
  float lozenge = 1.0 - smoothstep(0.045, 0.085, abs(r.y) + abs(r.x) * 0.24);
  h += 0.0013 * lozenge * aa;
#endif
  if (P.z > 0.02) h -= P.z * 0.028 * shFbm(uv * 2.6 + fract(P.w) * 40.0);
  return h;
}
`;

export function panelMaterial(style: PanelStyle, csm: CSM | null, params: THREE.MeshStandardMaterialParameters) {
  const mat = new THREE.MeshStandardMaterial(params);
  mat.name = `ship-${style}`;
  if (csm) csm.setupMaterial(mat);
  mat.defines = { ...(mat.defines ?? {}), SHIP_STYLE: STYLE_ID[style] };
  const csmHook = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    csmHook?.call(mat, shader, renderer);
    shader.vertexShader = shader.vertexShader
      .replace(
        '#include <common>',
        `#include <common>
        attribute vec4 aPanel; attribute float aHeat; attribute vec3 aTan; attribute vec3 aBit; attribute vec2 aUv;
        varying vec4 vPanel; varying float vHeat; varying vec3 vPT; varying vec3 vPB; varying vec2 vPUv; varying vec3 vPW;`,
      )
      .replace(
        '#include <project_vertex>',
        `#include <project_vertex>
        vPanel = aPanel; vHeat = aHeat; vPUv = aUv;
        vPT = normalize(normalMatrix * aTan); vPB = normalize(normalMatrix * aBit);
        vPW = (modelMatrix * vec4(transformed, 1.0)).xyz;`,
      );
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', `#include <common>\nvarying vec4 vPanel; varying float vHeat; varying vec3 vPT; varying vec3 vPB; varying vec2 vPUv; varying vec3 vPW;\n${NOISE}\n${HEIGHT}`)
      .replace(
        '#include <color_fragment>',
        `#include <color_fragment>
        vec2 shE2 = min(vPUv, vPanel.xy - vPUv);
        float shEdge = min(shE2.x, shE2.y);
        float shSeed = fract(vPanel.w);
        float shScheme = floor(vPanel.w + 0.001);
        float shN = shFbm(vPUv * 2.2 + shSeed * 23.0);
        float shScorch = clamp((vPanel.z * 1.7 - 0.3 - shN * 0.7) * 3.0, 0.0, 1.0);
        #if SHIP_STYLE == 0
          float shTone = 0.9 + 0.2 * fract(shSeed * 13.7);
          if (shScheme < 0.5 && fract(shSeed * 3.31) > 0.86) shTone *= 0.72;
          diffuseColor.rgb *= shTone;
          if (shScheme > 0.5 && shScheme < 1.5) diffuseColor.rgb *= vec3(0.2, 0.205, 0.215);
          if (shScheme > 1.5 && shScheme < 2.5) {
            float k = fract((vPUv.x + vPUv.y) * 2.6);
            diffuseColor.rgb = mix(vec3(0.02), vec3(0.52, 0.34, 0.03), step(0.5, k)) * shTone;
          }
          if (shScheme > 2.5 && vPUv.y < 0.11 && vPUv.y > 0.03) diffuseColor.rgb = vec3(0.46, 0.15, 0.02) * shTone;
          // regolith dust: settles low on the hull and in the panel seams
          float shDust = shFbm(vPW.xz * 0.7 + vPW.y * 1.3);
          diffuseColor.rgb *= mix(0.8, 1.04, shDust) * mix(0.82, 1.0, smoothstep(0.0, 0.05, shEdge));
        #elif SHIP_STYLE == 1
          diffuseColor.rgb *= (0.95 + 0.1 * fract(shSeed * 13.7)) * mix(0.75, 1.0, smoothstep(0.0, 0.04, shEdge));
        #else
          diffuseColor.rgb *= mix(0.7, 1.08, shFbm(vPUv * 1.7 + shSeed * 9.0)) * mix(0.7, 1.0, smoothstep(0.0, 0.03, shEdge));
        #endif
        diffuseColor.rgb *= mix(1.0, 0.09, shScorch);`,
      )
      .replace('#include <roughnessmap_fragment>', '#include <roughnessmap_fragment>\nroughnessFactor = mix(roughnessFactor, 0.95, shScorch);')
      .replace(
        '#include <normal_fragment_maps>',
        `{
          // procedural relief: finite differences in panel space → tilt along the panel axes
          float aa = 1.0 - smoothstep(0.0035, 0.011, length(fwidth(vPUv)));
          const float e = 0.0015;
          float h0 = shHeight(vPUv, vPanel, aa);
          vec2 g = vec2(shHeight(vPUv + vec2(e, 0.0), vPanel, aa) - h0, shHeight(vPUv + vec2(0.0, e), vPanel, aa) - h0) / e;
          normal = normalize(normal - (g.x * vPT + g.y * vPB));
        }`,
      )
      .replace('#include <emissivemap_fragment>', '#include <emissivemap_fragment>\ntotalEmissiveRadiance += vec3(3.2, 0.95, 0.22) * vHeat * vHeat * (0.25 + shScorch);');
  };
  mat.customProgramCacheKey = () => `ship-panel-${style}`;
  return mat;
}

/** Plain lit material registered with the cascaded shadows. */
export function litMaterial(csm: CSM | null, params: THREE.MeshStandardMaterialParameters) {
  const mat = new THREE.MeshStandardMaterial(params);
  if (csm) csm.setupMaterial(mat);
  return mat;
}

export function glassMaterial(csm: CSM | null) {
  const mat = new THREE.MeshStandardMaterial({
    color: new THREE.Color().setRGB(0.02, 0.028, 0.035),
    roughness: 0.06,
    metalness: 0.6,
    transparent: true,
    opacity: 0.32,
    envMapIntensity: 2.2,
    depthWrite: false,
  });
  if (csm) csm.setupMaterial(mat);
  return mat;
}

/**
 * Every light-emitting bit of the ship (fixtures, LEDs, nav lights, nozzles…) in one draw call:
 * each vertex carries a lamp index into a uniform array of (rgb · intensity, visible).
 */
export class LampMaterial extends THREE.ShaderMaterial {
  readonly levels: THREE.Vector4[];

  constructor(count: number) {
    const levels = Array.from({ length: count }, () => new THREE.Vector4(0, 0, 0, 1));
    super({
      uniforms: { uLamp: { value: levels } },
      vertexShader: /* glsl */ `
        attribute float aLamp;
        uniform vec4 uLamp[${count}];
        varying vec3 vCol;
        varying vec3 vN;
        varying vec3 vV;
        #include <common>
        #include <logdepthbuf_pars_vertex>
        void main() {
          vec4 L = uLamp[int(aLamp + 0.5)];
          vCol = L.rgb;
          vec4 mv = modelViewMatrix * vec4(position, 1.0);
          vN = normalize(normalMatrix * normal);
          vV = normalize(-mv.xyz);
          gl_Position = L.a > 0.5 ? projectionMatrix * mv : vec4(0.0, 0.0, -2.0, 1.0);
          #include <logdepthbuf_vertex>
        }`,
      fragmentShader: /* glsl */ `
        varying vec3 vCol;
        varying vec3 vN;
        varying vec3 vV;
        #include <common>
        #include <logdepthbuf_pars_fragment>
        void main() {
          #include <logdepthbuf_fragment>
          // unlit lens: a little darker at grazing angles so shapes still read when off
          float f = 0.55 + 0.45 * abs(dot(normalize(vN), normalize(vV)));
          gl_FragColor = vec4(max(vCol, vec3(0.012)) * f, 1.0);
        }`,
    });
    this.levels = levels;
  }

  set(i: number, r: number, g: number, b: number, visible = true) {
    this.levels[i].set(r, g, b, visible ? 1 : 0);
  }
}
