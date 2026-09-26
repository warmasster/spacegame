import type { WebSocket } from 'ws';
import { MAX_PLAYERS, SERVER_SNAPSHOT_RATE, WORLD_SEED } from '../shared/constants.js';
import {
  PROTOCOL_VERSION,
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

  constructor(private readonly log: (msg: string) => void) {
    this.snapshotTimer = setInterval(() => this.broadcastSnapshot(), 1000 / SERVER_SNAPSHOT_RATE);
  }

  dispose() {
    clearInterval(this.snapshotTimer);
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
    const t = now();
    if (m.info.id === 0 || !isFiniteVec(p) || !m.rockets.length) return;
    m.rockets.shift();
    const edit: TerrainEdit = { x: round2(p[0]), z: round2(p[2]), r: CRATER_RADIUS, d: 1 };
    this.edits.push(edit);
    if (this.edits.length > MAX_EDITS) this.edits.shift();
    this.broadcast({ type: 'explode', id: m.info.id, p, edit });
    for (const target of this.players()) {
      if (target.dead || !target.state) continue;
      const [x, y, z] = target.state.p;
      const dist = Math.hypot(x - p[0], y + 0.9 - p[1], z - p[2]);
      if (dist > BLAST_RADIUS) continue;
      let dmg = Math.round(110 * (1 - dist / BLAST_RADIUS));
      if (target === m) dmg = Math.round(dmg * 0.5);
      if (dmg <= 0) continue;
      target.hp = Math.max(0, target.hp - dmg);
      const dead = target.hp === 0;
      if (dead) {
        target.dead = true;
        this.log(`☠ ${target.info.name} ← ${m.info.name}`);
        setTimeout(() => this.respawn(target), RESPAWN_MS);
      }
      this.broadcast({ type: 'health', id: target.info.id, hp: target.hp, by: m.info.id, dead });
    }
    void t;
  }

  private respawn(m: Member) {
    if (!this.members.has(m.socket)) return;
    m.hp = MAX_HP;
    m.dead = false;
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

