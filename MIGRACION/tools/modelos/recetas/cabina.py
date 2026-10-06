"""What a cabin is fitted with: seats, consoles, lockers, boxes, rails, cargo clamps.

A piece that carries a panel (a console's face, a wall box) keeps the face the panel is laid on
flat and where its shape has it: the game draws the panel's controls on it.
"""
from kit import TINTE, hundir, receta


@receta('asiento')
def asiento(m):
    """A crew seat for someone in a suit with its pack on: a pan whose pad is under the thighs
    and whose shell runs on behind as a shelf under the pack; instead of a back, a cradle for the
    pack (a plate behind it with its rest pads, a wing each side, the latches that take it); a
    helmet rest over the pack; the harness stowed on the wings and the pan's sides; armrests on
    brackets off the pan; all on a sprung column that runs on floor rails.

    Where the sitter is (the seat's frame: origin on the deck under the column): the thighs on
    the pad (its top at y 0.46) out to |x| 0.24; the torso's back at z -0.20; the pack behind it
    from z -0.21 to -0.49, y 0.53 to 1.41, 0.57 wide; the helmet a ball of radius 0.2 at y 1.32,
    z -0.055. Nothing of the seat stands in those. (Everything below is said in its piece's own
    frame, as the data places it: a piece moved there takes its geometry with it, and what it
    clears has to be looked at again.)"""
    base, cojin, resp, cab = m['base'], m['cojin'], m['respaldo'], m['cabeza']
    # ---- the base: rails, a carriage, the column with its bellows, the plate under the pan
    for x in (-0.17, 0.17):
        m.caja((0.045, 0.028, 0.46), en=(x, -0.166, 0), mat='acero', pieza=base, marco=base, bisel=0.004)
        m.tornillos([(x, -0.152, z) for z in (-0.19, -0.06, 0.06, 0.19)], 0.007, 0.004, 'acero_oscuro', base, base)
    m.caja((0.4, 0.05, 0.36), en=(0, -0.125, 0), mat=TINTE, pieza=base, marco=base, bisel=0.012, seg=3)
    m.cilindro(0.085, 0.07, en=(0, -0.07, 0), mat=TINTE, pieza=base, marco=base, lados=24, bisel=0.008)
    # (the bellows of its damper: a stack of rubber rings)
    perfil = [(0.05, -0.035)]
    for k in range(5):
        y = -0.035 + 0.032 * k
        perfil += [(0.068, y + 0.008), (0.05, y + 0.016), (0.068, y + 0.024), (0.05, y + 0.032)]
    m.torno(perfil, en=(0, 0.0, 0), mat='goma', pieza=base, marco=base, lados=20, liso=70)
    m.caja((0.42, 0.03, 0.42), en=(0, 0.165, 0), mat=TINTE, pieza=base, marco=base, bisel=0.01, seg=2)
    m.caja((0.3, 0.05, 0.3), en=(0, 0.13, 0), mat='acero_oscuro', pieza=base, marco=base, bisel=0.01)
    # its height lever, under the pan's port side
    m.tubo([(0.2, 0.13, 0.05), (0.26, 0.13, 0.08), (0.26, 0.12, 0.2)], 0.008, 'acero', base, base, codo=0.03)
    m.cilindro(0.013, 0.06, en=(0.26, 0.12, 0.2), mat='goma', pieza=base, marco=base, eje='z', lados=10, bisel=0.004)

    # ---- the pan: a tray with a cheek down each side, the whole length of the seat
    m.caja((0.6, 0.022, 0.83), en=(0, -0.039, 0), mat='plastico_negro', pieza=cojin, marco=cojin, bisel=0.008)
    mejilla = [(-0.417, -0.052), (0.397, -0.052), (0.417, -0.037), (0.417, -0.01), (0.325, 0.015), (-0.275, 0.015), (-0.335, 0.04), (-0.417, 0.04)]
    for x in (-0.291, 0.291):
        m.extrusion(mejilla, 0.018, en=(x, 0, 0), mat='plastico_negro', pieza=cojin, marco=cojin, eje='x', bisel=0.004, seg=2)
    # the pad under the thighs (its top where the piece's is), a bolster each side kept outside
    # the thighs, a roll under the knees, two seams across
    m.caja((0.5, 0.08, 0.47), en=(0, 0.01, 0.14), mat=TINTE, pieza=cojin, marco=cojin, bisel=0.03, seg=4)
    for x in (-0.262, 0.262):
        m.capsula(0.04, 0.47, en=(x, 0.02, 0.145), mat=TINTE, pieza=cojin, marco=cojin, eje='z', lados=14, fondo=0.9)
    m.capsula(0.042, 0.5, en=(0, 0.008, 0.373), mat=TINTE, pieza=cojin, marco=cojin, eje='x', lados=14, fondo=0.9)
    for z in (0.07, 0.21):
        m.caja((0.44, 0.004, 0.012), en=(0, 0.0505, z), mat='tela_negra', pieza=cojin, marco=cojin, bisel=0.001, seg=1)
    # behind the pad, the shelf the pack hangs over: hard, 4 cm lower, two rubber rest strips
    m.caja((0.57, 0.042, 0.323), en=(0, -0.009, -0.2515), mat='plastico_negro', pieza=cojin, marco=cojin, bisel=0.01, seg=2)
    for x in (-0.14, 0.14):
        m.caja((0.06, 0.014, 0.24), en=(x, 0.018, -0.25), mat='goma', pieza=cojin, marco=cojin, bisel=0.005, seg=2)
        m.tornillos([(x, 0.025, z) for z in (-0.35, -0.15)], 0.005, 0.002, 'acero', cojin, cojin)
    # what carries the cradle: the hinge tube across the back (through the wings' feet), a web
    # each side from the column's plate out under the shelf
    m.cilindro(0.018, 0.652, en=(0, 0.03, -0.41), mat='acero_oscuro', pieza=cojin, marco=cojin, eje='x', lados=14, bisel=0.004)
    for x in (-0.12, 0.12):
        m.extrusion([(-0.025, -0.05), (-0.395, -0.05), (-0.395, -0.07), (-0.025, -0.12)], 0.012, en=(x, 0, 0), mat='acero_oscuro', pieza=cojin, marco=cojin, eje='x', bisel=0.003)
    for s in (-1, 1):
        m.tornillos([(s * 0.326, 0.03, -0.41)], 0.009, 0.004, 'acero', cojin, cojin, eje=(s, 0, 0))
        # the lap belt: its reel on the pan's side, the tongue standing out of it
        m.caja((0.03, 0.06, 0.07), en=(s * 0.317, -0.005, -0.145), mat='plastico_negro', pieza=cojin, marco=cojin, bisel=0.008)
        m.tornillos([(s * 0.332, -0.005, -0.145)], 0.01, 0.004, 'acero', cojin, cojin, eje=(s, 0, 0))
        m.caja((0.004, 0.03, 0.04), en=(s * 0.317, 0.035, -0.145), mat='tela_gris', pieza=cojin, marco=cojin, bisel=0.001, seg=1)
        m.caja((0.005, 0.045, 0.034), en=(s * 0.317, 0.065, -0.145), mat='acero', pieza=cojin, marco=cojin, bisel=0.002, seg=1)
        # the foot of the armrest's bracket, bolted to the cheek
        m.extrusion([(-0.045, -0.035), (0.155, -0.035), (0.155, 0.02), (0.115, 0.09), (-0.018, 0.09), (-0.045, 0.02)], 0.012, en=(s * 0.312, 0, 0), mat='acero_oscuro', pieza=cojin, marco=cojin, eje='x', bisel=0.003)
        m.caja((0.008, 0.045, 0.18), en=(s * 0.303, -0.008, 0.055), mat='acero_oscuro', pieza=cojin, marco=cojin, bisel=0.002, seg=1)
        m.tornillos([(s * 0.318, -0.008, z) for z in (0.0, 0.11)], 0.007, 0.004, 'acero', cojin, cojin, eje=(s, 0, 0))
    # the crotch strap down the pan's front, between the calves, with the harness's buckle
    m.caja((0.04, 0.075, 0.005), en=(0, -0.025, 0.4175), mat='tela_gris', pieza=cojin, marco=cojin, bisel=0.001, seg=1)
    m.cilindro(0.028, 0.014, en=(0, -0.04, 0.427), mat='acero', pieza=cojin, marco=cojin, eje='z', lados=16, bisel=0.004)
    m.cilindro(0.013, 0.006, en=(0, -0.04, 0.436), mat='pintura_roja', pieza=cojin, marco=cojin, eje='z', lados=12, bisel=0.002)

    # ---- the cradle: the plate behind the pack, its back sunk between a frame of two rails
    placa = m.caja((0.616, 0.99, 0.03), en=(0, 0, 0.014), mat='plastico_negro', pieza=resp, marco=resp, bisel=0.012, seg=3)
    hundir(placa, (0, 0, -1), 0.05, 0.006, marco=resp)
    for x in (-0.2, 0.2):
        m.caja((0.045, 0.94, 0.038), en=(x, 0, -0.012), mat='acero_oscuro', pieza=resp, marco=resp, bisel=0.005)
        m.tornillos([(x, y, -0.031) for y in (-0.42, -0.2, 0.2, 0.42)], 0.007, 0.004, 'acero', resp, resp, eje=(0, 0, -1))
    for y in (-0.36, 0.0, 0.36):
        m.caja((0.36, 0.04, 0.03), en=(0, y, -0.008), mat='acero_oscuro', pieza=resp, marco=resp, bisel=0.004)
    # (ribs pressed in the shell between them; a grab handle over the upper cross-member, no
    # farther back than the piece: a seat may stand with its back to a wall)
    for y in (-0.18, 0.18):
        for x in (-0.09, 0.0, 0.09):
            m.caja((0.032, 0.24, 0.014), en=(x, y, 0.0), mat='plastico_negro', pieza=resp, marco=resp, bisel=0.006, seg=1)
    m.asa((-0.1, 0.42, 0.003), (0.1, 0.42, 0.003), (0, 0, -1), alto=0.04, r=0.008, mat='acero', pieza=resp, marco=resp)
    # what the pack rests against: a pad each side of the rail it locks on (their faces half a
    # centimetre behind the pack's back)
    for x in (-0.168, 0.168):
        m.caja((0.195, 0.8, 0.029), en=(x, 0.02, 0.0435), mat=TINTE, pieza=resp, marco=resp, bisel=0.012, seg=3)
        for y in (-0.18, 0.02, 0.22):
            m.caja((0.175, 0.004, 0.006), en=(x, y, 0.057), mat='tela_negra', pieza=resp, marco=resp, bisel=0.001, seg=1)
    m.caja((0.05, 0.74, 0.012), en=(0, 0.02, 0.034), mat='acero_oscuro', pieza=resp, marco=resp, bisel=0.003)
    m.tornillos([(0, y, 0.04) for y in (-0.3, -0.14, 0.18, 0.34)], 0.006, 0.003, 'acero', resp, resp, eje=(0, 0, 1))
    m.caja((0.07, 0.06, 0.018), en=(0, 0.02, 0.046), mat='acero', pieza=resp, marco=resp, bisel=0.004, seg=1)
    m.caja((0.03, 0.02, 0.005), en=(0, 0.02, 0.0565), mat='pintura_amarilla', pieza=resp, marco=resp, bisel=0.0015, seg=1)
    # the ledge under the pack, on two gussets, with its rubber
    m.caja((0.42, 0.012, 0.086), en=(0, -0.438, 0.072), mat='acero_oscuro', pieza=resp, marco=resp, bisel=0.004)
    m.caja((0.38, 0.005, 0.06), en=(0, -0.4305, 0.078), mat='goma', pieza=resp, marco=resp, bisel=0.0015, seg=1)
    for x in (-0.15, 0.15):
        m.extrusion([(0.029, -0.444), (0.105, -0.444), (0.029, -0.49)], 0.01, en=(x, 0, 0), mat='acero_oscuro', pieza=resp, marco=resp, eje='x', bisel=0.002)
    ala = [(-0.005, -0.55), (0.145, -0.55), (0.155, -0.54), (0.155, -0.5), (0.145, -0.45), (0.145, 0.33), (0.105, 0.45), (-0.005, 0.495)]
    for s in (-1, 1):
        # the latch over the pack's top: a claw on its block, its lever painted
        m.caja((0.06, 0.03, 0.03), en=(s * 0.225, 0.476, 0.043), mat='acero_oscuro', pieza=resp, marco=resp, bisel=0.005, seg=1)
        m.caja((0.045, 0.012, 0.075), en=(s * 0.225, 0.476, 0.09), mat='acero', pieza=resp, marco=resp, bisel=0.003, seg=1)
        m.caja((0.028, 0.01, 0.05), en=(s * 0.225, 0.487, 0.085), mat='pintura_amarilla', pieza=resp, marco=resp, bisel=0.003, seg=1)
        # the wing that holds the pack sideways: a plate from the pan's cheek to the top, bolted
        # to the plate's edge; a thin pad on its inner face and a roll round its front edge (a
        # centimetre off the pack's side)
        m.extrusion(ala, 0.012, en=(s * 0.314, 0, 0), mat='plastico_negro', pieza=resp, marco=resp, eje='x', bisel=0.004, seg=2)
        m.caja((0.014, 0.7, 0.085), en=(s * 0.302, -0.05, 0.0925), mat=TINTE, pieza=resp, marco=resp, bisel=0.006, seg=2)
        m.capsula(0.012, 0.74, en=(s * 0.3075, -0.05, 0.137), mat=TINTE, pieza=resp, marco=resp, eje='y', lados=8, fondo=0.9)
        m.tornillos([(s * 0.32, y, 0.014) for y in (-0.36, -0.12, 0.14, 0.4)], 0.006, 0.003, 'acero', resp, resp, eje=(s, 0, 0))
        m.caja((0.008, 0.045, 0.08), en=(s * 0.304, -0.525, 0.07), mat='acero_oscuro', pieza=resp, marco=resp, bisel=0.002, seg=1)
        m.tornillos([(s * 0.32, -0.525, z) for z in (0.055, 0.095)], 0.007, 0.004, 'acero', resp, resp, eje=(s, 0, 0))
        # the shoulder strap, stowed: its reel on the wing's outer face, the strap down the wing
        # to its tongue in a clip
        m.caja((0.024, 0.08, 0.07), en=(s * 0.332, 0.4, 0.085), mat='plastico_negro', pieza=resp, marco=resp, bisel=0.008)
        m.tornillos([(s * 0.344, 0.4, 0.085)], 0.01, 0.004, 'acero', resp, resp, eje=(s, 0, 0))
        m.caja((0.004, 0.57, 0.045), en=(s * 0.3225, 0.08, 0.085), mat='tela_gris', pieza=resp, marco=resp, bisel=0.001, seg=1)
        m.caja((0.007, 0.06, 0.05), en=(s * 0.3238, -0.23, 0.085), mat='acero', pieza=resp, marco=resp, bisel=0.002, seg=1)
        m.caja((0.012, 0.018, 0.064), en=(s * 0.326, -0.262, 0.085), mat='acero_oscuro', pieza=resp, marco=resp, bisel=0.003, seg=1)

    # ---- the helmet rest: the top of the cradle, an arm each side over the pack with a plate
    # between them, the pad on its carrier at their end (a wing each side), behind the helmet
    m.caja((0.29, 0.014, 0.2), en=(0, -0.047, -0.02), mat='plastico_negro', pieza=cab, marco=cab, bisel=0.005)
    m.caja((0.3, 0.1, 0.02), en=(0, 0.004, 0.065), mat='plastico_negro', pieza=cab, marco=cab, bisel=0.008)
    m.caja((0.28, 0.1, 0.066), en=(0, 0.004, 0.106), mat=TINTE, pieza=cab, marco=cab, bisel=0.028, seg=4)
    m.caja((0.22, 0.004, 0.008), en=(0, 0.004, 0.139), mat='tela_negra', pieza=cab, marco=cab, bisel=0.001, seg=1)
    for s in (-1, 1):
        m.caja((0.03, 0.026, 0.215), en=(s * 0.13, -0.042, -0.0275), mat='acero_oscuro', pieza=cab, marco=cab, bisel=0.006)
        m.extrusion([(-0.12, -0.032), (0.056, -0.032), (0.056, 0.04), (0.03, 0.04)], 0.01, en=(s * 0.13, 0, 0), mat='acero_oscuro', pieza=cab, marco=cab, eje='x', bisel=0.002)
        m.caja((0.05, 0.09, 0.055), en=(s * 0.15, 0.004, 0.106), mat=TINTE, pieza=cab, marco=cab, bisel=0.02, seg=3, rot=(0, s * -24, 0))
        # (the arm's foot, bolted down the back of the plate)
        m.caja((0.036, 0.06, 0.012), en=(s * 0.13, -0.075, -0.142), mat='acero_oscuro', pieza=cab, marco=cab, bisel=0.003, seg=1)
        m.tornillos([(s * 0.13, -0.085, -0.148)], 0.006, 0.003, 'acero', cab, cab, eje=(0, 0, -1))

    # ---- the armrests (outside the thighs, ahead of the pack's corners): a pad on its frame
    # at the top of the piece, the upper half of the bracket that comes up from the pan's side
    # (the piece takes it in, down to the pan's: an armrest holds to its own seat) and its pivot
    for id, s in (('brazo_i', 1), ('brazo_d', -1)):
        b = m[id]
        m.caja((0.06, 0.035, 0.36), en=(0, 0.072, 0), mat=TINTE, pieza=b, marco=b, bisel=0.014, seg=3)
        m.caja((0.036, 0.02, 0.3), en=(0, 0.046, -0.02), mat='acero_oscuro', pieza=b, marco=b, bisel=0.004)
        m.extrusion([(-0.153, -0.06), (-0.02, -0.06), (-0.045, -0.015), (-0.045, 0.038), (-0.135, 0.038), (-0.135, -0.015)], 0.012, en=(s * 0.002, 0, 0), mat='acero_oscuro', pieza=b, marco=b, eje='x', bisel=0.003)
        m.cilindro(0.015, 0.03, en=(s * 0.004, 0.005, -0.09), mat='acero', pieza=b, marco=b, eje='x', lados=12, bisel=0.003)


