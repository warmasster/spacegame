// Reusable display contracts: no renderer, weapon or host implementation here.
export interface ConsoleCommand {
  namespace: string;
  target: string;
  action: string;
}

export interface CameraDisplayDef {
  /** Provider registry kind and host-relative reference (mount, mirror, fixed camera…). */
  source: { kind: string; ref: string };
  width?: number;
  height?: number;
  fps?: number;
  reach?: number;
  /** Optical stops, independent of image resolution or renderer budget. */
  zoom?: readonly number[];
  /** Image processing recipes: independent of camera kind and host. Empty = clean image. */
  effects?: readonly ImageEffectSpec[];
}

export interface ImageEffectSpec { kind: string; amount?: number }
export const CAMERA_ZOOM: readonly number[] = [1, 1.5, 3];

/** Optical magnification keeps the perspective projection's focal length proportional to zoom. */
export function cameraFov(base: number, zoom: number) {
  return 360 / Math.PI * Math.atan(Math.tan(base * Math.PI / 360) / Math.max(1, zoom));
}

export interface CameraBudget { width: number; height: number; fps: number; reach: number }

/** Limits apply to every provider. A broken or ambitious catalog cannot bypass the render budget. */
export function cameraBudget(def: CameraDisplayDef): CameraBudget {
  const bounded = (n: number | undefined, fallback: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, Number.isFinite(n) ? n! : fallback));
  return {
    width: Math.round(bounded(def.width, 256, 64, 384)),
    height: Math.round(bounded(def.height, 144, 36, 216)),
    fps: bounded(def.fps, 12, 1, 15),
    reach: bounded(def.reach, 10, 1, 16),
  };
}
