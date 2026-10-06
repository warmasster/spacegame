"""High-frequency fabric detail (seams, folds, wrinkles) baked into a tangent-space normal map."""

import math

import bpy  # noqa: I001
import numpy as np
from mathutils import Vector

from suitkit import link


def _seg_project(P, a, b):
    """Distance to segment a-b, param t in [0,1], radial unit vectors."""
    ab = b - a
    L = np.linalg.norm(ab)
    axis = ab / L
    d = P - a
    t = np.clip(d @ axis / L, 0.0, 1.0)
    foot = a + np.outer(t * L, axis)
    radial = P - foot
    dist = np.linalg.norm(radial, axis=1)
    return dist, t, radial / np.maximum(dist, 1e-9)[:, None], axis, L


def _groove(u, depth=0.0017, width=0.0028, pucker=0.0007):
    return -depth * np.exp(-(u / width) ** 2) + pucker * np.exp(-((np.abs(u) - 2.6 * width) / width) ** 2)


def _noise(P, seed, count=7, fmin=8.0, fmax=30.0):
    rng = np.random.default_rng(seed)
    out = np.zeros(len(P))
    for _ in range(count):
        d = rng.normal(size=3)
        d /= np.linalg.norm(d)
        f = rng.uniform(fmin, fmax)
        out += np.sin(P @ d * f + rng.uniform(0, 2 * np.pi))
    return out / math.sqrt(count)


def detail_field(P, segments):
    """segments: list of dicts {a, b, kind, flex_dir, seams_t}. Returns displacement (m) along normals."""
    n = len(P)
    best = np.full(n, np.inf)
    idx = np.zeros(n, dtype=int)
    proj = []
    for i, s in enumerate(segments):
        dist, t, radial, axis, L = _seg_project(P, s['a'], s['b'])
        # bias so torso does not steal limb vertices
        score = dist - s.get('bias', 0.0)
        m = score < best
        best[m] = score[m]
        idx[m] = i
        proj.append((dist, t, radial, axis, L))

    disp = np.zeros(n)
    warp = _noise(P, 11, fmin=10, fmax=22)
    mask_n = 0.5 + 0.5 * _noise(P, 12, count=5, fmin=5, fmax=12)
    for i, s in enumerate(segments):
        m = idx == i
        if not m.any():
            continue
        dist, t, radial, axis, L = proj[i]
        dist, t, radial = dist[m], t[m], radial[m]
        Pm = P[m]
        fwd = np.array(s.get('ref', (0.0, -1.0, 0.0)))
        fwd = fwd - axis * (fwd @ axis)
        fwd /= np.linalg.norm(fwd)
        side = np.cross(axis, fwd)
        phi = np.arctan2(radial @ side, radial @ fwd)
        along = t * L
        d = np.zeros(m.sum())
        if s['kind'] == 'limb':
            # longitudinal side seams
            for ph0 in (np.pi / 2, -np.pi / 2):
                dphi = np.angle(np.exp(1j * (phi - ph0)))
                u = dphi * dist
                inside = (t > 0.04) & (t < 0.96)
                d += np.where(inside, _groove(u), 0.0)
            # circumferential seams
            for t0 in s.get('seams_t', ()):
                d += _groove((t - t0) * L)
            # flexion folds near the joint end(s)
            for end, flex_phi in s.get('flex', ()):
                s_end = np.abs(along - (L if end == 1 else 0.0))
                ang = np.angle(np.exp(1j * (phi - flex_phi)))
                w = np.exp(-(ang / 0.95) ** 2) * np.exp(-s_end / 0.07)
                chevron = np.sin(2 * np.pi * s_end / 0.024 + 2.2 * np.cos(ang) + 0.6 * warp[m])
                d += 0.0036 * w * chevron
            # general horizontal wrinkles, irregular
            wr = np.sin(2 * np.pi * along / 0.043 + 2.5 * warp[m] + 0.8 * np.cos(phi * 2))
            d += 0.0018 * wr * mask_n[m] ** 2
            # broad sag folds that read from a distance
            sag = np.sin(2 * np.pi * along / 0.085 + 1.7 * warp[m] + 1.3 * np.sin(phi))
            d += 0.0025 * sag * (0.35 + 0.65 * mask_n[m])
        else:  # torso
            x, y, z = Pm[:, 0], Pm[:, 1], Pm[:, 2]
            front = y < 0.0
            # front panel seams
            for x0 in (-0.078, 0.078):
                d += np.where(front & (z > 1.13) & (z < 1.46), _groove(x - x0), 0.0)
            # back centre seam
            d += np.where(~front & (z > 1.0) & (z < 1.47), _groove(x), 0.0)
            # horizontal seam around the waist
            d += np.where(z > 1.0, _groove(z - 1.215), 0.0)
            # abdomen compression folds
            wz = np.exp(-((z - 1.17) / 0.045) ** 2)
            d += 0.003 * wz * np.sin(2 * np.pi * z / 0.028 + 1.3 * np.sin(x / 0.05) + 0.6 * warp[m])
            # brief/crotch folds
            wb = np.exp(-((z - 0.93) / 0.05) ** 2)
            d += 0.0026 * wb * np.sin(2 * np.pi * (z + 0.35 * np.abs(x)) / 0.032 + 0.6 * warp[m])
            d += 0.0012 * _noise(Pm, 21, fmin=12, fmax=26) * mask_n[m]
        disp[m] = d
    return disp


