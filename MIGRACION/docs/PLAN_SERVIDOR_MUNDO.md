# Plan: el servidor con el mundo dentro (migración de la arquitectura de la versión web)

Fernando (2026-10-07): «¿por qué no están sincronizadas? La posición se debería coger del mismo
servidor, y simularse en el servidor la posición de ese objeto»; «¿cómo entra en todo esto la idea
de diseño de todo el mundo, los informes de simulación del mundo?».

La versión web (TypeScript) ya tiene esa arquitectura: un **núcleo del mundo** aparte
([`../../docs/MUNDO.md`](../../docs/MUNDO.md), base: el informe «la simulación por dentro»), un
**servidor que tiene la verdad** de los objetos sueltos y de las personas, y **réplica por interés**
([`../../docs/RED.md`](../../docs/RED.md)). El prototipo Rust no la migró: su servidor solo reparte
mensajes y cada partida simula el mundo entero ([`MULTIJUGADOR.md`](MULTIJUGADOR.md)). Este
documento dice cómo llevarla a Rust, pieza a pieza, sin romper lo que funciona.

Es un plan, no un compromiso cerrado: cada fase termina con algo que se puede jugar y medir, y al
final de cada una se decide si se sigue igual.

**Sustituido en parte (2026-10-07):** las fases 0 a 5 las reemplaza
[`PLAN_AUTORITATIVO.md`](PLAN_AUTORITATIVO.md), en el que el servidor no vigila sino que decide
todo (también cómo se mueven las naves y los jugadores, con predicción en el cliente). Las fases
6 a 8 (núcleo del mundo, personas, galaxia) siguen valiendo y van encima de aquel.

---

## 0. Principios

1. **Una sola verdad.** De cada cosa decide un solo sitio: el servidor, o quien él diga (el
   jugador que maneja una caja, el piloto de su nave). Las demás máquinas predicen, siguen y
   dibujan.
2. **El cliente nunca espera.** Lo propio (tu cuerpo, la nave que pilotas, lo que llevas en la
   mano) se simula en tu máquina al momento. Lo de los demás se sigue suave (`follow`). Lo que
   decide el servidor se aplica cuando llega, sin saltos.
3. **El mundo es un núcleo aparte**, sin gráficos ni red ni plataforma, en su propio hilo y
   determinista (`MUNDO.md` §3): solo `now`, su azar por contador y sus tablas.
4. **Lo que no se ve no se simula pieza a pieza** (`MUNDO.md` §10): lo lejano es un número que se
   concreta al acercarse y vuelve a sumarse al alejarse, con conservación. «Si puedes calcular
   cuándo pasa algo, no compruebes si ha pasado.»
5. **Cada uno sabe lo que tiene cerca** (réplica por interés con histéresis), no todo.
6. **Datos antes que código; nada nuevo sin prueba; medir antes y después.** Ninguna dependencia
   nueva: red de `std`, hilos de `std`, `rayon` como ya hay.
7. **Ningún paso rompe el juego.** Cada fase convive con la anterior detrás de una opción, hasta
   que la nueva pasa las mismas pruebas.

---

## 1. Dónde estamos

### 1.1. Las dos versiones, pieza a pieza

| Pieza | Web (TypeScript) | Rust hoy | Falta |
|---|---|---|---|
| Núcleo del mundo (tiempo, cola, tablas, azar, registro de causas) | `src/sim/core/` (~1 500 líneas) | — | todo |
| Guardar y cargar la partida | `src/sim/persist/` (binario por columnas, dos ranuras, segmentos) | — | todo |
| Hilo del mundo con presupuesto | `src/sim/host/` (runner, protocolo, worker) | — | todo |
| Niveles de detalle con conservación, lugares | `src/sim/kit/` (`lod.ts`, `place.ts`) | `SimLevel` de estructuras (solo física, sin agregados; por el estado de cada una, nunca por quién mira) | los agregados |
| Módulos (jugadores, objetos, personas, lo que saben, «qué hay aquí») | `src/sim/modules/` | — | todo |
| Servidor | `src/server/room.ts` y cía.: simula cajas y NPC, decide daño | `crates/net/src/server*` + `crates/server`: solo reparte, ordena y da dueños | simular |
| Objetos sueltos del servidor | `server/objects.ts` (dueño = quien la maneja, si no reposa) | trozos y cajas locales de cada partida | todo |
| Réplica por interés | `shared/net/interest.ts` (histéresis, anfitriones, fijados, rejilla) | todos reciben todo | todo |
| Puente servidor ↔ mundo | `server/world.ts`, `worldObjects.ts`, `worldPeople.ts` | — | todo |
| Personas (andador, percepción, mentes) | `shared/actors/`, `server/npcs.ts`, `modules/people.ts` | `scene::Crowd` (decorado) | todo |
| Física de estructuras, daño, trozos, proyectiles | parcial (Rapier) | **`lunar-core`: completa y sin gráficos** | — |
| Naves como datos (máquinas, señales, mandos) | `shared/ship/` | **`lunar-ship`: completa y sin gráficos** | — |
| Reloj, marcos, red con pérdidas, seguir copias | `shared/time`, `shared/net/replica.ts` | **`lunar-net` + `app/multi`: hecho y probado** | — |
| Un solo sistema de proyectiles, daño con eco, linajes | — | **`app/blasts.rs`, `app/multi`: hecho** | moverlo al servidor |

