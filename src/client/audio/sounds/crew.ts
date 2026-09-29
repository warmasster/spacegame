// The astronaut: boots on each kind of ground (surfaces.ts names them), landings, the jetpack,
// tools, the suit and the helmet (breathing, its fan, its warnings, what hits it).

import { defineSound } from '../bank';
import { ad, add, brown, buf, crackle, dc, edges, filter, fit, loopOf, modes, noise, normalize, pink, shape, sweep, thump, tick, tone, wobble } from '../dsp';

const step = { ref: 0.6, range: 30, jitter: 0.06 };

// boot on regolith: a soft heel thud and the grit crunching under it
defineSound('step.regolith', {
  ...step,
  gain: 0.42,
  variants: 5,
  make: (s) => {
    const b = buf(s, 0.32);
    thump(s, b, 0, 95, 60, 0.035, 0.8);
    const g = buf(s, 0.12);
    crackle(s, g, 1100, ad(0.005, 0.03), 0.0015);
    filter(s, g, 'lp', 2600);
    add(s, b, g, 0.9, 0.005);
    const sc = buf(s, 0.1);
    noise(s, sc, ad(0.01, 0.025));
    filter(s, sc, 'lp', 1200);
    add(s, b, sc, 0.3, 0.03);
    return edges(s, normalize(dc(s, b), 0.9), 0.0005, 0.04);
  },
});

// boot on a metal deck: a thud and the plate ringing
defineSound('step.deck', {
  ...step,
  gain: 0.5,
  variants: 5,
  make: (s) => {
    const k = 1 + s.rng.range(-0.07, 0.07);
    const b = buf(s, 0.5);
    thump(s, b, 0, 125, 80, 0.03, 0.9);
    tick(s, b, 0, 0.006, 2000, 0.4, 'bp', 0.8);
    modes(s, b, [[410 * k, 0.09, 0.3], [935 * k, 0.06, 0.22], [1660 * k, 0.04, 0.14], [2610 * k, 0.025, 0.08]]);
    return edges(s, normalize(dc(s, b), 0.9), 0.0005, 0.05);
  },
});

// boot on a cargo crate: hollow
defineSound('step.crate', {
  ...step,
  gain: 0.45,
  variants: 3,
  make: (s) => {
    const b = buf(s, 0.35);
    thump(s, b, 0, 170, 120, 0.04, 1);
    modes(s, b, [[300, 0.06, 0.35], [660, 0.04, 0.2]]);
    return edges(s, normalize(dc(s, b), 0.9), 0.0005, 0.04);
  },
});

defineSound('land.regolith', {
  ...step,
  ref: 0.8,
  gain: 0.65,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.6);
    thump(s, b, 0, 80, 45, 0.08, 1);
    const g = buf(s, 0.3);
    crackle(s, g, 1400, ad(0.005, 0.08), 0.002);
    filter(s, g, 'lp', 2400);
    add(s, b, g, 1, 0.005);
    const d = buf(s, 0.5);
    noise(s, d, ad(0.02, 0.12));
    filter(s, d, 'lp', 900);
    add(s, b, d, 0.35, 0.02);
    return edges(s, normalize(dc(s, b), 0.92), 0.0005, 0.06);
  },
});

defineSound('land.deck', {
  ...step,
  ref: 0.8,
  gain: 0.7,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.9);
    thump(s, b, 0, 105, 60, 0.07, 1);
    tick(s, b, 0, 0.01, 1500, 0.5, 'lp', 0.7);
    modes(s, b, [[330, 0.2, 0.35], [760, 0.14, 0.25], [1420, 0.09, 0.15], [2500, 0.05, 0.08]]);
    return edges(s, normalize(dc(s, b), 0.92), 0.0005, 0.08);
  },
});

// jetpack: cold gas through the pack's nozzles, fluttering
defineSound('jet.hiss', {
  loop: true,
  ref: 0.8,
  range: 40,
  gain: 0.5,
  make: (s) => {
    const L = 1.5;
    const f = wobble(s, L, 16);
    return loopOf(
      s,
      L,
      (b) => {
        noise(s, b, (t) => 0.6 * (0.75 + 0.25 * f(t)));
        filter(s, b, 'hp', 1100);
        filter(s, b, 'peak', 3000, 0.8, 5);
        filter(s, b, 'lp', 9000);
        crackle(s, b, 60, 0.1, 0.001);
      },
      null,
    );
  },
});

