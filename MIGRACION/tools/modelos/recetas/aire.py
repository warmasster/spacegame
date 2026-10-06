"""Air moved on purpose (docs/AIRE.md): the hand valves in the bulkheads, the compressor that
draws a compartment down and the tank that keeps what it recovers.

A valve's plate carries a panel (its wheel and its gauge, drawn by the game): its face is kept
flat and where its shape has it; what the model adds goes round it and under it.
"""
import math

from kit import TINTE, hundir, receta


def _placa(m, p, lado, franja=None):
    """The plate of a hand valve on one side of its wall: a cast tray bolted to the wall, its
    face flat for the panel, and under it the valve's mouth on that side — a short pipe out of
    the wall with a slatted cap — joined to the plate by its sensing line. `lado`: +1 the plate
    that looks along +z, -1 the one that looks along -z."""
    sx, sy, sz = p.tam
    m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.008, seg=3)
    # a foot all round where it meets the wall, and its bolts at the corners
    m.caja((sx + 0.016, sy + 0.016, 0.01), en=(0, 0, -lado * (sz / 2 - 0.005)), mat='acero_oscuro', pieza=p, marco=p, bisel=0.003, seg=1)
    m.tornillos([(x * (sx / 2 - 0.014), y * (sy / 2 - 0.011), lado * sz / 2) for x in (-1, 1) for y in (-1, 1)], 0.006, 0.004, 'acero', p, p, eje=(0, 0, lado))
    if franja:
        # a warning band down each edge of its face's sides (not on the face)
        for x in (-1, 1):
            m.caja((0.004, sy * 0.9, sz * 0.7), en=(x * (sx / 2 + 0.001), 0, 0), mat=franja, pieza=p, marco=p, bisel=0.001, seg=1)
    # the mouth: a pipe out of the wall under the plate, a flange on the wall, a slatted cap
    y = -sy / 2 - 0.06
    z0 = -lado * sz / 2
    m.brida(0.058, en=(0, y, z0 + lado * 0.006), grosor=0.012, pernos=6, mat='acero', pieza=p, marco=p, eje=(0, 0, lado), r_int=0.036)
    # (a bell with a rolled lip, hollow; its slats across the bore, tilted down)
    m.torno([(0.036, 0.0), (0.036, 0.036), (0.05, 0.05), (0.05, 0.066), (0.046, 0.07), (0.04, 0.07), (0.04, 0.05), (0.03, 0.04), (0.03, 0.0)], en=(0, y, z0), mat=TINTE, pieza=p, marco=p, eje=(0, 0, lado), lados=28)
    m.cilindro(0.03, 0.004, en=(0, y, z0 + lado * 0.03), mat='pintura_oscura', pieza=p, marco=p, eje='z', lados=20, bisel=0.0)
    m.rejilla((0.056, 0.056), en=(0, y, z0 + lado * 0.058), pieza=p, marco=p, lamas=5, grosor=0.003, inclinar=40.0 * lado, normal='z', mat='acero')
    # the line that tells its gauge: up from the mouth into the plate's edge
    m.tubo([(0.03, y + 0.02, z0 + lado * 0.03), (0.07, y + 0.03, z0 + lado * 0.03), (0.07, -sy / 2 + 0.01, z0 + lado * 0.03)], 0.006, 'cobre', p, p, lados=8, codo=0.015)
    m.cilindro(0.01, 0.014, en=(0.07, -sy / 2 + 0.003, z0 + lado * 0.03), mat='laton', pieza=p, marco=p, lados=6, bisel=0.002)


def _cuerpo_en_pared(m, p):
    """The valve itself, in its wall: a body between two flanges (its axis is the piece's y)."""
    r, h = p.r, p.h
    m.torno([(0, -h / 2), (r, -h / 2), (r, -h / 2 + 0.008), (r * 0.72, -h / 2 + 0.012), (r * 0.72, h / 2 - 0.012), (r, h / 2 - 0.008), (r, h / 2), (0, h / 2)], mat=TINTE, pieza=p, marco=p, lados=20)


@receta('valvula_igualacion')
def valvula_igualacion(m):
    """A pressure equalisation valve through a bulkhead: its body in the wall, a plate on each
    side with its mouth under it."""
    _cuerpo_en_pared(m, m[''])
    _placa(m, m['placa_a'], -1)
    _placa(m, m['placa_b'], 1)


