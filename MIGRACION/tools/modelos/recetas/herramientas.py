"""What the suit holds in its hands (`assets/defs/gear.jsonc`, `"modelo"`). A tool is built in
the frame it is held in — x to the left, y up, z ahead, its origin where the game holds it
(`crates/app/src/gear.rs`: `WELDER_AT`, `LAUNCHER_AT`) — and it wears its own colours: nothing of
it is a `TINTE`. What moves or lights (a screen's letters, a tip glowing, a lamp) the game draws
over it, in the places these recipes leave for them.

A tool's pieces: its body is the piece without a name; what the game moves on it (a trigger, a
hatch, the rocket in its chamber) is a piece of its own, built where it rests, its origin its
pivot. The `.glb` has each piece in its own frame and says nothing of where that is: the origins
are the constants below (`SD_...` the welder's, `LZ_...` the launcher's), which the game copies.

The grips are made for a pressurised glove: 44 mm across, 115 mm long, raked back, a trigger
guard a gloved finger goes into (60 mm by 40 mm at the least), nothing sharp near them."""
import math

import bmesh
from mathutils import Matrix, Vector

from kit import M, Marco, receta, suavizar
from recetas.estilos import banda, superelipse


# ---------------------------------------------------------------------------------------------
# the trade's tools


def tramo(m, p, secciones, mat, n=20, e=2.6, liso=60.0):
    """A body along z: `secciones` are (z, half width, half height, y of its centre)."""
    anillos = [[(a, cy + b, z) for a, b in superelipse(sx, sy, n, e)] for z, sx, sy, cy in secciones]
    return m.piel(anillos, mat=mat, pieza=p, liso=liso)


def afilar(ob, bisel=0.001, seg=2, angulo=22.0, liso=35.0):
    """Breaks the corners of a faceted thing: every edge that bends more than `angulo` rounded
    by `bisel`, the flats left flat."""
    me = ob.data
    bm = bmesh.new()
    bm.from_mesh(me)
    lim = math.radians(angulo)
    filos = [e for e in bm.edges if len(e.link_faces) == 2 and e.calc_face_angle(0.0) > lim]
    if filos and bisel > 0:
        bmesh.ops.bevel(bm, geom=filos, offset=bisel, segments=seg, profile=0.5, affect='EDGES', clamp_overlap=True)
    bm.to_mesh(me)
    bm.free()
    suavizar(ob, liso)
    return ob


def ranura(m, ob, tam, en, mat='plastico_negro'):
    """A slot cut into `ob`: a box that size at `en`, the walls it leaves of `mat`."""
    corte = m.caja(tam, en=en, mat=ob.data.materials[0], pieza=ob['pieza'], bisel=0)
    # (what a cut leaves takes the material of the cutter's slot: the second of both)
    ob.data.materials.append(M(mat))
    corte.data.materials.append(M(mat))
    for cara in corte.data.polygons:
        cara.material_index = 1
    return m.restar(ob, corte)


def casco(m, p, secciones, mat, bisel=0.0012, seg=2, marco=None, liso=35.0):
    """A faceted hull: a skin over `secciones` — each (z, [(x, y)...]) or a ring of points —
    its corners broken."""
    anillos = [[(x, y, s[0]) for x, y in s[1]] if len(s) == 2 and not hasattr(s[0], '__len__') else list(s) for s in secciones]
    return afilar(m.piel(anillos, mat=mat, pieza=p, marco=marco, liso=liso), bisel, seg, liso=liso)


def arco(r, a0, a1, n=8, c=(0.0, 0.0)):
    """The points of an arc of radius `r` round `c`, from `a0` to `a1` (degrees from the first
    axis toward the second)."""
    return [(c[0] + r * math.cos(math.radians(a0 + (a1 - a0) * k / n)), c[1] + r * math.sin(math.radians(a0 + (a1 - a0) * k / n))) for k in range(n + 1)]


def redondear(pts, r, pasos=5):
    """A flat path with its corners rounded by `r`."""
    pts = [Vector(q) for q in pts]
    out = [pts[0]]
    for a, b, c in zip(pts, pts[1:], pts[2:]):
        d1, d2 = a - b, c - b
        k = min(r, d1.length * 0.45, d2.length * 0.45)
        p1, p2 = b + d1.normalized() * k, b + d2.normalized() * k
        for i in range(pasos + 1):
            t = i / pasos
            out.append((1 - t) ** 2 * p1 + 2 * (1 - t) * t * b + t ** 2 * p2)
    out.append(pts[-1])
    return out


def desplazar(camino, grosor, hacia=(0.0, 0.0)):
    """A flat path moved `grosor` sideways, to the side `hacia` is on (seen from its middle)."""
    pts = [Vector(q) for q in camino]
    normales = []
    for a, b in zip(pts, pts[1:]):
        t = (b - a).normalized()
        normales.append(Vector((-t.y, t.x)))
    k = len(normales) // 2
    if normales[k].dot(Vector(hacia) - (pts[k] + pts[k + 1]) * 0.5) < 0:
        normales = [-n for n in normales]
    out = []
    for i, q in enumerate(pts):
        a, b = normales[max(i - 1, 0)], normales[min(i, len(normales) - 1)]
        s = (a + b).normalized()
        out.append(q + s * (grosor / max(s.dot(a), 0.4)))
    return out


def chapa(m, p, camino, grosor, d0, d1, mat, plano='xy', hacia=(0.0, 0.0), bisel=0.0007, seg=2, sesgo=(0.0, 0.0), medias=(), marco=None, liso=35.0):
    """A plate bent along `camino` — points (x, y) pushed along z, or (`plano` 'zy') points
    (z, y) pushed along x — `grosor` thick to the side `hacia` is on, from `d0` to `d1`.
    `camino` may be a function of the place along it (a plate over a body that tapers: `medias`
    the places between where that body bends); `sesgo`: how much each end leans with y (a plate
    cut on the slant)."""
    f = camino if callable(camino) else (lambda d: camino)
    anillos = []
    for d, s in zip([d0] + list(medias) + [d1], [sesgo[0]] + [0.0] * len(medias) + [sesgo[1]]):
        fuera = [Vector(q) for q in f(d)]
        if s:
            fuera = [Vector(f(d + s * q.y)[i]) for i, q in enumerate(fuera)]
        pts = fuera + desplazar(fuera, grosor, hacia)[::-1]
        anillos.append([(q.x, q.y, d + s * q.y) for q in pts] if plano == 'xy' else [(d, q.y, q.x) for q in pts])
    return afilar(m.piel(anillos, mat=mat, pieza=p, marco=marco, liso=liso), bisel, seg, liso=liso)


def encoger(pts, d):
    """A convex outline with every side moved `d` in."""
    pts = [Vector(q) for q in pts]
    n = len(pts)
    c = sum(pts, Vector((0.0, 0.0))) / n
    lados = []
    for i in range(n):
        a, b = pts[i], pts[(i + 1) % n]
        t = (b - a).normalized()
        k = Vector((-t.y, t.x))
        if k.dot(c - a) < 0:
            k = -k
        lados.append((a + k * d, t))
    out = []
    for i in range(n):
        (p1, t1), (p2, t2) = lados[i - 1], lados[i]
        den = t1.x * t2.y - t1.y * t2.x
        s = ((p2.x - p1.x) * t2.y - (p2.y - p1.y) * t2.x) / den
        out.append(tuple(p1 + t1 * s))
    return out


def franja(m, p, r0, r1, z0, z1, n=12, mats=('pintura_amarilla', 'plastico_negro'), marco=None):
    """A hazard band round z: `n` blocks of two colours in turn."""
    paso = 360.0 / n
    for k in range(n):
        m.torno([(r0, z0), (r1, z0), (r1, z1), (r0, z1), (r0, z0)], mat=mats[k % 2], pieza=p, marco=(marco or Marco()).girado((0, 0, paso * k)), eje='z', lados=3, arco=paso)


