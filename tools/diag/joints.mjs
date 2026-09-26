// Joint self-check: drives the astronaut through a movement script (idle, walk, run, jump,
// crouch, turn, draw weapon, aim up/down, fire) and records every joint each frame:
// angles vs rest on the model axes (flex / twist / side) and world angular speed.
// Fails (exit 1) if any joint leaves the EVA-suit range of motion or pops (speed spike).
// Writes tools/diag/out/joints.json (full time series) and joints_worst.txt, plus a snapshot
// with the axis gizmos. Needs a running server (npm run dev). See docs/DIAGNOSTICS.md.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const pw = await import('playwright').catch(() => import('/opt/node22/lib/node_modules/playwright/index.mjs'));
const OUT = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(OUT, { recursive: true });
const browser = await pw.chromium.launch({ args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 640, height: 400 } });
page.on('pageerror', (e) => console.log('pageerror:', e.message));
await page.goto(`${process.env.GAME_URL ?? 'http://localhost:3000'}/?manual&offline`);
await page.fill('#name', 'diag');
await page.click('button[type=submit]');
await page.waitForFunction(() => document.querySelector('.menu')?.classList.contains('hidden'), null, { timeout: 180000 });
await page.evaluate(() => (document.querySelector('#ui').style.display = 'none'));

// Each frame = 1/30 s of game time (software GL in headless Chromium is slow, ~1 s per frame).
// [name, frames, setup, perFrame?] — run in the page with g = game, d = game.debug;
// perFrame also gets t (0→1 through the phase) and f (frame count) so inputs ramp like a player's
const script = [
  ['idle', 20, 'g.me.setArmed(false)'],
  ['walk', 30, 'd.input.setKey("KeyW", true)'],
  ['run', 30, 'd.input.setKey("ShiftLeft", true)'],
  ['stop', 20, 'd.input.setKey("KeyW", false); d.input.setKey("ShiftLeft", false)'],
  ['jump', 45, 'd.input.setKey("Space", true)'],
  ['land', 20, 'd.input.setKey("Space", false)'],
  ['crouch', 25, 'd.input.setKey("KeyC", true)'],
  ['stand', 20, 'd.input.setKey("KeyC", false)'],
  ['turn', 20, '', 'd.controller.yaw += 1.2 / f'],
  ['draw', 25, 'g.me.setArmed(true)'],
  ['aim-up', 20, 'g.__p0 = d.controller.pitch', 'd.controller.pitch = g.__p0 + (0.6 - g.__p0) * Math.min(1, t * 2)'],
  ['aim-down', 20, 'g.__p0 = d.controller.pitch', 'd.controller.pitch = g.__p0 + (-0.6 - g.__p0) * Math.min(1, t * 2)'],
  ['aim-level', 15, 'g.__p0 = d.controller.pitch', 'd.controller.pitch = g.__p0 * (1 - Math.min(1, t * 2))'],
  ['fire', 30, 'g.tryFire()'],
  ['walk-armed', 30, 'd.input.setKey("KeyW", true)'],
  ['holster', 25, 'd.input.setKey("KeyW", false); g.me.setArmed(false)'],
];
const series = [];
// worst value per bone and axis, and the phase where it happened
const worst = {};
for (const [name, frames, setup, perFrame = ''] of script) {
  const rows = await page.evaluate(({ frames, setup, perFrame }) => {
    const g = window.game, d = g.debug;
    g.jointsRecording = true;
    new Function('g', 'd', setup)(g, d);
    const each = new Function('g', 'd', 't', 'f', perFrame);
    const rows = [];
    for (let i = 0; i < frames; i++) {
      g.step(1, 1 / 60);
      rows.push(g.joints.last.map((j) => [j.bone, j.flex, j.twist, j.side, j.speed, j.violations.join('|')]));
    }
    return rows;
  }, { frames, setup, perFrame });
  series.push({ phase: name, frames: rows });
  for (const frame of rows)
    for (const [bone, flex, twist, side, speed] of frame) {
      worst[bone] ??= {};
      for (const [k, v] of [['flex', Math.abs(flex)], ['twist', Math.abs(twist)], ['side', Math.abs(side)], ['speed', speed]])
        if (!(worst[bone][k]?.v >= v)) worst[bone][k] = { v, phase: name };
    }
}
const summary = await page.evaluate(() => window.game.joints.summary());
// snapshot with axis gizmos and joint panel
await page.evaluate(() => {
  const g = window.game, d = g.debug;
  const ui = document.querySelector('#ui');
  ui.style.display = '';
  for (const el of ui.querySelectorAll(':scope > *')) if (!el.classList.contains('joints-panel')) el.style.visibility = 'hidden';
  window.dispatchEvent(new KeyboardEvent('keydown', { code: 'F6' }));
  if (d.rig.mode !== 'third') d.rig.toggle();
  g.debugOrbit = true;
  d.rig.orbit = 2.4;
  d.rig.zoom = 2.2;
  g.me.setArmed(true);
  g.step(30);
});
await page.setViewportSize({ width: 1280, height: 720 });
await page.evaluate(() => window.game.step(2));
await page.screenshot({ path: join(OUT, 'joints_gizmos.png') });
await browser.close();

writeFileSync(join(OUT, 'joints.json'), JSON.stringify({ axes: 'flex=X twist=Y side=Z (model axes, degrees vs rest); speed=world °/s', series }, null, 0));
const lines = ['ejes: flex=X twist=Y side=Z (swing-twist, grados vs reposo) · °/s = velocidad angular local', 'bone        flex  twist  side  °/s   estado'];
for (const s of summary) lines.push(`${s.bone.padEnd(11)}${String(s.flex).padStart(5)}${String(s.twist).padStart(7)}${String(s.side).padStart(6)}${String(s.speed).padStart(6)}   ${s.ok ? 'OK' : 'FUERA: ' + s.bad.join('; ')}`);
lines.push('', 'peor valor por hueso y eje (fase):');
for (const [bone, w] of Object.entries(worst)) lines.push(`  ${bone.padEnd(10)} ${['flex', 'twist', 'side', 'speed'].map((k) => `${k} ${Math.round(w[k].v)} (${w[k].phase})`).join('  ')}`);
writeFileSync(join(OUT, 'joints_worst.txt'), lines.join('\n'));
console.log(lines.join('\n'));
const bad = summary.filter((s) => !s.ok).length;
console.log(bad ? `\n${bad} articulación(es) fuera de rango` : '\ntodas las articulaciones dentro de rango');
process.exit(bad ? 1 : 0);
