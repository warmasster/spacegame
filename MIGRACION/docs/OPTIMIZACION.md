# Optimización: problemas pendientes

Lista viva de lo que cuesta de más en el prototipo en Rust (`migracion/`). Las reglas generales de
rendimiento del juego en TS están en [`../../docs/RENDIMIENTO.md`](../../docs/RENDIMIENTO.md).

**Regla para todos los agentes:**
- Si tocas algo de esta lista, **táchalo** en el mismo cambio: `~~título~~`, con la fecha y una
  línea de qué se hizo y cómo se midió.
- Si lo arreglas a medias, deja la entrada abierta y apunta qué falta.
- Si encuentras un problema nuevo de rendimiento, añádelo aquí aunque no lo arregles.

Prioridad: **A** (se nota jugando), **B** (se nota con muchas cosas en pantalla), **C** (limpieza o
código muerto).

## V41g (2026-10-08): batalla de cien naves (`online::a_battle_of_a_hundred_ships`)

Cincuenta Azores por bando, a 2 km, que se manejan solos por sus mandos (radar, transpondedor,
traza ante el morro hostil y fijada, piloto automático en PERSEG., cañones en fuego automático y
luego misiles), un jugador mirando entre las líneas; servidor y cliente en el mismo proceso, en un
contenedor de 4 núcleos. 90 s de batalla en 98 s de reloj; 28 naves deshechas, 464 pedazos,
20 000 impactos; 1 corrección, 0 peticiones de nave entera, 0 mensajes demasiado grandes.

- ~~**A. Cada impacto pesaba la nave entera.**~~ (2026-10-08) Cada impacto, aunque solo quitara
  puntos de vida, volvía a pesar la nave (masa, centro, inercia, el índice de sus piezas) y la
  miraba entera para ver qué se soltaba (`breakup::apply` → `refresh`, `detach`, `tidy`). Ahora
  solo cuando algo se rompe (una pieza perdida o desconchada, una unión que cede); el resto solo
  sube la versión. Medido en la batalla, en los 10 s del intercambio de cañonazos (18 000
  impactos): aplicar los impactos pasó de 9,9 s a 0,6 s de CPU del servidor y el paso del servidor
  de 26 ms de media (p95 80, peor 110) a 10 ms (p95 19, peor 31). El resultado, el mismo impacto a
  impacto (mismas cuentas).
- **A. Los sistemas de cien naves.** En los 10 primeros segundos (todas arrancando, radares,
  dirección de tiro, piloto automático) `Ships::update` se lleva 10 ms por paso del servidor
  (4 núcleos); luego unos 4 ms. CPU de los sistemas de nave en esos 10 s, servidor y cliente
  juntos: máquinas 4,1 s, **paneles (indicadores, luces, pantallas: `panels.after`) 2,6 s**,
  mandos que escriben (`panels.write`) 1,4 s, ordenador de vuelo 1,4 s, uniones 1,2 s, táctico
  1,0 s, señales derivadas 0,5 s, aire 0,3 s. Lo más barato de quitar: los paneles de una nave
  que nadie mira (nadie a bordo ni cerca) no tienen por qué refrescar sus indicadores en cada
  paso. Falta hacerlo.
- **B. Lo que se manda en el cañoneo.** Al jugador entre las líneas le llegaban hasta unos
  600 kB/s mientras las cien naves se tiroteaban (los golpes de cada impacto y lo que vuela, a
  todo el que conoce la nave), 140 kB/s después. Juntar los golpes de un mismo paso por nave, o
  contar menos de lo que vuela lejos del jugador, está por hacer.
- **C. Equilibrio, no rendimiento:** el cañón de 20 mm apenas araña al Azor (5 000 impactos le
  quitan un 1 % de vida): lo que deshace naves son los misiles. Apuntado en `PENDIENTES.md`.

## V41f (2026-10-08): sesiones selladas

- **Medido:** sellar y abrir un datagrama de 1 200 bytes (ChaCha20-Poly1305) cuesta 5,75 µs entre
  los dos extremos (`seal::tests::sealing_and_opening_a_datagram_costs_little`): con 16 jugadores
  a 60 datagramas por segundo en cada sentido, unos 6 ms por segundo del servidor (0,6 % de un
  núcleo). Cada datagrama lleva 24 bytes más que sin sellar (16 más que con la firma de antes).
  Sin reservas: se sella en el búfer del canal y se abre en uno suyo. Comprobar las direcciones
  desconocidas abre a prueba (sin tomar nada) con cada sesión: como mucho 256 por segundo.

## V41e (2026-10-08): lo dormido en vuelo, el rastro, lo que vuela al guardar

- **Lo dormido en vuelo** avanza un paso de Verlet por segundo (`schedule::drift`): dos consultas
  a `field` (62 ns cada una) y, a menos de 12 km más lo que avanza de un suelo, la altura del
  terreno. Mil naves dormidas en órbita: unos 0,1 ms por segundo. Antes no costaba nada (no se
  movía) pero al despertar estaba en otra parte.
- **El rastro** (`Host::tracks`): por jugador y 5 veces por segundo, cada nave que no conoce entera
  (una consulta a `field` y una distancia) y cada jugador; lo de más cerca primero, en un datagrama.
  Con 24 naves y 16 jugadores, unos 400 cálculos cada 12 pasos. Sin reservas: búferes del `Peer`.
- **Guardar lo que vuela** escribe cada bala, misil, guiado y señuelo (60–100 bytes cada uno); la
  partida de la prueba con cuatro en el aire: 0,31 ms y 34 kB.
- **Visto (no es de este cambio):** `masa.rs`
  `a_ship_with_nothing_flowing_is_never_weighed_…` falla en este contenedor por tiempo: la Azor
  quemando cuesta 26–33 µs por tic y en reposo 14 (el límite es 1,5 × reposo + 5 µs = 26). La
  simulación de la nave sola no la toca este cambio; hay que medirlo en una máquina normal y, si
  se repite, ver qué hace de más quemando (sus 6 piezas con propelente se pesan 810 veces en
  400 s, lo previsto).

## V41d (2026-10-08): firmas y límites (fase 11)

- **Medido (ya no se usa: ahora se sella, V41f):** firmar o comprobar un datagrama entero
  (1 200 bytes) con SipHash-2-4 costaba 0,69 µs: con 16 jugadores a 60 datagramas por
  segundo cada uno, menos de 1 ms por segundo del servidor. Sin reservas: se firmaba en el búfer del
  canal.

## V41c (2026-10-08): reconectar y guardar la partida (fase 9)

- **Medido:** guardar 16 estructuras (34 kB) cuesta 0,2 ms en el hilo de la partida; el disco va en
  un hilo propio (`guardado`), así que ningún paso espera al disco. Retomar: 3 ms.
- **B · Pendiente:** `Ships::adopt` cuesta de 0,6 a 1,8 ms por nave (hace la nave entera: sistemas,
  máquinas, señales). Lo paga el jugador en su fotograma cada vez que le entra una nave en el
  interés (`Event::Made`); con varias a la vez (llegar a un puerto) es un tirón. Medido cronometrando
  `online::put_made` por estructura al retomar. Arreglo: hacer en otro hilo lo que es solo datos
  (catálogo, máquinas) o repartir las adopciones entre fotogramas.
- **C · Pendiente:** `save::write` hace un `Event::Made` por estructura (tres `Vec`) y copia los
  cráteres: cada 5 minutos no importa; con decenas de miles de estructuras, escribir directo al búfer.

## V41b (2026-10-07): el servidor que simula (`lunar_play::host`) y el jugador que predice (`online`)

