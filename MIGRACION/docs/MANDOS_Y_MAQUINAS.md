# Mandos físicos, máquinas simuladas, paneles y generador (definición)

Estado: **definición, sin implementar**. Pendiente del visto bueno de Fernando.

## 0. En una frase

Tres crates propios y sin dependencias externas encadenan entrada, mandos, señales, máquinas e indicadores.
- `lunar-signals`: señales reactivas con unidades físicas.
- `lunar-controls`: mecanismos de los mandos y el framework de paneles.
- `lunar-machines`: modelos físicos de motores, reactores, baterías, etc.

Todo se declara en `.jsonc`, se monta sobre las piezas de las estructuras modulares y escala con el LOD. Con una tecla se generan estructuras nuevas en el mundo desde un catálogo o con variantes por semilla.

## 1. Crates y módulos

| Crate | Depende de | Qué tiene | Qué NO tiene |
|---|---|---|---|
| `lunar-signals` | std (+ `serde` opcional para datos) | unidades SI, parser `"45 kN"`, almacén de señales, expresiones derivadas, propagación reactiva | render, input, física |
| `lunar-controls` | `lunar-signals` | mecanismos de mandos (máquinas de estados continuas), indicadores, paneles (modelo + **layout**), bindings | render, input, estructuras |
| `lunar-machines` | `lunar-signals` | trait `Machine`, modelos físicos (motor cohete, reactor, batería, depósito presurizado, válvula, bomba, radiador, panel solar, disyuntor, soporte vital…), `settle` para el LOD | render, input |
| `lunar-core` (existente) | los tres | estado por estructura (señales, mandos, máquinas viven en `Structure`/`Part`), redes en SI, roturas → fallos | — |
| `lunar-render` | core | `render::panels`: modelos por tipo de mando con poses, serigrafía (atlas SDF propio), lámparas, agujas, pantallas, LOD de panel | — |
| `lunar-app` | todos | `input::controls`: rayo, alcance, intents con modificadores, HUD; `spawner` | — |

Regla: `lunar-controls` no sabe qué es una nave ni qué es un píxel; recibe `Intent`s y produce `Pose`s, valores y eventos.

## 2. Flujo reactivo (orden por tick)

```
input (frame)  ──Intent──►  Mechanism ──valor SI──► señal de mando (cmd)
                                                        │
tick de simulación (20–50 Hz si activa):                ▼
  1. máquinas leen cmd + concesiones de red ──► integran su física (subpasos propios)
  2. máquinas escriben telemetría (empuje, T, P, SoC…) y demandas de red
  3. redes reparten (eléctrica W, propelente kg/s, refrigerante kg/s+K) ──► concesiones
  4. señales derivadas (expresiones) recalculan solo lo sucio, en orden topológico
  5. indicadores leen señales ──► estado visual (lámpara, aguja con inercia, texto)
```

- Cada señal tiene **un único escritor** (mando, máquina o derivada), validado al cargar. Si varios mandos mandan sobre la misma orden, la orden declara la regla de fusión (`ultimo`, `max`, `suma`).
- No hay asignaciones por tick: los valores van en un vector denso, las banderas de sucio en un bitset y las versiones en contadores.
- Los vetos de máquina (enclavamientos) vuelven al mecanismo como `Gate::veto`. Por ejemplo, la palanca de arranque no pasa a START si la presión de alimentación es menor que 1,2 × Pc; el HUD dice por qué.

## 3. `lunar-signals`

### Unidades
SI interno en `f64`. Los datos aceptan cadenas con unidad: `"45 kN"`, `"311 s"`, `"6.5 MPa"`, `"600 °C"`, `"120 kW"`, `"2.5 kg/s"`, `"12 kWh"`, `"28 V"`, `"3000 rpm"`, `"35 %"`. Cada señal declara su `unidad` y su rango; cada indicador declara la unidad y los decimales con que se muestra.

```rust
pub struct Unit { pub dims: [i8; 7], pub scale: f64, pub offset: f64 } // m kg s A K mol cd
pub fn parse(q: &str) -> Result<(f64, Unit), UnitError>;               // "6.5 MPa" → (6.5e6, Pa)
```

