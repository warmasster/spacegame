# Chorros: lo que sale de los motores, las toberas de maniobra y la mochila

Todo propulsor que empuja se ve empujar, en proporción a cuánto empuja: los motores de las naves
(las góndolas del Alcotán y el Cachalote, los motores de elevación del Abejorro), sus toberas de
maniobra (RCS) y de gas frío, y las toberas de la mochila del traje. Un solo sistema, sin código
por nave ni por motor: qué empuja lo dice la nave (o el traje); cómo se ve es un **estilo con
nombre** en `assets/defs/chorros.jsonc`.

## Qué se ve y por qué

En el vacío un chorro no es una llama de vela. El gas sale *subexpandido*: no hay aire que lo
contenga, se abre en abanico nada más salir y se enrarece hasta no verse en unos pocos diámetros
de tobera. Lo que se ve es el gas caliente en la boca y un cono tenue, azulado y translúcido, que
se abre mucho y se apaga pronto. No hay diamantes de Mach (son del aire que aprieta el chorro;
esto es deducción mía, no de una fuente).

Siendo honestos con las fuentes de abajo: un motor hipergólico real en el vacío casi **no se
ve** (el despegue del módulo lunar no deja estela visible), y un chorro de gas frío es
invisible salvo por lo que se congela en él. Aquí se dibuja más de lo real, lo justo para que
el jugador lea qué empuja y hacia dónde, con la forma que tendría si se viera.

| Estilo | Quién lo usa | Qué se ve |
|---|---|---|
| `motor` | motores cohete (`motor_cohete`): `gondola_*` del Alcotán (48 kN) y del Cachalote (65 kN), `motor_*` del Abejorro (4 kN) | boca al blanco, cono azulado de 7,6 m (48 kN) que se abre 26° por lado; ilumina la góndola y el suelo; una bocanada de propelente sin quemar al encender y otra al vaciarse la cámara |
| `rcs` | toberas de maniobra (`rcs`): `rcs_*` del Alcotán (2,2 kN) y del Cachalote (18 kN) | cono corto, pálido y nítido solo mientras dispara (2,1 m a 2,2 kN); al cerrar, la «nieve» de lo que quedaba en las líneas |
| `gas_frio` | las doce toberas `rcs_*` del Abejorro (500 N, 72 s de impulso: nitrógeno), por su dato `"chorro"` | sin fuego: un cono blanquecino que es el sol sobre el gas helado (a la sombra casi nada), con cristales que brillan |
| `gas_mochila` | las diez toberas de la mochila (`tobera_*` en el traje) | conos de 0,4-0,7 m y bocanadas, en la dirección contraria a cada empuje |

Los chorros siguen a su tobera: una góndola que bascula se lleva el suyo (la boca se guarda en el
marco de la pieza de la máquina). El polvo que levantan del suelo **no** es de este sistema: es
de `app/dust.rs`, que ya existía y no se ha tocado.

## Cómo funciona

```
nave / traje                 core::plumes                    render
────────────                 ────────────                    ──────
ship::exhaust   ──Nozzle──►  Exhaust::add  ──Plume──────►  PlumesGpu (un búfer, un dibujo)
 (boca, dirección,           (estilo, memoria,  ──Glow───►  luces (como destellos)
  empuje de cada             distancia, tope)   ──puff───►  partículas (las de siempre)
  propulsor)
```

1. **Qué empuja** (`crates/ship/src/exhaust.rs`). Una máquina es un propulsor si sus datos dicen
   hacia dónde empuja (`empuje`) o algo de su chorro (`chorro`). Al montar el tipo de nave se
   calcula una vez su **tobera** (`MachinePlan::jet`): la boca es el extremo de su componente en la
   dirección en que sale el gas (el borde de la campana), su radio el de ese borde, y su empuje
   nominal el de sus parámetros. Nadie escribe posiciones: salen de la forma de las piezas (se
   pueden dar a mano con `chorro.salida` y `chorro.radio`). `exhaust::thruster(nave, estructura,
   máquina)` da dónde está la boca ahora, hacia dónde sale el gas y cuánto empuja;
   `exhaust::firing` da todos los que empujan (para quien quiera oírlos o levantar polvo con
   ellos).
