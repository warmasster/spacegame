// Arranque, bucle y entrada. La simulación corre con un presupuesto de milisegundos por fotograma:
// si no llega, el trabajo pendiente espera al siguiente y el mundo va un poco tarde.
(function (g) {
  'use strict';
  const S = g.SIM; const U = g.UI; const M = U.mapa; const P = U.paneles; const D = S.dios;
  const $ = (id) => document.getElementById(id);
  const E = U.estado = { m: null, vel: 6, velAntes: 6, sel: null, pest: 'ficha', evSel: null, herr: null, cron: { imp: 1, aqui: false }, per: { filtro: 'historicos', est: -1 }, ars: { est: -1 }, seguir: null, visto: -1, objetivo: 0, sucio: true, tPanel: 0, pre: null, mano: false, abiertos: new Set(), tema: null, bio: { imp: 0, orden: 'desc' }, hist: { prof: 4 } };
  const PRESUPUESTO = 13;

  function aviso(txt, ms) { $('aviso').textContent = txt || ''; if (ms) { clearTimeout(aviso.t); aviso.t = setTimeout(() => { if (!E.herr) $('aviso').textContent = ''; else avisoHerr(); }, ms); } }
  function avisoHerr() { const d = S.reg.dios[E.herr]; $('aviso').textContent = d ? d.ico + ' ' + d.nom + ' — haz clic en el mapa (Esc para soltar)' : ''; }

  // ── Paneles.
  function pintarPanel() {
    const m = E.m; const c = $('contenido'); if (!m) return;
    const sc = E.scPend !== null && E.scPend !== undefined ? E.scPend : c.scrollTop; E.scPend = null; let h = '';
    if (E.pest === 'ficha') h = (E.sel ? '<div class="filtro"><button data-accion="centrar">📍 Ir</button><button data-accion="soltar">✕</button></div>' : '') + P.ficha(m, E.sel, { tema: E.tema, bio: E.bio });
    else if (E.pest === 'cronica') h = P.cronica(m, { imp: E.cron.imp, aqui: E.cron.aqui, sel: E.sel });
    else if (E.pest === 'historiador') h = P.historiador(m, E.evSel, E.hist);
    else if (E.pest === 'sismografo') h = P.sismografo(m, { sel: E.sel });
    else if (E.pest === 'estados') h = P.estados(m);
    else if (E.pest === 'facciones') h = P.facciones(m);
    else if (E.pest === 'personas') h = P.personas(m, E.per);
    else if (E.pest === 'arsenal') h = P.arsenal(m, { est: E.ars.est >= 0 ? E.ars.est : (E.sel && E.sel.t === 'E' ? E.sel.id : -1) });
    else if (E.pest === 'batallas') h = P.batallas(m, { abiertos: E.abiertos });
    else if (E.pest === 'economia') h = P.economia(m);
    else if (E.pest === 'teorias') h = P.teorias(m);
    else if (E.pest === 'historia') h = P.historia(m);
    c.innerHTML = h; c.scrollTop = sc;
    const det = c.querySelector('details.mano'); if (det) { det.open = E.mano; det.addEventListener('toggle', () => { E.mano = det.open; }); }
    // Los desplegables recuerdan si estaban abiertos; lo de dentro se monta al abrirlos.
    for (const d of c.querySelectorAll('details[data-k]')) d.addEventListener('toggle', () => { const k = d.dataset.k; if (d.open === E.abiertos.has(k)) return; if (d.open) E.abiertos.add(k); else E.abiertos.delete(k); E.sucio = true; });
    P.pintar(m, c);
    E.sucio = false; E.tPanel = performance.now(); apunta();
  }
  function pestana(p) { E.pest = p; for (const b of $('pestanas').children) b.classList.toggle('activa', b.dataset.p === p); $('contenido').scrollTop = 0; E.sucio = true; }
  function elegir(ref, noCambiar) { E.sel = ref; if (!noCambiar && E.pest !== 'sismografo' && E.pest !== 'cronica') pestana('ficha'); else E.sucio = true; if (E.pest === 'ficha') $('contenido').scrollTop = 0; }
  function refDe(s) {
    if (s[0] === 'C') { const [i, coh, a] = s.slice(1).split('_').map(Number); const c = S.per.caras(E.m, E.m.ase[a], 20).find(x => x.i === i && x.coh === coh); return c ? { t: 'C', id: i, a, cara: c } : null; }
    return { t: s[0], id: +s.slice(1) };
  }
  function verHecho(id) { E.evSel = id; pestana('historiador'); if (M.capa === 'noticia') M.elegirNoticia(E.m, id); }

  // ── Atrás y adelante, como en un navegador. Una «página» es lo que se está mirando en el panel: la pestaña y,
  // según cuál sea, lo elegido, el tema abierto, el hecho del historiador o el Estado del arsenal. Los filtros no cuentan.
  const nav = U.nav = { pila: [], i: -1, quieto: false };
  const PEST = { ficha: 'Ficha', cronica: 'Crónica', historiador: 'Historiador', sismografo: 'Sismógrafo', estados: 'Estados', facciones: 'Facciones', personas: 'Personas', historia: 'Historia', arsenal: 'Arsenal', batallas: 'Batallas', economia: 'Economía', teorias: 'Teorías' };
  const refTxt = (r) => (r ? (r.t === 'C' && r.cara ? 'C' + r.id + '_' + r.cara.coh + '_' + r.a : r.t + r.id) : '');
  function clave() {
    const p = E.pest;
    if (p === 'historiador') return p + '|' + E.evSel;
    if (p === 'arsenal') return p + '|' + E.ars.est;
    if (p === 'ficha') return p + '|' + (E.sel ? refTxt(E.sel) : E.tema ? 'tema:' + E.tema.id : 'mundo');
    if (p === 'sismografo' || p === 'cronica') return p + '|' + refTxt(E.sel);
    return p;
  }
  const nomRef = (m, r) => (r.t === 'C' ? (r.cara ? r.cara.nom : 'una cara') : r.t === 'U' ? 'Guarnición de ' + m.ase[m.uni[r.id].ase].nom : r.t === 'D' ? S.cap(m.dis[r.id].nom) : m.nom(r.t, r.id));
  function titulo(x) {
    const m = E.m; let d = '';
    try {
      if (x.pest === 'ficha') d = x.sel ? nomRef(m, x.sel) : x.tema ? S.cap(P.temaDe(m, x.tema.id).nom) : 'El mundo';
      else if (x.pest === 'historiador') d = x.evSel !== null && m.ev[x.evSel] ? m.ev[x.evSel].k.replace(/_/g, ' ') + ', ' + m.fecha(m.ev[x.evSel].t).toLowerCase() : '';
      else if (x.pest === 'arsenal') d = x.ars >= 0 && m.est[x.ars] ? m.est[x.ars].nom : '';
      else if ((x.pest === 'sismografo' || x.pest === 'cronica') && x.sel) d = nomRef(m, x.sel);
    } catch (e) { d = ''; }
    return (PEST[x.pest] || x.pest) + (d ? ' › ' + d : '');
  }
  function pintaNav() {
    const a = $('navAtras'), d = $('navAdelante'); const pa = nav.pila[nav.i - 1], pd = nav.pila[nav.i + 1];
    a.disabled = !pa; d.disabled = !pd; a.title = (pa ? 'Atrás: ' + titulo(pa) : 'Atrás') + ' (Alt + ←)'; d.title = (pd ? 'Adelante: ' + titulo(pd) : 'Adelante') + ' (Alt + →)';
    $('navDonde').textContent = nav.pila[nav.i] ? titulo(nav.pila[nav.i]) : '';
  }
  // Se llama cada vez que se pinta el panel: si lo que se mira ha cambiado, es una página nueva (y se pierde el «adelante»).
  function apunta() {
    if (nav.quieto || !E.m) return; const k = clave(); const act = nav.pila[nav.i]; if (act && act.k === k) return;
    nav.pila.length = nav.i + 1; nav.pila.push({ k, pest: E.pest, sel: E.sel, tema: E.tema, evSel: E.evSel, ars: E.ars.est, sc: 0 });
    if (nav.pila.length > 150) nav.pila.shift(); nav.i = nav.pila.length - 1; pintaNav();
  }
  const guardaSc = () => { const a = nav.pila[nav.i]; if (a) a.sc = $('contenido').scrollTop || 0; };
  function navegar(d) {
    const j = nav.i + d; if (j < 0 || j >= nav.pila.length || !E.m) return false; guardaSc(); const x = nav.pila[j]; nav.i = j; nav.quieto = true;
    E.sel = x.sel && x.sel.t === 'N' && !E.m.nav[x.sel.id] ? null : x.sel; E.tema = x.tema; E.evSel = x.evSel; E.ars.est = x.ars; pestana(x.pest); E.scPend = x.sc;
    if (x.pest === 'historiador' && M.capa === 'noticia' && x.evSel !== null) M.elegirNoticia(E.m, x.evSel);
    nav.quieto = false; pintaNav(); return true;
  }
  nav.ir = navegar; nav.clave = clave; nav.titulo = titulo;

  // ── La mano.
  function aplicar(id, ref) {
    const m = E.m; if (!m || m.presente <= 0 && E.pre) return;
    const def = S.reg.dios[id]; if (!D.puede(m, id, ref)) { aviso('Eso no se puede hacer ahí.', 2500); return; }
    const antes = m.ev.length; const ev = D.hacer(m, id, ref);
    consumirNuevos();
    aviso(def.ico + ' ' + def.nom + ': hecho. El mundo le buscará una causa.', 3500);
    if (ev >= 0 && m.ev[ev]) { E.evSel = ev; if (E.pest === 'historiador') E.sucio = true; }
    void antes; E.sucio = true;
  }
  function construirMano() {
    const c = $('herramientas'); let h = '';
    D.CATS.forEach((cat, k) => {
      h += '<div class="cat">' + cat + '</div><div class="rej">';
      for (const id in S.reg.dios) { const d = S.reg.dios[id]; if (d.cat !== k) continue; h += '<button data-h="' + id + '" title="' + d.nom + '\n' + d.desc.replace(/"/g, '&quot;') + '">' + d.ico + '</button>'; }
      h += '</div>';
    });
    c.innerHTML = h;
    c.addEventListener('click', (e) => { const b = e.target.closest('button[data-h]'); if (!b) return; herramienta(E.herr === b.dataset.h ? null : b.dataset.h); });
  }
  function herramienta(id) {
    E.herr = id; for (const b of $('herramientas').querySelectorAll('button')) b.classList.toggle('activa', b.dataset.h === id);
    $('mapa').classList.toggle('herramienta', !!id); if (id) avisoHerr(); else aviso('');
  }

  // ── Hechos nuevos: avisos en el mapa y en el pie.
  function consumirNuevos() {
    const m = E.m; let ult = null;
    for (const id of m.nuevos) { if (id <= E.visto) continue; E.visto = id; const e = m.ev[id]; if (m.presente > 0 || e.imp >= 2) M.ping(m, e); if (m.presente > 0) U.local.efecto(m, e); if (e.imp >= 2 || !ult) ult = e; }
    if (ult) { const u = $('ultimo'); u.textContent = m.fecha(ult.t) + ' · ' + m.texto(ult); u.dataset.ev = ult.id; }
  }
  function barra() {
    const m = E.m; $('fecha').textContent = m.fecha();
    let g2 = 0, est = 0; for (const e of m.est) if (e.vivo) { est++; g2 += e.gue.size; }
    let bat = 0; for (const b of m.bat) if (b.vivo) bat++;
    $('resumen').textContent = est + ' estados · ' + g2 / 2 + ' guerras' + (bat ? ' · ' + bat + ' batallas en curso' : '');
  }

  // ── Bucle.
  let tAnt = 0;
  function cuadro(ahora) {
    requestAnimationFrame(cuadro);
    const dt = Math.min(0.1, (ahora - tAnt) / 1000); tAnt = ahora; const m = E.m;
    if (m) {
      if (E.pre) {
        // Prehistoria: a toda velocidad, sin esperar a nadie.
        m.avanzar(Math.min(E.pre.fin, m.t + 90), 45);
        $('barraProg').style.width = Math.round((m.t - E.pre.ini) / (E.pre.fin - E.pre.ini) * 100) + '%';
        $('estadoInicio').textContent = 'Simulando la prehistoria… ' + m.fecha() + ' · ' + m.ev.length.toLocaleString('es-ES') + ' hechos que nadie ha escrito';
        if (m.t >= E.pre.fin) { m.presente = m.t; m.reg('presente', 'Acaba la prehistoria: ' + Math.round((E.pre.fin - E.pre.ini) / S.ANIO) + ' años que nadie ha escrito. A partir de aquí, miras', { imp: 1 }); E.pre = null; E.objetivo = m.t; $('inicio').classList.add('oculto'); E.sucio = true; }
      } else if (E.vel > 0) {
        E.objetivo += E.vel * dt;
        const ok = m.avanzar(E.objetivo, PRESUPUESTO);
        if (!ok) { E.objetivo = m.t; $('carga').textContent = '● la simulación va tarde'; } else $('carga').textContent = '';
      }
      consumirNuevos(); barra();
      M.famaDe = E.sel && E.sel.t === 'P' ? E.sel.id : E.seguir && E.seguir.t === 'P' ? E.seguir.id : M.capa === 'fama' ? S.his.pesos(m).orden[0] : -1;
      if (E.sel && E.sel.t === 'N' && !m.nav[E.sel.id]) E.sel = null;
      // Seguir a alguien: la cámara va detrás, suave.
      if (E.seguir) { const p = M.posDe(m, E.seguir, m.t); if (p) { M.cam.x += (p.x - M.cam.x) * 0.12; M.cam.y += (p.y - M.cam.y) * 0.12; } else seguir(null); }
    }
    M.dibujar(m, m ? m.t : 0, E.sel);
    const quieto = document.activeElement && document.activeElement.tagName === 'SELECT';
    if (m && !quieto && (E.sucio || (ahora - E.tPanel > (E.pre ? 1500 : 700) && (E.vel > 0 || E.pre) && !ratonEnPanel))) pintarPanel();
  }
  let ratonEnPanel = false;

  function seguir(ref) { E.seguir = ref; M.seguir = ref; if (ref) { if (M.cam.z < 2.6) M.cam.z = 2.6; aviso('👁 Siguiendo a ' + (ref.t === 'P' ? E.m.per[ref.id].nom : E.m.nav[ref.id].nom) + ' (arrastra el mapa o pulsa Esc para soltar)', 6000); } }
  function velocidad(v) { E.vel = v; for (const b of $('vel').children) b.classList.toggle('activa', +b.dataset.v === v); if (E.m) E.objetivo = E.m.t; }

  function crear() {
    const semilla = Math.max(1, +$('semilla').value | 0), sistemas = +$('sistemas').value, anios = +$('prehistoria').value;
    $('crear').disabled = true; $('estadoInicio').textContent = 'Generando la galaxia…';
    setTimeout(() => {
      const m = S.crear(semilla, { sistemas }); E.m = m; g.mundo = m; nav.pila.length = 0; nav.i = -1; E.tema = null;
      M.ajustar(m); E.visto = -1; E.sel = null; E.evSel = null; E.seguir = null; M.seguir = null;
      E.pre = { ini: m.t, fin: m.t + anios * S.ANIO };
      $('inicio').classList.add('prehistoria');
    }, 30);
  }

  function iniciar() {
    U.prefs.aplicar();
    const cv = $('mapa'); M.iniciar(cv); construirMano(); velocidad(6);
    g.addEventListener('resize', () => { M.redimensionar(); });
    // Ratón en el mapa: arrastrar, rueda y clic.
    let arr = null;
    cv.addEventListener('mousedown', (e) => { arr = { x: e.clientX, y: e.clientY, cx: M.cam.x, cy: M.cam.y, mov: 0 }; cv.classList.add('arrastra'); });
    g.addEventListener('mousemove', (e) => { if (!arr) return; const dx = e.clientX - arr.x, dy = e.clientY - arr.y; arr.mov = Math.max(arr.mov, Math.abs(dx) + Math.abs(dy)); if (arr.mov > 8 && E.seguir) seguir(null); M.cam.x = arr.cx - dx / M.cam.z; M.cam.y = arr.cy - dy / M.cam.z; });
    g.addEventListener('mouseup', (e) => {
      if (!arr) return; const fue = arr; arr = null; cv.classList.remove('arrastra');
      if (fue.mov > 5 || e.target !== cv || !E.m) return;
      const r = cv.getBoundingClientRect(); const ref = M.elegir(e.clientX - r.left, e.clientY - r.top);
      if (E.herr) {
        if (!ref || ref.t === 'V') { aviso('Ahí no hay nada.', 1500); return; }
        let obj = ref; if (ref.t === 'C') { const p = S.per.promover(E.m, E.m.ase[ref.a], ref.cara); obj = { t: 'P', id: p.id }; }
        const id = D.resolver(E.m, E.herr, obj);
        if (id < 0) { aviso('Esa herramienta no se aplica a eso.', 2500); return; }
        aplicar(E.herr, id); const q = S.reg.dios[E.herr].obj; elegir({ t: q, id }, true); return;
      }
      if (ref && ref.t === 'V') { verHecho(ref.id); return; }
      elegir(ref);
    });
    cv.addEventListener('wheel', (e) => { e.preventDefault(); const r = cv.getBoundingClientRect(); M.zoom(e.clientX - r.left, e.clientY - r.top, Math.exp(-e.deltaY * 0.0016)); }, { passive: false });
    cv.addEventListener('dblclick', (e) => { const r = cv.getBoundingClientRect(); M.zoom(e.clientX - r.left, e.clientY - r.top, 2.2); });
    // Panel: todo por delegación.
    const panel = $('panel');
    panel.addEventListener('mouseenter', () => { ratonEnPanel = true; }); panel.addEventListener('mouseleave', () => { ratonEnPanel = false; });
    $('pestanas').addEventListener('click', (e) => { const b = e.target.closest('button'); if (b && b.dataset.p) pestana(b.dataset.p); });
    // Atrás y adelante: botones, Alt + flechas y los botones laterales del ratón. La altura a la que estabas se recuerda.
    $('navAtras').addEventListener('click', () => navegar(-1)); $('navAdelante').addEventListener('click', () => navegar(1));
    g.addEventListener('mousedown', guardaSc, true);
    g.addEventListener('mouseup', (e) => { if (e.button === 3 || e.button === 4) { e.preventDefault(); navegar(e.button === 3 ? -1 : 1); } });
    $('contenido').addEventListener('click', (e) => {
      const m = E.m; if (!m) return; const t = e.target;
      // Una casilla del mapa de calor de la historia: abre ese tema.
      if (t._clic) { const r = t.getBoundingClientRect(); const o = t._clic(e.clientX - r.left, e.clientY - r.top); if (o && o.tema) { E.tema = { id: o.tema, imp: 0, lim: 150, fuera: {} }; E.sel = null; pestana('ficha'); } return; }
      const sr = t.closest('[data-serie]'); if (sr) { U.prefs.alternarSerie(sr.dataset.serie); E.sucio = true; return; }
      // Una cifra de la ficha del mundo: se abre su tema (su historia entera). Y dentro, qué clases de hecho se ven.
      const tm = t.closest('[data-tema]'); if (tm) { E.tema = { id: tm.dataset.tema, imp: 0, lim: 150, fuera: {} }; $('contenido').scrollTop = 0; E.sucio = true; return; }
      const tk = t.closest('[data-tk]'); if (tk && E.tema) { const k = tk.dataset.tk; if (E.tema.fuera[k]) delete E.tema.fuera[k]; else E.tema.fuera[k] = 1; E.sucio = true; return; }
      const d = t.closest('[data-dios]'); if (d && !d.disabled) { aplicar(d.dataset.dios, +d.dataset.id); return; }
      const ac = t.closest('[data-accion]');
      if (ac) {
        const a = ac.dataset.accion;
        if (a === 'centrar' && E.sel) M.centrar(m, E.sel.t === 'C' ? { t: 'A', id: E.sel.a } : E.sel);
        else if (a === 'soltar') elegir(null);
        else if (a === 'temaFuera') { E.tema = null; $('contenido').scrollTop = 0; E.sucio = true; }
        else if (a === 'capaFama' && E.sel && E.sel.t === 'P') { M.capa = 'fama'; $('capa').value = 'fama'; if (M.cam.z > 1.4) M.ajustar(m); aviso('🌡 En caliente, los mundos donde se conoce su nombre.', 5000); }
        else if (a === 'temaMas' && E.tema) { E.tema.lim += 150; E.sucio = true; }
        else if (a === 'seguir' && E.sel && (E.sel.t === 'P' || E.sel.t === 'N')) seguir(E.seguir && E.seguir.t === E.sel.t && E.seguir.id === E.sel.id ? null : { t: E.sel.t, id: E.sel.id });
        else if (a === 'plano' && E.sel && E.sel.t === 'A') M.centrar(m, E.sel, 12.5);
        else if (a === 'ir') { const ref = refDe(ac.dataset.r); if (ref) { seguir(null); M.centrar(m, ref, +ac.dataset.z || 0); if (M.cam.z < +ac.dataset.z) M.cam.z = +ac.dataset.z; } }
        else if (a === 'arsenal') { E.ars.est = +ac.dataset.id; pestana('arsenal'); }
        else if (a === 'promover' && E.sel && E.sel.t === 'C') { const p = S.per.promover(m, m.ase[E.sel.a], E.sel.cara); M.caras = null; elegir({ t: 'P', id: p.id }); }
        else if (a === 'centrarEv' && E.evSel !== null) M.centrar(m, { t: 'V', id: E.evSel });
        else if (a === 'noticia' && E.evSel !== null) { const k = M.elegirNoticia(m, E.evSel); M.capa = 'noticia'; $('capa').value = 'noticia'; aviso(k ? 'En verde, quien ya lo sabe (y la cifra que le ha llegado). En rojo, quien aún no.' : 'De ese hecho no viaja ninguna noticia.', 5000); }
        return;
      }
      const r = t.closest('a.ref');
      if (r) { if (r.dataset.ev !== undefined) { verHecho(+r.dataset.ev); return; } const ref = refDe(r.dataset.r); if (ref) { elegir(ref); if (e.detail >= 2 || e.ctrlKey) M.centrar(m, ref.t === 'C' ? { t: 'A', id: ref.a } : ref); } return; }
      const ev = t.closest('[data-ev]'); if (ev) verHecho(+ev.dataset.ev);
    });
    $('contenido').addEventListener('change', (e) => { const o = e.target.dataset.op; if (o === 'imp') E.cron.imp = +e.target.value; if (o === 'aqui') E.cron.aqui = e.target.checked; if (o === 'pf') E.per.filtro = e.target.value; if (o === 'pe') E.per.est = +e.target.value; if (o === 'ae') E.ars.est = +e.target.value; if (o === 'ti' && E.tema) E.tema.imp = +e.target.value; if (o === 'hi') E.bio.imp = +e.target.value; if (o === 'ho') E.bio.orden = e.target.value; if (o === 'gp') E.hist.prof = +e.target.value; e.target.blur(); E.sucio = true; });
    $('ultimo').addEventListener('click', () => { const id = $('ultimo').dataset.ev; if (id !== undefined) { verHecho(+id); M.centrar(E.m, { t: 'V', id: +id }, 1.5); } });
    $('vel').addEventListener('click', (e) => { const b = e.target.closest('button'); if (b) velocidad(+b.dataset.v); });
    $('capa').addEventListener('change', (e) => { M.capa = e.target.value; if (M.capa === 'noticia' && E.evSel !== null && E.m) M.elegirNoticia(E.m, E.evSel); });
    $('crear').addEventListener('click', crear);
    $('btnAyuda').addEventListener('click', () => $('ayuda').classList.toggle('oculto'));
    // Aspecto: un panel pequeño bajo su botón; cada cambio se aplica y se guarda en el momento.
    const asp = $('aspecto'); const pintaAspecto = () => { asp.innerHTML = U.prefs.html(); };
    $('btnAspecto').addEventListener('click', () => { pintaAspecto(); asp.classList.toggle('oculto'); });
    const tocaAspecto = (e) => { const el = e.target.closest ? e.target.closest('[data-pref]') : null; if (!el || (e.type === 'click' && el.type === 'checkbox')) return; if (U.prefs.accion(el)) { pintaAspecto(); E.sucio = true; } };
    asp.addEventListener('click', tocaAspecto); asp.addEventListener('change', tocaAspecto);
    U.viz.raton($('contenido'));
    $('cerrarAyuda').addEventListener('click', () => $('ayuda').classList.add('oculto'));
    g.addEventListener('keydown', (e) => {
      if (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT') return;
      if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) { e.preventDefault(); navegar(e.key === 'ArrowLeft' ? -1 : 1); return; }
      if (e.code === 'Space') { e.preventDefault(); if (E.vel > 0) { E.velAntes = E.vel; velocidad(0); } else velocidad(E.velAntes || 6); }
      else if (e.key >= '1' && e.key <= '4') velocidad([1, 6, 30, 180][+e.key - 1]);
      else if (e.key === 'Escape') { herramienta(null); seguir(null); $('ayuda').classList.add('oculto'); $('aspecto').classList.add('oculto'); }
      else if (e.key === 'h' || e.key === 'H') $('ayuda').classList.toggle('oculto');
    });
    requestAnimationFrame(cuadro);
  }
  U.iniciar = iniciar; U.pintarPanel = pintarPanel; U.aplicar = aplicar;
  if (g.document && document.getElementById('mapa') && !g.SIN_ARRANQUE) iniciar();
})(typeof globalThis !== 'undefined' ? globalThis : this);
