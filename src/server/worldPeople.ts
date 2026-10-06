// The world's people near a player walk about as NPCs (docs/MUNDO.md §11): a link of the world bridge.
//
//   start   the sites' crews are registered in the world
//   tick    crews come out as people (a body each, round where they gather) or go back to being a
//           number; where the people are goes back to the world every few seconds
//   tasks   what their minds decide (channel 'npc.task', site coordinates) is laid on the ground

import type { SimClient } from '../sim/host/client.js';
import type { CrewAt, MadePerson, NpcOrder } from '../sim/modules/people.js';
import type { Vec3 } from '../shared/protocol.js';
import type { SiteCrew } from '../shared/ship/spawn.js';
import type { NpcTask, Npcs } from './npcs.js';
import type { Observation, WorldLink } from './world.js';

export interface PeopleLinkHost {
  readonly npcs: Npcs;
  /** The sites' crews. */
  crews(): SiteCrew[];
  /** A point of a site's frame on its ground (world); null: no such site. */
  siteToWorld(site: string, x: number, z: number): Vec3 | null;
  /** The time of the current step (ms, server clock). */
  now(): number;
  log(msg: string): void;
}

/** Where the people are goes back to the world every this many ticks. */
const REPORT_EVERY = 5;

export class PeopleLink implements WorldLink {
  readonly name = 'people';
  private ticks = 0;
  private crews = new Map<string, SiteCrew>();
  /** Last order per person (one that came before its body is applied when it gets one). */
  private readonly orders = new Map<number, NpcTask>();
  private off: (() => void) | null = null;

  constructor(private readonly host: PeopleLinkHost) {}

  async start(sim: SimClient): Promise<void> {
    const crews = this.host.crews();
    this.crews = new Map(crews.map((c) => [c.key, c]));
    const at: CrewAt[] = crews.map((c) => ({ key: c.key, p: c.center, count: c.count, slots: c.slots, enter: c.enter, leave: c.leave, mix: c.mix }));
    await sim.ask('people.register', at);
    this.off?.();
    const offTask = sim.onEmit('npc.task', (d) => this.order(d as NpcOrder));
    const offSay = sim.onEmit('npc.say', (d) => {
      const s = d as { id: number; text: string };
      this.host.npcs.say(s.id, s.text);
    });
    this.off = () => {
      offTask();
      offSay();
    };
    this.host.log(`Mundo: ${crews.length} dotación(es) de personas`);
  }

  stop(): void {
    this.off?.();
    this.off = null;
  }

  async reseed(): Promise<void> {
    for (const id of this.orders.keys()) this.host.npcs.despawn(id);
    this.orders.clear();
  }

  async tick(sim: SimClient, o: Observation): Promise<void> {
    const r = await sim.ask<{ up: MadePerson[]; down: number[] }>('people.observe', o);
    for (const m of r.up) {
      const crew = this.crews.get(m.crew);
      if (!crew) continue;
      // an individual comes back where it was left; the rest round where they gather, apart
      const a = (m.index / Math.max(1, crew.slots)) * Math.PI * 2;
      const p = m.p ?? this.host.siteToWorld(crew.site, crew.x + Math.cos(a) * 2.5, crew.z + Math.sin(a) * 2.5);
      if (!p) continue;
      this.host.npcs.spawn({ id: m.id, name: m.name, variant: m.variant, p, task: this.orders.get(m.id) }, this.host.now());
    }
    for (const id of r.down) {
      this.host.npcs.despawn(id);
      this.orders.delete(id);
    }
    if (++this.ticks % REPORT_EVERY === 0 && this.host.npcs.size) await sim.ask('people.at', this.host.npcs.positions());
  }

  /** What a mind decided, laid on the ground. */
  private order(o: NpcOrder): void {
    const at = o.p ?? this.host.siteToWorld(o.site, o.x, o.z);
    if (!at) return;
    const faceAt = o.fx !== undefined && o.fz !== undefined ? this.host.siteToWorld(o.site, o.fx, o.fz) ?? undefined : undefined;
    const task: NpcTask = o.kind === 'wander' ? { kind: 'wander', c: at, r: o.r ?? 10 } : o.kind === 'goto' ? { kind: 'goto', p: at, face: faceAt, run: o.run } : { kind: 'idle', face: faceAt };
    this.orders.set(o.id, task);
    this.host.npcs.assign(o.id, task);
  }
}
