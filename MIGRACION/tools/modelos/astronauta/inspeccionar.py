"""Reads a GLB and says what the game will see: joints (in glTF order, with parent and rest head
position in the model's axes), meshes, vertex/triangle counts, materials, textures, animations and
a few checks on the skin weights.

    python tools/modelos/astronauta/inspeccionar.py [assets/models/astronauta.glb] [--pesos] [--manos]

Plain Python (no Blender needed; Blender's own Python runs it too). `--pesos`: per material, which
joints its vertices use. `--manos`: palm centre, knuckle line, palm normal and hinge axes of each
hand, from the joints.
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import glb  # noqa: E402


def sub(a, b):
    return tuple(a[i] - b[i] for i in range(3))


def add(a, b):
    return tuple(a[i] + b[i] for i in range(3))


def mul(a, k):
    return tuple(a[i] * k for i in range(3))


def dot(a, b):
    return sum(a[i] * b[i] for i in range(3))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def norm(a):
    l = math.sqrt(dot(a, a)) or 1.0
    return tuple(x / l for x in a)


def f3(v):
    return '(%+.4f, %+.4f, %+.4f)' % tuple(v)


DEDOS = ('thumb', 'index', 'middle', 'ring', 'pinky')


def piel(js, binary, malla, origen, n, radio=0.007):
    """How far along `n` from `origen` the skin of the mesh node `malla` lies (its vertices within
    `radio` of that line, the farthest one)."""
    mejor = None
    for node in js['nodes']:
        if node.get('name') != malla or 'mesh' not in node:
            continue
        for prim in js['meshes'][node['mesh']]['primitives']:
            for p in glb.accessor(js, binary, prim['attributes']['POSITION'])[0]:
                d = sub(p, origen)
                t = dot(d, n)
                if t > 0 and math.dist(d, mul(n, t)) < radio and (mejor is None or t > mejor):
                    mejor = t
    return mejor


def decir_manos(js, binary=None, out=print):
    """Palm centre, knuckle line and palm normal of each hand, from the joints (and how far from
    the bones' plane the palm's skin is, from the mesh)."""
    hs = {h[0]: h for h in glb.huesos(js)}
    pos = {k: h[2] for k, h in hs.items()}
    for s in ('L', 'R'):
        mu = pos[f'hand.{s}']
        k_index, k_pinky = pos[f'index.01.{s}'], pos[f'pinky.01.{s}']
        k_mid = mul(add(add(pos[f'index.01.{s}'], pos[f'middle.01.{s}']), add(pos[f'ring.01.{s}'], pos[f'pinky.01.{s}'])), 0.25)
        centro = mul(add(mu, k_mid), 0.5)
        a_lo_largo = norm(sub(k_mid, mu))                 # wrist -> knuckles
        hacia_indice = norm(sub(k_index, k_pinky))        # little finger -> index
        # the palm looks where the fingers curl to
        n = norm(cross(a_lo_largo, hacia_indice))
        if dot(n, sub(pos[f'middle.03.{s}'], pos[f'middle.01.{s}'])) < 0:
            n = mul(n, -1)
        out(f'mano {s}:')
        out(f'  muneca (hand.{s})          {f3(mu)}')
        out(f'  nudillo del indice        {f3(k_index)}')
        out(f'  nudillo del menique       {f3(k_pinky)}')
        out(f'  centro de los nudillos    {f3(k_mid)}')
        out(f'  centro de la palma        {f3(centro)}   (en el plano de los huesos)')
        out(f'  normal de la palma        {f3(n)}')
        out(f'  muneca -> nudillos        {f3(a_lo_largo)}')
        out(f'  menique -> indice         {f3(hacia_indice)}')
        if binary is not None:
            a = piel(js, binary, f'Glove.{s}', centro, n)
            agarre = sub(k_mid, mul(a_lo_largo, 0.012))
            b = piel(js, binary, f'Glove.{s}', agarre, n)
            out(f'  piel de la palma          a {a * 1000:.1f} mm del centro de la palma segun la normal: {f3(add(centro, mul(n, a)))}')
            out(f'  linea de agarre           {f3(agarre)} (12 mm antes de los nudillos); la piel, a {b * 1000:.1f} mm segun la normal')
        for d in DEDOS:
            a, b, c = pos[f'{d}.01.{s}'], pos[f'{d}.02.{s}'], pos[f'{d}.03.{s}']
            eje = norm(cross(sub(b, a), sub(c, b)))
            # the joint's own X axis (first column of its world matrix) should be that hinge
            peor = 1.0
            for k in ('01', '02', '03'):
                m = hs[f'{d}.{k}.{s}'][3]
                peor = min(peor, dot(eje, (m[0][0], m[1][0], m[2][0])))
            out(f'  bisagra {d:6s}  {f3(eje)}   falanges {math.dist(a, b) * 1000:.1f} / {math.dist(b, c) * 1000:.1f} mm   eje.X local = {peor:+.4f}')


def informe(path, pesos=False, manos_tambien=False, manos=None):
    manos_tambien = manos_tambien or bool(manos)
    js, binary = glb.leer(path)
    print(f'{path}: {os.path.getsize(path)} bytes ({os.path.getsize(path) / 1024:.0f} KB)')
    print('generator:', js['asset'].get('generator'))
    print(f"skins: {len(js.get('skins', []))}  animations: {len(js.get('animations', []))}  "
          f"textures: {len(js.get('textures', []))}  images: {len(js.get('images', []))}")
    hs = glb.huesos(js)
    print(f'joints ({len(hs)}):')
    print('  #  name              parent            head (x, y, z)')
    for i, (name, pname, head, _m) in enumerate(hs):
        print(f'  {i:2d} {name:17s} {str(pname):17s} {f3(head)}')
    # scale check: the columns of every joint's world matrix must be unit length
    peor = 0.0
    for _n, _p, _h, m in hs:
        for c in range(3):
            peor = max(peor, abs(math.sqrt(sum(m[r][c] ** 2 for r in range(3))) - 1.0))
    print(f'joint scale error (must be ~0): {peor:.2e}')

    tv = tt = 0
    usados = {}
    joint_names = [h[0] for h in hs]
    por_material = {}
    max_inf = 0
    print('meshes:')
    for n in js['nodes']:
        if 'mesh' not in n:
            continue
        mesh = js['meshes'][n['mesh']]
        for p in mesh['primitives']:
            at = p['attributes']
            nv = js['accessors'][at['POSITION']]['count']
            nt = js['accessors'][p['indices']]['count'] // 3
            mat = js['materials'][p['material']]['name'] if 'material' in p else None
            tv += nv
            tt += nt
            print(f"  {n['name']:14s} {str(mat):12s} {nv:6d} v {nt:6d} t  skin={n.get('skin')}  {sorted(at)}")
            if 'JOINTS_0' in at:
                jj, _ = glb.accessor(js, binary, at['JOINTS_0'])
                ww, wa = glb.accessor(js, binary, at['WEIGHTS_0'])
                den = {5121: 255.0, 5123: 65535.0}.get(wa['componentType'], 1.0)
                for j4, w4 in zip(jj, ww):
                    s = sum(w4) / den
                    if abs(s - 1.0) > 0.02:
                        usados['__no_normalizado__'] = usados.get('__no_normalizado__', 0) + 1
                    max_inf = max(max_inf, sum(1 for w in w4 if w > 0))
                    for j, w in zip(j4, w4):
                        if w > 0:
                            usados[joint_names[j]] = usados.get(joint_names[j], 0) + 1
                            por_material.setdefault(mat, {}).setdefault(joint_names[j], 0)
                            por_material[mat][joint_names[j]] += 1
    print(f'TOTAL {tv} vertices, {tt} triangles; at most {max_inf} joints on a vertex')
    print('materials:', ', '.join(m['name'] for m in js['materials']))
    con_tex = [m['name'] for m in js['materials'] if 'normalTexture' in m or 'baseColorTexture' in m.get('pbrMetallicRoughness', {})]
    print('materials with textures:', con_tex or 'none')
    sin = [n for n in joint_names if n not in usados]
    print('joints without any weight:', ', '.join(sin) or 'none')
    if usados.get('__no_normalizado__'):
        print('!! vertices whose weights do not add up to 1:', usados['__no_normalizado__'])
    if pesos:
        for mat, d in sorted(por_material.items(), key=lambda kv: str(kv[0])):
            print(f'  {mat}: ' + ', '.join(f'{k}={v}' for k, v in sorted(d.items())))
    if manos_tambien:
        decir_manos(js, binary)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    informe(args[0] if args else 'assets/models/astronauta.glb', '--pesos' in sys.argv, '--manos' in sys.argv)


if __name__ == '__main__':
    main()