def empunadura(m, p, arriba, abajo, ancho=0.044, fondo=0.052, entra=0.012, mat='goma'):
    """A grip for a pressurised glove, from `arriba` (the middle of its top, where it leaves
    the body) down to `abajo` (the middle of its butt): `ancho` across, `fondo` fore and aft,
    ribbed rubber between a hard collar and a flared butt."""
    a, b = Vector(arriba), Vector(abajo)
    largo = (b - a).length
    d = (b - a) / largo
    u = Vector((1.0, 0.0, 0.0))
    w = d.cross(u).normalized()
    if w.z < 0:
        w = -w

    def anillo(s, k):
        c = a + d * s
        return [tuple(c + u * (x * k) + w * (z * k)) for x, z in superelipse(ancho / 2, fondo / 2, 20, 2.5)]

    # the rubber: a swell for the palm, ribs across it
    est = [(-entra, 1.0), (0.0, 1.0)]
    n = 36
    for k in range(1, n):
        t = k / n
        s = 1.0 + 0.045 * math.sin(t * math.pi)
        if 0.1 < t < 0.9 and k % 4 in (2, 3):
            s -= 0.035
        est.append((largo * t, s))
    est.append((largo, 1.0))
    m.piel([anillo(s, k) for s, k in est], mat=mat, pieza=p, liso=60.0)
    # the collar where it leaves the body, the butt
    m.piel([anillo(s, k) for s, k in ((-entra, 1.05), (0.005, 1.05), (0.009, 0.99))], mat='plastico_negro', pieza=p, liso=50.0)
    m.piel([anillo(s, k) for s, k in ((largo - 0.004, 0.99), (largo - 0.001, 1.08), (largo + 0.005, 1.08), (largo + 0.008, 1.0))], mat='plastico_negro', pieza=p, liso=50.0)
    return d


# ---------------------------------------------------------------------------------------------
# the welder

# its place and its screen's as the game turns it to the eye (gear.rs: `show`)
SOLDADOR_EN = Vector((-0.23, -0.2, 0.46))
PANTALLA = Vector((0.03, 0.078, -0.02))

# its trigger: the pin it turns about (its piece's origin), along x; squeezed, it turns
# `SD_GATILLO_RECORRE` degrees about +x (its blade back, toward the grip)
SD_PIVOTE = Vector((0.0, -0.027, 0.012))
SD_GATILLO_RECORRE = 14.0
# its grip: from the middle of its top to the middle of its butt
SD_EMPUNADURA = (Vector((0.0, -0.031, -0.03)), Vector((0.0, -0.1421, -0.0598)))


def marco_pantalla():
    c = SOLDADOR_EN + PANTALLA
    n = (-c).normalized()
    u = (Vector((-1, 0, 0)) - n * Vector((-1, 0, 0)).dot(n)).normalized()
    v = n.cross(u)
    return Marco(PANTALLA, Matrix((u, v, n)).transposed()), n


@receta('soldador')
def soldador(m):
    """The welder-scanner: a yellow housing with its battery clipped behind, a ceramic nozzle
    with a copper tip on a finned neck, the scanner's lens under it, a rubber grip a glove
    takes, its trigger (a piece of its own) in a wide guard, and its screen on a ball arm,
    turned to the eye."""
    p = m.pieza('', (0.2, 0.7, 0.75))
    gatillo = m.pieza('gatillo', (0.014, 0.045, 0.02), SD_PIVOTE)
    # the housing, tapering to the nozzle; a slot under it for the trigger
    carcasa = tramo(m, p, [(-0.086, 0.019, 0.023, 0.0), (-0.078, 0.025, 0.03, 0.0), (-0.02, 0.026, 0.031, 0.0), (0.035, 0.026, 0.031, 0.0), (0.062, 0.022, 0.026, 0.004), (0.082, 0.016, 0.019, 0.007), (0.086, 0.012, 0.014, 0.008)], 'pintura_amarilla', e=3.0)
    ranura(m, carcasa, (0.014, 0.022, 0.022), (0, -0.0305, 0.012))
    # black bumpers round its ends
    tramo(m, p, [(-0.09, 0.02, 0.024, 0.0), (-0.084, 0.0265, 0.0315, 0.0), (-0.074, 0.0265, 0.0315, 0.0), (-0.072, 0.0255, 0.0305, 0.0)], 'goma', e=3.0)
    tramo(m, p, [(0.03, 0.0265, 0.0315, 0.0), (0.032, 0.027, 0.032, 0.0), (0.04, 0.027, 0.032, 0.0005), (0.042, 0.0255, 0.0305, 0.001)], 'goma', e=3.0)
    # cooling slots down each side, a data plate, screws
    for lado in (1, -1):
        for k in range(6):
            m.caja((0.0024, 0.03, 0.0035), en=(lado * 0.0258, 0.004, -0.058 + k * 0.009), mat='acero_oscuro', pieza=p, bisel=0.0008, seg=1)
        m.tornillos([(lado * 0.0262, y, z) for y in (-0.02, 0.021) for z in (-0.066, 0.02)], 0.0026, 0.001, 'acero', p, None, eje=(lado, 0, 0))
    m.caja((0.0012, 0.013, 0.026), en=(0.0262, -0.012, -0.03), mat='aluminio', pieza=p, bisel=0.0003, seg=1)
    # the battery, clipped on behind: three lamps on it
    m.caja((0.044, 0.05, 0.036), en=(0, -0.003, -0.108), mat='plastico_negro', pieza=p, bisel=0.007, seg=3)
    m.caja((0.03, 0.006, 0.012), en=(0, 0.024, -0.104), mat='pintura_naranja', pieza=p, bisel=0.002, seg=1)
    for k in range(3):
        m.piloto((0.012 - k * 0.012, 0.01, -0.1265), 0.0028, 'piloto_verde' if k else 'piloto_ambar', p, None, eje=(0, 0, -1))
    # a rail along its top
    m.caja((0.014, 0.006, 0.1), en=(0, 0.0335, -0.022), mat='acero_oscuro', pieza=p, bisel=0.0015, seg=1)
    for k in range(7):
        m.caja((0.016, 0.003, 0.005), en=(0, 0.0372, -0.062 + k * 0.0135), mat='acero_oscuro', pieza=p, bisel=0.0008, seg=1)
    # the nozzle: a collar, a finned neck, a ceramic shroud, a copper tip
    y = 0.008
    m.torno([(0.0, 0.082), (0.0145, 0.082), (0.0155, 0.086), (0.0155, 0.098), (0.012, 0.102), (0.0085, 0.104), (0.0085, 0.136)], en=(0, y, 0), mat='acero', pieza=p, eje='z', lados=20)
    for k in range(5):
        banda(m, p, 0.008, 0.0125, 0.108 + k * 0.0055, 0.1105 + k * 0.0055, 'aluminio', en=(0, y, 0), eje='z', lados=18, marco=Marco())
    m.torno([(0.0085, 0.134), (0.0125, 0.138), (0.0125, 0.15), (0.0075, 0.167), (0.004, 0.167), (0.004, 0.15)], en=(0, y, 0), mat='plastico_claro', pieza=p, eje='z', lados=20)
    m.torno([(0.0038, 0.16), (0.0034, 0.17), (0.0016, 0.178), (0.0, 0.179)], en=(0, y, 0), mat='cobre', pieza=p, eje='z', lados=12)
    # the gas line, round the right side to the collar
    m.tubo([(-0.02, 0.012, -0.092), (-0.031, 0.014, -0.07), (-0.031, 0.015, 0.045), (-0.02, 0.012, 0.078), (-0.013, 0.01, 0.09)], 0.0024, 'cobre', p, None, lados=8, codo=0.012)
    for z in (-0.05, 0.0, 0.04):
        m.caja((0.006, 0.008, 0.005), en=(-0.0285, 0.0145, z), mat='acero', pieza=p, bisel=0.001, seg=1)
    # the scanner's lens under the nozzle
    m.torno([(0.0, 0.06), (0.0105, 0.06), (0.0105, 0.094), (0.0085, 0.094), (0.0085, 0.09), (0.0, 0.09)], en=(0, -0.016, 0), mat='acero_oscuro', pieza=p, eje='z', lados=16)
    m.esfera(0.0082, en=(0, -0.016, 0.0885), mat='luz_cian', pieza=p, lados=12, de=0, eje='z', aplastar=0.35)
    # the grip, for a glove; the guard a gloved finger goes into, from under the lens back to it
    d = empunadura(m, p, *SD_EMPUNADURA, fondo=0.05)
    chapa(m, p, redondear([(0.056, -0.018), (0.056, -0.0715), (-0.012, -0.0715), (-0.022, -0.063)], 0.013), 0.0035, -0.009, 0.009, 'acero_oscuro', plano='zy', hacia=(0.2, -0.3), bisel=0.001)
    # the trigger: a blade on its pin, up through the slot (a bezel round it)
    for x in (-0.0092, 0.0092):
        m.caja((0.0042, 0.0022, 0.03), en=(x, -0.0313, 0.012), mat='plastico_negro', pieza=p, bisel=0.0008, seg=1)
    for z in (-0.0012, 0.0252):
        m.caja((0.0226, 0.0022, 0.0042), en=(0, -0.0313, z), mat='plastico_negro', pieza=p, bisel=0.0008, seg=1)
    m.cilindro(0.0036, 0.011, en=SD_PIVOTE, mat='acero', pieza=gatillo, eje='x', lados=12, bisel=0.0007)
    cara = redondear([(0.0142, -0.0245), (0.0146, -0.039), (0.0162, -0.049), (0.0195, -0.057), (0.0235, -0.0615)], 0.008)
    chapa(m, gatillo, cara, 0.0052, -0.006, 0.006, 'pintura_roja', plano='zy', hacia=(-0.05, -0.045), bisel=0.0011)
    # the cable to the suit, off its butt
    pie = SD_EMPUNADURA[1] + d * 0.008
    m.torno([(0.0, pie.y - 0.009), (0.006, pie.y - 0.009), (0.007, pie.y - 0.003), (0.007, pie.y + 0.001)], en=(0, 0, pie.z), mat='acero', pieza=p, lados=10)
    m.manguera((0, pie.y - 0.007, pie.z), (0.06, -0.36, -0.3), 0.0042, comba=0.03, hacia=(0, -1, 0.4), mat='goma', pieza=p)
    # the screen's housing on its ball arm
    marco, n = marco_pantalla()
    w, h = 0.09, 0.056
    m.caja((w + 0.016, h + 0.016, 0.012), en=(0, 0, -0.007), mat='plastico_negro', pieza=p, marco=marco, bisel=0.004, seg=2)
    for u, v, tam in ((0, 1, (w + 0.016, 0.005, 0.006)), (0, -1, (w + 0.016, 0.005, 0.006)), (1, 0, (0.005, h + 0.008, 0.006)), (-1, 0, (0.005, h + 0.008, 0.006))):
        m.caja(tam, en=(u * (w / 2 + 0.0055), v * (h / 2 + 0.0055), 0.001), mat='goma', pieza=p, marco=marco, bisel=0.0015, seg=1)
    # two keys under it and a lamp
    for k in (-1, 1):
        m.caja((0.012, 0.004, 0.003), en=(k * 0.03, -h / 2 - 0.0055, 0.0042), mat='plastico_gris', pieza=p, marco=marco, bisel=0.001, seg=1)
    m.piloto((0, -h / 2 - 0.0055, 0.004), 0.0018, 'piloto_verde', p, marco, eje='z')
    espalda = PANTALLA - n * 0.016
    m.esfera(0.007, en=espalda, mat='acero', pieza=p, lados=12)
    codo = Vector((0.012, 0.058, -0.036))
    m.tubo([espalda, codo, (0, 0.04, -0.03)], 0.0036, 'acero_oscuro', p, None, lados=8, codo=0.008)
    m.cilindro(0.0075, 0.009, en=(0, 0.04, -0.03), mat='acero', pieza=p, lados=12, bisel=0.002)


