// Console controls: one voice per kind of control (client/audio/shipSounds.ts CONTROL_SOUNDS),
// and the buzzer of a control that refuses. Small, close, bright: heard in the cabin air, felt
// through the glove in vacuum.

import { defineSound } from '../bank';
import { add, buf, dc, edges, filter, modes, noise, normalize, shape, thump, tick, tone } from '../dsp';

const small = { ref: 0.35, range: 14, jitter: 0.05 };

// push button: plastic, a press and a softer release
defineSound('ctl.button', {
  ...small,
  gain: 0.32,
  variants: 3,
  make: (s) => {
    const r = s.rng;
    const b = buf(s, 0.18);
    tick(s, b, 0, 0.004, 3400 + r.range(-300, 300), 0.8, 'bp', 0.9);
    modes(s, b, [[2300 + r.range(-200, 200), 0.012, 0.5], [4100, 0.006, 0.35], [640, 0.012, 0.45]]);
    const rel = 0.075 + r.range(0, 0.025);
    tick(s, b, rel, 0.003, 4300, 0.4, 'bp', 1);
    modes(s, b, [[2650, 0.008, 0.28], [5200, 0.004, 0.18]], rel);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.02);
  },
});

// toggle switch: a metal snap over its spring
defineSound('ctl.toggle', {
  ...small,
  gain: 0.42,
  variants: 3,
  make: (s) => {
    const k = 1 + s.rng.range(-0.06, 0.06);
    const b = buf(s, 0.22);
    tick(s, b, 0, 0.0025, 6000, 1, 'hp', 0.7);
    modes(s, b, [[1850 * k, 0.035, 0.6], [3320 * k, 0.022, 0.45], [5150 * k, 0.012, 0.35], [7400 * k, 0.007, 0.2], [430 * k, 0.018, 0.35]]);
    tone(s, b, 2950 * k, (t) => 0.05 * Math.exp(-t / 0.04), 'sine', 0.002);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.03);
  },
});

// lever: a ratchet over its teeth, then the detent clunks home
defineSound('ctl.lever', {
  ...small,
  gain: 0.5,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.36);
    for (let i = 0; i < 4; i++) {
      const at = i * 0.032 + s.rng.range(0, 0.006);
      tick(s, b, at, 0.002, 3800, 0.35, 'bp', 2);
      modes(s, b, [[2750, 0.006, 0.25], [4400, 0.004, 0.15]], at);
    }
    thump(s, b, 0.14, 240, 150, 0.05, 0.7);
    tick(s, b, 0.14, 0.006, 1400, 0.6, 'bp', 0.8);
    modes(s, b, [[610, 0.05, 0.35], [1330, 0.03, 0.25], [2900, 0.012, 0.15]], 0.14);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.04);
  },
});

// breaker: a heavy spring-loaded clack (also when it trips on its own)
defineSound('ctl.breaker', {
  ref: 0.5,
  range: 22,
  jitter: 0.04,
  gain: 0.62,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.35);
    tick(s, b, 0, 0.004, 2200, 1, 'hp', 0.6);
    thump(s, b, 0, 180, 110, 0.045, 0.8);
    modes(s, b, [[620, 0.07, 0.5], [1460, 0.05, 0.45], [2900, 0.025, 0.3], [5300, 0.01, 0.2]]);
    return edges(s, normalize(dc(s, b), 0.95), 0.0003, 0.05);
  },
});

// master caution: a soft square push-button under a lens
defineSound('ctl.master', {
  ...small,
  gain: 0.3,
  make: (s) => {
    const b = buf(s, 0.12);
    tick(s, b, 0, 0.005, 1500, 0.8, 'lp', 0.7);
    modes(s, b, [[1500, 0.012, 0.4], [2650, 0.007, 0.25], [520, 0.015, 0.4]]);
    return edges(s, normalize(dc(s, b), 0.85), 0.0003, 0.02);
  },
});