2. **La mochila** (`crates/app/src/pilot/pack.rs`, `Pilot::push`): lo que la mochila empuja en este
   paso, como vector (arriba 1, abajo 0,6, a los lados y frenando lo que gastan). Sus toberas son
   los puntos del traje que empiezan por `tobera_` (`assets/defs/rigs/astronauta.jsonc`,
   `puntos`: dónde está cada una y, en `palma`, hacia dónde sale su chorro). Cada empuje lo
   responden las toberas cuyo chorro sale a menos de 60° de la dirección contraria, repartiéndoselo.
3. **Cómo se ve** (`crates/core/src/plumes.rs`). Por cada tobera con algo que mostrar,
   `Exhaust::add` saca: su **cono** (largo = `largo` × √kN de empuje; ancho al final = radio de la
   boca + largo × tan `apertura`; luz = nivel^`gamma`), su **luz** sobre lo que tiene alrededor si
   está entre las cuatro más cercanas y brillantes, y su **gas** en partículas (ráfaga al encender
   y al apagar, corriente mientras empuja), dentro del presupuesto. Cada tobera tiene una memoria
   de 16 bytes: el nivel mostrado sube al instante y baja en `desvanecer` segundos (un chorro no
   parpadea), y con ella se detectan el encendido y el apagado.
4. **Cómo se dibuja** (`crates/render/src/plumes.rs`, `shaders/plumes.wgsl`). Una primitiva
   nueva, la única: por chorro, 48 bytes en un búfer de instancias y dos cuadrados en una sola
   llamada de dibujo para todos. Uno es el cono visto de lado (un cuadrado a lo largo del eje que
   gira sobre él para mirar a la cámara); el otro, la boca (un disco de gas caliente mirando a la
   cámara, aplastado según el ángulo, que además es lo que queda al mirar a lo largo del eje y
   desde muy lejos, donde se queda en un píxel repartiendo su luz). Solo suma luz, sin textura:
   la forma son unas líneas de aritmética. Se funde donde toca el casco o el suelo (profundidad de
   la escena) y la boca no se ve si la tobera está tapada.

   *Por qué no partículas:* un chorro continuo hecho de partículas necesita muchas para no verse
   a bolas (es la razón de ser de Waterfall, abajo), habría que ordenarlas cada fotograma con
   las demás y se quedarían atrás al moverse la nave. Un cono por tobera cuesta 4 triángulos.

## Datos

`assets/defs/chorros.jsonc`:

```jsonc
{
  // qué estilo lleva una máquina cuyo dato no nombra ninguno, por su modelo
  "por_modelo": { "motor_cohete": "motor", "rcs": "rcs" },
  // la mochila: su estilo, el prefijo de sus toberas en el traje, el radio de su boca (m) y el
  // ángulo dentro del cual una tobera responde a un empuje
  "mochila": { "estilo": "gas_mochila", "toberas": "tobera_", "radio": 0.02, "cono": 60.0 },
  "estilos": {
    "motor": {
      "nucleo": [10.0, 9.0, 8.5],   // luz del gas en la boca (lineal, HDR)
      "pluma": [0.4, 0.55, 0.95],   // luz del cono
      "largo": 1.1,                 // m de cono por raíz de kN de empuje
      "apertura": 26.0,             // semiángulo de su borde (grados)
      "nucleo_largo": 0.07,         // hasta dónde llega el núcleo (parte del largo)
      "nucleo_ancho": 0.6,          // su ancho (radios de boca)
      "caida": 0.9,                 // lo deprisa que se enrarece al abrirse
      "parpadeo": 0.25,             // 0 fijo .. 1
      "corriente": 4.0,             // velocidad de sus vetas (largos por segundo)
      "desvanecer": 0.12,           // segundos en apagarse al cortar el empuje
      // "sol": 0.85,               // parte de su luz que es el sol sobre él (gas frío): a la sombra no se ve
      // "gamma": 0.6,              // su luz va con el nivel de empuje elevado a esto
      "luz": { "color": [255, 226, 196], "intensidad": 3.0, "alcance": 1.7 },  // alcance: m por raíz de kN
      // ráfagas: "cuantas" partículas del estilo (particles.jsonc); velocidad en largos/s, cono en
      // grados, tamaño en largos, vida en s. "particulas": lo mismo con "por_segundo" a pleno empuje
      "arranque": { "estilo": "bocanada", "cuantas": 10, "velocidad": [0.8, 2.2], "cono": 30.0, "tamano": [0.035, 0.08], "vida": [0.35, 0.8] },
      "apagado": { "estilo": "bocanada", "cuantas": 5, "velocidad": [0.3, 1.0], "cono": 35.0, "tamano": [0.03, 0.06], "vida": [0.4, 0.9] }
    }
  }
}
```

