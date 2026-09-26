// Wire protocol shared by client and server.
// JSON over WebSocket for now; the message shapes are kept flat so a binary
// encoding can replace JSON later without touching game code.

export const PROTOCOL_VERSION = 1;

/** Suit stripe colour / crew role. 0 = commander (red stripes), 1 = crew (plain), ... */
export type SuitVariant = number;

export const StateFlags = {
  Grounded: 1 << 0,
  Running: 1 << 1,
  Crouching: 1 << 2,
  Lamps: 1 << 3,
} as const;

/** Kinematic state of an astronaut, sent by its owner ~20 times per second. */
export interface PlayerState {
  /** World position of the feet (metres). */
  p: [number, number, number];
  /** Velocity (m/s). */
  v: [number, number, number];
  /** Body heading (radians, around +Y). */
  yaw: number;
  /** Look pitch (radians, + = up). */
  pitch: number;
  /** Bitset of StateFlags. */
  f: number;
}

export interface PlayerInfo {
  id: number;
  name: string;
  variant: SuitVariant;
}

export type ClientMessage =
  | { type: 'hello'; version: number; name: string }
  | { type: 'state'; s: PlayerState }
  | { type: 'ping'; t: number };

export type ServerMessage =
  | {
      type: 'welcome';
      id: number;
      variant: SuitVariant;
      players: PlayerInfo[];
      spawn: [number, number, number];
      worldSeed: number;
      serverTime: number;
    }
  | { type: 'reject'; reason: string }
  | { type: 'join'; player: PlayerInfo }
  | { type: 'leave'; id: number }
  /** States relayed by the server, stamped with server receive time (ms). */
  | { type: 'snapshot'; t: number; states: Array<{ id: number; t: number; s: PlayerState }> }
  | { type: 'pong'; t: number; serverTime: number };