### 1.2. El obstáculo de verdad

`lunar-core` y `lunar-ship` no tienen gráficos, así que el servidor podría usarlos ya. Pero **la
simulación de juego vive en `crates/app`**, que depende de `lunar-render`, `winit` y `egui`:

| Fichero de `app` | Líneas | Qué tiene de simulación | Qué tiene de presentación |
|---|---|---|---|
| `builds.rs` | 205 | estructuras del escenario, golpes, eco, lo que revienta | efectos de rotura |
| `blasts.rs` | 950 | `Launch`, proyectiles, guiados, finales, red | partículas, luces, cámara que sigue al misil |
| `ships.rs` | 330 | correr las naves, lo que revienta y arranca el aire | mallas, rótulos |
| `tactics.rs` | 274 | sensores, armas | — |
| `pilot/` | ~2 100 | el cuerpo del jugador: andar, mochila, marcos | — |
| `aboard.rs` | 840 | asientos, mandos, puertas, anclajes | avisos del HUD |
| `gear.rs` | 803 | herramientas (soldar, disparar) | su dibujo, chispas |
| `world.rs`, `content.rs` | 306 | el escenario y sus datos | instancias del renderizador |
| `multi/` | ~2 300 | todo | cuerpos de los demás |

Por eso la **fase 0** es sacar la simulación de `app` a un crate sin gráficos. Sin eso, el servidor
tendría que copiar código, y eso está prohibido por nuestras normas.

---

## 2. Adónde vamos

```
                ┌──────────────────────────── luna-servidor ────────────────────────────┐
                │                                                                       │
 jugadores ⇄ red │  lunar-net (sesiones, canal, interés)                                 │
                │        │ entradas, golpes pedidos            ▲ estados, altas, bajas    │
                │        ▼                                     │                          │
                │  lunar-play  ── el mundo físico, 60 Hz ── (estructuras, naves,          │
                │  (sin gráficos)                            proyectiles, trozos, cajas)  │
                │        │ hechos (disparo, robo, daño)        ▲ peticiones (qué hay,     │
                │        ▼                                     │ concreta / suma)         │
                │  lunar-world ── el mundo vivo, su hilo ── (entidades, cola, registro,   │
                │  (sin gráficos ni red)                     agregados, personas, guardado)│
                └───────────────────────────────────────────────────────────────────────┘

 cliente (lunar-app) = lunar-play (predice lo suyo, sigue lo ajeno) + render + audio + UI
 sin conexión        = el mismo servidor dentro del proceso (como `?offline` en la web)
```

| Crate | Qué es | Depende de | Nuevo |
|---|---|---|---|
| `lunar-core` | física, cuerpos, estructuras, proyectiles, efectos como datos | glam, serde, rayon | — |
| `lunar-ship` | naves como datos | core, signals, controls, machines | — |
| **`lunar-play`** | el juego sin gráficos: escenario, `Builds`, `Blasts` (sim), `Ships` (sim), tácticas, cuerpo del jugador, asientos, herramientas (sim), reglas de quién decide qué | core, ship | **sí (fase 0)** |
| `lunar-net` | transporte, canal, reloj, sesiones; **+ interés y réplica de entidades** | glam | ampliado |
| **`lunar-world`** | el núcleo del mundo: port de `src/sim` | serde (solo para defs) | **sí (fase 6)** |
| `luna-servidor` | proceso: red + `lunar-play` + `lunar-world` + puente + guardado | net, play, world | ampliado |
| `lunar-app` | cliente: presentación, predicción, entrada | play, render, net, audio | adelgaza |

