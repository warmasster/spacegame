# El núcleo del mundo

La simulación del mundo (economía, facciones, cohortes, mentes de NPC…) va en un **núcleo aparte**,
sin gráficos ni física ni plataforma (`src/sim`), en su propio hilo. El motor 3D solo le pregunta qué
hay aquí y le dice qué ha hecho el jugador. Así el mundo puede simular siglos en segundo plano,
probarse en consola con miles de semillas y sobrevivir a un cambio de motor. Base: el informe «la
simulación por dentro».

Este documento dice **qué garantiza** el núcleo, **cómo se escribe un sistema** para él y **cómo se
comprueba**. Pruebas: `npm run test:sim`, `test:sim:server`, `test:world`, `test:npcs`, `test:actors`,
`bench:sim`; herramientas: el historiador `sim:why`, el barrido de semillas `sim:seeds` y el
sismógrafo `sim:scope`.

## 1. Piezas

| Pieza | Archivo | Qué hace |
|---|---|---|
| Mundo | `core/world.ts` | Junta todo: entidades, tablas, cola, registro, azar y tiempo. `World.create` (nuevo) / `restore` (cargado) |
| Módulos | `modules/index.ts` | Los sistemas del mundo (`SimModule`), en orden. Aquí entra el contenido |
| Tablas | `core/store.ts` | Componentes como estructuras de arrays (`defineComponent`), índice id → fila por páginas |
| Tiempo | `core/queue.ts`, `core/calendar.ts` | La línea de tiempo (montículo sobre arrays tipados), el calendario, el reloj de juego |
| Perezoso | `core/lazy.ts` | Formas cerradas: valor a cualquier hora y cuándo cruza un umbral |
| Azar | `core/rng.ts` | Por contador: `hash(clave, n)`; `seedOf(mundo, grupo, índice)` |
| Registro | `core/chronicle.ts` | Solo añadir; cada hecho apunta a sus causas |
| Guardado | `persist/` | Binario por columnas, segmentos del registro, dos ranuras, gzip |
| Hilo | `host/runner.ts`, `host/client.ts`, `host/node/` | Corre el mundo con presupuesto; hechos, peticiones y consultas por mensajes |
| Kit | `kit/place.ts`, `kit/lod.ts` | Dónde está cada cosa (y qué hay cerca); agregados que se concretan y se vuelven a sumar, con conservación (§10) |
| Herramientas | `tools/seismograph.ts` | Los indicadores de los módulos en el tiempo de juego (§13) |
| Contenido | `modules/` | Jugadores, objetos sueltos y almacenes, personas y dotaciones, lo que saben y sienten, «qué hay aquí» |

El núcleo (`core/`, `persist/`, `host/`, `kit/`, `tools/`) no importa nada de `client`, `server`,
`engine` ni `shared`. Los módulos de contenido (`modules/`) pueden leer datos puros del juego
(`src/shared`: sitios, catálogos). `host/` importa del núcleo; nunca al revés. El motor habla con el
mundo desde `src/server/world*.ts` (§10).

## 2. Tiempo

- **Tiempo de juego** = segundos desde el nacimiento del mundo (float64: exacto de sobra para miles
  de años). El **calendario** (`Calendar`, por defecto días de 24 h y años de 365) solo lo nombra:
  `w.date()` → «año 3, día 46, 08:05». Lo que hable de días o años pregunta a `w.calendar`.
- **Escala**: el hilo lleva el mundo a su reloj (`WorldClock`), por defecto **×72: un día de juego en
  20 minutos reales**. Cambiar la escala no hace saltar el tiempo; 0 lo pausa.
- **La cola** (`w.schedule(t, tipo, destino, a, b, datos)`, `w.after(dt, …)`) guarda lo que va a pasar
  a una hora conocida. Orden total, igual en cada ejecución: **tiempo → `order` del tipo → id del
  destino → orden en que se programó**. Programar, cancelar, mover y sacar cuestan O(log n).
