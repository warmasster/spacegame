"""The plain parts stations, towers and landers are built of (`assets/defs/structures/parts`):
each of one shape, its model in `parte_<id>.glb`."""
import math

from kit import TINTE, hundir, receta, rugoso
from recetas.exterior import campana_de_tobera


@receta('parte_viga')
def viga(m):
    """A beam: an I section with a stiffener every so often and a plate at each end."""
    p = m['']
    sx, sy, sz = p.tam
    a, t = sx / 2, sx * 0.12
    perfil = [(-a, -sy / 2), (a, -sy / 2), (a, -sy / 2 + t), (t / 2, -sy / 2 + t), (t / 2, sy / 2 - t), (a, sy / 2 - t), (a, sy / 2), (-a, sy / 2), (-a, sy / 2 - t), (-t / 2, sy / 2 - t), (-t / 2, -sy / 2 + t), (-a, -sy / 2 + t)]
    m.extrusion(perfil, sz - 0.04, mat=TINTE, pieza=p, marco=p, eje='z', bisel=0.006, seg=1)
    for z in (-1, 1):
        m.caja((sx, sy, 0.02), en=(0, 0, z * (sz / 2 - 0.01)), mat=TINTE, pieza=p, marco=p, bisel=0.005)
        m.tornillos([(x * a * 0.6, y * sy * 0.3, z * sz / 2) for x in (-1, 1) for y in (-1, 1)], 0.016, 0.01, 'acero_oscuro', p, p, eje=(0, 0, z))
    for k in range(1, 4):
        z = -sz / 2 + sz * k / 4
        m.caja((sx * 0.94, sy - 2 * t, 0.012), en=(0, 0, z), mat=TINTE, pieza=p, marco=p, bisel=0.002, seg=1)


@receta('parte_pilar')
def pilar(m):
    """A column: an H section on a base plate with its anchor bolts, a cap plate on top."""
    p = m['']
    sx, sy, sz = p.tam
    a, t = sx / 2, sx * 0.1
    perfil = [(-a, -sz / 2), (a, -sz / 2), (a, -sz / 2 + t), (t / 2, -sz / 2 + t), (t / 2, sz / 2 - t), (a, sz / 2 - t), (a, sz / 2), (-a, sz / 2), (-a, sz / 2 - t), (-t / 2, sz / 2 - t), (-t / 2, -sz / 2 + t), (-a, -sz / 2 + t)]
    m.extrusion(perfil, sy - 0.06, mat=TINTE, pieza=p, marco=p, eje='y', bisel=0.006, seg=1)
    for y in (-1, 1):
        m.caja((sx, 0.03, sz), en=(0, y * (sy / 2 - 0.015), 0), mat=TINTE, pieza=p, marco=p, bisel=0.006)
    m.tornillos([(x * a * 0.75, -sy / 2 + 0.03, z * sz * 0.38) for x in (-1, 1) for z in (-1, 1)], 0.022, 0.016, 'acero_oscuro', p, p)


@receta('parte_pata')
def pata(m):
    """A landing leg: a tube strut with a collar at each end and a dished foot pad."""
    p = m['']
    sx, sy, sz = p.tam
    r = sx * 0.3
    m.cilindro(r, sy - 0.1, en=(0, 0.05, 0), mat=TINTE, pieza=p, marco=p, lados=20, bisel=0.006)
    for y in (-sy * 0.3, sy * 0.36):
        m.torno([(r, -0.03), (r + 0.012, -0.022), (r + 0.012, 0.022), (r, 0.03)], en=(0, y, 0), mat='acero', pieza=p, marco=p, lados=20)
    m.torno([(0, -sy / 2), (sx * 0.5, -sy / 2), (sx * 0.5, -sy / 2 + 0.02), (r * 1.2, -sy / 2 + 0.07), (r, -sy / 2 + 0.1)], mat='acero_oscuro', pieza=p, marco=p, lados=24)
    m.esfera(r * 1.1, en=(0, -sy / 2 + 0.1, 0), mat='acero', pieza=p, marco=p, lados=16)


