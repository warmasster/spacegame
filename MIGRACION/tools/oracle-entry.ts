// Execute the ORIGINAL TS as an independent reference, never reimplement its formula here.
import { BodySurface, MOON_SURFACE, surfaceSample } from '../../src/shared/space/surface.js';
import { MOON_BODY, bareSurface, surfaceOf } from '../../src/shared/space/body.js';
import { cubeDir, paramsOn, facePoint } from '../../src/shared/space/cubeSphere.js';
import { Noise3, hash2i, hash3i, mulberry32, craterProfile } from '../../src/shared/noise.js';
export { BodySurface, MOON_SURFACE, surfaceSample, MOON_BODY, bareSurface, surfaceOf,
  cubeDir, paramsOn, facePoint, Noise3, hash2i, hash3i, mulberry32, craterProfile };
