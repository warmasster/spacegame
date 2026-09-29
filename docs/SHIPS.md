# Naves y sistemas de nave

Cómo está montado, cómo se hace una nave nueva y cómo se añade una mecánica nueva.

## 1. Arquitectura

```
datos de la nave (hauler.ts)  ──finishShip()──►  ShipDef (validada)
                                                    │
        ┌───────────────────────────────────────────┼──────────────────────────┐
        ▼                                           ▼                          ▼
  ShipSim (sim.ts)                          cliente: vista, física,        servidor / offline:
  reglas de mandos, daños,                  pantallas (client/ship/*)      room.ts / game.ts
  reparación, explosiones                   leen la MISMA ShipDef          usan crew.ts (trajes)
        │                                                                  y shipBlast() iguales
        ▼
  ShipSystems (systems.ts) = núcleo, no conoce ninguna máquina por su nombre
        │  pide módulos a cada fábrica de modules/index.ts
        ▼
  módulos (modules/*.ts): power · loads · movers · gear · propellant · life · reactor · apu · engines · …
```

**El núcleo** (`systems.ts`) crea la tabla de variables compartida (integridad de cada máquina `id.hp`,
recorrido de cada mecanismo `mv.key`), pregunta a cada fábrica qué módulos necesita esta nave y los
ejecuta por fases en cada tick (20 Hz, en el servidor o en el cliente offline):

| Fase | Quién | Qué hace |
|---|---|---|
| `input` | máquinas | reaccionan a los interruptores: pulsos (SCRAM, arranque), máquinas de estados |
| `loads` | todos | declaran lo que piden este tick: kW por circuito (`t.load`), potencia que ofrecen (`t.source`), propelente que quieren quemar (`t.burn`) |
| `solve` | redes, por `order` | reparten: red eléctrica (10) → propelente (20) → atmósfera (30) |
| `step` | máquinas | reaccionan a lo que les dieron: temperaturas, giro, fallos, daños |
| alarmas | núcleo | evalúa las alarmas de cada módulo y enclava la alarma general |

Además cada módulo puede:
- **vetar un mando** (`interlock(c, next, …)`): el tren con peso encima, una puerta con diferencia de
  presión, el reactor sin refrigeración… `ShipSim.blocked()` primero mira lo genérico (mando destruido,
  tapa cerrada, disyuntor/conducto) y luego pregunta a todos los módulos.
- **declarar alarmas** (`alerts()`): id, texto, nivel, lámpara del anunciador y la condición.
- **declarar cómo suena** (`sounds()`): bucles con su volumen y su tono, golpes al cambiar de estado,
  desde la pieza que suena. El cliente los toca por el aire, el casco o el suelo ([`AUDIO.md`](AUDIO.md)).
- **reaccionar a una máquina destruida** (`destroyed()`): un depósito lleno revienta.
- **ofrecer un servicio** a otros módulos (`sys.provide`): la red eléctrica (`sys.power`, con
  `canStart()` para saber si hay energía de arranque), el propelente (`sys.fuel`, con `reachable()`,
  `canFeed()`, `isolated()`), la atmósfera (`sys.life`).

Los módulos **no se nombran entre sí**: el reactor no sabe que existe una APU; pregunta a la red si
hay «energía de arranque» y la red contesta con cualquier batería o generador que haya.

**Todo el estado continuo** vive en `st` (tabla de números con nombre). El servidor manda solo lo que
cambió (`VarSync`); el cliente lleva un espejo y dibuja con él. Los interruptores viven en `sw`.

### Ficheros