@receta('parte_mastil')
def mastil(m):
    """A mast: a tube in bolted lengths with rungs up one side."""
    p = m['']
    r, h = p.r, p.h
    m.cilindro(r * 0.8, h, mat=TINTE, pieza=p, marco=p, lados=20, bisel=0.01)
    n = 4
    for k in range(n + 1):
        y = -h / 2 + h * k / n
        y = min(max(y, -h / 2 + 0.03), h / 2 - 0.03)
        m.brida(r, en=(0, y, 0), grosor=0.03, pernos=8, mat=TINTE, pieza=p, marco=p, r_int=r * 0.78)
    for k in range(14):
        y = -h / 2 + 0.3 + (h - 0.6) * k / 13
        m.tubo([(r * 0.7, y, -0.07), (r * 0.7 + 0.1, y, -0.07), (r * 0.7 + 0.1, y, 0.07), (r * 0.7, y, 0.07)], 0.008, 'acero', p, p, lados=6, codo=0.02)


@receta('parte_placa')
def placa(m):
    """A deck plate: a frame with a plate of tread let into it, bolted down at its corners."""
    p = m['']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=2)
    hundir(ob, (0, 1, 0), 0.08, 0.012, marco=p)
    hundir(ob, (0, -1, 0), 0.08, 0.012, marco=p)
    m.tornillos([(x * (sx / 2 - 0.04), sy / 2, z * (sz / 2 - 0.04)) for x in (-1, 0, 1) for z in (-1, 0, 1) if x or z], 0.016, 0.008, 'acero_oscuro', p, p)


@receta('parte_casco')
def casco(m):
    """A cabin shell: a box of framed panels, a hatch in one wall with its coaming and handle,
    a port in the others."""
    p = m['']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.06, seg=3)
    for lado in ((1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0)):
        hundir(ob, lado, 0.16, 0.03, marco=p)
    # a hatch aft
    m.caja((0.9, 1.5, 0.05), en=(0, -0.15, -sz / 2 + 0.012), mat=TINTE, pieza=p, marco=p, bisel=0.02, seg=3)
    m.torno([(0.12, -0.01), (0.14, 0), (0.12, 0.01), (0.1, 0)], en=(0, -0.1, -sz / 2 - 0.03), mat='acero', pieza=p, marco=p, eje='z', lados=20)
    m.cilindro(0.03, 0.05, en=(0, -0.1, -sz / 2 - 0.01), mat='acero', pieza=p, marco=p, eje='z', lados=12, bisel=0.006)
    for lado in (-1, 1):
        m.torno([(0.24, 0), (0.28, 0.008), (0.28, 0.03), (0.24, 0.03), (0.22, 0.02), (0, 0.02)], en=(lado * (sx / 2 - 0.03), 0.3, 0), mat='acero', pieza=p, marco=p, eje=(lado, 0, 0), lados=28)
        m.cilindro(0.2, 0.01, en=(lado * (sx / 2 - 0.004), 0.3, 0), mat='cristal_oscuro', pieza=p, marco=p, eje='x', lados=28, bisel=0.002)


@receta('parte_modulo')
def modulo(m):
    """A habitat module: a barrel with ring frames, a stringer every eighth of the way round,
    a docking collar on each end's dome, a port each side and handrails along its top."""
    p = m['']
    r, h = p.r, p.h
    fondo = 0.3
    m.capsula(r * 0.985, h, mat=TINTE, pieza=p, marco=p, lados=48, fondo=fondo)
    recto = h - 2 * r * fondo
    for k in range(6):
        y = -recto / 2 + recto * k / 5
        m.torno([(r * 0.985, -0.06), (r, -0.04), (r, 0.04), (r * 0.985, 0.06)], en=(0, y, 0), mat=TINTE, pieza=p, marco=p, lados=48)
    for k in range(8):
        a = math.tau * (k + 0.5) / 8
        m.caja((0.07, recto, 0.03), en=(r * 0.992 * math.cos(a), 0, r * 0.992 * math.sin(a)), mat=TINTE, pieza=p, marco=p.girado((0, -math.degrees(a) + 90, 0)).en((0, 0, 0)), bisel=0.008) if False else None
    for y in (-1, 1):
        m.torno([(0.75, 0), (0.75, 0.1), (0.62, 0.1), (0.62, 0)], en=(0, y * (h / 2 - 0.1) - (0.1 if y > 0 else 0), 0), mat='acero', pieza=p, marco=p, lados=32)
    for lado in (-1, 1):
        m.torno([(0.3, 0), (0.36, 0.01), (0.36, 0.05), (0.3, 0.05), (0.27, 0.03), (0, 0.03)], en=(lado * (r - 0.03), 0.8, 0), mat='acero', pieza=p, marco=p, eje=(lado, 0, 0), lados=28)
        m.cilindro(0.26, 0.012, en=(lado * (r - 0.004), 0.8, 0), mat='cristal_oscuro', pieza=p, marco=p, eje='x', lados=28, bisel=0.002)
    for x in (-0.5, 0.5):
        m.asa((x, -recto * 0.4, r - 0.01), (x, recto * 0.4, r - 0.01), (0, 0, 1), alto=0.1, r=0.02, mat='pintura_amarilla', pieza=p, marco=p)


