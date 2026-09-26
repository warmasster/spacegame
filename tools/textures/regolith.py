"""
Tileable lunar regolith detail textures (albedo + normal/height), generated procedurally.

    python tools/textures/regolith.py  [--size 1024] [--out public/assets/tex]

Tile ≈ 2 m (≈2 mm/px). Everything is periodic (FFT noise, wrapped stamps) so it tiles seamlessly.
"""
import argparse
import os

import numpy as np
from PIL import Image


def fft_noise(n, beta, rng, fmin=1.0, fmax=None):
    """Periodic noise with power spectrum ~ 1/f^beta."""
    fx = np.fft.fftfreq(n) * n
    f = np.sqrt(fx[:, None] ** 2 + fx[None, :] ** 2)
    f[0, 0] = 1.0
    amp = f ** (-beta / 2.0)
    amp[f < fmin] = 0
    if fmax:
        amp *= np.exp(-(f / fmax) ** 2)
    phase = rng.uniform(0, 2 * np.pi, (n, n))
    spec = amp * np.exp(1j * phase)
    out = np.real(np.fft.ifft2(spec))
    out -= out.mean()
    return out / (out.std() + 1e-9)


def stamp(field, cx, cy, radius, fn):
    """Add fn(r) (r normalised to radius) around (cx, cy) with wrap-around."""
    n = field.shape[0]
    R = int(np.ceil(radius * 2.6)) + 1
    icx, icy = int(cx), int(cy)
    ys = np.arange(icy - R, icy + R + 1) % n
    xs = np.arange(icx - R, icx + R + 1) % n
    dy = np.arange(-R, R + 1)[:, None] + (int(cy) - cy)
    dx = np.arange(-R, R + 1)[None, :] + (int(cx) - cx)
    r = np.sqrt(dx * dx + dy * dy) / radius
    field[np.ix_(ys, xs)] += fn(r)


def crater_fn(depth):
    def f(r):
        bowl = np.where(r < 1, (r * r - 1) * depth, 0.0)
        rim = np.exp(-((r - 1.0) / 0.28) ** 2) * depth * 0.35
        ejecta = np.where(r > 1, np.exp(-(r - 1) * 3.0) * depth * 0.08, 0)
        return bowl + rim + ejecta
    return f


def pebble_fn(height, sharp):
    def f(r):
        return height * np.clip(1 - r ** 2, 0, None) ** sharp
    return f


def normal_from_height(h, strength):
    dx = (np.roll(h, -1, axis=1) - np.roll(h, 1, axis=1)) * 0.5
    dy = (np.roll(h, -1, axis=0) - np.roll(h, 1, axis=0)) * 0.5
    nx, ny, nz = -dx * strength, dy * strength, np.ones_like(h)
    l = np.sqrt(nx * nx + ny * ny + nz * nz)
    return np.stack([nx / l, ny / l, nz / l], axis=-1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--size', type=int, default=1024)
    ap.add_argument('--out', default='public/assets/tex')
    a = ap.parse_args()
    n = a.size
    rng = np.random.default_rng(1969)
    os.makedirs(a.out, exist_ok=True)

    # height in "pixels" units (1 px ≈ 2 mm)
    h = 2.2 * fft_noise(n, 2.4, rng, fmin=2) + 0.9 * fft_noise(n, 1.6, rng, fmin=8) + 0.35 * fft_noise(n, 0.8, rng, fmin=40)
    alb = 0.03 * fft_noise(n, 2.0, rng, fmin=1) + 0.018 * fft_noise(n, 1.0, rng, fmin=16)
    s = n / 1024.0
    # micro craters
    for _ in range(int(70 * s * s)):
        r = rng.uniform(6, 70) * s
        cx, cy = rng.uniform(0, n, 2)
        stamp(h, cx, cy, r, crater_fn(r * 0.22))
        stamp(alb, cx, cy, r, lambda rr: np.where(rr < 0.9, -0.012, 0.0) + np.exp(-((rr - 1.05) / 0.25) ** 2) * 0.02)
    # pebbles and clods
    for _ in range(int(900 * s * s)):
        r = rng.uniform(1.5, 5.0) * s * (1 + 3 * rng.random() ** 6)
        cx, cy = rng.uniform(0, n, 2)
        stamp(h, cx, cy, r, pebble_fn(r * rng.uniform(0.5, 0.95), rng.uniform(0.35, 0.8)))
        tone = rng.normal(0, 0.05)
        stamp(alb, cx, cy, r * 1.05, lambda rr, t=tone: np.where(rr < 1, t, 0.0))
    # fine grain sparkle / dust
    alb += 0.012 * rng.normal(size=(n, n))

    nm = normal_from_height(h, 0.55)
    rgb = ((nm * 0.5 + 0.5) * 255).clip(0, 255).astype(np.uint8)
    Image.fromarray(rgb, 'RGB').save(os.path.join(a.out, 'regolith_n.jpg'), quality=94)

    base = 0.5 + alb
    # cavity darkening
    cav = h - (np.roll(h, 3, 0) + np.roll(h, -3, 0) + np.roll(h, 3, 1) + np.roll(h, -3, 1)) / 4
    base += cav * 0.012
    base = base.clip(0.3, 0.75)
    g = (base * 255).astype(np.uint8)
    rgbA = np.stack([g, g, (base * 0.985 * 255).astype(np.uint8)], axis=-1)
    Image.fromarray(rgbA, 'RGB').save(os.path.join(a.out, 'regolith_a.jpg'), quality=90)

    # large-scale macro variation (tileable, ~150 m) for breaking repetition
    m = n // 2
    macro = 0.5 + 0.18 * fft_noise(m, 2.2, rng, fmin=1) + 0.06 * fft_noise(m, 1.2, rng, fmin=6)
    Image.fromarray((macro.clip(0, 1) * 255).astype(np.uint8), 'L').save(os.path.join(a.out, 'macro.png'), optimize=True)
    print('ok', a.out)


if __name__ == '__main__':
    main()
