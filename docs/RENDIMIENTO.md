# Rendimiento: calidad automática

El juego tiene que ir fluido en un portátil con gráfica integrada, no solo en una tarjeta dedicada.
Hay dos piezas, ambas en `src/client/render/`, que se comprueban con `npm run test:render`.

## 1. El perfil por defecto sigue a la gráfica (`gpuTier.ts`)

El menú pregunta a WebGL cómo se llama la GPU (`WEBGL_debug_renderer_info`).

- **BAJA**:
  - integradas: Intel UHD, Iris Xe e «Intel Graphics»; las APU de AMD («Radeon(TM) Graphics», Vega
    N); Apple; móviles;
  - por software: SwiftShader, llvmpipe;
  - cualquier equipo con 4 núcleos o menos.
- **ALTA**: dedicadas (NVIDIA, AMD RX, Intel Arc).

Antes solo se miraban los núcleos de la CPU, y un portátil moderno con 8-12 hilos y gráfica integrada
recibía ALTA: MSAA 4×, resolución hasta 1,5×, 3 cascadas de sombra de 2048 y 220 m de sombras.

Si el jugador elige un perfil en el menú, se respeta (`selene.qualityPicked`); si no, manda la
detección. El nombre de la GPU aparece al pasar el ratón sobre el selector.

## 2. El regulador mantiene el ritmo (`governor.ts`)

- **Qué mide.** En cada fotograma mira cuánto duró (el intervalo de la pantalla), cuánto tardó la CPU
  y cuánto la GPU. El tiempo de la GPU sale de un temporizador (`EXT_disjoint_timer_query_webgl2`) si
  el navegador lo da; si no, se estima como intervalo menos CPU.
- **Cuándo baja.** Si en una ventana de 2 s la mediana pasa de 1/60 s en más de un 15 % y el cuello es
  la GPU, baja la imagen un escalón.
- **Cuándo no toca nada.** Si el cuello es la CPU: menos píxeles no ayudarían.
- **Cuándo sube.** Si sobra margen, sube un escalón.
  - Con temporizador, solo lo hace si la GPU va por debajo del 55 % del fotograma.
  - Sin temporizador, tantea. Si no aguanta, vuelve a bajar y espera el doble antes del siguiente
    intento.

Tras bajar espera 15 s antes de subir, y los primeros 8 s de partida no cuentan
(compilación de shaders, llegada del terreno).

**Escalones**, de la pérdida menos visible a la más visible:

| # | Resolución | MSAA | Bloom |
|---|---|---|---|
| 0 | 100 % | sí (perfil ALTA) | sí |
| 1 | 100 % | no | sí |
| 2 | 85 % | no | sí |
| 3 | 70 % | no | sí |
| 4 | 70 % | no | no |
| 5 | 60 % | no | no |
| 6 | 50 % | no | no |

El perfil BAJA empieza en el escalón 1, porque no tiene MSAA. La resolución se aplica justo antes de
dibujar (sin fotograma en negro); MSAA y bloom, al momento.

**F3 → «calidad automática»** muestra el escalón, la resolución, si hay MSAA y bloom, y si el tiempo
de GPU es medido o estimado. `?nodynres` lo desactiva: la imagen queda como diga el perfil.

## 3. Llamadas de dibujo, perfil BAJA y tirones

Las cifras son las de `npm run perf`, en el punto de aparición y como máximo (sin contar lo que las
cascadas descartan por quedar fuera de su volumen).

| Qué | Antes | Ahora |
|---|---|---|
| Terreno, BAJA | 422 nodos + 340 × 2 cascadas = 1121 | 167 + 68 × 1 = 242 |
| Terreno, ALTA | 422 + 340 × 3 = 1461 | 167 + 124 × 3 = 546 |
| Cada astronauta lejano, BAJA | 23 + 23 × 2 = 69 | 12 + 12 × 1 = 24 |
| Las tres armas que lleva cada jugador | 36 piezas (y sus sombras) | 8 |
| Naves (las tres), sombra en BAJA | 221 × 2 | 221 × 1 |

El fotograma entero en la base (BAJA, pasada principal + sombra; `npm run perf frame`):

| Mirando | Antes | Ahora |
|---|---|---|
| al cielo | 354 (43 + 311) | 155 (43 + 112) |
| a la nave más cercana | 695 (384 + 311) | 614 (337 + 277) |

Incluye las naves, el terreno y siete astronautas. No incluye el cielo, las partículas, las cajas, las
rocas, las armas ni las pasadas de posproceso.

