"""Tests the models: what is in `assets/models` against what the game takes and what each recipe
says of its model.

    blender -b -P tools/modelos/comprobar.py -- [name...] [--antes <folder>]

Without names, every recipe. For each model it says `bien` or what is wrong, and ends with a
count; the process ends with 1 if anything failed (so it can gate a build).

Of the file, as the game reads it (`core/structure/models.rs`):
  - it is there, it parses, no node carries a transform, all triangles, a shade for every corner;
  - its pieces are the recipe's, no more and no less;
  - its normals are unit and its triangles face the way their corners' normals say (a face
    turned inside out is not seen);
  - every `#finish` is one the game has (`core/src/mesh.rs`, `FINISHES`);
  - it is the recipe as it is now (as many triangles, the same box): not a stale file;
  - a kind of the game keeps to its shape (the rule of `crates/ship/tests/modelos.rs`);
  - it is within its triangle budget (`Modelo.presupuesto`).
Of what the recipe says (`Modelo.mueve`, `punto`, `pantalla`, `luz`):
  - a piece that moves cuts nothing through its whole travel (not what it rides on, not the
    body, not another piece that moves);
  - a screen is flat glass where it is said to be, as big as said, looking the way said, and
    nothing stands in front of it;
  - a grip is as thick as a glove takes (`mango`), and long enough for the four fingers;
  - a lamp's lens is where it is said, of the material said.
`--antes <folder>`: the models as they were (their `.glb`): every piece that was there still is,
by the same name, as big and in the same place (to 2 cm, or what the recipe allows it).
"""
import json
import math
import os
import re
import sys

import numpy as np
from mathutils import Vector

AQUI = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, AQUI)

import banco  # noqa: E402
import kit  # noqa: E402

TOL_ANTES = 0.02


def acabados():
    try:
        texto = open(os.path.join(kit.RAIZ, 'crates', 'core', 'src', 'mesh.rs'), encoding='utf-8').read()
        lista = re.search(r'FINISHES: \[&str; \d+\] = \[(.*?)\];', texto, re.S).group(1)
        return set(re.findall(r'"(\w+)"', lista))
    except (OSError, AttributeError):
        return None


def formas_del_juego():
    """{model name: {piece name: half box}} for every kind and part of the game."""
    out = {}

    def medio(f):
        k = f.get('kind')
        if k in ('box', 'wedge'):
            return [x / 2 for x in f['size']]
        if k == 'cylinder':
            t = max(1.0, f.get('taper', 1.0))
            return [f['radius'] * t, f['height'] / 2, f['radius'] * t]
        pts = f.get('points', [(0, 0, 0)])
        return [max(abs(p[i]) for p in pts) for i in range(3)]

    for id, k in kit.componentes().items():
        out[id] = {(p.get('id') or 'cuerpo'): medio(p['forma']) for p in k['piezas'] if 'modelo' not in p}
    for id, k in kit.partes().items():
        out['parte_' + id] = {'cuerpo': medio(k['shape'])}
    return out


def caras_de_frente(p):
    """The share of a piece's triangles that face the way the normals of their corners say."""
    pos, nrm, idx = p['pos'], p['nrm'], p['idx'].reshape(-1, 3)
    a, b, c = pos[idx[:, 0]], pos[idx[:, 1]], pos[idx[:, 2]]
    cara = np.cross(b - a, c - a)
    area = np.linalg.norm(cara, axis=1)
    vale = area > 1e-12
    medio = nrm[idx[:, 0]] + nrm[idx[:, 1]] + nrm[idx[:, 2]]
    de_frente = (np.einsum('ij,ij->i', cara, medio) > 0) & vale
    return float(de_frente.sum()) / max(int(vale.sum()), 1), int((~vale).sum())


