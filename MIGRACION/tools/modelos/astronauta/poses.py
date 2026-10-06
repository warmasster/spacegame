"""Poses the astronaut and takes pictures of it, to judge how the mesh deforms.

    blender -b -P tools/modelos/astronauta/poses.py -- [pose...] [--glb assets/models/astronauta.glb]
            [--construir] [--lista] [--tam 800] [--alambre] [--sombra]

Without names, every pose of `POSES`. It loads the `.glb` the game reads (so what is judged is
what was exported); `--construir` builds the model in memory instead (quick, to try weights: no
baked shade). `--alambre` draws the gloves' mesh lines; `--sombra` shows the baked shade (COLOR_0)
instead of the materials (it needs the `.glb`). `--lista` says what poses there are.

Pictures: `out/modelos/astronauta_<pose>.png` (every view of the pose side by side), each view
alone in `out/modelos/astronauta_vistas/`, and, when every pose is taken, the contact sheet
`out/modelos/astronauta_poses.png` (the first three views of each). The first-person views
(`primera`, `abajo`) hide what the game hides: the helmet and the neck ring.

A pose is a dictionary `bone -> [(axis, degrees), ...]`: turns applied in that order, each one of the
bone about its own head and relative to its parent, as if the parent were at rest. The axes:

    'X' 'Y' 'Z'   the model's own at rest, as the game sees them: X to its left, Y up, Z forward
    'H'           a finger bone's hinge: the normal of the plane of its digit's three bones; + closes
    'N' 'A' 'S'   of that side's hand at rest: the palm's normal, along the hand (wrist to knuckles)
                  and across it (little finger to index); written for the right hand and mirrored
                  for the left, so the same numbers do the same on both

    (x, y, z)     any other axis, in the model's axes
    '->'          not a turn about an axis: instead of degrees, a direction (model axes) the bone must
                  point along once its parents are posed; it gets there the short way, with no twist

A turn is right-handed about its axis. So an arm raised forward is ('X', -80): the model's left
is +X and the arm hangs toward -Y.

A pose may also be a function of the rig that returns such a dictionary: the fist (`agarre`) is
worked out from the skeleton, each phalanx laid on a handle of the given diameter.
"""
import math
import os
import sys

AQUI = os.path.dirname(os.path.abspath(__file__))
RAIZ = os.path.normpath(os.path.join(AQUI, '..', '..', '..'))
sys.path.insert(0, AQUI)

import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

SALIDA = os.path.join(RAIZ, 'out', 'modelos')
# what the game hides in first person: the helmet and the neck ring, by material name
CASCO = ('Helmet', 'HelmetDark', 'HelmetInner', 'Visor', 'Lamp', 'NeckRing', 'NeckRingBand')
DEDOS = ('thumb', 'index', 'middle', 'ring', 'pinky')


# --------------------------------------------------------------------------------------
# Poses
# --------------------------------------------------------------------------------------

def espejo(pose):
    """The same pose for the other side: `.L` <-> `.R`, the turns about Y and Z the other way."""
    out = {}
    for hueso, giros in pose.items():
        otro = hueso[:-2] + ('.R' if hueso.endswith('.L') else '.L') if hueso[-2:] in ('.L', '.R') else hueso
        out[otro] = [_giro_espejo(e, g) for e, g in giros]
    return out


def _giro_espejo(e, g):
    if e == '->':
        return e, (-g[0], g[1], g[2])
    if not isinstance(e, str):
        return (e[0], -e[1], -e[2]), g
    return e, (-g if e in ('Y', 'Z') else g)


def ambos(pose):
    """A pose given for the left side, on both."""
    return {**pose, **espejo(pose)}


def dedos(lado, cierre, pulgar=None, solo=None):
    """Fingers of one hand: `cierre` = degrees added to the knuckle, middle and end joints of the
    four fingers (or of those named in `solo`), about their hinges; `pulgar` = turns of the thumb's
    three bones (lists of (axis, degrees))."""
    out = {}
    for d in DEDOS[1:]:
        if solo and d not in solo:
            continue
        for k, g in enumerate(cierre):
            out[f'{d}.{k + 1:02d}.{lado}'] = [('H', g)]
    for k, giros in enumerate(pulgar or ()):
        out[f'thumb.{k + 1:02d}.{lado}'] = list(giros)
    return out


