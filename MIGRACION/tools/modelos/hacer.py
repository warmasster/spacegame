"""Builds the models: runs their recipes in Blender and writes `assets/models/<name>.glb`.

    blender -b -P tools/modelos/hacer.py -- [names...] [--sin-vista] [--sin-sombra] [--lista]

Without names, every recipe there is. `--sin-vista`: no picture to `out/modelos`;
`--sin-sombra`: no baked shade (quick, to try a shape); `--lista`: says what recipes there are
and which kinds of the game have none yet.

A recipe is a function of `tools/modelos/recetas/*.py` marked `@receta('<name>')`; the name is a
component kind or a plain part kind of the game (the model takes its pieces from its data), or a
thing of its own (a tool). A style (`@estilo('<name>')`: what a ship's plain shape is seen as — a
wing, a gear leg, a girder) is built as the model `estilo_<name>`, with a piece for every size
the ships use it with.
"""
import importlib
import os
import sys
import time

AQUI = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, AQUI)

import kit  # noqa: E402

for f in sorted(os.listdir(os.path.join(AQUI, 'recetas'))):
    if f.endswith('.py') and not f.startswith('_'):
        importlib.import_module('recetas.' + f[:-3])


def main():
    args = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
    nombres = [a for a in args if not a.startswith('--')]
    if '--lista' in args:
        hay = set(kit.RECETAS)
        print('recetas:', ', '.join(sorted(hay)))
        print('componentes sin receta:', ', '.join(sorted(set(kit.componentes()) - hay)))
        print('partes sin receta:', ', '.join(sorted(k for k in kit.partes() if 'parte_' + k not in hay)))
        print('estilos:', ', '.join(sorted(kit.ESTILOS)))
        for e, usos in sorted(kit.estilos_en_uso().items()):
            print(f"  {e}{'' if e in kit.ESTILOS else '  <-- no existe'}: {', '.join(sorted(usos))}")
        return
    # a style is a model too (`estilo_<style>`): one piece for every size the ships use it with
    usos = kit.estilos_en_uso()
    sin_estilo = sorted(set(usos) - set(kit.ESTILOS))
    if sin_estilo:
        raise SystemExit(f"las naves piden estilos que no hay: {', '.join(sin_estilo)}")
    estilos = {'estilo_' + e: e for e in kit.ESTILOS if e in usos}
    falta = [n for n in nombres if n not in kit.RECETAS and n not in estilos]
    if falta:
        raise SystemExit(f"no hay receta para: {', '.join(falta)}")
    t0 = time.time()
    for nombre in nombres or sorted(kit.RECETAS) + sorted(estilos):
        kit.limpiar()
        m = kit.Modelo.de(nombre)
        if nombre in estilos:
            for p in list(m.piezas.values()):
                kit.ESTILOS[estilos[nombre]](m, p)
        else:
            kit.RECETAS[nombre](m)
        m.terminar(vista='--sin-vista' not in args, sombra='--sin-sombra' not in args)
    print(f'hecho en {time.time() - t0:.0f} s')


main()