- **Terreno** (`world/terrainGrid.ts`): los nodos son de 56 × 56 celdas y se dividen a 1,25 anchos, en
  lugar de 32 × 32 a 2,2. La celda se ve igual (0,0143 frente a 0,0142 rad). Como el número de nodos va
  con el cuadrado de la división, hay un 60 % menos de nodos. A cambio hay un 20 % más de triángulos a
  la vista y los mismos en la sombra. La celda más fina, junto a los pies, pasa de 0,33 m a 0,37 m (el
  objetivo es menos de 0,4 m). Ningún vecino queda a más de un nivel, que es lo que exige el fundido
  entre niveles. `perf` mide la media de ocho direcciones de la vista, y `test:terrain` comprueba las
  costuras con las dos rejillas.

  Los nodos proyectan sombra solo hasta donde llegan las cascadas del perfil
  (`TERRAIN_SHADOW_RANGE`: 260 m en ALTA, 110 m en BAJA). Los que llegan de los trabajadores entran
  en la escena unos pocos por fotograma (al menos 2, y más mientras no se pasen 2 ms), para que una
  tanda no sea un tirón.
- **Sombras que no caen en la vista** (`render/shadowCull.ts`): las cascadas se ajustan a lo cercano
  mire uno donde mire. Por eso, al mirar al cielo, todo lo que hay alrededor volvía a dibujarse en la
  sombra aunque ninguna de sus sombras se viera. Antes de cada fotograma se barre la esfera de cada
  proyector 300 m en la dirección de la luz. Si ese barrido no toca la vista, el proyector se queda
  fuera de la pasada de sombra de ese fotograma. Mirando al cielo, la sombra pasa de 311 a 112
  llamadas. Mirando a la nave no cambia nada, porque las sombras caen donde se mira.

  Las mallas instanciadas se quedan como están, porque ya descartan sus instancias ellas mismas. F3 →
  **«sombras fuera»** dice cuántos proyectores quedaron fuera de cuántos, y `?noshcull` lo desactiva.
- **Sombra por cascada** (`render/shadowCull.ts`, `useCascades`): three dibuja cada proyector cercano
  en todas las cascadas en cuyo volumen cae, que con ALTA suele ser las tres, aunque su sombra solo
  aterrice en una. Ahora el mismo barrido se hace por cascada, contra su tramo de la vista, y el
  proyector solo entra en las cascadas cuyo tramo alcanza su sombra (capas 20-23). El alcance es
  unas cuatro veces su radio más 20 m, porque el sol está bajo. `?nocascull` lo desactiva.
- **Astronauta lejano** (`player/astronaut.ts`, `buildFarSuit`): a partir del LOD 1 (12 m), las piezas
  que quedan se funden por acabado (mate y brillo) en dos mallas con esqueleto. La geometría se
  comparte entre todos los astronautas y los colores salen de una paleta de cada uno (su franja). El
  visor se queda aparte. Pasa de 13 llamadas a 4, y de 13 proyectores a 4 por cascada.
- **Cajas** (`cargo/crates.ts`): ya eran instancias por aspecto, pero sin recorte, así que se dibujaban
  siempre y en todas las cascadas. Ahora cada aspecto tiene una esfera alrededor de sus cajas vivas,
  recalculada en cada fotograma, y se recorta como una malla más.
- **Naves lejanas y muchas naves** (`npx tsx tools/perf/fleet.ts 10 30 60 100`):
  - a más de 300 m, una nave es una sola malla horneada, compartida por tipo y estado;
  - solo tiene colisiones en el mundo lunar a menos de 400 m del jugador;
  - aparcada, se duerme aunque resbale unos milímetros.

  Con 100 naves y 100 NPC, el cliente pasa de 2184 llamadas y ~42 ms de CPU a 482 llamadas y ~13 ms.
- **Armas** (`fx/weapons.ts`, `mergeByMaterial`): un arma es rígida, así que sus piezas se funden en una
  malla por material. El lanzador pasa de 11 piezas a 3, la soldadora de 15 a 3 y el fusil de 10 a 2.
  Cada jugador lleva las tres (en la mano o a la espalda), así que dibuja 28 piezas menos, y otras
  tantas sombras.
- **Astronautas** (`player/astronaut.ts`, `DETAIL_MATERIALS`): las piezas de detalle (pomos, parche,
  correas, bolsillos, manguera, suelas…) no proyectan sombra y no se dibujan más allá de 12 m. El
  cuerpo, el casco, el visor, los guantes y las botas se quedan siempre.
- **Perfil BAJA** (`world/lighting.ts`, `render/pipeline.ts`):
  - una sola cascada de sombra de 2048 que cubre 90 m, en lugar de dos de 1024 hasta 140 m: una
    pasada de sombra en vez de dos y sombras más nítidas cerca;
  - las sombras horneadas del suelo toman el relevo entre 50 y 80 m;
  - **SMAA** después del mapeo de tonos, en lugar de ningún suavizado. También se activa en ALTA
    cuando el regulador quita el MSAA; `?noaa` lo desactiva;
  - resolución de partida de hasta 1,25×, porque una pantalla escalada al 125 % se veía borrosa a 1×.
    El regulador la baja si la GPU no llega.
