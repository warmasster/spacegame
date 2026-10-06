import type { WebSocket } from 'ws';
import { MAX_PLAYERS, SERVER_SNAPSHOT_RATE, WORLD_SEED } from '../shared/constants.js';
import { CREW_BLAST_RADIUS, REACH, REPAIR_RATE, shipBlast, ShipSim, SYSTEMS_HZ } from '../shared/ship/sim.js';
import { copyPose, FLIGHT_IDLE, MASS, type Lump } from '../shared/ship/flight/index.js';
import { StepClock } from '../shared/time/stepClock.js';
import { Replica } from '../shared/net/replica.js';
import { PROJECTILES, projectileById, projectileOf, terrainImpact, weaponById, type ImpactDef } from '../shared/items/index.js';
import { siteCrews, siteDepots, siteToWorld, startCrates, startShips } from '../shared/ship/spawn.js';
import { shipHullReach } from '../shared/frames/ships.js';
import { bodyGround } from '../shared/actors/ground.js';
import type { Obstacle } from '../shared/actors/walker.js';
import { LOUDNESS, witnesses } from '../shared/actors/perception.js';
import { physicalRegions, regionAt } from '../shared/space/galaxy.js';
import { JUMP, jumpPose, jumpRefusal } from '../shared/space/jump.js';
import { VarSync } from '../shared/ship/state.js';
import { shotFits, works } from '../shared/ship/modules/weapons.js';
import type { SysEvent } from '../shared/ship/systems.js';
import { SUIT, crewStep } from '../shared/ship/crew.js';
import { BODIES, bodyAt, surfaceOf, type CelestialBody } from '../shared/space/body.js';
import type { TerrainMod } from '../shared/space/terrainMods/index.js';
import { spawnPoint } from '../shared/space/world.js';
import { decodeClient, encodeServer } from '../shared/wire.js';
import { LooseObjects } from './objects.js';
import type { Peer } from './peer.js';
import { WorldBridge } from './world.js';
import { ObjectsLink } from './worldObjects.js';
import { PeopleLink } from './worldPeople.js';
import { Npcs } from './npcs.js';
import type { SimClient } from '../sim/host/client.js';
import {
  PROTOCOL_VERSION,
  StateFlags,
  type ClientMessage,
  type PlayerInfo,
  type PlayerState,
  type PoseWire,
  type ServerMessage,
  type Vec3,
} from '../shared/protocol.js';

export const MAX_HP = 100;
const BLAST_RADIUS = CREW_BLAST_RADIUS; // m, damage falls off linearly
const RESPAWN_MS = 4000;
/** Flight step of the ships nobody pilots (Hz); the systems tick every FLIGHT_HZ / SYSTEMS_HZ steps. */
const FLIGHT_HZ = 60;
/** A pilot whose client stopped reporting its flight this long ago (ms) hands the ship to the server. */
const PILOT_STALE_MS = 600;

interface Member {
  info: PlayerInfo;
  socket: WebSocket;
  state: PlayerState | null;
  stateTime: number;
  alive: boolean;
  hp: number;
  dead: boolean;
  /** Projectiles in flight (kind, fire time) — a hit is only accepted for one that was fired. */
  shots: Array<{ k: string; t: number }>;
  lastFire: number;
  lastRepair: number;
  /** Suit oxygen 0..1 and whether it breathes cabin air right now. */
  suitO2: number;
  cabin: boolean;
  /** Last flight report from this member's client (ms), while it pilots. */
  lastFlight: number;
  /** Ships this member follows closely (network interest) and where it was last seen (world). */
  near: Set<number>;
  at: Vec3 | null;
  /** How the server's parts (objects, NPCs, the world) see and reach it. */
  peer: Peer;
  /** Who it is in the world simulation (0 until the world says). */
  simId: number;
}

const now = () => performance.now();
/** Test and admin commands (`dev` messages) are accepted only with DEV_TOOLS=1. */
const DEV_TOOLS = process.env.DEV_TOOLS === '1';

/** A client's time of a state (ms, its step clock), trusted within reason: never ahead of us, never ancient. */
const clampTime = (t: unknown, at = now()) => (typeof t === 'number' && Number.isFinite(t) ? Math.min(at + 50, Math.max(at - 1000, t)) : at);

/**
 * A single shared world instance. The server relays player states (co-op, trusted clients) and is
 * the authority for damage, craters, the ships' machinery and every ship nobody pilots; a pilot's
 * client flies its own ship and reports it here; loose crates are simulated by whoever handles them.
 */
export class Room {
  private members = new Map<WebSocket, Member>();
  private nextId = 1;
  private snapshotTimer: NodeJS.Timeout;
  /**
   * The world: every body's ground is the same deterministic surface the clients build (with its
   * sites), plus the craters of the game (its dynamic modifiers, kept here and sent to newcomers).
   */
  private seed = WORLD_SEED;
  private surfaces = (b: CelestialBody) => surfaceOf(b, this.seed);
  private ships: ShipSim[] = [];
  private sync = new Map<number, VarSync>();
  /** Who flies each ship (the rest the server flies itself). */
  private pilots = new Map<number, Member>();
  /** Ships the server was flying on its last step (a pilot leaving hands them back mid-flight). */
  private serverFlown = new Set<number>();
  /** Loose objects: who simulates each, who knows each (server/objects.ts). */
  private objects = new LooseObjects(
    {
      toWorld: (fr, p, out) => {
        const ship = this.ships.find((s) => s.id === fr);
        if (!ship) return null;
        const w = ship.toWorld(p);
        out[0] = w[0];
        out[1] = w[1];
        out[2] = w[2];
        return out;
      },
    },
    () => this.peers(),
    now,
  );
  private stepTimer: NodeJS.Timeout;
  private lastTick = now();
  private tickAcc = 0;
  private steps = 0;
  private ticks = 0;
  /**
   * Time of each flight step's state (ms): exactly 1/FLIGHT_HZ apart whatever the timer does, kept
   * on the wall clock by slewing. Poses go out stamped with it (a stamp from the timer's jitter is
   * a ship that jumps back and forth on every screen: docs/MOVIMIENTO.md).
   */
  private clock = new StepClock(1000 / FLIGHT_HZ);
  /** Ships a pilot flies: their reports, carried to our present every step (shared/net/replica.ts). */
  private piloted = new Map<number, Replica>();
  /** The world simulation (docs/MUNDO.md), once its thread is up: where loose objects are kept. */
  private world: { sim: SimClient; bridge: WorldBridge } | null = null;
  /** The world's people near a player, with a body (server/npcs.ts). */
  private npcs = new Npcs(
    {
      ground: () => bodyGround(this.surfaces),
      obstacles: (p, r) => {
        const out: Obstacle[] = [];
        for (const s of this.ships) {
          const reach = shipHullReach(s.def, 0) * 0.75;
          if (dist(s.pose.p, p) < r + reach) out.push({ c: [s.pose.p[0], s.pose.p[1], s.pose.p[2]], r: reach });
        }
        return out;
      },
    },
    () => this.peers(),
  );

