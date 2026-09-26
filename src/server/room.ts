import type { WebSocket } from 'ws';
import { MAX_PLAYERS, SERVER_SNAPSHOT_RATE, SHIP_SPAWNS, WORLD_SEED } from '../shared/constants.js';
import { placeShip, REACH, REPAIR_RATE, SHIP_DEFS, ShipSim, SYSTEMS_HZ } from '../shared/ship/sim.js';
import { VarSync } from '../shared/ship/state.js';
import type { SysEvent } from '../shared/ship/systems.js';
import { SUIT, crewContext } from '../shared/ship/crew.js';
import { LunarTerrain } from '../shared/terrain.js';
import {
  PROTOCOL_VERSION,
  StateFlags,
  type ClientMessage,
  type PlayerInfo,
  type PlayerState,
  type ServerMessage,
  type TerrainEdit,
  type Vec3,
} from '../shared/protocol.js';

export const MAX_HP = 100;
const BLAST_RADIUS = 6; // m, damage falls off linearly
const CRATER_RADIUS = 2.4;
const RESPAWN_MS = 4000;
const MAX_EDITS = 4000;
/** Explosions higher than this above the ground (m) leave no crater (e.g. on a ship's hull). */
const CRATER_MAX_HEIGHT = 1.2;

interface Member {
  info: PlayerInfo;
  socket: WebSocket;
  state: PlayerState | null;
  stateTime: number;
  alive: boolean;
  hp: number;
  dead: boolean;
  /** Rockets in flight (fire timestamps) — a hit is only accepted for a fired rocket. */
  rockets: number[];
  lastFire: number;
  lastRepair: number;
  /** Suit oxygen 0..1 and whether it breathes cabin air right now. */
  suitO2: number;
  cabin: boolean;
}

/** Spawn points (x, z) around the landing site; y is resolved by the client on the terrain. */
const SPAWNS: Array<[number, number]> = [
  [0, 0],
  [2.2, 1.4],
  [-2.0, 1.8],
  [0.6, -2.4],
];

const now = () => performance.now();

/**
 * A single shared world instance. For the MVP the server relays player states
 * (co-op, trusted clients); authority over physics moves here in a later phase.
 */
export class Room {
  private members = new Map<WebSocket, Member>();
  private nextId = 1;
  private snapshotTimer: NodeJS.Timeout;
  private edits: TerrainEdit[] = [];
  /** Same deterministic terrain the clients use (+ the craters), to decide where craters form. */
  private terrain = new LunarTerrain(WORLD_SEED);
  private ships: ShipSim[];
  private sync = new Map<number, VarSync>();
  private systemsTimer: NodeJS.Timeout;
  private lastTick = now();
  private tickAcc = 0;
  private ticks = 0;

  constructor(private readonly log: (msg: string) => void) {
    this.snapshotTimer = setInterval(() => this.broadcastSnapshot(), 1000 / SERVER_SNAPSHOT_RATE);
    // ships sit on the pristine surface, so placement never depends on later craters
    const base = new LunarTerrain(WORLD_SEED);
    this.ships = SHIP_SPAWNS.map((s) => {
      const def = SHIP_DEFS[s.def];
      return new ShipSim(s.id, def, placeShip(def, s.x, s.z, s.yaw, base), base);
    });
    for (const ship of this.ships) this.sync.set(ship.id, new VarSync(ship.vars, ship.st));
    this.terrain.edits = this.edits;
    // ship machinery runs on a fixed step whatever the timer jitter
    this.systemsTimer = setInterval(() => this.stepSystems(), 1000 / SYSTEMS_HZ / 2);
  }

  dispose() {
    clearInterval(this.snapshotTimer);
    clearInterval(this.systemsTimer);
  }