- **Codificado una vez** (`net::append_event`): lo que va a muchos (un golpe, lo que nace, lo
  disparado) se escribe una vez y se copia a la cola de cada uno; `Made` se hace una vez por paso
  aunque lo conozcan diez.
- **Por jugador, en todos los hilos**: el interés y las instantáneas de cada jugador se hacen en
  paralelo (`par_iter_mut`), sin reservar en el camino caliente (instantáneas, órdenes y listas
  reutilizadas por jugador).
- **Lo anclado no viaja** en instantáneas; lo que reposa, tres veces y no más hasta que se mueva.
- **A · Pendiente:** `Ships::update` reserva por paso un `HashMap`, dos `Vec` y a veces un
  `HashSet` (emparejar naves con su estructura). Ahora corre en servidor y clientes 60 veces por
  segundo: emparejar por índice (las estructuras están en orden de id) con un búfer reutilizado.
- **B · Pendiente:** `lunar_net::Server` copia cada mensaje del juego que llega (`GameIn::data`,
  un `Vec` por datagrama: 60 por segundo y jugador). Un banco de búferes que vuelvan.
- ~~**B · El interés recorre todas las estructuras por jugador y paso (fuerza bruta)**~~ (2026-10-08):
  `interest::Index`, una vez por paso para todos: lo pequeño y lento en celdas (cada jugador mira
  las 27 que lo rodean), lo grande o rápido aparte, y la posición y el alcance de cada estructura
  calculados una vez y no por jugador. Medido (`tests/interest.rs`, 6 000 estructuras, 16
  jugadores, un cuarto a velocidad orbital): de 3,5 a 1,7 ms por paso, sabiendo lo mismo. Queda:
  quien va deprisa sigue mirándolo todo (con muchos así, celdas a su escala o mirar cada 4 pasos).
- **C · Pendiente:** `sync::Digest::of` reserva una lista por máquina; va una vez por paso (la nave
  del turno) en el servidor y una por instantánea en el cliente.
- **Medido:** `Pilot::body` reservaba una lista por jugador y paso: ahora `body_into` sobre la de
  `Game` (cero reservas).

## V41 (2026-10-07): paso fijo, dibujo entre pasos, barrido, `lunar-play`

Fernando: «acuérdate de hiperoptimizar todo» y «aprovechar la mayor cantidad de hilos posible».

- **Paso fijo** (`lunar_play::game::Game::tick`): el trabajo por segundo de simular ya no crece con
  los FPS (antes, a 144 fps la física daba 144 lonchas por segundo; ahora siempre 60). A más de
  60 fps se ahorra física; a menos, se dan hasta 4 pasos por fotograma.
- **Dibujar entre pasos** (`Structures::present` / `restore`): dos pasadas por la lista de
  estructuras por fotograma (copiar posición y giro, interpolar), sin reservas (`shown` se
  reutiliza). El tráfico se calcula dos veces por fotograma (a la hora del dibujo y de vuelta a la
  del paso): 0,3 ms cada una con 2 000 naves (medido en la V29). **C · Pendiente:** dibujar el
  tráfico a su hora sin tocar su estado (una función que escriba las posiciones a una hora dada
  directamente en las instancias), y así una sola pasada.
- **Partículas atrás con su velocidad** (`Particles::ahead`, `upload(.., lag)`): una resta por
  partícula al subirlas, nada más.
- **Barrido entre estructuras** (`physics.rs::sweep`): solo para parejas que se acercan más de
  25 cm en el paso y cuyos caminos se cruzan (rejilla por el camino de cada una,
  `Grid::build_swept`); como mucho 1 024 rayos por pareja y paso, con el árbol de cajas de cada
  estructura. Una nave posada o a la deriva junto a otra no lo paga. **B · Sin medir** en una
  batalla de cientos (`tools/rendimiento/batalla_100.jsonc` con proyectiles grandes y naves
  chocando): medirlo y, si pesa, rayos solo desde las piezas del lado que mira al otro.
- **Contra el suelo** (`ground_ahead`): solo lo que va más de 25 cm por paso y está a menos de lo
  que recorre en un paso sobre el suelo; hasta 32 muestras del relieve y 12 bisecciones.
- **Jugadores sin reservas por paso**: todos los cuerpos van como un solo `Among` (`Crowd`), sin
  lista nueva por paso; `seen_from`, `awake_now` y `people` del `Game` se reutilizan.
- **Hilos**: la física ya busca contactos en todos los núcleos (`rayon`, desde 6 cuerpos) y los
  sistemas de las naves corren en paralelo (desde 8). Lo que viene (el servidor) reparte por
  jugador el interés y las instantáneas (fase 5 del plan).

## V40 (2026-10-07): ordenador de vuelo

- Reparto entre toberas (`flight.rs` → `allocate`): descenso por coordenadas con arranque desde el
  tic anterior, hasta 40 barridos que paran en cuanto nada se mueve; sin reservas por tic (los
  vectores `cols`, `u`, `warm`, `norms` se reutilizan). **0,22 µs por llamada con 16 toberas**
  (`flight::tests::a_small_turn_is_given_whole_and_pushes_nothing`, perfil de pruebas sin
  optimizar del todo, 10 000 llamadas en caliente). Antes eran 40 pasos fijos de gradiente.
- `can_turn` recorre las toberas una vez más por tic para saber cuánto par dan (16 productos
  vectoriales): sin medir aparte, del orden del reparto. Si una nave con cientos de toberas lo
  notara, guardarlo y rehacerlo solo cuando cambie la versión de la estructura o el centro de
  masas.

## V40 (2026-10-07): multijugador y teclas

- **Naves ajenas:** una corrección por nave ajena y fotograma (`follow::steer`: unas cuantas
  sumas y un `slerp`), y la llevada al presente de su última instantánea (`rigid_carried`, sin
  reservar). Daño: cada impacto en una estructura compartida son ≤ 42 bytes por la red y un
  `hit` igual que sin red; las copias de los proyectiles ajenos cuestan lo mismo que los propios
  sin la parte del daño. Los disparos de los jugadores van en trozos de 20 por mensaje.
- **Proyectiles por red:** un lanzamiento son unos 50 bytes y un final unos 40, fiables; un guiado
  en vuelo, 10 avisos por segundo de unos 45. Un golpe lleva la postura de lo articulado de lo
  golpeado (28 bytes por hueso, una vez por mensaje: el Cachalote, ~0,3 kB). Aplicar un golpe con
  otra postura cuesta dos `set_pose` (y sus masas) solo cuando la postura difiere.
- **C · `Named::Piece` se busca recorriendo la lista** de estructuras por su linaje: con cientos de
  trozos y muchos golpes por fotograma, un mapa linaje → estructura.
- **C · `Multi::named` busca la estructura nombrada recorriendo la lista** (y `find` de lo visto
  igual). Con decenas de naves no se nota; con cientos de impactos por fotograma convendría un
  mapa de id de red → estructura que ya se tiene (`self.ships`) para las naves.
- **C · Teclas:** `input::shown` hace una `String` cada vez que la pide el HUD (el gas de la
  mochila, una por herramienta): unas 10 cadenas cortas por fotograma, como el resto de los
  textos del HUD. Si el HUD se pasa a cadenas guardadas, guardarlas también (cambian solo cuando
  el jugador cambia una tecla). Buscar qué hace una tecla es recorrer ~60 pares: nada.
- **C · Pruebas:** `Defs::load` tarda unos 12 s en el perfil de pruebas (las naves con sus
  modelos); las del multijugador lo cargan una vez para todas (`OnceLock`), pero cada binario de
  pruebas que lo usa lo paga. Si se juntan más, cachear en disco lo compilado de las naves.

## V39 (2026-10-06): barrido de proyectiles en el reloj del mundo

