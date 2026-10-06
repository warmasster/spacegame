// What someone notices (docs/MUNDO.md §11–12): sight and hearing as physics would allow them.
//
//   sight    within range, inside the visor's cone (anything very close is noticed anyway), and in
//            the same space: a hull stands between who is aboard a ship and who is outside
//   hearing  through what carries sound: the same breathable air (a pressurised ship: full range),
//            the same structure without air (the hull rings: a fraction), the ground both stand on
//            (vibration through the regolith: a small fraction); vacuum carries nothing
//
// Pure: the server asks it who witnessed what (server/npcs.ts), a test asks it anything.

export type V3 = [number, number, number];

/** Where someone or something is, as the senses care. */
export interface Sense {
  /** The host it is in (0: outside everything). */
  host: number;
  /** World position. */
  p: V3;
  /** In breathable air (a pressurised compartment). */
  air: boolean;
}

/** Someone who looks: where the visor faces (world, unit). */
export interface Eye extends Sense {
  fwd: V3;
}

export const SENSES = {
  /** How far a figure can be made out (m). */
  sight: 80,
  /** Half-angle of the visor's view (rad). */
  fov: (65 * Math.PI) / 180,
  /** Anything this close is noticed whatever the direction (m). */
  near: 2.5,
  /** Share of a sound's range carried by: the same air, a structure without air, the ground. */
  air: 1,
  structure: 0.4,
  ground: 0.12,
};

/** How far each sound carries in air (m). */
export const LOUDNESS = {
  step: 8,
  handle: 6,
  voice: 15,
  shot: 150,
  impact: 120,
  explosion: 2000,
};

const dist = (a: V3, b: V3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

export function sees(eye: Eye, t: Sense, k = SENSES): boolean {
  if (eye.host !== t.host) return false;
  const d = dist(eye.p, t.p);
  if (d > k.sight) return false;
  if (d < k.near) return true;
  const c = ((t.p[0] - eye.p[0]) * eye.fwd[0] + (t.p[1] - eye.p[1]) * eye.fwd[1] + (t.p[2] - eye.p[2]) * eye.fwd[2]) / d;
  return c >= Math.cos(k.fov);
}

/** Does a sound of range `loud` (m, in air) made at `src` reach `ear`? */
export function hears(ear: Sense, src: Sense, loud: number, k = SENSES): boolean {
  const d = dist(ear.p, src.p);
  if (ear.host !== 0 && ear.host === src.host) return d < loud * (ear.air && src.air ? k.air : k.structure);
  if (ear.host === 0 && src.host === 0) return d < loud * k.ground;
  return false;
}

/** Who noticed something (seen, if it can be seen; or heard): the ids of `who`, in order. */
export function witnesses(what: Sense & { loud: number; visible: boolean }, who: ReadonlyArray<{ id: number; eye: Eye }>, k = SENSES): number[] {
  const out: number[] = [];
  for (const w of who) if ((what.visible && sees(w.eye, what, k)) || hears(w.eye, what, what.loud, k)) out.push(w.id);
  return out;
}
