"""Builds the astronaut (EVA suit, rigged, fingers and all) and writes `assets/models/astronauta.glb`.

    blender -b -P tools/modelos/astronauta/hacer.py -- [--salida assets/models/astronauta.glb]
            [--sin-sombra] [--detalle] [--tam-textura 2048] [--curl 30,45,30] [--vista]

Run from the MIGRACION folder (paths given are taken from where it is run; the default output does
not depend on it). `--sin-sombra`: no baked shade (quick, to try a shape or weights). `--detalle`:
bakes the fabric's folds into a normal map and puts the picture on the mission patch (the game uses
neither: the default is no textures at all). `--curl`: the rest flexion of the three joints of the
four fingers. `--vista`: pictures of the rest pose into `out/modelos/` (see `poses.py` for the rest).

What it builds, how the bones and the weights are made and how to check them: `docs/RIGGING.md`.
"""
import os
import sys
import time

AQUI = os.path.dirname(os.path.abspath(__file__))
RAIZ = os.path.normpath(os.path.join(AQUI, '..', '..', '..'))
sys.path.insert(0, AQUI)

import astronaut  # noqa: E402
import dedos  # noqa: E402
import inspeccionar  # noqa: E402
import suitkit  # noqa: E402

# The joints of the file come in this order: the 22 the game always had, as they were, then the
# fingers (left hand, right hand; thumb to little finger; from the knuckle out).
CUERPO = ['root', 'pelvis', 'spine', 'chest', 'neck', 'head',
          'clavicle.L', 'upperarm.L', 'forearm.L', 'hand.L', 'clavicle.R', 'upperarm.R', 'forearm.R', 'hand.R',
          'thigh.L', 'shin.L', 'foot.L', 'toe.L', 'thigh.R', 'shin.R', 'foot.R', 'toe.R']
ORDEN = CUERPO + [f'{d}.{k:02d}.{lado}' for lado in ('L', 'R') for d in dedos.DIGITS for k in (1, 2, 3)]


def main():
    args = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []

    def valor(clave, defecto):
        return args[args.index(clave) + 1] if clave in args else defecto

    salida = os.path.abspath(valor('--salida', os.path.join(RAIZ, 'assets', 'models', 'astronauta.glb')))
    curl = tuple(float(x) for x in valor('--curl', ','.join(str(c) for c in dedos.CURL)).split(','))
    t0 = time.time()
    rig = astronaut.build(detail='--detalle' in args, tex_size=int(valor('--tam-textura', 2048)), curl=curl)
    if '--sin-sombra' not in args:
        suitkit.bake_ao()
        astronaut.shade_collar()
    suitkit.export_glb(salida, joint_order=ORDEN)
    print(f'hecho en {time.time() - t0:.0f} s')
    inspeccionar.informe(salida, manos=True)
    if '--vista' in args:
        import poses
        poses.fotos(poses.cargar(salida), ['reposo'])


main()
