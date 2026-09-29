// Blasts, hits and breakage: explosions, hull panels struck and blown, machines wrecked, sparks,
// a ship setting down, loose cargo.

import { defineSound } from '../bank';
import { ad, add, brown, buf, crackle, dc, edges, filter, modes, noise, normalize, pink, saturate, sweep, thump, tick, tone } from '../dsp';

/** An explosion of a given size (1 = a rocket; bigger rumbles longer and lower). */
function blast(size: number) {
  return (s: import('../dsp').Synth) => {
    const T = 2 + 2 * size;
    const b = buf(s, T);
    // the crack of the blast front
    const c = buf(s, 0.03);
    noise(s, c, ad(0.0003, 0.006 * size));
    add(s, b, c, 1.1);
    // the body: broadband, its highs dying first
    const body = buf(s, T);
    noise(s, body, ad(0.003, 0.35 * size));
    sweep(s, body, 'lp', (t) => 5000 * Math.exp(-t / (0.18 * size)) + 180);
    add(s, b, body, 1.2);
    // the push of it
    thump(s, b, 0, 75 / Math.sqrt(size), 26, 0.45 * size, 1.5);
    // the rumble that rolls on
    const r = buf(s, T);
    brown(s, r, ad(0.15, 0.9 * size));
    filter(s, r, 'lp', 170);
    add(s, b, r, 0.9);
    // debris and gravel falling back
    const d = buf(s, T);
    crackle(s, d, (t) => 260 * Math.exp(-t / (0.5 * size)), (t) => (t < 0.08 ? 0 : 0.6), 0.003);
    filter(s, d, 'lp', 3500);
    add(s, b, d, 0.5);
    saturate(b, 1.8);
    return edges(s, normalize(dc(s, b), 0.98), 0.0002, 0.4);
  };
}

defineSound('boom.big', { ref: 10, range: 6000, gain: 1.5, variants: 3, jitter: 0.08, make: blast(1.4) });
defineSound('boom.small', { ref: 5, range: 2500, gain: 1.1, variants: 2, jitter: 0.08, make: blast(0.7) });

// a hull plate struck: a clang with its plate modes
defineSound('hull.hit', {
  ref: 2,
  range: 120,
  gain: 0.8,
  variants: 3,
  make: (s) => {
    const k = 1 + s.rng.range(-0.1, 0.1);
    const b = buf(s, 1.3);
    tick(s, b, 0, 0.006, 3000, 0.8, 'hp', 0.7);
    thump(s, b, 0, 110, 70, 0.06, 0.9);
    modes(s, b, [[240 * k, 0.55, 0.45], [530 * k, 0.42, 0.4], [910 * k, 0.3, 0.3], [1370 * k, 0.24, 0.25], [2210 * k, 0.15, 0.15], [3330 * k, 0.08, 0.1]]);
    return edges(s, normalize(dc(s, b), 0.92), 0.0003, 0.2);
  },
});

// a hull plate torn out: tearing, crashing, ringing
defineSound('hull.breach', {
  ref: 3,
  range: 300,
  gain: 1.1,
  variants: 2,
  make: (s) => {
    const T = 2;
    const b = buf(s, T);
    thump(s, b, 0, 90, 40, 0.15, 1);
    const tear = buf(s, 0.8);
    noise(s, tear, ad(0.004, 0.25));
    sweep(s, tear, 'bp', (t) => 2400 * Math.exp(-t / 0.35) + 380, 7);
    add(s, b, tear, 1.3);
    crackle(s, b, (t) => 400 * Math.exp(-t / 0.3), 0.5, 0.004);
    modes(s, b, [[180, 0.9, 0.3], [470, 0.6, 0.25], [1020, 0.35, 0.18], [1900, 0.2, 0.1]]);
    saturate(b, 1.4);
    return edges(s, normalize(dc(s, b), 0.95), 0.0003, 0.3);
  },
});

// a blown panel welded shut again: the plate seats
defineSound('hull.seal', { like: 'hull.hit', pitch: 0.8, gain: 0.55 });

// a bullet striking: a sharp tick and a short ring of whatever it hit
defineSound('bullet.hit', { like: 'hull.hit', pitch: 1.9, gain: 0.5 });

// an electrical spark: a crackle and a buzz
defineSound('spark', {
  ref: 0.5,
  range: 25,
  gain: 0.35,
  variants: 3,
  jitter: 0.1,
  make: (s) => {
    const b = buf(s, 0.35);
    crackle(s, b, (t) => 900 * Math.exp(-t / 0.08), 1, 0.0008);
    filter(s, b, 'hp', 1800);
    tone(s, b, 100, (t) => 0.2 * Math.exp(-t / 0.05), 'square', 0, 3000);
    return edges(s, normalize(dc(s, b), 0.85), 0.0003, 0.05);
  },
});

