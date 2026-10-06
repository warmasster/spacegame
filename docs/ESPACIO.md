# Salida al espacio

Primera versión del objetivo «salir al espacio sin cargas»: despegar de la base, llegar a órbita
alrededor de una Luna esférica y volver, todo en la misma partida.

## El modelo

- **Cuerpos celestes como datos** (`src/shared/space/body.ts`): radio, μ = g·R² (el peso en la base
  es exactamente `MOON.gravity`), centro, eje del polo y su superficie (`SurfaceDef`). La Luna tiene
  su centro justo debajo del origen del mundo (`[0, −R, 0]`), con la base en esa vertical y su suelo
  en y = 0, y el polo a 20° sobre el horizonte norte de la base.
- **Gravedad radial** μ/r² hacia el centro, y un **marco local** en cualquier punto: arriba (lejos
  del centro), norte (hacia el polo, sobre el horizonte) y este. En la base coincide con los ejes
  del mundo (y arriba, −z norte, +x este), pero nada depende de ello.
  - 1,62 m/s² en la superficie, ~1,45 a 100 km.
  - Sin atmósfera no hay velocidad límite: dejada caer desde parada, la nave llega al suelo desde
    10 m a ~5,7 m/s y desde 100 km a ~550 m/s, tras ~6 min.
- **Un solo suelo por cuerpo** (ver «El suelo» más abajo): el relieve global procedural más los
  modificadores de terreno. La base no es un mundo aparte: es un sitio que aplana su campo y sus
  pistas sobre ese mismo suelo. Rocas, cráteres de explosión, colisiones y el mismo nivel de detalle
  existen en cualquier punto: se puede aterrizar, bajar a pie y disparar en cualquier sitio.
- **Cuerpo dominante** (`bodyAt(p)`): todo lo que necesita «el cuerpo» (gravedad, órbita, altura,
  piloto automático, suelo) se lo pide a `bodyAt` en vez de usar la Luna a mano. Hoy solo hay uno;
  un cuerpo nuevo es una entrada en `BODIES`.

## El suelo (`shared/space/`)

Toda la superficie de un cuerpo es **una** función determinista (cliente, workers y servidor ven el
mismo suelo sin guardarlo ni enviarlo):

- **Relieve global** (`surface.ts`, `BodySurface`): tierras altas, mares, cordilleras dispersas,
  ondulación hasta ~1 m y cráteres a todas las escalas (de 170 km a 2 m). Es un dato por cuerpo (`SurfaceDef`, en
  `BODIES`) y la **semilla del mundo** lo varía entero. El nivel de referencia de cada mundo es el
  suelo natural de su sitio de inicio (`bareSurface`): queda sobre la esfera media, así que la base
  está en el origen del mundo (y = 0) y allí el peso es exactamente la gravedad del cuerpo, sea
  cual sea la semilla; nada depende de ello. Los cráteres procedurales tienen una
  candidata por celda del cubo-esfera (`cubeSphere.ts`); un punto mira las 3×3 celdas de su cara y
  las de la cara vecina cerca de una arista, así que ningún cráter se corta (sin costuras).
  Las cordilleras son `SurfaceDef.mountains`: ruido de crestas a tres escalas, atenuado en los mares;
  la colisión y el servidor usan la misma altura y los límites de culling incluyen sus cumbres.
  El albedo descarta ruido más fino que la celda del LOD para evitar cuadros a distancia.
  El detalle visual usa `regolith_macro.png` (RGB, generado con `python tools/textures/macro.py`):
  dos canales de deformación suave de coordenadas y uno de variación entre 20 m y 2,56 km. Se calcula
  una vez, tiene mipmaps y cuesta una consulta de textura; todos los nodos comparten el mismo periodo.
  La penumbra de las sombras precalculadas es angular y constante: no oscurece parches por su LOD.
