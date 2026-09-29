// Machinery and propulsion: what the modules' cues name (shared/ship/modules/*: `sounds()`).
// Loops are exact (tones fitted to the loop, noise blended at the seam) and meant to be played
// faster or slower (`pitch`: spool, rpm, flow). Bigger machines of the same kind play them lower.

import { defineSound } from '../bank';
import { ad, add, brown, buf, crackle, dc, edges, filter, fit, loopOf, modes, noise, normalize, pink, saturate, sweep, thump, tick, tone, win, wobble } from '../dsp';

// ------------------------------------------------------------------------------------------------
// Loops: machines running
// ------------------------------------------------------------------------------------------------

// electrical hum: transformers, inverters, anything powered (the default voice of a running machine)
defineSound('mach.hum', {
  loop: true,
  ref: 0.8,
  range: 30,
  gain: 0.2,
  make: (s) => {
    const L = 2;
    return loopOf(
      s,
      L,
      (b) => {
        pink(s, b, 0.05);
        filter(s, b, 'bp', 2200, 0.6);
      },
      (b) => {
        const am = (t: number) => 0.9 + 0.1 * Math.sin(2 * Math.PI * fit(3, L) * t);
        for (const [f, a] of [[100, 0.5], [200, 0.28], [300, 0.3], [400, 0.12], [600, 0.08], [1000, 0.03]] as const) tone(s, b, fit(f, L), (t) => a * am(t));
      },
      0.8,
    );
  },
});

// the reactor: a deep beating hum with the coolant rushing through its loop
defineSound('mach.reactor', {
  loop: true,
  ref: 2,
  range: 80,
  gain: 0.55,
  make: (s) => {
    const L = 4;
    const flow = wobble(s, L, 1.5);
    return loopOf(
      s,
      L,
      (b) => {
        pink(s, b, (t) => 0.22 * (0.7 + 0.3 * flow(t)));
        filter(s, b, 'bp', 900, 0.7);
        const r = buf(s, b.length / s.sr);
        brown(s, r, 0.3);
        filter(s, r, 'lp', 120);
        add(s, b, r);
      },
      (b) => {
        const throb = (t: number) => 0.85 + 0.15 * Math.sin(2 * Math.PI * fit(0.5, L) * t);
        for (const [f, a] of [[45, 0.6], [45.5, 0.3], [90, 0.35], [135, 0.22], [180, 0.12], [270, 0.05]] as const) tone(s, b, fit(f, L), (t) => a * throb(t));
      },
    );
  },
});

// a turbine (the APU): the blade whine over its roar; played from 0.3× (spooling) to 1×
defineSound('mach.turbine', {
  loop: true,
  ref: 1.5,
  range: 160,
  gain: 0.6,
  make: (s) => {
    const L = 2;
    const fl = wobble(s, L, 6);
    return loopOf(
      s,
      L,
      (b) => {
        pink(s, b, (t) => 0.5 * (0.85 + 0.15 * fl(t)));
        filter(s, b, 'lp', 2200);
        filter(s, b, 'ls', 200, 0.7, 6);
        noise(s, b, 0.05);
      },
      (b) => {
        for (const [f, a] of [[330, 0.3], [660, 0.18], [990, 0.1], [3630, 0.09], [7590, 0.04]] as const) tone(s, b, fit(f, L), a);
      },
    );
  },
});

// a pump: its motor and the pulses of the fluid it moves
defineSound('mach.pump', {
  loop: true,
  ref: 0.8,
  range: 35,
  gain: 0.32,
  make: (s) => {
    const L = 2;
    return loopOf(
      s,
      L,
      (b) => {
        const n = 14 * (b.length / s.sr / L);
        for (let i = 0; i < n; i++) thump(s, b, i / 7 + s.rng.range(0, 0.004), 95, 70, 0.035, 0.5);
        const w = buf(s, b.length / s.sr);
        noise(s, w, (t) => 0.2 * (0.5 + 0.5 * Math.cos(2 * Math.PI * 7 * t)));
        filter(s, w, 'bp', 700, 0.8);
        add(s, b, w);
      },
      (b) => {
        tone(s, b, fit(240, L), 0.08);
        tone(s, b, fit(480, L), 0.04);
      },
    );
  },
});

