// Barrido de semillas: «simula N semillas sin gráficos y mide la diversidad». node pruebas/semillas.js [n] [años] [sistemas]
const S = require('./cargar.js');
const n = +(process.argv[2] || 3), anios = +(process.argv[3] || 25), sistemas = +(process.argv[4] || 40);
const CL = ['estados', 'naves', 'piratas', 'facciones', 'guerras', 'civiles', 'batallas', 'conquistas', 'hambrunas', 'revoluciones', 'masacres', 'cismas', 'fusiones', 'faccionesNacidas', 'huelgas', 'explosiones', 'robos', 'abordajes', 'motines', 'sucesiones', 'fragmentaciones', 'venganzas', 'condenas', 'profecias', 'independencias', 'golpes', 'caidos'];
console.log('semilla  seg  pob(M) calle ' + CL.map(c => c.slice(0, 6).padStart(7)).join(''));
for (let s = 1; s <= n; s++) {
  const t0 = Date.now(); const m = S.crear(s * 7919, { sistemas }); m.avanzar(anios * S.ANIO);
  const r = S.resumen(m); let calle = 0; for (const a of m.ase) if (a.f > 0.3) calle++;
  console.log(String(s * 7919).padStart(7), String(Math.round((Date.now() - t0) / 1000)).padStart(4), (r.pob / 1e6).toFixed(2).padStart(7), String(calle).padStart(5), CL.map(c => String(r[c]).padStart(7)).join(''));
}
