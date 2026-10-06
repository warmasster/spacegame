// Gráficas y piezas de datos: series (área, línea o barras, con rejilla, cursor y valores al pasar el ratón),
// minigráficas, rosquillas, áreas apiladas y las piezas HTML que las acompañan (cifras, medidores, repartos,
// insignias). Todo toma colores y estilo de las preferencias de aspecto.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI = g.UI || {}; const Pr = U.prefs; const V = U.viz = {};
  const TAU = Math.PI * 2;
  V.SEM = { rojo: '#e8635c', verde: '#5cc28a', azul: '#5fb0c9', ambar: '#e0b84c', rosa: '#ff6fb5', violeta: '#a98bff', gris: '#8793ab' };
  const rgb = (c) => { if (c[0] === '#') { let s = c.slice(1); if (s.length === 3) s = s.replace(/./g, '$&$&'); const n = parseInt(s.slice(0, 6), 16); return [n >> 16 & 255, n >> 8 & 255, n & 255]; } const x = /(\d+)\D+(\d+)\D+(\d+)/.exec(c); return x ? [+x[1], +x[2], +x[3]] : [128, 128, 128]; };
  const alfa = V.alfa = (c, a) => { const [r, g2, b] = rgb(c); return 'rgba(' + r + ',' + g2 + ',' + b + ',' + a + ')'; };
  const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;');
  V.lienzo = function (cv) {
    const dpr = g.devicePixelRatio || 1; const w = +cv.dataset.w || cv.clientWidth || 360, h = +cv.dataset.h || cv.clientHeight || 120;
    const W = Math.round(w * dpr), H = Math.round(h * dpr); if (cv.width !== W) cv.width = W; if (cv.height !== H) cv.height = H;
    const c = cv.getContext('2d'); c.setTransform(dpr, 0, 0, dpr, 0, 0); c.clearRect(0, 0, w, h); return [c, w, h];
  };
  // Tope «redondo» para el eje: 1, 2, 2,5, 5 o 10 por la potencia de diez que toque.
  V.bonito = function (x) { if (!(x > 0)) return 1; const p = Math.pow(10, Math.floor(Math.log10(x))); const f = x / p; return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * p; };
  const num = V.num = (x) => { const a = Math.abs(x); return a >= 1000 ? S.fmt(x) : a >= 100 ? x.toFixed(0) : a >= 10 ? x.toFixed(1) : a === 0 ? '0' : x.toFixed(2); };
  // Curva monótona por los puntos (no se pasa de frenada entre dos muestras).
  function trazo(c, p, suave) {
    if (!p.length) return; c.moveTo(p[0][0], p[0][1]);
    if (!suave || p.length < 3) { for (let i = 1; i < p.length; i++) c.lineTo(p[i][0], p[i][1]); return; }
    const n = p.length; const d = new Array(n - 1), mt = new Array(n);
    for (let i = 0; i < n - 1; i++) d[i] = (p[i + 1][1] - p[i][1]) / Math.max(1e-9, p[i + 1][0] - p[i][0]);
    mt[0] = d[0]; mt[n - 1] = d[n - 2]; for (let i = 1; i < n - 1; i++) mt[i] = d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2;
    for (let i = 0; i < n - 1; i++) { if (d[i] === 0) { mt[i] = 0; mt[i + 1] = 0; continue; } const a = mt[i] / d[i], b = mt[i + 1] / d[i]; const s = a * a + b * b; if (s > 9) { const k = 3 / Math.sqrt(s); mt[i] = k * a * d[i]; mt[i + 1] = k * b * d[i]; } }
    for (let i = 0; i < n - 1; i++) { const dx = (p[i + 1][0] - p[i][0]) / 3; c.bezierCurveTo(p[i][0] + dx, p[i][1] + mt[i] * dx, p[i + 1][0] - dx, p[i + 1][1] - mt[i + 1] * dx, p[i + 1][0], p[i + 1][1]); }
  }

  // ── Series en el tiempo. o = { series: [{ nom, col, d }], t (tiempos), max, min, presente, fecha(t), tipo, eje: 'anios'|'dias', cursor }.
  V.series = function (cv, o) {
    const [c, w, h] = V.lienzo(cv); const t = Pr.tema(); const v = Pr.v; cv._viz = { f: V.series, o };
    let n = 0; for (const s of o.series) if (s.d && s.d.length > n) n = s.d.length;
    c.font = '10px Segoe UI, sans-serif'; c.textBaseline = 'alphabetic';
    if (n < 2) { c.fillStyle = t.tenue; c.textAlign = 'center'; c.fillText('aún no hay datos', w / 2, h / 2 + 3); c.textAlign = 'left'; return; }
    let i0 = 0; if (o.t && v.periodo > 0 && !o.sinPeriodo) { const lim = o.t[Math.min(n, o.t.length) - 1] - v.periodo * S.ANIO; while (i0 < n - 2 && o.t[i0] < lim) i0++; }
    const N = n - i0; let mn = o.min !== undefined ? o.min : 0; let mx = o.max;
    if (mx === undefined) {
      let hi = -Infinity, lo = Infinity; for (const s of o.series) for (let i = i0; i < s.d.length; i++) { const x = s.d[i]; if (x === x) { if (x > hi) hi = x; if (x < lo) lo = x; } }
      if (!(hi > -Infinity)) { hi = 1; lo = 0; }
      // Una serie que apenas se mueve lejos del cero (la población) se mira de cerca: el eje no arranca en cero.
      if (o.min === undefined && lo > 0 && hi - lo < 0.35 * hi) { const hol = Math.max((hi - lo) * 0.25, hi * 0.02); mn = lo - hol; mx = hi + hol; }
      else mx = V.bonito(hi > mn ? hi : mn + 1);
    }
    const pl = 3, pr = 3, pt = 9, pb = 15; const X = (i) => pl + (i - i0) / (N - 1) * (w - pl - pr), Y = (x) => h - pb - (S.clamp(x, mn, mx) - mn) / (mx - mn) * (h - pt - pb);
    // Rejilla: cuatro alturas y una raya por año (o cada cinco, si hay muchos).
    if (v.rejilla) {
      c.lineWidth = 1; c.strokeStyle = alfa(t.txt, 0.07); c.beginPath(); for (const f of [0.25, 0.5, 0.75, 1]) { const y = Math.round(Y(mn + (mx - mn) * f)) + 0.5; c.moveTo(pl, y); c.lineTo(w - pr, y); } c.stroke();
      if (o.t && o.eje !== 'dias') { const a0 = Math.ceil(o.t[i0] / S.ANIO), a1 = Math.floor(o.t[n - 1] / S.ANIO); const paso = a1 - a0 > 40 ? 10 : a1 - a0 > 14 ? 5 : a1 - a0 > 6 ? 2 : 1; c.strokeStyle = alfa(t.txt, 0.05); c.fillStyle = alfa(t.tenue, 0.9); c.textAlign = 'center'; c.beginPath(); for (let a = Math.ceil(a0 / paso) * paso; a <= a1; a += paso) { const x = pl + (a * S.ANIO - o.t[i0]) / Math.max(1, o.t[n - 1] - o.t[i0]) * (w - pl - pr); c.moveTo(Math.round(x) + 0.5, pt); c.lineTo(Math.round(x) + 0.5, h - pb); if (x > 18 && x < w - 18) c.fillText('año ' + a, x, h - 3); } c.stroke(); c.textAlign = 'left'; }
    }
    c.strokeStyle = alfa(t.txt, 0.18); c.beginPath(); c.moveTo(pl, h - pb + 0.5); c.lineTo(w - pr, h - pb + 0.5); c.stroke();
    // Desde cuándo miras: lo de antes es prehistoria.
    if (o.t && o.presente > o.t[i0] && o.presente < o.t[n - 1]) { const x = pl + (o.presente - o.t[i0]) / (o.t[n - 1] - o.t[i0]) * (w - pl - pr); c.fillStyle = alfa(t.txt, 0.035); c.fillRect(pl, pt, x - pl, h - pt - pb); c.strokeStyle = alfa(Pr.acento(), 0.5); c.setLineDash([2, 3]); c.beginPath(); c.moveTo(x, pt); c.lineTo(x, h - pb); c.stroke(); c.setLineDash([]); }
    const tipo = o.tipo || v.graf; const una = o.series.length === 1;
    o.series.forEach((s, j) => {
      const col = s.col || Pr.col(j); const p = []; for (let i = i0; i < s.d.length; i++) { const x = s.d[i]; if (x === x) p.push([X(i), Y(x)]); }
      if (p.length < 2) return;
      if (tipo === 'barras' && una) {
        const nb = Math.max(2, Math.min(p.length, Math.floor((w - pl - pr) / 4))); const bw = (w - pl - pr) / nb; const gr = c.createLinearGradient(0, pt, 0, h - pb); gr.addColorStop(0, col); gr.addColorStop(1, alfa(col, 0.35)); c.fillStyle = gr;
        for (let b = 0; b < nb; b++) { const a = Math.floor(b / nb * p.length), z = Math.max(a + 1, Math.floor((b + 1) / nb * p.length)); let y = 0; for (let i = a; i < z; i++) y += p[i][1]; y /= z - a; c.fillRect(pl + b * bw + 0.5, y, Math.max(1, bw - 1), h - pb - y); }
        return;
      }
      if (tipo !== 'linea' && (una || o.series.length <= 2)) { const gr = c.createLinearGradient(0, pt, 0, h - pb); gr.addColorStop(0, alfa(col, una ? 0.42 : 0.26)); gr.addColorStop(1, alfa(col, 0.02)); c.fillStyle = gr; c.beginPath(); trazo(c, p, v.suave); c.lineTo(p[p.length - 1][0], h - pb); c.lineTo(p[0][0], h - pb); c.closePath(); c.fill(); }
      c.save(); if (v.brillo) { c.shadowColor = alfa(col, 0.7); c.shadowBlur = 7; } c.strokeStyle = col; c.lineWidth = 1.8; c.lineJoin = 'round'; c.beginPath(); trazo(c, p, v.suave); c.stroke(); c.restore();
      const u = p[p.length - 1]; c.fillStyle = col; c.beginPath(); c.arc(u[0] - 1, u[1], 2.6, 0, TAU); c.fill();
    });
    c.fillStyle = alfa(t.tenue, 0.95); c.textAlign = 'left'; c.fillText(num(mx), pl + 2, pt + 8); if (mn !== 0 && o.eje !== 'dias') c.fillText(num(mn), pl + 2, h - pb - 3);
    if (o.eje === 'dias') { c.textAlign = 'right'; c.fillText((n - 1) + (n - 1 === 1 ? ' día' : ' días'), w - pr - 2, h - 3); c.textAlign = 'left'; c.fillText('día 0', pl + 2, h - 3); }
    // Cursor: la muestra más cercana, con su fecha y el valor de cada serie.
    if (o.cursor !== undefined && o.cursor !== null) {
      const i = Math.round(S.clamp(i0 + (o.cursor - pl) / (w - pl - pr) * (N - 1), i0, n - 1)); const x = X(i);
      c.strokeStyle = alfa(t.txt, 0.45); c.lineWidth = 1; c.beginPath(); c.moveTo(Math.round(x) + 0.5, pt); c.lineTo(Math.round(x) + 0.5, h - pb); c.stroke();
      const lin = [o.t && o.fecha ? o.fecha(o.t[i]) : o.eje === 'dias' ? 'día ' + i : '']; const cols = [null];
      o.series.forEach((s, j) => { const y = s.d[i]; if (y !== y || y === undefined) return; const col = s.col || Pr.col(j); c.fillStyle = t.panel; c.beginPath(); c.arc(x, Y(y), 4, 0, TAU); c.fill(); c.fillStyle = col; c.beginPath(); c.arc(x, Y(y), 2.8, 0, TAU); c.fill(); lin.push((o.series.length > 1 && s.nom ? s.nom + ': ' : '') + (o.fmt ? o.fmt(y) : num(y))); cols.push(col); });
      c.font = '11px Segoe UI, sans-serif'; let tw = 0; for (const l of lin) tw = Math.max(tw, c.measureText(l).width); const bw = tw + 20, bh = lin.length * 13 + 6; const bx = x + 10 + bw > w ? x - 10 - bw : x + 10, by = pt;
      c.fillStyle = alfa(t.panel2, 0.95); c.strokeStyle = alfa(t.txt, 0.25); c.beginPath(); if (c.roundRect) c.roundRect(bx, by, bw, bh, 5); else c.rect(bx, by, bw, bh); c.fill(); c.stroke();
      lin.forEach((l, k) => { const y = by + 13 + k * 13; if (cols[k]) { c.fillStyle = cols[k]; c.fillRect(bx + 6, y - 7, 6, 6); } c.fillStyle = k ? t.txt : t.tenue; c.fillText(l, bx + (cols[k] ? 16 : 6), y); });
    }
  };

  // ── Minigráfica: una línea con su sombra, sin ejes.
  V.spark = function (cv, d, col) {
    const [c, w, h] = V.lienzo(cv); if (!d || d.length < 2) return; let n = d.length; let i0 = Math.max(0, n - 120);
    let mx = -Infinity, mn = Infinity; for (let i = i0; i < n; i++) { const x = d[i]; if (x === x) { if (x > mx) mx = x; if (x < mn) mn = x; } } if (!(mx > mn)) { mx = mn + 1; mn = mn - 1; }
    const p = []; for (let i = i0; i < n; i++) { const x = d[i]; if (x === x) p.push([1 + (i - i0) / (n - 1 - i0) * (w - 4), h - 2 - (x - mn) / (mx - mn) * (h - 5)]); } if (p.length < 2) return;
    const gr = c.createLinearGradient(0, 0, 0, h); gr.addColorStop(0, alfa(col, 0.35)); gr.addColorStop(1, alfa(col, 0)); c.fillStyle = gr; c.beginPath(); trazo(c, p, true); c.lineTo(p[p.length - 1][0], h); c.lineTo(p[0][0], h); c.closePath(); c.fill();
    c.strokeStyle = col; c.lineWidth = 1.4; c.beginPath(); trazo(c, p, true); c.stroke(); const u = p[p.length - 1]; c.fillStyle = col; c.beginPath(); c.arc(u[0], u[1], 2, 0, TAU); c.fill();
  };

  // ── Rosquilla: partes = [{ nom, v, col }]; en el centro, una cifra y su nombre.
  V.dona = function (cv, o) {
    const [c, w, h] = V.lienzo(cv); const t = Pr.tema(); let tot = 0; for (const p of o.partes) tot += Math.max(0, p.v); if (!(tot > 0)) return;
    const cx = w / 2, cy = h / 2, R = Math.min(w, h) / 2 - 2, r = R * 0.62; let a = -Math.PI / 2; const hueco = o.partes.length > 1 ? 0.035 : 0;
    for (const p of o.partes) { const da = Math.max(0, p.v) / tot * TAU; if (da <= 0) continue; const h2 = Math.min(hueco, da * 0.3); c.fillStyle = p.col; c.beginPath(); c.arc(cx, cy, R, a + h2 / 2, a + da - h2 / 2); c.arc(cx, cy, r, a + da - h2 / 2, a + h2 / 2, true); c.closePath(); c.fill(); a += da; }
    c.textAlign = 'center'; c.fillStyle = t.txt; c.font = 'bold ' + Math.round(R * 0.36) + 'px Segoe UI, sans-serif'; c.fillText(o.centro || '', cx, cy + 2); c.fillStyle = t.tenue; c.font = Math.round(Math.max(9, R * 0.19)) + 'px Segoe UI, sans-serif'; c.fillText(o.sub || '', cx, cy + R * 0.28 + 4); c.textAlign = 'left';
  };

  // ── Área apilada (cada muestra suma 1): capas = [{ col, d }].
  V.apilada = function (cv, o) {
    const [c, w, h] = V.lienzo(cv); const n = o.capas.length ? o.capas[0].d.length : 0; if (n < 2) return; const pb = 13;
    const ac = new Array(n).fill(0);
    for (const capa of o.capas) { const base = ac.slice(); for (let i = 0; i < n; i++) ac[i] += capa.d[i]; c.fillStyle = capa.col; c.beginPath(); for (let i = 0; i < n; i++) { const x = i / (n - 1) * w, y = (h - pb) * (1 - ac[i]); if (i) c.lineTo(x, y); else c.moveTo(x, y); } for (let i = n - 1; i >= 0; i--) c.lineTo(i / (n - 1) * w, (h - pb) * (1 - base[i])); c.closePath(); c.fill(); c.strokeStyle = 'rgba(0,0,0,0.25)'; c.lineWidth = 0.6; c.stroke(); }
    c.fillStyle = Pr.tema().tenue; c.font = '10px Segoe UI, sans-serif'; c.fillText(o.ini || '', 2, h - 2); c.textAlign = 'right'; c.fillText(o.fin || '', w - 2, h - 2); c.textAlign = 'left';
  };

  // ── Datos que una pestaña deja preparados para los lienzos que acaba de escribir (rosquillas, series propias).
  V.datos = {};
  V.dato = function (id, x) { V.datos[id] = x; return id; };
  V.pintar = function (m, raiz) {
    const fecha = (t) => m.fecha(t);
    for (const cv of raiz.querySelectorAll('canvas[data-series]')) {
      if (!cv.dataset.series) continue;
      const claves = cv.dataset.series.split(','); const nombres = (cv.dataset.nombres || '').split('|');
      V.series(cv, { t: m.series.t || [], presente: m.presente, fecha, max: cv.dataset.max ? +cv.dataset.max : undefined, series: claves.map((k, j) => ({ nom: nombres[j] || '', d: m.series[k] || [], col: cv.dataset.col || undefined })) });
    }
    for (const cv of raiz.querySelectorAll('canvas[data-spark]')) { if (!cv.dataset.spark) continue; const d = m.series[cv.dataset.spark]; V.spark(cv, d, cv.dataset.col || Pr.acento()); }
    for (const cv of raiz.querySelectorAll('canvas[data-dona]')) { const o = V.datos[cv.dataset.dona]; if (o) V.dona(cv, o); }
    for (const cv of raiz.querySelectorAll('canvas[data-apilada]')) { const o = V.datos[cv.dataset.apilada]; if (o) V.apilada(cv, o); }
    for (const cv of raiz.querySelectorAll('canvas[data-viz]')) { const o = V.datos[cv.dataset.viz]; if (o) V.series(cv, o); }
    for (const cv of raiz.querySelectorAll('canvas[data-duelo]')) { const o = V.datos[cv.dataset.duelo]; if (o) V.series(cv, Object.assign({ eje: 'dias', tipo: Pr.v.graf === 'barras' ? 'area' : undefined, fmt: (x) => Math.round(x).toLocaleString('es-ES') }, o)); }
  };
  // El ratón sobre una gráfica: se repinta con el cursor en su sitio; al salir, se quita.
  V.raton = function (cont) {
    let ult = null; const quita = () => { if (ult && ult._viz) { ult._viz.o.cursor = null; ult._viz.f(ult, ult._viz.o); } ult = null; };
    cont.addEventListener('mousemove', (e) => { const cv = e.target; if (!cv || !cv._viz) { quita(); return; } if (ult && ult !== cv) quita(); ult = cv; const r = cv.getBoundingClientRect(); cv._viz.o.cursor = e.clientX - r.left; cv._viz.o.cy = e.clientY - r.top; cv._viz.f(cv, cv._viz.o); });
    cont.addEventListener('mouseleave', quita);
  };

  // ── Piezas HTML.
  // Una cifra grande con su nombre y, si se quiere, la minigráfica de su serie.
  V.kpi = function (valor, nombre, o) { o = o || {}; return '<div class="kpi"' + (o.tema ? ' data-tema="' + o.tema + '" title="Ver su historia"' : '') + (o.col ? ' style="--c:' + o.col + '"' : '') + '><span class="v">' + (o.ico ? '<i>' + o.ico + '</i>' : '') + valor + '</span><span class="l">' + nombre + '</span>' + (o.sub ? '<span class="s">' + o.sub + '</span>' : '') + (o.spark ? '<canvas data-spark="' + o.spark + '"' + (o.col ? ' data-col="' + o.col + '"' : '') + '></canvas>' : '') + '</div>'; };
  V.kpis = (l) => '<div class="kpis">' + l.filter(x => x).join('') + '</div>';
  // Un hecho contado: icono, cifra y nombre, en una ficha pequeña.
  V.cuenta = (ico, n, nombre, tema) => '<div class="cta' + (n === '0' ? ' cero' : '') + '"' + (tema ? ' data-tema="' + tema + '" title="Ver cada uno"' : '') + '><i>' + ico + '</i><b>' + n + '</b><span>' + nombre + '</span></div>';
  V.medidor = function (f, col, ancho) { return '<span class="med"' + (ancho ? ' style="width:' + ancho + 'px"' : '') + '><i style="width:' + (S.clamp(f, 0, 1) * 100).toFixed(1) + '%;background:' + (col || 'var(--acento)') + '"></i></span>'; };
  // Una barra partida en trozos de color, con su leyenda debajo.
  V.reparto = function (partes, o) {
    o = o || {}; let tot = 0; for (const p of partes) tot += Math.max(0, p.v); if (!(tot > 0)) return '';
    return '<span class="reparto">' + partes.filter(p => p.v > 0).map(p => '<i style="width:' + (p.v / tot * 100).toFixed(2) + '%;background:' + p.col + '" title="' + esc(p.nom) + '"></i>').join('') + '</span>' + (o.sinLeyenda ? '' : '<div class="leyenda">' + partes.filter(p => p.v > 0).map(p => '<span><span class="chip" style="background:' + p.col + '"></span>' + esc(p.nom) + ' <b>' + (o.fmt ? o.fmt(p.v) : Math.round(p.v / tot * 100) + ' %') + '</b></span>').join('') + '</div>');
  };
  V.insignia = (txt, tono) => '<span class="ins ' + (tono || '') + '">' + txt + '</span>';
  V.leyenda = (l) => '<div class="leyenda">' + l.map(x => '<span><span class="chip" style="background:' + x[0] + '"></span>' + x[1] + '</span>').join('') + '</div>';
  // Título de una gráfica con su valor de ahora a la derecha.
  V.titulo = (nombre, valor) => '<div class="gtit"><span>' + nombre + '</span>' + (valor !== undefined && valor !== '' ? '<b>' + valor + '</b>' : '') + '</div>';
})(typeof globalThis !== 'undefined' ? globalThis : this);
