# Plan: servidor autoritativo — el servidor simula y decide todo

Fernando (2026-10-07): «necesito que el juego sea con servidor autoritativo, que sincronice todo;
es un cambio gordo, lo sé; primero analiza y planea todo a la perfección. Molaría que pudiera
aprovechar la gráfica del servidor para ciertas cosas, pero igual es fumada».

Este plan **sustituye las fases 0 a 5** de [`PLAN_SERVIDOR_MUNDO.md`](PLAN_SERVIDOR_MUNDO.md).
Aquel dejaba al piloto mandar en cómo se mueve su nave y al jugador en su cuerpo, con el servidor
vigilando. Aquí no: **el servidor tiene la única verdad de todo** y los clientes solo predicen y
dibujan. Sus fases 6 a 8 (núcleo del mundo `lunar-world`, personas, galaxia) siguen valiendo y van
encima de esto.

Es un plan, no código: cada fase acaba en algo que se juega y se mide, y al acabar cada una se
decide si se sigue igual.

## Avance

| Fase | Estado | Qué quedó |
|---|---|---|
| 0 | hecha (2026-10-07) | la base: 90 baterías de pruebas en verde antes de tocar nada (13,5 min); las pruebas que miden lo que no debía depender del proceso ni de los fotogramas están en `crates/play/tests/game.rs` |
| 1 | hecha (2026-10-07) | pasos fijos de 1/60 s (`Game::tick`), los sistemas de las naves a un tic por paso, dibujo entre pasos (`present`/`restore`, partículas con `ahead`/`lag`, lo disparado mientras se dibuja sale del paso: `Blasts::hold`), **barrido entre estructuras y contra el suelo**. Pruebas: la misma partida a 30, 60, 144, 240 fps y a trompicones; en otro proceso; nada salta entre pasos a 0, 300 y 7 800 m/s; bloques de frente hasta 15 600 m/s |
| 2 | hecha (2026-10-07) | `crates/play` (`lunar-play`): `Game` para N jugadores (`Player`), `Builds`, `Blasts` sin dibujo ni teclas, `Ships` sin su aspecto (`app/shipview.rs`), `Tactics`, `Pilot` (con su estado entero y su resumen: `pilot/state.rs`), `Hands`, lo que hace una mano a una nave (`controls.rs`), el aire que empuja (`air.rs`), `told`, `follow`; `View` en el núcleo. El cliente usa todo eso y solo dibuja, suena y lee teclas |
| 3–4 | hechas (2026-10-08) | **`lunar_play::host::Host`** (el juego del servidor: comandos por paso con su anillo, adivinados si no llegan, actos comprobados —alcance, ritmo, trampas—, el cuerpo comparado con lo que su juego dice que obtuvo y corregido, `Say::Server`: todo golpe se hace en orden y se cuenta con su azar) y **`lunar_play::online::Online`** (el juego del jugador: va unos pasos por delante del servidor con un reloj que se fija con la primera instantánea y se ajusta con lo que el servidor dice de lo temprano que llega cada comando; su cuerpo predicho, corregido y vuelto a dar con lo pedido desde entonces, el salto del ojo repartido; todo lo demás simulado aquí y enderezado por lo que estaba de desviado *en el mismo paso*, `Track`). El resumen del cuerpo es un estado pequeño comparado con tolerancia de 1 mm (`pilot::Summary`), no un hash: un hash redondeado daba correcciones por diferencias del último bit con las naves de al lado. Lo que reposa en el servidor llega en reposo con su pose exacta y se queda dormido igual (`RigidState::resting`). Los sistemas de una nave contados en el paso S se adelantan a nuestro paso (`Ships::catch_up`), y al subir a una nave el servidor la cuenta entera otra vez. Pruebas (`crates/play/tests/online.rs`, servidor real de `lunar-net` sobre red en memoria): **0 correcciones** andando 15 s en red limpia, con 40 ms y 3 % de pérdidas y con 150 ms y 10 %; **0 correcciones** de pie en una nave a 0, 300, 1 600 y 7 800 m/s; el reloj se asienta con 10–150 ms; un empujón que solo ve el servidor se corrige sin saltar el ojo; los demás se ven donde están; lo que vuela está donde el servidor lo tiene a cualquier velocidad. El servidor de verdad (`luna-servidor`, fase 3) y la ventana (fase 10) van por aquí. **Los pasos se repiten como eran** (2026-10-08): tras una corrección el servidor ya no compara lo que el jugador había dicho de los pasos siguientes antes de saberla (daba una escalera: 36 correcciones al poner a alguien a bordo de una nave en marcha, ahora 1; 4 en una bodega, ahora 1); la gravedad propia de una nave viaja con su estado (cada copia la encendía desde que la conocía y quien iba a bordo pesaba distinto); y los pasos se repiten contra la nave que lleva al cuerpo tal como estaba en cada uno (`Online::as_then`: giro, velocidad, aceleración, gravedad y piezas móviles, guardados por paso en su `Track`). Prueba: puesto en el aire en una nave cuya gravedad se enciende, 2 correcciones por 2 empujones (6 sin esto) y el cuerpo igual al bit al acabar de repetir. Queda: la nave que aún bota sobre sus patas (hasta 4 correcciones el medio segundo que bota, 0 después: `PENDIENTES.md`) |
| 5 | hecha (2026-10-08) | `lunar_play::interest`: conocer por tamaño (alcance + metros de radio), histéresis (se olvida pasado ×1,3 durante 1,5 s), fijados (lo que te lleva, tu asiento, la nave junto a la que flotas), horizonte de 8 s para lo que llega deprisa, prioridad acumulada por instantánea; lo anclado no viaja; el reposo se cuenta una vez, exacto y seguro (`Event::Rest`). **Índice por celdas** (`interest::Index`, hecho una vez por paso para todos): lo pequeño y lento en celdas de 4 km, cada jugador mira las 27 que lo rodean; lo grande o rápido, todos; quien va deprisa, todo. **Sabe exactamente lo mismo que mirarlo todo** (`tests/interest.rs`: 6 000 estructuras de todos los tamaños, algunas a 4 km/s, 16 jugadores de quietos a 7,8 km/s, 120 pasos) y cuesta la mitad (3,5 → 1,7 ms por paso, con un cuarto de los jugadores a velocidad orbital, que siguen mirándolo todo). En combate (la Azor disparando 58 balas) cada jugador recibe 4,4 kB/s. **Nivel de rastro** (2026-10-08): lo que no se conoce entero —naves pasado su alcance, jugadores pasado lo que lleva una instantánea— se cuenta de lejos hasta 150 km (`TRACK_REACH`), 5 veces por segundo y por turnos (`net::TRACKS`, sin fiar: dónde está, al centímetro desde quien lo recibe, y cómo va; lo más cercano primero, lo que quepa en un datagrama); el juego lo tiene en `Online::far` y lo lleva al momento con su velocidad (`far_now`), y la ventana pinta su nombre y su distancia en el cielo. Prueba: una nave posada a 40 km y otra volando a 600 m/s a 30 km, conocidas solo de lejos y a menos de 1 m de donde las tiene el servidor; al acercarse, la cercana se conoce entera y deja de estar de lejos. **El radar ve lo de lejos**: las naves conocidas solo de lejos van al sistema táctico de cada juego (`Tactics::far`, donde está cada una en cada paso): los radares las encuentran como lo que se conoce entero, y un guiado o el SEGUIR del piloto automático pueden apuntarles (`tactics::tests`). Los guiados lejanos no necesitan rastro: su lanzamiento y dónde van cada 0,1 s llegan a todos hasta `most` (400 km) |
| 6 | 90 % | El servidor lanza lo que pide la mano (`Act::Launch`, con alcance y cadencia por arma), decide cada golpe y lo cuenta con su azar: la nave golpeada y cada trozo quedan **iguales en el servidor y en dos jugadores**, también para quien entra tarde; los trozos que un juego rompe por su cuenta toman el nombre del servidor por su linaje. **Los cráteres son del servidor** (`Event::Crater`, y el suelo entero a quien entra, `Event::Ground`): el cliente no cava por su cuenta, y cada partida de un proceso tiene su propio suelo (`Game::new_apart`; antes lo compartían, y una explosión cambiaba el suelo de las demás). **El disparo propio se ve una vez**: el lanzamiento lleva el número que le dio el juego que lo lanzó, y a ese juego el servidor no le repite el lanzamiento y le cuenta su final y dónde va (un guiado) por su número (`blasts::OWN`): si aún vuela allí, acaba donde acabó en el servidor; si ya estalló allí, no estalla dos veces. **Las armas de las naves desde el asiento**: las teclas del asiento mueven sus mandos también en el servidor, que dispara; la Azor armada por la mano del piloto (`Act::Control`) y con el gatillo apretado suelta 58 balas en el servidor y el piloto y el que mira ven 58 cada uno. Pruebas de tramposos: fase 11. **Todas las armas de los datos, una a una, desde una nave a 2 km/s** (2026-10-08): junto a una Alcotán y contra una Cachalote que van a la par, 150 m de lado, con uno en su bodega: las 13 (balas, misil, guiados, señuelos) arrancan una vez en cada juego, 510 golpes y cada estructura y trozo acaba igual en los tres con el nombre del servidor; el tirador, 0 correcciones; el de la bodega, 11 (los golpes llegan a su juego un momento después que al servidor: la nave le da tirones después). **Compensación de retraso, el gancho** (§3.5): el servidor guarda dónde estaba cada cuerpo los últimos 300 ms (`Host::bodies_at`, `REWIND`), probado contra lo que tenía en cada paso. Contra estructuras no hace falta: el juego del jugador y el servidor las tienen en el mismo paso. **Dos tiradores a la vez contra una nave que esquiva** pilotada por otro (un guiado de radar y ráfagas de ametralladora): 36 lanzamientos vistos una vez en cada uno de los cuatro juegos, la nave igual en todos, nadie corregido. Falta: juzgar con él un disparo a una persona, el día que se pueda herir a alguien |
| 7 | hecha (2026-10-08) | Las teclas del asiento viajan en el comando como máscara (`Cmd::keys`) y las mueve el mismo `seats::Drive` en el servidor y en el piloto; sentarse y levantarse son `Act` comprobados (alcance, asiento libre); los sistemas de una nave contados en el paso S se adelantan al paso del cliente (`Ships::catch_up`) y al subir a ella se cuenta entera otra vez. Prueba: una nave lejos de todo cuerpo pilotada con 40 ms y 1 % de pérdidas va donde el servidor la tiene (< 5 cm), **0 correcciones del piloto**, y otro jugador la ve ahí. **Relevo del piloto a 7,8 km/s**: uno pilota la Alcotán, se levanta, se sienta el otro y sigue; la nave no salta (4,9 mm como mucho de un paso a otro sobre lo que da su velocidad), la copia de cada piloto va a 3 mm de la del servidor y **nadie es corregido mientras pilota** (unas pocas al levantarse junto a otro: cada juego lo pone por donde tiene al otro, de hace un momento). Las armas desde el asiento, en la fase 6. **La nave sin piloto en órbita, igual para todos** (2026-10-08): la Alcotán en órbita a 18 km sin nadie, uno la mira desde la base y otro entra medio minuto tarde; minuto y medio después (150 km de órbita) cada copia está a 1 mm de la del servidor; puesto uno a bordo, va con ella sin una corrección y las copias siguen a 2 mm. Para eso, lo dormido en vuelo sigue su camino por Verlet cada segundo en vez de una parábola desde donde se durmió (`schedule::drift`: tres horas dormida, la órbita a menos de 2 m; lo que cae se despierta antes del suelo) y `put_on` toma la nave en el momento del mundo. **Formación** de dos naves con uno en cada una, de 0 a 7 800 m/s y de 30 a 240 fps: cada uno ve la otra a 0,2 mm como mucho de su sitio, sin correcciones. **`coherencia.rs` por la red**: en cada nave que vuela (Abejorro, Alcotán, Azor, Cachalote), cada tecla de su asiento que mueve la palanca, mantenida un segundo por la red, la hace girar o empujar por el mismo eje, en el mismo sentido y casi tanto como la misma tecla en un juego propio (48 teclas); la copia del piloto a menos de 5 cm del servidor y ninguna corrección. Lo demás de `coherencia.rs` (retenes, pasos de rueda, ordenador de vuelo) es de la nave y no pasa por la red: las teclas llegan en el paso en que se pulsan |
| 8 | hecha (2026-10-08) | Coger, soltar y la rueda son `Act`; lo que llevas lo mueve tu mano predicha y la misma mano en el servidor; lo que sujeta cada cosa (las garras de una bodega, una mano) se cuenta (`Event::Hold`, y en `Made`) con los sistemas de la nave que sujeta; los trozos que un juego suelta por su cuenta son suyos hasta que el servidor los nombra. Flotando con la mochila, el giro del ratón va en el comando (`Cmd::frame`). Pruebas: la caja de la bodega del Cachalote (la garra se abre, se coge, se lleva, se suelta y reposa **exactamente igual** en todos); flotar con la mochila sin correcciones (63 sin el marco). **Dos que van a por la misma caja**: la tiene el primero; al otro se le niega (`lo lleva otro`) y su juego la suelta (`Event::Unheld`). **Soldar por red**: una pieza de la Cachalote casi rota, soldada por uno pidiendo todo en cada paso: con las manos vacías no sube nada; con el soldador, lo que da el soldador (su ritmo y medio, como mucho un segundo de golpe: fase 11) y los tres juegos acaban con la misma vida al punto (`Event::State`, y lo que no llegaba a contarse, exacto al quedarse quieta: `Shadow::settle`). **Reconstruir por red**: dos piezas de la bodega quitadas; con el soldador en la mano la primera vuelve en todos los juegos, la segunda pedida al momento se niega (una cada tanto como tarda) y vuelve después, igual en todos. **Soldar es un paso del juego** (`lunar_play::weld`): la herramienta de la mano y el gatillo van en el comando, y lo que se suelda es lo que la mira del cuerpo encuentra al alcance del soldador; el servidor lo decide y el juego del jugador lo predice igual (la ventana ya no dice qué suelda ni cuánto: dibuja el arco y lee el escáner con el mismo `weld::target`). Prueba: un segundo de gatillo sobre una pieza herida, 82 de 576 de vida (el soldador da 81) igual en el servidor y en los dos juegos; mirando donde estaba otra, el tiempo que tarda, y vuelve en todos; con las manos vacías el gatillo no hace nada; ninguna corrección |
| 9 | 96 % | **Reconectar**: `Event::Hello` da una clave; el cuerpo de quien se cae se queda de pie en el mundo 60 s (los demás lo ven; a los 0,5 s sin oírle deja de hacer lo último que pidió: `GUESS_MOST`) y quien vuelve con la clave (`Act::Back`) lo recupera tal cual (`Event::Back`); la ventana lo intenta sola cada 4 s. **Guardar**: `lunar_play::save` (el mundo como se le cuenta a quien entra —`Made` de cada estructura que no está como la pone el escenario, `Ground`, `Gone`—, el reloj y los cuerpos con sus claves, con suma de comprobación) en dos ranuras (`partida.a.bin` y `.b.bin`; cada guardado va sobre la más vieja, a un fichero aparte y luego en su sitio), cada 5 minutos y al parar, hecho en el hilo de la partida (0,2 ms para 16 estructuras) y escrito en uno propio; al arrancar se retoma la más nueva que esté entera (la de otra versión o de otros datos se aparta, no se pisa). Las naves del escenario cuentan ya como escenario (`scenario_end` tras ellas): quien entra tarde se entera de la que ya no está. Pruebas: cortar la red 20 s y volver: el cuerpo donde esperó y **0 correcciones** después; una clave que no vale entra de nuevo; lo que nadie reclama se olvida; guardar y retomar: cada estructura, trozo y cráter igual (la huella de cada nave), y el jugador de vuelta en su cuerpo con 0 correcciones; el programa de verdad por UDP: «salir» guarda en la ranura a, al arrancar «Partida retomada… 1 cuerpo esperando», se vuelve con la clave y el siguiente guardado va a la b. **Entrar en una partida cargada**: con 10 jugadores en marcha, 6 naves y 100 estructuras alrededor y un 10 % de pérdidas, el que llega juega a los 1,22 s y ya tiene todo lo cercano (106 estructuras); quien llega a un mundo de 1 000 cráteres y 24 naves (272 kB) lo recibe entero, en tantos mensajes como hagan falta. **Pantalla de carga** (2026-10-08): el servidor cuenta a quien entra todo lo de alrededor en su primer paso y detrás `Event::Ready` (llega después de todo lo anterior: los mensajes fiables van en orden); hasta entonces la ventana muestra «Cargando lo de alrededor…» con lo recibido y no deja mover el cuerpo (`Online::ready`). Prueba: en la partida cargada, `Ready` llega cuando ya tiene las 106 estructuras, no antes. **Lo que vuela se guarda** (2026-10-08): cada bala, misil, guiado y señuelo de la partida, al bit (`Blasts::keep`/`take_up`, formato 2): guardada con un cohete, un misil, un guiado y un señuelo en el aire, la retomada los tiene igual y medio segundo después siguen igual que en la que no se paró. Falta: guardar al cerrar la consola (Ctrl+C: pide código `unsafe`, decisión de Fernando) |
| 10 | 95 % | **`lunar_play::local`**: el mismo anfitrión que `luna-servidor` en un hilo propio, con la red en memoria sin retraso para los jugadores de esta máquina y UDP para los de fuera si se pide; al pararlo devuelve la partida. **La ventana sin conexión juega a través de él** (su mundo se hace en otro hilo mientras se carga el nuestro): lo que la ventana hace al mundo va como en red (mandos, manos, soldar, poner naves; colocar el cuerpo desde el menú o reiniciar, `Act::Body`, que solo se deja donde se dejan las pruebas); el editor (F6) deja la partida sola en este juego. `--anfitrion PUERTO` aloja la partida propia para otros; `--directo`, sin servidor, como antes; guiones, bench y capturas van directos (manejan el mundo a mano). Prueba en tiempo real (`crates/play/tests/local.rs`): se entra en 0,03 s, 0 correcciones andando, otro entra por UDP y nos ve, ponerse en otro sitio no se deshace; el paso del servidor local, 0,58 ms (3,5 % de un núcleo). **La partida propia se guarda** como la del servidor (las mismas ranuras y el mismo guardián, ahora en `lunar_play::keep`): en `partidas/propia.a.bin` y `.b.bin` junto al programa, cada 5 minutos y al cerrar, y al abrir se retoma y se vuelve al mismo cuerpo con su clave (`Local::back`); `--nueva` empieza de cero (la guardada sigue en su ranura hasta que los guardados de la nueva pasan por encima). Prueba: jugar, cerrar, abrir: «Partida retomada… 1 cuerpo esperando», retomada en 41 ms, de vuelta en el mismo sitio y 0 correcciones. **El protocolo 1 (el relevo) está quitado**: de `lunar-net` se fueron los estados repartidos (`Up`/`Down`), lo dicho a todos (`Tell`/`Hint`/`To`), las llaves y el anfitrión (`Claim`/`Owner`/`Host`), la interpolación y el ahorro de envíos (`snap`, `throttle`), y de la ventana `app/multi` y `--relevo` (protocolo de red 3: solo entrar, salir, chat y lo que dice el juego, `Game`/`Quick`). De paso: lo que el juego dice de más no echa al jugador, los sucesos de un paso van en tantos mensajes como hagan falta y el suelo en trozos (`net::ground`): quien entra tarde a un mundo de 1 000 cráteres y 24 naves (272 kB) lo recibe entero. Falta: probar la ventana a mano (aquí no hay gráfica), los guiones de `tools/camara` por el servidor del proceso (manejan el mundo a mano y hacen fotos: van directos, sin el servidor, y aquí no se pueden probar). `servidores/LunaServidor.exe` rehecho desde Linux con la versión de red de hoy (`V41+p7`; sin probar en Windows: aquí no hay Wine) |
| 11 | 92 % | **Datagramas firmados**: cada datagrama de una sesión lleva su SipHash-2-4 (`lunar_net::sip`, 0,69 µs por datagrama entero) con una clave que sale del saludo (la galleta del servidor y la sal del cliente: quien no estuvo en el camino no la sabe); lo que no pasa se tira sin leerlo y se cuenta (`ServerStats::forged`) sin echar a nadie. **La sesión se reconoce por su clave, no por la dirección**: si el router cambia la del jugador, sus datagramas firmados lo siguen (`ServerEvent::Moved`; unos cientos por segundo de direcciones desconocidas como mucho). **Límites por jugador**: mensajes de comandos (2 por paso, 24 de golpe), actos (1,5 por paso, 120 de golpe), estructuras pedidas otra vez (8 por segundo), mensajes para la partida en la red (512 cada vez que la partida los toma: quien inunda no tapa a los demás); lo que sobra se tira y se cuenta (`flooded`). **Lo que no se cree**: la mirada no pasa de lo que gira el cuello, coger lo que lleva otro no se puede, lo que solo dejan las pruebas (poner naves, colocar el cuerpo, explosiones de prueba) se niega, el alcance de manos, mandos y disparos y la cadencia de cada arma (de antes). **Lo que se lleva en las manos** (`lunar_play::gear`: lo que hace cada herramienta de `gear.jsonc`, leído igual por el servidor y la ventana): solo vuela el tiro de la herramienta de la mano, a su recarga; solo se suelda con el soldador, a su ritmo y medio, y se repone una pieza cada tanto como tarda; lo demás (las teclas de prueba) solo donde se dejan las pruebas. Lo que no se deja volar desaparece del juego que lo lanzó (`Event::Unfired`): ni un cohete fantasma. De paso: lo que una mano cambia de una estructura se contaba en saltos de 1/128 de cada pieza y el último resto no se contaba nunca (copias distintas para siempre); ahora, quieta 0,25 s, se cuenta exacto (`Shadow::settle`). Pruebas: un cliente tramposo hecho a mano (comandos a cientos, mirada imposible, actos prohibidos y lejanos, `Resync` en bucle, basura y datagramas falsos en nombre del honrado): 15 040 tirados, 387 negados, el honrado sin una corrección; 50 000 mensajes basura contra la partida; la dirección que cambia; quien inunda no tapa a los demás; 3 000 datagramas basura contra el programa de verdad. Pruebas nuevas: con las manos vacías, antes de recargar o con la tecla de una ametralladora no vuela nada (en el servidor ni en el juego que lo lanzó) y con el lanzacohetes, a su ritmo, sí, sin una corrección; soldar con las manos vacías no suelda. Falta: cifrar contra quien sí ve el tráfico (pide una dependencia: decisión de Fernando) |

