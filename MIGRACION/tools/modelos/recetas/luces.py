"""Lamps. The light itself and its lit lens are the game's (a lamp's `luz`): a model is the
fitting round them, and leaves the place of that lens clear."""
import math

from kit import TINTE, receta


@receta('plafon')
def plafon(m):
    """A ceiling fitting: a tray with an end cap each side and its screws, the diffuser under it."""
    c, d = m['carcasa'], m['difusor']
    sx, sy, sz = c.tam
    m.caja((sx, sy, sz), mat=TINTE, pieza=c, marco=c, bisel=0.012, seg=3)
    for x in (-1, 1):
        m.caja((0.03, sy + 0.004, sz + 0.004), en=(x * (sx / 2 - 0.02), 0, 0), mat='pintura_gris', pieza=c, marco=c, bisel=0.008, seg=2)
    m.tornillos([(x * (sx / 2 - 0.02), -sy / 2 - 0.002, z * (sz / 2 - 0.035)) for x in (-1, 1) for z in (-1, 1)], 0.005, 0.003, 'acero', c, c, eje=(0, -1, 0))
    m.caja(tuple(d.tam), mat=TINTE, pieza=d, marco=d, bisel=0.005, seg=2)
    for z in (-1, 1):
        m.caja((d.tam.x, 0.006, 0.006), en=(0, -0.002, z * (d.tam.z / 2 + 0.002)), mat='acero', pieza=d, marco=d, bisel=0.001, seg=1)


@receta('flexo')
def flexo(m):
    """A console lamp: a ribbed gooseneck on its nut, a small hood open downward."""
    tallo, hood = m[''], m['pantalla']
    perfil = [(0.012, -tallo.h / 2), (0.012, -tallo.h / 2 + 0.012), (tallo.r, -tallo.h / 2 + 0.016)]
    n = 22
    for k in range(n):
        y = -tallo.h / 2 + 0.016 + (tallo.h - 0.02) * k / n
        dy = (tallo.h - 0.02) / n
        perfil += [(tallo.r + 0.0016, y + dy * 0.3), (tallo.r, y + dy * 0.6), (tallo.r, y + dy)]
    m.torno(perfil, mat=TINTE, pieza=tallo, marco=tallo, lados=10, liso=60)
    r0, r1, h = hood.r, hood.r * hood.taper, hood.h
    m.torno([(0, h / 2), (r1, h / 2), (r0, -h / 2), (r0 - 0.003, -h / 2), (r1 - 0.003, h / 2 - 0.004), (0, h / 2 - 0.004)], mat=TINTE, pieza=hood, marco=hood, lados=20)


def faro(m, hacia_arriba, cupula=TINTE):
    """A beacon: a base with its gasket and a dome over the lamp."""
    p = m['']
    marco = p if hacia_arriba else p.girado((180, 0, 0))
    r, h = p.r, p.h
    m.torno([(0, -h / 2), (r, -h / 2), (r, -h / 2 + h * 0.26), (r * 0.9, -h / 2 + h * 0.34), (0, -h / 2 + h * 0.34)], mat='acero_oscuro', pieza=p, marco=marco, lados=24)
    m.torno([(r * 0.84, -h / 2 + h * 0.3), (r * 0.9, -h / 2 + h * 0.4), (r * 0.84, -h / 2 + h * 0.44)], mat='goma', pieza=p, marco=marco, lados=24)
    alto = h * 0.62
    m.esfera(r * 0.8, en=(0, -h / 2 + h * 0.38, 0), mat=cupula, pieza=p, marco=marco, lados=24, de=0, aplastar=alto / (r * 0.8))
    m.tornillos([(r * 0.78 * math.cos(math.tau * k / 3), -h / 2 + h * 0.26, r * 0.78 * math.sin(math.tau * k / 3)) for k in range(3)], 0.005, 0.003, 'acero', p, marco)


@receta('baliza')
def baliza(m):
    faro(m, True)


@receta('luz_nav')
def luz_nav(m):
    faro(m, True)


@receta('estroboscopio')
def estroboscopio(m):
    faro(m, True, 'lente')


@receta('baliza_obra')
def baliza_obra(m):
    faro(m, False)