@receta('valvula_venteo')
def valvula_venteo(m):
    """A vent valve out through the hull: a plate inside with its mouth, the valve through the
    wall, and outside its nozzle — a flange on the skin, a short flared pipe, a guard across."""
    p = m['']
    r, h = p.r, p.h
    # (its axis is the piece's y: inside the wall from -h/2, the wall's outer face 6 cm on)
    piel = -h / 2 + 0.06
    m.torno([(0, -h / 2), (r * 0.8, -h / 2), (r * 0.8, piel), (r * 0.72, piel), (r * 0.72, h / 2 - 0.03), (r, h / 2), (r * 0.9, h / 2), (r * 0.6, h / 2 - 0.035), (r * 0.6, piel + 0.01), (0, piel + 0.01)], mat=TINTE, pieza=p, marco=p, lados=24)
    m.brida(r + 0.03, en=(0, piel + 0.007, 0), grosor=0.014, pernos=8, mat='acero', pieza=p, marco=p, r_int=r * 0.72)
    for a in (0, 90):
        marco = p.girado((0, a, 0))
        m.caja((r * 1.8, 0.004, 0.006), en=(0, h / 2 - 0.006, 0), mat='acero', pieza=p, marco=marco, bisel=0.001, seg=1)
    _placa(m, m['placa_a'], -1, franja='pintura_amarilla')


