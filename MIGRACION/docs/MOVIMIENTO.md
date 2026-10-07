# Movimiento: un solo reloj y ninguna dependencia (prototipo en Rust)

Todo lo que se mueve entre estructuras —el jugador hoy; mañana cualquiera que ande, flote o vaya
cargado entre ellas— tiene que verse y chocar **igual** vaya la nave de al lado a 2 m/s o a 7 km/s,
esté donde esté, a cualquier número de fotogramas por segundo y haya pasado lo que haya pasado
antes. Aquí está por qué ya no puede fallar, dónde está cada pieza y cómo se comprueba. Las reglas
de la versión web: [`docs/MOVIMIENTO.md`](../../docs/MOVIMIENTO.md).

Las secciones 1 a 5 son el reloj (V39, primera parte). Las 6 a 10 son lo que rige en cada sitio
(V39, segunda parte): qué gravedad hay, qué suelo, cuál es el arriba, qué se pesa a bordo y qué
enseña la brújula, **sin depender de en qué cuerpo se está, de dónde nació cada cosa, de la
velocidad, de los fotogramas ni de lo que pasó antes**.

## 1. La causa de raíz (V37)

Había **dos relojes**. Las estructuras avanzaban cada fotograma, con lo que durase; el jugador, en
pasos propios de 1/60 s con su acumulador, y se dibujaba un paso tarde. De ahí salía todo:

| Síntoma | Por qué |
|---|---|
| Con la mochila dentro de una nave rápida, teletransportes | Se preguntaba «¿en qué sala estoy?» con la posición de mundo del jugador, que era de un instante anterior al de la nave: a 1600 m/s, 24 m por detrás, fuera de la nave. Se perdía la nave en pleno aire |
| Tirón al salir de la nave | Al dejar de ir en el marco de la nave, la cámara caía de golpe uno o dos pasos de la velocidad de la nave |
| La nave parpadea después | El cuerpo iba y venía hasta un paso de esa velocidad respecto al casco según el resto del acumulador: lo tocaba, la nave lo recogía, lo soltaba… |

La V38 lo **compensaba** dentro del jugador (qué parte de su velocidad se daba en qué reloj, y
cómo cambiar de una a otra sin que se viera). Funcionaba, pero era el jugador sabiendo de
velocidades ajenas. La V39 quita la causa: no hay nada que compensar.

Al buscarla aparecieron otras tres dependencias de la velocidad, estas en la física del mundo y
en el propio jugador, que tampoco eran del jugador sino del mismo principio:

| Dónde | Qué pasaba | Medido antes → ahora |
|---|---|---|
| `core/structure/physics.rs` | La velocidad de cada cuerpo se guardaba en simple precisión: a velocidad orbital un empuje pequeño se perdía en el redondeo | un empuje de 5 mm/s² a 1600 m/s daba 14,7 mm/s en 2 s en vez de 10 → 10,0 |
| `core/structure/physics.rs` | Lo que una nave lleva sujeto (carga anclada, otra nave en su cuna) contaba como **quieto en el mundo** para lo que chocaba con ello | un bloque suelto que toca a 0,5 m/s uno sujeto a otro, todos a 1600 m/s, acababa 2,6 km atrás → igual que parados |
| `app/pilot` | Chocar con algo en marcha quitaba la velocidad **de mundo** hacia la pared; caer en una cubierta que baja hacía botar | contra un bloque a 300 m/s quedabas 73 m atrás → te quedas contra él |

## 2. El principio

**Nada sabe a qué velocidad va nada. Cada cosa se mueve con el mundo, en su mismo instante, y lo
que toca la frena contra eso que toca.**

1. **Un solo reloj: el del mundo.** `Structures::simulate_with` (`core/structure/schedule.rs`)
   avanza las estructuras en lonchas de 1/60 s como mucho (`physics::slices`) y, después de **cada**
   loncha, da ese mismo `dt` a lo que vive entre ellas: el rasgo `Among`. No hay acumuladores ni
   pasos fijos aparte, ni interpolación: lo que se dibuja es dónde está cada cosa ahora.
2. **A bordo se está donde se está en la nave** (`Ride::local`). La posición de mundo se rehace de
   ahí al empezar cada loncha. Quien pregunte dónde está el jugador *en una nave* (salas, sonido)
   usa ese sitio.
3. **Lo que se toca te para contra sí mismo** (`pilot/walk.rs`, `passing`): suelo, techo, pared,
   escalón y la cubierta que buscan las piernas recortan la velocidad relativa a la pieza tocada;
   las botas frenan lo que resbalas respecto a lo que pisas. En la física, lo sujeto es parte del
   cuerpo que lo sujeta (`part_of`).
4. **La velocidad se guarda entera.** En la física, la de cada cuerpo es exacta (`Body::v0`) y el
   paso trabaja sobre lo que le añade. En el jugador es un vector de mundo que cada paso se parte
   en «nivel» y «arriba» según el arriba de ese sitio, sin perder nada al dar la vuelta al cuerpo.
