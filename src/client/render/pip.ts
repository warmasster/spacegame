// Picture-in-picture infrastructure: reusable in monitors, mirrors and other host adapters.
// See docs/CAMERAS.md. It knows no ships, seats, weapons or gameplay commands.
import * as THREE from 'three';
import { cameraBudget, type CameraDisplayDef, type CameraBudget } from '../../shared/screens';
import { ImageMaterial } from './imageEffects';

export interface PipStatus { title: string; detail: string; hint: string; warning: boolean; locked: boolean }
export interface PipSource {
  readonly camera: THREE.PerspectiveCamera;
  enabled: boolean;
  live: boolean;
  readonly status: PipStatus;
  /** Place the camera in render space, respecting the floating origin. Called only when captured. */
  prepare(): void;
  update?(): void;
  dispose?(): void;
}

/** Hooks restore view-specific culling/streaming even when a capture fails. */
export interface CaptureScope { begin(camera: THREE.PerspectiveCamera): void; end(): void }

const BLACK = new THREE.DataTexture(new Uint8Array([0, 0, 0, 255]), 1, 1);
BLACK.needsUpdate = true;
const _view = new THREE.Matrix4();
const _frustum = new THREE.Frustum();
const _eye = new THREE.Vector3();
const _point = new THREE.Vector3();
const _normal = new THREE.Vector3();
const _viewport = new THREE.Vector4();
const _scissor = new THREE.Vector4();

/** A surface and a transparent status overlay; reused by any camera provider. */
export class PipDisplay {
  readonly root = new THREE.Group();
  readonly surface: THREE.Mesh<THREE.PlaneGeometry, ImageMaterial>;
  readonly budget: CameraBudget;
  source: PipSource | null = null;
  lastCapture = -Infinity;
  lastVisible = -Infinity;
  private overlay: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>;
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private textKey = '';
  private textSlot = -1;
  private image: THREE.Texture = BLACK;

  constructor(w: number, h: number, def: CameraDisplayDef) {
    this.budget = cameraBudget(def);
    this.surface = new THREE.Mesh(new THREE.PlaneGeometry(w, h), new ImageMaterial({ map: BLACK, toneMapped: false, color: 0x000000 }, def.effects, this.budget.width, this.budget.height));
    this.canvas = document.createElement('canvas');
    this.canvas.width = 512;
    this.canvas.height = Math.round(512 * h / w);
    this.ctx = this.canvas.getContext('2d')!;
    const tex = new THREE.CanvasTexture(this.canvas);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.generateMipmaps = false;
    tex.minFilter = THREE.LinearFilter;
    this.overlay = new THREE.Mesh(new THREE.PlaneGeometry(w, h), new THREE.MeshBasicMaterial({ map: tex, transparent: true, depthWrite: false, toneMapped: false }));
    this.overlay.position.z = 0.001;
    this.root.add(this.surface, this.overlay);
    this.root.name = 'camera-display';
    this.root.userData.cameraDisplay = true;
  }

  /** Target textures swap without changing shader features (there is always a map). */
  texture(texture: THREE.Texture) {
    this.image = texture;
    this.surface.material.map = texture;
  }

  update(now: number) {
    const source = this.source;
    const on = !!source?.enabled;
    if (on) this.surface.material.time(now);
    this.overlay.visible = on;
    const live = !!(on && source?.live && this.image !== BLACK);
    this.surface.material.active(live); // Noise effects must not illuminate an unpowered screen.
    this.surface.material.color.setScalar(live ? 1 : 0);
    if (!on || !source || Math.floor(now * 4) === this.textSlot) return;
    this.textSlot = Math.floor(now * 4);
    const s = source.status;
    const key = s.title + '|' + s.detail + '|' + s.hint + '|' + s.warning + '|' + s.locked + '|' + source.live;
    if (key === this.textKey) return;
    this.textKey = key;
    const g = this.ctx, w = this.canvas.width, h = this.canvas.height;
    g.clearRect(0, 0, w, h);
    g.fillStyle = 'rgba(0,8,14,.84)';
    g.fillRect(0, 0, w, 39);
    g.fillRect(0, h - 63, w, 63);
    g.font = 'bold 19px ui-monospace, Consolas, monospace';
    g.fillStyle = s.warning ? '#ffc04a' : '#8ff0c8';
    g.fillText(s.title, 12, 26, w - 24);
    g.font = 'bold 17px ui-monospace, Consolas, monospace';
    g.fillText(s.detail, 12, h - 38, w - 24);
    g.font = '14px ui-monospace, Consolas, monospace';
    g.fillStyle = '#bfefff';
    g.fillText(s.hint, 12, h - 13, w - 24);
    if (source.live) {
      g.strokeStyle = s.locked ? '#ffc04a' : '#bfefff';
      g.lineWidth = 2;
      g.beginPath();
      g.moveTo(w / 2 - 12, h / 2); g.lineTo(w / 2 + 12, h / 2);
      g.moveTo(w / 2, h / 2 - 12); g.lineTo(w / 2, h / 2 + 12);
      g.stroke();
    }
    this.overlay.material.map!.needsUpdate = true;
  }

