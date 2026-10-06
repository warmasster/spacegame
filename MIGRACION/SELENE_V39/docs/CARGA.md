# Carga: lo que llevan las cosas, lo que pesa y el imán

Tres piezas, todas de datos y sin una línea por objeto:

1. **Sustancias** (`assets/defs/sustancias.jsonc`): el registro de lo que se puede llevar.
2. **Contenido** (`"contenido"` en un tipo de componente o en un componente colocado): qué
   sustancia lleva, cuánta cabe y cuánta trae. De ahí salen su masa, lo que tiene rotulado, lo que
   lee el escáner y lo que hace al reventar. Lo que queda cambia en marcha por una sola API.
3. **El imán de carga** (`electroiman_carga`, el mismo en la grúa del Cachalote y bajo el Abejorro):
   se lleva **todo** lo que tiene bajo la cara, hasta su carga nominal.

Código: `crates/core/src/structure/contents.rs` (registro, recipientes, lo que queda, masa),
`crates/ship/src/contents.rs` (la clave `contenido` de los componentes), `crates/ship/src/cargo.rs`
(anclajes e imanes). Pruebas: `crates/ship/tests/contenido.rs` y las de cada módulo.

## 1. Sustancias

`assets/defs/sustancias.jsonc` es un mapa de identificadores. Lo carga el catálogo de estructuras
(`Catalog::load`), así que vale para cualquier estructura, no solo para naves.

| Clave | Qué es |
|---|---|
| `nombre` | Como se escribe en una frase (`"agua"`); los rótulos lo ponen en mayúsculas. |
| `densidad` | kg/m³ tal como se lleva (un líquido, la suya; un sólido suelto, la que tiene ensacado). |
| `estado` | `"liquido"`, `"granel"` (sólido suelto), `"piezas"` (cosas embaladas) o `"gas"`. Todo menos un gas se posa en el lado bajo del recipiente; un gas lo llena entero. |
| `medida` | Cómo se cuenta para quien lo lee: `"L"` o `"kg"`. Sin ella, un líquido por litros y lo demás por kilos. |
| `estalla` | Lo que suelta un recipiente al destruirse, según lo que llevaba: `energia` (J por kg), `radio` (m por raíz cúbica de kg: el alcance de una explosión va con la raíz cúbica de su energía) y `efecto` (`assets/defs/explosions`). |

```jsonc
"agua":       { "nombre": "agua", "densidad": 1000, "estado": "liquido" },
"propelente": { "nombre": "propelente", "densidad": 875, "estado": "liquido",
                "estalla": { "energia": 5400, "radio": 1.13, "efecto": "granada" } },
"regolito":   { "nombre": "regolito", "densidad": 1750, "estado": "granel" },
"repuestos":  { "nombre": "repuestos", "densidad": 400, "estado": "piezas" }
```

Una sustancia nueva es una línea aquí. No hay código por sustancia.

## 2. Contenido de un recipiente

En un tipo de componente (`assets/defs/components/*.jsonc`):

```jsonc
"bidon_agua": {
  "nombre": "Bidón de agua",
  "rotulos": [ { "texto": "{sustancia}", ... }, { "texto": "POTABLE · {capacidad}", ... } ],
  "contenido": { "sustancia": "agua", "capacidad": "170 L" },
  "piezas": [ { "id": "", "hueco": 0.004, "forma": { "kind": "cylinder", "radius": 0.26, "height": 0.86 }, ... }, ... ]
}
```

| Clave de `contenido` | Qué es |
|---|---|
| `sustancia` | Identificador del registro. |
| `capacidad` | Lo más que lleva, por volumen o por masa: `"170 L"`, `"600 kg"` (un número suelto son kg). Sin ella, lo que cabe. |
| `lleno` | La parte de eso que trae al construirse (0 a 1; sin ella, 1: lleno). |
| `piezas` | Las piezas del componente que lo llevan, por su `id`; se lo reparten según el sitio de cada una (las tres capas de sacos del palé). Sin ella, la primera pieza. |
| `nivel` | El nombre de una señal del componente que dice cuántos kg lleva (`<id>.<nivel>`: la `masa` de la máquina de un depósito). Lo que pesa la sigue. Ver §6. |

