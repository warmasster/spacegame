# Naves (prototipo en Rust)

Una nave es **una estructura con sistemas**, toda de datos. No hay código por nave ni por pieza: el
Alcotán (`assets/defs/ships/alcotan.jsonc`) se arma con los mismos generadores, catálogos y
registros que armará cualquier otra.

## Crates

| Crate | Qué es |
|---|---|
| `lunar-signals` | Señales con unidades SI (`Store`), expresiones compiladas a bytecode (`expr`), derivadas. |
| `lunar-controls` | Mandos físicos (pulsador, interruptor, selector, rueda, palanca, tapa, llave, disyuntor, teclado), indicadores, maquetación de paneles. Intención → estado → valor. |
| `lunar-machines` | Redes genéricas para todos los medios (eléctrica, hidráulica, propelente, refrigerante, calor, aire, O₂, N₂, datos), modelos de máquina (registro por nombre), actuadores (accionamiento + varillaje + bloqueos + sensores + desgaste), procedencia (fabricante, serie, lote, huella). |
| `lunar-ship` | La nave: definición, generadores (casco por secciones, cubiertas, mamparos, conductos con su tramo a cada aparato), componentes, `ShipKind` (se arma una vez), `Ship` (en marcha, tick fijo), paneles, aire por compartimentos, ordenador de vuelo, caja negra, escena (mandos, serigrafía, pantallas, lámparas, vástagos). |
| `lunar-app` | `ships.rs` (mundo real para cada nave), `aboard.rs` (manos: mira, clic, rueda, arrastre, asientos), `spawner.rs` (catálogo G), `inspector.rs` (F4). |

## Datos

- `assets/defs/components/*.jsonc` — componentes (piezas con forma, material, «hueco», máquina, luz).
- `assets/defs/panels/*.jsonc` — paneles (rejilla, grupos, mandos, indicadores, enclavamientos).
- `assets/defs/ships/*.jsonc` — la nave: casco, cubiertas, mamparos, componentes, articulaciones,
  actuadores, redes, compartimentos, paneles montados, asientos con teclas, derivadas, vuelo.
- `assets/defs/scenario.jsonc` → `"ships"`: naves puestas al empezar (cualquier cuerpo).

Añadir una nave = un `.jsonc` en `ships/` (y los componentes o paneles nuevos que necesite). Sale
sola en el catálogo G.

## En el mundo (cualquier cuerpo, órbita o espacio)

**Disparar a bordo o después de salir (V39):** las armas de mano y los cañones emiten desde su
boca con la velocidad de ese punto, incluida la parte tangencial del giro. El proyectil no es
una foto de un fotograma adelantado: el mundo lo mueve en sus lonchas, y su contacto se barre
contra las dos poses de la nave. El daño conserva estructura y punto local. Las cajas en las
manos frenan respecto a la mano también fuera de la nave; soltarlas no cambia su velocidad.
Detalles, pruebas y límites: [`MOVIMIENTO.md`](MOVIMIENTO.md) §12.

Cada fotograma `Ships::update` le da a cada nave el `World` de donde está **ahora**
(`lunar_ship::World::at`, que pregunta a `BodyRegistry::field`): el tirón del sitio en el marco
de la nave (cero fuera de la influencia de todo cuerpo), altura de la quilla sobre el suelo de
debajo (`NO_GROUND` donde no lo hay), velocidad, ascenso y rumbo del morro (`nave.rumbo`, grados
desde el norte del cuerpo; −1 donde no hay norte); y sol, sombra de cualquier cuerpo y presión
exterior (0 hasta que haya cuerpos con atmósfera). Nada supone la Luna ni recuerda dónde se hizo
la nave: [`MOVIMIENTO.md`](MOVIMIENTO.md) §6-7.

- **Gravedad de a bordo** (`"gravedad": { "g", "senal", "tiempo" }`, obligatorio en cada nave):
  lo que pesa quien está en sus salas, hacia su cubierta, esté la nave como esté y donde esté.
  `g` 0: no da ninguna. `senal`: la señal que dice si funciona (`abordo.gravedad` en el Alcotán
  y el Cachalote: mientras haya barra esencial). Entra y sale en `tiempo` s. La nave la deja en
  su estructura (`Structure::gravity`, `Structure::rooms`) y de ahí la lee quien anda por ella.

- La física de la estructura es la de cualquier estructura suelta (`structure/physics.rs`); el
  empuje de motores y RCS entra como `force/torque`. Posada, se apoya en las esquinas bajas
  repartidas por la huella (tren) y se duerme; `rest_on_ground` la deja sobre la pendiente al
  ponerla.
- Lo que revienta dentro (depósitos, reactor) sale como `Burst` → explosión de estructuras + efecto.
- Escena: props instanciados + glifos SDF (`assets/fonts/serigrafia`) + hasta 24 lámparas (las de
  dentro no iluminan el terreno).

## Compartimentos: lo que cada uno trae solo (`ship/src/rooms.rs`)

Antes de armar la nave, la definición se completa (vale para cualquier nave o estación). Cada
compartimento, salvo `"automatico": false`, recibe:

- **Venteo** al vacío (`C.venteo`, área por volumen: se vacía en unos 40 s) y una alarma de
  descompresión.
- Por cada cierre: su **ΔP** (`D.dp`), una **válvula de igualar** con el de al lado
  (`D.igualar`, un interruptor por lado) y el **enclavamiento** que no la deja abrir con más de
  5 kPa (hacia el vacío: con el compartimento presurizado). Si la nave declara una **válvula de
  mano** entre esos dos sitios (`trasvase.valvulas`, ver [`AIRE.md`](AIRE.md)), el interruptor no
  se crea: el panel enseña lo abierta que está (`valvula_D`) y se iguala con su volante.
- **REPRES.** desde las botellas si los datos dicen de qué redes (`"represurizar": { "o2", "n2" }`)
  y nada de la nave lo hace ya: un inyector por gas, con su tramo de tubo.
- **Luces** mandadas desde dentro: APAG. / AUTO (lo que diga el techo del puente) / ENC.
- **Integridad** del casco que lo encierra (`C.integridad`: la chapa más dañada).
- Y, donde los datos lo coloquen (`"panel": { "en", "normal", "energia" }`), **su panel**: atmósfera,
  presión, cada puerta (abrir, ΔP, igualar), luces, casco y tensión. Cada mando con su descripción.

**Prioridades de los circuitos** (`"prioridades": { "en", "normal" }` en la nave): un selector
BAJA / NORMAL / ALTA por cada disyuntor `brk.X` (señal `prio.X`); con poca energía todo lo que
cuelga de ese circuito (hasta el siguiente interruptor) pide con esa prioridad.

Una señal tiene un solo escritor: cuando varios paneles mandan lo mismo, cada uno escribe su lado y
la señal común es el «o» de todos (`bodega.venteo = bodega.venteo_soporte || ... || _sala_bodega`).
La excepción son los mandos que son **un solo mecanismo** visto desde dos sitios (el volante de una
válvula a cada lado de su mamparo): `"bind": { "senal", "comun": true }`; mover uno mueve el otro y
escribe uno solo.

## Aire: válvulas de mano, compresor y depósito (`ship/src/airworks.rs`, `transfer.rs`)

Todo en [`AIRE.md`](AIRE.md). La nave declara `"trasvase": { "valvulas", "compresores" }` y recibe:
una válvula con **volante** (mando `volante`: se gira con la rueda del ratón) y manómetro de ΔP a
cada lado de cada mamparo, abertura analógica por la misma ecuación de orificio; y el panel del
**compresor** (`compresor`: saca aire de un compartimento hasta 5 kPa, cada vez más despacio, y lo
mete en un `deposito_aire` o en otro compartimento; del depósito vuelve solo, sin motor). Cerradas
y parado no cuestan nada.

## Pantallas multifunción (`controls/src/mfd.rs`, `ship/src/mfd_auto.rs`)

Un indicador `mfd`: páginas de instrumentos (valor, barra, depósito, reloj con zonas, gráfica de
historia, plano de zonas, texto) en una rejilla de 4×3, cada uno en el color de su zona (verde,
ámbar, rojo). Su **bisel** (`bisel`, un mando de varios botones) se crea solo: 5 botones por lado,
la leyenda de cada página escrita en la pantalla junto a su botón y la mostrada resaltada. Las
páginas con `"auto"` las hace la nave con lo que tiene: `energia`, `depositos`, `aire`, `casco`,
`motores`, `puertas`. Pantallas a 10 Hz (gráficas a 2 Hz, de todas las páginas) y con luz de
pantalla: nunca entran en el resplandor.