| Fichero | Qué |
|---|---|
| `shared/ship/def.ts` | Tipos de la nave, constructores (casco, consolas, máquinas), anclajes, `finishShip` + `checkShip` (valida referencias), `SCREEN_PAGES` |
| `shared/ship/ships/` | Cada nave: `selene.ts`, `peregrina.ts`, `albatros.ts` (dos cubiertas), registro en `index.ts` |
| `shared/ship/flight.ts` | Masa, empuje, RCS, integrador y `flyShip` (cliente offline y servidor) |
| `shared/ship/sim.ts` | `ShipSim`: mandos, bloqueos, explosiones, reparación, instantáneas, `shipBlast()` |
| `shared/ship/systems.ts` | Núcleo de sistemas (fases, alarmas, consultas) |
| `shared/ship/modules/api.ts` | El contrato de un módulo (`ShipModule`, `Tick`, `SystemFactory`) |
| `shared/ship/modules/index.ts` | **Lista de sistemas**: añadir una mecánica = una línea aquí |
| `shared/ship/modules/power.ts` | Red eléctrica, baterías, prioridades, disyuntores; cargas conmutadas y cortocircuitos |
| `shared/ship/modules/movers.ts` | Puertas, rampa, persianas, tren, radiadores (+ enclavamiento del tren) |
| `shared/ship/modules/propellant.ts` | Depósitos, válvulas, bombas, transferencia, repostaje, fugas, rotura |
| `shared/ship/modules/life.ts` + `atmos.ts` | Soporte vital y física del aire por compartimento |
| `shared/ship/airflow.ts` + `modules/decomp.ts` | Aire en movimiento (tirón, chorros, empuje sobre la nave, carga sobre los paneles) y descompresión explosiva (§6) |
| `shared/ship/modules/reactor.ts` | Reactor + circuito de refrigeración + radiadores |
| `shared/ship/modules/apu.ts`, `engines.ts` | APU, motores principales, RCS |
| `shared/ship/crew.ts` | Tripulación ↔ nave: compartimento, aire, umbilical; `crewStep` (mismo código en servidor y offline) |
| `client/ship/screens.ts` | Páginas de las pantallas (registro `PAGES`) |

## 2. Una nave nueva

Una nave es un fichero de datos que termina en `finishShip({...})` y una línea en `SHIP_DEFS`
(`sim.ts`). Lo mínimo es casco y medidas; todo lo demás es opcional y, si no está, ese sistema
simplemente no se crea (sin reactor no hay módulo de reactor, sin compartimentos no hay atmósfera,
sin rampa no hay rampa en la vista ni en la física).

```ts
export const POD = finishShip({
  id: 'pod', name: 'Cápsula', registry: 'POD-1', floorHeight: 0.5,
  panels: B.panels, modules: [mod], bounds: {...},
  zones: [{ id: 'cab', label: 'CABINA', min, max, lights: [[0, 1.9, 0]], lightKey: 'light' }],
  compartments: [{ id: 'cab', label: 'CABINA', volume: 16 }],
  subsystems: [{ id: 'main', label: 'PRINCIPAL', breaker: 'brk.main', priority: 'pri.main', routes: [], color: 0xffffff, rating: 5, base: 0.1 }],
  loads: [{ key: 'light', circuit: 'main', kw: 0.3 }],
  parts: buildParts([{ id: 'bat', type: 'battery', name: 'Batería', c, half, zone: 'cab', maxHp: 50, sw: { on: 'bat' }, p: { kwh: 2 } }]),
  defaults: { bat: 1, light: 1, 'cab.p0': 70 },
});
```

(Es la nave de la prueba «nave mínima hecha desde cero» en `tools/ship/check.ts`: funciona tal cual.)

Piezas de datos:

- **Máquinas** (`parts`): una caja en la nave con `type`, `maxHp`, circuito, zona. Sus números van en
  `p` (capacidad, potencia…; cada módulo tiene valores por defecto) y los interruptores que lee en
  `sw` por papel (`{ run: 'scrub' }`, `{ valve: 'v.gasO2' }`); lo que no se dé sigue la convención del
  módulo a partir del id. `tag` cambia el prefijo de sus variables (`rx` → `rx.temp`), `link` la ata a
  otra máquina (radiador → su reactor), `lamp` su lámpara de alarma.
- **Cargas** (`loads`): «este interruptor consume X kW de este circuito» (o una lista por posición para
  selectores). Con `part`, esa máquina hace cortocircuito si está muy dañada y encendida.
- **Mecanismos** (`movers`): tecla, velocidad, circuito, consumo al moverse y sensor de obstrucción
  (`doorSensor(d)`, `rampSensor(r)`).
- **Aberturas** (`openings`): puertas/rampa/respiraderos/conductos entre compartimentos o al vacío. Las
  puertas y rampas se niegan a abrir con más de 5 kPa de diferencia, sin código especial.
- **Propelente** (`fluid`): nodos, tuberías con válvula, circuito de las bombas, transferencia,
  repostaje y pares a equilibrar.
- **Soporte vital** (`life`): teclas de modo, ventiladores, calefacción, represurización y compresor.
- **Consolas** (`ConsoleSpec`): mandos, pantallas (con sus botones de página generados), tapas de
  seguridad generadas, indicadores (`reactor-core`, `gear-greens`), montaje en panel / máquina / prop.
- **Pantallas**: cada página es un id de `SCREEN_PAGES`; `readouts` dice qué interruptores lista cada
  página como filas simples (vuelo, radar…).
- **Cubiertas** (`decks`) y **escotillas de cubierta** (`hatches`): naves de varios pisos, ver §2.1.

