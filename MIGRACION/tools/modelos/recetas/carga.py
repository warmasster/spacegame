"""Cargo: what a hold carries (`assets/defs/components/carga.jsonc`)."""
import math

from kit import TINTE, hundir, receta


def bidon(m, tapon_mat):
    """A drum: rolled hoops, a rolled rim top and bottom, a sunk lid with its bung and its vent."""
    p = m['']
    r, h = p.r, p.h
    # its wall: two hoops rolled into it at a third and two thirds, a chime at each end
    perfil = [(0, -h / 2 + 0.012), (r - 0.022, -h / 2 + 0.012), (r - 0.016, -h / 2), (r - 0.004, -h / 2), (r, -h / 2 + 0.01), (r, -h / 2 + 0.026), (r - 0.008, -h / 2 + 0.034)]
    for y in (-h / 6, h / 6):
        perfil += [(r - 0.008, y - 0.03), (r - 0.002, y - 0.016), (r - 0.002, y + 0.016), (r - 0.008, y + 0.03)]
    perfil += [(r - 0.008, h / 2 - 0.034), (r, h / 2 - 0.026), (r, h / 2 - 0.01), (r - 0.004, h / 2), (r - 0.016, h / 2), (r - 0.022, h / 2 - 0.014), (0, h / 2 - 0.014)]
    m.torno(perfil, mat=TINTE, pieza=p, marco=p, lados=40)
    # the band round it (its own piece in the data): a clamping ring with its bolt
    a = m['aro']
    m.torno([(a.r - 0.012, -a.h / 2), (a.r, -a.h / 2 + 0.006), (a.r, a.h / 2 - 0.006), (a.r - 0.012, a.h / 2)], mat=TINTE, pieza=a, marco=a, lados=40)
    m.caja((0.034, a.h * 0.9, 0.05), en=(0, 0, a.r + 0.012), mat=TINTE, pieza=a, marco=a, bisel=0.004)
    m.cilindro(0.006, 0.07, en=(0, 0, a.r + 0.02), mat='acero', pieza=a, marco=a, eje='x', lados=10, bisel=0.001)
    # the bung (its own piece), a smaller vent across from it
    t = m['tapon']
    m.torno([(0, -t.h / 2), (t.r, -t.h / 2), (t.r, t.h / 2 - 0.008), (t.r - 0.006, t.h / 2), (t.r * 0.55, t.h / 2), (t.r * 0.5, t.h / 2 - 0.01), (0, t.h / 2 - 0.01)], mat=tapon_mat, pieza=t, marco=t, lados=20)
    m.cilindro(t.r * 0.42, 0.012, en=(0, t.h / 2 - 0.002, 0), mat=tapon_mat, pieza=t, marco=t, lados=6, bisel=0.002)
    m.cilindro(0.022, 0.02, en=(-0.13, h / 2 - 0.006, 0.02), mat=tapon_mat, pieza=p, marco=p, lados=12, bisel=0.003)


@receta('bidon_agua')
def bidon_agua(m):
    bidon(m, 'plastico_claro')


@receta('bidon_combustible')
def bidon_combustible(m):
    bidon(m, 'acero')


@receta('caja_repuestos')
def caja_repuestos(m):
    """A transit case: a ribbed tub, a lid with a lip, corner blocks, two latches, a handle each
    side, and the strap round it."""
    p, tapa, cincha = m[''], m['tapa'], m['cincha']
    sx, sy, sz = p.tam
    cuerpo = m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.018, seg=3)
    # ribs sunk into its four walls
    for hacia in ((1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)):
        hundir(cuerpo, hacia, 0.05, 0.012, marco=p)
    # corner blocks, top and bottom
    for x in (-1, 1):
        for z in (-1, 1):
            m.caja((0.07, 0.06, 0.07), en=(x * (sx / 2 - 0.03), -sy / 2 + 0.03, z * (sz / 2 - 0.03)), mat='plastico_negro', pieza=p, marco=p, bisel=0.012, seg=2)
    # a handle on each side: a bar on two lugs
    for x in (-1, 1):
        for z in (-0.09, 0.09):
            m.caja((0.022, 0.03, 0.024), en=(x * (sx / 2 + 0.008), 0.02, z), mat='plastico_negro', pieza=p, marco=p, bisel=0.004)
        m.tubo([(x * (sx / 2 + 0.03), 0.02, -0.1), (x * (sx / 2 + 0.03), 0.02, 0.1)], 0.009, mat='goma', pieza=p, marco=p, lados=10)
    # the lid: a lip over the tub, a raised field
    lid = m.caja(tuple(tapa.tam), mat=TINTE, pieza=tapa, marco=tapa, bisel=0.014, seg=3)
    hundir(lid, (0, 1, 0), 0.06, -0.008, marco=tapa)
    # two latches at the front
    for x in (-0.14, 0.14):
        m.caja((0.06, 0.07, 0.016), en=(x, -0.03, tapa.tam.z / 2 + 0.006), mat='acero', pieza=tapa, marco=tapa, bisel=0.004)
        m.caja((0.036, 0.02, 0.02), en=(x, -0.012, tapa.tam.z / 2 + 0.014), mat='acero_oscuro', pieza=tapa, marco=tapa, bisel=0.003)
    # the strap, with its buckle on top
    cx, cy, cz = cincha.tam
    m.caja((cx, cy, cz), mat=TINTE, pieza=cincha, marco=cincha, bisel=0.004, seg=1)
    m.caja((cx + 0.016, 0.014, 0.07), en=(0, cy / 2 + 0.002, 0.06), mat='acero', pieza=cincha, marco=cincha, bisel=0.003)


