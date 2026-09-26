// Wire protocol shared by client and server.
// JSON over WebSocket for now; the message shapes are kept flat so a binary
// encoding can replace JSON later without touching game code.

import type { ShipSnapshot } from './ship/sim.js';

export const PROTOCOL_VERSION = 3;

/** Suit stripe colour / crew role. 0 = commander (red stripes), 1 = crew (plain), ... */
export type SuitVariant = number;

export const StateFlags = {
  Grounded: 1 << 0,
  Running: 1 << 1,
  Crouching: 1 << 2,
  Lamps: 1 << 3,
  Jetpack: 1 << 4,
  Dead: 1 << 5,
  Armed: 1 << 6,
  /** Tool in hand is the welder (else the launcher). */
  Welder: 1 << 7,
  /** Welder trigger held. */
  Welding: 1 << 8,
  Seated: 1 << 9,
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
  | { type: 'ping'; t: number }
  /** Rocket launched (origin, direction). */
  | { type: 'fire'; o: Vec3; d: Vec3 }
  /** Shooter-reported impact point of its rocket. */
  | { type: 'hit'; p: Vec3 }
  /** Operate a ship control (index into the ship definition's controls). */
  | { type: 'interact'; ship: number; ctl: number }
  /** Repair tool on a panel; sent ~10 times per second while held. */
  | { type: 'repair'; ship: number; panel: number };

export type Vec3 = [number, number, number];

/** A permanent terrain modification (crater). Server-ordered, replayed on join. */
export interface TerrainEdit {
  x: number;
  z: number;
  /** Radius (m). */
  r: number;
  /** Depth multiplier (1 = regular crater). */
  d: number;
}

export type ServerMessage =
  | {
      type: 'welcome';
      id: number;
      variant: SuitVariant;
      players: PlayerInfo[];
      spawn: [number, number, number];
      worldSeed: number;
      serverTime: number;
      edits: TerrainEdit[];
      health: Array<{ id: number; hp: number }>;
      ships: ShipSnapshot[];
    }
  | { type: 'reject'; reason: string }
  | { type: 'join'; player: PlayerInfo }
  | { type: 'leave'; id: number }
  /** States relayed by the server, stamped with server receive time (ms). */
  | { type: 'snapshot'; t: number; states: Array<{ id: number; t: number; s: PlayerState }> }
  | { type: 'pong'; t: number; serverTime: number }
  | { type: 'fire'; id: number; o: Vec3; d: Vec3 }
  /** Explosion; `edit` is the crater when it went off near the ground. */
  | { type: 'explode'; id: number; p: Vec3; edit?: TerrainEdit }
  /** Ship state change: switches and/or panel integrity [index, hp]; `by` = who caused it. */
  | { type: 'ship'; ship: number; sw?: Record<string, number>; hp?: Array<[number, number]>; by?: number }
  /** A control request the server refused (the client normally predicts this itself). */
  | { type: 'shipDenied'; ship: number; ctl: number; reason: string }
  /** Health change; `by` = attacker id when damaged by someone. */
  | { type: 'health'; id: number; hp: number; by?: number; dead?: boolean }
  | { type: 'respawn'; id: number; spawn: Vec3 };