5. **Ninguna regla mira la historia.** En el aire dentro de las salas de una nave, esa nave te
   lleva, la hayas pisado antes o no. La mochila te estabiliza respecto a lo que tengas cerca y
   vaya más como tú *ahora* (`pilot/pack.rs`, `Hold`).
6. **Lo que vive entre estructuras las mantiene a su paso.** Qué estructuras se simulan en fino
   se decidía por la distancia a la cámara; pero al jugador se le da cada loncha esté donde esté
   la cámara. Con la cámara lejos (un guion, la vista de un misil, el menú de inicio) la nave de
   al lado esperaba a su paso grueso mientras el jugador seguía: otra vez un fotograma de
   diferencia. Ahora quien vive entre estructuras dice dónde está (`Among::at`) y el mundo lo
   cuenta como un observador más (`Structures::watch`).

## 3. Las piezas

| Pieza | Dónde | Qué hace |
|---|---|---|
| `Among` | `core/structure/schedule.rs` | Lo que vive entre estructuras: `slice(set, bodies, dt)`. El mundo lo llama tras cada loncha (`physics_among`), con lo sujeto ya puesto donde fue quien lo sujeta. `at()`: dónde está; lo que tiene alrededor se simula en fino |
| `Structures::simulate_with` | ídem | El paso del mundo con sus `Among`. `Builds::update` se lo pasa; `play.rs` pasa al jugador y a `rounds::Flight` |
| `Pilot` | `app/pilot/mod.rs` | Estado, asiento, vistas y `step`: lo que una loncha le hace. `begin` recoge las teclas del fotograma; `impl Among` |
| el cuerpo | `app/pilot/walk.rs` | Tres esferas entre las piezas: `collide`, `step_on`, `under`, `room`, `headroom`, `passing` |
| la mochila | `app/pilot/pack.rs` | Chorros, gasto, y a qué te estabiliza (`Hold`: quieto respecto a lo que te lleva o al suelo; junto a una estructura tal como va; una velocidad que tenía lo que perdiste de vista) |
| los números | `assets/defs/scenario.jsonc` → `player` | `cuerpo` (masa, esferas, agachado, pendiente, piernas, rodillas, cabeza, deriva), `mochila` (empujes, gastos, tiempos, `junto`: alcance y cuándo cambiar de referencia), `linterna`. En el código no queda ninguno; solo holguras numéricas con nombre en `walk.rs` |

### El fotograma (`play.rs`)

| Momento | Qué |
|---|---|
| inicio | viento y sala (`pilot.cabin`) con el jugador y las naves del mismo instante; `pilot.begin(teclas, opciones)` |
| naves | `ships.update(dt)`: sistemas, empujes, mecanismos; el asiento que se mueve lleva al sentado |
| mundo | `builds.update(dt, …, [jugador, proyectiles])`: por cada loncha, poses iniciales → estructuras → lo sujeto → jugador y proyectiles |
| vista | desde donde está el jugador (`view_aboard`, `eye`); nuevas emisiones y dibujo de proyectiles desde ese mismo instante |

## 4. Pruebas

Núcleo (`cargo test -p lunar-core --test schedule --test physics`):

- `what_lives_among_structures_is_of_their_instant_at_every_slice`: un punto junto a una
  estructura en caída libre, a 0 / 300 / 1600 / 7800 m/s y con fotogramas de 4 a 100 ms: en cada
  loncha está donde estaba respecto a ella.
- `the_smallest_push_counts_the_same_however_fast_it_goes`,
  `what_a_body_holds_is_that_body_to_what_runs_into_it`.
- `what_lives_among_structures_keeps_them_stepped_with_it_wherever_the_watcher_is`: lo mismo con
  quien mira a 100 m, a 5 km y a 500 km. Con la regla anterior el punto se iba 1,15 m de una
  estructura que cae parada vista desde 5 km (y cientos de metros a velocidad orbital).

Jugador (`cargo test -p lunar-app pilot::`): `Loop` corre el fotograma como el juego —las teclas,
y el mundo con el jugador dentro—, con las estructuras movidas a mano o con la física real, y mide
el **salto del ojo de un fotograma a otro en el marco de la nave**:

- `a_ship_left_stays_where_it_is_at_any_speed_and_any_frame_rate`: 0, 30, 300, 1600 y 7800 m/s ×
  10 / 30 / 60 / 144 / 240 fps, desigual y a trompicones, a mano y con la física: milímetros.
- `in_the_air_in_a_cabin_under_way_the_ship_keeps_you` (un Alcotán de verdad),
  `in_the_rooms_of_a_ship_never_boarded_it_carries_us`,
  `what_goes_as_we_do_is_kept_to_though_we_never_rode_it`,
  `a_ship_that_speeds_up_leaves_us_without_a_jolt`, `beside_a_ship_that_falls_we_fall_with_it`,
  `down_on_the_ground_with_speed_nothing_jolts`.
