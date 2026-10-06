# Réplica por interés

Qué sabe cada jugador de lo que existe, y cómo aparece y desaparece en tiempo real. Pruebas:
`npm run test:net`. El movimiento suave de lo que viaja por red está en
[`MOVIMIENTO.md`](MOVIMIENTO.md); el mundo que decide qué existe, en [`MUNDO.md`](MUNDO.md).

## La regla (`shared/net/interest.ts`)

Un jugador **sabe** una cosa replicada (objetos sueltos, NPC…) mientras se cumpla alguna de estas
condiciones:

- está cerca: la aprende dentro de `enter` y la olvida más allá de `leave`, con `leave > enter` para
  que nada parpadee en el borde;
- está en el mismo anfitrión: lo que hay dentro de una nave se sabe desde cualquier distancia a bordo;
- la tiene **fijada**: por ejemplo, lo que simula él mismo, aunque haya volado lejos.

Al entrar en su interés recibe la cosa entera (`spawn`); al salir, la olvida (`gone`). Entre medias
solo sus conocedores oyen sus noticias. Una rejilla sobre posiciones del mundo hace que cada
actualización cueste en proporción a lo que hay cerca, no a todo lo que existe. Es puro: el servidor
lo conecta a sockets y las pruebas, a arrays.

| Clase | Qué | Interés |
|---|---|---|
| Objetos sueltos (`k: 'obj'`) | `server/objects.ts` | 1500 / 1800 m (o a bordo; el dueño siempre) |
| NPC (`k: 'npc'`) | `server/npcs.ts`, estados en `npcs` (binario, 20 Hz) | 600 / 800 m |
| Naves | `room.updateInterest` (su propio esquema, anterior) | 1500 / 1800 m el estado; las poses, a todos |

## Aparecer y desaparecer

- **Servidor**: `objects.spawn(...)` pone un objeto en el mundo y avisa ya a quien esté cerca;
  `objects.despawn(id)` lo quita y avisa a quien lo conocía. Lo mismo para `npcs`. Los ids de lo que
  guarda el mundo son sus entidades; lo local va desde 2³¹.
- **Cliente**: `Crates.spawn` / `Crates.forget` (el cuerpo Rapier, la instancia y su hueco, que se
  reutiliza) y el mapa `npcs` de `game.ts`, dibujados con `RemotePlayer`.
- **Protocolo 17**: `spawn { e: EntityWire[] }`, `gone { k, ids }`, `npcs { t, states }`. El
  `welcome` trae solo lo que el recién llegado sabe al aparecer; lo demás llega por `spawn` mientras
  se mueve.

## Una clase nueva que se replica

1. Su forma en `EntityWire` (`shared/protocol.ts`) con su `k`.
2. En el servidor, un `Replication<Cosa, Peer>` con `where` (posición en el mundo), `frame` (su
   anfitrión) y, si hace falta, `pinned`. Llama a `interest()` desde `room.updateInterest` y envía
   sus noticias con `watchers(id)`.
3. En el cliente, sus casos en `onSpawn` y `onGone` de `game.ts`.
4. Un caso en `tools/net/check.ts`.

## Herramientas de prueba (`DEV_TOOLS=1`)

Con `DEV_TOOLS=1` el servidor acepta mensajes `dev`, para pruebas y administración:

- `obj.spawn` y `obj.despawn`;
- `world.ask` (una petición a un módulo del mundo);
- `world.query` (una consulta: `recent`, `chain`…);
- `world.save`.

Las respuestas llegan como `devReply`. Las pruebas (`tools/net/harness.ts`) arrancan el servidor así.