# ---------------------------------------------------------------------------------------------
# the rocket launcher

# where the game holds it, from the eye: on the right shoulder (the eye is at -this in its frame)
LANZADOR_EN = Vector((-0.29, -0.13, 0.12))

LZ_BOCA, LZ_COLA = 0.46, -0.42          # the muzzle's rim and the venturi's, along the bore (z)
LZ_ANIMA = 0.0297                       # the bore's radius
LZ_CALIBRE, LZ_COHETE_LARGO = 0.055, 0.30
# the rocket loaded: its centre (its piece's origin), its nose ahead; it is dropped in from above
LZ_COHETE = Vector((0.0, 0.0, 0.195))
# the chamber: an open tray from `LZ_RECAMARA[0]` to `[1]` along z, its ledge (where the hatch
# rests) at y `LZ_REPISA`, the deck round it at `LZ_CUBIERTA`
LZ_RECAMARA = (0.041, 0.349)
LZ_REPISA, LZ_CUBIERTA = 0.042, 0.050
# the hatch: the middle of its hinge line (its piece's origin), along x at its REAR edge; it
# opens upward and back, turning `LZ_PUERTA_ABRE` degrees about -x
LZ_BISAGRA = Vector((0.0, 0.055, 0.030))
LZ_PUERTA_ABRE = 105.0
# the trigger: its pin (its piece's origin), along x; squeezed, it turns `LZ_GATILLO_RECORRE`
# degrees about +x (its blade back, toward the grip)
LZ_PIVOTE = Vector((0.0, -0.0545, 0.142))
LZ_GATILLO_RECORRE = 14.0
# the grips, from the middle of the top to the middle of the butt: the right hand's (raked back
# 15 degrees), the left hand's under the front (20 degrees)
LZ_EMPUNADURA = (Vector((0.0, -0.060, 0.088)), Vector((0.0, -0.1711, 0.0582)))
LZ_GUARDAMANO = (Vector((0.0, -0.056, 0.333)), Vector((0.0, -0.1594, 0.2954)))
# the sight's display: the middle of its window, 0.032 wide and 0.020 tall, turned to the eye
# (no wider: it stands between the hatch's swing, 41 mm off the bore, and the helmet's 85)
LZ_PANTALLA = Vector((0.0628, 0.076, 0.199))
LZ_PANTALLA_TAM = (0.032, 0.020)
# the seat of the lamp the game lights (ready, loading): a ball of radius 0.007 there
LZ_LAMPARA = Vector((0.0635, 0.101, 0.236))

# how much the body's two end faces lean forward with height (z per y)
LZ_INCLINA = 0.22
# its body's outline along z: (z, (half width, top, bottom, the top corners' cut across and
# down, the bottom corners' across and up)) — full where the chamber is, tapering at its ends
LZ_SECCIONES = [
    (-0.100, (0.0405, 0.0455, -0.0415, 0.0120, 0.0190, 0.0150, 0.0190)),
    (-0.058, (0.0480, 0.0500, -0.0460, 0.0135, 0.0230, 0.0180, 0.0240)),
    (0.358, (0.0480, 0.0500, -0.0460, 0.0135, 0.0230, 0.0180, 0.0240)),
    (0.404, (0.0425, 0.0455, -0.0412, 0.0125, 0.0195, 0.0160, 0.0195)),
]


def contorno(z, dentro=0.0):
    """The launcher's body's outline at `z` (eight points, from the top's left corner round by
    the left side), `dentro` in from its skin."""
    s = LZ_SECCIONES
    z = min(max(z, s[0][0]), s[-1][0])
    for (z0, a), (z1, b) in zip(s, s[1:]):
        if z <= z1:
            t = (z - z0) / (z1 - z0)
            w, yt, yb, ctx, cty, cbx, cby = (x + (y - x) * t for x, y in zip(a, b))
            break
    pts = [(w - ctx, yt), (w, yt - cty), (w, yb + cby), (w - cbx, yb), (-(w - cbx), yb), (-w, yb + cby), (-w, yt - cty), (-(w - ctx), yt)]
    return encoger(pts, dentro) if dentro else pts


def marco_visor():
    """The launcher's display: its frame (x across it, y up it, z out of it, to the eye)."""
    c = LANZADOR_EN + LZ_PANTALLA
    n = (-c).normalized()
    u = (Vector((-1, 0, 0)) - n * Vector((-1, 0, 0)).dot(n)).normalized()
    v = n.cross(u)
    return Marco(LZ_PANTALLA, Matrix((u, v, n)).transposed()), n


def bobina(m, p, z0, largo, r0, r1, ala=0.0035):
    """A coil round the barrel: a copper winding between two flanges."""
    n = max(2, int(round((largo - 0.008) / 0.004)))
    paso = (largo - 0.008) / n
    perfil = [(r0, z0 + 0.004)]
    for k in range(n):
        z = z0 + 0.004 + paso * k
        perfil += [(r1 - 0.0009, z), (r1, z + paso * 0.5)]
    perfil += [(r1 - 0.0009, z0 + largo - 0.004), (r0, z0 + largo - 0.004), perfil[0]]
    m.torno(perfil, mat='cobre', pieza=p, eje='z', lados=20, liso=50.0)
    for z in (z0, z0 + largo - 0.004):
        banda(m, p, r0, r1 + ala, z, z + 0.004, 'aluminio_anodizado', eje='z', lados=20, marco=Marco())