- **El sitio sale de la forma**: el volumen de la pieza tal como está definida (un cilindro es
  redondo: π·r²·h) menos sus paredes (`"hueco"` × su superficie). Lo declarado **tiene que caber**:
  si no, la nave no se arma y el error dice cuánto cabe («200 L de agua no caben: dentro hay sitio
  para 175 L»). Una pieza maciza (sin `hueco`) no puede llevar nada.
- **Donde se coloca** un componente (`assets/defs/ships/*.jsonc`) puede decir lo suyo, solo lo que
  cambia: un bidón a medias, o el mismo bidón con otra cosa dentro.

```jsonc
{ "id": "carga_agua_2", "tipo": "bidon_agua", "en": [...], "anclaje": "anclaje_agua",
  "contenido": { "lleno": 0.5 } }
{ "id": "bidon_x", "tipo": "bidon_agua", "en": [...],
  "contenido": { "sustancia": "propelente", "capacidad": "140 kg" } }   // su rótulo dirá PROPELENTE y estallará
```

- Una **forma suelta** (`"forma"` sin `"tipo"`) con `"hueco"` y `"contenido"` también vale.
- En las piezas de una estructura que no es nave (`assets/defs/structures/parts/*.jsonc`) la clave
  es `"holds": { "substance": "agua", "capacity": 4000, "fill": 1.0 }` (kg; `capacity` opcional).

### Lo que sale solo de esos datos

- **Masa.** La de un recipiente vacío es la de sus cáscaras (material × superficie × `hueco`, como
  cualquier pieza); lleno, eso más lo que lleva; y menos según se gasta.
- **Rótulos.** En el `texto` de un rótulo, `{sustancia}` («AGUA») y `{capacidad}` («170 L»), además
  de los de siempre (`{id}`, `{nombre}`, `{etiqueta}`, `{serie}`, `{matricula}`, `{nave}`). Un rótulo
  que los usa en algo que no lleva nada no sale.
- **Lo que lee el escáner.** «170 L de agua». Con `"recurso"` se dice de otra manera:
  `"recurso": "{cantidad} de {sustancia} en sacos"` → «600 kg de regolito en sacos» (`{cantidad}` lo
  que queda, `{capacidad}` lo que cabe, `{sustancia}`). `recurso` sin `contenido` sigue siendo un
  texto fijo, para lo que no se cuenta.
- **Lo que hace al reventar.** Lo dice la sustancia (`estalla`), por los kg que traía. Un bidón de
  propelente a medias suelta la mitad; **vacío no estalla**. (Una pieza con su propio `"estalla"`
  sigue soltando eso lleve lo que lleve: una botella a presión.)

## 3. Cómo pesa (y qué se aproxima)

Lo que lleva una pieza es un **bloque** dentro de ella: posado en el lado del recipiente que mira
hacia abajo según está la estructura, con la altura que le toca por la parte del sitio que ocupa. Su
masa, su sitio y su inercia entran en las de la estructura (`Structure::mass_props`), junto a las
piezas y a lo que la estructura sujeta. Así el centro de masas de un bidón baja al vaciarse (de
0,42 m sobre su base lleno a 0,33 con tres cuartos, 0,24 a medias y 0,19 con un cuarto) y vuelve
al del bidón solo (0,45 m) cuando no queda nada.

Aproximaciones, dichas una vez:

- el bloque es una **caja** (un cilindro tumbado se llena como una caja de su sección);
- **no chapotea**: gira con su recipiente (un bidón volcado conserva el agua donde la tenía hasta
  que cambia la cantidad, y entonces se posa en el lado que mira abajo en el marco de su estructura);
- un **gas** llena su recipiente entero haya lo que haya.

### La API (una sola puerta)

