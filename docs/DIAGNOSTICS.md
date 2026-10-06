# Diagnóstico

Herramientas para **medir** en vez de adivinar. Regla: toda corrección visual se acompaña de una
métrica que falle si el problema vuelve, **y** de capturas que se miran (una métrica puede estar
midiendo su propia suposición).

## En el juego

| Tecla | Qué muestra |
|---|---|
| F3 | Frame (media/máx); draw calls y triángulos **del frame entero** (todas las pasadas del postproceso y las cascadas de sombra, no solo la última); **draws por categoría** (naves, terreno, rocas, astronautas…: escena / sombras · triángulos); **basura JS** (MB/s, Chromium); **subidas de textura/s**; cuerpos/colisiones, trabajos de terreno, **ms por sistema** |
| F4 | Terreno en alambre |
| F5 | Colisiones de Rapier |
| F6 | **Articulaciones**: tabla por hueso y ejes RGB dibujados sobre el astronauta |
| F7 | **Movimiento**: salto por fotograma frente a la cámara de todo lo que se mueve (naves, cajas, proyectiles, astronautas), gráfica de los últimos segundos y eventos que lo causan (cambios de marco, burbuja recolocada, correcciones de red, reloj de pasos). Ver [`MOVIMIENTO.md`](MOVIMIENTO.md) |

### Panel de articulaciones (F6)

Cada hueso se mide **respecto a su pose de reposo**, sobre los **ejes del modelo** (los mismos con
los que anima el código, `Astronaut.pose`), así todos los huesos se leen igual aunque Blender los
orientase distinto:

| Columna | Eje | Gizmo | Significado |
|---|---|---|---|
| `flex` | X | rojo | flexión / extensión (codo, rodilla, inclinar hacia delante) |
| `twist` | Y | verde | giro sobre el eje vertical / del miembro (girar el torso, pronación del antebrazo) |
| `side` | Z | azul | abducción / inclinación lateral (levantar el brazo de lado) |
| `°/s` | — | — | velocidad angular del hueso en el mundo este frame |
| `pico` | — | — | máxima velocidad angular desde que se abrió el panel |

Cálculo: Euler XYZ de `Mᵀ · (reposo⁻¹ · actual) · M`, con `M` = ejes del modelo en el espacio del
hueso (`src/client/player/jointDiag.ts`).

`⚠` marca un hueso fuera de **`LIMITS`**: rango de movilidad de un traje EVA presurizado (aprox.
EMU de la NASA, generoso) y velocidad angular máxima plausible. Fuera de rango = pose imposible;
pico de velocidad = "pop" o salto entre frames. Los límites están en `jointDiag.ts` y se ajustan
ahí (un solo sitio).

Opciones de URL de render: `?rdepth` prueba la profundidad invertida (`reversedDepthBuffer`, conserva
el early-Z que la logarítmica anula; necesita `EXT_clip_control`), `?ao`, `?nan`.

Las categorías de F3 salen de `userData.cat` del objeto o de un antecesor (`game.ts` las pone en las
raíces: naves, terreno, rocas, astronautas, partículas, cajas…). Un objeto nuevo en la escena debería
llevar la suya.

### Sonda de movimiento (F7)

Para cada cosa dibujada que se mueve compara, en cada fotograma, su posición relativa a la cámara
con la que predecían los dos fotogramas anteriores a velocidad constante. Un movimiento suave, por
rápido que sea (una nave a 1,6 km/s a tu lado), falla por micras; un salto, un parón o un vaivén se
ve con su tamaño en metros. `!` marca lo que pasa de 5 cm. Debajo: el reloj de pasos (error y saltos),
la burbuja (modo, velocidad, versión), tu marco, y por cada nave y astronauta remoto la corrección de
red más grande y cuántas se tomaron como salto. Los eventos llevan su antigüedad: si un salto coincide
con «caja 3: nave 1 → mundo» o con «burbuja», ese es el sospechoso.

`game.motion.enabled = true` graba sin abrir el panel; `game.motion.report()` devuelve
`{ subjects: [{ key, window, worst, last }], events }` (metros).

## Movimiento sin navegador (`npm run test:motion`)

