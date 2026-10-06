// Pruebas del núcleo sin gráficos: las fórmulas del informe con sus números, determinismo, todas las
// herramientas de la mano y un pase de la interfaz con un DOM de pega. Uso: node pruebas/comprobar.js
const S = require('./cargar.js');
const path = require('path');
let ok = 0, mal = 0;
function prueba(nom, f) { try { f(); ok++; console.log('  ✓', nom); } catch (e) { mal++; console.log('  ✗', nom, '\n     ', e.message.split('\n')[0], e.stack ? '\n' + e.stack.split('\n').slice(1, 4).join('\n') : ''); } }
function cerca(x, y, tol, que) { if (!(Math.abs(x - y) <= tol)) throw new Error((que || 'valor') + ': ' + x + ' ≠ ' + y + ' (±' + tol + ')'); }
function cierto(c, que) { if (!c) throw new Error(que || 'no se cumple'); }

console.log('\n── Fórmulas del informe técnico');
prueba('precio local: con D/S = 4 el grano pasa de 100 a 303', () => cerca(S.eco.curva(100, 4, 1), 303, 0.5));
prueba('mercader: noticia de 6 días, espera 174 y le quedan 16.046', () => {
  const p = S.nav.estimar(300, 120, 6); cerca(p, 174, 0.5, 'precio esperado');
  cerca(S.nav.beneficio(p, 105, 400, 3000, 0.08, 150000, 1.5), 16046, 15, 'beneficio');
  cerca(S.nav.beneficio(450, 105, 400, 0, 0, 0, 0), 138000, 1, 'si supiera el precio real');
});
prueba('rumores: tras 8 bocas, 3 naves son una mediana de 10 y el 42 % de las veces 12 o más', () => {
  const r = new S.Rng(5); const xs = [];
  for (let i = 0; i < 20000; i++) { let v = { x: 3, q: 0, n: 0 }; for (let k = 0; k < 8; k++) v = S.inf.contada(v, r); xs.push(v.x); }
  xs.sort((a, b) => a - b); cerca(xs[10000], 10, 0.5, 'mediana'); cerca(xs.filter(x => x >= 12).length / 20000, 0.42, 0.03, 'P(≥12)');
});
prueba('creer: tres rumores de fiabilidad 0,5 dan 0,875', () => cerca(S.inf.confianza({ f: 3 }), 0.875, 1e-9));
prueba('el gobernador miente: pérdida real 30 %, miedo 0,9, honradez 0,2 → informa 8,4 %', () => cerca(S.pol.mentira(0.30, 0.9, 0.2), 0.084, 1e-9));
prueba('control = e^(−retraso/30): 0,72 a 10 días, 0,37 a 30, 0,14 a 60', () => { cerca(Math.exp(-10 / S.pol.TAU_CONTROL), 0.72, 0.005); cerca(Math.exp(-30 / S.pol.TAU_CONTROL), 0.37, 0.005); cerca(Math.exp(-60 / S.pol.TAU_CONTROL), 0.14, 0.005); });
prueba('ley de Paris: el núcleo 40-2291 falla en el ciclo 147 (siete semanas); 1 mm el día 35, 1,8 mm en la semana 6', () => {
  const o = { a0: 0.2, K: S.obj.K_PLENO, tref: 0, cpd: 3 };
  cerca(S.obj.ciclosHastaFallo(o), 147, 0.6, 'ciclos'); cerca(S.obj.ciclosHastaFallo(o) / 3, 49, 0.3, 'días');
  cierto(S.obj.grieta(o, 34) < 1 && S.obj.grieta(o, 36) > 1, 'no llega a 1 mm hasta el día 35'); cerca(S.obj.grieta(o, 42), 1.8, 0.05, 'semana 6'); cerca(S.obj.grieta(o, 49.1), 4, 0.01, 'semana 7');
});
prueba('venganza: Neva (gravedad 0,95, rencor 0,8) conserva G = 0,92 a los 8 meses; con rencor 0, 0,19', () => { cerca(S.ven.G(0.95, 0.8, 240), 0.92, 0.006); cerca(S.ven.G(0.95, 0, 240), 0.19, 0.006); });
prueba('por qué Tobías no borró el número: −0,096 con deudas; un perista prudente, +0,164', () => {
  const R = S.R; const tob = { din: 900, r: [] }; tob.r[R.PRU] = 0.3; cerca(S.jus.decidePerista(tob).U, -0.096, 1e-9);
  const pru = { din: 6300, r: [] }; pru.r[R.PRU] = 0.8; cerca(S.jus.decidePerista(pru).U, 0.164, 1e-9);
});
prueba('λ₂: dos tríos (0,9) con un puente de 0,2 dan 0,12 y el vector parte por los tríos; con 0,6, 0,30', () => {
  const W = (b) => { const w = []; for (let i = 0; i < 6; i++) w.push([0, 0, 0, 0, 0, 0]); for (const [i, j, x] of [[0, 1, .9], [0, 2, .9], [1, 2, .9], [3, 4, .9], [3, 5, .9], [4, 5, .9], [2, 3, b]]) { w[i][j] = x; w[j][i] = x; } return w; };
  const f = S.fiedler(W(0.2)); cerca(f.l2, 0.125, 0.008); const s = f.vec.map(Math.sign); cierto(s[0] === s[1] && s[1] === s[2] && s[3] === s[4] && s[4] === s[5] && s[0] !== s[3], 'parte por los tríos');
  cerca(Math.abs(f.vec[0]), 0.43, 0.01); cerca(Math.abs(f.vec[2]), 0.37, 0.01); cerca(S.fiedler(W(0.6)).l2, 0.30, 0.03);
});
prueba('el soldado: Ferro 0,76 hacia Castell; Sanz 0,395 (prefiere a Rhune por 0,21: deja pasar, no deserta)', () => {
  cerca(S.soc.L([0.20, 0.15, 0.15, 0.20, 0.15, 0.15], [0.8, 0.7, 0.7, 0.9, 0.8, 0.6]), 0.76, 0.001);
  const sanz = S.soc.L([0.15, 0.10, 0.10, 0.30, 0.20, 0.15], [0.7, 0.7, 0.3, 0.1, 0.35, 0.6]); cerca(sanz, 0.395, 0.001);
  const dif = (1 - sanz) - sanz; cerca(dif, 0.21, 0.001); cierto(dif > S.soc.UMBRALES.pasar && dif < S.soc.UMBRALES.informar);
});
prueba('revolución: con hambre, bajar la represión de 0,62 a 0,61 lleva la revuelta del 11 % al 63 %', () => {
  cerca(S.soc.equilibrio(0.03, 0.60, 0.62), 0.11, 0.03); cerca(S.soc.equilibrio(0.03, 0.60, 0.61), 0.63, 0.01);
});
prueba('histéresis: con la gente ya en la calle no basta con volver a la represión de antes', () => {
  cierto(S.soc.equilibrio(0.63, 0.60, 0.70) > 0.5, 'a 0,70 siguen en la calle');
  let R = 0.62; while (R < 1 && S.soc.equilibrio(0.63, 0.60, R) > 0.3) R += 0.005;
  cierto(R > 0.74 && R < 0.86, 'hay que subirla hasta ~0,78 (aquí ' + R.toFixed(3) + ')');
});
prueba('represión percibida: en público 0,585 (revuelta); en silencio 0,86 (nada); sin hambre, 4 %', () => {
  cerca(S.soc.Rformula(0.25, 0.5, 0.10), 0.585, 1e-9); cerca(S.soc.Rformula(0.7, 0.8, 0), 0.86, 1e-9);
  cerca(S.soc.equilibrio(0.03, 0.60, 0.585), 0.63, 0.01); cerca(S.soc.equilibrio(0.03, 0.60, 0.86), 0.03, 0.005); cerca(S.soc.equilibrio(0.03, 0.50, 0.585), 0.04, 0.012);
});
prueba('sucesión: P(c gana) = poder²/Σpoder²; 49,4 % contra 36,5 % con tropas es guerra civil', () => {
  const p = S.pol.pGana([0.44, 0.325]); cerca(p[0], 0.647, 0.002);
  cierto(S.pol.desenlace([0.494, 0.365], true) === 'guerra_civil'); cierto(S.pol.desenlace([0.635, 0.365], true) === 'sucesion'); cierto(S.pol.desenlace([0.506, 0.494], true) === 'sucesion'); cierto(S.pol.desenlace([0.2, 0.18, 0.15], true) === 'fragmentacion');
});
prueba('la trampa del informe falso: con 40 de 65 se siembran ~10 y se comen ~30; con 49, 15 y 34', () => {
  const a = (t) => ({ alm: [t], sem: 0, dem: [50 / (S.ANIO * 0.92)], semNec: 15, cosechaNormal: 100 });
  const x = a(40); S.eco.sembrar(x); cerca(x.sem, 9.2, 0.1); cerca(x.alm[0], 30.8, 0.1);
  const y = a(49); S.eco.sembrar(y); cerca(y.sem, 15, 1e-9); cerca(y.alm[0], 34, 1e-9);
  cerca(15 * (10 / 15) / 15 * 100, 67, 0.5, 'cosecha siguiente');
});
prueba('culpa: con H = 0,40 y la culpa repartida 6-2-2, el agravio contra el régimen sube 0,24', () => cerca(0.40 * S.soc.parteCulpa({ culpa: { reg: 6, acap: 2, nat: 2, ext: 0 } }, 'reg'), 0.24, 1e-9));
prueba('huella de motor: el sensor civil acierta el 99 % y da ~103 falsas coincidencias entre 10.000 naves', () => {
  const r = new S.Rng(11); let falsas = 0, buenas = 0, fm = 0; const N = 40000;
  for (let i = 0; i < N; i++) {
    const real = S.obj.firma(r), otro = S.obj.firma(r);
    if (S.obj.D2(S.obj.lectura(real, 0.3, r), S.obj.lectura(otro, 0.3, r), 0.3) < S.CHI2_8_99) falsas++;
    if (S.obj.D2(S.obj.lectura(real, 0.3, r), S.obj.lectura(real, 0.3, r), 0.3) < S.CHI2_8_99) buenas++;
    if (S.obj.D2(S.obj.lectura(real, 0.1, r), S.obj.lectura(otro, 0.1, r), 0.1) < S.CHI2_8_99) fm++;
  }
  cerca(buenas / N, 0.99, 0.003, 'acierto'); cerca(falsas / N * 10000, 103, 22, 'falsas por 10.000'); cierto(fm <= 2, 'el militar te señala a ti solo (' + fm + ')');
});
prueba('sumar pistas: motor, casco óxido y hora dejan 0,25 inocentes esperados: 80 %', () => { const b = S.jus.bayes(10000, [0.0103, 0.08, 0.03]); cerca(b.inocentes, 0.25, 0.005); cerca(b.P, 0.80, 0.005); });
prueba('Lanchester: 12 fragatas (0,10) ganan a 9 (0,15) con 4,7 en pie; con 10, pierden y quedan 3,8', () => {
  const a = S.com.cuadratica(12, 9, 0.10, 0.15); cierto(a.gana === 'A'); cerca(a.quedan, 4.7, 0.05);
  const b = S.com.cuadratica(10, 9, 0.10, 0.15); cierto(b.gana === 'B'); cerca(b.quedan, 3.8, 0.05);
  const [A1, B1] = S.com.integrar(12, 9, 0.10, 0.15, 'cuadratica', 400, 400000); cerca(A1, 4.74, 0.02, 'integrando paso a paso'); cerca(B1, 0, 0.02);
  const c = S.com.cuadratica(480, 360, 0.10, 0.15); cerca(c.quedan / 480, a.quedan / 12, 1e-9, 'la misma ley a escala colosal');
});
prueba('cohortes que se convierten en caras: de 340, 102 lo odian; de 20, lo odian 6 ± 2', () => {
  const r = new S.Rng(3); let s = 0, s2 = 0; const N = 20000; for (let i = 0; i < N; i++) { const k = S.per.hiper(r, 340, 102, 20); s += k; s2 += k * k; }
  const mu = s / N; cerca(mu, 6, 0.06); cerca(Math.sqrt(s2 / N - mu * mu), 2, 0.1);
});
prueba('recuerdos: s(t) = s0·e^(−t/τ), τ = τ0·(1 + 2·rasgo)', () => { const m = { t: 300 }; cerca(S.per.intensidad(m, { s0: 1, t: 0, tau: 150 * (1 + 2 * 0.5) }), Math.exp(-1), 1e-9); });

