// A connected player as the server's parts see it (loose objects, NPCs, the world bridge): who it
// is, where it is, and a way to reach it — never its socket or the room's bookkeeping.

import type { Watcher } from '../shared/net/interest.js';
import type { ServerMessage } from '../shared/protocol.js';
import { encodeServer } from '../shared/wire.js';

export interface Peer extends Watcher {
  /** Player id (> 0 once it has joined). */
  readonly id: number;
  readonly dead: boolean;
  /** Raw send (already encoded). */
  send(data: string | ArrayBuffer): void;
}

/** One message to several peers, encoded once. */
export function deliver(to: Iterable<Peer>, msg: ServerMessage): void {
  let data: string | ArrayBuffer | null = null;
  for (const p of to) {
    data ??= encodeServer(msg) ?? JSON.stringify(msg);
    p.send(data);
  }
}