- `structure::motion::Sweep`: BVH de volúmenes recorridos, una construcción por loncha con
  proyectiles; vacío, no guarda poses ni construye nada. Usa el BVH existente, sin dependencias.
  Las poses y los tres almacenes del árbol se reutilizan; prueba de punteros/capacidades durante
  100 reconstrucciones (`swept_bounds_keep_their_storage_once_warm`). La cola de impactos
  reserva al crearla la capacidad del pool de proyectiles, no crece al tocar muchas paredes.
- `core/tests/sweep.rs`: **500 estructuras y 2.000 tramos, 1,182 ms/loncha**, 5,708 candidatos
  por tramo frente a las 500 estructuras anteriores (perfil de pruebas optimizado, sin ventana).
  Incluye reconstrucción y dos consultas por tramo para contar candidatos y medir el contacto.
  No es una medida GPU ni de una batalla completa. Falta medir 20.000 tiros en una batalla real.
- La previsión por fotograma despierta solo estructuras candidatas de los tramos recorridos,
  incluso si no están cerca del jugador ni de la cámara. No agrega 20.000 observadores al
  planificador ni introduce un barrido proyectiles × estructuras.
- Efectos móviles: base de velocidad heredada en doble precisión, aparte de la expansión.
  Añade **24 bytes por partícula** (96 en total; 480 kB para 20.000), una suma por paso y nada
  al formato GPU ni al número de draws. `core/tests/effects.rs`: **0,846 ms/paso para 20.000**,
  100 pasos, sin crecimiento ni cambio de dirección del almacén. Luz y sacudida son listas
  acotadas a ocho. Medido en perfil de pruebas, no es un presupuesto de GPU.
- Pendiente: estas mejoras cubren `rounds` (mano y cañones). Guiados, misiles estratégicos y
  partículas conservan sus pasos anteriores; su traslado al reloj común queda abierto.

## Nuevo en V39 (2026-10-06): lo que rige en cada sitio. Medido en pruebas, no en el juego

Lo que corre por loncha o por fotograma y lo que cuesta ([`MOVIMIENTO.md`](MOVIMIENTO.md) §6-9):

- **`BodyRegistry::field`**: una raíz y unas multiplicaciones por cuerpo del sistema, sin
  reservar nada. **104 ns por consulta con tres cuerpos** en el perfil de pruebas
  (`core/tests/field.rs`, `asking_what_holds_somewhere_costs_next_to_nothing`: dos millones de
  consultas repartidas por suelo, franja, espacio y junto a la Luna menor; falla por encima de
  2 µs).
- **Física de estructuras:** una consulta por cuerpo despierto y loncha (antes: gravedad y
  vertical del cuerpo de nacimiento, que eran otras dos raíces). El umbral de empuje para
  despertar ya no se recalcula por contacto: se guarda por cuerpo en la loncha (`shoved`). Sin
  suelo debajo no se busca parche ni esquinas.
- **Jugador:** una consulta a `field` por loncha y, al tocar algo, otra cuenta de peso (sumas).
- **Proyectiles y partículas:** una consulta por proyectil y por partícula con gravedad y paso
  (antes ya era una raíz por cuerpo para saber cuál dominaba, más las de su gravedad).
- **Brújula:** se rellena una vez por fotograma en un almacén que no vuelve a reservar
  (`nav/tests.rs`, `filling_the_compass_allocates_nothing_once_it_has_held_its_most`); su dibujo
  son unas decenas de líneas y textos.
- **Naves:** `World::at` hace una consulta por nave y tic; la gravedad de a bordo es una
  comparación y una suma.
- **Niveles de simulación:** quien vive entre estructuras cuenta como observador (`Among::at`):
  una distancia más por estructura y fotograma, en una lista que se reutiliza. A cambio, lo que
  rodea al jugador va siempre en fino aunque la cámara esté lejos (antes, con la cámara de un
  guion o de un misil a kilómetros, la nave del jugador iba a paso grueso).

Pendiente (nuevo):

- **B · `field` por proyectil.** Con 20 000 proyectiles en vuelo son 20 000 consultas por paso
  (unos 2 ms en el perfil de pruebas; sin medir en release). Si pesa: una consulta por tanda de
  proyectiles cercanos, o saltarla en los que están lejos de todo cuerpo (basta la distancia al
  cuadrado contra el alcance mayor).
- **C · `Structures::rooms_at`** (en qué salas está el jugador) recorre todas las estructuras
  una vez por fotograma. Antes recorría todas las naves; con miles de estructuras, por rejilla.
- **C · Lista de cuerpos lineal.** `field` recorre todos los cuerpos. Con dos es lo óptimo; con
  un sistema de decenas, descartar por regiones.
- **No medido en el juego:** no se ha pasado ningún guion de `tools/rendimiento` (había una
  partida abierta).

## Nuevo en V38 (2026-10-06): combate. Sin medir en el juego todavía

Lo que se hizo para que no cueste, y lo que queda por medir (no se lanzó ningún guion de
rendimiento: había una partida abierta):

- **Hecho:** una nave sin sensores ni armas no tiene sistema táctico (`Ship::tactical` es `None`);
  con ellos apagados no pide contactos (`Tactical::looking`) y `app/tactics.rs` sale sin hacer
  nada. Las naves que miran reciben contactos cada 0,25 s, no cada fotograma; la lista de cosas
  se hace una vez para todas. Trazas en una tabla fija de 32; las pantallas se redibujan cuando
  hay barrido (o 10 veces por segundo con una traza elegida). Misiles y señuelos en reservas
  fijas (256 y 512). Nada reserva memoria una vez en marcha.
- **B · Contactos: todas contra todas.** Cada nave que mira recorre todas las cosas (naves,
  estructuras sueltas, 2 000 de tráfico, misiles, señuelos). Con 100 naves mirando son unas
  210 000 comprobaciones cuatro veces por segundo. Falta medirlo; si pesa, usar la rejilla que
  ya existe (`core/structure/broadphase.rs`) y no meter el tráfico lejano.
- **B · Trazas en pantalla: una caja por línea.** Una página de radar son unas 80 a 150 cajas
  instanciadas (anillos, marcas, vectores). Solo se dibuja la página a la vista y solo de cerca,
  pero un anillo podría ser una sola pieza.
- ~~**C · Proyectiles de nave contra estructuras.**~~ **Hecho 2026-10-06:** los cañones y las
  armas de mano comparten `rounds::Flight` y el BVH de volúmenes barridos. Medida arriba.
- **C · Presupuesto por nave:** el Azor pasa `tests/naves.rs` (piezas y vértices por metro, tic);
  el blindaje de la panza se bajó de 9 420 a 2 524 triángulos.

## Hecho el 2026-10-03 (V29): naves para batallas de cientos

Medido en el juego con `tools/rendimiento/*.jsonc` (RTX 4060, Vulkan, sin sincronía vertical; cada
guion escribe `out/rendimiento/<nombre>.json` con cada fotograma y un resumen en su `.log`):

| Prueba | Antes (V27) | Ahora |
|---|---|---|
| 21 Alcotán quietas (`flota_20`) | — | 241 FPS (4,1 ms) |
| 20 Alcotán cayendo unas sobre otras (`caida_20`) | 2 FPS con 4 naves | 239 FPS; física 1,25 ms de media, 2,5 ms la peor |
| 21 Alcotán bajo 120 balas/s (`batalla_20`) | la malla entera rehecha por impacto | 230 FPS; peor fotograma 8 ms; **0 mallas rehechas** |
| 99 Alcotán, 49 cayendo sobre 50, 300 balas/s (`batalla_100`) | — | 165 FPS de media; p99 9,4 ms |
| 2000 naves de tráfico (`tools/camara/trafico.jsonc`) | — | 365 FPS; el tráfico entero 0,3 ms |