@receta('parte_tunel')
def tunel(m):
    """A tunnel between modules: a tube with a bolted flange each end and hoops between."""
    p = m['']
    r, h = p.r, p.h
    m.torno([(r * 0.9, -h / 2), (r * 0.96, -h / 2), (r * 0.96, h / 2), (r * 0.9, h / 2), (r * 0.9, -h / 2)], mat=TINTE, pieza=p, marco=p, lados=40)
    for y in (-1, 1):
        m.brida(r, en=(0, y * (h / 2 - 0.03), 0), grosor=0.06, pernos=16, mat=TINTE, pieza=p, marco=p, r_int=r * 0.9)
    for y in (-h * 0.18, h * 0.18):
        m.torno([(r * 0.96, -0.04), (r * 0.985, -0.03), (r * 0.985, 0.03), (r * 0.96, 0.04)], en=(0, y, 0), mat=TINTE, pieza=p, marco=p, lados=40)


@receta('parte_tanque')
def tanque(m):
    """A tank: a capsule with its bands, a manhole on top, a valve and its pipe at the bottom."""
    p = m['']
    r, h = p.r, p.h
    m.capsula(r * 0.98, h, mat=TINTE, pieza=p, marco=p, lados=44, fondo=0.5)
    for y in (-h * 0.22, h * 0.22):
        m.torno([(r * 0.98, -0.05), (r, -0.035), (r, 0.035), (r * 0.98, 0.05)], en=(0, y, 0), mat='acero_oscuro', pieza=p, marco=p, lados=44)
    m.brida(0.3, en=(0, h / 2 - 0.03, 0), grosor=0.04, pernos=12, mat='acero_oscuro', pieza=p, marco=p, r_int=0.16)
    m.cilindro(0.16, 0.06, en=(0, h / 2 - 0.02, 0), mat='acero_oscuro', pieza=p, marco=p, lados=20, bisel=0.01)
    m.tubo([(r * 0.5, -h / 2 + 0.2, 0), (r * 0.9, -h / 2 + 0.1, 0), (r * 0.9, -h * 0.2, 0)], 0.04, 'acero', p, p, lados=10, codo=0.1)
    m.cilindro(0.07, 0.12, en=(r * 0.9, -h * 0.2, 0), mat='pintura_roja', pieza=p, marco=p, lados=12, bisel=0.012)


@receta('parte_reactor')
def reactor(m):
    """A reactor vessel: a ribbed barrel under a bolted head with its drives, coolant nozzles
    with their flanges round it."""
    p = m['']
    r, h = p.r, p.h
    perfil = [(0, -h / 2), (r * 0.8, -h / 2), (r * 0.94, -h / 2 + 0.12)]
    for k in range(4):
        y = -h / 2 + 0.35 + (h - 0.9) * k / 3
        perfil += [(r * 0.94, y - 0.08), (r * 0.97, y - 0.05), (r * 0.97, y + 0.05), (r * 0.94, y + 0.08)]
    perfil += [(r * 0.94, h / 2 - 0.32), (r, h / 2 - 0.3), (r, h / 2 - 0.2), (r * 0.9, h / 2 - 0.2), (r * 0.6, h / 2 - 0.05), (0, h / 2 - 0.02)]
    m.torno(perfil, mat=TINTE, pieza=p, marco=p, lados=48)
    m.tornillos([(r * 0.95 * math.cos(math.tau * k / 24), h / 2 - 0.2, r * 0.95 * math.sin(math.tau * k / 24)) for k in range(24)], 0.03, 0.03, 'acero_oscuro', p, p)
    for k in range(7):
        a = math.tau * k / 6
        x, z = (0.42 * math.cos(a), 0.42 * math.sin(a)) if k < 6 else (0.0, 0.0)
        m.cilindro(0.07, 0.14, en=(x, h / 2 - 0.09, z), mat='acero_oscuro', pieza=p, marco=p, lados=12, bisel=0.01)
    for k in range(4):
        a = math.tau * (k + 0.5) / 4
        d = (math.cos(a), 0, math.sin(a))
        m.brida(0.24, en=(d[0] * (r * 0.97), -h * 0.1, d[2] * (r * 0.97)), grosor=0.05, pernos=10, mat='acero', pieza=p, marco=p, eje=d, r_int=0.14)


