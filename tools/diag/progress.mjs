// Progress for the diagnostic tools: they run in software GL and take minutes, so they say how far
// along they are. `boot()` also relays the game's own loading messages (the slowest part).
//
//   const P = progress('ship', 26);  …  P.step('rampa');   → [ 42%] ship · rampa (1:10, faltan ~1:37)

const fmt = (ms) => {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};

export function progress(title, total) {
  const t0 = Date.now();
  let done = 0;
  return {
    /** One unit of work finished. */
    step(label) {
      done++;
      const el = Date.now() - t0;
      const pct = Math.min(done >= total ? 100 : 99, Math.round((done / total) * 100));
      const left = done < total ? `, faltan ~${fmt((el / done) * (total - done))}` : '';
      console.log(`[${String(pct).padStart(3)}%] ${title} · ${label} (${fmt(el)}${left})`);
    },
    /** Total changed (e.g. a mode skips part of the work). */
    setTotal(n) {
      total = n;
    },
  };
}

/**
 * Open the game, join and wait until it is playable, printing its loading status as it changes
 * (with elapsed time) so a slow start is visibly alive.
 */
export async function boot(page, url, { name = 'diag', quality = null, label = 'juego' } = {}) {
  const t0 = Date.now();
  console.log(`[  0%] cargando ${label}…`);
  await page.goto(url);
  await page.fill('#name', name);
  if (quality) await page.selectOption('#quality', quality);
  await page.click('button[type=submit]');
  let last = '';
  for (;;) {
    const s = await page.evaluate(() => ({ ready: !!document.querySelector('.menu')?.classList.contains('hidden'), status: document.querySelector('#status')?.textContent ?? '' }));
    if (s.ready) break;
    if (s.status && s.status !== last) {
      last = s.status;
      console.log(`       ${label}: ${s.status} (${fmt(Date.now() - t0)})`);
    }
    if (Date.now() - t0 > 240000) throw new Error(`${label}: no arrancó en 4 min (${last})`);
    await new Promise((r) => setTimeout(r, 1000));
  }
  console.log(`       ${label} listo en ${fmt(Date.now() - t0)}`);
}