## Fichas al mirar (`ship/src/panels.rs`: `card_control`, `card_indicator`)

Mirando un mando (2,4 m) o un instrumento (3,5 m): qué es, cómo está (en el color de su zona),
para qué sirve (su `ayuda`, o la que sale de su tipo, posiciones y señal), su tapa, sus
enclavamientos y si su panel no tiene energía.

## Canalizaciones (`ship/src/conduits.rs`)

Sencillas a propósito: baratas de dibujar, de golpear y de entender. Sustituyen a los cables
pieza a pieza (2 069 piezas en el Alcotán; ahora 77).

- **Troncos:** un tubo por pared de cada sala (uno a lo largo de cada costado, uno sobre la puerta
  de cada mamparo), unidos en las esquinas y a través de los mamparos. Todo lo eléctrico y de datos
  va por ellos. Son piezas (tramos de hasta 2,2 m): un impacto corta lo que pasa por ese tramo.
  La altura la busca sola (la más alta de `HEIGHTS` que no tape aparatos ni pase por delante de un
  cristal; una pared sin sitio se queda sin tronco); `"canalizaciones": { "altura": 0.7 }` en la
  nave la fija.
- **Tomas:** cajas en el tronco, cada medio metro como mucho y solo donde se enchufa algo. Son
  piezas: sin toma, lo enchufado se apaga. Muy dañadas y con corriente, echan chispas.
- **Bajantes:** el cable de la toma a su aparato. Solo se ve: va pegado a la pared (por el contorno
  de la sala), nunca por delante de un cristal, y forma parte del aspecto de su toma. Un aparato a
  más de 1,3 m de una pared, o fuera de las salas, no tiene bajante (está conectado igual).
- **Tuberías** entre máquinas (hidráulica, propelente, refrigerante, gas): fijas, solo se ven, por
  debajo del suelo y subiendo a cada boca. **Conductos de aire:** tramos rectos entre sus nudos.
- **Dos caminos:** cada sala es un anillo de troncos, y cada tramo se tiende por el camino corto y
  por el otro lado. Los nudos de las redes (barras, circuitos) se ponen donde se juntan dos tramos
  de tronco. Resultado: ningún corte único deja la nave sin lo esencial.
- **`"esenciales"`** en la nave: lo que debe aguantar cualquier corte único (`"qué": "condición"`).
  `diag::single_failures` corta cada canalización por turno y dice qué se pierde;
  `tests/fallo_unico.rs` lo exige (Alcotán: 77 cortes, 11 esenciales, 0 pérdidas).
- **Puntos de ruta a mano:** `"por"` en un tramo de red. **Sin cableado:** `"cableado": false` en
  lo colocado.
- Piezas `no_collide` (troncos, tomas, conductos: reciben daño, no chocan) y `ghost` (bajantes y
  tuberías: solo se ven) en el catálogo de estructuras.

## Daño, reparación y escáner

- **Sin grietas:** una pieza dañada se va poniendo negra (hollín a manchas hasta carbonizarse) y,
  si es una toma con corriente, chispea. Destruida, desaparece en su efecto. El daño es una palabra
  por pieza en la GPU: nada se rehace (`docs/OPTIMIZACION.md`).
- **Reparar:** `Structure::mend` (suelda: sube los puntos de la pieza y sus uniones) y
  `Structure::rebuild` (repone una pieza que falta, en su sitio y unida a lo que tocaba). La nave lee
  sus piezas cada tic: una placa repuesta vuelve a sellar su sala, una lámpara vuelve a lucir, una
  toma vuelve a dar corriente (`tests/reparar.rs`).
- **Qué es cada pieza:** `ShipKind::describe` («Unidad hidráulica», «Panel de casco», «Tronco de
  cables», «Bidón de agua · 170 L de agua»).
- **Carga:** componentes de `components/carga.jsonc` con `"contenido"` (qué sustancia del registro
  `sustancias.jsonc` llevan, cuánta cabe y cuánta traen): de ahí salen su masa (vacío + lo que
  queda), sus rótulos (`{sustancia}`, `{capacidad}`), lo que lee el escáner y lo que hacen al
  reventar. Ver [`CARGA.md`](CARGA.md). El Alcotán lleva en la bodega un palé de regolito, cuatro
  cajas de repuestos, dos bidones de agua y uno de propelente.

## Modelos (`tools/modelos`, `core/structure/models.rs`)

Una pieza sigue siendo su forma convexa para todo lo físico (lo que choca, lo que pesa, cómo se
rompe). Un **modelo** es solo lo que se ve de cerca, dibujado en lugar de esa forma.

- **Un fichero por cosa** en `assets/models/<nombre>.glb`; cada malla dentro es una pieza, con su
  nombre (`cuerpo` la que no lo tiene), en el marco de esa pieza (x a babor, y arriba, z a proa).
  Dejar caer un fichero es todo: `Catalog::load` los lee y quien monta piezas los pide por nombre.
- **Qué nombre:** un tipo de componente, el suyo (`bidon_agua/cuerpo`, `asiento/cojin`); una pieza
  de estructura, `parte_<id>/cuerpo`; una herramienta, el de `"modelo"` en `gear.jsonc`; una pieza
  suelta de una nave con `"estilo": "ala"`, `estilo_ala/<forma>` donde la forma es su tamaño al
  milímetro (`b2500x160x2400`, `c85x1000`, `w100x1200x1600`: `components::shape_key`); una
  envolvente de puntos (`hull`), `h` y ocho cifras hexadecimales (FNV-1a de sus puntos al
  milímetro, en orden; `kit.clave_forma` hace lo mismo en Python).
- **Materiales:** el que empieza por `tinte` es la superficie propia de la pieza según sus datos
  (color, acabado), por la sombra horneada: un modelo sirve para el bidón azul y para el rojo. Los
  demás son colores del modelo; `#acabado` al final del nombre elige el acabado.
- **Recetas** (`tools/modelos/recetas/*.py`): una función con `@receta('<nombre>')` que pone
  geometría sobre las piezas tal como las tienen los datos (`m['cojin'].tam`), en el marco del
  juego. El taller (`kit.py`) da cajas biseladas, tornos, tubos, extrusiones, pieles, tornillos,
  rejillas, bridas, taladros (booleanos), y hornea la oclusión ambiental en los vértices.
  **Estilos** (`@estilo('ala')`, `recetas/estilos.py`): una función para cualquier tamaño; el
  taller construye una pieza por cada tamaño que piden las naves. Un estilo puede vestir también
  una envolvente de puntos: `ala_flecha` hace un ala en flecha y con estrechamiento (o un canard,
  un estabilizador) de cualquier contorno convexo visto desde arriba, tan gruesa en la raíz y en
  la punta como digan sus puntos.
- **Construir:** `blender -b -P tools/modelos/hacer.py -- [nombres] [--sin-vista] [--sin-sombra]
  [--lista]`. Sin nombres, todo (93 modelos en unos 20 s). Deja una vista de cada uno en
  `out/modelos/` y `python tools/modelos/hoja.py` las junta en hojas. `--lista` dice qué falta.
  Hay que **volver a construir** cuando cambia el tamaño de una pieza con estilo o se añade un
  tipo: `tests/modelos.rs` falla si falta alguno o si uno no cabe en su forma.
- **Copia reflejada:** la copia `"espejo"` de una pieza con estilo lleva el modelo reflejado
  (`PartModel::mirrored`): la punta del ala queda por fuera en las dos. (`espejo` gira la pieza,
  no refleja su forma de choque: una envolvente que no es simétrica en su propio x — un ala en
  flecha — se escribe a cada lado con sus puntos, como `ala_izq` y `ala_der` del Azor.)
- **Construir sin Blender instalado:** con Python 3.11, `pip install "bpy==4.5.*"` y
  `python -c "import bpy, sys, runpy; sys.argv=['hacer.py','--', ...]; runpy.run_path('tools/modelos/hacer.py', run_name='__main__')"`
  (hay que importar `bpy` antes de que el taller importe `bmesh`).
- **Casco, puertas:** no llevan modelo de Blender; su aspecto fino lo hace el generador
  (`geom::plate_look`: junta cortada alrededor de cada chapa, aristas `suaves` redondeadas;
  `geom::window_look`: marco y cristal rehundido; `closures::leaf_look`: marco, panel y nervios).
- **Cristal:** de una pieza de cristal, lo que no es liso como el cristal (rugosidad ≥ 64: su
  marco, su junta, su canto) se dibuja sólido (`look::GLASS_ROUGH`).
- **Herramientas en la mano:** el pase de *props* dibuja mallas además de sus cuatro primitivas
  (`Renderer::prop_mesh`, `Prop::mesh`); la malla lleva sus colores.

