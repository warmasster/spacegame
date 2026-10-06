"""What a warship carries (`assets/defs/components/combate.jsonc`): its power pack and its brain,
its vectoring nacelles and tanks, its sensors, its weapons and decoys, the pieces of a fighter's
cockpit; and the styles of a fighter's own plain shapes (armour, the seat's platform and its
masts, the belly plug, the ramp's second length).

Kinds that ships mount in mirrored pairs (nacelles, tanks, cannons, launchers, radiators) are
built the same on both sides of their own x: the game turns a mirrored kind, it does not
reflect it."""
import math

from mathutils import Vector

from kit import TINTE, estilo, hundir, receta
from recetas.estilos import banda, rect_redondo, superelipse, tramos
from recetas.exterior import campana_de_tobera

# ---------------------------------------------------------------------------------------------
# power, brain


@receta('reactor_unificado')
def reactor_unificado(m):
    """The power pack: a sealed vessel in its jacket of cooling fins, a belt of studs at its
    waist, four risers up to the manifold on its head, on a bolted bed plate."""
    v, base, col = m['vasija'], m['base'], m['colector']
    r, h = v.r, v.h
    cuerpo = r - 0.05
    perfil = [(0, -h / 2), (cuerpo * 0.8, -h / 2), (cuerpo, -h / 2 + 0.06)]
    for y in (-h * 0.3, 0.0, h * 0.3):
        perfil += [(cuerpo, y - 0.03), (cuerpo + 0.018, y - 0.018), (cuerpo + 0.018, y + 0.018), (cuerpo, y + 0.03)]
    perfil += [(cuerpo, h / 2 - 0.1), (cuerpo * 0.72, h / 2 - 0.02), (0, h / 2)]
    m.torno(perfil, mat=TINTE, pieza=v, marco=v, lados=48)
    # its jacket: fins standing out all round, between the belts
    for k in range(24):
        a = math.degrees(math.tau * k / 24)
        for y0, y1 in ((-h * 0.3 + 0.04, -0.04), (0.04, h * 0.3 - 0.04)):
            c = ((cuerpo + 0.022) * math.cos(math.radians(a)), (y0 + y1) / 2, -(cuerpo + 0.022) * math.sin(math.radians(a)))
            m.caja((0.046, y1 - y0, 0.008), en=c, mat='aluminio', pieza=v, marco=v, bisel=0.002, seg=1, rot=(0, a, 0))
    # the studs of its waist belt and of its head
    m.tornillos([((cuerpo + 0.018) * math.cos(math.tau * k / 20), 0.0, (cuerpo + 0.018) * math.sin(math.tau * k / 20)) for k in range(20)], 0.012, 0.008, 'acero', v, v,
                eje=(0, 1, 0))
    banda(m, v, cuerpo * 0.5, cuerpo * 0.74, h / 2 - 0.02, h / 2 + 0.012, 'acero_oscuro', lados=32)
    m.tornillos([(cuerpo * 0.62 * math.cos(math.tau * k / 12), h / 2 + 0.012, cuerpo * 0.62 * math.sin(math.tau * k / 12)) for k in range(12)], 0.014, 0.01, 'acero', v, v)
    # hazard band low on it, and its instrument conduit
    m.rayas((math.tau * (cuerpo + 0.001), 0.07), en=(0, -h / 2 + 0.14, cuerpo + 0.001), pieza=v, marco=v, normal=(0, 0, 1), arriba=(0, 1, 0), n=40, radio=cuerpo + 0.001)
    m.tubo([(-(cuerpo + 0.03), -h / 2 + 0.2, 0.0), (-(cuerpo + 0.03), h / 2 - 0.16, 0.0)], 0.012, 'acero', v, v, lados=8)
    # the bed plate: eight sides, a stud at each
    sx, sy, sz = base.tam
    ocho = [(sx / 2 * math.cos(math.tau * (k + 0.5) / 8) / math.cos(math.pi / 8), sz / 2 * math.sin(math.tau * (k + 0.5) / 8) / math.cos(math.pi / 8)) for k in range(8)]
    m.extrusion(ocho, sy, mat=TINTE, pieza=base, marco=base, eje='y', bisel=0.012, seg=2)
    m.tornillos([(sx * 0.46 * math.cos(math.tau * k / 16), sy / 2, sz * 0.46 * math.sin(math.tau * k / 16)) for k in range(16)], 0.016, 0.012, 'acero', base, base)
    # the manifold: a ring main on four risers, its two outlets to the radiators
    cr, ch = col.r, col.h
    m.torno([(cr - 0.07, -ch / 2 + 0.05), (cr, -ch / 2 + 0.05), (cr, ch / 2 - 0.05), (cr - 0.07, ch / 2 - 0.05), (cr - 0.07, -ch / 2 + 0.05)], mat=TINTE, pieza=col, marco=col, lados=40)
    for k in range(4):
        a = math.tau * (k + 0.5) / 4
        x, z = (cr - 0.035) * math.cos(a), (cr - 0.035) * math.sin(a)
        m.tubo([(x, -ch / 2 - 0.1, z), (x, -ch / 2 + 0.06, z)], 0.03, 'acero', col, col, lados=12)
        m.brida(0.05, en=(x, -ch / 2 - 0.02, z), grosor=0.014, pernos=6, mat='acero_oscuro', pieza=col, marco=col, r_int=0.03)
    for lado in (-1, 1):
        m.tubo([(lado * (cr - 0.035), ch / 2 - 0.06, 0), (lado * (cr - 0.035), ch / 2 + 0.03, 0)], 0.036, 'acero', col, col, lados=12)
        m.brida(0.058, en=(lado * (cr - 0.035), ch / 2 + 0.03, 0), grosor=0.014, pernos=6, mat='acero_oscuro', pieza=col, marco=col, r_int=0.036)
    m.piloto((0, ch / 2 - 0.048, cr * 0.5), 0.01, 'piloto_verde', col, col)


