"""Styles: what a ship's plain shapes are seen as from close by. A ship says a wing is a box
2.5 m by 0.16 by 2.4 with `"estilo": "ala"`; the style builds a wing that size. One function serves
every size: it reads the piece's (`p.tam`, or `p.r`, `p.h`) and works in its frame (x to port,
y up, z forward; a copy across the centreline is drawn mirrored by the game).

A style keeps to its shape: what it builds fills it and stays within a few centimetres of it."""
import math

from mathutils import Vector

from kit import TINTE, estilo


def rect_redondo(a, b, r, n=4):
    """The outline of an `a` by `b` rectangle with its corners rounded by `r`."""
    r = min(r, a * 0.49, b * 0.49)
    pts = []
    for cx, cy, a0 in ((a / 2 - r, b / 2 - r, 0), (-a / 2 + r, b / 2 - r, 90), (-a / 2 + r, -b / 2 + r, 180), (a / 2 - r, -b / 2 + r, 270)):
        for k in range(n + 1):
            ang = math.radians(a0 + 90 * k / n)
            pts.append((cx + r * math.cos(ang), cy + r * math.sin(ang)))
    return pts


def superelipse(a, b, n=24, e=2.6):
    """The outline of a rounded box: half axes `a`, `b`; `e` 2 an ellipse, more a squarer one."""
    out = []
    for k in range(n):
        t = math.tau * k / n
        c, s = math.cos(t), math.sin(t)
        out.append((a * math.copysign(abs(c) ** (2 / e), c), b * math.copysign(abs(s) ** (2 / e), s)))
    return out


def largo(p):
    """A long piece: (its long axis's letter, its length, its section's two sizes as an
    extrusion along that axis takes them)."""
    t = list(p.tam)
    i = max(range(3), key=lambda k: t[k])
    # (an outline pushed along z is (x, y); along x, (z, y); along y, (x, z))
    a, b = {2: (0, 1), 0: (2, 1), 1: (0, 2)}[i]
    return 'xyz'[i], t[i], t[a], t[b]


def a_lo_largo(eje, a, b, s):
    """The point at section coordinates (`a`, `b`) and `s` along the axis."""
    return {'z': Vector((a, b, s)), 'x': Vector((s, b, a)), 'y': Vector((a, s, b))}[eje]


def radio_alto(p):
    """A leg: its radius and its height, whether its shape is a cylinder or a box standing."""
    if p.r is not None:
        return p.r, p.h
    return min(p.tam.x, p.tam.z) / 2, p.tam.y


def banda(m, p, r0, r1, y0, y1, mat, en=(0, 0, 0), eje='y', lados=24, marco=None):
    """A ring round `eje` at `en`: from radius `r0` out to `r1`, from `y0` to `y1` along it."""
    return m.torno([(r0, y0), (r1, y0), (r1, y1), (r0, y1), (r0, y0)], en=en, mat=mat, pieza=p, marco=marco or p, eje=eje, lados=lados)


def tramos(desde, hasta, paso):
    """Evenly spaced points from `desde` to `hasta`, about `paso` apart."""
    n = max(1, int(round((hasta - desde) / paso)))
    return [desde + (hasta - desde) * k / n for k in range(n + 1)]


# ---------------------------------------------------------------------------------------------
# wings and tail


def grosor_ala(sy, sz):
    """Half the thickness of a wing along its chord (z): full over its box spar, a quarter less
    at its front (where its leading edge piece takes over), thinning aft to its trailing edge."""
    zt, zf = -sz / 2, sz / 2
    zk = zt + sz * 0.5

    def h(z):
        if z >= zk:
            return sy / 2 * (1.0 - 0.25 * ((z - zk) / (zf - zk)) ** 2.2)
        f = (z - zt) / (zk - zt)
        return 0.012 + (sy / 2 - 0.012) * math.sin(f * math.pi / 2) ** 0.9

    return h, zk


@estilo('ala')
def ala(m, p):
    """A stub wing: its box spar and skin, a flap along its trailing edge on three hinges, rows
    of fasteners over its ribs and spars, two access panels and a filler cap on top."""
    sx, sy, sz = p.tam
    zt, zf = -sz / 2, sz / 2
    h, zk = grosor_ala(sy, sz)
    z_flap = zt + sz * 0.27
    zs = tramos(z_flap + 0.004, zf, (zf - z_flap) / 14)
    m.extrusion([(z, h(z)) for z in zs] + [(z, -h(z)) for z in reversed(zs)], sx, mat=TINTE, pieza=p, marco=p, eje='x')
    # the flap, a gap all round it; a fixed piece of the same section each end
    zs = tramos(zt, z_flap - 0.004, (z_flap - zt) / 8)
    perfil = [(z, h(z)) for z in zs] + [(z_flap + 0.002, 0)] + [(z, -h(z)) for z in reversed(zs)]
    m.extrusion(perfil, sx - 0.16, mat=TINTE, pieza=p, marco=p, eje='x')
    for x in (-1, 1):
        m.extrusion(perfil, 0.07, en=(x * (sx / 2 - 0.035), 0, 0), mat=TINTE, pieza=p, marco=p, eje='x')
    for x in (-sx * 0.3, 0.0, sx * 0.3):
        m.caja((0.045, 0.034, 0.28), en=(x, -h(z_flap) - 0.008, z_flap + 0.03), mat='pintura_gris', pieza=p, marco=p, bisel=0.012, seg=3)
    # fasteners: over each rib, along the two spars; top and bottom
    costillas = tramos(-sx / 2 + 0.1, sx / 2 - 0.1, 0.56)
    arriba, abajo = [], []
    for x in costillas:
        for z in tramos(z_flap + 0.09, zf - 0.07, 0.13):
            arriba.append((x, h(z), z))
            abajo.append((x, -h(z), z))
    for z in (zk + 0.03, zf - 0.12):
        for x in tramos(-sx / 2 + 0.17, sx / 2 - 0.17, 0.14):
            arriba.append((x, h(z), z))
            abajo.append((x, -h(z), z))
    m.tornillos(arriba, 0.0045, 0.0014, 'acero', p, p, eje='y')
    m.tornillos(abajo, 0.0045, 0.0014, 'acero', p, p, eje=(0, -1, 0))
    # two access panels and the filler cap
    zc = zk + (zf - zk) * 0.38
    for x in (-sx * 0.2, sx * 0.03):
        m.caja((0.36, 0.004, 0.24), en=(x + 0.14, h(zc) - 0.0005, zc), mat=TINTE, pieza=p, marco=p, bisel=0.0012, seg=1)
        m.tornillos([(x + 0.14 + a * 0.16, h(zc) + 0.0015, zc + b * 0.1) for a in (-1, 0, 1) for b in (-1, 1)], 0.004, 0.0012, 'acero_oscuro', p, p, eje='y')
    m.cilindro(0.05, 0.007, en=(sx * 0.33, h(zc), zc), mat='acero_oscuro', pieza=p, marco=p, lados=20, bisel=0.002)
    m.cilindro(0.032, 0.012, en=(sx * 0.33, h(zc) + 0.002, zc), mat='pintura_roja', pieza=p, marco=p, lados=16, bisel=0.003)
    # static wicks off the trailing edge
    for x in (-sx * 0.38, -sx * 0.12, sx * 0.14, sx * 0.4):
        m.tubo([(x, 0, zt + 0.01), (x, -0.004, zt - 0.1)], 0.0025, 'plastico_negro', p, p, lados=6)