@receta('lanzacohetes')
def lanzacohetes(m):
    """The rocket launcher: a barrel under a faceted shroud of white ceramic and carbon panels,
    its chamber an open tray under a hatch on top (hinged at its rear edge), a vented heat
    shield round its muzzle, a stage of coils and a vaned venturi behind, a thermal sight up on
    its left with its display turned to the eye, two grips a glove takes, a pad for the
    shoulder. Its pieces: the body, the hatch (`puerta`), the rocket in its chamber (`cohete`),
    the trigger (`gatillo`)."""
    p = m.pieza('', (0.2, 0.3, 0.9))
    puerta = m.pieza('puerta', (0.08, 0.072, 0.67), LZ_BISAGRA)
    cohete = m.pieza('cohete', (LZ_CALIBRE, LZ_CALIBRE, LZ_COHETE_LARGO), LZ_COHETE)
    gatillo = m.pieza('gatillo', (0.016, 0.05, 0.03), LZ_PIVOTE)
    # (the hatch opens and the rocket is taken in the hand: each shaded on its own)
    m.sombra_aparte = ('puerta', 'cohete')
    lz_cuerpo(m, p)
    lz_recamara(m, p)
    lz_canon(m, p)
    lz_bajos(m, p, gatillo)
    lz_mira(m, p)
    lz_puerta(m, p, puerta)
    lz_cohete(m, cohete)


def lz_cuerpo(m, p):
    """The shroud: a steel core with the bore through it and the chamber cut open on top,
    panels over it — white above, carbon below — with a gap between each and its fasteners."""
    z0, z1 = LZ_RECAMARA

    def seccion(z):
        return [(x, y, z + LZ_INCLINA * y) for x, y in contorno(z, 0.0025)]

    nucleo = casco(m, p, [seccion(-0.100), (-0.058, contorno(-0.058, 0.0025)), (0.358, contorno(0.358, 0.0025)), seccion(0.404)], 'acero_oscuro', bisel=0.001)
    m.taladrar(nucleo, [(0, 0, 0.152)], LZ_ANIMA, 0.54, eje='z', lados=32)
    m.restar(nucleo, m.caja((2 * LZ_ANIMA - 0.0006, 0.07, z1 - z0), en=(0, 0.035, (z0 + z1) / 2), mat='acero_oscuro', pieza=p, bisel=0))
    # (the rebate the hatch closes into: a ledge each side of the tray)
    m.restar(nucleo, m.caja((0.12, 0.03, 0.3235), en=(0, LZ_REPISA + 0.015, 0.19625), mat='acero_oscuro', pieza=p, bisel=0))

    def lado_alto(s):
        def f(z):
            q = contorno(z)
            t = (q[0][1] - 0.0405) / (q[0][1] - q[1][1])
            return [(s * (q[0][0] + (q[1][0] - q[0][0]) * t), 0.0405), (s * q[1][0], q[1][1]), (s * q[1][0], 0.0045)]
        return f

    def silla(z):
        q = contorno(z)
        return [(q[1][0], 0.0045), q[1], q[0], q[7], q[6], (q[6][0], 0.0045)]

    def cuna(z):
        q = contorno(z)
        return [(q[2][0], 0.0005), q[2], q[3], q[4], q[5], (q[5][0], 0.0005)]

    g, s = 0.003, 0.5
    # beside the hatch: three panels a side, cut on the slant; a tub under each
    cortes = [(0.0345, 0.0), (0.139, s), (0.250, s), (0.3565, 0.0)]
    for (a, sa), (b, sb) in zip(cortes, cortes[1:]):
        a, b = a + (0.001 if sa else 0.0), b - (0.001 if sb else 0.0)
        for lado in (1, -1):
            chapa(m, p, lado_alto(lado), g, a, b, 'pintura_blanca', sesgo=(sa, sb))
            m.tornillos([(lado * 0.048, y, z) for y in (0.0105, 0.0225) for z in (a + sa * y + 0.007, b + sb * y - 0.007)], 0.0021, 0.0007, 'acero', p, None, eje=(lado, 0, 0))
            m.tornillos([(lado * 0.048, y, z) for y in (-0.005, -0.017) for z in (a + sa * y + 0.007, b + sb * y - 0.007)], 0.0021, 0.0007, 'acero', p, None, eje=(lado, 0, 0))
        chapa(m, p, cuna, g, a, b, 'carbono', sesgo=(sa, sb))
    # behind the hatch and ahead of it: a saddle over the top, a tub under; its end faces lean
    # forward as the cuts between its panels do
    inc = LZ_INCLINA
    for a, b, sa, sb, arriba in ((-0.0975, -0.0595, inc, 0.0, 'pintura_blanca'), (-0.0575, 0.0325, 0.0, 0.0, 'pintura_blanca'), (0.3585, 0.4025, 0.0, inc, 'titanio')):
        chapa(m, p, silla, g, a, b, arriba, sesgo=(sa, sb))
        chapa(m, p, cuna, g, a, b, 'carbono', sesgo=(sa, sb))
        for lado in (1, -1):
            m.tornillos([(lado * (contorno(z)[1][0] - 0.0002), y, z) for y in (0.0105, -0.012) for z in (a + sa * y + 0.006, b + sb * y - 0.006)], 0.0021, 0.0007, 'acero', p, None, eje=(lado, 0, 0))
        m.tornillos([(x, contorno(z)[0][1] - 0.0002, z) for x in (-0.024, 0.024) for z in (a + sa * 0.046 + 0.006, b + sb * 0.046 - 0.006)], 0.0021, 0.0007, 'acero', p, None, eje='y')
    # a line of lamps in the seam between white and carbon, along the front half
    for lado in (1, -1):
        for k in range(10):
            m.caja((0.0012, 0.0022, 0.0105), en=(lado * 0.0461, 0.0025, 0.218 + k * 0.0135), mat='luz_cian', pieza=p, bisel=0)
    # the status strip on the deck behind the hatch
    m.caja((0.013, 0.0016, 0.05), en=(0, 0.0502, -0.022), mat='plastico_negro', pieza=p, bisel=0.0005, seg=1)
    for k in range(5):
        m.caja((0.008, 0.001, 0.0062), en=(0, 0.0512, -0.040 + k * 0.009), mat='luz_cian', pieza=p, bisel=0.0003, seg=1)
    # the data plate on its left, with its rivets and its lines of letters
    m.caja((0.001, 0.017, 0.042), en=(0.0483, 0.0155, 0.088), mat='aluminio', pieza=p, bisel=0.0003, seg=1)
    m.tornillos([(0.0488, 0.0155 + a * 0.0065, 0.088 + b * 0.0185) for a in (-1, 1) for b in (-1, 1)], 0.0009, 0.0004, 'acero', p, None, eje='x', lados=5)
    for k, largo in enumerate((0.03, 0.022, 0.026)):
        m.caja((0.0003, 0.0016, largo), en=(0.04885, 0.0195 - k * 0.004, 0.073 + largo / 2), mat='plastico_negro', pieza=p, bisel=0)
    # a warning mark on the panel ahead of it, either side: an orange block, a black bar
    for lado in (1, -1):
        m.caja((0.0006, 0.012, 0.022), en=(lado * 0.0481, 0.016, 0.300 if lado < 0 else 0.172), mat='pintura_naranja', pieza=p, bisel=0)
        m.caja((0.0006, 0.003, 0.022), en=(lado * 0.0482, 0.0105, 0.300 if lado < 0 else 0.172), mat='plastico_negro', pieza=p, bisel=0)
    for lado in (1, -1):
        for i, largo in enumerate((0.03, 0.02)):
            m.caja((0.0005, 0.0016, largo), en=(lado * 0.0481, 0.036 - 0.0125 - i * 0.0034, 0.312 + largo / 2), mat='pintura_oscura', pieza=p, bisel=0)
    # on its right: a louvred vent, the arming lever, the sling's stud and ring
    m.caja((0.0012, 0.02, 0.058), en=(-0.0484, 0.0155, 0.196), mat='plastico_negro', pieza=p, bisel=0.0004, seg=1)
    m.rejilla((0.015, 0.05), en=(-0.0482, 0.0155, 0.196), mat='acero_oscuro', pieza=p, lamas=9, grosor=0.0012, normal='x')
    m.cilindro(0.008, 0.004, en=(-0.05, 0.014, 0.082), mat='acero_oscuro', pieza=p, eje='x', lados=14, bisel=0.001)
    m.caja((0.004, 0.007, 0.024), en=(-0.0535, 0.014, 0.091), mat='pintura_naranja', pieza=p, bisel=0.0016, seg=2)
    m.cilindro(0.0045, 0.009, en=(-0.049, -0.012, 0.380), mat='acero', pieza=p, eje='x', lados=10, bisel=0.001)
    m.tubo([(-0.0545, -0.022 + 0.010 * math.sin(math.tau * k / 14), 0.380 + 0.010 * math.cos(math.tau * k / 14)) for k in range(15)], 0.0018, 'acero', p, None, lados=6, tapas=False)


