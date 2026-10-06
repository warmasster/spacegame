// Names for the people the world makes (drawn from a key: the same person, the same name).

import { draw } from '../core/rng.js';

const FIRST = [
  'Ada', 'Aitana', 'Alba', 'Amara', 'Ana', 'Bruno', 'Carmen', 'Darío', 'Diego', 'Elena', 'Emma', 'Enzo',
  'Eva', 'Gael', 'Hugo', 'Inés', 'Iker', 'Irene', 'Jara', 'Joel', 'Julia', 'Kai', 'Lara', 'Leo',
  'Lucía', 'Luna', 'Marco', 'Marta', 'Mateo', 'Mía', 'Nadia', 'Nico', 'Noa', 'Olga', 'Omar', 'Pau',
  'Rocío', 'Rubén', 'Sara', 'Teo', 'Valeria', 'Vera', 'Yago', 'Zoe',
];

const LAST = [
  'Aguirre', 'Alonso', 'Arias', 'Bravo', 'Cano', 'Castro', 'Cruz', 'Delgado', 'Díaz', 'Duarte', 'Esteban',
  'Ferrer', 'Flores', 'Gallego', 'Garrido', 'Gil', 'Herrera', 'Ibáñez', 'Iglesias', 'León', 'Lozano',
  'Marín', 'Medina', 'Molina', 'Montero', 'Morales', 'Navarro', 'Núñez', 'Ortega', 'Pastor', 'Peña',
  'Prieto', 'Ramos', 'Reyes', 'Rivas', 'Román', 'Rubio', 'Santos', 'Serrano', 'Soler', 'Torres', 'Vargas',
  'Vega', 'Vidal',
];

/** A name from a key (a person's seed). */
export function nameOf(key: number): string {
  return `${FIRST[draw(key, 0) % FIRST.length]} ${LAST[draw(key, 1) % LAST.length]}`;
}
