"""The machines of a ship: what moves its air, its power, its coolant and its oil."""
import math

from kit import TINTE, hundir, receta


@receta('rejilla')
def rejilla(m):
    """A vent: a frame with its louvres and the dark of the duct behind them."""
    p = m['']
    sx, sy, sz = p.tam
    for z in (-1, 1):
        m.caja((sx, sy, 0.03), en=(0, 0, z * (sz / 2 - 0.015)), mat=TINTE, pieza=p, marco=p, bisel=0.006, seg=2)
    for x in (-1, 1):
        m.caja((0.03, sy, sz - 0.06), en=(x * (sx / 2 - 0.015), 0, 0), mat=TINTE, pieza=p, marco=p, bisel=0.006, seg=2)
    m.rejilla((sx - 0.06, sz - 0.06), pieza=p, marco=p, lamas=7, normal='y', mat=TINTE, grosor=0.003)
    m.caja((sx - 0.05, 0.003, sz - 0.05), en=(0, -sy / 2 + 0.003, 0), mat='pintura_oscura', pieza=p, marco=p, bisel=0.0)
    m.tornillos([(x * (sx / 2 - 0.015), sy / 2, z * (sz / 2 - 0.015)) for x in (-1, 1) for z in (-1, 1)], 0.005, 0.003, 'acero', p, p)


@receta('ventilador')
def ventilador(m):
    """A duct fan: a shroud with a flange each end, seven blades on a hub with its spinner, a
    guard of struts, and its motor in a finned box with a junction box on it."""
    c, mo = m['carcasa'], m['motor']
    r, h = c.r, c.h
    m.torno([(r - 0.03, -h / 2), (r, -h / 2), (r, -h / 2 + 0.025), (r - 0.012, -h / 2 + 0.035), (r - 0.012, h / 2 - 0.035), (r, h / 2 - 0.025), (r, h / 2), (r - 0.03, h / 2), (r - 0.03, -h / 2)], mat=TINTE, pieza=c, marco=c, lados=36)
    m.cilindro(0.055, 0.13, mat='acero_oscuro', pieza=c, marco=c, lados=18, bisel=0.008)
    m.esfera(0.055, en=(0, 0.065, 0), mat='acero_oscuro', pieza=c, marco=c, lados=18, de=0, aplastar=0.8)
    for k in range(7):
        marco = c.girado((0, 360 * k / 7, 0))
        m.caja((0.11, 0.005, 0.075), en=(0.108, 0, 0), mat='aluminio', pieza=c, marco=marco, bisel=0.002, seg=1, rot=(32, 0, 0))
    for k in range(3):
        a = math.tau * k / 3
        m.tubo([(0.05 * math.cos(a), -h / 2 + 0.03, 0.05 * math.sin(a)), ((r - 0.03) * math.cos(a), -h / 2 + 0.03, (r - 0.03) * math.sin(a))], 0.007, 'acero', c, c, lados=8)
    for y in (-h / 2 + 0.012, h / 2 - 0.012):
        m.tornillos([((r - 0.006) * math.cos(math.tau * k / 8), y, (r - 0.006) * math.sin(math.tau * k / 8)) for k in range(8)], 0.006, 0.004, 'acero_oscuro', c, c, eje=(0, 1 if y > 0 else -1, 0))
    sx, sy, sz = mo.tam
    m.caja((sx, sy, sz), mat=TINTE, pieza=mo, marco=mo, bisel=0.015, seg=3)
    m.aletas((sx * 0.9, 0.03, sz * 0.8), en=(0, sy / 2 + 0.012, 0), n=9, grosor=0.004, mat='acero_oscuro', pieza=mo, marco=mo, apila='x')
    m.caja((0.08, 0.06, 0.03), en=(0, 0, -sz / 2 - 0.012), mat='plastico_negro', pieza=mo, marco=mo, bisel=0.006)