---

## 0. La respuesta corta

- **Se puede, y el código está mejor preparado de lo que parece.** La física y las naves ya no
  tienen gráficos (`lunar-core`, `lunar-ship`); el planificador ya simula para varios
  observadores; la foto entera de una nave y sus diferencias ya están escritas
  (`ship/src/sync.rs`, sin enchufar); el cuerpo del jugador ya es una función de sus teclas
  (`Pilot::step(dt, Input, …)`); la red (canal fiable y suelto, troceo, RTT, reloj, cuantización,
  red de pruebas que pierde) está hecha y probada; las pruebas de multijugador ya corren partidas
  enteras sin ventana.
- **Hay tres cosas que cambiar de raíz**, y en este orden:
  1. **El reloj**: hoy el mundo avanza en lonchas que dependen de los fotogramas
     (`physics::slices`: 16,7 ms a 60 fps, 6,9 ms a 144). Dos máquinas no pueden simular lo mismo
     así. Pasa a **pasos fijos de 1/60 s para todos**, y el dibujo interpola entre los dos últimos.
  2. **La simulación sale de la ventana y admite N jugadores**: un crate sin gráficos
     (`lunar-play`) que el servidor y el cliente corren igual. Hoy hay un solo `pilot`, una sola
     mano, un solo `aboard`, y `play.rs::frame` lo mezcla todo con el dibujo.
  3. **El protocolo**: arriba solo **intenciones** (teclas, mirada, clics, «disparo»); abajo
     **estado** (instantáneas por interés, con prioridad y diferencias) y **hechos** (golpes,
     altas, bajas). Desaparecen los estados que manda el cliente y el eco de daños.