  constructor(private readonly log: (msg: string) => void) {
    this.snapshotTimer = setInterval(() => this.broadcastSnapshot(), 1000 / SERVER_SNAPSHOT_RATE);
    this.clock.sync(now());
    this.buildWorld();
    // flight and ship machinery run on a fixed step whatever the timer jitter
    this.stepTimer = setInterval(() => this.step(), 1000 / FLIGHT_HZ / 2);
  }

  dispose() {
    clearInterval(this.snapshotTimer);
    clearInterval(this.stepTimer);
    this.world?.bridge.stop();
  }

  /**
   * The world's thread is up (server/world.ts): loose objects are kept there from now on, and its
   * people walk about near the players.
   */
  attachWorld(sim: SimClient) {
    const links = [
      new ObjectsLink({
        objects: this.objects,
        startCrates: () => startCrates(this.ships, this.surfaces),
        depots: () => siteDepots(this.surfaces),
        taker: (by) => this.taker(by),
        log: this.log,
      }),
      new PeopleLink({
        npcs: this.npcs,
        crews: () => siteCrews(this.surfaces),
        siteToWorld: (site, x, z) => siteToWorld(this.surfaces, site, x, z),
        now: () => this.clock.t,
        log: this.log,
      }),
    ];
    const bridge = new WorldBridge(
      sim,
      {
        peers: () => this.peers(),
        hosts: () => this.ships.map((s): [number, number, number, number] => [s.id, s.pose.p[0], s.pose.p[1], s.pose.p[2]]),
        log: this.log,
      },
      links,
    );
    this.world = { sim, bridge };
    bridge.start().catch((e: Error) => this.log(`Mundo: la sala sigue sin él (${e.message})`));
    // those already here: who they are in the world
    for (const m of this.players()) this.meet(m);
  }

  /** Before the world is saved: what came to rest goes in. */
  async flushWorld() {
    await this.world?.bridge.flush().catch(() => undefined);
  }

  /** A fresh world: no craters, the ships parked on their pads, their cargo aboard. */
  private buildWorld() {
    for (const b of Object.values(BODIES)) this.surfaces(b)?.mods.clearDynamic();
    this.ships = startShips(this.surfaces);
    this.sync.clear();
    for (const ship of this.ships) this.sync.set(ship.id, new VarSync(ship.vars, ship.st));
    this.pilots.clear();
    this.serverFlown.clear();
    this.objects.reset(startCrates(this.ships, this.surfaces));
    this.piloted.clear();
  }

  /** Fixed steps: flight at FLIGHT_HZ, ship systems + crew life support at SYSTEMS_HZ. */
  private step() {
    const t = now();
    this.tickAcc = Math.min(this.tickAcc + (t - this.lastTick) / 1000, 0.25);
    this.lastTick = t;
    const h = 1 / FLIGHT_HZ;
    const every = Math.round(FLIGHT_HZ / SYSTEMS_HZ);
    while (this.tickAcc >= h) {
      this.tickAcc -= h;
      this.steps++;
      this.clock.step();
      this.stepFlight(h);
      if (this.steps % every === 0) {
        this.stepSystems(1 / SYSTEMS_HZ);
        // people on foot: a step, stamped with this step's time, to whoever sees them
        this.npcs.step(1 / SYSTEMS_HZ, this.clock.t);
        this.npcs.broadcast(this.clock.t);
      }
    }
    // the last step's state belongs to now minus what is still to be simulated
    this.clock.sync(t - this.tickAcc * 1000);
  }

  /**
   * Ships nobody pilots (or whose pilot's client went quiet) are flown here with the stick idle:
   * coupled flight holds them where they are, the autopilot keeps flying, a parked ship sleeps.
   * Their pose goes to everyone at 30 Hz while awake, once a second asleep.
   */
  private stepFlight(h: number) {
    const wall = now();
    // the time this step's state belongs to (not when the timer fired)
    const t = this.clock.t;
    for (const ship of this.ships) {
      const pilot = this.pilots.get(ship.id);
      const reports = this.piloted.get(ship.id);
      if (pilot && wall - pilot.lastFlight < PILOT_STALE_MS) {
        // a pilot's ship is where its reports say it is now (they are a trip old): the systems
        // and the reach checks work on the present
        const s = reports?.sample(t);
        if (s) copyPose(ship.pose, s);
        this.serverFlown.delete(ship.id);
        continue;
      }
      if (reports) {
        // taking over: from where its reports put it the step before — this step flies it to `t`
        // (not from the last report, a network trip old: at 1.6 km/s that is tens of metres back)
        const s = reports.sample(t - this.clock.last);
        if (s) copyPose(ship.pose, s);
        this.piloted.delete(ship.id);
      }
      if (!this.serverFlown.has(ship.id)) {
        this.serverFlown.add(ship.id);
        ship.flight.resume();
      }
      const env = { extra: this.aboard(ship), surface: this.surfaces(bodyAt(ship.pose.p)) };
      // sim LOD: parked with nobody near, it rests (its contacts need not finish settling)
      if (!ship.flight.sleeping && ship.landed && !this.pilots.has(ship.id) && Math.hypot(...ship.pose.v) < REST_V && !this.playerWithin(ship.pose.p, REST_NEAR_M)) ship.flight.settle(env);
      ship.flight.step(h, FLIGHT_IDLE, env);
      const every = ship.flight.sleeping ? FLIGHT_HZ : 2;
      // interest: players close get every pose, the far ones a couple a second (their compass and
      // the distant speck still move)
      if (this.steps % every === 0) this.toPose(ship, this.poseMessage(ship, t), this.steps % FAR_POSE_EVERY === 0);
    }
  }

