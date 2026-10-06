import {
  BloomEffect,
  Effect,
  EffectComposer,
  EffectPass,
  NoiseEffect,
  BlendFunction,
  RenderPass,
  SMAAEffect,
  SMAAPreset,
  ToneMappingEffect,
  ToneMappingMode,
  VignetteEffect,
} from 'postprocessing';
import { N8AOPostPass } from 'n8ao';
import * as THREE from 'three';
import { NOTCHES, QualityGovernor, type Notch } from './governor';

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

/**
 * The GPU's time per frame, from a timer query round the whole frame (EXT_disjoint_timer_query_webgl2;
 * without it, none: the governor estimates). Measurement only: what to do with it is the governor's
 * (render/governor.ts).
 */
class GpuTimer {
  private queries: WebGLQuery[] = [];
  private busy = false;
  private done: number[] = [];

  constructor(
    private gl: WebGL2RenderingContext,
    private ext: { TIME_ELAPSED_EXT: number; GPU_DISJOINT_EXT: number },
  ) {}

  begin() {
    if (this.busy || this.queries.length > 8) return;
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

  /** The finished queries' mean (ms), or null if none finished since last time. */
  take(): number | null {
    const gl = this.gl;
    const disjoint = gl.getParameter(this.ext.GPU_DISJOINT_EXT);
    while (this.queries.length) {
      const q = this.queries[0];
      if (!gl.getQueryParameter(q, gl.QUERY_RESULT_AVAILABLE)) break;
      const ns = gl.getQueryParameter(q, gl.QUERY_RESULT) as number;
      if (!disjoint) this.done.push(ns / 1e6);
      gl.deleteQuery(q);
      this.queries.shift();
    }
    if (!this.done.length) return null;
    const ms = this.done.reduce((x, y) => x + y, 0) / this.done.length;
    this.done.length = 0;
    return ms;
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
  /** The last pass (bloom, tone mapping, vignette, grain) and the same without bloom (the governor's). */
  private finalPass: EffectPass;
  private finalNoBloom: EffectPass | null = null;
  private finalEffects: Effect[];
  /** MSAA samples of this profile (the governor may turn them off). */
  private readonly msaa: number;
  /** Cheap post anti-aliasing (SMAA) after tone mapping, whenever there is no MSAA; `?noaa`: none. */
  private aaPass: EffectPass | null;
  /** Pixel ratio before the governor scales it. */
  private basePixelRatio: number;
  private gpuTimer: GpuTimer | null = null;
  /** Keeps the frame rate up by lowering the image when the GPU can't keep up (render/governor.ts). */
  private governor: QualityGovernor | null = null;
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
    // a laptop's screen scaled to 125 % is blurry at 1: the low profile starts near native too (the
    // governor lowers it if the GPU can't keep up)
    this.basePixelRatio = Math.min(window.devicePixelRatio, opts.quality === 'high' ? 1.5 : 1.25);
    renderer.setPixelRatio(this.basePixelRatio);
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFShadowMap;
    renderer.toneMapping = THREE.NoToneMapping; // done in post
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer = renderer;

    this.msaa = opts.quality === 'high' ? 4 : 0;
    const composer = new EffectComposer(renderer, {
      frameBufferType: THREE.HalfFloatType,
      multisampling: this.msaa,
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
    this.finalEffects = [this.toneMapping, vignette, grain];
    this.finalPass = new EffectPass(camera, this.bloom, ...this.finalEffects);
    composer.addPass(this.finalPass);
    this.aaPass = new URLSearchParams(location.search).has('noaa') ? null : new EffectPass(camera, new SMAAEffect({ preset: opts.quality === 'high' ? SMAAPreset.MEDIUM : SMAAPreset.LOW }));
    if (this.aaPass && !this.msaa) composer.addPass(this.aaPass);
    this.composer = composer;

    // ?nodynres: the image stays as the profile says, however slow
    if (!new URLSearchParams(location.search).has('nodynres')) {
      const gl = renderer.getContext() as WebGL2RenderingContext;
      const ext = gl.getExtension('EXT_disjoint_timer_query_webgl2') as { TIME_ELAPSED_EXT: number; GPU_DISJOINT_EXT: number } | null;
      if (ext) this.gpuTimer = new GpuTimer(gl, ext);
      // a profile without MSAA starts past that notch
      this.governor = new QualityGovernor({ start: this.msaa ? 0 : 1 });
    }
  }

  /** Current resolution scale (1 = full). */
  get resolutionScale() {
    return this.governor?.current.scale ?? 1;
  }

  /** What the governor has done to the image (F3). */
  get governorState(): string {
    const g = this.governor;
    if (!g) return 'fija (?nodynres)';
    const n = g.current;
    const aa = n.msaa && this.msaa ? 'MSAA' : this.aaPass ? 'SMAA' : 'sin AA';
    return `${g.notch}/${NOTCHES.length - 1} · ${Math.round(n.scale * 100)} % · ${aa} · bloom ${n.bloom ? 'sí' : 'no'}${this.gpuTimer ? '' : ' · GPU estimada'}`;
  }

  /**
   * How the last frame went: its interval (ms, the display's) and the CPU's time in it (ms). The
   * governor decides from it (and the GPU's time, when measured) whether to change the image.
   */
  frameStats(intervalMs: number, cpuMs: number) {
    const n = this.governor?.frame(intervalMs, cpuMs, this.gpuTimer?.take() ?? null);
    if (n) this.apply(n);
  }

  /** A notch of the governor: resolution before the next frame is drawn, MSAA and bloom now. */
  private apply(n: Notch) {
    this.pendingScale = n.scale;
    const samples = n.msaa ? this.msaa : 0;
    if (this.composer.multisampling !== samples) this.composer.multisampling = samples;
    const want = n.bloom ? this.finalPass : (this.finalNoBloom ??= new EffectPass(this.camera, ...this.finalEffects));
    const aa = samples === 0 ? this.aaPass : null;
    const passes = this.composer.passes;
    const now = passes.find((p) => p === this.finalPass || p === this.finalNoBloom);
    const hasAa = !!this.aaPass && passes.includes(this.aaPass);
    if (now === want && hasAa === !!aa) return;
    // the tail again, in order (the composer draws its last pass to the screen)
    if (this.aaPass && hasAa) this.composer.removePass(this.aaPass);
    if (now) this.composer.removePass(now);
    this.composer.addPass(want);
    if (aa) this.composer.addPass(aa);
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
    const timer = this.gpuTimer;
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
    timer?.begin();
    this.beforeRender?.();
    this.composer.render(dt);
    timer?.end();
  }

  dispose() {
    this.composer.dispose();
    this.renderer.dispose();
    void this.scene;
    void this.ao;
  }

  /** Auxiliary views draw inside the frame's GPU timer and draw-call accounting. */
  beforeRender: (() => void) | null = null;
}

const _size = new THREE.Vector2();
