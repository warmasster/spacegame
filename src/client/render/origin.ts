import * as THREE from 'three';

/**
 * Floating origin of the render (AUDITORIA §6.2). The game works in world coordinates (float64 on
 * the CPU: exact to fractions of a millimetre anywhere in a star system), but the GPU works in
 * float32, where a vertex millions of metres from the origin is off by decimetres: skinned suits
 * burst, instanced crates shake, shadows swim. So the scene is drawn relative to a point O near the
 * camera ("render space" = world − O), re-centred when the camera wanders off it.
 *
 * How it is laid out:
 * - `root` holds everything placed in world coordinates (terrain, ships, astronauts, rockets, the
 *   camera itself…): their `.position` stays in world coordinates and `root`'s transform (−O) puts
 *   them in render space. Nothing else has to know.
 * - Anything read back from three.js — `matrixWorld`, `getWorldPosition`, `localToWorld` — is in
 *   render space: game logic converts with `toWorld` (or keeps its own world matrices, as the ships
 *   do). Directions are the same in both spaces (the origin only translates).
 * - What writes GPU buffers directly in world terms (instanced crates and debris, particles) writes
 *   `world − O` itself and lives under the scene, not under `root`.
 *
 * O moves in whole metres, so it is exact in float32 up to ~16 000 km and shaders that need
 * absolute coordinates near the base (texture coordinates of the terrain) can add `uniform.value`.
 */
class RenderOrigin {
  /** World coordinates of render-space zero (whole metres). */
  x = 0;
  y = 0;
  z = 0;
  /** Parent of everything placed in world coordinates (−O). */
  readonly root = new THREE.Group();
  /** O for shaders (`uOrigin`): render position + uOrigin = world position. */
  readonly uniform = { value: new THREE.Vector3() };
  /** Bumped on every re-centring (whoever caches render-space values checks it). */
  version = 0;
  /** Listeners told about each re-centring (the shift of render space, world metres). */
  private moved: Array<(dx: number, dy: number, dz: number) => void> = [];

  constructor() {
    this.root.name = 'world';
    this.root.matrixAutoUpdate = false;
  }

  /**
   * Keep O within `reach` metres of the camera (world). A re-centring recomputes every matrix under
   * `root` right away, so whatever reads them this frame sees the new space.
   */
  follow(cam: { x: number; y: number; z: number }, reach = 1000) {
    const dx = cam.x - this.x;
    const dy = cam.y - this.y;
    const dz = cam.z - this.z;
    if (dx * dx + dy * dy + dz * dz < reach * reach) return false;
    this.set(Math.round(cam.x), Math.round(cam.y), Math.round(cam.z));
    return true;
  }

  set(x: number, y: number, z: number) {
    const dx = x - this.x;
    const dy = y - this.y;
    const dz = z - this.z;
    if (!dx && !dy && !dz) return;
    this.x = x;
    this.y = y;
    this.z = z;
    this.uniform.value.set(x, y, z);
    this.root.matrix.makeTranslation(-x, -y, -z);
    this.root.matrixWorldNeedsUpdate = true;
    this.root.updateMatrixWorld(true);
    this.version++;
    for (const f of this.moved) f(dx, dy, dz);
  }

  /** Called with the shift (world metres) whenever O moves: render-space buffers move by −shift. */
  onMove(f: (dx: number, dy: number, dz: number) => void) {
    this.moved.push(f);
  }

  /** Render space → world, in place. */
  toWorld<T extends { x: number; y: number; z: number }>(v: T): T {
    v.x += this.x;
    v.y += this.y;
    v.z += this.z;
    return v;
  }

  /** World → render space, in place. */
  toRender<T extends { x: number; y: number; z: number }>(v: T): T {
    v.x -= this.x;
    v.y -= this.y;
    v.z -= this.z;
    return v;
  }

  /** World position of a three.js object (from its render-space matrix). */
  worldOf(o: THREE.Object3D, out: THREE.Vector3) {
    return this.toWorld(o.getWorldPosition(out));
  }
}

/** The one render origin of the client. */
export const origin = new RenderOrigin();