---

## 3. Las fases

Cada fase: **qué**, **piezas y ficheros**, **protocolo**, **pruebas**, **hecho cuando**, **riesgos**
y **tamaño** (S: una sesión, M: unas pocas, L: muchas).

### Fase 0 — Separar la simulación de la presentación (`lunar-play`) · L

**Qué.** Un crate sin gráficos con todo lo que decide qué pasa; `app` se queda con lo que se ve, se
oye y se toca. Ni una línea de comportamiento cambia.

**Piezas.**
1. **Efectos como datos** (`lunar-core::effects`): separar lo que una explosión *hace* (`BlastDef`:
   energía, radio, cráter) de cómo *se ve* (partículas, destellos, sacudida). La simulación produce
   una lista de «cosas que se ven» (`Shown { kind, at, vel, scale }`) que el cliente pinta. Hoy
   `Blasts` llama a `fx.explode_*` y recibe el `BlastDef` a la vez: se parte en dos llamadas.
2. **`Builds`** entera a `lunar-play` (no dibuja nada salvo efectos de rotura: pasan a `Shown`).
3. **`Blasts`** en dos: `Shots` (sim: `Launch`, rondas, misiles, guiados, señuelos, finales, `Seen`)
   en `lunar-play`; `BlastsView` (partículas, luces, estelas, cámara tras el misil) en `app`.
4. **`Ships`**: la simulación (`run`, lo que revienta, lo que arranca el aire, articulaciones) a
   `lunar-play`; las mallas, rótulos y pantallas siguen en `app` (`ShipViews`, indexadas igual).
5. **`Pilot`** (`pilot/`) a `lunar-play` tal cual: ya no toca gráficos.
6. **`Aboard`**: la lógica (asientos, mandos, puertas, anclajes, órdenes de vuelo) a `lunar-play`;
   el HUD y los avisos se quedan (salen como una lista de avisos, igual que `Shown`).
7. **`Gear`**: qué hace una herramienta (soldar, disparar, cargar) a `lunar-play`; cómo se coge y
   se ve, en `app`.
8. **`Tactics`**, **`content::Defs`** (la carga de datos) y el escenario (`world.rs`, la parte que no
   escribe instancias) a `lunar-play`.
9. **`Play`** (`lunar-play::Game`): el bucle de un paso de simulación, el mismo que hoy está repartido
   por `play.rs` (recibir, naves, mundo, proyectiles, mandar), con una entrada (`Input`: lo que pide
   el jugador este paso) y una salida (`Frame`: lo que se ve, se oye y se avisa).

**Pruebas.** Todas las de hoy siguen pasando sin tocarlas (se mueven de crate). Una nueva,
`play_headless`: el escenario corrido 60 s sin ventana da el mismo resumen (`digest`: posiciones de
estructuras, vida de cada pieza, estado de cada nave) que corrido dentro de `app`.

**Hecho cuando** `lunar-play` compila sin `lunar-render`, `winit` ni `egui`, y `app` es solo
presentación más entrada.

**Riesgos.** Lo más enredado es `play.rs` (1 930 líneas) y `Gear`/`holding`/`handwork` (la mano
mezcla qué hace y cómo se ve). Se hace por trozos, uno por commit, con las pruebas en verde en
cada uno.

### Fase 1 — Un servidor que simula a la sombra · M

**Qué.** `luna-servidor` corre `lunar-play` con el mismo escenario que los clientes, a 60 Hz,
**sin mandar nada todavía**: aplica lo que los clientes cuentan (mandos, golpes con eco,
lanzamientos) y compara su mundo con el de ellos.

**Piezas.**
- `crates/server`: carga los datos (`Defs`) con la carpeta `assets/defs` al lado del ejecutable;
  bucle de paso fijo con presupuesto (como el `runner` web: 60 Hz, aviso si un paso pasa de 8 ms).
- El servidor es un «jugador sin cuerpo» para `Multi`: recibe todo lo que se dice.
- `Server::digest()`: cada segundo, el resumen del mundo; los clientes mandan el suyo (un
  mensaje nuevo, `Digest`, suelto) y el servidor anota las diferencias en el registro.

**Pruebas.** La mesa de pruebas (`multi/tests.rs`) con el servidor simulando: tras cada prueba, el
resumen del servidor es igual al de los clientes (lo que ya está sincronizado: daño, trozos).