Y por nave (`crates/ship/tests/presupuesto.rs`, que falla si alguien se pasa):

| Una Alcotán | Antes | Ahora | Tope del test |
|---|---|---|---|
| Piezas | 2 478 | 513 (con la carga de la bodega) | 700 |
| Uniones | 9 219 | 1 455 | 3 000 |
| Vértices de la malla completa | 1 128 732 | 49 242 | 90 000 |
| Tic de sistemas | 0,092 ms | 0,081 ms (con cada tramo por dos caminos) | 1 ms (perfil de test) |
| Impacto de bala | 2,56 ms + rehacer la malla (19 ms) | 0,48 ms, nada que rehacer | 2 ms |
| 4 naves una dentro de otra, por paso de física | 170 ms | 1,7 ms | 4 ms |

Qué se hizo:

- ~~**Cables pieza a pieza**~~ `ship/conduits.rs` en lugar de `cableway.rs` y `route.rs`: un tronco
  por pared de cada sala, tomas donde se enchufa algo y bajantes que solo se ven (van en el aspecto
  de su toma). Las tuberías entre máquinas son fijas y solo se ven. 2 069 piezas de conducto → 77.
- ~~**Naves iguales: una malla por nave**~~ y ~~**rehacer la malla al dañar**~~ `render/structures.rs`:
  una malla por plano, compartida por todas sus naves; lo que cambia (pieza que falta, daño, luz) es
  una palabra por pieza en un buffer (`look::part_states`) que lee el shader. Un impacto escribe
  esas palabras y nada más. Una nave partida en dos son dos estructuras con la misma malla. Un
  trozo pequeño (menos de un cuarto de las piezas) lleva malla propia, que sale más barata.
- ~~**Contactos entre estructuras: todas las piezas contra todas**~~ `structure/bvh.rs`: árbol de
  cajas por estructura, usado por contactos, rayos (disparos, miras, escáner) y explosiones. Los
  contactos de cada cuerpo se buscan en todos los núcleos (rayon). Como mucho 256 pares de piezas
  por pareja de cuerpos y paso.
- ~~**Conductos como piezas físicas**~~ Piezas `no_collide` (troncos, tomas, conductos): reciben
  daño pero no entran en contactos. Piezas `ghost` (bajantes, tuberías): solo se ven.
- ~~**Sistemas de naves lejanas**~~ `Ship::run` con `Pace`: todo ritmo (la que pisas, las cercanas,
  las que tienen algo en marcha), un tic cada 0,5 s con el resto asentado de golpe, o cada 4 s las
  lejanas. Todas las naves se simulan a la vez en varios hilos (`app/ships.rs`).
- ~~**Espiral de fotogramas lentos**~~ La física da como mucho 3 pasos por fotograma y los sistemas
  4 tics: un fotograma lento hace el juego un poco más lento, no más trabajo.
- ~~**Piezas sueltas al primer impacto**~~ 23 piezas de cada Alcotán no tocaban nada y salían
  volando con la primera bala (138 cuerpos sueltos con 6 naves). Ahora toda pieza queda unida a la
  más cercana al construir la nave.
- ~~**Suelo bajo cada cuerpo**~~ La física guarda el plano del suelo bajo cada cuerpo mientras no se
  mueva de sitio; los cuerpos pequeños se apoyan en ese plano y los que están altos ni se miran.
- ~~**Redes de las estructuras sin nada que repartir**~~ `networks.rs` sale al momento si ninguna
  pieza produce, consume o guarda (naves y escombros).
- ~~**Sombras de todas las estructuras en todas las cascadas**~~ Cada cascada dibuja solo lo que
  contiene; lo que ocupa menos de un píxel no se dibuja.
- **Herramienta de medida**: `perf.rs` cronometra las partes de cada fotograma (mundo, disparos,
  naves, física, aire, visibilidad, escena, render); los pasos de guion `flota`, `fuego`, `perfil`
  y `fin_perfil` montan una batalla y la graban.

## Hecho el 2026-10-02 (V26)

- ~~**Naves lejos: todo el detalle**~~ Seis niveles por distancia (`DetailLods`), silueta lejana
  propia o automática; el Alcotán pasa de 96 364 triángulos a 6 604 / 4 508 / 1 567 / 548 / 312
  (`tests/lod.rs`). El 93 % eran cables y tubos.
- ~~**Mallas completas de naves lejanas en la GPU**~~ Fuera de memoria a 1,6× la distancia del primer
  paso (con el guion `niveles`: 338 277 → 49 185 vértices con un Alcotán lejos).
- ~~**Naves que no se ven**~~ Oclusión por rayos contra el relieve y el casco en el que estás
  (`visibility.rs`), 0,25 ms por fotograma como mucho; sin mandos ni luces interiores.
- ~~**Tests lentos**~~ Perfil de pruebas `opt-level = 1` (dependencias 3): los de paneles pasan de
  220 s a 12 s.

## Pendiente

### C · Imán encendido y vacío: busca recorriendo todas las estructuras
- **Qué pasa:** un imán de carga encendido que no sujeta nada mira cada 0,4 s qué tiene debajo
  (`Ship::clamps` → `cargo::serve` → `Structures::loose_in`), y `loose_in` pasa por todas las
  estructuras del mundo (una distancia por cada una). Sujetando algo, o apagado, no busca. Ya era
  así antes de que el imán tomase todo lo que tiene debajo (2026-10-04, `docs/CARGA.md`); lo nuevo
  solo mide la altura de los que caen en su zona.
- **Coste:** nada con pocos imanes; con cientos de naves con el imán encendido y miles de
  estructuras, imanes × estructuras × 2,5 por segundo.
- **Cómo:** `loose_in` por la rejilla de `broadphase`, o buscar solo cuando algo cambió (un anclaje
  propio soltó, su articulación se movió, la nave se movió).

### B · Crear una nave cuesta 5 ms
- **Qué pasa:** `Ship::new` compila las expresiones de señales, alarmas y paneles de cada nave
  aunque sean del mismo tipo. Sacar 49 naves en un fotograma son 250 ms (`batalla_100`, su peor
  fotograma).
- **Cómo:** compilar una vez por tipo (`ShipKind`) y copiar el estado inicial.

### C · Niveles 0 y 1 de las naves vistas desde fuera
- **Qué pasa:** hasta 200 m se dibuja la malla completa (49 000 vértices, más de la mitad
  interiores) aunque la nave se vea desde fuera.
- **Cómo:** otro nivel sin canalizaciones ni interiores pequeños a partir de unos 40 m para quien
  mira desde fuera. Medir antes con `flota_20`: hoy el fotograma lo marca la GPU (4 ms).

### A · Física: los grupos grandes que se desprenden no llegan a reposar
- **Qué pasa:** cuando una bomba arranca bloques enteros de la torre, algunos de los grandes (de
  10 a 26 t) siguen moviéndose a los 90 s y se meten en el suelo hasta 1,8 m.
  - Lo destapó la rotura en partículas: antes caían sobre una alfombra de esquirlas que los frenaba.
  - Prueba: `a_bomb_brings_part_of_the_tower_down_and_everything_settles`, en
    `core/tests/breakup.rs`. Con la fractura «particulas» falla («6 of 12 still moving»); con
    «planos», que es como corre ahora, pasa.
- **Por qué cuesta:** cuerpos despiertos que nunca se duermen, con su física y sus contactos frame
  a frame.