- **Regla de oro** (del informe): si puedes calcular cuándo va a pasar algo, **no compruebes si ha
  pasado**. Calcula la fecha, programa el evento y guarda su **handle** en una tabla; si cambia algo
  (una reparación, más carga), `w.move(handle, nuevaHora)` (Infinity lo cancela). Los handles
  sobreviven a guardar y cargar.
- **Perezoso**: una cantidad guarda valor, ritmo y hora (`linearFields('comida')` = tres columnas;
  `LinearField` para leer y escribir). Se lee a cualquier hora sin escribir. Se escribe solo en
  eventos, y `when(umbral)` da la hora del evento. También hay `approachAt/When` (un ánimo o un rencor
  que se apaga) y `compoundAt` (una deuda con interés). Para umbrales, redondea la hora hacia arriba
  (`Math.ceil`): el valor ya lo ha cruzado cuando llega el evento.
- **Presupuesto**: `w.advance(hasta, ms)` corre los eventos en orden y para cuando gasta su tope
  (pasados 32 eventos). Lo que no cabe espera al siguiente tick: el mundo lejano puede ir un poco
  tarde (`status.lag`). **La historia no depende de dónde paró.**

## 3. Determinismo

Con la misma semilla, los mismos módulos y los mismos hechos, la historia es **idéntica**, se corra de
una vez o a trozos con cualquier presupuesto, y antes y después de guardar y cargar. Se comprueba
comparando la huella de todo el mundo (`digest(w)`).

Para no romperlo, el código de un módulo usa **solo**:

- `w.now` (nunca `Date`, `performance`, temporizadores);
- `w.random(id)` (el flujo de esa entidad: su contador se guarda en la tabla `$rng`) o `Rng`/`draw`
  con claves de `seedOf` (nunca `Math.random`);
- las tablas, la cola y el registro (nada de estado en variables del módulo, cachés aparte, ni orden de
  un `Map`/`Set` que no venga de ellas).

El azar mezcla solo enteros: es idéntico en cualquier motor JS. `Math.log/exp/sin` (y por tanto
`Rng.normal`, `approach*`, `compound*`) son reproducibles en el mismo motor (el servidor), no bit a bit
entre motores distintos.

«Se genera igual al volver» (el informe): lo que sale al acercarte se genera con
`new Rng(seedOf(w.seed, grupo, índice))`, sin estado, y sale igual cada vez.

## 4. Entidades y tablas

- `w.spawn()` da un id nuevo (u32, **nunca se reutiliza**: el registro habla también de los muertos).
  `w.despawn(id)` lo quita de todas las tablas.
- `defineComponent('colonia', { pob: 'u32', ...linearFields('comida'), hambruna: 'f64' })`. Los tipos
  son `f64 f32 i32 u32 u16 u8 i8 bool`, `ref` (id de entidad), `sym` (cadena internada, `w.syms.id`) y
  `time`. Cada campo admite `{ type, default }`.
- `const t = w.table(Colonia)`: `t.add(id, {...})`, `t.get/set`, `t.remove`. Para recorridos rápidos,
  las columnas: `for (let r = 0; r < t.count; r++) t.c.pob[r]…` (`t.ids[r]` es el id). Las columnas se
  sustituyen al crecer: léelas después del último `add`. Quitar mueve la última fila al hueco;
  recorre hacia atrás si quitas mientras recorres.
- Lo único del mundo (un precio global…) va en la fila del id 0 de su tabla.
- **Relaciones** (quién debe a quién): tablas de entidades-relación con dos `ref`, o listas enlazadas
  en columnas. Si una consulta necesita un índice, se construye en `loaded` desde las tablas.

## 5. Registro de causas

- `w.record('hambruna', actor, sujeto, a, { data })` escribe un hecho a la hora actual. Su **causa** es
  la actual: el hecho que provocó el evento que se está manejando. El hecho nuevo **pasa a ser la causa**
  de lo que ese evento haga después (`keep: true` para que no lo sea). `also: [...]` añade más causas.
