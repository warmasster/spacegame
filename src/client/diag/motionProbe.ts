// Motion probe (F7): is what the eye sees moving smoothly? For everything drawn that moves (ships,
// loose objects, projectiles, the other astronauts) it takes, every rendered frame, its position
// relative to the camera and compares it with where its last two frames said it would be
// (constant velocity). Smooth motion — however fast: a ship at 1.6 km/s beside you — misses by
// micrometres (only its acceleration shows, a·dt²); a jump of a frame, a stall or a back-and-forth
// shows as the size of the jump, in metres. Alongside, what could cause one: frame changes, the
// physics bubble re-laid, the network correcting a replica, the step clock jumping.
//
// Only while it is on (F7, or `game.motion.enabled = true` for automation) does it cost anything.
// `game.motion.report()` gives the numbers; the panel shows the last seconds and a trace of the
// worst jump of each frame (docs/MOVIMIENTO.md, docs/DIAGNOSTICS.md).

export interface MotionSubject {
  /** Stable key (e.g. `ship:1`, `crate:4`, `shot:12`). */
  key: string;
  /** World position as drawn this frame. */
  x: number;
  y: number;
  z: number;
}

interface Track {
  key: string;
  /** Relative position (to the camera) the last two frames, and when. */
  r1: [number, number, number];
  r2: [number, number, number];
  n: number;
  /** Worst jump in the window and ever (m), and when the window's was. */
  max: number;
  maxAt: number;
  worst: number;
  last: number;
  /** Frame number and time it was last seen. */
  seen: number;
  seenT: number;
}

export interface MotionEvent {
  t: number;
  kind: string;
  text: string;
}

/** A jump bigger than this (m) is flagged (a smooth thing moves by far less than this off its line). */
const FLAG_M = 0.05;
/** Window of the panel and of the maxima (s). */
const WINDOW_S = 4;
const TRACE = 240;

export class MotionProbe {
  enabled = false;
  readonly el: HTMLDivElement;
  private text: HTMLPreElement;
  private canvas: HTMLCanvasElement;
  private tracks = new Map<string, Track>();
  private events: MotionEvent[] = [];
  private trace = new Float32Array(TRACE);
  private traceKey: string[] = new Array(TRACE).fill('');
  private head = 0;
  private dt1 = 0;
  private time = 0;
  private frameNo = 0;
  private shownT = 0;
  /** Extra lines (the game's own counters), refreshed with the panel. */
  info: () => Record<string, string | number> = () => ({});

  constructor(parent: HTMLElement) {
    this.el = document.createElement('div');
    this.el.className = 'debug-overlay motion-probe hidden';
    Object.assign(this.el.style, { left: 'auto', right: '28px', top: 'auto', bottom: '120px', width: '360px', pointerEvents: 'none' });
    this.canvas = document.createElement('canvas');
    this.canvas.width = 336;
    this.canvas.height = 70;
    Object.assign(this.canvas.style, { display: 'block', width: '336px', height: '70px', marginBottom: '6px' });
    this.text = document.createElement('pre');
    Object.assign(this.text.style, { margin: '0', whiteSpace: 'pre', fontSize: '10.5px', lineHeight: '1.45' });
    this.el.append(this.canvas, this.text);
    parent.appendChild(this.el);
    window.addEventListener('keydown', (e) => {
      if (e.code !== 'F7') return;
      e.preventDefault();
      this.enabled = !this.enabled;
      this.el.classList.toggle('hidden', !this.enabled);
      if (this.enabled) this.reset();
    });
  }

  reset() {
    this.tracks.clear();
    this.events.length = 0;
    this.trace.fill(0);
    this.traceKey.fill('');
    this.dt1 = 0;
  }

  /** Something that may cause a jump happened (a frame change, a re-laying, a network correction). */
  event(kind: string, text: string) {
    if (!this.enabled) return;
    this.events.push({ t: this.time, kind, text });
    if (this.events.length > 40) this.events.shift();
  }

