import * as THREE from 'three';

export interface DebugStats {
  [label: string]: string | number;
}

/**
 * In-game diagnostics (F3): frame timing, GPU counters, simulation and streaming stats.
 * F4 terrain wireframe · F5 physics colliders.
 */
/** Category of an object for the draw-call breakdown: `userData.cat` on it or on an ancestor. */
function categoryOf(o: THREE.Object3D, cache: WeakMap<THREE.Object3D, string>): string {
  const hit = cache.get(o);
  if (hit !== undefined) return hit;
  const own = o.userData.cat as string | undefined;
  const cat = own ?? (o.parent ? categoryOf(o.parent, cache) : 'otros');
  cache.set(o, cat);
  return cat;
}

export class DebugOverlay {
  readonly el: HTMLDivElement;
  visible = false;
  wireframe = false;
  physicsLines = false;
  private frameMs: number[] = [];
  private lines: THREE.LineSegments;
  /** Draw calls this frame by category, main view and shadow maps (only counted while F3 is open). */
  private drawn = new Map<string, { main: number; shadow: number; tris: number }>();
  private catCache = new WeakMap<THREE.Object3D, string>();
  private hooked: THREE.WebGLRenderer | null = null;
  /** Texture uploads (texImage/texSubImage) counted per second. */
  private uploads = 0;
  private uploadsRate = 0;
  /** JS heap: last reading and the garbage rate (sum of the rises, per second). */
  private heapLast = 0;
  private heapRise = 0;
  private heapRate = 0;
  private windowT = 0;
  private shown = 0;

  constructor(parent: HTMLElement, scene: THREE.Scene) {
    this.el = document.createElement('div');
    this.el.className = 'debug-overlay hidden';
    parent.appendChild(this.el);
    this.lines = new THREE.LineSegments(
      new THREE.BufferGeometry(),
      new THREE.LineBasicMaterial({ vertexColors: true, depthTest: true, transparent: true, opacity: 0.8 }),
    );
    this.lines.frustumCulled = false;
    this.lines.visible = false;
    scene.add(this.lines);
    window.addEventListener('keydown', (e) => {
      if (e.code === 'F3') this.visible = !this.visible;
      else if (e.code === 'F4') this.wireframe = !this.wireframe;
      else if (e.code === 'F5') this.physicsLines = !this.physicsLines;
      else return;
      e.preventDefault();
      this.el.classList.toggle('hidden', !this.visible);
    });
  }