def lz_recamara(m, p):
    """What is in the tray: two rails the rocket lies on, the contacts for its bands, a warning
    painted along each wall, the hinge's knuckles and the latch's keeper on the deck."""
    z0, z1 = LZ_RECAMARA
    r = LZ_ANIMA
    chapa(m, p, arco(r - 0.0008, 180, 360, 14), 0.0012, z0 + 0.0005, z1 - 0.0005, 'aluminio', hacia=(0, -1), bisel=0.0003, seg=1)
    for a in (-128.0, -52.0):
        c, s = math.cos(math.radians(a)), math.sin(math.radians(a))
        m.caja((0.005, 0.003, z1 - z0 - 0.008), en=((r - 0.0001) * c, (r - 0.0001) * s, (z0 + z1) / 2), mat='cromo', pieza=p, bisel=0.0006, seg=1, rot=(0, 0, a + 90))
    for z in (z0 + 0.0004, z1 - 0.0004):
        m.caja((0.022, 0.0052, 0.0022), en=(0, 0.0362, z), mat='plastico_negro', pieza=p, bisel=0.0006, seg=1)
        m.caja((0.018, 0.0028, 0.0012), en=(0, 0.0362, z + (0.001 if z < 0.2 else -0.001)), mat='luz_cian', pieza=p, bisel=0)
    for lado in (1, -1):
        x = lado * (r - 0.0003)
        # the contacts, on their carrier
        m.caja((0.0012, 0.013, 0.036), en=(x, 0.011, 0.150), mat='plastico_negro', pieza=p, bisel=0)
        for z in (0.140, 0.160):
            m.caja((0.0024, 0.008, 0.005), en=(lado * (r - 0.0006), 0.011, z), mat='laton', pieza=p, bisel=0.0005, seg=1)
        # the warning: a band of yellow and black blocks under the ledge, an orange label
        for k in range(18):
            m.caja((0.0008, 0.0075, 0.0125), en=(x, 0.0365, 0.085 + k * 0.0125), mat='pintura_amarilla' if k % 2 else 'plastico_negro', pieza=p, bisel=0)
        m.caja((0.0008, 0.011, 0.03), en=(x, 0.0335, 0.062), mat='pintura_naranja', pieza=p, bisel=0)
        for k in range(3):
            m.caja((0.0004, 0.0012, 0.022), en=(lado * (r - 0.0008), 0.037 - k * 0.0032, 0.062), mat='plastico_negro', pieza=p, bisel=0)
        # the ledge's seal
        m.caja((0.0035, 0.0008, z1 - z0 - 0.012), en=(lado * 0.0335, LZ_REPISA + 0.0001, (z0 + z1) / 2), mat='goma', pieza=p, bisel=0.0003, seg=1)
        # the hinge's knuckles on the deck, their pin's heads
        m.cilindro(0.0045, 0.011, en=(lado * 0.0255, LZ_BISAGRA.y, LZ_BISAGRA.z), mat='acero_oscuro', pieza=p, eje='x', lados=14, bisel=0.001)
        m.caja((0.011, 0.007, 0.012), en=(lado * 0.0255, 0.0520, 0.0255), mat='acero_oscuro', pieza=p, bisel=0.0014, seg=1)
        m.cilindro(0.0026, 0.0024, en=(lado * 0.0318, LZ_BISAGRA.y, LZ_BISAGRA.z), mat='acero', pieza=p, eje='x', lados=8, bisel=0.0006)
    # the latch's keeper ahead of the hatch
    m.caja((0.02, 0.003, 0.005), en=(0, 0.0512, 0.3685), mat='acero', pieza=p, bisel=0.0008, seg=1)


def lz_canon(m, p):
    """The barrel fore and aft of the body: the muzzle in its vented heat shield, the stage of
    coils behind with its spine, keel and bus bars, the venturi with its vanes."""
    r, R = LZ_ANIMA, 0.0345
    # ---- the muzzle
    m.torno([(r - 0.0003, 0.392), (R, 0.392), (R, 0.4462), (r - 0.0003, 0.4462), (r - 0.0003, 0.392)], mat='acero_oscuro', pieza=p, eje='z', lados=32)
    bobina(m, p, 0.412, 0.03, R, 0.0372, ala=0.002)
    banda(m, p, R, 0.0436, 0.404, 0.409, 'acero_oscuro', eje='z', lados=32, marco=Marco())
    banda(m, p, 0.0436, 0.0442, 0.4055, 0.4075, 'luz_cian', eje='z', lados=32, marco=Marco())
    escudo = m.torno([(0.0402, 0.408), (0.0436, 0.408), (0.0436, 0.4475), (0.0402, 0.4475), (0.0402, 0.408)], mat='pintura_blanca', pieza=p, eje='z', lados=32)
    for k in range(4):
        m.taladrar(escudo, [(0, 0, 0.428)], 0.0034, 0.12, eje='x', marco=Marco().girado((0, 0, 22.5 + 45 * k)), lados=10, forma='ranura', alto=0.027)
    corona = m.torno([(r, 0.446), (0.0450, 0.446), (0.0468, 0.448), (0.0468, 0.4565), (0.0440, LZ_BOCA), (0.0340, LZ_BOCA), (r, 0.4555), (r, 0.446)], mat='titanio', pieza=p, eje='z', lados=32)
    for k in range(3):
        m.taladrar(corona, [(0, 0, LZ_BOCA + 0.0012)], 0.005, 0.12, eje='x', marco=Marco().girado((0, 0, 60 * k)), lados=12)
    franja(m, p, 0.0466, 0.0474, 0.4486, 0.4552)
    # ---- behind the body: a collar, the barrel, five coils with a lit ring between each
    m.torno([(r - 0.0003, -0.3255), (R, -0.3255), (R, -0.088), (r - 0.0003, -0.088), (r - 0.0003, -0.3255)], mat='acero_oscuro', pieza=p, eje='z', lados=32)
    banda(m, p, R, 0.0440, -0.118, -0.099, 'acero_oscuro', eje='z', lados=32, marco=Marco())
    m.tornillos([(0.0395 * math.cos(math.tau * (k + 0.5) / 8), 0.0395 * math.sin(math.tau * (k + 0.5) / 8), -0.118) for k in range(8)], 0.0028, 0.002, 'acero', p, None, eje=(0, 0, -1))
    for k in range(5):
        z = -0.318 + k * 0.040
        bobina(m, p, z, 0.032, R, 0.0420)
        banda(m, p, R, 0.0368, z + 0.0345, z + 0.0375, 'luz_cian', eje='z', lados=20, marco=Marco())
    # its spine (white, cut on the slant behind), its keel, a bus bar each side on its insulators
    casco(m, p, [[(0.011, 0.0425, -0.328), (0.008, 0.0555, -0.316), (-0.008, 0.0555, -0.316), (-0.011, 0.0425, -0.328)], (-0.100, [(0.011, 0.0425), (0.008, 0.0555), (-0.008, 0.0555), (-0.011, 0.0425)])], 'pintura_blanca', bisel=0.001)
    m.tornillos([(0, 0.0553, z) for z in (-0.300, -0.250, -0.200, -0.150, -0.112)], 0.0024, 0.0008, 'acero', p, None, eje='y')
    casco(m, p, [(-0.330, [(0.010, -0.0425), (0.0075, -0.056), (-0.0075, -0.056), (-0.010, -0.0425)]), (-0.100, [(0.010, -0.0425), (0.0075, -0.056), (-0.0075, -0.056), (-0.010, -0.0425)])], 'aluminio_anodizado', bisel=0.001)
    for lado in (1, -1):
        m.caja((0.004, 0.013, 0.214), en=(lado * 0.0485, 0, -0.222), mat='aluminio', pieza=p, bisel=0.0012, seg=1)
        for k in range(5):
            m.caja((0.006, 0.009, 0.01), en=(lado * 0.0452, 0, -0.302 + k * 0.040), mat='plastico_claro', pieza=p, bisel=0.0015, seg=1)
        m.caja((0.0045, 0.0145, 0.012), en=(lado * 0.0485, 0, -0.322), mat='pintura_naranja', pieza=p, bisel=0.0014, seg=1)
    # ---- the venturi: a throat ring, the cone, its liner, six vanes on a hub
    m.torno([(r, -0.325), (0.0425, -0.325), (0.0445, -0.328), (0.0445, -0.340), (0.0410, -0.346), (0.0562, -0.411), (0.0588, -0.413), (0.0588, LZ_COLA), (0.0552, LZ_COLA), (0.0536, -0.414), (0.0372, -0.350), (r, -0.336), (r, -0.325)], mat='titanio', pieza=p, eje='z', lados=32)
    m.torno([(r - 0.0006, -0.3365), (0.0366, -0.3505), (0.0530, -0.4145), (0.0540, -0.4185), (0.0546, -0.4185), (0.0537, -0.4142), (0.0373, -0.3502), (r, -0.3360), (r - 0.0006, -0.3365)], mat='tobera', pieza=p, eje='z', lados=32)
    m.torno([(0.0, -0.350), (0.0045, -0.356), (0.0082, -0.372), (0.0088, -0.400), (0.0070, -0.413), (0.0, -0.416)], mat='titanio', pieza=p, eje='z', lados=16)
    for k in range(6):
        marco = Marco().girado((0, 0, 60 * k + 30))
        m.extrusion([(-0.366, 0.0070), (-0.360, 0.0400), (-0.411, 0.0530), (-0.4135, 0.0060)], 0.0022, mat='titanio', pieza=p, marco=marco, eje='x', bisel=0.0005, seg=1)
        # (a rib down the cone outside, between the vanes)
        m.extrusion([(-0.342, 0.0420), (-0.348, 0.0450), (-0.404, 0.0585), (-0.4105, 0.0570), (-0.4105, 0.0540)], 0.004, mat='titanio', pieza=p, marco=Marco().girado((0, 0, 60 * k)), eje='x', bisel=0.0008, seg=1)
    franja(m, p, 0.0586, 0.0594, -0.4195, -0.4135, n=16)


