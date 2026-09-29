/**
 * Measuring helpers for the perf tool: wall time per call and bytes allocated per call (V8's
 * sampling heap profiler, counting what the minor and major GCs already collected — the garbage a
 * frame leaves behind is exactly what we want to see), and scene-graph counts as the renderer
 * would issue them (before culling).
 */
import { Session } from 'node:inspector/promises';
import * as THREE from 'three';

export interface Alloc {
  /** Estimated bytes allocated per call. */
  bytes: number;
  /** Biggest allocators (function @ file:line → bytes per call). */
  top: Array<[string, number]>;
}

let session: Session | null = null;

async function inspector() {
  if (!session) {
    session = new Session();
    session.connect();
    await session.post('HeapProfiler.enable');
  }
  return session;
}

export function close() {
  session?.disconnect();
  session = null;
}

/** Milliseconds per call (after a warm-up). */
export function timeIt(fn: () => void, n = 400, warm = 60) {
  for (let i = 0; i < warm; i++) fn();
  const t0 = performance.now();
  for (let i = 0; i < n; i++) fn();
  return (performance.now() - t0) / n;
}

interface ProfileNode {
  callFrame: { functionName: string; url: string; lineNumber: number };
  selfSize: number;
  children: ProfileNode[];
}

/** Bytes allocated per call, and by whom. */
export async function allocIt(fn: () => void, n = 300, warm = 60): Promise<Alloc> {
  for (let i = 0; i < warm; i++) fn();
  const s = await inspector();
  await s.post('HeapProfiler.startSampling', { samplingInterval: 64, includeObjectsCollectedByMajorGC: true, includeObjectsCollectedByMinorGC: true } as never);
  for (let i = 0; i < n; i++) fn();
  const { profile } = (await s.post('HeapProfiler.stopSampling')) as unknown as { profile: { head: ProfileNode } };
  let total = 0;
  const by = new Map<string, number>();
  const walk = (node: ProfileNode) => {
    total += node.selfSize;
    if (node.selfSize > 0) {
      const f = node.callFrame;
      const file = f.url.replace(/^.*\/src\//, '').replace(/^.*node_modules\//, '');
      const key = `${f.functionName || '(anónima)'} @ ${file}:${f.lineNumber + 1}`;
      by.set(key, (by.get(key) ?? 0) + node.selfSize);
    }
    for (const c of node.children) walk(c);
  };
  walk(profile.head);
  const top = [...by.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6).map(([k, v]): [string, number] => [k, v / n]);
  return { bytes: total / n, top };
}

export interface SceneStats {
  meshes: number;
  /** Draw calls of the main pass (a multi-material mesh issues one per group). */
  draws: number;
  /** Draw calls that also go into every shadow cascade. */
  shadowDraws: number;
  triangles: number;
  materials: number;
  programs: number;
  unculled: number;
  byName: Map<string, number>;
}

function triangles(o: THREE.Mesh) {
  const g = o.geometry;
  const n = g.index ? g.index.count : g.getAttribute('position')?.count ?? 0;
  const range = Math.min(n, g.drawRange.count);
  const inst = (o as THREE.InstancedMesh).isInstancedMesh ? (o as THREE.InstancedMesh).count : 1;
  if ((o as unknown as THREE.BatchedMesh).isBatchedMesh) return 0;
  return (range / 3) * inst;
}

/** What the renderer would draw of `root` (visible objects only, no culling). */
export function sceneStats(root: THREE.Object3D): SceneStats {
  const st: SceneStats = { meshes: 0, draws: 0, shadowDraws: 0, triangles: 0, materials: 0, programs: 0, unculled: 0, byName: new Map() };
  const mats = new Set<THREE.Material>();
  const programs = new Set<string>();
  root.traverseVisible((o) => {
    const m = o as THREE.Mesh;
    if (!m.isMesh && !(o as THREE.Points).isPoints && !(o as THREE.Line).isLine) return;
    if ((m as unknown as THREE.InstancedMesh).isInstancedMesh && (m as unknown as THREE.InstancedMesh).count === 0) return;
    const list = Array.isArray(m.material) ? m.material : [m.material];
    const groups = Array.isArray(m.material) ? m.geometry.groups.filter((g) => list[g.materialIndex ?? 0]?.visible !== false).length : list[0].visible ? 1 : 0;
    if (!groups) return;
    st.meshes++;
    st.draws += groups;
    if (m.castShadow) st.shadowDraws += groups;
    st.triangles += triangles(m);
    if (!m.frustumCulled) st.unculled++;
    const top = topName(o, root);
    st.byName.set(top, (st.byName.get(top) ?? 0) + groups);
    for (const mat of list) {
      mats.add(mat);
      const key = [mat.type, (mat as THREE.ShaderMaterial).isShaderMaterial ? (mat as THREE.ShaderMaterial).fragmentShader.length : '', mat.customProgramCacheKey(), (mat as THREE.MeshStandardMaterial).vertexColors, JSON.stringify(mat.defines ?? {}), (m as unknown as THREE.InstancedMesh).isInstancedMesh ? 'inst' : '', (m as unknown as THREE.BatchedMesh).isBatchedMesh ? 'batch' : ''].join('|');
      programs.add(key);
    }
  });
  st.materials = mats.size;
  st.programs = programs.size;
  return st;
}

/** Name of the direct child of `root` an object hangs from (what kind of thing it is). */
function topName(o: THREE.Object3D, root: THREE.Object3D) {
  let p = o;
  while (p.parent && p.parent !== root) p = p.parent;
  const n = p.name || p.type;
  return n.replace(/[:-].*$/, '');
}

export const kb = (b: number) => `${(b / 1024).toFixed(b < 10 * 1024 ? 1 : 0)} KB`;
export const ms = (v: number) => `${v.toFixed(v < 0.1 ? 3 : 2)} ms`;