  frame(ms: number) {
    this.frameMs.push(ms);
    if (this.frameMs.length > 120) this.frameMs.shift();
    // heap and uploads, per second (performance.memory: Chromium only)
    const mem = (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory;
    if (mem) {
      const h = mem.usedJSHeapSize;
      if (this.heapLast && h > this.heapLast) this.heapRise += h - this.heapLast;
      this.heapLast = h;
    }
    this.windowT += ms;
    if (this.windowT >= 1000) {
      this.heapRate = (this.heapRise * 1000) / this.windowT;
      this.uploadsRate = (this.uploads * 1000) / this.windowT;
      this.heapRise = 0;
      this.uploads = 0;
      this.windowT = 0;
    }
  }

  /**
   * Count every draw call of the frame by category and pass (main / shadow), and every texture
   * upload. Hooks the renderer once; the counting only runs while the overlay is open.
   */
  private hook(renderer: THREE.WebGLRenderer) {
    if (this.hooked === renderer) return;
    this.hooked = renderer;
    const r = renderer as unknown as {
      renderBufferDirect: (camera: THREE.Camera, scene: THREE.Scene | null, geometry: THREE.BufferGeometry, material: THREE.Material, object: THREE.Object3D, group: unknown) => void;
    };
    const direct = r.renderBufferDirect.bind(renderer);
    r.renderBufferDirect = (camera, scene, geometry, material, object, group) => {
      if (this.visible) {
        const cat = categoryOf(object, this.catCache);
        let d = this.drawn.get(cat);
        if (!d) this.drawn.set(cat, (d = { main: 0, shadow: 0, tris: 0 }));
        const m = material as THREE.Material & { isMeshDepthMaterial?: boolean; isMeshDistanceMaterial?: boolean };
        const shadow = !!(m.isMeshDepthMaterial || m.isMeshDistanceMaterial || (object as THREE.Mesh).customDepthMaterial === material || (object as THREE.Mesh).customDistanceMaterial === material);
        if (shadow) d.shadow++;
        else d.main++;
        const index = geometry.index;
        const count = index ? index.count : (geometry.attributes.position?.count ?? 0);
        const inst = (object as THREE.InstancedMesh).isInstancedMesh ? (object as THREE.InstancedMesh).count : 1;
        d.tris += (count / 3) * inst;
      }
      direct(camera, scene, geometry, material, object, group);
    };
    const gl = renderer.getContext() as WebGL2RenderingContext;
    const count = <K extends 'texImage2D' | 'texSubImage2D' | 'texImage3D' | 'texSubImage3D' | 'compressedTexImage2D'>(name: K) => {
      const f = (gl[name] as (...a: unknown[]) => void).bind(gl);
      (gl as unknown as Record<string, unknown>)[name] = (...a: unknown[]) => {
        this.uploads++;
        return f(...a);
      };
    };
    count('texImage2D');
    count('texSubImage2D');
    count('texImage3D');
    count('texSubImage3D');
    count('compressedTexImage2D');
  }

  /** Rapier debug render buffers → line segments. */
  /** Rapier's debug lines; `frame`: where that world's coordinates sit in the scene (render space). */
  setPhysicsLines(buffers: { vertices: Float32Array; colors: Float32Array } | null, frame?: THREE.Matrix4) {
    this.lines.visible = !!buffers;
    if (!buffers) return;
    if (frame) {
      this.lines.matrixAutoUpdate = false;
      this.lines.matrix.copy(frame);
      this.lines.matrixWorld.copy(frame);
    }
    const g = this.lines.geometry;
    g.setAttribute('position', new THREE.BufferAttribute(buffers.vertices, 3));
    g.setAttribute('color', new THREE.BufferAttribute(buffers.colors, 4));
  }

  /**
   * Call once per frame after rendering. The counters are the whole frame (every pass of the post
   * chain and the shadow maps: the pipeline resets `renderer.info` itself at the frame start).
   */
  update(renderer: THREE.WebGLRenderer, stats: DebugStats) {
    this.hook(renderer);
    if (!this.visible) {
      this.drawn.clear();
      return;
    }
    // the text at ~5 Hz; the per-category counts are the last frame's
    const now = performance.now();
    if (now - this.shown < 200) {
      for (const d of this.drawn.values()) d.main = d.shadow = d.tris = 0;
      return;
    }
    this.shown = now;
    const f = this.frameMs;
    let sum = 0;
    let max = 0;
    for (const v of f) {
      sum += v;
      max = Math.max(max, v);
    }
    const avg = sum / Math.max(1, f.length);
    const info = renderer.info;
    let main = 0;
    let shadow = 0;
    const cats = [...this.drawn.entries()].sort((a, b) => b[1].main + b[1].shadow - (a[1].main + a[1].shadow));
    for (const [, d] of cats) {
      main += d.main;
      shadow += d.shadow;
    }
    const rows: DebugStats = {
      'frame ms (avg/max)': `${avg.toFixed(1)} / ${max.toFixed(1)}`,
      fps: Math.round(1000 / Math.max(avg, 0.01)),
      'draw calls (frame)': `${info.render.calls} · escena ${main} · sombras ${shadow}`,
      'triángulos (frame)': info.render.triangles.toLocaleString(),
      geometries: info.memory.geometries,
      textures: info.memory.textures,
      'subidas de textura/s': Math.round(this.uploadsRate),
      programs: info.programs?.length ?? 0,
      'heap JS': this.heapLast ? `${(this.heapLast / 1048576).toFixed(0)} MB · basura ~${(this.heapRate / 1048576).toFixed(1)} MB/s` : 'n/d',
      ...stats,
    };
    let html = '<b>DIAGNÓSTICO</b>';
    for (const k in rows) html += `<div><span>${k}</span><i>${rows[k]}</i></div>`;
    html += '<b>DRAWS POR CATEGORÍA (escena / sombras · triángulos)</b>';
    for (const [cat, d] of cats) html += `<div><span>${cat}</span><i>${d.main} / ${d.shadow} · ${Math.round(d.tris).toLocaleString()}</i></div>`;
    html += `<small>F4 alambre ${this.wireframe ? 'ON' : 'off'} · F5 colisiones ${this.physicsLines ? 'ON' : 'off'}</small>`;
    this.el.innerHTML = html;
    for (const d of this.drawn.values()) d.main = d.shadow = d.tris = 0;
  }
}