// ventilation fan: air through a duct and the blades' tone
defineSound('mach.fan', {
  loop: true,
  ref: 1.5,
  range: 22,
  gain: 0.16,
  make: (s) => {
    const L = 3;
    const w = wobble(s, L, 0.8);
    return loopOf(
      s,
      L,
      (b) => {
        pink(s, b, (t) => 0.6 * (0.85 + 0.15 * w(t)));
        filter(s, b, 'bp', 650, 0.45);
        filter(s, b, 'hs', 3000, 0.7, -8);
      },
      (b) => {
        tone(s, b, fit(147, L), (t) => 0.05 * (0.8 + 0.2 * Math.sin(2 * Math.PI * fit(4, L) * t)));
        tone(s, b, fit(294, L), 0.02);
      },
    );
  },
});

// oxygen generator: an electrolysis cell humming and bubbling
defineSound('mach.electrolyser', {
  loop: true,
  ref: 0.8,
  range: 22,
  gain: 0.2,
  make: (s) => {
    const L = 3;
    return loopOf(
      s,
      L,
      (b) => {
        const T = b.length / s.sr;
        for (let i = 0; i < T * 22; i++) {
          const at = s.rng.range(0, T - 0.05);
          const f = s.rng.range(420, 1100);
          tone(s, b, (t) => f * (1 + 2 * t), (t) => 0.12 * Math.exp(-t / 0.012), 'sine', at, 8000, 0.04);
        }
      },
      (b) => {
        tone(s, b, fit(120, L), 0.25);
        tone(s, b, fit(240, L), 0.1);
        tone(s, b, fit(360, L), 0.06);
      },
    );
  },
});

// radar: the antenna's drive and the sweep of its rotation
defineSound('mach.radar', {
  loop: true,
  ref: 0.8,
  range: 30,
  gain: 0.25,
  make: (s) => {
    const L = 2.4;
    return loopOf(
      s,
      L,
      (b) => {
        noise(s, b, (t) => 0.15 * (0.5 + 0.5 * Math.sin(2 * Math.PI * (2 / L) * t)) ** 2);
        filter(s, b, 'bp', 1400, 0.9);
      },
      (b) => {
        tone(s, b, fit(60, L), 0.3);
        tone(s, b, fit(780, L), 0.05);
        tone(s, b, fit(1560, L), 0.03);
      },
    );
  },
});

// servo motor: turrets, loaders, radiators, anything small that drives a gear
defineSound('mach.motor', {
  loop: true,
  ref: 0.8,
  range: 30,
  gain: 0.3,
  make: (s) => {
    const L = 1.5;
    return loopOf(
      s,
      L,
      (b) => {
        noise(s, b, 0.12);
        filter(s, b, 'bp', 2600, 0.7);
      },
      (b) => {
        tone(s, b, fit(118, L), 0.35, 'saw', 0, 3000);
        tone(s, b, fit(885, L), 0.08);
        tone(s, b, fit(1770, L), 0.04);
      },
    );
  },
});

// inertial compensator: a slow throb under a thin shimmer
defineSound('mach.grav', {
  loop: true,
  ref: 1.5,
  range: 40,
  gain: 0.42,
  make: (s) => {
    const L = 4;
    return loopOf(
      s,
      L,
      (b) => {
        noise(s, b, 0.04);
        filter(s, b, 'bp', 6200, 2);
      },
      (b) => {
        const beat = (t: number) => 0.55 + 0.45 * Math.sin(2 * Math.PI * fit(1.5, L) * t) ** 2;
        tone(s, b, fit(33, L), (t) => 0.8 * beat(t));
        tone(s, b, fit(66, L), (t) => 0.35 * beat(t));
        tone(s, b, fit(99, L), 0.08);
      },
    );
  },
});

// air recovery compressor: pistons and their valves
defineSound('mach.compressor', {
  loop: true,
  ref: 1,
  range: 40,
  gain: 0.42,
  make: (s) => {
    const L = 2;
    return loopOf(
      s,
      L,
      (b) => {
        const T = b.length / s.sr;
        for (let t = 0; t < T; t += 0.08) {
          thump(s, b, t, 75, 55, 0.03, 0.6);
          tick(s, b, t + 0.03, 0.02, 3500, 0.15, 'hp', 0.7);
        }
      },
      (b) => {
        tone(s, b, fit(150, L), 0.18, 'saw', 0, 2000);
      },
    );
  },
});