(«largos»: el largo del cono a pleno empuje, así el mismo estilo vale para un motor de 4 kN y
para uno de 65.)

En la máquina de un componente o de una nave (`"maquina"`), opcional:

```jsonc
"chorro": { "estilo": "gas_frio" }                       // otro estilo que el de su modelo
"chorro": { "salida": [0, -2.1, 0], "radio": 0.6 }       // la boca a mano (marco de su pieza)
```

**Un motor nuevo** es un componente con su `maquina` (`modelo`, `empuje`, sus parámetros): su
chorro sale solo, con el estilo de su modelo, del extremo de su tobera y del tamaño de su
empuje. **Un estilo nuevo** es una entrada en `estilos`. **Otra mochila** son otros puntos
`tobera_*` en el traje.

Partícula nueva en `assets/defs/particles.jsonc`: `bocanada` (el gas que sale de una tobera:
blanco, tenue, se hincha y desaparece; nada lo frena en el vacío). La tabla de estilos de
partícula pasa de 16 a 24 (estaba llena).

## Lo que cuesta

Sin medir en el juego (no se han pasado pruebas de rendimiento, por orden): lo que sigue sale de
los datos y de las pruebas (`cargo test -p lunar-app plumes -- --nocapture` lo imprime).

- **Una nave que no empuja no cuesta nada.** `Plumes::frame` pregunta `Ship::busy()` (una
  comparación) y pasa a la siguiente: ni busca su estructura. Una flota posada no hace más que
  eso por nave. Una nave ocupada que no empuja (una puerta que se mueve) cuesta además mirar si
  su fuerza y su par son cero.
- **De una nave que empuja** se miran solo sus propulsores (Abejorro 16, Alcotán 18, Cachalote
  20), y de ellos solo se calcula el que da empuje o se está apagando: posición de la boca, una
  raíz, una potencia. Sin reservar memoria tras la primera vez que la nave dispara (su memoria
  son 16 bytes por propulsor; se olvida a los 1800 fotogramas de no disparar).
- **Dibujo:** 48 bytes y 4 triángulos por chorro, **una** llamada de dibujo para todos y ninguna
  si no hay chorros. Como mucho, todos los propulsores de una nave a la vez (18 conos el
  Alcotán); el búfer es fijo (2048 chorros, 98 kB). Nada se vuelve a mallar.
- **Hasta dónde** (pantalla de 1080 px de alto y 60° de campo; con otra, en proporción): un
  chorro se dibuja mientras su cono mida 2 px o su boca 0,08 px, y las naves más lejos de eso ni
  se recorren:

  | Chorro | Largo a tope | Se dibuja hasta | Partículas: todas hasta / ninguna desde |
  |---|---|---|---|
  | motor del Alcotán (48 kN) | 7,6 m | 4,4 km | 100 m / 510 m |
  | motor del Cachalote (65 kN) | 8,9 m | 4,4 km | 120 m / 590 m |
  | motor del Abejorro (4 kN) | 2,2 m | 1,7 km | 29 m / 150 m |
  | RCS del Alcotán (2,2 kN) | 2,1 m | 970 m | 28 m / 140 m |
  | RCS del Cachalote (18 kN) | 5,9 m | 2,8 km | 80 m / 400 m |
  | gas frío del Abejorro (500 N) | 1,1 m | 1,0 km | 14 m / 71 m |
  | tobera de la mochila (315 N) | 0,73 m | 340 m | 10 m / 49 m |

