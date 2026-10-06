"""What a ship carries outside: engines, tanks, radiators, aerials, magnets, cradles."""
import math

from kit import TINTE, hundir, receta


def campana_de_tobera(r_garganta, r_boca, largo, pared=0.012, pasos=10):
    """The profile of a bell nozzle from its throat (at 0) to its mouth (at `largo`): out and
    back in, so that it is hollow."""
    fuera, dentro = [], []
    for k in range(pasos + 1):
        t = k / pasos
        r = r_garganta + (r_boca - r_garganta) * (1 - (1 - t) ** 1.8)
        fuera.append((r, largo * t))
        dentro.append((max(r - pared, 0.01), largo * t))
    return fuera + list(reversed(dentro))


@receta('gondola')
def gondola(m):
    """An engine nacelle: a barrelled cowl with its panel bands and fasteners, a domed fairing
    at its nose, the stripe of its livery, and at its tail the engine's bell with its stiffening
    hoops and the turbine's exhaust beside it."""
    c, ad, fr, tb = m['carcasa'], m['admision'], m['franja'], m['tobera']
    r, h = c.r, c.h
    perfil = [(r * 0.86, -h / 2), (r * 0.96, -h / 2 + 0.12), (r, -h / 2 + 0.4)]
    for y in (-0.55, 0.25, 0.9):
        perfil += [(r, y - 0.012), (r - 0.008, y - 0.006), (r - 0.008, y + 0.006), (r, y + 0.012)]
    perfil += [(r, h / 2 - 0.2), (r * 0.985, h / 2)]
    m.torno(perfil, mat=TINTE, pieza=c, marco=c, lados=48)
    # an access panel each side, a scoop underneath
    for lado in (-1, 1):
        m.caja((0.02, 0.7, 0.42), en=(lado * (r - 0.004), -0.15, 0), mat=TINTE, pieza=c, marco=c, bisel=0.008, seg=2)
        m.tornillos([(lado * (r + 0.006), -0.15 + y, z) for y in (-0.31, 0, 0.31) for z in (-0.18, 0.18)], 0.01, 0.005, 'acero', c, c, eje=(lado, 0, 0))
    m.caja((0.3, 0.5, 0.12), en=(0, 0.5, -r + 0.01), mat=TINTE, pieza=c, marco=c, bisel=0.04, seg=3)
    # the nose: a ring and a dome
    ar, ah = ad.r, ad.h
    m.torno([(ar * 0.9, -ah / 2), (ar, -ah / 2 + 0.04), (ar, ah * 0.1), (ar * 0.93, ah / 2)], mat=TINTE, pieza=ad, marco=ad, lados=48)
    m.esfera(ar * 0.93, en=(0, ah / 2, 0), mat=TINTE, pieza=ad, marco=ad, lados=40, de=0, aplastar=0.55)
    # the stripe: a band standing a hair proud of the cowl
    fr_r, fr_h = fr.r, fr.h
    m.torno([(fr_r - 0.012, -fr_h / 2), (fr_r, -fr_h / 2 + 0.01), (fr_r, fr_h / 2 - 0.01), (fr_r - 0.012, fr_h / 2)], mat=TINTE, pieza=fr, marco=fr, lados=48)
    # the engine: a throat ring, the bell, hoops round it
    tr, th = tb.r, tb.h
    boca = tr * tb.taper
    m.torno([(tr * 0.95, -th / 2), (tr * 1.0, -th / 2 + 0.03), (tr * 0.62, -th / 2 + 0.1)], mat='acero_oscuro', pieza=tb, marco=tb, lados=40)
    m.torno([(r_, y - th / 2 + 0.1) for r_, y in campana_de_tobera(tr * 0.6, boca, th - 0.1, 0.016)], mat=TINTE, pieza=tb, marco=tb, lados=40, liso=60)
    for t in (0.35, 0.65, 0.97):
        y = -th / 2 + 0.1 + (th - 0.1) * t
        rr = tr * 0.6 + (boca - tr * 0.6) * (1 - (1 - t) ** 1.8)
        m.torno([(rr, -0.012), (rr + 0.014, -0.006), (rr + 0.014, 0.006), (rr, 0.012)], en=(0, y, 0), mat='acero', pieza=tb, marco=tb, lados=40)
    m.torno([(0.07, 0), (0.07, th * 0.55), (0.058, th * 0.55), (0.058, 0)], en=(tr * 0.82, -th / 2, 0), mat='acero_oscuro', pieza=tb, marco=tb, lados=14)


