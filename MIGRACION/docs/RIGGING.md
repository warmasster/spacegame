# Rigging: el astronauta (y cómo armar otro modelo igual)

`assets/models/astronauta.glb` es el traje EVA con esqueleto, dedos incluidos. Es el cuerpo del
jugador (en primera persona se ven brazos, manos, piernas y botas) y lo mueve la cinemática inversa
del juego, así que lo que importa es **cómo se deforma**: manos, muñecas, codos, hombros, caderas,
rodillas y tobillos. Todo sale de un generador (Blender 5.2, sin ventana); nada se retoca a mano.

## Reconstruir

Desde `MIGRACION` (Blender siempre con `-b`):

```
B="/c/Program Files/Blender Foundation/Blender 5.2/blender.exe"
"$B" -b -P tools/modelos/astronauta/hacer.py --            # el .glb (15 s) y su informe
python tools/modelos/astronauta/comprobar.py               # las reglas de abajo, vértice a vértice
"$B" -b -P tools/modelos/astronauta/poses.py --            # las fotos de las poses (30 s)
python tools/modelos/astronauta/inspeccionar.py --manos    # huesos, mallas, materiales, manos
```

`hacer.py`: `--salida <ruta>`, `--sin-sombra` (sin sombra horneada: 5 s, para probar formas o
pesos), `--curl 30,45,30` (flexión en reposo de los dedos), `--detalle` (mapa de normales de la
tela y dibujo del parche: el juego no los usa, **por defecto no hay ninguna textura**), `--vista`.

| Fichero (`tools/modelos/astronauta/`) | Qué hace |
|---|---|
| `hacer.py` | Entrada: construye, hornea la sombra, exporta, ordena los huesos, informa. |
| `astronaut.py` | El traje: esqueleto, tela (modificador Skin), casco, mochila, caja del pecho, botas, anillos, y el atado de cada pieza a sus huesos. |
| `dedos.py` | Las manos: articulaciones de cada dedo, huesos, malla del guante y sus pesos. |
| `pesos.py` | Lo que se corrige por regla en los pesos automáticos del cuerpo. |
| `suitkit.py`, `suitdetail.py` | Utillaje de modelado, materiales, horneado, exportación. |
| `glb.py`, `inspeccionar.py`, `comprobar.py` | Leer el `.glb` sin Blender: informe y reglas. |
| `poses.py` | Poses de prueba y fotos. |

## Esqueleto (52 huesos, un solo `skin`)

Los 22 de siempre, con los mismos nombres, padres y posiciones de reposo, y en el mismo orden de
articulaciones (0–21); después los 30 de los dedos (22–36 la mano izquierda, 37–51 la derecha):

```
root
└ pelvis
  ├ spine ─ chest ┬ neck ─ head
  │               ├ clavicle.L ─ upperarm.L ─ forearm.L ─ hand.L ┬ thumb.01.L ─ thumb.02.L ─ thumb.03.L
  │               │                                              ├ index.01.L ─ index.02.L ─ index.03.L
  │               │                                              ├ middle.01.L ─ …  ├ ring.01.L ─ …
  │               │                                              └ pinky.01.L ─ pinky.02.L ─ pinky.03.L
  │               └ clavicle.R ─ … ─ hand.R ─ (los mismos quince, con .R)
  ├ thigh.L ─ shin.L ─ foot.L ─ toe.L
  └ thigh.R ─ shin.R ─ foot.R ─ toe.R
```

`root`, `neck` y `head` no deforman nada (el casco va rígido con `chest`). La lista exacta con la
cabeza de cada hueso la imprime `inspeccionar.py`; el exportador de Blender los escribe según
recorre el árbol y `glb.ordenar_huesos` los deja en ese orden fijo (vértices y matrices inversas
incluidos).

## Convenios

- **Ejes del fichero**: Y arriba, el personaje mira a +Z, `.L` está en +X, metros, escala 1. (En
  Blender: Z arriba, mira a −Y.)
- **Reposo**: pose en A relajada, brazos caídos y algo abiertos, palmas hacia los muslos, pulgares
  hacia delante, dedos medio cerrados (30°/45°/30° en nudillo, articulación media y última).
- **Ejes locales de un hueso**: +Y a lo largo del hueso. En el cuerpo, +Z mira hacia delante y +X
  es la bisagra principal. En los dedos, +Z mira hacia donde el dedo se cierra y **+X es la
  bisagra: girar +θ sobre +X cierra el dedo, en las dos manos**.
