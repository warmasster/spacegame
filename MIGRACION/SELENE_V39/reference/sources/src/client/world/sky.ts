import * as THREE from 'three';

/**
 * Everything infinitely far away: black lunar sky, Milky Way, real star catalogue, the Sun
 * and the Earth. Drawn first (renderOrder), without depth, re-centred on the camera.
 */

export interface SkyConfig {
  /** Unit vector toward the Sun (world). */
  sunDir: THREE.Vector3;
  /** Unit vector toward the Earth (world). */
  earthDir: THREE.Vector3;
  /** Rotation from the equatorial (J2000) frame to the local world frame. */
  celestial: THREE.Matrix3;
}

const SKY_DIST = 1000;
const EARTH_ANGULAR_DIAMETER = THREE.MathUtils.degToRad(1.9);

export class Sky {
  readonly group = new THREE.Group();
  private skyMat: THREE.ShaderMaterial;
  private starMat: THREE.ShaderMaterial;
  private earth: THREE.Mesh;
  private earthMat: THREE.ShaderMaterial;
  private halo: THREE.Mesh;
  /** One reusable sky quad, hidden between infrequent distant comet appearances. */
  readonly comet: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>;
  private nextComet = -1;
  private cometStart = -1;
  private cometLife = 0;
  private cometBase = new THREE.Vector3();
  private cometDrift = new THREE.Vector3();