# arms forward and the hands apart in front of the chest: what the hand poses are seen in
BRAZOS_DELANTE = ambos({
    'upperarm.L': [('->', (0.35, -0.62, 0.70))],
    'forearm.L': [('->', (-0.12, 0.28, 0.95))],
})

PULGAR_AGARRE = ([('A', 30), ('H', 12)], [('H', 28)], [('H', 30)])
ABIERTA = {'cierre': (-30, -45, -30), 'pulgar': ([('A', -22), ('H', -18)], [('H', -22)], [('H', -18)])}

VISTAS_MANOS = [('mano', 'R', 'palma'), ('mano', 'R', 'dorso'), ('mano', 'R', 'pulgar'), ('mano', 'R', 'menique'), ('mano', 'R', 'punta'),
                ('mano', 'R', 'tres'), ('mano', 'L', 'palma'), ('mano', 'L', 'dorso'), ('mano', 'L', 'pulgar'), 'primera']


def sitio_mango(m, radio):
    """Where a handle of that radius lies in a hand (its frame at rest, `marco_mano`): against the
    palm just before the knuckles, its axis across the hand."""
    return m['nudillos'] - m['A'] * 0.012 + m['N'] * (0.0205 + radio)     # the palm's skin is 21 mm from the bones there


def agarre(rig, lado, diametro, pulgar=PULGAR_AGARRE, solo=None):
    """The four fingers of a hand closed round a handle of that diameter held against the palm: each
    phalanx is turned about its joint until it lies on the handle (its bone tangent to the handle
    grown by half the finger's thickness). Returns the pose of the fingers (and of the thumb as
    given: it wraps the other way and is not solved)."""
    import dedos as manos           # the glove's module (here `dedos` is the pose helper above)
    m = marco_mano(rig, lado)
    c = sitio_mango(m, diametro / 2)
    out = {}
    for i, d in enumerate(DEDOS[1:]):
        if solo and d not in solo:
            continue
        nombres = [f'{d}.{k}.{lado}' for k in ('01', '02', '03')]
        p = [cabeza(rig, n) for n in nombres]
        h = (p[1] - p[0]).cross(p[2] - p[1]).normalized()
        reposo = [(rig.matrix_world.to_3x3() @ rig.data.bones[n].matrix_local.to_3x3() @ Vector((0, 1, 0))).normalized() for n in nombres]
        largo = [(p[1] - p[0]).length, (p[2] - p[1]).length, 0.0]
        ro = diametro / 2 + 0.93 * manos.FINGER_R[i]
        q = p[0].copy()
        antes_reposo, antes = None, None
        for k in range(3):
            w = c - q
            w = w - h * w.dot(h)                         # the handle's centre, in the finger's plane
            lejos = max(w.length, 1e-6)
            # the direction from this joint that just touches the grown handle, the handle on the palm side
            u = Matrix.Rotation(-math.asin(min(1.0, ro / lejos)), 3, h) @ w.normalized()
            giro = _angulo(reposo[k] if k == 0 else antes, u, h) - (0.0 if k == 0 else _angulo(antes_reposo, reposo[k], h))
            if k > 0:
                # a joint does not bend backwards, nor past a right angle and a bit
                total = max(0.0, min(110.0, _angulo(antes_reposo, reposo[k], h) + giro))
                giro = total - _angulo(antes_reposo, reposo[k], h)
                u = Matrix.Rotation(math.radians(total), 3, h) @ antes
            out[nombres[k]] = [('H', giro)]
            q = q + u * largo[k]
            antes_reposo, antes = reposo[k], u
    for k, giros in enumerate(pulgar or ()):
        out[f'thumb.{k + 1:02d}.{lado}'] = list(giros)
    return out


def _angulo(a, b, eje):
    """Degrees from `a` to `b` about `eje` (right-handed)."""
    return math.degrees(math.atan2(a.cross(b).dot(eje), a.dot(b)))


def puno(rig):
    return {**BRAZOS_DELANTE, **agarre(rig, 'L', 0.04), **agarre(rig, 'R', 0.04)}


def gatillo(rig):
    """The grip of the fist, the index finger alone off the handle and hooked round a trigger."""
    out = dict(BRAZOS_DELANTE)
    for lado in 'LR':
        mano = agarre(rig, lado, 0.04)
        for hueso, mas in ((f'index.01.{lado}', -30), (f'index.02.{lado}', 22), (f'index.03.{lado}', 18)):
            mano[hueso] = [('H', mano[hueso][0][1] + mas)]
        out.update(mano)
    return out