- **Plano de cada dedo**: sus tres huesos están en un plano *por construcción* (cada dirección es
  la anterior girada sobre el mismo eje; la última falange del pulgar se proyecta al plano de las
  otras dos). La bisagra es la normal: `n = normalizar((p02 − p01) × (p03 − p02))`, y un giro
  positivo (mano derecha) sobre `n` cierra. La punta no es una articulación: está a lo largo del
  +Y de `.03` (índice 24 mm, corazón 27, anular 26, meñique 21, pulgar 30).
- **Manos** (reposo, ejes del fichero; la derecha es la izquierda con x cambiada de signo):

  | | mano L |
  |---|---|
  | muñeca (`hand.L`) | (0.4050, 0.9350, 0.0000) |
  | nudillos: índice … meñique (`*.01.L`) | (0.4222, 0.8201, 0.0498) … (0.4234, 0.8134, −0.0279) |
  | centro de la línea de nudillos | (0.4228, 0.8167, 0.0107) |
  | centro de la palma (plano de los huesos) | (0.4139, 0.8759, 0.0053); la piel, 27.7 mm hacia la normal |
  | normal de la palma | (−0.9888, −0.1494, −0.0029): hacia el cuerpo |
  | muñeca → nudillos | (0.1485, −0.9849, 0.0891) |
  | meñique → índice | (−0.0159, 0.0856, 0.9962) |

  Un mango de radio `r` agarrado con el puño queda con el eje a lo largo de «meñique → índice» y
  el centro en `nudillos − 0.012·(muñeca→nudillos) + (0.0205 + r)·normal` (`poses.sitio_mango`).

## Materiales (contrato con el juego)

`AnoBlue, AnoRed, SuitBody, SuitStripe, Boot, Display, Glove, GloveGrip, Helmet, HelmetDark,
HelmetInner, Hose, Lamp, Metal, MetalDark, NeckRing, NeckRingBand, Patch, Rubber, Strap,
SuitFabric, SuitHard, Visor`. Sin texturas; la sombra horneada va en `COLOR_0`; normales suaves.

En primera persona el juego oculta por nombre de material **el casco** (`Helmet`, `HelmetDark`,
`HelmetInner`, `Visor`, `Lamp`) y **el aro del cuello** (`NeckRing`, `NeckRingBand`: mismo aspecto
que `SuitHard` y `AnoBlue`, pero materiales propios que no lleva nada más; mirando hacia abajo el
aro taparía la vista). Toda pieza que sólo sea del casco o del aro debe llevar uno de esos
materiales y ninguna otra pieza puede llevarlos. El muñón del cuello de la tela es una tapa plana
en y = 1.509 (bajo el borde alto del aro, 1.523; el generador la mantiene ≤ 1.515) con la sombra
igualada en oscuro: con casco y aro ocultos es la boca oscura del cuello, no asoma nada.

## Cómo se hacen los pesos

Cada vértice lleva como mucho 4 huesos y suman 1.

- **Tela del cuerpo** (`Body`): pesos automáticos de Blender (calor de huesos) contra los 16 huesos
  del cuerpo que deforman. **Los huesos de los dedos nacen sin deformar** y se activan después de
  atar el cuerpo, así que la tela no los conoce. Luego `pesos.py`:
  la tela bajo el aro del hombro (`ScyeBearing`, rígido con `upperarm`) sigue sólo a `upperarm`
  —si no, el brazo se sale del aro al levantarlo—, se quitan los pesos menores del 1 % y se limita
  a 4. Qué vértices son «del brazo» no se decide por distancia: el grafo del modificador Skin lleva
  etiquetas (`T_arm.L`…) que el modificador y la subdivisión reparten por la malla.
- **Piezas duras**: rígidas con un hueso (casco, mochila, caja y mangueras con `chest`; aro de la
  cintura con `spine`; aros de hombro con `upperarm`; aro de la muñeca y lista de comprobación con
  `forearm`; bolsillo con `thigh.R`; suelas con `foot`; correa con `shin`).
- **Degradados**: el puño del guante (`forearm` bajo el aro de la muñeca y por encima, `hand` en
  su borde: 3 cm de transición), la caña de la bota (`foot` → `shin`), la puntera (`foot` → `toe`).
