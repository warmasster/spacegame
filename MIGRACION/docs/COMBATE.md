# Combate: sensores, trazas, armas, señuelos y piloto automático

Todo lo de combate es **genérico**: ninguna línea de código conoce al Azor. Una nave tiene sistema
táctico por montar una máquina de estos modelos; lo demás son datos (componentes, paneles,
secciones de panel, registros de misiles y señuelos). De dónde sale cada idea:
[`INVESTIGACION.md`](INVESTIGACION.md) §Combate.

## Qué hay y dónde

| Pieza | Dónde | Qué hace |
|---|---|---|
| Modelos de máquina `radar`, `alertador`, `irst`, `transpondedor`, `perturbador`, `arma` | `machines/models/combat.rs` | Cajas que consumen, tardan en estar listas y dicen qué radian; lanzadores con cargador, cadencia y cañón que se calienta. No saben qué es un blanco. |
| Modelo `reactor_unificado` | `machines/models/powerpack.rs` | Reactor, refrigeración, conversor y radiador en una unidad: una salida a 28 V. |
| Sistema táctico | `ship/tactical.rs` | De los contactos que le cuentan hace trazas, deja elegir y fijar una, calcula la solución de tiro, decide qué arma dispara y suelta señuelos. Dibuja sus pantallas. |
| Trazas en pantallas | `ship/plots.rs`, instrumento `trazas` de las MFD (`controls/mfd.rs`) | Un sistema dibuja líneas, marcas y palabras bajo un nombre; la pantalla que lo pide lo pinta. |
| Piloto automático | `ship/autopilot.rs` | Dos familias de modos (navegación y combate) como dos listas; cada modo es una función. |
| Ordenador de vuelo configurable | `ship/flight.rs` (`perfil`, `limite`) | Perfil FINO / NORMAL / COMBATE y límite de g del piloto automático. |
| Misiles guiados y señuelos en vuelo | `core/guided.rs` | Navegación proporcional, buscador con campo de visión, señuelos que florecen y se apagan. |
| Enlace con el mundo | `app/tactics.rs`, `app/blasts.rs` | Reparte contactos a las naves que miran y convierte lo disparado en proyectiles, misiles y señuelos. |

Datos: `components/combate.jsonc`, `guiados.jsonc`, `senuelos.jsonc`, `shots.jsonc` (`canon_20`),
`secciones.jsonc` (radar, sensores, objetivo, armas, contramedidas, piloto_nav, piloto_combate,
ordenador_vuelo, grupo_energia), `ships/azor.jsonc`, `panels/azor_*.jsonc`.

## Sensores

- **Radar.** Encuentra lo que está en su sector (60° a cada lado del morro en el RM-9) y a su
  alcance: `alcance` (contra 1 m² a plena potencia) × ⁴√(potencia × lo que refleja el blanco).
  **Escala de alcance configurable** (5, 20, 40, 80 km): la escala es hasta dónde llega la
  pantalla y, con ella, cuánto radia — solo lo necesario para ese alcance. Media escala es la
  dieciseisava parte de la potencia. Encendido y en SILENCIO no radia.
- **Alertador de radar.** Oye a los radares que barren la nave, por rumbo. Como la señal solo
  hace el camino de ida, oye a un radar a unas tres veces la distancia a la que ese radar ve la
  nave: **radiar te delata**. Dice también si te tienen fijada y si viene un misil de radar.
  Fuera del haz solo se oyen los lóbulos laterales, de cerca.
- **Buscador infrarrojo.** Encuentra lo caliente sin radiar: un motor a tope se ve a decenas de
  kilómetros, una nave con los motores apagados casi nada.
- **Transpondedor.** Lo que responde con código sale como traza aunque el radar esté callado (el
  tráfico se ve por su baliza). Código igual al tuyo: AMIGO. Otro código: CIVIL. Sin código:
  DESCONOCIDO. HOSTIL es lo que te fija o te dispara, o lo que tú declares.
- **Perturbador.** Acorta el alcance de los radares que te miran; a cambio te oye cualquiera.
- **Horizonte.** Nada se ve a través del cuerpo sobre el que se vuela.

**Cualquier traza se puede fijar** y se le puede disparar: civil, desconocida o lo que sea. El
único seguro es el maestro de armas. El fuego automático solo tira a lo declarado hostil.

## Pantalla táctica (MFD izquierda del Azor)

- **RADAR:** tú en el centro, el morro arriba. Cuadrado ámbar: desconocida. Círculo cian: civil.
  Círculo verde: amiga. Rombo rojo: hostil. Triángulo rojo: misil. Un ángulo en el borde: solo se
  sabe por dónde (la oye el alertador). La elegida va recuadrada, con su vector de velocidad.