  /** A ship's pose: to everyone near it (or aboard) every time, to the rest only when `far` is due. */
  private toPose(ship: ShipSim, msg: ServerMessage, far: boolean, skip?: Member) {
    let data: string | ArrayBuffer | null = null;
    for (const m of this.members.values()) {
      if (m === skip || m.info.id <= 0 || m.socket.readyState !== m.socket.OPEN) continue;
      if (!far && !this.poseNear(m, ship)) continue;
      data ??= encodeServer(msg) ?? JSON.stringify(msg);
      m.socket.send(data);
    }
  }

  private playerWithin(p: Vec3, r: number) {
    for (const m of this.members.values()) if (m.info.id > 0 && m.at && dist(m.at, p) < r) return true;
    return false;
  }

  private poseNear(m: Member, ship: ShipSim) {
    if (m.state?.fr === ship.id || this.pilots.get(ship.id) === m) return true;
    const at = m.at;
    return !!at && dist(at, ship.pose.p) < POSE_NEAR_M;
  }

  /**
   * Interest, every few ticks: which ships each player follows closely (state diffs, switches,
   * panels, the systems' messages). A ship coming into interest sends its whole state first.
   */
  private updateInterest() {
    for (const m of this.members.values()) {
      if (m.info.id <= 0) continue;
      m.at = this.feet(m);
      for (const ship of this.shipsList()) {
        const aboard = m.state?.fr === ship.id || this.pilots.get(ship.id) === m;
        const d = m.at ? dist(m.at, ship.pose.p) : Infinity;
        const was = m.near.has(ship.id);
        const now = aboard || d < (was ? SHIP_OUT_M : SHIP_IN_M);
        if (now === was) continue;
        if (now) {
          m.near.add(ship.id);
          send(m.socket, { type: 'shipSync', snap: ship.snapshot() });
        } else m.near.delete(ship.id);
      }
    }
    this.objects.interest();
    this.npcs.interest();
  }

  private shipsList() {
    return this.ships;
  }

  /** Every connected socket's player, as the server's parts see it. */
  private *peers(): Generator<Peer> {
    for (const m of this.members.values()) yield m.peer;
  }

  /** A ship's own news (switches, panels, state diffs, its messages): to the players following it. */
  private toNear(ship: ShipSim, msg: ServerMessage) {
    let data: string | ArrayBuffer | null = null;
    for (const m of this.members.values()) {
      if (m.info.id <= 0 || !m.near.has(ship.id) || m.socket.readyState !== m.socket.OPEN) continue;
      data ??= encodeServer(msg) ?? JSON.stringify(msg);
      m.socket.send(data);
    }
  }

  private poseMessage(ship: ShipSim, t: number): ServerMessage {
    const r4 = (n: number) => Math.round(n * 1e4) / 1e4;
    const r5 = (n: number) => Math.round(n * 1e5) / 1e5;
    const p = ship.pose;
    return { type: 'shipPose', ship: ship.id, t, p: p.p.map(r4) as Vec3, q: p.q.map(r5) as [number, number, number, number], v: p.v.map(r4) as Vec3, w: p.w.map(r5) as Vec3, landed: ship.landed, pad: ship.onPad };
  }

  /** Mass aboard a ship that its data doesn't know: crew and crates in its frame (ship space). */
  /** Per ship: the lumps aboard, reused every flight step (the flight model reads them right away). */
  private lumps = new Map<number, { out: Lump[]; pool: Lump[] }>();

  private aboard(ship: ShipSim): Lump[] {
    let l = this.lumps.get(ship.id);
    if (!l) this.lumps.set(ship.id, (l = { out: [], pool: [] }));
    const out = l.out;
    out.length = 0;
    let k = 0;
    const lump = () => (l!.pool[k++] ??= { m: 0, c: [0, 0, 0] });
    for (const m of this.members.values()) {
      if (m.info.id <= 0 || m.state?.fr !== ship.id || m.dead) continue;
      const x = lump();
      x.m = MASS.crewKg;
      x.c = m.state.p;
      x.half = undefined;
      x.group = 'tripulación';
      out.push(x);
    }
    for (const c of this.objects.inFrame(ship.id)) {
      const x = lump();
      x.m = c.mass;
      x.c = c.p;
      x.half = c.half;
      x.group = 'carga';
      out.push(x);
    }
    return out;
  }

  /** Per ship: simulated time its systems still owe (far, parked ships take it in bigger steps). */
  private sysOwed = new Map<number, number>();