@receta('consola_puente')
def consola_puente(m):
    """The flight console: a deep base with its kick recess, vents and a foot rail; the panel's
    face left flat for its instruments; a padded glare shield over it."""
    cuerpo, cara, visera = m['cuerpo'], m['cara'], m['visera']
    sx, sy, sz = cuerpo.tam
    ob = m.caja((sx, sy, sz), mat=TINTE, pieza=cuerpo, marco=cuerpo, bisel=0.02, seg=3)
    # toward the seats (aft): a recess for the knees, dark, with a vent each side
    hundir(ob, (0, 0, -1), 0.07, 0.035, mat='pintura_oscura', marco=cuerpo)
    for x in (-0.85, 0.85):
        m.rejilla((0.42, 0.2), en=(x, -0.12, -sz / 2 + 0.03), pieza=cuerpo, marco=cuerpo, lamas=7, normal=(0, 0, -1))
        m.caja((0.46, 0.24, 0.012), en=(x, -0.12, -sz / 2 + 0.036), mat='acero_oscuro', pieza=cuerpo, marco=cuerpo, bisel=0.004)
    m.tubo([(-sx / 2 + 0.12, -sy / 2 + 0.1, -sz / 2 - 0.05), (sx / 2 - 0.12, -sy / 2 + 0.1, -sz / 2 - 0.05)], 0.016, 'acero', cuerpo, cuerpo, lados=12)
    for x in (-sx / 2 + 0.12, 0.42, -0.42, sx / 2 - 0.12):
        m.caja((0.03, 0.05, 0.07), en=(x, -sy / 2 + 0.1, -sz / 2 - 0.018), mat='acero_oscuro', pieza=cuerpo, marco=cuerpo, bisel=0.006)
    # its ends: an access panel each
    for lado in (-1, 1):
        hundir(ob, (lado, 0, 0), 0.05, 0.008, marco=cuerpo)
    m.pernos(cuerpo, 'x', 0.07, (2, 2), 0.007, lado=1)
    m.pernos(cuerpo, 'x', 0.07, (2, 2), 0.007, lado=-1)
    m.caja(tuple(cara.tam), mat=TINTE, pieza=cara, marco=cara, bisel=0.006, seg=2)
    # the glare shield: a board with a padded roll along the edge toward the eyes
    vx, vy, vz = visera.tam
    m.caja((vx, vy * 0.7, vz), mat=TINTE, pieza=visera, marco=visera, bisel=0.008, seg=2)
    m.capsula(0.03, vx, en=(0, 0, -vz / 2 + 0.01), mat='goma', pieza=visera, marco=visera, eje='x', lados=14, fondo=0.8)


