// Naves: cada una con casco, motor (huella) y núcleo propios. Se mueven por eventos (llegada programada),
// no por ticks. Las conductas van por registro: mercader, pirata, correo, granelero, carroñero, censo…
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const NB = S.NB; const R = S.R;
  const Nv = S.nav = {};
  const C = (id, f) => S.def('conducta', id, { decide: f });

  Nv.crear = function (m, cls, o) {
    const def = S.reg.nave[cls]; const id = m.nav.length; const rng = m.rngDe('N', id);
    const n = Object.assign({
      id, cls, nom: S.gen.nombreNave(m, def.guerra), col: rng.pesos(S.CASCOS.map(c => c.p)), casco: -1, motor: -1, nuc: -1,
      dueno: { t: 'P', id: -1 }, cap: -1, trip: def.trip, est: -1, flo: -1, carga: null, nucs: [], pas: 0, pasDe: -1, dinero: 0,
      con: new Map(), not: new Map(), saca: [], en: -1, sisEn: -1, ruta: null, st: 'atracada', dan: 0, cic: [], pin: [], cn: [], rng,
      base: -1, impago: 0, fuel: def.fuel * 1400, cascos: def.cascos ? Math.max(1, Math.round(def.cascos * rng.r(0.8, 1.25))) : 1, pirata: false, patente: -1, vivo: true, tPaga: m.t, conv: -1, x: 0, y: 0, inmov: false, llega: null, botin: 0,
    }, o || {});
    if (n.base < 0) n.base = n.en;
    if (n.est >= 0) n.pin.push(n.est);
    const sn = 1000 + rng.i(9000);
    n.casco = S.obj.crear(m, 'casco', { serie: 'C-' + sn + '-' + id, nave: id, donde: { t: 'N', id } }).id;
    n.motor = S.obj.crear(m, 'motor', { serie: 'M-' + (1000 + rng.i(9000)) + '-' + id, firma: S.obj.firma(rng), donde: { t: 'N', id } }).id;
    if (n.cap >= 0) { const p = m.per[n.cap]; p.en = { t: 'N', id }; p.cargo = { t: 'capitan', id, nom: (n.pirata ? 'capitán pirata de la ' : 'capitán de la ') + n.nom }; }
    m.nav.push(n); S.emitir(m, 'nave.creada', n); return n;
  };
  Nv.conducta = function (n) {
    if (n.pirata) return 'pirata';
    if (n.conducta) return n.conducta;
    if (n.flo >= 0) return 'flota';
    if (n.patente >= 0) return 'corsario';
    return { carguero: 'mercader', correo: 'correo', granelero: 'granelero', granja: 'navegranja', carronero: 'carronero', censo: 'censo' }[n.cls] || 'quieta';
  };
  Nv.fuerza = (n) => S.reg.nave[n.cls].ef * (1 - 0.6 * n.dan);   // por casco

  Nv.posAsent = function (m, a, t, out) {
    const s = m.sis[a.sis]; const ang = a.orb.fase + a.orb.vel * t;
    out = out || {}; out.x = s.x + Math.cos(ang) * a.orb.r; out.y = s.y + Math.sin(ang) * a.orb.r; return out;
  };
  Nv.pos = function (m, n, t, out) {
    out = out || {};
    if (n.en >= 0) { Nv.posAsent(m, m.ase[n.en], t, out); out.ang = 0; return out; }
    const r = n.ruta;
    if (r && n.st === 'viaje') {
      const u = S.clamp((t - r.t0) / (r.t1 - r.t0), 0, 1) * r.L; const P = r.pts; let k = 1;
      while (k < r.len.length - 1 && r.len[k] < u) k++;
      const a = P[k - 1], b = P[k]; const seg = r.len[k] - r.len[k - 1]; const f = seg > 0 ? (u - r.len[k - 1]) / seg : 0;
      out.x = a.x + (b.x - a.x) * f; out.y = a.y + (b.y - a.y) * f; out.ang = Math.atan2(b.y - a.y, b.x - a.x); return out;
    }
    out.x = n.x; out.y = n.y; out.ang = 0; return out;
  };
  Nv.distancia = function (m, sa, sb) { return sa === sb ? 22 : m.dist[sa][sb] + 22; };
  Nv.sisDe = (m, n) => n.en >= 0 ? m.ase[n.en].sis : n.sisEn;

  // Viaja a un asentamiento ({t:'A'}) o a un sistema ({t:'S'}, para acechar, rescatar o montar guardia).
  Nv.viajar = function (m, n, dest, o) {
    o = o || {};
    const def = S.reg.nave[n.cls]; const s0 = Nv.sisDe(m, n); const s1 = dest.t === 'A' ? m.ase[dest.id].sis : dest.id;
    if (s0 < 0) return false;
    // Una tormenta cierra un sistema: nadie traza una ruta que lo cruce.
    for (let s = s0; ;) { if (m.sis[s].bloqueo > m.t) return false; if (s === s1) break; s = m.sig[s][s1]; if (s < 0) return false; }
    const d = Nv.distancia(m, s0, s1);
    const fuel = d * def.fuel;
    if (n.en >= 0) {
      const a = m.ase[n.en];
      a.demHoy[B.comb] += fuel;                                                 // lo que este puerto necesita para su tráfico
      if (!o.sinFuel) { Nv.repostar(m, n, a, fuel); if (n.fuel < fuel) return false; }   // sin combustible no se zarpa
      if (dest.t === 'A') S.cor.cargar(m, n, a, dest.id);
    }
    if (!o.sinFuel) n.fuel = Math.max(0, n.fuel - fuel);
    const p0 = Nv.pos(m, n, m.t); const pts = [{ x: p0.x, y: p0.y }]; const cruces = [];
    let s = s0; let acc = 0;
    while (s !== s1) { s = m.sig[s][s1]; if (s < 0) return false; if (s !== s1 || dest.t === 'S') pts.push({ x: m.sis[s].x, y: m.sis[s].y }); cruces.push(s); }
    const dur = d / (o.vel || def.vel) + 0.15;
    if (dest.t === 'A') { const pf = Nv.posAsent(m, m.ase[dest.id], m.t + dur); pts.push({ x: pf.x, y: pf.y }); }
    else if (pts.length === 1) pts.push({ x: m.sis[s1].x + n.rng.r(-14, 14), y: m.sis[s1].y + n.rng.r(-14, 14) });
    else { const u = pts[pts.length - 1]; u.x += n.rng.r(-14, 14); u.y += n.rng.r(-14, 14); }
    const len = [0]; for (let i = 1; i < pts.length; i++) { acc += Math.hypot(pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y); len.push(acc); }
    n.ruta = { pts, len, L: acc, t0: m.t, t1: m.t + dur, dest, de: n.en, st: o.st || 'acecho' };
    if (n.sisEn >= 0) Nv.salirSis(m, n);
    n.en = -1; n.st = 'viaje';
    // Cada sistema que cruza es un punto donde puede haber alguien esperando.
    if (!o.sinCruce) for (let i = 0; i < cruces.length; i++) m.prog(m.t + dur * (i + 0.8) / (cruces.length + 0.2), 'nave.cruce', { n: n.id, sis: cruces[i], r: n.ruta });
    n.llega = m.prog(m.t + dur, 'nave.llega', { n: n.id });
    return true;
  };
  // Depósito: reposta donde hay; si el puerto anda corto, se raciona (lo justo para el viaje, o un quinto de lo que quede).
  Nv.tanque = (n) => S.reg.nave[n.cls].fuel * 1400;
  Nv.repostar = function (m, n, a, need) {
    const quiere = Nv.tanque(n) - n.fuel; if (quiere <= 0.01) return;
    // Solo llena el depósito donde el combustible está a buen precio; si está caro, lo justo para el viaje.
    const hay = a.alm[B.comb]; const barato = a.pr[B.comb] <= S.bien[B.comb].pref * 1.25 || n.dueno.t === 'E';
    const toma = Math.min(quiere, hay, Math.max(need - n.fuel, barato ? hay * 0.2 : 0));
    if (toma <= 0) return;
    a.alm[B.comb] -= toma; n.fuel += toma;
    if (n.dueno.t !== 'E') n.dinero -= toma * a.pr[B.comb]; else if (n.est >= 0) m.est[n.est].gasto += toma * a.pr[B.comb];
  };
  Nv.salirSis = function (m, n) { const s = m.sis[n.sisEn]; const i = s.pir.indexOf(n.id); if (i >= 0) s.pir.splice(i, 1); n.sisEn = -1; };
  // Deja de acechar (huye o vuelve con el botín) pero sigue en ese sistema hasta que zarpe.
  Nv.dejarAcecho = function (m, n, d) { const s = m.sis[n.sisEn]; if (s) { const i = s.pir.indexOf(n.id); if (i >= 0) s.pir.splice(i, 1); } n.st = 'huida'; m.prog(m.t + (d || 0.1), 'nave.decide', { n: n.id }); };

  S.en('nave.cruce', function (m, e) {
    const n = m.nav[e.n]; if (!n.vivo || n.ruta !== e.r) return;
    const s = m.sis[e.sis]; s.traf = (s.traf || 0) + 1;
    S.emitir(m, 'cruce', n, s);
    if (s.pir.length) S.emitir(m, 'nave.cruce', n, s);
  });
  S.en('nave.llega', function (m, e) {
    const n = m.nav[e.n]; if (!n.vivo || n.llega !== e) return;
    const r = n.ruta; n.ruta = null; n.llega = null; n.conv = -1;
    if (r.dest.t === 'A') {
      const a = m.ase[r.dest.id]; n.en = a.id; n.st = 'atracada';
      Nv.atracar(m, n, a);
      m.prog(m.t + n.rng.r(0.4, 1.2), 'nave.decide', { n: n.id });
    } else {
      const s = m.sis[r.dest.id]; const p = r.pts[r.pts.length - 1]; n.x = p.x; n.y = p.y; n.sisEn = s.id; n.st = r.st; n.tSis = m.t;
      if (n.st === 'acecho') s.pir.push(n.id);
      S.emitir(m, 'nave.llegaSis', n, s);
      m.prog(m.t + (n.st === 'rescate' ? 3 : n.rng.r(14, 26)), 'nave.decide', { n: n.id });
    }
  });
  Nv.atracar = function (m, n, a) {
    a.atr.push({ n: n.id, t: m.t, col: n.col }); if (a.atr.length > 80) a.atr.shift();
    S.inf.anotarPrecios(m, a);
    S.inf.atracar(m, n, a, n.cap >= 0 ? 1 - m.per[n.cap].r[R.PRU] * 0.6 : 0.5);
    S.cor.descargar(m, n, a);
    S.inf.mezclar(n.con, a.tabla); n.tMezcla = m.t;
    S.emitir(m, 'nave.atraca', n, a);
  };
  S.en('nave.decide', function (m, e) {
    const n = m.nav[e.n]; if (!n.vivo || n.st === 'viaje' || n.st === 'batalla') return;
    const c = S.reg.conducta[Nv.conducta(n)]; if (c) c.decide(m, n, n.en >= 0 ? m.ase[n.en] : null);
  });
  const luego = (m, n, d) => m.prog(m.t + d, 'nave.decide', { n: n.id });

  // ── Destrucción: queda un pecio con su caja negra, y lo que llevaba se pierde con ella.
  Nv.destruir = function (m, n, o) {
    if (!n.vivo) return -1;
    o = o || {}; n.vivo = false;
    const p = Nv.pos(m, n, m.t); const sis = n.en >= 0 ? m.ase[n.en].sis : (n.sisEn >= 0 ? n.sisEn : Nv.sisCercano(m, p));
    if (n.sisEn >= 0) Nv.salirSis(m, n);
    if (n.llega) { n.llega.x = true; n.llega = null; }
    const a = n.en;
    const ev = o.ev !== undefined ? o.ev : m.reg(o.k || 'nave_destruida', o.txt || 'La {N' + n.id + '} se pierde con toda su tripulación', { c: o.c, a, s: sis, imp: o.imp === undefined ? (S.reg.nave[n.cls].guerra ? 0 : 1) : o.imp, d: { n: n.id } });
    n.cn.push(ev); n.evFin = ev; n.st = 'pecio'; n.x = p.x + n.rng.r(-6, 6); n.y = p.y + n.rng.r(-6, 6); n.sisPecio = sis; n.en = -1; n.ruta = null;
    m.sis[sis].pecios.push(n.id);
    if (n.saca.length) {
      const of = n.saca.filter(c => c.of >= 0);
      if (of.length) m.reg('correo_perdido', 'Con la {N' + n.id + '} se pierden ' + n.saca.length + ' cartas: ' + of.slice(0, 3).map(c => S.reg.carta[c.k] ? S.reg.carta[c.k].nom : c.k).join(', '), { c: [ev], s: sis, imp: 1, d: { cartas: of.map(c => c.k) } });
      n.saca = [];
    }
    for (const id of [n.nuc, n.casco, n.motor]) if (id >= 0) { const ob = m.obj[id]; S.obj.anotar(m, ob, ev); if (ob.tipo === 'nucleo') { ob.vivo = false; if (ob.falla) ob.falla.x = true; } }
    for (const id of n.nucs) { m.obj[id].vivo = false; S.obj.anotar(m, m.obj[id], ev); }
    n.nucs = []; n.carga = null; n.extra = [];
    if (n.cap >= 0 && m.per[n.cap].vivo && !o.capVive) S.per.matar(m, m.per[n.cap], { modo: o.modo || 'combate', c: [ev], por: o.por });
    if (n.flo >= 0) { const f = m.flo[n.flo]; const i = f.nav.indexOf(n.id); if (i >= 0) f.nav.splice(i, 1); }
    S.emitir(m, 'nave.destruida', n, ev, o);
    return ev;
  };
  Nv.sisCercano = function (m, p) { let b = 0, bd = Infinity; for (const s of m.sis) { const d = (s.x - p.x) * (s.x - p.x) + (s.y - p.y) * (s.y - p.y); if (d < bd) { bd = d; b = s.id; } } return b; };

  // Un núcleo que revienta: en vuelo se lleva la nave; atracada, se lleva también a los que estaban cargando.
  S.gancho('nucleo.revienta.nave', function (m, n, o, causas) {
    if (!n || !n.vivo) return;
    if (n.cascos > 1) {
      // En una escuadra revienta un casco, no todos: el buque insignia.
      n.cascos--; n.nuc = -1;
      const ev = m.reg('explosion', 'Revienta el núcleo ' + o.interno + ' del buque insignia de la {N' + n.id + '}: se pierde con sus ' + S.reg.nave[n.cls].trip + ' tripulantes', { c: causas, a: n.en, s: S.nav.sisDe(m, n) >= 0 ? S.nav.sisDe(m, n) : Nv.sisCercano(m, Nv.pos(m, n, m.t)), imp: 1, d: { n: n.id, o: o.id } });
      S.obj.anotar(m, o, ev); n.cn.push(ev); S.emitir(m, 'explosion.nucleo', o, ev, null, n);
      return;
    }
    const a = n.en >= 0 ? m.ase[n.en] : null;
    const ev = m.reg('explosion', 'El núcleo ' + o.interno + ' de la {N' + n.id + '} revienta' + (a ? ' en el muelle de {A' + a.id + '}, en mitad de una carga' : ' en ruta'), { c: causas, a: a ? a.id : -1, s: a ? a.sis : Nv.sisCercano(m, Nv.pos(m, n, m.t)), imp: a ? 2 : 1, d: { n: n.id, o: o.id } });
    S.obj.anotar(m, o, ev);
    Nv.destruir(m, n, { ev, modo: 'explosion' });
    if (a) {
      const cohs = a.coh.map(c => m.coh[c]).filter(c => c.ins >= 0 && c.n > 20); const co = cohs.length ? n.rng.el(cohs) : m.coh[a.cohGen];
      const k = Math.min(co.n - 1, 4 + n.rng.i(22));
      if (k > 0) { m.ev[ev].txt += '; mueren ' + S.numPal(k) + ' trabajadores de ' + (co.ins >= 0 ? '{I' + co.ins + '}' : 'los muelles'); m.ev[ev].d.muertos = k; S.soc.muertes(m, a, co, k, ev, { clave: o.robado ? 'crim' : 'pat', hecho: 'explosion', nucleo: o.id, nave: n.id }); }
    }
    S.emitir(m, 'explosion.nucleo', o, ev, a, n);
  });
  S.gancho('nucleo.revienta.inst', function (m, i, o, causas) {
    if (!i || i.salud <= 0.05) return;
    const a = m.ase[i.ase]; i.nuc = -1;
    const ev = S.eco.danarInst(m, i, { k: 'explosion', dano: 0.6 + a.rng.r(0, 0.3), muertos: a.rng.r(0.03, 0.14), txt: 'El núcleo ' + o.interno + ' revienta en {I' + i.id + '}: mueren {n} trabajadores', c: causas, clave: o.robado ? 'crim' : 'pat', hecho: 'explosion', nucleo: o.id });
    S.obj.anotar(m, o, ev);
    S.emitir(m, 'explosion.nucleo', o, ev, a, null);
  });

  // Inspección de muelle: detector de grietas de 1 mm. Un inspector corrupto firma sin mirar.
  S.gancho('nave.atraca', function (m, n, a) {
    if (n.nuc < 0 || a.inspector < 0 || n.pirata) return;
    const o = m.obj[n.nuc]; if (!o.vivo || m.t - o.insp < 60) return;
    const ins = m.per[a.inspector]; if (!ins.vivo) return;
    const corrupto = ins.r[R.COD] > 0.8 && ins.r[R.PRU] < 0.5;
    if (corrupto) { o.insp = m.t - 30; if (n.dueno.t !== 'E') { n.dinero -= 150; ins.din += 150; } o.inspFalsa = (o.inspFalsa || 0) + 1; o.evInspFalsa = o.evInspFalsa !== undefined ? o.evInspFalsa : m.reg('inspeccion_falsa', '{P' + ins.id + '} firma la inspección de la {N' + n.id + '} sin abrir el reactor', { a: a.id, imp: 0, d: { p: ins.id } }); return; }
    if (!S.obj.inspeccionar(m, o)) return;
    const ev = m.reg('grieta', 'El inspector de {A' + a.id + '} encuentra una grieta de ' + S.obj.grieta(o, m.t).toFixed(1) + ' mm en el núcleo ' + o.interno + ' de la {N' + n.id + '}', { a: a.id, imp: 0, c: [o.evDano], d: { o: o.id } });
    S.obj.anotar(m, o, ev);
    if (!Nv.cambiarNucleo(m, n, a, ev)) {
      // No hay núcleo de repuesto: el capitán pesa la prisa contra el miedo (como Tobías con el número interno).
      const cap = n.cap >= 0 ? m.per[n.cap] : null; const prisa = S.clamp(1 - n.dinero / 20000, 0.1, 1), miedo = cap ? cap.r[R.PRU] : 0.5;
      if (n.dueno.t !== 'E' && prisa * 0.6 - miedo * 0.5 > 0) { o.evDano = m.reg('vuela_con_grieta', 'No hay núcleos en {A' + a.id + '} y la {N' + n.id + '} tiene deudas: zarpa con la grieta', { a: a.id, imp: 0, c: [ev, a.evSinNucleos] }); S.obj.anotar(m, o, o.evDano); }
      else n.inmov = true;
    }
  });
  Nv.cambiarNucleo = function (m, n, a, ev) {
    if (!a.nuc.length) return false;
    const precio = S.eco.precioNucleo(a); if (n.dueno.t !== 'E' && n.dinero < -6000) return false;
    const viejo = n.nuc >= 0 ? m.obj[n.nuc] : null; if (viejo) { viejo.vivo = false; if (viejo.falla) { viejo.falla.x = true; viejo.falla = null; } }
    const nu = m.obj[a.nuc.shift()]; nu.donde = { t: 'N', id: n.id }; n.nuc = nu.id; S.obj.activar(m, nu, 3);
    if (n.dueno.t !== 'E') n.dinero -= precio; else if (n.est >= 0) m.est[n.est].gasto += precio;
    const e2 = m.reg('nucleo_cambiado', 'La {N' + n.id + '} monta el núcleo ' + nu.serie + ' en {A' + a.id + '}', { a: a.id, imp: 0, c: [ev] });
    S.obj.anotar(m, nu, e2); n.inmov = false;
    if (nu.robado && !nu.borrado) n.cn.push(e2);
    return true;
  };

  S.gancho('nave.botada', function (m, a, i, p, nuc) {
    const def = S.reg.nave[p.cls];
    // La calidad de cada componente depende del oficio del astillero y de lo que faltó mientras se construía.
    const n = Nv.crear(m, p.cls, Object.assign({ en: a.id, est: p.est === undefined ? a.est : p.est, dinero: p.dinero || 0, q: def.guerra ? S.tec.calidad(a.rng, i.exp, p.falta) : undefined }, p.o || {}));
    i.exp = (i.exp || 0) + n.cascos;
    const o = m.obj[nuc]; o.donde = { t: 'N', id: n.id }; n.nuc = nuc; S.obj.activar(m, o, 3);
    m.obj[n.casco].fab = i.id;
    const ev = m.reg('botadura', '{I' + i.id + '} bota ' + (def.guerra ? 'la fragata' : 'el ' + def.nom.toLowerCase()) + ' {N' + n.id + '}', { a: a.id, imp: 0, c: [p.ev] });
    n.cn.push(ev);
    S.emitir(m, 'nave.nueva', n, p, ev);
    m.prog(m.t + 1, 'nave.decide', { n: n.id });
  });

  // ── Riesgo de una ruta según lo que se cree en el puerto (los rumores inflan el miedo).
  Nv.riesgo = function (m, a, sDest) {
    let s = a.sis, q = 1; const rie = a.rie; if (!rie) return 0;
    while (s !== sDest) { s = m.sig[s][sDest]; if (s < 0) break; q *= 1 - rie[s]; }
    return 1 - q;
  };
  S.gancho('noticia.pirata', function (m, a, p, ver) { if (!a.rie) a.rie = new Float32Array(m.sis.length); a.rie[p.d.sis] = Math.min(0.5, a.rie[p.d.sis] + 0.06 * Math.max(1, ver.x)); });
  S.gancho('asent.mes', function (m, a) { if (a.rie) for (let i = 0; i < a.rie.length; i++) a.rie[i] *= 0.85; });

  // ── Mercader: decide con lo que sabe. p̂ = w·p_noticia + (1−w)·p_normal, w = e^(−edad/τ), τ = 5 días.
  Nv.TAU = 5; Nv.T_RUTA = 0.22;                    // temperatura de la softmax, relativa al mejor beneficio por día
  Nv.estimar = (pNoticia, pNormal, edad) => { const w = Math.exp(-edad / Nv.TAU); return w * pNoticia + (1 - w) * pNormal; };
  // beneficio = (p̂ − p_compra)·q − combustible − P_ataque·(carga + 0,2·nave)·aversión
  Nv.beneficio = (pHat, pCompra, q, comb, pAtaque, valorNave, aversion) => (pHat - pCompra) * q - comb - pAtaque * (pCompra * q + 0.2 * valorNave) * aversion;

  function vender(m, n, a) {
    if (n.carga) {
      const cg = n.carga; const r = S.eco.vender(m, a, cg.c, cg.q); n.dinero += r.ingreso; n.ultGanancia = r.ingreso - cg.coste; n.carga = null; S.emitir(m, 'venta', n, a, cg, r);
      if (cg.robado) a.caliente = (a.caliente || 0) + cg.q;
    }
    if (n.extra && n.extra.length) { for (const cg of n.extra) { const r = S.eco.vender(m, a, cg.c, cg.q); n.dinero += r.ingreso; S.emitir(m, 'venta', n, a, cg, r); } n.extra = []; }
    if (n.nucs.length) { for (const id of n.nucs) { const o = m.obj[id]; n.dinero += S.eco.precioNucleo(a) * 0.9; o.donde = { t: 'A', id: a.id }; a.nuc.push(id); } n.nucs = []; }
    if (n.pas > 0) {
      const de = m.ase[n.pasDe];
      if (a.id !== n.pasDe) {
        a.pob += n.pas; a.refEnt = (a.refEnt || 0) + n.pas;
        // Con los refugiados viaja su versión de quién tuvo la culpa.
        const gen = m.coh[a.cohGen]; const k = Math.min(0.04, 3 * n.pas / a.pob);
        if (de.est === a.est && n.pasCulpa === 'reg') gen.agr.reg = Math.min(1, gen.agr.reg + k);
        if (n.pasCulpa === 'ext') gen.agr.ext = Math.min(1, gen.agr.ext + k);
        if (a.refEnt > 800 && !a.refEv) a.refEv = m.reg('refugiados', 'Llegan refugiados de {A' + de.id + '} a {A' + a.id + '}: cuentan quién tuvo la culpa', { a: a.id, imp: 0, c: [de.hambruna, de.causaHambre] });
      } else de.pob += n.pas;
      n.pas = 0;
    }
  }
  function pagar(m, n, a) {
    const dias = m.t - n.tPaga; n.tPaga = m.t;
    const piezas = Math.min(a.alm[B.piezas], 0.02 * dias);
    a.alm[B.piezas] -= piezas; a.demHoy[B.piezas] += piezas;
    if (piezas < 0.02 * dias * 0.5) n.dan = Math.min(0.6, n.dan + 0.004 * dias); else n.dan = Math.max(0, n.dan - 0.01 * dias);
    n.dinero -= n.trip * 3 * dias + piezas * a.pr[B.piezas];
    if (n.dinero < 0) n.impago++; else n.impago = 0;
  }

  C('mercader', function (m, n, a) {
    if (!a) return;
    const def = S.reg.nave[n.cls]; const cap = n.cap >= 0 ? m.per[n.cap] : null;
    if (n.st === 'amarrada') {
      // Vuelve al servicio si aquí falta de algo y alguien adelanta el dinero.
      let falta = 0; for (let c = 0; c < NB; c++) if (S.eco.objetivo(a, c) > 40 && a.pr[c] > S.bien[c].pref * 1.6) falta++;
      if (falta >= 2 && n.rng.p(0.35)) { n.st = 'atracada'; n.dinero = 9000; n.tPaga = m.t; if (n.nuc >= 0 && m.obj[n.nuc].vivo) S.obj.activar(m, m.obj[n.nuc], 3); } else return luego(m, n, 60);
    }
    vender(m, n, a); pagar(m, n, a);
    if (n.impago >= 3) return Nv.motin(m, n, a);
    if (n.dueno.t === 'F' && n.dinero > 60000) { m.fac[n.dueno.id].caja += n.dinero - 40000; n.dinero = 40000; }
    else if (n.dueno.t === 'P' && cap && n.dinero > 80000) { cap.din += n.dinero - 50000; n.dinero = 50000; }
    if (n.inmov) {
      // Con el núcleo agrietado: lo cambia aquí o va al puerto más cercano donde sabe que hay repuestos.
      if (!Nv.cambiarNucleo(m, n, a, -1)) {
        let best = -1, bd = Infinity;
        for (const d of a.cerca) { const e = n.con.get(d); if (e && e.p[NB] < S.eco.P_NUCLEO * 1.6) { const x = m.dist[a.sis][m.ase[d].sis]; if (x < bd) { bd = x; best = d; } } }
        if (best >= 0 && Nv.viajar(m, n, { t: 'A', id: best })) return;
        return luego(m, n, 6);
      }
    }
    if (m.t - (n.tMezcla || 0) > 1.5) { S.inf.anotarPrecios(m, a); S.inf.mezclar(n.con, a.tabla); n.tMezcla = m.t; }
    Nv.repostar(m, n, a, 0);
    const av = 0.8 + 1.4 * (cap ? cap.r[R.PRU] : 0.5); const fondos = Math.max(0, n.dinero) + 8000;
    const ar = S.eco.ARANCEL; const pComb = a.pr[B.comb];
    const ops = [], us = [];
    // 1. Lo que cree que sacaría en cada destino por cada género, contando con que su propia carga hunde el precio.
    //    Una noticia vieja no vale nada: sin noticia fresca supone el precio normal.
    const D = []; const top = []; for (let c = 0; c < NB; c++) top.push([-1, -1, 0, 0]);
    for (let x = 0; x < a.cerca.length; x++) {
      const d = a.cerca[x]; const e = n.con.get(d); if (!e) continue;
      const b = m.ase[d]; const edad = m.t - e.t; const dist = Nv.distancia(m, a.sis, b.sis);
      if (n.est >= 0 && b.est >= 0 && n.est !== b.est && S.com.enGuerra(m, n.est, b.est)) continue;   // embargo: no se comercia con el enemigo
      const o = { d, e, sis: b.sis, dist, dias: dist / def.vel + 1.4, pA: Nv.riesgo(m, a, b.sis), sv: new Array(NB), sq: new Array(NB) };
      for (let c = 0; c < NB; c++) {
        const T = e.T[c]; if (T <= 0) { o.sq[c] = 0; o.sv[c] = 0; continue; }
        const pref = S.bien[c].pref; const q = Math.min(def.cmax, T * 0.9);
        const sNot = edad > 22 ? T : T / Math.pow(Math.max(e.p[c], 1) / pref, 1.25);   // almacén que implica la noticia
        o.sq[c] = q; o.sv[c] = (edad > 22 ? S.eco.curva(pref, T, T + q / 2) : Nv.estimar(S.eco.curva(pref, T, sNot + q / 2), S.eco.curva(pref, T, T + q / 2), edad)) * (1 - ar);
        const v = o.sv[c] * q; const t = top[c];
        if (v > t[2]) { t[1] = t[0]; t[3] = t[2]; t[0] = D.length; t[2] = v; } else if (v > t[3]) { t[1] = D.length; t[3] = v; }
      }
      D.push(o);
    }
    // 2. Comprar aquí y vender allí.
    const deCamino = new Map();
    for (let x = 0; x < D.length; x++) {
      const o = D[x]; const comb = o.dist * def.fuel * pComb;
      for (let c = 0; c < NB; c++) {
        const st = a.alm[c]; if (st < 12 || o.sq[c] <= 0 || o.sv[c] < a.pr[c] * 1.12) continue;
        const q = Math.min(o.sq[c], st * 0.6, fondos / a.pr[c]); if (q < 6) continue;
        const pb = S.eco.precio(a, c, st - q / 2) * (1 + ar);
        const ben = Nv.beneficio(o.sv[c], pb, q, comb, o.pA, def.valor, av); const U = ben / o.dias;
        if (U > 0) { ops.push({ d: o.d, c, q, pA: o.pA }); us.push(U); const dc = deCamino.get(o.d); if (!dc || ben > dc.ben) deCamino.set(o.d, { c, q, ben }); }
      }
      if (a.nuc.length > (a.prodNuc ? 5 : 2) && n.dinero > 8000) {
        const pH = S.eco.precioNucleo(a); const pHat = Nv.estimar(o.e.p[NB], S.eco.P_NUCLEO * 0.55, m.t - o.e.t);
        const k = Math.min(4, a.nuc.length - 2); const U = ((pHat * 0.9 - pH) * k - comb) / o.dias;
        if (U > 0) { ops.push({ d: o.d, c: NB, q: k, pA: o.pA }); us.push(U); }
      }
    }
    // 3. Ir a comprar donde se produce barato lo que en otro sitio se paga (dos tramos). De camino lleva lo que allí se venda.
    for (let x = 0; x < D.length; x++) {
      const o = D[x]; const e = o.e; if (m.t - e.t > 40) continue; let bu = 0;
      for (let c = 0; c < NB; c++) {
        if (e.x[c] < 2) continue; const t = top[c];
        for (let k = 0; k < 2; k++) {
          const j = t[k]; if (j < 0 || j === x) continue; const o2 = D[j];
          const q = Math.min(o2.sq[c], e.x[c] * 12); const margen = o2.sv[c] - e.p[c] * 1.1; if (margen <= 0 || q < 20) continue;
          const d2 = Nv.distancia(m, o.sis, o2.sis);
          const u = (margen * q - (o.dist + d2) * def.fuel * pComb) / ((o.dist + d2) / def.vel + 3) * 0.7;
          if (u > bu) bu = u;
        }
      }
      if (bu > 0) { const dc = deCamino.get(o.d); ops.push({ d: o.d, c: dc ? dc.c : -1, q: dc ? dc.q : 0, pA: o.pA }); us.push(bu + (dc ? dc.ben / o.dias : 0)); }
    }
    if (!ops.length) {
      n.esperas = (n.esperas || 0) + 1;
      // Sin negocio a la vista: sale a buscar noticias a otro puerto.
      if (n.esperas >= 2 && a.cerca.length && Nv.viajar(m, n, { t: 'A', id: a.cerca[n.rng.i(Math.min(8, a.cerca.length))] })) { n.esperas = 0; return; }
      return luego(m, n, n.rng.r(2, 4));
    }
    n.esperas = 0;
    let mx = 0; for (let i = 0; i < us.length; i++) if (us[i] > mx) mx = us[i];
    const o = ops[n.rng.soft(us, Math.max(30, Nv.T_RUTA * mx))];   // las rutas buenas atraen a más, pero no a todos
    const dist = Nv.distancia(m, a.sis, m.ase[o.d].sis);
    Nv.repostar(m, n, a, dist * def.fuel);
    if (n.fuel < dist * def.fuel) return luego(m, n, n.rng.r(2, 4));              // sin combustible para el viaje: espera a que llegue
    if (o.c === NB) { for (let i = 0; i < o.q; i++) { const id = a.nuc.pop(); m.obj[id].donde = { t: 'N', id: n.id, carga: true }; n.nucs.push(id); } n.dinero -= S.eco.precioNucleo(a, a.nuc.length + o.q / 2) * o.q; }
    else if (o.c >= 0) { const r = S.eco.comprar(m, a, o.c, o.q); n.dinero -= r.coste; n.carga = { c: o.c, q: r.q, coste: r.coste, ori: a.id, t: m.t, robado: false }; }
    // Carga mixta: el hueco que queda en la bodega se llena con lo que también se paga en el destino, aunque sea poco.
    const od = D.find(x => x.d === o.d);
    if (od && o.c !== NB) {
      let hueco = def.cmax - (o.c >= 0 ? o.q : 0); n.extra = [];
      for (let c = 0; c < NB && hueco > 0.5 && n.extra.length < 4; c++) {
        if (c === o.c || od.sq[c] <= 0 || a.alm[c] < 1 || od.sv[c] < a.pr[c] * 1.12) continue;
        const q = Math.min(hueco, od.sq[c], a.alm[c] * 0.5, Math.max(0, n.dinero + 4000) / a.pr[c]); if (q < 0.3) continue;
        const r = S.eco.comprar(m, a, c, q); n.dinero -= r.coste; n.extra.push({ c, q: r.q, coste: r.coste, ori: a.id, t: m.t }); hueco -= r.q;
      }
    }
    // Huir del hambre o del sitio: la gente sube a lo que zarpa.
    if (a.H > 0.35 && a.pob > 1500) { const k = Math.min(160, Math.round(a.pob * 0.004)); a.pob -= k; n.pas = k; n.pasDe = a.id; n.pasCulpa = S.soc.parteCulpa(a, 'reg') > 0.45 ? 'reg' : (S.soc.parteCulpa(a, 'ext') > 0.3 ? 'ext' : 'nat'); a.refSal = (a.refSal || 0) + k; }
    // Convoy: si la ruta da miedo, sale más a cuenta esperar a otros que van al mismo sitio.
    const sd = m.ase[o.d].sis;
    if (o.pA > 0.08) {
      let cv = a.conv.get(sd);
      if (!cv) { cv = { nav: [], dest: o.d }; a.conv.set(sd, cv); m.prog(m.t + 2.5, 'convoy.sale', { a: a.id, sis: sd, cv }); }
      cv.nav.push(n.id); n.destConv = o.d; return;
    }
    if (!Nv.viajar(m, n, { t: 'A', id: o.d })) luego(m, n, 3);
  });
  S.en('convoy.sale', function (m, e) {
    const a = m.ase[e.a]; a.conv.delete(e.sis);
    const naves = e.cv.nav.map(i => m.nav[i]).filter(n => n.vivo && n.en === a.id);
    if (!naves.length) return;
    const id = m.nConv = (m.nConv || 0) + 1; const vel = Math.min.apply(null, naves.map(n => S.reg.nave[n.cls].vel));
    const ok = [];
    for (const n of naves) { if (Nv.viajar(m, n, { t: 'A', id: n.destConv }, { vel, sinCruce: ok.length > 0 })) { n.conv = id; ok.push(n.id); } else luego(m, n, 2); }
    if (ok.length >= 3) m.reg('convoy', 'Sale un convoy de ' + ok.length + ' naves de {A' + a.id + '} hacia {S' + e.sis + '}: nadie se lo ha ordenado', { a: a.id, imp: 0 });
    (m.convoyes = m.convoyes || new Map()).set(id, ok);
  });

  // Tripulación sin cobrar: vende la nave o se echa a la piratería. Los piratas no salen de la nada.
  Nv.motin = function (m, n, a) {
    const cap = n.cap >= 0 ? m.per[n.cap] : null;
    let sinley = null, bd = Infinity; for (const b of m.ase) if (b.sinley) { const d = m.dist[a.sis][b.sis]; if (d < bd) { bd = d; sinley = b; } }
    const uP = 0.2 + (cap ? cap.r[R.VAL] * 0.4 + cap.r[R.COD] * 0.3 - cap.r[R.PRU] * 0.3 : 0.2) + (1 - a.ley) * 0.4 - bd / 2500;
    if (sinley && n.rng.soft([uP, 0.6], 0.12) === 0) {
      n.pirata = true; n.base = sinley.id; n.impago = 0; n.dinero = 2000; n.est = -1; n.dueno = { t: 'P', id: n.cap };
      const ev = m.reg('motin', 'La tripulación de la {N' + n.id + '} lleva meses sin cobrar: se amotina en {A' + a.id + '} y se echa a la piratería', { a: a.id, imp: 1, d: { n: n.id } });
      n.cn.push(ev); n.origenPirata = ev;
      if (cap) { cap.rol = 'pirata'; cap.cargo.nom = 'capitán pirata de la ' + n.nom; cap.est = -1; }
      if (!Nv.viajar(m, n, { t: 'A', id: sinley.id }, { sinFuel: true })) luego(m, n, 3);
    } else {
      // Se vende a una casa mercante: la nave sigue con su número de serie y su historia.
      const casas = m.fac.filter(f => f.vivo && f.tipo === 'casa' && f.caja > 20000);
      if (casas.length && n.rng.p(0.5)) { const f = n.rng.el(casas); f.caja -= 15000; n.dueno = { t: 'F', id: f.id }; n.dinero = 12000; n.impago = 0; n.cn.push(m.reg('venta_nave', 'La {N' + n.id + '}, arruinada, pasa a manos de {F' + f.id + '}', { a: a.id, imp: 0 })); luego(m, n, 2); }
      else {
        // Nadie la compra: queda amarrada hasta que vuelva a haber negocio.
        n.st = 'amarrada'; n.impago = 0; n.dinero = 0;
        if (n.nuc >= 0 && m.obj[n.nuc].vivo) S.obj.activar(m, m.obj[n.nuc], 0);          // reactor apagado: la grieta deja de crecer
        n.cn.push(m.reg('amarrada', 'La {N' + n.id + '} queda amarrada en {A' + a.id + '}: no hay negocio que pague a la tripulación', { a: a.id, imp: 0 }));
        luego(m, n, 60);
      }
    }
  };

  // ── Correo del Estado: lleva las sacas adonde más urge y recorre las provincias.
  C('correo', function (m, n, a) {
    if (!a) return;
    const e = n.est >= 0 ? m.est[n.est] : null; if (!e || !e.vivo) return luego(m, n, 30);
    let dest = S.cor.urgencia(m, a, n);
    if (dest < 0 || dest === a.id) {
      if (a.id !== e.cap && n.rng.p(0.6)) dest = e.cap;
      else { const prov = m.ase.filter(b => b.est === e.id && b.id !== a.id && b.sis !== a.sis); if (prov.length) { n.circ = ((n.circ || 0) + 1 + n.rng.i(2)) % prov.length; dest = prov[n.circ].id; } }
    }
    if (dest < 0 || dest === a.id || !Nv.viajar(m, n, { t: 'A', id: dest })) luego(m, n, 1.5);
  });

  // ── Granelero del Estado: mueve el tributo de los graneros a donde la capital cree que hace falta.
  C('granelero', function (m, n, a) {
    if (!a) return;
    const e = n.est >= 0 ? m.est[n.est] : null; if (!e || !e.vivo) return luego(m, n, 30);
    if (n.carga) { a.res[n.carga.c] += n.carga.q; n.carga = null; }
    const cmax = S.reg.nave[n.cls].cmax;
    if (a.est === e.id && a.semNec > 0 && !a.cap && a.res[B.grano] > 150) {
      const q = Math.min(cmax, a.res[B.grano]); a.res[B.grano] -= q; n.carga = { c: B.grano, q, coste: 0, ori: a.id, t: m.t };
      let dest = e.cap, bh = 0.04;
      for (const [id, s] of e.sab) { const b = m.ase[id]; if (b.est === e.id && id !== a.id && s.H > bh) { bh = s.H; dest = id; } }
      if (m.ase[dest].est !== e.id) dest = e.cap;
      if (Nv.viajar(m, n, { t: 'A', id: dest })) return;
      a.res[B.grano] += q; n.carga = null; return luego(m, n, 3);
    }
    let src = -1, br = 150; for (const b of m.ase) if (b.est === e.id && b.id !== a.id && b.semNec > 0 && !b.cap && b.res[B.grano] > br) { br = b.res[B.grano]; src = b.id; }
    if (src >= 0 && Nv.viajar(m, n, { t: 'A', id: src })) return;
    luego(m, n, 8);
  });

  // ── Nave-granja: cultiva en vuelo y vende donde cree que el grano está más caro.
  C('navegranja', function (m, n, a) {
    if (!a) return;
    const def = S.reg.nave[n.cls]; const dias = m.t - (n.tCultivo || m.t); n.tCultivo = m.t;
    const q0 = (n.carga ? n.carga.q : 0) + def.cultivo * dias * (1 - n.dan);
    if (q0 > 40) { n.carga = { c: B.grano, q: Math.min(def.cmax, q0), coste: 0, ori: -1, t: m.t }; vender(m, n, a); }
    pagar(m, n, a); if (n.dinero < -20000) n.dinero = -20000;
    if (n.dueno.t === 'F' && n.dinero > 50000) { m.fac[n.dueno.id].caja += n.dinero - 30000; n.dinero = 30000; }
    const ops = [], us = [];
    for (const d of a.cerca) { const e = n.con.get(d); if (!e) continue; ops.push(d); us.push(Nv.estimar(e.p[B.grano], 100, m.t - e.t) / (Nv.distancia(m, a.sis, m.ase[d].sis) / def.vel + 3)); }
    if (!ops.length || !Nv.viajar(m, n, { t: 'A', id: ops[n.rng.soft(us, 4)] })) luego(m, n, 4);
  });

  // ── Censo: los ojos del imperio. Lo que no cuenta, no existe para la capital.
  C('censo', function (m, n, a) {
    if (!a) return;
    const e = n.est >= 0 ? m.est[n.est] : null; if (!e || !e.vivo) return luego(m, n, 30);
    if (a.est === e.id) { a.censo = { pob: a.pob, t: m.t }; }
    const prov = m.ase.filter(b => b.est === e.id && b.id !== a.id); if (!prov.length) return luego(m, n, 30);
    prov.sort((x, y) => ((x.censo ? x.censo.t : -1e9) - (y.censo ? y.censo.t : -1e9)) || x.id - y.id);
    if (!Nv.viajar(m, n, { t: 'A', id: prov[0].id })) luego(m, n, 3);
  });

  // ── Carroñero: llega días después de cada batalla, porque se enteró por las noticias.
  C('carronero', function (m, n, a) {
    if (a) {
      vender(m, n, a); pagar(m, n, a); if (n.dinero < -5000) n.dinero = -5000;
      if (n.cajas && n.cajas.length) { for (const id of n.cajas) { const o = m.obj[id]; o.donde = { t: 'A', id: a.id }; n.dinero += 1500; S.obj.anotar(m, o, m.reg('venta_caja', 'Un carroñero vende en {A' + a.id + '} la {O' + id + '}', { a: a.id, imp: 0 })); S.emitir(m, 'caja.vendida', o, a); } n.cajas = []; }
      let best = -1, bt = -1;
      for (const [id] of n.not) { const p = m.paq[id]; if ((p.k === 'batalla' || p.k === 'pecio') && (m.sis[p.d.sis].pecios.length || m.sis[p.d.sis].restos > 0) && p.t > bt && m.saltos[a.sis][p.d.sis] <= 6) { bt = p.t; best = p.d.sis; } }
      if (best < 0 && (m.sis[a.sis].pecios.length || m.sis[a.sis].restos > 0)) best = a.sis;
      if (best >= 0 && Nv.viajar(m, n, { t: 'S', id: best }, { st: 'rescate' })) return;
      if (n.rng.p(0.3) && a.cerca.length) { if (Nv.viajar(m, n, { t: 'A', id: a.cerca[n.rng.i(Math.min(6, a.cerca.length))] })) return; }
      return luego(m, n, 8);
    }
    // En el campo de restos: desguaza y arranca las cajas negras.
    const s = m.sis[n.sisEn]; let metal = 0;
    if (s.restos > 0) { const k = Math.min(s.restos, 20); s.restos -= k; metal += k * 8; }     // el campo de restos de una gran batalla da para muchos viajes
    while (s.pecios.length && metal < 240) {
      const w = m.nav[s.pecios.shift()]; w.st = 'desguazada'; metal += 60;
      const ev = m.reg('desguace', 'La {N' + n.id + '} desguaza el pecio de la {N' + w.id + '} y arranca su caja negra', { s: s.id, imp: 0, c: [w.evFin] });
      const caja = S.obj.crear(m, 'caja negra', { serie: 'CN-' + w.id, nave: w.id, donde: { t: 'N', id: n.id }, hist: w.cn.slice() }); S.obj.anotar(m, caja, ev);
      (n.cajas = n.cajas || []).push(caja.id);
    }
    if (metal > 0) n.carga = { c: B.metal, q: metal, coste: 0, ori: -1, t: m.t };
    let dest = n.base, bd = Infinity; for (const b of m.ase) if (b.tipo === 'comercial' || b.sinley || b.tipo === 'industrial') { const d = m.dist[s.id][b.sis]; if (d < bd) { bd = d; dest = b.id; } }
    if (!Nv.viajar(m, n, { t: 'A', id: dest })) luego(m, n, 5);
  });

  C('quieta', function () { });
  C('flota', function () { });

  S.sismografo('naves', 'Naves en servicio', m => { let k = 0; for (const n of m.nav) if (n.vivo) k++; return k; });
  S.sismografo('piratas', 'Naves piratas', m => { let k = 0; for (const n of m.nav) if (n.vivo && n.pirata) k++; return k; });
  S.gancho('sismo', function (m) { for (const s of m.sis) s.traf = (s.traf || 0) * 0.7; });
})(typeof globalThis !== 'undefined' ? globalThis : this);
