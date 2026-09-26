// Life support and stores: O₂ generators, CO₂ scrubbers, gas bottles (O₂ = gas 0, N₂ = gas 1),
// potable water tanks, water recyclers and food lockers. `rate` scales a generator's / scrubber's
// output against the reference M unit.

import type { ComponentDef } from './types.js';

export const LIFE: ComponentDef[] = [
  { id: 'o2gen.S', type: 'o2gen', size: 'S', maker: 'hokuto', name: 'Generador de O₂', desc: 'Electrólisis compacta: oxígeno para 5 personas.', half: [0.3, 0.38, 0.28], maxHp: 50, mass: 80, p: { rate: 0.06, kw: 2 } },
  { id: 'o2gen.M', type: 'o2gen', size: 'M', maker: 'hokuto', name: 'Generador de O₂ (electrólisis)', desc: 'Electrólisis: oxígeno para 10 personas.', half: [0.55, 0.5, 0.5], maxHp: 70, mass: 160, p: { rate: 0.125, kw: 4 } },
  { id: 'scrubber.S', type: 'scrubber', size: 'S', maker: 'hokuto', name: 'Depurador de CO₂', desc: 'Lecho de amina compacto.', half: [0.3, 0.34, 0.28], maxHp: 50, mass: 70, p: { rate: 0.6, kw: 0.8 } },
  { id: 'scrubber.M', type: 'scrubber', size: 'M', maker: 'hokuto', name: 'Depurador de CO₂', desc: 'Lecho de amina regenerable.', half: [0.55, 0.46, 0.5], maxHp: 70, mass: 140, p: { rate: 1, kw: 1.5 } },

  { id: 'gas.o2.S', type: 'gas', size: 'S', maker: 'lunagen', name: 'Botella de O₂', desc: 'Botella de oxígeno de 30 kg.', half: [0.12, 0.55, 0.12], maxHp: 40, soft: 1.2, mass: 30, p: { gas: 0, cap: 30 } },
  { id: 'gas.o2.M', type: 'gas', size: 'M', maker: 'lunagen', name: 'Botella de O₂', desc: 'Botella de oxígeno de 80 kg.', half: [0.17, 0.8, 0.17], maxHp: 50, soft: 1.2, mass: 70, p: { gas: 0, cap: 80 } },
  { id: 'gas.n2.S', type: 'gas', size: 'S', maker: 'lunagen', name: 'Botellas de N₂', desc: 'Dos botellas de nitrógeno, 70 kg.', half: [0.12, 0.55, 0.26], maxHp: 45, mass: 60, p: { gas: 1, cap: 70 } },
  { id: 'gas.n2.M', type: 'gas', size: 'M', maker: 'lunagen', name: 'Botellas de N₂', desc: 'Tres botellas de nitrógeno, 200 kg.', half: [0.17, 0.8, 0.42], maxHp: 60, mass: 170, p: { gas: 1, cap: 200 } },

  { id: 'water.S', type: 'water', size: 'S', maker: 'selk', name: 'Depósito de agua potable', desc: 'Depósito de agua de 150 kg.', half: [0.3, 0.42, 0.28], maxHp: 60, mass: 40, p: { cap: 150 } },
  { id: 'water.M', type: 'water', size: 'M', maker: 'selk', name: 'Depósito de agua potable', desc: 'Depósito de agua de 400 kg.', half: [0.45, 0.6, 0.4], maxHp: 80, mass: 90, p: { cap: 400 } },
  { id: 'recycler.S', type: 'recycler', size: 'S', maker: 'hokuto', name: 'Reciclador de agua', desc: 'Destilación por compresión de vapor: recupera el 90 % del agua usada.', half: [0.3, 0.4, 0.28], maxHp: 50, mass: 75, p: { kw: 0.8, eff: 0.9 } },
  { id: 'pantry.S', type: 'pantry', size: 'S', maker: 'selk', name: 'Despensa', desc: 'Armario de raciones: 45 kg de comida.', half: [0.3, 0.5, 0.25], maxHp: 40, mass: 35, p: { cap: 45 } },
];
