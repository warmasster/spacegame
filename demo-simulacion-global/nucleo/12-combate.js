// Combate de flotas con las ecuaciones de Lanchester: dA/dt = −b·B, dB/dt = −a·A.
// La fuerza crece con el cuadrado del número; entre asteroides se pelea con la ley lineal.
// Los coeficientes a y b ya no son fijos: salen del diseño, la calidad de cada componente, la veteranía,
// la doctrina, la moral y lo lejos que se pelea de casa. En tierra, los combates duran días (ley lineal).
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const R = S.R;
  const Co = S.com = {};

  // Forma cerrada de la ley cuadrática: gana A si a·A0² > b·B0²; quedan √((a·A0² − b·B0²)/a).
  Co.cuadratica = function (A0, B0, a, b) {
    const x = a * A0 * A0, y = b * B0 * B0;
    return x > y ? { gana: 'A', quedan: Math.sqrt((x - y) / a) } : { gana: 'B', quedan: Math.sqrt((y - x) / b) };
  };
  // Integración paso a paso (para comprobar la forma cerrada y para las batallas en curso).
  Co.integrar = function (A, Bn, a, b, ley, dias, pasos, k) {
    const dt = dias / pasos;
    for (let i = 0; i < pasos && A > 0 && Bn > 0; i++) {
      let dA, dB;
      if (ley === 'lineal') { dA = -b * A * Bn / k; dB = -a * A * Bn / k; } else { dA = -b * Bn; dB = -a * A; }
      A = Math.max(0, A + dA * dt); Bn = Math.max(0, Bn + dB * dt);
    }
    return [A, Bn];
  };

  const enGuerra = (m, x, y) => x >= 0 && y >= 0 && x !== y && (m.est[x].gue.has(y) || m.est[y].gue.has(x));
  Co.enGuerra = enGuerra;
  // Moral y logística de un bando: pagas, cohesión (asabiya) y el gradiente de pérdida de fuerza con la distancia a la base.
  Co.moralDet = function (m, l, sis) {
    const e = l.est >= 0 ? m.est[l.est] : null; if (!e) return { paga: 1, cohesion: 1, lejos: 0, dist: 0, msc: 0, asabiya: 0.6, mando: 1, total: 0.9 };
    const alm = l.alm >= 0 ? m.per[l.alm] : null; const mando = alm && alm.vivo && alm.fig === 'invicto' ? 1.15 : 1;
    let d = 0, k = 0; for (const fid of l.flo) { const f = m.flo[fid]; d += m.dist[m.ase[f.base].sis][sis]; k++; }
    const lejos = k ? Math.min(1, d / k / 800) : 0; const asabiya = e.asabiya === undefined ? 0.6 : e.asabiya;
    const paga = e.msc ? Math.max(0.6, 1 - 0.08 * e.msc) : 1, cohesion = 0.7 + 0.5 * asabiya;
    return { paga, cohesion, lejos, dist: k ? d / k : 0, msc: e.msc || 0, asabiya, mando, total: paga * cohesion * (1 - 0.35 * lejos) * mando };
  };
  Co.moral = function (m, l, sis) { return Co.moralDet(m, l, sis).total; };
  // La cuenta de la pegada de un bando, factor a factor (medias ponderadas por cascos): para poder explicarla.
  Co.desglose = function (m, l, enemigo, sis) {
    let k = 0, base = 0, dis = 0, tec = 0, vet = 0;
    for (const id of l.nav) { const n = m.nav[id]; if (!n.vivo) continue; const c = n.cascos; base += S.reg.nave[n.cls].ef * c; dis += S.tec.bruto(S.tec.efectivo(m, n), enemigo.g) / S.tec.R0 * c; tec += Math.pow(S.tec.nivel(m, n) / enemigo.niv, 0.7) * c; vet += (n.vet || 0) * c; k += c; }
    if (!k) return null; const e = l.est >= 0 ? m.est[l.est] : null;
    return { base: base / k, dis: dis / k, tec: tec / k, vet: vet / k, doc: e && e.doc ? e.doc.naval : 0.2, niv: e ? e.tec : 0.85, nivRival: enemigo.niv, moral: Co.moralDet(m, l, sis) };
  };
  // Quién va ganando: en espacio abierto la fuerza es pegada × número²; entre asteroides, pegada × número.
  Co.fuerza = function (bt) { const p = bt.ley === 'lineal' ? 1 : 2; const x = bt.L[0].ef * Math.pow(bt.L[0].n, p), y = bt.L[1].ef * Math.pow(bt.L[1].n, p); return x + y > 0 ? x / (x + y) : 0.5; };
  // Pronóstico: lo que pasaría si todo siguiera como hoy (misma pegada y la misma regla de retirada).
  Co.pronostico = function (m, bt) {
    let A = bt.L[0].n, Bn = bt.L[1].n; const a = bt.L[0].ef, b = bt.L[1].ef; const d0 = Math.max(0, Math.round(m.t - bt.t0));
    const val = bt.L.map(l => { const f = m.flo[l.flo[0]]; const p = f && f.alm >= 0 ? m.per[f.alm] : null; return p ? p.r[R.VAL] : 0; });
    for (let d = 1; d <= 360; d++) {
      const r = Co.integrar(A, Bn, a, b, bt.ley, 1, 8, bt.k); A = r[0]; Bn = r[1];
      if (A < 0.5 || Bn < 0.5) return { gana: A >= Bn ? 0 : 1, dias: d, quedan: [A, Bn], huye: -1 };
      const fa = a * A * A, fb = b * Bn * Bn;
      if (d0 + d >= 2) { if (fa < fb * 0.55 && val[0] < 0.8) return { gana: 1, dias: d, quedan: [A, Bn], huye: 0 }; if (fb < fa * 0.55 && val[1] < 0.8) return { gana: 0, dias: d, quedan: [A, Bn], huye: 1 }; }
    }
    return { gana: -1, dias: 360, quedan: [A, Bn], huye: -1 };
  };
  Co.pronosticoTierra = function (m, t) {
    const K = (t.atk.n0 + t.def.n0) / 2; let A = t.atk.n, D = t.def.n; const d0 = Math.round(m.t - t.t0);
    for (let d = 1; d <= 60; d++) {
      for (let i = 0; i < 4; i++) { const dA = t.def.ef * A * D / K * 0.25, dD = t.atk.ef * A * D / K * 0.25; A = Math.max(0, A - dA); D = Math.max(0, D - dD); }
      if (D < t.def.n0 * 0.25 || D < A * 0.2) return { gana: 0, dias: d, quedan: [A, D], plazo: false };
      if (A < t.atk.n0 * 0.3 || A < D * 0.25) return { gana: 1, dias: d, quedan: [A, D], plazo: false };
      if (d0 + d > 45) return { gana: 1, dias: d, quedan: [A, D], plazo: true };
    }
    return { gana: 1, dias: 60, quedan: [A, D], plazo: true };
  };
  // Bajas por casco y día de un bando contra el perfil del otro.
  Co.eficacia = function (m, l, enemigo, sis) {
    let s = 0, k = 0; for (const id of l.nav) { const n = m.nav[id]; if (n.vivo) { s += S.tec.tasa(m, n, enemigo) * n.cascos; k += n.cascos; } }
    return k ? s / k * Co.moral(m, l, sis) : 0;
  };
  Co.velFlota = function (m, naves) { let v = Infinity; for (const n of naves) { const d = n.dis !== undefined ? m.dis[n.dis] : null; const x = S.reg.nave[n.cls].vel * (d ? 0.8 + 1.8 * d.g[6] * n.q[6] : 1); if (x < v) v = x; } return v; };

  Co.mover = function (m, f, dest) {
    const naves = f.nav.map(i => m.nav[i]).filter(n => n.vivo && n.st !== 'viaje');
    if (!naves.length) return false;
    const s0 = f.sis; const s1 = dest.t === 'A' ? m.ase[dest.id].sis : dest.id;
    const d = S.nav.distancia(m, s0, s1);
    // Sin combustible la flota no sale.
    const puerto = naves[0].en >= 0 ? m.ase[naves[0].en] : null;
    if (puerto) {
      let falta = false; for (const n of naves) { const need = d * S.reg.nave[n.cls].fuel; S.nav.repostar(m, n, puerto, need); if (n.fuel < need) falta = true; }
      if (falta) { if (!f.evSinComb || m.t - m.ev[f.evSinComb].t > 120) f.evSinComb = m.reg('flota_parada', 'La ' + f.nom + ' no puede zarpar de {A' + puerto.id + '}: no hay combustible', { a: puerto.id, imp: 1, c: [puerto.evEscasez], d: { e: f.est } }); return false; }
    }
    const vel = Co.velFlota(m, naves);                                         // la flota va al paso de su escuadra más lenta
    let dur = 0, ok = false;
    for (const n of naves) { n.fuel = Math.max(0, n.fuel - d * S.reg.nave[n.cls].fuel); if (S.nav.viajar(m, n, dest, { st: 'guardia', sinFuel: true, sinCruce: true, vel })) { dur = n.ruta.t1 - m.t; ok = true; } }
    if (!ok) return false;
    f.st = 'viaje'; f.dest = dest; f.llega = m.prog(m.t + dur + 0.05, 'flota.llega', { f: f.id });
    return true;
  };
  S.en('flota.llega', function (m, e) {
    const f = m.flo[e.f]; if (!f.vivo || f.llega !== e) return;
    const dest = f.dest; f.sis = dest.t === 'A' ? m.ase[dest.id].sis : dest.id; f.st = dest.t === 'A' ? 'base' : 'guardia'; f.tLlega = m.t;
    Co.encuentro(m, f);
  });
  // Al llegar a un sistema: ¿hay alguien con quien estemos en guerra?
  Co.encuentro = function (m, f) {
    if (f.st === 'batalla' || f.st === 'viaje') return;
    const rivales = m.flo.filter(o => o.vivo && o !== f && o.sis === f.sis && o.st !== 'viaje' && o.st !== 'batalla' && o.nav.length && enGuerra(m, f.est, o.est));
    if (rivales.length) return Co.iniciar(m, f.sis, [f], rivales);
    const s = m.sis[f.sis];
    if (s.pir.length && f.st !== 'base') Co.cazar(m, f, s);
  };
  Co.cazar = function (m, f, s) {
    const fuerza = f.nav.length;
    for (const id of s.pir.slice()) {
      const n = m.nav[id]; if (!n.vivo) continue;
      if (n.patente >= 0 && !enGuerra(m, f.est, n.patente)) continue;
      if (n.rng.p(S.clamp(0.35 + 0.08 * fuerza, 0, 0.85))) {
        const ev = m.reg('pirata_cazado', 'La ' + f.nom + ' caza a la {N' + n.id + '} en {S' + s.id + '}', { s: s.id, imp: 1, c: [f.ordenEv, n.origenPirata], d: { n: n.id } });
        S.nav.destruir(m, n, { ev, por: f.alm });
        const a = m.ase[s.ase[0]]; if (a) S.inf.crear(m, ev, 'pirata_cazado', a.id, 0.5, 1, { sis: s.id });
      } else S.nav.dejarAcecho(m, n, 0.2);
    }
  };
  S.gancho('noticia.pirata_cazado', function (m, a, p) { if (a.rie) a.rie[p.d.sis] *= 0.3; });

  const cascosDe = (m, l) => { let c = 0; for (const id of l.nav) if (m.nav[id].vivo) c += m.nav[id].cascos; return c; };
  Co.iniciar = function (m, sis, fa, fb) {
    const s = m.sis[sis];
    // De cada bando se apunta también qué pinta aquí (quién llega, de quién es el sistema, a qué venía) y con cuántos cascos entra cada escuadra.
    const lado = (fs, llega) => { const nav = [], c0 = []; let c = 0; for (const f of fs) for (const id of f.nav) if (m.nav[id].vivo && m.nav[id].st !== 'viaje') { nav.push(id); c0.push(m.nav[id].cascos); c += m.nav[id].cascos; } const f0 = fs[0]; return { est: f0.est, flo: fs.map(f => f.id), nav, nav0: nav.slice(), c0, n0: c, n: c, esc0: nav.length, ef: 0, muertos: 0, alm: f0.alm, papel: { llega, casa: s.ase.some(id => m.ase[id].est === f0.est), mis: f0.mis ? f0.mis.tipo : '', st: f0.st } }; };
    const L = [lado(fa, true), lado(fb, false)];
    if (!L[0].n || !L[1].n) return;
    for (const l of L) l.perfil0 = S.tec.perfil(m, l.nav);
    L[0].ef = Co.eficacia(m, L[0], L[1].perfil0, sis); L[1].ef = Co.eficacia(m, L[1], L[0].perfil0, sis);
    L[0].des0 = Co.desglose(m, L[0], L[1].perfil0, sis); L[1].des0 = Co.desglose(m, L[1], L[0].perfil0, sis);
    const bt = { id: m.bat.length, sis, t0: m.t, ley: s.terr === 'asteroides' ? 'lineal' : 'cuadratica', L, vivo: true, k: (L[0].n0 + L[1].n0) / 2, hist: [[L[0].n, L[1].n]], gano: -1 };
    const w = m.est[L[0].est].gue.get(L[1].est) || m.est[L[1].est].gue.get(L[0].est);
    bt.ev = m.reg('batalla', 'Batalla en {S' + sis + '}: ' + L[0].n + ' naves de {E' + L[0].est + '} (' + L[0].esc0 + ' escuadras) contra ' + L[1].n + ' de {E' + L[1].est + '} (' + L[1].esc0 + ')' + (bt.ley === 'lineal' ? ', entre asteroides (no todos pueden disparar a todos)' : ''), { s: sis, imp: 2, c: [w ? w.ev : -1, fa[0].ordenEv], d: { bt: bt.id } });
    m.bat.push(bt);
    for (const f of fa.concat(fb)) { f.stPrev = f.st; f.st = 'batalla'; f.bat = bt.id; for (const id of f.nav) { const n = m.nav[id]; if (n.vivo && n.st !== 'viaje') { n.stPrev = n.st; n.enPrev = n.en; n.st = 'batalla'; n.x = s.x + n.rng.r(-16, 16); n.y = s.y + n.rng.r(-16, 16); n.en = -1; } } }
    S.emitir(m, 'batalla.inicio', bt);
    m.prog(m.t + 1, 'batalla.paso', { b: bt.id });
  };
  S.en('batalla.paso', function (m, e) {
    const bt = m.bat[e.b]; if (!bt.vivo) return;
    const [A, Bq] = bt.L;
    for (const l of bt.L) { l.nav = l.nav.filter(id => m.nav[id].vivo); l.n = Math.min(l.n, cascosDe(m, l)); }
    // Cada día se recalcula: lo que queda de cada bando ya no es lo que entró.
    const pa = S.tec.perfil(m, A.nav), pb = S.tec.perfil(m, Bq.nav);
    A.ef = Co.eficacia(m, A, pb, bt.sis) || A.ef; Bq.ef = Co.eficacia(m, Bq, pa, bt.sis) || Bq.ef;
    const [na, nb] = Co.integrar(A.n, Bq.n, A.ef, Bq.ef, bt.ley, 1, 8, bt.k);
    A.n = na; Bq.n = nb; bt.hist.push([na, nb]);
    const sis = m.sis[bt.sis];
    for (const l of bt.L) {
      // Las bajas se reparten entre las escuadras (caen antes las peor blindadas); la que se queda sin cascos desaparece.
      let perd = cascosDe(m, l) - Math.ceil(l.n - 1e-9);
      while (perd > 0 && l.nav.length) {
        const k = m.rng.pesos(l.nav.map(id => { const n = m.nav[id]; const d = m.dis[n.dis]; return n.cascos / (0.3 + (d ? d.g[3] + d.g[4] + d.g[5] : 0.36)); })); const n = m.nav[l.nav[k]];
        const q = Math.min(perd, n.cascos, Math.max(1, Math.ceil(n.cascos * 0.3))); n.cascos -= q; perd -= q;
        l.muertos += q * S.reg.nave[n.cls].trip; sis.restos = (sis.restos || 0) + q;
        if (n.cascos <= 0) {
          n.cascos = 0; l.nav.splice(k, 1); const alm = n.flo >= 0 ? m.flo[n.flo].alm : -1;
          S.nav.destruir(m, n, { ev: bt.ev, modo: 'combate' });
          if (alm >= 0 && m.per[alm].vivo && m.rng.p(0.12)) S.per.matar(m, m.per[alm], { modo: 'combate', c: [bt.ev] });
        }
      }
    }
    // Retirada: el almirante compara fuerzas con la ley cuadrática.
    let fin = A.n < 0.5 || Bq.n < 0.5; let huye = -1;
    if (!fin) for (let i = 0; i < 2; i++) {
      const yo = bt.L[i], el = bt.L[1 - i]; const f = m.flo[yo.flo[0]]; const alm = f && f.alm >= 0 ? m.per[f.alm] : null;
      const mia = yo.ef * yo.n * yo.n, suya = el.ef * el.n * el.n;
      if (mia < suya * 0.55 && (!alm || alm.r[R.VAL] < 0.8) && m.t - bt.t0 >= 2) { huye = i; fin = true; break; }
    }
    if (!fin) { m.prog(m.t + 1, 'batalla.paso', e); return; }
    Co.fin(m, bt, huye);
  });
  Co.fin = function (m, bt, huye) {
    bt.vivo = false; bt.t1 = m.t; bt.huye = huye;
    const [A, Bq] = bt.L; const g2 = huye >= 0 ? 1 - huye : (A.n >= Bq.n ? 0 : 1); const G = bt.L[g2], Pd = bt.L[1 - g2]; bt.gano = g2;
    for (const l of bt.L) { l.nFin = cascosDe(m, l); l.cFin = l.nav0.map(id => m.nav[id].vivo ? m.nav[id].cascos : 0); }
    const perd = (A.n0 - A.nFin) + (Bq.n0 - Bq.nFin); const muertos = A.muertos + Bq.muertos;
    const eg = m.est[G.est], ep = m.est[Pd.est];
    const ev = m.reg('batalla_fin', 'Batalla de {S' + bt.sis + '}: gana {E' + G.est + '} con ' + G.nFin + ' de ' + G.n0 + ' naves en pie; {E' + Pd.est + '} ' + (huye >= 0 ? 'se retira con ' + Pd.nFin + ' de ' + Pd.n0 : 'pierde sus ' + Pd.n0) + '. Quedan ' + perd + ' pecios a la deriva y unos ' + muertos.toLocaleString('es-ES') + ' muertos', { s: bt.sis, imp: perd >= 150 ? 3 : 2, c: [bt.ev], d: { perdidas: perd, muertos, gana: G.est, pierde: Pd.est, sis: bt.sis, bt: bt.id } });
    bt.fin = ev; eg.victorias++; ep.derrotas++;
    const w1 = eg.gue.get(Pd.est), w2 = ep.gue.get(G.est); if (w1) w1.score = (w1.score || 0) + 1; if (w2) { w2.score = (w2.score || 0) - 1; w2.cans = (w2.cans || 0) + 0.25; }
    const s = m.sis[bt.sis]; const a = m.ase[s.ase[0]];
    if (a) S.inf.crear(m, ev, 'batalla', a.id, 0.85, perd, { sis: bt.sis, lados: [G.est, Pd.est], pierde: Pd.est, gana: G.est });
    S.emitir(m, 'batalla.fin', bt, ev);
    for (const l of bt.L) for (const fid of l.flo) {
      const f = m.flo[fid]; f.nav = f.nav.filter(id => m.nav[id].vivo);
      for (const id of f.nav) { const n = m.nav[id]; if (n.st !== 'batalla') continue; n.dan = Math.min(0.8, n.dan + n.rng.r(0.03, 0.25)); n.cic.push({ t: m.t, ev: bt.ev, lado: n.rng.el(['babor', 'estribor', 'proa', 'popa']) }); n.cn.push(ev); n.st = n.enPrev >= 0 ? 'atracada' : 'guardia'; n.en = n.enPrev; if (n.en < 0) n.sisEn = bt.sis; }
      f.st = f.stPrev === 'base' ? 'base' : 'guardia'; f.bat = -1;
      if (l === Pd && f.nav.length && f.st !== 'base') { if (!Co.mover(m, f, { t: 'A', id: f.base })) f.st = 'guardia'; f.mis = null; }
      if (l === Pd && f.st === 'base' && f.nav.length) { // derrotada en su propia base: huye a otra si la hay
        const e = m.est[f.est]; const otra = m.ase.filter(x => x.est === e.id && x.sis !== bt.sis).sort((x, y) => m.dist[bt.sis][x.sis] - m.dist[bt.sis][y.sis])[0];
        if (otra) { f.base = otra.id; Co.mover(m, f, { t: 'A', id: otra.id }); }
      }
    }
  };

  // ── Combate en tierra: dura días. Ley lineal (calle a calle no todos pueden disparar a todos).
  Co.tierra = function (m, a, o) {
    m.tie = m.tie || [];
    const t = { id: m.tie.length, a: a.id, tipo: o.tipo, atk: Object.assign({ n0: o.atk.n }, o.atk), def: Object.assign({ n0: o.def.n }, o.def), t0: m.t, ev: o.ev, vivo: true, flo: o.flo === undefined ? -1 : o.flo, civiles: 0, hist: [[o.atk.n, o.def.n]], viejo: a.est };
    m.tie.push(t); a.tie = t.id;
    m.prog(m.t + 1, 'tierra.paso', { g: t.id });
    return t;
  };
  S.en('tierra.paso', function (m, e) {
    const t = m.tie[e.g]; if (!t.vivo) return; const a = m.ase[t.a];
    const K = (t.atk.n0 + t.def.n0) / 2; let A = t.atk.n, D = t.def.n;
    for (let i = 0; i < 4; i++) { const dA = t.def.ef * A * D / K * 0.25, dD = t.atk.ef * A * D / K * 0.25; A = Math.max(0, A - dA); D = Math.max(0, D - dD); }
    const civ = Math.round(a.pob * 0.0003 * (1 + (A + D) / Math.max(2000, a.pob * 0.05))); t.civiles += civ; a.pob = Math.max(200, a.pob - civ);
    t.atk.n = A; t.def.n = D; t.hist.push([A, D]);
    if (t.def.uni >= 0) m.uni[t.def.uni].n = Math.max(20, Math.round(D));
    if (t.flo >= 0 && (!m.flo[t.flo].vivo || !m.flo[t.flo].nav.length || m.flo[t.flo].st === 'batalla' && false)) return Co.finTierra(m, t, false);
    const dias = m.t - t.t0;
    if (D < t.def.n0 * 0.25 || D < A * 0.2) return Co.finTierra(m, t, true);
    if (A < t.atk.n0 * 0.3 || A < D * 0.25 || dias > 45) return Co.finTierra(m, t, false);
    m.prog(m.t + 1, 'tierra.paso', e);
  });
  Co.finTierra = function (m, t, ganaAtk) {
    t.vivo = false; t.t1 = m.t; t.gano = ganaAtk ? 0 : 1; const a = m.ase[t.a]; a.tie = -1;
    const caen = Math.round((t.atk.n0 - t.atk.n) + (t.def.n0 - t.def.n)); t.muertos = caen + t.civiles;
    const dias = Math.max(1, Math.round(m.t - t.t0));
    S.emitir(m, 'tierra.fin', t);
    if (t.tipo === 'asalto') {
      const f = m.flo[t.flo]; const e = m.est[t.atk.est];
      if (f && f.vivo && f.st === 'sitio') f.st = 'guardia';
      if (ganaAtk && f && f.vivo && e.vivo && a.est === t.viejo) Co.conquistar(m, f, a, false, t);
      else {
        m.reg('asalto_fallido', (f ? 'La ' + f.nom : 'El asalto') + ' no logra tomar {A' + a.id + '} tras ' + dias + ' días de combates: caen ' + caen.toLocaleString('es-ES') + ' soldados y ' + t.civiles.toLocaleString('es-ES') + ' vecinos', { a: a.id, imp: 1, c: [t.ev], d: { e: t.atk.est, muertos: t.muertos } });
        if (t.viejo >= 0) m.est[t.viejo].victorias += 0.5; e.derrotas += 0.5;
        if (f && f.vivo) { for (const id of f.nav) m.nav[id].dan = Math.min(0.8, m.nav[id].dan + 0.1); f.fallos = (f.fallos || 0) + 1; if (f.fallos >= 2) { f.fallos = 0; f.mis = null; } }
        if (t.civiles > 20) S.soc.muertes(m, a, m.coh[a.cohGen], Math.min(t.civiles, 60), t.ev, { clave: 'ext', hecho: 'invasion' });
      }
    } else {
      const f = m.fac[t.atk.fac];
      if (ganaAtk) {
        if (f) { S.tec.aprende(m, f, 0.15); f.exitos += 3; f.armas = Math.max(0, f.armas * 0.7) + 15; }
        const ev = m.reg('revolucion', 'La insurrección vence en {A' + a.id + '} tras ' + dias + ' días: lo que queda de la guarnición (' + Math.round(t.def.n).toLocaleString('es-ES') + ') se rinde a ' + (f ? f.nom : 'la calle') + '. ' + t.muertos.toLocaleString('es-ES') + ' muertos', { a: a.id, imp: 3, c: [t.ev], d: { f: a.f, dif: 1, muertos: t.muertos } });
        a.revuelta = ev; S.emitir(m, 'revolucion', a, ev, 1);
      } else {
        if (f) { S.tec.aprende(m, f, 0.08); f.comp = Math.max(0, f.comp - 0.2); f.armas *= 0.3; }
        const ev = m.reg('insurreccion_aplastada', 'La guarnición aplasta la insurrección de {A' + a.id + '} en ' + dias + ' días: ' + t.muertos.toLocaleString('es-ES') + ' muertos. Los que sobreviven han aprendido a pelear', { a: a.id, imp: 2, c: [t.ev], d: { muertos: t.muertos } });
        a.terror = 0.35; a.masacre = (a.masacre || 0) + 0.12; a.nMasacres = (a.nMasacres || 0) + 1; a.f *= 0.3; a.culpa.reg += 4;
        S.soc.muertes(m, a, m.coh[a.cohGen], Math.min(m.coh[a.cohGen].n - 1, Math.round(t.civiles * 0.2) + 20), ev, { clave: 'reg', hecho: 'masacre', culpable: a.gob >= 0 ? a.gob : (a.est >= 0 ? m.est[a.est].gob : -1) });
        S.inf.crear(m, ev, 'masacre', a.id, 0.8, t.muertos, { e: a.est });
      }
    }
  };

  // ── Asalto a un asentamiento: primero la cuenta de cada soldado; si no se rinden, se pelea en tierra.
  Co.tropas = function (m, f) { let k = 0; for (const id of f.nav) { const n = m.nav[id]; if (n.vivo) k += (S.reg.nave[n.cls].tropas || 5) * n.cascos * (1 - n.dan); } return Math.round(k); };
  Co.asalto = function (m, f, a) {
    if (a.tie >= 0) return true;
    const e = m.est[f.est]; const viejo = a.est; const u = a.uni >= 0 ? m.uni[a.uni] : null;
    const tropas = Co.tropas(m, f); const def = u ? u.n * 1.4 : 0;
    let rinde = !u || u.n < 50;
    if (u && viejo >= 0) { const s = S.soc.tironRegimen(m, u, a); s[5] = S.clamp(def / (def + tropas), 0.05, 0.95); const dif = 1 - 2 * S.soc.L(S.soc.pesos(m, u), s); if (dif > S.soc.UMBRALES.desertar) rinde = true; }
    if (rinde) { Co.conquistar(m, f, a, true, null); return true; }
    const causa = e.gue.get(viejo) ? e.gue.get(viejo).ev : f.ordenEv;
    const ev = m.reg('desembarco', 'La ' + f.nom + ' desembarca ' + tropas.toLocaleString('es-ES') + ' soldados en {A' + a.id + '}: ' + u.n.toLocaleString('es-ES') + ' defensores les esperan en las calles', { a: a.id, imp: 2, c: [causa], d: { e: e.id } });
    f.st = 'sitio';
    S.inf.crear(m, ev, 'desembarco', a.id, 0.85, tropas, { sis: a.sis, e: e.id, viejo });
    const mo = Co.moralDet(m, { est: e.id, flo: [f.id] }, a.sis);
    Co.tierra(m, a, { tipo: 'asalto', ev, flo: f.id, atk: { n: tropas, ef: 0.10 * (0.5 + (e.doc ? e.doc.tierra : 0.3)) * 1.3 * mo.total, est: e.id, nom: f.nom, des: { doc: e.doc ? e.doc.tierra : 0.3, moral: mo } }, def: { n: u.n, ef: S.tec.efTierra(m, u, a) * 1.4, est: viejo, uni: u.id, nom: 'la guarnición de ' + a.nom, des: { doc: u.doc, arm: u.arm, msc: u.msc || 0, casa: 1.4 } } });
    return true;
  };
  Co.conquistar = function (m, f, a, rinde, t) {
    const e = m.est[f.est]; const viejo = a.est; const u = a.uni >= 0 ? m.uni[a.uni] : null;
    const causa = t ? t.ev : (e.gue.get(viejo) ? e.gue.get(viejo).ev : f.ordenEv);
    const ev = m.reg('conquista', 'La ' + f.nom + ' toma {A' + a.id + '} para {E' + e.id + '}' + (rinde ? ': la guarnición (' + (u ? u.n.toLocaleString('es-ES') : 0) + ' soldados, ' + (u ? u.msc : 0) + ' meses sin cobrar) se rinde sin disparar' : ' tras ' + Math.max(1, Math.round(m.t - t.t0)) + ' días de combate casa por casa: ' + t.muertos.toLocaleString('es-ES') + ' muertos, ' + t.civiles.toLocaleString('es-ES') + ' de ellos vecinos'), { a: a.id, imp: 2, c: [causa], d: { e: e.id, viejo, muertos: t ? t.muertos : 0 } });
    if (t && t.civiles > 0) S.soc.muertes(m, a, m.coh[a.cohGen], Math.min(m.coh[a.cohGen].n - 1, Math.max(5, Math.round(t.civiles * 0.1))), ev, { clave: 'ext', hecho: 'invasion' });
    const civil = viejo >= 0 && e.gue.get(viejo) && e.gue.get(viejo).civil; const eraCap = viejo >= 0 && m.est[viejo].cap === a.id;
    S.inf.crear(m, ev, 'conquista', a.id, 0.8, 1, { e: e.id, viejo, sis: a.sis });
    S.pol.cambiarDueno(m, a, e.id, ev, { pacifico: rinde });
    if (e.doc && !rinde) e.doc.tierra = Math.min(1, e.doc.tierra + 0.04);
    const w = e.gue.get(viejo); if (w) w.score = (w.score || 0) + 1; e.victorias++;
    if (viejo >= 0) { const o = m.est[viejo]; o.derrotas++; const w2 = o.gue.get(e.id); if (w2) w2.cans = (w2.cans || 0) + 0.3; }
    // Guerra civil: quien pierde su capital pierde la guerra; el resto vuelve al redil.
    if (civil && eraCap) {
      const o = m.est[viejo]; const resto = S.pol.territorio(m, o);
      const fin = m.reg('fin_guerra_civil', 'Cae la capital de {E' + viejo + '}: {E' + e.id + '} gana la guerra civil', { a: a.id, imp: 3, c: [ev, causa], d: { e: e.id, j: viejo } });
      for (const x of resto) S.pol.cambiarDueno(m, x, e.id, fin, { pacifico: true });
      const gob = o.gob >= 0 ? m.per[o.gob] : null; if (gob && gob.vivo) S.per.matar(m, gob, { modo: 'ejecucion', por: e.gob, c: [fin] });
      if (o.vivo) S.pol.caer(m, o, fin);
      e.fiestas.push({ nom: 'Día de la Victoria', dia: Math.floor(m.t % S.ANIO) });
    }
    return ev;
  };

  S.en('flota.tic', function (m, x) {
    const f = m.flo[x.f]; if (!f.vivo) return;
    const e = m.est[f.est]; if (!e.vivo) { f.vivo = false; return; }
    m.prog(x.t + 3, 'flota.tic', x);
    f.nav = f.nav.filter(id => m.nav[id].vivo);
    if (f.st === 'viaje' || f.st === 'batalla') return;
    if (!f.nav.length) { f.st = 'base'; f.sis = m.ase[f.base].sis; return; }
    // En la base, las escuadras diezmadas de la misma clase se funden en una.
    if (f.st === 'base') for (const id of f.nav) { const n = m.nav[id]; const nom = S.reg.nave[n.cls].cascos; if (!n.vivo || n.cascos >= nom * 0.5) continue; const o = f.nav.map(i => m.nav[i]).find(y => y !== n && y.vivo && y.cls === n.cls && y.cascos < nom * 0.6); if (o) { n.vet = (n.vet * n.cascos + (o.vet || 0) * o.cascos) / (n.cascos + o.cascos); n.cascos += o.cascos; o.cascos = 0; o.vivo = false; o.st = 'fundida'; if (o.nuc >= 0 && m.obj[o.nuc].falla) m.obj[o.nuc].falla.x = true; f.nav = f.nav.filter(i => m.nav[i].vivo); break; } }
    if (m.ase[f.base].est !== e.id) f.base = e.cap;
    Co.encuentro(m, f); if (f.st === 'batalla') return;
    if (f.st === 'sitio') { const a = m.sis[f.sis].ase.map(i => m.ase[i]).find(y => y.tie >= 0 && m.tie[y.tie].flo === f.id); if (a) return; f.st = 'guardia'; }
    if (f.st === 'base') {
      const b = m.ase[f.base];
      // En la base se repara el casco y se reajustan los componentes tocados, si hay con qué.
      for (const id of f.nav) { const n = m.nav[id]; if (b.alm[B.piezas] > 2 && b.alm[B.metal] > 3) { if (n.dan > 0) { n.dan = Math.max(0, n.dan - 0.03); b.alm[B.piezas] -= 0.4; b.alm[B.metal] -= 0.8; } if (n.q) for (let k = 0; k < 8; k++) if (n.q[k] < 0.9) { n.q[k] += 0.01; b.alm[B.piezas] -= 0.05; } } }
      const o = f.orden; if (!o || o.tipo === 'volver') { f.orden = null; return; }
      if (o.tipo === 'atacar' && !enGuerra(m, f.est, m.ase[o.ase].est)) { f.orden = null; return; }
      if (f.nav.some(id => m.nav[id].dan > 0.5) && f.nav.length < 3) return;
      if (Co.mover(m, f, { t: 'S', id: o.sis })) { f.mis = o; f.orden = null; f.tMis = m.t; }
      return;
    }
    // De guardia en un sistema.
    const mis = f.mis;
    if (mis && mis.tipo === 'atacar') {
      const obj = m.sis[f.sis].ase.map(id => m.ase[id]).filter(a => enGuerra(m, f.est, a.est)).sort((p, q) => (p.id === mis.ase ? -1 : 0) - (q.id === mis.ase ? -1 : 0) || q.pob - p.pob);
      if (obj.length && m.t - f.tLlega >= 3) { Co.asalto(m, f, obj[0]); return; }
      if (obj.length) return;
      f.mis = null;
    }
    if (mis && (mis.tipo === 'patrulla' || mis.tipo === 'defender') && m.t - f.tLlega < 24) return;
    f.mis = null;
    if (!Co.mover(m, f, { t: 'A', id: f.base })) { /* sin combustible: se queda */ }
  });

  // Una nave pasa de un Estado a otro con el asentamiento donde estaba atracada su flota.
  S.gancho('asent.dueno', function (m, a, viejo) {
    for (const f of m.flo) { if (!f.vivo || f.base !== a.id || f.est !== viejo) continue; const e = m.est[viejo]; if (e && e.vivo) { const otra = m.ase.filter(y => y.est === viejo)[0]; if (otra) f.base = otra.id; } }
  });

  S.sismografo('fragatas', 'Naves de guerra (cascos)', m => { let k = 0; for (const n of m.nav) if (n.vivo && n.flo >= 0) k += n.cascos; return k; });
  S.sismografo('pecios', 'Pecios a la deriva', m => { let k = 0; for (const s of m.sis) k += s.pecios.length + (s.restos || 0); return k; });
})(typeof globalThis !== 'undefined' ? globalThis : this);