### Almacén y derivadas
```rust
pub struct SignalId(u32);
pub struct Store { values: Vec<f64>, meta: Vec<Meta>, dirty: BitSet, version: Vec<u32> }
pub struct Meta { name: Box<str>, unit: Unit, range: (f64, f64), quality: Quality, writer: Writer }
pub enum Quality { Ok, Stale, Failed }       // Failed: su fuente se rompió (pieza destruida)
pub trait Derived { fn eval(&self, s: &Store) -> f64; fn inputs(&self) -> &[SignalId]; }
```

Las derivadas se escriben en un lenguaje de expresiones mínimo, compilado a bytecode propio (nada externo):
- aritmética y comparaciones: `+ - * / < > <= >= == && || !`;
- funciones: `min`, `max`, `clamp`, `abs`, `lerp`, `if(c, a, b)`;
- con estado: `lag(x, τ)` (primer orden), `rate(x)`, `blink(hz)`, `latch(set, reset)`, `edge(x)`, `hold(x, s)`.

```jsonc
"derivadas": {
  "alarma.presion_baja": "deposito_ox.p < 1.5 MPa && motor_1.encendido",
  "alarma.general":      "latch(alarma.presion_baja || reactor.scram, panel.reconocer)"
}
```

## 4. `lunar-controls`: mecanismos

### Contrato
```rust
pub enum Intent {
    Press, Release, Hold { secs: f32 },
    Turn { notches: f32, rate: f32, m: Mods },   // rueda del ratón / mando: muescas y velocidad de giro
    Drag { delta: f32, m: Mods },                // arrastre (palancas, ruedas grandes)
    Type(char), Hover(bool),
}
pub struct Mods { pub coarse: bool /* Shift */, pub fine: bool /* Ctrl */ }
pub struct Gate { pub supply: f32, pub working: bool, pub role: Role, pub veto: Option<Veto> }

pub trait Mechanism: Send + Sync {
    fn init(&self, d: &ControlDef) -> ControlState;
    fn intent(&self, d: &ControlDef, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome; // cambios + eventos
    fn advance(&self, d: &ControlDef, st: &mut ControlState, dt: f32) -> bool;              // inercia, muelles, parpadeo
    fn value(&self, d: &ControlDef, st: &ControlState) -> f64;                              // SI, lo que escribe
    fn pose(&self, d: &ControlDef, st: &ControlState, out: &mut Pose);                       // giro/desplazamiento por elemento
    fn reachable(&self, d: &ControlDef, st: &ControlState) -> bool;                          // tapas, llaves
    fn describe(&self, d: &ControlDef, st: &ControlState, out: &mut String);                 // "Empuje 62,4 % (fino)"
}
```
`ControlState` son datos planos (posición, velocidad angular, retén, búfer…), así que se guarda, se clona y viaja con la pieza.

### Tipos

| Tipo (`kind`) | Estados / parámetros | Notas |
|---|---|---|
| `pulsador` | `momentaneo`, `enclavado`, `seta` (empujar para parar, girar para soltar), `iluminado` | escribe 0/1, o un pulso de un tick (`pulso: true`) |
| `interruptor` | 2 o 3 posiciones, `muelle` por posición (p. ej. ARRANQUE momentáneo, ON, OFF) | `tirar_para_mover`: la posición protegida pide `Hold` antes de pasar |
| `selector` | N retenes con etiquetas, `vuelta: tope/libre`, `tirar_para_girar` en ciertas posiciones | valor = índice o valores declarados |
| `rueda` | **continua**: rango, resolución, pasos grueso/fino, inercia, aceleración, topes, retenes, curva, vueltas | ver 4.1 |
| `volante` | una `rueda` con maneras de válvula de mano: 0–100 %, 5 % por muesca (25 % con Mayús, 1 % con Ctrl), tres vueltas, topes duros | se dibuja como un volante con su husillo; con `"bind": { "comun": true }` es el mismo mecanismo a los dos lados de un mamparo (ver `AIRE.md`) |
| `palanca` | 1 o 2 ejes, recorrido continuo, retenes (ralentí, MIL…), `compuerta` (hay que levantar para pasar), `friccion` o `muelle_al_centro` | aceleradores, joystick, palanca de mezcla |
| `tapa` | cerrada/abierta, protege a otros mandos (`protege: [ids]`), `precinto` opcional | el mando protegido no es `reachable` con la tapa cerrada |
| `llave` | necesita un objeto o rol, 2-3 posiciones | permisos |
| `teclado` | dígitos, punto, CLR, ENTER; `mascara`, `unidad`, `destino` (señal que recibe al ENTER) | con pantalla de 7 segmentos asociada |
| `disyuntor` | manual ON/OFF + disparo automático por corriente I²t (`nominal`, `curva`) | saltado → hay que rearmar a mano |
| `lampara` | reglas → color/parpadeo (`verde` si…, `ambar_parpadeo` si…) | consume W de la red |
| `aguja` | escala, zonas (verde/ámbar/roja), **inercia de aguja** (masa-muelle), unidad mostrada | sin energía cae a cero |
| `barra`, `display7`, `contador` | valor → segmentos/dígitos, decimales, unidad | |
| `pantalla` | páginas (texto + datos de señales), botones de bisel (`bisel` es otro mando) | refresco 10 Hz |
| `anunciador` | matriz de avisos con enclavamiento + "reconocer" (master caution) | |

