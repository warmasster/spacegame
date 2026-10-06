"""The Cachalote's definition, derived from the Alcotán's.

The Cachalote's crew section IS the Alcotán's (cabin, bridge, nose: the same hull section, the
same consoles, seats and panels, already checked for reach and fit), moved forward; what is its
own is everything aft of it: a hold 22 m long and 8 m wide with an overhead gantry crane and its
magnet, four tilting nacelles on two wings, a dorsal cradle with an Abejorro docked in it,
control moment gyros, and a ramp the width of the hold.

Run:  python tools/naves/cachalote.py   (writes assets/defs/ships/cachalote.jsonc)
The file it writes is the ship's definition like any other: edit the rules here and run again,
or edit the file (then stop running this).
"""
import json, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, 'assets', 'defs', 'ships', 'alcotan.jsonc')
OUT = os.path.join(ROOT, 'assets', 'defs', 'ships', 'cachalote.jsonc')


def load(path):
    text = open(path, encoding='utf-8').read()
    text = re.sub(r'^\s*//.*$', '', text, flags=re.M)
    text = re.sub(r'(?<=[\s,\[{}\]"0-9a-z])//[^\n"]*$', '', text, flags=re.M)
    text = re.sub(r',(\s*[\]}])', r'\1', text)
    return json.loads(text)


# ---------------------------------------------------------------- the geometry of the change
KX, KY = 1.93, 1.85          # the hold's section: the Alcotán's outline this much bigger
DZ = 4.0                     # the crew section moved forward
AFT = -22.0                  # the stern (the Alcotán's is at -9.3)
WALL = 2.0                   # how far out the hold's walls went
ROOF = 2.5                   # how far up its ceiling went
CREW_FROM = -3.62            # the Alcotán's z from which things go with the crew section
# The wings: where the forward one goes (the Alcotán's, this far along) and how far behind it the
# after one. Set so that the four engines push through the loaded ship's centre of mass (a test
# holds it to that: crates/ship/tests/cachalote.rs)
WING_Z = -0.8
AFT_WING = 9.0
# The tanks under each wing: where the two are across, from the wing's middle (the hull's wall
# is 1.4 m inboard of it, the nacelle's pylon 1.15 m outboard; a tank is 0.84 m across). The
# first is the one with a pipe to the manifold's junction, which is over it
TANKS = (0.12, -0.78)
# The crane's mast: the underside of its trolley, the length of each of its four sections and
# how far each comes out of the one over it
MAST_TOP = 3.37
MAST = 0.85
MAST_OUT = 0.7
# The hold's bays: where each column is across, the first row (by the ramp), the step between rows
BAY_X = {'a': 1.35, 'b': 0.0, 'c': -1.35}
BAY_Z = -19.4
BAY_STEP = 1.6
BAY_ROWS = 8
# what is in them (the rest are free)
LOADS = [
    ('b', 1, 'pale'), ('a', 1, 'cajas3'), ('c', 1, 'agua'),
    ('a', 2, 'pale'), ('b', 2, 'pale'), ('c', 2, 'pale'),
    ('b', 3, 'pale'), ('a', 3, 'cajas'), ('c', 3, 'prop'),
    ('b', 4, 'pale'), ('a', 4, 'agua'), ('c', 4, 'cajas3'),
    ('a', 5, 'pale'), ('c', 5, 'pale'),
]
# The crane's console: in the middle of the hold, forward of the bays
CONSOLE_Z = -6.4
# The hold's grid, painted on its deck: a cell round each bay (BAY_CELL across, BAY_STEP along),
# a rule down each side of it in metres from the ramp's sill and one across each end in metres
# from the centreline; the crane's console reads where its hook is in the same figures
BAY_CELL = 1.35
GRID_X = round(1.5 * BAY_CELL, 3)            # its edges across
GRID_Z = (round(BAY_Z - BAY_STEP / 2, 3), round(BAY_Z + BAY_STEP * (BAY_ROWS - 0.5), 3))   # and along
RULE_X = 2.2                                 # where the rules along it run
PAINT = [236, 196, 40]                       # the grid's paint (the bays' names are in it too)
WHITE = [236, 236, 230]                      # the rules'
CRANE_Z = -20.6                              # the crane's bridge at rest, and how far it and the trolley go
CRANE_RUN = 15.5
CRANE_ACROSS = 3.0


def crew(p):
    return [p[0], p[1], round(p[2] + DZ, 4)]


def winged(p):
    """Outside, to the sides at wing height: wings, nacelles, tanks, their feeds, tip lights
    (wherever along the hull the Alcotán has them: beside its cabin, since its V36)."""
    return abs(p[0]) > 2.25 and 0.3 < p[1] < 1.6


def T(p):
    """A point of the Alcotán in the Cachalote."""
    x, y, z = p
    s = 1.0 if x >= 0 else -1.0
    ax = abs(x)
    r = lambda v: [round(v[0], 4), round(v[1], 4), round(v[2], 4)]
    if winged(p):
        return r([x + s * 2.1, y + 1.3, z + WING_Z])
    if z > CREW_FROM:
        return crew(p)
    # under the deck: gear, hydraulic and propellant runs
    if y < -0.05:
        if ax > 0.9:
            return r([x + s * 2.1, y, z - 8.0])
        return r([x, y, z - 8.0])
    # over the roof: spine, fins, radiators
    if y >= 2.97:
        return r([x, y + 2.55, z - 14.0])
    # the stern: the ramp's winches and lights, the APU, the stern quads
    if z <= -8.5 and (y >= 2.0 or ax > 2.05):
        return r([x + (s * WALL if ax > 0.9 else 0.0), y + (ROOF - 0.9 if y >= 2.0 else 0.6), z + (AFT + 9.3)])
    # the ceiling of the hold
    if y >= 2.35:
        return r([x, y + ROOF, z - 0.6])
    # machinery along the walls (and what was in the Alcotán's wall stays in ours)
    if ax >= 0.9:
        return r([s * min(ax + WALL, 3.94), y, z - 0.6])
    return r([x, y, z - 0.6])


