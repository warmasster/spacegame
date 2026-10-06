// Paneles: fichas de cualquier cosa del mundo, crónica, historiador (grafo de causas), sismógrafo y listas.
// Todo se lee del núcleo; aquí no hay reglas.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI = g.UI || {}; const B = S.B;
  const P = U.paneles = {};
  const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;');
  const pc = (x) => Math.round(x * 100) + ' %';
  const n0 = (x) => Math.round(x).toLocaleString('es-ES');
  let m = null;
  const L = (t, id, txt) => id === undefined || id === null || id < 0 ? '—' : '<a class="ref" data-r="' + t + id + '">' + esc(txt || m.nom(t, id)) + '</a>';
  const tok = (s) => esc(s).replace(/\{([ASPNEFOI])(\d+)\}/g, (_, t, id) => L(t, +id));
  const txtEv = (e) => tok(e.txt);
  const barra = (v, cls) => '<span class="barra ' + (cls || '') + '"><i style="width:' + Math.round(S.clamp(v, 0, 1) * 100) + '%"></i></span>';
  const evHtml = (e, n) => '<div class="ev i' + e.imp + (e.k === 'mano' ? ' mano' : '') + '" data-ev="' + e.id + '"' + (n !== undefined ? ' style="--n:' + n + '"' : '') + '><span class="f">' + m.fecha(e.t) + ' · ' + e.k.replace(/_/g, ' ') + '</span>' + txtEv(e) + '</div>';
  const kv = (rows) => '<div class="kv">' + rows.filter(r => r).map(r => '<span>' + r[0] + '</span><span>' + r[1] + '</span>').join('') + '</div>';
  const boton = (id, ref, txt) => { const d = S.reg.dios[id]; const ok = S.dios.puede(m, id, ref); return '<button data-dios="' + id + '" data-id="' + ref + '" title="' + esc(d.desc).replace(/"/g, '&quot;') + '"' + (ok ? '' : ' disabled') + '>' + d.ico + ' ' + (txt || d.nom) + '</button>'; };
  // Todas las herramientas del registro que se aplican a este tipo de cosa, más algunas de lo que tiene dentro.
  const mano = (tipo, id, extra) => { let h = '', k = 0; for (const x in S.reg.dios) if (S.reg.dios[x].obj === tipo) { h += boton(x, id); k++; } if (extra) for (const e of extra) if (e && e[1] >= 0) { h += boton(e[0], e[1], e[2]); k++; } return k ? '<details class="mano"><summary>🖐 La mano · ' + k + ' cosas que puedes hacerle</summary><div class="dios">' + h + '</div></details>' : ''; };
  const chip = (e) => '<span class="chip" style="background:' + (e ? e.col : '#59647a') + '"></span>';
  // Lienzos que se pintan después de insertar el HTML: retratos, banderas, planos de nave.
  const av = (ref, s) => '<canvas class="av" data-av="' + ref + '" data-w="' + s + '" data-h="' + s + '" style="width:' + s + 'px;height:' + s + 'px"></canvas>';
  const ban = (ref, w, h2) => '<canvas class="ban" data-ban="' + ref + '" data-w="' + w + '" data-h="' + h2 + '" style="width:' + w + 'px;height:' + h2 + 'px"></canvas>';
  P.usar = function (mm) { m = mm; P._m = mm; };
  const estadoDe = (id) => id >= 0 ? chip(m.est[id]) + L('E', id) : 'puerto franco';
  const dias = (d) => d < 1.5 ? 'hoy' : d < 60 ? Math.round(d) + ' días' : d < 720 ? Math.round(d / 30) + ' meses' : (d / 360).toFixed(1) + ' años';
  const ROL = { gobernante: 'gobernante', gobernador: 'gobernador', general: 'general', ministro: 'ministro', arzobispo: 'arzobispo', jefe_guardia: 'jefe de la Guardia', almirante: 'almirante', comisario: 'comisario', inspector: 'inspector de muelle', jefe_calidad: 'jefe de calidad', capitan: 'capitán mercante', pirata: 'capitán pirata', corsario: 'corsario', perista: 'perista', cazador: 'cazarrecompensas', piloto: 'piloto retirado', predicador: 'predicador', mercader: 'cabeza de casa mercante', lider: 'cabecilla', organizador: 'enlace', vengador: 'busca venganza', civil: 'vecino', preso: 'preso', exiliado: 'exiliado' };

  // op.tema: el tema abierto en la ficha del mundo; op.bio: el filtro de los historiales (importancia y orden).
  P.ficha = function (mm, ref, op) {
    m = mm; P._m = mm; P._op = op || null;
    if (!ref) return op && op.tema && P.tema ? P.tema(mm, op.tema) : '<h2>El mundo</h2><p class="nota">Pulsa cualquier cifra para ver su historia entera. Para la ficha de algo, haz clic en el mapa: un sistema, un asentamiento, una instalación, una nave o una cara. De lejos son números; de cerca, gente.</p>' + P.resumen();
    const f = { S: sistema, A: asent, I: inst, N: nave, P: persona, E: estado, F: faccion, O: objeto, C: cara }[ref.t] || (P.extra && P.extra[ref.t]);
    return f ? f(ref) + (P.historialPleg && 'SANEF'.indexOf(ref.t) >= 0 ? P.historialPleg(ref) : '') : '';
  };
  P.resumen = function () {
    const r = S.resumen(m); const V = U.viz; const C = V.SEM;
    const ult = (k) => { const s = m.series[k]; if (s) for (let i = s.length - 1; i >= 0; i--) if (s[i] === s[i]) return s[i]; return 0; };
    let gue = 0; for (const e of m.est) if (e.vivo) gue += e.gue.size; gue /= 2; let bat = 0; for (const b of m.bat) if (b.vivo) bat++; for (const t of (m.tie || [])) if (t.vivo) bat++;
    const partes = m.est.filter(e => e.vivo).sort((x, y) => (y.pob || 0) - (x.pob || 0)).map(e => ({ nom: e.nom, v: e.pob || 0, col: e.col, id: e.id }));
    let libre = r.pob; for (const p of partes) libre -= p.v; if (libre > r.pob * 0.01) partes.push({ nom: 'Puertos francos', v: libre, col: '#59647a', id: -1 });
    let h = '<h3>El mundo ahora</h3>' + V.kpis([
      V.kpi(S.fmt(r.pob), 'Población', { spark: 'pob', col: C.azul }),
      V.kpi(r.estados, 'Estados', { spark: 'estados', col: C.ambar }),
      V.kpi(gue, 'Guerras en curso', { spark: 'guerras', col: C.rojo, sub: bat ? bat + (bat === 1 ? ' combate ahora' : ' combates ahora') : '' }),
      V.kpi(pc(ult('hambre')), 'Hambre media', { spark: 'hambre', col: C.rojo }),
      V.kpi(pc(ult('calle')), 'En la calle (máximo)', { spark: 'calle', col: C.rosa }),
      V.kpi(r.naves, 'Naves', { spark: 'naves', col: C.verde, sub: r.piratas + ' piratas' }),
    ]);
    h += '<h3>Quién tiene a la gente</h3><div class="dona"><canvas data-dona="' + V.dato('pobMundo', { partes, centro: S.fmt(r.pob), sub: 'habitantes' }) + '" data-w="112" data-h="112" style="width:112px;height:112px"></canvas><div class="leyenda col">' + partes.slice(0, 8).map(p => '<span><span class="chip" style="background:' + p.col + '"></span>' + (p.id >= 0 ? L('E', p.id) : esc(p.nom)) + ' <b>' + pc(p.v / Math.max(1, r.pob)) + '</b></span>').join('') + '</div></div>';
    h += '<h3>Lo que ha pasado</h3><div class="ctas">' + [['⚔', r.guerras, 'guerras'], ['💥', r.batallas, 'batallas'], ['🚩', r.conquistas, 'conquistas'], ['🔥', r.revoluciones, 'revoluciones'], ['🩸', r.masacres, 'masacres'], ['🥣', r.hambrunas, 'hambrunas'], ['👑', r.sucesiones, 'sucesiones'], ['🗡', r.golpes, 'golpes'], ['✊', r.faccionesNacidas, 'facciones nacidas'], ['☢', r.explosiones, 'núcleos reventados'], ['🏴', r.abordajes, 'abordajes'], ['📓', r.venganzas, 'venganzas'], ['🔮', r.profecias, 'profecías cumplidas'], ['🕊', r.independencias, 'independencias']].map(x => V.cuenta(x[0], n0(x[1]), x[2])).join('') + '</div>';
    return h + '<p class="nota">' + n0(r.personas) + ' personas con ficha · ' + n0(r.hechos) + ' hechos registrados · ' + n0(r.eventos) + ' eventos simulados.</p>';
  };

  function sistema(ref) {
    const s = m.sis[ref.id]; let pob = 0; for (const id of s.ase) pob += m.ase[id].pob;
    const aqui = m.nav.filter(n => n.vivo && S.nav.sisDe(m, n) === s.id);
    return '<h2>Sistema ' + esc(s.nom) + '</h2><div class="sub">' + (s.terr === 'asteroides' ? 'campo de asteroides (aquí se pelea con la ley lineal)' : 'espacio abierto') + ' · ' + estadoDe(s.est) + (s.bloqueo > m.t ? ' · <b>cerrado por una tormenta (' + Math.round(s.bloqueo - m.t) + ' d)</b>' : '') + '</div>' + mano('S', s.id) + (P.extraSis ? P.extraSis(s) : '') +
      kv([['Población', n0(pob)], s.restos > 0 ? ['Campo de restos', n0(s.restos) + ' cascos a la deriva'] : null, ['Tráfico reciente', (s.traf || 0).toFixed(1)], ['Al acecho', s.pir.length ? s.pir.map(i => L('N', i)).join(', ') : 'nadie'], ['Rutas', s.vec.map(v => L('S', v.a) + ' (' + Math.round(v.d / 55) + ' d)').join(', ')]]) +
      '<h3>Asentamientos</h3>' + s.ase.map(id => { const a = m.ase[id]; return '<div>' + L('A', id) + ' · ' + S.reg.asent[a.tipo].nom.toLowerCase() + ' · ' + n0(a.pob) + '</div>'; }).join('') +
      '<h3>Naves aquí (' + aqui.length + ')</h3>' + aqui.slice(0, 30).map(n => L('N', n.id) + ' <span class="nota">' + S.reg.nave[n.cls].nom.toLowerCase() + '</span>').join(' · ') +
      (s.pecios.length ? '<h3>Pecios a la deriva</h3>' + s.pecios.map(i => L('N', i)).join(' · ') : '');
  }

  function asent(ref) {
    const a = m.ase[ref.id]; const e = a.est >= 0 ? m.est[a.est] : null; const u = a.uni >= 0 ? m.uni[a.uni] : null; const gob = S.pol.gobernadorDe(m, a);
    const c = a.culpa; const ct = c.reg + c.acap + c.nat + c.ext; const P_ = S.eco.nivelPrecios(m, a);
    const fabrica = S.dios.fabrica(m, a);
    let h = '<div class="cab">' + (gob ? av('P' + gob.id, 64) : '') + '<div><h2>' + esc(a.nom) + '</h2><div class="sub">' + S.reg.asent[a.tipo].nom + ' · ' + L('S', a.sis) + ' · ' + estadoDe(a.est) + (a.cap ? ' · <b>capital</b>' : '') + '</div><div class="filtro"><button data-accion="plano">🔍 Ver el plano</button></div></div></div>';
    h += mano('A', a.id, [fabrica ? ['bomba', fabrica.id, 'Bomba a ' + S.reg.inst[fabrica.tipo].nom.toLowerCase()] : null, gob ? ['matar_publico', gob.id, 'Matar a quien manda'] : null]);
    const loc = u ? u.ori.find(o => o.a === a.id) : null;
    h += kv([
      ['Población', n0(a.pob) + (a.refEnt ? ' <span class="nota">(+' + n0(a.refEnt) + ' refugiados)</span>' : '')],
      ['Gobierna', gob ? L('P', gob.id) : '<span class="nota">vacante</span>'],
      u ? ['Guarnición', L('U', u.id, n0(u.n) + ' soldados') + ' · ' + (u.msc ? '<b>' + u.msc + ' meses sin cobrar</b>' : 'al día') + ' · ' + pc(loc ? loc.f : 0) + ' de aquí' + (u.ultDif !== undefined ? '<br><span class="nota">última cuenta: ' + (u.ultDif > 0.4 ? 'desertar' : u.ultDif > 0.15 ? 'no disparar' : 'obedecer') + ' (' + u.ultDif.toFixed(2) + ')</span>' : '')] : null,
      ['Hambre', barra(a.H, 'r') + pc(a.H)],
      a.estacion ? ['Aire (filtros)', barra(a.Hf, 'r') + (a.Hf > 0.3 ? 'la gente tose' : 'bien')] : null,
      ['Agravio', barra(a.agr, 'a') + a.agr.toFixed(2)],
      e ? ['Represión percibida', barra(a.R) + a.R.toFixed(2) + ' <span class="nota">π<sub>G</sub> ' + a.piG.toFixed(2) + ' · π<sub>E</sub> ' + a.piE.toFixed(2) + ' · señal ' + a.senal.toFixed(2) + '</span>'] : null,
      ['En la calle', barra(a.f / 0.63, 'r') + pc(a.f)],
      e ? ['Retratos sin ojos', barra(a.ret, 'a') + pc(a.ret)] : null,
      e && !a.cap ? ['Control de la capital', barra(a.control, 'v') + pc(a.control) + ' <span class="nota">(órdenes: ' + Math.round(a.retraso) + ' d ida y vuelta)</span>'] : null,
      ['¿Quién tiene la culpa?', 'régimen ' + pc(c.reg / ct) + ' · acaparadores ' + pc(c.acap / ct) + ' · naturaleza ' + pc(c.nat / ct) + (c.ext > 0.05 ? ' · enemigo ' + pc(c.ext / ct) : '')],
      a.semNec > 0 ? ['Cosecha', 'en ' + Math.round(a.diasCosecha) + ' días · semilla ' + pc(a.sem / a.semNec) + (a.plaga > 0 ? ' · <b>plaga ' + pc(a.plaga) + '</b>' : '')] : null,
      a.huelga > m.t ? ['Huelga', 'quedan ' + Math.round(a.huelga - m.t) + ' días'] : null,
    ]);
    h += P.extraAsent(a);
    h += '<h3>Mercado' + (P_ > 1.02 ? ' (precios en ' + e.mon.nom + ', ×' + P_.toFixed(2) + ')' : '') + '</h3><table><tr><th>Bien</th><th class="n">Almacén</th><th class="n">Objetivo</th><th class="n">Precio</th><th></th></tr>';
    for (let k = 0; k < S.NB; k++) { const T = S.eco.objetivo(a, k); if (T <= 0 && a.alm[k] < 1) continue; const r = a.pr[k] / S.bien[k].pref; h += '<tr><td>' + S.bien[k].nom + '</td><td class="n">' + S.fmt(a.alm[k]) + '</td><td class="n">' + (T > 0 ? S.fmt(T) : '—') + '</td><td class="n">' + Math.round(a.pr[k] * P_) + '</td><td>' + (T > 0 ? barra(r / 6, r > 2 ? 'r' : r > 1.2 ? 'a' : 'v') : '') + '</td></tr>'; }
    h += '</table>' + kv([['Núcleos de reactor', a.nuc.length + ' en almacén (' + n0(S.eco.precioNucleo(a)) + ' c/u)' + (a.nuc.length ? ' · ' + a.nuc.slice(0, 4).map(i => L('O', i, m.obj[i].serie)).join(', ') : '')], a.res[B.grano] > 1 ? ['Reserva del Estado', S.fmt(a.res[B.grano]) + ' t de grano'] : null, a.acap[B.grano] > 1 ? ['Almacenes privados', S.fmt(a.acap[B.grano]) + ' t guardadas por ' + L('F', a.acapFac)] : null]);
    h += '<h3>Instalaciones</h3><table><tr><th></th><th>Nivel</th><th>Salud</th><th class="n">Plantilla</th></tr>';
    for (const id of a.ins) { const i = m.ins[id]; const d = S.reg.inst[i.tipo]; const co = i.coh >= 0 ? m.coh[i.coh] : null; h += '<tr><td>' + d.ico + ' ' + L('I', id, d.nom === 'Memorial' ? i.nom : d.nom) + (i.huelga ? ' ✊' : '') + '</td><td>' + (d.trab ? i.nivel.toFixed(1) : '') + '</td><td>' + (d.indestructible ? '' : barra(i.salud, i.salud > 0.5 ? 'v' : 'r')) + '</td><td class="n">' + (co ? n0(co.n) : '') + '</td></tr>'; }
    h += '</table>';
    const facs = m.fac.filter(f => f.vivo && f.tipo !== 'casa' && (f.sede === a.id || f.cel.some(x => x.ase === a.id)));
    if (facs.length) h += '<h3>Facciones</h3>' + facs.map(f => '<div>' + L('F', f.id, f.nom || 'gente que se reúne (sin nombre aún)') + ' <span class="nota">' + f.tipo + ' · organización ' + f.O.toFixed(2) + '</span></div>').join('');
    if (a.rec && a.rec.length) {
      h += '<h3>Lo que aquí se sabe</h3>';
      for (const pid of a.rec.slice(-7).reverse()) { const p = m.paq[pid]; const v = a.not.get(pid); if (!v) continue; const e2 = m.ev[p.ev]; const inflado = p.x > 0 && Math.round(v.x) !== Math.round(p.x); h += '<div class="ev i1" data-ev="' + p.ev + '"><span class="f">ocurrió hace ' + dias(m.t - p.t) + ' en ' + esc(m.ase[p.a].nom) + ' · llegó tras ' + v.n + ' bocas' + (v.f > 1 ? ' · ' + v.f + ' fuentes' : '') + '</span>' + txtEv(e2) + (inflado ? ' <b>Aquí dicen que fueron ' + Math.round(v.x) + '</b> (fueron ' + Math.round(p.x) + ').' : '') + (v.q > 0 ? ' Lo atribuyen a ' + S.inf.QUIEN[v.q] + '.' : '') + '</div>'; }
    }
    const naves = m.nav.filter(n => n.vivo && n.en === a.id);
    h += '<h3>Muelle (' + naves.length + ')</h3>' + (naves.slice(0, 24).map(n => L('N', n.id)).join(' · ') || '<span class="nota">vacío</span>');
    if (a.buzon.length) h += '<div class="nota">Buzón: ' + a.buzon.length + ' cartas esperando una nave (' + a.buzon.slice(0, 4).map(x => (S.reg.carta[x.k] ? S.reg.carta[x.k].nom : x.k) + ' → ' + esc(m.ase[x.para].nom)).join('; ') + ')</div>';
    if (a.hechos.length) h += '<h3>Lo que este lugar recuerda</h3>' + a.hechos.slice().sort((x, y) => y.s - x.s).slice(0, 5).map(x => evHtml(m.ev[x.ev])).join('');
    // De cerca, carne: 20 personas de aquí. Siempre las mismas, y cuadran con los números.
    const caras = S.per.caras(m, a, 20); const od = caras.filter(x => x.odia).length; let tot = 0, odT = 0; for (const id of a.coh) { const co = m.coh[id]; tot += co.n; odT += co.n * (1 - S.cdfN((0.5 - co.agr.reg) / co.disp)); }
    h += '<h3>Veinte caras</h3><div class="nota">' + od + ' de ' + caras.length + ' odian al régimen (en los números: ' + pc(tot ? odT / tot : 0) + ').</div><div class="caras2">' + caras.map(x => av(x.ficha >= 0 ? 'P' + x.ficha : 'C' + x.i + '_' + x.coh + '_' + a.id, 26) + '<a class="ref' + (x.odia ? ' odia' : '') + '" data-r="' + (x.ficha >= 0 ? 'P' + x.ficha : 'C' + x.i + '_' + x.coh + '_' + a.id) + '">' + esc(x.nom) + (x.ficha >= 0 ? ' ★' : '') + '</a>').join('') + '</div>';
    return h;
  }

  function nucleoHtml(o) {
    if (!o) return '<span class="nota">sin núcleo (generadores auxiliares)</span>';
    const a = S.obj.grieta(o, m.t);
    return L('O', o.id, (o.marca || 'núcleo') + ' ' + o.serie) + (o.serie !== o.interno ? ' <span class="nota">(anillo interno: ' + (o.borrado ? 'limado' : esc(o.interno)) + ')</span>' : '') + '<br>' + barra(a / 4, a >= 1 ? 'r' : 'v') + 'grieta ' + a.toFixed(a < 0.1 ? 3 : 2) + ' mm' + (o.vivo && isFinite(o.tF) ? ' · fallará en ' + dias(o.tF - m.t) : '') + (o.robado ? ' · <b>robado</b>' : '') + (o.defecto ? ' · lote sin inspección' : '');
  }
  function inst(ref) {
    const i = m.ins[ref.id]; const d = S.reg.inst[i.tipo]; const a = m.ase[i.ase]; const co = i.coh >= 0 ? m.coh[i.coh] : null;
    let h = '<h2>' + d.ico + ' ' + esc(i.nom) + '</h2><div class="sub">' + d.nom + ' · ' + L('A', a.id) + '</div>' + mano('I', i.id);
    const io = (o) => o ? Object.keys(o).map(k => (o[k] * i.nivel).toFixed(0) + ' t de ' + S.bien[B[k]].nom.toLowerCase()).join(' + ') : '';
    h += kv([
      d.trab ? ['Nivel', i.nivel.toFixed(1)] : null,
      d.indestructible ? null : ['Salud', barra(i.salud, i.salud > 0.5 ? 'v' : 'r') + pc(i.salud)],
      d.sal || d.nucleos || d.astillero ? ['Rendimiento', pc(i.ef || 0) + (i.huelga ? ' · <b>en huelga</b>' : '')] : null,
      d.ent ? ['Consume al día', io(d.ent)] : null, d.sal ? ['Produce al día', io(d.sal)] : null,
      d.cosecha ? ['Cosecha al año', S.fmt(d.cosecha * i.nivel) + ' t'] : null,
      co ? ['Plantilla', n0(co.n) + ' de ' + n0(d.trab * i.nivel)] : null,
      co ? ['Agravio de la plantilla', 'régimen ' + co.agr.reg.toFixed(2) + ' · patrón ' + co.agr.pat.toFixed(2) + (co.agr.crim > 0.05 ? ' · criminales ' + co.agr.crim.toFixed(2) : '') + (co.fac >= 0 ? '<br>organizados en ' + L('F', co.fac, m.fac[co.fac].nom || 'una protofacción') : '')] : null,
      d.nuc ? ['Núcleo de reactor', nucleoHtml(i.nuc >= 0 ? m.obj[i.nuc] : null)] : null,
      d.astillero ? ['Oficio del astillero', Math.round(i.exp || 0) + ' cascos botados <span class="nota">(con oficio se construye antes y mejor)</span>'] : null,
      d.calidad ? ['Jefe de calidad', (i.jefe >= 0 ? av('P' + i.jefe, 26) + ' ' : '') + L('P', i.jefe)] : null,
      d.nucleos ? ['Lote en curso', i.lote + ' (' + i.enLote + '/12)' + (i.loteDef ? ' · <b>sale sin inspección</b>' : '')] : null,
      i.muertos ? ['Nombres grabados', i.muertos] : null,
    ]);
    if (d.astillero) h += '<h3>Gradas</h3>' + (a.pedidos.length ? a.pedidos.map(p => '<div>' + S.reg.nave[p.cls].nom + ' ' + barra(p.prog, 'v') + pc(Math.min(1, p.prog)) + (p.espera ? ' · <b>esperando un núcleo</b>' : '') + '</div>').join('') : '<span class="nota">sin encargos</span>');
    if (i.tipo === 'memorial' && i.ev !== undefined) { const r = new S.Rng(S.hash(m.semilla, 'memorial', i.ev)); const ns = []; for (let k = 0; k < Math.min(i.muertos, 40); k++) ns.push(esc(S.per.nombre(r, r.p(0.5) ? 'M' : 'H'))); h += '<h3>Los nombres</h3><div class="nota">' + ns.join(' · ') + (i.muertos > 40 ? ' … y ' + (i.muertos - 40) + ' más' : '') + '</div>' + evHtml(m.ev[i.ev]); }
    if (i.evDano !== undefined && i.evDano >= 0) h += '<h3>Último daño</h3>' + evHtml(m.ev[i.evDano]);
    return h;
  }

  function nave(ref) {
    const n = m.nav[ref.id]; const d = S.reg.nave[n.cls]; const cas = S.CASCOS[n.col];
    const due = n.dueno.t === 'E' ? estadoDe(n.dueno.id) : n.dueno.t === 'F' ? L('F', n.dueno.id) : n.dueno.t === 'A' ? L('A', n.dueno.id) : (n.dueno.id >= 0 ? L('P', n.dueno.id) : 'nadie');
    let st;
    if (!n.vivo) st = n.st === 'pecio' ? 'pecio a la deriva en ' + L('S', n.sisPecio) : 'desguazada';
    else if (n.en >= 0) st = (n.st === 'amarrada' ? 'amarrada (sin negocio) en ' : 'atracada en ') + L('A', n.en);
    else if (n.st === 'viaje' && n.ruta) st = 'en viaje' + (n.ruta.de >= 0 ? ' desde ' + L('A', n.ruta.de) : '') + ' hacia ' + L(n.ruta.dest.t, n.ruta.dest.id) + ' · llega en ' + dias(n.ruta.t1 - m.t) + (n.conv >= 0 ? ' · en convoy' : '');
    else st = ({ acecho: 'al acecho en ', guardia: 'de guardia en ', batalla: 'en combate en ', rescate: 'desguazando pecios en ', huida: 'huyendo de ' })[n.st] + L('S', n.sisEn >= 0 ? n.sisEn : 0);
    let h = '<h2>' + esc(n.nom) + '</h2><div class="sub">' + d.nom + (n.cascos > 1 ? ' · <b>' + n.cascos + ' naves</b>' : '') + (n.pirata ? ' <b>pirata</b>' : '') + ' · casco ' + cas.nom + ' · de ' + due + '</div>' + mano('N', n.id, [n.cap >= 0 ? ['matar_publico', n.cap, 'Matar al capitán'] : null]);
    h += '<canvas class="plano" data-nave="' + n.id + '" data-w="388" data-h="132" style="width:100%;height:132px"></canvas>' + (n.vivo ? '<div class="filtro"><button data-accion="seguir">👁 Seguir por el mapa</button></div>' : '') + P.extraNave(n);
    const carga = [n.carga].concat(n.extra || []).filter(x => x).map(x => S.fmt(x.q) + ' t de ' + S.bien[x.c].nom.toLowerCase() + (x.robado ? ' (robado)' : '') + (x.ori >= 0 ? ' <span class="nota">de ' + esc(m.ase[x.ori].nom) + '</span>' : ''));
    if (n.nucs.length) carga.push(n.nucs.length + ' núcleos: ' + n.nucs.map(i => L('O', i, m.obj[i].serie)).join(', '));
    h += kv([
      ['Estado', st], ['Capitán', n.cap >= 0 ? av('P' + n.cap, 26) + ' ' + L('P', n.cap) : '—'], n.flo >= 0 ? ['Flota', esc(m.flo[n.flo].nom) + (n.casaTrip !== undefined ? ' · tripulación nacida en ' + L('A', n.casaTrip) : '')] : null,
      n.patente >= 0 ? ['Patente de corso', estadoDe(n.patente)] : null,
      ['Tripulación', n0(n.trip * n.cascos) + (n.cascos > 1 && d.tropas ? ' · ' + n0(d.tropas * n.cascos) + ' soldados embarcados' : '')], n.dueno.t !== 'E' ? ['Caja', n0(n.dinero) + (n.impago ? ' · <b>' + n.impago + ' pagas atrasadas</b>' : '')] : null,
      ['Combustible', barra(S.nav.tanque(n) > 0 ? n.fuel / S.nav.tanque(n) : 0) + S.fmt(n.fuel) + ' t'], ['Daños', barra(n.dan, 'r') + pc(n.dan)],
      ['Bodega', carga.join('<br>') || 'vacía'], n.pas > 0 ? ['Refugiados a bordo', n.pas + ' de ' + L('A', n.pasDe)] : null,
      n.presos ? ['Presos', n.presos.length ? n.presos.map(i => L('P', i)).join(', ') : 'ninguno'] : null,
      ['Núcleo', nucleoHtml(n.nuc >= 0 ? m.obj[n.nuc] : null)],
      ['Huella del motor', '<span class="nota">' + m.obj[n.motor].firma.map(x => x.toFixed(1)).join(' ') + '</span>'],
      n.pin.length > 1 ? ['Bajo la pintura', n.pin.slice().reverse().map(i => chip(m.est[i]) + esc(m.est[i].nom)).join(' ← ')] : null,
      n.cic.length ? ['Cicatrices', n.cic.slice(-4).map(c => '<a class="ref" data-ev="' + c.ev + '">' + c.lado + ' (' + m.fecha(c.t) + ')</a>').join(', ')] : null,
    ]);
    if (n.saca.length) h += '<h3>Saca de correo</h3>' + n.saca.slice(0, 12).map(c => '<div>' + (S.reg.carta[c.k] ? S.reg.carta[c.k].nom : c.k) + ' → ' + L('A', c.para) + ' <span class="nota">(escrita hace ' + dias(m.t - c.t) + ')</span></div>').join('');
    if (n.not.size) { h += '<h3>Lo que cuenta la tripulación</h3>'; const ids = Array.from(n.not.keys()).sort((x, y) => y - x).slice(0, 5); for (const pid of ids) { const p = m.paq[pid]; const v = n.not.get(pid); h += '<div class="ev i1" data-ev="' + p.ev + '">' + txtEv(m.ev[p.ev]) + (p.x > 0 && Math.round(v.x) !== Math.round(p.x) ? ' <b>Ellos dicen ' + Math.round(v.x) + '</b> (fueron ' + Math.round(p.x) + ').' : '') + '</div>'; } }
    h += '<h3>Caja negra</h3>' + (n.cn.length ? n.cn.slice(-12).reverse().map(i => evHtml(m.ev[i])).join('') : '<span class="nota">sin anotaciones</span>');
    return h;
  }

  function persona(ref) {
    const p = m.per[ref.id]; const lug = S.per.lugar(m, p);
    let h = '<div class="cab">' + av('P' + p.id, 104) + '<div><h2>' + esc(p.nom) + (P.epiteto ? P.epiteto(p) : '') + '</h2><div class="sub">' + (p.cargo && p.cargo.nom ? esc(p.cargo.nom) : (ROL[p.rol] || p.rol)) + ' · ' + (p.vivo ? Math.floor(S.per.edad(m, p)) + ' años' : 'murió a los ' + Math.floor((p.muere - p.nace) / S.ANIO)) + ' · ' + (p.en.t === 'N' && m.nav[p.en.id] ? 'a bordo de ' + L('N', p.en.id) : L('A', lug)) + '</div>';
    h += '<div class="filtro"><button data-accion="seguir">👁 Seguir por el mapa</button></div></div></div>' + P.extraPersona(p) + (P.muertesFicha ? P.muertesFicha(p) : '');
    if (p.vivo) h += mano('P', p.id); else if (p.evMuerte !== undefined && p.evMuerte >= 0) h += evHtml(m.ev[p.evMuerte]);
    h += '<h3>Rasgos</h3>' + kv(S.RNOM.map((nm, i) => [S.cap(nm), barra(p.r[i]) + p.r[i].toFixed(2)]).concat([['Carisma', barra(p.car, 'a') + p.car.toFixed(2)], ['Habilidad', barra(p.hab, 'a') + p.hab.toFixed(2)]]));
    h += '<h3>Ideas</h3><div class="nota">' + S.IDEO.map((nm, i) => { const [x, y] = nm.split('–'); return (p.ideo[i] > 0 ? x : y) + ' ' + Math.abs(p.ideo[i]).toFixed(1); }).join(' · ') + '</div>';
    h += kv([['Dinero', n0(p.din)], p.est >= 0 ? ['Estado', estadoDe(p.est)] : null, p.fac >= 0 ? ['Facción', L('F', p.fac, m.fac[p.fac].nom || 'protofacción')] : null, p.oficio ? ['Oficio', esc(p.oficio)] : null,
      p.fam.con >= 0 ? ['Pareja', L('P', p.fam.con) + (m.per[p.fam.con].vivo ? '' : ' †')] : null, p.fam.hijos.length ? ['Hijos', p.fam.hijos.map(i => L('P', i) + (m.per[i].vivo ? ' (' + Math.floor(S.per.edad(m, m.per[i])) + ')' : ' †')).join(', ')] : null, p.fam.padres.length ? ['Padres', p.fam.padres.map(i => L('P', i)).join(', ')] : null]);
    if (p.ven) {
      const v = p.ven; const G = S.ven.actual(m, p);
      h += '<h3>Venganza</h3>' + kv([['Rencor G', barra(G, 'r') + G.toFixed(2) + ' <span class="nota">(actúa por encima de 0,60)</span>'], ['Estado', v.estado], ['Por', L('P', v.victima)], ['Busca a', v.obj >= 0 ? L('P', v.obj) + (v.errada ? ' <span class="nota">(un estibador se lo señaló)</span>' : '') : 'aún no sabe a quién'], ['Medios', (v.escaner ? 'escáner · ' : '') + (v.vuela ? 'sabe volar · ' : '') + (v.aliados ? v.aliados + ' cazarrecompensas · ' : '') + (v.donde >= 0 ? 'sabe dónde: ' + L('A', v.donde) : 'no sabe dónde')], ['Plan', v.plan && v.plan.length ? v.plan.map(x => x.replace(/_/g, ' ')).join(' → ') : '—'], v.obj >= 0 && p.vivo ? ['P(vencer)', pc(S.ven.pGanar(m, p)) + ' <span class="nota">(si es menos del 50 %, no ataca: busca ventaja)</span>'] : null]);
      const lib = m.obj[v.libreta]; if (lib && lib.entradas.length) h += '<div class="nota"><b>La libreta</b> (' + L('O', lib.id, 'objeto') + ')</div>' + lib.entradas.slice(-8).map(x => '<div class="ev"><span class="f">' + m.fecha(x.t) + '</span>' + esc(x.txt) + '</div>').join('');
      h += evHtml(m.ev[v.ev]);
    }
    h += '<h3>Su historia</h3>' + (P.historial ? P.historial({ t: 'P', id: p.id }) : P.bio(p));
    if (p.mem.length) h += '<h3>Recuerdos</h3>' + p.mem.slice().sort((x, y) => S.per.intensidad(m, y) - S.per.intensidad(m, x)).slice(0, 5).map(x => '<div>' + barra(S.per.intensidad(m, x), 'r') + x.emo + (x.ev >= 0 ? ' · <a class="ref" data-ev="' + x.ev + '">' + m.fecha(m.ev[x.ev].t) + '</a>' : '') + '</div>').join('');
    if (p.rel.size) { const rs = Array.from(p.rel.entries()).sort((x, y) => Math.abs(y[1].op) - Math.abs(x[1].op)).slice(0, 6); h += '<h3>Relaciones</h3>' + rs.map(([id, r]) => '<div>' + L('P', id) + ' <span class="nota">opinión ' + r.op.toFixed(1) + ' · confianza ' + r.conf.toFixed(1) + (r.miedo > 0.05 ? ' · miedo ' + r.miedo.toFixed(1) : '') + '</span></div>').join(''); }
    return h;
  }
  function cara(ref) {
    const c = ref.cara; const a = m.ase[ref.a]; if (!c) return '';
    return '<h2>' + esc(c.nom) + '</h2><div class="sub">' + esc(c.oficio) + ' · ' + c.edad + ' años · ' + L('A', a.id) + '</div><p>' + (c.odia ? 'Odia al régimen, aunque en el desfile aplaude.' : 'No tiene nada contra el régimen. O eso dice.') + '</p><p class="nota">Es una de las ' + n0(m.coh[c.coh].n) + ' personas de su cohorte: todavía es un número. Si la tocas, pasa a tener ficha propia y ya no vuelve al grupo anónimo.</p><div class="dios"><button data-accion="promover">Hablar con ' + (c.sexo === 'M' ? 'ella' : 'él') + '</button></div>';
  }

  function estado(ref) {
    const e = m.est[ref.id]; const mis = S.pol.territorio(m, e); const def = S.reg.gob[e.tipoGob];
    let h = '<div class="cab">' + ban('E' + e.id, 96, 64) + (e.gob >= 0 ? av('P' + e.gob, 64) : '') + '<div><h2>' + esc(e.nom) + '</h2><div class="sub">' + def.nom + ' · poder ' + ({ personal: 'personal (se sostiene en la persona)', estado: 'de Estado (leyes y burocracia)', popular: 'popular (es querido)' })[e.base] + (e.vivo ? '' : ' · <b>ya no existe</b>') + '</div>';
    h += '</div></div>' + P.extraEstado(e);
    if (e.vivo) h += mano('E', e.id, [e.gob >= 0 ? ['matar_publico', e.gob, 'Matar al gobernante en público'] : null, e.gob >= 0 ? ['matar_silencio', e.gob, 'Que parezca un infarto'] : null]);
    const deu = m.deu.filter(d => d.deudor === e.id && d.monto > 1);
    h += kv([['Gobierna', e.gob >= 0 ? L('P', e.gob) + ' <span class="nota">desde hace ' + dias(m.t - e.desde) + '</span>' : '<b>nadie (interregno)</b>'], ['Capital', L('A', e.cap)], ['Población', n0(e.pob || 0) + ' en ' + mis.length + ' asentamientos'],
      ['Tesoro', n0(e.tes) + ' <span class="nota">(mes: +' + n0(e.ultIngreso || 0) + ' / −' + n0(e.ultGasto || 0) + ')</span>'], ['Impuestos · tributo', pc(e.imp) + ' · ' + pc(e.trib) + ' de la cosecha'],
      ['Moneda', esc(e.mon.nom) + ' · precios ×' + e.mon.P.toFixed(2) + (e.infl > 0.002 ? ' · subiendo' : '')], deu.length ? ['Deuda', deu.map(d => n0(d.monto) + ' a ' + L('F', d.acreedor)).join('<br>')] : null,
      e.msc ? ['Pagas', '<b>' + e.msc + ' meses de atraso</b>'] : null, ['Flota', n0(e.navG || 0) + ' naves (en fragatas) · ' + (e.escuadras || 0) + ' escuadras'], ['Ejército', n0(e.soldados || 0) + ' soldados de guarnición'], ['Agravio medio', barra(e.agr || 0, 'a') + (e.agr || 0).toFixed(2)],
      e.gue.size ? ['En guerra con', Array.from(e.gue.entries()).map(([j, w]) => estadoDe(j) + ' <span class="nota">' + (w.civil ? 'civil · ' : '') + 'cansancio ' + (w.cans || 0).toFixed(1) + ' · marcador ' + (w.score || 0) + '</span>').join('<br>')] : null,
      e.fiestas.length ? ['Fiestas', e.fiestas.map(f => esc(f.nom)).join(', ')] : null, e.origen >= 0 ? ['Nació de', estadoDe(e.origen)] : null]);
    if (e.vivo) {
      h += '<h3>Las piezas del poder</h3><table><tr><th>Pieza</th><th>Manda</th><th class="n">Cuota</th></tr>' + S.pol.piezas(m, e).map(p => '<tr><td>' + esc(p.nom) + '</td><td>' + L('P', p.lid) + '</td><td class="n">' + (p.poder * 100).toFixed(1) + ' %</td></tr>').join('') + '</table>';
      if (e.suc) h += '<div class="nota"><b>Subasta en curso:</b> ' + e.suc.cands.map(c => L('P', c.p) + ' ' + pc(c.poder)).join(' · ') + '</div>';
    }
    if (e.ultSuc) h += '<h3>La última subasta (' + m.fecha(e.ultSuc.t) + ')</h3><table><tr><th>Pieza</th><th class="n">Poder</th><th>Eligió</th></tr>' + e.ultSuc.tabla.map(p => '<tr><td>' + esc(p.nom) + '</td><td class="n">' + (p.poder * 100).toFixed(1) + ' %</td><td>' + (typeof p.lado === 'number' ? L('P', p.lado) : esc(String(p.lado))) + '</td></tr>').join('') + '</table>';
    if (e.vivo) {
      h += '<h3>Lo que sabe la capital</h3><table><tr><th>Provincia</th><th class="n">Control</th><th class="n">Hambre real</th><th class="n">informada</th><th class="n">hace</th></tr>';
      for (const a of mis.slice().sort((x, y) => x.control - y.control).slice(0, 30)) { if (a.cap) continue; const s = e.sab.get(a.id); h += '<tr><td>' + L('A', a.id) + '</td><td class="n">' + pc(a.control) + '</td><td class="n">' + pc(a.H) + '</td><td class="n">' + (s ? pc(s.H) : '—') + '</td><td class="n">' + (s ? Math.round(m.t - s.t) + ' d' : '—') + '</td></tr>'; }
      h += '</table><h3>Flotas</h3>' + (e.flo.map(i => m.flo[i]).filter(f => f.vivo).map(f => '<div>' + esc(f.nom) + ': ' + n0(f.nav.reduce((s, i) => s + m.nav[i].cascos, 0)) + ' naves en ' + f.nav.length + ' escuadras (' + f.nav.slice(0, 6).map(i => L('N', i)).join(', ') + ') · ' + f.st + (f.mis ? ' · misión: ' + f.mis.tipo + ' ' + L('S', f.mis.sis) : '') + ' · ' + L('P', f.alm) + '</div>').join('') || '<span class="nota">ninguna</span>');
    }
    return h;
  }

  function faccion(ref) {
    const f = m.fac[ref.id]; const casa = f.tipo === 'casa';
    let h = '<div class="cab">' + ban('F' + f.id, 78, 52) + (f.lid >= 0 ? av('P' + f.lid, 52) : '') + '<div><h2>' + esc(f.nom || 'Gente que se reúne') + '</h2><div class="sub">' + (casa ? 'casa mercante' : f.etapa === 'proto' ? 'protofacción: aún sin nombre, solo una queja común y alguien que conecta a la gente' : f.tipo) + (f.vivo ? '' : ' · <b>disuelta</b>') + '</div>';
    h += '</div></div>' + P.extraFaccion(f);
    h += mano('F', f.id, [f.lid >= 0 && m.per[f.lid].vivo ? ['matar_publico', f.lid, 'Matar a quien la encabeza'] : null]);
    h += kv([['Cabeza', L('P', f.lid)], ['Sede', L('A', f.sede)], f.sim ? ['Emblema', esc(f.sim)] : null,
      casa ? ['Naves', f.naves === undefined ? '—' : f.naves] : ['Miembros', n0(S.fac.miembros(m, f))], ['Caja', n0(f.caja)],
      casa ? null : ['Organización', barra(f.O / 2, 'v') + f.O.toFixed(2) + (f.etapa === 'proto' ? ' <span class="nota">(a 1,00 es institución)</span>' : '')],
      casa ? null : ['Compromiso', barra(f.comp) + f.comp.toFixed(2)],
      casa ? null : ['Contra', ({ reg: 'el régimen', pat: 'la patronal', acap: 'los acaparadores', ext: 'el invasor', crim: 'quienes lo hicieron' })[f.enem.k] + (f.enem.k === 'reg' && f.enem.id >= 0 ? ' (' + estadoDe(f.enem.id) + ')' : '')],
      f.cel.length >= 2 ? ['Cohesión λ₂', barra(f.l2, f.l2 < 0.15 ? 'r' : 'v') + f.l2.toFixed(2) + ' <span class="nota">(cerca de 0: a una crisis de romperse)</span>'] : null,
      f.ult ? ['Último movimiento', f.ult.replace(/_/g, ' ')] : null, f.estado >= 0 ? ['Gobierna', estadoDe(f.estado)] : null,
      f.normas.length ? ['Normas', f.normas.map(x => esc(x.c) + ' → ' + esc(x.cast)).join('<br>')] : null,
      f.fiestas && f.fiestas.length ? ['Fiestas', f.fiestas.map(x => esc(x.nom)).join(', ')] : null,
      casa && f.almacenes ? ['Almacenes', f.almacenes.map(i => L('A', i)).join(', ')] : null]);
    if (casa) { const d = m.deu.filter(x => x.acreedor === f.id && x.monto > 1); if (d.length) h += '<h3>Le deben</h3>' + d.map(x => '<div>' + estadoDe(x.deudor) + ': ' + n0(x.monto) + '</div>').join(''); }
    if (f.cel.length) h += '<h3>Casas</h3><table><tr><th>Dónde</th><th>Enlace</th><th class="n">Puente</th></tr>' + f.cel.map(c => '<tr><td>' + L('A', c.ase) + '</td><td>' + L('P', c.lid) + (c.lid >= 0 && !m.per[c.lid].vivo ? ' †' : '') + '</td><td class="n">' + (c.padre < 0 ? 'sede' : c.puente.toFixed(2)) + '</td></tr>').join('') + '</table>' + (f.cel.length >= 2 ? '<p class="nota">Mata al enlace entre dos casas y el puente se queda en nada: provocarás un cisma.</p>' : '');
    if (f.fund >= 0) h += '<h3>Hecho fundador</h3>' + evHtml(m.ev[f.fund]);
    return h;
  }

  function objeto(ref) {
    const o = m.obj[ref.id]; const d = o.donde;
    let h = '<h2>' + esc(S.cap(S.obj.nombre(m, o))) + '</h2><div class="sub">' + o.tipo + (o.vivo ? '' : ' · destruido') + '</div>';
    h += kv([['Número exterior', esc(o.serie)], o.interno !== o.serie ? ['Anillo interno', o.borrado ? 'limado' : esc(o.interno)] : null, o.fab >= 0 ? ['Fabricado en', L('I', o.fab) + (o.lote ? ' · lote ' + o.lote + ', turno de ' + o.turno : '')] : null, ['Fecha', m.fecha(o.t0)],
      d ? ['Dónde está', d.t === 'P' ? 'lo lleva ' + L('P', d.id) : L(d.t, d.id) + (d.carga ? ' (en la bodega)' : '')] : null,
      o.tipo === 'nucleo' ? ['Estado', nucleoHtml(o)] : null, o.tipo === 'nucleo' ? ['Trabajo', o.cpd ? o.cpd + ' ciclos al día · K = ' + o.K.toFixed(4) : 'parado'] : null,
      o.firma ? ['Huella', '<span class="nota">' + o.firma.map(x => x.toFixed(2)).join(' ') + '</span>'] : null]);
    if (o.tipo === 'libreta') h += '<h3>Entradas</h3>' + (o.entradas.map(x => '<div class="ev"><span class="f">' + m.fecha(x.t) + '</span>' + esc(x.txt) + '</div>').join('') || '<span class="nota">en blanco</span>');
    h += '<h3>Historial</h3>' + (o.hist.length ? o.hist.map(i => evHtml(m.ev[i])).join('') : '<span class="nota">Nada que haya importado. Todavía.</span>');
    return h;
  }

  // ── Crónica.
  P.cronica = function (mm, op) {
    m = mm; const imp = op.imp; const lug = op.aqui && op.sel ? op.sel : null;
    let h = '<div class="filtro">Importancia <select data-op="imp"><option value="0"' + (imp === 0 ? ' selected' : '') + '>todo</option><option value="1"' + (imp === 1 ? ' selected' : '') + '>notable</option><option value="2"' + (imp === 2 ? ' selected' : '') + '>grande</option><option value="3"' + (imp === 3 ? ' selected' : '') + '>histórico</option></select><label><input type="checkbox" data-op="aqui"' + (op.aqui ? ' checked' : '') + '> solo lo elegido</label></div>';
    let k = 0; const sis = lug && lug.t === 'S' ? lug.id : -1; const as = lug && lug.t === 'A' ? lug.id : lug && lug.t === 'I' ? m.ins[lug.id].ase : -1; const tok = lug ? '{' + lug.t + lug.id + '}' : null;
    for (let i = m.ev.length - 1; i >= 0 && k < 220; i--) {
      const e = m.ev[i]; if (e.imp < imp && e.k !== 'mano') continue;
      if (lug && !(e.a === as && as >= 0) && !(e.s === sis && sis >= 0) && e.txt.indexOf(tok) < 0) continue;
      h += evHtml(e); k++;
    }
    return h + (k ? '' : '<p class="nota">Nada todavía.</p>');
  };

  // ── Historiador: de un hecho, hacia atrás hasta su causa y hacia delante hasta lo que provocó.
  P.historiador = function (mm, id) {
    m = mm;
    if (id === undefined || id === null || !m.ev[id]) return '<h2>Historiador</h2><p class="nota">Elige un hecho en la crónica (o en cualquier ficha) y sigue las flechas: por qué pasó y qué provocó. Cada hecho guarda sus causas; nadie ha escrito la cadena.</p>';
    const e = m.ev[id];
    let h = '<h2>Historiador</h2><div class="dios"><button data-accion="centrarEv">📍 Ir al lugar</button><button data-accion="noticia">📡 ¿Quién se ha enterado?</button></div>' + evHtml(e);
    const cad = m.cadena(id, 80).slice(1);
    h += '<h3>Por qué pasó (' + cad.length + ')</h3><div class="arbol">' + (cad.map(x => evHtml(m.ev[x.id], x.n - 1)).join('') || '<span class="nota">No tiene causa dentro del mundo' + (e.k === 'mano' ? ': fuiste tú.' : ' que haya quedado registrada.') + '</span>') + '</div>';
    const out = []; const visto = new Set([id]); let nivel = [id];
    for (let n = 0; n < 6 && out.length < 90; n++) { const sig = []; for (const x of nivel) { const ef = m.efe[x]; if (!ef) continue; for (const y of ef) if (!visto.has(y)) { visto.add(y); out.push({ id: y, n }); sig.push(y); } } nivel = sig; }
    out.sort((x, y) => m.ev[x.id].t - m.ev[y.id].t);
    h += '<h3>Lo que provocó (' + out.length + ')</h3><div class="arbol">' + (out.slice(0, 90).map(x => evHtml(m.ev[x.id], x.n)).join('') || '<span class="nota">Nada, de momento.</span>') + '</div>';
    return h;
  };

  // ── Sismógrafo: qué series se ven lo decide cada cual (se guarda con el aspecto).
  const GLOBALES = ['pob', 'hambre', 'pgrano', 'agravio', 'R', 'calle', 'eficacia', 'psi', 'asabiya', 'control', 'guerras', 'estados', 'facciones', 'l2min', 'docreb', 'tec', 'naves', 'piratas', 'fragatas', 'pecios', 'nucleos', 'pcomb', 'paro', 'ipc', 'comercio', 'vengadores', 'paquetes'];
  P.sismografo = function (mm, op) {
    m = mm; const sel = op.sel; const V = U.viz; const Pr = U.prefs; const act = Pr.series();
    const ult = (k) => { const s = m.series[k]; if (s) for (let i = s.length - 1; i >= 0; i--) if (s[i] === s[i]) return V.num(s[i]); return ''; };
    const graf = (claves, tit, max, nombres, col) => V.titulo(tit, claves.length === 1 ? ult(claves[0]) : undefined) + (nombres ? V.leyenda(nombres.map((x, j) => [Pr.col(j), x + ' <b>' + ult(claves[j]) + '</b>'])) : '') + '<canvas class="graf" data-series="' + claves.join(',') + '"' + (nombres ? ' data-nombres="' + nombres.join('|') + '"' : '') + (max ? ' data-max="' + max + '"' : '') + (col ? ' data-col="' + col + '"' : '') + '></canvas>';
    let h = '<h2>Sismógrafo</h2><p class="nota">Una muestra al mes de cada cosa, para ver cuándo algo va a romperse. Pasa el ratón por una gráfica y lee el valor de cada momento. El estilo, el periodo y los colores se cambian en 🎨.</p>';
    if (sel && sel.t === 'A') {
      const a = m.ase[sel.id];
      h += '<h3>' + esc(a.nom) + '</h3>' + graf(['H' + a.id, 'Q' + a.id, 'R' + a.id, 'f' + a.id], 'La olla', 1, ['hambre', 'agravio', 'represión percibida', 'en la calle']) + graf(['G' + a.id], 'Precio del grano', 0, null, Pr.col(1)) + graf(['p' + a.id], 'Población', 0, null, Pr.col(2));
      h += '<h3>Umbral de revuelta aquí</h3><div class="nota">Gente en la calle según la represión percibida, con el agravio de hoy (' + a.agr.toFixed(2) + '). Las dos líneas no coinciden: es histéresis. La raya es la represión actual.</div><canvas class="graf" data-umbral="' + a.id + '"></canvas>';
    } else if (sel && sel.t === 'F') { const f = m.fac[sel.id]; h += '<h3>' + esc(f.nom) + '</h3>' + graf(['L' + f.id], 'Cohesión λ₂', 1, null, Pr.col(4)) + graf(['M' + f.id], 'Miembros', 0, null, Pr.col(2)); }
    else if (sel && sel.t === 'E') { const e = m.est[sel.id]; h += '<h3>' + esc(e.nom) + '</h3>' + [['T', 'Tesoro'], ['P', 'Nivel de precios'], ['W', 'Naves de guerra'], ['Y', 'Tensión estructural Ψ'], ['B', 'Asabiya', 1], ['X', 'Nivel técnico'], ['A', 'Aptitud del diseño de fragata en producción']].map((x, j) => graf([x[0] + e.id], x[1], x[2] || 0, null, Pr.col(j))).join(''); }
    h += '<h3>Galaxia</h3><div class="chips">' + GLOBALES.map(k => { const d = S.sismo.find(x => x.clave === k); return d ? '<button class="chipb' + (act.indexOf(k) >= 0 ? ' on' : '') + '" data-serie="' + k + '">' + esc(d.nom) + '</button>' : ''; }).join('') + '</div>';
    let n = 0; GLOBALES.forEach((k, j) => { if (act.indexOf(k) < 0) return; const d = S.sismo.find(x => x.clave === k); if (d) { h += graf([k], d.nom, 0, null, Pr.col(j)); n++; } });
    if (!n) h += '<p class="nota">Pulsa arriba las series que quieras ver.</p>';
    return h;
  };
  const met = (nom, val, f, col) => '<div><span class="l">' + nom + '</span><b>' + val + '</b>' + U.viz.medidor(f, col) + '</div>';
  P.h = { L, kv, barra, evHtml, esc, n0, pc, estadoDe, chip, dias, av, ban, txtEv, tok, met, ROL };
  P.pintar = function (mm, raiz) {
    m = mm; P._m = mm; if (P.pintarArte) P.pintarArte(mm, raiz); U.viz.pintar(mm, raiz); if (P.pintarExtra) P.pintarExtra(mm, raiz);
    for (const cv of raiz.querySelectorAll('canvas[data-umbral]')) { if (cv.dataset.umbral === undefined) continue; const [c, w, hh] = U.viz.lienzo(cv); c.lineWidth = 1.4; c.font = '10px Segoe UI'; c.fillStyle = U.prefs.tema().tenue; umbral(c, w, hh, m.ase[+cv.dataset.umbral]); }
  };
  function umbral(c, w, hh, a) {
    const X = (R) => (R - 0.4) / 0.6 * w, Y = (f) => hh - 6 - f / 0.7 * (hh - 16);
    c.strokeStyle = U.prefs.tema().linea; c.beginPath(); c.moveTo(0, Y(0.5)); c.lineTo(w, Y(0.5)); c.stroke(); c.fillText('media ciudad', 3, Y(0.5) - 3);
    // Bajando la represión desde la calma, y subiéndola con la gente ya en la calle.
    let f = 0.03; c.strokeStyle = U.prefs.col(2); c.beginPath(); for (let i = 0; i <= 120; i++) { const R = 1 - i / 120 * 0.6; f = S.soc.equilibrio(f, a.agr, R); if (i === 0) c.moveTo(X(R), Y(f)); else c.lineTo(X(R), Y(f)); } c.stroke();
    c.strokeStyle = U.prefs.col(3); c.beginPath(); for (let i = 0; i <= 120; i++) { const R = 0.4 + i / 120 * 0.6; f = S.soc.equilibrio(f, a.agr, R); if (i === 0) c.moveTo(X(R), Y(f)); else c.lineTo(X(R), Y(f)); } c.stroke();
    c.strokeStyle = '#fff'; c.setLineDash([3, 3]); c.beginPath(); c.moveTo(X(a.R), 0); c.lineTo(X(a.R), hh); c.stroke(); c.setLineDash([]);
    c.fillText('R = 0,4', 3, hh - 3); c.textAlign = 'right'; c.fillText('R = 1', w - 3, hh - 3); c.textAlign = 'left';
  }

  // ── Estados: quién tiene qué, de un vistazo, y una tarjeta por Estado.
  P.estados = function (mm) {
    m = mm; P._m = mm; const V = U.viz; const vivos = m.est.filter(e => e.vivo).sort((x, y) => (y.pob || 0) - (x.pob || 0));
    const terr = new Map(vivos.map(e => [e.id, S.pol.territorio(m, e).length])); const mx = (f) => Math.max(1, ...vivos.map(f));
    const mP = mx(e => e.pob || 0), mF = mx(e => e.navG || 0), mT = mx(e => Math.max(0, e.tes)), mA = mx(e => terr.get(e.id));
    const trozos = (f) => vivos.map(e => ({ nom: e.nom, v: Math.max(0, f(e)), col: e.col }));
    let h = '<h2>Estados</h2>';
    if (vivos.length) h += '<div class="dona"><canvas data-dona="' + V.dato('pobEst', { partes: trozos(e => e.pob || 0), centro: String(vivos.length), sub: vivos.length === 1 ? 'Estado' : 'Estados' }) + '" data-w="104" data-h="104" style="width:104px;height:104px"></canvas><div style="flex:1;min-width:0"><span class="nota">Población</span>' + V.reparto(trozos(e => e.pob || 0), { sinLeyenda: true }) + '<span class="nota">Flota de guerra</span>' + V.reparto(trozos(e => e.navG || 0), { sinLeyenda: true }) + '<span class="nota">Tesoro</span>' + V.reparto(trozos(e => e.tes), { sinLeyenda: true }) + '</div></div>';
    for (const e of vivos) {
      const ins = (e.gue.size ? V.insignia('⚔ en guerra', 'mal') + ' ' : '') + (e.suc ? V.insignia('👑 sucesión', 'aviso') + ' ' : '') + (e.msc ? V.insignia('🪙 ' + e.msc + ' meses sin pagar', 'aviso') : '');
      h += '<div class="tarjeta est" style="--c:' + e.col + '"><div class="cab">' + ban('E' + e.id, 48, 32) + (e.gob >= 0 ? av('P' + e.gob, 34) : '') + '<div><b>' + L('E', e.id) + '</b> ' + ins + '<br><span class="nota">' + (e.gob >= 0 ? L('P', e.gob) : 'interregno') + ' · ' + S.reg.gob[e.tipoGob].nom.toLowerCase() + '</span></div></div><div class="met">' + met('Población', S.fmt(e.pob || 0), (e.pob || 0) / mP, e.col) + met('Territorio', terr.get(e.id), terr.get(e.id) / mA, e.col) + met('Flota', n0(e.navG || 0), (e.navG || 0) / mF, e.col) + met('Tesoro', S.fmt(e.tes), Math.max(0, e.tes) / mT, e.col) + '</div></div>';
    }
    const muertos = m.est.filter(e => !e.vivo);
    if (muertos.length) h += '<h3>Los que ya no existen</h3>' + muertos.map(e => '<div>' + chip(e) + L('E', e.id) + '</div>').join('');
    const casas = m.fac.filter(f => f.vivo && f.tipo === 'casa').sort((x, y) => y.caja - x.caja); const mc = Math.max(1, ...casas.map(f => f.caja));
    h += '<h3>Casas mercantes</h3>' + casas.map(f => '<div class="flujo">' + L('F', f.id) + ' <span class="nota">caja ' + S.fmt(f.caja) + ' · ' + (f.naves || 0) + ' naves</span>' + V.medidor(Math.max(0, f.caja) / mc, V.SEM.verde) + '</div>').join('');
    return h;
  };
  P.facciones = function (mm) {
    m = mm; P._m = mm; const V = U.viz; const vivas = m.fac.filter(f => f.vivo && f.tipo !== 'casa').sort((x, y) => y.O - x.O);
    const mi = new Map(vivas.map(f => [f.id, S.fac.miembros(m, f)])); const mM = Math.max(1, ...vivas.map(f => mi.get(f.id)));
    let h = '<h2>Facciones</h2><p class="nota">Nadie las ha escrito: nacen de gente con un problema común, crecen si les va bien y se rompen por donde las amistades son más débiles.</p>';
    for (const f of vivas) h += '<div class="fila">' + ban('F' + f.id, 36, 24) + '<div><b>' + L('F', f.id, f.nom || '(sin nombre) ' + m.ase[f.sede].nom) + '</b> ' + (f.enHuelga ? V.insignia('✊ en huelga', 'aviso') : '') + '<br><span class="nota">' + f.tipo + ' · ' + L('A', f.sede) + '</span><div class="met tres">' + met('Miembros', n0(mi.get(f.id)), mi.get(f.id) / mM, V.SEM.azul) + met('Organización', f.O.toFixed(2), f.O / 3, V.SEM.verde) + met('Cohesión λ₂', f.cel.length >= 2 ? f.l2.toFixed(2) : '—', f.cel.length >= 2 ? f.l2 : 0, f.l2 < 0.15 ? V.SEM.rojo : V.SEM.ambar) + '</div></div></div>';
    if (!vivas.length) h += '<p class="nota">Ninguna, de momento.</p>';
    const ven = (m.vengadores || []).filter(p => p.vivo && p.ven && p.ven.estado === 'activa');
    if (ven.length) h += '<h3>Con una libreta</h3>' + ven.slice(0, 30).map(p => '<div class="fila">' + av('P' + p.id, 30) + '<div>' + L('P', p.id) + ' <span class="nota">' + (p.ven.obj >= 0 ? 'busca a ' : 'no sabe a quién busca') + '</span>' + (p.ven.obj >= 0 ? L('P', p.ven.obj) : '') + '<br><span class="nota">rencor ' + S.ven.actual(m, p).toFixed(2) + ' · ' + (p.ven.paso || '').replace(/_/g, ' ') + '</span></div></div>').join('');
    const muertas = m.fac.filter(f => !f.vivo && f.etapa === 'inst' && f.tipo !== 'casa').slice(-12);
    if (muertas.length) h += '<h3>Las que se deshicieron</h3>' + muertas.map(f => '<div>' + L('F', f.id) + '</div>').join('');
    return h;
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
