// Prueba de humo: genera un mundo y lo deja correr. Uso: node pruebas/humo.js [semilla] [años] [sistemas] [v]
const S = require('./cargar.js');
const semilla = +(process.argv[2] || 1), anios = +(process.argv[3] || 10), sistemas = +(process.argv[4] || 40);
const verboso = process.argv[5] === 'v';
const t0 = Date.now();
const m = S.crear(semilla, { sistemas });
console.log('generado en', Date.now() - t0, 'ms:', m.texto(m.ev[m.ev.length - 1]));
const t1 = Date.now();
S.prehistoria(m, anios, (mm) => { if (mm.anio() % 5 === 0) process.stdout.write(' ' + mm.anio()); });
console.log('\n' + anios + ' años en', Date.now() - t1, 'ms');
console.log(S.resumen(m));
if (verboso) for (const e of m.ev) if (e.imp >= 2) console.log(m.fecha(e.t), '·', m.texto(e));