@receta('depurador')
def depurador(m):
    """A CO2 scrubber: a cabinet with two canister doors on its front, their handles, a lamp
    and a gauge between them, hose ports on top, feet."""
    p = m['']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.018, seg=3)
    hundir(ob, (0, 1, 0), 0.04, 0.006, marco=p)
    for cara in (-1, 1):
        for x in (-0.15, 0.15):
            m.torno([(0.105, 0), (0.105, 0.012), (0.09, 0.022), (0.03, 0.022), (0.03, 0.03), (0, 0.03)], en=(x, 0.01, cara * sz / 2), mat='aluminio', pieza=p, marco=p, eje=(0, 0, cara), lados=28)
            m.asa((x - 0.045, 0.01, cara * (sz / 2 + 0.022)), (x + 0.045, 0.01, cara * (sz / 2 + 0.022)), (0, 0, cara), alto=0.028, r=0.006, mat='acero', pieza=p, marco=p)
        m.piloto((0, 0.14, cara * sz / 2), 0.008, 'piloto_verde', p, p, eje=(0, 0, cara))
        m.cilindro(0.028, 0.012, en=(0, -0.12, cara * (sz / 2 + 0.004)), mat='acero', pieza=p, marco=p, eje='z', lados=18, bisel=0.003)
        m.cilindro(0.023, 0.004, en=(0, -0.12, cara * (sz / 2 + 0.011)), mat='lente', pieza=p, marco=p, eje='z', lados=18, bisel=0.001)
    for x in (-0.2, 0.2):
        m.torno([(0.035, 0), (0.035, 0.03), (0.042, 0.034), (0.042, 0.044), (0.03, 0.044), (0.03, 0)], en=(x, sy / 2 - 0.006, 0), mat='acero', pieza=p, marco=p, lados=18)
    for x in (-1, 1):
        for z in (-1, 1):
            m.cilindro(0.025, 0.02, en=(x * (sx / 2 - 0.06), -sy / 2 - 0.006, z * (sz / 2 - 0.06)), mat='goma', pieza=p, marco=p, lados=12, bisel=0.004)


