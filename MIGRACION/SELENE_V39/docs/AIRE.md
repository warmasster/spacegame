# Aire: pasarlo de un compartimento a otro y recuperarlo

Dos cosas, para cualquier nave que las declare en sus datos (hoy el Alcotán y, por salir de él, el
Cachalote; el Abejorro no tiene cabina):

1. **Válvulas de mano en los mamparos** para pasar aire de un compartimento a otro, o de uno al
   vacío: un volante que se gira con la rueda del ratón y un manómetro de la diferencia de presión
   a su lado. No es una palanquita: se abre lo que se gire.
2. **Un compresor** para sacar el aire de un compartimento sin tirarlo: lo mete en un **depósito de
   aire recuperado** (o en otro compartimento), y de ahí vuelve cuando se pide.

Código: `crates/machines/src/gas.rs` y `models/air.rs` (el gas, el compresor, el depósito),
`crates/ship/src/airworks.rs` (de los datos a piezas, aberturas y paneles), `transfer.rs` (mover
el gas sin crear ni perder un mol), `atmos.rs` (válvulas que abren a medias), `controls/mech/wheel.rs`
(el volante). Pruebas: `crates/ship/tests/trasvase.rs`.

## Cómo funciona

### Válvulas de mano (`trasvase.valvulas`)

- Cada válvula es un componente en su pared (`valvula_igualacion` entre dos compartimentos,
  `valvula_venteo` hacia el vacío): el cuerpo dentro de la pared y una **placa** a cada lado que sea
  un compartimento, con su **volante** y su **manómetro de ΔP**. No necesitan corriente (como las
  de la ISS y las de Gateway: mecánicas, se accionan desde los dos lados).
- Los volantes de los dos lados son **un solo mecanismo** (`bind.comun`): girar uno gira el otro.
- La abertura es **analógica**: la señal `<id>.apertura` va de 0 a 1 y el aire pasa por un orificio
  de esa parte del paso de la válvula, con la misma ecuación de orificio compresible que las
  brechas y las puertas (`gas::orifice_mdot`: se ahoga por debajo de 0,528 de relación de
  presiones). Va del lado con más presión al otro y lleva su composición (O₂, N₂, CO₂) y su
  temperatura. Cerrada, no existe: ni pasa aire ni se calcula nada.
- Donde sale el aire (`boca`) se ve y se siente el chorro como en cualquier abertura (`atmos::Vent`)
  y se oye un **silbido** (`sonido.silbido`, según el caudal).
- Una válvula abierta al vacío, por poco que sea, deja el compartimento **no estanco**: la
  represurización y el compresor no meten aire en él mientras tanto.
- El panel de cada compartimento ya no tiene el interruptor IGUALAR de las puertas que tienen
  válvula de mano: enseña lo abierta que está (`VÁLVULA`, en %).

### Compresor (`compresor`) y depósito (`deposito_aire`)

Un **lugar** es cualquier cosa que diga su presión como `<nombre>.p` y si está cerrada como
`<nombre>.estanco`: un compartimento, o un depósito (que además dice cuánto aguanta,
`<nombre>.p_max`). El compresor tiene una lista de lugares (`lugares`) y dos selectores: de cuál
saca (ORIGEN) y en cuál mete (DESTINO).

Es un compresor de pistones de varias etapas con refrigeración intermedia:

- **Lo que barre** su primera etapa (`caudal`, en la admisión) por la densidad del gas ahí: a
  menos presión en el origen, menos gas por carrera. Y menos todavía por el gas que queda en el
  espacio muerto de cada cilindro y se vuelve a expandir: `ηv = 1 − c·(r^(1/n) − 1)` por etapa
  (`espacio_muerto`, `etapas`).
- **Lo que le da el motor**: el trabajo isotermo de comprimir, `R·T·ln(r)` por mol, entre su
  rendimiento isotermo (`eficiencia`), sin pasar de `potencia`. Con el depósito ya a presión es
  esto lo que manda: por eso va cada vez más despacio.
