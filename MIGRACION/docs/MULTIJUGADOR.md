# Multijugador: los cimientos

**Hoy (V41, 2026-10-08): el servidor tiene la partida.** `luna-servidor` simula el mundo entero
(`lunar_play::host`), decide todo y le cuenta a cada jugador lo que necesita; el juego de cada uno
predice su cuerpo y lo demás y el servidor lo endereza (`lunar_play::online`); sin conexión, el
mismo servidor corre dentro del proceso (`lunar_play::local`). Cómo y por qué:
[`PLAN_AUTORITATIVO.md`](PLAN_AUTORITATIVO.md). Esta biblioteca (`crates/net`, `lunar_net`) es solo
la conexión: quién entra y sale, el chat, y los mensajes del juego (`Msg::Game` fiables, `Msg::Quick`
los más nuevos) llevados sin leerlos.

Lo que sigue de las secciones marcadas **(histórico)** es del **relevo** que hubo antes (V35–V40):
un servidor que no simulaba nada y clientes que simulaban cada uno el mundo entero, con dueños de
naves y estados repartidos. Se quitó en la fase 10 del plan; queda aquí lo medido entonces.

## Dónde está cada cosa

| Fichero (`crates/net/src`) | Qué hace |
|---|---|
| `wire.rs` | `Writer` / `Reader` sobre rebanadas de bytes: enteros, varints, zig-zag, `f32`/`f64`, cadenas con tope, `Vec3`. Leer nunca hace `panic`: lo corto o roto es un `Err`. |
| `quant.rs` | Valores cuantizados: ángulo en 16 bits, coma fija en varint, cuaternión «los tres menores» en 48 bits. |
| `transport.rs` | `trait Transport` (mandar y recibir datagramas sin bloquear), `Addr`, `MTU` = 1200. |
| `transport/udp.rs` | `Udp`: un `UdpSocket` no bloqueante. |
| `transport/memory.rs` | `MemoryNet` / `Memory`: una red dentro del proceso que pierde, retrasa, duplica y desordena a propósito, igual en cada ejecución con la misma semilla (pruebas). |
| `channel.rs` (+ `channel/`) | Conexión con un par: cabecera con secuencia y acuses, mensajes **fiables ordenados** y **no fiables secuenciados**, varios por datagrama, troceo de los largos, RTT, latidos, silencio. |
| `proto.rs` | Qué es un datagrama (`Hello`, `Challenge`, `Welcome`, `Refused`, `Data`, `Bye`) y los mensajes del canal (`Msg`). |
| `game.rs` (+ `game/`) | `PlayerState`, `RigidState`: datos planos, su codificación y su mezcla (lo que llevan las instantáneas de `lunar_play::net`). Los bits de `flags` en `game::flag`. |
| `clock.rs` | `now()` y la estimación del reloj del servidor (`Ping`/`Pong`). |
| `server.rs` (+ `server/`) | `Server`: sesiones, entrada, chat, lo que cada juego dice a la partida (`take_game`) y lo que la partida le dice (`send_game`). Sin E/S propia. |
| `client.rs` (+ `client/`) | `Client`: lo que usa el juego (`send_game`, `send_quick`, `events`, `chat`). |
| `text.rs` | Todos los textos que lee una persona (en castellano) y la limpieza de lo que la gente escribe. |

`servidores/LunaServidor.exe` se rehace desde Linux (2026-10-08, con `rustup target add
x86_64-pc-windows-gnu` y el paquete `mingw-w64` por sus bibliotecas):
`CARGO_TARGET_DIR=target/win CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=<sysroot>/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C linker-flavor=ld.lld -C link-self-contained=yes -L /usr/x86_64-w64-mingw32/lib -L /usr/lib/gcc/x86_64-w64-mingw32/13-posix" cargo build --release -p luna-servidor --target x86_64-pc-windows-gnu`
y `x86_64-w64-mingw32-strip`; en Windows, `cargo build --release -p luna-servidor` con la
configuración de `.cargo/config.toml`.

`crates/server/src`: `main.rs` (la red), `sim.rs` (la partida en su hilo), `config.rs`
(`servidor.jsonc` y opciones), `console.rs` (órdenes), `journal.rs` (consola + `servidor.log`).
Las dos ranuras de la partida guardada y quien la guarda cada tanto están en `lunar_play::keep`:
las usan el servidor y la partida propia de la ventana (`lunar_play::local`).

## El protocolo

### Datagramas

El primer byte dice qué es. Ninguno pasa de 1200 bytes.

