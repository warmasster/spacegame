# Documento de diseño — Space Game (FPV 3D, estilo Star Citizen)

> Estado: borrador v0.1 — organización de ideas antes de escribir código.

## 1. Visión

Simulador espacial multijugador en primera persona. Todo es físico y diegético:
no hay menús mágicos ni teclas atajo para los sistemas de la nave; **se mira un
botón y se pulsa**. Entrar y salir de la nave, caminar por dentro y por la
superficie ocurre sin pantallas de carga.

**Alcance v1:** una base lunar + la Luna explorable + una nave de carga.
**Regla de arquitectura:** nada puede asumir "solo existe la Luna". Cuerpos
celestes, bases y naves son datos, no código fijo.

## 2. Pilares

1. **Interacción diegética** — cada función es un objeto clicable del mundo.
2. **Sistemas con lógica propia** — radar, interceptor, energía, soporte vital…
   se simulan, no son decorado.
3. **Modularidad física** — la nave es un grafo de componentes que se dañan,
   fallan y se reparan por separado.
4. **Continuidad** — mundo sin cargas: base ↔ superficie ↔ nave ↔ (futuro) espacio.
5. **Cooperativo** — la nave está pensada para 4–5 tripulantes con roles.

## 3. Mundo

| Elemento | v1 | Futuro |
|---|---|---|
| Cuerpos celestes | Luna (gravedad 1.62 m/s², sin atmósfera) | Planetas con atmósfera, órbitas, más lunas |
| Terreno | Heightmap por chunks/LOD + **deformable** (cráteres por impactos/aterrizajes, voxels o CSG local) | Planetas completos, streaming por esferas |
| Base lunar | Hangar, esclusas, módulos presurizados, puertas con teclado de código | Varias bases, economía |
| Objetivo | Explorar: puntos de interés, anomalías, señales a descubrir | Misiones, carga, comercio |
| Coordenadas | Origen flotante (floating origin) + coordenadas locales por cuerpo | Sistema solar a escala |

## 4. Jugador (FPV)

- Caminar, correr, agacharse, saltar con gravedad del cuerpo actual.
- Traje EVA: oxígeno, temperatura, integridad; casco/visor.
- **Retícula de interacción**: raycast desde la cámara → resalta el objeto →
  clic = acción (pulsar, girar, arrastrar palanca, mantener).
- Sentarse en asientos → pasar a control de estación (piloto, torreta, sensores).
- Transición interior/exterior de nave continua (la nave es un marco de
  referencia móvil: te mueves con ella al caminar dentro).

## 5. Nave de carga (4–5 tripulantes)

### 5.1 Distribución
Cabina (piloto + copiloto) → estación de sensores/comunicaciones → pasillo con
esclusa lateral → bodega de carga con rampa trasera → 1–2 torretas tripuladas
→ sala de máquinas (reactor, soporte vital, paneles eléctricos).

### 5.2 Elementos clicables (todos funcionales)
- **Luces**: interiores por zona, exteriores, focos de aterrizaje, luces de
  emergencia (se activan solas si falla la energía).
- **Asientos**: altura, inclinación, distancia; cinturón.
- **Tren de aterrizaje**: subir/bajar con enclavamiento de seguridad; se
  puede dañar.
- **Escudo térmico**: desplegar/retraer, **visible desde fuera**.
- **Puertas, rampa y esclusa**: ciclo de presurización real; teclado de código
  pulsando cada tecla.
- **Paneles de energía**: interruptores/breakers por subsistema.
- Radar, interceptor, torretas, comunicaciones (ver §6).

### 5.3 Vuelo ("entretenido pero realista")
- Física newtoniana 6DOF con propulsores individuales (cada uno un módulo
  que puede fallar → empuje asimétrico).
- Asistencias conmutables con botones: estabilización, desacople (decoupled),
  asistente de aterrizaje, límite de G.
- Aerodinámica: modelo de superficies (sustentación/resistencia por densidad
  del aire). En la Luna densidad = 0 → vuelo puro por propulsores; el modelo
  queda listo para planetas con atmósfera.
- Masa dinámica: la carga y el combustible afectan a la inercia.

## 6. Herramientas y sistemas (cada uno con lógica propia)

| Sistema | Lógica / manejo |
|---|---|
| **Radar** | Modos activo/pasivo; barrido con alcance, ganancia y ángulo ajustables con perillas; ruido y ecos del terreno; el activo te delata; firmas (EM/IR/sección radar); fijado de blancos |
| **Interceptor de señales** | Sintonizar frecuencia con dial, ancho de banda, filtrar ruido; triangular fuente moviendo la nave; decodificar (minijuego: alinear patrón/cifrado); señales llevan a puntos de interés |
| **Comunicaciones** | Canales de voz internos/externos, frecuencias |
| **Torretas** | Estación tripulada; munición y calor; necesitan energía; se dañan por separado; munición física |
| **Energía** | Reactor → buses → consumidores; repartir energía; sobrecargas saltan breakers |
| **Soporte vital** | O₂, presión y temperatura por compartimento |
| **Navegación** | Mapa, waypoints, altímetro radar |

## 7. Daño modular y fallos

- La nave = **grafo de componentes** (casco por secciones, propulsores,
  tanques, cables/conductos, reactor, sensores, tren, escudo térmico…).
  Cada uno con HP, estado (OK/dañado/destruido) y dependencias.
