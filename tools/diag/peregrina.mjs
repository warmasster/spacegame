// Peregrina diagnostics: the passenger shuttle in the real game (offline). Every control acts or
// refuses with a reason, the exterior lights sit on the hull, the boarding stairs can be walked up,
// the airlock cycles with the doors and colliders following, passenger seats work, the solar wings
// and radiators animate; then screenshots from fixed cameras. Exit code 1 if a check fails.
//
//   node tools/diag/peregrina.mjs [views|checks|all]      (default: all)
//
// Needs `npm run dev` (GAME_URL to point elsewhere) and Playwright. Output: tools/diag/out/prg_*.png,
// prg_sheet.png, prg_report.txt
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { boot, progress } from './progress.mjs';

const pw = await import('playwright');
const mode = process.argv[2] ?? 'all';
const OUT = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(OUT, { recursive: true });
const url = process.env.GAME_URL ?? 'http://localhost:3000';

const browser = await pw.chromium.launch({ args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 640, height: 400 } });
const errors = [];
page.on('pageerror', (e) => {
  errors.push(e.message);
  console.log('pageerror:', e.message);
});
page.on('console', (m) => {
  if (m.type() !== 'error') return;
  errors.push(m.text().slice(0, 300));
  console.log('console:', m.text().slice(0, 1500));
});
const CHECK_BLOCKS = 7;
const VIEWS = 12;
const P = progress('peregrina', (mode === 'views' ? 0 : CHECK_BLOCKS) + (mode === 'checks' ? 0 : VIEWS));
await boot(page, `${url}/?manual&offline`, { label: 'juego' });

// helpers inside the page (ship = the Peregrina)
await page.evaluate(() => {
  const g = window.game;
  const ship = () => g.ships.find((s) => s.sim.def.id === 'peregrina');
  const V = (x, y, z) => g.debug.camera.position.clone().set(x, y, z);
  window.prg = {
    ship,
    w: (x, y, z) => ship().view.root.localToWorld(V(x, y, z)),
    local: (p) => ship().local(p),
    look(from, to) {
      const a = this.w(...from);
      const b = this.w(...to);
      const d = b.clone().sub(a);
      g.inspectCam = [a.x, a.z, Math.atan2(-d.x, -d.z), Math.atan2(d.y, Math.hypot(d.x, d.z)), a.y - g.debug.game.groundY(a.x, a.z)];
    },
    stand(at, to) {
      const a = this.w(...at);
      const b = this.w(...to);
      const c = g.debug.controller;
      c.teleport(a);
      const d = b.clone().sub(a.clone().setY(a.y + 1.6));
      c.yaw = Math.atan2(-d.x, -d.z);
      c.pitch = Math.atan2(d.y, Math.hypot(d.x, d.z));
      g.inspectCam = null;
    },
    /** operate a control of this ship by id (through the normal path) */
    ctl(id) {
      const s = ship();
      return g.shipControl(id, g.ships.indexOf(s));
    },
  };
});

const report = [];
let failed = 0;
const check = (name, ok, detail = '') => {
  report.push(`${ok ? 'OK  ' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`);
  console.log(ok ? 'OK  ' : 'FAIL', name, detail);
  if (!ok) failed++;
};
const shots = [];
const shot = async (name, setup) => {
  await page.evaluate(() => {
    document.querySelector('#ui').style.display = 'none';
    document.querySelector('.pause')?.classList.add('hidden');
  });
  await page.evaluate(setup);
  await page.evaluate(() => window.game.step(6));
  await page.waitForTimeout(250);
  await page.evaluate(() => window.game.step(2));
  const f = join(OUT, `prg_${name}.png`);
  await page.screenshot({ path: f, timeout: 240000 });
  shots.push(f);
  P.step(`captura ${name}`);
};

