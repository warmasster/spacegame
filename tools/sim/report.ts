// Small self-contained HTML reports for the world's tools (seed sweep, seismograph): inline SVG, no
// libraries, light and dark. Written to tools/sim/out/.

import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

export const OUT_DIR = resolve(import.meta.dirname, 'out');

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!);
const fmt = (n: number) => (Number.isFinite(n) ? (Math.abs(n) >= 1000 || Number.isInteger(n) ? n.toLocaleString('es-ES', { maximumFractionDigits: 0 }) : n.toFixed(3)) : '—');

export function quantile(sorted: readonly number[], q: number): number {
  if (!sorted.length) return NaN;
  const i = (sorted.length - 1) * q;
  const lo = Math.floor(i);
  const hi = Math.ceil(i);
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (i - lo);
}

export function stats(values: readonly number[]) {
  const s = values.filter(Number.isFinite).sort((a, b) => a - b);
  const mean = s.reduce((a, b) => a + b, 0) / (s.length || 1);
  return { n: s.length, min: s[0], p10: quantile(s, 0.1), median: quantile(s, 0.5), p90: quantile(s, 0.9), max: s[s.length - 1], mean };
}

/** A histogram of values (bins between min and max). */
export function histogram(values: readonly number[], bins = 20, w = 320, h = 90): string {
  const v = values.filter(Number.isFinite);
  if (!v.length) return '<svg></svg>';
  const lo = Math.min(...v);
  const hi = Math.max(...v);
  const counts = new Array<number>(bins).fill(0);
  for (const x of v) counts[hi > lo ? Math.min(bins - 1, Math.floor(((x - lo) / (hi - lo)) * bins)) : 0]++;
  const top = Math.max(...counts);
  const bw = w / bins;
  const bars = counts.map((c, i) => `<rect x="${(i * bw + 1).toFixed(1)}" y="${(h - 16 - (c / top) * (h - 20)).toFixed(1)}" width="${(bw - 2).toFixed(1)}" height="${((c / top) * (h - 20)).toFixed(1)}" rx="1.5"/>`).join('');
  return `<svg viewBox="0 0 ${w} ${h}" class="chart">${bars}<text x="0" y="${h - 2}">${fmt(lo)}</text><text x="${w}" y="${h - 2}" text-anchor="end">${fmt(hi)}</text></svg>`;
}

/** Lines over x: each series its own path (NaN: gaps); `band`: [p10, median, p90] drawn as a band. */
export function lines(x: readonly number[], series: ReadonlyArray<readonly number[]>, opts: { band?: boolean; w?: number; h?: number; xlabel?: (x: number) => string } = {}): string {
  const w = opts.w ?? 320, h = opts.h ?? 110;
  const all = series.flat().filter(Number.isFinite);
  if (!all.length || x.length < 2) return '<svg></svg>';
  const lo = Math.min(...all), hi = Math.max(...all);
  const X = (i: number) => ((x[i] - x[0]) / (x[x.length - 1] - x[0] || 1)) * (w - 4) + 2;
  const Y = (v: number) => h - 16 - (hi > lo ? (v - lo) / (hi - lo) : 0.5) * (h - 24);
  const path = (s: readonly number[]) => s.map((v, i) => (Number.isFinite(v) ? `${i && Number.isFinite(s[i - 1]) ? 'L' : 'M'}${X(i).toFixed(1)},${Y(v).toFixed(1)}` : '')).join('');
  let body = '';
  if (opts.band && series.length === 3) {
    const [p10, , p90] = series;
    const up = p90.map((v, i) => `${i ? 'L' : 'M'}${X(i).toFixed(1)},${Y(v).toFixed(1)}`).join('');
    const down = [...p10].map((v, i) => [i, v] as const).reverse().map(([i, v]) => `L${X(i).toFixed(1)},${Y(v).toFixed(1)}`).join('');
    body += `<path class="band" d="${up}${down}Z"/>`;
    body += `<path class="line" d="${path(series[1])}"/>`;
  } else for (const s of series) body += `<path class="line" d="${path(s)}"/>`;
  const xl = opts.xlabel ?? ((v: number) => fmt(v));
  return `<svg viewBox="0 0 ${w} ${h}" class="chart">${body}<text x="0" y="${h - 2}">${esc(xl(x[0]))}</text><text x="${w}" y="${h - 2}" text-anchor="end">${esc(xl(x[x.length - 1]))}</text><text x="${w}" y="10" text-anchor="end">${fmt(hi)}</text><text x="${w}" y="${h - 18}" text-anchor="end">${fmt(lo)}</text></svg>`;
}

