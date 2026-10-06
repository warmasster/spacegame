// Estados: tesoro, moneda, deuda, control que cae con el retraso de las órdenes, informes de gobernadores
// que mienten con fórmula, guerras decididas con información vieja y la sucesión como subasta de poder.
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const R = S.R; const P = S.per;
  const Po = S.pol = {};
  Po.TAU_CONTROL = 30; Po.VEL_CORREO = 110;
  const K = (id, nom, llega) => S.def('carta', id, { nom, llega });

  Po.nuevoEstado = function (m, o) {
    const id = m.est.length; const def = S.reg.gob[o.tipo]; const rng = m.rngDe('E', id); const cap = m.ase[o.cap];
    const e = {
      id, nom: o.nom || def.nom + ' de ' + cap.nom, col: o.col || S.COLORES_ESTADO[id % S.COLORES_ESTADO.length], cap: o.cap, gob: -1, tipoGob: o.tipo, base: def.base,
      tes: o.tes === undefined ? 60000 : o.tes, mon: { nom: rng.el(S.NOM.moneda) + ' de ' + cap.nom, M: 1e6, M0: 1e6, P: 1, Ppaga: 1 }, imp: def.imp, trib: def.trib, rep: def.rep,
      guardia: !!def.guardia, piG: 1, piE: 1, gue: new Map(), rel: new Map(), sab: new Map(), flo: [], vivo: true, suc: null, tec: rng.r(0.85, 1.25),
      fiestas: [], rng, nac: m.t, ingr: 0, gasto: 0, infl: 0, fuerza: 0.6, corte: { general: -1, ministro: -1, arzobispo: -1, guardia: -1, heredero: -1 }, origen: o.origen === undefined ? -1 : o.origen,
      hist: [], victorias: 0, derrotas: 0, evNace: o.ev === undefined ? -1 : o.ev,
    };
    m.est.push(e);
    m.prog(m.t + 2 + rng.r(0, 28), 'estado.mes', { e: id });
    return e;
  };
  Po.refrescarSistemas = function (m) {
    for (const s of m.sis) { let best = -1, bp = -1; for (const id of s.ase) { const a = m.ase[id]; if (a.pob > bp) { bp = a.pob; best = a.est; } } s.est = best; }
  };
  Po.gobernadorDe = function (m, a) {
    if (a.gob >= 0 && m.per[a.gob].vivo) return m.per[a.gob];
    if (a.est >= 0 && m.est[a.est].cap === a.id && m.est[a.est].gob >= 0) return m.per[m.est[a.est].gob];
    return null;
  };
  Po.territorio = (m, e) => m.ase.filter(a => a.est === e.id);
  Po.titulo = (e, p) => S.reg.gob[e.tipoGob].tit[p.sexo === 'M' ? 1 : 0];

  function cortesano(m, e, rol, nom, o) {
    const cap = m.ase[e.cap]; const gob = e.gob >= 0 ? m.per[e.gob] : null;
    const p = P.crear(m, Object.assign({ casa: cap.id, est: e.id, rol, ideo: gob ? gob.ideo : undefined, not: 0.6, cargo: { t: rol, id: e.id, nom: nom + ' de ' + e.nom } }, o || {}));
    if (gob) { const r = P.rel(p, gob.id); r.op = p.rng.r(-0.2, 0.7); r.conf = p.rng.r(0.3, 0.8); r.miedo = e.rep; }
    return p;
  }
  Po.fundarCorte = function (m, e, gobId) {
    const cap = m.ase[e.cap]; const rng = e.rng; const def = S.reg.gob[e.tipoGob];
    let gob = gobId !== undefined && gobId >= 0 ? m.per[gobId] : null;
    if (!gob) {
      const tirano = def.base === 'personal';
      gob = P.crear(m, { casa: cap.id, est: e.id, rol: 'gobernante', not: 1, nace: m.t - rng.r(38, 64) * S.ANIO, ideo: [tirano ? 0.7 : rng.r(-0.4, 0.5), rng.r(-0.3, 0.6), def.base === 'popular' ? 0.6 : rng.r(-0.5, 0.4), rng.r(-0.5, 0.5), rng.r(-0.5, 0.5)], r: { AMB: rng.r(0.6, 0.95), EMP: tirano ? rng.r(0.05, 0.4) : rng.r(0.2, 0.8), PRU: tirano ? rng.r(0.55, 0.95) : rng.r(0.2, 0.8) } });
    }
    gob.rol = 'gobernante'; gob.est = e.id; gob.cargo = { t: 'gobernante', id: e.id, nom: Po.titulo(e, gob) + ' de ' + cap.nom }; gob.en = { t: 'A', id: cap.id }; gob.not = 1; e.gob = gob.id; e.desde = m.t;
    P.familia(m, gob);
    const her = gob.fam.hijos.map(h => m.per[h]).filter(h => h.vivo && P.edad(m, h) >= 16).sort((a, b) => a.nace - b.nace)[0];
    e.corte.heredero = her ? her.id : -1; if (her) { her.not = 0.5; her.r[R.AMB] = Math.max(her.r[R.AMB], 0.5); }
    e.corte.general = cortesano(m, e, 'general', 'general del ejército', { r: { VAL: rng.r(0.5, 0.95) } }).id;
    e.corte.ministro = cortesano(m, e, 'ministro', 'ministro de Abastos').id;
    e.corte.arzobispo = cortesano(m, e, 'arzobispo', 'arzobispo', { r: { FE: rng.r(0.7, 1) } }).id;
    if (e.guardia) e.corte.guardia = cortesano(m, e, 'jefe_guardia', 'jefe de la Guardia', { r: { LEA: rng.r(0.6, 0.95), EMP: rng.r(0, 0.3) } }).id;
  };
  Po.fundarFlota = function (m, e) {
    const rng = e.rng; const mis = Po.territorio(m, e); let pob = 0; for (const a of mis) { pob += a.pob; a.censo = { pob: a.pob, t: m.t }; }
    const cap = m.ase[e.cap]; const prov = mis.filter(a => a.id !== e.cap).sort((a, b) => b.pob - a.pob);
    const nF = S.clamp(Math.round(pob / 45000) + 3, 4, 16); const nFl = nF > 13 ? 3 : nF > 8 ? 2 : 1;
    const bases = [cap]; for (const a of prov) { if (bases.length >= nFl) break; if (a.sis !== cap.sis && !bases.some(b => b.sis === a.sis)) bases.push(a); }
    let puestas = 0;
    bases.forEach((b, k) => {
      const f = Po.nuevaFlota(m, e, b); const n = k === 0 ? Math.ceil(nF / bases.length) + (nF % bases.length ? 0 : 0) : Math.floor(nF / bases.length);
      for (let i = 0; i < n && puestas < nF; i++, puestas++) Po.nuevaFragata(m, e, f, b, prov, puestas % 4 === 3 ? 'crucero' : 'fragata');
    });
    if (nF >= 7) Po.nuevaFragata(m, e, m.flo[e.flo[0]], cap, prov, 'acorazado');
    const tit = (a, cls, o) => S.nav.crear(m, cls, Object.assign({ en: a.id, est: e.id, dueno: { t: 'E', id: e.id } }, o || {}));
    for (let i = 0; i < 2 + Math.round(prov.length / 6); i++) tit(cap, 'correo');
    const agr = mis.filter(a => a.semNec > 0 && !a.cap).length;
    for (let i = 0; i < S.clamp(1 + agr, 1, 4); i++) tit(cap, 'granelero');
    tit(cap, 'censo');
    if (e.rep >= 0.6) { const pr = tit(cap, 'prision'); pr.presos = []; e.prision = pr.id; }
    if (S.reg.gob[e.tipoGob].hereditario) e.funeraria = tit(cap, 'funeraria').id;
    for (let i = 0; i < rng.i(3); i++) { const c = P.crear(m, { casa: cap.id, est: e.id, rol: 'corsario', r: { VAL: rng.r(0.6, 0.95) } }); S.nav.crear(m, 'corbeta', { en: cap.id, est: e.id, cap: c.id, dueno: { t: 'P', id: c.id }, patente: e.id, dinero: 9000 }); }
  };
  Po.nuevaFlota = function (m, e, base) {
    const alm = P.crear(m, { casa: base.id, est: e.id, rol: 'almirante', not: 0.6, ideo: m.per[e.gob].ideo, r: { VAL: e.rng.r(0.4, 0.95) } });
    const f = { id: m.flo.length, est: e.id, alm: alm.id, nav: [], base: base.id, sis: base.sis, st: 'base', orden: null, mis: null, vivo: true, nom: 'Flota de ' + base.nom };
    alm.cargo = { t: 'almirante', id: f.id, nom: 'almirante de la ' + f.nom };
    m.flo.push(f); e.flo.push(f.id);
    m.prog(m.t + 3 + e.rng.r(0, 4), 'flota.tic', { f: f.id });
    return f;
  };
  // La tripulación de cada fragata nació en algún sitio: eso decide a quién sigue cuando el Estado se parte.
  Po.nuevaFragata = function (m, e, f, base, prov, cls) {
    const rng = e.rng; const casa = rng.p(0.5) || !prov.length ? e.cap : prov[rng.pesos(prov.map(a => a.pob))].id;
    const n = S.nav.crear(m, cls || 'fragata', { en: base.id, est: e.id, dueno: { t: 'E', id: e.id }, flo: f.id, casaTrip: casa });
    f.nav.push(n.id); return n;
  };

  // ── Piezas del Estado y su cuota de poder (para la sucesión y los golpes).
  Po.piezas = function (m, e) {
    const pz = []; const mis = Po.territorio(m, e); const cap = m.ase[e.cap];
    let navT = 0; for (const id of e.flo) navT += m.flo[id].nav.length;
    for (const id of e.flo) { const f = m.flo[id]; if (!f.nav.length) continue; pz.push({ t: 'flota', nom: f.nom + ' (' + f.nav.reduce((s, i) => s + m.nav[i].cascos, 0) + ' naves)', lid: f.alm, w: 0.30 * f.nav.length / Math.max(1, navT), tropas: true, ref: id, ase: f.base }); }
    pz.push({ t: 'ejercito', nom: 'Ejército de la capital', lid: e.corte.general, w: 0.15, tropas: true, ase: e.cap });
    if (e.guardia) pz.push({ t: 'guardia', nom: 'Guardia', lid: e.corte.guardia, w: 0.15, tropas: true, ase: e.cap });
    pz.push({ t: 'ministerios', nom: 'Ministerios', lid: e.corte.ministro, w: 0.10, ase: e.cap });
    pz.push({ t: 'iglesia', nom: 'Iglesia', lid: e.corte.arzobispo, w: 0.08, ase: e.cap });
    for (const d of m.deu) if (d.deudor === e.id && d.monto > 2000 && m.fac[d.acreedor].vivo) pz.push({ t: 'casa', nom: m.fac[d.acreedor].nom + ' (acreedora)', lid: m.fac[d.acreedor].lid, w: 0.03 + 0.05 * Math.min(1, d.monto / 150000), ref: d.acreedor, ase: m.fac[d.acreedor].sede, deuda: d.monto });
    let pobP = 0, cosT = 0; const porSis = new Map();
    for (const a of mis) { if (a.sis === cap.sis) continue; pobP += a.pob; cosT += a.cosechaNormal; const x = porSis.get(a.sis) || { pob: 0, cos: 0, top: a }; x.pob += a.pob; x.cos += a.cosechaNormal; if (a.pob > x.top.pob) x.top = a; porSis.set(a.sis, x); }
    for (const [sis, x] of porSis) { if (x.top.gob < 0) continue; pz.push({ t: 'gobernador', nom: (x.cos > 0 ? 'Graneros y milicias de ' : 'Provincia de ') + x.top.nom, lid: x.top.gob, w: 0.20 * x.pob / Math.max(1, pobP) + 0.12 * (cosT > 0 ? x.cos / cosT : 0), tropas: true, ref: sis, ase: x.top.id }); }
    let t = 0; for (const p of pz) t += p.w; for (const p of pz) { p.poder = p.w / t; p.lado = null; }
    return pz;
  };

  // ── Gobernador: informa a la capital. pérdida informada = pérdida real · (1 − miedo·(1 − honradez)).
  Po.mentira = (real, miedo, honradez) => real * (1 - miedo * (1 - honradez));
  Po.miedoDe = function (m, p, e) { const r = p.rel.get(e.gob); return r ? r.miedo : e.rep; };
  S.def('rol', 'gobernador', {
    nom: 'gobernador',
    tic(m, p) {
      const a = m.ase[p.cargo.id]; if (!a || a.gob !== p.id || a.est < 0) return;
      const e = m.est[a.est]; const miedo = Po.miedoDe(m, p, e), hon = p.r[R.LEA];
      S.cor.enviar(m, a.id, e.cap, 'informe', { a: a.id, H: Po.mentira(a.H, miedo, hon), agr: Po.mentira(a.agr, miedo, hon), pob: a.pob, f: Po.mentira(a.f, miedo, hon) }, -1, e.id);
      // Lejos de la capital y con ambición: independencia de hecho.
      if (a.control < 0.3 + 0.15 * (a.demo || 0) && p.r[R.AMB] > 0.7 && !e.suc && p.rng.p(0.04 * Math.max(0.1, 1 - a.control * 2) * (1 + 3 * (a.demo || 0)))) {
        const ev = m.reg('independencia', '{P' + p.id + '} deja de obedecer: las órdenes tardan ' + Math.round(a.retraso) + ' días en ir y volver y la capital controla un ' + Math.round(a.control * 100) + ' %. {A' + a.id + '} es independiente de hecho', { a: a.id, imp: 2, d: { e: e.id } });
        Po.secesion(m, e, [a.sis], p, 'senorio', ev);
      }
    },
  });
  S.gancho('cosecha', function (m, a, perdida, ev) {
    if (a.est < 0 || a.cap) return; const e = m.est[a.est]; const gob = Po.gobernadorDe(m, a); if (!gob) return;
    const decl = Po.mentira(perdida, Po.miedoDe(m, gob, e), gob.r[R.LEA]);
    a.evInforme = -1;
    if (perdida > 0.12) a.evInforme = m.reg('informe', '{P' + gob.id + '} informa a la capital de que la cosecha de {A' + a.id + '} cayó un ' + Math.round(decl * 100) + ' % (cayó un ' + Math.round(perdida * 100) + ' %)', { a: a.id, imp: decl < perdida * 0.6 ? 1 : 0, c: [ev], d: { real: perdida, decl } });
    S.cor.enviar(m, a.id, e.cap, 'informe_cosecha', { a: a.id, decl, urg: 2 }, a.evInforme >= 0 ? a.evInforme : ev, e.id);
  });
  K('informe', 'un informe de provincia', function (m, c, cap) {
    const a = m.ase[c.d.a]; if (a.est < 0 || m.est[a.est].cap !== cap.id) return;
    const e = m.est[a.est]; e.sab.set(a.id, { t: c.t, H: c.d.H, agr: c.d.agr, pob: c.d.pob, f: c.d.f }); a.ultInf = c.t; a.ultInfLlega = m.t;
  });
  // La capital solo perdona tributo si la pérdida DECLARADA pasa del 15 %.
  K('informe_cosecha', 'el informe de la cosecha', function (m, c, cap) {
    const a = m.ase[c.d.a]; if (a.est < 0 || m.est[a.est].cap !== cap.id) return; const e = m.est[a.est];
    if (c.d.decl >= 0.15) S.cor.enviar(m, cap.id, a.id, 'perdon', { a: a.id, v: c.d.decl, urg: 2 }, c.c, e.id);
  });
  K('perdon', 'un perdón de tributo', function (m, c, a) { if (m.ase[c.d.a] === a || m.ase[c.d.a].sis === a.sis) m.ase[c.d.a].perdon = c.d.v; });
  K('nombramiento', 'un nombramiento', function (m, c) {
    const a = m.ase[c.d.a]; if (a.est !== c.d.e || (a.gob >= 0 && m.per[a.gob].vivo)) return;
    const e = m.est[a.est]; const p = P.crear(m, { casa: a.id, est: e.id, rol: 'gobernador', ideo: e.gob >= 0 ? m.per[e.gob].ideo : undefined, cargo: { t: 'gobernador', id: a.id, nom: 'gobernador de ' + a.nom } });
    a.gob = p.id; if (a.uni >= 0) m.uni[a.uni].cmd = p.id;
    m.reg('ascenso', '{P' + p.id + '} llega a {A' + a.id + '} como nuevo gobernador; detrás de él sube un escalón cada subordinado', { a: a.id, imp: 0, c: [c.c] });
  });

  // ── El mes de un Estado.
  S.en('estado.mes', function (m, ev) {
    const e = m.est[ev.e]; if (!e.vivo) return;
    m.prog(ev.t + S.MES, 'estado.mes', ev);
    const mis = Po.territorio(m, e);
    if (!mis.length) return Po.caer(m, e, -1);
    if (mis.indexOf(m.ase[e.cap]) < 0) Po.mudarCapital(m, e, mis);
    const cap = m.ase[e.cap]; const gob = e.gob >= 0 && m.per[e.gob].vivo ? m.per[e.gob] : null;
    // Control: e^(−retraso/τ). El retraso es el viaje de ida y vuelta del correo, más lo que tarde en llegar el informe.
    let imp = 0, pobT = 0, agrT = 0;
    for (const a of mis) {
      pobT += a.pob; agrT += a.agr * a.pob;
      if (a.id === cap.id || a.sis === cap.sis) { a.retraso = 0; a.control = 1; }
      else {
        const teo = 2 * m.dist[cap.sis][a.sis] / Po.VEL_CORREO + 3; const silencio = Math.max(0, m.t - a.ultInf - 50);
        a.retraso = teo + silencio * 0.5; a.control = Math.exp(-a.retraso / (Po.TAU_CONTROL * (0.7 + 0.6 * (e.asabiya === undefined ? 0.5 : e.asabiya))));
        if (a.gob < 0 || !m.per[a.gob].vivo) { a.control *= 0.6; if (!a.nombrando || m.t - a.nombrando > 90) { a.nombrando = m.t; S.cor.enviar(m, cap.id, a.id, 'nombramiento', { a: a.id, e: e.id, urg: 1 }, a.evVacante, e.id); } }
      }
      // Lo que no se puede contar no se puede gobernar: sin censo reciente, la capital cobra a ojo.
      const cen = a.censo ? a.censo.pob * (m.t - a.censo.t > 6 * S.ANIO ? 0.55 : 1) : a.pob * 0.5;
      const g = Po.gobernadorDe(m, a); const sisa = g && g !== gob ? g.r[R.COD] * 0.3 * (1 - a.control * 0.5) : 0;
      const t = Math.min(cen, a.pob) * 1.15 * e.imp * (0.3 + 0.7 * a.control) * (a.huelga > m.t ? 0.6 : 1);
      imp += t * (1 - sisa); if (g && sisa) g.din += t * sisa * 0.1;
    }
    e.pob = pobT; e.agr = pobT ? agrT / pobT : 0;
    if (!e.mon.cal) { e.mon.cal = true; e.mon.M = e.mon.M0 = Math.max(2e5, pobT * 1.15 * e.imp * 12) * e.mon.P; }   // la masa monetaria, a la medida del presupuesto de un año
    e.sab.set(cap.id, { t: m.t, H: cap.H, agr: cap.agr, pob: cap.pob, f: cap.f });
    // Gastos: guarniciones, flota, Guardia (cobra el doble), corte, intereses.
    let sold = 0, navG = 0, navO = 0, esc = 0;
    for (const a of mis) if (a.uni >= 0) sold += m.uni[a.uni].n;
    for (const n of m.nav) if (n.vivo && n.dueno.t === 'E' && n.dueno.id === e.id) { if (S.reg.nave[n.cls].guerra) { esc++; navG += n.cascos * S.reg.nave[n.cls].ef / 0.10; } else navO++; }
    navG = Math.round(navG); e.escuadras = esc; e.soldados = sold;                 // navG = cascos, en equivalente a fragatas
    let gasto = sold * 2.4 + esc * 1500 + navO * 400 + 3000 + e.gasto + (e.guardia ? pobT * 0.004 * 24 : 0);
    let inter = 0; for (const d of m.deu) if (d.deudor === e.id) { inter += d.monto * d.tasa / 12; }
    gasto += inter;
    const ingreso = imp + e.ingr; e.ultIngreso = ingreso; e.ultGasto = gasto; e.navG = navG;
    e.ultRentas = e.ingr; e.ingr = 0; e.gasto = 0; e.tes += ingreso - gasto;
    for (const d of m.deu) if (d.deudor === e.id && m.fac[d.acreedor].vivo) m.fac[d.acreedor].caja += d.monto * d.tasa / 12;
    let impago = false;
    if (e.tes < 0 && gob) {
      const def = -e.tes; const ceca = cap.ins.some(i => m.ins[i].tipo === 'ceca' && m.ins[i].salud > 0.3);
      // Las casas prestan mientras la deuda no pase de un año de ingresos y el Estado no haya quebrado hace poco.
      const deuda = Po.deuda(m, e); const credito = deuda < ingreso * 12 && m.t > (e.sinCredito || 0);
      const casa = credito ? m.fac.filter(f => f.vivo && f.tipo === 'casa' && f.caja > def * 1.5).sort((x, y) => y.caja - x.caja)[0] : null;
      if (deuda > ingreso * 30 && deuda > 50000) Po.bancarrota(m, e, gob);
      const k = gob.rng.soft([ceca ? 0.3 - gob.r[R.PRU] * 0.2 : -9, casa ? 0.4 : -9, 0.2 + gob.r[R.COD] * 0.15, e.imp < 0.34 ? 0.12 : -9, 0.22 + gob.r[R.PRU] * 0.15], 0.12);
      if (k === 0) { // imprimir: los precios en esa moneda suben parecido a medio plazo
        e.mon.M += def * 2.5; e.tes = 0;
        if (!e.evImprime || m.t - m.ev[e.evImprime].t > 300) e.evImprime = m.reg('imprime', '{E' + e.id + '} imprime moneda (' + e.mon.nom + ') para pagar al ejército: los precios subirán detrás', { a: cap.id, imp: 1, d: { e: e.id } });
      } else if (k === 1) { Po.prestamo(m, e, casa, def * 1.3); }
      else if (k === 2) { impago = true; e.tes = 0; }
      else if (k === 4) {
        // Recortes: menos soldados en cada guarnición (y menos represión detrás).
        for (const a of mis) if (a.uni >= 0) { const u = m.uni[a.uni]; u.n = Math.max(100, Math.round(u.n * 0.9)); }
        e.recortes = (e.recortes || 0) + 1; e.tes = 0; impago = e.recortes % 2 === 0;
        if (!e.evRecorte || m.t - m.ev[e.evRecorte].t > 500) e.evRecorte = m.reg('recortes', '{E' + e.id + '} no puede pagar a su ejército: licencia a uno de cada diez soldados', { a: cap.id, imp: 1, d: { e: e.id } });
      }
      else { e.imp = Math.min(0.36, e.imp + 0.02); impago = true; e.tes = 0; m.reg('impuestos', '{E' + e.id + '} sube los impuestos al ' + Math.round(e.imp * 100) + ' %', { a: cap.id, imp: 0, d: { e: e.id } }); }
    } else if (e.tes > 150000) {
      const d = m.deu.find(x => x.deudor === e.id && x.monto > 0); if (d) { const q = Math.min(d.monto, e.tes - 100000); d.monto -= q; e.tes -= q; if (m.fac[d.acreedor].vivo) m.fac[d.acreedor].caja += q; }
      else if (e.imp > S.reg.gob[e.tipoGob].imp || (e.tes > 300000 && e.imp > S.reg.gob[e.tipoGob].imp * 0.6)) e.imp -= 0.005;
    }
    // Sin dinero no llegan las pagas: primero dejan de cobrar los que están más lejos.
    const ord = mis.filter(a => a.uni >= 0).sort((x, y) => x.control - y.control);
    ord.forEach((a, i) => { const u = m.uni[a.uni]; if (impago && i < Math.ceil(ord.length * 0.6)) u.msc++; else if (u.msc > 0) u.msc--; });
    e.msc = impago ? (e.msc || 0) + 1 : Math.max(0, (e.msc || 0) - 1);
    if (e.tes > 60000) for (const a of mis) if (a.uni >= 0) { const u = m.uni[a.uni]; const pl = Math.max(150, Math.round(a.pob * 0.03)); if (u.n < pl) u.n = Math.min(pl, Math.round(u.n * 1.05) + 1); }
    // Moneda: P tiende a M/M0.
    const P0 = e.mon.P; e.mon.M0 *= 1.002; e.mon.P += (e.mon.M / e.mon.M0 - e.mon.P) * 0.12; if (e.mon.P < 1) e.mon.P = 1;
    e.infl += ((e.mon.P - P0) / P0 - e.infl) * 0.4; e.mon.Ppaga += (e.mon.P - e.mon.Ppaga) * 0.06;
    e.piG += (1 - e.piG) * 0.12; e.piE += (1 - e.piE) * 0.12;
    e.fuerza = S.clamp(0.55 + 0.08 * (e.victorias - e.derrotas) + (e.msc ? -0.1 : 0.05) - (e.suc ? 0.15 : 0), 0.2, 0.9);
    e.victorias *= 0.97; e.derrotas *= 0.97;
    e.hist.push(navG); if (e.hist.length > 40) e.hist.shift();
    if (gob) { Po.encargos(m, e, mis, esc, navO); S.teo.diplomacia(m, e, gob); Po.diplomacia(m, e, gob, mis); Po.ordenes(m, e, mis); Po.golpe(m, e, gob); gob.ideo[0] = Math.min(1, gob.ideo[0] + 0.004); }   // el poder también cambia las ideas de quien lo tiene
    else if (!e.suc) Po.sucesion(m, e, -1, 'silencio');
  });

  Po.prestamo = function (m, e, f, q) {
    f.caja -= q; e.tes += q;
    let d = m.deu.find(x => x.deudor === e.id && x.acreedor === f.id);
    if (!d) { d = { deudor: e.id, acreedor: f.id, monto: 0, tasa: 0.08 }; m.deu.push(d); }
    d.monto += q;
    if (!d.ev || m.t - m.ev[d.ev].t > 400) d.ev = m.reg('prestamo', f.nom + ' presta ' + S.fmt(q) + ' a {E' + e.id + '}: ya le debe ' + S.fmt(d.monto), { a: e.cap, imp: 0, d: { e: e.id, f: f.id } });
  };
  Po.deuda = (m, e) => { let s = 0; for (const d of m.deu) if (d.deudor === e.id) s += d.monto; return s; };
  Po.bancarrota = function (m, e, gob) {
    let total = 0; const ac = [];
    for (const d of m.deu) if (d.deudor === e.id && d.monto > 0) { total += d.monto; const f = m.fac[d.acreedor]; ac.push(f.nom); d.monto = 0; f.evQuiebra = -1; }
    e.sinCredito = m.t + 12 * S.ANIO;
    const ev = m.reg('bancarrota', '{E' + e.id + '} se declara en bancarrota: deja de pagar ' + S.fmt(total) + ' a ' + ac.join(', ') + '. Nadie le prestará en años', { a: e.cap, imp: 2, c: [e.evGuerra], d: { e: e.id } });
    for (const d of m.deu) if (d.deudor === e.id) m.fac[d.acreedor].evQuiebra = ev;
  };

  Po.encargos = function (m, e, mis, navG, navO) {
    const quiere = S.clamp(Math.max(Math.round(e.pob / 45000) + 3 + (e.gue.size ? 3 : 0), S.teo.carrera(m, e)), 4, 22);
    const yardas = mis.filter(a => a.pedidos.length < 2 && a.ins.some(i => m.ins[i].tipo === 'astillero' && m.ins[i].salud > 0.4));
    if (!yardas.length) return;
    const y = yardas[0];
    const enObra = (cls) => mis.reduce((s, a) => s + a.pedidos.filter(p => (cls === 'guerra' ? S.reg.nave[p.cls].linea : p.cls === cls) && p.estado === e.id).length, 0);
    if (navG + enObra('guerra') < quiere && e.tes > 90000) {
      const cls = ['fragata', 'crucero', 'acorazado'][e.rng.pesos([0.65, 0.27, 0.08])]; const coste = { fragata: 70000, crucero: 110000, acorazado: 170000 }[cls];
      if (e.tes > coste + 20000) { e.tes -= coste; S.eco.pedirNave(m, y, cls, { estado: e.id, est: e.id, ev: e.evGuerra }); return; }
    }
    let cor = 0, gra = 0; for (const n of m.nav) if (n.vivo && n.dueno.t === 'E' && n.dueno.id === e.id) { if (n.cls === 'correo') cor++; if (n.cls === 'granelero') gra++; }
    if (cor + enObra('correo') < 2 + Math.round(mis.length / 7) && e.tes > 30000) { e.tes -= 15000; S.eco.pedirNave(m, y, 'correo', { estado: e.id, est: e.id }); return; }
    if (gra + enObra('granelero') < 1 && mis.some(a => a.semNec > 0 && !a.cap) && e.tes > 60000) { e.tes -= 40000; S.eco.pedirNave(m, y, 'granelero', { estado: e.id, est: e.id }); }
  };
  S.gancho('nave.nueva', function (m, n, p) {
    if (p.estado === undefined) return; const e = m.est[p.estado];
    if (!e.vivo) { n.pirata = true; return; }
    n.dueno = { t: 'E', id: e.id }; n.est = e.id;
    if (S.reg.nave[n.cls].linea) {
      let f = e.flo.map(i => m.flo[i]).filter(x => x.vivo && x.st === 'base' && m.ase[x.base].sis === m.ase[n.en].sis)[0];
      if (!f) f = e.flo.map(i => m.flo[i]).filter(x => x.vivo)[0];
      if (!f || f.st !== 'base' || m.ase[f.base].sis !== m.ase[n.en].sis) f = Po.nuevaFlota(m, e, m.ase[n.en]);
      n.flo = f.id; f.nav.push(n.id); n.casaTrip = n.en;
    }
  });

  // ── Diplomacia con información vieja: lo que la capital cree de la flota vecina tiene meses de retraso.
  Po.rel = function (m, e, j) { let r = e.rel.get(j); if (!r) { const o = m.est[j]; r = { op: e.gob >= 0 && o.gob >= 0 ? S.sim5(m.per[e.gob].ideo, m.per[o.gob].ideo) - 0.5 : 0, cb: 0, ev: -1 }; e.rel.set(j, r); } return r; };
  Po.vecinos = function (m, e) {
    const v = new Set();
    for (const s of m.sis) { if (s.est !== e.id) continue; for (const x of s.vec) { const o = m.sis[x.a].est; if (o >= 0 && o !== e.id && m.est[o].vivo) v.add(o); } }
    return Array.from(v);
  };
  Po.fuerzaCreida = function (m, e, j) {
    const o = m.est[j]; const lag = Math.min(o.hist.length - 1, Math.round(m.dist[m.ase[e.cap].sis][m.ase[o.cap].sis] / 55 / 30) + 1);
    const n = o.hist.length ? o.hist[Math.max(0, o.hist.length - 1 - lag)] : 6;
    return n * n * 0.1 * o.tec;                                                // la fuerza crece con el cuadrado del número
  };
  Po.diplomacia = function (m, e, gob, mis) {
    if (e.suc) return;
    const propia = (e.navG || 0) * (e.navG || 0) * 0.1 * e.tec;
    // Guerras en curso: cansancio y paz.
    for (const [j, w] of e.gue) {
      const o = m.est[j]; if (!o.vivo) { e.gue.delete(j); continue; }
      w.cans = (w.cans || 0) + 0.04 + (e.tes < 0 ? 0.05 : 0) + (e.agr > 0.55 ? 0.03 : 0);
      if (w.civil) continue;
      if (w.cans > 1 + gob.r[R.AMB] * 0.8 + (w.score || 0) * 0.2 && !w.pidePaz) { w.pidePaz = true; S.cor.enviar(m, e.cap, o.cap, 'paz', { de: e.id, a: j, urg: 2 }, w.ev, e.id); }
    }
    if (e.gue.size) return;
    const us = [0.95 + gob.r[R.PRU] * 0.3], ids = [-1];
    let hambre = 0; for (const [, s] of e.sab) if (s.H > hambre) hambre = s.H;
    for (const j of Po.vecinos(m, e)) {
      const o = m.est[j]; const r = Po.rel(m, e, j); r.cb *= 0.985;
      const rival = Po.fuerzaCreida(m, e, j); const ventaja = propia / (propia + rival + 1e-9) - 0.5;
      const granero = Po.territorio(m, o).some(a => a.semNec > 0) ? hambre * 0.5 : 0;
      us.push(gob.r[R.AMB] * 0.3 + r.cb * 0.22 + ventaja * 1.1 - r.op * 0.25 + granero + (e.agr > 0.55 ? 0.12 : 0) - gob.r[R.PRU] * 0.25 - (e.tes < 20000 ? 0.25 : 0) - (o.origen === e.id || e.origen === j ? -0.2 : 0) + S.teo.guerraU(m, e, j, propia, rival));
      ids.push(j);
    }
    const k = gob.rng.soft(us, 0.11);
    if (ids[k] >= 0) Po.declarar(m, e, ids[k]);
  };
  Po.declarar = function (m, e, j, o) {
    o = o || {}; const r = Po.rel(m, e, j); const riv = m.est[j];
    const motivo = o.motivo || (r.cb > 0.8 ? 'no perdona lo ocurrido' : (e.agr > 0.55 ? 'necesita un enemigo fuera' : 'cree que su flota es más fuerte'));
    const ev = m.reg('guerra', '{E' + e.id + '} declara la guerra a {E' + j + '}: ' + motivo, { a: e.cap, imp: 3, c: [r.ev, o.c], d: { e: e.id, j, civil: !!o.civil } });
    e.gue.set(j, { t0: m.t, sabe: true, ev, cans: 0, score: 0, civil: !!o.civil }); e.evGuerra = ev;
    if (o.civil) riv.gue.set(e.id, { t0: m.t, sabe: true, ev, cans: 0, score: 0, civil: true });
    else S.cor.enviar(m, e.cap, riv.cap, 'guerra', { de: e.id, a: j, urg: 3 }, ev, e.id);
    S.inf.crear(m, ev, 'guerra', e.cap, 0.9, 1, { e: e.id, j });
    return ev;
  };
  K('guerra', 'una declaración de guerra', function (m, c) { const o = m.est[c.d.a], e = m.est[c.d.de]; if (!o.vivo || !e.vivo || o.gue.has(e.id)) return; o.gue.set(e.id, { t0: m.t, sabe: true, ev: c.c, cans: 0, score: 0 }); o.evGuerra = c.c; m.reg('guerra_recibida', 'La declaración de guerra de {E' + e.id + '} llega a {A' + o.cap + '} ' + Math.round(m.t - c.t) + ' días después de firmarse', { a: o.cap, imp: 1, c: [c.c] }); });
  // Si atacan a un Estado que aún no sabía nada, se entera por las noticias.
  S.gancho('noticia.batalla', function (m, a, p) {
    for (const lado of p.d.lados) { const e = m.est[lado]; if (!e || !e.vivo || e.cap !== a.id) continue; for (const otro of p.d.lados) if (otro !== lado && otro >= 0 && m.est[otro].vivo && !e.gue.has(otro)) e.gue.set(otro, { t0: m.t, sabe: true, ev: p.ev, cans: 0, score: 0 }); }
    if (a.est >= 0 && p.d.pierde === a.est) { a.senal += 0.05; a.evSenal = p.ev; }
  });
  K('paz', 'una oferta de paz', function (m, c) {
    const o = m.est[c.d.a], e = m.est[c.d.de]; if (!o.vivo || !e.vivo) return; const w = o.gue.get(e.id); if (!w) return;
    const gob = o.gob >= 0 ? m.per[o.gob] : null;
    if (!gob || (w.cans || 0) > 0.5 + gob.r[R.AMB] * 0.5 || o.tes < 0) {
      o.gue.delete(e.id); e.gue.delete(o.id);
      const ev = m.reg('paz', '{E' + o.id + '} acepta la paz con {E' + e.id + '}: cada uno se queda con lo que tiene', { a: o.cap, imp: 2, c: [w.ev], d: { e: e.id, j: o.id } });
      S.inf.crear(m, ev, 'paz', o.cap, 0.8, 1, { e: e.id, j: o.id });
      for (const x of [e, o]) for (const id of x.flo) { const f = m.flo[id]; if (f.vivo) S.cor.enviar(m, x.cap, f.base, 'orden', { flo: id, tipo: 'volver', urg: 2 }, ev, x.id); }
    } else { const w2 = e.gue.get(o.id); if (w2) w2.pidePaz = false; }
  });

  // Órdenes a las flotas: viajan como cartas a su base. Si el correo no llega, la flota no se mueve.
  Po.ordenes = function (m, e, mis) {
    const flotas = e.flo.map(i => m.flo[i]).filter(f => f.vivo && f.nav.length);
    // Sistemas propios donde la capital SABE que hay una flota enemiga: le ha llegado la noticia del desembarco o de la batalla.
    const cap0 = m.ase[e.cap]; const invadidos = [];
    for (const [pid] of cap0.not) { const p = m.paq[pid]; if (m.t - p.t > 50 || (p.k !== 'desembarco' && p.k !== 'conquista' && p.k !== 'batalla')) continue; const s = p.d.sis; if (s === undefined || invadidos.indexOf(s) >= 0) continue; if (m.sis[s].est === e.id || m.sis[s].ase.some(a => m.ase[a].est === e.id) || p.d.viejo === e.id) invadidos.push(s); }
    for (const f of flotas) {
      if (f.ordenPend && m.t - f.ordenPend < 70) continue;
      let orden = null;
      const enemigos = Array.from(e.gue.keys()).filter(j => m.est[j].vivo);
      if (enemigos.length && invadidos.length && f.st === 'base') {
        // Defender lo propio antes que atacar lo ajeno.
        const sb = m.ase[f.base].sis; const s = invadidos.slice().sort((x, y) => m.dist[sb][x] - m.dist[sb][y])[0];
        orden = { tipo: 'defender', sis: s, c: e.evGuerra };
      } else if (enemigos.length && f.base === e.cap && flotas.length >= 2 && e.rng.p(0.6)) {
        // La flota de la capital se queda en casa: alguien tiene que guardarla.
      } else if (enemigos.length) {
        // Objetivo: el asentamiento enemigo más valioso cerca de la base.
        let best = null, bs = 0; const sb = m.ase[f.base].sis;
        for (const a of m.ase) { if (enemigos.indexOf(a.est) < 0) continue; const d = m.dist[sb][a.sis]; const v = (Math.sqrt(a.pob) + (a.cap ? 250 : 0) + (a.semNec > 0 ? 80 : 0) + a.ins.length * 10) * (1 + (m.sis[a.sis].bc || 0)) / (150 + d); if (v > bs) { bs = v; best = a; } }
        if (best) orden = { tipo: 'atacar', sis: best.sis, ase: best.id, c: e.gue.get(best.est).ev };
      } else {
        // Paz: cazar piratas donde la capital ha oído que atacan.
        const cap = m.ase[e.cap]; let bs = 0.1, sis = -1;
        if (cap.rie) for (let s = 0; s < cap.rie.length; s++) if (cap.rie[s] > bs && (m.sis[s].est === e.id || m.sis[s].vec.some(v => m.sis[v.a].est === e.id))) { bs = cap.rie[s]; sis = s; }
        if (sis >= 0 && f.st === 'base') orden = { tipo: 'patrulla', sis };
      }
      if (orden) { f.ordenPend = m.t; orden.flo = f.id; orden.urg = 2; S.cor.enviar(m, e.cap, f.base, 'orden', orden, orden.c, e.id); }
    }
  };
  K('orden', 'órdenes para la flota', function (m, c) { const f = m.flo[c.d.flo]; if (!f || !f.vivo) return; f.orden = c.d; f.ordenEv = c.c; f.ordenPend = 0; });

  // ── Golpe: un general ambicioso con el ejército sin cobrar.
  Po.golpe = function (m, e, gob) {
    if (e.suc || m.t - e.desde < 2 * S.ANIO) return;
    const g = e.corte.general >= 0 ? m.per[e.corte.general] : null; if (!g || !g.vivo) return;
    const u = g.r[R.AMB] * 0.5 + (e.msc || 0) * 0.08 + (e.derrotas > 1 ? 0.15 : 0) + (e.agr > 0.55 ? 0.1 : 0) - g.r[R.LEA] * 0.45 - 0.46 + 0.1 * S.clamp((e.emp || 0) - 0.5, 0, 1) + Math.min(0.15, e.conspira || 0) - 0.3 * ((e.asabiya === undefined ? 0.6 : e.asabiya) - 0.5);
    if (e.conspira) e.conspira *= 0.95;
    if (u > 0 && g.rng.p(u * 0.25)) {
      const ev = m.reg('golpe', '{P' + g.id + '} da un golpe: el ejército lleva ' + (e.msc || 0) + ' meses sin cobrar y entra en palacio', { a: e.cap, imp: 2, d: { e: e.id } });
      P.matar(m, gob, { modo: 'publico', por: g.id, c: [ev] });
    }
  };

  // ── Atentado de una facción contra un gobernador o un gobernante.
  Po.atentado = function (m, f, obj, a) {
    const lid = m.per[f.lid]; const e = a.est >= 0 ? m.est[a.est] : null;
    const modo = lid.r[R.VAL] > lid.r[R.PRU] ? 'publico' : 'silencio';
    const pOk = S.clamp(0.2 + f.O * 0.1 + lid.hab * 0.25 - obj.r[R.PRU] * 0.3 - (a.cap ? 0.12 : 0), 0.05, 0.7);
    const ev = m.reg('atentado', f.nom + ' atenta contra {P' + obj.id + '} en {A' + a.id + '}' + (modo === 'publico' ? ', a plena luz' : ', con veneno en la cena'), { a: a.id, imp: 2, c: [f.evNace], d: { f: f.id, p: obj.id, modo } });
    if (modo === 'silencio' && obj.r[R.PRU] > 0.7) {
      // Es paranoico y tiene catador: muere el catador y se purga la cocina entera. Esos cocineros también tenían familia.
      const cat = P.crear(m, { casa: a.id, est: a.est, rol: 'civil' }); P.matar(m, cat, { modo: 'catador', c: [ev] });
      const k = 6 + f.rng.i(10); const pe = m.reg('purga_cocina', '{P' + obj.id + '} purga la cocina de palacio: ' + S.numPal(k) + ' ejecutados', { a: a.id, imp: 1, c: [ev], d: { muertos: k } });
      S.soc.muertes(m, a, m.coh[a.cohGen], k, pe, { clave: 'reg', hecho: 'masacre', culpable: obj.id });
      return;
    }
    if (f.rng.p(pOk)) { f.exitos += 2; P.matar(m, obj, { modo, por: f.lid, c: [ev] }); if (modo === 'publico') { a.senal += 0.1; a.evSenal = ev; } }
    else {
      const mil = P.crear(m, { casa: a.id, est: a.est, rol: 'civil' }); mil.fac = f.id; P.familia(m, mil);
      P.matar(m, mil, { modo: 'ejecucion', c: [ev], por: obj.id }); a.terror = (a.terror || 0) + 0.1; f.comp = Math.max(0, f.comp - 0.08);
      for (const id of f.coh) if (m.coh[id].ase === a.id) m.coh[id].agr.reg = Math.min(1, m.coh[id].agr.reg + 0.05);
    }
  };

  // ── Cadenas de vacantes: un puesto libre lo ocupa el subordinado mejor valorado; debajo queda otro hueco.
  S.gancho('pers.muere', function (m, p, ev, o) {
    const c = p.cargo; if (!c) return;
    if (c.t === 'gobernante') { const e = m.est[c.id]; if (e.vivo && e.gob === p.id) { e.gob = -1; Po.sucesion(m, e, ev, o.modo === 'publico' ? 'publico' : 'silencio', o); } return; }
    if (c.t === 'gobernador') { const a = m.ase[c.id]; if (a.gob === p.id) { a.gob = -1; a.evVacante = ev; a.nombrando = 0; } return; }
    const e = p.est >= 0 ? m.est[p.est] : null;
    const sube = (rol, nom, casa) => {
      const jefe = e && e.gob >= 0 ? m.per[e.gob] : null; let best = null, bs = -1;
      for (let i = 0; i < 3; i++) { const x = { hab: p.rng.t01(0.55, 0.18), car: p.rng.t01(), ideo: (jefe ? jefe.ideo : p.ideo).map(v => S.clamp(v + p.rng.n(0, 0.35), -1, 1)) }; const s = x.hab + (jefe ? S.sim5(x.ideo, jefe.ideo) : 0.5) + 0.5 * x.car; if (s > bs) { bs = s; best = x; } }   // mérito percibido + afinidad + padrinos
      const q = P.crear(m, { casa, est: p.est, rol, car: best.car, not: p.not, cargo: { t: c.t, id: c.id, nom } }); q.hab = best.hab; q.ideo = best.ideo;
      m.reg('ascenso', '{P' + q.id + '} ocupa el puesto de {P' + p.id + '} (' + nom + '); su hueco lo llena otro, y así hasta el fondo', { a: casa, imp: 0, c: [ev] });
      return q;
    };
    if (e && e.vivo && e.corte) for (const k of ['general', 'ministro', 'arzobispo', 'guardia']) if (e.corte[k] === p.id) { e.corte[k] = sube(p.rol, c.nom, e.cap).id; if (k === 'general') { const u = m.ase[e.cap].uni; if (u >= 0) m.uni[u].cmd = e.corte[k]; } return; }
    if (c.t === 'almirante') { const f = m.flo[c.id]; if (f && f.vivo && f.alm === p.id) f.alm = sube('almirante', c.nom, f.base).id; return; }
    if (c.t === 'comisario' || c.t === 'inspector') { const a = m.ase[c.id]; if (a[c.t] === p.id) a[c.t] = sube(p.rol, c.nom, a.id).id; return; }
    if (c.t === 'jefe_calidad') { const i = m.ins[c.id]; if (i.jefe === p.id) i.jefe = sube('jefe_calidad', c.nom, i.ase).id; }
  });

  // ── Sucesión: U_j(c) = α·afinidad + β·promesas·P(c gana) + δ·deuda honrada − ε·riesgo de purga.
  // P(c gana) = poder_c² / Σ poder² (como la ley de Lanchester).
  Po.pGana = function (poderes) { let t = 0; for (const p of poderes) t += p * p; return poderes.map(p => t > 0 ? p * p / t : 1 / poderes.length); };
  Po.desenlace = function (cuotas, conTropas) { // cuotas del poder activo, ordenadas de mayor a menor
    if (cuotas[0] > 0.5) return 'sucesion';
    if (cuotas.length >= 2 && cuotas[0] >= 0.25 && cuotas[1] >= 0.25 && conTropas) return 'guerra_civil';
    if (cuotas[0] < 0.25) return 'fragmentacion';
    return 'sucesion';
  };
  Po.sucesion = function (m, e, evMuerte, modo, o) {
    if (e.suc || !e.vivo) return;
    o = o || {}; const cap = m.ase[e.cap]; const def = S.reg.gob[e.tipoGob];
    for (const k of ['general', 'ministro', 'arzobispo', 'guardia']) if (e.corte[k] >= 0 && !m.per[e.corte[k]].vivo) e.corte[k] = cortesano(m, e, m.per[e.corte[k]].rol, m.per[e.corte[k]].cargo ? m.per[e.corte[k]].cargo.nom.split(' de ')[0] : k).id;
    const pzs = Po.piezas(m, e).filter(p => p.lid >= 0 && m.per[p.lid].vivo);
    const pub = modo === 'publico';
    // De qué estaba hecho su poder: personal (la Guardia obedecía a la persona), de Estado o popular.
    if (e.base === 'personal') { e.piG = pub ? 0.25 : 0.7; e.piE = pub ? 0.5 : 0.8; }
    else if (e.base === 'estado') { e.piG = pub ? 0.7 : 0.9; e.piE = pub ? 0.8 : 0.95; }
    else { e.piG = 0.9; e.piE = 0.9; }
    if (pub) { cap.senal += 0.10; cap.evSenal = evMuerte; }
    const g = pzs.find(p => p.t === 'guardia');
    if (g && e.base === 'personal') { const huye = pub ? 0.6 : 0.2; pzs.push({ t: 'guardia', nom: 'Guardia que huye (' + Math.round(huye * 100) + ' %)', lid: -1, poder: g.poder * huye, lado: 'desaparece', ase: e.cap }); g.poder *= 1 - huye; g.nom = 'Guardia que se queda (' + Math.round((1 - huye) * 100) + ' %)'; }
    // Candidatos.
    const cands = []; const add = (p, leg, pz) => { if (p && p.vivo && !cands.some(c => c.p === p.id)) cands.push({ p: p.id, leg, pz, propio: pz ? pz.poder : 0.05 }); };
    if (o.candidato !== undefined) add(m.per[o.candidato], 0.3, null);
    if (def.hereditario && e.corte.heredero >= 0) add(m.per[e.corte.heredero], 0.25, null);
    if (e.base === 'estado') { const mi = pzs.find(p => p.t === 'ministerios'); if (mi) add(m.per[mi.lid], 0.15, mi); }
    for (const p of pzs) if (p.lid >= 0 && p.t !== 'casa' && p.t !== 'iglesia' && m.per[p.lid].r[R.AMB] >= 0.5) add(m.per[p.lid], 0, p);
    if (!cands.length) { const top = pzs.filter(p => p.lid >= 0).sort((a, b) => b.poder - a.poder)[0]; if (top) add(m.per[top.lid], 0, top); }
    cands.sort((a, b) => (b.propio * (0.5 + m.per[b.p].r[R.AMB]) + b.leg) - (a.propio * (0.5 + m.per[a.p].r[R.AMB]) + a.leg));
    cands.length = Math.min(cands.length, 3);
    if (!cands.length) return Po.caer(m, e, evMuerte);
    for (const c of cands) { c.poder = c.propio; if (c.pz) c.pz.lado = c.p; }
    e.suc = { t0: m.t, ev: evMuerte, modo, cands, pzs, martir: e.base === 'popular' && pub };
    const ev = m.reg('interregno', 'Muerto ' + (o.por >= 0 ? 'a manos de {P' + o.por + '}' : '') + ' el gobernante de {E' + e.id + '}, empieza la subasta: ' + cands.map(c => '{P' + c.p + '}').join(', ') + ' se disputan el poder', { a: cap.id, imp: 2, c: [evMuerte], d: { e: e.id } });
    e.suc.evI = ev;
    S.inf.crear(m, evMuerte >= 0 ? evMuerte : ev, 'muerte_gob', cap.id, 1, 1, { e: e.id, modo });
    let maxD = 2;
    pzs.forEach((p, i) => { if (p.lado) return; const d = m.ase[p.ase].sis === cap.sis ? 0.5 + i * 0.01 : m.dist[cap.sis][m.ase[p.ase].sis] / Po.VEL_CORREO + 1; maxD = Math.max(maxD, d); m.prog(m.t + Math.min(d, 40), 'suc.decide', { e: e.id, i, su: e.suc }); });
    m.prog(m.t + Math.min(maxD, 40) + 2, 'suc.fin', { e: e.id, su: e.suc });
  };
  Po.utilSuc = function (m, e, pz, c) {
    const j = m.per[pz.lid], cd = m.per[c.p]; const su = e.suc;
    const rel = j.rel.get(cd.id); const afin = 0.5 * S.sim5(j.ideo, cd.ideo) + 0.5 * (rel ? (rel.op + 1) / 2 : 0.5) + (j.casa === cd.casa ? 0.05 : 0);
    const pg = Po.pGana(su.cands.map(x => x.poder))[su.cands.indexOf(c)];
    let prom = 0.5, deuda = 0, riesgo = (1 - afin) * cd.r[R.REN] * 0.8;
    if (pz.t === 'guardia') { prom = cd.ideo[0] > 0.2 ? 0.9 : 0.25; riesgo += cd.ideo[0] < 0 ? 0.5 : 0; }         // sangre en las manos: van con quien promete mantener sus privilegios
    else if (pz.t === 'casa') { const honra = 0.25 + 0.5 * cd.r[R.LEA] + (cd.ideo[1] > 0 ? 0.2 : 0); deuda = Math.min(1, pz.deuda / 150000) * honra; prom = 0.3; }
    else if (pz.t === 'flota' || pz.t === 'ejercito') prom = 0.45 + (cd.rol === 'general' || cd.rol === 'almirante' ? 0.3 : 0) + c.leg;
    else if (pz.t === 'gobernador') { prom = 0.4 + (cd.ideo[0] < 0 ? 0.3 : 0) + (cd.rol === 'ministro' ? 0.2 : 0); const a = m.ase[pz.ase]; if (a.H > 0.15) prom += cd.rol === 'ministro' ? 0.3 : -0.1; }
    else if (pz.t === 'iglesia') prom = 0.2 + 0.7 * cd.r[R.FE];
    else if (pz.t === 'ministerios') prom = 0.4 + (cd.ideo[1] > 0 ? 0.25 : 0) + c.leg;
    return 0.35 * afin + 0.40 * prom * (0.4 + pg) + 0.5 * deuda - 0.25 * riesgo;
  };
  S.en('suc.decide', function (m, x) {
    const e = m.est[x.e]; if (e.suc !== x.su) return; const pz = e.suc.pzs[x.i]; if (pz.lado) return;
    const j = m.per[pz.lid]; if (!j.vivo) { pz.lado = 'desaparece'; return; }
    const us = e.suc.cands.map(c => Po.utilSuc(m, e, pz, c));
    let b = 0, s = -1; for (let i = 1; i < us.length; i++) if (us[i] > us[b]) b = i; for (let i = 0; i < us.length; i++) if (i !== b && (s < 0 || us[i] > us[s])) s = i;
    // Quien tiene poco que ganar y mucho que perder espera: esperar da información.
    if (s >= 0 && us[b] - us[s] < 0.02 + 0.07 * j.r[R.PRU] && !x.final) { pz.lado = pz.t === 'casa' && j.r[R.COD] > 0.75 ? 'ambos' : 'espera'; return; }
    pz.lado = e.suc.cands[b].p; e.suc.cands[b].poder += pz.poder;
  });
  S.en('suc.fin', function (m, x) {
    const e = m.est[x.e]; const su = e.suc; if (su !== x.su || !e.vivo) return;
    for (let i = 0; i < su.pzs.length; i++) if (!su.pzs[i].lado) S.man['suc.decide'](m, { e: e.id, i, su });
    let activo = 0; for (const c of su.cands) activo += c.poder;
    su.cands.sort((a, b) => b.poder - a.poder);
    const cuotas = su.cands.map(c => c.poder / activo); su.cuotas = cuotas;
    const tropas = (c) => su.pzs.some(p => p.lado === c.p && p.tropas);
    const res = Po.desenlace(cuotas, su.cands.length >= 2 && tropas(su.cands[0]) && tropas(su.cands[1]));
    const tabla = su.pzs.map(p => ({ nom: p.nom, poder: p.poder, lado: p.lado }));
    const pct = (i) => Math.round(cuotas[i] * 100) + ' %';
    if (res === 'sucesion') {
      const g = m.per[su.cands[0].p];
      const ev = m.reg('sucesion', '{P' + g.id + '} reúne el ' + pct(0) + ' del poder y se hace con {E' + e.id + '}' + (su.modo === 'silencio' ? ': una sucesión ordenada' : ''), { a: e.cap, imp: 3, c: [su.evI, su.ev], d: { e: e.id, tabla, cuotas } });
      Po.coronar(m, e, g, ev);
      // Purgas: los que apostaron por otro.
      for (const p of su.pzs) { if (p.lid < 0 || p.lado === g.id || typeof p.lado !== 'number') continue; const q = m.per[p.lid]; if (q.vivo && q.id !== g.id && g.rng.p(g.r[R.REN] * 0.6)) P.matar(m, q, { modo: 'purga', por: g.id, c: [ev] }); }
      for (const c of su.cands.slice(1)) { const q = m.per[c.p]; if (q.vivo && g.rng.p(0.3 + g.r[R.REN] * 0.5)) P.matar(m, q, { modo: 'purga', por: g.id, c: [ev] }); }
      if (su.martir) Po.cruzada(m, e, su, ev);
    } else if (res === 'guerra_civil') {
      Po.guerraCivil(m, e, su, tabla);
    } else {
      const ev = m.reg('fragmentacion', 'Nadie reúne ni una cuarta parte del poder en {E' + e.id + '}: el territorio se rompe en señores de la guerra', { a: e.cap, imp: 3, c: [su.evI, su.ev], d: { e: e.id, tabla, cuotas } });
      const g = m.per[su.cands[0].p]; Po.coronar(m, e, g, ev);
      const capSis = m.ase[e.cap].sis;
      for (const p of su.pzs) if (p.t === 'gobernador' && p.lado !== g.id && m.per[p.lid].vivo) Po.secesion(m, e, [p.ref], m.per[p.lid], 'senorio', ev);
      void capSis;
    }
    e.suc = null; e.ultSuc = { t: m.t, tabla, cuotas, res, cands: su.cands.map(c => c.p) };
  });
  Po.coronar = function (m, e, g, ev) {
    const cap = m.ase[e.cap];
    for (const k of ['general', 'ministro', 'arzobispo', 'guardia']) if (e.corte[k] === g.id) e.corte[k] = -1;
    if (g.cargo && g.cargo.t === 'almirante') { const f = m.flo[g.cargo.id]; if (f && f.alm === g.id) f.alm = P.crear(m, { casa: f.base, est: e.id, rol: 'almirante', cargo: { t: 'almirante', id: f.id, nom: 'almirante de la ' + f.nom } }).id; }
    if (g.cargo && g.cargo.t === 'gobernador') { const a = m.ase[g.cargo.id]; if (a.gob === g.id) { a.gob = -1; a.evVacante = ev; } }
    g.rol = 'gobernante'; g.cargo = { t: 'gobernante', id: e.id, nom: Po.titulo(e, g) + ' de ' + cap.nom }; g.en = { t: 'A', id: cap.id }; g.not = 1; g.est = e.id;
    e.gob = g.id; e.desde = m.t; e.nGob = (e.nGob || 1) + 1;
    for (const k of ['general', 'ministro', 'arzobispo']) if (e.corte[k] < 0 || !m.per[e.corte[k]].vivo) e.corte[k] = cortesano(m, e, k, ({ general: 'general del ejército', ministro: 'ministro de Abastos', arzobispo: 'arzobispo' })[k]).id;
    if (e.guardia && (e.corte.guardia < 0 || !m.per[e.corte.guardia].vivo)) e.corte.guardia = cortesano(m, e, 'jefe_guardia', 'jefe de la Guardia').id;
    const u = cap.uni; if (u >= 0) m.uni[u].cmd = e.corte.general;
    P.familia(m, g); const her = g.fam.hijos.map(h => m.per[h]).filter(h => h.vivo && P.edad(m, h) >= 16)[0]; e.corte.heredero = her ? her.id : -1;
    // ¿Honra las deudas del régimen anterior?
    for (const d of m.deu) if (d.deudor === e.id && d.monto > 0 && g.rng.p(0.75 - 0.5 * g.r[R.LEA] - (g.ideo[1] > 0 ? 0.2 : 0))) {
      const f = m.fac[d.acreedor]; f.evQuiebra = m.reg('repudio', '{P' + g.id + '} no reconoce la deuda de {E' + e.id + '} con ' + f.nom + ' (' + S.fmt(d.monto) + ')', { a: e.cap, imp: 1, c: [ev], d: { e: e.id, f: f.id } }); d.monto = 0;
    }
    if (ev >= 0 && m.ev[ev].k === 'muerte') { /* nada */ }
    S.emitir(m, 'estado.gobernante', e, g, ev);
  };
  // Poder popular: el muerto era querido. Se convierte en mártir y su sucesor lanza una cruzada.
  Po.cruzada = function (m, e, su, ev) {
    for (const a of Po.territorio(m, e)) { for (const id of a.coh) m.coh[id].agr.reg *= 0.7; a.terror = (a.terror || 0) + 0.1; }
    const por = su.ev >= 0 && m.ev[su.ev].d ? m.ev[su.ev].d.por : -1;
    const c2 = m.reg('cruzada', '{E' + e.id + '} llora a su gobernante como a un mártir y lanza una cruzada para encontrar al culpable' + (por >= 0 ? ': {P' + por + '} es la persona más buscada del sector' : ''), { a: e.cap, imp: 2, c: [ev, su.ev], d: { e: e.id } });
    e.fiestas.push({ nom: 'Día del Mártir', dia: Math.floor(m.t % S.ANIO) });
    if (por >= 0 && m.per[por].vivo) S.jus.buscarPersona(m, e, m.per[por], c2);
    else { const v = Po.vecinos(m, e); if (v.length) { const j = v[e.rng.i(v.length)]; const r = Po.rel(m, e, j); r.cb += 1.5; r.ev = c2; } }
  };

  // Guerra civil: el Estado se parte por las piezas; las tripulaciones siguen a su casa.
  Po.guerraCivil = function (m, e, su, tabla) {
    const [c1, c2] = su.cands; const p1 = m.per[c1.p], p2 = m.per[c2.p];
    // Se queda con la capital quien tenga al ejército de la capital (o más poder).
    const ej = su.pzs.find(p => p.t === 'ejercito'); const leal = ej && ej.lado === p2.id ? p2 : p1; const reb = leal === p1 ? p2 : p1;
    const sisReb = su.pzs.filter(p => p.t === 'gobernador' && p.lado === reb.id).map(p => p.ref);
    const ev = m.reg('guerra_civil', 'Guerra civil en {E' + e.id + '}: {P' + c1.p + '} reúne el ' + Math.round(su.cuotas[0] * 100) + ' % del poder y {P' + c2.p + '} el ' + Math.round(su.cuotas[1] * 100) + ' %. ' + su.pzs.filter(p => p.lado === 'espera').map(p => p.nom).join(' y ') + (su.pzs.some(p => p.lado === 'espera') ? ' aún no han elegido bando' : ''), { a: e.cap, imp: 3, c: [su.evI, su.ev], d: { e: e.id, tabla, cuotas: su.cuotas } });
    Po.coronar(m, e, leal, ev);
    if (!sisReb.length) { const casa = m.ase[reb.casa]; if (casa && casa.est === e.id && casa.sis !== m.ase[e.cap].sis) sisReb.push(casa.sis); }
    if (!sisReb.length) { if (reb.vivo) P.matar(m, reb, { modo: 'purga', por: leal.id, c: [ev] }); return; }
    const e2 = Po.secesion(m, e, sisReb, reb, e.tipoGob === 'autocracia' ? 'republica' : e.tipoGob, ev, 'Protectorado de ');
    // Flotas: por almirante; dentro de cada flota, cada tripulación mira dónde está su casa.
    for (const p of su.pzs) if (p.t === 'flota' && p.lado === reb.id) Po.pasarFlota(m, m.flo[p.ref], e2, ev);
    let pasan = 0;
    for (const id of e.flo.slice()) { const f = m.flo[id]; for (const nid of f.nav.slice()) { const n = m.nav[nid]; const casa = m.ase[n.casaTrip]; if (casa && casa.est === e2.id && n.rng.p(0.75)) { Po.pasarNave(m, n, e2, ev); pasan++; } } }
    if (pasan) m.reg('motin_flota', 'Las tripulaciones de ' + pasan + ' escuadras nacieron en tierras de {P' + reb.id + '}: se amotinan y se pasan a su bando', { a: e2.cap, imp: 2, c: [ev], d: { e: e2.id } });
    Po.declarar(m, e, e2.id, { civil: true, motivo: 'no reconoce a {P' + reb.id + '}', c: ev });
    // Los indecisos: deciden más tarde, cuando vean quién parece que gana.
    const ind = su.pzs.filter(p => p.lado === 'espera' && p.lid >= 0);
    for (const p of ind) m.prog(m.t + e.rng.r(50, 130), 'suc.indeciso', { e: e.id, e2: e2.id, pz: p, ev });
  };
  S.en('suc.indeciso', function (m, x) {
    const e = m.est[x.e], e2 = m.est[x.e2]; if (!e.vivo || !e2.vivo || !e.gue.has(e2.id)) return;
    const pz = x.pz; const j = m.per[pz.lid]; if (!j || !j.vivo) return;
    const f1 = (e.navG || 1), f2 = (e2.navG || 1); const pg = Po.pGana([f1 + e.victorias * 2, f2 + e2.victorias * 2]);
    const a1 = S.sim5(j.ideo, m.per[e.gob].ideo), a2 = S.sim5(j.ideo, m.per[e2.gob].ideo);
    const va = j.rng.soft([0.4 * a1 + 0.6 * pg[0], 0.4 * a2 + 0.6 * pg[1]], 0.08) === 1;
    const ev = m.reg('indeciso', '{P' + j.id + '} (' + pz.nom + ') elige por fin: va con {E' + (va ? e2.id : e.id) + '}', { a: pz.ase, imp: 2, c: [x.ev], d: { e: e.id, e2: e2.id } });
    if (!va) return;
    if (pz.t === 'flota' && m.flo[pz.ref].est === e.id) Po.pasarFlota(m, m.flo[pz.ref], e2, ev);
    else if (pz.t === 'gobernador') for (const id of m.sis[pz.ref].ase) if (m.ase[id].est === e.id) Po.cambiarDueno(m, m.ase[id], e2.id, ev, { pacifico: true });
    else if (pz.t === 'iglesia') { for (const a of Po.territorio(m, e)) { a.senal += 0.06; a.evSenal = ev; } e2.piG = 1; }
    else if (pz.t === 'ejercito') { e.derrotas += 2; e2.victorias += 2; }
  });
  Po.pasarNave = function (m, n, e2, ev) {
    const f0 = m.flo[n.flo]; if (f0) { const i = f0.nav.indexOf(n.id); if (i >= 0) f0.nav.splice(i, 1); }
    let f = e2.flo.map(i => m.flo[i]).find(x => x.vivo && x.st === 'base'); if (!f) f = Po.nuevaFlota(m, e2, m.ase[e2.cap]);
    n.est = e2.id; n.dueno = { t: 'E', id: e2.id }; n.pin.push(e2.id); n.flo = f.id; f.nav.push(n.id); n.cn.push(ev);
    if (n.st !== 'viaje' && n.vivo) { n.en = f.base; n.sisEn = -1; n.st = 'atracada'; }
  };
  Po.pasarFlota = function (m, f, e2, ev) {
    const e = m.est[f.est]; const i = e.flo.indexOf(f.id); if (i >= 0) e.flo.splice(i, 1);
    f.est = e2.id; e2.flo.push(f.id); f.orden = null; f.mis = null;
    if (m.ase[f.base].est !== e2.id) f.base = e2.cap;
    for (const id of f.nav) { const n = m.nav[id]; n.est = e2.id; n.dueno = { t: 'E', id: e2.id }; n.pin.push(e2.id); n.cn.push(ev); }
    const alm = m.per[f.alm]; if (alm) alm.est = e2.id;
  };

  // ── Cambios de dueño.
  Po.cambiarDueno = function (m, a, nuevo, ev, o) {
    o = o || {}; const viejo = a.est; if (viejo === nuevo) return;
    a.est = nuevo; a.cap = false; a.revuelta = -1; a.perdon = 0;
    const gob = a.gob >= 0 ? m.per[a.gob] : null;
    if (gob && gob.vivo && !o.conGobernador) { gob.cargo = null; gob.rol = 'exiliado'; a.gob = -1; if (viejo >= 0 && m.est[viejo].vivo) gob.en = { t: 'A', id: m.est[viejo].cap }; }
    if (a.gob < 0 && nuevo >= 0 && !o.sinGobernador) { const e = m.est[nuevo]; const p = P.crear(m, { casa: a.id, est: nuevo, rol: 'gobernador', ideo: e.gob >= 0 ? m.per[e.gob].ideo : undefined, cargo: { t: 'gobernador', id: a.id, nom: 'gobernador de ' + a.nom } }); a.gob = p.id; }
    if (a.gob >= 0) m.per[a.gob].est = nuevo;
    if (a.uni >= 0) { const u = m.uni[a.uni]; u.est = nuevo; u.msc = 0; u.cmd = a.gob; if (!o.pacifico) { u.n = Math.max(150, Math.round(a.pob * 0.025)); u.ori = nuevo >= 0 ? [{ a: m.est[nuevo].cap, f: 1 }] : [{ a: a.id, f: 1 }]; } }
    if (!o.pacifico && viejo >= 0) { a.ocup = { t: m.t, de: viejo }; a.extId = nuevo; }
    a.control = 0.7; a.ultInf = m.t; a.censo = { pob: a.pob * 0.8, t: m.t }; a.f = So().RAD;
    for (const k of ['comisario', 'inspector']) if (a[k] >= 0) m.per[a[k]].est = nuevo;
    Po.refrescarSistemas(m);
    if (viejo >= 0) { const ev0 = m.est[viejo]; if (ev0.vivo && ev0.cap === a.id) { const mis = Po.territorio(m, ev0); if (mis.length) Po.mudarCapital(m, ev0, mis, ev); else Po.caer(m, ev0, ev); } else if (ev0.vivo && !Po.territorio(m, ev0).length) Po.caer(m, ev0, ev); }
    S.emitir(m, 'asent.dueno', a, viejo, nuevo, ev);
  };
  const So = () => S.soc;
  Po.mudarCapital = function (m, e, mis, ev) {
    const c = mis.slice().sort((x, y) => y.pob - x.pob)[0]; const vieja = e.cap; e.cap = c.id; m.ase[vieja].cap = false; c.cap = true;
    if (c.gob >= 0) { const g = m.per[c.gob]; if (g.vivo) { g.cargo = null; g.rol = 'ministro'; } c.gob = -1; }
    if (e.gob >= 0) m.per[e.gob].en = { t: 'A', id: c.id };
    for (const k in e.corte) if (e.corte[k] >= 0) m.per[e.corte[k]].en = { t: 'A', id: c.id };
    for (const id of e.flo) { const f = m.flo[id]; if (m.ase[f.base].est !== e.id) f.base = c.id; }
    e.derrotas += 2;
    m.reg('capital', '{E' + e.id + '} pierde {A' + vieja + '} y traslada su capital a {A' + c.id + '}', { a: c.id, imp: 2, c: [ev], d: { e: e.id } });
  };
  // Un trozo del Estado se va con un líder: provincias rebeldes, guerras civiles, señores de la guerra.
  Po.secesion = function (m, e, sistemas, lider, tipo, ev, prefijo) {
    const ases = []; for (const s of sistemas) for (const id of m.sis[s].ase) if (m.ase[id].est === e.id && id !== e.cap) ases.push(m.ase[id]);
    if (!ases.length) return null;
    const cap = ases.slice().sort((x, y) => y.pob - x.pob)[0];
    const def = S.reg.gob[tipo];
    const e2 = Po.nuevoEstado(m, { cap: cap.id, tipo, nom: (prefijo || def.nom + ' de ') + cap.nom, col: S.COLORES_ESTADO[m.est.length % S.COLORES_ESTADO.length], tes: 20000, origen: e.id, ev });
    for (const a of ases) { const conGob = a.gob === lider.id || (a.gob >= 0 && a.id !== cap.id); Po.cambiarDueno(m, a, e2.id, ev, { pacifico: true, conGobernador: conGob && a.id !== cap.id, sinGobernador: a.id === cap.id }); }
    cap.cap = true; if (cap.gob >= 0) { cap.gob = -1; }
    Po.fundarCorte(m, e2, lider.id);
    if (cap.uni >= 0) m.uni[cap.uni].cmd = e2.corte.general;
    for (const a of ases) { a.control = 1; a.censo = { pob: a.pob, t: m.t }; }
    e2.mon.P = e.mon.P; e2.mon.Ppaga = e.mon.P; e2.tec = e.tec;
    const r = Po.rel(m, e, e2.id); r.cb += 1.2; r.ev = ev; const r2 = Po.rel(m, e2, e.id); r2.cb += 0.6; r2.ev = ev;
    Po.refrescarSistemas(m);
    return e2;
  };
  // Media ciudad en la calle y la guarnición con ella: el asentamiento deja de obedecer.
  S.gancho('revolucion', function (m, a, ev) {
    const e = m.est[a.est]; const gob = a.gob >= 0 ? m.per[a.gob] : null;
    S.inf.crear(m, ev, 'revolucion', a.id, 0.9, 1, { e: e.id });
    const fac = m.fac.find(f => f.vivo && f.etapa === 'inst' && f.enem.k === 'reg' && f.enem.id === e.id && (f.sede === a.id || f.cel.some(c => c.ase === a.id)));
    let lider = fac ? m.per[fac.lid] : null;
    if (!lider || !lider.vivo) lider = P.crear(m, { casa: a.id, est: a.est, rol: 'lider', car: a.rng.r(0.7, 0.95), ideo: m.coh[a.cohGen].ideo });
    if (e.cap === a.id) {
      // La capital: cae el régimen entero.
      const g = e.gob >= 0 ? m.per[e.gob] : null;
      if (g && g.vivo) { P.matar(m, g, { modo: 'publico', por: lider.id, c: [ev], candidato: lider.id, txt: '{P' + g.id + '} es arrastrado fuera de palacio por la multitud' }); }
      return;
    }
    if (gob && gob.vivo) { if (a.agr > 0.6 && a.rng.p(0.6)) P.matar(m, gob, { modo: 'publico', por: lider.id, c: [ev] }); }
    // Si ya hay una comuna vecina nacida del mismo Estado, se une a ella.
    const hermana = m.est.find(x => x.vivo && x.origen === e.id && x.tipoGob === 'comuna' && m.saltos[m.ase[x.cap].sis][a.sis] <= 2);
    const sis = [a.sis];
    if (hermana) { for (const id of m.sis[a.sis].ase) if (m.ase[id].est === e.id && id !== e.cap) Po.cambiarDueno(m, m.ase[id], hermana.id, ev, { pacifico: true }); }
    else {
      const e2 = Po.secesion(m, e, sis, lider, 'comuna', ev);
      if (e2 && fac) { fac.estado = e2.id; e2.nom = 'Comuna de ' + a.nom + ' (' + fac.nom + ')'; e2.fiestas.push({ nom: 'Día de la Calle', dia: Math.floor(m.t % S.ANIO) }); }
    }
    for (const id of a.coh) m.coh[id].agr.reg = 0.2;
  });
  S.gancho('noticia.revolucion', function (m, a, p) { if (a.est === p.d.e) { a.senal += 0.08; a.evSenal = p.ev; } });
  S.gancho('noticia.muerte_gob', function (m, a, p) { if (a.est === p.d.e && !a.cap && p.d.modo === 'publico') { a.senal += 0.10; a.evSenal = p.ev; } });

  // Un Estado que cae: su moneda no vale nada, sus deudas mueren y sus corsarios se quedan sin patente.
  Po.caer = function (m, e, ev) {
    if (!e.vivo) return; e.vivo = false;
    const c = m.reg('estado_cae', '{E' + e.id + '} deja de existir. Sus billetes (' + e.mon.nom + ') ya no valen nada', { a: e.cap, imp: 3, c: [ev], d: { e: e.id } });
    for (const d of m.deu) if (d.deudor === e.id && d.monto > 0) { const f = m.fac[d.acreedor]; f.caja -= d.monto * 0.5; f.evQuiebra = c; d.monto = 0; }
    let cors = 0;
    for (const n of m.nav) {
      if (!n.vivo) continue;
      if (n.patente === e.id) { n.patente = -1; n.pirata = true; n.est = -1; cors++; let b = null, bd = Infinity; for (const a of m.ase) if (a.sinley) { const d = m.dist[S.nav.sisDe(m, n) >= 0 ? S.nav.sisDe(m, n) : 0][a.sis]; if (d < bd) { bd = d; b = a; } } if (b) n.base = b.id; n.origenPirata = c; if (n.cap >= 0) { m.per[n.cap].rol = 'pirata'; } }
      else if (n.dueno.t === 'E' && n.dueno.id === e.id && n.flo < 0) { n.dueno = { t: 'P', id: n.cap }; n.est = -1; if (n.cls === 'correo' || n.cls === 'censo' || n.cls === 'granelero') n.st = 'abandonada'; }
    }
    if (cors) m.reg('patentes', 'Los ' + cors + ' corsarios con patente de {E' + e.id + '} descubren que su patente ya no vale nada: de la noche a la mañana son piratas', { a: e.cap, imp: 1, c: [c] });
    for (const id of e.flo) { const f = m.flo[id]; f.vivo = false; for (const nid of f.nav) { const n = m.nav[nid]; n.flo = -1; n.pirata = true; n.est = -1; n.dueno = { t: 'P', id: -1 }; let b = null, bd = Infinity; const s0 = S.nav.sisDe(m, n); for (const a of m.ase) if (a.sinley) { const d = m.dist[s0 >= 0 ? s0 : 0][a.sis]; if (d < bd) { bd = d; b = a; } } if (b) n.base = b.id; n.origenPirata = c; m.prog(m.t + 1, 'nave.decide', { n: n.id }); } f.nav = []; }
    for (const o of m.est) { o.gue.delete(e.id); }
    S.emitir(m, 'estado.cae', e, c);
  };

  S.sismografo('estados', 'Estados vivos', m => { let k = 0; for (const e of m.est) if (e.vivo) k++; return k; });
  S.sismografo('guerras', 'Guerras en curso', m => { let k = 0; for (const e of m.est) if (e.vivo) k += e.gue.size; return k / 2; });
  S.sismografo('control', 'Control medio de las provincias', m => { let s = 0, k = 0; for (const a of m.ase) if (a.est >= 0 && !a.cap) { s += a.control; k++; } return k ? s / k : 1; });
  S.gancho('sismo', function (m) { for (const e of m.est) if (e.vivo) { m.serie('T' + e.id, e.tes); m.serie('P' + e.id, e.mon.P); m.serie('W' + e.id, e.navG || 0); } });
})(typeof globalThis !== 'undefined' ? globalThis : this);
