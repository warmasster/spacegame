import {
  BloomEffect,
  Effect,
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

/**
 * Brightest value a pixel may carry into the post chain. A mirror-smooth surface catching the sun
 * can shade far above what a half-float target holds (65504): the pixel becomes Inf, the bloom's
 * mip chain spreads it and the tone mapper turns it into NaN — the whole screen blinks black. Real
 * highlights stay far below this, so clamping here changes nothing you can see.
 */
const HDR_MAX = 4096;

/**
 * Clamps the scene's HDR colour and drops NaN / Inf before anything blurs it (see HDR_MAX): a NaN
 * that reaches the bloom's mip chain blacks out whole screen-aligned blocks. The test reads the
 * float's bits: on Windows (ANGLE → Direct3D) the shader compiler may fold `isnan()` away and
 * `clamp()` of a NaN is not guaranteed to be finite, but integer bit tests always survive.
 */
class SanitizeEffect extends Effect {
  /** `show`: paint the bad pixels magenta instead of hiding them (URL `?nan`, to find their source). */
  constructor(show = false) {
    super(
      'SanitizeEffect',
      /* glsl */ `
      bool bad(float x) { highp uint b = floatBitsToUint(x); return (b & 0x7f800000u) == 0x7f800000u; }
      // finite → clamped to [0, hi]; +Inf → hi; NaN or −Inf → 0
      float sane(float x, float hi) {
        highp uint b = floatBitsToUint(x);
        if ((b & 0x7f800000u) != 0x7f800000u) return min(max(x, 0.0), hi);
        return (b & 0x807fffffu) == 0u ? hi : 0.0;
      }
      void mainImage(const in vec4 inputColor, const in vec2 uv, out vec4 outputColor) {
        const float HI = ${HDR_MAX.toFixed(1)};
        outputColor = vec4(sane(inputColor.r, HI), sane(inputColor.g, HI), sane(inputColor.b, HI), sane(inputColor.a, 1.0));
        ${show ? 'if (bad(inputColor.r) || bad(inputColor.g) || bad(inputColor.b) || bad(inputColor.a)) outputColor = vec4(3.0, 0.0, 3.0, 1.0);' : ''}
      }`,
    );
  }
}

export interface PipelineOptions {
  canvas: HTMLCanvasElement;
  quality: 'high' | 'low';
}

/** Lowest resolution scale dynamic resolution goes to, and its steps. */
const DYN_MIN = 0.7;
const DYN_DOWN = 0.1;
const DYN_UP = 0.05;
/** Seconds between two changes (each one reallocates the post chain's buffers). */
const DYN_EVERY = 3;
/** Seconds after start before it may change anything (shader compiles and terrain streaming are not the steady load). */
const DYN_WARMUP = 8;

/**
 * Dynamic resolution driven by the GPU's own frame time (a timer query around the whole frame):
 * over budget → a lower pixel ratio, well under → back up. The CPU's time never lowers the image
 * (a CPU-bound frame gains nothing from fewer pixels). Without EXT_disjoint_timer_query_webgl2 it
 * stays off; `?nodynres` turns it off.
 */
class DynamicResolution {
  private queries: WebGLQuery[] = [];
  private busy = false;
  private gpuMs: number[] = [];
  private frameMs: number[] = [];
  private last = performance.now() + DYN_WARMUP * 1000;
  /** Consecutive polls over budget: one slow burst (a streaming spike) doesn't lower the image. */
  private over = 0;
  scale = 1;

  constructor(
    private gl: WebGL2RenderingContext,
    private ext: { TIME_ELAPSED_EXT: number; GPU_DISJOINT_EXT: number },
  ) {}

  begin() {
    if (this.busy) return;
    const q = this.gl.createQuery();
    if (!q) return;
    this.gl.beginQuery(this.ext.TIME_ELAPSED_EXT, q);
    this.queries.push(q);
    this.busy = true;
  }

  end() {
    if (!this.busy) return;
    this.gl.endQuery(this.ext.TIME_ELAPSED_EXT);
    this.busy = false;
  }

  /** Collect the finished queries; returns a new scale when it is time to change, else null. */
  poll(dtMs: number): number | null {
    const gl = this.gl;
    this.frameMs.push(dtMs);
    if (this.frameMs.length > 120) this.frameMs.shift();
    const disjoint = gl.getParameter(this.ext.GPU_DISJOINT_EXT);
    while (this.queries.length) {
      const q = this.queries[0];
      if (!gl.getQueryParameter(q, gl.QUERY_RESULT_AVAILABLE)) break;
      const ns = gl.getQueryParameter(q, gl.QUERY_RESULT) as number;
      if (!disjoint) this.gpuMs.push(ns / 1e6);
      if (this.gpuMs.length > 90) this.gpuMs.shift();
      gl.deleteQuery(q);
      this.queries.shift();
    }
    const now = performance.now();
    if (now - this.last < DYN_EVERY * 1000 || this.gpuMs.length < 30) return null;
    this.last = now;
    // the frame the display allows: the quickest recent frames (vsync), capped at 60 Hz's
    const sorted = [...this.frameMs].sort((a, b) => a - b);
    const interval = Math.min(1000 / 60, sorted[Math.floor(sorted.length * 0.1)] ?? 1000 / 60);
    const budget = interval * 0.85;
    const gpu = this.gpuMs.reduce((a, b) => a + b, 0) / this.gpuMs.length;
    let next = this.scale;
    this.over = gpu > budget ? this.over + 1 : 0;
    if (this.over >= 2 && this.scale > DYN_MIN) next = Math.max(DYN_MIN, this.scale - DYN_DOWN);
    else if (gpu < budget * 0.6 && this.scale < 1) next = Math.min(1, this.scale + DYN_UP);
    if (next === this.scale) return null;
    this.scale = next;
    this.gpuMs.length = 0;
    return next;
  }
}

/**
 * Renderer + post chain: MSAA → ambient occlusion → HDR clamp → bloom → AgX tone mapping → vignette/grain.
 * Scene values are HDR (physically based lights), the tone mapper produces the final image.
 */
export class RenderPipeline {
  readonly renderer: THREE.WebGLRenderer;
  readonly composer: EffectComposer;
  private ao: N8AOPostPass | null = null;
  private bloom: BloomEffect;
  private toneMapping: ToneMappingEffect;
  /** Pixel ratio before dynamic resolution scales it. */
  private basePixelRatio: number;
  private dyn: DynamicResolution | null = null;
  /** Called when the pixel ratio changes (point sprites and stars size themselves with it). */
  onPixelRatio?: (pr: number) => void;

  constructor(
    private scene: THREE.Scene,
    private camera: THREE.PerspectiveCamera,
    opts: PipelineOptions,
  ) {
    // ?rdepth: reversed-Z (keeps early-Z, which a logarithmic depth buffer turns off) — needs
    // EXT_clip_control; three falls back to the plain buffer without it. Opt-in until it's been
    // tried with the shadows and the post chain on real GPUs.
    const reversed = new URLSearchParams(location.search).has('rdepth');
    const renderer = new THREE.WebGLRenderer({
      canvas: opts.canvas,
      antialias: false,
      stencil: false,
      depth: true,
      powerPreference: 'high-performance',
      logarithmicDepthBuffer: !reversed,
      reversedDepthBuffer: reversed,
    });
    // the post chain calls render() several times a frame: count the whole frame, not the last pass
    renderer.info.autoReset = false;
    this.basePixelRatio = Math.min(window.devicePixelRatio, opts.quality === 'high' ? 1.5 : 1);
    renderer.setPixelRatio(this.basePixelRatio);
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

    // before the bloom reads the frame: one overflowing highlight must not black out the screen
    composer.addPass(new EffectPass(camera, new SanitizeEffect(new URLSearchParams(location.search).has('nan'))));

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

    if (!new URLSearchParams(location.search).has('nodynres')) {
      const gl = renderer.getContext() as WebGL2RenderingContext;
      const ext = gl.getExtension('EXT_disjoint_timer_query_webgl2') as { TIME_ELAPSED_EXT: number; GPU_DISJOINT_EXT: number } | null;
      if (ext) this.dyn = new DynamicResolution(gl, ext);
    }
  }

  /** Current dynamic-resolution scale (1 = full). */
  get resolutionScale() {
    return this.dyn?.scale ?? 1;
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

  /** A resolution change waiting for the next frame (see render). */
  private pendingScale: number | null = null;

  render(dt: number) {
    this.renderer.info.reset();
    const dyn = this.dyn;
    // A new pixel ratio resizes the canvas, and a resized canvas is blank until drawn again: it
    // is applied right before drawing, never after (after, the browser would show that blank
    // frame — a black flash).
    if (this.pendingScale !== null) {
      const pr = this.basePixelRatio * this.pendingScale;
      this.pendingScale = null;
      const size = this.renderer.getSize(_size);
      this.renderer.setPixelRatio(pr);
      this.composer.setSize(size.x, size.y, false);
      this.onPixelRatio?.(pr);
    }
    dyn?.begin();
    this.composer.render(dt);
    dyn?.end();
    const next = dyn?.poll(dt * 1000);
    if (next != null) this.pendingScale = next;
  }

  dispose() {
    this.composer.dispose();
    this.renderer.dispose();
    void this.scene;
    void this.ao;
  }
}

const _size = new THREE.Vector2();
