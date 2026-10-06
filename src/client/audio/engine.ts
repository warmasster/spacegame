// The audio engine: one Web Audio context for the whole game (`sfx`). Anything that makes a noise
// asks it by sound id and place — a one-shot (`play`) or a loop it drives every frame (`loop`) —
// and the engine does the rest every frame: how each sound reaches the ear (medium.ts: air,
// structure, ground, suit), from which direction (HRTF panner at the source's real position,
// relative to the listener so the moon's million-metre coordinates don't matter), how muffled, how
// much of the room's reverb, and which voices are worth keeping.
//
//   source → gain → low-pass (the medium) → panner (the direction) → dry bus ─┐
//                                        └→ reverb send ─→ convolver ───────┤→ master → duck → limiter → out
//   helmet sounds (own = 2) skip the panner ────────────────────────────────┘
//
// Nothing plays until the player has clicked (browsers only start audio after a gesture): `unlock`
// on every input. In a frame it allocates nothing but the nodes of a new one-shot.

import { Bank, soundParams, type SoundParams } from './bank';
import { copyPlace, hear, newHeard, newListener, newPlace, type Acoustics, type Heard, type Place, type VacuumMode } from './medium';

/** A continuous sound its owner drives every frame: set `level`, `pitch` and `place`. */
export class Loop {
  level = 0;
  pitch = 1;
  readonly place: Place = newPlace();
  /** @internal */
  voice: Voice | null = null;
  /** @internal: seconds it has been inaudible. */
  quiet = 0;
  /** @internal */
  released = false;

  constructor(
    readonly sound: string,
    public gain = 1,
  ) {}

  /** Done with it: it fades out and goes. */
  release() {
    this.released = true;
    this.level = 0;
  }
}

interface Voice {
  src: AudioBufferSourceNode;
  amp: GainNode;
  lp: BiquadFilterNode;
  pan: PannerNode | null;
  send: GainNode;
  place: Place;
  params: SoundParams;
  /** Loudness of the take and the caller's gain. */
  base: number;
  /** Loudness of the take itself (a `like` sound's). */
  take: number;
  rate: number;
  loop: Loop | null;
  ended: boolean;
  /** Last values sent to the nodes (only real changes are scheduled). */
  g: number;
  c: number;
  w: number;
  r: number;
  /** How loud it was last frame (who to drop when there are too many). */
  loud: number;
}

/** At most this many one-shots and loops sounding at once. */
const MAX_SHOTS = 40;
const MAX_LOOPS = 36;
/** Below this a sound isn't started (and a loop is let go). */
const AUDIBLE = 0.0015;

const NO_ACOUSTICS: Acoustics = { air: () => 0, way: () => 0, toWorld: () => false };

const load = (k: string) => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};
const save = (k: string, v: string) => {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* private mode */
  }
};

export class AudioEngine {
  ctx: AudioContext | null = null;
  private bank: Bank | null = null;
  /** The ears, filled by the game every frame (see Listener). */
  readonly listener = newListener();
  /** Who answers about air (the ships); set by the game. */
  acoustics: Acoustics = NO_ACOUSTICS;
  mode: VacuumMode = load('selene.vacuum') === 'muffled' ? 'muffled' : 'physical';
  volume = Math.max(0, Math.min(1, Number(load('selene.volume') ?? 0.8)));
  private master!: GainNode;
  private duck!: GainNode;
  private dry!: GainNode;
  private helmet!: GainNode;
  private wetIn!: GainNode;
  private shots: Voice[] = [];
  private loops: Loop[] = [];
  private heard: Heard = newHeard();
  private got = { buffer: null as AudioBuffer | null, pitch: 1, gain: 1 };
  private helmetPlace: Place = { ...newPlace(), own: 2 };
  private stunned = 0;