**Hecho cuando** el servidor aguanta el escenario con 8 jugadores sin pasarse del presupuesto y sus
resúmenes coinciden con los de los clientes en todo lo compartido.

**Riesgos.** CPU: el servidor simula las naves de todos. Hay que medir: naves activas por núcleo.
Nada se simula distinto por quién mira (`MULTIJUGADOR.md`, «El mundo no depende de quién mira»):
todo a paso completo; lo que reposa, la física no lo mueve.

### Fase 2 — Objetos sueltos del servidor, réplica por interés · L

Esto arregla lo que preguntaste: **dónde está cada trozo, cada caja, cada bidón lo dice el
servidor.**

**Qué.** Los objetos sueltos (trozos que se desprenden, cajas, bidones, piezas de repuesto) son del
servidor, como en la web (`server/objects.ts`):

- **dueño**: quien lo maneja (lo lleva en la mano, lo arrastra, lo tiene en un anclaje que mueve)
  lo simula en su máquina y cuenta dónde está; si nadie, el servidor;
- **reposo**: cuando se para, el servidor apunta dónde quedó y deja de simularlo (no cuesta nada);
- **alta y baja**: el servidor lo da de alta a quien está cerca (`Spawn`) y de baja a quien se
  aleja (`Gone`), con histéresis (1 500 m para aprender, 1 800 m para olvidar, o a bordo del mismo
  anfitrión, o fijado: lo que tú simulas lo sabes aunque vuele lejos).

**Piezas.**
1. **Interés** en `lunar-net` (port de `shared/net/interest.ts`, 258 líneas): `Replication<T>` con
   `where`, `frame` y `pinned`; rejilla por posición del mundo; coste proporcional a lo cercano.
2. **Entidades replicadas** en el protocolo: `Spawn { kind, id, estado }`, `Gone { kind, ids }`,
   estados en lotes por interés. Un tipo nuevo que se replica es una entrada en una tabla (como en
   `RED.md`: «una clase nueva que se replica»).
3. **Trozos**: hoy nacen igual en todas las partidas (linaje, `Named::Piece`); pasan a nacer **en el
   servidor** y llegar por `Spawn` a quien está cerca. El cliente que los ve nacer antes de que
   llegue el alta (porque predijo el golpe) los dibuja como copia provisional, y la casa con la del
   servidor por su linaje: así no hay retraso visible. El linaje que ya existe es justo lo que hace
   falta para casarlos.
4. **Carga suelta** (`lunar-core::structure::hold`, cajas y bidones del escenario): la misma
   réplica. Coger una caja = pedir su llave (`Claim`); soltarla = devolverla.
5. **Niveles de detalle**: nunca por quién mira (el mundo es el mismo esté quien esté): todo a
   paso completo, y lo que reposa no se mueve. Un atajo para lo lejano solo si da lo mismo, o si
   lo decide el estado de lo abreviado, igual en todas las partidas.

**Pruebas.** Port de `tools/net/check.ts` («réplica por interés contra fuerza bruta»): mil objetos,
jugadores moviéndose al azar; cada uno sabe exactamente lo que la regla dice, y nada parpadea en el
borde. Mesa de pruebas: el cohetazo que arranca trozos, con un tercer jugador lejos que se acerca
después y los ve donde están. Pérdidas de red: un `Gone` perdido no deja fantasmas.

**Hecho cuando** dos jugadores ven cada trozo y cada caja en el mismo sitio (a lo que permiten los
relojes) para siempre, y un jugador que llega tarde los ve donde están.

**Riesgos.** Ancho de banda tras una gran explosión (decenas de trozos): los trozos pequeños y
lejanos van más despacio (prioridad por tamaño y distancia, como ya prevé `MULTIJUGADOR.md`).

### Fase 3 — El daño lo decide el servidor · M

**Qué.** Hoy el daño va «con eco»: lo decide la partida que acierta y todas lo aplican en el orden
del servidor. Pasa a decidirlo **el servidor**, que tiene la verdad de dónde está cada cosa.

**Piezas.**
1. **Los proyectiles van al servidor**: un `Launch` llega al servidor, que lo vuela (`Shots` de
   `lunar-play`, el mismo código) y decide dónde acaba. Los clientes siguen volando su copia para
   verla al momento (ya lo hacen) y acaban donde dice el servidor (ya lo hacen: `Seen::End`).
