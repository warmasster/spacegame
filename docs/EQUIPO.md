# Equipo: marcos, proyectiles, armas y objetos

Lo que la tripulación dispara, lleva y maneja es **datos en catálogos compartidos** (`src/shared/items/`),
y todo lo que se mueve vive en un **marco** (`src/shared/frames/`). El mismo código corre en el cliente
y en el servidor.

## 1. Marcos (`shared/frames`)

Hay un marco para el mundo (`WORLD_FRAME`, id 0) y uno para cada **anfitrión**: algo con espacio propio
en el que se puede estar dentro. Hoy es una nave; mañana puede ser una estación o un vehículo en una
bodega. Un anfitrión es un `FrameHost`:

| Campo | Qué |
|---|---|
| `id` | Su id (nunca 0) |
| `pose`, `prev` | Su pose en el mundo en este paso y en el anterior (para dibujar entre los dos) |
| `reach` | Más allá de esto (m, en su espacio) no hay nada dentro: descarte barato |
| `inside(l)` | ¿Está este punto de su espacio dentro de uno de sus compartimentos? |

- `carry(a, b, p, v, q)` lleva un movimiento de un marco a otro sin saltos: la misma posición y la
  misma velocidad en el mundo.
- `hostAt(anfitriones, p)` dice en qué anfitrión está un punto del mundo, si está en alguno.
- Una nave es anfitrión así: en el cliente, `ShipClient implements FrameHost`; en el servidor,
  `shipHost(sim)`. Lo de dentro son sus zonas (`inShip`, `shipReach`).

La regla de «dentro» es una sola. La usan los proyectiles, las cajas y el astronauta, que además
añaden sus propias condiciones (estar apoyado en el casco, pisar la escalera).

**Por qué:** dentro de un anfitrión todo son números pequeños y locales. No importa lo rápido que vaya
(en órbita, 1,6 km/s), dónde lo tenga cada cliente en la red ni la precisión de float32. En la red algo
de un anfitrión viaja como `fr` + coordenadas locales.

### Cuerpo balístico (`ballistic.ts`)

Todo lo que vuela libre (cohetes, balas y lo que se añada) es un `Ballistic` y avanza con
`stepBallistic(b, spec, dt, env)`:

- **Dentro de un anfitrión** vuela en su espacio:
  - sale justo de donde se lanzó;
  - la cabina no puede adelantarlo;
  - siente la gravedad aparente de la cabina;
  - choca con los colisionadores de la nave.
- **Al salir** por una puerta o una brecha pasa al mundo en un paso fijo, con la misma posición y
  velocidad. Si entra en un anfitrión, pasa a su marco del mismo modo.
- **En el mundo** cae con la gravedad radial real del cuerpo en el que esté y choca con cascos, suelo y
  tripulación. La tripulación se comprueba contra el tramo recorrido, así que no hay túnel a ninguna
  velocidad.
- **Subpasos según lo que recorre** (máx. 6 m cada uno). A bordo se mueve unos m/s respecto a la
  cabina: normalmente basta un subpaso.

Lo que atraviesa lo pone cada lado con un `BallisticEnv`: anfitriones, gravedad de cabina, barrido
dentro de un anfitrión, barrido de cascos en el mundo, altura sobre el suelo y tripulación. El cliente
lo monta sobre Rapier (`client/fx/projectiles.ts`). El servidor puede montar el suyo para simular los
disparos de un NPC o de una torreta.

## 2. Proyectiles (`shared/items/projectiles.ts`)

```ts
defineProjectile({
  id: 'bullet', name: 'Bala',
  speed: 380,          // m/s respecto al lanzador (se suma su velocidad)
  life: 3,             // s
  gravity: 1,          // multiplicador de la gravedad que siente
  radius: 0.45,        // cuánto tiene que pasar de cerca del centro de alguien para darle
  impact: { radius: 0.5, crew: 34, falloff: false, hull: { radius: 0.3, damage: 6 }, crater: false, fx: 'hit' },
  look: 'tracer',      // aspecto en el cliente (client/fx/projectileLooks.ts)
  sounds: { impact: 'bullet.hit' },
});
```

`impact` lo aplica el servidor:
- `crew`: daño al traje. Con `falloff` baja con la distancia (explosión); sin él es igual en todo el
  radio (impacto directo).
