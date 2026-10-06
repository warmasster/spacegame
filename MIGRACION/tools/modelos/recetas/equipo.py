"""Hand gear: what the suit carries besides its welder and its launcher — a datapad, a clamp
meter, a camera, a marker beacon, the computer on its wrist.

Each is built as a tool is (`herramientas.py`): in the frame it is held in — x to the holder's
left, y up, z ahead, metres — with its own colours, its body the piece without a name and what
the game moves on it a piece of its own, built where it rests, its origin its pivot. What the
game needs of each (where each piece's pivot is and how far it goes, where a hand closes on it,
where its screen is) the recipe SAYS (`m.mueve`, `m.punto`, `m.pantalla`, `m.luz`): it is
written to `tools/modelos/datos/<name>.json` with the model, and `comprobar.py` holds the model
to it.

Everything a glove touches is made for a pressurised one (the suit's is 115 mm across the palm,
its fingers 28 mm thick): grips 44 mm across and 115 mm long at the least, a trigger guard a
gloved finger goes into, keys 30 mm across and as far apart, nothing sharp near them.
"""
import math

from mathutils import Vector

from kit import Marco, receta
from recetas.estilos import banda, rect_redondo, superelipse
from recetas.herramientas import arco, casco, chapa, empunadura, redondear, tramo

# how thick what a hand closes on is (the rig's `mango`, `crates/app/src/rig.rs`)
MANGO = 0.044


# ---------------------------------------------------------------------------------------------
# what several of them have


def pistola(m, p, gatillo, arriba, color='pintura_roja', guarda='acero_oscuro', recorre=14.0, fondo=0.05, punto='agarre', pieza_gatillo='gatillo'):
    """A pistol grip with its trigger, as the welder has them: the grip from `arriba` (the
    middle of its top, where it leaves the body) raked back 15 degrees, 115 mm long; the trigger
    a blade on a pin 42 mm ahead of it (the piece `gatillo`, its origin that pin), in a guard a
    gloved finger goes into. Says the trigger's travel and the grip's point; gives back
    (the grip's butt, the pin)."""
    a = Vector(arriba)
    abajo = a + Vector((0.0, -0.1111, -0.0298))
    d = empunadura(m, p, a, abajo, fondo=fondo)
    pin = a + Vector((0.0, 0.004, 0.042))
    # the guard: from the body ahead of the trigger down, back under the finger and up to the grip
    z0, y0 = a.z, a.y
    chapa(m, p, redondear([(z0 + 0.086, y0 + 0.013), (z0 + 0.086, y0 - 0.0405), (z0 + 0.018, y0 - 0.0405), (z0 + 0.008, y0 - 0.032)], 0.013), 0.0035, a.x - 0.009, a.x + 0.009, guarda, plano='zy', hacia=(z0 + 0.23, y0 - 0.27), bisel=0.001)
    # the trigger: a blade on its pin
    m.cilindro(0.0036, 0.011, en=pin, mat='acero', pieza=gatillo, eje='x', lados=12, bisel=0.0007)
    cara = redondear([(pin.z + 0.0022, pin.y + 0.0025), (pin.z + 0.0026, pin.y - 0.012), (pin.z + 0.0042, pin.y - 0.022), (pin.z + 0.0075, pin.y - 0.03), (pin.z + 0.0115, pin.y - 0.0345)], 0.008)
    chapa(m, gatillo, cara, 0.0052, a.x - 0.006, a.x + 0.006, color, plano='zy', hacia=(pin.z - 0.06, pin.y - 0.02), bisel=0.0011)
    m.mueve(pieza_gatillo, eje=(1, 0, 0), giro=recorre, nota='apretado gira sobre +x: su hoja atrás, hacia la empuñadura')
    medio = (a + abajo) / 2
    m.punto(punto, medio, palma=(1, 0, 0.12), traves=d, mano='der', mango=MANGO, nota='mano derecha; el índice en el gatillo (pose "gatillo") o fuera ("dedo_fuera")')
    return abajo, pin