// welding arc: dense crackle over the power supply's buzz
defineSound('weld.arc', {
  loop: true,
  ref: 0.5,
  range: 30,
  gain: 0.45,
  make: (s) => {
    const L = 1.6;
    const w = wobble(s, L, 9);
    return loopOf(
      s,
      L,
      (b) => {
        crackle(s, b, (t) => 1100 + 900 * w(t), 1, 0.0006);
        filter(s, b, 'hp', 1500);
        const h = buf(s, b.length / s.sr);
        noise(s, h, 0.08);
        filter(s, h, 'hp', 5000);
        add(s, b, h);
      },
      (b) => tone(s, b, fit(100, L), 0.12, 'square', 0, 3000),
    );
  },
});

// rocket launcher: the tube kicks, the motor lights and is gone
defineSound('launcher.fire', {
  ref: 0.8,
  range: 300,
  gain: 0.9,
  variants: 2,
  make: (s) => {
    const T = 1.3;
    const b = buf(s, T);
    thump(s, b, 0, 85, 40, 0.08, 1);
    const c = buf(s, 0.04);
    noise(s, c, ad(0.0003, 0.006));
    add(s, b, c, 0.8);
    const w = buf(s, T);
    pink(s, w, ad(0.01, 0.35));
    sweep(s, w, 'bp', (t) => 700 + 2800 * Math.exp(-t / 0.3), 0.8);
    add(s, b, w, 1.1);
    return edges(s, normalize(dc(s, b), 0.95), 0.0003, 0.2);
  },
});

// carbine: a sharp crack through the stock, the action cycling
defineSound('rifle.fire', {
  ref: 0.6,
  range: 400,
  gain: 0.75,
  variants: 3,
  jitter: 0.05,
  make: (s) => {
    const T = 0.5;
    const b = buf(s, T);
    thump(s, b, 0, 150, 70, 0.03, 0.9);
    const c = buf(s, 0.03);
    noise(s, c, ad(0.0002, 0.004));
    add(s, b, c, 1);
    const w = buf(s, T);
    pink(s, w, ad(0.002, 0.08));
    sweep(s, w, 'bp', (t) => 900 + 3000 * Math.exp(-t / 0.05), 0.9);
    add(s, b, w, 0.8);
    // the bolt going back and home
    tick(s, b, 0.05, 0.003, 2600, 0.35, 'bp', 1);
    modes(s, b, [[1700, 0.02, 0.2], [3900, 0.01, 0.12]], 0.05);
    return edges(s, normalize(dc(s, b), 0.95), 0.0002, 0.1);
  },
});

// tools: drawn from the pack / slung back
defineSound('tool.equip', {
  ref: 0.4,
  range: 12,
  gain: 0.35,
  make: (s) => {
    const b = buf(s, 0.35);
    tick(s, b, 0, 0.004, 3000, 0.6, 'bp', 1);
    modes(s, b, [[1400, 0.03, 0.3], [3200, 0.015, 0.2]]);
    thump(s, b, 0.12, 200, 140, 0.03, 0.5);
    tick(s, b, 0.12, 0.004, 2500, 0.5, 'bp', 1);
    modes(s, b, [[980, 0.04, 0.25]], 0.12);
    return edges(s, normalize(dc(s, b), 0.85), 0.0003, 0.04);
  },
});

defineSound('tool.stow', {
  ref: 0.4,
  range: 12,
  gain: 0.3,
  make: (s) => {
    const b = buf(s, 0.35);
    thump(s, b, 0, 180, 130, 0.03, 0.5);
    tick(s, b, 0, 0.004, 2500, 0.5, 'bp', 1);
    tick(s, b, 0.09, 0.003, 3600, 0.5, 'bp', 1.5);
    modes(s, b, [[2100, 0.02, 0.3]], 0.09);
    return edges(s, normalize(dc(s, b), 0.85), 0.0003, 0.04);
  },
});

// a harness buckle and the seat taking the weight
defineSound('seat.buckle', {
  ref: 0.5,
  range: 12,
  gain: 0.4,
  make: (s) => {
    const b = buf(s, 0.6);
    thump(s, b, 0, 120, 80, 0.05, 0.6);
    const c = buf(s, 0.3);
    noise(s, c, ad(0.03, 0.08));
    filter(s, c, 'bp', 700, 2);
    add(s, b, c, 0.3);
    tick(s, b, 0.22, 0.003, 3800, 0.6, 'bp', 1.5);
    modes(s, b, [[2400, 0.03, 0.35], [4600, 0.015, 0.2]], 0.22);
    return edges(s, normalize(dc(s, b), 0.85), 0.0005, 0.04);
  },
});

// ---- inside the helmet ----------------------------------------------------------------------