- **Presión mínima** (`presion_minima`, 5 kPa): por debajo sus válvulas de admisión ya no abren.
  **No llega al vacío**: lo que queda se ventea a mano por la válvula de venteo.
- **Paso libre**: si el origen tiene más presión de la que admite su entrada (`admision_max`: es
  el depósito), el aire baja solo por su línea de retorno (un orificio de paso `paso`) y **el
  motor no gira**. Así se devuelve el aire del depósito a un compartimento.
- **Límites del destino**: un depósito se llena hasta su `p_max`; un compartimento, hasta
  `presion_max` (1 atm). No manda aire a un compartimento que no esté estanco.
- **Se para solo** (motor quieto, gasta 40 W de sus válvulas) cuando el origen llega a su límite
  o el destino se llena, y sigue si deja de estarlo.
- Todo lo que consume acaba como **calor** en el compartimento donde está (la bodega llega a
  56 °C tras media hora con el compresor a plena potencia, y se enfría después).
- Necesita **corriente**: su disyuntor (`brk.compresor`, 650 A, bus A). Con poca energía es de lo
  primero que se queda sin ella (prioridad 70).

`<compresor>.estado`: 0 parado · 1 en marcha · 2 paso libre · 3 origen en su límite · 4 destino
lleno · 5 sin energía · 6 destino abierto (no estanco) · 7 origen y destino iguales.

El **depósito** guarda lo que le meten, cada gas por sus moles (lo recuperado es aire de cabina:
O₂ y N₂ mezclados, no un gas puro). Dice lo mismo que un compartimento (`p`, `o2`, `n2`, `co2`,
`estanco`), su masa, su nivel y su `p_max`. Dañado pierde aire hacia el compartimento donde está;
destruido lo suelta todo de golpe (y si estaba a más de 1 MPa, revienta: `rupture`).

### Lo que se decidió y por qué

- **Depósito propio y no las botellas.** Las botellas son de O₂ y de N₂ puros (la nave las cuenta por kilos de un
  solo gas) y de ellas salen los inyectores con sus consignas: meterles aire mezclado las
  contaminaría (y la red de gas no lleva composición). El depósito de aire recuperado guarda la mezcla tal cual y la devuelve tal
  cual. Es lo que propone la NASA para no perder el gas de las esclusas (un depósito evacuado al
  que va el gas de la despresurización).
- **Devolver el aire por el mismo equipo, sin motor.** El aire del depósito está a mucha más
  presión que el compartimento: baja solo. Hacer girar el compresor «al revés» no tiene sentido
  físico; lo que hay es su línea de retorno (paso libre) con los mismos selectores: ORIGEN el
  depósito, DESTINO el compartimento. El motor solo trabaja cuesta arriba.
- **El tiempo es el de verdad.** Recuperar el aire de una bodega de 58 m³ son unos 18 MJ de
  trabajo (7,5 kWh de batería con el rendimiento del compresor): con 15 kW, media hora larga. La
  esclusa de la ISS (5,5 m³) tarda una hora con una bomba de 1,5 kW. Quien tenga prisa puede
  mandar el aire a la cabina (5 minutos, hasta 1 atm) o ventearlo (40 s, perdiéndolo). Para que
  vaya más deprisa basta subir `potencia` y `caudal` del compresor en
  `assets/defs/components/naves.jsonc` (30 kW y 240 L/s: unos 17 min; 60 kW y 480 L/s: unos 9; la energía es la misma, 7,6 kWh).

## Datos

En la nave (`assets/defs/ships/<nave>.jsonc`):