@receta('inyector')
def inyector(m):
    """A gas injector: a valve block with a solenoid on top and a port each side."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx * 0.8, sy * 0.55, sz * 0.9), en=(0, -sy * 0.2, 0), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=3)
    m.cilindro(sz * 0.3, sy * 0.5, en=(0, sy * 0.22, 0), mat='acero_oscuro', pieza=p, marco=p, lados=20, bisel=0.008)
    m.cilindro(sz * 0.12, 0.02, en=(0, sy * 0.48, 0), mat='laton', pieza=p, marco=p, lados=6, bisel=0.003)
    for x in (-1, 1):
        m.torno([(0.03, 0), (0.03, 0.012), (0.022, 0.016), (0.022, 0.03), (0.016, 0.03), (0.016, 0)], en=(x * sx * 0.4, -sy * 0.2, 0), mat='laton', pieza=p, marco=p, eje=(x, 0, 0), lados=6)
    m.tornillos([(x * sx * 0.3, sy * 0.075, z * sz * 0.34) for x in (-1, 1) for z in (-1, 1)], 0.006, 0.004, 'acero', p, p)


@receta('botella')
def botella(m):
    """A gas bottle: a round shoulder and a foot ring, a neck ring, its valve with a handwheel
    and a guard."""
    c, v = m['cuerpo'], m['valvula']
    r, h = c.r, c.h
    perfil = [(0, -h / 2 + 0.02), (r * 0.6, -h / 2 + 0.012), (r * 0.86, -h / 2 + 0.03), (r * 0.9, -h / 2), (r, -h / 2), (r, -h / 2 + 0.05), (r, h / 2 - r * 1.0)]
    for k in range(1, 7):
        a = math.radians(90 * k / 6)
        perfil.append((0.045 + (r - 0.045) * math.cos(a), h / 2 - r * 1.0 + (r * 0.85) * math.sin(a)))
    perfil += [(0.045, h / 2 - 0.02), (0.052, h / 2 - 0.02), (0.052, h / 2), (0, h / 2)]
    m.torno(perfil, mat=TINTE, pieza=c, marco=c, lados=32, liso=50)
    # a band and a label plate round its middle
    m.torno([(r, -0.03), (r + 0.004, -0.026), (r + 0.004, 0.026), (r, 0.03)], en=(0, h * 0.18, 0), mat='acero', pieza=c, marco=c, lados=32)
    vr, vh = v.r, v.h
    m.cilindro(vr * 0.7, vh * 0.6, en=(0, -vh * 0.2, 0), mat=TINTE, pieza=v, marco=v, lados=8, bisel=0.004)
    m.cilindro(vr * 0.4, vh * 0.5, en=(0, vh * 0.2, 0), mat=TINTE, pieza=v, marco=v, lados=10, bisel=0.003)
    m.torno([(vr * 0.9, -0.005), (vr * 1.15, 0), (vr * 0.9, 0.005), (vr * 0.6, 0)], en=(0, vh * 0.42, 0), mat='pintura_roja', pieza=v, marco=v, lados=20)
    m.cilindro(vr * 0.3, vr * 1.2, en=(vr * 0.9, -vh * 0.1, 0), mat=TINTE, pieza=v, marco=v, eje='x', lados=8, bisel=0.002)


@receta('bateria')
def bateria(m):
    """A battery bank: a ribbed case of cells, a lid with a vent cap per cell, two terminals
    under boots with the bar between banks, a carrying strap each end."""
    p = m['']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy * 0.86, sz), en=(0, -sy * 0.07, 0), mat=TINTE, pieza=p, marco=p, bisel=0.014, seg=3)
    for lado in (-1, 1):
        hundir(ob, (0, 0, lado), 0.035, 0.008, marco=p)
        for k in range(6):
            x = -sx * 0.36 + sx * 0.72 * k / 5
            m.caja((0.02, sy * 0.6, 0.012), en=(x, -sy * 0.07, lado * (sz / 2 - 0.004)), mat=TINTE, pieza=p, marco=p, bisel=0.004)
    m.caja((sx + 0.012, sy * 0.1, sz + 0.012), en=(0, sy * 0.4, 0), mat='plastico_negro', pieza=p, marco=p, bisel=0.008, seg=2)
    for k in range(6):
        x = -sx * 0.36 + sx * 0.72 * k / 5
        m.cilindro(0.022, 0.014, en=(x, sy * 0.46, -sz * 0.18), mat='plastico_gris', pieza=p, marco=p, lados=12, bisel=0.003)
    for x, mat in ((-sx * 0.3, 'pintura_roja'), (sx * 0.3, 'plastico_negro')):
        m.cilindro(0.03, 0.04, en=(x, sy * 0.47, sz * 0.2), mat=mat, pieza=p, marco=p, lados=14, bisel=0.008)
        m.cilindro(0.012, 0.02, en=(x, sy * 0.5, sz * 0.2), mat='laton', pieza=p, marco=p, lados=6, bisel=0.002)
    m.caja((sx * 0.5, 0.008, 0.03), en=(0, sy * 0.455, sz * 0.2), mat='cobre', pieza=p, marco=p, bisel=0.002)
    for x in (-1, 1):
        m.asa((x * sx / 2, sy * 0.25, -0.09), (x * sx / 2, sy * 0.25, 0.09), (x, 0.3, 0), alto=0.04, r=0.007, mat='goma', pieza=p, marco=p)


@receta('convertidor_cc')
def convertidor_cc(m):
    """A DC-DC converter: a block between two banks of cooling fins, its connectors and a lamp
    on its face, lugs to bolt it down."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx * 0.62, sy * 0.92, sz), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=3)
    for x in (-1, 1):
        m.aletas((sx * 0.17, sy * 0.8, sz * 0.9), en=(x * sx * 0.405, 0, 0), n=9, grosor=0.005, mat='aluminio', pieza=p, marco=p, apila='z')
    for cara in (-1, 1):
        for y in (-0.1, 0.02):
            m.torno([(0.022, 0), (0.022, 0.02), (0.016, 0.02), (0.016, 0.012), (0, 0.012)], en=(0, y, cara * sz / 2), mat='acero', pieza=p, marco=p, eje=(0, 0, cara), lados=12)
        m.piloto((0, 0.16, cara * sz / 2), 0.006, 'piloto_verde', p, p, eje=(0, 0, cara))
    for x in (-1, 1):
        m.caja((0.05, 0.012, 0.06), en=(x * sx * 0.25, -sy / 2 + 0.006, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.003)


@receta('apu')
def apu(m):
    """An auxiliary power unit: a turbine in its casing on a skid, the gearbox and generator at
    one end, an intake screen on top, lifting eyes, and its exhaust."""
    c, e = m['cuerpo'], m['escape']
    sx, sy, sz = c.tam
    for x in (-1, 1):
        m.caja((0.07, 0.07, sz), en=(x * (sx / 2 - 0.06), -sy / 2 + 0.035, 0), mat='acero_oscuro', pieza=c, marco=c, bisel=0.008)
    m.caja((sx, sy * 0.5, sz * 0.94), en=(0, -sy * 0.14, 0), mat=TINTE, pieza=c, marco=c, bisel=0.02, seg=3)
    m.capsula(sx * 0.36, sz * 0.86, en=(0, sy * 0.13, 0), mat=TINTE, pieza=c, marco=c, eje='z', lados=28, fondo=0.35)
    for z in (-sz * 0.22, 0.0, sz * 0.22):
        m.torno([(sx * 0.36, -0.02), (sx * 0.375, -0.014), (sx * 0.375, 0.014), (sx * 0.36, 0.02)], en=(0, sy * 0.13, z), mat='acero', pieza=c, marco=c, eje='z', lados=28)
    m.caja((sx * 0.5, 0.05, sz * 0.26), en=(0, sy / 2 - 0.035, sz * 0.2), mat='acero_oscuro', pieza=c, marco=c, bisel=0.01)
    m.rejilla((sx * 0.44, sz * 0.2), en=(0, sy / 2 - 0.006, sz * 0.2), pieza=c, marco=c, lamas=6, normal='y')
    m.caja((sx * 0.5, sy * 0.4, 0.16), en=(0, sy * 0.05, sz / 2 - 0.09), mat='pintura_oscura', pieza=c, marco=c, bisel=0.014)
    for x in (-1, 1):
        m.torno([(0.008, -0.004), (0.02, 0), (0.008, 0.004), (0.014, 0)], en=(x * sx * 0.28, sy / 2 - 0.01, -sz * 0.25), mat='acero', pieza=c, marco=c, eje='x', lados=12)
    m.pernos(c, 'x', 0.08, (3, 2), 0.007, lado=1)
    m.pernos(c, 'x', 0.08, (3, 2), 0.007, lado=-1)
    r, h = e.r, e.h
    m.torno([(r * 0.8, -h / 2), (r, -h / 2), (r, h / 2 - 0.02), (r + 0.012, h / 2 - 0.02), (r + 0.012, h / 2), (r * 0.82, h / 2), (r * 0.8, -h / 2)], mat=TINTE, pieza=e, marco=e, lados=24)
    m.brida(r + 0.03, en=(0, -h / 2 + 0.01, 0), grosor=0.014, pernos=6, mat='acero', pieza=e, marco=e, r_int=r)


@receta('reactor_compacto')
def reactor_compacto(m):
    """A compact reactor: a ribbed vessel between a base and a shield plate, four coolant
    nozzles with their flanges, the rod drives through the shield, a ring of studs each end."""
    n, sh, base = m['nucleo'], m['blindaje'], m['base']
    r, h = n.r, n.h
    perfil = [(r * 0.9, -h / 2), (r, -h / 2 + 0.05)]
    for k in range(5):
        y = -h / 2 + 0.2 + (h - 0.4) * k / 4
        perfil += [(r, y - 0.05), (r + 0.02, y - 0.035), (r + 0.02, y + 0.035), (r, y + 0.05)]
    perfil += [(r, h / 2 - 0.05), (r * 0.9, h / 2)]
    m.torno(perfil, mat=TINTE, pieza=n, marco=n, lados=40)
    for k in range(4):
        a = math.tau * (k + 0.5) / 4
        d = (math.cos(a), 0, math.sin(a))
        y = h * 0.22 if k % 2 == 0 else -h * 0.22
        m.torno([(0.085, 0), (0.085, 0.1), (0.07, 0.1), (0.07, 0)], en=(d[0] * (r - 0.01), y, d[2] * (r - 0.01)), mat=TINTE, pieza=n, marco=n, eje=d, lados=18)
        m.brida(0.115, en=(d[0] * (r + 0.09), y, d[2] * (r + 0.09)), grosor=0.02, pernos=8, mat='acero', pieza=n, marco=n, eje=d, r_int=0.07)
    # an instrument conduit up its side
    m.tubo([(r + 0.03, -h / 2 + 0.1, 0.0), (r + 0.03, h / 2 - 0.1, 0.0)], 0.014, 'acero', n, n, lados=8)
    for y in (-h * 0.3, 0.0, h * 0.3):
        m.caja((0.03, 0.03, 0.05), en=(r + 0.015, y, 0), mat='acero_oscuro', pieza=n, marco=n, bisel=0.004)
    for p, lado in ((sh, 1), (base, -1)):
        sx, sy, sz = p.tam
        # an eight-sided plate, its studs in a ring
        ocho = [(sx / 2 * math.cos(math.tau * (k + 0.5) / 8) / math.cos(math.pi / 8), sz / 2 * math.sin(math.tau * (k + 0.5) / 8) / math.cos(math.pi / 8)) for k in range(8)]
        m.extrusion(ocho, sy, mat=TINTE, pieza=p, marco=p, eje='y', bisel=0.012, seg=2)
        m.tornillos([(r * 1.08 * math.cos(math.tau * k / 16), lado * sy / 2, r * 1.08 * math.sin(math.tau * k / 16)) for k in range(16)], 0.016, 0.012, 'acero', p, p, eje=(0, lado, 0))
    sy = sh.tam.y
    for k in range(5):
        a = math.tau * k / 5
        x, z = (0.24 * math.cos(a), 0.24 * math.sin(a)) if k else (0.0, 0.0)
        m.cilindro(0.05, 0.09, en=(x, sy / 2 + 0.04, z), mat='acero_oscuro', pieza=sh, marco=sh, lados=14, bisel=0.008)
        m.cilindro(0.022, 0.05, en=(x, sy / 2 + 0.1, z), mat='acero', pieza=sh, marco=sh, lados=10, bisel=0.004)


@receta('conversor_termico')
def conversor_termico(m):
    """A thermoelectric converter: a hot core between banks of fins, the hot loop in and out at
    one end, the bus terminals at the other."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx * 0.5, sy * 0.94, sz * 0.9), mat=TINTE, pieza=p, marco=p, bisel=0.016, seg=3)
    for x in (-1, 1):
        m.aletas((sx * 0.22, sy * 0.84, sz * 0.84), en=(x * sx * 0.37, 0, 0), n=12, grosor=0.006, mat='aluminio', pieza=p, marco=p, apila='y')
    for y in (-sy * 0.25, sy * 0.25):
        m.torno([(0.05, 0), (0.05, 0.06), (0.04, 0.06), (0.04, 0)], en=(0, y, sz * 0.45), mat='acero', pieza=p, marco=p, eje='z', lados=16)
        m.brida(0.07, en=(0, y, sz * 0.45 + 0.05), grosor=0.014, pernos=6, mat='acero', pieza=p, marco=p, eje='z', r_int=0.04)
    for y, mat in ((-0.1, 'pintura_roja'), (0.1, 'plastico_negro')):
        m.cilindro(0.03, 0.03, en=(0, y, -sz * 0.45 - 0.01), mat=mat, pieza=p, marco=p, eje='z', lados=14, bisel=0.006)
    m.caja((sx * 0.56, 0.03, sz * 0.96), en=(0, -sy / 2 + 0.015, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.006)


@receta('bomba_refrigerante')
def bomba_refrigerante(m):
    """A coolant pump: a volute with its inlet on the axis and its outlet at a tangent, bolted
    to a finned motor with its terminal box."""
    c, mo = m['carcasa'], m['motor']
    r, h = c.r, c.h
    m.torno([(0, -h / 2), (r * 0.8, -h / 2), (r, -h / 2 + 0.04), (r, h / 2 - 0.08), (r * 0.7, h / 2 - 0.03), (r * 0.42, h / 2 - 0.03), (r * 0.42, h / 2), (r * 0.3, h / 2), (r * 0.3, h / 2 - 0.05), (0, h / 2 - 0.05)], mat=TINTE, pieza=c, marco=c, lados=32)
    m.brida(r * 0.62, en=(0, h / 2 - 0.006, 0), grosor=0.014, pernos=6, mat='acero', pieza=c, marco=c, r_int=r * 0.3)
    m.torno([(r * 0.36, 0), (r * 0.36, r * 1.1), (r * 0.28, r * 1.1), (r * 0.28, 0)], en=(r * 0.62, -h * 0.12, 0), mat=TINTE, pieza=c, marco=c, eje='z', lados=16)
    m.brida(r * 0.52, en=(r * 0.62, -h * 0.12, r * 1.08), grosor=0.014, pernos=6, mat='acero', pieza=c, marco=c, eje='z', r_int=r * 0.28)
    m.tornillos([(r * 0.92 * math.cos(math.tau * k / 10), -h / 2 + 0.04, r * 0.92 * math.sin(math.tau * k / 10)) for k in range(10)], 0.008, 0.006, 'acero', c, c)
    sx, sy, sz = mo.tam
    m.cilindro(sx * 0.46, sz * 0.94, mat=TINTE, pieza=mo, marco=mo, eje='z', lados=24, bisel=0.012)
    for k in range(16):
        marco = mo.girado((0, 0, 360 * k / 16))
        m.caja((0.004, 0.02, sz * 0.7), en=(0, sx * 0.47, 0), mat=TINTE, pieza=mo, marco=marco, bisel=0.001, seg=1)
    m.caja((0.09, 0.05, 0.1), en=(0, sy / 2 + 0.01, -0.03), mat='plastico_negro', pieza=mo, marco=mo, bisel=0.008)
    for x in (-1, 1):
        m.caja((0.05, 0.02, 0.06), en=(x * sx * 0.36, -sy / 2 + 0.01, 0), mat='acero_oscuro', pieza=mo, marco=mo, bisel=0.004)


@receta('deposito_refrigerante')
def deposito_refrigerante(m):
    """An expansion vessel: a capsule on a skirt, a sight glass up its side, a filler cap."""
    p = m['']
    r, h = p.r, p.h
    m.capsula(r, h * 0.86, en=(0, h * 0.07, 0), mat=TINTE, pieza=p, marco=p, lados=32, fondo=0.5)
    m.torno([(r * 0.86, -h / 2), (r * 0.9, -h / 2), (r * 0.9, -h / 2 + h * 0.2), (r * 0.86, -h / 2 + h * 0.2)], mat='acero_oscuro', pieza=p, marco=p, lados=32)
    for y in (-h * 0.12, h * 0.26):
        m.torno([(r, -0.012), (r + 0.006, -0.008), (r + 0.006, 0.008), (r, 0.012)], en=(0, y, 0), mat='acero', pieza=p, marco=p, lados=32)
    m.tubo([(0, -h * 0.1, r + 0.022), (0, h * 0.24, r + 0.022)], 0.01, 'lente', p, p, lados=10)
    for y in (-h * 0.1, h * 0.24):
        m.caja((0.03, 0.026, 0.04), en=(0, y, r + 0.012), mat='laton', pieza=p, marco=p, bisel=0.004)
    m.cilindro(0.045, 0.03, en=(0, h / 2 - 0.012, 0), mat='pintura_amarilla', pieza=p, marco=p, lados=10, bisel=0.006)


@receta('ordenador')
def ordenador(m):
    """A flight computer: an avionics box with a finned top, a row of round connectors and two
    hold-downs on its face, a handle to draw it by."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx, sy * 0.82, sz), en=(0, -sy * 0.09, 0), mat=TINTE, pieza=p, marco=p, bisel=0.012, seg=3)
    m.aletas((sx * 0.9, sy * 0.16, sz * 0.9), en=(0, sy * 0.41, 0), n=14, grosor=0.006, mat=TINTE, pieza=p, marco=p, apila='x')
    for cara in (-1, 1):
        for k in range(4):
            x = -sx * 0.3 + sx * 0.2 * k
            m.torno([(0.026, 0), (0.026, 0.018), (0.02, 0.022), (0.02, 0.03), (0.014, 0.03), (0.014, 0.01), (0, 0.01)], en=(x, -sy * 0.02, cara * sz / 2), mat='acero', pieza=p, marco=p, eje=(0, 0, cara), lados=14)
        m.asa((sx * 0.18, -sy * 0.3, cara * sz / 2), (sx * 0.42, -sy * 0.3, cara * sz / 2), (0, 0, cara), alto=0.03, r=0.006, mat='acero', pieza=p, marco=p)
        m.piloto((-sx * 0.38, -sy * 0.3, cara * sz / 2), 0.006, 'piloto_verde', p, p, eje=(0, 0, cara))
        m.piloto((-sx * 0.3, -sy * 0.3, cara * sz / 2), 0.006, 'piloto_ambar', p, p, eje=(0, 0, cara))
    m.caja((sx + 0.02, 0.02, sz * 0.5), en=(0, -sy / 2 + 0.01, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.004)


@receta('rack')
def rack(m):
    """An avionics rack: four posts and their shelves, five units drawn into it, each with its
    handle, its lamps and its connectors."""
    p = m['']
    sx, sy, sz = p.tam
    for x in (-1, 1):
        for z in (-1, 1):
            m.caja((0.035, sy, 0.035), en=(x * (sx / 2 - 0.0175), 0, z * (sz / 2 - 0.0175)), mat=TINTE, pieza=p, marco=p, bisel=0.005)
    for y in (-sy / 2 + 0.015, sy / 2 - 0.015):
        m.caja((sx, 0.03, sz), en=(0, y, 0), mat=TINTE, pieza=p, marco=p, bisel=0.006)
    n = 5
    alto = (sy - 0.08) / n
    for k in range(n):
        y = -sy / 2 + 0.04 + alto * (k + 0.5)
        m.caja((sx - 0.08, alto - 0.012, sz - 0.03), en=(0, y, 0), mat='pintura_oscura', pieza=p, marco=p, bisel=0.006, seg=2)
        for cara in (-1, 1):
            z = cara * (sz / 2 - 0.015)
            m.asa((-sx * 0.3, y - alto * 0.2, z), (-sx * 0.3, y + alto * 0.2, z), (0, 0, cara), alto=0.022, r=0.005, mat='acero', pieza=p, marco=p)
            m.piloto((sx * 0.28, y + alto * 0.2, z), 0.005, 'piloto_verde' if k != 2 else 'piloto_ambar', p, p, eje=(0, 0, cara))
            m.cilindro(0.016, 0.014, en=(sx * 0.1, y, z + cara * 0.006), mat='acero', pieza=p, marco=p, eje='z', lados=12, bisel=0.003)
            m.cilindro(0.016, 0.014, en=(-sx * 0.05, y, z + cara * 0.006), mat='acero', pieza=p, marco=p, eje='z', lados=12, bisel=0.003)


@receta('unidad_hidraulica')
def unidad_hidraulica(m):
    """A hydraulic power pack: a base tank carrying the motor and pump, a manifold with its
    valves and a filter can, a gauge; the reservoir a capsule on top with its sight glass."""
    c, d = m['cuerpo'], m['deposito']
    sx, sy, sz = c.tam
    m.caja((sx, sy * 0.5, sz), en=(0, -sy * 0.25, 0), mat=TINTE, pieza=c, marco=c, bisel=0.016, seg=3)
    m.pernos(c, 'z', 0.05, (4, 1), 0.007, lado=1)
    m.pernos(c, 'z', 0.05, (4, 1), 0.007, lado=-1)
    # the motor lying along it, the pump on its end
    m.cilindro(sy * 0.21, sx * 0.48, en=(-sx * 0.2, sy * 0.21, -sz * 0.12), mat='pintura_oscura', pieza=c, marco=c, eje='x', lados=24, bisel=0.012)
    for k in range(12):
        marco = c.en((-sx * 0.2, sy * 0.21, -sz * 0.12)).girado((360 * k / 12, 0, 0))
        m.caja((sx * 0.36, 0.016, 0.004), en=(0, sy * 0.215, 0), mat='pintura_oscura', pieza=c, marco=marco, bisel=0.001, seg=1)
    m.cilindro(sy * 0.15, sx * 0.14, en=(sx * 0.11, sy * 0.21, -sz * 0.12), mat='acero', pieza=c, marco=c, eje='x', lados=20, bisel=0.008)
    # the manifold: a block with three valves, a filter can, a gauge
    m.caja((sx * 0.42, sy * 0.16, sz * 0.2), en=(-sx * 0.18, sy * 0.08, sz * 0.3), mat='acero', pieza=c, marco=c, bisel=0.008)
    for k in range(3):
        m.cilindro(0.022, 0.07, en=(-sx * 0.32 + sx * 0.14 * k, sy * 0.2, sz * 0.3), mat='acero_oscuro', pieza=c, marco=c, lados=12, bisel=0.004)
        m.cilindro(0.012, 0.02, en=(-sx * 0.32 + sx * 0.14 * k, sy * 0.28, sz * 0.3), mat='pintura_roja', pieza=c, marco=c, lados=8, bisel=0.003)
    m.cilindro(0.045, sy * 0.36, en=(sx * 0.36, sy * 0.18, sz * 0.26), mat='pintura_blanca', pieza=c, marco=c, lados=18, bisel=0.01)
    m.cilindro(0.03, 0.014, en=(sx * 0.12, sy * 0.1, sz / 2 + 0.004), mat='acero', pieza=c, marco=c, eje='z', lados=18, bisel=0.003)
    m.cilindro(0.025, 0.004, en=(sx * 0.12, sy * 0.1, sz / 2 + 0.012), mat='lente', pieza=c, marco=c, eje='z', lados=18, bisel=0.001)
    m.manguera((sx * 0.18, sy * 0.21, -sz * 0.12), (sx * 0.36, sy * 0.3, sz * 0.2), 0.012, comba=0.05, hacia=(0, 1, 0), pieza=c, marco=c)
    r, h = d.r, d.h
    m.capsula(r, h, mat=TINTE, pieza=d, marco=d, lados=28, fondo=0.45)
    m.cilindro(0.04, 0.03, en=(0, h / 2 - 0.004, 0), mat='pintura_amarilla', pieza=d, marco=d, lados=10, bisel=0.006)
    m.tubo([(0, -h * 0.22, r + 0.016), (0, h * 0.22, r + 0.016)], 0.008, 'lente', d, d, lados=8)
    for y in (-h * 0.22, h * 0.22):
        m.caja((0.024, 0.02, 0.03), en=(0, y, r + 0.008), mat='laton', pieza=d, marco=d, bisel=0.003)
    for y in (-h * 0.3, h * 0.3):
        m.torno([(r, -0.012), (r + 0.006, -0.008), (r + 0.006, 0.008), (r, 0.012)], en=(0, y, 0), mat='acero', pieza=d, marco=d, lados=28)


@receta('acumulador')
def acumulador(m):
    """A hydraulic accumulator: a tube between two heads held by tie rods, the gas valve on top."""
    p = m['']
    r, h = p.r, p.h
    m.cilindro(r * 0.86, h * 0.84, mat=TINTE, pieza=p, marco=p, lados=28, bisel=0.004)
    for y in (-1, 1):
        m.cilindro(r, h * 0.09, en=(0, y * (h / 2 - h * 0.045), 0), mat='acero', pieza=p, marco=p, lados=8, bisel=0.008)
    for k in range(4):
        a = math.tau * (k + 0.5) / 4
        m.tubo([(r * 0.92 * math.cos(a), -h / 2 + 0.01, r * 0.92 * math.sin(a)), (r * 0.92 * math.cos(a), h / 2 - 0.01, r * 0.92 * math.sin(a))], 0.007, 'acero', p, p, lados=6)
    m.cilindro(0.016, 0.04, en=(0, h / 2 + 0.012, 0), mat='laton', pieza=p, marco=p, lados=6, bisel=0.003)
    m.torno([(r * 0.86, -0.03), (r * 0.88, -0.03), (r * 0.88, 0.03), (r * 0.86, 0.03)], en=(0, 0, 0), mat='pintura_amarilla', pieza=p, marco=p, lados=28)


@receta('giroscopo_cmg')
def giroscopo_cmg(m):
    """A control moment gyro: its drive in a housing, and on it the rotor's can hung in a
    gimbal ring between two bearings."""
    p, v = m[''], m['volante']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy * 0.7, sz), en=(0, -sy * 0.15, 0), mat=TINTE, pieza=p, marco=p, bisel=0.02, seg=3)
    for lado in ((1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)):
        hundir(ob, lado, 0.05, 0.008, marco=p)
    m.aletas((sx * 0.7, sy * 0.1, sz * 0.7), en=(0, sy * 0.24, 0), n=10, grosor=0.006, mat='acero_oscuro', pieza=p, marco=p, apila='x')
    for x in (-1, 1):
        m.caja((0.05, sy * 0.36, 0.1), en=(x * sx * 0.42, sy * 0.3, 0), mat='acero', pieza=p, marco=p, bisel=0.01)
    r, h = v.r, v.h
    m.torno([(0, -h / 2), (r * 0.7, -h / 2), (r, -h * 0.2), (r, h * 0.2), (r * 0.7, h / 2), (0, h / 2)], mat=TINTE, pieza=v, marco=v, lados=36)
    m.torno([(r + 0.012, -0.012), (r + 0.03, -0.012), (r + 0.03, 0.012), (r + 0.012, 0.012), (r + 0.012, -0.012)], mat='acero_oscuro', pieza=v, marco=v, lados=36)
    for x in (-1, 1):
        m.cilindro(0.03, 0.05, en=(x * (r + 0.04), 0, 0), mat='acero', pieza=v, marco=v, eje='x', lados=14, bisel=0.006)
    m.cilindro(r * 0.3, h + 0.02, mat='acero', pieza=v, marco=v, lados=18, bisel=0.006)


