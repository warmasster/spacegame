"""The bench: a model built to be looked at and measured, not written (`ver.py`, `comprobar.py`).

Builds a recipe without baking its shade, joins what each piece was given, and from there:
poses the pieces that move (as the recipe says they do: `Modelo.mueve`), takes pictures from any
side, and measures — boxes, triangles, what cuts what, how far a ray goes before it meets the
thing. Everything is said in the model's own frame (x left/port, y up, z ahead), never Blender's.
"""
import importlib
import json
import math
import os
import struct
import sys

import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

AQUI = os.path.dirname(os.path.abspath(__file__))
if AQUI not in sys.path:
    sys.path.insert(0, AQUI)

import kit  # noqa: E402

B = kit.B


def cargar_recetas():
    for f in sorted(os.listdir(os.path.join(AQUI, 'recetas'))):
        if f.endswith('.py') and not f.startswith('_'):
            importlib.import_module('recetas.' + f[:-3])


def J(p):
    """A point of Blender's in the game's axes."""
    return Vector((p.x, p.z, -p.y))


class Banco:
    """A model on the bench: `m` the recipe's `Modelo`, `ob` its pieces joined (by id)."""

    def __init__(self, nombre):
        kit.limpiar()
        self.nombre = nombre
        usos = kit.estilos_en_uso()
        estilos = {'estilo_' + e: e for e in kit.ESTILOS if e in usos}
        m = kit.Modelo.de(nombre)
        if nombre in estilos:
            for p in list(m.piezas.values()):
                kit.ESTILOS[estilos[nombre]](m, p)
        else:
            kit.RECETAS[nombre](m)
        piezas = {}
        for ob in m.objetos:
            piezas.setdefault(ob['pieza'], []).append(ob)
        self.ob = {id: kit.unir(obs, m.piezas[id].nombre) for id, obs in piezas.items()}
        for c in m.cortadores:
            bpy.data.objects.remove(c)
        m.cortadores = []
        self.m = m
        self.mov = m.datos['piezas']
        self._arboles = {}

    # ---- posing --------------------------------------------------------------------------------

    def _local(self, id, k):
        """The move of piece `id` at `k` of its travel (0 at one end, 1 at the other; None: at
        rest, as it is built), about its own origin, in Blender's axes; `k` a number, or a list
        of them for a piece with several axes."""
        d = self.mov.get(id, {})
        if k is None or not d:
            return Matrix.Identity(4)
        c = B(self.m.piezas[id].c)
        T = Matrix.Identity(4)
        if 'ejes' in d:
            ks = k if isinstance(k, (list, tuple)) else [k] * len(d['ejes'])
            for e, kk in zip(d['ejes'], ks):
                if kk is None:
                    continue
                ang = e['desde'] + (e['giro'] - e['desde']) * kk
                T = T @ Matrix.Rotation(math.radians(ang), 4, B(Vector(e['eje'])))
        elif 'giro' in d:
            kk = k[0] if isinstance(k, (list, tuple)) else k
            ang = d.get('desde', 0.0) + (d['giro'] - d.get('desde', 0.0)) * kk
            T = Matrix.Rotation(math.radians(ang), 4, B(Vector(d['eje'])))
        elif 'recorre' in d:
            kk = k[0] if isinstance(k, (list, tuple)) else k
            return Matrix.Translation(B(Vector(d['eje']) * d['recorre'] * kk))
        return Matrix.Translation(c) @ T @ Matrix.Translation(-c)

    def posar(self, pose=None):
        """Every piece where `pose` has it: {piece: k} — 0 one end of its travel, 1 the other —,
        the rest as they are built. A piece that rides on another moves with it."""
        pose = pose or {}
        for id, ob in self.ob.items():
            T = self._local(id, pose.get(id))
            padre = self.mov.get(id, {}).get('padre')
            while padre is not None and padre in self.ob and padre != id:
                T = self._local(padre, pose.get(padre)) @ T
                padre = self.mov.get(padre, {}).get('padre')
            ob.matrix_world = T
        self._arboles = {}

    def fin(self):
        """The pose with every moving piece at the end of its travel."""
        return {id: 1.0 for id in self.mov}

    # ---- measuring -----------------------------------------------------------------------------

    def puntos(self, id=None, mat=None):
        """The vertices (game axes, as posed) of a piece, or of all; `mat`: only those of the
        faces of that material."""
        out = []
        for i, ob in self.ob.items():
            if id is not None and i != id:
                continue
            me = ob.data
            usados = None
            if mat is not None:
                nombres = [x.name.split('#')[0] for x in me.materials]
                if mat not in nombres:
                    continue
                k = nombres.index(mat)
                usados = set(v for f in me.polygons if f.material_index == k for v in f.vertices)
            for j, v in enumerate(me.vertices):
                if usados is None or j in usados:
                    out.append(J(ob.matrix_world @ v.co))
        return out

    @staticmethod
    def caja(ps):
        return Vector((min(p.x for p in ps), min(p.y for p in ps), min(p.z for p in ps))), Vector((max(p.x for p in ps), max(p.y for p in ps), max(p.z for p in ps)))

    def tris(self, id=None):
        n = 0
        for i, ob in self.ob.items():
            if id is None or i == id:
                ob.data.calc_loop_triangles()
                n += len(ob.data.loop_triangles)
        return n

    def arbol(self, id):
        if id not in self._arboles:
            ob = self.ob[id]
            me = ob.data
            me.calc_loop_triangles()
            vs = [ob.matrix_world @ v.co for v in me.vertices]
            self._arboles[id] = (BVHTree.FromPolygons(vs, [tuple(t.vertices) for t in me.loop_triangles], epsilon=0.0), vs, me)
        return self._arboles[id]

    def solapes(self, a, b):
        """Where the triangles of piece `a` cut those of `b` as posed: how many pairs, and the
        box they are in (game axes)."""
        ta, va, ma = self.arbol(a)
        tb, _, _ = self.arbol(b)
        pares = ta.overlap(tb)
        if not pares:
            return 0, None
        ps = [J(va[k]) for i, _ in pares for k in ma.loop_triangles[i].vertices]
        return len(pares), self.caja(ps)

    def rayo(self, desde, hacia, max_dist=10.0):
        """How far from `desde` (game axes) going `hacia` the thing is met: (distance, piece,
        material), or None."""
        mejor = None
        o, d = B(Vector(desde)), B(Vector(hacia).normalized())
        for id in self.ob:
            arbol, _, me = self.arbol(id)
            loc, _, idx, dist = arbol.ray_cast(o, d, max_dist)
            if loc is not None and (mejor is None or dist < mejor[0]):
                tri = me.loop_triangles[idx]
                mejor = (dist, id, me.materials[tri.material_index].name.split('#')[0])
        return mejor

    # ---- pictures ------------------------------------------------------------------------------

    def foto(self, ruta, desde=(0.62, 0.42, 0.72), mira=None, dist=None, lente=70.0, orto=None, tam=(1000, 750), arriba=(0, 1, 0), fov_y=None, en=None, piezas=None, ciclos=False):
        """A picture: the camera `dist` off `mira` toward `desde` (a direction), or at `en`
        looking at `mira`; `orto`: an orthographic view that many metres across."""
        sc = bpy.context.scene
        obs = [ob for id, ob in self.ob.items() if piezas is None or id in piezas]
        ps = [J(ob.matrix_world @ v.co) for ob in obs for v in ob.data.vertices]
        lo, hi = self.caja(ps)
        c = Vector(mira) if mira is not None else (lo + hi) * 0.5
        r = max((hi - lo).length * 0.5, 0.03)
        cam = bpy.data.objects.new('camara', bpy.data.cameras.new('camara'))
        sc.collection.objects.link(cam)
        cam.data.clip_start = 0.003
        cam.data.clip_end = 80.0
        cam.data.lens = lente
        cam.data.sensor_fit = 'VERTICAL'
        if orto:
            cam.data.type = 'ORTHO'
            cam.data.ortho_scale = orto if orto is not True else r * 2.3
        if fov_y:
            cam.data.sensor_height = 24.0
            cam.data.lens = 12.0 / math.tan(math.radians(fov_y) / 2)
        lugar = Vector(en) if en is not None else c + Vector(desde).normalized() * (dist or r * 4.4)
        cam.location = B(lugar)
        z = -(B(c) - B(lugar)).normalized()
        x = B(Vector(arriba)).cross(z)
        x = x.normalized() if x.length > 1e-5 else Vector((1, 0, 0))
        cam.rotation_euler = Matrix((x, z.cross(x), z)).transposed().to_euler()
        sc.camera = cam
        if sc.world is None:
            sc.world = bpy.data.worlds.new('mundo')
        if ciclos:
            materiales_de_verdad()
            luces(c, r)
            sc.render.engine = 'CYCLES'
            sc.cycles.device = 'CPU'
            sc.cycles.samples = 32
            sc.cycles.use_denoising = True
            sc.view_settings.view_transform = 'Standard'
            sc.world.use_nodes = True
            fondo = next((n for n in sc.world.node_tree.nodes if n.type == 'BACKGROUND'), None)
            if fondo:
                fondo.inputs[0].default_value = (0.045, 0.05, 0.06, 1.0)
                fondo.inputs[1].default_value = 1.0
        else:
            sc.render.engine = 'BLENDER_WORKBENCH'
            sh = sc.display.shading
            sh.light = 'STUDIO'
            sh.color_type = 'MATERIAL'
            sh.show_cavity = True
            sh.cavity_type = 'BOTH'
            sh.show_shadows = True
            sh.shadow_intensity = 0.3
            sh.show_specular_highlight = True
            sc.display.render_aa = '8'
            sc.world.color = (0.035, 0.04, 0.05)
        sc.render.resolution_x, sc.render.resolution_y = tam
        sc.render.film_transparent = False
        visibles = set(obs)
        for ob in sc.objects:
            if ob.type == 'MESH':
                ob.hide_render = ob not in visibles
        os.makedirs(os.path.dirname(ruta), exist_ok=True)
        sc.render.filepath = ruta
        bpy.ops.render.render(write_still=True)
        bpy.data.objects.remove(cam)
        for ob in sc.objects:
            if ob.type == 'MESH':
                ob.hide_render = False


