"""
Procedural EVA-suit astronaut for the lunar prototype, built and rigged headless in Blender.

    <python with bpy>  tools/blender/astronaut.py  --out public/assets/astronaut.glb  [--preview preview.png] [--pose]

Conventions (Blender): Z up, character faces -Y, metres. The glTF exporter converts to
three.js conventions (Y up, character facing +Z). Material names are a contract with the
client (see src/client/player/astronautModel.ts).
"""

import argparse
import math
import sys

import bpy  # noqa: I001  (bpy must be imported before bmesh)
import bmesh
from mathutils import Matrix, Vector

from suitkit import (
    catmull_rom,
    lathe,
    link,
    make_materials,
    mesh_object,
    rounded_box,
    rounded_profile,
    sweep,
    transform_bm,
)

# --------------------------------------------------------------------------------------
# Skeleton (rest pose = relaxed A-pose). head/tail in metres.
# --------------------------------------------------------------------------------------

BONES = {
    # name: (head, tail, parent, deform)
    'root': ((0, 0, 0), (0, 0, 0.25), None, False),
    'pelvis': ((0, 0.01, 0.95), (0, 0.01, 1.07), 'root', True),
    'spine': ((0, 0.01, 1.07), (0, 0.012, 1.24), 'pelvis', True),
    'chest': ((0, 0.012, 1.24), (0, 0.015, 1.50), 'spine', True),
    'neck': ((0, 0.015, 1.50), (0, 0.01, 1.60), 'chest', False),
    'head': ((0, 0.01, 1.60), (0, 0.0, 1.78), 'neck', False),
}
for side, sx in (('L', 1), ('R', -1)):
    BONES.update({
        f'clavicle.{side}': ((0.05 * sx, 0.015, 1.43), (0.18 * sx, 0.02, 1.44), 'chest', True),
        f'upperarm.{side}': ((0.20 * sx, 0.02, 1.43), (0.325 * sx, 0.03, 1.17), f'clavicle.{side}', True),
        f'forearm.{side}': ((0.325 * sx, 0.03, 1.17), (0.405 * sx, 0.0, 0.935), f'upperarm.{side}', True),
        f'hand.{side}': ((0.405 * sx, 0.0, 0.935), (0.43 * sx, -0.012, 0.80), f'forearm.{side}', True),
        f'thigh.{side}': ((0.105 * sx, 0.01, 0.92), (0.122 * sx, 0.0, 0.52), 'pelvis', True),
        f'shin.{side}': ((0.122 * sx, 0.0, 0.52), (0.132 * sx, 0.02, 0.125), f'thigh.{side}', True),
        f'foot.{side}': ((0.132 * sx, 0.02, 0.125), (0.138 * sx, -0.10, 0.045), f'shin.{side}', True),
        f'toe.{side}': ((0.138 * sx, -0.10, 0.045), (0.14 * sx, -0.17, 0.04), f'foot.{side}', True),
    })

HELMET_C = Vector((0.0, -0.005, 1.622))   # centre of the helmet sphere
HELMET_R = 0.172


def V(*a):
    return Vector(a)


# --------------------------------------------------------------------------------------
# Armature
# --------------------------------------------------------------------------------------

def build_armature():
    arm = bpy.data.armatures.new('Rig')
    rig = bpy.data.objects.new('Astronaut', arm)
    link(rig)
    bpy.context.view_layer.objects.active = rig
    rig.select_set(True)
    bpy.ops.object.mode_set(mode='EDIT')
    for name, (head, tail, parent, deform) in BONES.items():
        eb = arm.edit_bones.new(name)
        eb.head, eb.tail = V(*head), V(*tail)
        # Consistent rolls: local Z points forward (-Y) so local X is the main hinge axis.
        eb.align_roll(V(0, -1, 0) if abs((eb.tail - eb.head).normalized().y) < 0.9 else V(0, 0, 1))
        eb.use_deform = deform
    for name, (_, _, parent, _) in BONES.items():
        if parent:
            arm.edit_bones[name].parent = arm.edit_bones[parent]
    bpy.ops.object.mode_set(mode='OBJECT')
    rig.select_set(False)
    return rig


def bone_axis(name):
    h, t = V(*BONES[name][0]), V(*BONES[name][1])
    return h, t


# --------------------------------------------------------------------------------------
# Soft suit (fabric) — skin modifier graph → subdivision → convolutes
# --------------------------------------------------------------------------------------