@receta('pedestal')
def pedestal(m):
    cuerpo, cara = m['cuerpo'], m['cara']
    sx, sy, sz = cuerpo.tam
    ob = m.caja((sx, sy, sz), mat=TINTE, pieza=cuerpo, marco=cuerpo, bisel=0.018, seg=3)
    for lado in (-1, 1):
        hundir(ob, (lado, 0, 0), 0.06, 0.01, marco=cuerpo)
    m.pernos(cuerpo, 'x', 0.035, (2, 4), 0.006, lado=1)
    m.pernos(cuerpo, 'x', 0.035, (2, 4), 0.006, lado=-1)
    # a rubbing strip round its foot
    m.caja((sx + 0.012, 0.05, sz + 0.012), en=(0, -sy / 2 + 0.03, 0), mat='goma', pieza=cuerpo, marco=cuerpo, bisel=0.008)
    m.caja(tuple(cara.tam), mat=TINTE, pieza=cara, marco=cara, bisel=0.006, seg=2)


def placa_de_techo(m, p):
    """An overhead panel's plate: flat faces, a grab handle at each end."""
    sx, sy, sz = p.tam
    m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.008, seg=2)
    for lado in (-1, 1):
        m.asa((lado * sx / 2, 0, -sz * 0.28), (lado * sx / 2, 0, sz * 0.28), (lado, 0, 0), alto=0.035, r=0.008, mat='acero', pieza=p, marco=p)


