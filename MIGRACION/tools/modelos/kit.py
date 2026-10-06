"""The workshop: what a recipe builds a model with.

A model is the look of a thing the game already knows by its shapes (a kind of component of
`assets/defs/components`, a kind of part of `assets/defs/structures/parts`, a tool): the game keeps
those shapes for everything physical, and draws the model instead of them from close by.

A recipe is a function that takes a `Modelo` and puts geometry on its pieces:

    @receta('bidon_agua')
    def bidon(m):
        p = m['']                                   # the piece without a name (its body)
        m.torno([(0, -p.h/2), (p.r, -p.h/2), ...], mat=TINTE, pieza=p, marco=p)

Everything is said in the GAME's frame — x to port, y up, z forward, metres — and by default in
the component's own (as its pieces are placed in the data); `marco=p` works in a piece's own
frame instead (its centre, its axes). Blender's own axes never show.

A piece's surface as its data has it (its paint, its finish) is the material `TINTE`: what is
given it takes the colour the ship gives that piece, so one model serves every colour. Every
other material is a colour of the model's own (`M('acero')`...; `#finish` picks the finish the
game draws it with).

`Modelo.terminar()` joins what each piece was given, bakes the shade of its corners into its
vertices (Cycles, ambient occlusion), and writes `assets/models/<name>.glb` with each piece's
triangles in that piece's frame, which is what the game reads (`core/structure/models.rs`).
"""
import json
import math
import os
import re
import struct

import bmesh
import bpy
from mathutils import Matrix, Vector

RAIZ = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
SALIDA = os.path.join(RAIZ, 'assets', 'models')
VISTAS = os.path.join(RAIZ, 'out', 'modelos')

# ---------------------------------------------------------------------------------------------
# data


def jsonc(path):
    """A .jsonc file: comments and trailing commas out."""
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
        elif text.startswith('/*', i):
            i = text.index('*/', i) + 2
        else:
            out.append(c)
            i += 1
    return json.loads(re.sub(r',(\s*[}\]])', r'\1', ''.join(out)))


def componentes():
    """Every component kind of the game, by id."""
    out = {}
    d = os.path.join(RAIZ, 'assets', 'defs', 'components')
    for f in sorted(os.listdir(d)):
        if f.endswith('.jsonc'):
            out.update(jsonc(os.path.join(d, f)))
    return out


def partes():
    """Every plain part kind (structures), by id."""
    out = {}
    d = os.path.join(RAIZ, 'assets', 'defs', 'structures', 'parts')
    for f in sorted(os.listdir(d)):
        if f.endswith('.jsonc'):
            out.update(jsonc(os.path.join(d, f)))
    return out


def naves():
    """Every ship of the game, by id."""
    out = {}
    d = os.path.join(RAIZ, 'assets', 'defs', 'ships')
    for f in sorted(os.listdir(d)):
        if f.endswith('.jsonc'):
            out[f[:-6]] = jsonc(os.path.join(d, f))
    return out


def clave_forma(forma):
    """A shape's size as a name, to the millimetre — the same the game makes
    (`crates/ship/src/components.rs`, `shape_key`): `b300x660x700`, `c85x1000`, `w100x1200x1600`,
    and a hull of points `h` and eight hex digits."""
    import numpy as np

    def mm(v):
        # (as the game rounds it: in single precision, halves away from zero)
        x = float(np.float32(v) * np.float32(1000.0))
        return int(math.floor(abs(x) + 0.5)) * (1 if x >= 0 else -1)

    k = forma.get('kind')
    if k == 'box':
        return 'b%dx%dx%d' % tuple(mm(v) for v in forma['size'])
    if k == 'cylinder':
        t = forma.get('taper', 1.0)
        return 'c%dx%d' % (mm(forma['radius']), mm(forma['height'])) + ('' if abs(t - 1.0) < 5e-4 else 't%d' % mm(t))
    if k == 'wedge':
        return 'w%dx%dx%d' % tuple(mm(v) for v in forma['size'])
    if k == 'hull':
        # by its points to the millimetre, in their order: FNV-1a over them as little-endian i32
        import struct
        h = 0x811c9dc5
        for p in forma['points']:
            for v in p:
                for b in struct.pack('<i', mm(v)):
                    h = ((h ^ b) * 0x01000193) & 0xffffffff
        return 'h%08x' % h
    return None


def estilos_en_uso():
    """The styles the ships' plain shapes ask for: {style: {shape key: (shape, component)}}."""
    out = {}
    for nave in naves().values():
        for c in nave.get('componentes', []):
            if 'estilo' in c and 'forma' in c:
                k = clave_forma(c['forma'])
                if k:
                    out.setdefault(c['estilo'], {}).setdefault(k, (c['forma'], c))
    return out


# ---------------------------------------------------------------------------------------------
# frames: the game's axes (x port, y up, z forward) against Blender's (x, -z forward... z up)


def B(v):
    """A point of the game in Blender's axes."""
    return Vector((v[0], -v[2], v[1]))


def rot_xyz(deg):
    """The game's rotation of a piece (glam `EulerRot::XYZ`, degrees): Rx · Ry · Rz."""
    x, y, z = (math.radians(a) for a in deg)
    return Matrix.Rotation(x, 3, 'X') @ Matrix.Rotation(y, 3, 'Y') @ Matrix.Rotation(z, 3, 'Z')


class Marco:
    """A frame of the game: where its origin is and how it is turned."""

    def __init__(self, c=(0, 0, 0), R=None):
        self.c = Vector(c)
        self.R = R.copy() if R is not None else Matrix.Identity(3)

    def a_mundo(self, p):
        return self.c + self.R @ Vector(p)

    def a_local(self, p):
        return self.R.transposed() @ (Vector(p) - self.c)

    def girado(self, deg):
        """The same turned (degrees, about its own x, y, z in turn)."""
        return Marco(self.c, self.R @ rot_xyz(deg))

    def en(self, p):
        """The same moved to `p` (in its own axes)."""
        return Marco(self.a_mundo(p), self.R)


MUNDO = Marco()

EJES = {'x': Vector((1, 0, 0)), 'y': Vector((0, 1, 0)), 'z': Vector((0, 0, 1))}


def base_de(eje):
    """A frame's rotation whose y is `eje` ('x', 'y', 'z', or a vector): what is built round y
    (a lathe, a cylinder) comes out round that axis."""
    if isinstance(eje, str):
        return {'y': Matrix.Identity(3), 'x': rot_xyz((0, 0, -90)), 'z': rot_xyz((90, 0, 0))}[eje]
    y = Vector(eje).normalized()
    x = y.orthogonal().normalized()
    z = x.cross(y)
    return Matrix((x, y, z)).transposed()


class Pieza(Marco):
    """A piece of the thing: its shape in the data, its frame in the component's."""

    def __init__(self, id, forma, en=(0, 0, 0), rot=(0, 0, 0), datos=None):
        super().__init__(en, rot_xyz(rot))
        self.id = id
        self.forma = forma
        self.datos = datos or {}
        k = forma.get('kind')
        if k == 'box':
            self.tam = Vector(forma['size'])
            self.r = self.h = None
        elif k == 'cylinder':
            self.r = forma['radius']
            self.h = forma['height']
            self.taper = forma.get('taper', 1.0)
            self.tam = Vector((self.r * 2 * max(1.0, self.taper), self.h, self.r * 2 * max(1.0, self.taper)))
        elif k == 'wedge':
            self.tam = Vector(forma['size'])
            self.r = self.h = None
        else:
            pts = [Vector(p) for p in forma.get('points', [(0, 0, 0)])]
            lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
            hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
            self.tam = hi - lo
            self.r = self.h = None

    @property
    def nombre(self):
        return self.id or 'cuerpo'

    @property
    def semi(self):
        """How far it reaches from its origin each way: half its size, centred (a hull of points
        need not be: as far as its farthest point, as the game measures it)."""
        if self.forma.get('kind') == 'hull':
            pts = [Vector(p) for p in self.forma.get('points', [(0, 0, 0)])]
            return Vector(tuple(max(abs(p[i]) for p in pts) for i in range(3)))
        return self.tam * 0.5


# ---------------------------------------------------------------------------------------------
# materials

TINTE = 'tinte'