```jsonc
"trasvase": {
  // válvulas de mano: dónde atraviesan la pared y hacia dónde va la normal (del primero de
  // "entre" al segundo). "vacio" (solo el segundo) es el espacio: placa solo por dentro
  "valvulas": [
    { "id": "vi_bodega", "entre": ["bodega", "cabina"], "en": [-1.5, 0.93, -3.4], "normal": [0, 0, 1] },
    { "id": "vv_bodega", "entre": ["bodega", "vacio"], "en": [1.72, 1.5, -9.3], "normal": [0, 0, -1] }
  ],
  // el panel de cada compresor (se hace solo con lo que alcanza): dónde va y de dónde toma luz
  "compresores": [
    { "id": "compresor_aire", "panel": { "en": [1.938, 1.55, -7.9], "normal": [-1, 0, 0], "energia": "elec:c_soporte" } }
  ]
},
// y, entre sus componentes, el compresor (con su circuito y los lugares que alcanza) y el depósito
{ "id": "compresor_aire", "tipo": "compresor_aire", "en": [1.5, 0, -7.9],
  "maquina": { "puertos": { "motor": "elec:c_compresor" }, "params": { "lugares": ["deposito_aire", "bodega", "cabina", "puente"] } } },
{ "id": "deposito_aire", "tipo": "deposito_aire", "en": [1.55, 0, -4.4] },
```

Opcionales de una válvula: `nombre`, `paso` (su diámetro: «70 mm» entre compartimentos y «100 mm»
al vacío si no se dice) y `tipo` (otro componente con piezas `placa_a` / `placa_b` y ancla `boca`).

Parámetros de las máquinas (`assets/defs/components/naves.jsonc`):

| Máquina | Clave | Por defecto | Qué es |
|---|---|---|---|
| `compresor` | `caudal` | 120 L/s | lo que barre la primera etapa, en la admisión (a 1 atm: 144 g/s) |
| | `potencia` | 15 kW | motor |
| | `eficiencia` | 0,65 | rendimiento isotermo (trabajo isotermo / eléctrico) |
| | `etapas`, `espacio_muerto` | 4, 0,06 | rendimiento volumétrico y relación máxima |
| | `presion_minima` | 5 kPa | presión por debajo de la cual no saca nada |
| | `admision_max` | 150 kPa | por encima, el origen pasa por el paso libre |
| | `presion_max` | 101 kPa | lo más que mete en un compartimento |
| | `paso` | 10 mm | diámetro del paso libre (retorno) |
| | `potencia_reposo` | 40 W | sus válvulas, con el motor parado |
| | `lugares` | — | compartimentos y depósitos que alcanza, en el orden de los selectores |
| `deposito_aire` | `volumen` | 0,42 m³ | |
| | `presion_max` | 12 MPa | lleno: 2 060 mol, unos 60 kg de aire |
| | `presion_inicial`, `o2` | 0, 0,3 | con qué empieza (vacío) |

Otras claves nuevas: `aberturas[].en` y `.normal` (dónde suelta el gas una abertura de un
compartimento; su señal puede valer entre 0 y 1), el mando `"kind": "volante"` (una rueda con
maneras de válvula: de 0 a 100 % en tres vueltas, un 5 % por muesca, 25 % con Mayús, 1 % con Ctrl)
y `"bind": { "comun": true }` (varios mandos, un mecanismo).

Señales (en unidades SI; los paneles las enseñan en kPa, MPa y g/s): `<valvula>.apertura` (0..1),
`<valvula>.dp` (Pa); `<compresor>.estado`, `.caudal` (kg/s), `.p_origen`, `.p_destino` (Pa),
`.potencia` (W), `.giro` (0..1), y sus órdenes `.marcha`, `.origen`, `.destino`;
`<deposito>.p`, `.o2`, `.n2`, `.co2` (Pa), `.masa` (kg), `.nivel` (0..1), `.p_max` (Pa), `.estanco`;
`sonido.compresor`, `sonido.silbido` (0..1).

Las pantallas: la página automática **AIRE** (`mfd_auto.rs`) enseña, además del plano de
presiones, el depósito (nivel y presión), el compresor (qué hace, de dónde a dónde, caudal) y lo
abierta que está cada válvula, en cualquier nave que los tenga.