- `a_jump_on_a_deck_under_way_comes_down_where_it_went_up`,
  `steps_are_walked_the_same_on_a_deck_under_way`, `set_on_a_deck_under_way_we_are_still_to_it`,
  `what_is_bumped_into_stops_you_against_itself_not_against_the_world`.

En el juego (`tools/camara/mochila_nave.jsonc`, ventana oculta): un Alcotán soltado a 9 km cae
cada vez más deprisa; el jugador, de pie en su techo, despega con la mochila y se queda a su lado
con las manos fuera, y luego flota en su cabina. El paso de guion `donde` escribe dónde tiene los
pies en el marco de la nave, qué lo lleva y a qué velocidad va respecto a ella.

## 5. Algo nuevo que se mueve entre estructuras

| Quiero… | Hago |
|---|---|
| Que algo ande, flote o vaya entre naves (un tripulante, un dron, otro jugador simulado aquí) | Implementar `Among` y pasarlo a `Builds::update`. Nunca un acumulador propio |
| Saber dónde está el jugador en una nave | `pilot.ride` → `local` |
| Su velocidad de mundo | `pilot.velocity_in(set)` (a bordo suma la de la nave); `velocity()` es en lo que lo lleva |
| Que algo que sale de él (gas, un objeto) nazca bien | `motion_in(set).velocity_at(punto_de_salida)`, en el mundo; incluye el giro de lo que lo lleva |
| Poner al jugador en una nave en marcha (un guion, una aparición) | `put` y luego `still_to(set, id)` |
| Cambiar cómo se mueve el cuerpo o la mochila | `scenario.jsonc`, `player.cuerpo` / `player.mochila` |
| Probar algo junto a una nave rápida | `Loop` en `app/pilot/tests.rs`, barriendo velocidades y duraciones de fotograma |

## 6. Lo que rige en un punto: una sola función

Había cuatro dependencias, las cuatro del mismo tipo: algo se decidía por un dato que no era
«dónde está esto ahora».

| Dependencia | Qué pasaba | Ahora |
|---|---|---|
| La gravedad no se acababa nunca y solo contaba un cuerpo | `Body::gravity_at` caía con el cuadrado de la distancia para siempre: a 30 km la Luna tiraba 1,57 m/s² y a 90 km aún 1,46 | Cada cuerpo declara hasta dónde llega (`reach`); fuera de todos no hay gravedad, ni suelo, ni arriba |
| Las naves eran del cuerpo donde nacieron | `Structure::body` se fijaba al crearlas: una nave hecha en la Luna y llevada a la Luna menor seguía tirada por la Luna (se iba 1,61 m en 2 s, en vez de caer 0,37 m hacia la menor) y con el suelo de la Luna | `Structure::body` no existe. La física pregunta en cada loncha qué rige donde está el cuerpo |
| El arriba del jugador era el del cuerpo más cercano | A bordo el suelo era lo que mirase hacia ese arriba, no la cubierta; se pesaba lo del cuerpo aunque la nave cayera | El arriba es hacia donde se pesa, y lo que se pesa lo dice lo que te lleva (§8) |
| El rumbo salía de un eje fijo del mundo | Norte = −Z del mundo proyectado, con otro eje a menos de una diezmilésima del radio del punto donde −Z es la vertical: al cruzar ese borde el rumbo y la mirada saltaban 90° | El jugador lleva su propio marco (§8) y la brújula es un sistema de referencias (§9) |

**`BodyRegistry::field(p)`** (`core/src/body.rs`) es la única respuesta a «qué gravedad, qué
suelo y qué arriba hay aquí». Devuelve `Field`:

| Campo | Qué es |
|---|---|
| `pull` | El tirón en ese punto (vector, m/s²). Cero fuera de toda influencia |
| `ground` | El cuerpo cuyo suelo hay debajo: el más cercano de los que llegan hasta ahí. `None`: no hay suelo |
| `hold` | Cuánto del punto tienen los cuerpos entre todos (0: espacio libre; 1: gravedad entera de alguno) |
| `nearest` | El cuerpo de superficie más cercana, esté a lo que esté: solo para dibujar (su suelo, su cielo). Nada que se mueva se rige por él |
| `g()`, `up()` | El módulo del tirón y el arriba (contra el tirón; `None` donde nada tira) |

La usan la física de estructuras, el jugador, los proyectiles, las partículas, los misiles, las
explosiones (el tamaño del cráter), lo que se le cuenta a cada nave (`lunar_ship::World::at`: de
ahí beben el ordenador de vuelo y el piloto automático) y, para su velocidad de órbita, el
tráfico (`Body::own_pull`, la ley de un cuerpo solo, para formas cerradas alrededor de él).

### La influencia de cada cuerpo (dato)

`assets/defs/bodies/<id>.jsonc`, obligatorio:

```jsonc
// entera (cae con el cuadrado de la distancia) hasta `to - band` m sobre el datum;
// de ahí baja suave a cero en `to`
"reach": { "to": 30000.0, "band": 10000.0 },
// hacia dónde apunta su eje: su polo norte. El norte de su suelo es hacia él
"north": [0.0, 0.0, -1.0],
```