- Impactos localizados → daño al componente golpeado y propagación por el grafo
  (cortar un conducto deja sin energía lo que alimenta).
- **Pérdida de presión/vacío**: brecha en un compartimento → el aire sale a
  una tasa según el tamaño del agujero; las puertas estancas aíslan zonas.
- **Fallos de sistemas**: por daño, sobrecalentamiento o desgaste; avisos,
  alarmas y luces de emergencia.
- Reparación in situ con herramientas (futuro próximo).
- Daño visible por fuera (paneles deformados, chispas, fugas).

## 8. Multijugador

- Arquitectura **servidor autoritativo** (física y estado de naves en el servidor).
- Predicción en cliente para el jugador y la nave que pilota; interpolación para el resto.
- Replicación por interés (solo lo cercano); estado de cada botón/sistema
  sincronizado → si un compañero enciende una luz, todos la ven.
- Chat de voz por proximidad y por radio de la nave (futuro).

## 9. Arquitectura técnica (propuesta)

**Motor/stack** — decisión pendiente (ver §11):
- **Opción A — Web (Three.js / Babylon + Rapier + Node/WebSocket):** jugable
  en navegador, rápido de iterar; límites de rendimiento.
- **Opción B — Godot 4:** gratis, buen multijugador integrado, C#/GDScript.
- **Opción C — Unreal / Unity:** mayor calidad gráfica, más pesado.

Módulos independientes del motor:
```
core/        coordenadas, origen flotante, tiempo, eventos
world/       cuerpos celestes, terreno (chunks, LOD, deformación), bases
player/      controlador FPV, EVA, interacción (raycast + interactables)
ship/        definición por datos, componentes, grafo de daño, compartimentos
systems/     energía, soporte vital, radar, interceptor, torretas, comms
flight/      física 6DOF, propulsores, aerodinámica, asistencias
net/         servidor autoritativo, replicación, predicción
ui/          solo HUD mínimo del casco; el resto son pantallas dentro del mundo
content/     datos: naves, cuerpos, bases, señales (JSON/recursos)
```
Patrón clave: `Interactable` (botón, palanca, dial, teclado) emite acciones →
los `System` las procesan → el estado se replica.

## 10. Hoja de ruta

| Fase | Contenido |
|---|---|
| **0 — Prototipo** | Terreno lunar básico, jugador FPV con gravedad lunar, sistema de interacción mirar+clic, puerta con teclado de código |
| **1 — Nave caminable** | Nave con interior, marco de referencia móvil, asientos, luces, tren, escudo térmico, rampa; entrar/salir sin cargas |
| **2 — Vuelo** | 6DOF, propulsores, asistencias, aterrizaje |
| **3 — Multijugador** | Servidor autoritativo, 4–5 jugadores en la misma nave |
| **4 — Sistemas** | Energía, soporte vital, radar, interceptor, POIs de exploración |
| **5 — Daño** | Daño modular, compartimentos y despresurización, fallos, torretas |
| **6 — Terreno deformable** | Cráteres dinámicos sincronizados en red |
| **7+** | Más cuerpos, atmósfera, reparación, economía |

## 11. Estado actual — v0.1 (mínimo viable)

Decisiones tomadas para el MVP:

- **Motor: web (Three.js + Rapier + Node/WebSocket).** Se juega desde el navegador sin instalar nada
  (el segundo jugador solo abre una URL), se itera muy rápido y todo el contenido se genera por código.
  El diseño de módulos es independiente del motor; si el rendimiento lo exige más adelante, el paso
  natural es WebGPU o un cliente nativo reutilizando servidor y protocolo.
- **Cooperativo, 2 jugadores**, servidor que retransmite estados (clientes de confianza). La autoridad
  física pasará al servidor cuando haya naves y daños.
- **Estilo realista**: el traje es un modelo propio generado en Blender por script; terreno, rocas,
  texturas y cielo son procedurales o datos reales (catálogo estelar, texturas NASA de la Tierra).
- El terreno se define como función determinista compartida (`src/shared/terrain.ts`) con curvatura
  real del cuerpo; los cuerpos celestes siguen siendo datos (`BodyDef`), no código fijo.

Incluido: dos astronautas con animación procedural, FPV con cuerpo visible y 3ª persona, gravedad lunar,
física, terreno con LOD, sombras, cielo real, HUD de casco, luces del casco sincronizadas, nombres.

**v0.3 — nave caminable (fase 1, en tierra):** nave de carga modular definida por datos
(`src/shared/ship/`): 126 paneles rompibles y reparables, 26 mandos, energía reactor → disyuntores →
subsistemas con conductos, puertas, rampa, escudo térmico visible desde fuera, tren con enclavamiento,
asientos, carga suelta con física, pantallas. Servidor autoritativo del estado de la nave. Falta: vuelo
(fase 2), marco de referencia móvil, presión por compartimento, replicar la física de la carga.

## 12. Decisiones abiertas

1. **Motor / plataforma** (web, Godot, Unity, Unreal).
2. Estilo visual (realista vs. estilizado low-poly).
3. ¿PvP o solo cooperativo contra la IA/entorno?
4. Hosting: servidor dedicado vs. un jugador hace de host.
5. Nivel de complejidad del minijuego del interceptor.
