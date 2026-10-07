# Guía para agentes (y humanos)

Juego FPV cooperativo lunar: Three.js + Vite + TypeScript (cliente), Node + `ws` (servidor), Rapier (física).
Este archivo es el punto de entrada para un agente de IA que vaya a desarrollar el proyecto.

## Mapa del código

| Carpeta | Qué hay |
|---|---|
| `src/engine/` | Núcleo reutilizable: `loop.ts` (paso fijo 60 Hz), `systems.ts` (planificador de sistemas), `debris.ts` (cuerpos rígidos), `debug.ts` (F3/F4/F5) |
| `src/client/game.ts` | Orquestador: crea el mundo, registra sistemas (`registerSystems`), red, HUD, cámara |
| `src/client/player/` | Astronauta (rig, animación procedural, IK de brazos/agarres), controlador Rapier, cámara |
| `src/client/fx/` | Cómo se ven y se sujetan las armas (`weapons.ts`: aspecto + agarres por id del catálogo), proyectiles en vuelo (`projectiles.ts` + `projectileLooks.ts`), partículas |
| `src/client/cargo/` | Objetos sueltos (`crates.ts`: cuerpos Rapier en su marco, dueño que los simula, red), su aspecto por tipo (`looks.ts`), llevarlos en las manos (`carry.ts`) |
| `src/client/ship/` | Nave en el cliente: `view.ts` (casco, estructura, consolas, luces), `physics.ts` (colisiones), `interaction.ts` (mirar + clic, soldar, asientos), `cargo.ts` (cajas dinámicas), `screens.ts` (MFD), `interiorLights.ts` |
| `src/client/world/` | Terreno único de cada cuerpo (`sphereTerrain.ts`, CDLOD en workers), colisión en streaming, rocas, iluminación |
| `src/client/render/pip.ts`, `cameraSources.ts`, `imageEffects.ts` | Capturas PiP, proveedores y efectos reutilizables en cualquier contexto. Adaptador de naves: `client/ship/cameraScreens.ts`; guía: [`docs/CAMERAS.md`](docs/CAMERAS.md) |
| `src/client/audio/` | Sonido sintetizado y posicional: motor (`engine.ts`, `sfx`), propagación física aire/estructura/suelo/traje (`medium.ts`), anfitriones acústicos genéricos y entorno del cuerpo (`acoustics.ts`), cues declarados (`cues.ts`, contrato en `shared/sound.ts`), banco y recetas (`bank.ts`, `sounds/`), adaptadores: nave (`shipSounds.ts`), tripulación. Guía: [`docs/AUDIO.md`](docs/AUDIO.md) |
| `src/shared/` | Código común cliente/servidor: protocolo de red, constantes, ruido |
| `src/shared/frames/` | Marcos de referencia compartidos: el mundo y los anfitriones (naves…), paso de uno a otro sin saltos (`carry`, `hostAt`), cuerpo balístico que vuela por ellos (`ballistic.ts`), historia dibujada que cruza marcos (`track.ts`), a qué marco pertenece un objeto suelto (`membership.ts`). Guías: [`docs/EQUIPO.md`](docs/EQUIPO.md), [`docs/MOVIMIENTO.md`](docs/MOVIMIENTO.md) |
| `src/shared/time/`, `src/shared/net/` | Reloj de pasos (`stepClock.ts`: el tiempo al que pertenece cada estado) y réplicas de lo que simula otra máquina (`replica.ts`: en el presente en el mundo, con retraso dentro de una nave). Guía: [`docs/MOVIMIENTO.md`](docs/MOVIMIENTO.md) |
| `src/client/diag/` | Sonda de movimiento (F7, `motionProbe.ts`): salto por fotograma de todo lo que se ve y qué lo causó |
| `src/shared/items/` | Catálogos de equipo: proyectiles (cohete, bala, minimisil), armas y herramientas (lanzacohetes, soldadora, fusil), montajes de armas (`mounts.ts`: torretas en cualquier anfitrión), objetos sueltos (caja, pieza de repuesto). Guía: [`docs/EQUIPO.md`](docs/EQUIPO.md) |
| `src/shared/ship/` | Naves como datos: `ships/` (la Selene, la Peregrina y el Albatros, de dos cubiertas), `catalog/` (componentes reutilizables), `def.ts` (tipos, `finishShip`), `sim.ts` (mandos, daño), `flight.ts` (vuelo offline), `systems.ts` (núcleo), `airflow.ts` (aire en movimiento: tirón, chorros, paneles bajo presión) y `modules/`. Guía: [`docs/SHIPS.md`](docs/SHIPS.md) |
| `src/shared/space/` | Cuerpos celestes como datos: gravedad radial, marco local, órbitas, eclipse; su suelo (`surface.ts`), los modificadores de terreno (`terrainMods/`), los sitios (`sites.ts`), las rocas (`rocks.ts`) y los marcos tangentes (`tangent.ts`). Guía: [`docs/ESPACIO.md`](docs/ESPACIO.md) |
| `src/server/` | Servidor autoritativo (salas, daño, explosiones, ediciones de terreno); arranca el hilo del mundo y lo guarda al salir |
| `src/sim/` | **Núcleo del mundo** sin gráficos ni plataforma: `core/` (entidades y tablas SoA, línea de tiempo, registro de causas, azar determinista, calendario, formas cerradas, `World`), `persist/` (partidas binarias, segmentos del registro, dos ranuras), `host/` (el hilo con presupuesto: `runner.ts` genérico, adaptador de Node en `host/node/`), `kit/` (lugares, niveles de detalle con conservación), `tools/` (sismógrafo), `modules/` (jugadores, objetos y almacenes, personas y dotaciones, lo que saben, «qué hay aquí»). Guía: [`docs/MUNDO.md`](docs/MUNDO.md) |
| `src/server/objects.ts`, `npcs.ts`, `world*.ts`, `peer.ts` | Objetos sueltos y NPC del servidor con réplica por interés; el puente con el mundo (enlaces de objetos y de personas). Guías: [`docs/RED.md`](docs/RED.md), [`docs/MUNDO.md`](docs/MUNDO.md) §10-12 |
| `src/shared/net/interest.ts` | Quién sabe qué: interés con histéresis, anfitriones y fijados, rejilla. Guía: [`docs/RED.md`](docs/RED.md) |
| `src/shared/actors/` | Gente sin motor físico: andador sobre cualquier suelo (`walker.ts`, `ground.ts`), percepción por vista y oído según el medio (`perception.ts`) |
| `src/shared/space/galaxy.ts`, `jump.ts` | La galaxia (2000 sistemas), las regiones de los sistemas con cuerpo y el salto entre ellas. Guía: [`docs/MUNDO.md`](docs/MUNDO.md) §14 |
| `tools/blender/` | Traje del astronauta generado por código (Blender headless → `public/assets/astronaut.glb`) |
| `tools/diag/` | Herramientas de autodiagnóstico visual y numérico |
| `tools/sim/` | Pruebas, rendimiento e historiador del núcleo del mundo (`check.ts`, `kit.ts`, `tools.ts`, `server.ts`, `bench.ts`, `why.ts`), barrido de semillas (`seeds.ts`) y sismógrafo (`scope.ts`); `scenario.ts`: un mundo de juguete que lo ejercita todo |
| `tools/net/`, `tools/actors/`, `tools/space/` | Servidor real y clientes sin navegador (`harness.ts`): réplica, objetos del mundo, NPC y testigos; andador y percepción; galaxia y salto |

