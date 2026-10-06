"""Lists the pieces of every component and part kind of the game, as a recipe needs them:

    python tools/modelos/piezas.py [name...]

(plain Python: it does not need Blender.)
"""
import json
import os
import re
import sys

RAIZ = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))


def jsonc(path):
    text = open(path, encoding='utf-8').read()
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == '\\' else 1
            out.append(text[i:j + 1])
            i = j + 1
        elif text.startswith('//', i):
            while i < n and text[i] != '\n':
                i += 1
        else:
            out.append(c)
            i += 1
    return json.loads(re.sub(r',(\s*[}\]])', r'\1', ''.join(out)))


def forma(f):
    if f['kind'] == 'box':
        return 'caja %s' % f['size']
    if f['kind'] == 'cylinder':
        return 'cil r%.3g h%.3g%s' % (f['radius'], f['height'], (' taper %.3g' % f['taper']) if 'taper' in f else '')
    return f['kind'] + str(f.get('size', ''))


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    quiero = set(sys.argv[1:])
    d = os.path.join(RAIZ, 'assets', 'defs', 'components')
    for fichero in sorted(os.listdir(d)):
        for k, v in jsonc(os.path.join(d, fichero)).items():
            if quiero and k not in quiero:
                continue
            print(f"{k}: {v['nombre']}" + (' [luz]' if 'luz' in v else '') + (f" [maq {v['maquina'].get('modelo')}]" if 'maquina' in v else ''))
            for p in v['piezas']:
                print(f"   '{p.get('id', '')}': {forma(p['forma'])} en {p.get('en', [0, 0, 0])}" + (f" rot {p['rot']}" if 'rot' in p else '') + (f" color {p['color']}" if 'color' in p else '') + (f" mat {p['material']}" if 'material' in p else ''))
            if 'luz' in v:
                print(f"   luz en {v['luz'].get('en')} dir {v['luz'].get('dir')} lente {v['luz'].get('lente')}")
    d = os.path.join(RAIZ, 'assets', 'defs', 'structures', 'parts')
    for fichero in sorted(os.listdir(d)):
        for k, v in jsonc(os.path.join(d, fichero)).items():
            if quiero and k not in quiero:
                continue
            print(f"{k}: {v['name']} (parte) {forma(v['shape'])} mat {v['material']}")


main()
