import { Game } from './game';
import { sfx } from './audio/engine';
import { gpuInfo, suggestedQuality } from './render/gpuTier';

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
        <button type="button" class="sc-ghost" id="new-world">MUNDO NUEVO</button>
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
        ${[['W A S D','moverse'],['Mayús','trote lunar'],['Espacio','saltar · mantener en el aire: jetpack'],['Clic izq. / F','disparar · con la soldadora: mantener sobre un panel o una máquina para repararlo'],['E / clic en un mando','accionarlo · en un asiento: sentarse / levantarse'],['Rueda sobre un selector','girarlo; si no, zoom'],['1 / 2','lanzacohetes / soldadora'],['X','sacar / guardar herramienta'],['Clic der.','zoom mantenido'],['C / Ctrl','agacharse'],['L','luces del casco'],['V','primera / tercera persona'],['Alt + ratón','mirar alrededor (3ª)'],['M','manual de la nave'],['H','ayuda en pantalla'],['Esc','menú']].map(([k, v]) => `<div><kbd>${k}</kbd><span>${v}</span></div>`).join('')}
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
  <small class="sc-sub">SONIDO</small>
  <label class="sc-audio">VOLUMEN <output id="volume-v"></output>
    <input type="range" id="volume" min="0" max="100" step="1" />
  </label>
  <label class="sc-audio">EN EL VACÍO
    <select id="vacuum">
      <option value="physical">FÍSICO</option>
      <option value="muffled">AMORTIGUADO</option>
    </select>
  </label>
  <p class="sc-hint" id="vacuum-help"></p>
  <p>WASD moverse · Espacio saltar/jetpack · Clic disparar/soldar · E accionar/sentarse · 1/2 herramienta · rueda selector/zoom · M manual · V cámara · L luces · H ayuda</p>
</div>`;
ui.appendChild(pause);

// sound settings (kept in the browser by the audio engine)
const volume = pause.querySelector('#volume') as HTMLInputElement;
const volumeV = pause.querySelector('#volume-v') as HTMLOutputElement;
const vacuum = pause.querySelector('#vacuum') as HTMLSelectElement;
const vacuumHelp = pause.querySelector('#vacuum-help') as HTMLParagraphElement;
const VACUUM_HELP: Record<string, string> = {
  physical: 'Sin aire no viaja el sonido: fuera solo oyes lo que te llega por el suelo, la estructura que pisas o tu propio traje.',
  muffled: 'Todo se oye aunque no haya aire, como un retumbo apagado.',
};
const showAudio = () => {
  volumeV.textContent = `${volume.value} %`;
  vacuumHelp.textContent = VACUUM_HELP[vacuum.value] ?? '';
};
volume.value = String(Math.round(sfx.volume * 100));
vacuum.value = sfx.mode;
showAudio();
volume.addEventListener('input', () => {
  sfx.setVolume(Number(volume.value) / 100);
  showAudio();
});
vacuum.addEventListener('change', () => {
  sfx.setMode(vacuum.value === 'muffled' ? 'muffled' : 'physical');
  showAudio();
});

const nameInput = menu.querySelector('#name') as HTMLInputElement;
const quality = menu.querySelector('#quality') as HTMLSelectElement;
const status = menu.querySelector('#status') as HTMLParagraphElement;
nameInput.value = localStorageGet('selene.name') ?? '';
// the profile follows the machine's graphics (render/gpuTier.ts) unless the player picked one
const gpu = gpuInfo();
quality.value = localStorageGet('selene.qualityPicked') === '1' ? (localStorageGet('selene.quality') ?? suggestedQuality(gpu)) : suggestedQuality(gpu);
quality.title = `Gráfica: ${gpu.renderer || 'desconocida'}${gpu.integrated ? ' (integrada)' : gpu.software ? ' (por software)' : ''}`;
quality.addEventListener('change', () => localStorageSet('selene.qualityPicked', '1'));

let game: Game | null = null;

const worldButton = menu.querySelector('#new-world') as HTMLButtonElement;
const showWorld = () => {
  const n = Number(sessionStorage.getItem('selene.world'));
  worldButton.textContent = sessionStorage.getItem('selene.reseed') === '1' && n > 0 ? `MUNDO NUEVO · ${n}` : 'MUNDO NUEVO';
};
showWorld();
worldButton.addEventListener('click', () => {
  const seed = (Math.floor(Math.random() * 0xffffffff) >>> 0) || 1;
  sessionStorage.setItem('selene.world', String(seed));
  sessionStorage.setItem('selene.reseed', '1');
  showWorld();
});

menu.querySelector('#join')!.addEventListener('submit', async (e) => {
  e.preventDefault();
  if (game) return;
  const name = nameInput.value.trim();
  localStorageSet('selene.name', name);
  localStorageSet('selene.quality', quality.value);
  (menu.querySelector('.sc-cta') as HTMLButtonElement).disabled = true;
  status.classList.remove('error');
  const rolled = sessionStorage.getItem('selene.reseed') === '1' ? Number(sessionStorage.getItem('selene.world')) : 0;
  if (rolled > 0) sessionStorage.removeItem('selene.reseed');
  game = new Game({
    canvas,
    ui,
    name,
    quality: quality.value === 'low' ? 'low' : 'high',
    worldSeed: rolled > 0 ? rolled : undefined,
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
    if (rolled > 0) sessionStorage.setItem('selene.reseed', '1');
    showWorld();
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
  // reading the manual frees the mouse without pausing
  pause.classList.toggle('hidden', document.pointerLockElement === canvas || game.manualOpen);
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

