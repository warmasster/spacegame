"""Flight controls a seated crew member takes in a gloved hand: the side-stick and the throttle.

Each is built in the frame of what it is bolted to — x to port, y up, z forward, metres — its
origin the middle of its underside (its mounting face). Its body is the piece without a name;
what the game moves is a piece of its own, its origin its pivot, and what rides on a piece that
moves (the trigger and the hat on the stick) says so (`padre`). The recipe says the travels, the
grips and the places (`tools/modelos/datos/<name>.json`); `comprobar.py` holds the model to them.

Where they go by a seat (`recetas/cabina.py`, `asiento`: its frame's origin on the deck under its
column, its armrests' pads from z -0.17 to 0.19 at x +-0.31, their tops at y 0.65; a seated
crew member's shoulders at (+-0.21, 1.11, -0.07)): with their origin at (-+0.31, 0.65, 0.29) — on a
bracket off the armrest's front — the stick's grip is at (-0.31, 0.81, 0.31) and the throttle's at
(0.31, 0.82, 0.29): half a metre from the shoulder, the elbow bent some 95 degrees.
"""
import math

from mathutils import Vector

from kit import Marco, receta, rot_xyz
from recetas.equipo import MANGO, mango
from recetas.herramientas import casco, chapa, redondear

# ---------------------------------------------------------------------------------------------
# the side-stick

PV_BASE = (0.11, 0.05, 0.13)                # its base: wide, tall, long
PV_PIVOTE = Vector((0.0, 0.05, 0.0))        # the middle of the ball the stick turns on
PV_BOLA = 0.03                              # ... and that ball's radius
PV_INCLINA = 12.0                           # how far forward the stick leans at rest (degrees)
PV_RECORRE = 20.0                           # how far it goes each way, fore-aft and side to side
PV_AGARRE = 0.119                           # the middle of its grip, along it from the pivot
PV_GATILLO = (0.0, 0.174, 0.046)            # the trigger's pin, in the stick's own axes
PV_GATILLO_RECORRE = 14.0
PV_CUBIERTA = (0.0, 0.2134, 0.004)          # the middle of the head's deck, in the stick's own axes
PV_CUBIERTA_CAE = 16.7                      # ... and how far it slopes down toward the seat
PV_SETA = (0.010, -0.0026, -0.0125)         # the hat's pivot, in the deck's axes (under its skin)
PV_SETA_RECORRE = 14.0
PV_BOTON = (-0.011, 0.0, 0.0105)            # the red button, in the deck's axes


def marco_palanca():
    """The stick's own axes: its origin the pivot, its y up the stick, its z ahead of it."""
    return Marco(PV_PIVOTE).girado((PV_INCLINA, 0, 0))


def marco_cubierta():
    """The axes of the head's deck: its origin the deck's middle, its y out of it, its z up its
    slope (ahead)."""
    ms = marco_palanca()
    return Marco(ms.a_mundo(PV_CUBIERTA), ms.R @ rot_xyz((-PV_CUBIERTA_CAE, 0, 0)))


def octogono(hw, hd, cz, y, corte=0.3):
    """Eight points round (0, y, cz) in a level plane: a rectangle `hw` by `hd` (halves) with
    its corners cut; `y` a height, or a function of z."""
    cx, cd = hw * corte, hd * corte
    pts = [(hw - cx, hd), (hw, hd - cd), (hw, -hd + cd), (hw - cx, -hd), (-hw + cx, -hd), (-hw, -hd + cd), (-hw, hd - cd), (-hw + cx, hd)]
    return [(x, y(cz + z) if callable(y) else y, cz + z) for x, z in pts]


