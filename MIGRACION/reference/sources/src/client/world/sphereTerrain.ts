import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import { surfaceOf, type CelestialBody } from '../../shared/space/body';
import { cubeArc, cubeDir, faceOf, facePoint } from '../../shared/space/cubeSphere';
import type { BodySurface } from '../../shared/space/surface';
import { modFeature, modTouches, type TerrainMod } from '../../shared/space/terrainMods';
import type { TerrainWorkerPool } from './workerPool';
import { origin } from '../render/origin';
import { MORPH_GLSL, patchTerrainDepth } from './terrainShader';
import { TERRAIN_RES, TERRAIN_SHADOW_RANGE, TERRAIN_SPLIT } from './terrainGrid';

/**
 * A whole body's terrain, the same everywhere: six quadtrees over the faces of a cube blown out onto
 * the sphere (shared/space/cubeSphere.ts), each node a fixed 32×32 grid laid on the body's one
 * surface — the procedural relief with every terrain modifier on it (the base's field and pads,
 * craters: shared/space/surface.ts) — built in a worker. Nodes split with the camera's distance
 * down to ~0.3 m a cell; a node only splits once its four children are ready (no holes), CDLOD
 * morphing blends each node into its parent's grid before it gives way (crack-free joins, no
 * popping) and skirts cover what a transient jump of more than one level leaves. What is below the
 * horizon or outside the view is neither drawn nor built first.
 *
 * Precision: vertices are relative to each node's middle and the nodes hang from the render
 * origin's root (render/origin.ts); the detail textures run on metres along the face relative to a
 * corner snapped to TEX_PERIOD, never on absolute positions. Horizon shadows are baked per node with
 * the sun of the moment (`setSun` rebuilds them when it moves).
 */

/** Cells per node side, and split while the camera is closer than SPLIT node widths (world/terrainGrid.ts). */
const RES = TERRAIN_RES;
const SPLIT = TERRAIN_SPLIT;
/** Cells this wide (m) right under the camera: the deepest level. */
const FINEST_CELL = 0.4;
/** Height (m, from the mean sphere) of the sphere the horizon test hides things behind. */
const OCCLUDER = -3000;
/**
 * The detail textures' coordinates restart every this many metres: every layer's period divides it
 * (2, 8 rotated, 160, 1280 m), so neighbouring nodes agree.
 */
const TEX_PERIOD = 2560;
/** Nodes this close (m) cast sun shadows into the cascades; the baked horizon shadows do the rest. */
const SHADOW_RANGE = TERRAIN_SHADOW_RANGE.high;
/** Modifiers this far past a node (m) still shade it (horizon shadows), when at least this big (m). */
const SHADE_MARGIN = 2000;
const SHADE_FEATURE = 30;
/** Built nodes joining the scene per frame: at least this many, more while under this much time (ms). */
const INTEGRATE_MIN = 2;
const INTEGRATE_MS = 2;

interface SNode {
  face: number;
  level: number;
  a0: number;
  b0: number;
  size: number;
  /** Middle direction (unit, from the centre) and width over the surface (m). */
  dir: [number, number, number];
  arc: number;
  /** Width of a cell (m). */
  cell: number;
  hMin: number;
  hMax: number;
  state: 'idle' | 'loading' | 'ready';
  mesh: THREE.Mesh | null;
  kids: SNode[] | null;
  parent: SNode | null;
  lastUsed: number;
  cancel?: () => void;
  setPriority?: (p: number) => void;
  /** Built from ground that changed since (a modifier): rebuilt in place, the old mesh kept until then. */
  stale: boolean;
  rebuilding: boolean;
  /** Sun it was baked with. */
  sunV: number;
}

const _cam = new THREE.Vector3();
const _sphere = new THREE.Sphere();
const _m = new THREE.Matrix4();
const _mods: TerrainMod[] = [];
const _fp = facePoint();

