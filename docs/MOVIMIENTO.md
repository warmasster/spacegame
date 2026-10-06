# Movimiento suave a cualquier velocidad

Todo lo que se mueve (naves, astronautas, cajas y cualquier objeto suelto, proyectiles, escombros)
debe verse **continuo** frente al ojo, vaya a 2 m/s o a 1600 m/s, lo simule este cliente, otro o el
servidor, y cambie de marco (nave ↔ mundo) cuantas veces quiera. A 1600 m/s un milisegundo son
1,6 m y un paso fijo (1/60 s) son 27 m: cualquier desajuste de tiempo o de marco se ve como un
vaivén «palante-patrás».

Este documento dice **qué reglas lo garantizan**, **qué módulo** implementa cada una y **cómo se
comprueba**. Pruebas: `npm run test:motion` (sin navegador) y la sonda **F7** en el juego.

## 1. Qué fallaba (y dónde estaba)

| Síntoma | Causa | Tamaño medido antes → ahora (`test:motion`) |
|---|---|---|
| Nave que no pilotas vibrando adelante/atrás | Las poses iban selladas con la **hora de reloj de pared** del temporizador del servidor (Windows: saltos de ~15,6 ms, varios pasos con la misma hora) y el cliente las reproducía con un reloj que corregía a tirones | 3,7 m → 3 mm (250 m/s) · 23,8 m → 3 mm (1600 m/s) |
| Caja que sale de la nave y pega un salto | Se pasaba de marco **antes** de que los cuerpos alcanzaran a los marcos: pose de la nave de este paso con la caja del paso anterior | 4,2 m → 0,05 mm (250 m/s) |
| Todo lo de la burbuja salta al recolocarse | Al recolocar la burbuja se «congelaba» un paso (sin interpolar) | 35,7 m → 0 (1600 m/s) |
| Caja/astronauta que sale por la puerta se para un paso | El cambio de marco reiniciaba la interpolación | 0,64 m → 0,2 mm |
| Se levanta el piloto y la nave se congela y salta atrás | Sin poses hasta que llegaba la primera del servidor, que además partía de la última pose adoptada (un viaje de red de antigüedad) | continuo (< 2 mm) |
| Tercera persona: la cámara se queda atrás y tiembla | Suavizado en coordenadas absolutas: a 250 m/s, 20 m detrás y temblando con cada variación del fotograma | suavizado relativo al astronauta |
| Escombros a trompicones | Sin interpolar entre pasos | interpolados como todo |
| Caja lanzada fuera a 250 m/s se queda atrás | Amortiguación lineal de Rapier medida en el marco de simulación (frena contra la burbuja, no contra nada real) | sin amortiguación en vacío |
| Astronauta que salta de una nave rápida frenado o recortado | El control en el aire frenaba hacia la velocidad del marco y la vertical se recortaba a 9 m/s | empuje solo en la dirección pedida |
| Disparo de otro jugador en órbita aparece detrás | El proyectil nacía donde estaba el tirador hace un viaje de red | adelantado al presente |

## 2. Tiempo: el reloj de pasos (`shared/time/stepClock.ts`)

- Cada máquina tiene un `StepClock` en el **dominio del reloj del servidor** (ms). `step()` al
  empezar cada paso fijo; `t` es el tiempo al que pertenece el estado de ese paso.
- **Todo lo que se envía lleva ese `t`**: poses de nave (servidor y piloto), estados del astronauta
  (`state.t`), cajas (`CrateWire.t`), disparos (`fire.t`). Nunca la hora de envío ni la de llegada.
- El reloj se engancha a su referencia (`sync`) **cambiando su ritmo** (±0,5 % como mucho), nunca
  moviendo `t` entre dos pasos: las poses ya calculadas dejarían de corresponder al tiempo dibujado.
  Solo salta si va a más de 250 ms (arranque, parón largo).
- Servidor: `sync(ahora − lo que queda en el acumulador)` tras cada tanda de pasos. Cliente:
  `sync(serverNow − α·paso)` tras `loop.advance`.
- Tiempo **dibujado** en un fotograma: `renderTime(α)` = un paso por detrás del último (la
  interpolación muestra el mundo un paso tarde, todo por igual).