// a machine too hot: coolant knocking and boiling in the pipes
defineSound('mach.overheat', {
  loop: true,
  ref: 1.5,
  range: 45,
  gain: 0.5,
  make: (s) =>
    loopOf(
      s,
      3,
      (b) => {
        const T = b.length / s.sr;
        for (let i = 0; i < T * 5; i++) {
          const at = s.rng.range(0, T - 0.1);
          thump(s, b, at, s.rng.range(160, 260), 120, 0.03, s.rng.range(0.3, 0.8));
          modes(s, b, [[s.rng.range(700, 1100), 0.05, 0.2]], at);
        }
        crackle(s, b, 90, 0.35, 0.004);
        const h = buf(s, T);
        noise(s, h, 0.08);
        filter(s, h, 'hp', 2500);
        add(s, b, h);
      },
      null,
    ),
});

// a wreck with power and propellant reaching it: spraying and arcing
defineSound('mach.sputter', {
  loop: true,
  ref: 1,
  range: 40,
  gain: 0.5,
  make: (s) =>
    loopOf(
      s,
      2,
      (b) => {
        const w = wobble(s, b.length / s.sr, 5);
        noise(s, b, (t) => 0.35 * (0.4 + 0.6 * w(t)));
        filter(s, b, 'hp', 1800);
        crackle(s, b, 70, 0.8, 0.0015);
      },
      (b) => tone(s, b, fit(100, 2), 0.06, 'square', 0, 2000),
    ),
});

// ------------------------------------------------------------------------------------------------
// Loops: propulsion
// ------------------------------------------------------------------------------------------------

// a rocket engine burning: the roar with its grit and flutter; pitch rises with the throttle
defineSound('eng.roar', {
  loop: true,
  ref: 6,
  range: 4000,
  gain: 1,
  make: (s) => {
    const L = 3;
    const fl = wobble(s, L, 9);
    const sl = wobble(s, L, 1.3);
    return loopOf(
      s,
      L,
      (b) => {
        brown(s, b, (t) => 0.9 * (0.75 + 0.25 * sl(t)));
        const p = buf(s, b.length / s.sr);
        pink(s, p, (t) => 0.55 * (0.75 + 0.25 * fl(t)));
        filter(s, p, 'lp', 2600);
        add(s, b, p);
        const g = buf(s, b.length / s.sr);
        crackle(s, g, 160, 0.5, 0.0012);
        filter(s, g, 'lp', 4500);
        add(s, b, g);
        saturate(b, 1.4);
      },
      (b) => tone(s, b, fit(52, L), (t) => 0.15 * (0.7 + 0.3 * sl(t))),
    );
  },
});

// turbopumps winding up (pitch follows the spool)
defineSound('eng.spool', {
  loop: true,
  ref: 2,
  range: 300,
  gain: 0.5,
  make: (s) => {
    const L = 2;
    return loopOf(
      s,
      L,
      (b) => {
        noise(s, b, 0.2);
        filter(s, b, 'bp', 3000, 0.7);
      },
      (b) => {
        tone(s, b, fit(1180, L), 0.25);
        tone(s, b, fit(2360, L), 0.12);
        tone(s, b, fit(590, L), 0.1);
      },
    );
  },
});

// overdrive: mass injection makes the flame run rough and pop
defineSound('eng.crackle', {
  loop: true,
  ref: 5,
  range: 2500,
  gain: 0.75,
  make: (s) =>
    loopOf(
      s,
      2,
      (b) => {
        crackle(s, b, 38, 1, 0.006);
        filter(s, b, 'bp', 1400, 0.5);
        const p = buf(s, b.length / s.sr);
        crackle(s, p, 12, 1, 0.02);
        filter(s, p, 'lp', 400);
        add(s, b, p, 1.2);
      },
      null,
    ),
});