- **Dónde mirar:** `structure/physics.rs`.
  - `ground` solo prueba las 8 esquinas más bajas contra el suelo. **2026-10-02 (a medias):** ahora
    son las 8 más bajas *repartidas por la huella* (`low_corners`; con muchas piezas, solo las que
    llegan cerca del fondo); la nave sobre su tren ya no cabecea (prueba
    `alcotan_rests_on_its_gear`: se duerme en 0,6 s). Falta repetir la prueba de la torre con
    fractura «particulas».
  - Faltan contactos arista-arista (ya era un hueco conocido).
  - Un bloque grande sobre suelo irregular cabecea sin parar.

### B · Escena de las naves rehecha cada fotograma
- **Qué pasa:** `Ship::scene` rehace todos los props, glifos y lámparas de cada nave a menos de
  70 m cada fotograma (`app/src/ships.rs`), aunque nada haya cambiado.
- **Cómo:** cachear por panel y rehacer solo los mandos/indicadores cuyo estado cambió; las naves
  lejanas (solo lámparas) no deberían generar props para tirarlos.

### C · Jugador contra estructuras sin rejilla
- **Qué pasa:** `Structures::sphere_contacts` recorre todas las estructuras (esfera) y luego todas
  las piezas de las cercanas, tres esferas × tres iteraciones por paso. Con pocas estructuras
  no se nota; con cientos sí.
- **2026-10-06 (V39):** el jugador ya no da 60 pasos por segundo fijos sino uno por loncha del
  mundo (`Among`): a 144 fps son 144 por segundo, a 30 fps siguen siendo 60. Sin medir en el
  juego; si se nota con muchas naves, esta rejilla es lo primero.
- **2026-10-06 (V39, lo que rige):** al tocar algo se calcula además qué se pesa sobre ello
  (unas sumas por paso); y `rooms_at` recorre las estructuras una vez por fotograma (arriba).
- **Cómo:** usar la rejilla de `broadphase` que ya tiene la física.

### B · Partículas: orden en CPU cada frame
- **Qué pasa:** `render/src/particles.rs` ordena todas las partículas vivas de lejos a cerca cada
  frame (`sort_unstable_by`) y sube el buffer entero.
- **Coste:** con 1000+ partículas se nota en CPU.
- **Cómo:** sin bolas de humo grandes, lo aditivo (chispas, fuego) no necesita orden. Ordenar solo lo
  que tenga alpha normal, o pasar a un orden por cubetas.

### B · Mallas de los cascos sin optimizar para la caché de vértices
- **Qué pasa:** `core/src/hull.rs` junta primitivas y greebles con `Mesh::append`, sin soldar vértices
  ni reordenar índices para la caché de vértices de la GPU.
- **Cómo:** al terminar `Hull::build`, soldar duplicados y reordenar índices (Forsyth o tipo meshopt,
  propio y sin dependencias). Medir los ms de GPU del pase de naves con `--bench`.

### B · Proyectiles contra estructuras: sin rejilla espacial
- ~~Cada proyectil de `shots.jsonc` prueba su tramo contra todas las estructuras.~~
  **Hecho 2026-10-06:** BVH de los volúmenes recorridos (`motion::Sweep`); 500 estructuras,
  5,708 candidatos por tramo y 1,182 ms/loncha para 2.000 tramos (prueba `core/tests/sweep.rs`).
- **Parcial:** los otros sistemas de misiles mantienen sus pruebas anteriores; revisar al
  migrarlos al reloj común. Esta entrada queda abierta para ellos.

### C · Impostores declarados pero nunca horneados
- **Qué pasa:** el shader (`impostor_vs` e `impostor_fs` en `mesh.wgsl`), la familia
  `Family::Impostor`, `set_impostor_layer`, `impostor_atlas` y la casilla «Impostores» de la UI
  existen, pero nadie hornea el atlas ni registra un LOD impostor.
- **Hoy:** de lejos se usa el LOD más pobre (unos 300 triángulos) hasta 1,2 px y luego los glows.
- **Cómo:** hornearlos de verdad (8×8 vistas por modelo al cargar) o borrar el código muerto.

### C · Escombros: tope global de 600 cuerpos sueltos
- **Qué pasa:** `Rules::max_debris = 600`, con física y render de todos ellos.
- **Avance:** con la rotura en partículas ya casi no hay escombros; la bomba de la torre deja 12
  grupos enteros en lugar de 160 trozos.
- **Falta:** que lo suelto duerma antes y desaparezca con la distancia (ver el primer punto).

## Hecho

### ~~A · Proyectiles en vuelo y brillos lejanos~~ (2026-10-02)
- **Proyectiles:** `core/src/rounds.rs`.
  - Un disparo con `speed` vuela: es balístico, cae con la gravedad del cuerpo y choca con el suelo
    o con las estructuras. Sin `speed` sigue siendo un impacto instantáneo.
  - Se guardan en un grupo de capacidad fija (20 000) que no asigna memoria tras crearse.
  - La prueba de suelo es gratis lejos de él (por encima de 25 km sobre el radio medio del cuerpo).
  - Se dibujan como partículas (`look`: estilo y tamaño) en el mismo búfer y la misma llamada de
    dibujo que las partículas; los misiles igual.
  - En una batalla, miles de proyectiles son un bucle sobre un array plano y una sola llamada de
    dibujo.
- **Partículas:** ya funcionaban así (capacidad fija, un búfer persistente, una llamada de dibujo).
  Lo que les queda es el orden en CPU (ver Pendiente).
- **Destellos de motores y balizas de las naves:**
  - Antes tenían un brillo mínimo fijo del 35 %: 200 naves vistas de lejos se sumaban en una mancha.
  - Ahora conservan la energía (área real entre área dibujada) y se apagan con el cuadrado de la
    distancia.
- **Brillo central de la explosión** (estilo `brillo`, `glow: true`):
  - Es un degradado que llega a cero en el borde, solo suma luz y lo hace brillar el bloom.
  - Se dibuja a su tamaño real (0,7 radios de cráter). Por debajo de 1 px reparte su luz en vez de
    parpadear.
  - Se ve hasta `glow_reach` radios suyos (1800): la bomba se apaga entre 6 y 10 km, y una carga
    mayor se ve más lejos en proporción.
- **Además:**
  - El cráter vuelve a excavarse en el instante de la explosión, bajo el brillo y el polvo.
  - Hay 3,5 veces menos chispas.

### ~~B · Explosiones: polvo grande con overdraw, sin bench que lo mida~~ (2026-10-02)
- Ya se mide con `--bench N --explode bomba@40`.
- La sombra filtrada por píxel de cada capa de polvo era lo caro: ahora va por vértice.
- **Con la bomba:** de 3,64 a 3,16 ms por frame (sin bomba, 2,73 ms).
- **Si vuelve a hacer falta:** un escalón de calidad para el polvo grande en el regulador.

### ~~A · Rotura de bloques: corte dinámico en pedacitos~~ (2026-10-02)
- **Antes:** cada pieza rota se cortaba con planos en hasta 14 trozos convexos, cada uno un sólido
  rígido con su malla nueva. Una bomba en la torre dejaba **160** cuerpos sueltos.
- **Ahora:**
  - El modelo de fractura por defecto es «particulas» (`ParticleFracture` en `fracture.rs`): la
    pieza desaparece en el efecto `breaks` de su material, escalado por su tamaño
    (`Effects::explode_scaled`).
  - No hay esquirlas sueltas y no se corta nada.
  - La misma bomba deja **12** cuerpos: solo los grupos enteros que se desprenden.
  - «planos» sigue existiendo por si un material lo pide (`"fracture": "planos"`).
- **Queda:** los grupos grandes que se desprenden no reposan (ver arriba, en Pendiente).

