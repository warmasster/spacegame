// Reusable image processing on any textured mesh: one texture fetch, no extra render pass.
import * as THREE from 'three';
import type { ImageEffectSpec } from '../../shared/screens';

export interface ImageEffect {
  /** GLSL on imageUv, imageTime and imageSize, before the texture fetch. */
  uv?(amount: string, index: number): string;
  /** GLSL on diffuseColor.rgb after the fetch. Use unique names (index) for local variables. */
  color?(amount: string, index: number): string;
}

const effects = new Map<string, ImageEffect>();
export function defineImageEffect(kind: string, effect: ImageEffect) {
  if (effects.has(kind)) throw new Error(`image effect already registered: ${kind}`);
  effects.set(kind, effect);
}

/** Shader assembled once from recipes. A quality upgrade changes data, not a host's renderer. */
export class ImageMaterial extends THREE.MeshBasicMaterial {
  private clock = { value: 0 };
  private activeImage = { value: 1 };
  private size: { value: THREE.Vector2 };
  private program: string;
  constructor(parameters: THREE.MeshBasicMaterialParameters, recipes: readonly ImageEffectSpec[] = [], width = 256, height = 144) {
    super(parameters);
    this.size = { value: new THREE.Vector2(width, height) };
    let uv = '', color = '', key = 'image-v1';
    for (let i = 0; i < recipes.length; i++) {
      const recipe = recipes[i], effect = effects.get(recipe.kind);
      if (!effect) throw new Error(`image effect not registered: ${recipe.kind}`);
      const amount = Math.max(0, Math.min(1, Number.isFinite(recipe.amount) ? recipe.amount! : 0.2)).toFixed(5);
      uv += effect.uv?.(amount, i) ?? '';
      color += effect.color?.(amount, i) ?? '';
      key += '/' + recipe.kind + ':' + amount;
    }
    this.program = key;
    if (recipes.length) this.onBeforeCompile = shader => {
      shader.uniforms.imageTime = this.clock;
      shader.uniforms.imageSize = this.size;
      shader.uniforms.imageActive = this.activeImage;
      shader.fragmentShader = shader.fragmentShader.replace('#include <common>', '#include <common>\nuniform float imageTime, imageActive;\nuniform vec2 imageSize;');
      // Keep Three's texture/color handling and extend its basic material, rather than duplicating it.
      const map = THREE.ShaderChunk.map_fragment.replaceAll('vMapUv', 'imageUv');
      shader.fragmentShader = shader.fragmentShader.replace('#include <map_fragment>', '#ifdef USE_MAP\nvec2 imageUv = vMapUv;\n' + uv + '\n#endif\n' + map + '\n#ifdef USE_MAP\n' + color + '\ndiffuseColor.rgb *= imageActive;\n#endif');
    };
  }
  time(seconds: number) { this.clock.value = seconds; }
  active(on: boolean) { this.activeImage.value = on ? 1 : 0; }
  override customProgramCacheKey() { return this.program; }
}

// Restrained analog feed: tiny horizontal instability, scan lines, grain and desaturation.
defineImageEffect('vhs', {
  uv: amount => `imageUv.x = clamp(imageUv.x + ${amount} * 0.003 * sin(imageUv.y * 130.0 + imageTime * 9.0), 0.001, 0.999);\n`,
  color: (amount, i) => `
    float imageGrain${i} = fract(sin(dot(floor(imageUv * imageSize), vec2(12.9898,78.233)) + floor(imageTime * 12.0)) * 43758.5453) - 0.5;
    diffuseColor.rgb = mix(diffuseColor.rgb, vec3(dot(diffuseColor.rgb, vec3(0.2126,0.7152,0.0722))), ${amount} * 0.2);
    diffuseColor.rgb *= 1.0 - ${amount} * 0.13 * (0.5 + 0.5 * sin(imageUv.y * imageSize.y * 3.14159265));
    diffuseColor.rgb = max(vec3(0.0), diffuseColor.rgb + imageGrain${i} * ${amount} * 0.05);\n`,
});