@receta('campana')
def campana(m):
    """A high-bay lamp: a spun bell open downward, its driver in a finned can on top, an eye to
    hang it by."""
    p = m['']
    r0, r1, h = p.r, p.r * p.taper, p.h
    # the bell, outside and inside
    m.torno([(r1 * 0.7, h / 2 - 0.06), (r1, h / 2 - 0.075), (r0 - 0.012, -h / 2 + 0.012), (r0, -h / 2), (r0 - 0.006, -h / 2), (r0 - 0.02, -h / 2 + 0.014), (r1 - 0.01, h / 2 - 0.085), (0, h / 2 - 0.085)], mat=TINTE, pieza=p, marco=p, lados=32)
    m.cilindro(r1 * 0.62, 0.06, en=(0, h / 2 - 0.03, 0), mat='acero_oscuro', pieza=p, marco=p, lados=20, bisel=0.006)
    for k in range(12):
        marco = p.girado((0, 30 * k, 0))
        m.caja((0.03, 0.05, 0.004), en=(r1 * 0.62 + 0.012, h / 2 - 0.032, 0), mat='acero_oscuro', pieza=p, marco=marco, bisel=0.001, seg=1)
    m.torno([(0.012, -0.004), (0.018, 0), (0.012, 0.004), (0.008, 0)], en=(0, h / 2 + 0.008, 0), mat='acero', pieza=p, marco=p, eje='x', lados=12)


@receta('luz_emergencia')
def luz_emergencia(m):
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx, sy * 0.8, sz), en=(0, sy * 0.1, 0), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=3)
    m.caja((sx * 0.82, sy * 0.3, sz * 0.7), en=(0, -sy * 0.34, 0), mat='lente', pieza=p, marco=p, bisel=0.008, seg=3)
    # a wire guard over the lens
    for x in (-sx * 0.25, 0, sx * 0.25):
        m.tubo([(x, -sy * 0.2, -sz * 0.42), (x, -sy * 0.56, -sz * 0.3), (x, -sy * 0.56, sz * 0.3), (x, -sy * 0.2, sz * 0.42)], 0.0025, 'acero', p, p, lados=6, codo=0.01)


@receta('tira_suelo')
def tira_suelo(m):
    """A floor light: an aluminium channel with its strip of lens."""
    p = m['']
    sx, sy, sz = p.tam
    m.extrusion([(-sx / 2, -sy / 2), (sx / 2, -sy / 2), (sx / 2, sy / 2), (sx / 2 - 0.004, sy / 2), (sx / 2 - 0.006, -sy / 2 + 0.006), (-sx / 2 + 0.006, -sy / 2 + 0.006), (-sx / 2 + 0.004, sy / 2), (-sx / 2, sy / 2)], sz, mat='aluminio', pieza=p, marco=p, eje='z')
    m.caja((sx - 0.012, sy * 0.5, sz - 0.01), en=(0, sy * 0.12, 0), mat=TINTE, pieza=p, marco=p, bisel=0.003, seg=2)
    for z in (-sz / 2, sz / 2):
        m.caja((sx + 0.002, sy + 0.002, 0.012), en=(0, 0, z * 0.995), mat='plastico_negro', pieza=p, marco=p, bisel=0.002, seg=1)


@receta('foco')
def foco(m):
    """A floodlight: a can with cooling fins round its back, a bezel round its glass, on a yoke."""
    p = m['carcasa']
    r0, r1, h = p.r, p.r * p.taper, p.h
    m.torno([(0, -h / 2), (r0 * 0.8, -h / 2), (r0, -h / 2 + 0.02), (r1, h / 2 - 0.022), (r1 + 0.006, h / 2 - 0.018), (r1 + 0.006, h / 2), (r1 - 0.01, h / 2), (r1 - 0.014, h / 2 - 0.012), (0, h / 2 - 0.012)], mat=TINTE, pieza=p, marco=p, lados=32)
    for k in range(14):
        marco = p.girado((0, 360 * k / 14, 0))
        m.caja((0.018, h * 0.42, 0.004), en=(r0 + 0.006, -h * 0.22, 0), mat=TINTE, pieza=p, marco=marco, bisel=0.001, seg=1)
    m.tubo([(r1 + 0.02, 0, 0), (r1 + 0.02, -h / 2 - 0.02, 0), (-r1 - 0.02, -h / 2 - 0.02, 0), (-r1 - 0.02, 0, 0)], 0.008, 'acero_oscuro', p, p, lados=8, codo=0.025)
    for x in (-1, 1):
        m.cilindro(0.016, 0.02, en=(x * (r1 + 0.012), 0, 0), mat='acero', pieza=p, marco=p, eje='x', lados=10, bisel=0.003)