## Reglas del motor

- **Nada de lógica en el bucle principal.** Una funcionalidad nueva es un sistema:
  `this.systems.add({ name, phase: 'fixed' | 'frame', order, update })`.
  Orden: 10 física · 20 personajes · 30 objetos del mundo · 40 jugabilidad · 90 efectos.
  F3 muestra los ms de cada sistema.
- La simulación (física, movimiento, proyectiles) va en `phase: 'fixed'` (determinista, 60 Hz).
  El render interpola entre pasos (`ctl.renderPosition`).
- **Datos antes que código:** un arma nueva es un `defineWeapon` en `shared/items/weapons.ts` (qué
  hace, cadencia, retroceso; la tecla sale sola por orden) y su aspecto en `client/fx/weapons.ts`
  con sus agarres (`Grip`: centro, eje, radio, lado de la mano). Las manos se colocan solas por IK;
  no hay animaciones hechas a mano. Un proyectil es un `defineProjectile`, un objeto suelto un
  `defineObject`, un arma montada (torreta, cañón fijo) un `defineMountKind` + su colocación en el
  anfitrión, sin código propio de nave ni de sitio (`docs/EQUIPO.md` §3.1).
- **Todo lo que se mueve vive en un marco** (`shared/frames`): dentro de una nave, en su espacio; al
  salir, en el mundo, con la misma posición y velocidad reales. La regla de «dentro» es una sola
  (`FrameHost.inside`); lo que vuela libre usa `stepBallistic`, igual en cliente y servidor. En la
  red viaja con su marco (`fr` + coordenadas locales).
