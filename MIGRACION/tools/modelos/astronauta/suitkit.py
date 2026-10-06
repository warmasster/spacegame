"""Small procedural-modelling toolkit on top of bpy/bmesh used by the asset scripts."""

import math
import os

import bpy  # noqa: I001  (bpy must be imported before bmesh)
import bmesh
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))


def link(ob):
    bpy.context.scene.collection.objects.link(ob)
    return ob


def transform_bm(bm, origin, axis):
    """Rotate so local +Z points along `axis`, then move to `origin`."""
    axis = Vector(axis).normalized()
    rot = Vector((0, 0, 1)).rotation_difference(axis).to_matrix().to_4x4()
    rot.translation = Vector(origin)
    bm.transform(rot)
    return bm


def mesh_object(name, bm, mat=None, smooth=True):
    me = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    if smooth:
        me.shade_smooth()
    ob = bpy.data.objects.new(name, me)
    link(ob)
    if mat is not None:
        me.materials.append(mat)
    return ob


def rounded_profile(points, radius, segs=3, closed=False):
    """Fillet the corners of a 2D polyline [(r, z), ...] with quadratic arcs."""
    pts = [Vector(p) for p in points]
    n = len(pts)
    out = []
    for i in range(n):
        if not closed and (i == 0 or i == n - 1):
            out.append(pts[i])
            continue
        p0, p1, p2 = pts[(i - 1) % n], pts[i], pts[(i + 1) % n]
        d0, d1 = (p0 - p1), (p2 - p1)
        l0, l1 = d0.length, d1.length
        if l0 < 1e-9 or l1 < 1e-9:
            out.append(p1)
            continue
        cosang = max(-1.0, min(1.0, d0.dot(d1) / (l0 * l1)))
        ang = math.acos(cosang)
        if ang > math.radians(175):
            out.append(p1)
            continue
        t = radius / math.tan(ang / 2)
        t = min(t, 0.45 * l0, 0.45 * l1)
        a = p1 + d0.normalized() * t
        b = p1 + d1.normalized() * t
        for k in range(segs + 1):
            u = k / segs
            out.append((1 - u) ** 2 * a + 2 * (1 - u) * u * p1 + u ** 2 * b)
    return [(p.x, p.y) for p in out]


def lathe(profile, segs=48, closed_profile=False):
    """Revolve [(r, z)] around +Z. Open profiles should run bottom→out→top for outward normals."""
    bm = bmesh.new()
    rings = []
    for r, z in profile:
        ring = []
        for i in range(segs):
            a = 2 * math.pi * i / segs
            ring.append(bm.verts.new((r * math.cos(a), r * math.sin(a), z)))
        rings.append(ring)
    count = len(rings) if closed_profile else len(rings) - 1
    for j in range(count):
        r0, r1 = rings[j], rings[(j + 1) % len(rings)]
        for i in range(segs):
            quad = (r0[i], r0[(i + 1) % segs], r1[(i + 1) % segs], r1[i])
            try:
                bm.faces.new(quad)
            except ValueError:
                pass
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    # drop faces collapsed at the axis
    bad = [f for f in bm.faces if f.calc_area() < 1e-12]
    bmesh.ops.delete(bm, geom=bad, context='FACES')
    if closed_profile:
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return bm


def rounded_box(sx, sy, sz, radius, segs=3):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=(sx, sy, sz), verts=bm.verts)
    radius = min(radius, 0.49 * min(sx, sy, sz))
    if radius > 0:
        bmesh.ops.bevel(bm, geom=list(bm.edges), offset=radius, segments=segs, profile=0.5, affect='EDGES',
                        clamp_overlap=True)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return bm