@receta('bloque_rcs')
def bloque_rcs(m):
    """A thruster block: a housing with its corners cut, a seat ring on each face (a nozzle
    stands in some, the rest are blanked), foil over its corners, its feed in at one side."""
    p = m['']
    s = p.tam.x
    m.caja((s * 0.92, s * 0.92, s * 0.92), mat=TINTE, pieza=p, marco=p, bisel=0.05, seg=1)
    for i in range(3):
        u, w = [k for k in range(3) if k != i]
        for lado in (1, -1):
            d = [0.0, 0.0, 0.0]
            d[i] = lado
            c = [x * (s / 2 - 0.012) for x in d]
            m.torno([(0.035, -0.006), (0.085, -0.006), (0.09, 0.0), (0.09, 0.008), (0.078, 0.012), (0.05, 0.012), (0.035, 0.004), (0.035, -0.006)], en=c, mat='acero_oscuro', pieza=p, marco=p, eje=d, lados=22)
            pts = []
            for k in range(8):
                q = [x + 0.012 * y for x, y in zip(c, d)]
                q[u] += 0.07 * math.cos(math.tau * k / 8)
                q[w] += 0.07 * math.sin(math.tau * k / 8)
                pts.append(q)
            m.tornillos(pts, 0.006, 0.004, 'acero', p, p, eje=d)
    # foil taped over its cut corners
    for x in (-1, 1):
        for y in (-1, 1):
            for z in (-1, 1):
                m.esfera(0.05, en=(x * s * 0.33, y * s * 0.33, z * s * 0.33), mat='aislante', pieza=p, marco=p, lados=8)
    m.cilindro(0.012, 0.03, en=(-s * 0.2, -s * 0.2, -s / 2 + 0.03), mat='laton', pieza=p, marco=p, eje='z', lados=8, bisel=0.002)


@receta('tobera_rcs')
def tobera_rcs(m):
    """A thruster: a small bell on its chamber, hollow."""
    p = m['']
    r0, r1, h = p.r, p.r * p.taper, p.h
    perfil = [(0, -h / 2), (r0 * 0.9, -h / 2), (r0, -h / 2 + h * 0.25), (r0 * 0.5, -h / 2 + h * 0.4)]
    for k in range(1, 7):
        t = k / 6
        perfil.append((r0 * 0.5 + (r1 - r0 * 0.5) * (t ** 0.6), -h / 2 + h * 0.4 + h * 0.6 * t))
    for k in range(6, 0, -1):
        t = k / 6
        perfil.append((r0 * 0.5 + (r1 - 0.006 - r0 * 0.5) * (t ** 0.6), -h / 2 + h * 0.4 + h * 0.6 * t))
    perfil.append((0, -h / 2 + h * 0.42))
    m.torno(perfil, mat=TINTE, pieza=p, marco=p, lados=20, liso=60)