| Datagrama | Quién | Contenido |
|---|---|---|
| `Hello` | cliente | marca `LUNA`, versión del protocolo (3), `salt` (un número que el cliente inventa para esta conexión), `cookie`, naves del escenario, versión del juego, nombre. **Siempre 300 bytes** (relleno). |
| `Challenge` | servidor | `salt`, `cookie`: «repítelo con esto». |
| `Welcome` | servidor | `salt`, id del jugador, nombre tal como quedó, nombre del servidor. |
| `Refused` | servidor | `salt`, motivo (texto para la persona). |
| `Data` | ambos | un datagrama del canal. |
| `Bye` | ambos | `salt`, motivo. El cliente lo manda tres veces al cerrar. |

**Entrada**: el cliente dice `Hello` (cada 0,25 s, hasta 5 s); el servidor contesta `Challenge`
con una `cookie` (un hash con clave de la dirección y el `salt`: no guarda nada); el cliente
repite el `Hello` con ella y entonces el servidor crea la sesión y contesta `Welcome` (o
`Refused`). Así solo entra quien de verdad está en la dirección que dice el datagrama, y como
el `Hello` es más largo que cualquier respuesta, el servidor no sirve para amplificar. Los
rechazos se limitan a 8 por segundo (ni se contestan ni se apuntan más). El servidor no manda
nada por el canal hasta oír al cliente por él (un `Welcome` perdido no desperdicia la puesta al
día).

### El canal

Cabecera de 10 bytes: marca (1), secuencia (2), último recibido (2), los 32 anteriores en bits
(4), y cuántos ms lleva esperando ese acuse (1). Cada acuse viaja en hasta 33 datagramas.
Detrás, los mensajes: `[etiqueta][id, si es fiable][longitud][bytes]`.

- **Fiables ordenados**: en cola hasta que se acusa un datagrama que los llevó; se reenvían si
  pasa `RTT + 4·variación` (entre 50 ms y 1 s) sin acuse; se entregan una vez y en orden.
  Ventana de 256 en vuelo. Uno más largo que un datagrama se corta en trozos de 1024 bytes
  (tope 64 KiB).
- **No fiables secuenciados**: se mandan una vez; si llegan después de un datagrama más
  nuevo se tiran.
- **RTT**: solo mide el acuse del datagrama más nuevo, y se le resta lo que esperó en el otro
  lado; suavizado de TCP.
- **Latido**: un datagrama vacío por segundo si no hay nada que decir.
- **Sin reservas de memoria por paquete** una vez caliente: los búferes dan la vuelta. (Por
  construcción; no medido con un asignador contador, que necesitaría `unsafe`.)

### Mensajes

| Mensaje | Fiable | Quién | Para qué |
|---|---|---|---|
| `Ping` / `Pong` | no | c / s | reloj del servidor (y que se le oiga mientras el juego calla) |
| `Game` | sí | ambos | lo que el juego dice (actos del jugador; lo que pasó: `lunar_play::net::Event`) |
| `Quick` | no | ambos | lo mismo, el más nuevo (comandos de cada paso; instantáneas) |
| `Chat` / `Said` | sí | c / s | chat; `Said` sin autor es el servidor |
| `Joined`, `Left` | sí | servidor | quién está |
| `Bundle`, `Synced` | sí | servidor | la puesta al día de quien entra (quién está), y su final |

## Sellado y límites (fase 11)

- **Cada sesión va sellada** (`seal.rs`, `channel::Channel::seal`; protocolo 4): en el saludo
  cada lado enseña una clave X25519 hecha para ese saludo (el cliente en su `Hello`, el servidor
  en su `Welcome`); de lo que los dos sacan de ellas y de lo que dijo el saludo (la galleta del
  `Challenge` y la sal del cliente) salen dos claves, una por sentido (HKDF con SHA-256); cada
  datagrama va cifrado y firmado con la de su sentido y un número suyo que no se repite
  (ChaCha20-Poly1305: 8 bytes de número tras el primer byte y 16 de sello al final; el primer
  byte, firmado). Quien ve el tráfico (la misma wifi) no lee nada; uno que no abre se tira sin
  leerlo (`ChannelError::Forged`, `ServerStats::forged`) y no echa a nadie; uno que ya llegó
  (la red que repite, o quien lo repite) no se toma otra vez. Sellar y abrir un datagrama de
  1 200 bytes, 5,75 µs entre los dos extremos. Lo que no para: quien está en medio del camino
  desde el saludo mismo y contesta como si fuera el servidor (haría falta conocer de antemano la
  clave propia del servidor). Las cuentas están en las dependencias (`x25519-dalek`,
  `chacha20poly1305`, `hkdf`, `sha2`, `getrandom`): nuestro código sigue sin `unsafe`.