@receta('compresor_aire')
def compresor_aire(m):
    """A four-stage piston compressor for breathing air: a skid that is its receiver, with the
    oil glass, the drain and a gauge on its service side; on it the block — a crankcase and four
    finned cylinders, each smaller than the one before, their heads joined by the coils of the
    intercoolers — and the motor that drives it through a guarded coupling."""
    c, e, mo = m['cuerpo'], m['etapas'], m['motor']
    sx, sy, sz = c.tam
    # ---- the skid: a welded base, its service face (-x) flat for what is written on it
    ob = m.caja((sx, sy * 0.86, sz), en=(0, sy * 0.07, 0), mat=TINTE, pieza=c, marco=c, bisel=0.016, seg=3)
    for lado in ((0, 0, 1), (0, 0, -1)):
        hundir(ob, lado, 0.04, 0.008, marco=c)
    for x in (-1, 1):
        m.caja((0.07, sy * 0.14, sz), en=(x * (sx / 2 - 0.05), -sy / 2 + sy * 0.07, 0), mat='acero_oscuro', pieza=c, marco=c, bisel=0.008)
        for z in (-1, 1):
            m.cilindro(0.035, 0.02, en=(x * (sx / 2 - 0.05), -sy / 2 + 0.008, z * (sz / 2 - 0.07)), mat='goma', pieza=c, marco=c, lados=14, bisel=0.005)
    m.pernos(c, 'y', 0.05, (2, 3), 0.008, lado=1)
    # the oil sight glass, the drain cock and a gauge, at the ends of its service face
    m.cilindro(0.028, 0.014, en=(-sx / 2 - 0.004, -sy * 0.2, -sz * 0.4), mat='laton', pieza=c, marco=c, eje='x', lados=18, bisel=0.003)
    m.cilindro(0.021, 0.004, en=(-sx / 2 - 0.012, -sy * 0.2, -sz * 0.4), mat='lente', pieza=c, marco=c, eje='x', lados=18, bisel=0.001)
    m.cilindro(0.012, 0.03, en=(-sx / 2 - 0.012, -sy * 0.3, sz * 0.4), mat='laton', pieza=c, marco=c, eje='x', lados=8, bisel=0.002)
    m.caja((0.008, 0.012, 0.05), en=(-sx / 2 - 0.03, -sy * 0.3, sz * 0.4), mat='pintura_roja', pieza=c, marco=c, bisel=0.002)
    m.cilindro(0.04, 0.02, en=(-sx / 2 - 0.006, sy * 0.24, sz * 0.4), mat='acero', pieza=c, marco=c, eje='x', lados=20, bisel=0.004)
    m.cilindro(0.034, 0.004, en=(-sx / 2 - 0.017, sy * 0.24, sz * 0.4), mat='lente', pieza=c, marco=c, eje='x', lados=20, bisel=0.001)
    for z in (-1, 1):
        m.asa((-sx / 2, sy * 0.3, z * sz * 0.2 - 0.07), (-sx / 2, sy * 0.3, z * sz * 0.2 + 0.07), (-1, 0, 0), alto=0.035, r=0.008, mat='acero', pieza=c, marco=c)
    # ---- the block: crankcase, four cylinders in a row (along z), each with its fins and head
    ex, ey, ez = e.tam
    m.caja((ex * 0.9, ey * 0.42, ez * 0.96), en=(0, -ey * 0.29, 0), mat=TINTE, pieza=e, marco=e, bisel=0.02, seg=3)
    m.cilindro(ey * 0.17, 0.02, en=(-ex * 0.45 - 0.006, -ey * 0.29, 0), mat='acero_oscuro', pieza=e, marco=e, eje='x', lados=20, bisel=0.004)
    radios = (0.062, 0.05, 0.038, 0.028)
    zs = (-ez * 0.33, -ez * 0.06, ez * 0.17, ez * 0.35)
    cabezas = []
    for r, z in zip(radios, zs):
        alto = ey * 0.44
        y0 = -ey * 0.08
        m.cilindro(r * 0.8, alto, en=(0, y0 + alto / 2, z), mat=TINTE, pieza=e, marco=e, lados=20, bisel=0.004)
        for k in range(6):
            m.cilindro(r, 0.005, en=(0, y0 + alto * (0.12 + 0.13 * k), z), mat='aluminio', pieza=e, marco=e, lados=20, bisel=0.0015)
        m.cilindro(r * 0.92, 0.022, en=(0, y0 + alto + 0.008, z), mat='acero_oscuro', pieza=e, marco=e, lados=8, bisel=0.004)
        cabezas.append((0, y0 + alto + 0.02, z))
    # the intercoolers: from each head out to starboard of the block, a coil, and into the next
    for (a, b) in zip(cabezas, cabezas[1:]):
        x = ex * 0.36
        m.tubo([a, (x * 0.5, a[1] + 0.02, a[2]), (x, a[1] - 0.03, a[2]), (x, a[1] - 0.14, (a[2] + b[2]) / 2), (x, b[1] - 0.03, b[2]), (x * 0.5, b[1] + 0.02, b[2]), b], 0.007, 'cobre', e, e, lados=8, codo=0.02)
    m.aletas((0.05, ey * 0.3, ez * 0.8), en=(ex * 0.42, ey * 0.1, 0), n=14, grosor=0.003, mat='aluminio', pieza=e, marco=e, apila='z')
    # the delivery line off the last head, with its relief valve
    ult = cabezas[-1]
    m.tubo([ult, (-ex * 0.3, ult[1] + 0.02, ult[2]), (-ex * 0.42, ult[1] - 0.08, ult[2])], 0.008, 'acero', e, e, lados=8, codo=0.02)
    m.cilindro(0.014, 0.04, en=(-ex * 0.42, ult[1] - 0.1, ult[2]), mat='laton', pieza=e, marco=e, lados=6, bisel=0.003)
    m.cilindro(0.009, 0.016, en=(-ex * 0.42, ult[1] - 0.128, ult[2]), mat='pintura_roja', pieza=e, marco=e, lados=8, bisel=0.002)
    # ---- the motor (its axis along z): a finned body, its end bell, the terminal box, the guard
    # of its coupling toward the block
    r, h = mo.r, mo.h
    m.cilindro(r * 0.86, h * 0.8, en=(0, h * 0.06, 0), mat=TINTE, pieza=mo, marco=mo, lados=28, bisel=0.01)
    for k in range(18):
        marco = mo.girado((0, 360 * k / 18, 0))
        m.caja((0.004, h * 0.62, 0.02), en=(0, h * 0.06, r * 0.9), mat=TINTE, pieza=mo, marco=marco, bisel=0.001, seg=1)
    m.torno([(0, h / 2 - 0.02), (r * 0.5, h / 2), (r * 0.86, h / 2 - 0.03), (r * 0.86, h / 2 - 0.05)], mat='acero_oscuro', pieza=mo, marco=mo, lados=28)
    m.torno([(0, -h / 2), (r * 0.95, -h / 2), (r * 0.95, -h / 2 + 0.07), (r * 0.86, -h / 2 + 0.08), (0, -h / 2 + 0.08)], mat='pintura_amarilla', pieza=mo, marco=mo, lados=28)
    # (the piece's z looks down: its terminal box is on top of it)
    m.caja((0.1, 0.09, 0.05), en=(0, h * 0.05, -r * 0.92), mat='plastico_negro', pieza=mo, marco=mo, bisel=0.008)
    m.tornillos([(x * 0.035, h * 0.05 + y * 0.03, -r * 0.92 - 0.025) for x in (-1, 1) for y in (-1, 1)], 0.004, 0.002, 'acero', mo, mo, eje=(0, 0, -1))
    for y in (-1, 1):
        m.caja((r * 1.5, 0.03, 0.02), en=(0, y * h * 0.3, r * 0.92), mat='acero_oscuro', pieza=mo, marco=mo, bisel=0.004)