POSES = {
    'reposo': {
        'huesos': {},
        'vistas': ['frente', 'frente34', 'lado', 'espalda', 'primera', 'abajo'],
    },
    # arms raised forward to hold a tool at shoulder height: shoulders flexed 80, elbows bent 80
    'herramienta': {
        'huesos': ambos({
            'upperarm.L': [('Z', -4), ('X', -80)],
            'forearm.L': [((-0.6, 0.0, -0.8), 80)],
            **dedos('L', (25, 15, 12), PULGAR_AGARRE),
        }),
        'vistas': ['frente', 'frente34', 'lado', 'espalda34', 'arriba', 'primera',
                   ('hueso', 'upperarm.L', (0.9, 0.5, 0.9), 1.2), ('hueso', 'upperarm.L', (1.0, 0.3, -0.6), 1.2), ('hueso', 'forearm.L', (1.0, 0.2, 0.2), 1.0), ('hueso', 'upperarm.L', (0.3, -0.6, 1.0), 1.2)],
    },
    # the same lower, at the chest: shoulders flexed 40, elbows bent 95 (how a tool is carried)
    'herramienta_baja': {
        'huesos': ambos({
            'upperarm.L': [('Z', -6), ('X', -40)],
            'forearm.L': [((-0.75, 0.0, -0.65), 95)],
            **dedos('L', (25, 15, 12), PULGAR_AGARRE),
        }),
        'vistas': ['frente', 'frente34', 'lado', 'espalda34', 'arriba', 'primera', 'abajo',
                   ('hueso', 'upperarm.L', (0.9, 0.5, 0.9), 1.2), ('hueso', 'forearm.L', (1.0, 0.2, 0.2), 1.0)],
    },
    # one arm across the chest (in front of the chest box), the other one hanging
    'brazo_cruzado': {
        'huesos': {
            'upperarm.L': [('->', (-0.18, -0.55, 0.82))],
            'forearm.L': [('->', (-0.93, 0.25, 0.28))],
        },
        'vistas': ['frente', 'frente34', 'lado', 'arriba', 'primera', ('hueso', 'upperarm.L', (0.9, 0.6, 0.9), 1.2), ('hueso', 'upperarm.L', (1.0, 0.3, -0.6), 1.2), ('hueso', 'upperarm.L', (0.2, -0.7, 1.0), 1.2)],
    },
    # past what was asked, to know where the mesh gives up: the left arm straight up and its elbow
    # shut 130, the right arm out to the side and level
    'extremos': {
        'huesos': {
            'upperarm.L': [('->', (0.12, 0.97, 0.2))], 'forearm.L': [((-1.0, 0.0, 0.0), 130)],
            'upperarm.R': [('->', (-1.0, 0.0, 0.0))], 'forearm.R': [('->', (-1.0, 0.0, 0.05))],
            'thigh.R': [('Z', -40)], 'spine': [('Y', 25)], 'chest': [('Y', 15)],
        },
        'vistas': ['frente', 'frente34', 'espalda', 'espalda34', ('hueso', 'upperarm.L', (0.9, 0.3, 0.9), 1.3), ('hueso', 'upperarm.L', (0.6, 0.2, -1.0), 1.3),
                   ('hueso', 'upperarm.R', (-0.5, 0.5, 1.0), 1.3), ('hueso', 'upperarm.R', (-0.4, -0.5, -1.0), 1.3), ('hueso', 'thigh.R', (-0.3, 0.0, 1.0), 1.4)],
    },
    # a walking stride: thighs +-35, the trailing knee bent 50, arms swinging
    'zancada': {
        'huesos': {
            'thigh.L': [('X', -35)], 'shin.L': [('X', 12)], 'foot.L': [('X', 8)],
            'thigh.R': [('X', 35)], 'shin.R': [('X', 50)], 'foot.R': [('X', 18)], 'toe.R': [('X', -25)],
            'upperarm.L': [('X', 25)], 'forearm.L': [('X', -15)],
            'upperarm.R': [('X', -28)], 'forearm.R': [('X', -30)],
            'spine': [('X', 4)],
        },
        'vistas': ['lado', 'lado_d', 'frente34', 'espalda34', 'primera',
                   ('hueso', 'shin.R', (-1.0, 0.1, -0.3), 1.3), ('hueso', 'thigh.L', (0.9, 0.2, 0.7), 1.3), ('hueso', 'thigh.L', (0.2, 0.1, -1.0), 1.5), ('hueso', 'pelvis', (0.0, -0.25, 1.0), 1.5)],
    },
    # a deep knee bend: thighs up 95, knees 100
    'sentadilla': {
        'huesos': {
            **ambos({'thigh.L': [('Z', 8), ('X', -95)], 'shin.L': [('X', 100)], 'foot.L': [('X', -8)],
                     'upperarm.L': [('X', -45)], 'forearm.L': [('X', -40)]}),
            'spine': [('X', 12)],
        },
        'vistas': ['lado', 'frente34', 'espalda34', 'frente', 'primera',
                   ('hueso', 'shin.L', (1.0, 0.25, 0.25), 1.3), ('hueso', 'shin.L', (0.5, 0.3, -1.0), 1.3), ('hueso', 'pelvis', (0.0, -0.5, 1.0), 1.5), ('hueso', 'pelvis', (0.6, 0.1, -1.0), 1.5)],
    },
    # feet pitched at the ankle: the left one up 25, the right one down 25
    'tobillos': {
        'huesos': {'foot.L': [('X', -25)], 'foot.R': [('X', 25)]},
        'vistas': [('hueso', 'foot.L', (1.0, 0.15, 0.1), 0.95), ('hueso', 'foot.R', (-1.0, 0.15, 0.1), 0.95), ('hueso', 'foot.L', (0.7, 0.3, 1.0), 0.95), ('hueso', 'foot.R', (-0.7, 0.3, 1.0), 0.95),
                   ('hueso', 'foot.L', (0.7, 0.3, -1.0), 0.95), ('hueso', 'foot.R', (-0.7, 0.3, -1.0), 0.95), ('hueso', 'foot.L', (-1.0, 0.15, 0.1), 0.95), ('hueso', 'foot.R', (1.0, 0.15, 0.1), 0.95)],
    },
    # the wrists: the right hand bent 40 toward the palm, the left one 40 toward its back
    'munecas': {
        'huesos': {**BRAZOS_DELANTE, 'hand.R': [('S', 40)], 'hand.L': [('S', -40)]},
        'vistas': [('mano', 'R', 'pulgar'), ('mano', 'R', 'menique'), ('mano', 'R', 'dorso'), ('mano', 'R', 'palma'), ('mano', 'L', 'pulgar'), ('mano', 'L', 'menique'), ('mano', 'L', 'dorso'), ('mano', 'L', 'palma'), 'primera'],
        'lejos': 1.25,
    },
    # the wrists turned: the right hand rolled 60 about the forearm, the left one swung 25 sideways
    'munecas_giro': {
        'huesos': {**BRAZOS_DELANTE, 'hand.R': [('A', 60)], 'hand.L': [('N', 25)]},
        'vistas': [('mano', 'R', 'pulgar'), ('mano', 'R', 'menique'), ('mano', 'R', 'dorso'), ('mano', 'R', 'palma'), ('mano', 'L', 'pulgar'), ('mano', 'L', 'menique'), ('mano', 'L', 'dorso'), ('mano', 'L', 'palma'), 'primera'],
        'lejos': 1.25,
    },
    # the hands as they are at rest (the arms forward, to see them from every side)
    'manos': {
        'huesos': BRAZOS_DELANTE,
        'vistas': VISTAS_MANOS,
    },
    # a closed fist round a 4 cm handle
    'puno': {
        'huesos': puno,
        'mango': 0.04,
        'vistas': VISTAS_MANOS,
    },
    # a fully open hand
    'mano_abierta': {
        'huesos': {**BRAZOS_DELANTE, **dedos('L', **ABIERTA), **dedos('R', **ABIERTA)},
        'vistas': VISTAS_MANOS,
    },
    # the grip of the fist, the index finger alone curled further: a trigger
    'gatillo': {
        'huesos': gatillo,
        'mango': 0.04,
        'vistas': VISTAS_MANOS,
    },
}

