# Pendientes (funcionalidad)

Lista viva de lo que Fernando ha pedido y aún no está hecho. Lo de rendimiento va en
[`OPTIMIZACION.md`](OPTIMIZACION.md).

**Regla para todos los agentes:** si haces algo de esta lista, táchalo en el mismo cambio, con la
fecha y una línea de cómo quedó. Si queda a medias, apunta qué falta.

## V40 (2026-10-07): prueba de coherencia, ordenador de vuelo, teclas, mochila, piloto automático

Fernando: el Azor «se inclina hacia abajo» y lleva los giróscopos siempre saturados; Mayús no sube
los gases (hay que arrastrar la palanca) y Ctrl en las ruedas «sube de 5 a 40»; SEGUIR hace cosas
raras; ¿qué hace el piloto automático con una altura puesta en el espacio?; la mochila tiene que
ir como en Space Engineers. Pidió «una mega prueba de coherencia de cada mando, cada cosa» y que lo
aprendido valga para todas las naves. Hecho ([`NAVES.md`](NAVES.md) «Coherencia y ordenador de
vuelo (V40)», [`COMBATE.md`](COMBATE.md), [`MOVIMIENTO.md`](MOVIMIENTO.md) §8):

- **`crates/ship/tests/coherencia.rs`**, para toda nave que vuela, sin nombrar ninguna: teclas
  mantenidas a 30/60/144/240 fps, pasos de ruedas (normal, Mayús, Ctrl) contra datos y ayuda,
  retenes de palancas, cada tecla de la palanca por su eje y su sentido (y la misma en todas las
  naves), potencia sin giro en cada posición de las góndolas, piloto automático fuera de todo
  cuerpo. Lo que encontró al principio: 46 pasos de rueda o retenes incoherentes, 8 teclas que
  no movían los gases, 12 casos de nave que se gira sola con gas (el Azor 25° en 30 s, el
  Abejorro 21°), 18 de teclas que mueven más de un eje o que el estabilizador no paraba, la
  guiñada del Alcotán que alabeaba, y el piloto sin manera de decir «aquí no puedo».
- **Por qué el Azor se iba de morro**: sus góndolas empujan 4 mm detrás del centro de masas
  (unos 440 N·m), los giróscopos se llenaban en 3 s y no se descargaban, y el reparto entre
  toberas no las encendía nunca (40 pasos de gradiente que no llegaban y un corte al 2 %). Ahora:
  reparto exacto por coordenadas, compensación por el empuje real, giróscopos que se descargan,
  reparto entre motores que no cambia alabeo por un cabeceo imposible, y el estabilizador
  mantiene la actitud con la palanca suelta. Con el gas al 60 % y la palanca suelta ninguna nave
  se gira más de 0,1° en 30 s (antes hasta 25°).
- **Teclado**: un retén atrapa la palanca que entra, no la que sale (Mayús y Ctrl mueven los
  gases igual a cualquier fps); la velocidad de giro de la rueda se mide (`Spin`); Ctrl es
  siempre el paso fino; las teclas de un asiento en un solo sitio (`ship/src/seat_keys.rs`).
- **Piloto automático**: `ap.sin_cuerpo` (lámpara ámbar y «SIN CUERPO» en la pantalla) cuando lo
  pedido no tiene sentido fuera de todo cuerpo; el peso que sostiene es el de ir como va (en
  órbita, nada); SEGUIR con margen entre cerrar y apuntar, sin dar media vuelta por un empuje de
  través, frenando contando lo que tarda en responder.
- **Mochila tipo Space Engineers**: W empuja hacia donde miras (también arriba o abajo), sin peso
  el ratón gira todo el cuerpo sin tope, Q y E alabean, al apagarla te quedas como estabas, con
  peso te enderezas, y el estabilizador frena también en el espacio libre (respecto a la
  estructura más cercana o al marco de los cuerpos). La metralleta de pruebas pasa de Q a U.

- **Entrar al Azor de pie**: patas 0,9 m más largas (la panza a 2,1 m), dos compuertas en la
  panza (`panza_izq`, `panza_der`, motorreductores en el circuito de mecanismos) que se abren
  antes de que se mueva la plataforma y se cierran cuando ha subido (`plataforma.pide`,
  `panza.orden`, `plataforma.orden`), plataforma hasta el suelo (2,35 m) con mangas
  telescópicas en los mástiles, botón ASIENTO en el reposabrazos derecho (panel `azor_asiento`,
  baja con el asiento) y tecla P sentado. Modelos de los mástiles y las patas regenerados con
  Blender (`tools/modelos/hacer.py`). `coherencia.rs` comprueba en toda nave que un asiento que
  se mueve tiene a mano lo que lo mueve en cada extremo, y una tecla.

- **El Azor más épico**: alas en flecha de 11,2 m de envergadura (antes 5,4) en dos paneles por
  lado con la góndola girando en el hueco, canards, derivas más altas, estabilizadores más
  anchos, DARDO en los paneles exteriores, luces en las puntas, panel exterior en la pata. Para
  ello los estilos de Blender visten también envolventes de puntos (`shape_key` de `hull` en Rust
  y en Python) y hay un estilo nuevo, `ala_flecha`, para cualquier nave.

- **Repaso de lo que dependía de los fotogramas** (Fernando: «a lo mejor lo del porcentaje y
  los fotogramas pasa en más sistemas»): las teclas mantenidas del asiento (arreglado, arriba);
  la vista que sigue al ordenador de muñeca se suavizaba con un factor lineal por fotograma (ahora
  exponencial, `play.rs`); la inercia de las ruedas depende de su paso pero corre al tic fijo de
  la nave (50 Hz), no al fotograma; el jugador y lo que vive entre estructuras van por lonchas del
  mundo (`MOVIMIENTO.md`). No se encontró nada más con un umbral o un redondeo por fotograma.

- **Dos cosas de jugabilidad** (Fernando: «el brazo en las cabinas puede tapar controles»; «con
  el lanzacohetes el codo izquierdo se mete en el pecho»). El cuerpo se mide de su propia malla
  (`app/src/bulk.rs`: el frente del tronco y el grosor de cada hueso del brazo), así que vale
  para cualquier traje:
  - **El brazo que tapa lo que miras se transparenta**: con la mira sobre tu propio brazo (a tus
    ojos, en cualquier sitio), ese brazo entero se dibuja como una trama al 25 % y se ven los
    mandos de detrás; vuelve en un cuarto de segundo. Por hueso (`BodyScene::fades`), en el
    shader de cuerpos con trama ordenada (sin ordenar transparencias); la sombra sigue entera.
  - **El codo rodea el tronco**: el giro del codo (`body.rs`, `ease`) cuenta lo que el brazo se
    mete en el tronco del lado contrario, con la mano ya asentada en su tope de muñeca. Con el
    lanzacohetes el brazo izquierdo se metía 12 cm en el pecho; ahora 0 mirando recto, abajo y
    arriba (`handwork.rs`, `an_arm_on_a_tool_goes_round_the_trunk_not_through_it`). Medido: el
    cuerpo entero con las dos manos forzadas sigue en unos 42 µs por fotograma.
  - Fotos: `tools/camara/brazo.jsonc`, `tools/camara/codo.jsonc`.