### Rótulos (`core/structure/labels.rs`)

Lo que hay escrito es de la **pieza** (`PartKindDef::labels`, en su marco): va donde vaya ella —un
bidón conserva su «AGUA» fuera de la bodega— y lo puede llevar cualquier estructura.

- En los datos, `"rotulos"` en un tipo de componente (marco del componente) o en una nave (marco
  de la nave): `texto`, `en`, `normal`, `arriba`, `alto` (m), `color`, `alinear`, `relieve`
  (pintado, en relieve o grabado), `brillo`, `radio` (da la vuelta a un cilindro de ese radio:
  bidones, botellas, góndolas) y `pieza`.
- En el texto: `{id}`, `{nombre}`, `{matricula}`, `{nave}`, `{serie}` (un número propio de cada
  uno, siempre el mismo), `{etiqueta}` (lo que dice `"etiqueta"` donde se monta el componente:
  el gas de una botella; sin ella, ese rótulo no sale) y, en lo que lleva algo (`"contenido"`),
  `{sustancia}` («AGUA») y `{capacidad}` («170 L»): lo que pone un bidón sale de sus datos.
- Se dibujan con la letra de serigrafía (campo de distancias) solo los que se pueden leer: de
  frente y a menos de 350 alturas de letra.

## Niveles de detalle y visibilidad (para cualquier estructura)

- **De cerca, modelos:** dentro de 3,8 radios (40 m como poco) se dibuja el aspecto fino; de ahí a
  200 m, el básico (lo que había antes de los modelos, con su cristal).

- `look::DetailLods`: 6 pasos por distancia (radios de la estructura; el Alcotán, de 10,6 m): tal
  cual hasta 200 m; sin cables ni cosas pequeñas de dentro (600 m es el siguiente); solo el
  exterior; simplificado 40 %; 15 %; y su **silueta lejana** desde 4,8 km. Se funden por tramado.
- **Silueta lejana:** la nave la registra (`Catalog::far`: casco por pocas estaciones con su punta y
  cajas para alas y góndolas, `ship/src/far.rs`); cualquier otra estructura la saca sola de su
  malla (`auto_far`: vóxeles, huecos rellenos, pocas cajas). El renderizador la pinta con los
  colores reales que tiene debajo (`paint_far`).
- **Lejos, fuera de memoria:** pasada 1,6 veces la distancia del primer paso, la malla completa
  sale de la GPU (se queda con los niveles gruesos) y vuelve al acercarse.
- **Oclusión por rayos** (`app/src/visibility.rs`): una estructura detrás del relieve, o fuera del
  casco cuando estás dentro (salvo por ventanas y puertas abiertas), no se dibuja (solo su sombra),
  ni sus mandos, ni sus luces de dentro. Unas cuantas por fotograma, en 0,25 ms.
- Lejos de una nave, sus paneles y luces interiores no se generan.

## Editor de naves y MCP (`crates/editor`)

Una nave se edita como **documento** (`lunar_editor::Doc`: su `.jsonc`) con **operaciones**
(`Op`): poner, mover, desplazar, girar, quitar, duplicar, cableado (sí/no), fijar (cualquier valor
por su ruta), nodo, tramo, punto de ruta (cable a mano), conectar (puerto de una máquina),
panel de sala, deshacer, rehacer. Cada operación deja una definición válida o no se hace.
`Doc::check` la arma y pasa los diagnósticos (`lunar_ship::diag`: estanqueidad por vóxeles, uso
desde cada compartimento, cables); `Doc::save` la guarda (la copia vieja, con comentarios, en
`out/copias`). Las mismas operaciones las usarán la construcción y la reparación en juego.

- **F6 (debug): editor en el juego.** La nave en la que estás (o la más cercana). Clic en una pieza:
  su componente (recuadro amarillo); en un cable o tubo, su tramo de red. Ventana: mover por pasos,
  girar, cableado, duplicar, quitar, poner un componente del catálogo donde miras, punto de ruta
  donde miras, deshacer, rehacer, comprobar, **aplicar** (la nave rehecha en su sitio, en caliente)
  y guardar. Botón derecho mantenido: mirar. Guion `tools/camara/editor.jsonc` (pasos `editar` y
  `aplicar_edicion`).
- **MCP (`LunaMCP.exe`, registrado en `MIGRACION/.mcp.json` como `luna-naves`):** herramientas
  `naves`, `catalogo`, `leer_nave`, `operar`, `validar`, `foto` (el juego con la nave tal como
  está, cámara donde se pida, imagen reducida de vuelta) y `guardar`. Así un modelo diseña,
  comprueba y mira una nave.
- El juego carga una nave editada sin tocar su fichero con `LUNA_NAVE_EDITADA=id=fichero.jsonc`.

## Tren de aterrizaje con amortiguadores (`core/structure/state.rs`, `physics.rs`)

Una pata es una articulación `corredera` con `"muelle": {}`: nada la mueve, la aprieta lo que
tenga bajo el pie. `Spring` es un muelle de gas (`F = precarga·(1 − x/L)^(−n)`, blando al
principio y duro al final) con freno de orificio (más al comprimir rápido, distinto al entrar y al
salir); el contacto de cada pata entra en el resolvedor como restricción blanda (estable a
cualquier paso). La carga de diseño de cada pata sale de la estática de la nave donde se posa
(`Ship::leg_shares`), así que bajo su peso todas descansan a tres cuartos del recorrido;
`rest_on_legs` la deja ya asentada al ponerla. Datos por pata (`SpringDef`): `carga`, `precarga`,
`fin`, `freno`, `agarre`; señales `<pata>.carga` y la posición de siempre. `tests/tren.rs`.

## Mecanismos que se topan con algo (`core/structure/obstruct.rs`, `Ship::stop_at_obstacles`)

Lo que mueve una articulación (rampa, puerta, izado, puente, tren al salir) se prueba donde va a
quedar contra el suelo, las otras estructuras, la carga anclada de la propia nave y quien esté
ahí (el cuerpo del jugador). Si entra en algo, la articulación vuelve por bisección a donde toca
y **no sigue** en ese sentido hasta que la manden al otro lado o se quite el estorbo; lo dice con
`<articulación>.bloqueada` y con un aviso a quien esté en la nave (`Ship::said`). Lo que lleva
agarrado (la carga del imán) cuenta como parte de lo que se mueve: no atraviesa la cubierta ni
otra carga. Una articulación con `"cede": true` (o una hoja abisagrada por abajo: una rampa) no
sostiene el casco y el suelo la levanta al asentarse la nave; cerrada es casco como lo demás.
Para no costar: no se mira nada si el casco no se ha movido 2 cm ni ha girado medio grado y
ninguna articulación parable se ha movido; el suelo se mira una vez por comprobación (un plano);
cayendo o dando tumbos no se mira. `tests/mecanismos.rs`.

`"sigue": { "de": "<articulación>", "razon": 0.33 }`: una articulación que va donde otra, por una
razón (los tramos intermedios de un mástil telescopico, la segunda hoja de una puerta plegable).

## Las naves que hay

Una nave es un fichero de `assets/defs/ships`; `ship/tests/naves.rs` las pasa todas por lo mismo
sin una línea de test propia: cabe una persona y se trabaja desde donde se debe, es estanca, cabe
en su presupuesto (piezas y vértices por metro, tic de sistemas) y **posada se duerme** (en llano
y en una docena de sitios del terreno, pendientes de hasta 17°).

| Nave | Qué es | Tamaño | Lo suyo |
|---|---|---|---|
| **Alcotán** | Lanzadera VTOL | 20 m, 20 t | La de referencia: todos los sistemas. |
| **Abejorro** | Remolcador de carga de un asiento | 7 m, 2,4 t | Sin casco ni cabina (se vuela con el traje). Cuatro motores de elevación, gas frío, **giróscopos de control** (giran sin gastar; se saturan y se descargan) y un **imán electropermanente** bajo la panza (sujetar no gasta; sin corriente no suelta). |
| **Azor** | Caza monoplaza | 11 m, 12 t | Todo lo de combate ([`COMBATE.md`](COMBATE.md)): radar con escala de alcance, alertador, infrarrojo, transpondedor, perturbador, dos cañones, diez misiles, señuelos, piloto automático de navegación y de combate. **Un solo grupo de energía** (reactor con su refrigeración), un ordenador, dos baterías en dos buses. El **asiento baja por la panza** en una plataforma; a popa, **dos compuertas** (rampa de tres tramos y visera) a la bahía de utilidades; **tambor de tres paneles**. Se vuela con el traje. |
| **Cachalote** | Carguero pesado | 45 m, 60 t | Generado desde el Alcotán (`tools/naves/cachalote.py`: su sección de tripulación, una bodega de 22 m). **Puente grúa** de tres ejes con imán y balizas de maniobra, nueve anclajes, cuatro góndolas, giróscopos y una **cuna en el lomo** con un Abejorro atracado (`"lleva"`). |