  /**
   * One rendered frame: `dt` (s) since the last, the camera (world) and every subject as drawn.
   * Call it after everything is placed for the frame, before rendering.
   */
  frame(dt: number, cam: { x: number; y: number; z: number }, subjects: Iterable<MotionSubject>) {
    if (!this.enabled) return;
    this.time += dt;
    this.frameNo++;
    let worst = 0;
    let worstKey = '';
    const k = this.dt1 > 1e-5 ? dt / this.dt1 : 1;
    for (const s of subjects) {
      let tr = this.tracks.get(s.key);
      if (!tr) this.tracks.set(s.key, (tr = { key: s.key, r1: [0, 0, 0], r2: [0, 0, 0], n: 0, max: 0, maxAt: 0, worst: 0, last: 0, seen: -9, seenT: 0 }));
      const rx = s.x - cam.x, ry = s.y - cam.y, rz = s.z - cam.z;
      if (tr.n >= 2 && tr.seen === this.frameNo - 1) {
        // where the last two frames said it would be now (constant velocity)
        const px = tr.r1[0] + (tr.r1[0] - tr.r2[0]) * k;
        const py = tr.r1[1] + (tr.r1[1] - tr.r2[1]) * k;
        const pz = tr.r1[2] + (tr.r1[2] - tr.r2[2]) * k;
        const e = Math.hypot(rx - px, ry - py, rz - pz);
        tr.last = e;
        if (e > tr.worst) tr.worst = e;
        if (e >= tr.max || this.time - tr.maxAt > WINDOW_S) {
          tr.max = e;
          tr.maxAt = this.time;
        }
        if (e > worst) {
          worst = e;
          worstKey = s.key;
        }
      } else if (tr.seen !== this.frameNo - 1) tr.n = 0;
      tr.r2[0] = tr.r1[0];
      tr.r2[1] = tr.r1[1];
      tr.r2[2] = tr.r1[2];
      tr.r1[0] = rx;
      tr.r1[1] = ry;
      tr.r1[2] = rz;
      tr.n++;
      tr.seen = this.frameNo;
      tr.seenT = this.time;
    }
    this.dt1 = dt;
    this.trace[this.head] = worst;
    this.traceKey[this.head] = worstKey;
    this.head = (this.head + 1) % TRACE;
    if (worst > FLAG_M) this.event('salto', `${worstKey}: ${fmt(worst)}`);
    if (this.time - this.shownT > 0.2) {
      this.shownT = this.time;
      this.draw();
    }
  }

  /** The numbers: worst jump per subject in the window and ever, and the recent events. */
  report() {
    const subjects = [...this.tracks.values()].map((t) => ({ key: t.key, window: t.max, worst: t.worst, last: t.last })).sort((a, b) => b.worst - a.worst);
    return { time: this.time, flag: FLAG_M, subjects, events: this.events.slice() };
  }

  private draw() {
    const now = this.time;
    const rows = [...this.tracks.values()].filter((t) => now - t.seenT < 0.5).sort((a, b) => b.max - a.max).slice(0, 10);
    const lines = ['MOVIMIENTO (F7) — salto frente a la cámara por fotograma', `umbral ${fmt(FLAG_M)} · ventana ${WINDOW_S} s`, ''];
    for (const t of rows) lines.push(`${t.max > FLAG_M ? '!' : ' '} ${t.key.padEnd(16)} ${fmt(t.max).padStart(9)}  peor ${fmt(t.worst)}`);
    const info = this.info();
    if (Object.keys(info).length) lines.push('');
    for (const [k, v] of Object.entries(info)) lines.push(`${k.padEnd(18)} ${v}`);
    lines.push('', 'eventos:');
    for (const e of this.events.slice(-8)) lines.push(`${(now - e.t).toFixed(1).padStart(5)} s  ${e.kind}: ${e.text}`);
    this.text.textContent = lines.join('\n');
    // the trace: the worst jump of each frame, log scale (1 mm … 10 m), the flag line
    const c = this.canvas.getContext('2d');
    if (!c) return;
    const W = this.canvas.width, H = this.canvas.height;
    c.clearRect(0, 0, W, H);
    c.fillStyle = 'rgba(0,20,30,0.6)';
    c.fillRect(0, 0, W, H);
    const y = (m: number) => H - (Math.max(0, Math.min(4, Math.log10(Math.max(m, 1e-3)) + 3)) / 4) * H;
    c.strokeStyle = 'rgba(255,120,80,0.8)';
    c.beginPath();
    c.moveTo(0, y(FLAG_M));
    c.lineTo(W, y(FLAG_M));
    c.stroke();
    for (let i = 0; i < TRACE; i++) {
      const v = this.trace[(this.head + i) % TRACE];
      const x = (i / TRACE) * W;
      c.fillStyle = v > FLAG_M ? '#ff7a50' : '#6fe8c8';
      const top = y(v);
      c.fillRect(x, top, Math.max(1, W / TRACE), H - top);
    }
    c.fillStyle = '#9fd';
    c.font = '9px monospace';
    c.fillText('10 m', 2, 9);
    c.fillText('1 mm', 2, H - 2);
  }
}

const fmt = (m: number) => (m >= 1 ? `${m.toFixed(2)} m` : m >= 0.01 ? `${(m * 100).toFixed(1)} cm` : `${(m * 1000).toFixed(1)} mm`);