## Para el manual: qué hace el jugador

**Igualar dos compartimentos (por ejemplo cabina y puente, o cabina y bodega):**

1. Ve al mamparo, junto a la puerta: a media altura hay una placa con un volante amarillo y un
   manómetro (ΔP). Está a los dos lados.
2. Apunta al volante y gira la **rueda del ratón** hacia arriba para abrir (Mayús: un cuarto de
   golpe; Ctrl: fino). Se oye pasar el aire.
3. Mira el ΔP: cuando entra en verde (menos de 5 kPa) la puerta ya abre. Con la válvula del todo
   abierta, cabina y bodega se igualan en minuto y medio.
4. Cierra la válvula (rueda hacia abajo).

**Abrir la rampa sin tirar el aire de la bodega:**

1. En la bodega, pared de babor, sobre el compresor: panel **COMPRESOR DE AIRE**. ORIGEN en
   `BODEGA`, DESTINO en `DEPÓS.` (así empiezan).
2. Levanta la tapa y pulsa **MARCHA**. La aguja ORIGEN baja y la del DEPÓSITO sube; el CAUDAL va
   cayendo según baja la presión. Tarda algo más de media hora (el 60 % sale en el primer cuarto de
   hora: se puede parar cuando se quiera y ventear el resto).
3. Cuando se enciende **LÍMITE** (unos 5 kPa) el compresor se para solo: no saca más. Pulsa MARCHA
   para apagarlo.
4. Ve a la popa, a babor de la rampa: la placa con el **volante rojo** («AL VACÍO»). Ábrelo: lo
   que queda sale fuera. En 20 s el manómetro entra en verde (menos de 3 kPa).
5. Baja la rampa. Cierra el volante rojo antes de volver a dar presión.

**Volver a llenar la bodega con lo recuperado:**

1. Rampa cerrada y trabada, volante rojo cerrado (la lámpara ESTANCO del panel de la bodega en
   verde).
2. En el panel del compresor: ORIGEN en `DEPÓS.`, DESTINO en `BODEGA`. Pulsa MARCHA.
3. **EN MARCHA** parpadea: el aire baja solo del depósito, sin motor (el último resto, por debajo
   de 150 kPa, lo saca el motor en unos segundos: lámpara fija). En 3 minutos la bodega está
   a 66 kPa (vuelve el 93 % del aire que tenía). Se para solo (LÍMITE: depósito vacío). Pulsa
   MARCHA.
4. Lo que falta hasta 70 kPa lo pone REPRES. del panel de la bodega, desde las botellas.

**Con prisa:** DESTINO en `CABINA`. En 5 minutos la cabina sube a 1 atm (LLENO) y la bodega baja
a 40 kPa; el resto, al depósito o al vacío. Para devolverlo, abre la válvula del mamparo
bodega–cabina. O ventéalo todo con VENTEO (tapa precintada): 40 s y se pierde.

Lámparas del panel: **EN MARCHA** (fija: comprimiendo; parpadea: paso libre), **LÍMITE** (el
origen no da más), **LLENO** (depósito a 12 MPa o compartimento a 1 atm), **ABIERTO** (el
compartimento de destino no está estanco), **SIN ENERGÍA** (mira el disyuntor COMPR. del panel de
disyuntores y la tensión del bus A).

## Números (medidos por las pruebas, Alcotán)

