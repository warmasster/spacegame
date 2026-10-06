# Equipo: marcos, proyectiles, armas y objetos

Lo que la tripulación dispara, lleva y maneja es **datos en catálogos compartidos** (`src/shared/items/`),
y todo lo que se mueve vive en un **marco** (`src/shared/frames/`). El mismo código corre en el cliente
y en el servidor.

## 1. Marcos (`shared/frames`)

Hay un marco para el mundo (`WORLD_FRAME`, id 0) y uno para cada **anfitrión**: algo con espacio propio
en el que se puede estar dentro. Hoy es una nave; mañana puede ser una estación o un vehículo en una
bodega. Un anfitrión es un `FrameHost`:

| Campo | Qué |
|---|---|
| `id` | Su id (nunca 0) |
| `pose`, `prev` | Su pose en el mundo en este paso y en el anterior (para dibujar entre los dos) |
| `reach` | Más allá de esto (m, en su espacio) no hay nada dentro: descarte barato |
| `inside(l)` | ¿Está este punto de su espacio dentro de uno de sus compartimentos? |

- `carry(a, b, p, v, q)` lleva un movimiento de un marco a otro sin saltos: la misma posición y la
  misma velocidad en el mundo.
- `hostAt(anfitriones, p)` dice en qué anfitrión está un punto del mundo, si está en alguno.
- Una nave es anfitrión así: en el cliente, `ShipClient implements FrameHost`; en el servidor,
  `shipHost(sim)`. Lo de dentro son sus zonas (`inShip`, `shipReach`).

La regla de «dentro» es una sola. La usan los proyectiles, las cajas y el astronauta, que además
añaden sus propias condiciones (estar apoyado en el casco, pisar la escalera).

**Por qué:** dentro de un anfitrión todo son números pequeños y locales. No importa lo rápido que vaya
(en órbita, 1,6 km/s), dónde lo tenga cada cliente en la red ni la precisión de float32. En la red algo
de un anfitrión viaja como `fr` + coordenadas locales.

### Cuerpo balístico (`ballistic.ts`)

Todo lo que vuela libre (cohetes, balas y lo que se añada) es un `Ballistic` y avanza con
`stepBallistic(b, spec, dt, env)`:

- **Dentro de un anfitrión** vuela en su espacio:
  - sale justo de donde se lanzó;
  - la cabina no puede adelantarlo;
  - siente la gravedad aparente de la cabina;
  - choca con los colisionadores de la nave.
- **Al salir** por una puerta o una brecha pasa al mundo en un paso fijo, con la misma posición y
  velocidad. Si entra en un anfitrión, pasa a su marco del mismo modo.
- **En el mundo** cae con la gravedad radial real del cuerpo en el que esté y choca con cascos, suelo y
  tripulación. La tripulación se comprueba contra el tramo recorrido, así que no hay túnel a ninguna
  velocidad.
- **Subpasos según lo que recorre** (objetivo de 6 m, hasta 8 por paso fijo). A bordo se mueve unos
  m/s respecto a la cabina: normalmente basta un subpaso.
- **Contacto con el suelo sobre el tramo:** cuando un subpaso cruza la superficie, `groundHit`
  refina el cruce usando `BallisticEnv.groundAlt`, hasta 1 mm de recorrido. Conserva el último
  punto exterior; no explota en el extremo ya enterrado. Si hay un casco, limita el tramo hasta
  ese casco: suelo anterior y tripulación anterior tienen prioridad. La consulta usa el suelo
  radial actual con sus modificadores, igual para cohetes, minimisiles y balas; no añade un caso
  específico de arma ni depende de cuándo llegue la malla reconstruida desde un worker.

Lo que atraviesa lo pone cada lado con un `BallisticEnv`: anfitriones, gravedad de cabina, barrido
dentro de un anfitrión, barrido de cascos en el mundo, altura sobre el suelo y tripulación. El cliente
lo monta sobre Rapier (`client/fx/projectiles.ts`). El servidor puede montar el suyo para simular los
disparos de un NPC o de una torreta.

## 2. Proyectiles (`shared/items/projectiles.ts`)

```ts
defineProjectile({
  id: 'bullet', name: 'Bala',
  speed: 380,          // m/s respecto al lanzador (se suma su velocidad)
  life: 3,             // s
  gravity: 1,          // multiplicador de la gravedad que siente
  radius: 0.45,        // cuánto tiene que pasar de cerca del centro de alguien para darle
  impact: { radius: 0.5, crew: 34, falloff: false, hull: { radius: 0.3, damage: 6 }, fx: 'hit' },
  look: 'tracer',      // aspecto en el cliente (client/fx/projectileLooks.ts)
  sounds: { impact: 'bullet.hit' },
});
```

