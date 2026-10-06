// Diagnóstico para equilibrar: node pruebas/diag.js [semilla] [años] [sistemas]
const S = require('./cargar.js');
const semilla = +(process.argv[2] || 1), anios = +(process.argv[3] || 10), sistemas = +(process.argv[4] || 40);
const m = S.crear(semilla, { sistemas });
const B = S.B;
function foto() {
  let pob = 0, H = 0, hamb = 0, agr = 0, din = 0, nm = 0, neg = 0, R = 0, nr = 0, calle = 0;
  for (const a of m.ase) { pob += a.pob; H += a.H * a.pob; if (a.H > 0.2) hamb++; agr += a.agr * a.pob; if (a.est >= 0) { R += a.R; nr++; } if (a.f > 0.3) calle++; }
  for (const n of m.nav) if (n.vivo && n.cls === 'carguero' && !n.pirata) { din += n.dinero; nm++; if (n.dinero < 0) neg++; }
  const st = [0,0,0,0,0,0,0,0], dm = [0,0,0,0,0,0,0,0], pr = [0,0,0,0,0,0,0,0];
  for (const a of m.ase) for (let c = 0; c < 8; c++) { st[c] += a.alm[c]; dm[c] += a.dem[c]; pr[c] += a.pr[c] / S.bien[c].pref / m.ase.length; }
  let nuc = 0; for (const a of m.ase) nuc += a.nuc.length;
  const r = S.resumen(m);
  console.log('A' + String(m.anio()).padStart(3), 'pob', (pob / 1e6).toFixed(2) + 'M', 'H', (H / pob).toFixed(2), 'hamb', hamb, 'agr', (agr / pob).toFixed(2), 'R', (R / nr).toFixed(2), 'calle', calle,
    '| merc', nm, 'din', Math.round(din / nm / 1000) + 'k', 'neg', neg, 'pir', r.piratas, '| est', r.estados, 'fac', r.facciones, 'nuc', nuc,
    '| dias', st.map((s, c) => dm[c] > 0 ? Math.round(s / dm[c]) : '-').join(' '), '| p', pr.map(x => x.toFixed(1)).join(' '));
}
foto();
for (let y = 0; y < anios; y++) { m.avanzar(m.t + S.ANIO); foto(); }
console.log(S.resumen(m));
console.log('--- hambrientos');
for (const a of m.ase) if (a.H > 0.1) console.log(' ', a.nom.padEnd(20), a.tipo.padEnd(10), 'pob', Math.round(a.pob), 'H', a.H.toFixed(2), 'grano', Math.round(a.alm[0]), 'T', Math.round(S.eco.objetivo(a, 0)), 'p', Math.round(a.pr[0]), 'est', a.est, 'ctl', a.control.toFixed(2), 'agr', a.agr.toFixed(2), 'R', a.R.toFixed(2), 'f', a.f.toFixed(2), 'culpaReg', S.soc.parteCulpa(a, 'reg').toFixed(2));
console.log('--- estados');
for (const e of m.est) if (e.vivo) console.log(' ', e.nom.padEnd(34), 'tes', Math.round(e.tes), 'ing', Math.round(e.ultIngreso), 'gas', Math.round(e.ultGasto), 'P', e.mon.P.toFixed(2), 'msc', e.msc || 0, 'navG', e.navG, 'agr', (e.agr || 0).toFixed(2), 'deuda', Math.round(S.pol.deuda(m, e)), 'terr', S.pol.territorio(m, e).length);