- **Multijugador: naves, disparos y daño** (Fernando: «verifícame todo en multijugador… disparos
  entre naves a alta velocidad, verse volar bien»). Detalle en [`MULTIJUGADOR.md`](MULTIJUGADOR.md)
  «En el juego»:
  - las naves ajenas se **guían** hacia donde dice su dueño (semivida 80 ms, igual a cualquier
    fps) en vez de ponerse: sin saltos a ninguna velocidad, y quien va dentro va con ellas;
  - **daño con eco**: lo que se le hace a una estructura que tienen todas las partidas se manda y
    se aplica en todas (también en la que disparó) en el orden del servidor y con la semilla del
    mensaje: el mismo daño bit a bit. Las armas de naves ajenas disparan gemelos que se ven y no
    hacen daño;
  - los disparos de los jugadores van junto a la nave en la que van (salen de la boca a 7,8
    km/s); quien flota junto a una nave se cuenta en el marco de esa nave;
  - el reloj: media de los pings más rápidos, corrección a 0,5 ms/s, pings deprisa hasta
    asentarse; lo llevado al presente cuenta la aceleración;
  - puertas y anclajes a mano viajan (`ACT`): antes se apuntaban en una lista que nadie leía y
    que crecía sin fin;
  - pruebas en `app/src/multi/tests.rs`: partidas enteras contra el servidor real en una red que
    pierde, duplica y retrasa; vuelo de 0 a 7800 m/s a 30–240 fps, tres partidas disparándose a
    2 km/s con el mismo daño, dos tiradores a la vez, cohete a velocidad orbital, jugador
    flotando, pasajero de pie, mandos, puertas, naves puestas y dueños (~30 s todas).

- **Un solo sistema de proyectiles y sincronía exacta** (Fernando: «todo escalable, para no tener
  que ir tocando en multijugador; la sincronización debería ser perfecta; si desde una nave abro la
  bodega debería poder meter cohetazos a cualquier nave, con el arma que sea; el sistema de
  proyectiles centralizado»). Detalle en [`MULTIJUGADOR.md`](MULTIJUGADOR.md) «En el juego»:
  - todo lo que se dispara o estalla es un `Launch` y sale por `Blasts::launch` (mano, naves,
    pruebas, guiones); un arma nueva es solo datos. Se acabaron los «gemelos» que cada partida
    disparaba desde su copia de la nave ajena: las armas de una nave las dispara su dueño, y cada
    lanzamiento y cada final se cuentan una vez (`Seen::Launch`/`End`/`Track`), en el marco de lo
    que lo soltó o de lo que golpeó;
  - lo ajeno no decide nada y estalla donde lo dice su partida; un guiado ajeno lo lleva el empuje
    de su dueño;
  - el daño, igual bit a bit en todas aunque la copia esté dormida, con lo articulado en otra
    postura o con depósitos que revientan: el golpe viaja con la postura de lo golpeado; lo que
    revienta lo dice el dueño; los trozos tienen nombre en la red y se comparten;
  - las copias de las naves siguen también la aceleración del dueño (de 6–12 cm a 3–6 cm en
    formación, sin quedarse atrás en los virajes);
  - de paso: el fondo de la partida de pruebas no volaba los misiles; los mensajes vistos se
    colocaban un fotograma tarde y antes de corregir las copias; lo que revienta usaba una semilla
    distinta en cada partida.
  - **Queda:** una bala rápida contra un blanco cercano puede detectarse un paso tarde o
    atravesarlo (encontrado por la prueba de todas las armas, que recorre todo lo que hay en los
    datos; en ello); dónde está cada trozo no se sincroniza.

- **Teclas a gusto de cada uno** (Fernando: «las acciones con la F ok, pero mete cambiar
  controles… usar las flechas para algo de la nave… también tenemos numpad»):
  - las teclas son datos: `assets/defs/controles.jsonc` (las del juego y sus perfiles) y
    `ajustes/controles.jsonc` (lo que cambia el jugador; solo las diferencias). Lo que hace cada
    acción sigue en el código (`input::ACTIONS`); qué teclas, en datos;
  - menú Esc → CONTROLES: cada acción con «Cambiar» (pulsa la tecla; Esc deja, Retroceso quita)
    y «+» (otra más), perfil y «Restablecer todo». Una tecla dada se le quita a lo que la tenía
    y se dice;
  - **órdenes de vuelo**: los asientos ya no nombran W, S, Mayús…: nombran una orden
    (`cabecear_abajo`, `gases_mas`, `disparar`…, `input::ORDERS`) y la tecla es la del jugador,
    igual en todas las naves. Solo lo propio de una nave lleva tecla fija (P la plataforma del
    Azor, G el agarre del Abejorro);
  - por defecto: **F usar** (E queda solo para alabear con la mochila), las **flechas** trasladan
    la nave, el **teclado numérico** es la palanca (8/2 cabeceo, 4/6 guiñada, 7/9 alabeo, +/-
    gases, 0 cortar, Intro disparar, 5 fijar), Re Pág / Av Pág suben y bajan; vuelo libre de
    pruebas pasa de F a F9;
  - perfil **«Como Space Engineers»**: W/S/A/D trasladan la nave, R/C suben y bajan, Q/E
    alabean, las flechas apuntan el morro, X la mochila;
  - pruebas: las teclas del juego y de cada perfil sin choques, los nombres de teclas, cambiar
    y guardar y volver a leer, cada orden de cada asiento existe y no pisa una tecla propia, y
    sentado las flechas y el teclado numérico mueven los mandos. Foto: `tools/camara/controles.jsonc`;
  - de paso: el juego en debug abortaba al abrir el menú desde un guion (egui pide soltar a
    propósito las texturas de un frame: `UiFrame` lo hace al tirarse), y en la cabecera del menú
    se pisaban dos textos en ventanas estrechas.

Queda de esto:

- **Nadie lo ha jugado.** Todo está comprobado con pruebas; los .exe de `SELENE_V39` no se han
  recompilado (aquí no hay Windows): hay que compilar una V40.
- La prueba de coherencia no vuela todavía con gravedad ni mira cada mando de cada panel uno a uno
  contra su efecto (eso lo hacen a su manera `logica_mandos.rs` y `uso.rs`); se puede ampliar.
- El Abejorro lleva la descarga de sus giróscopos en un interruptor (DESCARGA, apagado al salir):
  con gas y sin descargar los llena hasta la mitad. Es su diseño; si molesta, encenderlo por
  defecto.