def planta(puntos):
    """A flat convex wing given as a hull of points (in its piece's frame, span along x, chord
    along z, thickness along y): its outline seen from above (convex, counter-clockwise in x-z),
    its mid-plane height, and its thickness at its root (least x) and at its tip (most x)."""
    pts = [(p[0], p[2]) for p in puntos]
    pts = sorted(set(pts))

    def giro(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    bajo, alto = [], []
    for q in pts:
        while len(bajo) >= 2 and giro(bajo[-2], bajo[-1], q) <= 0:
            bajo.pop()
        bajo.append(q)
    for q in reversed(pts):
        while len(alto) >= 2 and giro(alto[-2], alto[-1], q) <= 0:
            alto.pop()
        alto.append(q)
    contorno = bajo[:-1] + alto[:-1]
    xs = [p[0] for p in puntos]
    x0, x1 = min(xs), max(xs)
    ys = [p[1] for p in puntos]
    medio = (min(ys) + max(ys)) / 2

    def grueso(x):
        cerca = [p[1] for p in puntos if abs(p[0] - x) < 1e-4]
        return (max(cerca) - min(cerca)) if cerca else 0.0

    return contorno, medio, grueso(x0), grueso(x1), x0, x1


def cuerda(contorno, x):
    """Where the outline is crossed at span station `x`: (trailing edge z, leading edge z)."""
    zs = []
    n = len(contorno)
    for i in range(n):
        (ax, az), (bx, bz) = contorno[i], contorno[(i + 1) % n]
        if (ax - x) * (bx - x) <= 0 and ax != bx:
            zs.append(az + (bz - az) * (x - ax) / (bx - ax))
        elif ax == x:
            zs.append(az)
    return min(zs), max(zs)


@estilo('ala_flecha')
def ala_flecha(m, p):
    """A swept, tapered wing (a canard, a stabiliser) from its hull of points: its outline from
    above, as thick at root and tip as its points say, an airfoil all along (rounded leading
    edge, sharp trailing edge), the leading edge in its warning paint, a flap along the inner
    two thirds of its trailing edge, rows of fasteners over its spars and wicks off its tip."""
    contorno, y0, g0, g1, x0, x1 = planta(p.forma['points'])
    n_u = 14
    # (more stations near the root and the tip, where the outline turns)
    estaciones = [x0 + (x1 - x0) * (0.5 - 0.5 * math.cos(math.pi * k / 10)) for k in range(11)]

    def perfil(x, enc=1.0):
        zt, zl = cuerda(contorno, x)
        t = g0 + (g1 - g0) * (x - x0) / max(x1 - x0, 1e-6)
        anillo = []
        for k in range(n_u + 1):
            u = k / n_u
            # thickness along the chord (NACA-like, its thickest a third back), from the edge
            h = t / 2 * enc * (2.969 * math.sqrt(u) - 1.26 * u - 3.516 * u ** 2 + 2.843 * u ** 3 - 1.015 * u ** 4) / 1.0
            anillo.append((x, y0 + h, zl + (zt - zl) * u))
        for k in range(n_u - 1, 0, -1):
            u = k / n_u
            h = t / 2 * enc * (2.969 * math.sqrt(u) - 1.26 * u - 3.516 * u ** 2 + 2.843 * u ** 3 - 1.015 * u ** 4)
            anillo.append((x, y0 - h, zl + (zt - zl) * u))
        return anillo

    m.piel([perfil(x) for x in estaciones], mat=TINTE, pieza=p, marco=p, liso=55.0)
    # the leading edge in warning paint: the first tenth of the chord, a touch proud
    banda = []
    for x in estaciones[1:-1]:
        zt, zl = cuerda(contorno, x)
        t = g0 + (g1 - g0) * (x - x0) / max(x1 - x0, 1e-6)
        anillo = []
        for k in range(7):
            a = math.radians(-90 + 180 * k / 6)
            anillo.append((x, y0 + t * 0.36 * math.sin(a), zl + 0.004 - (zt - zl) * 0.06 * (1 - math.cos(a))))
        banda.append(anillo)
    m.piel(banda, mat='pintura_roja', pieza=p, marco=p, cerrado=False, liso=60.0)
    # (its root is its thicker end, whichever side of the ship it is on)
    raiz, punta = (x0, x1) if g0 >= g1 else (x1, x0)
    hacia = 1.0 if punta > raiz else -1.0
    # the flap: a line along the inner two thirds of the trailing edge (a slot top and bottom)
    xf = raiz + (punta - raiz) * 0.66
    for lado in (1, -1):
        pts = []
        for x in tramos(raiz + 0.05 * hacia, xf, 0.25):
            zt, zl = cuerda(contorno, x)
            t = g0 + (g1 - g0) * (x - x0) / max(x1 - x0, 1e-6)
            z = zt + (zl - zt) * 0.22
            pts.append((x, y0 + lado * t * 0.21, z))
        m.tubo(pts, 0.004, 'acero_oscuro', p, p, lados=6)
    # fasteners over its two spars, top and bottom
    arriba, abajo = [], []
    for f in (0.25, 0.6):
        for x in tramos(x0 + 0.1, x1 - 0.1, 0.18):
            zt, zl = cuerda(contorno, x)
            t = g0 + (g1 - g0) * (x - x0) / max(x1 - x0, 1e-6)
            z = zl + (zt - zl) * f
            arriba.append((x, y0 + t * 0.49, z))
            abajo.append((x, y0 - t * 0.49, z))
    m.tornillos(arriba, 0.0045, 0.0014, 'acero', p, p, eje='y')
    m.tornillos(abajo, 0.0045, 0.0014, 'acero', p, p, eje=(0, -1, 0))
    # static wicks off the tip's trailing edge
    zt, zl = cuerda(contorno, punta - 0.02 * hacia)
    for f in (0.0, 0.3):
        a = Vector((punta - (0.02 + f * 0.4) * hacia, y0, zt + 0.01))
        m.tubo([a, a + Vector((0, -0.004, -0.12))], 0.0025, 'plastico_negro', p, p, lados=6)


@estilo('borde_ala')
def borde_ala(m, p):
    """A wing's leading edge: a rounded nose in its paint, a band over each rib joint."""
    sx, sy, sz = p.tam
    n = 12

    def nariz(k):
        pts = [(-sz / 2, -sy / 2 * k)]
        for i in range(n + 1):
            a = math.radians(-90 + 180 * i / n)
            pts.append((-sz / 2 + sz * k * math.cos(a) ** 0.8, sy / 2 * k * math.sin(a)))
        return pts[:-1] + [(-sz / 2, sy / 2 * k)]

    m.extrusion(nariz(1.0), sx, mat=TINTE, pieza=p, marco=p, eje='x')
    for x in tramos(-sx / 2 + 0.1, sx / 2 - 0.1, 0.56):
        m.extrusion(nariz(1.012), 0.022, en=(x, 0, 0), mat='aluminio', pieza=p, marco=p, eje='x')


@estilo('pilon')
def pilon(m, p):
    """What carries a nacelle at a wing's tip: a faired body, the nacelle's pivot ring outboard."""
    sx, sy, sz = p.tam
    anillos = []
    for i in range(15):
        t = -1 + 2 * i / 14
        s = 0.16 + 0.84 * max(1 - abs(t) ** 2.6, 0.0) ** 0.5
        anillos.append([(a, b, t * sz / 2) for a, b in superelipse(sx / 2 * s, sy / 2 * s, 20, 2.8)])
    m.piel(anillos, mat=TINTE, pieza=p, marco=p, liso=60.0)
    r = sy * 0.44
    m.cilindro(r, 0.05, en=(sx / 2 - 0.012, 0, 0), mat='acero_oscuro', pieza=p, marco=p, eje='x', lados=24, bisel=0.004)
    m.brida(r * 1.12, en=(sx / 2 + 0.006, 0, 0), grosor=0.012, pernos=10, mat='acero', pieza=p, marco=p, eje='x', r_int=r * 0.7)
    # a fairing strip over its top seam, with its fasteners
    m.tornillos([(0, sy / 2 * (0.16 + 0.84 * max(1 - abs(t) ** 2.6, 0.0) ** 0.5), t * sz / 2) for t in (-0.7, -0.45, -0.2, 0.2, 0.45, 0.7)], 0.005, 0.0016, 'acero', p, p, eje='y')


@estilo('aleta')
def aleta(m, p):
    """A fin (a wedge: its tall edge aft, its slope the leading edge): a thin tapered section,
    a steel strip up its leading edge, a wider root, rows of fasteners, wicks off its tip."""
    sx, sy, sz = p.tam
    ct = sz * 0.14
    n = 9

    def borde(v):
        # where its leading edge is at height `v` (0 its root, 1 its tip)
        return sz / 2 - v * (sz - ct)

    def medio(u, v):
        # half its thickness at `u` along the chord (0 the leading edge, 1 the trailing)
        raiz = 1.0 + 0.9 * max(0.0, 1.0 - v / 0.12) ** 2
        return sx / 2 * (1.0 - 0.45 * v) * raiz * (0.08 + 0.92 * (u ** 0.5 * (1 - u)) / 0.385)

    def punto(u, v, lado):
        zl = borde(v)
        return (lado * medio(u, v), -sy / 2 + v * sy, zl + (-sz / 2 - zl) * u)

    us = [(k / n) ** 1.6 for k in range(n + 1)]
    anillos = []
    for v in (0.0, 0.04, 0.08, 0.12, 0.25, 0.4, 0.55, 0.7, 0.85, 0.96, 1.0):
        anillos.append([punto(u, v, 1) for u in us] + [punto(u, v, -1) for u in reversed(us[1:-1])])
    m.piel(anillos, mat=TINTE, pieza=p, marco=p, liso=50.0)
    m.tubo([punto(0.0, v, 0) for v in (0.02, 0.5, 0.98)], sx * 0.09, 'acero', p, p, lados=8)
    for lado in (1, -1):
        pts = [punto(u, v, lado) for v in (0.2, 0.45, 0.7) for u in (0.14, 0.26, 0.38, 0.5, 0.62, 0.74, 0.86)]
        m.tornillos(pts, 0.0045, 0.0014, 'acero', p, p, eje=(lado, 0, 0))
    for v in (0.55, 0.75, 0.93):
        a = Vector(punto(1.0, v, 0))
        m.tubo([a, a + Vector((0, 0.004, -0.1))], 0.0025, 'plastico_negro', p, p, lados=6)


@estilo('lomo')
def lomo(m, p):
    """A spine along a hull's back: covers over what runs under it, each on four fasteners, its
    ends faired down, a handrail for whoever works out there on every other one."""
    eje, L, sx, sy = largo(p)
    n = max(2, int(round(L / 0.72)))
    paso = L / n

    def seccion(k=1.0):
        pts = [(-sx / 2 * k, -sy / 2), (sx / 2 * k, -sy / 2)]
        for i in range(11):
            a = math.pi * i / 10
            pts.append((sx / 2 * 0.94 * k * math.cos(a), -sy / 2 + sy * (0.3 + 0.7 * k * math.sin(a) ** 0.7)))
        return pts

    def techo(x):
        c = max(min(x / (sx / 2 * 0.94), 1.0), -1.0)
        return -sy / 2 + sy * (0.3 + 0.7 * (1 - c * c) ** 0.35)

    for k in range(n):
        s = -L / 2 + paso * (k + 0.5)
        if k in (0, n - 1):
            # an end: faired down to nothing much
            hacia = -1 if k == 0 else 1
            anillos = []
            for f in (0.0, 0.35, 0.65, 0.85, 1.0):
                esc = 1.0 - 0.72 * f ** 1.8
                anillos.append([a_lo_largo(eje, a, b, s - hacia * (paso / 2 - 0.003) + hacia * (paso - 0.006) * f) for a, b in seccion(esc)])
            m.piel(anillos, mat=TINTE, pieza=p, marco=p, liso=50.0)
            continue
        m.extrusion(seccion(), paso - 0.006, en=a_lo_largo(eje, 0, 0, s), mat=TINTE, pieza=p, marco=p, eje=eje)
        x = sx * 0.3
        m.tornillos([a_lo_largo(eje, a, techo(a), s + b * (paso / 2 - 0.05)) for a in (-x, x) for b in (-1, 1)], 0.006, 0.002, 'acero', p, p, eje=a_lo_largo(eje, 0, 1, 0))
        if k % 2 == 1:
            m.asa(a_lo_largo(eje, 0, techo(0), s - paso * 0.3), a_lo_largo(eje, 0, techo(0), s + paso * 0.3), a_lo_largo(eje, 0, 1, 0), alto=0.05, r=0.008, mat='pintura_amarilla', pieza=p, marco=p)


# ---------------------------------------------------------------------------------------------
# landing gear


@estilo('tren_pata')
def tren_pata(m, p):
    """A gear leg: a polished rod out of its sleeve (a gland nut and its wiper where it comes
    out), torque links between them, a hydraulic line clipped down the sleeve, a ball at its end
    for the foot."""
    r, h = radio_alto(p)
    arriba, abajo = h / 2, -h / 2
    union = arriba - h * 0.46
    vast = r * 0.72
    m.torno([(0, abajo), (vast * 0.9, abajo), (vast, abajo + 0.01), (vast, union + 0.03)], mat='cromo', pieza=p, marco=p, lados=20)
    m.torno([(vast + 0.002, union - 0.014), (r * 0.92, union - 0.014), (r * 1.06, union), (r * 1.06, union + 0.04), (r, union + 0.05), (r, arriba - 0.005), (r - 0.005, arriba), (0, arriba)], mat=TINTE, pieza=p, marco=p, lados=24)
    banda(m, p, vast + 0.001, vast + 0.012, union - 0.024, union - 0.014, 'goma', lados=20)
    # a marking band round the sleeve
    banda(m, p, r - 0.002, r + 0.0015, union + 0.2, union + 0.25, 'pintura_amarilla')
    # its end: a collar and the ball the foot turns on
    m.cilindro(vast * 1.3, 0.05, en=(0, abajo + 0.075, 0), mat='acero_oscuro', pieza=p, marco=p, lados=16, bisel=0.006)
    m.esfera(vast * 1.2, en=(0, abajo + 0.03, 0), mat='acero', pieza=p, marco=p, lados=16)
    # torque links: from a lug on the sleeve out to a knee and back to the collar
    lug_a, lug_b = Vector((0, union + 0.03, r)), Vector((0, abajo + 0.08, vast * 1.25))
    # (in proportion to the leg: on a thin one they are small)
    codo = Vector((0, (lug_a.y + lug_b.y) / 2, r + min(0.1, h * 0.1, r * 1.3)))
    for x in (-r * 0.22, r * 0.22):
        d = Vector((x, 0, 0))
        m.tubo([lug_a + d, codo + d, lug_b + d], max(0.004, r * 0.095), 'acero_oscuro', p, p, lados=8)
    for c in (lug_a, codo, lug_b):
        m.cilindro(max(0.006, r * 0.13), r * 0.74, en=c, mat='acero', pieza=p, marco=p, eje='x', lados=10, bisel=0.002)
    # the hydraulic line, on the other side
    x = r + 0.01
    m.tubo([(-x * 0.7, arriba - 0.01, -x * 0.7), (-x * 0.7, union + 0.09, -x * 0.7), (-x * 0.5, union + 0.06, -x * 0.5)], 0.005, 'acero', p, p, lados=8, codo=0.02)
    for y in (arriba - 0.08, (arriba + union) / 2 + 0.05, union + 0.13):
        m.caja((0.022, 0.014, 0.022), en=(-x * 0.68, y, -x * 0.68), mat='acero_oscuro', pieza=p, marco=p, bisel=0.003, seg=1, rot=(0, 45, 0))


@estilo('tren_amort')
def tren_amort(m, p):
    """Where a gear leg goes into the hull: a ribbed housing with its bolted ring and a
    charging valve."""
    r, h = radio_alto(p)
    m.torno([(r * 0.72, -h / 2), (r * 0.86, -h / 2 + 0.014), (r * 0.86, h / 2 - 0.045), (r, h / 2 - 0.04), (r, h / 2 - 0.004), (r - 0.004, h / 2), (r * 0.6, h / 2)], mat=TINTE, pieza=p, marco=p, lados=28)
    for k in range(8):
        marco = p.girado((0, 45 * k + 22.5, 0))
        m.caja((0.008, h - 0.07, r * 0.16), en=(0, -0.012, r * 0.9), mat=TINTE, pieza=p, marco=marco, bisel=0.002, seg=1)
    m.tornillos([(r * 0.93 * math.cos(math.tau * k / 12), h / 2, r * 0.93 * math.sin(math.tau * k / 12)) for k in range(12)], 0.006, 0.004, 'acero', p, p, eje='y')
    banda(m, p, r * 0.62, r * 0.78, -h / 2 - 0.004, -h / 2 + 0.006, 'goma')
    # the charging valve
    m.cilindro(0.008, 0.035, en=(r * 0.9, 0.02, 0), mat='laton', pieza=p, marco=p, eje='x', lados=8, bisel=0.001)
    m.cilindro(0.011, 0.012, en=(r * 0.9 + 0.022, 0.02, 0), mat='pintura_roja', pieza=p, marco=p, eje='x', lados=8, bisel=0.002)


@estilo('tren_pie')
def tren_pie(m, p):
    """A foot pad: a tray with an upturned rim on a rubber sole, ribs inside it out from the
    socket the leg's ball sits in."""
    sx, sy, sz = p.tam
    a, b = sx / 2, sz / 2

    def anillo(k, y):
        return [(u * k, y, v * k) for u, v in superelipse(a, b, 28, 3.2)]

    suela = -sy / 2 + min(0.014, sy * 0.25)
    m.piel([anillo(0.9, -sy / 2), anillo(0.98, -sy / 2 + 0.004), anillo(0.98, suela)], mat='goma', pieza=p, marco=p, liso=50.0)
    m.piel([anillo(1.0, suela), anillo(1.0, sy * 0.1), anillo(1.03, sy / 2), anillo(0.95, sy / 2), anillo(0.9, 0.0), anillo(0.3, -sy * 0.05)], mat=TINTE, pieza=p, marco=p, liso=50.0)
    rb = min(a, b) * 0.34
    m.torno([(rb, -sy * 0.05), (rb, sy * 0.3), (rb * 0.8, sy / 2), (rb * 0.55, sy / 2), (rb * 0.45, sy * 0.2), (0, sy * 0.2)], mat='acero_oscuro', pieza=p, marco=p, lados=20)
    m.tornillos([(rb * 0.68 * math.cos(math.tau * (k + 0.5) / 6), sy / 2, rb * 0.68 * math.sin(math.tau * (k + 0.5) / 6)) for k in range(6)], 0.005, 0.003, 'acero', p, p, eje='y')
    for k in range(8):
        ang = math.tau * (k + 0.5) / 8
        c, s = math.cos(ang), math.sin(ang)
        # from the socket out to the rim, whichever way the pad is longer
        fin = 0.84 / max(abs(c) / a, abs(s) / b)
        largo_ = fin - rb
        m.caja((largo_, sy * 0.4, 0.006), en=((rb + largo_ / 2) * c, sy * 0.12, (rb + largo_ / 2) * s), mat=TINTE, pieza=p, marco=p, bisel=0.0015, seg=1, rot=(0, -math.degrees(ang), 0))


# ---------------------------------------------------------------------------------------------
# flight deck


@estilo('consola_lateral')
def consola_lateral(m, p):
    """A side console (its panel lies on its top, which stays flat): a body over a recessed
    kick plate, a door with slats each side, a fire bottle clipped to its back."""
    sx, sy, sz = p.tam
    zocalo = 0.06
    m.caja((sx - 0.03, zocalo + 0.01, sz - 0.03), en=(0, -sy / 2 + zocalo / 2, 0), mat='plastico_negro', pieza=p, marco=p, bisel=0.004, seg=1)
    cuerpo = m.caja((sx, sy - zocalo, sz), en=(0, zocalo / 2, 0), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=3)
    del cuerpo
    # (each side the same: the piece turned half round)
    for marco in (p, p.girado((0, 180, 0))):
        x = sx / 2
        m.caja((0.006, sy * 0.62, sz * 0.74), en=(x, zocalo / 2 - 0.02, 0), mat=TINTE, pieza=p, marco=marco, bisel=0.002, seg=1)
        m.rejilla((0.11, sz * 0.5), en=(x + 0.004, -sy * 0.2, 0), mat='acero_oscuro', pieza=p, marco=marco, lamas=12, grosor=0.003, inclinar=30.0, normal='x')
        m.tornillos([(x + 0.003, zocalo / 2 - 0.02 + a * sy * 0.28, b * sz * 0.34) for a in (-1, 1) for b in (-1, 1)], 0.006, 0.002, 'acero', p, marco, eje='x')
        m.asa((x + 0.003, sy * 0.2, -0.07), (x + 0.003, sy * 0.2, 0.07), (1, 0, 0), alto=0.022, r=0.005, mat='acero', pieza=p, marco=marco)
    # the fire bottle, on its back
    z = -sz / 2
    m.capsula(0.036, 0.25, en=(0, 0.04, z - 0.04), mat='pintura_roja', pieza=p, marco=p, lados=16, fondo=0.7)
    m.cilindro(0.014, 0.03, en=(0, 0.04 + 0.135, z - 0.04), mat='acero', pieza=p, marco=p, lados=10, bisel=0.002)
    m.caja((0.05, 0.012, 0.016), en=(0.012, 0.04 + 0.156, z - 0.04), mat='plastico_negro', pieza=p, marco=p, bisel=0.003, seg=1)
    for y in (-0.03, 0.11):
        m.torno([(0.038, -0.008), (0.04, -0.008), (0.04, 0.008), (0.038, 0.008)], en=(0, y, z - 0.04), mat='acero', pieza=p, marco=p, lados=16)
        m.caja((0.03, 0.018, 0.04), en=(0, y, z - 0.012), mat='acero', pieza=p, marco=p, bisel=0.002, seg=1)


# ---------------------------------------------------------------------------------------------
# frames


@estilo('viga_cajon')
def viga_cajon(m, p):
    """A box beam: a thin-walled tube with lightening holes down its sides, a bolted plate on
    each end."""
    eje, L, a, b = largo(p)
    pared = max(0.004, min(a, b) * 0.07)
    rc = min(a, b) * 0.14
    tubo = m.extrusion(rect_redondo(a, b, rc), L - 0.012, mat=TINTE, pieza=p, marco=p, eje=eje)
    m.restar(tubo, m.extrusion(rect_redondo(a - 2 * pared, b - 2 * pared, max(rc - pared, 0.002)), L + 0.05, mat=TINTE, pieza=p, marco=p, eje=eje))
    # holes through the sides one sees it from: level, across the beam
    # (a beam lying along z or standing: across x; one lying along x: across z)
    traves = 'z' if eje == 'x' else 'x'
    cara, ancho = b, a
    d = cara * 0.5
    margen = max(0.1, 1.1 * max(a, b))
    hueco = max(0.0, L - 2 * margen)
    n = int(hueco / max(d * 2.0, 0.16))
    if n >= 1:
        pts = []
        for k in range(n):
            s = -hueco / 2 + hueco * (k + 0.5) / n
            pts.append({'z': (0, 0, s), 'x': (s, 0, 0), 'y': (0, s, 0)}[eje])
        m.taladrar(tubo, pts, d / 2, ancho + 0.05, eje=traves, marco=p, lados=16)
    for lado in (-1, 1):
        c = a_lo_largo(eje, 0, 0, lado * (L / 2 - 0.004))
        tam = a_lo_largo(eje, a + 0.006, b + 0.006, 0.008)
        m.caja((abs(tam.x), abs(tam.y), abs(tam.z)), en=c, mat=TINTE, pieza=p, marco=p, bisel=0.002, seg=1)
        m.tornillos([a_lo_largo(eje, u * (a / 2 - rc * 1.1), v * (b / 2 - rc * 1.1), lado * L / 2) for u in (-1, 1) for v in (-1, 1)], min(0.007, min(a, b) * 0.09), 0.004, 'acero', p, p, eje=a_lo_largo(eje, 0, 0, lado))


@estilo('tubo_jaula')
def tubo_jaula(m, p):
    """A bar of a roll cage: a round tube between two cast nodes, a clamp in its middle if it
    is long, a padded sleeve where a head might meet it if it stands."""
    eje, L, a, b = largo(p)
    r = min(a, b) / 2 * 0.9
    m.cilindro(r, L, mat=TINTE, pieza=p, marco=p, eje=eje, lados=14, bisel=0.001)
    for lado in (-1, 1):
        c = a_lo_largo(eje, 0, 0, lado * (L / 2 - 0.035))
        tam = a_lo_largo(eje, a + 0.008, b + 0.008, 0.07)
        m.caja((abs(tam.x), abs(tam.y), abs(tam.z)), en=c, mat='acero_oscuro', pieza=p, marco=p, bisel=0.008, seg=2)
        for u, v in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            m.tornillos([a_lo_largo(eje, u * (a / 2 + 0.004), v * (b / 2 + 0.004), lado * (L / 2 - 0.035))], 0.006, 0.003, 'acero', p, p, eje=a_lo_largo(eje, u, v, 0))
    if L > 1.2:
        banda(m, p, r - 0.002, r + 0.004, -0.016, 0.016, 'acero', eje=eje, lados=14)
    if eje == 'y' and L > 1.0:
        m.cilindro(r + 0.014, min(0.5, L * 0.3), en=(0, L * 0.14, 0), mat='acolchado', pieza=p, marco=p, lados=14, bisel=0.008)


@estilo('chapa_cubierta')
def chapa_cubierta(m, p):
    """A deck plate: its tread plate in a frame screwed down all round, four rings to lash
    cargo to lying in their pans."""
    sx, sy, sz = p.tam
    marco_ = 0.03
    m.caja((sx - marco_, sy - 0.003, sz - marco_), en=(0, -0.0015, 0), mat=TINTE, pieza=p, marco=p, bisel=0.0, seg=1)
    for z in (-1, 1):
        m.caja((sx, sy, marco_), en=(0, 0, z * (sz / 2 - marco_ / 2)), mat='aluminio', pieza=p, marco=p, bisel=0.003, seg=1)
    for x in (-1, 1):
        m.caja((marco_, sy, sz - 2 * marco_ + 0.004), en=(x * (sx / 2 - marco_ / 2), 0, 0), mat='aluminio', pieza=p, marco=p, bisel=0.003, seg=1)
    pts = [(x * (sx / 2 - marco_ / 2), sy / 2, z) for x in (-1, 1) for z in tramos(-sz / 2 + 0.06, sz / 2 - 0.06, 0.24)]
    pts += [(x, sy / 2, z * (sz / 2 - marco_ / 2)) for z in (-1, 1) for x in tramos(-sx / 2 + 0.12, sx / 2 - 0.12, 0.24)]
    m.tornillos(pts, 0.005, 0.0015, 'acero_oscuro', p, p, eje='y')
    for x in (-1, 1):
        for z in (-1, 1):
            c = Vector((x * (sx / 2 - 0.16), sy / 2 - 0.003, z * (sz / 2 - 0.2)))
            m.cilindro(0.045, 0.004, en=c, mat='acero_oscuro', pieza=p, marco=p, lados=18, bisel=0.001)
            m.tubo([c + Vector((0.026 * math.cos(math.tau * k / 14), 0.006, 0.026 * math.sin(math.tau * k / 14))) for k in range(15)], 0.0045, 'acero', p, p, lados=6, tapas=False)


@estilo('estribo')
def estribo(m, p):
    """A step: a tube frame round a grating of bars on edge, open between them."""
    sx, sy, sz = p.tam
    r = sy * 0.36
    a, b = sx / 2 - r, sz / 2 - r
    m.tubo([(0, 0, -b), (a, 0, -b), (a, 0, b), (-a, 0, b), (-a, 0, -b), (0, 0, -b)], r, TINTE, p, p, lados=10, codo=r * 2.2)
    for z in tramos(-b + 0.05, b - 0.05, 0.055):
        m.caja((2 * a, sy * 0.62, 0.005), en=(0, 0, z), mat='acero_oscuro', pieza=p, marco=p, bisel=0.001, seg=1)
    for x in tramos(-a + 0.08, a - 0.08, 0.16):
        m.tubo([(x, -sy * 0.12, -b), (x, -sy * 0.12, b)], 0.004, 'acero_oscuro', p, p, lados=6)


@estilo('viga_i')
def viga_i(m, p):
    """A rail: an I beam with a stiffener every metre or so, a bolted splice every few, a
    polished strip where the wheels run and a stop with its buffer at each end."""
    eje, L, a, b = largo(p)
    tf, tw = b * 0.17, a * 0.2
    f = min(0.012, (a - tw) * 0.2)
    perfil = [(-a / 2, -b / 2), (a / 2, -b / 2), (a / 2, -b / 2 + tf), (tw / 2 + f, -b / 2 + tf), (tw / 2, -b / 2 + tf + f), (tw / 2, b / 2 - tf - f), (tw / 2 + f, b / 2 - tf), (a / 2, b / 2 - tf),
              (a / 2, b / 2), (-a / 2, b / 2), (-a / 2, b / 2 - tf), (-tw / 2 - f, b / 2 - tf), (-tw / 2, b / 2 - tf - f), (-tw / 2, -b / 2 + tf + f), (-tw / 2 - f, -b / 2 + tf), (-a / 2, -b / 2 + tf)]
    m.extrusion(perfil, L, mat=TINTE, pieza=p, marco=p, eje=eje)
    ala_ = (a - tw) / 2
    for s in tramos(-L / 2 + 0.5, L / 2 - 0.5, 1.1):
        for lado in (-1, 1):
            tam = a_lo_largo(eje, ala_ - 0.004, b - 2 * tf, 0.008)
            m.caja((abs(tam.x), abs(tam.y), abs(tam.z)), en=a_lo_largo(eje, lado * (tw / 2 + ala_ / 2), 0, s), mat=TINTE, pieza=p, marco=p, bisel=0.0, seg=1)
    for s in tramos(-L / 2, L / 2, 4.4)[1:-1]:
        for lado in (-1, 1):
            tam = a_lo_largo(eje, 0.008, b - 2 * tf - 0.03, 0.3)
            m.caja((abs(tam.x), abs(tam.y), abs(tam.z)), en=a_lo_largo(eje, lado * (tw / 2 + 0.004), 0, s + 0.55), mat='acero', pieza=p, marco=p, bisel=0.002, seg=1)
            m.tornillos([a_lo_largo(eje, lado * (tw / 2 + 0.008), v * (b / 2 - tf - 0.04), s + 0.55 + u * 0.1) for u in (-1, 0, 1) for v in (-1, 1)], 0.007, 0.004, 'acero_oscuro', p, p, eje=a_lo_largo(eje, lado, 0, 0))
    tam = a_lo_largo(eje, a * 0.45, 0.005, L - 0.2)
    m.caja((abs(tam.x), abs(tam.y), abs(tam.z)), en=a_lo_largo(eje, 0, b / 2 + 0.002, 0), mat='acero', pieza=p, marco=p, bisel=0.001, seg=1)
    for lado in (-1, 1):
        tam = a_lo_largo(eje, a, b * 0.8, 0.05)
        m.caja((abs(tam.x), abs(tam.y), abs(tam.z)), en=a_lo_largo(eje, 0, b * 0.5, lado * (L / 2 - 0.025)), mat='pintura_roja', pieza=p, marco=p, bisel=0.006, seg=2)
        m.cilindro(b * 0.22, 0.05, en=a_lo_largo(eje, 0, b * 0.55, lado * (L / 2 - 0.075)), mat='goma', pieza=p, marco=p, eje=eje, lados=14, bisel=0.008)


# ---------------------------------------------------------------------------------------------
# the gantry crane


def rueda(m, p, c, r, ancho, eje):
    """A flanged wheel at `c` turning about `eje`, with its hub."""
    m.torno([(r * 0.3, -ancho / 2), (r * 1.12, -ancho / 2), (r * 1.12, -ancho / 2 + ancho * 0.18), (r, -ancho / 2 + ancho * 0.26), (r, ancho / 2), (r * 0.3, ancho / 2), (r * 0.3, -ancho / 2)], en=c, mat='acero', pieza=p, marco=p, eje=eje, lados=22)
    m.cilindro(r * 0.36, ancho + 0.02, en=c, mat='acero_oscuro', pieza=p, marco=p, eje=eje, lados=12, bisel=0.004)


@estilo('testero')
def testero(m, p):
    """A crane's end truck: a body over two flanged wheels, a drive on its top, a buffer at
    each end."""
    sx, sy, sz = p.tam
    r = sy * 0.26
    m.caja((sx * 0.86, sy * 0.56, sz), en=(0, sy * 0.2, 0), mat=TINTE, pieza=p, marco=p, bisel=0.016, seg=3)
    for z in (-1, 1):
        zc = z * (sz / 2 - r - 0.06)
        m.caja((sx, sy * 0.5, r * 2.5), en=(0, -sy * 0.08, zc), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=2)
        rueda(m, p, (0, -sy / 2 + r * 1.12, zc), r, sx * 0.4, 'x')
        m.cilindro(r * 0.5, sx + 0.016, en=(0, -sy / 2 + r * 1.12, zc), mat='acero_oscuro', pieza=p, marco=p, eje='x', lados=14, bisel=0.004)
        m.cilindro(0.04, 0.05, en=(0, sy * 0.2, z * (sz / 2 - 0.02)), mat='goma', pieza=p, marco=p, eje='z', lados=14, bisel=0.008)
    # the drive: a gearbox and its motor along the truck, on its top
    m.caja((sx * 0.5, sy * 0.16, 0.16), en=(0, sy / 2 - sy * 0.09, -0.02), mat='pintura_gris', pieza=p, marco=p, bisel=0.008, seg=2)
    perfil = [(0, -0.11), (0.045, -0.11), (0.05, -0.1)]
    for k in range(9):
        y = -0.1 + 0.02 * k
        perfil += [(0.05, y + 0.004), (0.056, y + 0.006), (0.056, y + 0.012), (0.05, y + 0.014)]
    perfil += [(0.05, 0.1), (0.04, 0.11), (0, 0.11)]
    m.torno(perfil, en=(0, sy / 2 - 0.052, 0.19), mat='pintura_amarilla', pieza=p, marco=p, eje='z', lados=18)
    m.tornillos([(a * sx * 0.3, sy * 0.2 + sy * 0.28, b) for a in (-1, 1) for b in (-sz * 0.3, -sz * 0.16)], 0.007, 0.004, 'acero', p, p, eje='y')


@estilo('carro_grua')
def carro_grua(m, p):
    """A crane's trolley: a frame on four flanged wheels, the hoist's motor (finned) and its
    gearbox on it, a collar under it where the mast hangs, its terminal box."""
    sx, sy, sz = p.tam
    r = min(0.07, sy * 0.2)
    yb = -sy / 2 + 2 * r + 0.01
    for z in (-1, 1):
        m.caja((sx, 0.08, 0.07), en=(0, yb + 0.04, z * (sz / 2 - 0.035)), mat=TINTE, pieza=p, marco=p, bisel=0.008, seg=2)
        for x in (-1, 1):
            rueda(m, p, (x * (sx / 2 - r - 0.03), -sy / 2 + r * 1.12, z * (sz / 2 - 0.035)), r, 0.05, 'z')
    for x in (-1, 1):
        m.caja((0.07, 0.08, sz - 0.14), en=(x * (sx / 2 - 0.16), yb + 0.04, 0), mat=TINTE, pieza=p, marco=p, bisel=0.008, seg=2)
    m.caja((sx * 0.5, 0.012, sz - 0.14), en=(0, yb + 0.08, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.002, seg=1)
    # the motor, across the trolley
    alto = sy / 2 - (yb + 0.086)
    rm = min(alto / 2, 0.1)
    perfil = [(0, -0.17), (rm * 0.8, -0.17), (rm * 0.9, -0.15)]
    for k in range(10):
        y = -0.15 + 0.026 * k
        perfil += [(rm * 0.9, y + 0.006), (rm, y + 0.009), (rm, y + 0.017), (rm * 0.9, y + 0.02)]
    perfil += [(rm * 0.9, 0.13), (rm * 0.7, 0.15), (rm * 0.35, 0.15), (rm * 0.35, 0.19), (0, 0.19)]
    m.torno(perfil, en=(-sx * 0.14, yb + 0.086 + rm, -sz * 0.12), mat='pintura_amarilla', pieza=p, marco=p, eje='x', lados=20)
    m.caja((0.2, alto * 0.94, 0.24), en=(sx * 0.2, yb + 0.086 + alto * 0.47, -sz * 0.12), mat='pintura_gris', pieza=p, marco=p, bisel=0.012, seg=3)
    m.tornillos([(sx * 0.2 + a * 0.07, yb + 0.086 + alto * 0.94, -sz * 0.12 + b * 0.09) for a in (-1, 1) for b in (-1, 1)], 0.007, 0.004, 'acero', p, p, eje='y')
    # the terminal box and its conduit
    m.caja((0.16, alto * 0.6, 0.1), en=(-sx * 0.22, yb + 0.086 + alto * 0.3, sz * 0.22), mat='plastico_gris', pieza=p, marco=p, bisel=0.008, seg=2)
    m.tubo([(-sx * 0.22 + 0.08, yb + 0.12, sz * 0.22), (sx * 0.1, yb + 0.12, sz * 0.22), (sx * 0.1, yb + 0.12, -sz * 0.02)], 0.011, 'goma', p, p, lados=8, codo=0.04)
    # the collar the mast hangs from
    m.caja((0.32, yb + sy / 2 - 0.004, 0.32), en=(0, (-sy / 2 + yb) / 2, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.014, seg=2)
    m.tornillos([(a * 0.12, yb + 0.092, b * 0.12) for a in (-1, 1) for b in (-1, 1)], 0.009, 0.005, 'acero', p, p, eje='y')


@estilo('telescopio_exterior')
def telescopio_exterior(m, p):
    """The outer tube of a telescopic mast: a square tube with stiffening bands, a bolted
    flange on top, a collar with its bronze slide pads below, a hose down one side."""
    eje, L, a, b = largo(p)
    m.extrusion(rect_redondo(a, b, a * 0.14), L, mat=TINTE, pieza=p, marco=p, eje='y')
    for f in (0.28, -0.05, -0.38):
        m.extrusion(rect_redondo(a + 0.012, b + 0.012, a * 0.14 + 0.006), 0.035, en=(0, L * f, 0), mat=TINTE, pieza=p, marco=p, eje='y')
    m.extrusion(rect_redondo(a + 0.07, b + 0.07, 0.02), 0.014, en=(0, L / 2 - 0.007, 0), mat='acero_oscuro', pieza=p, marco=p, eje='y')
    m.tornillos([(u * (a / 2 + 0.018), L / 2 - 0.014, v * (b / 2 + 0.018)) for u in (-1, 0, 1) for v in (-1, 0, 1) if u or v], 0.007, 0.004, 'acero', p, p, eje=(0, -1, 0))
    m.extrusion(rect_redondo(a + 0.03, b + 0.03, a * 0.14 + 0.012), 0.07, en=(0, -L / 2 + 0.035, 0), mat='acero_oscuro', pieza=p, marco=p, eje='y')
    for u, v in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        m.caja((0.05 if v else 0.012, 0.04, 0.05 if u else 0.012), en=(u * (a / 2 + 0.018), -L / 2 + 0.035, v * (b / 2 + 0.018)), mat='laton', pieza=p, marco=p, bisel=0.002, seg=1)
    x = a / 2 + 0.014
    m.tubo([(x, L / 2 - 0.05, b * 0.25), (x, -L / 2 + 0.1, b * 0.25)], 0.008, 'goma', p, p, lados=8)
    for y in tramos(-L / 2 + 0.2, L / 2 - 0.15, 0.4):
        m.caja((0.012, 0.016, 0.03), en=(x - 0.002, y, b * 0.25), mat='acero', pieza=p, marco=p, bisel=0.002, seg=1)


@estilo('telescopio_interior')
def telescopio_interior(m, p):
    """The inner tube of a telescopic mast: a ground square tube, a stop collar at its top, a
    clevis and its pin at its end."""
    eje, L, a, b = largo(p)
    m.extrusion(rect_redondo(a * 0.92, b * 0.92, a * 0.16), L - 0.05, en=(0, 0.025, 0), mat=TINTE, pieza=p, marco=p, eje='y')
    m.extrusion(rect_redondo(a, b, a * 0.16), 0.04, en=(0, L / 2 - 0.02, 0), mat='acero_oscuro', pieza=p, marco=p, eje='y')
    m.caja((a, 0.07, b), en=(0, -L / 2 + 0.035, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.012, seg=2)
    m.cilindro(0.016, a + 0.03, en=(0, -L / 2 + 0.035, 0), mat='acero', pieza=p, marco=p, eje='x', lados=12, bisel=0.003)


@estilo('telescopio_tramo')
def telescopio_tramo(m, p):
    """A middle section of a telescopic mast: a ground square tube that slides in the one over
    it and carries the next: a stop collar at its top, a collar with its bronze slide pads at
    its end."""
    eje, L, a, b = largo(p)
    m.extrusion(rect_redondo(a * 0.94, b * 0.94, a * 0.15), L - 0.04, en=(0, 0.02, 0), mat=TINTE, pieza=p, marco=p, eje='y')
    m.extrusion(rect_redondo(a, b, a * 0.15), 0.035, en=(0, L / 2 - 0.0175, 0), mat='acero_oscuro', pieza=p, marco=p, eje='y')
    m.extrusion(rect_redondo(a + 0.022, b + 0.022, a * 0.15 + 0.01), 0.06, en=(0, -L / 2 + 0.03, 0), mat='acero_oscuro', pieza=p, marco=p, eje='y')
    for u, v in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        m.caja((0.04 if v else 0.01, 0.032, 0.04 if u else 0.01), en=(u * (a / 2 + 0.013), -L / 2 + 0.03, v * (b / 2 + 0.013)), mat='laton', pieza=p, marco=p, bisel=0.002, seg=1)