def build_body(mats):
    nodes = {}   # name -> (co, (rx, ry))
    edges = []

    def n(name, co, r, parent=None):
        nodes[name] = (V(*co), r if isinstance(r, tuple) else (r, r))
        if parent:
            edges.append((parent, name))

    n('pelvis', (0, 0.012, 0.975), (0.185, 0.150))
    n('belly', (0, 0.010, 1.075), (0.180, 0.140), 'pelvis')
    n('waist', (0, 0.012, 1.15), (0.192, 0.148), 'belly')
    n('chest', (0, 0.016, 1.29), (0.222, 0.160), 'waist')
    n('upper', (0, 0.020, 1.415), (0.205, 0.148), 'chest')
    n('neck', (0, 0.018, 1.545), (0.142, 0.136), 'upper')

    for s, sx in (('L', 1), ('R', -1)):
        # arm: shoulder → elbow → wrist, extra nodes around the elbow for the convolutes
        n(f'sh.{s}', (0.175 * sx, 0.02, 1.425), 0.098, 'upper')
        arm = [
            (f'ua0.{s}', (0.225 * sx, 0.022, 1.385), 0.092),
            (f'ua1.{s}', (0.262 * sx, 0.025, 1.300), 0.084),
            (f'ua2.{s}', (0.292 * sx, 0.028, 1.232), 0.079),
            (f'el0.{s}', (0.312 * sx, 0.029, 1.200), 0.076),
            (f'el1.{s}', (0.328 * sx, 0.029, 1.160), 0.075),
            (f'el2.{s}', (0.343 * sx, 0.025, 1.120), 0.072),
            (f'fa0.{s}', (0.360 * sx, 0.018, 1.070), 0.069),
            (f'fa1.{s}', (0.384 * sx, 0.008, 1.000), 0.064),
            (f'wr.{s}', (0.400 * sx, 0.002, 0.950), 0.058),
        ]
        prev = f'sh.{s}'
        for name, co, r in arm:
            n(name, co, r, prev)
            prev = name
        # leg: hip → knee → ankle
        n(f'hip.{s}', (0.098 * sx, 0.012, 0.905), 0.118, 'pelvis')
        leg = [
            (f'th0.{s}', (0.106 * sx, 0.010, 0.800), 0.114),
            (f'th1.{s}', (0.114 * sx, 0.006, 0.680), 0.104),
            (f'th2.{s}', (0.119 * sx, 0.003, 0.600), 0.097),
            (f'kn0.{s}', (0.121 * sx, 0.001, 0.560), 0.094),
            (f'kn1.{s}', (0.123 * sx, 0.000, 0.515), 0.093),
            (f'kn2.{s}', (0.125 * sx, 0.003, 0.470), 0.091),
            (f'sh0.{s}', (0.127 * sx, 0.008, 0.400), 0.088),
            (f'sh1.{s}', (0.130 * sx, 0.014, 0.290), 0.082),
            (f'an.{s}', (0.132 * sx, 0.018, 0.190), 0.078),
        ]
        prev = f'hip.{s}'
        for name, co, r in leg:
            n(name, co, r, prev)
            prev = name

    names = list(nodes)
    bm = bmesh.new()
    verts = {k: bm.verts.new(nodes[k][0]) for k in names}
    for a, b in edges:
        bm.edges.new((verts[a], verts[b]))
    me = bpy.data.meshes.new('BodySkin')
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new('BodySkin', me)
    link(ob)
    skin = ob.modifiers.new('Skin', 'SKIN')
    skin.use_smooth_shade = True
    skin.branch_smoothing = 0.6
    for i, k in enumerate(names):
        sv = me.skin_vertices[0].data[i]
        sv.radius = nodes[k][1]
        sv.use_root = k == 'pelvis'
    sub = ob.modifiers.new('Sub', 'SUBSURF')
    sub.levels = sub.render_levels = 3

    dg = bpy.context.evaluated_depsgraph_get()
    body_me = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
    bpy.data.objects.remove(ob)
    body = bpy.data.objects.new('Body', body_me)
    link(body)
    body_me.shade_smooth()

    # --- convolutes (pressure bellows) at elbows and knees + material bands -------------
    bm = bmesh.new()
    bm.from_mesh(body_me)
    joints = []
    for s, sx in (('L', 1), ('R', -1)):
        joints.append((nodes[f'el0.{s}'][0], nodes[f'el2.{s}'][0], 4, 0.0065))
        joints.append((nodes[f'kn0.{s}'][0], nodes[f'kn2.{s}'][0], 5, 0.0075))
    for a, b, ridges, amp in joints:
        axis = (b - a)
        length = axis.length
        axis.normalize()
        pad = 0.012
        for v in bm.verts:
            d = v.co - a
            t = d.dot(axis)
            if t < -pad or t > length + pad:
                continue
            radial = d - axis * t
            dist = radial.length
            if dist < 0.04 or dist > 0.14:
                continue
            u = (t + pad) / (length + 2 * pad)
            env = math.sin(math.pi * min(max(u, 0), 1)) ** 0.6
            bump = 0.5 - 0.5 * math.cos(2 * math.pi * ridges * u)
            v.co += radial.normalized() * (amp * env * (bump * 1.3 - 0.3))
    bm.to_mesh(body_me)
    bm.free()

    body_me.materials.append(mats['SuitBody'])
    body_me.materials.append(mats['SuitStripe'])
    # ID stripes on upper arms and thighs: cut clean edge loops, then assign the stripe material
    bands = []
    for s_, sx in (('L', 1), ('R', -1)):
        bands.append((nodes[f'ua0.{s_}'][0], nodes[f'ua2.{s_}'][0], 0.40, 0.62, 0.11))
        bands.append((nodes[f'th0.{s_}'][0], nodes[f'th2.{s_}'][0], 0.30, 0.55, 0.135))
    bm = bmesh.new()
    bm.from_mesh(body_me)
    for a, b, t0, t1, rmax in bands:
        ax = b - a
        axn = ax.normalized()

        def near(co):
            t = (co - a).dot(ax) / ax.length_squared
            return -0.3 < t < 1.3 and (co - (a + ax * t)).length < rmax

        for tc in (t0, t1):
            faces = [f for f in bm.faces if all(near(v.co) for v in f.verts)]
            geom = list({e for f in faces for e in f.edges}) + faces + list({v for f in faces for v in f.verts})
            bmesh.ops.bisect_plane(bm, geom=geom, plane_co=a + ax * tc, plane_no=axn, dist=0.0005)
        is_arm = rmax < 0.12
        for f in bm.faces:
            c = f.calc_center_median()
            t = (c - a).dot(ax) / ax.length_squared
            in_torso = is_arm and (c.x / 0.238) ** 2 + ((c.y - 0.016) / 0.172) ** 2 < 1.0
            if t0 < t < t1 and near(c) and not in_torso and abs(f.normal.dot(axn)) < 0.6:
                f.material_index = 1
    bm.to_mesh(body_me)
    bm.free()
    return body, nodes