- **Tirones**: F3 → **«pico (5 s)»** muestra el peor fotograma de los últimos 5 s, cuánto de él fue
  CPU y los sistemas que más tardaron en él, con su render (`engine/spikes.ts`). Si ningún sistema es
  lento pero el fotograma sí, ha sido basura (GC) o la GPU. El regulador espera 15 s antes de volver a
  subir, porque cada cambio reserva búferes y da un tirón.

## 4. Medir

- **`npm run test:render`** (`tools/perf/render.ts`):
  - la detección, con cadenas de GPU reales;
  - las armas fundidas: una pieza por material, el mismo volumen y la misma sombra;
  - el recorte de sombras: mirando al frente queda fuera lo que está detrás y entra lo que, estando
    fuera de la vista, proyecta dentro de ella; mirando al cielo queda fuera todo; y después vuelve
    todo como estaba;
  - el regulador contra un modelo de fotograma, en el que el tiempo de GPU crece con los píxeles, el
    MSAA y el bloom, y la pantalla espera al vsync. Casos: limitado por la GPU (baja hasta que cabe y
    se queda), limitado por la CPU (no toca nada), equipo rápido (nada), sin temporizador (tanteos
    cada vez más espaciados) y con margen otra vez (vuelve arriba);
  - el registro de picos.
- **`npm run perf`**: el coste de las naves, las rocas, el terreno (nodos, sombras, salto de nivel), los
  astronautas, el fotograma entero en la base (`frame`: principal y sombra, con y sin recorte) y la
  red, sin navegador. Se puede pedir solo una parte, por ejemplo `npx tsx tools/perf/perf.ts frame`.
  `GRIDS=48x1.46,64x1.1` añade rejillas de terreno a la comparación. Las rocas se cuentan con sus
  reglas reales:
  - nivel de detalle por distancia;
  - las demasiado pequeñas para verse no se dibujan;
  - quién proyecta sombra, en cada cascada.

  En el punto de aparición se dibujan 106 de 1446 rocas, unos 10 K triángulos. La cifra de 4,16 M de
  antes no aplicaba esas reglas.
- **F3 en el juego**: llamadas de dibujo, triángulos, milisegundos por sistema y la calidad automática.

## 5. `npm run dev` frente a `npm run play`

El túnel (`npm run share`) sirve lo que corra en :3000.

- **`npm run dev`**: Vite sirve módulos sueltos, sin empaquetar ni minificar y con HMR. La primera
  carga es mucho más lenta, sobre todo por el túnel (cientos de peticiones). En marcha, el código
  que se ejecuta es el mismo, así que los FPS casi no cambian.
- **`npm run play`**: construye el paquete de producción y lo sirve. Es lo que conviene para medir y
  para jugar por el túnel.

## 6. Siguientes candidatos (sin medir en una GPU real)

- **Profundidad logarítmica** (la de por defecto): escribe la profundidad en cada fragmento, lo que
  anula el descarte temprano y encarece cada píxel dibujado encima de otro. Con `?rdepth` se usa Z
  invertida, que conserva ese descarte. Probar si da FPS y se ve bien (sombras, cielo, partículas)
  antes de hacerla la opción por defecto.
- **El terreno en una sola llamada.** Hoy no compensa hacerlo con `BatchedMesh`:
  - habría que reservar de antemano los búferes de todos los nodos posibles (unos 150 MB), porque cada
    nodo trae sus propias alturas;
  - el shader usa `modelMatrix` y `gl_VertexID`, este último para los faldones;
  - que un nodo proyecte sombra o no, no se puede decidir por instancia.

  El camino bueno es otro renderizador de terreno: una sola rejilla fija e instanciada, con las
  alturas y normales de cada nodo en una textura (una capa por nodo). Serían 1-2 llamadas más una
  por cascada, en lugar de las 242 que quedan en BAJA. Es una reescritura de `sphereTerrain.ts` y de
  su trabajador que hay que ver en pantalla.
- **Naves**: 100-136 llamadas por nave, y 221 proyectores entre las tres. Ya funden los muebles por
  sala, material y sombra, solo dan sombra los de salas con ventana y dejan de dibujar el interior a
  más de 40 m. De los 221 proyectores, solo unos 30 son pequeños (menos de 0,5 m y lejos), y 53 son
  piezas de más de 5 m. Quitar las sombras pequeñas ahorra poco. Lo que ahorraría más:
  - que los muebles de dentro (unos 40 proyectores entre las tres naves) solo den sombra con el
    jugador dentro de esa nave;
  - fundir en instancias las piezas exteriores animadas (paneles solares, radiadores, tren de
    aterrizaje, rampa, torreta).