- **ALERT:** anillo del alertador; cuanto más fuerte se oye, más al centro.
- **MIRA:** lo que hay 30° alrededor del morro. Un aro verde con alas: hacia dónde va la nave
  (en el borde si no es hacia delante) y a qué velocidad. Con una traza elegida: su marca
  recuadrada, su distancia y a cuánto te acercas, y una cruz donde hay que poner el morro para
  darle con el arma elegida; EN TIRO cuando da. Sin traza elegida solo enseña el aro: no es un
  visor en el cristal, es una página de la pantalla.
- **ARMAS**, **VUELO**.

## Armas

Un `arma` dice qué lanza (`municion`), de qué clase (`proyectil`, `guiado`, `senuelo`) y su
`grupo`. El selector ARMA cuenta los grupos en el orden en que están montados.

- **Cañones rotativos de 20 mm** (dos): 40 disparos por segundo cada uno, 600 por cañón. El
  proyectil sale con la velocidad de la nave además de la suya. El cañón se calienta y, muy
  caliente, deja de tirar hasta que se enfría.
- **DARDO** (6): corto alcance, sigue el calor. No radia: ningún alertador lo oye venir. Se le
  engaña con bengalas y cortando gases.
- **LANZA** (4): medio alcance, guiado por radar. Hay que fijar la traza. Lo oye el alertador y lo
  engañan los dipolos.
- Un misil por pulsación, alternando lanzadores.

## Señuelos

**En el vacío nada los frena**: un señuelo conserva la velocidad de la nave que lo soltó más la
que le dio el dispensador, así que sigue a su lado hasta que la nave gira o acelera. Se usan
así: soltar y alejarse.

- **Nube de dipolos:** se abre en medio segundo y refleja como un carguero; se aclara según se
  sigue abriendo. Se lleva el blocaje de un radar (y el buscador de un LANZA) si florece junto al
  blanco. Un radar ayudado por el infrarrojo no se deja engañar.
- **Bengala:** unos segundos más caliente que un motor.
- Programa: MANUAL, MISIL (se sueltan solos si viene un misil), FIJADO (también si te fijan).

## Piloto automático

Hecho como el de la versión web (`src/shared/ship/flight/autopilot.ts`): cosas que se apilan, cada
una con su interruptor y su rueda, y programas. Está en la cara NAVEG. del tambor.

**Retenciones** (se pueden poner las tres a la vez):

| Interruptor | Rueda | Qué hace |
|---|---|---|
| ALTURA | ALT m | Mantiene esa altura sobre el suelo (sigue el relieve). |
| RUMBO | RUMBO ° | Gira al rumbo de brújula (0 norte, 90 este) y lo mantiene. |
| VELOC. | VEL m/s | Lleva la velocidad sobre el suelo a esa, hacia el morro (o hacia el rumbo retenido). Bajada a 0, frena y se queda parada. |

Con cualquiera puesta **el ordenador vuela los motores y mueve él las góndolas**: el casco va
siempre horizontal. Lo que no se retiene sigue como va, y las teclas de traslación empujan
(R/F suben y bajan si no hay ALTURA; I/K/J/L aceleran si no hay VELOC.). Fuera de todo cuerpo
solo cuenta VELOC. (a lo largo de como vas): ALTURA y RUMBO no significan nada allí y **lo
dice**: la lámpara ACTIVO parpadea en ámbar y la señal `ap.sin_cuerpo` se enciende (igual
DESPEG. y ATERRIZ.); si no hay nada que volar, la nave queda para el piloto. Antes se encendía
en verde y no hacía nada.

**El peso que sostiene es el de ir como va** (`weight` en `autopilot.rs`): el tirón menos lo
que se lleva dar la vuelta al cuerpo a su velocidad a nivel. Parado, todo su peso; a la velocidad
de órbita, nada (caer alrededor es la órbita: ALTURA en órbita no gasta en sostenerla); más
deprisa, tiene que empujar hacia abajo para no subir. Lo mismo en todos los modos.

**SEGUIR y los modos de combate sin bandazos:** entre «ir a estar con la traza» y «el morro en
ella» hay un margen (`WITH_IT`, `WITH_IT_AGAIN`): no salta de uno a otro y vuelta. Un empuje de
través se toma con el extremo del casco que ya tiene puesto hasta que el otro es claramente
mejor (`OTHER_END`), en vez de dar media vuelta cada vez que el empuje cruza el través. Se acerca
tan deprisa como aún puede frenar contando lo que tarda en responder (`ANSWER`).

**Programas** (selector PROGRAMA):

- **FRENAR:** se para sobre el suelo y se queda sostenida (a la altura retenida, si hay).
- **SEGUIR:** va a la traza elegida y se queda a la DISTANCIA pedida, yendo como ella. Con ALTURA
  puesta, a esa altura; si no, a la de la traza.