@receta('motor_elevacion')
def motor_elevacion(m):
    """A lift engine: a thrust chamber with its injector head and cooling jacket, a gimbal ring
    with its two actuators, the feed lines and their valves, and the bell under it."""
    c, tb = m['camara'], m['tobera']
    r, h = c.r, c.h
    perfil = [(0, h / 2), (r * 0.7, h / 2), (r * 0.82, h / 2 - 0.03), (r * 0.82, h / 2 - 0.08), (r * 0.7, h / 2 - 0.1)]
    perfil += [(r * 0.7, -h / 2 + 0.06), (r * 0.5, -h / 2)]
    m.torno(perfil, mat=TINTE, pieza=c, marco=c, lados=32)
    # the jacket: ribs down the chamber
    for k in range(20):
        marco = c.girado((0, 360 * k / 20, 0))
        m.caja((0.012, h * 0.62, 0.008), en=(r * 0.71, -h * 0.1, 0), mat=TINTE, pieza=c, marco=marco, bisel=0.002, seg=1)
    m.torno([(r * 0.86, -0.014), (r, -0.014), (r, 0.014), (r * 0.86, 0.014), (r * 0.86, -0.014)], en=(0, h * 0.08, 0), mat='acero', pieza=c, marco=c, lados=32)
    for a in (0.0, 90.0):
        marco = c.girado((0, a, 0))
        m.cilindro(0.018, 0.2, en=(r * 0.98, h * 0.2, 0), mat='acero', pieza=c, marco=marco, lados=10, bisel=0.004)
        m.cilindro(0.011, 0.12, en=(r * 0.98, h * 0.02, 0), mat='cromo', pieza=c, marco=marco, lados=10, bisel=0.002)
    m.tubo([(-r * 0.5, h / 2 - 0.02, 0), (-r * 0.9, h / 2 + 0.01, 0), (-r * 0.95, h * 0.1, 0.05)], 0.016, 'acero', c, c, lados=10, codo=0.04)
    m.tubo([(0, h / 2 - 0.02, -r * 0.5), (0, h / 2 + 0.01, -r * 0.9), (0.05, h * 0.1, -r * 0.95)], 0.016, 'cobre', c, c, lados=10, codo=0.04)
    for p in ((-r * 0.95, h * 0.1, 0.05), (0.05, h * 0.1, -r * 0.95)):
        m.cilindro(0.03, 0.05, en=p, mat='acero_oscuro', pieza=c, marco=c, lados=12, bisel=0.006)
    # the bell hangs under it (its piece is turned over: its own up is down)
    tr, th = tb.r, tb.h
    boca = tr * tb.taper
    m.torno([(r_, y - th / 2) for r_, y in campana_de_tobera(tr * 0.55, boca, th, 0.01)], mat=TINTE, pieza=tb, marco=tb, lados=36, liso=60)
    for t in (0.4, 0.96):
        rr = tr * 0.55 + (boca - tr * 0.55) * (1 - (1 - t) ** 1.8)
        m.torno([(rr, -0.008), (rr + 0.01, -0.004), (rr + 0.01, 0.004), (rr, 0.008)], en=(0, -th / 2 + th * t, 0), mat='acero', pieza=tb, marco=tb, lados=36)


def deposito(m, p, r, largo, bandas, tapa=True):
    """A propellant tank lying along its piece's axis: a capsule with its straps, a fill cap."""
    m.capsula(r, largo, mat=TINTE, pieza=p, marco=p, lados=40, fondo=0.5)
    for y in bandas:
        m.torno([(r, -0.03), (r + 0.008, -0.022), (r + 0.008, 0.022), (r, 0.03)], en=(0, y, 0), mat='acero', pieza=p, marco=p, lados=40)
    if tapa:
        m.cilindro(0.05, 0.03, en=(0, largo * 0.2, r + 0.006), mat='acero', pieza=p, marco=p, eje='z', lados=12, bisel=0.006)


@receta('deposito_tug')
def deposito_tug(m):
    p = m['']
    deposito(m, p, p.r, p.h, (-p.h * 0.28, p.h * 0.28))
    # its saddles, under the straps (the piece lies along the ship: its own z is down)
    for y in (-p.h * 0.28, p.h * 0.28):
        m.caja((p.r * 1.3, 0.06, 0.07), en=(0, y, p.r * 0.92), mat='acero_oscuro', pieza=p, marco=p, bisel=0.01)


