"""Calcas: decals for ships and structures (posters in pixel art, makers' logos, safety signs,
plates, hazard stripes), packed into one RGBA atlas.

Writes assets/textures/calcas.bin (raw RGBA8), calcas.json (atlas size; each decal's rectangle
in the atlas as u0, v0, u1, v1 and its aspect) and calcas.png (to look at).

    python tools/texturas/calcas.py

Posters are drawn small and blown up without smoothing (pixel art); signs and plates are drawn
at full size with smooth edges. Fonts come from the system (Windows) when there; the atlas is
an asset, so the game never needs them.
"""
import json
import math
import os

import numpy as np
from PIL import Image, ImageDraw, ImageFont

import pixfont as pf

ROOT = os.path.join(os.path.dirname(__file__), "..", "..")
OUT = os.path.join(ROOT, "assets", "textures")
FONTS = "C:/Windows/Fonts"
ATLAS = 2048


def font(name, size):
    for f in (name, "arialbd.ttf", "arial.ttf"):
        try:
            return ImageFont.truetype(os.path.join(FONTS, f), size)
        except OSError:
            continue
    return ImageFont.load_default()


def text_center(d, xy, s, f, fill, anchor="mm"):
    d.text(xy, s, font=f, fill=fill, anchor=anchor)


# ---------------------------------------------------------------- pixel art

class Pix:
    """A small canvas drawn pixel by pixel, blown up at the end."""

    def __init__(self, w, h, bg):
        self.im = Image.new("RGBA", (w, h), bg)
        self.d = ImageDraw.Draw(self.im)
        self.d.fontmode = "1"

    def rect(self, x0, y0, x1, y1, c):
        self.d.rectangle([x0, y0, x1, y1], fill=c)

    def px(self, x, y, c):
        if 0 <= x < self.im.width and 0 <= y < self.im.height:
            self.im.putpixel((int(x), int(y)), c)

    def disc(self, cx, cy, r, c):
        self.d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=c)

    def text(self, x, y, s, size, c, anchor="mm", face="consolab.ttf"):
        self.d.text((x, y), s, font=font(face, size), fill=c, anchor=anchor)

    def stars(self, seed, n, y1, colors):
        r = np.random.default_rng(seed)
        for _ in range(n):
            self.px(r.integers(0, self.im.width), r.integers(0, y1), colors[r.integers(0, len(colors))])

    def big(self, k=6):
        return self.im.resize((self.im.width * k, self.im.height * k), Image.NEAREST)


def frame(p, c):
    w, h = p.im.size
    p.d.rectangle([0, 0, w - 1, h - 1], outline=c)
    p.d.rectangle([1, 1, w - 2, h - 2], outline=(0, 0, 0, 255))


W, H = 64, 88  # poster canvas (px), blown up x5