# --------------------------------------------------------------------------------------
# Hard parts
# --------------------------------------------------------------------------------------

def ring(name, center, axis, r_in, r_out, width, mat, bevel=0.004, segs=56):
    """Bearing / seal ring: rounded rectangular cross-section revolved around axis."""
    h = width / 2
    prof = rounded_profile([(r_in, -h), (r_out, -h), (r_out, h), (r_in, h)], bevel, 2, closed=True)
    bm = lathe(prof, max(16, int(segs * 0.75)), closed_profile=True)
    transform_bm(bm, center, axis)
    return mesh_object(name, bm, mat)


def build_helmet(mats):
    parts = []
    c = HELMET_C
    fwd = V(0, -1, -0.12).normalized()   # visor looks slightly down

    # EVVA outer shell: sphere with a front opening, open at the neck.
    def shell(radius, open_deg, thickness, mat, name, cut_z, keep_front=False):
        prof = []
        steps = 26
        a0 = math.radians(open_deg)
        for i in range(steps + 1):
            a = a0 + (math.pi - a0) * i / steps if not keep_front else a0 * i / steps
            prof.append((radius * math.sin(a), radius * math.cos(a)))
        prof.reverse()  # run back→front so normals face outward
        bm = lathe(prof, 56)
        transform_bm(bm, c, fwd)
        # cut off the neck opening
        geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
        bmesh.ops.bisect_plane(bm, geom=geom, plane_co=V(0, 0, cut_z), plane_no=V(0, 0, 1), clear_inner=True)
        ob = mesh_object(name, bm, mat)
        sol = ob.modifiers.new('Solid', 'SOLIDIFY')
        sol.thickness = thickness
        sol.offset = -1
        sol.use_rim = True
        sol.use_even_offset = True
        bev = ob.modifiers.new('Bevel', 'BEVEL')
        bev.width = thickness * 0.45
        bev.segments = 2
        bev.limit_method = 'ANGLE'
        bev.angle_limit = math.radians(50)
        ob.modifiers.new('WN', 'WEIGHTED_NORMAL').keep_sharp = True
        return ob

    parts.append(shell(HELMET_R + 0.012, 58, 0.010, mats['Helmet'], 'HelmetShell', c.z - 0.115))
    visor = shell(HELMET_R + 0.004, 64, 0.004, mats['Visor'], 'Visor', c.z - 0.118, keep_front=True)
    parts.append(visor)
    # dark inner bubble so the visor never shows the void behind it
    bub = shell(HELMET_R - 0.004, 0, 0.003, mats['HelmetInner'], 'Bubble', c.z - 0.11)
    bub.modifiers.clear()
    parts.append(bub)

    # visor pivots on the sides
    for sx in (1, -1):
        p = c + V(sx * (HELMET_R + 0.016), 0.012, -0.005)
        prof = rounded_profile([(0.0, -0.007), (0.026, -0.007), (0.026, 0.006), (0.0, 0.006)], 0.004, 3)
        bm = lathe(prof, 32)
        transform_bm(bm, p, V(sx, 0, 0))
        parts.append(mesh_object('Pivot', bm, mats['Helmet']))
        prof = rounded_profile([(0.0, 0.004), (0.012, 0.004), (0.012, 0.012), (0.0, 0.012)], 0.003, 2)
        bm = lathe(prof, 24)
        transform_bm(bm, p, V(sx, 0, 0))
        parts.append(mesh_object('PivotCap', bm, mats['HelmetDark']))

    # helmet lights (EHIP): forward-facing lamp housings at the top corners + HD camera on the right
    for sx in (1, -1):
        base = c + V(sx * 0.158, -0.045, 0.078)
        axis = V(-sx * 0.1, -1, -0.08).normalized()
        prof = rounded_profile([(0.0, -0.06), (0.03, -0.06), (0.034, 0.02), (0.031, 0.04), (0.0, 0.04)], 0.008, 3)
        bm = lathe(prof, 40)
        transform_bm(bm, base, axis)
        parts.append(mesh_object('LampHousing', bm, mats['Helmet']))
        prof = [(0.026, 0.038), (0.025, 0.041), (0.0, 0.043)]
        bm = lathe(prof, 40)
        transform_bm(bm, base, axis)
        parts.append(mesh_object('Lamp', bm, mats['Lamp']))
        parts.append(ring('LampBezel', base + axis * 0.038, axis, 0.025, 0.0318, 0.01, mats['HelmetDark'], 0.0025, 40))
        # mount bracket down to the visor pivot
        pts = [base + V(sx * 0.004, 0.04, -0.012), base + V(sx * 0.012, 0.07, -0.045), c + V(sx * 0.18, 0.03, 0.012)]
        parts.append(mesh_object('LampArm', sweep(catmull_rom(pts, 10), lambda s: 0.0085, 12), mats['HelmetDark']))
    cam = rounded_box(0.036, 0.062, 0.032, 0.007)
    transform_bm(cam, c + V(-0.098, -0.03, 0.172), V(0, 0, 1))
    parts.append(mesh_object('Camera', cam, mats['HelmetDark']))
    prof = [(0.0, 0.0), (0.01, 0.0), (0.012, 0.012), (0.0095, 0.013), (0.0, 0.009)]
    bm = lathe(prof, 24)
    transform_bm(bm, c + V(-0.098, -0.06, 0.172), V(0, -1, 0))
    parts.append(mesh_object('CamLens', bm, mats['HelmetDark']))

    # neck ring (helmet ↔ torso), anodised
    parts.append(ring('NeckRing', V(0, 0.01, c.z - 0.13), V(0, 0, 1), 0.118, 0.17, 0.062, mats['SuitHard'], 0.008, 72))
    parts.append(ring('NeckRingBand', V(0, 0.01, c.z - 0.13), V(0, 0, 1), 0.164, 0.17, 0.012, mats['AnoBlue'], 0.002, 72))
    return parts


