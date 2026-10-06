// La mano: lo único que no tiene causa dentro del mundo. Cada intervención queda en el registro como raíz
// de una cadena, y el mundo le busca un culpable (porque la gente no sabe que existes).
// Las herramientas van por registro: añadir una es declarar { cat, ico, nom, obj, desc, hacer }.
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const R = S.R;
  const D = S.dios = {};
  D.CATS = ['Destruir', 'Personas', 'Pueblo', 'Dinero y rumores', 'Guerra y poder', 'Facciones'];
  // obj: sobre qué se aplica (I instalación, A asentamiento, N nave, P persona, E estado, F facción, S sistema).
  const T = (id, o) => S.def('dios', id, o);
  const lugarDe = {
    I: (m, i) => m.ins[i].ase, A: (m, a) => a, P: (m, p) => S.per.lugar(m, m.per[p]), E: (m, e) => m.est[e].cap, F: (m, f) => m.fac[f].sede,
    N: (m, n) => m.nav[n].en, S: () => -1,
  };

  D.hacer = function (m, id, ref, op) {
    const def = S.reg.dios[id]; if (!def || !D.puede(m, id, ref)) return -1;
    const a = lugarDe[def.obj](m, ref);
    const mano = m.reg('mano', 'LA MANO · ' + def.nom + ' · ' + (def.obj === 'F' ? m.fac[ref].nom || 'una protofacción' : '{' + def.obj + ref + '}'), { a: a >= 0 ? a : -1, s: def.obj === 'S' ? ref : undefined, imp: 0, d: { id, ref } });
    m.dios.push({ t: m.t, id, ref });
    const r = def.hacer(m, ref, mano, op || {});
    return r === undefined ? mano : r;
  };
  D.puede = function (m, id, ref) {
    const def = S.reg.dios[id]; if (!def) return false;
    const tabla = { I: m.ins, A: m.ase, N: m.nav, P: m.per, E: m.est, F: m.fac, S: m.sis }[def.obj]; if (!tabla[ref]) return false;
    if (def.obj === 'P' && !m.per[ref].vivo) return false; if (def.obj === 'N' && !m.nav[ref].vivo) return false;
    if ((def.obj === 'E' || def.obj === 'F') && !tabla[ref].vivo) return false;
    return !def.puede || def.puede(m, ref);
  };
  // Convierte lo que se ha tocado en el mapa en el tipo de cosa que pide la herramienta.
  D.resolver = function (m, id, ref) {
    const def = S.reg.dios[id]; if (!def || !ref) return -1; const q = def.obj; const t = ref.t;
    if (t === q) return ref.id;
    const aDe = () => t === 'A' ? m.ase[ref.id] : t === 'I' ? m.ase[m.ins[ref.id].ase] : t === 'N' && m.nav[ref.id].en >= 0 ? m.ase[m.nav[ref.id].en] : t === 'S' ? m.sis[ref.id].ase.map(i => m.ase[i]).sort((x, y) => y.pob - x.pob)[0] : t === 'P' ? m.ase[S.per.lugar(m, m.per[ref.id])] : t === 'E' ? m.ase[m.est[ref.id].cap] : t === 'F' ? m.ase[m.fac[ref.id].sede] : t === 'C' ? m.ase[ref.a] : null;
    const a = aDe();
    if (q === 'A') return a ? a.id : -1;
    if (q === 'S') return t === 'N' ? S.nav.sisDe(m, m.nav[ref.id]) : a ? a.sis : -1;
    if (q === 'E') return t === 'N' && m.nav[ref.id].dueno.t === 'E' ? m.nav[ref.id].dueno.id : a ? a.est : -1;
    if (q === 'I') { if (!a) return -1; const f = D.fabrica(m, a); return f ? f.id : -1; }
    if (q === 'P') { if (t === 'N') return m.nav[ref.id].cap; if (t === 'F') return m.fac[ref.id].lid; if (t === 'E') return m.est[ref.id].gob; if (!a) return -1; const g2 = S.pol.gobernadorDe(m, a); return g2 ? g2.id : -1; }
    if (q === 'F') { if (!a) return -1; const f = m.fac.filter(x => x.vivo && x.tipo !== 'casa' && (x.sede === a.id || x.cel.some(c => c.ase === a.id))).sort((x, y) => y.O - x.O)[0]; return f ? f.id : -1; }
    if (q === 'N') return -1;
    return -1;
  };
  D.fabrica = function (m, a) {
    const val = (i) => { const d = S.reg.inst[i.tipo]; return i.salud < 0.06 || d.indestructible ? -1 : (d.nucleos ? 100 : d.astillero ? 60 : d.sal ? 20 + i.nivel : d.cosecha ? 15 + i.nivel : 1); };
    return a.ins.map(i => m.ins[i]).filter(i => val(i) > 0).sort((x, y) => val(y) - val(x))[0] || null;
  };

  // ¿Quién tiene la culpa? El régimen elige a quién señalar, y eso tiene consecuencias.
  D.culpar = function (m, a, ev, que) {
    const e = a.est >= 0 ? m.est[a.est] : null;
    if (!e || e.gob < 0 || !m.per[e.gob].vivo) { a.culpa.nat += 1; return; }
    const gob = m.per[e.gob]; const us = [0.3 + gob.r[R.PRU] * 0.2], ops = [{ t: 'acc' }];
    for (const j of S.pol.vecinos(m, e)) { const r = S.pol.rel(m, e, j); us.push(0.18 + r.cb * 0.3 - r.op * 0.4 + gob.r[R.AMB] * 0.2 + (e.gue.has(j) ? 0.35 : 0)); ops.push({ t: 'ext', j }); }
    const f = m.fac.find(x => x.vivo && x.etapa === 'inst' && x.enem.k === 'reg' && x.enem.id === e.id && (x.sede === a.id || x.cel.some(c => c.ase === a.id)));
    if (f) { us.push(0.25 + e.rep * 0.4 + f.O * 0.1); ops.push({ t: 'reb', f }); }
    const o = ops[gob.rng.soft(us, 0.12)];
    if (o.t === 'ext') {
      const r = S.pol.rel(m, e, o.j); r.cb += 1.3; a.culpa.ext += 4; a.extId = o.j;
      for (const id of a.coh) m.coh[id].agr.ext = Math.min(1, m.coh[id].agr.ext + 0.15);
      r.ev = m.reg('culpa', '{E' + e.id + '} culpa a {E' + o.j + '} de ' + que + ': «un acto de guerra»', { a: a.id, imp: 2, c: [ev], d: { e: e.id, j: o.j } });
    } else if (o.t === 'reb') {
      const k = 4 + gob.rng.i(9);
      const pe = m.reg('culpa', '{E' + e.id + '} culpa a ' + o.f.nom + ' de ' + que + ': redadas de noche y ' + S.numPal(k) + ' fusilados', { a: a.id, imp: 2, c: [ev], d: { e: e.id, f: o.f.id } });
      a.terror = (a.terror || 0) + 0.15; S.soc.muertes(m, a, m.coh[a.cohGen], k, pe, { clave: 'reg', hecho: 'masacre', culpable: a.gob >= 0 ? a.gob : e.gob });
    } else {
      a.culpa.reg += 1.5; for (const id of a.coh) m.coh[id].agr.pat = Math.min(1, m.coh[id].agr.pat + 0.08);
      m.reg('culpa', '{E' + e.id + '} llama «accidente» a ' + que + '. En {A' + a.id + '} nadie se lo cree', { a: a.id, imp: 1, c: [ev], d: { e: e.id } });
    }
    a.senal += 0.04; a.evSenal = ev;
  };
  const sinley = (m, s0) => { let b = null, bd = Infinity; for (const a of m.ase) if (a.sinley) { const d = m.dist[s0 >= 0 ? s0 : 0][a.sis]; if (d < bd) { bd = d; b = a; } } return b; };

  // ════ Destruir ════
  T('bomba', {
    cat: 0, nom: 'Bomba', ico: '💣', obj: 'I', desc: 'Arrasa una instalación y mata a parte de su plantilla. En el mapa: la fábrica más valiosa del sitio que toques.',
    puede: (m, i) => m.ins[i].salud > 0.05 && !S.reg.inst[m.ins[i].tipo].indestructible,
    hacer(m, id, mano) {
      const i = m.ins[id]; const a = m.ase[i.ase];
      const ev = S.eco.danarInst(m, i, { k: 'bomba', dano: 1, muertos: m.rng.r(0.15, 0.45), txt: 'Una explosión sin explicación arrasa {I' + id + '}: mueren {n} trabajadores', c: [mano], clave: 'pat', hecho: 'bomba', imp: 2 });
      S.inf.crear(m, ev, 'bomba', a.id, 0.8, m.ev[ev].d.muertos, { ins: id });
      a.evEscasez = ev; S.jus.caso(m, a, { tipo: 'atentado', ev, nave: null, grav: 1 });
      D.culpar(m, a, ev, 'la explosión de ' + i.nom);
      return ev;
    },
  });
  T('bombardeo', {
    cat: 0, nom: 'Bombardeo orbital', ico: '🛰', obj: 'A', desc: 'Fuego desde la órbita sobre todo el asentamiento: daña todas las instalaciones y mata al 6 % de la gente.',
    hacer(m, id, mano) {
      const a = m.ase[id]; const k = Math.round(a.pob * 0.06);
      const ev = m.reg('bomba', 'Fuego desde la órbita sobre {A' + id + '}: arden los muelles, las fábricas y los barrios. ' + k.toLocaleString('es-ES') + ' muertos', { a: id, imp: 3, c: [mano], d: { muertos: k } });
      for (const x of a.ins) { const i = m.ins[x]; if (S.reg.inst[i.tipo].indestructible) continue; i.salud = Math.max(0, i.salud - m.rng.r(0.2, 0.75)); i.evDano = ev; const co = i.coh >= 0 ? m.coh[i.coh] : null; if (co) co.n = Math.max(1, Math.round(co.n * 0.85)); if (i.salud <= 0.05 && i.nuc >= 0) { m.obj[i.nuc].vivo = false; if (m.obj[i.nuc].falla) m.obj[i.nuc].falla.x = true; i.nuc = -1; } }
      for (let c = 0; c < S.NB; c++) a.alm[c] *= 0.6;
      a.causaHambre = ev; a.evSemilla = ev; a.evEscasez = ev;
      S.soc.muertes(m, a, m.coh[a.cohGen], Math.min(k, m.coh[a.cohGen].n - 1), ev, { clave: 'ext', hecho: 'bomba' });
      S.inf.crear(m, ev, 'bomba', id, 1, k, {}); D.culpar(m, a, ev, 'el bombardeo de ' + a.nom);
      return ev;
    },
  });
  T('asteroide', {
    cat: 0, nom: 'Lanzar un asteroide', ico: '☄', obj: 'A', desc: 'Secuestra un remolcador de asteroides del sistema y deja caer la roca. Borra media ciudad. (Hace falta un remolcador en el sistema.)',
    puede: (m, a) => m.nav.some(n => n.vivo && n.cls === 'remolcador' && S.nav.sisDe(m, n) === m.ase[a].sis),
    hacer(m, id, mano) {
      const a = m.ase[id]; const rem = m.nav.find(n => n.vivo && n.cls === 'remolcador' && S.nav.sisDe(m, n) === a.sis);
      const k = Math.round(a.pob * 0.3);
      const ev = m.reg('asteroide', 'El remolcador {N' + rem.id + '} deja caer un asteroide sobre {A' + id + '}: ' + k.toLocaleString('es-ES') + ' muertos y media ciudad borrada del mapa', { a: id, imp: 3, c: [mano], d: { muertos: k } });
      S.nav.destruir(m, rem, { ev });
      for (const x of a.ins) { const i = m.ins[x]; if (!S.reg.inst[i.tipo].indestructible && m.rng.p(0.6)) { i.salud = Math.max(0, i.salud - m.rng.r(0.5, 1)); i.evDano = ev; const co = i.coh >= 0 ? m.coh[i.coh] : null; if (co) co.n = Math.max(1, Math.round(co.n * 0.6)); } }
      for (let c = 0; c < S.NB; c++) a.alm[c] *= 0.4;
      a.causaHambre = ev; a.evSemilla = ev; a.evEscasez = ev;
      S.soc.muertes(m, a, m.coh[a.cohGen], Math.min(k, m.coh[a.cohGen].n - 1), ev, { clave: 'ext', hecho: 'bomba' });
      S.inf.crear(m, ev, 'bomba', id, 1, k, {}); D.culpar(m, a, ev, 'la caída del asteroide');
      return ev;
    },
  });
  T('destruir_nave', {
    cat: 0, nom: 'Destruir nave', ico: '💥', obj: 'N', desc: 'Con todo lo que lleva: carga, cartas y capitán. Si es una escuadra, cae entera.',
    hacer(m, id, mano) { const n = m.nav[id]; const sis = S.nav.sisDe(m, n); if (n.cascos > 1 && sis >= 0) m.sis[sis].restos = (m.sis[sis].restos || 0) + n.cascos; return S.nav.destruir(m, n, { txt: 'La {N' + id + '} (' + S.reg.nave[n.cls].nom.toLowerCase() + (n.cascos > 1 ? ', ' + n.cascos + ' naves' : '') + ') desaparece en un destello', c: [mano], imp: n.cascos > 1 ? 2 : 1 }); },
  });
  T('hundir_flota', {
    cat: 0, nom: 'Hundir media flota', ico: '🌊', obj: 'E', desc: 'Una tormenta de plasma sorprende a sus escuadras: pierde la mitad de sus naves de guerra.',
    puede: (m, e) => m.nav.some(n => n.vivo && n.flo >= 0 && n.est === e),
    hacer(m, id, mano) {
      let k = 0; const ev = m.reg('desastre_flota', 'Una tormenta de plasma sorprende a la flota de {E' + id + '}', { a: m.est[id].cap, imp: 2, c: [mano], d: { e: id } });
      for (const n of m.nav) { if (!n.vivo || n.flo < 0 || n.est !== id) continue; const q = Math.ceil(n.cascos * 0.5); n.cascos -= q; k += q; n.dan = Math.min(0.8, n.dan + 0.2); n.cn.push(ev); if (n.cascos <= 0) S.nav.destruir(m, n, { ev }); }
      m.ev[ev].txt += ': se pierden ' + k + ' naves'; m.est[id].derrotas += 2;
      S.inf.crear(m, ev, 'batalla', m.est[id].cap, 0.8, k, { sis: m.ase[m.est[id].cap].sis, lados: [id, -1], pierde: id, gana: -1 });
      return ev;
    },
  });
  T('agrietar', {
    cat: 0, nom: 'Agrietar el núcleo', ico: '🔩', obj: 'N', desc: 'Microfisura de 0,2 mm a plena tensión: fallará en unas siete semanas. Nadie lo ve.',
    puede: (m, n) => m.nav[n].nuc >= 0 && m.obj[m.nav[n].nuc].vivo,
    hacer(m, id, mano) { const n = m.nav[id]; const o = m.obj[n.nuc]; const ev = m.reg('golpe_bodega', 'Algo golpea el núcleo ' + o.interno + ' de la {N' + id + '}: microfisura de 0,2 mm que nadie ve', { imp: 0, c: [mano], d: { o: o.id } }); o.golpeado = true; S.obj.danar(m, o, 0.2, ev, true); return ev; },
  });
  T('agrietar_inst', {
    cat: 0, nom: 'Agrietar el reactor', ico: '🔧', obj: 'I', desc: 'El reactor de la instalación fallará en unas siete semanas y se llevará a parte de la plantilla.',
    puede: (m, i) => m.ins[i].nuc >= 0 && m.obj[m.ins[i].nuc].vivo,
    hacer(m, id, mano) { const i = m.ins[id]; const o = m.obj[i.nuc]; const ev = m.reg('golpe_bodega', 'Algo golpea el núcleo ' + o.interno + ' de {I' + id + '}: microfisura de 0,2 mm que nadie ve', { a: i.ase, imp: 0, c: [mano], d: { o: o.id } }); o.golpeado = true; S.obj.danar(m, o, 0.2, ev, true); return ev; },
  });
  T('apagon', {
    cat: 0, nom: 'Vaciar los depósitos', ico: '⛽', obj: 'A', desc: 'Todo el combustible del asentamiento se pierde: apagones, naves que no zarpan, flotas paradas.',
    hacer(m, id, mano) { const a = m.ase[id]; const q = a.alm[B.comb]; a.alm[B.comb] = 0; a.evEscasez = m.reg('fuga', 'Los depósitos de {A' + id + '} amanecen vacíos: ' + S.fmt(q) + ' t de combustible perdidas. Luces que parpadean en los barrios', { a: id, imp: 1, c: [mano] }); return a.evEscasez; },
  });
  T('quemar_grano', {
    cat: 0, nom: 'Quemar los graneros', ico: '🔥', obj: 'A', desc: 'Arde el grano del mercado, la reserva del Estado y la semilla. El hambre llega en semanas.',
    hacer(m, id, mano) {
      const a = m.ase[id]; const q = a.alm[B.grano] * 0.9 + a.res[B.grano] * 0.9 + a.sem * 0.7; a.alm[B.grano] *= 0.1; a.res[B.grano] *= 0.1; a.sem *= 0.3; a.acap[B.grano] *= 0.2;
      const ev = m.reg('incendio', 'Arden los graneros de {A' + id + '}: ' + S.fmt(q) + ' t de grano convertidas en humo', { a: id, imp: 2, c: [mano] });
      a.causaHambre = ev; a.evSemilla = ev; S.inf.crear(m, ev, 'bomba', id, 0.7, 0, {}); D.culpar(m, a, ev, 'el incendio de los graneros'); return ev;
    },
  });
  T('epidemia', {
    cat: 0, nom: 'Epidemia', ico: '🦠', obj: 'A', desc: 'Una fiebre recorre los barrios: muere el 8 % de la población. Nadie tiene la culpa (¿o sí?).',
    hacer(m, id, mano) {
      const a = m.ase[id]; const k = Math.round(a.pob * 0.08);
      const ev = m.reg('epidemia', 'Una fiebre recorre {A' + id + '}: ' + k.toLocaleString('es-ES') + ' muertos en un mes', { a: id, imp: 2, c: [mano], d: { muertos: k } });
      a.culpa.nat += 3; for (const c of a.coh) { const co = m.coh[c]; co.n = Math.max(1, Math.round(co.n * 0.92)); } a.pob = Math.max(200, a.pob - k);
      S.soc.hecho(m, a, ev, 'hambre', 0.8, k); return ev;
    },
  });
  T('tormenta', {
    cat: 0, nom: 'Tormenta de iones', ico: '🌀', obj: 'S', desc: 'Cierra un sistema durante 90 días: ninguna nave traza una ruta que lo cruce. Lo que dependa de esa ruta se queda sin suministro.',
    hacer(m, id, mano) { m.sis[id].bloqueo = m.t + 90; const ev = m.reg('tormenta', 'Una tormenta de iones cierra {S' + id + '}: ninguna nave podrá cruzarlo en 90 días', { s: id, imp: 2, c: [mano] }); for (const a of m.sis[id].ase) { m.ase[a].causaHambre = ev; m.ase[a].evEscasez = ev; } return ev; },
  });

  // ════ Personas ════
  T('matar_publico', { cat: 1, nom: 'Matar a la vista de todos', ico: '🗡', obj: 'P', desc: 'Muere en público: si gobernaba, el régimen parece débil. En el mapa: quien mande donde toques (o el capitán de la nave).', hacer(m, id, mano) { return S.per.matar(m, m.per[id], { modo: 'publico', c: [mano] }); } });
  T('matar_silencio', { cat: 1, nom: 'Matar en silencio', ico: '☠', obj: 'P', desc: 'Parece un infarto: la sucesión suele ser ordenada.', hacer(m, id, mano) { return S.per.matar(m, m.per[id], { modo: 'silencio', c: [mano] }); } });
  const cambia = (id, ico, nom, desc, f, txt) => T(id, { cat: 1, nom, ico, obj: 'P', desc, hacer(m, pid, mano) { const p = m.per[pid]; f(m, p); return m.reg('susurro', txt.replace('{P}', '{P' + pid + '}'), { a: S.per.lugar(m, p), imp: 0, c: [mano], d: { p: pid } }); } });
  cambia('corromper', '🤑', 'Corromper', 'Codicia al máximo, prudencia y lealtad por los suelos. Un inspector así firma sin mirar; un jefe de calidad se queda el dinero; un comisario vende los horarios.', (m, p) => { p.r[R.COD] = 0.97; p.r[R.PRU] = 0.15; p.r[R.LEA] = 0.1; }, 'Algo cambia en {P}: de pronto todo tiene un precio');
  cambia('honrar', '😇', 'Volver honrado', 'Lealtad y empatía al máximo, codicia a cero. Un gobernador así dice la verdad a la capital.', (m, p) => { p.r[R.COD] = 0.03; p.r[R.LEA] = 0.97; p.r[R.EMP] = 0.9; }, '{P} deja de aceptar sobres');
  cambia('ambicion', '👑', 'Susurrar ambición', 'Ambición y valor al máximo, lealtad a cero. Un general así da un golpe; un gobernador lejano se independiza.', (m, p) => { p.r[R.AMB] = 0.99; p.r[R.VAL] = 0.92; p.r[R.LEA] = 0.03; }, '{P} empieza a mirar el sillón de otro');
  cambia('carisma', '🗣', 'Dar el don de la palabra', 'Carisma y habilidad al máximo: si hay una queja común a su alrededor, la organizará.', (m, p) => { p.car = 0.98; p.hab = 0.95; const a = m.ase[S.per.lugar(m, p)]; if (a) a.conector = true; }, 'De pronto todo el mundo escucha a {P}');
  cambia('enriquecer', '💰', 'Enriquecer', 'Veinte mil créditos en su bolsillo. Con dinero, un plan de venganza avanza; un perista ya no tiene prisa.', (m, p) => { p.din += 20000; }, '{P} encuentra una bolsa con veinte mil créditos');
  cambia('rencor', '😡', 'Avivar el rencor', 'Rencor al máximo. Si ya buscaba venganza, vuelve a arder como el primer día.', (m, p) => { p.r[R.REN] = 0.99; if (p.ven) { p.ven.G0 = 1.8; p.ven.t0 = m.t; p.ven.tau = 450; if (p.ven.estado === 'apagada') { p.ven.estado = 'activa'; } } }, '{P} no consigue dormir');
  T('senalar', {
    cat: 1, nom: 'Señalarlo como culpable', ico: '👉', obj: 'P', desc: 'Todos los que por aquí llevan una libreta y no saben a quién buscan reciben este nombre. Aunque sea inocente.',
    puede: (m, p) => (m.vengadores || []).some(v => v.vivo && v.ven && v.ven.estado === 'activa' && v.id !== p),
    hacer(m, id, mano) {
      const p = m.per[id]; const s0 = m.ase[S.per.lugar(m, p)].sis; let k = 0;
      const ev = m.reg('sospecha', 'Corre la voz de que fue {P' + id + '}', { a: S.per.lugar(m, p), imp: 1, c: [mano], d: { p: id } });
      for (const v of m.vengadores) { if (!v.vivo || !v.ven || v.ven.estado !== 'activa' || v.id === id) continue; if (v.ven.obj >= 0 && m.saltos[m.ase[S.per.lugar(m, v)].sis][s0] > 2) continue; v.ven.obj = id; v.ven.errada = true; v.ven.donde = -1; v.ven.pista = ev; S.ven.anotar(m, v, 'Dicen que fue ' + p.nom + '.'); k++; }
      m.ev[ev].txt += ': ' + k + ' libretas apuntan su nombre'; return ev;
    },
  });

  // ════ Pueblo ════
  T('plaga', {
    cat: 2, nom: 'Plaga en las cosechas', ico: '🐛', obj: 'A', desc: 'La próxima cosecha cae un 30 %. El gobernador decidirá qué le cuenta a la capital.',
    puede: (m, a) => m.ase[a].semNec > 0,
    hacer(m, id, mano) { const a = m.ase[id]; a.plaga = Math.min(0.7, a.plaga + 0.3); a.culpa.nat += 2; a.evPlaga = m.reg('plaga', 'Una plaga arrasa los cultivos de {A' + id + '}: se perderá un ' + Math.round(a.plaga * 100) + ' % de la cosecha', { a: id, imp: 2, c: [mano], d: { plaga: a.plaga } }); S.inf.crear(m, a.evPlaga, 'plaga', id, 0.6, a.plaga * 100, {}); return a.evPlaga; },
  });
  T('cosecha_buena', {
    cat: 2, nom: 'Año de bienes', ico: '🌻', obj: 'A', desc: 'La próxima cosecha sale un 50 % mayor.', puede: (m, a) => m.ase[a].semNec > 0,
    hacer(m, id, mano) { const a = m.ase[id]; a.plaga = -0.5; return m.reg('buen_anio', 'Llueve cuando tiene que llover en {A' + id + '}: la cosecha viene enorme', { a: id, imp: 1, c: [mano] }); },
  });
  T('grano', { cat: 2, nom: 'Soltar grano', ico: '🌾', obj: 'A', desc: 'Sesenta días de comida aparecen en el almacén. Nadie sabe de quién.', hacer(m, id, mano) { const a = m.ase[id]; const q = a.dem[B.grano] * 60; a.alm[B.grano] += q; return m.reg('mana', 'Amanecen ' + Math.round(q) + ' t de grano en los almacenes de {A' + id + '}. Nadie sabe quién las ha traído', { a: id, imp: 1, c: [mano] }); } });
  T('abrir_deposito', {
    cat: 2, nom: 'Abrir el depósito', ico: '🚪', obj: 'A', desc: 'La reserva del Estado y lo que guardan los acaparadores sale al mercado.', puede: (m, a) => m.ase[a].res[B.grano] + m.ase[a].acap[B.grano] > 1,
    hacer(m, id, mano) { const a = m.ase[id]; const q = a.res[B.grano] + a.acap[B.grano]; a.alm[B.grano] += q; a.res[B.grano] = 0; a.acap[B.grano] = 0; a.acapFac = -1; a.reparto += 3; return m.reg('motin_pan', 'Alguien deja abiertas de noche las puertas de los depósitos de {A' + id + '}: ' + S.fmt(q) + ' t de grano salen a la calle', { a: id, imp: 1, c: [mano] }); },
  });
  T('incitar', { cat: 2, nom: 'Incitar', ico: '📢', obj: 'A', desc: 'Sube el agravio contra el régimen y aparecen retratos sin ojos.', hacer(m, id, mano) { const a = m.ase[id]; for (const c of a.coh) m.coh[c].agr.reg = Math.min(1, m.coh[c].agr.reg + 0.2); a.ret = Math.min(1, a.ret + 0.25); a.masacre = (a.masacre || 0) + 0.12; const ev = m.reg('pintadas', 'En una noche, la mitad de los retratos de {A' + id + '} amanecen sin ojos', { a: id, imp: 1, c: [mano] }); a.evSenal = ev; return ev; } });
  T('calmar', { cat: 2, nom: 'Pan y circo', ico: '🎭', obj: 'A', desc: 'Baja el agravio, se borran las pintadas y se olvidan las masacres. De momento.', hacer(m, id, mano) { const a = m.ase[id]; for (const c of a.coh) { const g2 = m.coh[c].agr; g2.reg = Math.max(0, g2.reg - 0.25); g2.pat = Math.max(0, g2.pat - 0.2); } a.ret *= 0.3; a.masacre = 0; a.nMasacres = 0; a.f = S.soc.RAD; a.senal = 0; return m.reg('fiesta', 'Fiestas de una semana en {A' + id + '}: nadie recuerda quién las paga', { a: id, imp: 0, c: [mano] }); } });
  T('levantar', {
    cat: 2, nom: 'Sacar a la gente a la calle', ico: '✊', obj: 'A', desc: 'Media ciudad sale a la calle hoy. Lo que pase depende de la cuenta que haga cada soldado.', puede: (m, a) => m.ase[a].est >= 0,
    hacer(m, id, mano) { const a = m.ase[id]; a.f = 0.62; a.revuelta = -1; a.evSenal = mano; S.soc.levantamiento(m, a); return a.revuelta; },
  });
  T('revolucion', {
    cat: 2, nom: 'Que triunfe la revolución', ico: '🚩', obj: 'A', desc: 'La guarnición se une a la gente: el asentamiento deja de obedecer (si es la capital, cae el régimen).', puede: (m, a) => m.ase[a].est >= 0,
    hacer(m, id, mano) { const a = m.ase[id]; a.f = 0.63; const ev = m.reg('revolucion', 'Revolución en {A' + id + '}: la guarnición se amotina y se une a la gente', { a: id, imp: 3, c: [mano, a.hambruna], d: { f: a.f, dif: 1 } }); a.revuelta = ev; S.emitir(m, 'revolucion', a, ev, 1); return ev; },
  });
  T('independencia', {
    cat: 2, nom: 'Independencia', ico: '🏴', obj: 'A', desc: 'El gobernador deja de obedecer a la capital: todo su sistema se convierte en un señorío.', puede: (m, a) => m.ase[a].est >= 0 && !m.ase[a].cap && m.ase[a].gob >= 0 && m.per[m.ase[a].gob].vivo && m.ase[a].sis !== m.ase[m.est[m.ase[a].est].cap].sis,
    hacer(m, id, mano) { const a = m.ase[id]; const p = m.per[a.gob]; const e = m.est[a.est]; const ev = m.reg('independencia', '{P' + p.id + '} deja de obedecer a la capital: {A' + id + '} es independiente de hecho', { a: id, imp: 2, c: [mano], d: { e: e.id } }); S.pol.secesion(m, e, [a.sis], p, 'senorio', ev); return ev; },
  });
  T('desarmar', { cat: 2, nom: 'Dejar sin paga a la guarnición', ico: '🪙', obj: 'A', desc: 'El arca de la paga aparece vacía: cuatro meses sin cobrar. Un soldado sin cobrar es otro soldado.', puede: (m, a) => m.ase[a].uni >= 0, hacer(m, id, mano) { const a = m.ase[id]; m.uni[a.uni].msc += 4; return m.reg('sin_paga', 'El arca de la paga de la guarnición de {A' + id + '} aparece vacía: llevan cuatro meses sin cobrar', { a: id, imp: 1, c: [mano] }); } });
  T('pagar', { cat: 2, nom: 'Pagar a la guarnición', ico: '🎖', obj: 'A', desc: 'Se ponen al día los atrasos y la guarnición vuelve a su plantilla.', puede: (m, a) => m.ase[a].uni >= 0, hacer(m, id, mano) { const a = m.ase[id]; const u = m.uni[a.uni]; u.msc = 0; u.n = Math.max(u.n, Math.round(a.pob * 0.03)); return m.reg('paga', 'Llega un arca sellada al cuartel de {A' + id + '}: se pagan todos los atrasos', { a: id, imp: 0, c: [mano] }); } });
  T('huelga_general', {
    cat: 2, nom: 'Huelga general', ico: '🛑', obj: 'A', desc: 'Paran todas las instalaciones durante 45 días. Lo que salía de aquí deja de salir.',
    hacer(m, id, mano) { const a = m.ase[id]; const l = []; for (const x of a.ins) { const i = m.ins[x]; if (i.coh >= 0 && !i.huelga) { i.huelga = true; l.push(x); } } a.huelga = m.t + 45; const ev = m.reg('huelga', 'Huelga general en {A' + id + '}: paran ' + l.length + ' instalaciones durante 45 días', { a: id, imp: 2, c: [mano] }); a.evEscasez = ev; for (const x of l) { m.ins[x].evHuelga = ev; if (m.ins[x].tipo === 'fab_nucleos') for (const b of m.ase) b.evSinNucleos = ev; } m.prog(m.t + 45, 'dios.finHuelga', { a: id, ins: l, ev }); return ev; },
  });
  S.en('dios.finHuelga', function (m, e) { for (const x of e.ins) m.ins[x].huelga = false; m.ase[e.a].huelga = 0; m.reg('huelga_fin', 'Acaba la huelga general de {A' + e.a + '}', { a: e.a, imp: 0, c: [e.ev] }); });
  T('exodo', {
    cat: 2, nom: 'Éxodo', ico: '🧳', obj: 'A', desc: 'Uno de cada cinco habitantes huye a los asentamientos vecinos, con su versión de quién tuvo la culpa.',
    hacer(m, id, mano) {
      const a = m.ase[id]; const k = Math.round(a.pob * 0.2); const dest = a.cerca.slice(0, 4).map(i => m.ase[i]); if (!dest.length) return mano;
      const ev = m.reg('exodo', k.toLocaleString('es-ES') + ' personas huyen de {A' + id + '} en lo que pueden hacia ' + dest.map(d => '{A' + d.id + '}').join(', '), { a: id, imp: 2, c: [mano, a.hambruna] });
      a.pob -= k; const culpa = S.soc.parteCulpa(a, 'reg') > 0.4;
      for (const d of dest) { d.pob += k / dest.length; d.refEnt = (d.refEnt || 0) + k / dest.length; const gen = m.coh[d.cohGen]; if (culpa && d.est === a.est) gen.agr.reg = Math.min(1, gen.agr.reg + 0.06); }
      return ev;
    },
  });
  T('conector', {
    cat: 2, nom: 'Sembrar un conector', ico: '🧵', obj: 'A', desc: 'Aparece alguien que conoce a todo el mundo. Si aquí hay una queja común, en unos meses tendrá reuniones, caja y nombre.',
    hacer(m, id, mano) { const a = m.ase[id]; a.conector = true; const p = S.per.crear(m, { casa: id, est: a.est, rol: 'civil', car: 0.96, ideo: m.coh[a.cohGen].ideo }); p.hab = 0.9; return m.reg('conector', '{P' + p.id + '} llega a {A' + id + '} y en un mes conoce a todo el mundo', { a: id, imp: 0, c: [mano] }); },
  });

  // ════ Dinero y rumores ════
  T('imprimir', { cat: 3, nom: 'Inflar la moneda', ico: '💸', obj: 'E', desc: 'Un 25 % más de billetes: los precios en esa moneda suben parecido a medio plazo, y la paga del soldado vale menos.', hacer(m, id, mano) { const e = m.est[id]; e.mon.M *= 1.25; return m.reg('imprime', 'Aparecen billetes falsos de ' + e.mon.nom + ' en todos los puertos de {E' + id + '}: un 25 % más de dinero en circulación', { a: e.cap, imp: 1, c: [mano], d: { e: id } }); } });
  T('tesoro_lleno', { cat: 3, nom: 'Llenar el tesoro', ico: '🏦', obj: 'E', desc: 'Medio millón en las arcas: flota nueva, pagas al día, impuestos más bajos.', hacer(m, id, mano) { const e = m.est[id]; e.tes += 500000; return m.reg('tesoro', 'Aparece un filón de oro en las cuentas de {E' + id + '}', { a: e.cap, imp: 1, c: [mano] }); } });
  T('tesoro_vacio', { cat: 3, nom: 'Vaciar el tesoro', ico: '🕳', obj: 'E', desc: 'Las arcas amanecen vacías y con un agujero: tendrá que imprimir, pedir prestado, dejar de pagar o recortar.', hacer(m, id, mano) { const e = m.est[id]; const q = e.tes; e.tes = -80000; return m.reg('desfalco', 'Desaparecen ' + S.fmt(Math.max(0, q)) + ' del tesoro de {E' + id + '}; nadie sabe quién firmó', { a: e.cap, imp: 2, c: [mano] }); } });
  T('bancarrota', { cat: 3, nom: 'Bancarrota', ico: '📉', obj: 'E', desc: 'El Estado deja de pagar todo lo que debe. Las casas acreedoras lo pierden, y no le prestarán en años.', puede: (m, e) => S.pol.deuda(m, m.est[e]) > 1000 && m.est[e].gob >= 0, hacer(m, id) { const e = m.est[id]; S.pol.bancarrota(m, e, m.per[e.gob]); return m.ev.length - 1; } });
  T('nucleos', { cat: 3, nom: 'Dejar núcleos de repuesto', ico: '☢', obj: 'A', desc: 'Ocho núcleos de reactor nuevos en el almacén. Donde faltan, las naves dejan de volar con grietas.', hacer(m, id, mano) { const a = m.ase[id]; const fabs = m.ins.filter(i => i.tipo === 'fab_nucleos'); for (let k = 0; k < 8; k++) { const o = S.obj.nuevoNucleo(m, fabs.length ? fabs[m.rng.i(fabs.length)] : null); o.donde = { t: 'A', id }; a.nuc.push(o.id); } return m.reg('repuestos', 'Aparecen ocho núcleos de reactor sin albarán en un almacén de {A' + id + '}', { a: id, imp: 0, c: [mano] }); } });
  T('rumor_hambre', {
    cat: 3, nom: 'Falso rumor de hambre', ico: '📣', obj: 'A', desc: 'Corre la noticia de que aquí se paga el grano a precio de oro. Treinta mercaderes acudirán a la vez y el precio se hundirá.',
    hacer(m, id, mano) { const a = m.ase[id]; const ev = m.reg('rumor', 'Alguien cuenta en cada muelle que en {A' + id + '} hay hambre y el grano se paga a precio de oro. No es verdad', { a: id, imp: 1, c: [mano] }); const T0 = Math.max(S.eco.objetivo(a, B.grano), a.dem[B.grano] * 60); for (const b of m.ase) { if (b.id === id) continue; const p = S.inf.crear(m, ev, 'hambruna', b.id, 0.75, 60, { a: id, T: T0 }); void p; if (m.saltos[a.sis][b.sis] > 3) continue; } return ev; },
  });
  T('rumor_piratas', {
    cat: 3, nom: 'Falso rumor de piratas', ico: '🏴‍☠️', obj: 'S', desc: 'En todos los puertos cercanos se cuenta que este sistema está infestado: los mercaderes lo evitan o esperan a formar convoy.',
    hacer(m, id, mano) { const ev = m.reg('rumor', 'En cada cantina se cuenta que {S' + id + '} está infestado de piratas. Nadie ha visto ninguno', { s: id, imp: 1, c: [mano] }); for (const a of m.ase) { if (m.saltos[a.sis][id] > 4) continue; if (!a.rie) a.rie = new Float32Array(m.sis.length); a.rie[id] = 0.5; } return ev; },
  });
  T('amotinar', {
    cat: 3, nom: 'Amotinar la tripulación', ico: '⚓', obj: 'N', desc: 'La tripulación se echa a la piratería con la nave. Los piratas no salen de la nada: salen de aquí.', puede: (m, n) => !m.nav[n].pirata && m.nav[n].flo < 0 && S.reg.nave[m.nav[n].cls].cmax > 0,
    hacer(m, id, mano) { const n = m.nav[id]; const b = sinley(m, S.nav.sisDe(m, n)); n.pirata = true; n.patente = -1; n.est = -1; n.dueno = { t: 'P', id: n.cap }; n.dinero = 2000; if (b) n.base = b.id; const ev = m.reg('motin', 'La tripulación de la {N' + id + '} se amotina y se echa a la piratería', { a: n.en, imp: 1, c: [mano], d: { n: id } }); n.cn.push(ev); n.origenPirata = ev; if (n.cap >= 0) { const c = m.per[n.cap]; c.rol = 'pirata'; c.est = -1; } if (n.st !== 'viaje') m.prog(m.t + 0.2, 'nave.decide', { n: id }); return ev; },
  });

  // ════ Guerra y poder ════
  T('casus_belli', {
    cat: 4, nom: 'Provocar una guerra', ico: '⚔', obj: 'E', desc: 'Un incidente en la frontera que nadie puede probar: declara la guerra a su peor vecino.', puede: (m, e) => S.pol.vecinos(m, m.est[e]).length > 0 && m.est[e].gob >= 0 && !m.est[e].suc,
    hacer(m, id, mano) { const e = m.est[id]; const v = S.pol.vecinos(m, e).filter(j => !e.gue.has(j)); if (!v.length) return mano; v.sort((x, y) => S.pol.rel(m, e, x).op - S.pol.rel(m, e, y).op); const j = v[0]; const r = S.pol.rel(m, e, j); r.cb += 3; r.ev = m.reg('incidente', 'Un carguero de {E' + id + '} aparece destrozado junto a la frontera de {E' + j + '}. Nadie puede probar nada', { a: e.cap, imp: 1, c: [mano] }); return S.pol.declarar(m, e, j, { motivo: 'no perdona el incidente de la frontera', c: r.ev }); },
  });
  T('paz', {
    cat: 4, nom: 'Imponer la paz', ico: '🕊', obj: 'E', desc: 'Acaban todas sus guerras, cada uno con lo que tiene. Las flotas tardarán en enterarse.', puede: (m, e) => m.est[e].gue.size > 0,
    hacer(m, id, mano) { const e = m.est[id]; let ev = mano; for (const j of Array.from(e.gue.keys())) { const o = m.est[j]; e.gue.delete(j); o.gue.delete(id); ev = m.reg('paz', '{E' + id + '} y {E' + j + '} firman un armisticio que nadie recuerda haber negociado', { a: e.cap, imp: 2, c: [mano], d: { e: id, j } }); for (const x of [e, o]) for (const fid of x.flo) { const f = m.flo[fid]; if (f.vivo) S.cor.enviar(m, x.cap, f.base, 'orden', { flo: fid, tipo: 'volver', urg: 2 }, ev, x.id); } } return ev; },
  });
  T('golpe', {
    cat: 4, nom: 'Golpe de Estado', ico: '🎖', obj: 'E', desc: 'El general entra hoy en palacio. Después viene la subasta.', puede: (m, e) => { const x = m.est[e]; return x.gob >= 0 && x.corte.general >= 0 && m.per[x.corte.general].vivo && !x.suc; },
    hacer(m, id, mano) { const e = m.est[id]; const g2 = m.per[e.corte.general]; g2.r[R.AMB] = Math.max(g2.r[R.AMB], 0.9); const ev = m.reg('golpe', '{P' + g2.id + '} da un golpe: el ejército entra en palacio', { a: e.cap, imp: 2, c: [mano], d: { e: id } }); S.per.matar(m, m.per[e.gob], { modo: 'publico', por: g2.id, c: [ev] }); return ev; },
  });
  T('armada', {
    cat: 4, nom: 'Regalar una armada', ico: '🚀', obj: 'E', desc: 'Cuatro escuadras de fragatas y una de cruceros aparecen en su capital: unas doscientas naves.',
    hacer(m, id, mano) { const e = m.est[id]; const cap = m.ase[e.cap]; let f = e.flo.map(i => m.flo[i]).find(x => x.vivo && x.st === 'base' && x.base === e.cap); if (!f) f = S.pol.nuevaFlota(m, e, cap); let k = 0; const prov = S.pol.territorio(m, e).filter(a => a.id !== e.cap); const fabs = m.ins.filter(i => i.tipo === 'fab_nucleos'); for (const cls of ['fragata', 'fragata', 'fragata', 'fragata', 'crucero']) { const n = S.pol.nuevaFragata(m, e, f, cap, prov, cls); const o = S.obj.nuevoNucleo(m, fabs.length ? fabs[m.rng.i(fabs.length)] : null); o.donde = { t: 'N', id: n.id }; n.nuc = o.id; S.obj.activar(m, o, 3); k += n.cascos; } return m.reg('armada', 'Una armada de ' + k + ' naves sin bandera entra en {A' + e.cap + '} y jura lealtad a {E' + id + '}', { a: e.cap, imp: 2, c: [mano], d: { e: id } }); },
  });
  T('revocar_patentes', {
    cat: 4, nom: 'Revocar las patentes', ico: '📜', obj: 'E', desc: 'Sus corsarios descubren que su patente ya no vale nada: de la noche a la mañana son piratas.', puede: (m, e) => m.nav.some(n => n.vivo && n.patente === e),
    hacer(m, id, mano) { let k = 0; const ev = m.reg('patentes', '{E' + id + '} revoca todas sus patentes de corso', { a: m.est[id].cap, imp: 1, c: [mano] }); for (const n of m.nav) { if (!n.vivo || n.patente !== id) continue; n.patente = -1; n.pirata = true; n.est = -1; n.origenPirata = ev; const b = sinley(m, S.nav.sisDe(m, n)); if (b) n.base = b.id; if (n.cap >= 0) m.per[n.cap].rol = 'pirata'; k++; } m.ev[ev].txt += ': ' + k + ' corsarios pasan a ser piratas'; return ev; },
  });
  T('interceptar', {
    cat: 4, nom: 'Matar al mensajero', ico: '✉', obj: 'E', desc: 'Todos los correos del Estado que estén en vuelo se pierden con sus sacas: órdenes, informes y declaraciones que no llegan.', puede: (m, e) => m.nav.some(n => n.vivo && n.cls === 'correo' && n.dueno.t === 'E' && n.dueno.id === e),
    hacer(m, id, mano) { let ev = mano; for (const n of m.nav) if (n.vivo && n.cls === 'correo' && n.dueno.t === 'E' && n.dueno.id === id && n.st === 'viaje') ev = S.nav.destruir(m, n, { txt: 'El correo {N' + n.id + '} no llega a su destino', c: [mano], imp: 1 }); return ev; },
  });

  // ════ Facciones ════
  T('financiar', { cat: 5, nom: 'Financiar', ico: '💼', obj: 'F', desc: 'Veinticinco mil en su caja: huelgas más largas, grano de contrabando, cazarrecompensas.', hacer(m, id, mano) { const f = m.fac[id]; f.caja += 25000; return m.reg('donativo', 'Un donante anónimo deja 25.000 en la caja de ' + (f.nom || 'la gente que se reúne en {A' + f.sede + '}'), { a: f.sede, imp: 0, c: [mano], d: { f: id } }); } });
  T('radicalizar', { cat: 5, nom: 'Radicalizar', ico: '🧨', obj: 'F', desc: 'Organización y compromiso al máximo: sabotajes, atentados, levantamientos.', puede: (m, f) => m.fac[f].tipo !== 'casa', hacer(m, id, mano) { const f = m.fac[id]; f.O = Math.max(f.O, 1.6) + 0.4; f.comp = 1; f.tAtentado = 0; f.tSabotaje = 0; if (f.etapa === 'proto') S.fac.bautizar(m, f); if (f.tipo === 'hermandad') f.tipo = 'sindicato'; return m.reg('radical', f.nom + ' ya no se conforma con reunirse', { a: f.sede, imp: 1, c: [mano, f.evNace], d: { f: id } }); } });
  T('redada', {
    cat: 5, nom: 'Delatarla', ico: '🚨', obj: 'F', desc: 'La policía recibe una lista con nombres: redada, cabecilla ejecutado y la facción descabezada.', puede: (m, f) => m.fac[f].tipo !== 'casa' && m.fac[f].lid >= 0 && m.per[m.fac[f].lid].vivo,
    hacer(m, id, mano) { const f = m.fac[id]; const a = m.ase[f.sede]; const ev = m.reg('redada', 'Alguien deja en el cuartel de {A' + a.id + '} una lista con los nombres de ' + (f.nom || 'los que se reúnen') + ': redada de madrugada', { a: a.id, imp: 2, c: [mano, f.evNace], d: { f: id } }); f.comp = Math.max(0, f.comp - 0.3); f.O = Math.max(0, f.O - 0.5); for (const c of f.coh) m.coh[c].agr.reg = Math.min(1, m.coh[c].agr.reg + 0.08); a.terror = (a.terror || 0) + 0.15; S.per.matar(m, m.per[f.lid], { modo: 'ejecucion', c: [ev], por: a.gob >= 0 ? a.gob : -1 }); return ev; },
  });
  T('cisma', {
    cat: 5, nom: 'Romper el puente', ico: '✂', obj: 'F', desc: 'Los enlaces entre sus casas dejan de hablarse: la facción se parte por donde las amistades son más débiles.', puede: (m, f) => m.fac[f].cel.length >= 2,
    hacer(m, id, mano) { const f = m.fac[id]; for (const c of f.cel) if (c.padre >= 0) c.puente = 0.02; f.evPuente = mano; const r = S.fac.cohesion(m, f); const g2 = r ? S.fac.cisma(m, f, r.vec, 'alguien ha envenenado la confianza entre sus casas (λ₂ = ' + f.l2.toFixed(2) + ')') : null; return g2 ? g2.fund : mano; },
  });

  // ════ Técnica, alianzas y cohesión (lo nuevo) ════
  T('genio', { cat: 4, nom: 'Un genio en la oficina de diseño', ico: '🧠', obj: 'E', desc: 'Su nivel técnico sube un 20 % de golpe: todas sus naves pegan más y aguantan más.', hacer(m, id, mano) { const e = m.est[id]; e.tec *= 1.2; return m.reg('genio', 'Una ingeniera de {E' + id + '} resuelve en una noche lo que la oficina de diseño llevaba diez años sin ver', { a: e.cap, imp: 1, c: [mano], d: { e: id } }); } });
  T('espias', { cat: 4, nom: 'Regalarle espías', ico: '🕵', obj: 'E', desc: 'Sabe exactamente cómo son hoy las naves de todos sus vecinos: sus próximos diseños irán contra eso.', puede: (m, e) => !!m.est[e].dis, hacer(m, id, mano) { const e = m.est[id]; let k = 0; for (const o of m.est) { if (!o.vivo || o.id === id || !o.dis) continue; e.intel.set(o.id, { g: o.dis.fragata.mejor.g.slice(), niv: o.tec, t: m.t }); k++; } return m.reg('espionaje', 'Sobre la mesa del gobernante de {E' + id + '} aparecen los planos de las flotas de ' + k + ' Estados', { a: e.cap, imp: 1, c: [mano], d: { e: id } }); } });
  T('planos_falsos', { cat: 4, nom: 'Colarle planos falsos', ico: '🗞', obj: 'E', desc: 'Su oficina adopta diseños disparatados: las próximas escuadras saldrán mal repartidas.', puede: (m, e) => !!m.est[e].dis, hacer(m, id, mano) { const e = m.est[id]; for (const cls of ['fragata', 'crucero', 'acorazado']) { const li = e.dis[cls]; const g2 = S.tec.G0.map(() => m.rng.r(0.02, 0.3)); const d = S.tec.nuevoDiseno(m, id, cls, g2, li.gen + 1, li.base, [li.mejor.id]); d.fit = 9; li.gen++; li.pob.unshift(d); li.pob.pop(); li.mejor = d; } return m.reg('planos_falsos', 'La oficina de diseño de {E' + id + '} da por buenos unos planos que alguien dejó en el archivo', { a: e.cap, imp: 1, c: [mano], d: { e: id } }); } });
  T('forjar_alianza', {
    cat: 4, nom: 'Forjar una alianza', ico: '🤝', obj: 'E', desc: 'Se alía con el vecino con el que mejor se lleva. Los aliados entran en guerra juntos y se pasan planos.', puede: (m, e) => S.pol.vecinos(m, m.est[e]).some(j => !m.est[e].gue.has(j) && !(m.est[e].alianzas && m.est[e].alianzas.has(j))),
    hacer(m, id, mano) { const e = m.est[id]; const v = S.pol.vecinos(m, e).filter(j => !e.gue.has(j) && !(e.alianzas && e.alianzas.has(j))).sort((x, y) => S.pol.rel(m, e, y).op - S.pol.rel(m, e, x).op); const o = m.est[v[0]]; e.alianzas = e.alianzas || new Set(); o.alianzas = o.alianzas || new Set(); e.alianzas.add(o.id); o.alianzas.add(id); const ev = m.reg('alianza', '{E' + id + '} y {E' + o.id + '} firman una alianza que nadie esperaba', { a: e.cap, imp: 2, c: [mano], d: { e: id, j: o.id } }); (e.alCon = e.alCon || new Map()).set(o.id, { contra: o.id, t: m.t, ev }); (o.alCon = o.alCon || new Map()).set(id, { contra: id, t: m.t, ev }); return ev; },
  });
  T('romper_alianzas', { cat: 4, nom: 'Romper sus alianzas', ico: '💔', obj: 'E', desc: 'Sus aliados dejan de fiarse: se queda solo.', puede: (m, e) => !!(m.est[e].alianzas && m.est[e].alianzas.size), hacer(m, id, mano) { const e = m.est[id]; let ev = mano; for (const j of Array.from(e.alianzas)) { e.alianzas.delete(j); if (m.est[j].alianzas) m.est[j].alianzas.delete(id); S.pol.rel(m, m.est[j], id).op -= 0.5; ev = m.reg('alianza_rota', 'Una carta que nadie recuerda haber escrito rompe la alianza entre {E' + id + '} y {E' + j + '}', { a: e.cap, imp: 2, c: [mano], d: { e: id, j } }); } return ev; } });
  T('asabiya_alta', { cat: 4, nom: 'Devolverle la cohesión', ico: '🔥', obj: 'E', desc: 'Asabiya al máximo: su gente vuelve a creer. Flotas con moral, provincias que obedecen, golpes improbables.', hacer(m, id, mano) { const e = m.est[id]; e.asabiya = 1; return m.reg('fervor', 'Algo recorre {E' + id + '}: vuelven a cantarse los himnos viejos', { a: e.cap, imp: 1, c: [mano], d: { e: id } }); } });
  T('decadencia', { cat: 4, nom: 'Decadencia', ico: '🍷', obj: 'E', desc: 'Asabiya por los suelos y una docena de segundones ambiciosos sin cargo: élites de sobra, cohesión de menos.', hacer(m, id, mano) { const e = m.est[id]; e.asabiya = 0.2; const cap = m.ase[e.cap]; for (let i = 0; i < 12; i++) S.per.crear(m, { casa: cap.id, est: id, rol: 'aspirante', r: { AMB: m.rng.r(0.7, 0.98) }, ideo: e.gob >= 0 ? m.per[e.gob].ideo : undefined, nace: m.t - m.rng.r(20, 32) * S.ANIO }); e.conspira = 0.3; return m.reg('decadencia', 'La corte de {E' + id + '} se llena de herederos sin herencia y banquetes sin motivo', { a: e.cap, imp: 1, c: [mano], d: { e: id } }); } });
  T('veteranos', { cat: 0, nom: 'Tripulación de veteranos', ico: '🎗', obj: 'N', desc: 'Veteranía al máximo: pega un 50 % más.', puede: (m, n) => m.nav[n].dis !== undefined, hacer(m, id, mano) { const n = m.nav[id]; n.vet = 1; const ev = m.reg('veteranos', 'La {N' + id + '} amanece tripulada por gente que ya ha estado en todas las guerras', { a: n.en, imp: 0, c: [mano] }); n.cn.push(ev); return ev; } });
  T('chatarra', { cat: 0, nom: 'Componentes defectuosos', ico: '🧯', obj: 'N', desc: 'La calidad de todos sus componentes cae a la mitad: cañones que se encasquillan, sensores ciegos.', puede: (m, n) => !!m.nav[n].q, hacer(m, id, mano) { const n = m.nav[id]; for (let k = 0; k < 8; k++) n.q[k] *= 0.55; const ev = m.reg('chatarra', 'Un lote entero de repuestos de la {N' + id + '} resulta ser chatarra pintada', { a: n.en, imp: 0, c: [mano] }); n.cn.push(ev); return ev; } });
  T('armar', { cat: 5, nom: 'Armarla y adiestrarla', ico: '🔫', obj: 'F', desc: 'Cuarenta toneladas de armas y alguien que enseña a usarlas: su próximo levantamiento será una insurrección armada.', puede: (m, f) => m.fac[f].tipo !== 'casa', hacer(m, id, mano) { const f = m.fac[id]; if (f.doc === undefined) { f.doc = 0.1; f.armas = 0; } f.armas += 40; f.doc = Math.min(1, f.doc + 0.3); if (f.etapa === 'proto') S.fac.bautizar(m, f); f.evArmas = m.reg('armas', 'Un carguero sin matrícula descarga cajas largas para ' + f.nom + ' en {A' + f.sede + '}', { a: f.sede, imp: 1, c: [mano, f.evNace], d: { f: id } }); return f.evArmas; } });
  T('arca', { cat: 3, nom: 'Que llegue el arca', ico: '🛸', obj: 'S', desc: 'Una nave generacional que partió hace doscientos años entra hoy en los sensores. A bordo creen que el viejo imperio sigue en pie.', hacer(m, id, mano) { const antes = m.ev.length; S.man['arca.aparece'](m, {}); const e = m.ev.slice(antes).find(x => x.k === 'arca'); if (e) e.c.push(mano); return e ? e.id : mano; } });
})(typeof globalThis !== 'undefined' ? globalThis : this);