- **Movimiento suave a cualquier velocidad** ([`docs/MOVIMIENTO.md`](docs/MOVIMIENTO.md)): lo que se
  dibuja entre pasos guarda un `PoseTrack` y al cambiar de marco lleva **sus dos pasos**
  (`Frames.carryTrack`); los cambios de marco y la recolocación de la burbuja ocurren **al final del
  movimiento del paso** (sistemas `physics` → `player` → `frames`), nunca con los marcos ya movidos y
  los cuerpos sin mover. Todo lo que va por red lleva `t` = **el tiempo de su paso** (`StepClock`), no
  la hora de envío ni la de llegada; lo que simula otro se dibuja con un `Replica` (en el mundo, en el
  presente). Nada frena ni recorta contra el marco en que se simula (sin amortiguación lineal, control
  en el aire solo en la dirección pedida, cámaras suavizadas en el marco del astronauta).
- **El mundo es un núcleo aparte** ([`docs/MUNDO.md`](docs/MUNDO.md)): `src/sim` no importa nada del
  motor 3D, del cliente ni del servidor, y corre en su propio hilo. Sus sistemas son `SimModule`
  (componentes + eventos + creación) en `src/sim/modules`; todo su estado vive en tablas, la cola y el
  registro (se guarda solo). Determinista: solo `w.now`, `w.random(id)` y las tablas; nunca `Date`,
  `Math.random` ni estado fuera del mundo. Lo que se puede calcular se programa (`schedule`/`move`),
  no se comprueba cada tick.
- Las medidas del cuerpo se miden del propio modelo (p. ej. `Astronaut.calibrateHands` saca palma y
  dedos de la malla del guante), no se escriben a mano.
- El suelo de cada cuerpo es **una** función determinista compartida (`shared/space/surface.ts`:
  relieve global + modificadores). Todo cambio del suelo es un **modificador de terreno**
  (`shared/space/terrainMods/`): un tipo nuevo es una entrada en `MOD_KINDS` (función pura). Los
  del juego (cráteres) los guarda y reenvía el servidor; los de los sitios los rehace cada máquina.
- **Un lugar es un sitio** (`shared/space/sites.ts`: datos que emiten modificadores, pistas,
  puntos de aparición, NAV). Nada del terreno se escribe a mano para la base ni para la Luna.
- El servidor es autoritativo para daño, explosiones y ediciones, y para el estado de las naves
  (`ShipSim`: interruptores, integridad de cada panel). El cliente lleva un espejo del mismo `ShipSim`
  para predecir negativas ("sin energía") y evaluar la energía; en `?offline` hace de servidor.
- **Una mecánica de nave es un módulo** (`shared/ship/modules/`, contrato en `api.ts`, lista en `index.ts`); el núcleo no conoce ninguna máquina por su nombre. Ver `docs/SHIPS.md`.
- **Un sonido es datos** (`docs/AUDIO.md`): una máquina declara cómo suena en su módulo (`sounds()`, como
  `alerts()`), un sonido nuevo es un `defineSound` en `client/audio/sounds/`, un material de suelo una
  entrada en `SURFACES`, un tipo de mando una en `CONTROL_SOUNDS`. Todo suena desde su sitio real y
  llega por lo que lo transporta (aire, estructura, suelo, traje): nada de sonidos globales sin sitio.
  El audio no conoce naves: cualquier cosa con aire y estructura propia es un `AcousticHost`
  (`client/audio/acoustics.ts`, un adaptador por tipo: la nave es `shipSounds.ts`) y el exterior sale de
  los datos del cuerpo (atmósfera, viento, suelo). Nada de lechos de ruido de ambiente continuos.
