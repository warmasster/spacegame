"""A contact sheet of the models' pictures (`out/modelos/*.png`), to look at many at once:

    python tools/modelos/hoja.py [name...]         ->  out/modelos/_hoja_<n>.png (12 to a sheet)
    python tools/modelos/hoja.py --vistas <name>   ->  out/modelos/vistas/_<name>.png: every view
                                                       `ver.py` took of that model, in one sheet
    options: --cols N (4), --por-hoja N (12), --celda WxH (400x300)
"""
import os
import sys

from PIL import Image, ImageDraw

RAIZ = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
D = os.path.join(RAIZ, 'out', 'modelos')


def opcion(args, nombre, defecto):
    if nombre in args:
        k = args.index(nombre)
        valor = args[k + 1]
        del args[k:k + 2]
        return valor
    return defecto


def hoja(rutas, salida, cols, celda):
    w, a = celda
    filas = (len(rutas) + cols - 1) // cols
    im = Image.new('RGB', (cols * w, filas * a), (10, 12, 16))
    for k, (rotulo, ruta) in enumerate(rutas):
        foto = Image.open(ruta).convert('RGB')
        foto.thumbnail((w, a))
        tesela = Image.new('RGB', (w, a), (30, 33, 40))
        tesela.paste(foto, ((w - foto.width) // 2, (a - foto.height) // 2))
        ImageDraw.Draw(tesela).text((8, 6), rotulo, fill=(255, 255, 255))
        im.paste(tesela, ((k % cols) * w, (k // cols) * a))
    im.save(salida)
    print(salida)


def main():
    args = sys.argv[1:]
    cols = int(opcion(args, '--cols', 4))
    por_hoja = int(opcion(args, '--por-hoja', 12))
    celda = tuple(int(x) for x in opcion(args, '--celda', '400x300').split('x'))
    if '--vistas' in args:
        args.remove('--vistas')
        v = os.path.join(D, 'vistas')
        for nombre in args:
            carpeta = os.path.join(v, nombre)
            orden = ['izq', 'der', 'atras', 'lado', 'lado_d', 'arriba', 'abajo', 'frente', 'espalda', 'fin', 'fin_atras', 'ojo']
            fotos = sorted((f[:-4] for f in os.listdir(carpeta) if f.endswith('.png')), key=lambda f: (orden.index(f) if f in orden else 99, f))
            hoja([(f'{nombre} {f}', os.path.join(carpeta, f + '.png')) for f in fotos], os.path.join(v, f'_{nombre}.png'), cols, celda if '--celda' in sys.argv else (600, 450))
        return
    nombres = args or sorted(f[:-4] for f in os.listdir(D) if f.endswith('.png') and not f.startswith('_') and not f.startswith('astronauta'))
    for h in range(0, len(nombres), por_hoja):
        grupo = nombres[h:h + por_hoja]
        hoja([(n, os.path.join(D, n + '.png')) for n in grupo], os.path.join(D, f'_hoja_{h // por_hoja + 1}.png'), cols, celda)


main()
