# Cámaras, imágenes y puestos

Una cámara produce una vista, una superficie presenta una imagen y un puesto ofrece mandos.
Son tres contratos independientes. El núcleo no conoce naves, torretas, asientos ni nombres de
objetos. Una cámara de retrovisor o de vigilancia registra otro proveedor; un filtro de imagen
registra otra receta. La Selene es una aplicación de estos contratos.

## Uso del puesto de la Selene

La consola **ARTILLERÍA · CÁMARA** está delante del asiento derecho del copiloto, encima de su
MFD. Solo se enciende con un jugador vivo sentado allí y con AVIÓNICA alimentada. No basta estar
de pie al lado. ON alimenta la torreta desde ARMAMENTO; también existe TORRETA en el panel de
energía. El estado y el motivo de una negativa aparecen en la pantalla y en el HUD del artillero.

| Mando | Comportamiento |
|---|---|
| APUNTAR / G | Centra la vista del casco en el centro de la pantalla. El ratón dirige la torreta sin mover la cabeza. G devuelve el control de la mirada para pulsar otros mandos. |
| DISPARO / T | Dispara desde el montaje. T funciona mirando cualquier parte de la cabina; mantenla para disparar a su cadencia. En APUNTAR también sirve clic izquierdo. |
| FIJAR | Mantiene la puntería sobre el punto del mundo bajo la cruz de la cámara. Otra pulsación lo libera. No sigue objetos móviles ni guía los misiles. |
| ZOOM | 1× → 1,5× → 3× → 1×; el ratón se hace más preciso al ampliar. |
| CENTRO | Libera el punto fijado y devuelve los montajes al centro del recorrido. |
| Espacio | Se levanta, libera el foco de mirada y apaga el puesto si no queda otro jugador ocupándolo. |

La imagen es 384×216 a 12 Hz, con VHS suave. El texto y la cruz tienen textura independiente para
conservar legibilidad. Un minimisil ligero hace 30 de daño máximo al traje y 22 al casco, con
caída por distancia; su cráter tiene radio 1,1 m y desnivel máximo 0,45 m. La soldadora y el
lanzacohetes de mano siguen sus propios catálogos.

## Mapa de módulos

| Archivo | Responsabilidad |
|---|---|
| `shared/screens.ts` | Datos de fuente, resolución, frecuencia, alcance, escalones ópticos y recetas de imagen. `ConsoleCommand` identifica espacio de nombres, destino y acción. Límites de captura y proyección óptica pura. |
| `client/render/cameraSources.ts` | Registro de proveedores parametrizado por cualquier contexto; falla ante tipos desconocidos o registros repetidos. |
| `client/render/pip.ts` | Superficie PiP, estado visual, visibilidad, turnos, texturas y captura. No decide permisos, energía, objetivos ni daño. |
| `client/render/imageEffects.ts` | Registro de efectos y `ImageMaterial`, utilizable en cualquier malla con textura. Composición del shader una sola vez; uniformes reutilizados. |
| `client/render/worldCapture.ts` | Adaptador del mundo: cielo, interiores, terreno y restauración de visibilidad. El visor principal puede ocultar el exterior mediante portales y el secundario seguir viéndolo. |
| `client/ship/cameraScreens.ts` | Colocación desde `ScreenDef`; puerta común de asiento, energía e integridad; proveedor de cámara de montaje. |
| `client/ship/commands.ts` | Enrutador genérico `ConsoleCommands<Host>`; no depende de `ShipClient` ni del simulador. |
| `client/ship/gunnery.ts` | Permisos del asiento, intención de puntería, giro predictivo, mandos, zoom, punto fijado, diagnóstico y disparo. |
| `client/player/cameraRig.ts` | `focus: Object3D \| null`: mirada fijada en un objeto cualquiera, en espacio de render. No conoce pantallas. |
| `shared/ship/modules/weapons.ts` | Autoridad de giro, cargador, energía y daño; razón compartida de indisponibilidad. |

## Contrato de una fuente

`PipSource` ofrece `camera`, `enabled`, `live`, `status` y `prepare()`, con `update()` y `dispose()`
opcionales. `enabled=false` apaga imagen y texto; `enabled=true, live=false` permite mostrar el
diagnóstico sobre negro. `prepare()` solo se llama cuando hay una captura autorizada por el
planificador, y coloca la cámara respetando el origen flotante. El proveedor no renderiza.

```ts
const sources = new CameraSources<MyContext>();
sources.register('rear-view', context => new RearViewCamera(context));
const source = sources.create('rear-view', context);
const display = new PipDisplay(0.4, 0.225, {
  source: { kind: 'rear-view', ref: 'rear' },
  width: 256, height: 144, fps: 10,
  effects: [],
});
display.source = source;
pip.add(display);
```