def flecha(m, p, en, hacia, normal, arriba, tam=0.004, mat='plastico_claro', marco=None):
    """An arrow head painted on a face at `en` (a, b offsets round it), pointing `hacia` (a
    direction in the face: (to the right, up))."""
    a, b = hacia
    m.lamina([[(en[0] + a * tam - b * tam * 0.7, en[1] + b * tam + a * tam * 0.7), (en[0] + a * tam + b * tam * 0.7, en[1] + b * tam - a * tam * 0.7), (en[0] + a * tam * 2.3, en[1] + b * tam * 2.3)]], (0, 0, 0), mat, p, marco, normal=normal, arriba=arriba, sobre=0.0002)


@receta('palanca_vuelo')
def palanca_vuelo(m):
    """A side-stick for a gloved hand: a base bolted beside the seat, and on a ball in it the
    stick — a neck, a shelf the hand rests on, a ribbed rubber grip 44 mm across leaning 12
    degrees forward, and a head with its trigger under the index finger, a four-way hat and a
    red button under the thumb. Its pieces: the base, the stick (`palanca`: it turns 20 degrees
    each way about x and about z on its ball), the trigger (`gatillo`) and the hat (`seta`),
    which ride on the stick."""
    sx, sy, sz = PV_BASE
    ms, md = marco_palanca(), marco_cubierta()
    p = m.pieza('', (sx, sy, sz), (0, 0, 0))
    palanca = m.pieza('palanca', (0.09, 0.26, 0.11), PV_PIVOTE)
    gatillo = m.pieza('gatillo', (0.014, 0.045, 0.02), ms.a_mundo(PV_GATILLO))
    seta = m.pieza('seta', (0.02, 0.016, 0.02), md.a_mundo(PV_SETA))
    m.presupuesto = 10000
    m.marco_dicho = 'x a babor, y arriba, z a proa; metros; origen: el centro de la cara de abajo de su base (la que se atornilla)'
    # ---- the base: a cast box with the ball's socket bored in its top, on a flange with its bolts
    base = m.caja((sx - 0.012, sy - 0.006, sz - 0.012), en=(0, sy / 2 + 0.003, 0), mat='pintura_gris', pieza=p, bisel=0.004, seg=3)
    m.taladrar(base, [(0, sy, 0)], PV_BOLA + 0.0012, 0.064, eje='y', lados=32)
    m.caja((sx, 0.007, sz), en=(0, 0.0035, 0), mat='acero_oscuro', pieza=p, bisel=0.002, seg=1)
    m.tornillos([(x * (sx / 2 - 0.007), 0.007, z * (sz / 2 - 0.008)) for x in (-1, 1) for z in (-1, 1)], 0.0042, 0.0026, 'acero', p, None)
    # the bracket it stands on: a plate under its base that runs back on to the front of the
    # seat's armrest, where it is bolted (the base is ahead of the armrest: `asiento` below)
    m.caja((0.074, 0.012, sz + 0.2), en=(0, -0.006, -0.1), mat='acero_oscuro', pieza=p, bisel=0.003, seg=1)
    m.caja((0.05, 0.02, 0.012), en=(0, -0.016, -sz / 2 - 0.012), mat='acero_oscuro', pieza=p, bisel=0.003, seg=1)
    m.tornillos([(x, 0.0, z) for x in (-0.022, 0.022) for z in (-sz / 2 - 0.085, -sz / 2 - 0.165)], 0.0048, 0.003, 'acero', p, None)
    # the socket's ring, its screws
    r0 = PV_BOLA + 0.0014
    m.torno([(r0, sy - 0.001), (r0, sy + 0.005), (r0 + 0.0026, sy + 0.007), (r0 + 0.0086, sy + 0.007), (r0 + 0.0116, sy + 0.004), (r0 + 0.0116, sy - 0.001)], mat='acero_oscuro', pieza=p, lados=32)
    m.tornillos([((r0 + 0.0056) * math.cos(math.tau * (k + 0.5) / 6), sy + 0.007, (r0 + 0.0056) * math.sin(math.tau * (k + 0.5) / 6)) for k in range(6)], 0.0022, 0.0009, 'acero', p, None)
    # what is written on its top, for whoever sits behind it: which way is which
    arriba = (0, sy, 0)
    m.texto('CABECEO', (0.004, sy, 0.0485), 0.0046, 'plastico_claro', p, normal=(0, 1, 0), arriba=(0, 0, 1), sobre=0.0002)
    m.texto('ALABEO', (0.0, sy, -0.0485), 0.0046, 'plastico_claro', p, normal=(0, 1, 0), arriba=(0, 0, 1), sobre=0.0002)
    marco_arriba = Marco(arriba)
    for s in (1, -1):
        flecha(m, p, (s * 0.017, -0.0485), (s, 0), (0, 1, 0), (0, 0, 1), tam=0.003, marco=marco_arriba)
    flecha(m, p, (0.0245, 0.0445), (0, 1), (0, 1, 0), (0, 0, 1), tam=0.0022, marco=marco_arriba)
    flecha(m, p, (0.0325, 0.0525), (0, -1), (0, 1, 0), (0, 0, 1), tam=0.0022, marco=marco_arriba)
    # toward the seat: its data plate; the lever that locks the stick, ahead; the cable, behind
    m.rotulo('PALANCA DE VUELO PV-2', (0.008, 0.031, -sz / 2 + 0.006), 0.0044, p, normal=(0, 0, -1), fondo='aluminio', tinta='plastico_negro', ancho=0.064)
    m.texto('N/S 2210 · 28 V CC', (0.008, 0.0195, -sz / 2 + 0.006), 0.003, 'plastico_claro', p, normal=(0, 0, -1), sobre=0.0002)
    m.cilindro(0.008, 0.006, en=(0.028, 0.028, sz / 2 - 0.004), mat='acero_oscuro', pieza=p, eje='z', lados=14, bisel=0.0012)
    m.caja((0.007, 0.024, 0.006), en=(0.028, 0.036, sz / 2 + 0.001), mat='pintura_roja', pieza=p, bisel=0.002, seg=2)
    m.texto('BLOQUEO', (-0.012, 0.028, sz / 2 - 0.006), 0.0042, 'plastico_claro', p, normal=(0, 0, 1), sobre=0.0002)
    m.cilindro(0.011, 0.012, en=(-0.034, 0.027, -sz / 2 + 0.002), mat='acero', pieza=p, eje='z', lados=8, bisel=0.002)
    m.manguera((-0.034, 0.027, -sz / 2 - 0.002), (-0.034, -0.002, -sz / 2 - 0.05), 0.0062, comba=0.012, hacia=(0, 0, -1), mat='goma', pieza=p)
    # a panel on each side, with its screws
    for lado in (1, -1):
        m.caja((0.003, 0.026, 0.084), en=(lado * (sx / 2 - 0.006), 0.028, 0), mat='pintura_oscura', pieza=p, bisel=0.001, seg=1)
        m.tornillos([(lado * (sx / 2 - 0.0045), 0.028, z) for z in (-0.036, 0.036)], 0.0024, 0.0009, 'acero', p, None, eje=(lado, 0, 0))
    # ---- the stick: the ball, the neck, the shelf under the hand
    q = palanca
    m.torno([(0.0, -PV_BOLA * 0.72)] + [(PV_BOLA * math.cos(math.radians(a)), PV_BOLA * math.sin(math.radians(a))) for a in range(-46, 83, 8)] + [(0.0, PV_BOLA)], mat='plastico_negro', pieza=q, marco=ms, lados=28, liso=80.0)
    m.torno([(0.0, 0.026), (0.0125, 0.026), (0.0125, 0.03), (0.0105, 0.033), (0.0105, 0.05), (0.014, 0.054), (0.0, 0.054)], mat='acero', pieza=q, marco=ms, lados=18)
    casco(m, q, [octogono(0.030, 0.036, -0.004, 0.051, 0.36), octogono(0.034, 0.041, -0.005, 0.055, 0.36), octogono(0.034, 0.041, -0.005, 0.059, 0.36), octogono(0.026, 0.03, -0.002, 0.063, 0.36)], 'aluminio_anodizado', bisel=0.0012, marco=ms)
    # the grip: rubber, ribbed, from the head down to the shelf
    mango(m, q, ms.a_mundo((0, 0.176, 0)), ms.a_mundo((0, 0.062, 0)), (1, 0, 0), ancho=MANGO, fondo=0.05)

    # the head: faceted, its top a deck sloped to the thumb
    def cubierta(z):
        return PV_CUBIERTA[1] + math.tan(math.radians(PV_CUBIERTA_CAE)) * (z - PV_CUBIERTA[2])

    casco(m, q, [octogono(0.0235, 0.0265, 0.0, 0.170), octogono(0.028, 0.033, 0.003, 0.183), octogono(0.028, 0.034, 0.004, 0.204), octogono(0.0245, 0.03, 0.004, cubierta)], 'aluminio_anodizado', bisel=0.0014, marco=ms)
    # on the deck: the red button in its ring, the hat's boot, what the hat is for
    bx, _, bz = PV_BOTON
    m.torno([(0.0072, -0.001), (0.0092, -0.001), (0.0092, 0.0022), (0.0072, 0.0022), (0.0072, -0.001)], en=(bx, 0, bz), mat='acero_oscuro', pieza=q, marco=md, lados=16)
    m.cilindro(0.0062, 0.0056, en=(bx, 0.0012, bz), mat='pintura_roja', pieza=q, marco=md, lados=16, bisel=0.0014)
    hx, hy, hz = PV_SETA
    m.torno([(0.0058, -0.001), (0.0092, -0.001), (0.0092, 0.0016), (0.0076, 0.003), (0.0058, 0.003), (0.0058, -0.001)], en=(hx, 0, hz), mat='goma', pieza=q, marco=md, lados=16)
    m.texto('TRIM', (hx, 0, hz - 0.0138), 0.0032, 'plastico_claro', q, marco=md, normal=(0, 1, 0), arriba=(0, 0, 1), sobre=0.0003)
    # the trigger's cheeks under the head's chin, the heads of its pin
    gx, gy, gz = PV_GATILLO
    for lado in (1, -1):
        m.extrusion([(0.020, 0.172), (0.030, 0.193), (0.054, 0.185), (0.0535, 0.169), (0.043, 0.165)], 0.0042, en=(lado * 0.0092, 0, 0), mat='acero_oscuro', pieza=q, marco=ms, eje='x', bisel=0.001, seg=1)
        m.cilindro(0.003, 0.0018, en=(lado * 0.0118, gy, gz), mat='acero', pieza=q, marco=ms, eje='x', lados=10, bisel=0.0005)
    # ---- the trigger: a blade on its pin, between the cheeks
    m.cilindro(0.0034, 0.0118, en=(0, gy, gz), mat='acero', pieza=gatillo, marco=ms, eje='x', lados=12, bisel=0.0006)
    cara = redondear([(gz + 0.002, gy + 0.002), (gz + 0.0026, gy - 0.012), (gz + 0.0046, gy - 0.022), (gz + 0.0082, gy - 0.030), (gz + 0.012, gy - 0.0345)], 0.008)
    chapa(m, gatillo, cara, 0.005, -0.0058, 0.0058, 'pintura_roja', plano='zy', hacia=(gz - 0.06, gy - 0.02), bisel=0.001, marco=ms)
    # ---- the hat: a stem out of its boot, a cap with four ridges
    m.cilindro(0.0036, 0.0085, en=(hx, hy + 0.0096, hz), mat='acero', pieza=seta, marco=md, lados=10, bisel=0.0006)
    m.torno([(0.0, 0.013), (0.0088, 0.013), (0.0096, 0.0142), (0.0096, 0.0166), (0.008, 0.0182), (0.0, 0.0176)], en=(hx, hy, hz), mat='plastico_gris', pieza=seta, marco=md, lados=16)
    for k in range(4):
        m.caja((0.0034, 0.0016, 0.0034), en=(hx + 0.0066 * math.cos(math.tau * k / 4), hy + 0.0186, hz + 0.0066 * math.sin(math.tau * k / 4)), mat='plastico_gris', pieza=seta, marco=md, bisel=0.0006, seg=1)
    # ---- what the game needs of it
    r = PV_RECORRE
    m.mueve('palanca', eje=[((1, 0, 0), -r, r), ((0, 0, 1), -r, r)], nota='gira sobre su rótula: +x cabeza a proa (picar), +z cabeza a estribor; en reposo 0')
    m.mueve('gatillo', eje=(1, 0, 0), giro=PV_GATILLO_RECORRE, padre='palanca', nota='apretado gira sobre +x: su hoja atrás, hacia la empuñadura')
    m.mueve('seta', eje=[((1, 0, 0), -PV_SETA_RECORRE, PV_SETA_RECORRE), (tuple(md.R @ Vector((0, 0, 1))), -PV_SETA_RECORRE, PV_SETA_RECORRE)], padre='palanca', nota='cuatro direcciones bajo el pulgar: se inclina sobre x (adelante/atrás) y sobre el z de su cubierta (a los lados)')
    s = ms.R @ Vector((0, 1, 0))
    m.punto('agarre', ms.a_mundo((0, PV_AGARRE, 0)), palma=(1, 0, 0.12), traves=-s - Vector((1, 0, 0.12)).normalized() * (-s).dot(Vector((1, 0, 0.12)).normalized()), pieza='palanca', mano='der', mango=MANGO, nota='mano derecha: palma a babor, dedos por delante, índice arriba (en el gatillo: pose "gatillo")')
    m.punto('pulgar_seta', md.a_mundo((hx, hy + 0.0182, hz)), pieza='seta', nota='la yema del pulgar sobre la seta')
    m.punto('pulgar_boton', md.a_mundo((bx, 0.004, bz)), pieza='palanca', nota='la yema del pulgar sobre el botón rojo')
    m.punto('gatillo', ms.a_mundo((0, gy - 0.02, gz + 0.004)), pieza='gatillo', nota='el centro de la cara del gatillo (donde aprieta el índice)')
    m.datos['asiento'] = {'en': [-0.31, 0.65, 0.29], 'nota': 'sugerido, en el marco del asiento: sobre un soporte ante el reposabrazos derecho'}
    m.datos['vistas'] = {'cabeza': dict(desde=(0.5, 0.6, -0.65), mira=tuple(ms.a_mundo((0, 0.19, 0.01))), dist=0.26, lente=50), 'gatillo': dict(desde=(1, 0.1, 0.5), mira=tuple(ms.a_mundo((0, 0.16, 0.03))), dist=0.24, lente=50, pose={'gatillo': 1.0}), 'picada': dict(desde=(1, 0.25, 0.1), pose={'palanca': [1.0, None]}, orto=0.36), 'alabeo': dict(desde=(0.15, 0.3, -1), pose={'palanca': [None, 1.0], 'seta': [1.0, None]}, orto=0.36), 'base': dict(desde=(0.2, 1, -0.5), mira=(0, 0.05, 0), dist=0.3, lente=50)}