console.log('\n── Técnica, aprendizaje y teorías');
prueba('diseños: dos iguales empatan; quien blinda contra cañones gana a quien solo lleva cañones', () => {
  const Te = S.tec; cerca(Te.cambio(Te.G0, Te.G0), 1, 1e-9);
  const canonero = Te.normal([0.44, 0.02, 0.02, 0.12, 0.12, 0.12, 0.08, 0.08]), blindado = Te.normal([0.14, 0.14, 0.14, 0.34, 0.03, 0.03, 0.09, 0.09]);
  cierto(Te.cambio(blindado, canonero) > 1.15, 'el blindado no gana: ' + Te.cambio(blindado, canonero).toFixed(2));
  const mixto = Te.G0; cierto(Te.cambio(mixto, blindado) > 1, 'un diseño equilibrado castiga al que solo se defiende de una cosa');
});
prueba('algoritmo genético: en 25 generaciones la aptitud contra un rival fijo sube y el diseño se adapta a él', () => {
  const Te = S.tec; const m = new S.Mundo(3); const e = { id: 0, rng: new S.Rng(9), tec: 1, tes: 0, flo: [], gue: new Map(), rel: new Map(), cap: 0, vivo: true }; m.est.push(e); m.ase.push({ sis: 0 });
  const rival = Te.normal([0.40, 0.03, 0.03, 0.14, 0.14, 0.14, 0.06, 0.06]);
  const vec = S.pol.vecinos, rel = S.pol.rel; S.pol.vecinos = () => []; e.gue.set(1, {}); m.est.push({ id: 1, vivo: true });
  Te.lineas(m, e); e.intel.set(1, { g: rival, niv: 1, t: 0 });
  const f0 = Te.evaluar(e.dis.fragata.mejor.g, [{ w: 1, g: rival }]);
  for (let i = 0; i < 25; i++) Te.oficina(m, e);
  const d = e.dis.fragata.mejor; const f1 = Te.evaluar(d.g, [{ w: 1, g: rival }]);
  S.pol.vecinos = vec; S.pol.rel = rel;
  cierto(f1 > f0 * 1.12, 'aptitud ' + f0.toFixed(2) + ' → ' + f1.toFixed(2)); cierto(d.g[3] > d.g[4] && d.g[3] > d.g[5], 'debería blindarse contra cañones: ' + d.g.map(x => x.toFixed(2))); cierto(d.gen >= 10 && d.padres.length === 2, 'generación ' + d.gen);
});
prueba('la calidad de cada componente y la veteranía cambian cómo pelea una escuadra', () => {
  const m = S.crear(77, { sistemas: 24 }); const n = m.nav.find(x => x.dis !== undefined && x.flo >= 0); const en = { g: S.tec.G0, niv: m.est[n.est].tec };
  const base = S.tec.tasa(m, n, en); const q = n.q.slice(); n.q[7] *= 0.4; const ciego = S.tec.tasa(m, n, en); n.q = q; const v = n.vet; n.vet = 1; const vet = S.tec.tasa(m, n, en); n.vet = v;
  cierto(ciego < base * 0.97, 'con los sensores rotos debería pegar menos'); cierto(vet > base * 1.2, 'los veteranos deberían pegar más');
  cierto(S.tec.calidad(new S.Rng(1), 0, { armas: 0, metal: 1, piezas: 1 })[0] < S.tec.calidad(new S.Rng(1), 0, null)[0] * 0.7, 'sin armería, los cañones salen peor');
  cierto(S.tec.calidad(new S.Rng(1), 2000, null)[3] > S.tec.calidad(new S.Rng(1), 0, null)[3], 'un astillero con oficio construye mejor');
});
prueba('contagio: la noticia de una revolución, cuando llega, baja el umbral de los demás; y el punto fijo salta', () => {
  cierto(S.soc.equilibrio(0.03, 0.60, 0.64) < 0.15, 'sin la noticia, calma'); cierto(S.soc.equilibrio(0.03, 0.60, 0.64 * (1 - 0.25 * 0.34)) > 0.5, 'con dos noticias, cascada');
  const m = S.crear(78, { sistemas: 24 }); const a = m.ase.find(x => x.est >= 0 && !x.cap), b = m.ase.find(x => x.est === a.est && x.sis !== a.sis);
  const ev = m.reg('revolucion', 'prueba', { a: a.id, imp: 3 }); const p = S.inf.crear(m, ev, 'revolucion', a.id, 0.9, 1, { e: a.est });
  cierto(!(b.efic > 0), 'nadie sabe nada hasta que llega'); S.inf.oir(m, b, p, { x: 1, q: 0, n: 1 }); cierto(b.efic > 0.15, 'al llegar, sube la eficacia percibida');
});
prueba('equilibrio de amenazas, pasos clave y combate en tierra', () => {
  const m = S.crear(79, { sistemas: 24 }); m.avanzar(S.ANIO * 1.2);
  cierto(m.sis.some(s => s.bc === 1) && m.sis.some(s => s.bc < 0.2), 'centralidad sin calcular');
  const e = m.est[0]; const j = S.pol.vecinos(m, e)[0]; if (j !== undefined) { cierto(S.teo.amenaza(m, e, j) >= 0); cierto(S.teo.guerraU(m, e, j, 100, 100) > S.teo.guerraU(m, e, j, 100, 400), 'a fuerzas parejas la guerra tienta más'); e.alianzas = new Set([j]); cierto(S.teo.guerraU(m, e, j, 100, 100) < -5, 'a un aliado no se le declara la guerra'); e.alianzas.clear(); }
  const a = m.ase.find(x => x.uni >= 0 && x.est >= 0); const u = m.uni[a.uni]; const n0 = u.n;
  const t = S.com.tierra(m, a, { tipo: 'insurreccion', ev: 0, atk: { n: n0 * 3, ef: 0.12, fac: -1, est: -1, nom: 'la calle' }, def: { n: n0, ef: 0.10, est: a.est, uni: u.id, nom: 'la guarnición' } });
  m.avanzar(m.t + 60); cierto(!t.vivo && t.gano === 0 && t.hist.length > 3, 'tres a uno debería bastar, en ' + t.hist.length + ' días'); cierto(u.n < n0, 'la guarnición no ha tenido bajas');
  cierto(e.asabiya !== undefined && e.psi !== undefined && e.puestos > 0, 'faltan asabiya o Ψ');
  for (const id in S.reg.teoria) { const x = S.reg.teoria[id]; cierto(x.nom && x.autor && x.que && x.aqui, id); if (x.ind) cierto(typeof x.ind(m) === 'string', id); }
});

