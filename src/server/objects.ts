// Loose objects on the server (docs/RED.md): every crate, spare part… of the world, who simulates
// each one (its owner: whoever handles it; or nobody: it lies still) and who knows about it
// (interest, shared/net/interest.ts). They come and go at any time: `spawn` puts one in the world
// and tells whoever is near, `despawn` takes it out and tells whoever knew it. A player learns the
// objects near it as it moves and forgets the far ones; an object's news go only to its knowers.

import { Replication, type InterestRule } from '../shared/net/interest.js';
import type { CrateWire, EntityWire, Vec3 } from '../shared/protocol.js';
import { validCrate, WORLD_FRAME, type Crate } from '../shared/ship/crates.js';
import { deliver, type Peer } from './peer.js';

/** Where the hosts (ships) are: a point of a host's space in the world (null: no such host). */
export interface ObjectHosts {
  toWorld(fr: number, p: Vec3, out: Vec3): Vec3 | null;
}

/** What happens to the objects (the world bridge listens: docs/MUNDO.md). */
export interface ObjectEvents {
  /** It came to rest (nobody simulates it now) where it is. */
  rest?(c: Crate): void;
  /** A player started handling it (it is theirs to simulate). */
  taken?(c: Crate, by: number): void;
}

/** A player knows the objects within `enter` m (or in its ship) and forgets them beyond `leave`. */
export const OBJECT_INTEREST: InterestRule = { enter: 1500, leave: 1800 };
/** An owner silent this long (ms) loses its object to whoever asks. */
const STALE_MS = 1500;
/** Ids of objects the world doesn't keep (made here, for tests) start here: never a world entity's. */
const LOCAL_IDS = 2 ** 31;

type Obj = Crate & { heard: number };

export class LooseObjects {
  events: ObjectEvents = {};
  private readonly rep: Replication<Obj, Peer>;
  private nextLocal = LOCAL_IDS;

  constructor(
    hosts: ObjectHosts,
    private readonly peers: () => Iterable<Peer>,
    private readonly now: () => number,
    rule: InterestRule = OBJECT_INTEREST,
  ) {
    this.rep = new Replication<Obj, Peer>(
      {
        where: (c, out) => (c.fr === WORLD_FRAME ? ((out[0] = c.p[0]), (out[1] = c.p[1]), (out[2] = c.p[2]), out) : hosts.toWorld(c.fr, c.p, out)),
        frame: (c) => c.fr,
        // what a player simulates it knows, however far it flew
        pinned: (w, c) => c.owner !== 0 && c.owner === w.id,
      },
      rule,
    );
  }

  get size(): number {
    return this.rep.size;
  }

  get(id: number): Crate | undefined {
    return this.rep.get(id);
  }

  values(): IterableIterator<Crate> {
    return this.rep.values();
  }

  /** Objects in a host (its cargo, for its mass). */
  inFrame(fr: number): Iterable<Crate> {
    return this.rep.inFrame(fr);
  }

  /** Does this player know it? */
  knows(p: Peer, id: number): boolean {
    return this.rep.knows(p, id);
  }

  /** A fresh world's objects (whoever knew the old ones forgets them). */
  reset(list: readonly Crate[]): void {
    for (const c of [...this.rep.values()]) this.despawn(c.id);
    for (const c of list) this.spawn(c);
  }

  /**
   * Puts an object in the world (its id: the world's entity; none: a local one, not kept); whoever
   * is near learns it now.
   */
  spawn(c: Omit<Crate, 'id'> & { id?: number }): Crate {
    const id = c.id ?? this.nextLocal++;
    if (this.rep.get(id)) throw new Error(`objeto ${id} repetido`);
    const o: Obj = { ...c, id, heard: 0 } as Obj;
    const to = this.rep.add(o, this.alive());
    if (to.length) deliver(to, { type: 'spawn', e: [spawnWire(o)] });
    return o;
  }

  /** Takes an object out of the world; whoever knew it forgets it. */
  despawn(id: number): boolean {
    const had = this.rep.get(id) !== undefined;
    const to = this.rep.remove(id);
    if (to.length) deliver(to, { type: 'gone', k: 'obj', ids: [id] });
    return had;
  }