def catmull_rom(ctrl, samples=8):
    pts = [Vector(p) for p in ctrl]
    ext = [pts[0] + (pts[0] - pts[1])] + pts + [pts[-1] + (pts[-1] - pts[-2])]
    out = []
    for i in range(1, len(ext) - 2):
        p0, p1, p2, p3 = ext[i - 1], ext[i], ext[i + 1], ext[i + 2]
        for k in range(samples):
            t = k / samples
            t2, t3 = t * t, t * t * t
            out.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 +
                              (-p0 + 3 * p1 - 3 * p2 + p3) * t3))
    out.append(pts[-1])
    return out


def sweep(points, radius_fn, segs=12, caps=True):
    """Tube along a polyline with parallel-transport frames; radius_fn(arc_length)."""
    pts = [Vector(p) for p in points]
    n = len(pts)
    tangents = []
    for i in range(n):
        a = pts[max(i - 1, 0)]
        b = pts[min(i + 1, n - 1)]
        tangents.append((b - a).normalized())
    ref = Vector((0, 0, 1)) if abs(tangents[0].z) < 0.9 else Vector((1, 0, 0))
    normal = tangents[0].cross(ref).normalized()
    s = 0.0
    bm = bmesh.new()
    rings = []
    for i in range(n):
        if i > 0:
            s += (pts[i] - pts[i - 1]).length
            q = tangents[i - 1].rotation_difference(tangents[i])
            normal = (q @ normal).normalized()
        binormal = tangents[i].cross(normal)
        r = radius_fn(s)
        ring = []
        for k in range(segs):
            a = 2 * math.pi * k / segs
            ring.append(bm.verts.new(pts[i] + (normal * math.cos(a) + binormal * math.sin(a)) * r))
        rings.append(ring)
    for j in range(n - 1):
        for k in range(segs):
            bm.faces.new((rings[j][k], rings[j][(k + 1) % segs], rings[j + 1][(k + 1) % segs], rings[j + 1][k]))
    if caps:
        for ring, p in ((rings[0], pts[0]), (rings[-1], pts[-1])):
            c = bm.verts.new(p)
            for k in range(segs):
                bm.faces.new((ring[k], ring[(k + 1) % segs], c))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return bm


# --------------------------------------------------------------------------------------
# Materials (names are read by the game client)
# --------------------------------------------------------------------------------------

def _principled(name, color, rough, metal=0.0, emission=None, strength=0.0):
    m = bpy.data.materials.new(name)
    if m.node_tree is None or 'Principled BSDF' not in m.node_tree.nodes:
        m.use_nodes = True          # before Blender 5 a new material has no nodes
    # what the solid view (and the pose pictures) shows
    m.diffuse_color = (*color, 1.0)
    m.roughness = rough
    m.metallic = metal
    p = m.node_tree.nodes['Principled BSDF']
    p.inputs['Base Color'].default_value = (*color, 1.0)
    p.inputs['Roughness'].default_value = rough
    p.inputs['Metallic'].default_value = metal
    if emission is not None:
        p.inputs['Emission Color'].default_value = (*emission, 1.0)
        p.inputs['Emission Strength'].default_value = strength
    return m


