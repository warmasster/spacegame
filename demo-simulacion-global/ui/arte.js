// Arte procedural: retratos, banderas, planos de nave e iconos de armas. Todo sale de una semilla y de los
// datos de la simulación (edad, rasgos, cargo, cicatrices, diseño, calidad de cada componente): lo que se ve es lo que hay.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI = g.UI || {};
  const A = U.arte = {};
  const TAU = Math.PI * 2;
  const hex = (c) => { if (c[0] === '#') { const n = parseInt(c.slice(1), 16); return [n >> 16 & 255, n >> 8 & 255, n & 255]; } const x = /(\d+)\D+(\d+)\D+(\d+)/.exec(c); return x ? [+x[1], +x[2], +x[3]] : [128, 128, 128]; };
  const rgb = (r, g2, b) => 'rgb(' + Math.round(S.clamp(r, 0, 255)) + ',' + Math.round(S.clamp(g2, 0, 255)) + ',' + Math.round(S.clamp(b, 0, 255)) + ')';
  const tono = A.tono = (c, k) => { const [r, g2, b] = hex(c); return k >= 0 ? rgb(r + (255 - r) * k, g2 + (255 - g2) * k, b + (255 - b) * k) : rgb(r * (1 + k), g2 * (1 + k), b * (1 + k)); };
  const mezcla = (c1, c2, k) => { const a = hex(c1), b = hex(c2); return rgb(a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k); };
  const rgba = A.rgba = (c, al) => { const [r, g2, b] = hex(c); return 'rgba(' + r + ',' + g2 + ',' + b + ',' + al + ')'; };
  const PIEL = ['#f6d7c3', '#edc4a8', '#dba983', '#c68f6a', '#a9714f', '#87563a', '#6b4230', '#4f3024'];
  const PELO = ['#1b1512', '#2e211a', '#4a3222', '#6b4526', '#8b5a2b', '#b07a3f', '#d6b36a', '#a13d1e', '#3a3a3a'];
  const IRIS = ['#3b2a1a', '#5b3a1e', '#2f4f6f', '#3f6f4f', '#6f6f6f', '#1f1f1f', '#4a6a8a'];

  // ── Descriptor de un retrato a partir de lo que la simulación sabe de la persona.
  A.descPersona = function (m, p) {
    const R = S.R; const e = p.est >= 0 && m.est[p.est] ? m.est[p.est] : null; const f = p.fac >= 0 ? m.fac[p.fac] : null;
    const tipo = p.cargo ? p.cargo.t : '';
    let rol = p.rol; if (tipo === 'gobernante') rol = 'gobernante';
    return {
      seed: p.avatar !== undefined ? p.avatar : S.hash(m.semilla, 'avatar', p.id), sexo: p.sexo, edad: (p.vivo ? m.t : p.muere) / S.ANIO - p.nace / S.ANIO, rol, vivo: p.vivo,
      col: e ? e.col : (f && f.tipo !== 'casa' ? '#7a2f2f' : '#4a5670'), gob: e ? e.tipoGob : '', rencor: p.r[R.REN], empatia: p.r[R.EMP], valor: p.r[R.VAL], prudencia: p.r[R.PRU], fe: p.r[R.FE],
      cicatrices: Math.min(3, (p.bajas || 0) > 0 ? 1 + ((p.batallas || 0) > 2 ? 1 : 0) : (p.batallas || 0) > 1 ? 1 : 0) + (p.ven && p.ven.estado === 'activa' ? 0 : 0),
      medallas: Math.min(6, (p.victorias || 0) + (tipo === 'almirante' || tipo === 'general' ? 1 : 0)), muescas: f && f.hecho && f.hecho.muertos >= 2 && f.hecho.muertos < 30 ? f.hecho.muertos : 0,
      rico: p.din > 15000, luto: !!(p.ven && p.ven.estado === 'activa'),
    };
  };
  A.descCara = function (m, a, c) { const co = m.coh[c.coh]; return { seed: S.hash(m.semilla, 'avatar-cara', c.coh, c.i), sexo: c.sexo, edad: c.edad, rol: co.arca ? 'predicador' : co.ins >= 0 ? 'obrero' : 'civil', vivo: true, col: a.est >= 0 ? m.est[a.est].col : '#4a5670', gob: '', rencor: c.odia ? 0.75 : 0.3, empatia: 0.5, valor: 0.5, prudencia: 0.5, fe: 0.5, cicatrices: 0, medallas: 0, muescas: 0 }; };
  A.descSoldado = function (m, u, i) { const r = new S.Rng(S.hash(m.semilla, 'soldado', u.id, i)); return { seed: S.hash(m.semilla, 'avatar-sold', u.id, i), sexo: r.p(0.3) ? 'M' : 'H', edad: 19 + r.i(22), rol: 'soldado', vivo: true, col: u.est >= 0 ? m.est[u.est].col : '#6a7a5a', gob: '', rencor: 0.4, empatia: 0.4, valor: 0.7, prudencia: 0.5, fe: 0.5, cicatrices: r.p((u.doc || 0.3) * 0.5) ? 1 : 0, medallas: 0, muescas: 0, nom: S.per.nombre(r, r.p(0.3) ? 'M' : 'H') }; };

  // ── Retrato. Se dibuja en una caja de 100×100 escalada al tamaño pedido.
  A.avatar = function (c, x, y, s, d) {
    const r = new S.Rng(d.seed); const H = d.sexo !== 'M'; const edad = d.edad;
    c.save(); c.translate(x, y); c.scale(s / 100, s / 100);
    c.beginPath(); c.rect(0, 0, 100, 100); c.clip();
    // Fondo.
    const gr = c.createLinearGradient(0, 0, 0, 100); gr.addColorStop(0, tono(d.col, -0.45)); gr.addColorStop(1, tono(d.col, -0.78)); c.fillStyle = gr; c.fillRect(0, 0, 100, 100);
    c.strokeStyle = 'rgba(255,255,255,0.05)'; c.lineWidth = 3; for (let i = -100; i < 100; i += 14) { c.beginPath(); c.moveTo(i, 100); c.lineTo(i + 100, 0); c.stroke(); }
    const piel = PIEL[r.i(PIEL.length)]; const sombra = tono(piel, -0.2), oscura = tono(piel, -0.38);
    let pelo = PELO[r.i(PELO.length)]; pelo = mezcla(pelo, '#d9d9d9', S.clamp((edad - 42) / 30, 0, 1));
    const ancho = 25 + r.r(-3, 3.5) + (H ? 1.5 : 0), alto = 33 + r.r(-3, 3), mand = r.r(0.72, 1.0) + (H ? 0.06 : -0.04);
    const estilo = r.i(10), barba = H && edad > 18 ? r.i(9) : 0, cejaG = 1.5 + r.r(0, 1.6) + (H ? 0.4 : 0), ojoSep = 11 + r.r(-1.3, 1.8), ojoW = 5.4 + r.r(-0.9, 1.1), ojoH = 3.1 + r.r(-0.6, 0.9), iris = IRIS[r.i(IRIS.length)];
    const narL = 10 + r.r(-2, 3), narW = 4.5 + r.r(-1, 2.2), bocaW = 12 + r.r(-2.5, 3.5), bocaY = 69 + r.r(-1.5, 2), pecas = r.p(0.18), lunar = r.p(0.15), gafas = r.p(0.1 + (d.prudencia > 0.8 ? 0.15 : 0)), implante = r.p(0.09), pend = r.p(H ? 0.1 : 0.4), calvo = H && edad > 38 && r.p(0.3 + (edad - 38) / 80);
    const oy = 47 + r.r(-1, 1.5);
    // Ropa (hombros).
    A.ropa(c, d, r, piel);
    // Cuello y orejas.
    c.fillStyle = sombra; c.fillRect(42, 66, 16, 18);
    c.fillStyle = piel; for (const sg of [-1, 1]) { c.beginPath(); c.ellipse(50 + sg * (ancho + 0.5), oy + 3, 3.6, 6, 0, 0, TAU); c.fill(); }
    if (pend) { c.fillStyle = d.rico ? '#e8c860' : '#c0c8d0'; c.beginPath(); c.arc(50 - ancho - 1, oy + 10, 1.5, 0, TAU); c.fill(); }
    // Pelo por detrás (melenas).
    c.fillStyle = pelo;
    if (!calvo && (estilo === 3 || estilo === 6 || (!H && estilo < 5))) { c.beginPath(); c.moveTo(50 - ancho - 5, oy - 8); c.quadraticCurveTo(50, oy - 44, 50 + ancho + 5, oy - 8); c.lineTo(50 + ancho + 7, 84); c.quadraticCurveTo(50, 78, 50 - ancho - 7, 84); c.closePath(); c.fill(); }
    // Cara.
    c.fillStyle = piel; c.beginPath(); c.moveTo(50 - ancho, oy - 2);
    c.bezierCurveTo(50 - ancho, oy - 32, 50 + ancho, oy - 32, 50 + ancho, oy - 2);
    c.bezierCurveTo(50 + ancho, oy + alto * 0.62, 50 + ancho * mand * 0.78, oy + alto * 0.92, 50, oy + alto);
    c.bezierCurveTo(50 - ancho * mand * 0.78, oy + alto * 0.92, 50 - ancho, oy + alto * 0.62, 50 - ancho, oy - 2); c.fill();
    const sh = c.createLinearGradient(50 - ancho, 0, 50 + ancho, 0); sh.addColorStop(0, 'rgba(0,0,0,0.20)'); sh.addColorStop(0.45, 'rgba(0,0,0,0)'); sh.addColorStop(1, 'rgba(255,255,255,0.06)'); c.fillStyle = sh; c.fill();
    // Barba (debajo de la boca y la nariz).
    if (barba >= 6) { c.fillStyle = pelo; c.globalAlpha = barba === 8 ? 0.35 : 0.92; c.beginPath(); c.moveTo(50 - ancho + 1, oy + 8); c.bezierCurveTo(50 - ancho, oy + alto * 0.7, 50 - ancho * mand * 0.8, oy + alto * 0.98 + (barba === 7 ? 6 : 0), 50, oy + alto + (barba === 7 ? 9 : 2)); c.bezierCurveTo(50 + ancho * mand * 0.8, oy + alto * 0.98 + (barba === 7 ? 6 : 0), 50 + ancho, oy + alto * 0.7, 50 + ancho - 1, oy + 8); c.quadraticCurveTo(50, oy + 22, 50 - ancho + 1, oy + 8); c.fill(); c.globalAlpha = 1; }
    // Arrugas.
    c.strokeStyle = oscura; c.lineWidth = 0.7; c.globalAlpha = S.clamp((edad - 40) / 30, 0, 0.75);
    if (edad > 40) { for (let i = 0; i < 3; i++) { c.beginPath(); c.moveTo(50 - 12 + i, oy - 17 + i * 3.2); c.quadraticCurveTo(50, oy - 19 + i * 3.2, 50 + 12 - i, oy - 17 + i * 3.2); c.stroke(); } for (const sg of [-1, 1]) { c.beginPath(); c.moveTo(50 + sg * (ojoSep + ojoW + 1), oy - 1); c.lineTo(50 + sg * (ojoSep + ojoW + 4), oy - 3); c.moveTo(50 + sg * (ojoSep + ojoW + 1), oy + 1); c.lineTo(50 + sg * (ojoSep + ojoW + 4), oy + 2); c.stroke(); } }
    if (edad > 52) for (const sg of [-1, 1]) { c.beginPath(); c.moveTo(50 + sg * (narW + 1.5), oy + narL + 1); c.quadraticCurveTo(50 + sg * (bocaW * 0.5 + 4), bocaY - 4, 50 + sg * (bocaW * 0.5 + 3), bocaY + 3); c.stroke(); }
    c.globalAlpha = 1;
    // Ojos, con ojeras si hay años o desvelo.
    const cans = S.clamp((edad - 45) / 60 + (d.luto ? 0.25 : 0), 0, 0.45);
    for (const sg of [-1, 1]) {
      const ex = 50 + sg * ojoSep;
      if (cans > 0.1) { c.strokeStyle = oscura; c.globalAlpha = cans; c.lineWidth = 1; c.beginPath(); c.arc(ex, oy + 1.5, ojoW * 0.95, 0.25, Math.PI - 0.25); c.stroke(); c.globalAlpha = 1; }
      c.fillStyle = '#f4f1ea'; c.beginPath(); c.ellipse(ex, oy, ojoW, ojoH * (1 - cans * 0.5), 0, 0, TAU); c.fill();
      c.fillStyle = iris; c.beginPath(); c.arc(ex + sg * 0.3, oy, ojoH * 0.92, 0, TAU); c.fill();
      c.fillStyle = '#0c0c0c'; c.beginPath(); c.arc(ex + sg * 0.3, oy, ojoH * 0.45, 0, TAU); c.fill();
      c.fillStyle = 'rgba(255,255,255,0.85)'; c.beginPath(); c.arc(ex + sg * 0.3 - 0.9, oy - 0.9, 0.7, 0, TAU); c.fill();
      c.strokeStyle = oscura; c.lineWidth = 1.1; c.beginPath(); c.ellipse(ex, oy, ojoW, ojoH * (1 - cans * 0.5), 0, Math.PI, TAU); c.stroke();
      // Cejas: el rencor las baja hacia dentro; la empatía las levanta.
      const inc = (d.rencor - 0.45) * 5 - (d.empatia - 0.5) * 2.5; c.strokeStyle = pelo; c.lineWidth = cejaG; c.lineCap = 'round';
      c.beginPath(); c.moveTo(ex - sg * (ojoW + 0.5), oy - ojoH - 3.2 + inc); c.quadraticCurveTo(ex, oy - ojoH - 5.5, ex + sg * (ojoW + 1.5), oy - ojoH - 3.2 - inc * 0.4); c.stroke();
    }
    // Nariz.
    c.strokeStyle = oscura; c.lineWidth = 1.2; c.lineCap = 'round'; c.beginPath(); c.moveTo(50 + 1.2, oy + 2); c.quadraticCurveTo(50 + narW * 0.7, oy + narL * 0.75, 50 + narW * 0.45, oy + narL); c.quadraticCurveTo(50, oy + narL + 1.8, 50 - narW * 0.5, oy + narL); c.stroke();
    // Boca: sonríe o se tuerce según el carácter.
    const curva = (d.empatia - d.rencor) * 5 + r.r(-1, 1);
    c.strokeStyle = H ? tono(piel, -0.45) : mezcla(piel, '#b03a48', 0.6); c.lineWidth = H ? 1.7 : 2.4; c.beginPath(); c.moveTo(50 - bocaW / 2, bocaY); c.quadraticCurveTo(50, bocaY + curva, 50 + bocaW / 2, bocaY); c.stroke();
    // Bigote, perilla, barba de tres días.
    c.fillStyle = pelo; c.strokeStyle = pelo;
    if (barba === 3 || barba === 4 || barba >= 6) { c.lineWidth = 2.6; c.beginPath(); c.moveTo(50 - bocaW / 2 - 1.5, bocaY - 2.2); c.quadraticCurveTo(50, bocaY - 5.5, 50 + bocaW / 2 + 1.5, bocaY - 2.2); c.stroke(); }
    if (barba === 4 || barba === 5) { c.beginPath(); c.ellipse(50, bocaY + 6.5, 4, 5, 0, 0, TAU); c.fill(); }
    // Pecas, lunar, cicatrices.
    if (pecas) { c.fillStyle = rgba('#7a4a30', 0.5); for (let i = 0; i < 14; i++) { c.beginPath(); c.arc(50 + r.r(-ancho * 0.7, ancho * 0.7), oy + r.r(3, 12), 0.55, 0, TAU); c.fill(); } }
    if (lunar) { c.fillStyle = '#3a241a'; c.beginPath(); c.arc(50 + r.r(-12, 12), oy + r.r(8, 20), 0.9, 0, TAU); c.fill(); }
    for (let i = 0; i < d.cicatrices; i++) { const sx = 50 + (i % 2 ? -1 : 1) * r.r(8, 16), sy = oy + r.r(-8, 14), an = r.r(-1, 1); c.strokeStyle = '#b8686a'; c.lineWidth = 1.3; c.beginPath(); c.moveTo(sx - 5 * Math.cos(an), sy - 5 * Math.sin(an)); c.lineTo(sx + 5 * Math.cos(an), sy + 5 * Math.sin(an)); c.stroke(); c.lineWidth = 0.6; for (let k = -1; k <= 1; k++) { c.beginPath(); c.moveTo(sx + k * 2.5 * Math.cos(an) - 1.3 * Math.sin(an), sy + k * 2.5 * Math.sin(an) + 1.3 * Math.cos(an)); c.lineTo(sx + k * 2.5 * Math.cos(an) + 1.3 * Math.sin(an), sy + k * 2.5 * Math.sin(an) - 1.3 * Math.cos(an)); c.stroke(); } }
    // Pelo por delante.
    c.fillStyle = pelo;
    if (calvo) { for (const sg of [-1, 1]) { c.beginPath(); c.ellipse(50 + sg * (ancho - 1), oy - 12, 4, 9, sg * 0.2, 0, TAU); c.fill(); } }
    else {
      c.beginPath(); c.moveTo(50 - ancho - 1.5, oy - 4);
      c.bezierCurveTo(50 - ancho - 3, oy - 36, 50 + ancho + 3, oy - 36, 50 + ancho + 1.5, oy - 4);
      const fr = oy - 14 - (estilo % 3) * 2.5;
      if (estilo === 1) c.quadraticCurveTo(50 + 6, fr - 2, 50 - ancho + 4, fr + 8); else if (estilo === 2) { c.lineTo(50 + ancho - 2, fr + 4); c.quadraticCurveTo(50, fr - 6, 50 - ancho + 2, fr + 4); } else if (estilo === 7) { c.lineTo(50 + ancho - 3, fr + 7); c.lineTo(50, fr + 9); c.lineTo(50 - ancho + 3, fr + 7); } else c.quadraticCurveTo(50, fr - 3, 50 - ancho - 1.5, oy - 4);
      c.closePath(); c.fill();
      if (estilo === 4) { c.beginPath(); c.arc(50, oy - 33, 7, 0, TAU); c.fill(); }                                                       // moño
      if (estilo === 5) { for (let i = 0; i < 16; i++) { c.beginPath(); c.arc(50 + r.r(-ancho - 2, ancho + 2), oy - 22 + r.r(-10, 8), 4.2, 0, TAU); c.fill(); } }   // rizos
      if (estilo === 8) { c.beginPath(); c.moveTo(44, oy - 24); c.lineTo(50, oy - 42); c.lineTo(56, oy - 24); c.fill(); }                         // cresta
      if (estilo === 9) { c.beginPath(); c.ellipse(50 + ancho + 5, oy + 6, 3.5, 14, -0.15, 0, TAU); c.fill(); }                                   // coleta
    }
    // Gafas, implante, parche.
    if (gafas) { c.strokeStyle = '#1a1a1a'; c.lineWidth = 1.3; for (const sg of [-1, 1]) { c.beginPath(); c.arc(50 + sg * ojoSep, oy, ojoW + 1.8, 0, TAU); c.stroke(); } c.beginPath(); c.moveTo(50 - ojoSep + ojoW + 1.8, oy); c.lineTo(50 + ojoSep - ojoW - 1.8, oy); c.stroke(); }
    if (implante) { c.fillStyle = '#8f9aa8'; c.fillRect(50 + ancho - 7, oy - 14, 6, 5); c.fillStyle = '#6ee7ff'; c.fillRect(50 + ancho - 3, oy - 12, 1.4, 1.4); }
    if (d.rol === 'pirata' && r.p(0.45) || d.cicatrices >= 3) { c.fillStyle = '#0d0d0d'; c.beginPath(); c.ellipse(50 - ojoSep, oy, ojoW + 1.5, ojoH + 2, 0, 0, TAU); c.fill(); c.strokeStyle = '#0d0d0d'; c.lineWidth = 1.4; c.beginPath(); c.moveTo(50 - ancho, oy - 10); c.lineTo(50 + ancho, oy + 4); c.stroke(); }
    // Las muescas de su facción, tatuadas en el cuello.
    if (d.muescas) { c.strokeStyle = 'rgba(20,30,50,0.8)'; c.lineWidth = 0.7; for (let i = 0; i < Math.min(d.muescas, 14); i++) { c.beginPath(); c.moveTo(44 + i * 0.9, 76); c.lineTo(44 + i * 0.9, 79.5); c.stroke(); } }
    A.tocado(c, d, r, ancho, oy, pelo);
    // Los muertos, en gris y con crespón.
    if (!d.vivo) { c.globalCompositeOperation = 'saturation'; c.fillStyle = '#808080'; c.fillRect(0, 0, 100, 100); c.globalCompositeOperation = 'source-over'; c.fillStyle = 'rgba(10,10,14,0.35)'; c.fillRect(0, 0, 100, 100); c.fillStyle = '#000'; c.beginPath(); c.moveTo(68, 0); c.lineTo(100, 0); c.lineTo(100, 32); c.closePath(); c.fill(); }
    c.restore();
    c.strokeStyle = d.vivo ? tono(d.col, 0.15) : '#555'; c.lineWidth = Math.max(1, s / 45); c.strokeRect(x + 0.5, y + 0.5, s - 1, s - 1);
  };
  const UNIF = { gobernante: 1, general: 1, almirante: 1, jefe_guardia: 1, comisario: 1, soldado: 1, corsario: 1 };
  A.ropa = function (c, d, r, piel) {
    const rol = d.rol; let col = '#4a4f5c', borde = null;
    if (rol === 'gobernante') col = d.gob === 'monarquia' ? '#5a1f5e' : d.gob === 'teocracia' ? '#e8e2d0' : d.gob === 'comuna' ? '#5a2a22' : tono(d.col, -0.55);
    else if (UNIF[rol]) col = rol === 'almirante' ? '#1c2a48' : rol === 'comisario' ? '#22304a' : tono(d.col, -0.5);
    else if (rol === 'arzobispo') col = '#efe9da'; else if (rol === 'predicador') col = '#2a2420';
    else if (rol === 'mercader' || rol === 'aspirante') col = '#3d2a4a'; else if (rol === 'capitan' || rol === 'piloto') col = '#5a4630';
    else if (rol === 'pirata') col = '#5a1f1f'; else if (rol === 'perista') col = '#232628'; else if (rol === 'obrero') col = '#c2702a'; else if (rol === 'preso') col = '#d9d2b8';
    else if (rol === 'lider' || rol === 'organizador') col = '#4a3a30'; else if (rol === 'vengador') col = '#18181c'; else if (rol === 'cazador') col = '#3a4238'; else col = ['#4a4f5c', '#5a5246', '#3f4a52', '#56474f'][r.i(4)];
    c.fillStyle = col; c.beginPath(); c.moveTo(8, 100); c.quadraticCurveTo(12, 80, 36, 77); c.lineTo(64, 77); c.quadraticCurveTo(88, 80, 92, 100); c.closePath(); c.fill();
    c.fillStyle = 'rgba(0,0,0,0.25)'; c.beginPath(); c.moveTo(8, 100); c.quadraticCurveTo(12, 80, 36, 77); c.lineTo(42, 100); c.closePath(); c.fill();
    // Cuello de la prenda.
    if (rol === 'arzobispo' || rol === 'predicador') { c.fillStyle = rol === 'arzobispo' ? '#c8a23c' : '#5a4a40'; c.fillRect(46, 80, 8, 20); c.fillRect(40, 86, 20, 5); }
    else if (rol === 'gobernante' && d.gob === 'monarquia') { c.fillStyle = '#f2efe6'; c.beginPath(); c.moveTo(30, 78); c.lineTo(50, 92); c.lineTo(70, 78); c.lineTo(66, 76); c.lineTo(50, 84); c.lineTo(34, 76); c.fill(); c.fillStyle = '#1a1a1a'; for (let i = 0; i < 5; i++) { c.beginPath(); c.arc(36 + i * 7, 81 + (i % 2) * 2, 0.9, 0, TAU); c.fill(); } }
    else if (rol === 'preso') { c.strokeStyle = '#3a3a3a'; c.lineWidth = 2.5; for (let i = 14; i < 92; i += 8) { c.beginPath(); c.moveTo(i, 78); c.lineTo(i, 100); c.stroke(); } }
    else if (rol === 'pirata' || rol === 'obrero' || rol === 'civil') { c.fillStyle = piel; c.beginPath(); c.moveTo(44, 78); c.lineTo(50, 90); c.lineTo(56, 78); c.fill(); }
    else { c.fillStyle = tono(col, 0.25); c.beginPath(); c.moveTo(40, 77); c.lineTo(50, 88); c.lineTo(60, 77); c.lineTo(57, 76); c.lineTo(50, 83); c.lineTo(43, 76); c.fill(); }
    if (UNIF[rol] && rol !== 'soldado') {
      // Charreteras, banda y medallas (una por victoria).
      c.fillStyle = '#d6b24a'; c.fillRect(13, 83, 15, 4); c.fillRect(72, 83, 15, 4); for (let i = 0; i < 4; i++) { c.fillRect(14 + i * 4, 87, 1.5, 4); c.fillRect(73 + i * 4, 87, 1.5, 4); }
      if (rol === 'gobernante' || rol === 'general') { c.strokeStyle = d.col; c.lineWidth = 5; c.beginPath(); c.moveTo(30, 78); c.lineTo(66, 100); c.stroke(); }
      for (let i = 0; i < d.medallas; i++) { c.fillStyle = ['#d6b24a', '#c0c8d0', '#c0723a'][i % 3]; c.beginPath(); c.arc(62 + (i % 3) * 6, 91 + Math.floor(i / 3) * 6, 2.2, 0, TAU); c.fill(); c.fillStyle = ['#c0392b', '#2f6fb0', '#3a8a4a'][i % 3]; c.fillRect(60.5 + (i % 3) * 6, 86 + Math.floor(i / 3) * 6, 3, 3); }
    }
    if (rol === 'soldado' || rol === 'cazador') { c.fillStyle = tono(col, 0.18); c.fillRect(30, 82, 40, 18); c.strokeStyle = 'rgba(0,0,0,0.4)'; c.lineWidth = 1; c.strokeRect(30, 82, 40, 18); c.beginPath(); c.moveTo(50, 82); c.lineTo(50, 100); c.stroke(); }
    if (rol === 'comisario' || rol === 'inspector') { c.fillStyle = '#d6b24a'; c.beginPath(); for (let i = 0; i < 6; i++) { const an = i * TAU / 6 - Math.PI / 2; c.lineTo(66 + 4 * Math.cos(an), 90 + 4 * Math.sin(an)); } c.fill(); }
    if (rol === 'lider' || rol === 'organizador') { c.fillStyle = '#c0392b'; c.fillRect(16, 88, 12, 6); }
    if (rol === 'mercader' && d.rico) { c.strokeStyle = '#d6b24a'; c.lineWidth = 1.6; c.beginPath(); c.arc(50, 86, 11, 0.4, Math.PI - 0.4); c.stroke(); c.fillStyle = '#d6b24a'; c.beginPath(); c.arc(50, 97, 3, 0, TAU); c.fill(); }
    if (rol === 'capitan' || rol === 'piloto') { c.strokeStyle = '#9aa3ad'; c.lineWidth = 3; c.beginPath(); c.arc(50, 80, 15, 0.15, Math.PI - 0.15); c.stroke(); }   // anillo del traje
  };
  A.tocado = function (c, d, r, ancho, oy, pelo) {
    const rol = d.rol; const t = oy - 22;
    if (rol === 'gobernante' && d.gob === 'monarquia') { c.fillStyle = '#e0bc48'; c.beginPath(); c.moveTo(50 - ancho + 2, t + 6); for (let i = 0; i <= 4; i++) { const x = 50 - ancho + 2 + i * (2 * ancho - 4) / 4; c.lineTo(x - 3, t - 2); c.lineTo(x, t - 12 - (i === 2 ? 4 : 0)); c.lineTo(x + 3, t - 2); } c.lineTo(50 + ancho - 2, t + 6); c.closePath(); c.fill(); c.fillStyle = '#c0392b'; c.beginPath(); c.arc(50, t - 2, 2, 0, TAU); c.fill(); c.fillStyle = '#2f6fb0'; c.beginPath(); c.arc(50 - 10, t, 1.5, 0, TAU); c.arc(50 + 10, t, 1.5, 0, TAU); c.fill(); }
    else if (rol === 'gobernante' && d.gob === 'republica') { c.strokeStyle = '#7fae5a'; c.lineWidth = 2; for (const sg of [-1, 1]) for (let i = 0; i < 5; i++) { const an = Math.PI / 2 + sg * (0.5 + i * 0.28); c.beginPath(); c.ellipse(50 + Math.cos(an) * (ancho + 1), oy - 8 - Math.sin(an) * 22, 3.6, 1.5, an + sg * 0.6, 0, TAU); c.stroke(); } }
    else if (rol === 'gobernante' && d.gob === 'teocracia' || rol === 'arzobispo') { c.fillStyle = '#efe9da'; c.beginPath(); c.moveTo(50 - ancho + 3, t + 5); c.lineTo(50 - 9, t - 16); c.lineTo(50, t - 26); c.lineTo(50 + 9, t - 16); c.lineTo(50 + ancho - 3, t + 5); c.closePath(); c.fill(); c.strokeStyle = '#c8a23c'; c.lineWidth = 2; c.beginPath(); c.moveTo(50, t - 20); c.lineTo(50, t); c.moveTo(44, t - 12); c.lineTo(56, t - 12); c.stroke(); }
    else if (rol === 'gobernante' && d.gob === 'comuna') { c.fillStyle = '#8a2a22'; c.beginPath(); c.ellipse(50 + 4, t - 2, ancho + 1, 9, -0.12, 0, TAU); c.fill(); c.fillStyle = '#d6b24a'; A.estrella(c, 50, t - 2, 3.2, 5); }
    else if (rol === 'gobernante' || rol === 'general' || rol === 'almirante' || rol === 'jefe_guardia' || rol === 'comisario') {
      const col = rol === 'almirante' ? '#f0efe8' : rol === 'comisario' ? '#22304a' : tono(d.col, -0.55);
      c.fillStyle = col; c.beginPath(); c.moveTo(50 - ancho - 3, t + 4); c.quadraticCurveTo(50, t - 20, 50 + ancho + 3, t + 4); c.closePath(); c.fill();
      c.fillStyle = '#101216'; c.beginPath(); c.moveTo(50 - ancho - 4, t + 4); c.quadraticCurveTo(50, t + 13, 50 + ancho + 4, t + 4); c.lineTo(50 + ancho + 1, t + 2); c.quadraticCurveTo(50, t + 7, 50 - ancho - 1, t + 2); c.fill();
      c.fillStyle = d.col; c.fillRect(50 - ancho - 2, t, 2 * ancho + 4, 3); c.fillStyle = '#d6b24a'; A.estrella(c, 50, t - 5, 3.4, 5);
    }
    else if (rol === 'obrero') { c.fillStyle = '#e0b020'; c.beginPath(); c.arc(50, t + 6, ancho + 2, Math.PI, TAU); c.fill(); c.fillRect(50 - ancho - 5, t + 4, 2 * ancho + 10, 3); c.fillStyle = '#fffbe0'; c.beginPath(); c.arc(50, t - 6, 3.2, 0, TAU); c.fill(); c.fillStyle = 'rgba(255,250,200,0.25)'; c.beginPath(); c.arc(50, t - 6, 7, 0, TAU); c.fill(); }
    else if (rol === 'soldado' || rol === 'cazador') { c.fillStyle = tono(d.col, -0.4); c.beginPath(); c.arc(50, t + 8, ancho + 3, Math.PI, TAU); c.lineTo(50 + ancho + 3, t + 14); c.lineTo(50 + ancho - 2, t + 14); c.lineTo(50 + ancho - 2, t + 8); c.lineTo(50 - ancho + 2, t + 8); c.lineTo(50 - ancho + 2, t + 14); c.lineTo(50 - ancho - 3, t + 14); c.closePath(); c.fill(); c.fillStyle = 'rgba(120,200,255,0.35)'; c.fillRect(50 - ancho + 2, t + 2, 2 * ancho - 4, 5); }
    else if (rol === 'pirata') { c.fillStyle = '#9a2420'; c.beginPath(); c.moveTo(50 - ancho - 2, t + 8); c.quadraticCurveTo(50, t - 14, 50 + ancho + 2, t + 8); c.quadraticCurveTo(50, t + 1, 50 - ancho - 2, t + 8); c.fill(); c.beginPath(); c.moveTo(50 + ancho, t + 6); c.lineTo(50 + ancho + 9, t + 14); c.lineTo(50 + ancho + 4, t + 16); c.fill(); }
    else if (rol === 'predicador' || rol === 'perista') { c.fillStyle = rol === 'perista' ? '#1a1c1e' : '#2a2420'; c.beginPath(); c.moveTo(50 - ancho - 7, oy + 26); c.bezierCurveTo(50 - ancho - 9, t - 24, 50 + ancho + 9, t - 24, 50 + ancho + 7, oy + 26); c.lineTo(50 + ancho - 1, oy + 8); c.bezierCurveTo(50 + ancho, t - 6, 50 - ancho, t - 6, 50 - ancho + 1, oy + 8); c.closePath(); c.fill(); }
    else if (rol === 'mercader') { c.fillStyle = '#2a1a30'; c.beginPath(); c.ellipse(50, t + 5, ancho + 9, 4, 0, 0, TAU); c.fill(); c.beginPath(); c.moveTo(50 - ancho + 3, t + 5); c.lineTo(50 - ancho + 6, t - 13); c.lineTo(50 + ancho - 6, t - 13); c.lineTo(50 + ancho - 3, t + 5); c.fill(); c.fillStyle = '#d6b24a'; c.fillRect(50 - ancho + 5, t - 1, 2 * ancho - 10, 2.5); }
    void pelo; void r;
  };
  A.estrella = function (c, x, y, r, n) { c.beginPath(); for (let i = 0; i < n * 2; i++) { const an = -Math.PI / 2 + i * Math.PI / n; const rr = i % 2 ? r * 0.45 : r; c.lineTo(x + Math.cos(an) * rr, y + Math.sin(an) * rr); } c.closePath(); c.fill(); };

  // ── Emblemas (para banderas y símbolos de facción). Se dibujan centrados en (x,y) con radio r.
  const EMB = {
    estrella: (c, x, y, r) => A.estrella(c, x, y, r, 5),
    sol: (c, x, y, r) => { c.beginPath(); c.arc(x, y, r * 0.5, 0, TAU); c.fill(); c.lineWidth = r * 0.14; for (let i = 0; i < 12; i++) { const an = i * TAU / 12; c.beginPath(); c.moveTo(x + Math.cos(an) * r * 0.65, y + Math.sin(an) * r * 0.65); c.lineTo(x + Math.cos(an) * r, y + Math.sin(an) * r); c.stroke(); } },
    luna: (c, x, y, r) => { c.beginPath(); c.arc(x, y, r * 0.85, 0.6, TAU - 0.6); c.arc(x + r * 0.35, y, r * 0.65, TAU - 0.9, 0.9, true); c.fill(); },
    engranaje: (c, x, y, r) => { c.beginPath(); for (let i = 0; i < 16; i++) { const an = i * TAU / 16; const rr = i % 2 ? r : r * 0.72; c.lineTo(x + Math.cos(an) * rr, y + Math.sin(an) * rr); c.lineTo(x + Math.cos(an + TAU / 32) * rr, y + Math.sin(an + TAU / 32) * rr); } c.closePath(); c.fill(); c.globalCompositeOperation = 'destination-out'; c.beginPath(); c.arc(x, y, r * 0.3, 0, TAU); c.fill(); c.globalCompositeOperation = 'source-over'; },
    espiga: (c, x, y, r) => { c.lineWidth = r * 0.1; c.beginPath(); c.moveTo(x, y + r); c.lineTo(x, y - r * 0.9); c.stroke(); for (let i = 0; i < 5; i++) for (const sg of [-1, 1]) { c.beginPath(); c.ellipse(x + sg * r * 0.22, y - r * 0.7 + i * r * 0.3, r * 0.2, r * 0.1, sg * 0.9, 0, TAU); c.fill(); } },
    espada: (c, x, y, r) => { c.beginPath(); c.moveTo(x, y - r); c.lineTo(x + r * 0.13, y + r * 0.45); c.lineTo(x - r * 0.13, y + r * 0.45); c.closePath(); c.fill(); c.fillRect(x - r * 0.4, y + r * 0.45, r * 0.8, r * 0.13); c.fillRect(x - r * 0.08, y + r * 0.55, r * 0.16, r * 0.4); },
    ojo: (c, x, y, r) => { c.lineWidth = r * 0.14; c.beginPath(); c.moveTo(x - r, y); c.quadraticCurveTo(x, y - r * 0.8, x + r, y); c.quadraticCurveTo(x, y + r * 0.8, x - r, y); c.stroke(); c.beginPath(); c.arc(x, y, r * 0.3, 0, TAU); c.fill(); },
    ojo_tachado: (c, x, y, r) => { EMB.ojo(c, x, y, r); c.lineWidth = r * 0.2; c.beginPath(); c.moveTo(x - r * 0.9, y + r * 0.7); c.lineTo(x + r * 0.9, y - r * 0.7); c.stroke(); },
    puno: (c, x, y, r) => { c.beginPath(); c.rect(x - r * 0.45, y - r * 0.5, r * 0.9, r * 0.75); c.fill(); for (let i = 0; i < 4; i++) { c.beginPath(); c.arc(x - r * 0.34 + i * r * 0.225, y - r * 0.55, r * 0.14, 0, TAU); c.fill(); } c.fillRect(x - r * 0.3, y + r * 0.25, r * 0.6, r * 0.75); c.beginPath(); c.arc(x - r * 0.5, y - r * 0.05, r * 0.16, 0, TAU); c.fill(); },
    ancla: (c, x, y, r) => { c.lineWidth = r * 0.16; c.beginPath(); c.arc(x, y - r * 0.7, r * 0.2, 0, TAU); c.moveTo(x, y - r * 0.5); c.lineTo(x, y + r * 0.9); c.moveTo(x - r * 0.4, y - r * 0.2); c.lineTo(x + r * 0.4, y - r * 0.2); c.stroke(); c.beginPath(); c.arc(x, y + r * 0.2, r * 0.7, 0.15, Math.PI - 0.15); c.stroke(); },
    corona: (c, x, y, r) => { c.beginPath(); c.moveTo(x - r, y + r * 0.6); c.lineTo(x - r, y - r * 0.3); c.lineTo(x - r * 0.5, y + r * 0.1); c.lineTo(x, y - r * 0.8); c.lineTo(x + r * 0.5, y + r * 0.1); c.lineTo(x + r, y - r * 0.3); c.lineTo(x + r, y + r * 0.6); c.closePath(); c.fill(); },
    llama: (c, x, y, r) => { c.beginPath(); c.moveTo(x, y - r); c.bezierCurveTo(x + r * 0.9, y - r * 0.1, x + r * 0.7, y + r, x, y + r); c.bezierCurveTo(x - r * 0.7, y + r, x - r * 0.6, y, x - r * 0.15, y - r * 0.2); c.bezierCurveTo(x - r * 0.1, y - r * 0.6, x - r * 0.3, y - r * 0.6, x, y - r); c.fill(); },
    llave: (c, x, y, r) => { c.lineWidth = r * 0.18; c.beginPath(); c.arc(x - r * 0.5, y, r * 0.35, 0, TAU); c.moveTo(x - r * 0.15, y); c.lineTo(x + r, y); c.moveTo(x + r * 0.6, y); c.lineTo(x + r * 0.6, y + r * 0.4); c.moveTo(x + r * 0.9, y); c.lineTo(x + r * 0.9, y + r * 0.3); c.stroke(); },
    farol: (c, x, y, r) => { c.fillRect(x - r * 0.35, y - r * 0.4, r * 0.7, r * 0.9); c.beginPath(); c.moveTo(x - r * 0.5, y - r * 0.4); c.lineTo(x, y - r * 0.85); c.lineTo(x + r * 0.5, y - r * 0.4); c.fill(); c.fillRect(x - r * 0.5, y + r * 0.5, r, r * 0.15); c.lineWidth = r * 0.1; c.beginPath(); c.arc(x, y - r * 0.85, r * 0.18, 0, TAU); c.stroke(); },
    casco: (c, x, y, r) => { c.beginPath(); c.arc(x, y + r * 0.1, r * 0.8, Math.PI, TAU); c.fill(); c.fillRect(x - r, y + r * 0.1, r * 2, r * 0.2); c.beginPath(); c.arc(x, y - r * 0.45, r * 0.18, 0, TAU); c.fill(); },
    mesa: (c, x, y, r) => { c.fillRect(x - r, y - r * 0.25, r * 2, r * 0.25); c.fillRect(x - r * 0.8, y, r * 0.18, r * 0.8); c.fillRect(x + r * 0.62, y, r * 0.18, r * 0.8); },
    vela: (c, x, y, r) => { c.fillRect(x - r * 0.2, y - r * 0.2, r * 0.4, r * 1.1); EMB.llama(c, x, y - r * 0.6, r * 0.38); },
    pico: (c, x, y, r) => { c.lineWidth = r * 0.16; c.beginPath(); c.moveTo(x - r * 0.6, y + r * 0.9); c.lineTo(x + r * 0.5, y - r * 0.5); c.stroke(); c.beginPath(); c.arc(x + r * 0.15, y + r * 0.1, r * 0.95, -2.1, -0.4); c.stroke(); },
    balanza: (c, x, y, r) => { c.lineWidth = r * 0.12; c.beginPath(); c.moveTo(x, y - r); c.lineTo(x, y + r); c.moveTo(x - r * 0.9, y - r * 0.6); c.lineTo(x + r * 0.9, y - r * 0.6); c.moveTo(x - r * 0.5, y + r); c.lineTo(x + r * 0.5, y + r); c.stroke(); for (const sg of [-1, 1]) { c.beginPath(); c.arc(x + sg * r * 0.8, y - r * 0.1, r * 0.35, 0, Math.PI); c.fill(); } },
    hoz: (c, x, y, r) => { c.lineWidth = r * 0.2; c.beginPath(); c.arc(x, y - r * 0.1, r * 0.75, -0.3, Math.PI * 1.05); c.stroke(); c.beginPath(); c.moveTo(x - r * 0.72, y + r * 0.15); c.lineTo(x - r * 0.95, y + r * 0.9); c.stroke(); },
    cuenco: (c, x, y, r) => { c.beginPath(); c.arc(x, y - r * 0.1, r * 0.9, 0, Math.PI); c.fill(); c.fillRect(x - r * 0.4, y + r * 0.75, r * 0.8, r * 0.14); },
    mano: (c, x, y, r) => { c.beginPath(); c.rect(x - r * 0.4, y - r * 0.1, r * 0.8, r * 0.9); c.fill(); for (let i = 0; i < 4; i++) c.fillRect(x - r * 0.4 + i * r * 0.21, y - r * (0.8 + (i === 1 || i === 2 ? 0.15 : 0)), r * 0.17, r * 0.8); c.fillRect(x - r * 0.7, y + r * 0.05, r * 0.3, r * 0.2); },
    hilo: (c, x, y, r) => { c.lineWidth = r * 0.12; c.beginPath(); c.moveTo(x - r, y + r * 0.5); c.bezierCurveTo(x - r * 0.4, y - r * 1.3, x + r * 0.3, y + r * 1.3, x + r, y - r * 0.5); c.stroke(); },
    moneda: (c, x, y, r) => { c.beginPath(); c.arc(x, y, r * 0.85, 0, TAU); c.fill(); c.globalCompositeOperation = 'destination-out'; c.beginPath(); c.arc(x, y, r * 0.55, 0, TAU); c.fill(); c.globalCompositeOperation = 'source-over'; c.beginPath(); c.arc(x, y, r * 0.2, 0, TAU); c.fill(); },
    torre: (c, x, y, r) => { c.fillRect(x - r * 0.45, y - r * 0.5, r * 0.9, r * 1.5); for (let i = 0; i < 3; i++) c.fillRect(x - r * 0.6 + i * r * 0.45, y - r * 0.9, r * 0.3, r * 0.45); },
    cruz: (c, x, y, r) => { c.fillRect(x - r * 0.15, y - r, r * 0.3, r * 2); c.fillRect(x - r * 0.7, y - r * 0.45, r * 1.4, r * 0.3); },
    anillo: (c, x, y, r) => { c.lineWidth = r * 0.25; c.beginPath(); c.arc(x, y, r * 0.7, 0, TAU); c.stroke(); },
  };
  A.EMB = EMB;
  A.emblema = function (c, k, x, y, r, col) { c.save(); c.fillStyle = col; c.strokeStyle = col; c.lineCap = 'round'; (EMB[k] || EMB.estrella)(c, x, y, r); c.restore(); };
  const POR_GOB = { autocracia: ['sol', 'ojo', 'puno', 'torre'], monarquia: ['corona'], republica: ['balanza', 'engranaje', 'ancla'], teocracia: ['llama', 'vela', 'cruz'], junta: ['espada', 'torre'], comuna: ['puno', 'espiga', 'estrella'], senorio: ['espada', 'llave', 'torre'] };
  const POR_OBJ = { casco: 'casco', visor: 'ojo', 'lámpara': 'farol', farol: 'farol', pico: 'pico', espiga: 'espiga', trigo: 'espiga', cuenco: 'cuenco', hoz: 'hoz', ojo: 'ojo', mano: 'mano', cuerda: 'anillo', plaza: 'mesa', ceniza: 'llama', 'cráter': 'anillo', llama: 'llama', cielo: 'estrella', mesa: 'mesa', vela: 'vela', voz: 'hilo', moneda: 'moneda', ancla: 'ancla', cuchillo: 'espada', llave: 'llave', 'puño': 'puno', hilo: 'hilo', sol: 'sol', sello: 'moneda' };
  A.descBanderaEstado = function (m, e) { const r = new S.Rng(S.hash(m.semilla, 'bandera', e.id)); const pal = [e.col, tono(e.col, -0.6), ['#f2efe6', '#101216', '#d6b24a'][r.i(3)]]; const l = POR_GOB[e.tipoGob] || ['estrella']; return { col: pal, patron: r.i(12), emb: l[r.i(l.length)], embCol: pal[2], canton: r.p(0.3) }; };
  A.descBanderaFaccion = function (m, f) {
    const r = new S.Rng(S.hash(m.semilla, 'bandera-f', f.id)); const t = ((f.sim || '') + ' ' + (f.nom || '')).toLowerCase(); let emb = f.tipo === 'casa' ? 'moneda' : 'puno';
    for (const k in POR_OBJ) if (t.indexOf(k) >= 0) { emb = POR_OBJ[k]; break; }
    if (emb === 'ojo' && t.indexOf('tachado') >= 0) emb = 'ojo_tachado';
    const base = f.tipo === 'casa' ? ['#2a2440', '#d6b24a', '#f2efe6'] : f.tipo === 'orden' ? ['#1a1a22', '#e8e2d0', '#c8a23c'] : f.tipo === 'hermandad' || f.tipo === 'sindicato' ? ['#22303a', '#e0b020', '#f2efe6'] : ['#7a1f1f', '#101216', '#f2efe6'];
    const muescas = /(\w+) muescas/.exec(t); return { col: base, patron: [0, 1, 8, 9, 10][r.i(5)], emb, embCol: base[2], canton: false, muescas: muescas && f.hecho ? Math.min(f.hecho.muertos, 20) : 0 };
  };
  A.bandera = function (c, x, y, w, h, d) {
    c.save(); c.translate(x, y); c.beginPath(); c.rect(0, 0, w, h); c.clip();
    const [a, b, k] = d.col; c.fillStyle = a; c.fillRect(0, 0, w, h); c.fillStyle = b;
    switch (d.patron) {
      case 1: c.fillRect(0, h / 2, w, h / 2); break;
      case 2: c.fillRect(w / 2, 0, w / 2, h); break;
      case 3: c.fillRect(0, h / 3, w, h / 3); c.fillStyle = k; c.fillRect(0, 2 * h / 3, w, h / 3); break;
      case 4: c.fillRect(w / 3, 0, w / 3, h); c.fillStyle = k; c.fillRect(2 * w / 3, 0, w / 3, h); break;
      case 5: c.fillRect(w * 0.42, 0, w * 0.16, h); c.fillRect(0, h * 0.38, w, h * 0.24); break;
      case 6: c.lineWidth = h * 0.2; c.strokeStyle = b; c.beginPath(); c.moveTo(0, 0); c.lineTo(w, h); c.moveTo(w, 0); c.lineTo(0, h); c.stroke(); break;
      case 7: c.fillRect(0, 0, w * 0.45, h * 0.5); break;
      case 8: c.beginPath(); c.moveTo(0, h); c.lineTo(w * 0.35, h); c.lineTo(w, 0); c.lineTo(w * 0.65, 0); c.closePath(); c.fill(); break;
      case 9: c.lineWidth = h * 0.22; c.strokeStyle = b; c.strokeRect(0, 0, w, h); break;
      case 10: c.beginPath(); c.moveTo(0, 0); c.lineTo(w * 0.45, h / 2); c.lineTo(0, h); c.closePath(); c.fill(); break;
      case 11: c.fillRect(0, 0, w / 2, h / 2); c.fillRect(w / 2, h / 2, w / 2, h / 2); break;
    }
    const ex = d.canton && d.patron !== 10 ? w * 0.24 : d.patron === 10 ? w * 0.17 : w / 2, ey = d.canton && d.patron !== 10 ? h * 0.3 : h / 2; const er = Math.min(w, h) * (d.canton ? 0.2 : 0.3);
    if (d.patron === 0 || d.patron === 5 || d.patron === 6 || d.patron === 9 || d.patron === 2 || d.patron === 1) { c.fillStyle = 'rgba(0,0,0,0.28)'; c.beginPath(); c.arc(ex, ey, er * 1.25, 0, TAU); c.fill(); }
    A.emblema(c, d.emb, ex, ey, er, d.embCol);
    if (d.muescas) { c.strokeStyle = d.embCol; c.lineWidth = Math.max(1, w * 0.012); for (let i = 0; i < d.muescas; i++) { c.beginPath(); c.moveTo(w * 0.08 + i * w * 0.042, h * 0.8); c.lineTo(w * 0.08 + i * w * 0.042, h * 0.93); c.stroke(); } }
    const br = c.createLinearGradient(0, 0, w, 0); br.addColorStop(0, 'rgba(255,255,255,0.10)'); br.addColorStop(0.5, 'rgba(0,0,0,0)'); br.addColorStop(1, 'rgba(0,0,0,0.22)'); c.fillStyle = br; c.fillRect(0, 0, w, h);
    c.restore(); c.strokeStyle = 'rgba(255,255,255,0.35)'; c.lineWidth = 1; c.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
  };
  A.banderaDe = function (c, x, y, w, h, m, t, id) { A.bandera(c, x, y, w, h, t === 'E' ? A.descBanderaEstado(m, m.est[id]) : A.descBanderaFaccion(m, m.fac[id])); };

  // ── Plano de una nave. Lo que lleva el diseño se ve: torres, emisores, celdas de misiles, blindaje, escudo, toberas, antenas.
  const FORMA = { fragata: [0.6, 0.15], crucero: [0.74, 0.19], acorazado: [0.88, 0.26], corbeta: [0.46, 0.13], carguero: [0.62, 0.24], correo: [0.42, 0.09], granelero: [0.8, 0.26], granja: [0.7, 0.3], carronero: [0.52, 0.2], censo: [0.48, 0.14], remolcador: [0.4, 0.2], prision: [0.66, 0.26], funeraria: [0.5, 0.13], bazar: [0.78, 0.3], astillero_n: [0.84, 0.34], arca: [0.92, 0.32] };
  A.nave = function (c, x, y, w, h, m, n) {
    const def = S.reg.nave[n.cls]; const [fl, fh] = FORMA[n.cls] || [0.6, 0.18]; const L = w * fl, Hh = h * fh * 1.6; const cx0 = x + w / 2, cy0 = y + h * 0.5;
    const casco = S.CASCOS[n.col].hex; const est = n.dueno.t === 'E' && m.est[n.dueno.id] ? m.est[n.dueno.id].col : null; const d = n.dis !== undefined && m.dis[n.dis] ? m.dis[n.dis] : null; const g2 = d ? d.g : null; const q = n.q;
    c.save(); c.fillStyle = '#0b0f19'; c.fillRect(x, y, w, h);
    c.strokeStyle = 'rgba(95,176,201,0.10)'; c.lineWidth = 1; for (let i = x; i < x + w; i += 12) { c.beginPath(); c.moveTo(i, y); c.lineTo(i, y + h); c.stroke(); } for (let j = y; j < y + h; j += 12) { c.beginPath(); c.moveTo(x, j); c.lineTo(x + w, j); c.stroke(); }
    const x0 = cx0 - L / 2, x1 = cx0 + L / 2;
    // Escudo.
    if (g2 && g2[4] > 0.03) { c.strokeStyle = 'rgba(95,176,201,' + S.clamp(g2[4] * 3 * (q ? q[4] : 1), 0.1, 0.8) + ')'; c.lineWidth = 1.5 + g2[4] * 10; c.beginPath(); c.ellipse(cx0, cy0, L / 2 + 10, Hh / 2 + 12, 0, 0, TAU); c.stroke(); }
    // Toberas y llama: más motor, más tobera.
    const mot = g2 ? g2[6] * (q ? q[6] : 1) : 0.1; const tob = Hh * (0.22 + mot * 2.2);
    for (const k of [-1, 1]) { const ty = cy0 + k * Hh * 0.22; const gl = c.createLinearGradient(x0 - 6 - tob * 1.6, 0, x0, 0); gl.addColorStop(0, 'rgba(226,112,58,0)'); gl.addColorStop(1, 'rgba(255,200,120,0.85)'); c.fillStyle = gl; c.beginPath(); c.moveTo(x0, ty - tob / 2); c.lineTo(x0 - 6 - tob * 1.6, ty); c.lineTo(x0, ty + tob / 2); c.fill(); c.fillStyle = '#3a3f4a'; c.fillRect(x0 - 5, ty - tob / 2, 7, tob); }
    // Casco.
    c.beginPath();
    if (n.cls === 'carguero' || n.cls === 'granelero' || n.cls === 'prision' || n.cls === 'bazar') { c.rect(x0, cy0 - Hh / 2, L * 0.86, Hh); c.moveTo(x0 + L * 0.86, cy0 - Hh * 0.35); c.lineTo(x1, cy0 - Hh * 0.12); c.lineTo(x1, cy0 + Hh * 0.12); c.lineTo(x0 + L * 0.86, cy0 + Hh * 0.35); }
    else if (n.cls === 'granja' || n.cls === 'arca') { c.ellipse(cx0, cy0, L / 2, Hh / 2, 0, 0, TAU); }
    else if (n.cls === 'astillero_n') { c.rect(x0, cy0 - Hh / 2, L, Hh * 0.2); c.rect(x0, cy0 + Hh * 0.3, L, Hh * 0.2); c.rect(x0, cy0 - Hh / 2, L * 0.08, Hh); c.rect(x1 - L * 0.08, cy0 - Hh / 2, L * 0.08, Hh); }
    else { c.moveTo(x0, cy0 - Hh * 0.42); c.lineTo(x0 + L * 0.55, cy0 - Hh * 0.5); c.quadraticCurveTo(x1 - L * 0.08, cy0 - Hh * 0.3, x1, cy0); c.quadraticCurveTo(x1 - L * 0.08, cy0 + Hh * 0.3, x0 + L * 0.55, cy0 + Hh * 0.5); c.lineTo(x0, cy0 + Hh * 0.42); c.closePath(); }
    const hg = c.createLinearGradient(0, cy0 - Hh / 2, 0, cy0 + Hh / 2); hg.addColorStop(0, tono(casco, 0.18)); hg.addColorStop(0.5, casco); hg.addColorStop(1, tono(casco, -0.45)); c.fillStyle = hg; c.fill();
    c.strokeStyle = g2 ? tono('#8d99ad', -0.1) : 'rgba(0,0,0,0.5)'; c.lineWidth = g2 ? 1 + g2[3] * 22 * (q ? q[3] : 1) : 1; c.stroke();     // blindaje: grosor del contorno
    // Franja con el color del dueño (y debajo, las capas de pintura anteriores).
    if (est) { c.fillStyle = est; c.fillRect(x0 + L * 0.3, cy0 - Hh * 0.46, L * 0.07, Hh * 0.92); }
    for (let i = 0; i < Math.min(3, n.pin.length - 1); i++) { c.fillStyle = m.est[n.pin[n.pin.length - 2 - i]] ? m.est[n.pin[n.pin.length - 2 - i]].col : '#888'; c.globalAlpha = 0.45; c.fillRect(x0 + L * (0.39 + i * 0.03), cy0 - Hh * 0.3, L * 0.02, Hh * 0.6); c.globalAlpha = 1; }
    if (g2) {
      const malo = (k) => q && q[k] < 0.75;
      // Cañones: torres sobre cubierta.
      const nc = Math.round(g2[0] * 22); for (let i = 0; i < nc; i++) { const tx = x0 + L * (0.12 + 0.7 * (i + 0.5) / Math.max(1, nc)); const sg = i % 2 ? 1 : -1; c.fillStyle = malo(0) ? '#7a4a4a' : '#59616e'; c.fillRect(tx - 3, cy0 + sg * Hh * 0.5 - (sg > 0 ? 0 : 5), 6, 5); c.strokeStyle = '#c9a23e'; c.lineWidth = 1.4; c.beginPath(); c.moveTo(tx, cy0 + sg * (Hh * 0.5 + 2)); c.lineTo(tx + 9, cy0 + sg * (Hh * 0.5 + 6)); c.stroke(); }
      // Láseres: emisores en el morro.
      const nl = Math.round(g2[1] * 16); for (let i = 0; i < nl; i++) { const ly = cy0 + (i - (nl - 1) / 2) * Hh * 0.7 / Math.max(1, nl); c.fillStyle = malo(1) ? '#7a4a4a' : '#e0564f'; c.beginPath(); c.arc(x1 - L * 0.1 - (Math.abs(i - (nl - 1) / 2)) * 3, ly, 2, 0, TAU); c.fill(); }
      // Misiles: celdas en el centro.
      const nm = Math.round(g2[2] * 26); c.fillStyle = malo(2) ? '#7a5a5a' : '#f0f0f0'; for (let i = 0; i < nm; i++) c.fillRect(x0 + L * 0.45 + (i % 7) * 4.5, cy0 - 6 + Math.floor(i / 7) * 4.5, 3, 3);
      // Defensa de punto: puntos por el lomo.
      const np = Math.round(g2[5] * 24); c.fillStyle = '#b98fd6'; for (let i = 0; i < np; i++) { c.beginPath(); c.arc(x0 + L * (0.08 + 0.8 * i / Math.max(1, np)), cy0 + (i % 2 ? 1 : -1) * Hh * 0.33, 1.3, 0, TAU); c.fill(); }
      // Sensores: mástil y plato.
      const sn = g2[7] * (q ? q[7] : 1); c.strokeStyle = '#5cc28a'; c.lineWidth = 1.2; c.beginPath(); c.moveTo(x0 + L * 0.7, cy0 - Hh * 0.5); c.lineTo(x0 + L * 0.7, cy0 - Hh * 0.5 - 6 - sn * 90); c.stroke(); c.beginPath(); c.arc(x0 + L * 0.7, cy0 - Hh * 0.5 - 6 - sn * 90, 3 + sn * 18, Math.PI * 0.15, Math.PI * 0.85); c.stroke();
    } else if (n.cls === 'carguero' || n.cls === 'granelero' || n.cls === 'bazar') {
      const carga = [n.carga].concat(n.extra || []).filter(Boolean); let k = 0; const tot = def.cmax || 1;
      for (const lot of carga) { const cols = Math.max(1, Math.round(lot.q / tot * 14)); c.fillStyle = S.bien[lot.c].col; for (let i = 0; i < cols && k < 14; i++, k++) c.fillRect(x0 + L * 0.06 + k * L * 0.055, cy0 - Hh * 0.36, L * 0.045, Hh * 0.72); }
      c.strokeStyle = 'rgba(0,0,0,0.35)'; for (let i = 0; i < 14; i++) c.strokeRect(x0 + L * 0.06 + i * L * 0.055, cy0 - Hh * 0.36, L * 0.045, Hh * 0.72);
    } else if (n.cls === 'granja') { c.fillStyle = 'rgba(92,194,138,0.7)'; for (let i = 0; i < 7; i++) c.fillRect(x0 + L * (0.15 + i * 0.1), cy0 - Hh * 0.3, L * 0.06, Hh * 0.6); }
    else if (n.cls === 'arca') { c.strokeStyle = '#d6b24a'; c.lineWidth = 2; for (let i = 0; i < 5; i++) { c.beginPath(); c.ellipse(x0 + L * (0.2 + i * 0.15), cy0, 4, Hh / 2, 0, 0, TAU); c.stroke(); } }
    else if (n.cls === 'prision') { c.strokeStyle = '#111'; c.lineWidth = 2; for (let i = 0; i < 12; i++) { c.beginPath(); c.moveTo(x0 + L * (0.08 + i * 0.065), cy0 - Hh * 0.4); c.lineTo(x0 + L * (0.08 + i * 0.065), cy0 + Hh * 0.4); c.stroke(); } }
    // Daños y cicatrices: donde le dieron de verdad.
    const rr = new S.Rng(S.hash(n.id, 'dano')); c.fillStyle = 'rgba(10,8,6,0.75)'; for (let i = 0; i < Math.round(n.dan * 9) + Math.min(4, n.cic.length); i++) { c.beginPath(); c.ellipse(x0 + rr.r(0.1, 0.9) * L, cy0 + rr.r(-0.35, 0.35) * Hh, rr.r(2, 6), rr.r(1.5, 4), rr.r(0, 3), 0, TAU); c.fill(); }
    if (!n.vivo) { c.fillStyle = 'rgba(7,10,18,0.6)'; c.fillRect(x, y, w, h); c.strokeStyle = '#e0564f'; c.lineWidth = 2; c.beginPath(); c.moveTo(x + 8, y + 8); c.lineTo(x + w - 8, y + h - 8); c.stroke(); }
    c.fillStyle = 'rgba(215,222,234,0.8)'; c.font = '10px Segoe UI, sans-serif'; c.textAlign = 'left'; c.textBaseline = 'alphabetic'; c.fillText(n.nom.toUpperCase() + (d ? ' · ' + d.nom : ''), x + 5, y + h - 5);
    if (n.cascos > 1) { c.textAlign = 'right'; c.fillText('× ' + n.cascos, x + w - 5, y + 12); }
    c.restore();
  };
  // Silueta pequeña para el mapa (vista de cerca): mirando hacia +x.
  A.naveMini = function (c, cls, L, col, borde) {
    const an = cls === 'acorazado' ? 0.42 : cls === 'crucero' ? 0.36 : cls === 'carguero' || cls === 'granelero' ? 0.5 : 0.32;
    c.fillStyle = col; c.beginPath(); c.moveTo(L, 0); c.lineTo(L * 0.2, -L * an); c.lineTo(-L * 0.8, -L * an * 0.85); c.lineTo(-L, -L * an * 0.4); c.lineTo(-L, L * an * 0.4); c.lineTo(-L * 0.8, L * an * 0.85); c.lineTo(L * 0.2, L * an); c.closePath(); c.fill();
    if (borde) { c.strokeStyle = borde; c.lineWidth = 1; c.stroke(); }
    c.fillStyle = 'rgba(255,190,110,0.9)'; c.fillRect(-L * 1.25, -L * an * 0.25, L * 0.25, L * an * 0.5);
  };
  // Icono de componente (0..7 en el orden del genoma).
  A.arma = function (c, x, y, s, k) {
    const col = S.tec.COL[k]; c.save(); c.translate(x, y); c.scale(s / 24, s / 24); c.strokeStyle = col; c.fillStyle = col; c.lineWidth = 2; c.lineCap = 'round';
    if (k === 0) { c.fillRect(3, 11, 9, 7); c.beginPath(); c.moveTo(12, 14); c.lineTo(22, 8); c.stroke(); c.beginPath(); c.arc(7, 19, 2.5, 0, TAU); c.fill(); }
    else if (k === 1) { c.beginPath(); c.arc(7, 12, 4, 0, TAU); c.fill(); c.beginPath(); c.moveTo(11, 12); c.lineTo(23, 12); c.stroke(); c.lineWidth = 1; c.beginPath(); c.moveTo(11, 9); c.lineTo(20, 6); c.moveTo(11, 15); c.lineTo(20, 18); c.stroke(); }
    else if (k === 2) { c.beginPath(); c.moveTo(21, 4); c.lineTo(13, 6); c.lineTo(5, 14); c.lineTo(10, 19); c.lineTo(18, 11); c.closePath(); c.fill(); c.fillStyle = '#e2703a'; c.beginPath(); c.moveTo(5, 14); c.lineTo(2, 22); c.lineTo(10, 19); c.fill(); }
    else if (k === 3) { c.beginPath(); c.moveTo(12, 3); c.lineTo(20, 6); c.lineTo(20, 13); c.quadraticCurveTo(19, 19, 12, 22); c.quadraticCurveTo(5, 19, 4, 13); c.lineTo(4, 6); c.closePath(); c.fill(); }
    else if (k === 4) { c.beginPath(); c.arc(12, 14, 9, Math.PI * 1.1, Math.PI * 1.9); c.stroke(); c.beginPath(); c.arc(12, 14, 5, Math.PI * 1.1, Math.PI * 1.9); c.stroke(); c.beginPath(); c.arc(12, 16, 2.5, 0, TAU); c.fill(); }
    else if (k === 5) { c.fillRect(8, 13, 8, 7); c.beginPath(); c.moveTo(12, 13); c.lineTo(17, 5); c.stroke(); for (let i = 0; i < 3; i++) { c.beginPath(); c.arc(19 + i, 3 + i * 3, 1, 0, TAU); c.fill(); } }
    else if (k === 6) { c.beginPath(); c.moveTo(14, 5); c.lineTo(22, 8); c.lineTo(22, 16); c.lineTo(14, 19); c.closePath(); c.fill(); c.fillStyle = '#ffd24f'; c.beginPath(); c.moveTo(14, 8); c.lineTo(2, 12); c.lineTo(14, 16); c.fill(); }
    else { c.beginPath(); c.arc(12, 10, 8, Math.PI * 0.1, Math.PI * 0.9); c.stroke(); c.beginPath(); c.moveTo(12, 12); c.lineTo(12, 22); c.moveTo(7, 22); c.lineTo(17, 22); c.stroke(); c.beginPath(); c.arc(12, 9, 2, 0, TAU); c.fill(); }
    c.restore();
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