`tools/motion/check.ts` reproduce lo que produce vaivenes y lo mide igual que F7, con el sistema
antiguo como control (la prueba falla si deja de ver el fallo antiguo): flujo de poses de nave con
temporizador de servidor irregular, red con latencia variable y cliente a 144 Hz con tirones, a 250 y
1600 m/s; relevo del piloto; salida por una puerta con la nave girando; burbuja recolocada; cambio de
marco con Rapier; regla de pertenencia; disparos tardíos; tiempos en el códec. `--verbose` para los
números.

`npm run test:motion:net` (`tools/motion/net.ts`) hace lo mismo contra **el servidor de verdad** (lo
arranca en el puerto 3107, sin navegador): un cliente pilota una nave a 1600 m/s desde un bucle de paso
fijo con temporizador irregular, otro observa a bordo; comprueba que las poses del piloto y del
servidor van selladas con su paso (la velocidad entre muestras coincide con la declarada) y que al
levantarse el piloto el servidor sigue desde donde está la nave ahora (sin salto atrás).

## Núcleo del mundo (`npm run test:sim`, `test:sim:server`, `bench:sim`, `sim:why`)

Todo sin navegador ni gráficos ([`MUNDO.md`](MUNDO.md) §9):

- `test:sim` (`tools/sim/check.ts`): azar (uniformidad, independencia entre tiradas y entidades),
  la cola contra una referencia ordenada, las tablas contra un `Map`, formas cerradas, calendario;
  **determinismo** (un mundo de juguete, `tools/sim/scenario.ts`, de una vez = a trozos al azar con
  presupuestos al azar, en 20 semillas); **guardar/cargar** (seguir tras cargar = no haber parado, en
  memoria y en disco; partidas dañadas; versiones con otros campos, tablas y eventos); causas; el
  presupuesto; el hilo (en proceso y en un worker de Node de verdad). Exit 1; `--verbose`.
- `test:sim:server` (`tools/sim/server.ts`): el servidor real (puerto 3108) con su mundo en una carpeta
  temporal: lo crea, lo guarda, lo reanuda tras matarlo.
- `bench:sim`: eventos/s del núcleo, barrido de un millón de filas, guardado y carga de un mundo
  grande, y cuánto tardaría la prehistoria del informe.
- `sim:why` (el historiador): `npm run sim:why -- 1234` sigue las causas del registro 1234 hasta la
  raíz y lo que causó; `e57` todo lo de la entidad 57; `recent 50` lo último. Lee la última partida
  guardada (`data/world` o `WORLD_DIR`), nunca escribe.
- `sim:seeds` (barrido de semillas): muchas semillas en paralelo. Da la distribución de cada
  indicador, qué pasa por siglo, cuántas historias distintas hay y los errores. Salida en
  `tools/sim/out/seeds.html` y en JSON. `--n`, `--years`, `--modules`, `--workers`.
- `sim:scope` (el sismógrafo): los indicadores del mundo del servidor (`series.json` junto a la
  partida) o de un mundo simulado con `--run <años>`, con gráficas en la consola y en
  `tools/sim/out/scope.html`.

## El mundo con el motor (`test:net`, `test:world`, `test:actors`, `test:npcs`, `test:galaxy`)

Sin navegador. El servidor real arranca en su propio puerto con `DEV_TOOLS=1`, con clientes de
`tools/net/harness.ts` ([`RED.md`](RED.md), [`MUNDO.md`](MUNDO.md) §10-14).

- `test:net`: la réplica por interés contra fuerza bruta, y en el servidor: aprender y olvidar
  objetos al moverse, un objeto hecho en marcha que solo sabe quien está cerca.
- `test:world`: el mundo guarda los objetos. El almacén saca 9 cajas y las recoge; la que se coge
  queda donde la dejaron, también tras guardar y reiniciar.
- `test:actors`: el andador sobre el suelo real (en la base y lejos, rodeando obstáculos), la
  percepción por medio, y las mentes por hora del día.
- `test:npcs`: la dotación de la base en el servidor real, con estados cada 50 ms y pies en el suelo;
  un disparo con testigos que reaccionan y la cadena de causas; «qué hay aquí»; al irse, los cuerpos
  se van.
