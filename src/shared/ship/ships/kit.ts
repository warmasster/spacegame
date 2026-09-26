// Helpers every ship file uses: circuits with their breaker / priority switches, the breaker and
// priority boards, loads read from the machines' own catalog numbers, door push-button panels,
// and help texts written from a machine's parameters (so a bigger reactor explains itself with
// its own numbers).

import { boardFrame, type ConsoleSpec, type ControlSpec, type DoorDef, type LoadDef, type PartSpec, type SubsystemDef } from '../def.js';
import type { V2, V3 } from '../geom.js';

/** A circuit as a ship writes it: its priority tier (0 ALTA, 1 NORMAL, 2 BAJA) and short label. */
export type CircuitSpec = Omit<SubsystemDef, 'breaker' | 'priority'> & { pri: number; short: string };

/** Circuits → subsystems (breaker `brk.<id>`, priority `pri.<id>`) and their default switch positions. */
export function circuits(list: CircuitSpec[]) {
  const subsystems: SubsystemDef[] = list.map(({ pri: _p, short: _s, ...c }) => ({ ...c, breaker: `brk.${c.id}`, priority: `pri.${c.id}` }));
  const defaults: Record<string, number> = {};
  for (const c of list) {
    defaults[`brk.${c.id}`] = 1;
    defaults[`pri.${c.id}`] = c.pri;
  }
  const short = Object.fromEntries(list.map((c) => [c.id, c.short]));
  return { subsystems, defaults, short };
}

/** One breaker per circuit on a grid (cols per row from `origin`, pitch dx, dy). */
export function breakerControls(subsystems: SubsystemDef[], short: Record<string, string>, g: { cols: number; origin: V2; dx: number; dy: number }): ControlSpec[] {
  return subsystems.map((c, i) => ({
    key: c.breaker,
    kind: 'breaker',
    label: short[c.id],
    name: `Disyuntor · ${c.label.toLowerCase()} (${c.rating} kW)`,
    help: `Corta o da corriente al circuito ${c.label}. ${c.desc ?? ''} Salta solo si pide más de ${c.rating} kW durante 2 s (una máquina muy dañada que hace cortocircuito, demasiadas cargas): quita la causa antes de subirlo.`,
    states: ['ABIERTO', 'CERRADO'],
    at: [g.origin[0] + (i % g.cols) * g.dx, g.origin[1] - Math.floor(i / g.cols) * g.dy],
  }));
}

/** One priority selector per circuit on a grid. */
export function priorityControls(subsystems: SubsystemDef[], short: Record<string, string>, g: { cols: number; origin: V2; dx: number; dy: number }): ControlSpec[] {
  return subsystems.map((c, i) => ({
    key: c.priority,
    kind: 'rotary',
    label: short[c.id],
    name: `Prioridad · ${c.label}`,
    help: `Decide cuándo se corta ${c.label.toLowerCase()} si falta energía: primero se quedan sin nada los circuitos en BAJA, luego los NORMAL; los ALTA son los últimos. ${c.desc ?? ''}`,
    states: ['ALTA', 'NORMAL', 'BAJA'],
    at: [g.origin[0] + (i % g.cols) * g.dx, g.origin[1] - Math.floor(i / g.cols) * g.dy],
    action: 'cycle',
  }));
}

/**
 * The switched load of a machine, from its catalog numbers: `kw` while its switch is on (radars:
 * off / passive / active positions). `key` defaults to the part id.
 */
export function loadOf(parts: PartSpec[], id: string, key = id): LoadDef {
  const p = parts.find((x) => x.id === id);
  if (!p) throw new Error(`carga: no hay máquina "${id}"`);
  if (!p.circuit) throw new Error(`carga: la máquina "${id}" no está conectada a ningún circuito`);
  const kw = p.p?.kwActive !== undefined ? [0, p.p.kwPassive ?? 0, p.p.kwActive] : p.p?.kw;
  if (kw === undefined) throw new Error(`carga: la máquina "${id}" no dice cuánto consume (p.kw)`);
  return { key, circuit: p.circuit, kw, part: id };
}