def make_patch_image():
    """Mission patch texture drawn with PIL (original design). Blender's Python has no PIL: the
    picture kept beside this file is used as it is then."""
    path = os.path.join(HERE, 'patch.png')
    try:
        from PIL import Image, ImageDraw, ImageFont
    except ImportError:
        return path
    S = 512
    img = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    c = S / 2
    d.ellipse((4, 4, S - 4, S - 4), fill=(214, 206, 188, 255))            # embroidered border
    d.ellipse((26, 26, S - 26, S - 26), fill=(17, 30, 62, 255))           # deep blue field
    # stars
    import random
    rnd = random.Random(7)
    for _ in range(40):
        x, y = rnd.uniform(70, S - 70), rnd.uniform(60, 300)
        if (x - c) ** 2 + (y - c) ** 2 < (c - 60) ** 2:
            r = rnd.choice((1.5, 2, 2.5, 3.5))
            d.ellipse((x - r, y - r, x + r, y + r), fill=(235, 235, 225, 255))
    # lunar limb (lower part) with craters
    d.pieslice((-140, 300, S + 140, 900), 180, 360, fill=(170, 170, 165, 255))
    for (x, y, r) in ((150, 350, 22), (300, 330, 14), (380, 370, 26), (230, 395, 10), (95, 400, 12)):
        d.ellipse((x - r, y - r * 0.45, x + r, y + r * 0.45), fill=(135, 135, 130, 255))
    # earth
    d.ellipse((318, 110, 388, 180), fill=(52, 104, 190, 255))
    d.chord((318, 110, 388, 180), 90, 270, fill=(14, 22, 44, 255))
    # flight path arc
    d.arc((90, 120, 430, 460), 200, 320, fill=(214, 170, 60, 255), width=6)
    # clip to disc
    mask = Image.new('L', (S, S), 0)
    ImageDraw.Draw(mask).ellipse((26, 26, S - 26, S - 26), fill=255)
    field = img.copy()
    img.paste((214, 206, 188, 255), (0, 0, S, S), Image.eval(mask, lambda v: 255 - v))
    img.paste(field, (0, 0), mask)
    d = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf', 44)
        text = 'SELENE  I'
        w = d.textlength(text, font=font)
        d.text((c - w / 2, 180), text, font=font, fill=(240, 238, 230, 255))
    except OSError:
        pass
    img.save(path)
    return path


def make_materials(patch_texture=False):
    mats = {
        'SuitFabric': _principled('SuitFabric', (0.80, 0.79, 0.76), 0.86),
        'SuitBody': _principled('SuitBody', (0.80, 0.79, 0.76), 0.86),
        'GloveGrip': _principled('GloveGrip', (0.30, 0.31, 0.33), 0.9),
        'SuitStripe': _principled('SuitStripe', (0.55, 0.04, 0.03), 0.8),
        'SuitHard': _principled('SuitHard', (0.83, 0.83, 0.81), 0.42),
        'Helmet': _principled('Helmet', (0.84, 0.84, 0.82), 0.35),
        # the neck ring: the look of SuitHard and AnoBlue, but materials of its own (and of nothing
        # else), so the game can hide it in first person as it hides the helmet
        'NeckRing': _principled('NeckRing', (0.83, 0.83, 0.81), 0.42),
        'NeckRingBand': _principled('NeckRingBand', (0.10, 0.22, 0.60), 0.3, 1.0),
        'HelmetDark': _principled('HelmetDark', (0.1, 0.1, 0.11), 0.35, 1.0),
        'Glove': _principled('Glove', (0.72, 0.72, 0.70), 0.8),
        'Boot': _principled('Boot', (0.74, 0.73, 0.70), 0.82),
        'Rubber': _principled('Rubber', (0.045, 0.045, 0.05), 0.75),
        'Strap': _principled('Strap', (0.10, 0.10, 0.11), 0.7),
        'Hose': _principled('Hose', (0.58, 0.60, 0.62), 0.55),
        'Metal': _principled('Metal', (0.80, 0.80, 0.82), 0.28, 1.0),
        'MetalDark': _principled('MetalDark', (0.12, 0.12, 0.13), 0.38, 1.0),
        'AnoBlue': _principled('AnoBlue', (0.10, 0.22, 0.60), 0.3, 1.0),
        'AnoRed': _principled('AnoRed', (0.60, 0.07, 0.05), 0.3, 1.0),
        'Visor': _principled('Visor', (1.0, 0.76, 0.33), 0.06, 1.0),
        'HelmetInner': _principled('HelmetInner', (0.02, 0.02, 0.02), 0.3),
        'Lamp': _principled('Lamp', (0.9, 0.9, 0.85), 0.1, 0.0, (1.0, 0.96, 0.88), 3.0),
        'Display': _principled('Display', (0.01, 0.012, 0.015), 0.08, 0.0, (0.1, 0.45, 0.3), 0.25),
    }
    if patch_texture:
        patch = _principled('Patch', (1, 1, 1), 0.85)
        img = bpy.data.images.load(make_patch_image())
        img.pack()
        nt = patch.node_tree
        tex = nt.nodes.new('ShaderNodeTexImage')
        tex.image = img
        nt.links.new(tex.outputs['Color'], nt.nodes['Principled BSDF'].inputs['Base Color'])
    else:
        # no picture: the deep blue of its field (the game tints it anyway)
        patch = _principled('Patch', (0.07, 0.12, 0.26), 0.85)
    mats['Patch'] = patch
    return mats


