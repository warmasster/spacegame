// Sociedad: cohortes con agravio y culpa, la revuelta como modelo de umbrales (Granovetter) con histéresis,
// la represión percibida y la lealtad del soldado como suma ponderada de seis tirones.
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B;
  const So = S.soc = {};
  So.RAD = 0.03; So.DESC = 0.60; So.SD = 0.12;

  So.nuevaCohorte = function (m, a, ins, n) {
    const id = m.coh.length; const rng = a.rng;
    if (!a.ideo) { a.ideo = []; for (let i = 0; i < 5; i++) a.ideo.push(S.clamp(rng.n(0, 0.4), -1, 1)); }
    const c = { id, ase: a.id, ins, n, agr: { reg: rng.r(0.2, 0.35), pat: rng.r(0, 0.15), acap: 0.05, ext: 0, crim: 0 }, ideo: a.ideo.map(x => S.clamp(x + rng.n(0, 0.15), -1, 1)), disp: So.SD, fac: -1 };
    m.coh.push(c); a.coh.push(id); return c;
  };
  So.ajustarPlantilla = function (m, i) { if (i.coh < 0) return; const def = S.reg.inst[i.tipo]; m.coh[i.coh].n = Math.max(1, Math.round(def.trab * i.nivel)); So.resinc(m, m.ase[i.ase]); };
  // La cohorte general es el resto de la población; los números siempre cuadran con el total.
  So.resinc = function (m, a) {
    let w = 0; for (const id of a.coh) if (id !== a.cohGen) w += m.coh[id].n;
    const pob = Math.round(a.pob);
    if (w > pob * 0.7) { const f = pob * 0.7 / w; w = 0; for (const id of a.coh) if (id !== a.cohGen) { const c = m.coh[id]; c.n = Math.max(1, Math.floor(c.n * f)); w += c.n; } }
    m.coh[a.cohGen].n = Math.max(0, pob - w);
  };

  So.nuevaGuarnicion = function (m, a) {
    const e = a.est >= 0 ? m.est[a.est] : null; const gob = e ? S.reg.gob[e.tipoGob] : null;
    const fo = e && e.cap !== a.id ? gob.forasteros : 0;
    const u = { id: m.uni.length, tipo: 'guarnicion', ase: a.id, n: Math.max(150, Math.round(a.pob * (e ? 0.03 : 0.02))), cmd: a.gob, est: a.est, ori: fo > 0 ? [{ a: a.id, f: 1 - fo }, { a: e.cap, f: fo }] : [{ a: a.id, f: 1 }], msc: 0, ideo: a.ideo.slice(), lado: -1 };
    if (e && e.cap === a.id) u.cmd = e.corte.general;
    m.uni.push(u); a.uni = u.id; return u;
  };

  // ── Hechos que el asentamiento recuerda (materia de facciones, memoriales y culpas).
  So.hecho = function (m, a, ev, clave, s, muertos) {
    a.hechos.push({ ev, clave, s, t: m.t, muertos: muertos || 0 });
    if (a.hechos.length > 12) { let k = 0; for (let i = 1; i < a.hechos.length; i++) if (a.hechos[i].s < a.hechos[k].s) k = i; a.hechos.splice(k, 1); }
  };

  // Muertes en una cohorte: lo que destruyes, alguien lo echa de menos.
  // o.clave = contra quién va el agravio de los supervivientes (pat, reg, ext, crim).
  So.muertes = function (m, a, co, n, ev, o) {
    o = o || {}; n = Math.min(n, co.n); if (n <= 0) return;
    const antes = co.n; co.n -= n; a.pob = Math.max(200, a.pob - n);
    const clave = o.clave || 'pat';
    const d = Math.min(0.6, 0.25 + 1.5 * n / antes);
    co.agr[clave] = Math.min(1, co.agr[clave] + d);
    const gen = m.coh[a.cohGen]; gen.agr[clave] = Math.min(1, gen.agr[clave] + d * Math.min(0.6, 12 * n / Math.max(200, a.pob) + 0.08));
    So.hecho(m, a, ev, o.hecho || 'explosion', Math.min(1, 0.3 + n / 40), n);
    if (n >= 4 && !a.ins.some(i => m.ins[i].tipo === 'memorial' && m.ins[i].ev === ev)) S.eco.nuevaInst(m, a, 'memorial', 1, { ev, nom: 'Memorial de los ' + S.numPal(n) + ' (' + a.nom + ')', muertos: n });
    S.emitir(m, 'muertes', a, co, n, ev, o);
  };

  // ── Lealtad del soldado: L = Σ w_k·s_k. s = cuánto tira cada factor hacia el bando A (0..1).
  So.L = function (w, s) { let x = 0; for (let i = 0; i < 6; i++) x += w[i] * s[i]; return x; };
  So.UMBRALES = { pasar: 0.15, informar: 0.25, desertar: 0.40, motin: 0.60 };
  So.pesos = function (m, u) {
    const w = [0.20, 0.15, 0.15, 0.20, 0.15, 0.15];
    let hambreCasa = 0; for (const o of u.ori) hambreCasa += o.f * m.ase[o.a].H;
    w[3] += 0.25 * Math.min(1, hambreCasa * 2); if (u.msc >= 2) w[4] += 0.05;
    const t = w[0] + w[1] + w[2] + w[3] + w[4] + w[5]; for (let i = 0; i < 6; i++) w[i] /= t;
    return w;
  };
  // Tirones hacia el régimen frente al pueblo del asentamiento a.
  So.tironRegimen = function (m, u, a) {
    const e = u.est >= 0 ? m.est[u.est] : null; const cmd = u.cmd >= 0 && m.per[u.cmd].vivo ? m.per[u.cmd] : null;
    const gob = e && e.gob >= 0 ? m.per[e.gob] : null;
    let casa = 0;
    for (const o of u.ori) {
      const h = m.ase[o.a];
      if (o.a === a.id) casa += o.f * S.clamp(1 - a.agr * 1.3, 0, 1);                 // su gente es la que está en la calle
      else casa += o.f * S.clamp(0.9 - h.H * 1.5 * So.parteCulpa(h, 'reg') - (h.est !== u.est ? 0.4 : 0), 0.05, 0.9);
    }
    // La paga llega tarde y mermada a las guarniciones que la capital apenas controla.
    const paga = S.clamp(1 - u.msc / 4, 0, 1) / (e ? Math.max(1, e.mon.P / e.mon.Ppaga) : 1) * (0.45 + 0.55 * (a.cap ? 1 : a.control));
    return [
      cmd ? 0.3 + 0.6 * cmd.r[S.R.LEA] : 0.4,
      e ? (e.suc ? 0.35 : 0.75) * (0.3 + 0.7 * (a.cap ? 1 : a.control)) : 0.5,           // la legalidad pesa lo que pesen las órdenes que llegan
      gob ? S.sim5(u.ideo, gob.ideo) * (0.7 + 0.5 * (e.asabiya === undefined ? 0.6 : e.asabiya)) : 0.5,
      casa, paga,
      0.5 * (1 - a.f) + 0.5 * (e ? S.clamp(e.fuerza || 0.6, 0.2, 0.9) : 0.5),
    ];
  };
  So.parteCulpa = function (a, k) { const c = a.culpa; const t = c.reg + c.acap + c.nat + c.ext; return t > 0 ? c[k] / t : 0; };

  // ── Represión percibida: R = (0,5·(0,2 + 0,8·π_G) + 0,5·(0,8 + 0,2·π_E))·(1 − señal).
  So.Rformula = (piG, piE, senal) => (0.5 * (0.2 + 0.8 * piG) + 0.5 * (0.8 + 0.2 * piE)) * (1 - senal);
  So.represion = function (m, a) {
    const e = a.est >= 0 ? m.est[a.est] : null;
    if (!e) { a.piG = 0.3; a.piE = 0.5; return a.R = 0.62; }
    const u = a.uni >= 0 ? m.uni[a.uni] : null;
    const ctl = a.cap ? 1 : a.control;
    const guar = u ? S.clamp(u.n / (a.pob * 0.03), 0, 1) * S.clamp(1 - u.msc / 5, 0.2, 1) : 0;
    a.piG = e.piG * Math.sqrt(ctl) * (0.5 + 0.5 * a.ley);
    a.piE = e.piE * guar * (0.4 + 0.6 * ctl);
    const r = So.Rformula(a.piG, a.piE, Math.min(0.6, a.senal)) * S.clamp(0.6 + 0.5 * e.rep, 0.5, 1) * (1 + (a.terror || 0));
    return a.R = Math.min(1, r);
  };
  // Un paso del modelo de umbrales: T_i = 1 − 0,75·k_i/R; f' = fracción con T_i ≤ f.
  So.umbral = function (f, mu, R, rad, desc, sd) {
    rad = rad === undefined ? So.RAD : rad; desc = desc === undefined ? So.DESC : desc;
    const k = (1 - f) * R / 0.75;
    return Math.min(1, rad + desc * (1 - S.cdfN((k - mu) / (sd || So.SD))));
  };
  // Itera hasta el punto fijo desde f0 (para pruebas y para el panel del sismógrafo).
  So.equilibrio = function (f0, mu, R) { let f = f0; for (let i = 0; i < 400; i++) { const n = So.umbral(f, mu, R); if (Math.abs(n - f) < 1e-7) break; f = n; } return f; };

  So.dia = function (m, a) {
    const e = a.est >= 0 ? m.est[a.est] : null; const c = a.culpa;
    // La culpa se olvida despacio.
    c.reg += (1 - c.reg) * 0.004; c.acap += (0.5 - c.acap) * 0.004; c.nat += (1 - c.nat) * 0.004; c.ext *= 0.996;
    if (a.acapFac >= 0 && a.acap[B.grano] > a.dem[B.grano] * 10 && a.H > 0.1) c.acap += 0.08;       // almacenes llenos y gente con hambre
    if (a.reparto > 0) { c.reg = Math.max(0.2, c.reg - 0.05 * Math.min(1, a.reparto)); a.reparto = 0; }
    const tot = c.reg + c.acap + c.nat + c.ext; const pReg = c.reg / tot, pAcap = c.acap / tot, pExt = c.ext / tot;
    let base = 0.15;
    if (e) {
      const gob = e.gob >= 0 ? m.per[e.gob] : null;
      base = 0.08 + 0.25 * e.rep + 0.9 * Math.max(0, e.imp - 0.12) + (e.base === 'popular' ? -0.1 : 0) + Math.min(0.3, (e.infl || 0) * 3);
      if (a.ocup) { const dt = (m.t - a.ocup.t) / S.ANIO; if (dt > 6) a.ocup = null; else base += 0.28 * Math.exp(-dt / 2); }
      a._gobIdeo = gob ? gob.ideo : null;
    } else a._gobIdeo = null;
    base += 0.5 * (a.brecha || 0);                                             // privación relativa (curva J)
    if (a.masacre) { base += a.masacre; a.masacre *= 0.9985; if (a.masacre < 0.01) a.masacre = 0; }
    if (a.terror) { a.terror *= 0.992; if (a.terror < 0.01) a.terror = 0; }
    if (a.nMasacres) a.nMasacres *= 0.9985;
    a.senal *= 0.9885;                                                          // τ ≈ 60 días
    So.resinc(m, a);
    let sn = 0, sa = 0;
    for (let x = 0; x < a.coh.length; x++) {
      const co = m.coh[a.coh[x]]; if (co.n <= 0) continue; const ag = co.agr;
      const obj = S.clamp(base + (a._gobIdeo ? 0.25 * (1 - S.sim5(co.ideo, a._gobIdeo)) : 0) + (a.H + 0.3 * a.Hf) * pReg, 0, 1);
      ag.reg += (obj - ag.reg) / (obj > ag.reg ? 30 : 120);
      const oa = a.H * pAcap; ag.acap += (oa - ag.acap) / (oa > ag.acap ? 30 : 120);
      const oe = a.H * pExt; if (oe > ag.ext) ag.ext += (oe - ag.ext) / 30; else ag.ext *= 0.9985;
      ag.pat *= 0.9988; ag.crim *= 0.998;
      sn += co.n; sa += co.n * ag.reg;
    }
    a.agr = sn > 0 ? sa / sn : 0;
    // Los retratos sin ojos: la única forma de ver lo que la gente piensa de verdad.
    a.ret += (S.clamp((a.agr - 0.35) * 1.6, 0, 1) - a.ret) * 0.01;
    So.represion(m, a);
    const rad = So.RAD + (a.celula || 0);
    const fv = Math.min(1, a.f + 0.1 * a.ret * (e ? 1 : 0));
    const nf = So.umbral(fv, a.agr, a.R * (1 - 0.25 * (a.efic || 0)), rad);   // si otros lo han conseguido, el miedo pesa menos
    a.f += (nf - a.f) * 0.5;
    if (a.celula) a.celula *= 0.97;
    if (e && a.revuelta < 0 && a.f >= 0.5) So.levantamiento(m, a);
    else if (a.revuelta >= 0 && a.f < 0.2) a.revuelta = -1;
  };

  // Media ciudad en la calle: lo que pase depende de la cuenta que haga cada soldado.
  So.levantamiento = function (m, a) {
    const e = m.est[a.est]; const u = a.uni >= 0 ? m.uni[a.uni] : null;
    let dif = 1;
    // Cada vez que les mandan disparar contra los vecinos les cuesta más: la cuenta se va torciendo.
    if (u && u.n > 0) { const Lr = So.L(So.pesos(m, u), So.tironRegimen(m, u, a)); dif = 1 - 2 * Lr + 0.2 * (a.nMasacres || 0); u.ultDif = dif; }
    const causas = [a.hambruna, a.causaHambre, a.evSenal];
    if (dif > So.UMBRALES.desertar) {
      a.revuelta = m.reg('revolucion', 'Revolución en {A' + a.id + '}: la guarnición ' + (dif > So.UMBRALES.motin ? 'se amotina y se une a la gente' : 'deserta y deja las armas') + '; el ' + Math.round(a.f * 100) + ' % de la ciudad está en la calle', { c: causas, a: a.id, imp: 3, d: { f: a.f, dif } });
      S.emitir(m, 'revolucion', a, a.revuelta, dif);
    } else if (dif > So.UMBRALES.pasar || So.noOrdena(m, a)) {
      const g = dif > So.UMBRALES.pasar ? null : S.pol.gobernadorDe(m, a);
      a.revuelta = m.reg('motin_pan', g ? '{P' + g.id + '} se niega a dar la orden de disparar contra la multitud de {A' + a.id + '} y manda abrir el depósito' : 'La gente rodea el depósito de {A' + a.id + '}; la guarnición espera órdenes que no llegan, no dispara y el depósito se abre', { c: causas, a: a.id, imp: 2, d: { f: a.f, dif } });
      const q = a.res[B.grano]; a.res[B.grano] = 0; a.alm[B.grano] += q; a.reparto += 3;
      for (const id of a.coh) m.coh[id].agr.reg = Math.max(0, m.coh[id].agr.reg - 0.12);
      a.control *= 0.8; a.senal += 0.05; a.f *= 0.5;
      S.emitir(m, 'motin_pan', a, a.revuelta);
    } else if (S.tec.insurreccion(m, a, causas)) {
      // Hay milicia organizada: la calle no espera a que disparen (el combate dura días y se resuelve en tierra).
    } else {
      const muertos = Math.round(a.pob * a.f * 0.004) + 3;
      a.revuelta = m.reg('masacre', 'Masacre en {A' + a.id + '}: la guarnición dispara contra la multitud y deja ' + muertos + ' muertos', { c: causas, a: a.id, imp: 2, d: { muertos } });
      a.terror = 0.35; a.masacre = (a.masacre || 0) + 0.12; a.f *= 0.4; a.culpa.reg += 4; a.nMasacres = (a.nMasacres || 0) + 1;
      if (u) u.n = Math.max(100, Math.round(u.n * 0.9));
      So.muertes(m, a, m.coh[a.cohGen], muertos, a.revuelta, { clave: 'reg', hecho: 'masacre', culpable: a.gob >= 0 ? a.gob : e.gob });
      S.emitir(m, 'masacre', a, a.revuelta, muertos);
    }
  };

  // Quien tiene que dar la orden también es una persona.
  So.noOrdena = function (m, a) { const g = S.pol.gobernadorDe(m, a); return !!g && g.r[S.R.EMP] > 0.62 && g.r[S.R.EMP] > g.r[S.R.AMB] * 0.8; };

  S.en('asent.mes', function (m, e) {
    const a = m.ase[e.a];
    S.emitir(m, 'asent.mes', a);
    m.prog(e.t + S.MES, 'asent.mes', e);
  });

  S.sismografo('agravio', 'Agravio medio contra el régimen', m => { let s = 0, p = 0; for (const a of m.ase) { s += a.agr * a.pob; p += a.pob; } return s / p; });
  S.sismografo('R', 'Represión percibida media', m => { let s = 0, p = 0; for (const a of m.ase) if (a.est >= 0) { s += a.R * a.pob; p += a.pob; } return p ? s / p : 0; });
  S.sismografo('calle', 'Gente en la calle (máximo)', m => { let s = 0; for (const a of m.ase) if (a.f > s) s = a.f; return s; });
})(typeof globalThis !== 'undefined' ? globalThis : this);