2. **Compensación de retraso**: el que dispara vio el blanco hace ~100 ms. El servidor guarda la
   pose de cada estructura de los últimos 300 ms (anillo, ya hay `Sweep` con poses inicial y final)
   y vuela el disparo **desde el momento en que se disparó** («a favor del que dispara», como casi
   todos los juegos de disparos). Decisión para Fernando: cuánto retraso se perdona (propuesta:
   hasta 250 ms).
3. **`STRIKES` lo manda el servidor**, no los clientes: desaparece `tell_all` para el daño. La
   postura con que se aplica ya no hace falta mandarla (es la del servidor), pero se mantiene en
   el mensaje para que el cliente lo aplique igual a su copia.
4. Lo que revienta, lo que arranca el aire, los trozos: todo en el servidor.

**Pruebas.** Las de hoy (`every_weapon_in_the_data_…`, el cohete desde la bodega, el guiado) con
el servidor decidiendo: mismo resultado, y además el resumen del servidor coincide. Una nueva: un
cliente tramposo que dice que acertó no hace nada.

**Hecho cuando** ningún cliente decide daño.

### Fase 4 — Las naves, del servidor (con predicción para el piloto) · L

**Qué.** El estado de cada nave (posición, máquinas, señales, depósitos, articulaciones) lo tiene
el servidor. El piloto manda sus mandos y **predice** su nave; los demás la siguen.

**Piezas.**
1. **Entradas, no estados**: el piloto manda lo que hace con los mandos en cada paso, sellado con
   su paso (`StepClock`); el servidor los aplica en ese paso.
2. **Predicción y corrección**: el piloto simula su nave al momento; cuando llega el estado del
   servidor (con el paso al que corresponde), compara con lo que predijo para ese paso y, si se
   desvió, corrige suave (`follow::steer`, ya existe) en vez de rebobinar. Rebobinar una nave con
   miles de piezas no es viable; corregir suave sí, y es lo que ya se hace con las copias.
3. **Estado de las máquinas al entrar tarde**: llega solo (el servidor manda la foto de la nave:
   `lunar-ship::sync`, que ya existe sin enchufar). Pendiente de hoy que se cierra.
4. **Paso intermedio recomendado**: antes de la predicción, mantener la autoridad del piloto sobre
   *cómo se mueve* su nave (como hoy) y dar al servidor la de *sus sistemas y su daño*. Ya da casi
   todo lo bueno con poco riesgo.

**Pruebas.** Las de vuelo de hoy (0–7 800 m/s, 30–240 fps, red mala) con el servidor al mando; las
de coherencia de naves en el servidor; un jugador que entra a mitad de partida ve cada nave con sus
máquinas como están.

**Riesgos.** Es la fase más delicada para la sensación de pilotaje. Se mide con la sonda de
movimiento: ni un salto por encima de lo que da hoy.

### Fase 5 — Los jugadores: el cuerpo es tuyo, el servidor vigila · S

**Qué.** Cada jugador sigue simulando su cuerpo (como en la web y como hoy). El servidor comprueba
que lo que cuenta es posible (velocidad, atravesar paredes, gas de la mochila) y corrige si no.

**Piezas.** `lunar-play::Pilot` en el servidor para cada jugador, solo para comprobar; un aviso
`Correct` raro (no cada paso).

### Fase 6 — El núcleo del mundo en Rust (`lunar-world`) · L

**Qué.** Port de `src/sim` (~3 600 líneas de TypeScript), módulo a módulo, con sus pruebas. Es la
base del mundo vivo (`MUNDO_VIVO.md`).

**Orden y correspondencia.**

