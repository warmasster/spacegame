// Equipment that only draws power and can be damaged until its mechanics exist (radar, turret,
// inertial compensator…): the catalog gives it a box, a mass and its draw.

import type { ComponentDef } from './types.js';

export const EQUIPMENT: ComponentDef[] = [
  { id: 'radar.dome.S', type: 'radar', size: 'S', maker: 'vanta', name: 'Radar (cúpula)', desc: 'Radar de cúpula: pasivo 0,5 kW, activo 3 kW.', half: [0.34, 0.14, 0.34], maxHp: 40, mass: 60, p: { kwPassive: 0.5, kwActive: 3 } },
  { id: 'antenna.S', type: 'antenna', size: 'S', maker: 'vanta', name: 'Antena de comunicaciones', desc: 'Antena parabólica orientable.', half: [0.28, 0.3, 0.28], maxHp: 30, mass: 25, p: {} },
  { id: 'turret.M', type: 'turret', size: 'M', maker: 'kestrel', name: 'Torreta de minimisiles', desc: 'Torreta dorsal de dos tubos.', half: [0.45, 0.32, 0.45], maxHp: 80, mass: 380, p: { kw: 1.5 } },
  { id: 'grav.M', type: 'grav', size: 'M', maker: 'vanta', name: 'Compensador inercial', desc: 'Reduce las aceleraciones que siente la tripulación.', half: [0.45, 0.16, 0.45], maxHp: 60, mass: 210, p: { kw: 6 } },
  { id: 'loader.S', type: 'loader', size: 'S', maker: 'kestrel', name: 'Cargador de misiles', desc: 'Cargador automático de la torreta.', half: [0.3, 0.3, 0.12], maxHp: 50, mass: 90, p: {} },
];