| Cuerpo | Entera hasta | Nada desde |
|---|---|---|
| Luna | 20 km | 30 km |
| Luna menor | 3,5 km | 6 km |

- La bajada es una curva de quinto grado (`Body::hold`): su pendiente y su curvatura son cero en
  los dos extremos de la franja, así que al entrar o salir no hay tirón ni cambio brusco de
  aceleración.
- **Dos cuerpos a la vez:** el que llega menos lejos releva al otro en la medida en que rige
  (`BodyRegistry::shares`). Dentro de la influencia de la Luna menor manda la Luna menor aunque
  ahí llegue también la de la Luna, y a lo largo de la franja de la menor una entra tan suave
  como sale la otra. No hay «cambio de cuerpo»: hay pesos que se cruzan.
- El tráfico en órbita no vuela por encima de donde la gravedad es entera (`whole_to`).

## 7. Las estructuras, por donde están ahora

`core/structure/physics.rs`. En cada loncha, por cada cuerpo que se mueve, **una** consulta a
`field` en su centro de masas (`Physics::fields`): de ahí salen el tirón que se le aplica y de
qué cuerpo es el suelo que tiene debajo. Lo que se guarda por estructura para no preguntar el
suelo cada paso (el parche: `Patch`) lleva de qué cuerpo es y se vuelve a tomar si ha cambiado;
sin suelo no hay parche ni contactos con el terreno. `Structure::grounded` y el umbral de empuje
para despertar (`shoved`) usan el tirón de ahí.

También por donde están ahora: el durmiente que se pone al día (`schedule::coast`), lo que le
estorba a un mecanismo (`obstruct`), `rest_on_ground` y el ordenador de cada nave.

Cada estructura dice además dos cosas que necesita lo que lleva encima:

| Campo | Qué es | Quién lo pone |
|---|---|---|
| `Structure::acc` | Cómo cambió su velocidad en su último paso (m/s²; cero en reposo) | La física |
| `Structure::rooms` | Sus salas: cajas en su marco. Lo que está en una, está a bordo | Quien la hace (una nave: sus compartimentos) |
| `Structure::gravity` | La gravedad que da en sus salas (`OwnGravity`: `g` y cuánta hay ahora) | Los sistemas de la nave, de sus datos |

## 8. El jugador: su marco, su peso, su arriba

### Qué se pesa (`core/structure/weight.rs`, `felt`)

Una regla para cualquiera que no sea una estructura (el jugador hoy; un tripulante o una carga
posada mañana), desde dónde está y qué lo lleva **ahora**:

| Situación | Qué pesa | Hacia dónde |
|---|---|---|
| Por su cuenta | El tirón del sitio (`field`) | Hacia donde tire. Fuera de toda influencia: nada |
| En las salas de una estructura que da gravedad propia | Esa gravedad (`gravedad.g` de la nave) | Hacia su cubierta, esté como esté y donde esté |
| Llevado por una estructura sin gravedad propia (o fuera de sus salas) | El tirón del sitio **menos lo que acelera lo que lo lleva** | Todo si está posada o sostenida; nada si cae o va en órbita; lo que empujen sus motores, al revés, si va con empuje; y hacia fuera si gira |

La misma regla dice si se está de pie sobre algo: se pesa sobre lo que se toca solo en la medida
en que eso no se va cayendo igual que tú. Sobre una nave en caída libre no se está de pie.

### La gravedad de a bordo (dato de la nave)

`assets/defs/ships/<nave>.jsonc`, obligatorio:

```jsonc
"gravedad": { "g": 1.62, "senal": "abordo.gravedad", "tiempo": 2.0 },
```

`g` 0: no da ninguna (el Abejorro y el Azor, que se vuelan con el traje). `senal`: la señal que
dice si funciona (en el Alcotán y el Cachalote, la derivada `abordo.gravedad` =
`elec.bus_ess > 18 V`: sin barra esencial no hay gravedad). Entra y sale en `tiempo` segundos,
nunca de golpe. Lo comprueba para todas las naves `ship/tests/gravedad.rs`.

### El arriba y hacia dónde se mira (`app/src/pilot/mod.rs`)

- **El arriba del cuerpo es hacia donde pesa.** A bordo con gravedad propia, la cubierta; en un
  cuerpo, su vertical; sin peso, **el que traía**: nada lo gira.
- **Lo que te lleva es tu marco:** gira contigo dentro (`turned`): tu arriba, tu frente y cómo
  vas en él.
- **Enderezarse** (`right`): el arriba del cuerpo va hacia el que da el peso a `enderezar.ritmo`
  del ángulo que falta, `enderezar.giro` rad/s como mucho, y solo a ese ritmo desde un peso de
  `enderezar.peso`; con menos, tanto más despacio. En el borde de una influencia casi nada pesa
  y casi nada gira: por eso entrar no se ve. Pasar del suelo a una cubierta inclinada o de una
  nave a otra es el mismo giro, a su ritmo.