| Qué | Cuánto |
|---|---|
| Válvula bodega–cabina (70 mm), bodega vacía y cabina a 70 kPa: igualadas a menos de 1 kPa | 86 s abierta del todo; 353 s al 25 % |
| Compresor, bodega (58 m³) de 70 kPa a su límite, al depósito | **33,9 min** hasta 5,1 kPa; 7,55 kWh; pico 15,0 kW; depósito a 8,99 MPa |
| … por tramos | 60 kPa a los 4,4 min · 50 a los 8,1 · 40 a los 12,2 · 30 a los 16,7 · 20 a los 21,7 · 10 a los 27,7 · 7 a los 30,7 |
| Caudal | 99 g/s al arrancar (depósito vacío, 1,3 kW), 42 g/s con 70 kPa → 1 MPa (ya a 15 kW), 20 g/s con 20 kPa → 6,5 MPa, 8 g/s con 7 kPa → 8,7 MPa |
| Aire recuperado | **93,0 %** del de la bodega (el 7,0 % se ventea) |
| Válvula de venteo (100 mm): de 5,1 a 3 kPa | 22 s; un minuto después, 1 kPa |
| Del depósito a la bodega | 3,0 min (2,8 de ellos sin motor), 0,013 kWh; bodega a 65,8 kPa con 19,9 kPa de O₂; vuelve el 99,9 % de lo recuperado |
| De la bodega a la cabina (hasta 1 atm) | 4,7 min y 0,42 kWh; la cabina a 101,0 kPa y la bodega a 40,5 kPa. De vuelta por la válvula del mamparo: las dos a 69,8 kPa a los 4 min |
| Puente (20 m³) al depósito, primer minuto | de 70,0 a 56,4 kPa |
| Depósito | 0,42 m³, 12 MPa: 60 kg de aire (una bodega y cuarto) |
| Baterías tras vaciar la bodega | del 85 y 80 % al 46 y 41 % |

En el Cachalote la bodega es de 700 m³: el mismo equipo llena el depósito con el primer 10 % de su
aire. Allí sirve para cabina y puente; su bodega se trabaja al vacío.

## Paneles e identificadores (para fotos y guiones)

| Panel (`"panel"`) | Dónde, Alcotán | Dónde, Cachalote | Mandos |
|---|---|---|---|
| `vi_bodega_a` | mamparo bodega–cabina, lado bodega, estribor (−1,5; 0,93; −3,48) | (−1,5; 0,93; 0,53) | `vi_bodega_a/volante`, indicador `dp` |
| `vi_bodega_b` | el mismo, lado cabina (z −3,33) | (z 0,68) | `vi_bodega_b/volante`, `dp` |
| `vi_puente_a` | mamparo cabina–puente, lado cabina, babor (1,5; 0,93; 2,52) | (z 6,53) | `vi_puente_a/volante`, `dp` |
| `vi_puente_b` | el mismo, lado puente (z 2,67) | (z 6,67) | `vi_puente_b/volante`, `dp` |
| `vv_bodega_a` | mamparo de popa por dentro, a babor de la rampa (1,72; 1,5; −9,23) | (3,55; 1,5; −21,92) | `vv_bodega_a/volante`, `dp` |
| `panel_compresor_aire` | bodega, pared de babor sobre el compresor (1,91; 1,55; −7,9) | (4,01; 1,55; −8,5) | `origen`, `destino`, `tapa_marcha`, `marcha`; lámparas `l_marcha`, `l_limite`, `l_lleno`, `l_abierto`, `l_energia`; `p_origen`, `p_deposito`, `p_destino`, `caudal` |
| `disyuntores` | (ya estaba) | | nuevo: `disyuntores/compresor` |
| `sala_<compartimento>` | (ya estaban) | | nuevo: `valvula_<puerta>` (lo abierta que está) en lugar de `igualar_<puerta>` |

Piezas: `compresor_aire.cuerpo` / `.etapas` / `.motor` (Alcotán: bodega, babor, z −7,9),
`deposito_aire.cuerpo` / `.cuna` (bodega, babor, z −4,4, tumbado bajo el panel de la bodega),
`vi_*`, `vi_*.placa_a`, `vi_*.placa_b`, `vv_bodega`, `vv_bodega.placa_a`.

## Lo que cuesta

- Una válvula cerrada es una lectura de señal por tic (como los interruptores de igualar que
  sustituye); no crea abertura ni cálculo de flujo. El silbido solo se escribe cuando cambia.
