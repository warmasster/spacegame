// Propulsion: propellant tanks (cylindrical, conformal, spherical), main engines and RCS
// thrusters. Engines push along −z of the ship (toward the nose) from the end of their box;
// thrust and flow are what the flight model uses (shared/ship/flight.ts).

import type { ComponentDef } from './types.js';

export const PROPULSION: ComponentDef[] = [
  { id: 'tank.cyl.S', type: 'tank', size: 'S', maker: 'selk', name: 'Depósito cilíndrico TC-300', desc: 'Depósito de propelente cilíndrico de 300 kg.', half: [0.22, 0.22, 1.5], maxHp: 90, mass: 60, p: { cap: 300 } },
  { id: 'tank.cyl.M', type: 'tank', size: 'M', maker: 'selk', name: 'Depósito cilíndrico TC-1100', desc: 'Depósito de propelente de góndola, 1100 kg.', half: [0.74, 0.74, 1.95], maxHp: 110, mass: 180, p: { cap: 1100 } },
  { id: 'tank.cyl.L', type: 'tank', size: 'L', maker: 'selk', name: 'Depósito cilíndrico TC-2600', desc: 'Depósito de propelente principal, 2600 kg.', half: [1.0, 1.0, 2.4], maxHp: 150, mass: 350, p: { cap: 2600 } },
  { id: 'tank.conformal.S', type: 'tank', size: 'S', maker: 'selk', name: 'Depósito conformado TF-520', desc: 'Depósito plano bajo la cubierta, 520 kg.', half: [0.8, 0.15, 1.3], maxHp: 100, mass: 110, p: { cap: 520 }, look: { form: 1 } },
  { id: 'tank.sphere.S', type: 'tank', size: 'S', maker: 'selk', name: 'Depósito esférico TE-250', desc: 'Depósito esférico de 250 kg (presión alta, poco volumen muerto).', half: [0.5, 0.5, 0.5], maxHp: 80, mass: 55, p: { cap: 250 }, look: { form: 2 } },

  { id: 'engine.main.S', type: 'engine', size: 'S', maker: 'kestrel', name: 'Motor principal', desc: 'Motor principal de lanzadera: 30 kN.', half: [0.45, 0.45, 0.9], maxHp: 90, mass: 420, p: { thrustN: 30000, flowKg: 1.0, spoolS: 2.5, idle: 0.05, kw: 1.5 } },
  { id: 'engine.main.M', type: 'engine', size: 'M', maker: 'kestrel', name: 'Motor principal', desc: 'Motor principal de carguero: 60 kN.', half: [0.74, 0.74, 1.6], maxHp: 120, mass: 900, p: { thrustN: 60000, flowKg: 2.0, spoolS: 3, idle: 0.05, kw: 3 } },
  { id: 'engine.main.L', type: 'engine', size: 'L', maker: 'kestrel', name: 'Motor principal', desc: 'Motor principal pesado: 140 kN.', half: [1.0, 1.0, 2.1], maxHp: 170, mass: 2000, p: { thrustN: 140000, flowKg: 4.4, spoolS: 4, idle: 0.05, kw: 5 } },

  { id: 'rcs.S', type: 'rcs', size: 'S', maker: 'kestrel', name: 'Bloque RCS', desc: 'Bloque de cuatro toberas de maniobra, 400 N cada una.', half: [0.1, 0.1, 0.14], maxHp: 30, mass: 12, p: { thrustN: 400, flowKg: 0.15 } },
  { id: 'rcs.M', type: 'rcs', size: 'M', maker: 'kestrel', name: 'Bloque RCS', desc: 'Bloque de cuatro toberas de maniobra, 900 N cada una.', half: [0.12, 0.12, 0.18], maxHp: 40, mass: 20, p: { thrustN: 900, flowKg: 0.3 } },
];