- **El frente no sale de ningún eje del mundo.** El jugador guarda `up` y `fore` (el frente
  desde el que se cuenta su giro) y los lleva consigo al cambiar el arriba, por el giro más
  corto. No hay punto de ningún cuerpo donde la mirada salte.
- **Sin peso no se anda:** por debajo de `cuerpo.sin_peso` (0,05 m/s²) sobre lo que hay bajo los
  pies las botas no agarran y se flota. Con `sin_peso` a 0 las botas agarrarían cualquier
  cubierta que tocasen.

### La mochila, como en Space Engineers (`app/src/pilot/pack.rs`, V40)

- **Empuja hacia donde miras.** En el aire, W empuja hacia donde mira la cámara, también hacia
  arriba o hacia abajo si miras así; A y D, a los lados de la vista. Espacio y Ctrl, a lo largo
  del arriba del cuerpo. (Antes W/A/D empujaban solo a nivel y la mirada arriba o abajo no
  contaba.) En el suelo W sigue siendo andar.
- **Flotando, el cuerpo es tuyo** (`Pilot::floating`: mochila encendida, en el aire, sin peso por
  encima de `cuerpo.sin_peso`). El ratón gira el cuerpo entero, sin tope al mirar arriba o abajo
  (puedes dar la vuelta completa), y **Q / E alabean** a `mochila.alabeo` rad/s. La mirada pasa a
  ser la del cuerpo (`fold_look`) la primera vez que mueves el ratón flotando. Al apagar la
  mochila **te quedas como estabas**, girado y con tu velocidad: nada te endereza donde nada pesa.
  E sigue siendo sentarse si apuntas a un asiento.
- **Con peso te enderezas** hacia donde pesas, como siempre (`right`), con la mochila o sin ella.
- Sostiene contra lo que peses: donde nada pesa no tiene nada que sostener y parada no gasta.
- **El estabilizador siempre tiene a qué sujetarte.** Te estabiliza respecto a la nave que tengas
  cerca o al suelo donde lo hay; sin suelo y sin nada cerca, **respecto a la estructura más
  cercana tal como va, y si no hay ninguna, respecto al marco de los cuerpos** (los cuerpos están
  quietos en el mundo). Antes, en el espacio libre, no frenaba nada (`Hold::Free` queda solo para
  quien no lleva mochila). Z lo apaga: entonces sigues con lo que llevas.
- Pruebas: `pilot/frames.rs` → `floating_the_body_turns_every_way_and_the_pack_pushes_where_we_look`
  (vuelta completa con el ratón, alabeo con E, W hacia donde se mira, al apagar se queda como
  estaba, el estabilizador frena lejos de todo) y `past_every_reach_nothing_pulls_and_nothing_turns_us`.

### Los números (`scenario.jsonc` → `player.cuerpo`)

| Dato | Valor | Qué es |
|---|---|---|
| `enderezar.ritmo` | 8 1/s | Parte del ángulo que falta que se recorre por segundo |
| `enderezar.giro` | 2,5 rad/s | Lo más deprisa que se endereza |
| `enderezar.peso` | 0,5 m/s² | Peso desde el que se endereza a ese ritmo |
| `sin_peso` | 0,05 m/s² | Por debajo, no se anda: se flota |

## 9. La brújula: referencias por régimen (`app/src/nav.rs`, `navegacion.jsonc`)

No es un rumbo. Es una cinta alrededor de tu propio arriba en la que se marca **cada referencia
que significa algo ahora**, tan sólida como pese su régimen.

| Régimen | Pesa | Referencia | Marcas |
|---|---|---|---|
| `superficie` | Lo que el cuerpo tiene del sitio, mientras no orbitas | El norte del cuerpo (su eje) | N, NE, E… y la escala RUMBO |
| `orbita` | Lo mismo, cuando sí | Hacia donde vas alrededor del cuerpo | PRO, RETRO, RAD±, NOR± y la escala MARCHA |
| `espacio` | Lo que no tiene ningún cuerpo | La nave que te lleva, la que tienes al lado o la más cercana; y el sol | PROA, POPA, la nave con su distancia, SOL y la escala PROA |

- **Órbita o superficie** no es una altura ni una velocidad escritas: es ir de lado a una parte
  de la velocidad de órbita circular de donde estás (`orbita`: de 0,45 a 0,85). Vale igual en la
  Luna (1 678 m/s a ras de suelo) que en la Luna menor (68 m/s).
- **Nada salta.** Los pesos son funciones suaves de dónde estás y cómo vas; al cambiar de
  régimen unas marcas se apagan mientras salen las otras. Con dos cuerpos a la vez salen las de
  los dos. Las cifras bajo la marca son las de la escala que más pesa y solo se ven cuando pesa
  más de la mitad: nunca cambian a la vista.
- **Sin puntos singulares.** Cerca de un polo el norte se apaga (`polo`); una dirección casi
  vertical se apaga (`vertical`) y dice cuántos grados tiene por encima o por debajo; una marca
  de borde justo detrás se apaga antes de cambiar de lado (`Mark::shown`).
