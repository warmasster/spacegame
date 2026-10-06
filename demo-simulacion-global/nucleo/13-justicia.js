// Crimen y castigo: piratas que salen de algún sitio, peristas que reetiquetan (o no) el número interno,
// policía que suma pistas con cocientes de verosimilitud, y vengadores con expediente y plan (GOAP).
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const R = S.R; const P = S.per; const O = S.obj; const Nv = S.nav;
  const J = S.jus = {};
  const luego = (m, n, d) => m.prog(m.t + d, 'nave.decide', { n: n.id });
  const enGuerra = (m, x, y) => x >= 0 && y >= 0 && x !== y && (m.est[x].gue.has(y) || m.est[y].gue.has(x));

  // ── Piratas.
  S.def('conducta', 'pirata', {
    decide(m, n, a) {
      const cap = n.cap >= 0 && m.per[n.cap].vivo ? m.per[n.cap] : null;
      if (!cap) { cap0(m, n); }
      if (!a) { if (!Nv.viajar(m, n, { t: 'A', id: n.base }, { sinFuel: true })) luego(m, n, 5); return; }
      if (n.golpe && n.golpe.a === a.id) return J.golpe(m, n, a);
      if (a.id !== n.base) { if (!Nv.viajar(m, n, { t: 'A', id: n.base }, { sinFuel: true })) luego(m, n, 5); return; }
      J.venderBotin(m, n, a);
      const dias = m.t - n.tPaga; n.tPaga = m.t; n.dinero -= n.trip * 4 * dias; n.dan = Math.max(0, n.dan - 0.1);
      if (n.dinero < 0) n.impago++; else n.impago = 0;
      if (n.impago >= 4) {
        n.pirata = false; n.impago = 0; n.dinero = 4000; n.dueno = { t: 'P', id: n.cap };
        const c = m.per[n.cap]; if (c && c.vivo) c.rol = 'capitan';
        n.cn.push(m.reg('banda_disuelta', 'La {N' + n.id + '} lleva meses sin botín: la banda se deshace y vuelve al comercio', { a: a.id, imp: 0, c: [n.origenPirata] }));
        return luego(m, n, 2);
      }
      const c = m.per[n.cap]; const us = [0.1], ops = [null];
      const s0 = a.sis;
      for (const s of m.sis) {
        if (s.id === s0 || m.saltos[s0][s.id] > 3) continue;
        const patr = m.flo.some(f => f.vivo && f.sis === s.id && f.st !== 'viaje' && f.nav.length) ? 0.5 : 0;
        us.push(0.15 + Math.min(0.6, (s.traf || 0) * 0.03) - patr * (0.4 + c.r[R.PRU] * 0.4) - m.saltos[s0][s.id] * 0.03); ops.push({ t: 'S', id: s.id });
      }
      if (n.dinero < 4000) for (const d of a.cerca) {
        const b = m.ase[d]; if (b.nuc.length < 4 || b.sinley) continue;
        const com = b.comisario >= 0 ? m.per[b.comisario] : null; const vend = com && com.vivo && com.r[R.COD] > 0.72;
        us.push(0.3 + c.r[R.COD] * 0.3 + c.r[R.VAL] * 0.15 - b.ley * 0.45 + (vend ? 0.3 : 0)); ops.push({ t: 'G', id: d, vend });
      }
      const k = c.rng.soft(us, 0.12); const o = ops[k];
      if (!o) return luego(m, n, 6);
      if (o.t === 'S') { if (!Nv.viajar(m, n, o, { st: 'acecho', sinFuel: true })) luego(m, n, 4); return; }
      n.golpe = { a: o.id, vend: o.vend };
      if (!Nv.viajar(m, n, { t: 'A', id: o.id }, { sinFuel: true })) { n.golpe = null; luego(m, n, 4); }
    },
  });
  function cap0(m, n) { const p = P.crear(m, { casa: n.base, est: -1, rol: 'pirata', r: { VAL: n.rng.r(0.6, 0.95) } }); n.cap = p.id; p.en = { t: 'N', id: n.id }; p.cargo = { t: 'capitan', id: n.id, nom: 'capitán pirata de la ' + n.nom }; }
  S.def('conducta', 'corsario', {
    decide(m, n, a) {
      const e = n.patente >= 0 ? m.est[n.patente] : null;
      if (!a) { if (!Nv.viajar(m, n, { t: 'A', id: n.base }, { sinFuel: true })) luego(m, n, 5); return; }
      if (!e || !e.vivo) return luego(m, n, 10);
      if (n.carga) { const r = S.eco.vender(m, a, n.carga.c, n.carga.q); n.dinero += r.ingreso; n.carga = null; }
      if (m.ase[n.base].est !== e.id) n.base = e.cap;
      const enem = Array.from(e.gue.keys()).filter(j => m.est[j].vivo);
      if (!enem.length) return luego(m, n, 15);
      // Con patente: acecha en los sistemas del enemigo.
      const obj = m.sis.filter(s => enem.indexOf(s.est) >= 0 && m.saltos[a.sis][s.id] <= 4);
      if (!obj.length || !Nv.viajar(m, n, { t: 'S', id: n.rng.el(obj).id }, { st: 'acecho' })) luego(m, n, 10);
    },
  });

  // Un mercante cruza un sistema donde alguien espera.
  S.gancho('nave.cruce', function (m, n, s) {
    if (n.pirata || n.flo >= 0 || n.cls === 'fragata') return;
    let naves = [n];
    if (n.conv >= 0 && m.convoyes && m.convoyes.has(n.conv)) naves = m.convoyes.get(n.conv).map(i => m.nav[i]).filter(x => x.vivo && x.st === 'viaje');
    if (!naves.length) return;
    let sm = 0; for (const x of naves) sm += Nv.fuerza(x);
    for (const pid of s.pir.slice()) {
      const p = m.nav[pid]; if (!p.vivo || p.st !== 'acecho' || p === n) continue;
      if (p.patente >= 0 && !enGuerra(m, p.patente, n.est)) continue;
      if (!p.rng.p(0.25)) continue;
      const sp = Nv.fuerza(p); const pw = sp * sp / (sp * sp + sm * sm);        // también aquí manda el cuadrado
      if (pw < 0.45) continue;                                                   // contra un convoy no se atreve
      const v = naves[p.rng.i(naves.length)];
      if (p.rng.p(pw)) J.abordaje(m, p, v, s);
      else { p.dan = Math.min(1, p.dan + 0.3); if (p.dan >= 0.95) Nv.destruir(m, p, { txt: 'La {N' + p.id + '}, pirata, revienta al intentar abordar a la {N' + v.id + '}', s: s.id, imp: 1, c: [p.origenPirata] }); else Nv.dejarAcecho(m, p, 0.2); }
      return;
    }
  });
  J.abordaje = function (m, p, v, s) {
    const cv = v.cap >= 0 ? m.per[v.cap] : null; const cp = p.cap >= 0 ? m.per[p.cap] : null;
    const resiste = cv && cv.r[R.VAL] > 0.75 && cv.rng.p(0.5);
    const q = v.carga ? v.carga.q : 0;
    const ev = m.reg('pirateria', 'La {N' + p.id + '}' + (p.patente >= 0 ? ', corsaria con patente de {E' + p.patente + '},' : '') + ' aborda a la {N' + v.id + '} en {S' + s.id + '}' + (q ? ' y se lleva ' + Math.round(q) + ' t de ' + S.bien[v.carga.c].nom.toLowerCase() : ''), { s: s.id, imp: 0, c: [p.origenPirata], d: { p: p.id, v: v.id } });
    s.ataques = (s.ataques || 0) + 1; p.cn.push(ev); v.cn.push(ev);
    if (resiste && p.rng.p(0.4)) {
      m.ev[ev].txt += '; la {N' + v.id + '} se resiste y es destruida'; m.ev[ev].imp = 1; m.nuevos.push(ev);
      Nv.destruir(m, v, { ev, por: cp ? cp.id : -1, modo: 'combate' });
      const a0 = m.ase[s.ase[0]]; if (a0) S.inf.crear(m, ev, 'pecio', a0.id, 0.4, 1, { sis: s.id });
    } else {
      const def = S.reg.nave[p.cls];
      if (v.carga) { p.carga = { c: v.carga.c, q: Math.min(def.cmax, v.carga.q), coste: 0, ori: v.carga.ori, t: m.t, robado: true, ev }; v.carga = null; }
      for (const id of v.nucs) { const o = m.obj[id]; o.robado = true; o.evRobo = ev; o.donde = { t: 'N', id: p.id, carga: true }; O.anotar(m, o, ev); p.nucs.push(id); }
      v.nucs = []; const d = Math.max(0, v.dinero * 0.25); v.dinero -= d; p.dinero += d;
      v.cic.push({ t: m.t, ev, lado: v.rng.el(['babor', 'estribor', 'popa']) }); v.denuncia = { ev, p: p.id, sis: s.id };
    }
    if (p.patente >= 0 && v.est >= 0) { const r = S.pol.rel(m, m.est[v.est], p.patente); r.cb += 0.15; r.ev = ev; }
    if (p.carga || p.nucs.length) Nv.dejarAcecho(m, p, 0.1);
  };
  // La víctima lo cuenta en el siguiente puerto: de ahí sale la noticia (y el miedo).
  S.gancho('nave.atraca', function (m, n, a) {
    if (!n.denuncia) return; const d = n.denuncia; n.denuncia = null;
    S.inf.crear(m, d.ev, 'pirata', a.id, 0.55, 1, { sis: d.sis, n: d.p, col: m.nav[d.p].col });
  });

  // El perista paga un tercio y reetiqueta. Si tiene deudas, trabaja rápido y no borra el número interno.
  J.decidePerista = function (per) { const prisa = S.clamp(1 - per.din / 9000, 0.1, 1), miedo = per.r[R.PRU]; return { U: miedo * 0.28 - prisa * 0.2, prisa, miedo }; };
  J.venderBotin = function (m, n, a) {
    const per = m.per.find(p => p.vivo && p.rol === 'perista' && p.casa === a.id);
    if (n.carga) {
      const cg = n.carga; const v = a.pr[cg.c] * cg.q / 3; n.dinero += v; a.alm[cg.c] += cg.q; a.caliente = (a.caliente || 0) + cg.q; n.carga = null; if (per) per.din += v * 0.3;
    }
    for (const id of n.nucs) {
      const o = m.obj[id]; n.dinero += S.eco.P_NUCLEO / 3;
      if (per) {
        const d = J.decidePerista(per); const borra = per.rng.soft([d.U, 0], 0.05) === 0;
        o.serie = (10 + per.rng.i(80)) + '-' + (1000 + per.rng.i(9000)); o.borrado = borra; o.perista = per.id; per.din += 1200;
        o.evPerista = m.reg('reetiquetado', '{P' + per.id + '} lima el número de serie del núcleo ' + o.interno + ' y graba ' + o.serie + (borra ? '; borra también el número del anillo interno' : '; tiene deudas y trabaja rápido: no borra el número grabado en el anillo interno'), { a: a.id, imp: 0, c: [o.evRobo], d: { o: id, p: per.id, borra } });
        O.anotar(m, o, o.evPerista);
      }
      o.donde = { t: 'A', id: a.id }; a.nuc.push(id);
    }
    n.nucs = [];
  };

  // El golpe: cambio de turno, diez minutos con un solo guardia en el muelle de carga.
  J.golpe = function (m, n, a) {
    const cap = m.per[n.cap]; const g = n.golpe; n.golpe = null;
    const fab = a.ins.map(i => m.ins[i]).find(i => i.tipo === 'fab_nucleos') || a.ins.map(i => m.ins[i]).find(i => i.tipo === 'astillero') || m.ins[a.ins[0]];
    const pOk = S.clamp(0.55 - a.ley * 0.35 + (g.vend ? 0.25 : 0) + cap.hab * 0.2, 0.1, 0.9);
    if (g.vend) { const com = m.per[a.comisario]; com.din += 800; n.dinero -= 400; }
    if (!a.nuc.length || !cap.rng.p(pOk)) {
      const ev = m.reg('robo_frustrado', 'Sorprenden a la tripulación de la {N' + n.id + '} en el muelle de carga de {I' + fab.id + '}: huye con las manos vacías', { a: a.id, imp: 0, c: [n.origenPirata] });
      J.caso(m, a, { tipo: 'intento de robo', ev, nave: n, grav: 0.3 });
    } else {
      const k = Math.min(4, a.nuc.length); const robados = [];
      for (let i = 0; i < k; i++) { const o = m.obj[a.nuc.pop()]; o.robado = true; o.donde = { t: 'N', id: n.id, carga: true }; n.nucs.push(o.id); robados.push(o); }
      const ev = m.reg('robo', 'Roban ' + S.numPal(k) + ' núcleos de reactor (' + robados.map(o => o.interno).join(', ') + ') de {I' + fab.id + '} en el cambio de turno de las 04:00' + (g.vend ? '; alguien vendió el horario de las patrullas' : ''), { a: a.id, imp: 1, c: [n.origenPirata], d: { n: n.id, objs: robados.map(o => o.id) } });
      for (const o of robados) {
        o.evRobo = ev; O.anotar(m, o, ev);
        // Al salir maniobra demasiado rápido: un núcleo se suelta del anclaje. No se ve nada raro.
        if (cap.rng.p(0.3)) { o.golpeado = true; const b = m.reg('golpe_bodega', 'El núcleo ' + o.interno + ' se suelta de su anclaje y golpea la pared de la bodega de la {N' + n.id + '}: microfisura de 0,2 mm que nadie ve', { a: a.id, imp: 0, c: [ev], d: { o: o.id } }); O.danar(m, o, 0.2, b, true); }
      }
      n.cn.push(ev);
      a.robados = a.robados || new Set(); for (const o of robados) a.robados.add(o.interno);
      S.inf.crear(m, ev, 'robo', a.id, 0.6, k, { series: robados.map(o => o.interno) });
      J.caso(m, a, { tipo: 'robo', ev, nave: n, grav: 0.6 });
    }
    if (!Nv.viajar(m, n, { t: 'A', id: n.base }, { sinFuel: true })) luego(m, n, 3);
  };
  S.gancho('noticia.robo', function (m, a, p) { for (const s of p.d.series) a.robados.add(s); });

  // ── Policía. Registro de huellas de motor: lo que el sensor de cada puerto ha leído al atracar cada nave.
  S.gancho('nave.atraca', function (m, n, a) {
    if (a.sensor && a.est >= 0) { const e = m.est[a.est]; e.registro = e.registro || new Map(); if (!e.registro.has(n.id)) e.registro.set(n.id, O.lectura(m.obj[n.motor].firma, S.reg.sensor[a.sensor].sigma, n.rng)); }
    if (a.busca.size && a.busca.has(n.id) && a.comisario >= 0) J.arresto(m, n, a, a.busca.get(n.id));
  });
  // odds_final = odds_inicial · Π P(pista|culpable)/P(pista|inocente). Devuelve inocentes esperados y P(culpable).
  J.bayes = function (N, pistas) { let inoc = N; for (const p of pistas) inoc *= p; return { inocentes: inoc, P: 1 / (1 + inoc) }; };
  J.P_MOTOR = { civil: 0.0103, militar: 0.000004 };
  J.caso = function (m, a, o) {
    const c = { id: m.cas.length, a: a.id, ev: o.ev, tipo: o.tipo, t: m.t, grav: o.grav, culpable: o.nave ? o.nave.id : -1, pruebas: [], sosp: [], estado: 'abierto', P: 0, obj: -1 };
    m.cas.push(c);
    if (a.comisario < 0 || !a.sensor || a.est < 0 || !o.nave) { c.estado = 'sin pruebas'; return c; }
    const e = m.est[a.est]; const reg = e.registro || new Map(); const sig = S.reg.sensor[a.sensor].sigma; const n = o.nave;
    // 1. Motor: D² = ‖m1 − m2‖²/2σ² < χ²(8; 0,99).
    const lec = O.lectura(m.obj[n.motor].firma, sig, a.rng);
    let cand = []; for (const [id, r] of reg) if (m.nav[id].vivo && O.D2(lec, r, sig) < S.CHI2_8_99) cand.push(id);
    const N = Math.max(reg.size, 50); const pistas = [J.P_MOTOR[a.sensor]];
    c.pruebas.push('huella de motor (sensor ' + S.reg.sensor[a.sensor].nom + '): ' + cand.length + ' sospechosos entre ' + reg.size + ' naves registradas');
    // 2. Testigo: denunciar es una decisión. En un barrio que odia a la policía, nadie ha visto nada.
    if (a.rng.p(S.clamp(0.9 - a.agr * 0.9, 0.1, 0.9))) {
      const col = S.CASCOS[n.col]; cand = cand.filter(id => m.nav[id].col === n.col); pistas.push(col.p);
      c.pruebas.push('un testigo vio un casco color ' + col.nom + ' (lo lleva el ' + Math.round(col.p * 100) + ' % de las naves): quedan ' + cand.length);
    } else c.pruebas.push('nadie ha visto nada');
    // 3. Atracó en la hora del robo (3 % de las naves).
    const serv = a.ins.map(i => m.ins[i]).find(i => i.tipo === 'policia');
    if (serv && serv.salud > 0.4) { const antes = cand.length; cand = cand.filter(id => a.atr.some(x => x.n === id && m.t - x.t < 2)); pistas.push(0.03); c.pruebas.push('registro del muelle a la hora del delito: de ' + antes + ' quedan ' + cand.length); }
    else c.pruebas.push('la sala de servidores no conserva las grabaciones');
    const b = J.bayes(N, pistas); c.sosp = cand; c.inocentes = b.inocentes;
    c.P = cand.length ? b.P / cand.length * Math.min(1, cand.length) : 0; if (cand.length === 1) c.P = b.P;
    if (cand.length >= 1 && c.P >= 0.5) {
      c.obj = cand[0]; c.estado = 'orden de busca';
      const ev = m.reg('busca', 'La policía de {A' + a.id + '} suma pistas (' + pistas.length + ') y pide orden de busca contra la {N' + c.obj + '}: probabilidad ' + Math.round(c.P * 100) + ' %' + (c.obj !== c.culpable ? ' (no fue ella)' : ''), { a: a.id, imp: c.obj !== c.culpable ? 1 : 0, c: [o.ev], d: { caso: c.id, n: c.obj } });
      c.evBusca = ev; a.busca.set(c.obj, c.id);
      S.inf.crear(m, ev, 'busca', a.id, 0.5, 1, { n: c.obj, caso: c.id });
    } else c.estado = cand.length ? 'lista de ' + cand.length + ' sospechosos' : 'frío';
    return c;
  };
  S.gancho('noticia.busca', function (m, a, p) { if (a.est >= 0 && a.est === m.ase[p.a].est) a.busca.set(p.d.n, p.d.caso); });
  J.arresto = function (m, n, a, casoId) {
    const c = m.cas[casoId]; const com = m.per[a.comisario]; if (!com.vivo || c.estado === 'cerrado') { a.busca.delete(n.id); return; }
    const cap = n.cap >= 0 ? m.per[n.cap] : null; if (!cap || !cap.vivo) return;
    if (com.r[R.COD] > 0.72 && n.dinero > 3000) { n.dinero -= 2500; com.din += 2500; return; }       // el comisario mira para otro lado
    if (!com.rng.p(0.75)) return;
    a.busca.delete(n.id); c.estado = 'cerrado';
    const inocente = n.id !== c.culpable; const e = m.est[a.est]; const dura = e.rep >= 0.6;
    const ev = m.reg('condena', 'Detienen a {P' + cap.id + '} al atracar la {N' + n.id + '} en {A' + a.id + '}; juicio y ' + (dura ? 'horca' : 'celda') + (inocente ? '. Era inocente: su motor se parecía demasiado' : ''), { a: a.id, imp: inocente ? 2 : 1, c: [c.evBusca, c.ev], d: { p: cap.id, inocente } });
    P.familia(m, cap);
    if (dura) P.matar(m, cap, { modo: 'ejecucion', por: com.id, c: [ev] });
    else { cap.rol = 'preso'; cap.cargo = null; const pr = e.prision !== undefined ? m.nav[e.prision] : null; if (pr && pr.vivo) { pr.presos.push(cap.id); cap.en = { t: 'N', id: pr.id }; } }
    n.cap = -1; n.pirata = false;
    const casas = m.fac.filter(f => f.vivo && f.tipo === 'casa'); if (casas.length) { const f = casas[com.rng.i(casas.length)]; n.dueno = { t: 'F', id: f.id }; const nc = P.crear(m, { casa: a.id, est: a.est, rol: 'capitan' }); n.cap = nc.id; nc.en = { t: 'N', id: n.id }; nc.cargo = { t: 'capitan', id: n.id, nom: 'capitán de la ' + n.nom }; n.dinero = 10000; }
    if (inocente && !dura) V.nacer(m, cap, com.id, ev, a, 0.7);
  };
  // Cruzada: un Estado entero busca a una persona.
  J.buscarPersona = function (m, e, p, ev) { e.buscados = e.buscados || new Map(); e.buscados.set(p.id, ev); };

  // Un núcleo revienta: el inspector busca entre los restos el anillo interno.
  S.gancho('explosion.nucleo', function (m, o, ev, a, n) {
    if (!a) return;
    const ins = a.inspector >= 0 ? m.per[a.inspector] : null; const hay = ins && ins.vivo;
    if (o.robado && !o.borrado && hay && a.robados.has(o.interno)) {
      const per = o.perista !== undefined ? m.per[o.perista] : null;
      const inv = m.reg('investigacion', '{P' + ins.id + '}, inspector de {A' + a.id + '}, cobra poco pero es honrado: entre los restos encuentra el anillo interno con el número ' + o.interno + ', que el registro marca como robado' + (per ? '. El rastro lleva hasta {P' + per.id + '}' : ''), { a: a.id, imp: 2, c: [ev, o.evRobo, o.evPerista], d: { o: o.id } });
      a.crimId = per ? per.id : -1; a.evCrim = inv;
      J.culpables(m, ev, per ? [per.id] : [], inv, o);
    } else if (o.defecto && hay) {
      const fab = m.ins[o.fab]; const jefe = fab && fab.jefe >= 0 ? m.per[fab.jefe] : null;
      const inv = m.reg('escandalo', 'Escándalo: el núcleo ' + o.interno + ' era del lote ' + o.lote + ' de {I' + o.fab + '}, que salió sin inspección' + (jefe ? '. {P' + jefe.id + '} se quedaba el dinero' : ''), { a: a.id, imp: 2, c: [ev, o.evDefecto], d: { o: o.id } });
      if (jefe && jefe.vivo) { J.culpables(m, ev, [jefe.id], inv, o); const fa = m.ase[fab.ase]; if (fa.est >= 0 && fa.comisario >= 0) m.prog(m.t + m.dist[a.sis][fa.sis] / 55 + 5, 'justicia.jefe', { p: jefe.id, ev: inv, a: fa.id }); }
    } else if (o.inspFalsa && o.evInspFalsa !== undefined) {
      const quien = m.ev[o.evInspFalsa].d.p;
      const inv = m.reg('escandalo', 'La inspección del núcleo ' + o.interno + ' estaba firmada sin abrir el reactor: {P' + quien + '} cobraba por no mirar', { a: a.id, imp: 1, c: [ev, o.evInspFalsa] });
      J.culpables(m, ev, [quien], inv, o);
    }
  });
  S.en('justicia.jefe', function (m, e) {
    const p = m.per[e.p]; if (!p.vivo) return; const a = m.ase[e.a];
    if (a.est >= 0 && m.est[a.est].rep >= 0.5) { P.matar(m, p, { modo: 'ejecucion', c: [e.ev] }); return; }
    const i = m.ins.find(x => x.jefe === p.id); p.rol = 'preso'; const nom = p.cargo ? p.cargo.nom : 'jefe de calidad'; p.cargo = null;
    if (i) i.jefe = P.crear(m, { casa: a.id, est: a.est, rol: 'jefe_calidad', cargo: { t: 'jefe_calidad', id: i.id, nom } }).id;
    m.reg('condena', '{P' + p.id + '} acaba en una celda de {A' + a.id + '}', { a: a.id, imp: 0, c: [e.ev], d: { p: p.id } });
  });
  // Señala culpables con nombre a los allegados de las víctimas de un hecho.
  J.culpables = function (m, evMuertes, ids, inv, o) {
    const vs = m.vengadores || []; for (const p of vs) { if (!p.vivo || !p.ven || p.ven.ev !== evMuertes || p.ven.obj >= 0) continue; if (ids.length) { p.ven.obj = ids[0]; p.ven.pista = inv; p.ven.nuc = o ? o.id : -1; V.anotar(m, p, 'Dicen que fue ' + m.per[ids[0]].nom + '.'); } }
  };

  // Jefe de calidad: si se queda el dinero de las inspecciones, el lote sale defectuoso.
  S.gancho('lote.nuevo', function (m, i, a) {
    const j = i.jefe >= 0 ? m.per[i.jefe] : null; i.loteDef = false; i.evLote = -1;
    if (!j || !j.vivo) return;
    const u = j.r[R.COD] * 0.6 + (j.din < 1000 ? 0.2 : 0) - j.r[R.PRU] * 0.35 - j.r[R.LEA] * 0.3;
    if (j.rng.soft([u, 0.3], 0.1) === 0) { i.loteDef = true; j.din += 4000; i.evLote = m.reg('corrupcion', '{P' + j.id + '} se queda el dinero de las inspecciones: el lote ' + i.lote + ' de {I' + i.id + '} sale sin revisar', { a: a.id, imp: 0, d: { p: j.id } }); }
  });

  // ── Venganza. G(t) = gravedad·(1 + rencor)·e^(−t/τ), τ = 150 d·(1 + 2·rencor).
  const V = S.ven = {};
  V.G = function (grav, rencor, dias) { return grav * (1 + rencor) * Math.exp(-dias / (150 * (1 + 2 * rencor))); };
  V.actual = function (m, p) { const v = p.ven; return Math.min(v.G0 * Math.exp(-(m.t - v.t0) / v.tau), 2); };
  V.anotar = function (m, p, txt) { const o = m.obj[p.ven.libreta]; o.entradas.push({ t: m.t, txt }); if (o.entradas.length > 40) o.entradas.shift(); };
  V.nacer = function (m, victima, culpable, ev, a, cercania) {
    P.familia(m, victima);
    const al = P.allegados(m, victima).filter(x => P.edad(m, x[0]) >= 10);
    if (!al.length) return null;
    m.vengadores = m.vengadores || [];
    if (m.vengadores.filter(x => x.vivo && x.ven && x.ven.estado === 'activa').length > 45) return null;
    const [p, cer] = al[0]; if (p.ven) return null;
    const grav = 0.95 * (cercania || 1) * cer; const ren = p.r[R.REN];
    p.ven = { ev, victima: victima.id, G0: grav * (1 + ren), tau: 150 * (1 + 2 * ren), t0: m.t, obj: culpable === undefined || culpable === null ? -1 : culpable, estado: 'activa', escaner: false, vuela: false, aliados: 0, donde: -1, herr: true, plan: [], paso: '', libreta: -1, aniv: m.t };
    p.ven.libreta = O.crear(m, 'libreta', { serie: 'libreta', de: p.id, entradas: [], donde: { t: 'P', id: p.id } }).id; p.inv.push(p.ven.libreta);
    if (p.rol === 'civil') p.rol = 'vengador';
    P.recordar(m, p, ev, { t: 'P', id: p.ven.obj }, 'rabia', 1, ren);
    m.vengadores.push(p);
    m.reg('rencor', '{P' + p.id + '} entierra a {P' + victima.id + '} en {A' + a.id + '} y abre una libreta', { a: a.id, imp: 0, c: [ev], d: { p: p.id } });
    return p;
  };
  // Las víctimas con nombre: cada hecho con muertos deja al menos una ficha (y una familia).
  S.gancho('muertes', function (m, a, co, n, ev, o) {
    if (n < 3 && !o.culpable) return;
    const rng = a.rng; const k = n >= 10 ? 2 : 1;
    for (let i = 0; i < k; i++) {
      const v = P.crear(m, { casa: a.id, est: a.est, rol: 'civil', nace: m.t - rng.r(24, 55) * S.ANIO, ideo: co.ideo });
      v.vivo = false; v.muere = m.t; v.evMuerte = ev; v.oficio = co.ins >= 0 ? m.ins[co.ins].nom : 'vecino';
      V.nacer(m, v, o.culpable, ev, a);
    }
  });
  // Heredar el rencor: 0,6 para una hija.
  S.gancho('pers.muere', function (m, p, ev, o) {
    if (p.ven && p.ven.estado === 'activa') {
      const v = p.ven; const G = V.actual(m, p); v.estado = 'muerta';
      const lib = m.obj[v.libreta]; lib.donde = { t: 'A', id: P.lugar(m, p) };
      const h = p.fam.hijos.map(x => m.per[x]).filter(x => x.vivo && !x.ven)[0];
      if (h && G * 0.6 >= 0.45) {
        h.ven = Object.assign({}, v, { G0: G * 0.6 * 1.0, t0: m.t, tau: 150 * (1 + 2 * h.r[R.REN]), estado: 'activa', aliados: 0, plan: [], obj: o.por >= 0 && o.modo !== 'natural' ? o.por : v.obj });
        lib.de = h.id; lib.donde = { t: 'P', id: h.id }; if (h.rol === 'civil') h.rol = 'vengador'; m.vengadores.push(h);
        m.reg('herencia', '{P' + h.id + '}, de ' + Math.floor(P.edad(m, h)) + ' años, hereda la libreta y el rencor de {P' + p.id + '}', { a: P.lugar(m, h), imp: 1, c: [ev, v.ev], d: { p: h.id } });
      } else if (o.modo === 'natural') m.reg('libreta', '{P' + p.id + '} muere sin haber encontrado a quien buscaba; la libreta queda en un cajón de {A' + lib.donde.id + '}', { a: lib.donde.id, imp: 0, c: [v.ev] });
    }
    // Matar crea viudas, y ya sabes lo que pasa con las viudas.
    if (o.por >= 0 && o.modo !== 'natural' && o.modo !== 'ejecucion' && p.not >= 0 && (o.modo === 'venganza' || o.modo === 'combate' || o.modo === 'purga' || o.modo === 'publico')) {
      if (p.rng.p(o.modo === 'combate' ? 0.35 : 0.6)) V.nacer(m, p, o.por, ev, m.ase[P.lugar(m, p)]);
    }
  });

  // Plan: acciones con condiciones en el mundo real (GOAP sobre un estado pequeño).
  const BIT = { quien: 1, dinero: 2, escaner: 4, vuela: 8, aliados: 16, donde: 32, muerto: 64 };
  V.ACC = [
    { id: 'vender_herramientas', pre: 0, no: BIT.dinero, ef: BIT.dinero, c: 1, ok: (m, p) => p.ven.herr },
    { id: 'pedir_a_la_hermandad', pre: 0, no: BIT.dinero, ef: BIT.dinero, c: 2, ok: (m, p) => m.fac.some(f => f.vivo && f.etapa === 'inst' && f.sede === p.casa && f.caja > 4000 && f.tipo !== 'casa') },
    { id: 'trabajar_y_ahorrar', pre: 0, no: BIT.dinero, ef: BIT.dinero, c: 6, ok: () => true },
    { id: 'averiguar_quien', pre: 0, no: BIT.quien, ef: BIT.quien, c: 3, ok: () => true },
    { id: 'comprar_escaner', pre: BIT.dinero, no: BIT.escaner, ef: BIT.escaner, c: 1, ok: (m, p) => m.ase[P.lugar(m, p)].alm[B.piezas] >= 1 },
    { id: 'aprender_a_volar', pre: BIT.dinero, no: BIT.vuela, ef: BIT.vuela, c: 2, ok: (m, p) => m.per.some(x => x.vivo && x.rol === 'piloto' && x.casa === P.lugar(m, p)) },
    { id: 'contratar_cazarrecompensas', pre: BIT.dinero | BIT.quien, no: BIT.aliados, ef: BIT.aliados, c: 2, ok: (m, p) => m.per.some(x => x.vivo && x.rol === 'cazador' && !x.ocupado) },
    { id: 'preguntar_en_los_muelles', pre: BIT.quien, no: BIT.donde, ef: BIT.donde, c: 3, ok: () => true },
    { id: 'emboscar', pre: BIT.quien | BIT.donde, no: BIT.muerto, ef: BIT.muerto, c: 1, ok: (m, p) => V.pGanar(m, p) >= 0.5 },
  ];
  V.estado = function (m, p) { const v = p.ven; return (v.obj >= 0 ? BIT.quien : 0) | (p.din >= 2000 ? BIT.dinero : 0) | (v.escaner ? BIT.escaner : 0) | (v.vuela ? BIT.vuela : 0) | (v.aliados > 0 ? BIT.aliados : 0) | (v.donde >= 0 ? BIT.donde : 0); };
  // Búsqueda de coste uniforme hasta «culpable muerto», usando solo acciones cuyas condiciones se cumplen hoy.
  V.planear = function (m, p) {
    const ini = V.estado(m, p); const acc = V.ACC.filter(a => a.ok(m, p));
    const dist = new Map([[ini, 0]]); const prev = new Map(); const cola = [ini];
    while (cola.length) {
      cola.sort((x, y) => dist.get(x) - dist.get(y)); const s = cola.shift();
      if (s & BIT.muerto) { const plan = []; let x = s; while (prev.has(x)) { const [ant, a] = prev.get(x); plan.unshift(a.id); x = ant; } return plan; }
      for (const a of acc) { if ((s & a.pre) !== a.pre || (s & a.no)) continue; let t = s | a.ef; if (a.id === 'comprar_escaner' || a.id === 'aprender_a_volar' || a.id === 'contratar_cazarrecompensas') t &= ~BIT.dinero; const d = dist.get(s) + a.c; if (!dist.has(t) || d < dist.get(t)) { dist.set(t, d); prev.set(t, [s, a]); cola.push(t); } }
    }
    return [];
  };
  V.fuerzaObj = function (m, q) { const c = q.cargo ? q.cargo.t : ''; return ({ gobernante: 1.4, gobernador: 0.9, general: 0.9, almirante: 0.9, comisario: 0.7, capitan: 0.6, lider: 0.6, jefe_guardia: 0.9 })[c] || (q.rol === 'pirata' ? 0.7 : q.rol === 'perista' ? 0.35 : 0.3); };
  V.pGanar = function (m, p) {
    const v = p.ven; if (v.obj < 0) return 0; const q = m.per[v.obj];
    const f = 0.3 + (v.vuela ? 0.15 : 0) + 0.3 * v.aliados + (v.escaner ? 0.2 : 0) + p.r[R.VAL] * 0.15; const o = V.fuerzaObj(m, q);
    return f * f / (f * f + o * o);
  };
  V.tic = function (m, p) {
    const v = p.ven; if (v.estado !== 'activa') return;
    if (P.edad(m, p) < 17) return;
    // Cada aniversario lo reaviva: +0,2 (tope 1).
    if (m.t - v.aniv >= S.ANIO) { v.aniv = m.t; const G = Math.min(1, V.actual(m, p) + 0.2); v.G0 = G; v.t0 = m.t; }
    const G = V.actual(m, p); const a = m.ase[P.lugar(m, p)];
    if (v.obj >= 0 && !m.per[v.obj].vivo) { v.estado = 'cumplida'; m.reg('rencor_fin', '{P' + p.id + '} se entera de que {P' + v.obj + '} ha muerto. Cierra la libreta', { a: a.id, imp: 0, c: [v.ev, m.per[v.obj].evMuerte] }); return; }
    if (G < 0.35) { v.estado = 'apagada'; m.reg('rehace', '{P' + p.id + '} rehace su vida en {A' + a.id + '}. El rencor no desaparece, pero deja de ser lo primero', { a: a.id, imp: 0, c: [v.ev] }); if (p.rol === 'vengador') p.rol = 'civil'; return; }
    if (G < 0.6) return;                                                        // ha pasado página, de momento
    v.plan = V.planear(m, p); const paso = v.plan[0]; v.paso = paso || 'esperar';
    if (!paso) { p.din += 150; return; }
    const obj = v.obj >= 0 ? m.per[v.obj] : null;
    switch (paso) {
      case 'vender_herramientas': { const q = 2200 * a.pr[B.piezas] / 420; p.din += q; v.herr = false; V.anotar(m, p, 'Vendo sus herramientas en ' + a.nom + ': ' + Math.round(q) + '.'); m.reg('vende_herramientas', '{P' + p.id + '} vende las herramientas de {P' + v.victima + '} en {A' + a.id + '}', { a: a.id, imp: 0, c: [v.ev] }); break; }
      case 'pedir_a_la_hermandad': { const f = m.fac.find(x => x.vivo && x.etapa === 'inst' && x.sede === p.casa && x.caja > 4000 && x.tipo !== 'casa'); const q = Math.min(4000, f.caja * 0.3); f.caja -= q; p.din += q; v.padrino = f.id; V.anotar(m, p, f.nom + ' pone ' + Math.round(q) + '.'); break; }
      case 'trabajar_y_ahorrar': p.din += 350; break;
      case 'averiguar_quien': {
        // Sin culpable con nombre: pregunta quién atracó aquel día. Puede equivocarse de nave.
        const ev = m.ev[v.ev]; const lug = ev.a >= 0 ? m.ase[ev.a] : a; const cand = lug.atr.filter(x => Math.abs(x.t - ev.t) < 2 && m.nav[x.n].vivo && m.nav[x.n].cap >= 0);
        if (cand.length && p.rng.p(0.3)) { const n = m.nav[p.rng.el(cand).n]; v.obj = n.cap; v.errada = true; V.anotar(m, p, 'Un estibador recuerda a la ' + n.nom + ', casco ' + S.CASCOS[n.col].nom + ', saliendo con prisa.'); m.reg('sospecha', '{P' + p.id + '} no sabe quién fue; un estibador le habla de la {N' + n.id + '}, que atracó aquel día. Ya tiene un nombre: {P' + n.cap + '}', { a: a.id, imp: 1, c: [v.ev], d: { p: p.id } }); }
        break;
      }
      case 'comprar_escaner': a.alm[B.piezas] -= 1; p.din -= 1800; v.escaner = true; V.anotar(m, p, 'Escáner de segunda mano en ' + a.nom + '.'); break;
      case 'aprender_a_volar': { const pil = m.per.find(x => x.vivo && x.rol === 'piloto' && x.casa === a.id); p.din -= 1500; pil.din += 1500; v.vuela = true; V.anotar(m, p, pil.nom + ', piloto retirado, me enseña a volar.'); break; }
      case 'contratar_cazarrecompensas': { const cz = m.per.filter(x => x.vivo && x.rol === 'cazador' && !x.ocupado).slice(0, 2); for (const c of cz) { c.ocupado = p.id; c.din += 1500; } p.din -= 3000; v.aliados = cz.length; V.anotar(m, p, 'Contrato a ' + cz.map(c => c.nom).join(' y ') + '.'); m.reg('cazarrecompensas', '{P' + p.id + '} contrata a ' + cz.map(c => '{P' + c.id + '}').join(' y ') + ' para dar con {P' + v.obj + '}', { a: a.id, imp: 0, c: [v.ev] }); break; }
      case 'preguntar_en_los_muelles': {
        // Pregunta a cada jefe de muelle; los que no se mueven de su sitio son fáciles de encontrar.
        if (obj.en.t !== 'N') { v.donde = obj.en.t === 'A' ? obj.en.id : obj.en.a; V.anotar(m, p, obj.nom + ' vive en ' + m.ase[obj.en.id].nom + '.'); break; }
        const nave = m.nav[obj.en.id]; const lugares = [a.id].concat(a.cerca.slice(0, v.vuela ? 14 : 5));
        for (const id of lugares) { const b = m.ase[id]; const x = b.atr.slice().reverse().find(y => y.n === nave.id); if (x && m.t - x.t < 45) { V.anotar(m, p, '«¿La ' + S.CASCOS[nave.col].nom + '? Atracó en ' + b.nom + ' hace ' + Math.round(m.t - x.t) + ' días.»'); v.vistas = (v.vistas || 0) + 1; if (m.t - x.t < 12 || v.vistas >= 3 || v.escaner && v.vistas >= 2) v.donde = nave.base; break; } }
        break;
      }
      case 'emboscar': {
        const lug = obj.en.t === 'A' ? obj.en.id : obj.en.t === 'V' ? obj.en.a : m.nav[obj.en.id].base; const pg = V.pGanar(m, p);
        if (p.en.t === 'V') break;                                               // aún va de camino
        if (P.lugar(m, p) !== lug) { const d = S.mas.viajar(m, p, lug); V.anotar(m, p, 'Salgo hacia ' + m.ase[lug].nom + '.'); if (d > 0) break; }
        p.en = { t: 'A', id: lug };
        const ev = m.reg('emboscada', '{P' + p.id + '} lleva ' + (Math.round((m.t - m.ev[v.ev].t) / 30) <= 1 ? 'un mes' : Math.round((m.t - m.ev[v.ev].t) / 30) + ' meses') + ' buscando a {P' + obj.id + '}. ' + (obj.sexo === 'M' ? 'La' : 'Lo') + ' espera en {A' + lug + '}' + (v.aliados ? ' con ' + v.aliados + ' cazarrecompensas' : ''), { a: lug, imp: 1, c: [v.ev, v.pista], d: { p: p.id } });
        for (const c of m.per) if (c.ocupado === p.id) c.ocupado = 0;
        if (p.rng.p(pg)) { v.estado = 'cumplida'; P.matar(m, obj, { modo: 'venganza', por: p.id, c: [ev], txt: '{P' + obj.id + '} muere a manos de {P' + p.id + '}' + (v.errada ? ', que se equivocó de hombre' : '') }); m.ev[ev].imp = 2; if (p.rol === 'vengador') p.rol = 'civil'; }
        else if (p.rng.p(0.5)) P.matar(m, p, { modo: 'combate', por: obj.id, c: [ev], txt: '{P' + p.id + '} muere intentando vengarse de {P' + obj.id + '}' });
        else { v.aliados = 0; v.donde = -1; V.anotar(m, p, 'Se me escapó.'); }
        break;
      }
    }
  };
  // Si no puede vencerlo, no ataca: busca ventaja. Eso lo decide V.ACC.emboscar.ok; aquí solo se registra el porqué.
  S.sismografo('vengadores', 'Vengadores con libreta', m => (m.vengadores || []).filter(p => p.vivo && p.ven && p.ven.estado === 'activa').length);

  // ── Predicadores y profecías.
  S.def('rol', 'predicador', {
    nom: 'predicador',
    tic(m, p) {
      if (p.fac >= 0 || !p.rng.p(0.035)) return;
      const pr = p.rng.el(S.PROFECIAS); m.profecias = m.profecias || [];
      if (m.profecias.some(x => x.p === p.id && m.t - x.t < 4 * S.ANIO)) return;
      const a = m.ase[p.casa];
      const ev = m.reg('profecia', '{P' + p.id + '}, predicador de los barrios bajos de {A' + a.id + '}, anuncia que «' + pr.txt + '». Nadie le hace caso', { a: a.id, imp: 0 });
      m.profecias.push({ p: p.id, k: pr.id, t: m.t, a: a.id, ev, casa: pr.casa });
    },
  });
  S.gancho('hecho', function (m, e) {
    const l = m.profecias; if (!l || !l.length || e.k === 'profecia') return;
    for (let i = l.length - 1; i >= 0; i--) {
      const x = l[i]; if (m.t - x.t > 6 * S.ANIO) { l.splice(i, 1); continue; }
      if (e.t <= x.t || !x.casa(e)) continue;
      const a = m.ase[x.a]; const cerca = e.a >= 0 ? (m.ase[e.a].est === a.est && a.est >= 0) || m.saltos[m.ase[e.a].sis][a.sis] <= 1 : (e.s >= 0 && m.saltos[e.s][a.sis] <= 1);
      if (!cerca) continue;
      l.splice(i, 1); m.prog(m.t + 0.5, 'profecia.cumple', { pr: x, ev: e.id });
    }
  });
  S.en('profecia.cumple', function (m, e) {
    const x = e.pr; const p = m.per[x.p]; if (!p.vivo || p.fac >= 0) return; const a = m.ase[x.a];
    const f = S.fac.nueva(m, { lid: p.id, sede: a.id, enem: { k: 'reg', id: a.est }, hecho: { ev: e.ev, clave: 'profecia', muertos: 0, s: 1 }, fund: e.ev, O: 1 });
    f.cel.push({ ase: a.id, lid: p.id, ideo: p.ideo.slice(), padre: -1, puente: 1 });
    const co = m.coh[a.cohGen]; if (co.fac < 0) { co.fac = f.id; f.coh.push(co.id); }
    f.evProto = m.reg('profecia_cumplida', 'Se cumple la profecía de {P' + p.id + '}: sus seguidores se multiplican en {A' + a.id + '}, convencidos de que fue obra de un enviado', { a: a.id, imp: 2, c: [x.ev, e.ev], d: { p: p.id } });
    S.fac.bautizar(m, f);
  });
})(typeof globalThis !== 'undefined' ? globalThis : this);
