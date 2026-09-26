"""
Sky assets from real data:
  * stars.bin      — ~9000 naked-eye stars (d3-celestial / HYG, mag ≤ 6): float32 [x, y, z, mag, bv] in the
                     equatorial frame (X→RA 0h, Y→RA 6h, Z→north celestial pole).
  * milkyway.jpg   — equirectangular Milky Way brightness, modelled in the real galactic frame.
  * earth_*.jpg    — NASA Blue Marble derived Earth maps (via the three.js examples), resized.

    python tools/sky/build_sky.py  [--out public/assets/sky]
"""
import argparse
import io
import json
import os
import urllib.request

import numpy as np
from PIL import Image

CACHE = os.path.join(os.path.dirname(__file__), '..', '.cache')
D3C = 'https://raw.githubusercontent.com/ofrohn/d3-celestial/master/data/'
THREE = 'https://raw.githubusercontent.com/mrdoob/three.js/dev/examples/textures/planets/'


def fetch(url):
    os.makedirs(CACHE, exist_ok=True)
    path = os.path.join(CACHE, url.rsplit('/', 1)[1])
    if not os.path.exists(path):
        print('download', url)
        with urllib.request.urlopen(url, timeout=120) as r, open(path, 'wb') as f:
            f.write(r.read())
    return path


def build_stars(out):
    data = json.load(open(fetch(D3C + 'stars.6.json')))
    rows = []
    for ft in data['features']:
        ra, dec = ft['geometry']['coordinates']
        mag = float(ft['properties']['mag'])
        try:
            bv = float(ft['properties'].get('bv', 0.6))
        except (TypeError, ValueError):
            bv = 0.6
        a, d = np.radians(ra), np.radians(dec)
        rows.append((np.cos(d) * np.cos(a), np.cos(d) * np.sin(a), np.sin(d), mag, bv))
    arr = np.array(rows, dtype=np.float32)
    arr = arr[np.argsort(arr[:, 3])]
    arr.tofile(os.path.join(out, 'stars.bin'))
    print('stars', len(arr), 'brightest mag', arr[0, 3])


# J2000 equatorial → galactic rotation
EQ2GAL = np.array([[-0.0548755604, -0.8734370902, -0.4838350155],
                   [0.4941094279, -0.4448296300, 0.7469822445],
                   [-0.8676661490, -0.1980763734, 0.4559837762]])


def periodic_noise(h, w, beta, rng, aspect=1.0):
    fx = np.fft.fftfreq(h)[:, None] * h
    fy = np.fft.fftfreq(w)[None, :] * w
    f = np.sqrt(fx ** 2 + (fy * aspect) ** 2)
    f[0, 0] = 1
    spec = f ** (-beta / 2) * np.exp(1j * rng.uniform(0, 2 * np.pi, (h, w)))
    n = np.real(np.fft.ifft2(spec))
    return (n - n.mean()) / n.std()


def build_milkyway(out, w=2048, h=1024):
    """Procedural Milky Way placed with the real galactic frame (bulge, Great Rift, Magellanic Clouds)."""
    ra = (np.arange(w) + 0.5) / w * 2 * np.pi - np.pi          # same convention as the stars (-180..180)
    dec = np.pi / 2 - (np.arange(h) + 0.5) / h * np.pi
    RA, DEC = np.meshgrid(ra, dec)
    eq = np.stack([np.cos(DEC) * np.cos(RA), np.cos(DEC) * np.sin(RA), np.sin(DEC)], axis=-1)
    gal = eq @ EQ2GAL.T
    l = np.degrees(np.arctan2(gal[..., 1], gal[..., 0]))
    b = np.degrees(np.arcsin(np.clip(gal[..., 2], -1, 1)))
    rng = np.random.default_rng(3)
    n1 = periodic_noise(h, w, 3.0, rng, 0.5)
    n2 = periodic_noise(h, w, 2.3, rng, 0.5)
    sigma = 4.5 + 8.0 * np.exp(-(l / 38.0) ** 2) + 1.2 * n1
    band = np.exp(-(b / np.maximum(sigma, 2.0)) ** 2)
    inten = 0.32 + 0.68 * np.exp(-(l / 70.0) ** 2)
    bulge = 0.9 * np.exp(-((l / 14.0) ** 2 + (b / 9.0) ** 2))
    mw = band * inten + bulge
    # Great Rift and other dust lanes
    rift_mask = np.exp(-((l - 10) / 48.0) ** 2)
    rift = np.exp(-((b - 1.2 - 1.5 * n1) / (1.8 + 0.8 * np.abs(n2))) ** 2) * rift_mask
    mw *= 1 - 0.75 * np.clip(rift, 0, 1)
    mw *= np.clip(0.72 + 0.32 * n2 + 0.18 * n1, 0.15, 1.6)
    # Magellanic Clouds
    for ra0, dec0, rad, amp in ((80.9, -69.8, 4.0, 0.55), (13.2, -72.8, 2.2, 0.35)):
        r0 = np.radians(ra0 if ra0 <= 180 else ra0 - 360)
        c = np.array([np.cos(np.radians(dec0)) * np.cos(r0), np.cos(np.radians(dec0)) * np.sin(r0), np.sin(np.radians(dec0))])
        ang = np.degrees(np.arccos(np.clip(eq @ c, -1, 1)))
        mw += amp * np.exp(-(ang / rad) ** 2) * np.clip(0.8 + 0.3 * n2, 0.3, 1.3)
    mw = np.clip(mw / np.percentile(mw, 99.8), 0, 1) ** 1.15
    Image.fromarray((mw * 255).astype(np.uint8), 'L').save(os.path.join(out, 'milkyway.jpg'), quality=90)
    print('milky way', w, h)


def build_earth(out, size=1024):
    day = Image.open(fetch(THREE + 'earth_day_4096.jpg')).convert('RGB').resize((size * 2, size), Image.LANCZOS)
    day.save(os.path.join(out, 'earth_day.jpg'), quality=90)
    night = Image.open(fetch(THREE + 'earth_night_4096.jpg')).convert('RGB').resize((size * 2, size), Image.LANCZOS)
    night.save(os.path.join(out, 'earth_night.jpg'), quality=88)
    brc = Image.open(fetch(THREE + 'earth_bump_roughness_clouds_4096.jpg')).convert('RGB').resize((size * 2, size), Image.LANCZOS)
    r, g, b = brc.split()
    # R: bump, G: roughness (ocean glint mask), B: clouds
    Image.merge('RGB', (b, g, r)).save(os.path.join(out, 'earth_clouds.jpg'), quality=90)
    print('earth', size)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', default='public/assets/sky')
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    build_stars(a.out)
    build_milkyway(a.out)
    build_earth(a.out)


if __name__ == '__main__':
    main()