- **La gráfica del servidor: para la simulación, fumada** (§4): la verdad tiene que salir igual
  que la predicción del cliente, que es CPU en `f64`; la GPU es `f32`, distinta según marca y
  driver; y lo que el servidor simula (cientos de cuerpos con ramas, no millones de cosas
  iguales) ya cabe de sobra en la CPU, así que subir y bajar los datos por PCIe en cada paso
  costaría más de lo que ahorra. **Para mirar sí vale**: espectador, retransmisión, repeticiones y
  pruebas con imagen, como fase opcional al final, sin que el servidor dependa nunca de tener
  gráfica.
- **Tamaño**: unas 30–40 sesiones de trabajo en 12 fases, más una opcional (§5). La primera que
  se nota jugando es la 4 (tu cuerpo, del servidor); la que más se nota, la 7 (las naves).

---

## 1. Qué quiere decir «autoritativo y que sincronice todo»

### 1.1. Las reglas

1. **Solo el servidor decide.** El cliente manda lo que *quiere* hacer, nunca lo que *ha pasado*:
   ni dónde está, ni a qué ha dado, ni qué ha reventado.
2. **Todo lo que existe tiene una verdad, en el servidor**: jugadores, naves (cuerpo rígido,
   articulaciones, máquinas, señales, aire, depósitos, armas), estructuras del escenario, trozos,
   carga suelta, lo que se lleva en la mano, proyectiles, explosiones, cráteres, puertas y
   anclajes, quién va a los mandos, tráfico, PNJ, chat.
3. **El cliente nunca espera.** Lo tuyo (tu cuerpo, la nave que pilotas, lo que llevas en la
   mano, el fogonazo de tu disparo, la palanca que mueves) se simula en tu máquina al momento con
   el mismo código que el servidor, y se corrige **sin saltos visibles** cuando llega la verdad.
4. **Lo ajeno se ve en el presente** en el mundo (llevado hasta ahora con su velocidad, su
   aceleración y las teclas de quien lo maneja) y **con retraso dentro de un anfitrión** (lo que va
   en una nave, en el marco de la nave): la misma regla que la versión web
   ([`../../docs/MOVIMIENTO.md`](../../docs/MOVIMIENTO.md)).
5. **Una sola forma de jugar**: sin conexión es el mismo servidor dentro del proceso, por la red en
   memoria.
6. **Ningún paso rompe el juego**: el protocolo de hoy sigue funcionando detrás de una opción hasta
   que el nuevo pasa las mismas pruebas (fase 10).

### 1.2. Quién decide cada cosa, hoy y después

| Qué | Hoy | Después |
|---|---|---|
| Tu cuerpo (andar, mochila, asiento) | tu partida, y lo manda a 20 Hz | el servidor con tus teclas; tú lo predices |
| Cómo se mueve una nave | la partida de quien va a sus mandos (o del anfitrión) | el servidor con las teclas del piloto; el piloto lo predice |
| Sus máquinas, señales, aire | cada partida las simula; solo viajan los mandos | el servidor; los clientes llevan un espejo que se corrige |
| A qué da un disparo, el daño | la partida que acierta, con eco ordenado por el servidor | el servidor |
| Qué revienta según lo que llevaba dentro | el dueño de la nave | el servidor |
| Dónde está cada trozo | cada partida con su física (se separan) | el servidor |
| Cajas, bidones, lo que llevas en la mano | cada partida, sin compartir | el servidor; quien lo lleva lo predice |
| Qué naves existen y con qué id | el orden del escenario; «el primero que entra fija la cuenta» | el servidor |
| Las semillas del azar del daño | contadores de cada partida (`blasts.rs:404`, `builds.rs:175`) | el servidor, en cada golpe |
| Puertas y anclajes a mano | quien los toca, a todos | el servidor, a petición |
| Tráfico (2000 naves con plan de vuelo) | cada partida, igual por semilla | igual (es una fórmula del tiempo): el servidor da semilla y hora; lo que se toca pasa a ser del servidor |
| Gente del escenario (`Crowd`) | decorado local | decorado local hasta que haya PNJ (fase 7 del plan del mundo) |
| Partículas, sonido, polvo, huellas, visor | local | local (son aspecto, no verdad) |

---

## 2. Dónde estamos: lo que ya sirve y lo que lo impide

### 2.1. Lo que ya sirve

| Pieza | Dónde | Por qué importa |
|---|---|---|
| Física, estructuras, daño, rotura, proyectiles, guiados | `lunar-core` | sin gráficos; posiciones y velocidades en `f64` (`Structure::pos`, `vel`) |
| Naves como datos | `lunar-ship` | sin gráficos; un tic de sistemas de un Alcotán, 0,081 ms |
| Varios observadores | `Structures::simulate_with(…, watchers, among)`, `LodPolicy`, `SimLevel` | el servidor simula en fino alrededor de *cada* jugador sin inventar nada |
| La foto de una estructura y de una nave | `ship/src/sync.rs`: `write_make`, `write_state`, `Shadow::delta`, `write_ship`/`read_ship`, `Digest` | es exactamente lo que el servidor manda a quien entra, tras un golpe, y para comprobar que un cliente sigue de acuerdo |
| El cuerpo como función de las teclas | `pilot/mod.rs`: `Input`, `begin`, `step`, `impl Among` | predecir y repetir pasos es llamar a `step` otra vez |
| Golpes en el marco de lo golpeado, con postura y semilla | `Builds::strike_done`, `Structures::posed` | el servidor manda el golpe y cada cliente lo aplica igual (ya probado «bit a bit») |
| Linajes de los trozos | `Structure::lineage`, `breakup.rs::child_of` | un trozo nacido de un golpe tiene el mismo nombre en todas: no hace falta darlo de alta |
| Un solo sistema de proyectiles | `Blasts::launch`, `Launch`, `Seen::{Launch, End, Track}` | el servidor lo vuela igual; el cliente ya sabe volar copias que acaban donde se les dice |
| Copias guiadas | `multi/follow.rs::steer`, `Client::rigid_carried`, `rigid_speeding` | lo mismo sirve para llevar las copias hacia el estado del servidor |
| Red | `lunar-net`: canal, troceo, RTT, latidos, `clock.rs` con deslizamiento, `quant.rs`, `throttle.rs`, `MemoryNet` | el transporte no cambia |
| Pruebas | `app/src/multi/tests.rs` (`Game` sin ventana: `Ships`, `Builds`, `Pilot`, `Blasts`), 73 pruebas de red | son la vara de medir de cada fase |
| Sin reloj de pared en la simulación | solo `Instant::now` para medir tiempos | nada que quitar |
| Paralelismo que no cambia resultados | `par_iter_mut().map(..).collect()` conserva el orden (`physics.rs:453`); la única suma en paralelo es de enteros (`ships.rs:200`) | lo paralelo puede quedarse |

### 2.2. Lo que lo impide (con dónde está)

| # | Obstáculo | Dónde | Qué hay que hacer |
|---|---|---|---|
| 1 | **El paso depende de los fotogramas** | `play.rs:799` (`dt = raw.min(0.1)`), `physics.rs:303` (`slices`: entre 1 y 3 lonchas de `dt/n`) | paso fijo (fase 1) |
| 2 | **Hay un segundo reloj**: los sistemas de las naves van a 50 Hz con su acumulador | `ship.rs:27` (`TICK = 1/50`), `Ship::update` | un tic de sistemas por paso de mundo (fase 1) |
| 3 | **La simulación vive en el bucle de la ventana** | `play.rs::frame` (1 930 líneas); `gear.update` y `world.update` reciben `&mut Renderer`; `blasts.update` devuelve la vista | `lunar-play` (fase 2) |
| 4 | **Un solo jugador** | `self.pilot`, `self.gear`, `self.hands`, `self.aboard` en `play.rs` | tablas por jugador (fase 2) |
| 5 | **Un solo observador para el ritmo de las naves** | `ships.update(…, eye, awake, …)` | lista de observadores (fase 2) |
| 6 | **El protocolo es de estados y ecos** | `Msg::Up` (estados del cliente), `Down` (reparto), `Tell { echo }` | protocolo 2 (fase 4) |
| 7 | **Las ids las fija el escenario o el primer cliente** | `MULTIJUGADOR.md` «Las naves del escenario no se anuncian» | `NetId` del servidor (fase 4) |
| 8 | **Las semillas del daño salen de contadores locales** | `blasts.rs:404`, `builds.rs:175` | semilla en cada golpe, del servidor (fase 6) |
| 9 | **El servidor no tiene mundo ni datos** | `crates/server` solo depende de `lunar-net` | cargar `Defs` y correr `lunar-play` (fase 3) |
| 10 | **Dos estructuras rápidas se atraviesan** (pasa hoy, con o sin red): solo se buscan contactos entre las que ya se solapan al empezar la loncha | `physics.rs:426` (`grid.along(c, c, …)`: el punto de partida, no el camino) y el filtro de debajo | barrido entre estructuras (fase 1, §3.1) |