```rust
// lunar_core::structure — sobre la estructura, por pieza
impl Structure {
    pub fn contents(&self, part: u32) -> Option<&Stored>;   // qué lleva ahora (None: no es un recipiente)
    pub fn take(&mut self, part: u32, kg: f32) -> f32;      // saca hasta `kg`: lo que salió
    pub fn put(&mut self, part: u32, kg: f32) -> f32;       // mete hasta `kg`: lo que entró
    pub fn fill(&mut self, part: u32, kg: f32) -> f32;      // lo deja en `kg`: lo que lleva entonces
    pub fn spent(&self, part: u32) -> bool;                 // recipiente con menos del 2 %
    pub fn told(&self, cat: &Catalog, parts: impl IntoIterator<Item = u32>) -> Option<String>; // «85 L de agua · 50 %»
    pub fn cargo_card(&self, cat: &Catalog) -> Option<String>;                                  // «85 L de agua · 50 % · 97 kg»
}
pub struct Stored { pub part: u32, pub substance: u16, pub mass: f32, pub capacity: f32, /* ... */ }
```

```rust
let agua = s.take(bidon, 0.3);                 // alguien bebe: 0,3 kg menos (o lo que quede)
let sobra = 50.0 - s.put(deposito, 50.0);      // repostar: lo que no cabe se queda fuera
let queda = s.contents(bidon).map(|c| c.share());   // 0..1
```

- **Barato.** `Structure::stored` es una lista corta (solo las piezas que llevan algo: una
  estructura sin recipientes no paga nada). Pedir lo que ya lleva no hace nada. Las propiedades de
  masa se rehacen **solo cuando la cantidad ha cambiado un cuanto** (`QUANTUM`: 0,5 % de la cabida)
  desde la última vez, o al quedar vacío o lleno: 40 sorbos de 0,1 kg a un bidón son 5 pesadas, no
  40. Mientras tanto la cantidad exacta está en `Stored::mass` y la masa de la estructura va como
  mucho un cuanto por detrás. `Structure::weighings` cuenta las pesadas (lo usan las pruebas: una
  nave en reposo no se vuelve a pesar).
- **Va con lo suyo.** Suelta (`Structures::separate`), la carga se lleva lo que llevaba; al tomarla
  un anclaje o un imán sigue siendo suya y pesa en quien la sujeta (`hold::Load`); bebida mientras
  cuelga de un imán, el imán y la nave pesan eso menos.
- Una pieza destruida pierde lo que llevaba; repuesta (`Structure::rebuild`), vuelve vacía.

## 4. Masas: antes y ahora

¿Cuánta agua cabe en uno de esos bidones? El modelo es un cilindro de 0,52 m de diámetro por 0,86 m:
**183 L** por fuera, **175 L** por dentro con sus paredes de 4 mm (180 L el de acero, de 1,2 mm).
Se llena a **170 L**, como uno de verdad (un bidón «de 200 L» mide 0,57 m × 0,85 m por dentro y le
caben 218,7 L hasta el borde: se llena al 95 %). Son **170 kg de agua**; de propelente (875 kg/m³),
149 kg. Antes el rótulo decía «200 L» y el bidón pesaba 42 kg lo llevase lleno o vacío.

| Tipo (`components/carga.jsonc`) | Antes | Vacío | Lleva | Ahora (lleno) | Uno de verdad |
|---|---|---|---|---|---|
| `bidon_agua` — bidón de plástico, 170 L | 41,9 kg | 12,4 kg | 170 kg de agua | **182,4 kg** | vacío 9–13 kg (el de 208 L: 9–12,7); lleno 175–190 |
| `bidon_combustible` — bidón de acero, 170 L | 114,8 kg | 20,8 kg | 148,8 kg de propelente | **169,6 kg** | vacío 17–25 kg; lleno 160–178 |
| `caja_repuestos` — caja de transporte, 118 L | 68,1 kg | 13,8 kg | 40 kg de repuestos | **53,8 kg** | vacía 8–16 kg; llena 45–60 |
| `pale_regolito` — palé con tres capas de sacos | 962,4 kg | 39,5 kg | 600 kg de regolito | **639,5 kg** | vacío 28–48 kg; cargado 625–650 |

