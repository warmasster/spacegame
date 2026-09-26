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

## Añadir un diagnóstico nuevo

1. La medida vive junto al código que mide (p. ej. `graspReport` en `Astronaut`, `JointDiagnostics`).
2. Mide lo **renderizado** (malla, huesos finales), no las variables internas del algoritmo.
3. Un script en `tools/diag/` que la ejecute en varias situaciones, guarde capturas y salga con
   código 1 si falla; añádelo a `package.json` y a esta tabla.
