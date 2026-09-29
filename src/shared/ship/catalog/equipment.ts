// Equipment: the inertial compensator (modules/grav.ts), and machines that only draw power and can
// be damaged until their mechanics exist (radar, turret, loader): a box, a mass and the draw.

import type { ComponentDef } from './types.js';

export const EQUIPMENT: ComponentDef[] = [
  { id: 'radar.dome.S', type: 'radar', size: 'S', maker: 'vanta', name: 'Radar (cúpula)', desc: 'Radar de cúpula: pasivo 0,5 kW, activo 3 kW.', half: [0.34, 0.14, 0.34], maxHp: 40, mass: 60, p: { kwPassive: 0.5, kwActive: 3 }, sounds: { run: 'mach.radar' } },
  { id: 'antenna.S', type: 'antenna', size: 'S', maker: 'vanta', name: 'Antena de comunicaciones', desc: 'Antena parabólica orientable.', half: [0.28, 0.3, 0.28], maxHp: 30, mass: 25, p: {} },
  { id: 'turret.M', type: 'turret', size: 'M', maker: 'kestrel', name: 'Torreta de minimisiles', desc: 'Torreta dorsal de dos tubos.', half: [0.45, 0.32, 0.45], maxHp: 80, mass: 380, p: { kw: 1.5 }, sounds: { run: 'mach.motor' } },
  { id: 'grav.S', type: 'grav', size: 'S', maker: 'vanta', name: 'Compensador inercial', desc: 'Mantiene el suelo como «abajo» a bordo aunque la nave se incline o acelere.', half: [0.32, 0.12, 0.32], maxHp: 45, decomp: 0.35, mass: 90, p: { kw: 2.5 }, lamp: 'GRAVEDAD' },
  { id: 'grav.M', type: 'grav', size: 'M', maker: 'vanta', name: 'Compensador inercial', desc: 'Mantiene el suelo como «abajo» a bordo aunque la nave se incline o acelere.', half: [0.45, 0.16, 0.45], maxHp: 60, decomp: 0.35, mass: 210, p: { kw: 6 }, lamp: 'GRAVEDAD' },
  { id: 'loader.S', type: 'loader', size: 'S', maker: 'kestrel', name: 'Cargador de misiles', desc: 'Cargador automático de la torreta.', half: [0.3, 0.3, 0.12], maxHp: 50, decomp: 0.2, mass: 90, p: {}, sounds: { run: 'mach.motor' } },
];