(«Antes»: la masa salía de las cáscaras y de piezas macizas — el aro del bidón de acero era un disco
de 71 kg, los sacos eran bloques de regolito sinterizado — y lo que llevaban no pesaba. La prueba
`every_kind_of_cargo_weighs_what_a_real_one_does` imprime la tabla y falla si un tipo se sale de su
intervalo o si falta en ella un tipo nuevo.)

Las cáscaras se afinaron con `hueco` (las formas no cambian, los modelos tampoco): bidón de
plástico 4 mm, de acero 1,2 mm, aros 0,5 mm, caja 4 mm, palé 6 mm, sacos 1,5 mm, cinchas 1 mm. Más
finas aguantan menos: el cuerpo de un bidón de acero tiene ahora 8,7 kJ de puntos de integridad
en vez de 21,6.

Las naves, con su carga a bordo:

| Nave | Antes | Ahora | Su carga antes → ahora |
|---|---|---|---|
| Alcotán | 31 316 kg | 31 272 kg | 1 433 → 1 389 kg (palé, 4 cajas, 2 bidones de agua, 1 de propelente) |
| Cachalote | 70 479 kg | 68 453 kg | 8 641 → 6 615 kg (8 palés, 8 cajas, 4 bidones de agua, 2 de propelente) |
| Abejorro | 2 417 kg | 2 417 kg | no lleva |

El bidón de agua pesa cuatro veces más, el de propelente vez y media, y los palés un tercio menos
(pesaban 962 kg diciendo «600 kg»), así que el Cachalote sale 2 t más ligero. No hizo falta tocar
su lista de carga ni sus motores (`tools/naves/cachalote.py` no cambia).

Con las manos (`player.manos`: 1 200 N de fuerza): un bidón lleno pesa en la Luna 296 N y un palé
1 036 N (antes 1 559 N), así que el palé queda justo al alcance de las manos y un bidón lleno tiene
cuatro veces más inercia que antes. Sin comprobar en el juego.

## 5. El imán de carga

Un solo tipo, `electroiman_carga` (`components/naves.jsonc`), y un solo código (`cargo::Mode::Grip`):
lo que sigue vale igual para `grua_iman` (Cachalote) y para `iman` (Abejorro).

```jsonc
"anclaje": { "centro": [0, -0.78, 0], "zona": [0.5, 0.5, 0.5], "masa": 2500, "carga": 2500, "modo": "iman" }
```

| Clave de `anclaje` | Qué es |
|---|---|
| `centro`, `zona` | La caja (centro y medias medidas, m, marco del componente) donde busca lo suelto: el centro de cada bulto tiene que estar dentro. Para el imán: bajo su cara de 0,8 × 0,8 m con un palmo de margen (0,5 m a cada lado; antes 0,75, con lo que se llevaba cosas que no estaban bajo la placa) y hasta 1 m por debajo de ella, como antes. |
| `masa` | Lo más pesado que toma de una pieza (kg). |
| `carga` | **Carga nominal**: todo lo que sujeta a la vez (kg). Sin ella, sin límite (los anclajes del suelo). |
| `cuantos` | Cuántas cosas como mucho. Sin ella: seis, y un imán **todas las que tenga debajo**. |
| `modo` | Sin él asienta la carga en el suelo de la zona; `"cuna"` la centra y la cuadra; `"iman"` la toma como está. |

**Qué se lleva** (`cargo::choose`). Al pasar a AGARRA mira una vez qué cuerpos sueltos tienen su
centro dentro de la zona, y de ellos toma:

1. los que están **contra la cara**: los que llegan a menos de 20 cm por debajo del más alto (la
   cara baja hasta el más alto; una caja corta junto a un bidón, o la de debajo de una pila, no la
   toca y se queda);