`finishShip` rellena lo que falte, cierra disyuntores, enruta los conductos por detrás de los paneles y
**se niega a crear la nave si algo cita algo que no existe** (un circuito mal escrito, una tubería a un
nodo inexistente, una página sin registrar…). El núcleo además se niega si una máquina es de un tipo
que ningún sistema maneja.

### 2.1. Varias cubiertas, salas anidadas (el Albatros)

Una nave de dos pisos no necesita código propio; son las mismas piezas con alturas:

- **Cubiertas** (`decks`): los pisos de abajo arriba (`{ id, label, y }`, `y` = cara de arriba de sus
  placas). Sin nada, una sola cubierta a 0. El manual dibuja un plano por cubierta (`deckOf` reparte
  consolas, máquinas y asientos según la sala en la que están).
- **Secciones** (`modules`): la base de una sección es el punto más bajo de su perfil (`moduleBase`).
  Las de la cubierta de arriba empiezan en el techo de la de abajo; la quilla (cubas del casco,
  colisiones de la panza, sondas de contacto con el suelo) solo va bajo las que apoyan en 0
  (`keelModules`). `cap()` cierra desde la base del perfil, no desde 0.
- **El suelo de en medio** (`B.deck(zone, prefix, { y, xs, zs, holes, other })`): placas en una rejilla de
  cortes; `other(x, z)` dice qué compartimento hay debajo de cada una. Una placa con `other` rota une
  las dos salas (no ventea al vacío) y solo soporta la diferencia de presión entre ellas, igual que un mamparo. Donde
  no hay nada debajo (un voladizo) se deja `undefined`. Los `holes` quedan abiertos (la escalera). La
  sección de abajo deja fuera su techo con `strip(..., kindAt)` devolviendo `null`.
- **Escotilla de cubierta** (`HatchDef`: hueco `c`, `w` × `l`, espesor, dirección `slide`): una placa
  corredera a ras del suelo que tapa el hueco. La mueve el mecanismo de su tecla (con `hatchSensor`, no
  se cierra con alguien en la escalera) y el aire pasa por una abertura `door` con la misma tecla (`at`,
  `n` = el hueco y hacia arriba), así que hereda el enclavamiento de 5 kPa. La vista y los colisionadores
  usan la misma `hatchPlate(h, abierta)`.
- **Salas anidadas**: una zona puede estar dentro de otra (la esclusa en la esquina de la sala de
  máquinas, las salas de arriba dentro de la caja de la bodega de doble altura). Manda la **primera** zona
  de la lista que contiene el punto: pon las pequeñas antes. Sus paredes son tabiques
  (`B.partition(zone, other, prefix, a, b, { y0, y1, … })`). `B.splitAt(plano)` parte los paneles que
  cruzan su límite, y `B.assign(caja, { zone, other }, kinds)` pasa a la sala anidada lo que queda dentro
  (su trozo de casco, sus placas de suelo). Las puertas se recortan con `cutOpening`, como siempre.
- **Escaleras**: el mueble `stairs` (su colisión es la cuña bajo los peldaños) de una cubierta a otra, y
  `railing` (barandilla) alrededor del hueco.
- Ayudantes con altura: `deckAt(x, z, up, y)`, `supportBlock(..., base)`, `doorButtons` (relativo al
  umbral de la puerta), `doorSensor` (relativo a `d.c[1]`).

`test:ship` («dos cubiertas») comprueba que cada placa del suelo alto nombra la sala que tiene debajo,
que una rota une las dos salas y una del voladizo ventea, que la escotilla tapa y destapa su hueco y
que no abre con diferencia de presión.

### El manual (tecla M)

Se genera solo a partir de la nave (`client/ui/manual.ts`): los capítulos de guía son `def.manual`
(texto con `[[consola/tecla]]` que enlaza a la ficha del mando, pasos, notas, avisos, figuras, lecturas
en vivo `{ live }` y fichas `{ controls: [...] }`); el plano, la tabla de circuitos, las alarmas, las
páginas de pantalla y la referencia de todos los mandos salen de los datos. Por eso cada mando necesita
su `help` (1–3 frases: qué hace y qué necesita) y cada alarma su `help` (qué significa y qué hacer):
`test:ship` falla si falta alguno.

## 3. Una mecánica nueva

1. Un fichero en `shared/ship/modules/` con una `SystemFactory`: qué tipos de máquina maneja (`parts`)
   y `make(sys)`, que mira la nave y devuelve sus módulos (uno por máquina, o uno para toda la nave).
2. Una línea en `SYSTEM_FACTORIES` (`modules/index.ts`).
3. Si tiene pantalla: una entrada en `SCREEN_PAGES` y su dibujo en `client/ship/screens.ts`.