# --------------------------------------------------------------------------------------
# Baking, export
# --------------------------------------------------------------------------------------

def bake_ao(distance=0.22, samples=96, alone=('Glove',), alone_distance=0.05, alone_floor=0.35):
    """Ambient occlusion of the rest pose into the vertex colours (`AO`, what goes out as COLOR_0).

    Objects whose name starts with one of `alone` (the gloves) are baked among themselves, with a short
    reach: a hand goes everywhere, so it must not carry the shade of the thigh it hangs beside, only
    that of its own creases; and never darker than `alone_floor`."""
    sc = bpy.context.scene
    if sc.world is None:
        sc.world = bpy.data.worlds.new('World')
    sc.render.engine = 'CYCLES'
    sc.cycles.device = 'CPU'
    sc.cycles.samples = samples
    sc.render.bake.target = 'VERTEX_COLORS'
    sc.render.bake.use_selected_to_active = False
    meshes = [o for o in sc.objects if o.type == 'MESH']
    for ob in meshes:
        me = ob.data
        attr = me.color_attributes.new('AO', 'BYTE_COLOR', 'POINT')
        me.color_attributes.active_color = attr
        me.color_attributes.render_color_index = me.color_attributes.active_color_index
    for ob in meshes:
        solo = ob.name.startswith(tuple(alone))
        sc.world.light_settings.distance = alone_distance if solo else distance
        for o in meshes:
            o.select_set(False)
            o.hide_render = solo and not o.name.startswith(tuple(alone))
        ob.select_set(True)
        bpy.context.view_layer.objects.active = ob
        bpy.ops.object.bake(type='AO', target='VERTEX_COLORS', use_selected_to_active=False)
        if solo:
            for c in ob.data.color_attributes['AO'].data:
                c.color = tuple(alone_floor + (1.0 - alone_floor) * x for x in c.color[:3]) + (1.0,)
        print('  baked AO', ob.name)
    for o in meshes:
        o.hide_render = False


def export_glb(path, joint_order=None):
    """One skin with every bone as a joint, the baked shade as COLOR_0, smooth normals, the
    materials by name, nothing else (no animations, no cameras, no lights). Y up, facing +Z.

    `joint_order`: names of the bones that must come first, in that order, among the skin's joints
    (the exporter writes them as it walks the tree: the file is rewritten after)."""
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    for o in bpy.context.scene.objects:
        o.select_set(o.type in ('MESH', 'ARMATURE'))
    textured = any(n.type == 'TEX_IMAGE' for m in bpy.data.materials if m.node_tree for n in m.node_tree.nodes)
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format='GLB',
        use_selection=True,
        export_apply=True,
        export_yup=True,
        export_skins=True,
        export_influence_nb=4,
        export_all_influences=False,
        export_rest_position_armature=True,
        export_animations=False,
        export_morph=False,
        export_def_bones=False,
        export_leaf_bone=False,
        export_vertex_color='ACTIVE',
        export_all_vertex_colors=False,
        export_active_vertex_color_when_no_material=True,
        export_normals=True,
        export_tangents=False,
        export_texcoords=textured,
        export_materials='EXPORT',
        export_image_format='WEBP' if textured else 'NONE',
        export_image_quality=92,
        export_cameras=False,
        export_lights=False,
        export_extras=False,
    )
    if joint_order:
        import glb
        glb.ordenar_huesos(path, joint_order)
    print('exported', path, os.path.getsize(path) // 1024, 'KB')
