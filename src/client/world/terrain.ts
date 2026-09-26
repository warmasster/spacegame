import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import type { TerrainEdit } from '../../shared/protocol';
import type { LunarTerrain } from '../../shared/terrain';
import type { TerrainWorkerPool } from './workerPool';

/** Edits that can affect a square region (crater influence reaches 2.2 radii). */
export function editsNear(edits: TerrainEdit[], x0: number, z0: number, size: number) {
  return edits.filter((e) => {
    const m = e.r * 2.7;
    return e.x > x0 - m && e.x < x0 + size + m && e.z > z0 - m && e.z < z0 + size + m;
  });
}

/**
 * Quadtree LOD terrain. Leaves are fixed-resolution grids generated in workers from the
 * shared deterministic height function; skirts hide cracks between levels. A node only
 * splits once its four children are ready, so the ground never has holes.
 */

const ROOT_SIZE = 32768;
const MIN_SIZE = 16;
const RES = 32;
const SPLIT_FACTOR = 1.5; // > √2 guarantees neighbours differ by at most one level

/** CDLOD vertex morph (shared by the colour and shadow-depth shaders). */
const MORPH_GLSL = /* glsl */ `
  vec3 cdlodWorld = (modelMatrix * vec4(transformed, 1.0)).xyz;
  float cdlodK = clamp((distance(cdlodWorld, uViewer) - morph.w * 0.68) / (morph.w * 0.3), 0.0, 1.0);
  transformed = mix(transformed, morph.xyz, cdlodK);
`;

interface Node {
  key: string;
  level: number;
  x0: number;
  z0: number;
  size: number;
  state: 'idle' | 'loading' | 'ready';
  mesh: THREE.Mesh | null;
  lastUsed: number;
  minY: number;
  maxY: number;
  cancel?: () => void;
  setPriority?: (p: number) => void;
  /** Geometry out of date (terrain edited): rebuilt in place, old mesh kept until then. */
  stale?: boolean;
  rebuilding?: boolean;
}

export class TerrainSystem {
  readonly group = new THREE.Group();
  readonly material: THREE.MeshStandardMaterial;
  private depthMaterial: THREE.MeshDepthMaterial;
  private viewer = { value: new THREE.Vector3() };
  private nodes = new Map<string, Node>();
  private index: THREE.BufferAttribute;
  private frame = 0;
  private uniforms: Record<string, THREE.IUniform>;
  private visible = new Set<Node>();

