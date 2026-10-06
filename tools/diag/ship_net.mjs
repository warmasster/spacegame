// Ship over the network: two real clients against the running server (npm run dev). Checks that a
// control operated by A reaches B, that a rocket hit reported by A damages the hull for both
// (server-side blast), and that welding by A (with the welder in hand) repairs it for B.
// The server keeps its ship state between runs, so every check is relative. Exit 1 on failure.
//
//   node tools/diag/ship_net.mjs
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { boot, progress } from './progress.mjs';

const pw = await import('playwright').catch(() => import('/opt/node22/lib/node_modules/playwright/index.mjs'));
const OUT = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(OUT, { recursive: true });
const url = process.env.GAME_URL ?? 'http://localhost:3000';
const browser = await pw.chromium.launch({ args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });

async function joinGame(name) {
  const page = await browser.newPage({ viewport: { width: 640, height: 400 } });
  page.on('pageerror', (e) => console.log(`[${name}] pageerror:`, e.message));
  await boot(page, `${url}/?manual`, { name, quality: 'low', label: name });
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

const P = progress('red', 8);
const A = await joinGame('netA');
P.step('cliente A dentro');
const B = await joinGame('netB');
P.step('cliente B dentro');
await run(1500, [A, B]);
const nShips = await A.evaluate(() => window.game.ships.length);
check('both clients see the same ships', nShips > 0 && (await B.evaluate(() => window.game.ships.length)) === nShips, `${nShips} naves`);

// --- a control operated by A reaches B -------------------------------------------------------------
const before = await B.evaluate(() => window.game.ships[0].sim.sw.ramp);
await A.evaluate(() => {
  const g = window.game;
  const c = g.ships[0].sim.def.controls.find((x) => x.id === 'ext.ramp/ramp');
  const at = window.diag.w(c.c[0], 0, c.c[2] + 0.7);
  g.teleport(at.setY(g.debug.game.groundY(at.x, at.z) + 0.05));
});
await run(600, [A, B]);
const reason = await A.evaluate(() => window.game.shipControl('ext.ramp/ramp'));
await run(800, [A, B]);
const after = await B.evaluate(() => window.game.ships[0].sim.sw.ramp);
check('A presses the ramp button, B sees the ramp switch', !reason && after !== before, reason ?? `${before} → ${after}`);
await A.evaluate(() => window.game.shipControl('ext.ramp/ramp'));
await run(600, [A, B]);
P.step('mando por red');

// --- a rocket hit reported by A: server blast damages the hull for both ------------------------------
const hit = await A.evaluate(() => {
  const g = window.game;
  const s = g.ships[0];
  const p = s.sim.def.panels.find((q) => q.id === 'CG-L2-2');
  const out = window.diag.w(p.c[0] + p.n[0] * 0.25, p.c[1] + p.n[1] * 0.25, p.c[2] + p.n[2] * 0.25);
  const from = out.clone().add(g.debug.camera.position.clone().set(p.n[0], p.n[1], p.n[2]).transformDirection(s.view.root.matrixWorld).multiplyScalar(10));
  const d = out.clone().sub(from).normalize();
  g.net.sendFire('launcher', [from.x, from.y, from.z], [d.x, d.y, d.z]);
  g.net.sendHit('rocket', [out.x, out.y, out.z]);
  return { index: p.index, hpA: s.sim.hp[p.index] };
});
await run(800, [A, B]);
const hpB = await B.evaluate((i) => window.game.ships[0].sim.hp[i], hit.index);
const hpA = await A.evaluate((i) => window.game.ships[0].sim.hp[i], hit.index);
P.step('impacto por red');
check('server blast damages the panel for both clients', hpB < hit.hpA && Math.abs(hpA - hpB) < 0.2, `hp ${hit.hpA} → A ${hpA} / B ${hpB}`);

// --- A welds it back; B sees the integrity rise ------------------------------------------------------
await A.evaluate((i) => {
  const g = window.game;
  const s = g.ships[0];
  const p = s.sim.def.panels[i];
  g.me.equip('welder');
  g.me.setArmed(true);
  const at = window.diag.w(p.c[0] - p.n[0] * 1.3, 0.05, p.c[2]);
  g.teleport(at);
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
// Observe while the repair is active. On a slow software renderer the panel can already be
// completely repaired by the end of a 60-frame run, so its final welding flag should be false.
let welding = false;
for (let k = 0; k < 7; k++) {
  await run(400, [A, B], 8);
  welding ||= await B.evaluate(() => [...window.game.debug.remotes.values()].some((r) => r.welding));
}
await A.evaluate(() => window.game.debug.input.setKey('Mouse0', false));
await run(500, [A, B]);
const hp1 = await B.evaluate((i) => window.game.ships[0].sim.hp[i], hit.index);
P.step('soldadura por red');
check('A welds, B sees the panel repaired', hp1 > hp0 + 15, `${aimed} · hp ${hp0.toFixed(1)} → ${hp1.toFixed(1)}`);
check('B sees A welding (sparks on its side)', welding);

// Equipment identity must survive the binary state path, including for the remote rig.
const remoteTool = await B.evaluate(() => [...window.game.remotes.values()][0]?.astronaut.equipped);
check('B sees A holding the welder from the catalog', remoteTool === 'welder', remoteTool);

// --- A mans a mount: server slews it and spends rounds; B receives both ---------------------------
await A.evaluate(() => {
  const g = window.game, ship = g.ships[0];
  g.teleport(ship.world([0.72, 0.05, -6.8]));
  g.sitDown(ship, 1);
  // Camera stations aim with the mouse, independently of the astronaut's head angle.
  g.gunnery.command(ship, ship.sim.def.mounts[0].id, 'aim');
  g.input.addLook(-0.55 / g.input.sensitivity, -0.25 / g.input.sensitivity);
});
await run(500, [A, B]);
const turretReason = await A.evaluate(() => window.game.ships[0].sim.sw.turret === 1 ? null : window.game.shipControl('ck.over/turret'));
await run(1400, [A, B], 50);
const aim = await B.evaluate(() => {
  const s = window.game.ships[0], m = s.sim.mounts.list[0];
  return { yaw: s.sim.st[m.iYaw], ready: s.sim.st[m.iReady], ammo: s.sim.st[m.iAmmo] };
});
check('server slews the powered mount, B sees its aim', !turretReason && aim.ready === 1 && Math.abs(aim.yaw - 0.55) < 0.1, { ...aim, reason: turretReason });
const launched = await A.evaluate(() => {
  const g = window.game;
  g.input.locked = true;
  window.dispatchEvent(new MouseEvent('mousedown', { button: 0 }));
  g.step(1, 1 / 30, false);
  window.dispatchEvent(new MouseEvent('mouseup', { button: 0 }));
  return g.projectiles.list.some(f => f.b.kind === 'minimissile');
});
await run(400, [A, B]);
const mountShot = await B.evaluate(() => {
  const g = window.game, s = g.ships[0], m = s.sim.mounts.list[0];
  return { ammo: s.sim.st[m.iAmmo], missile: g.projectiles.list.some(f => f.b.kind === 'minimissile') };
});
check('gunner click is accepted by server, replicated missile and ammo', launched && mountShot.missile && mountShot.ammo < aim.ammo, mountShot);
P.step('torreta por red');

// --- A fires at the ground through normal input; both clients receive the same surface edit -------
const groundBefore = await B.evaluate(async () => {
  const { bodyAt } = await import('/src/shared/space/body.ts');
  const g = window.game;
  return g.surfaces(bodyAt(g.welcome.spawn)).mods.dynamic;
});
for (let shot = 0; shot < 3; shot++) {
  await A.evaluate(shot => {
    const g = window.game, spawn = g.welcome.spawn;
    const x = spawn[0] + 35 + shot * 12, z = spawn[2] + 25;
    g.teleport(g.camera.position.clone().set(x, g.groundY(x, z) + 0.05, z));
    g.ctl.yaw = 0;
    g.ctl.pitch = -0.8;
    g.me.equip('launcher');
    g.me.setArmed(true);
    g.step(20, 1 / 30, false);
    g.input.locked = true;
    window.dispatchEvent(new MouseEvent('mousedown', { button: 0 }));
    g.step(1, 1 / 30, false);
    window.dispatchEvent(new MouseEvent('mouseup', { button: 0 }));
  }, shot);
  await run(800, [A, B], 40);
}
const groundOf = p => p.evaluate(async () => {
  const { bodyAt } = await import('/src/shared/space/body.ts');
  const g = window.game, s = g.surfaces(bodyAt(g.welcome.spawn));
  const mods = s.mods.dynamic;
  const mod = mods.at(-1);
  return { count: mods.length, mods, height: mod ? s.height(mod.center) : null };
});
const ga = await groundOf(A), gb = await groundOf(B);
// The previously fired minimissile can reach the surface during this phase too. It creates its
// own smaller crater; count the three rocket-sized edits rather than all projectile impacts.
const rocketEdits = ga.mods.slice(groundBefore.length).filter(m => Math.abs(m.radius - 2.4) < 1e-6);
check('three separate rocket impacts create separate identical craters for A and B', rocketEdits.length === 3 && JSON.stringify(ga.mods.slice(0, groundBefore.length)) === JSON.stringify(groundBefore) && JSON.stringify(ga) === JSON.stringify(gb), { before: groundBefore.length, rocketEdits: rocketEdits.length, A: ga, B: gb });
// A late joiner receives the authority's persisted edits, rather than local-only effects.
await B.close();
const C = await joinGame('netLate');
const gc = await groundOf(C);
check('late joiner receives the same persisted crater', JSON.stringify(gc) === JSON.stringify(ga), gc);
P.step('cráter por red y al entrar');

P.step('fin');
writeFileSync(join(OUT, 'ship_net_report.txt'), report.join('\n') + '\n');
await browser.close();
console.log(failed ? `\n${failed} comprobación(es) fallida(s)` : '\ntodo OK');
process.exit(failed ? 1 : 0);
