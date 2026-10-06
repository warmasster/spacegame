"""A contact sheet of a script's pictures, to look at many at once:

    python tools/camara/hoja.py <prefix> [name...] [--cols N] [--ancho PX]

`prefix`: the pictures are out/camara/<prefix>_<name>.png; without names, all there are (by
date). Writes out/camara/_<prefix>_hoja_<n>.png (6 to a sheet by default).
"""
import os
import sys

from PIL import Image, ImageDraw

RAIZ = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
D = os.path.join(RAIZ, 'out', 'camara')


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    cols = int(sys.argv[sys.argv.index('--cols') + 1]) if '--cols' in sys.argv else 3
    w = int(sys.argv[sys.argv.index('--ancho') + 1]) if '--ancho' in sys.argv else 640
    args = [a for a in args if not a.isdigit()]
    pre = args[0]
    nombres = args[1:]
    if not nombres:
        fs = [f for f in os.listdir(D) if f.startswith(pre + '_') and f.endswith('.png')]
        fs.sort(key=lambda f: os.path.getmtime(os.path.join(D, f)))
        nombres = [f[len(pre) + 1:-4] for f in fs]
    por = cols * 2
    for h in range(0, len(nombres), por):
        grupo = nombres[h:h + por]
        ims = [Image.open(os.path.join(D, f'{pre}_{n}.png')).convert('RGB') for n in grupo]
        a = int(w * ims[0].height / ims[0].width)
        filas = (len(grupo) + cols - 1) // cols
        hoja = Image.new('RGB', (cols * w, filas * a), (10, 12, 16))
        for k, (n, im) in enumerate(zip(grupo, ims)):
            im = im.resize((w, a), Image.LANCZOS)
            ImageDraw.Draw(im).text((8, 6), n, fill=(255, 255, 0))
            hoja.paste(im, ((k % cols) * w, (k // cols) * a))
        ruta = os.path.join(D, f'_{pre}_hoja_{h // por + 1}.png')
        hoja.save(ruta)
        print(ruta)


main()