2. **del más centrado al menos**, mientras el total no pase de la carga nominal. El que la haría
   pasar se deja y se sigue con los siguientes por si caben.

Se toman como están: el más alto sube hasta la cara y los demás cuelgan a su lado tal cual estaban
(todos se mueven lo mismo: nunca se meten unos en otros). Dos bidones juntos bajo la placa suben los
dos; de tres en fila con el imán sobre dos, esos dos. Al pasar a SUELTA los suelta todos.

**Por qué del más centrado al menos centrado** y no por peso: el que está en el centro es el que quien
maneja el imán fue a buscar, y así lo que cuelga queda bajo el eje del mástil (o de la panza del
remolcador) en vez de descentrado; además es el orden en que ya los da `Structures::loose_in`.

**Lo que no toma, y cómo lo dice:**

- lo que **sigue amarrado** a su anclaje del suelo: como antes, `<id>.anclada` y el aviso «lo que
  tiene debajo sigue en su anclaje (ANCLAJE_B1): suéltalo primero»;
- lo que le haría **pasar de su carga nominal**: señal `<id>.sobrecarga` a 1 (hasta que se apaga o
  deja de dejar algo) y un aviso, una sola vez, por `Ship::said` y a la caja negra: «Imán de carga:
  deja Palé de regolito (639 kg): pasaría de su carga nominal (300 kg)» (en la prueba, con un imán
de 300 kg).

**Señales nuevas** (de todos los anclajes): `<id>.masa` (kg que sujeta: lo que ha tomado y lo que
lleva amarrado, con su contenido). Del imán: `<id>.sobrecarga`. La consola de la grúa del Cachalote
las enseña en su pantalla: «CARGA 365 kg  EXCESO».

**La carga nominal: 2 500 kg.** Un imán electropermanente de izado se marca por masa, con factor de
seguridad 3:1 (arranca a tres veces lo marcado). Una placa comercial de 590 × 230 mm levanta 2 500 kg
de chapa plana y una de 1 000 × 280 mm, 5 000 kg; la nuestra mide 800 × 800 mm pero agarra tapas de
bidón, cajas y sacos cinchados, no chapa limpia (los fabricantes rebajan la capacidad con el
entrehierro y la superficie), así que se queda en la clase de 2,5 t. En la Luna eso pesa 4 kN, pero
la masa es la misma cuando la nave acelera: el límite es de masa.

**Cuándo busca.** Solo al encenderlo y, mientras está encendido y no sujeta nada, cada 0,4 s (para
tomar lo que le llega después: baja sobre la carga, o el anclaje del suelo se suelta). Sujetando
algo, o apagado, no busca nada; nunca por fotograma.

## 6. Los depósitos de las naves (activo desde la V36)

Una nave pesa lo que lleva. Cada depósito, botella o vaso dice en su tipo qué guarda y de dónde
sale cuánto:

```jsonc
"contenido": { "sustancia": "propelente", "piezas": ["cuerpo", "proa", "popa"], "nivel": "masa", "cabida": "capacidad", "inicial": "masa_inicial" }
```

- `nivel`: la señal de su máquina que dice lo que queda (`<id>.masa`); la nave deja eso en la pieza
  (una comparación por tic; se vuelve a pesar un cuanto cada vez, solo el bloque de ese depósito).
- `cabida` / `inicial`: los parámetros de la máquina que dicen cuánto toma y con cuánto empieza.
  **Lo que toma tiene que caber** en el volumen de la pieza a la densidad de la sustancia
  (`sustancias.jsonc`): si no, la nave no se monta y lo dice (`diag`, prueba
  `a_machine_told_to_hold_more_than_fits...`).
- Sustancias nuevas para esto: `oxigeno` (270 kg/m³ a 20 MPa), `nitrogeno` (212), `aire` (144 a
  12 MPa) y `refrigerante` (1 070).