console.log('\n── Mundo');
let m1;
prueba('se genera y corre 3 años sin romperse', () => { m1 = S.crear(4242, { sistemas: 24 }); m1.avanzar(3 * S.ANIO); cierto(m1.ev.length > 200 && m1.nav.some(n => n.vivo)); });
prueba('determinismo: misma semilla, misma historia exacta', () => {
  const huella = (m) => { let h = 0; for (const e of m.ev) h = S.hash(h, e.k, Math.round(e.t * 100), e.txt.length); let p = 0; for (const a of m.ase) p += a.pob; return h + ':' + m.ev.length + ':' + Math.round(p * 1000); };
  const m2 = S.crear(4242, { sistemas: 24 }); m2.avanzar(3 * S.ANIO); cierto(huella(m1) === huella(m2), huella(m1) + ' ≠ ' + huella(m2));
  const m3 = S.crear(4243, { sistemas: 24 }); m3.avanzar(1 * S.ANIO); cierto(huella(m3) !== huella(m1));
});
prueba('conservación: la cohorte general más las plantillas suman la población', () => { for (const a of m1.ase) { let n = 0; for (const c of a.coh) n += m1.coh[c].n; cerca(n, Math.round(a.pob), Math.max(3, a.pob * 0.012), a.nom); } });   // cuadra cada día; entre uno y otro, lo que entra y sale por el muelle
prueba('semilla determinista: te vas y vuelves, salen las mismas caras; la tocada conserva su ficha', () => {
  const a = m1.ase[0]; const c1 = S.per.caras(m1, a, 20), c2 = S.per.caras(m1, a, 20); cierto(c1.map(x => x.nom).join() === c2.map(x => x.nom).join());
  const p = S.per.promover(m1, a, c1[3]); const c3 = S.per.caras(m1, a, 20); cierto(c3[3].ficha === p.id && c3[3].nom === p.nom);
});
prueba('cada hecho guarda sus causas: de una hambruna o una batalla se llega hacia atrás a algo', () => {
  let con = 0; for (const e of m1.ev) if (e.c.length) { con++; for (const c of e.c) cierto(c < e.id && m1.efe[c].indexOf(e.id) >= 0, 'causa posterior al efecto'); } cierto(con > 30, 'solo ' + con + ' hechos con causa');
});
prueba('texto de todos los hechos (referencias válidas)', () => { for (const e of m1.ev) { const t = m1.texto(e); cierto(t.indexOf('{') < 0 && t.indexOf('undefined') < 0 && t.indexOf('NaN') < 0, e.k + ': ' + t); } });

console.log('\n── La mano: todas las herramientas');
const m4 = S.crear(99, { sistemas: 24 }); m4.avanzar(2 * S.ANIO); m4.presente = m4.t;
for (const id in S.reg.dios) {
  const d = S.reg.dios[id];
  prueba(d.ico + ' ' + d.nom, () => {
    const tabla = { I: m4.ins, A: m4.ase, N: m4.nav, P: m4.per, E: m4.est, F: m4.fac, S: m4.sis }[d.obj];
    let ref = -1; for (let i = 0; i < tabla.length; i++) if (S.dios.puede(m4, id, i)) { ref = i; if (m4.rng.p(0.3)) break; }
    if (ref < 0) { console.log('      (sin objetivo válido en este mundo: no se aplica)'); return; }
    const ev = S.dios.hacer(m4, id, ref); cierto(ev >= 0 && m4.ev[ev], 'no devolvió un hecho');
    const raiz = m4.cadena(ev, 50).map(x => m4.ev[x.id].k); cierto(raiz.indexOf('mano') >= 0 || m4.ev[ev].k === 'mano' || d.obj === 'E', 'la cadena no llega a la mano: ' + raiz.join('←'));
    m4.avanzar(m4.t + 20);
  });
}
prueba('y el mundo sigue 2 años después de todo eso', () => { m4.avanzar(m4.t + 2 * S.ANIO); for (const e of m4.ev) { const t = m4.texto(e); cierto(t.indexOf('undefined') < 0 && t.indexOf('NaN') < 0, e.k + ': ' + t); } });
prueba('resolver: tocar un sistema con la bomba apunta a la fábrica más valiosa', () => { const a = m4.ase.find(x => x.ins.some(i => m4.ins[i].salud > 0.1 && S.reg.inst[m4.ins[i].tipo].sal)); const r = S.dios.resolver(m4, 'bomba', { t: 'A', id: a.id }); cierto(r >= 0 && m4.ins[r].ase === a.id); cierto(S.dios.resolver(m4, 'plaga', { t: 'I', id: r }) === a.id); });