### 4.1 Rueda continua

- **Valor:** continuo en `[min, max]` con la `resolucion` propia de esa rueda; el valor interno es `f64` y sin cuantizar fuera de los retenes.
- **Paso de una muesca o tecla:**
  - normal: `paso`;
  - Shift: `paso_grueso` (por defecto ×10);
  - Ctrl: `paso_fino` (por defecto ÷10, nunca por debajo de la resolución).
  - Ejemplo: 0,1 °C normal, 1 °C con Shift, 0,01 °C con Ctrl.
- **Aceleración:** el paso se multiplica según la velocidad de giro (muescas/s): `mult = 1 + k·max(0, v − v0)^γ`, con tope en `acel_max`.
- **Inercia:** la rueda tiene velocidad angular ω que decae con `amortiguamiento`. Un giro rápido sigue girando (volante de mando) hasta pararse, o hasta un tope o un retén.
- **Topes:** `topes: "duros"` recorta y suena "tope"; `"libre"` da vueltas sin fin (encoder).
- **Retenes:** lista de valores con `fuerza` y `ancho de captura`. Cerca de uno el valor se imanta y suena "clic". Con Ctrl (fino) se ignoran.
- **Curva:** `lineal` o `log`. En `log`, la resolución es relativa al valor, para ganancias y frecuencias.
- **Ángulo:** `recorrido` en grados, o `vueltas` (p. ej. un potenciómetro de 10 vueltas, 3600°). `pose` gira el pomo; la escala serigrafiada se genera con las marcas.
- **Pulsar para restablecer** (`pulsar: "reset"`) vuelve al valor por defecto; `pulsar: "grueso"` cambia el modo de paso.
- **HUD:** valor con su unidad y decimales derivados de la resolución, más el modo actual.

```jsonc
{
  "id": "consigna_T", "kind": "rueda", "nombre": "Consigna de temperatura del refrigerante",
  "rango": ["250 °C", "650 °C"], "resolucion": "0.01 °C", "defecto": "420 °C",
  "paso": "0.5 °C", "paso_grueso": "5 °C", "paso_fino": "0.05 °C",
  "aceleracion": { "desde": 4.0, "k": 0.6, "gamma": 1.5, "max": 20.0 },   // muescas/s
  "inercia": { "momento": 0.002, "amortiguamiento": 6.0 },
  "topes": "duros", "retenes": [ { "en": "420 °C", "fuerza": 0.6, "ancho": "1.5 °C" } ],
  "curva": "lineal", "recorrido": 300.0, "pulsar": "reset",
  "bind": { "senal": "reactor.consigna_T" },
  "requiere": { "energia": "5 W" }, "sonido": { "reten": "clic_fino", "tope": "tope_pomo" }
}
```

## 5. `lunar-machines`: modelos físicos