`impact` lo aplica el servidor:
- `crew`: daño al traje. Con `falloff` baja con la distancia (explosión); sin él es igual en todo el
  radio (impacto directo).
- `hull`: daño a paneles y máquinas, a escala de nave.
- `terrain` opcional: edición de la superficie, declarada con el registro de modificadores.

`fx` dice cómo se ve en los clientes:
- `blast`: bola de fuego, onda que empuja y cajas volando.
- `hit`: chispas, un golpe metálico y un empujón a un objeto suelto.

Aspecto nuevo: `defineProjectileLook(id, { mesh, trail, launch, tail })`.

### 2.1. Consecuencias sobre el suelo (`shared/items/impacts.ts`)

Una munición declara qué cambia, sin ramas para su nombre ni para su nave:

```ts
impact: {
  radius: 4, crew: 70, falloff: true,
  hull: { radius: 1.8, damage: 55 }, fx: 'blast',
  terrain: { kind: 'crater', radius: 2.4, maxHeight: 1.2 },
}
```

`terrainImpact(efecto, puntoMundo, superficies)` es el único criterio para preparar la edición,
tanto offline como en el servidor. Busca `bodyAt`, mide `heightAboveGround` sobre la superficie
actual y centra el modificador radialmente. No usa Y, una Luna fija ni el tipo de lanzador.
Sin `terrain`, sin superficie o por encima de `maxHeight` no crea edición. `kind` pertenece a
`MOD_KINDS`; `params` y `yaw` son opcionales y tienen el significado de ese modificador. El catálogo
rechaza un tipo desconocido al cargar. Los parámetros NaN se resuelven antes de serializar;
el catálogo original no se modifica.

La función **prepara datos; no modifica el mundo**. La autoridad añade la edición una sola vez
y emite `explode.mod`. Cada cliente aplica el evento a `BodySurface` e invalida terreno, rocas
y colisiones. Los workers reconstruyen las mallas con la misma lista de ediciones; la malla
antigua se conserva hasta recibir la nueva. `welcome.mods` permite que quien entre más tarde
vea el mismo suelo. Un impacto repetido puede profundizar un cráter existente sin aumentar el
contador: comprobar solo el número de modificadores no demuestra que la edición haya fallado.

La fusión solo alcanza centros a menos del 30 % del radio y radios que difieren menos del 30 %.
Las direcciones del modificador se redondean al serializarlas: su longitud puede diferir de 1.
Todas las distancias de fusión, selección para workers e invalidación pasan por el ángulo estable
`space/angle.ts` (`atan2(|a×b|, a·b)`), y terreno y rocas comparten `modTouches`. No uses
`acos(a·b)` con estos datos: el error de longitud puede convertir 12 m en 0 m o el propio centro
en un punto a 54 m, fusionando impactos separados o dejando una malla sin reconstruir.

Otra munición con un cráter mayor solo cambia `radius`; otro perfil usa otro `kind` del registro.
Un perfil nuevo se implementa en `terrainMods/kinds.ts`, sin tocar disparos, marcos ni clases de
nave. `terrainModAt` es el constructor radial común; `blastCrater` sigue siendo su comodidad
específica para los sitios y herramientas de diagnóstico.

## 3. Armas y herramientas (`shared/items/weapons.ts`)

Las de mano son equipables. Las teclas numéricas las eligen por orden de catálogo (1, 2, 3…). La misma tecla
otra vez, o X, la guarda; guardada va en su sitio de la mochila. Qué hace el gatillo (`action`):

| `action` | Qué |
|---|---|
| `{ kind: 'fire', projectile, auto? }` | Dispara ese proyectil; con `auto`, mientras se mantiene el botón |
| `{ kind: 'weld' }` | Suelda y repara paneles y máquinas mientras se mantiene (el servidor solo repara con una de estas en mano) |

Además: `cooldown` (lo comprueba el servidor), `recoil` (N·s: cuerpo, brazos y cámara), `hud` (textos
del casco) y `sounds.fire`.

El aspecto va aparte, en `client/fx/weapons.ts` (`LOOKS`): malla, agarres de cada mano, boca y funda en
la mochila. Un arma del catálogo sin aspecto da error al arrancar.

