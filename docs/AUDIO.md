# Sonido

Todo el sonido del juego se sintetiza con números (sin ficheros de audio) y sale de donde se produce:
cada fuente tiene un sitio en el mundo y lo que la transporta hasta el oído (el aire de un espacio, la
estructura de lo que la sostiene, el suelo, el propio traje). Código en `src/client/audio/`.

El sistema no conoce naves, bases ni cuerpos por su nombre. Tres capas, cada una se entera de lo mínimo:

1. **El medio** (`medium.ts`, `engine.ts`, `bank.ts`, `dsp.ts`): sitios, oyente, caminos. Solo números: un
   anfitrión es un id, sus espacios son índices.
2. **La acústica del mundo** (`acoustics.ts`): anfitriones acústicos registrados (cualquier cosa con
   espacios de aire y estructura propia) y el entorno del cuerpo (su atmósfera, su viento, su suelo).
3. **Adaptadores** que registran cosas concretas: `shipSounds.ts` (una nave), `crewSounds.ts` (un
   astronauta), los catálogos (`shared/items`: armas, proyectiles, objetos con sus ids de sonido).

Es la misma idea que los marcos (`shared/frames`): un `FrameHost` es cualquier cosa con espacio interior
(hoy una nave) por la que se mueven cosas; un `AcousticHost` es cualquier cosa con aire y estructura por la
que viaja el sonido. Una base, un róver o una estación nuevos son un adaptador más; no se toca el resto.

## 1. Piezas

```
                  quién suena                                    cómo llega                  qué suena
 cualquier estado ── sounds(): SoundCue[] ──► CuePlayer ──┐
 (shared/sound.ts; hoy los módulos de nave)   (cues.ts)   │
 anfitriones: nave (shipSounds.ts), mañana base… ─────────┤
   = AcousticHost en acoustics.ts: aire, caminos, apoyo   ├─► sfx (engine.ts) ── medium.ts ──► bank.ts + sounds/
 entorno del cuerpo: viento donde hay aire ───────────────┤   voces HRTF en la    aire / estructura /  recetas (dsp.ts)
 astronautas (pisadas, jetpack, herramientas) ────────────┤   posición real       suelo / traje
 sucesos (explosiones, proyectiles, clics…) ── locate() ──┘
```

| Fichero | Qué |
|---|---|
| `shared/sound.ts` | `SoundCue`: cómo suena algo con estado, declarado por quien lo tiene (hoy los módulos de nave, con `sounds()`) |
| `engine.ts` | `sfx`: el contexto Web Audio. `play(id, sitio)` (una vez), `loop(id)` (continuo: su dueño pone `level`, `pitch` y `place` cada frame), `ui(id)` (dentro del casco), `stun(k)`. Límite de voces, reverberación, limitador |
| `medium.ts` | El modelo físico: `Place`, `Listener`, `hear()`. Pregunta a `Acoustics` |
| `acoustics.ts` | Registro de anfitriones (`acousticHosts`), `AirWays` (caminos de aire entre espacios, genérico), el aire de fuera según el cuerpo (`outsideAir`), `locate` / `feetAt` (dónde está acústicamente un punto) y `Environment` (el viento) |
| `cues.ts` | `CuePlayer`: toca las `SoundCue` de cualquier anfitrión; dónde suena cada una lo dice el anfitrión |
| `shipSounds.ts` | Adaptador de nave: sus compartimentos como espacios, sus aberturas y paneles como caminos, sus cues, y lo que solo tienen las naves (mandos, chorros, paneles, toma de contacto) |
| `crewSounds.ts` | Un astronauta (el local o uno remoto: solo cambia el sitio) y los avisos del casco |
| `surfaces.ts` | Qué suena bajo la bota según el material |
| `director.ts` | El lado del juego: llena el oído, despierta anfitriones, entorno (sistema `audio`, orden 95) |
| `bank.ts`, `dsp.ts`, `sounds/*.ts` | El banco (`defineSound`), el kit de síntesis y la biblioteca |

## 2. Cómo llega un sonido (`medium.ts`)

Cada sonido tiene un **sitio** (`Place`: punto del mundo, anfitrión cuya estructura lo lleva, espacio cuyo
aire lo lleva, acoplamiento al suelo, si es del propio traje) y el oyente dice en qué está y qué toca.
`hear()` suma los caminos (en potencia) y mezcla sus filtros:

| Camino | Cuándo | Cómo suena |
|---|---|---|
| aire | fuente y oído en aire (espacio presurizado, o un cuerpo con atmósfera), con caminos abiertos entre ellos | lleno; más fino a baja presión; se apaga con la distancia; reverberación |
| estructura | la fuente está en el anfitrión que pisas, en el que vas sentado o cuyo aire respiras | sordo, grave, tanto como el sonido hace vibrar la estructura (`body` del sonido) |
| suelo | la fuente toca el suelo (una explosión, algo posado, un chorro a poca altura) y tú también | un golpe sordo que muere en decenas de metros; llega con retraso |
| traje | tus botas, tu jetpack, tus herramientas | siempre; amortiguado en el vacío, abierto en aire |
| casco | avisos, radio | directo, sin dirección |

