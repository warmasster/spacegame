// Actual flight displays, Rapier cargo and moving hull sweeps at orbital speed; visual QA.
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import pw from 'playwright';
import { boot } from './progress.mjs';

const out = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(out, { recursive: true });
const browser = await pw.chromium.launch({ args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 640, height: 400 } });
const errors = [], report = [], images = [];
page.on('pageerror', e => errors.push(e.message));
page.on('console', m => { if (m.type() === 'error') errors.push(m.text().slice(0, 500)); });
const check = (name, result) => { report.push({ name, ...result }); console.log(`${result.ok ? 'OK' : 'FAIL'} ${name}: ${JSON.stringify(result)}`); };
const shot = async name => { const path = join(out, `space_${name}.png`); await page.screenshot({ path, timeout: 120000 }); images.push(path); console.log(`captura ${name}`); };
try {
  await boot(page, `${process.env.GAME_URL ?? 'http://localhost:3000'}/?manual&offline`, { quality: 'high', label: 'vuelo y espacio' });
  await page.evaluate(() => { document.querySelector('#ui').style.display = 'none'; });
  const panel = await page.evaluate(async () => {
    const { copyPose } = await import('/src/shared/ship/flight/pose.ts');
    const g = window.game, ship = g.ships[0], auth = g.shipAuthority.get(ship.id);
    auth.pose.p = [240000, 100000, -180000];
    auth.pose.v = [1600, 0, 100]; auth.pose.w = [0, 0, 0];
    auth.landed = false; auth.sw['fa.hold'] = 0; auth.sw['ap.on'] = 0;
    ship.sim.sw['fa.hold'] = 0; ship.sim.sw['ap.on'] = 0;
    auth.flight.resume(); copyPose(ship.sim.pose, auth.pose); copyPose(ship.prev, auth.pose);
    g.sitDown(ship, g.helmIndex());
    g.step(1, 1 / 20, false);
    const item = ship.view.screens.items.find(it => it.def.pages.includes('flight'));
    auth.sw[item.def.id] = ship.sim.sw[item.def.id] = item.def.pages.indexOf('flight');
    g.step(1, 1 / 20, false);
    const initial = item.tex.version, before = item.ctx.canvas.toDataURL();
    for (let i = 0; i < 30; i++) g.step(1, 1 / 20, false);
    const refreshes = item.tex.version - initial, after = item.ctx.canvas.toDataURL();
    window.flightCanvas = after;
    return { ok: refreshes >= 12 && before !== after, refreshes, speed: Math.hypot(...ship.sim.pose.v), origin: g.camera.position.toArray(), page: item.page };
  });
  check('panel de vuelo cambia en órbita sin cambiar de pestaña', panel);
  writeFileSync(join(out, 'space_flight_panel.png'), Buffer.from((await page.evaluate(() => window.flightCanvas)).split(',')[1], 'base64'));
  const cargo = await page.evaluate(async () => {
    const { carry } = await import('/src/shared/frames/frame.ts');
    const g = window.game, ship = g.ships[0];
    const c = g.crates.list.find(c => c.fr === ship.id);
    g.crates.take(c);
    c.body.setTranslation({ x: 0, y: 2, z: 6.5 }, true);
    c.body.setLinvel({ x: 0, y: 0, z: 5.5 }, true);
    c.body.setAngvel({ x: 0.1, y: 0.2, z: 0.3 }, true);
    ship.sim.pose.w = [0, 0.3, 0];
    const r = c.body.rotation(), lv = c.body.linvel();
    const before = carry(g.frames.pose(c.fr), null, [0, 2, 6.5], [lv.x, lv.y, lv.z], [r.x, r.y, r.z, r.w]);
    const spinBefore = g.frames.dirToWorld(c.fr, [0.1, 0.2, 0.3]).map((n, i) => n + ship.sim.pose.w[i]);
    // frame changes happen after the worlds step (every frame and body at the same time)
    g.crates.afterStep(1 / 60);
    const p = c.body.translation(), v = c.body.linvel(), q = c.body.rotation(), av = c.body.angvel();
    const after = carry(g.frames.pose(c.fr), null, [p.x, p.y, p.z], [v.x, v.y, v.z], [q.x, q.y, q.z, q.w]);
    const spinAfter = g.frames.dirToWorld(c.fr, [av.x, av.y, av.z]).map((n, i) => n + g.frames.pose(c.fr).w[i]);
    const positionError = Math.hypot(...after.p.map((n, i) => n - before.p[i]));
    const velocityError = Math.hypot(...after.v.map((n, i) => n - before.v[i]));
    const spinError = Math.hypot(...spinAfter.map((n, i) => n - spinBefore[i]));
    ship.sim.pose.w = [0, 0, 0];
    return { ok: c.fr === 0 && positionError < 0.002 && velocityError < 0.002 && spinError < 1e-5, frame: c.fr, positionError, velocityError, spinError };
  });
  check('caja sale inmediatamente y conserva posición, velocidad y giro', cargo);
  const rockets = await page.evaluate(async () => {
    const { localAt } = await import('/src/shared/frames/frame.ts');
    const g = window.game, ship = g.ships[0], auth = g.shipAuthority.get(ship.id);
    // Fly toward the exit: a static-pose sweep would start 27 m behind the hull and falsely hit it.
    auth.pose.v = [0, 0, 1600]; ship.sim.pose.v = [0, 0, 1600];
    const ramp = ship.sim.def.ramp.key, idx = auth.sys.moverIndex(ramp);
    auth.sw[ramp] = ship.sim.sw[ramp] = 1;
    auth.st[idx] = ship.sim.st[idx] = 1; ship.anim.movers[ramp] = 1;
    ship.physics.update(ship.anim, ship.view.rampPhi(1), ship.sim.flight.feetAnywhere(ship.sim.pose, null, 1));
    g.projectiles.spawn(99999, 'rocket', ship.id, [0, 2, 5.1], [0, 0, 1]);
    const f = g.projectiles.list.at(-1);
    let left = false, alive = true, maxSpeed = 0;
    for (let i = 0; i < 30; i++) {
      g.step(1, 1 / 60, false);
      alive = g.projectiles.list.includes(f);
      if (!alive) break;
      left ||= f.b.fr === 0;
      maxSpeed = Math.max(maxSpeed, Math.hypot(...f.b.v));
    }
    // A projectile still touching the ship in local coordinates must hit its actual panel.
    const z = ship.sim.def.zones.find(z => z.id === 'cargo') ?? ship.sim.def.zones.at(-1);
    const p = [0, 1.2, 0], q = [4, 1.2, 0];
    const direct = ship.localHit(p, q);
    const a = ship.sim.toWorld(p), b = ship.sim.toWorld(q);
    const blocked = g.projectiles.env.sweepWorld(a, b, 1, 1);
    const scratch = [0, 0, 0]; localAt(ship, a, 1, scratch);
    return { ok: alive && left && maxSpeed > 1500 && !!direct && !!blocked, alive, left, maxSpeed, solidHullDetected: !!blocked, zone: z.id };
  });
  check('cohete atraviesa rampa orbital abierta sin impacto falso; casco sigue sólido', rockets);
  // One fixed quad: no trail particles, lights or new geometry over repeated appearances.
  const comet = await page.evaluate(() => {
    const g = window.game, sky = g.sky, mesh = sky.comet, geo = mesh.geometry;
    sky.update(g.camera, sky.nextComet); sky.update(g.camera, sky.cometStart + 20);
    const u = mesh.material.uniforms, dir = u.uDir.value, tail = u.uTail.value;
    const active = mesh.visible, sunDot = tail.dot(sky.sunDir), tangentDot = tail.dot(dir);
    sky.update(g.camera, sky.cometStart + sky.cometLife + 1);
    return { ok: active && !mesh.visible && mesh.geometry === geo && geo.index.count === 6 && sunDot < 0 && Math.abs(tangentDot) < 0.01, active, hiddenAfter: !mesh.visible, triangles: geo.index.count / 3, sunDot, tangentDot };
  });
  check('cometa 2D escaso, reutilizado y cola antisolar', comet);
  // Landscape views: detail underfoot, a distant mountain horizon and a high-altitude survey.
  await page.evaluate(() => { window.game.standUp(); window.game.teleport(window.game.camera.position.clone().set(0, 2, 0)); });
  for (const [name, view] of [
    ['ground', [0, 0, 1.57, -0.35, 1.8]],
    ['mountains', [149000, -146000, 1.62, 0.47, 220]],
    ['highlands', [14000, 8000, 0.8, -0.35, 2200]],
  ]) {
    await page.evaluate(view => { window.game.inspectCam = view; }, view);
    for (let i = 0; i < 20; i++) { await page.evaluate(() => window.game.step(1)); await page.waitForTimeout(200); }
    await shot(name);
  }
  await page.evaluate(async () => {
    const g = window.game;
    g.inspectCam = null; g.step(1, 1 / 60, false);
    for (const ground of g.grounds) { ground.terrain.group.visible = false; ground.rocks.group.visible = false; }
    for (const ship of g.ships) ship.view.root.visible = false;
    g.me.root.visible = false; g.sky.group.visible = true;
    g.sky.update(g.camera, g.sky.nextComet); g.sky.update(g.camera, g.sky.cometStart + 20);
    const dir = g.sky.comet.material.uniforms.uDir.value;
    const { origin } = await import('/src/client/render/origin.ts');
    g.camera.lookAt(origin.toRender(g.camera.position.clone().add(dir)));
    g.camera.fov = 35; g.camera.updateProjectionMatrix();
    g.camera.updateMatrixWorld(); g.pipeline.render(1 / 60);
  });
  await shot('comet');
  await page.evaluate(async () => {
    const g = window.game;
    g.sky.comet.visible = false;
    g.camera.fov = 75; g.camera.updateProjectionMatrix();
    const galaxy = g.camera.position.clone().set(-0.05487556, -0.87343709, -0.48383502).applyMatrix3(g.sky.cfg.celestial);
    const { origin } = await import('/src/client/render/origin.ts');
    g.camera.lookAt(origin.toRender(g.camera.position.clone().add(galaxy))); g.camera.updateMatrixWorld(); g.pipeline.render(1 / 60);
  });
  await shot('sky');
  check('navegador sin excepciones ni errores de shaders', { ok: errors.length === 0, errors });
  const sheet = await browser.newPage({ viewport: { width: 640 * 2, height: 400 * Math.ceil(images.length / 2) } });
  await sheet.setContent(`<body style="margin:0;display:flex;flex-wrap:wrap">${images.map(p => `<img width="640" height="400" src="data:image/png;base64,${readFileSync(p).toString('base64')}">`).join('')}</body>`);
  await sheet.screenshot({ path: join(out, 'space_sheet.png') });
  assert.ok(report.every(r => r.ok), 'flight/space regression failed');
} finally {
  writeFileSync(join(out, 'space_report.json'), JSON.stringify(report, null, 2));
  await browser.close();
}