- `test:galaxy`: la galaxia, las regiones con su cuerpo y su gravedad, las reglas del salto, y un
  salto real en el que los informes tardíos se descartan.

Los mensajes `dev` (`obj.spawn`, `obj.despawn`, `world.ask`, `world.query`, `world.save`) solo se
aceptan con `DEV_TOOLS=1`.

## Calidad automática (`npm run test:render`)

La detección de la GPU, que decide el perfil por defecto, y el regulador que baja o sube la imagen
según el tiempo de la GPU ([`RENDIMIENTO.md`](RENDIMIENTO.md)), comprobados contra un modelo de
fotograma sin navegador. En el juego, F3 → «calidad automática» muestra el escalón, la resolución,
MSAA, bloom y si el tiempo de GPU es medido o estimado.

## Rendimiento (`npm run perf`)

`tools/perf/perf.ts` mide sin navegador (Node + V8, sin GPU) lo que cuesta cada camino caliente y la
**basura** que deja (perfilador de montículo de V8, contando también lo que recogen las GC menores):

```bash
npm run perf                 # todo
npm run perf -- ships sim    # solo unas secciones: ships · sim · rocks · world · net
npm run perf -- --json out.json
```

- `ships`: `ShipView` de cada nave (mallas, draws, sombras, triángulos, materiales, programas) y el
  coste/basura de `update` por frame.
- `sim`: `ShipSim.tick` (sistemas, 20 Hz) y `flight.step` despierta, en estacionario y dormida.
- `rocks`, `world` (`terrain.height` con y sin 1.000 cráteres), `net` (tamaños de mensajes).

Objetivo: < 20 KB de basura por frame en total y ~0 en una nave aparcada. Un cambio en un camino
caliente (vuelo, sistemas, vista de nave, partículas…) se mide antes y después.

## Herramientas automáticas (`tools/diag/`)

Requisitos: `npm run dev` corriendo y `npm i -D playwright && npx playwright install chromium`.
Salida en `tools/diag/out/` (ignorada por git).

