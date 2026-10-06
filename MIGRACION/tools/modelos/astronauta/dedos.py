"""Hands: where the glove has its joints, the finger bones on them, the glove's mesh and its weights.

The glove is drawn as a right hand in its own frame: Z up the forearm (the hand hangs toward -Z),
X toward the thumb, Y the palm's normal (the palm looks toward +Y). `glove_matrix` puts it on the
hand bone; the left glove is that one mirrored in Y, so both palms look at the thighs.

Every digit is a chain of four points (`chains`): the head of `.01`, of `.02`, of `.03` and the tip.
The three bones of a digit lie in one plane by construction (each direction is the one before
turned about the same hinge axis), so the hinge axis is the normal of that plane.

The mesh is a cage of quads (a slab for the palm cut in four columns, a square tube for each digit,
a ring on every joint, on its bisector plane) rounded by two levels of Catmull-Clark. The cage marks
which digit each vertex belongs to (`T_<digit>`, carried through the subdivision), and the weights
are worked out from that and from the distance to the bisector plane of every joint (`weights`).
"""
import math

import bpy  # noqa: I001  (bpy must be imported before bmesh)
import bmesh
from mathutils import Matrix, Vector

DIGITS = ('thumb', 'index', 'middle', 'ring', 'pinky')
FINGERS = DIGITS[1:]

FINGER_X = (0.0395, 0.0133, -0.0133, -0.0385)   # index, middle, ring, little: +X is toward the thumb
FINGER_LEN = (0.094, 0.104, 0.100, 0.082)       # knuckle to tip, straight
SEG = (0.42, 0.32, 0.26)                        # share of that length of each phalanx
CURL = (30.0, 45.0, 30.0)                       # rest flexion of knuckle, middle and end joints (degrees)
SPLAY = 0.9                                     # how much the fingers fan out (tangent of the angle per metre of x)
KNUCKLE_Y, KNUCKLE_Z = 0.004, -0.108            # the knuckle line
THUMB_BASE = (0.05, 0.014, -0.034)
THUMB_SEGS = (((0.5, 0.45, -0.7), 0.046), ((0.1, 0.75, -0.6), 0.038), ((-0.2, 0.8, -0.4), 0.030))
WRIST_DROP = 0.012                              # the glove's origin, this far down the hand bone from its head

FINGER_R = (0.0134, 0.0136, 0.0132, 0.0122)     # half-width of each finger's cage (the round finger is 0.93 of it)
WEB = 0.42                                      # the fingers part this far along the first phalanx
HALF_CELL = 0.0131                              # half the room of a finger on the knuckle line

# weights: half-width of the blend across each joint (metres from its bisector plane)
BLEND_KNUCKLE = (0.012, 0.014)                  # into the palm, into the finger
BLEND_JOINT = 0.008
BLEND_SIDE = 0.005                              # between neighbouring fingers, on the palm only


def V(*a):
    return Vector(a)


def smoothstep(e0, e1, x):
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def chains(curl=CURL):
    """{digit: [p0, p1, p2, p3]} in the glove's frame: the joints of each digit and its tip."""
    out = {}
    x = V(1, 0, 0)
    for name, fx, fl in zip(FINGERS, FINGER_X, FINGER_LEN):
        d = V(fx * SPLAY, 0, -1).normalized()
        h = (x - d * x.dot(d)).normalized()           # the hinge: across the finger, square to it
        p = V(fx, KNUCKLE_Y, KNUCKLE_Z)
        pts = [p.copy()]
        ang = 0.0
        for j in range(3):
            ang += math.radians(curl[j])
            p = p + (Matrix.Rotation(ang, 3, h) @ d) * (fl * SEG[j])   # curls toward the palm (+Y)
            pts.append(p.copy())
        out[name] = pts
    d1, d2, d3 = (V(*s[0]).normalized() for s in THUMB_SEGS)
    h = d1.cross(d2).normalized()
    d3 = (d3 - h * d3.dot(h)).normalized()            # the last phalanx into the plane of the other two
    p = V(*THUMB_BASE)
    pts = [p.copy()]
    for d, s in zip((d1, d2, d3), THUMB_SEGS):
        p = p + d * s[1]
        pts.append(p.copy())
    out['thumb'] = pts
    return out