/** Small "open" push-button panels on both faces of a door's wall. */
export function doorButtons(d: DoorDef, o: { id: string; name: string; side: number; y?: number; t: number; requires: string; states?: string[] }): ConsoleSpec[] {
  const along: V3 = [d.n[2], 0, -d.n[0]];
  const out: ConsoleSpec[] = [];
  for (const face of [-1, 1] as const) {
    const c: V3 = [d.c[0] + along[0] * o.side * face + d.n[0] * face * (o.t / 2 + 0.01), o.y ?? 1.25, d.c[2] + along[2] * o.side * face + d.n[2] * face * (o.t / 2 + 0.01)];
    out.push({
      id: `${o.id}.${face < 0 ? 'a' : 'b'}`,
      title: 'PUERTA',
      frame: boardFrame(c, [d.n[0] * face, 0, d.n[2] * face]),
      w: 0.2,
      h: 0.26,
      depth: 0.03,
      controls: [{ key: d.key, kind: 'button', label: 'ABRIR', name: o.name, states: o.states ?? ['CERRADA', 'ABIERTA'], at: [0, -0.02], requires: o.requires }],
    });
  }
  return out;
}

const num = (v: number, d = 0) => v.toFixed(d).replace('.', ',');

/** Help lines written from a machine's own numbers. */
export const HELP = {
  battery: (p: PartSpec) => `Conecta el banco de baterías (${num(p.p!.kwh)} kWh) a la red. Cubre lo que los generadores no dan (hasta ${num(p.p!.maxOut)} kW) y se carga con lo que sobra. Hace falta para arrancar el reactor con todo parado.`,
  reactorLever: (p: PartSpec) => `Palanca del reactor. Arriba inicia el arranque (unos ${num(p.p!.startS)} s): exige la bomba de refrigerante en marcha y energía de arranque (batería). Abajo baja la potencia hasta pararlo.`,
  reactorSet: (p: PartSpec) => `Potencia pedida al reactor, de 0 a 110 % de ${num(p.p!.kw)} kW. Por encima de 100 % es sobrecarga: calienta más deprisa. Un reactor dañado no llega a lo pedido.`,
  scram: () => 'Parada de emergencia: mete las barras y el reactor se para al instante. Bajo tapa. También salta solo a 750 °C. Para volver a arrancar hace falta REARME.',
  reactorReset: () => 'Quita el SCRAM cuando el núcleo ha bajado de 300 °C; la palanca vuelve a PARADO y ya se puede arrancar de nuevo.',
  coolpump: (p: PartSpec) => `Bomba del circuito de refrigeración (${num(p.p!.kw, 1)} kW): lleva el calor del núcleo a los radiadores. Sin ella el reactor se calienta hasta el SCRAM y no se deja arrancar.`,
  o2gen: (p: PartSpec, where: string) => `Generador de oxígeno por electrólisis (${num(p.p!.kw, 1)} kW) ${where}. Repone el O₂ que respira la tripulación; los ventiladores lo reparten al resto de la nave.`,
  scrub: (p: PartSpec, where: string) => `Depurador de CO₂ (${num(p.p!.kw, 1)} kW) ${where}. Retira el CO₂ que exhala la tripulación; necesita los ventiladores para limpiar el aire de los demás compartimentos.`,
  gasValve: (p: PartSpec) => `Válvula de ${p.name.toLowerCase()} (${num(p.p!.cap)} kg). Cerrada no hay represurización con ${p.p!.gas === 1 ? 'nitrógeno (el gas de relleno del aire)' : 'oxígeno ni O₂ para los trajes acoplados a los asientos'}.`,
  engineArm: (name: string) => `Arma el ${name.toLowerCase()}: da tensión al encendido y a su control. Sin armar no arranca. Desarmarlo apaga el motor y, con la alimentación cerrada, lo deja seguro si está dañado.`,
  engineStart: (name: string, circuit: string) => `Arranca el ${name.toLowerCase()} (unos 3 s hasta régimen). Necesita el motor armado, el circuito ${circuit} con energía y propelente llegando por su válvula. Un motor muy dañado puede explotar al encenderse.`,
};