@receta('nucleo_computo')
def nucleo_computo(m):
    """The ship's one computer: a tall sealed cabinet of drawn modules, each with its catches
    and its lamps, a cable trunk into its head and a wall of fins on its back."""
    p, dis = m[''], m['disipador']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.014, seg=3)
    hundir(ob, (0, 0, 1), 0.03, 0.012, mat='pintura_oscura', marco=p)
    for lado in (-1, 1):
        hundir(ob, (lado, 0, 0), 0.05, 0.006, marco=p)
        m.rejilla((sz * 0.5, sy * 0.22), en=(lado * (sx / 2 - 0.004), sy * 0.3, 0), pieza=p, marco=p, lamas=7, normal=(lado, 0, 0))
    # seven modules down its face
    filas = 7
    alto = (sy - 0.1) / filas
    for k in range(filas):
        y = -sy / 2 + 0.05 + alto * (k + 0.5)
        m.caja((sx - 0.09, alto - 0.014, 0.012), en=(0, y, sz / 2 - 0.008), mat='aluminio_anodizado', pieza=p, marco=p, bisel=0.003, seg=1)
        m.asa((-sx * 0.36, y, sz / 2 - 0.002), (-sx * 0.2, y, sz / 2 - 0.002), (0, 0, 1), alto=0.018, r=0.004, mat='acero', pieza=p, marco=p)
        for i, color in enumerate(('piloto_cian', 'piloto_verde', 'piloto_verde', 'piloto_ambar')):
            m.piloto((sx * 0.08 + i * 0.03, y, sz / 2 - 0.002), 0.0045, color if (k + i) % 5 else 'piloto_cian', p, p, eje=(0, 0, 1))
        m.tornillos([(sx * 0.4, y, sz / 2 - 0.002), (-sx * 0.42, y, sz / 2 - 0.002)], 0.005, 0.003, 'acero', p, p, eje=(0, 0, 1))
    # the trunk into its head and its hold-downs
    m.tubo([(sx * 0.2, sy / 2 - 0.01, 0), (sx * 0.2, sy / 2 + 0.05, 0), (sx * 0.2, sy / 2 + 0.07, -sz * 0.3)], 0.022, 'goma', p, p, lados=10, codo=0.03)
    m.brida(0.036, en=(sx * 0.2, sy / 2 + 0.004, 0), grosor=0.01, pernos=6, mat='acero_oscuro', pieza=p, marco=p, r_int=0.022)
    m.caja((sx + 0.03, 0.025, sz + 0.03), en=(0, -sy / 2 + 0.0125, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.005)
    # the fins on its back
    dx, dy, dz = dis.tam
    m.caja((dx, dy, dz * 0.3), en=(0, 0, dz * 0.35), mat=TINTE, pieza=dis, marco=dis, bisel=0.003, seg=1)
    m.aletas((dx, dy * 0.96, dz * 0.7), en=(0, 0, -dz * 0.15), n=22, grosor=0.006, mat=TINTE, pieza=dis, marco=dis, apila='x')


# ---------------------------------------------------------------------------------------------
# engines, tanks, radiators


@receta('gondola_caza')
def gondola_caza(m):
    """A vectoring nacelle: a waisted cowl with its bands and access panels, a pivot collar at
    its middle on either side, a sharp-lipped intake with its centrebody, and the bell in its
    ring of petals."""
    c, ad, tb = m['carcasa'], m['admision'], m['tobera']
    r, h = c.r, c.h
    perfil = [(r * 0.78, -h / 2), (r * 0.93, -h / 2 + 0.1), (r, -h / 2 + 0.34)]
    for y in (-0.42, 0.42):
        perfil += [(r, y - 0.012), (r - 0.007, y - 0.006), (r - 0.007, y + 0.006), (r, y + 0.012)]
    perfil += [(r, h / 2 - 0.3), (r * 0.985, h / 2 - 0.02), (r * 0.94, h / 2)]
    m.torno(perfil, mat=TINTE, pieza=c, marco=c, lados=48)
    for lado in (-1, 1):
        # where it turns on its wing: a collar and its trunnion
        m.torno([(0.17, 0), (0.17, 0.035), (0.13, 0.05), (0.13, 0.0)], en=(lado * (r - 0.012), 0, 0), mat='acero_oscuro', pieza=c, marco=c, eje=(lado, 0, 0), lados=28)
        m.tornillos([(lado * (r + 0.024), 0.15 * math.cos(math.tau * k / 10), 0.15 * math.sin(math.tau * k / 10)) for k in range(10)], 0.009, 0.005, 'acero', c, c, eje=(lado, 0, 0))
        # strakes above and below, an access panel fore and aft of the collar
        for y in (-0.62, 0.62):
            m.caja((0.014, 0.3, 0.34), en=(lado * (r - 0.002), y, 0), mat=TINTE, pieza=c, marco=c, bisel=0.005, seg=1)
            m.tornillos([(lado * (r + 0.006), y + dy, dz) for dy in (-0.13, 0.13) for dz in (-0.15, 0.15)], 0.007, 0.004, 'acero', c, c, eje=(lado, 0, 0))
    for arriba in (-1, 1):
        m.extrusion([(-0.012, 0), (0.012, 0), (0.004, 0.07), (-0.004, 0.07)], h * 0.62, en=(0, -0.05, arriba * (r - 0.006)), mat=TINTE, pieza=c, marco=c, eje='y', bisel=0.002) if arriba > 0 else \
            m.extrusion([(-0.012, 0), (0.012, 0), (0.004, -0.07), (-0.004, -0.07)], h * 0.62, en=(0, -0.05, arriba * (r - 0.006)), mat=TINTE, pieza=c, marco=c, eje='y', bisel=0.002)
    # the intake: a sharp lip, the duct dark behind it, a spike in its mouth
    ar, ah = ad.r, ad.h
    boca = ar * ad.taper
    m.torno([(ar * 0.95, -ah / 2), (ar, -ah / 2 + 0.03), (boca + 0.012, ah / 2 - 0.02), (boca, ah / 2), (boca - 0.03, ah / 2 - 0.03), (boca - 0.05, -ah / 2)], mat=TINTE, pieza=ad, marco=ad, lados=48)
    m.torno([(boca - 0.05, -ah / 2), (boca - 0.05, -ah / 2 + 0.01), (0.0, -ah / 2 + 0.01)], mat='tobera', pieza=ad, marco=ad, lados=32)
    m.torno([(0.13, -ah / 2 + 0.01), (0.11, 0.0), (0.05, ah / 2 - 0.03), (0.0, ah / 2 + 0.02)], mat='acero_oscuro', pieza=ad, marco=ad, lados=28, liso=60)
    for k in range(4):
        a = math.degrees(math.tau * (k + 0.5) / 4)
        d = (boca - 0.05 + 0.12) / 2
        m.caja((boca - 0.05 - 0.1, 0.05, 0.008), en=(d * math.cos(math.radians(a)), -ah / 2 + 0.04, -d * math.sin(math.radians(a))), mat='acero_oscuro', pieza=ad, marco=ad, bisel=0.002, seg=1, rot=(0, a, 0))
    # the bell and its petals
    tr, th = tb.r, tb.h
    salida = tr * tb.taper
    m.torno([(tr * 1.08, -th / 2), (tr * 1.12, -th / 2 + 0.03), (tr * 0.7, -th / 2 + 0.09)], mat='acero_oscuro', pieza=tb, marco=tb, lados=40)
    m.torno([(r_, y - th / 2 + 0.08) for r_, y in campana_de_tobera(tr * 0.62, salida * 0.9, th - 0.1, 0.014)], mat=TINTE, pieza=tb, marco=tb, lados=40, liso=60)
    for k in range(14):
        a = math.tau * k / 14
        a0, a1 = a - math.tau / 14 * 0.42, a + math.tau / 14 * 0.42
        def at(ang, rr, y):
            return (rr * math.cos(ang), y, rr * math.sin(ang))
        y0, y1 = -th / 2 + 0.07, th / 2 - 0.01
        r0, r1 = tr * 1.06, salida + 0.012
        m.piel([[at(a0, r0, y0), at(a1, r0, y0), at(a1, r0 + 0.008, y0), at(a0, r0 + 0.008, y0)], [at(a0, r1, y1), at(a1, r1, y1), at(a1, r1 + 0.006, y1), at(a0, r1 + 0.006, y1)]], mat='titanio', pieza=tb, marco=tb, liso=20)


@receta('deposito_caza')
def deposito_caza(m):
    """A tank faired into a wing root: a drawn shell with its weld bands and two straps, a
    filler under a flush cap, an ogive forward and a boat tail aft."""
    c, proa, popa = m['cuerpo'], m['proa'], m['popa']
    r, h = c.r, c.h
    perfil = [(r * 0.98, -h / 2)]
    for y in (-h * 0.3, 0.0, h * 0.3):
        perfil += [(r, y - 0.014), (r + 0.004, y - 0.006), (r + 0.004, y + 0.006), (r, y + 0.014)]
    perfil += [(r * 0.98, h / 2)]
    m.torno(perfil, mat=TINTE, pieza=c, marco=c, lados=36)
    for y in (-h * 0.38, h * 0.38):
        banda(m, c, r - 0.002, r + 0.008, y - 0.03, y + 0.03, 'acero_oscuro', lados=36)
        for lado in (-1, 1):
            m.caja((0.03, 0.05, 0.04), en=(lado * (r + 0.012), y, 0), mat='acero', pieza=c, marco=c, bisel=0.005)
    # filler caps on top and underneath (it is mounted either way up)
    for arriba in (-1, 1):
        m.cilindro(0.055, 0.012, en=(0, h * 0.18, arriba * (r - 0.002)), mat='acero', pieza=c, marco=c, eje=(0, 0, arriba), lados=18, bisel=0.003)
        m.tornillos([(0.03 * math.cos(math.tau * k / 6), h * 0.18 + 0.03 * math.sin(math.tau * k / 6), arriba * (r + 0.004)) for k in range(6)], 0.005, 0.003, 'acero_oscuro', c, c, eje=(0, 0, arriba))
    pr, ph = proa.r, proa.h
    m.torno([(pr * 0.98, -ph / 2), (pr * 0.9, -ph * 0.1), (pr * 0.62, ph * 0.3), (pr * proa.taper * 0.7, ph / 2 - 0.02), (0, ph / 2)], mat=TINTE, pieza=proa, marco=proa, lados=36, liso=70)
    qr, qh = popa.r, popa.h
    m.torno([(qr * 0.98, -qh / 2), (qr * 0.86, 0.0), (qr * popa.taper, qh / 2 - 0.02), (qr * popa.taper * 0.6, qh / 2), (0, qh / 2)], mat=TINTE, pieza=popa, marco=popa, lados=36, liso=70)


@receta('radiador_caza')
def radiador_caza(m):
    """A radiator that lies on the spine: a framed panel of fine fins with a header along each
    long edge and the tubes between them, three hinge knuckles on each."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx - 0.08, sy * 0.5, sz - 0.04), mat=TINTE, pieza=p, marco=p, bisel=0.002, seg=1)
    m.aletas((sx - 0.1, sy, sz - 0.08), n=34, grosor=0.006, mat=TINTE, pieza=p, marco=p, apila='z')
    for lado in (-1, 1):
        m.tubo([(lado * (sx / 2 - 0.022), 0, -sz / 2 + 0.01), (lado * (sx / 2 - 0.022), 0, sz / 2 - 0.01)], 0.02, 'aluminio', p, p, lados=12)
        for z in (-sz * 0.36, 0.0, sz * 0.36):
            m.cilindro(0.026, 0.11, en=(lado * (sx / 2 - 0.022), 0, z), mat='acero_oscuro', pieza=p, marco=p, eje='z', lados=14, bisel=0.004)
    for z in (-sz / 2 + 0.012, sz / 2 - 0.012):
        m.caja((sx - 0.04, sy, 0.024), en=(0, 0, z), mat='aluminio', pieza=p, marco=p, bisel=0.003, seg=1)


# ---------------------------------------------------------------------------------------------
# sensors


@receta('radar_morro')
def radar_morro(m):
    """A flat array on its ring: rings of radiating elements on its face, the transmitter box
    behind it with its fins and its two fat connectors."""
    p, tras = m[''], m['trasera']
    r, h = p.r, p.h
    perfil = [(0, h / 2)]
    for k in range(1, 7):
        rr = r * 0.92 * k / 6
        perfil += [(rr - 0.012, h / 2), (rr - 0.008, h / 2 - 0.004), (rr - 0.004, h / 2)]
    perfil += [(r * 0.94, h / 2), (r * 0.94, h / 2 - 0.03)]
    m.torno(perfil, mat=TINTE, pieza=p, marco=p, lados=48)
    m.torno([(r * 0.94, h / 2 - 0.03), (r, h / 2 - 0.03), (r, -h / 2), (r * 0.5, -h / 2), (r * 0.5, h / 2 - 0.03)], mat='acero_oscuro', pieza=p, marco=p, lados=48)
    m.tornillos([(r * 0.97 * math.cos(math.tau * k / 16), h / 2 - 0.03, r * 0.97 * math.sin(math.tau * k / 16)) for k in range(16)], 0.007, 0.004, 'acero', p, p)
    for k in range(8):
        a = math.degrees(math.tau * k / 8)
        m.caja((r * 0.9, 0.003, 0.004), en=(r * 0.47 * math.cos(math.radians(a)), h / 2 + 0.0005, -r * 0.47 * math.sin(math.radians(a))), mat='laton', pieza=p, marco=p, bisel=0.0, seg=1, rot=(0, a, 0))
    sx, sy, sz = tras.tam
    m.caja((sx * 0.86, sy * 0.86, sz * 0.8), en=(0, 0, sz * 0.05), mat=TINTE, pieza=tras, marco=tras, bisel=0.012, seg=2)
    m.aletas((sx * 0.8, sy * 0.8, sz * 0.2), en=(0, 0, -sz * 0.4), n=12, grosor=0.005, mat='aluminio', pieza=tras, marco=tras, apila='x')
    for lado in (-1, 1):
        m.torno([(0.026, 0), (0.026, 0.02), (0.02, 0.026), (0.02, 0.04), (0, 0.04)], en=(lado * sx * 0.43, 0.0, 0.0), mat='acero', pieza=tras, marco=tras, eje=(lado, 0, 0), lados=14)


@receta('irst_bola')
def irst_bola(m):
    """An infrared seeker: a turned collar bolted down, and on it the ball in its yoke, a dark
    window let into it."""
    p, op = m[''], m['optica']
    r, h = p.r, p.h
    m.torno([(r, -h / 2), (r, -h / 2 + 0.02), (r * 0.8, -h / 2 + 0.03), (r * 0.74, h / 2), (0, h / 2)], mat=TINTE, pieza=p, marco=p, lados=32)
    m.tornillos([(r * 0.9 * math.cos(math.tau * k / 8), -h / 2 + 0.02, r * 0.9 * math.sin(math.tau * k / 8)) for k in range(8)], 0.006, 0.004, 'acero', p, p)
    orr = op.r
    m.esfera(orr * 0.98, en=(0, 0.0, 0), mat='acero_oscuro', pieza=op, marco=op, lados=28)
    m.esfera(orr * 0.62, en=(0, orr * 0.3, orr * 0.52), mat=TINTE, pieza=op, marco=op, lados=20)
    m.torno([(orr * 0.66, 0), (orr * 0.72, 0.006), (orr * 0.66, 0.012)], en=(0, orr * 0.28, orr * 0.5), mat='acero', pieza=op, marco=op, eje=(0, 0.5, 0.86), lados=24)


@receta('alertador_radar')
def alertador_radar(m):
    """A blade antenna: a swept fin on a bolted foot."""
    p = m['']
    sx, sy, sz = p.tam
    m.extrusion([(-sz / 2, -sy / 2 + 0.012), (sz / 2, -sy / 2 + 0.012), (sz * 0.1, sy / 2), (-sz * 0.34, sy / 2)], sx * 0.36, mat=TINTE, pieza=p, marco=p, eje='x', bisel=0.005, seg=2)
    m.caja((sx * 1.5, 0.012, sz * 1.08), en=(0, -sy / 2 + 0.006, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.004)
    m.tornillos([(x * sx * 0.55, -sy / 2 + 0.012, z * sz * 0.46) for x in (-1, 1) for z in (-1, 1)], 0.005, 0.003, 'acero', p, p)


@receta('transpondedor')
def transpondedor(m):
    """A transponder: a finned box with its coaxial connectors and its two lamps."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx, sy * 0.76, sz), en=(0, -sy * 0.12, 0), mat=TINTE, pieza=p, marco=p, bisel=0.008, seg=2)
    m.aletas((sx * 0.9, sy * 0.22, sz * 0.9), en=(0, sy * 0.38, 0), n=11, grosor=0.005, mat=TINTE, pieza=p, marco=p, apila='x')
    for cara in (-1, 1):
        for k in range(3):
            m.torno([(0.012, 0), (0.012, 0.012), (0.008, 0.016), (0.008, 0.024), (0, 0.024)], en=(-sx * 0.25 + sx * 0.18 * k, -sy * 0.1, cara * sz / 2), mat='laton', pieza=p, marco=p, eje=(0, 0, cara), lados=12)
        m.piloto((sx * 0.32, -sy * 0.1, cara * sz / 2), 0.005, 'piloto_verde', p, p, eje=(0, 0, cara))
        m.piloto((sx * 0.4, -sy * 0.1, cara * sz / 2), 0.005, 'piloto_ambar', p, p, eje=(0, 0, cara))
    m.caja((sx + 0.02, 0.014, sz * 0.5), en=(0, -sy / 2 + 0.007, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.004)


@receta('perturbador')
def perturbador(m):
    """A jammer pod under the belly: a slim body between two pale caps that let its noise out,
    a scoop for its cooling, and the saddle it hangs by."""
    p = m['']
    sx, sy, sz = p.tam
    a, b = sx / 2 * 0.86, sy / 2 * 0.82

    def anillo(k, z):
        return [(x * k, y * k - sy * 0.06, z) for x, y in superelipse(a, b, 20, 2.4)]

    m.piel([anillo(0.86, -sz * 0.34), anillo(1.0, -sz * 0.2), anillo(1.0, sz * 0.2), anillo(0.86, sz * 0.34)], mat=TINTE, pieza=p, marco=p, tapas=False)
    for lado in (-1, 1):
        m.piel([anillo(0.86, lado * sz * 0.34), anillo(0.7, lado * sz * 0.43), anillo(0.36, lado * sz * 0.49), anillo(0.05, lado * sz * 0.5)], mat='plastico_claro', pieza=p, marco=p, tapas=True, liso=60)
        m.piel([anillo(0.88, lado * sz * 0.335), anillo(0.88, lado * sz * 0.345)], mat='acero_oscuro', pieza=p, marco=p, tapas=False)
    m.caja((sx * 0.36, sy * 0.2, sz * 0.2), en=(0, -sy / 2 + sy * 0.12, 0), mat=TINTE, pieza=p, marco=p, bisel=0.014, seg=2)
    m.rejilla((sx * 0.3, sy * 0.12), en=(0, -sy / 2 + sy * 0.12, sz * 0.1 + 0.001), pieza=p, marco=p, lamas=4, normal=(0, 0, 1))
    for z in (-sz * 0.22, sz * 0.22):
        m.caja((sx * 0.42, sy * 0.3, 0.07), en=(0, sy / 2 - sy * 0.15, z), mat='acero_oscuro', pieza=p, marco=p, bisel=0.008)


# ---------------------------------------------------------------------------------------------
# weapons


@receta('canon_rotativo')
def canon_rotativo(m):
    """A rotary cannon in its fairing: the fairing drawn to a point each end with its vents and
    its ejection port, six barrels in their clamps out of its nose, and behind it the drum with
    the chute that feeds it."""
    car, can, tam = m['carenado'], m['canon'], m['tambor']
    sx, sy, sz = car.tam
    a, b = sx / 2, sy / 2

    def anillo(k, z, baja=0.0):
        return [(x * k, y * k + baja, z) for x, y in superelipse(a, b, 20, 3.2)]

    m.piel([anillo(0.2, -sz / 2), anillo(0.7, -sz * 0.42), anillo(1.0, -sz * 0.26), anillo(1.0, sz * 0.24), anillo(0.82, sz * 0.4), anillo(0.5, sz / 2, -0.02)], mat=TINTE, pieza=car, marco=car, tapas=True)
    for lado in (-1, 1):
        m.rejilla((sz * 0.16, sy * 0.3), en=(lado * (a - 0.002), sy * 0.12, -sz * 0.05), pieza=car, marco=car, lamas=5, normal=(lado, 0, 0))
        m.caja((0.01, sy * 0.24, sz * 0.12), en=(lado * (a - 0.002), -sy * 0.14, sz * 0.14), mat='tobera', pieza=car, marco=car, bisel=0.003, seg=1)
        m.tornillos([(lado * a, y * sy * 0.36, z) for y in (-1, 1) for z in tramos(-sz * 0.24, sz * 0.22, 0.16)], 0.005, 0.003, 'acero', car, car, eje=(lado, 0, 0))
    # six barrels round an axle, three clamps along them, a flash hider at the muzzle
    r, h = can.r, can.h
    for k in range(6):
        ang = math.tau * k / 6
        m.cilindro(0.0125, h * 0.98, en=(r * 0.62 * math.cos(ang), 0.0, r * 0.62 * math.sin(ang)), mat=TINTE, pieza=can, marco=can, lados=10, bisel=0.002)
    m.cilindro(0.012, h * 0.9, mat='acero_oscuro', pieza=can, marco=can, lados=8, bisel=0.002)
    for y in (-h * 0.34, 0.0, h * 0.34):
        m.cilindro(r, 0.03, en=(0, y, 0), mat='acero_oscuro', pieza=can, marco=can, lados=24, bisel=0.004)
    m.torno([(r * 0.98, 0), (r * 1.0, 0.03), (r * 0.8, 0.05), (r * 0.8, 0.0)], en=(0, h / 2 - 0.06, 0), mat='acero_oscuro', pieza=can, marco=can, lados=24)
    # the drum (its axis across the ship) and its chute up to the breech
    tr, th = tam.r, tam.h
    m.torno([(tr * 0.9, -th / 2), (tr, -th / 2 + 0.02), (tr, th / 2 - 0.02), (tr * 0.9, th / 2), (0, th / 2)], mat=TINTE, pieza=tam, marco=tam, lados=32)
    m.torno([(0, -th / 2), (tr * 0.9, -th / 2)], mat=TINTE, pieza=tam, marco=tam, lados=32)
    for lado in (-1, 1):
        m.brida(tr * 0.5, en=(0, lado * th / 2, 0), grosor=0.01, pernos=8, mat='acero_oscuro', pieza=tam, marco=tam, eje=(0, lado, 0), r_int=tr * 0.2)
    m.caja((0.08, th * 0.7, 0.05), en=(tr * 0.7, 0, tr * 0.7), mat='acero_oscuro', pieza=tam, marco=tam, bisel=0.008)


@receta('lanzador_misiles')
def lanzador_misiles(m):
    """A box of four launch tubes: each mouth with the nose of its missile deep in it, blast
    doors on its back, two lugs on top to hang it by, and its warning bands."""
    p = m['']
    sx, sy, sz = p.tam
    ob = m.caja((sx, sy * 0.86, sz), en=(0, -sy * 0.07, 0), mat=TINTE, pieza=p, marco=p, bisel=0.014, seg=3)
    hundir(ob, (0, 0, 1), 0.014, 0.02, mat='pintura_oscura', marco=p)
    hundir(ob, (0, 0, -1), 0.014, 0.012, mat='pintura_oscura', marco=p)
    rt = min(sx, sy * 0.86) * 0.2
    for ix in (-1, 1):
        for iy in (-1, 1):
            c = (ix * sx * 0.23, -sy * 0.07 + iy * sy * 0.2, 0.0)
            # the tube's mouth, its dark bore and the missile's nose in it
            m.torno([(rt, 0), (rt + 0.008, 0.006), (rt + 0.008, 0.02), (rt, 0.02)], en=(c[0], c[1], sz / 2 - 0.022), mat='acero', pieza=p, marco=p, eje=(0, 0, 1), lados=20)
            m.torno([(rt, 0.004), (0, 0.004)], en=(c[0], c[1], sz / 2 - 0.09), mat='tobera', pieza=p, marco=p, eje=(0, 0, 1), lados=20)
            m.torno([(rt * 0.86, 0.0), (rt * 0.8, 0.03), (rt * 0.5, 0.062), (0, 0.078)], en=(c[0], c[1], sz / 2 - 0.088), mat='pintura_blanca', pieza=p, marco=p, eje=(0, 0, 1), lados=20, liso=70)
            # its blast door at the back
            m.caja((rt * 1.8, rt * 1.8, 0.008), en=(c[0], c[1], -sz / 2 + 0.014), mat='acero_oscuro', pieza=p, marco=p, bisel=0.004)
            m.tornillos([(c[0] + dx * rt * 0.7, c[1] + dy * rt * 0.7, -sz / 2 + 0.01) for dx in (-1, 1) for dy in (-1, 1)], 0.004, 0.003, 'acero', p, p, eje=(0, 0, -1))
    # the two lugs it hangs by, and a strongback between them
    m.caja((sx * 0.3, sy * 0.1, sz * 0.7), en=(0, sy / 2 - sy * 0.09, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.008)
    for z in (-sz * 0.26, sz * 0.26):
        m.cilindro(0.022, sx * 0.4, en=(0, sy / 2 - sy * 0.04, z), mat='acero', pieza=p, marco=p, eje='x', lados=12, bisel=0.004)
    for lado in (-1, 1):
        m.rayas((sz * 0.16, sy * 0.5), en=(lado * (sx / 2 + 0.0004), -sy * 0.07, sz * 0.36), pieza=p, marco=p, normal=(lado, 0, 0), arriba=(0, 1, 0), n=5, mats=('pintura_roja', 'pintura_blanca'))
        m.tornillos([(lado * sx / 2, -sy * 0.07 + y * sy * 0.34, z) for y in (-1, 1) for z in tramos(-sz * 0.4, sz * 0.2, 0.2)], 0.005, 0.003, 'acero', p, p, eje=(lado, 0, 0))


@receta('lanzasenuelos')
def lanzasenuelos(m):
    """A dispenser of decoys: a block of fifteen square cells, each under its cap, in a frame
    screwed to the hull."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx, sy * 0.8, sz), en=(0, -sy * 0.1, 0), mat=TINTE, pieza=p, marco=p, bisel=0.008, seg=2)
    m.caja((sx * 0.94, 0.012, sz * 0.94), en=(0, sy * 0.3 + 0.004, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.003)
    nx, nz = 3, 5
    cx, cz = sx * 0.86 / nx, sz * 0.88 / nz
    for i in range(nx):
        for j in range(nz):
            c = (-sx * 0.43 + cx * (i + 0.5), sy * 0.3 + 0.012, -sz * 0.44 + cz * (j + 0.5))
            m.caja((cx * 0.84, 0.01, cz * 0.84), en=c, mat='pintura_roja' if (i + j) % 2 else 'plastico_claro', pieza=p, marco=p, bisel=0.003, seg=1)
    m.tornillos([(x * sx * 0.46, sy * 0.3 + 0.01, z * sz * 0.47) for x in (-1, 0, 1) for z in (-1, 1)], 0.005, 0.003, 'acero', p, p)


# ---------------------------------------------------------------------------------------------
# a fighter's cockpit


@receta('consola_caza')
def consola_caza(m):
    """A fighter's console: a narrow base cut away for the knees with a vent each side and a
    foot rail, the panel's face left flat, a padded glare shield over it."""
    cuerpo, cara, visera = m['cuerpo'], m['cara'], m['visera']
    sx, sy, sz = cuerpo.tam
    ob = m.caja((sx, sy, sz), mat=TINTE, pieza=cuerpo, marco=cuerpo, bisel=0.02, seg=3)
    hundir(ob, (0, 0, -1), 0.06, 0.035, mat='pintura_oscura', marco=cuerpo)
    for x in (-sx * 0.3, sx * 0.3):
        m.rejilla((sx * 0.24, 0.16), en=(x, -0.14, -sz / 2 + 0.03), pieza=cuerpo, marco=cuerpo, lamas=6, normal=(0, 0, -1))
    m.tubo([(-sx / 2 + 0.1, -sy / 2 + 0.1, -sz / 2 - 0.04), (sx / 2 - 0.1, -sy / 2 + 0.1, -sz / 2 - 0.04)], 0.014, 'acero', cuerpo, cuerpo, lados=12)
    for x in (-sx / 2 + 0.1, 0.0, sx / 2 - 0.1):
        m.caja((0.03, 0.05, 0.06), en=(x, -sy / 2 + 0.1, -sz / 2 - 0.014), mat='acero_oscuro', pieza=cuerpo, marco=cuerpo, bisel=0.006)
    for lado in (-1, 1):
        hundir(ob, (lado, 0, 0), 0.05, 0.008, marco=cuerpo)
        m.pernos(cuerpo, 'x', 0.07, (2, 2), 0.007, lado=lado)
    m.caja(tuple(cara.tam), mat=TINTE, pieza=cara, marco=cara, bisel=0.006, seg=2)
    vx, vy, vz = visera.tam
    m.caja((vx, vy * 0.7, vz), mat=TINTE, pieza=visera, marco=visera, bisel=0.008, seg=2)
    m.capsula(0.026, vx, en=(0, 0, -vz / 2 + 0.01), mat='goma', pieza=visera, marco=visera, eje='x', lados=14, fondo=0.8)


@receta('tambor_paneles')
def tambor_paneles(m):
    """The drum of panels: three flat faces round an axle, a bright rail down each edge between
    them and a hub at each end."""
    p = m['']
    r, h = p.r, p.h
    tri = [(r * math.cos(math.tau * k / 3), r * math.sin(math.tau * k / 3)) for k in range(3)]
    # (cut back a little at its corners: the faces stay flat right out to where the panels end)
    corte = 0.9
    seis = []
    for k in range(3):
        a, b, c = Vector(tri[k - 1]), Vector(tri[k]), Vector(tri[(k + 1) % 3])
        seis += [tuple(b + (a - b) * (1 - corte) * 0.5), tuple(b + (c - b) * (1 - corte) * 0.5)]
    m.extrusion(seis, h, mat=TINTE, pieza=p, marco=p, eje='y', bisel=0.002, seg=1)
    for k in range(3):
        x, z = tri[k]
        m.tubo([(x * 0.93, -h / 2 + 0.004, z * 0.93), (x * 0.93, h / 2 - 0.004, z * 0.93)], 0.006, 'aluminio', p, p, lados=8)
    for lado in (-1, 1):
        m.cilindro(r * 0.42, 0.012, en=(0, lado * (h / 2 + 0.004), 0), mat='acero_oscuro', pieza=p, marco=p, lados=20, bisel=0.003)
        m.cilindro(0.018, 0.03, en=(0, lado * (h / 2 + 0.014), 0), mat='acero', pieza=p, marco=p, lados=12, bisel=0.003)


@receta('soporte_tambor')
def soporte_tambor(m):
    """The drum's cradle: a bearing plate at each end on an arm to the wall, and the little
    motor that turns it."""
    for nombre in ('proa', 'popa'):
        pl, br = m[nombre], m['brazo_' + nombre]
        sx, sy, sz = pl.tam
        m.caja((sx, sy, sz * 0.8), mat=TINTE, pieza=pl, marco=pl, bisel=0.012, seg=2)
        m.brida(0.045, en=(0, 0.01, 0), grosor=sz * 1.1, pernos=6, mat='acero_oscuro', pieza=pl, marco=pl, eje='z', r_int=0.02)
        bx, by, bz = br.tam
        m.caja((bx, by, bz * 0.8), mat=TINTE, pieza=br, marco=br, bisel=0.008, seg=2)
        m.tornillos([(bx / 2 - 0.02, y, 0) for y in (-by * 0.25, by * 0.25)], 0.006, 0.004, 'acero', br, br, eje=(1, 0, 0))
    mo = m['motor']
    m.cilindro(mo.r, mo.h, mat=TINTE, pieza=mo, marco=mo, lados=20, bisel=0.006)
    for y in tramos(-mo.h * 0.3, mo.h * 0.3, 0.02):
        banda(m, mo, mo.r - 0.002, mo.r + 0.006, y - 0.003, y + 0.003, 'aluminio', lados=20)


@receta('repisa_caza')
def repisa_caza(m):
    """A shelf on the cockpit's sill: the panel's face flat on its plate, held off the wall by a
    bracket with two gussets."""
    cara, pie = m['cara'], m['pie']
    m.caja(tuple(cara.tam), mat=TINTE, pieza=cara, marco=cara, bisel=0.005, seg=2)
    sx, sy, sz = pie.tam
    m.caja((sx * 0.5, sy, sz * 0.9), mat=TINTE, pieza=pie, marco=pie, bisel=0.01, seg=2)
    for z in (-sz * 0.36, sz * 0.36):
        m.extrusion([(-sx / 2, -sy / 2), (sx / 2, sy / 2), (-sx / 2, sy / 2)], 0.02, en=(0, 0, z), mat=TINTE, pieza=pie, marco=pie, eje='z', bisel=0.003)
    m.tornillos([(-sx / 2 * 0.5, y, z) for y in (-sy * 0.3, sy * 0.3) for z in (-sz * 0.2, sz * 0.2)], 0.006, 0.004, 'acero', pie, pie, eje=(-1, 0, 0))


# ---------------------------------------------------------------------------------------------
# styles of a fighter's plain shapes


def ejes_de_placa(p):
    """A plate: (index of its thin axis, the two others)."""
    t = list(p.tam)
    i = min(range(3), key=lambda k: t[k])
    return i, [k for k in range(3) if k != i]


@estilo('blindaje')
def blindaje(m, p):
    """Armour: a backing plate with a course of thick tiles on each face, a gap between every
    two and a bolt through every corner where four meet."""
    i, (a, b) = ejes_de_placa(p)
    t = list(p.tam)
    fondo = t[i] * 0.5
    tam = [0, 0, 0]
    tam[i], tam[a], tam[b] = fondo, t[a], t[b]
    m.caja(tam, mat='acero_oscuro', pieza=p, marco=p, bisel=0.003, seg=1)
    na, nb = max(1, round(t[a] / 0.44)), max(1, round(t[b] / 0.44))
    da, db = t[a] / na, t[b] / nb
    for lado in (-1, 1):
        for u in range(na):
            for v in range(nb):
                c, s = [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]
                c[i] = lado * (fondo / 2 + t[i] * 0.125)
                c[a], c[b] = -t[a] / 2 + da * (u + 0.5), -t[b] / 2 + db * (v + 0.5)
                s[i], s[a], s[b] = t[i] * 0.25, da - 0.012, db - 0.012
                m.caja(s, en=c, mat=TINTE, pieza=p, marco=p, bisel=min(0.006, t[i] * 0.1), seg=1)
        pts = []
        for u in range(na + 1):
            for v in range(nb + 1):
                q = [0.0, 0.0, 0.0]
                q[i] = lado * fondo / 2
                q[a] = min(max(-t[a] / 2 + da * u, -t[a] / 2 + 0.02), t[a] / 2 - 0.02)
                q[b] = min(max(-t[b] / 2 + db * v, -t[b] / 2 + 0.02), t[b] / 2 - 0.02)
                pts.append(q)
        eje = [0, 0, 0]
        eje[i] = lado
        m.tornillos(pts, 0.008, t[i] * 0.2, 'acero', p, p, eje=tuple(eje))


@estilo('plataforma_asiento')
def plataforma_asiento(m, p):
    """The seat's platform: a tread plate in a frame, a hazard band round its edge, and the
    foot of each mast at its after corners."""
    sx, sy, sz = p.tam
    m.caja((sx - 0.05, sy * 0.9, sz - 0.05), en=(0, -sy * 0.05, 0), mat=TINTE, pieza=p, marco=p, bisel=0.002, seg=1)
    for z in (-1, 1):
        m.caja((sx, sy, 0.035), en=(0, 0, z * (sz / 2 - 0.0175)), mat='aluminio', pieza=p, marco=p, bisel=0.004, seg=1)
        m.rayas((sx - 0.08, 0.05), en=(0, sy / 2 - sy * 0.05 + 0.0004, z * (sz / 2 - 0.07)), pieza=p, marco=p, normal=(0, 1, 0), arriba=(0, 0, 1), n=14)
    for x in (-1, 1):
        m.caja((0.035, sy, sz - 0.066), en=(x * (sx / 2 - 0.0175), 0, 0), mat='aluminio', pieza=p, marco=p, bisel=0.004, seg=1)
        m.rayas((sz - 0.2, 0.05), en=(x * (sx / 2 - 0.07), sy / 2 - sy * 0.05 + 0.0004, 0), pieza=p, marco=p, normal=(0, 1, 0), arriba=(1, 0, 0), n=20)
    pts = [(x * (sx / 2 - 0.0175), sy / 2, z) for x in (-1, 1) for z in tramos(-sz / 2 + 0.06, sz / 2 - 0.06, 0.2)]
    m.tornillos(pts, 0.005, 0.002, 'acero_oscuro', p, p)


@estilo('panza')
def panza(m, p):
    """The belly plug that rides under the seat's platform: a smooth plate with a seal round
    its edge and, underneath, the warning of what comes down there."""
    sx, sy, sz = p.tam
    m.caja((sx, sy * 0.8, sz), en=(0, -sy * 0.1, 0), mat=TINTE, pieza=p, marco=p, bisel=0.006, seg=2)
    m.caja((sx - 0.03, sy * 0.3, sz - 0.03), en=(0, sy * 0.35, 0), mat='goma', pieza=p, marco=p, bisel=0.004, seg=1)
    for z in (-1, 1):
        m.rayas((sx - 0.1, 0.07), en=(0, -sy / 2 - 0.0004, z * (sz / 2 - 0.07)), pieza=p, marco=p, normal=(0, -1, 0), arriba=(0, 0, 1), n=18)
    for x in (-1, 1):
        m.rayas((sz - 0.26, 0.07), en=(x * (sx / 2 - 0.07), -sy / 2 - 0.0004, 0), pieza=p, marco=p, normal=(0, -1, 0), arriba=(1, 0, 0), n=18)
    m.texto('ASIENTO', en=(0, -sy / 2 - 0.0004, sz * 0.12), alto=0.12, mat='pintura_amarilla', pieza=p, marco=p, normal=(0, -1, 0), arriba=(0, 0, 1))
    m.texto('NO PASAR DEBAJO', en=(0, -sy / 2 - 0.0004, -sz * 0.1), alto=0.07, mat='pintura_amarilla', pieza=p, marco=p, normal=(0, -1, 0), arriba=(0, 0, 1))
    m.tornillos([(x * (sx / 2 - 0.03), -sy / 2, z * (sz / 2 - 0.03)) for x in (-1, 1) for z in (-1, 1)], 0.008, 0.003, 'acero', p, p, eje=(0, -1, 0))


@estilo('mastil')
def mastil(m, p):
    """A mast the seat's platform rides on: a bright rod out of a dark sleeve, a collar where
    one enters the other and a foot on the platform."""
    r = min(p.tam.x, p.tam.z) / 2
    h = p.tam.y
    m.cilindro(r * 0.62, h * 0.98, mat='cromo', pieza=p, marco=p, lados=16, bisel=0.002)
    m.cilindro(r * 0.92, h * 0.36, en=(0, -h / 2 + h * 0.18, 0), mat=TINTE, pieza=p, marco=p, lados=18, bisel=0.004)
    banda(m, p, r * 0.6, r * 1.0, -h / 2 + h * 0.36 - 0.012, -h / 2 + h * 0.36 + 0.012, 'acero_oscuro', lados=18)
    m.caja((r * 3.0, 0.016, r * 3.0), en=(0, -h / 2 + 0.008, 0), mat='acero_oscuro', pieza=p, marco=p, bisel=0.004)
    banda(m, p, r * 0.6, r * 0.9, h / 2 - 0.03, h / 2, 'acero_oscuro', lados=18)


@estilo('rampa_larga')
def rampa_larga(m, p):
    """The ramp's second length: a plate in a frame, cleats across it to climb by, a yellow lip
    at its foot."""
    sx, sy, sz = p.tam
    m.caja((sx - 0.06, sy - 0.06, sz * 0.7), mat=TINTE, pieza=p, marco=p, bisel=0.002, seg=1)
    for x in (-1, 1):
        m.caja((0.04, sy, sz), en=(x * (sx / 2 - 0.02), 0, 0), mat='aluminio', pieza=p, marco=p, bisel=0.004, seg=1)
    m.caja((sx - 0.08, 0.04, sz), en=(0, -sy / 2 + 0.02, 0), mat='aluminio', pieza=p, marco=p, bisel=0.004, seg=1)
    m.caja((sx - 0.08, 0.06, sz), en=(0, sy / 2 - 0.03, 0), mat='pintura_amarilla', pieza=p, marco=p, bisel=0.004, seg=1)
    for y in tramos(-sy / 2 + 0.12, sy / 2 - 0.14, 0.11):
        m.caja((sx - 0.12, 0.014, 0.008), en=(0, y, sz / 2 + 0.001), mat='acero_oscuro', pieza=p, marco=p, bisel=0.002, seg=1)