  constructor(
    private pool: TerrainWorkerPool,
    private terrain: LunarTerrain,
    loader: THREE.TextureLoader,
    private sunDir: THREE.Vector3,
    csm: CSM | null,
    maxAnisotropy: number,
  ) {
    this.group.name = 'Terrain';
    this.index = buildIndex(RES);

    const tex = (url: string, srgb: boolean) => {
      const t = loader.load(url);
      t.wrapS = t.wrapT = THREE.RepeatWrapping;
      t.colorSpace = srgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
      t.anisotropy = maxAnisotropy;
      return t;
    };
    this.uniforms = {
      tRegA: { value: tex('/assets/tex/regolith_a.jpg', false) }, // data: mean ≈ 0.5
      tRegN: { value: tex('/assets/tex/regolith_n.jpg', false) },
      tMacro: { value: tex('/assets/tex/macro.png', false) },
      uSunDirW: { value: sunDir },
      uViewer: this.viewer,
      uBakedFade: { value: new THREE.Vector2(120, 190) },
    };

    // linear albedo ≈ 0.13 (real regolith 0.10–0.15); the detail maps average to 1.0
    const mat = new THREE.MeshStandardMaterial({ color: new THREE.Color().setRGB(0.135, 0.134, 0.13, THREE.LinearSRGBColorSpace), roughness: 0.96, metalness: 0 });
    mat.name = 'LunarRegolith';
    if (csm) csm.setupMaterial(mat);
    const csmHook = mat.onBeforeCompile;
    mat.onBeforeCompile = (shader, renderer) => {
      csmHook?.call(mat, shader, renderer);
      Object.assign(shader.uniforms, this.uniforms);
      shader.vertexShader = shader.vertexShader
        .replace(
          '#include <common>',
          `#include <common>
          attribute float albedo;
          attribute vec4 morph;
          attribute float sunVis;
          uniform vec3 uViewer;
          varying float vSunVis;
          varying float vSkirt;
          varying float vAlbedo;
          varying vec3 vWorldPos;
          varying vec3 vWorldNormal;`,
        )
        .replace(
          '#include <begin_vertex>',
          `#include <begin_vertex>
          ${MORPH_GLSL}
          vAlbedo = albedo;
          vSunVis = sunVis;
          vSkirt = float(gl_VertexID >= ${(RES + 1) * (RES + 1)});
          vWorldPos = (modelMatrix * vec4(transformed, 1.0)).xyz;
          vWorldNormal = normalize(mat3(modelMatrix) * objectNormal);`,
        );
      shader.fragmentShader = shader.fragmentShader
        .replace(
          '#include <common>',
          `#include <common>
          uniform sampler2D tRegA, tRegN, tMacro;
          uniform vec3 uSunDirW;
          uniform vec2 uBakedFade;
          varying float vSunVis;
          varying float vSkirt;
          varying float vAlbedo;
          varying vec3 vWorldPos;
          varying vec3 vWorldNormal;`,
        )
        .replace(
          '#include <lights_fragment_begin>',
          // baked horizon shadow multiplies only the sun (directional) lights, not helmet lamps
          THREE.ShaderChunk.lights_fragment_begin.replace(
            /getDirectionalLightInfo\(\s*directionalLights?(\[0\])?,\s*directLight\s*\);/g,
            (m) => `${m} directLight.color *= sunVisF;`,
          ).replace(/&&\s*receiveShadow\s*\)/g, '&& receiveShadow && vSkirt < 0.5 )'),
        )
        .replace(
          '#include <map_fragment>',
          `
          vec2 wuv = vWorldPos.xz;
          float camDist = length(vWorldPos - cameraPosition);
          // near field: real-time cascades only; baked horizon shadows take over further out
          float sunVisF = mix(1.0, vSunVis, smoothstep(uBakedFade.x, uBakedFade.y, camDist));
          vec3 a1 = texture2D(tRegA, wuv / 1.9).rgb;
          vec3 a2 = texture2D(tRegA, wuv / 8.3 + vec2(0.31, 0.77)).rgb;
          float m1 = texture2D(tMacro, wuv / 157.0).r;
          float m2 = texture2D(tMacro, wuv / 1290.0 + 0.5).r;
          float nearFade = 1.0 - smoothstep(25.0, 160.0, camDist);
          vec3 det = mix(vec3(1.0), (a1 + a2), 0.45 + 0.55 * nearFade);
          float slope = 1.0 - clamp(normalize(vWorldNormal).y, 0.0, 1.0);
          float rocky = smoothstep(0.22, 0.5, slope);
          diffuseColor.rgb *= det * vAlbedo * (0.78 + 0.44 * m1) * (0.88 + 0.24 * m2) * mix(1.0, 0.78, rocky);
          `,
        )
        .replace(
          '#include <normal_fragment_maps>',
          `
          {
            vec3 nA = texture2D(tRegN, wuv / 1.9).xyz * 2.0 - 1.0;
            vec3 nB = texture2D(tRegN, wuv / 8.3 + vec2(0.31, 0.77)).xyz * 2.0 - 1.0;
            float s = (0.5 + 0.9 * rocky) * (0.35 + 0.65 * nearFade);
            vec3 Nw = normalize(vWorldNormal);
            vec3 T = normalize(vec3(1.0, 0.0, 0.0) - Nw * Nw.x);
            vec3 B = normalize(cross(T, Nw));
            vec3 nW = normalize(T * (nA.x + 0.8 * nB.x) * s + B * (nA.y + 0.8 * nB.y) * s + Nw);
            normal = normalize((viewMatrix * vec4(nW, 0.0)).xyz);
            // Regolith photometry: Lommel–Seeliger limb behaviour + opposition surge.
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
    mat.customProgramCacheKey = () => 'lunar-terrain-v4';
    this.material = mat;

    // shadow casting must morph exactly like the visible surface
    const depth = new THREE.MeshDepthMaterial({ depthPacking: THREE.RGBADepthPacking });
    depth.onBeforeCompile = (shader) => {
      shader.uniforms.uViewer = this.viewer;
      shader.vertexShader = shader.vertexShader
        .replace('#include <common>', '#include <common>\nattribute vec4 morph;\nuniform vec3 uViewer;')
        .replace('#include <begin_vertex>', `#include <begin_vertex>\n${MORPH_GLSL}`);
    };
    depth.customProgramCacheKey = () => 'lunar-terrain-depth-v2';
    this.depthMaterial = depth;
  }

