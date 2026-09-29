# Sonido

Todo el sonido del juego se sintetiza con números (sin ficheros de audio) y sale de donde se produce:
cada fuente tiene un sitio en el mundo y lo que la transporta hasta el oído (el aire de una sala, la
estructura de una nave, el suelo, el propio traje). Código en `src/client/audio/`.

## 1. Piezas

```
                  quién suena                                   cómo llega                 qué suena
 módulos de nave ── sounds(): SoundCue[] ──┐
 (shared/ship/modules)                      ├─► ShipSounds ──┐
 reglas de cualquier nave (mandos, chorros, ┘   (por nave)   │
 paneles, máquinas, toma de contacto)                        ├─► sfx (engine.ts) ── medium.ts ──► bank.ts + sounds/
 astronautas (pisadas, jetpack, herramientas) ─ CrewSounds ──┤     voces HRTF          aire / casco /    recetas de
 casco (respiración, avisos) ────────────────── Helmet ──────┤     en la posición      suelo / traje     síntesis (dsp.ts)
 sucesos (explosiones, cohetes, clics…) ── director.placeAt ─┘     real de la fuente
```

| Fichero | Qué |
|---|---|
| `engine.ts` | `sfx`: el contexto Web Audio. `play(id, sitio)` (una vez), `loop(id)` (continuo: su dueño pone `level`, `pitch` y `place` cada frame), `ui(id)` (dentro del casco), `stun(k)` (ensordecimiento). Límite de voces, reverberación de cabina, limitador |
| `medium.ts` | El modelo físico: `Place` (dónde está y qué lo lleva), `Listener` (el oído), `hear()` (por qué caminos llega, con qué volumen y filtro). No sabe nada de naves: pregunta a `Acoustics` |
| `bank.ts` | El banco: `defineSound(id, receta)`. Se sintetiza por trozos al arrancar |
| `dsp.ts` | Kit de síntesis: ruidos, filtros (fijos y barridos), modos resonantes (metal, plástico, cristal), tonos, chasquidos, envolventes, bucles sin costura |
| `sounds/*.ts` | La biblioteca: mandos, máquinas, aire, impactos, tripulación, alarmas |
| `shipSounds.ts` | Una nave cualquiera: toca las `SoundCue` de sus módulos y las reglas comunes; responde por el aire de sus salas |
| `crewSounds.ts` | Un astronauta (el local o uno remoto: solo cambia el sitio) y el casco |
| `surfaces.ts` | Qué suena bajo la bota según el material |
| `director.ts` | El lado del juego: llena el oído cada frame (sistema `audio`, orden 95) y sitúa acústicamente cualquier punto del mundo |

## 2. Cómo llega un sonido (`medium.ts`)

El sonido necesita algo por donde viajar. Cada sonido tiene un **sitio** (`Place`: punto del mundo, nave
cuya estructura lo lleva, sala cuyo aire lo lleva, acoplamiento al suelo, si es del propio traje) y el
oyente dice en qué está y qué toca. `hear()` suma los caminos (en potencia) y mezcla sus filtros:

| Camino | Cuándo | Cómo suena |
|---|---|---|
| aire | fuente y oído en aire (sala presurizada, o un cuerpo con atmósfera), con puertas / escotillas / rampa / brechas abiertas entre ellos | lleno; más fino a baja presión; se apaga con la distancia; reverberación de la sala |
| estructura | la fuente está en la nave en la que estás de pie, sentado o respirando su aire | sordo, grave (≈500 Hz), desde donde está la máquina |
| suelo | la fuente toca el suelo (una explosión, una nave posada, un chorro a poca altura) y tú también | un golpe sordo que muere en decenas de metros; llega con retraso |
| traje | tus botas, tu jetpack, tus herramientas | siempre; amortiguado en el vacío, abierto en aire |
| casco | respiración, avisos, radio | directo, sin dirección |

En el vacío y sin tocar nada no se oye nada. El menú de pausa tiene **SONIDO EN EL VACÍO**: *físico* (así)
o *amortiguado* (todo pasa como un retumbo apagado). Volumen y modo se guardan en el navegador.

