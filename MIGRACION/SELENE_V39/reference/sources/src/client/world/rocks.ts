import * as THREE from 'three';
import type { CSM } from 'three/addons/csm/CSM.js';
import { mergeVertices } from 'three/addons/utils/BufferGeometryUtils.js';
import { mulberry32 } from '../../shared/noise';
import { surfaceOf, type CelestialBody } from '../../shared/space/body';
import { cellOf, cubeDir, faceOf, facePoint } from '../../shared/space/cubeSphere';
import { ROCK_VARIANTS, rockTileArc, rockTileLevel } from '../../shared/space/rocks';
import type { BodySurface } from '../../shared/space/surface';
import { tangentAxes } from '../../shared/space/tangent';
import { modTouches, type TerrainMod } from '../../shared/space/terrainMods';
import type { TerrainWorkerPool } from './workerPool';

/** [ring radius in tiles, minimum rock size] — small stones only near the viewer. */
const RINGS: Array<[number, number]> = [
  [1, 0],
  [3, 0.28],
  [6, 0.75],
];
/**
 * Mesh detail per LOD (icosahedron subdivision: 720, 320, 80 and 20 triangles) and the distance
 * (m) up to which each one is used.
 */
const LODS: Array<[number, number]> = [
  [5, 14],
  [3, 45],
  [1, 140],
  [0, Infinity],
];
/** A rock smaller than this over its distance is not drawn at all (a pixel or less). */
const MIN_ANGLE = 0.0025;
/** Sun shadows: rocks from this size in the inner ring, from the second in the middle one; none further out. */
const CAST_INNER = 0.4;
const CAST_MID = 1.5;
/** Most rocks each batch holds (the rings hold ~1.5 K). */
const MAX_ROCKS = 12000;
/** The viewer moves this far (m) before the LODs are chosen again. */
const LOD_STEP = 2;
/** The instances are relative to an anchor near the viewer (float32 stays exact); a new one past this (m). */
const REANCHOR = 2000;
/** Numbers per rock in a job's result (terrain.worker.ts `buildRocks`). */
const STRIDE = 9;

interface RockTile {
  key: number;
  face: number;
  ti: number;
  tj: number;
  ring: number;
  /** Middle direction of the tile (unit). */
  dir: [number, number, number];
  data: Float32Array | null;
  center: [number, number, number];
  /** Instances it put in the batches: batch, id, batch, id… */
  ids: number[];
  cancel?: () => void;
}

/** One BatchedMesh and what the field knows of each instance in it. */
interface Batch {
  mesh: THREE.BatchedMesh;
  /** Geometry id per [variant][lod]. */
  geo: number[][];
  /** Position relative to the anchor (m). */
  x: Float32Array;
  y: Float32Array;
  z: Float32Array;
  size: Float32Array;
  variant: Uint8Array;
  /** LOD drawn now, -1 hidden (too small to see), -2 no instance. */
  lod: Int8Array;
  /** Highest instance id in use + 1. */
  hi: number;
  count: number;
}

const _m = new THREE.Matrix4();
const _q = new THREE.Quaternion();
const _qu = new THREE.Quaternion();
const _e = new THREE.Euler();
const _p = new THREE.Vector3();
const _s = new THREE.Vector3();
const _up = new THREE.Vector3();
const _Y = new THREE.Vector3(0, 1, 0);
const _fp = facePoint();
const _te = [0, 0, 0];
const _ts = [0, 0, 0];
const _tu = [0, 0, 0];
const _mods: TerrainMod[] = [];

/**
 * Procedural boulders anywhere on a body: a few fractured-rock meshes generated at startup (each in
 * four levels of detail), scattered deterministically over the cube-sphere cells of its surface
 * (shared/space/rocks.ts) by the terrain workers, and drawn as two batches (the ones that cast sun
 * shadows and the rest). Every instance is culled against the view (and each shadow cascade) on its
 * own, takes the level of detail its distance asks for, and disappears when it would be under a
 * pixel. Tiles come and go as the viewer walks; nothing is rebuilt. The instances are relative to an
 * anchor near the viewer that hangs from the render origin's root, so float32 holds them anywhere.
 */
export class RockField {
  readonly group = new THREE.Group();
  private batches: Batch[];
  private tiles = new Map<number, RockTile>();
  private arrived: RockTile[] = [];
  /** Tile the viewer was in (face·2^40 + i·2^20 + j), -1: none yet. */
  private at = -1;
  /** The anchor (world) and the viewer relative to it when the LODs were last chosen. */
  private anchor = new THREE.Vector3(Infinity, 0, 0);
  private lodAt = new THREE.Vector3(Infinity, 0, Infinity);
  private surface: BodySurface;
  private tileLevel: number;
  private tileArc: number;

