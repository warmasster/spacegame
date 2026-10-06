// Fotos de verdad: abre index.html en un Chromium sin ventana, recoge los errores de consola y guarda capturas.
// Uso: node pruebas/foto.js [años de prehistoria] [semilla]
const path = require('path'); const fs = require('fs');
const { chromium } = require('playwright');
const DIR = path.resolve(__dirname, '..', 'fotos'); fs.mkdirSync(DIR, { recursive: true });
const ANIOS = process.argv[2] || '3'; const SEMILLA = process.argv[3] || '2291';

(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1600, height: 900 } });
  const errores = [];
  p.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') errores.push(m.type() + ': ' + m.text()); });
  p.on('pageerror', (e) => { const s = 'ERROR: ' + e.message + '\n' + String(e.stack || '').split('\n').slice(0, 4).join('\n'); if (errores.filter(x => x === s).length < 1) errores.push(s); });
  p.on('requestfailed', (r) => errores.push('no carga: ' + r.url()));
  const foto = async (n) => { await p.screenshot({ path: path.join(DIR, n + '.png') }); console.log('foto', n); };
  const espera = (ms) => p.waitForTimeout(ms);

  await p.goto('file:///' + path.resolve(__dirname, '..', 'index.html').replace(/\\/g, '/'));
  await espera(400); await foto('01-inicio');
  await p.fill('#semilla', SEMILLA); await p.selectOption('#prehistoria', ANIOS);
  await p.click('#crear'); await espera(2500); await foto('02-prehistoria');
  await p.waitForFunction(() => document.getElementById('inicio').classList.contains('oculto'), null, { timeout: 500000 });
  await espera(1500); await foto('03-galaxia');

  const ev = (f, a) => p.evaluate(f, a);
  for (const capa of ['rebeliones', 'relaciones', 'comercio', 'estrategia', 'hambre']) {
    await p.selectOption('#capa', capa); await espera(500); await foto('04-capa-' + capa);
  }
  await p.selectOption('#capa', 'politico');
  for (const t of ['cronica', 'estados', 'facciones', 'personas', 'arsenal', 'batallas', 'economia', 'teorias', 'sismografo']) {
    await p.click('#pestanas button[data-p="' + t + '"]'); await espera(700); await foto('05-' + t);
  }
  // Un sistema, un asentamiento en plano y, si la hay, una batalla.
  const info = await ev(() => {
    const m = globalThis.mundo; const a = m.ase.filter(x => x.vivo !== false).sort((x, y) => y.pob - x.pob)[0];
    const bt = m.bat.find(x => x.vivo); const ti = m.tie.find(x => x && x.vivo);
    const rey = m.est.find(e => e.vivo && e.gob >= 0);
    return { a: a.id, sis: a.sis, bat: bt ? bt.sis : -1, tie: ti ? ti.a : -1, rey: rey ? rey.gob : -1, est: rey ? rey.id : -1 };
  });
  console.log('info', JSON.stringify(info));
  await p.keyboard.press('Space');
  await ev((i) => { const U = globalThis.UI; U.mapa.centrar(globalThis.mundo, { t: 'S', id: i.sis }, 3.4); }, info); await espera(600); await foto('06-sistema');
  await ev((i) => { const U = globalThis.UI; U.mapa.centrar(globalThis.mundo, { t: 'A', id: i.a }, 7.5); }, info); await espera(600); await foto('07-asentamiento');
  await ev((i) => { const U = globalThis.UI; U.mapa.centrar(globalThis.mundo, { t: 'A', id: i.a }, 14); }, info); await espera(900); await foto('08-plano');
  if (info.bat >= 0) { await ev((i) => { globalThis.UI.mapa.centrar(globalThis.mundo, { t: 'S', id: i.bat }, 5.5); }, info); await espera(900); await foto('09-batalla'); }
  if (info.tie >= 0) { await ev((i) => { globalThis.UI.mapa.centrar(globalThis.mundo, { t: 'A', id: i.tie }, 14); }, info); await espera(900); await foto('10-tierra'); }
  // Fichas: un gobernante, su Estado y una nave de guerra.
  const ficha = async (ref, n) => { await ev((r) => { const U = globalThis.UI; U.estado.sel = r; U.estado.pest = 'ficha'; for (const b of document.getElementById('pestanas').children) b.classList.toggle('activa', b.dataset.p === 'ficha'); U.estado.sucio = true; }, ref); await espera(700); await foto(n); };
  if (info.rey >= 0) await ficha({ t: 'P', id: info.rey }, '11-ficha-persona');
  if (info.est >= 0) await ficha({ t: 'E', id: info.est }, '12-ficha-estado');
  const nav = await ev(() => { const m = globalThis.mundo; const n = m.nav.find(x => x && x.vivo !== false && x.dis); return n ? n.id : -1; });
  if (nav >= 0) await ficha({ t: 'N', id: nav }, '13-ficha-nave');
  await ficha({ t: 'A', id: info.a }, '14-ficha-asentamiento');

  console.log(errores.length ? '\n— CONSOLA —\n' + [...new Set(errores)].slice(0, 30).join('\n') : '\nconsola limpia');
  await b.close();
})().catch((e) => { console.error('FALLA', e); process.exit(1); });