# name: (sRGB 0-255, roughness, metalness, glow): the colours models have of their own
PALETA = {
    'acero': ((150, 153, 158), 0.42, 0.9, 0),
    'acero_oscuro': ((62, 65, 70), 0.5, 0.85, 0),
    'cromo': ((205, 208, 212), 0.16, 1.0, 0),
    'aluminio': ((176, 179, 183), 0.36, 0.9, 0),
    'aluminio_anodizado': ((44, 48, 56), 0.38, 0.8, 0),
    'laton': ((176, 142, 70), 0.34, 1.0, 0),
    'cobre': ((176, 104, 66), 0.36, 1.0, 0),
    'titanio': ((120, 116, 110), 0.44, 0.9, 0),
    'tobera': ((46, 42, 40), 0.55, 0.8, 0),
    'goma#goma': ((22, 22, 24), 0.9, 0.0, 0),
    'plastico_negro#plastico': ((26, 27, 30), 0.6, 0.0, 0),
    'plastico_gris#plastico': ((92, 96, 102), 0.6, 0.0, 0),
    'plastico_claro#plastico': ((196, 198, 194), 0.6, 0.0, 0),
    'pintura_gris#pintura': ((108, 112, 118), 0.55, 0.1, 0),
    'pintura_oscura#pintura': ((48, 51, 56), 0.55, 0.1, 0),
    'pintura_blanca#pintura': ((218, 218, 212), 0.5, 0.05, 0),
    'pintura_amarilla#pintura': ((214, 168, 28), 0.5, 0.05, 0),
    'pintura_naranja#pintura': ((206, 96, 30), 0.5, 0.05, 0),
    'pintura_roja#pintura': ((176, 40, 32), 0.5, 0.05, 0),
    'pintura_azul#pintura': ((40, 84, 150), 0.5, 0.05, 0),
    'pintura_verde#pintura': ((62, 104, 66), 0.5, 0.05, 0),
    'pintura_oliva#pintura': ((78, 88, 62), 0.6, 0.05, 0),
    'tela_cincha#trenzado': ((206, 146, 34), 0.85, 0.0, 0),
    'tela_gris#tela': ((96, 98, 102), 0.9, 0.0, 0),
    'tela_negra#tela': ((30, 31, 34), 0.9, 0.0, 0),
    'acolchado#acolchado': ((52, 60, 76), 0.85, 0.0, 0),
    'aislante#aislante': ((196, 172, 96), 0.4, 0.6, 0),
    'carbono#carbono': ((30, 31, 34), 0.4, 0.2, 0),
    'cristal_oscuro': ((12, 16, 20), 0.08, 0.0, 0),
    'lente': ((230, 232, 226), 0.1, 0.0, 0),
    'luz_blanca': ((255, 246, 228), 0.3, 0.0, 6.0),
    'luz_ambar': ((255, 170, 40), 0.3, 0.0, 6.0),
    'luz_roja': ((255, 40, 30), 0.3, 0.0, 6.0),
    'luz_verde': ((60, 255, 110), 0.3, 0.0, 6.0),
    'luz_cian': ((90, 210, 255), 0.3, 0.0, 4.0),
    'piloto_verde': ((60, 255, 110), 0.3, 0.0, 2.0),
    'piloto_ambar': ((255, 170, 40), 0.3, 0.0, 2.0),
    'piloto_rojo': ((255, 50, 36), 0.3, 0.0, 2.0),
    'pantalla': ((6, 9, 12), 0.1, 0.0, 0),
    # (added with the hand gear: beta cloth, a camera's silver paint, anodised colours, lamps)
    'pintura_plata#pintura': ((186, 189, 192), 0.42, 0.55, 0),
    'pintura_negra#pintura': ((24, 25, 28), 0.5, 0.1, 0),
    'tela_blanca#tela': ((214, 212, 204), 0.9, 0.0, 0),
    'anodizado_rojo': ((150, 30, 30), 0.34, 0.85, 0),
    'anodizado_azul': ((32, 70, 150), 0.34, 0.85, 0),
    'anodizado_oro': ((184, 140, 60), 0.34, 0.9, 0),
    'cristal_azul': ((20, 34, 52), 0.06, 0.0, 0),
    'luz_cian_suave': ((90, 210, 255), 0.3, 0.0, 1.5),
    'piloto_cian': ((90, 210, 255), 0.3, 0.0, 2.0),
    # a lamp the game lights: unlit glass of its own colour until the game tints it
    'tinte_luz_baliza': ((255, 196, 92), 0.25, 0.0, 0),
}
PALETA[TINTE] = ((255, 255, 255), 0.5, 0.0, 0)


