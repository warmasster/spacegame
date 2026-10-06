// Historia grande en pantalla: la pestaña «Historia» (épocas, mapa de calor de temas por año, figuras por peso
// histórico, credos, peste) y la biografía escrita de cada persona en su ficha.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI; const P = U.paneles; const V = U.viz; const Pr = U.prefs; const C = V.SEM; const H = S.his;
  const { L, esc, n0, pc, av, ban, tok, estadoDe, ROL } = P.h;
  const AB = () => (U.estado ? U.estado.abiertos : null);
  const NIVCOL = ['#8793ab', '#5fb0c9', '#5cc28a', '#e0b84c', '#ff6fb5'];
  const TEMACOL = { guerra: '#e8635c', revuelta: '#ff6fb5', hambre: '#e0b84c', peste: '#6fdc8c', paz: '#5fb0c9', figura: '#a98bff', caos: '#ff9f43' };
  const TEMANOM = { guerra: 'guerra', revuelta: 'revuelta', hambre: 'hambre', peste: 'peste', paz: 'calma', figura: 'una persona', caos: 'de todo a la vez' };
  const FIGICO = { caudillo: '📣', profeta: '🕯', martir: '🩸', invicto: '⚓', reformador: '⚖', tirano: '⛓', genio: '💡', unificador: '👑', sanador: '✚' };
  const insNivel = (b) => '<span class="ins" style="border-color:' + NIVCOL[b.nivel] + ';color:' + NIVCOL[b.nivel] + '">' + b.nivelNom + '</span>';
  const insFig = (m, p) => (p.fig && S.reg.figura[p.fig] ? V.insignia((FIGICO[p.fig] || '★') + ' ' + S.reg.figura[p.fig].nom, 'aviso') : '');

  // ── La biografía, en la ficha de la persona.
  P.epiteto = (p) => { const e = H.epiteto(P._m, p); return e ? ' <span class="epi">' + esc(e) + '</span>' : ''; };
  P.bioHtml = function (p) {
    const m = P._m; const b = H.biografia(m, p);
    let h = '<div class="bio n' + b.nivel + '" style="--c:' + NIVCOL[b.nivel] + '"><div class="cabb">' + insNivel(b) + ' ' + insFig(m, p) + ' <span class="nota">puesto ' + n0(b.puesto) + ' de ' + n0(m.per.length) + ' por peso en la historia</span></div>';
    h += '<div class="met tres"><div><span class="l">Peso histórico</span><b>' + V.num(b.peso) + '</b>' + V.medidor(b.E, NIVCOL[b.nivel]) + '</div><div><span class="l">Épica</span><b>' + b.E.toFixed(2) + '</b>' + V.medidor(b.E, C.ambar) + '</div><div><span class="l">Drama</span><b>' + b.D.toFixed(2) + '</b>' + V.medidor(b.D, C.rojo) + '</div></div>';
    for (const s of b.secciones) h += '<h4>' + s.t + '</h4>' + s.p.map(x => '<p>' + tok(x) + '</p>').join('');
    return h + '<p class="nota">Texto generado: las frases las elige su peso en la historia (cuánto de lo que pasó después cuelga de lo que hizo) y lo dramático de su vida, no el azar.</p></div>';
  };
  const extra0 = P.extraPersona; P.extraPersona = (p) => P.bioHtml(p) + extra0(p) + (p.fig && p.vivo ? '<div class="filtro"><button data-accion="capaFama">🌡 Ver en el mapa dónde se le conoce</button></div>' : '');

  // ── La pestaña.
  P.historia = function (mm) {
    P.usar(mm); const m = mm; const q = H.pesos(m); const ep = H.epocas(m);
    let h = '<h2>Historia</h2><p class="nota">La historia de este mundo vista de lejos: en qué épocas se parte, qué pasó cada año y quién pesó más en ella. Nada está escrito de antemano: sale de los hechos y de sus causas.</p>';
    // Épocas: una banda proporcional a los años, y debajo cada una con su nombre.
    const a0 = ep.length ? ep[0].a0 : 0, a1 = ep.length ? ep[ep.length - 1].a1 : 0; const tot = Math.max(1, a1 - a0 + 1);
    h += '<h3>Épocas</h3><div class="epocas">' + ep.map(e => '<i style="width:' + ((e.a1 - e.a0 + 1) / tot * 100).toFixed(2) + '%;background:' + TEMACOL[e.tema] + '" title="' + esc(e.nom) + '"></i>').join('') + '</div>';
    h += ep.map(e => '<div class="fila"><span class="chip" style="background:' + TEMACOL[e.tema] + ';margin-top:5px"></span><div><b>«' + esc(e.nom) + '»</b><br><span class="nota">años ' + e.a0 + ' a ' + e.a1 + ' · la marca ' + (e.fig >= 0 ? L('P', e.fig) : TEMANOM[e.tema]) + '</span></div></div>').join('');
    h += '<p class="nota">Los cortes los pone un algoritmo de puntos de cambio: parte la historia donde el perfil de cada año (guerra, revuelta, hambre, peste) deja de parecerse al de los anteriores.</p>';
    // Mapa de calor: temas × años.
    const filas = []; const de = {}; for (const [, l] of P.SECCIONES) for (const t of l) { const f = { id: t.id, nom: t.ico + ' ' + t.nom, d: [] }; filas.push(f); for (const k of (t.cuenta || t.ks)) de[k] = f; }
    const y0 = m.ev.length ? Math.floor(m.ev[0].t / S.ANIO) : 0, y1 = Math.floor(m.t / S.ANIO); const n = y1 - y0 + 1; for (const f of filas) f.d = new Array(n).fill(0);
    for (const e of m.ev) { const f = de[e.k]; if (f) f.d[Math.min(n - 1, Math.floor(e.t / S.ANIO) - y0)]++; }
    const vivas = filas.filter(f => f.d.some(x => x > 0));
    h += '<h3>Qué pasó cada año</h3><p class="nota">Cada casilla es un tema en un año: cuanto más encendida, más hubo (frente al peor año de ese tema). Pasa el ratón para ver la cifra; pulsa para abrir el tema.</p><canvas class="graf" data-calor="' + V.dato('calorHist', { filas: vivas, y0 }) + '" style="height:' + (vivas.length * 15 + 22) + 'px"></canvas>';
    // Figuras por peso histórico.
    h += '<h3>Quién pesó más</h3><p class="nota">Peso histórico = lo que pesan los hechos que protagonizó más una parte de todo lo que colgó de ellos después (se suma hacia atrás por el grafo de causas, con descuento del ' + Math.round((1 - H.LAMBDA) * 100) + ' % por eslabón), más sus muertes.</p>';
    const top = q.orden.slice(0, 30); const mx = top.length ? Math.max(1, q.w[top[0]]) : 1;
    top.forEach((id, i) => { const p = m.per[id]; if (q.w[id] <= 0) return; const niv = H.nivel(m, p); h += '<div class="fila pers"><span class="rk">' + (i + 1) + '</span>' + av('P' + id, 40) + '<div>' + L('P', id) + (p.vivo ? '' : ' †') + P.epiteto(p) + ' <span class="ins" style="border-color:' + NIVCOL[niv] + ';color:' + NIVCOL[niv] + '">' + H.NIVELES[niv] + '</span> ' + insFig(m, p) + '<br><span class="nota">' + (p.cargo && p.cargo.nom ? esc(p.cargo.nom) : (ROL[p.rol] || p.rol)) + (p.est >= 0 ? ' · ' + esc(m.est[p.est].nom) : '') + ' · conocid' + (p.sexo === 'M' ? 'a' : 'o') + ' en ' + (p.fama || 0) + ' mundos</span>' + V.medidor(q.w[id] / mx, NIVCOL[niv]) + '</div></div>'; });
    // Credos.
    const cre = (m.cre || []).filter(c => c.fieles > 1).sort((x, y) => y.fieles - x.fieles); let pob = 0; for (const a of m.ase) pob += a.pob;
    h += '<h3>Credos</h3>' + (cre.length ? cre.map(c => '<div class="flujo"><span class="chip" style="background:' + c.col + '"></span><b>' + esc(S.cap(c.nom)) + '</b> <span class="nota">la empezó ' + L('P', c.fundador) + ' · ' + S.fmt(c.fieles) + ' fieles · con peso en ' + (c.mundos || 0) + ' mundos' + (c.estado >= 0 && m.est[c.estado].vivo && m.est[c.estado].credo === c.id ? ' · fe de ' + esc(m.est[c.estado].nom) : '') + '</span>' + V.medidor(c.fieles / Math.max(1, pob), c.col) + '</div>').join('') + '<p class="nota">Una fe crece donde hay agravio y hueco (curva logística con techo) y viaja en las bodegas por las rutas con más carga. Capa del mapa: «Credos».</p>' : '<p class="nota">Ninguno todavía. Hace falta un predicador con carisma y fe, y que su nombre empiece a correr.</p>');
    // Peste.
    const pes = m.pes; h += '<h3>Peste</h3>' + (pes ? '<div class="tarjeta">' + (pes.vivo ? '<b>En curso</b> desde hace ' + P.h.dias(m.t - pes.t0) : 'La última acabó hace ' + P.h.dias(m.t - pes.t1)) + ' · empezó en ' + L('A', pes.origen) + '<div class="met tres"><div><span class="l">Muertos</span><b>' + n0(pes.muertos) + '</b></div><div><span class="l">Mundos tocados</span><b>' + pes.mundos + '</b></div><div><span class="l">R₀ (β/γ)</span><b>' + (pes.beta / pes.gamma).toFixed(1) + '</b></div></div><span class="nota">Modelo SIR en cada asentamiento; viaja con las tripulaciones que atracan. Mortalidad de los enfermos: ' + pc(pes.mort) + '. Capa del mapa: «Peste».</span></div>' : '<p class="nota">Ninguna, de momento. El riesgo sube con el hambre.</p>');
    return h;
  };

  // ── Mapa de calor en lienzo: filas (temas) × columnas (años). Cada fila se compara consigo misma.
  V.calor = function (cv, o) {
    const [c, w, h] = V.lienzo(cv); const t = Pr.tema(); cv._viz = { f: V.calor, o }; const filas = o.filas; if (!filas.length) return; const n = filas[0].d.length;
    const izq = Math.min(150, w * 0.36), abajo = 14, ch = (h - abajo) / filas.length, cw = (w - izq - 2) / n; const ac = Pr.acento();
    c.font = '11px Segoe UI Emoji, Segoe UI, sans-serif'; c.textBaseline = 'middle';
    filas.forEach((f, i) => {
      const mx = Math.max(1, ...f.d); const y = i * ch; c.fillStyle = o.fy === i ? t.txt : t.tenue; c.textAlign = 'left'; c.fillText(f.nom, 2, y + ch / 2, izq - 6);
      for (let j = 0; j < n; j++) { const v = f.d[j] / mx; c.fillStyle = v > 0 ? V.alfa(ac, 0.12 + 0.88 * Math.sqrt(v)) : V.alfa(t.txt, 0.04); c.fillRect(izq + j * cw + 0.5, y + 0.5, Math.max(1, cw - 1), ch - 1); }
    });
    c.textBaseline = 'alphabetic'; c.fillStyle = t.tenue; c.font = '10px Segoe UI, sans-serif'; c.textAlign = 'center'; const paso = n > 60 ? 10 : n > 24 ? 5 : n > 12 ? 2 : 1; for (let j = 0; j < n; j++) if ((o.y0 + j) % paso === 0) c.fillText(String(o.y0 + j), izq + (j + 0.5) * cw, h - 3);
    c.textAlign = 'left';
    cv._clic = (x, y) => { const j = Math.floor((x - izq) / cw), i = Math.floor(y / ch); return i >= 0 && i < filas.length ? { tema: filas[i].id, anio: j >= 0 && j < n ? o.y0 + j : null } : null; };
    if (o.cursor !== undefined && o.cursor !== null && o.cy !== undefined) {
      const j = Math.floor((o.cursor - izq) / cw), i = Math.floor(o.cy / ch); o.fy = i;
      if (i >= 0 && i < filas.length && j >= 0 && j < n) { c.strokeStyle = '#fff'; c.lineWidth = 1.5; c.strokeRect(izq + j * cw + 0.5, i * ch + 0.5, Math.max(1, cw - 1), ch - 1); const txt = 'año ' + (o.y0 + j) + ' · ' + filas[i].d[j]; c.font = '11px Segoe UI, sans-serif'; const tw = c.measureText(txt).width + 12; const bx = Math.min(w - tw - 2, Math.max(izq, izq + j * cw - tw / 2)), by = i * ch > 22 ? i * ch - 19 : (i + 1) * ch + 3; c.fillStyle = V.alfa(t.panel2, 0.96); c.strokeStyle = V.alfa(t.txt, 0.3); c.lineWidth = 1; c.beginPath(); if (c.roundRect) c.roundRect(bx, by, tw, 16, 4); else c.rect(bx, by, tw, 16); c.fill(); c.stroke(); c.fillStyle = t.txt; c.fillText(txt, bx + 6, by + 12); }
    } else o.fy = -1;
  };
  const pintarExtra0 = P.pintarExtra;
  P.pintarExtra = function (m, raiz) { if (pintarExtra0) pintarExtra0(m, raiz); for (const cv of raiz.querySelectorAll('canvas[data-calor]')) { const o = cv.dataset ? V.datos[cv.dataset.calor] : null; if (o) V.calor(cv, o); } };

  // En Personas, un filtro más: quién cambió la historia.
  P.filtroHistoricos = ['Los que cambiaron la historia', (m) => { const q = H.pesos(m); return q.orden.filter(i => q.w[i] > 0).map(i => m.per[i]); }, (m, p) => { const q = H.pesos(m); return '<span class="ins" style="border-color:' + NIVCOL[H.nivel(m, p)] + ';color:' + NIVCOL[H.nivel(m, p)] + '">' + H.NIVELES[H.nivel(m, p)] + '</span> ' + insFig(m, p) + ' peso ' + V.num(q.w[p.id]) + (H.epiteto(m, p) ? ' · «' + esc(H.epiteto(m, p)) + '»' : ''); }, (m, p) => H.pesos(m).w[p.id]];
})(typeof globalThis !== 'undefined' ? globalThis : this);
