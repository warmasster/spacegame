import { Game } from './game';

const canvas = document.getElementById('view') as HTMLCanvasElement;
const ui = document.getElementById('ui') as HTMLDivElement;

const menu = document.createElement('div');
menu.className = 'menu';
menu.innerHTML = `
  <div class="menu-card">
    <div class="menu-kicker">Misión</div>
    <h1>SELENE <span>I</span></h1>
    <p class="menu-sub">Exploración lunar cooperativa · prototipo FPV</p>
    <form id="join">
      <label>Nombre del astronauta
        <input id="name" maxlength="20" autocomplete="off" spellcheck="false" placeholder="p. ej. Ana" />
      </label>
      <label>Calidad gráfica
        <select id="quality">
          <option value="high">Alta (sombras 2K, AO, MSAA)</option>
          <option value="low">Baja (portátiles / GPU integrada)</option>
        </select>
      </label>
      <button type="submit">Iniciar EVA</button>
    </form>
    <p class="menu-status" id="status"></p>
    <p class="menu-foot">Hasta 2 astronautas por servidor. Terreno, estrellas y traje generados de forma procedural;
      texturas de la Tierra: NASA Blue Marble.</p>
  </div>`;
ui.appendChild(menu);

const pause = document.createElement('div');
pause.className = 'pause hidden';
pause.innerHTML = `<div><b>Haz clic para continuar</b><span>WASD moverse · Mayús correr · Espacio saltar · V cámara · L luces · H ayuda</span></div>`;
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
  (menu.querySelector('button') as HTMLButtonElement).disabled = true;
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
    (menu.querySelector('button') as HTMLButtonElement).disabled = false;
    game = null;
  }
});

canvas.addEventListener('click', () => game?.lockPointer());
pause.addEventListener('click', () => game?.lockPointer());
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