def mango(m, p, a, b, u, ancho=MANGO, fondo=0.05, entra=0.012, mat='goma', n=20, costillas=None, marco=None):
    """A rubber sleeve a glove closes on, from `a` (where it leaves what carries it: a hard
    collar there, running `entra` into it) to `b` (a flared stop): `ancho` across along `u`,
    `fondo` the other way, ribbed. It may run any way (`empunadura` cannot run along x)."""
    a, b = Vector(a), Vector(b)
    largo = (b - a).length
    d = (b - a) / largo
    u = Vector(u)
    u = (u - d * u.dot(d)).normalized()
    w = d.cross(u)

    def anillo(s, k):
        c = a + d * s
        return [tuple(c + u * (x * k) + w * (z * k)) for x, z in superelipse(ancho / 2, fondo / 2, n, 2.5)]

    pasos = costillas * 4 if costillas else max(8, int(round(largo / 0.0032 / 4)) * 4)
    est = [(-entra, 1.0), (0.0, 1.0)] if entra else [(0.0, 1.0)]
    for k in range(1, pasos):
        t = k / pasos
        s = 1.0 + 0.045 * math.sin(t * math.pi)
        if 0.1 < t < 0.9 and k % 4 in (2, 3):
            s -= 0.035
        est.append((largo * t, s))
    est.append((largo, 1.0))
    m.piel([anillo(s, k) for s, k in est], mat=mat, pieza=p, marco=marco, liso=60.0)
    m.piel([anillo(s, k) for s, k in ((-entra, 1.05), (0.005, 1.05), (0.009, 0.99))], mat='plastico_negro', pieza=p, marco=marco, liso=50.0)
    m.piel([anillo(s, k) for s, k in ((largo - 0.004, 0.99), (largo - 0.001, 1.08), (largo + 0.005, 1.08), (largo + 0.008, 1.0))], mat='plastico_negro', pieza=p, marco=marco, liso=50.0)
    return d


def tecla(m, pieza, tam, en, mat='plastico_gris', normal=(0, 0, -1), bisel=0.002):
    """A chunky key: a block `tam` (wide, tall, deep) whose face — its middle at `en` — looks
    `normal` (-z: toward the holder); it is a piece of its own, its origin the middle of its
    face."""
    n = Vector(normal)
    return m.caja(tam, en=Vector(en) - n * (tam[2] / 2), mat=mat, pieza=pieza, bisel=bisel, seg=2)


def anilla(m, p, en, r=0.011, grosor=0.0022, eje=(1, 0, 0), mat='acero', arco_de=0.0, arco_a=360.0, lados=16):
    """A ring to tie a tether to, round `eje` at `en`."""
    e = Vector(eje).normalized()
    u = e.orthogonal().normalized()
    w = e.cross(u)
    n = lados
    pts = [Vector(en) + (u * math.cos(math.radians(arco_de + (arco_a - arco_de) * k / n)) + w * math.sin(math.radians(arco_de + (arco_a - arco_de) * k / n))) * r for k in range(n + 1)]
    return m.tubo(pts, grosor, mat, p, None, lados=6, tapas=arco_a - arco_de < 359.0)


def marco_de(centro, normal, arriba):
    """A frame at `centro` whose z looks `normal` and whose y is `arriba` (made square to it):
    what is built flat in it — x across, y up — is seen the right way round from in front."""
    n = Vector(normal).normalized()
    u = Vector(arriba)
    u = (u - n * u.dot(n)).normalized()
    from mathutils import Matrix
    return Marco(centro, Matrix((u.cross(n), u, n)).transposed())


# ---------------------------------------------------------------------------------------------
# the datapad

TB_PANTALLA = (0.180, 0.120)             # its glass: wide, tall; its middle the model's origin
TB_CUERPO = (-0.112, 0.112, -0.096, 0.080)  # its body: x from, to; y from, to
TB_FONDO = 0.027                         # ... and how far back it goes (z)
TB_ASA = Vector((0.172, -0.008, 0.014))  # the middle of its handle (a bar along y)
TB_TECLAS = [(0.052, -0.076), (0.0, -0.076), (-0.052, -0.076)]   # the middle of each key's face (x, y)
TB_TECLA_Z = -0.0068                     # ... and how far toward the holder it stands
TB_TECLA_RECORRE = 0.0025