def grid_paint():
    """The hold's grid as paint on its deck: the lines of its cells and its rules."""
    out = []

    def line(x0, z0, x1, z1, color):
        # stretches of 'linea' end to end (each as long as the picture is, 1.6 m)
        n = max(1, round(max(abs(x1 - x0), abs(z1 - z0)) / 1.6))
        along_z = abs(z1 - z0) > abs(x1 - x0)
        for k in range(n):
            t = (k + 0.5) / n
            out.append({'imagen': 'linea', 'en': [round(x0 + (x1 - x0) * t, 3), 0.0, round(z0 + (z1 - z0) * t, 3)], 'normal': [0, 1, 0],
                        'arriba': [1, 0, 0] if along_z else [0, 0, 1], 'ancho': round(max(abs(x1 - x0), abs(z1 - z0)) / n, 3), 'tinte': color})

    # the cells: a line along each side of each column, a line across each end of each row
    for k in range(4):
        x = round(GRID_X - k * BAY_CELL, 3)
        line(x, GRID_Z[0], x, GRID_Z[1], PAINT)
    for k in range(BAY_ROWS + 1):
        z = round(GRID_Z[0] + k * BAY_STEP, 3)
        line(-GRID_X, z, GRID_X, z, PAINT)
    # the rules: down each side, from the sill forward (their ticks toward the grid); across each end
    m = int(GRID_Z[1] - AFT) + 1
    for side in (1, -1):
        for k in range(m):
            out.append({'imagen': 'regla', 'en': [side * RULE_X, 0.0, AFT + k + 0.5], 'normal': [0, 1, 0], 'arriba': [side, 0, 0], 'ancho': 1.0, 'tinte': WHITE})
    for z, up in ((round(GRID_Z[0] - 0.14, 3), 1), (round(GRID_Z[1] + 0.14, 3), -1)):
        for k in range(-3, 3):
            out.append({'imagen': 'regla', 'en': [k + 0.5, 0.0, z], 'normal': [0, 1, 0], 'arriba': [0, 0, up], 'ancho': 1.0, 'tinte': WHITE})
    return out


def grid_labels():
    """What the grid's lines and rules are called: columns and rows at its edges, as on a map;
    metres along and across by its rules."""
    out = []

    def text(t, x, z, up, tall, color):
        out.append({'texto': t, 'en': [round(x, 3), 0.001, round(z, 3)], 'normal': [0, 1, 0], 'arriba': [0, 0, up], 'alto': tall, 'color': color})

    # columns: at the ramp's end (read coming in) and at the console's (read from it)
    for col in 'abc':
        text(col.upper(), BAY_X[col], GRID_Z[0] - 1.25, 1, 0.3, PAINT)
        text(col.upper(), BAY_X[col], GRID_Z[1] + 0.62, -1, 0.3, PAINT)
    # rows: down each aisle, read from the aisle's ramp end
    for row in range(1, BAY_ROWS + 1):
        z = BAY_Z + BAY_STEP * (row - 1)
        for side in (1, -1):
            text(str(row), side * (RULE_X + 0.62), z, 1, 0.3, PAINT)
    # metres from the sill, every two, by the rules along
    for k in range(2, int(GRID_Z[1] - AFT) + 1, 2):
        for side in (1, -1):
            text('%d m' % k, side * (RULE_X + 0.24), AFT + k, 1, 0.1, WHITE)
    # metres from the centreline, by the rules across (port is +x: on the left coming in)
    for z, up in ((GRID_Z[0] - 0.34, 1), (GRID_Z[1] + 0.34, -1)):
        for k in range(-3, 4):
            text('0' if k == 0 else ('%d' % abs(k)), k, z, up, 0.1, WHITE)
        text('BABOR', 2.5, z - up * 0.17, up, 0.07, WHITE)
        text('ESTRIBOR', -2.5, z - up * 0.17, up, 0.07, WHITE)
    return out