if (mode === 'checks' || mode === 'all') {
  // --- every control does something or says why not ----------------------------------------------
  const r = await page.evaluate(() => {
    const g = window.game;
    const s = window.prg.ship();
    const out = { panels: s.sim.def.panels.length, colliders: s.sim.def.panels.filter((p) => s.physics.hasPanel(p.index)).length, acts: [] };
    const sw0 = { ...s.sim.sw };
    for (const c of s.sim.def.controls) {
      // the cycle has its own check; covers are opened for the control they guard
      if (c.key === 'lock.cycle' || c.kind === 'cover') continue;
      const guard = c.guard && s.sim.sw[c.guard] !== 1 ? s.sim.def.controls.find((x) => x.kind === 'cover' && x.key === c.guard && x.console === c.console) : null;
      if (guard) window.prg.ctl(guard.id);
      const before = s.sim.sw[c.key];
      const reason = window.prg.ctl(c.id);
      const after = s.sim.sw[c.key];
      out.acts.push({ id: c.id, key: c.key, action: c.action, before, after, reason });
      if (!reason && c.action === 'toggle' && after !== before) window.prg.ctl(c.id);
      if (guard) window.prg.ctl(guard.id);
      g.step(1, 1 / 30, false);
    }
    // everything back where it was (selectors were stepped, not toggled back), then undo the SCRAM
    const auth = [...g.debug.game.shipAuthority.values()].find((x) => x.id === s.id);
    for (const [k, v] of Object.entries(sw0)) {
      if (s.sim.sw[k] === v) continue;
      auth.sw[k] = v;
      s.apply({ [k]: v });
    }
    g.step(3, 1 / 30, false);
    // The sweep's own REARME already clears a SCRAM whose core is under 300 °C (the micro-reactor
    // idles there), and restoring the lever starts it again. A core still too hot has to be cooled
    // and reset here. Either way, wait the start out (XS is 10 s).
    if (s.sim.get('rx.state') === 3) {
      auth.st[auth.vars.idx('rx.temp')] = 100;
      g.step(3, 1 / 30, false);
      window.prg.ctl('ck.r/rx.reset');
      g.step(3, 1 / 30, false);
    }
    if (s.sim.get('rx.state') !== 2 && s.sim.sw.reactor !== 1) window.prg.ctl('ck.r/reactor');
    if (s.sim.get('rx.state') !== 2) g.step(16 * 30, 1 / 30, false);
    out.rx = s.sim.get('rx.state');
    return out;
  });
  check('ship loaded', r.panels > 80, `${r.panels} paneles`);
  check('every panel has a collider', r.colliders === r.panels, `${r.colliders}/${r.panels}`);
  const expected = { gear: /peso/, 'door.ext': /presurizada|diferencia/, 'door.lock': /presurizada|diferencia/, 'rx.reset': /SCRAM|caliente/, 'eng.start': /desarmado/, refuel: /repostaje/ };
  for (const a of r.acts) {
    if (a.key === 'caution') check(`control ${a.id}`, !a.reason, a.reason ?? 'reset');
    else if (a.key in expected && a.reason) check(`control ${a.id} (negativa esperada)`, expected[a.key].test(a.reason), a.reason);
    else if (a.action === 'set' && a.before === a.after && !a.reason) check(`control ${a.id}`, true, 'ya estaba ahí');
    else if (a.action === 'pulse' && !a.reason) check(`control ${a.id}`, true, 'pulso consumido');
    else check(`control ${a.id}`, !a.reason && a.after !== a.before, a.reason ?? `${a.key}: ${a.before} → ${a.after}`);
  }
  check('reactor back online after the SCRAM test', r.rx === 2, `estado ${r.rx}`);
  P.step('mandos');

  // --- exterior lights on the hull ------------------------------------------------------------------
  const lights = await page.evaluate(() => {
    const s = window.prg.ship();
    return s.sim.def.extLights.map((l) => {
      const from = window.prg.w(l.pos[0] + l.n[0] * 0.25, l.pos[1] + l.n[1] * 0.25, l.pos[2] + l.n[2] * 0.25);
      const to = window.prg.w(...l.pos);
      const hit = s.physics.castRay(from, to.clone().sub(from).normalize(), 0.6);
      return { kind: l.kind, gap: hit ? hit.t - 0.25 : null };
    });
  });
  for (const l of lights) check(`ext light ${l.kind} mounted on the hull`, l.gap !== null && Math.abs(l.gap) < 0.06, l.gap === null ? 'flota' : `${(l.gap * 100).toFixed(1)} cm`);
  P.step('luces');

  // --- walk up the boarding stairs into the airlock (hatch open at spawn) -------------------------------
  const wk = await page.evaluate(() => {
    const g = window.game;
    const s = window.prg.ship();
    const c = g.debug.controller;
    window.prg.stand([5.4, 0, 4.1], [0, 1.6, 4.1]);
    const p0 = c.position.clone();
    c.teleport(p0.setY(g.debug.game.groundY(p0.x, p0.z) + 0.05));
    g.step(10, 1 / 30, false);
    g.debug.input.setKey('KeyW', true);
    for (let i = 0; i < 56; i++) g.step(5, 1 / 30, false);
    g.debug.input.setKey('KeyW', false);
    g.step(20, 1 / 30, false);
    const l = window.prg.local(c.position);
    return { x: l.x, y: l.y, z: l.z, zone: s.zoneAt(c.position.clone().setY(c.position.y + 1))?.id ?? 'fuera' };
  });
  check('walks up the stairs through the hatch', Math.abs(wk.y) < 0.15 && wk.x < 1.3, `local ${wk.x.toFixed(2)}, ${wk.y.toFixed(2)}, ${wk.z.toFixed(2)} · ${wk.zone}`);
  check('ends inside the airlock', wk.zone === 'lock', wk.zone);
  P.step('escalerilla');

  // --- airlock cycle in the game: in, then out; the hatch collider follows ------------------------------
  const lk = await page.evaluate(() => {
    const g = window.game;
    const s = window.prg.ship();
    const o = {};
    o.inReason = window.prg.ctl('lk.ctl/lock.cycle=0');
    g.step(20 * 30, 1 / 30, false);
    o.phaseIn = s.sim.get('lock.phase');
    o.pLock = s.sim.get('lock.p');
    o.inner = s.anim.movers['door.lock'];
    o.outer = s.anim.movers['door.ext'];
    // the hatch is shut: a ray from outside through it hits the door leaf
    const from = window.prg.w(3, 1, 4.3);
    const dir = window.prg.w(0, 1, 4.3).sub(from).normalize();
    const hit = s.physics.castRay(from, dir, 4);
    o.hatchSolid = !!hit && hit.t < 1.7;
    o.breathable = s.sim.sys.breathable(s.sim.st, 'lock');
    window.prg.ctl('lk.ctl/lock.cycle=1');
    g.step(40 * 30, 1 / 30, false);
    o.phaseOut = s.sim.get('lock.phase');
    o.pCabin = s.sim.get('cabin.p');
    const hit2 = s.physics.castRay(from, dir, 4);
    o.hatchOpen = !hit2 || hit2.t > 1.7;
    return o;
  });
  check('cycle in: inner door opens on a full airlock', lk.inReason === null && lk.phaseIn === 0 && lk.inner > 0.98 && lk.outer < 0.02 && lk.pLock > 60, `fase ${lk.phaseIn}, ${lk.pLock?.toFixed(1)} kPa, interior ${lk.inner}, escotilla ${lk.outer} ${lk.inReason ?? ''}`);
  check('shut hatch is solid', lk.hatchSolid);
  check('airlock air is breathable after the cycle', lk.breathable);
  check('cycle out: hatch opens, cabin keeps its air', lk.phaseOut === 4 && lk.hatchOpen && lk.pCabin > 60, `fase ${lk.phaseOut}, habitáculo ${lk.pCabin?.toFixed(1)} kPa`);
  P.step('esclusa');

  // --- passenger seat --------------------------------------------------------------------------------
  const st = await page.evaluate(() => {
    const g = window.game;
    const s = window.prg.ship();
    const seat = s.sim.def.seats[1];
    g.sitDown(s, 1);
    g.step(20, 1 / 30);
    const sat = window.prg.local(g.playerWorld().p);
    const seated = !!g.seat;
    g.standUp();
    g.step(10, 1 / 30);
    const after = window.prg.local(g.playerWorld().p);
    return { seated, sat: sat.toArray(), root: seat.root, after: after.toArray(), exit: seat.exit, standing: !g.seat };
  });
  check('passenger sits down', st.seated && Math.hypot(st.sat[0] - st.root[0], st.sat[2] - st.root[2]) < 0.06, `local ${st.sat.map((v) => v.toFixed(2))}`);
  check('passenger stands up at the aisle', st.standing && Math.hypot(st.after[0] - st.exit[0], st.after[2] - st.exit[2]) < 0.3, `local ${st.after.map((v) => v.toFixed(2))}`);
  P.step('asientos');

  // --- deployables animate with their mover -----------------------------------------------------------
  const dp = await page.evaluate(() => {
    const g = window.game;
    const s = window.prg.ship();
    const wing = s.view.root.getObjectByName('part:solar')?.getObjectByName('wingR');
    const open = wing?.rotation.z;
    window.prg.ctl('ck.r/solar');
    g.step(8 * 30, 1 / 30);
    const folded = wing?.rotation.z;
    window.prg.ctl('ck.r/solar');
    g.step(8 * 30, 1 / 30);
    return { open, folded, back: wing?.rotation.z, kw: s.sim.get('solar.kw') };
  });
  check('solar wing folds and unfolds', dp.open !== undefined && Math.abs(dp.open) < 0.01 && Math.abs(dp.folded - Math.PI) < 0.05 && Math.abs(dp.back) < 0.01, `${dp.open} → ${dp.folded?.toFixed?.(2)} → ${dp.back}`);
  check('solar power with the wings out', dp.kw > 3, `${dp.kw?.toFixed?.(2)} kW`);
  P.step('despliegues');

  // --- the manual follows the ship you are in -----------------------------------------------------------
  const mn = await page.evaluate(() => {
    const g = window.game;
    window.prg.stand([0, 0.05, 0.4], [0, 1.2, -2]);
    g.step(5, 1 / 30);
    g.debug.game.hud.toggleManual(true);
    const title = document.querySelector('.mn-title b')?.textContent;
    const figs = document.querySelectorAll('.mn-fig').length;
    const equip = document.querySelectorAll('.mn-spec').length;
    g.debug.game.hud.toggleManual(false);
    return { title, figs, equip };
  });
  check('manual opens on the Peregrina', /Peregrina/.test(mn.title ?? ''), mn.title);
  check('manual shows the generated figures and the equipment sheet', mn.figs >= 8 && mn.equip > 40, `${mn.figs} figuras, ${mn.equip} datos`);
  P.step('manual');
}