@receta('tableta')
def tableta(m):
    """A datapad for checklists, made for a suit: a screen in a magnesium body with rubber
    round its corners, a bar handle down its left side for the left hand (a strap over the back
    of that hand), three chunky keys under the screen for the right index finger, a battery door
    and its data plate behind, a ring for its tether. The screen's glass is flat, sunk 3 mm
    behind its bezel: the game draws on it."""
    p = m.pieza('', (0.38, 0.2, 0.08))
    m.presupuesto = 9000
    W, H = TB_PANTALLA
    x0, x1, y0, y1 = TB_CUERPO
    cy = (y0 + y1) / 2
    # ---- the body: its front 2 mm behind the glass, a step round its back
    m.caja((x1 - x0, y1 - y0, TB_FONDO - 0.002), en=(0, cy, 0.002 + (TB_FONDO - 0.002) / 2), mat='pintura_gris', pieza=p, bisel=0.005, seg=3)
    # the glass (2 mm, its face at z = 0) and the bezel round it, 3 mm proud
    m.caja((W, H, 0.002), en=(0, 0, 0.001), mat='pantalla', pieza=p, bisel=0)
    fz, fg = -0.0005, 0.005          # the bezel's middle and its thickness (z from -3 to +2 mm)
    m.caja((x1 - x0 - 0.008, y1 - H / 2 - 0.004, fg), en=(0, (y1 - 0.004 + H / 2) / 2, fz), mat='plastico_negro', pieza=p, bisel=0.0012, seg=1)
    for lado in (1, -1):
        m.caja((x1 - 0.004 - W / 2, H, fg), en=(lado * (x1 - 0.004 + W / 2) / 2, 0, fz), mat='plastico_negro', pieza=p, bisel=0.0012, seg=1)
    chin = m.caja((x1 - x0 - 0.008, -H / 2 - (y0 + 0.004), fg), en=(0, (y0 + 0.004 - H / 2) / 2, fz), mat='plastico_negro', pieza=p, bisel=0.0012, seg=1)
    # ---- the keys, each in its well through the bezel: back, accept, forward
    for k, (x, y) in enumerate(TB_TECLAS):
        m.restar(chin, m.caja((0.0356, 0.0196, 0.02), en=(x, y, 0), mat='plastico_negro', pieza=p, bisel=0))
        b = m.pieza(f'boton_{k + 1}', (0.034, 0.018, 0.006), (x, y, TB_TECLA_Z))
        tecla(m, b, (0.034, 0.018, 0.0058), (x, y, TB_TECLA_Z), mat='pintura_naranja' if k == 1 else 'plastico_gris')
        m.mueve(f'boton_{k + 1}', eje=(0, 0, 1), recorre=TB_TECLA_RECORRE, nota='pulsada entra hacia +z')
        m.punto(f'boton_{k + 1}', (x, y, TB_TECLA_Z), palma=(0, -0.5, 0.866), traves=(-1, 0, 0), pieza=f'boton_{k + 1}', nota='la yema del índice derecho sobre la tecla')
        # (what each says: an arrow, OK, an arrow)
        if k == 1:
            m.texto('OK', (x, y, TB_TECLA_Z), 0.0075, 'plastico_negro', b, normal=(0, 0, -1), sobre=0.0002)
        else:
            s = 1 if k == 0 else -1
            m.lamina([[(-s * 0.006, 0.0), (s * 0.004, 0.0052), (s * 0.004, -0.0052)]], (x, y, TB_TECLA_Z), 'plastico_negro', b, normal=(0, 0, -1), sobre=0.0002)
    # ---- on the bezel: its name, three lamps, the light sensor's window
    m.texto('TABLETA EVA  T-4', (0.083, 0.069, -0.003), 0.0042, 'plastico_claro', p, normal=(0, 0, -1), alinear='izq', sobre=0.0002)
    for k, luz in enumerate(('piloto_verde', 'piloto_ambar', 'acero_oscuro')):
        m.cilindro(0.0026, 0.0012, en=(-0.058 - k * 0.011, 0.069, -0.0034), mat=luz, pieza=p, eje='z', lados=10, bisel=0.0003)
    m.cilindro(0.004, 0.0008, en=(-0.03, 0.069, -0.0032), mat='cristal_oscuro', pieza=p, eje='z', lados=12, bisel=0.0002)
    m.tornillos([(x, y, -0.003) for x in (-0.1, 0.1) for y in (-0.088, 0.071)], 0.0024, 0.0008, 'acero', p, None, eje=(0, 0, -1))
    # ---- rubber: a block round each corner, a ribbed rail along the top and the bottom
    zc = TB_FONDO / 2
    for sx in (1, -1):
        for y in (y1 - 0.012, y0 + 0.012):
            sy = 1 if y > cy else -1
            m.caja((0.038, 0.038, TB_FONDO + 0.012), en=(sx * (x1 - 0.013), y + sy * 0.006, zc - 0.0005), mat='goma', pieza=p, bisel=0.008, seg=3)
    for y in (y1 + 0.0015, y0 - 0.0015):
        m.caja((0.15, 0.007, 0.02), en=(0, y, zc), mat='goma', pieza=p, bisel=0.002, seg=1)
        for k in range(9):
            m.caja((0.005, 0.009, 0.016), en=(-0.06 + k * 0.015, y, zc), mat='goma', pieza=p, bisel=0.0012, seg=1)
    # its right side: two ports under their rubber caps, the tether's ring on its lug
    for y in (0.02, -0.018):
        m.caja((0.004, 0.026, 0.013), en=(x0 - 0.0012, y, zc), mat='goma', pieza=p, bisel=0.0016, seg=1)
        m.caja((0.0052, 0.006, 0.006), en=(x0 - 0.0012, y - 0.016, zc), mat='goma', pieza=p, bisel=0.001, seg=1)
    m.caja((0.012, 0.016, 0.012), en=(x0 - 0.004, -0.062, zc), mat='acero_oscuro', pieza=p, bisel=0.003, seg=2)
    anilla(m, p, (x0 - 0.0165, -0.062, zc), r=0.0105, grosor=0.002, eje=(0, 0, 1))
    # ---- the handle: a bar down the left side on two lugs, a glove's width clear of the body
    arriba, abajo = TB_ASA + Vector((0, 0.061, 0)), TB_ASA + Vector((0, -0.061, 0))
    empunadura(m, p, arriba, abajo, ancho=MANGO, fondo=0.048)
    for y, sy in ((arriba.y + 0.0185, 1), (abajo.y - 0.0155, -1)):
        casco(m, p, [(-0.004, [(x1 - 0.004, y - 0.0065), (TB_ASA.x + 0.027, y - 0.0065), (TB_ASA.x + 0.027, y + 0.0065), (x1 - 0.004, y + 0.0065)]), (0.032, [(x1 - 0.004, y - 0.0065), (TB_ASA.x + 0.027, y - 0.0065), (TB_ASA.x + 0.027, y + 0.0065), (x1 - 0.004, y + 0.0065)])], 'aluminio_anodizado', bisel=0.002)
        m.tornillos([(x, y + sy * 0.0065, 0.014) for x in (x1 + 0.012, TB_ASA.x)], 0.0034, 0.0012, 'acero', p, None, eje=(0, sy, 0))
        # (the strap's keeper on its end)
        m.caja((0.004, 0.011, 0.04), en=(TB_ASA.x + 0.031, y, -0.004), mat='acero', pieza=p, bisel=0.0012, seg=1)
    # the strap over the back of the hand, its buckle
    xs = TB_ASA.x
    cincha = redondear([(xs + 0.03, arriba.y + 0.018), (xs + 0.07, arriba.y + 0.004), (xs + 0.094, TB_ASA.y), (xs + 0.07, abajo.y - 0.002), (xs + 0.03, abajo.y - 0.0155)], 0.03, pasos=6)
    chapa(m, p, cincha, 0.0024, -0.022, 0.012, 'tela_cincha', plano='xy', hacia=(xs + 0.2, TB_ASA.y), bisel=0.0006, seg=1)
    m.caja((0.007, 0.022, 0.04), en=(xs + 0.0795, arriba.y - 0.012, -0.005), mat='acero', pieza=p, bisel=0.0016, seg=1, rot=(0, 0, -34))
    # ---- behind: the battery's door with its two latches, cooling ribs, the data plate
    zb = TB_FONDO
    m.caja((0.124, 0.074, 0.004), en=(-0.03, -0.03, zb + 0.001), mat='plastico_gris', pieza=p, bisel=0.0016, seg=2)
    for x in (-0.074, 0.014):
        m.cilindro(0.008, 0.003, en=(x, -0.03, zb + 0.0042), mat='acero_oscuro', pieza=p, eje='z', lados=14, bisel=0.0008)
        m.caja((0.012, 0.0022, 0.0016), en=(x, -0.03, zb + 0.0062), mat='pintura_naranja', pieza=p, bisel=0.0005, seg=1)
    m.rayas((0.11, 0.008), en=(-0.03, -0.061, zb + 0.003), pieza=p, normal=(0, 0, 1), n=11)
    for k in range(8):
        m.caja((0.15, 0.004, 0.005), en=(-0.012, 0.062 - k * 0.0085, zb + 0.0015), mat='pintura_gris', pieza=p, bisel=0.0012, seg=1)
    m.rotulo('TABLETA EVA T-4', (0.064, -0.012, zb), 0.0055, p, normal=(0, 0, 1), fondo='aluminio', tinta='plastico_negro', ancho=0.07)
    m.texto('N/S 0417 · 28 V', (0.064, -0.024, zb), 0.0036, 'plastico_claro', p, normal=(0, 0, 1), sobre=0.0002)
    m.texto('BATERÍA', (-0.03, -0.012, zb + 0.003), 0.0048, 'plastico_negro', p, normal=(0, 0, 1), sobre=0.0002)
    m.aviso((0.064, -0.05, zb), 0.02, p, normal=(0, 0, 1))
    m.tornillos([(x, y, zb) for x in (-0.1, 0.1) for y in (-0.084, 0.068)], 0.003, 0.001, 'acero', p, None, eje=(0, 0, 1))
    # ---- what the game needs of it
    m.pantalla('pantalla', (0, 0, 0), (W, H), normal=(0, 0, -1), arriba=(0, 1, 0), nota='cristal plano hundido 3 mm tras el marco; el juego dibuja sobre él')
    m.punto('agarre_izq', TB_ASA, palma=(-1, 0, 0), traves=(0, -1, 0), mano='izq', mango=MANGO, nota='mano izquierda en el asa, pulgar arriba, bajo la cincha')
    m.punto('toque', (-0.03, 0.0, 0.0), palma=(0, -0.5, 0.866), traves=(-1, 0, 0), nota='donde el índice derecho toca el cristal (cualquier punto de la pantalla vale)')
    m.sosten = {'en': [0.03, -0.2, 0.4], 'giro': [-24, 0, 0], 'nota': 'sugerido: ante el pecho, la pantalla vuelta al ojo'}
    m.datos['vistas'] = {'teclas': dict(desde=(-0.2, -0.25, -1), mira=(0, -0.07, 0), dist=0.2, lente=50), 'asa': dict(desde=(0.8, 0.3, -0.6), mira=(0.17, 0, 0.01), dist=0.42, lente=50), 'pulsada': dict(desde=(-0.4, -0.5, -0.8), mira=(0, -0.07, 0), dist=0.2, lente=50, pose={'boton_1': 1.0, 'boton_2': 1.0})}