def main():
    a = load(SRC)
    c = {}
    c['nombre'] = 'Cachalote'
    c['matricula'] = 'CCH-03'
    c['clase'] = 'Carguero pesado VTOL'
    c['astillero'] = 'Astilleros Orell'
    c['modelo'] = 'Orell CH-40'
    c['descripcion'] = 'Carguero de 36 m: bodega de 22 m con puente grúa e imán, cuatro góndolas basculantes, giróscopos de control y una cuna en el lomo con un Abejorro atracado.'

    # ------------------------------------------------------------ hull
    h = json.loads(json.dumps(a['casco']))
    h['perfiles'] = {k: [[round(x * KX, 3), round(y * KY, 3)] for x, y in v] for k, v in h['perfiles'].items()}
    alc = h['estaciones']
    own = [
        {'z': AFT, 'perfil': 'casco', 'escala': [0.97, 0.97], 'compartimento': 'bodega', 'divisiones': 2},
        {'z': -17.0, 'perfil': 'casco', 'compartimento': 'bodega', 'divisiones': 2},
        {'z': -13.0, 'perfil': 'casco', 'compartimento': 'bodega'},
        {'z': -11.4, 'perfil': 'casco', 'compartimento': 'bodega', 'divisiones': 2},
        {'z': -7.5, 'perfil': 'casco', 'compartimento': 'bodega', 'divisiones': 2},
        {'z': -3.5, 'perfil': 'casco', 'compartimento': 'bodega', 'divisiones': 2},
        # the hold ends square: a shoulder a hand deep down to the crew section's own section
        # (a truck: its box, then its cab)
        {'z': round(-3.4 + DZ - 0.15, 3), 'perfil': 'casco', 'compartimento': 'bodega'},
    ]
    SHIFT = len(own) - 3          # the Alcotán's interval i (from its cabin on) is ours i + SHIFT
    for st in alc[3:]:
        e = st.get('escala', [1.0, 1.0])
        st = dict(st)
        st['z'] = round(st['z'] + DZ, 3)
        st['escala'] = [round(e[0] / KX, 4), round(e[1] / KY, 4)]
        own.append(st)
    h['estaciones'] = own
    h['ventanas'] = [[i + SHIFT, f] for i, f in h['ventanas']]
    # gear bays: the main legs' (a short interval of its own), the nose leg's
    h['huecos'] = [[2, 0], [2, 1], [2, 10], [2, 11]] + [[i + SHIFT, f] for i, f in h['huecos'] if i >= 3]
    h['pintura'] = [
        {'caras': [0, 1, 10, 11], 'color': [52, 54, 58]},
        {'caras': [2, 9], 'color': [60, 62, 68]},
        {'caras': [3, 8], 'tramos': [0, 5], 'color': [40, 86, 150]},
        {'caras': [4, 7], 'tramos': [0, 0], 'color': [230, 190, 30]},
        {'caras': [5, 6], 'tramos': [8 + SHIFT, 9 + SHIFT], 'color': [52, 54, 58]},
    ]
    c['casco'] = h

    c['suelos'] = [
        {'id': 'suelo_bodega', 'desde': [-3.9, AFT], 'hasta': [3.9, round(-3.4 + DZ - 0.15, 3)], 'divisiones': [5, 12], 'material': 'panel_suelo', 'compartimento': 'bodega', 'color': [112, 116, 118]},
        # the sill at the cabin door, inside the shoulder
        {'id': 'suelo_bodega_proa', 'desde': [-1.9, round(-3.4 + DZ - 0.15, 3)], 'hasta': [1.9, -3.4 + DZ], 'divisiones': [3, 1], 'material': 'panel_suelo', 'compartimento': 'bodega', 'color': [112, 116, 118]},
    ]
    for f in a['suelos'][1:]:
        f = dict(f)
        f['desde'] = [f['desde'][0], round(f['desde'][1] + DZ, 3)]
        f['hasta'] = [f['hasta'][0], round(f['hasta'][1] + DZ, 3)]
        c['suelos'].append(f)
    c['mamparos'] = []
    for m in a['mamparos']:
        m = dict(m)
        if m['id'] == 'popa':
            m['z'] = AFT
            m['puerta'] = [-3.3, 3.3, 4.3]
            m['desde_y'] = round(m['desde_y'] * KY, 3)
        else:
            m['z'] = round(m['z'] + DZ, 3)
        c['mamparos'].append(m)
    c['canalizaciones'] = a.get('canalizaciones', {})

    # ------------------------------------------------------------ components
    comps = []
    drop = ('anclaje_', 'carga_', 'lomo', 'aleta_')
    for k in a['componentes']:
        if k['id'].startswith(drop):
            continue
        k = json.loads(json.dumps(k))
        k['en'] = T(k['en'])
        comps.append(k)
    by = {k['id']: k for k in comps}
    # a gear for a hundred tonnes: thicker legs, wider feet, a longer stroke (it rests further in:
    # the legs a little longer for it)
    for k in comps:
        if not k['id'].startswith('tren_'):
            continue
        f = k['forma']
        if k['id'].endswith('_pata'):
            f['radius'] = round(f['radius'] * 1.9, 3)
            f['height'] = round(f['height'] + 0.1, 3)
            k['en'][1] = round(k['en'][1] - 0.05, 3)
        elif k['id'].endswith('_amort'):
            f['radius'] = round(f['radius'] * 1.9, 3)
            f['height'] = round(f['height'] * 1.3, 3)
        elif k['id'].endswith('_pie'):
            f['size'] = [round(f['size'][0] * 2.2, 3), round(f['size'][1] * 1.6, 3), round(f['size'][2] * 2.2, 3)]
            k['en'][1] = round(k['en'][1] - 0.1 - f['size'][1] * 0.19, 3)
    # a wing's leading edge goes with its wing (it is ahead of where the crew section starts)
    by['borde_ala_izq']['en'] = [by['ala_izq']['en'][0], by['ala_izq']['en'][1], round(by['ala_izq']['en'][2] + 1.3, 3)]

    def machine(k, **params):
        m = by[k].setdefault('maquina', {})
        m.setdefault('params', {}).update(params)

    # bigger engines, more tanks, stronger thrusters, wider radiators, a reactor to feed them
    machine('gondola_izq', empuje_vacio='65 kN')
    machine('radiador_izq', area_plegado='10 m2', area_desplegado='34 m2', caudal='0.8 kg/s')
    # Its propellant: the Alcotán's wing tank, which holds what fits in it (its kind says how
    # much: assets/defs/components/naves.jsonc, docs/CARGA.md), two of them abreast under each
    # of its four wings: eight. The four of a side feed that side's manifold through the same
    # valve; the first by its pipe, as in the Alcotán, the others by pipes that are not drawn
    tank, wing = by['deposito_izq'], by['ala_izq']['en']
    tank['en'] = [round(wing[0] + TANKS[0], 3), tank['en'][1], tank['en'][2]]
    for name, dx, dz in (('deposito_int_izq', TANKS[1], 0.0), ('deposito_popa_izq', TANKS[0], -AFT_WING), ('deposito_popa_int_izq', TANKS[1], -AFT_WING)):
        k = json.loads(json.dumps(tank))
        k['id'] = name
        k['en'] = [round(wing[0] + dx, 3), tank['en'][1], round(tank['en'][2] + dz, 3)]
        k['cableado'] = False
        comps.append(k)
    for k in comps:
        if k.get('tipo') == 'tobera_rcs':
            k['maquina'].setdefault('params', {})['empuje'] = '18 kN'
    # the after wing: a second pair of nacelles on the same switches and the same tilt
    for src in ('ala_izq', 'borde_ala_izq', 'pilon_izq', 'gondola_izq'):
        k = json.loads(json.dumps(by[src]))
        k['id'] = src.replace('_izq', '_popa_izq')
        k['en'] = [k['en'][0], k['en'][1], round(k['en'][2] - AFT_WING, 3)]
        if src == 'gondola_izq':
            k['articulacion'] = 'gondola_popa_izq'
            k['maquina']['puertos']['propelente'] = 'prop:alim_popa_izq'
        comps.append(k)
    # the stern lights of the wing tips go to the after wing
    for src in ('estrobo_izq',):
        by[src]['en'] = [by[src]['en'][0], by[src]['en'][1], round(by[src]['en'][2] - AFT_WING, 3)]

    comps += [
        # ---- the spine and the fins ----
        {'id': 'lomo', 'forma': {'kind': 'box', 'size': [0.4, 0.16, 12.0]}, 'estilo': 'lomo', 'en': [0, 5.63, -15.9], 'material': 'panel_casco', 'color': [60, 62, 68], 'chapa': 0.4},
        {'id': 'aleta_izq', 'forma': {'kind': 'wedge', 'size': [0.14, 2.2, 3.0]}, 'estilo': 'aleta', 'en': [2.6, 6.4, -20.3], 'rot': [0, 0, -20], 'material': 'panel_casco', 'color': [40, 86, 150], 'espejo': True},

        # ---- control moment gyros: they hold and turn 100 t without propellant, slowly ----
        {'id': 'giroscopo', 'tipo': 'giroscopo_cmg', 'en': [-3.2, 0, -1.9],
         'maquina': {'puertos': {'energia': 'elec:c_giro'}, 'ordenes': {'marcha': 'giro.marcha'},
                     'params': {'momento': 400000, 'par': 60000, 'potencia_reposo': '600 W', 'potencia_par': '4 kW', 'arranque': '10 s', 'parada': '900 s'}}},

        # ---- the dorsal cradle: a clamp that takes a small ship set down on it ----
        {'id': 'cuna', 'tipo': 'cuna_atraque', 'en': [0, 5.56, -8.0]},

        # ---- the gantry crane: a bridge on two rails, a trolley on the bridge, a hoist under it
        #      with a magnet at its end (crates/ship/src/cargo.rs: a clamp that grips) ----
        {'id': 'rail_izq', 'forma': {'kind': 'box', 'size': [0.16, 0.2, 17.6]}, 'estilo': 'viga_i', 'en': [3.6, 3.9, -12.6], 'material': 'acero', 'color': [206, 164, 30], 'hueco': 0.01, 'espejo': True},
        {'id': 'grua_puente', 'forma': {'kind': 'box', 'size': [7.0, 0.3, 0.4]}, 'estilo': 'viga_cajon', 'en': [0, 3.85, -20.6], 'material': 'acero', 'color': [206, 164, 30], 'hueco': 0.012, 'articulacion': 'grua_puente'},
        # an amber beacon under each end of the bridge: they flash while the crane moves
        {'id': 'grua_baliza_izq', 'tipo': 'baliza_obra', 'en': [3.2, 3.65, -20.6], 'articulacion': 'grua_puente', 'espejo': True,
         'maquina': {'puertos': {'alimentacion': 'elec:c_grua'}, 'ordenes': {'encender': 'grua.moviendo'}}},
        {'id': 'grua_testero_izq', 'forma': {'kind': 'box', 'size': [0.3, 0.34, 0.9]}, 'estilo': 'testero', 'en': [3.6, 3.85, -20.6], 'material': 'acero', 'color': [58, 60, 66], 'articulacion': 'grua_puente', 'espejo': True},
        {'id': 'grua_carro', 'forma': {'kind': 'box', 'size': [0.8, 0.36, 0.7]}, 'estilo': 'carro_grua', 'en': [0, 3.55, -20.6], 'material': 'acero', 'color': [58, 60, 66], 'hueco': 0.01, 'articulacion': 'grua_carro'},
        # a telescopic mast in four sections: the outer tube hangs from the trolley, two sections
        # come down out of it in step (joints that follow the hoist's), the last one carries the
        # magnet. Drawn in, a load hangs clear over everything on the deck; let out, the magnet
        # is a hand over the deck. (Rigid, not a cable: it works with no weight too.)
        {'id': 'grua_tubo', 'forma': {'kind': 'box', 'size': [0.27, MAST, 0.27]}, 'estilo': 'telescopio_exterior', 'en': [0, round(MAST_TOP - MAST / 2, 3), -20.6], 'material': 'acero', 'color': [58, 60, 66], 'hueco': 0.01, 'articulacion': 'grua_carro'},
        {'id': 'grua_tramo_1', 'forma': {'kind': 'box', 'size': [0.21, MAST, 0.21]}, 'estilo': 'telescopio_tramo', 'en': [0, round(MAST_TOP - MAST / 2 - 0.03, 3), -20.6], 'material': 'acero', 'color': [150, 152, 156], 'hueco': 0.008, 'articulacion': 'grua_tramo_1'},
        {'id': 'grua_tramo_2', 'forma': {'kind': 'box', 'size': [0.155, MAST, 0.155]}, 'estilo': 'telescopio_tramo', 'en': [0, round(MAST_TOP - MAST / 2 - 0.06, 3), -20.6], 'material': 'acero', 'color': [170, 172, 176], 'hueco': 0.008, 'articulacion': 'grua_tramo_2'},
        {'id': 'grua_mastil', 'forma': {'kind': 'box', 'size': [0.1, MAST, 0.1]}, 'estilo': 'telescopio_interior', 'en': [0, round(MAST_TOP - MAST / 2 - 0.09, 3), -20.6], 'material': 'acero', 'color': [190, 192, 196], 'hueco': 0.008, 'articulacion': 'grua_gancho'},
        {'id': 'grua_iman', 'tipo': 'electroiman_carga', 'en': [0, round(MAST_TOP - MAST - 0.09, 3), -20.6], 'articulacion': 'grua_gancho',
         'maquina': {'puertos': {'energia': 'elec:c_grua'}, 'ordenes': {'agarrar': 'grua.agarrar'}, 'params': {'condensador': 4000, 'pulso': 1500, 'potencia_carga': '900 W'}}},
        # the crane's console: in the middle of the hold, forward of the bays, facing aft: whoever
        # works it stands looking down the hold at the crane
        {'id': 'consola_grua', 'tipo': 'consola_tug', 'en': [0, 0, CONSOLE_Z], 'rot': [0, 180, 0]},
    ]

    # ---- the bays: three columns (A to port, B on the centreline, C to starboard) of eight rows
    #      from the ramp forward, each a clamp with its name painted on the deck by it; an aisle
    #      down each side, between the bays and the machinery on the walls. Half of them loaded:
    #      pallets, crates and drums; the crane reaches all of it ----
    def bay(col, row):
        return BAY_X[col], round(BAY_Z + BAY_STEP * (row - 1), 3)

    def cargo(col, row, what):
        x, z = bay(col, row)
        name = 'anclaje_%s%d' % (col, row)
        out = []
        if what == 'pale':
            out.append({'id': 'carga_%s%d' % (col, row), 'tipo': 'pale_regolito', 'en': [x, 0.012, z], 'anclaje': name})
        elif what in ('cajas', 'cajas3'):
            for i, dx in enumerate((-0.27, 0.27)):
                out.append({'id': 'carga_%s%d_%d' % (col, row, i + 1), 'tipo': 'caja_repuestos', 'en': [round(x + dx, 3), 0.012, z], 'anclaje': name})
            if what == 'cajas3':
                out.append({'id': 'carga_%s%d_3' % (col, row), 'tipo': 'caja_repuestos', 'en': [x, 0.525, z], 'rot': [0, 9, 0], 'anclaje': name})
        else:
            for i, dx in enumerate((-0.28, 0.28)):
                out.append({'id': 'carga_%s%d_%d' % (col, row, i + 1), 'tipo': 'bidon_agua' if what == 'agua' else 'bidon_combustible', 'en': [round(x + dx, 3), 0.012, z], 'anclaje': name})
        return out

    for row in range(1, BAY_ROWS + 1):
        for col in 'abc':
            x, z = bay(col, row)
            comps.append({'id': 'anclaje_%s%d' % (col, row), 'tipo': 'anclaje_carga', 'en': [x, 0, z]})
    for col, row, what in LOADS:
        comps += cargo(col, row, what)

    comps += [
        # ---- more light for a bigger hold ----
        {'id': 'campana_bod_izq', 'tipo': 'campana', 'en': [1.9, 5.2, -19.5], 'repetir': [4, [0, 0, 5.4]], 'espejo': True,
         'maquina': {'puertos': {'alimentacion': 'elec:c_luces_bod'}, 'ordenes': {'encender': 'luces.bodega'}}},
    ]
    c['componentes'] = comps

    # ------------------------------------------------------------ closures, decals
    c['cierres'] = json.loads(json.dumps(a['cierres']))
    for cl in c['cierres']:
        if cl['id'] == 'rampa':
            cl['amortiguamiento'] = 9000
            # down to the ground and no further (the leaf is 4.46 m, its hinge 1.65 m over the
            # feet): open further it would stand the ship on its ramp
            cl["mueve"]["angulo"] = 110.8
    c['calcas'] = []
    for d in a['calcas']:
        if d['en'][2] > CREW_FROM and not winged(d['en']) and 'pieza' not in d and d['imagen'] != 'poster_carga':
            d = dict(d)
            d['en'] = crew(d['en'])
            c['calcas'].append(d)
    c['calcas'] += [
        {'imagen': 'senal_carga', 'en': [0, 0.0, -20.9], 'normal': [0, 1, 0], 'arriba': [0, 0, 1], 'ancho': 2.0},
        {'imagen': 'franjas', 'en': [0, 4.6, -21.967], 'normal': [0, 0, 1], 'ancho': 4.0},
        {'imagen': 'matricula', 'en': [0, 2.4, -22.137], 'normal': [0, 0, -1], 'ancho': 2.6, 'pieza': 'rampa.hoja'},
    ]
    c['calcas'] += grid_paint()

    # ------------------------------------------------------------ joints, actuators
    j = {}
    for k, v in a['articulaciones'].items():
        v = dict(v)
        if 'pivote' in v:
            v['pivote'] = T(v['pivote'])
        j[k] = v
    for side in ('izq', 'der'):
        v = dict(j['gondola_' + side])
        v['pivote'] = [v['pivote'][0], v['pivote'][1], round(v['pivote'][2] - AFT_WING, 3)]
        v['nombre'] = v['nombre'].replace('Góndola', 'Góndola de popa')
        j['gondola_popa_' + side] = v
    for k in ('amort_izq', 'amort_der', 'amort_morro'):
        j[k]['limites'] = [0, 0.45]
    j['grua_puente'] = {'tipo': 'corredera', 'eje': [0, 0, 1], 'limites': [0, 15.5], 'inicial': 0, 'nombre': 'Puente de la grúa', 'amortiguamiento': 400}
    j['grua_carro'] = {'tipo': 'corredera', 'eje': [1, 0, 0], 'limites': [-3.0, 3.0], 'inicial': 0, 'padre': 'grua_puente', 'nombre': 'Carro de la grúa', 'amortiguamiento': 600}
    j['grua_gancho'] = {'tipo': 'corredera', 'eje': [0, -1, 0], 'limites': [0, round(3 * MAST_OUT, 3)], 'inicial': 0, 'padre': 'grua_carro', 'nombre': 'Izado de la grúa', 'amortiguamiento': 2000}
    for i in (1, 2):
        j['grua_tramo_%d' % i] = {'tipo': 'corredera', 'eje': [0, -1, 0], 'limites': [0, round(i * MAST_OUT, 3)], 'inicial': 0, 'padre': 'grua_carro', 'nombre': 'Tramo %d del mástil' % i,
                                  'sigue': {'de': 'grua_gancho', 'razon': round(i / 3.0, 5)}}
    c['articulaciones'] = j

    act = {}
    for k, v in a['actuadores'].items():
        v = json.loads(json.dumps(v))
        t = v.get('transmision', {})
        for key in ('anclaje_fijo', 'anclaje_movil'):
            if key in t:
                t[key] = T(t[key])
        act[k] = v
    for side in ('izq', 'der'):
        # the ramp is the width of the hold: its winches further apart, with the pull for it
        w = act['act_rampa_' + side]
        s = 1 if side == 'izq' else -1
        w['transmision']['anclaje_fijo'] = [s * 3.0, 4.6, AFT + 0.7]
        w['transmision']['anclaje_movil'] = [s * 3.0, 4.6, AFT - 0.08]
        # (what its two winches draw goes through the RAMPA breaker, 200 A: 78 A each. With
        # 60 N*m each they drew 673 A, the breaker tripped at the first pull and the ramp never
        # shut. A shorter gear train: it shuts in 9 s — docs/TIEMPOS.md)
        w['accionamiento']['par_bloqueo'] = '7 N*m'
        w['transmision']['reduccion'] = 30
        v = json.loads(json.dumps(act['act_gondola_' + side]))
        v['articulacion'] = 'gondola_popa_' + side
        v['nombre'] = v['nombre'].replace('góndola', 'góndola de popa')
        act['act_gondola_popa_' + side] = v
        # (four screws on the GÓNDOL. breaker, 200 A: 39 A each. With 30 N*m each they drew
        # 1 350 A between them, the breaker tripped and no nacelle ever tilted)
        # each must hold its nacelle's weight too (1.6 kN*m at cruise): a longer gear train
        for g in (act['act_gondola_' + side], v):
            g['accionamiento']['par_bloqueo'] = '3.5 N*m'
            g['transmision']['reduccion'] = 1200
        act['act_rad_' + side]['accionamiento']['par_bloqueo'] = '6 N*m'
    # (the crane goes where its levers are left at its own pace: the bridge 2 m/s, the trolley
    # 1 m/s, the hoist 0.5 m/s, each from end to end in under 12 s — docs/TIEMPOS.md)
    crane = lambda joint, name, order, pitch, stall: {
        'articulacion': joint, 'nombre': name, 'fabricante': 'Kestrel', 'modelo': 'Husillo de grúa GH-8',
        'accionamiento': {'tipo': 'electrico', 'velocidad_vacio': '3000 rpm', 'par_bloqueo': stall},
        'transmision': {'tipo': 'husillo', 'paso': pitch, 'eficiencia': 0.35},
        'control': {'modo': 'posicion', 'orden': order, 'frenado': 0.03}, 'redes': {'motor': 'elec:c_grua'}}
    act['act_grua_puente'] = crane('grua_puente', 'Traslación del puente grúa', 'grua.puente', '40 mm', '24 N*m')
    act['act_grua_carro'] = crane('grua_carro', 'Traslación del carro', 'grua.carro', '20 mm', '14 N*m')
    act['act_grua_gancho'] = crane('grua_gancho', 'Izado', 'grua.gancho', '10 mm', '14 N*m')
    c['actuadores'] = act

    # ------------------------------------------------------------ networks
    nets = json.loads(json.dumps(a['redes']))
    for n in nets.values():
        n['nodos'] = {k: T(v) for k, v in n.get('nodos', {}).items()}
    e = nets['elec']
    e['nodos'].update({
        'b_grua': [1.96, 1.2, -2.7 + DZ], 'b_giro': [1.96, 1.15, -2.7 + DZ],
        'c_grua': [3.6, 4.3, -4.6], 'c_giro': [-3.2, 0.7, -1.9],
    })
    e['tramos'] += [{'de': 'b_grua', 'a': 'c_grua', 'tipo': 'cable_grueso'}, {'de': 'b_giro', 'a': 'c_giro', 'tipo': 'cable_grueso'}]
    e['interruptores'] += [
        {'id': 'brk.grua', 'de': 'bus_b', 'a': 'b_grua', 'medir': True, 'pieza': 'disyuntores.caja'},
        {'id': 'brk.giro', 'de': 'bus_b', 'a': 'b_giro', 'medir': True, 'pieza': 'disyuntores.caja'},
    ]
    p = nets['prop']
    for side, s in (('izq', 1), ('der', -1)):
        p['nodos']['alim_popa_' + side] = [p['nodos']['alim_' + side][0], p['nodos']['alim_' + side][1], round(p['nodos']['alim_' + side][2] - AFT_WING, 3)]
        p['tramos'].append({'de': 'm_' + side, 'a': 'alim_popa_' + side})
    c['redes'] = nets
    if 'maquinas' in a:
        c['maquinas'] = a['maquinas']

    # ------------------------------------------------------------ rooms, panels, seats
    rooms = json.loads(json.dumps(a['compartimentos']))
    for r in rooms:
        if r['id'] == 'bodega':
            r['cajas'] = [[[-4.0, 0, AFT], [4.0, 5.3, -3.4 + DZ]]]
            r['volumen'] = 700
            # its vent, for its volume (as `rooms` gives any room: 0.0015 m2 per m3)
            r['aberturas'] = [dict(v, area=round(700 * 0.0015, 2)) if v.get('senal') == 'bodega.venteo' else v for v in r.get('aberturas', [])]
            r['panel']['en'] = [4.05, 1.45, -5.2]
        else:
            r['cajas'] = [[crew(b[0]), crew(b[1])] for b in r['cajas']]
            r['panel']['en'] = crew(r['panel']['en'])
    c['compartimentos'] = rooms

    panels = []
    for m in a['paneles']:
        m = json.loads(json.dumps(m))
        en = m['en']
        if m['id'] == 'reactor':
            m['en'] = [-4.05, 1.55, -7.9]
        elif m['id'] == 'rampa_bodega':
            m['en'] = [4.0, 1.35, AFT + 0.6]
        elif m['id'] == 'rampa_ext':
            m['en'] = [3.75, 1.0, AFT - 0.162]
        else:
            m['en'] = crew(en)
        if m['id'] == 'principal':
            m['prefijo'] = {'emp_max': '75', 'emp_min': '20', 'emp_nom': '65', 'emp_rojo': '68'}
        # its breaker panel whole: the crane's and the gyros' breakers, which the Alcotán leaves out
        if m['id'] == 'disyuntores':
            assert m.pop('sin', None) == ['grua', 'giro'], 'the Alcotán leaves other breakers out now'
        panels.append(m)
    panels += [
        # the crane, on its console in the middle of the hold: worked standing, looking down the
        # hold at it
        {'id': 'grua', 'panel': 'cach_grua', 'pieza': 'consola_grua.cara', 'en': [0, 0.9655, round(CONSOLE_Z + 0.0421, 4)], 'normal': [0, 0.5736, 0.8192], 'energia': 'elec:c_grua', 'cableado': False},
        # gyros and cradle, on the bridge's after wall
        {'id': 'auxiliar', 'panel': 'cach_auxiliar', 'en': [-1.05, 2.05, 2.6 + DZ + 0.112], 'normal': [0, 0, 1], 'energia': 'elec:c_pantallas'},
    ]
    c['paneles'] = panels
    # the bays' names on the deck, twice each: aft of the bay to be read coming in by the ramp,
    # forward of it to be read from the crane's console
    labels = list(c.get('rotulos', []))
    for row in range(1, BAY_ROWS + 1):
        for col in 'abc':
            x = BAY_X[col]
            z = round(BAY_Z + BAY_STEP * (row - 1), 3)
            for dz, up in ((-0.62, 1), (0.62, -1)):
                labels.append({'texto': '%s%d' % (col.upper(), row), 'en': [x, 0.001, round(z + dz, 3)], 'normal': [0, 1, 0], 'arriba': [0, 0, up], 'alto': 0.16, 'color': PAINT})
    labels += grid_labels()
    c['rotulos'] = labels

    seats = json.loads(json.dumps(a['asientos']))
    for s in seats:
        s['ojos'] = crew(s['ojos'])
        s['salida'] = crew(s['salida'])
    c['asientos'] = seats
    c['bajadas'] = [dict(b, en=crew(b['en'])) for b in a.get('bajadas', [])]

    # ------------------------------------------------------------ logic
    c['senales'] = dict(a.get('senales', {}))
    # (the crane's and the gyros' breakers have their handles on the breaker panel like the rest)
    d = dict(a['derivadas'])
    d['alarma.disyuntor'] = d['alarma.disyuntor'].replace('!(', '!(brk.grua && brk.giro && ', 1)
    # a leak in any of its eight tanks is a leak (the Alcotán's two, and the six more)
    leak = 'deposito_izq.fuga > 0.05 || deposito_der.fuga > 0.05'
    leaks = ' || '.join('deposito_%s%s.fuga > 0.05' % (t, s) for t in ('', 'int_', 'popa_', 'popa_int_') for s in ('izq', 'der'))
    assert leak in d['alarma.algo'] and a['alarmas']['Fuga de propelente'] == leak, 'the Alcotán says its leaks otherwise now'
    d['alarma.algo'] = d['alarma.algo'].replace(leak, leaks)
    # the failure of either aft engine is a failure too (crates/ship/tests/logica_avisos.rs: the
    # failure of every main engine is watched by an alarm, not only written on a page)
    failed = 'gondola_der.estado == 6'
    assert failed in d['alarma.algo'], 'the Alcotán says its engines\' failures otherwise now'
    d['alarma.algo'] = d['alarma.algo'].replace(failed, failed + ' || gondola_popa_izq.estado == 6 || gondola_popa_der.estado == 6')
    # the cradle and the crane's magnet are clamps: their order is the switch (and the magnet's state)
    d['cuna.soltar'] = 'cuna.abrir'
    d['grua_iman.soltar'] = '!grua_iman.activo'
    # where the hook is, in the figures painted on the deck: metres from the ramp's sill and to
    # port of the centreline, and the bay under it (its column 0..2 = A..C, its row, and whether
    # it is over the bay's middle, within a hand)
    d['grua.largo'] = '%s + grua_puente.pos * %s' % (round(CRANE_Z - AFT, 3), CRANE_RUN)
    d['grua.banda'] = '(grua_carro.pos - 0.5) * %s' % (2 * CRANE_ACROSS)
    d['grua.columna'] = 'clamp(round((%s - grua.banda) / %s), 0, 2)' % (BAY_X['a'], BAY_CELL)
    d['grua.fila'] = 'clamp(round((grua.largo - %s) / %s) + 1, 1, %d)' % (round(BAY_Z - AFT, 3), BAY_STEP, BAY_ROWS)
    d['grua.centrada'] = 'abs(grua.banda - (%s - grua.columna * %s)) < 0.12 && abs(grua.largo - (%s + (grua.fila - 1) * %s)) < 0.12' % (BAY_X['a'], BAY_CELL, round(BAY_Z - AFT, 3), BAY_STEP)
    # the crane on its way somewhere (any of its three ways still short of where its lever says)
    d['grua.moviendo'] = 'abs(grua.puente - grua_puente.pos) > 0.012 || abs(grua.carro - grua_carro.pos) > 0.012 || abs(grua.gancho - grua_gancho.pos) > 0.012'
    c['derivadas'] = d
    pr = dict(a['prioridades'])
    pr['en'] = [4.05, 1.55, -6.7]
    c['prioridades'] = pr
    # air moved on purpose (docs/AIRE.md): the hand valves go with their bulkheads (the one out
    # of the hold, to the new stern, by the ramp), the compressor's panel to the hold's wall over
    # its compressor (which went there with the rest of the machinery)
    tv = json.loads(json.dumps(a.get('trasvase', {})))
    for v in tv.get('valvulas', []):
        v['en'] = [3.55, 1.5, AFT] if 'vacio' in v['entre'] else crew(v['en'])
    for k in tv.get('compresores', []):
        k['panel']['en'] = [4.05] + T(k['panel']['en'])[1:]
    if tv:
        c['trasvase'] = tv
    c['enclavamientos'] = a['enclavamientos']
    c['esenciales'] = a['esenciales']
    al = dict(a['alarmas'])
    al['Fuga de propelente'] = leaks
    al['Giróscopos saturados'] = 'giroscopo.carga > 0.95'
    al['Motor de popa izquierdo: fallo'] = 'gondola_popa_izq.estado == 6'
    al['Motor de popa derecho: fallo'] = 'gondola_popa_der.estado == 6'
    c['alarmas'] = al
    v = dict(a['vuelo'])
    v['motores'] = ['gondola_izq', 'gondola_der', 'gondola_popa_izq', 'gondola_popa_der']
    v['giro'] = [9, 10, 12]
    v['ruedas'] = ['giroscopo']
    v['descarga'] = 'vuelo.descarga'
    c['vuelo'] = v
    # its own gravity, as the shuttle's (its derived signal "abordo.gravedad" comes with the rest)
    c['gravedad'] = dict(a['gravedad'])
    # what it carries docked as it is built: a ship set in a clamp of its own
    c['lleva'] = [{'nave': 'abejorro', 'anclaje': 'cuna'}]

    write(c)


