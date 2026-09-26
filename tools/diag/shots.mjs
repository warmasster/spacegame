// Automated screenshots for visual diagnosis (headless Chromium, software GL).
//
//   node tools/diag/shots.mjs terrain "x,z,yaw,pitch,height;..." [flags]   → free-camera terrain views
//   node tools/diag/shots.mjs pose    "orbit,zoom,walk,armed;..."   [flags] → astronaut from orbiting cameras
//
// Needs a running server (npm run dev) and Playwright: npm i -D playwright && npx playwright install chromium
// Flags are appended to the URL: &nobaked &nomorph &skirts &ao. Output: tools/diag/out/<mode>_<n>.png + a contact sheet.
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { boot, progress } from './progress.mjs';

const pw = await import('playwright').catch(() => import('/opt/node22/lib/node_modules/playwright/index.mjs'));
const [mode = 'terrain', spec = '', flags = ''] = process.argv.slice(2);
const OUT = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(OUT, { recursive: true });
const url = process.env.GAME_URL ?? 'http://localhost:3000';

const browser = await pw.chromium.launch({ args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 640, height: 400 } });
page.on('pageerror', (e) => console.log('pageerror:', e.message));
await boot(page, `${url}/?manual&offline${flags}`, { quality: 'high' });
await page.evaluate(() => (document.querySelector('#ui').style.display = 'none'));
const files = [];
let i = 0;
const views = spec.split(';').filter(Boolean);
const P = progress(mode, views.length + (views.length > 1 ? 1 : 0));
for (const v of spec.split(';').filter(Boolean)) {
  const n = v.split(',').map(Number);
  if (mode === 'terrain') {
    await page.evaluate((c) => (window.game.inspectCam = c), n);
    for (let k = 0; k < 8; k++) {
      await page.evaluate(() => window.game.step(1));
      await page.waitForTimeout(350);
    }
  } else {
    const [orbit = 3, zoom = 2.2, walk = 0, armed = 1] = n;
    await page.evaluate(([o, z, w, a]) => {
      const g = window.game, d = g.debug;
      if ((d.rig.mode === 'third') !== o >= -9) d.rig.toggle(); // orbit -10 = first person
      g.debugOrbit = true;
      d.rig.orbit = o;
      d.rig.zoom = z;
      d.controller.yaw = -Math.PI / 2;
      d.controller.pitch = -0.08;
      d.input.setKey('KeyW', !!w);
      g.me.setArmed(!!a);
    }, [orbit, zoom, walk, armed]);
    await page.evaluate(() => window.game.step(40));
  }
  const f = join(OUT, `${mode}_${i++}.png`);
  await page.screenshot({ path: f, timeout: 240000 });
  files.push(f);
  P.step(`vista ${v}`);
}
// contact sheet
if (files.length > 1) {
  const imgs = await Promise.all(files.map(async (f) => `data:image/png;base64,${(await import('node:fs')).readFileSync(f).toString('base64')}`));
  const sheet = await browser.newPage({ viewport: { width: 640 * Math.min(3, imgs.length), height: 400 * Math.ceil(imgs.length / 3) } });
  await sheet.setContent(`<body style="margin:0;display:flex;flex-wrap:wrap;width:${640 * Math.min(3, imgs.length)}px">${imgs.map((s) => `<img src="${s}" width=640 height=400>`).join('')}</body>`);
  await sheet.screenshot({ path: join(OUT, `${mode}_sheet.png`) });
  P.step('hoja de contactos');
}
await browser.close();