La dirección es la real: cada voz tiene un panner HRTF en la posición de su fuente, relativa al oyente
(las coordenadas de la Luna, de millones de metros, no cuentan). Los sitios de una nave se guardan en
espacio de nave (`Place.local`): el sonido sigue a la nave mientras suena.

## 3. Hacer que algo suene

**Una máquina nueva** (un módulo): declara sus sonidos igual que sus alarmas.

```ts
sounds(): SoundCue[] {
  const i = this.iT;
  return [
    { sound: 'mach.hum', role: 'run', part: this.part, level: (st) => (st[i] > 0 ? 1 : 0), pitch: (st) => 0.8 + 0.4 * st[i] },
    { sound: 'relay.clack', role: 'start', part: this.part, on: (st) => st[i] > 0 },     // una vez, al pasar a cierto
  ];
}
```

- `level` → un bucle tan fuerte como devuelva (0..1); `pitch` → su velocidad (arranque, rpm).
- `on` → un golpe cada vez que pasa a cierto (nunca en la primera lectura ni al despertar la nave).
- `motion` → un bucle mientras un valor cambia (el recorrido de una puerta), fuerte según su velocidad.
- Sitio: `at` (punto de la nave, o una función si se mueve) → la pieza (`part`) → la sala (`zone`) → el centro de la nave.
  La sala de una pieza es su `zone`; `zone: null` es fuera del casco.
- Una pieza destruida calla. Una pieza más grande suena más grave (talla del catálogo).
- Solo variables replicadas (las de cuanto negativo no llegan al cliente).

El núcleo junta todo en `ShipSystems.soundCues()` y añade la **alarma general** (un altavoz en el techo
de cada sala: aviso rojo o precaución ámbar mientras la alarma está enclavada; reconocerla la calla).
Las cargas conmutadas con máquina (`LoadDef.part`) suenan solas mientras están encendidas (`SwitchedLoads`,
una `mach.hum` genérica que el módulo propio de esa máquina sustituye si declara el mismo `role`).

**Otra voz para un componente**: `sounds: { run: 'mach.pump' }` en el componente del catálogo (o en la
pieza). Sustituye el sonido de ese papel sin tocar el módulo. Un mecanismo: `MoverDef.sounds`.

**Lo que hace cualquier nave** no se declara (`shipSounds.ts`): el clic de un mando según su tipo
(`CONTROL_SOUNDS`: un tipo nuevo es una línea), el zumbador de una negativa, el chorro de cada abertura
con gas pasando (`airflow`), paneles que crujen bajo presión, golpeados, reventados, soldados, máquinas
golpeadas y destruidas, la toma de contacto. Los mecanismos (puertas, rampa, tren, persianas, actuadores)
los declara `movers.ts` según lo que mueven (`MOVER_SOUNDS`).

**Un suceso del juego**: `sfx.play('id', sitio)`. El sitio de un punto del mundo lo da
`director.placeAt(p)` (sala de nave, casco, suelo); el de un punto de una nave, `ship.sounds.placeOf(local)`
o directamente `ship.sounds.playAt('id', local)`. Dentro del casco: `sfx.ui('id')`.

**Un material de suelo**: una entrada en `SURFACES` (`surfaces.ts`) y `BodyDef.ground` en el cuerpo.

**Un sonido nuevo**: `defineSound('familia.nombre', { make: (s) => …, loop?, variants?, gain, ref, range })`
en el fichero de su familia (`sounds/`). `gain` es el volumen en la fuente, `ref` su tamaño (a esa distancia
suena entero; luego −6 dB por cada doble), `range` hasta dónde se toca. `like: 'otro', pitch` reutiliza
otro a otra altura. Un id que nadie definió no suena y avisa una vez en la consola.

## 4. Reglas

- Nada de audio en el bucle principal: el sistema `audio` (frame, orden 95) llena el oído, despierta las
  naves cercanas (lejos duermen: sin bucles ni flancos) y actualiza el motor.
- En caminos calientes nada de basura: los `Place` y los `Loop` se reutilizan; `hear()` no crea objetos.
- Las recetas se sintetizan una vez; los bucles son exactos (tonos ajustados a su duración con `fit`,
  ruido fundido en la costura con `loopOf`).
- F3 muestra las voces sonando (golpes + bucles / fuentes) y el estado del contexto.