| Paso | Web | Rust | Notas |
|---|---|---|---|
| 1 | `core/rng.ts` (83) | `rng.rs` | azar por contador, `hash(clave, n)`, `seed_of`; solo enteros: idéntico bit a bit |
| 2 | `core/store.ts` (302) | `store.rs` | componentes como columnas (`Vec` por campo); ids que nunca se reutilizan; un macro `component!` en vez de `defineComponent` |
| 3 | `core/queue.ts` (329) | `queue.rs` | montículo con orden total (tiempo, orden del tipo, destino, orden de programación); handles estables, `move`, cancelar |
| 4 | `core/calendar.ts`, `lazy.ts` (192) | `calendar.rs`, `lazy.rs` | formas cerradas: valor a cualquier hora y cuándo cruza un umbral |
| 5 | `core/chronicle.ts` (160) | `chronicle.rs` | registro solo-añadir con causas; `chain`, `effects`, `about` |
| 6 | `core/world.ts`, `symbols.ts` (383) | `world.rs` | `World`: junta todo; `SimModule` es un `trait` (componentes, eventos, peticiones, `describe`, indicadores) |
| 7 | `persist/` (512) | `persist/` | binario por columnas, dos ranuras, segmentos del registro, a prueba de cortes, versiones por nombre; sin gzip externo (formato propio sencillo o sin comprimir: decidir) |
| 8 | `host/` (300) | `host.rs` | un hilo de `std` con presupuesto; canal de mensajes (`mpsc`) en vez de worker |
| 9 | `kit/place.ts`, `lod.ts` (397) | `kit/` | lugares y agregados con conservación e histéresis |
| 10 | `tools/seismograph.ts` | `tools/` | indicadores en el tiempo; herramientas de consola (`sim-why`, `sim-scope`, `sim-seeds`) como binarios |
| 11 | `modules/players, objects, here, names` | `modules/` | jugadores por nombre, objetos guardados (enlaza con la fase 2), «qué hay aquí» |
| 12 | `modules/people, knowledge` | `modules/` | dotaciones, mentes, recuerdos y rencores (con la fase 7) |

**Reglas que se conservan tal cual** (`MUNDO.md`): solo `now`, el azar por contador y las tablas;
«si puedes calcular cuándo, prográmalo»; el presupuesto no cambia la historia; guardar y cargar no
cambia la historia; un `trait` nuevo es un módulo nuevo y el núcleo no conoce ninguno por su
nombre.

**Pruebas.** Port de `tools/sim/check.ts` caso a caso (azar, cola contra referencia, tablas contra
`HashMap`, formas cerradas contra paso a paso, determinismo a trozos con presupuestos al azar en 20
semillas, guardar/cargar, partidas dañadas, versiones, causas). `bench`: eventos por segundo con
100 000 entidades (la web da ~2,6 M/s; Rust debería dar bastante más).

**El puente** (`server/world.ts` → `luna-servidor/bridge.rs`): cada segundo dice a cada enlace dónde
están los jugadores; los enlaces piden y aplican sin esperar dentro de un paso. Primer enlace:
objetos (lo que la fase 2 deja en reposo lo guarda el mundo y sigue ahí tras reiniciar).

### Fase 7 — Personas · M

Port de `shared/actors/walker.ts` (andador sobre cualquier suelo), `perception.ts` (vista y oído
según el medio: aire, estructura, suelo, vacío) y `server/npcs.ts`. Las dotaciones del mundo se
concretan cerca de los jugadores; los testigos reaccionan; «qué hay aquí». Los cuerpos ya se
dibujan como a cualquier jugador (`multi::bodies`).

### Fase 8 — Escala: la galaxia, el salto y el mundo vivo · L

`galaxy.ts`, `jump.ts` y, después, la economía, las facciones y las consecuencias de
`MUNDO_VIVO.md`, ya sobre `lunar-world`.

---

## 4. El protocolo

Una versión nueva por fase (el servidor rechaza con su motivo a quien llegue con otra, como hoy).

| Mensaje | Dirección | Fiable | Fase | Qué |
|---|---|---|---|---|
| `Digest` | ⇄ | no | 1 | resumen del mundo cada segundo (diagnóstico) |
| `Spawn { kind, id, estado }` | S→C | sí | 2 | algo entra en tu interés |
| `Gone { kind, ids }` | S→C | sí | 2 | sale de tu interés o deja de existir |
| `States { kind, t, [id, estado] }` | S→C | no | 2 | lotes de estados por interés, sellados con su paso |
| `Claim` / `Release` | C→S | sí | 2 | ya existen: pedir y soltar una cosa |
| `Launch` | C→S | sí | 3 | lo que disparas (hoy va a los demás; pasa al servidor) |
| `Strikes` | S→C | sí | 3 | el daño decidido, con su semilla y postura |
| `Input { step, mandos }` | C→S | no (redundante: los últimos 3) | 4 | lo que haces con los mandos |
| `ShipSync` | S→C | sí | 4 | la foto entera de una nave al entrar en tu interés |
| `Correct` | S→C | sí | 5 | tu cuerpo no podía estar ahí |
| `World*` | S→C | sí | 6–8 | lo que el mundo cuenta: hechos, «qué hay aquí», avisos |