def lz_bajos(m, p, gatillo):
    """Under it: the trigger's housing with the grip and the guard, the trigger (a piece of its
    own), the front grip on its mount, the power cell, the shoulder's pad on the keel."""
    # ---- the trigger's housing, a slot through its floor
    caja = m.extrusion([(0.044, -0.0430), (0.204, -0.0430), (0.199, -0.0600), (0.049, -0.0600)], 0.034, mat='aluminio_anodizado', pieza=p, eje='x', bisel=0.002, seg=2)
    ranura(m, caja, (0.016, 0.03, 0.030), (0, -0.0595, 0.143))
    for lado in (1, -1):
        m.tornillos([(lado * 0.017, -0.0515, z) for z in (0.058, 0.186)], 0.0024, 0.0008, 'acero', p, None, eje=(lado, 0, 0))
        # (the pin's heads)
        m.cilindro(0.003, 0.0022, en=(lado * 0.0178, LZ_PIVOTE.y, LZ_PIVOTE.z), mat='acero', pieza=p, eje='x', lados=10, bisel=0.0006)
    # the safety: an orange lever on its left, over the thumb
    m.cilindro(0.0055, 0.003, en=(0.0182, -0.0515, 0.096), mat='acero_oscuro', pieza=p, eje='x', lados=12, bisel=0.0008)
    m.caja((0.0035, 0.006, 0.02), en=(0.0205, -0.0515, 0.104), mat='pintura_naranja', pieza=p, bisel=0.0015, seg=2)
    # ---- the grip and the guard round the trigger
    empunadura(m, p, *LZ_EMPUNADURA)
    chapa(m, p, redondear([(0.192, -0.050), (0.192, -0.100), (0.110, -0.100), (0.099, -0.091)], 0.014), 0.005, -0.012, 0.012, 'aluminio_anodizado', plano='zy', hacia=(0.3, -0.3), bisel=0.0014)
    # ---- the trigger: a broad blade on its pin
    m.cilindro(0.0045, 0.013, en=LZ_PIVOTE, mat='acero', pieza=gatillo, eje='x', lados=12, bisel=0.0008)
    cara = redondear([(0.1436, -0.0520), (0.1440, -0.0660), (0.1460, -0.0780), (0.1500, -0.0880), (0.1550, -0.0935)], 0.01)
    chapa(m, gatillo, cara, 0.0055, -0.007, 0.007, 'pintura_naranja', plano='zy', hacia=(0.10, -0.075), bisel=0.0012)
    # ---- the front grip on its mount
    m.caja((0.036, 0.016, 0.064), en=(0, -0.050, 0.333), mat='aluminio_anodizado', pieza=p, bisel=0.003, seg=2)
    for lado in (1, -1):
        m.tornillos([(lado * 0.018, -0.051, z) for z in (0.312, 0.354)], 0.0026, 0.001, 'acero', p, None, eje=(lado, 0, 0))
    empunadura(m, p, *LZ_GUARDAMANO, fondo=0.048)
    # ---- the power cell under the breech: its catch, its three lamps
    m.caja((0.044, 0.028, 0.078), en=(0, -0.055, -0.056), mat='plastico_negro', pieza=p, bisel=0.006, seg=3)
    m.caja((0.024, 0.006, 0.01), en=(0, -0.067, -0.094), mat='pintura_naranja', pieza=p, bisel=0.002, seg=1)
    for k in range(3):
        m.piloto((0.0218, -0.056, -0.036 - k * 0.011), 0.0022, 'piloto_verde' if k < 2 else 'piloto_ambar', p, None, eje=(1, 0, 0))
    for lado in (1, -1):
        for k in range(4):
            m.caja((0.0012, 0.016, 0.004), en=(lado * 0.0222, -0.056, -0.084 + k * 0.008), mat='acero_oscuro', pieza=p, bisel=0.0004, seg=1)
    # ---- the shoulder's pad: ribbed rubber on two posts off the keel
    chapa(m, p, arco(0.090, 112, 68, 8, c=(0, -0.176)), 0.014, -0.275, -0.125, 'goma', hacia=(0, 0.5), bisel=0.004, seg=3)
    for k in range(7):
        z = -0.265 + k * 0.0215
        chapa(m, p, arco(0.0885, 110, 70, 8, c=(0, -0.176)), 0.003, z, z + 0.008, 'goma', hacia=(0, 0.5), bisel=0.001, seg=1)
    for z in (-0.245, -0.155):
        m.caja((0.016, 0.022, 0.024), en=(0, -0.064, z), mat='acero_oscuro', pieza=p, bisel=0.003, seg=2)
    # the sling's ring behind it
    m.tubo([(0, -0.064 + 0.009 * math.sin(math.tau * k / 14), -0.312 + 0.009 * math.cos(math.tau * k / 14)) for k in range(15)], 0.0018, 'acero', p, None, lados=6, tapas=False)