## Jugar

- **A pie (V33):** sube sin saltar lo que no pase de `escalon` (0,45 m): si algo estorba al pie y
  encima hay sitio, se pone arriba (los ojos suben después, suave); si el suelo se va bajo los pies
  (escalón abajo, rampa, pendiente) las piernas lo buscan y no se queda flotando. Al tocar suelo
  con velocidad frenan las botas (`agarre`) y las piernas (`frenada`). La mochila con el
  estabilizador mantiene también la altura.
- **Arriba, peso y suelo:** el arriba es hacia donde pesas. En las salas de una nave con
  gravedad propia, su cubierta (aunque vaya boca abajo o dando tumbos); en un cuerpo, su
  vertical; sin peso se flota y no gira nada. Al pasar de uno a otro el cuerpo se endereza a su
  ritmo. [`MOVIMIENTO.md`](MOVIMIENTO.md) §8.
- **A pie:** el jugador es tres esferas que empujan las piezas (`Structures::sphere_contacts`). Lo
  que pisas te lleva: a bordo conservas tu sitio en el marco de la nave aunque vuele o gire, y la
  cámara se pone en ese marco después de mover la nave (sin retraso a ninguna velocidad). El
  jugador no tiene reloj propio: el mundo le da el paso con las estructuras, loncha a loncha
  (`Among`), así que él y cualquier nave son siempre del mismo instante. Cómo y por qué, en
  [`MOVIMIENTO.md`](MOVIMIENTO.md). Los números del cuerpo (esferas, agachado, pendiente, piernas,
  rodillas, cabeza) son datos: `scenario.jsonc`, `player.cuerpo`.
- **Mira + clic:** acciona el mando apuntado (2,4 m). Mantener = tirar / armar. Rueda = girar
  (Mayús grueso, Ctrl fino). Palancas: mantén clic y arrastra.
- **F:** sentarse en el asiento apuntado. **Espacio:** levantarse. Sentado, las teclas del asiento
  (datos: `asientos[].mandos`) mueven sus mandos (palanca de vuelo, guiñada, traslación,
  acelerador). Cada uno dice o una **orden de vuelo** (`"orden": "cabecear_abajo"`, la misma en
  todas las naves; su tecla es la del jugador, `assets/defs/controles.jsonc` → `vuelo`) o, si es
  propio de esa nave, una tecla fija (`"tecla": "P"`). Las órdenes están en `app/src/input.rs`
  (`ORDERS`); una nueva es una fila allí y su tecla en `controles.jsonc`.
- **G:** catálogo (naves y, en la versión debug, todas las estructuras). Elige, apunta, clic para
  colocar; rueda gira, Mayús+rueda acerca/aleja en el espacio; clic derecho o G suelta. Sin suelo
  bajo la mira, la pone flotando delante (órbita, espacio).
- **F4 (solo debug):** inspector de la nave en la que estás o la más cercana: señales, islas de
  cada red, máquinas con procedencia, actuadores y articulaciones, caja negra.
- **C:** agacharse (más bajo y lento, el aire te arrastra menos; no se levanta bajo un techo).
  **L:** linterna del casco. **T:** telémetro (distancia y qué es lo de la mira, nave más cercana y
  su nivel de detalle). **F7 (debug):** naves teñidas por nivel de detalle. **U:** metralleta (Q y E alabean flotando con la mochila). **F9 (debug):** vuelo libre.
- **Equipo** (`assets/defs/gear.jsonc`, `app/src/gear.rs`), con las teclas de número:
  - **1 · Soldador-escáner.** Su pantallita dice qué es lo que miras y su integridad. Clic
    mantenido: suelda (o repone lo que falta, que tarda un momento). Botón derecho: **vista de
    integridad** (lo sano en gris apagado, lo dañado en rojo cada vez más vivo, lo que falta en
    morado donde debería estar). Nada de esto sale en el HUD.
  - **2 · Lanzacohetes.** Clic dispara (`shots.jsonc`: `cohete`); botón derecho apunta.
- **Mochila propulsora** (`scenario.jsonc`: `player.mochila`): **J** la enciende y la apaga
  (apagada no hay mochila: Espacio solo salta). Encendida, Espacio empuja hacia arriba; en el aire
  las teclas de andar empujan de lado y al soltarlas te frena; Ctrl empuja hacia abajo. Solo toma
  el mando cuando estás en el aire de verdad (un salto, un empuje, o más de 0,35 s sin suelo): un
  escalón o el borde de la rampa siguen siendo andar. Gasta gas (45 s a todo empuje) y se rellena
  a bordo de una nave.
- **Teclas** (`app/src/input.rs`): una tabla dice qué hace cada acción (`ACTIONS`) y otra las
  órdenes de vuelo (`ORDERS`); qué teclas las hacen son datos (`assets/defs/controles.jsonc`, con
  perfiles, y encima `ajustes/controles.jsonc`, lo que cambia el jugador desde el menú). El juego
  y el menú leen el mismo `Keymap`, así que no pueden discrepar. Una acción nueva es una fila y su
  tecla en el archivo; una tecla que no existe, una acción que no es ninguna o una tecla dada dos
  veces se dicen en el menú y en el registro.
- **HUD** (`app/src/hud.rs`): quien tiene algo que enseñar dice qué es (un indicador del traje, una
  ranura de herramienta, una ficha, un aviso), nunca dónde: cada clase de cosa tiene su sitio y el
  centro queda libre. Traje abajo a la izquierda (el gas de la mochila, con su tecla), herramientas
  abajo en el centro (con lo que hace la que llevas encima), datos de vuelo abajo a la derecha, lo
  apuntado en dos palabras bajo la mira y su ficha a la derecha, telémetro arriba, avisos arriba a
  la derecha (uno por asunto: el nuevo sustituye al anterior). Arriba a la izquierda, qué tira
  donde estás (el cuerpo, la nave si da gravedad, o «Espacio»); arriba en el centro, la
  **brújula**: una cinta con las referencias que significan algo ahí (`app/src/nav.rs`,
  `navegacion.jsonc`: norte en superficie, marcha y radial en órbita, la nave y el sol en
  espacio libre), que se funden al cambiar de régimen ([`MOVIMIENTO.md`](MOVIMIENTO.md) §9). Un tema para todo, escalado a la
  ventana, y el mismo para las ventanas (`hud::style`).
- **Menú (Esc)** con pestañas: CONTROLES (cada acción y cada orden de vuelo con su tecla y
  «Cambiar» / «+», el perfil, «Restablecer todo», las herramientas del traje, las teclas propias
  de los asientos de la nave más cercana tal como las dan sus datos —`asientos[].mandos[].ayuda`—
  y la sensibilidad del ratón), GRÁFICOS y, en debug, ESCENA Y PRUEBAS.
- **Asientos y sus paneles:** `asientos[].paneles` dice qué paneles se trabajan desde cada asiento.
  `diag::seat_reach` comprueba que cada mando e instrumento de esos paneles se ve desde los ojos
  del asiento (la cabeza gira hasta ahí, la placa no queda de canto, nada lo tapa, la mano lo
  encuentra), está al alcance de la mano y a un brazo (1,3 m) de alguno de los asientos que lo
  comparten. Sentado, una tecla mantiene su mando mientras está pulsada (`F_AXIS_X/Y` en la
  palanca): el muelle solo lo devuelve al soltar.
- **Paneles de pared:** un panel sin pieza propia trae su caja y se **asienta** en su pared al
  construir la nave (`kind::seat_on_wall`): su cara sale lo que la pared necesite (las paredes del
  casco se inclinan y giran) y su caja llega hasta la pared por detrás. Las bajantes y los tubos,
  que solo se ven, rodean los paneles (`conduits::Keep`). `diag::panels_cut` y `diag::panel_faces`
  lo comprueban; el editor (y el MCP) los pasan con el resto.
- **Carga y anclajes** (`ship/src/cargo.rs`): la carga se coloca como un componente con
  `"anclaje": "<id del anclaje>"`. No es de la nave: se une a sus propias piezas y a su anclaje
  (unión `amarre`, más débil que una soldadura) y a nada más, ni al suelo ni al bulto de al lado.
  Mientras el anclaje sujeta, va como parte de la nave (pesa, se daña). Suelta —clic en el anclaje,
  su orden `<id>.soltar`, el anclaje destruido, los amarres rotos de un golpe— sale entera como
  estructura propia con la física de cualquier cuerpo suelto (`Structures::separate`), y no falta en
  la nave (`PartKindDef::carried`: ni morado en la vista de integridad ni nada que reponer).
  `<id>.sujeta` dice si aún sujeta. Lo que estalla lo dice su pieza (`"estalla"`: energía, radio,
  efecto): al destruirse suelta esa explosión donde estaba (`Event::Burst`), y una fila de bidones
  se va encadenando.