export interface Card {
  title: string;
  svg: string;
  facts?: Array<[string, string | number]>;
}

/** A page of cards, written to tools/sim/out/<file>; returns its path. */
export function writePage(file: string, title: string, intro: string, sections: Array<{ title: string; cards: Card[] }>, tables: string[] = []): string {
  const css = `:root{--bg:#f6f7f9;--fg:#1c2330;--muted:#5d6878;--card:#fff;--line:#2f6fdb;--band:rgba(47,111,219,.18);--bar:#2f6fdb;--border:#e2e6ec}
@media (prefers-color-scheme:dark){:root{--bg:#10141b;--fg:#e6ebf2;--muted:#9aa6b6;--card:#171d27;--line:#7fb0ff;--band:rgba(127,176,255,.2);--bar:#7fb0ff;--border:#263042}}
body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.45 system-ui,-apple-system,Segoe UI,sans-serif}
main{max-width:1100px;margin:0 auto;padding:24px 16px}h1{font-size:22px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 10px}
p.intro{color:var(--muted);margin:0 0 12px}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:12px}
.card{background:var(--card);border:1px solid var(--border);border-radius:10px;padding:12px}.card h3{font-size:13px;margin:0 0 6px;font-weight:600;word-break:break-all}
.chart{width:100%;height:auto;display:block}.chart rect{fill:var(--bar)}.chart .line{fill:none;stroke:var(--line);stroke-width:1.6}.chart .band{fill:var(--band)}.chart text{fill:var(--muted);font-size:10px}
dl{display:grid;grid-template-columns:auto 1fr;gap:2px 10px;margin:8px 0 0;font-size:12px;color:var(--muted)}dd{margin:0;color:var(--fg);text-align:right;font-variant-numeric:tabular-nums}
table{border-collapse:collapse;width:100%;background:var(--card);border:1px solid var(--border);border-radius:10px;overflow:hidden;font-size:13px}th,td{padding:6px 10px;border-bottom:1px solid var(--border);text-align:right}th:first-child,td:first-child{text-align:left}
.wrap{overflow-x:auto}`;
  const card = (c: Card) => `<div class="card"><h3>${esc(c.title)}</h3>${c.svg}${c.facts ? `<dl>${c.facts.map(([k, v]) => `<dt>${esc(k)}</dt><dd>${esc(typeof v === 'number' ? fmt(v) : v)}</dd>`).join('')}</dl>` : ''}</div>`;
  const html = `<!doctype html><html lang="es"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${esc(title)}</title><style>${css}</style></head><body><main><h1>${esc(title)}</h1><p class="intro">${esc(intro)}</p>${tables.map((t) => `<div class="wrap">${t}</div>`).join('')}${sections.map((s) => `<h2>${esc(s.title)}</h2><div class="grid">${s.cards.map(card).join('')}</div>`).join('')}</main></body></html>`;
  mkdirSync(OUT_DIR, { recursive: true });
  const path = join(OUT_DIR, file);
  writeFileSync(path, html);
  return path;
}

export function table(head: string[], rows: Array<Array<string | number>>): string {
  return `<table><thead><tr>${head.map((h) => `<th>${esc(h)}</th>`).join('')}</tr></thead><tbody>${rows.map((r) => `<tr>${r.map((c) => `<td>${esc(typeof c === 'number' ? fmt(c) : c)}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
}

/** A sparkline for the terminal. */
export function spark(values: readonly number[], width = 50): string {
  const v = values.slice(-width).filter(Number.isFinite);
  if (!v.length) return '';
  const lo = Math.min(...v), hi = Math.max(...v);
  const bars = '▁▂▃▄▅▆▇█';
  return v.map((x) => bars[hi > lo ? Math.min(7, Math.floor(((x - lo) / (hi - lo)) * 8)) : 3]).join('');
}

export { fmt };
