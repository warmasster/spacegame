"""Acabados: tileable surface textures for structures and ships (crates/render, structure.wgsl).

Each finish is a 512x512 tile made from a height field and a few masks, all periodic so it
repeats without seams. Written as one RGBA8 layer per finish, in the order of
`lunar_core::mesh::FINISHES` (index 1 is the first layer):

  R  albedo factor (128 = x1.0), the finish's own light and dark
  G  roughness offset (128 = none)
  B  normal x along the tile's u (from the height field)
  A  normal y along the tile's v (rows)

Writes assets/textures/acabados.bin (raw layers), acabados.json (size and names) and
acabados.png (a contact sheet to look at).

    python tools/texturas/acabados.py
"""
import json
import os

import numpy as np
from PIL import Image

N = 512
ROOT = os.path.join(os.path.dirname(__file__), "..", "..")
OUT = os.path.join(ROOT, "assets", "textures")

# order matters: lunar_core::mesh::FINISHES[1..]
NAMES = [
    "pintura", "cepillado", "fundicion", "goma", "plastico", "tela", "lagrimado", "perforado",
    "rejilla", "acolchado", "carbono", "hormigon", "trenzado", "corrugado", "madera", "aislante",
]

yy, xx = np.mgrid[0:N, 0:N].astype(np.float32) / N  # 0..1, rows (v) then columns (u)


def rng(seed):
    return np.random.default_rng(seed)


def periodic_noise(seed, cutoff, aniso=(1.0, 1.0)):
    """Smooth periodic noise: white noise low-passed in frequency (cutoff in cycles per tile)."""
    w = rng(seed).standard_normal((N, N)).astype(np.float32)
    f = np.fft.fft2(w)
    ky = np.fft.fftfreq(N)[:, None] * N / aniso[1]
    kx = np.fft.fftfreq(N)[None, :] * N / aniso[0]
    k = np.sqrt(kx * kx + ky * ky)
    f *= np.exp(-(k / cutoff) ** 2)
    out = np.real(np.fft.ifft2(f))
    out -= out.min()
    return out / max(out.max(), 1e-9)


def fbm(seed, base, octaves=4, gain=0.5, aniso=(1.0, 1.0)):
    acc = np.zeros((N, N), np.float32)
    amp, total = 1.0, 0.0
    for o in range(octaves):
        acc += amp * periodic_noise(seed + o * 101, base * 2 ** o, aniso)
        total += amp
        amp *= gain
    return acc / total


def tri(x):
    """Triangle wave 0..1..0 over each unit."""
    f = x - np.floor(x)
    return 1.0 - np.abs(f * 2.0 - 1.0)