En el vacío y sin tocar nada no se oye nada. El menú de pausa tiene **SONIDO → EN EL VACÍO**: *físico* (así)
o *amortiguado* (todo pasa como un retumbo apagado). Volumen y modo se guardan en el navegador.

**El entorno** sale del cuerpo en el que estás (`BodyDef`): `atmosphereDensity` (aire de fuera, más fino con
la altura según `scaleHeight`), `wind` (viento: suena alrededor del oyente, al aire libre, así que dentro de
un anfitrión cerrado el medio no lo deja pasar) y `ground` (material del suelo). La Luna: sin aire, sin
viento, regolito.

La dirección es la real: cada voz tiene un panner HRTF en la posición de su fuente, relativa al oyente.
Los sitios de un anfitrión se guardan en su espacio (`Place.local`): el sonido le sigue mientras suena.

## 3. Hacer que algo suene

**Una máquina nueva** (un módulo de nave, o el estado de cualquier anfitrión): declara sus sonidos igual
que sus alarmas.

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
- `on` → un golpe cada vez que pasa a cierto (nunca en la primera lectura ni al despertar el anfitrión).
- `motion` → un bucle mientras un valor cambia (el recorrido de una puerta), fuerte según su velocidad.
- Sitio: `at` (punto del anfitrión, o una función si se mueve) → la pieza (`part`) → su espacio (`zone`) →
  el centro. `zone: null` es fuera del anfitrión.
- Una pieza destruida calla. Una pieza más grande suena más grave (talla del catálogo).
- Solo estado que llega al cliente (las variables de cuanto negativo no llegan).

En una nave, el núcleo junta todo en `ShipSystems.soundCues()` y añade la **alarma general** (un altavoz en
el techo de cada sala mientras la alarma está enclavada; reconocerla la calla). Las cargas conmutadas con
máquina suenan solas (`SwitchedLoads`, una `mach.hum` genérica que el módulo de esa máquina sustituye si
declara el mismo `role`). Otra voz para un componente: `sounds: { run: 'mach.pump' }` en el catálogo; un
mecanismo: `MoverDef.sounds`.

**Un anfitrión nuevo** (una base, un róver): un adaptador que implemente `AcousticHost` (centro y radio,
de mundo a su espacio y vuelta, qué espacio hay en un punto, el aire de cada espacio, lo abierto que está el
camino desde el oyente —`AirWays` lo resuelve con sus pasos—, su apoyo en el suelo, `update`) y se registre
con `acousticHosts.add(this)`. Para sus sonidos declarados, un `CuePlayer`.

**Un cuerpo nuevo**: sus datos (`atmosphereDensity`, `scaleHeight`, `wind`, `ground`). Nada más.

**Un suceso del juego**: `sfx.play('id', sitio)`. El sitio de un punto del mundo lo da `locate(p, …)`
(`director.placeAt`); el de un punto de una nave, `ship.sounds.placeOf(local)` o `ship.sounds.playAt('id', local)`.
Dentro del casco: `sfx.ui('id')`. Los catálogos llevan sus ids: arma (`sounds.fire`), proyectil
(`sounds.flight`, `sounds.impact`), objeto (`sounds.grab`, `drop`, `throw`).

**Un material de suelo**: una entrada en `SURFACES` (`surfaces.ts`) y `BodyDef.ground`.

**Un tipo de mando**: una línea en `CONTROL_SOUNDS` (`shipSounds.ts`).

**Un sonido nuevo**: `defineSound('familia.nombre', { make: (s) => …, loop?, variants?, gain, ref, range, body })`
en el fichero de su familia (`sounds/`). `gain`: volumen en la fuente; `ref`: su tamaño (a esa distancia
suena entero; luego −6 dB por cada doble); `range`: hasta dónde se toca; `body`: cuánto hace vibrar la
estructura en la que está (un motor o una explosión 1, un ventilador casi nada, un altavoz 0). `like: 'otro',
pitch` reutiliza otro a otra altura. Un id que nadie definió no suena y avisa una vez en la consola.

## 4. Reglas

- Nada de audio en el bucle principal: el sistema `audio` (frame, orden 95) llena el oído, despierta los
  anfitriones cercanos (lejos duermen: sin bucles ni flancos), suena el entorno y actualiza el motor.
- Nada de lechos de ruido continuos de ambiente (respiración, «aire» de ventiladores, fluido de un
  reactor): se oyen como viento. Las máquinas en marcha suenan tonales; el aire suena solo cuando de verdad
  se mueve (una fuga, un venteo, una brecha, el viento de un cuerpo con atmósfera).
- En caminos calientes nada de basura: los `Place` y los `Loop` se reutilizan; `hear()` no crea objetos.
- Las recetas se sintetizan una vez; los bucles son exactos (tonos ajustados con `fit`, ruido fundido con `loopOf`).
- F3 muestra las voces sonando (golpes + bucles / fuentes) y el estado del contexto.