  /**
   * Ship systems + crew life support; state diffs to everyone at half the rate. Level of detail:
   * a ship with nobody within FAR_M and parked asleep runs its machinery every FAR_EVERY ticks
   * with the time it owes (the same physics in bigger steps: nobody is there to see it).
   */
  private stepSystems(h: number) {
    this.ticks++;
    if (this.ticks % INTEREST_EVERY === 0) this.updateInterest();
    const players = this.players().filter((m) => m.state && !m.dead);
    // ships tick with their crew; suits breathe the cabin where it is breathable, drink from a
    // seat umbilical, or spend their reserve (shared/ship/crew.ts, same rules offline)
    const crew = players.map((m) => ({ p: this.feet(m)!, seated: (m.state!.f & StateFlags.Seated) !== 0, o2: m.suitO2 }));
    const due: ShipSim[] = [];
    const dts = new Map<ShipSim, number>();
    for (const ship of this.ships) {
      const owed = (this.sysOwed.get(ship.id) ?? 0) + h;
      let near = !ship.flight.sleeping;
      for (let i = 0; i < crew.length && !near; i++) {
        const p = crew[i].p;
        const dx = p[0] - ship.pose.p[0];
        const dy = p[1] - ship.pose.p[1];
        const dz = p[2] - ship.pose.p[2];
        near = dx * dx + dy * dy + dz * dz < FAR_M * FAR_M;
      }
      if (near || owed >= FAR_EVERY * h - 1e-9) {
        due.push(ship);
        dts.set(ship, owed);
        this.sysOwed.set(ship.id, 0);
      } else this.sysOwed.set(ship.id, owed);
    }
    const r = crewStep(due, crew, h, (ship, ctx) => ship.tick(dts.get(ship) ?? h, ctx));
    due.forEach((ship, k) => {
      const res = r.results[k];
      // switches the machinery moved, panels that tore under pressure
      const sw = hasKeys(res.sw) ? res.sw : undefined;
      const hp = res.hp.length ? res.hp : undefined;
      if (sw || hp) this.toNear(ship, { type: 'ship', ship: ship.id, sw, hp });
      const blown = res.hp.filter(([, v]) => v <= 0).map(([i]) => ship.def.panels[i].id);
      if (blown.length) this.log(`✸ ${ship.def.name}: ${blown.join(', ')} reventado por la presión`);
      this.shipEvents(ship, res.events);
    });
    players.forEach((m, i) => {
      m.suitO2 = r.o2[i];
      m.cabin = r.cabin[i];
      if (m.suitO2 <= 0) this.damage(m, SUIT.choke * h, 0);
    });
    if (this.ticks % 2 === 0) {
      for (const ship of this.ships) {
        const d = this.sync.get(ship.id)!.diff(ship.st);
        if (d.length) this.toNear(ship, { type: 'shipSt', ship: ship.id, d });
      }
    }
    if (this.ticks % 10 === 0) for (const m of players) send(m.socket, { type: 'vitals', o2: Math.round(m.suitO2 * 1000) / 1000, cabin: m.cabin });
  }

  /** Resolve what the ship systems reported: internal explosions, messages. */
  private shipEvents(ship: ShipSim, events: SysEvent[]) {
    for (const e of events) {
      if (e.type === 'explode') {
        this.log(`✸ ${ship.def.name}: ${e.cause}`);
        this.explosion(ship.toWorld(e.at), 0, { radius: e.radius, damage: e.damage });
        this.toNear(ship, { type: 'say', ship: ship.id, text: e.cause });
      } else if (e.type === 'say') this.toNear(ship, { type: 'say', ship: ship.id, text: e.text });
      else if (e.type === 'trip') {
        const c = ship.def.subsystems.find((x) => x.id === e.circuit)!;
        this.toNear(ship, { type: 'say', ship: ship.id, text: `Disyuntor saltado por sobrecarga: ${c.label}` });
      } else if (e.type === 'destroyed') {
        const part = ship.def.parts.find((p) => p.id === e.part);
        if (part) this.toNear(ship, { type: 'say', ship: ship.id, text: `${part.name}: destruido` });
      }
    }
  }

  /** Empty room only: a new terrain, no craters, ships sat on the fresh ground. */
  private reseed(seed: number) {
    this.seed = seed >>> 0;
    this.buildWorld();
    this.world?.bridge.reseed().catch((e: Error) => this.log(`Mundo: no se pudo cambiar de terreno (${e.message})`));
    this.log(`mundo nuevo · semilla ${this.seed}`);
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
      shots: [],
      lastFire: 0,
      lastRepair: 0,
      suitO2: 1,
      cabin: false,
      lastFlight: 0,
      near: new Set(),
      at: null,
      peer: null as unknown as Peer,
      simId: 0,
    };
    member.peer = {
      get id() {
        return member.info.id;
      },
      get dead() {
        return member.dead;
      },
      get at() {
        return member.at;
      },
      get fr() {
        return member.state?.fr ?? 0;
      },
      send: (data) => {
        if (socket.readyState === socket.OPEN) socket.send(data);
      },
    };
    this.members.set(socket, member);