# ---------------------------------------------------------------------------------------------
# the throttle

MG_BASE = (0.09, 0.2)                      # its quadrant: wide, long
MG_PIVOTE = Vector((0.0, 0.014, 0.0))      # the lever's shaft
MG_RADIO = 0.082                           # the radius of the quadrant's top, about that shaft
MG_RECORRE = 28.0                          # how far the lever goes each way from upright (degrees)
MG_BRAZO = 0.156                           # from the shaft to the middle of the bar the hand takes
MG_BARRA = 0.128                           # that bar's length
MG_RANURA = 38.0                           # how far each way from upright the slot runs


def en_arco(a, r=MG_RADIO, x=0.0):
    """The point at `a` degrees round the throttle's shaft (0 ahead, 90 up), `r` from it."""
    return Vector((x, MG_PIVOTE.y + r * math.sin(math.radians(a)), r * math.cos(math.radians(a))))


def sobre_arco(a):
    """How a mark lies on the quadrant's top at `a` degrees: (the way the top looks there, the
    way ahead along it)."""
    return (en_arco(a) - MG_PIVOTE).normalized(), Vector((0, -math.cos(math.radians(a)), math.sin(math.radians(a))))


def marco_arco(a):
    """A frame at the shaft turned so that what is turned round x in it starts at `a` degrees
    (0 ahead, 90 up) and goes on aft."""
    return Marco(MG_PIVOTE).girado((-(a + 90.0), 0, 0))