- **Una nave es datos** (`shared/ship/ships/`): paneles convexos rompibles/reparables, consolas con
  mandos (`key` = interruptor que accionan, `requires` = bus que necesitan), subsistemas con disyuntor y
  recorrido de conductos (un panel destruido corta el bus que pasa por detrás), asientos, carga suelta.
  Mandos y consolas montados en un panel desaparecen si ese panel revienta.

## Naves en el prototipo Rust

La arquitectura de naves de `migracion/` (señales, mandos, máquinas, actuadores, nave de datos,
caminar a bordo, catálogo G, inspector F4) está en
[`migracion/docs/NAVES.md`](migracion/docs/NAVES.md); lo que falta, en
[`migracion/docs/PENDIENTES.md`](migracion/docs/PENDIENTES.md) (tachar al hacerlo).

**Movimiento en el prototipo Rust: un solo reloj** ([`migracion/docs/MOVIMIENTO.md`](migracion/docs/MOVIMIENTO.md)).
El mundo avanza en **pasos fijos de 1/60 s** (`lunar_play::game::Game::tick`, el mismo en el
cliente y en el servidor; el único acumulador es el del fotograma, que paga pasos) y se dibuja
**entre los dos últimos** (`Game::present` / `restore`: todo del mismo instante; las partículas,
atrás con su velocidad). Lo que se mueve entre estructuras (el jugador, y cualquier cosa nueva que
ande, flote o vaya cargada entre naves) lo mueve **el mundo**, loncha a loncha y justo después de
las estructuras: se implementa `Among` (`core/structure/schedule.rs`) y se pasa a `Builds::update`
(lo hace `Game::tick`). Nunca un acumulador ni un paso propios, ni código que «compense» la
velocidad de otra cosa: si algo
necesita saber a qué velocidad va lo de al lado para verse o chocar bien, la causa está en otro
sitio. Lo que se toca frena contra eso que se toca, no contra el mundo. Los números de cómo se
mueve algo van en datos (`scenario.jsonc`), no en el código.

**Lo que rige en un punto, en el prototipo Rust: una sola pregunta** ([`migracion/docs/MOVIMIENTO.md`](migracion/docs/MOVIMIENTO.md) §6-11).
Qué gravedad, qué suelo y qué arriba hay se le pregunta a `BodyRegistry::field(punto)` cada vez,
con el punto donde está la cosa **ahora**. Nada guarda «su cuerpo» (no existe `Structure::body`
ni `Body::gravity_at`), nada supone que haya suelo o arriba (fuera de la influencia de todo
cuerpo no los hay: `Field::ground` y `Field::up()` son `Option`) y nada depende de dónde se hizo,
de lo deprisa que va o de lo que pasó antes. `dominant` / `Field::nearest` son solo para dibujar.
Lo que pesa algo que anda o flota es `structure::weight::felt` (la gravedad propia de la nave en
sus salas, o el tirón menos lo que acelera lo que lo lleva), y su arriba es hacia donde pesa; sin
peso, el que traía. El norte es el eje del cuerpo (`Body::north_at`), nunca un eje del mundo, y
quien se orienta todo el tiempo no cuenta desde él: lleva su propio marco. Una referencia de
orientación nueva es una entrada en `app/src/nav.rs` y sus marcas en `navegacion.jsonc`; un
cambio de régimen se funde por pesos, nunca salta. Alcances, gravedad de a bordo, ritmos y
umbrales van en datos (`bodies/*.jsonc`, `ships/*.jsonc`, `scenario.jsonc`, `navegacion.jsonc`).
Toda regla nueva de este tipo se prueba barriendo cuerpos (los dos que hay y uno inventado),
dentro, en la franja y fuera, velocidades de 0 a 7 800 m/s, fotogramas de 10 a 240 fps y la
nave derecha, volcada y girando (`app/src/pilot/frames.rs` tiene los sitios y las posturas).

