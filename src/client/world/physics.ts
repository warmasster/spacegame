import RAPIER from '@dimforge/rapier3d-compat';
import { toLocal, toWorld, type ShipPose } from '../../shared/ship/flight';
import { qConj, qMul, type V3 } from '../../shared/ship/geom';
import { surfaceOf } from '../../shared/space/body';
import { modReach, type TerrainMod } from '../../shared/space/terrainMods';
import type { Bubble } from '../frames/bubble';
import type { TerrainWorkerPool } from './workerPool';

const TILE = 32;
const TILE_RES = 64; // 0.5 m: about the finest render level's sampling
const RADIUS = 2; // tiles around the player (5x5 = 160 m: debris and explosions nearby have ground)
/** Boulders from this size (m) get a collider. */
const MIN_ROCK = 0.22;
const _mods: TerrainMod[] = [];
const _c: V3 = [0, 0, 0];

interface Tile {
  key: string;
  state: 'loading' | 'ready';
  colliders: RAPIER.Collider[];
  /** Centre (bubble coordinates; moved with the colliders when the bubble is re-laid). */
  cx: number;
  cz: number;
  cancel?: () => void;
}

/**
 * The Rapier world of the outside — the physics bubble's (frames/bubble.ts) — with streamed
 * collision around the local player: heightfield tiles of the body's one ground (its surface with
 * every terrain modifier: pads, craters) laid in the bubble's tangent frame, built in the terrain
 * workers, and simple colliders for the big boulders; in space there are none. The same anywhere
 * round any body. Gravity is the bubble's (real, radial at its anchor): dynamic bodies use scale 1.
 *
 * When the bubble is re-laid the tiles that exist are carried rigidly into the new frame (they are
 * still the same ground) and stay until the new grid covers them: the ground never disappears
 * under anything for the time the workers take.
 */
export class Physics {
  readonly world: RAPIER.World;
  private tiles = new Map<string, Tile>();
  /** Tiles from before the last re-laying (in the new frame now), until the new grid covers them. */
  private stale: Tile[] = [];
  private bubble: Bubble | null = null;
  /** Bubble version the grid belongs to. */
  private version = -1;

  static async create(pool: TerrainWorkerPool, seed: number) {
    await RAPIER.init();
    return new Physics(pool, seed);
  }

  private constructor(
    private pool: TerrainWorkerPool,
    private seed: number,
  ) {
    // its gravity is the bubble's, set before every step (the body it is laid on)
    this.world = new RAPIER.World({ x: 0, y: 0, z: 0 });
  }

  get rapier() {
    return RAPIER;
  }

  /** The bubble whose frame this world is in (set once, before the first step). */
  attach(bubble: Bubble) {
    this.bubble = bubble;
    this.version = bubble.version;
  }

  /** Stream collision tiles around bubble (x, z). */
  update(x: number, z: number) {
    const b = this.bubble;
    if (b && b.mode === 'space') {
      // nothing to stand on up there
      this.dropAll();
      return;
    }
    const cx = Math.floor(x / TILE);
    const cz = Math.floor(z / TILE);
    for (let dz = -RADIUS; dz <= RADIUS; dz++) {
      for (let dx = -RADIUS; dx <= RADIUS; dx++) {
        const tx = cx + dx;
        const tz = cz + dz;
        const key = `${tx}:${tz}`;
        if (!this.tiles.has(key)) this.load(tx, tz, key, Math.abs(dx) + Math.abs(dz));
      }
    }
    for (const [key, tile] of this.tiles) {
      // keep a margin before dropping to avoid thrashing on tile borders
      const tx = Math.floor(tile.cx / TILE);
      const tz = Math.floor(tile.cz / TILE);
      if (Math.abs(tx - cx) <= RADIUS + 1 && Math.abs(tz - cz) <= RADIUS + 1) continue;
      this.drop(tile);
      this.tiles.delete(key);
    }
    // old tiles: gone once far, or once the new grid is solid where they are
    for (let i = this.stale.length - 1; i >= 0; i--) {
      const t = this.stale[i];
      const far = Math.abs(t.cx - x) > (RADIUS + 1.5) * TILE || Math.abs(t.cz - z) > (RADIUS + 1.5) * TILE;
      if (far || this.gridReady(t.cx, t.cz)) {
        this.drop(t);
        this.stale.splice(i, 1);
      }
    }
  }

  /** True when there is collision under bubble (x, z) (or there is none to wait for: in space). */
  readyAt(x: number, z: number) {
    if (this.bubble?.mode === 'space') return true;
    if (this.gridReady(x, z)) return true;
    for (const t of this.stale) if (t.state === 'ready' && Math.abs(t.cx - x) < TILE / 2 && Math.abs(t.cz - z) < TILE / 2) return true;
    return false;
  }

  private gridReady(x: number, z: number) {
    return this.tiles.get(`${Math.floor(x / TILE)}:${Math.floor(z / TILE)}`)?.state === 'ready';
  }

