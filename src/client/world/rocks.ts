import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import { mergeVertices } from 'three/addons/utils/BufferGeometryUtils.js';
import { mulberry32 } from '../../shared/noise';
import { ROCK_VARIANTS } from '../../shared/terrain';
import type { LunarTerrain } from '../../shared/terrain';
import { editsNear } from './terrain';
import type { TerrainWorkerPool } from './workerPool';

const TILE = 64;
/** [ring radius in tiles, minimum rock size] — small stones only near the viewer. */
const RINGS: Array<[number, number, number]> = [
  [1, 0, 1e9],
  [3, 0.28, 1e9],
  [7, 0.75, 1e9],
];
const CAPACITY = 6000;

interface RockTile {
  key: string;
  data: Float32Array | null;
  cancel?: () => void;
}

/**
 * Procedural boulders: a few fractured-rock meshes generated at startup, instanced over the
 * deterministic scatter computed in the terrain workers.
 */
export class RockField {
  readonly group = new THREE.Group();
  private meshes: THREE.InstancedMesh[] = [];
  private tiles = new Map<string, RockTile>();
  private centerKey = '';
  private dirty = false;

  constructor(
    private pool: TerrainWorkerPool,
    private terrain: LunarTerrain,
    material: THREE.MeshStandardMaterial,
  ) {
    this.group.name = 'Rocks';
    for (let v = 0; v < ROCK_VARIANTS; v++) {
      const geo = makeRockGeometry(terrain.seed * 13 + v * 101);
      const mesh = new THREE.InstancedMesh(geo, material, CAPACITY);
      mesh.count = 0;
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      mesh.frustumCulled = false;
      mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
      this.meshes.push(mesh);
      this.group.add(mesh);
    }
  }

  static material(loader: THREE.TextureLoader, csm: CSM | null) {
    const n = loader.load('/assets/tex/regolith_n.jpg');
    n.wrapS = n.wrapT = THREE.RepeatWrapping;
    n.colorSpace = THREE.NoColorSpace;
    const mat = new THREE.MeshStandardMaterial({ roughness: 0.93, metalness: 0, vertexColors: true });
    mat.name = 'LunarRock';
    if (csm) csm.setupMaterial(mat);
    const csmHook = mat.onBeforeCompile;
    mat.onBeforeCompile = (shader, renderer) => {
      csmHook?.call(mat, shader, renderer);
      shader.uniforms.tRockN = { value: n };
      shader.vertexShader = shader.vertexShader
        .replace('#include <common>', '#include <common>\nvarying vec3 vRockPos;\nvarying vec3 vRockNrm;\nvarying mat3 vRockRot;')
        .replace(
          '#include <begin_vertex>',
          '#include <begin_vertex>\nvRockPos = position * 2.3;\nvRockNrm = normal;\nvRockRot = normalMatrix * mat3(instanceMatrix);',
        );
      shader.fragmentShader = shader.fragmentShader
        .replace('#include <common>', '#include <common>\nuniform sampler2D tRockN;\nvarying vec3 vRockPos;\nvarying vec3 vRockNrm;\nvarying mat3 vRockRot;')
        .replace(
          '#include <normal_fragment_maps>',
          `{
            // triplanar detail bump in object space
            vec3 w = pow(abs(normalize(vRockNrm)), vec3(4.0));
            w /= (w.x + w.y + w.z);
            vec3 nx = texture2D(tRockN, vRockPos.zy).xyz * 2.0 - 1.0;
            vec3 ny = texture2D(tRockN, vRockPos.xz).xyz * 2.0 - 1.0;
            vec3 nz = texture2D(tRockN, vRockPos.xy).xyz * 2.0 - 1.0;
            vec3 bump = vec3(0.0, nx.y, nx.x) * w.x + vec3(ny.x, 0.0, ny.y) * w.y + vec3(nz.x, nz.y, 0.0) * w.z;
            vec3 objN = normalize(vRockNrm + bump * 1.4);
            normal = normalize(vRockRot * objN);
          }`,
        );
    };
    mat.customProgramCacheKey = () => 'lunar-rock-v1';
    return mat;
  }

  update(viewer: THREE.Vector3) {
    const cx = Math.floor(viewer.x / TILE);
    const cz = Math.floor(viewer.z / TILE);
    const key = `${cx}:${cz}`;
    if (key !== this.centerKey) {
      this.centerKey = key;
      const wanted = new Set<string>();
      for (const [ring, minSize, maxSize] of RINGS) {
        for (let dz = -ring; dz <= ring; dz++) {
          for (let dx = -ring; dx <= ring; dx++) {
            // each tile is owned by the innermost ring that contains it
            const inner = RINGS.find(([r]) => Math.max(Math.abs(dx), Math.abs(dz)) <= r)!;
            if (inner[0] !== ring) continue;
            const tkey = `${cx + dx}:${cz + dz}:${minSize}`;
            wanted.add(tkey);
            if (!this.tiles.has(tkey)) this.request(cx + dx, cz + dz, tkey, minSize, maxSize, Math.hypot(dx, dz));
          }
        }
      }
      for (const [k, t] of this.tiles) {
        if (wanted.has(k)) continue;
        t.cancel?.();
        this.tiles.delete(k);
      }
      this.dirty = true;
    }
    if (this.dirty) this.rebuild();
  }