---

## 3. Arquitectura de destino

```
            ┌──────────────────────── luna-servidor (sin gráficos) ────────────────────────┐
            │                                                                              │
 clientes ⇄ │ lunar-net ── comandos por paso ──► colas por jugador (con su colchón)        │
   (UDP)    │     ▲                                   │                                    │
            │     │ instantáneas por interés          ▼                                    │
            │     │ + hechos fiables          lunar-play: un paso de 1/60 s                │
            │     │                           naves → estructuras + jugadores + proyectiles│
            │  interés · prioridad ◄───────── hechos (golpes, altas, bajas, avisos)        │
            │  · diferencias por cliente                                                 │
            │                                 [más adelante: lunar-world en su hilo]       │
            └──────────────────────────────────────────────────────────────────────────────┘

 cliente = lunar-play (predice lo suyo, guía las copias de lo demás) + dibujo + sonido + HUD
 sin conexión = luna-servidor en un hilo del mismo proceso, por MemoryNet sin retraso
```

| Crate | Qué es | Cambia |
|---|---|---|
| `lunar-core` | física, cuerpos, estructuras, proyectiles | paso fijo; poses de pasos anteriores para repetir y compensar |
| `lunar-ship` | naves como datos | un tic por paso; `sync` enchufado |
| **`lunar-play`** (nuevo) | el juego sin gráficos: escenario, `Builds`, `Shots`, `Ships`, tácticas, jugadores (cuerpo, mano, herramienta, asiento, mandos), reglas | nuevo, con lo que hoy está en `app` |
| `lunar-net` | transporte, canal, reloj, sesiones | + comandos, instantáneas, interés, prioridad, diferencias, `NetId` |
| `luna-servidor` | el proceso | corre `lunar-play` a 60 Hz |
| `lunar-app` | el cliente: dibujo, sonido, HUD, entrada, predicción | adelgaza |

### 3.1. Un solo reloj de pasos fijos

- **El paso** (`Tick`, entero de 64 bits; en la red, los bits bajos con vuelta) dura exactamente
  1/60 s. La hora del mundo es `tick / 60`. Cada paso es **una** loncha (`Among` recibe 1/60) y un
  tic de sistemas de cada nave a ritmo pleno (las de `Pace::Slow`/`Asleep`, cada tantos pasos,
  contados en pasos, no en segundos sueltos).
- **En el cliente**, el fotograma acumula tiempo real y corre de 0 a 4 pasos (más, y el juego va
  más lento, como hoy con `MAX_SLICES`). **Se dibuja entre los dos últimos pasos** con
  `α = resto / paso`: cada estructura interpolada (posición en `f64`, giro por el camino corto,
  articulaciones), y lo que va a bordo **en el marco de lo que lo lleva** (el jugador por su
  `Ride::local` interpolado y luego llevado con la pose interpolada de la nave): así nada tiembla
  respecto a la nave a 7,8 km/s. Al cambiar de marco entre dos pasos se llevan los dos pasos al
  marco nuevo (como `Frames.carryTrack` en la web).
- **La mirada no espera al paso**: el ratón gira la cámara en cada fotograma; el paso usa la mirada
  que había al empezar (es parte del comando).
- **Por qué no pasos variables que el cliente cuenta** (como hacían Quake o Source con la duración
  de cada comando): el servidor tendría que dar a cada jugador sus pasos propios, fuera de las
  lonchas de las estructuras, y volverían los **dos relojes** de la V37 (`MOVIMIENTO.md` §1).
- **Regla que cambia**: `MOVIMIENTO.md` §2.1 dice «lonchas de 1/60 s como mucho… ni
  interpolación». Pasa a «pasos de 1/60 s exactos para todo y el dibujo entre los dos últimos,
  todo del mismo instante». Sigue habiendo **un** reloj; lo que se quita es que dependa de los
  fotogramas. Hay que cambiarlo también en `CLAUDE.md` (decisión 1, §8).
- **De regalo**: el juego deja de depender de los fotogramas también sin red (hoy un salto a 30 fps
  y a 240 fps no simula lo mismo).
- **Que dos naves no se atraviesen no depende de los Hz, sino del barrido.** Medido el
  2026-10-07 con dos bloques de 1,5 m que se acercan de frente, lejos de todo cuerpo:

  | Se acercan a | 60 Hz | 240 Hz |
  |---|---|---|
  | 5 y 50 m/s | chocan | chocan |
  | 200 m/s | **se atraviesan** | chocan |
  | 500, 1 000, 3 000, 7 800 m/s | **se atraviesan** | **se atraviesan** |

  Se atraviesan cuando lo que se acercan en un paso pasa de lo que miden juntos: a 7,8 km/s son
  130 m por paso a 60 Hz y 33 m a 240 Hz. Para pararlo subiendo los Hz harían falta miles por
  segundo. Hoy ya pasa (a 144 fps las lonchas son de 6,9 ms). Lo que lo arregla es lo que ya hacen
  los proyectiles (`structure::motion::Sweep`): mirar **el camino** de cada cuerpo en el paso, no
  solo dónde empieza, y si dos caminos se cruzan, llevar a los dos hasta el instante del contacto,
  resolverlo ahí y seguir con lo que queda del paso. Solo cuesta cuando dos cosas rápidas están
  cerca. Lo que importa es la velocidad **de una respecto a la otra**: dos naves en la misma
  órbita a 7,8 km/s que se acercan a 10 m/s recorren 17 cm por paso entre ellas.

### 3.2. Tres tiempos en cada cliente

| Tiempo | Qué es | Qué se dibuja así |
|---|---|---|
| `S`: el del servidor | la verdad; llega con la mitad del RTT de retraso | nada directamente |
| `P`: el predicho | `S` + la mitad del RTT + un colchón de 1–2 pasos: tus comandos llegan justo a tiempo | tu cuerpo, tu nave si la pilotas, lo que llevas, y **las copias de lo demás llevadas a `P`** |
| `D`: el de dibujo | `P` − (1 − α) pasos | todo |

- **Ajuste fino**: cada instantánea dice cuántos pasos antes (o después) llegó tu último comando.
  El cliente corre un 2–5 % más deprisa o más despacio hasta tener el colchón que toca, sin saltos
  (lo que hace Overwatch). Si se va más de 10 pasos, se recoloca de golpe una vez.
- **Lo ajeno en `P`**: las copias de las naves y los trozos cercanos se siguen simulando en tu
  máquina (hay que pisarlas y chocar con ellas) y se guían hacia el estado del servidor llevado a
  `P` (`follow::steer`, como hoy hacia el del dueño). A una nave ajena se le dan además **las
  teclas que su piloto tiene ahora** (vienen en la instantánea): su copia maniobra como la de
  verdad en vez de seguir recta. Lo que no se simula a fondo (`Coarse`/`Dormant`) se pone donde
  dice el servidor.
- **Lo que no se puede quitar**: de lo ajeno se sabe tarde lo que decide quien lo maneja. Hoy,
  un RTT (del dueño al servidor y del servidor a ti); con el servidor, un RTT y medio más el
  colchón, porque tú vas por delante del servidor. A 7,8 km/s la copia está donde predice su
  física con las teclas de su piloto; el error solo existe cuando el piloto cambia de idea en ese
  tiempo, y se reparte sin saltos. Las pruebas lo comparan con su cota física, como hoy.

### 3.3. Predicción y corrección, cosa por cosa

| Qué | En tu máquina | Cuando llega la verdad |
|---|---|---|
| **Tu cuerpo** | predicho con tus comandos | **rebobinar y repetir**: se guarda por paso tu estado (~200 B) y tu comando, 128 pasos. Si el del servidor para el paso `N` difiere de lo guardado más que lo que da la cuantización, se pone el suyo y se repiten `N+1…P` con los comandos guardados y **las poses guardadas de las estructuras que tienes a menos de 30 m** (como `Structures::posed` hace con las articulaciones). Tu estado viaja en el marco de lo que te lleva (`ride` + `local`): lo que la copia de esa nave difiera del servidor no cuenta. Lo que salta el ojo se reparte en ~60 ms; más de 2 m, de golpe |
| **Tu nave, si la pilotas** | predicha entera (física y sistemas) | **sin rebobinar la nave**: el error en `N` (servidor − lo que predijiste para `N`) se suma a la de ahora y se reparte. Lo discreto (interruptores, disyuntores, retenes) se pone tal cual del servidor; si el resumen (`Digest`) no coincide, la foto entera |
| **Lo que llevas en la mano** | predicho (el muelle de la mano, `hands.rs`) | como la nave |
| **Naves, trozos y cajas ajenos cerca** | copias simuladas | guiadas (`follow::steer`) |
| **Lejos** | sin simular | puestas donde dice el servidor |
| **Otros jugadores** | dibujados desde sus estados (como hoy) | — |
| **Tus disparos** | copia provisional al momento: fogonazo, trazadora, sonido, retroceso | el servidor le da su id; acaba donde diga `End` |
| **El daño** | **no se predice** (§3.4) | el golpe del servidor, con su semilla y su postura, se aplica igual en todas |
| **Un mando que tocas** | se mueve al momento | si el servidor lo niega (sin energía, otro va a los mandos), vuelve |
| **Máquinas, aire, depósitos** | espejo que se simula | diferencias del servidor + resumen; foto entera si no coincide |

**Por qué no se rebobina una nave entera**: son ~500 piezas con contactos y sus sistemas; repetir
10–20 pasos en cada corrección pide guardar la nave entera en cada paso (kilobytes) y
multiplica el coste. En vuelo libre la nave predicha y la del servidor reciben las mismas teclas en
el mismo paso: las diferencias son la cuantización y algún comando que llegó tarde, y repartir el
error basta. Si al medir no basta (posada y empujada, atracando), la red de seguridad es rebobinar
solo su cuerpo rígido con sus contactos (fase 7).

### 3.4. El daño: hechos ordenados y estado de reserva

- El servidor vuela los proyectiles y decide los golpes. Cada golpe sale como **hecho fiable y
  ordenado** a quien tiene esa estructura en su interés: `Strike { id, golpe en su marco, semilla,
  postura, paso }` (lo que hoy viaja con eco, ≤ 42 bytes más la postura).
- Cada cliente lo aplica a su copia (`strike_done`): el resultado es el mismo bit a bit (ya está
  probado con tres partidas, todas las armas, lo articulado y lo que revienta). Los trozos nacen
  con el mismo linaje → **la misma `NetId` sin mandarlos**; desde ese momento sus posiciones vienen
  en las instantáneas.
- **Lo que depende de lo que se simula aparte** (si un depósito revienta según lo que llevaba) lo
  decide el servidor y lo manda como otro golpe (`Strike::Blow`), como hoy lo decide el dueño.
- **Red de seguridad**: cada estructura en interés manda su `Digest` cada 2 s, repartidos en el
  tiempo. Si no coincide, el servidor manda `write_state` (y `write_make` si es un trozo que el
  cliente no tiene). Entrar tarde es lo mismo: la foto de lo que hay.
