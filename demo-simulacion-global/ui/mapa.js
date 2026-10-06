// Mapa 2D. Solo pregunta al núcleo «qué hay aquí» y lo pinta con cuatro niveles de detalle:
// galaxia (estados y rutas) → sistema (órbitas y asentamientos) → asentamiento (plano, edificios, gente) → caras.
// Encima van las capas: política, hambre, rebeliones, interrelaciones, comercio, estrategia, técnica…
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI = g.UI || {}; const A = U.arte; const Lc = U.local;
  const M = U.mapa = { cam: { x: 0, y: 0, z: 0.5 }, capa: 'politico', pings: [], zonas: [], W: 0, H: 0, ox: 0, oy: 0, panel: 400, caras: null, noticia: null, seguir: null };
  const Z_SIS = 1.5, Z_NAV = 2.8, Z_INS = 6.5, Z_BAT = 4.2, Z_LOCAL = 11;
  const TAU = Math.PI * 2;
  let cv, cx, estrellas = [];
  const tmp = {}, tmp2 = {};
  const azar = (a) => { const x = Math.sin(a * 127.1 + 311.7) * 43758.5453; return x - Math.floor(x); };

  M.iniciar = function (canvas) {
    cv = canvas; M.cv = canvas; cx = cv.getContext('2d');
    const r = new S.Rng(7); for (let i = 0; i < 420; i++) estrellas.push([r.f(), r.f(), r.f()]);
    M.redimensionar();
  };
  M.redimensionar = function () {
    const dpr = g.devicePixelRatio || 1; M.dpr = dpr;
    M.W = cv.clientWidth || 1280; M.H = cv.clientHeight || 720;
    cv.width = Math.round(M.W * dpr); cv.height = Math.round(M.H * dpr);
    M.panel = M.W > 900 ? (g.UI.prefs ? g.UI.prefs.v.ancho : 420) : 0;
    M.ox = (M.W - M.panel + 146) / 2; M.oy = (M.H + 14) / 2;      // centro de lo que queda entre la mano y el panel
  };
  M.ajustar = function (m) {
    M.cam.x = m.ancho / 2; M.cam.y = m.alto / 2;
    M.cam.z = Math.min((M.W - M.panel - 190) / m.ancho, (M.H - 110) / m.alto);
  };
  M.sx = (x) => (x - M.cam.x) * M.cam.z + M.ox;
  M.sy = (y) => (y - M.cam.y) * M.cam.z + M.oy;
  M.aMundo = (px, py) => ({ x: (px - M.ox) / M.cam.z + M.cam.x, y: (py - M.oy) / M.cam.z + M.cam.y });
  M.zoom = function (px, py, f) {
    const w = M.aMundo(px, py); M.cam.z = S.clamp(M.cam.z * f, 0.12, 70);
    M.cam.x = w.x - (px - M.ox) / M.cam.z; M.cam.y = w.y - (py - M.oy) / M.cam.z;
  };

  function colEstado(m, id) { return id >= 0 && m.est[id] ? m.est[id].col : '#59647a'; }
  function rgba(hex, a) { const n = parseInt(hex.slice(1), 16); return 'rgba(' + (n >> 16 & 255) + ',' + (n >> 8 & 255) + ',' + (n & 255) + ',' + a + ')'; }
  function radioAsent(a, z) { const b = a.forma === 'planeta' ? 4.5 : a.forma === 'luna' ? 3.2 : 3; return S.clamp((b + Math.log10(Math.max(1000, a.pob)) * 0.55 - 1.6) * Math.pow(z / 2, 0.55), 2.5, 34); }
  const calor = (v) => 'rgba(' + Math.round(70 + 185 * v) + ',' + Math.round(190 - 150 * v) + ',' + Math.round(110 - 60 * v) + ',';
  const CLASE = { centro: '#e0b84c', semiperiferia: '#5fb0c9', periferia: '#7a4a3a' };

  // Valor 0..1 de la capa elegida para un asentamiento (capas «de calor»).
  M.valorCapa = function (m, a) {
    switch (M.capa) {
      case 'hambre': return S.clamp(a.H * 1.6, 0, 1);
      case 'agravio': return S.clamp(a.agr, 0, 1);
      case 'calle': case 'rebeliones': return S.clamp(a.f / 0.63, 0, 1);
      case 'control': return a.est >= 0 ? 1 - (a.cap ? 1 : a.control) : 0.5;
      case 'grano': return S.clamp((a.pr[0] / 100 - 0.5) / 4, 0, 1);
      case 'tecnica': return a.est >= 0 ? S.clamp((m.est[a.est].tec - 0.8) / 1.2, 0, 1) : 0;
      case 'paro': return S.clamp((a.paro || 0) * 1.5, 0, 1);
      case 'fama': return M.famaDe >= 0 ? S.his.famaEn(a, M.famaDe) : 0;                      // dónde se conoce a una persona
      case 'credo': return S.clamp(S.his.creTotal(a) / 0.6, 0, 1);                            // cuánta gente sigue algún credo
      case 'peste': return S.clamp((a.pI || 0) * 6 + (a.pR || 0) * 0.3, 0, 1);                // enfermos (y por dónde ha pasado)
      case 'sangre': return S.clamp(Math.log10(1 + (a.sangre || 0)) / 4.5, 0, 1);             // muertes acumuladas (logarítmico)
      case 'memoria': return S.clamp(Math.log10(1 + (a.memoria || 0)) / 2.8, 0, 1);           // cuánta historia ha pasado allí
    }
    return 0;
  };
  const DE_CALOR = { hambre: 1, agravio: 1, calle: 1, control: 1, grano: 1, tecnica: 1, paro: 1, fama: 1, credo: 1, peste: 1, sangre: 1, memoria: 1 };
  M.famaDe = -1;
  M.posDe = function (m, ref, t) {
    if (!ref) return null;
    switch (ref.t) {
      case 'S': return { x: m.sis[ref.id].x, y: m.sis[ref.id].y };
      case 'A': case 'U': return S.nav.posAsent(m, m.ase[ref.t === 'U' ? m.uni[ref.id].ase : ref.id], t, {});
      case 'C': return S.nav.posAsent(m, m.ase[ref.a], t, {});
      case 'I': return S.nav.posAsent(m, m.ase[m.ins[ref.id].ase], t, {});
      case 'N': { const n = m.nav[ref.id]; return S.nav.pos(m, n, t, {}); }
      case 'P': {
        const p = m.per[ref.id];
        if (p.en.t === 'N' && m.nav[p.en.id]) return S.nav.pos(m, m.nav[p.en.id], t, {});
        if (p.en.t === 'V') { const a0 = S.nav.posAsent(m, m.ase[p.en.de], t, {}), a1 = S.nav.posAsent(m, m.ase[p.en.a], t, {}); const u = S.clamp((t - p.en.t0) / (p.en.t1 - p.en.t0), 0, 1); return { x: a0.x + (a1.x - a0.x) * u, y: a0.y + (a1.y - a0.y) * u }; }
        const a = m.ase[S.per.lugar(m, p)]; return a ? S.nav.posAsent(m, a, t, {}) : null;
      }
      case 'E': return S.nav.posAsent(m, m.ase[m.est[ref.id].cap], t, {});
      case 'F': return m.fac[ref.id].sede >= 0 ? S.nav.posAsent(m, m.ase[m.fac[ref.id].sede], t, {}) : null;
      case 'D': { const d = m.dis[ref.id]; return d.est >= 0 ? S.nav.posAsent(m, m.ase[m.est[d.est].cap], t, {}) : null; }
      case 'O': { const o = m.obj[ref.id]; const d = o.donde; if (!d) return null; return M.posDe(m, { t: d.t, id: d.id }, t); }
      case 'V': { const e = m.ev[ref.id]; if (e.a >= 0) return S.nav.posAsent(m, m.ase[e.a], t, {}); if (e.s >= 0) return { x: m.sis[e.s].x, y: m.sis[e.s].y }; return null; }
    }
    return null;
  };
  M.centrar = function (m, ref, zmin) {
    const p = M.posDe(m, ref, m.t); if (!p) return;
    M.cam.x = p.x; M.cam.y = p.y;
    const quiere = zmin || ({ S: 2.2, A: 12.5, C: 14, I: 13, N: 3, P: 8, E: 1.2, F: 6, V: 2.5, O: 4, U: 13 })[ref.t] || 2;
    if (M.cam.z < quiere) M.cam.z = quiere;
  };
  M.ping = function (m, e) {
    const p = M.posDe(m, { t: 'V', id: e.id }, m.t); if (!p) return;
    const col = ({ guerra: '#ff5a4f', batalla: '#ff5a4f', batalla_fin: '#ff5a4f', conquista: '#ff9a4f', hambruna: '#e0b84c', revolucion: '#ff4fd0', insurreccion: '#ff4fd0', masacre: '#c0392b', explosion: '#ffd24f', bomba: '#c05cf0', asteroide: '#c05cf0', faccion: '#5cc28a', muerte: '#ffffff', alianza: '#5cf08a' })[e.k] || '#8fb6ff';
    M.pings.push({ x: p.x, y: p.y, t0: performance.now(), col, g: e.imp });
    if (M.pings.length > 60) M.pings.shift();
  };
  function linea(x0, y0, x1, y1, col, w, dash) { cx.strokeStyle = col; cx.lineWidth = w; if (dash) cx.setLineDash(dash); cx.beginPath(); cx.moveTo(x0, y0); cx.lineTo(x1, y1); cx.stroke(); if (dash) cx.setLineDash([]); }
  const pAse = (m, a, t) => { const p = S.nav.posAsent(m, a, t, tmp); return [M.sx(p.x), M.sy(p.y)]; };
  const pSis = (m, s) => [M.sx(m.sis[s].x), M.sy(m.sis[s].y)];
  const pCap = (m, e, t) => M.cam.z >= Z_SIS ? pAse(m, m.ase[e.cap], t) : pSis(m, m.ase[e.cap].sis);

  M.dibujar = function (m, t, sel) {
    const z = M.cam.z; const W = M.W, H = M.H; const zon = M.zonas; zon.length = 0;
    cx.setTransform(M.dpr, 0, 0, M.dpr, 0, 0);
    cx.fillStyle = '#070a12'; cx.fillRect(0, 0, W, H);
    for (let i = 0; i < estrellas.length; i++) {
      const e = estrellas[i]; const k = 0.04 + e[2] * 0.08;
      let x = (e[0] * W - M.cam.x * k * z) % W, y = (e[1] * H - M.cam.y * k * z) % H; if (x < 0) x += W; if (y < 0) y += H;
      cx.fillStyle = 'rgba(200,215,255,' + (0.15 + e[2] * 0.5) + ')'; cx.fillRect(x, y, e[2] > 0.85 ? 1.6 : 1, e[2] > 0.85 ? 1.6 : 1);
    }
    if (!m) return;
    Lc.limpiar(m);
    const sx = M.sx, sy = M.sy; const capa = M.capa; const calorC = !!DE_CALOR[capa]; const tenue = capa === 'rebeliones' || capa === 'relaciones' || capa === 'comercio' || capa === 'estrategia';
    // Territorios.
    const alfa = S.clamp(0.9 / Math.sqrt(z + 0.3), 0.18, 1) * (tenue ? 0.35 : 1);
    for (const s of m.sis) {
      const x = sx(s.x), y = sy(s.y); const r = Math.max(34, 118 * z);
      if (x < -r || y < -r || x > W + r || y > H + r) continue;
      if (calorC) { let v = 0; for (const id of s.ase) v = Math.max(v, M.valorCapa(m, m.ase[id])); if (v > 0.03) { const gr = cx.createRadialGradient(x, y, 0, x, y, r); gr.addColorStop(0, calor(v) + (0.15 + 0.5 * v) * alfa + ')'); gr.addColorStop(1, calor(v) + '0)'); cx.fillStyle = gr; cx.beginPath(); cx.arc(x, y, r, 0, TAU); cx.fill(); } continue; }
      if (s.est < 0) continue; const col = m.est[s.est].col; const a0 = 0.30 * alfa;
      const gr = cx.createRadialGradient(x, y, 0, x, y, r); gr.addColorStop(0, rgba(col, a0)); gr.addColorStop(0.6, rgba(col, a0 * 0.55)); gr.addColorStop(1, rgba(col, 0));
      cx.fillStyle = gr; cx.beginPath(); cx.arc(x, y, r, 0, TAU); cx.fill();
    }
    // Rutas.
    cx.lineWidth = 1;
    for (const s of m.sis) for (const v of s.vec) {
      if (v.a < s.id) continue; const o = m.sis[v.a];
      const guerra = s.est >= 0 && o.est >= 0 && s.est !== o.est && (m.est[s.est].gue.has(o.est) || m.est[o.est].gue.has(s.est));
      const cerrada = s.bloqueo > t || o.bloqueo > t;
      linea(sx(s.x), sy(s.y), sx(o.x), sy(o.y), cerrada ? 'rgba(180,120,255,0.5)' : guerra ? 'rgba(255,90,79,0.45)' : 'rgba(130,150,190,0.20)', 1, cerrada ? [3, 5] : null);
    }
    if (capa === 'comercio') capaComercio(m, t, z);
    // Sistemas.
    cx.textAlign = 'center'; cx.textBaseline = 'middle';
    for (const s of m.sis) {
      const x = sx(s.x), y = sy(s.y); if (x < -200 || y < -200 || x > W + 200 || y > H + 200) continue;
      const rs = S.clamp(2.2 + z * 1.6, 2.5, 13);
      const gl = cx.createRadialGradient(x, y, 0, x, y, rs * 3.2); gl.addColorStop(0, s.col); gl.addColorStop(0.25, rgba(s.col, 0.55)); gl.addColorStop(1, 'rgba(255,220,160,0)');
      cx.fillStyle = gl; cx.beginPath(); cx.arc(x, y, rs * 3.2, 0, TAU); cx.fill();
      if (s.terr === 'asteroides' && z > 0.35) { cx.strokeStyle = 'rgba(190,170,140,0.35)'; cx.setLineDash([2, 4]); cx.beginPath(); cx.arc(x, y, Math.max(12, 30 * z), 0, TAU); cx.stroke(); cx.setLineDash([]); }
      if (s.bloqueo > t) { cx.strokeStyle = 'rgba(180,120,255,0.8)'; cx.lineWidth = 2; const an = performance.now() / 500; cx.beginPath(); cx.arc(x, y, Math.max(16, 34 * z), an, an + 4.5); cx.stroke(); cx.lineWidth = 1; }
      if (z < Z_SIS) {
        zon.push({ x, y, r: 13, ref: { t: 'S', id: s.id }, pri: 1 });
        let ico = ''; let capE = -1;
        for (const id of s.ase) { const a = m.ase[id]; if (a.cap) capE = a.est; if (a.H > 0.3 && ico.indexOf('🥣') < 0) ico += '🥣'; if (a.f > 0.4 && ico.indexOf('🔥') < 0) ico += '🔥'; if (a.plaga > 0.05 && ico.indexOf('🐛') < 0) ico += '🐛'; if (a.huelga > t && ico.indexOf('✊') < 0) ico += '✊'; if (a.tie >= 0 && ico.indexOf('⚔') < 0) ico += '⚔'; if (a.feria > t && ico.indexOf('🎪') < 0) ico += '🎪'; }
        if (s.pir.length) ico += '☠';
        if (capE >= 0) { cx.strokeStyle = '#e0b84c'; cx.lineWidth = 1.5; cx.beginPath(); cx.arc(x, y, rs + 5, 0, TAU); cx.stroke(); cx.lineWidth = 1; if (z > 0.28) A.banderaDe(cx, x + rs + 6, y - rs - 14, 17, 11, m, 'E', capE); }
        if (z > 0.3) {
          cx.font = '12px Segoe UI, sans-serif'; cx.fillStyle = 'rgba(215,222,234,0.9)'; cx.fillText(s.nom, x, y + rs + 13);
          if (ico) { cx.font = '12px Segoe UI Emoji, sans-serif'; cx.fillText(ico, x - (capE >= 0 ? 8 : 0), y - rs - 11); }
        }
      } else { cx.font = '12px Segoe UI, sans-serif'; cx.fillStyle = 'rgba(215,222,234,0.45)'; cx.fillText(s.nom, x, y + rs + 13); }
    }
    // ¿Qué asentamiento tienes delante? (el que se abre en plano).
    let local = null;
    if (z >= Z_LOCAL) { let dc = Infinity; for (const a of m.ase) { const p = S.nav.posAsent(m, a, t, tmp); const x = sx(p.x), y = sy(p.y); if (x < -50 || y < -50 || x > W + 50 || y > H + 50) continue; const d = (x - M.ox) * (x - M.ox) + (y - M.oy) * (y - M.oy); if (d < dc) { dc = d; local = a; } } }
    // Asentamientos e instalaciones.
    if (z >= Z_SIS) for (const a of m.ase) {
      const s = m.sis[a.sis]; const x0 = sx(s.x), y0 = sy(s.y);
      if (x0 < -400 * z / 4 - 200 || y0 < -400 * z / 4 - 200 || x0 > W + 400 * z / 4 + 200 || y0 > H + 400 * z / 4 + 200) { a._x = undefined; continue; }
      cx.strokeStyle = 'rgba(150,170,210,0.13)'; cx.lineWidth = 1; cx.beginPath(); cx.arc(x0, y0, a.orb.r * z, 0, TAU); cx.stroke();
      const p = S.nav.posAsent(m, a, t, tmp); const x = sx(p.x), y = sy(p.y); let r = radioAsent(a, z);
      if (a === local) r = S.clamp(z * 10, 130, Math.min(W - M.panel - 190, H - 150) * 0.4);
      a._x = x; a._y = y; a._r = r;
      if (a === local) continue;
      const col = capa === 'centro' ? (CLASE[a.clase] || '#59647a') : calorC ? calor(M.valorCapa(m, a)) + '1)' : colEstado(m, a.est);
      cx.fillStyle = col; cx.strokeStyle = a.sinley ? '#ff5a4f' : 'rgba(255,255,255,0.55)'; cx.lineWidth = 1; cx.globalAlpha = tenue ? 0.6 : 1;
      cx.beginPath();
      if (a.forma === 'planeta' || a.forma === 'luna') cx.arc(x, y, r, 0, TAU);
      else if (a.forma === 'cinturon') { cx.rect(x - r * 0.8, y - r * 0.8, r * 1.6, r * 1.6); }
      else { cx.moveTo(x, y - r); cx.lineTo(x + r, y); cx.lineTo(x, y + r); cx.lineTo(x - r, y); cx.closePath(); }
      cx.fill(); cx.stroke(); cx.globalAlpha = 1;
      if (a.cap) { cx.strokeStyle = '#e0b84c'; cx.lineWidth = 2; cx.beginPath(); cx.arc(x, y, r + 3.5, 0, TAU); cx.stroke(); cx.lineWidth = 1; if (a.est >= 0) A.banderaDe(cx, x + r + 5, y - r - 15, 20, 13, m, 'E', a.est); }
      if (a.H > 0.05) { cx.strokeStyle = 'rgba(224,86,79,0.9)'; cx.lineWidth = 2.5; cx.beginPath(); cx.arc(x, y, r + 6, -Math.PI / 2, -Math.PI / 2 + TAU * Math.min(1, a.H)); cx.stroke(); cx.lineWidth = 1; }
      if (a.f > 0.08) { cx.strokeStyle = 'rgba(255,79,208,0.9)'; cx.lineWidth = 2; cx.beginPath(); cx.arc(x, y, r + 9, -Math.PI / 2, -Math.PI / 2 + TAU * Math.min(1, a.f / 0.63)); cx.stroke(); cx.lineWidth = 1; }
      zon.push({ x, y, r: r + 5, ref: { t: 'A', id: a.id }, pri: 2 });
      if (z > 2.2) {
        cx.font = (z > 5 ? '13px' : '11px') + ' Segoe UI, sans-serif'; cx.fillStyle = 'rgba(230,236,246,0.95)'; cx.fillText(a.nom, x, y + r + (z >= Z_INS ? 44 : 14));
        let ico = ''; if (a.plaga > 0.05) ico += '🐛'; if (a.huelga > t) ico += '✊'; if (a.hambruna >= 0) ico += '🥣'; if (a.revuelta >= 0) ico += '🔥'; if (a.tie >= 0) ico += '⚔'; if (a.feria > t) ico += '🎪';
        if (ico && z < Z_INS) { cx.font = '12px Segoe UI Emoji, sans-serif'; cx.fillText(ico, x, y - r - 14); }
      }
      if (z >= Z_INS && x > -100 && y > -100 && x < W + 100 && y < H + 100) {
        const n = a.ins.length; const R1 = r + 24;
        for (let i = 0; i < n; i++) {
          const ins = m.ins[a.ins[i]]; const def = S.reg.inst[ins.tipo]; const ang = -Math.PI / 2 + i * TAU / n;
          const ix = x + Math.cos(ang) * (R1 + (n > 12 && i % 2 ? 22 : 0)), iy = y + Math.sin(ang) * (R1 + (n > 12 && i % 2 ? 22 : 0));
          cx.fillStyle = ins.salud < 0.05 ? '#3a1414' : ins.huelga ? '#3d3414' : '#172033'; cx.strokeStyle = ins.salud < 0.05 ? '#a33' : 'rgba(160,180,220,0.5)';
          cx.beginPath(); cx.rect(ix - 10, iy - 10, 20, 20); cx.fill(); cx.stroke();
          cx.font = '13px Segoe UI Emoji, sans-serif'; cx.fillStyle = '#fff'; cx.globalAlpha = ins.salud < 0.05 ? 0.35 : 1; cx.fillText(def.ico, ix, iy + 1); cx.globalAlpha = 1;
          if (ins.salud < 1 && !def.indestructible) { cx.fillStyle = '#400'; cx.fillRect(ix - 10, iy + 11, 20, 2.5); cx.fillStyle = ins.salud > 0.5 ? '#5cc28a' : '#e0564f'; cx.fillRect(ix - 10, iy + 11, 20 * ins.salud, 2.5); }
          if (ins.huelga) { cx.font = '10px Segoe UI Emoji'; cx.fillText('✊', ix + 9, iy - 9); }
          zon.push({ x: ix, y: iy, r: 12, ref: { t: 'I', id: ins.id }, pri: 4 });
        }
      }
    }
    // Naves.
    const kz = S.clamp(Math.pow(z, 0.45), 0.7, 3.2);
    for (const n of m.nav) {
      if (!n.vivo) { if (n.st === 'pecio' && z > 0.7) { const x = sx(n.x), y = sy(n.y); if (x > 0 && y > 0 && x < W && y < H) { cx.strokeStyle = 'rgba(160,160,160,0.55)'; cx.lineWidth = 1; cx.beginPath(); cx.moveTo(x - 2.5, y - 2.5); cx.lineTo(x + 2.5, y + 2.5); cx.moveTo(x + 2.5, y - 2.5); cx.lineTo(x - 2.5, y + 2.5); cx.stroke(); zon.push({ x, y, r: 5, ref: { t: 'N', id: n.id }, pri: 2.5 }); } } continue; }
      let x, y, ang = 0; const atr = n.en >= 0;
      if (atr) {
        if (z < Z_NAV) continue; const a = m.ase[n.en]; if (a._x === undefined || a === local) continue;
        const h = (n.id * 2.399963) % TAU; const rr = a._r + (z >= Z_INS ? 50 : 9) + (n.id % 3) * 4; x = a._x + Math.cos(h) * rr; y = a._y + Math.sin(h) * rr; ang = h + Math.PI / 2;
      } else { const p = S.nav.pos(m, n, t, tmp2); x = sx(p.x); y = sy(p.y); ang = p.ang; }
      if (x < -10 || y < -10 || x > W + 10 || y > H + 10) continue;
      const def = S.reg.nave[n.cls]; const gu = def.guerra; const escu = n.cascos > 1;
      if (n.st === 'batalla') { zon.push({ x, y, r: 6, ref: { t: 'N', id: n.id }, pri: 3 }); continue; }   // en combate se pinta el enjambre entero
      let col = n.pirata ? '#ff5a4f' : n.dueno.t === 'E' ? colEstado(m, n.dueno.id) : n.patente >= 0 ? '#ff9a4f' : n.cls === 'carronero' ? '#b0a080' : n.cls === 'bazar' ? '#e0b84c' : n.cls === 'arca' ? '#f0e6c0' : '#d7deea';
      if (n.cls === 'correo') col = '#6ee7ff';
      const L = (gu ? (escu ? 4.2 + Math.log10(n.cascos) * 2.4 : 4.4) : n.cls === 'arca' ? 9 : n.cls === 'granelero' || n.cls === 'granja' || n.cls === 'bazar' || n.cls === 'astillero_n' ? 4.8 : n.cls === 'correo' ? 2.8 : 3.4) * kz;
      cx.save(); cx.translate(x, y); cx.rotate(ang); cx.globalAlpha = n.st === 'amarrada' ? 0.35 : tenue && !gu ? 0.45 : 1;
      if (z >= Z_INS) A.naveMini(cx, n.cls, L, col, gu ? 'rgba(255,255,255,0.8)' : null);
      else { cx.fillStyle = col; cx.beginPath(); cx.moveTo(L, 0); cx.lineTo(-L * 0.8, L * 0.6); cx.lineTo(-L * 0.4, 0); cx.lineTo(-L * 0.8, -L * 0.6); cx.closePath(); cx.fill(); if (gu) { cx.strokeStyle = 'rgba(255,255,255,0.8)'; cx.lineWidth = 1; cx.stroke(); } }
      cx.restore(); cx.globalAlpha = 1;
      if (n.pas > 0) { cx.fillStyle = '#ffe66e'; cx.fillRect(x - 1, y - L - 3, 2, 2); }
      if (escu && !atr && z > 0.8) { cx.font = '9px Segoe UI, sans-serif'; cx.fillStyle = 'rgba(255,255,255,0.85)'; cx.fillText(n.cascos, x, y + L + 6); }
      if (z >= Z_INS && !atr && (gu || n.cls === 'arca' || n.cls === 'bazar')) { cx.font = '10px Segoe UI, sans-serif'; cx.fillStyle = 'rgba(230,236,246,0.8)'; cx.fillText(n.nom, x, y - L - 6); }
      zon.push({ x, y, r: 6 + kz, ref: { t: 'N', id: n.id }, pri: 3 });
    }
    // El asentamiento que tienes delante, en plano.
    if (local && local._x !== undefined) Lc.asentamiento(cx, m, local, local._x, local._y, local._r, zon, t);
    // Campos de restos de las grandes batallas.
    if (z > 0.5) for (const s of m.sis) { if (!(s.restos > 0)) continue; const x = sx(s.x), y = sy(s.y); const k = Math.min(160, Math.round(s.restos)); const Rr = S.clamp(22 * z, 20, 200); cx.fillStyle = 'rgba(150,150,150,0.5)'; for (let i = 0; i < k; i++) { const ang = azar(i * 3.3 + s.id) * TAU, rad = Rr * (0.5 + 0.7 * azar(i * 7.7 + s.id)); cx.fillRect(x + Math.cos(ang) * rad, y + Math.sin(ang) * rad, 1.4, 1.4); } }
    // Batallas en curso: de lejos, dos enjambres; de cerca, nave a nave.
    const pul = (performance.now() % 900) / 900; const tb = performance.now();
    for (const b of m.bat) {
      if (!b.vivo) continue; const s = m.sis[b.sis]; const x = sx(s.x), y = sy(s.y); const Rb = S.clamp(30 * z, 36, 520);
      if (x < -Rb * 2 || y < -Rb * 2 || x > W + Rb * 2 || y > H + Rb * 2) continue;
      if (z >= Z_BAT) { Lc.batalla(cx, m, b, x, y, Rb, z, zon); continue; }
      cx.strokeStyle = 'rgba(255,90,79,' + (1 - pul) + ')'; cx.lineWidth = 2; cx.beginPath(); cx.arc(x, y, Rb * 0.5 + pul * Rb, 0, TAU); cx.stroke(); cx.lineWidth = 1;
      const pts = [[], []];
      b.L.forEach((l, k) => {
        const n = Math.min(520, Math.round(l.n)); const lado = k ? 1 : -1; cx.fillStyle = colEstado(m, l.est); const tam = z > 3 ? 2.4 : 1.7;
        for (let i = 0; i < n; i++) { const h1 = azar(i * 1.37 + k * 71.3), h2 = azar(i * 2.11 + k * 13.7); const ang = (h1 - 0.5) * 2.5; const rad = Rb * (0.22 + 0.78 * Math.sqrt(h2)); const px = x + lado * Math.cos(ang) * rad + Math.sin(tb / 600 + i) * 1.2, py = y + Math.sin(ang) * rad * 0.95 + Math.cos(tb / 700 + i * 1.7) * 1.2; cx.fillRect(px - tam / 2, py - tam / 2, tam, tam); if (i < 60) pts[k].push(px, py); }
      });
      const q = Math.floor(tb / 80);
      for (let i = 0; i < 34 && pts[0].length && pts[1].length; i++) { const a0 = Math.floor(azar(q + i * 3.1) * pts[0].length / 2) * 2, b0 = Math.floor(azar(q * 1.7 + i * 5.3) * pts[1].length / 2) * 2; cx.strokeStyle = azar(q + i) > 0.5 ? 'rgba(255,220,150,0.5)' : 'rgba(255,120,100,0.45)'; cx.beginPath(); cx.moveTo(pts[0][a0], pts[0][a0 + 1]); cx.lineTo(pts[1][b0], pts[1][b0 + 1]); cx.stroke(); }
      cx.font = 'bold 13px Segoe UI, sans-serif'; cx.fillStyle = '#fff'; cx.fillText(Math.round(b.L[0].n) + '  ⚔  ' + Math.round(b.L[1].n), x, y - Rb - 12);
      zon.push({ x, y: y - Rb - 12, r: 16, ref: { t: 'V', id: b.ev }, pri: 3.5 });
    }
    if (capa === 'rebeliones') capaRebeliones(m, t, z);
    else if (capa === 'relaciones') capaRelaciones(m, t, z, sel);
    else if (capa === 'estrategia') capaEstrategia(m, t, z);
    else if (capa === 'noticia' && M.noticia) capaNoticia(m, z);
    if (sel && (sel.t === 'P' || sel.t === 'F') && capa !== 'relaciones') lazos(m, t, sel, 0.55);
    // Avisos de hechos recientes.
    const ah = performance.now();
    for (let i = M.pings.length - 1; i >= 0; i--) {
      const p = M.pings[i]; const e = (ah - p.t0) / (p.g >= 3 ? 4200 : 2600); if (e >= 1) { M.pings.splice(i, 1); continue; }
      const x = sx(p.x), y = sy(p.y); cx.strokeStyle = rgba(p.col, 1 - e); cx.lineWidth = p.g >= 2 ? 2.5 : 1.3;
      cx.beginPath(); cx.arc(x, y, 6 + e * (p.g >= 3 ? 90 : p.g >= 2 ? 55 : 28), 0, TAU); cx.stroke(); cx.lineWidth = 1;
    }
    // A quién sigues: su retrato sobre el mapa.
    if (M.seguir) {
      const p = M.posDe(m, M.seguir, t);
      if (p) { const x = sx(p.x), y = sy(p.y); cx.strokeStyle = '#ffe9b0'; cx.lineWidth = 1.5; cx.beginPath(); cx.arc(x, y, 10, 0, TAU); cx.stroke(); if (M.seguir.t === 'P') { const pe = m.per[M.seguir.id]; A.avatar(cx, x + 14, y - 52, 44, A.descPersona(m, pe)); cx.font = 'bold 11px Segoe UI, sans-serif'; cx.textAlign = 'left'; cx.fillStyle = '#ffe9b0'; cx.fillText(pe.nom, x + 62, y - 44); cx.textAlign = 'center'; linea(x, y, x + 14, y - 8, '#ffe9b0', 1); } }
    }
    // Selección.
    if (sel) {
      const p = M.posDe(m, sel, t);
      if (p) { let x = sx(p.x), y = sy(p.y); if (sel.t === 'N' && m.nav[sel.id].en >= 0 && z >= Z_NAV) { const zz = zon.find(q => q.ref.t === 'N' && q.ref.id === sel.id); if (zz) { x = zz.x; y = zz.y; } } if (sel.t === 'I') { const zz = zon.find(q => q.ref.t === 'I' && q.ref.id === sel.id); if (zz) { x = zz.x; y = zz.y; } } if (!(sel.t === 'A' && local && local.id === sel.id)) { cx.strokeStyle = '#ffffff'; cx.lineWidth = 1; cx.setLineDash([4, 3]); cx.beginPath(); cx.arc(x, y, 13 + 2 * Math.sin(ah / 250), 0, TAU); cx.stroke(); cx.setLineDash([]); } }
    }
  };

  // ── Capa: solo rebeliones. Calle, terror, casas de las facciones, sus puentes y por dónde corre el ejemplo.
  function capaRebeliones(m, t, z) {
    const cerca = z >= Z_SIS; const pos = (a) => cerca ? pAse(m, a, t) : pSis(m, a.sis);
    // Contagio: de dónde salió cada revolución reciente y adónde ha llegado ya la noticia.
    for (let i = m.paq.length - 1; i >= 0 && i > m.paq.length - 4000; i--) {
      const p = m.paq[i]; if (m.t - p.t > 2 * S.ANIO) break; if (p.k !== 'revolucion' && p.k !== 'insurreccion') continue;
      const [x0, y0] = pos(m.ase[p.a]); const al = 0.55 * (1 - (m.t - p.t) / (2 * S.ANIO));
      for (const a of m.ase) { if (a.id === p.a || !a.not.has(p.id)) continue; const [x1, y1] = pos(a); linea(x0, y0, x1, y1, 'rgba(110,231,255,' + al + ')', 1, [2, 6]); }
      cx.strokeStyle = 'rgba(255,79,208,' + (al + 0.3) + ')'; cx.lineWidth = 2; cx.beginPath(); cx.arc(x0, y0, 16, 0, TAU); cx.stroke();
    }
    for (const f of m.fac) {
      if (!f.vivo || f.tipo === 'casa' || !f.cel.length) continue;
      f.cel.forEach((c, i) => { if (c.padre < 0 || !f.cel[c.padre]) return; const [x0, y0] = pos(m.ase[f.cel[c.padre].ase]), [x1, y1] = pos(m.ase[c.ase]); linea(x0, y0, x1, y1, c.puente < 0.1 ? 'rgba(255,90,79,0.9)' : 'rgba(92,194,138,0.85)', 1 + c.puente * 4, c.puente < 0.1 ? [4, 4] : null); void i; });
      const [x, y] = pos(m.ase[f.sede]); if (f.etapa === 'inst') { A.banderaDe(cx, x - 9, y - 30 - (f.id % 3) * 12, 18, 11, m, 'F', f.id); M.zonas.push({ x, y: y - 25 - (f.id % 3) * 12, r: 9, ref: { t: 'F', id: f.id }, pri: 3.8 }); }
    }
    for (const a of m.ase) {
      if (a.f < 0.06 && !(a.terror > 0.1) && !(a.efic > 0.1)) continue; const [x, y] = pos(a); const r0 = cerca ? (a._r || 6) : 8;
      if (a.efic > 0.1) { cx.strokeStyle = 'rgba(110,231,255,' + Math.min(0.9, a.efic * 1.5) + ')'; cx.lineWidth = 1.5; cx.setLineDash([2, 3]); cx.beginPath(); cx.arc(x, y, r0 + 22, 0, TAU); cx.stroke(); cx.setLineDash([]); }
      if (a.f >= 0.06) { const gr = cx.createRadialGradient(x, y, r0, x, y, r0 + 10 + 44 * a.f); gr.addColorStop(0, 'rgba(255,79,208,' + (0.25 + a.f) + ')'); gr.addColorStop(1, 'rgba(255,79,208,0)'); cx.fillStyle = gr; cx.beginPath(); cx.arc(x, y, r0 + 10 + 44 * a.f, 0, TAU); cx.fill(); cx.font = 'bold 11px Segoe UI'; cx.fillStyle = '#ffd0f0'; cx.fillText(Math.round(a.f * 100) + ' %', x, y - r0 - 8); }
      if (a.terror > 0.1) { cx.strokeStyle = 'rgba(192,57,43,0.95)'; cx.lineWidth = 3; cx.beginPath(); cx.arc(x, y, r0 + 14, 0, TAU); cx.stroke(); cx.lineWidth = 1; }
    }
  }
  // ── Capa: interrelaciones. Guerras, alianzas, rencores, comercio y deudas entre Estados; y los lazos de lo que elijas.
  function capaRelaciones(m, t, z, sel) {
    const an = performance.now() / 60;
    for (const e of m.est) {
      if (!e.vivo) continue; const [x0, y0] = pCap(m, e, t);
      for (const [j, w] of e.gue) { if (j < e.id || !m.est[j].vivo) continue; const [x1, y1] = pCap(m, m.est[j], t); cx.lineDashOffset = -an; linea(x0, y0, x1, y1, 'rgba(255,70,60,0.95)', w.civil ? 5 : 3.5, [10, 7]); cx.lineDashOffset = 0; rotulo((x0 + x1) / 2, (y0 + y1) / 2, w.civil ? 'guerra civil' : 'guerra', '#ff8a80'); }
      if (e.alianzas) for (const j of e.alianzas) { if (j < e.id || !m.est[j].vivo) continue; const [x1, y1] = pCap(m, m.est[j], t); linea(x0, y0, x1, y1, 'rgba(92,240,138,0.9)', 3); rotulo((x0 + x1) / 2, (y0 + y1) / 2, 'alianza', '#9dffbb'); }
      for (const [j, r] of e.rel) {
        if (j < e.id || !m.est[j].vivo || e.gue.has(j) || (e.alianzas && e.alianzas.has(j))) continue; const [x1, y1] = pCap(m, m.est[j], t); const r2 = m.est[j].rel.get(e.id); const cb = Math.max(r.cb, r2 ? r2.cb : 0), com = r.com || 0;
        if (cb > 0.5) { linea(x0, y0 + 3, x1, y1 + 3, 'rgba(255,160,70,' + Math.min(0.9, 0.3 + cb * 0.25) + ')', 1 + Math.min(3, cb), [3, 4]); rotulo((x0 + x1) / 2, (y0 + y1) / 2 + 12, 'rencor', '#ffbe8a'); }
        if (com > 60) linea(x0, y0 - 3, x1, y1 - 3, 'rgba(95,176,201,0.75)', 1 + Math.min(5, Math.log10(com) - 1.2));
      }
    }
    for (const d of m.deu) { if (d.monto < 5000 || !m.est[d.deudor].vivo || !m.fac[d.acreedor].vivo) continue; const [x0, y0] = z >= Z_SIS ? pAse(m, m.ase[m.fac[d.acreedor].sede], t) : pSis(m, m.ase[m.fac[d.acreedor].sede].sis); const [x1, y1] = pCap(m, m.est[d.deudor], t); linea(x0, y0, x1, y1, 'rgba(224,184,76,0.7)', 1 + Math.min(4, d.monto / 150000), [1, 5]); }
    for (const e of m.est) { if (!e.vivo) continue; const [x, y] = pCap(m, e, t); A.banderaDe(cx, x - 15, y - 34, 30, 20, m, 'E', e.id); cx.font = 'bold 11px Segoe UI'; cx.fillStyle = '#fff'; cx.fillText(e.nom, x, y - 42); M.zonas.push({ x, y: y - 24, r: 16, ref: { t: 'E', id: e.id }, pri: 3.6 }); }
    if (sel) lazos(m, t, sel, 0.95);
  }
  function rotulo(x, y, txt, col) { cx.font = '10px Segoe UI, sans-serif'; cx.fillStyle = 'rgba(7,10,18,0.75)'; const w = txt.length * 5.6 + 8; cx.fillRect(x - w / 2, y - 7, w, 14); cx.fillStyle = col; cx.fillText(txt, x, y); }
  // Lazos de una persona (familia, a quién busca, a quién sirve, su facción) o de una facción (sus casas).
  function lazos(m, t, sel, al) {
    const P = (id) => { const p = M.posDe(m, { t: 'P', id }, t); return p ? [M.sx(p.x), M.sy(p.y)] : null; };
    if (sel.t === 'P') {
      const p = m.per[sel.id]; const o = P(p.id); if (!o) return; const tr = (id, col, w, txt, dash) => { const q = P(id); if (!q) return; const d = Math.hypot(q[0] - o[0], q[1] - o[1]); if (d < 3) return; linea(o[0], o[1], q[0], q[1], col, w, dash); if (d > 40) rotulo((o[0] + q[0]) / 2, (o[1] + q[1]) / 2, txt + ': ' + m.per[id].nom, '#fff'); cx.fillStyle = col; cx.beginPath(); cx.arc(q[0], q[1], 4, 0, TAU); cx.fill(); };
      if (p.fam.con >= 0) tr(p.fam.con, 'rgba(255,255,255,' + al + ')', 1.5, 'pareja');
      for (const h of p.fam.hijos) tr(h, 'rgba(220,220,255,' + al * 0.8 + ')', 1, 'hijo');
      for (const h of p.fam.padres) tr(h, 'rgba(220,220,255,' + al * 0.8 + ')', 1, 'padre');
      if (p.ven && p.ven.obj >= 0) tr(p.ven.obj, 'rgba(255,70,60,' + al + ')', 2.5, 'busca a', [6, 4]);
      for (const v of (m.vengadores || [])) if (v.vivo && v.ven && v.ven.estado === 'activa' && v.ven.obj === p.id) tr(v.id, 'rgba(255,70,60,' + al + ')', 2.5, 'lo busca', [6, 4]);
      if (p.est >= 0 && m.est[p.est].vivo && m.est[p.est].gob >= 0 && m.est[p.est].gob !== p.id) tr(m.est[p.est].gob, rgba(m.est[p.est].col, al), 1.5, 'sirve a');
      if (p.fac >= 0 && m.fac[p.fac].lid >= 0 && m.fac[p.fac].lid !== p.id) tr(m.fac[p.fac].lid, 'rgba(92,194,138,' + al + ')', 1.5, 'sigue a');
      let k = 0; for (const [id, r] of p.rel) { if (k++ > 5 || Math.abs(r.op) < 0.4) continue; tr(id, r.op > 0 ? 'rgba(92,240,138,' + al * 0.6 + ')' : 'rgba(255,160,70,' + al * 0.6 + ')', 1, r.op > 0 ? 'aprecia a' : 'detesta a', [2, 3]); }
    } else if (sel.t === 'F') {
      const f = m.fac[sel.id]; const pos = (a) => M.cam.z >= Z_SIS ? pAse(m, a, t) : pSis(m, a.sis);
      f.cel.forEach((c) => { const [x1, y1] = pos(m.ase[c.ase]); if (c.padre >= 0 && f.cel[c.padre]) { const [x0, y0] = pos(m.ase[f.cel[c.padre].ase]); linea(x0, y0, x1, y1, c.puente < 0.1 ? 'rgba(255,90,79,0.9)' : 'rgba(92,194,138,' + al + ')', 1 + c.puente * 4, c.puente < 0.1 ? [4, 4] : null); rotulo((x0 + x1) / 2, (y0 + y1) / 2, 'puente ' + c.puente.toFixed(2), '#bff0d0'); } cx.strokeStyle = 'rgba(92,194,138,0.9)'; cx.lineWidth = 2; cx.beginPath(); cx.arc(x1, y1, 12, 0, TAU); cx.stroke(); });
      if (f.almacenes) for (const id of f.almacenes) { const [x1, y1] = pos(m.ase[id]); cx.strokeStyle = 'rgba(224,184,76,0.9)'; cx.lineWidth = 2; cx.beginPath(); cx.arc(x1, y1, 12, 0, TAU); cx.stroke(); }
    }
  }
  // ── Capa: comercio. Por dónde se mueve el género (grosor = toneladas al mes; color = lo que más se mueve).
  function capaComercio(m, t, z) {
    if (!m.flu) return; const cerca = z >= Z_SIS; const fl = Array.from(m.flu.values()).sort((x, y) => y.q - x.q).slice(0, 170);
    for (const f of fl) {
      const a = m.ase[f.de], b = m.ase[f.a]; const [x0, y0] = cerca ? pAse(m, a, t) : pSis(m, a.sis); const [x1, y1] = cerca ? pAse(m, b, t) : pSis(m, b.sis);
      let c = 0; for (let k = 1; k < S.NB; k++) if (f.c[k] > f.c[c]) c = k;
      const w = S.clamp(Math.sqrt(f.q) / 9, 0.6, 7); const mx = (x0 + x1) / 2 - (y1 - y0) * 0.12, my = (y0 + y1) / 2 + (x1 - x0) * 0.12;
      cx.strokeStyle = rgba(S.bien[c].col, 0.6); cx.lineWidth = w; cx.beginPath(); cx.moveTo(x0, y0); cx.quadraticCurveTo(mx, my, x1, y1); cx.stroke();
      const u = (performance.now() / 2500 + f.de * 0.13) % 1; const px = (1 - u) * (1 - u) * x0 + 2 * (1 - u) * u * mx + u * u * x1, py = (1 - u) * (1 - u) * y0 + 2 * (1 - u) * u * my + u * u * y1; cx.fillStyle = S.bien[c].col; cx.beginPath(); cx.arc(px, py, Math.max(1.5, w * 0.7), 0, TAU); cx.fill();
    }
    cx.lineWidth = 1; let y = 62; cx.textAlign = 'left'; cx.font = '11px Segoe UI, sans-serif';
    for (let c = 0; c < S.NB; c++) { cx.fillStyle = S.bien[c].col; cx.fillRect(160, y - 5, 12, 8); cx.fillStyle = '#d7deea'; cx.fillText(S.bien[c].nom, 177, y); y += 14; }
    cx.textAlign = 'center';
  }
  // ── Capa: estrategia. Pasos por los que cruza todo, flotas, tormentas, guerras y alianzas.
  function capaEstrategia(m, t, z) {
    for (const s of m.sis) { const bc = s.bc || 0; if (bc < 0.25) continue; const [x, y] = pSis(m, s.id); cx.strokeStyle = 'rgba(224,184,76,' + (0.25 + bc * 0.7) + ')'; cx.lineWidth = 1 + bc * 4; cx.beginPath(); cx.arc(x, y, 18 + bc * 26, 0, TAU); cx.stroke(); cx.lineWidth = 1; if (bc > 0.5) rotulo(x, y - 26 - bc * 26, 'paso clave ' + Math.round(bc * 100), '#ffe9a0'); }
    for (const e of m.est) { if (!e.vivo) continue; const [x0, y0] = pCap(m, e, t); for (const [j] of e.gue) { if (j < e.id || !m.est[j].vivo) continue; const [x1, y1] = pCap(m, m.est[j], t); linea(x0, y0, x1, y1, 'rgba(255,70,60,0.7)', 3, [10, 7]); } if (e.alianzas) for (const j of e.alianzas) { if (j < e.id || !m.est[j].vivo) continue; const [x1, y1] = pCap(m, m.est[j], t); linea(x0, y0, x1, y1, 'rgba(92,240,138,0.7)', 2.5); } }
    for (const f of m.flo) {
      if (!f.vivo || !f.nav.length) continue; let c = 0; for (const id of f.nav) c += m.nav[id].cascos; const n = m.nav[f.insignia !== undefined && m.nav[f.insignia] && m.nav[f.insignia].vivo ? f.insignia : f.nav[0]]; const p = S.nav.pos(m, n, t, tmp2); const x = M.sx(p.x), y = M.sy(p.y);
      cx.fillStyle = 'rgba(7,10,18,0.8)'; cx.fillRect(x + 8, y - 20, 62, 15); A.banderaDe(cx, x + 9, y - 19, 19, 13, m, 'E', f.est); cx.font = 'bold 11px Segoe UI'; cx.textAlign = 'left'; cx.fillStyle = '#fff'; cx.fillText(c + (f.st === 'sitio' ? ' ⚔' : f.st === 'viaje' ? ' →' : ''), x + 31, y - 12); cx.textAlign = 'center';
      if (f.mis && f.mis.sis !== undefined) { const [x1, y1] = pSis(m, f.mis.sis); linea(x, y, x1, y1, rgba(colEstado(m, f.est), 0.8), 1.5, [5, 4]); }
    }
  }
  function capaNoticia(m, z) {
    for (const a of m.ase) {
      let ver = null; for (const pid of M.noticia.paq) { const v = a.not.get(pid); if (v) { ver = v; break; } }
      let x, y, r;
      if (z >= Z_SIS && a._x !== undefined) { x = a._x; y = a._y; r = Math.min(a._r, 34) + 5; } else { const s = m.sis[a.sis]; x = M.sx(s.x); y = M.sy(s.y); r = 9 + (a.id % 3) * 3; }
      cx.strokeStyle = ver ? '#5cf08a' : 'rgba(255,90,79,0.5)'; cx.lineWidth = ver ? 2 : 1; cx.beginPath(); cx.arc(x, y, r, 0, TAU); cx.stroke(); cx.lineWidth = 1;
      if (ver && M.noticia.x > 0 && z > 0.5) { cx.font = '10px Segoe UI'; cx.fillStyle = '#b9ffcf'; cx.fillText(Math.round(ver.x) + (ver.n ? ' (' + ver.n + ' bocas)' : ''), x, y - r - 7); }
    }
  }

  // Qué hay bajo el cursor: gana lo más concreto (cara > instalación > nave > asentamiento > sistema).
  M.elegir = function (px, py) {
    let best = null, bs = -Infinity;
    for (const q of M.zonas) { const d = Math.hypot(q.x - px, q.y - py); if (d > q.r + 3) continue; const s = q.pri * 100 - d; if (s > bs) { bs = s; best = q; } }
    return best ? best.ref : null;
  };
  M.elegirNoticia = function (m, evId) {
    const paq = []; let x = 0;
    for (const p of m.paq) if (p.ev === evId) { paq.push(p.id); if (p.x > x) x = p.x; }
    M.noticia = { ev: evId, paq, x };
    return paq.length;
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