@receta('panel_techo')
def panel_techo(m):
    placa_de_techo(m, m[''])


@receta('consola_techo')
def consola_techo(m):
    placa_de_techo(m, m[''])
    b = m['brazo']
    m.cilindro(0.028, b.tam.y, mat=TINTE, pieza=b, marco=b, lados=16, bisel=0.004)
    m.brida(0.044, en=(0, b.tam.y / 2 - 0.006, 0), grosor=0.012, pernos=4, mat='acero_oscuro', pieza=b, marco=b, r_int=0.026)
    m.caja((0.07, 0.03, 0.07), en=(0, -b.tam.y / 2 + 0.015, 0), mat='acero_oscuro', pieza=b, marco=b, bisel=0.008)


@receta('consola_tug')
def consola_tug(m):
    """The tug's console: a column bolted to the deck with its cable run up the back, the
    panel's face on top."""
    pie, cara = m['pie'], m['cara']
    sx, sy, sz = pie.tam
    ob = m.caja((sx * 0.8, sy, sz * 0.8), mat=TINTE, pieza=pie, marco=pie, bisel=0.02, seg=3)
    hundir(ob, (0, 0, -1), 0.04, 0.008, marco=pie)
    m.caja((sx + 0.06, 0.02, sz + 0.08), en=(0, -sy / 2 + 0.01, 0), mat='acero_oscuro', pieza=pie, marco=pie, bisel=0.005)
    m.tornillos([(x * (sx / 2 + 0.01), -sy / 2 + 0.02, z * (sz / 2 + 0.02)) for x in (-1, 1) for z in (-1, 1)], 0.009, 0.006, 'acero', pie, pie)
    m.tubo([(0.06, -sy / 2 + 0.03, sz * 0.4 + 0.02), (0.06, sy / 2 - 0.12, sz * 0.4 + 0.02), (0.06, sy / 2 - 0.05, sz * 0.2)], 0.014, 'goma', pie, pie, codo=0.04)
    m.caja(tuple(cara.tam), mat=TINTE, pieza=cara, marco=cara, bisel=0.006, seg=2)


