// Ship over the network: two real clients against the running server (npm run dev). Checks that a
// control operated by A reaches B, that a rocket hit reported by A damages the hull for both
// (server-side blast), and that welding by A (with the welder in hand) repairs it for B.
// The server keeps its ship state between runs, so every check is relative. Exit 1 on failure.
//
//   node tools/diag/ship_net.mjs
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const pw = await import('playwright').catch(() => import('/opt/node22/lib/node_modules/playwright/index.mjs'));
const OUT = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(OUT, { recursive: true });
const url = process.env.GAME_URL ?? 'http://localhost:3000';
const browser = await pw.chromium.launch({ args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });

async function joinGame(name) {
  const page = await browser.newPage({ viewport: { width: 640, height: 400 } });
  page.on('pageerror', (e) => console.log(`[${name}] pageerror:`, e.message));
  await page.goto(`${url}/?manual`);
  await page.fill('#name', name);
  await page.selectOption('#quality', 'low');
  await page.click('button[type=submit]');
  await page.waitForFunction(() => document.querySelector('.menu')?.classList.contains('hidden'), null, { timeout: 240000 });
  await page.evaluate(() => {
    const g = window.game;
    window.diag = {
      w: (x, y, z) => g.ships[0].view.root.localToWorld(g.debug.camera.position.clone().set(x, y, z)),
    };
  });
  return page;
}

const report = [];
let failed = 0;
const check = (name, ok, detail = '') => {
  report.push(`${ok ? 'OK  ' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`);
  console.log(ok ? 'OK  ' : 'FAIL', name, detail);
  if (!ok) failed++;
};
/** Step both clients for at least `ms` of real time (the server clocks repair in real time) and
 *  at least `minFrames` game frames (a loaded machine may render very few frames per second). */
const run = async (ms, pages, minFrames = 0) => {
  const t0 = Date.now();
  let frames = 0;
  while (Date.now() - t0 < ms || frames < minFrames) {
    frames += 2;
    await Promise.all(pages.map((p) => p.evaluate(() => window.game.step(2, 1 / 30, false))));
    await new Promise((r) => setTimeout(r, 60));
  }
};

const A = await joinGame('netA');
const B = await joinGame('netB');
await run(1500, [A, B]);
check('both clients see the ship', await B.evaluate(() => window.game.ships.length === 1) && (await A.evaluate(() => window.game.ships.length === 1)));

// --- a control operated by A reaches B -------------------------------------------------------------
const before = await B.evaluate(() => window.game.ships[0].sim.sw.ramp);
await A.evaluate(() => {
  const g = window.game;
  const c = g.ships[0].sim.def.controls.find((x) => x.id === 'ext.ramp/ramp');
  const at = window.diag.w(c.c[0], 0, c.c[2] + 0.7);
  g.debug.controller.teleport(at.setY(g.debug.game.terrain.height(at.x, at.z) + 0.05));
});
await run(600, [A, B]);
const reason = await A.evaluate(() => window.game.shipControl('ext.ramp/ramp'));
await run(800, [A, B]);
const after = await B.evaluate(() => window.game.ships[0].sim.sw.ramp);
check('A presses the ramp button, B sees the ramp switch', !reason && after !== before, reason ?? `${before} → ${after}`);
await A.evaluate(() => window.game.shipControl('ext.ramp/ramp'));
await run(600, [A, B]);

// --- a rocket hit reported by A: server blast damages the hull for both ------------------------------
const hit = await A.evaluate(() => {
  const g = window.game;
  const s = g.ships[0];
  const p = s.sim.def.panels.find((q) => q.id === 'CG-L2-2');
  const out = window.diag.w(p.c[0] + p.n[0] * 0.25, p.c[1] + p.n[1] * 0.25, p.c[2] + p.n[2] * 0.25);
  const from = out.clone().add(g.debug.camera.position.clone().set(p.n[0], p.n[1], p.n[2]).transformDirection(s.view.root.matrixWorld).multiplyScalar(10));
  const d = out.clone().sub(from).normalize();
  g.net.sendFire([from.x, from.y, from.z], [d.x, d.y, d.z]);
  g.net.sendHit([out.x, out.y, out.z]);
  return { index: p.index, hpA: s.sim.hp[p.index] };
});
await run(800, [A, B]);
const hpB = await B.evaluate((i) => window.game.ships[0].sim.hp[i], hit.index);
const hpA = await A.evaluate((i) => window.game.ships[0].sim.hp[i], hit.index);
check('server blast damages the panel for both clients', hpB < hit.hpA && Math.abs(hpA - hpB) < 0.2, `hp ${hit.hpA} → A ${hpA} / B ${hpB}`);

// --- A welds it back; B sees the integrity rise ------------------------------------------------------
await A.evaluate((i) => {
  const g = window.game;
  const s = g.ships[0];
  const p = s.sim.def.panels[i];
  g.me.equip('welder');
  g.me.setArmed(true);
  const at = window.diag.w(p.c[0] - p.n[0] * 1.3, 0.05, p.c[2]);
  g.debug.controller.teleport(at);
  g.inspectCam = null;
}, hit.index);
for (let k = 0; k < 3; k++) {
  await run(200, [A]);
  await A.evaluate((i) => {
    const g = window.game;
    const p = g.ships[0].sim.def.panels[i];
    const cam = g.debug.camera.getWorldPosition(g.debug.camera.position.clone());
    const d = window.diag.w(...p.c).sub(cam);
    g.debug.controller.yaw = Math.atan2(-d.x, -d.z);
    g.debug.controller.pitch = Math.atan2(d.y, Math.hypot(d.x, d.z));
  }, hit.index);
}
const aimed = await A.evaluate(() => {
  const t = window.game.interaction.target;
  return t ? `${t.kind}:${t.index}:${t.inReach}` : 'none';
});
const hp0 = await B.evaluate((i) => window.game.ships[0].sim.hp[i], hit.index);
await A.evaluate(() => window.game.debug.input.setKey('Mouse0', true));
await run(2500, [A, B], 60);
const welding = await B.evaluate(() => [...window.game.debug.remotes.values()].some((r) => r.welding));
await A.evaluate(() => window.game.debug.input.setKey('Mouse0', false));
await run(500, [A, B]);
const hp1 = await B.evaluate((i) => window.game.ships[0].sim.hp[i], hit.index);
check('A welds, B sees the panel repaired', hp1 > hp0 + 15, `${aimed} · hp ${hp0.toFixed(1)} → ${hp1.toFixed(1)}`);
check('B sees A welding (sparks on its side)', welding);

writeFileSync(join(OUT, 'ship_net_report.txt'), report.join('\n') + '\n');
await browser.close();
console.log(failed ? `\n${failed} comprobación(es) fallida(s)` : '\ntodo OK');
process.exit(failed ? 1 : 0);
