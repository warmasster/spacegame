// Loose objects are kept by the world (docs/MUNDO.md §10): a link of the world bridge.
//
//   start     the sites' depots are registered; a fresh world is given the room's starting
//             objects; then the room's objects are the world's (same ids, where they last lay)
//   rest      an object that comes to rest: where it lies is written to the world (it persists)
//   taken     a player handling an object: it is its own now (a depot's unit leaves it for good)
//   tick      depots put their crates out near the players and take them back when they go

import type { SimClient } from '../sim/host/client.js';
import type { DepotDef, MadeThing, ThingAt, ThingSpec } from '../sim/modules/objects.js';
import type { CrateSpec, Quat, Vec3 } from '../shared/protocol.js';
import type { Crate } from '../shared/ship/crates.js';
import type { SiteDepot } from '../shared/ship/spawn.js';
import type { LooseObjects } from './objects.js';
import type { Observation, WorldLink } from './world.js';

type Laid = { id: number; host: number; p: Vec3; q: Quat };

export interface ObjectsLinkHost {
  readonly objects: LooseObjects;
  /** The objects a fresh world starts with. */
  startCrates(): Crate[];
  /** The sites' depots, laid on the ground. */
  depots(): SiteDepot[];
  /** Who took an object as the world knows them, where, and who noticed (null: nobody we know). */
  taker(by: number): { actor: number; at: Vec3; witnesses: number[] } | null;
  log(msg: string): void;
}

export class ObjectsLink implements WorldLink {
  readonly name = 'objects';
  private readonly rested = new Map<number, Laid>();
  private layouts = new Map<string, SiteDepot>();

  constructor(private readonly host: ObjectsLinkHost) {}

  async start(sim: SimClient): Promise<void> {
    const depots = this.host.depots();
    this.layouts = new Map(depots.map((d) => [d.key, d]));
    const defs: DepotDef[] = depots.map((d) => ({ key: d.key, spec: thingSpec(d.spec), host: 0, p: d.center, count: d.count, slots: d.slots, enter: d.enter, leave: d.leave }));
    await sim.ask('depots.register', defs);
    const state = await sim.ask<{ registered: boolean }>('objects.state');
    if (!state.registered) {
      const start = this.host.startCrates().map((c) => ({ spec: thingSpec(c), host: c.fr, p: c.p, q: c.q }));
      await sim.ask('objects.register', start);
    }
    const list = await sim.ask<ThingAt[]>('objects.list');
    this.host.objects.reset(list.map((t) => crate(t.id, t.host, t.p, t.q, t.spec)));
    this.host.objects.events.rest = (c) => this.rested.set(c.id, { id: c.id, host: c.fr, p: [...c.p], q: [...c.q] as Quat });
    this.host.objects.events.taken = (c, by) => {
      const who = this.host.taker(by);
      sim.ask('objects.taken', { id: c.id, by: who?.actor ?? 0, witnesses: who?.witnesses ?? [], at: who?.at }).catch(() => undefined);
    };
    this.host.log(`Mundo: ${list.length} objetos sueltos, ${depots.length} almacén(es)`);
  }

  async reseed(sim: SimClient): Promise<void> {
    this.rested.clear();
    await sim.ask('objects.reset');
  }

  async flush(sim: SimClient): Promise<void> {
    if (!this.rested.size) return;
    const list = [...this.rested.values()];
    this.rested.clear();
    await sim.ask('objects.rest', list);
  }

  async tick(sim: SimClient, o: Observation): Promise<void> {
    const r = await sim.ask<{ up: MadeThing[]; down: number[] }>('objects.observe', o);
    const laid: Laid[] = [];
    for (const m of r.up) {
      const slot = this.layouts.get(m.depot)?.slot(m.index);
      const at: Laid = slot ? { id: m.id, host: 0, p: slot.p, q: slot.q } : { id: m.id, host: m.host, p: m.p, q: m.q };
      if (!this.host.objects.get(m.id)) this.host.objects.spawn(crate(m.id, at.host, at.p, at.q, m.spec));
      laid.push(at);
    }
    // where the room laid them is where they are
    if (laid.length) await sim.ask('objects.rest', laid);
    for (const id of r.down) this.host.objects.despawn(id);
  }
}

function thingSpec(c: CrateSpec): ThingSpec {
  return { ...(c.kind ? { kind: c.kind } : {}), half: [c.half[0], c.half[1], c.half[2]], mass: c.mass, paint: c.paint };
}

function crate(id: number, fr: number, p: Vec3, q: Quat, s: ThingSpec): Crate {
  return { id, fr, p: [...p], q: [...q] as Quat, v: [0, 0, 0], w: [0, 0, 0], owner: 0, ...(s.kind ? { kind: s.kind } : {}), half: [...s.half], mass: s.mass, paint: s.paint };
}
