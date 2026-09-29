import {
  PROTOCOL_VERSION,
  type ClientMessage,
  type CrateWire,
  type PlayerInfo,
  type PlayerState,
  type PoseWire,
  type ServerMessage,
  type TerrainMod,
  type Vec3,
} from '../../shared/protocol';
import { decodeServer, encodeClient } from '../../shared/wire';
import type { ShipSnapshot } from '../../shared/ship/sim';

export type Welcome = Extract<ServerMessage, { type: 'welcome' }>;

export interface NetEvents {
  join(p: PlayerInfo): void;
  leave(id: number): void;
  state(id: number, serverTime: number, s: PlayerState): void;
  disconnect(reason: string): void;
  /** A shot of weapon `w`; `fr`: fired aboard that ship (`o`, `d`, `v` in its space). */
  fire(id: number, w: string, o: Vec3, d: Vec3, v?: Vec3, fr?: number): void;
  /** `aboard`: it happened in or against that ship, at `l` in its space; `k`: the projectile kind (none: a ship's own blast). */
  explode(id: number, p: Vec3, mod?: TerrainMod, aboard?: { fr: number; l: Vec3 }, k?: string): void;
  health(id: number, hp: number, by?: number, dead?: boolean): void;
  respawn(id: number, spawn: Vec3): void;
  ship(ship: number, sw: Record<string, number> | undefined, hp: Array<[number, number]> | undefined, by?: number): void;
  shipDenied(ship: number, ctl: number, reason: string): void;
  shipState(ship: number, d: number[]): void;
  /** A ship's whole state (it came back into interest). */
  shipSync(snap: ShipSnapshot): void;
  shipPose(ship: number, t: number, pose: PoseWire): void;
  pilot(ship: number, id: number): void;
  crate(c: CrateWire, rest: boolean): void;
  say(ship: number, text: string): void;
  vitals(o2: number, cabin: boolean): void;
}

/** WebSocket session: handshake, state upload, snapshot delivery and server-clock estimate. */
export class NetClient {
  private ws: WebSocket | null = null;
  private clockOffset = 0; // serverTime - performance.now()
  private offsetSamples: number[] = [];
  private pingTimer = 0;
  rtt = 0;
  connected = false;
  /** Our player id (the server sends everyone the same snapshot, ours included). */
  private selfId = -1;

  constructor(private events: NetEvents) {}

