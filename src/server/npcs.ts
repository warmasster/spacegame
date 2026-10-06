// NPCs with a body (docs/MUNDO.md §11): the people of the world who are near a player. The world
// decides who they are and what they do (tasks: go somewhere, wander round a place, stand); the
// server moves them on the ground (shared/actors/walker.ts, no physics engine) at 20 Hz, stamped
// with its step clock like everything else (docs/MOVIMIENTO.md), and tells whoever is near
// (interest: shared/net/interest.ts): who they are when they come into view, their states while
// they are, that they are gone when they leave. Clients draw them as they draw other astronauts.

import { GAIT, face, walk, type Obstacle, type WalkGround, type WalkState } from '../shared/actors/walker.js';
import type { Eye } from '../shared/actors/perception.js';
import { Replication, type InterestRule } from '../shared/net/interest.js';
import { StateFlags, type EntityWire, type PlayerState, type Vec3 } from '../shared/protocol.js';
import { Rng } from '../sim/core/rng.js';
import { deliver, type Peer } from './peer.js';

/** What an NPC is doing (the world's word, in world coordinates). */
export type NpcTask =
  /** Walk there (and stand, facing `face` if given). */
  | { kind: 'goto'; p: Vec3; run?: boolean; face?: Vec3 }
  /** Stroll between random points within `r` of `c`, pausing at each. */
  | { kind: 'wander'; c: Vec3; r: number }
  /** Stand (facing `face` if given). */
  | { kind: 'idle'; face?: Vec3 };

export interface NpcSpawn {
  id: number;
  name: string;
  variant: number;
  /** Feet (world). */
  p: Vec3;
  yaw?: number;
  task?: NpcTask;
}

export interface NpcHost {
  /** The ground at a world point (null: nowhere to stand: it stays put). */
  ground(p: Vec3): WalkGround | null;
  /** What to walk round near a point (the ships). */
  obstacles(p: Vec3, r: number): Obstacle[];
}

interface Npc extends WalkState {
  readonly id: number;
  readonly name: string;
  readonly variant: number;
  task: NpcTask;
  /** Where it is walking now (a wander point, a goto target). */
  goal: Vec3 | null;
  /** Seconds to stand before the next wander point. */
  pause: number;
  /** Time of the step its state belongs to (ms, server clock). */
  t: number;
}

/** NPCs are known within `enter` m and forgotten beyond `leave`. */
export const NPC_INTEREST: InterestRule = { enter: 600, leave: 800 };
/** How far a suit radio carries what an NPC says (m). */
const SAY_M = 60;

export class Npcs {
  private readonly rep: Replication<Npc, Peer>;
  private readonly rng = new Rng(0x6e7063);

  constructor(
    private readonly host: NpcHost,
    private readonly peers: () => Iterable<Peer>,
    rule: InterestRule = NPC_INTEREST,
  ) {
    this.rep = new Replication<Npc, Peer>(
      {
        where: (n, out) => ((out[0] = n.p[0]), (out[1] = n.p[1]), (out[2] = n.p[2]), out),
        frame: () => 0,
      },
      rule,
    );
  }

  get size(): number {
    return this.rep.size;
  }

  has(id: number): boolean {
    return this.rep.get(id) !== undefined;
  }

  /** A body in the world; whoever is near sees it now. */
  spawn(s: NpcSpawn, t: number): void {
    if (this.rep.get(s.id)) return;
    const n: Npc = { id: s.id, name: s.name, variant: s.variant, p: [...s.p], v: [0, 0, 0], yaw: s.yaw ?? 0, task: s.task ?? { kind: 'idle' }, goal: null, pause: 0, t };
    const to = this.rep.add(n, this.alive());
    if (to.length) deliver(to, { type: 'spawn', e: [wire(n)] });
  }

  despawn(id: number): void {
    const to = this.rep.remove(id);
    if (to.length) deliver(to, { type: 'gone', k: 'npc', ids: [id] });
  }

  /** What it does from now on. */
  assign(id: number, task: NpcTask): void {
    const n = this.rep.get(id);
    if (!n) return;
    n.task = task;
    n.goal = task.kind === 'goto' ? [...task.p] : null;
    n.pause = 0;
  }

  /** One step of everyone's walking (`t`: the time of the state it makes, ms). */
  step(dt: number, t: number): void {
    for (const n of this.rep.values()) {
      n.t = t;
      const g = this.host.ground(n.p);
      if (!g) continue;
      const task = n.task;
      if (task.kind === 'wander') {
        if (!n.goal) {
          n.pause -= dt;
          if (n.pause <= 0) {
            const a = this.rng.float() * Math.PI * 2;
            const r = task.r * Math.sqrt(this.rng.float());
            n.goal = pointNear(g, task.c, r, a);
          }
        }
        if (n.goal) {
          const left = walk(n, n.goal, GAIT.walk * 0.7, dt, g, this.host.obstacles(n.p, 40));
          if (left === 0) {
            n.goal = null;
            n.pause = 3 + this.rng.float() * 9;
          }
        } else walk(n, null, 0, dt, g);
      } else if (task.kind === 'goto' && n.goal) {
        const left = walk(n, n.goal, task.run ? GAIT.run : GAIT.walk, dt, g, this.host.obstacles(n.p, 40));
        if (left === 0) n.goal = null;
      } else {
        walk(n, null, 0, dt, g);
        const at = task.face;
        if (at && Math.hypot(n.v[0], n.v[1], n.v[2]) < 0.1) face(n, at, dt, g);
      }
    }
  }