- **Modificadores de terreno** (`terrainMods/`): datos serializables
  `{ kind, body, center (dirección unitaria), radius, params, yaw?, seed? }`. Cada tipo es una
  función pura (`kinds.ts`, registro `MOD_KINDS`) que recibe la muestra del suelo (altura, albedo,
  densidad de rocas, suelo trabajado) y la devuelve modificada:
  - `flatten`: explanada con borde suave (campo de la base, pistas);
  - `crater`: explosiones y cráteres puestos a mano;
  - `trench` (zanja) y `ramp` (rampa entre dos alturas);
  - un parámetro `NaN` significa «el suelo que hay aquí»: se mide al colocarlo (`BodySurface.addMod`),
    con los modificadores anteriores ya puestos, y a partir de ahí es un número fijo.
- **Almacén e índice** (`store.ts`, `TerrainMods`): cada superficie guarda dos capas, en orden:
  - estática: la de los sitios, que cada máquina reconstruye desde la semilla;
  - dinámica: los cráteres de la partida, que guarda el servidor, acotada (4.096) y compactada: una
    explosión encima de un cráter del mismo tamaño lo ahonda en vez de añadir otro, y al pasar del
    máximo se van los más viejos por lotes.
  - Índice espacial sobre la esfera: cada modificador se archiva en las celdas de un nivel del
    cubo-esfera al menos 4 veces mayores que su alcance; una muestra consulta una celda por nivel
    usado. Sin basura al muestrear; funciona igual en workers, cliente y servidor.
  - Los detalles más pequeños que la resolución de una muestra se omiten (como el relieve
    procedural): un nodo lejano no ve un cráter de 2 m.
  - `angle.ts` mide separaciones con `atan2(|a×b|, a·b)`, sin arrays temporales. La dirección
    serializada es redondeada y no tiene longitud exactamente 1: no se puede usar `acos(a·b)`
    para fusionar cráteres o invalidar nodos. `modTouches` comparte la prueba de alcance entre
    mallas y rocas, y `TerrainMods.near` usa el mismo ángulo al preparar listas para workers.
- **Sitios** (`sites.ts`): lugares como datos que emiten modificadores. Un tipo de sitio es una
  entrada en `SITE_KINDS` (hoy `base`: campo aplanado, pistas, puntos de aparición, cajas en el
  suelo; `crater`: un cráter puesto a mano). La posición es una dirección o un punto del marco de
  otro sitio. Los generadores procedurales (asentamientos, caminos) irán en `SITE_GENERATORS`:
  funciones `(cuerpo, semilla) → sitios` que todas las máquinas ejecutan igual.
- **Marcos tangentes** (`tangent.ts`): `TangentFrame` (x este, y arriba, z sur, en cualquier punto)
  y `SurfaceGround` (el suelo visto desde un marco como un campo de alturas). Con ellos se colocan
  naves (`placeShip`), cajas y personas en cualquier sitio y trabaja el contacto de las naves.
- **Rocas** (`rocks.ts`): una candidata por celda del cubo-esfera para cada clase de tamaño
  (`SurfaceDef.rocks`), más densas sobre eyecta brillante y ninguna donde un modificador limpió el
  suelo. Se sirven por baldosas (celdas del nivel de la clase más gruesa, ~83 m en la Luna).
- **Preguntar por el suelo**: `heightAboveGround(cuerpo, p, superficie)` (en el cliente,
  `game.groundAlt`) y la superficie de un cuerpo es `surfaceOf(cuerpo, semilla)` (en el cliente,
  `game.surfaces`).

## El cielo (`client/world/sky.ts`)

El fondo usa direcciones celestes y cubre los 360°, sin depender del origen flotante ni del tamaño
del terreno. La Vía Láctea tiene mipmaps, polvo oscuro y una variación de color tenue. Los cometas
lejanos reutilizan un solo cuadrilátero de dos triángulos: aparecen cada varios minutos, se desvanecen
suavemente y sus colas apuntan en dirección opuesta al Sol. No generan partículas, luces ni mallas
nuevas por frame; el sistema `sky` los actualiza después de la cámara.

