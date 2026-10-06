// Vistas de cerca (L3): la batalla nave a nave y la superficie de un asentamiento con sus edificios, su gente,
// sus soldados, sus colas, sus ruinas y sus combates. Todo se pinta a partir del estado real del núcleo:
// si una escuadra pierde cascos, aquí revientan; si arrasan una fábrica, aquí hay un cráter.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI = g.UI || {}; const A = U.arte;
  const L = U.local = { vis: new Map(), planos: new Map(), gente: null, fx: [] };
  const TAU = Math.PI * 2;
  const azar = (a) => { const x = Math.sin(a * 127.1 + 311.7) * 43758.5453; return x - Math.floor(x); };
  const ahora = () => performance.now();
  const TAM = { acorazado: 3.4, crucero: 2.3, fragata: 1.5, corbeta: 1.2 };

  // ════ Batalla ════
  function crearVis(m, bt) {
    const v = { lados: [], proj: [], boom: [], tDisparo: 0 };
    for (let k = 0; k < 2; k++) {
      const l = bt.L[k]; let tot = 0; for (const id of l.nav0) tot += Math.max(0, m.nav[id].cascos);
      const esc = Math.min(1, 380 / Math.max(1, tot)); const sq = []; const nq = l.nav0.length; const cols = Math.max(1, Math.ceil(Math.sqrt(nq / 2)));
      let insignia = -1, bi = -1;
      l.nav0.forEach((id, i) => {
        const n = m.nav[id]; const want = n.vivo ? Math.max(1, Math.round(n.cascos * esc)) : 0; const col = i % cols, fil = Math.floor(i / cols); const filas = Math.ceil(nq / cols);
        const cx0 = cols > 1 ? (col / (cols - 1) - 0.5) * 1.5 : 0, cy0 = filas > 1 ? (fil / (filas - 1) - 0.5) * 1.7 : 0; const sp = [];
        for (let j = 0; j < want; j++) sp.push({ ox: cx0 + (azar(id * 3.1 + j) - 0.5) * 0.55, oy: cy0 + (azar(id * 7.7 + j * 1.3) - 0.5) * 0.5, ph: azar(id + j * 0.37) * 9, mu: 0 });
        const val = S.reg.nave[n.cls].ef * 100 + n.cascos; if (n.vivo && val > bi) { bi = val; insignia = i; }
        sq.push({ id, cls: n.cls, sp, vivos: want });
      });
      v.lados.push({ sq, esc, insignia });
    }
    return v;
  }
  // Colores de los bandos: el del Estado (rosa para insurrectos, gris sin Estado); si se parecen, el segundo se aclara u oscurece.
  const rgbDe = (c) => { if (c[0] === '#') { let s = c.slice(1); if (s.length === 3) s = s.replace(/./g, '$&$&'); const n = parseInt(s.slice(0, 6), 16); return [n >> 16 & 255, n >> 8 & 255, n & 255]; } const x = /(\d+)\D+(\d+)\D+(\d+)/.exec(c); return x ? [+x[1], +x[2], +x[3]] : [128, 128, 128]; };
  L.contraste = function (a, b) {
    const p = rgbDe(a), q = rgbDe(b); if (Math.abs(p[0] - q[0]) + Math.abs(p[1] - q[1]) + Math.abs(p[2] - q[2]) > 120) return [a, b];
    const claro = q[0] + q[1] + q[2] > 480; return [a, 'rgb(' + q.map(v => Math.round(claro ? v * 0.45 : v + (255 - v) * 0.65)).join(',') + ')'];
  };
  const colEst = (m, id, def) => id >= 0 && m.est[id] ? m.est[id].col : def;
  L.colBatalla = (m, bt) => L.contraste(colEst(m, bt.L[0].est, '#bbbbbb'), colEst(m, bt.L[1].est, '#bbbbbb'));
  L.colTierra = (m, t) => L.contraste(colEst(m, t.atk.est, '#ff4fd0'), colEst(m, t.def.est, '#bbbbbb'));
  // Qué pinta cada bando en la batalla, en una palabra.
  L.papel = function (bt, k) {
    const p = bt.L[k].papel, o = bt.L[1 - k].papel; if (!p || !o) return k ? 'defiende' : 'ataca';
    if (p.casa !== o.casa) return p.casa ? 'defiende' : 'invade';
    return p.llega ? 'ataca' : 'defiende';
  };
  const by0 = (y, R) => y - R * 0.66 + 2;
  L.batalla = function (c, m, bt, x, y, R, z, zonas) {
    let v = L.vis.get(bt.id); if (!v) { v = crearVis(m, bt); L.vis.set(bt.id, v); }
    const t = ahora(); const sc = S.clamp(z / 6, 0.9, 3.2);
    const cen = [x - R * (0.5 - 0.06 * Math.sin(t / 3800)), x + R * (0.5 - 0.06 * Math.sin(t / 3800))];
    const pos = (k, sp) => [cen[k] + (k ? -1 : 1) * sp.ox * R * 0.36 + Math.sin(t / 900 + sp.ph) * 2.2, y + sp.oy * R * 0.5 + Math.cos(t / 1100 + sp.ph) * 2.2];
    const vivos = [[], []];
    // Sincroniza con el núcleo: los cascos que la simulación ha dado por perdidos revientan aquí.
    for (let k = 0; k < 2; k++) {
      const lv = v.lados[k]; const col = L.colBatalla(m, bt)[k];
      lv.sq.forEach((sq, qi) => {
        const n = m.nav[sq.id]; const want = n.vivo && n.st === 'batalla' ? Math.max(1, Math.round(n.cascos * lv.esc)) : (n.vivo ? Math.round(n.cascos * lv.esc) : 0);
        while (sq.vivos > want) { const cand = sq.sp.filter(s => !s.mu); if (!cand.length) break; cand[Math.floor(azar(t + sq.vivos) * cand.length)].mu = t; sq.vivos--; }
        const Lz = (TAM[sq.cls] || 1.5) * sc * 2.2;
        for (let i = sq.sp.length - 1; i >= 0; i--) {
          const s = sq.sp[i]; const [px, py] = pos(k, s);
          if (s.mu) { const e = (t - s.mu) / 800; if (e >= 1) { sq.sp.splice(i, 1); continue; } c.fillStyle = 'rgba(255,' + Math.round(220 - 160 * e) + ',' + Math.round(120 - 120 * e) + ',' + (1 - e) + ')'; c.beginPath(); c.arc(px, py, Lz * (0.6 + e * 2.6), 0, TAU); c.fill(); continue; }
          const ins = qi === lv.insignia && i === 0;
          c.save(); c.translate(px, py); if (k) c.rotate(Math.PI); A.naveMini(c, sq.cls, ins ? Lz * 1.9 : Lz, col, ins ? '#ffffff' : null); c.restore();
          vivos[k].push(px, py, sq.cls === 'acorazado' ? 2 : 1);
          if (ins) { c.font = '10px Segoe UI, sans-serif'; c.fillStyle = '#fff'; c.textAlign = 'center'; c.fillText('✦ ' + n.nom, px, py - Lz * 1.9 - 5); zonas.push({ x: px, y: py, r: 12, ref: { t: 'N', id: n.id }, pri: 4 }); }
        }
      });
    }
    // Fuego: el color de cada disparo es el del arma que lo hace (cañón, láser, misil), en la proporción del diseño de cada bando.
    if (t - v.tDisparo > 70 && vivos[0].length && vivos[1].length) {
      v.tDisparo = t;
      for (let k = 0; k < 2; k++) {
        const g2 = bt.L[k].perfil0 ? bt.L[k].perfil0.g : S.tec.G0; const ge = bt.L[1 - k].perfil0 ? bt.L[1 - k].perfil0.g : S.tec.G0; const nd = Math.min(14, 1 + Math.floor(vivos[k].length / 3 / 14));
        for (let i = 0; i < nd; i++) {
          const a0 = Math.floor(azar(t + i * 3.3 + k) * vivos[k].length / 3) * 3, b0 = Math.floor(azar(t * 1.7 + i * 5.1 + k) * vivos[1 - k].length / 3) * 3;
          const u = azar(t * 0.31 + i) * (g2[0] + g2[1] + g2[2]); const tipo = u < g2[0] ? 0 : u < g2[0] + g2[1] ? 1 : 2;
          const para = azar(t * 0.77 + i * 2.2) < ge[3 + tipo] * 2.2;      // lo para la defensa que toca
          v.proj.push({ x0: vivos[k][a0], y0: vivos[k][a0 + 1], x1: vivos[1 - k][b0] + (azar(i + t) - 0.5) * 6, y1: vivos[1 - k][b0 + 1] + (azar(i * 2 + t) - 0.5) * 6, t0: t, dur: tipo === 1 ? 110 : tipo === 0 ? 420 : 950, tipo, para });
        }
      }
      if (v.proj.length > 420) v.proj.splice(0, v.proj.length - 420);
    }
    for (let i = v.proj.length - 1; i >= 0; i--) {
      const p = v.proj[i]; const e = (t - p.t0) / p.dur; const fin = p.para ? 0.86 : 1;
      if (e >= fin) { v.boom.push({ x: p.x0 + (p.x1 - p.x0) * fin, y: p.y0 + (p.y1 - p.y0) * fin, t0: t, k: p.para ? 3 + p.tipo : p.tipo }); v.proj.splice(i, 1); continue; }
      const px = p.x0 + (p.x1 - p.x0) * e, py = p.y0 + (p.y1 - p.y0) * e;
      if (p.tipo === 1) { c.strokeStyle = 'rgba(255,80,70,' + (0.85 - e * 0.5) + ')'; c.lineWidth = 1.2; c.beginPath(); c.moveTo(p.x0, p.y0); c.lineTo(p.x1, p.y1); c.stroke(); }
      else if (p.tipo === 0) { const dx = (p.x1 - p.x0), dy = (p.y1 - p.y0), d = Math.hypot(dx, dy) || 1; c.strokeStyle = 'rgba(255,225,130,0.95)'; c.lineWidth = 1.3; c.beginPath(); c.moveTo(px, py); c.lineTo(px - dx / d * 5, py - dy / d * 5); c.stroke(); }
      else { c.fillStyle = '#ffffff'; c.fillRect(px - 1, py - 1, 2, 2); c.strokeStyle = 'rgba(220,220,220,0.25)'; c.lineWidth = 1; c.beginPath(); c.moveTo(px, py); c.lineTo(px - (p.x1 - p.x0) * 0.08, py - (p.y1 - p.y0) * 0.08); c.stroke(); }
    }
    if (v.boom.length > 260) v.boom.splice(0, v.boom.length - 260);
    for (let i = v.boom.length - 1; i >= 0; i--) {
      const b = v.boom[i]; const e = (t - b.t0) / 260; if (e >= 1) { v.boom.splice(i, 1); continue; }
      if (b.k >= 3) { c.strokeStyle = (b.k === 4 ? 'rgba(95,176,201,' : b.k === 5 ? 'rgba(185,143,214,' : 'rgba(170,180,195,') + (1 - e) + ')'; c.lineWidth = 1.2; c.beginPath(); c.arc(b.x, b.y, 2 + e * 5, 0, TAU); c.stroke(); }
      else { c.fillStyle = 'rgba(255,' + Math.round(230 - 120 * e) + ',140,' + (1 - e) + ')'; c.beginPath(); c.arc(b.x, b.y, 1.5 + e * 3.5, 0, TAU); c.fill(); }
    }
    // Rótulo de cada bando: bandera, nombre en su color, qué hace aquí y cuántas naves le quedan.
    const cols = L.colBatalla(m, bt);
    for (let k = 0; k < 2; k++) {
      // El de la izquierda escribe hacia la izquierda y el de la derecha hacia la derecha: nunca se pisan.
      const l = bt.L[k]; const e = m.est[l.est]; const bx = x + (k ? 14 : -14), by = y - R * 0.66; c.textAlign = k ? 'left' : 'right';
      if (e) A.banderaDe(c, k ? bx : bx - 26, by - 34, 26, 17, m, 'E', e.id);
      c.font = 'bold 12px Segoe UI, sans-serif'; c.fillStyle = cols[k]; c.fillText((e ? e.nom : 'Sin Estado') + ' · ' + L.papel(bt, k), bx, by - 4);
      c.font = '11px Segoe UI, sans-serif'; c.fillStyle = '#fff'; c.fillText(Math.round(l.n).toLocaleString('es-ES') + ' de ' + l.n0.toLocaleString('es-ES') + ' naves', bx, by + 10);
    }
    c.textAlign = 'center'; c.font = '13px Segoe UI Emoji, Segoe UI, sans-serif'; c.fillStyle = '#fff'; c.fillText('⚔', x, by0(y, R));
    // Balanza: quién va ganando (pegada × número², o × número entre asteroides).
    const fz = S.com.fuerza(bt); const bw = R * 0.9, bx0 = x - bw / 2, byb = y + R * 0.6;
    c.fillStyle = cols[0]; c.fillRect(bx0, byb, bw * fz, 5); c.fillStyle = cols[1]; c.fillRect(bx0 + bw * fz, byb, bw * (1 - fz), 5);
    c.strokeStyle = 'rgba(255,255,255,0.8)'; c.lineWidth = 1; c.strokeRect(bx0, byb, bw, 5);
    c.font = '11px Segoe UI, sans-serif'; c.fillStyle = 'rgba(215,222,234,0.85)'; c.fillText('Batalla de ' + m.sis[bt.sis].nom + ' · día ' + Math.max(1, Math.round(m.t - bt.t0)) + (bt.ley === 'lineal' ? ' · entre asteroides' : '') + ' · la barra dice quién va ganando', x, y + R * 0.6 + 20);
    zonas.push({ x, y: y + R * 0.6 + 20, r: 14, ref: { t: 'S', id: bt.sis }, pri: 3.5 });
  };
  L.limpiar = function (m) { for (const [id] of L.vis) if (!m.bat[id] || !m.bat[id].vivo) L.vis.delete(id); const t = ahora(); L.fx = L.fx.filter(f => t - f.t0 < 4000); };
  L.efecto = function (m, e) { if (e.a < 0) return; if (e.k === 'bomba' || e.k === 'explosion' || e.k === 'sabotaje' || e.k === 'asteroide' || e.k === 'incendio' || e.k === 'masacre') L.fx.push({ a: e.a, ins: e.d && e.d.ins !== undefined ? e.d.ins : -1, t0: ahora(), k: e.k }); };

  // ════ Superficie de un asentamiento ════
  L.plano = function (m, a) {
    const clave = a.id + '|' + a.ins.length; let p = L.planos.get(a.id); if (p && p.clave === clave) return p;
    const r = new S.Rng(S.hash(m.semilla, 'plano', a.id)); p = { clave, par: [], casas: [], muelle: r.r(0, TAU), manchas: [] };
    let kc = 0;
    for (const id of a.ins) {
      const i = m.ins[id]; const tp = i.tipo; let s = tp === 'granja' ? Math.min(0.24, 0.13 + 0.03 * i.nivel) : tp === 'memorial' ? 0.035 : tp === 'palacio' ? 0.1 : 0.06 + 0.016 * Math.sqrt(i.nivel);
      let x = 0, y = 0;
      if (tp === 'palacio' || tp === 'templo' || tp === 'ceca' || tp === 'memorial') { const an = kc++ * 1.9 + 0.6; const rr = tp === 'memorial' ? 0.13 : 0.24; x = Math.cos(an) * rr; y = Math.sin(an) * rr; }
      else for (let k = 0; k < 90; k++) { const an = r.r(0, TAU), rr = Math.sqrt(r.r(0.1, 0.66)); x = Math.cos(an) * rr; y = Math.sin(an) * rr; let ok = true; for (const q of p.par) if (Math.hypot(q.x - x, q.y - y) < (q.s + s) * (k < 60 ? 1.05 : 0.7)) { ok = false; break; } if (ok) break; }
      p.par.push({ ins: id, x, y, s });
    }
    const nh = S.clamp(Math.round(Math.log10(Math.max(1000, a.pob)) * 26 - 62), 10, 96);
    for (let k = 0; k < nh; k++) { const an = r.r(0, TAU), rr = Math.sqrt(r.r(0.02, 0.72)); p.casas.push([Math.cos(an) * rr, Math.sin(an) * rr, r.r(0.012, 0.03), r.r(0.012, 0.03), r.r(0, 3)]); }
    for (let k = 0; k < 9; k++) p.manchas.push([r.r(-0.7, 0.7), r.r(-0.7, 0.7), r.r(0.12, 0.34), r.r(0, 3)]);
    L.planos.set(a.id, p); return p;
  };
  function humo(c, x, y, s, t, col) { for (let i = 0; i < 4; i++) { const e = ((t / 1600 + i * 0.25) % 1); c.fillStyle = (col || 'rgba(170,170,170,') + (0.35 * (1 - e)) + ')'; c.beginPath(); c.arc(x + Math.sin(e * 5 + i) * s * 0.2 + e * s * 0.5, y - e * s * 1.6, s * (0.18 + e * 0.35), 0, TAU); c.fill(); } }
  function fuego(c, x, y, s, t) { for (let i = 0; i < 5; i++) { const e = ((t / 500 + i * 0.2) % 1); c.fillStyle = 'rgba(255,' + Math.round(210 - 150 * e) + ',60,' + (0.8 * (1 - e)) + ')'; c.beginPath(); c.arc(x + (azar(i * 3.3) - 0.5) * s, y - e * s * 0.9, s * 0.28 * (1 - e * 0.5), 0, TAU); c.fill(); } }
  L.edificio = function (c, m, a, i, x, y, s, t, colEst) {
    const def = S.reg.inst[i.tipo]; const tp = i.tipo; const act = (i.ef || 0) > 0.3 && !i.huelga;
    if (i.salud < 0.05 && !def.indestructible) { c.fillStyle = '#1b1612'; c.beginPath(); c.arc(x, y, s * 0.9, 0, TAU); c.fill(); c.fillStyle = '#3a2f28'; for (let k = 0; k < 7; k++) c.fillRect(x + (azar(i.id + k) - 0.5) * s * 1.4, y + (azar(i.id * 2 + k) - 0.5) * s * 1.2, s * 0.3, s * 0.2); humo(c, x, y, s, t, 'rgba(60,55,50,'); return; }
    c.lineWidth = 1; c.strokeStyle = 'rgba(0,0,0,0.5)';
    if (tp === 'granja') { const mad = 1 - S.clamp(a.diasCosecha / S.ANIO, 0, 1); c.fillStyle = a.plaga > 0.05 ? '#6a5a30' : 'rgb(' + Math.round(70 + 150 * mad) + ',' + Math.round(140 + 30 * mad) + ',60)'; c.fillRect(x - s, y - s * 0.75, s * 2, s * 1.5); c.strokeStyle = 'rgba(0,0,0,0.25)'; for (let k = 1; k < 7; k++) { c.beginPath(); c.moveTo(x - s + k * s * 2 / 7, y - s * 0.75); c.lineTo(x - s + k * s * 2 / 7, y + s * 0.75); c.stroke(); } if (a.sem < a.semNec * 0.9) { c.fillStyle = 'rgba(40,30,20,0.55)'; c.fillRect(x - s, y - s * 0.75, s * 2 * (1 - a.sem / Math.max(1, a.semNec)), s * 1.5); } }
    else if (tp === 'hidroponia') { c.fillStyle = '#9fe0b0'; for (let k = 0; k < 3; k++) { c.beginPath(); c.rect(x - s * 0.8, y - s * 0.6 + k * s * 0.45, s * 1.6, s * 0.3); c.fill(); c.stroke(); } }
    else if (tp === 'mina') { c.fillStyle = '#3a2f26'; c.beginPath(); c.arc(x, y, s * 0.85, 0, TAU); c.fill(); c.strokeStyle = '#6a5a48'; for (let k = 1; k < 4; k++) { c.beginPath(); c.arc(x, y, s * 0.85 * k / 4, 0, TAU); c.stroke(); } c.strokeStyle = '#c0c8d0'; c.lineWidth = 1.5; c.beginPath(); c.moveTo(x - s * 0.3, y - s * 0.2); c.lineTo(x, y - s * 1.1); c.lineTo(x + s * 0.3, y - s * 0.2); c.stroke(); }
    else if (tp === 'refineria') { c.fillStyle = '#b8a890'; for (let k = 0; k < 3; k++) { c.beginPath(); c.arc(x - s * 0.5 + k * s * 0.5, y, s * 0.3, 0, TAU); c.fill(); c.stroke(); } c.strokeStyle = '#e2703a'; c.beginPath(); c.moveTo(x - s * 0.8, y + s * 0.5); c.lineTo(x + s * 0.8, y + s * 0.5); c.stroke(); if (act) fuego(c, x + s * 0.8, y - s * 0.3, s * 0.3, t); }
    else if (tp === 'astillero') { c.strokeStyle = '#9fb4d6'; c.lineWidth = 1.5; c.strokeRect(x - s, y - s * 0.5, s * 2, s); for (let k = 0; k < 5; k++) { c.beginPath(); c.moveTo(x - s + k * s * 0.5, y - s * 0.5); c.lineTo(x - s + k * s * 0.5, y + s * 0.5); c.stroke(); } const p0 = a.pedidos[0]; if (p0) { c.fillStyle = colEst; c.beginPath(); c.moveTo(x - s * 0.8, y - s * 0.22); c.lineTo(x - s * 0.8 + s * 1.6 * Math.min(1, p0.prog), y - s * 0.22); c.lineTo(x - s * 0.8 + s * 1.6 * Math.min(1, p0.prog), y + s * 0.22); c.lineTo(x - s * 0.8, y + s * 0.22); c.fill(); if (act && Math.floor(t / 180) % 3 === 0) { c.fillStyle = '#fff6c0'; c.fillRect(x - s * 0.8 + s * 1.6 * Math.min(1, p0.prog) - 1, y + (azar(t) - 0.5) * s * 0.4, 2, 2); } } }
    else if (tp === 'almacen') { const cap = Math.max(1, S.eco.cap(m, a)); let st = 0; for (let k = 0; k < S.NB; k++) st += a.alm[k]; const ll = S.clamp(st / cap, 0, 1); c.fillStyle = '#4a5466'; c.fillRect(x - s * 0.8, y - s * 0.6, s * 1.6, s * 1.2); c.stroke(); for (let k = 0; k < 12; k++) { c.fillStyle = k / 12 < ll ? '#c9a23e' : 'rgba(0,0,0,0.3)'; c.fillRect(x - s * 0.7 + (k % 4) * s * 0.36, y - s * 0.48 + Math.floor(k / 4) * s * 0.34, s * 0.28, s * 0.24); } }
    else if (tp === 'cuartel') { c.fillStyle = A.rgba(colEst.startsWith('#') ? colEst : '#59647a', 0.35); c.strokeStyle = colEst; c.lineWidth = 1.5; c.beginPath(); for (let k = 0; k < 5; k++) { const an = -Math.PI / 2 + k * TAU / 5; c.lineTo(x + Math.cos(an) * s, y + Math.sin(an) * s); const a2 = an + TAU / 10; c.lineTo(x + Math.cos(a2) * s * 0.55, y + Math.sin(a2) * s * 0.55); } c.closePath(); c.fill(); c.stroke(); }
    else if (tp === 'palacio') { c.fillStyle = '#d8d2c0'; c.fillRect(x - s, y - s * 0.5, s * 2, s); c.fillStyle = '#b8b09a'; for (let k = 0; k < 6; k++) c.fillRect(x - s * 0.9 + k * s * 0.34, y - s * 0.5, s * 0.12, s); c.fillStyle = '#8a8270'; c.beginPath(); c.moveTo(x - s * 1.05, y - s * 0.5); c.lineTo(x, y - s * 1.0); c.lineTo(x + s * 1.05, y - s * 0.5); c.fill(); if (a.est >= 0) A.banderaDe(c, x - s * 0.3, y - s * 1.55, s * 0.7, s * 0.45, m, 'E', a.est); }
    else if (tp === 'templo') { c.fillStyle = '#c8c0d8'; c.fillRect(x - s * 0.6, y - s * 0.3, s * 1.2, s * 0.9); c.beginPath(); c.moveTo(x - s * 0.3, y - s * 0.3); c.lineTo(x, y - s * 1.3); c.lineTo(x + s * 0.3, y - s * 0.3); c.fill(); }
    else if (tp === 'memorial') { c.fillStyle = '#b0b4bc'; c.beginPath(); c.moveTo(x - s * 0.5, y + s); c.lineTo(x, y - s * 1.6); c.lineTo(x + s * 0.5, y + s); c.fill(); c.fillStyle = '#ffd24f'; for (let k = 0; k < 4; k++) c.fillRect(x - s * 1.2 + k * s * 0.8, y + s * 1.1 + Math.sin(t / 300 + k) * 0.4, 1.5, 1.5); }
    else if (tp === 'ceca') { c.fillStyle = '#6a6250'; c.fillRect(x - s * 0.7, y - s * 0.5, s * 1.4, s); c.fillStyle = '#e0bc48'; c.beginPath(); c.arc(x, y, s * 0.3, 0, TAU); c.fill(); }
    else if (tp === 'policia') { c.fillStyle = '#2f4a78'; c.fillRect(x - s * 0.7, y - s * 0.5, s * 1.4, s); c.stroke(); c.strokeStyle = '#c0c8d0'; c.beginPath(); c.moveTo(x + s * 0.4, y - s * 0.5); c.lineTo(x + s * 0.4, y - s * 1.3); c.stroke(); c.fillStyle = Math.floor(t / 500) % 2 ? '#ff5a4f' : '#5a8aff'; c.fillRect(x + s * 0.4 - 1.5, y - s * 1.3 - 1.5, 3, 3); }
    else {
      // Fábricas: nave con tejado en diente de sierra y chimenea. El color dice qué hacen.
      const col = tp === 'fundicion' ? '#7a5a48' : tp === 'armeria' ? '#6a3a3a' : tp === 'fab_nucleos' ? '#5a6a4a' : tp === 'fab_filtros' ? '#4a6a6a' : '#5a6070';
      c.fillStyle = col; c.fillRect(x - s * 0.9, y - s * 0.35, s * 1.8, s * 0.9); c.stroke(); c.fillStyle = A.tono(col, 0.2); c.beginPath(); for (let k = 0; k < 4; k++) { c.moveTo(x - s * 0.9 + k * s * 0.45, y - s * 0.35); c.lineTo(x - s * 0.9 + k * s * 0.45, y - s * 0.7); c.lineTo(x - s * 0.9 + (k + 1) * s * 0.45, y - s * 0.35); } c.fill();
      c.fillStyle = '#3a3f4a'; c.fillRect(x + s * 0.55, y - s * 1.2, s * 0.2, s * 0.85);
      if (act) humo(c, x + s * 0.65, y - s * 1.2, s * 0.8, t);
      if (tp === 'fundicion' && act) { c.fillStyle = 'rgba(255,150,60,' + (0.5 + 0.3 * Math.sin(t / 200)) + ')'; c.fillRect(x - s * 0.5, y - s * 0.05, s * 0.5, s * 0.3); }
      if (tp === 'fab_nucleos') { c.fillStyle = '#e0c020'; c.font = Math.round(s * 0.8) + 'px Segoe UI Emoji'; c.textAlign = 'center'; c.fillText('☢', x - s * 0.3, y + s * 0.4); }
    }
    if (i.salud < 1 && !def.indestructible) { if (i.salud < 0.6) fuego(c, x - s * 0.3, y, s * 0.5, t); c.fillStyle = '#400'; c.fillRect(x - s * 0.8, y + s * 0.8, s * 1.6, 2.5); c.fillStyle = i.salud > 0.5 ? '#5cc28a' : '#e0564f'; c.fillRect(x - s * 0.8, y + s * 0.8, s * 1.6 * i.salud, 2.5); }
    if (i.huelga) { for (let k = 0; k < 6; k++) { c.fillStyle = '#ffdf70'; c.fillRect(x - s * 0.8 + k * s * 0.3, y + s * 0.95 + Math.sin(t / 250 + k) * 1.2, 2, 2); } c.fillStyle = '#c0392b'; c.fillRect(x - s * 0.2, y + s * 0.7, s * 0.5, s * 0.22); }
  };
  L.asentamiento = function (c, m, a, x, y, R, zonas, tSim) {
    const t = ahora(); const p = L.plano(m, a); const e = a.est >= 0 ? m.est[a.est] : null; const colEst = e ? e.col : '#8a94a8'; const u = a.uni >= 0 ? m.uni[a.uni] : null;
    c.save(); c.beginPath(); c.arc(x, y, R, 0, TAU); c.clip();
    // Suelo.
    if (a.forma === 'planeta') { const gr = c.createRadialGradient(x - R * 0.3, y - R * 0.3, R * 0.1, x, y, R); const ag = a.tipo === 'agricola'; gr.addColorStop(0, ag ? '#4a6a3a' : '#5a5448'); gr.addColorStop(1, ag ? '#22361e' : '#2a2824'); c.fillStyle = gr; c.fillRect(x - R, y - R, R * 2, R * 2); for (const [mx, my, mr, k] of p.manchas) { c.fillStyle = k < 1 ? 'rgba(40,70,110,0.5)' : k < 2 ? 'rgba(90,110,70,0.35)' : 'rgba(110,100,80,0.3)'; c.beginPath(); c.ellipse(x + mx * R, y + my * R, mr * R, mr * R * 0.7, k, 0, TAU); c.fill(); } }
    else if (a.forma === 'luna') { c.fillStyle = '#4a4c52'; c.fillRect(x - R, y - R, R * 2, R * 2); for (const [mx, my, mr] of p.manchas) { c.strokeStyle = 'rgba(0,0,0,0.3)'; c.fillStyle = 'rgba(30,30,34,0.35)'; c.beginPath(); c.arc(x + mx * R, y + my * R, mr * R * 0.5, 0, TAU); c.fill(); c.stroke(); } }
    else { c.fillStyle = a.forma === 'cinturon' ? '#3a322a' : '#1f2634'; c.fillRect(x - R, y - R, R * 2, R * 2); c.strokeStyle = 'rgba(140,160,200,0.22)'; c.lineWidth = 1; for (let k = 1; k <= 4; k++) { c.beginPath(); c.arc(x, y, R * k / 4.2, 0, TAU); c.stroke(); } for (let k = 0; k < 8; k++) { c.beginPath(); c.moveTo(x, y); c.lineTo(x + Math.cos(k * TAU / 8) * R, y + Math.sin(k * TAU / 8) * R); c.stroke(); } }
    // Calles y casas.
    c.strokeStyle = 'rgba(210,215,225,0.16)'; c.lineWidth = Math.max(1, R / 110);
    for (const q of p.par) { c.beginPath(); c.moveTo(x, y); c.lineTo(x + q.x * R, y + q.y * R); c.stroke(); }
    c.beginPath(); c.arc(x, y, R * 0.47, 0, TAU); c.stroke();
    for (const [hx, hy, hw, hh, k] of p.casas) { c.fillStyle = a.apagon ? 'rgba(90,96,110,0.6)' : k < 1 ? 'rgba(190,196,210,0.7)' : k < 2 ? 'rgba(160,150,140,0.7)' : 'rgba(140,156,176,0.7)'; c.fillRect(x + hx * R - hw * R, y + hy * R - hh * R, hw * R * 2, hh * R * 2); if (!a.apagon && azar(hx * 9 + Math.floor(t / 2500)) > 0.6) { c.fillStyle = 'rgba(255,230,150,0.8)'; c.fillRect(x + hx * R - 1, y + hy * R - 1, 2, 2); } }
    // Edificios.
    c.textAlign = 'center';
    for (const q of p.par) {
      const i = m.ins[q.ins]; const px = x + q.x * R, py = y + q.y * R, s = q.s * R; q.px = px; q.py = py;
      L.edificio(c, m, a, i, px, py, s, t, colEst);
      zonas.push({ x: px, y: py, r: Math.max(8, s * 0.9), ref: { t: 'I', id: i.id }, pri: 4 });
      if (R > 210 && i.tipo !== 'memorial') { c.font = '9px Segoe UI, sans-serif'; c.fillStyle = 'rgba(230,236,246,0.75)'; c.fillText(S.reg.inst[i.tipo].nom, px, py + s + 11); }
    }
    const parDe = (tipo) => p.par.find(q => m.ins[q.ins].tipo === tipo);
    // Plaza: la gente que está en la calle, de verdad en la calle.
    c.fillStyle = 'rgba(200,205,215,0.18)'; c.beginPath(); c.arc(x, y, R * 0.09, 0, TAU); c.fill();
    const nc = Math.round(Math.min(1, a.f / 0.63) * 300 * (a.f > 0.05 ? 1 : 0));
    for (let k = 0; k < nc; k++) { const an = azar(k * 1.7 + a.id) * TAU, rr = Math.sqrt(azar(k * 3.1 + a.id)) * R * (0.06 + 0.16 * Math.min(1, a.f / 0.63)); c.fillStyle = k % 7 ? 'rgba(255,120,200,0.9)' : '#ffe0f0'; c.fillRect(x + Math.cos(an) * rr + Math.sin(t / 300 + k) * 1.2, y + Math.sin(an) * rr + Math.cos(t / 340 + k * 1.3) * 1.2, 2, 2); }
    if (nc > 30) { const f = m.fac.find(ff => ff.vivo && ff.etapa === 'inst' && ff.tipo !== 'casa' && (ff.sede === a.id || ff.cel.some(cc => cc.ase === a.id))); for (let k = 0; k < 3; k++) { const bx = x + (k - 1) * R * 0.07, by = y - R * 0.05 + Math.sin(t / 400 + k) * 2; c.strokeStyle = '#ddd'; c.lineWidth = 1; c.beginPath(); c.moveTo(bx, by + 10); c.lineTo(bx, by - 2); c.stroke(); if (f) A.banderaDe(c, bx, by - 3, 11, 7, m, 'F', f.id); else { c.fillStyle = '#c0392b'; c.fillRect(bx, by - 3, 10, 6); } } }
    // Soldados: junto al cuartel, o en línea frente a la plaza si hay gente en la calle.
    if (u) {
      const qc = parDe('cuartel') || parDe('policia'); const sx0 = qc ? qc.px : x + R * 0.3, sy0 = qc ? qc.py : y; const ns = S.clamp(Math.round(u.n / 60), 4, 90); const enLinea = a.f > 0.2 || (a.terror || 0) > 0.15;
      const dx = x - sx0, dy = y - sy0, d = Math.hypot(dx, dy) || 1;
      for (let k = 0; k < ns; k++) {
        let px, py; if (enLinea) { const lat = (k - ns / 2) * 3.2; px = x - dx / d * R * 0.2 + (-dy / d) * lat; py = y - dy / d * R * 0.2 + (dx / d) * lat; } else { px = sx0 + (azar(k * 2.3 + u.id) - 0.5) * R * 0.14; py = sy0 + (azar(k * 5.1 + u.id) - 0.5) * R * 0.14 + R * 0.05; }
        c.fillStyle = u.msc >= 3 ? '#8a8a6a' : colEst; c.fillRect(px - 1.2, py - 1.2, 2.6, 2.6); c.strokeStyle = 'rgba(0,0,0,0.6)'; c.strokeRect(px - 1.2, py - 1.2, 2.6, 2.6);
        if ((a.terror || 0) > 0.28 && azar(k + Math.floor(t / 110)) > 0.8) { c.strokeStyle = 'rgba(255,230,120,0.9)'; c.beginPath(); c.moveTo(px, py); c.lineTo(px + dx / d * R * 0.12, py + dy / d * R * 0.12); c.stroke(); }
      }
    }
    // Colas en el reparto.
    if (a.H > 0.1) { const qa = parDe('almacen'); if (qa) { const nq = Math.round(a.H * 70); for (let k = 0; k < nq; k++) { c.fillStyle = 'rgba(230,200,150,0.9)'; c.fillRect(qa.px + qa.s * R * 0.9 + k * 2.6, qa.py + qa.s * R * 0.5 + Math.sin(k * 0.7) * 3, 1.8, 1.8); } } }
    // Gente: las mismas caras de siempre, yendo de un sitio a otro.
    if (!L.gente || L.gente.a !== a.id || t - L.gente.t > 4000) L.gente = { a: a.id, t, l: S.per.caras(m, a, 44) };
    const np = p.par.length;
    for (const ca of L.gente.l) {
      const qa = p.par[Math.floor(ca.u * np) % np], qb = p.par[Math.floor(ca.rad * np + 1) % np]; if (!qa || !qb) continue;
      let w = ((t / (9000 + ca.ang * 2500) + ca.ang) % 2); if (w > 1) w = 2 - w; const viaCentro = w < 0.5 ? w * 2 : (w - 0.5) * 2;
      const px = w < 0.5 ? qa.px + (x - qa.px) * viaCentro : x + (qb.px - x) * viaCentro, py = w < 0.5 ? qa.py + (y - qa.py) * viaCentro : y + (qb.py - y) * viaCentro;
      c.fillStyle = ca.ficha >= 0 ? '#ffe9b0' : ca.odia ? '#ff7a6e' : '#bcd0f0'; c.beginPath(); c.arc(px, py, ca.ficha >= 0 ? 3 : 2.1, 0, TAU); c.fill();
      zonas.push({ x: px, y: py, r: 6, ref: { t: 'C', id: ca.i, a: a.id, cara: ca }, pri: 5 });
    }
    // Combate en tierra: los que desembarcan vienen del muelle; los que defienden, del centro.
    if (a.tie >= 0 && m.tie[a.tie] && m.tie[a.tie].vivo) L.tierra(c, m, a, m.tie[a.tie], x, y, R, t, p);
    // Explosiones recientes.
    for (const f of L.fx) { if (f.a !== a.id) continue; const e2 = (t - f.t0) / 2600; if (e2 >= 1) continue; const q = f.ins >= 0 ? p.par.find(z => z.ins === f.ins) : null; const fx = q ? q.px : x, fy = q ? q.py : y; c.fillStyle = 'rgba(255,' + Math.round(240 - 180 * e2) + ',' + Math.round(180 - 180 * e2) + ',' + (1 - e2) + ')'; c.beginPath(); c.arc(fx, fy, R * (0.03 + e2 * 0.16), 0, TAU); c.fill(); c.strokeStyle = 'rgba(255,255,255,' + (1 - e2) + ')'; c.lineWidth = 2; c.beginPath(); c.arc(fx, fy, R * e2 * 0.45, 0, TAU); c.stroke(); }
    c.restore();
    c.strokeStyle = colEst; c.lineWidth = 2; c.beginPath(); c.arc(x, y, R, 0, TAU); c.stroke();
    // Muelles: las naves que están atracadas, cada una en su sitio.
    const an0 = p.muelle; const naves = []; for (const n of m.nav) if (n.vivo && n.en === a.id) { naves.push(n); if (naves.length >= 26) break; }
    c.strokeStyle = 'rgba(160,180,220,0.6)'; c.lineWidth = 2; for (let k = -1; k <= 1; k++) { const an = an0 + k * 0.42; c.beginPath(); c.moveTo(x + Math.cos(an) * R, y + Math.sin(an) * R); c.lineTo(x + Math.cos(an) * R * 1.3, y + Math.sin(an) * R * 1.3); c.stroke(); }
    naves.forEach((n, k) => {
      const an = an0 + ((k % 3) - 1) * 0.42; const rr = R * (1.07 + Math.floor(k / 3) * 0.035) + (k % 2 ? 7 : -7) * 0; const lado = Math.floor(k / 3) % 2 ? 1 : -1;
      const px = x + Math.cos(an) * rr - Math.sin(an) * lado * 9, py = y + Math.sin(an) * rr + Math.cos(an) * lado * 9; const gu = S.reg.nave[n.cls].guerra;
      const col = n.pirata ? '#ff5a4f' : n.dueno.t === 'E' && m.est[n.dueno.id] ? m.est[n.dueno.id].col : S.CASCOS[n.col].hex;
      c.save(); c.translate(px, py); c.rotate(an + Math.PI / 2 * lado); A.naveMini(c, n.cls, gu ? 7 + Math.log10(n.cascos + 1) * 3 : 5.5, col, n.st === 'amarrada' ? '#555' : 'rgba(255,255,255,0.4)'); c.restore();
      zonas.push({ x: px, y: py, r: 8, ref: { t: 'N', id: n.id }, pri: 4.5 });
      if (gu && n.cascos > 1) { c.font = '9px Segoe UI'; c.fillStyle = '#fff'; c.textAlign = 'center'; c.fillText(n.cascos, px, py + 3); }
    });
    if (a.feria > tSim) { for (let k = 0; k < 9; k++) { const an = an0 + 0.8 + k * 0.07; c.fillStyle = ['#e0564f', '#e0b84c', '#5cc28a', '#5fb0c9', '#b98fd6'][k % 5]; c.beginPath(); c.moveTo(x + Math.cos(an) * R * 0.99, y + Math.sin(an) * R * 0.99); c.lineTo(x + Math.cos(an + 0.03) * R * 1.06, y + Math.sin(an + 0.03) * R * 1.06); c.lineTo(x + Math.cos(an + 0.06) * R * 0.99, y + Math.sin(an + 0.06) * R * 0.99); c.fill(); } }
    c.font = 'bold 13px Segoe UI, sans-serif'; c.textAlign = 'center'; c.fillStyle = '#fff'; c.fillText(a.nom + ' · ' + Math.round(a.pob).toLocaleString('es-ES') + ' hab.', x, y + R + 18);
    let est = ''; if (a.hambruna >= 0) est += ' 🥣 hambre'; if (a.f > 0.3) est += ' 🔥 ' + Math.round(a.f * 100) + ' % en la calle'; if (a.huelga > tSim) est += ' ✊ huelga'; if (a.plaga > 0.05) est += ' 🐛 plaga'; if (a.feria > tSim) est += ' 🎪 feria'; if (a.tie >= 0) est += ' ⚔ combates';
    if (est) { c.font = '12px Segoe UI Emoji, Segoe UI, sans-serif'; c.fillStyle = '#ffd0c8'; c.fillText(est, x, y + R + 34); }
    if (a.tie >= 0 && m.tie[a.tie] && m.tie[a.tie].vivo) L.leyendaTierra(c, m, m.tie[a.tie], x, y + R + 54);
  };
  L.tierra = function (c, m, a, tr, x, y, R, t, p) {
    const prog = 1 - S.clamp(tr.def.n / Math.max(1, tr.def.n0), 0, 1); const an0 = p.muelle; const [colA, colD] = L.colTierra(m, tr);
    const frente = R * (0.82 - 0.62 * prog); const fx0 = x + Math.cos(an0) * frente * 0.5, fy0 = y + Math.sin(an0) * frente * 0.5;
    const nA = S.clamp(Math.round(tr.atk.n / 55), 6, 230), nD = S.clamp(Math.round(tr.def.n / 55), 6, 230); const ptsA = [], ptsD = [];
    for (let k = 0; k < nA; k++) { const lat = (azar(k * 1.9 + tr.id) - 0.5) * R * 1.1, prof = azar(k * 4.3 + tr.id) * R * 0.3; const px = x + Math.cos(an0) * (frente + prof) - Math.sin(an0) * lat + Math.sin(t / 400 + k), py = y + Math.sin(an0) * (frente + prof) + Math.cos(an0) * lat + Math.cos(t / 450 + k); if (Math.hypot(px - x, py - y) > R * 0.98) continue; c.fillStyle = colA; c.fillRect(px - 1.3, py - 1.3, 2.8, 2.8); c.strokeStyle = '#000'; c.lineWidth = 0.5; c.strokeRect(px - 1.3, py - 1.3, 2.8, 2.8); ptsA.push(px, py); }
    for (let k = 0; k < nD; k++) { const lat = (azar(k * 2.7 + tr.id * 3) - 0.5) * R * 1.0, prof = azar(k * 3.9 + tr.id * 5) * R * 0.25; const px = x + Math.cos(an0) * (frente - R * 0.09 - prof) - Math.sin(an0) * lat, py = y + Math.sin(an0) * (frente - R * 0.09 - prof) + Math.cos(an0) * lat; if (Math.hypot(px - x, py - y) > R * 0.98) continue; c.fillStyle = colD; c.fillRect(px - 1.3, py - 1.3, 2.8, 2.8); c.strokeStyle = '#fff'; c.lineWidth = 0.5; c.strokeRect(px - 1.3, py - 1.3, 2.8, 2.8); ptsD.push(px, py); }
    const q = Math.floor(t / 90);
    for (let i = 0; i < 26 && ptsA.length && ptsD.length; i++) { const a0 = Math.floor(azar(q + i * 2.1) * ptsA.length / 2) * 2, d0 = Math.floor(azar(q * 1.3 + i * 3.7) * ptsD.length / 2) * 2; c.strokeStyle = i % 2 ? 'rgba(255,230,130,0.8)' : 'rgba(255,140,110,0.7)'; c.lineWidth = 1; c.beginPath(); c.moveTo(ptsA[a0], ptsA[a0 + 1]); c.lineTo(ptsD[d0], ptsD[d0 + 1]); c.stroke(); }
    for (let i = 0; i < 5; i++) { const e = ((t / 700 + i * 0.2) % 1); const bx = fx0 + (azar(Math.floor(t / 700 + i * 0.2) * 3.3 + i) - 0.5) * R * 0.9, by = fy0 + (azar(Math.floor(t / 700 + i * 0.2) * 7.1 + i) - 0.5) * R * 0.5; c.fillStyle = 'rgba(255,' + Math.round(220 - 150 * e) + ',90,' + (1 - e) + ')'; c.beginPath(); c.arc(bx, by, 3 + e * 9, 0, TAU); c.fill(); }
  };
  // Leyenda del combate (fuera del disco): cada bando con su color, quién es y cuántos le quedan.
  L.leyendaTierra = function (c, m, tr, x, y) {
    const [colA, colD] = L.colTierra(m, tr); const asalto = tr.tipo === 'asalto';
    const filas = [[colA, (asalto ? 'Asaltantes' : 'Insurrectos') + ' · ' + tr.atk.nom, tr.atk.n, tr.atk.n0], [colD, 'Defensores · ' + tr.def.nom, tr.def.n, tr.def.n0]];
    c.font = 'bold 12px Segoe UI, sans-serif'; c.textAlign = 'left';
    filas.forEach((f, i) => { const txt = Math.round(f[2]).toLocaleString('es-ES') + ' de ' + Math.round(f[3]).toLocaleString('es-ES') + ' · ' + f[1]; const w = c.measureText(txt).width + 16; const lx = x - w / 2, ly = y + i * 17; c.fillStyle = 'rgba(7,10,18,0.75)'; c.fillRect(lx - 5, ly - 12, w + 10, 16); c.fillStyle = f[0]; c.fillRect(lx, ly - 9, 10, 10); c.strokeStyle = '#fff'; c.lineWidth = 0.6; c.strokeRect(lx, ly - 9, 10, 10); c.fillStyle = '#fff'; c.fillText(txt, lx + 15, ly); });
    c.textAlign = 'center';
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