def lz_mira(m, p):
    """The sight, up on the left: a faceted housing on a bracket, its lens hooded ahead, its
    display behind turned to the eye in a rubber shroud with a strip of lamps under it, the
    seat of the ready lamp on top."""
    cx, cy, a, b, k = 0.0635, 0.072, 0.0185, 0.026, 0.007
    marco, n = marco_visor()
    forma = [(cx + a - k, cy + b), (cx + a, cy + b - k), (cx + a, cy - b + k), (cx + a - k, cy - b), (cx - a + k, cy - b), (cx - a, cy - b + k), (cx - a, cy + b - k), (cx - a + k, cy + b)]

    def atras(x):
        # (its back is cut square to the display's look, 6 mm behind its glass)
        h = math.hypot(n.x, n.z)
        return LZ_PANTALLA.z + (-0.006 * h - (x - LZ_PANTALLA.x) * n.x) / n.z

    casco(m, p, [[(x, y, atras(x)) for x, y in forma], (0.325, forma)], 'aluminio_anodizado', bisel=0.0015)
    # the bracket off the body's side, its bolts; the wire down to the body
    m.caja((0.016, 0.042, 0.078), en=(0.053, 0.029, 0.266), mat='acero_oscuro', pieza=p, bisel=0.003, seg=2)
    m.tornillos([(0.061, 0.016 + i * 0.016, 0.238 + j * 0.056) for i in (0, 1) for j in (0, 1)], 0.003, 0.0012, 'acero', p, None, eje='x')
    m.tubo([(0.070, 0.047, 0.314), (0.072, 0.034, 0.318), (0.058, 0.014, 0.326), (0.0485, 0.008, 0.326)], 0.002, 'goma', p, None, lados=6, codo=0.008)
    # the lens in its hood, a lit ring round it; the rangefinder's window under it
    eje = (cx, 0.076, 0)
    m.torno([(0.0125, 0.317), (0.0158, 0.317), (0.0158, 0.336), (0.0148, 0.3375), (0.0135, 0.3375), (0.0125, 0.335), (0.0125, 0.317)], en=eje, mat='plastico_negro', pieza=p, eje='z', lados=24)
    m.cilindro(0.0126, 0.004, en=(cx, 0.076, 0.3245), mat='plastico_negro', pieza=p, eje='z', lados=24, bisel=0.0005)
    m.esfera(0.0112, en=(cx, 0.076, 0.3262), mat='cristal_oscuro', pieza=p, lados=20, de=0, eje='z', aplastar=0.22)
    banda(m, p, 0.0113, 0.0125, 0.3262, 0.3272, 'luz_cian', en=eje, eje='z', lados=24, marco=Marco())
    m.caja((0.016, 0.008, 0.003), en=(cx, 0.052, 0.3255), mat='plastico_negro', pieza=p, bisel=0.001, seg=1)
    m.caja((0.011, 0.004, 0.001), en=(cx, 0.052, 0.3272), mat='luz_cian', pieza=p, bisel=0.0003, seg=1)
    # the display: its glass (the game writes on it), the shroud round it, the lamps under it
    w, h = LZ_PANTALLA_TAM
    m.caja((w + 0.008, h + 0.012, 0.008), en=(0, -0.002, -0.0045), mat='plastico_negro', pieza=p, marco=marco, bisel=0.002, seg=2)
    m.caja((w, h, 0.0012), en=(0, 0, -0.0007), mat='pantalla', pieza=p, marco=marco, bisel=0.0003, seg=1)
    for u, v, tam in ((0, 1, (w + 0.008, 0.003, 0.009)), (1, 0, (0.003, h + 0.003, 0.009)), (-1, 0, (0.003, h + 0.003, 0.009))):
        m.caja(tam, en=(u * (w / 2 + 0.0025), v * (h / 2 + 0.0015) + (0.0015 if u else 0.0), 0.004), mat='goma', pieza=p, marco=marco, bisel=0.001, seg=1)
    for i in range(5):
        m.caja((0.0044, 0.0022, 0.001), en=(-0.0116 + i * 0.0058, -h / 2 - 0.004, -0.0003), mat='luz_cian' if i < 4 else 'piloto_ambar', pieza=p, marco=marco, bisel=0.0003, seg=1)
    # on top: the ready lamp's seat (the game lights the lamp in it), a strip of lamps
    m.torno([(0.0072, -0.0008), (0.0096, -0.0008), (0.0096, 0.002), (0.0086, 0.003), (0.0072, 0.003), (0.0072, -0.0008)], en=(LZ_LAMPARA.x, cy + b, LZ_LAMPARA.z), mat='acero_oscuro', pieza=p, lados=16)
    m.cilindro(0.0072, 0.001, en=(LZ_LAMPARA.x, cy + b + 0.0003, LZ_LAMPARA.z), mat='plastico_negro', pieza=p, lados=16, bisel=0.0002)
    m.caja((0.008, 0.0014, 0.05), en=(cx, cy + b + 0.0003, 0.284), mat='plastico_negro', pieza=p, bisel=0.0004, seg=1)
    for i in range(6):
        m.caja((0.0052, 0.001, 0.0055), en=(cx, cy + b + 0.0011, 0.264 + i * 0.008), mat='luz_cian', pieza=p, bisel=0.0003, seg=1)
    # on its left: two keys and the cell's cap
    for z in (0.262, 0.278):
        m.cilindro(0.0042, 0.0026, en=(cx + a + 0.0004, 0.080, z), mat='goma', pieza=p, eje='x', lados=12, bisel=0.0008)
    m.cilindro(0.0075, 0.0024, en=(cx + a + 0.0003, 0.064, 0.303), mat='acero_oscuro', pieza=p, eje='x', lados=16, bisel=0.0008)
    m.caja((0.001, 0.0022, 0.011), en=(cx + a + 0.0014, 0.064, 0.303), mat='acero', pieza=p, bisel=0.0003, seg=1)


def lz_puerta(m, p, puerta):
    """The loading hatch: a plate that closes the chamber's top, the bore's roof under it, its
    hinge's knuckle and leaf at its rear edge, the grab lip and the latch at its front."""
    del p
    # the plate, its sides carrying on the body's slope
    afilar(m.extrusion([(0.0345, LZ_CUBIERTA), (0.0390, LZ_REPISA + 0.0008), (-0.0390, LZ_REPISA + 0.0008), (-0.0345, LZ_CUBIERTA)], 0.320, en=(0, 0, 0.196), mat='pintura_blanca', pieza=puerta, eje='z'), 0.001, 2)
    # under it: the roof of the bore, that holds the rocket down
    # (it thins toward the hinge: open, nothing of it is in the rocket's way down)
    y0 = LZ_REPISA + 0.0012
    techo = [(0.0226, y0)] + arco(LZ_ANIMA, 40, 140, 10) + [(-0.0226, y0)]
    casco(m, puerta, [[(x, y0 - 0.0012 if 0 < i < len(techo) - 1 else y0, 0.066) for i, (x, y) in enumerate(techo)], (0.125, techo), (0.343, techo)], 'acero_oscuro', bisel=0.0006, seg=1)
    m.caja((0.056, 0.0012, 0.300), en=(0, LZ_REPISA + 0.0004, 0.196), mat='titanio', pieza=puerta, bisel=0.0004, seg=1)
    m.tornillos([(x, LZ_REPISA - 0.0002, z) for x in (-0.0255, 0.0255) for z in (0.052, 0.148, 0.244, 0.340)], 0.0018, 0.0006, 'acero', puerta, None, eje=(0, -1, 0))
    for lado in (1, -1):
        for k in range(6):
            m.caja((0.0036, 0.0005, 0.011), en=(lado * 0.0255, LZ_REPISA - 0.0003, 0.074 + k * 0.011), mat='pintura_amarilla' if k % 2 else 'plastico_negro', pieza=puerta, bisel=0)
    for z in (0.165, 0.300):
        m.caja((0.012, 0.0016, 0.022), en=(0, LZ_ANIMA - 0.0003, z), mat='goma', pieza=puerta, bisel=0.0005, seg=1)
    # the hinge: its knuckle between the body's two, its leaf screwed on the plate
    m.cilindro(0.0045, 0.038, en=LZ_BISAGRA, mat='acero', pieza=puerta, eje='x', lados=14, bisel=0.001)
    m.caja((0.034, 0.0045, 0.0105), en=(0, 0.05325, 0.03375), mat='acero', pieza=puerta, bisel=0.001, seg=1)
    m.caja((0.034, 0.0058, 0.014), en=(0, 0.0526, 0.0432), mat='acero', pieza=puerta, bisel=0.0012, seg=1)
    m.tornillos([(x, 0.0555, 0.0445) for x in (-0.011, 0.011)], 0.0024, 0.0008, 'acero_oscuro', puerta, None, eje='y')
    # a stiffening rib down its middle, its fasteners; a lit slot each side of it
    m.caja((0.012, 0.003, 0.236), en=(0, 0.0512, 0.186), mat='aluminio_anodizado', pieza=puerta, bisel=0.001, seg=1)
    m.tornillos([(0, 0.0527, z) for z in (0.078, 0.132, 0.186, 0.240, 0.294)], 0.0022, 0.0007, 'acero', puerta, None, eje='y')
    for lado in (1, -1):
        m.caja((0.004, 0.0012, 0.05), en=(lado * 0.024, 0.0502, 0.105), mat='plastico_negro', pieza=puerta, bisel=0.0003, seg=1)
        m.caja((0.0022, 0.0006, 0.044), en=(lado * 0.024, 0.0508, 0.105), mat='luz_cian', pieza=puerta, bisel=0)
        m.tornillos([(lado * 0.029, LZ_CUBIERTA - 0.0002, z) for z in (0.046, 0.145, 0.245)], 0.002, 0.0007, 'acero', puerta, None, eje='y')
    # stencilled marks: lines of letters, an arrow each side toward the lip
    for i, largo in enumerate((0.034, 0.024, 0.03)):
        m.caja((0.0016, 0.0005, largo), en=(0.0285 - i * 0.0034, LZ_CUBIERTA + 0.0001, 0.176 + largo / 2), mat='pintura_oscura', pieza=puerta, bisel=0)
    m.caja((0.009, 0.0005, 0.009), en=(-0.0245, LZ_CUBIERTA + 0.0001, 0.182), mat='pintura_naranja', pieza=puerta, bisel=0)
    m.caja((0.0016, 0.0005, 0.03), en=(-0.0245, LZ_CUBIERTA + 0.0001, 0.21), mat='pintura_oscura', pieza=puerta, bisel=0)
    for lado in (1, -1):
        m.extrusion([(lado * 0.0175, 0.286), (lado * 0.0305, 0.286), (lado * 0.024, 0.302)], 0.0006, en=(0, LZ_CUBIERTA + 0.0001, 0), mat='pintura_naranja', pieza=puerta, eje='y')
    # hazard blocks across it behind the lip
    for k in range(8):
        m.caja((0.0078, 0.0006, 0.012), en=(-0.0273 + k * 0.0078, LZ_CUBIERTA + 0.0001, 0.316), mat='pintura_amarilla' if k % 2 else 'plastico_negro', pieza=puerta, bisel=0)
    # the grab lip: a ramp up to an edge the fingers hook, across its front
    m.extrusion([(0.3245, 0.0496), (0.3370, 0.0590), (0.3410, 0.0590), (0.3410, 0.0496)], 0.068, mat='pintura_amarilla', pieza=puerta, eje='x', bisel=0.0012, seg=2)
    # the latch: an orange paddle, its tongue over the keeper
    m.caja((0.022, 0.0065, 0.012), en=(0, 0.0528, 0.3492), mat='pintura_naranja', pieza=puerta, bisel=0.0018, seg=2)
    m.caja((0.013, 0.003, 0.0125), en=(0, 0.0552, 0.3585), mat='acero', pieza=puerta, bisel=0.0009, seg=1)


