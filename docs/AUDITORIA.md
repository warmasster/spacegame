# Auditoría técnica: rendimiento y sistemas cableados

> **Fecha:** 27-09-2026 · **Código revisado:** `main` + los cambios sin commitear de ese día.
> **Pregunta:** dónde hay optimizaciones serias (empezando por «dibujar solo lo que ve el jugador») y
> qué sistemas están cableados de forma que impiden llegar a la hoja de ruta: salir al espacio sin
> cargas, naves y NPCs con objetivos propios, combate por subsistemas, simulación por niveles de
> detalle, entidades comunes, persistencia, planetas esféricos, economía y facciones emergentes.
>
> Las cifras de §1 están **medidas** con scripts fuera del repo (Node 24 + V8, sin GPU; método en
> §11). Lo que pone *estimación* no se ha medido en el navegador.

---

## Estado de las optimizaciones (28-09-2026)

Aplicado (de las «12 que más rinden», §3.1 y §5). Sin verificar aún en el navegador con GPU: ver
«pendiente de comprobar en el juego» al final de esta sección.

| # | Qué | Dónde | Estado |
|---|---|---|---|
| 1 | F3 honesto: contadores del frame entero (`info.autoReset = false`), draws por categoría y pasada (escena/sombras), basura JS/s, subidas de textura/s | `render/pipeline.ts`, `engine/debug.ts` | Hecho |
| 2 | Rocas: `BatchedMesh` (proyectoras y no proyectoras), culling por instancia en la vista y en cada cascada, LOD de malla (720/320/80/20 triángulos) por distancia y tamaño, piedras sub-píxel ocultas, sombras solo de las grandes y cercanas, alta/baja por baldosa sin reconstruir | `world/rocks.ts` | Hecho |
| 3 | Portales: desde la sala de la cámara, cada portal abierto (puerta, trampilla, rampa, ventana, cristal interior, panel reventado) recorta la vista a su rectángulo en pantalla; solo se dibujan los muebles y máquinas de las salas alcanzadas, y el exterior (terreno, rocas, cielo, otras naves) solo si un portal hacia fuera está en pantalla. LOD de nave: sin interior a más de radio + 40 m, sin detalles exteriores pequeños a más de radio + 250 m | `ship/ship.ts` (`portalView`), `ship/view.ts` | Hecho |
| 4 | Sombras selectivas: mandos, piezas < 10 cm e interior de salas sin ventana no proyectan; 2 cascadas en calidad baja | `ship/view.ts`, `world/lighting.ts` | Hecho |
| 5 | Materiales: franja de librea como uniforme (un programa por estilo); lámpara de nave en `DataTexture` (sin límite de uniformes) | `ship/materials.ts` | Hecho. **Pendiente:** materiales compartidos + fusión por (zona × material) |
| 6 | `ShipView.update` sin basura: marcos de mando precalculados, solo se reescribe lo que cambia, lámparas por índice, `blocked()` a ~20 Hz y solo de cerca, matrices estáticas congeladas y raíz que no recalcula si la nave está quieta | `ship/view.ts`, `shared/ship/hold.ts` | Hecho |
| 7 | Vuelo: masa estática por `ShipDef`, «dormida» antes del trabajo, fase previa de contacto con caché de alturas, asignador e integración sin asignaciones | `shared/ship/flight/*` | Hecho |
| 8 | Pantallas: solo las legibles (≤ 9 m y de cara), cada una en su fase, fondo cacheado | `ship/screens.ts` | Hecho |
| 9 | Astronautas: esfera de culling fija; IK de agarre con búsqueda completa cada 8 frames; sin IK a más de 25 m; LOD de malla generado al cargar (agrupación de vértices: 89 K → ~12 K → ~2 K triángulos, mismos pesos de piel y materiales) a 12 y 35 m | `player/astronaut.ts`, `net/remotePlayer.ts` | Hecho. **Pendiente:** materiales en atlas |
| 10 | `reversedDepthBuffer` · resolución dinámica medida con temporizador de GPU (baja hasta 0,7 solo si la GPU no llega) · shaders compilados en la carga (`compileAsync`) | `render/pipeline.ts`, `game.ts` | Hecho (`?rdepth` opcional, `?nodynres` la apaga) |
| 11 | Pool de luces reales (4 focos + 2 puntuales) para cascos, focos de aterrizaje y explosiones | `render/lightPool.ts` | Hecho |
| 12 | Red: binario para estados de jugador, instantáneas, poses de nave, informes de vuelo y diferencias de estado (`shared/wire.ts`, protocolo 8); interés por zona (estado, interruptores, paneles y mensajes de una nave solo a quien está a < 1,5 km o a bordo, con estado completo al entrar; poses y jugadores lejanos a 1–2 Hz; cajas en movimiento a < 600 m); cada mensaje se serializa una vez | `server/room.ts`, `net/netClient.ts`, `shared/wire.ts` | Hecho |

Además (§3.1, §5): `ShipSim.tick` sin basura (tick reutilizable, propelente con grafo de enteros y
alcance cacheado por estado de válvulas, red eléctrica, atmósfera, soporte vital y descompresión sin
asignaciones, `circuitLive`/`conduitCut` precalculados); ediciones del terreno en rejilla espacial;
plataformas de aterrizaje aplanadas; `explode()` descarta naves fuera de alcance; sistemas a 5 Hz en
naves aparcadas sin nadie a 400 m (servidor); física interior de una nave solo si hay alguien, algo
despierto o una puerta/rampa/tren en movimiento; `Frames.ship()` O(1); cajas instanciadas por aspecto
y `Map` por id; partículas con lista de vivas y subida parcial; HUD sin `innerHTML` por frame; fuga de
geometría de los cohetes; terreno con claves numéricas, sin `Set` por frame y generación priorizada
por el frustum; `pick()` de la mira con descarte por esfera; geometría estática de las naves
indexada (estructura, decoración, consolas, lámparas, mobiliario); sombra solar horneada del terreno
con ~2,7× menos muestras; `npm run perf`.

**Fase 2, primer paso (salida al espacio):** gravedad radial, marco local y régimen orbital en el
vuelo, Luna esférica lejana, datos de órbita en HUD y pantallas, eclipse, suelo esférico fuera del
parche. Detalle y límites en [`ESPACIO.md`](ESPACIO.md).

**Fase 2, segundo paso (29-09-2026): un solo suelo por cuerpo.** El parche plano de la base
(`LunarTerrain`, 15 km, el blend `site`, `SURFACE_PATCH`, el modo `base` de la burbuja) ya no
existe: toda la superficie es `BodySurface` = relieve global + **modificadores de terreno**
(`shared/space/terrainMods/`: datos serializables, tipos como funciones puras, índice espacial en
el cubo-esfera, capa estática de los sitios y dinámica de la partida, acotada y compactada). La base
es un **sitio** (`shared/space/sites.ts`) que aplana su campo y sus pistas. Un solo renderizador
(`sphereTerrain.ts`) con el material, el morph, los faldones y las sombras de horizonte de la base
en todas partes; rocas, cráteres y colisiones (solo el trabajo `tangent`) en cualquier punto;
naves, cajas, aparición y NAV colocados sobre el suelo global con marcos tangentes; protocolo 11
(`TerrainMod` en `welcome` y `explode`). Detalle en [`ESPACIO.md`](ESPACIO.md) («El suelo»).
Medido con `npm run perf`: una muestra del suelo cuesta 3,7 µs a detalle completo (2,6 µs a 5 m) y
4,0 µs con 1.000 cráteres cerca; 169 baldosas de rocas y ~1.450 rocas alrededor de la aparición.
**Pendiente de comprobar en el juego:** el aspecto del suelo en la base y lejos de ella, la carga de
los workers al volar bajo y rápido, y el número de nodos dibujados en F3.

Queda fuera, a propósito: construir la vista de nave en un worker (hoy solo cuesta en la carga,
45–88 ms por nave; tendrá sentido cuando aparezcan naves durante la partida) y un impostor para
naves a varios km.

Medido con `npm run perf` (Node + V8, sin GPU; antes → después):

| Qué | Selene | Peregrina | Albatros |
|---|---|---|---|
| Draws de una nave (antes de culling) | 217 → 89 | 218 → 96 | 341 → 131 |
| Draws que proyectan sombra | 187 → 56 | 191 → 70 | 293 → 87 |
| Materiales | 118 → 33 | 110 → 33 | 154 → 39 |
| Objetos sin frustum culling | 30 → 0 | 26 → 0 | 42 → 0 |
| `ShipView.update` por frame | 0,30 → 0,04 ms | 0,25 → 0,04 ms | 0,53 → 0,07 ms · ~530 → ~15 KB |
| `ShipSim.tick` (20 Hz) | 0,10 → 0,05 ms | 0,08 → 0,03 ms | 0,18 → 0,05 ms · ~136 → 18 KB |
| Vuelo despierta cerca del suelo (60 Hz) | 0,12 → 0,07 ms | 0,09 → 0,05 ms | 0,18 → 0,08 ms · ~930 → 37 KB |
| Vuelo dormida (aparcada) | — | — | ~168 KB → 0,2 KB · 0,002 ms |

**Pendiente de comprobar en el juego** (no se ha visto en un navegador con GPU): que las rocas,
astronautas y piezas instanciadas no desaparecen en el borde de la vista ni en las sombras; que la
ocultación del exterior en salas cerradas no se activa con una ventana a la vista; que las luces del
pool alumbran igual que antes; y las cifras reales de F3.

---

## 0. Resumen

**La base de las naves es buena y hay que conservarla.** Tiene un núcleo de sistemas por módulos
que no conoce ninguna máquina por su nombre, una tabla de variables replicada por diferencias y naves
como datos validados por `finishShip`. Además, cada nave tiene su propio mundo de física en espacio de
nave, con gravedad aparente. Todo eso sirve sin cambios para naves NPC, pecios, abordajes y guardado.

**No escalan dos cosas:**

1. **El cliente dibuja y simula todo, siempre.** No hay LOD ni oclusión, y varios de los objetos más
   caros desactivan incluso el frustum culling. Cada nave cuesta entre 217 y 341 draw calls (más sus
   sombras), y hasta ~530 KB de basura y 0,5 ms de CPU por frame aunque no haga nada. Las
   rocas del entorno son unos 950 K triángulos por pasada y se dibujan en las 4 pasadas, se vean o no.
   Diez naves NPC o diez NPCs a pie cerca multiplicarían todo eso.
2. **El mundo es un parche plano de la Luna de 32 km**, cableado en unos 40 sitios:
   - gravedad fija de −Y × 1,62;
   - terreno lunar acoplado a los aparcamientos de las naves;
   - sol y cielo fijos;
   - exterior siempre en vacío;
   - coordenadas absolutas en float32 en GPU y en Rapier;
   - «marco 0 = la Luna».

   Eso bloquea el primer objetivo inmediato: despegar, llegar a órbita y volar entre cuerpos.

A eso se suman tres carencias de arquitectura que bloquean casi todo lo demás de la hoja de ruta:

- **No hay modelo de entidades.** Jugadores, naves, cajas, cohetes y escombros tienen cada uno sus
  propios ids, mensajes y replicación.