## El vuelo (`shared/ship/flight`)

- El ordenador de vuelo recibe la pose **en el marco local** (arriba = +y), así que nivelar, el
  rumbo, la altura y el estacionario funcionan en cualquier punto alrededor de la Luna.
- Lo que carga la nave es la **gravedad efectiva** `g − v²/r`: a velocidad orbital es cero y los
  propulsores de sustentación dejan de trabajar solos.
- **Régimen orbital** (más de 150 m/s horizontales o más de 15 km de altura, con histéresis):
  - el vuelo acoplado deja de retener posición y altura (frenaría la órbita);
  - el acelerador manda en el motor principal;
  - los modos de superficie del piloto automático se anulan (menos NIVEL); mandan las caras ÓRBITA
    y ESPACIO;
  - la retención de actitud es inercial;
  - en órbita cerrada los propulsores no sostienen el peso salvo que se pida una velocidad vertical
    (sostenerlo deformaría la órbita).
- El piloto automático solo gestiona el motor principal si vuela una velocidad o una altura
  (NIVEL o RUMBO solos le dejan el acelerador al piloto).
- Los motores principales del catálogo tienen un impulso específico de ~6.100 s, que da para subir
  y bajar. Δv con depósitos llenos: Selene ~7,3 km/s, Albatros ~7,2 km/s, Peregrina ~3,5 km/s
  (la Peregrina llega justa a órbita y no vuelve).

### Piloto automático: el tambor

El panel del piloto automático es un **tambor de tres caras** que se gira con GIRAR (`ap.face`).
Solo se pueden pulsar los mandos de la cara que queda fuera; los modos conectados siguen funcionando
aunque su cara esté dentro. Los mandos que no se pueden usar ahora se encienden en **ámbar**:
- los modos de superficie, en régimen orbital;
- el acelerador, mientras una maniobra orbital lleva el motor.

| Cara | Mandos |
|---|---|
| SUPERFICIE | los de siempre: NIVEL, ALTURA, RUMBO, VELOC., NAV, DESPEG., ATERRIZ. y sus selectores |
| ÓRBITA | **SUBIR** (a la órbita del selector ÓRBITA, 15–120 km), **CIRCUL.** (redondea la órbita a la altura actual), **BAJAR** (vuelta a la base), **BAJ.AQUÍ** (frena y baja donde estés), **CRUCERO** (velocidad horizontal del selector CRUCERO a altura fija), los selectores ÓRBITA y CRUCERO y **SOBREPOT.** (bajo tapa) |
| ESPACIO | apuntar el morro: PROGR., RETRÓG., RADIAL, NADIR, NORMAL, ANTINOR. (el acelerador sigue siendo del piloto); **VELOC.** (velocidad respecto al cuerpo dominante, en inercia) y el selector CRUCERO |

El manual de cada nave tiene la sección «Órbita y espacio» (`SPACE_MANUAL` en `ships/kit.ts`), que
explica cómo encaja todo: regímenes, las tres caras, el crucero de cada una, las tres formas de
volver al suelo y por qué unos motores desiguales hacen girar la nave.

- **SUBIR:**
  - despega con los propulsores de sustentación y sube el tren;
  - a partir de 150 m quema el motor principal repartiendo su empuje entre subir (y sostener el
    peso) y ganar velocidad horizontal;
  - al llegar a la velocidad orbital pasa a CIRCUL., que se desconecta sola al redondear la órbita.
- **BAJAR:**
  - espera en órbita a que la base quede delante justo a la distancia de frenado; si acaba de
    pasarla, es una vuelta entera, ~1 h 50 min;
  - frena con el motor siguiendo un perfil que para en la base, y baja con un perfil de altura;
  - a 1,5 km y menos de 25 m/s gira el tambor a SUPERFICIE y deja NAV a BASE en estacionario a
    100 m. ATERRIZ. la posa.