- **DESPEG.:** sube en vertical a la altura de la rueda (50 m sin ALTURA) y se queda.
- **ATERRIZ.:** baja en vertical donde estés, más despacio cuanto más bajo (toca a medio metro
  por segundo), y al posarse suelta los motores.
- **ACTITUD**, **PROGR.**, **RETRO:** solo apuntan el morro; los motores son tuyos.

**Combate** (selector de la cara COMBATE; manda sobre lo anterior): PERSEG. (el morro donde hay
que tirar y hasta la distancia pedida de la traza), APUNTAR (solo el morro; los motores son
tuyos), DISTAN. (el morro en la traza y la distancia), EVADIR, SEPARAR.

**La palanca de mando manda sobre todo** mientras esté fuera del centro.

Cómo gira: donde algo pesa, **por el horizonte**: primero de lado hasta el rumbo de la traza,
luego arriba o abajo, con el techo siempre arriba; si está volcada, primero se endereza. Donde
nada pesa, por sus propios ejes y sin alabear. Nunca acaba boca abajo (`aim` en
`autopilot.rs`, y una prueba que lo vuela).

**Góndolas vectoriales.** En el Azor giran de empujar hacia atrás (FRENO) a hacia arriba (VERT.)
y hacia delante (CRUCERO). Con MANTENER, una retención o un programa, las mueve el ordenador a
donde hace falta el empuje (`vuelo.vector`, `vuelo.vector_on`; `flight.rs`), y lo que no dan
ellas lo dan las toberas: por eso frena con el casco horizontal. Sin nada de eso, la palanca
GÓNDOLAS. Una nave lo tiene si en sus datos algo lee `vuelo.vector` (en el Azor, la derivada
`gondolas.mando`); el Alcotán y el Cachalote siguen con su palanca.

Un modo nuevo es una entrada en `NAV` o `COMBAT` y su nombre en el selector de la sección
(`secciones.jsonc`); una prueba comprueba que coinciden.

**Lo que no hace todavía:** ir a un punto del mapa, subir a órbita o bajar de ella.

## Grupo de energía unificado

Hecho para no necesitar manual: **PARO es parar**, sin disparo ni rearme. Si no puede soltar
calor (radiadores dentro) **da menos** en vez de dispararse (lámpara LIMITADO). Si se dispara
(seta, temperatura, daño) la pantalla dice por qué y **se rearma solo** al enfriarse; con MARCHA
puesta vuelve a arrancar. Arranca con las baterías en 6 s y luego ya no las necesita.

## El Azor

Caza monoplaza de 11 m y 12 t, posado en horizontal sobre tres patas largas.

- **Entrar:** panel exterior a babor, tras el ala: ASIENTO baja el asiento por la panza. Te
  sientas (E) y lo subes con ASIENTO en la repisa de tu derecha. Levantarte con el asiento
  abajo te deja en el suelo junto a la nave; con él arriba, en la cabina.
- **Bahía de utilidades:** BAHÍA abre las dos compuertas de popa (rampa de tres tramos y visera).
  Dentro: grupo de energía, ordenador, dos baterías, hidráulica, giróscopos.
- **Tambor de paneles** sobre la consola izquierda: el selector PANEL lo gira. NAVEG. (piloto
  de navegación y ordenador de vuelo), COMBATE (armas, piloto de combate, señuelos), SENSOR.
- **Consola izquierda:** gases, góndolas, motores, disyuntores, masa. **Derecha:** palanca,
  traslación, gatillo. **Repisa derecha:** energía, baterías, mecanismos, luces.
- **Teclas sentado:** vuelo como el Alcotán; **H** gatillo, **M** señuelos, **N** traza
  siguiente, **U** la más cercana, **B** fijar.
- Cabina y bahía van en vacío: se vuela con el traje.
- Las compuertas y la plataforma llevan su propio accionamiento (cilindros electrohidrostáticos y
  husillo): funcionan con la nave fría. El tren va con la bomba hidráulica (HIDRÁUL.).

## Para probarlo

Hay tráfico civil alrededor: enciende IDENTIF. y el alertador (vienen encendidos) y verás trazas
cian sin radar. RADAR + EMITE para el resto. N o U para elegir, B para fijar. Guion de cámara:
`tools/camara/azor.jsonc`.

## Pruebas

`crates/ship/tests/azor.rs` (grupo de energía, sensores y alcances, fijar y señuelo, armas,
piloto, tambor, plataforma y compuertas), más las genéricas de toda nave.
`crates/ship/tests/piloto.rs` **vuela** el piloto automático con los motores, las toberas y el
peso de la nave sobre un suelo: MANTENER y una tecla, DESPEG., las tres retenciones a la vez,
frenar, ATERRIZ., apuntar a trazas detrás y encima sin alabear, y SEGUIR una traza que se mueve. `machines`:
`models::combat`, `models::powerpack`. `core`: `guided`. `ship`: `autopilot`, `tactical`,
`plots`.