- La mochila sin peso: hacia dónde «se endereza» al volver a una nave o a un cuerpo es el giro de
  siempre (`right`), a su ritmo; falta sentirlo jugando.

## V39 (2026-10-06): un solo reloj. La mochila y las naves a cualquier velocidad, de raíz

Fernando: con la mochila dentro de una nave «a toda hostia» se teletransportaba, al salir pegaba
un tirón y luego la nave parpadeaba; «debe funcionar independientemente de la velocidad, de la
posición, de todo, y debe ir mega suave». La V38 lo compensó dentro del jugador; pidió entonces
que fuera «sistemático y modular, nada hardcodeado», arreglado «de raíz». Hecho
([`MOVIMIENTO.md`](MOVIMIENTO.md)):

- **Un solo reloj.** El mundo da el paso a lo que vive entre sus estructuras (`Among`,
  `Structures::simulate_with`), loncha a loncha y justo después de ellas. El jugador ya no tiene
  acumulador, pasos fijos ni interpolación, ni nada de lo que la V38 añadió para compensarlos.
- **Física sin dependencia de la velocidad** (`core/structure/physics.rs`): la velocidad de cada
  cuerpo se guarda entera (un empuje pequeño cuenta igual a velocidad orbital) y lo que un cuerpo
  lleva sujeto es ese cuerpo para lo que choca con ello.
- **El jugador en módulos** (`app/src/pilot/`: `mod.rs`, `walk.rs` el cuerpo entre piezas,
  `pack.rs` la mochila) y **sus números en datos** (`scenario.jsonc`: `player.cuerpo`,
  `player.mochila`, `player.linterna`). Lo que se toca te para contra sí mismo; las botas frenan
  respecto a lo que pisas; la velocidad es un vector entero que se reparte en nivel y arriba cada
  paso; en las salas de una nave, la nave te lleva, la hayas pisado o no.
- El estabilizador de la mochila sigue a lo que va a tu lado tal como va ahora (cae con ello).
- Pruebas: núcleo (`tests/schedule.rs`, `tests/physics.rs`) y jugador (`pilot/tests.rs`), de 0 a
  7800 m/s y de 10 a 240 fps, con las estructuras a mano y con la física real.

Queda de esto:

- Nadie lo ha jugado todavía: comprobado con pruebas y con `tools/camara/mochila_nave.jsonc`.
  Al no haber pasos fijos, andar, saltar y la mochila se dan ahora en lonchas de hasta 1/60 s que
  cambian con el fotograma: los números son los mismos, pero hay que sentirlo jugando.
- ~~A bordo se pesa lo que pesa el cuerpo celeste de debajo aunque la nave vaya en caída libre
  (dentro de una nave en órbita no se flota).~~ **Hecho el 2026-10-06** (abajo, «lo que rige en
  cada sitio»): lo que se pesa a bordo lo dice lo que te lleva; en una nave sin gravedad propia
  que cae, se flota.
- ~~El resto de lo que se mueve ya iba con el reloj del mundo.~~ **Corrección 2026-10-06:**
  no era cierto para los proyectiles: los de `shots.jsonc` ahora usan `Among` y barridos entre
  poses de una misma loncha. La carga ya es física de estructuras. Falta trasladar también
  misiles estratégicos/guiados y revisar el resto de emisores; las explosiones de los cohetes
  ya heredan también el movimiento. La réplica de jugadores no se revisó aquí.
- ~~`crates/app/src/multi/old.rs` es una copia vieja que no se compila: borrarla.~~ Borrada
  (2026-10-07), con `things.rs` y `kinds.rs`, que tampoco se usaban.

## V39 (2026-10-06): cohetes y cajas a velocidad orbital

- ~~El cohete del lanzador no sale contigo ni toca bien las paredes de una nave rápida.~~
  Hereda la velocidad de la boca (también su giro), nace exactamente allí y vuela con el mundo
  por `Among`. El contacto usa las poses inicial y final de la nave y guarda el punto local.
- ~~Al llevar una caja fuera de una nave el agarre frena contra el mundo.~~ Ahora usa el
  movimiento de la mano; soltar conserva la velocidad y el giro de la caja.
- ~~La explosión de un cohete queda atrás en una nave rápida, aunque el contacto sea correcto.~~
  Detectado en las fotos: nube, luz y sacudida nacían sin velocidad. Ahora heredan la del punto
  alcanzado mediante el contrato genérico `Motion`, sin estirarse ni frenarse contra el mundo.
- Pruebas: `MOVIMIENTO.md` §12. Controles negativos ejecutados: boca desplazada, velocidad
  omitida, rayo sin pose inicial, amortiguación contra el mundo y explosión sin herencia
  producen fallos reales. Batería completa: **739 pasadas, 3 omitidas, 0 fallos**.
- Entrega comprobada: recompiladas las tres ediciones de `SELENE_V39`; los tres
  `--prueba-arranque` y el guion `tools/camara/cohetes_nave.jsonc` desde esa carpeta terminan
  con código 0, sin stderr. Leídas sus nueve fotos y el registro: sale por la boca en los
  tres casos; contactos 1, 2 y 3 contra la pared/casco; la explosión interior rápida ya no
  queda atrás. La etapa exterior cae hacia la Luna bajo su influencia; no es una prueba
  visual de espacio sin gravedad (eso sí lo barre la prueba numérica).
- Pendiente de tacto humano: disparos y cajas en una partida jugada a mano y entre dos clientes
  de red. La reproducción `Seen::Round` se comprueba numéricamente, no en una sesión remota.
- Se mantienen abiertos los misiles estratégicos/guiados, el paso de partículas por `Among`
  y los demás emisores, CCD exacto de piezas articuladas y gravedad artificial para toda la
  carga/proyectiles. La explosión móvil del cohete sí está corregida; su cohete pertenece a
  `shots.jsonc` y comparte las lonchas del mundo.
- La orientación de los conos visuales de la explosión sigue usando la vertical del cuerpo
  de dibujo: en la nave volcada su nube sale hacia el otro lado de la cubierta. Pendiente
  darle una orientación de emisión propia del impacto, sin cuerpo celeste en espacio libre.

## V39 (2026-10-06): lo que rige en cada sitio. Gravedad, suelo, arriba y brújula sin dependencias

Fernando: nada puede depender de estar en la Luna, de la velocidad, de la posición, de los
fotogramas, de dónde nació una cosa ni de lo que pasó antes; la gravedad debe perderse bastante
antes que en el mundo real; a bordo la referencia es la nave; la brújula tiene que significar
algo en cada sitio. Hecho ([`MOVIMIENTO.md`](MOVIMIENTO.md) §6-11):

- ~~**A. Influencia de cada cuerpo.**~~ `BodyRegistry::field(p)` es la única función que dice
  qué gravedad, qué suelo y qué arriba hay en un punto. Cada cuerpo declara su alcance y su
  franja (`reach` en `bodies/*.jsonc`: la Luna entera hasta 20 km y nada desde 30; la Luna
  menor, 3,5 y 6 km). Fuera de todo: sin gravedad, sin suelo, sin arriba. Donde se juntan dos,
  el que llega menos lejos releva al otro, suave.
