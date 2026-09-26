// Every ship the game knows, by id. A new ship is a data file in this folder plus one line here
// (and a spawn in shared/constants.ts if it should be parked in the world). See docs/SHIPS.md.

import type { ShipDef } from '../def.js';
import { PEREGRINA } from './peregrina.js';
import { HAULER } from './selene.js';

export { HAULER, PEREGRINA };

export const SHIP_DEFS: Record<string, ShipDef> = Object.fromEntries([HAULER, PEREGRINA].map((d) => [d.id, d]));