// VTOL lift pad: a jet blowing down, hiss over a roar
defineSound('eng.lift', {
  loop: true,
  ref: 3,
  range: 2000,
  gain: 0.85,
  make: (s) => {
    const L = 2;
    const fl = wobble(s, L, 7);
    return loopOf(
      s,
      L,
      (b) => {
        pink(s, b, (t) => 0.7 * (0.8 + 0.2 * fl(t)));
        filter(s, b, 'bp', 1100, 0.4);
        const r = buf(s, b.length / s.sr);
        brown(s, r, 0.6);
        filter(s, r, 'lp', 220);
        add(s, b, r);
        const h = buf(s, b.length / s.sr);
        noise(s, h, 0.12);
        filter(s, h, 'hp', 5000);
        add(s, b, h);
      },
      null,
    );
  },
});

// RCS: cold gas out of a small nozzle
defineSound('rcs.hiss', {
  loop: true,
  ref: 1,
  range: 500,
  gain: 0.55,
  make: (s) => {
    const fl = wobble(s, 1, 14);
    return loopOf(
      s,
      1,
      (b) => {
        noise(s, b, (t) => 0.6 * (0.85 + 0.15 * fl(t)));
        filter(s, b, 'hp', 1600);
        filter(s, b, 'lp', 9000);
        filter(s, b, 'peak', 3200, 1, 4);
      },
      null,
    );
  },
});

// a rocket's small motor in flight
defineSound('rocket.motor', {
  loop: true,
  ref: 1,
  range: 400,
  gain: 0.6,
  make: (s) =>
    loopOf(
      s,
      1,
      (b) => {
        pink(s, b, 0.7);
        filter(s, b, 'bp', 1500, 0.5);
        crackle(s, b, 120, 0.4, 0.002);
      },
      null,
    ),
});

// ------------------------------------------------------------------------------------------------
// Loops: mechanisms (shared/ship/modules/movers.ts MOVER_SOUNDS)
// ------------------------------------------------------------------------------------------------

// a sliding door or hatch: its motor and the rollers in their track
defineSound('mover.slide', {
  loop: true,
  ref: 1,
  range: 35,
  gain: 0.38,
  make: (s) => {
    const L = 1.6;
    const r = wobble(s, L, 22);
    return loopOf(
      s,
      L,
      (b) => {
        noise(s, b, (t) => 0.3 * (0.5 + 0.5 * r(t)));
        filter(s, b, 'bp', 1300, 0.8);
      },
      (b) => {
        tone(s, b, fit(176, L), 0.3, 'saw', 0, 2500);
        tone(s, b, fit(1410, L), 0.04);
      },
    );
  },
});

// hydraulics (ramp, gear): the pump's whine, the fluid, the rams groaning under load
defineSound('mover.hydraulic', {
  loop: true,
  ref: 1.5,
  range: 90,
  gain: 0.55,
  make: (s) => {
    const L = 2;
    const g = wobble(s, L, 1.5);
    return loopOf(
      s,
      L,
      (b) => {
        noise(s, b, 0.22);
        filter(s, b, 'bp', 2200, 0.9);
        const r = buf(s, b.length / s.sr);
        brown(s, r, (t) => 0.5 * (0.6 + 0.4 * g(t)));
        filter(s, r, 'lp', 180);
        add(s, b, r);
      },
      (b) => {
        tone(s, b, fit(520, L), 0.14);
        tone(s, b, fit(1040, L), 0.06);
        tone(s, b, fit(72, L), (t) => 0.3 * (0.6 + 0.4 * g(t)));
      },
    );
  },
});

// roller shutters: a chain rattling over its sprocket
defineSound('mover.roller', {
  loop: true,
  ref: 1.5,
  range: 40,
  gain: 0.4,
  make: (s) => {
    const L = 1.2;
    return loopOf(
      s,
      L,
      (b) => {
        const T = b.length / s.sr;
        for (let t = 0; t < T; t += 1 / 24) tick(s, b, t + s.rng.range(0, 0.004), 0.004, 2400, s.rng.range(0.2, 0.4), 'bp', 2);
      },
      (b) => tone(s, b, fit(140, L), 0.25, 'saw', 0, 2000),
    );
  },
});

defineSound('mover.motor', { like: 'mach.motor', pitch: 1.2, gain: 1.1 });

