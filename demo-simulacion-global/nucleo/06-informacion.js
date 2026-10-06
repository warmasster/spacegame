// Información: no hay internet galáctico. Cada hecho relevante crea un paquete que solo se mueve si alguien
// lo lleva; dentro de un sistema la radio lo reparte al momento. Los rumores se inflan al contarse.
// El correo (cartas con destinatario) viaja en sacas físicas: si la nave no llega, la carta tampoco.
(function (g) {
  'use strict';
  const S = g.SIM;
  const I = S.inf = {};
  I.QUIEN = ['un piloto', 'un mercenario', 'un capitán corsario', 'un almirante renegado'];
  I.MU = 0.15; I.SD = 0.35;                       // x' = x·e^ε, ε ~ N(0,15; 0,35²)

  I.crear = function (m, ev, k, a, imp, x, d) {
    const p = { id: m.paq.length, ev, k, t: m.t, a, s: m.ase[a].sis, imp, x: x || 0, d: d || null };
    m.paq.push(p);
    I.oir(m, m.ase[a], p, { x: p.x, q: 0, n: 0 });
    return p;
  };
  // Un asentamiento se entera. Devuelve true si era nuevo.
  I.oir = function (m, a, p, ver, porRadio) {
    const ya = a.not.get(p.id);
    if (ya) { if (!porRadio) ya.f++; return false; }
    const v = { x: ver.x, q: ver.q, n: ver.n, t: m.t, f: 1 };
    a.not.set(p.id, v); (a.rec = a.rec || []).push(p.id); if (a.rec.length > 48) a.rec.shift();
    S.emitir(m, 'noticia.' + p.k, a, p, v); S.emitir(m, 'noticia', a, p, v);
    if (!porRadio) { const s = m.sis[a.sis]; for (let i = 0; i < s.ase.length; i++) if (s.ase[i] !== a.id) I.oir(m, m.ase[s.ase[i]], p, v, true); }
    return true;
  };
  I.contada = function (ver, rng) {
    return { x: ver.x > 0 ? ver.x * Math.exp(rng.n(I.MU, I.SD)) : ver.x, q: Math.min(3, ver.q + (rng.p(0.25) ? 1 : 0)), n: ver.n + 1 };
  };
  // confianza = 1 − Π(1 − r_i), con rumores de fiabilidad 0,5.
  I.confianza = (ver, r) => 1 - Math.pow(1 - (r || 0.5), ver.f || 1);

  // Una tripulación cuenta en el muelle lo que sabe (y se lleva lo que oye).
  I.atracar = function (m, n, a, hablador) {
    const rng = n.rng;
    for (const [id, ver] of n.not) {
      if (a.not.has(id)) { a.not.get(id).f++; continue; }
      const p = m.paq[id]; if (m.t - p.t > 300) continue;
      if (rng.p(S.clamp(p.imp * (0.5 + hablador), 0.05, 1))) I.oir(m, a, p, I.contada(ver, rng));
    }
    // Del muelle se lleva lo último que se cuenta allí.
    const rec = a.rec; if (rec) for (let i = 0; i < rec.length; i++) {
      const id = rec[i]; if (n.not.has(id)) continue;
      const p = m.paq[id]; if (m.t - p.t > 200) continue;
      if (rng.p(S.clamp(p.imp * 1.2, 0.05, 1))) n.not.set(id, I.contada(a.not.get(id), rng));
    }
    if (n.not.size > 70) { const ids = Array.from(n.not.keys()).sort((x, y) => x - y); for (let i = 0; i < ids.length - 50; i++) n.not.delete(ids[i]); }
  };
  // El hambre es noticia: quien la oye sabe que allí el grano se paga a precio de oro (aunque nadie haya vuelto de allí).
  S.gancho('hambruna', function (m, a, ev) { I.crear(m, ev, 'hambruna', a.id, 0.75, Math.round(a.H * 100), { a: a.id, T: S.eco.objetivo(a, S.B.grano) }); });
  S.gancho('noticia.hambruna', function (m, a, p) {
    if (a.id === p.d.a) return; const viejo = a.tabla.get(p.d.a); if (viejo && viejo.t >= p.t) return;
    const e = viejo ? { t: p.t, p: viejo.p.slice(), T: viejo.T.slice(), x: viejo.x } : { t: p.t, p: S.bien.map(b => b.pref).concat([S.eco.P_NUCLEO]), T: new Array(S.NB).fill(0), x: new Array(S.NB).fill(0) };
    e.p[S.B.grano] = S.bien[S.B.grano].pref * 6; e.T[S.B.grano] = Math.max(e.T[S.B.grano], p.d.T); a.tabla.set(p.d.a, e);
  });
  S.gancho('asent.mes', function (m, a) {
    if (a.not.size < 150) return;
    for (const [id] of a.not) if (m.t - m.paq[id].t > 500) a.not.delete(id);
  });

  // Tablón de precios: lo que aquí se sabe de los precios de otros sitios, con su fecha.
  I.anotarPrecios = function (m, a) {
    // Cada anotación es un objeto nuevo: las copias que viajan en las naves no cambian después.
    const e = { t: m.t, p: new Array(S.NB + 1), T: new Array(S.NB), x: new Array(S.NB) };
    for (let c = 0; c < S.NB; c++) { e.p[c] = a.pr[c]; e.T[c] = S.eco.objetivo(a, c); e.x[c] = a.prod[c]; }
    e.p[S.NB] = S.eco.precioNucleo(a); a.tabla.set(a.id, e);
    const s = m.sis[a.sis]; for (let i = 0; i < s.ase.length; i++) if (s.ase[i] !== a.id) m.ase[s.ase[i]].tabla.set(a.id, e);   // por radio, al momento
  };
  I.mezclar = function (x, y) { // el dato más reciente gana, en los dos sentidos
    for (const [id, e] of x) { const o = y.get(id); if (!o || o.t < e.t) y.set(id, e); }
    for (const [id, e] of y) { const o = x.get(id); if (!o || o.t < e.t) x.set(id, e); }
  };

  // ── Correo.
  const C = S.cor = {};
  C.enviar = function (m, de, para, k, d, causa, oficial) {
    const c = { id: m.cartas++, k, de, para, t: m.t, d: d || {}, c: causa === undefined ? -1 : causa, of: oficial === undefined ? -1 : oficial };
    if (m.ase[de].sis === m.ase[para].sis) C.entregar(m, c);
    else m.ase[de].buzon.push(c);
    return c;
  };
  C.entregar = function (m, c) { c.llega = m.t; const def = S.reg.carta && S.reg.carta[c.k]; if (def) def.llega(m, c, m.ase[c.para]); };
  C.puedeLlevar = function (n, c) { return c.of < 0 || (n.dueno.t === 'E' && n.dueno.id === c.of); };
  // Al atracar: entrega lo que venía para este sistema.
  C.descargar = function (m, n, a) {
    if (!n.saca.length) return;
    const q = [];
    for (const c of n.saca) { if (m.ase[c.para].sis === a.sis) C.entregar(m, c); else q.push(c); }
    n.saca = q;
  };
  // Al zarpar hacia `dest`: deja en el buzón lo que no se acerca y recoge lo que sí.
  C.cargar = function (m, n, a, dest) {
    const D = m.dist; const sd = m.ase[dest].sis;
    if (n.saca.length) { const q = []; for (const c of n.saca) { const sp = m.ase[c.para].sis; if (D[sd][sp] < D[a.sis][sp] - 1) q.push(c); else a.buzon.push(c); } n.saca = q; }
    if (a.buzon.length) { const q = []; for (const c of a.buzon) { const sp = m.ase[c.para].sis; if (C.puedeLlevar(n, c) && D[sd][sp] < D[a.sis][sp] - 1 && n.saca.length < 60) n.saca.push(c); else q.push(c); } a.buzon = q; }
  };
  C.urgencia = function (m, a, n) { // a qué asentamiento conviene llevar el correo que hay aquí
    const u = new Map();
    for (const c of a.buzon) if (C.puedeLlevar(n, c)) u.set(c.para, (u.get(c.para) || 0) + 1 + (c.d.urg || 0) + (m.t - c.t) * 0.05);
    for (const c of n.saca) u.set(c.para, (u.get(c.para) || 0) + 1 + (c.d.urg || 0) + (m.t - c.t) * 0.05);
    let best = -1, bu = 0; for (const [p, x] of u) if (x > bu) { bu = x; best = p; }
    return best;
  };

  S.sismografo('paquetes', 'Noticias creadas', m => m.paq.length);
})(typeof globalThis !== 'undefined' ? globalThis : this);