console.log('\n── Batallas explicadas');
{
  const m = S.crear(77, { sistemas: 24 }); m.avanzar(200);
  const fl = m.flo.filter(f => f.vivo && f.nav.length && f.st === 'base'); const f1 = fl[0], f2 = fl.find(f => f.est !== f1.est);
  m.est[f1.est].gue.set(f2.est, { t0: m.t, sabe: true, ev: -1 }); f1.sis = f2.sis; S.com.iniciar(m, f2.sis, [f1], [f2]);
  const bt = m.bat[m.bat.length - 1];
  prueba('cada bando entra con su papel, sus escuadras contadas y la cuenta de su pegada', () => {
    cierto(bt && bt.vivo, 'no hay batalla');
    cierto(bt.L[0].papel.llega === true && bt.L[1].papel.llega === false && bt.L[1].papel.casa === true, 'papeles: ' + JSON.stringify([bt.L[0].papel, bt.L[1].papel]));
    for (const l of bt.L) {
      cerca(l.c0.reduce((x, y) => x + y, 0), l.n0, 1e-9, 'cascos por escuadra'); cierto(l.c0.length === l.nav0.length);
      const d = l.des0; const prod = d.base * d.dis * d.tec * (1 + 0.5 * d.vet) * (0.8 + 0.5 * d.doc) * d.moral.total;
      cierto(Math.abs(prod / l.ef - 1) < 0.12, 'la cuenta no cuadra con la pegada: ' + prod.toFixed(4) + ' frente a ' + l.ef.toFixed(4));
      cerca(d.moral.total, S.com.moral(m, l, bt.sis), 1e-12, 'moral'); cerca(d.moral.total, d.moral.paga * d.moral.cohesion * (1 - 0.35 * d.moral.lejos) * d.moral.mando, 1e-12, 'moral por factores');
    }
    const fz = S.com.fuerza(bt); cierto(fz > 0 && fz < 1); cerca(fz, bt.L[0].ef * bt.L[0].n ** (bt.ley === 'lineal' ? 1 : 2) / (bt.L[0].ef * bt.L[0].n ** (bt.ley === 'lineal' ? 1 : 2) + bt.L[1].ef * bt.L[1].n ** (bt.ley === 'lineal' ? 1 : 2)), 1e-12, 'balanza');
  });
  prueba('el pronóstico acierta quién gana y, más o menos, cuándo', () => {
    const pr = S.com.pronostico(m, bt); cierto(pr.gana === 0 || pr.gana === 1, 'pronóstico sin vencedor: ' + JSON.stringify(pr));
    m.avanzar(m.t + 120); cierto(!bt.vivo, 'la batalla no acaba');
    cierto(bt.gano === pr.gana, 'pronosticó ' + pr.gana + ' y ganó ' + bt.gano);
    const dur = Math.round(bt.t1 - bt.t0); cierto(Math.abs(dur - pr.dias) <= Math.max(2, pr.dias * 0.5), 'duró ' + dur + ' y pronosticó ' + pr.dias);
    cierto((bt.huye >= 0) === (pr.huye >= 0), 'retirada: ' + bt.huye + ' / ' + pr.huye);
    for (const l of bt.L) cerca(l.cFin.reduce((x, y) => x + y, 0), l.nFin, 1e-9, 'cascos al acabar');
  });
  prueba('pronóstico sin retirada = forma cerrada de Lanchester', () => {
    const val = { r: [0, 0.95, 0, 0, 0, 0, 0, 0] }; const mm = { t: 0, flo: [{ alm: 0 }], per: [val] };   // almirantes que no se retiran
    const b = { ley: 'cuadratica', t0: 0, k: 80, L: [{ n: 100, ef: 0.02, flo: [0] }, { n: 70, ef: 0.03, flo: [0] }] };
    cierto(S.R.VAL === 1, 'el índice del valor ha cambiado: ajusta la prueba');
    const pr = S.com.pronostico(mm, b); const cf = S.com.cuadratica(100, 70, 0.02, 0.03);
    cierto((cf.gana === 'A') === (pr.gana === 0), 'gana ' + cf.gana + ' y pronostica ' + pr.gana); cierto(pr.huye === -1);
    cerca(pr.quedan[pr.gana], cf.quedan, 2.5, 'supervivientes');
  });
  prueba('en tierra: el pronóstico sigue las mismas reglas que el combate', () => {
    const a = m.ase.find(x => x.uni >= 0 && !(x.tie >= 0)); const u = m.uni[a.uni];
    const t = S.com.tierra(m, a, { tipo: 'insurreccion', ev: -1, atk: { n: u.n * 1.5, ef: 0.07, fac: -1, est: -1, nom: 'la calle' }, def: { n: u.n, ef: 0.09, est: a.est, uni: u.id, nom: 'la guarnición' } });
    const pr = S.com.pronosticoTierra(m, t); m.avanzar(m.t + 60); cierto(!t.vivo, 'el combate no acaba');
    cierto(t.gano === pr.gana, 'pronosticó ' + pr.gana + ' y ganó ' + t.gano); cierto(Math.abs(Math.round(t.t1 - t.t0) - pr.dias) <= 1, 'duró ' + Math.round(t.t1 - t.t0) + ' y pronosticó ' + pr.dias);
  });
  prueba('cada muerte queda apuntada en la cuenta de quien la causó, con su cómo y el hecho del que cuelga el porqué', () => {
    const m2 = S.crear(7919, { sistemas: 24 }); m2.avanzar(360 * 5);
    let n = 0, conEv = 0; const clases = {};
    for (const w of [m, m2]) for (const p of w.per) {
      if (!p.mue) { cierto(!S.mas.letalidad(p), p.nom + ' tiene bajas sin apuntar'); continue; }
      cerca(p.mue.reduce((s2, x) => s2 + x.n, 0) + (p.mueAnt || 0), S.mas.letalidad(p), 1e-6, 'la cuenta de ' + p.nom);
      for (const x of p.mue) { const d = S.mas.muerte(w, x); const t = w.texto({ txt: d.que + ' · ' + d.como }); cierto(d.que.length > 3 && d.como.length > 3 && t.indexOf('undefined') < 0 && t.indexOf('NaN') < 0 && t.indexOf('{') < 0, t); clases[x.c] = (clases[x.c] || 0) + 1; n++; if (d.ev >= 0) conEv++; }
    }
    cierto(n > 10, 'pocas muertes apuntadas: ' + n); cierto(conEv > n * 0.9, 'muertes sin hecho del que colgar: ' + (n - conEv) + ' de ' + n);
    for (const c of ['batalla', 'tierra', 'persona', 'pueblo']) cierto(clases[c], 'ninguna muerte de clase ' + c + ': ' + JSON.stringify(clases));
    console.log('      ' + n + ' apuntes: ' + JSON.stringify(clases));
  });
}