# whole-body views: where the camera is seen from the model (game axes), what it looks at, how far
CUERPO = {
    'frente': ((0.0, 0.12, 1.0), (0, 0.93, 0), 3.1),
    'frente34': ((0.75, 0.25, 1.0), (0, 0.93, 0), 3.1),
    'lado': ((1.0, 0.1, 0.05), (0, 0.93, 0), 3.1),
    'lado_d': ((-1.0, 0.1, 0.05), (0, 0.93, 0), 3.1),
    'espalda': ((0.0, 0.12, -1.0), (0, 0.93, 0), 3.1),
    'espalda34': ((-0.75, 0.3, -1.0), (0, 0.93, 0), 3.1),
    'arriba': ((0.25, 1.0, 0.75), (0, 1.25, 0.15), 3.0),
}
# hand views: where the camera is in the hand's own frame (across, palm normal, along), and its up
MANO = {
    'palma': ((0.0, 1.0, 0.25), 'A'),
    'dorso': ((0.0, -1.0, 0.25), 'A'),
    'pulgar': ((1.0, 0.15, 0.15), 'A'),
    'menique': ((-1.0, 0.15, 0.15), 'A'),
    'punta': ((0.1, 0.35, 1.0), 'N'),
    'tres': ((0.7, 0.7, 0.45), 'A'),
}


