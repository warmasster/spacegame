import RAPIER from '@dimforge/rapier3d-compat';
import { rocksInTile, type LunarTerrain } from '../../shared/terrain';
import type { TerrainWorkerPool } from './workerPool';

const TILE = 32;
const TILE_RES = 64; // 0.5 m — identical sampling to the finest render LOD
const RADIUS = 1; // tiles around the player (3x3)

interface Tile {
  key: string;
  state: 'loading' | 'ready';
  colliders: RAPIER.Collider[];
  cancel?: () => void;
}

/**
 * Rapier world with streamed collision around the local player: heightfield tiles (built in
 * the terrain workers from the shared height function) and simple colliders for boulders.
 */
export class Physics {
  readonly world: RAPIER.World;
  private tiles = new Map<string, Tile>();

  static async create(pool: TerrainWorkerPool, terrain: LunarTerrain) {
    await RAPIER.init();
    return new Physics(pool, terrain);
  }

  private constructor(
    private pool: TerrainWorkerPool,
    private terrain: LunarTerrain,
  ) {
    this.world = new RAPIER.World({ x: 0, y: 0, z: 0 });
  }

  get rapier() {
    return RAPIER;
  }

  /** Stream collision tiles around (x, z). */
  update(x: number, z: number) {
    const cx = Math.floor(x / TILE);
    const cz = Math.floor(z / TILE);
    const wanted = new Set<string>();
    for (let dz = -RADIUS; dz <= RADIUS; dz++) {
      for (let dx = -RADIUS; dx <= RADIUS; dx++) {
        const tx = cx + dx;
        const tz = cz + dz;
        const key = `${tx}:${tz}`;
        wanted.add(key);
        if (!this.tiles.has(key)) this.load(tx, tz, key, Math.abs(dx) + Math.abs(dz));
      }
    }
    for (const [key, tile] of this.tiles) {
      if (wanted.has(key)) continue;
      // keep a margin before dropping to avoid thrashing on tile borders
      const [tx, tz] = key.split(':').map(Number);
      if (Math.abs(tx - cx) <= RADIUS + 1 && Math.abs(tz - cz) <= RADIUS + 1) continue;
      tile.cancel?.();
      for (const c of tile.colliders) this.world.removeCollider(c, false);
      this.tiles.delete(key);
    }
  }

  /** True when the tile containing (x, z) has collision. */
  readyAt(x: number, z: number) {
    const t = this.tiles.get(`${Math.floor(x / TILE)}:${Math.floor(z / TILE)}`);
    return t?.state === 'ready';
  }

  step(dt: number) {
    this.world.timestep = dt;
    this.world.step();
  }

  private load(tx: number, tz: number, key: string, priority: number) {
    const tile: Tile = { key, state: 'loading', colliders: [] };
    this.tiles.set(key, tile);
    const x0 = tx * TILE;
    const z0 = tz * TILE;
    const job = this.pool.run({ kind: 'heights', seed: this.terrain.seed, x0, z0, size: TILE, res: TILE_RES }, -10 + priority);
    tile.cancel = job.cancel;
    job.promise.then((r) => {
      if (r.kind !== 'heights' || this.tiles.get(key) !== tile) return;
      // worker layout is row-major with rows along Z; Rapier wants column-major (x-major)
      const n = TILE_RES + 1;
      const hf = new Float32Array(n * n);
      for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) hf[i * n + j] = r.heights[j * n + i];
      const desc = RAPIER.ColliderDesc.heightfield(TILE_RES, TILE_RES, hf, { x: TILE, y: 1, z: TILE });
      desc.setTranslation(x0 + TILE / 2, 0, z0 + TILE / 2);
      desc.setFriction(0.9);
      tile.colliders.push(this.world.createCollider(desc));
      // boulders big enough to matter
      for (const rock of rocksInTile(this.terrain, x0, z0, TILE, 0.22)) {
        const h = this.terrain.height(rock.x, rock.z);
        const rad = rock.size * 0.78;
        const d = RAPIER.ColliderDesc.ball(rad).setTranslation(rock.x, h - rock.size * 0.3, rock.z);
        tile.colliders.push(this.world.createCollider(d));
      }
      tile.state = 'ready';
    });
  }
}