- Los eventos heredan la causa del momento en que se programan, y `move` la cambia por la del momento
  en que se mueven. Así la cadena se construye sola: *colonia abandonada ← hambruna ← saqueo del
  jugador*.
- **Hechos de fuera** (el jugador, el motor): `w.fact('robo', jugador, caja, cantidad, datos)`, o
  desde el motor `sim.fact(...)`. Queda un registro sin causa (la raíz) y, si algún módulo maneja ese
  tipo, un evento para `sujeto` con ese registro como causa.
- Solo se registra lo que importa (el informe: cambio de dueño, daño, delito…). El resto se resume
  en tablas.
- Consultas: `w.log.chain(id)` (hacia atrás hasta la raíz), `effects(id)` (hacia delante),
  `about(entidad)`. Herramienta: `npm run sim:why -- <id|e<entidad>|recent>`.

## 6. Guardar y cargar

- **Qué se guarda**: todo lo que decide el futuro, tal cual. Tablas (por columnas), la cola con sus
  ranuras (los handles siguen valiendo), contadores (ids, orden de la cola, flujos de azar), símbolos,
  tipos de evento y el registro. Un mundo cargado sigue **exactamente** como habría seguido el guardado.
- **Dónde**: `data/world/` (servidor; `WORLD_DIR` para otra carpeta, `WORLD_DIR=none` para no guardar).
  - `world-a.sim` y `world-b.sim`: dos ranuras que se escriben por turnos. Cabecera JSON + columnas +
    checksum, en gzip.
  - `log-NNNNNN.seg`: un segmento por trozo lleno del registro. Se escriben **una vez**: cada guardado
    solo añade los nuevos.
- **Seguro ante cortes**: primero los segmentos, luego la ranura más vieja. Cada archivo se escribe a
  uno temporal, se vuelca a disco y se renombra. Si la última partida está dañada (checksum) o le falta
  un segmento, se carga la anterior. Si están dañadas las dos, error: **nunca se pisa** una partida.
  Un mundo nuevo no se guarda encima de otro.
- **Versiones**: se casa por nombre.
  - Un campo nuevo toma su valor por defecto; uno quitado se descarta; uno de otro tipo se convierte.
  - Una tabla que ya no declara ningún módulo se conserva tal cual y se vuelve a guardar; si un módulo
    la declara más tarde, la recoge.
  - Los eventos de un tipo sin módulo se conservan y se ignoran al llegar.
  - Todo esto sale en el informe de carga (en el registro del servidor).
  - Una partida de un formato más nuevo se rechaza.
- **Cuándo**: al crear el mundo, cada 60 s reales si ha cambiado algo, y al cerrar el servidor (Ctrl+C,
  SIGTERM, cerrar la ventana).
- **Un proceso por carpeta** (`lock`): un segundo servidor sobre el mismo mundo corre sin guardar y lo
  avisa. Un candado de un proceso muerto se recupera solo.
- **Datos libres** (`data` de eventos y hechos): deben ser JSON. Para lo demás, tablas.

## 7. El hilo

- El servidor arranca el mundo en un **worker de Node** (`launchWorld`, `host/node/`). El hilo
  principal (red, naves, cajas) no paga nada. El juego no lo espera: si el mundo no arranca, el
  servidor sigue sin él.
- Cada tick (60 Hz) el runner lleva el mundo a su reloj con un tope de CPU (**4 ms** por defecto,
  `SimConfig.budgetMs`). Cada segundo envía su estado (`SimStatus`: fecha, entidades, eventos
  pendientes, registros, retraso, p95 del tick frente al presupuesto, guardados). Además guarda cada
  tanto.
- **Motor → mundo**: `sim.fact(tipo, actor, sujeto, a, datos)`, `sim.setScale(x)`, y las **peticiones**
  de los módulos: `await sim.ask('objects.observe', …)` (cada módulo declara las suyas en
  `SimModule.requests`; contestan al momento y pueden cambiar el mundo, como un hecho).