@receta('armario')
def armario(m):
    """A locker: its door sunk in its front with louvres top and bottom and three hinges, a bar
    handle, feet."""
    c, asa = m['cuerpo'], m['asa']
    sx, sy, sz = c.tam
    ob = m.caja((sx, sy - 0.04, sz), en=(0, 0.02, 0), mat=TINTE, pieza=c, marco=c, bisel=0.012, seg=3)
    hundir(ob, (-1, 0, 0), 0.03, 0.01, marco=c)
    for y in (-0.68, 0.72):
        m.rejilla((0.34, 0.12), en=(-sx / 2 + 0.012, y, 0), pieza=c, marco=c, lamas=6, normal=(-1, 0, 0), mat='acero_oscuro')
    for y in (-0.7, 0.0, 0.7):
        m.cilindro(0.011, 0.09, en=(-sx / 2 - 0.002, y, -sz / 2 + 0.035), mat='acero', pieza=c, marco=c, lados=10, bisel=0.002)
    # a card holder at eye height
    m.caja((0.006, 0.07, 0.12), en=(-sx / 2 + 0.008, 0.45, 0), mat='acero', pieza=c, marco=c, bisel=0.002)
    for x in (-1, 1):
        for z in (-1, 1):
            m.caja((0.06, 0.04, 0.06), en=(x * (sx / 2 - 0.05), -sy / 2 + 0.02, z * (sz / 2 - 0.05)), mat='goma', pieza=c, marco=c, bisel=0.008)
    m.asa((0.005, -0.1, 0), (0.005, 0.1, 0), (-1, 0, 0), alto=0.03, r=0.007, mat=TINTE, pieza=asa, marco=asa)