  /** Fixed-rate ship systems + crew life support; state diffs to everyone at half the rate. */
  private stepSystems() {
    const t = now();
    this.tickAcc = Math.min(this.tickAcc + (t - this.lastTick) / 1000, 0.5);
    this.lastTick = t;
    const h = 1 / SYSTEMS_HZ;
    while (this.tickAcc >= h) {
      this.tickAcc -= h;
      this.ticks++;
      const players = this.players().filter((m) => m.state && !m.dead);
      for (const ship of this.ships) {
        const crew = crewContext(ship, players.map((m) => ({ p: m.state!.p, seated: (m.state!.f & StateFlags.Seated) !== 0 })));
        const r = ship.tick(h, { crew: crew.counts, docked: crew.docked, bodies: crew.bodies });
        if (Object.keys(r.sw).length) this.broadcast({ type: 'ship', ship: ship.id, sw: r.sw });
        this.shipEvents(ship, r.events);
        // suits: breathe the cabin (and top up) where it is breathable, else spend the reserve
        players.forEach((m, i) => {
          const where = crew.members[i];
          if (!where.inside) return;
          m.cabin = where.breathable;
          if (where.breathable || where.docked) m.suitO2 = Math.min(1, m.suitO2 + (where.breathable ? SUIT.refill : SUIT.dockRefill) * h);
        });
      }
      for (const m of players) {
        const inAny = this.ships.some((ship) => crewContext(ship, [{ p: m.state!.p, seated: false }]).members[0].inside);
        if (!inAny) m.cabin = false;
        if (!m.cabin) m.suitO2 = Math.max(0, m.suitO2 - SUIT.use * h);
        if (m.suitO2 <= 0) this.damage(m, SUIT.choke * h, 0);
      }
      if (this.ticks % 2 === 0) {
        for (const ship of this.ships) {
          const d = this.sync.get(ship.id)!.diff(ship.st);
          if (d.length) this.broadcast({ type: 'shipSt', ship: ship.id, d });
        }
      }
      if (this.ticks % 10 === 0) for (const m of players) send(m.socket, { type: 'vitals', o2: Math.round(m.suitO2 * 1000) / 1000, cabin: m.cabin });
    }
  }

  /** Resolve what the ship systems reported: internal explosions, messages. */
  private shipEvents(ship: ShipSim, events: SysEvent[]) {
    for (const e of events) {
      if (e.type === 'explode') {
        this.log(`✸ ${ship.def.name}: ${e.cause}`);
        this.explosion(ship.toWorld(e.at), 0, { radius: e.radius, damage: e.damage, crater: false });
        this.broadcast({ type: 'say', ship: ship.id, text: e.cause });
      } else if (e.type === 'say') this.broadcast({ type: 'say', ship: ship.id, text: e.text });
      else if (e.type === 'trip') {
        const c = ship.def.subsystems.find((x) => x.id === e.circuit)!;
        this.broadcast({ type: 'say', ship: ship.id, text: `Disyuntor saltado por sobrecarga: ${c.label}` });
      } else if (e.type === 'destroyed') {
        const part = ship.def.parts.find((p) => p.id === e.part);
        if (part) this.broadcast({ type: 'say', ship: ship.id, text: `${part.name}: destruido` });
      }
    }
  }

  get playerCount() {
    return [...this.members.values()].filter((m) => m.info.id > 0).length;
  }

  connect(socket: WebSocket) {
    const member: Member = {
      info: { id: 0, name: '', variant: 0 },
      socket,
      state: null,
      stateTime: 0,
      alive: true,
      hp: MAX_HP,
      dead: false,
      rockets: [],
      lastFire: 0,
      lastRepair: 0,
      suitO2: 1,
      cabin: false,
    };
    this.members.set(socket, member);

    socket.on('pong', () => (member.alive = true));
    socket.on('message', (data, isBinary) => {
      if (isBinary) return;
      let msg: ClientMessage;
      try {
        msg = JSON.parse(data.toString());
      } catch {
        return;
      }
      this.handle(member, msg);
    });
    socket.on('close', () => this.disconnect(member));
    socket.on('error', () => socket.terminate());
  }

