// Batallas: cada combate en una tarjeta y, dentro, cada bando en su bloque y con su color (el mismo que llevan
// sus naves en el mapa y su línea en la gráfica): quién es, qué pinta ahí, qué tiene, por qué pega como pega
// y quién va ganando.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI; const P = U.paneles; const Lo = U.local; const Co = S.com;
  const { L, esc, n0, pc, av, ban } = P.h;
  const AB = () => (U.estado ? U.estado.abiertos : null);
  const ir = (ref, z, txt) => '<button data-accion="ir" data-r="' + ref + '" data-z="' + z + '">' + txt + '</button>';
  const mini = (cls, col) => '<canvas data-mini="' + cls + '" data-col="' + col + '" data-w="28" data-h="14" style="width:28px;height:14px;vertical-align:-2px"></canvas>';
  const chip = (col) => '<span class="chip" style="background:' + col + '"></span>';
  const vida = (f, col, corte) => '<span class="vida"><i style="width:' + Math.round(S.clamp(f, 0, 1) * 100) + '%;background:' + col + '"></i>' + (corte ? '<b style="left:' + Math.round(corte * 100) + '%"></b>' : '') + '</span>';
  const balanza = (f, cols, cortes) => '<span class="balanza"><i style="width:' + (f * 100).toFixed(1) + '%;background:' + cols[0] + '"></i><i style="width:' + ((1 - f) * 100).toFixed(1) + '%;background:' + cols[1] + '"></i>' + (cortes || []).map(x => '<b style="left:' + (x * 100).toFixed(1) + '%"></b>').join('') + '</span>';
  const x2 = (v) => '×' + v.toFixed(2);
  const pm = (v) => (v >= 1 ? '+' : '−') + Math.round(Math.abs(v - 1) * 100) + ' %';
  const nd = (d) => (d < 60 ? d + (d === 1 ? ' día' : ' días') : Math.round(d / 30) + ' meses');
  const nomCls = (cls) => S.reg.nave[cls].nom.replace('Escuadra de ', '').replace('División de ', '').toLowerCase();
  const nomEst = (m, id) => (id >= 0 && m.est[id] ? m.est[id].nom : 'gente sin Estado');
  const color = (col, txt) => '<b style="color:' + col + '">' + txt + '</b>';
  const plegable = P.plegable;

  // ── Por qué pega como pega una flota: lo que le ayuda y lo que le pesa, en palabras; y la cuenta entera aparte.
  function razones(d) {
    const f = [
      [d.dis, d.dis >= 1 ? 'sus armas encajan contra las defensas del rival' : 'las defensas del rival paran buena parte de sus armas'],
      [d.tec, d.tec >= 1 ? 'técnica superior' : 'técnica inferior'],
      [1 + 0.5 * d.vet, 'tripulaciones veteranas'],
      [0.8 + 0.5 * d.doc, d.doc >= 0.4 ? 'buena doctrina naval' : 'poca doctrina naval (apenas ha combatido)'],
      [d.moral.cohesion, d.moral.cohesion >= 1 ? 'un Estado unido' : 'un Estado desunido'],
      [1 - 0.35 * d.moral.lejos, 'pelea lejos de su base'],
      [d.moral.paga, d.moral.msc + ' meses sin cobrar'],
      [d.moral.mando || 1, 'un almirante invicto al mando'],
    ];
    const mas = f.filter(x => x[0] > 1.04).sort((a, b) => b[0] - a[0]), menos = f.filter(x => x[0] < 0.96).sort((a, b) => a[0] - b[0]);
    const fr = (l) => l.map(x => x[1] + ' (' + pm(x[0]) + ')').join(', ');
    return ((mas.length ? '<span class="mas">A favor:</span> ' + fr(mas) + '. ' : '') + (menos.length ? '<span class="menos">En contra:</span> ' + fr(menos) + '.' : '')) || 'Nada destaca: una flota corriente.';
  }
  function cuenta(d, ef) {
    const fila = (a, b, c) => '<tr><td>' + a + '</td><td>' + b + '</td><td class="n">' + c + '</td></tr>';
    return '<table><tr><th>Factor</th><th>Valor</th><th class="n">Efecto</th></tr>' +
      fila('Tipo de naves', 'lo que pegan de fábrica, por cada 100', (d.base * 100).toFixed(1)) +
      fila('Diseño contra el del rival', 'sus armas frente a las defensas de enfrente', x2(d.dis)) +
      fila('Nivel técnico', d.niv.toFixed(2) + ' frente a ' + d.nivRival.toFixed(2), x2(d.tec)) +
      fila('Veteranía', d.vet.toFixed(2), x2(1 + 0.5 * d.vet)) +
      fila('Doctrina naval', d.doc.toFixed(2), x2(0.8 + 0.5 * d.doc)) +
      fila('Cohesión del Estado (asabiya)', d.moral.asabiya.toFixed(2), x2(d.moral.cohesion)) +
      fila('Distancia a su base', nd(Math.round(d.moral.dist / 55)) + ' de viaje', x2(1 - 0.35 * d.moral.lejos)) +
      fila('Pagas', d.moral.msc ? d.moral.msc + ' meses sin cobrar' : 'al día', x2(d.moral.paga)) +
      ((d.moral.mando || 1) !== 1 ? fila('Mando', 'un almirante que nunca ha perdido', x2(d.moral.mando)) : '') +
      '<tr><td><b>Pegada</b></td><td>naves del rival hundidas al día por cada 100 propias</td><td class="n"><b>' + (ef * 100).toFixed(1) + '</b></td></tr></table>';
  }

  // ── Un bando de una batalla espacial.
  function bandoFlota(m, b, k, cols, ab) {
    const l = b.L[k], o = b.L[1 - k]; const viva = b.vivo; const e = m.est[l.est]; const col = cols[k]; const p = l.papel || {}; const papel = Lo.papel(b, k);
    const ahora = viva ? Math.round(l.n) : (l.nFin === undefined ? Math.round(l.n) : l.nFin); const perd = Math.max(0, l.n0 - ahora);
    const rot = papel === 'invade' ? 'Invasor' : papel === 'ataca' ? 'Ataca' : p.casa ? 'Defiende su territorio' : 'Defiende la posición';
    const que = p.casa ? (p.st === 'base' ? 'El sistema es suyo y aquí tiene su base.' : p.llega ? 'El sistema es suyo: ha venido a socorrerlo.' : 'El sistema es suyo.')
      : p.st === 'sitio' ? 'Estaba asaltando un asentamiento cuando llegó el enemigo.' : p.mis === 'atacar' ? 'Viene con orden de tomar el sistema.' : p.llega ? 'Llegó al sistema y se encontró al enemigo.' : 'Ya estaba aquí cuando llegó el enemigo.';
    const res = viva ? '' : b.gano === k ? '<span class="res gana">🏆 vence</span>' : b.huye === k ? '<span class="res">🏳 se retira</span>' : '<span class="res pierde">☠ aniquilado</span>';
    let h = '<div class="bando" style="border-left-color:' + col + '"><div class="rol" style="color:' + col + '">' + rot + res + '</div>';
    h += '<div class="quien">' + (e ? ban('E' + e.id, 36, 24) : '') + '<div><b>' + (e ? L('E', e.id) : 'Sin Estado') + '</b><br><span class="nota">' + que + '</span></div></div>';
    const alm = l.alm >= 0 ? m.per[l.alm] : null;
    h += '<div class="quien">' + (alm ? av('P' + alm.id, 36) : '') + '<div>' + l.flo.map(i => esc(m.flo[i].nom)).join(' y ') + '<br><span class="nota">' + (alm ? 'manda ' + L('P', alm.id) + (alm.vivo ? '' : ' †') + ' · ' + (alm.r[S.R.VAL] >= 0.8 ? 'de los que no se retiran' : 'se retira si lo ve perdido') : 'sin almirante') + '</span></div></div>';
    h += vida(ahora / l.n0, col) + '<b>' + n0(ahora) + '</b> de ' + n0(l.n0) + ' naves en pie' + (perd ? ' · ha perdido ' + n0(perd) + ' (' + pc(perd / l.n0) + ') y unos ' + n0(l.muertos) + ' tripulantes' : viva ? ' · sin bajas todavía' : ' · sin bajas');
    // De qué se compone: por clase de nave, con lo que entró y lo que queda.
    const por = {};
    l.nav0.forEach((id, i) => { const n = m.nav[id]; if (!n) return; const c0 = l.c0 ? l.c0[i] : n.cascos; const c = viva ? (n.vivo ? n.cascos : 0) : (l.cFin ? l.cFin[i] : 0); const x = por[n.cls] = por[n.cls] || { esc: 0, rotas: 0, c0: 0, c: 0, dis: n.dis }; x.esc++; if (c <= 0) x.rotas++; x.c0 += c0; x.c += c; });
    h += '<div class="comp">' + Object.keys(por).map(cls => { const x = por[cls]; const d = x.dis !== undefined ? m.dis[x.dis] : null; return '<div>' + mini(cls, col) + ' <b>' + n0(x.c) + '</b> de ' + n0(x.c0) + ' ' + nomCls(cls) + ' <span class="nota">en ' + x.esc + (x.esc === 1 ? ' escuadra' : ' escuadras') + (x.rotas ? ' (' + x.rotas + (x.rotas === 1 ? ' deshecha' : ' deshechas') + ')' : '') + (d ? ' · ' + L('D', d.id, d.nom) : '') + '</span></div>'; }).join('') + '</div>';
    const f0 = m.flo[l.flo[0]]; const ins = viva && f0 && f0.insignia !== undefined && m.nav[f0.insignia] && m.nav[f0.insignia].vivo ? f0.insignia : -1;
    if (ins >= 0) h += '<div class="nota">✦ Buque insignia: ' + L('N', ins) + '</div>';
    const des = viva ? (Co.desglose(m, l, S.tec.perfil(m, o.nav), b.sis) || l.des0) : l.des0;
    h += '<div class="pegada">Pegada: cada 100 de sus naves hunden <b>' + (l.ef * 100).toFixed(1) + '</b> del rival al día.</div>';
    if (des) h += '<div class="razon">' + razones(des) + (viva ? '' : ' <span class="nota">(tal como entró en combate)</span>') + '</div>' + plegable('c' + b.id + '_' + k, ab, 'Ver la cuenta completa', () => cuenta(des, l.ef));
    return h + '</div>';
  }

  P.tarjetaBatalla = function (m, b, ab) {
    const cols = Lo.colBatalla(m, b); const s = m.sis[b.sis]; const viva = b.vivo; const dias = Math.max(1, Math.round((viva ? m.t : b.t1) - b.t0));
    const nom = [0, 1].map(k => color(cols[k], esc(nomEst(m, b.L[k].est))));
    let h = '<div class="tarjeta batalla"><div class="tit">⚔ Batalla espacial de ' + L('S', b.sis) + '<span class="nota"> · ' + (viva ? 'día ' + dias + ', en curso' : m.fecha(b.t0) + ' · duró ' + nd(dias)) + '</span></div>';
    h += '<div class="filtro">' + ir('S' + b.sis, 7, '🔍 Verla en el mapa') + (b.ev >= 0 ? '<a class="ref" data-ev="' + b.ev + '">📜 por qué pelean</a>' : '') + '</div>';
    h += '<p class="nota">' + (b.ley === 'lineal' ? 'Campo de asteroides: no todos pueden disparar a todos, así que el número pesa menos y la calidad de cada nave, más.' : 'Espacio abierto: todos disparan a todos, así que el número pesa al cuadrado (el doble de naves vale por cuatro).');
    if (viva) { const inv = [0, 1].find(k => Lo.papel(b, k) === 'invade'); h += ' <b>Qué se juegan:</b> ' + (inv !== undefined ? 'si gana ' + nom[inv] + ', podrá desembarcar tropas en ' + (s.ase.length ? s.ase.slice(0, 3).map(id => L('A', id)).join(', ') : 'el sistema') + '; si gana ' + nom[1 - inv] + ', la invasión se acaba aquí.' : 'quien pierda vuelve a su base, y la victoria pesa en la guerra.'); }
    h += '</p>' + bandoFlota(m, b, 0, cols, ab) + '<div class="contra">contra</div>' + bandoFlota(m, b, 1, cols, ab);
    if (viva) {
      const fz = Co.fuerza(b); const pr = Co.pronostico(m, b);
      h += '<div class="veredicto"><div class="nota">Quién va ganando (naves × pegada): ' + nom[0] + ' ' + pc(fz) + ' · ' + nom[1] + ' ' + pc(1 - fz) + '</div>' + balanza(fz, cols, b.ley === 'lineal' ? [] : [0.355, 0.645]);
      if (pr.gana < 0) h += 'Muy igualados: a este ritmo nadie se impone en un año.';
      else { const G = pr.gana, Pd = 1 - G; h += 'A este ritmo gana ' + nom[G] + ' en unos ' + nd(pr.dias) + ': ' + (pr.huye >= 0 ? nom[Pd] + ' se retirará con unas ' + n0(pr.quedan[Pd]) + ' naves' : 'de ' + nom[Pd] + ' no quedará nada') + ', y al vencedor le quedarán unas ' + n0(pr.quedan[G]) + '.'; }
      if (b.ley !== 'lineal') h += ' <span class="nota">Las rayas blancas marcan dónde un almirante da la batalla por perdida y se retira.</span>';
      h += '</div>';
    } else if (b.gano >= 0) {
      const G = b.gano, Pd = 1 - G; const lg = b.L[G], lp = b.L[Pd];
      h += '<div class="veredicto">Ganó ' + nom[G] + ' con ' + n0(lg.nFin) + ' de ' + n0(lg.n0) + ' naves en pie. ' + nom[Pd] + (b.huye === Pd ? ' se retiró con ' + n0(lp.nFin) + ' de ' + n0(lp.n0) : ' perdió sus ' + n0(lp.n0)) + '. Murieron unos ' + n0(lg.muertos + lp.muertos) + ' tripulantes.</div>';
    }
    h += '<canvas class="graf" data-bat="' + b.id + '" style="height:70px"></canvas><div class="leyenda">' + chip(cols[0]) + esc(nomEst(m, b.L[0].est)) + ' &nbsp; ' + chip(cols[1]) + esc(nomEst(m, b.L[1].est)) + ' <span class="nota">· naves en pie, día a día</span></div>';
    return h + '</div>';
  };

  // ── Un bando de un combate en tierra.
  function bandoTierra(m, t, k, cols) {
    const x = k ? t.def : t.atk; const col = cols[k]; const a = m.ase[t.a]; const viva = t.vivo; const asalto = t.tipo === 'asalto'; const d = x.des;
    const e = x.est >= 0 ? m.est[x.est] : null; const f = !k && x.fac >= 0 ? m.fac[x.fac] : null; const u = k && x.uni >= 0 ? m.uni[x.uni] : null;
    const corte = k ? 0.25 : 0.3;
    const rot = k ? (asalto ? 'Defensores' : 'Guarnición del régimen') : (asalto ? 'Asaltantes' : 'Insurrectos');
    const res = viva ? '' : t.gano === k ? '<span class="res gana">🏆 ' + (k && !asalto ? 'vence' : 'vencen') + '</span>' : '<span class="res pierde">' + (k ? '🏳 se rinden' : '✖ fracasan') + '</span>';
    let quien, sub, cara = '';
    if (k) { quien = u ? L('U', u.id, 'Guarnición de ' + a.nom) : esc(x.nom); sub = (e ? 'soldados de ' + L('E', e.id) : 'sin Estado') + (u && u.cmd >= 0 ? ' · manda ' + L('P', u.cmd) : ''); if (u && u.cmd >= 0) cara = av('P' + u.cmd, 36); }
    else if (asalto) { const fl = t.flo >= 0 ? m.flo[t.flo] : null; quien = 'Tropa desembarcada por la ' + esc(x.nom); sub = (e ? 'soldados de ' + L('E', e.id) : '') + (fl && fl.alm >= 0 ? ' · manda ' + L('P', fl.alm) : ''); if (fl && fl.alm >= 0) cara = av('P' + fl.alm, 36); }
    else { quien = f ? L('F', f.id, f.nom || x.nom) : esc(x.nom); sub = d && d.mil ? n0(d.mil) + ' milicianos armados y ' + n0(d.vecinos) + ' vecinos que se les suman' : 'milicianos y vecinos que se les suman'; if (f && f.lid >= 0) { sub += ' · encabeza ' + L('P', f.lid); cara = av('P' + f.lid, 36); } }
    let h = '<div class="bando" style="border-left-color:' + col + '"><div class="rol" style="color:' + col + '">' + rot + res + '</div>';
    h += '<div class="quien">' + (e ? ban('E' + e.id, 36, 24) : f ? ban('F' + f.id, 36, 24) : '') + cara + '<div><b>' + quien + '</b><br><span class="nota">' + sub + '</span></div></div>';
    const perd = Math.max(0, x.n0 - x.n);
    h += vida(x.n / x.n0, col, corte) + '<b>' + n0(x.n) + '</b> de ' + n0(x.n0) + ' en pie' + (perd >= 1 ? ' · ' + n0(perd) + ' bajas (' + pc(perd / x.n0) + ')' : '') + '<br><span class="nota">La raya blanca: por debajo de ' + n0(x.n0 * corte) + ' ' + (k ? 'se rinden' : 'se retiran') + '.</span>';
    h += '<div class="pegada">Pegada: cada 100 causan <b>' + (x.ef * 100).toFixed(1) + '</b> bajas al día.</div>';
    let r = '';
    if (d) {
      if (k) r = 'Doctrina ' + d.doc.toFixed(2) + ' · equipo ' + d.arm.toFixed(2) + (d.arm < 0.5 ? ' (mal armados)' : '') + ' · ' + (d.msc ? d.msc + ' meses sin cobrar' : 'pagas al día') + ' · pelean en casa (' + pm(d.casa) + ')';
      else if (asalto) r = 'Doctrina en tierra ' + d.doc.toFixed(2) + ' · moral ' + x2(d.moral.total) + (d.moral.lejos > 0.05 ? ' (lejos de su base, ' + pm(1 - 0.35 * d.moral.lejos) + ')' : '') + (d.moral.msc ? ' · ' + d.moral.msc + ' meses sin cobrar' : '') + ' · tropa de asalto (+30 %)';
      else r = 'Saben pelear ' + d.doc.toFixed(2) + ' sobre 1 · hay armas para ' + pc(d.armados) + ' de la milicia';
    }
    if (r) h += '<div class="razon">' + r + '.</div>';
    return h + '</div>';
  }

  P.tarjetaTierra = function (m, t, ab) {
    const cols = Lo.colTierra(m, t); const a = m.ase[t.a]; const viva = t.vivo; const asalto = t.tipo === 'asalto'; const dias = Math.max(1, Math.round((viva ? m.t : t.t1) - t.t0));
    const N = [asalto ? 'los asaltantes' : 'los insurrectos', asalto ? 'los defensores' : 'la guarnición'].map((s, k) => color(cols[k], s));
    const gana = (k) => (k === 1 && !asalto ? 'gana ' : 'ganan ') + N[k];
    let h = '<div class="tarjeta batalla"><div class="tit">' + (asalto ? '⚔ Asalto a ' : '🔥 Insurrección en ') + L('A', t.a) + '<span class="nota"> · ' + (viva ? 'día ' + dias + ' de 45 como mucho' : m.fecha(t.t0) + ' · duró ' + nd(dias)) + '</span></div>';
    h += '<div class="filtro">' + ir('A' + t.a, 13, '🔍 Verlo en el mapa') + (t.ev >= 0 ? '<a class="ref" data-ev="' + t.ev + '">📜 cómo empezó</a>' : '') + '</div>';
    h += '<p class="nota">Calle a calle no todos pueden disparar a todos: cuenta más lo bien que pelea cada uno que el número.';
    if (viva) h += ' <b>Qué se juegan:</b> ' + (asalto ? 'si ' + gana(0) + ', ' + esc(a.nom) + ' pasa a ' + esc(nomEst(m, t.atk.est)) + '; si ' + N[1] + ' aguantan 45 días o los desangran, el asalto fracasa.' : 'si ' + gana(0) + ', hay revolución en ' + esc(a.nom) + '; si ' + gana(1) + ', vendrá el terror.');
    h += '</p>' + bandoTierra(m, t, 0, cols) + '<div class="contra">contra</div>' + bandoTierra(m, t, 1, cols);
    if (viva) {
      const fa = t.atk.ef * t.atk.n, fd = t.def.ef * t.def.n; const fz = fa + fd > 0 ? fa / (fa + fd) : 0.5; const pr = Co.pronosticoTierra(m, t);
      h += '<div class="veredicto"><div class="nota">Quién va ganando (hombres × pegada): ' + N[0] + ' ' + pc(fz) + ' · ' + N[1] + ' ' + pc(1 - fz) + '</div>' + balanza(fz, cols) + 'A este ritmo ' + gana(pr.gana) + ' en unos ' + nd(pr.dias) + (pr.plazo ? ': se acaba el plazo sin que nadie ceda' : '') + '. <span class="nota">Ya han muerto ' + n0(t.civiles) + ' vecinos entre dos fuegos.</span></div>';
    } else if (t.gano >= 0) {
      h += '<div class="veredicto">' + S.cap(gana(t.gano)) + ' en ' + nd(dias) + '. ' + n0(t.muertos || 0) + ' muertos, ' + n0(t.civiles) + ' de ellos vecinos.</div>';
    }
    h += '<canvas class="graf" data-tie="' + t.id + '" style="height:70px"></canvas><div class="leyenda">' + chip(cols[0]) + (asalto ? 'Asaltantes' : 'Insurrectos') + ' &nbsp; ' + chip(cols[1]) + (asalto ? 'Defensores' : 'Guarnición') + ' <span class="nota">· hombres en pie, día a día</span></div>';
    return h + '</div>';
  };

  // ── La pestaña: lo que arde ahora, entero; lo pasado, plegado en una línea que dice quién ganó a quién.
  const bandera = (m, id) => (id >= 0 && m.est[id] ? ban('E' + id, 22, 15) + ' ' : '');
  const resumenBat = (m, b) => { const G = b.gano >= 0 ? b.gano : 0, Pd = 1 - G; return '🏆 ' + bandera(m, b.L[G].est) + esc(nomEst(m, b.L[G].est)) + ' vence a ' + bandera(m, b.L[Pd].est) + esc(nomEst(m, b.L[Pd].est)) + ' en ' + esc(m.sis[b.sis].nom) + ' <span class="nota">· ' + m.fecha(b.t0) + ' · ' + n0(b.L[G].n0) + ' naves contra ' + n0(b.L[Pd].n0) + '</span>'; };
  const resumenTie = (m, t) => { const asalto = t.tipo === 'asalto'; const a = m.ase[t.a]; const quien = t.gano === 0 ? (asalto ? esc(nomEst(m, t.atk.est)) + ' toma ' + esc(a.nom) : 'La insurrección vence en ' + esc(a.nom)) : (asalto ? esc(a.nom) + ' rechaza el asalto de ' + esc(nomEst(m, t.atk.est)) : 'La guarnición aplasta la insurrección de ' + esc(a.nom)); return (t.gano === 0 ? '🏆 ' : '🛡 ') + quien + ' <span class="nota">· ' + m.fecha(t.t0) + ' · ' + n0(t.atk.n0) + ' contra ' + n0(t.def.n0) + ' · ' + n0(t.muertos || 0) + ' muertos</span>'; };
  P.batallas = function (mm, op) {
    P.usar(mm); const m = mm; const ab = (op && op.abiertos) || AB();
    let h = '<h2>Batallas</h2><p class="nota">Cada tarjeta es un combate. Cada bando va en su bloque y con su color: el mismo que llevan sus naves en el mapa y su línea en la gráfica. La barra partida dice quién va ganando.</p>';
    const viv = m.bat.filter(b => b.vivo), tv = (m.tie || []).filter(t => t.vivo);
    h += '<h3>Ahora mismo (' + (viv.length + tv.length) + ')</h3>';
    for (const b of viv) h += P.tarjetaBatalla(m, b, ab);
    for (const t of tv) h += P.tarjetaTierra(m, t, ab);
    if (!viv.length && !tv.length) h += '<p class="nota">Ninguna. Puedes provocar una con la mano (⚔ Provocar una guerra) y esperar a que lleguen las órdenes a las flotas.</p>';
    const pas = m.bat.filter(b => !b.vivo).slice(-14).reverse();
    if (pas.length) { h += '<h3>Las últimas en el espacio</h3><p class="nota">Pulsa una para ver a cada bando.</p>'; for (const b of pas) h += plegable('b' + b.id, ab, resumenBat(m, b), () => P.tarjetaBatalla(m, b, ab)); }
    const pt = (m.tie || []).filter(t => !t.vivo).slice(-10).reverse();
    if (pt.length) { h += '<h3>Las últimas en tierra</h3>'; for (const t of pt) h += plegable('t' + t.id, ab, resumenTie(m, t), () => P.tarjetaTierra(m, t, ab)); }
    return h;
  };

  // En la ficha del sistema y en la del asentamiento, el combate que haya en curso sale arriba con la misma tarjeta.
  P.extraSis = function (s) { const m = P._m; let h = ''; for (const b of m.bat) if (b.vivo && b.sis === s.id) h += P.tarjetaBatalla(m, b, AB()); return h; };
  const extraAsent0 = P.extraAsent;
  P.extraAsent = function (a) { const m = P._m; const t = a.tie >= 0 && m.tie && m.tie[a.tie] && m.tie[a.tie].vivo ? m.tie[a.tie] : null; return (t ? P.tarjetaTierra(m, t, AB()) : '') + extraAsent0(a); };
})(typeof globalThis !== 'undefined' ? globalThis : this);