  /** Create the context (call from a user gesture if possible) and start rendering the bank. */
  init() {
    if (this.ctx) return;
    let ctx: AudioContext;
    try {
      ctx = new AudioContext({ latencyHint: 'interactive' });
    } catch (e) {
      console.warn('[audio] no Web Audio', e);
      return;
    }
    this.ctx = ctx;
    const comp = ctx.createDynamicsCompressor();
    comp.threshold.value = -12;
    comp.knee.value = 8;
    comp.ratio.value = 10;
    comp.attack.value = 0.003;
    comp.release.value = 0.25;
    comp.connect(ctx.destination);
    this.duck = ctx.createGain();
    this.duck.connect(comp);
    this.master = ctx.createGain();
    this.master.gain.value = this.volume;
    this.master.connect(this.duck);
    this.dry = ctx.createGain();
    this.dry.connect(this.master);
    this.helmet = ctx.createGain();
    this.helmet.connect(this.master);
    const verb = ctx.createConvolver();
    verb.buffer = roomImpulse(ctx);
    const wet = ctx.createGain();
    wet.gain.value = 0.9;
    this.wetIn = ctx.createGain();
    this.wetIn.connect(verb).connect(wet).connect(this.master);
    const L = ctx.listener;
    if (L.positionX) {
      L.positionX.value = 0;
      L.positionY.value = 0;
      L.positionZ.value = 0;
    }
    this.bank = new Bank(ctx);
    this.bank.warm();
    // a hidden tab stops the game loop: stop the sound with it
    document.addEventListener('visibilitychange', () => {
      if (document.hidden) void ctx.suspend();
      else this.unlock();
    });
    const wake = () => this.unlock();
    window.addEventListener('pointerdown', wake, true);
    window.addEventListener('keydown', wake, true);
  }

  /** Browsers start audio only after a gesture: call on input. */
  unlock() {
    const ctx = this.ctx;
    if (ctx && ctx.state === 'suspended' && !document.hidden) void ctx.resume();
  }

  get running() {
    return this.ctx?.state === 'running';
  }

  setVolume(v: number) {
    this.volume = Math.max(0, Math.min(1, v));
    save('selene.volume', String(this.volume));
    if (this.ctx) this.master.gain.setTargetAtTime(this.volume, this.ctx.currentTime, 0.05);
  }

  setMode(m: VacuumMode) {
    this.mode = m;
    save('selene.vacuum', m);
  }

  /**
   * A one-shot at a place (copied: the caller may reuse it). `gain`, `pitch`: on top of the
   * sound's own. Nothing is made for a sound nobody would hear.
   */
  play(id: string, pl: Place, gain = 1, pitch = 1) {
    const ctx = this.ctx;
    if (!ctx || ctx.state !== 'running' || gain <= 0) return;
    const params = soundParams(id);
    const L = this.listener;
    if (pl.local && !this.acoustics.toWorld(pl.host, pl.local, pl.p)) return;
    if (pl.own !== 2 && Math.hypot(pl.p[0] - L.p[0], pl.p[1] - L.p[1], pl.p[2] - L.p[2]) > params.range) return;
    const h = hear(pl, L, this.acoustics, params.ref, params.body, this.mode, this.heard);
    if (h.gain * gain < AUDIBLE) return;
    const got = this.bank!.get(id, this.got);
    if (!got.buffer) return;
    const loud = h.gain * gain * got.gain;
    if (this.shots.length >= MAX_SHOTS) {
      // drop the quietest if this one is louder
      let q = -1;
      for (let i = 0; i < this.shots.length; i++) if (q < 0 || this.shots[i].loud < this.shots[q].loud) q = i;
      if (q < 0 || this.shots[q].loud >= loud) return;
      const old = this.shots[q];
      this.stop(old, 0.03);
      old.src.onended = () => this.free(old);
      this.shots.splice(q, 1);
    }
    const rate = pitch * got.pitch * (1 + (Math.random() * 2 - 1) * params.jitter);
    const v = this.voice(got.buffer, copyPlace(newPlace(), pl), params, gain * got.gain, rate, null);
    this.drive(v, 1, rate, true, false);
    v.src.start(ctx.currentTime + Math.min(1.5, h.delay));
    v.src.onended = () => {
      v.ended = true;
    };
    this.shots.push(v);
  }

  /** A sound inside the helmet (radio, warnings, the suit's own clicks). */
  ui(id: string, gain = 1, pitch = 1) {
    this.play(id, this.helmetPlace, gain, pitch);
  }

  /** A loop to drive every frame (starts silent: set `level`). */
  loop(id: string, gain = 1): Loop {
    const l = new Loop(id, gain);
    // without audio nobody would ever let it go
    if (this.ctx) this.loops.push(l);
    return l;
  }

  /** A blast close by: everything ducks and the ears ring for a while (k 0..1). */
  stun(k: number) {
    const ctx = this.ctx;
    if (!ctx || k < 0.25 || k <= this.stunned) return;
    this.stunned = k;
    const t = ctx.currentTime;
    const g = this.duck.gain;
    g.cancelScheduledValues(t);
    g.setValueAtTime(g.value, t);
    g.setTargetAtTime(1 - 0.8 * Math.min(1, k), t, 0.015);
    g.setTargetAtTime(1, t + 0.25 + k * 0.8, 0.9 + k);
    this.ui('ear.ring', Math.min(1, k));
  }