def build_plss(mats):
    parts = []
    center = V(0, 0.262, 1.335)
    # main pack, slightly curved to hug the back
    bm = rounded_box(0.47, 0.20, 0.62, 0.045, 4)
    for v in bm.verts:
        v.co.y += 0.028 * (1 - (v.co.x / 0.235) ** 2) * (1 if v.co.y > 0 else 0.4)
    transform_bm(bm, center, V(0, 0, 1))
    parts.append(mesh_object('PLSS', bm, mats['SuitHard']))
    # raised service panels on the back face
    for (px, pz, w, h) in ((0, 0.13, 0.36, 0.22), (-0.09, -0.14, 0.17, 0.2), (0.09, -0.14, 0.17, 0.2)):
        bm = rounded_box(w, 0.02, h, 0.008, 2)
        transform_bm(bm, center + V(px, 0.118 + 0.028 * (1 - (px / 0.235) ** 2), pz), V(0, 0, 1))
        parts.append(mesh_object('PLSSPanel', bm, mats['SuitHard']))
    # vent grilles
    for i in range(7):
        bm = rounded_box(0.075, 0.012, 0.007, 0.003, 2)
        transform_bm(bm, center + V(0.09, 0.136, -0.05 - i * 0.017), V(0, 0, 1))
        parts.append(mesh_object('Vent', bm, mats['MetalDark']))
    # secondary oxygen pack below
    bm = rounded_box(0.40, 0.17, 0.13, 0.035, 4)
    transform_bm(bm, center + V(0, -0.01, -0.39), V(0, 0, 1))
    parts.append(mesh_object('SOP', bm, mats['SuitHard']))
    # top handles
    for sx in (1, -1):
        pts = [center + V(sx * 0.16, 0.02, 0.305), center + V(sx * 0.16, 0.04, 0.345),
               center + V(sx * 0.08, 0.045, 0.352), center + V(sx * 0.02, 0.04, 0.345)]
        parts.append(mesh_object('Handle', sweep(catmull_rom(pts, 8), lambda s: 0.009, 12), mats['MetalDark']))
    # antenna stub
    prof = rounded_profile([(0.0, 0.0), (0.012, 0.0), (0.008, 0.07), (0.0, 0.075)], 0.003, 2)
    bm = lathe(prof, 20)
    transform_bm(bm, center + V(0.17, 0.05, 0.31), V(0.15, 0.2, 1).normalized())
    parts.append(mesh_object('Antenna', bm, mats['MetalDark']))
    # side connectors
    for sx in (1, -1):
        p = center + V(sx * 0.238, -0.03, -0.12)
        parts.append(ring('PLSSPort', p, V(sx, 0, 0), 0.0, 0.024, 0.02, mats['Metal'], 0.004, 32))
    return parts


def build_dcm(mats):
    """Display & Control Module on the chest, plus umbilical hoses to the PLSS."""
    parts = []
    tilt = Matrix.Rotation(math.radians(-12), 3, 'X')
    center = V(0, -0.196, 1.29)

    def place(bm, local):
        for v in bm.verts:
            v.co = center + tilt @ (v.co + local)

    body = rounded_box(0.30, 0.095, 0.15, 0.022, 4)
    place(body, V(0, 0, 0))
    parts.append(mesh_object('DCM', body, mats['SuitHard']))
    # display window on the top face
    disp = rounded_box(0.14, 0.05, 0.012, 0.004, 2)
    place(disp, V(0.0, -0.005, 0.073))
    parts.append(mesh_object('DCMDisplay', disp, mats['Display']))
    # knobs on the front face
    for i, (kx, kz, kr) in enumerate(((-0.1, 0.02, 0.016), (-0.05, 0.02, 0.013), (0.05, 0.02, 0.013), (0.1, 0.02, 0.016),
                                      (-0.075, -0.035, 0.011), (0.075, -0.035, 0.011))):
        prof = rounded_profile([(0.0, 0.0), (kr, 0.0), (kr, 0.014), (kr * 0.85, 0.02), (0.0, 0.02)], 0.003, 2)
        bm = lathe(prof, 28)
        # ribbed grip
        for v in bm.verts:
            r = math.hypot(v.co.x, v.co.y)
            if r > kr * 0.5 and 0.002 < v.co.z < 0.015:
                a = math.atan2(v.co.y, v.co.x)
                k = 1 + 0.06 * math.cos(a * 14)
                v.co.x *= k
                v.co.y *= k
        transform_bm(bm, V(0, 0, 0), V(0, -1, 0))
        place(bm, V(kx, -0.047, kz))
        parts.append(mesh_object('Knob', bm, mats['MetalDark'] if i < 4 else mats['AnoRed']))
    # side connectors + umbilical hoses back to the PLSS
    for sx in (1, -1):
        conn = lathe(rounded_profile([(0.0, 0.0), (0.02, 0.0), (0.02, 0.035), (0.0, 0.035)], 0.004, 2), 28)
        transform_bm(conn, V(0, 0, 0), V(sx, 0, 0))
        place(conn, V(sx * 0.15, 0.0, -0.01))
        parts.append(mesh_object('DCMPort', conn, mats['Metal']))
        start = center + tilt @ V(sx * 0.185, 0.0, -0.01)
        pts = [start, start + V(sx * 0.05, 0.01, -0.03), V(sx * 0.255, -0.12, 1.19), V(sx * 0.262, 0.05, 1.17),
               V(sx * 0.25, 0.2, 1.2)]
        hose = sweep(catmull_rom(pts, 34), lambda s: 0.0135 * (1 + 0.075 * math.cos(s * 2 * math.pi / 0.016)), 10)
        parts.append(mesh_object('Hose', hose, mats['Hose']))
    # mounting brackets
    for sx in (1, -1):
        bm = rounded_box(0.03, 0.05, 0.08, 0.008, 2)
        transform_bm(bm, V(sx * 0.11, -0.155, 1.30), V(0, 0, 1))
        parts.append(mesh_object('DCMBracket', bm, mats['MetalDark']))
    return parts


