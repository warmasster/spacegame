// Fotos de la pestaña Historia, de la biografía de la persona con más peso y de las capas de calor del mapa.
// Uso: node pruebas/foto-figuras.js [años de prehistoria] [semilla]
const path = require('path'); const fs = require('fs');
const { chromium } = require('playwright');
const DIR = path.resolve(__dirname, '..', 'fotos'); fs.mkdirSync(DIR, { recursive: true });
const ANIOS = process.argv[2] || '30'; const SEMILLA = process.argv[3] || '7919';

(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1500, height: 2300 } });
  const errores = [];
  p.on('console', (m) => { if (m.type() === 'error') errores.push('error: ' + m.text()); });
  p.on('pageerror', (e) => { const s = 'ERROR: ' + e.message + '\n' + String(e.stack || '').split('\n').slice(0, 4).join('\n'); if (errores.indexOf(s) < 0) errores.push(s); });
  const panel = async (n) => { await (await p.$('#panel')).screenshot({ path: path.join(DIR, n + '.png') }); console.log('foto', n); };
  const todo = async (n) => { await p.setViewportSize({ width: 1400, height: 820 }); await p.waitForTimeout(600); await p.screenshot({ path: path.join(DIR, n + '.png') }); await p.setViewportSize({ width: 1500, height: 2300 }); await p.waitForTimeout(300); console.log('foto', n); };
  const espera = (ms) => p.waitForTimeout(ms);
  const pest = (t) => p.click('#pestanas button[data-p="' + t + '"]');

  await p.goto('file:///' + path.resolve(__dirname, '..', 'index.html').replace(/\\/g, '/'));
  await p.evaluate(() => { try { localStorage.clear(); } catch (e) { /* nada */ } }); await p.reload();
  await p.fill('#semilla', SEMILLA); await p.selectOption('#prehistoria', ANIOS); await p.click('#crear');
  await p.waitForFunction(() => document.getElementById('inicio').classList.contains('oculto'), null, { timeout: 580000 });
  await espera(1200); await p.keyboard.press('Space');
  console.log(await p.evaluate(() => { const m = globalThis.mundo, c = m.cuenta; return 'figuras ' + (m.figuras || []).length + ' · credos ' + (m.cre || []).length + ' · peste ' + (c.peste || 0) + ' · martirios ' + (c.martirio || 0) + ' · oleadas ' + (c.oleada || 0) + ' · bancos de frases ' + Object.keys(globalThis.SIM.frases.banco).length; }));

  await pest('historia'); await espera(900); await panel('70-historia');
  const g1 = await p.$('#contenido canvas[data-calor]'); if (g1) { const r = await g1.boundingBox(); await p.mouse.move(r.x + r.width * 0.7, r.y + r.height * 0.4); await espera(250); await g1.screenshot({ path: path.join(DIR, '71-calor-cursor.png') }); console.log('foto 71-calor-cursor'); await p.mouse.move(300, 300); }
  // La persona de más peso: su ficha con la biografía.
  await p.evaluate(() => { const m = globalThis.mundo, U = globalThis.UI, q = globalThis.SIM.his.pesos(m); U.estado.sel = { t: 'P', id: q.orden[0] }; });
  await pest('ficha'); await espera(900); await panel('72-biografia-legendaria');
  // Alguien muerto de mala manera y con peso: para ver la muerte contada con su cadena.
  await p.evaluate(() => { const m = globalThis.mundo, U = globalThis.UI, q = globalThis.SIM.his.pesos(m); const id = q.orden.find(i => !m.per[i].vivo && m.per[i].evMuerte >= 0 && m.ev[m.per[i].evMuerte].d.modo !== 'natural'); if (id !== undefined) U.estado.sel = { t: 'P', id }; U.estado.sucio = true; });
  await espera(900); await panel('73-biografia-muerte');
  // Capas de calor.
  for (const capa of ['fama', 'credo', 'sangre', 'memoria']) { await p.selectOption('#capa', capa); await espera(400); await todo('74-capa-' + capa); }
  await p.selectOption('#capa', 'politico');

  console.log(errores.length ? '\n— CONSOLA —\n' + errores.slice(0, 20).join('\n') : '\nconsola limpia');
  await b.close();
})().catch((e) => { console.error('FALLA', e); process.exit(1); });
