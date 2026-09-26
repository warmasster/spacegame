import * as THREE from 'three';
import { CSM } from 'three/addons/csm/CSM.js';

/** Sun irradiance in scene units (tone mapping + exposure bring it to display range). */
export const SUN_INTENSITY = 5.2;

/** Direction from azimuth (from north, clockwise toward east) and elevation, in degrees. North = -Z, east = +X. */
export function dirFromAzEl(azDeg: number, elDeg: number) {
  const az = THREE.MathUtils.degToRad(azDeg);
  const el = THREE.MathUtils.degToRad(elDeg);
  return new THREE.Vector3(Math.sin(az) * Math.cos(el), Math.sin(el), -Math.cos(az) * Math.cos(el)).normalize();
}

/**
 * Harsh vacuum lighting: one sun with cascaded shadows (sharp, near-black shadows), no sky
 * light, and an environment made of the sunlit ground (the only real "fill" on the Moon).
 */
export class Lighting {
  readonly csm: CSM;
  readonly envMap: THREE.Texture;

  constructor(
    scene: THREE.Scene,
    camera: THREE.PerspectiveCamera,
    renderer: THREE.WebGLRenderer,
    readonly sunDir: THREE.Vector3,
    earthDir: THREE.Vector3,
    quality: 'high' | 'low',
  ) {
    this.csm = new CSM({
      camera,
      parent: scene,
      cascades: quality === 'high' ? 4 : 3,
      maxFar: 520,
      mode: 'practical',
      shadowMapSize: quality === 'high' ? 2048 : 1024,
      lightDirection: sunDir.clone().negate(),
      lightIntensity: SUN_INTENSITY,
      lightNear: 1,
      lightFar: 6000,
      lightMargin: 400,
      shadowBias: -0.00025,
    });
    this.csm.fade = true;
    for (const l of this.csm.lights) {
      l.color.setRGB(1.0, 0.985, 0.96);
      l.shadow.normalBias = 0.02;
      l.shadow.camera.layers.enableAll();
    }

    // Environment: black sky, faint earthshine, bright regolith below the horizon.
    this.envMap = buildEnvironment(renderer, sunDir, earthDir);
    scene.environment = this.envMap;
    scene.environmentIntensity = 1;
  }

  update() {
    this.csm.update();
  }

  dispose() {
    this.csm.dispose();
    this.envMap.dispose();
  }
}

function buildEnvironment(renderer: THREE.WebGLRenderer, sunDir: THREE.Vector3, earthDir: THREE.Vector3) {
  const envScene = new THREE.Scene();
  const mat = new THREE.ShaderMaterial({
    side: THREE.BackSide,
    uniforms: {
      uSun: { value: sunDir },
      uEarth: { value: earthDir },
    },
    vertexShader: /* glsl */ `
      varying vec3 vDir;
      void main() { vDir = normalize(position); gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
    fragmentShader: /* glsl */ `
      uniform vec3 uSun;
      uniform vec3 uEarth;
      varying vec3 vDir;
      void main() {
        vec3 d = normalize(vDir);
        // sunlit regolith seen from above: brighter looking away from the sun (backscatter)
        float below = smoothstep(0.02, -0.06, d.y);
        vec2 hs = normalize(uSun.xz + 1e-5);
        float away = 0.5 - 0.5 * dot(normalize(d.xz + 1e-5), hs);
        float ground = (0.05 + 0.035 * away) * max(uSun.y, 0.05) / 0.3;
        // horizon band slightly brighter (grazing Lommel–Seeliger)
        ground *= 1.0 + 0.6 * smoothstep(-0.35, 0.0, d.y);
        vec3 col = vec3(ground) * vec3(1.0, 0.99, 0.965) * below;
        // black sky + Earth (earthshine is tiny in daylight)
        col += vec3(0.0006, 0.0008, 0.0012) * (1.0 - below);
        col += vec3(0.35, 0.5, 0.9) * 0.6 * smoothstep(0.9993, 0.9998, dot(d, uEarth));
        gl_FragColor = vec4(col, 1.0);
      }`,
  });
  envScene.add(new THREE.Mesh(new THREE.SphereGeometry(10, 64, 32), mat));
  const pmrem = new THREE.PMREMGenerator(renderer);
  const rt = pmrem.fromScene(envScene, 0, 0.1, 100);
  pmrem.dispose();
  mat.dispose();
  return rt.texture;
}
