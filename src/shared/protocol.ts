// Wire protocol shared by client and server.
// JSON over WebSocket for now; the message shapes are kept flat so a binary
// encoding can replace JSON later without touching game code.

import type { ShipSnapshot } from './ship/sim.js';
import type { TerrainMod } from './space/terrainMods/types.js';

export type { TerrainMod } from './space/terrainMods/types.js';

export const PROTOCOL_VERSION = 13;

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
  /** The tool in hand is at work (the welder's trigger held). */
  Welding: 1 << 8,
  Seated: 1 << 9,
} as const;

/**
 * Kinematic state of an astronaut, sent by its owner ~20 times per second. Everything is in its
 * reference frame `fr`: the moon (absent / 0) or a ship (its id: ship space, so crew aboard a
 * moving ship stay put relative to the deck whatever the network does to the ship's pose).
 */
export interface PlayerState {
  /** Feet position (metres) in the frame. */
  p: [number, number, number];
  /** Velocity (m/s) relative to the frame. */
  v: [number, number, number];
  /** Body heading (radians, around the frame's +Y). */
  yaw: number;
  /** Look pitch (radians, + = up). */
  pitch: number;
  /** Bitset of StateFlags. */
  f: number;
  /** Reference frame: ship id, or absent on the moon. */
  fr?: number;
  /** The tool in hand or drawn next (weapon catalog id). */
  w?: string;
}

export type Quat = [number, number, number, number];

/** A ship's flight state on the wire (world): pose, ground contact. */
export interface PoseWire {
  p: Vec3;
  q: Quat;
  v: Vec3;
  w: Vec3;
  landed: boolean;
  pad: boolean;
}

/** A loose crate: its frame (0 = moon, else ship id), pose in that frame, and who simulates it. */
export interface CrateWire {
  id: number;
  fr: number;
  p: Vec3;
  q: Quat;
  v: Vec3;
  w: Vec3;
  /** Player simulating it (0 = nobody: it lies still where it is). */
  owner: number;
}

/**
 * What a loose object is (it keeps it wherever it goes): its kind in the object catalog
 * (shared/items/objects.ts; absent: a crate), its size, mass and paint.
 */
export interface CrateSpec {
  kind?: string;
  half: Vec3;
  mass: number;
  paint: 'orange' | 'grey';
}

export interface PlayerInfo {
  id: number;
  name: string;
  variant: SuitVariant;
}

export type ClientMessage =
  | { type: 'hello'; version: number; name: string; seed?: number }
  | { type: 'state'; s: PlayerState }
  | { type: 'ping'; t: number }
  /**
   * A shot of weapon `w` (catalog id; it fires its projectile): origin, direction, `v` the
   * launcher's velocity (carried by the projectile). In the world, or with `fr` (a ship's id) all
   * three in that ship's space, `v` relative to it: fired aboard, it flies with the ship wherever
   * each client has it.
   */
  | { type: 'fire'; w: string; o: Vec3; d: Vec3; v?: Vec3; fr?: number }
  /** Shooter-reported impact of its projectile of kind `k` (world, or with `fr` in that ship's space). */
  | { type: 'hit'; k: string; p: Vec3; fr?: number }
  /** Operate a ship control (index into the ship definition's controls); dir = wheel step on knobs. */
  | { type: 'interact'; ship: number; ctl: number; dir?: number }
  /** Repair tool on a panel; sent ~10 times per second while held. */
  | { type: 'repair'; ship: number; panel: number }
  /** Repair tool on a machine (part index). */
  | { type: 'repairPart'; ship: number; part: number }
  /** Take (on) or leave the helm: the pilot's client flies the ship while it holds it. */
  | { type: 'pilot'; ship: number; on: boolean }
  /**
   * The pilot's flight (~30 Hz): pose at server time `t` (the pilot's estimate), ground contact,
   * height above the ground and thruster outputs (order of the ship's thruster list).
   */
  | ({ type: 'flight'; ship: number; t: number; agl: number; out: number[] } & PoseWire)
  /** Ask to simulate a crate (pick it up, push it, a blast near it). */
  | { type: 'crateTake'; id: number }
  /** The owner's crate state; `rest` = it stopped, back to nobody's. */
  | { type: 'crate'; c: Omit<CrateWire, 'owner'>; rest?: boolean };

export type Vec3 = [number, number, number];

export type ServerMessage =
  | {
      type: 'welcome';
      id: number;
      variant: SuitVariant;
      players: PlayerInfo[];
      /** Where this player starts (world, on the ground). */
      spawn: [number, number, number];
      worldSeed: number;
      serverTime: number;
      /**
       * The terrain modifiers laid during the game (the dynamic layer of every body's surface, oldest
       * first): the static ones (sites) every machine rebuilds from the seed.
       */
      mods: TerrainMod[];
      health: Array<{ id: number; hp: number }>;
      ships: ShipSnapshot[];
      /** Who flies each ship (player id; ships not listed are flown by the server). */
      pilots: Array<[number, number]>;
      crates: Array<CrateWire & CrateSpec>;
    }
  | { type: 'reject'; reason: string }
  | { type: 'join'; player: PlayerInfo }
  | { type: 'leave'; id: number }
  /** States relayed by the server, stamped with server receive time (ms). */
  | { type: 'snapshot'; t: number; states: Array<{ id: number; t: number; s: PlayerState }> }
  | { type: 'pong'; t: number; serverTime: number }
  | { type: 'fire'; id: number; w: string; o: Vec3; d: Vec3; v?: Vec3; fr?: number }
  /**
   * An impact: a projectile of kind `k` (absent: a blast of the ship's own, a tank going up). `mod`
   * is the crater when it went off on the ground (added to its body's surface, in order). `fr` +
   * `l`: it went off in or against that ship, at `l` in its space (where each client draws it: `p`
   * is the server's world point, the ship may be elsewhere on a client).
   */
  | { type: 'explode'; id: number; p: Vec3; mod?: TerrainMod; fr?: number; l?: Vec3; k?: string }
  /** Ship state change: switches and/or panel integrity [index, hp]; `by` = who caused it. */
  | { type: 'ship'; ship: number; sw?: Record<string, number>; hp?: Array<[number, number]>; by?: number }
  /** A control request the server refused (the client normally predicts this itself). */
  | { type: 'shipDenied'; ship: number; ctl: number; reason: string }
  /** Continuous ship state diffs: flat [varIndex, value, …] (see shared/ship/state.ts). */
  | { type: 'shipSt'; ship: number; d: number[] }
  /** A ship's flight pose at server time `t` (~30 Hz while it moves, 1 Hz asleep). */
  | ({ type: 'shipPose'; ship: number; t: number } & PoseWire)
  /** A ship's whole state again: it just came into this player's interest (switches, panels, state table). */
  | { type: 'shipSync'; snap: ShipSnapshot }
  /** Who flies a ship now (0 = the server). */
  | { type: 'pilot'; ship: number; id: number }
  /** A crate moved (relayed from its owner) or changed hands. */
  | { type: 'crate'; c: CrateWire; rest?: boolean }
  /** Ship system message for the crew's helmet display (SCRAM, breaker tripped…). */
  | { type: 'say'; ship: number; text: string }
  /** Own suit: oxygen reserve 0..1 and whether it is breathing cabin air. */
  | { type: 'vitals'; o2: number; cabin: boolean }
  /** Health change; `by` = attacker id when damaged by someone. */
  | { type: 'health'; id: number; hp: number; by?: number; dead?: boolean }
  /** `spawn`: world, on the ground. */
  | { type: 'respawn'; id: number; spawn: Vec3 };
