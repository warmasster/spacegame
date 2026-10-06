// El mundo: tablas de entidades, línea de tiempo (cola de eventos), registro de hechos con sus causas
// (el historiador) y series temporales (el sismógrafo).
(function (g) {
  'use strict';
  const S = g.SIM;
  const ahora = () => (typeof performance !== 'undefined' ? performance.now() : Date.now());

  class Mundo {
    constructor(semilla) {
      this.semilla = semilla >>> 0;
      this.t = 0; this.seq = 0; this.cola = new S.Cola();
      // sistemas, asentamientos, instalaciones, cohortes, unidades, estados, facciones, personas, naves,
      // objetos únicos, flotas, batallas, casos policiales, paquetes de información, deudas.
      this.sis = []; this.ase = []; this.ins = []; this.coh = []; this.uni = []; this.est = []; this.fac = [];
      this.per = []; this.nav = []; this.obj = []; this.flo = []; this.bat = []; this.cas = []; this.paq = []; this.deu = [];
      this.ev = [];       // registro de solo añadir: {id,t,k,txt,c:[causas],a,s,imp,d}
      this.efe = [];      // índice inverso: efectos de cada hecho
      this.nuevos = [];   // hechos recientes con importancia (los consume la interfaz)
      this.cuenta = {};   // cuántos hechos de cada tipo
      this.series = {};   // sismógrafo
      this.rng = new S.Rng(S.hash(this.semilla, 'mundo'));
      this.dist = null; this.sig = null; this.cartas = 0; this.nEv = 0;
      this.presente = 0;  // instante en que acabó la prehistoria
      this.dios = [];     // acciones del jugador, con fecha (entrada del determinismo)
      this.tie = []; this.dis = []; this.bio = new Map(); this.flu = new Map();   // combates en tierra, diseños, biografías, flujos de comercio
    }
    rngDe(tipo, id) { return new S.Rng(S.hash(this.semilla, tipo, id)); }

    // «Si puedes calcular cuándo va a pasar algo, no compruebes si ha pasado»: se programa.
    prog(t, tipo, d) {
      const e = d || {};
      e.t = t < this.t ? this.t : t; e.s = this.seq++; e.k = tipo;
      this.cola.push(e); return e;
    }
    // Avanza hasta `hasta`. Con presupuesto en ms devuelve false si no ha llegado (el resto espera al siguiente frame).
    avanzar(hasta, msMax) {
      const c = this.cola; const t0 = msMax ? ahora() : 0; let n = 0;
      while (c.n && c.peek().t <= hasta) {
        const e = c.pop();
        if (e.x) continue;
        this.t = e.t; this.nEv++;
        const f = S.man[e.k]; if (f) f(this, e);
        if (msMax && (++n & 127) === 0 && ahora() - t0 > msMax) return false;
      }
      this.t = hasta; return true;
    }

    // Registra un hecho con sus causas. Devuelve su id para enlazarlo como causa de otros.
    reg(k, txt, o) {
      o = o || {};
      const c = [];
      if (o.c) for (let i = 0; i < o.c.length; i++) { const x = o.c[i]; if (x !== undefined && x !== null && x >= 0 && c.indexOf(x) < 0) c.push(x); }
      const a = o.a === undefined ? -1 : o.a;
      const e = { id: this.ev.length, t: this.t, k, txt, c, a, s: o.s !== undefined ? o.s : (a >= 0 ? this.ase[a].sis : -1), imp: o.imp || 0, d: o.d || null };
      this.ev.push(e); this.efe.push(null);
      for (let i = 0; i < c.length; i++) { (this.efe[c[i]] = this.efe[c[i]] || []).push(e.id); }
      this.cuenta[k] = (this.cuenta[k] || 0) + 1;
      if (e.imp >= 1) { this.nuevos.push(e.id); if (this.nuevos.length > 400) this.nuevos.splice(0, 200); }
      S.emitir(this, 'hecho', e);
      return e.id;
    }

    fecha(t) {
      if (t === undefined) t = this.t;
      const a = Math.floor(t / S.ANIO); const d = Math.floor(t - a * S.ANIO) + 1;
      return 'Año ' + a + ', día ' + d;
    }
    anio(t) { return Math.floor((t === undefined ? this.t : t) / S.ANIO); }

    // Nombre de una referencia {A3}, {P12}, {N7}…
    nom(tipo, id) {
      switch (tipo) {
        case 'A': return this.ase[id] ? this.ase[id].nom : '?';
        case 'S': return this.sis[id] ? this.sis[id].nom : '?';
        case 'P': return this.per[id] ? this.per[id].nom : '?';
        case 'N': return this.nav[id] ? this.nav[id].nom : '?';
        case 'E': return this.est[id] ? this.est[id].nom : '?';
        case 'F': return this.fac[id] ? this.fac[id].nom : '?';
        case 'O': return this.obj[id] ? S.obj.nombre(this, this.obj[id]) : '?';
        case 'I': { const i = this.ins[id]; return i ? i.nom : '?'; }
      }
      return '?';
    }
    texto(e) {
      const m = this;
      return (typeof e === 'number' ? this.ev[e] : e).txt.replace(/\{([ASPNEFOI])(\d+)\}/g, (_, t, id) => m.nom(t, +id));
    }

    // Cadena de causas hacia atrás (para el historiador y las pruebas).
    cadena(id, max) {
      const out = []; const visto = new Set(); const pila = [[id, 0]];
      while (pila.length && out.length < (max || 200)) {
        const [x, n] = pila.pop(); if (visto.has(x)) continue; visto.add(x);
        out.push({ id: x, n });
        const c = this.ev[x].c; for (let i = c.length - 1; i >= 0; i--) pila.push([c[i], n + 1]);
      }
      return out;
    }

    // Las series que nacen tarde (una facción, un estado nuevo) se rellenan por delante para ir alineadas con 't'.
    serie(clave, v) {
      let s = this.series[clave];
      if (!s) { s = this.series[clave] = []; const n = clave === 't' ? 0 : (this.series.t ? this.series.t.length - 1 : 0); for (let i = 0; i < n; i++) s.push(NaN); }
      s.push(v);
    }
  }
  S.Mundo = Mundo;

  // Sismógrafo: muestreadores declarados por cada módulo; se leen cada mes.
  S.sismo = [];
  S.sismografo = function (clave, nom, fn) { S.sismo.push({ clave, nom, fn }); };
  S.en('sismo', function (m) {
    m.serie('t', m.t);
    for (let i = 0; i < S.sismo.length; i++) m.serie(S.sismo[i].clave, S.sismo[i].fn(m));
    S.emitir(m, 'sismo');
    m.prog(m.t + S.MES, 'sismo');
  });
  S.en('anio', function (m) { S.emitir(m, 'anio'); m.prog(m.t + S.ANIO, 'anio'); });
})(typeof globalThis !== 'undefined' ? globalThis : this);