@receta('parte_panel_solar')
def panel_solar(m):
    """A solar panel: a frame with its cells in a grid of strings, a boom fitting on its back."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx, sy * 0.5, sz), en=(0, -sy * 0.1, 0), mat='aluminio', pieza=p, marco=p, bisel=0.006, seg=1)
    nx, nz = 8, 4
    for i in range(nx):
        for j in range(nz):
            x = -sx / 2 + sx * (i + 0.5) / nx
            z = -sz / 2 + sz * (j + 0.5) / nz
            m.caja((sx / nx - 0.03, sy * 0.3, sz / nz - 0.03), en=(x, sy * 0.3, z), mat=TINTE, pieza=p, marco=p, bisel=0.004, seg=1)
    m.caja((0.2, sy * 0.6, sz * 0.9), en=(0, -sy * 0.45, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.008)


@receta('parte_antena')
def antena(m):
    """A dish: a bowl looking up with its feed on four struts, on a short pedestal."""
    p = m['']
    r, h = p.r, p.h
    bowl, dentro = [], []
    for k in range(10):
        t = k / 9
        rr = r * (0.15 + 0.85 * t)
        y = -h / 2 + 0.03 + (h - 0.05) * t ** 2
        bowl.append((rr, y))
        dentro.append((rr, y + 0.012))
    m.torno([(0, -h / 2)] + bowl + [(r, bowl[-1][1] + 0.02)] + list(reversed(dentro)) + [(0, -h / 2 + 0.03)], mat=TINTE, pieza=p, marco=p, lados=40, liso=60)
    foco = h / 2 + 0.3
    m.cilindro(0.06, 0.12, en=(0, foco, 0), mat='acero_oscuro', pieza=p, marco=p, lados=14, bisel=0.01)
    for k in range(4):
        a = math.tau * (k + 0.5) / 4
        m.tubo([(r * 0.9 * math.cos(a), bowl[-2][1] + 0.01, r * 0.9 * math.sin(a)), (0.04 * math.cos(a), foco - 0.04, 0.04 * math.sin(a))], 0.012, 'acero', p, p, lados=6)


@receta('parte_luz')
def luz(m):
    """A floodlight: a finned housing with a bezel round its lens, on a bracket."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx, sy * 0.7, sz), en=(0, sy * 0.15, 0), mat=TINTE, pieza=p, marco=p, bisel=0.02, seg=3)
    m.aletas((sx * 0.86, sy * 0.2, sz * 0.86), en=(0, sy * 0.55, 0), n=9, grosor=0.008, mat='acero_oscuro', pieza=p, marco=p, apila='x')
    m.caja((sx * 0.84, sy * 0.2, sz * 0.84), en=(0, -sy * 0.3, 0), mat='lente', pieza=p, marco=p, bisel=0.012, seg=2)
    m.caja((sx * 0.96, sy * 0.12, sz * 0.96), en=(0, -sy * 0.2, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.01)


@receta('parte_motor')
def motor(m):
    """A descent engine: a chamber with its injector head, feed lines and a gimbal ring, over a
    wide bell with its hoops."""
    p = m['']
    r0, h = p.r, p.h
    r1 = r0 * p.taper
    # (its shape is wide at the bottom: the bell's mouth is down)
    m.torno([(0, h / 2), (r1 * 0.6, h / 2), (r1 * 0.7, h / 2 - 0.08), (r1 * 0.7, h / 2 - 0.4), (r1 * 0.4, h / 2 - 0.5)], mat='acero_oscuro', pieza=p, marco=p, lados=36)
    for k in range(24):
        marco = p.girado((0, 360 * k / 24, 0))
        m.caja((0.02, 0.3, 0.012), en=(r1 * 0.71, h / 2 - 0.25, 0), mat='acero_oscuro', pieza=p, marco=marco, bisel=0.003, seg=1)
    largo = h - 0.5
    perfil = [(rr, -h / 2 + largo - y) for rr, y in campana_de_tobera(r1 * 0.4, r0 * 0.98, largo, 0.02)]
    m.torno(perfil, mat=TINTE, pieza=p, marco=p, lados=44, liso=60)
    for t in (0.3, 0.62, 0.96):
        rr = r1 * 0.4 + (r0 * 0.98 - r1 * 0.4) * (1 - (1 - t) ** 1.8)
        m.torno([(rr, -0.02), (rr + 0.02, -0.01), (rr + 0.02, 0.01), (rr, 0.02)], en=(0, -h / 2 + largo * (1 - t), 0), mat='acero', pieza=p, marco=p, lados=44)
    m.torno([(r1 * 0.8, -0.03), (r1 * 0.95, -0.03), (r1 * 0.95, 0.03), (r1 * 0.8, 0.03), (r1 * 0.8, -0.03)], en=(0, h / 2 - 0.2, 0), mat='acero', pieza=p, marco=p, lados=36)
    m.tubo([(r1 * 0.3, h / 2 - 0.02, 0), (r1 * 0.9, h / 2 - 0.05, 0), (r1 * 0.95, h / 2 - 0.4, 0.1)], 0.04, 'acero', p, p, lados=10, codo=0.1)
    m.tubo([(-r1 * 0.3, h / 2 - 0.02, 0), (-r1 * 0.9, h / 2 - 0.05, 0), (-r1 * 0.95, h / 2 - 0.4, -0.1)], 0.04, 'cobre', p, p, lados=10, codo=0.1)


@receta('parte_bateria')
def bateria(m):
    """A battery pack: a ribbed case with a lid, its two terminals and a row of vents."""
    p = m['']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy * 0.86, sz), en=(0, -sy * 0.07, 0), mat=TINTE, pieza=p, marco=p, bisel=0.02, seg=3)
    for lado in ((0, 0, 1), (0, 0, -1), (1, 0, 0), (-1, 0, 0)):
        hundir(ob, lado, 0.05, 0.012, marco=p)
    m.caja((sx + 0.016, sy * 0.12, sz + 0.016), en=(0, sy * 0.4, 0), mat='plastico_negro', pieza=p, marco=p, bisel=0.01, seg=2)
    for k in range(6):
        m.cilindro(0.025, 0.016, en=(-sx * 0.36 + sx * 0.72 * k / 5, sy * 0.47, -sz * 0.18), mat='plastico_gris', pieza=p, marco=p, lados=12, bisel=0.004)
    for x, mat in ((-sx * 0.3, 'pintura_roja'), (sx * 0.3, 'plastico_negro')):
        m.cilindro(0.035, 0.05, en=(x, sy * 0.48, sz * 0.2), mat=mat, pieza=p, marco=p, lados=14, bisel=0.008)



