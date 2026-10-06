// Carga el núcleo en Node, en el mismo orden que index.html.
const path = require('path');
const ORDEN = ['00-base', '01-registros', '02-mundo', '07-objetos', '08-personas', '04-economia', '09-sociedad', '06-informacion', '05-naves', '10-facciones', '11-politica', '12-combate', '13-justicia', '14-dios', '16-tecnica', '17-teorias', '18-contenido', 'frases/00-espec', '19-historia', '03-generacion', '15-sim'];
// Los bancos de frases (vocabulario) que haya: sin ellos el mundo funciona igual, con texto llano.
const fs = require('fs'); for (const f of fs.readdirSync(path.join(__dirname, '..', 'nucleo', 'frases')).sort()) if (f !== '00-espec.js' && /\.js$/.test(f)) ORDEN.push('frases/' + f.replace(/\.js$/, ''));
for (const f of ORDEN) require(path.join(__dirname, '..', 'nucleo', f + '.js'));
module.exports = globalThis.SIM;