if (mode === 'views' || mode === 'all') {
  await page.evaluate(() => window.game.teleport(window.prg.w(9, 0, 0)));
  await shot('ext_hatch', () => window.prg.look([6.5, 1.2, 6.5], [0, 0.8, 2.5]));
  await shot('ext_front', () => window.prg.look([-3.5, 1.4, -11], [0, 0.8, -3]));
  await shot('ext_rear', () => window.prg.look([-4, 2.6, 11], [0, 1, 4]));
  await shot('ext_top', () => window.prg.look([5, 7, 3], [0, 2, 0.5]));
  await shot('ext_port', () => window.prg.look([-9, 1.6, 0], [0, 1, 0]));
  await shot('int_cockpit', () => window.prg.look([0.4, 1.65, -2.8], [0, 0.9, -5]));
  await shot('int_cabin_fwd', () => window.prg.look([0.1, 1.6, 2.2], [0, 1.1, -2.4]));
  await shot('int_cabin_aft', () => window.prg.look([0, 1.6, -2.3], [0, 1.0, 3.0]));
  await shot('int_airlock', () => window.prg.look([0.2, 1.6, 3.55], [0.6, 1.2, 4.8]));
  await shot('int_boards', () => window.prg.look([0.1, 1.55, -0.6], [0, 1.25, -2.1]));
  await shot('ext_both', () => {
    const g = window.game;
    const s = window.prg.ship();
    const other = g.ships.find((x) => x !== s);
    const a = s.view.root.position;
    const b = other.view.root.position;
    const mid = a.clone().add(b).multiplyScalar(0.5);
    const from = mid.clone().add(g.debug.camera.position.clone().set(-14, 9, 26));
    const d = mid.clone().sub(from);
    g.inspectCam = [from.x, from.z, Math.atan2(-d.x, -d.z), Math.atan2(d.y, Math.hypot(d.x, d.z)), from.y - g.debug.game.groundY(from.x, from.z)];
  });
  await shot('ext_deployed', () => window.prg.look([7, 4.5, -4], [0, 2.2, 1.5]));
}

if (shots.length > 1) {
  const imgs = shots.map((f) => `data:image/png;base64,${readFileSync(f).toString('base64')}`);
  const cols = 3;
  const sheet = await browser.newPage({ viewport: { width: 640 * cols, height: 400 * Math.ceil(imgs.length / cols) } });
  await sheet.setContent(`<body style="margin:0;display:flex;flex-wrap:wrap;width:${640 * cols}px">${imgs.map((s) => `<img src="${s}" width=640 height=400>`).join('')}</body>`);
  await sheet.screenshot({ path: join(OUT, 'prg_sheet.png') });
  console.log('sheet', join(OUT, 'prg_sheet.png'));
}
if (errors.length) check('no page errors', false, errors.slice(0, 3).join(' | '));
writeFileSync(join(OUT, 'prg_report.txt'), report.join('\n') + '\n');
await browser.close();
console.log(failed ? `\n${failed} comprobación(es) fallida(s)` : '\ntodo OK');
process.exit(failed ? 1 : 0);