  dispose() {
    this.root.removeFromParent();
    this.surface.geometry.dispose(); this.surface.material.dispose();
    this.overlay.geometry.dispose(); this.overlay.material.map!.dispose(); this.overlay.material.dispose();
  }
}

interface TargetSlot { target: THREE.WebGLRenderTarget; owner: PipDisplay }

/** Global budget: at most one capture per frame, four retained targets, no post chain or new shadows. */
export class PipSystem {
  readonly displays: PipDisplay[] = [];
  private targets: TargetSlot[] = [];
  private hidden: boolean[] = [];
  private cursor = 0;
  captures = 0;
  private time = 0;
  private observer: THREE.Camera | null = null;

  constructor(private renderer: THREE.WebGLRenderer, private scene: THREE.Scene, private scope?: CaptureScope) {}

  add(display: PipDisplay) { this.displays.push(display); this.hidden.push(false); }
  request(now: number, observer: THREE.Camera) { this.time = now; this.observer = observer; }
  renderPending() { if (this.observer) this.frame(this.time, this.observer); }

  private readable(display: PipDisplay, eye: THREE.Vector3) {
    for (let p: THREE.Object3D | null = display.root; p; p = p.parent) if (!p.visible) return false;
    _point.setFromMatrixPosition(display.surface.matrixWorld);
    const dx = eye.x - _point.x, dy = eye.y - _point.y, dz = eye.z - _point.z;
    if (dx * dx + dy * dy + dz * dz > display.budget.reach * display.budget.reach) return false;
    _normal.setFromMatrixColumn(display.surface.matrixWorld, 2);
    return _normal.x * dx + _normal.y * dy + _normal.z * dz > 0 && _frustum.intersectsObject(display.surface);
  }

  private lease(display: PipDisplay, now: number): THREE.WebGLRenderTarget | null {
    for (const s of this.targets) if (s.owner === display) return s.target;
    let slot: TargetSlot | undefined;
    for (const s of this.targets) if (!s.owner.source?.live || now - s.owner.lastVisible > 5) { slot = s; break; }
    if (!slot) {
      if (this.targets.length >= 4) return null;
      const target = new THREE.WebGLRenderTarget(display.budget.width, display.budget.height, { depthBuffer: true, stencilBuffer: false, minFilter: THREE.LinearFilter, magFilter: THREE.LinearFilter });
      target.texture.colorSpace = THREE.SRGBColorSpace;
      target.texture.generateMipmaps = false;
      slot = { target, owner: display };
      this.targets.push(slot);
    } else {
      slot.owner.texture(BLACK);
      slot.owner.lastCapture = -Infinity;
      slot.owner = display;
      slot.target.setSize(display.budget.width, display.budget.height);
    }
    display.texture(slot.target.texture);
    return slot.target;
  }

  frame(now: number, observer: THREE.Camera) {
    observer.updateMatrixWorld();
    _eye.setFromMatrixPosition(observer.matrixWorld);
    _view.multiplyMatrices(observer.projectionMatrix, observer.matrixWorldInverse);
    _frustum.setFromProjectionMatrix(_view);
    for (const d of this.displays) { d.surface.updateWorldMatrix(true, false); d.update(now); if (this.readable(d, _eye)) d.lastVisible = now; }
    const n = this.displays.length;
    for (let k = 0; k < n; k++) {
      const i = (this.cursor + k) % n, d = this.displays[i], source = d.source;
      if (!source?.enabled || !source.live || d.lastVisible !== now || now - d.lastCapture < 1 / d.budget.fps) continue;
      const target = this.lease(d, now);
      if (!target) continue;
      this.cursor = (i + 1) % n;
      source.prepare();
      const r = this.renderer, previous = r.getRenderTarget(), face = r.getActiveCubeFace(), level = r.getActiveMipmapLevel();
      r.getViewport(_viewport); r.getScissor(_scissor);
      const scissor = r.getScissorTest(), auto = r.autoClear, shadow = r.shadowMap.autoUpdate, shadowDirty = r.shadowMap.needsUpdate, tone = r.toneMapping;
      for (let j = 0; j < n; j++) { this.hidden[j] = this.displays[j].root.visible; this.displays[j].root.visible = false; }
      try {
        this.scope?.begin(source.camera);
        r.shadowMap.autoUpdate = false;
        r.shadowMap.needsUpdate = false;
        r.toneMapping = THREE.ACESFilmicToneMapping;
        r.autoClear = true;
        r.setRenderTarget(target);
        r.setScissorTest(false);
        r.render(this.scene, source.camera);
        d.lastCapture = now;
        d.surface.material.color.setScalar(1);
        d.surface.material.active(true);
        this.captures++;
      } finally {
        try { this.scope?.end(); } finally {
          for (let j = 0; j < n; j++) this.displays[j].root.visible = this.hidden[j];
          r.shadowMap.autoUpdate = shadow; r.shadowMap.needsUpdate = shadowDirty; r.toneMapping = tone; r.autoClear = auto;
          r.setRenderTarget(previous, face, level); r.setViewport(_viewport); r.setScissor(_scissor); r.setScissorTest(scissor);
        }
      }
      break;
    }
  }

  dispose() { for (const s of this.targets) s.target.dispose(); for (const d of this.displays) d.dispose(); this.targets.length = this.displays.length = this.hidden.length = 0; }
}