def build_glove(mats, side, sx):
    """Pressurised EVA glove in hand-local space, then placed along the hand bone."""
    nodes, edges = {}, []

    def n(name, co, r, parent=None):
        nodes[name] = (V(*co), r if isinstance(r, tuple) else (r, r))
        if parent:
            edges.append((parent, name))

    # local frame: Z up the forearm (hand hangs toward -Z), X toward the thumb (forward), Y = palm normal
    n('wrist', (0, 0, 0.0), 0.054)
    n('palm0', (0, 0, -0.04), (0.057, 0.036), 'wrist')
    n('palm1', (0, 0.002, -0.084), (0.056, 0.032), 'palm0')
    n('knuck', (0, 0.004, -0.108), (0.053, 0.027), 'palm1')
    finger_x = (0.037, 0.0125, -0.0125, -0.036)
    finger_len = (0.094, 0.104, 0.1, 0.082)
    tips = []
    for i, (fx, fl) in enumerate(zip(finger_x, finger_len)):
        seg = [0.42, 0.32, 0.26]
        curl = [math.radians(a) for a in (38, 62, 46)]  # relaxed grasp (pressurised gloves rest half-closed)
        p = V(fx, 0.004, -0.108)
        d = V(fx * 0.1, 0, -1).normalized()
        prev = 'knuck'
        ang = 0
        for j in range(3):
            ang += curl[j]
            dd = Matrix.Rotation(ang, 3, 'X') @ d  # curl toward the palm (+Y)
            p = p + dd * fl * seg[j]
            name = f'f{i}{j}'
            n(name, tuple(p), 0.0152 - j * 0.0013, prev)
            prev = name
        tips.append(name)
    p = V(0.05, 0.014, -0.034)
    n('th0', tuple(p), 0.02, 'palm0')
    for j, (dv, ln) in enumerate(((V(0.5, 0.45, -0.7), 0.046), (V(0.1, 0.75, -0.6), 0.038),
                                  (V(-0.2, 0.8, -0.4), 0.03))):
        p = p + dv.normalized() * ln
        n(f'th{j + 1}', tuple(p), 0.017 - j * 0.0013, f'th{j}')

    gme = skin_mesh('GloveSkin', nodes, edges, 'wrist', levels=2, branch=0.8)
    gme.materials.append(mats['Glove'])
    gme.materials.append(mats['GloveGrip'])
    tip_pts = [nodes[t][0] for t in tips] + [nodes['th3'][0]]
    for poly in gme.polygons:
        near_tip = min((poly.center - t).length for t in tip_pts) < 0.02
        palm = poly.center.z < -0.02 and poly.normal.y > 0.55
        if near_tip or palm:
            poly.material_index = 1
    glove = bpy.data.objects.new(f'Glove.{side}', gme)
    link(glove)
    # gauntlet cuff
    prof = rounded_profile([(0.056, -0.03), (0.062, 0.02), (0.07, 0.07), (0.074, 0.09), (0.067, 0.094), (0.058, 0.06),
                            (0.052, -0.02)], 0.006, 3, closed=True)
    cuff = lathe(prof, 44, closed_profile=True)
    parts = [glove, mesh_object(f'Cuff.{side}', cuff, mats['Glove'])]
    # back-of-hand thermal pad
    pad = rounded_box(0.07, 0.016, 0.06, 0.007, 2)
    transform_bm(pad, V(0.0, -0.03, -0.06), V(0, 0, 1))
    parts.append(mesh_object(f'GlovePad.{side}', pad, mats['Glove']))

    h, t = bone_axis(f'hand.{side}')
    down = (t - h).normalized()
    z = -down
    x = V(0, -1, 0)
    x = (x - z * x.dot(z)).normalized()
    y = z.cross(x)
    m = Matrix((x, y, z)).transposed().to_4x4()
    m.translation = h + down * 0.012
    for ob in parts:
        if sx < 0:
            ob.data.transform(Matrix.Scale(-1, 4, V(0, 1, 0)))
            ob.data.flip_normals()
        ob.data.transform(m)
    return parts


def skin_mesh(name, nodes, edges, root, levels=2, branch=0.8):
    """Skin-modifier graph → subdivided mesh data (modifiers applied)."""
    names = list(nodes)
    bm = bmesh.new()
    verts = {k: bm.verts.new(nodes[k][0]) for k in names}
    for a, b in edges:
        bm.edges.new((verts[a], verts[b]))
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new(name, me)
    link(ob)
    sk = ob.modifiers.new('Skin', 'SKIN')
    sk.use_smooth_shade = True
    sk.branch_smoothing = branch
    for i, k in enumerate(names):
        me.skin_vertices[0].data[i].radius = nodes[k][1]
        me.skin_vertices[0].data[i].use_root = k == root
    sub = ob.modifiers.new('Sub', 'SUBSURF')
    sub.levels = levels
    dg = bpy.context.evaluated_depsgraph_get()
    out = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
    bpy.data.objects.remove(ob)
    bpy.data.meshes.remove(me)
    out.shade_smooth()
    return out


