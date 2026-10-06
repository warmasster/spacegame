// Teoría política, sociológica y geopolítica aplicada. Cada teoría se declara en un registro (para que la
// interfaz pueda decir qué es, de quién y dónde actúa) y su mecánica vive aquí o en el módulo que se cita.
(function (g) {
  'use strict';
  const S = g.SIM; const R = S.R; const B = S.B;
  const Th = S.teo = {};
  const T = (id, o) => S.def('teoria', id, o);
  const pc = (x) => Math.round(x * 100) + ' %';
  const media = (m, f) => { let s = 0, k = 0; for (const e of m.est) if (e.vivo) { s += f(e); k++; } return k ? s / k : 0; };

  // ════ Sociología de la revuelta ════
  T('umbrales', { campo: 'Revuelta', nom: 'Modelo de umbrales', autor: 'Mark Granovetter (1978)', que: 'Cada persona sale a la calle cuando ve fuera a suficiente gente. La revuelta no crece: salta.', aqui: 'Cada asentamiento itera f′ = fracción con umbral ≤ f. El umbral baja con el agravio y sube con la represión percibida. Hay histéresis.', ind: (m) => { let k = 0; for (const a of m.ase) if (a.f > 0.3) k++; return k + ' asentamientos con más del 30 % en la calle'; } });
  T('falsificacion', { campo: 'Revuelta', nom: 'Falsificación de preferencias', autor: 'Timur Kuran (1995)', que: 'La gente aplaude al tirano mientras lo odia; nadie sabe cuántos piensan igual, y por eso las revoluciones pillan por sorpresa.', aqui: 'Los retratos sin ojos son la única señal pública del agravio privado y suben la gente que cada uno cree ver fuera.', ind: (m) => { let s = 0, k = 0; for (const a of m.ase) if (a.est >= 0) { s += a.ret; k++; } return 'retratos sin ojos: ' + pc(k ? s / k : 0) + ' de media'; } });
  T('contagio', {
    campo: 'Revuelta', nom: 'Contagio revolucionario y efecto demostración', autor: 'Lohmann (cascadas informativas, 1994); Huntington («bola de nieve», 1991); Weyland (2014)',
    que: 'Una revolución que triunfa enseña a las demás que se puede. Las oleadas (1848, 1989, 2011) viajan con la noticia.',
    aqui: 'Cuando la noticia de una revolución LLEGA a un asentamiento (en una bodega, no antes), sube allí la eficacia percibida y baja el umbral de todos; más si es el mismo régimen. Los gobernadores ambiciosos también toman nota de las independencias ajenas.',
    ind: (m) => { const l = (m.ola || []).filter(t => m.t - t < 2 * S.ANIO).length; let mx = 0; for (const a of m.ase) if ((a.efic || 0) > mx) mx = a.efic; return l + ' revoluciones en los dos últimos años · eficacia percibida máxima ' + mx.toFixed(2); },
  });
  T('jiujitsu', { campo: 'Revuelta', nom: 'Jiu-jitsu político (el tiro por la culata)', autor: 'Gene Sharp (1973); Martin («backfire», 2007)', que: 'La represión visible contra gente desarmada indigna a quienes se enteran y deslegitima al que dispara.', aqui: 'La noticia de una masacre sube el agravio en los demás asentamientos del mismo régimen, y cada orden de disparar tuerce un poco más la cuenta de los soldados.', ind: (m) => (m.cuenta.masacre || 0) + ' masacres registradas' });
  T('curvaj', {
    campo: 'Revuelta', nom: 'Privación relativa y curva J', autor: 'Ted Gurr (1970); James Davies (1962); Tocqueville (1856)',
    que: 'No se rebela el que peor está, sino el que está peor de lo que esperaba: tras años buenos, una caída brusca. Y las concesiones suben las expectativas.',
    aqui: 'Cada asentamiento guarda su bienestar y una expectativa que se mueve despacio; la brecha entre las dos se suma al agravio. Abrir el depósito o ceder ante una huelga sube la expectativa.',
    ind: (m) => { let mx = 0, q = null; for (const a of m.ase) if ((a.brecha || 0) > mx) { mx = a.brecha; q = a; } return q ? 'mayor brecha: ' + q.nom + ' (' + mx.toFixed(2) + ')' : 'sin brechas'; },
  });
  T('salida', { campo: 'Revuelta', nom: 'Salida, voz y lealtad', autor: 'Albert Hirschman (1970)', que: 'Ante el deterioro, la gente se va (salida) o protesta (voz). La emigración es una válvula: los que se van son los que habrían protestado.', aqui: 'Los refugiados que embarcan bajan el agravio y la calle del sitio que dejan; con el sistema cerrado (tormenta, sitio) no hay salida y la voz crece.', ind: (m) => { let s = 0; for (const a of m.ase) s += a.refSal || 0; return Math.round(s).toLocaleString('es-ES') + ' personas han emigrado'; } });
  T('olson', { campo: 'Revuelta', nom: 'Lógica de la acción colectiva', autor: 'Mancur Olson (1965)', que: 'Los grupos grandes se organizan peor: cada uno espera que se mueva el otro. Hace falta un organizador e incentivos.', aqui: 'La organización de una protofacción crece con sus miembros solo hasta unos cientos, y exige un conector con carisma; la caja común (colectas) es el incentivo.', ind: (m) => m.fac.filter(f => f.vivo && f.etapa === 'proto').length + ' protofacciones intentando organizarse' });
  T('movilizacion', { campo: 'Revuelta', nom: 'Movilización de recursos y oportunidad política', autor: 'Charles Tilly (1978); McCarthy y Zald (1977)', que: 'El agravio sobra; lo que falta es organización, dinero y una rendija en el Estado.', aqui: 'Una facción con caja compra armas y grano, y su milicia convierte un levantamiento en insurrección armada. La rendija es la represión percibida.', ind: (m) => { let s = 0; for (const f of m.fac) if (f.vivo && f.armas) s += f.armas; return 'armas en manos de facciones: ' + Math.round(s) + ' t'; } });
  T('brinton', {
    campo: 'Revuelta', nom: 'Anatomía de la revolución', autor: 'Crane Brinton (1938)', que: 'Las revoluciones pasan por fases: moderados, terror y termidor. Suelen acabar en un hombre fuerte.',
    aqui: 'Una comuna nacida de una revolución pasa de luna de miel a terror (tribunales y ejecutados) si se ve amenazada, y años después a termidor: junta o república.',
    ind: (m) => { const c = m.est.filter(e => e.vivo && e.fase); return c.length ? c.map(e => e.nom + ': ' + e.fase).join(' · ') : 'ninguna revolución en el poder'; },
  });
  T('michels', { campo: 'Revuelta', nom: 'Ley de hierro de la oligarquía', autor: 'Robert Michels (1911)', que: 'Toda organización acaba gobernada por unos pocos, también las que nacieron contra eso.', aqui: 'Quien manda se vuelve más autoritario cada mes que pasa en el cargo, y las facciones viejas pierden compromiso.', ind: (m) => 'autoridad media de los gobernantes: ' + media(m, e => e.gob >= 0 ? m.per[e.gob].ideo[0] : 0).toFixed(2) });

  // ════ Estado y élites ════
  T('selectorado', { campo: 'Estado', nom: 'Teoría del selectorado', autor: 'Bueno de Mesquita, Smith, Siverson y Morrow (2003)', que: 'El gobernante se sostiene pagando a una coalición ganadora dentro de un selectorado. Cuanto menor la coalición, más leal… mientras cobre.', aqui: 'A la muerte del gobernante, cada pieza del Estado (flota, ejército, Guardia, iglesia, ministerios, acreedores, gobernadores) elige candidato por utilidad; P(ganar) = poder²/Σpoder².', ind: (m) => (m.cuenta.sucesion || 0) + ' sucesiones, ' + (m.cuenta.guerra_civil || 0) + ' guerras civiles' });
  T('weber', { campo: 'Estado', nom: 'Tipos de legitimidad', autor: 'Max Weber (1922)', que: 'Se obedece por costumbre (tradicional), por la persona (carismática) o por la ley (legal-racional). El carisma muere con quien lo tenía.', aqui: 'El poder de cada Estado es personal, de Estado o popular, y eso decide qué pasa al matar al gobernante: caos, relevo ordenado o un mártir.', ind: (m) => { const c = {}; for (const e of m.est) if (e.vivo) c[e.base] = (c[e.base] || 0) + 1; return Object.keys(c).map(k => c[k] + ' de poder ' + k).join(' · '); } });
  T('sdt', {
    campo: 'Estado', nom: 'Teoría estructural-demográfica', autor: 'Jack Goldstone (1991); Peter Turchin (2003)',
    que: 'Los Estados se rompen cuando coinciden tres presiones: pueblo empobrecido, demasiados aspirantes a élite para los puestos que hay, y un tesoro en apuros. Ψ = las tres multiplicadas.',
    aqui: 'Cada año nacen segundones ambiciosos sin cargo. Con sobreproducción de élites se vuelven contra-élite: encabezan facciones o conspiran, y el golpe se hace más probable.',
    ind: (m) => { const l = m.est.filter(e => e.vivo && e.psi !== undefined).sort((x, y) => y.psi - x.psi); return l.length ? 'Ψ más alto: ' + l[0].nom + ' (' + l[0].psi.toFixed(2) + ', ' + (l[0].aspirantes || 0) + ' aspirantes para ' + (l[0].puestos || 0) + ' puestos)' : '—'; },
  });
  T('asabiya', {
    campo: 'Estado', nom: 'Asabiya (cohesión de grupo)', autor: 'Ibn Jaldún (1377); Turchin («frontera metaétnica», 2003)',
    que: 'Los grupos de frontera, endurecidos, conquistan; el lujo y la paz disuelven su cohesión en tres o cuatro generaciones, y otro grupo los releva.',
    aqui: 'Cada Estado tiene asabiya: nace alta, baja con los años de paz y riqueza, sube con la guerra en sus fronteras. Multiplica la moral en combate y el alcance del control; baja, facilita los golpes.',
    ind: (m) => { const l = m.est.filter(e => e.vivo && e.asabiya !== undefined).sort((x, y) => y.asabiya - x.asabiya); return l.length ? 'más cohesionado: ' + l[0].nom + ' (' + l[0].asabiya.toFixed(2) + ') · menos: ' + l[l.length - 1].nom + ' (' + l[l.length - 1].asabiya.toFixed(2) + ')' : '—'; },
  });
  T('skocpol', { campo: 'Estado', nom: 'Quiebra del Estado', autor: 'Theda Skocpol (1979)', que: 'Las grandes revoluciones no las hacen los revolucionarios: llegan cuando el Estado se quiebra por derrotas militares y ruina fiscal.', aqui: 'Una bancarrota o una flota perdida rebajan de golpe la claridad de mando de la Guardia y del ejército (π), y con ella la represión percibida en todo el territorio.', ind: (m) => (m.cuenta.bancarrota || 0) + ' bancarrotas' });
  T('rentista', { campo: 'Estado', nom: 'Estado rentista y maldición de los recursos', autor: 'Hossein Mahdavy (1970); Michael Ross (2001)', que: 'Un Estado que vive de rentas (peajes, aranceles) y no de impuestos no necesita a su gente: reprime más y rinde menos cuentas.', aqui: 'Si más de la mitad del ingreso son rentas, bajan los impuestos y sube la represión año a año.', ind: (m) => { const l = m.est.filter(e => e.vivo && e.renta > 0.5); return l.length ? 'rentistas: ' + l.map(e => e.nom).join(', ') : 'ningún Estado vive de rentas'; } });
  T('control', { campo: 'Estado', nom: 'El límite de los imperios', autor: 'Informe técnico (inspirado en Innis, 1950)', que: 'No se gobierna más lejos de lo que tarda una orden en ir y volver.', aqui: 'control = e^(−retraso/τ). Las provincias lejanas pagan menos, se rebelan antes y sus guarniciones cobran tarde.', ind: (m) => { let s = 0, k = 0; for (const a of m.ase) if (a.est >= 0 && !a.cap) { s += a.control; k++; } return 'control medio de las provincias: ' + pc(k ? s / k : 1); } });

  // ════ Geopolítica ════
  T('amenaza', {
    campo: 'Geopolítica', nom: 'Equilibrio de amenazas', autor: 'Stephen Walt (1987); Kenneth Waltz (equilibrio de poder, 1979)',
    que: 'Los Estados no se alían contra el más fuerte sino contra el que más temen: fuerte, cercano y con malas intenciones.',
    aqui: 'Cada mes cada Estado mide la amenaza de sus vecinos (flota que les cree, cercanía, hostilidad, conquistas recientes). Si alguien le asusta, propone por carta una alianza a otro que también lo tema. Los aliados entran en guerra cuando se enteran del ataque y comparten planos y doctrina.',
    ind: (m) => { const l = []; for (const e of m.est) if (e.vivo && e.alianzas) for (const j of e.alianzas) if (j > e.id && m.est[j].vivo) l.push(e.nom + ' ↔ ' + m.est[j].nom); return l.length ? l.join(' · ') : 'ninguna alianza en vigor'; },
  });
  T('dilema', { campo: 'Geopolítica', nom: 'Dilema de seguridad y carrera de armamentos', autor: 'Robert Jervis (1978); Lewis Richardson (1960)', que: 'Lo que uno construye para sentirse seguro asusta al vecino, que construye a su vez.', aqui: 'El tamaño de flota que cada Estado quiere crece con el de sus vecinos… según lo que le han contado, con meses de retraso.', ind: (m) => { let s = 0; for (const e of m.est) if (e.vivo) s += e.navG || 0; return s.toLocaleString('es-ES') + ' naves de guerra en la galaxia'; } });
  T('transicion', { campo: 'Geopolítica', nom: 'Transición de poder (trampa de Tucídides)', autor: 'A. F. K. Organski (1958); Graham Allison (2017)', que: 'La guerra es más probable cuando un aspirante en ascenso alcanza a la potencia establecida.', aqui: 'Si la fuerza creída de un vecino está entre el 80 % y el 125 % de la propia, declarar la guerra pesa más en la decisión del gobernante.', ind: (m) => (m.cuenta.guerra || 0) + ' guerras declaradas' });
  T('pazcomercial', { campo: 'Geopolítica', nom: 'Paz comercial y paz entre repúblicas', autor: 'Montesquieu (1748); Kant (1795); Norman Angell (1910); Michael Doyle (1983)', que: 'Dos Estados que comercian mucho pierden demasiado si pelean; dos repúblicas rara vez se atacan.', aqui: 'El comercio bilateral (toneladas que cruzan entre sus puertos) y que ambos sean repúblicas restan utilidad a la guerra.', ind: (m) => { let mx = 0, t = ''; for (const e of m.est) if (e.vivo) for (const [j, r] of e.rel) if ((r.com || 0) > mx && m.est[j].vivo) { mx = r.com; t = e.nom + ' ↔ ' + m.est[j].nom; } return t ? 'mayor comercio bilateral: ' + t + ' (' + Math.round(mx) + ' t/mes)' : '—'; } });
  T('estrangulamiento', {
    campo: 'Geopolítica', nom: 'Poder marítimo y puntos de estrangulamiento', autor: 'Alfred Mahan (1890); Halford Mackinder (1904); Nicholas Spykman (1942)',
    que: 'Quien controla los pasos por los que cruza el comercio cobra de todos y decide las guerras.',
    aqui: 'Cada sistema tiene una centralidad (cuántas rutas mínimas pasan por él). El dueño cobra peaje a cada carguero que cruza, y los pasos valen más como objetivo de guerra.',
    ind: (m) => { const l = m.sis.slice().sort((x, y) => (y.bc || 0) - (x.bc || 0)).slice(0, 3); return 'pasos clave: ' + l.map(s => s.nom + (s.est >= 0 ? ' (' + m.est[s.est].nom + ')' : ' (de nadie)')).join(', '); },
  });
  T('boulding', { campo: 'Geopolítica', nom: 'Gradiente de pérdida de fuerza', autor: 'Kenneth Boulding (1962); Clausewitz (punto culminante, 1832)', que: 'La fuerza de un ejército cae con la distancia a su base.', aqui: 'La eficacia de una flota en combate baja hasta un 35 % cuando pelea a 800 unidades de su base.', ind: () => 'actúa en cada batalla' });
  T('dependencia', { campo: 'Geopolítica', nom: 'Centro y periferia', autor: 'Raúl Prebisch (1950); Immanuel Wallerstein (1974)', que: 'El centro vende manufacturas caras y compra materias primas baratas: el intercambio desigual mantiene pobre a la periferia.', aqui: 'Cada asentamiento se clasifica por el valor que añade por habitante (centro, semiperiferia, periferia) y se mide su relación de intercambio. Es una capa del mapa; aún no cambia el comportamiento.', ind: (m) => { let c = 0, p = 0; for (const a of m.ase) { if (a.clase === 'centro') c++; if (a.clase === 'periferia') p++; } return c + ' asentamientos de centro, ' + p + ' de periferia'; } });

  // ════ Economía ════
  T('precio', { campo: 'Economía', nom: 'Precio local con elasticidad', autor: 'Informe técnico (Marshall, 1890)', que: 'El precio depende de cuánta demanda hay frente a cuánto llega.', aqui: 'p = p_ref·(D/S)^0,8 en cada asentamiento. No hay mercado galáctico: los mercaderes igualan precios (o no) moviendo carga con noticias viejas.', ind: (m) => { let s = 0; for (const a of m.ase) s += a.pr[B.grano]; return 'grano a ' + Math.round(s / m.ase.length) + ' de media'; } });
  T('cuantitativa', { campo: 'Economía', nom: 'Teoría cuantitativa del dinero', autor: 'Irving Fisher (1911)', que: 'Si se imprime un 25 % más de moneda, los precios en esa moneda suben parecido a medio plazo.', aqui: 'El nivel de precios de cada Estado tiende a M/M₀. La inflación suma agravio y erosiona la paga del soldado.', ind: (m) => 'inflación máxima: ×' + Math.max.apply(null, m.est.filter(e => e.vivo).map(e => e.mon.P).concat([1])).toFixed(2) });
  T('malthus', { campo: 'Economía', nom: 'Trampa maltusiana', autor: 'Thomas Malthus (1798)', que: 'La población crece hasta donde llega la comida; después, el hambre la frena.', aqui: 'El crecimiento de cada asentamiento depende del precio del grano: abundancia, más gente; carestía, ninguna; hambre, muertes.', ind: (m) => { let p = 0; for (const a of m.ase) p += a.pob; return 'población: ' + Math.round(p).toLocaleString('es-ES'); } });
  T('wright', { campo: 'Economía', nom: 'Curva de aprendizaje', autor: 'T. P. Wright (1936); Kenneth Arrow («aprender haciendo», 1962)', que: 'Cada vez que se dobla la producción acumulada, hacer una unidad cuesta menos y sale mejor.', aqui: 'Cada astillero acumula experiencia en cascos botados: construye más rápido y con mejor calidad en cada componente.', ind: (m) => { let mx = 0, q = null; for (const i of m.ins) if ((i.exp || 0) > mx) { mx = i.exp; q = i; } return q ? 'astillero con más oficio: ' + q.nom + ' (' + Math.round(mx) + ' cascos)' : '—'; } });
  T('inversion', { campo: 'Economía', nom: 'Respuesta de la oferta (inversión y cierre)', autor: 'Alfred Marshall (1890); principio del acelerador', que: 'Los precios altos sostenidos atraen inversión; los bajos cierran fábricas.', aqui: 'Cada año, donde un género lleva tiempo caro se amplían o se fundan instalaciones; donde sobra, se cierran turnos y hay paro.', ind: (m) => (m.cuenta.fundacion || 0) + ' instalaciones fundadas por el mercado' });
  T('informacion', { campo: 'Economía', nom: 'Información imperfecta', autor: 'Hayek (1945); Stigler (1961); Akerlof (1970)', que: 'Los precios son información, y la información cuesta y llega tarde.', aqui: 'Cada mercader decide con el tablón de precios que conoce, descontado por su edad (τ = 5 días), y elige ruta con una softmax para no ir todos al mismo sitio.', ind: (m) => m.paq.length + ' noticias creadas' });

  // ════ Guerra y técnica ════
  T('lanchester', { campo: 'Guerra', nom: 'Leyes de Lanchester', autor: 'Lanchester y Osipov (1915–1916)', que: 'Cuando todos pueden disparar a todos, la fuerza crece con el cuadrado del número. En terreno cerrado, solo con el número.', aqui: 'Las batallas de flota integran dA/dt = −b·B día a día; entre asteroides y en las calles se usa la ley lineal.', ind: (m) => (m.cuenta.batalla || 0) + ' batallas' });
  T('reinaroja', {
    campo: 'Guerra', nom: 'Coevolución (hipótesis de la Reina Roja)', autor: 'Leigh Van Valen (1973); John Holland (algoritmos genéticos, 1975)', que: 'Hay que correr todo lo posible para seguir en el mismo sitio: cada mejora del rival deja obsoleta la tuya.',
    aqui: 'Cada Estado mantiene una población de diseños por clase de nave. Cada año conserva los que mejor relación de bajas han dado (en batallas y en maniobras contra lo que cree que tiene el rival), los cruza y los muta. Los diseños se persiguen unos a otros.',
    ind: (m) => { let g2 = 0; for (const e of m.est) if (e.vivo && e.dis) g2 = Math.max(g2, e.dis.fragata.gen); return 'generación más avanzada: Mk ' + S.tec.romano(Math.max(1, g2)) + ' · ' + (m.dis ? m.dis.length : 0) + ' diseños probados'; },
  });
  T('difusion', { campo: 'Guerra', nom: 'Difusión de innovaciones', autor: 'Everett Rogers (1962)', que: 'Las innovaciones se copian: por alianza, por comercio o por captura.', aqui: 'Los aliados se pasan planos y doctrina; una caja negra vendida en tu puerto te enseña el diseño enemigo; las facciones que comparten casa se enseñan a pelear, y la experiencia ajena llega con las noticias.', ind: (m) => (m.cuenta.ingenieria_inversa || 0) + ' ingenierías inversas · ' + (m.cuenta.transferencia || 0) + ' transferencias entre aliados' });
  T('paris', { campo: 'Guerra', nom: 'Ley de Paris (fatiga de materiales)', autor: 'Paul Paris (1961)', que: 'Una grieta crece con cada ciclo de carga, despacio al principio y de golpe al final.', aqui: 'Cada núcleo de reactor tiene su grieta, con solución cerrada: se programa la fecha de fallo en vez de comprobarla cada día. Un detector de 1 mm solo la ve al final.', ind: (m) => (m.cuenta.explosion || 0) + ' núcleos reventados' });
  T('fiedler', { campo: 'Revuelta', nom: 'Conectividad algebraica', autor: 'Miroslav Fiedler (1973)', que: 'El segundo autovalor del laplaciano de un grafo mide lo cerca que está de partirse en dos, y su vector dice por dónde.', aqui: 'Cada facción calcula λ₂ del grafo de confianza entre sus casas. Si las ideas se separan o muere el enlace, se parte por las amistades.', ind: (m) => (m.cuenta.cisma || 0) + ' cismas' });

  // ── Contagio, jiu-jitsu, efecto demostración.
  S.gancho('noticia.revolucion', function (m, a, p) {
    const sim = a.est === p.d.e ? 1 : 0.35;
    a.efic = Math.min(0.7, (a.efic || 0) + 0.17 * sim); a.evSenal = p.ev;
  });
  S.gancho('noticia.masacre', function (m, a, p) {
    if (a.est < 0 || a.est !== p.d.e || a.id === p.a) return;
    for (const c of a.coh) m.coh[c].agr.reg = Math.min(1, m.coh[c].agr.reg + 0.03);
    a.efic = Math.max(0, (a.efic || 0) - 0.04);
  });
  S.gancho('noticia.independencia', function (m, a, p) { if (a.est === p.d.e) a.demo = Math.min(1, (a.demo || 0) + 0.5); });
  S.gancho('hecho', function (m, e) {
    if (e.k === 'revolucion') { (m.ola = m.ola || []).push(m.t); if (m.ola.length > 40) m.ola.shift(); }
    else if (e.k === 'independencia' && e.a >= 0 && e.d) S.inf.crear(m, e.id, 'independencia', e.a, 0.7, 1, { e: e.d.e });
    else if (e.k === 'masacre' && e.a >= 0 && m.ase[e.a].not && !m.paq.some(p => p.ev === e.id)) S.inf.crear(m, e.id, 'masacre', e.a, 0.8, e.d ? e.d.muertos : 0, { e: m.ase[e.a].est });
    else if (e.k === 'insurreccion' && e.a >= 0) S.inf.crear(m, e.id, 'insurreccion', e.a, 0.8, 1, { e: m.ase[e.a].est });
    else if (e.k === 'bancarrota' && e.d) { const x = m.est[e.d.e]; x.piG *= 0.7; x.piE *= 0.8; }                     // Skocpol
    else if ((e.k === 'concesion' || e.k === 'motin_pan') && e.a >= 0) { const a = m.ase[e.a]; a.expect = Math.min(1.2, (a.expect === undefined ? 1 : a.expect) + 0.08); }   // Tocqueville
    else if (e.k === 'batalla_fin' && e.d && e.d.pierde >= 0) { const x = m.est[e.d.pierde]; if (x.vivo && e.d.perdidas > 60) x.piE *= 0.92; }
  });

  // ── Curva J, salida y voz, clase centro/periferia: una vez al mes por asentamiento.
  S.gancho('asent.mes', function (m, a) {
    const caro = Math.max(0, a.pr[B.bienes] / S.bien[B.bienes].pref - 1.5) * 0.08;
    a.bien = S.clamp(1 - a.H - 0.3 * a.Hf - caro - (a.paro || 0) * 0.3, 0, 1);
    if (a.expect === undefined) a.expect = a.bien;
    a.expect += (a.bien - a.expect) * 0.03; a.brecha = Math.max(0, a.expect - a.bien);
    if (a.efic) a.efic *= 0.88; if (a.demo) a.demo *= 0.93;
    const sal = (a.refSal || 0) - (a._refPrev || 0); a._refPrev = a.refSal || 0;
    if (sal > 0) { const fr = Math.min(0.04, sal / a.pob); const gen = m.coh[a.cohGen]; gen.agr.reg = Math.max(0, gen.agr.reg - fr * 3); a.f *= 1 - Math.min(0.3, fr * 10); }
    else if (a.H > 0.3 && m.sis[a.sis].bloqueo > m.t) { const gen = m.coh[a.cohGen]; gen.agr.reg = Math.min(1, gen.agr.reg + 0.03); }
  });

  // ── Centralidad de cada sistema (cuántas rutas mínimas lo cruzan).
  Th.centralidad = function (m) {
    const N = m.sis.length; const bc = new Array(N).fill(0);
    for (let i = 0; i < N; i++) for (let j = i + 1; j < N; j++) { let s = m.sig[i][j]; while (s >= 0 && s !== j) { bc[s]++; s = m.sig[s][j]; } }
    const mx = Math.max.apply(null, bc.concat([1])); for (let i = 0; i < N; i++) m.sis[i].bc = bc[i] / mx;
  };
  // Peaje: quien cruza un paso paga a su dueño.
  S.gancho('cruce', function (m, n, s) {
    if (s.est < 0 || n.dueno.t === 'E' || !n.carga || n.pirata) return;
    const peaje = n.carga.q * S.bien[n.carga.c].pref * 0.012 * (0.2 + (s.bc || 0)); const e = m.est[s.est];
    e.ingr += peaje; e.peajes = (e.peajes || 0) + peaje; n.dinero -= peaje;
  });

  // ── Alianzas por equilibrio de amenazas.
  Th.amenaza = function (m, e, j) {
    const o = m.est[j]; const r = S.pol.rel(m, e, j);
    const propia = Math.max(20, e.navG || 20); const suya = S.pol.fuerzaCreida(m, e, j) / (0.1 * o.tec); // cascos² creídos
    return Math.sqrt(Math.max(0, suya)) / propia * (0.6 - r.op + 0.5 * r.cb + 0.25 * Math.min(4, o.victorias));
  };
  Th.aliados = (m, e, j) => !!(e.alianzas && e.alianzas.has(j));
  Th.diplomacia = function (m, e, gob) {
    e.alianzas = e.alianzas || new Set();
    for (const j of Array.from(e.alianzas)) {
      const o = m.est[j];
      if (!o.vivo) { e.alianzas.delete(j); continue; }
      // Los aliados comparten doctrina y lo que saben del enemigo.
      if (o.doc && e.doc) { if (o.doc.naval > e.doc.naval) e.doc.naval += 0.03 * (o.doc.naval - e.doc.naval); if (o.doc.tierra > e.doc.tierra) e.doc.tierra += 0.03 * (o.doc.tierra - e.doc.tierra); }
      if (o.intel && e.intel) for (const [k, it] of o.intel) { const mio = e.intel.get(k); if (k !== e.id && (!mio || mio.t < it.t)) e.intel.set(k, it); }
      const al = e.alCon && e.alCon.get(j); if (al && (!m.est[al.contra].vivo || m.t - al.t > 12 * S.ANIO) && gob.rng.p(0.05)) { e.alianzas.delete(j); o.alianzas.delete(e.id); m.reg('alianza_fin', 'Se deshace la alianza entre {E' + e.id + '} y {E' + j + '}: ya no hay a quién temer juntos', { a: e.cap, imp: 1, c: [al.ev], d: { e: e.id, j } }); }
    }
    if (e.suc || e.alianzas.size >= 2 || (e.tPropuesta && m.t - e.tPropuesta < 2 * S.ANIO)) return;
    const vec = S.pol.vecinos(m, e); let peor = -1, am = 0.9;
    for (const j of vec) { if (e.alianzas.has(j)) continue; const x = Th.amenaza(m, e, j); if (x > am) { am = x; peor = j; } }
    if (peor < 0) return;
    // Busca a otro que también tenga motivos para temerlo.
    let socio = -1, bs = -0.3;
    for (const k of new Set(vec.concat(S.pol.vecinos(m, m.est[peor])))) { if (k === peor || k === e.id || !m.est[k].vivo || e.gue.has(k) || e.alianzas.has(k)) continue; const s = S.pol.rel(m, e, k).op + (S.pol.vecinos(m, m.est[k]).indexOf(peor) >= 0 ? 0.3 : 0); if (s > bs) { bs = s; socio = k; } }
    if (socio < 0) return;
    e.tPropuesta = m.t;
    S.cor.enviar(m, e.cap, m.est[socio].cap, 'alianza', { de: e.id, a: socio, contra: peor, urg: 2 }, -1, e.id);
  };
  S.def('carta', 'alianza', {
    nom: 'una propuesta de alianza',
    llega(m, c) {
      const o = m.est[c.d.a], e = m.est[c.d.de], x = m.est[c.d.contra]; if (!o.vivo || !e.vivo || !x.vivo || o.gue.has(e.id) || o.gob < 0) return;
      o.alianzas = o.alianzas || new Set(); e.alianzas = e.alianzas || new Set();
      const teme = S.pol.vecinos(m, o).indexOf(x.id) >= 0 ? Th.amenaza(m, o, x.id) : 0.3; const r = S.pol.rel(m, o, e.id);
      if (teme + r.op * 0.5 < 0.45 || o.alianzas.size >= 2) { r.op -= 0.05; return; }
      const ev = m.reg('alianza', '{E' + e.id + '} y {E' + o.id + '} firman una alianza contra {E' + x.id + '}: lo temen más de lo que se temen entre sí', { a: o.cap, imp: 2, d: { e: e.id, j: o.id, contra: x.id } });
      o.alianzas.add(e.id); e.alianzas.add(o.id); (o.alCon = o.alCon || new Map()).set(e.id, { contra: x.id, t: m.t, ev }); (e.alCon = e.alCon || new Map()).set(o.id, { contra: x.id, t: m.t, ev });
      r.op = Math.min(1, r.op + 0.3); S.pol.rel(m, e, o.id).op = Math.min(1, S.pol.rel(m, e, o.id).op + 0.3);
      S.inf.crear(m, ev, 'alianza', o.cap, 0.7, 1, { e: e.id, j: o.id });
    },
  });
  // Un aliado se entera de que han atacado a su socio (cuando le llega la noticia) y decide si honra el pacto.
  S.gancho('noticia.guerra', function (m, a, p) {
    if (a.est < 0 || !a.cap) return; const e = m.est[a.est]; if (!e.alianzas || !e.alianzas.size || e.gob < 0) return;
    const agresor = p.d.e, victima = p.d.j;
    if (!e.alianzas.has(victima) || agresor === e.id || e.gue.has(agresor) || !m.est[agresor].vivo) return;
    const gob = m.per[e.gob];
    if (gob.rng.p(0.35 + 0.6 * gob.r[R.LEA])) S.pol.declarar(m, e, agresor, { motivo: 'honra su alianza con {E' + victima + '}', c: p.ev });
    else { e.alianzas.delete(victima); m.est[victima].alianzas.delete(e.id); const r = S.pol.rel(m, m.est[victima], e.id); r.op -= 0.6; r.cb += 0.5; r.ev = m.reg('alianza_rota', '{E' + e.id + '} no acude: deja solo a {E' + victima + '} frente a {E' + agresor + '}', { a: e.cap, imp: 2, c: [p.ev], d: { e: e.id, j: victima } }); }
  });
  // Ajustes a la utilidad de declarar una guerra (los llama la diplomacia de cada Estado).
  Th.guerraU = function (m, e, j, propia, rival) {
    if (Th.aliados(m, e, j)) return -9;
    const o = m.est[j]; const r = S.pol.rel(m, e, j); let u = 0;
    const ratio = propia / (rival + 1e-9); if (ratio > 0.8 && ratio < 1.25) u += 0.12;                                      // transición de poder
    u -= Math.min(0.3, (r.com || 0) / 4000);                                                                               // paz comercial
    if (e.tipoGob === 'republica' && o.tipoGob === 'republica') u -= 0.2;                                                  // paz entre repúblicas
    if (o.alianzas && o.alianzas.size) u -= 0.12 * o.alianzas.size;                                                        // disuasión
    return u;
  };
  // Carrera de armamentos: cuántas escuadras quiere, vista la flota que le cree al vecino.
  Th.carrera = function (m, e) { let mx = 0; for (const j of S.pol.vecinos(m, e)) { const o = m.est[j]; const lag = Math.min(o.hist.length - 1, 3); const n = o.hist.length ? o.hist[Math.max(0, o.hist.length - 1 - lag)] : 0; if (n > mx && !Th.aliados(m, e, j)) mx = n; } return Math.round(0.75 * mx / 36); };

  // ── Una vez al año por Estado: asabiya, Ψ, élites, rentas, fases de la revolución, planos entre aliados.
  S.gancho('anio', function (m) {
    if (m.sis[0].bc === undefined) Th.centralidad(m);
    // Centro y periferia.
    const va = m.ase.map(a => { let v = 0; for (let c = 0; c < S.NB; c++) v += a.prod[c] * S.bien[c].pref * (c === B.mineral || c === B.grano ? 1 : 2.2); return v / Math.max(1, a.pob) * 1000; }); const ord = va.slice().sort((x, y) => x - y);
    m.ase.forEach((a, i) => { a.va = va[i]; a.clase = va[i] >= ord[Math.floor(ord.length * 0.67)] ? 'centro' : va[i] >= ord[Math.floor(ord.length * 0.33)] ? 'semiperiferia' : 'periferia'; });
    for (const e of m.est) {
      if (!e.vivo) continue; const gob = e.gob >= 0 ? m.per[e.gob] : null; const cap = m.ase[e.cap];
      // Asabiya.
      if (e.asabiya === undefined) e.asabiya = e.nac > 1 ? 0.85 : e.rng.r(0.5, 0.8);
      let frontera = false; for (const j of S.pol.vecinos(m, e)) if (S.pol.rel(m, e, j).op < -0.15) frontera = true;
      e.asabiya = S.clamp(e.asabiya - 0.012 - 0.015 * Math.min(1, Math.max(0, e.tes) / 400000) + (e.gue.size ? 0.035 : 0) + (frontera ? 0.008 : 0) - (e.derrotas > 2 ? 0.02 : 0), 0.2, 1);
      // Élites: cada año nacen segundones sin cargo.
      const mios = m.per.filter(p => p.vivo && p.est === e.id);
      const cargos = mios.filter(p => p.cargo && p.cargo.t !== 'capitan'); e.puestos = cargos.length;
      let asp = mios.filter(p => p.rol === 'aspirante');
      for (const p of asp) if (S.per.edad(m, p) > 58) { p.rol = 'civil'; }
      asp = asp.filter(p => p.rol === 'aspirante');
      const nuevos = Math.min(4, Math.round(cargos.length * 0.035 * (1 + Math.min(1, Math.max(0, e.tes) / 200000))));
      for (let i = 0; i < nuevos && asp.length < 36 && cargos.length; i++) { const padre = cargos[e.rng.i(cargos.length)]; const p = S.per.crear(m, { casa: S.per.lugar(m, padre), est: e.id, rol: 'aspirante', apellido: S.per.apellido(padre), nace: m.t - e.rng.r(20, 30) * S.ANIO, ideo: padre.ideo, r: { AMB: e.rng.r(0.6, 0.98) } }); p.fam.padres.push(padre.id); padre.fam.hijos.push(p.id); asp.push(p); }
      e.aspirantes = asp.length;
      const emp = asp.length / Math.max(1, cargos.length); let Hm = 0, pb = 0; for (const a of S.pol.territorio(m, e)) { Hm += a.H * a.pob; pb += a.pob; } Hm = pb ? Hm / pb : 0;
      const deuda = S.pol.deuda(m, e); const sfd = 0.25 + S.clamp(deuda / Math.max(1, (e.ultIngreso || 1) * 12) + 0.15 * (e.msc || 0) + (e.tes <= 0 ? 0.3 : 0), 0, 3);
      e.mmp = (e.agr || 0.3) * (1 + 2 * Hm); e.emp = emp; e.sfd = sfd; e.psi = e.mmp * (0.3 + emp) * sfd * 3;
      // Estado rentista.
      e.renta = (e.ultRentas || 0) / Math.max(1, e.ultIngreso || 1);
      if (e.renta > 0.5) { e.rep = Math.min(0.9, e.rep + 0.01); e.imp = Math.max(0.06, e.imp - 0.004); }
      // Michels: quien manda se endurece (el gobernante ya lo hace mes a mes); las revoluciones siguen su anatomía.
      if (e.tipoGob === 'comuna' && gob) {
        const edad = (m.t - e.nac) / S.ANIO;
        if (!e.fase) e.fase = 'luna de miel';
        if (e.fase === 'luna de miel' && edad >= 1 && (e.gue.size || (e.agr || 0) > 0.4 || e.rng.p(0.35))) {
          e.fase = 'terror'; e.rep = 0.6; const k = 8 + e.rng.i(30);
          const ev = m.reg('terror', '{E' + e.id + '} entra en su fase de terror: tribunales populares en {A' + cap.id + '} y ' + S.numPal(k) + ' ejecutados por «enemigos de la revolución»', { a: cap.id, imp: 2, c: [e.evNace], d: { e: e.id, muertos: k } });
          S.soc.muertes(m, cap, m.coh[cap.cohGen], Math.min(k, m.coh[cap.cohGen].n - 1), ev, { clave: 'reg', hecho: 'masacre', culpable: gob.id });
          for (const p of mios) if (p.rol === 'exiliado' || (p.rol === 'aspirante' && e.rng.p(0.4))) S.per.matar(m, p, { modo: 'ejecucion', por: gob.id, c: [ev] });
        } else if ((e.fase === 'terror' && edad >= 4) || (e.fase === 'luna de miel' && edad >= 6)) {
          const fuerte = gob.r[R.AMB] > 0.6; e.fase = 'termidor'; e.tipoGob = fuerte ? 'junta' : 'republica'; const def = S.reg.gob[e.tipoGob]; e.rep = def.rep; e.imp = def.imp; e.base = def.base; e.guardia = !!def.guardia;
          const viejo = e.nom; e.nom = def.nom + ' de ' + cap.nom; gob.cargo.nom = S.pol.titulo(e, gob) + ' de ' + cap.nom;
          m.reg('termidor', 'Termidor: ' + viejo + ' se acaba. {P' + gob.id + '} ' + (fuerte ? 'se queda solo en el poder' : 'convoca una asamblea de notables') + ' y nace {E' + e.id + '}', { a: cap.id, imp: 2, c: [e.evNace], d: { e: e.id } });
        }
      }
      // Los aliados se pasan planos: el mejor diseño del socio entra en la población propia.
      if (e.alianzas && e.dis) for (const j of e.alianzas) { const o = m.est[j]; if (!o.vivo || !o.dis || !e.rng.p(0.3)) continue; const cls = e.rng.el(['fragata', 'crucero', 'acorazado']); const suyo = o.dis[cls].mejor, li = e.dis[cls]; if (suyo.fit <= li.pob[li.pob.length - 1].fit) continue; const d = S.tec.nuevoDiseno(m, e.id, cls, suyo.g, li.gen, li.base, [suyo.id]); d.fit = suyo.fit * 0.95; li.pob[li.pob.length - 1] = d; m.reg('transferencia', '{E' + o.id + '} pasa a su aliado {E' + e.id + '} los planos de la ' + suyo.nom, { a: e.cap, imp: 0, d: { e: e.id, j } }); }
    }
    for (const f of m.fac) if (f.vivo && f.etapa === 'inst' && f.tipo !== 'casa' && m.t - f.nac > 6 * S.ANIO) { f.comp = Math.max(0.1, f.comp - 0.02); const l = m.per[f.lid]; if (l && l.vivo) l.ideo[0] = Math.min(1, l.ideo[0] + 0.03); }
  });
  // Contra-élite: un segundón sin puesto busca otra escalera.
  S.def('rol', 'aspirante', {
    nom: 'aspirante sin cargo',
    tic(m, p) {
      const e = p.est >= 0 ? m.est[p.est] : null; if (!e || !e.vivo || (e.emp || 0) < 0.45 || p.fac >= 0) return;
      const a = m.ase[S.per.lugar(m, p)];
      const f = m.fac.find(x => x.vivo && x.tipo !== 'casa' && x.enem.k === 'reg' && x.enem.id === e.id && (x.sede === a.id || x.cel.some(c => c.ase === a.id)));
      const u = p.r[R.AMB] * 0.5 + e.emp * 0.4 - p.r[R.LEA] * 0.5 - p.r[R.PRU] * 0.2;
      if (f && m.t > (f.tElite || 0) && p.rng.p(Math.max(0, u) * 0.05)) {
        f.tElite = m.t + 3 * S.ANIO;
        p.fac = f.id; p.rol = 'lider'; p.cargo = { t: 'enlace', id: f.id, nom: 'contra-élite en ' + (f.nom || 'una protofacción') }; f.O += 0.25; f.comp = Math.min(1, f.comp + 0.1); if (f.doc !== undefined) f.doc = Math.min(1, f.doc + 0.05);
        m.reg('contraelite', '{P' + p.id + '}, sin puesto en la corte de {E' + e.id + '}, se pasa a ' + (f.nom || 'los descontentos de {A' + a.id + '}') + ': les lleva apellido, dinero y contactos', { a: a.id, imp: 1, c: [f.evNace], d: { e: e.id, f: f.id } });
      } else if (p.rng.p(Math.max(0, u) * 0.05)) e.conspira = Math.min(0.4, (e.conspira || 0) + 0.04);
    },
  });

  S.sismografo('psi', 'Tensión estructural Ψ (máxima)', m => { let s = 0; for (const e of m.est) if (e.vivo && e.psi > s) s = e.psi; return s; });
  S.sismografo('asabiya', 'Asabiya media', m => media(m, e => e.asabiya === undefined ? 0.6 : e.asabiya));
  S.sismografo('eficacia', 'Eficacia percibida de la revuelta (máx.)', m => { let s = 0; for (const a of m.ase) if ((a.efic || 0) > s) s = a.efic; return s; });
  S.gancho('sismo', function (m) { for (const e of m.est) if (e.vivo) { m.serie('Y' + e.id, e.psi || 0); m.serie('B' + e.id, e.asabiya === undefined ? 0.6 : e.asabiya); } });
})(typeof globalThis !== 'undefined' ? globalThis : this);