def lineal(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def M(nombre):
    """The material of that name (of `PALETA`, with or without its `#finish`)."""
    clave = nombre if nombre in PALETA else next((k for k in PALETA if k.split('#')[0] == nombre), None)
    if clave is None:
        raise KeyError(f"no hay material '{nombre}' (hay: {', '.join(sorted(k.split('#')[0] for k in PALETA))})")
    mat = bpy.data.materials.get(clave)
    if mat is None:
        rgb, rough, metal, glow = PALETA[clave]
        mat = bpy.data.materials.new(clave)
        mat['rgb'] = [lineal(c) for c in rgb]
        mat['rough'] = rough
        mat['metal'] = metal
        mat['glow'] = glow
        # (what a preview shows it as; a tint is shown as bare paint)
        shown = (150, 156, 164) if clave == TINTE else rgb
        mat.diffuse_color = [lineal(c) for c in shown] + [1.0]
        mat.roughness = rough
        mat.metallic = metal
    return mat


# ---------------------------------------------------------------------------------------------
# the model


RECETAS = {}


def receta(*nombres):
    """Says that the function builds the model(s) of that name (several: one recipe for several
    kinds of the same build)."""

    def registrar(f):
        for n in nombres:
            RECETAS[n] = f
        return f

    return registrar


ESTILOS = {}


def estilo(*nombres):
    """Says that the function builds a plain shape seen as that style: `f(m, p)` puts geometry
    on piece `p` (a box, a cylinder, a wedge or a hull of points, of whatever size a ship gave
    it), in its frame."""

    def registrar(f):
        for n in nombres:
            ESTILOS[n] = f
        return f

    return registrar


class Modelo:
    def __init__(self, nombre, piezas):
        self.nombre = nombre
        self.piezas = {p.id: p for p in piezas}
        self.objetos = []
        # what cuts holes in others (gone once they are joined), and the pieces a picture shows
        self.cortadores = []
        self.vista_de = None
        # the pieces whose shade is baked on their own: what the game takes off the thing or
        # swings clear of it (a rocket in its chamber, a hatch) neither shades the rest nor is
        # shaded by it
        self.sombra_aparte = ()
        # what a recipe says of its model for whoever uses it (`mueve`, `punto`, `pantalla`,
        # `luz`, `sosten`): written beside the recipes as `datos/<name>.json`
        self.datos = {'piezas': {}, 'puntos': {}, 'pantallas': {}, 'luces': {}}
        self.sosten = None
        self.marco_dicho = None
        # how many triangles it may have at the most (`comprobar.py` holds it to that)
        self.presupuesto = None

    @staticmethod
    def de(nombre):
        """The model of the thing of that name, with its pieces as the game's data has them: a
        kind of component, else a kind of plain part (one piece, its body)."""
        comps = componentes()
        if nombre in comps:
            k = comps[nombre]
            return Modelo(nombre, [Pieza(p.get('id', ''), p['forma'], p.get('en', (0, 0, 0)), p.get('rot', (0, 0, 0)), p) for p in k['piezas']])
        ps = partes()
        if nombre.startswith('parte_') and nombre[6:] in ps:
            return Modelo(nombre, [Pieza('', ps[nombre[6:]]['shape'], datos=ps[nombre[6:]])])
        if nombre.startswith('estilo_'):
            # a style: one piece for every size the ships use it with, each named after its size
            # and set well apart from the others (they are not one thing: none shades another)
            usos = estilos_en_uso().get(nombre[7:], {})
            paso = max([max(Pieza(k, f).tam) for k, (f, _) in usos.items()] + [1.0]) * 1.5 + 2.0
            m = Modelo(nombre, [Pieza(k, f, (i * paso, 0, 0), datos=c) for i, (k, (f, c)) in enumerate(sorted(usos.items()))])
            # (the picture: the biggest of them)
            m.vista_de = max(usos, key=lambda k: max(Pieza(k, usos[k][0]).tam)) if usos else None
            return m
        return Modelo(nombre, [])

    def __getitem__(self, id):
        return self.piezas[id]

    def pieza(self, id, tam, en=(0, 0, 0)):
        """A piece of its own (a thing the game has no shapes for: a tool): a box that size."""
        p = Pieza(id, {'kind': 'box', 'size': list(tam)}, en)
        self.piezas[id] = p
        return p

    # ---- geometry: every builder returns the object it made ---------------------------------

    def _objeto(self, bm, mat, pieza, marco, nombre, liso=35.0, recalc=True):
        """`bm` (in `marco`'s axes) as an object of `pieza`, in Blender's world. `recalc`: its
        faces are made to look out (a closed thing); an open sheet (letters, a mark painted on
        a face) keeps the side it was built with."""
        marco = marco or MUNDO
        for v in bm.verts:
            v.co = B(marco.a_mundo(v.co))
        # (the normals as the faces now lie, then all of them made to look out)
        bm.normal_update()
        if recalc:
            bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
            bm.normal_update()
        # the game's axes are Blender's turned, not mirrored: faces keep their side
        me = bpy.data.meshes.new(nombre)
        bm.to_mesh(me)
        bm.free()
        ob = bpy.data.objects.new(nombre, me)
        bpy.context.scene.collection.objects.link(ob)
        me.materials.append(M(mat) if isinstance(mat, str) else mat)
        ob['pieza'] = pieza.id if isinstance(pieza, Pieza) else pieza
        suavizar(ob, liso)
        self.objetos.append(ob)
        return ob

    def caja(self, tam, en=(0, 0, 0), mat=TINTE, pieza='', marco=None, bisel=0.008, seg=2, rot=None):
        """A box of that size centred at `en`, its edges rounded by `bisel`."""
        bm = bmesh.new()
        bmesh.ops.create_cube(bm, size=1.0)
        R = rot_xyz(rot) if rot else Matrix.Identity(3)
        for v in bm.verts:
            v.co = Vector((v.co.x * tam[0], v.co.y * tam[1], v.co.z * tam[2]))
        biselar(bm, bisel, seg)
        for v in bm.verts:
            v.co = Vector(en) + R @ v.co
        return self._objeto(bm, mat, pieza, marco, 'caja')

    def torno(self, perfil, en=(0, 0, 0), mat=TINTE, pieza='', marco=None, eje='y', lados=32, liso=40.0, arco=360.0):
        """A profile turned round an axis: `perfil` is a list of (radius, height along the
        axis). A profile that starts or ends off the axis is left open there (a tube); radius 0
        closes it. `arco` under 360 turns it only part of the way."""
        bm = bmesh.new()
        R = base_de(eje)
        completo = arco >= 359.9
        n = lados if completo else lados + 1
        anillos = []
        for (r, t) in perfil:
            if r <= 1e-6:
                anillos.append([bm.verts.new(Vector(en) + R @ Vector((0, t, 0)))])
            else:
                anillos.append([bm.verts.new(Vector(en) + R @ Vector((r * math.cos(math.radians(arco) * k / lados), t, r * math.sin(math.radians(arco) * k / lados)))) for k in range(n)])
        for a, b in zip(anillos, anillos[1:]):
            for k in range(lados if not completo else n):
                k2 = (k + 1) % n if completo else k + 1
                if len(a) == 1 and len(b) == 1:
                    continue
                if len(a) == 1:
                    bm.faces.new((a[0], b[k2], b[k]))
                elif len(b) == 1:
                    bm.faces.new((a[k], a[k2], b[0]))
                else:
                    bm.faces.new((a[k], a[k2], b[k2], b[k]))
        return self._objeto(bm, mat, pieza, marco, 'torno', liso)

    def cilindro(self, r, h, en=(0, 0, 0), mat=TINTE, pieza='', marco=None, eje='y', lados=28, bisel=0.004, r2=None):
        """A cylinder (or a cone to `r2`) of height `h` centred at `en`, its rims broken."""
        r2 = r if r2 is None else r2
        b = min(bisel, r * 0.4, r2 * 0.4 if r2 > 0 else bisel, h * 0.4)
        perfil = [(0, -h / 2), (r - b, -h / 2), (r, -h / 2 + b), (r2, h / 2 - b), (max(r2 - b, 0), h / 2), (0, h / 2)]
        return self.torno(perfil, en, mat, pieza, marco, eje, lados)

    def esfera(self, r, en=(0, 0, 0), mat=TINTE, pieza='', marco=None, lados=24, de=-90.0, a=90.0, eje='y', aplastar=1.0):
        """A sphere (or the band of it between two latitudes, degrees)."""
        pasos = max(4, lados // 2)
        perfil = [(r * math.cos(math.radians(de + (a - de) * k / pasos)), r * aplastar * math.sin(math.radians(de + (a - de) * k / pasos))) for k in range(pasos + 1)]
        return self.torno(perfil, en, mat, pieza, marco, eje, lados, liso=80.0)

    def capsula(self, r, largo, en=(0, 0, 0), mat=TINTE, pieza='', marco=None, eje='y', lados=28, fondo=0.6):
        """A tank: a cylinder of that whole length with domed ends (`fondo`: how deep the domes
        are, in radii; 1 is a half sphere)."""
        d = r * fondo
        recto = max(largo - 2 * d, 0.0)
        pasos = 6
        perfil = []
        for k in range(pasos + 1):
            ang = math.radians(90 * k / pasos)
            perfil.append((r * math.sin(ang), -recto / 2 - d * math.cos(ang)))
        for k in range(pasos + 1):
            ang = math.radians(90 * k / pasos)
            perfil.append((r * math.cos(ang), recto / 2 + d * math.sin(ang)))
        return self.torno(perfil, en, mat, pieza, marco, eje, lados, liso=80.0)

    def tubo(self, puntos, r, mat='acero', pieza='', marco=None, lados=10, codo=0.0, tapas=True):
        """A tube through those points, its corners rounded with radius `codo`."""
        pts = [Vector(p) for p in puntos]
        if codo > 0 and len(pts) > 2:
            suaves = [pts[0]]
            for a, b, c in zip(pts, pts[1:], pts[2:]):
                d1, d2 = (a - b), (c - b)
                k = min(codo, d1.length * 0.45, d2.length * 0.45)
                p1, p2 = b + d1.normalized() * k, b + d2.normalized() * k
                for t in (0.0, 0.25, 0.5, 0.75, 1.0):
                    # a quadratic bend through the corner
                    suaves.append((1 - t) ** 2 * p1 + 2 * (1 - t) * t * b + t ** 2 * p2)
            suaves.append(pts[-1])
            pts = suaves
        bm = bmesh.new()
        anillos = []
        for i, p in enumerate(pts):
            if i == 0:
                t = pts[1] - pts[0]
            elif i == len(pts) - 1:
                t = pts[-1] - pts[-2]
            else:
                t = (pts[i + 1] - p).normalized() + (p - pts[i - 1]).normalized()
            t = t.normalized()
            if i == 0:
                u = t.orthogonal().normalized()
            else:
                # carried along: no twist from one ring to the next
                u = (u - t * u.dot(t)).normalized()
            w = t.cross(u)
            anillos.append([bm.verts.new(p + (u * math.cos(math.tau * k / lados) + w * math.sin(math.tau * k / lados)) * r) for k in range(lados)])
        for a, b in zip(anillos, anillos[1:]):
            for k in range(lados):
                bm.faces.new((a[k], a[(k + 1) % lados], b[(k + 1) % lados], b[k]))
        if tapas:
            bm.faces.new(anillos[0])
            bm.faces.new(list(reversed(anillos[-1])))
        return self._objeto(bm, mat, pieza, marco, 'tubo', 50.0)

    def extrusion(self, perfil, largo, en=(0, 0, 0), mat=TINTE, pieza='', marco=None, eje='z', bisel=0.0, seg=1):
        """A flat outline pushed `largo` along `eje`, centred at `en`. Its points (a, b) are
        (x, y) for an outline pushed along z, (z, y) along x, (x, z) along y."""
        u, v, w = {'z': (EJES['x'], EJES['y'], EJES['z']), 'x': (EJES['z'], EJES['y'], EJES['x']), 'y': (EJES['x'], EJES['z'], EJES['y'])}[eje]
        bm = bmesh.new()
        abajo = [bm.verts.new(u * a + v * b - w * (largo / 2)) for a, b in perfil]
        arriba = [bm.verts.new(u * a + v * b + w * (largo / 2)) for a, b in perfil]
        n = len(perfil)
        bm.faces.new(abajo)
        bm.faces.new(list(reversed(arriba)))
        for k in range(n):
            bm.faces.new((abajo[k], abajo[(k + 1) % n], arriba[(k + 1) % n], arriba[k]))
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
        biselar(bm, bisel, seg)
        for x in bm.verts:
            x.co = Vector(en) + x.co
        return self._objeto(bm, mat, pieza, marco, 'extrusion')

    def piel(self, anillos, mat=TINTE, pieza='', marco=None, tapas=True, cerrado=True, liso=40.0):
        """A skin over rings of points (all of as many): faces from each ring to the next, round
        them if `cerrado`, the first and last ring capped if `tapas`."""
        bm = bmesh.new()
        vs = [[bm.verts.new(Vector(p)) for p in a] for a in anillos]
        n = len(vs[0])
        for a, b in zip(vs, vs[1:]):
            for k in range(n if cerrado else n - 1):
                bm.faces.new((a[k], a[(k + 1) % n], b[(k + 1) % n], b[k]))
        if tapas:
            bm.faces.new(vs[0])
            bm.faces.new(list(reversed(vs[-1])))
        return self._objeto(bm, mat, pieza, marco, 'piel', liso)

    def taladrar(self, ob, agujeros, r, largo, eje='x', marco=None, lados=16, forma='redondo', alto=None):
        """Holes through `ob`: a cutter at each point of `agujeros` (in `marco`), along `eje`,
        `largo` long; round of radius `r`, or 'ranura' — a slot `r` in radius and `alto` tall."""
        marco = marco or MUNDO
        R = base_de(eje)
        bm = bmesh.new()
        for p in agujeros:
            if forma == 'ranura':
                h = (alto or r * 3) / 2 - r
                pts = [(r * math.cos(a), r * math.sin(a) + (h if math.sin(a) >= 0 else -h)) for a in [math.tau * (k + 0.5) / lados for k in range(lados)]]
            else:
                pts = [(r * math.cos(math.tau * k / lados), r * math.sin(math.tau * k / lados)) for k in range(lados)]
            # (the outline lies across the cutter's axis: its second coordinate along `marco`'s up
            # when the axis is level)
            abajo = [bm.verts.new(B(marco.a_mundo(Vector(p) + R @ Vector((a, -largo / 2, b))))) for a, b in pts]
            arriba = [bm.verts.new(B(marco.a_mundo(Vector(p) + R @ Vector((a, largo / 2, b))))) for a, b in pts]
            bm.faces.new(abajo)
            bm.faces.new(list(reversed(arriba)))
            for k in range(lados):
                bm.faces.new((abajo[k], abajo[(k + 1) % lados], arriba[(k + 1) % lados], arriba[k]))
        bm.normal_update()
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
        me = bpy.data.meshes.new('cortador')
        bm.to_mesh(me)
        bm.free()
        cortador = bpy.data.objects.new('cortador', me)
        bpy.context.scene.collection.objects.link(cortador)
        # (the walls of a hole are of what it is cut in)
        for mat in ob.data.materials:
            me.materials.append(mat)
        return self._restar(ob, cortador)

    def restar(self, ob, otro):
        """Takes `otro` (a thing just built) out of `ob`: what is left of `ob` where `otro` is
        not. `otro` is gone from the model."""
        self.objetos.remove(otro)
        return self._restar(ob, otro)

    def _restar(self, ob, cortador):
        cortador.hide_render = True
        cortador.display_type = 'WIRE'
        self.cortadores.append(cortador)
        mod = ob.modifiers.new('resta', 'BOOLEAN')
        mod.operation = 'DIFFERENCE'
        mod.object = cortador
        mod.solver = 'EXACT'
        # first what is cut (in the order asked), then the rims made sharp, then the normals
        cortes = sum(1 for x in ob.modifiers if x.type == 'BOOLEAN') - 1
        ob.modifiers.move(ob.modifiers.find(mod.name), cortes)
        if not any(x.type == 'EDGE_SPLIT' for x in ob.modifiers):
            filo = ob.modifiers.new('filos', 'EDGE_SPLIT')
            filo.use_edge_angle = True
            filo.use_edge_sharp = True
            filo.split_angle = math.radians(40.0)
        ob.modifiers.move(ob.modifiers.find('filos'), cortes + 1)
        return ob

    def tornillos(self, puntos, r=0.006, alto=0.004, mat='acero', pieza='', marco=None, eje='y', lados=6):
        """A bolt head at each point, standing along `eje`."""
        bm = bmesh.new()
        R = base_de(eje)
        for p in puntos:
            abajo = [bm.verts.new(Vector(p) + R @ Vector((r * math.cos(math.tau * k / lados), 0, r * math.sin(math.tau * k / lados)))) for k in range(lados)]
            arriba = [bm.verts.new(Vector(p) + R @ Vector((r * 0.86 * math.cos(math.tau * k / lados), alto, r * 0.86 * math.sin(math.tau * k / lados)))) for k in range(lados)]
            bm.faces.new(list(reversed(arriba)))
            for k in range(lados):
                bm.faces.new((abajo[k], abajo[(k + 1) % lados], arriba[(k + 1) % lados], arriba[k]))
        return self._objeto(bm, mat, pieza, marco, 'tornillos', 20.0)

    def rejilla(self, tam, en=(0, 0, 0), mat='acero_oscuro', pieza='', marco=None, lamas=8, grosor=0.004, inclinar=35.0, normal='z'):
        """Slats across a `tam` (width, height) opening that faces `normal`, each tilted."""
        w, h = tam
        R = base_de(normal)
        bm = bmesh.new()
        paso = h / lamas
        fondo = paso * 0.9
        for k in range(lamas):
            y = -h / 2 + paso * (k + 0.5)
            c, s = math.cos(math.radians(inclinar)), math.sin(math.radians(inclinar))
            # a slat: a thin board tilted about the opening's width
            esquinas = []
            for (dx, dz, dy) in ((-1, -1, -1), (1, -1, -1), (1, 1, -1), (-1, 1, -1), (-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)):
                a, b = dz * fondo / 2, dy * grosor / 2
                esquinas.append(bm.verts.new(Vector(en) + R @ Vector((dx * w / 2, a * s + b * c, y + a * c - b * s))))
            for f in ((0, 1, 2, 3), (7, 6, 5, 4), (0, 4, 5, 1), (1, 5, 6, 2), (2, 6, 7, 3), (3, 7, 4, 0)):
                bm.faces.new([esquinas[i] for i in f])
        return self._objeto(bm, mat, pieza, marco, 'rejilla', 20.0)

    # ---- things made of the above -----------------------------------------------------------

    def pernos(self, pieza, cara='y', margen=0.03, n=(2, 2), r=0.006, mat='acero', lado=1):
        """Bolt heads on a face of a box piece: `cara` the axis it looks along, `lado` +1 or -1,
        `n` how many each way, `margen` in from its edges."""
        s = pieza.semi
        i = 'xyz'.index(cara)
        a, b = [k for k in range(3) if k != i]
        pts = []
        for u in range(n[0]):
            for v in range(n[1]):
                p = [0.0, 0.0, 0.0]
                p[i] = lado * s[i]
                p[a] = (-s[a] + margen) + (2 * s[a] - 2 * margen) * (u / max(n[0] - 1, 1) if n[0] > 1 else 0.5)
                p[b] = (-s[b] + margen) + (2 * s[b] - 2 * margen) * (v / max(n[1] - 1, 1) if n[1] > 1 else 0.5)
                pts.append(p)
        eje = Vector([lado if k == i else 0 for k in range(3)])
        return self.tornillos(pts, r, r * 0.6, mat, pieza, pieza, eje=eje)

    def asa(self, a, b, hacia, alto=0.04, r=0.007, mat='acero', pieza='', marco=None):
        """A handle from `a` to `b`, standing `alto` off them toward `hacia`."""
        h = Vector(hacia).normalized() * alto
        return self.tubo([Vector(a), Vector(a) + h, Vector(b) + h, Vector(b)], r, mat, pieza, marco, lados=10, codo=min(alto * 0.6, 0.02))

    def manguera(self, a, b, r, comba=0.1, hacia=(0, -1, 0), mat='goma', pieza='', marco=None):
        """A hose from `a` to `b` hanging `comba` toward `hacia` in its middle."""
        a, b = Vector(a), Vector(b)
        pts = []
        for k in range(9):
            t = k / 8
            pts.append(a.lerp(b, t) + Vector(hacia).normalized() * comba * 4 * t * (1 - t))
        return self.tubo(pts, r, mat, pieza, marco, lados=8)

    def brida(self, r, en=(0, 0, 0), grosor=0.012, pernos=8, mat='acero', pieza='', marco=None, eje='y', r_int=None):
        """A flange: a ring of that radius round `eje` with its bolts."""
        ri = r * 0.62 if r_int is None else r_int
        b = min(0.003, grosor * 0.3)
        self.torno([(ri, -grosor / 2), (r - b, -grosor / 2), (r, -grosor / 2 + b), (r, grosor / 2 - b), (r - b, grosor / 2), (ri, grosor / 2)], en, mat, pieza, marco, eje, lados=max(16, pernos * 3))
        R = base_de(eje)
        rr = (r + ri) * 0.5 if r_int is not None else r * 0.84
        pts = [Vector(en) + R @ Vector((rr * math.cos(math.tau * k / pernos), grosor / 2, rr * math.sin(math.tau * k / pernos))) for k in range(pernos)]
        return self.tornillos(pts, min(0.008, r * 0.09), 0.005, 'acero_oscuro', pieza, marco, eje=R @ Vector((0, 1, 0)))

    def aletas(self, tam, en=(0, 0, 0), n=8, grosor=0.004, mat='aluminio', pieza='', marco=None, a_lo_largo='z', apila='x'):
        """Cooling fins: `n` thin plates filling the box `tam` at `en`, stacked along `apila`."""
        i = 'xyz'.index(apila)
        out = []
        for k in range(n):
            t = [tam[0], tam[1], tam[2]]
            c = [en[0], en[1], en[2]]
            c[i] = en[i] - tam[i] / 2 + tam[i] * (k + 0.5) / n
            t[i] = grosor
            out.append(self.caja(t, c, mat, pieza, marco, bisel=min(grosor * 0.3, 0.0012), seg=1))
        del a_lo_largo
        return out

    def piloto(self, en, r=0.006, mat='piloto_verde', pieza='', marco=None, eje='y'):
        """A small lamp: a bezel and its lens."""
        self.cilindro(r * 1.5, r * 0.8, en, 'acero_oscuro', pieza, marco, eje, lados=12, bisel=r * 0.2)
        R = base_de(eje)
        return self.esfera(r, Vector(en) + R @ Vector((0, r * 0.3, 0)), mat, pieza, marco, lados=10, de=0, eje=eje)

    # ---- what is painted or written on a face (open sheets a hair over it) ----------------------

    def lamina(self, contornos, en=(0, 0, 0), mat='pintura_oscura', pieza='', marco=None, normal=(0, 0, 1), arriba=(0, 1, 0), radio=None, sobre=0.0003):
        """Flat shapes painted on a face: each of `contornos` an outline of (a, b) points — `a`
        to the right of whoever looks at the face, `b` up it — round `en`, `sobre` over the
        face that looks `normal`. `radio`: the face is a cylinder of that radius about `arriba`
        (a drum's wall): the shapes go round it."""
        n = Vector(normal).normalized()
        u = Vector(arriba)
        u = (u - n * u.dot(n)).normalized()
        r = u.cross(n)
        en = Vector(en)
        bm = bmesh.new()
        for c in contornos:
            vs = []
            for a, b in c:
                if radio:
                    ang = a / radio
                    vs.append(bm.verts.new(en - n * radio + (n * math.cos(ang) + r * math.sin(ang)) * (radio + sobre) + u * b))
                else:
                    vs.append(bm.verts.new(en + r * a + u * b + n * sobre))
            if len(vs) >= 3:
                # (counter-clockwise as seen from where it is looked at: it faces that way)
                area = sum(c[i][0] * c[(i + 1) % len(c)][1] - c[(i + 1) % len(c)][0] * c[i][1] for i in range(len(c)))
                bm.faces.new(vs if area > 0 else vs[::-1])
        return self._objeto(bm, mat, pieza, marco, 'lamina', 20.0, recalc=False)

    def texto(self, texto, en=(0, 0, 0), alto=0.01, mat='pintura_oscura', pieza='', marco=None, normal=(0, 0, 1), arriba=(0, 1, 0), alinear='centro', radio=None, sobre=0.0003, res=2, ancho=None):
        """Letters painted on a face (real ones: Blender's own font turned to triangles, some
        dozen a letter): capitals `alto` tall, the line's middle at `en` (`alinear` 'izq' or
        'der': its end there), read by whoever looks at the face that looks `normal` with
        `arriba` up. `radio`: round a cylinder of that radius about `arriba`. `ancho`: squeezed
        to be no wider than that."""
        bm = letras(texto, alto, res)
        if not bm.faces:
            bm.free()
            return None
        bm.normal_update()
        if sum(f.normal.z * f.calc_area() for f in bm.faces) < 0:
            bmesh.ops.reverse_faces(bm, faces=bm.faces[:])
        x0, x1 = min(v.co.x for v in bm.verts), max(v.co.x for v in bm.verts)
        k = min(1.0, ancho / (x1 - x0)) if ancho and x1 > x0 else 1.0
        ancla = {'centro': (x0 + x1) / 2, 'izq': x0, 'der': x1}[alinear]
        n = Vector(normal).normalized()
        u = Vector(arriba)
        u = (u - n * u.dot(n)).normalized()
        r = u.cross(n)
        en = Vector(en)
        if radio:
            # (a letter's flat faces are chords of the round: lifted by what they would sink)
            largo = max((e.calc_length() for e in bm.edges), default=0.0) * k
            sobre += largo * largo / (8 * radio)
        for v in bm.verts:
            a, b = (v.co.x - ancla) * k, v.co.y - alto / 2
            if radio:
                ang = a / radio
                v.co = en - n * radio + (n * math.cos(ang) + r * math.sin(ang)) * (radio + sobre) + u * b
            else:
                v.co = en + r * a + u * b + n * sobre
        return self._objeto(bm, mat, pieza, marco, 'texto', 20.0, recalc=False)

    def rayas(self, tam, en=(0, 0, 0), pieza='', marco=None, normal=(0, 0, 1), arriba=(0, 1, 0), n=6, mats=('pintura_amarilla', 'plastico_negro'), inclina=45.0, radio=None, sobre=0.0003):
        """A hazard band `tam` (wide, tall) painted on a face: a ground of the first colour,
        `n` slanted stripes of the second over it."""
        w, h = tam
        self.lamina([[(-w / 2, -h / 2), (w / 2, -h / 2), (w / 2, h / 2), (-w / 2, h / 2)]] if not radio else [[(-w / 2 + w * k / 8, -h / 2), (-w / 2 + w * (k + 1) / 8, -h / 2), (-w / 2 + w * (k + 1) / 8, h / 2), (-w / 2 + w * k / 8, h / 2)] for k in range(8)], en, mats[0], pieza, marco, normal, arriba, radio, sobre)
        paso = w / n
        t = h / 2 * math.tan(math.radians(inclina))
        formas = []
        k = -int(t / paso) - 2
        while k * paso - t < w:
            a0 = -w / 2 + k * paso
            forma = [(a0 - t, -h / 2), (a0 + paso / 2 - t, -h / 2), (a0 + paso / 2 + t, h / 2), (a0 + t, h / 2)]
            forma = recortar(forma, -w / 2, w / 2)
            if len(forma) >= 3:
                formas.append(forma)
            k += 1
        return self.lamina(formas, en, mats[1], pieza, marco, normal, arriba, radio, sobre * 2)

    def aviso(self, en=(0, 0, 0), lado=0.02, pieza='', marco=None, normal=(0, 0, 1), arriba=(0, 1, 0), mats=('pintura_amarilla', 'plastico_negro'), radio=None, sobre=0.0003):
        """A warning sign painted on a face: a yellow triangle `lado` across with its black
        edge and its exclamation mark."""
        a = lado / 2
        alto = lado * 0.866
        tri = [(-a, -alto / 2), (a, -alto / 2), (0, alto / 2)]
        self.lamina([tri], en, mats[1], pieza, marco, normal, arriba, radio, sobre)
        k = 0.74
        self.lamina([[(x * k, (y + alto / 6) * k - alto / 6) for x, y in tri]], en, mats[0], pieza, marco, normal, arriba, radio, sobre * 2)
        g = lado * 0.05
        return self.lamina([[(-g, -alto * 0.12), (g, -alto * 0.12), (g * 1.3, alto * 0.2), (-g * 1.3, alto * 0.2)], [(-g, -alto * 0.3), (g, -alto * 0.3), (g, -alto * 0.2), (-g, -alto * 0.2)]], en, mats[1], pieza, marco, normal, arriba, radio, sobre * 3)

    def rotulo(self, texto, en=(0, 0, 0), alto=0.008, pieza='', marco=None, normal=(0, 0, 1), arriba=(0, 1, 0), fondo='pintura_amarilla', tinta='plastico_negro', margen=None, radio=None, sobre=0.0003, ancho=None, borde=None):
        """A label painted on a face: a plate of `fondo` just big enough for the letters (or
        `ancho` wide), the letters of `tinta` on it; `borde`: a rim of that colour round it."""
        margen = alto * 0.45 if margen is None else margen
        libre = (ancho - 2 * margen) if ancho else None
        if ancho is None:
            ancho = self._ancho_texto(texto, alto) + 2 * margen
        self.texto(texto, en, alto, tinta, pieza, marco, normal, arriba, 'centro', radio, sobre * 3, ancho=libre)

        def placa(w, h):
            partes = 1 if not radio else max(2, int(w / 0.02))
            return [[(-w / 2 + w * k / partes, -h / 2), (-w / 2 + w * (k + 1) / partes, -h / 2), (-w / 2 + w * (k + 1) / partes, h / 2), (-w / 2 + w * k / partes, h / 2)] for k in range(partes)]

        w, h = ancho, alto + 2 * margen
        if borde:
            b = alto * 0.16
            self.lamina(placa(w + 2 * b, h + 2 * b), en, borde, pieza, marco, normal, arriba, radio, sobre)
            return self.lamina(placa(w, h), en, fondo, pieza, marco, normal, arriba, radio, sobre * 2)
        return self.lamina(placa(w, h), en, fondo, pieza, marco, normal, arriba, radio, sobre)

    @staticmethod
    def _ancho_texto(texto, alto):
        bm = letras(texto, alto, 1)
        xs = [v.co.x for v in bm.verts] or [0.0]
        bm.free()
        return max(xs) - min(xs)

    def desconchones(self, puntos, normal=(0, 1, 0), tam=0.006, mat='acero', pieza='', marco=None, semilla=1, sobre=0.0002, alarga=None, estira=2.2):
        """Chipped paint: at each point a small ragged patch of bare `mat` lying on the face
        that looks `normal` (one for all, or one a point), `tam` across — each a little
        different —, stretched `estira` times along `alarga` (the edge it is worn along)."""
        import random

        azar = random.Random(semilla)
        bm = bmesh.new()
        varias = len(normal) == len(puntos) and hasattr(normal[0], '__len__')
        for i, p in enumerate(puntos):
            n = Vector(normal[i] if varias else normal).normalized()
            a = Vector(alarga).normalized() if alarga is not None else n.orthogonal().normalized()
            a = (a - n * a.dot(n)).normalized()
            b = n.cross(a)
            lados = azar.randint(5, 7)
            r = tam / 2 * azar.uniform(0.6, 1.3)
            fase = azar.uniform(0, math.tau)
            vs = []
            for k in range(lados):
                ang = fase + math.tau * k / lados
                rr = r * azar.uniform(0.55, 1.0)
                vs.append(bm.verts.new(Vector(p) + a * (math.cos(ang) * rr * (estira if alarga is not None else 1.0)) + b * (math.sin(ang) * rr) + n * sobre))
            # (counter-clockwise about n: it faces out)
            cara = bm.faces.new(vs)
            cara.normal_update()
            if cara.normal.dot(n) < 0:
                bmesh.ops.reverse_faces(bm, faces=[cara])
        return self._objeto(bm, mat, pieza, marco, 'desconchones', 20.0, recalc=False)

    def espiral(self, a, b, r, r_tubo, vueltas=10, mat='goma', pieza='', marco=None, lados=6, pasos=10, comba=0.0, hacia=(0, -1, 0), cabos=0.012):
        """A coiled lead from `a` to `b`: a tube wound `vueltas` times at radius `r` round the
        line between them (which hangs `comba` toward `hacia`), a straight tail `cabos` long
        at each end."""
        a, b = Vector(a), Vector(b)
        eje = (b - a).normalized()
        u = eje.orthogonal().normalized()
        w = eje.cross(u)
        h = Vector(hacia).normalized()
        largo = (b - a).length
        pts = [a]
        total = vueltas * pasos
        for k in range(total + 1):
            t = k / total
            s = cabos + (largo - 2 * cabos) * t
            centro = a + eje * s + h * (comba * 4 * (s / largo) * (1 - s / largo))
            # (it leaves its ends on the line: the coil swells over its first and last turn)
            rr = r * min(1.0, k / pasos, (total - k) / pasos) if total > 2 * pasos else r * math.sin(math.pi * t)
            ang = math.tau * k / pasos
            pts.append(centro + (u * math.cos(ang) + w * math.sin(ang)) * rr)
        pts.append(b)
        return self.tubo(pts, r_tubo, mat, pieza, marco, lados=lados)

    # ---- what a recipe says of its model -------------------------------------------------------

    def mueve(self, pieza, eje=(1, 0, 0), giro=None, recorre=None, padre='', desde=0.0, nota=None):
        """Says how a piece moves about its own origin (its pivot): it turns from `desde` to
        `giro` degrees about `eje`, or slides `recorre` metres along it. `padre`: the piece it
        rides on (it moves with it). A list of (eje, desde, giro): it turns about several."""
        d = self.datos['piezas'].setdefault(pieza, {})
        if isinstance(eje, list):
            d['ejes'] = [{'eje': [round(x, 5) for x in Vector(e).normalized()], 'desde': de, 'giro': g} for e, de, g in eje]
        else:
            d['eje'] = [round(x, 5) for x in Vector(eje).normalized()]
            if giro is not None:
                d['giro'] = giro
                d['desde'] = desde
            if recorre is not None:
                d['recorre'] = recorre
        if padre:
            d['padre'] = padre
        if nota:
            d['nota'] = nota

    def punto(self, nombre, en, palma=None, traves=None, pieza=None, nota=None, mano=None, mango=None):
        """Says a place of the model by name. For a hand: the middle of what it closes on, the
        way the palm faces there (`palma`) and the way from its index finger to its little one
        (`traves`); `pieza`: it goes with that piece; `mano`: 'izq' or 'der'; `mango`: how
        thick what it closes on is there (m), for `comprobar.py` to hold it to."""
        d = {'en': [round(x, 5) for x in en]}
        if palma is not None:
            d['palma'] = [round(x, 4) for x in Vector(palma).normalized()]
        if traves is not None:
            d['traves'] = [round(x, 4) for x in Vector(traves).normalized()]
        if pieza:
            d['pieza'] = pieza
        if mano:
            d['mano'] = mano
        if mango:
            d['mango'] = mango
        if nota:
            d['nota'] = nota
        self.datos['puntos'][nombre] = d

    def pantalla(self, nombre, centro, tam, normal, arriba, pieza='', nota=None):
        """Says where a flat screen the game draws on is: the middle of its glass, its size
        (wide, tall), the way it looks and its up."""
        n = Vector(normal).normalized()
        u = Vector(arriba)
        u = (u - n * u.dot(n)).normalized()
        d = {'centro': [round(x, 5) for x in centro], 'tam': [round(x, 5) for x in tam], 'normal': [round(x, 4) for x in n], 'arriba': [round(x, 4) for x in u], 'derecha': [round(x, 4) for x in u.cross(n)]}
        if pieza:
            d['pieza'] = pieza
        if nota:
            d['nota'] = nota
        self.datos['pantallas'][nombre] = d

    def luz(self, nombre, en, radio, pieza='', material=None, nota=None):
        """Says where a lamp the game lights is: its middle, its radius, the piece that is its
        lens and the material of that lens."""
        d = {'en': [round(x, 5) for x in en], 'radio': radio}
        if pieza:
            d['pieza'] = pieza
        if material:
            d['material'] = material
        if nota:
            d['nota'] = nota
        self.datos['luces'][nombre] = d

    # ---- finishing --------------------------------------------------------------------------

    def _escribir_datos(self, info):
        """What the recipe said of its model, with what was built (each piece's triangles and
        its box in the model's frame), as `tools/modelos/datos/<name>.json`: nothing, if the
        recipe said nothing."""
        d = self.datos
        if not (d['piezas'] or d['puntos'] or d['pantallas'] or d['luces'] or self.sosten):
            return
        lo_t, hi_t = Vector((1e9,) * 3), Vector((-1e9,) * 3)
        piezas = {}
        for id, (tris, lo, hi) in info.items():
            p = self.piezas[id]
            esquinas = [p.a_mundo(Vector((x, y, z))) for x in (lo.x, hi.x) for y in (lo.y, hi.y) for z in (lo.z, hi.z)]
            a = Vector((min(e.x for e in esquinas), min(e.y for e in esquinas), min(e.z for e in esquinas)))
            b = Vector((max(e.x for e in esquinas), max(e.y for e in esquinas), max(e.z for e in esquinas)))
            lo_t = Vector((min(lo_t.x, a.x), min(lo_t.y, a.y), min(lo_t.z, a.z)))
            hi_t = Vector((max(hi_t.x, b.x), max(hi_t.y, b.y), max(hi_t.z, b.z)))
            piezas[p.nombre] = {'en': [round(x, 5) for x in p.c], 'triangulos': tris, 'caja': [[round(x, 4) for x in a], [round(x, 4) for x in b]], **d['piezas'].get(id, {})}
        hoja = {
            'modelo': self.nombre,
            'marco': self.marco_dicho or 'x a la izquierda de quien lo lleva, y arriba, z adelante; metros; origen: ver la receta',
            'triangulos': sum(t for t, _, _ in info.values()),
            'presupuesto': self.presupuesto,
            'caja': [[round(x, 4) for x in lo_t], [round(x, 4) for x in hi_t]],
            'tam': [round(x, 4) for x in hi_t - lo_t],
            'piezas': piezas,
            'puntos': d['puntos'],
            'pantallas': d['pantallas'],
            'luces': d['luces'],
        }
        if self.sosten:
            hoja['sosten'] = self.sosten
        # (whatever else the recipe said, but what is only for looking at it or checking it)
        for k, v in d.items():
            if k not in hoja and k not in ('vistas', 'cambia'):
                hoja[k] = v
        carpeta = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'datos')
        os.makedirs(carpeta, exist_ok=True)
        with open(os.path.join(carpeta, self.nombre + '.json'), 'w', encoding='utf-8', newline='\n') as f:
            json.dump(hoja, f, indent=1, ensure_ascii=False)
            f.write('\n')

    def terminar(self, vista=True, sombra=True):
        """Joins what each piece has, bakes its shade, writes the `.glb` (and a picture)."""
        piezas = {}
        for ob in self.objetos:
            piezas.setdefault(ob['pieza'], []).append(ob)
        for id in piezas:
            if id not in self.piezas:
                raise KeyError(f"{self.nombre}: no hay pieza '{id}' (hay: {', '.join(repr(p) for p in self.piezas)})")
        unidos = {id: unir(obs, self.piezas[id].nombre) for id, obs in piezas.items()}
        for c in self.cortadores:
            bpy.data.objects.remove(c)
        self.cortadores = []
        if sombra:
            hornear(list(unidos.values()), min(max(0.12, max(p.tam.length for p in self.piezas.values()) * 0.35), 0.9), aparte=[unidos[id] for id in self.sombra_aparte if id in unidos])
        os.makedirs(SALIDA, exist_ok=True)
        ruta = os.path.join(SALIDA, self.nombre + '.glb')
        info = escribir_glb(ruta, [(self.piezas[id], ob) for id, ob in unidos.items()])
        self._escribir_datos(info)
        if vista:
            # (in the picture each piece's own surface wears the colour its data gives it)
            for id, ob in unidos.items():
                color = self.piezas[id].datos.get('color')
                for k, mat in enumerate(ob.data.materials):
                    if mat.name == TINTE:
                        copia = mat.copy()
                        c = color or (150, 156, 164)
                        copia.diffuse_color = [lineal(x) for x in c] + [1.0]
                        ob.data.materials[k] = copia
            foto(os.path.join(VISTAS, self.nombre + '.png'), [ob for id, ob in unidos.items() if self.vista_de is None or id == self.vista_de])
        for id, (tris, lo, hi) in info.items():
            p = self.piezas[id]
            # what it takes up against what the game has it as: a model may round its shape off
            # and stand a little proud of it, never be another size
            fuera = max(max(-s - l, h - s) for s, l, h in zip(p.semi, lo, hi))
            # (a hull of points away from its origin fills what it reaches along one axis at least,
            # as the game measures it: `modelos.rs`)
            llenos = [(h - l) / max(2 * s, 1e-6) for s, l, h in zip(p.semi, lo, hi)]
            dentro = max(llenos) if p.forma.get('kind') == 'hull' else min(llenos)
            aviso = '  <-- se sale' if fuera > max(0.05, 0.12 * max(p.tam)) else ('  <-- se queda corta' if dentro < 0.5 else '')
            print(f'  {self.nombre}/{p.nombre}: {tris} triángulos, sale {fuera * 100:+.1f} cm, llena {dentro * 100:.0f} %{aviso}')
        print(f'{self.nombre}: {sum(t for t, _, _ in info.values())} triángulos -> {os.path.relpath(ruta, RAIZ)} ({os.path.getsize(ruta) // 1024} KB)')


# ---------------------------------------------------------------------------------------------
# tools of the trade

# the height of the capitals of Blender's own font, per unit of its size
FUENTE_ALTO = 0.7


def letras(texto, alto, res=2):
    """The letters of `texto` as flat faces (a bmesh): Blender's own font, its capitals `alto`
    tall, the line starting at x = 0 on y = 0. (Made off the scene: nothing of the model is
    worked out again for it.)"""
    cu = bpy.data.curves.new('texto', 'FONT')
    cu.body = texto
    cu.resolution_u = res
    cu.size = alto / FUENTE_ALTO
    cu.fill_mode = 'FRONT'
    ob = bpy.data.objects.new('texto', cu)
    me = bpy.data.meshes.new_from_object(ob)
    bpy.data.objects.remove(ob)
    bpy.data.curves.remove(cu)
    bm = bmesh.new()
    bm.from_mesh(me)
    bpy.data.meshes.remove(me)
    return bm


def recortar(forma, lo, hi):
    """A flat outline of (a, b) points cut to what of it lies between `a` = `lo` and `hi`."""
    for lim, signo in ((lo, 1.0), (hi, -1.0)):
        out = []
        for i, p in enumerate(forma):
            q = forma[(i + 1) % len(forma)]
            dp, dq = (p[0] - lim) * signo, (q[0] - lim) * signo
            if dp >= 0:
                out.append(p)
            if (dp > 0) != (dq > 0) and abs(dp - dq) > 1e-12:
                t = dp / (dp - dq)
                out.append((p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t))
        forma = out
        if len(forma) < 3:
            return []
    return forma



def biselar(bm, bisel, seg=2):
    if bisel > 0:
        bmesh.ops.bevel(bm, geom=bm.edges[:], offset=bisel, segments=seg, profile=0.5, affect='EDGES', clamp_overlap=True)


def suavizar(ob, angulo=35.0):
    """Smooth across what bends less than `angulo` degrees, sharp across the rest; big faces
    keep their own normal (weighted), so a flat with rounded edges is still flat."""
    me = ob.data
    bm = bmesh.new()
    bm.from_mesh(me)
    lim = math.radians(angulo)
    for e in bm.edges:
        e.smooth = len(e.link_faces) == 2 and e.calc_face_angle(0.0) < lim
    for f in bm.faces:
        f.smooth = True
    bm.to_mesh(me)
    bm.free()
    if not any(m.type == 'WEIGHTED_NORMAL' for m in ob.modifiers):
        mod = ob.modifiers.new('normales', 'WEIGHTED_NORMAL')
        mod.keep_sharp = True
        mod.weight = 60
        mod.mode = 'FACE_AREA'


def hundir(ob, hacia, margen, fondo, mat=None, marco=None):
    """Sinks a panel into the faces of `ob` that look `hacia` (a direction of the game, or of
    `marco`): a margin left round it, the rest pushed in `fondo` (out, if negative). The sunk
    faces take `mat` if given."""
    marco = marco or MUNDO
    d = B(marco.R @ Vector(hacia)).normalized()
    me = ob.data
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.normal_update()
    caras = [f for f in bm.faces if f.normal.dot(d) > 0.95]
    if caras:
        # (only the big ones: not the slivers of a bevel)
        mayor = max(f.calc_area() for f in caras)
        caras = [f for f in caras if f.calc_area() > mayor * 0.3]
        r = bmesh.ops.inset_region(bm, faces=caras, thickness=margen, depth=-fondo, use_even_offset=True)
        if mat is not None:
            m = M(mat) if isinstance(mat, str) else mat
            if m.name not in [x.name for x in me.materials]:
                me.materials.append(m)
            k = [x.name for x in me.materials].index(m.name)
            for f in caras:
                f.material_index = k
        del r
    bm.to_mesh(me)
    bm.free()
    suavizar(ob)
    return ob


def unir(objetos, nombre):
    """The objects as one, their modifiers applied (each keeps its material)."""
    dg = bpy.context.evaluated_depsgraph_get()
    dg.update()
    bm = bmesh.new()
    mats = []
    custom = []
    for ob in objetos:
        ev = ob.evaluated_get(dg)
        me = bpy.data.meshes.new_from_object(ev, preserve_all_data_layers=True, depsgraph=dg)
        normales = [tuple(n.vector) for n in me.corner_normals]
        base_caras = len(bm.faces)
        indices = []
        for m in me.materials:
            m = m or me.materials[0]
            if m.name not in mats:
                mats.append(m.name)
            indices.append(mats.index(m.name))
        tmp = bmesh.new()
        tmp.from_mesh(me)
        nuevos = [bm.verts.new(ob.matrix_world @ v.co) for v in tmp.verts]
        for f in tmp.faces:
            # (a face with no area — what a bevel of nothing leaves — is not drawn: not kept)
            if f.calc_area() < 1e-11:
                continue
            try:
                g = bm.faces.new([nuevos[v.index] for v in f.verts])
            except ValueError:
                continue
            g.material_index = indices[f.material_index] if indices else 0
            g.smooth = True
            custom.append([normales[l.index] for l in f.loops])
        tmp.free()
        bpy.data.meshes.remove(me)
        del base_caras
    me = bpy.data.meshes.new(nombre)
    bm.to_mesh(me)
    bm.free()
    for m in mats:
        me.materials.append(bpy.data.materials[m])
    # the normals each object had (its sharp edges, its weighted flats), corner by corner
    me.normals_split_custom_set([n for cara in custom for n in cara])
    unido = bpy.data.objects.new(nombre, me)
    bpy.context.scene.collection.objects.link(unido)
    for ob in objetos:
        bpy.data.objects.remove(ob)
    return unido


def hornear(objetos, distancia=0.3, muestras=24, aparte=()):
    """The shade of every corner (how much of the sky it sees) into its vertex colour. Those
    of `aparte` are shaded alone, and are not there while the others are."""
    sc = bpy.context.scene
    if sc.world is None:
        sc.world = bpy.data.worlds.new('mundo')
    sc.world.light_settings.distance = distancia
    sc.render.engine = 'CYCLES'
    sc.cycles.device = 'CPU'
    sc.cycles.samples = muestras
    sc.render.bake.target = 'VERTEX_COLORS'
    for ob in objetos:
        me = ob.data
        attr = me.color_attributes.new('sombra', 'BYTE_COLOR', 'POINT')
        me.color_attributes.active_color = attr
        me.color_attributes.render_color_index = me.color_attributes.active_color_index
    for ob in objetos:
        for o in sc.objects:
            o.select_set(False)
        for o in objetos:
            o.hide_render = o is not ob and (ob in aparte or o in aparte)
        ob.select_set(True)
        bpy.context.view_layer.objects.active = ob
        bpy.ops.object.bake(type='AO', target='VERTEX_COLORS', use_selected_to_active=False)
    for o in objetos:
        o.hide_render = False


def foto(ruta, objetos, tam=(560, 420)):
    """A picture of the thing from three quarters, front-port, lit as a workshop."""
    sc = bpy.context.scene
    lo = Vector((1e9, 1e9, 1e9))
    hi = -lo
    for ob in objetos:
        for v in ob.data.vertices:
            p = ob.matrix_world @ v.co
            lo = Vector((min(lo.x, p.x), min(lo.y, p.y), min(lo.z, p.z)))
            hi = Vector((max(hi.x, p.x), max(hi.y, p.y), max(hi.z, p.z)))
    c, r = (lo + hi) * 0.5, max((hi - lo).length * 0.5, 0.05)
    cam = bpy.data.objects.new('camara', bpy.data.cameras.new('camara'))
    sc.collection.objects.link(cam)
    cam.data.lens = 70
    cam.data.clip_start = 0.01
    # the game's forward is Blender's -y: from in front and to port, a little above
    cam.location = c + Vector((0.62, -0.72, 0.42)).normalized() * r * 4.6
    cam.rotation_euler = (c - cam.location).to_track_quat('-Z', 'Y').to_euler()
    sc.camera = cam
    sc.render.engine = 'BLENDER_WORKBENCH'
    sh = sc.display.shading
    sh.light = 'STUDIO'
    sh.color_type = 'MATERIAL'
    sh.show_cavity = True
    sh.cavity_type = 'BOTH'
    sh.show_shadows = True
    sh.show_specular_highlight = True
    sc.display.render_aa = '8'
    sc.render.resolution_x, sc.render.resolution_y = tam
    sc.render.film_transparent = False
    if sc.world is None:
        sc.world = bpy.data.worlds.new('mundo')
    sc.world.color = (0.035, 0.04, 0.05)
    os.makedirs(os.path.dirname(ruta), exist_ok=True)
    sc.render.filepath = ruta
    bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(cam)


def escribir_glb(ruta, piezas):
    """Writes the pieces — (piece, object) — as a `.glb`: one mesh each, named after it, in its
    own frame and the game's axes, a primitive per material, its shade in COLOR_0."""
    binario = bytearray()
    vistas, accesores, mallas, nodos, materiales = [], [], [], [], []
    mat_indice = {}
    info = {}

    def vista(datos):
        while len(binario) % 4:
            binario.append(0)
        vistas.append({'buffer': 0, 'byteOffset': len(binario), 'byteLength': len(datos)})
        binario.extend(datos)
        return len(vistas) - 1

    def material(mat):
        if mat.name not in mat_indice:
            g = {'name': mat.name, 'pbrMetallicRoughness': {'baseColorFactor': list(mat['rgb']) + [1.0], 'metallicFactor': mat['metal'], 'roughnessFactor': mat['rough']}}
            if mat['glow'] > 0:
                g['emissiveFactor'] = list(mat['rgb'])
                g['extensions'] = {'KHR_materials_emissive_strength': {'emissiveStrength': mat['glow']}}
            materiales.append(g)
            mat_indice[mat.name] = len(materiales) - 1
        return mat_indice[mat.name]

    for pieza, ob in piezas:
        me = ob.data
        me.calc_loop_triangles()
        normales = [n.vector.copy() for n in me.corner_normals]
        sombra = me.color_attributes.get('sombra')
        prims = []
        tris_total = 0
        lo, hi = Vector((1e9,) * 3), Vector((-1e9,) * 3)
        por_material = {}
        for t in me.loop_triangles:
            por_material.setdefault(t.material_index, []).append(t)
        for mi, tris in sorted(por_material.items()):
            claves, pos, nrm, col, idx = {}, [], [], [], []
            for t in tris:
                for l in t.loops:
                    v = me.loops[l].vertex_index
                    # Blender's axes back to the game's, then into the piece's own frame
                    pb = ob.matrix_world @ me.vertices[v].co
                    p = pieza.a_local(Vector((pb.x, pb.z, -pb.y)))
                    nb = normales[l]
                    n = pieza.R.transposed() @ Vector((nb.x, nb.z, -nb.y))
                    s = sombra.data[v].color if sombra else (1.0, 1.0, 1.0, 1.0)
                    # (never black: the darkest corner still shows its colour)
                    c = tuple(int(round(255 * (0.22 + 0.78 * min(max(x, 0.0), 1.0)))) for x in s[:3])
                    clave = (round(p.x, 5), round(p.y, 5), round(p.z, 5), round(n.x, 3), round(n.y, 3), round(n.z, 3), c)
                    k = claves.get(clave)
                    if k is None:
                        k = claves[clave] = len(pos)
                        pos.append((p.x, p.y, p.z))
                        nrm.append(tuple(n.normalized()))
                        col.append(c + (255,))
                        lo = Vector((min(lo.x, p.x), min(lo.y, p.y), min(lo.z, p.z)))
                        hi = Vector((max(hi.x, p.x), max(hi.y, p.y), max(hi.z, p.z)))
                    idx.append(k)
            tris_total += len(tris)
            pmin = [min(p[i] for p in pos) for i in range(3)]
            pmax = [max(p[i] for p in pos) for i in range(3)]
            accesores.append({'bufferView': vista(b''.join(struct.pack('<3f', *p) for p in pos)), 'componentType': 5126, 'count': len(pos), 'type': 'VEC3', 'min': pmin, 'max': pmax})
            accesores.append({'bufferView': vista(b''.join(struct.pack('<3f', *n) for n in nrm)), 'componentType': 5126, 'count': len(nrm), 'type': 'VEC3'})
            accesores.append({'bufferView': vista(b''.join(struct.pack('<4B', *c) for c in col)), 'componentType': 5121, 'normalized': True, 'count': len(col), 'type': 'VEC4'})
            ancho = '<%dH' if len(pos) < 65536 else '<%dI'
            accesores.append({'bufferView': vista(struct.pack(ancho % len(idx), *idx)), 'componentType': 5123 if len(pos) < 65536 else 5125, 'count': len(idx), 'type': 'SCALAR'})
            a = len(accesores) - 4
            prims.append({'attributes': {'POSITION': a, 'NORMAL': a + 1, 'COLOR_0': a + 2}, 'indices': a + 3, 'material': material(me.materials[mi])})
        mallas.append({'name': pieza.nombre, 'primitives': prims})
        nodos.append({'name': pieza.nombre, 'mesh': len(mallas) - 1})
        info[pieza.id] = (tris_total, lo, hi)
    while len(binario) % 4:
        binario.append(0)
    doc = {'asset': {'version': '2.0', 'generator': 'luna tools/modelos'}, 'scene': 0, 'scenes': [{'nodes': list(range(len(nodos)))}], 'nodes': nodos, 'meshes': mallas, 'materials': materiales, 'accessors': accesores, 'bufferViews': vistas, 'buffers': [{'byteLength': len(binario)}], 'extensionsUsed': ['KHR_materials_emissive_strength']}
    texto = json.dumps(doc, separators=(',', ':')).encode('utf-8')
    texto += b' ' * (-len(texto) % 4)
    with open(ruta, 'wb') as f:
        f.write(struct.pack('<4sII', b'glTF', 2, 12 + 8 + len(texto) + 8 + len(binario)))
        f.write(struct.pack('<I4s', len(texto), b'JSON'))
        f.write(texto)
        f.write(struct.pack('<I4s', len(binario), b'BIN\x00'))
        f.write(binario)
    return info


def limpiar():
    """An empty scene."""
    for ob in list(bpy.data.objects):
        bpy.data.objects.remove(ob)
    for me in list(bpy.data.meshes):
        bpy.data.meshes.remove(me)
    for m in list(bpy.data.materials):
        bpy.data.materials.remove(m)



def rugoso(ob, fuerza=0.01, escala=3.0, cortes=3, semilla=0.0):
    """Roughens `ob`: its faces cut finer and every vertex pushed in or out along its normal by
    up to `fuerza`, by a noise `escala` waves to the metre: a thing cast, sintered, hewn."""
    from mathutils import noise

    me = ob.data
    bm = bmesh.new()
    bm.from_mesh(me)
    if cortes > 0:
        bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=cortes, use_grid_fill=True)
    bm.normal_update()
    for v in bm.verts:
        v.co += v.normal * fuerza * noise.noise(v.co * escala + Vector((semilla, semilla * 0.7, -semilla)))
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    suavizar(ob, 50.0)
    return ob
