"""Looks at a model from several sides, its moving pieces at rest and at the end of their travel:

    blender -b -P tools/modelos/ver.py -- <name> [view...] [--ciclos] [--tam 1000x750]

Builds the recipe (no baked shade: quick) and writes `out/modelos/vistas/<name>/<view>.png`; then
`python tools/modelos/hoja.py --vistas <name>` joins them in one sheet. Views (all, if none is
named): izq, der, atras, lado, lado_d, arriba, abajo, frente, espalda, fin (its pieces at the end
of their travel), fin_atras, and ojo (from where the eye is when it is held: `Modelo.sosten`).
`--ciclos`: traced, with each material as the game has it (slower: for the last look).
A model's own close-ups: `Modelo.datos['vistas']` = {name: {desde, mira, dist, pose}}.
"""
import os
import sys

from mathutils import Vector

AQUI = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, AQUI)

import banco  # noqa: E402
import kit  # noqa: E402

VISTAS = {
    'izq': dict(desde=(0.62, 0.42, 0.72)),
    'der': dict(desde=(-0.7, 0.35, -0.6)),
    'atras': dict(desde=(0.45, 0.5, -0.75)),
    'lado': dict(desde=(1, 0, 0), orto=True),
    'lado_d': dict(desde=(-1, 0, 0), orto=True),
    'arriba': dict(desde=(0, 1, 0), orto=True, arriba=(0, 0, 1)),
    'abajo': dict(desde=(0.5, -0.65, 0.55)),
    'frente': dict(desde=(0, 0, 1), orto=True),
    'espalda': dict(desde=(0, 0, -1), orto=True),
    'fin': dict(desde=(0.62, 0.42, 0.72), pose='fin'),
    'fin_atras': dict(desde=(-0.55, 0.45, -0.7), pose='fin'),
}


def main():
    args = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
    ciclos = '--ciclos' in args
    tam = (1000, 750)
    if '--tam' in args:
        tam = tuple(int(x) for x in args[args.index('--tam') + 1].split('x'))
        args.remove(args[args.index('--tam') + 1])
    args = [a for a in args if not a.startswith('--')]
    banco.cargar_recetas()
    nombre, quiero = args[0], args[1:]
    b = banco.Banco(nombre)
    vistas = dict(VISTAS)
    vistas.update(b.m.datos.get('vistas', {}))
    if b.m.sosten:
        # the eye, in the model's frame, when it is held still (its turn there left out)
        ojo = -Vector(b.m.sosten['en'])
        vistas['ojo'] = dict(en=ojo, mira=ojo + Vector(b.m.sosten.get('mira', (0, -0.3, 1.0))), fov_y=72, tam=(1280, 720))
    if not b.mov:
        vistas.pop('fin', None)
        vistas.pop('fin_atras', None)
    salida = os.path.join(kit.VISTAS, 'vistas', nombre)
    for v, opciones in vistas.items():
        if quiero and v not in quiero:
            continue
        o = dict(opciones)
        pose = o.pop('pose', None)
        b.posar(b.fin() if pose == 'fin' else pose)
        o.setdefault('tam', tam)
        b.foto(os.path.join(salida, f'{v}.png'), ciclos=ciclos, **o)
        print('foto', f'out/modelos/vistas/{nombre}/{v}.png')


main()