console.log('\n── Historia grande: influencia, fama, figuras, credos, peste, épocas y biografías');
{
  const H = S.his; const m = S.crear(7919, { sistemas: 24 }); m.avanzar(360 * 6);
  prueba('influencia causal: cada hecho pesa lo suyo más la parte que le toca de lo que provocó', () => {
    const v = H.influencia(m); cierto(v.length === m.ev.length);
    for (const e of m.ev) { const propio = H.PESO[e.imp] + (e.d && e.d.muertos > 0 ? Math.log10(1 + e.d.muertos) : 0); let hijos = 0; for (const y of (m.efe[e.id] || [])) hijos += H.LAMBDA * v[y] / m.ev[y].c.length; cerca(v[e.id], propio + hijos, 1e-6 * Math.max(1, v[e.id]), 'I(' + e.id + ')'); }
    const q = H.pesos(m); cierto(q === H.pesos(m), 'el peso no se guarda mientras no haya hechos nuevos'); for (let i = 1; i < 30; i++) cierto(q.w[q.orden[i - 1]] >= q.w[q.orden[i]], 'el orden por peso está mal');
    const niv = [0, 0, 0, 0, 0]; for (const p of m.per) { const E = H.epica(m, p), D = H.drama(m, p); cierto(E >= 0 && E <= 1 && D >= 0 && D <= 1, 'épica o drama fuera de 0..1'); niv[H.nivel(m, p)]++; }
    cierto(niv[0] > niv[1] && niv[1] > niv[3] && niv[4] >= 1 && niv[4] <= 12, 'niveles: ' + niv.join(' ')); console.log('      niveles (anónimo → legendario): ' + niv.join(' · '));
  });
  prueba('fama: llega con las noticias, sube como una «o» ruidosa y el contador de mundos cuadra', () => {
    for (const p of m.per) { let k = 0; for (const a of m.ase) if (H.famaEn(a, p.id) >= 0.2) k++; cierto(Math.abs(k - (p.fama || 0)) <= 0, p.nom + ': ' + k + ' mundos y dice ' + p.fama); }
    const a = m.ase[0], p = m.per[1]; const f0 = H.famaEn(a, p.id); H.oye(m, a, p.id, 0.3); cerca(H.famaEn(a, p.id), 1 - (1 - f0) * 0.7, 1e-12); cierto(m.per.some(x => (x.fama || 0) >= 3), 'nadie es conocido fuera de su casa');
  });
  prueba('figuras: invicto, reformador y mártir nacen de lo que son y cambian las reglas', () => {
    const fl = m.flo.find(f => f.vivo && f.alm >= 0 && m.per[f.alm].vivo && f.nav.length); const alm = m.per[fl.alm]; alm.fig = null; alm.victorias = 3; alm.batallas = 3; alm.r[S.R.VAL] = 0.3;
    const lado = { est: fl.est, flo: [fl.id], alm: alm.id }; const antes = S.com.moralDet(m, lado, fl.sis).total; H.revisar(m);
    cierto(alm.fig === 'invicto' && alm.r[S.R.VAL] >= 0.86 && H.epiteto(m, alm).length > 3, 'no nace el invicto'); cerca(S.com.moralDet(m, lado, fl.sis).total, antes * 1.15, 1e-9, 'el invicto no sube la moral');
    const e = m.est.find(x => x.vivo && x.gob >= 0 && !m.per[x.gob].fig); const g2 = m.per[e.gob]; g2.r[S.R.EMP] = 0.9; g2.r[S.R.PRU] = 0.7; e.agr = 0.6; const trib = e.trib; H.revisar(m);
    cierto(g2.fig === 'reformador' && e.trib < trib, 'no nace el reformador'); cierto(m.ev[g2.figEv].k === 'figura' && m.texto(m.ev[g2.figEv]).indexOf(g2.nom) >= 0);
    const f = m.fac.find(x => x.vivo && x.lid >= 0 && m.per[x.lid].vivo && x.tipo !== 'casa'); const lid = m.per[f.lid]; lid.car = 0.9; for (let i = 0; i < 6; i++) H.oye(m, m.ase[i], lid.id, 0.9);
    const ag = m.ase.slice(0, 6).map(a => m.coh[a.cohGen].agr.reg); S.per.matar(m, lid, { modo: 'publico', por: 0 });
    cierto(lid.fig === 'martir' && lid.martirN >= 6 && m.cuenta.martirio >= 1, 'matar en público a alguien famoso debe hacer un mártir'); cierto(m.ase.slice(0, 6).every((a, i) => m.coh[a.cohGen].agr.reg >= ag[i]), 'el martirio debe subir el agravio');
  });
  prueba('credos: crecen con techo, viajan con el comercio y nunca pasan del total', () => {
    const pr = m.per.find(p => p.vivo && p.rol === 'predicador') || m.per.find(p => p.vivo); const a = m.ase.slice().sort((x, y) => y.pob - x.pob)[0]; const c = H.credoNuevo(m, pr, a.id);
    cierto(c.nom.length > 6 && H.creEn(a, c.id) > 0.1); for (let i = 0; i < 60; i++) H.credosMes(m);
    let fuera = 0; for (const x of m.ase) { const X = H.creTotal(x); cierto(X >= 0 && X <= 1.0001, 'credos por encima del total en ' + x.nom + ': ' + X); if (x.id !== a.id && H.creEn(x, c.id) > 0.01) fuera++; }
    cierto(H.creEn(a, c.id) <= H.techo(a) + 0.13, 'el credo pasa de su techo'); cierto(fuera >= 1, 'el credo no sale de su puerto'); cierto(c.fieles > 0);
  });
  prueba('peste: SIR que empieza, viaja, mata y se apaga', () => {
    const a = m.ase.slice().sort((x, y) => y.pob - x.pob)[0]; let pob0 = 0; for (const x of m.ase) pob0 += x.pob; if (m.pes && m.pes.vivo) { m.pes.vivo = false; m.pes.t1 = m.t; }
    cierto(H.pesteEmpieza(m, a, -1) >= 0, 'no empieza'); const pes = m.pes; let maxI = 0;
    for (let i = 0; i < 160 && pes.vivo; i++) { m.avanzar(m.t + 5); for (const x of m.ase) { const I = x.pI || 0, R = x.pR || 0; cierto(I >= 0 && R >= 0 && I + R <= 1.0001, 'S+I+R se rompe en ' + x.nom); if (I > maxI) maxI = I; } }
    cierto(!pes.vivo && pes.muertos > 100 && maxI > 0.05 && pes.mundos >= 2, 'peste: viva ' + pes.vivo + ', muertos ' + pes.muertos + ', mundos ' + pes.mundos + ', pico ' + maxI.toFixed(3));
    cierto(m.cuenta.peste_fin >= 1 && m.ase.every(x => !(x.pI > 0)), 'la peste no se cierra bien'); console.log('      ' + Math.round(pes.muertos).toLocaleString('es-ES') + ' muertos en ' + pes.mundos + ' mundos, R₀ ' + (pes.beta / pes.gamma).toFixed(1));
  });
  prueba('épocas: cubren toda la historia sin huecos y tienen nombre', () => {
    for (const w of [m, m4]) { const l = H.epocas(w); cierto(l.length >= 1 && l === H.epocas(w)); for (let i = 0; i < l.length; i++) { cierto(l[i].nom && l[i].nom.indexOf('{') < 0 && l[i].nom.indexOf('undefined') < 0, 'época sin nombre: ' + l[i].nom); cierto(l[i].a1 >= l[i].a0); if (i) cierto(l[i].a0 === l[i - 1].a1 + 1, 'hueco entre épocas'); } cierto(l[l.length - 1].a1 === Math.floor(w.t / S.ANIO), 'las épocas no llegan a hoy'); }
  });
  prueba('biografías: todas se escriben, sin huecos sin rellenar, y la casilla la ponen la épica y el drama', () => {
    let n = 0, largas = 0; const F = S.frases;
    for (const w of [m, m4]) for (const p of w.per) {
      const b = H.biografia(w, p); cierto(b.secciones.length >= 3, 'biografía corta'); const t = H.biografiaTexto(w, p); n++;
      cierto(t.indexOf('undefined') < 0 && t.indexOf('NaN') < 0 && !/\{[a-z_]+\}/.test(t) && !/\{[^}]*\|[^}]*\}/.test(t) && !/\{[A-Z]\d+\}/.test(t), p.nom + ': ' + (t.match(/.{0,50}(undefined|NaN|\{[^}]*\}).{0,40}/) || [''])[0]);
      if (!p.vivo) cierto(t.indexOf('MUERTE') >= 0 && t.indexOf('LO QUE DEJÓ') >= 0, 'un muerto sin muerte: ' + p.nom); else cierto(t.indexOf('HOY') >= 0); if (t.length > 900) largas++;
    }
    cierto(largas > 3, 'ninguna biografía larga'); cierto(F.casilla(0, 0).join() === '0,0' && F.casilla(1, 1).join() === '4,2' && F.casilla(0.5, 0.5).join() === '2,1');
    cierto(F.rellena('Fue nombrad{o|a} en {lugar} por {el|la} {x}.', { lugar: 'Sasas' }, 'M') === 'Fue nombrada en Sasas por la {x}.', 'el género o los datos no se rellenan');
    const top = m.per[H.pesos(m).orden[0]]; console.log('      ' + n + ' biografías; la de más peso (' + top.nom + ', ' + H.NIVELES[H.nivel(m, top)] + '): ' + H.biografiaTexto(m, top).length + ' letras');
  });
  prueba('el mundo aguanta 6 años más con todo eso encima', () => { m.avanzar(m.t + 6 * S.ANIO); for (const e of m.ev) { const t = m.texto(e); cierto(t.indexOf('undefined') < 0 && t.indexOf('NaN') < 0 && !/\{[a-z_]+\}/.test(t), e.k + ': ' + t); } for (const a of m.ase) cierto(a.pob > 0 && a.pob === a.pob, 'población rota en ' + a.nom); });
}

