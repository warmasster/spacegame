"""Reads and rewrites a `.glb` with plain Python (no Blender): what `inspeccionar.py` prints and the
one fix-up the export needs (`ordenar_huesos`: a fixed joint order)."""
import json
import struct

COMP = {5120: ('b', 1), 5121: ('B', 1), 5122: ('h', 2), 5123: ('H', 2), 5125: ('I', 4), 5126: ('f', 4)}
NCOMP = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}


def leer(path):
    """(json, binary chunk as a bytearray)."""
    with open(path, 'rb') as f:
        data = f.read()
    magic, _version, length = struct.unpack_from('<4sII', data, 0)
    assert magic == b'glTF', 'not a GLB'
    off = 12
    js, binary = None, bytearray()
    while off < length:
        clen, ctype = struct.unpack_from('<I4s', data, off)
        chunk = data[off + 8: off + 8 + clen]
        if ctype == b'JSON':
            js = json.loads(chunk.decode('utf-8'))
        elif ctype.startswith(b'BIN'):
            binary = bytearray(chunk)
        off += 8 + clen
    return js, binary


def escribir(path, js, binary):
    texto = json.dumps(js, separators=(',', ':')).encode('utf-8')
    texto += b' ' * (-len(texto) % 4)
    binary = bytes(binary) + b'\x00' * (-len(binary) % 4)
    with open(path, 'wb') as f:
        f.write(struct.pack('<4sII', b'glTF', 2, 12 + 8 + len(texto) + 8 + len(binary)))
        f.write(struct.pack('<I4s', len(texto), b'JSON'))
        f.write(texto)
        f.write(struct.pack('<I4s', len(binary), b'BIN\x00'))
        f.write(binary)


def _donde(js, idx):
    a = js['accessors'][idx]
    bv = js['bufferViews'][a['bufferView']]
    fmt, size = COMP[a['componentType']]
    n = NCOMP[a['type']]
    stride = bv.get('byteStride') or size * n
    base = bv.get('byteOffset', 0) + a.get('byteOffset', 0)
    return a, '<' + fmt * n, base, stride


def accessor(js, binary, idx):
    """([tuple per element], accessor)."""
    a, fmt, base, stride = _donde(js, idx)
    return [struct.unpack_from(fmt, binary, base + i * stride) for i in range(a['count'])], a


def poner(js, binary, idx, valores):
    a, fmt, base, stride = _donde(js, idx)
    assert len(valores) == a['count']
    for i, v in enumerate(valores):
        struct.pack_into(fmt, binary, base + i * stride, *v)


def mat_mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def node_local(n):
    if 'matrix' in n:
        m = n['matrix']
        return [[m[c * 4 + r] for c in range(4)] for r in range(4)]
    t = n.get('translation', [0, 0, 0])
    x, y, z, w = n.get('rotation', [0, 0, 0, 1])
    s = n.get('scale', [1, 1, 1])
    r = [[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
         [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
         [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]]
    return [[r[0][0] * s[0], r[0][1] * s[1], r[0][2] * s[2], t[0]],
            [r[1][0] * s[0], r[1][1] * s[1], r[1][2] * s[2], t[1]],
            [r[2][0] * s[0], r[2][1] * s[1], r[2][2] * s[2], t[2]],
            [0, 0, 0, 1]]


def mundo(js):
    """(world matrix of every node, {node: parent node})."""
    nodes = js['nodes']
    parent = {}
    for i, n in enumerate(nodes):
        for c in n.get('children', []):
            parent[c] = i
    cache = {}

    def w(i):
        if i not in cache:
            m = node_local(nodes[i])
            cache[i] = mat_mul(w(parent[i]), m) if i in parent else m
        return cache[i]

    return [w(i) for i in range(len(nodes))], parent


def huesos(js):
    """[(name, parent name or None, head (x, y, z), world matrix)] in the skin's joint order."""
    world, parent = mundo(js)
    out = []
    for skin in js.get('skins', []):
        joints = skin['joints']
        dentro = set(joints)
        for j in joints:
            p = parent.get(j)
            pname = js['nodes'][p]['name'] if p in dentro else None
            m = world[j]
            out.append((js['nodes'][j]['name'], pname, (m[0][3], m[1][3], m[2][3]), m))
    return out


def ordenar_huesos(path, orden):
    """Rewrites the file so the skin's joints come in `orden` (names; joints not named keep their
    place after those). Vertices and inverse bind matrices follow: nothing moves."""
    js, binary = leer(path)
    assert len(js.get('skins', [])) == 1, 'one skin expected'
    skin = js['skins'][0]
    viejo = skin['joints']
    nombres = [js['nodes'][j]['name'] for j in viejo]
    falta = [n for n in orden if n not in nombres]
    assert not falta, f'bones not in the file: {falta}'
    nuevo_nombres = list(orden) + [n for n in nombres if n not in orden]
    if nuevo_nombres == nombres:
        return nombres
    a_nuevo = {nombres.index(n): k for k, n in enumerate(nuevo_nombres)}     # old joint index -> new
    skin['joints'] = [viejo[nombres.index(n)] for n in nuevo_nombres]
    ibm, _ = accessor(js, binary, skin['inverseBindMatrices'])
    poner(js, binary, skin['inverseBindMatrices'], [ibm[nombres.index(n)] for n in nuevo_nombres])
    hechos = set()
    for mesh in js['meshes']:
        for prim in mesh['primitives']:
            for clave, idx in prim['attributes'].items():
                if clave.startswith('JOINTS_') and idx not in hechos:
                    hechos.add(idx)
                    vals, _ = accessor(js, binary, idx)
                    pesos, _ = accessor(js, binary, prim['attributes']['WEIGHTS_' + clave[7:]])
                    # a joint with no weight is written as 0: keep it 0 rather than whatever 0 maps to
                    poner(js, binary, idx, [tuple(a_nuevo[j] if w > 0 else 0 for j, w in zip(v, p)) for v, p in zip(vals, pesos)])
    escribir(path, js, binary)
    return nuevo_nombres