  connect(name: string, url?: string, seed?: number): Promise<Welcome> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(url ?? defaultUrl());
      // the frequent messages come in binary (shared/wire.ts)
      ws.binaryType = 'arraybuffer';
      this.ws = ws;
      let welcomed = false;
      ws.onopen = () => this.send({ type: 'hello', version: PROTOCOL_VERSION, name, seed });
      ws.onerror = () => {
        if (!welcomed) reject(new Error('No se pudo conectar con el servidor.'));
      };
      ws.onclose = () => {
        this.connected = false;
        clearInterval(this.pingTimer);
        if (welcomed) this.events.disconnect('Conexión perdida con el servidor.');
        else reject(new Error('El servidor cerró la conexión.'));
      };
      ws.onmessage = (e) => {
        let msg: ServerMessage;
        if (typeof e.data !== 'string') {
          const m = decodeServer(new DataView(e.data as ArrayBuffer));
          if (!m) return;
          msg = m;
        } else {
          try {
            msg = JSON.parse(e.data);
          } catch {
            return;
          }
        }
        switch (msg.type) {
          case 'welcome':
            welcomed = true;
            this.connected = true;
            this.selfId = msg.id;
            this.clockOffset = msg.serverTime - performance.now();
            this.pingTimer = window.setInterval(() => this.send({ type: 'ping', t: performance.now() }), 2000);
            this.send({ type: 'ping', t: performance.now() });
            resolve(msg);
            break;
          case 'reject':
            reject(new Error(msg.reason));
            break;
          case 'join':
            this.events.join(msg.player);
            break;
          case 'leave':
            this.events.leave(msg.id);
            break;
          case 'snapshot':
            for (const st of msg.states) if (st.id !== this.selfId) this.events.state(st.id, st.t, st.s);
            break;
          case 'fire':
            this.events.fire(msg.id, msg.w, msg.o, msg.d, msg.v, msg.fr);
            break;
          case 'explode':
            this.events.explode(msg.id, msg.p, msg.mod, msg.fr !== undefined && msg.l ? { fr: msg.fr, l: msg.l } : undefined, msg.k);
            break;
          case 'health':
            this.events.health(msg.id, msg.hp, msg.by, msg.dead);
            break;
          case 'respawn':
            this.events.respawn(msg.id, msg.spawn);
            break;
          case 'ship':
            this.events.ship(msg.ship, msg.sw, msg.hp, msg.by);
            break;
          case 'shipDenied':
            this.events.shipDenied(msg.ship, msg.ctl, msg.reason);
            break;
          case 'shipSt':
            this.events.shipState(msg.ship, msg.d);
            break;
          case 'shipSync':
            this.events.shipSync(msg.snap);
            break;
          case 'shipPose':
            this.events.shipPose(msg.ship, msg.t, msg);
            break;
          case 'pilot':
            this.events.pilot(msg.ship, msg.id);
            break;
          case 'crate':
            this.events.crate(msg.c, msg.rest === true);
            break;
          case 'say':
            this.events.say(msg.ship, msg.text);
            break;
          case 'vitals':
            this.events.vitals(msg.o2, msg.cabin);
            break;
          case 'pong': {
            const now = performance.now();
            this.rtt = now - msg.t;
            const offset = msg.serverTime + this.rtt / 2 - now;
            this.offsetSamples.push(offset);
            if (this.offsetSamples.length > 8) this.offsetSamples.shift();
            // use the median to reject jittery samples
            const sorted = [...this.offsetSamples].sort((a, b) => a - b);
            this.clockOffset = sorted[Math.floor(sorted.length / 2)];
            break;
          }
        }
      };
    });
  }

  /** Estimated current server time (ms). */
  serverNow() {
    return performance.now() + this.clockOffset;
  }

  sendState(s: PlayerState) {
    this.send({ type: 'state', s });
  }

  sendFire(w: string, o: Vec3, d: Vec3, v?: Vec3, fr?: number) {
    this.send({ type: 'fire', w, o, d, v, fr });
  }

  sendHit(k: string, p: Vec3, fr?: number) {
    this.send({ type: 'hit', k, p, fr });
  }

  sendInteract(ship: number, ctl: number, dir = 0) {
    this.send({ type: 'interact', ship, ctl, dir: dir || undefined });
  }

  sendRepair(ship: number, panel: number) {
    this.send({ type: 'repair', ship, panel });
  }

  sendRepairPart(ship: number, part: number) {
    this.send({ type: 'repairPart', ship, part });
  }

  /** Take (on) or leave the helm. */
  sendPilot(ship: number, on: boolean) {
    this.send({ type: 'pilot', ship, on });
  }

  /** Our flight of a ship we pilot (pose at server time `t`, height above ground, thruster outputs). */
  sendFlight(ship: number, t: number, agl: number, out: number[], pose: PoseWire) {
    this.send({ type: 'flight', ship, t, agl, out, p: pose.p, q: pose.q, v: pose.v, w: pose.w, landed: pose.landed, pad: pose.pad });
  }

  sendCrateTake(id: number) {
    this.send({ type: 'crateTake', id });
  }

  sendCrate(c: Omit<CrateWire, 'owner'>, rest: boolean) {
    this.send({ type: 'crate', c, rest: rest || undefined });
  }

  private send(msg: ClientMessage) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(encodeClient(msg) ?? JSON.stringify(msg));
  }

  close() {
    clearInterval(this.pingTimer);
    this.ws?.close();
  }
}

function defaultUrl() {
  const q = new URLSearchParams(location.search).get('server');
  if (q) return q;
  return `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
}