export class SphereTerrain {
  readonly group = new THREE.Group();
  readonly material: THREE.MeshStandardMaterial;
  private depthMaterial: THREE.MeshDepthMaterial;
  private roots: SNode[] = [];
  private nodes = new Set<SNode>();
  private index: THREE.BufferAttribute;
  private frame = 0;
  private drawn: SNode[] = [];
  private next: SNode[] = [];
  private surface: BodySurface;
  private frustum = new THREE.Frustum();
  private camRel = new THREE.Vector3();
  private viewer = { value: new THREE.Vector3() };
  private upOrigin = { value: new THREE.Vector3() };
  private uniforms: Record<string, THREE.IUniform>;
  private maxLevel: number;
  private sun = new THREE.Vector3(0, 1, 0);
  private sunV = 0;
  /** Built nodes waiting to join the scene (see `update`). */
  private readonly arrived: Array<() => void> = [];

  constructor(
    private pool: TerrainWorkerPool,
    private body: CelestialBody,
    private seed: number,
    loader: THREE.TextureLoader,
    sunDir: THREE.Vector3,
    csm: CSM | null,
    maxAnisotropy: number,
  ) {
    this.group.name = `Terrain:${body.def.id}`;
    this.surface = surfaceOf(body, seed)!;
    this.index = buildIndex(RES);
    this.sun.copy(sunDir);
    // the deepest level: cells of FINEST_CELL under the camera
    this.maxLevel = Math.ceil(Math.log2(cubeArc(2, body.radius) / (RES * FINEST_CELL)));
    const tex = (url: string) => {
      const t = loader.load(url);
      t.wrapS = t.wrapT = THREE.RepeatWrapping;
      t.colorSpace = THREE.NoColorSpace;
      t.anisotropy = maxAnisotropy;
      return t;
    };
    this.uniforms = {
      tRegA: { value: tex('/assets/tex/regolith_a.jpg') }, // data: mean ≈ 0.5
      tRegN: { value: tex('/assets/tex/regolith_n.jpg') },
      tMacro: { value: tex('/assets/tex/regolith_macro.png') },
      uSunDirW: { value: sunDir },
      // the viewer in render space, like the matrices it is compared with (render/origin.ts)
      uViewer: this.viewer,
      // render space → the body's centre (directions only: float32 is plenty)
      uUpOrigin: this.upOrigin,
      uBakedFade: { value: new THREE.Vector2(120, 190) },
    };
    this.material = this.buildMaterial(csm);
    // shadow casting must morph exactly like the visible surface
    const depth = new THREE.MeshDepthMaterial({ depthPacking: THREE.RGBADepthPacking });
    depth.onBeforeCompile = shader => patchTerrainDepth(shader, this.viewer, (RES + 1) * (RES + 1));
    depth.customProgramCacheKey = () => 'body-terrain-depth-v2';
    this.depthMaterial = depth;
    for (let f = 0; f < 6; f++) this.roots.push(this.node(f, 0, -1, -1, 2, null));
  }