- **El personaje se maneja solo con el teclado.**
- **El modo offline duplica las reglas del servidor en vez de ejecutarlo.**

### 0.1. Las 12 optimizaciones que más rinden

| # | Qué | Dónde | Ganancia esperada | Esfuerzo |
|---|---|---|---|---|
| 1 | **Arreglar el contador de F3**: three.js reinicia `renderer.info` en cada `render()` y el postproceso hace varios. Hoy F3 enseña solo la última pasada a pantalla, no la escena | [pipeline.ts:140](../src/client/render/pipeline.ts#L140) | Poder medir lo demás | 10 min |
| 2 | **Rocas**: frustum culling por baldosa, LOD de malla y sombras solo cerca | [rocks.ts:48](../src/client/world/rocks.ts#L48), [rocks.ts:185](../src/client/world/rocks.ts#L185) | ~3,8 M → < 0,5 M triángulos por frame | 1–2 días |
| 3 | **Portales por compartimento**: las zonas, aberturas y ventanas que ya existen deciden qué partes de cada nave se dibujan, y si se dibuja el exterior | [view.ts](../src/client/ship/view.ts), [geometry.ts](../src/client/ship/geometry.ts) | Desde fuera, −60–80 % de draws de nave. Dentro, ni terreno ni rocas si no hay ventana a la vista | 1–2 semanas |
| 4 | **Sombras selectivas**: las piezas de los mandos, las piezas pequeñas y los interiores cerrados no proyectan sombra del sol | [view.ts:464](../src/client/ship/view.ts#L464), [models/index.ts:37](../src/client/ship/models/index.ts#L37) | −60–80 % de draw calls de sombra | 1 día |
| 5 | **Materiales compartidos** por nave (el tinte de la soldadora y el color del fabricante, como atributo) y **fusión por (zona × material)** | [view.ts:447](../src/client/ship/view.ts#L447) | 110–154 → ~25 materiales por nave, y 217–341 → ~40–80 draws | 3–5 días |
| 6 | **`ShipView.update` sin basura** y solo cuando algo cambia, con `blocked()` cacheado a la frecuencia de los sistemas | [view.ts:825](../src/client/ship/view.ts#L825), [sim.ts:222](../src/shared/ship/sim.ts#L222) | 0,25–0,53 ms y hasta ~530 KB por nave y frame → < 0,05 ms y ~0 KB | 2–3 días |
| 7 | **Vuelo**: masa estática precalculada, comprobar «dormida» antes del trabajo pesado, contacto con fase previa (broadphase) | [flight/model.ts:142](../src/shared/ship/flight/model.ts#L142), [contact.ts:168](../src/shared/ship/flight/contact.ts#L168) | Aparcada: 168 KB/paso → ~0. En vuelo cerca del suelo: 930 KB/paso → < 50 KB | 2 días |
| 8 | **Pantallas (MFD)**: redibujar solo las visibles y cercanas, repartidas entre frames | [screens.ts:600](../src/client/ship/screens.ts#L600) | Hasta 168 subidas de textura/s → 10–20 | 1 día |
| 9 | **Astronautas**: esfera fija para el culling, LOD de malla y de animación (el IK de agarre) | [astronaut.ts:34](../src/client/player/astronaut.ts#L34), [astronaut.ts:710](../src/client/player/astronaut.ts#L710) | 96 draws por astronauta → 4–10 de lejos | 3–5 días |
| 10 | **Profundidad invertida** (`reversedDepthBuffer`) en vez de logarítmica | [pipeline.ts:79](../src/client/render/pipeline.ts#L79) | Recupera el early-Z (menos coste por píxel con MSAA 4×) | 1 día + pruebas |
| 11 | **Luces reales a un pool de N ranuras**, como ya se hace con las de cabina | [astronaut.ts:241](../src/client/player/astronaut.ts#L241), [view.ts:178](../src/client/ship/view.ts#L178), [rockets.ts:53](../src/client/fx/rockets.ts#L53) | Coste por píxel constante; no recompilar shaders al entrar un jugador o una nave | 2–3 días |
| 12 | **Red binaria + interés por zona** | [protocol.ts](../src/shared/protocol.ts), [room.ts](../src/server/room.ts) | 5–10× menos bytes; de O(N²) a O(N · vecinos) | 1–2 semanas |

### 0.2. Los 12 sistemas más cableados

| # | Sistema | Qué objetivo bloquea | Abstracción propuesta |
|---|---|---|---|
| 1 | El mundo es la Luna plana: gravedad, terreno, cielo, sol, vacío, marco 0 (§6.1) | Espacio, planetas, atmósferas | `CelestialBody` + servicio `Environment` + árbol de marcos |
| 2 | Coordenadas absolutas sin origen flotante (§6.2) | Volar entre cuerpos | Origen flotante, render relativo a la cámara, burbujas de física |
| 3 | Sin modelo de entidades (§6.3) | NPCs, criaturas, guardado, LOD de simulación | `EntityId` + componentes + replicación genérica |
| 4 | El personaje es el teclado (§6.4) | NPCs a pie, tripulantes IA | `Intent`, producido por `InputMapper` o por `Brain` |
| 5 | `Game` hace de todo y el offline duplica al servidor (§6.5) | Todo (cada regla existe dos veces) | Servidor local en proceso o en un Worker |
| 6 | Solo dos herramientas, cableadas en flags (§6.6) | Muestras, inventario, crafteo, armas de nave | Ítems como datos + acción genérica «usar» |
| 7 | El daño va por caminos separados según el tipo (§6.7) | Combate por subsistemas, abordajes | `DamageEvent` + consulta espacial + manejadores |
| 8 | Contenido del mundo en el código (§6.8) | Eventos, misiones, estaciones, guardado | Ficheros de sector + registro de puntos de interés |
| 9 | Naves escritas a mano, ~1.000 líneas con coordenadas a mano (§6.9) | Flota de 10–40 modelos con variantes | Generador de alto nivel + JSON |
| 10 | La física de cajas, vuelo y proyectiles la hacen los clientes (§6.10) | NPCs, PvP, cazarrecompensas | Islas de física en el servidor |
| 11 | El exterior es siempre vacío (§6.11) | Atmósferas por planeta | Atmósfera del entorno en soporte vital, vuelo y trajes |
| 12 | El cliente busca módulos por nombre (`startsWith('reactor:')`) (§6.12) | Máquinas nuevas sin tocar el cliente | Capacidades y registros |

---

## 1. Mediciones

### 1.1. Cada nave, tal y como la ve el renderizador

Se construye `ShipView` de cada nave en Node y se recorre el grafo de escena: son conteos **antes**
de culling.

| Nave | Paneles | Mandos | Pantallas | Máquinas + muebles | **Mallas / draws** | **Proyectan sombra** | Triángulos | **Materiales** | Programas | Sin frustum culling |
|---|---|---|---|---|---|---|---|---|---|---|
| Selene (`hauler`) | 126 | 142 | 4 | 28 + 11 | **217** | **187** | 60,6 K | **118** | 7 | 30 |
| Peregrina | 105 | 128 | 4 | 25 + 11 | **218** | **191** | 45,8 K | **110** | 7 | 26 |
| Albatros | 309 | 194 | 6 | 37 + 22 | **341** | **293** | 86,3 K | **154** | 7 | 42 |

- Los triángulos por nave son razonables. **El problema son los draw calls y los materiales.** Cada
  máquina y cada mueble es un grupo con una malla por hueco de material, y cada máquina **clona** sus
  materiales `body`/`trim`/`accent` para que la soldadora pueda teñirla
  ([view.ts:447](../src/client/ship/view.ts#L447)).
- Con CSM de 3 cascadas, cada malla que proyecta sombra se vuelve a dibujar en cada cascada que toca.
  Las que tienen `frustumCulled = false` se dibujan **en las tres sin comprobar nada**: three.js
  dibuja `!object.frustumCulled` sin test en `WebGLShadowMap`.
- La lámpara del Albatros usa un array de **247 `vec4` uniformes en el vertex shader**
  ([materials.ts:182](../src/client/ship/materials.ts#L182)). WebGL2 solo garantiza 256 vectores
  uniformes de vértice, así que una nave algo mayor no compilaría en equipos que estén en ese mínimo.
- Construir la vista de una nave cuesta **38–77 ms en el hilo principal**, más la compilación de
  shaders la primera vez. Hoy solo pasa al cargar. Con naves NPC llegando, sería un tirón cada vez.

### 1.2. Cada nave, en CPU y en basura (GC)

Tiempos medidos en V8 (el mismo motor que Chrome). La basura se midió con el perfilador de montículo
de V8, contando también lo que recogen las GC menores.

| Camino | Frecuencia | Selene | Peregrina | Albatros | Basura por llamada (Albatros) |
|---|---|---|---|---|---|
| `ShipView.update` (cliente, **todas** las naves, cada frame) | 60–144 Hz | 0,30 ms | 0,25 ms | 0,53 ms | **~530 KB** |
| `blocked()` de todos los mandos (dentro del anterior) | por frame | 0,06 ms | 0,05 ms | 0,17 ms | — |
| `ShipSim.tick` (autoridad) | 20 Hz | 0,10 ms | 0,08 ms | 0,18 ms | **~136 KB** |
| `flight.step` despierta, cerca del suelo | 60 Hz | 0,12 ms | 0,09 ms | 0,18 ms | **~930 KB** |
| `flight.step` dormida (aparcada) | 60 Hz | — | — | — | **~168 KB** |

De dónde sale la basura (Albatros, porcentaje de los bytes):

- `ShipView.update`: funciones anónimas y el propio `update` (≈42 %: un `Matrix4` y varios `Vector3`
  **por mando y frame**, y claves de texto `led:${i}` y `zone:${id}` por lámpara y frame), `setLed`
  (15 %), `life.interlock` vía `blocked()` (8 %), y dibujo de pantallas.
- `ShipSim.tick`: `propellant.solve` (52 %), `decomp.step`, el objeto `Tick` y sus cierres, que se
  crean en cada tick, y `power.solve`.
- `flight.step` despierta: `contact.forces` (**70 %**: por cada sonda del casco y en cada subpaso,
  arrays nuevos y una evaluación completa del terreno) y `massProperties` (16 %).
- `flight.step` dormida: `massProperties` (**91 %**). La masa estructural se recalcula entera en cada
  paso ([flight/model.ts:142](../src/shared/ship/flight/model.ts#L142)), y la comprobación de «dormida»
  ([model.ts:192](../src/shared/ship/flight/model.ts#L192)) llega **después** del trabajo pesado.

**Qué supone hoy.** En `?offline` el cliente vuela las 3 naves a 60 Hz y actualiza las 3 vistas cada
frame. Eso genera del orden de **100–200 MB/s de basura**, es decir, varias GC menores por segundo,
que se notan como tirones. El servidor vuela a 60 Hz todas las naves que nadie pilota, aunque estén
aparcadas: son ~10 MB/s de basura por nave parada.

### 1.3. El mundo alrededor del punto de aparición

| Qué | Medida |
|---|---|
| Hojas del quadtree de terreno seleccionadas | **448** (64 de 16 m y 48 por nivel hasta 4 km) → 917 K triángulos si se dibujasen todas |
| Coste de generar un chunk en el worker | **13–26 ms** (el ~94 % de las muestras son de `sunVisibility`: 22 por vértice) |
| Rocas cargadas (225 baldosas en 3 anillos) | **1.319**, con 720 triángulos cada una → **950 K triángulos por pasada** |
| Rocas: pasadas en las que se dibujan | **4** (principal + 3 cascadas), sin culling → ~3,8 M triángulos por frame |
| Astronauta (GLB) | 21 mallas / 24 primitivas, **89,5 K triángulos**, 21 materiales (clonados por instancia), 22 huesos, 3,4 MB |
| Astronauta por frame | ~24 draws × 4 pasadas ≈ **96 draw calls y 360 K triángulos**, sin culling |
| `terrain.height()` | 1,6 µs, que pasa a **4,4 µs con 1.000 cráteres**: recorre toda la lista de ediciones |

### 1.4. Red (JSON tal y como se envía)

| Mensaje | Tamaño |
|---|---|
| `welcome` (3 naves, cajas) | 11,3 KB, que pasan a **163 KB con 4.000 cráteres** (el máximo que guarda el servidor) |
| Instantánea de una nave | 2,1–3,7 KB |
| Estado de un jugador | ~100 B, a 20 Hz, retransmitido a todos: O(N²) |
| Pose de una nave | 195 B, a 30 Hz mientras se mueve |
| Diferencias `shipSt` | ~100–300 B, a 10 Hz por nave activa |

### 1.5. Un frame en el punto de aparición (*estimación*)

| Parte | Draw calls (principal + sombras) | Triángulos |
|---|---|---|
| 3 naves | ~776 + entre ~700 y ~2.000 | ~0,6–1 M |
| Rocas | 24 | **~3,8 M** |
| Terreno | ~150–200 + ~100–300 | ~0,5–0,8 M |
| Astronauta local | ~96 | ~0,36 M |
| Cielo, partículas, escombros, cajas | ~40 | pocos |
| **Total** | **~2.000–3.300** | **~5–6 M** |

En WebGL el coste de CPU por draw call manda sobre los triángulos. Las naves (draw calls) y las rocas
(triángulos) son el primer blanco. Para confirmarlo en el juego hay que arreglar antes el contador
de F3 (§2.11).

---

## 2. Render: dibujar solo lo que se ve

### 2.1. Frustum culling desactivado en lo que más pesa

Estos objetos tienen `frustumCulled = false`. Se dibujan aunque estén detrás de la cámara y entran en
las 3 cascadas de sombra sin comprobar nada:

| Objeto | Dónde | Arreglo |
|---|---|---|
| Rocas: 6 `InstancedMesh` con las 1.319 instancias | [rocks.ts:48](../src/client/world/rocks.ts#L48) | Un `InstancedMesh` por baldosa y variante con su `boundingSphere`, o un `BatchedMesh` con `perObjectFrustumCulled` (three r186 lo trae) |
| Astronautas: todas sus mallas con skinning | [astronaut.ts:34](../src/client/player/astronaut.ts#L34) | Asignar una `boundingSphere` fija y conservadora (~1,4 m alrededor de la cadera) y volver a activar el culling |
| Piezas de los mandos de cada nave (6 instanciadas) | [view.ts:616](../src/client/ship/view.ts#L616) | `boundingSphere` = los límites de la nave (`def.bounds` ya existe) |
| Malla de lámparas, persianas y pistones | [view.ts:470](../src/client/ship/view.ts#L470), [view.ts:167](../src/client/ship/view.ts#L167), [view.ts:735](../src/client/ship/view.ts#L735) | Lo mismo |
| Chorros de los propulsores | [thrusterFx.ts:93](../src/client/ship/thrusterFx.ts#L93) | Esfera por chorro |

El terreno recibe un `Frustum` que no usa: `void frustum` en
[terrain.ts:225](../src/client/world/terrain.ts#L225), y el juego además le pasa uno vacío
([game.ts:1300](../src/client/game.ts#L1300)). three.js sí descarta los chunks al dibujar, porque
tienen `boundingSphere`. Pero la cola de generación no da prioridad a lo visible, y el recorrido del
árbol crea ~600 claves de texto y un `Set` en cada frame ([terrain.ts:268](../src/client/world/terrain.ts#L268)).

### 2.2. Oclusión por portales: la mayor ganancia, y el juego ya tiene los datos

Una nave ya sabe qué **salas** (`zones`) tiene, qué **aberturas** las unen (`openings`: puertas,
rampa, escotillas, conductos) y qué paneles son **cristal**. Eso es exactamente un grafo de portales.
Hoy, sin embargo:

- **Desde fuera** se dibuja todo el interior: máquinas, muebles, consolas, asientos, pantallas,
  etiquetas y LEDs, que son la mayoría de los draws de una nave (del orden del 60–80 %). El casco lo
  tapa todo salvo lo que se ve por ventanas o puertas abiertas.
- **Desde dentro** se dibujan los 448 chunks de terreno (menos los descartados por frustum), las
  1.319 rocas, el cielo y las demás naves, aunque el casco solo deje ver el exterior por las ventanas.

**Diseño propuesto:**

1. **Fusionar por (zona × material)** en lugar de por material de toda la nave. `Parts`/`Kit` ya
   agrupan por material; basta añadir la zona a la clave. Los paneles saben su `zone` y las máquinas y
   muebles también.
2. En cada frame, en el cliente:
   - **Cámara fuera de la nave:** se dibuja la cara exterior del casco. Cada sala solo se dibuja si
     alguno de sus portales hacia fuera (ventana, puerta o rampa abierta, panel reventado) está en el
     frustum y a menos de ~40 m. Más allá de ~40–60 m, ningún interior.
   - **Cámara dentro de una sala:** se dibuja esa sala. Se recorren los portales abiertos, recortando
     el frustum con el rectángulo de cada uno (la técnica clásica de portales). El mundo exterior
     (terreno, rocas, otras naves, cielo) solo se dibuja si un portal exterior es visible.
3. Donde se decide todo es en un único `visible` por grupo (zona, material). No toca los shaders.
4. Lo mismo vale para **estaciones y bases** (salas + esclusas) y para **pecios**.

**Ganancia esperada:** fuera, −60–80 % de draws por nave. Dentro, sin ventana a la vista, desaparecen
entre 1.000 y 1.500 draws del mundo y ~4 M de triángulos. Es el caso más habitual, porque el juego
transcurre andando por las naves.

### 2.3. LOD de naves, obligatorio con naves NPC

| Distancia (orientativa) | Qué se dibuja | Draws |
|---|---|---|
| < 60 m | Todo, con portales (§2.2) | 40–150 |
| 60 m – 1 km | Casco exterior fusionado en 1–3 mallas, luces exteriores, chorros; nada interior; sin pantallas | 3–6 |
| 1–20 km | Malla simplificada o impostor, luces de navegación como puntos | 1–2 |
| > 20 km | Un punto con brillo y el marcador del HUD | 0–1 (instanciado) |

Además:

- **Geometría compartida entre naves del mismo modelo.** Hoy cada `ShipView` construye la suya. El
  daño y el calor por panel (atributos `aPanel.z` y `aHeat`) pueden ir a una `DataTexture` indexada
  por panel, y así todas las naves de un modelo comparten los buffers. Eso importa para flotas NPC
  del mismo modelo.
- **Construcción en un Worker** y `renderer.compileAsync()` al cargar, para evitar los tirones de
  38–77 ms más shaders.

### 2.4. Sombras

- Lo que proyecta sombra en cada nave: 187–293 draws. Proyectan hasta las piezas de los mandos
  (palancas, tapas, botones), las consolas y cada mueble y máquina del interior
  ([view.ts:464](../src/client/ship/view.ts#L464), [view.ts:590](../src/client/ship/view.ts#L590),
  [view.ts:615](../src/client/ship/view.ts#L615), [models/index.ts:37](../src/client/ship/models/index.ts#L37)).
- Reglas propuestas:
  - **No proyectan:** las piezas de los mandos, las piezas de menos de ~20 cm y la decoración interior
    (los LEDs y las etiquetas ya no lo hacen).
  - **El interior no proyecta sombra del sol** salvo en salas con ventana: la luz solo entra por ahí.
  - **Rocas:** proyectan solo las de más de ~0,4 m, y solo en las 2 primeras cascadas.
  - **Astronautas:** solo a menos de ~40 m.
- **CSM:** el sol está quieto. Las cascadas lejanas pueden actualizarse cada 2–4 frames, o cachear
  las proyectoras estáticas (terreno, rocas, naves aparcadas) y volver a dibujar solo lo que se mueve.
  En calidad baja bastan 2 cascadas.

### 2.5. Draw calls y materiales dentro de una nave

- **110–154 materiales por nave**, casi todos clones por máquina. El tinte de la soldadora
  ([view.ts:929-941](../src/client/ship/view.ts#L929)) y el color del fabricante pueden ser un
  **atributo por vértice** (índice de pieza) más una textura o un array pequeño con la integridad y el
  color de cada pieza. Así las máquinas se fusionan por hueco de material en toda la sala: de ~200
  mallas de máquinas y muebles (Albatros) a ~10–15 draws por sala.
- **Geometría no indexada**: `strip()` hace `toNonIndexed()` en [kit.ts](../src/client/ship/models/kit.ts)
  y en [geometry.ts](../src/client/ship/geometry.ts) para poder fusionar piezas distintas. Eso
  triplica los vértices y le quita la caché de vértices a la GPU. Conviene fusionar indexado:
  `mergeGeometries` lo admite si todas las piezas tienen los mismos atributos.
- La **clave de programa del panel incluye el color de franja** ([materials.ts:143](../src/client/ship/materials.ts#L143)):
  cada librea nueva compila un programa nuevo. Con libreas por facción serían decenas de
  compilaciones. Debería ser un uniforme.
- El shader de los paneles evalúa FBM de 4 octavas en el color y **3 veces la función de relieve**
  para las diferencias finitas, con otra FBM si el panel está dañado
  ([materials.ts:131-139](../src/client/ship/materials.ts#L131)). Dentro de la nave cubre la pantalla
  entera, con MSAA 4× y sin early-Z (§2.9). Se puede usar una versión barata más allá de ~5 m (ya
  existe el factor `aa` por `fwidth`: bastaría con saltarse el cálculo), o hornear un detalle
  genérico a textura.

### 2.6. Rocas

| Hoy | Propuesta |
|---|---|
| Icosaedro de detalle 5 (720 triángulos), incluso para piedras de 4 cm | LOD por anillo: detalle 3 (320), 1 (80) y 0 (20) triángulos. Las de < 10 cm, solo en el anillo interior |
| 6 `InstancedMesh` globales sin culling | Uno por baldosa (64 m) y variante, o un `BatchedMesh` con culling por instancia |
| Proyectan en las 3 cascadas | Solo las grandes y cercanas (§2.4) |
| Se rehacen todas las matrices al cambiar de baldosa central (`rebuild`) | Solo se añaden o quitan baldosas |

Ganancia: de ~3,8 M a < 0,5 M triángulos por frame. Es probablemente el mayor ahorro de GPU del juego.

### 2.7. Astronautas: tripulantes y NPCs a pie

Un astronauta cuesta ~96 draws y 360 K triángulos por frame, más un IK de agarre caro:
`bestGrasp` evalúa 64 candidatos por brazo, y hay ~7 `updateMatrixWorld(true)` por brazo y frame
([astronaut.ts:710-870](../src/client/player/astronaut.ts#L710)). Para tener tripulaciones NPC hace
falta:

1. **Culling** con una esfera fija (§2.1).
2. **LOD de malla**: el traje se genera en Blender por script, así que se pueden exportar LOD1 y LOD2
   decimados con **materiales en atlas** (1–3 draws).
3. **LOD de animación**:
   - Cerca: todo.
   - A 15–40 m: sin IK de agarre (pose fija del arma) y actualización a 30 Hz.
   - Más lejos: actualización a 10–15 Hz o animación horneada en textura.
   - Fuera de pantalla: solo lo que afecta a la lógica.
4. **Materiales**: hoy se clonan 21 por instancia (la franja y el recorte del ojo). La franja puede ir
   como atributo o uniforme en un material compartido, y el recorte del ojo solo existe en el jugador
   local.

### 2.8. Terreno

- Para flotar entre cuerpos hace falta otro terreno (§6.1). Mientras tanto:
  - Priorizar la generación de lo visible: el frustum ya llega a `update`.
  - Claves numéricas en vez de texto.
  - No dibujar el terreno dentro de una nave cerrada (§2.2).
- `sunVisibility` hace 22 muestras por vértice y asume un **sol fijo** horneado en los vértices
  ([terrain.worker.ts:97](../src/client/world/terrain.worker.ts#L97)). Con ciclo día/noche, otro
  planeta u otra órbita hay que rehornear todo. Alternativas: un mapa de horizonte en la GPU por
  chunk (unas pocas direcciones), sombras de terreno por raymarching de la altura en baja resolución,
  o simplemente más alcance de CSM con cascadas cacheadas.
- El rendimiento de generación (13–26 ms por chunk con 2–4 workers) limita la velocidad de vuelo a
  poca altura. Hay que medirlo cuando haya vuelo rápido.

### 2.9. Tubería de render

- **`logarithmicDepthBuffer: true`** ([pipeline.ts:79](../src/client/render/pipeline.ts#L79)) escribe
  `gl_FragDepth` en todos los shaders. Eso **desactiva el early-Z**: cada píxel tapado paga su shader
  entero (paneles con FBM, terreno con 6 lecturas de textura, CSM con PCF), con MSAA 4× y un factor de píxel de
  hasta 1,5.
  - three r186 trae `reversedDepthBuffer: true` (requiere `EXT_clip_control`, que Chrome soporta).
    Da precisión de sobra conservando el early-Z. Para aprovecharla, el buffer de profundidad del
    `EffectComposer` debe ser de coma flotante. Hay que probar sombras y postproceso.
  - Para distancias planetarias no basta un único rango de profundidad. Lo habitual es render
    relativo a la cámara (§6.2) y, para lo muy lejano, una pasada aparte con su propio near/far
    (cuerpos celestes), antes de la escena cercana.
- **Resolución dinámica**: bajar el factor de píxel entre 0,7 y 1,0 cuando el frame supera el
  presupuesto.
- **`SanitizeEffect`** es una pasada completa aparte (necesaria antes del bloom). Si los materiales
  acotan su salida HDR (el origen del NaN), se puede quitar.
- **N8AO** ya es opcional (`?ao`). Bien.

### 2.10. Luces

- Cada astronauta tiene una `SpotLight` real ([astronaut.ts:241](../src/client/player/astronaut.ts#L241)),
  cada nave otra ([view.ts:178](../src/client/ship/view.ts#L178)) y los cohetes una `PointLight`
  ([rockets.ts:53](../src/client/fx/rockets.ts#L53)). En three.js **todas** entran en el bucle de luces
  de **todos** los materiales iluminados, incluido el terreno, aunque su intensidad sea 0. Además, que
  cambie el número de luces (entra un jugador, aparece una nave) **recompila todos los shaders**.
- Las luces de cabina ya resuelven esto bien: 8 ranuras por relevancia, recortadas a su sala, en
  [interiorLights.ts](../src/client/ship/interiorLights.ts). Hay que **extender la idea al exterior**:
  un pool fijo de K ranuras de foco y punto (p. ej. 4 + 4) que se asignan cada frame a los emisores
  más relevantes (distancia, intensidad, frustum). El número de luces no cambia nunca y el coste queda
  acotado.
- La lámpara por nave (`LampMaterial`, un array uniforme por nave) debería leer una **`DataTexture`**.
  Así se evita el límite de uniformes (§1.1) y todas las naves comparten programa.

### 2.11. Medir primero: F3 miente

`WebGLRenderer.render()` hace `info.reset()` en cada llamada cuando `info.autoReset` es true, que es el
valor por defecto. `EffectComposer` llama a `render()` varias veces por frame (escena, bloom,
pasadas de efecto). Por eso F3 enseña los draw calls y triángulos **de la última pasada**, que es un
triángulo a pantalla completa. El arreglo es poner `renderer.info.autoReset = false` en
`RenderPipeline` y llamar a `renderer.info.reset()` al principio de `render()`.

Conviene además que F3 muestre:

- draw calls **por categoría** (naves, terreno, rocas, astronautas, sombras), contando por `layers` o
  con un `onBeforeRender` de diagnóstico;
- `performance.memory.usedJSHeapSize` por frame, para ver la basura;
- texturas subidas por segundo.

### 2.12. Pantallas de a bordo (MFD)

14 pantallas de 512 px en canvas (4 + 4 + 6) se redibujan a 4 Hz, o a 12 Hz si cualquiera de las de
esa nave muestra una página «viva», y cada redibujo **sube la textura completa**
([screens.ts:600-623](../src/client/ship/screens.ts#L600)). Eso ocurre aunque la nave esté a 40 m o a
la espalda. Además, todas cambian de «ranura» en el mismo frame, porque la ranura sale del reloj
([screens.ts:603](../src/client/ship/screens.ts#L603)), y eso produce un tirón periódico.

- Redibujar solo si la pantalla está en el frustum, a menos de ~6–10 m y en la nave en la que estás.
- Repartir los redibujos entre frames (desfase por pantalla).
- Cachear el fondo (rejilla, marco, pestañas) en un canvas propio.
- A distancia, una textura congelada o la pantalla apagada.

### 2.13. Partículas

- La simulación en CPU recorre las **6.500 ranuras enteras en cada frame**, vivas o no, y sube los
  4 buffers completos ([particles.ts:183](../src/client/fx/particles.ts#L183)). Cada `emit` crea un
  array ([particles.ts:102](../src/client/fx/particles.ts#L102)).
- Viven **en coordenadas del mundo** y la gravedad es un escalar en −Y: las chispas y el vapor dentro
  de una nave que vuela no la siguen.
- Propuesta:
  - Lista de índices vivos y `addUpdateRange` para subir solo lo que cambia.
  - Emisores con marco de referencia (espacio de nave), y gravedad y resistencia del aire del entorno.
  - A medio plazo, partículas en GPU (transform feedback en WebGL2, compute en WebGPU) para el
    combate.

---

## 3. CPU, simulación y basura

### 3.1. Arreglos concretos, de mayor a menor efecto

| Dónde | Problema | Arreglo |
|---|---|---|
| [flight/mass.ts:48](../src/shared/ship/flight/mass.ts#L48) | `massProperties` recalcula la masa de los 105–309 paneles y de todas las piezas **en cada paso de vuelo, incluso dormida** | Precalcular lo estructural por `ShipDef`; sumar solo los consumibles y los bultos extra, y recalcular solo si cambian más de un umbral (o a 2–5 Hz) |
| [flight/model.ts:192](../src/shared/ship/flight/model.ts#L192) | La comprobación de «dormida» va detrás de `massNow`, `vents()`, la envolvente y `fcs.compute` | Salir antes. Una nave aparcada debería costar casi nada |
| [contact.ts:112-190](../src/shared/ship/flight/contact.ts#L112) | Cada sonda del casco (cientos), en 4 subpasos: arrays nuevos y una evaluación completa del terreno (1,6 µs) | Fase previa: si la altura sobre el suelo supera la profundidad de la sonda más baja, no hacer nada. Rejilla de alturas bajo la nave, cacheada por paso. Vectores reutilizados |
| [flight/model.ts:188](../src/shared/ship/flight/model.ts#L188) | `sim.vents()` recalcula todas las aberturas en cada paso, y también `AirFlow.step` en el cliente | Marca de «sucio»: solo si hay brechas, grietas o puertas abiertas con diferencia de presión |
| [view.ts:825-947](../src/client/ship/view.ts#L825) | Por mando y frame: `frameMatrix` (1 `Matrix4` + 3 `Vector3`) y varios `Matrix4` más. `lampIds.get("led:" + i)` por mando; `reactors()` y los motores filtrados por prefijo dos veces | Marcos de mando precalculados (son estáticos); matrices reutilizadas; array `ledSlot[controlIndex]`; módulos localizados una vez |
| [view.ts:869](../src/client/ship/view.ts#L869) / [sim.ts:222](../src/shared/ship/sim.ts#L222) | `blocked()` de **todos** los mandos en cada frame. Cada llamada pregunta a todos los módulos, y `conduitCut` recorre todos los paneles ([sim.ts:177](../src/shared/ship/sim.ts#L177)) | Calcularlo al llegar estado nuevo (20 Hz), no por frame. Conducto cortado por subsistema precalculado al cambiar `hp`. `subsystems.find` → `Map`. `interlock` indexado por tecla (cada módulo declara las teclas que vigila) |
| [view.ts:751](../src/client/ship/view.ts#L751) | Toda la vista se actualiza aunque nada haya cambiado y la nave esté lejos | Marcas de sucio por `apply`/`applyState`. Fuera de ~60 m, solo la pose y las luces exteriores |
| [systems.ts:209-266](../src/shared/ship/systems.ts#L209) | En cada tick se crean el objeto `Tick`, sus cierres, `demand`, `fuel`, `holds` y `sources` | Reutilizarlos por nave (vaciar en vez de crear). `propellant.solve` es el 52 % de la basura: repasarlo |
| [systems.ts:168](../src/shared/ship/systems.ts#L168) | `circuitLive` recorre todos los paneles en cada consulta | Igual que `conduitCut` |
| [game.ts:347](../src/client/game.ts#L347) | Cada nave avanza su mundo interior de Rapier en cada paso fijo, aunque no haya nadie ni nada dinámico | Saltarlo si no hay cuerpos dinámicos despiertos. Crear el mundo interior solo para naves cercanas |
| [frames.ts:26](../src/client/frames/frames.ts#L26) | `Frames.ship()` hace `ships().find` y se llama decenas de veces por paso | `Map<FrameId, …>` |
| [hud.ts:191-231](../src/client/ui/hud.ts#L191) | 4 `innerHTML` por frame (marcadores, telemetría, constantes vitales, red) y lectura de `clientWidth` (layout forzado) | Nodos fijos con `textContent`, actualizar solo si cambia, texto a 10 Hz, ancho cacheado en `resize` |
| [rockets.ts:65](../src/client/fx/rockets.ts#L65) / [rockets.ts:150](../src/client/fx/rockets.ts#L150) | **Fuga**: cada cohete crea 5 geometrías nuevas que nunca se liberan (`remove` no llama a `dispose`) | Geometría compartida creada una vez |
| [cargo/crates.ts:123](../src/client/cargo/crates.ts#L123) | Una malla por caja (2 draws × 4 pasadas); `list.find` por mensaje ([crates.ts:237](../src/client/cargo/crates.ts#L237)); arrays por caja y paso | `InstancedMesh` por especificación, `Map` por id, reutilización |
| ~~terrain.ts:148~~ | Cada muestra de terreno recorre **todos** los cráteres (O(E)); el servidor guarda hasta 4.000 y los manda todos al entrar | **Hecho**: índice espacial en el cubo-esfera (`terrainMods/store.ts`), cráteres repetidos fusionados; se siguen mandando todos al entrar (~100 B cada uno) |
| [game.ts:1183-1190](../src/client/game.ts#L1183) | Por nave y frame: `new Vector3` por asiento y `[...remotes.values()].some()` | Estado de asiento como evento, no sondeo |

**Objetivo razonable:** < 20 KB de basura por frame en total y < 0,1 ms por nave cercana, y
prácticamente 0 por nave lejana.

### 3.2. Cómo evitar que la basura vuelva

El estilo funcional de `geom.ts` (`add`, `sub`, `qRotate`… devuelven arrays nuevos) es muy legible y
está bien para código frío. En los bucles calientes (contacto, asignador, vista) conviene añadir
variantes «en `out`» (`addTo(out, a, b)`) y usarlas solo ahí. Una prueba automática que mida la
basura por llamada (§11) mantiene el problema a raya.

### 3.3. Simulación por niveles de detalle (objetivo inmediato)

Es la pieza que hace posibles las naves NPC con rutas, las batallas que se resuelven en abstracto y
la economía. Hoy todo va a la máxima fidelidad: cada nave vuela a 60 Hz con asignador y contacto,
sus sistemas a 20 Hz, y su interior tiene física, esté donde esté.

| Nivel | Dónde (orientativo) | Física | Sistemas de nave | Vuelo | Red |
|---|---|---|---|---|---|
| **L0** completo | Burbuja del jugador (≤ 2 km) | Rapier: exterior + interiores | 20 Hz, todos los módulos | 60 Hz, modelo completo | 20–30 Hz |
| **L1** reducido | Visible (≤ 50 km) | Sin interiores; el casco es un cuerpo simple | 2–5 Hz, con los módulos que importan (energía, propelente, daño) | 10 Hz, masa puntual + actitud, autopiloto | 2–5 Hz |
| **L2** abstracto | Resto del sistema | Ninguna | Contadores: combustible, casco %, tripulación, carga, munición | «En raíles»: ruta u órbita parametrizada por tiempo | Eventos |
| **L3** agregado | Otros sistemas estelares | — | Flotas y facciones como cifras | — | — |

- **Hidratar** (de L2 a L0) genera el estado detallado de forma determinista a partir del agregado y
  una semilla: qué paneles están dañados según el casco %, cuánto propelente hay en cada depósito…
- **Deshidratar** (de L0 a L2) suma el detalle en contadores.
- `VarTable` ya es un estado plano y con nombre, así que es la base natural. Falta serializar **por
  nombre** y no por índice (§5.3), y que cada módulo sepa exportar e importar su parte del agregado.
- El planificador ([engine/systems.ts](../src/engine/systems.ts)) necesita **frecuencias por
  sistema** y **presupuesto por frame** (repartir 50 naves L1 entre varios frames).

---

## 4. Red

### 4.1. Hoy

- JSON sobre WebSocket ([netClient.ts:188](../src/client/net/netClient.ts#L188), [room.ts:604](../src/server/room.ts#L604)).
  El comentario de [protocol.ts](../src/shared/protocol.ts) ya prevé pasar a binario. Bien pensado.
- **Sin gestión de interés**: todo mensaje va a todos. Las instantáneas de jugadores son O(N²)
  ([room.ts:591](../src/server/room.ts#L591)), y las poses de nave y las diferencias `shipSt` van a
  todos, estén donde estén.
- **Clientes de confianza**: movimiento propio, cajas (las simula su «dueño»), vuelo de la nave
  pilotada (el servidor la adopta) e impactos de cohete (los dice el que dispara). Para cooperativo
  vale. Con PvP, cazarrecompensas y reputación no (§6.10).

*Estimación* del ancho de banda de bajada por cliente, con el formato actual:

| Situación | Jugadores | Naves moviéndose | Diferencias de sistemas | Total |
|---|---|---|---|---|
| 10 jugadores, 10 naves | 18 KB/s | 58 KB/s | ~20 KB/s | **~100 KB/s** (servidor: ~1 MB/s) |
| 50 jugadores, 50 naves | 98 KB/s | 292 KB/s | ~100 KB/s | **~490 KB/s** (servidor: ~25 MB/s) |

### 4.2. Propuesta

1. **Binario y cuantizado**:
   - Posición relativa a su marco en 3 × 16–24 bits; cuaternión «smallest three» en 32–48 bits;
     velocidades en 16 bits.
   - Una pose de nave pasa de 195 B a ~30 B.
   - Las diferencias de `VarTable` ya van cuantizadas: basta empaquetarlas como `u16` índice + `f32`
     o `u16` valor.
2. **Deltas contra la última instantánea confirmada** por el cliente, en vez de siempre completas.
3. **Interés** por celdas y por marco:
   - Siempre: tu nave, tu tripulación y lo que tienes en la mano.
   - Por distancia y relevancia: el resto, con un acumulador de prioridad (lo cercano y lo que cambia
     sube). Lo lejano va a 1–5 Hz.
4. **Replicación genérica por entidad** (§6.3): un esquema por componente en lugar de un tipo de
   mensaje por cosa. Los mensajes quedan para eventos (explosión, frase del sistema).
5. `welcome` **por streaming**: primero lo cercano, y las ediciones del terreno por región según se
   acerca el jugador.
6. Más adelante, **WebTransport** (datagramas no fiables) para el estado continuo, sin el bloqueo en
   cabeza de línea de TCP.

---

## 5. Servidor y escalado

### 5.1. Hoy

- Una sola `Room`, un hilo, `setInterval` cada ~8 ms ([room.ts:96](../src/server/room.ts#L96)),
  `MAX_PLAYERS = 2`.
- Todas las naves sin piloto vuelan a 60 Hz ([room.ts:140](../src/server/room.ts#L140)), aunque estén
  aparcadas (§1.2).
- `explosion()` recorre **todas** las naves, y cada una todos sus paneles y piezas, sin comprobar
  antes la distancia a la nave ([room.ts:390](../src/server/room.ts#L390), [sim.ts:298](../src/shared/ship/sim.ts#L298)).
- El servidor **no tiene física**: no puede simular NPCs a pie, cajas sin dueño ni proyectiles con
  autoridad.
- **Nada se guarda.** Los cráteres viven en memoria y las naves se recrean al arrancar.

### 5.2. Extrapolación

Con los costes medidos (§1.2), 50 naves NPC despiertas a fidelidad completa serían ~0,45 núcleos
solo en vuelo y ~0,13 en sistemas. La basura sería del orden de **1–3 GB/s**: entre 400 y 930 KB
por paso de vuelo × 60 Hz × 50 naves, más los ticks de sistemas. Sin arreglar la basura (§3.1), sin
simulación por niveles (§3.3) y sin trabajo fuera del hilo principal, no llega.

### 5.3. Arquitectura propuesta

- **Regiones**: sistema estelar → cuerpo → órbita o superficie. Cada una tiene sus entidades, su
  tick y sus niveles de detalle, y puede ir en un `worker_thread` o en otro proceso.
- **Islas de física** en el servidor (Rapier funciona en Node) alrededor de cada jugador y de cada
  zona «caliente» (un combate, un abordaje), fusionadas cuando se solapan.
- **Guardado** (SQLite o LevelDB para empezar):
  - Entidades con sus componentes serializados **por nombre** (no por índice de `VarTable`: si un
    módulo añade una variable, los índices cambian y un guardado viejo se rompe).
  - Versión del esquema.
  - Ediciones del terreno por región.
  - Reloj del mundo.
- **Reloj del mundo** único: hoy el sol está fijo y la Tierra gira con la hora local
  ([game.ts:1302](../src/client/game.ts#L1302)).

---

## 6. Sistemas cableados y cómo abstraerlos

Cada punto dice **qué** está cableado, **dónde**, **qué objetivo bloquea** y **qué hacer**.

### 6.1. El mundo es un parche plano de la Luna (crítico: bloquea «salir al espacio»)

`DESIGN.md` ya lo decía: *«nada puede asumir que solo existe la Luna»*. El código lo asume en todas
partes:

| Supuesto | Dónde |
|---|---|
| Un único cuerpo, `MOON` (radio y gravedad); `BODY_RADIUS` duplica el radio | [constants.ts:27](../src/shared/constants.ts#L27), [terrain.ts:37](../src/shared/terrain.ts#L37) |
| Terreno plano de 32 km (`ROOT_SIZE`), con curvatura aproximada por un paraboloide «exacto a ±20 km» | [client/world/terrain.ts:21](../src/client/world/terrain.ts#L21), [terrain.ts:165](../src/shared/terrain.ts#L165) |
| **Gravedad −Y × 1,62** repetida: vuelo (`FLIGHT.g`, `G`, contacto), física del mundo (−9,81 con escala por cuerpo), marcos, interior de nave, cajas, escombros, cohetes, partículas (≥10 literales `1.62`) | [flight/model.ts:28](../src/shared/ship/flight/model.ts#L28), [fcs.ts:106](../src/shared/ship/flight/fcs.ts#L106), [contact.ts:68](../src/shared/ship/flight/contact.ts#L68), [physics.ts:36](../src/client/world/physics.ts#L36), [frames.ts:18](../src/client/frames/frames.ts#L18), [shipSpace.ts:32](../src/client/frames/shipSpace.ts#L32), [crates.ts:171](../src/client/cargo/crates.ts#L171), [rockets.ts:7](../src/client/fx/rockets.ts#L7), [particles.ts:129](../src/client/fx/particles.ts#L129) |
| «Arriba» = +Y del mundo: `vs = v[1]`, `gs = hypot(v0, v2)`, altura = `p[1] − height(x, z)`, rumbo sobre XZ | [flight/model.ts](../src/shared/ship/flight/model.ts), [readout.ts](../src/shared/ship/flight/readout.ts), [contact.ts](../src/shared/ship/flight/contact.ts) |
| El movimiento del traje está ajustado a 1/6 g (salto a 1,3 m; el jetpack solo sube porque su empuje de 3,3 m/s² supera la gravedad lunar) | [controller.ts:19-35](../src/client/player/controller.ts#L19) |
| Sol, Tierra y cielo fijos; latitud de 20° cableada en la matriz celeste; sombras de terreno horneadas para ese sol | [game.ts:56-60](../src/client/game.ts#L56), [game.ts:1399](../src/client/game.ts#L1399), [constants.ts:40](../src/shared/constants.ts#L40) |
| La luz solar de la nave es 0 o 1 según el sol fijo | [sim.ts:85](../src/shared/ship/sim.ts#L85) |
| Marco 0 = «la Luna»; los marcos son ids de nave | [crates.ts:11](../src/shared/ship/crates.ts#L11), [frames.ts](../src/client/frames/frames.ts) |
| El exterior es siempre vacío (§6.11) | [atmos.ts:40](../src/shared/ship/modules/atmos.ts#L40) |

**Abstracción propuesta:**

```ts
/** Un cuerpo celeste es datos: la Luna es solo el primero. */
interface CelestialBody {
  id: BodyId;
  radius: number;                 // m
  mu: number;                     // GM (m³/s²): la gravedad sale de aquí, no de una constante
  rotation: { axis: V3; period: number };
  orbit?: OrbitElements;          // alrededor de su padre (en raíles)
  terrain: TerrainGenerator;      // cráteres, erosión, biomas… según el tipo
  atmosphere?: AtmosphereModel;   // presión/temperatura/composición por altura, viento, tormentas
}

/** Todo lo que el juego pregunta al «mundo» en un punto: nunca una constante. */
interface Environment {
  gravity(frame: FrameId, p: V3): V3;          // vector, en el marco dado
  up(frame: FrameId, p: V3): V3;               // −gravedad normalizada (o la del cuerpo dominante)
  surface(frame: FrameId, p: V3): { agl: number; normal: V3; body: BodyId } | null;
  atmosphere(frame: FrameId, p: V3): { kPa: number; K: number; density: number; o2: number; toxic: number; wind: V3 };
  sun(frame: FrameId, p: V3): { dir: V3; irradiance: number; eclipsed: boolean };
}
```

- El controlador del astronauta **ya admite un «arriba» cualquiera**: lo saca de su gravedad
  ([controller.ts:221-222](../src/client/player/controller.ts#L221)). `ShipSpace` ya compone gravedad y
  aceleración. Solo hay que alimentarlos con el `Environment` en lugar de `MOON.gravity`.
- `FlightModel.step` pasa a recibir `env: Environment`, con gravedad radial, AGL a lo largo del «arriba»
  local, resistencia y sustentación si hay atmósfera, y modos de autopiloto orbitales (§8).
- Los parámetros de movimiento del traje se derivan de la gravedad y del traje (masa y empuje), no de
  constantes lunares.

**Estado (29-09-2026):** resuelto en lo que toca al suelo: no hay mundo plano ni parche; el suelo
es por cuerpo y por datos (`BODIES`, `SurfaceDef`, sitios), la gravedad es radial (§ESPACIO) y todo
pregunta por «el suelo» con `heightAboveGround` y por «el cuerpo» con `bodyAt`. Quedan el traje
ajustado a 1/6 g, el sol y el cielo fijos para la latitud de la base.

### 6.2. Coordenadas absolutas sin origen flotante (crítico: bloquea «volar entre cuerpos»)

- Todo el mundo exterior usa coordenadas absolutas con origen en el punto de aterrizaje. En la CPU
  eso es float64 y va bien. Pero:
  - **En la GPU**, los shaders calculan posiciones de mundo en float32: `vWorldPos` del terreno (sus
    UV de textura), `vIntW` de las luces de cabina, `vPW` del polvo de los paneles, `vSuitWorld`.
    A 100 km ya hay escalones de ~8 mm; a 1.000 km, de ~6 cm; en órbita no funciona.
  - **Rapier** es f32. El mundo exterior está centrado en el origen, así que a 100 km el controlador
    de personaje tiembla y a 1.000 km no se puede usar.
- Lo que **ya está bien**: cada nave tiene su mundo interior en espacio de nave (`ShipSpace`), así que
  la tripulación nunca sufre este problema dentro de una nave. Es la idea a generalizar.

**Propuesta:**

1. **Árbol de marcos**: sistema → cuerpo (que gira) → superficie u órbita → nave → nave anidada
   (un caza en el hangar de un carguero, una nave posada en una estación). Hoy `Frames` es «la Luna
   + naves» con búsqueda lineal.
2. **Render relativo a la cámara**: la posición de cada objeto en la escena es «mundo − cámara»,
   calculada en float64 en la CPU. Las posiciones de mundo en los shaders se sustituyen por
   posiciones relativas a la cámara, más un desplazamiento de baja frecuencia para las texturas
   triplanares.
3. **Burbujas de física**: el mundo exterior de Rapier se recentra (se desplazan todos sus cuerpos)
   cuando el jugador se aleja más de 1–5 km del origen de la burbuja. En el servidor, una burbuja por
   isla (§5.3).
4. Los datos de red van **relativos a su marco** (ya pasa con la tripulación a bordo: `fr`), con
   cuantización por marco.

### 6.3. No hay modelo de entidades (crítico: «un sistema común de entidades para naves, personas y criaturas»)

Hoy cada tipo de cosa tiene su camino:

| Cosa | Servidor | Cliente | Red | Ids |
|---|---|---|---|---|
| Jugador | `Member` en `room.ts` | `Astronaut` + `RemotePlayer` + `PlayerController` | `state` / `snapshot` | contador de la sala |
| Nave | `ShipSim` | `ShipClient` (sim + vista + física + espacio) | `ship`, `shipSt`, `shipPose`, `pilot` | `SHIP_SPAWNS.id`, y además es el id de marco |
| Caja | `Crate` | `CrateBody` | `crate`, `crateTake` | su posición en la lista |
| Cohete | — (lo simula cada cliente) | `Rockets` | `fire`, `hit`, `explode` | ninguno |
| Escombro | — | `Debris` | — | — |

**Propuesta** (un ECS ligero; no hace falta una librería):

```ts
type EntityId = number;             // único en el mundo y persistente
// Componentes (datos) — cada sistema recorre los que le interesan:
Transform   { frame: FrameId; p: V3; q: Quat; v: V3; w: V3 }
Body        { kind: 'dynamic' | 'kinematic' | 'static'; shape: ShapeRef; mass: number }
Model       { asset: string; lod: LodSet }               // qué se dibuja y cómo se simplifica
Replicated  { schema: SchemaId; rate: number }            // qué campos viajan y cuantizados cómo
Authority   { owner: 'server' | PlayerId }                // quién simula
Health      { hp: number; max: number; armor: ArmorDef }  // o `ShipHull` para naves (paneles)
Interactable{ verbs: VerbDef[] }                          // §6.6
Inventory   { slots: ItemStack[] }  /  Container
Character   { suit: SuitDef; seat?: SeatRef }             // controlador de personaje
Brain       { kind: string; state: unknown }              // IA (§8)
Ship        { sim: ShipSim }                              // lo que ya existe, entero
Faction     { id: FactionId; reputation: … }
SimLod      { level: 0 | 1 | 2 | 3 }                      // §3.3
```

- `ShipSim` entra tal cual como componente. Su `VarTable` es el modelo para replicar cualquier
  componente: variables con nombre, cuantizadas, enviadas por diferencias.
- Un pecio es una entidad `Ship` sin tripulación y con sistemas muertos. Una criatura es
  `Character` + `Brain` + `Health`. Una caja es `Body` + `Container`.
- Con ids globales, el guardado, la replicación con interés y el LOD de simulación se escriben
  **una vez**.

### 6.4. El personaje es el teclado (alto: bloquea NPCs a pie y tripulantes IA)

- `PlayerController.update` lee `input.down('KeyW')`, `'KeyC'`, `'ShiftLeft'`, `'Space'`…
  ([controller.ts:227-267](../src/client/player/controller.ts#L227)).
- `game.ts` reparte ~17 teclas en `tick` ([game.ts:1120-1155](../src/client/game.ts#L1120)), y los
  mandos de piloto se leen directamente ([game.ts:655](../src/client/game.ts#L655)).
- `PILOT_KEYS` duplica el texto de las teclas.

**Propuesta:**

```ts
interface CharacterIntent { move: V2; yaw: number; pitch: number; jump: boolean; crouch: boolean; run: boolean;
                            use: EntityRef | null; primary: boolean; secondary: boolean; tool: ItemId | null }
// jugador: InputMapper (acciones remapeables, mando, HOTAS) → Intent
// NPC:     Brain → Intent          (el mismo controlador, en el servidor)
// nave:    FlightCommand ya es un "intent" de vuelo: lo pueden dar el jugador o un piloto IA
```

### 6.5. `Game` hace de todo y el offline duplica al servidor (alto)

- [game.ts](../src/client/game.ts) (1.413 líneas) reúne entrada, manejadores de red, autoridad
  offline, marcos, cajas, HUD, cámara y sistemas.
- El modo offline **reimplementa** reglas del servidor:
  - `offlineSystems` ([game.ts:550](../src/client/game.ts#L550)) y `offlineEvents`;
  - la rama offline de `onExplode`;
  - `localInteract` y `localRepair`.

  Son copias de `room.ts` que ya divergen (p. ej. el daño a jugadores no existe offline).

**Propuesta:** `Room` solo importa código compartido salvo el tipo `WebSocket`. Con una interfaz
`Connection { send(msg); onMessage(cb); onClose(cb) }` y un **transporte local** (en proceso, o en un
Web Worker para no cargar el hilo de render), el offline **es** el servidor. Después, partir `Game`
en:

- `WorldClient` (réplica de entidades, marcos, LOD);
- `LocalPlayer` (intención, herramienta, asiento);
- `ReplicationClient`;
- `HudPresenter`;
- `ToolController`.

### 6.6. Herramientas, armas e interacción (medio-alto: muestras, inventario, crafteo, armas de nave)

- Solo existen dos herramientas, cableadas en varios sitios:
  - `LAUNCHER` / `WELDER` con `Digit1` / `Digit2` en `game.ts`;
  - el bit `StateFlags.Welder` del protocolo;
  - el servidor solo deja reparar si ese bit está puesto ([room.ts:549-550](../src/server/room.ts#L549));
  - `equip('welder' | 'launcher')`.
- `Interaction` solo sabe de naves (mandos, paneles, asientos, máquinas). Las cajas se miran aparte
  en `game.ts` ([game.ts:756](../src/client/game.ts#L756)).

**Propuesta:**

- **Ítems como datos**: el catálogo de componentes ya es un buen germen de ítems. Cada ítem declara
  un **comportamiento**: proyectil, haz de reparación, taladro o toma de muestras, escáner, contenedor…
- **Mensaje genérico** `use(tool, target, verb)`, que el servidor valida (distancia, ítem en mano,
  energía).
- **Registro de `Interactable`** con una consulta espacial: mandos de nave, cajas, NPCs («hablar»),
  terminales de estación (tablón de encargos), botín de pecios, rocas y hielo (muestras), criaturas
  (espantar). Es el patrón que ya proponía [DESIGN.md §9](DESIGN.md).

### 6.7. Daño y explosiones por caminos separados (alto: combate por subsistemas y abordajes)

- `room.explosion` decide por su cuenta:
  - el cráter (radio fijo de 2,4 m);
  - el daño a cada nave, con `shipBlast` y el radio de tripulación fijo en 6 m;
  - el daño a jugadores, **110 lineal y cableado** ([room.ts:418](../src/server/room.ts#L418)).
- Las cajas las empuja el cliente que dispara ([game.ts:995](../src/client/game.ts#L995)).
- Los cohetes los simula cada cliente y el tirador informa del impacto.

**Propuesta:**

```ts
interface DamageEvent { kind: 'kinetic' | 'explosive' | 'thermal' | 'emp' | 'corrosion';
                        at: WorldPoint; dir?: V3; radius: number; amount: number; source: EntityId }
```

- Una consulta espacial (rejilla o BVH de límites de entidades por marco) entrega el evento a los
  manejadores de cada componente: `ShipSim.explode` (paneles + piezas, que ya existe), `Health`,
  empuje de `Body`, cráter si hay suelo cerca, criaturas.
- Las **armas de nave** son proyectiles o rayos simulados en el servidor. Los de mano pueden
  predecirse en el cliente con validación.
- Los **erizos come-metal** encajan aquí: un daño `corrosion` lento sobre paneles y piezas, más
  consumir ítems de chatarra. La reparación con la soldadora ya existe.

### 6.8. Contenido del mundo en el código (medio: eventos, misiones, estaciones, guardado)

| Contenido | Dónde |
|---|---|
| Naves aparcadas y su posición | `SHIP_SPAWNS` en [constants.ts:43](../src/shared/constants.ts#L43) |
| ~~Claros sin rocas: el generador de terreno depende de dónde se aparcan las naves~~ | **Hecho**: las pistas son modificadores del sitio `base` ([sites.ts](../src/shared/space/sites.ts)) |
| ~~Cráter emblemático y zona de aterrizaje aplanada en el origen~~ | **Hecho**: sitios `landmark` y `base` (datos) |
| ~~Puntos de reaparición~~ | **Hecho**: `spawns` del sitio de inicio ([world.ts](../src/shared/space/world.ts)) |
| ~~Cajas de suministro~~ | **Hecho**: `drops` del sitio ([spawn.ts](../src/shared/ship/spawn.ts)) |
| ~~Puntos de navegación~~ | **Hecho**: `nav` de cada sitio, como direcciones ([flight/nav.ts](../src/shared/ship/flight/nav.ts)) |
| ~~Marcadores del HUD «Base» y «Cráter»~~ | **Hecho**: `compass` de cada sitio |
| Textos del menú («SELENE», «LUNA · 20.2°N 30.8°E», «2 MÁX») | [main.ts](../src/client/main.ts) |
| `MAX_PLAYERS = 2` y la semilla `1969` en dos sitios | [constants.ts:4](../src/shared/constants.ts#L4), [game.ts:177](../src/client/game.ts#L177) |

**Propuesta:**

- **Ficheros de sector** (JSON) con cuerpos, estaciones, puntos de interés, aparcamientos y
  «sellos» de terreno (aplanados, cráteres, bases).
- Un **registro de puntos de interés** que usen la navegación, los marcadores del HUD, las misiones y
  los eventos.
- El generador de terreno recibe los sellos como datos y no importa nada de las naves.

### 6.9. Las naves se escriben a mano (alto: flota de 10–40 modelos con variantes)

Cada nave son ~940–1.210 líneas de TypeScript con coordenadas a mano: perfiles, cortes, rutas de
conductos punto a punto, posición de cada consola… Ver
[peregrina.ts](../src/shared/ship/ships/peregrina.ts). La **validación** (`finishShip`/`checkShip`)
y el **catálogo** son excelentes. Lo que no escala es escribir la geometría.

**Propuesta: un generador de alto nivel por encima del `ShipBuilder` actual.**

- **Plano (`blueprint`)**:
  - cadena de secciones con perfil y longitud;
  - grafo de salas con su rol (cabina, bodega, máquinas, esclusa, camarotes);
  - **huecos** de equipo con clase de tamaño y rol;
  - puertas y escaleras por adyacencia.
- **Equipamiento (`loadout`)**: qué componente del catálogo va en cada hueco.
- **Automatizado**:
  - circuitos y disyuntores a partir de los consumos;
  - **rutas de conductos** por A* sobre bandejas de techo y suelo;
  - **consolas por plantilla de rol** (timón, ingeniería, cuadro eléctrico, soporte vital), rellenas
    según el equipamiento;
  - luces según las salas;
  - el manual.
- **Variantes procedurales** (objetivo lejano): la misma base con otro equipamiento (blindaje,
  armamento), otra **librea por facción** (un uniforme, §2.5) y **desgaste** sembrado (paneles con
  daño inicial, suciedad).
- Formato **JSON**, para recarga en caliente y, más adelante, un editor en el navegador. La salida
  sigue siendo el `ShipDef` de hoy, así que todo lo que hay detrás no cambia.

### 6.10. La física la hacen los clientes (alto: NPCs, PvP, cazarrecompensas)

- Movimiento propio: el servidor se fía (solo comprueba que los números sean finitos).
- Cajas: las simula su «dueño», y el primer jugador por id responde por las que no tienen dueño
  ([game.ts:508](../src/client/game.ts#L508)).
- Vuelo: el cliente del piloto vuela la nave y el servidor adopta la pose.
- Cohetes: los simulan todos los clientes y el que dispara dice dónde impactó.

Es una buena decisión para un cooperativo de 2, pero **asume que hay un humano para simular cada
cosa**. Las naves NPC, las cajas de un pecio lejano o un NPC a pie no tienen «dueño». Con PvP y
reputación, además, hace falta autoridad (o validación: velocidades posibles, alcance, cadencia).
**Propuesta:** islas de física en el servidor (§5.3), predicción en el cliente para su personaje y su
nave, y reconciliación.

### 6.11. El exterior es siempre vacío (medio: atmósferas por planeta)

- `atmos.ts` modela el exterior como «b = −1 → vacío» (0 kPa).
- Las partículas no tienen resistencia del aire y el vuelo no tiene aerodinámica.
- `BodyDef.atmosphereDensity` existe pero **no lo lee nadie** ([constants.ts:24](../src/shared/constants.ts#L24)).

**Propuesta:** el exterior es el `Environment.atmosphere()` del punto (§6.1):

- una brecha **iguala con la presión de fuera** (que puede ser tóxica o más alta);
- los radiadores pierden calor por convección si hay aire;
- el traje usa O₂ y temperatura;
- el vuelo usa densidad y viento;
- el render usa dispersión y niebla.

`decomp` y `airflow` ya calculan el caudal por orificio con presión a los dos lados: solo hay que
dejar de suponer 0 fuera.

### 6.12. El cliente conoce los módulos por su nombre (medio)

- La vista, las pantallas y el manual buscan `m.id.startsWith('reactor:')`, `'engine:'` o `'apu:'`
  ([view.ts:899](../src/client/ship/view.ts#L899), [view.ts:969](../src/client/ship/view.ts#L969),
  [screens.ts:563-565](../src/client/ship/screens.ts#L563), [manual.ts:326](../src/client/ui/manual.ts#L326)).
- Los indicadores son una unión cerrada (`'reactor-core' | 'gear-greens' | 'airlock'`), y las páginas
  de pantalla un registro con `switch` en el cliente.

El núcleo cumple la regla «nada de `if (part.id === …)`», pero el cliente la rompe por los nombres de
módulo.

**Propuesta:**

- **Capacidades** declaradas por el módulo (`provides: ['thrust', 'heat', 'power-source']`), con
  consultas del tipo `sys.all('thrust')`.
- Indicadores y páginas registrados por tipo de módulo, igual que los `SYSTEM_FACTORIES`.
- Radar, torreta, antena y cargador ya están declarados ([modules/index.ts:26](../src/shared/ship/modules/index.ts#L26)).
  Darles vida es escribir su módulo.

### 6.13. Materiales y luces a base de parches de shader (medio)

- Terreno, paneles, traje, luces de cabina y CSM se hacen con `onBeforeCompile` y reemplazos de texto
  sobre los chunks de three.js. Funciona, pero:
  - cada actualización de three.js puede romper un `replace`;
  - **no se puede portar a WebGPU/TSL** sin reescribirlo.
- Las luces de cabina van en uniformes globales de módulo (un único renderizador y una única cámara).
- `LampMaterial` es uno por nave (§2.10).

**Propuesta:** una capa propia de materiales, del tipo `defineSurface({ albedo, relief, emissive… })`,
que hoy genere los parches de WebGL y mañana nodos TSL. WebGPU trae culling en la GPU, dibujado
indirecto y partículas por compute. No compensa migrar ya, pero sí dejar el camino abierto.

### 6.14. Protocolo a mano (medio)

Cada mensaje nuevo toca 4 sitios:

1. la unión de tipos en `protocol.ts`;
2. el `switch` de `netClient.ts`;
3. el manejador en `game.ts`;
4. el `switch` de `room.ts`.

`PROTOCOL_VERSION` se sube a mano. **Propuesta:** con entidades y componentes (§6.3) los mensajes se
reducen a eventos. Para esos, un registro con esquema (tipos + validación + codificación binaria
generados de una sola definición).

### 6.15. Otros cableados menores

| Qué | Dónde | Propuesta |
|---|---|---|
| Las cajas de asiento son una sola forma (`SEAT_BOXES`) | [def.ts:479](../src/shared/ship/def.ts#L479) | Asientos del catálogo (cabina de caza, banco, litera) |
| O₂ del traje, alcance de mandos y ritmo de reparación son constantes | `SUIT` en [crew.ts:10](../src/shared/ship/crew.ts#L10), `REACH` y `REPAIR_RATE` en [sim.ts](../src/shared/ship/sim.ts) | Datos del traje y de la herramienta |
| La masa del astronauta está en varios sitios **y no coincide**: 180 kg en el retroceso y en el arrastre del aire, 130 kg en la masa a bordo | [game.ts:1078](../src/client/game.ts#L1078), [airflow.ts:52](../src/shared/ship/airflow.ts#L52), `MASS.crewKg` en [mass.ts:12](../src/shared/ship/flight/mass.ts#L12) | Masa del `Character` (traje + carga) |
| 4 colores de franja o variantes | [constants.ts:36](../src/shared/constants.ts#L36) | Aspecto del personaje como datos (facción, rango) |
| El planificador solo tiene «fijo» (60 Hz) y «por frame» | [engine/systems.ts](../src/engine/systems.ts) | Frecuencia por sistema y presupuesto (IA a 5 Hz, economía a 0,1 Hz) |

---

## 7. Cada objetivo y lo que necesita

| Objetivo | Depende de | Lo que ya existe y sirve |
|---|---|---|
| **Salir al espacio sin cargas** | §6.1, §6.2, terreno esférico (§8.2), vuelo generalizado, render relativo a la cámara | `ShipSpace` (interior en espacio de nave), `Frames.transfer`, `FlightModel` con su asignador |
| **Naves NPC con rutas y motivos** | §6.3, §3.3, §6.10, IA de piloto, economía mínima (origen/destino/carga) | El autopiloto con modo NAV y puntos de ruta, `FlightCommand` como intención |
| **Combate entre naves** | §6.7, armas de nave (módulos nuevos), §6.10, §4 (interés) | Paneles rompibles, piezas con vida, conductos que cortan circuitos, reacciones en cadena |
| **Eventos: pecios, emboscadas, socorro** | §6.3, §6.8 (puntos de interés, director de eventos), §3.3 | Un pecio es una nave con estado degradado: `ShipSnapshot` ya guarda `sw`, `hp` y `st` |
| **Tareas y misiones** | §6.8, §6.6 (terminal del tablón), guardado, reputación | — |
| **NPCs a pie** | §6.4, §6.3, §2.7, navegación | **Salas + aberturas + escotillas + escaleras = grafo de navegación**; los mandos se accionan por `interact` (los NPC pulsarían botones de verdad) |
| **Cajas que empujen bien** | Autoridad en el servidor o predicción (§6.10); apilado estable | Rapier por marco, empuje con fuerza acotada, sujetar tipo Half-Life, arrastre del aire |
| **Recoger muestras** | §6.6 (herramienta de toma de muestras), ítems, `Environment.surface()` (material del suelo) | Terreno determinista con albedo, ediciones del terreno |
| **Simulación por niveles, entidades comunes, guardado** | §3.3, §6.3, §5.3 | `VarTable`/`VarSync`, `ShipSnapshot`, `finishShip` |
| **Ragdoll** | Huesos con cuerpos de Rapier (el esqueleto tiene 22 huesos), `Environment` para 0 g | El empuje del aire en la descompresión ya existe |
| **Erizos come-metal** | §6.3 (`Brain` + `Health`), §6.7 (`corrosion`), consulta de «metal cercano» | Integridad de paneles y piezas con reparación |
| **Inventario, crafteo, mercado** | Ítems (§6.6), contenedores, recetas como datos, precios por estación | Catálogo de componentes con masa, fabricante y talla |
| **Planetas esféricos procedurales** | §6.1, §6.2, quadtree de cubo-esfera, generadores por tipo | Workers de terreno, CDLOD con morph, generación determinista |
| **Estaciones y puestos** | Salas y esclusas como en las naves (una estación es una «nave» fija), acople, §6.8 | Todo el modelo de salas, aire, puertas y esclusa |
| **Flota de 10–15 y luego 30–40** | §6.9, LOD de naves (§2.3), materiales compartidos (§2.5) | Catálogo, `ShipBuilder`, validación, pruebas `test:ship` |
| **Batallas dinámicas** | §3.3 L2/L3 con **las mismas entidades** (una fragata a la deriva sigue ahí), §6.7 | Deshidratar una nave es guardar su `ShipSnapshot` |
| **Facciones, economía, NPCs con historia** | Entidades persistentes, reloj del mundo, planificador multifrecuencia | — |

---

## 8. Diseño de las piezas nuevas (resumen)

### 8.1. Marcos, entorno y precisión

```
Sistema (heliocéntrico, float64)
 └─ Cuerpo (gira)            ── Environment: gravedad μ/r², atmósfera, superficie, sol
     ├─ Superficie / región  ── burbuja de física (recentrado), terreno por chunks
     ├─ Órbita               ── naves L2 en raíles; L0/L1 integradas
     └─ Estación / nave      ── espacio propio (como ShipSpace hoy)
          └─ Nave anidada (hangar)
```

Render: `objeto.position = mundo − cámara` (float64 en la CPU), una pasada lejana para los cuerpos
celestes y otra cercana para la escena.

### 8.2. Terreno esférico

- Quadtree por cara de un **cubo-esfera** (6 raíces).
- Cada chunk tiene su origen local en su centro, con altura sobre el radio y normal radial.
- **Descarte por horizonte** además del frustum.
- Colisión: heightfields de Rapier orientados al plano tangente de cada baldosa, solo dentro de la
  burbuja.
- El CDLOD con morph, los workers y la generación determinista actuales se reutilizan cambiando la
  parametrización de (x, z) a (cara, u, v).

**Estado (29-09-2026): hecho.** `client/world/sphereTerrain.ts` es el único terreno (el de la base
se borró): cubo-esfera, CDLOD con morph y faldones, descarte por horizonte y frustum, prioridad en
los workers, detalle hasta ~0,33 m, texturas en coordenadas de nodo y sombras de horizonte horneadas
por nodo con el sol del momento. La colisión son heightfields en el marco tangente de la burbuja
(con los modificadores y las rocas grandes). Rocas y modificadores se indexan también en celdas del
cubo-esfera. Pendiente: la costura de las texturas de detalle en las aristas del cubo.

### 8.3. Vuelo

- `FlightEnv` pasa a ser el `Environment`: gravedad vectorial, «arriba» radial, AGL sobre la
  superficie, densidad y viento.
- Autopiloto con modos orbitales: circularizar, transferencia, aproximación y acople.
- LOD de vuelo:
  - **L0**: el modelo actual (asignador y contacto).
  - **L1**: masa puntual + actitud, con la envolvente de empuje calculada una vez.
  - **L2**: cónicas por tramos o rutas parametrizadas por tiempo.

### 8.4. IA

- **`Brain` → `Intent`** tanto para personajes como para naves (el `FlightCommand` y los modos del
  autopiloto ya son su «intención»).
- **Navegación a bordo**: el grafo de salas y aberturas (con el estado de puertas y escotillas) sirve
  para el camino global. Dentro de cada sala basta con dirección local.
- **Tareas diegéticas**: «ir a la consola X y accionar Y» usa el mismo `interact` que un jugador, así
  que un NPC pulsa los botones de verdad y el enclavamiento le puede decir que no.
- **Estratégica** (facciones, rutas comerciales): sobre agregados L2/L3, con el planificador
  multifrecuencia.

### 8.5. Guardado

- Tablas: `entities(id, kind, frame, components (por nombre), version)`, `terrain_edits(region, …)`,
  `world(clock, economía…)`.
- Guardado incremental de lo que cambió.
- Carga perezosa por región.

---

## 9. Hoja de ruta sugerida

### Fase 0: victorias rápidas (1–2 semanas, sin cambiar la arquitectura)

1. F3 honesto (§2.11) y una prueba de rendimiento en el repo (§11).
2. Rocas: culling + LOD + sombras (§2.6).
3. Culling de astronautas y de las piezas instanciadas de las naves (§2.1).
4. Sombras selectivas (§2.4).
5. Vuelo: masa cacheada, «dormida» antes, fase previa de contacto (§3.1).
6. `ShipView.update` sin basura, con marcas de sucio y `blocked()` a 20 Hz (§3.1).
7. Pantallas: visibles, cercanas y repartidas (§2.12).
8. HUD sin `innerHTML` por frame (§3.1).
9. Fuga de geometría de los cohetes (§3.1).
10. Probar `reversedDepthBuffer` (§2.9).

### Fase 1: cimientos (objetivo inmediato «bases de arquitectura»)

1. `Environment` + `CelestialBody`: quitar todo `1.62`, `MOON` y `−Y` (§6.1).
2. Árbol de marcos, origen flotante y render relativo a la cámara (§6.2).
3. Servidor local para el offline y partir `Game` (§6.5).
4. `Intent` + mapa de acciones (§6.4).
5. Entidades con ids globales, empezando por envolver lo que hay: `ShipSim`, `Crate`, `Member` (§6.3).
6. Replicación genérica, binaria y con interés (§4.2).
7. Guardado por nombre (§5.3).
8. Contenido del mundo a datos (§6.8).
9. Materiales por (zona × material) y compartidos, y portales (§2.2, §2.5).

### Fase 2: espacio

1. Terreno de cubo-esfera (§8.2).
2. Vuelo generalizado y autopiloto orbital (§8.3).
3. Cuerpos en raíles y reloj del mundo.
4. LOD de naves (§2.3) y de simulación L0/L1/L2 (§3.3).
5. Atmósfera exterior en soporte vital, vuelo y render (§6.11).

### Fase 3: NPCs, combate y eventos

1. `Brain` + navegación por salas (§8.4).
2. Islas de física en el servidor (§5.3).
3. `DamageEvent` y armas de nave (§6.7).
4. Ítems, herramientas y muestras (§6.6).
5. Pecios y director de eventos (§6.8).
6. Generador de naves (§6.9) para la primera tanda de modelos.

### Fase 4: medio y lejano plazo

Sobre lo anterior: criaturas, crafteo, mercado, facciones, batallas en abstracto, estaciones y
ciudades. Casi todo es contenido y simulación agregada una vez que existen entidades, niveles de
detalle y guardado.

---

## 10. Lo que está bien y conviene conservar

- **El núcleo de sistemas de nave** ([systems.ts](../src/shared/ship/systems.ts), [modules/api.ts](../src/shared/ship/modules/api.ts)):
  fases, redes que se resuelven por orden, enclavamientos, alarmas, servicios entre módulos sin
  nombrarse. Es exactamente lo que necesita el combate por subsistemas.
- **`VarTable` / `VarSync`**: estado plano, con nombre, cuantizado y replicado por diferencias. Es el
  modelo para replicar todo lo demás.
- **Naves como datos con validación** (`finishShip`/`checkShip`) y el **catálogo** de componentes con
  fabricantes y tallas.
- **Un mundo de física por nave** (`ShipSpace`) con gravedad aparente, y `Frames.transfer` para
  cambiar de marco sin saltos. Es la base del origen flotante.
- **Planificador de paso fijo** con tiempos por sistema (F3) y la regla «una funcionalidad = un
  sistema».
- **Luces de cabina por ranuras y recortadas a su sala**, **paneles fusionados** y **una sola malla
  de lámparas**: ya son optimizaciones buenas; hay que extenderlas, no sustituirlas.
- **Terreno determinista compartido** con CDLOD y workers.
- **La cultura de medir**: `test:ship`, los `diag:*` y la regla de «mirar las capturas».

---

## 11. Cómo se midió y cómo repetirlo

Los scripts se ejecutaron con `tsx` fuera del repo:

- **Vista**: `ShipView` se construye en Node, con `document`/`canvas` simulados que no dibujan. Se
  recorre el grafo contando mallas, grupos de material, `castShadow`, `frustumCulled`, materiales y
  claves de programa.
- **Tiempos**: 400 iteraciones tras calentar, en V8, el mismo motor que Chrome.
- **Basura**: `HeapProfiler.startSampling` del inspector de V8 con
  `includeObjectsCollectedByMinorGC/MajorGC`, agregando por función.
- **Mundo**: el quadtree se recorre con las reglas de [terrain.ts](../src/client/world/terrain.ts), las
  rocas con `rocksInTile` y los anillos de [rocks.ts](../src/client/world/rocks.ts). El worker se ejecutó
  en proceso, el GLB se leyó por su JSON y los mensajes se midieron con `JSON.stringify` de mensajes
  reales.

**Límites:**

- No hay medidas de GPU ni del navegador: los draw calls de §1.5 son estimaciones a partir del grafo
  de escena.
- El dibujo de las pantallas en canvas no se midió (el canvas simulado no dibuja).

**Propuesta:** convertir esto en `tools/perf/` con `npm run perf`, que falle si una nave pasa de un
umbral de draws, de ms por frame o de KB de basura por llamada. Igual que `test:ship`, pero para el
rendimiento, y con el F3 corregido (§2.11) para confirmar en el juego.