# --------------------------------------------------------------------------------------
# The rig
# --------------------------------------------------------------------------------------

def J(v):
    """A vector in the game's axes (Y up, +Z forward) -> Blender's (Z up, -Y forward)."""
    return Vector((v[0], -v[2], v[1]))


def cargar(ruta):
    """An empty scene with the `.glb`: returns its armature."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=ruta, bone_heuristic='BLENDER', import_shading='NORMALS')
    rig = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
    # what the pictures show is each material's own colour, as the file gives it
    import glb
    for m in glb.leer(ruta)[0]['materials']:
        mat = bpy.data.materials.get(m['name'])
        pbr = m.get('pbrMetallicRoughness', {})
        if mat:
            mat.diffuse_color = pbr.get('baseColorFactor', (1.0, 1.0, 1.0, 1.0))
            mat.roughness = pbr.get('roughnessFactor', 1.0)
            mat.metallic = pbr.get('metallicFactor', 1.0)
    return rig


def construir():
    import astronaut
    return astronaut.build()


def cabeza(rig, hueso):
    return rig.matrix_world @ rig.data.bones[hueso].head_local


def marco_mano(rig, lado):
    """The hand's frame at rest (armature space): centre of the palm, knuckle line centre, and the
    unit vectors across (little finger -> index), palm normal, along (wrist -> knuckles)."""
    mu = cabeza(rig, f'hand.{lado}')
    nud = [cabeza(rig, f'{d}.01.{lado}') for d in DEDOS[1:]]
    k = sum(nud, Vector()) / 4
    a = (k - mu).normalized()
    s = nud[0] - nud[3]
    s = (s - a * s.dot(a)).normalized()
    n = a.cross(s)
    if n.dot(cabeza(rig, f'middle.03.{lado}') - cabeza(rig, f'middle.01.{lado}')) < 0:
        n = -n
    return {'centro': (mu + k) / 2, 'nudillos': k, 'S': s, 'N': n, 'A': a}


def eje(rig, hueso, nombre):
    """(axis in armature space, sign of the angle) for an axis name of a pose."""
    if not isinstance(nombre, str):
        return J(nombre).normalized(), 1.0
    if nombre in 'XYZ':
        return J({'X': (1, 0, 0), 'Y': (0, 1, 0), 'Z': (0, 0, 1)}[nombre]), 1.0
    lado = hueso[-1]
    if nombre == 'H':
        d = hueso.split('.')[0]
        a, b, c = (cabeza(rig, f'{d}.{k}.{lado}') for k in ('01', '02', '03'))
        return (b - a).cross(c - b).normalized(), 1.0
    return marco_mano(rig, lado)[nombre], (1.0 if lado == 'R' else -1.0)


def aplicar(rig, huesos):
    """Puts the rig in the pose (see the module's text for what a pose is). `huesos` may be a
    function of the rig that returns the dictionary (a pose worked out from the skeleton)."""
    if callable(huesos):
        huesos = huesos(rig)
    rig.data.pose_position = 'POSE'
    for pb in rig.pose.bones:
        pb.rotation_mode = 'QUATERNION'
        pb.rotation_quaternion = (1, 0, 0, 0)
        pb.location = (0, 0, 0)
        pb.scale = (1, 1, 1)
    falta = [h for h in huesos if h not in rig.data.bones]
    if falta:
        raise SystemExit(f"la pose nombra huesos que no hay: {', '.join(falta)}")
    mundo = {}                      # bone -> how it is turned in all, parents included (armature space)
    for bone in rig.data.bones:     # parents come before their children
        hueso = bone.name
        padre = mundo[bone.parent.name] if bone.parent else Matrix.Identity(3)
        r = Matrix.Identity(3)
        for nombre, grados in huesos.get(hueso, ()):
            if nombre == '->':
                ahora = r @ (bone.tail_local - bone.head_local).normalized()
                quiere = padre.inverted() @ J(grados).normalized()
                r = ahora.rotation_difference(quiere).to_matrix() @ r
                continue
            v, signo = eje(rig, hueso, nombre)
            r = Matrix.Rotation(math.radians(grados * signo), 3, v) @ r
        mundo[hueso] = padre @ r
        b = bone.matrix_local.to_3x3()
        rig.pose.bones[hueso].rotation_quaternion = (b.inverted() @ r @ b).to_quaternion()
    bpy.context.view_layer.update()


def llevado(rig, hueso):
    """Rest (armature space) -> where the bone carries it in the pose."""
    return rig.matrix_world @ rig.pose.bones[hueso].matrix @ rig.data.bones[hueso].matrix_local.inverted() @ rig.matrix_world.inverted()


# --------------------------------------------------------------------------------------
# Pictures
# --------------------------------------------------------------------------------------

def preparar(tam, sombra=False):
    sc = bpy.context.scene
    sc.render.engine = 'BLENDER_WORKBENCH'
    sh = sc.display.shading
    sh.light = 'STUDIO'
    sh.color_type = 'VERTEX' if sombra else 'MATERIAL'
    sh.show_cavity = True
    sh.cavity_type = 'BOTH'
    sh.cavity_ridge_factor = 0.6
    sh.cavity_valley_factor = 1.2
    sh.show_shadows = False                     # the workbench's shadows draw wedges that look like mesh faults
    sh.show_specular_highlight = True
    sh.show_object_outline = False
    sc.display.light_direction = (-0.45, -0.35, 0.82)
    sc.display.render_aa = '8'
    sc.render.resolution_x = sc.render.resolution_y = tam
    sc.render.resolution_percentage = 100
    sc.render.film_transparent = False
    sc.render.image_settings.file_format = 'PNG'
    sc.view_settings.view_transform = 'Standard'
    if sc.world is None:
        sc.world = bpy.data.worlds.new('mundo')
    sc.world.color = (0.2, 0.215, 0.235)        # a neutral grey: the white suit reads well on it


def camara(pos, mira, arriba, lente=50.0):
    sc = bpy.context.scene
    cam = sc.camera
    if cam is None:
        cam = bpy.data.objects.new('camara', bpy.data.cameras.new('camara'))
        sc.collection.objects.link(cam)
        sc.camera = cam
    cam.data.lens = lente
    cam.data.sensor_width = 36.0
    cam.data.clip_start = 0.01
    cam.data.clip_end = 100.0
    f = (Vector(mira) - Vector(pos)).normalized()
    r = f.cross(Vector(arriba)).normalized()
    u = r.cross(f)
    m = Matrix((r, u, -f)).transposed().to_4x4()
    m.translation = Vector(pos)
    cam.matrix_world = m
    return cam


def rotulo(cam, texto):
    """The picture's name, written in a corner of it."""
    viejo = bpy.data.objects.get('rotulo')
    if viejo:
        bpy.data.objects.remove(viejo)
    cu = bpy.data.curves.new('rotulo', 'FONT')
    cu.body = texto
    ob = bpy.data.objects.new('rotulo', cu)
    bpy.context.scene.collection.objects.link(ob)
    mat = bpy.data.materials.get('rotulo') or bpy.data.materials.new('rotulo')
    mat.diffuse_color = (1.0, 1.0, 1.0, 1.0)
    cu.materials.append(mat)
    ob.display.show_shadows = False
    medio = 18.0 / cam.data.lens          # half the picture's width at 1 m
    d = 0.05
    ob.matrix_world = cam.matrix_world @ Matrix.Translation((-0.95 * medio * d, -0.92 * medio * d, -d)) @ Matrix.Scale(0.11 * medio * d, 4)
    return ob


def foto(ruta, texto):
    sc = bpy.context.scene
    rotulo(sc.camera, texto)
    sc.render.filepath = ruta
    bpy.ops.render.render(write_still=True)


def mango(rig, lado, diametro):
    """A handle across the palm of that hand, where a fist closes round it."""
    m = marco_mano(rig, lado)
    r = diametro / 2
    me = bpy.data.meshes.new('mango')
    import bmesh
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=40, radius1=r, radius2=r, depth=0.19)
    bm.to_mesh(me)
    bm.free()
    me.shade_smooth()
    ob = bpy.data.objects.new('mango.' + lado, me)
    bpy.context.scene.collection.objects.link(ob)
    mat = bpy.data.materials.get('mango') or bpy.data.materials.new('mango')
    mat.diffuse_color = (0.85, 0.42, 0.1, 1.0)
    me.materials.append(mat)
    c = sitio_mango(m, r)
    z = m['S']
    x = m['N']
    y = z.cross(x)
    base = Matrix((x, y, z)).transposed().to_4x4()
    base.translation = c
    ob.matrix_world = llevado(rig, f'hand.{lado}') @ base
    return ob


