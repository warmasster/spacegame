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

// Relief, filtered by the pixel's footprint on the panel `w` (m): a detail smaller than a pixel is
// not switched off (it used to vanish past 3–5 m) but widened to the footprint with its height
// scaled by how much of the pixel it covers — what a mip chain would do to a baked texture. So the
// rivet rows, quilting and tread still read at 10–15 m without shimmering, and each one only
// fades once its whole pattern is below a pixel.
const HEIGHT = /* glsl */ `
float shRivets(vec2 uv, vec2 size, float inset, float pitch, float w) {
  vec2 e2 = min(uv, size - uv);
  float ax = (fract(uv.x / pitch) - 0.5) * pitch;
  float ay = (fract(uv.y / pitch) - 0.5) * pitch;
  float r = 1.0 - smoothstep(0.0042, 0.0082 + w, length(vec2(ax, e2.y - inset)));
  r = max(r, 1.0 - smoothstep(0.0042, 0.0082 + w, length(vec2(ay, e2.x - inset))));
  // coverage of a widened rivet, and gone once the row spacing itself is under ~3 pixels
  return r * (0.012 / (0.012 + w)) * (1.0 - smoothstep(pitch * 0.2, pitch * 0.4, w));
}
float shHeight(vec2 uv, vec4 P, float w) {
  vec2 size = P.xy;
  vec2 e2 = min(uv, size - uv);
  float edge = min(e2.x, e2.y);
  float h = -0.006 * (1.0 - smoothstep(0.0, 0.022 + w, edge));
#if SHIP_STYLE == 0
  h += 0.0026 * shRivets(uv, size, 0.034, 0.11, w);
  if (fract(fract(P.w) * 7.13) > 0.55) {
    // access hatch outline
    vec2 hc = abs(uv - size * vec2(0.5, 0.55)) - size * vec2(0.2, 0.15);
    float sd = length(max(hc, 0.0)) + min(max(hc.x, hc.y), 0.0);
    h -= 0.003 * (1.0 - smoothstep(0.0015, 0.0045 + w, abs(sd))) * (0.006 / (0.006 + w));
  }
#elif SHIP_STYLE == 1
  // quilted insulation pads between seams
  vec2 cell = vec2(0.36, 0.26);
  vec2 f = (0.5 - abs(fract(uv / cell) - 0.5)) * cell;
  float pad = smoothstep(0.0, 0.035 + w, min(f.x, f.y));
  h += 0.009 * sqrt(pad) * (1.0 - smoothstep(0.06, 0.12, w));
  h += 0.002 * shRivets(uv, size, 0.03, 0.14, w);
#elif SHIP_STYLE == 2
  // diamond tread plate (5 cm cells: gone once a cell is under ~2 pixels)
  vec2 q = uv * 20.0;
  vec2 c = floor(q);
  vec2 f = fract(q) - 0.5;
  float flip = mod(c.x + c.y, 2.0) < 1.0 ? 1.0 : -1.0;
  vec2 r = vec2(f.x + f.y * flip, f.y - f.x * flip) * 0.7071;
  float wq = w * 20.0;
  float lozenge = 1.0 - smoothstep(0.045, 0.085 + wq, abs(r.y) + abs(r.x) * 0.24);
  h += 0.0013 * lozenge * (0.04 / (0.04 + wq)) * (1.0 - smoothstep(0.012, 0.025, w));
#endif
  if (P.z > 0.02) h -= P.z * 0.028 * shFbm(uv * 2.6 + fract(P.w) * 40.0);
  return h;
}
`;

