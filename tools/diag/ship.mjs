// Ship diagnostics: functional checks of the modular ship (controls, power, interlocks, damage,
// repair, physics, walking in) + screenshots from fixed cameras. Exit code 1 if a check fails.
//
//   node tools/diag/ship.mjs [views|checks|all]      (default: all)
//
// Needs `npm run dev` and Playwright. Output: tools/diag/out/ship_*.png, ship_sheet.png, ship_report.txt
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { boot, progress } from './progress.mjs';

const pw = await import('playwright').catch(() => import('/opt/node22/lib/node_modules/playwright/index.mjs'));
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
const CHECK_BLOCKS = 11;
const VIEWS = 14;
const P = progress('nave', (mode === 'views' ? 0 : CHECK_BLOCKS) + (mode === 'checks' ? 0 : VIEWS));
await boot(page, `${url}/?manual&offline`);

// helpers inside the page
await page.evaluate(() => {
  const g = window.game;
  const THREE_V = (x, y, z) => g.debug.camera.position.clone().set(x, y, z);
  window.diag = {
    ship: () => g.ships[0],
    /** ship-space point → world */
    w(x, y, z) {
      return g.ships[0].view.root.localToWorld(THREE_V(x, y, z));
    },
    /** free camera at ship-space `from`, looking at ship-space `to` */
    look(from, to) {
      const a = this.w(...from);
      const b = this.w(...to);
      const d = b.clone().sub(a);
      const yaw = Math.atan2(-d.x, -d.z);
      const pitch = Math.atan2(d.y, Math.hypot(d.x, d.z));
      g.inspectCam = [a.x, a.z, yaw, pitch, a.y - g.debug.game.terrain.height(a.x, a.z)];
    },
    /** place the astronaut at ship-space `at` facing ship-space `to` (first person) */
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
    local(p) {
      return g.ships[0].local(p);
    },
    /** seat collision boxes in ship space (seats face the nose: yaw 0) */
    seatBoxes(i) {
      const st = g.ships[0].sim.def.seats[i];
      const out = {};
      for (const [k, b] of Object.entries(window.__SEAT_BOXES ?? {})) out[k] = { c: [st.root[0] + b.c[0], st.root[1] + b.c[1], st.root[2] + b.c[2]], half: b.half };
      return out;
    },
  };
});
await page.evaluate(async () => {
  window.__SEAT_BOXES = (await import('/src/shared/ship/def.ts')).SEAT_BOXES;
});

const report = [];
let failed = 0;
const check = (name, ok, detail = '') => {
  report.push(`${ok ? 'OK  ' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`);
  console.log(ok ? 'OK  ' : 'FAIL', name, detail);
  if (!ok) failed++;
};
const step = (n = 1) => page.evaluate((k) => window.game.step(k, 1 / 30, false), n);
const shots = [];
const shot = async (name, setup, hud = false) => {
  await page.evaluate((h) => {
    document.querySelector('#ui').style.display = h ? '' : 'none';
    document.querySelector('.pause')?.classList.add('hidden');
  }, hud);
  await page.evaluate(setup);
  await page.evaluate(() => window.game.step(6));
  await page.waitForTimeout(250);
  await page.evaluate(() => window.game.step(2));
  const f = join(OUT, `ship_${name}.png`);
  await page.screenshot({ path: f, timeout: 240000 });
  shots.push(f);
  P.step(`captura ${name}`);
};

