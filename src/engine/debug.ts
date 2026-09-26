import * as THREE from 'three';

export interface DebugStats {
  [label: string]: string | number;
}

/**
 * In-game diagnostics (F3): frame timing, GPU counters, simulation and streaming stats.
 * F4 terrain wireframe · F5 physics colliders.
 */
export class DebugOverlay {
  readonly el: HTMLDivElement;
  visible = false;
  wireframe = false;
  physicsLines = false;
  private frameMs: number[] = [];
  private lines: THREE.LineSegments;

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
  }

  /** Rapier debug render buffers → line segments. */
  setPhysicsLines(buffers: { vertices: Float32Array; colors: Float32Array } | null) {
    this.lines.visible = !!buffers;
    if (!buffers) return;
    const g = this.lines.geometry;
    g.setAttribute('position', new THREE.BufferAttribute(buffers.vertices, 3));
    g.setAttribute('color', new THREE.BufferAttribute(buffers.colors, 4));
  }

  update(renderer: THREE.WebGLRenderer, stats: DebugStats) {
    if (!this.visible) return;
    const f = this.frameMs;
    const avg = f.reduce((a, b) => a + b, 0) / Math.max(1, f.length);
    const max = Math.max(...f, 0);
    const info = renderer.info;
    const rows: DebugStats = {
      'frame ms (avg/max)': `${avg.toFixed(1)} / ${max.toFixed(1)}`,
      fps: Math.round(1000 / Math.max(avg, 0.01)),
      'draw calls': info.render.calls,
      triangles: info.render.triangles.toLocaleString(),
      geometries: info.memory.geometries,
      textures: info.memory.textures,
      programs: info.programs?.length ?? 0,
      ...stats,
    };
    this.el.innerHTML =
      '<b>DIAGNÓSTICO</b>' +
      Object.entries(rows)
        .map(([k, v]) => `<div><span>${k}</span><i>${v}</i></div>`)
        .join('') +
      `<small>F4 alambre ${this.wireframe ? 'ON' : 'off'} · F5 colisiones ${this.physicsLines ? 'ON' : 'off'}</small>`;
  }
}