- ~~**B. Las naves se quedaban con el cuerpo donde nacieron.**~~ `Structure::body` ya no existe:
  la física, el durmiente que se pone al día, los mecanismos que se topan y el ordenador de cada
  nave preguntan por donde está la cosa ahora. El parche de suelo guardado lleva su cuerpo.
- ~~**C. El arriba del jugador.**~~ Es hacia donde pesa, y lo que pesa lo dice lo que lo lleva:
  la gravedad propia de la nave en sus salas (dato `gravedad` de cada nave, con la señal que la
  alimenta), o el tirón del sitio menos lo que acelera lo que lo lleva. Sin peso, el que traía.
  El cuerpo se endereza a su ritmo (dato), más despacio cuanto menos pesa. Su frente es un marco
  propio, sin ningún eje del mundo.
- ~~**D. La brújula.**~~ Sistema de referencias por régimen declarado en `navegacion.jsonc`
  (superficie: norte del cuerpo; órbita: marcha, radial, normal; espacio: la nave y el sol), con
  marcas que se funden según pese cada régimen y sin puntos singulares.
- **De paso (lo destapó el guion):** qué estructuras se simulan en fino dependía de dónde está
  la cámara, no de dónde está el jugador: con la cámara lejos, la nave de al lado se quedaba un
  fotograma atrás (4,4 m a 600 m/s). Ahora quien vive entre estructuras es un observador más
  (`Among::at`).
- Pruebas que barren cuerpos (los dos y uno inventado), sitios, velocidades, fotogramas y
  posturas de la nave: `core/tests/field.rs`, `app/src/pilot/frames.rs`, `app/src/nav/tests.rs`,
  `ship/tests/gravedad.rs`. Guion en el juego: `tools/camara/espacio.jsonc`.

**Decisiones de juego tomadas por mi cuenta** (todas son datos; cambiarlas es cambiar un número):

- La Luna deja de tirar a 30 km (`reach`). El tráfico en órbita va ahora entre 12 y 19 km.
- El Alcotán y el Cachalote dan 1,62 m/s² a bordo mientras tengan barra esencial; el Abejorro y
  el Azor, nada. La gravedad propia vale **en las salas** de la nave, no sobre su casco.
- **Sin peso no se anda: se flota** (`cuerpo.sin_peso` 0,05 m/s²). Dentro de una nave apagada
  en órbita se flota; en una con gravedad se anda aunque vaya boca abajo.
- La mochila sin gravedad empuja según el arriba que traigas, y sin suelo ni nave cerca no te
  frena (no hay respecto a qué).
- Qué cuenta como «órbita» en la brújula: ir de lado entre el 45 % y el 85 % de la velocidad de
  órbita circular de donde estés.

Queda de esto:

- **Nadie lo ha jugado.** Comprobado con pruebas y con el guion (fotos y registro), no a mano.
  En particular: cómo se siente enderezarse al entrar en una nave inclinada, y flotar dentro de
  una nave sin corriente.
- **La carga suelta dentro de una nave no siente la gravedad propia de la nave.** La física de
  estructuras solo aplica el tirón de los cuerpos: en una nave con gravedad en órbita, tú andas
  y un bidón suelto flota. Hace falta que `physics.rs` pregunte `weight::felt` para lo que esté
  en las salas de otra estructura (y que la nave reciba la reacción). Las partículas, igual.
- **Sin peso no hay con qué impulsarse**: ni agarrarse a asideros ni empujarse con las piernas.
  Sin mochila, flotando en una cabina, solo te para lo que toques.
- **Otros jugadores en red:** el protocolo solo lleva un giro y un cuerpo por jugador, así que
  en espacio libre o a bordo de una nave volcada el cuerpo de los demás se dibuja con la
  vertical del cuerpo más cercano. Hay que mandar el arriba. El byte `body` de las cosas ya no
  significa nada (se manda 0): quitarlo del protocolo.
- El guion pone la nave en cada sitio (`poner`); no la vuela de un sitio a otro con sus motores.
- Las partículas guardan el suelo del cuerpo sobre el que nacieron (viven segundos).
- El cuerpo se endereza girando alrededor de los ojos: durante el giro los pies barren (no se
  ve en primera persona; desde fuera, con V, sí).
- `Field::nearest` / `BodyRegistry::dominant` siguen existiendo para dibujar (terreno, cámara
  exterior, polvo). Quien los use para otra cosa está reintroduciendo la dependencia.

## V39 (2026-10-06): piloto automático que sirve

Fernando, tras volar el Azor: el piloto de combate «se vuelve loco» al apuntar, la MIRA «no hace
nada», y quiere seguir traza, velocidad, rumbo y altura en automático, como en la versión web.
Hecho ([`COMBATE.md`](COMBATE.md) §Piloto automático; `crates/ship/tests/piloto.rs` lo vuela):

- Las trazas giran con la nave entre barridos (`tactical.rs` `stood`): era la causa de las
  piruetas. El giro del piloto va por el horizonte, de lado y luego arriba, sin alabear.
- Retenciones que se apilan (ALTURA, RUMBO, VELOC.) y programas (FRENAR, SEGUIR, DESPEG.,
  ATERRIZ.); el ordenador mueve las góndolas vectoriales (`vuelo.vector`), también hacia atrás.
- Las toberas ya no reciben más empuje pedido del que pueden dar (`flight.rs` `RCS_SHARE`): con
  los motores girando para frenar la nave daba la vuelta.

Queda:

- **Nadie lo ha volado en el juego** con esto: solo las pruebas.
- Góndolas que mueve el ordenador en el Alcotán y el Cachalote (una derivada en sus datos).
- Ir a un punto del mapa (NAV), subir a órbita y bajar de ella, como en la web.
- Un visor de tiro en el cristal (hoy la MIRA es una página de la pantalla).
- A 80 m/s la altura retenida sigue el relieve con retraso: un cortado de 50 m se nota.

## V38 (2026-10-06): el Azor y el combate

Hecho ([`COMBATE.md`](COMBATE.md); pruebas en `crates/ship/tests/azor.rs` y las genéricas):

- Nave nueva, el **Azor**: caza monoplaza con asiento que baja por la panza, dos compuertas a
  popa, tambor de tres paneles, grupo de energía unificado, blindaje.
- Sistemas genéricos nuevos: táctico (radar con escala, alertador, infrarrojo, transpondedor,
  perturbador, trazas, fijar cualquier traza), armas montadas (cañón, misiles guiados, señuelos),
  piloto automático de navegación y de combate, perfil y límite del ordenador de vuelo,
  instrumento `trazas` para las pantallas, asiento sobre una articulación.
- El visor ya no se oscurece solo ni tiene visor solar: el filtro de soldadura va con el botón
  derecho del soldador.