  /** Every frame, after the listener and every loop were set. */
  update(dt: number) {
    const ctx = this.ctx;
    if (!ctx) return;
    this.stunned = Math.max(0, this.stunned - dt * 0.4);
    if (ctx.state !== 'running') return;
    const L = this.listener;
    const Ls = ctx.listener;
    if (Ls.forwardX) {
      Ls.forwardX.value = L.fwd[0];
      Ls.forwardY.value = L.fwd[1];
      Ls.forwardZ.value = L.fwd[2];
      Ls.upX.value = L.up[0];
      Ls.upY.value = L.up[1];
      Ls.upZ.value = L.up[2];
    } else Ls.setOrientation(L.fwd[0], L.fwd[1], L.fwd[2], L.up[0], L.up[1], L.up[2]);
    // one-shots: follow the listener (and the ship they are on) until they end
    for (let i = this.shots.length - 1; i >= 0; i--) {
      const v = this.shots[i];
      if (v.ended) {
        this.free(v);
        this.shots.splice(i, 1);
        continue;
      }
      if (v.place.local) this.acoustics.toWorld(v.place.host, v.place.local, v.place.p);
      this.drive(v, 1, v.rate, false, false);
    }
    // loops: the ones worth hearing get a voice, the rest let theirs go
    let active = 0;
    for (const l of this.loops) if (l.voice) active++;
    for (let i = this.loops.length - 1; i >= 0; i--) {
      const l = this.loops[i];
      const lvl = l.released ? 0 : l.level;
      let loud = 0;
      if (lvl > 0.001) {
        const pl = l.place;
        if (pl.local) this.acoustics.toWorld(pl.host, pl.local, pl.p);
        const params = soundParams(l.sound);
        if (pl.own === 2 || Math.hypot(pl.p[0] - L.p[0], pl.p[1] - L.p[1], pl.p[2] - L.p[2]) < params.range) {
          loud = lvl * l.gain * hear(pl, L, this.acoustics, params.ref, params.body, this.mode, this.heard).gain;
        }
      }
      if (loud > AUDIBLE) {
        l.quiet = 0;
        let fresh = false;
        if (!l.voice) {
          if (active >= MAX_LOOPS) {
            if (!this.stealLoop(loud)) continue;
            active--;
          }
          const got = this.bank!.get(l.sound, this.got);
          if (!got.buffer) continue;
          l.voice = this.voice(got.buffer, l.place, soundParams(l.sound), got.gain, got.pitch, l);
          l.voice.src.start(ctx.currentTime, Math.random() * got.buffer.duration);
          active++;
          fresh = true;
        }
        const v = l.voice;
        v.base = l.gain * v.take;
        // `this.heard` still holds how it is heard (just computed above)
        this.drive(v, lvl, l.pitch * v.rate, fresh, true);
      } else if (l.voice) {
        l.quiet += dt;
        const v = l.voice;
        if (v.g > 0) {
          v.amp.gain.setTargetAtTime(0, ctx.currentTime, 0.06);
          v.g = 0;
        }
        if (l.quiet > 0.4) {
          this.stop(v, 0);
          this.free(v);
          l.voice = null;
          active--;
        }
      }
      if (l.released && !l.voice) this.loops.splice(i, 1);
    }
  }

  /** How many voices are sounding (debug overlay). */
  stats() {
    let loops = 0;
    for (const l of this.loops) if (l.voice) loops++;
    return { shots: this.shots.length, loops, sources: this.loops.length };
  }

  /** Let the quietest loop voice go if `loud` is clearly louder. */
  private stealLoop(loud: number) {
    let q: Loop | null = null;
    for (const l of this.loops) if (l.voice && (!q || l.voice.loud < q.voice!.loud)) q = l;
    if (!q || q.voice!.loud * 1.5 >= loud) return false;
    this.stop(q.voice!, 0.05);
    this.free(q.voice!);
    q.voice = null;
    return true;
  }

  private voice(buffer: AudioBuffer, place: Place, params: SoundParams, base: number, rate: number, loop: Loop | null): Voice {
    const ctx = this.ctx!;
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.loop = !!loop;
    src.playbackRate.value = rate;
    const amp = ctx.createGain();
    amp.gain.value = 0;
    const lp = ctx.createBiquadFilter();
    lp.type = 'lowpass';
    lp.Q.value = 0.6;
    lp.frequency.value = 20000;
    src.connect(amp).connect(lp);
    let pan: PannerNode | null = null;
    // inside the helmet: no direction
    if (place.own !== 2) {
      pan = new PannerNode(ctx, { panningModel: 'HRTF', distanceModel: 'linear', refDistance: 1, maxDistance: 1e7, rolloffFactor: 0 });
      lp.connect(pan).connect(this.dry);
    } else lp.connect(this.helmet);
    const send = ctx.createGain();
    send.gain.value = 0;
    (pan ?? lp).connect(send).connect(this.wetIn);
    return { src, amp, lp, pan, send, place, params, base, take: this.got.gain, rate, loop, ended: false, g: -1, c: -1, w: -1, r: rate, loud: 0 };
  }