- **Declarado en datos:** qué regímenes hay, de qué pesa cada uno, desde qué dirección cuenta
  su escala y qué marcas tiene (`hacia`: `norte`, `marcha`, `radial`, `normal`, `proa`, `nave`,
  `sol`; `contra`, `giro`, `texto`, `mayor`, `borde`). Una referencia nueva es una entrada en
  `Dir` y una línea en `Nav::way`.
- El HUD no sabe nada de esto: pinta lo que `Nav::compass` le deja en `Hud::compass`.
- Cada nave tiene además la señal `nave.rumbo` (grados desde el norte del cuerpo; −1 donde no
  hay norte), que usa el piloto automático.

## 10. Pruebas de lo que rige

Cada prueba barre lo que no debe importar: los dos cuerpos que hay y uno inventado (otro radio,
otra gravedad, otro alcance, otro eje); dentro, en la franja y fuera de cada influencia, y donde
se juntan dos; de 0 a 7 800 m/s; fotogramas de 10 a 240 por segundo; la nave derecha, de lado,
boca abajo, encabritada y dando tumbos.

| Prueba | Qué mide | Con la regla anterior | Ahora |
|---|---|---|---|
| `core/tests/field.rs` `a_bodys_pull_ends_where_its_data_says_and_fades_without_a_jolt` | Tirón entero hasta su dato, cero desde su alcance; pendiente y curvatura en la franja | a 30 km la Luna tiraba 1,57 m/s² | 0; pendiente ≤ 1,83 g/franja, curvatura ≤ 5,6 g/franja² |
| `where_two_reaches_meet_one_gives_way_to_the_other` | Cambio del tirón por metro de la Luna a través de la Luna menor | 0,84 m/s² por metro (un salto al cambiar de cuerpo) | 0,0011 m/s² por metro |
| `a_structure_is_ruled_by_where_it_is_now_not_by_where_it_was_made` | Un bloque hecho en la Luna, puesto sobre cada cuerpo | sobre la Luna menor se iba 1,61 m en 2 s, tirado por la Luna | 0,37 m hacia la menor (su g) |
| `past_every_reach_a_structure_keeps_its_speed_and_meets_no_ground` | Cambio de velocidad en 5 s fuera de todo, de 0 a 7 800 m/s | 7,8 m/s | < 10⁻⁹ |
| `a_sleeper_catches_up_as_it_is_pulled_where_it_is` | Lo que cae un durmiente al despertar | caía 3,1 m donde no hay gravedad | lo que le toca |
| `app/pilot/frames.rs` `aboard_a_ship_with_gravity_of_its_own_the_deck_is_the_floor_…` | 250 casos: de pie, andando y saltando en la cubierta | falla en el primero (pesaba 1,605, lo del cuerpo) | salto del ojo < 2 mm; anda 1,8 m/s por la cubierta; el salto cae donde subió |
| `aboard_what_makes_no_gravity_one_weighs_what_is_left_of_the_pull` | Sostenida, en caída libre, con empuje; a mano y con la física | con empuje de 4 m/s² pesaba 1,605 | 4,00; en caída libre 0, ni un fotograma de pie, ojo < 2 mm |
| `past_every_reach_nothing_pulls_and_nothing_turns_us` | Espacio libre: recto, sin girar, la mochila sin gastar | «algo rige» | giro 0 exacto; gas intacto |
| `across_the_edge_of_a_reach_nothing_jumps` | Entrar en una influencia a 60, 400 y 2 500 m/s | giraba antes de entrar | salto del ojo < 1 µm sobre lo que da el tirón; giro ≤ 2,5 rad/s |
| `from_one_way_up_to_another_the_body_rights_itself_at_its_own_pace` | Enderezarse 3°, 25°, 80° y 170° | no se enderezaba (saltaba) | a su ritmo, sin pasarse, ojo a < 5,7 m/s |
| `how_one_faces_has_no_place_where_it_flips` | Andar sobre los puntos donde −Z y +Z del mundo son la vertical y sobre los polos | la mirada saltaba 1,57 rad | 2·10⁻⁸ rad por paso (la curvatura) |
| `app/nav/tests.rs` `nothing_on_the_tape_ever_jumps` | Lo que salta algo en la cinta de un paso a otro, en todo régimen | — | 0,25° (el paso del giro); solidez ≤ 0,011 |
| `the_heading_it_replaces_broke_where_this_does_not` | El rumbo de antes sobre su punto singular | 90° en un paso de 5 cm | 0° |
| `ship/tests/gravedad.rs` | La gravedad de a bordo de cada nave sale de sus datos y de lo que la alimenta | — | — |

«Con la regla anterior» está medido de verdad: la regla vieja repuesta en una copia del árbol
(`.local/tmp/antes.py a|b|c`), o calculada dentro de la propia prueba donde era una fórmula.