def build_boot(mats, side, sx):
    """Bulky lunar boot: treaded sole, padded upper to mid-calf, rubber toe/heel caps, strap."""
    parts = []
    bx = 0.133 * sx
    L, W = 0.33, 0.138
    cy = -0.03
    outline = []
    for i in range(72):
        a = 2 * math.pi * i / 72
        x, y = math.cos(a), math.sin(a)
        k = 2.5
        px = math.copysign(abs(x) ** (2 / k), x) * W / 2 * (1.0 - 0.1 * y)
        py = math.copysign(abs(y) ** (2 / k), y) * L / 2
        outline.append(V(px, py, 0))
    bm = bmesh.new()
    bottom = [bm.verts.new(p) for p in outline]
    f = bm.faces.new(bottom)
    ext = bmesh.ops.extrude_face_region(bm, geom=[f])
    top = [e for e in ext['geom'] if isinstance(e, bmesh.types.BMVert)]
    bmesh.ops.translate(bm, verts=top, vec=V(0, 0, 0.046))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    rim = [e for e in bm.edges if len(e.link_faces) == 2 and e.calc_face_angle(0) > 0.6]
    bmesh.ops.bevel(bm, geom=rim, offset=0.009, segments=3, affect='EDGES', profile=0.5, clamp_overlap=True)
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    transform_bm(bm, V(bx, cy, 0.0), V(0, 0, 1))
    parts.append(mesh_object(f'Sole.{side}', bm, mats['Rubber']))
    # tread lugs (chevrons)
    for iy in range(-7, 8):
        y = iy * 0.02
        if abs(y) > L / 2 - 0.018:
            continue
        half = W / 2 * (1 - (abs(y) / (L / 2)) ** 3) - 0.016
        for ix in (-1, 1):
            if half < 0.02:
                continue
            lug = rounded_box(half * 0.8, 0.009, 0.008, 0.0, 1)
            rot = Matrix.Rotation(math.radians(18 * ix), 4, 'Z')
            lug.transform(rot)
            transform_bm(lug, V(bx + ix * half * 0.45, cy + y, -0.003), V(0, 0, 1))
            parts.append(mesh_object('Lug', lug, mats['Rubber']))

    nodes = {
        'heel': (V(bx, cy + 0.105, 0.1), (0.066, 0.058)),
        'mid': (V(bx, cy + 0.01, 0.1), (0.07, 0.066)),
        'ball': (V(bx, cy - 0.08, 0.09), (0.068, 0.058)),
        'toe': (V(bx, cy - 0.125, 0.082), (0.06, 0.05)),
        'ankle': (V(bx, 0.03, 0.19), (0.092, 0.09)),
        'calf': (V(bx, 0.026, 0.29), (0.1, 0.1)),
        'top': (V(bx, 0.024, 0.345), (0.104, 0.104)),
    }
    edges = [('heel', 'mid'), ('mid', 'ball'), ('ball', 'toe'), ('mid', 'ankle'), ('ankle', 'calf'), ('calf', 'top')]
    bme = skin_mesh('BootSkin', nodes, edges, 'mid', levels=2, branch=1.0)
    for v in bme.vertices:
        if v.co.z < 0.06:
            v.co.z = 0.046 + (v.co.z - 0.06) * 0.08
    # padded top roll
    for v in bme.vertices:
        if v.co.z > 0.33:
            r = V(v.co.x - bx, v.co.y - 0.024, 0)
            if r.length > 1e-6:
                v.co += r.normalized() * 0.006
    boot = bpy.data.objects.new(f'Boot.{side}', bme)
    link(boot)
    bme.materials.append(mats['Boot'])
    parts.append(boot)
    # rubber toe and heel caps hugging the upper
    for name, cyo, sy in (('ToeCap', -0.13, 1), ('HeelCap', 0.115, -1)):
        prof = []
        for i in range(13):
            a = math.pi / 2 * i / 12
            prof.append((0.066 * math.cos(a) + 0.002, 0.058 * math.sin(a)))
        bm = lathe([(0.0, 0.0)] + prof[:-1] + [(0.0, 0.059)], 40)
        for v in bm.verts:
            v.co.y *= 0.9
        bm.transform(Matrix.Rotation(math.radians(90 * sy), 4, 'X'))
        transform_bm(bm, V(bx, cy + cyo + 0.035 * sy, 0.075), V(0, 0, 1))
        for v in bm.verts:
            if v.co.z < 0.046:
                v.co.z = 0.046
        parts.append(mesh_object(name, bm, mats['Rubber']))
    # strap hugging the measured boot shaft
    zs = 0.215
    rads = [math.hypot(v.co.x - bx, v.co.y - 0.03) for v in bme.vertices if abs(v.co.z - zs) < 0.012]
    rad = sum(rads) / len(rads)
    parts.append(ring('BootStrap', V(bx, 0.03, zs), V(0, 0, 1), rad - 0.004, rad + 0.0045, 0.024,
                      mats['Strap'], 0.003, 56))
    # buckle on the outer-front of the strap, facing outward
    ang = math.atan2(sx * 0.62, -0.78)
    buckle = rounded_box(0.034, 0.012, 0.03, 0.004, 2)
    rot = Matrix.Rotation(-ang, 4, 'Z') if sx > 0 else Matrix.Rotation(ang, 4, 'Z')
    buckle.transform(rot)
    transform_bm(buckle, V(bx + sx * rad * 0.62 * 1.04, 0.03 - rad * 0.78 * 1.04, zs), V(0, 0, 1))
    parts.append(mesh_object('Buckle', buckle, mats['Metal']))
    return parts