El módulo implementa solo las fases que necesita. Ejemplo real (prueba «mecánica nueva» en
`tools/ship/check.ts`): una bobina que consume 2 kW mientras su interruptor está encendido, se calienta
y da alarma:

```ts
export const coilSystem: SystemFactory = {
  id: 'coil',
  parts: ['coil'],
  make: (sys) => sys.def.parts.filter((p) => p.type === 'coil').map((p) => {
    const iT = sys.vars.define(`${p.id}.t`, 0.1, 0);            // variable replicada
    return {
      id: `coil:${p.id}`,
      loads: (t) => { if (t.sw[p.id] === 1) t.load(p.circuit, 2); },
      step: (t) => { if (sys.supply(t.st, p.circuit) >= 0.5 && t.sw[p.id] === 1) t.st[iT] += t.dt; },
      alerts: () => [{ id: `hot.${p.id}`, label: 'BOBINA CALIENTE', level: 1, lamp: 'BOBINA', on: (st) => st[iT] > 1 }],
    };
  }),
};
```

Con eso ya tiene: energía de su circuito (y deslastre, disyuntor, conducto cortado), daño por
explosiones, reparación con la soldadora, replicación en red, alarma en pantallas y anunciador.
Radar, torreta, compensador inercial y cargador están declarados en `equipmentSystem` (solo consumen
y se rompen): darles vida es moverlos a su propio módulo.

Reglas:
- Las variables se declaran en el constructor, siempre en el mismo orden (servidor y clientes deben
  tener los mismos índices). Cuanto negativo = privada del servidor (no se envía).
- Cambiar un interruptor desde la simulación solo con `t.setSw` (así se difunde).
- Nada de `if (part.id === 'algo')`: lo específico de una nave va en sus datos.

## 4. Catálogo y naves

Las máquinas salen del catálogo (`shared/ship/catalog/`): `part('reactor.fission.XS', { id, c, zone, circuit })`.
La nave solo dice dónde va y a qué circuito se ata; talla, masa, `p` y modelo vienen del componente.
Un componente propio es `defineComponent({ base, id, ... })`. Las naves registradas están en
`ships/index.ts` (`selene`, `peregrina`, `albatros`). El mundo las aparca con `SHIP_SPAWNS` (en el marco de un sitio; `r`: radio de su pista, que el sitio aplana y limpia de rocas, 14 m por defecto) sobre el suelo global (`shared/ship/spawn.ts`, `placeShip` en un marco tangente).

## 5. Vuelo

`flyShip` (`flight.ts`) integra la pose. En offline lo hace el cliente (`FLIGHT.enabled`), a 60 Hz. En red lo hace el servidor, a 20 Hz, y reenvía la pose (`shipPose`). El piloto, sentado en `helm.seat`, manda la palanca (`fly`) unas 20 veces por segundo; si deja de mandarla, el mando vuelve a cero. El cliente coloca el casco en esa pose y lleva consigo a quien va a bordo.

Cada paso, antes de que el jugador camine:

1. Empuje de los motores en marcha. Si la máquina tiene `gimbal` (las góndolas de la Selene, `+π/2`),
   el chorro gira con ese interruptor: VTOL levanta, CRUCERO empuja hacia la proa.
2. El RCS reparte traslación y rotación. Sentado en el asiento del piloto (`helm.seat`): W/S cabeceo,
   A/D guiñada, Z/C alabeo, flechas trasladan, R sube, F baja. El acelerador de la consola manda los
   motores principales: en `fa.hold` (acoplado) y con el piloto automático es el tope del motor que
   el ordenador puede usar para la velocidad pedida (W/S, o VEL / NAV); desacoplado es su empuje
   directo (control manual). El casco del piloto muestra `MOTOR`: empuje, tope y por qué no empuja. Acoplado y posada en tierra no empuja: hay que despegar.
3. `fa.sas` anula el par de los motores (si no, el empuje de atrás pica el morro) y frena los giros al soltar el mando. `ap.hor` (tablero de vuelo) nivela las alas cuando el cabeceo y el alabeo van sueltos. `ap.alt` guarda la altura al encenderlo y la mantiene; R y F la cambian. `fa.hold` (acoplado) pide velocidad en vez de
   aceleración. `fa.land` limita el descenso cerca del suelo. El tren no sube mientras `landed`.
4. Gravedad lunar y contacto con el suelo: las patas, con el tren abajo y la nave casi nivelada. Las cuatro esquinas del casco entran cuando está inclinada o con el tren arriba. Lo que se clava se empuja fuera. En el suelo, nivelada y quieta, la pose se congela para que no tiemble. `onPad` sigue siendo cierto mientras siga en el suelo
   a menos de 25 m de donde aparcó (el repostaje).
