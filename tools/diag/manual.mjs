// Ship manual (M): opens it offline and takes a screenshot of each part (guide, plan, live strips,
// circuits, alarms, the controls reference, a search). Fails on page errors or on a manual that
// renders without its generated parts.
//
//   npm run diag:manual            (GAME_URL=http://localhost:3000 by default)

import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import pw from 'playwright';
import { boot } from './progress.mjs';

const OUT = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(OUT, { recursive: true });
const url = process.env.GAME_URL ?? 'http://localhost:3000';

const browser = await pw.chromium.launch({ args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 1400, height: 860 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text().slice(0, 300)));
await boot(page, `${url}/?manual&offline`, { quality: 'low', label: 'manual' });

await page.evaluate(() => {
  window.game.step(2, 1 / 30, false);
  window.game.hud.toggleManual(true);
});
const facts = await page.evaluate(() => {
  const m = document.querySelector('.mn');
  return {
    open: !!m && !m.classList.contains('hidden'),
    chapters: m.querySelectorAll('.mn-sec').length,
    cards: m.querySelectorAll('.mn-card').length,
    consoles: m.querySelectorAll('.mn-console').length,
    alarms: m.querySelectorAll('.mn-lampgrp').length,
    planConsoles: m.querySelectorAll('.pl-con').length,
    live: m.querySelectorAll('.mn-live .mn-chip, .mn-live .mn-bar').length,
  };
});
console.log('manual:', JSON.stringify(facts));

const shot = async (name, id) => {
  if (id) await page.evaluate((i) => document.getElementById(i)?.scrollIntoView({ block: 'start' }), id);
  await page.evaluate(() => window.game.hud.updateManual(performance.now() / 1000 + 10));
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(OUT, `manual_${name}.png`) });
  console.log(`  ✓ manual_${name}.png`);
};
await shot('start', 'mn-start');
await shot('plan', 'mn-ship');
await shot('power', 'mn-power');
await shot('alerts', 'mn-alerts');
await shot('controls', 'mn-consoles');
await page.fill('.mn-search input', 'reactor');
await shot('search');

await browser.close();
const fail = [];
if (!facts.open) fail.push('el manual no se abrió');
if (facts.cards < 40) fail.push(`solo ${facts.cards} fichas de mandos`);
if (!facts.planConsoles) fail.push('el plano no tiene consolas');
if (!facts.live) fail.push('no hay lecturas en vivo');
fail.push(...errors.map((e) => `error de página: ${e}`));
console.log(fail.length ? `FALLO\n  ${fail.join('\n  ')}` : 'todo en orden');
process.exit(fail.length ? 1 : 0);