- **Sujetar** (`core/structure/hold.rs`): una estructura sujeta por otra se mueve con ella (no es
  un cuerpo: va donde va su dueña, exacta, a cualquier velocidad) y le suma su masa; al soltarla
  hereda la velocidad del punto donde estaba. Es lo que usan los anclajes de carga cuando
  **vuelven a tomar** algo suelto, el imán del Abejorro, el de la grúa del Cachalote y la cuna de
  su lomo. Un anclaje dice su zona y su modo (`"anclaje": { centro, zona, masa, carga, cuantos,
  modo }`): sin modo asienta la carga en su suelo, `"cuna"` la centra y la cuadra, `"iman"` la toma
  como está. Clic en la palanca de un anclaje abierto con algo suelto encima: lo ancla.
- **El imán se lleva todo lo que tiene debajo** (`cargo::choose`, el mismo en la grúa y en el
  remolcador): cada cuerpo suelto bajo su cara y contra ella, del más centrado al menos, hasta su
  carga nominal (`"carga"`: 2 500 kg). Lo que le haría pasarla lo deja y lo dice
  (`<id>.sobrecarga` y un aviso); lo que sigue amarrado al suelo, tampoco (`<id>.anclada`).
  `<id>.masa` son los kilos que sujeta cualquier anclaje. [`CARGA.md`](CARGA.md).
- **Manos** (`app/src/hands.rs`, `scenario.jsonc`: `player.manos`): con las manos libres, clic
  mantenido sobre algo suelto lo coge; va delante de ti con un muelle estable (Tan, Liu y Turk)
  de fuerza limitada: lo ligero se lleva, lo pesado se arrastra, lo anclado no se mueve (dice por
  qué). La rueda lo acerca o lo aleja.
- **Mochila y naves en marcha** (`app/src/pilot/pack.rs`, [`MOVIMIENTO.md`](MOVIMIENTO.md); sus
  números, en `player.mochila`): dentro de los compartimentos de una nave vas con ella, la hayas
  pisado o hayas entrado volando; al salir por el aire conservas su velocidad, y la
  mochila te estabiliza respecto a la nave que tienes al lado **tal como va ahora** (no al suelo):
  si cae o va en órbita caes con ella sin gastar gas, si acelera la sigue hasta donde puede, y a
  más de 60 m de ella te deja a la velocidad que llevaba. **Z** apaga el estabilizador: entonces
  nada de lo que haga la nave te arrastra. A cualquier velocidad la nave se queda quieta frente a
  ti (sin tirón al salir, sin saltos al tocarla). Con la mochila en el aire, abajo a la derecha:
  altura, subida y la nave más cercana con su distancia y tu velocidad respecto a ella.
- **Sonido** (`crates/audio`, `app/src/sound.rs`, `assets/defs/sounds.jsonc`): ningún sonido está
  grabado; cada uno es una receta de datos (capas de osciladores y ruidos por un filtro con su
  envolvente) que un mezclador de 32 voces hace sonar en el hilo de la tarjeta. Lo que se oye va
  por donde llega: **aire** (el del compartimento donde estás, a su presión: en el vacío, nada),
  **contacto** (lo que tocas: la cubierta bajo las botas, el casco que te lleva, el mando bajo la
  mano; sordo) y **traje** (tu respiración, el ventilador, la mochila; siempre). Nada es de una
  nave en concreto: pasos según lo que pisas, motores según lo que empuja al casco, mecanismos
  según cualquier articulación que se mueva, anclajes e imanes según su estado, mandos según lo
  que el propio mando dice que hizo (chasquido, tope, muelle, tapa). Una nave declara lo suyo con
  señales `sonido.<id>` (el nivel del bucle `<id>`): la alarma general suena hasta que se
  reconoce. Volumen en Esc > CONTROLES; `--mudo` lo apaga; `--prueba-sonido` dice qué tarjeta hay.
- **Polvo** (`app/src/dust.rs`): las botas levantan un abanico de granos al pisar regolito y todo
  chorro que pegue contra el suelo dentro de su alcance (40 m × √(empuje / 45 kN): la mochila
  unos 5 m, un motor de 65 kN unos 48) saca una lámina de polvo a ras de suelo, recta desde donde
  pega. Sin aire no hay nube: cada grano vuela su arco y cae. Tope de granos por segundo entre
  todos los chorros a la vista.
- **F12:** foto de lo que ves, sin HUD, a `fotos/`.
- **Ordenador de vuelo** (`ship/src/flight.rs`): con el peso en el tren (`vuelo.tierra`) el
  estabilizador suelta el casco (peleando contra el suelo, las toberas paseaban la nave); reparte
  el empuje entre los motores para que pase por el centro de masas donde lo haya dejado la carga
  (`trim`); con giróscopos (`vuelo.ruedas`) gira primero con ellos y con las toberas el resto, y
  los descarga siempre que el ordenador vaya (o con su interruptor, si la nave lo tiene:
  `vuelo.descarga`). Con la palanca suelta **mantiene la actitud** (V40, abajo).
- **Herramientas en la mano:** se dibujan en el marco del ojo (`gear::eye_frame`), así que siguen a
  la vista también hacia arriba y hacia abajo.
- **Tráfico** (`scenario.jsonc`: `trafico`, `core/src/traffic.rs`): 2 000 naves ligeras que van de
  campo en campo (despegan en vertical, crucero a su nivel, aterrizan, esperan y vuelven a salir) y
  unas cuantas en órbita. No se cruzan: una plaza por nave, niveles a 120 m, y un nivel solo se
  toma si nadie pasa cerca. No son naves simuladas: cada una es un plan de vuelo.
- **Guiones de cámara** (`--guion tools/camara/*.jsonc`): cámara en cualquier punto de una nave
  (`camara`) o del sitio (`camara_sitio`), `pulsar` (`mando#n` para el botón n de un bisel o
  teclado), `ajustar`, `senal`, `articulacion`, `volcar`, `foto`, `niveles_detalle`, `linterna`,
  `agachado`, `medir` (telémetro, escáner, tráfico y memoria de GPU al registro), `apuntar`,
  `equipo`, `gatillo`, `vista_integridad`, `danar`, `flota`, `fuego`, `perfil` y `fin_perfil`;
  `donde` (dónde está el jugador en la nave, qué lo lleva y a qué velocidad va respecto a ella;
  y qué rige ahí: cuerpo, tirón, peso, su arriba frente al de la nave y lo que lee la brújula),
  `poner` (la nave a tal altura sobre tal cuerpo, con velocidad, giro y volcada, con quien
  lleve dentro),
  `hud` (las fotos de un guion van sin HUD salvo que lo pida), `mochila`, `subir`, `mirar`
  (rumbo y cabeceo del jugador), `menu` (abre el menú en una pestaña), `coger`, `sentar` (el
  jugador en un asiento de la nave) y `oir` (escribe en el registro qué suena: el aire, los bucles
  y los golpes desde la última vez). Un guion puede decir qué naves del escenario quiere
  (`"naves": ["alcotan"]`): los de rendimiento lo hacen, para que lo que gane el escenario no
  cambie la medida.
  Guiones: `alcotan_revista`, `cables`, `tapas`, `pantallas`, `niveles`, `avisos`, `soldador`,
  `trafico`, `hud`, `puente`, `paneles_pared`, `carga`, `manos`, `abejorro`, `cachalote`, `sonido`,
  `mochila_nave`, `espacio` (del suelo a órbita, al espacio libre y a la Luna menor);
  y los de rendimiento en `tools/rendimiento/`.

## Dos versiones

- `LunaV32_debug.exe`: `cargo build --release -p lunar-app`.
- `LunaV32_demo.exe`: `cargo build --release -p lunar-app --features demo`. La jugable: sin vuelo
  libre (F), sin las armas de prueba, sin F4, F6 ni F7; el catálogo solo tiene naves. Se juega a
  pie, con la mochila y el equipo.
- `LunaMCP.exe`: `cargo build --release -p lunar-editor` (`target/release/lunar-mcp.exe`).

## El cuerpo del jugador, las manos y las herramientas