@receta('parte_losa')
def losa(m):
    """A slab: cast concrete in four pads between joints, a steel angle round its edge, four
    sockets to lift it by."""
    p = m['']
    sx, sy, sz = p.tam
    j = 0.02
    for x in (-1, 1):
        for z in (-1, 1):
            m.caja((sx / 2 - j, sy - 0.012, sz / 2 - j), en=(x * sx / 4, 0.006, z * sz / 4), mat=TINTE, pieza=p, marco=p, bisel=0.018, seg=2)
    m.caja((sx - 0.01, sy - 0.05, sz - 0.01), en=(0, -0.02, 0), mat=TINTE, pieza=p, marco=p, bisel=0.01, seg=1)
    # the angle round its foot
    for x in (-1, 1):
        m.caja((0.012, 0.09, sz + 0.004), en=(x * sx / 2, -sy / 2 + 0.045, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.002, seg=1)
    for z in (-1, 1):
        m.caja((sx + 0.004, 0.09, 0.012), en=(0, -sy / 2 + 0.045, z * sz / 2), mat='acero_oscuro', pieza=p, marco=p, bisel=0.002, seg=1)
    # lifting sockets, each a steel cup sunk in its pad
    for x in (-1, 1):
        for z in (-1, 1):
            c = (x * sx * 0.3, sy / 2 - 0.004, z * sz * 0.3)
            m.torno([(0.0, -0.02), (0.045, -0.02), (0.06, 0.004), (0.07, 0.004), (0.07, 0.0), (0.062, 0.0)], en=c, mat='acero_oscuro', pieza=p, marco=p, lados=18)
            m.tubo([(c[0] - 0.03, c[1] - 0.012, c[2]), (c[0] + 0.03, c[1] - 0.012, c[2])], 0.007, 'acero', p, p, lados=8)


def muro_de(m, p, largo_eje):
    """A wall panel along `largo_eje` ('x' or 'z'): precast concrete in bays between joints, the
    marks of its formwork ties, a kerb at its foot, a steel plate cast in at each top corner."""
    sx, sy, sz = p.tam
    L, t = (sx, sz) if largo_eje == 'x' else (sz, sx)
    otro = 'z' if largo_eje == 'x' else 'x'

    def v(a, y, w):
        return (a, y, w) if largo_eje == 'x' else (w, y, a)

    n = max(1, int(round(L / 2.0)))
    paso = L / n
    for k in range(n):
        a = -L / 2 + paso * (k + 0.5)
        tam = v(paso - 0.02, sy - 0.1, t)
        ob = m.caja((abs(tam[0]), abs(tam[1]), abs(tam[2])), en=v(a, 0.05, 0), mat=TINTE, pieza=p, marco=p, bisel=0.016, seg=2)
        # the tie marks, a shallow hole each, on both faces
        pts = []
        for da in (-paso * 0.28, paso * 0.28):
            for y in (-sy * 0.26, 0.05, sy * 0.3):
                for lado in (-1, 1):
                    pts.append(v(a + da, y, lado * t / 2))
        m.taladrar(ob, pts, 0.03, 0.03, eje=otro, marco=p, lados=14)
    tam = v(L, 0.1, t + 0.03)
    m.caja((abs(tam[0]), abs(tam[1]), abs(tam[2])), en=(0, -sy / 2 + 0.05, 0), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=1)
    tam = v(L - 0.006, sy - 0.14, t - 0.03)
    m.caja((abs(tam[0]), abs(tam[1]), abs(tam[2])), en=(0, 0.04, 0), mat=TINTE, pieza=p, marco=p, bisel=0.0, seg=1)
    for lado in (-1, 1):
        tam = v(0.24, 0.012, t + 0.004)
        m.caja((abs(tam[0]), abs(tam[1]), abs(tam[2])), en=v(lado * (L / 2 - 0.16), sy / 2 - 0.006, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.002, seg=1)
        m.tornillos([v(lado * (L / 2 - 0.16) + d, sy / 2, 0) for d in (-0.07, 0.07)], 0.014, 0.01, 'acero', p, p, eje='y')


@receta('parte_muro')
def muro(m):
    muro_de(m, m[''], 'x')


@receta('parte_muro_lado')
def muro_lado(m):
    muro_de(m, m[''], 'z')


@receta('parte_bloque')
def bloque(m):
    """A block of sintered regolith: its faces rough as they came out of the mould, its edges
    worn, a lifting groove along each long side."""
    p = m['']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.035, seg=2)
    rugoso(ob, 0.014, 5.0, 3)
    for z in (-1, 1):
        ranura = m.caja((sx * 0.5, 0.05, 0.03), en=(0, sy * 0.12, z * (sz / 2 - 0.006)), mat=TINTE, pieza=p, marco=p, bisel=0.006, seg=1)
        m.restar(ob, ranura)


@receta('parte_rampa')
def rampa(m):
    """A ramp (a wedge: its top at -z, down to the ground at +z): a deck of tread plate between
    two stringers, cleats across it, posts and braces under its high end."""
    p = m['']
    sx, sy, sz = p.tam
    hx, hy, hz = sx / 2, sy / 2, sz / 2
    ang = math.degrees(math.atan2(sy, sz))
    largo = math.hypot(sy, sz)
    # the deck's frame: along the slope, its middle at the wedge's middle
    marco = p.girado((ang, 0, 0))
    m.caja((sx - 0.12, 0.03, largo - 0.02), en=(0, -0.02, 0), mat=TINTE, pieza=p, marco=marco, bisel=0.004, seg=1)
    for x in (-1, 1):
        m.caja((0.06, 0.16, largo), en=(x * (hx - 0.03), -0.06, 0), mat=TINTE, pieza=p, marco=marco, bisel=0.008, seg=1)
    n = int(largo / 0.3)
    for k in range(1, n):
        m.caja((sx - 0.14, 0.012, 0.03), en=(0, 0.0, -largo / 2 + largo * k / n), mat='acero_oscuro', pieza=p, marco=marco, bisel=0.003, seg=1)
    # under its high end: two posts on feet, a brace each, ties between them
    for x in (-1, 1):
        m.caja((0.07, sy - 0.14, 0.07), en=(x * (hx - 0.035), -0.09, -hz + 0.08), mat=TINTE, pieza=p, marco=p, bisel=0.008, seg=1)
        m.caja((0.16, 0.02, 0.16), en=(x * (hx - 0.05), -hy + 0.01, -hz + 0.09), mat='acero_oscuro', pieza=p, marco=p, bisel=0.004, seg=1)
        m.tubo([(x * (hx - 0.035), -hy + 0.06, -hz + 0.1), (x * (hx - 0.035), -hy * 0.1, 0.0)], 0.02, 'acero_oscuro', p, p, lados=8)
    m.tubo([(-hx + 0.05, -hy + 0.2, -hz + 0.08), (hx - 0.05, hy - 0.3, -hz + 0.08)], 0.016, 'acero_oscuro', p, p, lados=8)
    m.tubo([(hx - 0.05, -hy + 0.2, -hz + 0.08), (-hx + 0.05, hy - 0.3, -hz + 0.08)], 0.016, 'acero_oscuro', p, p, lados=8)
    # its foot: a plate on the ground
    m.caja((sx, 0.012, 0.2), en=(0, -hy + 0.006, hz - 0.1), mat='acero_oscuro', pieza=p, marco=p, bisel=0.003, seg=1)


@receta('parte_ventana')
def ventana(m):
    """A window: its pane (the part's own glass) in an aluminium frame with a gasket, a bar
    across its middle."""
    p = m['']
    sx, sy, sz = p.tam
    f = 0.055
    m.caja((sx - 2 * f + 0.01, sy - 2 * f + 0.01, sz * 0.3), mat=TINTE, pieza=p, marco=p, bisel=0.0, seg=1)
    for y in (-1, 1):
        m.caja((sx, f, sz), en=(0, y * (sy / 2 - f / 2), 0), mat='aluminio', pieza=p, marco=p, bisel=0.006, seg=2)
        m.caja((sx - 2 * f, 0.012, sz * 0.6), en=(0, y * (sy / 2 - f - 0.004), 0), mat='goma', pieza=p, marco=p, bisel=0.002, seg=1)
    for x in (-1, 1):
        m.caja((f, sy - 2 * f + 0.004, sz), en=(x * (sx / 2 - f / 2), 0, 0), mat='aluminio', pieza=p, marco=p, bisel=0.006, seg=2)
        m.caja((0.012, sy - 2 * f, sz * 0.6), en=(x * (sx / 2 - f - 0.004), 0, 0), mat='goma', pieza=p, marco=p, bisel=0.002, seg=1)
    m.caja((0.03, sy - 2 * f + 0.004, sz * 0.8), mat='aluminio', pieza=p, marco=p, bisel=0.004, seg=1)
    for z in (-1, 1):
        m.tornillos([(x * (sx / 2 - f / 2), y * (sy / 2 - f / 2), z * sz / 2) for x in (-1, 1) for y in (-1, 1)], 0.008, 0.004, 'acero', p, p, eje=(0, 0, z))