Hoy: **lanzacohetes** (1), **soldadora** (2) y **fusil** (3: balas trazadoras, automático). El fusil
va en el costado derecho de la mochila, simétrico a la soldadora.

### Quién dispara

El disparo en la red es `fire { w, o, d, v, fr, t }` (arma, origen, dirección, velocidad del
lanzador, marco y el tiempo del paso en que salió: quien lo recibe un viaje de red después lo vuela
hacia delante hasta su presente, `Projectiles.spawn(…, ahead)`). El servidor lo valida con el catálogo (arma que dispara algo y cadencia) y lo reenvía. Cada
cliente simula el proyectil (`Projectiles.spawn(dueño, tipo, marco, …)`). El que dispara lo lanza en el
acto, sin esperar el eco, y avisa del impacto (`hit { k, p, fr }`). El servidor aplica `impact` y lo
anuncia (`explode { k, fr, l }`).

El `dueño` es un id cualquiera. Un NPC haría lo mismo desde el servidor. Faltaría su `BallisticEnv` del
lado del servidor para que él mismo decida el impacto.

### Identidad de herramientas en red (`shared/wire.ts`)

`PlayerState.w` es el id estable del catálogo, incluso con el arma guardada. Viaja tanto en el
estado binario del cliente como en las instantáneas binarias para sus compañeros. No es la tecla
numérica ni un bit por arma: añadir o reordenar entradas no cambia su identidad.
El formato guarda los campos fijos y después `uint16 longitud + bytes UTF-8 del id`; longitud 0
significa sin herramienta. Cada estado ocupa 50 B más el id; el mensaje individual añade la
etiqueta y el **tiempo del paso** al que pertenece (`t`, float64: 59 B más el id). Una instantánea
puede mezclar ids de distintas longitudes. El decodificador respeta el desplazamiento de un
`DataView` y rechaza paquetes truncados.

El servidor autoriza la reparación consultando `weaponById(state.w).action.kind` y `Armed`.
Al añadir un campo a `PlayerState`, hay que añadirlo **también** al códec binario y a las pruebas:
cambiar solo el tipo o el mensaje JSON no lo transporta. Estados, disparos y cajas llevan `t`, el
tiempo del paso del emisor ([`MOVIMIENTO.md`](MOVIMIENTO.md)). El protocolo actual es **16**;
hay que reiniciar el servidor y recargar todos los clientes tras actualizarlo.

## 3.1. Montajes de armas (`shared/items/mounts.ts`)

Un arma también puede ir **atornillada** a algo: una torreta, un cañón fijo en el morro, una torreta
en el muro de una base, la de un vehículo. El arma sigue siendo un `defineWeapon` (con `mounted: true`:
sin tecla numérica ni aspecto en las manos); el **tipo de montaje** dice cómo se mueve:

```ts
defineMountKind({
  id: 'turret.minimissile', name: 'Torreta de minimisiles',
  weapon: 'minimissile.tube',      // qué dispara (proyectil, cadencia, sonido)
  yaw: null, pitch: [-0.12, 1.35], // arcos (rad); yaw null = giro completo
  slew: 1.6,                       // rad/s por eje (0 = fijo, dispara hacia delante)
  pivot: 0.2, barrels: [[0.17, 0.02, 0.55], [-0.17, 0.02, 0.55]], // bocas por turnos
  magazine: 12, feed: 0.25,        // cargador y reposición por segundo con energía
  spread: 0.45,                    // lo que el servidor tolera entre el disparo y el eje de la cabeza
  look: 'turret.twin',             // aspecto (client/fx/mountLooks.ts)
});
```

Una **colocación** (`MountPlacement`: `id`, `kind`, `at`, `up`, `fwd`, en el espacio del anfitrión)
lo pone en cualquier sitio. La matemática no sabe qué es el anfitrión: `aimAt`/`aimAlong` (ángulos
que apuntan a un punto o una dirección, dentro de los arcos), `slewAim` (giro a su ritmo),
`mountMuzzle` (boca y eje de un cañón). Ángulos: guiñada sobre su `up` (+ hacia su izquierda),
alza desde su horizonte. Espacio de la cabeza: +Z la boca, +Y arriba, +X su izquierda.