- Una lámpara es «de dentro» por lo mismo que las piezas que alumbra (`kind.rs`
  `MachinePlan::lit_inside`: en un compartimento o dentro del contorno del casco), calculado al
  construir la nave y no cada fotograma. Antes solo contaban los compartimentos: en una nave sin
  aire propio (el Azor) sus luces de cabina no alumbraban nada de dentro. De las otras naves solo
  cambia una lámpara: el primer plafón del puente del Alcotán y del Cachalote, que quedaba un
  poco fuera de la caja de su compartimento y contaba como de fuera.
- Al levantarse de un asiento que se ha movido (la plataforma bajada) su salida va con él
  (`app/aboard.rs` `stand`): no se aparece en la cabina que ya no está ahí.

Queda de esto:

- **Nadie ha jugado el combate.** Está probado por pruebas y fotos; falta volar el Azor contra
  algo y afinar números (alcances, cadencias, ganancias del piloto automático, empuje).
- **Enemigos.** No hay nada que dispare al jugador: los modos EVADIR, el aviso de misil y los
  señuelos solo se han probado en pruebas. Hace falta una nave con piloto (el mismo piloto
  automático de combate sirve) y quien decida a quién ataca.
- **Dañar el tráfico.** El tráfico civil sale en el radar y se le puede fijar y disparar misiles
  (van hacia él), pero no es una estructura: no se rompe.
- **Multijugador:** nada de esto va por la red todavía (trazas, disparos de nave, misiles
  guiados, señuelos).
- **Sonidos:** cañón, lanzamiento, aviso de fijado y de misil (hay señales para ellos:
  `rwr.fijado`, `rwr.misil`, `armas.disparando`).
- **Reactor del Alcotán:** que el panel diga por qué saltó y qué falta para rearmar, y que PARO
  sea una parada normal (Fernando lo pidió el 2026-10-06; el grupo del Azor ya lo hace así).
- Cableado y compartimentos del Azor: va todo con el cableado oculto y sin aire (no tiene
  troncos que cortar ni salas con sus paneles).
- Modelos: el casco del Azor sale del generador de casco por secciones (facetado); una pasada
  más de forma (carenados entre ala y casco, tomas, antenas) lo mejoraría.

## V36 (2026-10-05): SELENE, vuelo, tiempos, lógica, masa, manos, huellas, visor, launcher

Hecho (cada cosa con sus pruebas; 501 en total, más 139 del launcher):

- El juego se llama SELENE; cada versión se entrega en su carpeta con sus datos (`SELENE_V36`,
  `V35_jugable`).
- Pantalla de carga con la estética del menú; launcher nuevo (tarjetas DEMO/DEBUG/MULTIJUGADOR,
  lista de versiones con novedades, historial, herramientas, sistema).
- Vuelo: alas del Alcotán 1,5 m a proa (góndolas bajo el centro de masas), alas del Cachalote
  recolocadas, motores con ralentí al 2 %, MANTENER nivela la nave sobre sus motores
  (`crates/ship/tests/vuelo.rs`: cuatro gravedades, ralentí, carga descentrada).
- Sin esperas: registro de procedimientos con presupuesto y su prueba ([`TIEMPOS.md`](TIEMPOS.md));
  reactor en línea a los 15 s, compresor 103 s.
- Lógica probada para toda nave ([`LOGICA.md`](LOGICA.md)).
- La nave pesa lo que lleva (depósitos, botellas) y lo dice en una sección MASA
  ([`CARGA.md`](CARGA.md) §6).
- Manos en los mandos, palanca y gases, gestos (Tab), ordenador de muñeca (Y).
- Huellas en el regolito; visor (filtro de soldar, visor solar con U, vaho).

Queda de esto (lo dejaron a medias los agentes que se pararon; por hacer, uno a uno):

- **Multijugador al 100 %.** Hechos en la V40 los daños, los disparos, las naves a cualquier
  velocidad, quien flota o va de pie en una nave ajena y las puertas a mano (`multi/told.rs`,
  `follow.rs`). **No** se comparten aún la carga suelta, la herramienta ni los gestos de los
  otros, ni el estado de las máquinas al entrar tarde (`ship/src/sync.rs` tiene la instantánea
  de una nave, sin enchufar).
- **Herramientas nuevas** (tableta de procedimientos, multímetro, cámara de fotos, balizas): sin
  empezar. Hay modelo de la tableta (`assets/models/tableta.glb`) y el registro de procedimientos
  ya da lo que la tableta necesita.
- **Modelos:** hechos palanca, gases, ordenador de muñeca y tableta. Sin hacer: multímetro,
  cámara, baliza y el repaso de los 8–10 modelos más flojos.
- **Manos:** falta que los otros jugadores hagan lo mismo (gesto, muñeca, mano al mando), repasar
  en foto cada tipo de mando (volante a dos manos, tapas) y que la mano no tape lo que miras.
- **Huellas y visor:** sin ver en foto el surco de una carga arrastrada, la marca de las patas y
  el barrido de los chorros; el vaho y el filtro de soldar, solo por sus pruebas.
- **Vuelo:** MANTENER nivela, pero trasladarse en MANTENER sigue siendo a toberas (lento con
  35 t); no hay todavía inclinación mandada para trasladarse con los motores. A 0,21 m/s² el
  Alcotán tarda en asentarse (1,3° de inclinación).
- Sin comprobar a mano: nada de la V36 se ha jugado con ventana (pruebas y fotos de guion con
  ventana oculta); el launcher nuevo, solo por sus autopruebas con foto; los FPS, sin medir.
- `docs/DETALLES.md` y `docs/MANOS.md` (cómo funcionan huellas, visor y manos por dentro) están sin
  escribir: lo dice el propio código (`app/src/footprints.rs`, `visor.rs`, `handwork.rs`).

## V35 (2026-10-04): aire, andar, chorros, vista libre, multijugador, carga, imán, reactor

Hecho:

- Aire de un compartimento a otro con válvulas de mano (volante y manómetro de ΔP, a los dos
  lados de cada mamparo) y al vacío (volante rojo); compresor con depósito de aire recuperado
  para vaciar un compartimento sin tirar el aire ([`AIRE.md`](AIRE.md)).
- Andar: un pie tras otro, siempre uno en el suelo; correr, con un instante en el aire.
- Mochila: con el estabilizador, al soltar las teclas frena en menos de un segundo.
- Chorros en motores, toberas de maniobra y mochila ([`CHORROS.md`](CHORROS.md)).
- Alt: mirar sin girar el cuerpo (la cabeza desde tus ojos, la cámara desde fuera).
- Multijugador preparado: edición `multiplayer`, carpeta `servidores`
  ([`MULTIJUGADOR.md`](MULTIJUGADOR.md)).
- Pantalla de carga (una cabina que arranca en frío) con el juego cargando en su propio hilo: la
  ventana responde siempre.
