// Fotos del panel de batallas: provoca guerras, espera a que haya una batalla espacial y un combate en tierra
// en curso y fotografía la pestaña (entera) y el mapa de cerca.
// Uso: node pruebas/foto-batallas.js [semilla]
const path = require('path'); const fs = require('fs');
const { chromium } = require('playwright');
const DIR = path.resolve(__dirname, '..', 'fotos'); fs.mkdirSync(DIR, { recursive: true });
const SEMILLA = process.argv[2] || '7919';

(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1500, height: 1900 } });
  const errores = [];
  p.on('console', (m) => { if (m.type() === 'error') errores.push('error: ' + m.text()); });
  p.on('pageerror', (e) => { const s = 'ERROR: ' + e.message + '\n' + String(e.stack || '').split('\n').slice(0, 4).join('\n'); if (errores.indexOf(s) < 0) errores.push(s); });
  // El mapa se fotografía con una ventana normal; el panel, con una muy alta para que quepa la tarjeta entera.
  const foto = async (n) => { await p.setViewportSize({ width: 1400, height: 820 }); await p.waitForTimeout(500); await p.screenshot({ path: path.join(DIR, n + '.png') }); await p.setViewportSize({ width: 1500, height: 1900 }); await p.waitForTimeout(300); console.log('foto', n); };
  const panel = async (n) => { await (await p.$('#panel')).screenshot({ path: path.join(DIR, n + '.png') }); console.log('foto', n); };
  const espera = (ms) => p.waitForTimeout(ms);

  await p.goto('file:///' + path.resolve(__dirname, '..', 'index.html').replace(/\\/g, '/'));
  await p.fill('#semilla', SEMILLA); await p.selectOption('#prehistoria', '3'); await p.click('#crear');
  await p.waitForFunction(() => document.getElementById('inicio').classList.contains('oculto'), null, { timeout: 500000 });
  await p.keyboard.press('Space');                                             // en pausa: el tiempo lo movemos desde aquí
  // Guerras: que cada Estado tenga motivo contra un vecino.
  await p.evaluate(() => { const m = globalThis.mundo, S = globalThis.SIM; for (const e of m.est) if (e.vivo && S.dios.puede(m, 'casus_belli', e.id)) S.dios.hacer(m, 'casus_belli', e.id); });
  const avanza = (cond, maxAnios) => p.evaluate(([c, mx]) => {
    const m = globalThis.mundo; const fin = m.t + mx * 360; const ok = new Function('m', 'return ' + c);
    const t0 = Date.now(); while (m.t < fin && !ok(m) && Date.now() - t0 < 20000) m.avanzar(m.t + 1, 5000);
    globalThis.UI.estado.objetivo = m.t; globalThis.UI.estado.sucio = true; return { ok: !!ok(m), anio: (m.t / 360).toFixed(1) };
  }, [cond, maxAnios]);
  const hasta = async (cond, anios) => { let r; for (let i = 0; i < 40; i++) { r = await avanza(cond, anios); if (r.ok) break; } return r; };
  const pest = (t) => p.click('#pestanas button[data-p="' + t + '"]');

  // 1. Batalla espacial en su segundo día.
  let r = await hasta('m.bat.some(b => b.vivo && m.t - b.t0 >= 2 && b.L[0].n0 + b.L[1].n0 > 60)', 1);
  console.log('batalla espacial', JSON.stringify(r));
  if (r.ok) {
    const bt = await p.evaluate(() => { const m = globalThis.mundo, U = globalThis.UI; const b = m.bat.filter(x => x.vivo).sort((x, y) => y.L[0].n0 + y.L[1].n0 - x.L[0].n0 - x.L[1].n0)[0]; U.estado.abiertos.add('c' + b.id + '_0'); U.mapa.centrar(m, { t: 'S', id: b.sis }, 7); return { id: b.id, sis: b.sis }; });
    await pest('batallas'); await espera(900); await panel('20-batalla-panel'); await foto('21-batalla-mapa');
    await p.evaluate((x) => { const U = globalThis.UI; U.estado.sel = { t: 'S', id: x.sis }; }, bt); await pest('ficha'); await espera(700); await panel('22-batalla-ficha-sistema');
  }
  // 2. Combate en tierra.
  r = await hasta('(m.tie || []).some(t => t.vivo && m.t - t.t0 >= 3)', 1);
  console.log('combate en tierra', JSON.stringify(r));
  if (r.ok) {
    await p.evaluate(() => { const m = globalThis.mundo, U = globalThis.UI; const t = m.tie.filter(x => x.vivo).sort((x, y) => y.atk.n0 - x.atk.n0)[0]; U.mapa.centrar(m, { t: 'A', id: t.a }, 14); });
    await pest('batallas'); await espera(900); await panel('23-tierra-panel'); await foto('24-tierra-mapa');
  }
  // 3. Batallas ya acabadas: plegadas, y una abierta.
  r = await hasta('m.bat.some(b => !b.vivo) && m.tie.some(t => !t.vivo) && !m.bat.some(b => b.vivo)', 1);
  await p.evaluate(() => { const m = globalThis.mundo, U = globalThis.UI; const b = m.bat.filter(x => !x.vivo).pop(); if (b) U.estado.abiertos.add('b' + b.id); const t = m.tie.filter(x => !x.vivo).pop(); if (t) U.estado.abiertos.add('t' + t.id); U.estado.sucio = true; });
  await pest('batallas'); await espera(900); await panel('25-batallas-pasadas');

  console.log(errores.length ? '\n— CONSOLA —\n' + errores.slice(0, 20).join('\n') : '\nconsola limpia');
  await b.close();
})().catch((e) => { console.error('FALLA', e); process.exit(1); });