if (mode === 'checks' || mode === 'all') {
  // --- controls & rules ----------------------------------------------------------------------
  const r = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const out = {};
    out.panels = s.sim.def.panels.length;
    out.controls = s.sim.def.controls.length;
    out.colliders = s.sim.def.panels.filter((p) => s.physics.hasPanel(p.index)).length;
    // every control does something (or refuses with a reason)
    out.acts = [];
    for (const c of s.sim.def.controls) {
      const before = s.sim.sw[c.key];
      const reason = g.shipControl(c.id);
      const after = s.sim.sw[c.key];
      out.acts.push({ id: c.id, key: c.key, before, after, reason });
      // restore
      if (!reason && c.action === 'toggle' && after !== before) g.shipControl(c.id);
    }
    return out;
  });
  // loose cargo settles on the deck before anything blows up
  const rest = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    g.step(60, 1 / 30, false);
    return s.cargo.bodies.map((b, i) => {
      const t = b.translation();
      return window.diag.local(g.debug.camera.position.clone().set(t.x, t.y, t.z)).y - s.sim.def.cargo[i].pos[1];
    });
  });
  // walking into a crate shoves it
  const push = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const b = s.cargo.bodies[6];
    const t0 = b.translation();
    const l0 = window.diag.local(g.debug.camera.position.clone().set(t0.x, t0.y, t0.z));
    window.diag.stand([l0.x, 0.02, l0.z + 1.2], [l0.x, 0.3, l0.z - 3]);
    g.step(5, 1 / 30, false);
    g.debug.input.setKey('KeyW', true);
    g.step(60, 1 / 30, false);
    g.debug.input.setKey('KeyW', false);
    g.step(20, 1 / 30, false);
    const t1 = b.translation();
    return Math.hypot(t1.x - t0.x, t1.z - t0.z);
  });
  check('walking into a crate pushes it', push > 0.5, `${push.toFixed(2)} m`);
  check('crates rest where they are stowed', rest.every((d) => Math.abs(d) < 0.06), rest.map((d) => d.toFixed(3)).join(' '));
  check('ship loaded', r.panels > 100 && r.controls > 20, `${r.panels} paneles, ${r.controls} mandos`);
  // exterior lights sit on the hull: a ray from 0.25 m out along the fixture normal meets a ship surface at the fixture
  const lights = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    return s.sim.def.extLights.map((l) => {
      const from = window.diag.w(l.pos[0] + l.n[0] * 0.25, l.pos[1] + l.n[1] * 0.25, l.pos[2] + l.n[2] * 0.25);
      const to = window.diag.w(...l.pos);
      const d = to.clone().sub(from).normalize();
      const hit = s.physics.castRay(from, d, 0.6);
      return { kind: l.kind, gap: hit ? hit.t - 0.25 : null };
    });
  });
  for (const l of lights) check(`ext light ${l.kind} mounted on the hull`, l.gap !== null && Math.abs(l.gap) < 0.06, l.gap === null ? 'flota: ninguna superficie detrás' : `${(l.gap * 100).toFixed(1)} cm`);
  check('every panel has a collider', r.colliders === r.panels, `${r.colliders}/${r.panels}`);
  for (const a of r.acts) {
    if (a.key === 'gear') check(`interlock ${a.id}`, !!a.reason && a.after === a.before, a.reason ?? 'no refusal');
    else if (a.key === 'caution') check(`control ${a.id}`, !a.reason, a.reason ?? 'reset');
    else check(`control ${a.id}`, !a.reason && a.after !== a.before, a.reason ?? `${a.key}: ${a.before} → ${a.after}`);
  }

  P.step('controles');
  // --- power: reactor off → buses dead, doors refuse, emergency lighting --------------------------
  const p = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const o = {};
    g.shipControl('cr.rct/reactor');
    o.reactor = s.sim.sw.reactor;
    o.lights = s.sim.powered('lights');
    o.door = g.shipControl('bk1.b/door.cockpit');
    g.shipControl('cr.rct/reactor');
    o.after = s.sim.powered('lights') && s.sim.powered('doors');
    // breaker: only its bus dies
    g.shipControl('cr.brk/brk.hyd');
    o.hyd = s.sim.powered('hyd');
    o.others = s.sim.powered('doors') && s.sim.powered('lights');
    o.ramp = g.shipControl('ext.ramp/ramp');
    g.shipControl('cr.brk/brk.hyd');
    o.rampOk = g.shipControl('ext.ramp/ramp') === null;
    g.shipControl('ext.ramp/ramp');
    return o;
  });
  check('reactor off kills power', p.reactor === 0 && !p.lights && !!p.door, `puerta: ${p.door}`);
  check('reactor on restores power', p.after);
  check('breaker isolates one bus', !p.hyd && p.others && !!p.ramp, `rampa: ${p.ramp}`);
  check('breaker closed → ramp works again', p.rampOk);

  P.step('energía');
  // --- movers: doors, ramp, shield travel + colliders follow ---------------------------------------
  const m = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const o = {};
    g.debug.controller.teleport(window.diag.w(0, 0.05, 12)); // out of the way
    g.shipControl('bk2.b/door.cargo');
    g.shipControl('ext.ramp/ramp');
    g.shipControl('ck.main/shield');
    g.step(200, 1 / 30, false);
    o.door = s.anim.doors['door.cargo'];
    o.ramp = s.anim.ramp;
    o.shield = s.anim.shield;
    // closed ramp blocks a ray through the rear opening
    const from = window.diag.w(0, 1.2, 8);
    const dir = window.diag.w(0, 1.2, 0).sub(from).normalize();
    o.rampBlocks = !!s.physics.castRay(from, dir, 4);
    g.shipControl('bk2.b/door.cargo');
    g.shipControl('ext.ramp/ramp');
    g.shipControl('ck.main/shield');
    g.step(200, 1 / 30, false);
    o.door2 = s.anim.doors['door.cargo'];
    o.ramp2 = s.anim.ramp;
    o.shield2 = s.anim.shield;
    const hit = s.physics.castRay(from, dir, 4);
    o.rampOpen = !hit || hit.t > 3.5;
    return o;
  });
  check('door closes', m.door === 0, `${m.door}`);
  check('ramp closes', m.ramp === 0, `${m.ramp}`);
  check('shield deploys', m.shield === 1, `${m.shield}`);
  check('closed ramp is solid', m.rampBlocks);
  check('movers return', m.door2 === 1 && m.ramp2 === 1 && m.shield2 === 0, `${m.door2} ${m.ramp2} ${m.shield2}`);
  check('open ramp lets you in', m.rampOpen);

  P.step('puertas y rampa');
  // before anything blows up: craters or thrown crates would change the ground at the ramp
  // --- walk up the ramp into the cargo bay ------------------------------------------------------------
  const wk = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const c = g.debug.controller;
    window.diag.stand([0, 0, 9.4], [0, 1.6, 0]);
    const p0 = c.position.clone();
    c.teleport(p0.setY(g.debug.game.terrain.height(p0.x, p0.z) + 0.05));
    g.step(10, 1 / 30, false);
    g.debug.input.setKey('KeyW', true);
    let maxLocalY = -9;
    for (let i = 0; i < 36; i++) {
      g.step(5, 1 / 30, false);
      maxLocalY = Math.max(maxLocalY, window.diag.local(c.position).y);
    }
    g.debug.input.setKey('KeyW', false);
    g.step(20, 1 / 30, false);
    const l = window.diag.local(c.position);
    return { x: l.x, y: l.y, z: l.z, zone: s.zoneAt(c.position.clone().setY(c.position.y + 1))?.id ?? 'fuera', grounded: c.grounded };
  });
  check('walks up the ramp onto the deck', Math.abs(wk.y) < 0.12 && wk.z < 5.4, `local ${wk.x.toFixed(2)}, ${wk.y.toFixed(2)}, ${wk.z.toFixed(2)} · ${wk.zone}`);
  check('ends inside the cargo bay', wk.zone === 'cargo' || wk.zone === 'corridor');
  P.step('subir la rampa andando');
  // --- damage: blasts blow a wall panel out; its collider goes; conduit cut kills its bus ------------
  const d = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const o = {};
    const target = s.sim.def.panels.find((p) => p.id === 'CG-R2-3');
    const out = window.diag.w(target.c[0] + target.n[0] * 0.2, target.c[1] + target.n[1] * 0.2, target.c[2] + target.n[2] * 0.2);
    g.blast(out);
    o.hp1 = s.sim.hp[target.index];
    g.blast(out);
    o.hp2 = s.sim.hp[target.index];
    o.hole = s.sim.hole(target.index);
    o.collider = s.physics.hasPanel(target.index);
    g.step(2, 1 / 30, false);
    // from inside the bay, out through the hole (the nacelle is right outside, so just check we pass the wall)
    const from = window.diag.w(target.c[0] - target.n[0] * 1.5, target.c[1] - target.n[1] * 1.5, target.c[2] - target.n[2] * 1.5);
    const to = window.diag.w(...target.c);
    const hit = s.physics.castRay(from, to.clone().sub(from).normalize(), 3.5);
    o.rayThrough = !hit || hit.t > 1.5 + target.t;
    o.caution = s.sim.sw.caution;
    // conduit: the hydraulic trunk under the cargo deck
    const trunk = s.sim.def.panels.find((p) => p.conduits.includes('hyd') && p.kind === 'floor' && p.zone === 'cargo');
    o.trunk = trunk.id;
    const top = window.diag.w(trunk.c[0], 0.3, trunk.c[2]);
    g.blast(top);
    g.blast(top);
    o.hyd = s.sim.powered('hyd');
    o.hydReason = g.shipControl('ck.main/ramp');
    return o;
  });
  check('blast damages a panel', d.hp1 < 100 && d.hp1 > 0, `hp ${d.hp1?.toFixed?.(1)}`);
  check('second blast blows it out', d.hole && d.hp2 === 0, `hp ${d.hp2}`);
  check('blown panel loses its collider', !d.collider);
  check('hole is open to rays', d.rayThrough);
  check('breach latches master caution', d.caution === 1);
  check(`conduit cut (${d.trunk}) kills hydraulics`, !d.hyd && /conducto/.test(d.hydReason ?? ''), d.hydReason);

  P.step('daño');
  // --- repair with the tool: aim at the hole, hold E ---------------------------------------------------
  const rp = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const target = s.sim.def.panels.find((p) => p.id === 'CG-R2-3');
    // stand inside the cargo bay facing the hole
    window.diag.stand([1.3, 0.05, target.c[2]], target.c);
    g.step(10, 1 / 30);
    const tgt = g.interaction.target;
    const o = { aimed: tgt ? `${tgt.kind}:${s.sim.def.panels[tgt.index]?.id}:${tgt.hole}:${tgt.inReach}` : 'none' };
    // repair tool: welder in hand, trigger held
    g.me.equip('welder');
    g.me.setArmed(true);
    g.step(20, 1 / 30);
    o.noToolBlocked = true;
    g.debug.input.setKey('Mouse0', true);
    g.step(45, 1 / 30);
    o.mid = s.sim.hp[target.index];
    o.midSolid = !s.sim.hole(target.index);
    o.collider = s.physics.hasPanel(target.index);
    g.step(150, 1 / 30);
    g.debug.input.setKey('Mouse0', false);
    o.final = s.sim.hp[target.index];
    // fix the trunk too
    const trunk = s.sim.def.panels.find((p) => p.conduits.includes('hyd') && p.kind === 'floor' && p.zone === 'cargo');
    for (let i = 0; i < 40; i++) g.debug.game.localRepair(s.id, trunk.index, 0.25);
    o.hyd = s.sim.powered('hyd');
    o.cut = s.sim.conduitCut('hyd')?.id ?? '-';
    g.shipControl('ck.main/caution');
    o.caution = s.sim.sw.caution;
    return o;
  });
  check('crosshair finds the hole', /^panel:CG-R2-3:true:true$/.test(rp.aimed), rp.aimed);
  check('welder + held click rebuilds the panel', rp.midSolid && rp.collider, `hp ${rp.mid?.toFixed?.(1)} a los 1.5 s`);
  check('repair reaches full integrity', rp.final >= 99.9, `hp ${rp.final?.toFixed?.(1)}`);
  check('repaired conduit restores hydraulics', rp.hyd, `corte: ${rp.cut}`);
  check('master caution reset', rp.caution === 0);

  P.step('soldadura');
  // --- click a real control through the crosshair ---------------------------------------------------
  const ck = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const c = s.sim.def.controls.find((x) => x.id === 'cg.ramp/light.cargo');
    window.diag.stand([c.c[0] + c.n[0] * 0.75, 0.02, c.c[2] + c.n[2] * 0.75], c.c);
    // aim from the real eye (the camera), like a player would
    for (let k = 0; k < 3; k++) {
      g.step(4, 1 / 30);
      const cam = g.debug.camera.getWorldPosition(g.debug.camera.position.clone());
      const d = window.diag.w(...c.c).sub(cam);
      g.debug.controller.yaw = Math.atan2(-d.x, -d.z);
      g.debug.controller.pitch = Math.atan2(d.y, Math.hypot(d.x, d.z));
    }
    g.step(2, 1 / 30);
    const before = s.sim.sw['light.cargo'];
    const t = g.interaction.target;
    const used = g.interaction.use(performance.now() / 1000);
    return { t: t ? `${t.kind}:${t.index}:${t.inReach}` : 'none', want: c.index, used, before, after: s.sim.sw['light.cargo'] };
  });
  check('crosshair click toggles cargo lights', ck.used && ck.after !== ck.before, `${ck.t} (quiero ${ck.want}) ${ck.before}→${ck.after}`);
  await page.evaluate(() => window.game.shipControl('cg.ramp/light.cargo'));

  P.step('clic por la mirilla');
  // --- welding needs the welder: with the launcher nothing happens ------------------------------------
  const nt = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const p = s.sim.def.panels.find((q) => q.id === 'CG-L1-4');
    g.blast(window.diag.w(p.c[0] + p.n[0] * 0.2, p.c[1] + p.n[1] * 0.2, p.c[2] + p.n[2] * 0.2));
    const hp0 = s.sim.hp[p.index];
    window.diag.stand([-1.3, 0.05, p.c[2]], p.c);
    g.me.equip('launcher');
    g.me.setArmed(true);
    g.step(15, 1 / 30);
    g.debug.input.setKey('Mouse0', true);
    g.step(30, 1 / 30);
    g.debug.input.setKey('Mouse0', false);
    const hp1 = s.sim.hp[p.index];
    for (let i = 0; i < 20; i++) g.debug.game.localRepair(s.id, p.index, 0.25);
    return { hp0, hp1 };
  });
  check('launcher does not weld', nt.hp1 === nt.hp0, `${nt.hp0} → ${nt.hp1}`);

  P.step('solo la soldadora repara');
  // --- cargo: loose boxes fall, get shoved by a blast ------------------------------------------------
  const cg = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    g.step(30, 1 / 30, false);
    const b = s.cargo.bodies[5];
    const p0 = b.translation();
    g.blast(g.debug.camera.position.clone().set(p0.x + 0.8, p0.y + 0.2, p0.z));
    g.step(60, 1 / 30, false);
    const p1 = b.translation();
    return { moved: Math.hypot(p1.x - p0.x, p1.z - p0.z) };
  });
  check('blast throws a crate', cg.moved > 0.3, `${cg.moved.toFixed(2)} m`);

  P.step('cajas');
  // --- seats ----------------------------------------------------------------------------------------
  const st = await page.evaluate(() => {
    const g = window.game;
    const s = g.ships[0];
    const seat = s.sim.def.seats[0];
    window.diag.stand([seat.root[0], 0.02, -6.9], [seat.root[0], 0.7, seat.root[2]]);
    for (let k = 0; k < 3; k++) {
      g.step(4, 1 / 30);
      const cam = g.debug.camera.getWorldPosition(g.debug.camera.position.clone());
      const d = window.diag.w(seat.root[0], 0.7, seat.root[2]).sub(cam);
      g.debug.controller.yaw = Math.atan2(-d.x, -d.z);
      g.debug.controller.pitch = Math.atan2(d.y, Math.hypot(d.x, d.z));
    }
    g.step(2, 1 / 30);
    const t = g.interaction.target;
    const aimed = t ? `${t.kind}:${t.index}:${t.inReach}` : 'none';
    g.interaction.use(performance.now() / 1000);
    g.debug.input.setKey('KeyW', true);
    g.step(40, 1 / 30);
    g.debug.input.setKey('KeyW', false);
    const sat = window.diag.local(g.debug.controller.position);
    const seated = !!g.seat;
    const eyeY = window.diag.local(g.me.eyePosition(g.debug.camera.position.clone())).y;
    // the seated suit (PLSS, helmet) must not cut into the seat frame: sample the hard-shell meshes
    // as rendered and test them against the seat boxes (a 1.5 cm tolerance for the padding)
    const V = (x, y, z) => g.debug.camera.position.clone().set(x, y, z);
    let worst = { d: 0, part: '-' };
    g.me.root.updateMatrixWorld(true);
    g.me.root.traverse((o) => {
      if (!o.isSkinnedMesh || !['SuitHard', 'Helmet', 'MetalDark'].includes(o.material.name)) return;
      const pos = o.geometry.getAttribute('position');
      for (let i = 0; i < pos.count; i += 5) {
        const l = s.local(o.getVertexPosition(i, V(0, 0, 0)).applyMatrix4(o.matrixWorld));
        for (const [name, b] of Object.entries(window.diag.seatBoxes(0))) {
          const q = [l.x - b.c[0], l.y - b.c[1], l.z - b.c[2]];
          const pen = Math.min(b.half[0] - Math.abs(q[0]), b.half[1] - Math.abs(q[1]), b.half[2] - Math.abs(q[2]));
          if (pen > worst.d) worst = { d: pen, part: `${o.material.name}→${name}` };
        }
      }
    });
    g.standUp();
    g.step(10, 1 / 30);
    return { aimed, seated, sat: sat.toArray(), eyeY, root: seat.root, exit: seat.exit, after: window.diag.local(g.debug.controller.position).toArray(), standing: !g.seat, clear: worst.d < 0.015, clearDetail: worst.d > 0 ? `peor ${worst.part} ${(worst.d * 100).toFixed(1)} cm` : 'sin contacto' };
  });
  check('crosshair finds the seat', /^seat:0:true$/.test(st.aimed), st.aimed);
  check('sits down and stays put', st.seated && Math.hypot(st.sat[0] - st.root[0], st.sat[2] - st.root[2]) < 0.05, `local ${st.sat.map((v) => v.toFixed(2))}`);
  check('seated eye height', st.eyeY > 1.0 && st.eyeY < 1.4, `${st.eyeY.toFixed(2)} m`);
  check('stands up at the exit', st.standing && Math.abs(st.after[2] - st.exit[2]) < 0.2, `local ${st.after.map((v) => v.toFixed(2))}`);
  check('seat holds the pack: PLSS clear of the seat frame', st.clear, st.clearDetail);

  P.step('asientos');
  // --- zoom -----------------------------------------------------------------------------------------
  const zm = await page.evaluate(() => {
    const g = window.game;
    const cam = g.debug.camera;
    g.debug.input.setKey('Mouse2', true);
    g.step(30, 1 / 30);
    const held = cam.fov;
    g.debug.input.setKey('Mouse2', false);
    g.step(30, 1 / 30);
    return { held, released: cam.fov };
  });
  check('right mouse zooms in', zm.held < 30 && zm.released > 65, `fov ${zm.held.toFixed(1)} → ${zm.released.toFixed(1)}`);

  P.step('zoom');
}