## 3. Lo que simula otro: réplicas en el presente (`shared/net/replica.ts`)

Un `Replica` recibe estados sellados con su `t` y da la pose a cualquier tiempo:

- **En el mundo (marco 0) se dibuja en el presente**: extrapolado desde el último estado con su
  velocidad y su aceleración (la de los dos últimos estados: gravedad y empuje por igual; un
  astronauta con los pies en el suelo, ninguna). Así una nave remota, una caja que tú lanzas desde
  ella y tú mismo coincidís a 1600 m/s; dibujada 120 ms en el pasado estaría 190 m detrás.
- **Dentro de un anfitrión** (coordenadas de nave) se dibuja `hostDelay` (120 ms) en el pasado,
  entre dos estados (Hermite): números pequeños y la nave misma ya está en el presente.
- Si un estado nuevo contradice lo que se estaba dibujando (una maniobra imprevista, un cambio de
  marco), la diferencia **se funde** en `smooth` segundos; solo un error enorme (más de 30 m o de
  ¼ s de su velocidad) se toma como salto.
- `seed(t, pose)`: empezar desde una pose conocida sin fundir (el piloto se levanta: su última pose
  local es la primera del flujo).

Usos: naves (`client/net/posePlayback.ts`), otros astronautas (`client/net/remotePlayer.ts`), cajas
ajenas (`client/cargo/crates.ts`) y, en el servidor, las naves que pilota un cliente (sus informes
llevados al presente del servidor: comprobaciones de alcance y relevo sin salto atrás).

Disparos ajenos: `Projectiles.spawn(..., ahead)` los vuela hacia delante `ahora − t` en pasos fijos.

## 4. Marcos: cambiar de marco sin que se note (`shared/frames/track.ts`)

Todo lo dibujado entre pasos guarda un `PoseTrack` (sus dos últimos pasos en su marco) en vez de
arrays propios:

- `push(...)` al final de cada paso con la pose alcanzada.
- `carry(de, a, fr)` al cambiar de marco: **los dos pasos** se llevan, cada uno con las poses de los
  marcos de **su** paso (`FramePair`: `pose` y `prev`). Lo dibujado sigue igual.
- `snap(p, q)` solo para un salto real (teletransporte, dejarlo sobre la silueta, asentarlo en el suelo).
- `at(α)` / `quatAt(α)` para dibujar, compuesto con la pose dibujada del marco (`Frames.pose(fr, true)`).

**Cuándo** se cambia de marco: al final del movimiento del paso, con todos los marcos y todos los
cuerpos en el mismo instante. El orden del paso fijo en `game.ts`:

| Orden | Sistema | Qué |
|---|---|---|
| 8 | `flight` | `stepClock.step()`; los marcos se mueven: burbuja, naves (propias o réplicas); objetivos cinemáticos de lo ajeno |
| 10 | `physics` | los cuerpos alcanzan a los marcos (Rapier); `crates.afterStep`: historia, **cambios de marco de objetos**, reposo, red |
| 20 | `player` | astronauta; su cambio de marco (`setFrame` lleva también el paso anterior) |
| 30 | `debris` | escombros: historia |
| 35 | `frames` | **la burbuja se recoloca aquí** (`followBubble`): residentes, historia, cascos exteriores, suelo |
| 40 | `projectiles` | el núcleo balístico (su cambio de marco ya lleva `prev` con las poses anteriores) |

La **burbuja** (`client/frames/bubble.ts`) al recolocarse define también dónde habría estado el paso
anterior (`prev` = pose − v·paso) y guarda `from`/`fromPrev`: `Frames.rebaseTrack` y
`Frames.rebasePrev` llevan cualquier historia al nuevo marco.

## 5. A qué marco pertenece un objeto suelto (`shared/frames/membership.ts`)

Una regla para todo objeto suelto (hechos de la física de cada lado, decisión en un sitio):

- **Entra en un anfitrión** si está en uno de sus compartimentos (en el acto), o si reposa sobre su
  exterior: apoyado en su estructura, a menos de `captureSpeed` (1,5 m/s) respecto a ella y sin haber
  cambiado de marco hace menos de `dwell` (0,25 s). Una caja lanzada que roza el casco no se recaptura.
