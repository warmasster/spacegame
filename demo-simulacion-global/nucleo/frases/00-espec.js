// Frases: el vocabulario con el que el mundo cuenta lo que pasa. Cada hueco («slot») tiene quince maneras de
// decir lo mismo, ordenadas en una matriz de 5 × 3: cinco grados de épica (filas) por tres de drama (columnas).
// Cuál se usa no lo decide el azar: lo deciden la importancia histórica y lo dramático de la vida de quien se cuenta.
//   épica 0 anodino (asiento de archivo) · 1 corriente (narración sobria) · 2 notable (prosa cuidada)
//         3 histórico (crónica solemne)  · 4 legendario (resonancia de mito)
//   drama 0 sereno · 1 tenso (conflicto, amargura, presagio) · 2 trágico (pérdida, violencia, fatalidad)
// En el texto: {palabra} es un dato que se rellena; {masculino|femenino} elige por el sexo de la persona.
(function (g) {
  'use strict';
  const S = g.SIM;
  const F = S.frases = function (slot, matriz) { F.banco[slot] = matriz; };
  F.banco = {}; F.ESPEC = {}; F.GRUPOS = {};
  F.casilla = (E, D) => [Math.min(4, Math.max(0, Math.round((E || 0) * 4))), Math.min(2, Math.max(0, Math.round((D || 0) * 2)))];
  F.rellena = function (txt, datos, sexo) {
    return txt.replace(/\{([^{}|]*)\|([^{}|]*)\}/g, (_, h, mu) => (sexo === 'M' ? mu : h)).replace(/\{([a-z_0-9]+)\}/g, (x, k) => (datos && datos[k] !== undefined && datos[k] !== null ? datos[k] : x));
  };
  // La frase de un hueco para una épica E y un drama D (los dos de 0 a 1). Sin banco, cadena vacía.
  F.de = function (slot, E, D, datos, sexo) { const b = F.banco[slot]; if (!b) return ''; const [e, d] = F.casilla(E, D); const t = (b[e] && b[e][d]) || (b[0] && b[0][0]) || ''; return F.rellena(t, datos, sexo); };
  F.hay = (slot) => !!F.banco[slot];
  // La k-ésima vez que un mismo hueco sale en un mismo texto no repite frase: se corre a la casilla vecina
  // (primero por la columna de drama, luego baja una fila de épica). Sigue sin haber azar.
  F.vez = function (slot, E, D, k, datos, sexo) { const b = F.banco[slot]; if (!b) return ''; let [e, d] = F.casilla(E, D); d = (d + k) % 3; e = Math.max(0, Math.min(4, e - Math.floor(k / 3))); return F.rellena((b[e] && b[e][d]) || b[0][0], datos, sexo); };

  // ── Especificación: qué huecos hay, de qué grupo (archivo) son, qué datos admite cada uno y cuándo se usa.
  const E = (grupo, slot, tokens, que, forma) => { F.ESPEC[slot] = { grupo, tokens: tokens ? tokens.split(' ') : [], que, forma: forma || 'frase' }; (F.GRUPOS[grupo] = F.GRUPOS[grupo] || []).push(slot); };

  // Grupo «origen» → nucleo/frases/10-origen.js
  E('origen', 'nace', 'lugar anio', 'Nacimiento: dónde ({lugar}) y cuándo (año {anio}).');
  E('origen', 'nace_padres', 'padre madre', 'De quién es hij{o|a}: sus padres tienen nombre.');
  E('origen', 'nace_sin_padres', '', 'Nada se sabe de sus padres.');
  E('origen', 'cuna_humilde', '', 'Nació y creció con muy poco.');
  E('origen', 'cuna_rica', '', 'Nació en una casa donde no faltaba el dinero.');
  for (const [r, a, b] of [['codicia', 'alta: lo quiere todo, el dinero l{o|a} mueve', 'baja: el dinero no le importa'], ['valor', 'alto: no retrocede ante el peligro', 'bajo: el miedo decide por {él|ella}'], ['lealtad', 'alta: no traiciona a los suyos', 'baja: cambia de bando cuando conviene'], ['empatia', 'alta: le duele el dolor ajeno', 'baja: el dolor ajeno no l{o|a} toca'], ['rencor', 'alto: no olvida una ofensa', 'bajo: perdona pronto'], ['ambicion', 'alta: quiere mandar, subir, más', 'baja: no aspira a nada más que a lo suyo'], ['fe', 'alta: cree de verdad', 'baja: no cree en nada que no vea'], ['prudencia', 'alta: mide cada paso', 'baja: se lanza sin pensar'], ['carisma', 'alto: la gente l{o|a} sigue', 'bajo: nadie se fija en {él|ella}'], ['habilidad', 'alta: hace bien lo que toca', 'baja: torpe en su oficio']]) {
    const fem = r !== 'valor' && r !== 'rencor' && r !== 'carisma';
    E('origen', 'rasgo_' + r + (fem ? '_alta' : '_alto'), '', 'Retrato de carácter: ' + r + ' ' + a + '.');
    E('origen', 'rasgo_' + r + (fem ? '_baja' : '_bajo'), '', 'Retrato de carácter: ' + r + ' ' + b + '.');
  }
  E('origen', 'familia_pareja', 'pareja', 'Tiene (o tuvo) pareja: {pareja}.');
  E('origen', 'familia_viudez', 'pareja', 'Su pareja, {pareja}, murió antes que {él|ella}.');
  E('origen', 'familia_hijos', 'n hijos', 'Tiene {n} hijos; {hijos} es la lista de nombres ya escrita («Ana y Luis»).');
  E('origen', 'familia_sola', '', 'No se le conoce pareja ni hijos.');
  E('origen', 'ideas_orden', '', 'Cree en el orden, la autoridad y que las cosas sigan como están.');
  E('origen', 'ideas_cambio', '', 'Cree que todo debe cambiar; desconfía de quien manda.');
  E('origen', 'fama_ninguna', '', 'Fuera de su casa nadie supo nunca su nombre.');
  E('origen', 'fama_extendida', 'n', 'Su nombre se conoce en {n} mundos.');

  // Grupo «carrera» → nucleo/frases/20-carrera.js
  E('carrera', 'cargo_gobernante', 'estado', 'Llegó a gobernar {estado} (el Estado entero).');
  E('carrera', 'cargo_gobernador', 'lugar', 'Fue gobernador{|a} de {lugar} (un asentamiento, por nombramiento de la capital).');
  E('carrera', 'cargo_almirante', 'flota', 'Mandó la {flota} (una flota de guerra).');
  E('carrera', 'cargo_general', 'estado', 'Fue general del ejército de {estado}.');
  E('carrera', 'cargo_ministro', 'estado', 'Fue ministr{o|a} de {estado}: papeles, impuestos, intrigas.');
  E('carrera', 'cargo_arzobispo', 'estado', 'Encabezó la iglesia de {estado}.');
  E('carrera', 'cargo_jefe_guardia', 'estado', 'Fue jef{e|a} de la Guardia de {estado}: la policía política del régimen.');
  E('carrera', 'cargo_capitan', 'nave', 'Fue capitán de la {nave}, una nave mercante.');
  E('carrera', 'cargo_pirata', 'nave', 'Fue capitán pirata de la {nave}.');
  E('carrera', 'cargo_corsario', 'nave estado', 'Fue corsari{o|a} con patente de {estado}, al mando de la {nave}.');
  E('carrera', 'cargo_cabecilla', 'faccion', 'Encabezó {faccion} (una facción: rebeldes, sindicato, orden…).');
  E('carrera', 'cargo_predicador', '', 'Fue predicador{|a}: iba de puerto en puerto anunciando profecías.');
  E('carrera', 'cargo_mercader', 'faccion', 'Fue cabeza de {faccion}, una casa mercante.');
  E('carrera', 'cargo_funcionario', 'oficio lugar', 'Tuvo un cargo menor: {oficio} en {lugar}.');
  E('carrera', 'cargo_ninguno', 'lugar', 'No tuvo cargo: una persona corriente de {lugar}.');
  E('carrera', 'hecho_batalla_gana', 'sistema enemigo n', 'Ganó la batalla de {sistema} contra {enemigo}; murieron {n} enemigos.');
  E('carrera', 'hecho_batalla_pierde', 'sistema enemigo', 'Perdió la batalla de {sistema} contra {enemigo}.');
  E('carrera', 'hecho_conquista', 'lugar', 'Sus tropas tomaron {lugar} casa por casa.');
  E('carrera', 'hecho_defensa', 'lugar', 'Defendió {lugar} de un asalto y aguantó.');
  E('carrera', 'hecho_masacre', 'lugar n', 'Mandó disparar contra la gente en {lugar}: {n} muertos.');
  E('carrera', 'hecho_reprime', 'lugar', 'Aplastó una insurrección armada en {lugar}.');
  E('carrera', 'hecho_revolucion', 'lugar', 'Encabezó una revolución que triunfó en {lugar}.');
  E('carrera', 'hecho_insurreccion_falla', 'lugar', 'Encabezó una insurrección en {lugar} que fue aplastada.');
  E('carrera', 'hecho_golpe', 'estado', 'Dio un golpe y tomó el poder en {estado}.');
  E('carrera', 'hecho_purga', 'n', 'Purgó a {n} rivales para que nadie le disputara el poder.');
  E('carrera', 'hecho_guerra', 'enemigo', 'Declaró la guerra a {enemigo}.');
  E('carrera', 'hecho_abre_deposito', 'lugar', 'Se negó a disparar contra la multitud hambrienta de {lugar} y abrió el depósito de grano.');
  E('carrera', 'hecho_funda', 'faccion lugar', 'Fundó {faccion} en {lugar}: empezó juntando a gente descontenta a puerta cerrada.');
  E('carrera', 'hecho_atentado_sufre', '', 'Sobrevivió a un atentado contra su vida.');
  E('carrera', 'hecho_ejecuta', 'victima', 'Mandó ejecutar a {victima}.');
  E('carrera', 'hecho_venganza_jura', 'victima', 'Juró vengar la muerte de {victima} y lo apuntó todo en una libreta.');
  E('carrera', 'hecho_venganza_cumple', 'victima', 'Encontró y mató a quien buscaba: {victima}.');
  E('carrera', 'hecho_abordajes', 'n', 'Abordó {n} naves.');
  E('carrera', 'hecho_profecia', '', 'Anunció una profecía que acabó cumpliéndose.');
  E('carrera', 'hecho_corrupcion', '', 'Aceptó sobornos; miró hacia otro lado.');
  E('carrera', 'hecho_informe_miente', 'lugar', 'Maquilló los informes que mandaba a la capital sobre {lugar}.');

  // Grupo «muerte» → nucleo/frases/30-muerte.js
  E('muerte', 'muerte_natural', 'edad lugar', 'Murió de muerte natural a los {edad} años en {lugar}.');
  E('muerte', 'muerte_combate', 'lugar', 'Murió en combate en {lugar}.');
  E('muerte', 'muerte_ejecucion', 'verdugo', 'Fue ejecutad{o|a} por orden de {verdugo}.');
  E('muerte', 'muerte_purga', 'verdugo', 'Cayó en una purga de {verdugo}, que no quería rivales.');
  E('muerte', 'muerte_linchamiento', 'lugar', 'La multitud l{o|a} sacó a rastras y l{o|a} mató en {lugar}.');
  E('muerte', 'muerte_atentado', 'verdugo', 'L{o|a} mataron en un atentado; fue {verdugo}.');
  E('muerte', 'muerte_silencio', '', 'Apareció muert{o|a}; pareció natural y nadie supo más.');
  E('muerte', 'muerte_venganza', 'verdugo', 'L{o|a} mató {verdugo}, que l{o|a} llevaba años buscando para vengarse.');
  E('muerte', 'muerte_veneno', '', 'Murió envenenad{o|a}.');
  E('muerte', 'muerte_explosion', 'lugar', 'Murió en una explosión en {lugar}.');
  E('muerte', 'muerte_peste', 'lugar', 'Se l{o|a} llevó la peste en {lugar}.');
  E('muerte', 'muerte_nave', 'nave', 'Se perdió con su nave, la {nave}.');
  E('muerte', 'cadena_1', 'hecho', 'Por qué murió, causa inmediata. {hecho} es una oración completa ya escrita, en minúscula, sin punto final.');
  E('muerte', 'cadena_2', 'hecho', 'La causa de la causa («y eso venía de…»). {hecho} igual que arriba.');
  E('muerte', 'cadena_3', 'hecho', 'El origen remoto de todo («en el principio de aquel hilo…»). {hecho} igual.');
  E('muerte', 'cadena_mano', '', 'Nadie supo nunca por qué pasó: no hay causa en este mundo que lo explique.');
  E('muerte', 'edad_joven', 'edad', 'Comentario: murió joven, a los {edad}.');
  E('muerte', 'edad_plena', 'edad', 'Comentario: murió en la plenitud, a los {edad}.');
  E('muerte', 'edad_vieja', 'edad', 'Comentario: murió vie{jo|ja}, a los {edad}.');
  E('muerte', 'legado_nada', '', 'Nada cambió con su muerte.');
  E('muerte', 'legado_sucesion', 'heredero', 'L{o|a} sucedió {heredero}.');
  E('muerte', 'legado_caos', 'estado', 'Su muerte dejó {estado} sin cabeza: interregno, bandos, guerra entre los suyos.');
  E('muerte', 'legado_martir', 'n', 'Su muerte encendió la calle en {n} mundos: pasó a ser un mártir.');
  E('muerte', 'legado_fiesta', 'fiesta', 'Se le recuerda cada año con una fiesta: {fiesta}.');
  E('muerte', 'legado_memorial', 'lugar', 'Hay un memorial en {lugar} con los nombres de los que murieron por su orden.');
  E('muerte', 'legado_hijos', 'hijos', 'L{o|a} sobreviven sus hijos: {hijos}.');
  E('muerte', 'legado_venganza', 'vengador', '{vengador} juró vengarl{o|a}.');
  E('muerte', 'legado_epoca', 'epoca', 'Una época entera lleva su marca: {epoca}.');
  E('muerte', 'cuenta_muertes', 'n', 'Balance: {n} personas murieron bajo su mando o por orden suya.');
  E('muerte', 'cuenta_manos', 'n', 'Balance: mató a {n} personas con sus propias manos.');
  E('muerte', 'vive', 'edad lugar', 'Sigue viv{o|a}: tiene {edad} años y está en {lugar}.');
  E('muerte', 'vive_preso', 'lugar', 'Sigue viv{o|a}, pres{o|a} en {lugar}.');
  E('muerte', 'cierre', '', 'Frase final de la biografía: balance de una vida.');

  // Grupo «figuras» → nucleo/frases/40-figuras.js
  E('figuras', 'figura_caudillo', 'faccion n', 'Se convirtió en caudillo: {faccion} l{o|a} sigue y su nombre se pronuncia en {n} mundos.');
  E('figuras', 'figura_profeta', 'credo n', 'Se convirtió en profeta: fundó {credo}, que tiene fieles en {n} mundos.');
  E('figuras', 'figura_martir', 'n', 'Muert{o|a}, se volvió mártir: {n} mundos se echaron a la calle al saberlo.');
  E('figuras', 'figura_invicto', 'n', 'Almirante invict{o|a}: {n} batallas, ninguna perdida.');
  E('figuras', 'figura_reformador', 'estado', 'Gobernante reformador{|a} de {estado}: bajó tributos y abrió depósitos.');
  E('figuras', 'figura_tirano', 'estado n', 'Tiran{o|a} de {estado}: {n} purgados, miedo en todas partes.');
  E('figuras', 'figura_genio', 'estado', 'Genio de la ingeniería: sus diseños dieron a {estado} una generación de ventaja.');
  E('figuras', 'figura_rey_pirata', 'n', 'Rey pirata: {n} naves bajo su bandera.');
  E('figuras', 'figura_unificador', 'n estado', 'Unificador{|a}: sumó {n} mundos a {estado} y los hizo sentirse parte de él.');
  E('figuras', 'figura_sanador', 'lugar', 'Sanador{|a}: en la peste de {lugar} cuidó a los enfermos cuando todos huían.');
  E('figuras', 'figura_rubicon', 'estado', 'Volvió su flota contra la capital de {estado}: los soldados le eran leales a {él|ella}, no al Estado.');
  E('figuras', 'figura_traidor', 'lugar', 'Traidor{|a}: abrió las puertas de {lugar} al enemigo.');
  E('figuras', 'ev_brote', 'lugar', 'CRÓNICA. Empieza una peste en {lugar}.');
  E('figuras', 'ev_peste_muertos', 'lugar n', 'CRÓNICA. La peste ha matado ya a {n} personas en {lugar}.');
  E('figuras', 'ev_cuarentena', 'lugar', 'CRÓNICA. {lugar} cierra el puerto por la peste: cuarentena.');
  E('figuras', 'ev_peste_fin', 'n', 'CRÓNICA. La peste se apaga; deja {n} muertos en total.');
  E('figuras', 'ev_arsenal', 'lugar n', 'CRÓNICA. Revienta en cadena el almacén de núcleos de {lugar}: {n} muertos.');
  E('figuras', 'ev_llamarada', 'sistema n', 'CRÓNICA. Una llamarada de la estrella de {sistema} obliga a evacuar: {n} personas huyen.');
  E('figuras', 'ev_hallazgo', 'lugar estado', 'CRÓNICA. En {lugar} aparece una máquina antigua; {estado} se la queda y sus ingenieros aprenden de ella.');
  E('figuras', 'ev_oleada', 'credo n', 'CRÓNICA. El día señalado, los fieles de {credo} se alzan a la vez en {n} mundos.');
  E('figuras', 'ev_martirio', 'martir n', 'CRÓNICA. La muerte de {martir} enciende la calle en {n} mundos.');
  E('figuras', 'ev_guerra_santa', 'estado enemigo credo', 'CRÓNICA. {estado} declara la guerra santa a {enemigo} en nombre de {credo}.');
  E('figuras', 'ev_conversion', 'estado credo', 'CRÓNICA. Quien gobierna {estado} abraza {credo} y lo hace fe del Estado.');
  E('figuras', 'ev_credo_nace', 'credo lugar fundador', 'CRÓNICA. {fundador} empieza a predicar {credo} en {lugar}.');
  E('figuras', 'epoca_guerra', 'clave', 'NOMBRE de una época dominada por la guerra; {clave} es un Estado o un lugar.', 'nombre');
  E('figuras', 'epoca_hambre', '', 'NOMBRE de una época de hambrunas.', 'nombre');
  E('figuras', 'epoca_revuelta', '', 'NOMBRE de una época de revueltas y revoluciones.', 'nombre');
  E('figuras', 'epoca_paz', '', 'NOMBRE de una época tranquila, sin grandes sobresaltos.', 'nombre');
  E('figuras', 'epoca_peste', '', 'NOMBRE de una época marcada por una peste.', 'nombre');
  E('figuras', 'epoca_figura', 'clave', 'NOMBRE de una época marcada por una persona; {clave} es su nombre.', 'nombre');
  E('figuras', 'epoca_caos', '', 'NOMBRE de una época en la que pasó de todo a la vez.', 'nombre');
  E('figuras', 'con_poco_despues', '', 'CONECTOR de tiempo corto, para empezar la frase siguiente («Poco después,»).', 'conector');
  E('figuras', 'con_anios_despues', 'n', 'CONECTOR de tiempo largo («Pasaron {n} años.» o «{n} años más tarde,»).', 'conector');
  E('figuras', 'con_giro', '', 'CONECTOR de giro o contraste («Pero»; «Y entonces»).', 'conector');
})(typeof globalThis !== 'undefined' ? globalThis : this);