def materiales_de_verdad():
    """Each material as the game has it (colour, metal, roughness, glow), for a traced picture."""
    for mat in bpy.data.materials:
        if 'rgb' not in mat:
            continue
        mat.use_nodes = True
        b = next((n for n in mat.node_tree.nodes if n.type == 'BSDF_PRINCIPLED'), None)
        if b is None:
            continue
        rgb = list(mat['rgb'])
        if mat.name == kit.TINTE:
            rgb = [kit.lineal(c) for c in (150, 156, 164)]
        b.inputs['Base Color'].default_value = rgb + [1.0]
        b.inputs['Metallic'].default_value = mat['metal']
        b.inputs['Roughness'].default_value = mat['rough']
        if mat['glow'] > 0:
            b.inputs['Emission Color'].default_value = rgb + [1.0]
            b.inputs['Emission Strength'].default_value = mat['glow'] * 1.5


def luces(c, r):
    sc = bpy.context.scene
    for ob in [o for o in sc.objects if o.type == 'LIGHT']:
        bpy.data.objects.remove(ob)
    for nombre, d, fuerza, tam, color in (('clave', (0.5, 0.9, 0.6), 28.0, 0.6, (1.0, 0.97, 0.92)), ('relleno', (-0.8, 0.3, 0.5), 9.0, 1.0, (0.8, 0.88, 1.0)), ('contra', (-0.2, 0.6, -0.9), 16.0, 0.7, (1.0, 1.0, 1.0)), ('suelo', (0.2, -1.0, 0.2), 5.0, 1.2, (0.9, 0.9, 1.0))):
        luz = bpy.data.lights.new(nombre, 'AREA')
        luz.energy = fuerza * (r / 0.5) ** 2
        luz.size = tam * r * 2
        luz.color = color
        ob = bpy.data.objects.new(nombre, luz)
        sc.collection.objects.link(ob)
        lugar = Vector(c) + Vector(d).normalized() * r * 3.0
        ob.location = B(lugar)
        z = (B(lugar) - B(Vector(c))).normalized()
        x = Vector((0, 0, 1)).cross(z)
        x = x.normalized() if x.length > 1e-4 else Vector((1, 0, 0))
        ob.rotation_euler = Matrix((x, z.cross(x), z)).transposed().to_euler()


