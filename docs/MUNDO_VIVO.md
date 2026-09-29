# Mundo vivo: economía, facciones y consecuencias

> Visión de diseño (no un plan cerrado). Idea central: **no hay misiones escritas**. El mundo es una
> simulación de números (producción, rutas, miedo, poder) que corre sola. El jugador ve los efectos
> y decide dónde meterse. Cuando se acerca, esos números se convierten en naves, personas y cajas
> reales (niveles de detalle de simulación, ver [AUDITORIA.md §3.3](AUDITORIA.md)).

---

## 1. Las piezas del mundo

Todo lo que existe se reduce a unos pocos tipos de "actores" con estado numérico:

| Pieza | Qué guarda | Ejemplos |
|---|---|---|
| **Lugar** | Población, producción, consumo, almacén, defensa, dueño | Planeta, ciudad, estación, puesto minero |
| **Instalación** | Qué transforma en qué, a qué ritmo, su integridad | Refinería (hielo → propelente), granja (agua + luz → comida), astillero |
| **Facción** | Territorio, tesoro, flota, ideología, líderes, relaciones | Gobierno planetario, corporación, gremio, banda pirata |
| **Personaje** | Rasgos, lealtades, ambición, miedo, riqueza, historia | Un dictador, un capitán mercante, un jefe pirata, un tripulante |
| **Flota / nave** | Dueño, misión, carga, estado, tripulación | Convoy mercante, patrulla, banda pirata, nave de rescate |
| **Ruta** | Origen, destino, riesgo percibido, tráfico | La ruta del hielo entre el polo y la estación orbital |
| **Noticia** | Qué pasó, dónde, quién lo sabe, cuánto se ha difundido | "Refinería de Kora destruida" |

Nada es especial: un dictador es un **personaje** con un cargo en una **facción**. Una guerra es un
**estado de relación** entre dos facciones. Un campo de batalla es un **lugar temporal** lleno de
pecios.

---

## 2. Los motores (lo que hace girar el mundo)

### 2.1. Economía: producción → rutas → precios

- Cada instalación produce y consume. Cada lugar tiene **almacén** y **demanda** (su población
  necesita comida, agua, O₂, combustible, piezas).
- **Precio local = f(escasez)**. Poco stock y mucha demanda disparan el precio.
- Los mercaderes NPC **eligen rutas por beneficio esperado**:

  `beneficio = (precio destino − precio origen) × carga − combustible − riesgo × valor de la carga`

  Si el riesgo sube (piratas, guerra), las rutas se abandonan aunque el beneficio sea alto. Eso
  produce escasez y la escasez sube el beneficio. Así aparecen oportunidades **para quien se atreva**,
  el jugador incluido.

### 2.2. Miedo e incentivo

Cada ruta y cada lugar tienen un **riesgo percibido**, que no es el riesgo real:

- Sube con: ataques recientes, naves desaparecidas, noticias de guerra, piratas vistos.
- Baja con: patrullas, escoltas, tiempo sin incidentes, recompensas publicadas.
- Los NPC deciden según **su** percepción y **su carácter** (un mercader codicioso arriesga más; uno
  prudente, no).

Esto hace que las consecuencias lleguen **tarde y de forma imperfecta**, como en la realidad. Matas al
jefe pirata y los mercaderes tardan semanas en volver, hasta que la noticia se difunde y nadie más
cae en esa ruta.

### 2.3. Poder: facciones que nacen, se dividen y mueren

- Cada facción tiene **legitimidad**, **tesoro**, **fuerza militar** y **cohesión interna**.
- La cohesión baja con derrotas, hambre, impuestos altos y líderes impopulares. Por debajo de un
  umbral hay **cisma**: la facción se parte en dos, cada una con parte del territorio, la flota y
  un líder (el más ambicioso de cada bando).
- **Estilo de gobierno** como parámetros (agresividad, apertura comercial, represión, corrupción).
  Lo decide el líder: si el líder cambia, los parámetros cambian.
- **Guerra** cuando el valor esperado de atacar (recursos del rival, odio, ambición del líder) supera
  el coste (fuerza del rival, cansancio, economía).

### 2.4. Información: las noticias

- Cada suceso importante genera una **noticia** que se propaga por las rutas a velocidad de nave
  (y más rápido si hay retransmisores).
