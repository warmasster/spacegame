import { Game } from './game';

const canvas = document.getElementById('view') as HTMLCanvasElement;
const ui = document.getElementById('ui') as HTMLDivElement;

const menu = document.createElement('div');
menu.className = 'menu';
menu.innerHTML = `
  <div class="sc-frame">
    <header class="sc-top">
      <span class="sc-tag">SELENE · MOBI//OS 0.2</span>
      <span class="sc-tag dim">LUNA · 20.2°N 30.8°E · MAÑANA LUNAR</span>
    </header>
    <nav class="sc-nav">
      <div class="sc-brand"><small>MISIÓN</small><h1>SELENE <em>I</em></h1><p>Exploración lunar cooperativa</p></div>
      <button type="button" class="sc-item active" data-tab="eva"><i></i>INICIAR EVA</button>
      <button type="button" class="sc-item" data-tab="controls"><i></i>CONTROLES</button>
      <button type="button" class="sc-item" data-tab="about"><i></i>INFORMACIÓN</button>
    </nav>
    <section class="sc-panel" data-panel="eva">
      <h2><span>01</span>REGISTRO DE TRIPULANTE</h2>
      <form id="join">
        <label>IDENTIFICADOR
          <input id="name" maxlength="20" autocomplete="off" spellcheck="false" placeholder="p. ej. Ana" />
        </label>
        <label>PERFIL GRÁFICO
          <select id="quality">
            <option value="high">ALTA · sombras en cascada, AO, MSAA</option>
            <option value="low">BAJA · portátiles / GPU integrada</option>
          </select>
        </label>
        <button type="submit" class="sc-cta"><span>INICIAR EVA</span><b>▸</b></button>
      </form>
      <p class="menu-status" id="status"></p>
      <div class="sc-stats">
        <div><small>GRAVEDAD</small><b>1,62 m/s²</b></div>
        <div><small>TRIPULACIÓN</small><b>2 MÁX</b></div>
        <div><small>ATMÓSFERA</small><b>VACÍO</b></div>
      </div>
    </section>
    <section class="sc-panel hidden" data-panel="controls">
      <h2><span>02</span>CONTROLES</h2>
      <div class="sc-keys">
        ${[['W A S D','moverse'],['Mayús','trote lunar'],['Espacio','saltar · mantener en el aire: jetpack'],['Clic izq. / F','disparar · con la soldadora: mantener sobre un panel para repararlo'],['E / clic en un mando','accionarlo · en un asiento: sentarse / levantarse'],['1 / 2','lanzacohetes / soldadora'],['X','sacar / guardar herramienta'],['Rueda · clic der.','zoom (1ª persona) · zoom mantenido'],['C / Ctrl','agacharse'],['L','luces del casco'],['V','primera / tercera persona'],['Alt + ratón','mirar alrededor (3ª)'],['H','ayuda en pantalla'],['Esc','menú']].map(([k, v]) => `<div><kbd>${k}</kbd><span>${v}</span></div>`).join('')}
      </div>
    </section>
    <section class="sc-panel hidden" data-panel="about">
      <h2><span>03</span>INFORMACIÓN</h2>
      <p class="sc-text">Prototipo de exploración lunar cooperativa. Terreno deformable, estrellas reales del catálogo
        HYG y traje EVA generado por código. Texturas de la Tierra: NASA Blue Marble.</p>
    </section>
    <footer class="sc-bottom"><span class="sc-tag dim">CONEXIÓN SEGURA · CANAL 01</span><span class="sc-tag dim">v0.2</span></footer>
  </div>`;
menu.querySelectorAll<HTMLButtonElement>('.sc-item').forEach((b) =>
  b.addEventListener('click', () => {
    menu.querySelectorAll('.sc-item').forEach((x) => x.classList.toggle('active', x === b));
    menu.querySelectorAll<HTMLElement>('.sc-panel').forEach((p) => p.classList.toggle('hidden', p.dataset.panel !== b.dataset.tab));
  }),
);
ui.appendChild(menu);

const pause = document.createElement('div');
pause.className = 'pause hidden';
pause.innerHTML = `<div class="sc-pause">
  <small>EVA EN PAUSA · CONTROL LOCAL</small>
  <button type="button" class="sc-cta" id="resume"><span>REANUDAR</span><b>▸</b></button>
  <button type="button" class="sc-ghost" id="leave">ABANDONAR EVA</button>
  <p>WASD moverse · Espacio saltar/jetpack · Clic disparar/soldar · E accionar/sentarse · 1/2 herramienta · clic der. zoom · V cámara · L luces · H ayuda</p>
</div>`;
ui.appendChild(pause);

const nameInput = menu.querySelector('#name') as HTMLInputElement;
const quality = menu.querySelector('#quality') as HTMLSelectElement;
const status = menu.querySelector('#status') as HTMLParagraphElement;
nameInput.value = localStorageGet('selene.name') ?? '';
quality.value = localStorageGet('selene.quality') ?? (isLikelyLowEnd() ? 'low' : 'high');

let game: Game | null = null;

menu.querySelector('#join')!.addEventListener('submit', async (e) => {
  e.preventDefault();
  if (game) return;
  const name = nameInput.value.trim();
  localStorageSet('selene.name', name);
  localStorageSet('selene.quality', quality.value);
  (menu.querySelector('.sc-cta') as HTMLButtonElement).disabled = true;
  status.classList.remove('error');
  game = new Game({
    canvas,
    ui,
    name,
    quality: quality.value === 'low' ? 'low' : 'high',
    onProgress: (t) => (status.textContent = t),
  });
  (window as unknown as { game: Game }).game = game;
  try {
    await game.start();
    menu.classList.add('hidden');
    pause.classList.remove('hidden');
  } catch (err) {
    console.error(err);
    status.textContent = err instanceof Error ? err.message : String(err);
    status.classList.add('error');
    (menu.querySelector('.sc-cta') as HTMLButtonElement).disabled = false;
    game?.dispose();
    game = null;
  }
});

canvas.addEventListener('click', () => game?.lockPointer());
pause.querySelector('#resume')!.addEventListener('click', () => game?.lockPointer());
pause.querySelector('#leave')!.addEventListener('click', () => location.reload());
document.addEventListener('pointerlockchange', () => {
  if (!game) return;
  pause.classList.toggle('hidden', document.pointerLockElement === canvas);
});

function localStorageGet(k: string) {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
}
function localStorageSet(k: string, v: string) {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* private mode */
  }
}
function isLikelyLowEnd() {
  return (navigator.hardwareConcurrency ?? 8) <= 4;
}