@receta('deposito_ala')
def deposito_ala(m):
    """A wing tank: a long barrel with its straps, an ogive nose with a probe, a tail cone."""
    c, proa, popa = m['cuerpo'], m['proa'], m['popa']
    r, h = c.r, c.h
    perfil = [(r * 0.97, -h / 2), (r, -h / 2 + 0.05), (r, h / 2 - 0.05), (r * 0.97, h / 2)]
    m.torno(perfil, mat=TINTE, pieza=c, marco=c, lados=40)
    for y in (-h * 0.3, 0.0, h * 0.3):
        m.torno([(r, -0.03), (r + 0.008, -0.022), (r + 0.008, 0.022), (r, 0.03)], en=(0, y, 0), mat='acero', pieza=c, marco=c, lados=40)
    m.cilindro(0.06, 0.03, en=(0, h * 0.18, -r - 0.004), mat='acero', pieza=c, marco=c, eje='z', lados=12, bisel=0.006)
    # the nose: an ogive (its piece's axis runs to its tip)
    ph = proa.h
    ogiva = []
    for k in range(9):
        t = k / 8
        ogiva.append((proa.r * math.cos(t * math.pi / 2) ** 0.7 * (1 - 0.72 * t) + proa.r * 0.02, -ph / 2 + ph * t))
    ogiva.append((0, ph / 2))
    m.torno(ogiva, mat=TINTE, pieza=proa, marco=proa, lados=40, liso=70)
    m.cilindro(0.012, 0.16, en=(0, ph / 2 + 0.06, 0), mat='acero', pieza=proa, marco=proa, lados=8, bisel=0.002)
    qh = popa.h
    m.torno([(popa.r, -qh / 2), (popa.r * 0.96, -qh / 2 + qh * 0.3), (popa.r * popa.taper, qh / 2), (0, qh / 2)], mat=TINTE, pieza=popa, marco=popa, lados=40, liso=60)
    m.torno([(popa.r, -0.02), (popa.r + 0.008, -0.012), (popa.r + 0.008, 0.012), (popa.r, 0.02)], en=(0, -qh / 2 + 0.02, 0), mat='acero', pieza=popa, marco=popa, lados=40)