// breathing: in and out through the suit's loop (played faster when working hard)
defineSound('suit.breath', {
  loop: true,
  gain: 0.13,
  make: (s) => {
    const L = 4.8;
    return loopOf(
      s,
      L,
      (b) => {
        const inh = (t: number) => (t > 0.05 && t < 1.35 ? Math.sin((Math.PI * (t - 0.05)) / 1.3) ** 1.5 : 0);
        const exh = (t: number) => (t > 1.8 && t < 3.5 ? Math.sin((Math.PI * (t - 1.8)) / 1.7) ** 1.2 : 0);
        const a = buf(s, b.length / s.sr);
        noise(s, a, (t) => 0.6 * inh(t));
        filter(s, a, 'bp', 1500, 0.8);
        const e = buf(s, b.length / s.sr);
        noise(s, e, (t) => 0.7 * exh(t));
        filter(s, e, 'bp', 650, 0.6);
        add(s, b, a);
        add(s, b, e);
      },
      null,
      0.85,
      0.2,
    );
  },
});

// the suit's fan pushing air past your face
defineSound('suit.fan', {
  loop: true,
  gain: 0.05,
  make: (s) => {
    const L = 2;
    return loopOf(
      s,
      L,
      (b) => {
        pink(s, b, 0.7);
        filter(s, b, 'bp', 1800, 0.4);
      },
      (b) => tone(s, b, fit(190, L), 0.05),
    );
  },
});

// low oxygen: three quick beeps
defineSound('suit.warn', {
  gain: 0.4,
  make: (s) => {
    const b = buf(s, 0.62);
    for (let i = 0; i < 3; i++) {
      const t = buf(s, 0.08);
      tone(s, t, 1760, 0.5, 'square', 0, 6000);
      shape(s, t, (x) => (x < 0.004 ? x / 0.004 : x > 0.07 ? Math.max(0, (0.08 - x) / 0.01) : 1));
      add(s, b, t, 1, i * 0.16);
    }
    filter(s, b, 'lp', 5000);
    return edges(s, normalize(b, 0.7), 0.0005, 0.01);
  },
});

// jetpack running dry: a low double beep
defineSound('suit.fuel', {
  gain: 0.3,
  make: (s) => {
    const b = buf(s, 0.4);
    for (const [at, f] of [[0, 880], [0.18, 660]] as const) {
      const t = buf(s, 0.12);
      tone(s, t, f, 0.5, 'tri', 0, 6000);
      shape(s, t, (x) => (x < 0.004 ? x / 0.004 : Math.exp(-(x - 0.004) / 0.05)));
      add(s, b, t, 1, at);
    }
    return edges(s, normalize(b, 0.6), 0.0005, 0.01);
  },
});

// the helmet lamp switch
defineSound('suit.click', {
  gain: 0.25,
  make: (s) => {
    const b = buf(s, 0.06);
    tick(s, b, 0, 0.002, 4200, 0.8, 'bp', 1.5);
    modes(s, b, [[3100, 0.006, 0.3]]);
    return edges(s, normalize(dc(s, b), 0.7), 0.0003, 0.01);
  },
});

// something hits you: a thud through the suit and the visor rattling
defineSound('suit.hit', {
  gain: 0.8,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.7);
    thump(s, b, 0, 90, 50, 0.08, 1);
    const n = buf(s, 0.2);
    noise(s, n, ad(0.001, 0.03));
    filter(s, n, 'lp', 2500);
    add(s, b, n, 0.7);
    modes(s, b, [[1800, 0.08, 0.15], [3900, 0.05, 0.1]], 0.01);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.08);
  },
});

// grit and pebbles of a blast pelting the suit
defineSound('suit.pelt', {
  gain: 0.55,
  variants: 2,
  make: (s) => {
    const T = 1.4;
    const b = buf(s, T);
    crackle(s, b, (t) => 350 * Math.exp(-t / 0.35), 1, 0.003);
    filter(s, b, 'bp', 1400, 0.6);
    for (let i = 0; i < 8; i++) thump(s, b, Math.pow(s.rng.next(), 2) * 0.8, 140, 90, 0.02, s.rng.range(0.3, 0.8));
    const d = buf(s, T);
    brown(s, d, ad(0.01, 0.3));
    filter(s, d, 'lp', 400);
    add(s, b, d, 0.5);
    return edges(s, normalize(dc(s, b), 0.85), 0.0005, 0.1);
  },
});

// ears ringing after a blast close by
defineSound('ear.ring', {
  gain: 0.12,
  make: (s) => {
    const T = 4.5;
    const b = buf(s, T);
    tone(s, b, (t) => 3800 - 30 * t, (t) => Math.min(1, t / 0.2) * Math.exp(-t / 1.6));
    tone(s, b, 7560, (t) => 0.1 * Math.exp(-t / 1));
    return edges(s, normalize(b, 0.7), 0.05, 0.3);
  },
});
