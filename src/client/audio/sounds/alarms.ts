// Alarms, chimes and the radio: the ship's speakers (master alarm, airlock, autopilot) and the
// helmet's comms. Speakers are small: everything is band-limited like one.

import { defineSound } from '../bank';
import { add, buf, edges, filter, modes, noise, normalize, shape, tone } from '../dsp';
import type { Buf, Synth } from '../dsp';

/** Through a small cabin speaker. */
function speaker(s: Synth, b: Buf) {
  filter(s, b, 'hp', 350, 0.7);
  filter(s, b, 'peak', 2000, 1, 4);
  filter(s, b, 'lp', 5000, 0.8);
  return b;
}

/** A struck bell tone (fundamental, octave, twelfth, decaying). */
function bell(s: Synth, b: Buf, f: number, at: number, amp = 1, tau = 0.6) {
  modes(s, b, [[f, tau, amp], [f * 2.01, tau * 0.6, amp * 0.45], [f * 3.02, tau * 0.4, amp * 0.25], [f * 4.2, tau * 0.25, amp * 0.1]], at);
}

// master warning (red): a rising whoop, again and again
defineSound('alarm.warning', {
  loop: true,
  ref: 2,
  range: 70,
  gain: 0.45,
  make: (s) => {
    const L = 1;
    const b = new Float32Array(Math.round(L * s.sr));
    const t = buf(s, 0.62);
    tone(s, t, (x) => 480 + 520 * (x / 0.62), 0.6, 'square', 0, 4500);
    shape(s, t, (x) => (x < 0.01 ? x / 0.01 : x > 0.58 ? Math.max(0, (0.62 - x) / 0.04) : 1));
    add(s, b, t);
    return normalize(speaker(s, b), 0.8);
  },
});

// master caution (amber): a two-tone chime every couple of seconds
defineSound('alarm.caution', {
  loop: true,
  ref: 2,
  range: 60,
  gain: 0.35,
  make: (s) => {
    const b = new Float32Array(Math.round(2.4 * s.sr));
    bell(s, b, 1047, 0.02, 1, 0.5);
    bell(s, b, 784, 0.38, 0.9, 0.6);
    return normalize(speaker(s, b), 0.8);
  },
});

// airlock: a cycle starts (two short beeps), the lock is ready (a rising chime)
defineSound('chime.cycle', {
  ref: 1.5,
  range: 30,
  gain: 0.35,
  make: (s) => {
    const b = buf(s, 0.45);
    for (const at of [0, 0.2]) {
      const t = buf(s, 0.12);
      tone(s, t, 1000, 0.5, 'tri', 0, 5000);
      shape(s, t, (x) => (x < 0.004 ? x / 0.004 : x > 0.1 ? Math.max(0, (0.12 - x) / 0.02) : 1));
      add(s, b, t, 1, at);
    }
    return edges(s, normalize(speaker(s, b), 0.75), 0.0005, 0.01);
  },
});

defineSound('chime.ok', {
  ref: 1.5,
  range: 30,
  gain: 0.35,
  make: (s) => {
    const b = buf(s, 1.3);
    bell(s, b, 880, 0, 1, 0.45);
    bell(s, b, 1319, 0.18, 1, 0.6);
    return edges(s, normalize(speaker(s, b), 0.75), 0.0005, 0.1);
  },
});

// autopilot engaged: one soft chime
defineSound('ap.on', {
  ref: 1,
  range: 20,
  gain: 0.3,
  make: (s) => {
    const b = buf(s, 0.9);
    bell(s, b, 1319, 0, 1, 0.4);
    return edges(s, normalize(speaker(s, b), 0.7), 0.0005, 0.1);
  },
});

// autopilot disconnect: the warble you can't miss
defineSound('ap.off', {
  ref: 1,
  range: 25,
  gain: 0.45,
  make: (s) => {
    const T = 1.5;
    const b = buf(s, T);
    tone(s, b, (t) => (Math.floor(t * 16) % 2 === 0 ? 1175 : 1480), (t) => (t < 1.4 ? 0.5 : Math.max(0, (T - t) / 0.1) * 0.5), 'tri', 0, 6000);
    return edges(s, normalize(speaker(s, b), 0.75), 0.002, 0.02);
  },
});

// radio: a message comes in (the HUD's line)
defineSound('ui.msg', {
  gain: 0.2,
  make: (s) => {
    const b = buf(s, 0.28);
    for (const [at, f] of [[0, 1400], [0.06, 2100]] as const) {
      const t = buf(s, 0.045);
      tone(s, t, f, 0.5);
      shape(s, t, (x) => (x < 0.003 ? x / 0.003 : x > 0.04 ? Math.max(0, (0.045 - x) / 0.005) : 1));
      add(s, b, t, 1, at);
    }
    const n = buf(s, 0.15);
    noise(s, n, (x) => 0.12 * Math.exp(-x / 0.05));
    filter(s, n, 'bp', 2500, 1.2);
    add(s, b, n, 1, 0.11);
    return edges(s, normalize(b, 0.6), 0.0005, 0.01);
  },
});