### Contrato
```rust
pub struct Port { pub red: NetworkId, pub kind: PortKind }          // Electrico, Fluido{medio}, Calor, Datos
pub struct Io<'a> { pub signals: &'a mut Store, pub grants: &'a [f64], pub demands: &'a mut [f64], pub env: &'a Env }
pub trait Machine: Send + Sync {
    fn ports(&self) -> &[Port];
    fn step(&mut self, io: &mut Io, dt: f64);       // tiempo real (activa), con subpasos internos si es rígida
    fn settle(&mut self, io: &mut Io, dt_long: f64); // LOD grueso/dormido: puesta al día analítica o por régimen
    fn fail(&mut self, damage: f32, rng: u64);       // la pieza dañada: modos de fallo
    fn state(&self) -> &[f64]; fn load(&mut self, s: &[f64]); // estado plano (viaja con la pieza)
}
```
Cada modelo declara qué señales lee (órdenes) y escribe (telemetría), con unidades. Los parámetros están en los datos de la pieza (`module.params`), con unidades.

### 5.1 Motor cohete (`motor_cohete`)

- **Empuje:** `F = ṁ·vₑ + (Pe − Pa)·Ae`, con `vₑ = Isp·g0`.
  - El Isp interpola entre vacío y nivel del mar con la presión ambiente (en la Luna, Pa ≈ 0).
  - El caudal nominal es `ṁ₀ = F_vac / (Isp_vac·g0)`.
- **Estrangulamiento:**
  - `ṁ = t·ṁ₀`, con `t ∈ [t_min, t_max]` (p. ej. 0,4–1,05).
  - Por debajo de `t_min` hay inestabilidad de combustión: se veta, o se apaga si persiste.
  - La presión de cámara es `Pc = Pc₀·t`.
  - El Isp baja con t según `isp_curva`.
- **Alimentación:** necesita `P_alim ≥ 1,2·Pc`, leída de la red de propelente (depósitos presurizados).
  - Sin presión suficiente, el caudal real se limita y el empuje cae.
  - La mezcla O/F reparte el caudal entre la red de oxidante y la de combustible.
- **Arranque**, una secuencia con transitorios:
  1. ARMADO (tapa);
  2. PURGA (`t_purga`);
  3. IGNICIÓN (encendedor, W de la red);
  4. SUBIDA (`ṁ` sigue a la orden con `τ_subida`).
  - Fallo de encendido si falta presión o energía; los rearranques están limitados (`reencendidos`).
- **Apagado:** cola de empuje con `τ_bajada` (purga de las líneas).
- **Térmica:** pared de cámara con capacidad `C_pared` (J/K).
  - Calor de entrada: `q = h·Pc^0.8·A`.
  - Refrigeración regenerativa (∝ ṁ) más radiativa (`ε σ A T⁴`).
  - Por encima de `T_aviso` aparece una alarma; por encima de `T_corte` se apaga solo; por encima de `T_daño` daña la pieza.
- **Telemetría:**
  - empuje (N), ṁ (kg/s), Pc (Pa), T pared (K);
  - estado (APAGADO, PURGA, IGNICIÓN, SUBIDA, EN MARCHA, BAJADA, FALLO);
  - Isp (s) y reencendidos restantes.
- **Fallos por daño:** fuga (ṁ perdido que no da empuje), `t_max` reducido, encendedor roto.

```jsonc
"motor_principal": {
  "name": "Motor principal", "shape": { "kind": "cylinder", "radius": 0.9, "height": 1.6, "taper": 0.6 },
  "material": "acero", "hollow": 0.03, "ports": ["energia", "oxidante", "combustible"],
  "module": { "kind": "motor_cohete", "params": {
    "empuje_vacio": "45 kN", "isp_vacio": "311 s", "isp_mar": "265 s",
    "pc_nominal": "1.0 MPa", "mezcla": 1.6, "area_salida": "0.78 m2", "p_salida": "4 kPa",
    "estrangulamiento": [0.40, 1.05], "isp_curva": [[0.4, 0.96], [1.0, 1.0]],
    "t_purga": "0.6 s", "tau_subida": "0.8 s", "tau_bajada": "0.35 s", "reencendidos": 12,
    "encendedor": "150 W", "valvulas": "40 W",
    "pared": { "capacidad": "180 kJ/K", "h": 0.004, "area": "2.1 m2", "emisividad": 0.8,
               "t_aviso": "1100 K", "t_corte": "1250 K", "t_dano": "1400 K" },
    "ordenes": { "armado": "motor_1.armado", "arranque": "motor_1.arranque", "acelerador": "motor_1.acelerador" },
    "telemetria": "motor_1"
  } }
}
```

