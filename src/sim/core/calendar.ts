// Game time (docs/MUNDO.md §2): seconds since the world began, a float64 (exact to well under a
// millisecond for thousands of years). The calendar only names it; a world can have its own
// (longer days, other years) and everything that talks about days or years asks it.

export const MINUTE = 60;
export const HOUR = 3600;

export interface CalendarDef {
  /** Seconds in a day. */
  readonly day: number;
  /** Days in a year. */
  readonly year: number;
  /** Number of the first year. */
  readonly epoch: number;
}

export const STANDARD_CALENDAR: CalendarDef = { day: 24 * HOUR, year: 365, epoch: 1 };

export interface DateParts {
  year: number;
  /** 1-based, within the year. */
  day: number;
  hour: number;
  minute: number;
  second: number;
}

const pad = (n: number) => String(n).padStart(2, '0');

export class Calendar {
  /** Seconds in a day. */
  readonly day: number;
  /** Seconds in a year. */
  readonly year: number;

  constructor(readonly def: CalendarDef = STANDARD_CALENDAR) {
    this.day = def.day;
    this.year = def.day * def.year;
  }

  parts(t: number): DateParts {
    const y = Math.floor(t / this.year);
    let r = t - y * this.year;
    const d = Math.floor(r / this.day);
    r -= d * this.day;
    const h = Math.floor(r / HOUR);
    r -= h * HOUR;
    const m = Math.floor(r / MINUTE);
    return { year: y + this.def.epoch, day: d + 1, hour: h, minute: m, second: r - m * MINUTE };
  }

  /** Game time of a date (missing parts: the start of the larger one). */
  at(p: Partial<DateParts>): number {
    return (
      ((p.year ?? this.def.epoch) - this.def.epoch) * this.year +
      ((p.day ?? 1) - 1) * this.day +
      (p.hour ?? 0) * HOUR +
      (p.minute ?? 0) * MINUTE +
      (p.second ?? 0)
    );
  }

  format(t: number): string {
    const p = this.parts(t);
    return `año ${p.year}, día ${p.day}, ${pad(p.hour)}:${pad(p.minute)}`;
  }
}

/**
 * Game time against a real clock (ms): `scale` game seconds per real second. Re-anchored on every
 * change of scale, so time never jumps; 0 pauses it.
 */
export class WorldClock {
  private real0 = 0;
  private game0 = 0;

  constructor(public scale = 1) {}

  anchor(real: number, game: number): void {
    this.real0 = real;
    this.game0 = game;
  }

  game(real: number): number {
    return this.game0 + ((real - this.real0) / 1000) * this.scale;
  }

  setScale(scale: number, real: number): void {
    this.anchor(real, this.game(real));
    this.scale = scale;
  }
}