### ~~A · Explosiones: las esferas~~ (2026-10-02)
- **Antes:**
  - La bomba lanzaba 170 bolas de fuego (`fogonazo`) y 90 de nube, de varios metros.
  - Lanzaba también 260 `rocas`: partículas duras que el shader pinta como bolas con relieve y que
    rebotan y ruedan por el suelo. Eran las esferas 3D que se veían chocar con el suelo.
  - La granada y el impacto grande tenían lo mismo, a menor escala.
  - Cada explosión era una lista de emisores escrita a mano, con el cráter apareciendo de golpe.
- **Ahora:**
  - Una explosión convencional es solo su carga, `"charge": { "tnt": kg, "casing": kg }`, en la
    explosión, el disparo o el misil.
  - `core/src/detonation.rs` saca de ella el cráter, el destello, la sacudida, el daño y las capas
    de partículas, con leyes de escala cuyas constantes están en `assets/defs/detonacion.jsonc`:
    - la onda, con la raíz cúbica de la carga;
    - el cráter, con la gravedad del cuerpo;
    - la eyecta, a múltiplos de √(g·R).
  - Capas: núcleo de polvo que tapa el punto, falda de polvo a ras de suelo (`on_ground: "skim"`)
    que llega a 2 o 3 radios, cortina de eyecta y chispas.
  - El cráter se excava en el instante de la explosión, bajo el brillo y el polvo: con retraso se
    veía aparecer después, y no gustó.
  - Nada de trozos duros ni estirados: se ven como bolas (las estelas de eyecta y la metralla se
    probaron y se quitaron).
- **Coste:**
  - La bomba lanza unas 800 partículas, frente a 1360 al principio.
  - La sombra de las partículas se calcula en las 4 esquinas de cada una y se interpola, en vez de
    8 a 12 muestras filtradas por píxel y por capa.
  - Medido con `--ships 100 --npcs 100 --bench 8 --explode bomba@40`, la mejor de 3 pasadas:
    2,73 ms sin bomba, 3,64 ms con la bomba en V15 y 3,16 ms ahora.

### ~~A · Naves: el detalle fino a base de triángulos (greebles)~~ (2026-10-02)
- **Antes:** el LOD0 de cada nave se llenaba con cajitas y tubos hasta 20 000 triángulos, el LOD1
  hasta 3000, y las tres naves eran cajas planas de un color.
- **Ahora:**
  - El detalle fino lo pone el shader, sin triángulos (`mesh.wgsl`, `plating`): planchas con juntas,
    tono y rugosidad por plancha, escotillas oscuras, suciedad que chorrea, y ventanas encendidas y
    apagadas.
  - Cada material de la paleta declara su plancha (`panel`, en metros) y, si quiere, ventanas
    (`windows`). Viaja en el byte libre del vértice (`params.w`): el vértice sigue ocupando 24 bytes.
  - Las juntas se apagan solas con la distancia (`fwidth`): ni moiré ni trabajo donde no se ven.
  - Las tres naves están rediseñadas: una lanzadera de dos góndolas, un carguero de contenedores y
    un transporte pesado con puente, alas solares y cuatro motores.
  - Las piezas pequeñas no llegan al LOD lejano (`lod_max: 1`).
- **Triángulos por LOD** (`cargo test -p lunar-core hull -- --nocapture`):

  | Nave | LOD0 | LOD1 | LOD2 |
  |---|---|---|---|
  | Pequeña | 9040 | 1808 | 192 |
  | Mediana | 9016 | 1900 | 204 |
  | Grande | 10 024 | 2280 | 500 |

- **Medido** con `--ships 300 --npcs 100 --bench 20` en una RTX 4060, frame medio: 2,34 ms antes
  (V13) y 2,27 ms ahora. Mismos draws (43,7) y el pase principal de GPU baja de 1,50 a 1,47 ms.
  Más bonitas por el mismo coste.
- **Revisarlas:** `lunar-app --look TIPO@D,AZ,EL --shot f.png` (TIPO 0, 1 o 2) hace una captura
  fija de cada nave.

## V36 (2026-10-05): masa, vuelo, manos, huellas

- **No medido en el juego:** siguen sin pasarse las pruebas de `tools/rendimiento` (a petición de
  Fernando). Lo que sigue son cifras de las pruebas de `cargo`.
- **Masa de los depósitos** (`masa.rs`): una nave sin nada fluyendo no se pesa nunca; con los
  motores quemando se vuelve a pesar solo el bloque del depósito que cruza un cuanto: 119 ns
  cada vez (pesar el Cachalote entero son 478 µs: 4 000 veces más). Tic del Alcotán en reposo
  84,5 µs y quemando 85,5 µs; Cachalote 113 y 116 µs.
- **Ordenador de vuelo:** nivelar en MANTENER es un producto vectorial más por tic; nada nuevo
  que guardar.
- **Procedimientos:** nada corre en el juego mientras nadie lo pida. La prueba hace 137
  procedimientos (1 744 s de nave) en 6,3 s de reloj.
- **Manos:** un cuerpo cuyas manos no hacen nada cuesta una comparación por fotograma
  (`handwork.rs`).
- **Huellas:** un búfer de instancias y un dibujo; el búfer se escribe solo cuando cambia y solo
  con las marcas cercanas al ojo (lo guardan sus pruebas: `footprints.rs`).
- **Prueba al azar** (`logica_fuzz.rs`): el tic se mantiene dentro del presupuesto de
  `presupuesto.rs` con todos los mandos accionados al azar.

## V35 (2026-10-04): arranque en segundo plano

- **Arranque:** antes, 13,4 s con la ventana congelada (definiciones 4,3 s, motor gráfico 1,6,
  modelos 2,0, mundo y tráfico 4,1, atlas y equipo 1,4). Ahora el juego se monta en un hilo
  propio y tarda 8,2 s (definiciones 3,6, motor gráfico 0,24, modelos 0,87, mundo 2,5, equipo
  1,0) mientras el hilo principal enseña la pantalla de carga con un dispositivo gráfico aparte
  (el de menos consumo): la ventana responde siempre. Los tiempos de cada etapa quedan en
  `out/arranque.json`.
- **Guiones y pruebas sin ventana:** `--guion`, `--bench` y `--shot` corren con la ventana
  oculta; no quitan el foco al juego que haya abierto (sí le quitan GPU mientras duran).
- **No medido en esta versión:** no se han pasado las pruebas de `tools/rendimiento` (Fernando
  pidió que no se lancen mientras juega). Lo nuevo que corre por fotograma: la vista libre
  (nada), los otros jugadores (un cuerpo por jugador: dos piernas y dos brazos por ley de
  cosenos, y un paquete de red cada 50 ms) y lo que digan `AIRE.md`, `CARGA.md` y `CHORROS.md`
  de lo suyo. Pendiente medir cuando él lo pida.
- **Aire (válvulas de mano, compresor, depósito; `AIRE.md`):** una válvula cerrada no crea
  abertura ni cálculo de flujo; el compresor parado sale de `plan` sin pedir ni escribir nada; el
  depósito solo escribe sus señales cuando cambia lo que tiene. Lo que se añade por tic es lo de
  cualquier panel (19 indicadores y 6 mandos más en el Alcotán), y a cambio **las agujas en reposo
  y los displays cuya lectura no cambia ya no se recalculan** en ninguna nave
  (`controls/indicator.rs`). Tic de una Alcotán en reposo con y sin todo ello: dentro del ruido de
  la medida (≈0,15 ms con otras compilaciones en marcha; `presupuesto.rs` sigue pasando). Una
  nave en reposo sigue a ritmo lento (`trasvase.rs`: `an_idle_ship_does_none_of_this_work`).