  /** The ground changed under a modifier (a crater): rebuild the collision tiles it reaches. */
  invalidate(mod: TerrainMod) {
    const b = this.bubble;
    if (!b || b.mode === 'space' || mod.body !== b.body.def.id) return;
    const body = b.body;
    const r = body.radius;
    for (let i = 0; i < 3; i++) _c[i] = body.center[i] + mod.center[i] * r;
    const l = toLocal(b.pose, _c);
    const x = l[0];
    const z = l[2];
    const m = modReach(mod);
    // snapshot: load() re-inserts keys, and iterating a Map while re-inserting never ends
    for (const [key, tile] of [...this.tiles]) {
      const [tx, tz] = key.split(':').map(Number);
      const x0 = tx * TILE;
      const z0 = tz * TILE;
      if (x + m < x0 || x - m > x0 + TILE || z + m < z0 || z - m > z0 + TILE) continue;
      tile.cancel?.();
      const old = tile.colliders;
      this.tiles.delete(key);
      this.load(tx, tz, key, 0, old);
    }
  }

  /**
   * The bubble was re-laid (from `a` to `b`): the tiles there are carried rigidly into the new frame
   * and kept until the new grid covers them; the ones still loading are dropped.
   */
  rebase(a: ShipPose, b: ShipPose) {
    const dq = qMul(qConj(b.q), a.q);
    for (const t of [...this.stale, ...this.tiles.values()]) {
      if (t.state !== 'ready') {
        this.drop(t);
        continue;
      }
      // a rebuild on its way (a crater) belongs to the old frame
      t.cancel?.();
      t.cancel = undefined;
      for (const c of t.colliders) {
        const p = c.translation();
        const q = c.rotation();
        const np = toLocal(b, toWorld(a, [p.x, p.y, p.z]));
        const nq = qMul(dq, [q.x, q.y, q.z, q.w]);
        c.setTranslation({ x: np[0], y: np[1], z: np[2] });
        c.setRotation({ x: nq[0], y: nq[1], z: nq[2], w: nq[3] });
      }
      const nc = toLocal(b, toWorld(a, [t.cx, 0, t.cz] as V3));
      t.cx = nc[0];
      t.cz = nc[2];
    }
    this.stale = [...this.stale, ...this.tiles.values()].filter((t) => t.state === 'ready');
    this.tiles.clear();
    this.version = this.bubble?.version ?? this.version + 1;
  }

  step(dt: number) {
    const g = this.bubble?.gravity;
    if (g) {
      const w = this.world.gravity;
      if (w.x !== g.x || w.y !== g.y || w.z !== g.z) this.world.gravity = { x: g.x, y: g.y, z: g.z };
    }
    this.world.timestep = dt;
    this.world.step();
  }

  private drop(t: Tile) {
    t.cancel?.();
    for (const c of t.colliders) this.world.removeCollider(c, false);
    t.colliders = [];
  }

  private dropAll() {
    for (const t of this.tiles.values()) this.drop(t);
    for (const t of this.stale) this.drop(t);
    this.tiles.clear();
    this.stale.length = 0;
  }

  private load(tx: number, tz: number, key: string, priority: number, replacing: RAPIER.Collider[] = []) {
    // while rebuilding, the previous colliders keep the tile solid
    const x0 = tx * TILE;
    const z0 = tz * TILE;
    const tile: Tile = { key, state: replacing.length ? 'ready' : 'loading', colliders: replacing, cx: x0 + TILE / 2, cz: z0 + TILE / 2 };
    this.tiles.set(key, tile);
    const b = this.bubble;
    if (!b) return;
    const version = this.version;
    // the modifiers that reach the tile (their ground is part of it)
    const surface = surfaceOf(b.body, this.seed);
    _mods.length = 0;
    if (surface) {
      const cx = x0 + TILE / 2;
      const cz = z0 + TILE / 2;
      const c = b.body.center;
      const px = b.pose.p[0] + b.e[0] * cx + b.s[0] * cz - c[0];
      const py = b.pose.p[1] + b.e[1] * cx + b.s[1] * cz - c[1];
      const pz = b.pose.p[2] + b.e[2] * cx + b.s[2] * cz - c[2];
      const pl = Math.hypot(px, py, pz);
      surface.mods.near([px / pl, py / pl, pz / pl], TILE * 0.75, TILE / TILE_RES, _mods);
    }
    const job = this.pool.run(
      { kind: 'tangent', body: b.body.def.id, seed: this.seed, mods: _mods.slice(), o: [...b.pose.p], e: [...b.e], u: [...b.u], s: [...b.s], x0, z0, size: TILE, res: TILE_RES, minRock: MIN_ROCK },
      -10 + priority,
    );
    tile.cancel = job.cancel;
    job.promise.then((r) => {
      if (r.kind !== 'tangent' || this.tiles.get(key) !== tile || this.version !== version) return;
      // worker layout is row-major with rows along Z; Rapier wants column-major (x-major)
      const n = TILE_RES + 1;
      for (const c of replacing) this.world.removeCollider(c, false);
      tile.colliders = [];
      const hf = new Float32Array(n * n);
      for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) hf[i * n + j] = r.heights[j * n + i];
      const desc = RAPIER.ColliderDesc.heightfield(TILE_RES, TILE_RES, hf, { x: TILE, y: 1, z: TILE });
      desc.setTranslation(x0 + TILE / 2, 0, z0 + TILE / 2);
      desc.setFriction(0.9);
      tile.colliders.push(this.world.createCollider(desc));
      // boulders big enough to matter (the same scatter the renderer draws): frame x, ground y, z, size
      const rk = r.rocks;
      for (let i = 0; i < rk.length; i += 4) {
        const size = rk[i + 3];
        const d = RAPIER.ColliderDesc.ball(size * 0.78).setTranslation(rk[i], rk[i + 1] - size * 0.3, rk[i + 2]);
        tile.colliders.push(this.world.createCollider(d));
      }
      tile.state = 'ready';
    });
  }
}
