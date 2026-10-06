// Air on the move: openings with gas through them (shared/ship/airflow.ts vents), gas fed into a
// room, a room blowing out, a panel giving way.

import { defineSound } from '../bank';
import { ad, add, brown, buf, dc, edges, filter, loopOf, modes, noise, normalize, pink, saturate, sweep, thump, wobble } from '../dsp';

// air rushing through an opening: turbulent, with the low roar of a big one (pitch: its speed)
defineSound('air.rush', {
  loop: true,
  ref: 1.5,
  range: 200,
  gain: 0.85,
  make: (s) => {
    const L = 3;
    const t1 = wobble(s, L, 11);
    const t2 = wobble(s, L, 3);
    return loopOf(
      s,
      L,
      (b) => {
        noise(s, b, (t) => 0.55 * (0.55 + 0.3 * t1(t) + 0.15 * t2(t)));
        filter(s, b, 'bp', 950, 0.45);
        const h = buf(s, b.length / s.sr);
        noise(s, h, (t) => 0.2 * (0.6 + 0.4 * t1(t)));
        filter(s, h, 'hp', 3500);
        add(s, b, h);
        const r = buf(s, b.length / s.sr);
        brown(s, r, (t) => 0.5 * (0.6 + 0.4 * t2(t)));
        filter(s, r, 'lp', 260);
        add(s, b, r);
      },
      null,
    );
  },
});

// wind of a body with an atmosphere (BodyDef.wind): gusts over open ground. Never on the Moon.
defineSound('env.wind', {
  loop: true,
  ref: 50,
  range: 1e9,
  gain: 0.5,
  body: 0,
  make: (s) => {
    const L = 8;
    const gust = wobble(s, L, 0.4);
    const flutter = wobble(s, L, 3);
    return loopOf(
      s,
      L,
      (b) => {
        pink(s, b, (t) => 0.7 * (0.35 + 0.65 * gust(t)) * (0.85 + 0.15 * flutter(t)));
        sweep(s, b, 'bp', (t) => 300 + 700 * gust(t), 0.6);
      },
      null,
    );
  },
});

// a vent valve dumping a room's air through its pipe: a rush narrowed by the pipe
defineSound('air.vent', { like: 'air.rush', pitch: 1.35, gain: 0.8 });

// a thin hiss: gas fed into a room, a crack, a tank venting
defineSound('air.hiss', {
  loop: true,
  ref: 0.6,
  range: 40,
  gain: 0.4,
  make: (s) => {
    const w = wobble(s, 1.2, 5);
    return loopOf(
      s,
      1.2,
      (b) => {
        noise(s, b, (t) => 0.5 * (0.85 + 0.15 * w(t)));
        filter(s, b, 'hp', 3300);
        filter(s, b, 'lp', 12000);
      },
      null,
    );
  },
});

// a panel blown out by the pressure behind it: the pop, the tearing metal, the gust
defineSound('decomp.bang', {
  ref: 4,
  range: 500,
  gain: 1.2,
  variants: 2,
  make: (s) => {
    const T = 2.6;
    const b = buf(s, T);
    thump(s, b, 0, 60, 28, 0.22, 1.2);
    const pop = buf(s, 0.05);
    noise(s, pop, ad(0.0005, 0.008));
    add(s, b, pop, 1);
    // the metal tearing: a resonant screech sliding down
    const tear = buf(s, 0.6);
    noise(s, tear, ad(0.005, 0.18));
    sweep(s, tear, 'bp', (t) => 3200 * Math.exp(-t / 0.25) + 500, 9);
    add(s, b, tear, 1.4, 0.01);
    // the gust of air after it
    const g = buf(s, T);
    noise(s, g, ad(0.02, 0.7));
    sweep(s, g, 'lp', (t) => 3500 * Math.exp(-t / 0.6) + 300);
    add(s, b, g, 0.8);
    saturate(b, 1.5);
    return edges(s, normalize(dc(s, b), 0.98), 0.0003, 0.3);
  },
});

// a room blowing out: the slug of air leaving all at once, the structure ringing
defineSound('decomp.blast', {
  ref: 5,
  range: 300,
  gain: 1,
  make: (s) => {
    const T = 2.8;
    const b = buf(s, T);
    brown(s, b, ad(0.03, 0.6));
    const w = buf(s, T);
    pink(s, w, ad(0.02, 0.9));
    sweep(s, w, 'lp', (t) => 2500 * Math.exp(-t / 0.8) + 250);
    add(s, b, w, 0.9);
    modes(s, b, [[95, 1.2, 0.35], [210, 0.8, 0.25], [380, 0.5, 0.15]]);
    return edges(s, normalize(dc(s, b), 0.95), 0.002, 0.3);
  },
});

// a panel past what it holds: the metal groans (stick-slip through its resonances)
defineSound('metal.creak', {
  ref: 1.5,
  range: 50,
  gain: 0.55,
  variants: 3,
  make: (s) => {
    const T = 1.3;
    const b = buf(s, T);
    const rate = wobble(s, T, 4);
    // an irregular train of impulses (the slips), faster as it strains
    let next = 0;
    for (let i = 0; i < b.length; i++) {
      if (i >= next) {
        b[i] += s.rng.range(0.5, 1);
        next = i + Math.round(s.sr / (18 + 50 * rate(i / s.sr)));
      }
    }
    const out = buf(s, T);
    for (const [f, q, a] of [[230 + s.rng.range(-30, 30), 18, 1], [590 + s.rng.range(-50, 50), 14, 0.6], [1120 + s.rng.range(-80, 80), 10, 0.35]] as const) {
      const c = b.slice();
      filter(s, c, 'bp', f, q);
      add(s, out, c, a);
    }
    const env = (t: number) => Math.sin(Math.PI * Math.min(1, t / T)) ** 0.7;
    for (let i = 0; i < out.length; i++) out[i] *= env(i / s.sr);
    return edges(s, normalize(dc(s, out), 0.9), 0.01, 0.05);
  },
});

// a glass pane giving way
defineSound('glass.break', {
  ref: 1.5,
  range: 80,
  gain: 0.8,
  variants: 2,
  make: (s) => {
    const T = 1.6;
    const b = buf(s, T);
    const c = buf(s, 0.08);
    noise(s, c, ad(0.0003, 0.01));
    filter(s, c, 'hp', 3000);
    add(s, b, c, 1);
    // shards: many small bright rings, thinning out
    for (let i = 0; i < 70; i++) {
      const at = Math.pow(s.rng.next(), 2) * 0.9;
      modes(s, b, [[s.rng.range(2800, 9000), s.rng.range(0.02, 0.09), s.rng.range(0.05, 0.25)]], at);
    }
    const n = buf(s, T);
    noise(s, n, ad(0.002, 0.25));
    filter(s, n, 'hp', 4000);
    add(s, b, n, 0.3);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.2);
  },
});