- **Mundo → motor**: `await sim.query({ q: 'status' | 'digest' | 'record' | 'chain' | 'about' | 'recent' |
  'gauges' | 'series' })`, `sim.save()`, `sim.stop()`, `sim.on(mensaje)`, y los **avisos** de los módulos:
  `w.emit('npc.task', …)` en el mundo → `sim.onEmit('npc.task', fn)` en el motor (tras cada tick).
- `runner.ts` y `client.ts` no saben de Node: el runner recibe un puerto (`post`/`listen`) y un
  almacenamiento (`SaveStorage`: `read/write/remove/list`). El modo sin conexión del navegador será un
  `Worker` con el mismo runner y un almacenamiento sobre IndexedDB (~60 líneas).

## 8. Un sistema nuevo

```ts
// src/sim/modules/granjas.ts
const Granja = defineComponent('granja', { ...linearFields('grano'), cosecha: 'f64' });

export const granjas: SimModule = {
  name: 'granjas',
  components: [Granja],
  events: [{
    name: 'granja.cosecha',
    run(w, e) {
      const grano = new LinearField(w.table(Granja), 'grano', 0);
      grano.add(e.target, w.now, 100 * (0.5 + w.random(e.target)));
      w.table(Granja).set(e.target, 'cosecha', w.after(30 * w.calendar.day, 'granja.cosecha', e.target));
    },
  }],
  create(w) { /* crear granjas con seedOf(w.seed, …) y programar su primera cosecha */ },
};
// y añadirlo a WORLD_MODULES en src/sim/modules/index.ts
```

| Quiero… | Hago |
|---|---|
| Datos por entidad | `defineComponent` y declararlo en `components` |
| Que pase algo a una hora | Un `EventDef` en `events` + `w.schedule`; guardar el handle si puede cambiar |
| Algo periódico | Que el evento se reprograme (`w.after`); repartir la fase con `stagger(clave, periodo)` |
| Una cantidad que cambia sola | `linearFields` + `LinearField`; su umbral, un evento con `when` |
| Que el jugador influya | `sim.fact(...)` en el motor + un evento con ese nombre en el módulo |
| Explicar por qué pasó | `w.record(...)` en los momentos que importan; las causas se enlazan solas |
| Que el motor le pregunte algo | Una entrada en `requests` (nombre con prefijo del módulo) |
| Decirle algo al motor | `w.emit(canal, datos)`; en el servidor, un enlace escucha `sim.onEmit(canal)` |
| Que aparezca en «qué hay aquí» | `describe(w, id)` devuelve qué es (o null si no es suyo) |
| Verlo en el sismógrafo y en el barrido | `gauges: { 'modulo.cosa': (w) => número }` |
| Algo que se concreta al acercarse | Un agregado de `kit/lod.ts` con un `Place` (§10) |
| Comprobarlo | Un caso en `tools/sim/check.ts` (o un escenario como `tools/sim/scenario.ts`) |

## 9. Medir

- **`npm run test:sim`** (`tools/sim/check.ts`) cubre:
  - azar: uniformidad, independencia entre tiradas y entre entidades vecinas;
  - la cola contra una referencia ordenada (200 000 operaciones);
  - las tablas contra un `Map`, las formas cerradas contra paso a paso, y el calendario;
  - **determinismo**: un mundo de colonias con hambrunas, nacimientos, muertes y saqueos, de una vez =
    a trozos al azar con presupuestos al azar, en 20 semillas, con invariantes;
  - **guardar/cargar**: seguir tras cargar = no haber parado, en memoria y en disco;
  - partidas dañadas y versiones distintas;
  - la cadena de causas y el presupuesto;
  - el runner, en proceso y en un worker de Node de verdad: hechos, consultas, autoguardado, candado,
    parar y reanudar.

  Exit 1 si algo falla; `--verbose` imprime los números.
- **`npm run test:sim:server`**: el servidor real con su mundo en una carpeta temporal. Lo crea, lo
  guarda, lo reanuda tras matarlo y recupera el candado huérfano.