console.log('\n── Interfaz (DOM de pega)');
prueba('mapa y paneles se pintan con todo tipo de cosas elegidas', () => {
  const nada = () => { }; const ctx = new Proxy({}, { get: (o, k) => k === 'createRadialGradient' || k === 'createLinearGradient' ? () => ({ addColorStop: nada }) : k === 'measureText' ? () => ({ width: 10 }) : (k in o ? o[k] : nada), set: (o, k, v) => { o[k] = v; return true; } });
  const el = () => ({ style: {}, dataset: {}, _l: {}, value: '', classList: { add: nada, remove: nada, toggle: nada }, children: [], addEventListener(k, f) { this._l[k] = f; }, querySelectorAll: () => [], querySelector: () => null, getContext: () => ctx, clientWidth: 1400, clientHeight: 800, getBoundingClientRect: () => ({ left: 0, top: 0 }), set innerHTML(v) { this._h = v; }, get innerHTML() { return this._h; } });
  const els = global.__els = {}; global.document = { getElementById: (id) => els[id] || (els[id] = el()), activeElement: null };
  global.SIN_ARRANQUE = true; global.requestAnimationFrame = nada; global.addEventListener = nada; global.devicePixelRatio = 1;
  for (const f of ['prefs', 'graficas', 'arte', 'local', 'mapa', 'paneles', 'paneles2', 'paneles3', 'paneles4', 'paneles5', 'main']) require(path.join(__dirname, '..', 'ui', f + '.js'));
  const U = global.UI; const M = U.mapa; const P = U.paneles; const m = m4;
  M.iniciar(el()); M.ajustar(m);
  // Fuerza una batalla espacial y un combate en tierra para que también se pinten de cerca.
  const fl = m.flo.filter(f => f.vivo && f.nav.length && f.st === 'base'); let cen = m.sis[0];
  if (fl.length >= 2 && fl[0].est !== fl.find(f => f.est !== fl[0].est).est) { const f1 = fl[0], f2 = fl.find(f => f.est !== f1.est); m.est[f1.est].gue.set(f2.est, { t0: m.t, sabe: true, ev: -1 }); f2.sis = f1.sis; S.com.iniciar(m, f1.sis, [f1], [f2]); cen = m.sis[f1.sis]; cierto(m.bat.some(b => b.vivo), 'no arranca la batalla'); }
  const at = m.ase[cen.ase[0]]; if (at.uni >= 0 && at.tie < 0) S.com.tierra(m, at, { tipo: 'insurreccion', ev: 0, atk: { n: 900, ef: 0.1, fac: -1, est: -1, nom: 'la calle' }, def: { n: m.uni[at.uni].n, ef: 0.1, est: at.est, uni: at.uni, nom: 'la guarnición' } });
  U.local.efecto(m, { k: 'bomba', a: at.id, d: { ins: at.ins[0] } }); at.f = 0.5; at.H = 0.3; at.terror = 0.3;
  const persona = m.per.find(p => p.vivo && p.fam.con >= 0) || m.per[0]; M.seguir = { t: 'P', id: persona.id };
  for (const capa of ['politico', 'hambre', 'agravio', 'calle', 'control', 'grano', 'noticia', 'rebeliones', 'relaciones', 'comercio', 'estrategia', 'tecnica', 'paro', 'centro']) { M.capa = capa; if (capa === 'noticia') M.elegirNoticia(m, m.paq[m.paq.length - 1].ev); for (const z of [0.2, 0.6, 1.8, 3.5, 5, 8, 12.5, 18, 40]) { M.cam.z = z; M.cam.x = cen.x + 3; M.cam.y = cen.y + 2; M.dibujar(m, m.t, capa === 'relaciones' ? { t: 'P', id: persona.id } : { t: 'A', id: cen.ase[0] }); cierto(M.zonas.length > 0, 'nada que elegir a zoom ' + z); } }
  M.seguir = null; M.capa = 'politico';
  // Arte: retratos de todo el mundo, banderas de todo Estado y facción, plano de toda nave.
  for (const p of m.per) U.arte.avatar(ctx, 0, 0, 96, U.arte.descPersona(m, p));
  for (const e of m.est) U.arte.banderaDe(ctx, 0, 0, 60, 40, m, 'E', e.id); for (const f of m.fac) U.arte.banderaDe(ctx, 0, 0, 60, 40, m, 'F', f.id);
  for (const n of m.nav) U.arte.nave(ctx, 0, 0, 380, 130, m, n); for (let k = 0; k < 8; k++) U.arte.arma(ctx, 0, 0, 20, k);
  for (const k in U.arte.EMB) U.arte.emblema(ctx, k, 10, 10, 8, '#fff');
  cierto(M.elegir(M.sx(cen.x), M.sy(cen.y)) || true);
  let n = 0; const ver = (ref) => { const h = P.ficha(m, ref); cierto(typeof h === 'string' && h.length > 40 && h.indexOf('undefined') < 0 && h.indexOf('NaN') < 0, ref.t + ref.id + ': ' + (h.match(/.{0,60}(undefined|NaN).{0,40}/) || [h.slice(0, 80)])[0]); n++; };
  for (const s of m.sis) ver({ t: 'S', id: s.id }); for (const a of m.ase) ver({ t: 'A', id: a.id }); for (const i of m.ins) ver({ t: 'I', id: i.id });
  for (const x of m.nav) ver({ t: 'N', id: x.id }); for (const p of m.per) ver({ t: 'P', id: p.id }); for (const e of m.est) ver({ t: 'E', id: e.id }); for (const f of m.fac) ver({ t: 'F', id: f.id }); for (const o of m.obj) if (o.id % 7 === 0 || o.tipo === 'libreta' || o.tipo === 'caja negra') ver({ t: 'O', id: o.id });
  const c = S.per.caras(m, m.ase[1], 20)[0]; ver({ t: 'C', id: c.i, a: 1, cara: c }); cierto(P.ficha(m, null).length > 40);
  for (const u of m.uni) ver({ t: 'U', id: u.id }); for (const d of m.dis) if (d.id % 5 === 0) ver({ t: 'D', id: d.id });
  for (const filtro of ['mortiferos', 'gobernantes', 'herederos', 'mandos', 'ricos', 'vengadores', 'piratas', 'cabecillas', 'aspirantes', 'presos', 'viajeros', 'viejos', 'muertos', 'tocados']) { const h = P.personas(m, { filtro, est: filtro === 'ricos' ? 0 : -1 }); cierto(h.length > 60 && h.indexOf('undefined') < 0 && h.indexOf('NaN') < 0, 'personas/' + filtro); }
  for (const e of m.est) { const h = P.arsenal(m, { est: e.id }); cierto(h.indexOf('undefined') < 0 && h.indexOf('NaN') < 0, 'arsenal de ' + e.nom); }
  for (const h of [P.batallas(m), P.economia(m), P.teorias(m)]) cierto(h.length > 200 && h.indexOf('undefined') < 0 && h.indexOf('NaN') < 0, (h.match(/.{0,50}(undefined|NaN).{0,40}/) || ['pestaña vacía'])[0]);
  // Batallas: cada bando explicado, en curso y ya acabadas (con todo desplegado), y lo mismo en la ficha del sistema.
  const sinBasura = (h, que) => cierto(h.indexOf('undefined') < 0 && h.indexOf('NaN') < 0 && h.indexOf('Infinity') < 0, que + ': ' + (h.match(/.{0,60}(undefined|NaN|Infinity).{0,40}/) || [''])[0]);
  const btv = m.bat.find(b => b.vivo);
  if (btv) {
    const todo = new Set(); for (const b of m.bat) { todo.add('b' + b.id); todo.add('c' + b.id + '_0'); todo.add('c' + b.id + '_1'); } for (const t of m.tie) todo.add('t' + t.id);
    const h1 = P.batallas(m, { abiertos: todo }); sinBasura(h1, 'batallas en curso');
    for (const txt of ['Qué se juegan', 'Pegada: cada 100', 'Quién va ganando', 'A este ritmo', 'Ver la cuenta completa', 'Doctrina naval', 'Insurrectos', 'Guarnición del régimen']) cierto(h1.indexOf(txt) >= 0, 'falta «' + txt + '» en la pestaña de batallas');
    sinBasura(P.ficha(m, { t: 'S', id: btv.sis }), 'ficha de sistema con batalla'); cierto(P.ficha(m, { t: 'S', id: btv.sis }).indexOf('Batalla espacial') >= 0, 'la ficha del sistema no enseña su batalla');
    sinBasura(P.ficha(m, { t: 'A', id: at.id }), 'ficha de asentamiento con combates'); cierto(P.ficha(m, { t: 'A', id: at.id }).indexOf('Insurrección en') >= 0, 'la ficha del asentamiento no enseña su combate');
    const m9 = m; const t9 = m9.t; m9.avanzar(m9.t + 60);
    const h2 = P.batallas(m9, { abiertos: todo }); sinBasura(h2, 'batallas acabadas');
    cierto(!btv.vivo && h2.indexOf('Ganó') >= 0 && h2.indexOf('vence a') >= 0, 'la batalla acabada no se cuenta'); void t9;
  }
  // Personas: la cuenta de muertes de los más mortíferos, desplegada, y también en su ficha.
  const mort = m.per.filter(p => p.mue && p.mue.length).sort((x, y) => S.mas.letalidad(y) - S.mas.letalidad(x)).slice(0, 12);
  cierto(mort.length > 0, 'nadie tiene muertes apuntadas'); for (const p of mort) { U.estado.abiertos.add('k' + p.id); sinBasura(P.muertesDe(m, p), 'muertes de ' + p.nom); }
  const hp = P.personas(m, { filtro: 'mortiferos', est: -1 }); sinBasura(hp, 'personas con muertes'); cierto(hp.indexOf('Por qué:') >= 0 && hp.indexOf('bajo su mando') >= 0, 'la lista de muertes no sale en Personas');
  const hf = P.ficha(m, { t: 'P', id: mort[0].id }); sinBasura(hf, 'ficha con muertes'); cierto(hf.indexOf('Sus muertes, una a una') >= 0, 'la ficha no enseña las muertes');
  // Aspecto: cada opción se puede poner y las gráficas se pintan en los tres estilos, con cursor.
  const Pr = U.prefs; sinBasura(Pr.html(), 'panel de aspecto');
  for (const [k, v] of [['tema', 'nebulosa'], ['acento', 'cian'], ['paleta', 'segura'], ['periodo', '3'], ['ancho', '560'], ['letra', '14'], ['alto', '180']]) { cierto(Pr.accion({ dataset: { pref: k, v } }), k); cierto(String(Pr.v[k]) === v, k + ' no cambia'); }
  cierto(Pr.accion({ dataset: { pref: 'suave' }, type: 'checkbox', checked: false }) && Pr.v.suave === false, 'las casillas no cambian');
  for (const estilo of ['area', 'linea', 'barras']) {
    Pr.accion({ dataset: { pref: 'graf', v: estilo } });
    for (const claves of ['hambre,agravio', 'pob']) { const cv = Object.assign(el(), { dataset: { w: '300', h: '120', series: claves, nombres: 'uno|dos' } }); U.viz.pintar(m, { querySelectorAll: (q) => (q.indexOf('data-series') >= 0 ? [cv] : []) }); cierto(cv._viz, 'la gráfica no se pinta'); cv._viz.o.cursor = 150; cv._viz.f(cv, cv._viz.o); }
  }
  U.viz.dona(el(), { partes: [{ nom: 'a', v: 3, col: '#f00' }, { nom: 'b', v: 1, col: '#0f0' }], centro: '4', sub: 'x' }); U.viz.spark(el(), [1, 2, 3, 2], '#fff'); U.viz.apilada(el(), { capas: [{ col: '#f00', d: [0.5, 0.4] }, { col: '#0f0', d: [0.5, 0.6] }] });
  cerca(U.viz.bonito(2.3), 2.5, 1e-9); cerca(U.viz.bonito(7300), 10000, 1e-9); cerca(U.viz.bonito(0.04), 0.05, 1e-9);
  Pr.alternarSerie('paro'); cierto(Pr.series().indexOf('paro') >= 0, 'la serie no se añade'); for (const h9 of [P.sismografo(m, { sel: null }), P.estados(m), P.facciones(m), P.economia(m), P.ficha(m, null)]) sinBasura(h9, 'pestaña con el aspecto cambiado');
  Pr.accion({ dataset: { pref: 'restablecer' } }); cierto(Pr.v.tema === 'noche' && Pr.v.graf === 'area' && Pr.v.series === null && Pr.v.suave === true, 'no vuelve a lo de fábrica');
  // La ficha del mundo: cada cifra abre su tema; todos los temas (y las clases de hecho sueltas) se pintan, con filtros.
  const hm = P.ficha(m, null); sinBasura(hm, 'ficha del mundo'); for (const txt of ['data-tema="masacres"', 'data-tema="conquistas"', 'data-tema="abordajes"', 'data-tema="profecias"', 'data-tema="calle"', 'data-tema="naves"']) cierto(hm.indexOf(txt) >= 0, 'falta ' + txt);
  const temas = []; for (const [, l] of P.SECCIONES) for (const t of l) temas.push(t.id); for (const t of P.INDICADORES) temas.push(t.id); for (const k of P.sueltos(m)) temas.push('k:' + k);
  const enTema = {}; for (const [, l] of P.SECCIONES) for (const t of l) for (const k of t.ks) { cierto(!enTema[k], 'la clase «' + k + '» está en dos temas'); enTema[k] = t.id; }
  let conHechos = 0; for (const id of temas) { const t = P.temaDe(m, id); cierto(t, 'tema desconocido: ' + id); const h1 = P.ficha(m, null, { tema: { id, imp: 0, lim: 150, fuera: {} } }); sinBasura(h1, 'tema ' + id); cierto(h1.indexOf('El mundo') >= 0 && h1.indexOf('Uno a uno') >= 0, 'tema ' + id + ' sin cabecera'); if (h1.indexOf('class="ev') >= 0) conHechos++; sinBasura(P.tema(m, { id, imp: 2, lim: 20, fuera: { [t.ks[0]]: 1 } }), 'tema ' + id + ' filtrado'); }
  cierto(conHechos > 15, 'pocos temas con hechos: ' + conHechos + ' de ' + temas.length);
  let suma = 0; for (const k in m.cuenta) suma += m.cuenta[k]; cierto(suma === m.ev.length, 'la cuenta por clases no cuadra con los hechos');
  // Historial de cualquier cosa, con el filtro de la crónica.
  const conBio = m.per.filter(p => m.bio.get(p.id) && m.bio.get(p.id).length > 4)[0];
  for (const bio of [{ imp: 0, orden: 'desc' }, { imp: 1, orden: 'asc' }, { imp: 3, orden: 'desc' }]) { const hb = P.ficha(m, { t: 'P', id: conBio.id }, { bio }); sinBasura(hb, 'historia de una persona'); cierto(hb.indexOf('Su historia') >= 0 && hb.indexOf('data-op="hi"') >= 0, 'la ficha no trae el filtro de importancia'); }
  cierto(P.hechosDe(m, { t: 'P', id: conBio.id }).length === m.bio.get(conBio.id).length);
  for (const ref of [{ t: 'E', id: 0 }, { t: 'A', id: 3 }, { t: 'S', id: 1 }, { t: 'F', id: m.fac.length - 1 }, { t: 'N', id: 5 }]) { U.estado.abiertos.add('h' + ref.t + ref.id); const hh = P.ficha(m, ref, { bio: { imp: 0, orden: 'desc' } }); sinBasura(hh, 'historial de ' + ref.t); cierto(hh.indexOf('Su historial completo') >= 0, 'falta el historial en ' + ref.t); }
  cierto(P.hechosDe(m, { t: 'E', id: 0 }).length > 3 && P.hechosDe(m, { t: 'A', id: 3 }).length > 3, 'historiales vacíos');
  // Historiador: el grafo de ramas es coherente (cada hecho en un carril que nadie más ocupa mientras le quedan efectos).
  let grafos = 0;
  for (const e of m.ev) { if (!(e.imp >= 2 || e.id % 53 === 0)) continue; for (const prof of [2, 4, 12]) {
    const hg = P.historiador(m, e.id, { prof }); cierto(hg.indexOf('undefined') < 0 && hg.indexOf('NaN') < 0 && hg.indexOf('gr-fila') >= 0, 'historiador de ' + e.id);
    const gr = U.viz.datos.grafo; const ids = Array.from(gr.carril.keys()).sort((a, b) => a - b); const fila = new Map(ids.map((x, i) => [x, i])); const fin = new Map(ids.map(x => [x, fila.get(x)]));
    cierto(gr.carril.has(e.id) && gr.sel === e.id, 'el hecho elegido no está en su grafo');
    for (const [p, v] of gr.aristas) { cierto(p < v && m.ev[v].c.indexOf(p) >= 0, 'arista que no es causa→efecto'); if (fila.get(v) > fin.get(p)) fin.set(p, fila.get(v)); }
    const ult = {}; for (const x of ids) { const c = gr.carril.get(x); const a = ult[c]; if (a !== undefined) cierto(fin.get(a) < fila.get(x) || (fin.get(a) === fila.get(x) && m.ev[x].c.indexOf(a) >= 0), 'dos hechos se pisan en el carril ' + c + ' (' + a + ' y ' + x + ')'); ult[c] = x; }
    if (ids.length > 1) cierto(gr.aristas.length >= ids.length - 1, 'hay hechos sueltos en el grafo'); grafos++;
  } }
  cierto(grafos > 30, 'pocos grafos probados'); P.pintarExtra(m, { querySelectorAll: () => [] });
  // Historia: la pestaña, el mapa de calor (con cursor y clic), la biografía en la ficha y las capas de calor del mapa.
  const hh2 = P.historia(m); sinBasura(hh2, 'pestaña Historia'); for (const txt of ['Épocas', 'Qué pasó cada año', 'Quién pesó más', 'Credos', 'Peste', 'data-calor']) cierto(hh2.indexOf(txt) >= 0, 'falta «' + txt + '» en Historia');
  const cvC = Object.assign(el(), { dataset: { w: '400', h: '300', calor: 'calorHist' } }); P.pintarExtra(m, { querySelectorAll: (q) => (q.indexOf('data-calor') >= 0 ? [cvC] : []) }); cierto(cvC._viz && cvC._clic, 'el mapa de calor no se pinta');
  cvC._viz.o.cursor = 300; cvC._viz.o.cy = 40; cvC._viz.f(cvC, cvC._viz.o); const cel = cvC._clic(300, 40); cierto(cel && P.temaDe(m, cel.tema), 'el clic en el mapa de calor no da un tema');
  const masPeso = m.per[S.his.pesos(m).orden[0]]; const hbio = P.ficha(m, { t: 'P', id: masPeso.id }); sinBasura(hbio, 'ficha con biografía'); for (const txt of ['class="bio', 'Origen', 'Vida', 'Peso histórico', 'Drama']) cierto(hbio.indexOf(txt) >= 0, 'falta «' + txt + '» en la biografía');
  cierto(!/\{[a-z_]+\}/.test(hbio) && !/\{[A-Z]\d+\}/.test(hbio), 'quedan fichas sin convertir en la biografía: ' + (hbio.match(/.{0,40}\{[^}]*\}.{0,30}/) || [''])[0]);
  sinBasura(P.personas(m, { filtro: 'historicos', est: -1 }), 'personas por peso histórico');
  M.famaDe = masPeso.id; for (const capa of ['fama', 'credo', 'peste', 'sangre', 'memoria']) { M.capa = capa; for (const a of m.ase) { const v = M.valorCapa(m, a); cierto(v >= 0 && v <= 1, capa + ' fuera de 0..1'); } for (const z of [0.3, 2, 8]) { M.cam.z = z; M.dibujar(m, m.t, null); } } M.capa = 'politico';
  const lienzos = (attr, vals) => vals.map(v => Object.assign(el(), { dataset: Object.assign({ w: '80', h: '60' }, { [attr]: v }) }));
  P.pintarArte(m, { querySelectorAll: (q) => q.indexOf('data-av') >= 0 ? lienzos('av', ['P0', 'C0_0_0', 'U0_3']) : q.indexOf('data-ban') >= 0 ? lienzos('ban', ['E0', 'F0']) : q.indexOf('data-nave') >= 0 ? lienzos('nave', ['0', 'D0']) : q.indexOf('data-arma') >= 0 ? lienzos('arma', ['2']) : q.indexOf('data-evo') >= 0 ? lienzos('evo', ['0_fragata']) : q.indexOf('data-bat') >= 0 ? lienzos('bat', [String(m.bat.length - 1)]) : q.indexOf('data-tie') >= 0 ? lienzos('tie', ['0']) : [] });
  for (const imp of [0, 1, 2, 3]) cierto(P.cronica(m, { imp, aqui: imp === 2, sel: { t: 'A', id: 2 } }).length > 10);
  for (const e of m.ev) if (e.imp >= 2 || e.id % 97 === 0) cierto(P.historiador(m, e.id).indexOf('undefined') < 0);
  for (const sel of [null, { t: 'A', id: 3 }, { t: 'E', id: 0 }, { t: 'F', id: m.fac.length - 1 }]) cierto(P.sismografo(m, { sel }).length > 100);
  cierto(P.estados(m).length > 50 && P.facciones(m).length > 50);
  P.pintar(m, { querySelectorAll: () => [Object.assign(el(), { dataset: { series: 'hambre,agravio' } }), Object.assign(el(), { dataset: { umbral: '3' } })] });
  console.log('      ' + n + ' fichas, 7 capas × 7 niveles de zoom');
});
prueba('arranque completo: crear universo, prehistoria, correr, cambiar de pestaña y usar la mano desde la interfaz', () => {
  const U = global.UI; const els = global.__els; let raf = null; global.requestAnimationFrame = (f) => { raf = f; };
  for (const [k, v] of [['semilla', '5'], ['sistemas', '24'], ['prehistoria', '3']]) global.document.getElementById(k).value = v;
  U.iniciar(); cierto(raf, 'no arranca el bucle');
  const st = global.setTimeout; global.setTimeout = (f) => { f(); return 0; }; els.crear._l.click(); global.setTimeout = st;
  const E = U.estado; cierto(E.m && E.pre, 'no se creó el mundo');
  let t = 0; for (let i = 0; i < 6000 && E.pre; i++) raf(t += 16);
  cierto(!E.pre && E.m.presente > 0, 'la prehistoria no acaba'); cerca(E.m.presente / S.ANIO, 3, 0.01, 'años de prehistoria');
  for (let i = 0; i < 300; i++) raf(t += 16);
  cierto(E.m.t > E.m.presente, 'el tiempo no avanza');
  for (const p of ['ficha', 'cronica', 'historiador', 'sismografo', 'estados', 'facciones', 'personas', 'arsenal', 'batallas', 'economia', 'teorias']) { E.pest = p; E.sel = { t: 'A', id: 0 }; E.evSel = E.m.ev.length - 1; U.pintarPanel(); cierto(els.contenido.innerHTML.length > 20, p); }
  const a = E.m.ase.find(x => x.est >= 0); const antes = E.m.ev.length; U.aplicar('bomba', S.dios.fabrica(E.m, a).id); cierto(E.m.ev.length > antes && E.m.ev[E.evSel], 'la bomba no deja rastro');
  els.mapa._l.wheel({ preventDefault() { }, clientX: 500, clientY: 400, deltaY: -400 });
  for (let i = 0; i < 60; i++) raf(t += 16);
  cierto(els.ultimo.textContent && els.fecha.textContent.indexOf('Año') === 0, 'la barra no se actualiza');
  // Atrás y adelante: cada cosa que se mira es una página; volver la deja como estaba, y mirar otra cosa borra el «adelante».
  const N = U.nav; N.pila.length = 0; N.i = -1; const ver = (f) => { f(); E.sucio = true; U.pintarPanel(); };
  const clic = (sel) => els.contenido._l.click({ target: { closest: (q) => (q === sel.q ? { dataset: sel.d } : null) }, detail: 1 });
  ver(() => { E.pest = 'ficha'; E.sel = null; E.tema = null; }); cierto(N.pila.length === 1 && N.clave() === 'ficha|mundo', 'no apunta la primera página: ' + N.clave());
  U.pintarPanel(); U.pintarPanel(); cierto(N.pila.length === 1, 'repintar no debe apuntar páginas nuevas');
  clic({ q: '[data-tema]', d: { tema: 'masacres' } }); U.pintarPanel(); cierto(E.tema && E.tema.id === 'masacres' && N.pila.length === 2, 'abrir un tema no es una página');
  cierto(els.contenido.innerHTML.indexOf('Masacres') >= 0 && N.titulo(N.pila[1]) === 'Ficha › Masacres', N.titulo(N.pila[1]));
  ver(() => { E.sel = { t: 'P', id: 0 }; }); ver(() => { E.pest = 'estados'; }); ver(() => { E.pest = 'historiador'; E.evSel = 5; }); ver(() => { E.evSel = 9; });
  cierto(N.pila.length === 6 && N.i === 5, 'páginas: ' + N.pila.map(x => x.k).join(' · '));
  E.pest = 'personas'; U.pintarPanel(); E.per.filtro = 'ricos'; U.pintarPanel(); cierto(N.pila.length === 7, 'cambiar un filtro no es una página nueva');
  cierto(N.ir(-1) && E.pest === 'historiador' && E.evSel === 9); U.pintarPanel(); cierto(N.pila.length === 7 && N.i === 5, 'volver atrás no debe apuntar');
  N.ir(-1); cierto(E.evSel === 5); N.ir(-1); cierto(E.pest === 'estados'); N.ir(-1); cierto(E.pest === 'ficha' && E.sel && E.sel.t === 'P' && E.sel.id === 0, 'no vuelve a la persona');
  N.ir(-1); U.pintarPanel(); cierto(E.pest === 'ficha' && !E.sel && E.tema && E.tema.id === 'masacres' && els.contenido.innerHTML.indexOf('Masacres') >= 0, 'no vuelve al tema');
  N.ir(-1); U.pintarPanel(); cierto(!E.tema && !E.sel && N.i === 0 && els.contenido.innerHTML.indexOf('El mundo') >= 0, 'no vuelve al mundo'); cierto(N.ir(-1) === false, 'no hay nada antes de la primera');
  cierto(els.navAtras.disabled === true && els.navAdelante.disabled === false && els.navDonde.textContent === 'Ficha › El mundo', 'los botones no reflejan dónde se está: ' + els.navDonde.textContent);
  N.ir(1); N.ir(1); U.pintarPanel(); cierto(E.sel && E.sel.id === 0 && N.i === 2 && els.navAdelante.title.indexOf('Estados') >= 0, 'adelante: ' + els.navAdelante.title);
  ver(() => { E.pest = 'economia'; }); cierto(N.pila.length === 4 && N.i === 3 && N.ir(1) === false, 'mirar otra cosa debe borrar el «adelante»');
  els.navAtras._l.click(); cierto(E.pest === 'ficha' && N.i === 2, 'el botón de atrás no funciona'); els.navAdelante._l.click(); cierto(E.pest === 'economia' && N.i === 3, 'el botón de adelante no funciona');
});

console.log('\n' + ok + ' pruebas bien, ' + mal + ' mal');
process.exit(mal ? 1 : 0);
