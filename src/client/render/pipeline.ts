import {
  BloomEffect,
  EffectComposer,
  EffectPass,
  NoiseEffect,
  BlendFunction,
  RenderPass,
  ToneMappingEffect,
  ToneMappingMode,
  VignetteEffect,
} from 'postprocessing';
import { N8AOPostPass } from 'n8ao';
import * as THREE from 'three';

export interface PipelineOptions {
  canvas: HTMLCanvasElement;
  quality: 'high' | 'low';
}

/**
 * Renderer + post chain: MSAA → ambient occlusion → bloom → AgX tone mapping → vignette/grain.
 * Scene values are HDR (physically based lights), the tone mapper produces the final image.
 */
export class RenderPipeline {
  readonly renderer: THREE.WebGLRenderer;
  readonly composer: EffectComposer;
  private ao: N8AOPostPass | null = null;
  private bloom: BloomEffect;
  private toneMapping: ToneMappingEffect;

  constructor(
    private scene: THREE.Scene,
    private camera: THREE.PerspectiveCamera,
    opts: PipelineOptions,
  ) {
    const renderer = new THREE.WebGLRenderer({
      canvas: opts.canvas,
      antialias: false,
      stencil: false,
      depth: true,
      powerPreference: 'high-performance',
      logarithmicDepthBuffer: true,
    });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, opts.quality === 'high' ? 1.5 : 1));
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFShadowMap;
    renderer.toneMapping = THREE.NoToneMapping; // done in post
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer = renderer;

    const composer = new EffectComposer(renderer, {
      frameBufferType: THREE.HalfFloatType,
      multisampling: opts.quality === 'high' ? 4 : 0,
    });
    composer.addPass(new RenderPass(scene, camera));

    // Screen-space AO produced banding at LOD joins and blocky patches on distant slopes on real
    // GPUs (half-res + log depth); under a hard vacuum sun it adds little. Opt-in with ?ao.
    if (opts.quality === 'high' && new URLSearchParams(location.search).has('ao')) {
      const ao = new N8AOPostPass(scene, camera, 1, 1);
      ao.configuration.aoRadius = 0.9;
      ao.configuration.distanceFalloff = 0.6;
      ao.configuration.intensity = 1.6;
      ao.configuration.halfRes = true;
      ao.configuration.depthAwareUpsampling = true;
      ao.configuration.gammaCorrection = false;
      ao.configuration.aoSamples = 12;
      ao.configuration.denoiseSamples = 6;
      composer.addPass(ao);
      this.ao = ao;
    }

    this.bloom = new BloomEffect({
      mipmapBlur: true,
      luminanceThreshold: 1.2,
      luminanceSmoothing: 0.35,
      intensity: 0.55,
      radius: 0.7,
    });
    this.toneMapping = new ToneMappingEffect({ mode: ToneMappingMode.AGX });
    const vignette = new VignetteEffect({ offset: 0.32, darkness: 0.42 });
    const grain = new NoiseEffect({ blendFunction: BlendFunction.OVERLAY, premultiply: true });
    grain.blendMode.opacity.value = 0.12;
    composer.addPass(new EffectPass(camera, this.bloom, this.toneMapping, vignette, grain));
    this.composer = composer;
  }

  /** Scene exposure (applied before tone mapping). */
  set exposure(v: number) {
    this.renderer.toneMappingExposure = v;
  }

  resize(width: number, height: number) {
    this.renderer.setSize(width, height, false);
    this.composer.setSize(width, height, false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
  }

  render(dt: number) {
    this.composer.render(dt);
  }

  dispose() {
    this.composer.dispose();
    this.renderer.dispose();
    void this.scene;
    void this.ao;
  }
}