# ---------------------------------------------------------------------------------------------
# the computer on the wrist

OM_RADIO = 0.0725                  # the strap's inner radius: the suit's sleeve there, and a little
OM_CORREA = (0.046, 0.003)         # the strap: wide (along the arm), thick
OM_CAJA = (0.104, 0.118)           # the block: across the arm (x), along it (z)
OM_CRISTAL = 0.0895                # how far from the arm's axis its glass is
OM_PANTALLA = (0.094, 0.068)       # the glass: wide (along the arm), tall (across it)
OM_PANTALLA_Z = 0.006              # ... and where its middle is along the arm
# where it goes on the suit (`tools/modelos/astronauta`, bone `forearm.L`): this far along the
# bone from the elbow to the wrist (its block ends where the sleeve widens into the glove's
# cuff, 0.62 of the way), its glass looking the way the suit faces at rest
OM_HUESO = ('forearm.L', 0.40)


@receta('ordenador_muneca')
def ordenador_muneca(m):
    """The computer on the suit's left forearm: a slim block with a flat screen, on a strap
    that goes round the sleeve. Built round the arm: its origin on the arm's axis, z along the
    arm toward the hand, y out through the glass, x across (to the far side of whoever reads it
    with the arm across the chest: the screen's up)."""
    p = m.pieza('', (0.16, 0.17, 0.13))
    m.presupuesto = 5000
    m.marco_dicho = 'origen en el eje del antebrazo; z a lo largo del antebrazo hacia la mano, y hacia fuera por el cristal, x = y × z (el "arriba" de la pantalla); metros de mundo (no del modelo del astronauta: su escala es 1,057)'
    R = OM_RADIO
    hx, hz = OM_CAJA[0] / 2, OM_CAJA[1] / 2
    cima = OM_CRISTAL - 0.002
    # ---- the block: its underside the sleeve's round, its top flat
    f0 = math.asin(hx / R)
    arco_bajo = [(R * math.sin(f0 - 2 * f0 * k / 10), R * math.cos(f0 - 2 * f0 * k / 10)) for k in range(11)]
    from recetas.herramientas import afilar
    afilar(m.extrusion([(hx, cima), (-hx, cima)] + arco_bajo[::-1], OM_CAJA[1], mat='aluminio_anodizado', pieza=p, eje='z'), 0.0024, 2, angulo=30.0)
    # the glass (2 mm, its face at OM_CRISTAL) and the bezel round it, 2 mm proud
    w, h = OM_PANTALLA
    z0, z1 = OM_PANTALLA_Z - w / 2, OM_PANTALLA_Z + w / 2
    m.caja((h, 0.002, w), en=(0, OM_CRISTAL - 0.001, OM_PANTALLA_Z), mat='pantalla', pieza=p, bisel=0)
    fy, fg = cima + 0.002, 0.004
    m.caja((2 * hx - 0.006, fg, hz - 0.003 - z1), en=(0, fy, (z1 + hz - 0.003) / 2), mat='plastico_negro', pieza=p, bisel=0.001, seg=1)
    barbilla = m.caja((2 * hx - 0.006, fg, z0 + hz - 0.003), en=(0, fy, (z0 - hz + 0.003) / 2), mat='plastico_negro', pieza=p, bisel=0.001, seg=1)
    for lado in (1, -1):
        m.caja((hx - 0.003 - h / 2, fg, w), en=(lado * (hx - 0.003 + h / 2) / 2, fy, OM_PANTALLA_Z), mat='plastico_negro', pieza=p, bisel=0.001, seg=1)
    del barbilla
    # two keys beside the glass, toward the elbow (in the middle of that strip of the bezel);
    # a lamp between them
    kz = (z0 - hz + 0.003) / 2
    for k, x in enumerate((0.019, -0.019)):
        m.caja((0.026, 0.0036, 0.0105), en=(x, fy + 0.0034, kz), mat='pintura_naranja' if k == 0 else 'plastico_gris', pieza=p, bisel=0.0014, seg=2)
        m.punto(f'tecla_{k + 1}', (x, fy + 0.0052, kz), nota='la yema del índice derecho sobre la tecla (fija: no es una pieza)')
    m.cilindro(0.0022, 0.0012, en=(0, fy + 0.0022, kz), mat='piloto_verde', pieza=p, lados=10, bisel=0.0003)
    # what it says: its name over the glass, its number under it (each in the middle of its
    # strip of the bezel)
    tx = (hx - 0.003 + h / 2) / 2
    m.texto('MUÑECA M-2', (tx, fy + 0.002, OM_PANTALLA_Z), 0.0046, 'plastico_claro', p, normal=(0, 1, 0), arriba=(1, 0, 0), sobre=0.0002)
    m.texto('N/S 3381', (-tx, fy + 0.002, OM_PANTALLA_Z + 0.02), 0.0032, 'plastico_claro', p, normal=(0, 1, 0), arriba=(1, 0, 0), sobre=0.0002)
    m.tornillos([(x, fy + 0.002, z) for x in (-hx + 0.0075, hx - 0.0075) for z in (-hz + 0.0075, hz - 0.0075)], 0.0022, 0.0008, 'acero', p, None)
    # rubber down its two long edges; the port under its cap, toward the elbow
    for lado in (1, -1):
        m.caja((0.006, 0.012, OM_CAJA[1] - 0.02), en=(lado * (hx + 0.001), cima - 0.004, 0), mat='goma', pieza=p, bisel=0.002, seg=2)
    m.caja((0.024, 0.009, 0.004), en=(-0.012, cima - 0.0075, -hz - 0.0008), mat='goma', pieza=p, bisel=0.0014, seg=1)
    m.caja((0.006, 0.004, 0.0052), en=(-0.027, cima - 0.0075, -hz - 0.0008), mat='goma', pieza=p, bisel=0.001, seg=1)
    # ---- the strap round the sleeve: webbing with a stitched edge, its buckle and its tab under
    banda(m, p, R, R + OM_CORREA[1], -OM_CORREA[0] / 2, OM_CORREA[0] / 2, 'tela_gris', eje='z', lados=40, marco=Marco())
    for z in (-OM_CORREA[0] / 2 + 0.004, OM_CORREA[0] / 2 - 0.004):
        banda(m, p, R + OM_CORREA[1] - 0.0004, R + OM_CORREA[1] + 0.0007, z - 0.0012, z + 0.0012, 'tela_negra', eje='z', lados=40, marco=Marco())
    yb = -(R + OM_CORREA[1])
    m.caja((0.034, 0.005, OM_CORREA[0] + 0.008), en=(0, yb - 0.001, 0), mat='acero', pieza=p, bisel=0.0016, seg=1)
    m.caja((0.022, 0.0034, OM_CORREA[0] - 0.004), en=(0, yb - 0.003, 0), mat='acero_oscuro', pieza=p, bisel=0.001, seg=1)
    lengua = redondear([(0.012, yb - 0.004), (0.03, yb - 0.0035), (0.05, yb + 0.012), (0.058, yb + 0.03)], 0.012, pasos=4)
    chapa(m, p, lengua, 0.002, -OM_CORREA[0] / 2 + 0.006, OM_CORREA[0] / 2 - 0.006, 'tela_cincha', plano='xy', hacia=(0.2, -0.3), bisel=0.0005, seg=1)
    # where the strap meets the block: a bar each side
    for lado in (1, -1):
        m.cilindro(0.003, OM_CORREA[0] + 0.006, en=(lado * (hx + 0.0005), R * math.cos(f0) + 0.0045, 0), mat='acero', pieza=p, eje='z', lados=10, bisel=0.0008)
    # ---- what the game needs of it
    m.pantalla('pantalla', (0, OM_CRISTAL, OM_PANTALLA_Z), (w, h), normal=(0, 1, 0), arriba=(1, 0, 0), nota='ancha a lo largo del antebrazo; su "derecha" es +z (hacia la mano), su "arriba" +x')
    m.punto('toque', (0, OM_CRISTAL, OM_PANTALLA_Z), palma=(0, -1, 0), traves=(0, 0, -1), nota='donde el índice derecho toca el cristal')
    hueso, k = OM_HUESO
    m.datos['hueso'] = {
        'hueso': hueso, 'a_lo_largo': k,
        'origen_modelo': [0.357, 1.076, -0.018], 'eje_z_modelo': [0.3199, -0.9398, 0.12], 'eje_y_modelo': [-0.0387, 0.1136, 0.9928], 'eje_x_modelo': [-0.9467, -0.3222, 0.0],
        'radio_manga': 0.0715,
        'nota': 'origen = cabeza del hueso + 0.40 (cola - cabeza); z = el eje del hueso (codo -> muñeca); y = el +z del modelo del astronauta (su frente) hecho perpendicular al hueso; x = y × z. Los cuatro vectores "_modelo" son eso mismo en ejes y unidades del modelo del astronauta en reposo (multiplicar el origen por la escala 1,057). El giro alrededor de z es libre: es donde la manga lleva su propia lista.',
    }
    m.datos['vistas'] = {'cristal': dict(desde=(0.3, 1, -0.4), mira=(0, OM_CRISTAL, 0), dist=0.3, lente=50, arriba=(1, 0, 0)), 'canto': dict(desde=(0, 0.25, -1), orto=0.2, arriba=(0, 1, 0))}
