// The standard catalog: every family registered once. Ship files import `part`, `prop` and
// `component` from here (that import is what fills the registry).

import { EQUIPMENT } from './equipment.js';
import { FURNITURE_LIST } from './furniture.js';
import { LIFE } from './life.js';
import { POWER } from './power.js';
import { PROPULSION } from './propulsion.js';
import { THERMAL } from './thermal.js';
import { registerComponents, registerFurniture } from './types.js';

/** Families in catalog order (the catalog tool and the tests walk them). */
export const FAMILIES = [
  { id: 'power', label: 'Energía', list: POWER },
  { id: 'thermal', label: 'Térmico', list: THERMAL },
  { id: 'propulsion', label: 'Propulsión', list: PROPULSION },
  { id: 'life', label: 'Soporte vital y víveres', list: LIFE },
  { id: 'equipment', label: 'Equipo', list: EQUIPMENT },
];

for (const f of FAMILIES) registerComponents(f.list);
registerFurniture(FURNITURE_LIST);

export { PROFILES, profilePoint } from './furniture.js';
export { MAKERS, maker } from './makers.js';
export { allComponents, allFurniture, component, componentSheet, defineComponent, furniture, part, prop, type ComponentDef, type FurnitureDef, type Placement } from './types.js';
