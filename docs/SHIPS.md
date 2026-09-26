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
| `shared/ship/hauler.ts` | La «Selene»: solo datos |
| `shared/ship/sim.ts` | `ShipSim`: mandos, bloqueos, explosiones, reparación, instantáneas. `SHIP_DEFS` (registro de naves), `shipBlast()` |
| `shared/ship/systems.ts` | Núcleo de sistemas (fases, alarmas, consultas) |
| `shared/ship/modules/api.ts` | El contrato de un módulo (`ShipModule`, `Tick`, `SystemFactory`) |
| `shared/ship/modules/index.ts` | **Lista de sistemas**: añadir una mecánica = una línea aquí |
| `shared/ship/modules/power.ts` | Red eléctrica, baterías, prioridades, disyuntores; cargas conmutadas y cortocircuitos |
| `shared/ship/modules/movers.ts` | Puertas, rampa, persianas, tren, radiadores (+ enclavamiento del tren) |
| `shared/ship/modules/propellant.ts` | Depósitos, válvulas, bombas, transferencia, repostaje, fugas, rotura |
| `shared/ship/modules/life.ts` + `atmos.ts` | Soporte vital y física del aire por compartimento |
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

`finishShip` rellena lo que falte, cierra disyuntores, enruta los conductos por detrás de los paneles y
**se niega a crear la nave si algo cita algo que no existe** (un circuito mal escrito, una tubería a un
nodo inexistente, una página sin registrar…). El núcleo además se niega si una máquina es de un tipo
que ningún sistema maneja.

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

## 4. Pendiente (sigue escrito a mano para la «Selene»)

- Decoración y colisiones fijas en `client/ship/view.ts` / `physics.ts`: mentón de la proa, aleta,
  lomo dorsal, pasamanos, pistones de la rampa. Deberían ser `props` con modelo.
- Modelos de máquinas por tipo en `client/ship/models.ts` (correcto, pero una máquina nueva necesita su
  modelo o sale una caja).
- El dibujo del manual (figuras `air`, `power`, `drive` en `hud.ts`) está pensado para esta nave.
- Vuelo, radar y torreta aún sin mecánica.