- El compresor parado sale de `plan` en la primera línea y no escribe nada; el depósito solo
  escribe sus señales cuando cambia lo que tiene. `Transfers::step` recorre dos listas de un
  elemento.
- Lo único que se añade por tic es lo de cualquier panel: 19 indicadores y 6 mandos más (de 79 a 98
  y de 137 a 143 en el Alcotán). Para compensarlo, las agujas en reposo y los displays cuya
  lectura no cambia ya no se recalculan (`controls/indicator.rs`), en ninguna nave.
- Una nave en reposo sigue yendo a ritmo lento: nada de esto la marca como ocupada
  (`trasvase.rs`: `an_idle_ship_does_none_of_this_work`, con contadores). En marcha, el compresor
  la mantiene a todo ritmo, como cualquier cosa que mueva aire.

## Pruebas (`crates/ship/tests/trasvase.rs`, y las de `machines` y `controls`)

- No se crea ni se pierde gas: los moles de O₂, N₂ y CO₂ entre compartimentos y depósito son los
  mismos tras cada maniobra (válvulas, compresor, retorno, depósito roto).
- Una válvula iguala los dos lados, más deprisa cuanto más abierta; cerrada no pasa nada; los dos
  volantes son uno.
- El compresor lleva la bodega a su límite, cada 10 kPa más despacio que los anteriores; no saca
  un mol más aunque se le deje; se para sin corriente (disyuntor, bus), con el depósito lleno, con
  el destino abierto, y no arranca de un sitio a sí mismo.
- Del depósito a la bodega vuelve lo recuperado.
- De la bodega a la cabina se para a 1 atm, y la válvula del mamparo lo devuelve.
- Un depósito destruido suelta su aire en el compartimento donde está, y no se le manda más.
- Una nave en reposo no hace nada de esto, y las dos naves cargan con todo y pasan `naves.rs`.

## Investigación

