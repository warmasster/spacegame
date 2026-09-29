/**
 * Just enough of a browser for the client's scene builders to run in Node: canvases whose 2D
 * context draws nothing (every call is counted, so a test can see how much a screen redraws), and
 * `location` for the URL switches. Nothing here renders; the perf tool reads the scene graph.
 */

/** Calls made on every fake 2D context since the last reset (fillText, fillRect, …). */
export const canvasCalls = { n: 0 };

function fontPx(font: string) {
  const m = /(\d+(?:\.\d+)?)px/.exec(font);
  return m ? Number(m[1]) : 10;
}

function context2d(canvas: FakeCanvas): CanvasRenderingContext2D {
  const state: Record<string | symbol, unknown> = { canvas, font: '10px sans-serif', fillStyle: '#000', strokeStyle: '#000', lineWidth: 1, textAlign: 'left', textBaseline: 'alphabetic', globalAlpha: 1 };
  const noop = () => {
    canvasCalls.n++;
  };
  return new Proxy(state, {
    get(t, key) {
      if (key === 'measureText') return (text: string) => ({ width: text.length * fontPx(String(t.font)) * 0.6 });
      if (key === 'getImageData') return (_x: number, _y: number, w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4), width: w, height: h });
      if (key === 'createLinearGradient' || key === 'createRadialGradient') return () => ({ addColorStop: noop });
      if (key in t) return t[key];
      return noop;
    },
    set(t, key, v) {
      t[key] = v;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

class FakeCanvas {
  width = 300;
  height = 150;
  style: Record<string, string> = {};
  private ctx: CanvasRenderingContext2D | null = null;
  getContext() {
    return (this.ctx ??= context2d(this));
  }
  addEventListener() {}
  removeEventListener() {}
}

class FakeElement {
  style: Record<string, string> = {};
  children: FakeElement[] = [];
  className = '';
  textContent = '';
  innerHTML = '';
  classList = { add() {}, remove() {}, toggle() {}, contains: () => false };
  appendChild(c: FakeElement) {
    this.children.push(c);
    return c;
  }
  addEventListener() {}
  removeEventListener() {}
  setAttribute() {}
  remove() {}
}

const g = globalThis as Record<string, unknown>;
g.document ??= {
  createElement: (tag: string) => (tag === 'canvas' ? new FakeCanvas() : new FakeElement()),
  createElementNS: (_ns: string, tag: string) => (tag === 'canvas' ? new FakeCanvas() : new FakeElement()),
};
g.location ??= { search: '', protocol: 'http:', host: 'localhost' };
g.window ??= globalThis;