- **La sesión es su clave, no su dirección**: un datagrama sellado por una sesión que llega desde
  otra dirección la lleva allí (`ServerEvent::Moved`): el router que cambia la dirección del
  jugador no lo echa. Se comprueban como mucho 256 por segundo de direcciones desconocidas.
- **Cada uno, lo suyo**: como mucho 512 mensajes para la partida de cada jugador cada vez que la
  partida los toma (`ServerStats::flooded`); los límites de lo que dice la partida (comandos,
  actos, `Resync`) son de `lunar_play::host`.

## Tamaños y errores

Medidos por `tests/wire.rs` (posición en la superficie de la Luna, sin ningún eje cerca de cero).

| Cosa | Bytes |
|---|---|
| `PlayerState` andando | **26** |
| `PlayerState` quieto | 22 |
| `PlayerState` sentado a los mandos de una nave | 37 |
| `PlayerState` con todo | 43 |
| `ShipState` entero, volando, 20 articulaciones | **74** (78 a velocidad orbital; 114 con 40) |
| `ShipState` sin articulaciones, como va cada envío | **34** volando (38 orbital, 23 aparcada) |
| Articulaciones aparte (20) | 41 |
| Cabecera de datagrama | **10** (+ 28 de IP y UDP) |
| Mando: pulsar / soltar / fijar / girar | 6 / 5 / 13 / 13 (+ 4 de marco) |

| Valor | Cómo viaja | Error máximo |
|---|---|---|
| Posición en el mundo (`f64`) | coma fija 1/4096 m, varint con signo: 5 bytes por eje hasta 4 194 km del origen, 6 hasta 536 000 km, 8 en todo el sistema solar | 0,12 mm |
| Posición en el marco de una nave | 1/2048 m (3 bytes por eje hasta 512 m) | 0,24 mm |
| Guiñada, cabeceo, cabeza | 16 bits por vuelta | 4,8·10⁻⁵ rad (0,0027°) |
| Velocidad del jugador | 1/256 m/s | 2 mm/s |
| Altura del ojo | centímetros (hasta 2,55 m) | 5 mm |
| Rotación de nave | **«los tres menores» en 48 bits**: 2 bits para la componente omitida, 15 para cada una de las otras | 1,4·10⁻⁴ rad medido (0,008°) |
| Velocidad de nave | 1/1024 m/s | 0,5 mm/s |
| Giro de nave | 1/4096 rad/s | 1,2·10⁻⁴ rad/s |
| Articulaciones | 1/2048 por unidad: 1 byte en cero, 2 hasta ±4, 3 hasta ±512; como mucho 64 | 0,24 mm o 0,014° |

**Por qué 48 bits y no 32** para la rotación: con 10 bits por componente el error llega a 0,14°,
que son 6 cm en la punta de una nave de 50 m, demasiado para un estado que se aplica a una
simulación; los 2 bytes de diferencia son 40 B/s por nave. El cero es exacto: la identidad y
los ángulos rectos llegan tal cual.

**Solo viaja lo que cambia** (`throttle.rs`): un estado se manda cuando sus bytes cambian, dos
veces más después del último cambio y una vez por segundo mientras sigue igual. Las
articulaciones de una nave van aparte y solo cuando cambian ellas: en vuelo son más de la mitad
de los bytes y casi nunca se mueven. Mientras un jugador va montado en una nave, su posición y
velocidad en el mundo no cuentan como cambio (los demás lo colocan con `local`).

## Ancho de banda medido (histórico)

`tests/bandwidth.rs`, red en memoria, bytes por segundo **contando los 28 de IP+UDP** de cada
datagrama. 8 jugadores andando y mirando alrededor, 4 naves volando (del anfitrión) con 20
articulaciones cada una, 20 envíos por segundo:

| Caso | Anfitrión (su jugador + 4 naves) | Otro cliente (su jugador) | Servidor en total |
|---|---|---|---|
| Naves volando, articulaciones quietas | sube 4,5 kB/s · baja 5,6 kB/s | sube 1,5 kB/s · baja **8,6 kB/s** | manda 65,6 kB/s (525 kbit/s), recibe 14,9 kB/s |
| Igual, con una articulación moviéndose siempre en cada nave | sube 7,7 kB/s · baja 5,6 kB/s | sube 1,5 kB/s · baja 11,8 kB/s | — |
| Todos quietos, naves aparcadas | sube 0,36 kB/s · baja 0,31 kB/s | sube 0,10 kB/s · baja 0,57 kB/s | — |
| 16 jugadores y 40 naves volando | sube 32 kB/s | baja 42 kB/s en 2 datagramas por envío | — |