HEAD = '''// CACHALOTE — heavy VTOL freighter (crates/ship, docs/NAVES.md). Ship frame: +Y up, nose toward +Z,
// +X to port, metres; the deck is y = 0.
//
// Written by tools/naves/cachalote.py from the Alcotán's definition: its crew section (cabin,
// bridge, nose) is the Alcotán's, moved forward; aft of it everything is its own — a hold 22 m
// long with an overhead gantry crane and its magnet, four tilting nacelles on two wings, control
// moment gyros, a ramp the width of the hold, and a dorsal cradle with an Abejorro docked in it.
'''


def one_line(v):
    return json.dumps(v, ensure_ascii=False)


def write(c):
    out = [HEAD, '{']
    keys = list(c.keys())
    for n, k in enumerate(keys):
        v = c[k]
        end = '' if n + 1 == len(keys) else ','
        if isinstance(v, list) and v and isinstance(v[0], dict):
            out.append(f'  {json.dumps(k)}: [')
            out += [f'    {one_line(x)}{"," if i + 1 < len(v) else ""}' for i, x in enumerate(v)]
            out.append(f'  ]{end}')
        elif isinstance(v, dict) and k in ('redes', 'casco'):
            out.append(f'  {json.dumps(k)}: {{')
            items = list(v.items())
            for i, (kk, vv) in enumerate(items):
                e2 = ',' if i + 1 < len(items) else ''
                if isinstance(vv, dict) and k == 'redes':
                    out.append(f'    {json.dumps(kk)}: {{')
                    sub = list(vv.items())
                    for m, (a, b) in enumerate(sub):
                        e3 = ',' if m + 1 < len(sub) else ''
                        if isinstance(b, list) and b and isinstance(b[0], dict):
                            out.append(f'      {json.dumps(a)}: [')
                            out += [f'        {one_line(x)}{"," if q + 1 < len(b) else ""}' for q, x in enumerate(b)]
                            out.append(f'      ]{e3}')
                        else:
                            out.append(f'      {json.dumps(a)}: {one_line(b)}{e3}')
                    out.append(f'    }}{e2}')
                elif isinstance(vv, list) and vv and isinstance(vv[0], dict):
                    out.append(f'    {json.dumps(kk)}: [')
                    out += [f'      {one_line(x)}{"," if q + 1 < len(vv) else ""}' for q, x in enumerate(vv)]
                    out.append(f'    ]{e2}')
                else:
                    out.append(f'    {json.dumps(kk)}: {one_line(vv)}{e2}')
            out.append(f'  }}{end}')
        elif isinstance(v, dict) and len(one_line(v)) > 160:
            out.append(f'  {json.dumps(k)}: {{')
            items = list(v.items())
            out += [f'    {json.dumps(a)}: {one_line(b)}{"," if i + 1 < len(items) else ""}' for i, (a, b) in enumerate(items)]
            out.append(f'  }}{end}')
        else:
            out.append(f'  {json.dumps(k)}: {one_line(v)}{end}')
    out.append('}')
    open(OUT, 'w', encoding='utf-8', newline='\n').write('\n'.join(out) + '\n')
    print('escrito', OUT)


if __name__ == '__main__':
    main()