@receta('pale_regolito')
def pale_regolito(m):
    """A pallet of sacks: a deck of boards on three runners, three courses of full sacks (each
    sack a pillow), two straps over the lot."""
    p = m['']
    sx, sy, sz = p.tam
    for z in (-sz / 2 + 0.05, 0, sz / 2 - 0.05):
        m.caja((sx, sy * 0.55, 0.09), en=(0, -sy * 0.2, z), mat=TINTE, pieza=p, marco=p, bisel=0.006)
    for k in range(7):
        x = -sx / 2 + 0.065 + k * (sx - 0.13) / 6
        m.caja((0.12, sy * 0.3, sz), en=(x, sy * 0.33, 0), mat=TINTE, pieza=p, marco=p, bisel=0.005)
    # sacks: pillows in courses, each course laid the other way
    for n, id in enumerate(('sacos_1', 'sacos_2', 'sacos_3')):
        s = m[id]
        wx, wy, wz = s.tam
        a_lo_largo = n % 2 == 0
        nx, nz = (3, 2) if a_lo_largo else (2, 3)
        for i in range(nx):
            for j in range(nz):
                x = -wx / 2 + wx / nx * (i + 0.5)
                z = -wz / 2 + wz / nz * (j + 0.5)
                # a full sack: a squashed, rounded pillow, a little askew
                giro = ((i * 7 + j * 13 + n * 5) % 7 - 3) * 1.6
                m.esfera(0.5, en=(x, 0, z), mat=TINTE, pieza=s, marco=s.en((0, 0, 0)), lados=14, aplastar=1.0).scale = (1, 1, 1)
                ob = m.objetos[-1]
                saco(ob, s, (x, 0, z), (wx / nx * 0.98, wy * 1.04, wz / nz * 0.98), giro)
    for id in ('cincha', 'cincha_2'):
        c = m[id]
        m.caja(tuple(c.tam), mat=TINTE, pieza=c, marco=c, bisel=0.004, seg=1)
        m.caja((c.tam.x + 0.016, 0.014, 0.08), en=(0, c.tam.y / 2 + 0.002, 0.1), mat='acero', pieza=c, marco=c, bisel=0.003)


def saco(ob, pieza, centro, tam, giro):
    """Turns a unit sphere into a sack: a box with its faces swollen (a superellipsoid), turned
    a little about the vertical."""
    from kit import B, rot_xyz
    from mathutils import Vector
    c = Vector(centro)
    R = rot_xyz((0, giro, 0))
    for v in ob.data.vertices:
        # back to the piece's axes round the sack's middle
        w = v.co
        g = Vector((w.x, w.z, -w.y))
        d = pieza.a_local(g) - c
        d = d / 0.5
        # a sphere's direction to a superellipsoid's (exponent 4: flat faces, round edges)
        e = 0.42
        q = Vector((math.copysign(abs(d.x) ** e, d.x), math.copysign(abs(d.y) ** e, d.y), math.copysign(abs(d.z) ** e, d.z)))
        q = Vector((q.x * tam[0] / 2, q.y * tam[1] / 2, q.z * tam[2] / 2))
        v.co = B(pieza.a_mundo(c + R @ q))
