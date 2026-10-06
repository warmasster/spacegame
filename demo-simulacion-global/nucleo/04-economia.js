// Economía: no hay mercado galáctico. Cada asentamiento produce, consume y pone su precio local:
//   p = p_ref · (D/S)^0,8   (con límites)
// La cosecha es anual: comer, sembrar y tributar salen del mismo montón (la trampa del informe falso).
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const NB = S.NB;
  const E = S.eco = {};
  E.ARANCEL = 0.04; E.AL_ESTADO = 0.15; E.P_NUCLEO = 6000;   // del arancel, la parte que llega al tesoro

  E.nuevoAsent = function (m, sis, tipo, nom) {
    const pl = S.reg.asent[tipo]; const id = m.ase.length; const rng = m.rngDe('A', id);
    const k = sis.ase.length;
    const a = {
      id, nom, sis: sis.id, tipo, forma: pl.forma, estacion: !!pl.estacion, sinley: !!pl.sinley,
      orb: { r: 9 + k * 7.5 + rng.r(-1.5, 1.5), fase: rng.r(0, 6.283), vel: (0.05 + rng.r(0, 0.03)) / Math.pow(1 + k, 1.2) },
      pob: Math.round(rng.r(pl.pob[0], pl.pob[1])), hor: pl.forma === 'planeta' ? 30 : 55, est: -1, cap: false, ins: [], coh: [], gob: -1, uni: -1, cohGen: -1,
      alm: new Array(NB).fill(0), res: new Array(NB).fill(0), acap: new Array(NB).fill(0), acapFac: -1,
      dem: new Array(NB).fill(0), prod: new Array(NB).fill(0), pr: new Array(NB).fill(0), demHoy: new Array(NB).fill(0), prodHoy: new Array(NB).fill(0),
      sem: 0, semNec: 0, cosechaDia: Math.floor(rng.r(0, S.ANIO)), cosechaNormal: 0, diasCosecha: 0, plaga: 0, perdon: 0,
      H: 0, Hf: 0, apagon: false, hambruna: -1, causaHambre: -1,
      culpa: { reg: 1, acap: 0.5, nat: 1, ext: 0 }, agr: 0.3, R: 1, f: 0.03, ret: 0, senal: 0, piG: 1, piE: 1, revuelta: -1,
      not: new Map(), buzon: [], atr: [], tabla: new Map(), rie: null, nuc: [], pedidos: [], robados: new Set(), busca: new Map(),
      ley: 0.5, sensor: null, comisario: -1, inspector: -1, control: 1, retraso: 0, ultInf: 0, hechos: [], huelga: 0, ocup: null, reparto: 0,
      conv: new Map(), cerca: [], rng, idMasacre: -1, pirTraf: 0, tie: -1,
    };
    m.ase.push(a); sis.ase.push(id);
    a.cohGen = S.soc.nuevaCohorte(m, a, -1, a.pob).id;
    for (const t in pl.ins) { const [mn, mx] = pl.ins[t]; const niv = rng.r(mn, mx + 0.999); if (niv < 0.5 && mn === 0) continue; E.nuevaInst(m, a, t, t === 'granja' ? Math.max(0.3, Math.min(niv, mx)) : Math.max(1, Math.floor(niv))); }
    return a;
  };

  E.nuevaInst = function (m, a, tipo, nivel, o) {
    const def = S.reg.inst[tipo]; const id = m.ins.length; const rng = a.rng;
    const i = Object.assign({ id, tipo, nom: def.nom + ' de ' + a.nom, ase: a.id, nivel, salud: 1, coh: -1, nuc: -1, jefe: -1, prog: 0, ef: 1, huelga: false, dueno: { t: 'L', id: a.id }, x: rng.f(), y: rng.f() }, o || {});
    if (def.nucleos) {
      const ap = rng.el(S.NOM.apellido); i.marca = ap + ' ' + 'KRTVHM'[rng.i(6)] + '-' + (2 + rng.i(8));
      i.nom = rng.el(S.NOM.marca) + ' ' + ap + ', planta ' + (1 + rng.i(4)) + ' (' + a.nom + ')'; i.lote = 10 + rng.i(60); i.serieSig = 1000 + rng.i(8000); i.enLote = 0; i.loteDef = false; i.evLote = -1;
    }
    if (tipo === 'policia' || tipo === 'cuartel' || tipo === 'palacio' || tipo === 'ceca' || tipo === 'astillero') i.dueno = { t: 'E', id: -1 };
    m.ins.push(i); a.ins.push(id);
    if (def.trab > 0) i.coh = S.soc.nuevaCohorte(m, a, id, Math.round(def.trab * nivel)).id;
    if (def.cosecha) { a.semNec += nivel * def.cosecha * 0.15; a.cosechaNormal += nivel * def.cosecha; }
    if (def.nucleos) a.prodNuc = true;
    return i;
  };

  E.cap = function (m, a) { let c = 2500; for (const id of a.ins) { const i = m.ins[id]; if (i.tipo === 'almacen') c += 5000 * i.nivel * i.salud; } return c; };

  // p = p_ref·(D/S)^0,8. D = demanda sobre el horizonte (hasta la próxima cosecha si el grano es de aquí); S = lo que hay.
  // Lo que el asentamiento querría tener en almacén (D sobre el horizonte). 0 = aquí eso no lo usa nadie.
  E.objetivo = function (a, c) {
    const d = Math.max(a.dem[c], 0.15 * a.prod[c]); if (d < 0.01) return 0;
    // El grano que es de aquí hay que guardarlo hasta la próxima cosecha; el que se importa, un mes.
    if (c === B.grano && a.semNec > 0) return d * 30 + Math.min(d, a.cosechaNormal * 0.85 / S.ANIO) * Math.max(0, a.diasCosecha - 10);
    return d * a.hor;                                                          // una estación guarda para más días que un planeta
  };
  E.curva = (pref, T, s) => pref * S.clamp(Math.pow(T / Math.max(s, T * 0.02), 0.8), 0.3, 6);
  E.precio = function (a, c, stock) {
    const T = E.objetivo(a, c); if (T <= 0) return S.bien[c].pref * 0.25;      // nadie lo quiere: no es un mercado
    return E.curva(S.bien[c].pref, T, stock === undefined ? a.alm[c] : stock);
  };
  E.precioNucleo = (a, n) => E.P_NUCLEO * S.clamp(Math.pow(2.5 / Math.max(n === undefined ? a.nuc.length : n, 0.5), 0.8), 0.5, 2.5);
  E.nivelPrecios = (m, a) => a.est >= 0 ? m.est[a.est].mon.P : 1;

  // Un mercader compra o vende: el precio se mueve con la propia operación (por eso las manadas hunden el precio).
  E.comprar = function (m, a, c, q) {
    q = Math.min(q, a.alm[c]); if (q <= 0) return { q: 0, coste: 0 };
    const p = E.precio(a, c, a.alm[c] - q / 2); a.alm[c] -= q; const v = p * q;
    if (a.est >= 0) m.est[a.est].ingr += v * E.ARANCEL * E.AL_ESTADO;
    return { q, coste: v * (1 + E.ARANCEL), p };
  };
  E.vender = function (m, a, c, q) {
    if (q <= 0) return { q: 0, ingreso: 0 };
    const p = E.precio(a, c, a.alm[c] + q / 2); a.alm[c] += q; const v = p * q;
    if (a.est >= 0) m.est[a.est].ingr += v * E.ARANCEL * E.AL_ESTADO;
    return { q, ingreso: v * (1 - E.ARANCEL), p };
  };

  E.stockInicial = function (m, a) {
    const kp = a.pob / 1000;
    for (let c = 0; c < NB; c++) { const b = S.bien[c]; if (b.pc && (!b.soloEstacion || a.estacion)) a.dem[c] = b.pc * kp; }
    for (const id of a.ins) { const i = m.ins[id]; const def = S.reg.inst[i.tipo]; if (def.ent) for (const c in def.ent) a.dem[B[c]] += def.ent[c] * i.nivel; if (def.sal) for (const c in def.sal) a.prod[B[c]] += def.sal[c] * i.nivel; if (def.nucleos) { a.dem[B.metal] += def.nucleos.metal / def.nucleos.dias * i.nivel; a.dem[B.piezas] += def.nucleos.piezas / def.nucleos.dias * i.nivel; } }
    a.dem[B.comb] += 2 + Math.sqrt(a.pob) / 40;                                 // tráfico del puerto
    for (let c = 0; c < NB; c++) a.alm[c] = a.dem[c] * (a.hor + 4) + a.prod[c] * 5;
    a.alm[B.piezas] += 10;
    if (a.semNec > 0) {
      a.sem = a.semNec; a.diasCosecha = a.cosechaDia;
      // El año ya está empezado: queda lo que falta por comer hasta la cosecha y la parte del excedente aún sin vender.
      const fr = a.cosechaDia / S.ANIO; const comer = a.dem[B.grano] * (a.cosechaDia + 25); const sobra = Math.max(0, a.cosechaNormal * 0.85 - a.dem[B.grano] * S.ANIO);
      const trib = a.est >= 0 && !a.cap ? Math.min(sobra, m.est[a.est].trib * a.cosechaNormal) : 0;
      a.alm[B.grano] = comer + (sobra - trib) * fr; a.res[B.grano] = trib * fr;
    }
    for (let c = 0; c < NB; c++) a.pr[c] = E.precio(a, c);
  };

  // ── El día de un asentamiento.
  E.dia = function (m, a) {
    const alm = a.alm, dh = a.demHoy, ph = a.prodHoy; dh.fill(0); ph.fill(0);
    a.diasCosecha = ((a.cosechaDia - (m.t % S.ANIO)) + S.ANIO) % S.ANIO;
    const apag = a.apagon ? 0.75 : 1;
    for (let x = 0; x < a.ins.length; x++) {
      const i = m.ins[a.ins[x]]; const def = S.reg.inst[i.tipo];
      if (i.salud < 1 && !def.indestructible) E.reparar(m, a, i);       // también lo arrasado se vuelve a levantar, si hay con qué
      if (!def.ent && !def.sal && !def.nucleos && !def.astillero) continue;
      const co = i.coh >= 0 ? m.coh[i.coh] : null; const plant = def.trab * i.nivel;
      const wf = plant > 0 && co ? Math.min(1, co.n / plant) : 1;
      let ef = i.salud < 0.05 || i.huelga ? 0 : i.salud * wf * apag;
      if (def.nuc && (i.nuc < 0 || !m.obj[i.nuc].vivo)) { ef *= 0.3; if (ef > 0) E.reponerNucleo(m, a, i); }   // sin núcleo: generadores auxiliares
      if (def.sal && def.sal.mineral && a.remolc) ef *= 1.25;
      i.ef = ef; const k = ef * i.nivel; let esc = 1;
      if (def.ent) {
        for (const c in def.ent) { const need = def.ent[c] * k; dh[B[c]] += def.ent[c] * i.nivel * i.salud * wf; if (need > 0) esc = Math.min(esc, alm[B[c]] / need); }
        esc = S.clamp(esc, 0, 1);
        for (const c in def.ent) alm[B[c]] -= def.ent[c] * k * esc;
      }
      if (def.sal) for (const c in def.sal) { const q = def.sal[c] * k * esc; alm[B[c]] += q; ph[B[c]] += q; }
      if (def.nucleos) E.fabricarNucleos(m, a, i, k);
      if (def.astillero) E.astillero(m, a, i, k);
    }
    // Consumo de la población.
    const kp = a.pob / 1000;
    for (let c = 0; c < NB; c++) {
      const b = S.bien[c]; if (!b.pc || (b.soloEstacion && !a.estacion)) continue;
      const need = b.pc * kp; dh[c] += need;
      if (c === B.grano) {
        // El Estado suelta su reserva en el mercado cuando falta: si hay hambre, eso es un reparto a la vista.
        if (a.res[c] > 0 && alm[c] < a.dem[c] * 25) { const q = Math.min(a.res[c], need * 1.5); a.res[c] -= q; alm[c] += q; if (a.est >= 0) m.est[a.est].ingr += q * a.pr[c] * 0.1; if (a.H > 0.1) a.reparto += q / need; }
        const caro = a.pr[c] / b.pref; const puede = S.clamp(1 - 0.25 * Math.max(0, caro - 2), 0.4, 1);
        const come = Math.min(need * puede, alm[c]); alm[c] -= come;
        a.H += ((1 - come / need) - a.H) * 0.08;
      } else {
        const q = Math.min(need, alm[c]); alm[c] -= q;
        if (c === B.filtros) a.Hf += ((1 - q / need) - a.Hf) * 0.05;
        if (c === B.comb) a.apagon = q < need * 0.5;
      }
    }
    const cap = E.cap(m, a);
    for (let c = 0; c < NB; c++) {
      a.dem[c] += (dh[c] - a.dem[c]) * 0.1; a.prod[c] += (ph[c] - a.prod[c]) * 0.1;
      if (alm[c] > cap && !(c === B.grano && a.semNec > 0)) alm[c] = cap + (alm[c] - cap) * 0.9;   // lo que no cabe se estropea
      a.pr[c] = E.precio(a, c);
    }
    // Población: crece despacio; el hambre y el aire sucio matan.
    // Trampa maltusiana: con el grano barato la población crece; caro, se estanca.
    let dp = a.pob * 0.0000055 * (a.H > 0.05 ? 0 : S.clamp(2 - a.pr[B.grano] / S.bien[B.grano].pref, 0.3, 2.2));
    const mh = a.pob * (Math.max(0, a.H - 0.2) * 0.0012 + a.Hf * 0.00015);
    dp -= mh; a.muertosHambre = (a.muertosHambre || 0) + a.pob * Math.max(0, a.H - 0.2) * 0.0012;
    a.pob = Math.max(200, a.pob + dp);
    if (a.hambruna < 0 && a.H > 0.3 && m.t > (a.sinHambruna || 0)) {
      a.muertosHambre = 0;
      a.hambruna = m.reg('hambruna', 'Hambre en {A' + a.id + '}: la cantina sirve pasta gris y hay colas en el reparto (el grano está a ' + Math.round(a.pr[B.grano] * E.nivelPrecios(m, a)) + ')', { c: [a.causaHambre], a: a.id, imp: 2, d: { H: a.H } });
      S.emitir(m, 'hambruna', a, a.hambruna);
    } else if (a.hambruna >= 0 && a.H < 0.08) {
      m.reg('fin_hambruna', 'Vuelve a haber pan en {A' + a.id + '}; el hambre se llevó a ' + Math.round(a.muertosHambre) + ' personas', { c: [a.hambruna], a: a.id, imp: 1, d: { muertos: Math.round(a.muertosHambre) } });
      if (a.muertosHambre > 30) S.soc.hecho(m, a, a.hambruna, 'hambre', Math.min(1, a.muertosHambre / 400), Math.round(a.muertosHambre));
      a.hambruna = -1; a.causaHambre = -1; a.sinHambruna = m.t + 150;
    }
  };

  E.reparar = function (m, a, i) {
    const need = 0.25 * i.nivel; if (a.alm[B.metal] < need || a.alm[B.piezas] < need * 0.4) return;
    a.alm[B.metal] -= need; a.alm[B.piezas] -= need * 0.4; a.demHoy[B.metal] += need; a.demHoy[B.piezas] += need * 0.4;
    i.salud = Math.min(1, i.salud + (i.salud < 0.05 ? 0.0025 : 0.004));
    if (i.salud >= 1 && i.evDano !== undefined) { m.reg('reparada', 'Vuelve a funcionar {I' + i.id + '}', { c: [i.evDano], a: a.id, imp: 0 }); i.evDano = undefined; }
  };
  E.reponerNucleo = function (m, a, i) {
    if (!a.nuc.length) return;
    const o = m.obj[a.nuc.shift()]; o.donde = { t: 'I', id: i.id }; i.nuc = o.id; S.obj.activar(m, o, 3);
  };

  // Fábrica de núcleos: produce por lotes; el jefe de calidad decide si se queda el dinero de las inspecciones.
  E.fabricarNucleos = function (m, a, i, k) {
    const d = S.reg.inst[i.tipo].nucleos; const alm = a.alm;
    const nm = d.metal / d.dias * k, np = d.piezas / d.dias * k;
    a.demHoy[B.metal] += d.metal / d.dias * i.nivel; a.demHoy[B.piezas] += d.piezas / d.dias * i.nivel;
    if (a.nuc.length >= 10) return;                                // almacén de núcleos lleno
    const esc = S.clamp(Math.min(nm > 0 ? alm[B.metal] / nm : 1, np > 0 ? alm[B.piezas] / np : 1), 0, 1);
    alm[B.metal] -= nm * esc; alm[B.piezas] -= np * esc;
    i.prog += k * esc / d.dias;
    while (i.prog >= 1) {
      i.prog -= 1;
      if (i.enLote === 0) S.emitir(m, 'lote.nuevo', i, a);
      const o = S.obj.nuevoNucleo(m, i, { defectuoso: i.loteDef });
      if (i.loteDef) o.evDefecto = i.evLote;
      o.donde = { t: 'A', id: a.id }; a.nuc.push(o.id);
      if (++i.enLote >= 12) { i.enLote = 0; i.lote++; }
    }
  };

  // Astillero: consume materiales al ritmo de la obra y al final necesita un núcleo de verdad.
  E.pedirNave = function (m, a, cls, o) {
    const p = Object.assign({ cls, prog: 0, t: m.t }, o); a.pedidos.push(p); return p;
  };
  E.astillero = function (m, a, i, k) {
    const p = a.pedidos[0]; if (!p) return;
    const def = S.reg.nave[p.cls]; const alm = a.alm;
    if (p.prog < 1) {
      // Curva de aprendizaje: un astillero con oficio construye más deprisa.
      let esc = 1; const paso = k / def.dias * (1 + 0.6 * (1 - Math.exp(-(i.exp || 0) / 400)));
      p.falta = p.falta || { metal: 1, piezas: 1, armas: 1 };
      for (const c in def.mat) { const need = def.mat[c] * paso; a.demHoy[B[c]] += def.mat[c] / def.dias * i.nivel; const r = need > 0 ? Math.min(1, alm[B[c]] / need) : 1; p.falta[c] += (r - p.falta[c]) * 0.03; if (r < esc) esc = r; }
      esc = S.clamp(esc, 0, 1);
      for (const c in def.mat) alm[B[c]] -= def.mat[c] * paso * esc;
      p.prog += paso * esc;
    }
    if (p.prog >= 1) {
      if (!a.nuc.length) { if (!p.espera) { p.espera = m.reg('astillero_parado', 'El casco de un ' + def.nom.toLowerCase() + ' espera en {I' + i.id + '}: no hay núcleos de reactor', { a: a.id, imp: 0, c: [a.evSinNucleos] }); } return; }
      a.pedidos.shift();
      const nuc = a.nuc.shift();
      S.emitir(m, 'nave.botada', a, i, p, nuc);
    }
  };

  // Daño a una instalación (bomba, explosión, sabotaje). Devuelve el hecho.
  E.danarInst = function (m, i, o) {
    const a = m.ase[i.ase]; const def = S.reg.inst[i.tipo]; const co = i.coh >= 0 ? m.coh[i.coh] : null;
    const muertos = co ? Math.min(co.n, Math.round(co.n * (o.muertos || 0))) : 0;
    i.salud = Math.max(0, i.salud - (o.dano === undefined ? 1 : o.dano));
    const ev = m.reg(o.k || 'explosion', o.txt.replace('{n}', muertos === 0 ? 'nadie' : S.numPal(muertos)).replace('{N}', muertos), { c: o.c, a: a.id, imp: o.imp === undefined ? (muertos >= 8 ? 2 : 1) : o.imp, d: { ins: i.id, muertos, tipo: i.tipo } });
    i.evDano = ev;
    if (i.salud <= 0.05 && i.nuc >= 0 && m.obj[i.nuc].vivo) { const nu = m.obj[i.nuc]; nu.vivo = false; if (nu.falla) nu.falla.x = true; S.obj.anotar(m, nu, ev); i.nuc = -1; }
    if (def.cosecha) a.evSemilla = ev;
    if (def.nucleos && i.salud <= 0.3) { for (const b of m.ase) b.evSinNucleos = ev; }
    if (i.tipo === 'granja' || i.tipo === 'hidroponia' || i.tipo === 'almacen') { a.causaHambre = ev; if (i.tipo === 'almacen') { const f = 0.5; a.alm[B.grano] *= f; } }
    if (muertos > 0) S.soc.muertes(m, a, co, muertos, ev, o);
    S.emitir(m, 'inst.danada', i, ev, o);
    return ev;
  };

  // ── Cosecha anual, tributo y siembra.
  S.en('cosecha', function (m, e) {
    const a = m.ase[e.a]; let cap = 0, normal = 0;
    for (const id of a.ins) { const i = m.ins[id]; const def = S.reg.inst[i.tipo]; if (!def.cosecha) continue; const co = m.coh[i.coh]; const wf = Math.min(1, co.n / (def.trab * i.nivel)); cap += i.nivel * def.cosecha * i.salud * wf; normal += i.nivel * def.cosecha; }
    a.cosechaNormal = normal; a.semNec = normal * 0.15;
    const frSem = a.semNec > 0 ? S.clamp(a.sem / a.semNec, 0, 1) : 1;
    const q = cap * frSem * (1 - a.plaga);
    const perdida = normal > 0 ? S.clamp(1 - q / normal, 0, 1) : 0;
    a.alm[B.grano] += q; a.sem = 0; a.ultCosecha = q;
    // El tributo se aparta el mismo día (lo que la capital exige de siempre); lo que quede se reparte entre comer y sembrar.
    a.tribPend = 0;
    if (a.est >= 0 && !a.cap) { const T0 = m.est[a.est].trib * normal * S.clamp(a.control + 0.3, 0.3, 1); a.tribPend = Math.min(T0, a.alm[B.grano]); a.alm[B.grano] -= a.tribPend; }
    E.sembrar(a);
    const causas = []; if (a.plaga > 0.02) causas.push(a.evPlaga); if (frSem < 0.95) causas.push(a.evSemilla); if (cap < normal * 0.9) causas.push(a.evSemilla);
    const ev = m.reg('cosecha', 'Cosecha en {A' + a.id + '}: ' + S.fmt(q) + ' t' + (perdida > 0.12 ? ' (un ' + Math.round(perdida * 100) + ' % menos de lo normal)' : ''), { c: causas, a: a.id, imp: perdida > 0.2 ? 1 : 0, d: { q, perdida } });
    if (perdida > 0.12) a.causaHambre = ev;
    a.perdidaReal = perdida; a.plaga = 0; a.perdon = 0;
    S.emitir(m, 'cosecha', a, perdida, ev);
    m.prog(m.t + 30, 'tributo', { a: a.id, ev });
    m.prog(m.t + S.ANIO, 'cosecha', { a: a.id });
  });
  // Si no llega para comer y sembrar, se aprietan el cinturón; si falta mucho, se reparte la escasez entre las dos cosas.
  E.sembrar = function (a) {
    // «Comer» es la parte del consumo que esta cosecha tiene que cubrir (un mundo que importa casi todo no se come la semilla).
    const alm = a.alm; const total = alm[B.grano] + a.sem; const comer = Math.min(a.dem[B.grano] * S.ANIO * 0.92, a.cosechaNormal * 0.55); const need = comer + a.semNec;
    a.sem = total >= need * 0.75 ? Math.min(a.semNec, total) : a.semNec * total / need; alm[B.grano] = total - a.sem;
    return need > 0 ? total / need : 1;
  };
  S.en('tributo', function (m, e) {
    const a = m.ase[e.a]; const alm = a.alm; let ev = e.ev; let tomado = 0;
    if (a.tribPend > 0) {
      // Solo si llegó a tiempo el perdón de la capital se devuelve parte de lo apartado.
      const dev = a.est >= 0 ? a.tribPend * S.clamp(a.perdon, 0, 1) : a.tribPend; tomado = a.tribPend - dev;
      alm[B.grano] += dev; a.res[B.grano] += tomado; a.tribPend = 0;
    }
    const cubre = E.sembrar(a); const hay = cubre, need = 1;
    if (a.sem < a.semNec * 0.97) {
      const def = 1 - a.sem / a.semNec;
      if (tomado > 0) {
        ev = m.reg('confiscacion', 'Los soldados se llevan el tributo de {A' + a.id + '} (' + S.fmt(tomado) + ' t) a la vista de todos, hasta el grano de siembra: solo se podrá sembrar el ' + Math.round((1 - def) * 100) + ' %', { c: [e.ev, a.evInforme], a: a.id, imp: def > 0.15 ? 2 : 1, d: { tomado, def } });
        a.culpa.reg += 10 * def + 1; S.soc.hecho(m, a, ev, 'hambre', Math.min(1, def * 2), 0);
      } else a.culpa.nat += 6 * def;
      a.causaHambre = ev; a.evSemilla = ev;
    }
    void hay; void need;
  });

  // Plagas: de vez en cuando, una arrasa los cultivos de un mundo. Lo que haga el gobernador con el informe es cosa suya.
  S.gancho('anio', function (m) {
    for (const a of m.ase) {
      if (a.semNec <= 0 || !a.rng.p(0.045)) continue;
      a.plaga = a.rng.r(0.15, 0.45); a.culpa.nat += 2;
      a.evPlaga = m.reg('plaga', 'Una plaga se extiende por los cultivos de {A' + a.id + '}: se perderá en torno a un ' + Math.round(a.plaga * 100) + ' % de la cosecha', { a: a.id, imp: 1, d: { plaga: a.plaga } });
    }
  });

  S.en('asent.dia', function (m, e) {
    const a = m.ase[e.a];
    E.dia(m, a); S.soc.dia(m, a);
    m.prog(e.t + 1, 'asent.dia', e);
  });

  S.sismografo('pob', 'Población total', m => { let s = 0; for (const a of m.ase) s += a.pob; return s; });
  S.sismografo('hambre', 'Hambre media', m => { let s = 0, p = 0; for (const a of m.ase) { s += a.H * a.pob; p += a.pob; } return s / p; });
  S.sismografo('pgrano', 'Precio medio del grano', m => { let s = 0, p = 0; for (const a of m.ase) { s += a.pr[B.grano] * a.pob; p += a.pob; } return s / p; });
  S.sismografo('pcomb', 'Precio medio del combustible', m => { let s = 0; for (const a of m.ase) s += a.pr[B.comb]; return s / m.ase.length; });
  S.sismografo('nucleos', 'Núcleos de reactor en almacén', m => { let s = 0; for (const a of m.ase) s += a.nuc.length; return s; });
  S.gancho('sismo', function (m) { for (const a of m.ase) { m.serie('H' + a.id, a.H); m.serie('G' + a.id, a.pr[B.grano]); m.serie('Q' + a.id, a.agr); m.serie('R' + a.id, a.R); m.serie('f' + a.id, a.f); m.serie('p' + a.id, a.pob); } });
})(typeof globalThis !== 'undefined' ? globalThis : this);
