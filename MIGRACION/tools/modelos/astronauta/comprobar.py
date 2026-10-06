"""Checks the weights of a built astronaut, vertex by vertex, and fails loudly when a rule is broken.

    python tools/modelos/astronauta/comprobar.py [assets/models/astronauta.glb]

Plain Python: it reads the `.glb` itself.

The rules (they are what `docs/RIGGING.md` promises):
  - every vertex has weights that add up to 1, on four joints at most;
  - a finger bone only weights the glove of its own side: never the body, never the other hand;
  - on a glove, a vertex beyond the web (where the fingers part) follows the bones of one digit
    alone, and the hand; only the web between the knuckles shares two neighbouring fingers;
  - the gloves carry no body bone but `hand` (the cuff: `forearm` and `hand`);
  - the three bones of every digit lie in one plane, and each one's local X is its hinge;
  - the left side is the mirror image of the right side (bones and weights);
  - the materials the game hides in first person exist, and `NeckRing` / `NeckRingBand` are worn by
    the neck ring alone; the fabric's neck stub stays under the ring's top (no higher than 1.56).
"""
import math
import os
import sys

AQUI = os.path.dirname(os.path.abspath(__file__))
RAIZ = os.path.normpath(os.path.join(AQUI, '..', '..', '..'))
sys.path.insert(0, AQUI)

import glb  # noqa: E402

DEDOS = ('thumb', 'index', 'middle', 'ring', 'pinky')
# what is the same on both sides (the patch, the pocket, the checklist are on one side only)
SIMETRICOS = ('SuitBody', 'SuitStripe', 'Glove', 'GloveGrip', 'Boot', 'Rubber', 'Strap')