### 5.2 Reactor de fisión (`reactor`)

- **Neutrónica de un grupo de precursores:**
  - `dn/dt = (ρ − β)/Λ·n + λ·C`
  - `dC/dt = β/Λ·n − λ·C`
  - Reactividad `ρ = ρ_barras(pos) + α_T·(T_comb − T_ref)`, con coeficiente de temperatura negativo.
  - Es rígida, así que se integra implícitamente con subpasos.
- **Potencia térmica:** `P_th = n·P_nom`.
- **Calor residual tras el SCRAM** (Way–Wigner): `P_d = 0,066·P_th0·(t^−0,2 − (t+t_op)^−0,2)`.
- **Térmica:**
  - combustible `C_comb`, refrigerante `C_ref`;
  - conductancia combustible→refrigerante `UA`;
  - lazo `ṁ_ref·cp·(T_sal − T_ent)` con la bomba (W de la red);
  - radiador `ε σ A (T⁴ − T_amb⁴)` más carga solar.
- **Potencia eléctrica:** `P_e = η(T_caliente, T_fria)·P_th`, con un tope de Carnot y una fracción de él.
- **SCRAM:** las barras caen a 0 en `t_caida`. Salta por:
  - T combustible mayor que el límite;
  - periodo demasiado corto (dn/dt/n mayor que el límite);
  - pérdida de caudal;
  - mando manual (pulsador de seta con tapa).
  - Rearmar exige T < `T_rearme` y llave.
- **Telemetría:** P_th, P_e, n, ρ, T combustible, T de entrada/salida del refrigerante, posición de barras, estado.
- **Fallos:** bomba averiada, radiador con menos área por los paneles rotos (las piezas radiador están en la red de calor).

### 5.3 Batería (`bateria`)

- **Celdas:** `serie × paralelo`, capacidad Ah y tensión nominal.
- **Tensión:** `V = OCV(SoC, T) − I·R(SoC, T)`.
  - OCV por tabla, curva típica de Li-ion.
  - R crece con el frío y con un SoC bajo.
- **Corrientes:** C-rate máximo de carga y de descarga.
- **Carga:** recuento de culombios `dSoC = −I·dt/(3600·Ah)`, con eficiencia coulómbica.
- **Térmica:** `I²R` más la masa térmica de la pieza y su disipación.
  - Con frío baja la capacidad útil.
  - Corte por `V_min`, `T_max` o `T_min` de carga.
- **En la red eléctrica:** la red pasa a modelarse como un bus con tensión, y las baterías son fuentes con su curva. Las fuentes de potencia (reactor, solar) entran por despacho y prioridades, como en TS.
- **Telemetría:** SoC (%), V, I (A), T (K), estado.

### 5.4 El resto
- **Depósito presurizado:** volumen; masa de propelente; ullage de gas ideal (`P = nRT/V_gas`, en blowdown o con presurizante y regulador).
- **Válvula:** área efectiva, apertura con tiempo, `ṁ = Cd·A·√(2ρΔP)`.
- **Bomba:** curva de altura–caudal, W de la red.
- **Radiador:** `ε σ A T⁴`, factor de vista y sol.
- **Panel solar:** irradiancia × área × η(T), con MPPT.
- **Soporte vital:** O₂ y CO₂ por tripulante, W.
- **RCS:** pulsos mínimos.
- **Motor eléctrico de vehículo:** curva par–velocidad.
- **Disyuntor:** disparo por I²t.
- **Calefactor y termostato.**

Cada modelo lleva sus tests científicos (§10).

## 6. Paneles: framework y layout