def straight(name):
    """The direction a finger would have with no curl at all."""
    fx = FINGER_X[FINGERS.index(name)]
    return V(fx * SPLAY, 0, -1).normalized()


def hinge(pts):
    return (pts[1] - pts[0]).cross(pts[2] - pts[1]).normalized()


def glove_matrix(head, tail, sx):
    """Glove frame -> model, for the hand bone head -> tail; `sx` > 0 is the left hand (mirrored)."""
    down = (tail - head).normalized()
    z = -down
    x = V(0, -1, 0)
    x = (x - z * x.dot(z)).normalized()
    y = z.cross(x)
    m = Matrix((x, y, z)).transposed().to_4x4()
    m.translation = head + down * WRIST_DROP
    if sx > 0:
        m = m @ Matrix.Scale(-1, 4, V(0, 1, 0))
    return m


def finger_bones(side, sx, head, tail, curl=CURL):
    """{bone: (head, tail, parent, deform, where its local Z looks)} for the 15 bones of one hand.

    Local Y runs along the bone, local Z looks to the side the finger curls to, so local X is the
    hinge and a positive turn about it closes the finger, on both hands."""
    m = glove_matrix(head, tail, sx)
    out = {}
    for digit in DIGITS:
        w = [m @ p for p in chains(curl)[digit]]
        n = hinge(w)
        parent = f'hand.{side}'
        for j in range(3):
            name = f'{digit}.{j + 1:02d}.{side}'
            d = (w[j + 1] - w[j]).normalized()
            out[name] = (tuple(w[j]), tuple(w[j + 1]), parent, True, tuple(n.cross(d)))
            parent = name
    return out


# --------------------------------------------------------------------------------------
# The cage
# --------------------------------------------------------------------------------------

def _boundaries():
    """x of the five lines that part the four fingers on the knuckle line, thumb side first."""
    fx = FINGER_X
    return [fx[0] + HALF_CELL, (fx[0] + fx[1]) / 2, (fx[1] + fx[2]) / 2, (fx[2] + fx[3]) / 2, fx[3] - HALF_CELL]


def _ring(c, t, h, hx, hy, miter=1.0):
    """Four corners round `c`, square to `t`: [back/+h, back/-h, palm/-h, palm/+h]."""
    n = h.cross(t).normalized()                       # toward the palm side
    return [c + h * hx - n * hy * miter, c - h * hx - n * hy * miter, c - h * hx + n * hy * miter, c + h * hx + n * hy * miter]


def _joint_ring(c, d_in, d_out, h, hx, hy):
    t = (d_in + d_out).normalized()
    return _ring(c, t, h, hx, hy, 1.0 / max(t.dot(d_in), 0.5))