def main():
    args = [a for a in (sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else sys.argv[1:]) if not a.startswith('--')]
    ruta = args[0] if args else os.path.join(RAIZ, 'assets', 'models', 'astronauta.glb')
    js, binary = glb.leer(ruta)
    hs = glb.huesos(js)
    nombres = [h[0] for h in hs]
    pos = {h[0]: h[2] for h in hs}
    mat = {h[0]: h[3] for h in hs}
    fallos = []

    def falla(texto):
        fallos.append(texto)
        print('  FALLA:', texto)

    # --- bones
    for lado in 'LR':
        for d in DEDOS:
            a, b, c = (pos[f'{d}.{k}.{lado}'] for k in ('01', '02', '03'))
            # the tip is not a joint: the plane is that of the three heads; the third bone's own Y must lie in it
            u = [b[i] - a[i] for i in range(3)]
            v = [c[i] - b[i] for i in range(3)]
            n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
            ln = math.sqrt(sum(x * x for x in n))
            n = [x / ln for x in n]
            ang = math.degrees(math.asin(min(1.0, ln / (math.dist(a, b) * math.dist(b, c)))))
            if ang < 15:
                falla(f'{d}.{lado}: the rest pose is too straight to find the hinge ({ang:.1f} deg)')
            for k in ('01', '02', '03'):
                m = mat[f'{d}.{k}.{lado}']
                x = (m[0][0], m[1][0], m[2][0])
                y = (m[0][1], m[1][1], m[2][1])
                if sum(x[i] * n[i] for i in range(3)) < 0.9999:
                    falla(f'{d}.{k}.{lado}: local X is not the hinge')
                if abs(sum(y[i] * n[i] for i in range(3))) > 1e-4:
                    falla(f'{d}.{k}.{lado}: the bone leaves the plane of its digit')
    for n in nombres:
        if n.endswith('.L'):
            a, b = pos[n], pos[n[:-2] + '.R']
            if max(abs(a[0] + b[0]), abs(a[1] - b[1]), abs(a[2] - b[2])) > 1e-5:
                falla(f'{n} is not the mirror of {n[:-2]}.R')

    # --- what first person hides
    hay = {m['name'] for m in js['materials']}
    for m in ('Helmet', 'HelmetDark', 'HelmetInner', 'Visor', 'Lamp', 'NeckRing', 'NeckRingBand'):
        if m not in hay:
            falla(f'no material named {m}')
    for node in js['nodes']:
        if 'mesh' not in node:
            continue
        for prim in js['meshes'][node['mesh']]['primitives']:
            material = js['materials'][prim['material']]['name']
            pp, _ = glb.accessor(js, binary, prim['attributes']['POSITION'])
            if material in ('NeckRing', 'NeckRingBand'):
                # the ring: 6 cm tall round the neck, 17 cm of radius
                fuera = [p for p in pp if not (1.455 <= p[1] <= 1.53 and math.hypot(p[0], p[2] + 0.01) <= 0.175)]
                if fuera:
                    falla(f'{material} is worn by something that is not the neck ring ({len(fuera)} vertices, e.g. {fuera[0]})')
            if material in ('SuitBody', 'SuitStripe'):
                alto = [p for p in pp if math.hypot(p[0], p[2]) < 0.2 and p[1] > 1.56]
                if alto:
                    falla(f'the fabric rises above the neck ring: {len(alto)} vertices, up to y = {max(p[1] for p in alto):.3f}')

    # --- weights
    total = 0
    web = 0
    por_malla = {}
    suma = {}
    for node in js['nodes']:
        if 'mesh' not in node:
            continue
        for prim in js['meshes'][node['mesh']]['primitives']:
            at = prim['attributes']
            material = js['materials'][prim['material']]['name']
            pp, _ = glb.accessor(js, binary, at['POSITION'])
            jj, _ = glb.accessor(js, binary, at['JOINTS_0'])
            ww, wa = glb.accessor(js, binary, at['WEIGHTS_0'])
            den = {5121: 255.0, 5123: 65535.0}.get(wa['componentType'], 1.0)
            if 'JOINTS_1' in at:
                falla(f"{node['name']}: more than four joints on a vertex")
            guante = node['name'].startswith('Glove.')
            for p, j4, w4 in zip(pp, jj, ww):
                total += 1
                w = {nombres[j]: x / den for j, x in zip(j4, w4) if x > 0}
                if material in SIMETRICOS:
                    for n, x in w.items():
                        suma[n] = suma.get(n, 0.0) + x
                if abs(sum(w.values()) - 1.0) > 0.01:
                    falla(f"{node['name']}: weights add up to {sum(w.values()):.3f} at {p}")
                dedos = {(n.split('.')[0], n[-1]) for n in w if n.split('.')[0] in DEDOS}
                por_malla.setdefault((node['name'], material), set()).update(w)
                if not guante:
                    if dedos:
                        falla(f"{node['name']}: a finger bone weights a vertex at {p}")
                    continue
                lado = node['name'][-1]
                otros = [n for n in w if n.split('.')[0] not in DEDOS and n != f'hand.{lado}']
                if otros:
                    falla(f"{node['name']}: {otros} weights the glove at {p}")
                if any(l != lado for _d, l in dedos):
                    falla(f"{node['name']}: a bone of the other hand at {p}")
                if len(dedos) > 1:
                    # two digits on a vertex: only on the web, where both are first phalanges mixed with the hand
                    web += 1
                    if any(not n.endswith(f'.01.{lado}') for n in w if n.split('.')[0] in DEDOS):
                        falla(f"{node['name']}: two fingers share a vertex beyond their first phalanx at {p}")
                    if 'thumb' in {d for d, _l in dedos}:
                        falla(f"{node['name']}: the thumb shares a vertex with a finger at {p}")
    for n in sorted(suma):
        if n.endswith('.L'):
            a, b = suma[n], suma.get(n[:-2] + '.R', 0.0)
            if abs(a - b) > 0.02 * max(a, b) + 0.5:
                falla(f'{n} carries {a:.1f} vertices in all, {n[:-2]}.R {b:.1f}: the sides are not mirror images')
    print(f'{total} vertices checked; {web} on the webs between knuckles share two fingers')
    for (malla, material), usados in sorted(por_malla.items()):
        if malla.startswith('Glove'):
            print(f'  {malla} / {material}: {len(usados)} joints')
    if fallos:
        raise SystemExit(f'{len(fallos)} rules broken')
    print('todo en regla')


main()