  /** Drops connections that stopped answering pings. */
  heartbeat() {
    for (const m of this.members.values()) {
      if (!m.alive) {
        m.socket.terminate();
        continue;
      }
      m.alive = false;
      m.socket.ping();
    }
  }

  private handle(member: Member, msg: ClientMessage) {
    switch (msg.type) {
      case 'hello':
        return this.join(member, msg);
      case 'state':
        if (member.info.id === 0 || !isValidState(msg.s)) return;
        member.state = msg.s;
        member.stateTime = now();
        return;
      case 'ping':
        return send(member.socket, { type: 'pong', t: msg.t, serverTime: now() });
      case 'fire':
        return this.fire(member, msg.o, msg.d);
      case 'hit':
        return this.hit(member, msg.p);
      case 'interact':
        return this.interact(member, msg.ship, msg.ctl, msg.dir);
      case 'repair':
        return this.repair(member, msg.ship, msg.panel, 'panel');
      case 'repairPart':
        return this.repair(member, msg.ship, msg.part, 'part');
    }
  }

  private join(member: Member, msg: Extract<ClientMessage, { type: 'hello' }>) {
    if (member.info.id !== 0) return;
    if (msg.version !== PROTOCOL_VERSION) {
      send(member.socket, { type: 'reject', reason: 'Versión de cliente distinta a la del servidor.' });
      member.socket.close();
      return;
    }
    if (this.playerCount >= MAX_PLAYERS) {
      send(member.socket, { type: 'reject', reason: `La partida está llena (${MAX_PLAYERS} jugadores).` });
      member.socket.close();
      return;
    }

    const taken = new Set([...this.members.values()].filter((m) => m.info.id > 0).map((m) => m.info.variant));
    let variant = 0;
    while (taken.has(variant)) variant++;

    member.info = { id: this.nextId++, name: sanitizeName(msg.name, variant), variant };
    const spawn = SPAWNS[variant % SPAWNS.length];

    send(member.socket, {
      type: 'welcome',
      id: member.info.id,
      variant,
      players: this.others(member).map((m) => m.info),
      spawn: [spawn[0], 0, spawn[1]],
      worldSeed: WORLD_SEED,
      serverTime: now(),
      edits: this.edits,
      health: this.others(member).map((m) => ({ id: m.info.id, hp: m.hp })),
      ships: this.ships.map((sh) => sh.snapshot()),
    });
    for (const other of this.others(member)) send(other.socket, { type: 'join', player: member.info });
    this.log(`+ ${member.info.name} (#${member.info.id}) — ${this.playerCount}/${MAX_PLAYERS}`);
  }

  private disconnect(member: Member) {
    this.members.delete(member.socket);
    if (member.info.id === 0) return;
    for (const other of this.members.values()) send(other.socket, { type: 'leave', id: member.info.id });
    this.log(`- ${member.info.name} (#${member.info.id}) — ${this.playerCount}/${MAX_PLAYERS}`);
  }

  private fire(m: Member, o: Vec3, d: Vec3) {
    const t = now();
    if (m.info.id === 0 || m.dead || !isFiniteVec(o) || !isFiniteVec(d) || t - m.lastFire < 900) return;
    m.lastFire = t;
    m.rockets = m.rockets.filter((f) => t - f < 12000);
    m.rockets.push(t);
    this.broadcast({ type: 'fire', id: m.info.id, o, d });
  }

  private hit(m: Member, p: Vec3) {
    if (m.info.id === 0 || !isFiniteVec(p) || !m.rockets.length) return;
    m.rockets.shift();
    this.explosion(p, m.info.id, { crater: true });
  }