@receta('radiador_aleta')
def radiador_aleta(m):
    """A radiator panel: a frame, a header pipe along each long edge and the tubes across from
    one to the other, a hinge fitting at its root."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx, sy * 0.4, sz), mat=TINTE, pieza=p, marco=p, bisel=0.004, seg=1)
    for x in (-1, 1):
        m.cilindro(sy * 0.5, sz, en=(x * (sx / 2 - sy * 0.5), 0, 0), mat='acero', pieza=p, marco=p, eje='z', lados=10, bisel=0.004)
    n = 22
    for k in range(n):
        z = -sz / 2 + sz * (k + 0.5) / n
        for lado in (-1, 1):
            m.caja((sx - sy * 2, 0.008, 0.03), en=(0, lado * sy * 0.24, z), mat=TINTE, pieza=p, marco=p, bisel=0.003, seg=1)
    for z in (-1, 1):
        m.caja((sx, sy, 0.05), en=(0, 0, z * (sz / 2 - 0.025)), mat='acero_oscuro', pieza=p, marco=p, bisel=0.006)


@receta('antena')
def antena(m):
    """An aerial: a mast on a bolted foot with a collar half way up, and on it a dish with its
    feed horn held on three struts."""
    mast, plato = m['mastil'], m['plato']
    r, h = mast.r, mast.h
    m.torno([(r * 2.4, -h / 2), (r * 2.4, -h / 2 + 0.02), (r * 1.3, -h / 2 + 0.05), (r, -h / 2 + 0.12), (r, h / 2)], mat=TINTE, pieza=mast, marco=mast, lados=16)
    m.tornillos([(r * 1.9 * math.cos(math.tau * k / 4), -h / 2 + 0.02, r * 1.9 * math.sin(math.tau * k / 4)) for k in range(4)], 0.008, 0.005, 'acero_oscuro', mast, mast)
    m.torno([(r, -0.02), (r + 0.008, -0.014), (r + 0.008, 0.014), (r, 0.02)], en=(0, h * 0.1, 0), mat='acero_oscuro', pieza=mast, marco=mast, lados=16)
    # the dish (its piece narrows upward: the dish opens downward from the mast head... it is a
    # bowl looking up, its rim at the piece's wide end)
    pr, ph = plato.r, plato.h
    bowl, dentro = [], []
    for k in range(9):
        t = k / 8
        rr = pr * (0.12 + 0.88 * t)
        y = ph / 2 - ph * 0.9 * (1 - t ** 2) if False else -ph / 2 + ph * 0.9 * (t ** 2)
        bowl.append((rr, y))
        dentro.append((rr, y + 0.008))
    m.torno([(0, -ph / 2)] + bowl + [(pr, bowl[-1][1] + 0.012)] + list(reversed(dentro)) + [(0, -ph / 2 + 0.008)], mat=TINTE, pieza=plato, marco=plato, lados=36, liso=60)
    foco = ph / 2 + 0.16
    m.cilindro(0.03, 0.06, en=(0, foco, 0), mat='acero_oscuro', pieza=plato, marco=plato, lados=14, bisel=0.006)
    for k in range(3):
        a = math.tau * k / 3
        m.tubo([(pr * 0.86 * math.cos(a), bowl[-2][1] + 0.006, pr * 0.86 * math.sin(a)), (0.02 * math.cos(a), foco - 0.02, 0.02 * math.sin(a))], 0.006, 'acero', plato, plato, lados=6)


@receta('electroiman_carga')
def electroiman_carga(m):
    """A cargo magnet: a thick pad with its pole pieces showing underneath, hung from a pylon,
    its coil wound round the yoke with a terminal box."""
    pad, pilon, bob = m[''], m['pilon'], m['bobina']
    sx, sy, sz = pad.tam
    m.caja((sx, sy * 0.6, sz), en=(0, sy * 0.2, 0), mat=TINTE, pieza=pad, marco=pad, bisel=0.012, seg=3)
    for k in range(5):
        x = -sx * 0.4 + sx * 0.2 * k
        m.caja((sx * 0.1, sy * 0.45, sz * 0.92), en=(x, -sy * 0.26, 0), mat='acero_oscuro', pieza=pad, marco=pad, bisel=0.008, seg=2)
    m.pernos(pad, 'y', 0.06, (4, 4), 0.012, lado=1)
    px, py, pz = pilon.tam
    m.caja((px, py, pz), mat=TINTE, pieza=pilon, marco=pilon, bisel=0.012, seg=2)
    for a in (0, 90, 180, 270):
        marco = pilon.girado((0, a, 0))
        m.extrusion([(px / 2, -py / 2), (px / 2 + 0.1, -py / 2), (px / 2, py * 0.3)], 0.012, mat=TINTE, pieza=pilon, marco=marco, eje='z', bisel=0.002)
    r, h = bob.r, bob.h
    perfil = []
    n = 14
    for k in range(n + 1):
        a = math.tau * k / n
        perfil.append((r * 0.78 + r * 0.2 * math.cos(a) * 0.9, h * 0.5 * math.sin(a)))
    m.torno(perfil, mat=TINTE, pieza=bob, marco=bob, lados=36, liso=70)
    m.caja((0.1, h * 1.2, 0.08), en=(r * 0.95, 0, 0), mat='plastico_negro', pieza=bob, marco=bob, bisel=0.008)


@receta('cuna_atraque')
def cuna_atraque(m):
    """A docking cradle: a frame of box sections bolted at its corners, a rubber pad along each
    beam, and a claw each side that shuts over the skid."""
    for id in ('', 'viga_popa', 'larguero_i', 'larguero_d'):
        p = m[id]
        sx, sy, sz = p.tam
        m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=2)
        largo_x = sx > sz
        if largo_x:
            m.caja((sx * 0.9, 0.012, sz * 0.6), en=(0, sy / 2 + 0.004, 0), mat='goma', pieza=p, marco=p, bisel=0.004)
            m.tornillos([(x * (sx / 2 - 0.08), sy / 2, z * sz * 0.3) for x in (-1, 1) for z in (-1, 1)], 0.014, 0.008, 'acero', p, p)
        else:
            m.caja((sx * 0.6, 0.012, sz * 0.9), en=(0, sy / 2 + 0.004, 0), mat='goma', pieza=p, marco=p, bisel=0.004)
            m.tornillos([(x * sx * 0.3, sy / 2, z * (sz / 2 - 0.08)) for x in (-1, 1) for z in (-1, 1)], 0.014, 0.008, 'acero', p, p)
    for id, lado in (('garra_i', 1), ('garra_d', -1)):
        g = m[id]
        gx, gy, gz = g.tam
        # a hook: up, and in over the skid
        gancho = [(lado * gx / 2, -gy / 2), (lado * gx / 2, gy / 2), (-lado * gx * 0.9, gy / 2), (-lado * gx * 0.9, gy * 0.22), (-lado * gx * 0.1, gy * 0.22), (-lado * gx * 0.1, -gy / 2)]
        m.extrusion(gancho, gz, mat=TINTE, pieza=g, marco=g, eje='z', bisel=0.012, seg=2)
        m.cilindro(0.03, gz + 0.04, en=(lado * gx * 0.2, -gy * 0.3, 0), mat='acero', pieza=g, marco=g, eje='z', lados=14, bisel=0.006)