  /** Update LOD around the viewer. Call every frame (cheap when nothing changes). */
  update(viewer: THREE.Vector3, frustum: THREE.Frustum) {
    this.frame++;
    this.viewer.value.copy(viewer);
    const root = this.getNode(0, -ROOT_SIZE / 2, -ROOT_SIZE / 2, ROOT_SIZE);
    const next = new Set<Node>();
    this.traverse(root, viewer, next);
    for (const n of this.visible) if (!next.has(n) && n.mesh) n.mesh.visible = false;
    for (const n of next) if (n.mesh) n.mesh.visible = true;
    this.visible = next;
    void frustum;
    if (this.frame % 120 === 0) this.evict();
  }

  /** True once the ground under `p` is loaded at the finest level. */
  readyAt(x: number, z: number) {
    for (const n of this.visible) {
      if (n.size === MIN_SIZE && x >= n.x0 && x < n.x0 + n.size && z >= n.z0 && z < n.z0 + n.size) return true;
    }
    return false;
  }

  get pendingJobs() {
    return this.pool.busy;
  }

  private traverse(node: Node, viewer: THREE.Vector3, out: Set<Node>) {
    node.lastUsed = this.frame;
    const dist = distToBox(viewer, node);
    if (node.size > MIN_SIZE && dist < node.size * SPLIT_FACTOR) {
      const h = node.size / 2;
      const kids = [
        this.getNode(node.level + 1, node.x0, node.z0, h),
        this.getNode(node.level + 1, node.x0 + h, node.z0, h),
        this.getNode(node.level + 1, node.x0, node.z0 + h, h),
        this.getNode(node.level + 1, node.x0 + h, node.z0 + h, h),
      ];
      for (const k of kids) {
        k.lastUsed = this.frame;
        this.request(k, viewer);
      }
      if (kids.every((k) => k.state === 'ready')) {
        for (const k of kids) this.traverse(k, viewer, out);
        return;
      }
    }
    if (node.state === 'ready') {
      out.add(node);
      if (node.stale) this.request(node, viewer);
    } else this.request(node, viewer);
  }

  private getNode(level: number, x0: number, z0: number, size: number): Node {
    const key = `${level}:${x0}:${z0}`;
    let n = this.nodes.get(key);
    if (!n) {
      n = { key, level, x0, z0, size, state: 'idle', mesh: null, lastUsed: this.frame, minY: -200, maxY: 2500 };
      this.nodes.set(key, n);
    }
    return n;
  }

