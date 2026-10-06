// La ficha del mundo con todo lo que ha pasado, tema a tema (cada cifra se abre en su historia entera), el
// historial completo de cualquier cosa con el filtro de importancia de la crónica, y el historiador dibujado
// como una línea de tiempo con ramas: cada hecho es un punto y las líneas unen cada causa con lo que provocó.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI; const P = U.paneles; const V = U.viz; const Pr = U.prefs; const C = V.SEM;
  const { L, esc, n0, pc, evHtml, txtEv, estadoDe, dias } = P.h;
  const AB = () => (U.estado ? U.estado.abiertos : null);
  const IMP = ['todo', 'notable', 'grande', 'histórico'];
  const selImp = (op, v) => 'Importancia <select data-op="' + op + '">' + IMP.map((x, i) => '<option value="' + i + '"' + (i === v ? ' selected' : '') + '>' + x + '</option>').join('') + '</select>';
  const nomK = (k) => k.replace(/_/g, ' ');

  // ── Temas: qué clases de hecho cuenta cada cifra, qué series la acompañan y qué clasificación de «ahora mismo».
  const T = (id, ico, nom, ks, o) => Object.assign({ id, ico, nom, ks }, o || {});
  P.SECCIONES = [
    ['Guerra', [
      T('guerras', '⚔', 'guerras', ['guerra', 'guerra_civil', 'guerra_recibida', 'paz', 'fin_guerra_civil', 'incidente'], { cuenta: ['guerra', 'guerra_civil'], series: ['guerras'], rank: 'guerras' }),
      T('batallas', '💥', 'batallas', ['batalla', 'batalla_fin', 'desastre_flota', 'flota_parada', 'motin_flota'], { cuenta: ['batalla'], series: ['fragatas', 'pecios'] }),
      T('conquistas', '🚩', 'conquistas', ['conquista', 'desembarco', 'asalto_fallido', 'capital'], { cuenta: ['conquista'] }),
      T('alianzas', '🤝', 'alianzas', ['alianza', 'alianza_rota', 'alianza_fin', 'transferencia'], { cuenta: ['alianza'] }),
      T('armamento', '🚀', 'diseños nuevos', ['diseno', 'modernizacion', 'ingenieria_inversa', 'armada', 'botadura', 'genio', 'espionaje', 'planos_falsos', 'veteranos', 'chatarra'], { cuenta: ['diseno'], series: ['tec', 'fragatas'] }),
    ]],
    ['Revuelta', [
      T('revoluciones', '🔥', 'revoluciones', ['revolucion', 'insurreccion', 'insurreccion_aplastada', 'motin_pan', 'terror', 'termidor'], { cuenta: ['revolucion'], series: ['calle', 'eficacia'], rank: 'calle' }),
      T('masacres', '🩸', 'masacres', ['masacre'], { series: ['R'] }),
      T('figuras', '🌟', 'figuras históricas', ['figura', 'martirio'], { series: ['figuras'] }),
      T('credos', '🕯', 'credos y oleadas', ['credo', 'conversion', 'oleada', 'guerra_santa'], { series: ['credos'] }),
      T('independencias', '🕊', 'independencias', ['independencia', 'fragmentacion', 'exodo'], { cuenta: ['independencia'] }),
      T('huelgas', '✊', 'huelgas', ['huelga', 'huelga_fin', 'concesion', 'sindicato'], { cuenta: ['huelga'] }),
      T('facciones', '🏴', 'facciones nacidas', ['faccion', 'protofaccion', 'cisma', 'fusion', 'disolucion', 'celula', 'conector', 'radical', 'pintadas', 'armas', 'sucesion_faccion', 'redada', 'contraelite', 'profanacion', 'donativo', 'sabotaje'], { cuenta: ['faccion'], series: ['facciones', 'l2min', 'docreb'] }),
    ]],
    ['Estado', [
      T('sucesiones', '👑', 'sucesiones', ['sucesion', 'interregno', 'herencia', 'indeciso', 'repudio', 'cruzada'], { cuenta: ['sucesion'] }),
      T('golpes', '🗡', 'golpes', ['golpe', 'atentado', 'purga_cocina'], { cuenta: ['golpe'] }),
      T('caidos', '🏚', 'Estados caídos', ['estado_cae'], { series: ['estados'] }),
      T('hacienda', '🪙', 'bancarrotas', ['bancarrota', 'recortes', 'imprime', 'impuestos', 'prestamo', 'tesoro', 'sin_paga', 'paga', 'confiscacion', 'quiebra', 'desfalco'], { cuenta: ['bancarrota'], series: ['ipc'] }),
      T('corte', '🎭', 'intrigas de palacio', ['informe', 'corrupcion', 'ascenso', 'escandalo', 'decadencia', 'fervor', 'fiesta', 'rumor', 'susurro'], { series: ['psi', 'asabiya'] }),
    ]],
    ['Economía', [
      T('hambrunas', '🥣', 'hambrunas', ['hambruna', 'fin_hambruna', 'plaga', 'cosecha', 'buen_anio', 'acaparamiento', 'refugiados', 'mana', 'incendio'], { cuenta: ['hambruna'], series: ['hambre', 'pgrano'], rank: 'hambre' }),
      T('pestes', '☣', 'pestes', ['peste', 'peste_muertos', 'peste_fin', 'cuarentena', 'epidemia'], { cuenta: ['peste', 'epidemia'], series: ['peste'] }),
      T('industria', '🏭', 'fundaciones', ['fundacion', 'astillero_parado', 'astillero_nomada', 'sin_repuesto', 'repuestos', 'reparada', 'rehace', 'feria', 'arca', 'arca_llega'], { cuenta: ['fundacion'], series: ['paro', 'comercio'] }),
      T('nucleos', '☢', 'núcleos reventados', ['explosion', 'grieta', 'vuela_con_grieta', 'nucleo_cambiado', 'inspeccion_falsa', 'reetiquetado', 'investigacion', 'culpa'], { cuenta: ['explosion'], series: ['nucleos'] }),
      T('desastres', '🌩', 'desastres', ['arsenal', 'llamarada', 'hallazgo', 'bomba', 'asteroide', 'tormenta']),
    ]],
    ['Naves y piratería', [
      T('abordajes', '🏴‍☠️', 'abordajes', ['pirateria', 'pirata_cazado', 'motin', 'banda_disuelta', 'patentes', 'convoy', 'correo_perdido', 'contrabando'], { cuenta: ['pirateria'], series: ['piratas'] }),
      T('perdidas', '🛸', 'naves perdidas', ['nave_destruida', 'desguace', 'venta_nave', 'amarrada'], { cuenta: ['nave_destruida'], series: ['naves', 'pecios'], rank: 'naves' }),
      T('robos', '💰', 'robos', ['robo', 'robo_frustrado', 'golpe_bodega', 'vende_herramientas', 'venta_caja'], { cuenta: ['robo'] }),
    ]],
    ['Justicia, venganza y fe', [
      T('venganzas', '📓', 'venganzas', ['emboscada', 'rencor', 'rencor_fin', 'libreta', 'cazarrecompensas', 'colecta_venganza'], { cuenta: ['emboscada'], series: ['vengadores'] }),
      T('justicia', '⚖', 'condenas', ['sospecha', 'busca', 'condena', 'fuga', 'fuga_fallida'], { cuenta: ['condena'] }),
      T('profecias', '🔮', 'profecías cumplidas', ['profecia_cumplida', 'profecia'], { cuenta: ['profecia_cumplida'] }),
      T('muertes', '⚰', 'muertes con nombre', ['muerte', 'entierro', 'cortejo'], { cuenta: ['muerte'] }),
      T('mano', '🖐', 'veces que metiste la mano', ['mano']),
    ]],
  ];
  // Las cifras grandes de arriba no cuentan hechos: son una serie, con los hechos que la mueven.
  P.INDICADORES = [
    T('pob', '👥', 'población', ['hambruna', 'peste_fin', 'epidemia', 'arsenal', 'llamarada', 'refugiados', 'exodo', 'masacre', 'fundacion'], { series: ['pob'], rank: 'pob', indicador: true }),
    T('estados', '🏛', 'Estados', ['estado_cae', 'independencia', 'fragmentacion', 'fin_guerra_civil', 'conquista', 'capital'], { series: ['estados', 'control', 'asabiya', 'psi'], indicador: true }),
    T('calle', '📣', 'gente en la calle', ['revolucion', 'insurreccion', 'insurreccion_aplastada', 'motin_pan', 'masacre', 'huelga'], { series: ['calle', 'agravio', 'R', 'eficacia'], rank: 'calle', indicador: true }),
    T('naves', '🛰', 'naves', ['botadura', 'nave_destruida', 'desguace', 'explosion', 'amarrada', 'motin', 'pirateria'], { series: ['naves', 'piratas', 'fragatas', 'pecios'], rank: 'naves', indicador: true }),
  ];
  const cuentaDe = (m, t) => { let n = 0; for (const k of (t.cuenta || t.ks)) n += m.cuenta[k] || 0; return n; };
  P.temaDe = function (m, id) {
    for (const [, l] of P.SECCIONES) for (const t of l) if (t.id === id) return t;
    for (const t of P.INDICADORES) if (t.id === id) return t;
    if (id.slice(0, 2) === 'k:') return T(id, '•', nomK(id.slice(2)), [id.slice(2)]);
    return null;
  };
  // Las clases de hecho que no están en ningún tema salen sueltas: nada se queda sin poder verse.
  P.sueltos = function (m) { const visto = { presente: 1, genesis: 1 }; for (const [, l] of P.SECCIONES) for (const t of l) for (const k of t.ks) visto[k] = 1; return Object.keys(m.cuenta).filter(k => !visto[k] && m.cuenta[k] > 0).sort((a, b) => m.cuenta[b] - m.cuenta[a]); };

  // ── La ficha del mundo: cifras grandes (cada una con su historia) y todo lo que ha pasado, por temas.
  P.resumen = function () {
    const m = P._m; const r = S.resumen(m);
    const ult = (k) => { const s = m.series[k]; if (s) for (let i = s.length - 1; i >= 0; i--) if (s[i] === s[i]) return s[i]; return 0; };
    let gue = 0; for (const e of m.est) if (e.vivo) gue += e.gue.size; gue /= 2; let bat = 0; for (const b of m.bat) if (b.vivo) bat++; for (const t of (m.tie || [])) if (t.vivo) bat++;
    const partes = m.est.filter(e => e.vivo).sort((x, y) => (y.pob || 0) - (x.pob || 0)).map(e => ({ nom: e.nom, v: e.pob || 0, col: e.col, id: e.id }));
    let libre = r.pob; for (const p of partes) libre -= p.v; if (libre > r.pob * 0.01) partes.push({ nom: 'Puertos francos', v: libre, col: '#59647a', id: -1 });
    let h = '<h3>El mundo ahora</h3>' + V.kpis([
      V.kpi(S.fmt(r.pob), 'Población', { spark: 'pob', col: C.azul, tema: 'pob' }),
      V.kpi(r.estados, 'Estados', { spark: 'estados', col: C.ambar, tema: 'estados' }),
      V.kpi(gue, 'Guerras en curso', { spark: 'guerras', col: C.rojo, tema: 'guerras', sub: bat ? bat + (bat === 1 ? ' combate ahora' : ' combates ahora') : '' }),
      V.kpi(pc(ult('hambre')), 'Hambre media', { spark: 'hambre', col: C.rojo, tema: 'hambrunas' }),
      V.kpi(pc(ult('calle')), 'En la calle (máximo)', { spark: 'calle', col: C.rosa, tema: 'calle' }),
      V.kpi(r.naves, 'Naves', { spark: 'naves', col: C.verde, tema: 'naves', sub: r.piratas + ' piratas' }),
    ]);
    h += '<h3>Quién tiene a la gente</h3><div class="dona"><canvas data-dona="' + V.dato('pobMundo', { partes, centro: S.fmt(r.pob), sub: 'habitantes' }) + '" data-w="112" data-h="112" style="width:112px;height:112px"></canvas><div class="leyenda col">' + partes.slice(0, 8).map(p => '<span><span class="chip" style="background:' + p.col + '"></span>' + (p.id >= 0 ? L('E', p.id) : esc(p.nom)) + ' <b>' + pc(p.v / Math.max(1, r.pob)) + '</b></span>').join('') + '</div></div>';
    h += '<p class="nota">Todo lo que ha pasado, por temas. Pulsa cualquier cifra y verás su historia entera, hecho a hecho.</p>';
    for (const [nom, l] of P.SECCIONES) h += '<h3>' + nom + '</h3><div class="ctas">' + l.map(t => V.cuenta(t.ico, n0(cuentaDe(m, t)), t.nom, t.id)).join('') + '</div>';
    const su = P.sueltos(m); if (su.length) h += '<h3>Y lo demás</h3><div class="ctas">' + su.map(k => V.cuenta('•', n0(m.cuenta[k]), nomK(k), 'k:' + k)).join('') + '</div>';
    return h + '<p class="nota">' + n0(r.personas) + ' personas con ficha · ' + n0(r.hechos) + ' hechos registrados · ' + n0(r.eventos) + ' eventos simulados.</p>';
  };

  // Lista de hechos con una raya por año: se lee como una línea de tiempo.
  function porAnios(m, l) { let h = '', a0 = null; for (const e of l) { const a = Math.floor(e.t / S.ANIO); if (a !== a0) { h += '<div class="anio">Año ' + a + '</div>'; a0 = a; } h += evHtml(e); } return h; }
  const RANK = {
    calle: (m) => ['Dónde hay más gente en la calle ahora', m.ase.slice().sort((a, b) => b.f - a.f).slice(0, 8).filter(a => a.f > 0.005).map(a => [L('A', a.id), pc(a.f), a.f / 0.7, C.rosa])],
    hambre: (m) => ['Dónde se pasa más hambre ahora', m.ase.slice().sort((a, b) => b.H - a.H).slice(0, 8).filter(a => a.H > 0.005).map(a => [L('A', a.id), pc(a.H), a.H, C.rojo])],
    pob: (m) => { const l = m.ase.slice().sort((a, b) => b.pob - a.pob).slice(0, 8); const mx = l.length ? l[0].pob : 1; return ['Dónde vive más gente', l.map(a => [L('A', a.id), S.fmt(a.pob), a.pob / mx, C.azul])]; },
    naves: (m) => { const por = {}; for (const n of m.nav) if (n.vivo) por[n.cls] = (por[n.cls] || 0) + Math.max(1, n.cascos || 1); const l = Object.keys(por).sort((a, b) => por[b] - por[a]).slice(0, 10); const mx = l.length ? por[l[0]] : 1; return ['Qué naves hay ahora (cascos)', l.map(k => [esc(S.reg.nave[k].nom), n0(por[k]), por[k] / mx, C.verde])]; },
    guerras: (m) => { const l = []; for (const e of m.est) if (e.vivo) for (const [j, w] of e.gue) if (j > e.id && m.est[j].vivo) l.push([estadoDe(e.id) + ' contra ' + estadoDe(j), (w.civil ? 'civil · ' : '') + 'desde hace ' + dias(m.t - (w.t0 === undefined ? m.t : w.t0)), Math.min(1, (w.cans || 0) / 3), C.rojo]); return ['Guerras en curso (la barra es el cansancio)', l]; },
  };

  // ── Un tema abierto: sus series, cuándo pasó, la clasificación de ahora y cada hecho, del último al primero.
  P.tema = function (m, st) {
    const t = P.temaDe(m, st.id); if (!t) return P.resumen();
    const todas = new Set(t.ks); const fuera = st.fuera || {}; const porK = {}; const l = []; let total = 0;
    for (const e of m.ev) { if (!todas.has(e.k)) continue; porK[e.k] = (porK[e.k] || 0) + 1; total++; if (!fuera[e.k] && e.imp >= (st.imp || 0)) l.push(e); }
    const cuenta = cuentaDe(m, t); const ultimo = l.length ? l[l.length - 1] : null;
    let h = '<div class="filtro"><button data-accion="temaFuera">← El mundo</button></div><h2>' + t.ico + ' ' + esc(S.cap(t.nom)) + '</h2><div class="sub">' + (t.indicador ? '' : '<b>' + n0(cuenta) + '</b> en toda la historia · ') + n0(total) + ' hechos relacionados' + (ultimo ? ' · el último, hace ' + dias(m.t - ultimo.t) : '') + '</div>';
    // Las series que lo acompañan.
    (t.series || []).forEach((k, j) => { const d = S.sismo.find(x => x.clave === k); if (!d) return; const s = m.series[k]; let v = ''; if (s) for (let i = s.length - 1; i >= 0; i--) if (s[i] === s[i]) { v = V.num(s[i]); break; } h += V.titulo(d.nom, v) + '<canvas class="graf" data-series="' + k + '" data-col="' + Pr.col(j) + '"></canvas>'; });
    // Cuándo: hechos por año.
    if (l.length) {
      const a0 = Math.floor(m.ev[0].t / S.ANIO), a1 = Math.floor(m.t / S.ANIO); const n = a1 - a0 + 1;
      if (n >= 2) { const d = new Array(n).fill(0), tt = []; for (let i = 0; i < n; i++) tt.push((a0 + i) * S.ANIO); for (const e of l) d[Math.floor(e.t / S.ANIO) - a0]++; h += V.titulo('Hechos por año', '') + '<canvas class="graf" data-viz="' + V.dato('temaHist', { tipo: 'barras', t: tt, min: 0, sinPeriodo: true, fecha: (x) => 'año ' + Math.floor(x / S.ANIO), fmt: (x) => n0(x) + (x === 1 ? ' hecho' : ' hechos'), series: [{ nom: '', d, col: Pr.acento() }] }) + '" style="height:84px"></canvas>'; }
    }
    if (t.rank && RANK[t.rank]) { const [tit, filas] = RANK[t.rank](m); if (filas.length) h += '<h3>' + tit + '</h3>' + filas.map(f => '<div class="flujo">' + f[0] + ' <b style="float:right">' + f[1] + '</b>' + V.medidor(f[2], f[3]) + '</div>').join(''); }
    h += '<h3>Uno a uno</h3><div class="filtro">' + selImp('ti', st.imp || 0) + '</div>';
    if (t.ks.length > 1) h += '<div class="chips">' + t.ks.filter(k => porK[k]).map(k => '<button class="chipb' + (fuera[k] ? '' : ' on') + '" data-tk="' + k + '">' + nomK(k) + ' ' + n0(porK[k]) + '</button>').join('') + '</div>';
    const lim = st.lim || 150; const ver = l.slice(-lim).reverse();
    h += (ver.length ? porAnios(m, ver) : '<p class="nota">Nada todavía' + (total ? ' con ese filtro.' : '.') + '</p>');
    if (l.length > lim) h += '<div class="filtro"><button data-accion="temaMas">Ver ' + Math.min(150, l.length - lim) + ' más antiguos (quedan ' + n0(l.length - lim) + ')</button></div>';
    return h;
  };

  // ── Historial completo de cualquier cosa, con el filtro de importancia de la crónica.
  P.hechosDe = function (m, ref) {
    if (ref.t === 'P' && m.bio && m.bio.get(ref.id)) return m.bio.get(ref.id).map(i => m.ev[i]);
    const tok = '{' + ref.t + ref.id + '}'; const out = []; const id = ref.id;
    for (const e of m.ev) { const d = e.d; if (e.txt.indexOf(tok) >= 0 || (ref.t === 'A' && e.a === id) || (ref.t === 'S' && e.s === id) || (d && ((ref.t === 'F' && d.f === id) || (ref.t === 'E' && (d.e === id || d.gana === id || d.pierde === id)) || (ref.t === 'N' && d.n === id)))) out.push(e); }
    return out;
  };
  P.historial = function (ref) {
    const m = P._m; const st = (P._op && P._op.bio) || { imp: 0, orden: 'desc' }; const todos = P.hechosDe(m, ref);
    let l = todos.filter(e => e.imp >= (st.imp || 0)); const asc = st.orden === 'asc'; const cab = l.length > 250; l = asc ? l.slice(0, 250) : l.slice(-250).reverse();
    return '<div class="filtro">' + selImp('hi', st.imp || 0) + '<select data-op="ho"><option value="desc"' + (asc ? '' : ' selected') + '>lo último primero</option><option value="asc"' + (asc ? ' selected' : '') + '>desde el principio</option></select></div><div class="nota">' + n0(todos.filter(e => e.imp >= (st.imp || 0)).length) + ' de ' + n0(todos.length) + ' hechos' + (cab ? ' (se enseñan 250)' : '') + '. Pulsa uno para ver de dónde viene y qué provocó.</div>' + (l.length ? porAnios(m, l) : '<p class="nota">Nada con ese filtro.</p>');
  };
  P.historialPleg = (ref) => P.plegable('h' + ref.t + ref.id, AB(), '📜 Su historial completo', () => '<div class="cuerpo">' + P.historial(ref) + '</div>');

  // ── Historiador: una línea de tiempo con ramas. Arriba lo más antiguo; el hecho elegido, resaltado; debajo, lo que provocó.
  P.historiador = function (mm, id, op) {
    P.usar(mm); const m = mm; op = op || {};
    if (id === undefined || id === null || !m.ev[id]) return '<h2>Historiador</h2><p class="nota">Elige un hecho en la crónica (o en cualquier ficha) y verás su historia como una línea de tiempo con ramas: de dónde viene y qué provocó. Cada hecho guarda sus causas; nadie ha escrito la cadena.</p>';
    const e = m.ev[id]; const prof = op.prof || 4; const MAXC = 40, MAXE = 60;
    const nodo = new Map([[id, 0]]);
    let nivel = [id], nc = 0; for (let d = 1; d <= prof; d++) { const sig = []; for (const x of nivel) for (const c of m.ev[x].c) if (!nodo.has(c) && nc < MAXC) { nodo.set(c, -d); sig.push(c); nc++; } nivel = sig; }
    nivel = [id]; let ne = 0, ocultos = 0; for (let d = 1; d <= prof; d++) { const cand = []; const visto = new Set(); for (const x of nivel) { const ef = m.efe[x]; if (ef) for (const y of ef) if (!nodo.has(y) && !visto.has(y)) { visto.add(y); cand.push(y); } } cand.sort((a, b) => m.ev[b].imp - m.ev[a].imp || a - b); const sig = []; for (const y of cand) { if (ne < MAXE) { nodo.set(y, d); sig.push(y); ne++; } else ocultos++; } nivel = sig; }
    // Las causas se registran siempre antes que sus efectos: ordenar por número es ordenar en el tiempo.
    const lista = Array.from(nodo.keys()).sort((a, b) => a - b); const fila = new Map(lista.map((x, i) => [x, i]));
    const aristas = []; const fin = new Map(lista.map(x => [x, fila.get(x)]));
    for (const v of lista) for (const c of m.ev[v].c) if (fila.has(c)) { aristas.push([c, v]); if (fila.get(v) > fin.get(c)) fin.set(c, fila.get(v)); }
    // Carriles, como en un grafo de ramas: cada hecho ocupa el suyo hasta su último efecto; ese último lo hereda.
    const carril = new Map(); const libreDesde = []; const cedido = new Set();
    for (const v of lista) {
      const r = fila.get(v); let c = -1;
      for (const p of m.ev[v].c) if (fila.has(p) && fin.get(p) === r && !cedido.has(p)) { c = carril.get(p); cedido.add(p); break; }
      if (c < 0) { c = 0; while (libreDesde[c] !== undefined && libreDesde[c] >= r) c++; }
      carril.set(v, c); libreDesde[c] = fin.get(v);
    }
    let nCar = 0; for (const c of carril.values()) if (c + 1 > nCar) nCar = c + 1; const ANCHO = 18 + (nCar - 1) * 13;
    V.dato('grafo', { ancho: ANCHO, carril, aristas, sel: id });
    let h = '<h2>Historiador</h2><div class="dios"><button data-accion="centrarEv">📍 Ir al lugar</button><button data-accion="noticia">📡 ¿Quién se ha enterado?</button></div>';
    h += '<div class="filtro">Hasta dónde seguir el hilo <select data-op="gp">' + [[2, 'cerca (2 pasos)'], [4, 'medio (4 pasos)'], [12, 'todo lo que haya']].map(x => '<option value="' + x[0] + '"' + (x[0] === prof ? ' selected' : '') + '>' + x[1] + '</option>').join('') + '</select></div>';
    h += '<p class="nota">Se lee de arriba abajo, como una línea de tiempo: arriba, lo más antiguo. Cada punto es un hecho y las líneas unen cada causa con lo que provocó; cuando una línea se abre en ramas, es que un hecho tuvo varias consecuencias. Pulsa cualquier hecho para ponerlo en el centro.</p>';
    const pad = ' style="padding-left:' + (ANCHO + 8) + 'px"'; const sinCausa = !nc;
    h += '<div class="grafo"><canvas></canvas>';
    if (nc) h += '<div class="gr-sep"' + pad + '>▲ De dónde viene (' + nc + ')</div>';
    let ant = null;
    for (const x of lista) {
      const e2 = m.ev[x]; const d = nodo.get(x);
      if (ant && e2.t - ant.t >= 180 && x !== id) h += '<div class="gr-salto"' + pad + '>' + dias(e2.t - ant.t) + ' después</div>';
      const ins = x === id ? V.insignia('este hecho', 'aviso') : d === -1 ? V.insignia('causa directa') : d === 1 ? V.insignia('consecuencia directa') : (!e2.c.length && d < 0 ? V.insignia(e2.k === 'mano' ? 'tu mano' : 'aquí empieza el hilo', e2.k === 'mano' ? 'mal' : 'ok') : '');
      h += '<div class="gr-fila i' + e2.imp + (x === id ? ' sel' : '') + '" data-ev="' + x + '"' + pad + '><span class="f">' + m.fecha(e2.t) + ' · ' + nomK(e2.k) + ' ' + ins + '</span>' + txtEv(e2) + '</div>';
      if (x === id) { if (sinCausa) h += '<div class="gr-salto"' + pad + '>No tiene causa dentro del mundo' + (e.k === 'mano' ? ': fuiste tú.' : ' que haya quedado registrada.') + '</div>'; h += '<div class="gr-sep"' + pad + '>▼ Lo que provocó (' + ne + (ocultos ? ', y ' + ocultos + ' más que no caben' : '') + ')</div>'; if (!ne) h += '<div class="gr-salto"' + pad + '>Nada, de momento.</div>'; }
      ant = e2;
    }
    return h + '</div>';
  };
  // Las ramas se pintan después de colocar el texto: cada fila dice a qué altura queda su punto.
  P.pintarExtra = function (m, raiz) {
    for (const cont of raiz.querySelectorAll('.grafo')) {
      const o = V.datos.grafo; const cv = cont.querySelector ? cont.querySelector('canvas') : null; if (!o || !cv) continue;
      const y = new Map(); const base = cont.getBoundingClientRect().top; for (const f of cont.querySelectorAll('.gr-fila')) y.set(+f.dataset.ev, f.getBoundingClientRect().top - base + 11);
      const H = Math.max(20, cont.scrollHeight || cont.clientHeight || 20); cv.style.width = o.ancho + 'px'; cv.style.height = H + 'px'; cv.dataset.w = o.ancho; cv.dataset.h = H;
      const [c] = V.lienzo(cv); const X = (id) => 9 + o.carril.get(id) * 13; const t = Pr.tema();
      c.lineWidth = 1.7; c.lineCap = 'round';
      for (const [p, v] of o.aristas) {
        const x0 = X(p), y0 = y.get(p), x1 = X(v), y1 = y.get(v); if (y0 === undefined || y1 === undefined) continue;
        c.strokeStyle = V.alfa(Pr.col(o.carril.get(x0 === x1 ? p : v)), 0.9); c.beginPath(); c.moveTo(x0, y0);
        if (x0 === x1) c.lineTo(x1, y1); else { const k = Math.min(18, (y1 - y0) * 0.7); c.lineTo(x0, y1 - k); c.bezierCurveTo(x0, y1 - k * 0.3, x1, y1 - k * 0.7, x1, y1); }
        c.stroke();
      }
      for (const [id, car] of o.carril) {
        const yy = y.get(id); if (yy === undefined) continue; const e = m.ev[id]; const x = 9 + car * 13; const r = id === o.sel ? 6 : 3 + Math.min(2, e.imp) * 0.8; const col = e.k === 'mano' ? '#c05cf0' : Pr.col(car);
        c.fillStyle = t.panel; c.beginPath(); c.arc(x, yy, r + 2, 0, Math.PI * 2); c.fill();
        if (id === o.sel) { c.fillStyle = '#fff'; c.beginPath(); c.arc(x, yy, r + 1, 0, Math.PI * 2); c.fill(); }
        c.fillStyle = col; c.beginPath(); c.arc(x, yy, r, 0, Math.PI * 2); c.fill();
      }
    }
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
