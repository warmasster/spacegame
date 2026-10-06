// Más paneles: lo que se añade a cada ficha (bajas, biografía, lealtades de una guarnición, componentes de una
// nave), fichas nuevas (unidad, diseño) y pestañas nuevas: Personas, Arsenal, Batallas, Economía y Teorías.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI = g.UI || {}; const P = U.paneles; const A = U.arte; const B = S.B; const Te = S.tec;
  const { L, kv, barra, evHtml, esc, n0, pc, estadoDe, dias, av, ban, txtEv, tok, met, ROL } = P.h;
  const V = U.viz; const C = V.SEM; const Pr = U.prefs;
  const M = () => P._m;
  const AB = () => (U.estado ? U.estado.abiertos : null);
  // Un desplegable que recuerda si estaba abierto; lo de dentro solo se monta cuando lo está.
  P.plegable = (k, ab, resumen, cuerpo) => { const abierto = !!(ab && ab.has(k)); return '<details class="pleg" data-k="' + k + '"' + (abierto ? ' open' : '') + '><summary>' + resumen + '</summary>' + (abierto ? cuerpo() : '') + '</details>'; };
  const boton = (acc, txt, extra) => '<button data-accion="' + acc + '"' + (extra || '') + '>' + txt + '</button>';
  const ir = (ref, z, txt) => boton('ir', txt || '📍 Ir', ' data-r="' + ref + '" data-z="' + z + '"');
  const arma = (k) => '<canvas class="arma" data-arma="' + k + '" data-w="20" data-h="20" style="width:20px;height:20px;vertical-align:-5px"></canvas>';

  P.extraAsent = function (a) {
    const m = M();
    return kv([
      a.clase ? ['Lugar en el mundo', a.clase + ' <span class="nota">(valor añadido por habitante: ' + (a.va || 0).toFixed(1) + ')</span>'] : null,
      a.ipc ? ['Coste de la vida', '×' + a.ipc.toFixed(2) + (a.paro > 0.02 ? ' · paro ' + pc(a.paro) : '')] : null,
      a.expect !== undefined ? ['Bienestar / esperado', barra(a.bien || 0, 'v') + (a.bien || 0).toFixed(2) + ' / ' + a.expect.toFixed(2) + (a.brecha > 0.03 ? ' · <b>brecha ' + a.brecha.toFixed(2) + '</b>' : '')] : null,
      a.efic > 0.02 ? ['«Se puede»', barra(a.efic / 0.7, 'r') + a.efic.toFixed(2) + ' <span class="nota">(aquí ha llegado la noticia de una revolución que triunfó)</span>'] : null,
      a.feria > m.t ? ['Feria', 'quedan ' + Math.round(a.feria - m.t) + ' días'] : null,
    ]);
  };
  P.extraPersona = function (p) {
    const m = M(); const b = p.bajas || 0, md = p.mando || 0;
    return kv([
      b || md ? ['Muertes a su cuenta', (b ? '<b>' + n0(b) + '</b> con sus manos' : '') + (b && md ? ' · ' : '') + (md ? '<b>' + n0(md) + '</b> bajo su mando o por orden suya' : '')] : null,
      p.batallas ? ['Batallas', p.batallas + ' (' + (p.victorias || 0) + ' ganadas)'] : null,
      p.en.t === 'V' ? ['De viaje', 'de ' + L('A', p.en.de) + ' a ' + L('A', p.en.a) + ' · llega en ' + dias(p.en.t1 - m.t)] : null,
    ]);
  };
  P.bio = function (p) { const m = M(); const l = m.bio.get(p.id); if (!l || !l.length) return ''; return '<h3>Su vida (' + l.length + ' hechos)</h3>' + l.slice(-14).reverse().map(i => evHtml(m.ev[i])).join(''); };
  P.extraEstado = function (e) {
    const m = M(); if (!e.vivo) return '';
    const al = e.alianzas ? Array.from(e.alianzas).filter(j => m.est[j].vivo) : [];
    return '<div class="filtro">' + boton('arsenal', '⚙ Arsenal y diseños', ' data-id="' + e.id + '"') + ir('E' + e.id, 1.2) + '</div>' + kv([
      e.asabiya !== undefined ? ['Asabiya (cohesión)', barra(e.asabiya, 'v') + e.asabiya.toFixed(2) + ' <span class="nota">· ' + (e.nGob || 1) + '.ª generación de gobernantes</span>'] : null,
      e.psi !== undefined ? ['Tensión estructural Ψ', barra(e.psi / 2, 'r') + e.psi.toFixed(2) + '<br><span class="nota">pueblo ' + (e.mmp || 0).toFixed(2) + ' × élites ' + (e.emp || 0).toFixed(2) + ' (' + (e.aspirantes || 0) + ' aspirantes para ' + (e.puestos || 0) + ' puestos) × tesoro ' + (e.sfd || 0).toFixed(2) + '</span>'] : null,
      al.length ? ['Alianzas', al.map(j => estadoDe(j) + (e.alCon && e.alCon.get(j) ? ' <span class="nota">contra ' + esc(m.est[e.alCon.get(j).contra].nom) + '</span>' : '')).join('<br>')] : null,
      ['Nivel técnico', '×' + e.tec.toFixed(2) + (e.doc ? ' <span class="nota">· doctrina naval ' + e.doc.naval.toFixed(2) + ' · en tierra ' + e.doc.tierra.toFixed(2) + '</span>' : '')],
      e.fase ? ['Fase de la revolución', e.fase] : null,
      e.renta > 0.3 ? ['Vive de rentas', pc(e.renta) + ' del ingreso (peajes y aranceles)'] : null,
    ]);
  };
  P.extraFaccion = function (f) {
    const m = M(); if (f.tipo === 'casa' || f.doc === undefined) return '';
    return kv([['Saben pelear', barra(f.doc, 'r') + f.doc.toFixed(2) + (f.aprendido ? ' <span class="nota">(' + f.aprendido + ' lecciones llegadas con las noticias)</span>' : '')], ['Armas', (f.armas || 0).toFixed(0) + ' t · milicia en la sede: ' + n0(Te.milicia(m, f, m.ase[f.sede]))]]);
  };
  P.genoma = function (g2, q, ref) {
    let h = '<table>'; for (let k = 0; k < 8; k++) h += '<tr><td>' + arma(k) + ' ' + Te.NOM[k] + '</td><td><span class="barra" style="width:120px"><i style="width:' + Math.round(g2[k] / 0.4 * 100) + '%;background:' + Te.COL[k] + '"></i></span>' + pc(g2[k]) + (ref ? ' <span class="nota">' + (g2[k] > ref[k] ? '+' : '') + Math.round((g2[k] - ref[k]) * 100) + '</span>' : '') + '</td>' + (q ? '<td class="n"' + (q[k] < 0.75 ? ' style="color:#ff8a80"' : '') + '>calidad ' + q[k].toFixed(2) + '</td>' : '') + '</tr>'; return h + '</table>';
  };
  P.extraNave = function (n) {
    const m = M(); if (n.dis === undefined || !m.dis[n.dis]) return ''; const d = m.dis[n.dis];
    return '<h3>Diseño y componentes</h3>' + kv([['Diseño', L('D', d.id, d.nom) + ' <span class="nota">generación ' + d.gen + '</span>'], ['Veteranía', barra(n.vet || 0, 'a') + (n.vet || 0).toFixed(2)], ['Pegada', (Te.tasa(m, n, { g: Te.G0, niv: 1 }) * 100).toFixed(1) + ' bajas al día por cada 100 cascos, contra un diseño corriente']]) + P.genoma(d.g, n.q);
  };

  // ── Ficha de una unidad: la cuenta que hace cada soldado, y doce de ellos con cara.
  function unidad(ref) {
    const m = M(); const u = m.uni[ref.id]; const a = m.ase[u.ase]; const e = u.est >= 0 ? m.est[u.est] : null;
    let h = '<div class="cab">' + (u.cmd >= 0 ? av('P' + u.cmd, 64) : '') + '<div><h2>Guarnición de ' + esc(a.nom) + '</h2><div class="sub">' + n0(u.n) + ' soldados · ' + estadoDe(u.est) + '</div></div></div>';
    h += kv([['Manda', L('P', u.cmd)], ['Pagas', u.msc ? '<b>' + u.msc + ' meses sin cobrar</b>' : 'al día'], ['De dónde son', u.ori.map(o => pc(o.f) + ' de ' + L('A', o.a)).join(' · ')], ['Doctrina', barra(u.doc || 0.3, 'a') + (u.doc || 0.3).toFixed(2)], ['Equipo', barra(u.arm === undefined ? 0.8 : u.arm, u.arm < 0.5 ? 'r' : 'v') + (u.arm === undefined ? 0.8 : u.arm).toFixed(2) + (u.arm < 0.5 ? ' <span class="nota">(no llegan armas)</span>' : '')], ['Eficacia en combate', (Te.efTierra(m, u, a) * 100).toFixed(1) + ' bajas al día por cada 100']]);
    if (e) {
      const w = S.soc.pesos(m, u), s = S.soc.tironRegimen(m, u, a); const Lr = S.soc.L(w, s); const dif = 1 - 2 * Lr + 0.2 * (a.nMasacres || 0);
      const F = ['Su comandante', 'Institución y legalidad', 'Ideas', 'Casa y familia', 'Sueldo', 'Quién parece que gana'];
      h += '<h3>La cuenta de cada soldado</h3><table><tr><th>Factor</th><th class="n">Peso</th><th>Tira hacia el régimen</th></tr>' + F.map((f, i) => '<tr><td>' + f + '</td><td class="n">' + w[i].toFixed(2) + '</td><td>' + barra(s[i], s[i] < 0.4 ? 'r' : 'v') + s[i].toFixed(2) + '</td></tr>').join('') + '</table>';
      h += kv([['Lealtad al régimen', '<b>' + Lr.toFixed(2) + '</b> · a la calle ' + (1 - Lr).toFixed(2)], ['Si hoy les mandan disparar', dif > 0.6 ? 'se amotinan con la gente' : dif > 0.4 ? 'desertan' : dif > 0.15 ? 'no disparan' : 'obedecen' + ' <span class="nota">(diferencia ' + dif.toFixed(2) + '; umbrales 0,15 / 0,40 / 0,60)</span>']]);
    }
    h += '<h3>Doce de ellos</h3><div class="caras2">';
    for (let i = 0; i < 12; i++) { const d = A.descSoldado(m, u, i); const o = u.ori[Math.min(u.ori.length - 1, Math.floor(i / 12 * u.ori.length + 0.3))]; h += av('U' + u.id + '_' + i, 34) + '<span>' + esc(d.nom) + '<br><span class="nota">' + Math.round(d.edad) + ' años · de ' + esc(m.ase[o.a].nom) + '</span></span>'; }
    return h + '</div>';
  }
  function diseno(ref) {
    const m = M(); const d = m.dis[ref.id]; const e = d.est >= 0 ? m.est[d.est] : null; const li = e && e.dis ? e.dis[d.cls] : null;
    let h = '<h2>' + esc(S.cap(d.nom)) + '</h2><div class="sub">' + S.reg.nave[d.cls].nom + ' · ' + (e ? estadoDe(e.id) : 'sin Estado') + (li && li.mejor === d ? ' · <b>en producción</b>' : '') + '</div>';
    h += '<canvas class="plano" data-nave="D' + d.id + '" data-w="388" data-h="132" style="width:100%;height:132px"></canvas>';
    h += kv([['Generación', 'Mk ' + Te.romano(d.gen) + ' <span class="nota">(' + m.fecha(d.t0) + ')</span>'], d.padres.length ? ['Sale de cruzar', d.padres.map(i => L('D', i, m.dis[i].nom + ' #' + i)).join(' × ')] : null,
      ['Aptitud', '<b>' + d.fit.toFixed(2) + '</b> <span class="nota">(relación de bajas: 1 = empate)</span>'], ['En maniobras', d.sim.toFixed(2) + ' contra lo que se cree del rival'], d.nReal ? ['En batalla', d.real.toFixed(2) + ' en ' + d.batallas + ' batallas · ' + n0(d.bajasC) + ' causadas / ' + n0(d.bajasS) + ' sufridas'] : ['En batalla', 'sin probar'], ['Cascos construidos', n0(d.construidas)]]);
    h += '<h3>Reparto del tonelaje</h3>' + P.genoma(d.g, null, Te.G0);
    if (d.ev !== undefined && d.ev >= 0) h += evHtml(m.ev[d.ev]);
    return h;
  }
  P.extra = { U: unidad, D: diseno };

  // ── Personas.
  const FILTROS = {
    historicos: ['Los que cambiaron la historia', (m) => { const q = S.his.pesos(m); return q.orden.filter(i => q.w[i] > 0).map(i => m.per[i]); }, (m, p) => (P.filtroHistoricos ? P.filtroHistoricos[2](m, p) : 'peso ' + S.his.pesos(m).w[p.id].toFixed(0))],
    mortiferos: ['Los más mortíferos', (m) => m.per.filter(p => S.mas.letalidad(p) > 0).sort((x, y) => S.mas.letalidad(y) - S.mas.letalidad(x)), (m, p) => '<b>' + n0(S.mas.letalidad(p)) + ' muertes</b>: ' + n0(p.bajas || 0) + ' con sus manos, ' + n0(p.mando || 0) + ' bajo su mando'],
    gobernantes: ['Quienes gobiernan', (m) => m.est.filter(e => e.vivo && e.gob >= 0).sort((x, y) => (y.pob || 0) - (x.pob || 0)).map(e => m.per[e.gob]), (m, p) => { const e = m.est[p.est]; return n0(e.pob || 0) + ' súbditos · en el poder ' + dias(m.t - e.desde) + ' · asabiya ' + (e.asabiya === undefined ? 0.6 : e.asabiya).toFixed(2); }],
    herederos: ['Herederos', (m) => m.est.filter(e => e.vivo && e.corte.heredero >= 0 && m.per[e.corte.heredero].vivo).map(e => m.per[e.corte.heredero]), (m, p) => 'hereda ' + esc(m.est[p.est].nom) + ' · ambición ' + p.r[S.R.AMB].toFixed(2)],
    mandos: ['Almirantes y generales', (m) => m.per.filter(p => p.vivo && p.cargo && (p.cargo.t === 'almirante' || p.cargo.t === 'general')).sort((x, y) => (y.victorias || 0) * 3 + (y.batallas || 0) - (x.victorias || 0) * 3 - (x.batallas || 0)), (m, p) => (p.batallas || 0) + ' batallas, ' + (p.victorias || 0) + ' ganadas · valor ' + p.r[S.R.VAL].toFixed(2)],
    ricos: ['Los más ricos', (m) => m.per.filter(p => p.vivo).sort((x, y) => y.din - x.din), (m, p) => n0(p.din) + ' créditos'],
    vengadores: ['Con una libreta', (m) => (m.vengadores || []).filter(p => p.vivo && p.ven && p.ven.estado === 'activa').sort((x, y) => S.ven.actual(m, y) - S.ven.actual(m, x)), (m, p) => 'rencor ' + S.ven.actual(m, p).toFixed(2) + ' · ' + (p.ven.obj >= 0 ? 'busca a ' + L('P', p.ven.obj) : 'no sabe a quién') + ' · ' + (p.ven.paso || '').replace(/_/g, ' ')],
    piratas: ['Piratas y corsarios', (m) => m.per.filter(p => p.vivo && (p.rol === 'pirata' || p.rol === 'corsario')).sort((x, y) => S.mas.letalidad(y) - S.mas.letalidad(x)), (m, p) => (p.cargo ? esc(p.cargo.nom) : 'pirata') + (S.mas.letalidad(p) ? ' · ' + n0(S.mas.letalidad(p)) + ' muertes' : '')],
    cabecillas: ['Cabecillas', (m) => m.fac.filter(f => f.vivo && f.tipo !== 'casa' && f.lid >= 0 && m.per[f.lid].vivo).sort((x, y) => y.O - x.O).map(f => m.per[f.lid]), (m, p) => { const f = m.fac[p.fac]; return f ? esc(f.nom || 'protofacción') + ' · ' + n0(S.fac.miembros(m, f)) + ' miembros' : ''; }],
    aspirantes: ['Élites sin puesto', (m) => m.per.filter(p => p.vivo && p.rol === 'aspirante').sort((x, y) => y.r[S.R.AMB] - x.r[S.R.AMB]), (m, p) => 'ambición ' + p.r[S.R.AMB].toFixed(2) + ' · lealtad ' + p.r[S.R.LEA].toFixed(2)],
    presos: ['Presos y exiliados', (m) => m.per.filter(p => p.vivo && (p.rol === 'preso' || p.rol === 'exiliado')), (m, p) => ROL[p.rol]],
    viajeros: ['De viaje ahora', (m) => m.per.filter(p => p.vivo && p.en.t === 'V'), (m, p) => 'hacia ' + L('A', p.en.a) + ' · llega en ' + dias(p.en.t1 - m.t)],
    viejos: ['Los más viejos', (m) => m.per.filter(p => p.vivo).sort((x, y) => x.nace - y.nace), (m, p) => Math.floor(S.per.edad(m, p)) + ' años'],
    muertos: ['Muertos recientes', (m) => m.per.filter(p => !p.vivo && p.muere !== undefined && p.evMuerte !== undefined && p.evMuerte >= 0).sort((x, y) => y.muere - x.muere), (m, p) => esc(m.texto(m.ev[p.evMuerte]))],
    tocados: ['Gente que tocaste', (m) => m.per.filter(p => p.coh !== undefined && p.rol !== 'lider'), (m, p) => p.vivo ? 'ya no es un número' : 'murió'],
  };
  // Lo que mide cada filtro (si mide algo): con eso se pinta la barra de cada persona, relativa a la primera.
  const METRICA = {
    historicos: [(m, p) => S.his.pesos(m).w[p.id], C.ambar],
    mortiferos: [(m, p) => S.mas.letalidad(p), C.rojo], gobernantes: [(m, p) => (m.est[p.est] ? m.est[p.est].pob || 0 : 0), C.azul], mandos: [(m, p) => (p.victorias || 0) * 3 + (p.batallas || 0), C.ambar], ricos: [(m, p) => Math.max(0, p.din), C.verde],
    vengadores: [(m, p) => S.ven.actual(m, p), C.rojo], piratas: [(m, p) => S.mas.letalidad(p), C.rojo], aspirantes: [(m, p) => p.r[S.R.AMB], C.violeta], viejos: [(m, p) => S.per.edad(m, p), C.gris],
  };
  // ── Las muertes a la cuenta de alguien, una a una: a quién, cómo y por qué (el hecho del que cuelga la cadena).
  const CLM = { batalla: ['en batallas', C.azul], tierra: ['en combates en tierra', C.ambar], pueblo: ['gente desarmada', C.rojo], nave: ['tripulaciones cazadas', C.violeta], persona: ['con nombre y apellidos', C.rosa] };
  const SALTA = { batalla: 1, batalla_fin: 1, muerte: 1 };
  P.muertesDe = function (m, p) {
    const l = (p.mue || []).slice().reverse(); if (!l.length) return '<p class="nota">Nada apuntado.</p>';
    const por = {}; for (const x of l) por[x.c] = (por[x.c] || 0) + x.n;
    let h = V.reparto(Object.keys(por).map(k => ({ nom: (CLM[k] || [k])[0], v: por[k], col: (CLM[k] || ['', C.gris])[1] })), { fmt: n0 });
    for (const x of l.slice(0, 80)) {
      // El porqué: el hecho del que cuelga la muerte y, hacia atrás, hasta dos de sus causas.
      const d = S.mas.muerte(m, x); const e = d.ev >= 0 ? m.ev[d.ev] : null; const cad = [];
      if (e) { let c = e; for (let k = 0; c && cad.length < 3 && k < 8; k++) { if (!SALTA[c.k] || (!c.c.length && !cad.length)) cad.push(c); c = c.c.length ? m.ev[c.c[0]] : null; } }
      h += '<div class="muerte" style="border-left-color:' + (CLM[x.c] || ['', C.gris])[1] + '"><div class="q">' + (x.c === 'persona' ? av('P' + x.v, 26) : '') + '<b>' + tok(d.que) + '</b> ' + V.insignia(x.k === 'bajas' ? 'con sus manos' : 'bajo su mando', x.k === 'bajas' ? 'mal' : '') + '</div><div class="nota">' + m.fecha(x.t) + '</div><div>' + tok(d.como) + '</div>' + (cad.length ? '<div class="porque"><b>Por qué:</b> ' + cad.map((c, i) => (i ? '<br>↳ y eso, porque: ' : '') + txtEv(c)).join('') + ' <a class="ref" data-ev="' + e.id + '">toda la cadena →</a></div>' : '') + '</div>';
    }
    const resto = l.slice(80).reduce((s, x) => s + x.n, 0) + (p.mueAnt || 0); if (resto > 0) h += '<p class="nota">… y ' + n0(resto) + ' muertes más, anteriores.</p>';
    return h;
  };
  P.muertesPleg = (m, p, ab) => (p.mue && p.mue.length ? P.plegable('k' + p.id, ab, '🩸 Sus muertes, una a una <span class="nota">(' + n0(S.mas.letalidad(p)) + ' en ' + p.mue.length + (p.mue.length === 1 ? ' hecho' : ' hechos') + ')</span>', () => '<div class="cuerpo">' + P.muertesDe(m, p) + '</div>') : '');
  P.muertesFicha = (p) => { const m = M(); return p.mue && p.mue.length ? P.muertesPleg(m, p, AB()) : ''; };
  P.personas = function (mm, op) {
    P.usar(mm); const m = mm; const f = FILTROS[op.filtro] || FILTROS.mortiferos; const ab = AB(); const me = METRICA[op.filtro];
    let h = '<h2>Personas</h2><div class="filtro"><select data-op="pf">' + Object.keys(FILTROS).map(k => '<option value="' + k + '"' + (k === op.filtro ? ' selected' : '') + '>' + FILTROS[k][0] + '</option>').join('') + '</select><select data-op="pe"><option value="-1">todos los Estados</option>' + m.est.filter(e => e.vivo).map(e => '<option value="' + e.id + '"' + (e.id === op.est ? ' selected' : '') + '>' + esc(e.nom) + '</option>').join('') + '</select></div>';
    let l = f[1](m); if (op.est >= 0) l = l.filter(p => p.est === op.est);
    const top = l.slice(0, 50); let mxv = 0; if (me) for (const p of top) mxv = Math.max(mxv, me[0](m, p));
    h += '<div class="nota">' + l.length + ' personas' + (l.length > 50 ? ' (se enseñan las 50 primeras)' : '') + '. Quien tiene muertes a su cuenta las lleva apuntadas una a una: pulsa en 🩸.</div>';
    top.forEach((p, i) => { h += '<div class="fila pers"><span class="rk">' + (i + 1) + '</span>' + av('P' + p.id, 44) + '<div>' + L('P', p.id) + (p.vivo ? '' : ' †') + ' <span class="nota">' + (p.cargo && p.cargo.nom ? esc(p.cargo.nom) : (ROL[p.rol] || p.rol)) + (p.est >= 0 ? ' · ' + esc(m.est[p.est].nom) : '') + '</span><br>' + f[2](m, p) + (me && mxv > 0 ? V.medidor(me[0](m, p) / mxv, me[1]) : '') + P.muertesPleg(m, p, ab) + '</div></div>'; });
    return h + (l.length ? '' : '<p class="nota">Nadie, de momento.</p>');
  };

  // ── Arsenal: los diseños de cada Estado, cómo han evolucionado y qué cree que tiene el rival.
  P.arsenal = function (mm, op) {
    P.usar(mm); const m = mm; const vivos = m.est.filter(e => e.vivo && e.dis); if (!vivos.length) return '<h2>Arsenal</h2><p class="nota">Aún no hay Estados con oficina de diseño.</p>';
    const e = vivos.find(x => x.id === op.est) || vivos[0];
    let h = '<h2>Arsenal</h2><div class="filtro"><select data-op="ae">' + vivos.map(x => '<option value="' + x.id + '"' + (x === e ? ' selected' : '') + '>' + esc(x.nom) + '</option>').join('') + '</select></div>';
    h += '<p class="nota">Cada año la oficina de diseño conserva los diseños con mejor relación de bajas (en batallas reales y en maniobras contra lo que <i>cree</i> que tiene el rival), los cruza y los muta. Lo que ves es la generación en producción.</p>' + V.leyenda(Te.NOM.map((x, k) => [Te.COL[k], x]));
    h += kv([['Nivel técnico', '×' + e.tec.toFixed(2)], ['Doctrina naval', barra(e.doc.naval, 'a') + e.doc.naval.toFixed(2)], ['Doctrina en tierra', barra(e.doc.tierra, 'a') + e.doc.tierra.toFixed(2)], ['Flota', n0(e.navG || 0) + ' naves en ' + (e.escuadras || 0) + ' escuadras']]);
    for (const cls of ['fragata', 'crucero', 'acorazado']) {
      const li = e.dis[cls]; const d = li.mejor;
      h += '<h3>' + S.reg.nave[cls].nom.replace('Escuadra de ', '').replace('División de ', '') + ' · ' + L('D', d.id, d.nom) + '</h3><canvas class="plano" data-nave="D' + d.id + '" data-w="388" data-h="110" style="width:100%;height:110px"></canvas>';
      h += '<div class="nota">aptitud ' + d.fit.toFixed(2) + (d.nReal ? ' · en batalla ' + d.real.toFixed(2) + ' (' + d.batallas + ')' : ' · sin probar en batalla') + ' · ' + li.gen + ' generaciones · variantes: ' + li.pob.map(v => L('D', v.id, v.fit.toFixed(2))).join(' ') + '</div>' + P.genoma(d.g, null, Te.G0);
      if (li.hist.length > 1) h += '<div class="nota">Cómo ha ido cambiando el reparto, generación a generación:</div><canvas class="graf" data-apilada="' + V.dato('evo' + e.id + cls, { capas: Te.COL.map((col, k) => ({ col, d: li.hist.map(x => x.g[k]) })), ini: 'Mk I', fin: 'Mk ' + Te.romano(li.hist[li.hist.length - 1].gen) }) + '" style="height:92px"></canvas>';
    }
    const riv = Te.rivales(m, e).filter(r => r.j >= 0);
    if (riv.length) h += '<h3>Lo que cree del rival</h3><table><tr><th>Rival</th><th>Sabe</th><th>Mejor arma contra él</th></tr>' + riv.map(r => { let k = 0; for (let i = 1; i < 3; i++) if (r.g[3 + i] < r.g[3 + k]) k = i; return '<tr><td>' + estadoDe(r.j) + '</td><td>' + (r.t >= 0 ? 'visto hace ' + dias(m.t - r.t) : '<span class="nota">nada: supone un diseño corriente</span>') + '</td><td>' + arma(k) + ' ' + Te.NOM[k] + '</td></tr>'; }).join('') + '</table>';
    h += '<h3>Escuadras</h3>';
    for (const fid of e.flo) { const f = m.flo[fid]; if (!f.vivo || !f.nav.length) continue; h += '<div class="nota"><b>' + esc(f.nom) + '</b> · ' + L('P', f.alm) + ' · ' + f.st + '</div><table>' + f.nav.map(i => m.nav[i]).filter(n => n.vivo).map(n => '<tr><td>' + L('N', n.id) + (f.insignia === n.id ? ' ✦' : '') + '</td><td class="n">' + n.cascos + '</td><td>' + esc(m.dis[n.dis].nom.replace('clase ', '')) + '</td><td>' + barra(n.vet || 0, 'a') + '</td><td class="n">' + (n.q.reduce((x, y) => x + y, 0) / 8).toFixed(2) + '</td></tr>').join('') + '</table>'; }
    const reb = m.fac.filter(f => f.vivo && f.doc !== undefined && f.tipo !== 'casa').sort((x, y) => y.doc - x.doc).slice(0, 12);
    if (reb.length) h += '<h3>Quien aprende a pelear sin uniforme</h3>' + reb.map(f => '<div>' + L('F', f.id, f.nom || 'protofacción') + ' ' + barra(f.doc, 'r') + f.doc.toFixed(2) + ' <span class="nota">' + (f.armas || 0).toFixed(0) + ' t de armas</span></div>').join('');
    return h;
  };

  // ── Batallas.
  P.batallas = function (mm) {
    P.usar(mm); const m = mm; let h = '<h2>Batallas</h2>';
    const viv = m.bat.filter(b => b.vivo), tv = (m.tie || []).filter(t => t.vivo);
    h += '<h3>Ahora mismo (' + (viv.length + tv.length) + ')</h3>';
    for (const b of viv) h += '<div class="tarjeta">' + ban('E' + b.L[0].est, 30, 20) + ' <b>' + n0(b.L[0].n) + '</b> ⚔ <b>' + n0(b.L[1].n) + '</b> ' + ban('E' + b.L[1].est, 30, 20) + ' ' + ir('S' + b.sis, 7, '🔍 Verla') + '<br>' + L('S', b.sis) + ' · día ' + Math.max(1, Math.round(m.t - b.t0)) + (b.ley === 'lineal' ? ' · entre asteroides' : '') + '<canvas class="graf" data-bat="' + b.id + '" style="height:70px"></canvas></div>';
    for (const t of tv) h += '<div class="tarjeta"><b>' + n0(t.atk.n) + '</b> ' + esc(t.atk.nom) + ' ⚔ <b>' + n0(t.def.n) + '</b> ' + esc(t.def.nom) + ' ' + ir('A' + t.a, 13, '🔍 Verlo') + '<br>' + (t.tipo === 'asalto' ? 'Asalto a ' : 'Insurrección en ') + L('A', t.a) + ' · día ' + Math.max(1, Math.round(m.t - t.t0)) + '<canvas class="graf" data-tie="' + t.id + '" style="height:70px"></canvas></div>';
    if (!viv.length && !tv.length) h += '<p class="nota">Ninguna. Puedes provocar una con la mano (⚔ Provocar una guerra) y esperar a que lleguen las órdenes a las flotas.</p>';
    const pas = m.bat.filter(b => !b.vivo).slice(-14).reverse();
    if (pas.length) { h += '<h3>Las últimas en el espacio</h3>'; for (const b of pas) h += '<div class="tarjeta">' + ban('E' + b.L[0].est, 24, 16) + ' ' + n0(b.L[0].n0) + ' → ' + n0(b.L[0].nFin) + (b.gano === 0 ? ' 🏆' : '') + ' &nbsp;⚔&nbsp; ' + n0(b.L[1].n0) + ' → ' + n0(b.L[1].nFin) + (b.gano === 1 ? ' 🏆' : '') + ' ' + ban('E' + b.L[1].est, 24, 16) + '<canvas class="graf" data-bat="' + b.id + '" style="height:54px"></canvas>' + (b.fin !== undefined ? evHtml(m.ev[b.fin]) : '') + '</div>'; }
    const pt = (m.tie || []).filter(t => !t.vivo).slice(-10).reverse();
    if (pt.length) { h += '<h3>Las últimas en tierra</h3>'; for (const t of pt) h += '<div class="tarjeta">' + (t.tipo === 'asalto' ? 'Asalto a ' : 'Insurrección en ') + L('A', t.a) + ': ' + n0(t.atk.n0) + ' → ' + n0(t.atk.n) + (t.gano === 0 ? ' 🏆' : '') + ' ⚔ ' + n0(t.def.n0) + ' → ' + n0(t.def.n) + (t.gano === 1 ? ' 🏆' : '') + ' · ' + n0(t.muertos || 0) + ' muertos<canvas class="graf" data-tie="' + t.id + '" style="height:54px"></canvas></div>'; }
    return h;
  };

  // ── Economía.
  P.economia = function (mm) {
    P.usar(mm); const m = mm; const N = S.NB; const prod = new Array(N).fill(0), dem = new Array(N).fill(0), st = new Array(N).fill(0), pr = new Array(N).fill(0), mov = new Array(N).fill(0); let pob = 0;
    for (const a of m.ase) { pob += a.pob; for (let c = 0; c < N; c++) { prod[c] += a.prod[c]; dem[c] += a.dem[c]; st[c] += a.alm[c]; pr[c] += a.pr[c] / S.bien[c].pref / m.ase.length; } }
    const fl = Array.from(m.flu.values()); for (const f of fl) for (let c = 0; c < N; c++) mov[c] += f.c[c];
    // Desigualdad entre asentamientos (Gini del valor añadido por habitante, ponderado por población).
    const vs = m.ase.filter(a => a.va !== undefined).map(a => [a.va, a.pob]).sort((x, y) => x[0] - y[0]); let gini = 0;
    if (vs.length) { let tp = 0, tv = 0; for (const [v, p] of vs) { tp += p; tv += v * p; } let cp = 0, cv = 0, area = 0; for (const [v, p] of vs) { const cp2 = cp + p / tp, cv2 = cv + v * p / Math.max(1e-9, tv); area += (cp2 - cp) * (cv + cv2) / 2; cp = cp2; cv = cv2; } gini = 1 - 2 * area; }
    let paro = 0, ipc = 0; for (const a of m.ase) { paro += (a.paro || 0) * a.pob; ipc += (a.ipc || 1) * a.pob; }
    let amar = 0, merc = 0; for (const n of m.nav) if (n.vivo && (n.cls === 'carguero' || n.cls === 'bazar')) { if (n.st === 'amarrada') amar++; else merc++; }
    let h = '<h2>Economía</h2><p class="nota">No hay mercado galáctico: cada asentamiento pone su precio. Aquí, la suma.</p>';
    h += V.kpis([V.kpi(S.fmt(pob), 'Población', { spark: 'pob', col: C.azul }), V.kpi('×' + (ipc / pob).toFixed(2), 'Coste de la vida', { spark: 'ipc', col: C.ambar }), V.kpi(pc(paro / pob), 'Paro', { spark: 'paro', col: C.rojo }), V.kpi(gini.toFixed(2), 'Desigualdad entre mundos', { col: C.violeta, sub: 'índice de Gini' }), V.kpi(merc, 'Mercantes navegando', { col: C.verde, sub: amar + ' amarrados sin negocio' }), V.kpi(m.ase.reduce((s, a) => s + a.nuc.length, 0), 'Núcleos en almacén', { spark: 'nucleos', col: C.gris })]);
    h += '<h3>Lo que se produce y lo que se gasta</h3><table><tr><th>Bien</th><th>Produce · gasta al día</th><th class="n">Almacén</th><th class="n">Viaja/mes</th><th class="n">Precio</th></tr>';
    for (let c = 0; c < N; c++) { const mxg = Math.max(prod[c], dem[c], 1e-9); h += '<tr><td><span class="chip" style="background:' + S.bien[c].col + '"></span>' + S.bien[c].nom + '</td><td><span class="doble">' + V.medidor(prod[c] / mxg, S.bien[c].col) + V.medidor(dem[c] / mxg, C.gris) + '</span><span class="nota">' + S.fmt(prod[c]) + ' · ' + S.fmt(dem[c]) + '</span></td><td class="n">' + S.fmt(st[c]) + '</td><td class="n">' + S.fmt(mov[c]) + '</td><td class="n">' + V.insignia('×' + pr[c].toFixed(2), pr[c] > 1.6 ? 'mal' : pr[c] > 1.2 ? 'aviso' : pr[c] < 0.8 ? 'ok' : '') + '</td></tr>'; }
    h += '</table><p class="nota">En cada bien, la barra de color es lo que sale de campos y fábricas; la gris, lo que se consume. El precio va contra el de referencia (×1).</p>';
    const rutas = fl.sort((x, y) => y.q - x.q).slice(0, 12); const qm = rutas.length ? Math.max(1, rutas[0].q) : 1;
    h += '<h3>Rutas con más carga</h3>' + (rutas.map(f => { let c = 0; for (let k = 1; k < N; k++) if (f.c[k] > f.c[c]) c = k; return '<div class="flujo">' + L('A', f.de) + ' → ' + L('A', f.a) + ' <span class="nota">' + S.fmt(f.q) + ' t/mes · sobre todo ' + S.bien[c].nom.toLowerCase() + '</span>' + V.medidor(f.q / qm, S.bien[c].col) + '</div>'; }).join('') || '<span class="nota">aún nada</span>');
    const es = m.est.filter(e => e.vivo); const deu = new Map(es.map(e => [e.id, S.pol.deuda(m, e)])); const mh = Math.max(1, ...es.map(e => Math.max(e.tes, deu.get(e.id))));
    h += '<h3>Haciendas</h3>' + es.map(e => '<div class="fila">' + ban('E' + e.id, 34, 23) + '<div>' + L('E', e.id) + ' ' + V.insignia('precios ×' + e.mon.P.toFixed(2), e.mon.P > 1.3 ? 'mal' : e.mon.P > 1.1 ? 'aviso' : '') + ' <span class="nota">impuestos ' + pc(e.imp) + '</span><div class="met" style="grid-template-columns:1fr 1fr">' + met('Tesoro', S.fmt(e.tes), Math.max(0, e.tes) / mh, C.verde) + met('Deuda', S.fmt(deu.get(e.id)), deu.get(e.id) / mh, C.rojo) + '</div></div></div>').join('');
    const casas = m.fac.filter(f => f.vivo && f.tipo === 'casa').sort((x, y) => y.caja - x.caja); const mc = Math.max(1, ...casas.map(f => f.caja));
    h += '<h3>Casas mercantes</h3>' + casas.map(f => '<div class="flujo">' + ban('F' + f.id, 24, 16) + ' ' + L('F', f.id) + ' <span class="nota">caja ' + S.fmt(f.caja) + ' · ' + (f.naves || 0) + ' naves</span>' + V.medidor(Math.max(0, f.caja) / mc, C.verde) + '</div>').join('');
    const inv = []; for (let i = m.ev.length - 1; i >= 0 && inv.length < 8; i--) if (m.ev[i].k === 'fundacion' || m.ev[i].k === 'acaparamiento' || m.ev[i].k === 'bancarrota' || m.ev[i].k === 'feria') inv.push(m.ev[i]);
    if (inv.length) h += '<h3>Lo último</h3>' + inv.map(e => evHtml(e)).join('');
    return h;
  };

  // ── Teorías: qué son, de quién y qué están haciendo ahora mismo.
  P.teorias = function (mm) {
    P.usar(mm); const m = mm; const por = {}; for (const id in S.reg.teoria) { const t = S.reg.teoria[id]; (por[t.campo] = por[t.campo] || []).push(t); }
    let h = '<h2>Teorías en marcha</h2><p class="nota">Nada de lo que pasa aquí está escrito: sale de estos modelos, tomados de la ciencia política, la sociología, la geopolítica y la economía, con parámetros de juego encima.</p>';
    for (const c of ['Revuelta', 'Estado', 'Geopolítica', 'Economía', 'Guerra']) { if (!por[c]) continue; h += '<h3>' + c + '</h3>'; for (const t of por[c]) { let ind = ''; try { ind = t.ind ? t.ind(m) : ''; } catch (e) { ind = ''; } h += '<div class="teoria"><b>' + esc(t.nom) + '</b> <span class="nota">' + esc(t.autor) + '</span><br>' + esc(t.que) + '<br><span class="aqui">Aquí: ' + esc(t.aqui) + '</span>' + (ind ? '<br><span class="ahora">Ahora: ' + esc(ind) + '</span>' : '') + '</div>'; } }
    return h;
  };

  // ── Lienzos de arte y gráficas pequeñas.
  function lienzo(cv) { const dpr = g.devicePixelRatio || 1; const w = +cv.dataset.w || cv.clientWidth || 360, h = +cv.dataset.h || cv.clientHeight || 80; cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr); const c = cv.getContext('2d'); c.setTransform(dpr, 0, 0, dpr, 0, 0); return [c, w, h]; }
  P.desc = function (m, ref) {
    if (!ref) return null;
    if (ref[0] === 'P') return m.per[+ref.slice(1)] ? A.descPersona(m, m.per[+ref.slice(1)]) : null;
    if (ref[0] === 'C') { const [i, coh, a] = ref.slice(1).split('_').map(Number); const co = m.coh[coh]; if (!co) return null; const r = new S.Rng(S.hash(m.semilla, 'cara', coh, i)); const sexo = r.p(0.5) ? 'M' : 'H'; S.per.nombre(r, sexo); const edad = Math.floor(r.r(17, 68)); return A.descCara(m, m.ase[a], { coh, i, sexo, edad, odia: false }); }
    if (ref[0] === 'U') { const [u, i] = ref.slice(1).split('_').map(Number); return m.uni[u] ? A.descSoldado(m, m.uni[u], i) : null; }
    return null;
  };
  P.pintarArte = function (m, raiz) {
    for (const cv of raiz.querySelectorAll('canvas[data-av]')) { const d = P.desc(m, cv.dataset.av); if (!d) continue; const [c, w] = lienzo(cv); A.avatar(c, 0, 0, w, d); }
    for (const cv of raiz.querySelectorAll('canvas[data-ban]')) { const r = cv.dataset.ban; if (!r) continue; const id = +r.slice(1); if (id < 0 || !(r[0] === 'E' ? m.est[id] : m.fac[id])) continue; const [c, w, h] = lienzo(cv); A.banderaDe(c, 0, 0, w, h, m, r[0], id); }
    for (const cv of raiz.querySelectorAll('canvas[data-nave]')) {
      const r = cv.dataset.nave; if (r === undefined) continue; const [c, w, h] = lienzo(cv);
      if (r[0] === 'D') { const d = m.dis[+r.slice(1)]; if (!d) continue; A.nave(c, 0, 0, w, h, m, { id: d.id, cls: d.cls, col: 1 + d.id % 7, dueno: { t: 'E', id: d.est }, pin: [], cic: [], dan: 0, vivo: true, nom: S.reg.nave[d.cls].nom, dis: d.id, q: null, cascos: 1, extra: [] }); }
      else if (m.nav[+r]) A.nave(c, 0, 0, w, h, m, m.nav[+r]);
    }
    for (const cv of raiz.querySelectorAll('canvas[data-arma]')) { if (cv.dataset.arma === undefined) continue; const [c, w] = lienzo(cv); A.arma(c, 0, 0, w, +cv.dataset.arma); }
    // Los combates, con cada bando en el color que lleva en el mapa y en su bloque de la tarjeta.
    const duelo = (cv, hist, k, nom) => V.series(cv, { eje: 'dias', tipo: Pr.v.graf === 'barras' ? 'area' : undefined, fmt: (x) => n0(x), series: [{ nom: nom[0], col: k[0], d: hist.map(p => p[0]) }, { nom: nom[1], col: k[1], d: hist.map(p => p[1]) }] });
    const ne = (id) => (id >= 0 && m.est[id] ? m.est[id].nom : 'sin Estado');
    for (const cv of raiz.querySelectorAll('canvas[data-bat]')) { const b = cv.dataset.bat !== undefined ? m.bat[+cv.dataset.bat] : null; if (b) duelo(cv, b.hist, U.local.colBatalla(m, b), [ne(b.L[0].est), ne(b.L[1].est)]); }
    for (const cv of raiz.querySelectorAll('canvas[data-tie]')) { const t = cv.dataset.tie !== undefined ? m.tie[+cv.dataset.tie] : null; if (t) duelo(cv, t.hist, U.local.colTierra(m, t), [t.tipo === 'asalto' ? 'Asaltantes' : 'Insurrectos', 'Defensores']); }
    for (const cv of raiz.querySelectorAll('canvas[data-mini]')) { if (!cv.dataset.mini) continue; const [c, w, h] = lienzo(cv); c.save(); c.translate(w * 0.55, h / 2); A.naveMini(c, cv.dataset.mini, w * 0.4, cv.dataset.col || '#bbb', 'rgba(255,255,255,0.6)'); c.restore(); }
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
