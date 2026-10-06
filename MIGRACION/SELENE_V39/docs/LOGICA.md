# Lógica de las naves: qué se prueba y cómo leer un fallo

Fernando (2026-10-04): «HAZ TESTS DE LÓGICA ELECTRÓNICA ETC DE LA NAVE, QUE TODO FUNCIONE BIEN».

Nueve juegos de pruebas (`crates/ship/tests/logica_*.rs`, 38 pruebas) que recorren **toda nave de
la biblioteca** sin nombrar ninguna: lo que comprueban sale de las redes, los paneles y las
máquinas de cada nave tal como se montan. Una nave nueva las pasa todas el día que se deja su
archivo en `assets/defs/ships`; un circuito nuevo no necesita una línea aquí. Lo común está en
`crates/ship/tests/comun/mod.rs`.

Correrlas: `tools/cargo.ps1 test --release -p lunar-ship --test logica_mandos` (o todas con
`-p lunar-ship`). Cada fallo dice **qué nave, qué mando, disyuntor o señal** está mal, en
castellano, para poder ir a por ello.

| Archivo | Qué prueba |
|---|---|
| `logica_topologia.rs` | El cableado: todo puerto tiene camino a algo que da o guarda lo que lleva su red; no hay redes sin puertos ni nudos sueltos; cada carga eléctrica va detrás de un disyuntor suyo y no se alimenta por dos lados; cada interruptor de cada red lo acciona algo (una mano, la lógica de la nave) o dice por qué no; abrir un interruptor corta exactamente lo que el plano dice. |
| `logica_interruptores.rs` | Cada disyuntor, contactor y válvula que acciona una mano: abierto, lo de detrás se queda sin alimentación y nada más; la tensión de los nudos de detrás cae a cero; cerrado, vuelve todo como estaba. Lo que hay detrás de cada uno se deduce del plano de la red. |
| `logica_energia.rs` | Conservación en toda red y todo medio (corriente, propelente, gas, hidráulico, refrigerante, calor, aire): lo que dan los productores menos lo que entra en los depósitos es lo que reciben los consumidores más lo que se fuga. El balance `energia.*` es esa misma cuenta; un convertidor entre barras no retroalimenta ni cuenta dos veces. Baterías: dan con déficit, toman con superávit, nunca salen de 0..1. Lo que pierden depósitos y botellas es lo que recibieron sus consumidores. |
| `logica_escasez.rs` | Cuando no hay para todos: se reparte por prioridad (nada de menor prioridad recibe mientras a una mayor le falta), sin parpadeo de un tic a otro, y al volver la alimentación vuelve todo. Disyuntores: lo que mide cada uno es lo que toma su circuito; con todo lo que una tripulación puede encender a la vez ninguno está cerca de su nominal; cada uno salta por su curva, lo dice y se rearma a mano. |
| `logica_mandos.rs` | Ningún mando muerto: todo mando se puede accionar (o dice qué lo retiene), su señal le sigue y algo la lee y hace algo con ella. Las tapas guardan lo que dicen. Todo instrumento lee señales que alguien escribe. Toda regla de lámpara puede ganar y todo aviso puede encenderse y apagarse. Lo que enseña un display es su señal en su unidad; toda página de las pantallas tiene algo que existe. |
| `logica_avisos.rs` | Avisos y alarmas: una nave recién puesta no da ninguno; un apagón levanta lo que vigila sus barras y, vuelto, todo queda como antes; un compartimento agujereado avisa y, reparado, se apaga; un disyuntor sacado se anuncia; el fallo de cada motor, reactor y generador lo vigila una alarma, un aviso o una lámpara. |
| `logica_enclavamientos.rs` | Enclavamientos y secuencias: lo que declaran los datos (con su condición, el mando se niega y dice por qué; sin ella, se mueve) y lo que declaran los modelos de máquina (un motor no arranca sin armar, sin corriente o sin propelente; un generador sin su disyuntor o sin combustible; un reactor sin corriente de control o sin caudal de refrigerante...). |
| `logica_fuzz.rs` | Robustez: todos los mandos de cada nave accionados al azar unos minutos, en tierra y en vuelo, y otra pasada con cosas rompiéndose (máquinas, paneles, chapas, conductos). Nada revienta, ninguna señal es NaN, todo depósito sigue en sus límites, ningún disyuntor salta sin avería, el tic sigue en su presupuesto, y **la misma semilla da la misma nave al final, bit a bit** (de eso vive el multijugador). `LUNA_FUZZ_SEMILLA` y `LUNA_FUZZ_MINUTOS` para otra semilla o más tiempo. |
| `logica_reposo.rs` | Una nave que nadie toca se asienta y deja de estar «ocupada»; el ritmo lento y el dormido llevan las mismas cuentas que el completo; despierta al tocarla; con todo abierto y cerrado nada se gasta. |

Y aparte, del vuelo y la masa:

| Archivo | Qué prueba |
|---|---|
| `vuelo.rs` | Con los motores encendidos y el acelerador sin tocar la nave no se mueve (ralentí); MANTENER la levanta nivelada y la deja quieta bajo cuatro gravedades (0,21 · 0,8 · 1,62 · 2,2 m/s²); la carga sujeta a un lado mueve el centro de masas lo que dice la suma a mano y la nave sigue nivelada. |
| `masa.rs` | La nave pesa lo que pesa en seco más lo que llevan sus depósitos; lo que queman sus motores la aligera en eso; cada depósito lleva lo que cabe en su volumen; con los depósitos vaciados de un lado se sostiene sin girar; las patas la aguantan llena y vacía; despega llena y aterriza vacía en todo cuerpo; pesarla no cuesta nada si nada fluye. |
| `procedimientos.rs` | Cada procedimiento dentro de su presupuesto de tiempo ([`TIEMPOS.md`](TIEMPOS.md)). |

## Lo que encontraron (y cómo quedó)

- El disyuntor de la grúa del Cachalote (400 A) no aguantaba su circuito con todo en marcha
  (17,3 kW a 28 V son 617 A): ahora es de 650 A.
- El balance de energía contaba dos veces lo que pasa por los convertidores (arreglado en V35).
- El centro de masas del Alcotán estaba 1,3 m por delante de sus góndolas y el del Cachalote no
  caía entre sus dos pares: alas recolocadas (ver [`NAVES.md`](NAVES.md), V36).
- Los motores encendidos daban el 35 % de su empuje sin tocar el acelerador: ralentí al 2 %.

## Lo que no cubren

- No miran nada del dibujo (eso son las fotos de guion) ni del sonido.
- El uso al azar no sienta a nadie ni usa herramientas: es la nave sola.
- Los tiempos no se afirman aquí (van en `procedimientos.rs`), para que retocar un tiempo no rompa
  la lógica.