def vista(rig, v, lejos_mano=0.78, lente_mano=85.0):
    """Places the camera for a view; returns (its name, objects to hide for it)."""
    if isinstance(v, str):
        if v in ('primera', 'abajo'):
            # the player's eyes: inside the helmet, looking forward and a little down, or down at the body
            ojo = llevado(rig, 'head') @ J((0.0, 1.66, 0.07))
            hacia = llevado(rig, 'head').to_3x3() @ J((0.0, -0.55, 1.0) if v == 'primera' else (0.0, -1.0, 0.42))
            camara(ojo, ojo + hacia, llevado(rig, 'head').to_3x3() @ J((0, 1, 0) if v == 'primera' else (0, 0.4, 1)), lente=17.0)
            return v, CASCO
        d, mira, lejos = CUERPO[v]
        camara(J(mira) + J(d).normalized() * lejos, J(mira), (0, 0, 1), lente=50.0)
        return v, ()
    if v[0] == 'mano':
        _, lado, cual = v
        m = marco_mano(rig, lado)
        t = llevado(rig, f'hand.{lado}')
        r3 = t.to_3x3()
        d, arriba = MANO[cual]
        s = m['S']
        centro = t @ (m['nudillos'] - m['A'] * 0.012 + m['N'] * 0.02)
        donde = centro + r3 @ (s * d[0] + m['N'] * d[1] + m['A'] * d[2]).normalized() * lejos_mano
        camara(donde, centro, r3 @ m[arriba], lente=lente_mano)
        return f'mano{lado}_{cual}', ()
    if v[0] == 'hueso':
        _, hueso, d, lejos = v
        pb = rig.pose.bones[hueso]
        centro = rig.matrix_world @ ((pb.head + pb.tail) / 2)
        camara(centro + J(d).normalized() * lejos, centro, (0, 0, 1), lente=50.0)
        return f'{hueso.replace(".", "")}_{"".join("%+d" % round(x * 10) for x in d)}', ()
    raise ValueError(v)