  constructor(
    private pool: TerrainWorkerPool,
    private body: CelestialBody,
    private seed: number,
    material: THREE.MeshStandardMaterial,
  ) {
    this.group.name = `Rocks:${body.def.id}`;
    this.group.matrixAutoUpdate = false;
    this.surface = surfaceOf(body, seed)!;
    this.tileLevel = rockTileLevel(this.surface);
    this.tileArc = rockTileArc(this.surface);
    // every variant at every level of detail (same shape: the same seed drives each)
    const geos: THREE.BufferGeometry[][] = [];
    let vertices = 0;
    let indices = 0;
    for (let v = 0; v < ROCK_VARIANTS; v++) {
      geos.push(
        LODS.map(([detail]) => {
          const g = makeRockGeometry(seed * 13 + v * 101, detail);
          vertices += g.getAttribute('position').count;
          indices += g.getIndex()!.count;
          return g;
        }),
      );
    }
    this.batches = [true, false].map((cast) => {
      const mesh = new THREE.BatchedMesh(MAX_ROCKS, vertices, indices, material);
      mesh.name = cast ? 'rocks-cast' : 'rocks';
      mesh.castShadow = cast;
      mesh.receiveShadow = true;
      // culled per instance (view and each shadow cascade); the object as a whole is not
      mesh.perObjectFrustumCulled = true;
      mesh.frustumCulled = false;
      mesh.sortObjects = false;
      mesh.matrixAutoUpdate = false;
      this.group.add(mesh);
      return {
        mesh,
        geo: geos.map((lods) => lods.map((g) => mesh.addGeometry(g))),
        x: new Float32Array(MAX_ROCKS),
        y: new Float32Array(MAX_ROCKS),
        z: new Float32Array(MAX_ROCKS),
        size: new Float32Array(MAX_ROCKS),
        variant: new Uint8Array(MAX_ROCKS),
        lod: new Int8Array(MAX_ROCKS).fill(-2),
        hi: 0,
        count: 0,
      };
    });
    for (const lods of geos) for (const g of lods) g.dispose();
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
          `#include <begin_vertex>
          vRockPos = position * 2.3;
          vRockNrm = normal;
          #if defined( USE_BATCHING )
            vRockRot = normalMatrix * mat3(batchingMatrix);
          #elif defined( USE_INSTANCING )
            vRockRot = normalMatrix * mat3(instanceMatrix);
          #else
            vRockRot = normalMatrix;
          #endif`,
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
    mat.customProgramCacheKey = () => 'lunar-rock-v2';
    return mat;
  }

  /** Stream the tiles round the viewer (world) and choose the levels of detail. */
  update(viewer: THREE.Vector3) {
    if (this.tileLevel < 0) return;
    if (this.anchor.distanceToSquared(viewer) > REANCHOR * REANCHOR) this.reanchor(viewer);
    const c = this.body.center;
    const f = faceOf(viewer.x - c[0], viewer.y - c[1], viewer.z - c[2], _fp);
    const here = f.face * 2 ** 40 + cellOf(f.a, this.tileLevel) * 2 ** 20 + cellOf(f.b, this.tileLevel);
    if (here !== this.at) {
      this.at = here;
      this.stream(viewer);
    }
    let added = false;
    if (this.arrived.length) {
      for (const t of this.arrived) {
        if (this.tiles.get(t.key) !== t || !t.data) continue;
        this.place(t, viewer);
        added = true;
      }
      this.arrived.length = 0;
    }
    if (added || this.lodAt.distanceToSquared(viewer) > LOD_STEP * LOD_STEP) {
      this.lodAt.copy(viewer);
      const vx = viewer.x - this.anchor.x;
      const vy = viewer.y - this.anchor.y;
      const vz = viewer.z - this.anchor.z;
      for (const b of this.batches) this.lods(b, vx, vy, vz);
    }
  }