def caja_de_mando(m, glands):
    """A wall box: a lid with a hinge down one side and a latch on the other (its faces flat for
    a panel), cable glands under it."""
    p = m['']
    sx, sy, sz = p.tam
    m.caja((sx, sy, sz), mat=TINTE, pieza=p, marco=p, bisel=0.01, seg=3)
    for k in range(glands):
        z = (k - (glands - 1) / 2) * sz / (glands + 0.5)
        m.cilindro(0.017, 0.03, en=(0, -sy / 2 - 0.012, z), mat='plastico_negro', pieza=p, marco=p, lados=8, bisel=0.003)
        m.cilindro(0.011, 0.02, en=(0, -sy / 2 - 0.034, z), mat='goma', pieza=p, marco=p, lados=10, bisel=0.002)
    for y in (-sy * 0.3, sy * 0.3):
        m.cilindro(0.008, sy * 0.14, en=(0, y, -sz / 2 - 0.004), mat='acero', pieza=p, marco=p, lados=10, bisel=0.002)
    m.caja((sx * 0.5, 0.05, 0.012), en=(0, 0, sz / 2 + 0.004), mat='acero', pieza=p, marco=p, bisel=0.003)


@receta('caja_pared')
def caja_pared(m):
    caja_de_mando(m, 3)