- **Partículas** (vistas de cerca, a pleno empuje):

  | Quién | En marcha | Al encender / al apagar |
  |---|---|---|
  | Alcotán: 2 motores | 0 por segundo | 20 / 10 |
  | Alcotán: 16 toberas RCS | 0 por segundo | 0 / 3 por tobera, como mucho cada 0,3 s |
  | Cachalote: 4 motores, 16 toberas | 0 por segundo | 40 / 20; 3 por tobera |
  | Abejorro: 4 motores | 0 por segundo | 40 / 20 |
  | Abejorro: 12 toberas de gas frío | 30 por segundo cada una que dispare (2-4 a la vez: 60-120) | 0 / 2 por tobera |
  | Mochila | 40 por segundo por tobera a tope: 80 subiendo, unas 32 sosteniéndose | 0 |

  Y por encima de todo, un tope común: **el 2,5 % del presupuesto de partículas por segundo**
  entre todos los chorros (50/s en la calidad más baja, 400/s en la normal, 750/s en la más
  alta), con medio segundo de eso en reserva para ráfagas; 3 por propulsor y fotograma y 48 de
  ráfagas por fotograma como mucho; y ninguna si la lista de partículas está al 90 % (lo que
  queda es de las explosiones). Si piden más, todos bajan en la misma proporción.
- **Luces:** cuatro como mucho, las más cercanas y brillantes, solo de chorros a menos de 300 m.
  Entran como los destellos de las explosiones, antes que las lámparas de las naves.
- **Pendiente de medir** cuando se puedan pasar pruebas: el relleno de píxeles con la cámara
  metida en un chorro grande (el cono es un cuadrado que solo suma; se atenúa a menos de 0,5 m
  del ojo) y una batalla con cientos de naves empujando a la vez.

## Verlo

`luna --guion tools/camara/chorros.jsonc` → `out/camara/chorros_*.png`: la mochila desde fuera
(subir, sostenerse, de lado, frenar, adelante, bajar) y desde los propios ojos; el Abejorro
encendiendo, al ralentí, despegando y deslizándose con el gas frío; el Alcotán encendiendo, al
ralentí en el suelo, sostenido en el aire (de lado, de cerca, desde abajo, desde popa, de lejos),
cabeceando y trasladándose con el RCS, y apagando. Los motores se arrancan por sus mandos, como
un piloto; las naves vuelan con MANTENER y las palancas sujetas con el paso nuevo `eje` (sujeta
un eje de una palanca con muelle, como la tecla de un asiento).

## De dónde sale

Solo enlaces abiertos y leídos para esto.