- **Por qué no se predice el daño**: cambiar una estructura (piezas que se van, trozos con nombre)
  y luego deshacerlo es lo más caro y lo que más se ve si sale mal. El fogonazo y las chispas sí son
  inmediatos; la rotura llega medio RTT después (30–80 ms en una red normal).

### 3.5. Disparar y acertar (compensación de retraso)

**Por qué hace falta, con un ejemplo.** Ana tiene 100 ms de ida y vuelta con el servidor. Luis
corre de lado a 3 m/s. Lo que Ana ve de Luis tiene ya 100–150 ms (lo que tardó en llegarle y el
margen para dibujarlo suave), y su disparo tarda otros 50 ms en llegar al servidor. Cuando el
servidor lo mira, Luis ya está 45–60 cm más allá. Si el servidor juzga con dónde está Luis *ahora*, a Ana se le
escapan disparos que en su pantalla eran perfectos («¡pero si le he dado!»). La compensación es
que el servidor **recuerda dónde estaba cada uno** los últimos 300 ms y juzga el disparo con
**donde Ana veía a Luis** al apretar el gatillo. El precio lo paga Luis: a veces le dan cuando en
su pantalla ya se había metido tras una roca («me han dado detrás de la esquina»). El tope (la
decisión 3, §8) es hasta dónde se perdona: con 200 ms, quien tiene una conexión peor que eso tiene
que adelantar el tiro.

- Tu disparo es un bit del comando del paso `P` (y el arma, la del asiento o la mano). El servidor
  lo procesa **en su paso `P`**, desde la boca de **su** nave o **su** cuerpo en ese paso, que es
  la que tú predijiste: sale de la boca en todas.
- Contra **naves y estructuras** no hace falta rebobinar: tú las veías llevadas a `P` y el servidor
  las tiene de verdad en `P`. El error es lo que el piloto ajeno cambió de idea en ese RTT.
- Contra **personas** (que cambian de dirección al momento) y para disparos instantáneos, el
  servidor reconstruye lo que vio el tirador (la misma cuenta que hizo su cliente desde la
  instantánea que tenía) y acepta el golpe si coincide, hasta un tope de antigüedad
  (decisión 3, §8). Lo guarda el servidor: un anillo con las poses de los últimos 300 ms.

### 3.6. Interés, prioridad y diferencias

Port de `shared/net/interest.ts` de la web (`RED.md`), ampliado con niveles:

| Clase | Completa (se simula la copia) | De rastro (solo cuerpo rígido, 1–5 Hz) |
|---|---|---|
| Naves | 2 km para aprender, 2,5 km para olvidar | hasta 100 km / 110 km (radar a 80 km, puntos en el cielo) |
| Jugadores | 2 km / 2,5 km | hasta 100 km (nombre en el HUD) |
| Trozos, cajas, bidones | 1,5 km / 1,8 km | — |
| Proyectiles | los que pueden cruzarse contigo en su vuelo | los guiados, como rastro |
| Lo que va a bordo del mismo anfitrión que tú | siempre | — |
| Lo **fijado** (tu cuerpo, tu nave, lo que llevas) | siempre | — |
| Tráfico | nunca: es una fórmula (semilla + hora); si algo lo toca, pasa a ser una nave del servidor | — |

- **Rejilla** sobre posiciones del mundo (celdas de 1 km para la completa, 32 km para el rastro):
  cada actualización cuesta lo que hay cerca, no lo que existe. Se recalcula cada 6 pasos.
- **Prioridad acumulada** (Fiedler, *State Synchronization*): cada cosa acumula prioridad cada paso
  (por cercanía, tamaño, velocidad relativa, si cambió, si está fijada); cada instantánea se llena
  por orden hasta el presupuesto de bytes del cliente y lo mandado vuelve a cero. Tras una gran
  explosión los trozos pequeños y lejanos van más despacio, pero van.
- **Diferencias** contra la última instantánea que el cliente acusó (anillo de 32 por cliente): lo
  que no cambió no viaja; lo quieto lleva un bit de reposo (`throttle.rs` y el bit `MOVING` ya lo
  hacen por cosa).
- **Ritmo**: 30 instantáneas por segundo en la completa (cada 2 pasos), lo de rastro según
  prioridad.

### 3.7. Nombres: `NetId`

- Toda cosa replicada tiene una `NetId` de 64 bits (en la red, varint) que da el servidor. Las del
  escenario salen en el `Welcome` (o se derivan de su id del escenario, igual en todas: la tabla
  `Named` de hoy, `id·4`).
- **Trozos**: la `NetId` sale de su linaje (`Named::Piece`, 61 bits), así que nacer de un golpe no
  cuesta ningún alta.
- **Tus proyectiles**: nacen con una marca provisional (jugador, número); el `Launch` del servidor
  dice qué `NetId` les tocó.
- **Naves puestas en partida**: las pone el servidor (a petición) y las anuncia con su foto.

### 3.8. El protocolo 2

| Mensaje | Dirección | Fiable | Qué lleva |
|---|---|---|---|
| `Cmd` | C → S | no; cada datagrama repite los 3 anteriores | paso; ejes (adelante, lado, vertical, alabeo: −127…127, listos para mando analógico); botones en bits (correr, impulso, saltar, agacharse, gatillo, apuntar, usar, estabilizador, linterna); mirada (16 bits cada ángulo, en el marco propio del jugador); cabeza libre; teclas mantenidas del asiento (`seat_keys`); a qué apunta la mano. ~12–20 bytes con diferencias |
| `Act` | C → S | sí, con su paso | lo discreto: un mando tocado (estructura, mando, valor), una puerta, sentarse y levantarse, coger y soltar, cambiar de herramienta, gesto, poner una nave, chat |
| `Snap` | S → C | no | paso; el último `Cmd` usado; cuánto llegó antes o tarde; tu estado en ese paso; las cosas por prioridad, con diferencias |
| `Event` | S → C | sí, en orden, con su paso | `Spawn` (foto: `write_make` + `write_state` + `write_ship`), `Gone`, `Strike`, `Launch` (con su `NetId`), `End`, `Burst`, `Seat`/`Pilot` (quién va a los mandos), `Denied` (un `Act` que no vale, con el porqué), `Said`, `Joined`, `Left` |
| `Digest` | S → C | no | resúmenes de las estructuras y naves en interés, repartidos en el tiempo |
| `Resync` | C → S | sí | «esto no me cuadra: mándame su foto» |

Lo de hoy que se va: `Up` (estados del cliente), `Down` (reparto), `Tell { echo }` para el daño y
lo visto, `Owner`/`Host` como autoridad de simulación (queda como «quién va a los mandos»), «el
primero que entra fija la cuenta».

### 3.9. El servidor por dentro

Un paso, cada 16,7 ms (dormir hasta el siguiente; el último milisegundo, esperando activo):

1. Leer datagramas: comandos a la cola de cada jugador (ordenados por paso), actos a su lista.
2. Por jugador, el comando del paso `S`; si falta, el último sin los bordes (un salto no se repite)
   y se anota «adivinado».
3. Los actos de este paso (o atrasados: en este paso), validados (§3.10).
4. `lunar-play::tick`: naves (sistemas, empujes, mecanismos) → estructuras y lo que vive entre
   ellas (jugadores y proyectiles), lonchas de 1/60 → lo que revienta.
5. Los hechos del paso a quien corresponda.
6. Interés (cada 6 pasos).
7. Instantáneas (cada 2 pasos), por cliente, dentro de su presupuesto.
8. Medidas: duración del paso (p50, p95, máx), bytes, comandos adivinados, correcciones.

Si un paso pasa de su tiempo, el siguiente sale antes; si va atrasado de forma sostenida, baja el
detalle (más cosas a `Coarse`) antes que el ritmo. La cuenta de pasos no se salta nunca.

Los datos (`assets/defs`) van junto al ejecutable; el escenario lo elige el servidor y lo dice en
el `Welcome` (con la versión del juego: otra versión, rechazada con su motivo, como hoy).

### 3.10. Lo que el servidor no se cree

| Qué | Cómo se comprueba |
|---|---|
| Ejes y botones | dentro de rango; la mirada no gira más que lo que un ratón puede en un paso |
| Disparar | el arma la llevas o es de tu asiento; cadencia de los datos; munición del servidor |
| Tocar un mando | estás a su alcance o en el asiento que lo manda (lo que ya decide `aboard`) |
| Coger algo | a tu alcance y nadie más lo lleva |
| Comandos | como mucho 2 por paso de media; el exceso se tira y se cuenta |
| Datagramas | firmados con una clave de la sesión (SipHash con clave, ~60 líneas, sin dependencias): quien no ve tu tráfico no puede meter comandos por ti. No es cifrado: contra quien sí lo ve (la misma wifi) haría falta un intercambio de claves, que pide una dependencia (decisión aparte, más adelante) |
| Cambio de dirección del router | la sesión se reconoce por su clave, no por la dirección |

Y ya no hay nada que *creerse* en lo demás: el cliente no puede decir dónde está ni a qué ha dado.

---

### 3.11. Intenciones: lo que el cliente interpreta y el servidor comprueba

Hay dos clases de entrada, y cada una se trata distinto:

| Clase | Qué es | Quién la convierte en lo que pasa |
|---|---|---|
| **Teclas del cuerpo** (`Cmd`) | ejes, saltar, agacharse, mochila, mirada, teclas mantenidas del asiento | el servidor, paso a paso, con el mismo `Pilot::step` que predice el cliente: la posición es verdad y solo la decide él |
| **Intenciones** (`Act`) | «este mando a este valor», «esta puerta abierta», «lanzo esto desde aquí hacia allí», «sueldo esta pieza», «cojo esto», «me siento aquí» | el cliente, que es el único que sabe dónde está la mira al píxel, la propone con su paso; el servidor la **comprueba contra su verdad** (estás a su alcance, llevas esa herramienta, tu arma está cargada, la boca está donde dice tu cuerpo o tu montaje, la cadencia es la de los datos) y la aplica o la niega (`Denied`, con el porqué) |

Así la interfaz (mirar, arrastrar una palanca, la rueda, las teclas a gusto de cada uno, la mano
que va al mando) se queda en el cliente y el servidor no necesita conocerla; y nada de lo que se
propone pasa sin que el servidor lo vea posible. Una herramienta nueva, un mando nuevo, un tipo
de arma nuevo: una intención nueva es una entrada en la tabla de intenciones con su comprobación,
nunca código en el núcleo de la red.

### 3.12. Para lo que vendrá: escalar sin tocar el núcleo

Fernando (2026-10-07): «que todo sea escalable: nuevas armas, nuevas mecánicas, nuevas naves,
nuevos planetas, nuevos objetos móviles, asteroides que entran a planetas… y que los documentos
del mundo puedan cumplirse a futuro» (*El universo que no te necesita* y *La simulación por
dentro*). Lo que pidió, lo que se deduce de ello y el enganche que deja el servidor para cada
cosa:

| Lo que vendrá | Qué le pide a la red y al servidor | El enganche |
|---|---|---|
| **Armas nuevas** | que nadie toque la red | ya son datos (`Launch` por su nombre del catálogo). El saludo lleva la **huella de los datos** (un hash de `assets/defs`): con otros datos, rechazo con su motivo, nunca un arma que en otra máquina es otra |
| **Naves nuevas** | su foto, sus asientos, sus teclas | `ShipKind` por nombre; la foto es `write_ship` (todo lo que guardan sus máquinas, `Machine::save`); las teclas son datos del asiento |
| **Mecánicas nuevas de nave** (módulos) | que su estado viaje y se guarde | lo que una máquina guarda es lo que viaja y lo que se guarda: un módulo nuevo no toca la red |
| **Objetos móviles nuevos** (drones, vehículos, grúas sueltas, ascensores) | que se repliquen, con su interés y su predicción | una **clase replicada** es una entrada en una tabla: cómo se escribe su estado, su radio de interés, si se predice o se guía, su prioridad |
| **Planetas y lunas nuevos** | que todo valga en cualquier cuerpo | `BodyRegistry` (ya), posiciones en `f64` en la red que valen en todo un sistema solar (8 bytes por eje), interés en coordenadas del mundo, nada que suponga un cuerpo |
| **Cuerpos que se mueven** (lunas que orbitan, planetas que giran) | que lo posado en un cuerpo que gira no se escurra | sus posiciones son una **fórmula del paso** (efemérides), igual en cliente y servidor: no se mandan. Lo posado va en el marco del cuerpo, como lo que va a bordo de una nave |
| **Asteroides que entran en planetas** | que no atraviesen el suelo; que se sepan antes de llegar; que dejen cráter | **barrido contra el suelo** (`ground_ahead`, hecho en la fase 1); **interés por horizonte**: se sabe lo que puede llegar hasta ti en los próximos segundos, no solo lo que está cerca (a 20 km/s, 2 km son 0,1 s); los **cráteres son estado del servidor** por cuerpo, se guardan y se mandan a quien llega |
| **Lo rápido y lejano en general** (misiles de largo alcance, naves en órbitas opuestas a 15,6 km/s) | lo mismo | el interés mira la velocidad relativa: radio = distancia que se recorre en el horizonte |
| **Naves dentro de naves, estaciones enormes** | anidar marcos | toda posición en la red va relativa a su anfitrión (`fr` + local), a cualquier profundidad |
| **Mucha gente** (la galaxia de 2 000 sistemas) | repartir el mundo | el `Welcome` dice la **región**; las ids son del mundo, no de la sesión: pasar de un servidor a otro al saltar es dar de baja aquí y de alta allí con la misma id |
| **Personas del mundo vivo** (PNJ con mente) | que anden y se vean como los jugadores | son jugadores sin red: el mismo andador en el servidor, su cuerpo se replica igual |
| **Lo oculto** («el juego lo sabe y tú no»: la grieta del núcleo 40-2291, el doble fondo) | que no viaje | cada dato de estado dice si es **visible**; lo oculto se queda en el servidor y no entra en lo que se manda ni en el resumen de lo visible |
| **Sensores y niebla** (radar, firma del motor, lo que no ves) | que no se pueda ver a través con trampas | lo lejano viaja como **lectura de sensor** (lo que tu nave podría saber), no como la verdad |
| **Causas** («todo tiene origen»: el historiador, las cajas negras) | saber por qué pasó cada cosa | cada hecho del servidor (golpe, rotura, robo, muerte) lleva **su causa** (quién o qué, y el hecho anterior): es lo que leerá el registro del mundo |
| **Lo tocado persiste** | que una caja robada siga siendo esa caja | la `NetId` es la **id del mundo**, que nunca se reutiliza (`MUNDO.md`) |
| **El tiempo del mundo** | un solo reloj | el paso es el reloj del núcleo del mundo (`now = paso / 60`); el mundo lejano va en su hilo a su ritmo |
| **Batallas lejanas, cohortes** («de lejos, números») | que lo lejano no cueste | lo que nadie ve no se simula pieza a pieza (`SimLevel`, ya) y, más adelante, se resume en números con conservación |
| **Atmósferas y viento** (entrada en un planeta con aire) | frenado, calor | es parte del campo del cuerpo (`field`): datos del cuerpo, igual en todos |
| **Radio con alcance, voces** | que llegue a quien está a su alcance | mensajes del mundo con su propio interés (por alcance de la emisión, no por distancia a la cosa) |
| **Mods y datos del servidor** | que el cliente juegue con los datos del servidor | la huella de los datos; más adelante, que el servidor los mande si no coinciden |

## 4. La gráfica del servidor

### 4.1. Qué tendría que cumplir algo para ganar en la GPU del servidor

1. Ser **mucho trabajo igual en paralelo** (miles de cosas parecidas), no lógica con ramas.
2. **No ser parte de la verdad** que el cliente predice, o ser algo que el cliente no necesita
   reproducir. La predicción del cliente es la CPU en `f64`: si el servidor calculara la verdad en
   la GPU (`f32`, distinta según marca y driver), cada diferencia sería una corrección.
3. **No esperar dentro del paso**: subir, calcular y leer el resultado por PCIe cuesta del orden de
   1–2 ms con su sincronización, y el paso entero tiene 16,7 ms.

### 4.2. Candidato por candidato

| Candidato | ¿Vale? | Por qué |
|---|---|---|
| Física de estructuras y contactos | **No** | árbol de cajas, ramas, cientos de cuerpos: hoy 1,25 ms de media con 20 naves cayendo (2,5 ms la peor). Falla en 2 y 3 |
| Choque con el terreno | **No** | ya es CPU en `f64` (`Surface::sample`), con el plano guardado por cuerpo. `terrain_gen.wgsl` es su gemelo en `f32` y existe para dibujar |
| Sistemas de las naves, señales, aire | **No** | lógica y grafos pequeños: 0,08 ms por nave |
| Proyectiles | **No** | 2 000 tramos contra 500 estructuras en 1,2 ms por loncha en la CPU |
| Mandar el terreno ya generado a los clientes | **No** | cada cliente lo genera en su gráfica en milisegundos; mandarlo costaría megas de red |
| Jugar en la nube (el servidor dibuja y manda vídeo) | **No** | 5–15 Mbit/s de subida por jugador, compresión que añade latencia, y la gráfica de cada jugador sin usar |
| Sensores de cientos de naves, vista de cientos de PNJ | **Quizá, a mucha escala** | hoy es barato en CPU (`tactics`: una pasada por nave que mira). Si un día hay miles de PNJ mirando, un lote de rayos contra el relieve; no antes de medirlo |
| Siglos de historia del mundo vivo | **No** | eventos discretos en una cola (`MUNDO.md`), no cálculo en paralelo |
| **Espectador y retransmisión** | **Sí, opcional** | una cámara del servidor que dibuja la partida (el mismo renderizador) para emitir o mirar: no toca la verdad |
| **Repeticiones** | **Sí, opcional** | el servidor guarda los comandos y los hechos; la repetición se dibuja después, a cualquier velocidad, desde cualquier cámara |
| **Pruebas con imagen** | **Sí** | los guiones de `tools/camara` con ventana oculta y las fotos de diagnóstico ya usan la GPU; en el ordenador del servidor de pruebas, las mismas, con bots |

### 4.3. Veredicto

- **Para la verdad del juego, fumada.** Lo que el servidor necesita es **CPU** (núcleos: la física
  ya reparte por cuerpos con `rayon`) y **subida de red** (unos 8 Mbit/s con 16 jugadores en
  combate). 4 núcleos bastan para empezar; se mide en la fase 3.
- **Para mirar, útil y barato** de hacer al final (fase 12), porque el renderizador ya existe.
- **El servidor no dependerá de tener gráfica**: tiene que correr en cualquier ordenador, también
  en un servidor alquilado sin GPU. El «ojo» del servidor es una opción (`--espectador`), en un
  hilo aparte, que nunca frena el paso.
- **Si el servidor corre en el PC donde tú juegas**, su gráfica ya la usa tu partida; el servidor no
  la toca. Lo que sí comparte es la CPU: ese PC simula el mundo dos veces (servidor y tu cliente).
  Con 8 núcleos no se nota; se mide en la fase 10.

---

## 5. Las fases

Cada fase: **qué**, **piezas**, **pruebas**, **hecho cuando**, **riesgos** y **tamaño** (S: una
sesión; M: 2–3; L: 4–6). Cada fase en commits pequeños con las pruebas en verde en cada uno.

### Fase 0 — Medir el punto de partida · S

**Qué.** Saber qué es igual y qué no antes de tocar nada.

**Piezas.**
- `play_headless` (`app`): el escenario corrido 60 s sin ventana con un guion de teclas, y su
  resumen (`Digest` de cada estructura y nave, posiciones con su cuantización).
- Una prueba que lo corre **dos veces en dos procesos**: si los resúmenes difieren, hay algo que
  depende del proceso (un mapa hash recorrido: `HashMap` usa una semilla al azar por proceso; a
  primera vista solo se usan para buscar —`physics.rs:255`, `broadphase.rs:10`—, pero la prueba lo
  dice). Y a 30, 60 y 144 fps: hoy **tienen** que diferir (es el obstáculo 1).
- Cifras de partida: CPU del paso con 1, 4 y 16 observadores repartidos; bytes por jugador.
- **Windows contra Linux**: el mismo resumen en los dos (las funciones `sin`, `exp`, `powf` las da la
  biblioteca del sistema y pueden diferir en el último bit). Decide si el servidor puede ir en
  Linux sin más correcciones.

**Hecho cuando** hay un número para cada cosa del §6 y una lista de lo que no es reproducible.

### Fase 1 — Paso fijo y dibujo interpolado · M

**Qué.** El mundo avanza en pasos de 1/60 s exactos, en el cliente sin red ya; el dibujo
interpola. Ningún comportamiento cambia salvo el que dependía de los fotogramas.

**Piezas.**
- `play.rs`: acumulador **del mundo** (uno solo), 0–4 pasos por fotograma.
- `physics::slices` deja de partir el paso; `Among` recibe siempre 1/60.
- **Barrido entre estructuras**: los vecinos de cada cuerpo se buscan a lo largo de su camino en
  el paso (`grid.along(inicio, fin, radio)`: la rejilla ya sabe hacerlo); a cada pareja cuyos
  caminos se cruzan se le calcula el instante del primer contacto (avance conservador contra el
  árbol de cajas de sus piezas) y se resuelve ahí. Lo de los proyectiles (`Sweep`) es el modelo.
- `lunar-ship`: `TICK = 1/60` (un tic por paso). `TIEMPOS.md` y `procedimientos.rs` lo vigilan: los
  procedimientos deben dar lo mismo (± un tic).
- Pose anterior y actual por estructura (posición, giro, huesos), por proyectil y del jugador (en
  su marco); `Renderer::set_structures` dibuja con `α`. Cámara con la mirada del fotograma.
- **La sonda de movimiento** (la web la tiene en F7; aquí solo existe dentro de las pruebas, en
  `Loop` de `pilot/tests.rs`): una tecla de depuración y un paso de guion que miden el salto por
  fotograma de todo lo que se ve, en el marco de lo que lo lleva. Es la vara de medir de las fases
  4 y 7.