### Modelo
- **Un panel** tiene `tamaño` (ancho × alto, m), `rejilla` (columnas × filas, o paso en mm), `margen` y `separación`, `fondo` (material, color) y `marco`.
- **Grupos:** rectángulos con marco serigrafiado y título. Se colocan fijos (`celda` + `ocupa`) o automáticos. Su `orden` es `filas`, `columnas`, `flujo` o `prioridad`, y llevan alineación.
- **Mandos:** cada tipo declara su huella (mm) y su holgura. Un mando puede ir `fijo` (celda o posición en mm) o `auto` dentro de su grupo, con `prioridad`.
- **Serigrafía:** rótulos de mando (arriba o abajo), escalas de pomos con marcas, leyendas de posición, títulos de grupo, fuente y tamaño. Se renderiza con un atlas SDF propio.

### Layout (determinista, en `lunar-controls::layout`)
1. Reserva lo fijo, tanto grupos como mandos, y detecta solapes, que son error.
2. Coloca los grupos automáticos en el hueco libre con un algoritmo de estanterías (*shelf*) por filas, en orden de prioridad.
3. Dentro de cada grupo coloca los mandos automáticos por filas o columnas, según la huella real de cada tipo, con su holgura, en orden de prioridad y alineados a la rejilla.
4. Coloca los rótulos y las marcas de escala sin que pisen otros mandos. Si no caben, los reduce o los mueve debajo.
5. Si algo no cabe, da un error de validación con la lista de lo que sobra y cuánto sitio falta.

La salida es un `PanelLayout`: transformaciones 2D más profundidad por mando, quads de texto y marcos. Render e input la consumen.

### Montaje
- **Por tipo de pieza:** `"paneles": [{ "panel": "cabina_lm", "en": [0, 1.1, 1.45], "rot": [0, 180, 0] }]`. El panel muere con la pieza.
- **Por estructura** (blueprint): `"paneles": [{ "panel": "control_motores", "pieza": "cabina", "cara": "+z", "prefijo": { "motor": "motor_2" } }]`.
- **Plantillas parametrizadas:** un mismo panel de motor sirve para varios motores. `prefijo` y `$motor` sustituyen los nombres de las señales.

### Panel completo de ejemplo
```jsonc
// structures/panels/cabina_lm.jsonc
{
  "name": "Cabina LM", "tamano": ["0.62 m", "0.40 m"], "rejilla": [12, 8], "margen": "12 mm", "separacion": "6 mm",
  "fondo": { "color": [52, 56, 60], "rugosidad": 200 },
  "grupos": [
    { "id": "motor", "titulo": "MOTOR PRINCIPAL", "orden": "filas", "celda": [0, 0], "ocupa": [6, 5] },
    { "id": "energia", "titulo": "ENERGÍA", "orden": "prioridad", "auto": true },
    { "id": "avisos", "titulo": "AVISOS", "orden": "flujo", "celda": [0, 6], "ocupa": [12, 2] }
  ],
  "mandos": [
    { "id": "tapa_armado", "kind": "tapa", "grupo": "motor", "protege": ["armado"] },
    { "id": "armado", "kind": "interruptor", "grupo": "motor", "posiciones": ["SEGURO", "ARMADO"],
      "bind": { "senal": "$motor.armado" }, "rotulo": "ARMADO" },
    { "id": "arranque", "kind": "interruptor", "grupo": "motor", "posiciones": ["PARO", "MARCHA", "ARRANQUE"],
      "muelle": { "ARRANQUE": "MARCHA" }, "tirar_para_mover": ["ARRANQUE"], "bind": { "senal": "$motor.arranque" } },
    { "id": "acelerador", "kind": "palanca", "grupo": "motor", "fijo": { "mm": [40, 120] },
      "rango": [0.0, 1.05], "retenes": [ { "en": 0.4, "nombre": "MÍN" }, { "en": 1.0, "nombre": "100 %" } ],
      "compuerta": { "en": 1.0, "levantar": true }, "friccion": 0.7, "unidad": "%",
      "bind": { "senal": "$motor.acelerador" } },
    { "id": "empuje", "kind": "aguja", "grupo": "motor", "senal": "$motor.empuje", "escala": ["0 kN", "50 kN"],
      "zonas": { "verde": ["18 kN", "45 kN"], "roja": ["47 kN", "50 kN"] }, "inercia": { "frecuencia": 3.0, "amortiguamiento": 0.7 } },
    { "id": "t_camara", "kind": "display7", "grupo": "motor", "senal": "$motor.t_pared", "unidad": "°C", "decimales": 0 },
    { "id": "bat", "kind": "disyuntor", "grupo": "energia", "prioridad": 1, "nominal": "40 A", "bind": { "union": "bateria-bus" } },
    { "id": "soc", "kind": "barra", "grupo": "energia", "prioridad": 2, "senal": "bateria.soc", "unidad": "%" },
    { "id": "consigna_T", "kind": "rueda", "grupo": "energia", "prioridad": 3, "rango": ["250 °C", "650 °C"],
      "resolucion": "0.01 °C", "paso": "0.5 °C", "paso_grueso": "5 °C", "bind": { "senal": "reactor.consigna_T" } },
    { "id": "aviso", "kind": "anunciador", "grupo": "avisos", "avisos": {
        "P ALIM": "alarma.presion_baja", "T CÁMARA": "$motor.t_pared > 1100 K", "SCRAM": "reactor.scram" },
      "reconocer": "panel.reconocer" }
  ],
  "derivadas": { "alarma.presion_baja": "deposito_ox.p < 1.5 MPa && $motor.encendido" }
}
```