# ---- a .glb as the game reads it (`core/structure/models.rs`) ------------------------------------


def leer_glb(ruta):
    """{piece: {'pos', 'nrm', 'col', 'idx', 'mat'}} of a `.glb` a recipe wrote: per piece its
    corners (positions, normals, shade), its triangles and the material of each corner; and
    its materials' names. Raises if it is not what the game's reader takes."""
    import numpy as np

    b = open(ruta, 'rb').read()
    if b[:4] != b'glTF':
        raise ValueError('no es un .glb')
    n = struct.unpack('<I', b[12:16])[0]
    doc = json.loads(b[20:20 + n])
    bin_ = b[20 + n + 8:]

    def acc(i):
        a = doc['accessors'][i]
        v = doc['bufferViews'][a['bufferView']]
        t = {5126: 'f4', 5121: 'u1', 5123: 'u2', 5125: 'u4'}[a['componentType']]
        k = {'VEC3': 3, 'VEC4': 4, 'SCALAR': 1}[a['type']]
        return np.frombuffer(bin_, dtype=t, count=a['count'] * k, offset=v.get('byteOffset', 0) + a.get('byteOffset', 0)).reshape(-1, k)

    piezas = {}
    for nodo in doc['nodes']:
        if 'mesh' not in nodo:
            continue
        if any(k in nodo for k in ('translation', 'rotation', 'scale', 'matrix')):
            raise ValueError(f"el nodo '{nodo['name']}' tiene una transformación sin aplicar")
        nombre = nodo['name'].split('.')[0]
        p = piezas.setdefault(nombre, {'pos': [], 'nrm': [], 'col': [], 'idx': [], 'mat': []})
        base = sum(len(x) for x in p['pos'])
        for prim in doc['meshes'][nodo['mesh']]['primitives']:
            if prim.get('mode', 4) != 4:
                raise ValueError(f"'{nombre}': no son triángulos")
            pos = acc(prim['attributes']['POSITION']).astype(float)
            p['pos'].append(pos)
            p['nrm'].append(acc(prim['attributes']['NORMAL']).astype(float))
            p['col'].append(acc(prim['attributes']['COLOR_0']).astype(float) / 255.0)
            p['idx'].append(acc(prim['indices']).astype(int).reshape(-1) + base)
            p['mat'].append(np.full(len(pos), prim['material']))
            base += len(pos)
    for p in piezas.values():
        for k in p:
            p[k] = np.concatenate(p[k])
    return piezas, [m['name'] for m in doc['materials']]
