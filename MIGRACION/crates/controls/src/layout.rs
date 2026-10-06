//! Panels (`docs/MANDOS_Y_MAQUINAS.md` §6): a plate with groups (a silkscreened frame and title)
//! and their controls. The layout is deterministic and made from the content:
//! 1. groups keep the arrangement the data gives (grid cells and spans; automatic groups and the
//!    controls of none take the first free cell);
//! 2. inside each group the controls go in lines (rows, or columns), by priority, as many to a
//!    line as makes the group closest to the panel's shape;
//! 3. every grid column and row is as wide and tall as what is in it: the plate is the content and
//!    a margin, nothing empty; `tamano` is the most room the panel has, and it takes the biggest
//!    scale (up to `MAX_SCALE`) that fits it;
//! 4. covers go over what they guard; labels under (or over) each control, shrunk to their slot;
//! 5. anything that does not fit is an error that says what and by how much.
//!
//! Everything here is millimetres from the bottom-left corner of the plate; the output
//! (`PanelLayout`) is what render and input consume.
use crate::def::ControlDef;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDef {
    pub id: String,
    #[serde(default)]
    pub titulo: Option<String>,
    /// "filas", "columnas", "flujo" or "prioridad".
    #[serde(default)]
    pub orden: Option<String>,
    #[serde(default)]
    pub celda: Option<[u32; 2]>,
    #[serde(default)]
    pub ocupa: Option<[u32; 2]>,
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub prioridad: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Background {
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub rugosidad: Option<u8>,
}

/// A panel as written in `panels/<id>.jsonc`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDef {
    pub name: String,
    /// Width and height ("0.62 m").
    pub tamano: [lunar_signals::Q; 2],
    /// Columns and rows.
    #[serde(default)]
    pub rejilla: Option<[u32; 2]>,
    #[serde(default)]
    pub margen: Option<lunar_signals::Q>,
    #[serde(default)]
    pub separacion: Option<lunar_signals::Q>,
    #[serde(default)]
    pub fondo: Background,
    /// Scale of the hand-worked controls; by default the biggest the plate fits.
    #[serde(default)]
    pub escala: Option<f32>,
    #[serde(default)]
    pub grupos: Vec<GroupDef>,
    pub mandos: Vec<ControlDef>,
    /// Derived signals this panel brings (`name: expression`).
    #[serde(default)]
    pub derivadas: std::collections::BTreeMap<String, String>,
}

/// Everything on a panel is drawn and laid out bigger than its nominal size (the sizes below and
/// `tamano` in the data are nominal): each panel takes the biggest scale up to `MAX_SCALE` its
/// room (`tamano`, the most the place it goes takes) fits, or the one its data asks (`escala`).
/// What the hand works grows the full scale; gauges, displays and keypads less; screens and
/// annunciators least.
pub const MAX_SCALE: f32 = 4.0;

/// How much bigger than nominal a control is drawn on a panel laid out at scale `s`.
pub fn scale_of(d: &ControlDef, s: f32) -> f32 {
    match d.kind.as_str() {
        "pulsador" | "interruptor" | "selector" | "rueda" | "volante" | "palanca" | "tapa" | "llave" | "disyuntor" | "lampara" => s,
        "teclado" => (1.0 + (s - 1.0) * 0.5).min(2.2),
        "aguja" | "barra" | "display7" | "contador" => 1.0 + (s - 1.0) * 0.6,
        // a multi-function display and its bezel grow together
        "mfd" | "bisel" => 1.0 + (s - 1.0) * 0.3,
        _ => 1.0 + (s - 1.0) * 0.3,
    }
}

/// Footprint of a control kind (mm, width × height of its body, as drawn at panel scale `s`).
pub fn footprint(d: &ControlDef, s: f32) -> [f32; 2] {
    let [w, h] = nominal(d);
    let k = scale_of(d, s);
    [w * k, h * k]
}

