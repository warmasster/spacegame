// Declaraciones de datos: bienes, instalaciones, clases de nave, gobiernos, plantillas de asentamiento y nombres.
// Añadir contenido es añadir una entrada aquí; los sistemas no nombran nada concreto.
(function (g) {
  'use strict';
  const S = g.SIM;

  // ── Bienes fungibles. pc = consumo en t por 1000 habitantes y día.
  S.B = { grano: 0, mineral: 1, metal: 2, comb: 3, piezas: 4, bienes: 5, armas: 6, filtros: 7 };
  S.NB = 8;
  S.bien = [
    { id: 'grano', nom: 'Grano', pref: 100, pc: 0.6, col: '#e0b84c' },
    { id: 'mineral', nom: 'Mineral', pref: 40, pc: 0, col: '#9a8672' },
    { id: 'metal', nom: 'Metal', pref: 170, pc: 0, col: '#a9b4c2' },
    { id: 'comb', nom: 'Combustible', pref: 90, pc: 0.08, col: '#e2703a' },
    { id: 'piezas', nom: 'Piezas', pref: 420, pc: 0.004, col: '#5fb0c9' },
    { id: 'bienes', nom: 'Bienes', pref: 300, pc: 0.05, col: '#b98fd6' },
    { id: 'armas', nom: 'Armas', pref: 900, pc: 0, col: '#d0484a' },
    { id: 'filtros', nom: 'Filtros de aire', pref: 520, pc: 0.03, col: '#7fd1a1', soloEstacion: true },
  ];

  // ── Instalaciones. ent/sal en t por día y nivel; trab = plantilla por nivel; nuc = funciona con núcleo de reactor.
  const I = (id, o) => S.def('inst', id, o);
  I('granja', { nom: 'Granja', ico: '🌾', trab: 220, cosecha: 14000 });
  I('hidroponia', { nom: 'Hidroponía', ico: '🥬', trab: 40, sal: { grano: 5 } });
  I('mina', { nom: 'Mina', ico: '⛏', trab: 180, sal: { mineral: 30 }, nuc: true });
  I('refineria', { nom: 'Refinería', ico: '🛢', trab: 90, ent: { mineral: 10 }, sal: { comb: 18 }, nuc: true });
  I('fundicion', { nom: 'Fundición', ico: '🔥', trab: 160, ent: { mineral: 24, comb: 4 }, sal: { metal: 12 }, nuc: true });
  I('fab_piezas', { nom: 'Fábrica de piezas', ico: '⚙', trab: 140, ent: { metal: 8 }, sal: { piezas: 5 }, nuc: true });
  I('fab_bienes', { nom: 'Fábrica de bienes', ico: '📦', trab: 150, ent: { metal: 3, piezas: 2 }, sal: { bienes: 5 } });
  I('fab_filtros', { nom: 'Planta de filtros', ico: '💨', trab: 60, ent: { piezas: 1.5 }, sal: { filtros: 2 } });
  I('armeria', { nom: 'Armería', ico: '🗡', trab: 120, ent: { metal: 4, piezas: 3 }, sal: { armas: 2 }, nuc: true });
  I('fab_nucleos', { nom: 'Fábrica de núcleos', ico: '☢', trab: 200, nucleos: { dias: 5, metal: 12, piezas: 8 }, calidad: true, nuc: true });
  I('astillero', { nom: 'Astillero', ico: '🚀', trab: 400, astillero: true, nuc: true });
  I('almacen', { nom: 'Almacén', ico: '🏬', trab: 30, cap: 5000 });
  I('policia', { nom: 'Cuartel de policía', ico: '🚓', trab: 60, servidores: true });
  I('cuartel', { nom: 'Cuartel', ico: '🛡', trab: 0 });
  I('palacio', { nom: 'Palacio', ico: '🏛', trab: 80 });
  I('ceca', { nom: 'Casa de moneda', ico: '🪙', trab: 40 });
  I('templo', { nom: 'Templo', ico: '⛪', trab: 30 });
  I('memorial', { nom: 'Memorial', ico: '🕯', trab: 0, indestructible: true });

  // ── Clases de nave. vel en unidades/día, fuel en t por unidad de distancia, ef = eficacia en combate (Lanchester).
  const N = (id, o) => S.def('nave', id, o);
  N('carguero', { nom: 'Carguero', cmax: 400, vel: 55, trip: 14, ef: 0.02, fuel: 0.035, valor: 30000, mat: { metal: 120, piezas: 60 }, dias: 50 });
  N('correo', { nom: 'Correo', cmax: 20, vel: 110, trip: 5, ef: 0.01, fuel: 0.02, valor: 20000, mat: { metal: 40, piezas: 40 }, dias: 30 });
  // Las naves de guerra se cuentan por escuadras: `cascos` naves que vuelan, combaten y mueren juntas. tropas = por casco.
  N('fragata', { nom: 'Escuadra de fragatas', cmax: 60, vel: 65, trip: 60, ef: 0.10, fuel: 0.08, valor: 120000, mat: { metal: 300, piezas: 160, armas: 60 }, dias: 110, tropas: 15, guerra: true, cascos: 40, linea: true });
  N('crucero', { nom: 'Escuadra de cruceros', cmax: 80, vel: 60, trip: 180, ef: 0.19, fuel: 0.10, valor: 200000, mat: { metal: 420, piezas: 220, armas: 110 }, dias: 150, tropas: 40, guerra: true, cascos: 14, linea: true });
  N('acorazado', { nom: 'División de acorazados', cmax: 100, vel: 52, trip: 700, ef: 0.48, fuel: 0.16, valor: 320000, mat: { metal: 700, piezas: 380, armas: 220 }, dias: 220, tropas: 150, guerra: true, cascos: 4, linea: true });
  N('corbeta', { nom: 'Corbeta', cmax: 150, vel: 78, trip: 25, ef: 0.05, fuel: 0.05, valor: 50000, mat: { metal: 140, piezas: 80, armas: 25 }, dias: 60, tropas: 5, guerra: true });
  N('granelero', { nom: 'Granelero', cmax: 1500, vel: 44, trip: 20, ef: 0.01, fuel: 0.06, valor: 60000, mat: { metal: 260, piezas: 80 }, dias: 80 });
  N('granja', { nom: 'Nave-granja', cmax: 900, vel: 30, trip: 60, ef: 0.01, fuel: 0.05, valor: 90000, mat: { metal: 300, piezas: 120 }, dias: 120, cultivo: 22 });
  N('carronero', { nom: 'Carroñero', cmax: 300, vel: 46, trip: 16, ef: 0.02, fuel: 0.04, valor: 25000, mat: { metal: 100, piezas: 60 }, dias: 50 });
  N('censo', { nom: 'Nave de censo', cmax: 30, vel: 60, trip: 20, ef: 0.01, fuel: 0.03, valor: 30000, mat: { metal: 60, piezas: 50 }, dias: 40 });
  N('remolcador', { nom: 'Remolcador de asteroides', cmax: 0, vel: 20, trip: 8, ef: 0.01, fuel: 0.05, valor: 25000, mat: { metal: 90, piezas: 50 }, dias: 40 });
  N('prision', { nom: 'Buque-prisión', cmax: 0, vel: 15, trip: 30, ef: 0.02, fuel: 0.05, valor: 40000, mat: { metal: 200, piezas: 60 }, dias: 90 });
  N('funeraria', { nom: 'Nave funeraria', cmax: 5, vel: 50, trip: 10, ef: 0.01, fuel: 0.03, valor: 30000, mat: { metal: 60, piezas: 40 }, dias: 40 });

  // ── Sensores de motor: ruido σ de la lectura (la firma real es N(0,1) en 8 rasgos).
  S.def('sensor', 'civil', { nom: 'civil de estación', sigma: 0.3 });
  S.def('sensor', 'militar', { nom: 'militar', sigma: 0.1 });
  S.CHI2_8_99 = 20.09;

  // ── Formas de gobierno. base = de qué está hecho el poder; forasteros = fracción de la guarnición traída de la capital.
  const G = (id, o) => S.def('gob', id, o);
  G('autocracia', { nom: 'Autarquía', tit: ['Autarca', 'Autarca'], base: 'personal', guardia: true, forasteros: 0.7, rep: 0.8, imp: 0.22, trib: 0.30 });
  G('monarquia', { nom: 'Corona', tit: ['Rey', 'Reina'], base: 'estado', hereditario: true, forasteros: 0.4, rep: 0.5, imp: 0.18, trib: 0.22 });
  G('republica', { nom: 'República Mercantil', tit: ['Cónsul', 'Cónsul'], base: 'estado', forasteros: 0.2, rep: 0.3, imp: 0.14, trib: 0.12 });
  G('teocracia', { nom: 'Santa Sede', tit: ['Pontífice', 'Pontífice'], base: 'popular', forasteros: 0.3, rep: 0.5, imp: 0.16, trib: 0.20 });
  G('junta', { nom: 'Junta', tit: ['General', 'Generala'], base: 'personal', guardia: true, forasteros: 0.6, rep: 0.7, imp: 0.20, trib: 0.26 });
  G('comuna', { nom: 'Comuna', tit: ['Portavoz', 'Portavoz'], base: 'popular', forasteros: 0, rep: 0.15, imp: 0.10, trib: 0.08 });
  G('senorio', { nom: 'Señorío', tit: ['Señor de la guerra', 'Señora de la guerra'], base: 'personal', forasteros: 0.2, rep: 0.6, imp: 0.2, trib: 0.2 });

  // ── Plantillas de asentamiento: [mín, máx] de nivel por instalación.
  const A = (id, o) => S.def('asent', id, o);
  A('agricola', { nom: 'Mundo agrícola', forma: 'planeta', pob: [40000, 110000], ins: { granja: [2.2, 3.4], almacen: [1, 2], fab_bienes: [0, 1], policia: [1, 1], cuartel: [1, 1], templo: [0, 1] } });
  A('industrial', { nom: 'Mundo industrial', forma: 'planeta', pob: [70000, 200000], ins: { granja: [0.5, 1.2], fundicion: [1, 2], fab_piezas: [1, 2], fab_bienes: [1, 2], armeria: [0, 1], almacen: [1, 2], policia: [1, 1], cuartel: [1, 1], templo: [1, 1] } });
  A('minera', { nom: 'Estación minera', forma: 'cinturon', estacion: true, pob: [2500, 9000], ins: { mina: [1, 3], refineria: [0, 1], hidroponia: [0, 1], almacen: [1, 1], policia: [0, 1] } });
  A('comercial', { nom: 'Estación comercial', forma: 'estacion', estacion: true, pob: [8000, 24000], ins: { almacen: [2, 3], fab_filtros: [1, 1], fab_bienes: [0, 1], hidroponia: [1, 1], policia: [1, 1], cuartel: [0, 1] } });
  A('luna', { nom: 'Luna', forma: 'luna', pob: [6000, 30000], ins: { mina: [0, 2], refineria: [0, 1], granja: [0, 0.6], fab_piezas: [0, 1], almacen: [1, 1], policia: [0, 1] } });
  A('sinley', { nom: 'Estación sin ley', forma: 'estacion', estacion: true, sinley: true, pob: [1500, 6000], ins: { almacen: [1, 1], hidroponia: [1, 1] } });

  // ── Colores de casco y su frecuencia (el 8 % de las naves es color óxido).
  S.CASCOS = [
    { nom: 'óxido', p: 0.08, hex: '#a8552a' }, { nom: 'gris', p: 0.30, hex: '#9aa3ad' }, { nom: 'blanco', p: 0.18, hex: '#e8e6df' },
    { nom: 'negro', p: 0.12, hex: '#3a3d44' }, { nom: 'azul', p: 0.10, hex: '#4a78c2' }, { nom: 'verde', p: 0.08, hex: '#4f9a6a' },
    { nom: 'rojo', p: 0.07, hex: '#c2413f' }, { nom: 'ocre', p: 0.07, hex: '#c9a23e' },
  ];
  S.COLORES_ESTADO = ['#e0564f', '#4f8fe0', '#e0b34f', '#5cc28a', '#b46fe0', '#e08a4f', '#4fd0d6', '#d64f9c', '#9bd04f', '#8f8fe8', '#e0d24f', '#4fe0a8', '#c98f6a', '#6ab0c9', '#d0a0d8', '#a0c070'];

  // ── Nombres.
  S.NOM = {
    silA: ['Al', 'Mi', 'Yun', 'Hue', 'Var', 'Len', 'Or', 'Cas', 'Rhu', 'Vey', 'Ta', 'Si', 'Be', 'Dra', 'Fe', 'Gal', 'Is', 'Ka', 'Lu', 'Mo', 'Na', 'Pa', 'Qui', 'Ro', 'Sa', 'Te', 'Ur', 'Va', 'Xi', 'Za', 'El', 'Um', 'Bra', 'Tor', 'Ner', 'Os', 'Ar', 'En', 'Il', 'Cor', 'Du', 'Go', 'Ha', 'Je', 'Li', 'Me', 'No', 'Pe', 'Ri', 'So'],
    silB: ['ma', 'e', 'ri', 'lo', 'ta', 'no', 've', 'ga', 'du', 'sa', 'li', 'ra', 'mi', '', '', '', '', 'ba', 'ce', 'po', 'tu', 'le', 'na'],
    silC: ['gra', 's', 'que', 'ndo', 'so', 'nz', 'll', 'ne', 'yl', 'u', 'ria', 'mar', 'dor', 'via', 'tán', 'ce', 'lia', 'ro', 'nte', 'x', 'th', 'ra', 'es', 'ón', 'al', 'ur', 'is', 'ia', 'os', 'ane'],
    estacion: ['Pozo {a}', 'Puerto {n}', 'Faro de {n}', 'Alto {n}', 'Dique {n}', 'Cantera {a}', 'Estación {n}', 'Bajo {n}', 'Paso {a}', 'Cinturón de {s}', 'Muelle {a}'],
    sinley: ['Mercado de {s}', 'La Trastienda', 'Puerto {s}', 'El Sumidero', 'Feria de {s}', 'La Deriva', 'Rincón de {s}'],
    adj: ['Hondo', 'Largo', 'Seco', 'Negro', 'Viejo', 'Rojo', 'Frío', 'Quieto', 'Alto', 'Ciego', 'Manso', 'Roto', 'Blanco', 'Perdido'],
    sust: ['Hueso', 'Yunque', 'Sal', 'Ceniza', 'Cobre', 'Óxido', 'Vidrio', 'Brea', 'Hierro', 'Cal', 'Trapo', 'Humo', 'Plomo', 'Estaño', 'Barro'],
    hombre: ['Tobías', 'Casimir', 'Oren', 'Lior', 'Darío', 'Tomás', 'Ferro', 'Ismael', 'Bruno', 'Gael', 'Íñigo', 'Marcial', 'Saúl', 'Teo', 'Unai', 'Vidal', 'Zenón', 'Abel', 'Baltasar', 'Ciro', 'Dimas', 'Elías', 'Fabián', 'Germán', 'Héctor', 'Jonás', 'Kilian', 'Lázaro', 'Mateo', 'Néstor', 'Odón', 'Pascual', 'Quirino', 'Ramiro', 'Simón', 'Tadeo', 'Ulises', 'Valerio', 'Yago', 'Anselmo', 'Blas', 'Cosme', 'Efrén', 'Leandro', 'Nicanor'],
    mujer: ['Neva', 'Irune', 'Adaia', 'Mirra', 'Selma', 'Tecla', 'Ula', 'Vera', 'Zoraida', 'Alba', 'Berta', 'Cloe', 'Dalia', 'Elvira', 'Fedra', 'Greta', 'Hilda', 'Inés', 'Jimena', 'Kora', 'Leire', 'Maia', 'Nerea', 'Olaya', 'Petra', 'Quima', 'Rut', 'Sira', 'Tania', 'Úrsula', 'Valka', 'Yara', 'Zulema', 'Ágata', 'Brígida', 'Celia', 'Dafne', 'Edurne', 'Flora', 'Gala', 'Elda', 'Noa', 'Lucía', 'Marta', 'Aitana'],
    apellido: ['Arriaga', 'Nuño', 'Veyl', 'Castell', 'Rhune', 'Ansa', 'Sanz', 'Orell', 'Varga', 'Lenz', 'Vaskar', 'Oru', 'Brezo', 'Calderón', 'Dávila', 'Espino', 'Fierro', 'Garza', 'Haro', 'Ibarra', 'Jordán', 'Lobo', 'Maldonado', 'Navas', 'Ochoa', 'Pardo', 'Quiroga', 'Robles', 'Salcedo', 'Tejada', 'Urrutia', 'Vidaurre', 'Zárate', 'Aldana', 'Bermejo', 'Cisneros', 'Durango', 'Elizondo', 'Forcada', 'Guzmán', 'Herrán', 'Iriarte', 'Larrea', 'Mújica', 'Noriega', 'Olmedo', 'Peñalba', 'Rentería', 'Sagasti', 'Torralba', 'Ugarte', 'Valcárcel', 'Yanguas', 'Zubiri', 'Amezcua', 'Balmaseda', 'Cortázar', 'Echave', 'Galarza', 'Mendívil'],
    nave: ['Paciencia', 'Carraca', 'Tordo', 'Alondra', 'Fe Ciega', 'Buena Hora', 'Viuda Alegre', 'Tres Hermanos', 'Cierzo', 'Solano', 'Mala Sombra', 'Deuda Vieja', 'Vuelta y Vuelta', 'Santa Rita', 'Pan Duro', 'Ancla Rota', 'Lucero', 'Garza Real', 'Última Carta', 'Medio Real', 'Doña Prisa', 'Calma Chicha', 'Tizón', 'Mirlo', 'Ventolera', 'Reina Mora', 'Perra Suerte', 'Barlovento', 'Sotavento', 'Dos Faroles', 'Clavo Ardiendo', 'Hija del Humo', 'Larga Espera', 'Viento Norte', 'Casco Viejo', 'Aguja de Marear', 'Sal Gruesa', 'Buen Padre', 'Mal Pagador', 'Ruiseñor', 'Cuervo Blanco', 'Trueno Sordo', 'Marea Baja', 'Flor de Hierro', 'Lobo Manso', 'Espuela', 'Candil', 'Arriero', 'Paloma Torcaz', 'Borrasca'],
    guerra: ['Implacable', 'Vigía', 'Tormenta', 'Martillo', 'Estandarte', 'Coloso', 'Baluarte', 'Azote', 'Ariete', 'Vanguardia', 'Legión', 'Ira', 'Soberana', 'Invicta', 'Tempestad', 'Guardiana', 'Hierro', 'Corona', 'Espada', 'Lanza', 'Tormenta', 'Martillo', 'Estandarte', 'Coloso', 'Baluarte', 'Azote', 'Ariete', 'Vanguardia', 'Legión', 'Ira', 'Soberana', 'Invicta', 'Tempestad', 'Guardiana', 'Hierro', 'Corona', 'Espada', 'Lanza', 'Tenaz', 'Furia', 'Constancia', 'Centinela', 'Audaz', 'Relámpago', 'Severa', 'Indómita', 'Veterana', 'Justicia', 'Resuelta', 'Custodia', 'Intrépida', 'Vengadora', 'Firmeza', 'Aurora', 'Leal', 'Altiva'],
    moneda: ['dinar', 'sol', 'marco', 'escudo', 'talento', 'real', 'lira', 'ducado', 'florín', 'peso'],
    marca: ['Fundiciones', 'Forjas', 'Talleres', 'Reactores', 'Aceros'],
    casa: ['Compañía de la Sal Negra', 'Casa Vaskar', 'Liga del Cobre', 'Compañía del Faro', 'Casa Varga-Lenz', 'Hermanos Orell', 'Compañía de los Tres Diques', 'Casa Quiroga', 'Sociedad del Lucero'],
    // Objeto más recordado según el tipo de hecho fundador; da el símbolo de la facción.
    objeto: {
      explosion: [['Casco', 'Rayado'], ['Visor', 'Roto'], ['Lámpara', 'Apagada'], ['Pico', 'Torcido']],
      hambre: [['Espiga', 'Seca'], ['Cuenco', 'Vacío'], ['Hoz', 'Mellada'], ['Trigo', 'Negro']],
      masacre: [['Ojo', 'Tachado'], ['Mano', 'Abierta'], ['Cuerda', 'Rota'], ['Plaza', 'Roja']],
      bomba: [['Ceniza', 'Fría'], ['Cráter', 'Hondo'], ['Llama', 'Blanca'], ['Cielo', 'Roto']],
      profecia: [['Ojo', 'Tachado'], ['Mesa', 'Puesta'], ['Vela', 'Negra'], ['Voz', 'Baja']],
      pirata: [['Moneda', 'Quemada'], ['Ancla', 'Partida'], ['Cuchillo', 'Largo']],
      otro: [['Farol', 'Ciego'], ['Llave', 'Vieja'], ['Puño', 'Cerrado'], ['Hilo', 'Rojo']],
    },
  };

  // Profecías: predicados sobre hechos futuros. Si se cumplen, el predicador gana una orden.
  S.PROFECIAS = [
    { id: 'tirano_publico', txt: 'el tirano caerá a la vista de todos', casa: (e) => e.k === 'muerte' && e.d && e.d.gobernante && e.d.modo === 'publico' },
    { id: 'tirano_silencio', txt: 'el tirano morirá en su cama y nadie sabrá por qué', casa: (e) => e.k === 'muerte' && e.d && e.d.gobernante && e.d.modo === 'silencio' },
    { id: 'fuego_cielo', txt: 'caerá fuego del cielo sobre las máquinas', casa: (e) => e.k === 'bomba' || e.k === 'asteroide' },
    { id: 'cosecha', txt: 'la cosecha se pudrirá en el surco', casa: (e) => e.k === 'plaga' },
    { id: 'reactor', txt: 'el corazón de hierro reventará', casa: (e) => e.k === 'explosion' },
    { id: 'flota', txt: 'la flota arderá entre las rocas', casa: (e) => e.k === 'batalla_fin' && e.d && e.d.perdidas >= 6 },
  ];
})(typeof globalThis !== 'undefined' ? globalThis : this);