if (mode === 'views' || mode === 'all') {
  await page.evaluate(() => {
    const g = window.game;
    g.debug.controller.teleport(window.diag.w(6.5, 0, 14));
  });
  await shot('ext_rear34', () => window.diag.look([7, 1.0, 13], [0, 0.6, 1]));
  await shot('ext_side', () => window.diag.look([-11, 1.6, -3], [0, 0.8, -2]));
  await shot('ext_front', () => window.diag.look([5.5, 1.2, -15], [0, 1, -6]));
  await shot('ext_spawn', () => {
    const g = window.game;
    g.inspectCam = [0, 0, -0.451, -0.05, 1.7];
  });
  await shot('int_cargo', () => window.diag.look([0.8, 1.6, 5.2], [-0.2, 1.1, -2.4]));
  await shot('int_corridor', () => window.diag.look([0.3, 1.6, -2.0], [-0.6, 1.1, -6]));
  await shot('int_cockpit', () => window.diag.look([0.3, 1.6, -6.9], [0, 0.9, -9.4]));
  await shot('int_breakers', () => window.diag.look([0.6, 1.5, -4.4], [-1.5, 1.3, -4.4]));
  await shot('int_reactor', () => window.diag.look([-0.6, 1.5, -4.2], [1.5, 1.3, -4.3]));
  await shot('seated', () => {
    const g = window.game;
    const s = g.ships[0];
    g.sitDown(s, 0);
    window.diag.look([0.5, 1.45, -9.0], [-0.72, 0.8, -7.6]);
  });
  await shot('welder', () => {
    const g = window.game;
    g.standUp();
    g.me.equip('welder');
    g.me.setArmed(true);
    window.diag.stand([0.2, 0.02, 1.2], [2.6, 1.2, 1.2]);
    g.step(20, 1 / 30);
    window.diag.look([1.6, 1.4, 0.0], [0.3, 1.1, 1.2]);
  });
  await shot('slung', () => {
    const g = window.game;
    g.me.equip('launcher');
    g.me.setArmed(true);
    window.diag.stand([0.2, 0.02, 1.2], [2.6, 1.2, 1.2]);
    g.step(20, 1 / 30);
    window.diag.look([-1.3, 1.5, 2.4], [0.3, 1.1, 1.2]);
  });
  await shot('damage', () => {
    const g = window.game;
    const s = g.ships[0];
    for (const id of ['CG-R1-2', 'CG-R2-2', 'CK-R2-2']) {
      const p = s.sim.def.panels.find((q) => q.id === id);
      g.blast(window.diag.w(p.c[0] + p.n[0] * 0.2, p.c[1] + p.n[1] * 0.2, p.c[2] + p.n[2] * 0.2));
    }
    window.diag.look([8.5, 1.8, 2.5], [2.6, 1, -1]);
  });
}

if (shots.length > 1) {
  const imgs = shots.map((f) => `data:image/png;base64,${readFileSync(f).toString('base64')}`);
  const cols = 3;
  const sheet = await browser.newPage({ viewport: { width: 640 * cols, height: 400 * Math.ceil(imgs.length / cols) } });
  await sheet.setContent(`<body style="margin:0;display:flex;flex-wrap:wrap;width:${640 * cols}px">${imgs.map((s) => `<img src="${s}" width=640 height=400>`).join('')}</body>`);
  await sheet.screenshot({ path: join(OUT, 'ship_sheet.png') });
  console.log('sheet', join(OUT, 'ship_sheet.png'));
  P.step('hoja de contactos');
}
if (errors.length) check('no page errors', false, errors.slice(0, 3).join(' | '));
writeFileSync(join(OUT, 'ship_report.txt'), report.join('\n') + '\n');
await browser.close();
console.log(failed ? `\n${failed} comprobación(es) fallida(s)` : '\ntodo OK');
process.exit(failed ? 1 : 0);
