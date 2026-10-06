// The bridge between the room (the physical world: bodies, sockets) and the world simulation on its
// own thread (docs/MUNDO.md §10–12). Each part of the world the room gives a body to is a link:
//
//   objects   loose objects are kept by the world (server/worldObjects.ts)
//   people    the world's people near a player walk about as NPCs (server/worldPeople.ts)
//
// Every second the bridge tells every link where the players are; links ask the world and apply
// its answers. If the world isn't there (it failed, or it isn't up yet) the room goes on without it.
// Nothing here waits for the world inside a game step.

import type { SimClient } from '../sim/host/client.js';
import type { Observer } from '../sim/kit/lod.js';
import type { Peer } from './peer.js';

/** What the links get every tick: where the players are and where the hosts are (world). */
export interface Observation {
  observers: Observer[];
  hosts: Array<[number, number, number, number]>;
}

/** A part of the world with a body in the room. */
export interface WorldLink {
  readonly name: string;
  /** Registers what the room's sites have and takes what the world keeps. */
  start(sim: SimClient): Promise<void>;
  /** Where the players are now. */
  tick(sim: SimClient, o: Observation): Promise<void>;
  /** Writes what is pending (before a save). */
  flush?(sim: SimClient): Promise<void>;
  /** A new terrain: forget and start again. */
  reseed?(sim: SimClient): Promise<void>;
  stop?(): void;
}

export interface BridgeHost {
  peers(): Iterable<Peer>;
  /** World position of each host (ship) now. */
  hosts(): Array<[number, number, number, number]>;
  log(msg: string): void;
}

/** How often the players' positions go to the world (ms). */
const OBSERVE_MS = 1000;

export class WorldBridge {
  private timer: ReturnType<typeof setInterval> | null = null;
  private busy = false;
  private failing = false;
  private generation = 0;

  constructor(
    private readonly sim: SimClient,
    private readonly host: BridgeHost,
    private readonly links: readonly WorldLink[],
  ) {}

  async start(): Promise<void> {
    const gen = ++this.generation;
    for (const l of this.links) {
      if (gen !== this.generation) return;
      await l.start(this.sim);
    }
    this.timer ??= setInterval(() => void this.tick(), OBSERVE_MS);
  }

  /** A new terrain (the room was reseeded). */
  async reseed(): Promise<void> {
    this.generation++;
    for (const l of this.links) await l.reseed?.(this.sim);
    await this.start();
  }

  async flush(): Promise<void> {
    for (const l of this.links) await l.flush?.(this.sim);
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
    this.generation++;
    for (const l of this.links) l.stop?.();
  }

  private async tick(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    const gen = this.generation;
    try {
      const observers: Observer[] = [];
      for (const p of this.host.peers()) if (p.id > 0 && p.at) observers.push({ host: p.fr, p: [p.at[0], p.at[1], p.at[2]] });
      const o: Observation = { observers, hosts: this.host.hosts() };
      for (const l of this.links) {
        if (gen !== this.generation) return;
        await l.flush?.(this.sim);
        await l.tick(this.sim, o);
      }
      this.failing = false;
    } catch (e) {
      if (!this.failing) this.host.log(`Mundo: sin respuesta (${e instanceof Error ? e.message : e})`);
      this.failing = true;
    } finally {
      this.busy = false;
    }
  }
}