- **Balance de energía (`ship/power.rs`):** una suma sobre los puertos eléctricos de la nave por
  tic (su lista se hace una vez, al montarla; los convertidores entre barras, otra lista), seis
  señales escritas y ninguna asignación. Nada se resuelve de nuevo. Las secciones de panel se
  despliegan al cargar las definiciones (texto a texto), no al montar cada nave.
- **Reactor:** la retención por periodo y el recorte por temperatura son dos comparaciones en su
  paso; no hay estado nuevo que guardar ni que mandar por la red (salen de lo que ya tenía).

## V34 (2026-10-04): vista exterior, bajadas, asiento nuevo, rejilla, menú de inicio

Una sola pasada de `tools/rendimiento/*.jsonc`, con el juego de Fernando abierto y con sincronía
vertical (la pantalla va a 180 Hz, así que los FPS no dicen nada: todas dan 180 menos
`batalla_100`, 148). Lo que vale son los milisegundos de CPU por etapa, comparables con V33:

| Prueba | naves (V33) | física (V33) |
|---|---|---|
| `flota_20` | 0,29 ms (0,29) | 0,01 ms (0,01) |
| `batalla_20` | 0,29 ms (0,28) | 0,25 ms (0,22) |
| `caida_20` | 0,43 ms (0,43) | 0,69 ms (0,58) |
| `batalla_100` | 1,32 ms (1,49) | 0,74 ms (0,83) |
| `flota_mixta` | 0,33 ms (0,32) | 0,02 ms (0,02) |

Nada de lo nuevo corre por nave y por fotograma: la cámara exterior es un rayo contra las
estructuras (dos con la mira), las bajadas se miran solo al levantarse de un asiento, y la
rejilla del Cachalote son 101 calcas y 60 rótulos que solo existen en su malla de cerca. El
asiento nuevo tiene 12 400 triángulos (el anterior 6 800): unos 33 000 más por Alcotán de cerca,
dentro del presupuesto que comprueban las pruebas (`every_ship_keeps_within_its_budget`).

Una segunda pasada sin sincronía vertical quedó estropeada (el otro juego abierto se llevaba la
GPU: de 95 a 220 FPS según el momento) y no se usa. **No se repite por iniciativa propia:**
Fernando pidió que no se le abran ventanas de pruebas de rendimiento mientras juega. Pendiente,
cuando él lo pida: medir FPS sin otro juego abierto.

## V33 (2026-10-04): tren con muelles, mecanismos que se topan, cuerpo con esqueleto

Medido con `tools/rendimiento/*.jsonc` (RTX 4060, Vulkan, sin sincronía vertical) **con otro juego
abierto a la vez** (el de Fernando: no se cierra): los FPS de esta tabla valen para comparar entre
sí las columnas de hoy, no con las de V32; los milisegundos de CPU sí son comparables.

| Prueba | V33 |
|---|---|
| 21 Alcotán quietas (`flota_20`) | 223 FPS; naves 0,29 ms; física 0,01 ms |
| 21 Alcotán bajo 120 balas/s (`batalla_20`) | 220 FPS; naves 0,28 ms; física 0,22 ms; 0 mallas rehechas |
| 20 Alcotán cayendo (`caida_20`) | 229 FPS; naves 0,43 ms; física 0,58 ms (V32: 0,91) |
| 99 Alcotán, 49 cayendo, 300 balas/s (`batalla_100`) | 133 FPS; p99 13,6 ms (V32: 14,1); naves 1,49 ms; física 0,83 ms |
| 12 Abejorro + 13 Alcotán + 6 Cachalote (`flota_mixta`) | 257 FPS; naves 0,32 ms; física 0,02 ms |

Qué cuesta lo nuevo y qué se hizo para que no cueste:

- **Patas con muelle:** una pata es un contacto más (blando) en el resolvedor que ya había. Mover
  un muelle no rehace la masa de la nave ni su caché de esquinas bajas (`Structure::stance` solo
  cambia si se mueve un hueso que no es de pata). La caída de 20 naves baja de 0,91 a 0,58 ms.
- **Mecanismos que se topan (`stop_at_obstacles`):** la primera versión costaba 1,4 ms en
  `batalla_100` (naves 0,92 → 2,32 ms). Lo que lo arregló, por orden de efecto: (1) las patas con
  muelle contaban como "articulación movida" cada fotograma y ninguna nave salía por la vía
  rápida; (2) el suelo se miraba esquina a esquina y en cada tanteo de la bisección: ahora es un
  plano por comprobación (tres muestras); (3) los topes se borraban en cuanto el casco temblaba 1
  mm y la rampa volvía a caer sobre lo que tenía debajo cada fotograma: ahora aguantan 2 cm y medio
  grado; (4) una nave que cae o da tumbos no se mira; (5) lo que cede al suelo (rampas) se mira
  20 veces por segundo, no cada fotograma. Queda en 0,57 ms sobre 99 naves en el peor caso (49
  estrelladas unas sobre otras) y en nada con las naves posadas.
- **El cuerpo del jugador:** una malla de 96 000 triángulos con piel (cuatro huesos por vértice,
  52 matrices), dibujada una vez en la vista y una por cascada de sombra. La pose es CPU: dos
  piernas y dos brazos por ley de cosenos y 30 falanges: microsegundos. La marcha pide el suelo
  dos o tres veces por fotograma mientras un pie está en el aire (el camino del pie se mira al
  levantarlo, ocho muestras, y no más salvo que cambie el destino).

Pendiente de rendimiento (nuevo):

- **Sombra del cuerpo:** se dibuja el modelo entero en cada cascada. Una versión de pocos
  triángulos para la sombra ahorraría casi todo.
- **`batalla_100`, naves 1,49 ms:** 0,9 son de antes (sistemas de 99 naves a todo ritmo); los 0,57
  de los topes son naves despiertas con la rampa apoyada en otra nave. Se podría no mirar las
  articulaciones que nadie acciona (una rampa que solo cuelga).
- **Medir sin otro juego abierto** para tener FPS comparables con V32.

## V32 (2026-10-04): modelos de cerca sin tocar lo de lejos

Los modelos solo existen dentro de 3,8 radios de cada nave (40 m como poco): más allá se dibuja el
aspecto de antes, así que una batalla vista de lejos cuesta lo mismo.

| Nave | Vértices de lejos (básico) | Vértices de cerca V31 | Vértices de cerca V32 |
|---|---|---|---|
| Abejorro (7 m) | 5 436 | 157 464 | 269 544 |
| Alcotán (24 m) | 49 650 | 632 178 | 771 054 |
| Cachalote (45 m) | 60 312 | 803 406 | 1 046 628 |

(El tope por prueba es 45 000 vértices por metro de cerca y 4 500 de lejos: `tests/naves.rs`.)

Medido con `tools/rendimiento/*.jsonc` (RTX 4060, Vulkan, sin sincronía vertical, sin otro juego
abierto), una pasada de cada una:

| Prueba | V31 | V32 |
|---|---|---|
| 21 Alcotán quietas (`flota_20`) | 224 FPS; física 0,01 ms | 188 FPS (GPU 5,1 ms; 3,7 M de triángulos); física 0,01 ms |
| 21 Alcotán bajo 120 balas/s (`batalla_20`) | 182-205 FPS; física 0,23 ms | 199 FPS; física 0,23 ms; 0 mallas rehechas |
| 20 Alcotán cayendo (`caida_20`) | 220 FPS; física 0,60 ms | 216 FPS; física 0,91 ms |
| 99 Alcotán, 49 cayendo, 300 balas/s (`batalla_100`) | 177 FPS; p99 9,0 ms | 149 FPS; p99 14,1 ms; física 0,73 ms |
| 12 Abejorro + 13 Alcotán + 6 Cachalote (`flota_mixta`) | 213-220 FPS | 267 FPS; física 0,04 ms |