**En una nave** (el primer adaptador): `ShipDef.mounts: [{ id, kind, part }]` sobre una máquina
(`part`: su interruptor, su circuito y su integridad son los del montaje; `at` sale de lo alto de su
caja). El módulo `modules/weapons.ts` la hace funcionar: en servicio (encendida, con energía, sin
destruir) gira hacia la puntería del artillero y el cargador la rellena; si no, se queda congelada.
Ángulos, munición y servicio son variables replicadas (`turret.yaw`, `.pitch`, `.ammo`, `.ready`).
Un asiento con `mounts: ['turret']` es el del artillero.

**Quién apunta y dispara** (`client/ship/gunnery.ts`): sentado en ese asiento, los montajes apuntan
a donde mira la cruz si no tienen monitor (convergen a 300 m), o se dirigen desde su puesto de
cámara. El disparo acciona los que estén en servicio y cargados.
El cliente dibuja la cabeza donde la apunta él (se adelanta); al servidor le manda `aim { ship, m, y, p }`
~15 Hz y el disparo es el `fire` de siempre con `m` (índice del montaje) y `fr` (la nave). El
servidor comprueba que va sentado en un asiento que lo maneja, que está en servicio, cargado, a su
cadencia y que el disparo sale de su boca a lo largo de su eje (`shotFits`). El proyectil vuela con el
núcleo balístico como cualquier otro (sale en el espacio de la nave y pasa al mundo). El disparo
suena desde la boca por la estructura del anfitrión (`turret.fire`); el giro, con `mach.motor`.

**Uso de la Selene:** siéntate en el asiento derecho, **Asiento del copiloto**, mirando el asiento
y pulsando E o clic. La mini consola **ARTILLERÍA · CÁMARA** está delante, sobre su MFD. ON
enciende la torreta (necesita ARMAMENTO); APUNTAR/G centra la vista en la pantalla y el ratón
dirige la torreta; T/DISPARO dispara. En APUNTAR también dispara clic; G libera la mirada para
pulsar FIJAR, ZOOM o CENTRO. Zoom: 1×/1,5×/3×. FIJAR mantiene un punto del mundo, sin seguir
objetos móviles ni guiar el misil. La pantalla necesita AVIÓNICA y copiloto sentado: vacío queda
negra. El HUD central y el monitor muestran cargador y motivo de una negativa. Son 12 minimisiles,
reponiendo uno cada 4 s con energía. Espacio permite levantarse. Su elevación tiene límites:
va sobre el lomo y no puede apuntar hacia abajo atravesando la nave.

El minimisil es ligero: radio de impacto sobre tripulación 0,35 m, daño máximo al traje 30 y al
casco 22, explosión de 2 m y cráter de radio 1,1 m/desnivel máximo 0,45 m. Su aspecto reutiliza
la geometría del cohete a escala 0,55. Cámaras, efectos de imagen y mandos reutilizables:
[`CAMERAS.md`](CAMERAS.md).

La adquisición de puntería se registra como `gunnery-aim` (frame, orden 38), una vez colocada la
cámara; el giro predictivo como `gunnery` (fixed, orden 39), antes de los proyectiles. El módulo
autoritativo gira en el paso de sistemas de nave. La matemática de bocas y puntería reutiliza
parámetros de salida y scratch, sin crear vectores temporales en cada cálculo.

Una intención que no cambia se reenvía cada 0,5 s. La primera puede llegar antes de `Seated`
y ser rechazada: «enviada» no significa que la autoridad la haya aceptado. El reenvío recupera
también la puntería después de un cambio de servicio.

Offline hay dos objetos: la autoridad en `game.shipAuthority` y el espejo `ship.sim`.
`GunneryIO.aim` y `fire` actúan sobre **la autoridad**, igual que un mensaje online. Después de
gastar munición se actualiza el espejo; los siguientes ticks recargan desde el estado autoritativo.
Escribir solo en el espejo pierde el cambio cuando se replica el próximo tick. Vista, HUD y audio
leen el espejo; no son propietarios de la munición ni del daño.

La munición usa quantum **0,01**, porque el cargador repone fracciones. Con quantum 1, gastar
una ronda y recargar 0,0125 antes de replicar deja un cambio menor de 1: `VarSync` lo omitiría
y el compañero seguiría viendo el cargador lleno. Una variable con un integrador fraccionario
necesita un quantum que pueda representar esos cambios, aunque el HUD muestre rondas enteras.

Otro anfitrión (una base, un vehículo) necesita solo su adaptador: dónde guarda los ángulos y la
munición, qué lo alimenta y quién lo maneja. La vista (`MountView`), la matemática y los catálogos
ya sirven.

## 4. Objetos sueltos (`shared/items/objects.ts`)