  /**
   * The tiles wanted round the viewer: a grid laid on its horizon at half-tile steps, each point's
   * tile owned by the innermost ring that reaches it (the cube's faces and their seams need no care).
   */
  private stream(viewer: THREE.Vector3) {
    const c = this.body.center;
    const R = this.body.radius;
    const l = Math.hypot(viewer.x - c[0], viewer.y - c[1], viewer.z - c[2]) || 1;
    _tu[0] = (viewer.x - c[0]) / l;
    _tu[1] = (viewer.y - c[1]) / l;
    _tu[2] = (viewer.z - c[2]) / l;
    tangentAxes(this.body.pole, _tu, _te, _ts);
    const T = this.tileLevel;
    const outer = RINGS[RINGS.length - 1][0];
    const wanted = new Map<number, { face: number; ti: number; tj: number; ring: number }>();
    for (let hz = -outer * 2; hz <= outer * 2; hz++) {
      for (let hx = -outer * 2; hx <= outer * 2; hx++) {
        const ring = Math.ceil(Math.max(Math.abs(hx), Math.abs(hz)) / 2);
        const x = hx * 0.5 * this.tileArc;
        const z = hz * 0.5 * this.tileArc;
        const f = faceOf(_tu[0] * R + _te[0] * x + _ts[0] * z, _tu[1] * R + _te[1] * x + _ts[1] * z, _tu[2] * R + _te[2] * x + _ts[2] * z, _fp);
        const ti = cellOf(f.a, T);
        const tj = cellOf(f.b, T);
        const id = f.face * 2 ** 40 + ti * 2 ** 20 + tj;
        const w = wanted.get(id);
        if (!w) wanted.set(id, { face: f.face, ti, tj, ring });
        else if (ring < w.ring) w.ring = ring;
      }
    }
    // a tile's key carries the smallest rock size it holds: moving to another ring asks again
    const keep = new Set<number>();
    for (const [id, w] of wanted) {
      const r = RINGS.findIndex(([rr]) => w.ring <= rr);
      const key = id * 4 + r;
      keep.add(key);
      if (!this.tiles.has(key)) this.request(key, w.face, w.ti, w.tj, r, w.ring);
    }
    for (const [k, t] of this.tiles) {
      if (keep.has(k)) continue;
      this.drop(t);
      this.tiles.delete(k);
    }
  }

  /** Re-scatter the tiles a modifier reaches (their rocks follow the new ground). */
  invalidate(mod: TerrainMod) {
    if (mod.body !== this.body.def.id) return;
    for (const [k, t] of this.tiles) {
      if (!modTouches(mod, this.body, t.dir, this.tileArc)) continue;
      this.drop(t);
      this.tiles.delete(k);
    }
    this.at = -1;
  }

  private request(key: number, face: number, ti: number, tj: number, ring: number, dist: number) {
    const n = 2 ** this.tileLevel;
    const dir = cubeDir(face, ((ti + 0.5) / n) * 2 - 1, ((tj + 0.5) / n) * 2 - 1, [0, 0, 0]) as [number, number, number];
    const tile: RockTile = { key, face, ti, tj, ring, dir, data: null, center: [0, 0, 0], ids: [] };
    this.tiles.set(key, tile);
    _mods.length = 0;
    this.surface.mods.near(dir, this.tileArc * 0.75, 0, _mods);
    const job = this.pool.run({ kind: 'rocks', body: this.body.def.id, seed: this.seed, mods: _mods.slice(), face, ti, tj, minSize: RINGS[ring][1], maxSize: 1e9 }, 2 + dist * 0.5);
    tile.cancel = job.cancel;
    job.promise.then((r) => {
      if (r.kind !== 'rocks' || this.tiles.get(key) !== tile) return;
      tile.data = r.rocks;
      tile.center = r.center;
      this.arrived.push(tile);
    });
  }

  /** A tile's rocks into the batches (relative to the anchor). */
  private place(t: RockTile, viewer: THREE.Vector3) {
    const d = t.data!;
    const castFrom = t.ring === 0 ? CAST_INNER : t.ring === 1 ? CAST_MID : Infinity;
    const ox = t.center[0] - this.anchor.x;
    const oy = t.center[1] - this.anchor.y;
    const oz = t.center[2] - this.anchor.z;
    const vx = viewer.x - this.anchor.x;
    const vy = viewer.y - this.anchor.y;
    const vz = viewer.z - this.anchor.z;
    for (let i = 0; i < d.length; i += STRIDE) {
      const size = d[i + 6];
      const bi = size >= castFrom ? 0 : 1;
      const b = this.batches[bi];
      if (b.count >= MAX_ROCKS) continue;
      const v = Math.floor(d[i + 8]);
      const j = (d[i + 8] - v) * 2;
      _up.set(d[i + 3], d[i + 4], d[i + 5]);
      // sunk a third of its height into the regolith, along the local vertical
      const x = ox + d[i] - _up.x * size * 0.3;
      const y = oy + d[i + 1] - _up.y * size * 0.3;
      const z = oz + d[i + 2] - _up.z * size * 0.3;
      const lod = this.lodFor(size, x - vx, y - vy, z - vz);
      const id = b.mesh.addInstance(b.geo[v][Math.max(0, lod)]);
      _e.set((j - 0.5) * 0.5, d[i + 7], (fract(j * 7.31) - 0.5) * 0.5);
      _q.setFromUnitVectors(_Y, _up).multiply(_qu.setFromEuler(_e));
      _p.set(x, y, z);
      _s.set(size * (0.85 + 0.3 * fract(j * 3.7)), size * (0.6 + 0.35 * fract(j * 5.3)), size * (0.85 + 0.3 * fract(j * 9.1)));
      b.mesh.setMatrixAt(id, _m.compose(_p, _q, _s));
      if (lod < 0) b.mesh.setVisibleAt(id, false);
      b.x[id] = x;
      b.y[id] = y;
      b.z[id] = z;
      b.size[id] = size;
      b.variant[id] = v;
      b.lod[id] = lod;
      b.hi = Math.max(b.hi, id + 1);
      b.count++;
      t.ids.push(bi, id);
    }
  }