5. El cuerpo de Rapier del casco exterior es cinemático mientras la nave se mueve y fijo cuando está quieta (el controlador del astronauta se atasca en los colliders de un cuerpo cinemático donde tocan el suelo: pie de la escalerilla, borde de la rampa), y se coloca en la pose. Quien está dentro del casco (y las cajas
   que no se han caído) se transforma con la pose, así que se puede andar por la nave mientras vuela.
   A bordo, el andar, la gravedad del traje y la cámara siguen el arriba de la nave, no el horizonte.
   La velocidad del suelo se resta del andar y se vuelve a sumar, para no patinar.

`npm run test:ship` comprueba que una nave quieta no deriva, que el RCS guía, que la Selene en VTOL
despega y que un punto a bordo sigue a la nave.

## 6. Descompresión

Cuando un compartimento presurizado pierde el aire deprisa (un panel reventado, una ventana que
cede, una puerta abierta al vacío) pasan cuatro cosas, todas sacadas del estado que ya se replica
(presión y temperatura de cada compartimento, integridad de los paneles, puertas, válvulas): no
hay mensajes nuevos.

1. **El flujo** (`airflow.ts`, `ventsOf`): cada abertura con gas pasando (los caminos del soporte
   vital que tienen sitio: paneles, puertas y la rampa; `finishShip` saca `at`/`n` de la puerta o
   la rampa de la misma tecla) da su caudal (tobera compresible, sónica si está ahogada), la velocidad
   de salida y el empuje del chorro. `airPush` da, en cualquier punto, la presión dinámica del aire:
   dentro, un sumidero (hemisférico junto al agujero, toda la sección de la sala más lejos); pasada la
   abertura, un chorro (hacia la sala de al lado, o un penacho al vacío que se abre y se diluye).
2. **Lo que arrastra** (cliente, `client/ship/decompression.ts`, sistema `air` a orden 15): la
   aceleración es presión dinámica × área de arrastre por kg × `AIR.drag`, con tope `AIR.maxAccel`.
   El astronauta (`DRAG.standing` / `crouched`) la recibe en `PlayerController.wind`: en cubierta
   la tracción de las botas aguanta hasta la frenada, así que solo arrastra a un metro o dos de una
   brecha grande; en el aire, empuja siempre; sentado, nada. Las cajas (`boxDrag`) vuelan antes:
   su dueño les aplica el impulso y, si nadie las simula, las toma el cliente de menor id.
3. **El choque** (módulo `decomp`, autoridad): la velocidad de la caída (kPa/s entre
   `DECOMP.rate`) mide su violencia. Cada máquina del compartimento pierde
   `maxHp × decomp × (kPa perdidos / 70) × violencia`, hasta el doble junto a la abertura, ±30 %.
   `decomp` es un dato del componente del catálogo (0 sellado y robusto … 1 destrozado; por defecto
   `DECOMP_DEFAULT`). Publica `<compartimento>.shock` (0..1) para la niebla y el temblor, y avisa
   «DESCOMPRESIÓN EXPLOSIVA». Un venteo o una grieta vacían despacio: no cuentan.
4. **Los paneles** (`panelStrain`): un panel aguanta `rating × (hp/maxHp)²` kPa (`AIR.rating` por
   tipo, o `PanelDef.rating`); con más diferencia de presión se raja (`DECOMP.tear`), salta la
   alarma PANEL CEDIENDO y en unos segundos revienta: el daño sale por `Tick.damagePanel` y
   `ShipSim.tick` lo devuelve en `hp` para que la autoridad lo difunda. Soldarlo a tiempo lo salva.

El chorro de lo que sale de la nave empuja la nave (`FlightModel`: una fuerza `AIR.thrust × empuje`
en cada abertura al vacío; despierta una nave dormida). Los efectos (chorros de vapor y hielo, polvo
corriendo hacia el agujero, niebla de condensación, paneles que crujen, el estallido y el temblor
de cámara) los dibuja cada cliente con los mismos datos.

Una nave nueva no tiene que hacer nada: con compartimentos tiene descompresión. Para afinarla:
`decomp` en sus componentes, `rating` en un panel especial, `at`/`n` en un venteo con sitio.
`npm run test:ship` comprueba el vaciado, el aviso y el daño (frágil sí, robusto no, venteo nada),
el panel que cede y el soldado a tiempo, el tirón (cerca más, agachado menos) y el empuje en vuelo.