// ------------------------------------------------------------------------------------------------
// One-shots: machines changing state
// ------------------------------------------------------------------------------------------------

// end of a mechanism's travel: a door or shutter stops
defineSound('mover.stop', {
  ref: 1,
  range: 35,
  gain: 0.5,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.55);
    thump(s, b, 0, 130, 90, 0.05, 0.8);
    tick(s, b, 0, 0.006, 1800, 0.5, 'bp', 0.8);
    modes(s, b, [[340, 0.12, 0.35], [780, 0.08, 0.3], [1530, 0.05, 0.2], [2900, 0.02, 0.1]]);
    return edges(s, normalize(dc(s, b), 0.9));
  },
});

// hydraulic lock: the ramp down on the ground, the gear locked
defineSound('mover.lock', {
  ref: 1.5,
  range: 90,
  gain: 0.75,
  variants: 2,
  make: (s) => {
    const b = buf(s, 0.9);
    thump(s, b, 0, 90, 55, 0.09, 1);
    tick(s, b, 0, 0.01, 900, 0.5, 'lp', 0.7);
    modes(s, b, [[150, 0.25, 0.4], [420, 0.18, 0.3], [960, 0.1, 0.2], [1880, 0.05, 0.1]]);
    const h = buf(s, 0.5);
    noise(s, h, ad(0.01, 0.12));
    filter(s, h, 'hp', 2500);
    add(s, b, h, 0.25, 0.08);
    return edges(s, normalize(dc(s, b), 0.95));
  },
});

// reactor start: relays close, the pumps come up, the core hum grows and a heavy clunk
defineSound('rx.start', {
  ref: 2,
  range: 80,
  gain: 0.7,
  make: (s) => {
    const T = 3;
    const b = buf(s, T);
    for (const at of [0, 0.22, 0.47]) {
      tick(s, b, at, 0.004, 2600, 0.6, 'hp', 0.7);
      modes(s, b, [[1450, 0.04, 0.35], [3100, 0.02, 0.2]], at);
    }
    tone(s, b, (t) => 25 + 20 * Math.min(1, t / 2.4), (t) => 0.5 * Math.min(1, Math.max(0, t - 0.5) / 2) * win(T, 0.01, 0.3)(t));
    tone(s, b, (t) => 50 + 40 * Math.min(1, t / 2.4), (t) => 0.2 * Math.min(1, Math.max(0, t - 0.5) / 2) * win(T, 0.01, 0.3)(t));
    tone(s, b, (t) => 200 + 180 * Math.min(1, t / 2), (t) => 0.1 * Math.min(1, Math.max(0, t - 0.6) / 1.5) * win(T, 0.01, 0.3)(t));
    thump(s, b, 2.5, 80, 50, 0.12, 0.8);
    return edges(s, normalize(dc(s, b), 0.9), 0.001, 0.2);
  },
});

// reactor online: a deep thunk and the hum settling in
defineSound('rx.online', {
  ref: 2,
  range: 80,
  gain: 0.75,
  make: (s) => {
    const b = buf(s, 1.8);
    thump(s, b, 0, 70, 45, 0.25, 1);
    modes(s, b, [[180, 0.6, 0.35], [410, 0.4, 0.25], [870, 0.25, 0.15]]);
    tone(s, b, 45, (t) => 0.4 * Math.min(1, t / 0.3) * Math.exp(-t / 1.2));
    return edges(s, normalize(dc(s, b), 0.9), 0.001, 0.2);
  },
});

// SCRAM: the control rods slam in, the hydraulics dump
defineSound('rx.scram', {
  ref: 2.5,
  range: 120,
  gain: 1,
  make: (s) => {
    const b = buf(s, 2.4);
    thump(s, b, 0, 95, 38, 0.3, 1.2);
    tick(s, b, 0, 0.012, 1500, 0.9, 'lp', 0.7);
    modes(s, b, [[220, 1.2, 0.4], [530, 0.8, 0.3], [1240, 0.5, 0.2], [2100, 0.3, 0.12]]);
    const h = buf(s, 1.6);
    noise(s, h, ad(0.02, 0.45));
    filter(s, h, 'hp', 1500);
    add(s, b, h, 0.35, 0.05);
    return edges(s, normalize(dc(s, b), 0.95), 0.0005, 0.2);
  },
});