---

## 5. Presupuestos y medidas

| Qué | Objetivo | Cómo se mide |
|---|---|---|
| Paso del servidor (60 Hz) | p95 < 8 ms con el escenario y 8 jugadores | contador por paso en el registro del servidor (como `SimStatus`) |
| Mundo vivo | ≤ 4 ms por paso en su hilo, sin frenar al físico | `SimStatus.lag`, p95 |
| Bajada por jugador | < 64 kB/s en combate, < 8 kB/s en calma | `tests/bandwidth.rs` ampliado |
| Subida por jugador | < 8 kB/s | idem |
| Trozos tras una gran explosión | 100 trozos en interés sin pasarse de la bajada | prioridad por tamaño y distancia |
| Lo que cuesta un objeto en reposo | casi nada por paso | lo que reposa no se mueve (`resting`) |
| Partida guardada | 100 000 entidades en < 20 MB | `bench` del mundo |

Todo lo nuevo entra en `OPTIMIZACION.md` con su medida, como siempre.

---

## 6. Pruebas de todo el camino

- **La mesa de pruebas** (`multi/tests.rs`: varias partidas enteras contra el servidor real en una
  red en memoria que pierde, duplica y retrasa) crece con el servidor simulando, y es la referencia
  de cada fase.
- **Resúmenes (`digest`)** en el servidor y en cada cliente: lo compartido debe coincidir siempre.
- **Bots** sin ventana (`lunar-play` + un guion de entradas): 8, 32, 64 jugadores contra un
  servidor real por UDP, para medir CPU y ancho de banda (`load_for_a_running_server` ya existe en
  pequeño).
- **El mundo**: las pruebas de `MUNDO.md` §9, portadas.
- **Nada se queda sin prueba**: cada tipo replicado, cada mensaje (ida y vuelta, cortado,
  basura), cada regla de autoridad (un cliente tramposo no consigue nada).

---

## 7. Lo que ya está hecho y se aprovecha

- **Un solo sistema de proyectiles** (`Launch`, `Seen::Launch`/`End`/`Track`): pasa tal cual al
  servidor; los clientes ya saben volar copias que acaban donde se les dice.
- **Linajes** (`Structure::lineage`, `Named::Piece`): el nombre de cada trozo, que es lo que hace
  falta para casar la copia provisional de un cliente con la del servidor.
- **Golpes con postura** (`Structures::posed`) y la regla de quién decide lo que revienta.
- **Reloj compartido, marcos, `follow::steer`** con la aceleración del dueño: lo mismo que usará
  la predicción del piloto.
- **Dueños por llaves** (`Claim`/`Release`, el anfitrión) y canales fiables en orden.
- **Pruebas**: la mesa de pruebas, la prueba de todas las armas, las de vuelo: siguen valiendo y
  hacen de vara de medir.

---

## 8. Decisiones para Fernando

1. **Quién mueve las naves**: el piloto (como hoy) con el servidor al mando de sistemas y daño
   (recomendado primero), o el servidor del todo con predicción (fase 4 completa).
2. **Retraso que se perdona al disparar** (compensación): propuesta, 250 ms.
3. **Sin conexión**: el servidor dentro del proceso del juego (recomendado: una sola forma de
   jugar, como `?offline` en la web) o el juego de un jugador como hoy.
4. **Compresión de las partidas**: sin dependencias nuevas, o una pequeña (deflate) por tamaño.
5. **Orden**: el recomendado es 0 → 1 → 2 → 3 → (6 en paralelo con 4) → 4 → 5 → 7 → 8. La fase 2
   es la primera que se nota jugando (trozos y cajas en el mismo sitio para todos).

---

## 9. Resumen

| Fase | Qué se nota | Tamaño |
|---|---|---|
| 0 | nada (orden por dentro) | L |
| 1 | nada (el servidor aprende a simular y vigila) | M |
| 2 | **trozos, cajas y bidones en el mismo sitio para todos; se ve lo cercano** | L |
| 3 | nadie puede hacer trampas con el daño | M |
| 4 | las naves y sus máquinas, iguales para quien entra tarde | L |
| 5 | nadie atraviesa paredes por trampa | S |
| 6 | el mundo recuerda (lo que dejas sigue ahí) | L |
| 7 | gente que vive, ve y recuerda | M |
| 8 | la galaxia y el mundo vivo | L |