  /**
   * An explosion at a world point (a rocket, or a ship's engine/tank going up; by = 0): crater
   * near the ground, damage to every ship's panels and machinery (which may chain), and to crew.
   */
  private explosion(p: Vec3, by: number, o: { radius?: number; damage?: number; crater: boolean }, depth = 0) {
    let edit: TerrainEdit | undefined;
    if (o.crater && p[1] - this.terrain.height(p[0], p[2]) < CRATER_MAX_HEIGHT) {
      edit = { x: round2(p[0]), z: round2(p[2]), r: CRATER_RADIUS, d: 1 };
      this.edits.push(edit);
      if (this.edits.length > MAX_EDITS) this.edits.shift();
    }
    this.broadcast({ type: 'explode', id: by, p, edit });
    const scale = (o.radius ?? BLAST_RADIUS) / BLAST_RADIUS;
    for (const ship of this.ships) {
      const r = ship.explode(p, { radius: 2.8 * Math.max(1, scale), damage: o.damage ?? 75 });
      if (r.hp.length) {
        this.broadcast({ type: 'ship', ship: ship.id, hp: r.hp, sw: Object.keys(r.sw).length ? r.sw : undefined, by });
        const holes = r.hp.filter(([, hp]) => hp <= 0).map(([i]) => ship.def.panels[i].id);
        if (holes.length) this.log(`✸ ${ship.def.name}: brecha ${holes.join(', ')} ← ${this.nameOf(by)}`);
      }
      // ruptured tanks go up in turn (bounded chain)
      if (depth < 3) this.shipEvents(ship, r.events.filter((e) => e.type !== 'explode'));
      if (depth < 3) for (const e of r.events) if (e.type === 'explode') this.explosion(ship.toWorld(e.at), 0, { radius: e.radius, damage: e.damage, crater: false }, depth + 1);
    }
    const radius = BLAST_RADIUS * scale;
    for (const target of this.players()) {
      if (target.dead || !target.state) continue;
      const [x, y, z] = target.state.p;
      const dist = Math.hypot(x - p[0], y + 0.9 - p[1], z - p[2]);
      if (dist > radius) continue;
      let dmg = Math.round(110 * (1 - dist / radius));
      if (target.info.id === by) dmg = Math.round(dmg * 0.5);
      if (dmg > 0) this.damage(target, dmg, by);
    }
  }

  /** Suit damage from any cause (by = 0: the environment / the ship). */
  private damage(target: Member, dmg: number, by: number) {
    if (target.dead) return;
    const before = Math.round(target.hp);
    target.hp = Math.max(0, target.hp - dmg);
    const dead = target.hp === 0;
    if (dead) {
      target.dead = true;
      this.log(`☠ ${target.info.name} ← ${by ? this.nameOf(by) : 'entorno'}`);
      setTimeout(() => this.respawn(target), RESPAWN_MS);
    }
    if (dead || Math.round(target.hp) !== before) this.broadcast({ type: 'health', id: target.info.id, hp: Math.round(target.hp), by: by || undefined, dead });
  }

  private nameOf(id: number) {
    return this.players().find((m) => m.info.id === id)?.info.name ?? (id ? `#${id}` : 'la nave');
  }

  /** Eye position of a member (feet + 1.6 m), or null before its first state. */
  private eye(m: Member): Vec3 | null {
    return m.state ? [m.state.p[0], m.state.p[1] + 1.6, m.state.p[2]] : null;
  }

  private interact(m: Member, shipId: unknown, ctl: unknown, dir: unknown) {
    const ship = this.ships.find((s) => s.id === shipId);
    const eye = this.eye(m);
    if (m.info.id === 0 || m.dead || !ship || !eye || !Number.isInteger(ctl) || !ship.def.controls[ctl as number]) return;
    const i = ctl as number;
    // generous: the owner's position is ~100 ms old and it may be moving
    if (dist(eye, ship.controlWorld(i)) > REACH.control + 1.2) return;
    const r = ship.interact(i, dir === 1 || dir === -1 ? dir : 0);
    if ('reason' in r) {
      send(m.socket, { type: 'shipDenied', ship: ship.id, ctl: i, reason: r.reason });
      return;
    }
    if (Object.keys(r.changed).length) this.broadcast({ type: 'ship', ship: ship.id, sw: r.changed, by: m.info.id });
  }