    socket.on('pong', () => (member.alive = true));
    socket.on('message', (data, isBinary) => {
      if (isBinary) {
        // the frequent ones (state, flight) come in binary (shared/wire.ts)
        const b = Array.isArray(data) ? Buffer.concat(data) : Buffer.isBuffer(data) ? data : Buffer.from(data);
        const m = decodeClient(new DataView(b.buffer, b.byteOffset, b.byteLength));
        if (m) this.handle(member, m);
        return;
      }
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
        // the time of the step it belongs to (its owner's step clock), not when it got here
        member.stateTime = clampTime(msg.t);
        return;
      case 'ping':
        return send(member.socket, { type: 'pong', t: msg.t, serverTime: now() });
      case 'fire':
        if (msg.m !== undefined) return this.fireMount(member, msg.m, msg.o, msg.d, msg.v, msg.fr, clampTime(msg.t));
        return this.fire(member, msg.w, msg.o, msg.d, msg.v, msg.fr, clampTime(msg.t));
      case 'aim':
        return this.aim(member, msg.ship, msg.m, msg.y, msg.p);
      case 'hit':
        if (!projectileById(msg.k)) return;
        return this.hit(member, msg.k, msg.p, msg.fr);
      case 'interact':
        return this.interact(member, msg.ship, msg.ctl, msg.dir);
      case 'repair':
        return this.repair(member, msg.ship, msg.panel, 'panel');
      case 'repairPart':
        return this.repair(member, msg.ship, msg.part, 'part');
      case 'pilot':
        return this.pilot(member, msg.ship, msg.on);
      case 'jump':
        return this.jump(member, msg.ship, msg.to);
      case 'flight':
        return this.flight(member, msg);
      case 'crateTake':
        return this.objects.take(member.peer, msg.id);
      case 'crate':
        return this.objects.move(member.peer, msg.c, msg.rest === true, (fr) => this.ships.some((s) => s.id === fr));
      case 'dev':
        if (DEV_TOOLS && member.info.id > 0) this.dev(member, msg.cmd, msg.a ?? {});
        return;
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

    if (this.playerCount === 0 && Number.isInteger(msg.seed) && (msg.seed as number) > 0) this.reseed(msg.seed as number);
    member.info = { id: this.nextId++, name: sanitizeName(msg.name, variant), variant };
    const spawn = spawnPoint(variant, this.surfaces);
    // it is where it will appear: what is near there it knows from the start
    member.at = spawn;

    send(member.socket, {
      type: 'welcome',
      id: member.info.id,
      variant,
      players: this.others(member).map((m) => m.info),
      spawn,
      worldSeed: this.seed,
      serverTime: now(),
      mods: this.dynamicMods(),
      health: this.others(member).map((m) => ({ id: m.info.id, hp: m.hp })),
      ships: this.ships.map((sh) => sh.snapshot()),
      pilots: [...this.pilots].map(([ship, m]): [number, number] => [ship, m.info.id]),
      crates: this.objects.welcome(member.peer),
    });
    for (const other of this.others(member)) send(other.socket, { type: 'join', player: member.info });
    this.log(`+ ${member.info.name} (#${member.info.id}) — ${this.playerCount}/${MAX_PLAYERS}`);
    this.meet(member);
  }

  /** Who a player is in the world (the same person every session, by name). */
  private meet(member: Member) {
    this.world?.sim
      .ask<number>('players.join', { name: member.info.name })
      .then((id) => (member.simId = id))
      .catch(() => undefined);
  }

  /** A player taking an object: who it is in the world, where, and which of the world's people noticed. */
  private taker(by: number): { actor: number; at: Vec3; witnesses: number[] } | null {
    const m = this.players().find((x) => x.info.id === by);
    const at = m ? this.centre(m) : null;
    if (!m || !at) return null;
    const seen = witnesses({ host: m.state?.fr ?? 0, p: [at[0], at[1], at[2]], air: false, loud: LOUDNESS.handle, visible: true }, this.npcs.eyes());
    return { actor: m.simId, at: [at[0], at[1], at[2]], witnesses: seen };
  }

  private disconnect(member: Member) {
    this.members.delete(member.socket);
    if (member.info.id === 0) return;
    // the ship it flew and the crates it handled are nobody's now
    for (const [ship, m] of [...this.pilots]) {
      if (m !== member) continue;
      this.pilots.delete(ship);
      this.broadcast({ type: 'pilot', ship, id: 0 });
    }
    this.objects.leave(member.peer);
    this.npcs.leave(member.peer);
    for (const other of this.members.values()) send(other.socket, { type: 'leave', id: member.info.id });
    this.log(`- ${member.info.name} (#${member.info.id}) — ${this.playerCount}/${MAX_PLAYERS}`);
  }

  /** A shot of weapon `w` (its catalog decides what it fires and how often). */
  private fire(m: Member, w: string, o: Vec3, d: Vec3, v?: Vec3, fr?: number, at = now()) {
    const t = now();
    const weapon = weaponById(w);
    const kind = projectileOf(weapon);
    if (m.info.id === 0 || m.dead || !weapon || weapon.mounted || !kind || !isFiniteVec(o) || !isFiniteVec(d)) return;
    // a little slack on the weapon's rate (the network bunches messages)
    if (t - m.lastFire < weapon.cooldown * 1000 * 0.75) return;
    // fired aboard: in the space of a ship that exists
    if (fr !== undefined && !this.ships.some((s) => s.id === fr)) return;
    // the launcher's own velocity (a ship in orbit): passed on as it came, if it makes sense
    if (v !== undefined && (!isFiniteVec(v) || Math.hypot(v[0], v[1], v[2]) > 20000)) v = undefined;
    m.lastFire = t;
    m.shots = m.shots.filter((s) => t - s.t < PROJECTILES[s.k].life * 1000 + 3000);
    m.shots.push({ k: kind.id, t });
    this.broadcast({ type: 'fire', id: m.info.id, w: weapon.id, o, d, v, fr, t: at });
    const ship = fr !== undefined ? this.ships.find((s) => s.id === fr) : undefined;
    this.report('shot', m, ship ? ship.toWorld(o) : o, fr ?? 0, LOUDNESS.shot);
  }

  /** The ship whose mount `i` this member works now (seated in a seat that lists it), or null. */
  private gunnery(m: Member, shipId: number, i: number): ShipSim | null {
    const s = m.state;
    if (m.info.id === 0 || m.dead || !s || s.fr !== shipId || !(s.f & StateFlags.Seated) || !Number.isInteger(i)) return null;
    const ship = this.ships.find((x) => x.id === shipId);
    return ship && ship.mounts && works(ship.def, s.p, i) ? ship : null;
  }

  /** The gunner's aim for a mount: its head slews there (the angles reach everyone as ship state). */
  private aim(m: Member, shipId: number, i: number, y: number, p: number) {
    this.gunnery(m, shipId, i)?.mounts!.aim(i, y, p);
  }

  /** A shot of a ship's weapon mount `i` (ship space): working, loaded, at its rate and where its head points. */
  private fireMount(m: Member, i: number, o: Vec3, d: Vec3, v?: Vec3, fr?: number, at = now()) {
    if (fr === undefined || !isFiniteVec(o) || !isFiniteVec(d)) return;
    const ship = this.gunnery(m, fr, i);
    const mounts = ship?.mounts;
    if (!ship || !mounts || !shotFits(mounts, ship.st, i, o, d)) return;
    const t = now();
    if (mounts.tryFire(ship.st, i, t / 1000) < 0) return;
    const weapon = weaponById(mounts.list[i].kind.weapon)!;
    const kind = projectileOf(weapon)!;
    if (v !== undefined && (!isFiniteVec(v) || Math.hypot(v[0], v[1], v[2]) > 20000)) v = undefined;
    m.shots = m.shots.filter((s) => t - s.t < PROJECTILES[s.k].life * 1000 + 3000);
    m.shots.push({ k: kind.id, t });
    this.broadcast({ type: 'fire', id: m.info.id, w: weapon.id, o, d, v, fr, m: i, t: at });
    this.report('shot', m, ship.toWorld(o), ship.id, LOUDNESS.shot);
  }

  /** The shooter's word on where its projectile of kind `k` stopped. */
  private hit(m: Member, k: string, p: Vec3, fr?: number) {
    const i = m.shots.findIndex((s) => s.k === k);
    if (m.info.id === 0 || !isFiniteVec(p) || i < 0) return;
    // in or against a ship: its space, to the world where the ship is here
    const ship = fr === undefined ? null : this.ships.find((s) => s.id === fr);
    if (fr !== undefined && !ship) return;
    m.shots.splice(i, 1);
    this.impact(ship ? ship.toWorld(p) : p, m.info.id, PROJECTILES[k].impact, 0, ship ? { fr: ship.id, l: p } : undefined, k);
  }

  /** A ship's own blast (a tank, an engine going up): crew scale radius (m) and hull damage. */
  private explosion(p: Vec3, by: number, o: { radius?: number; damage?: number }, depth = 0, aboard?: { fr: number; l: Vec3 }) {
    const radius = o.radius ?? BLAST_RADIUS;
    this.impact(p, by, { radius, crew: 110, falloff: true, hull: shipBlast(o.radius, o.damage), fx: 'blast' }, depth, aboard);
  }

  /**
   * Something went off or struck at a world point (a projectile, or a ship's engine/tank going up;
   * by = 0): crater near the ground, damage to every ship's panels and machinery (which may
   * chain), and to crew. `aboard`: it happened in or against that ship, there in its space (the
   * clients draw it there); `k`: the projectile kind, for how the clients show it.
   */
  private impact(p: Vec3, by: number, o: ImpactDef, depth = 0, aboard?: { fr: number; l: Vec3 }, k?: string) {
    const mod = terrainImpact(o.terrain, p, this.surfaces);
    // a crater wherever it went off on the ground, on whatever body: a modifier of its surface
    const body = bodyAt(p);
    const surface = this.surfaces(body);
    if (mod && surface) {
      // every client adds the same one in the same order (a repeat on the spot deepens the old one)
      surface.addMod(structuredClone(mod));
      // a crater under a parked ship: it has to notice the ground moved
      for (const ship of this.ships) if (dist(ship.pose.p, p) < 20) ship.flight.wake();
    }
    this.broadcast({ type: 'explode', id: by, p, mod, fr: aboard?.fr, l: aboard?.l, k });
    if (depth === 0) this.report('explosion', by, p, aboard?.fr ?? 0, LOUDNESS.explosion);
    for (const ship of this.ships) {
      const r = ship.explode(p, o.hull);
      if (r.hp.length) {
        this.toNear(ship, { type: 'ship', ship: ship.id, hp: r.hp, sw: hasKeys(r.sw) ? r.sw : undefined, by });
        const holes = r.hp.filter(([, hp]) => hp <= 0).map(([i]) => ship.def.panels[i].id);
        if (holes.length) this.log(`✸ ${ship.def.name}: brecha ${holes.join(', ')} ← ${this.nameOf(by)}`);
      }
      // ruptured tanks go up in turn (bounded chain)
      if (depth < 3) this.shipEvents(ship, r.events.filter((e) => e.type !== 'explode'));
      if (depth < 3) for (const e of r.events) if (e.type === 'explode') this.explosion(ship.toWorld(e.at), 0, { radius: e.radius, damage: e.damage }, depth + 1, { fr: ship.id, l: e.at });
    }
    for (const target of this.players()) {
      if (target.dead || !target.state) continue;
      const c = this.centre(target)!;
      const d = Math.hypot(c[0] - p[0], c[1] - p[1], c[2] - p[2]);
      if (d > o.radius) continue;
      let dmg = Math.round(o.falloff ? o.crew * (1 - d / o.radius) : o.crew);
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
    const at = this.centre(target);
    if (by && by !== target.info.id && at) this.report('hurt', by, at, target.state?.fr ?? 0, LOUDNESS.voice, target.simId);
  }

  /**
   * A fact of the world (docs/MUNDO.md §12): what a player did, and which of the world's people
   * noticed it (their eyes and ears: shared/actors/perception.ts). Nothing if the world isn't up.
   */
  private report(kind: string, by: Member | number, at: Vec3, host: number, loud: number, subject = 0) {
    const sim = this.world?.sim;
    if (!sim) return;
    const m = typeof by === 'number' ? this.players().find((x) => x.info.id === by) : by;
    const seen = witnesses({ host, p: [at[0], at[1], at[2]], air: false, loud, visible: true }, this.npcs.eyes());
    sim.fact(kind, m?.simId ?? 0, subject, 0, { witnesses: seen, at: [at[0], at[1], at[2]] });
  }

  private nameOf(id: number) {
    return this.players().find((m) => m.info.id === id)?.info.name ?? (id ? `#${id}` : 'la nave');
  }

  /** World position of a member's feet (its state is in its frame), or null before its first state. */
  private feet(m: Member): Vec3 | null {
    const s = m.state;
    if (!s) return null;
    const ship = s.fr ? this.ships.find((x) => x.id === s.fr) : undefined;
    return ship ? ship.toWorld(s.p) : s.p;
  }

  /** World position of a member's centre (0.9 m up its frame's vertical: the deck's, or away from the body). */
  private centre(m: Member): Vec3 | null {
    const s = m.state;
    if (!s) return null;
    const ship = s.fr ? this.ships.find((x) => x.id === s.fr) : undefined;
    if (ship) return ship.toWorld([s.p[0], s.p[1] + 0.9, s.p[2]]);
    const c = bodyAt(s.p).center;
    const u: Vec3 = [s.p[0] - c[0], s.p[1] - c[1], s.p[2] - c[2]];
    const k = 0.9 / (Math.hypot(u[0], u[1], u[2]) || 1);
    return [s.p[0] + u[0] * k, s.p[1] + u[1] * k, s.p[2] + u[2] * k];
  }

  /** Eye position of a member (feet + 1.6 m up its frame), or null before its first state. */
  private eye(m: Member): Vec3 | null {
    const s = m.state;
    if (!s) return null;
    const ship = s.fr ? this.ships.find((x) => x.id === s.fr) : undefined;
    return ship ? ship.toWorld([s.p[0], s.p[1] + 1.6, s.p[2]]) : [s.p[0], s.p[1] + 1.6, s.p[2]];
  }

  /**
   * Take or leave the helm. The first to sit flies the ship; a pilot whose client went quiet can
   * be replaced. Everyone learns who flies it (that client starts integrating it, the others play
   * its pose back).
   */
  private pilot(m: Member, shipId: unknown, on: unknown) {
    const ship = this.ships.find((s) => s.id === shipId);
    if (!ship || m.info.id === 0) return;
    const cur = this.pilots.get(ship.id);
    if (on === true && !m.dead) {
      if (cur && cur !== m && now() - cur.lastFlight < PILOT_STALE_MS * 3) {
        send(m.socket, { type: 'pilot', ship: ship.id, id: cur.info.id });
        return;
      }
      this.pilots.set(ship.id, m);
      m.lastFlight = now();
      this.broadcast({ type: 'pilot', ship: ship.id, id: m.info.id });
    } else if (on !== true && cur === m) {
      this.pilots.delete(ship.id);
      this.broadcast({ type: 'pilot', ship: ship.id, id: 0 });
    }
  }

  /** The pilot's flight: adopt it (the systems burn from its outputs) and pass the pose on. */
  private flight(m: Member, msg: Extract<ClientMessage, { type: 'flight' }>) {
    const ship = this.ships.find((s) => s.id === msg.ship);
    if (!ship || this.pilots.get(ship.id) !== m || !isPose(msg) || !Number.isFinite(msg.t) || !Number.isFinite(msg.agl) || !Array.isArray(msg.out)) return;
    // only a jump takes a ship to another system (and only here): reports from before one are late
    if (regionAt(msg.p).index !== regionAt(ship.pose.p).index) return;
    m.lastFlight = now();
    ship.flight.adopt({ ...msg, out: msg.out.map((v) => (typeof v === 'number' && Number.isFinite(v) ? v : 0)) });
    const t = clampTime(msg.t);
    let reports = this.piloted.get(ship.id);
    if (!reports) this.piloted.set(ship.id, (reports = new Replica({ hostDelay: 0 })));
    reports.push({ t, fr: 0, p: [...msg.p], v: [...msg.v], q: [...msg.q], w: [...msg.w] });
    const out: ServerMessage = { type: 'shipPose', ship: ship.id, t, p: msg.p, q: msg.q, v: msg.v, w: msg.w, landed: msg.landed, pad: msg.pad };
    const k = (this.flightMsgs.get(ship.id) ?? 0) + 1;
    this.flightMsgs.set(ship.id, k);
    this.toPose(ship, out, k % FAR_FLIGHT_EVERY === 0, m);
  }

  /** Pilot reports relayed per ship (the far players get one in FAR_FLIGHT_EVERY). */
  private flightMsgs = new Map<number, number>();

  /** When each ship last jumped (ms). */
  private jumps = new Map<number, number>();

  /**
   * Its pilot asks for a jump (shared/space/jump.ts): the ship goes to region `to`, at rest over its
   * body, and everything aboard with it (it all lives in the ship's space). Everyone is told where
   * it is now; the pilot's client flies on from there.
   */
  private jump(m: Member, shipId: unknown, to: unknown) {
    const ship = this.ships.find((s) => s.id === shipId);
    if (!ship || m.info.id === 0 || m.dead || this.pilots.get(ship.id) !== m || typeof to !== 'number') return;
    const t = now();
    const why = jumpRefusal(ship.pose.p, ship.landed, to) ?? (t - (this.jumps.get(ship.id) ?? -Infinity) < JUMP.cooldownS * 1000 ? 'el motor de salto aún se está cargando' : null);
    if (why) {
      send(m.socket, { type: 'say', ship: ship.id, text: `Salto: ${why}` });
      return;
    }
    this.jumps.set(ship.id, t);
    const region = physicalRegions()[to];
    jumpPose(ship.pose, region);
    ship.flight.wake();
    // what the pilot reported was the other system: start from here
    this.piloted.get(ship.id)?.seed({ t: this.clock.t, fr: 0, p: [...ship.pose.p], v: [0, 0, 0], q: [...ship.pose.q], w: [0, 0, 0] });
    this.broadcast({ type: 'jump', ship: ship.id, to, p: [ship.pose.p[0], ship.pose.p[1], ship.pose.p[2]] });
    this.log(`⇝ ${ship.def.name} salta a ${region.system.name} (${m.info.name})`);
  }

  /** Test and admin commands (DEV_TOOLS=1 only): make things appear and go, at runtime. */
  private dev(m: Member, cmd: string, a: Record<string, unknown>) {
    const vec = (v: unknown, d: Vec3): Vec3 => (isFiniteVec(v) ? v : d);
    switch (cmd) {
      case 'obj.spawn': {
        const fr = Number.isInteger(a.fr) && this.ships.some((s) => s.id === a.fr) ? (a.fr as number) : 0;
        const p = vec(a.p, m.at ?? [0, 0, 0]);
        this.objects.spawn({ fr, p: [...p], q: [0, 0, 0, 1], v: [0, 0, 0], w: [0, 0, 0], owner: 0, kind: typeof a.kind === 'string' ? a.kind : undefined, half: vec(a.half, [0.3, 0.3, 0.3]), mass: typeof a.mass === 'number' ? a.mass : 30, paint: a.paint === 'grey' ? 'grey' : 'orange' });
        return;
      }
      case 'obj.despawn':
        if (typeof a.id === 'number') this.objects.despawn(a.id);
        return;
      case 'world.ask': {
        const sim = this.world?.sim;
        const reply = (data: unknown) => send(m.socket, { type: 'devReply', cmd, data });
        if (!sim || typeof a.name !== 'string') return reply({ error: 'sin mundo' });
        sim.ask(a.name, a.payload).then(reply, (e: Error) => reply({ error: e.message }));
        return;
      }
      case 'world.query': {
        const sim = this.world?.sim;
        const reply = (data: unknown) => send(m.socket, { type: 'devReply', cmd, data });
        if (!sim || !a.q || typeof a.q !== 'object') return reply({ error: 'sin mundo' });
        sim.query(a.q as Parameters<SimClient['query']>[0]).then(reply, (e: Error) => reply({ error: e.message }));
        return;
      }
      case 'world.save': {
        const sim = this.world?.sim;
        if (!sim) return send(m.socket, { type: 'devReply', cmd, data: { error: 'sin mundo' } });
        this.flushWorld()
          .then(() => sim.save())
          .then(
            (info) => send(m.socket, { type: 'devReply', cmd, data: info }),
            (e: Error) => send(m.socket, { type: 'devReply', cmd, data: { error: e.message } }),
          );
        return;
      }
    }
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
    if (hasKeys(r.changed)) this.toNear(ship, { type: 'ship', ship: ship.id, sw: r.changed, by: m.info.id });
  }

  private repair(m: Member, shipId: unknown, target: unknown, kind: 'panel' | 'part') {
    const ship = this.ships.find((s) => s.id === shipId);
    const eye = this.eye(m);
    const list = kind === 'panel' ? ship?.def.panels : ship?.def.parts;
    if (m.info.id === 0 || m.dead || !ship || !eye || !list || !Number.isInteger(target) || !list[target as number]) return;
    // only with a tool that welds in hand
    const f = m.state?.f ?? 0;
    if (weaponById(m.state?.w)?.action.kind !== 'weld' || !(f & StateFlags.Armed)) return;
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
    if (hp !== null) this.toNear(ship, { type: 'ship', ship: ship.id, hp: [[i, hp]], by: m.info.id });
  }

  private respawn(m: Member) {
    if (!this.members.has(m.socket)) return;
    m.hp = MAX_HP;
    m.dead = false;
    m.suitO2 = 1;
    this.broadcast({ type: 'health', id: m.info.id, hp: m.hp });
    this.broadcast({ type: 'respawn', id: m.info.id, spawn: spawnPoint(m.info.variant + Math.floor(Math.random() * 4), this.surfaces) });
  }

  private players() {
    return [...this.members.values()].filter((m) => m.info.id > 0);
  }

  /** The modifiers laid during the game on every body's ground, in order (a newcomer's copy). */
  private dynamicMods(): TerrainMod[] {
    const out: TerrainMod[] = [];
    for (const b of Object.values(BODIES)) {
      const s = this.surfaces(b);
      if (s) out.push(...s.mods.dynamic);
    }
    return out;
  }

  /** Serialized once (binary for the frequent ones, shared/wire.ts), sent to every player. */
  private broadcast(msg: ServerMessage) {
    let data: string | ArrayBuffer | null = null;
    for (const m of this.members.values()) {
      if (m.info.id <= 0 || m.socket.readyState !== m.socket.OPEN) continue;
      data ??= encodeServer(msg) ?? JSON.stringify(msg);
      m.socket.send(data);
    }
  }

  private others(member: Member) {
    return [...this.members.values()].filter((m) => m !== member && m.info.id > 0);
  }

  private snapshots = 0;

  /**
   * Player states. Everyone close (PLAYER_NEAR_M, or in the same ship) at the snapshot rate; the far
   * ones once a second (compass markers). When everybody is near everybody (the usual co-op case)
   * one message, serialized once, goes to all (each client skips its own entry).
   */
  private broadcastSnapshot() {
    this.snapshots++;
    const list: Member[] = [];
    for (const m of this.members.values()) if (m.info.id > 0 && m.state) list.push(m);
    let n = 0;
    for (const m of this.members.values()) if (m.info.id > 0) n++;
    if (n < 2 || !list.length) return;
    const t = now();
    const farDue = this.snapshots % FAR_SNAPSHOT_EVERY === 0;
    const close = (a: Member, b: Member) => a.state?.fr !== undefined && a.state.fr === b.state?.fr ? true : !!a.at && !!b.at && dist(a.at, b.at) < PLAYER_NEAR_M;
    let allNear = farDue;
    if (!allNear) {
      allNear = true;
      for (let i = 0; i < list.length && allNear; i++) for (let j = i + 1; j < list.length; j++) if (!close(list[i], list[j])) allNear = false;
    }
    if (allNear) {
      this.broadcast({ type: 'snapshot', t, states: list.map((m) => ({ id: m.info.id, t: m.stateTime, s: m.state! })) });
      return;
    }
    for (const target of this.members.values()) {
      if (target.info.id <= 0) continue;
      const states: Array<{ id: number; t: number; s: PlayerState }> = [];
      for (const m of list) if (m !== target && close(target, m)) states.push({ id: m.info.id, t: m.stateTime, s: m.state! });
      if (states.length) send(target.socket, { type: 'snapshot', t, states });
    }
  }
}

function send(socket: WebSocket, msg: ServerMessage) {
  if (socket.readyState === socket.OPEN) socket.send(encodeServer(msg) ?? JSON.stringify(msg));
}

function sanitizeName(name: unknown, variant: number) {
  const clean = typeof name === 'string' ? name.replace(/[^\p{L}\p{N} _.-]/gu, '').trim().slice(0, 20) : '';
  return clean || `Astronauta ${variant + 1}`;
}

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
    Number.isInteger(s.f) &&
    (s.fr === undefined || Number.isInteger(s.fr)) &&
    (s.w === undefined || (typeof s.w === 'string' && s.w.length <= 32))
  );
}

