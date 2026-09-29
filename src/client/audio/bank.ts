// The sound bank: every sound the game can make, by id. A sound is data — a recipe that
// synthesises it (dsp.ts), how loud it is at its source, how big the source is and how far it
// can reach — registered with `defineSound` from the files in ./sounds/. Cues name sounds by id
// (a machine's module, a surface, a control kind), so a new sound is one entry and nothing else
// changes. An id nobody defined plays nothing and warns once.
//
// Rendering is lazy and cheap: `warm` synthesises everything in small slices after start-up; a
// sound asked for before its turn is made on the spot.

import { Rng, type Synth } from './dsp';

export interface SoundSpec {
  /** Synthesise one take (mono). Loops: made a little longer, the seam is done by the recipe (dsp.loopify). */
  make?: (s: Synth) => Float32Array;
  /** Or: another sound's takes, played at another pitch (and loudness). */
  like?: string;
  pitch?: number;
  /** A seamless loop (continuous sources) rather than a one-shot. */
  loop?: boolean;
  /** Takes to pick from at random (footsteps, clicks), each with its own seed. */
  variants?: number;
  /** Loudness at the source (relative across the bank; 1 = a loud machine at arm's length). */
  gain?: number;
  /** Size of the source (m): full loudness within it, then −6 dB per doubling of distance. */
  ref?: number;
  /** Beyond this distance (m) it isn't played. */
  range?: number;
  /** Random pitch spread of one-shots (±fraction). */
  jitter?: number;
}

const specs = new Map<string, SoundSpec>();

export function defineSound(id: string, spec: SoundSpec) {
  if (specs.has(id)) throw new Error(`sound "${id}" defined twice`);
  specs.set(id, spec);
}

/** The spec of a sound (following `like`), or null for an id nobody defined. */
export function soundSpec(id: string): SoundSpec | null {
  return specs.get(id) ?? null;
}

export function soundIds(): string[] {
  return [...specs.keys()];
}

/** Placement numbers of a sound (the first in its `like` chain that gives each), looked up once. */
export interface SoundParams {
  ref: number;
  range: number;
  jitter: number;
  loop: boolean;
}

const params = new Map<string, SoundParams>();

export function soundParams(id: string): SoundParams {
  let p = params.get(id);
  if (p) return p;
  let spec = specs.get(id);
  let ref: number | undefined, range: number | undefined, jitter: number | undefined, loop: boolean | undefined;
  for (let hops = 0; spec && hops < 5; hops++) {
    ref ??= spec.ref;
    range ??= spec.range;
    jitter ??= spec.jitter;
    loop ??= spec.loop;
    spec = spec.like ? specs.get(spec.like) : undefined;
  }
  p = { ref: ref ?? 1, range: range ?? 60, jitter: jitter ?? (loop ? 0 : 0.04), loop: !!loop };
  params.set(id, p);
  return p;
}

const warned = new Set<string>();

/** Rendered takes of every sound, made on demand. */
export class Bank {
  private takes = new Map<string, AudioBuffer[]>();
  private queue: string[] = [];

  constructor(private ctx: BaseAudioContext) {}

  /**
   * One take of a sound (a random one when there are several), rendering it now if it isn't yet,
   * with the pitch and gain its `like` chain adds. Null: unknown id.
   */
  get(id: string, out: { buffer: AudioBuffer | null; pitch: number; gain: number }, pick = Math.random()) {
    out.buffer = null;
    out.pitch = 1;
    out.gain = 1;
    let spec = specs.get(id);
    let key = id;
    for (let hops = 0; spec?.like && hops < 4; hops++) {
      out.pitch *= spec.pitch ?? 1;
      out.gain *= spec.gain ?? 1;
      key = spec.like;
      spec = specs.get(key);
    }
    if (!spec?.make) {
      if (!warned.has(id)) {
        warned.add(id);
        console.warn(`[audio] no sound "${id}" in the bank (client/audio/sounds)`);
      }
      return out;
    }
    out.gain *= spec.gain ?? 1;
    const list = this.render(key, spec);
    out.buffer = list[Math.min(list.length - 1, Math.floor(pick * list.length))];
    return out;
  }

  private render(id: string, spec: SoundSpec): AudioBuffer[] {
    let list = this.takes.get(id);
    if (list) return list;
    list = [];
    const sr = this.ctx.sampleRate;
    const n = Math.max(1, spec.variants ?? 1);
    for (let v = 0; v < n; v++) {
      const s: Synth = { sr, rng: new Rng(hash(id) + v * 7919), variant: v };
      let data: Float32Array;
      try {
        data = spec.make!(s);
      } catch (e) {
        console.error(`[audio] sound "${id}" failed to render`, e);
        data = new Float32Array(1);
      }
      const ab = this.ctx.createBuffer(1, Math.max(1, data.length), sr);
      ab.copyToChannel(data as Float32Array<ArrayBuffer>, 0);
      list.push(ab);
    }
    this.takes.set(id, list);
    return list;
  }

  /** Render everything, a few sounds per slice, without holding up a frame. */
  warm() {
    this.queue = [...specs.keys()].filter((id) => specs.get(id)!.make && !this.takes.has(id));
    const slice = () => {
      const t0 = performance.now();
      while (this.queue.length && performance.now() - t0 < 6) {
        const id = this.queue.shift()!;
        const spec = specs.get(id)!;
        this.render(id, spec);
      }
      if (this.queue.length) setTimeout(slice, 16);
    };
    setTimeout(slice, 0);
  }
}

function hash(s: string) {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}
