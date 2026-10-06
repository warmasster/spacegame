// Aspecto: lo que cada cual puede cambiar a su gusto (tema, acento, paleta y estilo de las gráficas, tamaño de
// letra, ancho del panel, qué series enseña el sismógrafo). Se guarda en el navegador y se aplica con variables CSS.
(function (g) {
  'use strict';
  const U = g.UI = g.UI || {};
  const Pr = U.prefs = {};
  Pr.TEMAS = {
    noche: { nom: 'Noche azul', fondo: '#070a12', panel: '#0e1320', panel2: '#171f31', tarjeta: '#0b101b', hueco: '#1c2538', linea: '#27324e', txt: '#dbe2ee', tenue: '#8793ab', enlace: '#9fc4ff' },
    carbon: { nom: 'Carbón', fondo: '#09090b', panel: '#131316', panel2: '#1e1e23', tarjeta: '#0e0e11', hueco: '#26262c', linea: '#34343c', txt: '#e6e6ea', tenue: '#92929d', enlace: '#b6c8ff' },
    nebulosa: { nom: 'Nebulosa', fondo: '#0b0714', panel: '#150e24', panel2: '#211636', tarjeta: '#110b1e', hueco: '#2a1d45', linea: '#3d2c61', txt: '#e6dcf6', tenue: '#9d8dbb', enlace: '#c9b3ff' },
    abismo: { nom: 'Abismo', fondo: '#030d0d', panel: '#081817', panel2: '#0f2625', tarjeta: '#061312', hueco: '#143231', linea: '#1f4543', txt: '#d6eee9', tenue: '#7fa7a0', enlace: '#8fe3d6' },
    oxido: { nom: 'Óxido', fondo: '#0f0907', panel: '#1b110d', panel2: '#2a1a13', tarjeta: '#150d0a', hueco: '#35211a', linea: '#4c2f24', txt: '#f1e2d6', tenue: '#b0937f', enlace: '#ffc49a' },
  };
  Pr.ACENTOS = { oro: '#e0b84c', coral: '#ff7a59', cian: '#4fd1e0', verde: '#6fdc8c', rosa: '#ff6fb5', violeta: '#a98bff' };
  // Paletas para las series de las gráficas. La tercera es la de Okabe e Ito, legible con daltonismo.
  Pr.PALETAS = {
    viva: { nom: 'Viva', c: ['#ff5d5d', '#ffc857', '#4fd1e0', '#ff6fb5', '#6fdc8c', '#a98bff', '#ff9f43', '#7aa2ff'] },
    pastel: { nom: 'Pastel', c: ['#f4a6a0', '#f3d9a4', '#a5d8e6', '#e8b4d8', '#b5e2c0', '#c9bdf2', '#f6c9a0', '#b3c7f7'] },
    segura: { nom: 'Daltónica', c: ['#e69f00', '#56b4e9', '#009e73', '#f0e442', '#0072b2', '#d55e00', '#cc79a7', '#bbbbbb'] },
  };
  Pr.ANCHOS = { 380: 'Estrecho', 440: 'Normal', 560: 'Ancho', 720: 'Muy ancho' };
  Pr.PERIODOS = { 0: 'Todo', 10: '10 años', 3: '3 años', 1: '1 año' };
  Pr.SERIES0 = ['pob', 'hambre', 'pgrano', 'agravio', 'calle', 'guerras', 'fragatas', 'tec'];
  const DEF = { tema: 'noche', acento: 'oro', paleta: 'viva', graf: 'area', suave: true, rejilla: true, brillo: true, periodo: 0, alto: 120, letra: 13, ancho: 440, series: null };
  const CLAVE = 'universo-que-no-te-necesita.aspecto';
  Pr.v = Object.assign({}, DEF);
  Pr.cargar = function () {
    try { const s = g.localStorage && g.localStorage.getItem(CLAVE); if (s) { const o = JSON.parse(s); for (const k in DEF) if (o[k] !== undefined && typeof o[k] === typeof DEF[k] || (k === 'series' && Array.isArray(o[k]))) Pr.v[k] = o[k]; } } catch (e) { /* sin almacenamiento: se queda lo de fábrica */ }
    if (!Pr.TEMAS[Pr.v.tema]) Pr.v.tema = DEF.tema; if (!Pr.ACENTOS[Pr.v.acento]) Pr.v.acento = DEF.acento; if (!Pr.PALETAS[Pr.v.paleta]) Pr.v.paleta = DEF.paleta;
  };
  Pr.guardar = function () { try { if (g.localStorage) g.localStorage.setItem(CLAVE, JSON.stringify(Pr.v)); } catch (e) { /* da igual */ } };
  Pr.tema = () => Pr.TEMAS[Pr.v.tema];
  Pr.acento = () => Pr.ACENTOS[Pr.v.acento];
  Pr.col = (i) => { const c = Pr.PALETAS[Pr.v.paleta].c; return c[((i % c.length) + c.length) % c.length]; };
  Pr.series = () => Pr.v.series || Pr.SERIES0;
  Pr.aplicar = function () {
    const d = g.document; if (!d || !d.documentElement || !d.documentElement.style || !d.documentElement.style.setProperty) return;
    const st = d.documentElement.style; const t = Pr.tema();
    for (const k of ['fondo', 'panel2', 'tarjeta', 'hueco', 'linea', 'txt', 'tenue', 'enlace']) st.setProperty('--' + k, t[k]);
    st.setProperty('--panel', t.panel + 'f2'); st.setProperty('--panel-s', t.panel);
    st.setProperty('--acento', Pr.acento()); st.setProperty('--letra', Pr.v.letra + 'px'); st.setProperty('--ancho', Pr.v.ancho + 'px'); st.setProperty('--alto-graf', Pr.v.alto + 'px');
    if (U.mapa && U.mapa.redimensionar && U.mapa.cv) U.mapa.redimensionar();
  };
  Pr.poner = function (k, v) { Pr.v[k] = v; Pr.guardar(); Pr.aplicar(); };
  Pr.restablecer = function () { Pr.v = Object.assign({}, DEF); Pr.guardar(); Pr.aplicar(); };
  Pr.alternarSerie = function (k) { const l = Pr.series().slice(); const i = l.indexOf(k); if (i >= 0) l.splice(i, 1); else l.push(k); Pr.poner('series', l); };

  // El panel de aspecto: se monta con lo que hay en los registros de arriba.
  Pr.html = function () {
    const v = Pr.v; const sw = (k, id, col, tit, extra) => '<button class="mu' + (v[k] === id ? ' on' : '') + '" data-pref="' + k + '" data-v="' + id + '" title="' + tit + '" style="' + (extra || 'background:' + col) + '"></button>';
    const op = (k, id, txt) => '<button class="op' + (String(v[k]) === String(id) ? ' on' : '') + '" data-pref="' + k + '" data-v="' + id + '">' + txt + '</button>';
    const ch = (k, txt) => '<label class="ch"><input type="checkbox" data-pref="' + k + '"' + (v[k] ? ' checked' : '') + '> ' + txt + '</label>';
    let h = '<div class="tit">🎨 Aspecto</div>';
    h += '<div class="gr"><span>Tema</span><div>' + Object.keys(Pr.TEMAS).map(id => { const t = Pr.TEMAS[id]; return sw('tema', id, '', t.nom, 'background:linear-gradient(135deg,' + t.panel + ' 50%,' + t.linea + ' 50%)'); }).join('') + '</div></div>';
    h += '<div class="gr"><span>Acento</span><div>' + Object.keys(Pr.ACENTOS).map(id => sw('acento', id, Pr.ACENTOS[id], id)).join('') + '</div></div>';
    h += '<div class="gr"><span>Colores de las gráficas</span><div>' + Object.keys(Pr.PALETAS).map(id => { const p = Pr.PALETAS[id]; return '<button class="pal' + (v.paleta === id ? ' on' : '') + '" data-pref="paleta" data-v="' + id + '" title="' + p.nom + '">' + p.c.slice(0, 5).map(c => '<i style="background:' + c + '"></i>').join('') + '</button>'; }).join('') + '</div></div>';
    h += '<div class="gr"><span>Gráficas</span><div>' + op('graf', 'area', 'Área') + op('graf', 'linea', 'Línea') + op('graf', 'barras', 'Barras') + '</div></div>';
    h += '<div class="gr"><span></span><div>' + ch('suave', 'curvas suaves') + ch('rejilla', 'rejilla') + ch('brillo', 'brillo') + '</div></div>';
    h += '<div class="gr"><span>Periodo</span><div>' + [0, 10, 3, 1].map(k => op('periodo', k, Pr.PERIODOS[k])).join('') + '</div></div>';
    h += '<div class="gr"><span>Alto de las gráficas</span><div>' + op('alto', 84, 'Bajo') + op('alto', 120, 'Medio') + op('alto', 180, 'Alto') + '</div></div>';
    h += '<div class="gr"><span>Letra</span><div>' + [12, 13, 14, 16].map(k => op('letra', k, k === 12 ? 'Pequeña' : k === 13 ? 'Normal' : k === 14 ? 'Grande' : 'Enorme')).join('') + '</div></div>';
    h += '<div class="gr"><span>Ancho del panel</span><div>' + Object.keys(Pr.ANCHOS).map(k => op('ancho', k, Pr.ANCHOS[k])).join('') + '</div></div>';
    return h + '<div class="pie"><button data-pref="restablecer">Volver a lo de fábrica</button></div>';
  };
  // Un clic o un cambio dentro del panel de aspecto. Devuelve true si tocó algo.
  Pr.accion = function (el) {
    const k = el && el.dataset ? el.dataset.pref : undefined; if (!k) return false;
    if (k === 'restablecer') { Pr.restablecer(); return true; }
    if (el.type === 'checkbox') { Pr.poner(k, !!el.checked); return true; }
    const raw = el.dataset.v; Pr.poner(k, typeof DEF[k] === 'number' ? +raw : raw); return true;
  };
  Pr.cargar();
})(typeof globalThis !== 'undefined' ? globalThis : this);
