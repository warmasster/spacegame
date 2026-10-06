// Which frame a loose body belongs to (a crate, a spare part, anything of the object catalog; the
// same rule for whatever loose thing is added later). The facts come from each side's physics; the
// decision is here, once:
//
//   - **Into a host**: inside one of its compartments, at once. Resting on its outside (a ramp, a
//     roof), only when it really rests there: slower than `captureSpeed` against the host's surface
//     under it, and not having just changed frame (`dwell`). A crate flying past a hull, or just
//     thrown off it, is not captured by grazing it.
//   - **Out of a host**: out of every compartment and nothing of the host holds it up — at once,
//     so it leaves with the host's velocity at that instant and is never dragged through a turn.
//     On the ground of the body, the ground has it.
//
// The hand-over itself carries position, velocity and the drawn history (frame.ts `carry`,
// track.ts `PoseTrack.carry`): changing frame is never visible.

export interface Presence {
  /** Inside one of the host's compartments. */
  inside: boolean;
  /** Something of the host's own structure holds it up (a short ray along "down" meets it). */
  supported: boolean;
  /** Speed relative to the host's surface where it is (m/s). */
  relSpeed: number;
  /** On (or in) the ground of the body. */
  grounded: boolean;
}

export const MEMBERSHIP = {
  /** Resting on a host's outside means slower than this against it (m/s). */
  captureSpeed: 1.5,
  /** A body that just changed frame stays this long (s) before resting on a hull can take it back. */
  dwell: 0.25,
};

/** In the world near a host: does the host take it now? `age`: s since its last change of frame. */
export function joinsHost(p: Presence, age: number): boolean {
  if (p.inside) return true;
  return p.supported && !p.grounded && p.relSpeed < MEMBERSHIP.captureSpeed && age >= MEMBERSHIP.dwell;
}

/** In a host: does it leave for the world now? */
export function leavesHost(p: Presence): boolean {
  if (p.inside) return false;
  return p.grounded || !p.supported;
}