@receta('caja_pequena')
def caja_pequena(m):
    caja_de_mando(m, 2)


@receta('baranda')
def baranda(m):
    p = m['']
    m.capsula(p.r, p.h, mat=TINTE, pieza=p, marco=p, lados=14, fondo=0.9)
    for y in (-p.h * 0.42, p.h * 0.42):
        m.torno([(p.r, -0.02), (p.r + 0.006, -0.014), (p.r + 0.006, 0.014), (p.r, 0.02)], en=(0, y, 0), mat='acero', pieza=p, marco=p, lados=14)


@receta('anclaje_carga')
def anclaje_carga(m):
    """A cargo clamp: a length of seat track let into the deck, a shoe at each end with its jaw
    and pin, and the lever that throws them."""
    riel, palanca = m[''], m['palanca']
    sx, sy, sz = riel.tam
    # the track: a plate with a raised lip each side and its row of stud holes
    m.caja((sx, sy, sz), mat=TINTE, pieza=riel, marco=riel, bisel=0.003, seg=1)
    for z in (-sz / 2 + 0.014, sz / 2 - 0.014):
        m.caja((sx - 0.16, 0.012, 0.02), en=(0, sy / 2 + 0.004, z), mat=TINTE, pieza=riel, marco=riel, bisel=0.004)
    n = 17
    m.tornillos([(-0.46 + 0.92 * k / (n - 1), sy / 2, 0) for k in range(n)], 0.015, 0.0015, 'acero_oscuro', riel, riel, lados=12)
    for id, lado in (('cierre_i', 1), ('cierre_d', -1)):
        c = m[id]
        cx, cy, cz = c.tam
        m.caja((cx, cy * 0.62, cz), en=(0, -cy * 0.19, 0), mat=TINTE, pieza=c, marco=c, bisel=0.012, seg=3)
        # the jaw: a hook leaning in over the load's foot
        m.extrusion([(-cx / 2, cy * 0.1), (cx / 2, cy * 0.1), (cx / 2, cy * 0.5), (-lado * cx * 0.1, cy * 0.5), (-lado * cx * 0.5 - lado * 0.02, cy * 0.3)] if lado > 0 else [(-cx / 2, cy * 0.1), (cx / 2, cy * 0.1), (cx * 0.5 + 0.02, cy * 0.3), (cx * 0.1, cy * 0.5), (-cx / 2, cy * 0.5)], cz * 0.7, mat=TINTE, pieza=c, marco=c, eje='z', bisel=0.006, seg=2)
        m.cilindro(0.012, cz + 0.016, en=(0, -cy * 0.1, 0), mat='acero', pieza=c, marco=c, eje='z', lados=12, bisel=0.003)
    px, py, pz = palanca.tam
    m.cilindro(0.008, py * 0.86, en=(0, -0.01, 0), mat='acero', pieza=palanca, marco=palanca, lados=10, bisel=0.002)
    m.capsula(0.016, 0.07, en=(0, py / 2 - 0.035, 0), mat=TINTE, pieza=palanca, marco=palanca, lados=14, fondo=0.9)
    m.cilindro(0.016, 0.03, en=(0, -py / 2 + 0.01, 0), mat='acero_oscuro', pieza=palanca, marco=palanca, eje='z', lados=12, bisel=0.003)