- El imán (un solo módulo, grúa del Cachalote y Abejorro) se lleva todo lo que tiene debajo; lo
  que lleva cada recipiente es un dato (sustancia y cantidad) y pesa lo que lleva
  ([`CARGA.md`](CARGA.md)).
- Reactor: en línea a los 45 s y cubre el gasto de la nave en unos 6 min (antes, más de un
  cuarto de hora); las barras no dejan que un salto de potencia lo dispare y se limita solo si no
  puede soltar el calor. Balance de energía (`energia.*`) en una sección genérica de panel
  (`secciones.jsonc`) y en la página ENERGÍA de las pantallas. Títulos de grupo más grandes que
  los rótulos, a cualquier escala de panel.

Queda de esto:

- **Compresor lento:** vaciar la bodega del Alcotán son 34 min a 15 kW. Uno mayor no lo alimenta
  el bus (se probó con 30 y 60 kW: sin energía). Haría falta un bus de potencia o dos compresores.
- **Multijugador:** no se sincronizan la carga suelta, los daños, los disparos ni las puertas
  movidas a mano; de los otros jugadores no se dibujan la herramienta en uso ni los chorros de su
  mochila. Probado solo en el mismo PC (dos juegos y el servidor); red local e internet, sin
  probar.
- **Los depósitos de las naves no pesan** lo que llevan (el propelente gastado no aligera la
  nave): preparado en `CARGA.md` §6, sin activar.
- **Las articulaciones no notan la carga** que cuelga del imán (la grúa sube igual 50 kg que
  2 500): el aviso de sobrecarga es del imán, no del brazo.
- Las tuberías del compresor a cada compartimento no se dibujan.
- Anotado por quien hizo los chorros, sin tocar: las toberas de maniobra del Alcotán dan unos
  217 N de sus 2,2 kN, y `vuelo.tierra` vale 1 con la nave en el aire.
- El reactor a su potencia por defecto no cubre el compresor de aire en marcha (15 kW): hay que
  subir POTENCIA. Con los radiadores recogidos se limita él solo (lámpara ámbar).
- La sección `energia` solo está montada en el Alcotán y el Cachalote (el Abejorro no tiene
  cabina ni paneles de pared).
- Sin comprobar a mano: el cambio de la ventana de carga a la del juego con la ventana a la
  vista (solo la prueba sin ventana), el launcher con la pestaña de multijugador, y los FPS (no
  se han pasado pruebas de rendimiento, a petición de Fernando).

## V34 (2026-10-04): vista exterior, bajadas, rejilla de la bodega, menú de inicio y launcher

Hecho:

- Vista desde fuera (V): a pie, cámara al hombro que entra si algo la tapa y no baja del suelo;
  sentado, la nave vista desde fuera; la rueda la acerca y la aleja. Lo que se apunta sigue
  siendo lo que hay bajo la mira.
- Dónde se baja uno de cada asiento lo dice cada nave (`bajadas`): zonas con su rumbo; se toma
  la primera con suelo y sitio. El Abejorro: a su estribo, y con los dos ocupados al suelo.
- Sentado, el cuerpo queda sobre el cojín, las botas en el suelo y las manos en los muslos; el
  asiento es nuevo (cuna para la mochila) y toda nave se comprueba (`diag::seat_fit`).
- La bodega del Cachalote tiene rejilla pintada: celdas, reglas en metros a lo largo y a lo
  ancho, letras y números en sus bordes; la consola de la grúa lee la bahía bajo el gancho.
- Menú de inicio (jugar, dónde empezar, opciones, salir) y `LunaLauncher.exe`.

Queda de esto:

- Desde fuera, sentado, no se accionan mandos con el ratón (las teclas del asiento sí vuelan la
  nave). Falta una cámara de persecución que siga el rumbo de la nave por sí sola.
- Las manos sentado descansan en los muslos: aún no van a la palanca ni al acelerador.
- La rejilla solo está en el Cachalote; la bodega del Alcotán no tiene (es de un solo anclaje).
- El launcher no cambia opciones de versiones anteriores a la V34 (no las entienden).
- Sin comprobar a mano: el launcher con ratón y teclado (solo su autoprueba con foto) y la
  versión demo con ventana (solo la de desarrollo ha pasado los guiones; la demo, las
  comprobaciones que no abren ventana).
- Medir FPS sin otro juego abierto, cuando Fernando lo pida (ver `OPTIMIZACION.md`, V34).

## Pedido el 2026-10-04 (V33): físicas, cuerpo y animación

Hecho: amortiguadores de verdad en el tren (los del Cachalote más grandes); los mecanismos se
paran en lo que encuentran (izado en la carga, rampa en el suelo, puerta con alguien en el hueco,
carga agarrada contra la cubierta); bodega del Cachalote con 24 bahías, dos pasillos, mástil
telescópico que pasa la carga por encima y la consola de la grúa en medio mirando a popa; el imán
avisa si la carga sigue anclada; escalones de hasta 45 cm sin saltar y bajada sin flotar; frenada
al aterrizar; la mochila mantiene la altura; cuerpo del astronauta con esqueleto, dedos, marcha
por procedimiento con cada pie en el suelo, sombra propia y postura sentada; manos en las
herramientas, inercia, retroceso, gatillo; lanzacohetes nuevo con recarga a la vista; herramientas
de rigging (`tools/modelos/astronauta`, `docs/RIGGING.md`). Falta:

- **Verlo jugando:** todo está comprobado con pruebas y fotos de guion, no con el mando en la
  mano. Lo que se note raro al jugar (un codo, el paso al girar, la mano en la empuñadura) se
  afina en `rigs/astronauta.jsonc` y `gear.jsonc` sin tocar código.
- **Manos libres:** al coger una caja con las manos el brazo no va hacia ella, y al pulsar un
  mando de un panel el dedo no va al mando. Hay con qué hacerlo (`Grip`), falta pedirlo.
- **Sentado:** las manos descansan; no van a la palanca ni al acelerador.
- **Los demás (PNJ y multijugador):** el cuerpo con esqueleto es solo el del jugador; la multitud
  sigue con su animación precocinada (`assets/defs/models/astronauta.jsonc`).
- **Tramos intermedios del mástil:** siguen al izado por una razón fija y no se prueban contra
  obstáculos por su cuenta (van siempre por donde ya pasó el imán).
- **Posarse en una ladera:** una nave dejada torcida en un talud baja sobre sus patas una tras
  otra y se desplaza unos metros hasta asentarse. Asentarla ya en equilibrio al ponerla pediría
  resolver la estática con el terreno.
- **Antebrazo sin huesos de torsión:** un giro grande de muñeca se lo lleva el puño del guante.
- **Sonidos de la recarga:** usa los chasquidos de tapa y anclaje que ya había; les falta el suyo.

## Pedido el 2026-10-04 (V32): modelos e interfaz