## 7. Integración con estructuras, redes y LOD
- **Estado:**
  - por estructura, un `Store` de señales;
  - por pieza, `ControlState`s de sus mandos y `Machine::state` de su máquina.
  - Al partirse la estructura, cada pieza se lleva sus mandos y su máquina. Sus señales se reasignan al almacén de la estructura nueva por espacio de nombres, y en la vieja pasan a `Failed`.
- **Rotura:**
  - pieza destruida: los mandos desaparecen y sus señales pasan a `Failed`. Las agujas caen, las lámparas se apagan y el anunciador lo marca.
  - pieza dañada (no `working`): mando atascado y máquina en `fail(damage)`, que activa sus modos de fallo.
  - Los paneles rotos se ven agrietados con el mismo shader.
- **Redes:** pasan a unidades SI.
  - eléctrica: W, más tensión del bus con las baterías;
  - fluidos: kg/s con presiones, para propelente y refrigerante;
  - calor: W.
  - Los puertos de las máquinas son demandas y ofertas, y la red responde con concesiones.
  - Los disyuntores son mandos que abren o cierran uniones del grafo, o el puerto de una pieza.
  - Las lámparas y pantallas consumen W, así que sin energía no hay indicadores.
- **LOD de simulación:**
  - activa: máquinas a 20–50 Hz, con subpasos propios para las rígidas (reactor y transitorio de motor a unos 200 Hz internos);
  - gruesa: `settle` a 1 Hz, en régimen o con relajación exponencial;
  - dormida: congelada, y al despertar `settle(dt_dormido)`. El calor residual sigue la ley de Way–Wigner y la batería el recuento de culombios.
- **LOD visual de paneles:**
  - menos de 6 m: modelos 3D por tipo con pose, serigrafía nítida, agujas con inercia y pantallas a 10 Hz;
  - 6–25 m: panel como textura cocida más lámparas;
  - más de 25 m: solo lámparas como glow.
- **Input:** solo dentro del alcance (2,5 m) y mirando al mando.
  - El HUD muestra nombre, valor con unidad, modo (grueso o fino) y, si está bloqueado, el motivo ("Tapa cerrada", "Sin energía", "Veto: presión de alimentación 0,9 MPa < 1,2 MPa").
  - Las intenciones salen del ratón, la rueda (Shift grueso, Ctrl fino), el arrastre (palancas) y mantener (tirar para mover). El mando de juego entra por el mismo `InputMap`.
  - No hay teclas para operar: solo F (vuelo libre de depuración) y las teclas de prueba de armas siguen como atajos de desarrollo.

## 8. Generador de estructuras (spawner)
- **Tecla G:** abre un catálogo de blueprints y generadores (lista con búsqueda) y muestra delante una vista fantasma de lo elegido.
  - Va pegada al suelo; la rueda la gira, Shift+rueda cambia la distancia y Alt cambia la semilla de la variante.
  - Clic coloca; Shift+G repite lo último en otra posición.
