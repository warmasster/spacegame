"""Signed-distance-field font atlas for panel silkscreen, screens and displays.

    python tools/fonts/atlas.py [--font PATH] [--out assets/fonts/serigrafia]

Writes <out>.bin (raw R8, width x height), <out>.json (metrics) and <out>.png (to look at).
Glyphs are rendered large, their distance field computed (inside positive) and stored in cells.
The default font is DejaVu Sans Condensed Bold (free licence: the atlas may ship with the game).
"""
import argparse
import json
import os

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy.ndimage import distance_transform_edt

CHARS = (
    "".join(chr(c) for c in range(32, 127))
    + "ÁÉÍÓÚÜÑáéíóúüñ°±·×µΩ²³¿¡ºª€→←↑↓▲▼●○■□"
)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--font", default=r"C:\Windows\Fonts\DejaVuSansCondensed-Bold.ttf")
    ap.add_argument("--out", default="assets/fonts/serigrafia")
    ap.add_argument("--em", type=int, default=56, help="font size in pixels (rendering)")
    ap.add_argument("--spread", type=int, default=7, help="distance range (pixels)")
    ap.add_argument("--width", type=int, default=1024)
    args = ap.parse_args()

    font = ImageFont.truetype(args.font, args.em)
    ascent, descent = font.getmetrics()
    pad = args.spread + 2
    glyphs = {}
    cells = []
    for ch in CHARS:
        try:
            box = font.getbbox(ch)
        except Exception:
            continue
        adv = font.getlength(ch)
        x0, y0, x1, y1 = box
        w, h = max(x1 - x0, 1), max(y1 - y0, 1)
        img = Image.new("L", (w + 2 * pad, h + 2 * pad), 0)
        d = ImageDraw.Draw(img)
        d.text((pad - x0, pad - y0), ch, fill=255, font=font)
        a = np.asarray(img, dtype=np.float32) / 255.0
        inside = a > 0.5
        if ch.strip() == "":
            sdf = np.zeros_like(a)
        else:
            d_out = distance_transform_edt(~inside)
            d_in = distance_transform_edt(inside)
            sdf = (d_in - d_out) / args.spread
        val = np.clip(0.5 + 0.5 * sdf, 0.0, 1.0)
        cells.append((ch, (val * 255).astype(np.uint8), x0, y0, adv))

    # shelf packing
    W = args.width
    x = y = row = 0
    placed = []
    for ch, cell, x0, y0, adv in cells:
        h, w = cell.shape
        if x + w > W:
            x = 0
            y += row + 1
            row = 0
        placed.append((ch, cell, x, y, x0, y0, adv))
        x += w + 1
        row = max(row, h)
    H = 1
    while H < y + row + 1:
        H *= 2
    atlas = np.zeros((H, W), dtype=np.uint8)
    for ch, cell, px, py, x0, y0, adv in placed:
        h, w = cell.shape
        atlas[py:py + h, px:px + w] = cell
        # metrics in em units: atlas rect, quad offset from the pen (x right, y up from the
        # baseline) and size, advance
        em = float(args.em)
        glyphs[ch] = [
            px / W, py / H, (px + w) / W, (py + h) / H,
            (x0 - pad) / em, (ascent - (y0 - pad) - h) / em, w / em, h / em,
            adv / em,
        ]
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    atlas.tofile(args.out + ".bin")
    Image.fromarray(atlas).save(args.out + ".png")
    meta = {
        "width": W,
        "height": H,
        "em": args.em,
        "spread": args.spread / args.em,
        "ascent": ascent / args.em,
        "descent": descent / args.em,
        "glyphs": glyphs,
    }
    with open(args.out + ".json", "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False)
    print(f"{len(glyphs)} glyphs, atlas {W}x{H}")


if __name__ == "__main__":
    main()