  /** A new anchor at the viewer (whole metres): every instance moves by the difference, in place. */
  private reanchor(viewer: THREE.Vector3) {
    const nx = Math.round(viewer.x);
    const ny = Math.round(viewer.y);
    const nz = Math.round(viewer.z);
    const fresh = !Number.isFinite(this.anchor.x);
    const dx = fresh ? 0 : this.anchor.x - nx;
    const dy = fresh ? 0 : this.anchor.y - ny;
    const dz = fresh ? 0 : this.anchor.z - nz;
    this.anchor.set(nx, ny, nz);
    this.group.position.copy(this.anchor);
    this.group.updateMatrix();
    this.group.updateMatrixWorld(true);
    if (fresh) return;
    for (const b of this.batches) {
      for (let id = 0; id < b.hi; id++) {
        if (b.lod[id] === -2) continue;
        b.x[id] += dx;
        b.y[id] += dy;
        b.z[id] += dz;
        b.mesh.getMatrixAt(id, _m);
        _m.elements[12] += dx;
        _m.elements[13] += dy;
        _m.elements[14] += dz;
        b.mesh.setMatrixAt(id, _m);
      }
    }
  }

  /** A tile's rocks out of the batches (and its pending job cancelled). */
  private drop(t: RockTile) {
    t.cancel?.();
    for (let k = 0; k < t.ids.length; k += 2) {
      const b = this.batches[t.ids[k]];
      const id = t.ids[k + 1];
      b.mesh.deleteInstance(id);
      b.lod[id] = -2;
      b.count--;
    }
    t.ids.length = 0;
  }

  /** Level of detail for a rock of `size` at (dx, dy, dz) from the viewer; -1 = too small to draw. */
  private lodFor(size: number, dx: number, dy: number, dz: number) {
    return rockLod(size, Math.sqrt(dx * dx + dy * dy + dz * dz));
  }

  /** Choose every instance's level of detail again (the viewer moved; its position relative to the anchor). */
  private lods(b: Batch, vx: number, vy: number, vz: number) {
    const mesh = b.mesh;
    for (let id = 0; id < b.hi; id++) {
      const was = b.lod[id];
      if (was === -2) continue;
      const lod = this.lodFor(b.size[id], b.x[id] - vx, b.y[id] - vy, b.z[id] - vz);
      if (lod === was) continue;
      b.lod[id] = lod;
      if (lod < 0) {
        mesh.setVisibleAt(id, false);
        continue;
      }
      if (was < 0) mesh.setVisibleAt(id, true);
      mesh.setGeometryIdAt(id, b.geo[b.variant[id]][lod]);
    }
  }
}

const fract = (x: number) => x - Math.floor(x);

/** Fractured boulder: squashed sphere + random planar breaks + multi-scale bumps (`detail`: its subdivision). */
function makeRockGeometry(seed: number, detail = 5): THREE.BufferGeometry {
  const rnd = mulberry32(seed);
  let geo: THREE.BufferGeometry = new THREE.IcosahedronGeometry(1, detail);
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

/** Level of detail of a rock of `size` at `dist` m from the viewer (index in ROCK_LODS; -1: not drawn). */
export function rockLod(size: number, dist: number): number {
  if (size < dist * MIN_ANGLE) return -1;
  // bigger rocks keep their detail further out
  const r = dist / Math.max(0.5, Math.min(3, size * 1.5));
  for (let k = 0; k < LODS.length; k++) if (r <= LODS[k][1]) return k;
  return LODS.length - 1;
}

/** Triangles of each LOD's mesh (icosahedron subdivisions). */
export const ROCK_LOD_TRIANGLES = LODS.map(([detail]) => 20 * (detail + 1) ** 2);
/** The rings (tile radius, smallest rock) and who casts shadows in them (for tools/perf). */
export const ROCK_RINGS = RINGS;
export const ROCK_CASTERS = [CAST_INNER, CAST_MID, Infinity];