def uv_unwrap(ob, angle=62, margin=0.004):
    sc = bpy.context.scene
    for o in sc.objects:
        o.select_set(False)
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project(angle_limit=math.radians(angle), island_margin=margin, area_weight=0.0,
                             correct_aspect=True, scale_to_bounds=False)
    bpy.ops.object.mode_set(mode='OBJECT')
    ob.select_set(False)


def bake_detail_normals(body, segments, size=2048, image_path=None):
    sc = bpy.context.scene
    uv_unwrap(body)

    hi = bpy.data.objects.new('BodyHi', body.data.copy())
    link(hi)
    hi.data.materials.clear()
    sub = hi.modifiers.new('Sub', 'SUBSURF')
    sub.levels = sub.render_levels = 2
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(hi.evaluated_get(dg))
    hi.modifiers.clear()
    old = hi.data
    hi.data = me
    bpy.data.meshes.remove(old)

    n = len(me.vertices)
    co = np.empty(n * 3, dtype=np.float64)
    me.vertices.foreach_get('co', co)
    co = co.reshape(-1, 3)
    nor = np.empty(n * 3, dtype=np.float64)
    me.vertices.foreach_get('normal', nor)
    nor = nor.reshape(-1, 3)
    disp = detail_field(co, segments)
    co += nor * disp[:, None]
    me.vertices.foreach_set('co', co.ravel())
    me.update()
    print(f'  detail mesh: {n} verts, disp range {disp.min() * 1000:.2f}..{disp.max() * 1000:.2f} mm')

    img = bpy.data.images.new('SuitBodyNormal', size, size, alpha=False)
    img.colorspace_settings.name = 'Non-Color'
    for mat in body.data.materials:
        nt = mat.node_tree
        tex = nt.nodes.new('ShaderNodeTexImage')
        tex.image = img
        nt.nodes.active = tex

    sc.render.engine = 'CYCLES'
    sc.cycles.samples = 8
    bake = sc.render.bake
    bake.use_selected_to_active = True
    bake.cage_extrusion = 0.012
    bake.max_ray_distance = 0.03
    bake.target = 'IMAGE_TEXTURES'
    bake.margin = 12
    for o in sc.objects:
        o.select_set(False)
    hi.select_set(True)
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.bake(type='NORMAL', normal_space='TANGENT', use_selected_to_active=True)
    body.select_set(False)
    bpy.data.objects.remove(hi)
    bpy.data.meshes.remove(me)

    if image_path:
        img.filepath_raw = image_path
        img.file_format = 'PNG'
        img.save()
    img.pack()
    for mat in body.data.materials:
        nt = mat.node_tree
        tex = nt.nodes.active
        nm = nt.nodes.new('ShaderNodeNormalMap')
        nt.links.new(tex.outputs['Color'], nm.inputs['Color'])
        nt.links.new(nm.outputs['Normal'], nt.nodes['Principled BSDF'].inputs['Normal'])
    return img


def body_segments(nodes):
    V = lambda v: np.array(v, dtype=np.float64)  # noqa: E731
    segs = [{'a': V(nodes['pelvis'][0]), 'b': V(nodes['neck'][0]), 'kind': 'torso', 'bias': -0.03}]
    for s in ('L', 'R'):
        segs += [
            {'a': V(nodes[f'sh.{s}'][0]), 'b': V(nodes[f'el1.{s}'][0]), 'kind': 'limb', 'seams_t': (0.3,),
             'flex': ((1, 0.0),)},
            {'a': V(nodes[f'el1.{s}'][0]), 'b': V(nodes[f'wr.{s}'][0]), 'kind': 'limb', 'seams_t': (0.55,),
             'flex': ((0, 0.0),)},
            {'a': V(nodes[f'hip.{s}'][0]), 'b': V(nodes[f'kn1.{s}'][0]), 'kind': 'limb', 'seams_t': (0.22,),
             'flex': ((1, math.pi),), 'bias': 0.01},
            {'a': V(nodes[f'kn1.{s}'][0]), 'b': V(nodes[f'an.{s}'][0]), 'kind': 'limb', 'seams_t': (0.55,),
             'flex': ((0, math.pi),)},
        ]
    return segs
