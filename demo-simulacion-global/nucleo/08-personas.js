// Personas con nombre (L2): rasgos, ideología, relaciones, recuerdos que se apagan, decisión por utilidad.
// Y las caras (L3): individuos que salen de una cohorte al acercarse, siempre los mismos y siempre cuadrando.
(function (g) {
  'use strict';
  const S = g.SIM; const R = S.R;
  const P = S.per = {};

  P.nombre = function (rng, sexo, apellido) {
    return rng.el(sexo === 'M' ? S.NOM.mujer : S.NOM.hombre) + ' ' + (apellido || rng.el(S.NOM.apellido));
  };
  P.apellido = (p) => p.nom.slice(p.nom.indexOf(' ') + 1);

  P.crear = function (m, o) {
    const id = m.per.length; const rng = m.rngDe('P', id);
    const sexo = o.sexo || (rng.p(0.5) ? 'M' : 'H');
    const p = {
      id, sexo, nom: o.nom || P.nombre(rng, sexo, o.apellido), nace: o.nace !== undefined ? o.nace : m.t - rng.r(24, 62) * S.ANIO,
      vivo: true, en: o.en || { t: 'A', id: o.casa }, casa: o.casa, rol: o.rol || 'civil', est: o.est === undefined ? -1 : o.est, fac: -1,
      r: [], car: rng.t01(0.4, 0.22), hab: rng.t01(0.5, 0.2), ideo: [], rel: new Map(), mem: [], din: o.din || rng.r(200, 3000), inv: [],
      fam: { con: -1, hijos: [], padres: [] }, cargo: o.cargo || null, ven: null, rng, not: o.not || 0,
    };
    for (let i = 0; i < 8; i++) p.r.push(rng.t01());
    const base = o.ideo; for (let i = 0; i < 5; i++) p.ideo.push(S.clamp((base ? base[i] : 0) + rng.n(0, base ? 0.3 : 0.5), -1, 1));
    if (o.r) for (const k in o.r) p.r[R[k]] = o.r[k];
    if (o.car !== undefined) p.car = o.car;
    m.per.push(p);
    m.prog(m.t + rng.r(1, 30), 'pers.tic', { p: id });
    return p;
  };
  P.edad = (m, p) => (m.t - p.nace) / S.ANIO;
  P.asent = function (m, p) { if (p.en.t === 'A') return p.en.id; if (p.en.t === 'V') return -1; const n = m.nav[p.en.id]; return n ? n.en : -1; };
  P.lugar = function (m, p) { if (p.en.t === 'V') return p.en.de; const a = P.asent(m, p); return a >= 0 ? a : p.casa; };

  P.rel = function (p, id) { let r = p.rel.get(id); if (!r) { if (p.rel.size >= 64) return { op: 0, conf: 0.2, miedo: 0, deuda: 0 }; r = { op: 0, conf: 0.3, miedo: 0, deuda: 0 }; p.rel.set(id, r); } return r; };
  P.lazo = function (a, b, op, conf) { const x = P.rel(a, b.id), y = P.rel(b, a.id); x.op = y.op = op; x.conf = y.conf = conf; };

  // Recuerdo: s(t) = s0·e^(−t/τ), τ = τ0·(1 + 2·rasgo).
  P.recordar = function (m, p, ev, quien, emo, s0, rasgo, tau0) {
    if (p.mem.length >= 64) { let k = 0, mn = Infinity; for (let i = 0; i < p.mem.length; i++) { const s = P.intensidad(m, p.mem[i]); if (s < mn) { mn = s; k = i; } } p.mem.splice(k, 1); }
    const x = { ev, q: quien, emo, s0, t: m.t, tau: (tau0 || 150) * (1 + 2 * (rasgo === undefined ? 0.5 : rasgo)) };
    p.mem.push(x); return x;
  };
  P.intensidad = (m, x) => Math.min(1, x.s0 * Math.exp(-(m.t - x.t) / x.tau));

  // Decisión por utilidad con softmax: P(a) = e^(U/T) / Σ e^(U/T).
  P.decidir = function (p, us, T) { return p.rng.soft(us, T === undefined ? 0.15 : T); };

  P.familia = function (m, p) {
    if (p.fam.hecha) return; p.fam.hecha = true;
    const ed = P.edad(m, p); if (ed < 20) return;
    const rng = p.rng; const ap = P.apellido(p);
    if (p.fam.con < 0 && rng.p(0.75)) {
      const c = P.crear(m, { casa: p.casa, est: p.est, sexo: p.sexo === 'M' ? (rng.p(0.9) ? 'H' : 'M') : (rng.p(0.9) ? 'M' : 'H'), nace: p.nace + rng.r(-6, 6) * S.ANIO, ideo: p.ideo });
      p.fam.con = c.id; c.fam.con = p.id; c.fam.hecha = true; P.lazo(p, c, 0.8, 0.9);
    }
    const nh = rng.i(4);
    for (let i = 0; i < nh; i++) {
      const edH = rng.r(1, Math.max(2, ed - 19));
      const h = P.crear(m, { casa: p.casa, est: p.est, apellido: ap, nace: m.t - edH * S.ANIO, ideo: p.ideo });
      h.fam.padres.push(p.id); if (p.fam.con >= 0) { h.fam.padres.push(p.fam.con); m.per[p.fam.con].fam.hijos.push(h.id); }
      p.fam.hijos.push(h.id); P.lazo(p, h, 0.8, 0.9);
    }
  };
  P.allegados = function (m, p) {
    const out = [];
    if (p.fam.con >= 0 && m.per[p.fam.con].vivo) out.push([m.per[p.fam.con], 1]);
    for (const h of p.fam.hijos) if (m.per[h].vivo) out.push([m.per[h], 0.6]);
    for (const h of p.fam.padres) if (m.per[h].vivo) out.push([m.per[h], 0.6]);
    return out;
  };

  const MODO = {
    natural: (m, p) => '{P' + p.id + '} muere a los ' + Math.floor(P.edad(m, p)) + ' años',
    publico: (m, p) => '{P' + p.id + '} cae muerto a la vista de todos',
    silencio: (m, p) => '{P' + p.id + '} aparece muerto; el médico firma «infarto»',
    combate: (m, p) => '{P' + p.id + '} muere en combate',
    ejecucion: (m, p) => '{P' + p.id + '} es ejecutado',
    explosion: (m, p) => '{P' + p.id + '} muere en la explosión',
    venganza: (m, p) => '{P' + p.id + '} es muerto por venganza',
    purga: (m, p) => '{P' + p.id + '} desaparece en la purga',
    catador: (m, p) => '{P' + p.id + '}, catador, muere envenenado en lugar de su señor',
  };
  P.matar = function (m, p, o) {
    if (!p.vivo) return -1;
    o = o || {}; p.vivo = false; p.muere = m.t;
    const a = P.lugar(m, p);
    const gob = p.cargo && p.cargo.t === 'gobernante';
    let txt = o.txt || (MODO[o.modo] || MODO.natural)(m, p);
    if (p.cargo && p.cargo.nom) txt += ' (' + p.cargo.nom + ')';
    const ev = m.reg('muerte', txt, { c: o.c, a, imp: gob ? 3 : (p.cargo || p.not > 0.5 ? 1 : 0), d: { p: p.id, modo: o.modo || 'natural', por: o.por === undefined ? -1 : o.por, gobernante: gob, est: p.est } });
    p.evMuerte = ev;
    S.emitir(m, 'pers.muere', p, ev, o);
    return ev;
  };

  S.en('pers.tic', function (m, e) {
    const p = m.per[e.p]; if (!p.vivo) return;
    const ed = P.edad(m, p);
    if (p.rng.p(0.0005 * Math.exp(0.085 * (ed - 30)) / 12)) { P.matar(m, p, { modo: 'natural' }); return; }
    const rol = S.reg.rol && S.reg.rol[p.rol];
    if (rol && rol.tic) rol.tic(m, p);
    if (p.ven) S.ven.tic(m, p);
    m.prog(m.t + 30, 'pers.tic', { p: p.id });
  });

  // ── L3: cohortes que se convierten en caras. Muestreo sin reemplazo (hipergeométrico) y semilla por índice.
  function hiper(rng, N, K, n) { let k = 0; for (let i = 0; i < n && i < N; i++) if (rng.f() < (K - k) / (N - i)) k++; return k; }
  P.hiper = hiper;
  P.caras = function (m, a, k) {
    const cohs = a.coh.map(c => m.coh[c]).filter(c => c.n > 0);
    let tot = 0; for (const c of cohs) tot += c.n;
    const out = []; if (!tot) return out;
    // Reparto proporcional por restos mayores (determinista).
    const cuota = cohs.map(c => k * c.n / tot); const kk = cuota.map(Math.floor); let falta = k - kk.reduce((x, y) => x + y, 0);
    const ord = cuota.map((q, i) => [q - kk[i], i]).sort((x, y) => y[0] - x[0] || x[1] - y[1]);
    for (let i = 0; i < falta && i < ord.length; i++) kk[ord[i][1]]++;
    for (let j = 0; j < cohs.length; j++) {
      const c = cohs[j]; const n = Math.min(kk[j], c.n); if (!n) continue;
      const q = 1 - S.cdfN((0.5 - c.agr.reg) / c.disp);            // fracción de la cohorte que odia al régimen
      const odian = hiper(new S.Rng(S.hash(m.semilla, 'hg', c.id, Math.round(q * 50))), c.n, Math.round(q * c.n), n);
      const caras = [];
      for (let i = 0; i < n; i++) {
        const rng = new S.Rng(S.hash(m.semilla, 'cara', c.id, i));
        const sexo = rng.p(0.5) ? 'M' : 'H';
        caras.push({ coh: c.id, i, sexo, nom: P.nombre(rng, sexo), edad: Math.floor(rng.r(17, 68)), u: rng.f(), ang: rng.f() * 6.283, rad: rng.f(), ficha: c.fichas && c.fichas[i] !== undefined ? c.fichas[i] : -1, odia: false, oficio: c.ins >= 0 ? m.ins[c.ins].nom : 'vecino' });
      }
      const porU = caras.slice().sort((x, y) => y.u - x.u);
      for (let i = 0; i < odian; i++) porU[i].odia = true;
      for (const x of caras) out.push(x);
    }
    return out;
  };
  // «Lo tocado persiste»: la cara pasa a tener ficha propia y ya no vuelve al grupo anónimo.
  P.promover = function (m, a, cara) {
    if (cara.ficha >= 0) return m.per[cara.ficha];
    const c = m.coh[cara.coh];
    const p = P.crear(m, { casa: a.id, est: a.est, sexo: cara.sexo, nom: cara.nom, nace: m.t - cara.edad * S.ANIO, ideo: c.ideo, rol: 'civil' });
    p.coh = c.id; (c.fichas = c.fichas || {})[cara.i] = p.id; cara.ficha = p.id;
    return p;
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
