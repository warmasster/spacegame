// Heat rejection: coolant pumps and radiators (fixed panels or wings that deploy). A radiator's
// `stowed` / `deployed` are its radiating areas (m², both faces); a fixed panel has them equal.

import type { ComponentDef } from './types.js';

export const THERMAL: ComponentDef[] = [
  { id: 'coolpump.S', type: 'coolpump', size: 'S', maker: 'hokuto', name: 'Bomba de refrigerante', desc: 'Bomba del circuito primario para reactores XS y S.', half: [0.18, 0.24, 0.22], maxHp: 45, decomp: 0.3, mass: 35, p: { kw: 1.2 }, sounds: { run: 'mach.pump' } },
  { id: 'coolpump.M', type: 'coolpump', size: 'M', maker: 'hokuto', name: 'Bomba de refrigerante', desc: 'Bomba del circuito primario para reactores M y L.', half: [0.24, 0.32, 0.3], maxHp: 60, decomp: 0.3, mass: 70, p: { kw: 2.5 }, sounds: { run: 'mach.pump' } },

  { id: 'radiator.panel.S', type: 'radiator', size: 'S', maker: 'vanta', name: 'Radiador fijo', desc: 'Panel radiador fijo de 4 m²: siempre radiando, sin mecanismo.', half: [0.5, 0.05, 1.0], maxHp: 40, soft: 1.3, mass: 40, p: { stowed: 4, deployed: 4 }, look: { fins: 5 } },
  { id: 'radiator.panel.M', type: 'radiator', size: 'M', maker: 'vanta', name: 'Radiador fijo', desc: 'Panel radiador fijo de 8 m².', half: [0.7, 0.05, 1.45], maxHp: 50, soft: 1.3, mass: 75, p: { stowed: 8, deployed: 8 }, look: { fins: 7 } },
  { id: 'radiator.wing.M', type: 'radiator', size: 'M', maker: 'vanta', name: 'Radiador desplegable', desc: 'Ala radiadora: 4 m² plegada, 14 m² desplegada (hidráulica).', half: [0.42, 0.06, 2.25], maxHp: 50, soft: 1.3, mass: 180, p: { stowed: 4, deployed: 14 }, look: { fins: 7, wing: 1 } },
  { id: 'radiator.wing.L', type: 'radiator', size: 'L', maker: 'vanta', name: 'Radiador desplegable', desc: 'Ala radiadora grande: 8 m² plegada, 28 m² desplegada.', half: [0.6, 0.07, 3.0], maxHp: 70, soft: 1.3, mass: 340, p: { stowed: 8, deployed: 28 }, look: { fins: 9, wing: 1 } },
];