Cómo leerlo: los FPS los marca la GPU y de una pasada a otra bailan un 10-15 %, así que solo es
firme lo grueso: con la cámara entre naves (`flota_20`, `batalla_100`) las que caen dentro de los
40 m se dibujan con un 20-70 % más de vértices y se nota (un 15 % menos); desde fuera
(`flota_mixta`) no cuesta nada. No hay regresión de CPU: los rótulos son un vistazo por pieza de
las naves cercanas (`labels::show`) y el pase de *props* sigue siendo un dibujo por malla.

Pendiente de rendimiento (nuevo):

- **Sombras de cerca con el modelo fino.** Las cascadas dibujan el nivel 0 entero. Dibujar las
  sombras con el aspecto básico ahorraría la mitad de esos vértices.
- **Tornillos y remaches como geometría.** Un tercio de los triángulos de un ala o de una chapa
  de cubierta son cabezas de tornillo. Llevarlos a un detalle del sombreador (como los remaches
  de la chapa) los quitaría de la malla.
- **Rótulos:** se maquetan cada fotograma los que están a la vista (unas decenas). Si crecen,
  guardarlos maquetados por tipo de pieza.

## V31 (2026-10-03): una nave posada no cuesta nada, y medido con la máquina libre

Medido con `tools/rendimiento/*.jsonc` (RTX 4060, Vulkan, sin sincronía vertical, sin otro juego
abierto). Los guiones dicen ahora qué naves del escenario quieren (`"naves": ["alcotan"]`), para
que las naves nuevas del escenario no cambien la medida, y el resumen dice cuántos cuerpos
estaban despiertos y cuáles seguían moviéndose al acabar.

| Prueba | V29 | V31 antes de tocar la física | V31 |
|---|---|---|---|
| 21 Alcotán quietas (`flota_20`) | 241 FPS | 227 FPS; **física 0,66 ms** con 2 naves que no se dormían | 224 FPS; **física 0,01 ms**, 0 despiertas |
| 21 Alcotán bajo 120 balas/s (`batalla_20`) | 230 FPS | 226 FPS; física 0,74 ms | 182-205 FPS; física 0,23 ms |
| 20 Alcotán cayendo unas sobre otras (`caida_20`) | 239 FPS; física 1,25 ms | 223 FPS; física 1,40 ms | 220 FPS; **física 0,60 ms** (13 despiertas de media) |
| 99 Alcotán, 49 cayendo sobre 50, 300 balas/s (`batalla_100`) | 165 FPS; p99 9,4 ms | 164 FPS; física 1,23 ms | **177 FPS**; p99 9,0 ms; física 0,66 ms |
| 12 Abejorro + 13 Alcotán + 6 Cachalote con su Abejorro (`flota_mixta`, nueva) | — | 254 FPS; física 1,33 ms | 213-220 FPS; física 0,04 ms |

Cómo leerlo: **los FPS los marca la GPU** (4,0-5,2 ms de GPU por fotograma; la misma prueba
repetida da de 191 a 224 FPS según la pasada), así que entre versiones solo es comparable lo de
CPU, que es lo que ha bajado. El pico de 100-290 ms de `caida_20` y `batalla_100` es crear 20 o 49
naves de golpe dentro de la medida (5 ms por nave: sigue pendiente, ver "Crear una nave cuesta
5 ms").

Qué se hizo (`core/structure/physics.rs`, `ship/flight.rs`):

- **Naves que no se dormían.** Dos de cada veinte quedaban despiertas para siempre, andando por
  el terreno: el estabilizador (toberas, y en las naves nuevas giróscopos) peleaba contra el
  suelo. Ahora el ordenador de vuelo sabe que tiene el **peso en el tren** (`Structure::grounded`,
  `vuelo.tierra`) y suelta el casco, con un segundo de histéresis; una nave recién creada cuenta
  como posada. Y lo que los sistemas de una nave la empujan solo la despierta si puede moverla
  (más del 20 % de su peso: `PUSH_FROM`), así que un motor al ralentí o un giróscopo trimando no
  la tienen en vela.
- **Apoyos por profundidad real.** Los contactos con el terreno se elegían entre "las esquinas
  más bajas", y en una pendiente la pata de arriba de un casco largo no es de las más bajas: se
  hundía, salía de golpe y la nave botaba. Ahora se guardan por cuerpo las 32 esquinas candidatas
  (`Low`), cada una se mide contra el terreno bajo ella y se quedan las ocho más hondas repartidas
  por la planta. La salida del terreno se limita a 6 cm por paso (`MAX_PUSH`).
- **Lo que costaba un cuerpo sobre el suelo.** Buscar esas esquinas era recorrer las piezas de la
  nave en cada paso (0,4-0,8 ms por nave despierta). Ahora se guardan en el marco de la nave y se
  vuelven a buscar solo si gira más de 2,5°, mueve una pieza o la pierde; y el terreno bajo cada
  esquina solo se vuelve a preguntar cuando la esquina se ha movido 3 cm. **Un paso de nave
  despierta sobre el suelo: de 0,4-0,8 ms a 0,03-0,06 ms** (`ship/tests/naves.rs` lo imprime).
- **El plano del suelo a la escala del cuerpo** (medio tamaño a cada lado, no medio metro): una
  nave no se inclina con la piedra que tiene debajo del centro.
- **Test genérico:** `every_ship_set_down_goes_to_sleep` posa cada nave de la biblioteca en una
  docena de sitios (pendientes de hasta 17°) y falla si no se duerme donde se posó, si tarda más
  de 15 s o si sus sistemas la empujan mientras se asienta.

Lo nuevo de V31 y lo que cuesta:

- **Naves nuevas** (`ship/tests/naves.rs`, topes por metro de eslora): Abejorro 7 m, 73 piezas,
  5,3 mil vértices, tic 0,005 ms; Cachalote 45 m, ~800 piezas, ~59 mil vértices, tic 0,12 ms;
  Alcotán como en V30.
- **Sonido:** el mezclador corre en el hilo de la tarjeta; lleno (28 voces a la vez) gasta el
  **2,2 % de un núcleo** (`audio/tests/mezcla.rs` falla por encima del 5 %; era 9,5 % antes de
  quitar las llamadas a `sin` y `floor` de la biblioteca). El juego solo empuja órdenes a una cola
  sin esperas, y solo cuando un nivel cambia.
- **Polvo:** tope de 600 granos por segundo entre todos los chorros a la vista (`dust.rs`:
  `BUDGET`), solo naves a menos de 700 m, y nada si la nave no empuja.
- **Sujetar** (`hold.rs`): lo sujeto no es un cuerpo; seguir a su dueño es una transformación.
- **Reparto de empuje** (`flight.rs`: `trim`): una matriz de 3×3 por tic y nave con motores.

## V30 (2026-10-03): lo que tocó el rendimiento

- **Carga suelta:** un cuerpo que toca a otro al que empujan sus sistemas (una nave despegando) se
  despierta y no se duerme sobre él (`physics.rs`: `riding`); dormido quedaría quieto en el mundo
  y retendría la nave. Coste: un bit por cuerpo y paso.
- **Anclajes:** cada tic mira unos pocos amarres por anclaje (los índices se buscan una vez).
- **Presupuesto del Alcotán tras V30:** 527 piezas (era 513: cuatro anclajes y el brazo de la
  consola de techo), 1 483 uniones, 49 650 vértices, tic 0,080 ms. Los techos de
  `tests/presupuesto.rs` no se han movido.
- ~~**Sin medir en el juego**~~ (medido en V31 con la máquina libre: ver arriba).