- **SOBREPOT. (inyección de masa):**
  - a potencia de chorro fija, empuje × velocidad de escape es constante;
  - el doble de caudal da √2 = +41 % de empuje con −29 % de impulso específico;
  - calienta la cámara: se corta sola a 1.250 °C y no se rearma hasta bajar de 1.000 °C.
- Selene, en la simulación de `npm run test:ship`:
  - SUBIR a 20 km tarda 351 s y gasta 1.054 kg; con SOBREPOT., 261 s y 1.231 kg;
  - BAJAR desde 600 km antes de la base la deja encima en 650 s;
  - ida y vuelta completas dejan ~300 kg de margen.

### Cómo se sube a órbita a mano

1. Despegar y subir unos kilómetros.
2. Soltar los mandos en vuelo acoplado para que se quede quieta en altura.
3. Piloto automático con NIVEL.
4. Pasar a **desacoplado** y poner el acelerador a tope.
5. Al llegar a la velocidad circular (página VUELO: VELOCIDAD frente a CIRCULAR, periápside
   positiva), cortar el motor.

Desde 10 km, la Selene tarda ~280 s y gasta ~870 kg de sus 2.300 kg (prueba
«salida al espacio» de `npm run test:ship`).

Para volver:
1. Frenar con la nave girada 180°.
2. Bajar hacia la base con la ayuda de la línea BASE del HUD y de la página VUELO, que dan la
   distancia por la superficie y el rumbo.
3. Ya lento y bajo, el vuelo vuelve a ser el de siempre y ATERRIZ. funciona.

## Render

- **Terreno único** (`client/world/sphereTerrain.ts`, uno por cuerpo con superficie): seis
  quadtrees sobre las caras del cubo-esfera, nodos de 32×32 construidos en los workers
  (`terrain.worker.ts`, trabajo `sphere`) con los modificadores que les tocan.
  - Detalle hasta ~0,33 m por celda bajo la cámara (el nivel más profundo sale del radio del cuerpo).
  - Morph CDLOD entre niveles (cada nodo trae la rejilla de su padre: sin grietas ni saltos) y
    faldones para los saltos transitorios.
  - Material del regolito: texturas de detalle (2 m y 8 m girada) y macro (160 m y 1.280 m) con
    coordenadas en metros a lo largo de la cara, relativas a una esquina ajustada a 2.560 m (nunca
    posiciones absolutas en float32), fotometría Lommel–Seeliger con oposición, suelo trabajado más
    liso en pistas y explanadas.
  - Sombras de horizonte horneadas por vértice con el sol del momento (`setSun` rehace los nodos
    si el sol gira más de medio grado); de cerca mandan las cascadas: los nodos a menos de 260 m
    proyectan sombra.
  - Descarte por horizonte y por frustum; los workers construyen primero lo que se ve, lo cercano
    antes que lo lejano.
  - Un modificador nuevo (un cráter) rehace los nodos que alcanza y que son lo bastante finos para
    verlo; la malla vieja se queda hasta que llega la nueva.
- **Rocas** (`client/world/rocks.ts`): baldosas del cubo-esfera en anillos alrededor del jugador,
  `BatchedMesh` con culling y LOD por instancia; las instancias son relativas a un ancla cerca del
  jugador que cuelga de `origin.root` (nada en float32 absoluto).
- El plano lejano de la cámara llega al limbo de la Luna (de 60 km a miles de km).
- Las rocas y la física del suelo solo se generan cerca del suelo. Terreno, rocas, cielo y otras
  naves se ocultan cuando los portales dicen que no se ve el exterior.