/// Footprint of a control kind at its nominal size (mm).
pub fn nominal(d: &ControlDef) -> [f32; 2] {
    if let Some(t) = d.tamano {
        // a multi-function display's size is its glass: the bezel's strip goes round it
        if matches!(d.kind.as_str(), "mfd" | "bisel") {
            return [t[0] + 2.0 * crate::mfd::STRIP, t[1] + 2.0 * crate::mfd::STRIP];
        }
        return t;
    }
    match d.kind.as_str() {
        "pulsador" => {
            if d.modo.as_deref() == Some("seta") {
                [34.0, 34.0]
            } else {
                [20.0, 20.0]
            }
        }
        "interruptor" => [18.0, 30.0],
        "selector" => [30.0, 30.0],
        "rueda" => [34.0, 34.0],
        // a valve's hand wheel: a hand takes it whole
        "volante" => [56.0, 56.0],
        "palanca" => {
            if d.ejes == Some(2) {
                [70.0, 70.0]
            } else {
                [32.0, 96.0]
            }
        }
        "tapa" => [24.0, 34.0],
        "llave" => [26.0, 26.0],
        "teclado" => [62.0, 92.0],
        "disyuntor" => [16.0, 26.0],
        "lampara" => [14.0, 14.0],
        "aguja" => [64.0, 64.0],
        "barra" => [16.0, 64.0],
        "display7" => [(d.digitos.unwrap_or(4) as f32 * 11.0 + 10.0).max(30.0), 22.0],
        "contador" => [d.digitos.unwrap_or(4) as f32 * 9.0 + 8.0, 16.0],
        "pantalla" => [130.0, 96.0],
        // its glass and the strip of buttons round it
        "mfd" | "bisel" => [150.0 + 2.0 * crate::mfd::STRIP, 112.0 + 2.0 * crate::mfd::STRIP],
        "anunciador" => {
            let cols = d.columnas.unwrap_or(d.avisos.len().clamp(1, 6) as u32).max(1) as f32;
            let rows = (d.avisos.len() as f32 / cols).ceil().max(1.0);
            [cols * 30.0 + 4.0, rows * 13.0 + 4.0]
        }
        _ => [20.0, 20.0],
    }
}

/// Room round a control's body (mm).
pub const CLEARANCE: f32 = 4.0;
/// Label height (mm, at scale 1) and the gap to its control.
pub const LABEL: f32 = 4.2;
pub const LABEL_GAP: f32 = 2.0;

/// Label height at panel scale `s` (mm): labels grow with the controls, less fast.
pub fn label_size(s: f32) -> f32 {
    LABEL * (1.0 + (s - 1.0) * 0.5)
}
/// Group frame: title height (at scale 1) and inner padding (mm).
pub const TITLE: f32 = 8.5;
pub const PAD: f32 = 3.5;

/// A group's title band at panel scale `s` (mm): it grows with the labels, so that the title is
/// always the biggest lettering of its group.
pub fn title_room(s: f32) -> f32 {
    TITLE * (1.0 + (s - 1.0) * 0.5)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w - 0.01 && o.x < self.x + self.w - 0.01 && self.y < o.y + o.h - 0.01 && o.y < self.y + self.h - 0.01
    }
    pub fn center(&self) -> [f32; 2] {
        [self.x + self.w * 0.5, self.y + self.h * 0.5]
    }
    /// Inside `o` (with a hair of slack).
    pub fn within(&self, o: &Rect) -> bool {
        self.x >= o.x - 0.01 && self.y >= o.y - 0.01 && self.x + self.w <= o.x + o.w + 0.01 && self.y + self.h <= o.y + o.h + 0.01
    }
}

/// A text on the plate: what, where (centre, mm) and how tall (mm).
#[derive(Clone, Debug, PartialEq)]
pub struct TextQuad {
    pub text: String,
    pub at: [f32; 2],
    pub size: f32,
    /// Which control it belongs to (scale marks, labels), if any.
    pub control: Option<usize>,
}