// mushroom (SCRAM): a big plastic thunk and the latch catching
defineSound('ctl.mushroom', {
  ref: 0.4,
  range: 18,
  jitter: 0.03,
  gain: 0.6,
  make: (s) => {
    const b = buf(s, 0.3);
    thump(s, b, 0, 220, 150, 0.06, 1);
    tick(s, b, 0, 0.008, 900, 0.7, 'lp', 0.7);
    modes(s, b, [[480, 0.04, 0.3], [1100, 0.02, 0.2]]);
    tick(s, b, 0.045, 0.003, 4200, 0.45, 'bp', 1.5);
    modes(s, b, [[3100, 0.01, 0.25]], 0.045);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.03);
  },
});

// rotary selector: one detent
defineSound('ctl.rotary', {
  ...small,
  gain: 0.3,
  variants: 3,
  make: (s) => {
    const b = buf(s, 0.08);
    tick(s, b, 0, 0.002, 4800 + s.rng.range(-500, 500), 0.7, 'bp', 1.2);
    modes(s, b, [[3200 + s.rng.range(-200, 200), 0.008, 0.35], [5600, 0.004, 0.2], [900, 0.006, 0.2]]);
    return edges(s, normalize(dc(s, b), 0.8), 0.0003, 0.01);
  },
});

// flip cover: the lid clacks over and its spring twangs
defineSound('ctl.cover', {
  ...small,
  gain: 0.38,
  make: (s) => {
    const b = buf(s, 0.3);
    tick(s, b, 0, 0.004, 2600, 0.8, 'bp', 0.8);
    modes(s, b, [[1750, 0.02, 0.4], [3900, 0.01, 0.25], [700, 0.02, 0.3]]);
    tone(s, b, (t) => 880 + 30 * Math.sin(t * 90), (t) => 0.12 * Math.exp(-t / 0.07), 'tri', 0.004, 5000);
    return edges(s, normalize(dc(s, b), 0.85), 0.0003, 0.04);
  },
});

// valve handwheel: a dry squeak of the stem, then it seats
defineSound('ctl.valve', {
  ...small,
  gain: 0.45,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.5);
    const f0 = 620 + s.rng.range(-60, 60);
    // stick-slip: the squeak comes and goes
    tone(s, b, (t) => f0 + 260 * t + 25 * Math.sin(t * 70), (t) => 0.3 * (t < 0.3 ? Math.sin((Math.PI * t) / 0.3) : 0) * (0.6 + 0.4 * Math.sin(t * 150) ** 2), 'saw', 0, 4000);
    const grit = buf(s, 0.3);
    noise(s, grit, 0.15);
    filter(s, grit, 'bp', 2400, 1.5);
    add(s, b, grit);
    thump(s, b, 0.32, 200, 140, 0.04, 0.6);
    modes(s, b, [[520, 0.05, 0.3], [1180, 0.03, 0.2]], 0.32);
    return edges(s, normalize(dc(s, b), 0.85), 0.002, 0.04);
  },
});

// MFD bezel key: a soft rubber click
defineSound('ctl.bezel', {
  ...small,
  gain: 0.22,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.07);
    tick(s, b, 0, 0.004, 2000, 0.7, 'lp', 0.7);
    modes(s, b, [[820, 0.01, 0.35], [1900, 0.006, 0.2]]);
    return edges(s, normalize(dc(s, b), 0.8), 0.0003, 0.01);
  },
});

// refused: the console's buzzer, twice
defineSound('ctl.deny', {
  ref: 0.4,
  range: 16,
  jitter: 0,
  gain: 0.42,
  make: (s) => {
    const b = buf(s, 0.42);
    for (const [at, f] of [[0, 196], [0.2, 165]] as const) {
      const t = buf(s, 0.15);
      tone(s, t, f, 0.5, 'square', 0, 3500);
      shape(s, t, (x) => (x < 0.004 ? x / 0.004 : x > 0.135 ? Math.max(0, (0.15 - x) / 0.015) : 1));
      add(s, b, t, 1, at);
    }
    // a small speaker behind a grille
    filter(s, b, 'hp', 280, 0.7);
    filter(s, b, 'peak', 1600, 1.2, 5);
    filter(s, b, 'lp', 3800, 0.8);
    return edges(s, normalize(b, 0.8), 0.0005, 0.01);
  },
});

// helmet version of a refusal (a control out of reach, a tool that can't)
defineSound('ui.deny', { like: 'ctl.deny', pitch: 1.3, gain: 0.6 });