  private request(node: Node, viewer: THREE.Vector3) {
    const priority = distToBox(viewer, node) / node.size + node.level * 0.02;
    if (node.state === 'loading') {
      node.setPriority?.(priority);
      return;
    }
    if (node.state === 'ready' && (!node.stale || node.rebuilding)) return;
    const rebuild = node.state === 'ready';
    if (rebuild) node.rebuilding = true;
    else node.state = 'loading';
    node.stale = false;
    const s = this.sunDir;
    const job = this.pool.run(
      {
        kind: 'chunk',
        seed: this.terrain.seed,
        edits: editsNear(this.terrain.edits, node.x0, node.z0, node.size),
        sun: [s.x, s.y, s.z],
        x0: node.x0,
        z0: node.z0,
        size: node.size,
        res: RES,
        skirt: Math.max(1.5, node.size * 0.03),
        morphEnd: SPLIT_FACTOR * node.size * 2,
      },
      priority,
    );
    node.cancel = job.cancel;
    node.setPriority = job.setPriority;
    job.promise.then((r) => {
      if (r.kind !== 'chunk') return;
      if (rebuild) {
        node.rebuilding = false;
        if (!node.mesh) return;
      } else if (node.state !== 'loading') return;
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(r.positions, 3));
      g.setAttribute('normal', new THREE.BufferAttribute(r.normals, 3));
      g.setAttribute('albedo', new THREE.BufferAttribute(r.albedo, 1));
      g.setAttribute('morph', new THREE.BufferAttribute(r.morph, 4));
      g.setAttribute('sunVis', new THREE.BufferAttribute(r.sunVis, 1));
      g.setIndex(this.index);
      g.boundingBox = new THREE.Box3(new THREE.Vector3(0, r.minY, 0), new THREE.Vector3(node.size, r.maxY, node.size));
      g.boundingSphere = g.boundingBox.getBoundingSphere(new THREE.Sphere());
      if (node.mesh) {
        // rebuilt after an edit: swap geometry, keep visibility
        node.mesh.geometry.dispose();
        node.mesh.geometry = g;
        node.minY = r.minY;
        node.maxY = r.maxY;
        node.state = 'ready';
        return;
      }
      const mesh = new THREE.Mesh(g, this.material);
      mesh.position.set(node.x0, 0, node.z0);
      mesh.updateMatrix();
      mesh.matrixAutoUpdate = false;
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      mesh.customDepthMaterial = this.depthMaterial;
      mesh.visible = false;
      mesh.name = `terrain:${node.key}`;
      this.group.add(mesh);
      node.mesh = mesh;
      node.minY = r.minY;
      node.maxY = r.maxY;
      node.state = 'ready';
    });
  }

  /** Distance band where baked shadows replace the real-time cascades (match CSM maxFar). */
  setBakedFade(start: number, end: number) {
    (this.uniforms.uBakedFade.value as THREE.Vector2).set(start, end);
  }

  /** Terrain changed around (x, z): rebuild every loaded chunk that can see it. */
  invalidate(x: number, z: number, radius: number) {
    const m = radius * 2.7;
    for (const n of this.nodes.values()) {
      if (x + m < n.x0 || x - m > n.x0 + n.size || z + m < n.z0 || z - m > n.z0 + n.size) continue;
      // coarse levels can't show a 2 m crater; don't waste work on them
      if (n.size / RES > radius * 2) continue;
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

  private evict() {
    for (const [key, n] of this.nodes) {
      if (n.level === 0 || this.frame - n.lastUsed < 600) continue;
      if (n.state === 'loading') n.cancel?.();
      if (n.mesh) {
        this.group.remove(n.mesh);
        n.mesh.geometry.dispose();
      }
      this.nodes.delete(key);
    }
  }
}

function distToBox(p: THREE.Vector3, n: Node) {
  const dx = Math.max(n.x0 - p.x, 0, p.x - (n.x0 + n.size));
  const dz = Math.max(n.z0 - p.z, 0, p.z - (n.z0 + n.size));
  const dy = Math.max(n.minY - p.y, 0, p.y - n.maxY);
  return Math.sqrt(dx * dx + dy * dy + dz * dz);
}

/** Grid + skirt index buffer shared by all chunks (layout matches terrain.worker). */
function buildIndex(res: number) {
  const idx: number[] = [];
  const v = (i: number, j: number) => j * (res + 1) + i;
  for (let j = 0; j < res; j++) {
    for (let i = 0; i < res; i++) {
      const a = v(i, j), b = v(i + 1, j), c = v(i, j + 1), d = v(i + 1, j + 1);
      idx.push(a, c, b, b, c, d);
    }
  }
  const base = (res + 1) * (res + 1);
  const s0 = base, s1 = base + (res + 1), s2 = base + 2 * (res + 1), s3 = base + 3 * (res + 1);
  for (let i = 0; i < res; i++) {
    // z = 0 edge (faces -Z)
    idx.push(v(i, 0), v(i + 1, 0), s0 + i, v(i + 1, 0), s0 + i + 1, s0 + i);
    // z = res edge (faces +Z)
    idx.push(v(i, res), s1 + i, v(i + 1, res), v(i + 1, res), s1 + i, s1 + i + 1);
    // x = 0 edge (faces -X)
    idx.push(v(0, i), s2 + i, v(0, i + 1), v(0, i + 1), s2 + i, s2 + i + 1);
    // x = res edge (faces +X)
    idx.push(v(res, i), v(res, i + 1), s3 + i, v(res, i + 1), s3 + i + 1, s3 + i);
  }
  return new THREE.BufferAttribute(new Uint16Array(idx), 1);
}
