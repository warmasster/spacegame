/**
 * A refusal has to stay the same for a moment before it counts. A button that just did its job
 * (start, reset, scram) flips into "you can't do that again" on the next tick; without this
 * settle the helmet would scold you for the thing you just asked for.
 */
export class ReasonHold {
  private held = new Map<string, { reason: string; since: number }>();
  private pinned = new Set<string>();

  constructor(private readonly settle: number) {}

  /** Show this one immediately (the authority refused and the local mirror hadn't noticed yet). */
  pin(id: string) {
    this.pinned.add(id);
  }

  /**
   * The reason to act on. Null while it is still new, or when there is nothing to refuse.
   * A different reason starts the wait over.
   */
  live(id: string, reason: string | null, now: number): string | null {
    if (!reason) {
      this.held.delete(id);
      this.pinned.delete(id);
      return null;
    }
    let h = this.held.get(id);
    if (!h || h.reason !== reason) {
      h = { reason, since: now };
      this.held.set(id, h);
    }
    if (this.pinned.has(id) || now - h.since >= this.settle) return h.reason;
    return null;
  }
}