- **Sale** en cuanto no está en un compartimento y nada del anfitrión la sostiene (o toca el suelo del
  cuerpo): hereda la velocidad de la nave en ese instante y no la arrastra un giro.
- «Cerca de una nave» es su propia caja (`clearOfHull`, de `def.bounds`), no un radio fijo.

## 6. Nada que dependa del marco en que se simula

Un marco puede moverse a cualquier velocidad (una nave, la burbuja en órbita, el suelo bajo una nave
que pasa a 250 m/s). Lo que se simula en él no puede notar esa velocidad:

- **Sin amortiguación lineal** en objetos sueltos (`DEFAULT_OBJECT_PHYSICS`): frenaría contra el marco.
  El rozamiento del aire lo pone el aire donde lo hay (`airPull`). Contacto y amortiguación por tipo
  en el catálogo (`defineObject({ physics })`); la velocidad de lanzamiento, también (`throwSpeed`, o
  de su masa con `THROW_IMPULSE`).
- **Control en el aire del astronauta**: empuja en la dirección pedida hasta su velocidad, nunca frena
  lo que ya traía; la mochila no recorta una velocidad heredada.
- **Cámara en tercera persona**: su suavizado es el del giro alrededor del astronauta, en los ejes de
  su marco; nunca el de su movimiento.
- **Partículas**: `gravity` es la fracción de la gravedad local del cuerpo (1 = cae como todo allí).
- **Compensador inercial**: su peso de cubierta es un dato suyo (`DECK_GRAVITY`), no la gravedad lunar.

## 7. Medir

- **`npm run test:motion`** (`tools/motion/check.ts`): servidor con temporizador irregular, red con
  latencia variable y paquetes desordenados, cliente a 144 Hz con tirones; mide el **salto por
  fotograma** (distancia a la continuación a velocidad constante de los dos fotogramas anteriores,
  frente a una cámara que se mueve suave) del sistema antiguo y del nuevo a 250 y 1600 m/s; el relevo
  del piloto; salida por la puerta con la nave girando; burbuja recolocada; cambio de marco con Rapier;
  regla de pertenencia; disparos tardíos; tiempos en el códec. Sale con código 1 si algo falla;
  `--verbose` imprime los números.
- **`npm run test:motion:net`** (`tools/motion/net.ts`): el servidor real y dos clientes sin
  navegador; sellos de paso de piloto y servidor, y relevo sin salto a 1600 m/s.
- **F7 en el juego** (`client/diag/motionProbe.ts`): la misma medida para todo lo que se ve (naves,
  cajas, proyectiles, astronautas, tú en tercera persona) con su gráfica (escala log, 1 mm … 10 m;
  línea roja a 5 cm), y los eventos que pueden causar un salto: cambios de marco (tú, cada caja, cada
  proyectil, con la regla que lo movió), recolocaciones de la burbuja, correcciones de red (máxima y
  saltos por nave y astronauta), el reloj de pasos (error, saltos). `game.motion.report()` desde la
  consola o Playwright (`game.motion.enabled = true` para grabar sin panel).

## 8. Algo nuevo que se mueve

| Quiero… | Hago |
|---|---|
| Dibujarlo entre pasos | Un `PoseTrack`: `push` al final del paso, `at(α)` al dibujar |
| Que cambie de marco | Al final del paso (tras `physics`), su estado con `Frames.transfer` y su historia con `Frames.carryTrack` |
| Que viva en la burbuja | Llevar cuerpo e historia en `followBubble` (`Frames.rebase` + `rebaseTrack`) |
| Enviarlo por red | Sellar con `stepClock.t` del paso de su estado; al recibir, un `Replica` y muestrearlo a `stepClock.t` (paso fijo) o `renderTime(α)` (fotograma) |
| Que sea un objeto suelto | `defineObject` (+ `physics`, `throwSpeed` si no valen los de por defecto): la maquinaria y la regla de marco son las de todos |
| Comprobarlo | Que aparezca en F7 (`Game.motionSubjects`) y, si es un mecanismo nuevo, un caso en `tools/motion/check.ts` |