def lz_cohete(m, c):
    """The rocket, 55 mm: a brass fuze on a dark ogive, a white warhead with its orange band
    and its stencilled marks, a steel motor with two copper bands for the chamber's contacts,
    a nozzle in a ring that carries four fins folded round the tail."""
    R = LZ_CALIBRE / 2
    L = LZ_COHETE_LARGO / 2

    def vuelta(perfil, mat, lados=28):
        return m.torno(perfil, mat=mat, pieza=c, marco=c, eje='z', lados=lados, liso=50.0)

    # the tail: the nozzle's cone inside, the fins' ring, the boom they fold along
    vuelta([(0.0, -0.112), (0.0062, -0.112), (0.0075, -0.117), (0.0186, -L), (0.0226, -L), (0.0238, -0.1485), (0.0238, -0.1375), (0.0226, -0.136), (0.0176, -0.136), (0.0176, -0.094), (0.0, -0.094)], 'tobera')
    banda(m, c, 0.0236, 0.0242, -0.1465, -0.1405, 'titanio', eje='z', lados=28)
    for k in range(4):
        a = 45 + 90 * k
        chapa(m, c, arco(0.0212, a - 31, a + 31, 6), 0.0018, -0.1345, -0.0965, 'aluminio', bisel=0.0005, seg=1, marco=c)
        m.cilindro(0.0014, 0.04, en=(0.0198, 0, -0.1155), mat='acero', pieza=c, marco=c.girado((0, 0, a - 33)), eje='z', lados=6, bisel=0.0004)
        m.caja((0.005, 0.0065, 0.006), en=(0.0212, 0, -0.1365), mat='acero', pieza=c, marco=c.girado((0, 0, a)), bisel=0.001, seg=1)
        m.caja((0.0004, 0.006, 0.01), en=(0.02135, 0, -0.104), mat='pintura_roja', pieza=c, marco=c.girado((0, 0, a)), bisel=0)
    # the motor: a steel case, grooved for its two copper bands
    vuelta([(0.0, -0.095), (0.0176, -0.095), (0.0262, -0.0835), (R, -0.0815), (R, -0.0755), (0.0268, -0.075), (0.0268, -0.068), (R, -0.0675), (R, -0.0585), (0.0268, -0.058), (0.0268, -0.052), (R, -0.0515), (R, -0.0385), (0.0268, -0.038), (0.0268, -0.032), (R, -0.0315), (R, -0.0135), (0.0264, -0.0125), (0.0, -0.0125)], 'acero_oscuro')
    for z in (-0.055, -0.035):
        banda(m, c, 0.0266, R, z - 0.003, z + 0.003, 'cobre', eje='z', lados=28)
    banda(m, c, 0.0266, R, -0.0748, -0.0682, 'pintura_amarilla', eje='z', lados=28)
    # the joint, the warhead with its band
    vuelta([(0.0, -0.0128), (0.0262, -0.0128), (0.0262, -0.0072), (0.0, -0.0072)], 'acero')
    vuelta([(0.0, -0.0075), (0.0266, -0.0075), (R, -0.0066), (R, 0.0355), (0.0268, 0.036), (0.0268, 0.048), (R, 0.0485), (R, 0.054), (0.0, 0.054)], 'pintura_blanca')
    banda(m, c, 0.0266, R, 0.0362, 0.0478, 'pintura_naranja', eje='z', lados=28)
    # its marks: lines of letters, a block and dashes, three times round it
    for k in range(3):
        a = 30 + 120 * k
        for i, largo in enumerate((0.022, 0.016, 0.019)):
            m.caja((0.0005, 0.0017, largo), en=(R, 0, 0.004 + largo / 2), mat='pintura_oscura', pieza=c, marco=c.girado((0, 0, a + 11 - i * 7.5)), bisel=0)
        m.caja((0.0005, 0.0056, 0.0062), en=(R - 0.00012, 0, 0.0235), mat='pintura_amarilla', pieza=c, marco=c.girado((0, 0, a - 18)), bisel=0)
        m.caja((0.0006, 0.0036, 0.0012), en=(R - 0.00006, 0, 0.0235), mat='pintura_oscura', pieza=c, marco=c.girado((0, 0, a - 18)), bisel=0)
        m.torno([(R - 0.0002, -0.0035), (R + 0.0002, -0.0035), (R + 0.0002, -0.0015), (R - 0.0002, -0.0015), (R - 0.0002, -0.0035)], mat='pintura_oscura', pieza=c, marco=c.girado((0, 0, 75 + 120 * k)), eje='z', lados=4, arco=50.0)
    # the ogive and the fuze
    rho = (R * R + 0.095 ** 2) / (2 * R)

    def ojiva(z):
        return math.sqrt(rho * rho - z * z) + R - rho

    vuelta([(0.0, 0.0538), (R, 0.0538), (ojiva(0.0034), 0.0574), (0.0, 0.0574)], 'pintura_amarilla')
    vuelta([(0.0, 0.0572)] + [(ojiva(0.0032 + (0.081 - 0.0032) * i / 10), 0.0572 + (0.081 - 0.0032) * i / 10) for i in range(11)] + [(0.0, 0.135)], 'pintura_oscura')
    vuelta([(0.0, 0.1348), (0.0082, 0.1348), (0.0088, 0.1362), (0.0088, 0.1392), (0.0072, 0.1408), (0.0064, 0.1452), (0.0046, 0.1484), (0.0026, 0.1497), (0.0, L)], 'laton', lados=20)
    banda(m, c, 0.0086, 0.0091, 0.1369, 0.1385, 'acero', eje='z', lados=20)