- **Eclipse:** a la sombra de la Luna no hay sol (luz, relleno del entorno y paneles solares).
- **Origen flotante del render** (`client/render/origin.ts`): la GPU trabaja en float32 y a
  1.000 km del origen se equivoca en decímetros (el traje «reventaba», las cajas temblaban, el
  recorte del casco fallaba). La escena se dibuja relativa a un punto O cerca de la cámara (se
  recoloca cada kilómetro, en metros enteros):
  - todo lo que se coloca en coordenadas de mundo cuelga de `origin.root` (su `.position` sigue en
    mundo); la cámara también;
  - lo que se lee de three.js (`matrixWorld`, `getWorldPosition`, `localToWorld`, `lookAt`) está en
    **espacio de render**: la lógica convierte con `origin.toWorld` / `worldOf`, o lleva sus propias
    matrices de mundo (las naves: `ShipClient.worldMatrix`);
  - lo que escribe búferes de GPU a mano (cajas y escombros instanciados, partículas) escribe
    `mundo − O` y cuelga de la escena;
  - ningún shader usa coordenadas absolutas: las texturas del terreno van en metros de la cara
    relativos a su nodo y la vertical local se saca del centro del cuerpo relativo al origen;
  - la escena no recalcula matrices cada fotograma (`scene.matrixAutoUpdate = false`): lo estático
    no cuesta nada hasta que el origen se mueve.
- **Burbuja de física** (`client/frames/bubble.ts`): Rapier también es float32, así que el mundo
  de Rapier del exterior (marco 0: el astronauta a pie, cajas y escombros fuera de las naves, el
  casco exterior de las naves, el suelo) se tiende alrededor del jugador:
  - `ground`: cerca del suelo, en cualquier sitio (la base no es distinta): un marco tangente
    (x este, y arriba, z sur) sobre el suelo bajo el jugador, quieto; el suelo se genera ahí
    (`tangent` en los workers, con los modificadores y las rocas grandes) y se vuelve a tender cada
    6 km; un cráter nuevo rehace las baldosas que toca;
  - `space`: alto o rápido, un marco tangente que se mueve con el jugador (galileano): una caja que
    flota junto a una nave en órbita, el casco y el astronauta se mueven unos m/s en él, no 1,6 km/s.
  - Al recolocarla todo lo que hay dentro se lleva de un marco al otro sin cambiar su movimiento en
    el mundo; las baldosas de suelo que ya existían se llevan rígidas hasta que las nuevas las
    cubren (nunca falta suelo mientras trabajan los workers). La gravedad es la real y radial.
  - En la red el marco 0 es el mundo (cada cliente tiene su burbuja): las cajas, el estado del
    jugador y los disparos se convierten al enviar y al recibir.
- **Proyectiles** (cohetes, balas…: `shared/frames/ballistic.ts`, guía en [`EQUIPO.md`](EQUIPO.md)):
  salen con la velocidad del lanzador (dentro de una nave en órbita llevan sus 1,6 km/s), caen con
  la gravedad radial y chocan con el suelo donde esté (`heightAboveGround`). Disparado a bordo, vuela
  en el espacio de la nave mientras está dentro de un compartimento (sale justo de la boca tal como
  se dibuja, siente la gravedad aparente de la cabina y la cabina no puede adelantarlo); al salir
  por una puerta o una brecha pasa al mundo en un paso fijo, con la misma posición y velocidad, y al
  revés si entra en una nave. El disparo, el impacto y la explosión viajan por la red en el espacio
  de la nave (`fr`), así cada cliente lo pone donde tiene la nave. El que dispara lo lanza en el
  acto, sin esperar el eco del servidor.
- La red manda las posiciones en float64.

## A bordo

- La gravedad aparente usa la gravedad radial real: en órbita, con el compensador inercial
  apagado, todo flota. Encendido, el suelo sigue siendo «abajo».
- Se puede salir de la nave en cualquier sitio: posada lejos de la base se baja al mismo suelo que
  en la base (colisión, rocas y cráteres en toda la superficie); en órbita se sale a flotar a su
  lado, en caída libre con ella (EVA real en el marco 0, que viaja con el astronauta).
