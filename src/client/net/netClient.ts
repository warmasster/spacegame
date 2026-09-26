import {
  PROTOCOL_VERSION,
  type ClientMessage,
  type PlayerInfo,
  type PlayerState,
  type ServerMessage,
  type TerrainEdit,
  type Vec3,
} from '../../shared/protocol';

export type Welcome = Extract<ServerMessage, { type: 'welcome' }>;

export interface NetEvents {
  join(p: PlayerInfo): void;
  leave(id: number): void;
  state(id: number, serverTime: number, s: PlayerState): void;
  disconnect(reason: string): void;
  fire(id: number, o: Vec3, d: Vec3): void;
  explode(id: number, p: Vec3, edit: TerrainEdit): void;
  health(id: number, hp: number, by?: number, dead?: boolean): void;
  respawn(id: number, spawn: Vec3): void;
}

/** WebSocket session: handshake, state upload, snapshot delivery and server-clock estimate. */
export class NetClient {
  private ws: WebSocket | null = null;
  private clockOffset = 0; // serverTime - performance.now()
  private offsetSamples: number[] = [];
  private pingTimer = 0;
  rtt = 0;
  connected = false;

  constructor(private events: NetEvents) {}

  connect(name: string, url = defaultUrl()): Promise<Welcome> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(url);
      this.ws = ws;
      let welcomed = false;
      ws.onopen = () => this.send({ type: 'hello', version: PROTOCOL_VERSION, name });
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
        try {
          msg = JSON.parse(e.data);
        } catch {
          return;
        }
        switch (msg.type) {
          case 'welcome':
            welcomed = true;
            this.connected = true;
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
            for (const st of msg.states) this.events.state(st.id, st.t, st.s);
            break;
          case 'fire':
            this.events.fire(msg.id, msg.o, msg.d);
            break;
          case 'explode':
            this.events.explode(msg.id, msg.p, msg.edit);
            break;
          case 'health':
            this.events.health(msg.id, msg.hp, msg.by, msg.dead);
            break;
          case 'respawn':
            this.events.respawn(msg.id, msg.spawn);
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

  sendFire(o: Vec3, d: Vec3) {
    this.send({ type: 'fire', o, d });
  }

  sendHit(p: Vec3) {
    this.send({ type: 'hit', p });
  }

  private send(msg: ClientMessage) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(msg));
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