  /** Someone says something: the players within earshot of its suit radio hear it (with its name). */
  say(id: number, text: string): void {
    const n = this.rep.get(id);
    if (!n) return;
    const to: Peer[] = [];
    for (const p of this.alive()) if (p.at && Math.hypot(p.at[0] - n.p[0], p.at[1] - n.p[1], p.at[2] - n.p[2]) < SAY_M) to.push(p);
    if (to.length) deliver(to, { type: 'say', ship: 0, text: `${n.name}: ${text}` });
  }

  /** Each player gets the states of the NPCs it knows. */
  broadcast(t: number): void {
    for (const p of this.alive()) {
      const states: Array<{ id: number; t: number; s: PlayerState }> = [];
      for (const n of this.rep.values()) if (this.rep.knows(p, n.id)) states.push({ id: n.id, t: n.t, s: state(n) });
      if (states.length) deliver([p], { type: 'npcs', t, states });
    }
  }

  /** Interest: each player learns the NPCs that came near and forgets the far ones. */
  interest(): void {
    for (const [p, ch] of this.rep.update(this.alive())) {
      if (ch.enter.length) deliver([p], { type: 'spawn', e: ch.enter.map(wire) });
      if (ch.leave.length) deliver([p], { type: 'gone', k: 'npc', ids: ch.leave });
    }
  }

  /** A player left. */
  leave(p: Peer): void {
    this.rep.drop(p);
  }

  /** Where each one is (world). */
  positions(): Array<{ id: number; p: Vec3 }> {
    return [...this.rep.values()].map((n) => ({ id: n.id, p: [n.p[0], n.p[1], n.p[2]] }));
  }

  /** Their eyes (perception: who saw what). */
  eyes(): Array<{ id: number; eye: Eye }> {
    const out: Array<{ id: number; eye: Eye }> = [];
    const e: Vec3 = [0, 0, 0], u: Vec3 = [0, 0, 0], s: Vec3 = [0, 0, 0];
    for (const n of this.rep.values()) {
      const g = this.host.ground(n.p);
      if (!g) continue;
      g.axes(n.p, e, u, s);
      // forward = −sin·east − cos·south; eyes 1.6 m up
      const f: Vec3 = [-Math.sin(n.yaw) * e[0] - Math.cos(n.yaw) * s[0], -Math.sin(n.yaw) * e[1] - Math.cos(n.yaw) * s[1], -Math.sin(n.yaw) * e[2] - Math.cos(n.yaw) * s[2]];
      out.push({ id: n.id, eye: { host: 0, air: false, p: [n.p[0] + u[0] * 1.6, n.p[1] + u[1] * 1.6, n.p[2] + u[2] * 1.6], fwd: f } });
    }
    return out;
  }

  private *alive(): Generator<Peer> {
    for (const p of this.peers()) if (p.id > 0) yield p;
  }
}

const r3 = (n: number) => Math.round(n * 1e3) / 1e3;

function state(n: Npc): PlayerState {
  const moving = Math.hypot(n.v[0], n.v[1], n.v[2]) > GAIT.walk * 1.2;
  return { p: [r3(n.p[0]), r3(n.p[1]), r3(n.p[2])], v: [r3(n.v[0]), r3(n.v[1]), r3(n.v[2])], yaw: r3(n.yaw), pitch: 0, f: StateFlags.Grounded | (moving ? StateFlags.Running : 0) };
}

function wire(n: Npc): EntityWire {
  return { k: 'npc', id: n.id, name: n.name, variant: n.variant, t: n.t, s: state(n) };
}

/** A point on the ground `r` m from `c`, at angle `a` in its plane. */
function pointNear(g: WalkGround, c: Vec3, r: number, a: number): Vec3 {
  const e: Vec3 = [0, 0, 0], u: Vec3 = [0, 0, 0], s: Vec3 = [0, 0, 0];
  g.axes(c, e, u, s);
  const p: Vec3 = [c[0] + (e[0] * Math.cos(a) + s[0] * Math.sin(a)) * r, c[1] + (e[1] * Math.cos(a) + s[1] * Math.sin(a)) * r, c[2] + (e[2] * Math.cos(a) + s[2] * Math.sin(a)) * r];
  const h = g.height(p);
  g.axes(p, e, u, s);
  return [p[0] - u[0] * h, p[1] - u[1] * h, p[2] - u[2] * h];
}
