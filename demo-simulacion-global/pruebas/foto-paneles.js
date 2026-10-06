// Fotos de las pestañas de datos (panel entero, ventana alta), del panel de aspecto y de un tema alternativo.
// Uso: node pruebas/foto-paneles.js [años de prehistoria] [semilla]
const path = require('path'); const fs = require('fs');
const { chromium } = require('playwright');
const DIR = path.resolve(__dirname, '..', 'fotos'); fs.mkdirSync(DIR, { recursive: true });
const ANIOS = process.argv[2] || '12'; const SEMILLA = process.argv[3] || '7919';

(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1500, height: 1700 } });
  const errores = [];
  p.on('console', (m) => { if (m.type() === 'error') errores.push('error: ' + m.text()); });
  p.on('pageerror', (e) => { const s = 'ERROR: ' + e.message + '\n' + String(e.stack || '').split('\n').slice(0, 4).join('\n'); if (errores.indexOf(s) < 0) errores.push(s); });
  const panel = async (n) => { await (await p.$('#panel')).screenshot({ path: path.join(DIR, n + '.png') }); console.log('foto', n); };
  const todo = async (n, w, h) => { await p.setViewportSize({ width: w || 1400, height: h || 820 }); await p.waitForTimeout(500); await p.screenshot({ path: path.join(DIR, n + '.png') }); await p.setViewportSize({ width: 1500, height: 1700 }); await p.waitForTimeout(300); console.log('foto', n); };
  const espera = (ms) => p.waitForTimeout(ms);
  const pest = (t) => p.click('#pestanas button[data-p="' + t + '"]');

  await p.goto('file:///' + path.resolve(__dirname, '..', 'index.html').replace(/\\/g, '/'));
  await p.evaluate(() => { try { localStorage.clear(); } catch (e) { /* nada */ } });
  await p.reload();
  await p.fill('#semilla', SEMILLA); await p.selectOption('#prehistoria', ANIOS); await p.click('#crear');
  await p.waitForFunction(() => document.getElementById('inicio').classList.contains('oculto'), null, { timeout: 560000 });
  await espera(1200); await p.keyboard.press('Space');

  await pest('ficha'); await espera(600); await panel('30-mundo');
  await pest('sismografo'); await espera(600); await panel('31-sismografo');
  // El cursor sobre una gráfica.
  const g1 = await p.$('#contenido canvas.graf'); if (g1) { const r = await g1.boundingBox(); await p.mouse.move(r.x + r.width * 0.62, r.y + r.height * 0.5); await espera(200); await (await p.$('#contenido')).screenshot({ path: path.join(DIR, '32-sismografo-cursor.png'), clip: undefined }); console.log('foto 32-sismografo-cursor'); await p.mouse.move(300, 300); }
  await pest('estados'); await espera(600); await panel('33-estados');
  await pest('economia'); await espera(600); await panel('34-economia');
  await pest('facciones'); await espera(600); await panel('35-facciones');
  // Personas: los más mortíferos, con la cuenta del primero y de otro que haya matado con sus manos.
  await p.evaluate(() => { const m = globalThis.mundo, S = globalThis.SIM, U = globalThis.UI; const l = m.per.filter(x => x.mue && x.mue.length).sort((x, y) => S.mas.letalidad(y) - S.mas.letalidad(x)); if (l[0]) U.estado.abiertos.add('k' + l[0].id); const q = l.slice(1, 12).find(x => x.mue.some(y => y.c === 'persona')); if (q) U.estado.abiertos.add('k' + q.id); });
  await pest('personas'); await espera(700); await panel('36-personas-muertes');
  await pest('arsenal'); await espera(600); await panel('37-arsenal');
  // Aspecto: el panel abierto, y luego otro tema con otra paleta y barras.
  await pest('sismografo'); await p.click('#btnAspecto'); await espera(400); await todo('38-aspecto');
  await p.click('#aspecto [data-pref="tema"][data-v="nebulosa"]'); await p.click('#aspecto [data-pref="acento"][data-v="cian"]'); await p.click('#aspecto [data-pref="paleta"][data-v="segura"]'); await p.click('#aspecto [data-pref="graf"][data-v="barras"]'); await p.click('#aspecto [data-pref="ancho"][data-v="560"]'); await espera(500); await todo('39-aspecto-nebulosa');
  await p.click('#btnAspecto'); await pest('estados'); await espera(500); await todo('40-estados-nebulosa');
  await p.click('#btnAspecto'); await p.click('#aspecto [data-pref="restablecer"]'); await p.click('#btnAspecto');

  console.log(errores.length ? '\n— CONSOLA —\n' + errores.slice(0, 20).join('\n') : '\nconsola limpia');
  await b.close();
})().catch((e) => { console.error('FALLA', e); process.exit(1); });