| Nave | Seca kg | Llena kg | Propelente kg | Empuje/peso en la Luna (llena · seca) |
|---|---|---|---|---|
| Abejorro | 2 417 | 2 897 | 440 (dos depósitos de 240) | 3,41 · 4,09 |
| Alcotán | 32 780 | 35 380 | 2 300 (dos de ala de 1 300) | 1,67 · 1,81 |
| Cachalote | 71 013 | 81 413 | 9 200 (ocho de ala) | 1,97 · 2,26 |

El Cachalote llevaba dos depósitos a los que se les decían 9 t cada uno (no cabían: 1,6 m³ son
1 415 kg): ahora lleva ocho del mismo tipo, dos bajo cada una de sus cuatro alas. Una botella
lleva lo que cabe a 20 MPa (13 kg de oxígeno, 10 de nitrógeno): el Alcotán lleva cuatro de
nitrógeno para poder llenar su bodega una vez.

Señales que toda nave tiene (`ship/mass.rs`): `nave.masa`, `nave.masa_seca`, `nave.propelente`,
`nave.propelente_nivel`, `nave.dv` (ecuación del cohete con el impulso de sus motores),
`nave.empuje_peso`, `nave.centrado`. Las enseña la sección de panel `masa` (`secciones.jsonc`),
montada en el panel de combustible del Alcotán y el Cachalote y en la consola del Abejorro, y la
página de las pantallas.

Pruebas: `crates/ship/tests/masa.rs` (12) y `vuelo.rs` (3). Coste: sección 7 y
[`OPTIMIZACION.md`](OPTIMIZACION.md), V36.

## 7. Rendimiento

- Nada nuevo por cuerpo y fotograma. Una estructura sin recipientes no recorre nada; con ellos,
  `mass_props` suma una lista corta solo cuando ya se iba a rehacer.
- La nave lee `<id>.masa` de sus anclajes solo cuando cambia algo de lo que pesa (versión, pesadas
  o número de cosas sujetas).
- El imán busca al conmutar (y a 2,5 Hz encendido y vacío, como antes); ya no hay tope de «uno».
- Una nave en reposo sigue en su camino rápido: no se vuelve a pesar (prueba) y los presupuestos de
  `tests/presupuesto.rs` siguen pasando.

## 8. De dónde salen las cifras

Solo enlaces abiertos y leídos.

