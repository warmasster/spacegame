# Tiempos: lo que tarda cada cosa a bordo, y su presupuesto

Fernando (2026-10-04): «QUE TODO FUNCIONE BIEN Y SEA DIVERTIDO NO ESPERAR UNA ETERNIDAD» y, del
compresor de aire, «si tienes que meter una eficiencia inventada dale, no pasa nada».

Nada a bordo hace esperar una eternidad, y una prueba lo guarda para toda nave, las de ahora y las
que vengan.

## Cómo funciona

- **Registro de procedimientos** (`assets/defs/procedimientos.jsonc`, `crates/ship/src/procedures.rs`):
  cada procedimiento es datos — a qué nave se aplica (a toda la que tenga lo que nombra), sus pasos
  (mandos por su id o por la señal que escriben), la condición que lo da por hecho (en el lenguaje
  de las lámparas), su presupuesto en segundos (`max`) y lo que va antes (`tras`). Con `cada` un
  procedimiento es una familia (uno por puerta, por motor, por anclaje...).
- **La prueba** (`crates/ship/tests/procedimientos.rs`) hace cada procedimiento en cada nave a la
  que se aplica, desde la nave fría, y falla si alguno pasa de su presupuesto **o si la nave tiene
  algo que tarda y que ningún procedimiento ejercita** (toda articulación con actuador y todo
  modelo de máquina con arranque o transición): una cosa lenta nueva no se cuela.
- No corre en el juego mientras nadie lo pida. El mismo registro servirá para listas de
  comprobación vivas (pendiente: la tableta).

## La tabla (segundos de nave; medida por la prueba)

| Procedimiento | Abejorro | Alcotán | Cachalote | Presupuesto |
|---|---|---|---|---|
| Dejar la nave fría y a oscuras | 0,3 | 2,7 | 2,7 | 5 |
| Dar energía desde frío (baterías, buses, convertidores) | 4,0 | 5,4 | 5,4 | 8 |
| Arrancar la APU | – | 8,0 | 8,0 | 12 |
| Reactor: en línea | – | 14,8 | 14,8 | 20 |
| Reactor: cubre el consumo de la nave | – | 44,3 | 47,6 | 60 |
| Reactor: a plena potencia | – | 62,3 | 62,5 | 90 |
| Cargar las baterías del 50 al 90 % con el reactor a plena potencia | – | 202,2 | 209,3 | 300 |
| Desplegar los radiadores | – | 4,9 | 4,9 | 8 |
| Recoger los radiadores | – | 4,8 | 4,9 | 8 |
| Sacar el mástil de la antena | – | 5,3 | 5,3 | 8 |
| Recoger el mástil de la antena | – | 5,2 | 5,2 | 8 |
| Hidráulica a presión y acumulador cargado, con una bomba | – | 4,1 | 4,1 | 8 |
| Subir el tren (en vuelo) | – | 4,3 | 4,4 | 8 |
| Bajar el tren y tres verdes (en vuelo) | – | 5,8 | 5,9 | 8 |
| Cerrar la rampa | – | 8,5 | 9,6 | 12 |
| Abrir la rampa | – | 7,3 | 7,9 | 12 |
| Abrir una puerta (*, desde *) | – | 1,9 | 1,9 | 4 |
| Cerrar una puerta (*, desde *) | – | 3,4 | 3,4 | 4 |
| Motor de frío a empuje disponible | 3,0 | 4,7 | 4,7 | 8 |
| Góndolas de VTOL a crucero | – | 3,4 | 6,5 | 10 |
| Góndolas de crucero a VTOL | – | 3,3 | 6,3 | 10 |
| De a oscuras a maniobra: las toberas responden | 0,4 | 5,4 | 5,4 | 8 |
| Giróscopos de parados a su régimen | 8,0 | – | 10,0 | 12 |
| Ventear un compartimento (*) | – | 15,6 | 15,6 | 20 |
| Represurizar un compartimento desde el vacío (*) | – | 28,0 | 27,7 | 40 |
| Igualar dos compartimentos con la válvula de mano (*) | – | 15,9 | 7,0 | 20 |
| Compresor: vaciar la bodega al depósito | – | 102,8 | – | 120 |
| Compresor: devolver el aire del depósito a la bodega | – | 35,1 | – | 45 |
| Compresor: de la bodega a la cabina (con prisa) | – | 20,0 | 15,6 | 45 |
| Compresor: vaciar el puente al depósito | – | 33,2 | 33,2 | 45 |
| Depurador: de CO2 ALTO (1,5 kPa) a aviso apagado (1 kPa) en la cabina | – | 82,6 | 82,6 | 100 |
| Grúa: el puente de punta a punta | – | – | 11,0 | 12 |
| Grúa: el carro de banda a banda | – | – | 6,3 | 12 |
| Grúa: el izado de arriba abajo | – | – | 4,3 | 8 |
| Imán: agarrar, soltar y listo para agarrar otra vez | 3,4 | – | 2,6 | 6 |
| Soltar un anclaje de carga | – | 0,3 | 0,3 | 2 |
| De fría y a oscuras a lista para despegar, siguiendo el manual | – | 27,4 | 27,4 | 60 |
| De frío y a oscuras a listo para despegar (un solo bus) | 8,0 | – | – | 20 |

137 procedimientos hechos, 1744 s de nave simulados en 7.2 s de reloj (cargar las naves: 8.8 s).

Exentos, con su porqué (en los datos):

- cachalote · represurizar:bodega: su bodega es de 700 m3: no cabe en sus botellas (540 kg de aire). Se trabaja al vacío
- cachalote · igualar:bodega: igualar con su bodega es vaciar la cabina en 700 m3 (queda a 5 kPa): no es una maniobra, es una avería
- cachalote · compresor_vaciar: el depósito de aire recuperado (60 kg) se llena con la décima parte de su bodega: allí el compresor es para cabina y puente
- cachalote · compresor_devolver: el depósito de aire recuperado (60 kg) se llena con la décima parte de su bodega: allí el compresor es para cabina y puente
- cachalote · repres_o2: su bodega es de 700 m3: no cabe en sus botellas (540 kg de aire). Se trabaja al vacío
- cachalote · repres_n2: su bodega es de 700 m3: no cabe en sus botellas (540 kg de aire). Se trabaja al vacío

## Antes y ahora (lo que más se notaba)

| Qué | V35 | V36 |
|---|---|---|
| Reactor en línea | 45 s | 15 s |
| Reactor cubre el consumo | ~6 min | 44 s |
| Reactor a plena potencia | ~12 min | 62 s |
| Compresor: vaciar la bodega del Alcotán | 34 min | 103 s |
| Compresor: devolver el aire | 3 min | 35 s |
| Igualar cabina y bodega con la válvula de mano | 86 s | 16 s |
| De fría y a oscuras a lista para despegar (Alcotán) | sin medir | 27 s |

## Qué es inventado

Los tiempos mandan sobre el realismo (memoria «divertido, sin esperas»). Están doblados para el
juego, y dicho en los datos con `BENT FOR PLAY`: la masa térmica y el periodo del reactor, el
caudal y el rendimiento del compresor de aire (a 15 kW, lo que da el bus), el paso de las
válvulas de mano, la capacidad del depurador y las velocidades de varios actuadores. Se conserva
la forma de cada cosa (el compresor va más despacio según se vacía el origen, el reactor sigue
pudiendo dispararse si se abusa, los disyuntores saltan).