Con el programa de verdad y UDP real en esta máquina (8 clientes, 50 s): otro cliente sube
1,46 kB/s y baja 7,5 kB/s; el servidor gasta **0,8–1,0 % de un núcleo**; parado y sin nadie,
**0,000 %** (0 ms de CPU en 12 s: duerme 50 ms entre vueltas; con jugadores, 5 ms).

La puesta al día de quien entra tarde va en pocos mensajes grandes (`Bundle`): 4096 mandos, 40
naves puestas y 340 dueños son 81 kB en 75 datagramas y 0,43 s con un 10 % de pérdidas.

## Reloj e interpolación (histórico)

- **Un solo reloj**: cada cliente estima el reloj del servidor con `Ping`/`Pong` (guarda los
  últimos 16; la estimación es la media de los que tardaron como mucho 2 ms más que el más
  rápido; mientras no tiene bastantes, pregunta deprisa; las correcciones se deslizan a 0,5 ms/s
  en vez de saltar, `clock::SLEW`) y sella sus estados con esa hora, en microsegundos. El servidor respeta el sello (acotado:
  ni más de 2 s atrás ni más de 0,1 s adelante).
- **Los demás se dibujan en el pasado** (`snap.rs`): unos 100 ms. Entre dos instantáneas se
  mezcla (posiciones en línea recta, ángulos y rotaciones por el camino corto); pasada la más
  nueva se sigue con su velocidad **250 ms como mucho** y luego se congela. Lo que se lleva al
  presente (`rigid_carried`) usa también la aceleración que dan las dos últimas instantáneas
  (`SnapBuffer::speeding`, acotada a 150 m/s²): una nave que gira o frena no se sale de su curva.
- **El retraso se adapta** (`client/delay.rs`): tiene que cubrir lo vieja que llega una
  instantánea más un intervalo. Mínimo 100 ms, máximo 400 ms; crece como mucho un 10 % del
  tiempo y baja un 3 %. En red limpia se queda en 100 ms; con 30–70 ms de retardo por sentido y
  3 % de pérdidas, en unos 210 ms.
- **Tras un silencio** el cambio no se estira: el estado va marcado y el receptor sabe que
  estuvo quieto hasta un envío antes.

Medido (`tests/interp.rs`): a 3 m/s con ese retardo irregular, error máximo 6,7 mm respecto
a donde estaba en el instante dibujado, 0,2 mm fuera de su línea, nunca hacia atrás.

## De quién es cada nave (histórico)

La decide el servidor (`server/world.rs`) y la anuncia con `Owner`:

- es de **quien está sentado a sus mandos**: `PlayerState::seat` con el bit
  `flag::AT_CONTROLS` (un asiento de pasajero no lo lleva). Si hay varios, del primero que se
  sentó; cuando se levanta, pasa al siguiente que esperaba a los mandos;
- sin nadie a los mandos, del **anfitrión**: el jugador conectado con la id más baja;
- cambia cuando un piloto se sienta o se levanta, o cuando alguien se va.

El servidor solo acepta estados de una nave si vienen de su dueño; los demás se ignoran y se
cuentan (`ServerStats::foreign`).

## El programa servidor

`servidores/LunaServidor.exe`, con `servidor.jsonc` al lado (JSON con comentarios y comas
finales): `puerto` (47600, UDP), `nombre`, `max_jugadores` (16), `espera` (10 s), `datos`, `trucos`,
`partida` (dónde se guarda: `partidas/partida`) y `guardar_cada` (300 s). Opciones: `--puerto N`,
`--nombre X`, `--datos CARPETA`, `--partida RUTA`, `--nueva`, `--sin-guardar`, `--trucos`, `--ayuda`. Órdenes: `jugadores`, `expulsar <id> [motivo]`,
`decir <texto>`, `salir`, `ayuda`. Todo lo que dice va también a `servidor.log`. Las horas son
UTC (`std` no conoce la hora local). Ctrl+C (y en Linux un `kill` sin más) es como `salir`:
guarda y para (la dependencia `ctrlc`: lo que llama al sistema está en ella, no en nuestro
código); cerrar la ventana en Windows lo para sin guardar. Ver `servidores/LEEME.txt`.

Tras cambiar el servidor: `tools/cargo.ps1 build --release -p luna-servidor --target-dir target/red`
y copiar `target/red/release/luna-servidor.exe` a `servidores/LunaServidor.exe`.

## Usarlo desde el juego (histórico)

