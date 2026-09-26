# Guía para agentes (y humanos)

Juego FPV cooperativo lunar: Three.js + Vite + TypeScript (cliente), Node + `ws` (servidor), Rapier (física).
Este archivo es el punto de entrada para un agente de IA que vaya a desarrollar el proyecto.

## Mapa del código

| Carpeta | Qué hay |
|---|---|
| `src/engine/` | Núcleo reutilizable: `loop.ts` (paso fijo 60 Hz), `systems.ts` (planificador de sistemas), `debris.ts` (cuerpos rígidos), `debug.ts` (F3/F4/F5) |
| `src/client/game.ts` | Orquestador: crea el mundo, registra sistemas (`registerSystems`), red, HUD, cámara |
| `src/client/player/` | Astronauta (rig, animación procedural, IK de brazos/agarres), controlador Rapier, cámara |
| `src/client/fx/` | Armas (`weapons.ts`, datos + sockets), cohetes, partículas |
| `src/client/world/` | Terreno LOD en workers, colisión en streaming, rocas, iluminación |
| `src/shared/` | Código común cliente/servidor: terreno determinista, protocolo de red |
| `src/server/` | Servidor autoritativo (salas, daño, explosiones, ediciones de terreno) |
| `tools/blender/` | Traje del astronauta generado por código (Blender headless → `public/assets/astronaut.glb`) |
| `tools/diag/` | Herramientas de autodiagnóstico visual y numérico |

## Reglas del motor

- **Nada de lógica en el bucle principal.** Una funcionalidad nueva es un sistema:
  `this.systems.add({ name, phase: 'fixed' | 'frame', order, update })`.
  Orden: 10 física · 20 personajes · 30 objetos del mundo · 40 jugabilidad · 90 efectos.
  F3 muestra los ms de cada sistema.
- La simulación (física, movimiento, proyectiles) va en `phase: 'fixed'` (determinista, 60 Hz).
  El render interpola entre pasos (`ctl.renderPosition`).
- **Datos antes que código:** un arma nueva es un `WeaponDef` en `fx/weapons.ts` con sus agarres
  (`Grip`: centro, eje, radio, lado de la mano). Las manos se colocan solas por IK; no hay
  animaciones hechas a mano.
- Las medidas del cuerpo se miden del propio modelo (p. ej. `Astronaut.calibrateHands` saca palma y
  dedos de la malla del guante), no se escriben a mano.
- El terreno es una función determinista compartida (`shared/terrain.ts`); todo cambio de terreno
  pasa por `edits` (el servidor los guarda y reenvía).
- El servidor es autoritativo para daño, explosiones y ediciones.

## Bucle de verificación (obligatorio antes de hacer commit)

```bash
npm run typecheck
npm run dev                  # en otra terminal
npm run diag:grasp           # agarre del arma: mide palma/dedos contra la empuñadura (falla con exit 1) + primeros planos
npm run diag:pose            # capturas del astronauta desde varias cámaras
npm run diag:terrain         # capturas del terreno
```

Las capturas quedan en `tools/diag/out/` (con hoja de contactos `*_sheet.png`). **Mírelas**: una
métrica que da siempre OK puede estar midiendo su propia suposición (ya pasó: el chequeo del agarre
usaba la misma calibración que el IK y no veía la mano al revés; ahora mide la malla renderizada).

Automatización desde el navegador (`?manual&offline`):
`game.step(n)`, `game.me.graspReport()`, `game.systems.list()`, `game.focusCam = { part: 'handR', az, el, dist }`,
`game.debug` (controlador, cámara, input), `?cam=x,z,yaw,pitch,h`, `?nobaked`, `?nomorph`, `?skirts`, `?ao`.

Requisitos de las herramientas: `npm i -D playwright && npx playwright install chromium`.
Regenerar el traje: Python 3.11 con `bpy==4.5.*` → `python tools/blender/astronaut.py --out public/assets/astronaut.glb`.
