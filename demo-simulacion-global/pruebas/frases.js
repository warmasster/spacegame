// Valida los bancos de frases contra la especificación (nucleo/frases/00-espec.js).
// Uso: node pruebas/frases.js            → todos los archivos de nucleo/frases
//      node pruebas/frases.js origen     → solo el grupo «origen» (y exige que estén todos sus huecos)
//      node pruebas/frases.js --huecos origen → lista los huecos de un grupo con sus datos y su descripción
const fs = require('fs'); const path = require('path');
globalThis.SIM = globalThis.SIM || {};
const DIR = path.join(__dirname, '..', 'nucleo', 'frases');
require(path.join(DIR, '00-espec.js'));
const F = globalThis.SIM.frases;
const args = process.argv.slice(2);
if (args[0] === '--huecos') {
  for (const s of F.GRUPOS[args[1]] || []) { const e = F.ESPEC[s]; console.log(s + '\n    datos: ' + (e.tokens.length ? e.tokens.map(t => '{' + t + '}').join(' ') : '(ninguno)') + (e.forma !== 'frase' ? '   forma: ' + e.forma : '') + '\n    ' + e.que); }
  process.exit(0);
}
const grupo = args[0] || null;
let cargados = 0;
for (const f of fs.readdirSync(DIR).sort()) { if (f === '00-espec.js' || !/\.js$/.test(f)) continue; if (grupo && f.indexOf(grupo) < 0) continue; try { require(path.join(DIR, f)); cargados++; } catch (e) { console.log('✗ ' + f + ' no carga: ' + e.message); process.exit(1); } }
const errores = []; let n = 0; const GLOBALES = ['nom', 'pila'];
const huecos = grupo ? (F.GRUPOS[grupo] || []) : Object.keys(F.ESPEC);
if (grupo && !F.GRUPOS[grupo]) { console.log('✗ grupo desconocido: ' + grupo + ' (hay: ' + Object.keys(F.GRUPOS).join(', ') + ')'); process.exit(1); }
for (const s of Object.keys(F.banco)) if (!F.ESPEC[s]) errores.push(s + ': hueco que no está en la especificación');
for (const s of huecos) {
  const e = F.ESPEC[s]; const b = F.banco[s];
  if (!b) { if (grupo || cargados >= 4) errores.push(s + ': falta'); continue; }
  if (!Array.isArray(b) || b.length !== 5) { errores.push(s + ': tiene que ser una matriz de 5 filas (épica 0..4)'); continue; }
  const vistas = new Set();
  for (let i = 0; i < 5; i++) {
    if (!Array.isArray(b[i]) || b[i].length !== 3) { errores.push(s + ' fila ' + i + ': tiene que tener 3 columnas (drama 0..2)'); continue; }
    for (let j = 0; j < 3; j++) {
      const t = b[i][j]; const donde = s + ' [' + i + '][' + j + ']'; n++;
      if (typeof t !== 'string' || t.trim().length < 3) { errores.push(donde + ': vacía'); continue; }
      if (t !== t.trim()) errores.push(donde + ': espacios al principio o al final');
      if (t.length > 280) errores.push(donde + ': demasiado larga (' + t.length + ' > 280)');
      if (vistas.has(t)) errores.push(donde + ': repetida dentro del hueco'); vistas.add(t);
      if ((t.match(/\{/g) || []).length !== (t.match(/\}/g) || []).length) errores.push(donde + ': llaves sin cerrar');
      for (const x of t.match(/\{[^{}]*\}/g) || []) {
        const d = x.slice(1, -1);
        if (d.indexOf('|') >= 0) { if (d.split('|').length !== 2) errores.push(donde + ': ' + x + ' debe tener una sola barra (masculino|femenino)'); continue; }
        if (e.tokens.indexOf(d) < 0 && GLOBALES.indexOf(d) < 0) errores.push(donde + ': dato desconocido ' + x + ' (admite: ' + e.tokens.concat(GLOBALES).map(k => '{' + k + '}').join(' ') + ')');
      }
      const limpio = F.rellena(t, {}, 'H');
      if (e.forma === 'frase') { if (!/[.!?»…]$/.test(t)) errores.push(donde + ': debe acabar en punto'); if (!/^[A-ZÁÉÍÓÚÑÜ«¿¡{]/.test(t)) errores.push(donde + ': debe empezar en mayúscula'); }
      if (e.forma === 'nombre' && /[.]$/.test(t)) errores.push(donde + ': un nombre de época no lleva punto final');
      if (/\{\}/.test(limpio)) errores.push(donde + ': llaves vacías');
    }
  }
  // Los datos del hueco deben usarse: cada {dato} obligatorio tiene que salir en al menos 12 de las 15.
  for (const k of e.tokens) { let usa = 0; for (const fila of b) for (const t of fila) if (typeof t === 'string' && t.indexOf('{' + k + '}') >= 0) usa++; if (usa < 12) errores.push(s + ': el dato {' + k + '} solo sale en ' + usa + ' de 15 frases (mínimo 12)'); }
}
if (errores.length) { console.log(errores.slice(0, 60).map(x => '✗ ' + x).join('\n')); if (errores.length > 60) console.log('… y ' + (errores.length - 60) + ' más'); }
console.log((errores.length ? errores.length + ' errores · ' : 'todo bien · ') + n + ' frases en ' + Object.keys(F.banco).length + ' huecos' + (grupo ? ' (grupo ' + grupo + ': ' + huecos.length + ' huecos)' : ''));
process.exit(errores.length ? 1 : 0);