| Qué | De dónde | Qué se tomó | Dónde |
|---|---|---|---|
| Recuperar el aire de una esclusa con una bomba | [NASA, *EVA Airlocks and Alternative Ingress/Egress Methods* (EVA-EXP-0031, 2018)](https://www.lpi.usra.edu/lunar/artemis/Mary-2018-EVA%20Airlocks-And-Alternative-Ingress-Egress-EVA-EXP-0031.pdf) | La esclusa Quest de la ISS (5,5 m³) devuelve «la mayor parte» de su aire a la estación con una bomba de 1,5 kW en dos ciclos de 30 min, y aun así pierde 1 lbm (0,45 kg) por salida: el resto se ventea. De ahí: una bomba que no llega al vacío (queda del orden de 1 psi), que tarda, y una válvula para ventear lo último. Nuestra presión mínima, 5 kPa. | `machines/models/air.rs` (`presion_minima`), `valvula_venteo` |
| Guardar el gas de la esclusa en un depósito | [Trevino y Lafuse, *Minimizing EVA Airlock Time and Depress Gas Losses* (NASA, 2008)](https://ntrs.nasa.gov/citations/20080013418) | El método de la ISS cuesta unos 45 min y 1 kW; proponen guardar el gas de la despresurización en un depósito y devolverlo después. Es el depósito de aire recuperado y la idea de devolverlo sin bomba. | `models/air.rs` (`Tank`, paso libre) |
| Por qué despresurizar solo una parte | [Howard, *A Multi-Functional, Two-Chamber Airlock Node…* (NASA, 2021)](https://ntrs.nasa.gov/api/citations/20210020897/downloads/A%20Multi%20Functional%20Two%20Chamber%20Airlock%20Node%20for%20a%20Common%20Habitat%20Architecture.pdf) | En Quest solo se vacía la cámara pequeña para perder menos gas en cada salida: vaciar un volumen grande es caro. Por eso una bodega de 58 m³ tarda lo que tarda. | números de arriba |
| Válvulas de igualación de las escotillas | [Aurora Flight Sciences, válvulas de Gateway (2025)](https://www.aurora.aero/2025/06/24/aurora-delivers-critical-components-for-nasas-gateway-lunar-space-station/) | La MPEV iguala la presión entre dos módulos a mano, con mandos **a los dos lados**, y es mecánica: sin software ni corriente. Nuestras válvulas: un volante a cada lado, el mismo eje, sin energía. | `ship/airworks.rs`, `panels.rs` (`bind.comun`) |
| Bomba y válvulas de igualación juntas | [D. Darling, *Quest Joint Airlock*](https://www.daviddarling.info/encyclopedia/Q/Quest_Joint_Airlock.html) | «A combination of the depress pump and pressure equalization valves located within the hatches» hace la despresurización y la presurización de la esclusa: las dos cosas que se pedían. | diseño general |
| Presiones de cabina y depósitos de gas | [Wikipedia, *Quest Joint Airlock*](https://en.wikipedia.org/wiki/Quest_Joint_Airlock) | 10,2 psi = 70 kPa (la presión de cabina del juego) en el *camp-out*; O₂ y N₂ en depósitos de alta presión separados (no se mezclan: de ahí el depósito aparte). | `deposito_aire` |
| Curva de vaciado de una bomba | [Leybold, *How to calculate pump-down time*](https://www.leybold.com/en-us/knowledge/vacuum-fundamentals/vacuum-generation/calculating-pump-down-time) | `t = V/S · ln(p₁/p₂)` con velocidad de bombeo constante; con presión última `p_ult`, la relación es `(p₁ − p_ult)/(p₂ − p_ult)`; la velocidad cae cerca de la presión última. Es la forma de la curva del compresor al final (la última décima sobre su mínimo). | `Compressor::pumping` |
| Rendimiento volumétrico de un compresor de pistón | [Midstream Calculator, *Recip volumetric efficiency*](https://midstreamcalculator.com/engineering/compressors/recip-volumetric-efficiency-fundamentals.html) | `ηv = 1 − c·(r^(1/k) − 1)`; el caudal se anula a `r_max = (1 + 1/c)^k`; espacios muertos del 8–15 %. | `Compressor::pumping`, `ultimate` |
| Trabajo y rendimiento isotermo | [MechCodex, *Reciprocating compressor*](https://mechcodex.com/learn/thermodynamics/reciprocating-compressor) | Trabajo isotermo `R·T·ln(r)` (el mínimo); rendimiento isotermo 0,65–0,80 en máquinas bien hechas; etapas con refrigeración intermedia. Usamos 0,65. | `Compressor::pumping` |
| Un compresor de aire respirable real | [Bauer, *Mariner 320*, hoja técnica](https://bauer-spareparts.com/media/products/AD_791_Mariner%20320_en%202020.pdf) | 320 L/min llenando botellas a 200 bar con 7,5 kW, 4 etapas, 154 kg, 1,3 × 0,65 × 0,7 m (rendimiento isotermo real: 0,3–0,4). El nuestro es el doble de motor en algo menos de sitio y con mejor rendimiento: optimista, no imposible. | `compresor_aire` (tamaño, 15 kW, 4 etapas) |

## Sin hacer o sin comprobar

- No se ha visto en el juego (solo pruebas y vistas de los modelos): mirar en una foto que el
  volante y la aguja caen bien sobre la placa de cada válvula, que los rótulos de la placa
  («IGUALACIÓN DE PRESIÓN», «VENTEO AL VACÍO») no pisan los del panel, que el depósito no estorba
  el panel de la bodega y que el compresor no tapa el suyo; y oír el compresor y el silbido.
- Las tuberías del compresor a cada compartimento no se dibujan (sería una red más, resuelta cada
  tic en todas las naves).
- El calor del compresor va al aire de su compartimento, no al circuito de refrigerante.
- En red: los mandos nuevos son mandos como los demás; el contenido del depósito viaja con
  `Machine::save/load`.
