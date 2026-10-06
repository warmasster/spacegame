// What graphics this machine has (docs/RENDIMIENTO.md): the GPU's name as WebGL reports it, and
// whether it is integrated (it shares the CPU's memory and power: most laptops) or software. The
// graphics profile a player gets by default follows it; the CPU's thread count alone says nothing
// about the GPU (a 12-thread laptop CPU usually comes with integrated graphics).

export interface GpuInfo {
  /** As the browser names it (ANGLE's string on Windows), '' if it won't say. */
  renderer: string;
  integrated: boolean;
  software: boolean;
}

const SOFTWARE = /swiftshader|llvmpipe|softpipe|microsoft basic render|software/i;
// integrated: Intel's (not Arc), AMD's APUs ("Radeon(TM) Graphics", "Vega 8"), phones', Apple's
const INTEGRATED = /intel(?!.*\barc\b)|\buhd\b|\biris\b|hd graphics|radeon\(tm\) graphics|radeon graphics|\bvega \d+\b|mali|adreno|powervr|apple gpu|apple m\d/i;

/** Classifies a renderer string (pure: tests call it). */
export function classifyGpu(renderer: string): GpuInfo {
  return { renderer, software: SOFTWARE.test(renderer), integrated: INTEGRATED.test(renderer) };
}

let cached: GpuInfo | null = null;

/** This machine's GPU (asked once, with a throwaway WebGL context). */
export function gpuInfo(): GpuInfo {
  if (cached) return cached;
  let renderer = '';
  try {
    const canvas = document.createElement('canvas');
    const gl = (canvas.getContext('webgl2') ?? canvas.getContext('webgl')) as WebGLRenderingContext | null;
    if (gl) {
      const dbg = gl.getExtension('WEBGL_debug_renderer_info');
      renderer = String(gl.getParameter(dbg ? dbg.UNMASKED_RENDERER_WEBGL : gl.RENDERER) ?? '');
      gl.getExtension('WEBGL_lose_context')?.loseContext();
    }
  } catch {
    // no WebGL here: the game will say so itself
  }
  cached = classifyGpu(renderer);
  return cached;
}

/** The profile a player gets unless they pick one: low on integrated or software graphics, or few cores. */
export function suggestedQuality(gpu: GpuInfo = gpuInfo(), cores = navigator.hardwareConcurrency ?? 8): 'high' | 'low' {
  return gpu.software || gpu.integrated || cores <= 4 ? 'low' : 'high';
}
