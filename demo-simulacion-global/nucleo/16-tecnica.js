// Técnica y aprendizaje. Cada nave de guerra es un diseño (un reparto de su tonelaje entre ocho componentes)
// fabricado con una calidad distinta en cada componente y tripulado por gente más o menos veterana.
// Los diseños evolucionan: cada año la oficina de diseño de cada Estado conserva los que mejor han
// rendido (en batallas reales y en maniobras contra lo que CREE que tiene el rival), los cruza y los muta.
// Es un algoritmo genético: la aptitud es la relación de bajas. Ejércitos y rebeldes aprenden doctrina peleando.
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const R = S.R;
  const Te = S.tec = {};
  // Armas: cinética, láser, misiles. Cada una tiene su defensa: blindaje, escudos, defensa de punto. Más motores y sensores.
  Te.GEN = ['cin', 'las', 'mis', 'bli', 'esc', 'pd', 'mot', 'sen'];
  Te.NOM = ['Cañones', 'Láseres', 'Misiles', 'Blindaje', 'Escudos', 'Defensa de punto', 'Motores', 'Sensores'];
  Te.COL = ['#e0b84c', '#e0564f', '#f0f0f0', '#8d99ad', '#5fb0c9', '#b98fd6', '#e2703a', '#5cc28a'];
  Te.G0 = [0.14, 0.14, 0.14, 0.12, 0.12, 0.12, 0.11, 0.11];
  Te.normal = function (g) { let s = 0; for (let i = 0; i < 8; i++) { if (g[i] < 0.02) g[i] = 0.02; s += g[i]; } for (let i = 0; i < 8; i++) g[i] /= s; return g; };
  // Daño que un diseño X le hace a un diseño Y: cada arma se estrella contra su defensa; los sensores afinan, los motores esquivan.
  Te.bruto = function (x, y) {
    let s = 0; for (let k = 0; k < 3; k++) s += x[k] * (1 - 0.9 * y[3 + k] / (y[3 + k] + 0.06));   // sin defensa, todo entra; con ella, hasta un 90 % menos
    return (1 + 3 * x[7]) * s / (1 + 2 * y[6]);
  };
  Te.R0 = Te.bruto(Te.G0, Te.G0);
  // Relación de bajas esperada de X contra Y (la aptitud en maniobras).
  Te.cambio = (x, y) => Te.bruto(x, y) / Te.bruto(y, x);
  Te.romano = function (n) { const r = [['M', 1000], ['CM', 900], ['D', 500], ['CD', 400], ['C', 100], ['XC', 90], ['L', 50], ['XL', 40], ['X', 10], ['IX', 9], ['V', 5], ['IV', 4], ['I', 1]]; let s = ''; for (const [l, v] of r) while (n >= v) { s += l; n -= v; } return s; };
  const CLASES = ['Tormenta', 'Vigía', 'Martillo', 'Azor', 'Coloso', 'Lanza', 'Égida', 'Centella', 'Bastión', 'Halcón', 'Trueno', 'Navaja', 'Escudo', 'Cometa', 'Yunque', 'Sable', 'Faro', 'Garra', 'Muralla', 'Rayo'];

  Te.nuevoDiseno = function (m, est, cls, g2, gen, base, padres) {
    m.dis = m.dis || [];
    const d = { id: m.dis.length, est, cls, g: Te.normal(g2.slice()), gen, base, nom: 'clase ' + base + ' Mk ' + Te.romano(gen), sim: 1, real: 1, nReal: 0, fit: 1, padres: padres || [], bajasC: 0, bajasS: 0, batallas: 0, t0: m.t, construidas: 0 };
    m.dis.push(d); return d;
  };
  // Las líneas de diseño de un Estado (una población de variantes por clase).
  Te.lineas = function (m, e) {
    if (e.dis) return e.dis;
    e.dis = {}; e.intel = new Map(); e.doc = { naval: e.rng.r(0.15, 0.45), tierra: e.rng.r(0.2, 0.5) };
    for (const cls of ['fragata', 'crucero', 'acorazado', 'corbeta']) {
      const base = CLASES[(e.id * 7 + cls.length * 3 + e.rng.i(CLASES.length)) % CLASES.length]; const pob = [];
      for (let i = 0; i < 6; i++) pob.push(Te.nuevoDiseno(m, e.id, cls, Te.G0.map(x => x + e.rng.n(0, 0.03)), 1, base));
      e.dis[cls] = { cls, base, pob, mejor: pob[0], gen: 1, hist: [] };
    }
    return e.dis;
  };
  Te.libre = function (m, cls) { // diseños sin Estado: piratas, corsarios que perdieron su patente
    m.disLibre = m.disLibre || {};
    if (!m.disLibre[cls]) { const g2 = Te.G0.slice(); g2[6] += 0.08; g2[7] += 0.04; m.disLibre[cls] = Te.nuevoDiseno(m, -1, cls, g2, 1, 'Hueso'); }
    return m.disLibre[cls];
  };
  // Cada casco sale de fábrica con una calidad distinta en cada componente (q): manda la experiencia del astillero
  // (curva de aprendizaje) y lo que faltaba en el almacén mientras se construía.
  Te.calidad = function (rng, exp, falta) {
    const q = []; const base = 0.82 + 0.22 * (1 - Math.exp(-(exp || 0) / 300));
    for (let k = 0; k < 8; k++) q.push(S.clamp(base + rng.n(0, 0.05), 0.5, 1.2));
    if (falta) { for (const k of [0, 1, 2]) q[k] *= 0.6 + 0.4 * falta.armas; q[3] *= 0.6 + 0.4 * falta.metal; for (const k of [4, 5, 6, 7]) q[k] *= 0.6 + 0.4 * falta.piezas; }
    return q;
  };
  S.gancho('nave.creada', function (m, n) {
    const def = S.reg.nave[n.cls]; if (!def.guerra) return;
    const e = n.est >= 0 && n.dueno.t === 'E' ? m.est[n.est] : null;
    const d = e ? Te.lineas(m, e)[n.cls].mejor : Te.libre(m, n.cls);
    n.dis = d.id; d.construidas += n.cascos; n.vet = n.rng.r(0.05, 0.3);
    if (!n.q) n.q = Te.calidad(n.rng, 200, null);
  });
  // Genoma efectivo de una escuadra: diseño × calidad de cada componente × desgaste.
  Te.efectivo = function (m, n) { const d = m.dis[n.dis]; const out = new Array(8); const w = 1 - 0.5 * n.dan; for (let k = 0; k < 8; k++) out[k] = d.g[k] * n.q[k] * w; return out; };
  Te.nivel = (m, n) => n.est >= 0 && m.est[n.est] ? m.est[n.est].tec : 0.85;
  // Perfil medio de un bando, ponderado por cascos: contra eso dispara el otro.
  Te.perfil = function (m, navs) {
    const p = { g: new Array(8).fill(0), niv: 0, n: 0 };
    for (const id of navs) { const n = m.nav[id]; if (!n.vivo || n.dis === undefined) continue; const ge = Te.efectivo(m, n); for (let k = 0; k < 8; k++) p.g[k] += ge[k] * n.cascos; p.niv += Te.nivel(m, n) * n.cascos; p.n += n.cascos; }
    if (p.n > 0) { for (let k = 0; k < 8; k++) p.g[k] /= p.n; p.niv /= p.n; } else { p.g = Te.G0.slice(); p.niv = 1; }
    return p;
  };
  // Bajas que causa al día cada casco de la escuadra n contra el perfil enemigo.
  Te.tasa = function (m, n, enemigo) {
    const def = S.reg.nave[n.cls]; const e = n.est >= 0 ? m.est[n.est] : null;
    const doc = e && e.doc ? e.doc.naval : 0.2;
    return def.ef * Te.bruto(Te.efectivo(m, n), enemigo.g) / Te.R0 * Math.pow(Te.nivel(m, n) / enemigo.niv, 0.7) * (1 + 0.5 * (n.vet || 0)) * (0.8 + 0.5 * doc);
  };

  // ── La oficina de diseño: selección, cruce y mutación una vez al año.
  Te.rivales = function (m, e) {
    const out = []; for (const j of S.pol.vecinos(m, e)) { const r = S.pol.rel(m, e, j); const it = e.intel.get(j); out.push({ j, w: 0.3 + r.cb + (e.gue.has(j) ? 2 : 0) - Math.min(0, r.op), g: it ? it.g : Te.G0, t: it ? it.t : -1 }); }
    for (const [j] of e.gue) if (!out.some(x => x.j === j) && m.est[j].vivo) { const it = e.intel.get(j); out.push({ j, w: 2, g: it ? it.g : Te.G0, t: it ? it.t : -1 }); }
    if (!out.length) out.push({ j: -1, w: 1, g: Te.G0, t: -1 });
    return out;
  };
  Te.evaluar = function (g2, riv) { let s = 0, w = 0; for (const r of riv) { s += r.w * Te.cambio(g2, r.g); w += r.w; } return s / w; };
  Te.oficina = function (m, e) {
    const L = Te.lineas(m, e); const riv = Te.rivales(m, e);
    for (const cls in L) {
      const li = L[cls]; const antes = li.mejor;
      for (const d of li.pob) { d.sim = Te.evaluar(d.g, riv); d.fit = d.nReal > 0 ? 0.4 * d.sim + 0.6 * d.real : d.sim; }
      li.pob.sort((x, y) => y.fit - x.fit || x.id - y.id);
      li.gen++; const vivos = li.pob.slice(0, 3); const hijos = [];
      for (let i = 0; i < 3; i++) {
        const p1 = vivos[e.rng.pesos(vivos.map(v => v.fit))], p2 = vivos[e.rng.i(vivos.length)];
        const g2 = []; for (let k = 0; k < 8; k++) g2.push((e.rng.p(0.5) ? p1.g[k] : p2.g[k]) + e.rng.n(0, 0.018));
        const h = Te.nuevoDiseno(m, e.id, cls, g2, li.gen, li.base, [p1.id, p2.id]); h.sim = Te.evaluar(h.g, riv); h.fit = h.sim; hijos.push(h);
      }
      li.pob = vivos.concat(hijos).sort((x, y) => y.fit - x.fit || x.id - y.id); li.mejor = li.pob[0];
      li.hist.push({ t: m.t, gen: li.mejor.gen, fit: li.mejor.fit, g: li.mejor.g.slice() }); if (li.hist.length > 80) li.hist.shift();
      if (li.mejor !== antes && cls !== 'corbeta') {
        let kUp = 0, kDn = 0; for (let k = 1; k < 8; k++) { if (li.mejor.g[k] - antes.g[k] > li.mejor.g[kUp] - antes.g[kUp]) kUp = k; if (li.mejor.g[k] - antes.g[k] < li.mejor.g[kDn] - antes.g[kDn]) kDn = k; }
        const rv = riv.slice().sort((x, y) => y.w - x.w)[0];
        li.ev = m.reg('diseno', '{E' + e.id + '} adopta para sus ' + S.reg.nave[cls].nom.toLowerCase().replace('escuadra de ', '').replace('división de ', '') + ' la ' + li.mejor.nom + ': más ' + Te.NOM[kUp].toLowerCase() + ', menos ' + Te.NOM[kDn].toLowerCase() + (rv.j >= 0 ? ' (pensando en {E' + rv.j + '}' + (rv.t >= 0 ? ', con lo que supo de sus naves hace ' + Math.round((m.t - rv.t) / 30) + ' meses)' : ', de cuyas naves no sabe nada)') : ''), { a: e.cap, imp: 0, c: [antes.ev, e.evGuerra], d: { e: e.id, dis: li.mejor.id } });
        li.mejor.ev = li.ev;
      }
    }
    // Nivel técnico: investigación (si hay tesoro), experiencia de guerra y lo que se filtra de aliados y socios.
    let sube = 0.004 + (e.tes > 100000 ? 0.008 : 0) + Math.min(0.01, (e.batallasAnio || 0) * 0.004); e.batallasAnio = 0;
    if (e.alianzas) for (const j of e.alianzas) if (m.est[j].vivo && m.est[j].tec > e.tec) sube += 0.2 * (m.est[j].tec - e.tec);
    e.tec *= 1 + sube; e.doc.naval *= 0.985; e.doc.tierra *= 0.985;
    // Modernización: una escuadra vieja al año pasa por el astillero.
    if (e.tes > 60000) for (const fid of e.flo) { const f = m.flo[fid]; if (!f.vivo || f.st !== 'base') continue; const n = f.nav.map(i => m.nav[i]).find(x => x.vivo && m.dis[x.dis].est === e.id && L[x.cls] && L[x.cls].mejor.gen - m.dis[x.dis].gen >= 3); if (n) { e.tes -= 20000; n.dis = L[n.cls].mejor.id; n.cn.push(m.reg('modernizacion', 'La {N' + n.id + '} sale del astillero convertida a la ' + L[n.cls].mejor.nom, { a: f.base, imp: 0, c: [L[n.cls].ev] })); break; } }
  };
  S.gancho('anio', function (m) { for (const e of m.est) if (e.vivo) Te.oficina(m, e); for (const f of m.fac) if (f.vivo && f.doc) f.doc *= 0.97; });

  // ── Lo que deja una batalla: aptitud real de cada diseño, veteranía, doctrina y lo que cada bando ha visto del otro.
  S.gancho('batalla.fin', function (m, bt) {
    const [A, Bq] = bt.L;
    for (let i = 0; i < 2; i++) {
      const yo = bt.L[i], el = bt.L[1 - i]; const e = m.est[yo.est]; if (!e || !e.dis) continue;
      const causadas = el.n0 - el.nFin, sufridas = yo.n0 - yo.nFin; const cambio = S.clamp((causadas + 2) / (sufridas + 2), 0.1, 10);
      const vistos = new Set();
      for (const id of yo.nav0) { const n = m.nav[id]; const d = m.dis[n.dis]; if (!d || vistos.has(d.id)) continue; vistos.add(d.id); d.real = d.nReal ? 0.5 * d.real + 0.5 * cambio : cambio; d.nReal++; d.batallas++; d.bajasC += Math.round(causadas / Math.max(1, yo.nav0.length)); d.bajasS += Math.round(sufridas / Math.max(1, yo.nav0.length)); }
      for (const id of yo.nav) { const n = m.nav[id]; if (!n.vivo) continue; n.vet = Math.min(1, (n.vet || 0) + 0.1 + 0.25 * sufridas / Math.max(1, yo.n0)); const k = n.rng.i(8); n.q[k] = Math.max(0.4, n.q[k] - n.rng.r(0.03, 0.18)); }
      // Se aprende de las victorias y, más, de las derrotas.
      const tam = Math.log(1 + (A.n0 + Bq.n0) / 100); e.doc.naval = Math.min(1, e.doc.naval + 0.03 * tam * (yo === bt.L[bt.gano] ? 1 : 1.5)); e.batallasAnio = (e.batallasAnio || 0) + 1;
      if (el.est >= 0 && el.perfil0) e.intel.set(el.est, { g: el.perfil0.g.slice(), niv: el.perfil0.niv, t: m.t });
    }
  });
  // Una caja negra vendida en tu puerto te cuenta cómo estaba hecha la nave: ingeniería inversa.
  S.gancho('caja.vendida', function (m, o, a) {
    if (a.est < 0) return; const e = m.est[a.est]; const w = m.nav[o.nave]; if (!w || w.dis === undefined || !e.dis) return; const d = m.dis[w.dis];
    if (d.est < 0 || d.est === e.id) return;
    e.intel.set(d.est, { g: d.g.slice(), niv: m.est[d.est].tec, t: m.t });
    if (m.est[d.est].tec > e.tec) e.tec += 0.05 * (m.est[d.est].tec - e.tec);
    m.reg('ingenieria_inversa', 'Los ingenieros de {E' + e.id + '} desmontan en {A' + a.id + '} los restos de una nave de la ' + d.nom + ' de {E' + d.est + '}: ya saben contra qué diseñar', { a: a.id, imp: 0, d: { e: e.id } });
  });

  // ── Doctrina en tierra: guarniciones y rebeldes.
  Te.efTierra = function (m, u, a) {
    const e = u.est >= 0 ? m.est[u.est] : null;
    if (u.doc === undefined) { u.doc = e && e.doc ? e.doc.tierra : 0.3; u.arm = 0.8; }
    return 0.10 * (0.5 + u.doc) * (0.5 + u.arm) * S.clamp(1 - u.msc / 6, 0.4, 1) * (e ? 0.7 + 0.5 * (e.asabiya === undefined ? 0.6 : e.asabiya) : 0.9);
  };
  // Las guarniciones se equipan con las armas que haya en el almacén; sin armería cerca, van peor armadas.
  S.gancho('asent.mes', function (m, a) {
    if (a.uni < 0) return; const u = m.uni[a.uni]; if (u.arm === undefined) { u.arm = 0.8; u.doc = 0.3; }
    const need = u.n * 0.0004; a.demHoy[B.armas] += need / 30;
    if (a.alm[B.armas] >= need) { a.alm[B.armas] -= need; u.arm = Math.min(1, u.arm + 0.04); } else u.arm = Math.max(0.35, u.arm - 0.03);
    const e = u.est >= 0 ? m.est[u.est] : null; if (e && e.doc) u.doc += (e.doc.tierra - u.doc) * 0.05;
  });
  Te.milicia = function (m, f, a) {
    if (f.doc === undefined) { f.doc = 0.1; f.armas = 0; }
    let n = 0; for (const c of f.coh) if (m.coh[c].ase === a.id) n += m.coh[c].n;
    return Math.round(Math.min(n * f.comp * 0.06, 200 + f.armas * 60));
  };
  // Comprar armas: acción de facción (registro).
  S.def('accionFac', 'armarse', {
    tipos: ['rebelde', 'resistencia', 'orden', 'sindicato'],
    U(m, f, x) { const a = m.ase[f.sede]; if (f.doc === undefined) { f.doc = 0.1; f.armas = 0; } return f.O >= 1.1 && f.caja > 2500 && a.alm[B.armas] > 2 && f.armas < 60 ? 0.12 + 0.3 * x.agr : -9; },
    hacer(m, f) { const a = m.ase[f.sede]; const q = Math.min(a.alm[B.armas] * 0.3, f.caja * 0.6 / a.pr[B.armas], 20); const r = S.eco.comprar(m, a, B.armas, q); f.caja -= r.coste; f.armas += r.q; if (!f.evArmas) f.evArmas = m.reg('armas', f.nom + ' compra armas en {A' + a.id + '}: ya no son solo reuniones', { a: a.id, imp: 1, c: [f.evNace], d: { f: f.id } }); },
  });
  // Insurrección armada: si hay milicia, la calle no espera a que disparen.
  Te.insurreccion = function (m, a, causas) {
    const u = a.uni >= 0 ? m.uni[a.uni] : null; if (!u || a.tie >= 0) return false;
    const f = m.fac.find(x => x.vivo && x.etapa === 'inst' && x.tipo !== 'casa' && (x.enem.k === 'reg' || x.enem.k === 'ext') && (x.sede === a.id || x.cel.some(c => c.ase === a.id)));
    if (!f) return false; const mil = Te.milicia(m, f, a); if (mil < u.n * 0.3 || mil < 120) return false;
    const gente = Math.round(a.pob * a.f * 0.02);
    const ev = m.reg('insurreccion', 'Insurrección en {A' + a.id + '}: ' + mil.toLocaleString('es-ES') + ' milicianos de ' + f.nom + ' y ' + gente.toLocaleString('es-ES') + ' vecinos se echan a la calle contra ' + u.n.toLocaleString('es-ES') + ' soldados', { c: causas.concat([f.evNace, f.evArmas]), a: a.id, imp: 2, d: { f: f.id } });
    a.revuelta = ev;
    S.com.tierra(m, a, { tipo: 'insurreccion', ev, atk: { n: mil + gente * 0.3, ef: 0.10 * (0.4 + f.doc) * (0.45 + Math.min(1, f.armas * 60 / Math.max(1, mil)) * 0.6), fac: f.id, est: -1, nom: f.nom, des: { doc: f.doc, armados: Math.min(1, f.armas * 60 / Math.max(1, mil)), mil, vecinos: gente * 0.3 } }, def: { n: u.n, ef: Te.efTierra(m, u, a) * 1.3, est: a.est, uni: u.id, nom: 'la guarnición', des: { doc: u.doc, arm: u.arm, msc: u.msc || 0, casa: 1.3 } } });
    return true;
  };
  // Lo que se aprende peleando se queda, y viaja con las noticias: otras facciones aprenden de la experiencia ajena.
  Te.aprende = function (m, f, k) { if (f.doc === undefined) { f.doc = 0.1; f.armas = 0; } f.doc = Math.min(1, f.doc + k); };
  for (const k of ['revolucion', 'insurreccion', 'masacre']) S.gancho('noticia.' + k, function (m, a, p) {
    for (const f of m.fac) { if (!f.vivo || f.tipo === 'casa' || f.etapa !== 'inst') continue; if (f.sede !== a.id && !f.cel.some(c => c.ase === a.id)) continue; Te.aprende(m, f, k === 'masacre' ? 0.01 : 0.035); f.aprendido = (f.aprendido || 0) + 1; }
  });
  // Facciones con el mismo enemigo que comparten casa o sistema se pasan lo que saben.
  S.gancho('asent.mes', function (m, a) {
    const fs = m.fac.filter(f => f.vivo && f.etapa === 'inst' && f.tipo !== 'casa' && f.doc !== undefined && (f.sede === a.id || f.cel.some(c => c.ase === a.id)));
    if (fs.length < 2) return; let mx = 0; for (const f of fs) if (f.doc > mx) mx = f.doc;
    for (const f of fs) f.doc += 0.15 * (mx - f.doc);
  });

  S.sismografo('tec', 'Nivel técnico medio', m => { let s = 0, k = 0; for (const e of m.est) if (e.vivo) { s += e.tec; k++; } return k ? s / k : 1; });
  S.sismografo('docreb', 'Doctrina rebelde (máxima)', m => { let s = 0; for (const f of m.fac) if (f.vivo && f.doc > s) s = f.doc; return s; });
  S.gancho('sismo', function (m) { for (const e of m.est) if (e.vivo && e.dis) { m.serie('X' + e.id, e.tec); m.serie('D' + e.id, e.doc.naval); m.serie('A' + e.id, e.dis.fragata.mejor.fit); } });
})(typeof globalThis !== 'undefined' ? globalThis : this);
