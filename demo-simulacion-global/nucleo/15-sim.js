// Fachada del núcleo: lo único que la interfaz necesita saber. El motor gráfico solo pregunta
// «qué hay aquí» y dice «qué ha hecho el jugador».
(function (g) {
  'use strict';
  const S = g.SIM;

  S.ORDEN = ['00-base', '01-registros', '02-mundo', '07-objetos', '08-personas', '04-economia', '09-sociedad', '06-informacion', '05-naves', '10-facciones', '11-politica', '12-combate', '13-justicia', '14-dios', '16-tecnica', '17-teorias', '18-contenido', '03-generacion', '15-sim'];

  S.crear = function (semilla, op) { return S.gen.crear(semilla, op); };
  // Prehistoria: se simula a toda velocidad antes de mirar. Las ruinas son de imperios que existieron.
  S.prehistoria = function (m, anios, cada) {
    const fin = m.t + anios * S.ANIO;
    while (m.t < fin) { m.avanzar(Math.min(fin, m.t + S.ANIO)); if (cada) cada(m); }
    m.presente = m.t;
    m.reg('presente', 'Acaba la prehistoria: ' + anios + ' años que nadie ha escrito. A partir de aquí, miras', { imp: 1 });
    return m;
  };
  S.resumen = function (m) {
    const c = m.cuenta; let pob = 0; for (const a of m.ase) pob += a.pob;
    return {
      anio: m.anio(), pob: Math.round(pob), estados: m.est.filter(e => e.vivo).length, naves: m.nav.filter(n => n.vivo).length,
      piratas: m.nav.filter(n => n.vivo && n.pirata).length, facciones: m.fac.filter(f => f.vivo && f.etapa === 'inst' && f.tipo !== 'casa').length,
      personas: m.per.length, hechos: m.ev.length, eventos: m.nEv,
      guerras: c.guerra || 0, civiles: c.guerra_civil || 0, batallas: c.batalla || 0, conquistas: c.conquista || 0, hambrunas: c.hambruna || 0, revoluciones: c.revolucion || 0,
      masacres: c.masacre || 0, cismas: c.cisma || 0, fusiones: c.fusion || 0, faccionesNacidas: c.faccion || 0, huelgas: c.huelga || 0, explosiones: c.explosion || 0,
      robos: c.robo || 0, abordajes: c.pirateria || 0, motines: c.motin || 0, sucesiones: c.sucesion || 0, fragmentaciones: c.fragmentacion || 0, venganzas: c.emboscada || 0,
      condenas: c.condena || 0, profecias: c.profecia_cumplida || 0, independencias: c.independencia || 0, golpes: c.golpe || 0, caidos: c.estado_cae || 0,
    };
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