| Qué | De dónde | Qué se tomó | Dónde |
|---|---|---|---|
| Un motor en el vacío casi no se ve | [Clavius, *Rocket engines*](https://www.clavius.org/techengine.html) | Los gases «se dispersan deprisa en el vacío, perdiendo temperatura y presión»; el penacho de Aerozine 50 con N₂O₄ en régimen es «casi invisible», incoloro y transparente; del encendido al régimen, menos de un segundo. | El cono es tenue y solo suma luz (`pluma` baja); `arranque` dura menos de un segundo |
| El despegue del módulo lunar no deja estela | [*Moon Hoax: Debunked!*, 6.12](https://moonhoaxdebunked.blogspot.com/2017/07/612-why-is-there-no-exhaust-from-lms.html) | En la retransmisión no se ve penacho; la segunda etapa de un Falcon 9 a 393 km muestra la tobera al rojo y ningún penacho. | Lo más brillante es la boca de la tobera (`nucleo`), no el cono |
| Cómo se abre el gas en el vacío | [NASA/TM-2007-215049, *Plume heating of the main engine on the CEV service module*](https://ntrs.nasa.gov/api/citations/20080002105/downloads/20080002105.pdf) | A la salida: 916 K, Mach 5, 0,0046 bar; es agua, CO₂, N₂ e H₂ (emiten en el infrarrojo, poco en el visible). En el vacío se abre hasta el límite de Prandtl-Meyer (103° con los 20° de la tobera), «pero la mayor parte de la masa sigue en el núcleo». | `apertura` ancha y `caida` (se enrarece al abrirse) con un `nucleo` estrecho; los números de apertura y caída son míos, a ojo |
| Toda tobera en el vacío va subexpandida | [Wikipedia, *Rocket engine nozzle*](https://en.wikipedia.org/wiki/Rocket_engine_nozzle) | «En el vacío prácticamente todas las toberas están subexpandidas»: para expandir del todo haría falta una tobera infinita. | El cono sale más ancho que la boca desde el primer palmo |
| Un motor que se estrangula | [Wikipedia, *Descent propulsion system*](https://en.wikipedia.org/wiki/Descent_propulsion_system) | El motor de descenso del módulo lunar: de 4,7 a 45 kN, presión de cámara de 76 a 760 kPa, 311 s. | El chorro va con el empuje: largo con su raíz (como el alcance del polvo en `dust.rs`), luz con el nivel |
| Qué se ve al disparar un RCS | [*Thrusters, light flashes and ice particles* (RCS del transbordador)](http://www.thelivingmoon.com/41pegasus/02documents/RCS.htm) | Un ingeniero de la NASA: el propelente casi no da luz al quemarse; hay una nubecilla de propelente sin quemar antes de disparar y una mayor al cerrar («nieve microscópica» que se hiela en el vacío), visible solo si le da el sol; pulsos de 80 ms; gas a 3500 m/s. | `apagado` del estilo `rcs` (partículas `bocanada`, iluminadas por el sol y no por sí mismas); `desvanecer` corto; una ráfaga cada 0,3 s como mucho (`PUFF_GAP`) |
| Gas frío | [Wikipedia, *Cold gas thruster*](https://en.wikipedia.org/wiki/Cold_gas_thruster) | Sin combustión: gas a presión por una tobera; nitrógeno, 73 s de impulso; la MMU, 24 toberas de 6,2 N. | Las toberas del Abejorro (72 s) llevan `gas_frio`: sin luz propia (`sol`), partículas de escarcha |
| La mochila | [Wikipedia, *Manned Maneuvering Unit*](https://en.wikipedia.org/wiki/Manned_Maneuvering_Unit); [*Astronaut propulsion unit*](https://en.wikipedia.org/wiki/Astronaut_propulsion_unit) | 24 toberas repartidas por la unidad, nitrógeno; una mano manda traslación y la otra giro; mantiene la actitud sola. SAFER: 1,4 kg de nitrógeno, unos 3 m/s. | Toberas por direcciones en la mochila (`tobera_*`), cada empuje respondido por las que miran en contra; sostenerse también echa gas |
| Penachos con malla y sombreador, no partículas | [Waterfall (KSP), README](https://github.com/post-kerbin-mining-corporation/Waterfall) y [*Concepts*](https://github.com/KSPModStewards/Waterfall/wiki/Concepts) | Efectos de motor «dirigidos por malla» y sombreador, movidos por *controladores* (acelerador, presión, azar) a través de *modificadores*; *plantillas* que comparten muchas piezas. | Una primitiva en vez de partículas para el chorro continuo; el nivel de empuje es el controlador; los estilos son las plantillas |
| Cuadrado que gira sobre un eje | [Lighthouse3D, *Billboarding tutorial*](https://www.lighthouse3d.com/opengl/billboarding/index.php) | El *billboard* cilíndrico gira solo alrededor de un eje; la ilusión se rompe al mirarlo a lo largo de ese eje. | El cono es eso sobre el eje del chorro; al mirarlo de punta se desvanece y queda el disco de la boca |

No se pudieron abrir (y por eso no se citan como fuente): el hilo de Waterfall en el foro de
KSP, la entrada de la ley del coseno de Simons en encyclopedia.pub y un hilo de gamedev.net
sobre *billboards* alineados a un eje.

## Pruebas

- `crates/core/src/plumes.rs` (7): los datos cargan y cada estilo está entero; nada dispara,
  nada se hace; el cono va con la raíz del empuje y la luz con el nivel; la corriente va con el
  empuje y nunca pasa del presupuesto; lo lejano no se dibuja ni echa gas; un chorro se apaga
  poco a poco y echa una sola bocanada al encender; el gas frío es el sol sobre él y las luces
  salen por cercanía.
- `crates/ship/tests/chorros.rs` (3) y `crates/ship/src/exhaust.rs` (1): cada propulsor de cada
  nave tiene tobera; la boca de una góndola es el borde de su campana y bascula con ella; lo que
  dispara es lo que empuja (la suma del gas es la fuerza sobre el casco) y nada dispara en una
  nave quieta.
- `crates/app/src/plumes.rs` (3): cada propulsor de cada nave tiene estilo (y lo que cuesta); una
  nave en reposo no cuesta nada y una que dispara enseña sus chorros donde están sus toberas; la
  mochila responde a cada empuje con los chorros contrarios.
