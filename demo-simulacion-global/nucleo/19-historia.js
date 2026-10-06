// Historia grande: quién pesa en ella y por qué. Todo sale de lo que el mundo ya registra (hechos con sus causas,
// noticias que viajan, flujos de comercio); aquí no hay guion.
//  · Influencia causal: cada hecho pesa lo suyo más una parte de lo que provocó (suma hacia atrás por el grafo de causas).
//  · Fama: el nombre de alguien llega a un asentamiento cuando llega una noticia que lo nombra.
//  · Figuras: caudillo, profeta, mártir, invicto, reformador, tirano, genio, unificador, sanador. Nacen de sus rasgos
//    y de lo que les ha pasado, y cada una cambia las reglas a su alrededor.
//  · Credos: una fe con nombre que se contagia por las rutas de comercio (crecimiento logístico más lo que traen las bodegas).
//  · Accidentes monumentales: peste (modelo SIR que viaja en las naves), almacén de núcleos en cadena, llamarada, hallazgo.
//  · Épocas: la historia se parte en tramos por puntos de cambio y cada tramo recibe nombre.
//  · Biografías: la vida de cada persona contada con frases elegidas por su importancia y por lo dramático de su vida.
(function (g) {
  'use strict';
  const S = g.SIM; const R = S.R; const B = S.B; const P = S.per; const F = S.frases;
  const H = S.his = {};
  const num = (x) => Math.round(x).toLocaleString('es-ES');
  const rngH = (m) => m.hisRng || (m.hisRng = m.rngDe('H', 0));
  // Una frase del banco para una magnitud y un drama; si el banco aún no tiene ese hueco, el texto llano de reserva.
  const fr = (slot, E, D, datos, sexo, reserva) => (F && F.hay(slot) ? F.de(slot, E, D, datos, sexo) : F ? F.rellena(reserva, datos, sexo) : reserva);
  const magn = (n, tope) => S.clamp(Math.log10(1 + Math.max(0, n)) / Math.log10(1 + tope), 0, 1);

  // ── 1. Influencia causal. I(e) = w(e) + λ · Σ_{efectos} I(efecto) / nº de causas del efecto.
  // Las causas se registran antes que sus efectos, así que basta una pasada del último hecho al primero.
  H.LAMBDA = 0.6; H.PESO = [0.2, 1, 4, 12];
  H.influencia = function (m) {
    if (m._inf && m._inf.n === m.ev.length) return m._inf.v;
    const n = m.ev.length; const v = new Float64Array(n);
    for (let i = n - 1; i >= 0; i--) { const e = m.ev[i]; v[i] += H.PESO[e.imp] + (e.d && e.d.muertos > 0 ? Math.log10(1 + e.d.muertos) : 0); const c = e.c; if (c.length) { const parte = H.LAMBDA * v[i] / c.length; for (let k = 0; k < c.length; k++) v[c[k]] += parte; } }
    m._inf = { n, v }; return v;
  };
  const sujeto = (e) => { const x = /\{P(\d+)\}/.exec(e.txt); return x ? +x[1] : -1; };
  // Peso histórico de cada persona: la influencia de los hechos que protagoniza (entera si es el sujeto; 0,4 si solo sale)
  // más las muertes a su cuenta en escala logarítmica.
  H.pesos = function (m) {
    if (m._pes && m._pes.n === m.ev.length) return m._pes;
    const v = H.influencia(m); const w = new Float64Array(m.per.length);
    if (m.bio) for (const [id, l] of m.bio) { let s = 0; for (const i of l) { const e = m.ev[i]; s += v[i] * (sujeto(e) === id || (e.d && (e.d.p === id || e.d.por === id)) ? 1 : 0.4); } w[id] = s; }
    for (const p of m.per) { const k = S.mas.letalidad(p); if (k > 0) w[p.id] += 3 * Math.log10(1 + k); if (p.fig) w[p.id] *= 1.25; }
    const ord = Array.from(w).map((x, i) => [x, i]).sort((a, b) => b[0] - a[0]); const pos = new Int32Array(m.per.length); ord.forEach((x, i) => { pos[x[1]] = i; });
    const ref = ord.length > 2 ? ord[2][0] : (ord.length ? ord[0][0] : 1);
    m._pes = { n: m.ev.length, w, pos, ref: Math.max(1, ref), orden: ord.map(x => x[1]) }; return m._pes;
  };
  H.NIVELES = ['anónimo', 'conocido', 'notable', 'histórico', 'legendario'];
  // Épica (0..1): el peso propio frente al del tercero más grande, en escala logarítmica. De ahí sale el nivel.
  H.epica = function (m, p) { const q = H.pesos(m); return S.clamp(Math.log1p(q.w[p.id]) / Math.log1p(q.ref), 0, 1); };
  H.nivel = function (m, p) { const E = H.epica(m, p); return E >= 0.93 ? 4 : E >= 0.72 ? 3 : E >= 0.5 ? 2 : E >= 0.25 ? 1 : 0; };
  // Drama (0..1): cada golpe de una vida suma como una «o» ruidosa: D = 1 − Π(1 − d_i).
  H.drama = function (m, p) {
    const d = []; const modo = !p.vivo && p.evMuerte >= 0 && m.ev[p.evMuerte].d ? m.ev[p.evMuerte].d.modo : null;
    if (modo && modo !== 'natural') d.push(0.45); if (!p.vivo && (p.muere - p.nace) / S.ANIO < 38) d.push(0.2);
    if (p.fam.con >= 0 && !m.per[p.fam.con].vivo && (p.vivo || m.per[p.fam.con].muere < p.muere)) d.push(0.2);
    for (const h of p.fam.hijos) if (!m.per[h].vivo && (p.vivo || m.per[h].muere < p.muere)) { d.push(0.25); break; }
    if (p.ven) d.push(0.25); if (p.rol === 'preso' || p.rol === 'exiliado') d.push(0.15);
    const k = S.mas.letalidad(p); if (k > 0) d.push(Math.min(0.5, 0.12 * Math.log10(1 + k)));
    if (p.mue) for (const x of p.mue) if (x.c === 'pueblo') { d.push(0.25); break; }
    if (p.fig === 'martir' || p.fig === 'tirano') d.push(0.3);
    let q = 1; for (const x of d) q *= 1 - x; return 1 - q;
  };

  // ── 2. Fama: en qué mundos se conoce un nombre. Sube con cada noticia que lo nombra: f' = 1 − (1 − f)(1 − k).
  const nombrados = (e) => { if (e._ps === undefined) { e._ps = null; if (e.txt.indexOf('{P') >= 0) { const s = new Set(); const re = /\{P(\d+)\}/g; let x; while ((x = re.exec(e.txt))) s.add(+x[1]); e._ps = Array.from(s); } } return e._ps; };
  H.oye = function (m, a, pid, k) {
    const p = m.per[pid]; if (!p) return; const f = a.fam || (a.fam = new Map()); const v0 = f.get(pid) || 0; const v1 = 1 - (1 - v0) * (1 - k);
    f.set(pid, v1); if (v0 < 0.2 && v1 >= 0.2) p.fama = (p.fama || 0) + 1;
  };
  H.famaEn = (a, pid) => (a.fam ? a.fam.get(pid) || 0 : 0);
  S.gancho('hecho', function (m, e) { if (e.a < 0) return; const a = m.ase[e.a]; a.memoria = (a.memoria || 0) + H.PESO[e.imp]; if (e.d && e.d.muertos > 0) a.sangre = (a.sangre || 0) + e.d.muertos; if (e.imp < 1) return; const ps = nombrados(e); if (ps) for (const id of ps) H.oye(m, m.ase[e.a], id, 0.25 + 0.2 * e.imp); });
  S.gancho('noticia', function (m, a, p) { const e = m.ev[p.ev]; if (!e) return; const ps = nombrados(e); if (ps) for (const id of ps) H.oye(m, a, id, 0.12 + 0.12 * e.imp); });
  S.gancho('anio', function (m) { for (const a of m.ase) if (a.fam) for (const [id, v] of a.fam) { const p = m.per[id]; const nv = v * (p.vivo ? 0.9 : 0.96); if (v >= 0.2 && nv < 0.2) p.fama = Math.max(0, (p.fama || 0) - 1); if (nv < 0.03) a.fam.delete(id); else a.fam.set(id, nv); } });

  // ── 3. Figuras. Cada arquetipo: quién puede serlo («apto»), qué pasa al serlo («nace») y qué hace cada mes («mes»).
  const Fg = (id, o) => S.def('figura', id, o);
  const cab = (m, p) => (p.fac >= 0 && m.fac[p.fac].vivo && m.fac[p.fac].lid === p.id ? m.fac[p.fac] : null);
  const gobierna = (m, p) => (p.cargo && p.cargo.t === 'gobernante' && p.est >= 0 && m.est[p.est].vivo && m.est[p.est].gob === p.id ? m.est[p.est] : null);
  H.EPITETOS = {
    caudillo: [['el Caudillo', 'la Caudilla'], ['el de la Voz', 'la de la Voz'], ['el Que Levanta', 'la Que Levanta']],
    profeta: [['el Profeta', 'la Profetisa'], ['el Anunciado', 'la Anunciada'], ['el de la Senda', 'la de la Senda']],
    martir: [['el Mártir', 'la Mártir'], ['el Inmolado', 'la Inmolada']],
    invicto: [['el Invicto', 'la Invicta'], ['el Nunca Vencido', 'la Nunca Vencida'], ['Martillo de Flotas', 'Martillo de Flotas']],
    reformador: [['el Justo', 'la Justa'], ['el Reformador', 'la Reformadora'], ['el de los Depósitos', 'la de los Depósitos']],
    tirano: [['el Cruel', 'la Cruel'], ['el Temido', 'la Temida'], ['el de las Purgas', 'la de las Purgas']],
    genio: [['el Ingeniero', 'la Ingeniera'], ['el Sabio', 'la Sabia']],
    unificador: [['el Grande', 'la Grande'], ['el Unificador', 'la Unificadora'], ['el Conquistador', 'la Conquistadora']],
    sanador: [['el Sanador', 'la Sanadora'], ['el de las Manos', 'la de las Manos']],
  };
  H.epiteto = function (m, p) { const l = H.EPITETOS[p.fig]; if (!l) return ''; const x = l[S.hash(m.semilla, 'epi', p.id) % l.length]; return x[p.sexo === 'M' ? 1 : 0]; };
  H.nombrar = function (m, p, fig, datos, c, a) {
    if (p.fig === fig) return -1; p.fig0 = p.fig || null; p.fig = fig; p.figT = m.t; (m.figuras = m.figuras || []).push(p.id);
    const txt = '{P' + p.id + '}, ' + H.epiteto(m, p) + ': ' + S.reg.figura[fig].que(m, p, datos || {});
    const lugar = a !== undefined ? a : P.lugar(m, p);
    const ev = m.reg('figura', txt, { a: lugar, imp: 2, c: c || [], d: { p: p.id, fig } }); p.figEv = ev;
    if (lugar >= 0) S.inf.crear(m, ev, 'figura', lugar, 0.8, 1, { p: p.id });
    return ev;
  };
  Fg('caudillo', {
    nom: 'caudillo', que: (m, p) => 'su nombre ya mueve a la gente de varios mundos contra quien manda',
    apto(m, p) { const f = cab(m, p); return !!f && f.etapa === 'inst' && f.tipo !== 'casa' && f.tipo !== 'orden' && p.car >= 0.68 && (p.fama || 0) >= 3 && f.O >= 1.2; },
    // Donde su nombre ha llegado, el miedo pesa menos: sube la eficacia percibida de la revuelta.
    mes(m, p) { const f = cab(m, p); if (!f) return; for (const a of m.ase) { const k = H.famaEn(a, p.id); if (k > 0.25 && a.est >= 0 && (f.enem.id < 0 || a.est === f.enem.id)) a.efic = Math.min(0.7, (a.efic || 0) + 0.035 * p.car * k); } },
  });
  Fg('profeta', {
    nom: 'profeta', que: (m, p, d) => 'predica ' + (d.credo || 'una fe nueva') + ' de puerto en puerto',
    apto(m, p) { return p.rol === 'predicador' && p.car >= 0.72 && p.r[R.FE] >= 0.8 && ((p.fama || 0) >= 2 || (cab(m, p) && cab(m, p).tipo === 'orden')); },
    nace(m, p) { const a = P.lugar(m, p); return a >= 0 ? { credo: H.credoNuevo(m, p, a).nom } : null; },
    // Viaja siguiendo el comercio: al puerto con más carga desde donde está y donde su fe aún es pequeña.
    mes(m, p) {
      if (p.en.t !== 'A' || p.credo === undefined) return; const de = p.en.id; const a = m.ase[de]; const c = p.credo; H.creSube(a, c, 0.06 * p.car);
      if (!p.rng.p(0.4)) return; let mejor = -1, pt = 0; for (const f of m.flu.values()) if (f.de === de) { const x = f.q * (1 - H.creEn(m.ase[f.a], c)); if (x > pt) { pt = x; mejor = f.a; } }
      if (mejor < 0 && a.cerca.length) mejor = a.cerca[p.rng.i(0, Math.min(3, a.cerca.length - 1))];
      if (mejor >= 0) S.mas.viajar(m, p, mejor);
    },
  });
  Fg('invicto', {
    nom: 'almirante invicto', que: (m, p) => 'lleva ' + (p.victorias || 0) + ' batallas sin perder ninguna',
    apto(m, p) { return p.cargo && p.cargo.t === 'almirante' && (p.victorias || 0) >= 3 && (p.batallas || 0) === (p.victorias || 0); },
    nace(m, p) { p.r[R.VAL] = Math.max(p.r[R.VAL], 0.86); return null; },      // ya no se retira
  });
  Fg('reformador', {
    nom: 'reformador', que: (m, p) => 'baja el tributo y abre los depósitos de {E' + p.est + '}',
    apto(m, p) { const e = gobierna(m, p); return !!e && p.r[R.EMP] >= 0.68 && p.r[R.PRU] >= 0.4 && (e.agr || 0) > 0.42; },
    // Menos tributo, menos mano dura y grano de la reserva: baja el agravio hoy… y sube lo que la gente espera mañana.
    nace(m, p) { const e = m.est[p.est]; e.trib *= 0.75; e.rep = Math.max(0.1, e.rep * 0.85); for (const a of m.ase) if (a.est === e.id) { const q = a.res[B.grano] * 0.6; a.res[B.grano] -= q; a.alm[B.grano] += q; if (a.expect !== undefined) a.expect += 0.12; for (const c of a.coh) m.coh[c].agr.reg *= 0.88; } return null; },
  });
  Fg('tirano', {
    nom: 'tirano', que: (m, p) => 'gobierna {E' + p.est + '} por el miedo',
    apto(m, p) { const e = gobierna(m, p); if (!e || p.r[R.EMP] > 0.34 || p.r[R.REN] < 0.6) return false; let k = 0; if (p.mue) for (const x of p.mue) if (x.c === 'persona' || x.c === 'pueblo') k++; return k >= 2; },
    nace(m, p) { const e = m.est[p.est]; e.rep = Math.min(1, e.rep + 0.15); return null; },
    // El miedo aprieta, pero cada mes quema un poco de la cohesión del Estado.
    mes(m, p) { const e = gobierna(m, p); if (e && e.asabiya !== undefined) e.asabiya = Math.max(0.05, e.asabiya - 0.004); },
  });
  Fg('genio', {
    nom: 'genio', que: (m, p) => 'sus diseños dan a {E' + p.est + '} una ventaja que nadie más tiene',
    apto(m, p) { return p.est >= 0 && m.est[p.est].vivo && p.hab >= 0.9 && p.r[R.AMB] >= 0.45 && (p.rol === 'jefe_calidad' || p.rol === 'inspector' || p.rol === 'civil' || p.rol === 'aspirante') && !(m.est[p.est].genio >= 0 && m.per[m.est[p.est].genio].vivo); },
    nace(m, p) { const e = m.est[p.est]; e.genio = p.id; e.tec *= 1.08; return null; },
    mes(m, p) { const e = m.est[p.est]; if (e && e.vivo && e.genio === p.id) e.tec *= 1.0025; },
  });
  Fg('unificador', {
    nom: 'unificador', que: (m, p) => 'suma mundos a {E' + p.est + '} y hace que se sientan parte de él',
    apto(m, p) { const e = gobierna(m, p); return !!e && p.car >= 0.55 && (e.conquistas || 0) >= 3; },
    // Lo conquistado deja de sentirse conquistado: el agravio contra «el invasor» se apaga más deprisa.
    mes(m, p) { const e = gobierna(m, p); if (!e) return; for (const a of m.ase) if (a.est === e.id) for (const c of a.coh) m.coh[c].agr.ext *= 0.94; if (e.asabiya !== undefined) e.asabiya = Math.min(1, e.asabiya + 0.003); },
  });
  Fg('sanador', { nom: 'sanador', que: (m, p) => 'cuida a los apestados cuando todos huyen', apto: () => false });
  Fg('martir', { nom: 'mártir', que: (m, p, d) => 'su muerte enciende la calle en ' + (d.n || 0) + ' mundos', apto: () => false });
  S.gancho('asent.dueno', function (m, a, viejo, nuevo) { if (nuevo >= 0 && viejo >= 0 && m.est[nuevo]) m.est[nuevo].conquistas = (m.est[nuevo].conquistas || 0) + 1; });
  // Una vez al año se mira quién ha llegado a ser qué.
  H.revisar = function (m) {
    for (const p of m.per) {
      if (!p.vivo) continue; if (cab(m, p)) p.cabeza = p.fac;                    // quién encabezaba qué, por si luego lo matan
      if (p.fig || !(p.cargo || p.rol !== 'civil' || p.hab >= 0.9)) continue;
      for (const id in S.reg.figura) { const d = S.reg.figura[id]; if (!d.apto(m, p)) continue; const datos = d.nace ? d.nace(m, p) : null; H.nombrar(m, p, id, datos, [p.evCargo]); break; }
    }
  };
  S.gancho('anio', function (m) { H.revisar(m); });
  S.gancho('sismo', function (m) { if (m.figuras) for (const id of m.figuras) { const p = m.per[id]; if (p.vivo && p.fig && S.reg.figura[p.fig].mes) S.reg.figura[p.fig].mes(m, p); } });
  // Mártir: matar en público a alguien cuyo nombre corre por muchos mundos no apaga nada; lo enciende.
  S.gancho('pers.muere', function (m, p, ev, o) {
    const modo = o ? o.modo : 'natural';
    if (p.fig === 'invicto' && p.est >= 0 && m.est[p.est].vivo && m.est[p.est].asabiya !== undefined) m.est[p.est].asabiya = Math.max(0.05, m.est[p.est].asabiya - 0.08);
    if (!(modo === 'ejecucion' || modo === 'publico' || modo === 'purga' || modo === 'silencio') || (p.fama || 0) < 3) return;
    const seguido = p.fig === 'caudillo' || p.fig === 'profeta' || p.fig === 'reformador' || p.fig === 'sanador' || ((cab(m, p) || p.cabeza !== undefined) && p.car >= 0.6); if (!seguido) return;
    let n = 0; for (const a of m.ase) { const k = H.famaEn(a, p.id); if (k < 0.2) continue; n++; a.efic = Math.min(0.7, (a.efic || 0) + 0.22 * k); for (const c of a.coh) { const co = m.coh[c]; co.agr.reg = Math.min(1, co.agr.reg + 0.1 * k * p.car); } if (p.credo !== undefined) H.creSube(a, p.credo, 0.08 * k); }
    p.fig0 = p.fig || null; p.fig = 'martir'; p.figT = m.t; p.martirN = n; (m.figuras = m.figuras || []).push(p.id);
    const E = magn(n, 30), D = 0.9;
    const e2 = m.reg('martirio', fr('ev_martirio', E, D, { martir: '{P' + p.id + '}', n: num(n) }, p.sexo, 'La muerte de {martir} enciende la calle en {n} mundos.'), { a: P.lugar(m, p), imp: n >= 6 ? 3 : 2, c: [ev], d: { p: p.id, n } });
    p.figEv = e2; const lug = P.lugar(m, p); if (lug >= 0) S.inf.crear(m, e2, 'martirio', lug, 0.9, n, { p: p.id });
    const f = p.fac >= 0 ? m.fac[p.fac] : null; if (f && f.vivo) { f.fiestas = f.fiestas || []; f.fiestas.push({ nom: 'Día de ' + p.nom, dia: Math.floor(m.t % S.ANIO) }); p.fiesta = 'Día de ' + p.nom; f.comp = Math.min(1, f.comp + 0.2); }
  });

  // ── 4. Credos: una fe con nombre. En cada asentamiento, x_c es la parte de la gente que la sigue (Σ x ≤ 1).
  // Cada mes: x += β·x·(1 − X)·(0,4 + agravio)  (logística: crece donde hay descontento y hueco)
  //           x += γ·(carga que llega de un puerto con esa fe)·x_origen·(1 − X)  (viaja en las bodegas)
  H.SUST = ['la Senda', 'la Llama', 'el Faro', 'la Regla', 'los Hijos', 'la Mesa', 'el Canto', 'la Vigilia', 'el Pacto', 'la Lámpara', 'los Testigos', 'la Casa', 'el Camino', 'la Orden', 'los Descalzos', 'la Espiga', 'el Umbral', 'la Marea', 'los Pacientes', 'la Voz', 'el Ancla', 'la Ceniza', 'los Despiertos', 'la Rueda', 'el Silencio', 'la Promesa', 'los Últimos', 'la Sal', 'el Retorno', 'la Hora'];
  H.COMPL = ['del Faro Apagado', 'de la Sal', 'del Muelle Vacío', 'de la Semilla', 'del Último Puerto', 'de los Mil Soles', 'del Grano Compartido', 'de la Noche Larga', 'del Hierro Frío', 'de la Estrella Quieta', 'del Pan Partido', 'de las Manos Abiertas', 'del Regreso', 'de la Deriva', 'del Agua Lenta', 'de los Sin Nombre', 'del Núcleo Dormido', 'de la Bodega Llena', 'del Viento Negro', 'de la Primera Cosecha', 'del Ojo Abierto', 'de la Llave Vieja', 'del Polvo', 'de la Madre Ausente', 'del Pozo', 'de los Tres Diques', 'de la Balanza', 'del Hambre Vencida', 'del Sol Bajo', 'de la Ruta Larga'];
  H.COLCRE = ['#ffd166', '#06d6a0', '#ef476f', '#8ecae6', '#c77dff', '#ff9f1c', '#b5e48c', '#f28482'];
  H.creEn = (a, c) => (a.cre ? a.cre[c] || 0 : 0);
  H.creTotal = (a) => { let s = 0; if (a.cre) for (const k in a.cre) s += a.cre[k]; return s; };
  H.creSube = function (a, c, dx) { a.cre = a.cre || {}; const X = H.creTotal(a); a.cre[c] = (a.cre[c] || 0) + Math.max(0, Math.min(dx, 0.97 - X)); };
  H.creMayor = function (a) { let c = -1, v = 0; if (a.cre) for (const k in a.cre) if (a.cre[k] > v) { v = a.cre[k]; c = +k; } return [c, v]; };
  H.credoNuevo = function (m, p, aid) {
    m.cre = m.cre || []; const r = p.rng; const id = m.cre.length; let nom; for (let i = 0; i < 20; i++) { nom = r.el(H.SUST) + ' ' + r.el(H.COMPL); if (!m.cre.some(x => x.nom === nom)) break; }
    const c = { id, nom, fundador: p.id, t0: m.t, sede: aid, col: H.COLCRE[id % H.COLCRE.length], vivo: true, estado: -1, oleadaT: -1e9, fieles: 0 };
    m.cre.push(c); p.credo = id; H.creSube(m.ase[aid], id, 0.12);
    c.ev = m.reg('credo', fr('ev_credo_nace', 0.3, 0.2, { credo: nom, lugar: '{A' + aid + '}', fundador: '{P' + p.id + '}' }, p.sexo, '{fundador} empieza a predicar {credo} en {lugar}.'), { a: aid, imp: 1, d: { p: p.id, credo: id } });
    return c;
  };
  S.gancho('pers.llega', function (m, p, aid) { if (p.vivo && p.credo !== undefined && p.fig === 'profeta') { H.creSube(m.ase[aid], p.credo, 0.05 * p.car); H.oye(m, m.ase[aid], p.id, 0.6); } });
  H.BETA = 0.09; H.GAMMA = 0.008;
  H.techo = (a) => S.clamp(0.12 + 0.75 * a.agr, 0.05, 0.9);   // cuánta gente está dispuesta a creer: más donde hay más agravio
  H.credosMes = function (m) {
    if (!m.cre || !m.cre.length) return;
    for (const a of m.ase) if (a.cre) { const X = H.creTotal(a), K = H.techo(a); for (const k in a.cre) { const x = a.cre[k]; a.cre[k] = Math.max(0, x + H.BETA * x * (1 - X / K) - 0.006 * x); if (a.cre[k] < 1e-4) delete a.cre[k]; } }
    for (const f of m.flu.values()) { const de = m.ase[f.de]; if (!de.cre) continue; const a = m.ase[f.a]; const X = H.creTotal(a); for (const k in de.cre) if (de.cre[k] > 0.05) H.creSube(a, +k, H.GAMMA * Math.min(1, f.q / 4000) * de.cre[k] * Math.max(0, 1 - X / H.techo(a))); }
    for (const c of m.cre) { c.fieles = 0; c.mundos = 0; }
    for (const a of m.ase) if (a.cre) for (const k in a.cre) { const c = m.cre[+k]; c.fieles += a.cre[k] * a.pob; if (a.cre[k] >= 0.25) c.mundos++; }
    for (const a of m.ase) {
      // La fe del pueblo frente a la del trono: si coinciden, legitima; si no, es fe perseguida.
      const [c, x] = H.creMayor(a); if (c < 0 || x < 0.3 || a.est < 0) continue; const e = m.est[a.est]; const gen = m.coh[a.cohGen];
      if (e.credo === c) gen.agr.reg = Math.max(0, gen.agr.reg - 0.006 * x); else if (e.credo !== undefined && e.credo >= 0) gen.agr.reg = Math.min(1, gen.agr.reg + 0.006 * x);
    }
    for (const e of m.est) {
      if (!e.vivo || e.gob < 0) continue; const g2 = m.per[e.gob]; const [c, x] = H.creMayor(m.ase[e.cap]);
      if (c >= 0 && x >= 0.42 && e.credo !== c && g2.r[R.FE] >= 0.55) {
        e.credo = c; m.cre[c].estado = e.id;
        e.evCredo = m.reg('conversion', fr('ev_conversion', magn(e.pob || 0, 2e6), 0.3, { estado: '{E' + e.id + '}', credo: m.cre[c].nom }, 'H', 'Quien gobierna {estado} abraza {credo} y lo hace fe del Estado.'), { a: e.cap, imp: 2, c: [m.cre[c].ev], d: { e: e.id, credo: c } });
        S.inf.crear(m, e.evCredo, 'conversion', e.cap, 0.8, 1, { e: e.id });
      }
    }
    // La oleada: cuando una fe tiene peso en varios mundos que no gobierna, sus fieles acuerdan un día.
    for (const c of m.cre) {
      if (m.t - c.oleadaT < 10 * S.ANIO) continue; const l = m.ase.filter(a => H.creEn(a, c.id) >= 0.3 && a.est >= 0 && m.est[a.est].credo !== c.id && a.agr > 0.3);
      if (l.length >= 5 && new Set(l.map(a => a.est)).size >= 2) { c.oleadaT = m.t; m.prog(m.t + 40, 'oleada.dia', { c: c.id }); }
    }
  };
  S.en('oleada.dia', function (m, x) {
    const c = m.cre[x.c]; const l = m.ase.filter(a => H.creEn(a, c.id) >= 0.3 && a.est >= 0 && m.est[a.est].credo !== c.id); if (l.length < 3) return;
    const ev = m.reg('oleada', fr('ev_oleada', magn(l.length, 25), 0.6, { credo: c.nom, n: num(l.length) }, 'H', 'El día señalado, los fieles de {credo} se alzan a la vez en {n} mundos.'), { a: c.sede, imp: 3, c: [c.ev], d: { credo: c.id, n: l.length } });
    for (const a of l) { const k = H.creEn(a, c.id); a.efic = Math.min(0.7, (a.efic || 0) + 0.45 * k); a.f = Math.min(0.6, a.f + 0.12 * k); for (const co of a.coh) m.coh[co].agr.reg = Math.min(1, m.coh[co].agr.reg + 0.08 * k); a.evOleada = ev; }
    S.inf.crear(m, ev, 'oleada', c.sede, 0.85, l.length, { credo: c.id });
  });
  // Guerra santa: dos tronos con fes distintas, cerca uno de otro, y al menos uno que cree de verdad.
  H.guerraSanta = function (m) {
    if (!m.cre) return;
    for (const e of m.est) {
      if (!e.vivo || e.gob < 0 || e.credo === undefined || e.credo < 0 || e.gue.size) continue; const g2 = m.per[e.gob]; if (g2.r[R.FE] < 0.72) continue;
      for (const j of m.est) {
        if (!j.vivo || j.id === e.id || j.credo === undefined || j.credo < 0 || j.credo === e.credo || e.gue.has(j.id) || j.gue.has(e.id) || (e.alianzas && e.alianzas.has(j.id))) continue;
        if (m.dist[m.ase[e.cap].sis][m.ase[j.cap].sis] > 700 || !e.rng.p(0.25 * g2.r[R.FE])) continue;
        const ev = m.reg('guerra_santa', fr('ev_guerra_santa', magn((e.pob || 0) + (j.pob || 0), 3e6), 0.7, { estado: '{E' + e.id + '}', enemigo: '{E' + j.id + '}', credo: m.cre[e.credo].nom }, 'H', '{estado} declara la guerra santa a {enemigo} en nombre de {credo}.'), { a: e.cap, imp: 3, c: [e.evCredo, j.evCredo], d: { e: e.id, j: j.id } });
        S.pol.declarar(m, e, j.id, { motivo: 'una guerra santa en nombre de ' + m.cre[e.credo].nom, c: ev }); return;
      }
    }
  };
  S.gancho('sismo', function (m) { H.credosMes(m); });
  S.gancho('anio', function (m) { H.guerraSanta(m); });

  // ── 5. Peste. Modelo SIR en cada asentamiento (S + I + R = 1), con pasos de 5 días:
  //    nuevos = β·I·S·dt ;  salen = γ·I·dt ;  de los que salen, muere la fracción μ.  R₀ = β/γ.
  // Viaja con las naves: una tripulación que atraca en un puerto apestado se la lleva al siguiente.
  H.pesteEmpieza = function (m, a, causa) {
    if (m.pes && m.pes.vivo) return -1; const r = rngH(m);
    const pes = { id: (m.pesN = (m.pesN || 0) + 1), t0: m.t, beta: r.r(0.22, 0.4), gamma: 0.1, mort: r.r(0.03, 0.11), vivo: true, muertos: 0, origen: a.id, mundos: 0 };
    m.pes = pes; a.pI = 0.004; a.pR = 0; a.pM = 0;
    pes.ev = m.reg('peste', fr('ev_brote', magn(a.pob, 4e5), 0.5, { lugar: '{A' + a.id + '}' }, 'H', 'Empieza una peste en {lugar}.'), { a: a.id, imp: 2, c: [causa], d: {} });
    S.inf.crear(m, pes.ev, 'peste', a.id, 0.8, 1, {}); m.prog(m.t + 5, 'peste.paso', { id: pes.id });
    return pes.ev;
  };
  S.en('peste.paso', function (m, x) {
    const pes = m.pes; if (!pes || !pes.vivo || pes.id !== x.id) return; const dt = 5; let activos = 0, mundos = 0;
    for (const a of m.ase) {
      if (!(a.pI > 0)) { if (a.pR > 0.05) mundos++; continue; }
      const Su = Math.max(0, 1 - a.pI - a.pR); const cura = a.sanador >= 0 && m.per[a.sanador] && m.per[a.sanador].vivo ? 0.6 : 1;
      const nuevos = Math.min(Su, pes.beta * a.pI * Su * dt * (a.cuar > m.t ? 0.6 : 1)), salen = pes.gamma * a.pI * dt; const mueren = Math.round(salen * pes.mort * cura * a.pob);
      a.pI = Math.max(0, a.pI + nuevos - salen); a.pR += salen; mundos++;
      if (mueren > 0) { const f = Math.max(0.5, 1 - mueren / Math.max(200, a.pob)); for (const c of a.coh) { const co = m.coh[c]; co.n = Math.max(1, Math.round(co.n * f)); } a.pob = Math.max(200, a.pob - mueren); a.pM = (a.pM || 0) + mueren; pes.muertos += mueren; a.culpa.nat += mueren / Math.max(200, a.pob) * 20; }
      // Quien tiene nombre también muere: con la misma probabilidad que cualquiera de su barrio.
      const pm = salen * pes.mort * cura; if (pm > 1e-4) for (const p of m.per) if (p.vivo && p.en.t === 'A' && p.en.id === a.id && p.id !== a.sanador && p.rng.p(pm)) P.matar(m, p, { modo: 'peste', c: [pes.ev], txt: '{P' + p.id + '} muere de la peste en {A' + a.id + '}' });
      // El sanador: quien más siente el dolor ajeno se queda a cuidar. Con él muere menos gente.
      if (a.sanador === undefined && a.pI > 0.02) { a.sanador = -1; let mejor = null; if ((pes.sanadores || 0) < 2 && a.pob > 20000) for (const p of m.per) if (p.vivo && !p.fig && p.en.t === 'A' && p.en.id === a.id && p.r[R.EMP] >= 0.86 && (!mejor || p.r[R.EMP] > mejor.r[R.EMP])) mejor = p; if (mejor) { pes.sanadores = (pes.sanadores || 0) + 1; a.sanador = mejor.id; H.nombrar(m, mejor, 'sanador', null, [pes.ev], a.id); H.oye(m, a, mejor.id, 0.9); } }
      // Cuarentena: la decide quien gobierna, según su prudencia.
      if (!(a.cuar > m.t) && a.pI > 0.03 && !a.cuarHecha) { const gob = S.pol.gobernadorDe(m, a); if (gob && gob.rng.p(0.25 + 0.6 * gob.r[R.PRU])) { a.cuar = m.t + 70; a.cuarHecha = true; m.reg('cuarentena', fr('ev_cuarentena', magn(a.pob, 4e5), 0.4, { lugar: '{A' + a.id + '}' }, 'H', '{lugar} cierra el puerto por la peste: cuarentena.'), { a: a.id, imp: 0, c: [pes.ev], d: { p: gob.id } }); } }
      if (!a.pEv && a.pM > a.pob * 0.03 && a.pM > 300) a.pEv = m.reg('peste_muertos', fr('ev_peste_muertos', magn(a.pM, 5e4), 0.8, { lugar: '{A' + a.id + '}', n: num(a.pM) }, 'H', 'La peste ha matado ya a {n} personas en {lugar}.'), { a: a.id, imp: 2, c: [pes.ev], d: { muertos: Math.round(a.pM) } });
      if (a.pI < 2e-4) a.pI = 0; else activos++;
    }
    pes.mundos = mundos;
    if (activos) { m.prog(m.t + dt, 'peste.paso', x); return; }
    pes.vivo = false; pes.t1 = m.t;
    m.reg('peste_fin', fr('ev_peste_fin', magn(pes.muertos, 3e5), 0.7, { n: num(pes.muertos) }, 'H', 'La peste se apaga; deja {n} muertos en total.'), { a: pes.origen, imp: pes.muertos > 2e4 ? 3 : 2, c: [pes.ev], d: { muertos: Math.round(pes.muertos) } });
    for (const a of m.ase) { a.pI = 0; a.pR = 0; a.pM = 0; a.pEv = 0; a.cuarHecha = false; a.sanador = undefined; }
  });
  S.gancho('nave.atraca', function (m, n, a) {
    const pes = m.pes; if (!pes || !pes.vivo) return;
    if (n.peste === pes.id && m.t - n.pesteT < 45 && !(a.pI > 0) && !(a.pR > 0.2)) { if (!(a.cuar > m.t) || n.rng.p(0.15)) { a.pI = 0.003; a.pR = a.pR || 0; a.pM = 0; } }
    if (a.pI > 0.01 && n.rng.p(Math.min(1, 8 * a.pI))) { n.peste = pes.id; n.pesteT = m.t; }
  });
  // Empieza donde hay hambre y mucho trasiego: riesgo anual = 0,02 + 0,5·(hambre media).
  H.pesteAzar = function (m) {
    if (m.pes && (m.pes.vivo || m.t - m.pes.t1 < 12 * S.ANIO)) return; let h = 0, pob = 0; for (const a of m.ase) { h += a.H * a.pob; pob += a.pob; }
    if (!rngH(m).p(0.02 + 0.5 * h / Math.max(1, pob))) return;
    let mejor = null, pt = -1; for (const a of m.ase) { const x = (a.H + 0.05) * Math.log10(10 + a.pob) * (1 + (m.sis[a.sis].traf || 0) * 0.1); if (x > pt) { pt = x; mejor = a; } }
    if (mejor) H.pesteEmpieza(m, mejor, mejor.hambruna);
  };

  // Almacén de núcleos en cadena: si revienta uno donde hay muchos guardados, pueden ir detrás. P = 1 − e^(−n/14).
  S.gancho('explosion.nucleo', function (m, o, ev, a) {
    if (!a || !a.nuc || a.nuc.length < 6 || a.cadena > m.t - 360) return; const r = a.rng; if (!r.p(1 - Math.exp(-a.nuc.length / 14))) return;
    a.cadena = m.t; let k = 0; const quedan = []; for (const id of a.nuc) { const nu = m.obj[id]; if (nu.vivo && r.p(0.6)) { nu.vivo = false; if (nu.falla) nu.falla.x = true; k++; } else quedan.push(id); } a.nuc = quedan; if (!k) return;
    const muertos = Math.round(Math.min(a.pob * 0.3, k * a.pob * 0.006 + k * 40)); const f = Math.max(0.5, 1 - muertos / Math.max(200, a.pob));
    for (const c of a.coh) { const co = m.coh[c]; co.n = Math.max(1, Math.round(co.n * f)); } a.pob = Math.max(200, a.pob - muertos);
    const e2 = m.reg('arsenal', fr('ev_arsenal', magn(muertos, 5e4), 0.9, { lugar: '{A' + a.id + '}', n: num(muertos) }, 'H', 'Revienta en cadena el almacén de núcleos de {lugar}: {n} muertos.'), { a: a.id, imp: 3, c: [ev], d: { muertos, nucleos: k } });
    for (const id of a.ins) { const i = m.ins[id]; if (!S.reg.inst[i.tipo].indestructible && r.p(0.45)) i.salud = Math.max(0, i.salud - r.r(0.3, 0.8)); }
    S.soc.hecho(m, a, e2, 'explosion', 1, muertos); S.inf.crear(m, e2, 'arsenal', a.id, 0.95, muertos, {});
  });
  // Llamarada: cada estrella tiene su genio; las inquietas se llevan más riesgo.
  H.llamarada = function (m) {
    const r = rngH(m); if (!r.p(0.035)) return; let mejor = null, pt = -1; for (const s of m.sis) { if (!s.ase.length || s.bloqueo > m.t) continue; const act = (S.hash(m.semilla, 'astro', s.id) % 1000) / 1000; const x = act * act * r.f(); if (x > pt) { pt = x; mejor = s; } } if (!mejor) return;
    let huyen = 0; const ev = m.reg('llamarada', '…', { s: mejor.id, a: mejor.ase[0], imp: 3, d: {} }); mejor.bloqueo = m.t + 120;
    for (const id of mejor.ase) { const a = m.ase[id]; const k = Math.round(a.pob * 0.35); const dest = a.cerca.map(i => m.ase[i]).filter(d => d.sis !== mejor.id).slice(0, 4); if (!dest.length) continue; a.pob -= k; huyen += k; for (const c of a.coh) { const co = m.coh[c]; co.n = Math.max(1, Math.round(co.n * 0.65)); } for (const d of dest) { d.pob += k / dest.length; d.refEnt = (d.refEnt || 0) + k / dest.length; } for (const ii of a.ins) { const i = m.ins[ii]; if (!S.reg.inst[i.tipo].indestructible && r.p(0.5)) i.salud = Math.max(0.1, i.salud - 0.3); } S.soc.hecho(m, a, ev, 'explosion', 0.9, k); }
    m.ev[ev].txt = fr('ev_llamarada', magn(huyen, 6e5), 0.7, { sistema: '{S' + mejor.id + '}', n: num(huyen) }, 'H', 'Una llamarada de la estrella de {sistema} obliga a evacuar: {n} personas huyen.'); m.ev[ev].d.huyen = huyen;
    if (mejor.ase.length) S.inf.crear(m, ev, 'llamarada', mejor.ase[0], 0.95, huyen, {});
  };
  // Hallazgo: en los campos de restos de las batallas viejas a veces hay algo más antiguo que las batallas.
  H.hallazgo = function (m) {
    const r = rngH(m); const l = m.sis.filter(s => (s.restos || 0) + s.pecios.length > 30 && s.ase.length && !s.hallazgo); if (!l.length || !r.p(0.05)) return;
    const s = r.el(l); const a = m.ase[s.ase[0]]; if (a.est < 0) return; const e = m.est[a.est]; s.hallazgo = true; e.tec *= 1.18;
    const ev = m.reg('hallazgo', fr('ev_hallazgo', magn(e.pob || 1e5, 2e6), 0.2, { lugar: '{A' + a.id + '}', estado: '{E' + e.id + '}' }, 'H', 'En {lugar} aparece una máquina antigua; {estado} se la queda y sus ingenieros aprenden de ella.'), { a: a.id, imp: 3, d: { e: e.id } });
    S.inf.crear(m, ev, 'hallazgo', a.id, 0.7, 1, { e: e.id });
  };
  S.gancho('anio', function (m) { H.pesteAzar(m); H.llamarada(m); H.hallazgo(m); });
  S.sismografo('credos', 'Fieles de algún credo (parte de la población)', m => { let f = 0, pob = 0; for (const a of m.ase) { f += H.creTotal(a) * a.pob; pob += a.pob; } return pob ? f / pob : 0; });
  S.sismografo('peste', 'Apestados (parte de la población)', m => { let f = 0, pob = 0; for (const a of m.ase) { f += (a.pI || 0) * a.pob; pob += a.pob; } return pob ? f / pob : 0; });
  S.sismografo('figuras', 'Figuras históricas vivas', m => { let k = 0; if (m.figuras) for (const id of m.figuras) if (m.per[id].vivo) k++; return k; });

  // ── 6. Épocas. Cada año es un vector de intensidades por tema; la historia se corta donde ese vector cambia.
  // Corte óptimo por programación dinámica: se minimiza la suma de varianzas dentro de cada tramo más un coste por tramo.
  H.TEMAS_EPOCA = { guerra: ['guerra', 'guerra_civil', 'batalla', 'batalla_fin', 'conquista', 'desembarco', 'guerra_santa'], revuelta: ['revolucion', 'insurreccion', 'insurreccion_aplastada', 'masacre', 'motin_pan', 'oleada', 'martirio', 'golpe', 'independencia'], hambre: ['hambruna', 'plaga', 'acaparamiento'], peste: ['peste', 'peste_muertos', 'peste_fin', 'cuarentena', 'epidemia'] };
  H.epocas = function (m) {
    if (m._epo && m._epo.n === m.ev.length) return m._epo.l;
    const a0 = Math.floor(m.ev.length ? m.ev[0].t / S.ANIO : 0), a1 = Math.floor(m.t / S.ANIO); const n = a1 - a0 + 1; const temas = Object.keys(H.TEMAS_EPOCA); const K = temas.length; const de = {}; temas.forEach((t, k) => { for (const x of H.TEMAS_EPOCA[t]) de[x] = k; });
    const X = []; for (let i = 0; i < n; i++) X.push(new Float64Array(K)); const inf = H.influencia(m);
    for (const e of m.ev) { const k = de[e.k]; if (k !== undefined) X[Math.min(n - 1, Math.floor(e.t / S.ANIO) - a0)][k] += H.PESO[e.imp] + (e.d && e.d.muertos > 0 ? Math.log10(1 + e.d.muertos) : 0); }
    // Normaliza cada tema por su máximo (y raíz, para que un año atroz no borre los demás).
    const mx = new Float64Array(K); for (const x of X) for (let k = 0; k < K; k++) if (x[k] > mx[k]) mx[k] = x[k]; for (const x of X) for (let k = 0; k < K; k++) x[k] = mx[k] > 0 ? Math.sqrt(x[k] / mx[k]) : 0;
    const MINL = 3; const out = [];
    if (n < 2 * MINL) out.push([0, n - 1]);
    else {
      const pre = [new Float64Array(K)], pre2 = [0]; for (let i = 0; i < n; i++) { const s = new Float64Array(K); let q = pre2[i]; for (let k = 0; k < K; k++) { s[k] = pre[i][k] + X[i][k]; q += X[i][k] * X[i][k]; } pre.push(s); pre2.push(q); }
      const coste = (i, j) => { let c = pre2[j + 1] - pre2[i]; const L = j - i + 1; for (let k = 0; k < K; k++) { const s = pre[j + 1][k] - pre[i][k]; c -= s * s / L; } return c; };
      const pena = Math.max(0.25, coste(0, n - 1) / Math.max(3, n / 4)); const best = new Float64Array(n + 1).fill(Infinity), ant = new Int32Array(n + 1).fill(-1); best[0] = 0;
      for (let j = MINL; j <= n; j++) for (let i = 0; i <= j - MINL; i++) { if (best[i] === Infinity) continue; const c = best[i] + coste(i, j - 1) + pena; if (c < best[j]) { best[j] = c; ant[j] = i; } }
      let j = n; const cortes = []; while (j > 0 && ant[j] >= 0) { cortes.push([ant[j], j - 1]); j = ant[j]; } if (!cortes.length) cortes.push([0, n - 1]); cortes.reverse(); for (const c of cortes) out.push(c);
    }
    const pes = H.pesos(m); const l = [];
    for (const [i, j] of out) {
      const med = new Float64Array(K); for (let y = i; y <= j; y++) for (let k = 0; k < K; k++) med[k] += X[y][k] / (j - i + 1);
      let kk = 0, altos = 0, tot = 0; for (let k = 0; k < K; k++) { if (med[k] > med[kk]) kk = k; if (med[k] > 0.3) altos++; tot += med[k]; }
      let tema = tot < 0.25 ? 'paz' : altos >= 3 ? 'caos' : temas[kk]; const t0 = (a0 + i) * S.ANIO, t1 = (a0 + j + 1) * S.ANIO;
      // ¿La marca una persona? Quien más influencia causal concentra en esos años, si pasa de un tercio del total.
      let total = 0; const porP = new Map(); for (const e of m.ev) { if (e.t < t0 || e.t >= t1) continue; total += inf[e.id]; const s = sujeto(e); if (s >= 0) porP.set(s, (porP.get(s) || 0) + inf[e.id]); }
      let fig = -1, fv = 0; for (const [id, v] of porP) if (v > fv) { fv = v; fig = id; }
      let clave = ''; if (fig >= 0 && fv > total * 0.33 && pes.pos[fig] < 12) { tema = 'figura'; clave = m.per[fig].nom; }
      else if (tema === 'guerra') { const cu = new Map(); for (const e of m.ev) if (e.t >= t0 && e.t < t1 && de[e.k] === 0 && e.d) for (const id of [e.d.e, e.d.j, e.d.gana, e.d.pierde]) if (id >= 0 && m.est[id]) cu.set(id, (cu.get(id) || 0) + 1); let me = -1, mv = 0; for (const [id, v] of cu) if (v > mv) { mv = v; me = id; } clave = me >= 0 ? m.ase[m.est[me].cap].nom : 'los Mundos'; }
      const E = S.clamp(tot / 1.6, 0, 1), D = S.clamp((med[0] + med[1] + med[2] * 0.8 + med[3]) / 1.5, 0, 1);
      const RES = { guerra: 'La Guerra de ' + clave, hambre: 'Los Años del Hambre', revuelta: 'Los Años de la Calle', paz: 'La Calma', peste: 'Los Años de la Peste', figura: 'El Tiempo de ' + clave, caos: 'Los Años Revueltos' };
      l.push({ a0: a0 + i, a1: a0 + j, tema, clave, fig: tema === 'figura' ? fig : -1, E, D, nom: fr('epoca_' + tema, E, D, { clave }, 'H', RES[tema]), med: Array.from(med) });
    }
    m._epo = { n: m.ev.length, l }; return l;
  };

  // ── 7. Biografía. Párrafos hechos de frases del banco; la casilla la ponen la épica y el drama de esa vida.
  const minus = (s) => { s = s.replace(/[.\s]+$/, ''); return s.charAt(0).toLowerCase() + s.slice(1); };
  const lista = (l) => (l.length <= 1 ? l.join('') : l.slice(0, -1).join(', ') + ' y ' + l[l.length - 1]);
  const RASGOS = [['codicia', 0, 'f'], ['valor', 1, 'm'], ['lealtad', 2, 'f'], ['empatia', 3, 'f'], ['rencor', 4, 'm'], ['ambicion', 5, 'f'], ['fe', 6, 'f'], ['prudencia', 7, 'f']];
  const CARGO = { gobernante: 'cargo_gobernante', gobernador: 'cargo_gobernador', almirante: 'cargo_almirante', general: 'cargo_general', ministro: 'cargo_ministro', arzobispo: 'cargo_arzobispo', jefe_guardia: 'cargo_jefe_guardia', lider: 'cargo_cabecilla' };
  const SALTA = { muerte: 1, batalla: 1, batalla_fin: 1, figura: 1 };
  H.biografia = function (m, p) {
    const E = H.epica(m, p), D = H.drama(m, p), sx = p.sexo; const nivel = H.nivel(m, p); const inf = H.influencia(m);
    const base = { nom: p.nom, pila: p.nom.split(' ')[0] }; const usos = {};
    // Si un hueco se repite en la misma vida (dos batallas, dos masacres), cada vez toma una casilla vecina: no hay dos frases iguales.
    const f = (slot, d, res) => { const k = usos[slot] = (usos[slot] === undefined ? 0 : usos[slot] + 1); const datos = Object.assign({}, base, d || {}); return F && F.hay(slot) ? F.vez(slot, E, D, k, datos, sx) : F ? F.rellena(res, datos, sx) : res; };
    const sec = []; const A = (id) => '{A' + id + '}', Pn = (id) => '{P' + id + '}';
    // Origen y retrato.
    const an = Math.floor(p.nace / S.ANIO); let o = [f('nace', { lugar: A(p.casa), anio: an >= 0 ? an : (-an) + ' antes del Cómputo' }, 'Nació en {lugar} el año {anio}.')];
    if (E >= 0.25) o.push(p.fam.padres.length >= 2 ? f('nace_padres', { padre: Pn(p.fam.padres[0]), madre: Pn(p.fam.padres[1]) }, 'Fue hij{o|a} de {padre} y de {madre}.') : f('nace_sin_padres', null, 'De sus padres no se sabe nada.'));
    sec.push({ t: 'Origen', p: [o.join(' ')] });
    const rasgos = RASGOS.map(([n2, i, gen]) => [Math.abs(p.r[i] - 0.5), 'rasgo_' + n2 + (p.r[i] >= 0.5 ? (gen === 'f' ? '_alta' : '_alto') : (gen === 'f' ? '_baja' : '_bajo')), S.RNOM[i] + (p.r[i] >= 0.5 ? ': mucha' : ': poca')]);
    rasgos.push([Math.abs(p.car - 0.45), 'rasgo_carisma_' + (p.car >= 0.45 ? 'alto' : 'bajo'), 'carisma'], [Math.abs(p.hab - 0.5), 'rasgo_habilidad_' + (p.hab >= 0.5 ? 'alta' : 'baja'), 'habilidad']);
    rasgos.sort((x, y) => y[0] - x[0]); const car = rasgos.slice(0, 2 + Math.round(E * 2)).filter(x => x[0] > 0.12).map(x => f(x[1], null, S.cap(x[2]) + '.'));
    if (Math.abs(p.ideo[1]) > 0.5) car.push(f(p.ideo[1] > 0 ? 'ideas_orden' : 'ideas_cambio', null, p.ideo[1] > 0 ? 'Creía en el orden.' : 'Creía que todo debía cambiar.'));
    const fam = []; if (p.fam.con >= 0) { const c = m.per[p.fam.con]; fam.push(!c.vivo && (p.vivo || c.muere < p.muere) ? f('familia_viudez', { pareja: Pn(c.id) }, 'Su pareja, {pareja}, murió antes.') : f('familia_pareja', { pareja: Pn(c.id) }, 'Su pareja fue {pareja}.')); }
    if (p.fam.hijos.length === 1) { const h = m.per[p.fam.hijos[0]]; fam.push((h.sexo === 'M' ? 'Tuvo una hija, ' : 'Tuvo un hijo, ') + Pn(h.id) + '.'); } else if (p.fam.hijos.length) fam.push(f('familia_hijos', { n: p.fam.hijos.length, hijos: lista(p.fam.hijos.map(Pn)) }, 'Tuvo {n} hijos: {hijos}.')); else if (p.fam.con < 0 && E >= 0.25) fam.push(f('familia_sola', null, 'No se le conoce familia.'));
    const fama = p.fama || 0; if (fama >= 2) fam.push(f('fama_extendida', { n: fama }, 'Su nombre se conoce en {n} mundos.')); else if (E < 0.5) fam.push(f('fama_ninguna', null, 'Fuera de su casa nadie supo su nombre.'));
    if (car.length) sec.push({ t: 'Carácter', p: [car.join(' ')] }); if (fam.length) sec.push({ t: 'Los suyos', p: [fam.join(' ')] });
    // Vida: el cargo y los hechos que más pesaron (por influencia causal), en orden.
    const vida = []; const cg = p.cargo;
    if (cg && CARGO[cg.t]) vida.push([p.evCargo >= 0 && m.ev[p.evCargo] ? m.ev[p.evCargo].t : p.nace + 25 * S.ANIO, f(CARGO[cg.t], { estado: '{E' + p.est + '}', lugar: A(cg.id !== undefined && cg.t === 'gobernador' ? cg.id : p.casa), flota: cg.t === 'almirante' && m.flo[cg.id] ? m.flo[cg.id].nom : 'flota', faccion: cg.t === 'lider' && m.fac[cg.id] ? '{F' + cg.id + '}' : 'los suyos' }, S.cap(cg.nom || cg.t) + '.')]);
    else if (p.rol === 'pirata' || p.rol === 'corsario' || p.rol === 'capitan') vida.push([p.nace + 25 * S.ANIO, f(p.rol === 'pirata' ? 'cargo_pirata' : p.rol === 'corsario' ? 'cargo_corsario' : 'cargo_capitan', { nave: p.en.t === 'N' && m.nav[p.en.id] ? '{N' + p.en.id + '}' : 'su nave', estado: p.est >= 0 ? '{E' + p.est + '}' : 'nadie' }, 'Fue capitán.')]);
    else if (p.rol === 'predicador') vida.push([p.nace + 25 * S.ANIO, f('cargo_predicador', null, 'Fue predicador{|a}.')]);
    else if (p.rol === 'mercader' && p.fac >= 0) vida.push([p.nace + 25 * S.ANIO, f('cargo_mercader', { faccion: '{F' + p.fac + '}' }, 'Fue cabeza de {faccion}.')]);
    else if (p.rol === 'inspector' || p.rol === 'comisario' || p.rol === 'jefe_calidad') vida.push([p.nace + 25 * S.ANIO, f('cargo_funcionario', { oficio: cg && cg.nom ? cg.nom : p.rol.replace('_', ' '), lugar: A(p.casa) }, 'Fue {oficio} en {lugar}.')]);
    else vida.push([p.nace + 20 * S.ANIO, f('cargo_ninguno', { lugar: A(p.casa) }, 'Una persona corriente de {lugar}.')]);
    // Hechos sacados de la cuenta de muertes (batallas, asaltos, masacres, ejecuciones).
    let purgas = 0, abord = 0; const porClase = {}; const pon = (cl, h) => { (porClase[cl] = porClase[cl] || []).push(h); };
    if (p.mue) for (const x of p.mue) {
      if (x.c === 'batalla') { const bt = m.bat[x.bt]; const yo = bt.L[x.lado], el = bt.L[1 - x.lado]; pon('bat' + (bt.gano === x.lado), [x.t, bt.gano === x.lado ? f('hecho_batalla_gana', { sistema: '{S' + bt.sis + '}', enemigo: '{E' + el.est + '}', n: num(x.n) }, 'Ganó la batalla de {sistema} contra {enemigo}.') : f('hecho_batalla_pierde', { sistema: '{S' + bt.sis + '}', enemigo: '{E' + el.est + '}' }, 'Perdió la batalla de {sistema} contra {enemigo}.'), inf[bt.ev] + 6]); void yo; }
      else if (x.c === 'tierra') { const t = m.tie[x.tie]; const asalto = t.tipo === 'asalto'; const gano = t.gano === x.lado; const slot = x.lado === 0 ? (asalto ? (gano ? 'hecho_conquista' : null) : (gano ? 'hecho_revolucion' : 'hecho_insurreccion_falla')) : (gano ? (asalto ? 'hecho_defensa' : 'hecho_reprime') : null); if (slot) pon(slot, [x.t, f(slot, { lugar: A(t.a) }, 'Combatió en {lugar}.'), 5 + x.n / 200 + (t.ev >= 0 ? inf[t.ev] : 0)]); }
      else if (x.c === 'pueblo' && x.hecho === 'masacre') pon('masacre', [x.t, f('hecho_masacre', { lugar: A(x.a), n: num(x.n) }, 'Mandó disparar contra la gente en {lugar}: {n} muertos.'), 6 + (x.ev >= 0 ? inf[x.ev] : 0)]);
      else if (x.c === 'persona') { if (x.modo === 'purga') purgas++; else if (x.modo === 'ejecucion') pon('ejecuta', [x.t, f('hecho_ejecuta', { victima: Pn(x.v) }, 'Mandó ejecutar a {victima}.'), 3 + (x.ev >= 0 ? inf[x.ev] : 0)]); else if (x.modo === 'venganza') vida.push([x.t, f('hecho_venganza_cumple', { victima: Pn(x.v) }, 'Encontró y mató a {victima}.'), 6]); }
      else if (x.c === 'nave') abord++;
    }
    for (const cl in porClase) for (const h of porClase[cl].sort((x, y) => (y[2] || 0) - (x[2] || 0)).slice(0, 2)) vida.push(h);
    if (purgas) vida.push([p.mue.find(x => x.modo === 'purga').t, f('hecho_purga', { n: purgas }, 'Purgó a {n} rivales.'), 7]);
    if (abord >= 2) vida.push([p.mue.find(x => x.c === 'nave').t, f('hecho_abordajes', { n: abord }, 'Destruyó {n} naves.'), 4]);
    // Y de su expediente: los hechos con forma conocida van con su frase; los demás, tal como quedaron escritos.
    const exp = m.bio && m.bio.get(p.id) ? m.bio.get(p.id).map(i => m.ev[i]) : []; const visto = {};
    const propios = { motin_pan: ['hecho_abre_deposito', (e) => ({ lugar: A(e.a) })], golpe: ['hecho_golpe', () => ({ estado: '{E' + p.est + '}' })], profecia_cumplida: ['hecho_profecia', () => ({})], corrupcion: ['hecho_corrupcion', () => ({})], rencor: ['hecho_venganza_jura', (e) => ({ victima: p.ven ? Pn(p.ven.victima) : 'los suyos' })], conector: ['hecho_funda', (e) => ({ faccion: p.fac >= 0 ? '{F' + p.fac + '}' : 'una hermandad', lugar: A(e.a) })], atentado: ['hecho_atentado_sufre', () => ({})], informe: ['hecho_informe_miente', (e) => ({ lugar: A(e.a) })] };
    for (const e of exp) { const pr = propios[e.k]; if (!pr || visto[e.k] || (e.k !== 'atentado' && sujeto(e) !== p.id) || (e.k === 'atentado' && sujeto(e) === p.id)) continue; visto[e.k] = 1; vida.push([e.t, f(pr[0], pr[1](e), S.cap(minus(m.texto(e))) + '.'), 2 + inf[e.id]]); }
    const usados = new Set(); const libres = exp.filter(e => e.imp >= 1 && !propios[e.k] && !SALTA[e.k]).sort((x, y) => inf[y.id] - inf[x.id]).slice(0, 2 + Math.round(E * 8));
    for (const e of libres) { usados.add(e.id); vida.push([e.t, S.cap(minus(e.txt)) + '.', inf[e.id]]); }
    const cab0 = vida[0]; const resto = vida.slice(1).sort((x, y) => (y[2] || 0) - (x[2] || 0)).slice(0, 3 + Math.round(E * 10)).sort((x, y) => x[0] - y[0]);
    const parr = [cab0[1]]; let tAnt = null; for (const h of resto) { const gap = tAnt === null ? 0 : (h[0] - tAnt) / S.ANIO; if (gap >= 3) parr.push(f('con_anios_despues', { n: Math.round(gap) }, 'Pasaron {n} años.') + ' ' + h[1]); else parr[parr.length - 1] += ' ' + h[1]; tAnt = h[0]; }
    let txt = '';
    if (p.fig && S.reg.figura[p.fig]) { const fg = p.fig === 'martir' && p.fig0 ? p.fig0 : p.fig; const e = p.est >= 0 ? '{E' + p.est + '}' : 'los suyos'; const cr = p.credo !== undefined && m.cre ? m.cre[p.credo] : null; if (fg !== 'martir') txt = f('figura_' + fg, { faccion: p.fac >= 0 ? '{F' + p.fac + '}' : 'su gente', n: fg === 'invicto' ? (p.victorias || 0) : fg === 'tirano' ? purgas : fg === 'unificador' ? (p.est >= 0 ? m.est[p.est].conquistas || 0 : 0) : fg === 'profeta' && cr ? cr.mundos || 1 : fama, credo: cr ? cr.nom : 'su fe', estado: e, lugar: A(P.lugar(m, p) >= 0 ? P.lugar(m, p) : p.casa) }, S.cap(S.reg.figura[fg].nom) + '.'); }
    if (txt) parr.push(txt); sec.push({ t: 'Vida', p: parr });
    // Muerte (con la cadena de causas) o presente.
    if (!p.vivo) {
      const ev = p.evMuerte >= 0 ? m.ev[p.evMuerte] : null; const d = ev && ev.d ? ev.d : {}; const edad = Math.floor((p.muere - p.nace) / S.ANIO); const lug = ev && ev.a >= 0 ? A(ev.a) : A(p.casa); const verd = d.por >= 0 ? Pn(d.por) : 'alguien cuyo nombre no quedó';
      const lin = ev && /multitud/.test(ev.txt); const nave = ev && /\{N(\d+)\}/.exec(ev.txt);
      const slot = d.modo === 'natural' || !d.modo ? 'muerte_natural' : d.modo === 'combate' ? (nave ? 'muerte_nave' : 'muerte_combate') : d.modo === 'ejecucion' ? 'muerte_ejecucion' : d.modo === 'purga' ? 'muerte_purga' : d.modo === 'publico' ? (lin ? 'muerte_linchamiento' : 'muerte_atentado') : d.modo === 'silencio' ? 'muerte_silencio' : d.modo === 'venganza' ? 'muerte_venganza' : d.modo === 'catador' ? 'muerte_veneno' : d.modo === 'explosion' ? 'muerte_explosion' : d.modo === 'peste' ? 'muerte_peste' : 'muerte_natural';
      const mu = [f(slot, { edad, lugar: lug, verdugo: verd, nave: nave ? '{N' + nave[1] + '}' : 'su nave' }, ev ? S.cap(minus(ev.txt)) + '.' : 'Murió.')];
      if (ev && slot !== 'muerte_natural') {
        const cad = []; let c = ev.c.length ? m.ev[ev.c[0]] : null; for (let k = 0; c && cad.length < (E >= 0.5 ? 3 : E >= 0.25 ? 2 : 1) && k < 8; k++) { if (!SALTA[c.k]) cad.push(c); c = c.c.length ? m.ev[c.c[0]] : null; }
        cad.forEach((c2, i) => mu.push(f('cadena_' + (i + 1), { hecho: minus(c2.txt) }, ['Fue porque {hecho}.', 'Y aquello venía de que {hecho}.', 'En el origen: {hecho}.'][i])));
        if (!cad.length) mu.push(f('cadena_mano', null, 'Nadie supo nunca por qué.'));
        mu.push(f(edad < 38 ? 'edad_joven' : edad < 62 ? 'edad_plena' : 'edad_vieja', { edad }, 'Tenía {edad} años.'));
      }
      sec.push({ t: 'Muerte', p: [mu.join(' ')] });
      // Legado: lo que colgó de su muerte, y lo que queda.
      const le = []; const ef = []; if (ev) { let niv = [ev.id]; for (let dd = 0; dd < 3; dd++) { const sig = []; for (const x of niv) for (const y of (m.efe[x] || [])) { ef.push(m.ev[y]); sig.push(y); } niv = sig; } }
      const suc = ef.find(e => e.k === 'sucesion'); const caos = ef.find(e => e.k === 'interregno' || e.k === 'guerra_civil' || e.k === 'fragmentacion' || e.k === 'estado_cae');
      if (p.fig === 'martir') le.push(f('legado_martir', { n: p.martirN || 0 }, 'Su muerte encendió la calle en {n} mundos.'));
      if (caos) le.push(f('legado_caos', { estado: '{E' + p.est + '}' }, 'Su muerte dejó {estado} sin cabeza.')); else if (suc) { const h = sujeto(suc); le.push(f('legado_sucesion', { heredero: h >= 0 && h !== p.id ? Pn(h) : 'quien pudo' }, 'L{o|a} sucedió {heredero}.')); }
      if (p.fiesta) le.push(f('legado_fiesta', { fiesta: p.fiesta }, 'Se le recuerda con una fiesta: {fiesta}.'));
      if (p.mue) { const x = p.mue.find(y => y.c === 'pueblo' && y.ev >= 0 && m.ase[y.a].ins.some(i => m.ins[i].tipo === 'memorial' && m.ins[i].ev === y.ev)); if (x) le.push(f('legado_memorial', { lugar: A(x.a) }, 'Hay un memorial en {lugar}.')); }
      const ven = ef.find(e => e.k === 'rencor'); if (ven && sujeto(ven) >= 0) le.push(f('legado_venganza', { vengador: Pn(sujeto(ven)) }, '{vengador} juró vengarl{o|a}.'));
      const vivos = p.fam.hijos.filter(h => m.per[h].vivo); if (vivos.length) le.push(f('legado_hijos', { hijos: lista(vivos.map(Pn)) }, 'L{o|a} sobreviven sus hijos: {hijos}.'));
      const ep = nivel >= 3 ? H.epocas(m).find(x => x.fig === p.id) : null; if (ep) le.push(f('legado_epoca', { epoca: '«' + ep.nom + '»' }, 'Una época lleva su marca: {epoca}.'));
      if (!le.length) le.push(f('legado_nada', null, 'Nada cambió con su muerte.'));
      if ((p.mando || 0) > 0) le.push(f('cuenta_muertes', { n: num(p.mando) }, '{n} personas murieron bajo su mando.')); if ((p.bajas || 0) > 0) le.push(f('cuenta_manos', { n: num(p.bajas) }, 'Mató a {n} con sus manos.'));
      le.push(f('cierre', null, 'No hay más que contar.')); sec.push({ t: 'Lo que dejó', p: [le.join(' ')] });
    } else {
      const lug = P.lugar(m, p); const ah = [p.rol === 'preso' ? f('vive_preso', { lugar: A(lug >= 0 ? lug : p.casa) }, 'Sigue viv{o|a}, pres{o|a} en {lugar}.') : f('vive', { edad: Math.floor(P.edad(m, p)), lugar: A(lug >= 0 ? lug : p.casa) }, 'Sigue viv{o|a}: tiene {edad} años y está en {lugar}.')];
      if ((p.mando || 0) > 0) ah.push(f('cuenta_muertes', { n: num(p.mando) }, '{n} personas han muerto bajo su mando.')); if ((p.bajas || 0) > 0) ah.push(f('cuenta_manos', { n: num(p.bajas) }, 'Ha matado a {n} con sus manos.'));
      sec.push({ t: 'Hoy', p: [ah.join(' ')] });
    }
    return { E, D, nivel, nivelNom: H.NIVELES[nivel], epiteto: H.epiteto(m, p), fig: p.fig || null, peso: H.pesos(m).w[p.id], puesto: H.pesos(m).pos[p.id] + 1, secciones: sec };
  };
  // El texto entero de una biografía, con los nombres ya puestos (sin fichas): para leerlo fuera de la interfaz.
  H.biografiaTexto = function (m, p) { const b = H.biografia(m, p); return b.secciones.map(s => s.t.toUpperCase() + '\n' + s.p.map(x => m.texto({ txt: x })).join('\n')).join('\n\n'); };
})(typeof globalThis !== 'undefined' ? globalThis : this);