En una nave el proveedor se envuelve en `ShipCameraFeed`. Esa envoltura comprueba el asiento
declarado, el circuito y el soporte; un proveedor nuevo hereda la regla automáticamente. Omitir
`seat` permite una cámara de servicio sin artillero. Fuera de una nave otro adaptador decide su
activación, usando exactamente el mismo `PipDisplay` y `PipSystem`.

`ScreenDef.camera` reemplaza las páginas MFD. El constructor de MFD ignora esas pantallas y el
constructor de cámaras ignora las demás. Los botones del puesto son `ControlDef.command` y
se enrutan por espacio de nombres; no escriben falsas variables de interruptor en `ShipSim`.
Una acción nueva de artillero registra `gunnery.registerAction(id, handler)` y añade un mando en
datos. Disparar mantiene las comprobaciones autoritativas de asiento, servicio, boca y cadencia.

## Efectos y calidad

Las recetas no están ligadas al proveedor, a una pantalla ni a un anfitrión. `ImageMaterial`
extiende el material básico de Three conservando su muestreo y conversión de color. Los efectos
modifican UV antes de leer la textura y color después. VHS usa una sola lectura de textura,
ligero temblor horizontal, desaturación, líneas y grano. La imagen apagada anula el resultado
final: el grano nunca enciende una pantalla vacía. El texto se dibuja aparte.

```ts
defineImageEffect('monochrome', {
  color: amount => `diffuseColor.rgb = mix(diffuseColor.rgb,
    vec3(dot(diffuseColor.rgb, vec3(0.2126,0.7152,0.0722))), ${amount});`,
});
// En datos: effects: [{ kind: 'monochrome', amount: 1 }]
```

Los efectos se componen al construir el material y tienen clave de programa propia. Si una
receta declara variables GLSL locales debe usar su índice para evitar colisiones. Para una
futura mejora de cámara o monitor cambian los datos de resolución, óptica o efectos y se
reconstruye su fuente/material mediante el adaptador; no se añaden condiciones por modelo al
renderizador. Todavía no hay catálogo comercial, compras, vídeo emitido por red ni seguimiento
de blancos móviles. La óptica y efectos locales no se replican al visor de otro jugador.

## Presupuesto y ciclo de vida

- Una captura global como máximo por frame y cuatro destinos retenidos como máximo. Si cuatro
  puestos visibles mantienen destinos, un quinto espera hasta que se libera uno. Los destinos
  de fuentes sin servicio o invisibles durante 5 s se reciclan. No crecen sin límite.
- Por vista: 64..384 × 36..216 píxeles, 1..15 Hz y alcance 1..16 m, con valores predeterminados
  256×144, 12 Hz y 10 m. El adaptador no puede saltarse los límites de `cameraBudget`.
- Solo capturan superficies visibles, delante del observador, dentro de su frustum y cercanas.
  Una pantalla apagada no captura ni pide trabajo a su proveedor.
- Todas las superficies PiP se ocultan durante una captura para evitar recursión. No hay cadena
  de postprocesado secundaria ni actualización de mapas de sombras. VHS se evalúa en la malla.
- La captura queda dentro de la medición GPU de `RenderPipeline`; F3 incluye `camera-feeds`.
- `SphereTerrain.update(camera, false)` selecciona únicamente nodos construidos. No crea hijos,
  envía trabajos a workers, cambia prioridades ni expulsa caché. Después se restaura el visor
  principal, sus uniformes y sus nodos. Las cámaras lejanas usan lo que esté disponible.
- `CaptureScope.end()` y el estado de renderizador se restauran en `finally`, incluso ante un
  error: destino, viewport, scissor, autoClear, tone mapping, sombras y visibilidad.
- Cámaras, texturas y geometrías se liberan al destruir el adaptador/sistema. El núcleo reutiliza
  vectores, matrices, listas y uniformes; no crea un segundo mundo ni una burbuja física.

## Verificación sin juego

`npm run test:cameras` comprueba permisos y apagado, autoridad de disparo, cámara tras mover el
origen, foco al centro, zoom, fijación, cadencia, efectos, presupuestos, turnos, visibilidad,
ausencia de recursión y restauración ante error. Usa renderer/canvas simulados, sin WebGL ni
navegador: verifica contratos, no la apariencia ni los FPS reales. Completan la verificación
`npm run typecheck`, `npm run test:ship`, `npm run test:equipment` y `npm run build`.

La consola modifica índices de mandos del catálogo. El protocolo 15 impide mezclar servidores
y clientes anteriores: reiniciar servidor y recargar clientes cuando se instalen estos cambios.