def alambre(objetos):
    """Dark copies of the meshes' edges, to see the topology in the pictures."""
    mat = bpy.data.materials.new('alambre')
    mat.diffuse_color = (0.02, 0.02, 0.02, 1.0)
    for ob in objetos:
        w = bpy.data.objects.new(ob.name + '.alambre', ob.data.copy())
        bpy.context.scene.collection.objects.link(w)
        w.parent = ob.parent
        w.matrix_world = ob.matrix_world
        for g in ob.vertex_groups:
            if g.name not in w.vertex_groups:
                w.vertex_groups.new(name=g.name)
        for mod in ob.modifiers:
            if mod.type == 'ARMATURE':
                w.modifiers.new('Armature', 'ARMATURE').object = mod.object
        wm = w.modifiers.new('alambre', 'WIREFRAME')
        wm.thickness = 0.0005
        wm.offset = 1.0
        wm.use_replace = True
        w.data.materials.clear()
        w.data.materials.append(mat)
        w.display.show_shadows = False


def leer_png(ruta):
    img = bpy.data.images.load(ruta, check_existing=False)
    w, h = img.size
    a = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    bpy.data.images.remove(img)
    return a.reshape(h, w, 4)[::-1]            # top row first


def guardar_png(ruta, a):
    h, w = a.shape[:2]
    img = bpy.data.images.new('hoja', w, h, alpha=False)
    img.pixels.foreach_set(np.ascontiguousarray(a[::-1], dtype=np.float32).ravel())
    img.filepath_raw = ruta
    img.file_format = 'PNG'
    img.save()
    bpy.data.images.remove(img)