// reactor off: the hum winds down
defineSound('rx.down', {
  ref: 2,
  range: 80,
  gain: 0.55,
  make: (s) => {
    const T = 3.5;
    const b = buf(s, T);
    const f = (t: number) => 20 + 25 * Math.exp(-t / 1.1);
    tone(s, b, f, (t) => 0.6 * Math.exp(-t / 1.3));
    tone(s, b, (t) => 2 * f(t), (t) => 0.3 * Math.exp(-t / 1));
    const n = buf(s, T);
    pink(s, n, (t) => 0.25 * Math.exp(-t / 0.9));
    filter(s, n, 'bp', 800, 0.7);
    add(s, b, n);
    return edges(s, normalize(dc(s, b), 0.8), 0.02, 0.3);
  },
});

// turbine start: the starter whines up, the fuel lights with a whump, the turbine takes over
defineSound('turbine.start', {
  ref: 1.5,
  range: 160,
  gain: 0.65,
  make: (s) => {
    const T = 3;
    const b = buf(s, T);
    tone(s, b, (t) => 200 + 450 * Math.min(1, t / 1.4), (t) => 0.2 * Math.min(1, t / 0.2) * Math.exp(-Math.max(0, t - 1.4) / 0.4), 'saw', 0, 4000);
    const w = buf(s, 1.6);
    noise(s, w, ad(0.12, 0.5));
    sweep(s, w, 'lp', (t) => 200 + 1800 * Math.min(1, t / 0.2));
    add(s, b, w, 0.9, 1.2);
    tone(s, b, (t) => 300 + 700 * Math.min(1, t / 1.8), (t) => 0.2 * Math.min(1, t / 0.4) * win(1.8, 0.01, 0.3)(t), 'sine', 1.2);
    return edges(s, normalize(dc(s, b), 0.9), 0.005, 0.3);
  },
});

// turbine off: the whine falls away
defineSound('turbine.down', {
  ref: 1.5,
  range: 160,
  gain: 0.55,
  make: (s) => {
    const T = 4;
    const b = buf(s, T);
    const f = (t: number) => 150 + 750 * Math.exp(-t / 1.1);
    tone(s, b, f, (t) => 0.3 * Math.exp(-t / 1.4));
    tone(s, b, (t) => 11 * f(t), (t) => 0.06 * Math.exp(-t / 0.8));
    const r = buf(s, T);
    pink(s, r, (t) => 0.4 * Math.exp(-t / 0.7));
    filter(s, r, 'lp', 1500);
    add(s, b, r);
    return edges(s, normalize(dc(s, b), 0.8), 0.02, 0.3);
  },
});

// turbine failure: a backfire and it sputters out
defineSound('turbine.fail', {
  ref: 1.5,
  range: 200,
  gain: 0.8,
  make: (s) => {
    const b = buf(s, 1.6);
    thump(s, b, 0, 70, 40, 0.12, 1);
    tick(s, b, 0, 0.05, 1200, 0.9, 'lp', 0.6);
    for (const at of [0.35, 0.6, 0.95]) {
      thump(s, b, at, 90, 60, 0.05, 0.4);
      tick(s, b, at, 0.03, 900, 0.35, 'lp', 0.7);
    }
    tone(s, b, (t) => 600 * Math.exp(-t / 0.5) + 80, (t) => 0.15 * Math.exp(-t / 0.5));
    return edges(s, normalize(dc(s, b), 0.95), 0.0005, 0.2);
  },
});

// engine ignition: the igniter snaps, the propellant lights with a whoomph
defineSound('eng.ignite', {
  ref: 4,
  range: 2000,
  gain: 0.95,
  make: (s) => {
    const b = buf(s, 2.2);
    for (let i = 0; i < 8; i++) tick(s, b, i / 12, 0.003, 3200, 0.3, 'bp', 1.5);
    const w = buf(s, 1.6);
    brown(s, w, ad(0.15, 0.5));
    noise(s, w, ad(0.1, 0.25));
    sweep(s, w, 'lp', (t) => 250 + 2200 * Math.min(1, t / 0.25));
    add(s, b, w, 1.2, 0.6);
    thump(s, b, 0.62, 60, 35, 0.2, 0.9);
    saturate(b, 1.3);
    return edges(s, normalize(dc(s, b), 0.95), 0.001, 0.3);
  },
});