/// Where everything of a panel went.
#[derive(Clone, Debug, Default)]
pub struct PanelLayout {
    /// Plate size (mm): its content and a margin round it, no more.
    pub size: [f32; 2],
    /// Body rectangle of each control, in definition order.
    pub controls: Vec<Rect>,
    pub texts: Vec<TextQuad>,
    /// Group frames.
    pub frames: Vec<Rect>,
    /// Scale of the panel (`scale_of` says what it means for each kind).
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutError(pub String);

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn mm(q: &Option<lunar_signals::Q>, or: f32) -> Result<f32, LayoutError> {
    match q {
        Some(v) => v.si_as("m").map(|m| m as f32 * 1000.0).map_err(|e| LayoutError(e.0)),
        None => Ok(or),
    }
}

/// Labels go under their control unless the data says "arriba".
fn label_over(d: &ControlDef) -> bool {
    d.rotulo_en.as_deref() == Some("arriba")
}

/// Room a control's label takes (mm), 0 without one.
fn label_room(d: &ControlDef, s: f32) -> f32 {
    if d.rotulo.is_some() { label_size(s) + LABEL_GAP } else { 0.0 }
}

/// The cover guarding control `i` of `p`, if any.
pub fn guard_of(p: &PanelDef, i: usize) -> Option<usize> {
    let id = &p.mandos[i].id;
    p.mandos.iter().position(|m| m.kind == "tapa" && m.protege.contains(id))
}

/// Size of cover `c` of `p` at scale `s` (mm): its own, or what it guards and its walls round
/// it (a mushroom button gets a cover as big as its cap).
pub fn cover_size(p: &PanelDef, c: usize, s: f32) -> [f32; 2] {
    let own = footprint(&p.mandos[c], s);
    let k = scale_of(&p.mandos[c], s);
    p.mandos[c].protege.iter().filter_map(|t| p.mandos.iter().find(|m| &m.id == t)).fold(own, |acc, m| {
        let f = footprint(m, s);
        // walls 1.6 mm, the hinge bracket 2.4 mm, a hair round it (nominal)
        [acc[0].max(f[0] + 4.4 * k), acc[1].max(f[1] + 5.4 * k)]
    })
}

/// Room kept free over a guarded control (mm, at scale `s`): where its cover lies open, laid
/// back over the panel above (`mech::cover_reach`), less what the slot has there anyway.
fn guard_room(p: &PanelDef, i: usize, s: f32) -> f32 {
    let Some(c) = guard_of(p, i) else { return 0.0 };
    let cover = cover_size(p, c, s);
    let body = footprint(&p.mandos[i], s);
    let reach = crate::mech::cover_reach(cover[1] / scale_of(&p.mandos[c], s)) * scale_of(&p.mandos[c], s);
    (reach + (cover[1] - body[1]) * 0.5 - CLEARANCE).max(0.0)
}

/// The slot control `i` needs: body plus clearance plus its label (and its cover's room open).
fn slot(p: &PanelDef, i: usize, s: f32) -> [f32; 2] {
    let d = &p.mandos[i];
    let f = footprint(d, s);
    [f[0] + 2.0 * CLEARANCE, f[1] + 2.0 * CLEARANCE + label_room(d, s) + guard_room(p, i, s)]
}

/// Lay out a panel: at the scale its data asks, else at the biggest (from `MAX_SCALE` down to 1
/// in tenths) at which its plate fits the room it has (`tamano`).
pub fn layout(p: &PanelDef) -> Result<PanelLayout, LayoutError> {
    let room = [mm(&Some(p.tamano[0].clone()), 0.0)?, mm(&Some(p.tamano[1].clone()), 0.0)?];
    let fits = |l: &PanelLayout| l.size[0] <= room[0] + 0.5 && l.size[1] <= room[1] + 0.5;
    if let Some(s) = p.escala {
        let l = layout_at(p, s.max(0.5))?;
        return if fits(&l) { Ok(l) } else { Err(too_big(p, &l, room)) };
    }
    let mut s = MAX_SCALE;
    while s > 1.0 + 1e-3 {
        if let Ok(l) = layout_at(p, s)
            && fits(&l)
        {
            return Ok(l);
        }
        s -= 0.1;
    }
    let l = layout_at(p, 1.0)?;
    if fits(&l) { Ok(l) } else { Err(too_big(p, &l, room)) }
}

fn too_big(p: &PanelDef, l: &PanelLayout, room: [f32; 2]) -> LayoutError {
    LayoutError(format!("{}: no cabe: pide {:.0}×{:.0} mm y tiene {:.0}×{:.0} mm", p.name, l.size[0], l.size[1], room[0], room[1]))
}

/// A group as laid out: its members and the size they take.
struct Group {
    title: Option<String>,
    members: Vec<usize>,
    /// Cell and span on the panel's grid.
    cell: [u32; 2],
    span: [u32; 2],
    /// Members per row (or per column), and the natural size (mm, frame included).
    per_line: usize,
    columns: bool,
    size: [f32; 2],
}

/// Members laid in lines of `k` (rows, or columns): the block's size (mm, without the frame).
fn block(p: &PanelDef, members: &[usize], k: usize, columns: bool, s: f32) -> [f32; 2] {
    let (mut w, mut h) = (0.0f32, 0.0f32);
    for line in members.chunks(k.max(1)) {
        let slots: Vec<[f32; 2]> = line.iter().map(|&i| slot(p, i, s)).collect();
        if columns {
            w += slots.iter().map(|x| x[0]).fold(0.0, f32::max);
            h = h.max(slots.iter().map(|x| x[1]).sum());
        } else {
            w = w.max(slots.iter().map(|x| x[0]).sum());
            h += slots.iter().map(|x| x[1]).fold(0.0, f32::max);
        }
    }
    [w, h]
}

/// Lay out a panel at scale `s`. The groups keep the arrangement the data gives them (their grid
/// cells and spans; automatic ones take the first free cell), but each grid column and row is as
/// wide and tall as what is in it: the plate is its content and a margin, nothing empty.
pub fn layout_at(p: &PanelDef, scale: f32) -> Result<PanelLayout, LayoutError> {
    let margin = mm(&p.margen, 10.0)?;
    let gap = mm(&p.separacion, 5.0)?;
    let [mut cols, mut rows] = p.rejilla.unwrap_or([12, 8]);
    if let Some(d) = p.mandos.iter().find(|d| d.fijo.is_some()) {
        return Err(LayoutError(format!("{}: '{}' tiene sitio fijo: los paneles se ajustan a su contenido (usa grupos)", p.name, d.id)));
    }
    let room = [mm(&Some(p.tamano[0].clone()), 0.0)?, mm(&Some(p.tamano[1].clone()), 0.0)?];
    let aspect = (room[0] / room[1].max(1.0)).clamp(0.2, 5.0);
    // ---- groups: the data's, and one for the controls of no group ----
    let mut groups: Vec<Group> = Vec::new();
    let mut taken: Vec<Vec<bool>> = vec![vec![false; cols as usize]; rows as usize];
    let mut fixed = Vec::new();
    let mut auto = Vec::new();
    for (gi, g) in p.grupos.iter().enumerate() {
        if g.celda.is_some() && !g.auto { fixed.push(gi) } else { auto.push(gi) }
    }
    auto.sort_by_key(|&g| (p.grupos[g].prioridad.unwrap_or(0), g));
    for (i, d) in p.mandos.iter().enumerate() {
        if let Some(g) = &d.grupo
            && !p.grupos.iter().any(|x| &x.id == g)
        {
            return Err(LayoutError(format!("{}: '{}' cita el grupo desconocido '{g}'", p.name, d.id)));
        }
        let _ = i;
    }
    let members_of = |id: Option<&str>| -> Vec<usize> {
        let mut m: Vec<usize> = (0..p.mandos.len()).filter(|&i| p.mandos[i].grupo.as_deref() == id && !matches!(p.mandos[i].kind.as_str(), "tapa" | "bisel")).collect();
        if let Some(g) = id.and_then(|id| p.grupos.iter().find(|g| g.id == id))
            && g.orden.as_deref() == Some("prioridad")
        {
            m.sort_by_key(|&i| (p.mandos[i].prioridad.unwrap_or(100), i));
        }
        m
    };
    let place = |cell: [u32; 2], span: [u32; 2], taken: &mut Vec<Vec<bool>>, what: &str, cols: u32, rows: u32| -> Result<(), LayoutError> {
        for r in cell[1]..cell[1] + span[1] {
            for c in cell[0]..cell[0] + span[0] {
                if c >= cols || r >= rows {
                    return Err(LayoutError(format!("{}: el grupo '{what}' se sale de la rejilla", p.name)));
                }
                if taken[r as usize][c as usize] {
                    return Err(LayoutError(format!("{}: el grupo '{what}' se solapa con otro", p.name)));
                }
                taken[r as usize][c as usize] = true;
            }
        }
        Ok(())
    };
    let new_group = |title: Option<String>, members: Vec<usize>, cell: [u32; 2], span: [u32; 2], orden: Option<&str>, lines: Option<usize>| -> Group {
        let columns = orden == Some("columnas");
        let n = members.len().max(1);
        // members per line: what the data says, else the shape closest to the panel's
        let k = lines.unwrap_or_else(|| {
            (1..=n)
                .min_by(|&a, &b| {
                    let cost = |k: usize| {
                        let [w, h] = block(p, &members, k, columns, scale);
                        let r = (w / h.max(1.0)) / aspect;
                        (w * h) * (if r > 1.0 { r } else { 1.0 / r })
                    };
                    cost(a).total_cmp(&cost(b))
                })
                .unwrap_or(1)
        });
        let [w, h] = block(p, &members, k, columns, scale);
        let size = [w + 2.0 * PAD, h + 2.0 * PAD + if title.is_some() { title_room(scale) } else { 0.0 }];
        Group { title, members, cell, span, per_line: k, columns, size }
    };
    for gi in fixed {
        let g = &p.grupos[gi];
        let (cell, span) = (g.celda.unwrap_or([0, 0]), g.ocupa.unwrap_or([1, 1]));
        place(cell, span, &mut taken, &g.id, cols, rows)?;
        groups.push(new_group(g.titulo.clone(), members_of(Some(&g.id)), cell, span, g.orden.as_deref(), None));
    }
    // automatic groups (and the controls of none): the first free cell, else a new row
    let mut loose: Vec<(Option<String>, Vec<usize>, Option<String>, Option<usize>)> = auto.iter().map(|&gi| (p.grupos[gi].titulo.clone(), members_of(Some(&p.grupos[gi].id)), p.grupos[gi].orden.clone(), None)).collect();
    let rest = members_of(None);
    if !rest.is_empty() {
        // a panel of no groups lays its controls `rejilla` columns to a row
        let k = if p.grupos.is_empty() { p.rejilla.map(|[c, _]| c as usize) } else { None };
        loose.push((None, rest, None, k));
    }
    for (title, members, orden, k) in loose {
        let free = (0..rows).flat_map(|r| (0..cols).map(move |c| [c, r])).find(|&[c, r]| !taken[r as usize][c as usize]);
        let cell = match free {
            Some(c) => c,
            None => {
                rows += 1;
                taken.push(vec![false; cols as usize]);
                [0, rows - 1]
            }
        };
        place(cell, [1, 1], &mut taken, title.as_deref().unwrap_or("(sin grupo)"), cols, rows)?;
        groups.push(new_group(title, members, cell, [1, 1], orden.as_deref(), k));
    }
    cols = cols.max(1);
    // ---- the grid: each column and row as big as what is in it ----
    let (mut cw, mut rh) = (vec![0.0f32; cols as usize], vec![0.0f32; rows as usize]);
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by_key(|&g| groups[g].span[0] * groups[g].span[1]);
    let used = |v: &[f32], a: u32, n: u32| {
        let s = &v[a as usize..(a + n) as usize];
        s.iter().sum::<f32>() + gap * (s.iter().filter(|x| **x > 0.0).count().max(1) - 1) as f32
    };
    for _ in 0..3 {
        for &g in &order {
            let gr = &groups[g];
            let need_w = gr.size[0] - used(&cw, gr.cell[0], gr.span[0]);
            if need_w > 0.01 {
                for c in gr.cell[0]..gr.cell[0] + gr.span[0] {
                    cw[c as usize] += need_w / gr.span[0] as f32;
                }
            }
            let need_h = gr.size[1] - used(&rh, gr.cell[1], gr.span[1]);
            if need_h > 0.01 {
                for r in gr.cell[1]..gr.cell[1] + gr.span[1] {
                    rh[r as usize] += need_h / gr.span[1] as f32;
                }
            }
        }
    }
    // positions: columns from the left, rows from the top; empty ones take nothing
    let starts = |v: &[f32]| {
        let mut out = Vec::with_capacity(v.len() + 1);
        let mut at = margin;
        for x in v {
            out.push(at);
            if *x > 0.0 {
                at += x + gap;
            }
        }
        out.push(at - if v.iter().any(|x| *x > 0.0) { gap } else { 0.0 });
        out
    };
    let (xs, ys) = (starts(&cw), starts(&rh));
    let w = xs[cols as usize] + margin;
    let h = ys[rows as usize] + margin;
    let mut out = PanelLayout { size: [w, h], controls: vec![Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }; p.mandos.len()], scale, ..Default::default() };
    // ---- each group in its cells, its members centred in it ----
    for gr in &groups {
        let x0 = xs[gr.cell[0] as usize];
        let x1 = xs[(gr.cell[0] + gr.span[0]) as usize] - if xs[(gr.cell[0] + gr.span[0]) as usize] < xs[cols as usize] { gap } else { 0.0 };
        let top = ys[gr.cell[1] as usize];
        let bottom = ys[(gr.cell[1] + gr.span[1]) as usize] - if ys[(gr.cell[1] + gr.span[1]) as usize] < ys[rows as usize] { gap } else { 0.0 };
        // y grows up from the plate's bottom edge
        let rect = Rect { x: x0, y: h - bottom, w: (x1 - x0).max(gr.size[0]), h: (bottom - top).max(gr.size[1]) };
        if gr.title.is_some() || !p.grupos.is_empty() {
            out.frames.push(rect);
        }
        if let Some(t) = &gr.title {
            let band = title_room(scale);
            out.texts.push(TextQuad { text: t.clone(), at: [rect.x + rect.w * 0.5, rect.y + rect.h - band * 0.55], size: band * 0.76, control: None });
        }
        let area = Rect { x: rect.x + PAD, y: rect.y + PAD, w: rect.w - 2.0 * PAD, h: rect.h - 2.0 * PAD - if gr.title.is_some() { title_room(scale) } else { 0.0 } };
        let [bw, bh] = block(p, &gr.members, gr.per_line, gr.columns, scale);
        let (bx, btop) = (area.x + (area.w - bw) * 0.5, area.y + area.h - (area.h - bh) * 0.5);
        // lines from the top-left of the block, each line centred across
        let (mut along, mut across) = (0.0f32, 0.0f32);
        for line in gr.members.chunks(gr.per_line.max(1)) {
            let slots: Vec<[f32; 2]> = line.iter().map(|&i| slot(p, i, scale)).collect();
            let thick = if gr.columns { slots.iter().map(|x| x[0]).fold(0.0, f32::max) } else { slots.iter().map(|x| x[1]).fold(0.0, f32::max) };
            let length: f32 = if gr.columns { slots.iter().map(|x| x[1]).sum() } else { slots.iter().map(|x| x[0]).sum() };
            let mut run = ((if gr.columns { bh } else { bw }) - length) * 0.5;
            for (&i, s) in line.iter().zip(&slots) {
                let d = &p.mandos[i];
                let fp = footprint(d, scale);
                // the slot's top-left
                let (sx, sy_top) = if gr.columns { (bx + across + (thick - s[0]) * 0.5, btop - run) } else { (bx + run, btop - across - (thick - s[1]) * 0.5) };
                let over = if label_over(d) { label_room(d, scale) } else { 0.0 } + guard_room(p, i, scale);
                out.controls[i] = Rect { x: sx + (s[0] - fp[0]) * 0.5, y: sy_top - CLEARANCE - over - fp[1], w: fp[0], h: fp[1] };
                run += if gr.columns { s[1] } else { s[0] };
            }
            across += thick;
            along += length;
        }
        let _ = along;
    }
    // covers (and bezels): over the first control they guard
    for (i, d) in p.mandos.iter().enumerate() {
        if !matches!(d.kind.as_str(), "tapa" | "bisel") {
            continue;
        }
        let target = d.protege.first().and_then(|t| p.mandos.iter().position(|m| &m.id == t));
        let k = target.ok_or_else(|| LayoutError(format!("{}: la tapa '{}' protege un mando que no está en el panel", p.name, d.id)))?;
        let fp = if d.kind == "bisel" { [out.controls[k].w, out.controls[k].h] } else { cover_size(p, i, scale) };
        let c = out.controls[k].center();
        out.controls[i] = Rect { x: c[0] - fp[0] * 0.5, y: c[1] - fp[1] * 0.5, w: fp[0], h: fp[1] };
    }
    // nothing overlaps (a cover sits over what it guards), nothing leaves the plate
    let plate = Rect { x: 0.0, y: 0.0, w, h };
    for a in 0..out.controls.len() {
        if !out.controls[a].within(&plate) {
            return Err(LayoutError(format!("{}: '{}' se sale del panel", p.name, p.mandos[a].id)));
        }
        for b in a + 1..out.controls.len() {
            let guards = |x: usize, y: usize| matches!(p.mandos[x].kind.as_str(), "tapa" | "bisel") && p.mandos[x].protege.contains(&p.mandos[y].id);
            if out.controls[a].overlaps(&out.controls[b]) && !guards(a, b) && !guards(b, a) {
                return Err(LayoutError(format!("{}: '{}' se solapa con '{}'", p.name, p.mandos[a].id, p.mandos[b].id)));
            }
        }
    }
    // labels under (or over) their controls, shrunk to their slot
    for (i, d) in p.mandos.iter().enumerate() {
        let Some(t) = &d.rotulo else { continue };
        let r = out.controls[i];
        let below = !label_over(d);
        let lh = label_size(scale);
        let y = if below { r.y - LABEL_GAP - lh * 0.5 } else { r.y + r.h + LABEL_GAP + lh * 0.5 };
        // a glyph is about 0.62 of its height wide
        let room = (r.w + 2.0 * CLEARANCE).max(16.0);
        let size = (room / (t.chars().count() as f32 * 0.62)).min(lh);
        out.texts.push(TextQuad { text: t.clone(), at: [r.x + r.w * 0.5, y], size, control: Some(i) });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LM: &str = r#"{
      "name": "Cabina LM", "tamano": ["0.62 m", "0.40 m"], "rejilla": [12, 8], "margen": "12 mm", "separacion": "6 mm",
      "grupos": [
        { "id": "motor", "titulo": "MOTOR PRINCIPAL", "orden": "filas", "celda": [0, 0], "ocupa": [6, 5] },
        { "id": "energia", "titulo": "ENERGÍA", "orden": "prioridad", "auto": true },
        { "id": "avisos", "titulo": "AVISOS", "orden": "flujo", "celda": [0, 6], "ocupa": [12, 2] }
      ],
      "mandos": [
        { "id": "tapa_armado", "kind": "tapa", "grupo": "motor", "protege": ["armado"] },
        { "id": "armado", "kind": "interruptor", "grupo": "motor", "posiciones": ["SEGURO", "ARMADO"], "rotulo": "ARMADO" },
        { "id": "arranque", "kind": "interruptor", "grupo": "motor", "posiciones": ["PARO", "MARCHA", "ARRANQUE"], "rotulo": "ARRANQUE" },
        { "id": "acelerador", "kind": "palanca", "grupo": "motor", "rango": [0.0, 1.05] },
        { "id": "empuje", "kind": "aguja", "grupo": "motor", "senal": "x", "escala": ["0 kN", "50 kN"] },
        { "id": "t_camara", "kind": "display7", "grupo": "motor", "senal": "x" },
        { "id": "bat", "kind": "disyuntor", "grupo": "energia", "prioridad": 1, "rotulo": "BAT" },
        { "id": "soc", "kind": "barra", "grupo": "energia", "prioridad": 2, "senal": "x" },
        { "id": "consigna_T", "kind": "rueda", "grupo": "energia", "prioridad": 3, "rango": ["250 °C", "650 °C"] },
        { "id": "aviso", "kind": "anunciador", "grupo": "avisos", "avisos": [
            { "rotulo": "P ALIM", "si": "x" }, { "rotulo": "T CÁMARA", "si": "x" }, { "rotulo": "SCRAM", "si": "x" } ] }
      ]
    }"#;

    #[test]
    fn example_panel_lays_out_without_overlaps() {
        let p: PanelDef = serde_json::from_str(LM).unwrap();
        let l = layout(&p).unwrap();
        assert_eq!(l.controls.len(), 10);
        assert_eq!(l.frames.len(), 3);
        for (i, r) in l.controls.iter().enumerate() {
            assert!(r.w > 0.0 && r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= 620.0 && r.y + r.h <= 400.0, "{} {:?}", p.mandos[i].id, r);
        }
        // deterministic
        let again = layout(&p).unwrap();
        assert_eq!(l.controls, again.controls);
        assert!(l.texts.iter().any(|t| t.text == "MOTOR PRINCIPAL"));
    }

    #[test]
    fn too_much_is_a_clear_error() {
        let mut p: PanelDef = serde_json::from_str(LM).unwrap();
        for k in 0..40 {
            p.mandos.push(serde_json::from_str(&format!(r#"{{ "id": "extra{k}", "kind": "aguja", "grupo": "motor", "senal": "x", "escala": [0, 1] }}"#)).unwrap());
        }
        let e = layout(&p).unwrap_err();
        assert!(e.0.contains("no cabe"), "{}", e.0);
    }
}