def smooth(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def cells(seed, count):
    """Periodic Voronoi: distance to the nearest and second nearest of `count` points (in tiles)."""
    pts = rng(seed).random((count, 2)).astype(np.float32)
    f1 = np.full((N, N), 9.0, np.float32)
    f2 = np.full((N, N), 9.0, np.float32)
    idx = np.zeros((N, N), np.int32)
    for i, (px, py) in enumerate(pts):
        dx = np.abs(xx - px)
        dx = np.minimum(dx, 1.0 - dx)
        dy = np.abs(yy - py)
        dy = np.minimum(dy, 1.0 - dy)
        d = np.sqrt(dx * dx + dy * dy)
        closer = d < f1
        f2 = np.where(closer, f1, np.minimum(f2, d))
        idx = np.where(closer, i, idx)
        f1 = np.where(closer, d, f1)
    return f1, f2, idx


def scratches(seed, count, length, depth):
    """Thin straight grooves at random (periodic)."""
    r = rng(seed)
    h = np.zeros((N, N), np.float32)
    for _ in range(count):
        x0, y0 = r.random(2)
        a = r.random() * np.pi
        L = length * (0.3 + r.random())
        steps = int(L * N * 2) + 2
        t = np.linspace(0, L, steps)
        xs = ((x0 + np.cos(a) * t) * N).astype(int) % N
        ys = ((y0 + np.sin(a) * t) * N).astype(int) % N
        h[ys, xs] -= depth * (0.4 + r.random())
    return h


def normals(h, strength):
    """Tangent normals of a periodic height field (h in tile units scaled by strength)."""
    dx = (np.roll(h, -1, axis=1) - np.roll(h, 1, axis=1)) * 0.5 * strength
    dy = (np.roll(h, -1, axis=0) - np.roll(h, 1, axis=0)) * 0.5 * strength
    n = np.stack([-dx, -dy, np.ones_like(h)], axis=-1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return n


def pack(albedo, rough, h, strength):
    n = normals(h, strength)
    r = np.clip(albedo * 128.0, 0, 255)
    g = np.clip(128.0 + rough * 128.0, 0, 255)
    b = np.clip(n[..., 0] * 127.5 + 127.5, 0, 255)
    a = np.clip(n[..., 1] * 127.5 + 127.5, 0, 255)
    return np.stack([r, g, b, a], axis=-1).astype(np.uint8)


# ---------------------------------------------------------------- finishes

def pintura():
    peel = fbm(1, 48, 3)  # orange peel
    smudge = fbm(2, 3, 3)
    h = peel * 0.25 + scratches(3, 28, 0.1, 0.22)
    albedo = 1.0 + (fbm(4, 6, 3) - 0.5) * 0.06 - smooth(0.6, 0.95, smudge) * 0.05
    rough = (smudge - 0.5) * 0.35
    return pack(albedo, rough, h, 6.0)


def cepillado():
    streak = fbm(10, 64, 3, aniso=(40.0, 1.0))  # long along u
    fine = periodic_noise(11, 200, aniso=(60.0, 1.0))
    h = streak * 0.6 + fine * 0.4
    albedo = 0.94 + streak * 0.1 + (fbm(12, 4, 2) - 0.5) * 0.05
    rough = (streak - 0.5) * 0.35 - 0.05
    return pack(albedo, rough, h, 4.0)


def fundicion():
    base = fbm(20, 8, 5)
    f1, _, _ = cells(21, 260)
    pits = smooth(0.012, 0.0, f1) * 0.9
    h = base * 1.2 - pits
    albedo = 0.85 + base * 0.25 - pits * 0.35
    rough = 0.15 + pits * 0.3 - (base - 0.5) * 0.2
    return pack(albedo, rough, h, 9.0)


def goma():
    stipple = periodic_noise(30, 160)
    scuff = smooth(0.62, 0.9, fbm(31, 5, 4))
    h = stipple * 0.6 - scuff * 0.2
    albedo = 0.92 + stipple * 0.12 + scuff * 0.25
    rough = 0.25 - scuff * 0.3
    return pack(albedo, rough, h, 7.0)


def plastico():
    f1, f2, _ = cells(40, 1600)
    ridge = smooth(0.0, 0.006, f2 - f1)
    h = ridge * 0.5 + periodic_noise(41, 120) * 0.2
    albedo = 0.97 + ridge * 0.05
    rough = (1.0 - ridge) * 0.12
    return pack(albedo, rough, h, 6.0)


def tela():
    k = 24  # threads per tile
    u, v = xx * k, yy * k
    warp = np.sin(np.pi * u) ** 2
    weft = np.sin(np.pi * v) ** 2
    over = (np.floor(u) + np.floor(v)) % 2
    h = np.where(over > 0, warp * 0.8 + weft * 0.2, weft * 0.8 + warp * 0.2)
    fuzz = periodic_noise(50, 180)
    h = h + fuzz * 0.15
    albedo = 0.8 + h * 0.25 + (fbm(51, 5, 3) - 0.5) * 0.08
    rough = 0.3 - h * 0.05
    return pack(albedo, rough, h, 3.0)


def lagrimado():
    k = 12  # lozenges per tile each way
    u, v = xx * k, yy * k
    cu, cv = np.floor(u), np.floor(v)
    fu, fv = u - cu - 0.5, v - cv - 0.5
    turn = (cu + cv) % 2
    a = np.where(turn > 0, fu, fv)
    b = np.where(turn > 0, fv, fu)
    # an elongated lozenge, rounded
    d = (np.abs(a) / 0.42) ** 1.6 + (np.abs(b) / 0.13) ** 1.6
    bar = smooth(1.15, 0.75, d)
    plate = fbm(60, 6, 3)
    h = bar * 1.0 + plate * 0.08 + scratches(61, 20, 0.12, 0.2) * 0.5
    worn = smooth(0.55, 0.9, fbm(62, 4, 3))
    albedo = 0.9 + bar * (0.12 + worn * 0.2) + (plate - 0.5) * 0.08
    rough = -bar * (0.15 + worn * 0.25) + 0.05
    return pack(albedo, rough, h, 10.0)


def perforado():
    k = 16
    u, v = xx * k, yy * k
    # staggered rows of round holes
    v2 = v
    u2 = u + (np.floor(v2) % 2) * 0.5
    fu, fv = u2 - np.floor(u2) - 0.5, v2 - np.floor(v2) - 0.5
    r = np.sqrt(fu * fu + fv * fv)
    hole = smooth(0.3, 0.26, r)
    rim = smooth(0.26, 0.3, r) * smooth(0.36, 0.3, r)
    h = -hole * 1.2 + rim * 0.15 + fbm(70, 8, 2) * 0.05
    albedo = 1.0 - hole * 0.85 + rim * 0.05
    rough = hole * 0.4
    return pack(albedo, rough, h, 8.0)


def rejilla():
    k = 10  # slots per tile
    v = yy * k
    f = v - np.floor(v)
    # a louvre: a slot, then a lip leaning out
    slot = smooth(0.18, 0.12, np.abs(f - 0.3))
    lip = smooth(0.45, 0.85, f) * smooth(1.0, 0.85, f)
    ends = smooth(0.04, 0.08, np.minimum((xx * 4) % 1.0, 1.0 - (xx * 4) % 1.0))
    h = (lip * 0.7 - slot * 1.0) * ends
    albedo = 1.0 - slot * ends * 0.85
    rough = slot * 0.3
    return pack(albedo, rough, h, 9.0)


def acolchado():
    k = 6
    u, v = (xx + yy) * k, (xx - yy) * k  # diamond quilting
    fu, fv = u - np.floor(u) - 0.5, v - np.floor(v) - 0.5
    pillow = np.cos(fu * np.pi) * np.cos(fv * np.pi)
    stitch = smooth(0.06, 0.0, np.minimum(0.5 - np.abs(fu), 0.5 - np.abs(fv)))
    weave = periodic_noise(80, 220) * 0.08
    h = pillow * 0.9 - stitch * 0.4 + weave
    albedo = 0.86 + pillow * 0.12 - stitch * 0.25
    rough = 0.2 + stitch * 0.1
    return pack(albedo, rough, h, 6.0)


def carbono():
    k = 32
    u, v = xx * k, yy * k
    band = (np.floor(u) + np.floor(v * 0.5)) % 2  # 2x2 twill
    warp = np.sin(np.pi * u) ** 2
    weft = np.sin(np.pi * v) ** 2
    h = np.where(band > 0, warp, weft) * 0.4
    albedo = np.where(band > 0, 0.85 + warp * 0.3, 0.75 + weft * 0.2)
    rough = np.where(band > 0, -0.25, -0.1)
    return pack(albedo, rough, h, 3.0)


def hormigon():
    base = fbm(90, 5, 6, 0.55)
    f1, _, _ = cells(91, 500)
    pores = smooth(0.006, 0.0, f1)
    stains = smooth(0.55, 0.85, fbm(92, 3, 4))
    h = base * 0.8 - pores * 0.8
    albedo = 0.85 + base * 0.25 - pores * 0.4 - stains * 0.15
    rough = 0.25 + pores * 0.2
    return pack(albedo, rough, h, 6.0)


def trenzado():
    k = 16
    a = (xx + yy) * k
    b = (xx - yy) * k
    sa = np.sin(np.pi * a) ** 2
    sb = np.sin(np.pi * b) ** 2
    over = (np.floor(a) + np.floor(b)) % 2
    h = np.where(over > 0, sa, sb)
    albedo = 0.8 + h * 0.3
    rough = -h * 0.15
    return pack(albedo, rough, h, 4.0)


def corrugado():
    k = 12
    wave = 0.5 + 0.5 * np.sin(xx * k * 2 * np.pi)
    dent = fbm(110, 4, 3)
    h = wave * 1.0 + dent * 0.1
    albedo = 0.95 + (dent - 0.5) * 0.1
    rough = (dent - 0.5) * 0.2
    return pack(albedo, rough, h, 9.0)


def madera():
    planks = 4
    v = yy * planks
    row = np.floor(v)
    seam = smooth(0.02, 0.0, np.minimum(v - row, 1.0 - (v - row)))
    shift = (row * 0.37) % 1.0
    grain = fbm(120, 6, 4, aniso=(30.0, 1.0))
    rings = 0.5 + 0.5 * np.sin((yy * 60 + grain * 6 + shift * 10) * np.pi)
    h = grain * 0.4 - seam * 1.0 + rings * 0.1
    tone = 0.75 + ((row * 0.618) % 1.0) * 0.3
    albedo = tone * (0.85 + rings * 0.15 + grain * 0.1) * (1.0 - seam * 0.6)
    rough = 0.1 + seam * 0.3 - rings * 0.05
    return pack(albedo, rough, h, 5.0)


def aislante():
    f1, f2, _ = cells(130, 140)
    crease = smooth(0.0, 0.02, f2 - f1)
    wrinkle = fbm(131, 10, 4)
    h = crease * 0.6 + wrinkle * 0.5
    albedo = 0.85 + wrinkle * 0.3
    rough = -0.25 + (1.0 - crease) * 0.2
    return pack(albedo, rough, h, 8.0)


def main():
    os.makedirs(OUT, exist_ok=True)
    layers = []
    for name in NAMES:
        tex = globals()[name]()
        assert tex.shape == (N, N, 4), name
        layers.append(tex)
        print(f"{name:<11} ok")
    data = np.stack(layers)
    data.tofile(os.path.join(OUT, "acabados.bin"))
    with open(os.path.join(OUT, "acabados.json"), "w", encoding="utf-8") as f:
        json.dump({"size": N, "capas": NAMES}, f, ensure_ascii=False)
    # contact sheet: albedo factor shaded by the normals' light from the upper left
    sheet = Image.new("RGB", (N // 2 * 4, N // 2 * ((len(NAMES) + 3) // 4)))
    for i, t in enumerate(layers):
        nx = t[..., 2].astype(np.float32) / 127.5 - 1.0
        ny = t[..., 3].astype(np.float32) / 127.5 - 1.0
        nz = np.sqrt(np.clip(1.0 - nx * nx - ny * ny, 0.0, 1.0))
        light = np.clip(-nx * 0.5 - ny * 0.5 + nz * 0.7, 0.0, 1.5)
        alb = t[..., 0].astype(np.float32) / 128.0
        img = np.clip(alb * light * 160.0, 0, 255).astype(np.uint8)
        im = Image.fromarray(img).resize((N // 2, N // 2))
        sheet.paste(im.convert("RGB"), ((i % 4) * (N // 2), (i // 4) * (N // 2)))
    sheet.save(os.path.join(OUT, "acabados.png"))
    print("escritos", len(NAMES), "acabados")


if __name__ == "__main__":
    main()