def comprobar(nombre, formas, finos, antes):
    fallos, notas = [], []
    ruta = os.path.join(kit.SALIDA, nombre + '.glb')
    if not os.path.exists(ruta):
        return [f'no hay {os.path.relpath(ruta, kit.RAIZ)}'], notas
    try:
        piezas, materiales = banco.leer_glb(ruta)
    except Exception as e:  # noqa: BLE001
        return [f'no se lee: {e}'], notas
    b = banco.Banco(nombre)
    m = b.m
    # ---- the file
    esperadas = {m.piezas[id].nombre for id in b.ob}
    if set(piezas) != esperadas:
        fallos.append(f'piezas del fichero {sorted(piezas)} != las de la receta {sorted(esperadas)}')
    total = 0
    for id, p in piezas.items():
        tris = len(p['idx']) // 3
        total += tris
        if not (len(p['pos']) == len(p['nrm']) == len(p['col'])) or len(p['idx']) % 3 or (len(p['idx']) and p['idx'].max() >= len(p['pos'])):
            fallos.append(f"'{id}': mallas rotas")
            continue
        largo = np.linalg.norm(p['nrm'], axis=1)
        if np.abs(largo - 1.0).max() > 0.02:
            fallos.append(f"'{id}': normales que no son unitarias ({largo.min():.3f}..{largo.max():.3f})")
        frente, nulos = caras_de_frente(p)
        if frente < 0.97:
            fallos.append(f"'{id}': solo el {frente * 100:.1f} % de sus caras mira hacia fuera")
        if nulos > tris * 0.02:
            fallos.append(f"'{id}': {nulos} triángulos sin área")
        if p['col'][:, :3].max() < 0.5:
            fallos.append(f"'{id}': toda en sombra")
    if finos is not None:
        for mat in materiales:
            if '#' in mat and mat.split('#')[1] not in finos:
                fallos.append(f"acabado desconocido en '{mat}'")
    # ---- it is the recipe as it is now
    ids = {m.piezas[id].nombre: id for id in b.ob}
    for nom, p in piezas.items():
        if nom not in ids:
            continue
        hechos = b.tris(ids[nom])
        if hechos != len(p['idx']) // 3:
            fallos.append(f"'{nom}': el fichero tiene {len(p['idx']) // 3} triángulos y la receta hace {hechos}: hay que reconstruirlo")
    if m.presupuesto and total > m.presupuesto:
        fallos.append(f'{total} triángulos: pasa de su presupuesto ({m.presupuesto})')
    notas.append(f'{total} tri' + (f' de {m.presupuesto}' if m.presupuesto else ''))
    # ---- a kind of the game keeps to its shape
    for nom, medio in formas.get(nombre, {}).items():
        if nom not in piezas:
            fallos.append(f"falta la pieza '{nom}' que el juego pide")
            continue
        h = np.array(medio)
        lo, hi = piezas[nom]['pos'].min(0), piezas[nom]['pos'].max(0)
        holgura = max(0.32 * h.max(), 0.06) + 0.15
        sale = max((-h - lo).max(), (hi - h).max())
        llena = ((hi - lo) / np.maximum(h * 2, 1e-6)).max()
        if sale > holgura or llena < 0.5:
            fallos.append(f"'{nom}': fuera de su forma (sale {sale * 100:.0f} cm de {holgura * 100:.0f}, llena {llena * 100:.0f} %)")
    # ---- as it was
    if antes:
        vieja = os.path.join(antes, nombre + '.glb')
        if os.path.exists(vieja):
            eran, _ = banco.leer_glb(vieja)
            permitido = m.datos.get('cambia', {})
            for nom, p in eran.items():
                if nom not in piezas:
                    fallos.append(f"la pieza '{nom}' que había ya no está")
                    continue
                lo0, hi0 = p['pos'].min(0), p['pos'].max(0)
                lo1, hi1 = piezas[nom]['pos'].min(0), piezas[nom]['pos'].max(0)
                d = max(np.abs(lo1 - lo0).max(), np.abs(hi1 - hi0).max())
                tol = permitido.get(nom, TOL_ANTES)
                if d > tol:
                    fallos.append(f"'{nom}': su caja cambió {d * 100:.1f} cm (antes {np.round(lo0, 3)}..{np.round(hi0, 3)}, ahora {np.round(lo1, 3)}..{np.round(hi1, 3)})")
            nuevas = sorted(set(piezas) - set(eran))
            if nuevas:
                fallos.append(f'piezas que antes no había: {nuevas}')
            notas.append('como antes: %d -> %d tri' % (sum(len(p['idx']) // 3 for p in eran.values()), total))
    # ---- what moves cuts nothing
    mov = m.datos['piezas']
    cortes = 0
    for id, d in mov.items():
        if id not in b.ob:
            fallos.append(f"se dice que se mueve '{id}', que no tiene geometría")
            continue
        if d.get('roza'):
            continue
        otros = [o for o in b.ob if o != id and mov.get(o, {}).get('padre') != id]
        pasos = 12
        ejes = len(d.get('ejes', [1]))
        for k in range(pasos + 1):
            # (several axes: each to its end in turn, and all together)
            poses = [[k / pasos if e == j else None for e in range(ejes)] for j in range(ejes)] + ([[k / pasos] * ejes, [k / pasos, 1 - k / pasos]] if ejes > 1 else [])
            for pose in poses:
                b.posar({id: pose if ejes > 1 else pose[0]})
                for o in otros:
                    n, donde = b.solapes(id, o)
                    if n:
                        cortes += n
                        if len(fallos) < 12:
                            fallos.append(f"'{id}' a {k}/{pasos} de su recorrido corta a '{m.piezas[o].nombre}': {n} pares en ({donde[0].x:+.4f} {donde[0].y:+.4f} {donde[0].z:+.4f})..({donde[1].x:+.4f} {donde[1].y:+.4f} {donde[1].z:+.4f})")
    b.posar()
    if mov:
        notas.append(f'{len(mov)} piezas móviles, {cortes} cortes')
    # ---- screens
    for nom, s in m.datos['pantallas'].items():
        c, n, u, r = Vector(s['centro']), Vector(s['normal']), Vector(s['arriba']), Vector(s['derecha'])
        w, h = s['tam']
        # its glass: met from in front, across the whole of it
        for a, bb in ((0, 0), (-0.45, -0.45), (0.45, -0.45), (0.45, 0.45), (-0.45, 0.45)):
            desde = c + r * (a * w) + u * (bb * h) + n * 0.05
            golpe = b.rayo(desde, -n, 0.2)
            if golpe is None or abs(golpe[0] - 0.05) > 0.0006:
                fallos.append(f"pantalla '{nom}': a ({a:+.2f}, {bb:+.2f}) de su centro el cristal no está en su plano (se encuentra {'nada' if golpe is None else '%s a %.1f mm' % (golpe[2], (0.05 - golpe[0]) * 1000)})")
                break
            if golpe[2] not in ('pantalla', 'cristal_oscuro', 'cristal_azul'):
                fallos.append(f"pantalla '{nom}': lo que se ve en ({a:+.2f}, {bb:+.2f}) es '{golpe[2]}', no cristal")
                break
        # nothing in front of it
        delante = [p for p in b.puntos() if (p - c).dot(n) > 0.0005 and abs((p - c).dot(r)) < w / 2 - 0.0005 and abs((p - c).dot(u)) < h / 2 - 0.0005]
        if delante:
            fallos.append(f"pantalla '{nom}': {len(delante)} vértices por delante de su cristal")
    # ---- grips
    for nom, g in m.datos['puntos'].items():
        if 'palma' not in g or 'traves' not in g:
            continue
        en, palma, traves = Vector(g['en']), Vector(g['palma']), Vector(g['traves'])
        if abs(palma.dot(traves)) > 0.05:
            fallos.append(f"punto '{nom}': la palma y el través no son perpendiculares")
        if 'mango' not in g:
            continue
        b.posar()
        lado = palma.cross(traves).normalized()
        gruesos = []
        for d in (palma, lado):
            a, c2 = b.rayo(en, d, 0.2), b.rayo(en, -d, 0.2)
            if a is None or c2 is None:
                fallos.append(f"punto '{nom}': no está dentro de un mango")
                break
            gruesos.append(a[0] + c2[0])
        else:
            if not (g['mango'] - 0.008 <= gruesos[0] <= g['mango'] + 0.008):
                fallos.append(f"punto '{nom}': el mango mide {gruesos[0] * 1000:.0f} mm a través de la palma, no {g['mango'] * 1000:.0f}")
            if not (0.03 <= gruesos[1] <= 0.062):
                fallos.append(f"punto '{nom}': el mango mide {gruesos[1] * 1000:.0f} mm de fondo: no cabe en una mano con guante")
            # long enough for four fingers: still a handle 4 cm either way along it
            for s in (-0.04, 0.04):
                if b.rayo(en + traves * s, palma, 0.06) is None or b.rayo(en + traves * s, -palma, 0.06) is None:
                    fallos.append(f"punto '{nom}': a {s * 100:+.0f} cm a lo largo ya no hay mango")
            notas.append(f"agarre '{nom}' {gruesos[0] * 1000:.0f}x{gruesos[1] * 1000:.0f} mm")
    # ---- lamps
    for nom, l in m.datos['luces'].items():
        id = l.get('pieza', '')
        id = '' if id == 'cuerpo' else id
        if id not in b.ob:
            fallos.append(f"luz '{nom}': no hay pieza '{id}'")
            continue
        ps = b.puntos(id, l.get('material'))
        if not ps:
            fallos.append(f"luz '{nom}': la pieza '{id}' no tiene nada de '{l.get('material')}'")
            continue
        lejos = max((p - Vector(l['en'])).length for p in ps)
        if lejos > l['radio'] * 1.6 + 0.002:
            fallos.append(f"luz '{nom}': su lente llega a {lejos * 1000:.0f} mm de su centro (radio dicho {l['radio'] * 1000:.0f})")
    return fallos, notas


def main():
    args = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
    antes = None
    if '--antes' in args:
        antes = args[args.index('--antes') + 1]
        args.remove(antes)
    nombres = [a for a in args if not a.startswith('--')]
    banco.cargar_recetas()
    usos = kit.estilos_en_uso()
    estilos = ['estilo_' + e for e in kit.ESTILOS if e in usos]
    formas = formas_del_juego()
    finos = acabados()
    mal = 0
    for nombre in nombres or sorted(kit.RECETAS) + sorted(estilos):
        fallos, notas = comprobar(nombre, formas, finos, antes)
        if fallos:
            mal += 1
            print(f'MAL   {nombre}: ' + '; '.join(notas))
            for f in fallos:
                print(f'        - {f}')
        else:
            print(f'bien  {nombre}: ' + '; '.join(notas))
    print(f'{mal} modelos con fallos' if mal else 'todo bien')
    sys.stdout.flush()
    if mal:
        os._exit(1)


main()
