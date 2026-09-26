// Gameplay/world constants shared by client and server.

export const DEFAULT_PORT = 3000;
export const MAX_PLAYERS = 2;

/** Rates (Hz). */
export const CLIENT_SEND_RATE = 20;
export const SERVER_SNAPSHOT_RATE = 20;

/** Remote players are rendered this far in the past (ms) to interpolate smoothly. */
export const INTERPOLATION_DELAY_MS = 120;

export const WORLD_SEED = 1969;

/** Celestial bodies are data, not code: the Moon is just the first entry. */
export interface BodyDef {
  id: string;
  name: string;
  /** Mean radius (m). Used for horizon curvature. */
  radius: number;
  /** Surface gravity (m/s^2). */
  gravity: number;
  /** Surface atmospheric density (kg/m^3). 0 = vacuum. */
  atmosphereDensity: number;
}

export const MOON: BodyDef = {
  id: 'moon',
  name: 'Luna',
  radius: 1_737_400,
  gravity: 1.62,
  atmosphereDensity: 0,
};

/** Suit stripe colours by variant (linear-ish sRGB hex). */
export const SUIT_STRIPES = [0xb3261e, 0xe8e6e1, 0x1f4fa8, 0xd9a21b];
export const SUIT_VARIANT_NAMES = ['Comandante', 'Especialista', 'Piloto', 'Ingeniero'];

/** Ships parked in the world at start: ship-space origin (deck centre) position and heading. */
export const SHIP_SPAWNS = [{ id: 1, def: 'hauler', x: 3.6, z: -23.7, yaw: -0.5 }];

/** Circles kept free of boulders (landing pads). */
export const CLEARINGS = SHIP_SPAWNS.map((s) => ({ x: s.x, z: s.z, r: 14 }));