  private buildMaterial(csm: CSM | null) {
    // linear albedo ≈ 0.13 (real regolith 0.10–0.15); the detail maps average to 1.0
    const mat = new THREE.MeshStandardMaterial({ color: new THREE.Color().setRGB(0.135, 0.134, 0.13, THREE.LinearSRGBColorSpace), roughness: 0.96, metalness: 0 });
    mat.name = 'Regolith';
    if (csm) csm.setupMaterial(mat);
    const csmHook = mat.onBeforeCompile;
    const grid = (RES + 1) * (RES + 1);
    mat.onBeforeCompile = (shader, renderer) => {
      csmHook?.call(mat, shader, renderer);
      Object.assign(shader.uniforms, this.uniforms);
      shader.vertexShader = shader.vertexShader
        .replace(
          '#include <common>',
          `#include <common>
          attribute vec3 surf;
          attribute vec4 morph;
          attribute vec4 morphN;
          attribute vec2 morphS;
          attribute vec4 tex;
          attribute vec3 faceU;
          uniform vec3 uViewer;
          uniform vec3 uUpOrigin;
          varying vec3 vSurf;
          varying vec2 vTex;
          varying float vSkirt;
          varying vec3 vWorldPos;
          varying vec3 vWorldNormal;
          varying vec3 vFaceU;
          varying vec3 vUp;`,
        )
        .replace(
          '#include <beginnormal_vertex>',
          `#include <beginnormal_vertex>
          float cdlodK0 = clamp((distance((modelMatrix * vec4(position, 1.0)).xyz, uViewer) - morph.w * 0.68) / (morph.w * 0.3), 0.0, 1.0);
          #ifdef DBG_NOMORPH
          cdlodK0 = 0.0;
          #endif
          objectNormal = normalize(mix(objectNormal, morphN.xyz, cdlodK0));`,
        )
        .replace(
          '#include <begin_vertex>',
          `#include <begin_vertex>
          ${MORPH_GLSL}
          vSurf = mix(surf, vec3(morphS, morphN.w), cdlodK);
          vTex = mix(tex.xy, tex.zw, cdlodK);
          vSkirt = float(gl_VertexID >= ${grid});
          vWorldPos = (modelMatrix * vec4(transformed, 1.0)).xyz;
          vWorldNormal = normalize(mat3(modelMatrix) * objectNormal);
          vFaceU = faceU;
          vUp = normalize(vWorldPos + uUpOrigin);`,
        );
      shader.fragmentShader = shader.fragmentShader
        .replace(
          '#include <common>',
          `#include <common>
          uniform sampler2D tRegA, tRegN, tMacro;
          uniform vec3 uSunDirW;
          uniform vec2 uBakedFade;
          varying vec3 vSurf;
          varying vec2 vTex;
          varying float vSkirt;
          varying vec3 vWorldPos;
          varying vec3 vWorldNormal;
          varying vec3 vFaceU;
          varying vec3 vUp;`,
        )
        .replace(
          '#include <lights_fragment_begin>',
          // baked horizon shadow multiplies only the sun (directional) lights, not helmet lamps
          THREE.ShaderChunk.lights_fragment_begin
            .replace(/getDirectionalLightInfo\(\s*directionalLights?(\[0\])?,\s*directLight\s*\);/g, (m) => `${m} directLight.color *= sunVisF;`)
            .replace(/&&\s*receiveShadow\s*\)/g, '&& receiveShadow && vSkirt < 0.5 )'),
        )
        .replace(
          '#include <map_fragment>',
          `
          // metres along the face, restarting every ${TEX_PERIOD} m (vTex is exact inside the node)
          vec2 wuv = vTex;
          // One baked, mipmapped lookup: independent seamless warp axes + multiscale tone.
          // The 2.56 km period is shared by all nodes; no per-patch random phase or GPU fractal noise.
          vec3 macro = texture2D(tMacro, wuv / ${TEX_PERIOD.toFixed(1)}).rgb;
          wuv += (macro.rg - 0.5) * 32.0;
          // the 8 m layer turned by atan(4/3): its lattice stops lining up with the 2 m one
          vec2 wuv8 = vec2(wuv.x * 3.0 + wuv.y * 4.0, wuv.y * 3.0 - wuv.x * 4.0) / 40.0 + vec2(0.31, 0.77);
          float camDist = length(vWorldPos - cameraPosition);
          // near field: real-time cascades only; baked horizon shadows take over further out
          float sunVisF = mix(1.0, vSurf.z, smoothstep(uBakedFade.x, uBakedFade.y, camDist));
          #ifdef DBG_NOBAKED
          sunVisF = 1.0;
          #endif
          float worked = vSurf.y;
          vec3 a1 = texture2D(tRegA, wuv / 2.0).rgb;
          vec3 a2 = texture2D(tRegA, wuv8).rgb;
          float nearFade = 1.0 - smoothstep(25.0, 160.0, camDist);
          // worked ground (pads, levelled fields) is smoother and a little darker
          vec3 det = mix(vec3(1.0), (a1 + a2), (0.45 + 0.55 * nearFade) * (1.0 - 0.45 * worked));
          float slope = 1.0 - clamp(dot(normalize(vWorldNormal), vUp), 0.0, 1.0);
          float rocky = smoothstep(0.22, 0.5, slope);
          diffuseColor.rgb *= det * vSurf.x * (0.87 + 0.26 * macro.b) * mix(1.0, 0.78, rocky) * (1.0 - 0.1 * worked);
          `,
        )
        .replace(
          '#include <normal_fragment_maps>',
          `
          {
            vec3 nA = texture2D(tRegN, wuv / 2.0).xyz * 2.0 - 1.0;
            vec3 nB = texture2D(tRegN, wuv8).xyz * 2.0 - 1.0;
            // the turned layer's normals back into the face's axes
            vec2 nb = vec2(nB.x * 0.6 - nB.y * 0.8, nB.x * 0.8 + nB.y * 0.6);
            float s = (0.5 + 0.9 * rocky) * (0.35 + 0.65 * nearFade) * (1.0 - 0.6 * worked);
            vec3 Nw = normalize(vWorldNormal);
            vec3 T = normalize(vFaceU - Nw * dot(Nw, vFaceU));
            vec3 B = cross(Nw, T);
            vec3 nW = normalize(T * (nA.x + 0.8 * nb.x) * s + B * (nA.y + 0.8 * nb.y) * s + Nw);
            normal = normalize((viewMatrix * vec4(nW, 0.0)).xyz);
            // regolith photometry: Lommel–Seeliger limb behaviour + opposition surge
            vec3 V = normalize(cameraPosition - vWorldPos);
            float mu0 = max(dot(nW, uSunDirW), 0.0);
            float mu = max(dot(nW, V), 0.0);
            float ls = mix(1.0, 2.0 / (mu0 + mu + 0.15), 0.55);
            float g = acos(clamp(dot(V, uSunDirW), -1.0, 1.0));
            float surge = 1.0 + 0.75 * exp(-g / 0.06) + 0.35 * exp(-g / 0.45);
            diffuseColor.rgb *= clamp(ls, 0.6, 2.4) * surge;
          }
          `,
        );
    };
    const dbg = new URLSearchParams(location.search);
    if (dbg.has('nobaked')) mat.defines = { ...mat.defines, DBG_NOBAKED: '' };
    if (dbg.has('nomorph')) mat.defines = { ...mat.defines, DBG_NOMORPH: '' };
    mat.customProgramCacheKey = () => 'body-terrain-v5';
    return mat;
  }

  private node(face: number, level: number, a0: number, b0: number, size: number, parent: SNode | null): SNode {
    const R = this.body.radius;
    const dir = cubeDir(face, a0 + size / 2, b0 + size / 2, [0, 0, 0]) as [number, number, number];
    const n: SNode = {
      face,
      level,
      a0,
      b0,
      size,
      dir,
      arc: cubeArc(size, R),
      cell: cubeArc(size / RES, R),
      hMin: this.surface.lowest,
      hMax: this.surface.highest,
      state: 'idle',
      mesh: null,
      kids: null,
      parent,
      lastUsed: this.frame,
      stale: false,
      rebuilding: false,
      sunV: -1,
    };
    this.nodes.add(n);
    return n;
  }

  /** Nodes this close (m) cast into the shadow cascades (default SHADOW_RANGE; a profile with shorter cascades, less). */
  private shadowRange: number = SHADOW_RANGE;
  setShadowRange(m: number) {
    this.shadowRange = m;
  }

  /** Distance band where baked shadows replace the real-time cascades (match CSM maxFar). */
  setBakedFade(start: number, end: number) {
    (this.uniforms.uBakedFade.value as THREE.Vector2).set(start, end);
  }

  /** The sun moved (world, unit): nodes are baked again once it has turned more than half a degree. */
  setSun(dir: THREE.Vector3) {
    if (dir.dot(this.sun) > Math.cos((0.5 * Math.PI) / 180)) return;
    this.sun.copy(dir);
    this.sunV++;
  }

  /** With streaming=false, reproject built nodes only (auxiliary views, without workers or eviction). */
  update(camera: THREE.Camera, streaming = true) {
    if (streaming) {
      this.frame++;
      // the nodes the workers built: a few per frame, within a time budget
      const t0 = performance.now();
      for (let k = 0; this.arrived.length && (k < INTEGRATE_MIN || performance.now() - t0 < INTEGRATE_MS); k++) this.arrived.shift()!();
    }
    // the camera in the world (its matrices are in render space: the frustum is, too)
    origin.worldOf(camera, _cam);
    const c = this.body.center;
    this.camRel.set(_cam.x - c[0], _cam.y - c[1], _cam.z - c[2]);
    this.viewer.value.set(_cam.x - origin.x, _cam.y - origin.y, _cam.z - origin.z);
    this.upOrigin.value.set(origin.x - c[0], origin.y - c[1], origin.z - c[2]);
    camera.updateMatrixWorld();
    _m.multiplyMatrices((camera as THREE.PerspectiveCamera).projectionMatrix, camera.matrixWorldInverse);
    this.frustum.setFromProjectionMatrix(_m);
    const out = this.next;
    out.length = 0;
    for (const r of this.roots) this.traverse(r, out, streaming);
    for (const n of this.drawn) if (n.mesh) n.mesh.visible = false;
    for (const n of out) if (n.mesh) n.mesh.visible = true;
    this.next = this.drawn;
    this.drawn = out;
    if (streaming && this.frame % 120 === 0) this.evict();
  }

  /**
   * Below the horizon from the camera: behind a sphere a little under the mean ground (the deepest
   * basins are left out: from inside one its walls hide the rest anyway), for the node's highest
   * point once known.
   */
  private hidden(n: SNode) {
    const R = this.body.radius;
    const low = R + Math.max(this.surface.lowest, OCCLUDER);
    const high = R + Math.max(n.hMax, 0);
    const D = this.camRel.length();
    const cx = this.camRel.x / D;
    const cy = this.camRel.y / D;
    const cz = this.camRel.z / D;
    const gamma = Math.acos(Math.max(-1, Math.min(1, cx * n.dir[0] + cy * n.dir[1] + cz * n.dir[2])));
    const thetaC = Math.acos(Math.min(1, low / D));
    const thetaN = Math.acos(Math.min(1, low / high));
    return gamma > thetaC + thetaN + (n.arc * 0.75) / R;
  }

  /** Distance from the camera to the node's piece of surface (m, a lower bound). */
  private distance(n: SNode) {
    const r = this.body.radius + (n.hMin + n.hMax) / 2;
    const dx = this.camRel.x - n.dir[0] * r;
    const dy = this.camRel.y - n.dir[1] * r;
    const dz = this.camRel.z - n.dir[2] * r;
    return Math.max(0, Math.sqrt(dx * dx + dy * dy + dz * dz) - n.arc * 0.75 - (n.hMax - n.hMin) / 2);
  }

  /** The node's piece of surface is in the camera's view (render space). */
  private inView(n: SNode) {
    const R = this.body.radius + (n.hMin + n.hMax) / 2;
    const c = this.body.center;
    _sphere.center.set(c[0] + n.dir[0] * R - origin.x, c[1] + n.dir[1] * R - origin.y, c[2] + n.dir[2] * R - origin.z);
    _sphere.radius = n.arc * 0.75 + (n.hMax - n.hMin) / 2;
    return this.frustum.intersectsSphere(_sphere);
  }

  /** Drawn or needing nothing: ready, or never to be drawn (below the horizon). */
  private settled(n: SNode) {
    return n.state === 'ready' || this.hidden(n);
  }

  private traverse(n: SNode, out: SNode[], streaming: boolean) {
    if (streaming) n.lastUsed = this.frame;
    if (this.hidden(n)) return;
    const dist = this.distance(n);
    if (n.level < this.maxLevel && dist < n.arc * SPLIT && (streaming || n.kids)) {
      if (!n.kids) {
        const h = n.size / 2;
        const l = n.level + 1;
        n.kids = [this.node(n.face, l, n.a0, n.b0, h, n), this.node(n.face, l, n.a0 + h, n.b0, h, n), this.node(n.face, l, n.a0, n.b0 + h, h, n), this.node(n.face, l, n.a0 + h, n.b0 + h, h, n)];
      }
      let ready = true;
      for (const k of n.kids) {
        if (streaming) k.lastUsed = this.frame;
        if (streaming && !this.hidden(k)) this.request(k);
        if (!this.settled(k)) ready = false;
      }
      if (ready) {
        for (const k of n.kids) this.traverse(k, out, streaming);
        return;
      }
    }
    if (n.state === 'ready') {
      if (streaming && (n.stale || n.sunV !== this.sunV)) this.request(n);
      if (n.mesh && n.mesh.geometry.boundingSphere) {
        // only the ground round the camera casts into the shadow cascades; that one stays even out
        // of the view (a ridge behind the camera shades what it sees: three.js leaves it out of the
        // view by its bounding sphere, the cascades draw it)
        const caster = dist < this.shadowRange;
        n.mesh.castShadow = caster;
        // out of the view: not drawn (kept, and still in the tree); centre (world) → render space
        const m = n.mesh.position;
        _sphere.center.set(m.x - origin.x, m.y - origin.y, m.z - origin.z);
        _sphere.radius = n.mesh.geometry.boundingSphere.radius;
        if (!caster && !this.frustum.intersectsSphere(_sphere)) return;
      }
      out.push(n);
    } else if (streaming) this.request(n);
  }

  private request(n: SNode) {
    // what the camera sees first, near before far, coarse before fine
    const priority = 1 + this.distance(n) / n.arc + n.level * 0.02 + (this.inView(n) ? 0 : 3);
    if (n.state === 'loading') {
      n.setPriority?.(priority);
      return;
    }
    const due = n.stale || n.sunV !== this.sunV;
    if (n.state === 'ready' && (!due || n.rebuilding)) return;
    const rebuild = n.state === 'ready';
    if (rebuild) n.rebuilding = true;
    else n.state = 'loading';
    n.stale = false;
    const sunV = this.sunV;
    // the modifiers that reach it (and the big ones round it: they shade it)
    _mods.length = 0;
    // Parent normals need a two-cell ring beyond the visible grid.
    this.surface.mods.near(n.dir, n.arc * 0.82, n.cell, _mods, SHADE_MARGIN, SHADE_FEATURE);
    const job = this.pool.run(
      {
        kind: 'sphere',
        body: this.body.def.id,
        seed: this.seed,
        mods: _mods.slice(),
        face: n.face,
        a0: n.a0,
        b0: n.b0,
        size: n.size,
        res: RES,
        skirt: Math.max(0.5, n.cell * 2, n.arc * 0.01),
        morphEnd: SPLIT * n.arc * 2,
        sun: [this.sun.x, this.sun.y, this.sun.z],
        texPeriod: TEX_PERIOD,
      },
      priority,
    );
    n.cancel = job.cancel;
    n.setPriority = job.setPriority;
    // built nodes join the scene a few per frame (each is a mesh to make and upload: a crowd of them
    // landing in one frame is a hitch)
    job.promise.then((r) => this.arrived.push(() => {
      if (r.kind !== 'sphere') return;
      if (rebuild) {
        n.rebuilding = false;
        if (!n.mesh) return;
      } else if (n.state !== 'loading') return;
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(r.positions, 3));
      g.setAttribute('normal', new THREE.BufferAttribute(r.normals, 3));
      g.setAttribute('surf', new THREE.BufferAttribute(r.surf, 3));
      g.setAttribute('morph', new THREE.BufferAttribute(r.morph, 4));
      g.setAttribute('morphN', new THREE.BufferAttribute(r.morphN, 4));
      g.setAttribute('morphS', new THREE.BufferAttribute(r.morphS, 2));
      g.setAttribute('tex', new THREE.BufferAttribute(r.tex, 4));
      g.setAttribute('faceU', new THREE.BufferAttribute(r.faceU, 3));
      g.setIndex(this.index);
      g.boundingSphere = new THREE.Sphere(new THREE.Vector3(), r.radius);
      n.hMin = r.hMin;
      n.hMax = r.hMax;
      n.sunV = sunV;
      if (n.mesh) {
        // rebuilt (the ground or the sun changed): swap the geometry, keep the rest
        n.mesh.geometry.dispose();
        n.mesh.geometry = g;
        n.state = 'ready';
        return;
      }
      const mesh = new THREE.Mesh(g, this.material);
      // world position under the render origin's root; its matrices follow once it hangs there
      mesh.position.set(r.center[0], r.center[1], r.center[2]);
      mesh.matrixAutoUpdate = false;
      mesh.updateMatrix();
      mesh.castShadow = false;
      mesh.receiveShadow = true;
      mesh.customDepthMaterial = this.depthMaterial;
      mesh.visible = false;
      mesh.name = `body:${n.face}:${n.level}`;
      this.group.add(mesh);
      mesh.updateMatrixWorld(true);
      n.mesh = mesh;
      n.state = 'ready';
    }));
  }

  /**
   * The ground changed under a modifier (a crater, a site): every node it reaches that is fine
   * enough to show it is built again (the old mesh stays until the new one is ready); the ones
   * still loading start over with the new ground.
   */
  invalidate(mod: TerrainMod) {
    if (mod.body !== this.body.def.id) return;
    const feature = modFeature(mod);
    for (const n of this.nodes) {
      // coarse levels can't show it (they leave it out, like any detail finer than their cells)
      if (n.cell > feature * 1.34 + 1e-9) continue;
      if (!modTouches(mod, this.body, n.dir, n.arc * 0.82)) continue;
      if (n.state === 'ready') {
        n.stale = true;
        if (n.rebuilding) {
          n.cancel?.();
          n.rebuilding = false;
        }
      } else if (n.state === 'loading') {
        n.cancel?.();
        n.state = 'idle';
      }
    }
  }

  /** True once the ground under world point `p` is built at (about) the finest level (whatever the camera looks at). */
  readyAt(p: readonly number[]) {
    const c = this.body.center;
    const f = faceOf(p[0] - c[0], p[1] - c[1], p[2] - c[2], _fp);
    let n = this.roots[f.face];
    // down the tree toward the point while the next level is built
    while (n.kids) {
      const h = n.size / 2;
      const kid = n.kids[(f.a >= n.a0 + h ? 1 : 0) + (f.b >= n.b0 + h ? 2 : 0)];
      if (kid.state !== 'ready') break;
      n = kid;
    }
    return n.state === 'ready' && n.level >= this.maxLevel - 1;
  }

  /** Jobs in the workers' queue (the terrain's, the physics' and the rocks'). */
  get pendingJobs() {
    return this.pool.busy;
  }

  private evict() {
    for (const n of this.nodes) {
      if (n.level === 0 || this.frame - n.lastUsed < 600) continue;
      if (n.state === 'loading' || n.rebuilding) n.cancel?.();
      if (n.mesh) {
        this.group.remove(n.mesh);
        n.mesh.geometry.dispose();
      }
      if (n.parent) n.parent.kids = null;
      this.nodes.delete(n);
    }
  }

  /** Nodes drawn this frame (diagnostics). */
  get drawnCount() {
    return this.drawn.length;
  }

  dispose() {
    for (const n of this.nodes) {
      n.cancel?.();
      n.mesh?.geometry.dispose();
    }
    this.material.dispose();
    this.depthMaterial.dispose();
  }
}