// a machine hit: a dull clunk and a spark
defineSound('machine.hit', {
  ref: 1,
  range: 60,
  gain: 0.65,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.8);
    thump(s, b, 0, 160, 90, 0.05, 1);
    tick(s, b, 0, 0.01, 1200, 0.6, 'lp', 0.7);
    modes(s, b, [[380, 0.18, 0.3], [870, 0.12, 0.2], [1650, 0.06, 0.12]]);
    crackle(s, b, (t) => 500 * Math.exp(-t / 0.1), 0.3, 0.0008);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.1);
  },
});

// a machine wrecked: crunch, parts going, sparks
defineSound('machine.break', {
  ref: 1.5,
  range: 120,
  gain: 0.95,
  variants: 2,
  make: (s) => {
    const T = 1.6;
    const b = buf(s, T);
    thump(s, b, 0, 110, 50, 0.12, 1);
    const n = buf(s, T);
    noise(s, n, ad(0.003, 0.2));
    filter(s, n, 'lp', 1600);
    add(s, b, n, 0.9);
    crackle(s, b, (t) => 600 * Math.exp(-t / 0.35), 0.5, 0.002);
    modes(s, b, [[260, 0.4, 0.25], [640, 0.3, 0.2], [1310, 0.2, 0.15], [2750, 0.1, 0.1]]);
    for (let i = 0; i < 5; i++) modes(s, b, [[s.rng.range(600, 3000), 0.05, 0.12]], s.rng.range(0.1, 0.8));
    saturate(b, 1.3);
    return edges(s, normalize(dc(s, b), 0.95), 0.0003, 0.2);
  },
});

// a ship setting down: the gear takes the weight
defineSound('ship.touchdown', {
  ref: 5,
  range: 600,
  gain: 1,
  make: (s) => {
    const T = 1.8;
    const b = buf(s, T);
    thump(s, b, 0, 55, 30, 0.35, 1.3);
    modes(s, b, [[120, 0.6, 0.35], [265, 0.4, 0.25], [590, 0.25, 0.15]]);
    // the oleos compressing
    const o = buf(s, 0.9);
    noise(s, o, ad(0.05, 0.25));
    filter(s, o, 'bp', 1600, 1.2);
    add(s, b, o, 0.25, 0.02);
    const d = buf(s, T);
    brown(s, d, ad(0.02, 0.4));
    filter(s, d, 'lp', 300);
    add(s, b, d, 0.6);
    return edges(s, normalize(dc(s, b), 0.95), 0.0005, 0.3);
  },
});

// loose cargo: grabbed by the suit's clamp, set or dropped down, thrown
defineSound('crate.grab', {
  ref: 0.6,
  range: 20,
  gain: 0.45,
  make: (s) => {
    const b = buf(s, 0.4);
    thump(s, b, 0, 180, 120, 0.04, 0.8);
    modes(s, b, [[720, 0.06, 0.3], [1650, 0.03, 0.2]]);
    tone(s, b, 120, (t) => (t > 0.02 && t < 0.11 ? 0.25 : 0), 'square', 0, 2500);
    return edges(s, normalize(dc(s, b), 0.85), 0.0003, 0.04);
  },
});

defineSound('crate.drop', {
  ref: 0.8,
  range: 35,
  gain: 0.6,
  variants: 3,
  make: (s) => {
    const k = 1 + s.rng.range(-0.1, 0.1);
    const b = buf(s, 0.6);
    thump(s, b, 0, 150 * k, 95 * k, 0.05, 1);
    tick(s, b, 0, 0.012, 900, 0.6, 'lp', 0.7);
    modes(s, b, [[320 * k, 0.09, 0.35], [710 * k, 0.06, 0.25], [1450 * k, 0.03, 0.12]]);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.06);
  },
});

defineSound('crate.throw', {
  ref: 0.6,
  range: 20,
  gain: 0.35,
  make: (s) => {
    const b = buf(s, 0.45);
    pink(s, b, (t) => Math.sin(Math.PI * Math.min(1, t / 0.4)) ** 2);
    sweep(s, b, 'bp', (t) => 500 + 2000 * t / 0.4, 1.5);
    return edges(s, normalize(dc(s, b), 0.7), 0.01, 0.05);
  },
});
