// Objetos únicos: número de serie, historial de solo añadir, huella de motor y núcleos de reactor
// cuya microfisura crece con la ley de Paris en forma cerrada: a(n) = (a0^-1/2 − K/2·n)^-2.
(function (g) {
  'use strict';
  const S = g.SIM;
  const O = S.obj = {};

  O.A_FALLO = 4;        // mm: el núcleo revienta
  O.A_DETECT = 1;       // mm: lo que ve un detector de grietas
  O.K_PLENO = 0.0236;   // constante de juego a plena carga (falla en el ciclo 147 desde 0,2 mm)
  O.K_NOMINAL = 0.0236 * 0.125; // carga nominal 0,5 → K ∝ Δσ³

  O.crear = function (m, tipo, o) {
    const x = Object.assign({ id: m.obj.length, tipo, serie: '', interno: '', fab: -1, marca: '', t0: m.t, donde: null, hist: [], robado: false, vivo: true }, o || {});
    if (!x.interno) x.interno = x.serie;
    m.obj.push(x); return x;
  };
  O.nombre = function (m, o) {
    if (o.tipo === 'nucleo') return (o.marca ? o.marca + ' ' : 'núcleo ') + o.interno;
    if (o.tipo === 'libreta') return 'libreta de ' + (m.per[o.de] ? m.per[o.de].nom : '?');
    if (o.tipo === 'caja negra') return 'caja negra de la ' + (m.nav[o.nave] ? m.nav[o.nave].nom : '?');
    return o.tipo + ' ' + o.serie;
  };
  // Solo se guardan los hechos con relevancia: cambio de dueño, daño, delito.
  O.anotar = function (m, o, ev) { if (ev >= 0 && o.hist[o.hist.length - 1] !== ev) o.hist.push(ev); };

  // ── Huella de motor: 8 desviaciones del patrón de empuje, N(0,1).
  O.firma = function (rng) { const f = []; for (let i = 0; i < 8; i++) f.push(rng.n(0, 1)); return f; };
  O.lectura = function (firma, sigma, rng) { return firma.map(x => x + rng.n(0, sigma)); };
  // D² = ‖m1 − m2‖² / 2σ²; con el mismo motor sigue una χ² de 8 grados.
  O.D2 = function (a, b, sigma) { let d = 0; for (let i = 0; i < 8; i++) { const x = a[i] - b[i]; d += x * x; } return d / (2 * sigma * sigma); };

  // ── Núcleos.
  O.nuevoNucleo = function (m, ins, o) {
    o = o || {};
    const rng = m.rng;
    let marca = 'Núcleo', lote = 1, num = 1000 + rng.i(9000), fab = -1;
    if (ins) { marca = ins.marca; lote = ins.lote; num = ins.serieSig++; fab = ins.id; }
    const def = !!o.defectuoso;
    const x = O.crear(m, 'nucleo', {
      serie: lote + '-' + num, interno: lote + '-' + num, marca, fab, lote, turno: o.turno || rng.el(['mañana', 'tarde', 'noche']),
      t0: o.t0 !== undefined ? o.t0 : m.t, defecto: def,
      a0: def ? rng.r(0.05, 0.15) : rng.r(0.003, 0.012), K: def ? O.K_NOMINAL * 2 : O.K_NOMINAL,
      tref: m.t, cpd: 0, tF: Infinity, falla: null, evDano: o.causa !== undefined ? o.causa : -1, insp: -1e9,
    });
    return x;
  };
  O.ciclos = (o, t) => Math.max(0, (t - o.tref) * o.cpd);
  O.grieta = function (o, t) {
    const b = 1 / Math.sqrt(o.a0) - 0.5 * o.K * O.ciclos(o, t);
    if (b <= 1 / Math.sqrt(O.A_FALLO)) return O.A_FALLO;
    return 1 / (b * b);
  };
  O.ciclosHastaFallo = (o) => (1 / Math.sqrt(o.a0) - 1 / Math.sqrt(O.A_FALLO)) * 2 / o.K;
  // Fija el estado actual como nuevo origen (antes de cambiar carga o daño) y reprograma la fecha de fallo.
  O.rebasar = function (m, o) { o.a0 = O.grieta(o, m.t); o.tref = m.t; };
  O.programar = function (m, o) {
    if (o.falla) { o.falla.x = true; o.falla = null; }
    o.tF = Infinity;
    if (!o.vivo || o.cpd <= 0) return;
    o.tF = o.tref + O.ciclosHastaFallo(o) / o.cpd;
    o.falla = m.prog(Math.max(m.t + 0.01, o.tF), 'nucleo.falla', { o: o.id });
  };
  // Instalar/retirar: los ciclos solo corren cuando el núcleo trabaja (3 al día).
  O.activar = function (m, o, cpd) { O.rebasar(m, o); o.cpd = cpd; O.programar(m, o); };
  // Un golpe: la fisura salta y la pieza queda trabajando a plena tensión.
  O.danar = function (m, o, a, ev, pleno) {
    O.rebasar(m, o);
    if (a > o.a0) o.a0 = a;
    if (pleno) o.K = O.K_PLENO;
    o.evDano = ev; O.anotar(m, o, ev); O.programar(m, o);
  };

  S.en('nucleo.falla', function (m, e) {
    const o = m.obj[e.o]; if (!o || !o.vivo || o.falla !== e) return;
    o.vivo = false; o.falla = null;
    const d = o.donde; const causas = [o.evDano]; if (o.evDefecto !== undefined) causas.push(o.evDefecto);
    for (let i = o.hist.length - 1; i >= 0 && causas.length < 4; i--) causas.push(o.hist[i]);
    if (d && d.t === 'N') S.emitir(m, 'nucleo.revienta.nave', m.nav[d.id], o, causas);
    else if (d && d.t === 'I') S.emitir(m, 'nucleo.revienta.inst', m.ins[d.id], o, causas);
  });

  // Mantenimiento de instalaciones: una vez al mes se mira el reactor (si hay inspector honrado) y se cambia si hay repuesto.
  S.gancho('asent.mes', function (m, a) {
    const ins = a.inspector >= 0 ? m.per[a.inspector] : null;
    const mira = !ins || !ins.vivo ? 0.6 : (ins.r[S.R.COD] > 0.8 && ins.r[S.R.PRU] < 0.5 ? 0.1 : 1);
    for (const id of a.ins) {
      const i = m.ins[id]; if (i.nuc < 0) continue; const o = m.obj[i.nuc]; if (!o.vivo) continue;
      if (m.t - o.insp < 55 || !a.rng.p(mira)) continue;
      if (!O.inspeccionar(m, o)) continue;
      if (!a.nuc.length) { if (o.evDano < 0) o.evDano = m.reg('sin_repuesto', '{I' + id + '} tiene el núcleo ' + o.interno + ' agrietado y no hay repuesto en {A' + a.id + '}: sigue funcionando', { a: a.id, imp: 0, c: [a.evSinNucleos] }); continue; }
      o.vivo = false; if (o.falla) { o.falla.x = true; o.falla = null; }
      const nu = m.obj[a.nuc.shift()]; nu.donde = { t: 'I', id }; i.nuc = nu.id; O.activar(m, nu, 3);
    }
    // Las naves que no se mueven del muelle (flotas en su base, remolcadores, buques-prisión) también pasan revisión.
    for (const n of m.nav) {
      if (!n.vivo || n.en !== a.id || n.st !== 'atracada') continue;
      if (n.nuc < 0) { if (n.cascos > 1) S.nav.cambiarNucleo(m, n, a, -1); continue; }
      const o = m.obj[n.nuc]; if (!o.vivo || m.t - o.insp < 55) continue;
      if (a.rng.p(mira) && O.inspeccionar(m, o)) S.nav.cambiarNucleo(m, n, a, -1);
    }
  });

  // Inspección con detector de grietas de 1 mm. Devuelve true si la ve.
  O.inspeccionar = function (m, o) { o.insp = m.t; return O.grieta(o, m.t) >= O.A_DETECT; };
})(typeof globalThis !== 'undefined' ? globalThis : this);
