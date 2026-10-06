// Más mundo: quién ha matado a cuánta gente, la biografía de cada persona, viajes de personas, flujos de
// comercio, inversión y cierre de fábricas, coste de la vida y paro, embargo en guerra, y las naves que
// faltaban: bazares errantes, astilleros nómadas, el arca generacional, el buque-prisión y la nave funeraria.
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const R = S.R; const P = S.per; const Nv = S.nav;
  const X = S.mas = {};

  // ── Bajas: las que causó con sus manos y las que murieron bajo su mando o por orden suya.
  // Cada una queda apuntada en su cuenta (p.mue): cuántos, quiénes, cómo y el hecho del que cuelga el porqué.
  const TOPE = 240;
  const anota = (m, id, k, n, x) => {
    if (id === undefined || id === null || id < 0 || !m.per[id] || !(n > 0)) return; const p = m.per[id];
    p[k] = (p[k] || 0) + n; x.t = m.t; x.n = n; x.k = k; const l = p.mue || (p.mue = []); l.push(x);
    if (l.length > TOPE) { const v = l.shift(); p.mueAnt = (p.mueAnt || 0) + v.n; }
  };
  S.gancho('pers.muere', function (m, p, ev, o) { if (o && o.por >= 0 && o.modo !== 'natural') anota(m, o.por, 'bajas', 1, { c: 'persona', v: p.id, ev, modo: o.modo || '' }); });
  S.gancho('muertes', function (m, a, co, n, ev, o) { if (o && o.culpable >= 0) anota(m, o.culpable, 'mando', n, { c: 'pueblo', a: a.id, ins: co ? co.ins : -1, ev, hecho: o.hecho || '' }); });
  S.gancho('nave.destruida', function (m, n, ev, o) { if (o && o.por >= 0) anota(m, o.por, 'mando', S.reg.nave[n.cls].trip * Math.max(1, n.cascos), { c: 'nave', nav: n.id, cascos: Math.max(1, n.cascos), ev }); });
  S.gancho('batalla.fin', function (m, bt, ev) { for (let i = 0; i < 2; i++) { anota(m, bt.L[i].alm, 'mando', bt.L[1 - i].muertos, { c: 'batalla', bt: bt.id, lado: i, ev: ev === undefined ? bt.ev : ev }); const p = m.per[bt.L[i].alm]; if (p) { p.batallas = (p.batallas || 0) + 1; if (bt.gano === i) p.victorias = (p.victorias || 0) + 1; } } });
  S.gancho('tierra.fin', function (m, t) {
    const a = m.ase[t.a]; const bA = Math.round(t.atk.n0 - t.atk.n), bD = Math.round(t.def.n0 - t.def.n); const civ = Math.round(t.civiles * 0.5);
    const jefeA = t.flo >= 0 ? m.flo[t.flo].alm : (t.atk.fac >= 0 ? m.fac[t.atk.fac].lid : -1); const g2 = S.pol.gobernadorDe(m, a);
    anota(m, jefeA, 'mando', bD + civ, { c: 'tierra', tie: t.id, lado: 0, sold: bD, civ, ev: t.ev }); if (g2) anota(m, g2.id, 'mando', bA + civ, { c: 'tierra', tie: t.id, lado: 1, sold: bA, civ, ev: t.ev });
  });
  // Cómo se cuenta cada clase de muerte: a quién mató («que») y de qué manera («como»). Con fichas {P1} {A2}… como los hechos.
  const num = (x) => Math.round(x).toLocaleString('es-ES');
  const MODO = { combate: 'En combate, con los suyos.', ejecucion: 'Una ejecución por orden suya.', purga: 'Una purga: no quería a nadie que pudiera disputarle el poder.', venganza: 'Venganza, cara a cara.', catador: 'Veneno.', explosion: 'Una explosión.', publico: 'En público, delante de todos.', silencio: 'Sin que nadie lo viera.' };
  S.def('muerte', 'persona', {
    que: (m, x) => '{P' + x.v + '}' + (m.per[x.v] && m.per[x.v].cargo && m.per[x.v].cargo.nom ? ', ' + m.per[x.v].cargo.nom : ''),
    como: (m, x) => MODO[x.modo] || 'Con sus propias manos.',
  });
  S.def('muerte', 'pueblo', {
    que: (m, x) => num(x.n) + (x.ins >= 0 && m.ins[x.ins] ? ' trabajadores de ' + m.ins[x.ins].nom : ' vecinos') + ' de {A' + x.a + '}',
    como: (m, x) => { const k = x.ev >= 0 && m.ev[x.ev] ? m.ev[x.ev].k : ''; return k === 'insurreccion_aplastada' ? 'Represalias en las calles después de aplastar la insurrección.' : x.hecho === 'masacre' ? 'Mandó disparar contra la gente que estaba en la calle.' : x.hecho === 'bomba' ? 'Murieron en la explosión de una bomba que se le atribuye.' : x.hecho === 'invasion' ? 'Murieron entre dos fuegos durante la invasión que mandaba.' : 'Murieron por un hecho del que se le hace responsable.'; },
  });
  S.def('muerte', 'nave', {
    que: (m, x) => num(x.n) + ' tripulantes de la {N' + x.nav + '}' + (x.cascos > 1 ? ' (' + x.cascos + ' naves)' : ''),
    como: (m, x) => { const k = x.ev >= 0 && m.ev[x.ev] ? m.ev[x.ev].k : ''; return k === 'pirata_cazado' ? 'Su flota la cazó cuando acechaba y la destruyó con toda su tripulación.' : 'La abordó o la destruyó con los suyos: no hubo supervivientes.'; },
  });
  S.def('muerte', 'batalla', {
    que: (m, x) => { const bt = m.bat[x.bt]; return num(x.n) + ' tripulantes de {E' + bt.L[1 - x.lado].est + '}'; },
    como: (m, x) => { const bt = m.bat[x.bt]; const yo = bt.L[x.lado], el = bt.L[1 - x.lado]; const d = Math.max(1, Math.round((bt.t1 === undefined ? m.t : bt.t1) - bt.t0)); return 'Su flota hundió ' + num(el.n0 - (el.nFin === undefined ? el.n : el.nFin)) + ' naves enemigas en la batalla de {S' + bt.sis + '}: ' + d + (d === 1 ? ' día' : ' días') + ' de combate' + (bt.ley === 'lineal' ? ' entre asteroides' : '') + '. ' + (bt.gano === x.lado ? 'La ganó' : bt.huye === x.lado ? 'Tuvo que retirarse' : 'La perdió') + ', con ' + num(yo.n0 - (yo.nFin === undefined ? yo.n : yo.nFin)) + ' naves propias perdidas.'; },
  });
  S.def('muerte', 'tierra', {
    que: (m, x) => { const t = m.tie[x.tie]; const asalto = t.tipo === 'asalto'; return num(x.sold) + (x.lado === 0 ? ' soldados de la guarnición' : asalto ? ' soldados asaltantes' : ' insurrectos') + (x.civ ? ' y ' + num(x.civ) + ' vecinos' : '') + ' de {A' + t.a + '}'; },
    como: (m, x) => { const t = m.tie[x.tie]; const asalto = t.tipo === 'asalto'; const d = Math.max(1, Math.round((t.t1 === undefined ? m.t : t.t1) - t.t0)); const dd = d + (d === 1 ? ' día' : ' días'); return x.lado === 0 ? (asalto ? 'Sus tropas asaltaron el asentamiento casa por casa durante ' + dd + '.' : 'Encabezó la insurrección: ' + dd + ' de combates en las calles.') + (t.gano === 0 ? ' Vencieron.' : ' Fracasaron.') : (asalto ? 'Su guarnición defendió el asentamiento durante ' + dd + '.' : 'Su guarnición combatió la insurrección durante ' + dd + '.') + (t.gano === 1 ? ' Aguantó.' : ' Cayó.'); },
  });
  // Lo que se enseña de una entrada de la cuenta: víctimas, manera y el hecho del que cuelga el porqué.
  X.muerte = function (m, x) { const d = S.reg.muerte[x.c]; return { que: d ? d.que(m, x) : num(x.n) + ' personas', como: d ? d.como(m, x) : '', ev: x.ev >= 0 && m.ev[x.ev] ? x.ev : -1 }; };
  X.letalidad = (p) => (p.bajas || 0) + (p.mando || 0);

  // ── Biografía: cada hecho que nombra a una persona queda en su expediente.
  S.gancho('hecho', function (m, e) {
    if (e.txt.indexOf('{P') < 0) return; m.bio = m.bio || new Map();
    const re = /\{P(\d+)\}/g; let x;
    while ((x = re.exec(e.txt))) { const id = +x[1]; let l = m.bio.get(id); if (!l) { l = []; m.bio.set(id, l); } if (l[l.length - 1] !== e.id) l.push(e.id); }
  });

  // ── Personas que viajan (como pasaje): se las puede seguir por el mapa.
  X.viajar = function (m, p, dest) {
    const de = P.lugar(m, p); if (de === dest || de < 0) { p.en = { t: 'A', id: dest }; return 0; }
    const d = Nv.distancia(m, m.ase[de].sis, m.ase[dest].sis); const dur = d / 50 + 1;
    p.en = { t: 'V', de, a: dest, t0: m.t, t1: m.t + dur }; m.prog(m.t + dur, 'pers.llega', { p: p.id, en: p.en });
    return dur;
  };
  S.en('pers.llega', function (m, e) { const p = m.per[e.p]; if (p.en !== e.en) return; p.en = { t: 'A', id: e.en.a }; S.emitir(m, 'pers.llega', p, e.en.a); });
  // Los almirantes van a bordo de su buque insignia.
  X.insignia = function (m, f) { let best = null; for (const id of f.nav) { const n = m.nav[id]; if (n.vivo && (!best || S.reg.nave[n.cls].ef * 100 + n.cascos > S.reg.nave[best.cls].ef * 100 + best.cascos)) best = n; } return best; };
  S.gancho('asent.mes', function (m, a) { for (const f of m.flo) { if (!f.vivo || f.base !== a.id || f.alm < 0) continue; const n = X.insignia(m, f); const p = m.per[f.alm]; if (n && p.vivo) { p.en = { t: 'N', id: n.id }; f.insignia = n.id; } } });

  // ── Flujos de comercio (para el mapa, la paz comercial y la tabla de economía).
  S.gancho('venta', function (m, n, a, cg, r) {
    if (cg.ori < 0 || cg.ori === a.id) return; m.flu = m.flu || new Map();
    const k = cg.ori * 4096 + a.id; let f = m.flu.get(k); if (!f) { f = { de: cg.ori, a: a.id, q: 0, c: new Array(S.NB).fill(0), v: 0 }; m.flu.set(k, f); }
    f.q += r.q; f.c[cg.c] += r.q; f.v += r.ingreso;
    const e1 = m.ase[cg.ori].est, e2 = a.est;
    if (e1 >= 0 && e2 >= 0 && e1 !== e2) { S.pol.rel(m, m.est[e1], e2).com = (S.pol.rel(m, m.est[e1], e2).com || 0) + r.q; S.pol.rel(m, m.est[e2], e1).com = (S.pol.rel(m, m.est[e2], e1).com || 0) + r.q; }
    m.comercio = m.comercio || new Array(S.NB).fill(0); m.comercio[cg.c] += r.q;
  });
  S.gancho('sismo', function (m) {
    if (m.flu) for (const [k, f] of m.flu) { f.q *= 0.75; f.v *= 0.75; for (let c = 0; c < S.NB; c++) f.c[c] *= 0.75; if (f.q < 3) m.flu.delete(k); }
    for (const e of m.est) for (const [, r] of e.rel) if (r.com) r.com *= 0.75;
    if (m.comercio) { m.serie('comercio', m.comercio.reduce((x, y) => x + y, 0)); m.comercio.fill(0); }
  });

  // ── Coste de la vida, salario real y paro; inversión y cierre una vez al año.
  S.gancho('asent.mes', function (m, a) {
    let num = 0, den = 0; for (let c = 0; c < S.NB; c++) { const b = S.bien[c]; if (!b.pc || (b.soloEstacion && !a.estacion)) continue; num += b.pc * a.pr[c]; den += b.pc * b.pref; }
    a.ipc = den ? num / den : 1;
    let tr = 0, par = 0; for (const id of a.ins) { const i = m.ins[id]; if (i.coh < 0) continue; const def = S.reg.inst[i.tipo]; if (!def.sal && !def.nucleos && !def.astillero) continue; const n = m.coh[i.coh].n; tr += n; if ((i.ef || 0) < 0.3 || i.salud < 0.3) { par += n; m.coh[i.coh].agr.pat = Math.min(1, m.coh[i.coh].agr.pat + 0.02); } }
    a.paro = tr ? par / tr : 0;
    if (!a.prM) a.prM = a.pr.map((p, c) => p / S.bien[c].pref);
    for (let c = 0; c < S.NB; c++) a.prM[c] += (a.pr[c] / S.bien[c].pref - a.prM[c]) * 0.15;
  });
  const PARA = { grano: 'hidroponia', comb: 'refineria', metal: 'fundicion', piezas: 'fab_piezas', bienes: 'fab_bienes', filtros: 'fab_filtros', armas: 'armeria' };
  S.gancho('anio', function (m) {
    for (const a of m.ase) {
      if (!a.prM || a.sinley) continue;
      for (const id of a.ins) {
        const i = m.ins[id]; const def = S.reg.inst[i.tipo]; if (!def.sal || i.salud < 0.5) continue;
        const c = B[Object.keys(def.sal)[0]]; let caro = false; if (def.ent) for (const k in def.ent) if (a.prM[B[k]] > 2.6) caro = true;
        if (a.prM[c] > 1.35 && !caro && i.nivel < 6 && m.coh[i.coh].n >= def.trab * i.nivel * 0.9) { i.nivel = Math.round((i.nivel + 0.15) * 100) / 100; S.soc.ajustarPlantilla(m, i); i.amplia = m.t; }
        else if (a.prM[c] < 0.45 && i.nivel > 0.4) { i.nivel = Math.round(Math.max(0.3, i.nivel - 0.1) * 100) / 100; S.soc.ajustarPlantilla(m, i); i.recorta = m.t; }
      }
      if (a.ins.length >= 15 || a.pob < 2500) continue;
      // Donde algo lleva años a precio de oro y hay con qué hacerlo, alguien levanta una planta.
      for (const k in PARA) {
        const c = B[k]; if (a.prM[c] < 2.3 || S.eco.objetivo(a, c) < 25) continue; const tipo = PARA[k]; if (a.ins.some(x => m.ins[x].tipo === tipo)) continue;
        const def = S.reg.inst[tipo]; if (tipo === 'hidroponia' && !a.estacion && a.forma !== 'luna') continue;
        let hay = true; if (def.ent) for (const e2 in def.ent) if (a.prM[B[e2]] > 1.8) hay = false; if (!hay || !a.rng.p(0.5)) continue;
        const i = S.eco.nuevaInst(m, a, tipo, 0.5); if (def.nuc && a.nuc.length) S.eco.reponerNucleo(m, a, i);
        m.reg('fundacion', 'Se levanta ' + def.nom.toLowerCase() + ' en {A' + a.id + '}: llevaban demasiado tiempo pagando ' + S.bien[c].nom.toLowerCase() + ' a precio de oro', { a: a.id, imp: 1 });
        break;
      }
    }
  });

  // ── Bazares errantes: naves-mercado que montan feria donde se cruzan las rutas.
  S.def('nave', 'bazar', { nom: 'Bazar errante', cmax: 1600, vel: 34, trip: 120, ef: 0.01, fuel: 0.07, valor: 120000, mat: { metal: 300, piezas: 120 }, dias: 120 });
  S.def('conducta', 'bazar', {
    decide(m, n, a) {
      if (!a) return;
      if (a.feria > m.t && n.feriaEn === a.id) { S.reg.conducta.mercader.decide(m, n, a); return; }
      if (n.feriaEn === a.id && m.t - n.tFeria < 28) { m.prog(m.t + 6, 'nave.decide', { n: n.id }); return X.feria(m, n, a); }
      if (n.feriaEn !== a.id) { n.feriaEn = a.id; n.tFeria = m.t; X.feria(m, n, a); m.prog(m.t + 6, 'nave.decide', { n: n.id }); return; }
      // Levanta la feria y busca el siguiente cruce de rutas.
      let best = -1, bs = -1; for (const d of a.cerca) { const b = m.ase[d]; if (b.sinley || d === a.id) continue; const s = (m.sis[b.sis].traf || 0) * (0.5 + n.rng.f()) + Math.sqrt(b.pob) * 0.01; if (s > bs) { bs = s; best = d; } }
      n.feriaEn = -1;
      if (best < 0 || !Nv.viajar(m, n, { t: 'A', id: best })) m.prog(m.t + 5, 'nave.decide', { n: n.id });
    },
  });
  X.feria = function (m, n, a) {
    // En feria se vende de todo un poco y se cuenta todo: el tablón de precios de aquí llega lejos.
    S.inf.anotarPrecios(m, a); const e = a.tabla.get(a.id);
    for (const d of a.cerca) if (m.saltos[a.sis][m.ase[d].sis] <= 3) m.ase[d].tabla.set(a.id, e);
    const otros = m.nav.filter(x => x.vivo && x.cls === 'bazar' && x.en === a.id).length;
    if (!(a.feria > m.t)) { a.feria = m.t + 30; for (const c of a.coh) m.coh[c].agr.reg = Math.max(0, m.coh[c].agr.reg - 0.02); if (otros >= 2 || !a.evFeria || m.t - m.ev[a.evFeria].t > 3 * S.ANIO) a.evFeria = m.reg('feria', (otros >= 2 ? otros + ' bazares errantes se acoplan entre sí' : 'El bazar errante {N' + n.id + '} abre sus bodegas') + ' en {A' + a.id + '}: hay feria durante un mes', { a: a.id, imp: otros >= 2 ? 1 : 0 }); }
    for (const c of [B.bienes, B.filtros, B.piezas]) { if (S.eco.objetivo(a, c) > 0 && a.alm[c] < S.eco.objetivo(a, c) * 0.6) a.alm[c] += Math.min(8, S.eco.objetivo(a, c) * 0.1); }
  };

  // ── Astilleros nómadas: una fábrica móvil con un pueblo entero a bordo.
  S.def('nave', 'astillero_n', { nom: 'Astillero nómada', cmax: 0, vel: 20, trip: 400, ef: 0.01, fuel: 0.09, valor: 200000, mat: { metal: 500, piezas: 200 }, dias: 200 });
  S.def('conducta', 'astillero_n', {
    decide(m, n, a) {
      if (!a) return;
      let ins = n.yarda !== undefined ? m.ins[n.yarda] : null;
      if (!ins || ins.ase !== a.id) {
        // Se instala: mientras esté aquí, este puerto tiene astillero.
        ins = S.eco.nuevaInst(m, a, 'astillero', 0.7, { nom: 'Astillero nómada «' + n.nom + '»', nomada: n.id }); n.yarda = ins.id; n.tYarda = m.t;
        if (n.nuc >= 0) ins.nuc = n.nuc;
        m.reg('astillero_nomada', 'El astillero nómada {N' + n.id + '} se acopla a {A' + a.id + '}: con él viaja un pueblo entero de ' + n.trip + ' soldadores', { a: a.id, imp: 1 });
        return m.prog(m.t + 60, 'nave.decide', { n: n.id });
      }
      if (a.pedidos.length || m.t - n.tYarda < 240) return m.prog(m.t + 60, 'nave.decide', { n: n.id });
      // Sin encargos: recoge y se va a donde haya guerra o falten gradas.
      let best = -1, bs = 0; for (const d of a.cerca) { const b = m.ase[d]; if (b.sinley || b.ins.some(i => m.ins[i].tipo === 'astillero')) continue; const e = b.est >= 0 ? m.est[b.est] : null; const s = (e && e.gue.size ? 3 : 0) + (b.cap ? 2 : 0) + n.rng.f() + (b.tipo === 'comercial' ? 1 : 0); if (s > bs) { bs = s; best = d; } }
      if (best < 0) return m.prog(m.t + 120, 'nave.decide', { n: n.id });
      const i2 = a.ins.indexOf(ins.id); if (i2 >= 0) a.ins.splice(i2, 1); const ci = a.coh.indexOf(ins.coh); if (ci >= 0) { a.coh.splice(ci, 1); m.coh[ins.coh].n = 0; } ins.salud = 0; ins.nuc = -1; n.yarda = undefined;
      if (!Nv.viajar(m, n, { t: 'A', id: best })) m.prog(m.t + 30, 'nave.decide', { n: n.id });
    },
  });

  // ── El arca generacional: salió hace doscientos años de un mundo que ya no existe.
  S.def('nave', 'arca', { nom: 'Nave generacional', cmax: 0, vel: 9, trip: 6000, ef: 0.01, fuel: 0, valor: 0, mat: { metal: 2000 }, dias: 999 });
  S.en('arca.aparece', function (m) {
    const cx = m.ancho / 2, cy = m.alto / 2; const borde = m.sis.slice().sort((p, q) => Math.hypot(q.x - cx, q.y - cy) - Math.hypot(p.x - cx, p.y - cy))[m.rng.i(4)];
    const dest = m.ase[borde.ase[0]]; const rng = m.rng;
    const cap = P.crear(m, { casa: dest.id, est: -1, rol: 'predicador', car: 0.9, ideo: [0.8, 0.8, 0.5, 0.6, -0.8], r: { FE: 0.95 }, nace: m.t - rng.r(40, 60) * S.ANIO });
    const n = Nv.crear(m, 'arca', { en: -1, sisEn: borde.id, cap: cap.id, dueno: { t: 'P', id: cap.id }, est: -1, st: 'guardia' });
    n.x = borde.x + (borde.x - cx) * 0.35; n.y = borde.y + (borde.y - cy) * 0.35; n.nom = 'Promesa de ' + S.gen.nombreLugar(m);
    cap.cargo.nom = 'decimonovena capitana del arca ' + n.nom;
    n.evArca = m.reg('arca', 'Una nave generacional, la {N' + n.id + '}, entra en los sensores de {S' + borde.id + '}. Partió hace doscientos años de un mundo que ya no existe; a bordo creen que el viejo imperio sigue en pie', { s: borde.id, imp: 2 });
    Nv.viajar(m, n, { t: 'A', id: dest.id }, { sinFuel: true, sinCruce: true });
  });
  S.gancho('nave.atraca', function (m, n, a) {
    if (n.cls !== 'arca' || n.desembarco) return; n.desembarco = true; const cap = m.per[n.cap];
    const co = S.soc.nuevaCohorte(m, a, -1, n.trip); co.ideo = [0.8, 0.8, 0.5, 0.6, -0.8]; co.agr.reg = 0.75; co.arca = true; a.pob += n.trip; S.soc.resinc(m, a);
    const ev = m.reg('arca_llega', 'La {N' + n.id + '} atraca en {A' + a.id + '}: desembarcan ' + n.trip.toLocaleString('es-ES') + ' personas que hablan un dialecto antiguo y preguntan por un emperador muerto hace un siglo', { a: a.id, imp: 2, c: [n.evArca] });
    if (cap && cap.vivo) { cap.en = { t: 'A', id: a.id }; cap.casa = a.id; cap.est = a.est; const f = S.fac.nueva(m, { lid: cap.id, sede: a.id, enem: { k: 'reg', id: a.est }, hecho: { ev, clave: 'profecia', muertos: 0, s: 1 }, fund: ev, O: 1 }); f.cel.push({ ase: a.id, lid: cap.id, ideo: co.ideo.slice(), padre: -1, puente: 1 }); co.fac = f.id; f.coh.push(co.id); f.evProto = ev; S.fac.bautizar(m, f); f.nom = 'Hijos del Arca'; f.sim = 'un sol de un imperio que ya no existe, bordado en la manga'; f.tipo = 'orden'; cap.cargo.nom = 'cabeza de ' + f.nom; }
    n.trip = 40;
  });

  // ── Buque-prisión: libera uno y tendrás tripulación al instante.
  S.gancho('anio', function (m) {
    for (const n of m.nav) {
      if (!n.vivo || !n.pirata || n.en < 0 || !m.ase[n.en].sinley || n.cap < 0) continue;
      const pr = m.nav.find(x => x.vivo && x.cls === 'prision' && x.presos && x.presos.length >= 1 && x.en >= 0 && m.saltos[m.ase[n.en].sis][m.ase[x.en].sis] <= 5);
      if (!pr || !n.rng.p(0.25)) continue;
      const a = m.ase[pr.en]; const cap = m.per[n.cap];
      if (n.rng.p(0.6 - a.ley * 0.3 + cap.r[R.VAL] * 0.2)) {
        const libres = pr.presos.map(i => m.per[i]).filter(p => p.vivo); pr.presos = [];
        for (const p of libres) { p.rol = 'pirata'; p.en = { t: 'N', id: n.id }; p.est = -1; p.cargo = { t: 'tripulante', id: n.id, nom: 'tripulante de la ' + n.nom + ', fugado del ' + pr.nom }; }
        n.trip += libres.length * 6 + 20; n.dan = Math.max(0, n.dan - 0.2); n.impago = 0;
        m.reg('fuga', 'La {N' + n.id + '} asalta el buque-prisión {N' + pr.id + '} en {A' + a.id + '}: ' + libres.slice(0, 4).map(p => '{P' + p.id + '}').join(', ') + (libres.length > 4 ? ' y otros ' + (libres.length - 4) : '') + ' ya son tripulación. Gente con oficio, con rencores y sin ninguna razón para ser leal', { a: a.id, imp: 1, c: [n.origenPirata] });
      } else { n.dan = Math.min(0.9, n.dan + 0.3); m.reg('fuga_fallida', 'La {N' + n.id + '} intenta asaltar el buque-prisión {N' + pr.id + '} y sale malparada', { a: a.id, imp: 0 }); }
    }
  });

  // ── Nave funeraria: lleva a los muertos de las grandes familias a enterrar en su mundo natal.
  S.gancho('pers.muere', function (m, p, ev) {
    if (!p.cargo || p.cargo.t !== 'gobernante') return; const e = m.est[p.cargo.id]; if (!e.vivo || e.funeraria === undefined) return;
    const n = m.nav[e.funeraria]; if (!n.vivo || n.en !== e.cap || n.restos !== undefined) return;
    let dest = p.casa !== e.cap && m.ase[p.casa] && m.ase[p.casa].est === e.id ? p.casa : -1;
    if (dest < 0) { const prov = S.pol.territorio(m, e).filter(a => a.id !== e.cap && a.sis !== m.ase[e.cap].sis); if (!prov.length) return; dest = prov[e.rng.i(prov.length)].id; }
    n.restos = p.id; n.evRestos = ev;
    if (!Nv.viajar(m, n, { t: 'A', id: dest })) { n.restos = undefined; return; }
    m.reg('cortejo', 'La nave funeraria {N' + n.id + '} zarpa con los restos de {P' + p.id + '} hacia {A' + dest + '}, donde nació su casa', { a: e.cap, imp: 1, c: [ev] });
  });
  S.gancho('nave.atraca', function (m, n, a) {
    if (n.cls !== 'funeraria' || n.restos === undefined) return; const e = n.est >= 0 ? m.est[n.est] : null;
    if (e && a.id === e.cap) { n.restos = undefined; return; }
    const p = m.per[n.restos];
    const ev = m.reg('entierro', '{P' + p.id + '} descansa en {A' + a.id + '}. Media ciudad desfila ante el féretro', { a: a.id, imp: 1, c: [n.evRestos] });
    S.eco.nuevaInst(m, a, 'memorial', 1, { ev, nom: 'Panteón de ' + p.nom + ' (' + a.nom + ')', muertos: 1 });
    n.restos = undefined; if (e && e.vivo) m.prog(m.t + 3, 'funeraria.vuelve', { n: n.id });
  });
  S.en('funeraria.vuelve', function (m, x) { const n = m.nav[x.n]; if (n.vivo && n.en >= 0 && n.est >= 0 && m.est[n.est].vivo) Nv.viajar(m, n, { t: 'A', id: m.est[n.est].cap }); });
  S.gancho('nave.destruida', function (m, n, ev) {
    if (n.cls !== 'funeraria' || n.restos === undefined || n.est < 0) return; const e = m.est[n.est]; const p = m.per[n.restos];
    const pr = m.reg('profanacion', 'La nave funeraria que llevaba los restos de {P' + p.id + '} no llega: {E' + e.id + '} clama venganza', { a: e.cap, imp: 2, c: [ev], d: { e: e.id } });
    e.asabiya = Math.min(1, (e.asabiya === undefined ? 0.6 : e.asabiya) + 0.1);
    const v = S.pol.vecinos(m, e); if (v.length) { v.sort((x, y) => S.pol.rel(m, e, x).op - S.pol.rel(m, e, y).op); const r = S.pol.rel(m, e, v[0]); r.cb += 1.5; r.ev = pr; }
  });

  // ── Sembrar lo nuevo en un mundo recién generado.
  X.sembrar = function (m) {
    const rng = m.rng; const casas = m.fac.filter(f => f.tipo === 'casa'); const com = m.ase.filter(a => a.tipo === 'comercial' || a.cap);
    if (casas.length && com.length) {
      for (let i = 0; i < Math.max(2, Math.round(m.sis.length / 14)); i++) { const a = rng.el(com); const cap = P.crear(m, { casa: a.id, est: a.est, rol: 'capitan' }); Nv.crear(m, 'bazar', { en: a.id, cap: cap.id, dueno: { t: 'F', id: rng.el(casas).id }, est: a.est, dinero: 60000, conducta: 'bazar' }); }
      for (let i = 0; i < Math.max(1, Math.round(m.sis.length / 30)); i++) { const a = rng.el(com.filter(x => !x.ins.some(j => m.ins[j].tipo === 'astillero')).concat(com)); const cap = P.crear(m, { casa: a.id, est: a.est, rol: 'capitan' }); Nv.crear(m, 'astillero_n', { en: a.id, cap: cap.id, dueno: { t: 'F', id: rng.el(casas).id }, est: a.est, conducta: 'astillero_n' }); }
    }
    m.prog(rng.r(8, 40) * S.ANIO, 'arca.aparece', {});
    S.teo.centralidad(m);
  };

  S.sismografo('paro', 'Paro medio', m => { let s = 0; for (const a of m.ase) s += a.paro || 0; return s / m.ase.length; });
  S.sismografo('ipc', 'Coste de la vida medio', m => { let s = 0; for (const a of m.ase) s += a.ipc || 1; return s / m.ase.length; });
})(typeof globalThis !== 'undefined' ? globalThis : this);