  /** Re-scatter tiles near an edit so rocks follow the new ground. */
  invalidate(x: number, z: number, radius: number) {
    const m = radius * 2.7;
    for (const [k, t] of this.tiles) {
      const [tx, tz] = k.split(':').map(Number);
      if (x + m < tx * TILE || x - m > (tx + 1) * TILE || z + m < tz * TILE || z - m > (tz + 1) * TILE) continue;
      t.cancel?.();
      this.tiles.delete(k);
    }
    this.centerKey = '';
  }

  private request(tx: number, tz: number, key: string, minSize: number, maxSize: number, dist: number) {
    const tile: RockTile = { key, data: null };
    this.tiles.set(key, tile);
    const job = this.pool.run(
      { kind: 'rocks', seed: this.terrain.seed, edits: editsNear(this.terrain.edits, tx * TILE, tz * TILE, TILE), x0: tx * TILE, z0: tz * TILE, size: TILE, minSize, maxSize },
      2 + dist * 0.5,
    );
    tile.cancel = job.cancel;
    job.promise.then((r) => {
      if (r.kind !== 'rocks' || this.tiles.get(key) !== tile) return;
      tile.data = r.rocks;
      this.dirty = true;
    });
  }

  private rebuild() {
    this.dirty = false;
    const counts = new Array(ROCK_VARIANTS).fill(0);
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const e = new THREE.Euler();
    const p = new THREE.Vector3();
    const s = new THREE.Vector3();
    for (const t of this.tiles.values()) {
      const d = t.data;
      if (!d) continue;
      for (let i = 0; i < d.length; i += 7) {
        const v = d[i + 5] | 0;
        if (counts[v] >= CAPACITY) continue;
        const size = d[i + 3];
        const j = d[i + 6];
        e.set((j - 0.5) * 0.5, d[i + 4], (fract(j * 7.31) - 0.5) * 0.5);
        q.setFromEuler(e);
        p.set(d[i], d[i + 1] - size * 0.3, d[i + 2]);
        s.set(size * (0.85 + 0.3 * fract(j * 3.7)), size * (0.6 + 0.35 * fract(j * 5.3)), size * (0.85 + 0.3 * fract(j * 9.1)));
        m.compose(p, q, s);
        this.meshes[v].setMatrixAt(counts[v]++, m);
      }
    }
    this.meshes.forEach((mesh, v) => {
      mesh.count = counts[v];
      mesh.instanceMatrix.needsUpdate = true;
    });
  }
}

const fract = (x: number) => x - Math.floor(x);

/** Fractured boulder: squashed sphere + random planar breaks + multi-scale bumps. */
function makeRockGeometry(seed: number): THREE.BufferGeometry {
  const rnd = mulberry32(seed);
  let geo: THREE.BufferGeometry = new THREE.IcosahedronGeometry(1, 5);
  geo.deleteAttribute('uv');
  geo.deleteAttribute('normal');
  geo = mergeVertices(geo);
  const pos = geo.getAttribute('position') as THREE.BufferAttribute;
  const planes: Array<[THREE.Vector3, number]> = [];
  const cuts = 7 + Math.floor(rnd() * 6);
  for (let i = 0; i < cuts; i++) {
    const n = new THREE.Vector3(rnd() * 2 - 1, rnd() * 1.6 - 0.5, rnd() * 2 - 1).normalize();
    planes.push([n, 0.5 + rnd() * 0.35]);
  }
  const stretch = new THREE.Vector3(0.8 + rnd() * 0.5, 0.75 + rnd() * 0.35, 0.8 + rnd() * 0.5);
  const lumps = Array.from({ length: 9 }, () => ({
    d: new THREE.Vector3(rnd() * 2 - 1, rnd() * 2 - 1, rnd() * 2 - 1).normalize(),
    f: 1.5 + rnd() * 6,
    ph: rnd() * Math.PI * 2,
    a: 0.02 + rnd() * 0.05,
  }));
  const colors = new Float32Array(pos.count * 3);
  const baseTone = 0.2 + rnd() * 0.12;
  const v = new THREE.Vector3();
  for (let i = 0; i < pos.count; i++) {
    v.fromBufferAttribute(pos, i);
    const dir = v.clone().normalize();
    let r = 1;
    for (const l of lumps) r += l.a * Math.sin(dir.dot(l.d) * l.f * 3 + l.ph);
    v.copy(dir).multiplyScalar(r).multiply(stretch);
    for (const [n, d] of planes) {
      const k = v.dot(n);
      if (k > d) v.addScaledVector(n, -(k - d) * 0.92);
    }
    if (v.y < -0.35) v.y = -0.35 + (v.y + 0.35) * 0.2;
    pos.setXYZ(i, v.x, v.y, v.z);
    // dust settles on top; darker fresh faces
    const tone = baseTone * (0.85 + 0.3 * Math.max(0, dir.y)) * (0.9 + 0.2 * Math.sin(v.x * 11 + v.z * 7));
    colors.set([tone, tone, tone * 0.98], i * 3);
  }
  geo.setAttribute('color', new THREE.BufferAttribute(colors, 3));
  geo.computeVertexNormals();
  geo.computeBoundingSphere();
  return geo;
}