- **`npm run bench:sim`**: medido en esta máquina, en un solo núcleo:
  - unos 2,6 M eventos/s con 100 000 entidades decidiendo cada hora;
  - unos 100 M actualizaciones perezosas/s en barridos de columnas: la prehistoria del informe
    (15 600 M) saldría en unos 2,5 min en un núcleo;
  - un mundo de 100 000 entidades y 700 000 registros ocupa unos 10 MB guardado.
- **`npm run test:world`**, **`test:npcs`**, **`test:actors`**, **`test:net`**, **`test:galaxy`**: las
  secciones 10 a 14, abajo, contra el servidor de verdad cuando hace falta (puertos 3109-3112, carpeta
  temporal, `DEV_TOOLS=1`).

## 10. Objetos sueltos y niveles de detalle

- **El mundo guarda los objetos sueltos** (`modules/objects.ts`): cada caja o pieza es una entidad con
  qué es (`Thing`) y dónde reposa (`Place`). El servidor los simula mientras existen físicamente y
  escribe dónde quedan al reposar (`objects.rest`). Lo que mueves sigue movido tras reiniciar, con
  los mismos ids. Un mundo nuevo recibe los objetos de inicio de la sala (`objects.register`).
- **Agregados** (`kit/lod.ts`): unidades que son un número hasta que alguien se acerca (`enter`), y
  entonces tantas cosas concretas como huecos tenga (`slots`); cuando todos se alejan más de `leave`,
  las no tocadas vuelven a sumarse.
  - **Conservación**: `ledger` (en el agregado + en sus miembros + ido por su cuenta + perdido) no
    cambia nunca.
  - **Lo tocado persiste**: `touch` saca a un miembro para siempre (un objeto que alguien coge, un
    testigo).
  - **Mezclas** (cohortes): hasta 8 categorías, sorteo sin reemplazo. De 20 personas de 340, con 102 que
    odian al gobernador, lo odian 6 ± 2. Al volver salen **las mismas**, porque el sorteo está en
    función de la ranura y de la versión; si alguien se fue para siempre, se sortea de nuevo.
  - **Histéresis**: se concreta dentro de `enter` y se suma solo más allá de `leave`. Para quien está a
    bordo del anfitrión, siempre se concreta.
- **Almacenes** (datos del sitio: `BaseSiteDef.depots`): la base tiene 40 cajas y saca 9 a la vez en su
  rejilla. Coger una la hace tuya para siempre y queda un hecho `object.taken`, con testigos (§12).
- **El puente** (`server/world.ts`): cada segundo dice dónde están los jugadores a cada enlace
  (`server/worldObjects.ts`, `server/worldPeople.ts`). Los enlaces piden y aplican sin esperar dentro
  de un paso de juego. Sin mundo, la sala sigue con sus objetos de siempre.

## 11. Personas

- **Dotaciones** (`modules/people.ts`, datos del sitio `BaseSiteDef.crew`): la de la base son 11
  personas (técnicos, estibadores y guardias), con 6 fuera a la vez. Al concretarse, cada una tiene
  nombre, cara, oficio y mente.
- **Mentes**: cada hora de juego, más o menos, eligen actividad por la hora del día y el oficio:
  trabajar en su puesto, pasear o descansar. De día trabajan; de noche descansan (los guardias
  vigilan). Lo mandan al motor por `npc.task`, en coordenadas del sitio o en un punto del mundo.
- **Cuerpos** (`server/npcs.ts`): el servidor los mueve sin motor físico con el andador compartido
  (`shared/actors/walker.ts`):
  - sirve en cualquier suelo de cualquier cuerpo;
  - gira antes de andar y rodea las naves;
  - pisa el suelo en cada paso.

  Van a 20 Hz, sellados con el reloj de pasos, y solo a quien los tiene en su interés (600-800 m).
  Los clientes los dibujan como a cualquier astronauta, con su nombre al acercarse.
- **Individuos**: quien fue tocado (un testigo) deja su dotación y tiene su propio nivel de detalle:
  cuerpo cerca, solo ficha lejos, nunca vuelve al número.

## 12. Hechos, testigos y «qué hay aquí»