Todo lo que está suelto como cuerpo físico real, sea una caja, una pieza de repuesto o lo que venga:

```ts
defineObject({
  id: 'spare', name: 'Pieza de repuesto',
  half: [0.16, 0.11, 0.16], mass: 9,          // tamaño y masa por defecto
  handling: { grab: true, throw: true },       // qué se puede hacer con él
  look: 'spare',                               // aspecto (client/cargo/looks.ts)
  sounds: { grab: 'crate.grab', drop: 'crate.drop', throw: 'crate.throw' },
});
```

- Todos comparten la misma maquinaria (`client/cargo/crates.ts`): cuerpo Rapier en su marco, un dueño
  que lo simula, red y paso entre nave y mundo.
- A qué marco pertenecen lo decide una regla compartida (`shared/frames/membership.ts`): entra en una
  nave dentro de un compartimento o reposando sobre ella (despacio respecto a su casco), sale en cuanto
  nada de la nave la sostiene. El cambio es al final del paso y lleva su historia dibujada
  ([`MOVIMIENTO.md`](MOVIMIENTO.md)).
- Contacto y amortiguación por tipo (`physics`: sin amortiguación lineal por defecto, es vacío) y la
  velocidad de lanzamiento (`handling.throwSpeed`, o de su masa: `THROW_IMPULSE` / masa).
- Todos se manejan igual:
  - al mirarlo, el casco dice qué es y cuánto pesa;
  - **E** lo coge o lo suelta;
  - **Q** lo lanza;
  - **clic** lo deja sobre la silueta.
- Una colocación (la bodega de una nave con `cargo: [{ kind: 'spare', … }]`, las cajas de un sitio)
  puede darle otro tamaño y color. Sin `kind` es una caja.
- Aspecto nuevo: `defineObjectLook(id, { geometry, materials })`. Todos los de un mismo aspecto,
  tamaño y color son una sola malla instanciada.

Hoy: **caja de carga** y **pieza de repuesto** (dos en la bodega de la Selene, encima de las cajas
pequeñas).

## 5. Añadir algo

| Quiero… | Toco |
|---|---|
| Otra munición | `defineProjectile` (+ `defineProjectileLook` si se ve distinta, + sonidos en `client/audio/sounds/`) |
| Otra arma | `defineWeapon` + su aspecto en `LOOKS` (`client/fx/weapons.ts`) |
| Otra torreta / cañón fijo | `defineWeapon` con `mounted: true` + `defineMountKind` + `defineMountLook`; en una nave, una entrada en `mounts` sobre una máquina y el asiento que la maneja |
| Otro objeto | `defineObject` + `defineObjectLook`; colocarlo en una bodega o un sitio |
| Otro tipo de anfitrión (estación, vehículo) | Que implemente `FrameHost`; lo demás ya lo trata igual |

## 6. Verificar los límites entre módulos

| Comando | Qué garantiza |
|---|---|
| `npm run test:equipment` | Ids binarios y truncamientos, autoridad y munición, lanzamiento orbital, impactos radiales/serializables, 24 cráteres separados e índice persistido, invalidación con direcciones redondeadas y contactos de suelo expuestos a distintas velocidades |
| `npm run test:cameras` | Contratos genéricos, mandos y permisos, foco al centro, cámara apagada sin ocupante/energía, zoom/fijación, efectos y presupuesto de render sin navegador |
| `npm run diag:equipment` | Cuatro clics reales → cuatro cráteres separados, sin profundizar los anteriores; rayos sobre cada malla y colisión; puntería y munición de torreta offline; capturas |
| `npm run diag:ship:net` | Dos clientes: mandos, daño y soldadura, herramienta remota, puntería y disparos montados, tres impactos separados y suelo idéntico al entrar después |

Los diagnósticos usan `GAME_URL` (por defecto `http://localhost:3000`). El de red necesita una
instancia de pruebas con un mundo limpio: destruir el mismo panel de una partida anterior puede
provocar explosiones secundarias y cambiar el objetivo de reparación. Los informes y capturas
quedan en `tools/diag/out/`. No lo ejecutes contra una partida compartida que quieras conservar.

Los fallos reparados estaban en los límites: `w` desaparecía en el códec binario; la torreta
offline escribía en el espejo; la primera puntería online podía llegar antes de `Seated` y no
repetirse; la recarga fraccionaria ocultaba la munición gastada al replicar. Las pruebas deben
atravesar esos límites: llamar directamente a `repair` o `tryFire` no prueba el recorrido completo.