Hecho: modelo de cerca para **todo** tipo de componente, toda pieza de estructura y toda pieza
suelta de las naves con `"estilo"` (alas, aletas, lomo, tren, vigas, jaula, grúa); casco con
juntas, aristas redondeadas y ventanas con marco; hojas de puerta y rampa con marco y nervios;
herramientas de mano como modelos; rótulos que van con la pieza; interfaz con visor, reloj de
misión, emblema y ventanas de herramientas con el mismo cristal. Falta:

- **Modelos sin mirar por Fernando:** están revisados por fotos de guion, no jugando. Los que
  canten (una proporción, un color, un rótulo tapado por un resalte) se corrigen en su receta.
- **El casco sigue siendo de facetas:** se redondean las aristas marcadas `suaves` y se cortan
  las juntas, pero la sección sigue siendo el polígono de 12 lados de los datos. Curvarlo de
  verdad pide que las calcas y lo que va pegado al casco sigan a la curva.
- **Cuadernas por dentro:** el interior del casco es chapa lisa con sus juntas pintadas; faltan
  cuadernas y larguerillos (hay que comprobar que no crucen paneles ni canalizaciones).
- **Mandos de los paneles:** interruptores, botones y agujas siguen siendo cuatro primitivas por
  pieza. El pase de *props* ya acepta mallas: falta dar un modelo a cada tipo de mando.
- **Rótulos del Abejorro y del Cachalote:** llevan los de sus componentes; el casco del Cachalote
  no tiene estampados propios (su generador no copia los del Alcotán).
- ~~**El astronauta**~~ (2026-10-04, V33: el del jugador tiene su modelo con esqueleto y dedos;
  el de la multitud sigue siendo el de antes). **Las naves de tráfico** van por otro camino
  (`assets/defs/models`) y no se han tocado.
- **Sombras de cerca:** las sombras de una nave a menos de 40 m se dibujan con el modelo fino.
  Si pesa, dibujarlas con el aspecto básico.

## Pedido el 2026-10-03 (V31)

Hecho: coger y arrastrar con las manos (clic mantenido con las manos libres; lo que pesa
demasiado se arrastra); volver a anclar una carga (suelta sobre un anclaje abierto, clic en su
palanca); pasillo libre en la bodega del Alcotán; al salir de una nave con la mochila se conserva
su velocidad y la mochila estabiliza respecto a ella (Z la apaga); dos naves nuevas — el
**Abejorro** (remolcador de 7 m, de un asiento, con giróscopos e imán electropermanente) y el
**Cachalote** (carguero de 45 m con puente grúa, cuatro góndolas, giróscopos y un Abejorro atracado
en el lomo) —; sonido; polvo; foto con F12; rendimiento medido. Falta:

- **Volar el Cachalote de verdad:** sus cuatro motores lo levantan con margen y el ordenador los
  reparte para que no cabecee (test), pero no se ha hecho un vuelo entero con él en el juego.
- **Fallo único del Cachalote:** hereda del Alcotán la lista de `esenciales`; no se ha repasado
  con sus circuitos nuevos (grúa, giróscopos).
- **Sonido sin oír:** el motor de sonido está probado en buffer (niveles, vacío, coste) y la
  tarjeta se abre, pero nadie ha escuchado aún cómo suena cada receta: afinar de oído.
- **Sonido de lo que no se toca:** disparos e impactos de otros no suenan (en el vacío no deben,
  salvo a través del suelo si pegan cerca); dentro de una cabina con aire sí deberían.
- **Polvo de las naves ajenas lejanas:** solo levantan polvo las naves a menos de 700 m.
- **Grúa:** mueve palés con el imán; falta que el carro siga a la carga cuando la nave acelera
  (hoy la carga va rígida con el imán) y un mando de la grúa que se lleve en la mano.
- **Lastre o trimado de carga:** el ordenador compensa con los motores; un tanque de lastre o
  avisar de "carga descentrada" en el panel sería lo propio.
- **Atracar el Abejorro volando:** la cuna toma lo que se pose en ella, pero posarse en el lomo de
  otra nave con el Abejorro no se ha probado jugando.

## Pedido el 2026-10-03 (V30)

Hecho: mochila con tecla (J) y que no se dispara al pisar un escalón; menú con pestaña de
controles sacada de la tabla de teclas; HUD con sitio para cada cosa (gas de la mochila abajo a la
izquierda); mando sostenido mientras se mantiene su tecla; asientos de los pilotos junto a los
paneles y consola de techo delante e inclinada, con test de alcance desde cada asiento; paneles de
pared asentados en su pared y sin cables por delante, con test; herramientas que siguen a la vista;
carga suelta con física y anclajes de carga; lo que estalla al romperse. Falta:

- ~~**Volver a anclar:**~~ (V31, 2026-10-03) hecho sin unir estructuras: lo anclado pasa a
  moverse con su dueño (`core/structure/hold.rs`) y le suma su masa. También cogerla con las manos
  (`app/hands.rs`), con el imán del Abejorro y con la grúa del Cachalote.
- ~~**Carga suelta contra chapa fina:**~~ (V31, 2026-10-03) contactos especulativos y la chapa
  como semiespacio: un palé no atraviesa la cubierta a 6 m/s (test en `ship/tests/carga.rs`).
- **Carga peligrosa de verdad:** hoy solo estalla el propelente. Faltan los líos de los que habló
  Fernando: plutonio (radiación), un mini reactor, gas que se escapa, líquido que se derrama.
- **Mando de anclajes en un panel:** se abren a mano o por su señal; falta un interruptor con tapa
  en el panel de la bodega para soltarlo todo (lanzar la carga en vuelo).
- ~~**Nombre de la carga suelta:**~~ (V31) el escáner dice lo que lleva (`contents` de la pieza).
- **Reasignar teclas:** la pestaña de controles las enseña, aún no deja cambiarlas.

## Pedido el 2026-10-03 (V29)

Hecho: canalizaciones en vez de cables; naves para batallas de cientos (malla compartida, daño por
pieza sin rehacer nada, física con árbol de cajas, sistemas a su ritmo y en varios hilos: ver
`OPTIMIZACION.md`); daño que ennegrece, sin grietas, y tomas que chispean; redundancia con test de
fallo único; equipo (soldador-escáner con pantalla y vista de integridad, reparar y reponer;
lanzacohetes; mochila); demo sin vuelo libre ni avisos de la nave en el HUD; carga en la bodega;
2 000 naves de tráfico que no se cruzan; avisos con letra de un tamaño y hidráulica solo con una
bomba pedida; tapas (la palanca se acciona con la tapa abierta) y brillo en el mando apuntado; luces
de fuera que no iluminan lo de dentro. Falta:

- **Reparar cuesta:** hoy soldar y reponer son gratis. Fernando quiere que más adelante pidan
  recursos o el mismo componente (los repuestos de la bodega ya existen como carga con `recurso`).