def build_details(mats, nodes):
    parts = []
    # bearings: scye (shoulder), wrist, waist
    for s, sx in (('L', 1), ('R', -1)):
        a, b = nodes[f'sh.{s}'][0], nodes[f'ua1.{s}'][0]
        axis = (b - a).normalized()
        parts.append(ring('ScyeBearing', a + axis * 0.05, axis, 0.084, 0.1, 0.03, mats['SuitHard'], 0.007, 64))
        a, b = nodes[f'fa1.{s}'][0], nodes[f'wr.{s}'][0]
        axis = (b - a).normalized()
        parts.append(ring('WristRing', b + axis * 0.01, axis, 0.052, 0.068, 0.03, mats['Metal'], 0.005, 56))
        parts.append(ring('WristBand', b - axis * 0.002, axis, 0.066, 0.0705, 0.012,
                          mats['AnoBlue'] if sx > 0 else mats['AnoRed'], 0.002, 56))
    waist = ring('WaistRing', V(0, 0.012, 1.105), V(0, 0.02, 1).normalized(), 0.176, 0.192, 0.03, mats['SuitHard'],
                 0.008, 72)
    for v in waist.data.vertices:
        v.co.y = 0.012 + (v.co.y - 0.012) * 0.79
    parts.append(waist)
    # D-rings on the waist ring for tethers
    for sx in (1, -1):
        dr = ring('DRing', V(sx * 0.165, -0.085, 1.075), V(sx * 0.5, -0.86, 0).normalized(), 0.008, 0.014, 0.004,
                  mats['Metal'], 0.0015, 24)
        parts.append(dr)
    # cuff checklist on the left forearm
    a, b = nodes['fa0.L'][0], nodes['fa1.L'][0]
    mid = (a + b) / 2
    axis = (b - a).normalized()
    out = V(0, -1, 0)
    out = (out - axis * out.dot(axis)).normalized()
    bm = rounded_box(0.074, 0.016, 0.085, 0.006, 2)
    for v in bm.verts:
        v.co.y += 0.018 * (v.co.x / 0.037) ** 2
    xax = axis.cross(out)
    m = Matrix((xax, -out, -axis)).transposed().to_4x4()
    m.translation = mid + out * 0.062
    bm.transform(m)
    parts.append(mesh_object('Checklist', bm, mats['SuitHard']))
    # thigh pocket (right), hugging the leg
    a, b = nodes['th0.R'][0], nodes['th2.R'][0]
    mid = a.lerp(b, 0.62)
    out = V(-0.55, -0.83, 0).normalized()
    m = Matrix.Rotation(math.atan2(-out.x, out.y) + math.pi, 4, 'Z')
    bm = rounded_box(0.11, 0.035, 0.13, 0.013, 3)
    for v in bm.verts:
        v.co.y += 0.03 * (v.co.x / 0.055) ** 2
    m.translation = mid + out * 0.098
    bm.transform(m)
    parts.append(mesh_object('Pocket', bm, mats['SuitFabric']))
    flap = rounded_box(0.116, 0.016, 0.044, 0.007, 2)
    for v in flap.verts:
        v.co.y += 0.03 * (v.co.x / 0.058) ** 2
    m.translation = mid + out * 0.114 + V(0, 0, 0.048)
    flap.transform(m)
    parts.append(mesh_object('PocketFlap', flap, mats['SuitFabric']))
    # mission patch on the left upper arm
    a, b = nodes['ua1.L'][0], nodes['ua2.L'][0]
    p = a.lerp(b, 0.9)
    out = V(0.75, -0.66, 0).normalized()
    prof = rounded_profile([(0.0, 0.0), (0.034, 0.0), (0.034, 0.003), (0.0, 0.0035)], 0.0015, 2)
    bm = lathe(prof, 40)
    uv = bm.loops.layers.uv.new('UVMap')
    for f in bm.faces:
        for loop in f.loops:
            co = loop.vert.co
            loop[uv].uv = (0.5 + co.x / 0.07, 0.5 + co.y / 0.07)
    transform_bm(bm, p + out * 0.079, out)
    parts.append(mesh_object('Patch', bm, mats['Patch']))
    return parts


# --------------------------------------------------------------------------------------
# Weights
# --------------------------------------------------------------------------------------

def rigid_bind(ob, rig, bone):
    ob.vertex_groups.clear()
    vg = ob.vertex_groups.new(name=bone)
    vg.add(list(range(len(ob.data.vertices))), 1.0, 'REPLACE')
    ob.parent = rig
    mod = ob.modifiers.new('Armature', 'ARMATURE')
    mod.object = rig


def gradient_bind(ob, rig, bone_a, bone_b, fn):
    """weight_b = fn(vertex co) in [0, 1]; rest goes to bone_a."""
    ob.vertex_groups.clear()
    ga = ob.vertex_groups.new(name=bone_a)
    gb = ob.vertex_groups.new(name=bone_b)
    for v in ob.data.vertices:
        w = min(max(fn(v.co), 0.0), 1.0)
        if w < 1:
            ga.add([v.index], 1 - w, 'REPLACE')
        if w > 0:
            gb.add([v.index], w, 'REPLACE')
    ob.parent = rig
    mod = ob.modifiers.new('Armature', 'ARMATURE')
    mod.object = rig


def auto_bind(ob, rig):
    for o in bpy.context.scene.objects:
        o.select_set(False)
    ob.select_set(True)
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.parent_set(type='ARMATURE_AUTO')
    ob.select_set(False)
    rig.select_set(False)


def smoothstep(e0, e1, x):
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