| Comando | Qué hace | Falla si… |
|---|---|---|
| `npm run diag:joints` | Recorre un guion (quieto, andar, correr, saltar, agacharse, girar, desenfundar, apuntar, disparar, andar armado, enfundar) registrando **todas las articulaciones cada frame**. Escribe `joints.json` (serie temporal completa), `joints_worst.txt` (peor valor por hueso y en qué fases se salió) y `joints_gizmos.png` | alguna articulación sale de su rango o supera su velocidad máxima |
| `npm run diag:grasp` | Agarre del arma en 4 poses: separación palma–empuñadura, orientación de la palma, dedos cruzando el mango, **flexión y giro de muñeca**, medidos sobre la malla renderizada del guante. Primeros planos de cada mano + `grasp_sheet.png` | palma > 3,5 cm, mal orientada, o muñeca > 45° |
| `npm run diag:pose` | Capturas del astronauta desde cámaras en órbita (`orbit,zoom,andar,armado;…`, `orbit -10` = primera persona) | — (revisión visual) |
| `npm run diag:terrain` | Capturas de terreno desde cámaras libres (`x,z,yaw,pitch,altura;…`) | — (revisión visual) |
| `npm run test:ship` | Nave sin navegador: definición, pantallas, tapas, atmósfera, enclavamientos, catálogo, Peregrina (esclusa, víveres, solar) y vuelo offline (quieta en el suelo, VTOL despega, el RCS guía, un punto a bordo sigue a la nave). Imprime el porcentaje. `diag:ship` sigue siendo la pasada visual, mucho más lenta | alguna comprobación falla |
| `npm run diag:ship` | Nave (`?offline`): cada mando hace lo suyo o se niega con motivo (enclavamiento del tren), reactor/disyuntores cortan sus buses, puertas/rampa/escudo viajan y sus colisiones siguen, explosiones abren brechas (sin colisión, abiertas a rayos, alarma), conducto cortado deja sin energía, soldadora + clic reconstruye el panel (el lanzacohetes no), cajas dinámicas en reposo y lanzadas por una explosión, sentarse/levantarse, zoom, clic por la mirilla, **subir la rampa andando**, **cada luz exterior apoyada en el casco** (rayo desde fuera a lo largo de su normal) y **la mochila del astronauta sentado no atraviesa el asiento** (vértices de la malla renderizada contra las cajas de `SEAT_BOXES`). Capturas `ship_*.png` + `ship_sheet.png`, informe `ship_report.txt`. `node tools/diag/ship.mjs views\|checks` para una parte | alguna comprobación falla o hay errores de página/shader |
| `npm run diag:manual` | Manual de la nave (M) en `?offline`: se abre con capítulos, fichas de mandos, plano con consolas y lecturas en vivo; capturas `manual_*.png` (inicio, plano, energía, alarmas, mandos, búsqueda) | el manual no se abre, le faltan partes generadas o hay errores de página |
| `npm run test:equipment` | Catálogos, red, munición, contactos de suelo expuestos y 24 cráteres separados con direcciones redondeadas, sin navegador | identidad, serialización, autoridad, replicación o distancias incoherentes |
| `npm run test:cameras` | Fuentes y mandos genéricos, apagado por asiento/energía, foco central, óptica, fijación, munición, efectos de imagen y planificación PiP; canvas/renderer simulados | violaciones de permisos, presupuesto, visibilidad, origen o restauración de estado |
| `npm run test:terrain` | Buffers reales del worker en bordes de igual/distinto LOD: geometría, normales, albedo, material y sombras; parche del shader de profundidad | bordes a más de 0,1 mm, normales incoherentes, sombras discontinuas o faldones sin excluir |
| `npm run diag:space` | Panel de vuelo a 1.600 m/s sin cambiar de pestaña, caja saliendo de una nave con giro, cohete por la rampa abierta y casco sólido; cometa de dos triángulos, capturas de suelo, montañas y cielo (`space_sheet.png`) | panel sin refresco, saltos de posición/velocidad/giro, impactos falsos o errores de shader |
| `npm run test:space` | Coordenadas de casco móvil a millones de metros, cordilleras deterministas y límites de altura | error > 10 nm en la transformación, relieve sin crestas o cumbres fuera de los límites |
| `npm run diag:equipment` | Cuatro cohetes y torreta offline con clic real; cada cráter medido sobre malla visible y colisiones; capturas `equipment_*.png` | impactos separados se fusionan, una malla no se reconstruye o hay excepciones |
| `npm run diag:ship:net` | Dos clientes y uno que entra después, contra una instancia con mundo limpio (**reiníciala** si cambió `src/server`): mandos, daño, soldadura, herramienta remota, puntería/disparo/munición de torreta y persistencia de cráteres | alguna comprobación falla |

## API de automatización (consola / Playwright, con `?manual&offline`)

```js
game.step(n, dt = 1/30, render = true)   // avanzar n frames
game.me.graspReport()                    // métricas del agarre por mano
game.joints.sample(dt) / .last / .summary() / .reset()
game.jointsRecording = true              // muestrear articulaciones cada frame sin abrir el panel
game.focusCam = { part: 'handR', az, el, dist }   // primer plano de un hueso
game.systems.list(), game.systems.timings         // sistemas y ms
game.debug                               // controlador, cámara, input (setKey)
```

URL: `?cam=x,z,yaw,pitch,h` (cámara libre), `?nobaked`, `?nomorph`, `?skirts`, `?ao`.

Naves: `game.ships[0].sim` (estado autoritativo espejado: `sw`, `hp`, `powered(bus)`, `blocked(ctl)`),
`game.shipControl(id)`, `game.blast(p)` (offline), `game.ships[0].physics.castRay(o, d, max)`,
`game.sitDown(ship, i)` / `game.standUp()`, `game.me.equip('welder' | 'launcher')`.

## Añadir un diagnóstico nuevo

1. La medida vive junto al código que mide (p. ej. `graspReport` en `Astronaut`, `JointDiagnostics`).
2. Mide lo **renderizado** (malla, huesos finales), no las variables internas del algoritmo.
3. Un script en `tools/diag/` que la ejecute en varias situaciones, guarde capturas y salga con
   código 1 si falla; añádelo a `package.json` y a esta tabla.