- ~~**Carga que se mueve:**~~ (2026-10-03, V30) la carga va sujeta por anclajes y suelta es un
  cuerpo con física (`ship/src/cargo.rs`). Falta llevarla y volver a anclarla (arriba).
- **Panel de disyuntores único:** si se destruye su caja caen todos los disyuntores (no es una
  canalización, el test de fallo único no lo cubre). Partirlo en dos, babor y estribor.
- **Tráfico que se pueda abordar:** las naves de tráfico son planes de vuelo con un modelo; al
  acercarse deberían pasar a ser naves de verdad. Tampoco esquivan a las naves del jugador.
- ~~**Modelos de las herramientas:**~~ (2026-10-04, V32) el soldador-escáner y el lanzacohetes son
  modelos (`tools/modelos/recetas/herramientas.py`); el pase de *props* acepta mallas
  (`Renderer::prop_mesh`).
- **Editor F6:** al hacer clic en un tronco no elige tramo (antes cada cable era su tramo).
- **Luces de dentro que salen fuera:** una luz de cabina aún puede iluminar el ala a través de la
  pared (lo de dentro ya no lo ilumina lo de fuera).

## Daño de los paneles: suciedad que oscurece y huecos de bala en un mapa de alturas

Pedido el 2026-10-02 y **cambiado el 2026-10-03**: Fernando no quiere grietas; el daño es la pieza
cada vez más negra (hecho en `structure.wgsl`: hollín a manchas, sin `cell_edge`). Lo de abajo (los
impactos con forma) queda como idea, no como pedido:

- ~~**Suciedad general.**~~ (2026-10-02) Hecho en `structure.wgsl` (`surface`): con el daño de la
  pieza la pintura se ensucia y se apaga (mugre en manchas, más rugosa) y salta en los cantos. Las
  grietas de Voronoi siguen encima a partir de daño medio: quitarlas cuando estén los sellos.
- **Impactos con forma.** Cada impacto **añade o modifica un mapa de alturas** de esa pieza con el
  hueco de la bala: un cráter pequeño con su labio levantado, la pintura saltada alrededor y el
  metal desnudo en el fondo. Dos impactos cerca se suman; uno que atraviesa deja un agujero.

Cómo hacerlo (propuesta, sin implementar):
- Un **atlas de alturas por estructura** (R16F o R8, p. ej. 2048², con casillas por pieza que haya
  recibido impactos; las piezas sanas no gastan atlas). La casilla de una pieza se proyecta sobre su
  cara por las coordenadas de pieza (triplanar en el espacio de la pieza, como el `plating`).
- El impacto lo escribe la CPU (`Structures::shoot` ya sabe el punto, la dirección y la energía) como
  un **sello**: perfil radial (fondo, pared, labio) escalado por el calibre y la energía, sumado al
  mapa con `min` para el fondo y `max` para el labio. Subida parcial de la casilla con
  `queue.write_texture`.
- El shader de estructuras saca la normal del mapa (diferencias finitas) y oscurece el fondo y el
  halo (suciedad, hollín, pintura saltada). La suciedad general es `damage` de la pieza en el albedo,
  sin patrón de grietas.
- El agujero pasante es un `discard` donde la altura baja de −espesor del panel (se ve el interior).
- **Red:** el sello es un dato pequeño (pieza, punto local, dirección, calibre, energía); viaja con
  el daño y cada cliente lo reaplica: el mapa no se envía.
- **Persistencia y procedencia:** los sellos son la «cicatriz de verdad» del informe de diseño (la
  abolladura de babor está donde te dieron). Guardarlos con la pieza.

## Naves: lo que falta (2026-10-02)

Hecho: Alcotán de datos con todos sus sistemas, en la app (cualquier cuerpo), caminar a bordo,
mandos con la mano, asientos con teclas, catálogo G, inspector F4, versiones debug y demo
([`NAVES.md`](NAVES.md)). Falta:

- ~~**Modelo bonito con Blender**~~ (2026-10-04, V32) hecho: un `.glb` por cosa en
  `assets/models`, escrito por una receta de `tools/modelos` y cargado como look de cerca
  ([`NAVES.md`](NAVES.md), «Modelos»). 93 ficheros, 217 000 triángulos.
- **Pintura de chapa realista** hecha en el shader (paneles alineados, juntas biseladas, tornillos,
  cantos picados, grano antideslizante en suelos). Sin mirar de cerca en juego: ajustar tamaños e
  intensidades si algo canta.
- Los tramos de conducto hasta piezas que se mueven (tren, góndolas, rampa) no siguen a la
  articulación (se ven quietos).
- La masa del propelente no cuenta en la masa de la estructura (vuela igual lleno que vacío).
- Red: la nave ya es estado plano (`Ship::snapshot`), falta mandarla por el servidor.
- Sentado, la cámara sigue el marco de la nave; de pie a bordo la vertical es la del cuerpo (en una
  nave muy inclinada se camina «recto» respecto al suelo del planeta).

## Pedido el 2026-10-02 (segunda tanda)

Hecho (V26): sin resplandor en pantallas; tapas bajas que se tumban; agacharse (C), linterna (L),
telémetro (T); paneles por compartimento con presión, puertas, igualar, luces, casco y energía;
venteo desde fuera para la esclusa; pantallas multifunción con bisel y páginas automáticas; fichas
al mirar; tests lógicos de uso; cables con codos y fuera del exterior; seis niveles de detalle,
silueta lejana, oclusión. Falta:

- ~~**Cables:** tramos medio enterrados y por fuera~~ (2026-10-03, V29) Sustituidos por
  canalizaciones (un tronco por pared, tomas y bajantes): 0 por fuera, 0 medio enterradas, 0 a
  través de aparatos (`tests/cables.rs`).
- **Más mandos con sentido:** ~~prioridad por circuito~~ (2026-10-02: un selector por disyuntor,
  panel en la bodega). Faltan piloto automático, transferencia de combustible, soporte vital
  AUTO/MANUAL, cámaras. Hoy: 136 mandos y 79 indicadores (web Albatros: 218 mandos, de ellos 32
  botones de bisel; aquí cada bisel es un mando de 20 botones).
- **Puestos genéricos** con pantallas multifunción (técnico de armas, cámaras): el sistema ya vale,
  faltan las páginas de armas y de cámara (imagen de cámara en la pantalla).
- ~~**Editor de naves 3D con MCP**~~ (2026-10-02, V27) Operaciones sobre la definición con
  deshacer, comprobación y guardado (`crates/editor`), servidor MCP `LunaMCP.exe` (leer, operar,
  validar, foto, guardar) y editor F6 en el juego con aplicación en caliente. Falta: **puertos que
  generan mandos** (conectar un aparato a un panel y que salgan sus mandos según el tipo del
  puerto), arrastrar con el ratón (hoy por pasos), mover paneles y nodos desde F6, vista de rayos X
  y de redes, marcar en rojo lo que falla la comprobación.
