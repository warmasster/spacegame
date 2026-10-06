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
  /** What its ground is made of: how it sounds underfoot and carries a blast (client/audio/surfaces.ts). Default 'regolith'. */
  ground?: string;
  /** Height (m) over which its atmosphere thins by e (default 8000). */
  scaleHeight?: number;
  /** Typical surface wind (m/s): what the outside sounds like where there is air (client/audio/acoustics.ts). */
  wind?: number;
}

export const MOON: BodyDef = {
  id: 'moon',
  name: 'Luna',
  radius: 1_737_400,
  gravity: 1.62,
  atmosphereDensity: 0,
  ground: 'regolith',
};

/** Suit stripe colours by variant (linear-ish sRGB hex). */
export const SUIT_STRIPES = [0xb3261e, 0xe8e6e1, 0x1f4fa8, 0xd9a21b];
export const SUIT_VARIANT_NAMES = ['Comandante', 'Especialista', 'Piloto', 'Ingeniero'];

/** Sun over the landing site: azimuth and elevation (degrees). Lighting, sky and solar arrays. */
export const SUN = { az: 98, el: 16 };

/**
 * Ships parked in the world at start: the site they stand on (space/sites.ts) and where, in that
 * site's frame (x east, z south, m: the deck centre), their heading and the radius of their pad.
 */
export const SHIP_SPAWNS: Array<{ id: number; def: string; site: string; x: number; z: number; yaw: number; r?: number }> = [
  { id: 1, def: 'hauler', site: 'base', x: 3.6, z: -23.7, yaw: -0.5 },
  // passenger shuttle, in sight of the spawn, hatch side toward the landing site
  { id: 2, def: 'peregrina', site: 'base', x: -10, z: -8, yaw: 0.5 },
  // heavy two-deck transport, past the hauler; `r`: its pad is bigger (the default is 14 m)
  { id: 3, def: 'albatros', site: 'base', x: 30, z: -8, yaw: -1.1, r: 22 },
];