@receta('mando_gases')
def mando_gases(m):
    """A throttle for a gloved hand: a quadrant bolted beside the seat — a housing with a round
    top, a slot along it, a scale down one side of the slot and the idle stop behind — and in it
    the lever (`palanca`), with a T-handle a fist closes on from above (the lunar rover's, which
    a pressurised glove could work when it could not work a stick). It turns 28 degrees each way
    from upright about x; forward is full."""
    w, largo = MG_BASE
    p = m.pieza('', (w, 0.1, largo), (0, 0, 0))
    palanca = m.pieza('palanca', (MG_BARRA, 0.2, 0.06), MG_PIVOTE)
    m.presupuesto = 8000
    m.marco_dicho = 'x a babor, y arriba, z a proa; metros; origen: el centro de la cara de abajo de su base (la que se atornilla)'
    R = MG_RADIO
    a0 = 24.0
    # ---- the quadrant: a housing with a round top, the lever's cavity cut through it
    perfil = [(-largo / 2 + 0.006, 0.006), (largo / 2 - 0.006, 0.006), (largo / 2 - 0.006, 0.034)] + [(en_arco(a).z, en_arco(a).y) for a in [a0 + (180 - 2 * a0) * k / 14 for k in range(15)]] + [(-largo / 2 + 0.006, 0.034)]
    cuerpo = m.extrusion(perfil, w - 0.012, mat='pintura_gris', pieza=p, eje='x', bisel=0.004, seg=2)
    ancho = 0.016
    cavidad = [(0.0, MG_PIVOTE.y)] + [(en_arco(a, 0.2).z, en_arco(a, 0.2).y) for a in (90 - MG_RANURA, 90 - 12, 90 + 12, 90 + MG_RANURA)]
    m.restar(cuerpo, m.extrusion(cavidad, ancho, mat='pintura_gris', pieza=p, eje='x'))
    m.taladrar(cuerpo, [tuple(MG_PIVOTE)], 0.04, ancho, eje='x', lados=24)
    m.caja((w, 0.007, largo), en=(0, 0.0035, 0), mat='acero_oscuro', pieza=p, bisel=0.002, seg=1)
    m.tornillos([(x * (w / 2 - 0.007), 0.007, z * (largo / 2 - 0.008)) for x in (-1, 1) for z in (-1, 1)], 0.0042, 0.0026, 'acero', p, None)
    # the bracket it stands on: a plate under its base that runs back on to the front of the
    # seat's armrest, where it is bolted
    m.caja((0.074, 0.012, largo + 0.17), en=(0, -0.006, -0.085), mat='acero_oscuro', pieza=p, bisel=0.003, seg=1)
    m.caja((0.05, 0.02, 0.012), en=(0, -0.016, -largo / 2 - 0.012), mat='acero_oscuro', pieza=p, bisel=0.003, seg=1)
    m.tornillos([(x, 0.0, z) for x in (-0.022, 0.022) for z in (-largo / 2 - 0.07, -largo / 2 - 0.14)], 0.0048, 0.003, 'acero', p, None)
    # the slot's rim: a strip each side of it, round the top
    for lado in (1, -1):
        m.torno([(R - 0.001, -0.003), (R + 0.0022, -0.0024), (R + 0.0022, 0.0024), (R - 0.001, 0.003), (R - 0.001, -0.003)], en=(lado * (ancho / 2 + 0.0036), 0, 0), mat='acero_oscuro', pieza=p, marco=marco_arco(90 - MG_RANURA - 3), eje='x', lados=40, arco=2 * (MG_RANURA + 3))
    # the scale to port of the slot: a tick every quarter, what each is, its name
    for k, marca in enumerate(('0', '25', '50', '75', '100')):
        a = 90 + MG_RECORRE - 2 * MG_RECORRE * k / 4
        n, t = sobre_arco(a)
        m.lamina([[(-0.004, -0.0007), (0.004, -0.0007), (0.004, 0.0007), (-0.004, 0.0007)]], en_arco(a, R, 0.019), 'plastico_claro', p, normal=n, arriba=t, sobre=0.0003)
        m.texto(marca, en_arco(a, R, 0.0315), 0.0052, 'plastico_claro', p, normal=n, arriba=t, sobre=0.0003, ancho=0.012)
    for k in range(16):
        if k % 4:
            a = 90 + MG_RECORRE - 2 * MG_RECORRE * k / 16
            n, t = sobre_arco(a)
            m.lamina([[(-0.002, -0.0005), (0.002, -0.0005), (0.002, 0.0005), (-0.002, 0.0005)]], en_arco(a, R, 0.017), 'plastico_claro', p, normal=n, arriba=t, sobre=0.0003)
    # to starboard of it: the blocks that say where full is, and its name
    for k in range(3):
        a = 90 - MG_RECORRE + 1.5 + k * 5.0
        n, t = sobre_arco(a)
        m.lamina([[(-0.006, -0.0026), (0.006, -0.0026), (0.006, 0.0026), (-0.006, 0.0026)]], en_arco(a, R, -0.0235), 'pintura_roja' if k == 0 else 'pintura_amarilla', p, normal=n, arriba=t, sobre=0.0003)
    n, t = sobre_arco(98.0)
    m.texto('GASES', en_arco(98.0, R, -0.0245), 0.0052, 'plastico_claro', p, normal=n, arriba=t, sobre=0.0003, ancho=0.024)
    # the idle stop behind the slot: a red latch on its block
    at = 90 + MG_RANURA + 9
    m.caja((0.03, 0.01, 0.012), en=en_arco(at, R + 0.003), mat='acero_oscuro', pieza=p, bisel=0.002, seg=1, rot=(90 - at, 0, 0))
    m.caja((0.012, 0.007, 0.018), en=en_arco(at, R + 0.0105), mat='pintura_roja', pieza=p, bisel=0.002, seg=2, rot=(90 - at, 0, 0))
    # the friction knob on the shaft, to port; the shaft's cap, to starboard
    m.torno([(0.0, 0.0), (0.012, 0.0), (0.012, 0.004), (0.021, 0.006), (0.021, 0.018), (0.018, 0.021), (0.0, 0.021)], en=(w / 2 - 0.006, MG_PIVOTE.y, 0), mat='aluminio_anodizado', pieza=p, eje='x', lados=14)
    m.lamina([[(-0.0012, 0.004), (0.0012, 0.004), (0.0012, 0.017), (-0.0012, 0.017)]], (w / 2 + 0.015, MG_PIVOTE.y, 0), 'plastico_claro', p, normal=(1, 0, 0), arriba=(0, 1, 0), sobre=0.0003)
    m.cilindro(0.012, 0.005, en=(-w / 2 + 0.0045, MG_PIVOTE.y, 0), mat='acero', pieza=p, eje='x', lados=6, bisel=0.001)
    # toward the seat: its data plate; ahead, where its cable leaves
    m.rotulo('MANDO DE GASES MG-1', (0, 0.0245, -largo / 2 + 0.006), 0.0042, p, normal=(0, 0, -1), fondo='aluminio', tinta='plastico_negro', ancho=0.062)
    m.texto('N/S 1873 · 28 V CC', (0, 0.0145, -largo / 2 + 0.006), 0.0028, 'plastico_claro', p, normal=(0, 0, -1), sobre=0.0002)
    m.cilindro(0.011, 0.012, en=(0.02, 0.02, largo / 2 - 0.004), mat='acero', pieza=p, eje='z', lados=8, bisel=0.002)
    m.manguera((0.02, 0.02, largo / 2), (0.02, -0.002, largo / 2 + 0.05), 0.0062, comba=0.012, hacia=(0, 0, 1), mat='goma', pieza=p)
    for lado in (1, -1):
        m.caja((0.003, 0.02, 0.12), en=(lado * (w / 2 - 0.006), 0.019, 0.04 * lado), mat='pintura_oscura', pieza=p, bisel=0.001, seg=1)
        m.tornillos([(lado * (w / 2 - 0.0045), 0.019, 0.04 * lado + z) for z in (-0.052, 0.052)], 0.0024, 0.0009, 'acero', p, None, eje=(lado, 0, 0))
    # ---- the lever: a hub on the shaft, a flat arm up through the slot, the T-handle
    q = palanca
    py = MG_PIVOTE.y
    m.cilindro(0.016, 0.0105, en=MG_PIVOTE, mat='acero_oscuro', pieza=q, eje='x', lados=24, bisel=0.0012)
    m.extrusion([(-0.006, py), (0.006, py), (0.006, py + MG_BRAZO - 0.018), (-0.006, py + MG_BRAZO - 0.018)], 0.0085, mat='acero', pieza=q, eje='x', bisel=0.0012, seg=1)
    # (a dust shield that slides with it over the slot)
    m.torno([(R + 0.0034, -ancho / 2 - 0.006), (R + 0.005, -ancho / 2 - 0.006), (R + 0.005, ancho / 2 + 0.006), (R + 0.0034, ancho / 2 + 0.006), (R + 0.0034, -ancho / 2 - 0.006)], mat='plastico_negro', pieza=q, marco=marco_arco(78.0), eje='x', lados=12, arco=24.0)
    centro = Vector((0, py + MG_BRAZO, 0))
    m.cilindro(0.0215, 0.03, en=centro, mat='aluminio_anodizado', pieza=q, eje='x', lados=20, bisel=0.002)
    for lado in (1, -1):
        mango(m, q, centro + Vector((lado * 0.014, 0, 0)), centro + Vector((lado * MG_BARRA / 2, 0, 0)), (0, 0, 1), ancho=MANGO, fondo=MANGO, entra=0.0, costillas=4)
    for s in (1, -1):
        m.tornillos([(0, py + MG_BRAZO - 0.006, s * 0.0206)], 0.003, 0.001, 'acero', q, None, eje=(0, 0, s))
    # ---- what the game needs of it
    m.mueve('palanca', eje=(1, 0, 0), desde=-MG_RECORRE, giro=MG_RECORRE, nota='gira sobre +x: -28 atrás (ralentí), +28 a proa (a fondo); construida derecha (0)')
    m.punto('agarre', centro, palma=(0, -0.7071, 0.7071), traves=(1, 0, 0), pieza='palanca', mano='izq', mango=MANGO, nota='mano izquierda sobre la barra, desde atrás y arriba: el brazo de la palanca entre el corazón y el anular')
    m.datos['asiento'] = {'en': [0.31, 0.65, 0.29], 'nota': 'sugerido, en el marco del asiento: sobre un soporte ante el reposabrazos izquierdo'}
    m.datos['vistas'] = {'escala': dict(desde=(0.5, 1, -0.5), mira=(0, 0.09, 0), dist=0.3, lente=50), 'a_fondo': dict(desde=(1, 0.2, 0), pose={'palanca': 1.0}, orto=0.3), 'ralenti': dict(desde=(1, 0.2, 0), pose={'palanca': 0.0}, orto=0.3)}