```rust
let mut net = lunar_net::Client::connect("192.168.1.20:47600", "Ana", "V35", naves_del_escenario)?;
// cada frame, con el mismo reloj siempre (lunar_net::now() vale):
net.set_player(&mi_estado);                                  // antes de update
for nave in mis_naves { if net.owns(nave.id) { net.set_ship(&estado(nave)); } }
net.update(now);
for e in net.events() { /* Joined, Left, Spawned, Owner, Control, Chat, Synced, Disconnected */ }
net.players(now, &mut otros);                                // los demás, interpolados
for id in naves_ajenas { if let Some(s) = net.ship(id, now) { /* aplicar */ } }
```

- `set_player` / `set_ship` **antes** de `update`: los estados se sellan con el `now` de ese `update`.
- Con `ride` puesto, colocar al jugador con `local` y la copia propia de la nave: su `pos` y
  `vel` pueden tener hasta un segundo.
- `ship(id, now)` es la nave como estaba hace ~100 ms (para dibujarla si no se simula);
  `ship_now(id, now, &mut s)` es su última instantánea llevada al presente con su velocidad
  (para corregir una simulación local) y devuelve su edad. `ship_into` es `ship` sin reservar.
- `Event::Control` no incluye los mandos propios; `Event::Chat` sí incluye las líneas propias.
- Lo que llega antes de `Event::Synced` es el pasado (aplicarlo sin sonidos ni avisos).
- `spawn()` no crea nada: la nave existe para la red cuando vuelve `Event::Spawned` con su id.
- Un rechazo o un «no responde» dejan `Status::Failed(motivo)`; `Event::Disconnected` solo sale
  si se llegó a estar dentro.

## En el juego: naves, disparos y daño (`app/src/multi/`) (histórico)

Cada partida simula todo; la red solo la mantiene de acuerdo con las demás.

| Fichero | Qué hace |
|---|---|
| `multi/mod.rs` | `Multi`: lo que el juego hace con la red cada frame (`receive` antes del mundo, `send` después) |
| `multi/told.rs` | lo que las partidas se dicen aparte de los estados: un byte de tipo y sus campos, todos en una tabla (`CONTROL`, `ACT`, `SPAWN`, `STRIKES`, `SEEN`) |
| `multi/follow.rs` | cómo una copia sigue a lo que simula otro (`steer`) y junto a qué nave se cuenta a quien flota (`pick`) |
| `multi/tests.rs` | la mesa de pruebas: varias partidas enteras contra un servidor real en una red en memoria |

### Naves ajenas

- **Guiadas, no puestas.** El dueño manda su nave en el marco del mundo. Nuestra copia sigue
  simulándose (sus mandos también llegan) y, antes de que el mundo avance, se la lleva hacia donde
  dice el dueño con una semivida de 80 ms, igual a cualquier fps (`follow::steer`): ningún salto de
  un frame a otro, y quien va dentro va con ella. Solo una copia muy lejos (> 40 m o > 0,6 rad: una
  nave puesta de golpe) se pone allí de una vez; y una copia que el mundo no simula a fondo (lejos:
  `SimLevel::Coarse`/`Dormant`) se pone exacta, con su reloj al del mundo.
- **Con la aceleración de su dueño.** A la copia se le da además lo que acelera la nave de verdad
  (de sus dos últimos estados, `Client::rigid_speeding`, menos la gravedad, que su física ya pone):
  una nave que esquiva a 7 g no se queda un metro atrás en cada viraje; la corrección solo quita lo
  que sobra (formación maniobrando: 3–6 cm con los relojes de acuerdo).
- **Sus articulaciones**, como las tiene el dueño (`set_joints`), puestas en la estructura en el
  acto (`Ship::pose_now`), no en el próximo tic lento de una nave lejana.
- **Lo que ningún marco quita**: dos partidas dicen dónde está algo en el mismo instante solo tan
  bien como coinciden sus relojes, y a 7,8 km/s cada milisegundo son 8 m. Con el reloj de arriba
  coinciden en 1–4 ms; el error es estable y suave (no tiembla), y las pruebas lo comprueban contra
  esa cota física (velocidad relativa × desacuerdo de los relojes), no contra un número a ojo.

### Un solo sistema de proyectiles (`blasts.rs`)

Todo lo que se dispara o estalla, lo dispare quien lo dispare —el lanzador en la mano, un cañón o
un lanzamisiles de una nave, una tecla de pruebas, un guion—, es un `Launch` y sale por
`Blasts::launch`; no hay otra manera:

| Campo | Qué es |
|---|---|
| `what: What` | qué de los datos: `Shot(i)` (`shots.jsonc`: vuela, o golpea al momento si no tiene velocidad), `Missile(i)` (balístico), `Guided(i)` (`guiados.jsonc`), `Decoy(i)` (`senuelos.jsonc`), `Boom(i)` (una explosión de `explosions/`) |
| `from`, `dir`, `speed` | de dónde sale, hacia dónde y su velocidad propia |
| `vel` | la velocidad de lo que lo soltó en ese punto (giro incluido: `velocity_at`) |
| `target` | lo que sigue un guiado |
| `by` | la estructura de la que sale: el marco en que se cuenta (y lo que un guiado no puede golpear antes de armarse) |

**Un arma nueva es solo datos.** Una entrada en `shots.jsonc` (o en `guiados.jsonc`,
`senuelos.jsonc`, `missiles.jsonc`, `explosions/`) y quien la dispare (`gear.jsonc` para la mano,
el componente del arma de una nave): ni `blasts` ni la red saben qué arma era. `Blasts::every()`
lista todo lo que se puede lanzar y `Blasts::what(nombre)` lo busca por su nombre.

### Lo que viaja de cada proyectil (`told::SEEN`)

| Mensaje | Cuándo | Cómo | En qué marco |
|---|---|---|---|
| `Seen::Launch { tag, launch }` | al lanzarlo | fiable, en orden | la estructura `by` si la tienen todas; si no se dijo, la nave en que va o junto a la que flota quien disparó; si no, el mundo |
| `Seen::End { tag, what, at, dir, vel, on, extra }` | donde acabó (incluido lo que golpea al momento) | fiable, en orden (tras su `Launch`) | la estructura golpeada `on`, tal como es ahora en cada partida |
| `Seen::Track { tag, pos, vel, push }` | cada 0,1 s mientras vuela un guiado (el primero, tras su primer paso) | suelto: el siguiente lo repite | el mundo |

En un marco, las posiciones van como desplazamientos en él (girados con él), las direcciones
giradas y las velocidades como lo que añaden a la suya en ese punto (giro incluido). El que lo
recibe lo pone junto a **su** copia de esa estructura tal como estaba entonces y lo lleva hasta
ahora: desde la bodega abierta de una nave a 7,8 km/s, el cohete sale de la boca en todas las
partidas (medido: 0,000 m).

Los mensajes vistos se leen **después** de llevar las copias a donde dice su dueño (`receive`
los guarda y los lee tras `follow`): un final cae sobre el casco ya en su sitio.

### En las demás partidas

- El proyectil vuela igual (su número lleva la marca `FOREIGN`) y **no decide nada**: lo que
  golpea aquí no cuenta y desaparece sin explosión; acaba donde su partida dice que acabó, y su
  explosión sale ahí, sobre la copia de lo que golpeó. Lo que todas ven estallar es donde se hizo
  el daño (medido: < 1 cm en el marco de la nave golpeada).
- Un guiado ajeno se vuela al llegar desde donde salió durante lo que tardó el aviso
  (`Flight::catch_up`, con su buscador: así sale con lo que ya había acelerado). Hasta su primer
  `Track` usa su buscador; desde entonces no: lo lleva el empuje que cuenta su dueño
  (`Guided::led`), y en cada aviso se pone donde está el de verdad (si está a menos de 10 m, de
  golpe: no se ve; si no, a medio camino cada vez).
- **Las armas de una nave las dispara solo su dueño** (`tactics::fire`): las copias no disparan;
  lo que disparan les llega contado.

### El daño: el mismo en todas, bit a bit

- **Golpes con eco.** Las estructuras que tienen todas las partidas son `shared`: un impacto
  decidido aquí no se aplica, se encola (`Builds::strikes`, `Strike::{Hit, Blow}`) y se manda con
  `tell_all`; todas, también la que lo mandó, lo aplican cuando el servidor lo reenvía, en su
  orden y con la semilla del mensaje (`strike_done`).
- **Con la postura de quien lo decidió.** El daño depende de dónde está cada pieza, y lo
  articulado (una antena que gira, una pata que se recoge, una compuerta) no está en la misma
  postura en todas las partidas en el mismo instante. El golpe viaja con los huesos de lo golpeado
  tal como estaban donde se decidió (`Structure::bones`, 28 bytes por hueso, una vez por mensaje)
  y cada partida lo aplica con esa postura y vuelve a la suya (`Structures::posed`). Vale para
  cualquier estructura articulada, no solo naves.
