// Grasp self-check: loads the game headless, draws the weapon, walks/aims through several poses and
// measures each hand against its grip (palm gap, palm facing, finger crossing). Exits 1 on failure
// and saves close-up shots of the hands to tools/diag/out/grasp_*.png for visual review.
//
//   npm run diag:grasp        (needs a running server: npm run dev)
import { mkdirSync } from 'node:fs';
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

const poses = [
  { name: 'idle', pitch: -0.08, walk: 0 },
  { name: 'aim-up', pitch: 0.5, walk: 0 },
  { name: 'aim-down', pitch: -0.5, walk: 0 },
  { name: 'walk', pitch: 0, walk: 1 },
];
let fail = 0;
let n = 0;
const files = [];
for (const p of poses) {
  const rep = await page.evaluate(({ pitch, walk }) => {
    const g = window.game, d = g.debug;
    if (d.rig.mode !== 'third') d.rig.toggle();
    g.debugOrbit = true;
    d.controller.yaw = -Math.PI / 2;
    d.controller.pitch = pitch;
    d.input.setKey('KeyW', !!walk);
    g.me.setArmed(true);
    g.step(45);
    return g.me.graspReport();
  }, p);
  for (const [side, r] of Object.entries(rep)) {
    console.log(`${r.ok ? 'OK  ' : 'FAIL'} ${p.name.padEnd(9)} ${side}  gap ${String(r.gapCm).padStart(5)} cm  palm-facing ${String(r.palmFacingDeg).padStart(3)}°  finger-cross ${String(r.fingerCrossDeg).padStart(3)}°`);
    if (!r.ok) fail++;
  }
  // close-ups from the right and left front
  // yaw offset 0 = camera in front of the astronaut looking back at it
  for (const [part, az, el] of [['handR', 0.9, 0.2], ['handR', -2.2, 0.5], ['handL', 0.4, 0.1], ['handL', -1.3, 0.35]]) {
    await page.evaluate(([part, az, el]) => { window.game.focusCam = { part, az, el, dist: 0.45 }; window.game.step(2); }, [part, az, el]);
    const f = join(OUT, `grasp_${n++}_${p.name}_${part}.png`);
    await page.screenshot({ path: f });
    files.push(f);
  }
  await page.evaluate(() => (window.game.focusCam = null));
}
const { readFileSync } = await import('node:fs');
const sheet = await browser.newPage({ viewport: { width: 1280, height: 200 * Math.ceil(files.length / 4) } });
await sheet.setContent(`<body style="margin:0;display:flex;flex-wrap:wrap;width:1280px">${files.map((f) => `<img src="data:image/png;base64,${readFileSync(f).toString('base64')}" width=320 height=200>`).join('')}</body>`);
await sheet.screenshot({ path: join(OUT, 'grasp_sheet.png') });
await browser.close();
console.log(fail ? `${fail} grasp check(s) failed` : 'all grasp checks passed');
process.exit(fail ? 1 : 0);