**Coherencia de las naves en Rust** ([`migracion/docs/NAVES.md`](migracion/docs/NAVES.md) «Coherencia
y ordenador de vuelo»): toda nave que vuela pasa `crates/ship/tests/coherencia.rs` (lo que un
piloto llamaría «no va»: teclas mantenidas, pasos de ruedas, retenes, cada tecla por su eje, nave
que no gira sola con gas, piloto automático que dice lo que no puede). Lo que dependa de los
fotogramas se prueba a 30, 60, 144 y 240 fps; una tecla mantenida manda `seat_keys::held(signo, dt)`
y el mando hace de eso un movimiento continuo (un retén atrapa lo que entra en él, no lo que sale).
Las teclas de un asiento se interpretan solo en `ship/src/seat_keys.rs`. Un ordenador de vuelo no
deja nada al estabilizador que pueda compensar al momento (el par real de los motores) ni pide
más giro del que sus toberas y giróscopos dan.

**Soltar y disparar en Rust:** se entrega posición y velocidad de mundo del punto de salida
(`structure::motion::Motion`, `Pilot::motion_in`), incluido el giro del portador, nunca la
velocidad relativa del jugador. No se desplaza otra vez una boca ya calculada. Lo balístico
avanza como `Among` (`rounds::Flight`); el barrido usa los dos extremos en las poses inicial y
final del sólido (`structure::motion::Sweep`), no un rayo contra la nave inmóvil. El impacto
guarda id, punto y dirección locales hasta aplicar el daño; el dibujo se toma después del
mundo y de las nuevas emisiones. La orientación del proyectil no es su velocidad de mundo.
El agarre frena respecto al movimiento de la mano, también fuera de cualquier nave.
Los efectos nacidos en movimiento reciben también `Motion` (`Effects::explode_moving`):
partículas, destello y origen de la sacudida heredan su velocidad. `Particle::drift` conserva
esa base en doble precisión; `vel` es expansión respecto a ella, para frenado y aspecto.
No estirar ni frenar una nube por la velocidad orbital que comparte con quien la ve.

## Optimización pendiente (obligatorio)

Los problemas de rendimiento conocidos están en una lista viva:
[`migracion/docs/OPTIMIZACION.md`](migracion/docs/OPTIMIZACION.md).

- **Antes de empezar:** léela.
- **Si tocas algo de esa lista:** táchalo en el mismo cambio, con la fecha y una línea de qué hiciste
  y cómo lo mediste. Si queda a medias, deja la entrada abierta y apunta qué falta.
- **Si descubres un problema nuevo de rendimiento:** apúntalo aunque no lo arregles.

## Rendimiento (reglas)

Lo que corre cada frame o cada paso **no deja basura**: objetos y arrays reutilizados (variables
`_scratch` de módulo, parámetros `out`), nada de `map/filter/forEach`, cierres, plantillas de texto
ni `clone()` en caminos calientes. Mídelo con `npm run perf` ([`docs/DIAGNOSTICS.md`](docs/DIAGNOSTICS.md)).

- **Calidad automática** ([`docs/RENDIMIENTO.md`](docs/RENDIMIENTO.md)): el perfil por defecto sigue a
  la GPU (integrada/software → BAJA, `render/gpuTier.ts`) y el regulador (`render/governor.ts`) baja
  MSAA, resolución y bloom cuando la GPU no llega a 60 Hz (nunca si el cuello es la CPU) y los sube
  cuando sobra. Un efecto nuevo caro entra como escalón suyo, no como coste fijo.

- **Sistemas de nave:** el `Tick` y sus contenedores (`demand`, `fuel`, `held`, `sources`, `events`)
  los reutiliza el núcleo tick tras tick: léelos, no los guardes. Un módulo que ofrece potencia
  reutiliza su `PowerSource`. `ShipSim.tick()` y `life.paths()` devuelven objetos reutilizados.
- **Luces reales:** nunca añadas una `THREE.*Light` a la escena; pide una al pool cada frame
  (`lights.spot(...)` / `lights.point(...)`, `client/render/lightPool.ts`): su número no cambia y no
  se recompilan shaders. Las de cabina son otro sistema (`interiorLights.ts`).
