/** Keyboard + mouse (pointer lock) input. Movement keys only; ship systems will be clicked. */
export class Input {
  private keys = new Set<string>();
  private pressed = new Set<string>();
  private dx = 0;
  private dy = 0;
  sensitivity = 0.0022;
  locked = false;

  constructor(private element: HTMLElement) {
    window.addEventListener('keydown', (e) => {
      if (e.repeat) return;
      if (!this.locked && e.code !== 'Escape') return;
      this.keys.add(e.code);
      this.pressed.add(e.code);
      if (['Space', 'Tab'].includes(e.code)) e.preventDefault();
    });
    window.addEventListener('keyup', (e) => this.keys.delete(e.code));
    window.addEventListener('blur', () => this.keys.clear());
    document.addEventListener('pointerlockchange', () => {
      this.locked = document.pointerLockElement === this.element;
      if (!this.locked) this.keys.clear();
    });
    // mouse buttons as pseudo keys: Mouse0 (left), Mouse2 (right)
    window.addEventListener('mousedown', (e) => {
      if (!this.locked) return;
      const code = `Mouse${e.button}`;
      if (!this.keys.has(code)) this.pressed.add(code);
      this.keys.add(code);
    });
    window.addEventListener('mouseup', (e) => this.keys.delete(`Mouse${e.button}`));
    window.addEventListener('contextmenu', (e) => {
      if (this.locked) e.preventDefault();
    });
    document.addEventListener('mousemove', (e) => {
      if (!this.locked) return;
      // clamp spikes some browsers produce when locking
      this.dx += Math.max(-300, Math.min(300, e.movementX));
      this.dy += Math.max(-300, Math.min(300, e.movementY));
    });
  }

  lock() {
    const req = this.element.requestPointerLock as (o?: { unadjustedMovement?: boolean }) => Promise<void> | void;
    try {
      const p = req.call(this.element, { unadjustedMovement: true });
      if (p && typeof (p as Promise<void>).catch === 'function') (p as Promise<void>).catch(() => this.element.requestPointerLock());
    } catch {
      this.element.requestPointerLock();
    }
  }

  /** Programmatic input (automation/tests). */
  setKey(code: string, isDown: boolean) {
    if (isDown) {
      if (!this.keys.has(code)) this.pressed.add(code);
      this.keys.add(code);
    } else this.keys.delete(code);
  }

  addLook(dx: number, dy: number) {
    this.dx += dx;
    this.dy += dy;
  }

  down(code: string) {
    return this.keys.has(code);
  }

  /** True once per key press. */
  consume(code: string) {
    const had = this.pressed.has(code);
    this.pressed.delete(code);
    return had;
  }

  /** Mouse delta since last call (radians). */
  look(out?: [number, number]): [number, number] {
    const r: [number, number] = out ?? [0, 0];
    r[0] = this.dx * this.sensitivity;
    r[1] = this.dy * this.sensitivity;
    this.dx = this.dy = 0;
    return r;
  }

  endFrame() {
    this.pressed.clear();
  }
}