  /**
   * Set a voice's nodes from how it is heard now. `first`: jump there, don't glide (a loop still
   * fades in). `heard`: `this.heard` already holds it.
   */
  private drive(v: Voice, level: number, rate: number, first: boolean, heard: boolean) {
    const ctx = this.ctx!;
    const t = ctx.currentTime;
    const L = this.listener;
    const pl = v.place;
    const h = heard ? this.heard : hear(pl, L, this.acoustics, v.params.ref, v.params.body, this.mode, this.heard);
    const g = h.gain * level * v.base;
    v.loud = g;
    const tau = v.loop ? 0.05 : 0.03;
    if (first) {
      if (v.loop) v.amp.gain.setTargetAtTime(g, t, 0.05);
      else v.amp.gain.value = g;
      v.lp.frequency.value = h.cutoff;
      v.send.gain.value = h.wet;
      v.src.playbackRate.value = rate;
    } else {
      if (Math.abs(g - v.g) > Math.max(0.0005, v.g * 0.03)) v.amp.gain.setTargetAtTime(g, t, tau);
      if (Math.abs(h.cutoff - v.c) > v.c * 0.04) v.lp.frequency.setTargetAtTime(h.cutoff, t, 0.06);
      if (Math.abs(h.wet - v.w) > 0.01) v.send.gain.setTargetAtTime(h.wet, t, 0.1);
      if (Math.abs(rate - v.r) > v.r * 0.004) v.src.playbackRate.setTargetAtTime(rate, t, 0.08);
    }
    v.g = g;
    v.c = h.cutoff;
    v.w = h.wet;
    v.r = rate;
    const p = v.pan;
    if (p) {
      // relative to the listener (at the origin): precise wherever in the world
      const x = pl.p[0] - L.p[0];
      const y = pl.p[1] - L.p[1];
      const z = pl.p[2] - L.p[2];
      // your own sounds right at the head have no direction worth giving
      const k = pl.own === 1 && x * x + y * y + z * z < 0.25 ? 0 : 1;
      if (p.positionX) {
        p.positionX.value = x * k;
        p.positionY.value = y * k;
        p.positionZ.value = z * k - (1 - k) * 0.3;
      } else p.setPosition(x * k, y * k, z * k - (1 - k) * 0.3);
    }
  }

  private stop(v: Voice, fade: number) {
    const t = this.ctx!.currentTime;
    try {
      if (fade > 0) {
        v.amp.gain.setTargetAtTime(0, t, fade / 3);
        v.src.stop(t + fade);
      } else v.src.stop();
    } catch {
      /* never started or already stopped */
    }
  }

  private free(v: Voice) {
    v.src.onended = null;
    v.src.disconnect();
    v.amp.disconnect();
    v.lp.disconnect();
    v.pan?.disconnect();
    v.send.disconnect();
  }
}

/** The room sound of a ship's cabin: a short, bright metal box (stereo decaying noise). */
function roomImpulse(ctx: BaseAudioContext) {
  const sr = ctx.sampleRate;
  const n = Math.round(sr * 1.1);
  const ir = ctx.createBuffer(2, n, sr);
  for (let ch = 0; ch < 2; ch++) {
    const d = ir.getChannelData(ch);
    let lp = 0;
    for (let i = 0; i < n; i++) {
      const t = i / sr;
      // highs die first (the air and the soft things in the room)
      const k = 0.35 + 0.6 * Math.exp(-t / 0.12);
      lp += (Math.random() * 2 - 1 - lp) * k;
      d[i] = lp * Math.exp(-t / 0.22) * (t < 0.004 ? t / 0.004 : 1);
    }
    // a few early reflections off the walls, 2–9 m away
    for (let r = 0; r < 6; r++) {
      const i = Math.round(sr * (0.006 + Math.random() * 0.024));
      if (i < n) d[i] += (Math.random() < 0.5 ? -1 : 1) * (0.5 - r * 0.06);
    }
  }
  return ir;
}

/** The game's one audio engine. */
export const sfx = new AudioEngine();

/** A helmet place for code that plays sounds of its own (`sfx.ui` is simpler). */
export const HELMET: Readonly<Place> = { ...newPlace(), own: 2 };