- **Lo que depende de lo que cada partida simula por su cuenta** —si una pieza revienta o no
  según lo que llevaba dentro— lo dice una sola: el dueño de la nave golpeada
  (`Structure::owned`), o, si es del escenario, quien decidió el golpe; y lo cuenta como cualquier
  explosión. Las demás solo lo ven estallar. Igual que las máquinas de una nave ajena: lo que
  revienta o arranca el aire en ella lo dice su dueño.
- **Los trozos también se comparten.** Cada estructura con nombre en la red lleva un linaje
  (`Structure::lineage`: la nave por su número, lo del escenario por su id); un trozo que se
  desprende recibe el de su padre mezclado con su orden entre los que se le han desprendido
  (`Structures::child_of`), igual en todas las partidas que lo rompieron igual, y queda compartido.
  La red lo nombra así (`Named::Piece`): un golpe sobre un trozo también va con eco.

### Cómo se nombra cada cosa en la red (`told::Named`)

| Nombre | Qué | Código |
|---|---|---|
| `Ship(k)` | una nave por su número en la red | `k·4 + 1` |
| `Built(id)` | lo que el escenario puso (la misma id en todas) | `id·4` |
| `Piece(l)` | un trozo de cualquiera de ellas, por su linaje (61 bits) | `l·4 + 2` |

### Otros

- **Quien flota junto a una nave** se cuenta en el marco de esa nave (`ride` + `local` +
  `flag::BESIDE`): sin eso se le dibujaría donde estaba hace 100 ms (780 m atrás en órbita).
- **Puertas y anclajes** accionados a mano (`aboard::Act`) viajan como `ACT`, fiables.

## Pruebas

`cargo test -p lunar-net -p luna-servidor -p lunar-play`, casi todas con la red en memoria
(deterministas):

| Fichero | Qué comprueba |
|---|---|
| `net/tests/wire.rs` (16) | ida y vuelta de cada tipo, cotas de error, tamaños; 200 000 datagramas de basura y mensajes reales cortados o con bits cambiados en todos los descodificadores, sin un `panic` |
| `net/tests/channel.rs` (9) | 30 % de pérdidas + desorden + duplicados: 1000 fiables llegan una vez y en orden; los no fiables nunca llegan viejos; 5 kB y 64 KiB llegan enteros; latidos y silencio; 70 000 datagramas (los contadores dan la vuelta) |
| `net/tests/session.rs` (15) | 2 y 8 clientes se conocen y lo que dice cada juego llega a la partida del servidor (y lo que ella dice, a cada uno); con 20 % de pérdidas lo fiable llega entero y en orden; versión, escenario y protocolo distintos rechazados con su motivo; servidor lleno; cliente que desaparece; nombres; expulsar, decir, cerrar; reconexión desde la misma dirección; basura contra un servidor en marcha; lo más largo que se puede decir llega entero y lo más largo no se manda |
| `net/tests/udp.rs` (3) | servidor y 2 clientes por UDP real en `127.0.0.1:0` |
| `server/src` (10), `server/tests/programa.rs` (3) | ajustes, órdenes, fechas; el `.exe` arrancado de verdad: entra un jugador y anda, «salir» guarda, se arranca y retoma, se vuelve con la clave; 3 000 datagramas basura |
| `play/src/keep.rs` | las ranuras: la más nueva entera se retoma, la rota o de otra versión se aparta, se escribe sobre la más vieja |
| `play/tests/online.rs` (31), `local.rs` (2), `load.rs`, `interest.rs` | la partida por red, la propia (se guarda al cerrar y se vuelve al mismo cuerpo), muchos jugadores, el interés contra mirarlo todo: ver [`PLAN_AUTORITATIVO.md`](PLAN_AUTORITATIVO.md), «Avance» |

Las pruebas del relevo (`app/src/multi/tests.rs`: formaciones a 7,8 km/s, armas de los datos una a
una, guiados, tiradores a la vez) se fueron con él; lo que probaban se prueba por el servidor que
tiene la partida a medida que llega cada fase (lo que falta, en `PENDIENTES.md`).

## Lo que no hace (todavía)

Lo que falta de cada fase está en [`PLAN_AUTORITATIVO.md`](PLAN_AUTORITATIVO.md), «Avance»; lo
que más se nota al jugar:

- **Cerrar la ventana del servidor** en Windows lo para sin guardar (Ctrl+C y «salir» guardan).
- **Quien está en medio desde el saludo** (contesta como si fuera el servidor) podría leer: la
  conexión va cifrada, pero sin conocer de antemano la clave del servidor no se sabe con quién.
- **Los jugadores no tienen vida**: no hay que rebobinar el mundo para ver a quién se dio
  (compensación de retraso); el día que la tengan, va en el servidor (`PLAN_AUTORITATIVO.md` §6).