| Qué | De dónde | Qué se tomó | Dónde |
|---|---|---|---|
| Imán electropermanente de izado: tamaños y capacidades | [Assfalg EPMM](https://www.assfalg-metal.com/magnets/lifting-magnets/magnets/electropermanent-lifting-magnet-epmm) | Se marcan por masa: 200, 500, 1 000, 2 500 (placa de 590 × 230 mm), 4 000 y 5 000 kg (1 000 × 280 mm); factor de seguridad 3; «no electricity flows during load transport» | `"carga": 2500` del imán; que sujetar no gaste |
| Imanes electropermanentes grandes | [Assfalg EPMH](https://www.assfalg-metal.com/magnets/lifting-magnets/lifting-magnets-with-mains-power/electropermanent-lifting-magnet-epmh) | 5 a 30 t; solo un impulso para magnetizar o desmagnetizar | Que la clase de 2,5 t es de las pequeñas |
| Reglas de uso de imanes de izado | [Armstrong Magnetics](https://www.armsmag.com/info-technical-data-lifting-magnets-knowledge-safe-operation-rules.html) | Factor de seguridad 3:1; casi nunca se llega a la capacidad máxima marcada (entrehierro, carga fina, superficie sucia); la carga, nivelada; «never lift more than one sheet at a time» | Rebaja de la placa a 2,5 t; que deje lo que pasa de la nominal; que solo tome lo que está contra la cara y no lo de debajo de una pila |
| Cómo funciona un imán electropermanente | [Wikipedia: Electropermanent magnet](https://en.wikipedia.org/wiki/Electropermanent_magnet) | Un pulso conmuta; mantener no pide energía; se usan para izar | Ya estaba en `machines/models/inertial.rs`; confirma el modelo |
| Bidón de 200 L: medidas | [Wikipedia: Drum (container)](https://en.wikipedia.org/wiki/Drum_(container)) | 572 mm × 851 mm por dentro; 218,7 L hasta el borde para 200 L nominales | Llenado al 95–97 %: 170 L en nuestro bidón de 175–180 L |
| Bidón de 208 L: tara | [Repackify: 55 gallon drum dimensions](https://www.repackify.com/blog/55-gallon-drum-dimensions) | Acero cerrado 38–48 lb (17–22 kg), abierto 45–55 lb (20–25 kg); plástico 20–28 lb (9–12,7 kg); con agua ~497 lb | Paredes (`hueco`) e intervalos de la tabla de masas |
| Densidad del agua | [Wikipedia: Properties of water](https://en.wikipedia.org/wiki/Properties_of_water) | 0,99997 g/mL a 4 °C; 0,99705 a 25 °C | `agua`: 1 000 kg/m³ |
| Propelente almacenable | [Wikipedia: Monomethylhydrazine](https://en.wikipedia.org/wiki/Monomethylhydrazine) | 0,875 g/cm³ a 20 °C; punto de inflamación −8 °C; hipergólico; OMS y RCS del transbordador | `propelente`: 875 kg/m³; «INFLAMABLE» y que estalle. Cuadra con los depósitos de las naves (1 300 kg en 1,5 m³) |
| Regolito lunar: densidad aparente | [Lunar Sourcebook, cap. 9 (LPI)](https://www.lpi.usra.edu/publications/books/lunar_sourcebook/pdf/Chapter09.pdf) | 1,50 ± 0,05 g/cm³ los 15 cm de arriba, 1,66 ± 0,05 los 60 cm; in situ de 1,36 a 1,85 (tubos del Apolo 15) y hasta 1,99 (sondeo del Apolo 17) | `regolito`: 1 750 kg/m³ ensacado y apretado: 600 kg en los 354 L de los sacos del palé |
| Regolito lunar | [Wikipedia: Lunar regolith](https://en.wikipedia.org/wiki/Lunar_regolith) | «about 1.5 g/cm3 and increases with depth» | Lo mismo, de comprobación |
| Repuestos | (sin fuente) | Caja de transporte de 118 L con piezas y espuma: 400 kg/m³, 40 kg | `repuestos` y `caja_repuestos` |

## 9. Lo que falta

- **La ficha del escáner en el juego** sigue enseñando lo que el recipiente traía al construirse
  (`ShipKind::describe`, `PartKindDef::contents`: «170 L de agua»). Lo que queda ahora y lo que
  pesa lo dan `lunar_ship::cargo::scan(kind, s, cat, part)` (pieza de una nave) y
  `Structure::cargo_card(cat)` (un bulto suelto): «85 L de agua · 50 % · 97 kg». Son dos líneas en
  `crates/app/src/gear.rs` (`Gear::read`), que no es de este cambio.
- **Fotos:** `tools/camara/iman_bidones.jsonc` (la grúa sobre los dos bidones de la bahía C1, el
  rótulo de un bidón, la pantalla con la carga). Está escrito sin poder ejecutarlo: hay que mirar
  que los dos bidones suben pegados a la placa y que la cuarta línea de la pantalla cabe.
- Los **articulaciones** no notan el peso de lo que cuelga del imán (el izado sube igual con 180 kg
  que con 2 500): `Ship::update_joint_loads` solo cuenta piezas propias.
- El remolcador no enseña `iman.masa` en su consola (no tiene pantalla); la señal está.
- La energía de `estalla` es la del recipiente tal como se construyó; en marcha solo distingue
  «queda algo» de «vacío» (el suceso `Event::Burst` no lleva cantidad).
- Las redes de las estructuras que no son naves (`structure/networks.rs`, `store`) guardan propelente
  y energía sin masa: no se han unido a esto.