  constructor(
    private cfg: SkyConfig,
    loader: THREE.TextureLoader,
    stars: Float32Array,
  ) {
    this.group.name = 'Sky';
    this.group.renderOrder = -1000;

    // --- background sphere: black + Milky Way + Sun disk/glare ---------------------------
    const mw = loader.load('/assets/sky/milkyway.jpg');
    mw.colorSpace = THREE.NoColorSpace;
    mw.generateMipmaps = true;
    mw.minFilter = THREE.LinearMipmapLinearFilter;
    mw.wrapS = THREE.RepeatWrapping;
    this.skyMat = new THREE.ShaderMaterial({
      uniforms: {
        uMilkyWay: { value: mw },
        uCelestialInv: { value: new THREE.Matrix3().copy(cfg.celestial).transpose() },
        uSunDir: { value: cfg.sunDir },
        uMilkyWayIntensity: { value: 0.028 },
        uSunIntensity: { value: 90.0 },
      },
      vertexShader: /* glsl */ `
        varying vec3 vDir;
        void main() {
          vDir = position;
          vec4 p = projectionMatrix * vec4(mat3(viewMatrix) * position, 1.0);
          gl_Position = p.xyww;
        }`,
      fragmentShader: /* glsl */ `
        uniform sampler2D uMilkyWay;
        uniform mat3 uCelestialInv;
        uniform vec3 uSunDir;
        uniform float uMilkyWayIntensity;
        uniform float uSunIntensity;
        varying vec3 vDir;
        const float PI = 3.141592653589793;
        void main() {
          vec3 d = normalize(vDir);
          vec3 e = uCelestialInv * d;                 // equatorial frame
          float ra = atan(e.y, e.x);                   // -PI..PI
          float dec = asin(clamp(e.z, -1.0, 1.0));
          vec2 uv = vec2(ra / (2.0 * PI) + 0.5, 0.5 + dec / PI);
          float mw = texture2D(uMilkyWay, uv).r;
          // Dust lanes and a warm stellar core sit behind the resolved catalogue stars.
          // Angular coordinates cover the whole sphere, independent of terrain size or origin.
          vec3 dustColor = mix(vec3(0.70, 0.78, 1.0), vec3(1.05, 0.94, 0.79), mw);
          vec3 col = vec3(0.00022, 0.00025, 0.00034) + uMilkyWayIntensity * mw * mw * dustColor;
          // Sun: 0.53 deg disk with limb darkening + optical glare from the visor
          float c = dot(d, uSunDir);
          float ang = acos(clamp(c, -1.0, 1.0));
          float r = ang / 0.004654;                    // 1.0 at the solar limb
          if (r < 1.0) {
            float mu = sqrt(1.0 - r * r);
            col += uSunIntensity * vec3(1.0, 0.98, 0.95) * (0.3 + 0.7 * mu);
          }
          col += vec3(1.0, 0.97, 0.92) * (0.35 * exp(-ang * 160.0) + 0.012 * exp(-ang * 14.0));
          gl_FragColor = vec4(col, 1.0);
        }`,
      depthTest: false,
      depthWrite: false,
      side: THREE.BackSide,
    });
    const sphere = new THREE.Mesh(new THREE.SphereGeometry(SKY_DIST * 1.2, 64, 32), this.skyMat);
    sphere.frustumCulled = false;
    sphere.renderOrder = -1003;
    this.group.add(sphere);

    // --- stars ------------------------------------------------------------------------------
    const count = stars.length / 5;
    const pos = new Float32Array(count * 3);
    const col = new Float32Array(count * 3);
    const size = new Float32Array(count);
    const v = new THREE.Vector3();
    for (let i = 0; i < count; i++) {
      v.set(stars[i * 5], stars[i * 5 + 1], stars[i * 5 + 2]).applyMatrix3(cfg.celestial).multiplyScalar(SKY_DIST);
      pos.set([v.x, v.y, v.z], i * 3);
      const mag = stars[i * 5 + 3];
      const rgb = bvToRgb(stars[i * 5 + 4]);
      // flux relative to a mag-0 star, compressed for display
      const flux = Math.pow(10, -0.4 * mag);
      const b = Math.min(4.5, Math.pow(flux, 0.62) * 1.25);
      col.set([rgb[0] * b, rgb[1] * b, rgb[2] * b], i * 3);
      size[i] = 1.2 + Math.min(3.4, Math.pow(flux, 0.33) * 2.2);
    }
    const sg = new THREE.BufferGeometry();
    sg.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    sg.setAttribute('color', new THREE.BufferAttribute(col, 3));
    sg.setAttribute('size', new THREE.BufferAttribute(size, 1));
    this.starMat = new THREE.ShaderMaterial({
      uniforms: { uPixelRatio: { value: 1 }, uIntensity: { value: 0.55 } },
      vertexShader: /* glsl */ `
        attribute vec3 color;
        attribute float size;
        uniform float uPixelRatio;
        varying vec3 vColor;
        void main() {
          vColor = color;
          vec4 p = projectionMatrix * vec4(mat3(viewMatrix) * position, 1.0);
          gl_Position = p.xyww;
          gl_PointSize = size * uPixelRatio;
        }`,
      fragmentShader: /* glsl */ `
        uniform float uIntensity;
        varying vec3 vColor;
        void main() {
          vec2 c = gl_PointCoord * 2.0 - 1.0;
          float r2 = dot(c, c);
          float a = exp(-r2 * 4.0);
          if (a < 0.01) discard;
          gl_FragColor = vec4(vColor * a * uIntensity, 1.0);
        }`,
      blending: THREE.AdditiveBlending,
      depthTest: false,
      depthWrite: false,
    });
    const points = new THREE.Points(sg, this.starMat);
    points.frustumCulled = false;
    points.renderOrder = -1002;
    this.group.add(points);

    const cometMat = new THREE.ShaderMaterial({
      uniforms: {
        uDir: { value: new THREE.Vector3() },
        uTail: { value: new THREE.Vector3() },
        uSide: { value: new THREE.Vector3() },
        uLength: { value: 0.028 },
        uWidth: { value: 0.006 },
        uFade: { value: 0 },
      },
      vertexShader: /* glsl */ `
        uniform vec3 uDir, uTail, uSide;
        uniform float uLength, uWidth;
        varying vec2 vComet;
        void main() {
          vComet = uv;
          vec3 d = normalize(uDir + uTail * (uv.x - 0.12) * uLength + uSide * (uv.y - 0.5) * uWidth);
          vec4 p = projectionMatrix * vec4(mat3(viewMatrix) * d * 1000.0, 1.0);
          gl_Position = p.xyww;
        }`,
      fragmentShader: /* glsl */ `
        uniform float uFade;
        varying vec2 vComet;
        void main() {
          float x = vComet.x - 0.12, y = vComet.y - 0.5;
          float head = exp(-pow(x / 0.06, 2.0) - pow(y / 0.13, 2.0));
          float behind = smoothstep(-0.025, 0.03, x) * (1.0 - smoothstep(0.55, 0.88, x));
          float dust = exp(-pow((y - 0.12 * x * x) / (0.025 + max(x, 0.0) * 0.17), 2.0)) * exp(-max(x, 0.0) * 3.0);
          float ion = exp(-pow(y / (0.012 + max(x, 0.0) * 0.025), 2.0)) * exp(-max(x, 0.0) * 2.4);
          vec3 col = head * vec3(0.65, 0.72, 0.69) + behind * (dust * vec3(0.14, 0.13, 0.10) + ion * vec3(0.03, 0.055, 0.085));
          gl_FragColor = vec4(col * uFade, 1.0);
        }`,
      blending: THREE.AdditiveBlending,
      depthTest: false,
      depthWrite: false,
      side: THREE.DoubleSide,
    });
    this.comet = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), cometMat);
    this.comet.name = 'Distant comet';
    this.comet.frustumCulled = false;
    this.comet.renderOrder = -1002;
    this.comet.visible = false;
    this.group.add(this.comet);

    // --- Earth ------------------------------------------------------------------------------
    const day = loader.load('/assets/sky/earth_day.jpg');
    day.colorSpace = THREE.SRGBColorSpace;
    const night = loader.load('/assets/sky/earth_night.jpg');
    night.colorSpace = THREE.SRGBColorSpace;
    const clouds = loader.load('/assets/sky/earth_clouds.jpg');
    clouds.colorSpace = THREE.NoColorSpace;
    for (const t of [day, night, clouds]) t.anisotropy = 4;
    this.earthMat = new THREE.ShaderMaterial({
      uniforms: {
        uDay: { value: day },
        uNight: { value: night },
        uClouds: { value: clouds },
        uSunDir: { value: cfg.sunDir },
        uSpin: { value: 0 },
        uSunIntensity: { value: 3.2 },
      },
      vertexShader: /* glsl */ `
        varying vec3 vNormal;
        varying vec2 vUv;
        varying vec3 vViewDir;
        void main() {
          vUv = uv;
          vNormal = normalize(mat3(modelMatrix) * normal);
          vec4 wp = modelMatrix * vec4(position, 1.0);
          vViewDir = normalize(cameraPosition - wp.xyz);
          vec4 p = projectionMatrix * viewMatrix * wp;
          gl_Position = p.xyww;
        }`,
      fragmentShader: /* glsl */ `
        uniform sampler2D uDay, uNight, uClouds;
        uniform vec3 uSunDir;
        uniform float uSpin, uSunIntensity;
        varying vec3 vNormal;
        varying vec2 vUv;
        varying vec3 vViewDir;
        void main() {
          vec2 uv = vec2(vUv.x + uSpin, vUv.y);
          vec3 n = normalize(vNormal);
          float ndl = dot(n, uSunDir);
          float lit = smoothstep(-0.08, 0.25, ndl);
          vec3 dayCol = texture2D(uDay, uv).rgb;
          vec3 cloud = texture2D(uClouds, uv).rgb;        // r: clouds, g: roughness
          float cl = smoothstep(0.15, 0.9, cloud.r);
          vec3 surf = mix(dayCol, vec3(1.0), cl * 0.92);
          vec3 col = surf * max(ndl, 0.0) * uSunIntensity * 0.32;
          // ocean glint
          vec3 h = normalize(uSunDir + normalize(vViewDir));
          float ocean = (1.0 - cloud.g) * (1.0 - cl);
          col += uSunIntensity * ocean * 0.35 * pow(max(dot(n, h), 0.0), 60.0) * lit;
          // city lights on the night side
          vec3 nightCol = texture2D(uNight, uv).rgb;
          col += nightCol * 0.08 * (1.0 - lit) * (1.0 - cl * 0.8);
          // Rayleigh-ish limb brightening
          float rim = pow(1.0 - max(dot(n, normalize(vViewDir)), 0.0), 3.0);
          col += vec3(0.25, 0.45, 1.0) * rim * max(ndl + 0.2, 0.0) * uSunIntensity * 0.35;
          gl_FragColor = vec4(col, 1.0);
        }`,
      depthTest: false,
      depthWrite: false,
    });
    const earthRadius = SKY_DIST * Math.tan(EARTH_ANGULAR_DIAMETER / 2);
    this.earth = new THREE.Mesh(new THREE.SphereGeometry(earthRadius, 96, 48), this.earthMat);
    this.earth.position.copy(cfg.earthDir).multiplyScalar(SKY_DIST);
    this.earth.rotation.set(0.41, 0, 0.12); // axial tilt as seen from here
    this.earth.frustumCulled = false;
    this.earth.renderOrder = -1001;
    this.group.add(this.earth);

    const haloMat = new THREE.ShaderMaterial({
      uniforms: { uSunDir: { value: cfg.sunDir }, uCenter: { value: this.earth.position } },
      vertexShader: /* glsl */ `
        varying vec3 vN;
        varying vec3 vV;
        void main() {
          vN = normalize(mat3(modelMatrix) * normal);
          vec4 wp = modelMatrix * vec4(position, 1.0);
          vV = normalize(cameraPosition - wp.xyz);
          vec4 p = projectionMatrix * viewMatrix * wp;
          gl_Position = p.xyww;
        }`,
      fragmentShader: /* glsl */ `
        uniform vec3 uSunDir;
        varying vec3 vN;
        varying vec3 vV;
        void main() {
          float f = 1.0 - abs(dot(normalize(vN), normalize(vV)));
          float glow = pow(f, 5.0) * smoothstep(-0.3, 0.4, dot(normalize(vN), uSunDir));
          gl_FragColor = vec4(vec3(0.3, 0.55, 1.0) * glow * 0.9, 1.0);
        }`,
      blending: THREE.AdditiveBlending,
      side: THREE.BackSide,
      depthTest: false,
      depthWrite: false,
    });
    this.halo = new THREE.Mesh(new THREE.SphereGeometry(earthRadius * 1.035, 64, 32), haloMat);
    this.halo.position.copy(this.earth.position);
    this.halo.frustumCulled = false;
    this.halo.renderOrder = -1000;
    this.group.add(this.halo);
  }

  /** In Sol's system the Earth hangs in the sky; in any other, it doesn't. */
  setHome(home: boolean) {
    this.earth.visible = home;
    this.halo.visible = home;
  }

  setPixelRatio(pr: number) {
    this.starMat.uniforms.uPixelRatio.value = pr;
  }

  /** Keep the sky centred on the viewer; spin the Earth (1 turn per sidereal day). */
  update(camera: THREE.Camera, timeSeconds: number) {
    this.group.position.copy(camera.position);
    this.earthMat.uniforms.uSpin.value = (timeSeconds / 86164) % 1;
    this.updateComet(timeSeconds);
  }

  private updateComet(time: number) {
    if (this.nextComet < 0) this.nextComet = time + 240 + Math.random() * 360;
    const u = this.comet.material.uniforms;
    if (!this.comet.visible) {
      if (time < this.nextComet) return;
      this.cometStart = time;
      this.cometLife = 90 + Math.random() * 60;
      this.nextComet = time + this.cometLife + 360 + Math.random() * 360;
      this.cometBase.randomDirection();
      // Tails point away from the Sun, projected onto the celestial sphere.
      const tail = u.uTail.value as THREE.Vector3;
      tail.copy(this.cometBase).multiplyScalar(this.cometBase.dot(this.cfg.sunDir)).sub(this.cfg.sunDir);
      if (tail.lengthSq() < 0.02) {
        this.cometBase.crossVectors(this.cfg.sunDir, _cometAxis).normalize();
        tail.copy(this.cfg.sunDir).negate();
      }
      tail.normalize();
      (u.uSide.value as THREE.Vector3).crossVectors(this.cometBase, tail).normalize();
      this.cometDrift.copy(u.uSide.value).multiplyScalar(0.000015);
      u.uLength.value = 0.022 + Math.random() * 0.014;
      this.comet.visible = true;
    }
    const age = time - this.cometStart;
    if (age >= this.cometLife) { this.comet.visible = false; u.uFade.value = 0; return; }
    (u.uDir.value as THREE.Vector3).copy(this.cometBase).addScaledVector(this.cometDrift, age).normalize();
    const t = Math.min(1, age / 15, (this.cometLife - age) / 20);
    u.uFade.value = t * t * (3 - 2 * t);
  }

  get sunDir() {
    return this.cfg.sunDir;
  }
}

const _cometAxis = new THREE.Vector3(0.37, 0.81, -0.45).normalize();

/** B–V colour index → linear RGB (approximation of a blackbody). */
function bvToRgb(bv: number): [number, number, number] {
  bv = Math.max(-0.4, Math.min(2.0, bv));
  const t = 4600 * (1 / (0.92 * bv + 1.7) + 1 / (0.92 * bv + 0.62)); // Ballesteros
  // Tanner Helland's blackbody fit, output sRGB 0..1
  const k = t / 100;
  let r: number, g: number, b: number;
  if (k <= 66) {
    r = 255;
    g = 99.4708025861 * Math.log(k) - 161.1195681661;
    b = k <= 19 ? 0 : 138.5177312231 * Math.log(k - 10) - 305.0447927307;
  } else {
    r = 329.698727446 * Math.pow(k - 60, -0.1332047592);
    g = 288.1221695283 * Math.pow(k - 60, -0.0755148492);
    b = 255;
  }
  const lin = (c: number) => Math.pow(Math.max(0, Math.min(255, c)) / 255, 2.2);
  return [lin(r), lin(g), lin(b)];
}
