// Atrás y adelante en un navegador de verdad: abre un tema, un hecho y una persona con clics reales, vuelve con el
// botón, con Alt + ← y con el botón lateral del ratón, y comprueba en cada paso dónde se está.
// Uso: node pruebas/foto-navegar.js [semilla]
const path = require('path'); const fs = require('fs');
const { chromium } = require('playwright');
const DIR = path.resolve(__dirname, '..', 'fotos'); fs.mkdirSync(DIR, { recursive: true });
const SEMILLA = process.argv[2] || '7919';

(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1400, height: 820 } });
  const errores = []; let mal = 0;
  p.on('console', (m) => { if (m.type() === 'error') errores.push('error: ' + m.text()); });
  p.on('pageerror', (e) => errores.push('ERROR: ' + e.message));
  const espera = (ms) => p.waitForTimeout(ms);
  const donde = () => p.textContent('#navDonde');
  const es = async (esperado, que) => { await espera(250); const d = await donde(); const ok = d.indexOf(esperado) === 0; if (!ok) mal++; console.log((ok ? '  ✓ ' : '  ✗ ') + que + ' → «' + d + '»' + (ok ? '' : ' (esperaba «' + esperado + '…»)')); };

  await p.goto('file:///' + path.resolve(__dirname, '..', 'index.html').replace(/\\/g, '/'));
  await p.fill('#semilla', SEMILLA); await p.selectOption('#prehistoria', '3'); await p.click('#crear');
  await p.waitForFunction(() => document.getElementById('inicio').classList.contains('oculto'), null, { timeout: 500000 });
  await espera(900); await p.keyboard.press('Space');

  await es('Ficha › El mundo', 'al empezar');
  await p.click('#contenido [data-tema="hambrunas"]'); await es('Ficha › Hambrunas', 'clic en la cifra de hambrunas');
  await p.evaluate(() => { document.getElementById('contenido').scrollTop = 600; }); await espera(150);
  await p.click('#contenido .ev >> nth=3'); await es('Historiador › ', 'clic en un hecho de la lista');
  await p.click('#contenido .gr-fila a.ref >> nth=0'); await es('Ficha › ', 'clic en un nombre dentro del historiador');
  await p.click('#pestanas button[data-p="estados"]'); await es('Estados', 'pestaña Estados');
  await p.screenshot({ path: path.join(DIR, '60-navegar.png') }); console.log('foto 60-navegar');
  await p.click('#navAtras'); await es('Ficha › ', 'botón atrás');
  await p.keyboard.press('Alt+ArrowLeft'); await es('Historiador › ', 'Alt + ←');
  // Playwright no sabe pulsar el botón lateral del ratón: se envía el evento.
  await p.evaluate(() => window.dispatchEvent(new MouseEvent('mouseup', { button: 3, bubbles: true, cancelable: true })));
  await es('Ficha › Hambrunas', 'botón lateral del ratón');
  const sc = await p.evaluate(() => document.getElementById('contenido').scrollTop); const okSc = Math.abs(sc - 600) < 40; if (!okSc) mal++; console.log((okSc ? '  ✓ ' : '  ✗ ') + 'vuelve a la altura donde estaba (' + Math.round(sc) + ' px)');
  await p.click('#navAtras'); await es('Ficha › El mundo', 'atrás hasta el principio');
  const dis = await p.evaluate(() => [document.getElementById('navAtras').disabled, document.getElementById('navAdelante').disabled]); const okD = dis[0] === true && dis[1] === false; if (!okD) mal++; console.log((okD ? '  ✓ ' : '  ✗ ') + 'al principio: atrás apagado, adelante encendido');
  await p.click('#navAdelante'); await p.keyboard.press('Alt+ArrowRight'); await es('Historiador › ', 'adelante dos veces');
  await p.click('#pestanas button[data-p="economia"]'); await es('Economía', 'mirar otra cosa');
  const dis2 = await p.evaluate(() => document.getElementById('navAdelante').disabled); if (!dis2) mal++; console.log((dis2 ? '  ✓ ' : '  ✗ ') + 'ya no hay «adelante»');

  console.log(errores.length ? '\n— CONSOLA —\n' + errores.slice(0, 20).join('\n') : '\nconsola limpia'); console.log(mal ? mal + ' comprobaciones mal' : 'todo bien');
  await b.close(); process.exit(mal || errores.length ? 1 : 0);
})().catch((e) => { console.error('FALLA', e); process.exit(1); });
