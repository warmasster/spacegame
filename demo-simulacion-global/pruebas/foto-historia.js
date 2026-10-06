// Fotos de la ficha del mundo, de un par de temas abiertos, del historiador con ramas y del historial de una persona.
// Uso: node pruebas/foto-historia.js [años de prehistoria] [semilla]
const path = require('path'); const fs = require('fs');
const { chromium } = require('playwright');
const DIR = path.resolve(__dirname, '..', 'fotos'); fs.mkdirSync(DIR, { recursive: true });
const ANIOS = process.argv[2] || '12'; const SEMILLA = process.argv[3] || '7919';

(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1500, height: 2100 } });
  const errores = [];
  p.on('console', (m) => { if (m.type() === 'error') errores.push('error: ' + m.text()); });
  p.on('pageerror', (e) => { const s = 'ERROR: ' + e.message + '\n' + String(e.stack || '').split('\n').slice(0, 4).join('\n'); if (errores.indexOf(s) < 0) errores.push(s); });
  const panel = async (n) => { await (await p.$('#panel')).screenshot({ path: path.join(DIR, n + '.png') }); console.log('foto', n); };
  const espera = (ms) => p.waitForTimeout(ms);
  const pest = (t) => p.click('#pestanas button[data-p="' + t + '"]');

  await p.goto('file:///' + path.resolve(__dirname, '..', 'index.html').replace(/\\/g, '/'));
  await p.evaluate(() => { try { localStorage.clear(); } catch (e) { /* nada */ } }); await p.reload();
  await p.fill('#semilla', SEMILLA); await p.selectOption('#prehistoria', ANIOS); await p.click('#crear');
  await p.waitForFunction(() => document.getElementById('inicio').classList.contains('oculto'), null, { timeout: 560000 });
  await espera(1200); await p.keyboard.press('Space');
  console.log('sueltos:', await p.evaluate(() => globalThis.UI.paneles.sueltos(globalThis.mundo).map(k => k + ' ' + globalThis.mundo.cuenta[k]).join(', ')));

  await pest('ficha'); await espera(600); await panel('50-mundo');
  await p.click('#contenido [data-tema="masacres"]'); await espera(700); await panel('51-tema-masacres');
  await p.click('#contenido [data-accion="temaFuera"]'); await espera(300); await p.click('#contenido [data-tema="calle"]'); await espera(700); await panel('52-tema-calle');
  await p.click('#contenido [data-accion="temaFuera"]'); await espera(300); await p.click('#contenido [data-tema="abordajes"]'); await espera(700); await panel('53-tema-abordajes');
  // Historiador: un hecho gordo con causas y consecuencias.
  const ev = await p.evaluate(() => { const m = globalThis.mundo; let mejor = -1, pt = -1; for (const e of m.ev) { if (e.imp < 2) continue; const c = m.cadena(e.id, 30).length; const f = (m.efe[e.id] || []).length; const s = Math.min(c, 12) + Math.min(f, 8) * 1.5; if (s > pt) { pt = s; mejor = e.id; } } globalThis.UI.estado.evSel = mejor; return mejor; });
  console.log('hecho', ev); await pest('historiador'); await espera(800); await panel('54-historiador-ramas');
  // Una persona con historia larga, filtrada por «notable».
  await p.evaluate(() => { const m = globalThis.mundo, U = globalThis.UI; let mejor = null, pt = -1; for (const [id, l] of m.bio) { const n = l.filter(i => m.ev[i].imp >= 1).length; if (n > pt) { pt = n; mejor = id; } } U.estado.sel = { t: 'P', id: mejor }; U.estado.bio.imp = 1; });
  await pest('ficha'); await espera(800); await panel('55-persona-historia');

  console.log(errores.length ? '\n— CONSOLA —\n' + errores.slice(0, 20).join('\n') : '\nconsola limpia');
  await b.close();
})().catch((e) => { console.error('FALLA', e); process.exit(1); });