- **Modelo y esqueleto:** `assets/models/astronauta.glb` (52 huesos, dedos incluidos) lo hace
  `tools/modelos/astronauta` (ver [`RIGGING.md`](RIGGING.md)). `assets/defs/rigs/astronauta.jsonc`
  dice qué hueso es qué (pelvis, columna, piernas, brazos, cada dedo), dónde están los ojos, las
  poses de mano, los puntos del traje a los que va una mano y cómo anda. El código no conoce
  ningún hueso por su nombre.
- **Núcleo (`core/src/anim`):** `skeleton` (árbol, pose, paleta de matrices), `ik` (miembro de dos
  huesos por ley de cosenos, con polo), `spring` (muelles críticos exactos), `clip` (movimientos
  por claves, en datos), `gait` (la marcha).
- **La marcha (`gait.rs`):** no hay ciclo grabado. Cada pie está plantado en un punto del suelo
  (en el marco de lo que pisas: el mundo o la nave) y no se mueve mientras el cuerpo pasa por
  encima; cuando le toca, se levanta y va en arco al sitio donde hará falta, preguntando al suelo
  (terreno o cubierta) y pasando por encima de lo que haya en medio (escalón, umbral). El vuelo del
  pie dura según la gravedad; parado, un pie fuera de sitio o girado se recoloca.
- **El cuerpo (`app/body.rs`):** cuelga de los ojos (la cámara) y pisa donde dice la marcha; las
  caderas bajan lo que haga falta para que las piernas lleguen; las manos van a donde se les pide
  (`Grip`) con el codo abajo, y si el brazo no llega gira el torso y adelanta el hombro. El codo
  rodea el tronco en vez de atravesarlo, y un brazo propio entre los ojos y lo que se mira se
  transparenta; las dos cosas con el tronco y el grosor de los brazos medidos de la malla
  (`app/bulk.rs`). Sentado, se sienta. Se dibuja con `render/bodies.rs` (piel con cuatro huesos
  por vértice, desvanecido por hueso con trama, sombra propia); a sus propios ojos sin casco ni
  aro del cuello (`oculto`).
- **Herramientas (`app/holding.rs`, `gear.jsonc` → `sujecion`):** dónde se sujeta, cuánto pesa en
  la mano (`inercia`, `paso`), `retroceso`, sus piezas móviles, sus puntos (agarres, boca,
  recámara), las manos en reposo, el gatillo y sus **clips** (`sacar`, `guardar`, `disparo`,
  `recarga`): canales por claves para la herramienta y sus piezas, la ruta de cada mano punto a
  punto (de la herramienta o del traje, `cuerpo:<punto>`) con su pose, lo que lleva la mano
  (`lleva`) y eventos (`cargado`, `sonido:<id>`). Una herramienta nueva es datos y un modelo.
- **Guiones:** `andar`, `correr`, `saltar`, `bajar`, `ir` (los pies a un punto de la nave),
  `mirar_a`, `camara_jugador` (cámara en el marco del jugador). `tools/camara/cuerpo.jsonc`,
  `herramientas.jsonc`, `recarga.jsonc`, `abordo.jsonc`; `tools/camara/hoja.py <prefijo>` junta
  las fotos en hojas.

## Asientos, bajadas y vista exterior (V34)

- **Sentarse (`ship/diag.rs` → `seat_fit`, `app/body.rs`):** el cuerpo cuelga de los ojos del
  asiento (`asientos[].ojos`), así que esos ojos tienen que estar donde los tiene un cuerpo sentado
  en el cojín (`asientos[].pieza`): 0,89 m sobre él (`SEATED_EYE`), con las rodillas saliendo de 0 a
  20 cm por delante de su borde. `seat_fit` lo comprueba para cualquier nave (lo corren
  `tests/naves.rs` y el validador del editor/MCP); una prueba del juego sienta el cuerpo en cada
  asiento de cada nave y mide cadera, rodilla, tobillo y mano. Las medidas salen del esqueleto
  (`rigs/astronauta.jsonc` → `sentado`): no se ponen a mano en el código.
- **El asiento (`components/naves.jsonc` → `asiento`, receta en `tools/modelos/recetas/cabina.py`):**
  para tripulante con traje y mochila: banqueta bajo los muslos, repisa bajo la mochila y una
  cuna detrás de ella en vez de respaldo. Ocupa 0,60 m por detrás de su origen: al colocarlo, los
  ojos van 0,11 m por delante de su origen y 1,35 m sobre su base.
- **Sentado**, las botas bajan al suelo que haya bajo ellas, las manos descansan en los puntos que
  diga el esqueleto (`sentado.manos`: los muslos) y «arriba» es el de la nave, vuele como vuele.
- **Bajarse (`ship/def.rs` → `ExitDef`, `app/aboard.rs` → `exits`, `app/pilot/walk.rs` → `room`):** cada
  nave declara sus `bajadas`: zonas (`en`, `zona` de ancho por largo, `rumbo` con el que quedas
  mirando, `baja` cuánto puede estar el suelo por debajo). Al levantarte se prueba primero la
  `salida` del asiento y luego las zonas (las que nombre el asiento en `bajadas`, o todas, la más
  cercana primero), punto a punto: se toma el primero con suelo debajo (cubierta, estribo o
  terreno) y sitio para un cuerpo de pie. Lo ocupado (carga, otra nave, una roca) se salta.
- **Vista exterior (`app/chase.rs`, tecla V):** a pie, un brazo de cámara desde junto a la cabeza
  hacia atrás y al hombro derecho; lo que se cruce la acerca de golpe y vuelve sola; nunca bajo el
  suelo. Sentado, gira alrededor de la nave a una distancia proporcional a su radio. Lo que el
  jugador apunta se recalcula desde sus ojos hacia lo que hay bajo el centro de la imagen, así que
  manos, herramientas y mandos funcionan igual. El cuerpo se dibuja entero (con casco).
- **Rejilla de la bodega (Cachalote, `tools/naves/cachalote.py` → `grid_paint`, `grid_labels`):**
  calcas `linea` y `regla` (`tools/texturas/calcas.py`) y rótulos: una celda por bahía, reglas
  en metros desde el umbral de la rampa y desde el centro, letras de columna y números de fila
  en los bordes. Las señales derivadas `grua.largo`, `grua.banda`, `grua.columna`, `grua.fila` y
  `grua.centrada` dan la posición del gancho en esas mismas medidas; la pantalla de la consola
  de la grúa las muestra.
- **Menú de inicio (`app/start.rs`) y opciones de arranque (`app/cli.rs`):** el juego abre con el
  mundo ya en marcha detrás y la cámara girando alrededor de donde se va a empezar; los sitios
  salen de lo que hay (el inicio del escenario y cada nave posada: junto a ella o a sus mandos).
  `--sin-menu` lo salta; `--pantalla completa|ventana` y `--ventana ANCHOxALTO` los usa el
  launcher (`crates/launcher` → `LunaLauncher.exe`).
- **Guiones:** `tercera` (metros; 0 la quita), `inicio`, `lugar`. `tools/camara/tercera.jsonc`,
  `rejilla.jsonc`, `inicio.jsonc`.

## Arranque, vista libre, andar y otros jugadores (V35)

- **Arranque (`app/boot.rs`, `app/splash.rs`, `app/splash_gpu.rs`, `play.rs` → `App::start`):** el
  juego se monta en un hilo propio (`State::new`, que avisa a `Boot` al acabar cada etapa) con su
  ventana aún sin enseñar; el hilo principal enseña la pantalla de carga en otra ventana con un
  dispositivo gráfico pequeño aparte, así que la ventana responde siempre. La superficie de la
  ventana del juego se crea en el hilo de la ventana (`lunar_render::Screen`) y el resto donde
  sea. Al terminar, la ventana del juego sale en el sitio de la de carga. La pantalla de carga
  (desde la V36) tiene la composición del menú de inicio: el emblema y el nombre en el mismo
  sitio y, donde luego están las entradas del menú, la etapa en curso, una línea que se llena y
  la lista de etapas con lo que tardó cada una; de fondo, el espacio y el limbo de la Luna, por
  el que avanza el amanecer según lo cargado. No hay nada que tocar. Al acabar, el menú toma el
  relevo en el mismo sitio y el mundo sale de negro detrás (`Start::after_splash`). Lo que tardó
  cada etapa se guarda en `out/arranque.json` para que la línea avance pareja la vez siguiente.
  Las etapas son una lista (`boot::STAGES`).
  `--prueba-carga CARPETA` saca fotos de esa pantalla y `--prueba-arranque` hace un arranque
  completo, las dos sin enseñar ventana.