/** `stripe`: the livery's stripe colour (linear RGB), painted on PAINT.stripe panels. */
export function panelMaterial(style: PanelStyle, csm: CSM | null, params: THREE.MeshStandardMaterialParameters, stripe: [number, number, number] = [0.46, 0.15, 0.02]) {
  // the livery's stripe is a uniform: every livery shares one program per panel style
  const stripeU = { value: new THREE.Vector3(stripe[0], stripe[1], stripe[2]) };
  const stripeGlsl = 'uShipStripe';
  const mat = new THREE.MeshStandardMaterial(params);
  mat.name = `ship-${style}`;
  if (csm) csm.setupMaterial(mat);
  mat.defines = { ...(mat.defines ?? {}), SHIP_STYLE: STYLE_ID[style] };
  const csmHook = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    csmHook?.call(mat, shader, renderer);
    shader.uniforms.uShipStripe = stripeU;
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
        // ship space: the dust stays on the hull when it flies (and float32 holds it anywhere)
        vPW = transformed;`,
      );
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', `#include <common>\nuniform vec3 uShipStripe;\nvarying vec4 vPanel; varying float vHeat; varying vec3 vPT; varying vec3 vPB; varying vec2 vPUv; varying vec3 vPW;\n${NOISE}\n${HEIGHT}`)
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
          if (shScheme > 2.5 && vPUv.y < 0.11 && vPUv.y > 0.03) diffuseColor.rgb = ${stripeGlsl} * shTone;
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
          // procedural relief: finite differences in panel space → tilt along the panel axes; the
          // step grows with the pixel so the widened details keep their slope (see HEIGHT)
          float fw = length(fwidth(vPUv)) * 0.7;
          float e = max(0.0015, fw * 0.5);
          float h0 = shHeight(vPUv, vPanel, fw);
          vec2 g = vec2(shHeight(vPUv + vec2(e, 0.0), vPanel, fw) - h0, shHeight(vPUv + vec2(0.0, e), vPanel, fw) - h0) / e;
          normal = normalize(normal - (g.x * vPT + g.y * vPB));
          // and a hint of it in the albedo (seams and hollows a touch darker, raised bits lighter):
          // the pattern still reads where the light hits flat
          // (around the style's mean height: the quilted lining's pads don't brighten the whole wall)
          #if SHIP_STYLE == 1
            const float shMean = 0.0075;
          #else
            const float shMean = 0.0;
          #endif
          diffuseColor.rgb *= clamp(1.0 + (h0 - shMean) * 22.0, 0.8, 1.12);
        }`,
      )
      .replace('#include <emissivemap_fragment>', '#include <emissivemap_fragment>\ntotalEmissiveRadiance += vec3(3.2, 0.95, 0.22) * vHeat * vHeat * (0.25 + shScorch);');
  };
  mat.customProgramCacheKey = () => `ship-panel-${style}-v4`;
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
    // not a perfect mirror: at 0.06 the sun's glint off a side window overflowed the HDR target
    roughness: 0.14,
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
  /**
   * (rgb · intensity, visible) per lamp, in a float texture 256 lamps wide: no uniform array (a
   * big ship ran into WebGL2's minimum of 256 vertex uniform vectors) and one program for every
   * ship whatever its lamp count.
   */
  private data: Float32Array;
  private tex: THREE.DataTexture;

  constructor(count: number) {
    const w = LAMP_TEX_W;
    const h = Math.max(1, Math.ceil(count / w));
    const data = new Float32Array(w * h * 4);
    for (let i = 0; i < count; i++) data[i * 4 + 3] = 1;
    const tex = new THREE.DataTexture(data, w, h, THREE.RGBAFormat, THREE.FloatType);
    tex.minFilter = tex.magFilter = THREE.NearestFilter;
    tex.generateMipmaps = false;
    tex.needsUpdate = true;
    super({
      uniforms: { uLampTex: { value: tex } },
      vertexShader: /* glsl */ `
        attribute float aLamp;
        uniform highp sampler2D uLampTex;
        varying vec3 vCol;
        varying vec3 vN;
        varying vec3 vV;
        #include <common>
        #include <logdepthbuf_pars_vertex>
        void main() {
          int li = int(aLamp + 0.5);
          vec4 L = texelFetch(uLampTex, ivec2(li % ${w}, li / ${w}), 0);
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
    this.data = data;
    this.tex = tex;
  }

  /** A lamp's colour × intensity and whether it is there at all (uploaded once a frame if anything changed). */
  set(i: number, r: number, g: number, b: number, visible = true) {
    const d = this.data;
    const o = i * 4;
    const a = visible ? 1 : 0;
    if (d[o] === r && d[o + 1] === g && d[o + 2] === b && d[o + 3] === a) return;
    d[o] = r;
    d[o + 1] = g;
    d[o + 2] = b;
    d[o + 3] = a;
    this.tex.needsUpdate = true;
  }

  isVisible(i: number) {
    return this.data[i * 4 + 3] > 0.5;
  }

  setVisible(i: number, on: boolean) {
    const o = i * 4 + 3;
    const a = on ? 1 : 0;
    if (this.data[o] === a) return;
    this.data[o] = a;
    this.tex.needsUpdate = true;
  }
}

const LAMP_TEX_W = 256;

/**
 * Text on a texture (labels, displays, decals) read from a distance: its mip chain blurs the letters
 * well before they are too small to read. A negative LOD bias samples a sharper level — a little
 * shimmer at grazing angles for text that stays legible a few metres further.
 */
export function sharpText<M extends THREE.Material>(mat: M, bias = -0.8): M {
  const prev = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    prev?.call(mat, shader, renderer);
    shader.fragmentShader = shader.fragmentShader.replace(
      '#include <map_fragment>',
      `#ifdef USE_MAP
        diffuseColor *= texture( map, vMapUv, ${bias.toFixed(2)} );
      #endif`,
    );
  };
  const key = mat.customProgramCacheKey.bind(mat);
  mat.customProgramCacheKey = () => `${key()}|sharp${bias}`;
  return mat;
}
