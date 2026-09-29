// Power: fission reactors, battery banks, auxiliary power units and solar arrays, in size classes.
// Reactor thermal numbers scale with the rating (core and coolant heat capacity, core→coolant
// conductance), so every size runs at the same temperatures when its radiators scale with it.

import type { ComponentDef } from './types.js';

const reactor = (size: ComponentDef['size'], name: string, kw: number, half: ComponentDef['half'], maxHp: number, mass: number, maker: string, startS: number): ComponentDef => {
  const k = kw / 60;
  return {
    id: `reactor.fission.${size}`,
    type: 'reactor',
    size,
    maker,
    name,
    desc: `Reactor de fisión de ${kw} kW eléctricos, refrigerado por líquido.`,
    half,
    maxHp,
    soft: 0.7,
    mass,
    p: { kw, heatKw: kw * (100 / 60), startS, startKw: Math.max(2, Math.round(8 * k)), coreCap: 60 * k, coolCap: 150 * k, ua: 1.2 * k, pumpHeat: 1.5 * k },
    look: { rings: size === 'XS' ? 1 : 2 },
  };
};

export const POWER: ComponentDef[] = [
  reactor('XS', 'Micro-reactor RX-20', 20, [0.36, 0.55, 0.36], 90, 1400, 'hokuto', 10),
  reactor('S', 'Reactor RX-35', 35, [0.48, 0.8, 0.48], 120, 2600, 'hokuto', 12),
  { ...reactor('M', 'Reactor RX-1', 60, [0.62, 1.05, 0.62], 150, 4200, 'kestrel', 15), p: { kw: 60, heatKw: 100, startS: 15, startKw: 8, coreCap: 60, coolCap: 150, ua: 1.2, pumpHeat: 1.5 } },
  reactor('L', 'Reactor RX-120', 120, [0.85, 1.3, 0.85], 220, 8200, 'kestrel', 20),

  { id: 'battery.XS', type: 'battery', size: 'XS', maker: 'lunagen', name: 'Batería auxiliar', desc: 'Banco de baterías pequeño para cargas de reserva.', half: [0.16, 0.3, 0.2], maxHp: 40, decomp: 0.15, mass: 60, p: { kwh: 3, maxOut: 8, maxIn: 3 } },
  { id: 'battery.S', type: 'battery', size: 'S', maker: 'lunagen', name: 'Batería', desc: 'Banco de baterías de lanzadera.', half: [0.2, 0.45, 0.3], maxHp: 60, decomp: 0.15, mass: 120, p: { kwh: 6, maxOut: 15, maxIn: 5 } },
  { id: 'battery.M', type: 'battery', size: 'M', maker: 'lunagen', name: 'Baterías principales', desc: 'Banco de baterías de carguero: arranque y respaldo de la red.', half: [0.22, 0.62, 0.42], maxHp: 80, decomp: 0.15, mass: 240, p: { kwh: 12, maxOut: 30, maxIn: 10 } },
  { id: 'battery.L', type: 'battery', size: 'L', maker: 'lunagen', name: 'Baterías de alta capacidad', desc: 'Banco de baterías para naves grandes o sin reactor.', half: [0.4, 0.7, 0.5], maxHp: 120, decomp: 0.15, mass: 560, p: { kwh: 30, maxOut: 60, maxIn: 25 } },

  { id: 'apu.S', type: 'apu', size: 'S', maker: 'kestrel', name: 'APU', desc: 'Turbina auxiliar compacta que quema propelente.', half: [0.16, 0.16, 0.45], maxHp: 55, decomp: 0.12, mass: 90, p: { kw: 8, startS: 6, spoolKw: 2 } },
  { id: 'apu.M', type: 'apu', size: 'M', maker: 'kestrel', name: 'APU', desc: 'Turbina auxiliar que quema propelente: respaldo y arranque.', half: [0.2, 0.2, 0.62], maxHp: 70, decomp: 0.12, mass: 160, p: { kw: 15, startS: 8, spoolKw: 3 } },

  // solar wings: `kw` deployed and facing the sun, `stowedFrac` folded on the hull
  { id: 'solar.wing.S', type: 'solar', size: 'S', maker: 'vanta', name: 'Ala solar', desc: 'Panel solar plegable con seguimiento del sol.', half: [0.9, 0.05, 0.7], maxHp: 30, soft: 1.4, mass: 45, p: { kw: 3.5, stowedFrac: 0.12 }, look: { cells: 6 } },
  { id: 'solar.wing.M', type: 'solar', size: 'M', maker: 'vanta', name: 'Ala solar', desc: 'Panel solar plegable de gran superficie.', half: [1.6, 0.06, 1.0], maxHp: 40, soft: 1.4, mass: 90, p: { kw: 8, stowedFrac: 0.1 }, look: { cells: 8 } },
];