def build_cage(curl=CURL):
    """The glove's cage as a bmesh, with a deform layer: group k marks the digit DIGITS[k] (1 on the
    rings that are the digit's alone). Faces of the palm side and of the fingertips carry material 1
    (grip)."""
    ch = chains(curl)
    bm = bmesh.new()
    dl = bm.verts.layers.deform.verify()
    grip = []

    def vert(co, digit=None):
        v = bm.verts.new(co)
        if digit is not None:
            v[dl][DIGITS.index(digit)] = 1.0
        return v

    def face(vs, palm=False):
        f = bm.faces.new(vs)
        if palm:
            grip.append(f)
        return f

    xb = _boundaries()
    frac = [(xb[0] - x) / (xb[0] - xb[4]) for x in xb]
    cosa = [math.cos(math.radians(18 + 36 * k)) for k in range(5)]
    sina = [math.sin(math.radians(18 + 36 * k)) for k in range(5)]

    # --- palm: rings of ten (five on the back, five on the palm), from the wrist to the knuckles.
    # `u` = 0 is a round wrist of radius (a, b), 1 a slab: half-widths xp/xm, half-thickness ht
    # thinned toward its two edges by `edge` (back) and `edge_p` (palm).
    def ring(z, u, a, b, xp, xm, ht, cy, edge=0.68, edge_p=0.72):
        prof_b = (edge, 0.5 + 0.5 * edge + 0.14, 1.0, 0.5 + 0.5 * edge + 0.14, edge)
        prof_p = (edge_p, 0.5 + 0.5 * edge_p + 0.12, 1.0, 0.5 + 0.5 * edge_p + 0.12, edge_p)
        back, palm = [], []
        for k in range(5):
            xs = xp - frac[k] * (xp + xm)
            x = a * cosa[k] * (1 - u) + xs * u
            back.append(V(x, (-b * sina[k]) * (1 - u) + (cy - ht * min(prof_b[k], 1.0)) * u, z))
            palm.append(V(x, (b * sina[k]) * (1 - u) + (cy + ht * min(prof_p[k], 1.0)) * u, z))
        return back, palm

    rings = [ring(0.016, 0.0, 0.0465, 0.0465, 0, 0, 0, 0),
             ring(-0.006, 0.3, 0.0470, 0.0445, 0.0490, 0.0480, 0.0320, 0.000),
             ring(-0.030, 1.0, 0, 0, 0.0498, 0.0488, 0.0305, 0.0005),
             ring(-0.062, 1.0, 0, 0, 0.0528, 0.0512, 0.0265, 0.002),
             ring(-0.090, 1.0, 0, 0, 0.0530, 0.0518, 0.0225, 0.003, 0.76, 0.78)]
    # the knuckle ring, on the bisector of the palm and the first phalanx
    a0 = math.radians(curl[0])
    t = V(0, math.sin(a0 / 2), -math.cos(a0 / 2))
    n = V(1, 0, 0).cross(t)
    prof = (0.84, 0.98, 1.0, 0.98, 0.84)
    k0 = 0.0190 / math.cos(a0 / 2)
    rings.append(([V(x, KNUCKLE_Y, KNUCKLE_Z) - n * k0 * prof[k] for k, x in enumerate(xb)],
                  [V(x, KNUCKLE_Y, KNUCKLE_Z) + n * k0 * prof[k] for k, x in enumerate(xb)]))
    # the web ring: where the fingers part, each finger's own frame, shared corners averaged
    webs = []
    hw = 0.0150
    for i, name in enumerate(FINGERS):
        p = ch[name]
        d1 = (p[1] - p[0]).normalized()
        c = p[0] + d1 * (WEB * (p[1] - p[0]).length)
        h = hinge(p)
        left, right = xb[i] - FINGER_X[i], FINGER_X[i] - xb[i + 1]
        nn = h.cross(d1)
        webs.append((c + h * left - nn * hw, c - h * right - nn * hw, c - h * right + nn * hw, c + h * left + nn * hw))
    back = [webs[0][0]] + [(webs[i][1] + webs[i + 1][0]) / 2 for i in range(3)] + [webs[3][1]]
    palm = [webs[0][3]] + [(webs[i][2] + webs[i + 1][3]) / 2 for i in range(3)] + [webs[3][2]]
    rings.append((back, palm))

    THUMB_FROM = 2                                     # the thumb grows out of the side between rings 2 and 3
    rv = [([vert(co) for co in b], [vert(co) for co in p]) for b, p in rings]
    face(rv[0][0] + rv[0][1][::-1])                    # the top, inside the cuff
    for a in range(len(rv) - 1):
        (b0, p0), (b1, p1) = rv[a], rv[a + 1]
        for k in range(4):
            face((b0[k], b0[k + 1], b1[k + 1], b1[k]))
            face((p0[k], p0[k + 1], p1[k + 1], p1[k]), palm=a >= 2)
        face((b0[4], p0[4], p1[4], b1[4]))             # the little finger's side
        if a != THUMB_FROM:
            face((b0[0], p0[0], p1[0], b1[0]))         # the thumb's side

    def tube(base, ring_cos, digit, palm_side, tip_from=5):
        """Rings after `base` (four verts), a cap on the last. `palm_side`: which of the four sides
        is the pad; from ring `tip_from` on, all four are (the rubber fingertip)."""
        prev = base
        for k, cos in enumerate(ring_cos):
            cur = [vert(co, digit) for co in cos]
            for s in range(4):
                face((prev[s], prev[(s + 1) % 4], cur[(s + 1) % 4], cur[s]), palm=s == palm_side or k >= tip_from)
            prev = cur
        face(prev, palm=True)

    # --- fingers
    for i, name in enumerate(FINGERS):
        p = ch[name]
        d = [(p[j + 1] - p[j]).normalized() for j in range(3)]
        ln = [(p[j + 1] - p[j]).length for j in range(3)]
        h = hinge(p)
        R = FINGER_R[i]
        cos = [
            _ring(p[0] + d[0] * (WEB * ln[0] + 0.008), d[0], h, R, R),
            _joint_ring(p[1], d[0], d[1], h, R * 0.97, R * 0.97),
            _ring(p[1] + d[1] * ln[1] * 0.5, d[1], h, R * 0.98, R * 0.98),
            _joint_ring(p[2], d[1], d[2], h, R * 0.93, R * 0.93),
            _ring(p[2] + d[2] * ln[2] * 0.5, d[2], h, R * 0.94, R * 0.94),
            _ring(p[2] + d[2] * ln[2] * 0.9, d[2], h, R * 0.80, R * 0.80),
            _ring(p[3] + d[2] * 0.002, d[2], h, R * 0.40, R * 0.40),
        ]
        base = [rv[-1][0][i], rv[-1][0][i + 1], rv[-1][1][i + 1], rv[-1][1][i]]
        tube(base, cos, name, 2)

    # --- thumb, out of the side of the palm
    p = ch['thumb']
    d = [(p[j + 1] - p[j]).normalized() for j in range(3)]
    ln = [(p[j + 1] - p[j]).length for j in range(3)]
    h = hinge(p)

    def tring(c, t, a, b, miter=1.0):
        nn = h.cross(t).normalized()                   # the side the thumb curls to
        return [c + h * a - nn * b * miter, c + h * a + nn * b * miter, c - h * a + nn * b * miter, c - h * a - nn * b * miter]

    def tjoint(c, d_in, d_out, a, b):
        tt = (d_in + d_out).normalized()
        return tring(c, tt, a, b, 1.0 / max(tt.dot(d_in), 0.5))

    cos = [
        tring(p[0] + d[0] * 0.020, d[0], 0.0215, 0.0195),          # the ball of the thumb
        tjoint(p[1], d[0], d[1], 0.0166, 0.0160),
        tring(p[1] + d[1] * ln[1] * 0.5, d[1], 0.0157, 0.0154),
        tjoint(p[2], d[1], d[2], 0.0148, 0.0145),
        tring(p[2] + d[2] * ln[2] * 0.5, d[2], 0.0146, 0.0143),
        tring(p[2] + d[2] * ln[2] * 0.9, d[2], 0.0124, 0.0121),
        tring(p[3] + d[2] * 0.002, d[2], 0.0062, 0.0060),
    ]
    a, b = THUMB_FROM, THUMB_FROM + 1
    base = [rv[a][0][0], rv[a][1][0], rv[b][1][0], rv[b][0][0]]    # back/top, palm/top, palm/bottom, back/bottom
    tube(base, cos, 'thumb', 1)

    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    for f in grip:
        f.material_index = 1
    return bm


