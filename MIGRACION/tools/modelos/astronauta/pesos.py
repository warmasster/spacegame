"""The body's weights after Blender's automatic binding: what is put right by rule.

The suit's fabric is bound with automatic weights (bone heat) to the 16 deforming body bones; that
holds up well at elbows, knees, hips and shoulders (see the pose pictures). Three things it cannot
know are done here:

  - a hard ring rides on the upper arm just below each shoulder (the scye bearing): the fabric
    under it must follow the upper arm alone, or the arm leaves the ring when it is raised;
  - tiny weights are dropped and no vertex keeps more than four bones (what a glTF skin carries),
    always adding up to 1;
  - the labels the mesh was born with (`T_...`, which limb a vertex belongs to) are removed.
"""
import bpy  # noqa: F401


def smoothstep(e0, e1, x):
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def leer(ob):
    """[{group name: weight}] for every vertex."""
    nombres = [g.name for g in ob.vertex_groups]
    return [{nombres[g.group]: g.weight for g in v.groups if g.weight > 0.0} for v in ob.data.vertices]


def escribir(ob, pesos):
    """Replaces every vertex group of the object by those of `pesos`."""
    ob.vertex_groups.clear()
    grupos = {}
    por_peso = {}
    for i, w in enumerate(pesos):
        for nombre, x in w.items():
            por_peso.setdefault((nombre, round(x, 6)), []).append(i)
    for (nombre, x), indices in por_peso.items():
        if nombre not in grupos:
            grupos[nombre] = ob.vertex_groups.new(name=nombre)
        grupos[nombre].add(indices, x, 'REPLACE')


def limpiar(pesos, huesos, maximo=4, minimo=0.01):
    """Only bones, none below `minimo`, `maximo` to a vertex at most, adding up to 1."""
    out = []
    for w in pesos:
        w = sorted(((x, n) for n, x in w.items() if n in huesos), reverse=True)[:maximo]
        if not w:
            out.append({})
            continue
        mayor = w[0][0]
        w = [(x, n) for x, n in w if x >= minimo or x == mayor]
        s = sum(x for x, _n in w)
        out.append({n: x / s for x, n in w})
    return out


def bajo_el_aro(ob, pesos, hueso, etiqueta, centro, eje, desde=-0.052, hasta=-0.020, suelta=(0.025, 0.065)):
    """The fabric under a ring that rides on `hueso` follows that bone alone.

    `centro`, `eje`: the ring's centre and its axis (pointing down the limb). Along the axis, the
    weights pass from what they were at `desde` (toward the trunk) to the bone alone at `hasta`,
    and back to what they were between the two distances of `suelta`, past the ring. Only vertices
    of that limb (`etiqueta` > 0, the label the mesh was born with)."""
    eje = eje.normalized()
    tocados = 0
    for v, w in zip(ob.data.vertices, pesos):
        miembro = w.get(etiqueta, 0.0)
        if miembro <= 0.5:
            continue
        t = (v.co - centro).dot(eje)
        f = smoothstep(desde, hasta, t) * (1.0 - smoothstep(suelta[0], suelta[1], t)) * smoothstep(0.5, 0.9, miembro)
        if f <= 0.0:
            continue
        for n in list(w):
            if not n.startswith('T_'):
                w[n] *= 1.0 - f
        w[hueso] = w.get(hueso, 0.0) + f
        tocados += 1
    return tocados