En el juego: `tools/camara/espacio.jsonc` lleva un Alcotán del suelo a órbita, fuera de la
influencia de la Luna y a la Luna menor, con el jugador dentro y fuera. El paso `donde` escribe
qué cuerpo rige y cuánto, el tirón, lo que pesa el jugador, a cuántos grados está su arriba del
de la nave y qué lee la brújula; `poner` deja la nave a tal altura sobre tal cuerpo con tal
velocidad.

## 11. Algo nuevo que dependa de dónde está

| Quiero… | Hago |
|---|---|
| Saber qué gravedad, qué suelo o qué arriba hay en un punto | `bodies.field(p)`. Nunca el cuerpo «de» algo, ni `dominant` (eso es solo para dibujar) |
| Un cuerpo celeste nuevo | Su `.jsonc` con `reach` y `north`. Nada más: todo lo de arriba vale |
| Que una estructura dé gravedad a bordo | `rooms` y `gravity` en la estructura (una nave: `"gravedad"` en sus datos) |
| Saber qué pesa algo que anda o flota | `weight::felt(tirón, punto, lo que lo lleva, si va con ello, si está en sus salas)` |
| Una referencia nueva en la brújula | Una entrada en `nav::Dir` y sus marcas en `navegacion.jsonc` |
| Probar algo «en cualquier sitio» | `places()` de `app/src/pilot/frames.rs`: los tres cuerpos, dentro, en la franja y fuera |

## 12. Disparar y soltar algo en movimiento (V39, 2026-10-06)

El informe anterior era demasiado amplio: **los proyectiles no iban en las lonchas del mundo**.
`Blasts::update` los adelantaba un fotograma contra las estructuras todavía sin mover. Además,
el lanzador de mano no entregaba la velocidad del jugador y `fire_from` añadía otro metro a
una boca ya situada. Una nave rápida dejaba el cohete atrás; sumar únicamente su velocidad
tampoco arreglaba el rayo de colisión contra un casco de otro instante.

Referencia contrastada: la versión web del propio proyecto, `shared/frames/ballistic.ts`
(`launch`, `stepBallistic`, `handOver`) y `client/fx/projectiles.ts` (`sweepShips`). Allí la
velocidad de salida incluye la del lanzador y cada extremo del barrido se transforma con la
pose que le corresponde. No se ha copiado su reloj fijo: Rust conserva el del mundo.

### Contratos compartidos

- `core/structure/motion.rs`, `Motion`: punto, velocidad y giro de un emisor o agarre.
  `velocity_at(punto)` incluye la velocidad tangencial. El jugador lo ofrece con
  `Pilot::motion_in`; equipo y manos consumen el mismo contrato. No conoce naves concretas.
- `Blasts::fire_from`: el origen recibido **ya es la boca**. Los disparos con velocidad usan
  el mismo `fire_round` que las armas montadas. Se suman velocidad heredada y velocidad de
  salida de `shots.jsonc`. La vida sale del alcance y velocidad propios, no de la velocidad
  del mundo. El morro dibujado conserva la dirección de lanzamiento; no gira hacia el vector
  de velocidad heredada. `Seen::Launch` conserva ambos por separado (`dir`/`speed` y `vel`) al reproducir un disparo.
- `Among::wake`: permite mantener en fino lo que un móvil va a atravesar, incluso lejos de
  la cámara. `Among::before`: toma las poses antes de la loncha. Son ganchos genéricos, sin
  ningún reloj adicional. `rounds::Flight` los usa y vuela en `slice` después de las estructuras.
- `Sweep`: guarda las poses iniciales y construye un BVH de los volúmenes recorridos. Solo
  examina los candidatos. El extremo inicial del proyectil va al marco inicial del sólido;
  el final, al final. El rayo local entre ambos no contiene el desplazamiento común. Sirve
  también para una pared que cruza un proyectil parado en el mundo. Coordenadas de mundo en
  doble precisión, coordenadas locales para la geometría; tolerancias numéricas con nombre.
- El impacto conserva **estructura, punto y dirección locales**. Al terminar el mundo,
  `land_rounds` aplica el daño a esa estructura y coloca allí la explosión: no vuelve a
  disparar un rayo de mundo contra una nave que entretanto ha avanzado. El terreno compite
  con el casco por el primer contacto, no pierde siempre ante cualquier estructura.
- Las imágenes de proyectiles se preparan **después** de mover el mundo y de emitir los
  nuevos disparos: uno recién disparado se ve en la boca ese mismo fotograma.
- La comprobación visual descubrió lo mismo en la **explosión**: su nube, luz y sacudida
  nacían quietas en el mundo. `Effects::explode_moving` recibe el mismo `Motion`, cualquiera
  que sea el emisor. `land_rounds` entrega la velocidad del punto alcanzado. Las partículas
  guardan esa base en doble precisión (`drift`) separada de su expansión (`vel`); el frenado
  y el estiramiento visual solo actúan sobre la expansión, no sobre el movimiento común.
  Destello y origen de sacudida avanzan con la velocidad recibida. La sacudida se evalúa con
  la cámara final, no con su posición anterior al paso del mundo. El suelo inicial cacheado
  de una explosión elevada es el suelo real, no un plano ficticio a la altura donde nació.
