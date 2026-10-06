// The world's systems, in registration order (docs/MUNDO.md §8): each one a SimModule with its
// components, its events, its requests and how it seeds a new world. Economy, factions, cohorts…
// go here; the core and the thread that runs it don't change. Modules may use the game's pure data
// (src/shared); the core (src/sim/core, persist, host, kit) never does.

import type { SimModule } from '../core/world.js';
import { here } from './here.js';
import { knowledge } from './knowledge.js';
import { objects } from './objects.js';
import { people } from './people.js';
import { players } from './players.js';

export const WORLD_MODULES: readonly SimModule[] = [players, objects, people, knowledge, here];
