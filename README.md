# SELENE I — exploración lunar cooperativa (prototipo FPV)

Prototipo mínimo jugable: dos astronautas con traje EVA sobre la superficie de la Luna,
en primera o tercera persona, conectados por red. Base técnica para el resto del juego
(nave caminable, sistemas, daños…) descrito en [`docs/DESIGN.md`](docs/DESIGN.md).

## Jugar

Requisitos: **Node.js 20 o superior** ([nodejs.org](https://nodejs.org), versión LTS) y un navegador
de escritorio actual (Chrome, Edge o Firefox) con aceleración gráfica activada.

```bash
npm install      # una sola vez
npm run play     # compila el cliente y arranca el servidor en el puerto 3000
```

Abre **http://localhost:3000**, escribe tu nombre y pulsa *Iniciar EVA*. Haz clic en la pantalla
para capturar el ratón.

### Que se una un amigo

El servidor admite **2 astronautas**. Quien arranca el servidor comparte una dirección y el otro
jugador solo necesita abrirla en su navegador (no instala nada).

**Misma red (casa, oficina, misma Wi‑Fi):** al arrancar, el servidor imprime la dirección de red,
por ejemplo `Red: http://192.168.1.34:3000`. Tu amigo abre esa dirección. En Windows, acepta el
aviso del cortafuegos para *Node.js* en redes privadas.

**Por internet:** deja `npm run play` funcionando y, en otra terminal, ejecuta:

```bash
npm run share
```

Aparecerá una dirección pública del tipo `https://algo-aleatorio.trycloudflare.com`: pásasela a tu
amigo. Es un túnel temporal y gratuito de Cloudflare (sin cuenta); deja de funcionar al cerrar la
terminal. Alternativas: redirigir el puerto TCP 3000 del router, o una VPN tipo Tailscale/ZeroTier.

### Controles

| Tecla | Acción |
|---|---|
| W A S D | moverse |
| Mayús | trote lunar |
| Espacio | saltar (≈1,3 m, 2,5 s en el aire) |
| C / Ctrl | agacharse |
| 1 / 2 | lanzacohetes / soldadora (otra vez: guardarla) · X sacar / guardar |
| Clic izq. | disparar · con la soldadora, mantener sobre un panel: repararlo |
| E (o clic) | accionar el mando que miras · sentarse / levantarse en un asiento |
| Clic der. (mantener) · Rueda | zoom (la rueda en 1ª persona; en 3ª, distancia) |
| L | luces del casco (el otro jugador las ve) |
| V | primera / tercera persona |
| Alt + ratón | mirar alrededor en tercera persona |
| Rueda | distancia de la cámara (3ª persona) |
| H | ocultar ayuda |
| Esc | liberar el ratón |

Si va lento, elige **Calidad: Baja** en la pantalla de inicio.

## Qué hay dentro

- **Nave de carga *Selene* (SLN-01)** aparcada junto al punto de aterrizaje, con la rampa bajada: cabina,
  pasillo de sistemas y bodega. **Cada pared, ventana, suelo y mamparo es un panel** con su integridad: los
  cohetes lo abollan y lo revientan (hueco real, se puede pasar), la **soldadora** lo reconstruye. **Cada
  botón funciona**: reactor, 6 disyuntores, luces por zona, navegación, baliza, focos, rampa, puertas,
  escudo térmico (persianas sobre el cristal), tren (con enclavamiento), alarma general; pantallas con el
  estado de sistemas, mapa del casco y distribución de energía. Los conductos pasan por detrás de paneles
  concretos: si revienta uno, su subsistema se queda sin energía hasta repararlo. Cajas sueltas con física,
  asientos. El servidor es autoritativo y sincroniza todo entre los jugadores.

- **Astronauta** modelado, riggeado y texturizado por código en Blender headless
  (`tools/blender/astronaut.py`): traje con fuelles en codos y rodillas, costuras y arrugas horneadas
  en un normal map, PLSS, módulo de pecho con mandos, casco con visor dorado, luces y cámara, guantes,
  botas con suela de tacos, AO horneado. Franjas de color por jugador.
- **Animación procedural** con IK de piernas: los pies se apoyan según la velocidad real, con paso de
  caminar, trote lunar (con fase de vuelo), saltos con amortiguación al aterrizar y agacharse.
- **Terreno determinista** (`src/shared/terrain.ts`) compartido por todos los clientes: campos de
  cráteres a 7 escalas, macizos lejanos, un cráter de 760 m a 1,4 km y curvatura real de la Luna (el
  horizonte está a ~2,5 km). LOD por quadtree con geomorphing CDLOD generado en workers.
- **Física** con Rapier: controlador cinemático, gravedad lunar (1,62 m/s²), inercia, escalones y
  pendientes; colisión en streaming alrededor del jugador (terreno y rocas grandes).
- **Luz de vacío**: Sol duro con sombras en cascada, sin luz de cielo, rebote del regolito, fotometría
  lunar (Lommel–Seeliger y efecto de oposición). Cielo con **5.044 estrellas reales** (posición, brillo
  y color), Vía Láctea en el marco galáctico real y la Tierra con fase coherente con el Sol.
- **Red**: servidor Node con WebSocket, estados a 20 Hz, interpolación de jugadores remotos, etiquetas
  de nombre y marcadores en la brújula.

## Desarrollo

```bash
npm run dev        # servidor + Vite con recarga en caliente (http://localhost:3000)
npm run typecheck
```

Regenerar assets (requiere Python 3.11 con `bpy==4.5.*`, `numpy`, `pillow`):

```bash
python tools/blender/astronaut.py --out public/assets/astronaut.glb [--preview preview.png]
python tools/textures/regolith.py
python tools/sky/build_sky.py
```

## Créditos de datos

- Catálogo estelar: [d3-celestial](https://github.com/ofrohn/d3-celestial) (derivado de HYG / Yale BSC).
- Texturas de la Tierra: NASA Blue Marble / Black Marble, vía los ejemplos de three.js.
- Todo lo demás (traje, terreno, rocas, texturas del regolito, Vía Láctea) se genera de forma
  procedural con los scripts de `tools/`.