- Una caja que acaba bajo el suelo (lejos del jugador no hay colisión, o cae desde órbita) se deja
  sobre él, en reposo.

## Red

- `welcome` trae la capa dinámica de modificadores de todos los cuerpos (`mods`) y el punto de
  aparición en coordenadas de mundo, ya sobre el suelo; `explode` trae el cráter (`mod`) cuando la
  explosión fue en el suelo, en cualquier cuerpo. Cada cliente lo añade a su superficie en el orden
    del servidor (la fusión de cráteres repetidos da el mismo resultado en todos). Protocolo 15.
- Un cráter de explosión viaja sin parámetros (los valores por defecto del tipo `crater` son los de
  una explosión): ~100 B en JSON; 4.000 son ~420 KB al entrar.

## Uniones de parches y niveles de detalle

La transición CDLOD debe llevar todos los atributos a la misma superficie gruesa: posición,
normal, albedo, material y visibilidad solar. Antes la posición sí usaba el padre, pero la normal
era la del vértice fino par: en una comprobación real del worker el borde coincidía a 0,002 mm y
su iluminación saltaba casi 12°. Ahora ambas normales se calculan con la misma diferencia
central sobre la malla del nivel correspondiente, incluida una corona de muestras exterior.

El padre también aporta albedo, material y sombra. En los bordes la marcha cercana hacia el sol
consulta la superficie compartida: interpolar la malla local con el Jacobiano del centro daba
un horizonte distinto según el parche. Se amplía la selección e invalidación de modificadores
para cubrir la corona que necesitan las normales del padre.

Los faldones siguen tapando huecos transitorios de LOD, pero el shader de profundidad los
descarta: una pared auxiliar de un parche no debe proyectar una línea oscura sobre su vecino.
La regla vive en `client/world/terrainShader.ts`; desplazamiento de color y profundidad comparten
la misma fórmula. `terrain.worker.ts` exporta `buildTerrainJob` para medir los buffers reales sin
navegador. `npm run test:terrain` comprueba bordes de igual/distinto nivel y continuidad de sombras.
No sustituye una revisión visual de texturas o sombras en GPU.

## Otros sistemas estelares

La galaxia (`shared/space/galaxy.ts`) tiene 2000 sistemas. Los más cercanos al Sol tienen un cuerpo
en su propia región del mismo espacio del mundo, a 10⁸ km uno de otro: todo lo de arriba (suelo,
gravedad, vuelo, órbitas) funciona allí igual. Se llega con un salto (**J** a los mandos, en vuelo y a
más de 20 km del suelo: `shared/space/jump.ts`). Detalle en [`MUNDO.md`](MUNDO.md) §14.

## Límites de esta versión

- Las texturas de detalle del terreno tienen una costura a lo largo de las 12 aristas del cubo (las
  coordenadas son de cada cara). El relieve no la tiene.
- El sol está fijo (no hay ciclo de día): el mecanismo para rehornear las sombras con el sol que se
  mueva está (`SphereTerrain.setSun`), nada lo llama todavía. El cielo sigue orientado para la
  latitud de la base.
- Ningún generador procedural de sitios todavía (`SITE_GENERATORS` vacío).
- En otros sistemas el sol alumbra desde donde el del Sol, y los cuerpos no tienen sitios.
- No hay aceleración del tiempo: una órbita baja dura ~2 h reales, y BAJAR puede esperar casi eso.
- BAJAR solo vuelve a la base (no a otros puntos) y no corrige el plano de la órbita: si la órbita
  pasa lejos de la base, la corrección lateral se hace al frenar y cuesta más.
- Los motores encendidos al ralentí solo mantienen la llama piloto (0,5 % del caudal).
- Los puntos NAV son direcciones sobre el cuerpo (`flight/nav.ts`, salen de los sitios): NAV y la
  página de navegación miden rumbo y distancia por el círculo máximo, en cualquier sitio.
