"""Bake seamless lunar tone (20 m .. 2.56 km) and independent texture warp axes.

python tools/textures/macro.py
Only one RGB lookup at runtime; mipmaps remove detail naturally with distance.
"""
from pathlib import Path
import numpy as np
from PIL import Image
from regolith import fft_noise


def build(size=1024):
    rng = np.random.default_rng(7331)
    # Low-pass displacement: decorrelate repeated detail without sharp cell borders.
    warp_x = 0.5 + 0.12 * fft_noise(size, 3.2, rng, fmin=1, fmax=18)
    warp_y = 0.5 + 0.12 * fft_noise(size, 3.2, rng, fmin=1, fmax=18)
    tone = (0.5 + 0.12 * fft_noise(size, 2.8, rng, fmin=1, fmax=5)
            + 0.10 * fft_noise(size, 2.4, rng, fmin=5, fmax=32)
            + 0.05 * fft_noise(size, 1.8, rng, fmin=32, fmax=128))
    data = np.stack([warp_x, warp_y, tone], axis=-1).clip(0, 1)
    # Assert periodic neighbour differences are comparable to those inside the texture.
    for axis in [0, 1]:
        delta = np.abs(data - np.roll(data, 1, axis))
        edge = np.take(delta, 0, axis=axis)
        assert edge.mean() < delta.mean() * 1.8, "discontinuous tile edge"
    out = Path(__file__).resolve().parents[2] / 'public/assets/tex/regolith_macro.png'
    Image.fromarray(np.round(data * 255).astype(np.uint8)).save(out, optimize=True)
    print(f'OK seamless macro texture: {out} ({out.stat().st_size / 1024:.0f} KB)')


if __name__ == '__main__':
    build()