## Investigación

Solo enlaces abiertos y leídos para este trabajo.

| Qué | De dónde | Qué se tomó | Dónde |
|---|---|---|---|
| Acuses redundantes | [Glenn Fiedler, *Reliable Ordered Messages*](https://gafferongames.com/post/reliable_ordered_messages/); [*Reliability and Congestion Avoidance over UDP*](https://gafferongames.com/post/reliability_ordering_and_congestion_avoidance_over_udp/) | Cabecera con secuencia de 16 bits, último recibido y 32 bits de anteriores: cada acuse va en ~33 datagramas. Comparar secuencias con vuelta. | `channel.rs` |
| Mensajes fiables sobre acuses de paquete | [Fiedler, *Reliable Ordered Messages*](https://gafferongames.com/post/reliable_ordered_messages/) | El mensaje se queda en cola y se incluye en datagramas hasta que se acusa uno que lo llevó; no pasar nunca de la ventana del receptor; búferes circulares indexados por secuencia. | `channel/outgoing.rs`, `channel/incoming.rs` |
| Trozos de un mensaje largo | [Fiedler, *Sending Large Blocks of Data*](https://gafferongames.com/post/sending_large_blocks_of_data/) | Trozos de 1024 bytes, reenvío por tiempo sin acuse. (Aquí cada trozo es un mensaje fiable más, sin acuse propio por trozo.) | `channel/outgoing.rs` (`FRAGMENT`) |
| Espera antes de reenviar | [RFC 6298](https://www.rfc-editor.org/rfc/rfc6298) | `SRTT` con 1/8, variación con 1/4, espera = `SRTT + 4·var`. Sin el mínimo de 1 s de TCP: suelo de 50 ms. | `channel/rtt.rs` |
| Medir el RTT descontando la espera del otro | [RFC 9000 §13.2.5](https://www.rfc-editor.org/rfc/rfc9000.html); [RFC 9002 §5](https://www.rfc-editor.org/rfc/rfc9002.html) | El acuse dice cuánto esperó; solo mide el paquete más nuevo recién acusado. | `channel.rs` (`acked`, byte de espera) |
| Entrada con reto y relleno | [Fiedler, *Client Server Connection*](https://gafferongames.com/post/client_server_connection/); [RFC 9000 §8](https://www.rfc-editor.org/rfc/rfc9000.html) | Petición, reto, respuesta: el servidor no guarda nada hasta que el cliente demuestra que recibe en su dirección. La petición, más larga que cualquier respuesta. Número por conexión; despedida repetida. | `server/join.rs`, `proto.rs` (`HELLO_SIZE`), `client/send.rs` |
| Dibujar a los demás en el pasado | [Fiedler, *Snapshot Interpolation*](https://gafferongames.com/post/snapshot_interpolation/); [Gabriel Gambetta, *Entity Interpolation*](https://www.gabrielgambetta.com/entity-interpolation.html) | Búfer de instantáneas con su hora; retraso suficiente para tener siempre hacia dónde interpolar; no extrapolar más que un poco. | `snap.rs`, `client/delay.rs` |
| Cuaternión «los tres menores» y posición acotada | [Fiedler, *Snapshot Compression*](https://gafferongames.com/post/snapshot_compression/) | Omitir la componente mayor (2 bits de índice), las otras en ±1/√2; bit de «en reposo» que ahorra las velocidades. | `quant.rs`, `game/ship.rs` (`MOVING`) |
| Precisión para un estado que se simula | [Fiedler, *State Synchronization*](https://gafferongames.com/post/state_synchronization/) | 4096 valores por metro y 15 bits por componente de cuaternión; qué se manda de un cuerpo rígido. | `game.rs` (`POS_UNITS`), `quant.rs` |
| Autoridad por interacción, árbitro | [Fiedler, *Networked Physics in Virtual Reality*](https://gafferongames.com/post/networked_physics_in_virtual_reality/) | Cada objeto con un dueño que lo simula; un árbitro decide los conflictos y el resto converge. Aquí el árbitro es el servidor y la regla es «quien está a los mandos». | `server/world.rs` |

*Source Multiplayer Networking* (Valve) no se pudo abrir (la página devolvió 403): no se cita
nada de ella.

**Hecho sin copiar de nadie** (salió de medir): mandar las articulaciones aparte y solo cuando
cambian; no mandar lo que no cambia y marcar el primer estado tras un silencio; retraso de
interpolación que se adapta; no mandar nada por el canal hasta que el cliente contesta al
`Welcome` (sin eso la puesta al día de 81 kB costaba 222 kB).