- `MOVIMIENTO.md` §2 y `CLAUDE.md` con la regla nueva (decisión 1).

**Pruebas.** Todas las de hoy (`pilot::`, `schedule`, `physics`, `coherencia`, `multi::`) sin tocar
sus cotas. Nuevas: **dos bloques y dos naves de verdad que se acercan de frente de 5 a
15 600 m/s (dos órbitas opuestas) chocan siempre**, también de refilón y girando; **el mismo guion de teclas a 30, 60, 144 y 240 fps da el mismo resumen**
(lo que la fase 0 vio diferir); el salto del ojo por fotograma en el marco de la nave, igual o
menor que hoy, de 0 a 7 800 m/s y de 10 a 240 fps; la sonda en el juego con los guiones de
`tools/camara`.

**Hecho cuando** el juego sin red no depende de los fotogramas y no se nota distinto.

**Riesgos.** Lo que hoy aprovecha lonchas cortas a muchos fps (contactos finos, el tren con
muelles) se nota a 60 Hz fijos. Si pasa, la solución es 2 sub-lonchas fijas por paso (120 Hz) en
la física, nunca volver a lonchas variables.

### Fase 2 — `lunar-play`: la simulación sin gráficos y para N jugadores · L

**Qué.** Un crate sin `lunar-render`, `winit`, `egui` ni audio con todo lo que decide qué pasa,
para cualquier número de jugadores. El cliente es presentación + entrada.

**Piezas** (cada una, un commit con las pruebas en verde):
1. **Efectos como datos**: la simulación devuelve una lista de lo que se ve (`Shown { qué, dónde,
   velocidad, escala }`) en vez de llamar a las partículas; el cliente la pinta. Hoy
   `builds.update` y `blasts` llaman a `fx`.
2. `Builds` entera; `Blasts` en dos (`Shots`: lanzar, volar, guiados, señuelos, finales;
   `BlastsView`: partículas, luces, estelas, cámara tras el misil); `Ships` en dos (sistemas,
   lo que revienta, articulaciones / mallas, rótulos, pantallas); `Tactics`; `content::Defs`; el
   escenario.
3. **Jugadores como tabla**: `Players` con, por jugador, su `Pilot`, su mano (`hands`), su
   herramienta (`gear`: lo que hace; `holding` y su dibujo se quedan en el cliente), su asiento y
   lo que apunta (`aboard`: la lógica; el HUD se queda). Cada `Pilot` es un `Among`.
4. `Ships::update` con **una lista de observadores** (todos los jugadores) en vez de un `eye`.
5. `lunar-play::Game::tick(&[Cmd]) -> &Out`: el paso entero (lo que hoy está repartido en
   `play.rs::frame`), con las entradas de todos y lo que sale (lo que se ve, se oye, se avisa).
6. **Nada lee la vista dentro de la simulación**: hoy `blasts.update` recibe y devuelve la vista y
   el `Pace` de las naves usa `view.eye`; pasan a ser los observadores.

**Pruebas.** Las de hoy, movidas de crate sin cambiar. Nueva: el resumen de `play_headless` es el
mismo corrido en `lunar-play` solo que dentro de `app`. Otra: dos jugadores en la misma partida
local (sin red) andan, se sientan y disparan a la vez.

**Hecho cuando** `lunar-play` compila sin gráficos y `play.rs` solo dibuja, suena y lee teclas.

**Riesgos.** El trozo más enredado es la mano (`gear`, `holding`, `handwork`, `hands`): mezclan qué
hace con cómo se ve. Se separa por lo que cambia la verdad (qué agarra, a qué dispara, qué suelda)
contra lo que no (dónde están los dedos).

### Fase 3 — Un servidor que simula, en la sombra · M

**Qué.** `luna-servidor` corre `lunar-play` a 60 Hz con el mismo escenario, con los jugadores que
entran, **sin mandar nada nuevo**: los clientes siguen con el protocolo de hoy y además le mandan
sus comandos. El servidor compara su mundo con lo que cuentan.

**Piezas.** El servidor carga `Defs`; bucle de paso fijo (§3.9); `Cmd` como mensaje nuevo (solo
de ida); diferencias anotadas en `servidor.log`; bots sin ventana (`lunar-play` + un guion de
comandos) que se conectan por UDP.

**Pruebas.** La mesa de pruebas con el servidor simulando (aplica los golpes con eco como una
partida más): tras cada prueba, el resumen del servidor igual al de los clientes en lo que ya
está sincronizado (daño, trozos al nacer). Carga: 16 bots
andando, volando y disparando.

**Hecho cuando** el paso del servidor tiene p95 < 8 ms con 16 bots en el escenario y sabemos qué se
separa y por qué.

**Riesgos.** CPU en batallas grandes: se mide aquí, antes de depender de ello.

### Fase 4 — Protocolo 2: comandos arriba, estado abajo; tu cuerpo, del servidor · L

**Qué.** Los jugadores pasan a ser del servidor, con tu cuerpo predicho. Es la base de todo lo
demás.

**Piezas.**
- `lunar-net`: `Cmd` (con redundancia), `Act`, `Snap`, `Event`; `NetId`; el `Welcome` dice el
  paso, el escenario y tu `NetId`; versión 2 del protocolo (la 1 sigue para el juego de hoy).
- Cliente: los tres tiempos (§3.2) con el ajuste fino; anillo de 128 pasos (comando, estado,
  poses de lo cercano); rebobinar y repetir (§3.3); el salto del ojo repartido.
- Servidor: colas por jugador, comandos adivinados, tu estado en tu instantánea.
- Los demás jugadores, dibujados desde el servidor (lo de hoy, `multi::bodies`).

**Pruebas.**
- En red limpia, 60 s andando, con mochila, de pie en una nave que vuela a 0, 300, 1 600 y
  7 800 m/s y sentado: **cero correcciones**.
- Con 40 ± 10 ms y 3 % de pérdidas, y con 150 ms y 10 %: correcciones raras y el salto del ojo por
  fotograma por debajo de lo de hoy sin red.
- Un empujón que solo ve el servidor (una explosión): el cuerpo se corrige sin salto visible.
- El ajuste fino: con RTT de 20 a 300 ms y ±30 ms de variación, el colchón se asienta en < 2 s.
- Las de hoy de quien flota junto a una nave o va de pie en una ajena, ahora contra el servidor.

**Hecho cuando** nadie dice dónde está: el servidor lo sabe, y moverse se siente igual que sin red.

### Fase 5 — Interés, prioridad y diferencias · M

**Qué.** Cada cliente sabe lo que tiene cerca (§3.6) y la bajada cabe en su presupuesto.

**Piezas.** `interest.rs` (port de `interest.ts`, 258 líneas): histéresis, mismo anfitrión, fijados,
rejilla, niveles completo/rastro; acumulador de prioridad; diferencias contra lo acusado; altas y
bajas.

**Pruebas.** Port de `tools/net/check.ts` («réplica por interés contra fuerza bruta»): mil cosas,
jugadores moviéndose al azar; cada uno sabe exactamente lo que dice la regla, nada parpadea en el
borde, un `Gone` perdido no deja fantasmas. Ancho de banda (`tests/bandwidth.rs` ampliado): 16
jugadores, 20 naves, 100 trozos.

**Hecho cuando** la bajada por jugador en combate es < 64 kB/s y un jugador que llega de lejos ve
cada cosa donde está.

### Fase 6 — Disparos, daño y trozos del servidor · M

**Qué.** Ningún cliente decide un golpe.

**Piezas.** El gatillo en el `Cmd`; el servidor lanza (`Shots`), vuela y golpea; `Launch`, `End`,
`Strike` y `Burst` del servidor con su semilla; copias provisionales en el cliente que casan con el
`Launch`; compensación para personas e instantáneos (§3.5); resúmenes y fotos de reserva (§3.4);
se quita `tell_all` para el daño. Las armas de las naves: el gatillo de su asiento.

**Pruebas.** Las de hoy (`every_weapon_in_the_data_…`, el cohete desde la bodega a 7,8 km/s, el
guiado contra el Cachalote que esquiva, dos tiradores a la vez) con el servidor decidiendo: mismo
resultado y **servidor y clientes con el mismo resumen**. Nuevas: un cliente tramposo que dice
que acertó o dispara más deprisa que su arma no consigue nada; un jugador que llega tarde ve los
trozos donde están.

**Hecho cuando** el daño solo lo decide el servidor y todas las copias acaban iguales.

### Fase 7 — Las naves, del servidor; el piloto predice · L

**Qué.** El cuerpo rígido, las articulaciones, las máquinas, las señales, el aire y los depósitos
de cada nave son del servidor. El piloto predice su nave; los demás la ven guiada, con sus teclas.

**Piezas.**
- Las teclas del asiento y los gases en el `Cmd`; los mandos tocados en `Act`; el servidor aplica
  ambos en su paso.
- Cliente piloto: su nave predicha; la corrección por error en `N` (§3.3); interruptores y retenes
  del servidor; espejo de sistemas con `Shadow::delta` y `Digest`.
- Los demás: copia guiada con las teclas del piloto.
- `ShipSync`: la foto entera (`write_ship`) al entrar en el interés y a quien entra tarde. Cierra el
  pendiente «el estado de las máquinas al entrar tarde».
- «Quién va a los mandos» del servidor (sentarse es un `Act`); sin nadie, la nave sigue
  simulándose en el servidor (piloto automático, caída, órbita).

**Pruebas.** Las de vuelo de hoy (formación de 0 a 7 800 m/s, de 30 a 240 fps, red mala) con el
servidor al mando; `coherencia.rs` corrido **por el servidor con las teclas que llegan por la red**;
un jugador que entra a mitad ve cada nave con sus máquinas como están; dos jugadores, uno pilota y
otro va de pie en la bodega; relevo del piloto a 7,8 km/s sin tirón; la nave sin piloto sigue en
órbita igual para todos.

**Hecho cuando** pilotar con 40 ms de red se siente como sin red (la sonda: ningún salto mayor que
hoy) y lo que hace cada máquina es igual para todos.

**Riesgos.** Es la fase de más riesgo para la sensación. Red de seguridad, si la medida no llega:
rebobinar el cuerpo rígido de la nave con sus contactos (no sus sistemas). Último recurso, sin
quitar nada de lo demás: que el piloto mande en el cuerpo rígido de su nave y el servidor lo
compruebe (lo que proponía el plan anterior); queda como opción, apagada.

### Fase 8 — Carga suelta, lo que llevas en la mano y las herramientas · M

**Qué.** Cajas, bidones, piezas y lo que se arrastra, del servidor; quien lo lleva, lo predice.

**Piezas.** Coger y soltar son `Act`; lo que llevas lo mueve tu mano predicha en tu máquina y la
misma mano en el servidor (sale de tu comando: mirada, distancia de la rueda); al soltarlo vuela
como cualquier cuerpo, del servidor. La carga anclada en una nave (`structure::hold`) va con su
nave. Soldar y escanear: el gatillo y a qué apuntas en el `Cmd`; reparar, del servidor. Lo que
llevan y hacen los demás (herramienta, gesto, muñeca) en su estado.

