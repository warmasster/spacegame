# Diagnóstico

Herramientas para **medir** en vez de adivinar. Regla: toda corrección visual se acompaña de una
métrica que falle si el problema vuelve, **y** de capturas que se miran (una métrica puede estar
midiendo su propia suposición).

## En el juego

| Tecla | Qué muestra |
|---|---|
| F3 | Frame (media/máx), draw calls, triángulos, cuerpos/colisiones, trabajos de terreno, **ms por sistema** |
| F4 | Terreno en alambre |
| F5 | Colisiones de Rapier |
| F6 | **Articulaciones**: tabla por hueso y ejes RGB dibujados sobre el astronauta |

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

## Herramientas automáticas (`tools/diag/`)

Requisitos: `npm run dev` corriendo y `npm i -D playwright && npx playwright install chromium`.
Salida en `tools/diag/out/` (ignorada por git).

| Comando | Qué hace | Falla si… |
|---|---|---|
| `npm run diag:joints` | Recorre un guion (quieto, andar, correr, saltar, agacharse, girar, desenfundar, apuntar, disparar, andar armado, enfundar) registrando **todas las articulaciones cada frame**. Escribe `joints.json` (serie temporal completa), `joints_worst.txt` (peor valor por hueso y en qué fases se salió) y `joints_gizmos.png` | alguna articulación sale de su rango o supera su velocidad máxima |
| `npm run diag:grasp` | Agarre del arma en 4 poses: separación palma–empuñadura, orientación de la palma, dedos cruzando el mango, **flexión y giro de muñeca**, medidos sobre la malla renderizada del guante. Primeros planos de cada mano + `grasp_sheet.png` | palma > 3,5 cm, mal orientada, o muñeca > 45° |
| `npm run diag:pose` | Capturas del astronauta desde cámaras en órbita (`orbit,zoom,andar,armado;…`, `orbit -10` = primera persona) | — (revisión visual) |
| `npm run diag:terrain` | Capturas de terreno desde cámaras libres (`x,z,yaw,pitch,altura;…`) | — (revisión visual) |
| `npm run test:ship` | Nave sin navegador: definición (ayuda, tapas, manual, pilones), páginas de las pantallas, tapa de seguridad, escalas que no dan la vuelta y selectores que sí, casco abierto a 0 kPa, presurizar al cerrar, enclavamientos (reactor, rampa presurizada, motor destruido), una lámpara del anunciador por grupo de alarmas, referencias de datos válidas, fallo de la APU enclavado, umbilical sin O₂, una nave mínima hecha desde cero, un módulo de sistema nuevo y soldadura de una máquina. Imprime el porcentaje. `diag:ship` sigue siendo la pasada visual, mucho más lenta | alguna comprobación falla |
| `npm run diag:ship` | Nave (`?offline`): cada mando hace lo suyo o se niega con motivo (enclavamiento del tren), reactor/disyuntores cortan sus buses, puertas/rampa/escudo viajan y sus colisiones siguen, explosiones abren brechas (sin colisión, abiertas a rayos, alarma), conducto cortado deja sin energía, soldadora + clic reconstruye el panel (el lanzacohetes no), cajas dinámicas en reposo y lanzadas por una explosión, sentarse/levantarse, zoom, clic por la mirilla, **subir la rampa andando**, **cada luz exterior apoyada en el casco** (rayo desde fuera a lo largo de su normal) y **la mochila del astronauta sentado no atraviesa el asiento** (vértices de la malla renderizada contra las cajas de `SEAT_BOXES`). Capturas `ship_*.png` + `ship_sheet.png`, informe `ship_report.txt`. `node tools/diag/ship.mjs views\|checks` para una parte | alguna comprobación falla o hay errores de página/shader |
| `npm run diag:manual` | Manual de la nave (M) en `?offline`: se abre con capítulos, fichas de mandos, plano con consolas y lecturas en vivo; capturas `manual_*.png` (inicio, plano, energía, alarmas, mandos, búsqueda) | el manual no se abre, le faltan partes generadas o hay errores de página |
| `npm run diag:ship:net` | Dos clientes reales contra el servidor (**reinícialo** si cambió `src/server`): A pulsa la rampa → B la ve; impacto de cohete de A → el servidor daña el panel para ambos; A suelda (con la soldadora en mano) → B ve subir la integridad y chispas | alguna comprobación falla |

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