  private repair(m: Member, shipId: unknown, target: unknown, kind: 'panel' | 'part') {
    const ship = this.ships.find((s) => s.id === shipId);
    const eye = this.eye(m);
    const list = kind === 'panel' ? ship?.def.panels : ship?.def.parts;
    if (m.info.id === 0 || m.dead || !ship || !eye || !list || !Number.isInteger(target) || !list[target as number]) return;
    // only with the welder in hand
    const f = m.state?.f ?? 0;
    if (!(f & StateFlags.Welder) || !(f & StateFlags.Armed)) return;
    const i = target as number;
    const t = now();
    // the tool works at a fixed rate whatever the message rate: credit the time since the last tick
    const dt = Math.min(0.25, (t - m.lastRepair) / 1000);
    m.lastRepair = t;
    if (kind === 'part') {
      // machines are big: measure to the box, not the centre
      const part = ship.def.parts[i];
      const local = ship.toLocal(eye);
      if (ship.sys.partDistance(part, local) > REACH.repair + 1.5) return;
      ship.repairPart(i, REPAIR_RATE * dt); // replicated with the state table
      return;
    }
    if (dist(eye, ship.panelWorld(i)) > REACH.repair + 2) return;
    const hp = ship.repair(i, REPAIR_RATE * dt);
    if (hp !== null) this.broadcast({ type: 'ship', ship: ship.id, hp: [[i, hp]], by: m.info.id });
  }

  private respawn(m: Member) {
    if (!this.members.has(m.socket)) return;
    m.hp = MAX_HP;
    m.dead = false;
    m.suitO2 = 1;
    const s = SPAWNS[(m.info.variant + Math.floor(Math.random() * 4)) % SPAWNS.length];
    this.broadcast({ type: 'health', id: m.info.id, hp: m.hp });
    this.broadcast({ type: 'respawn', id: m.info.id, spawn: [s[0], 0, s[1]] });
  }

  private players() {
    return [...this.members.values()].filter((m) => m.info.id > 0);
  }

  private broadcast(msg: ServerMessage) {
    for (const m of this.players()) send(m.socket, msg);
  }

  private others(member: Member) {
    return [...this.members.values()].filter((m) => m !== member && m.info.id > 0);
  }

  private broadcastSnapshot() {
    const players = [...this.members.values()].filter((m) => m.info.id > 0);
    if (players.length < 2) return;
    const t = now();
    for (const target of players) {
      const states = players
        .filter((m) => m !== target && m.state)
        .map((m) => ({ id: m.info.id, t: m.stateTime, s: m.state! }));
      if (states.length) send(target.socket, { type: 'snapshot', t, states });
    }
  }
}

function send(socket: WebSocket, msg: ServerMessage) {
  if (socket.readyState === socket.OPEN) socket.send(JSON.stringify(msg));
}

function sanitizeName(name: unknown, variant: number) {
  const clean = typeof name === 'string' ? name.replace(/[^\p{L}\p{N} _.-]/gu, '').trim().slice(0, 20) : '';
  return clean || `Astronauta ${variant + 1}`;
}

const round2 = (v: number) => Math.round(v * 100) / 100;
const dist = (a: Vec3, b: Vec3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

function isFiniteVec(v: unknown): v is [number, number, number] {
  return Array.isArray(v) && v.length === 3 && v.every((n) => typeof n === 'number' && Number.isFinite(n));
}

function isValidState(s: PlayerState | undefined): boolean {
  return (
    !!s &&
    isFiniteVec(s.p) &&
    isFiniteVec(s.v) &&
    Number.isFinite(s.yaw) &&
    Number.isFinite(s.pitch) &&
    Number.isInteger(s.f)
  );
}