def reducir(a, k):
    if k <= 1:
        return a
    h, w = a.shape[0] // k * k, a.shape[1] // k * k
    return a[:h, :w].reshape(h // k, k, w // k, k, 4).mean(axis=(1, 3))


def mosaico(cuadros, columnas):
    """Pictures of the same size, in a grid."""
    h, w = cuadros[0].shape[:2]
    filas = (len(cuadros) + columnas - 1) // columnas
    hoja = np.zeros((filas * h, columnas * w, 4), dtype=np.float32)
    hoja[..., 3] = 1.0
    hoja[..., :3] = 0.06
    for k, c in enumerate(cuadros):
        f, col = divmod(k, columnas)
        hoja[f * h:(f + 1) * h, col * w:(col + 1) * w] = c
    return hoja


def fotos(rig, nombres, tam=800, con_alambre=False, sombra=False):
    """Poses, takes every view of every pose, writes the pictures. Returns the files written."""
    os.makedirs(os.path.join(SALIDA, 'astronauta_vistas'), exist_ok=True)
    preparar(tam, sombra)
    if con_alambre:
        alambre([o for o in bpy.context.scene.objects if o.type == 'MESH' and o.name.startswith('Glove.')])
    sufijo = '_sombra' if sombra else ''
    escritas, para_hoja = [], []
    for nombre in nombres:
        pose = POSES[nombre]
        aplicar(rig, pose['huesos'])
        mangos = [mango(rig, lado, pose['mango']) for lado in ('L', 'R')] if pose.get('mango') else []
        cuadros = []
        for v in pose['vistas']:
            vn, ocultar = vista(rig, v, lejos_mano=0.78 * pose.get('lejos', 1.0))
            ocultos = [o for o in bpy.context.scene.objects if o.type == 'MESH' and (o.name.split('.')[0] in ocultar or o.name in ocultar)]
            for o in ocultos:
                o.hide_render = True
            ruta = os.path.join(SALIDA, 'astronauta_vistas', f'{nombre}_{vn}{sufijo}.png')
            foto(ruta, f'{nombre}: {vn}')
            for o in ocultos:
                o.hide_render = False
            cuadros.append(leer_png(ruta))
        for o in mangos:
            bpy.data.objects.remove(o)
        ruta = os.path.join(SALIDA, f'astronauta_{nombre}{sufijo}.png')
        guardar_png(ruta, mosaico(cuadros, min(len(cuadros), 5 if len(cuadros) > 4 else 4)))
        escritas.append(ruta)
        para_hoja += [reducir(c, 2) for c in cuadros[:3]]
        print('pose', nombre, '->', ruta)
    aplicar(rig, {})
    if list(nombres) == list(POSES):
        ruta = os.path.join(SALIDA, f'astronauta_poses{sufijo}.png')
        guardar_png(ruta, mosaico(para_hoja, 6))
        escritas.append(ruta)
        print('hoja ->', ruta)
    return escritas


def main():
    args = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
    if '--lista' in args:
        for n, p in POSES.items():
            print(f"{n}: {len(p['vistas'])} vistas")
        return

    def valor(clave, defecto):
        return args[args.index(clave) + 1] if clave in args else defecto

    salta = {args.index(k) + 1 for k in ('--glb', '--tam') if k in args}
    nombres = [a for i, a in enumerate(args) if not a.startswith('--') and i not in salta] or list(POSES)
    falta = [n for n in nombres if n not in POSES]
    if falta:
        raise SystemExit(f"no hay pose: {', '.join(falta)} (hay: {', '.join(POSES)})")
    if '--construir' in args:
        rig = construir()
    else:
        ruta = valor('--glb', os.path.join(RAIZ, 'assets', 'models', 'astronauta.glb'))
        rig = cargar(os.path.abspath(ruta))
    fotos(rig, nombres, tam=int(valor('--tam', 800)), con_alambre='--alambre' in args, sombra='--sombra' in args)


if __name__ == '__main__':
    main()
