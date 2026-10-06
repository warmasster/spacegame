// End-to-end equipment checks, through the game's real input and authority paths.
// GAME_URL=http://localhost:3000 node tools/diag/equipment.mjs
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import pw from 'playwright';
import { boot } from './progress.mjs';

const out = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(out, { recursive: true });
const browser = await pw.chromium.launch({ args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 960, height: 600 } });
const errors = [];
const report = [];
page.on('pageerror', e => errors.push(e.message));
const check = (name, ok, detail) => {
  report.push({ name, ok, detail });
  console.log(`${ok ? 'OK' : 'FAIL'} ${name}: ${JSON.stringify(detail)}`);
};
try {
  await boot(page, `${process.env.GAME_URL ?? 'http://localhost:3000'}/?manual&offline`, { quality: 'low', label: 'equipo' });
  const crater = await page.evaluate(async () => {
    const { bodyAt } = await import('/src/shared/space/body.ts');
    const g = window.game;
    const vec = (x, y, z) => g.camera.position.clone().set(x, y, z);
    const spawn = g.welcome.spawn;
    const x = spawn[0] + 35, z = spawn[2] + 25;
    g.teleport(vec(x, g.groundY(x, z) + 0.05, z));
    g.ctl.pitch = -0.8;
    g.me.equip('launcher');
    g.me.setArmed(true);
    g.step(20, 1 / 30, false);
    const surface = g.surfaces(bodyAt([x, 0, z]));
    const before = surface.mods.dynamic.length;
    g.input.locked = true;
    window.dispatchEvent(new MouseEvent('mousedown', { button: 0 }));
    g.step(1, 1 / 30, false);
    window.dispatchEvent(new MouseEvent('mouseup', { button: 0 }));
    const flying = g.projectiles.list.length;
    for (let i = 0; i < 90 && surface.mods.dynamic.length === before; i++) g.step(1, 1 / 30, false);
    const after = surface.mods.dynamic.length;
    const mod = surface.mods.dynamic.at(-1);
    let depth = 0;
    if (after > before) {
      const h = surface.height(mod.center);
      const list = surface.mods.dynamic;
      surface.mods.setDynamic(list.slice(0, -1));
      depth = surface.height(mod.center) - h;
      surface.mods.setDynamic(list);
    }
    g.step(1);
    return { before, after, depth, flying };
  });
  check('cohete disparado con clic crea un cráter real', crater.after > crater.before && crater.depth > 0.1, crater);
  const groundAt = index => page.evaluate(async (index) => {
    const THREE = await import('/node_modules/.vite/deps/three.js');
    const { bodyAt } = await import('/src/shared/space/body.ts');
    const { START_SITE, siteGround } = await import('/src/shared/space/world.ts');
    const { origin } = await import('/src/client/render/origin.ts');
    const g = window.game, body = bodyAt(g.welcome.spawn), surface = g.surfaces(body);
    const mod = surface.mods.dynamic[index];
    if (!mod) return { meshError: null, collisionError: null };
    const up = new THREE.Vector3(...mod.center).normalize();
    const at = new THREE.Vector3(...body.center).addScaledVector(up, body.radius + surface.height(mod.center));
    const site = siteGround(START_SITE, g.surfaces);
    const local = site.toLocal(at.toArray(), [0, 0, 0]);
    // Move the player off the crater so a collision ray cannot hit the suit itself.
    const feet = site.point(local[0] + 8, local[2] + 8, [0, 0, 0]);
    g.teleport(new THREE.Vector3(...feet));
    g.inspectCam = [local[0] + 7, local[2] + 7, Math.PI / 4, -0.48, 5];
    const top = at.clone().addScaledVector(up, 10);
    const meshes = [];
    let meshError = null, collisionError = null;
    for (let i = 0; i < 200; i++) {
      g.step(2, 1 / 30, false);
      g.scene.updateMatrixWorld(true);
      meshes.length = 0;
      for (const n of g.grounds[0].terrain.nodes) if (n.mesh?.visible) meshes.push(n.mesh);
      const ray = new THREE.Raycaster(origin.toRender(top.clone()), up.clone().negate(), 0, 15);
      const hit = ray.intersectObjects(meshes, false)[0];
      meshError = hit ? Math.abs(hit.distance - 10) : null;
      const o = g.frames.toLocal(0, top.toArray()), d = g.frames.dirToLocal(0, up.clone().negate().toArray());
      const r = new g.physics.rapier.Ray({ x: o[0], y: o[1], z: o[2] }, { x: d[0], y: d[1], z: d[2] });
      const solid = g.physics.world.castRay(r, 15, true);
      collisionError = solid ? Math.abs(solid.timeOfImpact - 10) : null;
      if (meshError !== null && meshError < 0.3 && collisionError !== null && collisionError < 0.3) break;
      await new Promise(r => setTimeout(r, 50));
    }
    document.querySelector('.pause').classList.add('hidden');
    document.querySelector('.hud-help').classList.add('hidden');
    g.step(1);
    g.inspectCam = null;
    return { meshError, collisionError };
  }, index);
  const ground = await groundAt(0);
  check('malla visible y colisión se hunden hasta el suelo del cráter', ground.meshError !== null && ground.meshError < 0.3 && ground.collisionError !== null && ground.collisionError < 0.3, ground);
  await page.screenshot({ path: join(out, 'equipment_crater.png') });

  // A single successful crater misses the rounded-direction merge/invalidation regression.
  // Fire at distinct nearby points, while their meshes already exist, and inspect every hole.
  for (let shot = 1; shot <= 3; shot++) {
    const edit = await page.evaluate(async shot => {
      const { bodyAt } = await import('/src/shared/space/body.ts');
      const g = window.game, spawn = g.welcome.spawn;
      const x = spawn[0] + 35 + shot * 12, z = spawn[2] + 25;
      g.teleport(g.camera.position.clone().set(x, g.groundY(x, z) + 0.05, z));
      g.ctl.yaw = 0;
      g.ctl.pitch = -0.8;
      g.me.equip('launcher');
      g.me.setArmed(true);
      g.step(20, 1 / 30, false);
      const surface = g.surfaces(bodyAt(spawn));
      const previous = JSON.stringify(surface.mods.dynamic), before = surface.mods.dynamic.length;
      g.input.locked = true;
      window.dispatchEvent(new MouseEvent('mousedown', { button: 0 }));
      g.step(1, 1 / 30, false);
      window.dispatchEvent(new MouseEvent('mouseup', { button: 0 }));
      for (let i = 0; i < 90 && surface.mods.dynamic.length === before; i++) g.step(1, 1 / 30, false);
      const mods = surface.mods.dynamic;
      return { before, after: mods.length, unchanged: JSON.stringify(mods.slice(0, before)) === previous };
    }, shot);
    check(`cohete ${shot + 1} abre otro cráter sin profundizar los anteriores`, edit.after === edit.before + 1 && edit.unchanged, edit);
    const solid = await groundAt(edit.after - 1);
    check(`cráter ${shot + 1} actualiza malla visible y colisión`, solid.meshError !== null && solid.meshError < 0.3 && solid.collisionError !== null && solid.collisionError < 0.3, solid);
    await page.screenshot({ path: join(out, `equipment_crater_${shot + 1}.png`) });
  }

  const turret = await page.evaluate(() => {
    const g = window.game, ship = g.ships[0];
    g.teleport(ship.world([0, 0.05, -6.8]));
    g.localInteract(ship.id, ship.sim.def.controls.find(c => c.key === 'turret').index);
    g.sitDown(ship, 1);
    g.gunnery.command(ship, ship.sim.def.mounts[0].id, 'aim');
    g.input.addLook(-250, -100);
    for (let i = 0; i < 80; i++) g.step(1, 1 / 30, false);
    const mirror = ship.sim.mounts.list[0];
    const authority = g.shipAuthority.get(ship.id);
    const rt = authority.mounts.list[0];
    const ammo = authority.st[rt.iAmmo];
    const yaw = authority.st[rt.iYaw];
    const lead = ship.view.mountLead[0].yaw;
    // Camera aiming consumes the click even while the helmet stays on the monitor.
    g.input.locked = true;
    window.dispatchEvent(new MouseEvent('mousedown', { button: 0 }));
    g.step(1, 1 / 30, false);
    window.dispatchEvent(new MouseEvent('mouseup', { button: 0 }));
    const spent = ammo - authority.st[rt.iAmmo];
    const flying = g.projectiles.list.length;
    g.step(10, 1 / 30, false);
    return { ready: ship.sim.st[mirror.iReady], yaw, lead, ammo, spent, flying, after: authority.st[rt.iAmmo] };
  });
  check('puntería de torreta llega a la autoridad offline', turret.ready === 1 && Math.abs(turret.yaw - turret.lead) < 0.06, turret);
  check('clic de artillero consume munición autoritativa y lanza minimisil', turret.spent > 0.9 && turret.flying > 0 && turret.after < turret.ammo - 0.5, turret);
  await page.evaluate(async () => {
    const { START_SITE, siteGround } = await import('/src/shared/space/world.ts');
    const g = window.game, ship = g.ships[0];
    g.standUp();
    const site = siteGround(START_SITE, g.surfaces);
    const from = ship.world([5, 7, 0]);
    const to = ship.world(ship.sim.def.mounts[0].at);
    const d = to.sub(from);
    const local = site.toLocal(from.toArray(), [0, 0, 0]);
    g.inspectCam = [local[0], local[2], Math.atan2(-d.x, -d.z), Math.atan2(d.y, Math.hypot(d.x, d.z)), 7];
    g.step(1);
  });
  await page.screenshot({ path: join(out, 'equipment_turret.png') });
  check('sin excepciones del navegador', errors.length === 0, errors);
} finally {
  await browser.close();
  writeFileSync(join(out, 'equipment_report.json'), JSON.stringify(report, null, 2));
}
process.exitCode = report.some(r => !r.ok) ? 1 : 0;