- El servidor informa de lo que hace un jugador (`room.report`): disparos, explosiones, heridas y
  objetos cogidos. Adjunta **quién lo notó**: la percepción compartida (`shared/actors/perception.ts`)
  decide por vista (alcance, cono del visor, el casco en medio) y oído (mismo aire, la estructura sin
  aire, el suelo; el vacío no lleva nada).
- Los jugadores son entidades del mundo, por nombre (`modules/players.ts`): los hechos apuntan a
  alguien que el mundo recuerda.
- **Lo que saben y sienten** (`modules/knowledge.ts`, qué pesa cada hecho en `FACTS`):
  - cada testigo guarda un recuerdo, que apunta al registro del hecho;
  - guarda un rencor contra quien lo hizo, que se apaga solo (la mitad en dos días de juego);
  - pasa a tener ficha propia;
  - si el rencor pasa de un umbral y tiene cuerpo, reacciona: lo dice por radio (`npc.say`) y va allí.

  El registro encadena *reaccionó ← presenció ← disparo*.
- **Qué hay aquí** (`modules/here.ts`): `sim.ask('world.here', { p, r })` devuelve lo que hay cerca, lo
  más próximo primero. Cada módulo dice qué es lo suyo (`describe`).

## 13. Herramientas: sismógrafo y barrido de semillas

- **Indicadores** (`SimModule.gauges`): números con nombre de cada módulo, como cajas en almacenes,
  personas fuera, individuos o el rencor medio y máximo.
- **Sismógrafo** (`tools/seismograph.ts`): el runner muestrea los indicadores cada hora de juego
  (`SimConfig.scopeS`), guarda `series.json` junto a la partida y lo recupera al reanudar.
  `npm run sim:scope` los pinta en la consola y en `tools/sim/out/scope.html`; con `--run <años>`
  simula un mundo sin gráficos.
- **Barrido** (`npm run sim:seeds -- --n 1000 --years 20 [--modules ruta]`): muchas semillas en
  paralelo (hilos). Da la distribución de cada indicador al final y año a año (p10, mediana, p90), qué
  pasa por siglo y en cuántas semillas, cuántas historias distintas hay y los errores. Salida en la
  consola, JSON y `tools/sim/out/seeds.html`. Sin contenido que evolucione solo (hoy, el mundo del
  juego depende de que haya jugadores cerca), se usa con un escenario (`tools/sim/scenario.ts`, el
  valor por defecto).

## 14. Escala: la galaxia y el salto

- **Galaxia** (`shared/space/galaxy.ts`): 2000 sistemas deterministas (semilla fija: es el mapa del
  juego), en años luz, con clase, temperatura, color, luminosidad y un nombre único. Sol es el 0.
- **Regiones**: los sistemas con presencia física (Sol y los `PHYSICAL_SYSTEMS` más cercanos) son
  cada uno una porción del mismo espacio `float64`, separadas por `SYSTEM_SPACING` (10⁸ km). Cada
  una tiene su cuerpo, hecho como la Luna pero con su propio suelo y su gravedad. `bodyAt`, la
  gravedad, el vuelo, los marcos, la red y el interés funcionan sin cambiar: en su región manda su
  cuerpo. La precisión se mantiene por debajo de 0,01 mm.
- **Salto** (`shared/space/jump.ts`):
  - el piloto pulsa **J**: al siguiente sistema, en vuelo y a más de 20 km del suelo, como mucho uno
    cada 15 s;
  - el servidor decide y avisa a todos (`jump`); todo lo que va a bordo va con la nave, porque vive
    en su espacio;
  - llega en reposo sobre el cuerpo de destino;
  - los informes del piloto que aún vengan del sistema anterior se descartan.

  En el cliente, el terreno de cada cuerpo se crea la primera vez que la cámara entra en su sistema,
  y fuera del Sol no se ve la Tierra.
- **Límites de esta versión**: la luz del sol y su dirección son las del Sol en todos los sistemas, y
  el resto de la galaxia solo existe como datos. Queda para cuando el mundo simule sistemas (economía,
  facciones).