- **Guantes** (`dedos.py`), sin pesos automáticos: la malla es una jaula de cuadriláteros (una
  losa para la palma, un tubo por dedo, un anillo en cada articulación sobre su plano bisector)
  redondeada con dos niveles de Catmull-Clark. La jaula marca de qué dedo es cada vértice
  (`T_<dedo>`), y con eso:
  - en cada articulación el peso pasa de un hueso al siguiente con un `smoothstep` de la distancia
    al **plano bisector** (±8 mm en las de los dedos; de −12 a +14 mm en los nudillos);
  - más allá de donde los dedos se separan, un vértice sólo lleva huesos de **su** dedo;
  - en la palma manda `hand`; sólo en la membrana entre nudillos se reparten dos dedos vecinos
    (±5 mm a cada lado de la línea que los separa), para que al cerrar un dedo solo no se rasgue;
  - la base del pulgar mezcla `hand` y `thumb.01` a lo largo del cono que lo une a la palma.

`comprobar.py` verifica todo esto sobre el `.glb` (y que los lados son simétricos, que cada dedo es
plano y que su +X local es la bisagra) y falla si algo no se cumple.

## Comprobar poses

`poses.py` carga el `.glb` (lo que se juzga es lo exportado; `--construir` lo arma en memoria, más
rápido para probar pesos), pone cada pose y saca fotos sobre fondo gris neutro:
`out/modelos/astronauta_<pose>.png` (todas las vistas de la pose), cada vista suelta en
`out/modelos/astronauta_vistas/` y la hoja de contactos `out/modelos/astronauta_poses.png`.
`--alambre` dibuja la malla de los guantes, `--sombra` enseña la sombra horneada, `--lista` las
poses. Las vistas `primera` y `abajo` son los ojos del jugador, con casco y aro del cuello ocultos.

Poses que hay: `reposo`, `herramienta` (hombros 80°, codos 80°), `herramienta_baja`,
`brazo_cruzado`, `extremos` (brazo arriba, codo a 130°, brazo en cruz, pierna abierta, torso
girado), `zancada`, `sentadilla` (rodillas 100°), `tobillos` (±25°), `munecas`, `munecas_giro`,
`manos`, `puno` (cerrado sobre un mango de 4 cm), `mano_abierta`, `gatillo`.

Una pose es un diccionario `hueso → [(eje, grados), …]` en `POSES`; añadir una es añadir una
entrada con sus vistas. Cada giro es del hueso respecto a su padre, expresado en ejes del modelo
en reposo (los del juego): `'X' 'Y' 'Z'`; `'H'` la bisagra del dedo (+ cierra); `'N' 'A' 'S'` la
normal de la palma, el largo y el ancho de esa mano (escritos para la derecha, espejados en la
izquierda); un vector cualquiera; o `('->', dirección)` para apuntar el hueso sin torcerlo.
`ambos()` copia una pose del lado izquierdo al derecho. El puño no es de números a ojo:
`agarre(rig, lado, diámetro)` tumba cada falange sobre el mango (tangente a él).

## Lo que queda

- Sin huesos de torsión: girar la mano 60° sobre el antebrazo se lo come el puño del guante (es
  de revolución y no se nota), pero un giro grande repartido por el antebrazo no existe.
- A 40° de muñeca el borde del puño del guante entra un poco en el aro metálico.
- Las articulaciones de los dedos doblan como manguera a presión (sin pliegues modelados).
- La sombra horneada es la del reposo (los guantes se hornean solos, para no llevarse la del
  muslo).

## Armar otro modelo de la misma manera

1. **Esqueleto como tabla** (`nombre: cabeza, cola, padre, deforma`), en metros y en reposo. El
   balanceo se fija con una regla (`align_roll`), no a ojo: que +X sea la bisagra.
2. **Tela con el modificador Skin** sobre un grafo de nodos con radio; poner en el grafo grupos
   de vértices que digan de qué miembro es cada nodo: llegan a la malla final y evitan decidir por
   distancias.
3. **Atar**: automático para la tela, *con los huesos que no deban contar puestos a no deformar*;
   rígido o degradado para lo duro; y corregir por regla lo que el automático no sabe (`pesos.py`).
4. **Manos**: copiar `dedos.py`. Cambiar `FINGER_X`, `FINGER_LEN`, `SEG`, `CURL`, `THUMB_*` y los
   anillos de la jaula; los huesos salen de las mismas cadenas que la malla (`chains`), así que
   caen exactos en las articulaciones, y los pesos salen de los planos bisectores.
5. **Exportar** con `suitkit.export_glb` (un skin, todos los huesos, `COLOR_0`, sin animaciones) y
   fijar el orden de articulaciones con `joint_order`.
6. **Mirar**: añadir las poses que el juego vaya a pedir al diccionario de `poses.py`, sacar las
   fotos y leerlas; `comprobar.py` para lo que se puede contar.