@receta('deposito_aire')
def deposito_aire(m):
    """A tank of recovered air: a wound vessel lying on its cradle — two saddles on a pair of
    rails — held down by a strap over each; at one end its neck with the shut-off valve, its
    gauge and the relief valve, and the line that leaves it."""
    c, q = m['cuerpo'], m['cuna']
    r, h = c.r, c.h
    # (its axis is the piece's y; the piece's z looks down)
    m.capsula(r, h, mat=TINTE, pieza=c, marco=c, lados=40, fondo=0.62)
    for y in (-h * 0.3, h * 0.3):
        m.torno([(r, -0.03), (r + 0.005, -0.026), (r + 0.005, 0.026), (r, 0.03)], en=(0, y, 0), mat='acero', pieza=c, marco=c, lados=40)
        m.caja((0.05, 0.04, 0.016), en=(0, y, -r - 0.008), mat='acero_oscuro', pieza=c, marco=c, bisel=0.003)
    # the winding shows as a band of carbon at each shoulder
    for y in (-1, 1):
        m.torno([(r + 0.001, -0.05), (r + 0.002, 0.0), (r + 0.001, 0.05)], en=(0, y * (h / 2 - r * 0.62 - 0.02), 0), mat='carbono', pieza=c, marco=c, lados=40)
    # its neck, at the end toward +y: a boss, the valve block with its wheel, a gauge, the relief
    m.torno([(0.06, 0), (0.06, 0.03), (0.04, 0.034), (0.04, 0.05), (0, 0.05)], en=(0, h / 2 - 0.012, 0), mat='acero', pieza=c, marco=c, lados=18)
    m.caja((0.07, 0.07, 0.07), en=(0, h / 2 + 0.07, 0), mat='laton', pieza=c, marco=c, bisel=0.008)
    m.cilindro(0.012, 0.05, en=(0, h / 2 + 0.07, -0.058), mat='acero', pieza=c, marco=c, eje='z', lados=8, bisel=0.002)
    m.torno([(0.02, -0.005), (0.045, 0), (0.02, 0.005), (0.03, 0)], en=(0, h / 2 + 0.07, -0.088), mat='pintura_azul', pieza=c, marco=c, eje='z', lados=20)
    m.cilindro(0.03, 0.016, en=(-0.05, h / 2 + 0.07, 0), mat='acero', pieza=c, marco=c, eje='x', lados=18, bisel=0.003)
    m.cilindro(0.025, 0.004, en=(-0.06, h / 2 + 0.07, 0), mat='lente', pieza=c, marco=c, eje='x', lados=18, bisel=0.001)
    m.cilindro(0.011, 0.04, en=(0.05, h / 2 + 0.07, 0), mat='pintura_roja', pieza=c, marco=c, eje='x', lados=8, bisel=0.002)
    m.tubo([(0, h / 2 + 0.105, 0), (0, h / 2 + 0.13, 0), (0, h / 2 + 0.13, 0.2), (0, h / 2 + 0.05, r + 0.04)], 0.009, 'acero', c, c, lados=8, codo=0.03)
    # ---- the cradle: two rails, and on them a saddle under each strap
    sx, sy, sz = q.tam
    for x in (-1, 1):
        m.caja((0.05, sy * 0.6, sz), en=(x * (sx / 2 - 0.025), -sy * 0.2, 0), mat=TINTE, pieza=q, marco=q, bisel=0.006)
    for z in (-h * 0.3, h * 0.3):
        # a saddle: a cross plate cut to the vessel's round
        perfil = [(-sx / 2, -sy / 2), (sx / 2, -sy / 2), (sx / 2, sy / 2 + 0.05)]
        for k in range(9):
            a = math.radians(-48 + 96 * k / 8)
            perfil.append((r * 1.01 * math.sin(-a), (c.c.y - q.c.y) - r * 1.01 * math.cos(a)))
        perfil.append((-sx / 2, sy / 2 + 0.05))
        m.extrusion(perfil, 0.05, en=(0, 0, z), mat=TINTE, pieza=q, marco=q, eje='z', bisel=0.004)
        m.tornillos([(x * (sx / 2 - 0.025), sy * 0.1, z + dz) for x in (-1, 1) for dz in (-0.05, 0.05)], 0.008, 0.005, 'acero', q, q)