- **Vistas auxiliares:** una cámara registra un proveedor en `CameraSources`, una imagen un
  `PipDisplay` y un filtro una receta de `ImageMaterial` ([`docs/CAMERAS.md`](docs/CAMERAS.md)).
  La activación pertenece al adaptador; el núcleo no conoce asientos ni máquinas. Respeta el
  presupuesto global, sin sombras ni postprocesado extra; el terreno usa `update(camera, false)`
  para capturar nodos existentes sin generar trabajo. Toda captura restaura el visor principal.
- **Vista de nave:** lo estático en espacio de nave tiene `matrixAutoUpdate = false` y la raíz solo
  recalcula matrices cuando la nave se mueve. Una pieza nueva que se mueva sola hay que registrarla
  como viva (ver `ShipView.freezeStatic`). El interior se oculta de lejos (`INTERIOR_REACH`) y, con la
  cámara en una sala, solo se dibujan las salas y el exterior que se ven a través de puertas,
  ventanas y brechas (`ShipClient.portalView`, con 0,3 s de margen para no parpadear).
- **Resolución dinámica:** el cambio de tamaño del lienzo se aplica justo antes de dibujar, nunca
  después (un lienzo redimensionado queda en negro hasta que se vuelve a dibujar).
- **Sombras:** piezas pequeñas (< 10 cm), mandos y el interior de salas sin ventana no proyectan.
- **Rocas:** `BatchedMesh` con culling y LOD por instancia (`world/rocks.ts`).

## Espacio (reglas)

- «Arriba» no es +Y: pídeselo a `shared/space/body.ts` (`frameAt`, `gravityAt`, `altitudeOf`). Cerca
  de la base coincide con +Y; en órbita, no.
- «¿Está en el suelo?» es `heightAboveGround` (en el cliente, `game.groundAlt`), nunca `y` contra
  `terrain.height(x, z)`: eso solo vale en el mundo plano de la base.
- «El cuerpo» es `bodyAt(p)`, nunca `MOON_BODY` a mano (habrá más cuerpos y sistemas).
- **Origen flotante** (`client/render/origin.ts`): las matrices de three.js (`matrixWorld`,
  `getWorldPosition`, `lookAt`…) están en espacio de render (mundo − O). Lo nuevo que se coloque en
  mundo cuelga de `origin.root`; la lógica que lea una matriz convierte con `origin.toWorld`.
- **Marco 0 = burbuja de física** (`client/frames/bubble.ts`), no el mundo: sus coordenadas pasan
  por `Frames` (`toWorld`, `transfer`); todo cuerpo de Rapier del marco 0 se recoloca cuando la
  burbuja se mueve (añadirlo a `Game.followBubble`). En la red el marco 0 son coordenadas de mundo.
- El suelo, las rocas, los cráteres y sus colisiones existen igual en cualquier punto de cualquier
  cuerpo: no hay parche ni zona especial. La superficie de un cuerpo es `surfaceOf(cuerpo, semilla)`
  (en el cliente, `game.surfaces`).
- Colocar algo en el suelo (naves, cajas, personas, sitios) se hace en un marco tangente
  (`shared/space/tangent.ts`: `SurfaceGround`, `placeShip`), nunca con `y = altura(x, z)`.
- En un shader, nada de posiciones de mundo en coordenadas absolutas para algo que tenga que ser
  preciso: a millones de metros el float32 falla por centímetros. Usa espacio de vista o de objeto.
- x y z pequeñas no significan «cerca de la base»: en las antípodas también lo son. Las distancias
  y rumbos a un lugar van por el círculo máximo (`bearingTo`, `baseFrom`).
- Texturas o cálculos del terreno en un shader: en coordenadas del nodo (metros de la cara relativos
  a su esquina), nunca en posiciones de mundo.
- En órbita cerrada el ordenador de vuelo no sostiene el peso (caer alrededor es la órbita): solo si
  se le pide una velocidad vertical. Todo empuje suelto (giros bruscos incluidos) mueve la órbita.
