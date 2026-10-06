// Base del núcleo: espacio de nombres, azar determinista, cola de eventos, matemáticas y registros.
// Nada de aquí toca el DOM: el núcleo corre igual en el navegador y en Node (pruebas).
(function (g) {
  'use strict';
  const S = g.SIM = g.SIM || {};
  const rotl = (x, k) => (x << k) | (x >>> (32 - k));

  // hash(semilla_mundo, id_grupo, índice…): todo lo generado sale de aquí, así se repite exacto.
  S.hash = function () {
    let h = 0x811c9dc5;
    for (let i = 0; i < arguments.length; i++) {
      const x = arguments[i];
      if (typeof x === 'string') {
        for (let j = 0; j < x.length; j++) { h ^= x.charCodeAt(j); h = Math.imul(h, 0x01000193); }
      } else {
        let v = x | 0;
        for (let k = 0; k < 4; k++) { h ^= v & 255; h = Math.imul(h, 0x01000193); v >>>= 8; }
      }
      h ^= 0x5f; h = Math.imul(h, 0x01000193);
    }
    h ^= h >>> 16; h = Math.imul(h, 0x85ebca6b); h ^= h >>> 13; h = Math.imul(h, 0xc2b2ae35); h ^= h >>> 16;
    return h >>> 0;
  };

  // xoshiro128**: un generador por entidad.
  class Rng {
    constructor(seed) {
      let s = seed >>> 0;
      const sm = () => { s = (s + 0x9e3779b9) | 0; let z = s; z = Math.imul(z ^ (z >>> 16), 0x85ebca6b); z = Math.imul(z ^ (z >>> 13), 0xc2b2ae35); return (z ^ (z >>> 16)) >>> 0; };
      this.a = sm(); this.b = sm(); this.c = sm(); this.d = sm();
    }
    u32() {
      const r = Math.imul(rotl(Math.imul(this.b, 5), 7), 9) >>> 0;
      const t = this.b << 9;
      this.c ^= this.a; this.d ^= this.b; this.b ^= this.c; this.a ^= this.d; this.c ^= t; this.d = rotl(this.d, 11);
      return r;
    }
    f() { return this.u32() / 4294967296; }
    r(a, b) { return a + (b - a) * this.f(); }
    i(n) { return Math.floor(this.f() * n); }
    p(prob) { return this.f() < prob; }
    n(mu, sd) {
      let u = 0; while (u < 1e-12) u = this.f();
      const v = this.f();
      return (mu || 0) + (sd === undefined ? 1 : sd) * Math.sqrt(-2 * Math.log(u)) * Math.cos(6.283185307179586 * v);
    }
    ln(mu, sd) { return Math.exp(this.n(mu, sd)); }
    el(arr) { return arr[this.i(arr.length)]; }
    pesos(ws) {
      let s = 0; for (let i = 0; i < ws.length; i++) s += ws[i];
      if (!(s > 0)) return this.i(ws.length);
      let x = this.f() * s;
      for (let i = 0; i < ws.length; i++) { x -= ws[i]; if (x <= 0) return i; }
      return ws.length - 1;
    }
    // P(a) = e^(U/T) / Σ e^(U/T)
    soft(us, T) {
      let mx = -Infinity; for (let i = 0; i < us.length; i++) if (us[i] > mx) mx = us[i];
      const ws = new Array(us.length);
      for (let i = 0; i < us.length; i++) ws[i] = Math.exp((us[i] - mx) / T);
      return this.pesos(ws);
    }
    bar(arr) { for (let i = arr.length - 1; i > 0; i--) { const j = this.i(i + 1); const t = arr[i]; arr[i] = arr[j]; arr[j] = t; } return arr; }
    t01(mu, sd) { return S.clamp(this.n(mu === undefined ? 0.5 : mu, sd === undefined ? 0.22 : sd), 0, 1); }
  }
  S.Rng = Rng;

  // Montículo binario ordenado por (t, s). El desempate por s hace el orden determinista.
  const men = (a, b) => a.t < b.t || (a.t === b.t && a.s < b.s);
  class Cola {
    constructor() { this.h = []; }
    get n() { return this.h.length; }
    peek() { return this.h[0]; }
    push(e) {
      const h = this.h; let i = h.length; h.push(e);
      while (i > 0) { const p = (i - 1) >> 1; if (men(h[i], h[p])) { const t = h[i]; h[i] = h[p]; h[p] = t; i = p; } else break; }
    }
    pop() {
      const h = this.h; const top = h[0]; const last = h.pop();
      if (h.length) {
        h[0] = last; let i = 0; const n = h.length;
        for (;;) {
          const l = 2 * i + 1, r = l + 1; let k = i;
          if (l < n && men(h[l], h[k])) k = l;
          if (r < n && men(h[r], h[k])) k = r;
          if (k === i) break;
          const t = h[i]; h[i] = h[k]; h[k] = t; i = k;
        }
      }
      return top;
    }
  }
  S.Cola = Cola;

  S.clamp = (x, a, b) => x < a ? a : x > b ? b : x;
  S.lerp = (a, b, t) => a + (b - a) * t;
  S.sig = (x) => 1 / (1 + Math.exp(-x));
  S.cdfN = function (x) { // Φ(x), Abramowitz–Stegun 7.1.26
    const s = x < 0 ? -1 : 1; const z = Math.abs(x) / Math.SQRT2;
    const t = 1 / (1 + 0.3275911 * z);
    const y = 1 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-z * z);
    return 0.5 * (1 + s * y);
  };
  S.sim5 = function (a, b) { // similitud de ideologías en 0..1 (vectores de 5 ejes en -1..1)
    let d = 0; for (let i = 0; i < 5; i++) { const x = a[i] - b[i]; d += x * x; }
    return 1 - Math.sqrt(d / 5) / 2;
  };
  S.dist5 = (a, b) => (1 - S.sim5(a, b)) * 2;

  // Autovalores y autovectores de una matriz simétrica (Jacobi cíclico). Para el vector de Fiedler.
  S.eigSim = function (A) {
    const n = A.length; const a = A.map(r => r.slice());
    const v = []; for (let i = 0; i < n; i++) { v.push(new Array(n).fill(0)); v[i][i] = 1; }
    for (let sw = 0; sw < 80; sw++) {
      let off = 0; for (let p = 0; p < n; p++) for (let q = p + 1; q < n; q++) off += a[p][q] * a[p][q];
      if (off < 1e-20) break;
      for (let p = 0; p < n; p++) for (let q = p + 1; q < n; q++) {
        if (Math.abs(a[p][q]) < 1e-15) continue;
        const th = (a[q][q] - a[p][p]) / (2 * a[p][q]);
        const t = (th >= 0 ? 1 : -1) / (Math.abs(th) + Math.sqrt(th * th + 1));
        const c = 1 / Math.sqrt(t * t + 1), s = t * c;
        for (let k = 0; k < n; k++) { const x = a[k][p], y = a[k][q]; a[k][p] = c * x - s * y; a[k][q] = s * x + c * y; }
        for (let k = 0; k < n; k++) { const x = a[p][k], y = a[q][k]; a[p][k] = c * x - s * y; a[q][k] = s * x + c * y; }
        for (let k = 0; k < n; k++) { const x = v[k][p], y = v[k][q]; v[k][p] = c * x - s * y; v[k][q] = s * x + c * y; }
      }
    }
    const idx = []; for (let i = 0; i < n; i++) idx.push(i);
    idx.sort((i, j) => a[i][i] - a[j][j]);
    return { val: idx.map(i => a[i][i]), vec: idx.map(i => v.map(r => r[i])) };
  };
  // λ₂ y vector de Fiedler de un grafo con pesos W (matriz simétrica, diagonal ignorada).
  S.fiedler = function (W) {
    const n = W.length; const L = [];
    for (let i = 0; i < n; i++) { L.push(new Array(n).fill(0)); let d = 0; for (let j = 0; j < n; j++) if (j !== i) { d += W[i][j]; L[i][j] = -W[i][j]; } L[i][i] = d; }
    const e = S.eigSim(L);
    return { l2: n > 1 ? e.val[1] : 0, vec: n > 1 ? e.vec[1] : [0] };
  };

  // Registros de datos (declaraciones), manejadores de eventos y ganchos entre módulos.
  S.reg = {};
  S.def = function (tipo, id, o) { (S.reg[tipo] = S.reg[tipo] || {})[id] = o; o.id = id; return o; };
  S.man = {};
  S.en = function (tipo, fn) { S.man[tipo] = fn; };
  S.gan = {};
  S.gancho = function (n, fn) { (S.gan[n] = S.gan[n] || []).push(fn); };
  S.emitir = function (m, n, a, b, c, d, e) { const l = S.gan[n]; if (l) for (let i = 0; i < l.length; i++) l[i](m, a, b, c, d, e); };

  S.ANIO = 360; S.MES = 30;
  // Rasgos de un personaje (8 valores 0–1).
  S.R = { COD: 0, VAL: 1, LEA: 2, EMP: 3, REN: 4, AMB: 5, FE: 6, PRU: 7 };
  S.RNOM = ['codicia', 'valor', 'lealtad', 'empatía', 'rencor', 'ambición', 'fe', 'prudencia'];
  S.IDEO = ['autoridad–libertad', 'orden–cambio', 'fe–razón', 'colectivo–individuo', 'cierre–apertura'];

  const UNI = ['cero', 'un', 'dos', 'tres', 'cuatro', 'cinco', 'seis', 'siete', 'ocho', 'nueve', 'diez', 'once', 'doce', 'trece', 'catorce', 'quince', 'dieciséis', 'diecisiete', 'dieciocho', 'diecinueve', 'veinte'];
  const DEC = ['', '', 'veinti', 'treinta', 'cuarenta', 'cincuenta', 'sesenta', 'setenta', 'ochenta', 'noventa'];
  S.numPal = function (n) {
    n = Math.round(n);
    if (n <= 20) return UNI[n];
    if (n < 30) return 'veinti' + UNI[n - 20];
    if (n < 100) return DEC[Math.floor(n / 10)] + (n % 10 ? ' y ' + UNI[n % 10] : '');
    if (n === 100) return 'cien';
    return String(n);
  };
  S.cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);
  S.fmt = function (x) {
    const a = Math.abs(x);
    if (a >= 1e6) return (x / 1e6).toFixed(a >= 1e7 ? 1 : 2) + ' M';
    if (a >= 1e4) return Math.round(x / 1e3) + ' k';
    if (a >= 100) return String(Math.round(x));
    if (a >= 10) return x.toFixed(1);
    return x.toFixed(2);
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