// engine cut-off: the valves shut and the flame dies
defineSound('eng.cutoff', {
  ref: 4,
  range: 2000,
  gain: 0.75,
  make: (s) => {
    const b = buf(s, 1.8);
    thump(s, b, 0, 140, 90, 0.05, 0.6);
    modes(s, b, [[480, 0.1, 0.3], [1100, 0.06, 0.2]]);
    const r = buf(s, 1.8);
    brown(s, r, (t) => 0.8 * Math.exp(-t / 0.35));
    pink(s, r, (t) => 0.4 * Math.exp(-t / 0.25));
    sweep(s, r, 'lp', (t) => 2400 * Math.exp(-t / 0.4) + 150);
    add(s, b, r);
    return edges(s, normalize(dc(s, b), 0.9), 0.001, 0.3);
  },
});

// an RCS pulse: the solenoid bangs open and a puff of gas
defineSound('rcs.pop', {
  ref: 1,
  range: 400,
  gain: 0.7,
  variants: 3,
  jitter: 0.08,
  make: (s) => {
    const b = buf(s, 0.35);
    tick(s, b, 0, 0.003, 3000, 0.8, 'hp', 0.7);
    thump(s, b, 0, 160, 100, 0.03, 0.8);
    modes(s, b, [[1210, 0.03, 0.3], [2680, 0.015, 0.2]]);
    const g = buf(s, 0.3);
    noise(s, g, ad(0.004, 0.06));
    filter(s, g, 'hp', 1100);
    add(s, b, g, 0.6, 0.004);
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.05);
  },
});

// relays: a bank of them clacking
defineSound('relay.clack', {
  ref: 0.8,
  range: 30,
  gain: 0.45,
  make: (s) => {
    const b = buf(s, 0.3);
    for (const at of [0, 0.035, 0.06]) {
      tick(s, b, at, 0.003, 3000, 0.7, 'hp', 0.7);
      modes(s, b, [[1600 + s.rng.range(-200, 200), 0.03, 0.4], [3500, 0.015, 0.2]], at);
    }
    return edges(s, normalize(dc(s, b), 0.9), 0.0003, 0.04);
  },
});

// the ship goes dark: relays drop, the inverters wind down
defineSound('power.down', {
  ref: 1.5,
  range: 40,
  gain: 0.6,
  make: (s) => {
    const T = 2;
    const b = buf(s, T);
    for (const at of [0, 0.04, 0.09, 0.16]) {
      tick(s, b, at, 0.004, 2500, 0.6, 'hp', 0.7);
      modes(s, b, [[1300, 0.04, 0.3]], at);
    }
    tone(s, b, (t) => 90 + 700 * Math.exp(-t / 0.45), (t) => 0.25 * Math.exp(-t / 0.7) * Math.min(1, t / 0.05), 'tri', 0.05, 5000);
    tone(s, b, 100, (t) => 0.3 * Math.exp(-t / 0.4));
    return edges(s, normalize(dc(s, b), 0.85), 0.0005, 0.2);
  },
});

// power back: relays close, the inverters sing up
defineSound('power.up', {
  ref: 1.5,
  range: 40,
  gain: 0.55,
  make: (s) => {
    const T = 1.4;
    const b = buf(s, T);
    for (const at of [0, 0.05, 0.12]) {
      tick(s, b, at, 0.004, 2500, 0.6, 'hp', 0.7);
      modes(s, b, [[1500, 0.04, 0.3]], at);
    }
    tone(s, b, (t) => 200 + 600 * (1 - Math.exp(-t / 0.3)), (t) => 0.18 * Math.min(1, t / 0.1) * Math.exp(-t / 0.6), 'tri', 0.1, 5000);
    thump(s, b, 0.12, 110, 80, 0.06, 0.5);
    return edges(s, normalize(dc(s, b), 0.85), 0.0005, 0.2);
  },
});