- **Validación al colocar:**
  - huella libre (rejilla espacial) y pendiente máxima;
  - conectividad (grafo);
  - opcionalmente, una prueba de reposo físico de 2 s en segundo plano.
  - Si no pasa, el fantasma se ve rojo y el HUD dice el motivo.
- **Generadores** (`structures/generators/<id>.jsonc`): trait `Generator { fn generate(&self, seed: u64, p: &Params) -> StructureDef }`.
  - `variantes`: blueprint base con reglas (número de plantas, materiales, densidad de ventanas, espejo, escala de módulos).
  - procedurales: `torre`, `nave` (módulos a lo largo del casco, número de motores y tanques, paneles de cabina), `base` (anillo de módulos con túneles) y `granja_solar`.
  - Son deterministas por semilla.

```jsonc
// structures/generators/torres.jsonc
{ "name": "Torres", "kind": "variantes", "base": "torre",
  "parametros": { "plantas": [2, 6], "ventanas": [0.3, 1.0], "material": { "hormigon": ["hormigon", "regolito"] }, "espejo": true },
  "paneles": [ { "panel": "control_torre", "pieza": "forjado#0", "cara": "+y" } ] }
```

- Lo generado entra en la lista del mundo, con simulación, LOD, redes y mandos. El mundo puede guardarse en `world.jsonc`, como opción.
- **Extra opcional:** capturar una estructura del mundo a blueprint (tecla P), "tal cual" o "como se construyó".

## 9. Plan por pasos (sesiones aproximadas)
1. `lunar-signals`: unidades, parser, almacén, expresiones (bytecode), propagación y tests — **1**
2. `lunar-controls`: los mecanismos, incluida la rueda continua completa, con tests de cada máquina de estados — **1,5**
3. Paneles: modelo, layout, serigrafía como datos, validación y tests de layout con disposiciones esperadas — **1,5**
4. `lunar-machines`: contrato, motor, reactor, batería, depósito, válvula, bomba, radiador, solar, disyuntor y tests científicos — **2–3**
5. Integración: redes SI (bus eléctrico, fluidos con presión), estado por pieza y estructura, rotura y fallos, `settle` con el LOD — **1,5**
6. Input: picking de mandos, intents con modificadores, HUD de valores, motivos de bloqueo — **1**
7. Render de paneles: modelos por tipo con poses, atlas SDF de serigrafía, agujas, lámparas, pantallas y LOD de panel — **2**
8. Generador: catálogo, fantasma, validación y generadores (variantes, torre, nave, base) — **1,5**

**Total: unas 12–14 sesiones.** Cada paso con exe probable y tests en verde, como hasta ahora.

## 10. Tests científicos previstos
- **Motor:** el empuje cumple `ṁ·vₑ` y la Δv de un quemado cumple Tsiolkovsky (`Δv = Isp·g0·ln(m0/m1)`, 0,1 %). Por debajo de t_min se veta. Una alimentación por debajo de 1,2·Pc limita el empuje. Las constantes de tiempo de subida y bajada salen dentro del 5 %. La temperatura de pared converge al equilibrio regenerativo más radiativo.
- **Reactor:** sin realimentación y con ρ = 0, la potencia queda estable. Un escalón de reactividad positivo da el salto inmediato y luego el periodo esperado. El calor residual tras el SCRAM sigue Way–Wigner (5 %). El balance de energía cuadra (calor generado = cambio de entalpía + calor radiado, al 1 %).
- **Batería:** el recuento de culombios cierra. La tensión baja con la carga según OCV − I·R. Con frío da menos capacidad. Los cortes saltan en V_min y T_max.
- **Rueda:** los pasos normal, grueso y fino cumplen la resolución. La aceleración crece con la velocidad de giro. La inercia frena hasta pararse. Los retenes atraen dentro de su ancho y Ctrl los ignora. Los topes recortan.
- **Layout:** los paneles de ejemplo dan disposiciones estables, nada se solapa y un panel que no cabe da un error claro.