/** Grid + skirt index (layout of the worker's `buildSphere`); skirts both ways round. */
function buildIndex(res: number) {
  const idx: number[] = [];
  const v = (i: number, j: number) => j * (res + 1) + i;
  for (let j = 0; j < res; j++) {
    for (let i = 0; i < res; i++) {
      const a = v(i, j);
      const b = v(i + 1, j);
      const c = v(i, j + 1);
      const d = v(i + 1, j + 1);
      idx.push(a, b, c, b, d, c);
    }
  }
  const grid = (res + 1) * (res + 1);
  const quad = (p: number, q: number, r: number, s: number) => idx.push(p, q, r, q, s, r, p, r, q, q, r, s);
  for (let i = 0; i < res; i++) {
    const s0 = grid + i;
    const s1 = grid + (res + 1) + i;
    const s2 = grid + 2 * (res + 1) + i;
    const s3 = grid + 3 * (res + 1) + i;
    quad(v(i, 0), v(i + 1, 0), s0, s0 + 1);
    quad(v(i, res), v(i + 1, res), s1, s1 + 1);
    quad(v(0, i), v(0, i + 1), s2, s2 + 1);
    quad(v(res, i), v(res, i + 1), s3, s3 + 1);
  }
  return new THREE.BufferAttribute(new Uint16Array(idx), 1);
}