- `Hands::update` amortigua contra `Motion`, no contra cero cuando el jugador sale de una
  nave. Soltar no recrea la caja ni cambia su velocidad o giro: conserva los que ya tenía.
  La carga anclada sigue usando `hold::let_go`, que ya hereda la velocidad del punto de amarre.

### Aceleración y datos

Soltar algo transmite la **velocidad instantánea**, incluido el giro. La aceleración no es
algo que se almacene y se herede para siempre: después de salir actúan sus propias fuerzas.
La gravedad común continúa; los motores de la nave o del traje no siguen empujando el cohete
separado. Los cohetes actuales son balísticos a la velocidad de salida escrita en `shots.jsonc`;
no se ha inventado un motor ni cambiado su potencia. **No hay nuevos datos de comportamiento**
ni umbrales por velocidad, cuerpo, nave o fps.

### Pruebas y regresiones

| Prueba | Qué observa | Antes / control negativo → ahora |
|---|---|---|
| `blasts::tests::a_handheld_round_starts_at_the_muzzle` | Boca real, velocidad, morro, duración y reproducción del disparo | El código original nace 1 m por delante; quitando la herencia falla desde 30 m/s → nace en la boca y conserva ambas velocidades |
| `pilot::shots::rockets_and_the_eye_share_the_world_at_every_speed_frame_body_and_attitude` | `Loop`, 2.500 vuelos, tres cuerpos, influencia/franja/fuera, 0–7.800 m/s, 10–240 fps, cinco posturas, giro y aceleración; física real y movimiento controlado | Volviendo al rayo contra la pose final falla ya a 300 m/s (2,247 mm); con el barrido temporal el mayor cambio del impacto es 0,001 mm; ojo por fotograma <1 mm |
| `pilot::shots::a_release_uses_the_velocity_of_its_point_aboard_and_after_leaving` | Velocidad tangencial en la boca y continuidad al dejar el marco | Comprueba que no se usa solo la velocidad del centro o la relativa del jugador |
| `hands::tests::carrying_and_releasing_a_box_uses_the_hands_motion_not_the_world` | Caja quieta respecto a una mano que se traslada y gira; 25 combinaciones velocidad/fps; soltar conserva velocidad y giro | Restaurando el freno respecto al mundo aparecen 1.200 N espurios → menos de 0,00001 N |
| `core/tests/sweep.rs` | Pared móvil, proyectil parado, cámara lejana; comparación explícita con el rayo anterior; 500 estructuras y 2.000 tramos | El rayo anterior no ve la pared; el barrido sí. De 500 estructuras a 5,708 candidatos/tramo; 1,182 ms por loncha en perfil de pruebas |
| `swept_bounds_keep_their_storage_once_warm` | Direcciones y capacidades de los cuatro vectores durante 100 reconstrucciones | Se conservan después del calentamiento; no se usa un asignador inseguro para medirlo |
| `core/tests/effects.rs`, `an_explosion_inherits_motion_without_stretching_or_braking_against_the_world` | 225 combinaciones: tres cuerpos, tres alturas, cinco velocidades y cinco fps; nube, luz y sacudida | Restaurando el nacimiento quieto, la nube queda 3,354 m atrás en un fotograma a 30 m/s; corregido, error <0,001 mm sin cambiar expansión ni aspecto |
| `inherited_particle_motion_is_bounded_in_cost_and_storage` | 20.000 partículas durante 100 pasos, capacidad y dirección del almacén | 0,846 ms/paso; 96 bytes/partícula; almacén conservado |

El guion `tools/camara/cohetes_nave.jsonc` usa el lanzador real a pie: interior quieto, interior
a 7.800 m/s con nave volcada y exterior tras abandonarla. `donde` informa además de proyectiles
en vuelo, velocidad relativa y contactos locales. Las comprobaciones y límites de la entrega
se anotan en `PENDIENTES.md`; no sustituye a probar el tacto jugando a mano.

Entrega verificada: 739 pruebas pasan, 3 omitidas; tres ediciones release recompiladas y sus
arranques correctos. Guion ejecutado desde `SELENE_V39`, nueve fotos leídas y tres contactos
confirmados. El fogonazo de salida permanece junto a la boca tanto quieto como a 7.800 m/s;
la explosión interior acompaña a la nave rápida. Los conos de la receta visual aún se orientan
por la vertical del cuerpo de dibujo, no por la superficie de impacto: queda anotado aparte.

Límites: el barrido aproxima por un segmento la trayectoria local durante cada loncha; no es
CCD exacto de cada pieza articulada. Los misiles estratégicos/guiados y otros emisores siguen
sus rutas anteriores en `Blasts::update`: esta corrección abarca `shots.jsonc` y sus impactos (cohetes, balas,
granadas y cañones). Sigue pendiente unificar la gravedad propia de las salas para toda la
carga libre y todo lo balístico; aquí no se ha cambiado esa regla de juego.