- Piloto automático: cada modo dice en qué cara del tambor va (`face` en `flight/autopilot.ts`: 0
  superficie, 1 órbita, 2 espacio). Los de superficie se anulan en régimen orbital. Un modo nuevo es
  una entrada en `AP_MODES`; su botón sale solo en su cara (`ships/kit.ts` `autopilotConsole`).

## Bucle de verificación (obligatorio antes de hacer commit)

Detalle de todas las herramientas: [`docs/DIAGNOSTICS.md`](docs/DIAGNOSTICS.md).

```bash
npm run typecheck
npm run dev                  # en otra terminal
npm run diag:joints          # articulaciones: ángulos por eje y velocidades angulares en un guion de movimientos
npm run diag:grasp           # agarre del arma: mide palma/dedos contra la empuñadura y muñecas (flexión y giro), falla con exit 1, + primeros planos
npm run diag:pose            # capturas del astronauta desde varias cámaras
npm run diag:terrain         # capturas del terreno
npm run diag:ship            # nave: mandos, energía, daño, reparación, rampa, cajas, asientos, zoom (exit 1) + capturas
npm run diag:ship:net        # nave en red: 2 clientes reales contra el servidor (mandos, impacto, soldadura)
npm run test:motion          # movimiento: vaivén de red, cambios de marco, burbuja, relevo del piloto (sin navegador, exit 1)
npm run test:motion:net      # lo mismo contra el servidor real (puerto 3107): sellos de tiempo y relevo del piloto
npm run test:sim             # núcleo del mundo: azar, cola, tablas, determinismo a trozos, guardar/cargar, causas, hilo (exit 1)
npm run test:sim:server      # el servidor real con su mundo (puerto 3108): lo crea, lo guarda, lo reanuda
npm run bench:sim            # rendimiento del núcleo (eventos/s, barridos/s, guardado) y lo que tardaría la prehistoria
npm run sim:why -- <id|e<entidad>|recent>   # el historiador: por qué pasó algo, leyendo data/world
npm run test:net             # réplica por interés: contra fuerza bruta y en el servidor real (puerto 3109)
npm run test:world           # objetos guardados por el mundo, almacén que saca y recoge, reinicio (puerto 3110)
npm run test:actors          # andador en el suelo real, percepción, mentes por hora del día
npm run test:npcs            # NPC en el servidor real, testigos que reaccionan, «qué hay aquí» (puerto 3111)
npm run test:galaxy          # galaxia, regiones con su cuerpo, salto en el servidor real (puerto 3112)
npm run test:render          # calidad automática: detección de GPU y regulador contra un modelo de frame
npm run sim:seeds            # barrido de semillas (distribuciones, qué pasa por siglo) → tools/sim/out/seeds.html
npm run sim:scope            # sismógrafo: los indicadores del mundo en el tiempo → tools/sim/out/scope.html
```

Las capturas quedan en `tools/diag/out/` (con hoja de contactos `*_sheet.png`). **Mírelas**: una
métrica que da siempre OK puede estar midiendo su propia suposición (ya pasó: el chequeo del agarre
usaba la misma calibración que el IK y no veía la mano al revés; ahora mide la malla renderizada).

En el juego, **F7** mide el salto por fotograma de todo lo que se ve (`game.motion.report()`).

Automatización desde el navegador (`?manual&offline`):
`game.step(n)`, `game.me.graspReport()`, `game.systems.list()`, `game.focusCam = { part: 'handR', az, el, dist }`,
`game.debug` (controlador, cámara, input), `game.groundY(x, z)` (y del suelo bajo x, z), `game.surfaces`,
`?cam=x,z,yaw,pitch,h` (x, z en el marco del sitio de inicio, h sobre su suelo), `?nobaked`, `?nomorph`, `?ao`.
Naves: `game.ships[0]` (`.sim`, `.anim`, `.physics`, `.cargo`), `game.shipControl('ck.main/ramp')`,
`game.blast(worldPoint)` (offline), `game.sitDown(ship, i)` / `game.standUp()`, `game.interaction.target`.

Requisitos de las herramientas: `npm i -D playwright && npx playwright install chromium`.
Regenerar el traje: Python 3.11 con `bpy==4.5.*` → `python tools/blender/astronaut.py --out public/assets/astronaut.glb`.