- `hull`: daño a paneles y máquinas, a escala de nave.
- `crater`: cráter si ocurre cerca del suelo.

`fx` dice cómo se ve en los clientes:
- `blast`: bola de fuego, onda que empuja y cajas volando.
- `hit`: chispas, un golpe metálico y un empujón a un objeto suelto.

Aspecto nuevo: `defineProjectileLook(id, { mesh, trail, launch, tail })`.

## 3. Armas y herramientas (`shared/items/weapons.ts`)

Todas son equipables. Las teclas numéricas las eligen por orden de catálogo (1, 2, 3…). La misma tecla
otra vez, o X, la guarda; guardada va en su sitio de la mochila. Qué hace el gatillo (`action`):

| `action` | Qué |
|---|---|
| `{ kind: 'fire', projectile, auto? }` | Dispara ese proyectil; con `auto`, mientras se mantiene el botón |
| `{ kind: 'weld' }` | Suelda y repara paneles y máquinas mientras se mantiene (el servidor solo repara con una de estas en mano) |

Además: `cooldown` (lo comprueba el servidor), `recoil` (N·s: cuerpo, brazos y cámara), `hud` (textos
del casco) y `sounds.fire`.

El aspecto va aparte, en `client/fx/weapons.ts` (`LOOKS`): malla, agarres de cada mano, boca y funda en
la mochila. Un arma del catálogo sin aspecto da error al arrancar.

Hoy: **lanzacohetes** (1), **soldadora** (2) y **fusil** (3: balas trazadoras, automático). El fusil
va en el costado derecho de la mochila, simétrico a la soldadora.

### Quién dispara

El disparo en la red es `fire { w, o, d, v, fr }` (arma, origen, dirección, velocidad del lanzador,
marco). El servidor lo valida con el catálogo (arma que dispara algo y cadencia) y lo reenvía. Cada
cliente simula el proyectil (`Projectiles.spawn(dueño, tipo, marco, …)`). El que dispara lo lanza en el
acto, sin esperar el eco, y avisa del impacto (`hit { k, p, fr }`). El servidor aplica `impact` y lo
anuncia (`explode { k, fr, l }`).

El `dueño` es un id cualquiera. Un NPC haría lo mismo desde el servidor. Faltaría su `BallisticEnv` del
lado del servidor para que él mismo decida el impacto.

## 4. Objetos sueltos (`shared/items/objects.ts`)

Todo lo que está suelto como cuerpo físico real, sea una caja, una pieza de repuesto o lo que venga:

```ts
defineObject({
  id: 'spare', name: 'Pieza de repuesto',
  half: [0.16, 0.11, 0.16], mass: 9,          // tamaño y masa por defecto
  handling: { grab: true, throw: true },       // qué se puede hacer con él
  look: 'spare',                               // aspecto (client/cargo/looks.ts)
  sounds: { grab: 'crate.grab', drop: 'crate.drop', throw: 'crate.throw' },
});
```

- Todos comparten la misma maquinaria (`client/cargo/crates.ts`): cuerpo Rapier en su marco, un dueño
  que lo simula, red y paso entre nave y mundo.
- Todos se manejan igual:
  - al mirarlo, el casco dice qué es y cuánto pesa;
  - **E** lo coge o lo suelta;
  - **Q** lo lanza;
  - **clic** lo deja sobre la silueta.
- Una colocación (la bodega de una nave con `cargo: [{ kind: 'spare', … }]`, las cajas de un sitio)
  puede darle otro tamaño y color. Sin `kind` es una caja.
- Aspecto nuevo: `defineObjectLook(id, { geometry, materials })`. Todos los de un mismo aspecto,
  tamaño y color son una sola malla instanciada.

Hoy: **caja de carga** y **pieza de repuesto** (dos en la bodega de la Selene, encima de las cajas
pequeñas).

## 5. Añadir algo

| Quiero… | Toco |
|---|---|
| Otra munición | `defineProjectile` (+ `defineProjectileLook` si se ve distinta, + sonidos en `client/audio/sounds/`) |
| Otra arma | `defineWeapon` + su aspecto en `LOOKS` (`client/fx/weapons.ts`) |
| Otro objeto | `defineObject` + `defineObjectLook`; colocarlo en una bodega o un sitio |
| Otro tipo de anfitrión (estación, vehículo) | Que implemente `FrameHost`; lo demás ya lo trata igual |
