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
| `src/client/audio/` | Sonido sintetizado y posicional: motor (`engine.ts`, `sfx`), propagación física aire/casco/suelo/traje (`medium.ts`), banco y recetas (`bank.ts`, `sounds/`), naves (`shipSounds.ts`), tripulación y casco. Guía: [`docs/AUDIO.md`](docs/AUDIO.md) |
| `src/shared/` | Código común cliente/servidor: protocolo de red, constantes, ruido |
| `src/shared/frames/` | Marcos de referencia compartidos: el mundo y los anfitriones (naves…), paso de uno a otro sin saltos (`carry`, `hostAt`), cuerpo balístico que vuela por ellos (`ballistic.ts`). Guía: [`docs/EQUIPO.md`](docs/EQUIPO.md) |
| `src/shared/items/` | Catálogos de equipo: proyectiles (cohete, bala), armas y herramientas (lanzacohetes, soldadora, fusil), objetos sueltos (caja, pieza de repuesto). Guía: [`docs/EQUIPO.md`](docs/EQUIPO.md) |
| `src/shared/ship/` | Naves como datos: `ships/` (la Selene, la Peregrina y el Albatros, de dos cubiertas), `catalog/` (componentes reutilizables), `def.ts` (tipos, `finishShip`), `sim.ts` (mandos, daño), `flight.ts` (vuelo offline), `systems.ts` (núcleo), `airflow.ts` (aire en movimiento: tirón, chorros, paneles bajo presión) y `modules/`. Guía: [`docs/SHIPS.md`](docs/SHIPS.md) |
| `src/shared/space/` | Cuerpos celestes como datos: gravedad radial, marco local, órbitas, eclipse; su suelo (`surface.ts`), los modificadores de terreno (`terrainMods/`), los sitios (`sites.ts`), las rocas (`rocks.ts`) y los marcos tangentes (`tangent.ts`). Guía: [`docs/ESPACIO.md`](docs/ESPACIO.md) |
| `src/server/` | Servidor autoritativo (salas, daño, explosiones, ediciones de terreno) |
| `tools/blender/` | Traje del astronauta generado por código (Blender headless → `public/assets/astronaut.glb`) |
| `tools/diag/` | Herramientas de autodiagnóstico visual y numérico |

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
  `defineObject` (`docs/EQUIPO.md`).
- **Todo lo que se mueve vive en un marco** (`shared/frames`): dentro de una nave, en su espacio; al
  salir, en el mundo, con la misma posición y velocidad reales. La regla de «dentro» es una sola
  (`FrameHost.inside`); lo que vuela libre usa `stepBallistic`, igual en cliente y servidor. En la
  red viaja con su marco (`fr` + coordenadas locales).
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
  llega por lo que lo transporta (aire, casco, suelo, traje): nada de sonidos globales sin sitio.
- **Una nave es datos** (`shared/ship/ships/`): paneles convexos rompibles/reparables, consolas con
  mandos (`key` = interruptor que accionan, `requires` = bus que necesitan), subsistemas con disyuntor y
  recorrido de conductos (un panel destruido corta el bus que pasa por detrás), asientos, carga suelta.
  Mandos y consolas montados en un panel desaparecen si ese panel revienta.

## Rendimiento (reglas)

Lo que corre cada frame o cada paso **no deja basura**: objetos y arrays reutilizados (variables
`_scratch` de módulo, parámetros `out`), nada de `map/filter/forEach`, cierres, plantillas de texto
ni `clone()` en caminos calientes. Mídelo con `npm run perf` ([`docs/DIAGNOSTICS.md`](docs/DIAGNOSTICS.md)).

- **Sistemas de nave:** el `Tick` y sus contenedores (`demand`, `fuel`, `held`, `sources`, `events`)
  los reutiliza el núcleo tick tras tick: léelos, no los guardes. Un módulo que ofrece potencia
  reutiliza su `PowerSource`. `ShipSim.tick()` y `life.paths()` devuelven objetos reutilizados.
- **Luces reales:** nunca añadas una `THREE.*Light` a la escena; pide una al pool cada frame
  (`lights.spot(...)` / `lights.point(...)`, `client/render/lightPool.ts`): su número no cambia y no
  se recompilan shaders. Las de cabina son otro sistema (`interiorLights.ts`).
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
```

Las capturas quedan en `tools/diag/out/` (con hoja de contactos `*_sheet.png`). **Mírelas**: una
métrica que da siempre OK puede estar midiendo su propia suposición (ya pasó: el chequeo del agarre
usaba la misma calibración que el IK y no veía la mano al revés; ahora mide la malla renderizada).

Automatización desde el navegador (`?manual&offline`):
`game.step(n)`, `game.me.graspReport()`, `game.systems.list()`, `game.focusCam = { part: 'handR', az, el, dist }`,
`game.debug` (controlador, cámara, input), `game.groundY(x, z)` (y del suelo bajo x, z), `game.surfaces`,
`?cam=x,z,yaw,pitch,h` (x, z en el marco del sitio de inicio, h sobre su suelo), `?nobaked`, `?nomorph`, `?ao`.
Naves: `game.ships[0]` (`.sim`, `.anim`, `.physics`, `.cargo`), `game.shipControl('ck.main/ramp')`,
`game.blast(worldPoint)` (offline), `game.sitDown(ship, i)` / `game.standUp()`, `game.interaction.target`.

Requisitos de las herramientas: `npm i -D playwright && npx playwright install chromium`.
Regenerar el traje: Python 3.11 con `bpy==4.5.*` → `python tools/blender/astronaut.py --out public/assets/astronaut.glb`.