**Pruebas.** Lanzar una caja desde una bodega a 7,8 km/s: cae igual para los tres; dos que agarran
la misma: la coge uno; soldar un panel que otro ve reparado al mismo tiempo; la caja que nadie toca
reposa y no cuesta nada (`SimLevel::Dormant`).

### Fase 9 — Entrar tarde, reconectar y guardar · M

**Qué.** Quien entra recibe lo que tiene cerca en pocos mensajes grandes; quien se cae vuelve a su
sitio; el servidor guarda la partida.

**Piezas.** Puesta al día por prioridad (lo cercano primero), con pantalla de carga hasta
`Synced`; la sesión vuelve por su clave en 60 s (tu cuerpo te espera); guardado en dos ranuras
(`write_make` + `write_state` + `write_ship` + el estado rígido de cada cosa + el paso), al salir y
cada 5 minutos. Más adelante el guardado pasa al núcleo del mundo (plan del mundo, fase 6).

**Pruebas.** Entrar con 100 trozos, 6 naves y 10 jugadores en marcha con 10 % de pérdidas: en
< 2 s se juega; cortar la red 20 s y volver; parar el servidor y arrancarlo: todo donde estaba.

### Fase 10 — Sin conexión = servidor en el proceso; quitar el protocolo 1 · S–M

**Qué.** Una sola forma de jugar; lo viejo se va.

**Piezas.** El juego de un jugador arranca `luna-servidor` en un hilo y se conecta por `MemoryNet`
sin retraso (la predicción siempre acierta); el anfitrión que juega en el PC del servidor, por
`127.0.0.1`; el lanzador con «crear partida» y «unirse». Se quitan `Up`, `Down`, `Tell { echo }`
para el daño y la autoridad de los clientes.

**Pruebas.** Todo el juego sin red se siente igual que antes de la fase 1 (guiones de
`tools/camara`, rendimiento de `tools/rendimiento`); CPU del PC que es servidor y juega a la vez.

**Riesgos.** El mundo simulado dos veces en un solo PC. Si pesa, un atajo medido después: sin
conexión, el cliente usa el mundo del servidor directamente para lo que no predice.

### Fase 11 — Seguridad y límites · S

Las comprobaciones de §3.10, la firma de los datagramas, límites de mensajes por jugador y de
tamaño de lo que se pide (`Resync` en bucle), y pruebas con un cliente tramposo hecho a mano por
cada regla y con basura contra un servidor en marcha (lo que ya hace `session.rs`).

### Fase 12 (opcional) — El ojo del servidor · M

`--espectador`: un hilo que sigue a un jugador o una cámara libre y dibuja con el renderizador de
siempre, a su ritmo, sin frenar el paso; grabar repeticiones (comandos y hechos) y verlas después;
los bots de prueba con foto. Va en una opción de compilación aparte (`--features espectador`): el
servidor de siempre no enlaza el renderizador ni necesita gráfica.

### Después: el mundo

Las fases 6 a 8 de [`PLAN_SERVIDOR_MUNDO.md`](PLAN_SERVIDOR_MUNDO.md) (núcleo del mundo en Rust,
personas, galaxia) van encima de esto, con un cambio: su puente ya no habla con clientes que
simulan, sino con `lunar-play` dentro del servidor.

### Orden y lo que puede ir a la vez

```
0 → 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11
                       └─ 12 en cualquier momento desde aquí
            └─ núcleo del mundo (plan del mundo, fase 6) en paralelo desde aquí
```

| Fase | Qué se nota jugando | Tamaño |
|---|---|---|
| 0 | nada (medidas) | S |
| 1 | el juego no depende de los fotogramas | M |
| 2 | nada (orden por dentro) | L |
| 3 | nada (el servidor aprende a simular) | M |
| 4 | **tu cuerpo es del servidor y se siente igual** | L |
| 5 | cada uno sabe lo que tiene cerca; partidas más grandes | M |
| 6 | **nadie decide el daño salvo el servidor; trozos en el mismo sitio** | M |
| 7 | **las naves y sus máquinas, iguales para todos, también para quien entra tarde** | L |
| 8 | cajas y lo que se lleva en la mano, compartidos | M |
| 9 | entrar tarde y volver tras un corte; la partida se guarda | M |
| 10 | una sola forma de jugar | S–M |
| 11 | nadie puede hacer trampas | S |
| 12 | ver la partida desde fuera, repeticiones | M |

---

## 6. Presupuestos y medidas

| Qué | Objetivo | Cómo se mide |
|---|---|---|
| Paso del servidor (60 Hz) | p95 < 8 ms, máx < 14 ms con 16 jugadores, 20 naves, 100 trozos | contador por paso en `servidor.log` (fase 3) |
| Instantáneas por cliente | < 0,1 ms por cliente y envío | idem |
| Bajada por jugador | < 64 kB/s en combate; < 8 kB/s en calma | `tests/bandwidth.rs` ampliado |
| Subida por jugador | < 6 kB/s (60 comandos por segundo con redundancia) | idem |
| Subida del servidor | < 8 Mbit/s con 16 jugadores en combate | idem |
| Correcciones de tu cuerpo | 0 en red limpia; < 1 por segundo con 3 % de pérdidas | contador en el cliente |
| Salto del ojo por fotograma | ≤ lo de hoy sin red, en todas las velocidades y fps | `pilot::` y la sonda |
| Error de una copia ajena | dentro de su cota física (velocidad relativa × lo que el piloto cambió en un RTT) | las pruebas de formación |
| Repetir tu cuerpo 20 pasos | < 0,2 ms | prueba con reloj |
| Entrar tarde | < 2 s jugando con 10 % de pérdidas | fase 9 |
| Coste de lo quieto | nada por paso | `SimLevel::Dormant` |

Todo lo nuevo entra en [`OPTIMIZACION.md`](OPTIMIZACION.md) con su medida, como siempre.

---

## 7. Pruebas de todo el camino

- **La mesa de pruebas** (`multi/tests.rs`): pasa a ser servidor que simula + N clientes por la red
  en memoria que pierde, duplica y retrasa. Es la referencia de cada fase.
- **Resúmenes** (`Digest`) en el servidor y en cada cliente: lo compartido coincide siempre (salvo
  la cuantización, con su cota).
- **La matriz de `CLAUDE.md`** para todo lo que se mueve: los dos cuerpos y uno inventado; dentro,
  en la franja y fuera de su influencia; de 0 a 7 800 m/s; de 10 a 240 fps; la nave derecha,
  volcada y girando (`app/src/pilot/frames.rs`). Ahora además por la red: limpia, 40 ± 10 ms con
  3 % de pérdidas, 150 ms con 10 %, y variación de 50 ms con desorden.
- **Bots** sin ventana contra un servidor real por UDP: 8, 16 y 32, para CPU y ancho de banda.
- **Tramposos**: un cliente hecho a mano por cada regla del §3.10.
- **Basura**: datagramas al azar y mensajes reales cortados contra cada descodificador nuevo, sin
  un `panic` (como `wire.rs` hoy).
- **Nada sin prueba**: cada clase replicada, cada mensaje (ida y vuelta, cortado, basura), cada
  regla de autoridad.

---

## 8. Decisiones para Fernando

| # | Qué | Recomendación | Alternativa |
|---|---|---|---|
| 1 | **Paso fijo de 60 Hz y dibujo interpolado** (cambia una regla de `CLAUDE.md` y `MOVIMIENTO.md`) | sí: sin esto no hay servidor autoritativo que funcione | 120 Hz: el doble de CPU en el servidor; solo si la física lo pide (fase 1) |
| 2 | Ritmo de las instantáneas | 30 por segundo cerca, de 1 a 5 lejos | 60 cerca: el doble de bajada |
| 3 | Retraso que se perdona al tirador contra personas | 200 ms | más: el que recibe el disparo nota «me dieron tras la esquina» |
| 4 | Dónde corre el servidor | proceso aparte que vale en tu PC, en otro o en un servidor alquilado sin gráfica (4 núcleos) | solo dentro del juego del anfitrión: más fácil de arrancar, peor para quien no es anfitrión |
| 5 | Sin conexión | el servidor en el proceso (una sola forma de jugar) | el juego de un jugador aparte: dos caminos que probar siempre |
| 6 | Daño predicho | no (fogonazo inmediato, rotura medio RTT después) | sí: mucho más código y deshacer roturas cuando falla |
| 7 | Tope de jugadores | 16 (lo de hoy) | 32: medir en la fase 3 primero |
| 8 | La gráfica del servidor | no para simular; espectador y repeticiones en la fase 12 si te apetece | — |
| 9 | Protocolo de hoy durante el cambio | se queda detrás de una opción hasta la fase 10 | quitarlo ya: menos código, pero el multijugador no funcionaría durante meses |

---

## 9. Riesgos

| Riesgo | Cuánto | Cómo se cubre |
|---|---|---|
| Pilotar se siente peor con red | alto | la nave predicha con el mismo código y las mismas teclas en el mismo paso; la sonda; redes de seguridad de la fase 7 |
| El paso fijo cambia el tacto (contactos, muelles) | medio | todas las pruebas de movimiento y coherencia; sub-lonchas fijas si hace falta |
| Separar la simulación del dibujo es largo y enredado | medio | un trozo por commit; las pruebas de hoy, movidas sin cambiarlas, en cada uno |
| CPU del servidor en batallas de cientos | medio | niveles de detalle por observador (ya existen); medido en la fase 3 antes de depender de ello |
| Ancho de banda tras grandes explosiones | medio | prioridad acumulada, bit de reposo, trozos pequeños y lejanos más despacio |
| Servidor en Linux y clientes en Windows simulan distinto en el último bit | bajo–medio | fase 0 lo mide; con estado replicado y resúmenes solo se traduce en más correcciones, nunca en partidas separadas |
| Algo depende del orden de un mapa hash o de una suma en paralelo | bajo | fase 0 lo busca con dos procesos; regla: nada de sumas de coma flotante en paralelo en la simulación |
| Lo ajeno a muy alta velocidad va un RTT por detrás de lo que sabe el servidor | inevitable | las teclas del piloto en la instantánea; la cota física en las pruebas |

---

## 10. Lo que se aprovecha y lo que se tira

**Se aprovecha casi todo**: `lunar-core`, `lunar-ship` (y `sync.rs`, que por fin se enchufa), el
transporte y el canal de `lunar-net`, el reloj, la cuantización, `follow::steer` y la llevada al
presente, los linajes, los golpes con postura, `Launch`/`Seen`, la mesa de pruebas, las pruebas de
vuelo, de coherencia y de todas las armas.

**Se tira**: los estados que manda el cliente (`Up`), el reparto del servidor (`Down`), el eco de
daños y de lo visto (`Tell { echo }` para eso), la autoridad de simulación de los dueños (queda
«quién va a los mandos»), los avisos de que una partida es dueña (`Structure::owned` y `remote`
cambian de sentido: «lo predigo» y «lo guío»), las ids por orden del escenario y «el primero que
entra fija la cuenta».