function isPose(p: PoseWire): boolean {
  const q = p.q as unknown;
  return isFiniteVec(p.p) && isFiniteVec(p.v) && isFiniteVec(p.w) && Array.isArray(q) && q.length === 4 && q.every((n) => typeof n === 'number' && Number.isFinite(n)) && typeof p.landed === 'boolean' && typeof p.pad === 'boolean';
}

/**
 * Interest (network): a player follows a ship's state closely within SHIP_IN_M (and lets it go
 * beyond SHIP_OUT_M), gets its poses at full rate within POSE_NEAR_M (a couple a second further
 * out), other players' states at full rate within PLAYER_NEAR_M (once a second further out); loose
 * objects by their own rule (server/objects.ts). Worked out every INTEREST_EVERY systems ticks.
 */
const SHIP_IN_M = 1500;
const SHIP_OUT_M = 1800;
const POSE_NEAR_M = 3000;
const PLAYER_NEAR_M = 2000;
const INTEREST_EVERY = 10;
const FAR_POSE_EVERY = 30;
/** A parked ship with no player this close (m) and slower than REST_V (m/s) is put to sleep (sim LOD). */
const REST_NEAR_M = 500;
const REST_V = 0.5;
const FAR_FLIGHT_EVERY = 15;
const FAR_SNAPSHOT_EVERY = 20;

/** Beyond this distance (m) from every player, a parked ship's systems run at a quarter of the rate. */
const FAR_M = 400;
const FAR_EVERY = 4;

/** An object has at least one own key (no array built, unlike Object.keys). */
function hasKeys(o: object) {
  for (const k in o) if (Object.prototype.hasOwnProperty.call(o, k)) return true;
  return false;
}