- Los NPC **solo reaccionan a lo que saben**. El jugador también: lee las noticias en las
  estaciones, intercepta comunicaciones con el interceptor de señales, o lo ve con sus ojos.
- Saber algo antes que nadie **vale dinero**: si te enteras de que la refinería cayó, llegas con
  combustible antes de que suba el precio para todos.

### 2.5. Personajes con historia

- Cada personaje importante tiene rasgos (ambición, crueldad, lealtad, codicia, miedo), relaciones
  (odia a, debe a, es leal a) y memoria (quién le ayudó, quién le traicionó).
- Sus decisiones salen de esos rasgos: un general ambicioso tras una derrota del líder se plantea un
  golpe; un jefe pirata con deudas ataca objetivos más arriesgados.
- Los NPC corrientes (tripulantes, soldados) se generan con un origen y una razón para estar donde
  están. No hace falta simularlos a todos: se detallan cuando te acercas y a partir de ahí se
  recuerdan.

---

## 3. Todo en cascada: el ejemplo del dictador

Nada de esto está escrito. Son los motores anteriores reaccionando unos a otros:

1. **Dos planetas de la misma facción**: uno produce comida, el otro combustible. Una mala cosecha y
   los impuestos altos bajan la cohesión.
2. **Cisma**: el gobernador del planeta del combustible (ambicioso, cruel) se declara independiente.
   Guerra civil.
3. **La guerra destruye instalaciones**: la refinería de un bando y las granjas del otro. La escasez
   dispara los precios de la comida y del combustible en ambos planetas.
4. **Las noticias se difunden.** Los mercaderes prudentes abandonan la zona (miedo alto). Los
   codiciosos, y el jugador, ven un margen enorme.
5. **Aparecen oportunidades sin que nadie las cree**:
   - contrabando de comida al planeta bloqueado;
   - escolta a precio de oro para los pocos mercaderes que se atreven;
   - piratería: los convoyes van mal protegidos porque las flotas están en la guerra;
   - carroña: los campos de batalla quedan llenos de pecios con carga, munición y piezas.
6. **El gobernador se convierte en dictador**: represión, reclutamiento forzoso, precios fijados.
   La población lo odia (legitimidad baja), sus generales dudan (lealtad baja).
7. **Alguien puede matarlo**: un rival interno, un agente del otro planeta o el jugador. Nadie te
   da "la misión". Lo sabes por las noticias, un contacto o una recompensa que alguien ha publicado.
   Llegar hasta él es un problema físico: dónde vive, quién lo guarda, cómo entras.
8. **Su muerte cambia los parámetros**: el siguiente líder sale de las personas reales de esa facción.
   Si es un moderado: baja la agresividad, se abre el comercio y el riesgo percibido de las rutas cae
   poco a poco.
9. **La economía se recupera**: los mercaderes vuelven y los precios bajan. Tu negocio de
   contrabando deja de rendir, y el mundo sigue.

Cada paso es una regla simple. La historia sale de que se encadenan.

---

## 4. Maneras de jugar (todas emergen, ninguna es una clase)

| Papel | De qué vive | Qué lo alimenta en la simulación |
|---|---|---|
| **Mercader** | Diferencias de precio | Escasez, rutas abandonadas, noticias tempranas |
| **Contrabandista** | Bloqueos y mercancías prohibidas | Guerras, embargos, leyes de cada facción |
| **Carroñero** | Pecios tras las batallas | Las batallas dejan naves reales a la deriva, con su carga |
| **Pirata** | Robar convoyes | Rutas mal protegidas; te ganas miedo y recompensas por tu cabeza |
| **Asaltante de estaciones** | Robar almacenes, rescates, sabotaje | Estaciones con defensas bajas o en crisis |
| **Escolta / mercenario** | Proteger a otros | Riesgo percibido alto en rutas rentables |
| **Cazarrecompensas** | Contratos sobre personas | Recompensas que publican facciones, víctimas o jugadores |
| **Soldado de facción** | Sueldo, rango, botín | Guerras reales; tu facción puede ganar o desaparecer |
| **Minero / recolector** | Materias primas | Demanda de las instalaciones |
| **Explorador** | Descubrir cosas y vender datos | Señales, pecios antiguos, yacimientos, rutas nuevas |
| **Rescatista** | Salvar naves en apuros | Averías, descompresiones, naves a la deriva |
| **Espía / informador** | Vender información | El sistema de noticias: saber antes vale dinero |
| **Transportista de personas** | Refugiados, trabajadores | Guerras y crisis que mueven población |

