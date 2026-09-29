// Every kind of ship system, in the order their modules run within each phase. A new mechanic is
// a module file (see api.ts) plus one line here; a ship picks the ones it has through its data
// (parts, circuits, compartments…) or explicitly with `ShipDef.systems`.

import { airlockSystem } from './airlock.js';
import type { SystemFactory } from './api.js';
import { apuSystem } from './apu.js';
import { autopilotSystem } from './autopilot.js';
import { decompSystem } from './decomp.js';
import { enginesSystem } from './engines.js';
import { gravSystem } from './grav.js';
import { helmSystem } from './helm.js';
import { lifeSystem } from './life.js';
import { moversSystem } from './movers.js';
import { powerSystem } from './power.js';
import { propellantSystem } from './propellant.js';
import { reactorSystem } from './reactor.js';
import { solarSystem } from './solar.js';
import { storesSystem } from './stores.js';

/**
 * Machines that only draw power and can be damaged (their loads are `LoadDef`s) until someone
 * writes their mechanics: the radar, the turret… Moving a type from here to its own module is how
 * it comes alive.
 */
const equipmentSystem: SystemFactory = {
  id: 'equipment',
  parts: ['radar', 'antenna', 'turret', 'loader'],
  make: () => [],
};

/**
 * Order matters only inside a phase: the airlock sequencer holds the pressure control before life
 * support solves, the helm writes engine commands after the engines declared them.
 */
export const SYSTEM_FACTORIES: SystemFactory[] = [
  powerSystem,
  moversSystem,
  airlockSystem,
  propellantSystem,
  lifeSystem,
  decompSystem,
  reactorSystem,
  solarSystem,
  apuSystem,
  enginesSystem,
  helmSystem,
  autopilotSystem,
  gravSystem,
  storesSystem,
  equipmentSystem,
];