def build_mesh(name, curl=CURL, levels=2):
    """The glove's mesh (glove frame), rounded, with the `T_<digit>` groups on a throwaway object."""
    bm = build_cage(curl)
    me = bpy.data.meshes.new(name + 'Cage')
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new(name + 'Cage', me)
    bpy.context.scene.collection.objects.link(ob)
    for d in DIGITS:
        ob.vertex_groups.new(name='T_' + d)
    sub = ob.modifiers.new('Sub', 'SUBSURF')
    sub.levels = sub.render_levels = levels
    dg = bpy.context.evaluated_depsgraph_get()
    out = bpy.data.meshes.new_from_object(ob.evaluated_get(dg), preserve_all_data_layers=True, depsgraph=dg)
    out.name = name
    bpy.data.objects.remove(ob)
    bpy.data.meshes.remove(me)
    out.shade_smooth()
    return out


# --------------------------------------------------------------------------------------
# Weights
# --------------------------------------------------------------------------------------

def weights(me, curl=CURL):
    """[{bone key: weight}] for every vertex of the glove's mesh (glove frame, before it is placed):
    keys are 'hand' and '<digit>.<01|02|03>'. Reads the `T_<digit>` groups (indices 0..4)."""
    ch = chains(curl)
    xb = _boundaries()
    planes = {}
    for name in FINGERS:
        p = ch[name]
        d = [straight(name)] + [(p[j + 1] - p[j]).normalized() for j in range(3)]
        planes[name] = [(p[j], (d[j] + d[j + 1]).normalized()) for j in range(3)]
    p = ch['thumb']
    d = [(p[j + 1] - p[j]).normalized() for j in range(3)]
    planes['thumb'] = [None] + [(p[j], (d[j - 1] + d[j]).normalized()) for j in (1, 2)]

    def chain(co, name, m, a0, own, out):
        pl = planes[name]
        a1 = smoothstep(-BLEND_JOINT, BLEND_JOINT, (co - pl[1][0]).dot(pl[1][1])) if own else 0.0
        a2 = smoothstep(-BLEND_JOINT, BLEND_JOINT, (co - pl[2][0]).dot(pl[2][1])) if own and a1 > 0.999 else 0.0
        for key, w in ((name + '.01', m * a0 * (1 - a1)), (name + '.02', m * a0 * a1 * (1 - a2)), (name + '.03', m * a0 * a1 * a2)):
            if w > 1e-4:
                out[key] = out.get(key, 0.0) + w

    result = []
    for v in me.vertices:
        co = v.co
        T = [0.0] * 5
        for g in v.groups:
            if g.group < 5:
                T[g.group] = g.weight
        tsum = min(1.0, sum(T))
        out = {}
        # the thumb: its base blends with the hand along the cone that joins it to the palm
        if T[0] > 0.0:
            chain(co, 'thumb', 1.0, smoothstep(0.05, 0.95, T[0]), T[0] > 0.5, out)
        # the fingers: on a finger, that finger's bones alone; on the palm, the neighbours share the web
        st = [1.0 - smoothstep(xb[k] - BLEND_SIDE, xb[k] + BLEND_SIDE, co.x) for k in (1, 2, 3)]
        c = [1.0 - st[0], st[0] - st[1], st[1] - st[2], st[2]]
        for i, name in enumerate(FINGERS):
            m = T[i + 1] + (1.0 - tsum) * c[i]
            if m <= 1e-4:
                continue
            pl = planes[name][0]
            a0 = smoothstep(-BLEND_KNUCKLE[0], BLEND_KNUCKLE[1], (co - pl[0]).dot(pl[1]))
            if a0 > 0.0:
                chain(co, name, m, a0, T[i + 1] > 0.5, out)
        rest = 1.0 - sum(out.values())
        if rest > 1e-4:
            out['hand'] = rest
        s = sum(out.values())
        result.append({k: w / s for k, w in out.items()})
    return result


def apply_weights(ob, side, curl=CURL):
    """Replaces the `T_<digit>` groups of the glove object by the bones' groups of that side."""
    ws = weights(ob.data, curl)
    ob.vertex_groups.clear()
    groups = {}
    for i, w in enumerate(ws):
        for key, x in w.items():
            name = f'{key}.{side}'
            if name not in groups:
                groups[name] = ob.vertex_groups.new(name=name)
            groups[name].add([i], x, 'REPLACE')
    return ob