- **Guiones sin ventana (`cli.rs` → `Options::hidden`):** `--guion`, `--bench` y `--shot` corren
  con la ventana oculta (los fotogramas se hacen igual y las fotos salen igual); `--visible` la
  enseña.
- **Vista libre (`app/freelook.rs`, Alt mantenido):** el ratón gira la cabeza (desde tus ojos) o
  la cámara (desde fuera) y no el cuerpo: la herramienta, la mira y los mandos siguen donde
  apuntaba el cuerpo. Al soltar, vuelve. El casco no gira (va fijo al torso): por eso desde
  fuera no se ve girar nada.
- **Andar (`core/anim/gait.rs`):** el ritmo sale de la longitud del paso (`paso`) y de la parte del
  ciclo que cada pie está en el suelo (`apoyo`: 60 % andando — siempre hay un pie en el suelo, con
  cualquier gravedad —, 40 % corriendo — un momento en el aire entre zancadas —, entre las
  velocidades `correr`). Datos en `rigs/astronauta.jsonc` → `marcha`.
- **Mochila:** con el estabilizador, al soltar las teclas frena a `mochila.frenada` m/s²
  (`scenario.jsonc`), más que su empuje lateral.
- **Otros jugadores (`app/multi.rs`, `crates/net`, `crates/server`; ver
  [`MULTIJUGADOR.md`](MULTIJUGADOR.md)):** con `--servidor HOST:PUERTO` cada juego simula todo y la
  red los pone de acuerdo: tu jugador sale unas veces por segundo y los demás entran igual (cada
  uno con su cuerpo, animado aquí a partir de eso); cada nave la simula su dueño (quien se sienta
  a sus mandos; si nadie, el primer jugador) y su estado va a los demás; un mando accionado se
  cuenta como «este mando vale ahora tanto» (`Aboard::changed` → `Intent::Set`; la prueba
  `ship/tests/replica.rs` comprueba que vale para todos los mandos de todas las naves) y una nave
  puesta en juego se pone en todas las copias. El nombre de cada uno sale sobre él (`Hud::tags`).
- **Ediciones:** `--features demo` (demo), `--features multijugador` (la demo más red:
  `LunaV<N>_multiplayer.exe`), sin nada (desarrollo; también puede conectarse, para pruebas).
- **Guiones:** `cabeza` [derecha, arriba] en grados. `tools/camara/andar.jsonc`,
  `cabeza.jsonc`, `multi_a.jsonc` + `multi_b.jsonc` (dos juegos contra un servidor).

## Energía, secciones de panel y reactor (V35)

- **Balance de energía (`ship/power.rs`):** toda nave tiene las señales `energia.generacion`,
  `energia.consumo`, `energia.baterias` (lo que sale de ellas; negativo si se cargan),
  `energia.balance`, `energia.carga` (0..1) y `energia.autonomia` (s; `NO_DRAIN` si no se gastan).
  No se resuelve nada para ellas: se leen de los puertos de las redes eléctricas tras cada
  resolución. Lo que solo pasa por una máquina de una red eléctrica a otra (un convertidor entre
  barras) no cuenta ni como generado ni como gastado: solo lo que pierde. La autonomía es la carga
  entre el ritmo al que cae, promediado 6 s (media simple hasta entonces, para que acierte desde
  el primer segundo).
- **Secciones (`ship/sections.rs`, `assets/defs/secciones.jsonc`):** un grupo de mandos con su
  título, escrito una vez, que cualquier panel lleva con `"seccion": "<id>"` en uno de sus grupos.
  Los mandos se llaman `<grupo>_<mando>` (un panel puede llevar la misma sección dos veces); el
  grupo toma el título de la sección si no tiene uno propio. Una sección nueva es solo datos. La
  primera: `energia` (GENERA, CONSUMO, QUEDAN, barra de carga, lámpara de baterías), en el panel
  `panels/energia.jsonc`, montado en el Alcotán y el Cachalote; la página `energia` de las
  pantallas (`mfd_auto.rs`) empieza por las mismas tres cifras.
- **Títulos de grupo (`controls/layout.rs` → `title_room`):** la banda del título crece con la
  escala del panel igual que los rótulos, y su letra es siempre mayor que la de ellos (6,5 mm
  frente a 4,2 a escala 1).
- **Reactor (`machines/models/reactor.rs`):** la masa térmica del combustible y del bucle es un
  dato (`capacidad_combustible`, `masa` del bucle); con los de ahora está en línea a los 45 s, da
  2,4 kW eléctricos a los 3 min y unos 7 a los 6,5. El accionamiento de barras retiene la subida
  si el periodo baja de `PERIOD_HOLD` (5,5 s; la parada por periodo corto es a 3 s): un salto de
  POTENCIA ya no lo dispara. Y baja lo pedido según se acerca la temperatura del combustible a la
  de parada (`SETBACK`, `SETBACK_BAND`; señal `reactor.limitado`, lámpara ESTADO en ámbar fijo y
  aviso): sin sumidero de calor se queda a media potencia en vez de pararse.
- **Aire, carga y chorros:** [`AIRE.md`](AIRE.md), [`CARGA.md`](CARGA.md),
  [`CHORROS.md`](CHORROS.md).

## Vuelo, masa y tiempos (V36)

- **Dónde empujan los motores.** El ordenador de vuelo reparte el gas entre los motores para que
  empujen por el centro de masas (`flight.rs` → `trim`), pero dos motores a la par no pueden
  compensar el cabeceo: en una nave de dos góndolas tienen que estar **bajo el centro de masas**.
  El Alcotán las tenía 1,3 m por detrás (se iba de morro y lo sostenían las toberas a tope): sus
  alas están ahora 1,5 m más a proa (`ala_*`, `pilon_*`, `gondola_*`, `deposito_*`, sus luces y
  sus nudos de red en `alcotan.jsonc`). El Cachalote coloca las suyas con `WING_Z` y `AFT_WING` en
  `tools/naves/cachalote.py` para que el centro caiga entre sus dos pares.
- **Ralentí.** Un motor encendido con el acelerador a cero da su mínimo de estrangulamiento:
  ahora el 2 % (`"estrangulamiento": [0.02, 1.05]`), no el 35 %. La nave no se mueve hasta que
  se le da gas.
- **MANTENER nivela.** Con MANTENER y la palanca suelta, el ordenador gira la nave hasta poner
  el empuje de sus motores contra la gravedad (`LEVEL`: 0,8 rad/s por radián, 0,12 rad/s como
  mucho) y descuenta al momento el par que dejan los motores (antes solo frenaba el giro y las
  toberas perseguían la deriva de un casco inclinado). Donde no hay gravedad no hay vertical que
  guardar.
- **La carga mueve el centro de masas** (lo que sujetan anclajes, imanes y cuna cuenta en la masa
  y en su momento: `core/structure/hold.rs` → `weigh`), y el reparto de motores lo sigue.
- **Pruebas:** `crates/ship/tests/vuelo.rs` (ralentí, MANTENER bajo cuatro gravedades, carga
  descentrada) y `masa.rs`.
- **Masa** (`ship/mass.rs`, `contents.rs`): los depósitos pesan lo que dice su máquina; señales
  `nave.masa`, `nave.masa_seca`, `nave.propelente`, `nave.propelente_nivel`, `nave.dv`,
  `nave.empuje_peso`, `nave.centrado`; sección de panel `masa` (`secciones.jsonc`). Ver
  [`CARGA.md`](CARGA.md) §6.
- **Tiempos y lógica:** [`TIEMPOS.md`](TIEMPOS.md), [`LOGICA.md`](LOGICA.md).

## Manos, huellas y visor (V36)

- **Manos sin herramienta** (`app/handwork.rs` y sus partes `reach.rs`, `cockpit.rs`,
  `gestures.rs`, `wrist.rs`; datos `manos.jsonc`, `gestos.jsonc`, `muneca.jsonc`): cada cosa dice
  dónde quiere una mano y `Handwork` se la da a la primera que la pide (herramienta, mando,
  ordenador de muñeca, gesto, palanca y gases del asiento) sin saltos. Qué hace la mano con cada
  clase de mando es un dato. El juego lo llama desde `play.rs` (`handwork.drive`, `worked`,
  `gesture_key`, `toggle_wrist`). Teclas: Tab (gestos), Y (muñeca). Guion:
  `tools/camara/manos_mandos.jsonc`; pasos `gesto` y `muneca`.
- **Huellas** (`app/footprints.rs`, `render/prints.rs`, `huellas.jsonc`): un anillo de marcas
  (bota, pata, bulto, barrido) dibujadas como cuadros sobre el suelo, un búfer y un dibujo; solo
  las cercanas van a la GPU.