# --------------------------------------------------------------------------------------
# Assembly
# --------------------------------------------------------------------------------------

def build(args):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    mats = make_materials()
    rig = build_armature()

    body, nodes = build_body(mats)
    if not args.no_detail:
        from suitdetail import bake_detail_normals, body_segments
        bake_detail_normals(body, body_segments(nodes), size=args.tex_size,
                            image_path=args.debug_dir and f'{args.debug_dir}/body_normal.png')
    auto_bind(body, rig)

    for ob in build_helmet(mats) + build_plss(mats) + build_dcm(mats):
        rigid_bind(ob, rig, 'chest')

    for s, sx in (('L', 1), ('R', -1)):
        h, t = bone_axis(f'hand.{s}')
        for ob in build_glove(mats, s, sx):
            if ob.name.startswith('Cuff'):
                gradient_bind(ob, rig, f'forearm.{s}', f'hand.{s}',
                              lambda co, h=h, t=t: smoothstep(-0.03, 0.02, (co - h).dot((t - h).normalized())))
            else:
                rigid_bind(ob, rig, f'hand.{s}')
        foot_h, foot_t = bone_axis(f'foot.{s}')
        for ob in build_boot(mats, s, sx):
            if ob.name.startswith('Boot.'):
                gradient_bind(ob, rig, f'foot.{s}', f'shin.{s}', lambda co: smoothstep(0.13, 0.22, co.z))
            elif ob.name.startswith('BootStrap') or ob.name.startswith('Buckle'):
                rigid_bind(ob, rig, f'shin.{s}')
            elif ob.name.startswith(('Sole', 'Lug', 'HeelCap')):
                rigid_bind(ob, rig, f'foot.{s}')
            else:
                # the toe box bends with the toe bone
                gradient_bind(ob, rig, f'foot.{s}', f'toe.{s}',
                              lambda co, ft=foot_t: smoothstep(0.0, 0.04, -(co.y - ft.y)))

    for ob in build_details(mats, nodes):
        c = sum((v.co for v in ob.data.vertices), V(0, 0, 0)) / max(1, len(ob.data.vertices))
        name = ob.name
        if name.startswith('ScyeBearing'):
            bone = 'upperarm.L' if c.x > 0 else 'upperarm.R'
        elif name.startswith('Wrist'):
            bone = 'forearm.L' if c.x > 0 else 'forearm.R'
        elif name.startswith('Checklist'):
            bone = 'forearm.L'
        elif name.startswith('Pocket'):
            bone = 'thigh.R'
        elif name.startswith('Patch'):
            bone = 'upperarm.L'
        elif name.startswith(('WaistRing', 'DRing')):
            bone = 'spine'
        else:
            bone = 'chest'
        rigid_bind(ob, rig, bone)

    # apply modifiers except the armature (solidify/bevel etc.)
    dg = bpy.context.evaluated_depsgraph_get()
    for ob in [o for o in bpy.context.scene.objects if o.type == 'MESH']:
        extra = [m for m in ob.modifiers if m.type != 'ARMATURE']
        if not extra:
            continue
        arm_mods = [m for m in ob.modifiers if m.type == 'ARMATURE']
        for m in arm_mods:
            m.show_viewport = False
        dg = bpy.context.evaluated_depsgraph_get()
        me = bpy.data.meshes.new_from_object(ob.evaluated_get(dg), preserve_all_data_layers=True, depsgraph=dg)
        for m in extra:
            ob.modifiers.remove(m)
        old = ob.data
        ob.data = me
        bpy.data.meshes.remove(old)
        for m in arm_mods:
            m.show_viewport = True

    merge_by_material(rig)
    report(rig)
    return rig


def merge_by_material(rig):
    groups = {}
    for ob in [o for o in bpy.context.scene.objects if o.type == 'MESH']:
        if len(ob.data.materials) != 1:
            groups.setdefault(ob.name, []).append(ob)   # multi-material meshes stay alone
            continue
        groups.setdefault(ob.data.materials[0].name, []).append(ob)
    for key, obs in groups.items():
        if len(obs) < 2:
            obs[0].name = key
            continue
        for o in bpy.context.scene.objects:
            o.select_set(False)
        for o in obs:
            o.select_set(True)
        bpy.context.view_layer.objects.active = obs[0]
        with bpy.context.temp_override(active_object=obs[0], selected_editable_objects=obs, selected_objects=obs):
            bpy.ops.object.join()
        obs[0].name = key


def report(rig):
    total = 0
    for ob in [o for o in bpy.context.scene.objects if o.type == 'MESH']:
        me = ob.data
        me.calc_loop_triangles()
        tris = len(me.loop_triangles)
        total += tris
        print(f'  {ob.name:24s} {tris:7d} tris  mats={[m.name for m in me.materials]}')
    print(f'  TOTAL {total} tris')


def main():
    argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else sys.argv[1:]
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', default='public/assets/astronaut.glb')
    ap.add_argument('--preview', default=None)
    ap.add_argument('--pose', action='store_true', help='pose the rig in the preview to check deformation')
    ap.add_argument('--no-bake', action='store_true', help='skip the AO vertex-colour bake')
    ap.add_argument('--no-detail', action='store_true', help='skip the fabric normal-map bake')
    ap.add_argument('--tex-size', type=int, default=2048)
    ap.add_argument('--debug-dir', default=None)
    args = ap.parse_args(argv)
    rig = build(args)
    import suitkit
    if not args.no_bake:
        suitkit.bake_ao()
    if args.preview:
        suitkit.render_preview(rig, args.preview, pose=args.pose)
    suitkit.export_glb(args.out)


if __name__ == '__main__':
    main()
