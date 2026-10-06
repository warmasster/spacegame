// Mounted weapons (shared/items/mounts.ts): what they sound like from their mount, carried by the
// host's structure to whoever is aboard and by the air (or nothing) outside.

import { defineSound } from '../bank';
import { ad, add, buf, dc, edges, modes, noise, normalize, pink, sweep, thump, tick } from '../dsp';

// mini-missile off the rail: the clamp lets go with a bang through the mount, the motor lights
defineSound('turret.fire', {
  ref: 1,
  range: 400,
  gain: 0.95,
  variants: 3,
  jitter: 0.05,
  make: (s) => {
    const T = 1.1;
    const b = buf(s, T);
    thump(s, b, 0, 70, 35, 0.07, 1);
    const c = buf(s, 0.03);
    noise(s, c, ad(0.0003, 0.005));
    add(s, b, c, 0.9);
    const w = buf(s, T);
    pink(s, w, ad(0.006, 0.25));
    sweep(s, w, 'bp', (t) => 500 + 3400 * Math.exp(-t / 0.18), 0.8);
    add(s, b, w, 1);
    // the rail clamp and the next round sliding into the tube
    tick(s, b, 0.0, 0.004, 1800, 0.4, 'bp', 1);
    modes(s, b, [[620, 0.12, 0.25], [1450, 0.06, 0.12]], 0.35);
    return edges(s, normalize(dc(s, b), 0.95), 0.0003, 0.15);
  },
});
