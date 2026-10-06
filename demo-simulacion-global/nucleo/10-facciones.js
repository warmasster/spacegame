// Facciones: nadie las escribe. Nacen de una comunidad con un agravio común y un conector, acumulan
// organización, se bautizan con su hecho fundador, se parten por el vector de Fiedler y se disuelven.
(function (g) {
  'use strict';
  const S = g.SIM; const B = S.B; const R = S.R;
  const F = S.fac = {};
  const CLAVES = ['reg', 'pat', 'acap', 'ext', 'crim'];
  const HECHO_DE = { pat: ['explosion'], reg: ['masacre', 'hambre', 'bomba'], acap: ['hambre'], ext: ['bomba', 'invasion'], crim: ['explosion', 'pirata'] };

  F.nueva = function (m, o) {
    const id = m.fac.length;
    const f = Object.assign({ id, nom: '', sim: '', tipo: 'proto', etapa: 'proto', lid: -1, coh: [], cel: [], O: 0, caja: 0, ideo: [0, 0, 0, 0, 0], normas: [], fund: -1, sede: -1, enem: { k: 'reg', id: -1 }, l2: 1, comp: 0.6, vivo: true, mesesNeg: 0, negativas: 0, nac: m.t, rng: m.rngDe('F', id), exitos: 0 }, o || {});
    m.fac.push(f);
    if (f.lid >= 0) { const p = m.per[f.lid]; p.fac = id; if (f.tipo !== 'proto') p.cargo = { t: 'lider', id, nom: 'cabeza de ' + f.nom }; f.ideo = p.ideo.slice(); }
    m.prog(m.t + 5 + f.rng.r(0, 25), 'faccion.mes', { f: id });
    return f;
  };
  F.miembros = function (m, f) { let n = 0; for (const c of f.coh) n += m.coh[c].n; return n; };
  F.agravio = function (m, f) { let n = 0, s = 0; for (const c of f.coh) { const co = m.coh[c]; n += co.n; s += co.n * co.agr[f.enem.k]; } return n ? s / n : 0; };

  // Detección de comunidades (movimiento local por modularidad + corte en componentes conexas, a la Leiden).
  F.comunidades = function (W) {
    const n = W.length; const k = W.map(r => r.reduce((a, b) => a + b, 0)); const m2 = k.reduce((a, b) => a + b, 0);
    const com = W.map((_, i) => i); if (m2 <= 0) return com;
    for (let it = 0, mej = true; mej && it < 30; it++) {
      mej = false;
      for (let i = 0; i < n; i++) {
        const kin = {}, tot = {};
        for (let j = 0; j < n; j++) { if (j === i) continue; tot[com[j]] = (tot[com[j]] || 0) + k[j]; if (W[i][j] > 0) kin[com[j]] = (kin[com[j]] || 0) + W[i][j]; }
        let best = com[i], bg = (kin[com[i]] || 0) - k[i] * (tot[com[i]] || 0) / m2;
        for (const c in kin) { const gan = kin[c] - k[i] * (tot[c] || 0) / m2; if (gan > bg + 1e-12) { bg = gan; best = +c; } }
        if (best !== com[i]) { com[i] = best; mej = true; }
      }
    }
    // Refinado: una comunidad tiene que ser conexa.
    let sig = n; const visto = new Array(n).fill(false);
    for (let i = 0; i < n; i++) {
      if (visto[i]) continue; const c = com[i]; const pila = [i]; visto[i] = true; const grupo = [i];
      while (pila.length) { const x = pila.pop(); for (let j = 0; j < n; j++) if (!visto[j] && com[j] === c && W[x][j] > 0) { visto[j] = true; pila.push(j); grupo.push(j); } }
      const resto = []; for (let j = 0; j < n; j++) if (!visto[j] && com[j] === c) resto.push(j);
      if (resto.length) { for (const j of grupo) com[j] = sig; sig++; }
    }
    return com;
  };

  const compartido = (x, y) => { let s = 0; for (const k of CLAVES) { const v = Math.min(x.agr[k], y.agr[k]); if (v > 0.25) s += v; } return s; };
  function objetivo(m, a, k, co) {
    if (k === 'reg') return a.est;
    if (k === 'acap') return a.acapFac;
    if (k === 'ext') return a.extId === undefined ? -1 : a.extId;
    if (k === 'crim') return a.crimId === undefined ? -1 : a.crimId;
    return a.id;
  }

  // Cada mes: ¿hay en este asentamiento una comunidad con agravio común y alguien que conecte a la gente?
  S.gancho('asent.mes', function (m, a) {
    const cohs = a.coh.map(c => m.coh[c]).filter(c => c.n > 0); const n = cohs.length; if (n < 1) return;
    const W = [];
    for (let i = 0; i < n; i++) { W.push(new Array(n).fill(0)); }
    for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) {
      const x = cohs[i], y = cohs[j];
      const conf = 0.3 + (x.ins >= 0 && y.ins >= 0 ? 0.3 : 0.2);
      W[i][j] = W[j][i] = conf * compartido(x, y);
    }
    const com = F.comunidades(W); const grupos = new Map();
    for (let i = 0; i < n; i++) { if (!grupos.has(com[i])) grupos.set(com[i], []); grupos.get(com[i]).push(cohs[i]); }
    for (const [, gr] of grupos) {
      let N = 0; const sum = { reg: 0, pat: 0, acap: 0, ext: 0, crim: 0 };
      for (const c of gr) { N += c.n; for (const k of CLAVES) sum[k] += c.n * c.agr[k]; }
      let k = 'reg'; for (const x of CLAVES) if (sum[x] > sum[k]) k = x;
      const media = sum[k] / N;
      if (N < 8 || media < 0.5) continue;
      const obj = objetivo(m, a, k, gr[0]);
      // Si ya hay una facción aquí con ese enemigo, la comunidad se le une.
      const ya = m.fac.find(f => f.vivo && f.enem.k === k && f.enem.id === obj && (f.sede === a.id || f.cel.some(c => c.ase === a.id)));
      if (ya) { for (const c of gr) if (c.fac < 0 && c.agr[k] >= 0.4) { c.fac = ya.id; ya.coh.push(c.id); } continue; }
      if (gr.some(c => c.fac >= 0)) continue;
      // El conector: alguien con carisma ≥ 0,7 entre N personas (cada una tiene un 8,7 %).
      const rng = new S.Rng(S.hash(m.semilla, 'conector', a.id, CLAVES.indexOf(k), Math.floor(m.t / S.ANIO)));
      if (!a.conector && !rng.p(1 - Math.pow(0.913, Math.min(N, 400)))) continue;
      const co = gr.slice().sort((x, y) => y.n * y.agr[k] - x.n * x.agr[k])[0];
      const pred = m.per.find(p => p.vivo && p.rol === 'predicador' && p.casa === a.id && p.fac < 0 && p.car >= 0.7);
      const lid = pred && k === 'reg' && rng.p(0.4) ? pred : S.per.crear(m, { casa: a.id, est: a.est, rol: 'lider', car: rng.r(0.7, 0.98), ideo: co.ideo, sexo: rng.p(0.5) ? 'M' : 'H', nace: m.t - rng.r(26, 55) * S.ANIO });
      lid.coh = co.id;
      const hs = a.hechos.filter(h => HECHO_DE[k].indexOf(h.clave) >= 0).sort((x, y) => y.s - x.s);
      const f = F.nueva(m, { lid: lid.id, sede: a.id, enem: { k, id: obj }, hecho: hs[0] || null, fund: hs[0] ? hs[0].ev : -1 });
      for (const c of gr) if (c.agr[k] >= 0.4) { c.fac = f.id; f.coh.push(c.id); }
      f.cel.push({ ase: a.id, lid: lid.id, ideo: co.ideo.slice(), padre: -1, puente: 1 });
      f.evProto = m.reg('protofaccion', 'En {A' + a.id + '}, {P' + lid.id + '} conoce a todo el mundo y empieza a juntar a la gente: ' + ({ pat: 'colectas para las viudas', reg: 'reuniones a puerta cerrada', acap: 'corros frente a los almacenes', ext: 'guardias de vecinos', crim: 'colectas y una lista de nombres' })[k], { a: a.id, imp: 0, c: [f.fund] });
    }
  });

  // Nombre, símbolo y normas salen del hecho fundador: el número de muertos, el lugar, el objeto más recordado.
  F.bautizar = function (m, f) {
    const a = m.ase[f.sede]; const rng = f.rng; const h = f.hecho; const muertos = h ? h.muertos : 0;
    const lid = m.per[f.lid]; const k = f.enem.k;
    const clave = h ? h.clave : 'otro'; const [obj, adj] = rng.el(S.NOM.objeto[clave] || S.NOM.objeto.otro);
    const fem = /a$/.test(adj); const del = fem ? 'de la ' : 'del '; const un = fem ? 'una ' : 'un ';
    f.tipo = lid.rol === 'predicador' ? 'orden' : ({ pat: 'hermandad', crim: 'hermandad', reg: 'rebelde', acap: 'rebelde', ext: 'resistencia' })[k];
    const nomNum = muertos >= 2 && muertos <= 99 ? S.cap(S.numPal(muertos)) : null;
    let nom;
    if (f.tipo === 'hermandad') nom = nomNum ? rng.el(['Hermandad del ' + nomNum, 'Los ' + nomNum + ' de ' + a.nom]) : rng.el(['Hermandad de ' + a.nom, 'Hijos ' + del + obj]);
    else if (f.tipo === 'orden') nom = rng.el(['Orden ' + del + obj + ' ' + adj, 'El Silencio de ' + a.nom, 'Los ' + del + obj + ' ' + adj]);
    else if (f.tipo === 'resistencia') nom = rng.el(['Libres de ' + a.nom, 'Guardia ' + del + obj]);
    else nom = rng.el(['Los ' + del + obj + ' ' + adj, 'Liga de ' + a.nom, 'Los ' + del + obj + ' ' + adj]);
    if (m.fac.some(x => x !== f && x.nom === nom)) nom += ' de ' + a.nom;
    f.nom = nom;
    f.sim = nomNum ? S.numPal(muertos) + ' muescas grabadas en ' + un + obj.toLowerCase() : un + obj.toLowerCase() + ' ' + adj.toLowerCase() + ' pintad' + (fem ? 'a' : 'o') + ' en la manga';
    const N = { explosion: ['trabajar sin inspección', 'paro inmediato'], masacre: ['colaborar con la guardia', 'muerte (juramento de sangre)'], hambre: ['acaparar grano', 'reparto forzoso'], bomba: ['callar lo que se vio', 'destierro'], pirata: ['tratar con peristas', 'la mano'], otro: ['faltar a la asamblea', 'multa'] };
    const nr = N[clave] || N.otro; f.normas.push({ c: nr[0], cast: nr[1], f: 0.5 });
    if (f.tipo === 'orden') f.normas.push({ c: 'dudar de la profecía', cast: 'expulsión', f: 0.5 });
    lid.cargo = { t: 'lider', id: f.id, nom: 'cabeza de ' + f.nom }; lid.not = Math.max(lid.not, 0.6);
    f.etapa = 'inst'; f.caja = F.miembros(m, f) * 3;
    f.evNace = m.reg('faccion', 'Nace ' + f.nom + ' en {A' + a.id + '}: ' + F.miembros(m, f) + ' personas, caja común y un emblema, ' + f.sim, { a: a.id, imp: 2, c: [f.evProto, f.fund], d: { f: f.id } });
    if (h && h.muertos) (f.fiestas = f.fiestas || []).push({ nom: 'Día de los ' + S.cap(S.numPal(h.muertos)), dia: Math.floor(m.ev[h.ev].t % S.ANIO) });
    S.emitir(m, 'faccion.nace', f);
  };

  // λ₂ del grafo de confianza entre células: el termómetro de qué facciones están a una crisis de romperse.
  F.cohesion = function (m, f) {
    const n = f.cel.length; if (n < 2) { f.l2 = 1; return null; }
    const W = []; for (let i = 0; i < n; i++) W.push(new Array(n).fill(0));
    for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) {
      const x = f.cel[i], y = f.cel[j]; let w = 0.02;
      if (m.ase[x.ase].sis === m.ase[y.ase].sis) w = 0.9;
      else if (y.padre === i) w = y.puente; else if (x.padre === j) w = x.puente;
      W[i][j] = W[j][i] = w;
    }
    const r = S.fiedler(W); f.l2 = r.l2; return r;
  };
  function dosMedias(cel) {
    let a = cel[0].ideo.slice(), b = cel[cel.length - 1].ideo.slice(); let g = [];
    for (let it = 0; it < 8; it++) {
      g = cel.map(c => S.dist5(c.ideo, a) <= S.dist5(c.ideo, b) ? 0 : 1);
      for (const [k, cen] of [[0, a], [1, b]]) { const l = cel.filter((_, i) => g[i] === k); if (!l.length) continue; for (let d = 0; d < 5; d++) { let s = 0; for (const c of l) s += c.ideo[d]; cen[d] = s / l.length; } }
    }
    let disp = 0; cel.forEach((c, i) => { disp += Math.pow(S.dist5(c.ideo, g[i] ? b : a), 2); }); disp = Math.sqrt(disp / cel.length);
    return { g, sep: S.dist5(a, b), disp };
  }
  F.cisma = function (m, f, lado, motivo) {
    const fuera = f.cel.filter((_, i) => lado[i] < 0), dentro = f.cel.filter((_, i) => lado[i] >= 0);
    if (!fuera.length || !dentro.length) return null;
    // La sede se queda con quien tenga al líder.
    const [queda, sale] = dentro.some(c => c.ase === f.sede) ? [dentro, fuera] : [fuera, dentro];
    const lidN = sale.map(c => m.per[c.lid]).filter(p => p && p.vivo).sort((x, y) => y.car - x.car)[0] || S.per.crear(m, { casa: sale[0].ase, rol: 'lider', car: 0.75 });
    const g2 = F.nueva(m, { lid: lidN.id, sede: sale[0].ase, enem: { k: f.enem.k, id: f.enem.id }, tipo: f.tipo, etapa: 'inst', O: f.O * 0.6, caja: f.caja * sale.length / f.cel.length, hecho: null });
    f.caja -= g2.caja;
    const asSale = new Set(sale.map(c => c.ase));
    g2.cel = sale.map(c => Object.assign({}, c, { padre: -1 })); g2.cel.forEach((c, i) => { if (i > 0) { c.padre = 0; } });
    f.cel = queda.map(c => Object.assign({}, c)); f.cel.forEach((c, i) => { c.padre = i === 0 ? -1 : 0; });
    f.coh = f.coh.filter(id => { const co = m.coh[id]; if (asSale.has(co.ase)) { co.fac = g2.id; g2.coh.push(id); return false; } return true; });
    const a2 = m.ase[g2.sede];
    g2.nom = f.rng.el(['Los Verdaderos de ', 'Rama de ', 'Los Fieles de ']) + a2.nom; g2.sim = f.sim + ', partido en dos';
    g2.normas = [{ c: 'traición', cast: 'muerte', f: 0.7 }]; lidN.cargo = { t: 'lider', id: g2.id, nom: 'cabeza de ' + g2.nom };
    const ev = m.reg('cisma', f.nom + ' se parte: ' + motivo + '. Nace ' + g2.nom + ' en {A' + a2.id + '}', { a: a2.id, imp: 2, c: [f.evPuente, f.evNace], d: { f: f.id, g: g2.id, l2: f.l2 } });
    g2.fund = ev; g2.evNace = ev; f.evPuente = undefined; f.sinFusion = g2.sinFusion = m.t + 8 * S.ANIO;
    F.cohesion(m, f); F.cohesion(m, g2);
    return g2;
  };

  // ── Acciones de facción: registro. Cada una declara para qué tipos vale, su utilidad y lo que hace.
  const AC = (id, o) => S.def('accionFac', id, o);
  const esTipo = (f, l) => l.indexOf(f.tipo) >= 0;
  AC('colecta', { tipos: ['hermandad', 'sindicato', 'rebelde', 'orden', 'resistencia'], U: (m, f, x) => 0.25 + (f.caja < x.n * 4 ? 0.3 : 0), hacer(m, f, x) { f.caja += x.n * x.agr * 2.5; } });
  AC('reclutar', {
    tipos: ['hermandad', 'sindicato', 'rebelde', 'orden', 'resistencia'], U: () => 0.3,
    hacer(m, f) { for (const c of f.cel) for (const id of m.ase[c.ase].coh) { const co = m.coh[id]; if (co.fac < 0 && co.agr[f.enem.k] >= 0.35) { co.fac = f.id; f.coh.push(id); } } f.comp = Math.min(1, f.comp + 0.03); },
  });
  AC('expandir', {
    tipos: ['hermandad', 'sindicato', 'rebelde', 'orden', 'resistencia'],
    U(m, f, x) {
      const a = m.ase[f.sede]; x.dest = -1; let bs = 0.45;
      for (const d of a.cerca) { const b = m.ase[d]; if (f.cel.some(c => c.ase === d) || m.saltos[a.sis][b.sis] > 2) continue; if (f.enem.k === 'reg' && b.est !== f.enem.id) continue; const ag = m.coh[b.cohGen].agr[f.enem.k === 'pat' ? 'pat' : f.enem.k]; if (ag > bs) { bs = ag; x.dest = d; } }
      return x.dest >= 0 && f.cel.length < 7 ? 0.22 + 0.2 * f.O : -9;
    },
    hacer(m, f, x) {
      const b = m.ase[x.dest]; const org = S.per.crear(m, { casa: f.sede, est: m.ase[f.sede].est, rol: 'organizador', ideo: m.coh[b.cohGen].ideo, car: f.rng.r(0.5, 0.9) });
      org.fac = f.id; org.en = { t: 'A', id: b.id }; org.cargo = { t: 'enlace', id: f.id, nom: 'enlace de ' + f.nom + ' en ' + b.nom };
      const padre = f.cel.findIndex(c => c.ase === f.sede);
      f.cel.push({ ase: b.id, lid: org.id, ideo: m.coh[b.cohGen].ideo.slice(), padre: Math.max(0, padre), puente: f.rng.r(0.25, 0.7) });
      const co = m.coh[b.cohGen]; if (co.fac < 0) { co.fac = f.id; f.coh.push(co.id); }
      m.reg('celula', f.nom + ' llega a {A' + b.id + '}: {P' + org.id + '} es el único enlace entre las dos casas', { a: b.id, imp: 1, c: [f.evNace], d: { f: f.id, p: org.id } });
    },
  });
  AC('exigir', {
    tipos: ['hermandad', 'sindicato'], U: (m, f, x) => 0.35 + 0.3 * x.agr,
    hacer(m, f, x) {
      const a = m.ase[f.sede]; const gob = S.pol.gobernadorDe(m, a);
      const cede = gob ? gob.rng.soft([gob.r[R.EMP] * 0.6 + f.O * 0.15 + (a.huelga > 0 ? 0.3 : 0), gob.r[R.COD] * 0.5 + 0.25], 0.15) === 0 : false;
      if (cede) {
        for (const id of f.coh) m.coh[id].agr[f.enem.k] *= 0.6; f.exitos++; f.comp = Math.min(1, f.comp + 0.1); f.normas[0].f = Math.min(1, f.normas[0].f + 0.1);
        m.reg('concesion', '{A' + a.id + '} cede ante ' + f.nom + ': inspecciones de seguridad y pensiones para las viudas', { a: a.id, imp: 1, c: [f.evNace], d: { f: f.id } });
      } else {
        f.negativas++; for (const id of f.coh) m.coh[id].agr[f.enem.k] = Math.min(1, m.coh[id].agr[f.enem.k] + 0.05);
        if (f.negativas === 2 && f.tipo === 'hermandad') { f.tipo = 'sindicato'; m.reg('sindicato', f.nom + ' ya no pide: es un sindicato. Primero pidió seguridad; luego, justicia', { a: a.id, imp: 1, c: [f.evNace], d: { f: f.id } }); }
      }
    },
  });
  AC('huelga', {
    tipos: ['sindicato'], U: (m, f, x) => f.enHuelga ? -9 : 0.1 + 0.2 * Math.min(3, f.negativas) + 0.4 * x.agr,
    hacer(m, f, x) {
      const dias = S.clamp(f.caja / Math.max(1, x.n * 0.5), 10, 70); const paradas = [];
      for (const id of f.coh) { const co = m.coh[id]; if (co.ins >= 0) { m.ins[co.ins].huelga = true; paradas.push(co.ins); m.ase[co.ase].huelga = m.t + dias; } }
      if (!paradas.length) return;
      f.enHuelga = true; f.caja = Math.max(0, f.caja - x.n * 0.5 * dias * 0.5);
      const ev = m.reg('huelga', f.nom + ' convoca huelga: paran ' + paradas.length + ' instalaciones (' + paradas.slice(0, 3).map(i => '{I' + i + '}').join(', ') + ') durante ' + Math.round(dias) + ' días', { a: f.sede, imp: 2, c: [f.evNace], d: { f: f.id, dias } });
      for (const i of paradas) { m.ins[i].evHuelga = ev; const a = m.ase[m.ins[i].ase]; if (m.ins[i].tipo === 'fab_nucleos') for (const b of m.ase) b.evSinNucleos = ev; a.evEscasez = ev; }
      S.inf.crear(m, ev, 'huelga', f.sede, 0.5, paradas.length, { f: f.id });
      m.prog(m.t + dias, 'huelga.fin', { f: f.id, ins: paradas, ev });
    },
  });
  AC('pintadas', {
    tipos: ['rebelde', 'orden', 'resistencia'], U: () => 0.3,
    hacer(m, f) { for (const c of f.cel) { const a = m.ase[c.ase]; a.ret = Math.min(1, a.ret + 0.08); a.senal += 0.015; } },
  });
  AC('contrabando', {
    tipos: ['rebelde', 'hermandad', 'orden'],
    U(m, f, x) {
      const a = m.ase[f.sede]; if (a.H < 0.18 || f.caja < 3000) return -9;
      x.src = -1; let bp = a.pr[B.grano] * 0.6;
      for (const d of a.cerca) { const e = a.tabla.get(d); if (e && e.p[B.grano] < bp && m.ase[d].alm[B.grano] > 200) { bp = e.p[B.grano]; x.src = d; } }
      return x.src >= 0 ? 0.5 + a.H : -9;
    },
    hacer(m, f, x) {
      const a = m.ase[f.sede], b = m.ase[x.src]; const q = Math.min(b.alm[B.grano] * 0.3, f.caja * 0.7 / b.pr[B.grano], 600);
      const r = S.eco.comprar(m, b, B.grano, q); f.caja -= r.coste;
      m.prog(m.t + S.nav.distancia(m, a.sis, b.sis) / 55 + 2, 'faccion.grano', { f: f.id, a: a.id, q: r.q, de: b.id });
    },
  });
  AC('sabotaje', {
    tipos: ['rebelde', 'resistencia'],
    U(m, f, x) { const l = m.per[f.lid]; return f.O < 1.15 || m.t < (f.tSabotaje || 0) ? -9 : 0.05 + l.r[R.VAL] * 0.25 - l.r[R.PRU] * 0.2 + x.agr * 0.2; },
    hacer(m, f) {
      f.tSabotaje = m.t + 1.5 * S.ANIO;
      const c = f.rng.el(f.cel); const a = m.ase[c.ase];
      const obj = a.ins.map(i => m.ins[i]).filter(i => (i.tipo === 'cuartel' || i.tipo === 'policia' || i.tipo === 'astillero' || i.tipo === 'ceca' || i.tipo === 'armeria') && i.salud > 0.3);
      if (!obj.length) return;
      const i = f.rng.el(obj);
      const ev = S.eco.danarInst(m, i, { k: 'sabotaje', dano: f.rng.r(0.25, 0.6), muertos: f.rng.r(0, 0.04), txt: 'Sabotaje en {I' + i.id + '}: arde de noche y mueren {n}; en las paredes aparece ' + f.sim, c: [f.evNace], clave: 'reg', hecho: 'bomba', imp: 1, autor: { t: 'F', id: f.id } });
      a.senal += 0.04; a.evSenal = ev; f.exitos++;
    },
  });
  AC('atentado', {
    tipos: ['rebelde', 'resistencia', 'orden'],
    U(m, f, x) {
      const l = m.per[f.lid]; if (f.O < 1.5 || m.t < (f.tAtentado || 0) || f.enem.k !== 'reg' && f.enem.k !== 'ext') return -9;
      const a = m.ase[f.cel[f.rng.i(f.cel.length)].ase]; const e = a.est >= 0 ? m.est[a.est] : null; if (!e) return -9;
      x.obj = a.cap ? e.gob : a.gob; x.a = a.id; if (x.obj < 0 || !m.per[x.obj].vivo) return -9;
      return -0.1 + l.r[R.VAL] * 0.2 + x.agr * 0.25 - l.r[R.PRU] * 0.15 + l.r[R.REN] * 0.1;
    },
    hacer(m, f, x) { f.tAtentado = m.t + 4 * S.ANIO; S.pol.atentado(m, f, m.per[x.obj], m.ase[x.a]); },
  });
  AC('levantamiento', {
    tipos: ['rebelde', 'resistencia', 'orden'],
    U(m, f, x) { const a = m.ase[f.sede]; return a.est < 0 ? -9 : 0.1 + 0.9 * (a.agr - 0.5) + 0.8 * (0.8 - a.R); },
    hacer(m, f, x) { for (const c of f.cel) { const a = m.ase[c.ase]; a.celula = Math.min(0.12, 0.02 + x.n / Math.max(1, a.pob)); a.evSenal = f.evNace; } },
  });
  AC('predicar', {
    tipos: ['orden'], U: () => 0.4,
    hacer(m, f) { for (const c of f.cel) { const co = m.coh[m.ase[c.ase].cohGen]; if (co.fac < 0) { co.fac = f.id; f.coh.push(co.id); } } f.comp = Math.min(1, f.comp + 0.05); f.caja += 300; },
  });
  AC('pagar_venganza', {
    tipos: ['hermandad', 'sindicato'],
    U(m, f, x) { x.ven = m.per.find(p => p.vivo && p.ven && p.ven.estado === 'activa' && p.casa === f.sede && p.din < 2500); return x.ven && f.caja > 4000 ? 0.45 : -9; },
    hacer(m, f, x) { const q = Math.min(6000, f.caja * 0.4); f.caja -= q; x.ven.din += q; x.ven.ven.padrino = f.id; m.reg('colecta_venganza', f.nom + ' pone ' + Math.round(q) + ' de su caja para que {P' + x.ven.id + '} contrate cazarrecompensas', { a: f.sede, imp: 0, c: [f.evNace, x.ven.ven.ev], d: { f: f.id } }); },
  });

  S.en('faccion.grano', function (m, e) {
    const f = m.fac[e.f]; const a = m.ase[e.a]; a.alm[B.grano] += e.q; a.reparto += 0;
    f.exitos++; f.comp = Math.min(1, f.comp + 0.15);
    for (const id of a.coh) { const co = m.coh[id]; if (co.fac < 0) { co.fac = f.id; f.coh.push(id); } }
    a.culpa.acap += 0.5;
    m.reg('contrabando', 'Llegan de noche ' + Math.round(e.q) + ' t de grano a {A' + a.id + '}, pagadas por ' + f.nom + ': en los barrios ya hablan del «fantasma del trigo»', { a: a.id, imp: 1, c: [a.hambruna, f.evNace], d: { f: f.id } });
  });
  S.en('huelga.fin', function (m, e) {
    const f = m.fac[e.f]; f.enHuelga = false;
    for (const i of e.ins) m.ins[i].huelga = false;
    const a = m.ase[f.sede]; a.huelga = 0;
    const gob = S.pol.gobernadorDe(m, a);
    const cede = !gob || gob.rng.p(0.35 + gob.r[R.EMP] * 0.4);
    if (cede) { for (const id of f.coh) m.coh[id].agr[f.enem.k] *= 0.5; f.exitos++; f.negativas = 0; f.comp = Math.min(1, f.comp + 0.15); }
    else { f.comp = Math.max(0, f.comp - 0.15); f.negativas++; }
    m.reg('huelga_fin', 'Acaba la huelga de ' + f.nom + (cede ? ': la patronal cede' : ': se acaba la caja de resistencia y vuelven al trabajo sin nada'), { a: f.sede, imp: 1, c: [e.ev], d: { f: f.id } });
  });

  S.en('faccion.mes', function (m, e) {
    const f = m.fac[e.f]; if (!f.vivo) return;
    m.prog(e.t + S.MES, 'faccion.mes', e);
    if (f.tipo === 'casa') return F.casaMes(m, f);
    if (f.estado !== undefined && f.estado >= 0) return;                // ya gobierna: actúa como Estado
    const lid = m.per[f.lid];
    if (!lid || !lid.vivo) return;
    f.coh = f.coh.filter(id => m.coh[id].fac === f.id && m.coh[id].n > 0);
    const n = F.miembros(m, f), agr = F.agravio(m, f); const a = m.ase[f.sede];
    f.comp += (S.clamp(agr + 0.1 * Math.min(3, f.exitos), 0, 1) - f.comp) * 0.15;
    if (f.etapa === 'proto') {
      // dO/dt = miembros·agravio·habilidad / k − represión
      const rep = f.enem.k === 'reg' && a.est >= 0 ? a.R * m.est[a.est].rep * 0.08 : 0.01;
      f.O = Math.max(0, f.O + Math.min(n, 400) * agr * lid.hab / 500 - rep);
      if (agr < 0.28 || n < 8) { f.mesesNeg++; if (f.mesesNeg >= 4) F.disolver(m, f, true); } else f.mesesNeg = 0;
      if (f.O >= 1) F.bautizar(m, f);
      return;
    }
    // Las ideas de cada célula derivan hacia las de su gente; la de la facción es la media.
    for (const c of f.cel) { const gi = m.coh[m.ase[c.ase].cohGen].ideo; for (let d = 0; d < 5; d++) c.ideo[d] = S.clamp(c.ideo[d] + 0.08 * (gi[d] - c.ideo[d]) + f.rng.n(0, 0.03), -1, 1); if (c.padre >= 0 && c.lid >= 0 && !m.per[c.lid].vivo) c.puente = Math.min(c.puente, 0.04); }
    for (let d = 0; d < 5; d++) { let s = 0; for (const c of f.cel) s += c.ideo[d]; f.ideo[d] = s / f.cel.length; }
    f.O = Math.min(3, f.O + 0.02 + 0.02 * f.exitos * 0.2);
    const coh = F.cohesion(m, f);
    if (coh && f.cel.length >= 2) {
      const dm = dosMedias(f.cel);
      const ideas = dm.sep > 2 * Math.max(0.05, dm.disp) && dm.sep > 0.42;
      if (f.l2 < 0.06 || (ideas && f.l2 < 0.36)) { F.cisma(m, f, coh.vec, f.l2 < 0.06 ? 'muerto el enlace, las dos casas ya no se hablan (λ₂ = ' + f.l2.toFixed(2) + ')' : 'las ideas se han separado y cada bando tiene quien lo guíe (λ₂ = ' + f.l2.toFixed(2) + ')'); }
    }
    if (f.caja < 0 || n < 6) f.mesesNeg++; else f.mesesNeg = 0;
    if ((f.mesesNeg >= 3 && f.comp < 0.3) || n < 6 && f.mesesNeg >= 6) return F.disolver(m, f);
    if (agr < 0.2) { f.frio = (f.frio || 0) + 1; if (f.frio >= 14) return F.disolver(m, f); } else f.frio = 0;
    F.fusion(m, f);
    // El líder elige qué hacer este mes.
    const x = { n, agr }; const ids = [], us = [];
    for (const id in S.reg.accionFac) { const ac = S.reg.accionFac[id]; if (!esTipo(f, ac.tipos)) continue; const u = ac.U(m, f, x); if (u > -5) { ids.push(id); us.push(u); } }
    if (!ids.length) return;
    const k = lid.rng.soft(us, 0.15);
    f.ult = ids[k]; S.reg.accionFac[ids[k]].hacer(m, f, x);
  });

  F.fusion = function (m, f) {
    for (const o of m.fac) {
      if (o === f || !o.vivo || o.etapa !== 'inst' || o.tipo === 'casa' || o.enem.k !== f.enem.k || o.enem.id !== f.enem.id || o.estado >= 0) continue;
      if (S.dist5(f.ideo, o.ideo) > 0.2 || m.saltos[m.ase[f.sede].sis][m.ase[o.sede].sis] > 2 || m.t < (f.sinFusion || 0) || m.t < (o.sinFusion || 0)) continue;
      const [gr, ch] = F.miembros(m, f) >= F.miembros(m, o) ? [f, o] : [o, f];
      const lc = m.per[ch.lid]; if (!lc || !lc.vivo || lc.r[R.AMB] >= 0.5 && !(gr.caja > 5000)) continue;   // el que queda debajo acepta: poca ambición o un cargo que lo compense
      const base = gr.cel.length;
      for (const c of ch.cel) gr.cel.push(Object.assign({}, c, { padre: c.padre < 0 ? 0 : c.padre + base, puente: c.padre < 0 ? 0.4 : c.puente }));
      for (const id of ch.coh) { m.coh[id].fac = gr.id; gr.coh.push(id); }
      gr.caja += ch.caja; ch.vivo = false; lc.fac = gr.id; lc.cargo = { t: 'enlace', id: gr.id, nom: 'segundo de ' + gr.nom };
      m.reg('fusion', ch.nom + ' se une a ' + gr.nom + ': mismas ideas, mismo enemigo', { a: gr.sede, imp: 1, c: [gr.evNace, ch.evNace], d: { f: gr.id } });
      return;
    }
  };
  F.disolver = function (m, f, callado) {
    f.vivo = false;
    for (const id of f.coh) if (m.coh[id].fac === f.id) m.coh[id].fac = -1;
    const l = m.per[f.lid]; if (l && l.vivo && l.cargo && l.cargo.t === 'lider') l.cargo = null;
    if (!callado) m.reg('disolucion', f.nom + ' se disuelve: sin caja, sin éxitos y sin nadie que vaya a las reuniones', { a: f.sede, imp: 1, c: [f.evNace], d: { f: f.id } });
  };

  // Si muere quien conectaba dos casas, el puente se queda en nada; si muere el líder, sube otro.
  S.gancho('pers.muere', function (m, p, ev) {
    if (p.fac < 0) return; const f = m.fac[p.fac]; if (!f || !f.vivo) return;
    for (const c of f.cel) if (c.lid === p.id && c.padre >= 0) { c.puente = 0.03; f.evPuente = ev; }
    if (f.lid === p.id) {
      const cand = f.cel.map(c => m.per[c.lid]).filter(x => x && x.vivo).sort((x, y) => y.car - x.car)[0] || (f.tipo === 'casa' || f.etapa === 'inst' ? S.per.crear(m, { casa: f.sede, rol: f.tipo === 'casa' ? 'mercader' : 'lider', car: f.rng.r(0.5, 0.85), ideo: f.ideo }) : null);
      if (!cand) return F.disolver(m, f, true);
      f.lid = cand.id; cand.fac = f.id; cand.cargo = { t: 'lider', id: f.id, nom: 'cabeza de ' + f.nom };
      if (f.etapa === 'inst') m.reg('sucesion_faccion', '{P' + cand.id + '} toma la cabeza de ' + f.nom, { a: f.sede, imp: 0, c: [ev] });
      if (f.tipo !== 'casa') f.comp = Math.max(0, f.comp - 0.1);
    }
  });

  // ── Casas mercantes: prestan, acaparan cuando huelen el hambre y encargan naves.
  F.casaMes = function (m, f) {
    const sede = m.ase[f.sede];
    if (!f.almacenes) { f.almacenes = [f.sede]; for (let i = 0; i < 2 && sede.cerca.length; i++) f.almacenes.push(sede.cerca[f.rng.i(Math.min(10, sede.cerca.length))]); }
    const lid = m.per[f.lid]; const cod = lid && lid.vivo ? lid.r[R.COD] : 0.6;
    for (const id of f.almacenes) {
      const a = m.ase[id]; if (a.acapFac >= 0 && a.acapFac !== f.id) continue;
      const p = a.pr[B.grano] / 100; const tengo = a.acap[B.grano];
      if (tengo > 0 && (p > 2.3 || (p < (a._pAnt || p) * 0.93 && p > 1.2) || p < 0.9)) {
        const r = S.eco.vender(m, a, B.grano, tengo * 0.5); a.acap[B.grano] -= r.q; f.caja += r.ingreso; if (a.acap[B.grano] < 5) { a.acap[B.grano] = 0; a.acapFac = -1; }
      } else if (tengo < 1500 && f.caja > 30000 && cod > 0.55 && p > 1.25 && p < 2.1 && p > (a._pAnt || p) * 1.04 && a.alm[B.grano] > 100) {
        // Huele el hambre: compra y guarda.
        const q = Math.min(a.alm[B.grano] * 0.25, f.caja * 0.2 / a.pr[B.grano]); const r = S.eco.comprar(m, a, B.grano, q);
        a.acap[B.grano] += r.q; a.acapFac = f.id; f.caja -= r.coste;
        if (!a.evAcap || m.t - m.ev[a.evAcap].t > 200) a.evAcap = m.reg('acaparamiento', f.nom + ' compra grano en {A' + a.id + '} y lo guarda: el precio sube y los almacenes están llenos', { a: a.id, imp: 1, c: [a.causaHambre], d: { f: f.id } });
      }
      a._pAnt = p;
    }
    let naves = 0; for (const n of m.nav) if (n.vivo && n.dueno.t === 'F' && n.dueno.id === f.id) naves++;
    f.naves = naves;
    if (f.caja > 200000 && !f.pedido && naves < 45 && !m.nav.some(n => n.vivo && n.st === 'amarrada')) {
      let y = null, bd = Infinity; for (const a of m.ase) if (a.pedidos.length < 3 && a.ins.some(i => m.ins[i].tipo === 'astillero' && m.ins[i].salud > 0.5)) { const d = m.dist[sede.sis][a.sis]; if (d < bd) { bd = d; y = a; } }
      if (y) { f.caja -= 70000; if (y.est >= 0) m.est[y.est].ingr += 20000; f.pedido = true; S.eco.pedirNave(m, y, 'carguero', { casa: f.id, dinero: 25000 }); }
    }
    if (f.caja < -50000) { f.mesesNeg++; if (f.mesesNeg > 12) { F.disolver(m, f, true); m.reg('quiebra', f.nom + ' quiebra', { a: f.sede, imp: 1, c: [f.evQuiebra], d: { f: f.id } }); for (const n of m.nav) if (n.vivo && n.dueno.t === 'F' && n.dueno.id === f.id) { n.dueno = { t: 'P', id: n.cap }; } } } else f.mesesNeg = 0;
  };
  S.gancho('nave.nueva', function (m, n, p) {
    if (p.casa === undefined) return; const f = m.fac[p.casa]; f.pedido = false;
    const a = m.ase[n.en]; const cap = S.per.crear(m, { casa: a.id, est: a.est, rol: 'capitan' });
    n.cap = cap.id; cap.en = { t: 'N', id: n.id }; cap.cargo = { t: 'capitan', id: n.id, nom: 'capitán de la ' + n.nom }; n.dueno = f.vivo ? { t: 'F', id: f.id } : { t: 'P', id: cap.id };
  });

  S.sismografo('facciones', 'Facciones vivas', m => { let k = 0; for (const f of m.fac) if (f.vivo && f.etapa === 'inst' && f.tipo !== 'casa') k++; return k; });
  S.sismografo('l2min', 'λ₂ mínima entre facciones', m => { let k = 1; for (const f of m.fac) if (f.vivo && f.cel.length >= 2 && f.l2 < k) k = f.l2; return k; });
  S.gancho('sismo', function (m) { for (const f of m.fac) if (f.vivo && f.etapa === 'inst' && f.tipo !== 'casa') { m.serie('L' + f.id, f.l2); m.serie('M' + f.id, F.miembros(m, f)); } });
})(typeof globalThis !== 'undefined' ? globalThis : this);