- **Visor** (`app/visor.rs`, `visor.jsonc`): filtro de soldar, visor solar (U) y vaho, como
  parámetros del posproceso.

## Coherencia y ordenador de vuelo (V40)

`crates/ship/tests/coherencia.rs` prueba cada nave que vuela como la encuentra un piloto, sin
nombrar ninguna: lo que falla ahí es lo que un jugador llamaría «no va» o «hace cosas raras».

- **Teclas mantenidas** (`ship/src/seat_keys.rs`, lo único que interpreta las teclas de un
  asiento, para el juego y para las pruebas): una tecla mantenida gira su mando a
  `HELD_RATE` muescas por segundo, cortadas en tantos fotogramas como haya; tiene que moverlo lo
  mismo a 30 que a 240 fps y sacarlo de cualquier retén. Antes Mayús no sacaba los gases de CORTE:
  cada fotograma los movía menos de lo que atrapa el retén y el retén los devolvía (cuantos más
  fps, peor). Ahora **un retén atrapa la palanca que entra en él, nunca la que sale**
  (`controls/mech/lever.rs`). (El retén de una rueda sí la sujeta contra una muesca: se sale
  girando deprisa o con Ctrl.)
- **Ruedas**: una muesca da el paso de sus datos y de su ayuda; Mayús el grueso y Ctrl el fino,
  **que ninguna velocidad de giro multiplica** (`controls/mech/wheel.rs`). La velocidad de giro
  de la rueda del ratón se mide (`lunar_controls::Spin`: una muesca suelta no va «deprisa»);
  antes se inventaba como muescas × 10 y una sola muesca contaba como un giro rápido (Ctrl daba
  40 en vez de 5).
- **Palanca y traslación**: cada tecla gira o empuja la nave por un solo eje, en el sentido que
  dice su ayuda, y la misma tecla hace lo mismo en todas las naves (la prueba imprime la tabla con
  `--nocapture`); soltada, el estabilizador la para.
- **Potencia sin giro**: con el acelerador abierto y la palanca suelta, en cada posición de las
  góndolas, la nave no se gira sola y sus giróscopos no se quedan llenos. Lo que había: el Azor se
  iba de morro 25° en 30 s y tenía siempre los giróscopos saturados.
- **Piloto automático sin cuerpo**: lo que no tiene sentido fuera de todo cuerpo (ALTURA, RUMBO,
  DESPEG., ATERRIZ.) lo dice (`ap.sin_cuerpo`) en vez de encenderse como si volara.

Lo que se arregló en el ordenador de vuelo (para todas las naves):

- **Reparto entre toberas** (`allocate`): mínimos cuadrados acotados por descenso por
  coordenadas (cada tobera puesta exactamente a lo que mejor completa lo que falta), con lo que se
  dio el tic anterior como punto de partida y un pequeño coste por lo que se gasta. Antes, 40
  pasos de gradiente con un paso diminuto no llegaban y lo que quedaba por debajo del 2 % se
  tiraba: **ninguna tobera del Azor se encendía nunca** para compensar sus motores. Lo que se pide
  más allá de lo que pueden dar en un eje se recorta a lo que pueden, y un par perdido pesa más
  que un empuje perdido (`TURN_WEIGHT`).
- **El par de los motores se compensa por lo que empujan de verdad** (no por lo que se les pidió:
  un motor que arranca o se apaga empuja otra cosa).
- **Reparto entre motores** (`trim`): solo se corrige la parte del par que los motores hacen
  distinta entre sí; la que hacen todos igual (dos góndolas a la par y el cabeceo) cambia con el
  total, que no se toca. Antes se intercambiaba por un cabeceo imposible y dejaba ~4 000 N·m de
  alabeo en el Azor.
- **Mantener la actitud**: con el estabilizador y la palanca suelta, primero para el giro y luego
  guarda la actitud en que se paró (`KEEP`); antes solo frenaba el giro y cualquier par pequeño
  la llevaba unos grados por minuto.
- **No se pide más giro del que se puede dar** (`can_turn`): el error de giro de cada eje se
  limita a lo que sus toberas y giróscopos dan. Con la inercia cruzada del casco, una guiñada
  imposible del Alcotán se comía el alabeo que debía compensarla.
- **Giróscopos**: se descargan siempre que el ordenador vaya (o con su interruptor si la nave lo
  tiene, como el Abejorro y el Cachalote).

## Formato

`rustfmt.toml` (líneas de hasta 255, `use_small_heuristics = "Max"`) es el estilo del código: sin
él, `cargo fmt` lo reparte todo a 100 columnas.

## Pruebas

`cargo test --workspace` (perfil de pruebas optimizado: segundos). En `crates/ship/tests/`:

- `coherencia.rs`: la nave como la encuentra un piloto (teclas mantenidas a cualquier fps, pasos
  de ruedas y retenes, cada tecla de la palanca por su eje, potencia sin giro, piloto automático
  sin cuerpo). Ver «Coherencia y ordenador de vuelo (V40)».
- `alcotan.rs`: se arma, arranca, presuriza, tres verdes, hidráulica, rampa, reactor, motores,
  posada y dormida.
- `aire.rs`, `estanqueidad.rs`: el modelo de aire y la estanqueidad por vóxeles.
- `trasvase.rs`: las válvulas de mano igualan (más deprisa cuanto más abiertas, nada cerradas, un
  volante a cada lado); el compresor vacía la bodega en el depósito hasta su límite, cada vez más
  despacio, se para sin corriente, con el depósito lleno o el destino abierto, y lo devuelve; no se
  crea ni se pierde gas; una nave en reposo no hace nada de esto.
- `cierres.rs`, `tapas.rs`, `paneles.rs`: rampa sin salto; tapas que encierran la palanca y se
  abren sin chocar; paneles que caben, sin nada delante, cables por arriba, sin tapar calcas.
  Y ningún panel cortado por su pared ni con un cable o un tubo por delante de su cara.
- `asientos.rs`: desde el asiento del piloto y el del copiloto, cada mando e instrumento de sus
  paneles (principal, pedestal, techo y su consola lateral) se ve, se alcanza y lo encuentra la
  mano; los ojos a un metro del panel principal y la consola de techo delante de la cabeza.
- `carga.rs`: la carga solo se une a su anclaje; un anclaje abierto (o destruido) la suelta entera,
  cae a la cubierta y se queda en la bodega; no falta en la nave; la carga suelta sube con la nave
  que despega; el bidón de propelente estalla al romperse (y el de agua no).
- `contenido.rs`: un bidón pesa vacío más lo que lleva, y menos al gastarse (él, la nave que lo
  lleva y lo que lee el imán que lo sujeta); su centro de masas baja al vaciarse; nada se vuelve a
  pesar si nada cambia; lo rotulado y lo que lee el escáner salen de los datos; cada tipo de carga
  pesa lo que uno de verdad; el imán toma los dos bidones que tiene debajo y suelta los dos, deja
  el tercero, deja lo que pasa de su carga nominal y lo dice, y no toma lo amarrado.
- `cables.rs`: ninguna canalización por fuera del casco, medio enterrada ni a través de aparatos.
- `fallo_unico.rs`: ningún corte único se lleva nada esencial; y sin troncos, todo cae.
- `reparar.rs`: una placa que falta se encuentra, se repone y la sala vuelve a sellar; una lámpara y
  cada toma repuestas vuelven a funcionar; lo soldado queda como nuevo.
- `presupuesto.rs`: topes de piezas, uniones, vértices, tic, impacto, explosión y choque entre
  naves.
- `modelos.rs`: cada tipo, cada pieza de estructura y cada estilo tiene su modelo y cabe en su
  forma; el reflejado es su reflejo; el marco de una ventana es sólido y su cristal no; los
  rótulos se leen de cerca, dan la vuelta al bidón y dicen lo que marca su componente.
- `lod.rs`, `lod_medida.rs`: seis pasos, silueta lejana con sus colores; peso y coste por tick.
- `gravedad.rs`: la gravedad de a bordo de cada nave sale de sus datos y de lo que la alimenta,
  entra y sale en su tiempo, vale en sus salas; en una nave que no da ninguna se pesa lo que
  queda del tirón según vaya.
- `uso.rs`: **tests lógicos de uso**: una nave en reposo no da ningún aviso; todo compartimento
  se gobierna desde dentro y la esclusa desde fuera; y en simulación: bodega con brecha (ventear la cabina), bodega en vacío (igualar
  con el volante del mamparo), entrar desde fuera (ventear la bodega), ventana del puente rota (igualar, ventear, represurizar).
- `pantallas.rs`, `fichas.rs`: cada botón del bisel su página, zonas de color, rayo al botón; cada
  mando e instrumento dice qué es.
