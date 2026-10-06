// Generación del mundo a partir de la semilla: sistemas, rutas, asentamientos, estados, cortes, guarniciones,
// casas mercantes, naves y núcleos. Después se simula la prehistoria: nada de lo que se ve al empezar está escrito.
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B;
  const G = S.gen = {};

  function nombreSilabas(rng) { return rng.el(S.NOM.silA) + rng.el(S.NOM.silB) + rng.el(S.NOM.silC); }
  function unico(m, f) { for (let i = 0; i < 60; i++) { const n = f(); if (!m._nombres.has(n)) { m._nombres.add(n); return n; } } const n = f() + ' ' + (m._nombres.size + 1); m._nombres.add(n); return n; }
  G.nombreLugar = (m) => unico(m, () => nombreSilabas(m.rng));
  G.nombreEstacion = function (m, sinley) {
    const rng = m.rng;
    return unico(m, () => rng.el(sinley ? S.NOM.sinley : S.NOM.estacion).replace('{a}', rng.el(S.NOM.adj)).replace('{s}', rng.el(S.NOM.sust)).replace('{n}', nombreSilabas(rng)));
  };
  G.nombreNave = function (m, guerra) { const rng = m.rng; return unico(m, () => rng.el(guerra ? S.NOM.guerra : S.NOM.nave)); };

  function genSistemas(m, N) {
    const rng = m.rng; const W = Math.sqrt(N * 56000 * 1.6), H = W / 1.6; m.ancho = W; m.alto = H;
    let minD = 150, intentos = 0;
    while (m.sis.length < N) {
      if (++intentos > 4000) { minD *= 0.92; intentos = 0; }
      const x = rng.r(0, W), y = rng.r(0, H);
      const ex = (x - W / 2) / (W / 2), ey = (y - H / 2) / (H / 2); if (ex * ex + ey * ey > 1.02) continue;
      let ok = true; for (const s of m.sis) { const dx = s.x - x, dy = s.y - y; if (dx * dx + dy * dy < minD * minD) { ok = false; break; } }
      if (!ok) continue;
      m.sis.push({ id: m.sis.length, nom: G.nombreLugar(m), x, y, vec: [], terr: rng.p(0.25) ? 'asteroides' : 'abierto', col: rng.el(['#ffd9a0', '#fff1d6', '#a9c8ff', '#ffb38a', '#ffe9b0', '#d6e4ff']), ase: [], est: -1, pir: [], pecios: [] });
    }
  }
  function genRutas(m) {
    const N = m.sis.length; const d = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
    const liga = (i, j) => { const a = m.sis[i], b = m.sis[j]; if (i === j || a.vec.some(v => v.a === j)) return; const x = d(a, b); a.vec.push({ a: j, d: x }); b.vec.push({ a: i, d: x }); };
    for (let i = 0; i < N; i++) {
      const ord = []; for (let j = 0; j < N; j++) if (j !== i) ord.push([d(m.sis[i], m.sis[j]), j]);
      ord.sort((x, y) => x[0] - y[0]);
      for (let k = 0; k < 3 && k < ord.length; k++) if (ord[k][0] < 520 && (k < 2 || m.sis[i].vec.length < 3)) liga(i, ord[k][1]);
    }
    // Conectividad: une los componentes por su par más cercano.
    for (;;) {
      const comp = new Array(N).fill(-1); let nc = 0;
      for (let i = 0; i < N; i++) { if (comp[i] >= 0) continue; const pila = [i]; comp[i] = nc; while (pila.length) { const x = pila.pop(); for (const v of m.sis[x].vec) if (comp[v.a] < 0) { comp[v.a] = nc; pila.push(v.a); } } nc++; }
      if (nc === 1) break;
      let best = null; for (let i = 0; i < N; i++) for (let j = i + 1; j < N; j++) if (comp[i] !== comp[j]) { const x = d(m.sis[i], m.sis[j]); if (!best || x < best[0]) best = [x, i, j]; }
      liga(best[1], best[2]);
    }
    // Caminos mínimos (Floyd–Warshall) y primer salto.
    const D = [], Sg = [], Hp = [];
    for (let i = 0; i < N; i++) { D.push(new Array(N).fill(Infinity)); Sg.push(new Array(N).fill(-1)); Hp.push(new Array(N).fill(99)); D[i][i] = 0; Hp[i][i] = 0; Sg[i][i] = i; for (const v of m.sis[i].vec) { D[i][v.a] = v.d; Sg[i][v.a] = v.a; Hp[i][v.a] = 1; } }
    for (let k = 0; k < N; k++) for (let i = 0; i < N; i++) { const dik = D[i][k]; if (dik === Infinity) continue; for (let j = 0; j < N; j++) { const x = dik + D[k][j]; if (x < D[i][j] - 1e-9) { D[i][j] = x; Sg[i][j] = Sg[i][k]; Hp[i][j] = Hp[i][k] + Hp[k][j]; } } }
    m.dist = D; m.sig = Sg; m.saltos = Hp;
  }

  function genAsentamientos(m) {
    const rng = m.rng; const N = m.sis.length;
    const cuotas = [['agricola', 0.22], ['industrial', 0.22], ['minera', 0.26], ['comercial', 0.14], ['luna', 0.16]];
    const lista = []; for (const [t, f] of cuotas) for (let i = 0; i < Math.round(f * N); i++) lista.push(t);
    while (lista.length < N) lista.push('minera'); rng.bar(lista);
    for (let i = 0; i < N; i++) {
      const s = m.sis[i]; const t = lista[i];
      const pl = S.reg.asent[t];
      S.eco.nuevoAsent(m, s, t, pl.forma === 'planeta' ? s.nom : (pl.forma === 'luna' ? G.nombreLugar(m) : G.nombreEstacion(m)));
      if (pl.forma === 'planeta' && rng.p(0.45)) S.eco.nuevoAsent(m, s, 'luna', G.nombreLugar(m));
      if (rng.p(0.35) && t !== 'minera') S.eco.nuevoAsent(m, s, 'minera', G.nombreEstacion(m));
      if (rng.p(0.15) && t !== 'comercial' && s.ase.length < 3) S.eco.nuevoAsent(m, s, 'comercial', G.nombreEstacion(m));
    }
  }

  function genEstados(m) {
    const rng = m.rng; const N = m.sis.length; const K = Math.max(3, Math.round(N / 7));
    const planetas = m.ase.filter(a => a.forma === 'planeta').sort((a, b) => b.pob - a.pob);
    const caps = [planetas[0]];
    while (caps.length < K && caps.length < planetas.length) {
      let best = null, bs = -1;
      for (const p of planetas) { if (caps.indexOf(p) >= 0 || caps.some(c => c.sis === p.sis)) continue; let mn = Infinity; for (const c of caps) mn = Math.min(mn, m.dist[p.sis][c.sis]); const sc = mn * Math.pow(p.pob, 0.3); if (sc > bs) { bs = sc; best = p; } }
      if (!best) break; caps.push(best);
    }
    // Estaciones sin ley: en los sistemas más lejos de toda capital.
    const L = Math.max(2, Math.round(N / 13));
    const lejos = m.sis.filter(s => !caps.some(c => c.sis === s.id)).map(s => { let mn = Infinity; for (const c of caps) mn = Math.min(mn, m.dist[s.id][c.sis]); return [mn, s]; }).sort((a, b) => b[0] - a[0]);
    const sinley = new Set();
    for (let i = 0; i < L && i < lejos.length; i++) { const s = lejos[i][1]; sinley.add(s.id); S.eco.nuevoAsent(m, s, 'sinley', G.nombreEstacion(m, true)); }
    const tipos = rng.bar(['autocracia', 'monarquia', 'republica', 'autocracia', 'teocracia', 'junta', 'monarquia', 'republica', 'autocracia', 'junta']);
    caps.forEach((c, i) => S.pol.nuevoEstado(m, { cap: c.id, tipo: tipos[i % tipos.length], col: S.COLORES_ESTADO[i % S.COLORES_ESTADO.length] }));
    for (const s of m.sis) {
      if (sinley.has(s.id)) continue;
      let best = -1, bd = Infinity;
      for (const e of m.est) { const cs = m.ase[e.cap].sis; if (m.saltos[s.id][cs] <= 3 && m.dist[s.id][cs] < bd) { bd = m.dist[s.id][cs]; best = e.id; } }
      if (best >= 0) for (const a of s.ase) { m.ase[a].est = best; }
    }
    for (const e of m.est) m.ase[e.cap].cap = true;
    S.pol.refrescarSistemas(m);
  }

  function genEspeciales(m) {
    const rng = m.rng;
    const ind = m.ase.filter(a => a.tipo === 'industrial');
    const tiene = (a, t) => a.ins.some(i => m.ins[i].tipo === t);
    // Fábricas de núcleos: pocas y estratégicas, en estados distintos si se puede.
    const nF = Math.max(3, Math.round(m.sis.length / 10)); const usados = new Set(); const cand = rng.bar(ind.slice());
    let puestas = 0;
    for (const paso of [0, 1]) for (const a of cand) { if (puestas >= nF) break; if (tiene(a, 'fab_nucleos')) continue; if (paso === 0 && usados.has(a.est)) continue; usados.add(a.est); S.eco.nuevaInst(m, a, 'fab_nucleos', 1); puestas++; }
    for (const e of m.est) { const c = m.ase[e.cap]; S.eco.nuevaInst(m, c, 'palacio', 1); S.eco.nuevaInst(m, c, 'ceca', 1); if (!tiene(c, 'astillero')) S.eco.nuevaInst(m, c, 'astillero', 1); if (!tiene(c, 'templo')) S.eco.nuevaInst(m, c, 'templo', 1); }
    let extra = 2; for (const a of rng.bar(ind.slice())) { if (extra <= 0) break; if (!tiene(a, 'astillero')) { S.eco.nuevaInst(m, a, 'astillero', 1); extra--; } }
    const asegurar = (t, n, filtro) => { let hay = m.ins.filter(i => i.tipo === t).length; for (const a of rng.bar(m.ase.filter(filtro))) { if (hay >= n) break; if (!tiene(a, t)) { S.eco.nuevaInst(m, a, t, 1); hay++; } } };
    asegurar('armeria', Math.max(2, Math.ceil(m.est.length / 2)), a => a.tipo === 'industrial');
    asegurar('refineria', Math.max(3, Math.ceil(m.sis.length / 6)), a => a.tipo === 'minera' || a.tipo === 'luna');
    asegurar('fab_filtros', 3, a => a.tipo === 'comercial' || a.tipo === 'industrial');
    asegurar('fundicion', 3, a => a.tipo === 'industrial' || a.tipo === 'luna');
    asegurar('mina', 4, a => a.tipo === 'minera' || a.tipo === 'luna');
  }

  function genGente(m) {
    const rng = m.rng;
    for (const e of m.est) S.pol.fundarCorte(m, e);
    for (const a of m.ase) {
      if (a.sinley) {
        const np = 1 + rng.i(2);
        for (let i = 0; i < np; i++) S.per.crear(m, { casa: a.id, rol: 'perista', r: { COD: rng.r(0.5, 0.95) } });
        for (let i = 0; i < 2; i++) S.per.crear(m, { casa: a.id, rol: 'cazador', r: { VAL: rng.r(0.6, 0.95) } });
      }
      if (a.tipo === 'comercial' || a.sinley) S.per.crear(m, { casa: a.id, rol: 'piloto', nace: m.t - rng.r(55, 70) * S.ANIO });
      if (!a.sinley) {
        const e = a.est >= 0 ? m.est[a.est] : null;
        if (!(e && e.cap === a.id)) { const gob = S.per.crear(m, { casa: a.id, est: a.est, rol: 'gobernador', ideo: e ? m.per[e.gob].ideo : undefined, cargo: { t: 'gobernador', id: a.id, nom: e ? 'gobernador de ' + a.nom : 'alcalde de ' + a.nom } }); a.gob = gob.id; }
        S.soc.nuevaGuarnicion(m, a);
        if (a.ins.some(i => m.ins[i].tipo === 'policia')) {
          a.comisario = S.per.crear(m, { casa: a.id, est: a.est, rol: 'comisario', cargo: { t: 'comisario', id: a.id, nom: 'comisario de ' + a.nom } }).id;
          a.inspector = S.per.crear(m, { casa: a.id, est: a.est, rol: 'inspector', cargo: { t: 'inspector', id: a.id, nom: 'inspector de muelle de ' + a.nom } }).id;
          a.ley = rng.r(0.5, 0.95); a.sensor = a.cap ? 'militar' : 'civil';
        } else { a.ley = rng.r(0.15, 0.4); a.sensor = null; }
      } else { a.ley = 0; a.sensor = null; }
      if (rng.p(0.14)) S.per.crear(m, { casa: a.id, est: a.est, rol: 'predicador', r: { FE: rng.r(0.75, 1) }, car: rng.r(0.5, 0.95) });
    }
    for (const i of m.ins) if (S.reg.inst[i.tipo].calidad) { const a = m.ase[i.ase]; i.jefe = S.per.crear(m, { casa: a.id, est: a.est, rol: 'jefe_calidad', cargo: { t: 'jefe_calidad', id: i.id, nom: 'jefe de calidad de ' + i.nom } }).id; }
  }

  function genNaves(m) {
    const rng = m.rng; let pobT = 0; for (const a of m.ase) pobT += a.pob;
    // Casas mercantes.
    const sedes = rng.bar(m.ase.filter(a => a.tipo === 'comercial' || a.cap));
    const nombres = rng.bar(S.NOM.casa.slice()); const nH = Math.min(sedes.length, nombres.length, m.est.length + 1);
    const casas = [];
    for (let i = 0; i < nH; i++) {
      const lid = S.per.crear(m, { casa: sedes[i].id, est: sedes[i].est, rol: 'mercader', din: 20000, r: { COD: rng.r(0.5, 0.95), AMB: rng.r(0.4, 0.9) } });
      casas.push(S.fac.nueva(m, { nom: nombres[i], tipo: 'casa', etapa: 'inst', sede: sedes[i].id, lid: lid.id, caja: rng.r(150000, 400000), O: 1, sim: 'un sello de lacre' }));
    }
    const puertos = m.ase.filter(a => !a.sinley); const pesos = puertos.map(a => Math.sqrt(a.pob));
    const nC = S.clamp(Math.round(pobT / 8500), 60, 360);
    for (let i = 0; i < nC; i++) {
      const base = puertos[rng.pesos(pesos)];
      const cap = S.per.crear(m, { casa: base.id, est: base.est, rol: 'capitan' });
      const casa = rng.p(0.6) ? rng.el(casas) : null;
      S.nav.crear(m, 'carguero', { en: base.id, cap: cap.id, dueno: casa ? { t: 'F', id: casa.id } : { t: 'P', id: cap.id }, est: base.est, dinero: rng.r(15000, 45000) });
    }
    const com = m.ase.filter(a => a.tipo === 'comercial');
    for (let i = 0; i < Math.max(2, Math.round(m.sis.length / 16)); i++) { const base = rng.el(com.length ? com : puertos); const cap = S.per.crear(m, { casa: base.id, est: base.est, rol: 'capitan' }); S.nav.crear(m, 'granja', { en: base.id, cap: cap.id, dueno: { t: 'F', id: rng.el(casas).id }, est: base.est, dinero: 30000 }); }
    for (const a of m.ase) {
      if (a.forma === 'cinturon') S.nav.crear(m, 'remolcador', { en: a.id, dueno: { t: 'A', id: a.id }, est: a.est });
      if (a.sinley || (a.tipo === 'comercial' && rng.p(0.3))) { const cap = S.per.crear(m, { casa: a.id, est: -1, rol: 'capitan' }); S.nav.crear(m, 'carronero', { en: a.id, cap: cap.id, dueno: { t: 'P', id: cap.id }, est: -1, dinero: 8000 }); }
    }
    for (const e of m.est) S.pol.fundarFlota(m, e);
    // Piratas: no salen de la nada. Cada banda inicial es una tripulación que se amotinó por impagos.
    for (const a of m.ase.filter(x => x.sinley)) {
      const n = 1 + rng.i(2);
      for (let i = 0; i < n; i++) {
        const cap = S.per.crear(m, { casa: a.id, est: -1, rol: 'pirata', r: { VAL: rng.r(0.6, 0.95), COD: rng.r(0.5, 0.9) } });
        const nv = S.nav.crear(m, rng.p(0.5) ? 'corbeta' : 'carguero', { en: a.id, cap: cap.id, dueno: { t: 'P', id: cap.id }, est: -1, dinero: 6000, pirata: true });
        const ev = m.reg('motin', 'La tripulación de la {N' + nv.id + '} lleva ' + (3 + rng.i(4)) + ' meses sin cobrar: se amotina y se echa a la piratería desde {A' + a.id + '}', { a: a.id, imp: 0 });
        nv.cn.push(ev); nv.origenPirata = ev;
      }
    }
  }

  // Ajusta niveles para que la cadena de suministro de toda la galaxia sea viable (oferta ≈ 1,15 × demanda).
  function equilibrar(m) {
    let pob = 0, pobEst = 0; for (const a of m.ase) { pob += a.pob; if (a.estacion) pobEst += a.pob; }
    const nNav = m.nav.length; const nGuerra = m.nav.filter(n => S.reg.nave[n.cls].guerra).length;
    const D = {};
    D.grano = pob * S.bien[B.grano].pc / 1000;
    D.bienes = pob * S.bien[B.bienes].pc / 1000;
    D.filtros = pobEst * S.bien[B.filtros].pc / 1000;
    D.armas = 0.5 * m.est.length + nGuerra * 0.01;
    const nucDia = (nNav + m.ins.filter(i => S.reg.inst[i.tipo].nuc).length) / (7 * S.ANIO) * 1.3;
    D.piezas = D.bienes * 2 / 5 + D.filtros * 0.75 + D.armas * 1.5 + pob * S.bien[B.piezas].pc / 1000 + nNav * 0.03 + nucDia * 8 + nNav * 0.02;
    D.metal = D.piezas * 8 / 5 + D.bienes * 3 / 5 + D.armas * 2 + nucDia * 12 + nNav * 0.04;
    D.comb = pob * S.bien[B.comb].pc / 1000 + nNav * 1.0 + D.metal / 3;
    D.mineral = D.metal * 2 + D.comb * 10 / 18;
    m.demBase = D; m.nucDia = nucDia;
    const escala = (tipo, salida, objetivo) => { let s = 0; const l = m.ins.filter(i => i.tipo === tipo); for (const i of l) s += i.nivel * salida; if (s <= 0) return; const f = objetivo / s; for (const i of l) i.nivel = Math.max(0.3, Math.round(i.nivel * f * 10) / 10); };
    // El grano: cosecha anual = consumo + semilla (15 %) + margen. La hidroponía cubre parte en las estaciones.
    let hidro = 0; for (const i of m.ins) if (i.tipo === 'hidroponia') hidro += i.nivel * 5;
    escala('granja', 14000 / S.ANIO, Math.max(1, D.grano - hidro) * 1.42);
    escala('fab_bienes', 5, D.bienes * 1.15); escala('fab_filtros', 2, D.filtros * 1.2); escala('armeria', 2, D.armas * 1.15);
    escala('fab_piezas', 5, D.piezas * 1.15); escala('fundicion', 12, D.metal * 1.15); escala('refineria', 18, D.comb * 1.5); escala('mina', 30, (D.mineral + D.comb * 0.35 * 10 / 18) * 1.3);
    const fn = m.ins.filter(i => i.tipo === 'fab_nucleos'); for (const i of fn) i.nivel = Math.max(0.5, Math.round(nucDia * 5 / fn.length * 1.35 * 10) / 10);
    for (const i of m.ins) S.soc.ajustarPlantilla(m, i);
  }

  function genStock(m) {
    const rng = m.rng;
    const fabs = m.ins.filter(i => i.tipo === 'fab_nucleos');
    const usado = (o) => { const nf = S.obj.ciclosHastaFallo(o); const n = rng.r(0, 0.75) * nf; const b = 1 / Math.sqrt(o.a0) - 0.5 * o.K * n; o.a0 = 1 / (b * b); o.t0 = m.t - n / 3; };
    for (const i of m.ins) {
      if (!S.reg.inst[i.tipo].nuc) continue;
      const o = S.obj.nuevoNucleo(m, rng.el(fabs)); usado(o); o.donde = { t: 'I', id: i.id }; i.nuc = o.id; S.obj.activar(m, o, 3);
    }
    for (const n of m.nav) { const o = S.obj.nuevoNucleo(m, rng.el(fabs)); usado(o); o.donde = { t: 'N', id: n.id }; n.nuc = o.id; S.obj.activar(m, o, 3); }
    for (const a of m.ase) {
      const k = a.ins.some(i => m.ins[i].tipo === 'fab_nucleos') ? 8 : a.ins.some(i => m.ins[i].tipo === 'astillero') ? 6 : (a.tipo === 'comercial' || a.cap) ? 4 : a.sinley ? 1 : 2;
      for (let j = 0; j < k; j++) { const o = S.obj.nuevoNucleo(m, rng.el(fabs)); o.donde = { t: 'A', id: a.id }; a.nuc.push(o.id); }
    }
    for (const a of m.ase) S.eco.stockInicial(m, a);
  }
  // El mundo ya llevaba tiempo funcionando: en cada tablón se conocen los precios de los vecinos.
  function tablones(m) {
    for (const a of m.ase) S.inf.anotarPrecios(m, a);
    for (const a of m.ase) for (const d of a.cerca) { const e = m.ase[d].tabla.get(d); a.tabla.set(d, { t: e.t - m.rng.r(0, 6), p: e.p, T: e.T, x: e.x }); }
  }

  function arrancar(m) {
    for (const a of m.ase) {
      // Asentamientos a los que llega un mercader desde aquí: hasta 4 saltos, los 40 más cercanos.
      a.cerca = m.ase.filter(b => b.id !== a.id && m.saltos[a.sis][b.sis] <= 4).sort((x, y) => m.dist[a.sis][x.sis] - m.dist[a.sis][y.sis] || x.id - y.id).slice(0, 30).map(b => b.id);
      const f = (a.id * 0.6180339887) % 1;
      m.prog(f, 'asent.dia', { a: a.id }); m.prog(f * 30 + 1, 'asent.mes', { a: a.id });
      if (a.semNec > 0) m.prog(a.cosechaDia, 'cosecha', { a: a.id });
    }
    tablones(m);
    for (const n of m.nav) m.prog(m.rng.r(0.1, 6), 'nave.decide', { n: n.id });
    m.prog(S.MES, 'sismo'); m.prog(S.ANIO, 'anio');
  }

  G.crear = function (semilla, op) {
    op = Object.assign({ sistemas: 40 }, op || {});
    const m = new S.Mundo(semilla); m.op = op; m._nombres = new Set();
    genSistemas(m, op.sistemas); genRutas(m); genAsentamientos(m); genEstados(m); genEspeciales(m);
    genGente(m); genNaves(m); S.mas.sembrar(m); equilibrar(m); genStock(m); arrancar(m);
    m.reg('genesis', 'El universo empieza a contar (semilla ' + m.semilla + '): ' + m.sis.length + ' sistemas, ' + m.ase.length + ' asentamientos, ' + m.est.length + ' estados, ' + m.nav.length + ' naves', { imp: 1 });
    return m;
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