  /** What a newcomer knows right away (its spawn point must be its position already). */
  welcome(p: Peer): Array<CrateWire & Crate> {
    const ch = this.rep.update([p]).get(p);
    return ch ? ch.enter.map(publicObj) : [];
  }

  /** Interest, now and then: each player learns what came near and forgets what went far. */
  interest(): void {
    for (const [p, ch] of this.rep.update(this.alive())) {
      if (ch.enter.length) deliver([p], { type: 'spawn', e: ch.enter.map(spawnWire) });
      if (ch.leave.length) deliver([p], { type: 'gone', k: 'obj', ids: ch.leave });
    }
  }

  /** A player wants to simulate an object: granted when nobody does (or its owner went quiet). */
  take(p: Peer, id: unknown): void {
    const c = typeof id === 'number' ? this.rep.get(id) : undefined;
    if (!c || p.id <= 0 || p.dead) return;
    const t = this.now();
    if (c.owner === p.id) {
      c.heard = t;
      return;
    }
    const ownerHere = [...this.peers()].some((o) => o.id === c.owner);
    if (c.owner !== 0 && t - c.heard < STALE_MS && ownerHere) {
      deliver([p], { type: 'crate', c: wire(c) });
      return;
    }
    c.owner = p.id;
    c.heard = t;
    deliver(this.knowers(c.id, p), { type: 'crate', c: wire(c) });
    this.events.taken?.(c, p.id);
  }

  /** The owner's object: keep it, pass it on to whoever sees it; at rest it goes back to nobody. */
  move(p: Peer, raw: unknown, rest: boolean, validFrame: (fr: number) => boolean): void {
    if (!validCrate(raw)) return;
    const c = this.rep.get(raw.id);
    if (!c || c.owner !== p.id) return;
    if (raw.fr !== WORLD_FRAME && !validFrame(raw.fr)) return;
    c.fr = raw.fr;
    c.p = raw.p;
    c.q = raw.q;
    c.v = rest ? [0, 0, 0] : raw.v;
    c.w = rest ? [0, 0, 0] : raw.w;
    c.heard = this.now();
    // the time of the owner's step this state belongs to (trusted within reason)
    c.t = typeof raw.t === 'number' && Number.isFinite(raw.t) ? Math.min(c.heard + 50, Math.max(c.heard - 1000, raw.t)) : c.heard;
    if (rest) c.owner = 0;
    this.rep.moved(c);
    const msg = { type: 'crate', c: wire(c), rest } as const;
    if (rest) {
      deliver(this.knowers(c.id, p), msg);
      this.events.rest?.(c);
    } else deliver(this.knowers(c.id).filter((o) => o !== p), msg);
  }

  /** A player left: what it simulated lies still where it last said, nobody's. */
  leave(p: Peer): void {
    for (const c of this.rep.values()) {
      if (c.owner !== p.id) continue;
      c.owner = 0;
      c.v = [0, 0, 0];
      c.w = [0, 0, 0];
      deliver(this.knowers(c.id).filter((o) => o !== p), { type: 'crate', c: wire(c), rest: true });
      this.events.rest?.(c);
    }
    this.rep.drop(p);
  }

  private *alive(): Generator<Peer> {
    for (const p of this.peers()) if (p.id > 0) yield p;
  }

  /** Players that know an object (and `also`, who must hear it anyway). */
  private knowers(id: number, also?: Peer): Peer[] {
    const out = this.rep.watchers(id);
    if (also && !out.includes(also)) out.push(also);
    return out;
  }
}

export function wire(c: Crate): CrateWire {
  return { id: c.id, fr: c.fr, p: c.p, q: c.q, v: c.v, w: c.w, owner: c.owner, t: c.t };
}

function publicObj(o: Obj): CrateWire & Crate {
  const { heard: _h, ...c } = o;
  return c;
}

function spawnWire(o: Obj): EntityWire {
  return { k: 'obj', ...publicObj(o) };
}