def title(p, y, s, c, scale=1):
    pf.centered(p, W // 2, y, s, c, scale)


def poster_flota():
    p = Pix(W, H, (14, 18, 46, 255))
    p.stars(1, 90, 66, [(255, 255, 255, 255), (170, 190, 255, 255), (255, 220, 160, 255)])
    p.disc(32, 104, 42, (150, 150, 158, 255))
    p.disc(18, 70, 4, (120, 120, 128, 255))
    p.disc(45, 76, 5, (124, 124, 132, 255))
    for y in range(26, 50):
        w = max(1, (y - 26) // 4)
        p.rect(32 - w, y, 32 + w, y, (236, 236, 230, 255))
    p.rect(27, 42, 28, 51, (230, 110, 30, 255))
    p.rect(36, 42, 37, 51, (230, 110, 30, 255))
    p.rect(31, 32, 33, 34, (60, 120, 200, 255))
    for k, c in enumerate([(255, 240, 160, 255), (255, 170, 40, 255), (230, 80, 20, 255)]):
        p.rect(30 + k // 2, 50 + k * 3, 34 - k // 2, 52 + k * 3, c)
    title(p, 4, "UNETE A", (255, 210, 70, 255))
    title(p, 13, "LA FLOTA", (255, 255, 255, 255), 1)
    p.rect(3, 74, 60, 84, (200, 40, 30, 255))
    title(p, 76, "ORELL", (255, 255, 255, 255))
    frame(p, (255, 210, 70, 255))
    return p.big(5)


def poster_traje():
    p = Pix(W, H, (232, 120, 30, 255))
    for y in range(H):
        if y % 6 < 3:
            p.rect(0, y, 2, y, (40, 30, 20, 255))
            p.rect(W - 3, y, W - 1, y, (40, 30, 20, 255))
    p.disc(32, 42, 18, (236, 236, 232, 255))
    p.disc(32, 42, 16, (220, 220, 214, 255))
    p.rect(19, 34, 45, 47, (30, 60, 110, 255))
    p.rect(22, 36, 31, 38, (120, 170, 230, 255))
    p.rect(16, 58, 48, 62, (200, 200, 196, 255))
    title(p, 5, "REVISA", (30, 20, 10, 255), 1)
    title(p, 14, "TU TRAJE", (30, 20, 10, 255), 1)
    p.rect(5, 68, 58, 84, (30, 20, 10, 255))
    title(p, 69, "SEGURIDAD", (255, 210, 120, 255))
    title(p, 77, "DE TODOS", (255, 210, 120, 255))
    frame(p, (30, 20, 10, 255))
    return p.big(5)


def poster_luna():
    p = Pix(W, H, (6, 6, 12, 255))
    p.stars(3, 90, 56, [(255, 255, 255, 255), (200, 200, 220, 255)])
    p.disc(46, 22, 9, (40, 90, 200, 255))
    p.disc(43, 19, 4, (60, 150, 70, 255))
    p.disc(49, 25, 3, (60, 150, 70, 255))
    p.rect(40, 15, 45, 15, (240, 240, 250, 255))
    for x in range(W):
        h = 62 + int(3 * math.sin(x * 0.25)) + (2 if 24 < x < 40 else 0)
        p.rect(x, h, x, H - 1, (140, 140, 146, 255))
        p.rect(x, h, x, h, (190, 190, 196, 255))
    p.disc(16, 62, 7, (230, 230, 224, 255))
    p.rect(9, 62, 23, 65, (140, 140, 146, 255))
    p.px(16, 55, (255, 60, 40, 255))
    title(p, 4, "VISITA", (255, 255, 255, 255))
    title(p, 36, "TYCHO", (255, 220, 120, 255), 2)
    p.rect(3, 74, 60, 85, (20, 20, 26, 255))
    title(p, 76, "BASE LUNAR", (200, 200, 210, 255))
    frame(p, (200, 200, 210, 255))
    return p.big(5)


def poster_oxigeno():
    p = Pix(W, H, (30, 120, 70, 255))
    for y in range(0, H, 4):
        p.rect(0, y, W - 1, y, (36, 132, 78, 255))
    p.rect(25, 24, 39, 64, (230, 230, 226, 255))
    p.rect(26, 25, 27, 63, (255, 255, 255, 255))
    p.rect(29, 18, 35, 23, (60, 62, 66, 255))
    p.rect(28, 15, 36, 17, (200, 170, 60, 255))
    title(p, 40, "O2", (30, 120, 70, 255))
    title(p, 5, "CADA", (255, 255, 255, 255))
    title(p, 70, "RESPIRO", (255, 255, 255, 255))
    title(p, 79, "CUENTA", (220, 255, 220, 255))
    frame(p, (255, 255, 255, 255))
    return p.big(5)


def poster_orell():
    p = Pix(W, H, (226, 224, 214, 255))
    c = (40, 44, 52, 255)
    for k in range(18):
        p.rect(32 - k - 3, 30 + k // 2, 32 - k, 30 + k // 2, c)
        p.rect(32 + k, 30 + k // 2, 32 + k + 3, 30 + k // 2, c)
    p.rect(29, 24, 35, 46, c)
    p.rect(30, 21, 34, 23, c)
    p.rect(26, 47, 38, 51, c)
    p.rect(29, 27, 35, 30, (230, 110, 30, 255))
    title(p, 58, "ORELL", c, 2)
    title(p, 76, "ASTILLEROS", (90, 92, 100, 255))
    p.rect(8, 8, 55, 11, (230, 110, 30, 255))
    frame(p, c)
    return p.big(5)


def poster_carga():
    p = Pix(W, H, (250, 200, 40, 255))
    for x in range(-88, W, 10):
        for k in range(5):
            p.d.line([(x + k, H - 1), (x + 20 + k, H - 21)], fill=(30, 30, 30, 255))
    p.rect(12, 30, 28, 46, (150, 100, 50, 255))
    p.rect(13, 31, 27, 45, (180, 130, 70, 255))
    p.rect(33, 36, 49, 52, (150, 100, 50, 255))
    p.rect(34, 37, 48, 51, (180, 130, 70, 255))
    title(p, 5, "ZONA DE", (30, 30, 30, 255))
    title(p, 14, "CARGA", (30, 30, 30, 255), 1)
    title(p, 57, "MIRA ANTES", (30, 30, 30, 255))
    frame(p, (30, 30, 30, 255))
    return p.big(5)


# ---------------------------------------------------------------- signs and plates

def sign_triangle(symbol, label, w=512, h=600):
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    tri = [(w / 2, 24), (w - 24, w - 60), (24, w - 60)]
    d.polygon(tri, fill=(20, 20, 20, 255))
    inner = [(w / 2, 70), (w - 66, w - 86), (66, w - 86)]
    d.polygon(inner, fill=(250, 200, 30, 255))
    symbol(d, w / 2, w * 0.58)
    d.rectangle([10, w - 30, w - 10, h - 10], fill=(250, 200, 30, 255), outline=(20, 20, 20, 255), width=8)
    text_center(d, (w / 2, (w - 30 + h - 10) / 2), label, font("bahnschrift.ttf", 54), (20, 20, 20, 255))
    return im


def bolt(d, cx, cy):
    pts = [(cx + 10, cy - 120), (cx - 60, cy + 10), (cx - 5, cy + 10), (cx - 25, cy + 120), (cx + 60, cy - 20), (cx + 5, cy - 20)]
    d.polygon(pts, fill=(20, 20, 20, 255))


def trefoil(d, cx, cy):
    r0, r1 = 26, 110
    for k in range(3):
        a0 = math.radians(-90 + k * 120 - 30)
        a1 = math.radians(-90 + k * 120 + 30)
        pts = [(cx + r0 * math.cos(a0), cy + r0 * math.sin(a0))]
        for t in np.linspace(a0, a1, 12):
            pts.append((cx + r1 * math.cos(t), cy + r1 * math.sin(t)))
        pts.append((cx + r0 * math.cos(a1), cy + r0 * math.sin(a1)))
        d.polygon(pts, fill=(20, 20, 20, 255))
    d.ellipse([cx - 18, cy - 18, cx + 18, cy + 18], fill=(20, 20, 20, 255))


def sign_square(bg, fg, draw, label, w=512, h=560):
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([8, 8, w - 8, h - 8], radius=28, fill=bg, outline=(255, 255, 255, 255), width=10)
    draw(d, w / 2, w * 0.45, fg)
    text_center(d, (w / 2, h - 70), label, font("bahnschrift.ttf", 56), fg)
    return im


def extinguisher(d, cx, cy, c):
    d.rounded_rectangle([cx - 50, cy - 70, cx + 50, cy + 130], radius=30, fill=c)
    d.rectangle([cx - 20, cy - 110, cx + 20, cy - 70], fill=c)
    d.line([(cx + 20, cy - 100), (cx + 110, cy - 60), (cx + 120, cy + 20)], fill=c, width=16)
    d.rectangle([cx - 70, cy - 120, cx + 30, cy - 104], fill=c)


def runner(d, cx, cy, c):
    d.ellipse([cx - 30, cy - 150, cx + 10, cy - 110], fill=c)
    d.line([(cx - 10, cy - 100), (cx - 30, cy + 10)], fill=c, width=26)
    d.line([(cx - 30, cy + 10), (cx + 30, cy + 70), (cx + 20, cy + 130)], fill=c, width=24)
    d.line([(cx - 30, cy + 10), (cx - 80, cy + 60), (cx - 120, cy + 50)], fill=c, width=24)
    d.line([(cx - 20, cy - 80), (cx + 50, cy - 40), (cx + 90, cy - 70)], fill=c, width=20)
    d.line([(cx - 20, cy - 80), (cx - 80, cy - 40)], fill=c, width=20)


def exit_sign():
    w, h = 768, 320
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([6, 6, w - 6, h - 6], radius=24, fill=(20, 150, 70, 255), outline=(240, 255, 240, 255), width=8)
    runner(d, 150, 180, (255, 255, 255, 255))
    text_center(d, (470, 130), "SALIDA", font("bahnschrift.ttf", 120), (255, 255, 255, 255))
    d.polygon([(380, 230), (620, 230), (620, 205), (680, 250), (620, 295), (620, 270), (380, 270)], fill=(255, 255, 255, 255))
    return im


def label(text, bg, fg, w=768, h=200, size=110, face="bahnschrift.ttf", border=None, sub=None):
    """A label plate; the text shrinks to fit; `sub`: a small lowered suffix of the first word (O2)."""
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([4, 4, w - 4, h - 4], radius=16, fill=bg, outline=border or bg, width=6)
    while size > 20:
        f = font(face, size)
        sw = font(face, int(size * 0.6)).getlength(sub) if sub else 0
        if f.getlength(text) + sw + 60 <= w:
            break
        size -= 4
    f = font(face, size)
    if sub:
        head, rest = text.split(" ", 1)
        fs = font(face, int(size * 0.6))
        total = f.getlength(head) + fs.getlength(sub) + f.getlength(" " + rest)
        x = (w - total) / 2
        d.text((x, h / 2), head, font=f, fill=fg, anchor="lm")
        x += f.getlength(head)
        d.text((x, h / 2 + size * 0.3), sub, font=fs, fill=fg, anchor="lm")
        x += fs.getlength(sub)
        d.text((x, h / 2), " " + rest, font=f, fill=fg, anchor="lm")
    else:
        text_center(d, (w / 2, h / 2), text, f, fg)
    return im


def stencil(text, color, w=1024, h=220, size=170):
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    text_center(d, (w / 2, h / 2), text, font("impact.ttf", size), color)
    # stencil bridges: thin gaps across the letters
    arr = np.array(im)
    for x in range(0, w, 64):
        arr[:, x:x + 5, 3] = (arr[:, x:x + 5, 3] * 0.0).astype(np.uint8)
    return Image.fromarray(arr)


def stripes(w=1024, h=128):
    im = Image.new("RGBA", (w, h), (250, 200, 30, 255))
    d = ImageDraw.Draw(im)
    for x in range(-h, w + h, 96):
        d.polygon([(x, h), (x + 48, h), (x + 48 + h, 0), (x + h, 0)], fill=(24, 24, 24, 255))
    return im


def plate(lines, w=768, h=320):
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([4, 4, w - 4, h - 4], radius=18, fill=(176, 178, 182, 255), outline=(90, 92, 98, 255), width=6)
    for (x, y) in [(28, 28), (w - 28, 28), (28, h - 28), (w - 28, h - 28)]:
        d.ellipse([x - 10, y - 10, x + 10, y + 10], fill=(120, 122, 128, 255), outline=(70, 72, 78, 255), width=3)
    y = 70
    for text, size in lines:
        text_center(d, (w / 2, y), text, font("bahnschrift.ttf", size), (34, 36, 42, 255))
        y += size + 22
    return im


def logo_orell(color=(40, 44, 52, 255), w=1180, h=420):
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    cx, cy = 210, 200
    wing = [(cx, cy - 20), (cx - 180, cy - 120), (cx - 150, cy - 40), (cx - 40, cy + 30)]
    d.polygon(wing, fill=color)
    d.polygon([(2 * cx - x, y) for x, y in wing], fill=color)
    d.polygon([(cx - 30, cy - 70), (cx + 30, cy - 70), (cx + 20, cy + 110), (cx, cy + 150), (cx - 20, cy + 110)], fill=color)
    d.polygon([(cx - 22, cy - 70), (cx + 22, cy - 70), (cx, cy - 110)], fill=(230, 110, 30, 255))
    d.text((430, 140), "ORELL", font=font("bahnschrift.ttf", 190), fill=color, anchor="lm")
    d.text((436, 290), "ASTILLEROS ORBITALES", font=font("bahnschrift.ttf", 54), fill=color, anchor="lm")
    return im


def logo_word(word, sub, color, accent, w=1100, h=300):
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rectangle([20, 60, 120, 240], fill=accent)
    d.polygon([(40, 80), (100, 150), (40, 220)], fill=color)
    d.text((160, 130), word, font=font("bahnschrift.ttf", 160), fill=color, anchor="lm")
    d.text((166, 245), sub, font=font("bahnschrift.ttf", 44), fill=color, anchor="lm")
    return im


def arrow(w=512, h=256):
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.polygon([(20, 90), (320, 90), (320, 30), (490, 128), (320, 226), (320, 166), (20, 166)], fill=(240, 240, 236, 255))
    return im


def ruler(w=1024, h=128, paint=(236, 236, 230, 255)):
    """A metre of a rule painted on a deck: a line along it, a tick every 10 cm, a longer one at
    the half and the longest at each end (laid end to end, the end ticks meet)."""
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rectangle([0, 0, w, 9], fill=paint)
    for k in range(11):
        x = round(k * (w - 1) / 10)
        tall, wide = (h, 9) if k in (0, 10) else (int(h * 0.72), 7) if k == 5 else (int(h * 0.42), 5)
        d.rectangle([x - wide // 2, 0, x + wide // 2, tall], fill=paint)
    return im


def line(w=1024, h=16, paint=(236, 236, 230, 255)):
    """A stretch of a painted line (tinted where it is laid)."""
    return Image.new("RGBA", (w, h), paint)


def anchor_point():
    w = 512
    im = Image.new("RGBA", (w, w), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.ellipse([10, 10, w - 10, w - 10], fill=(250, 200, 30, 255), outline=(24, 24, 24, 255), width=14)
    d.ellipse([w / 2 - 80, w / 2 - 120, w / 2 + 80, w / 2 + 40], outline=(24, 24, 24, 255), width=26)
    text_center(d, (w / 2, w / 2 + 120), "ANCLAJE", font("bahnschrift.ttf", 64), (24, 24, 24, 255))
    return im


def decals():
    blue = (30, 80, 170, 255)
    return {
        "poster_flota": poster_flota(),
        "poster_traje": poster_traje(),
        "poster_luna": poster_luna(),
        "poster_oxigeno": poster_oxigeno(),
        "poster_orell": poster_orell(),
        "poster_carga": poster_carga(),
        "senal_tension": sign_triangle(bolt, "ALTA TENSIÓN"),
        "senal_radiacion": sign_triangle(trefoil, "RADIACIÓN"),
        "senal_extintor": sign_square((200, 30, 30, 255), (255, 255, 255, 255), extinguisher, "EXTINTOR"),
        "senal_salida": exit_sign(),
        "senal_oxigeno": label("O OXÍGENO", (30, 140, 70, 255), (255, 255, 255, 255), w=900, sub="2"),
        "senal_nitrogeno": label("N NITRÓGENO", (40, 40, 44, 255), (255, 255, 255, 255), w=900, sub="2"),
        "senal_presion": label("COMPARTIMENTO PRESURIZADO", blue, (255, 255, 255, 255), w=1024, size=70),
        "senal_no_pisar": stencil("NO PISAR", (200, 30, 30, 255)),
        "senal_carga": label("CARGA MÁX. 2 000 kg", (250, 200, 30, 255), (24, 24, 24, 255), w=1024, size=84, border=(24, 24, 24, 255)),
        "senal_anclaje": anchor_point(),
        "franjas": stripes(),
        "regla": ruler(),
        "linea": line(),
        "flecha": arrow(),
        "logo_orell": logo_orell(),
        "logo_orell_blanco": logo_orell((240, 240, 236, 255)),
        "logo_kestrel": logo_word("KESTREL", "CABRESTANTES Y GRÚAS", (36, 38, 44, 255), (200, 40, 30, 255)),
        "logo_hokuto": logo_word("HOKUTO", "SOPORTE VITAL", (36, 38, 44, 255), (40, 140, 160, 255)),
        "placa_nave": plate([("ALCOTÁN  ALC-07", 64), ("ORELL AL-7 · Nº 07-2231", 44), ("PESO EN VACÍO 29 500 kg", 40)]),
        "rotulo_bodega": stencil("BODEGA", (236, 236, 230, 255)),
        "rotulo_cabina": stencil("CABINA", (236, 236, 230, 255)),
        "rotulo_puente": stencil("PUENTE", (236, 236, 230, 255)),
        "matricula": stencil("ALC-07", (30, 32, 38, 255), w=1024, h=240, size=200),
    }


def pack(items):
    """Shelf packing, tallest first. Returns the atlas and each item's rectangle (px). Signs and
    plates are drawn big and go in at 55 %; posters keep their pixels."""
    items = {n: (im if n.startswith("poster") else im.resize((int(im.width * 0.55), int(im.height * 0.55)), Image.LANCZOS)) for n, im in items.items()}
    atlas = Image.new("RGBA", (ATLAS, ATLAS), (0, 0, 0, 0))
    order = sorted(items.items(), key=lambda kv: -kv[1].height)
    x = y = shelf = 0
    rects = {}
    pad = 8
    for name, im in order:
        if im.width > ATLAS - 2 * pad:
            im = im.resize((ATLAS - 2 * pad, int(im.height * (ATLAS - 2 * pad) / im.width)))
        if x + im.width + pad > ATLAS:
            x = 0
            y += shelf + pad
            shelf = 0
        if y + im.height > ATLAS:
            raise SystemExit(f"el atlas de calcas no tiene sitio para {name}")
        atlas.paste(im, (x, y))
        rects[name] = (x, y, x + im.width, y + im.height)
        x += im.width + pad
        shelf = max(shelf, im.height)
    return atlas, rects


def main():
    os.makedirs(OUT, exist_ok=True)
    items = decals()
    atlas, rects = pack(items)
    data = np.array(atlas, dtype=np.uint8)
    data.tofile(os.path.join(OUT, "calcas.bin"))
    atlas.save(os.path.join(OUT, "calcas.png"))
    meta = {
        "size": ATLAS,
        "calcas": {n: {"uv": [r[0] / ATLAS, r[1] / ATLAS, r[2] / ATLAS, r[3] / ATLAS], "aspecto": (r[2] - r[0]) / (r[3] - r[1])} for n, r in sorted(rects.items())},
    }
    with open(os.path.join(OUT, "calcas.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=1)
    print(len(rects), "calcas")


if __name__ == "__main__":
    main()