---

## 5. Cómo se relaciona el jugador con la simulación

- **Reputación por facción y por grupo**: lo que haces se sabe si hay testigos, cámaras o
  supervivientes. Sin testigos, nadie sabe que fuiste tú.
- **Contratos, no misiones**: los contratos los generan los actores según sus necesidades. Una
  estación con hambre publica "traer comida, pago X". Un mercader con miedo publica "escolta".
  Una víctima publica "recompensa por el pirata Y". Otros jugadores también pueden publicarlos.
- **Tus acciones son una entrada más de la simulación**: vender mucha comida en un sitio baja su
  precio; hundir convoyes sube el miedo en esa ruta; salvar una estación sube su lealtad hacia ti.
- **Lo que abandonas sigue ahí**: la nave que dejaste a la deriva es un pecio que otro puede
  encontrar.

---

## 6. Niveles de simulación: todo números hasta que miras

| Nivel | Cuándo | Qué se simula |
|---|---|---|
| **Galaxia** | Siempre, cada pocos minutos de juego | Facciones, economía agregada por lugar, guerras, noticias |
| **Sistema** | Sistemas con jugadores, cada pocos segundos | Flotas como puntos en rutas, combates resueltos por fórmula |
| **Local** | Cerca de un jugador | Naves reales, tripulaciones, física, sistemas de a bordo |

- **Resolver en abstracto**: dos flotas que chocan lejos de todos se resuelven con una fórmula
  (fuerza, tecnología, moral, suerte). El resultado deja pecios **concretos** en un lugar concreto.
- **Hidratar al acercarse**: cuando llegas, esos pecios se convierten en naves reales con su daño,
  su carga y sus cadáveres, coherentes con lo que decía el número. Si llegas en plena batalla, pasa
  a ser física real y puedes intervenir.
- **Coherencia**: la regla de oro es que **el nivel detallado nunca contradiga al abstracto**. Si la
  fórmula dijo "30 % de la carga sobrevive", el pecio tiene ese 30 %.

---

## 7. Riesgos de diseño y cómo evitarlos

| Riesgo | Solución |
|---|---|
| **Caos sin sentido**: el mundo cambia tanto que nada importa | Inercia: las cosas cambian despacio, con estados estables (paz, tensión, guerra) y tiempos mínimos |
| **Espiral de muerte**: una facción gana todo y el mundo se congela | Fuerzas compensadoras: los imperios grandes pierden cohesión, los débiles se alían, aparecen facciones nuevas |
| **El jugador no entiende qué pasa** | Noticias legibles, rumores en bares, mapas de riesgo y precios, historial de cada lugar |
| **Nada que hacer en una zona tranquila** | Un "director" suave que empuja pequeños sucesos (averías, pecios antiguos, señales) sin romper la lógica |
| **Explotación** (comprar barato y vender caro en bucle) | Los precios reaccionan a tus propias ventas; el volumen satura el mercado |
| **Coste de simular** | Niveles de detalle; lo lejano, cada mucho tiempo y en agregado |
| **Multijugador**: un jugador rompe la economía del resto | Cada acción tiene un impacto acotado, y es la simulación la que reacciona, no un script |

---

## 8. Por dónde empezar (lo mínimo que ya "está vivo")

1. **Economía de 3–5 lugares y 3 recursos** (comida, combustible, piezas) con precios por escasez.
2. **Mercaderes NPC** que eligen rutas por beneficio y riesgo, en abstracto, y se hidratan al
   acercarte.
3. **Riesgo percibido por ruta** y **una banda pirata** con base real que ataca las rutas rentables.
4. **Noticias** que se propagan y que los NPC y el jugador leen.
5. **Contratos generados por necesidades** (transporte, escolta, recompensa).

Con eso ya hay oportunidades emergentes. Facciones, cismas, líderes y guerras se añaden encima con
los mismos motores.
