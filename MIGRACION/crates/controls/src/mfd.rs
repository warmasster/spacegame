//! Multi-function displays, as in military avionics: a screen whose pages are chosen with the
//! buttons all round its bezel (one bezel control, `bisel`, made for it: `expand`), each page
//! a few instruments drawn on the glass — values, bars, tanks, dial gauges, history graphs, maps of
//! zones, lines of text — each coloured by its zones (green normal, amber caution, red warning).
//! The legends of the buttons are written on the screen next to them; the page shown is boxed.
//!
//! A screen keeps what its look needs, refreshed at 10 Hz (graphs sample at 2 Hz, every page, so
//! a page shows its history the moment it comes up).
use crate::{
    def::{ControlDef, Page, Zones, named_color},
    indicator::Piece,
};
use lunar_signals::{
    Quality, SignalId, Store,
    units::{self, Unit},
};
use serde::Deserialize;

/// An instrument on a page, as written.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetDef {
    /// "valor", "barra", "deposito", "reloj", "grafica", "mapa", "texto" or "trazas".
    pub tipo: String,
    /// Plots ("trazas"): the name of the picture it shows, drawn by whatever system of its owner
    /// fills it (a radar's scope, a warner's ring): the screen only draws what is there.
    #[serde(default)]
    pub canal: Option<String>,
    #[serde(default)]
    pub senal: Option<String>,
    #[serde(default)]
    pub rotulo: Option<String>,
    /// Unit it shows in (else the signal's).
    #[serde(default)]
    pub unidad: Option<String>,
    #[serde(default)]
    pub escala: Option<[lunar_signals::Q; 2]>,
    #[serde(default)]
    pub decimales: Option<usize>,
    #[serde(default)]
    pub zonas: Option<Zones>,
    /// Graphs: seconds of history.
    #[serde(default)]
    pub ventana: Option<f64>,
    /// Cells it takes on the page's grid (columns, rows of `GRID`).
    #[serde(default)]
    pub ocupa: Option<[u32; 2]>,
    /// Text: lines (`{señal:dec}`, `{señal|A|B}` as in screens).
    #[serde(default)]
    pub lineas: Vec<String>,
    /// Maps: the zones drawn (a rectangle each, 0..1 of the widget), and what colours them.
    #[serde(default)]
    pub areas: Vec<AreaDef>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaDef {
    pub rotulo: String,
    pub senal: String,
    /// x, y, width, height (0..1 of the map, y up).
    pub rect: [f32; 4],
}

/// The page grid: columns and rows.
pub const GRID: [u32; 2] = [4, 3];
/// Bezel buttons per side.
pub const PER_SIDE: u32 = 5;
/// Width of the bezel strip round the glass (mm, nominal).
pub const STRIP: f32 = 16.0;

/// Where bezel button `k` (of `per` a side) is: side (0 left, 1 right, 2 bottom, 3 top) and its
/// place along it (0 first: top on the sides, left at the bottom and top).
pub fn slot(k: u32, per: u32) -> (u32, u32) {
    (k / per, k % per)
}

/// Its centre on a screen `w` × `h` (mm, the glass inside the strip, origin bottom-left), and the
/// strip's middle line outside the glass (button) as offsets: (legend point, button point).
pub fn slot_pos(k: u32, per: u32, w: f32, h: f32, strip: f32) -> ([f32; 2], [f32; 2]) {
    let (side, i) = slot(k, per);
    let f = (i as f32 + 0.5) / per as f32;
    match side {
        0 => ([0.0, h * (1.0 - f)], [-strip * 0.5, h * (1.0 - f)]),
        1 => ([w, h * (1.0 - f)], [w + strip * 0.5, h * (1.0 - f)]),
        2 => ([w * f, 0.0], [w * f, -strip * 0.5]),
        _ => ([w * f, h], [w * f, h + strip * 0.5]),
    }
}

#[derive(Clone, Debug)]
pub enum WKind {
    Value,
    Bar,
    Tank,
    Dial,
    Graph,
    Text(Vec<Vec<Piece>>),
    Map(Vec<(String, SignalId)>, Vec<[f32; 4]>),
    /// A picture drawn by its owner, by name.
    Plot(String),
}

#[derive(Clone, Debug)]
pub struct Widget {
    pub kind: WKind,
    pub label: String,
    pub signal: Option<SignalId>,
    pub unit: Unit,
    pub unit_name: String,
    pub lo: f64,
    pub hi: f64,
    pub decimals: usize,
    pub zones: Vec<(f64, f64, [u8; 3])>,
    /// Where on the page (0..1: x, y from the bottom, width, height).
    pub cell: [f32; 4],
    pub window: f32,
}

#[derive(Clone, Debug)]
pub struct MfdPage {
    pub title: String,
    /// Its bezel legend.
    pub button: String,
    pub widgets: Vec<Widget>,
}

#[derive(Clone, Debug)]
pub struct Mfd {
    pub pages: Vec<MfdPage>,
    pub page: Option<SignalId>,
    pub color: [u8; 3],
    pub per_side: u32,
}

/// One instrument now: its value (0..1 of its scale), text and colour; its history (graphs).
#[derive(Clone, Debug, Default)]
pub struct WState {
    pub u: f32,
    pub text: String,
    pub color: [u8; 3],
    /// Maps: each area's colour and text.
    pub areas: Vec<([u8; 3], String)>,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct MfdState {
    pub page: usize,
    pub widgets: Vec<WState>,
    /// Per page, per widget: its history (graphs only), oldest first.
    pub history: Vec<Vec<Vec<f32>>>,
    sample: f32,
}

/// Normal (white-cyan) when no zone holds it.
const PLAIN: [u8; 3] = [150, 230, 255];

fn zones(z: &Option<Zones>) -> Result<Vec<(f64, f64, [u8; 3])>, String> {
    let mut out = Vec::new();
    if let Some(z) = z {
        // red first: where zones overlap, the worst wins
        for (r, c) in [(&z.roja, "rojo"), (&z.ambar, "ambar"), (&z.verde, "verde")] {
            if let Some([a, b]) = r {
                out.push((a.si().map_err(|e| e.0)?, b.si().map_err(|e| e.0)?, named_color(c).unwrap_or([255; 3])));
            }
        }
    }
    Ok(out)
}

/// The colour of `v` by `zones` (its own, else plain).
pub fn zone_color(zones: &[(f64, f64, [u8; 3])], v: f64) -> [u8; 3] {
    zones.iter().find(|(a, b, _)| v >= a.min(*b) && v <= a.max(*b)).map_or(PLAIN, |z| z.2)
}

impl Mfd {
    pub fn build(d: &ControlDef, store: &Store, parse_line: impl Fn(&str) -> Result<Vec<Piece>, String>) -> Result<Mfd, String> {
        let mut pages = Vec::new();
        for p in &d.paginas {
            pages.push(page(p, store, &d.id, &parse_line)?);
        }
        if pages.is_empty() {
            return Err(format!("{}: una pantalla multifunción necesita páginas", d.id));
        }
        let page = match &d.pagina {
            Some(n) => Some(store.find(n).ok_or_else(|| format!("{}: señal de página desconocida '{n}'", d.id))?),
            None => None,
        };
        let color = d.color.as_ref().map_or(Ok([110, 255, 140]), crate::def::ColorDef::rgb)?;
        Ok(Mfd { pages, page, color, per_side: d.botones.unwrap_or(PER_SIDE).max(1) })
    }

    pub fn init(&self) -> MfdState {
        MfdState { page: 0, widgets: Vec::new(), history: self.pages.iter().map(|p| vec![Vec::new(); p.widgets.len()]).collect(), sample: 0.0 }
    }

    /// Follow the signals (the page shown at 10 Hz; graphs of every page at 2 Hz).
    pub fn update(&self, st: &mut MfdState, store: &Store, powered: bool, dt: f32, refresh: bool) {
        st.sample -= dt;
        if st.sample <= 0.0 {
            st.sample = 0.5;
            for (pi, p) in self.pages.iter().enumerate() {
                for (wi, w) in p.widgets.iter().enumerate() {
                    if let (WKind::Graph, Some(s)) = (&w.kind, w.signal) {
                        let h = &mut st.history[pi][wi];
                        h.push(store.get(s) as f32);
                        let keep = (w.window * 2.0).max(4.0) as usize;
                        if h.len() > keep {
                            h.drain(..h.len() - keep);
                        }
                    }
                }
            }
        }
        if !refresh {
            return;
        }
        st.page = self.page.map_or(0, |p| store.get(p).round().max(0.0) as usize).min(self.pages.len() - 1);
        let page = &self.pages[st.page];
        st.widgets.resize(page.widgets.len(), WState::default());
        if !powered {
            return;
        }
        for (w, ws) in page.widgets.iter().zip(st.widgets.iter_mut()) {
            ws.text.clear();
            match &w.kind {
                WKind::Text(lines) => {
                    ws.lines.resize(lines.len(), String::new());
                    for (l, out) in lines.iter().zip(ws.lines.iter_mut()) {
                        out.clear();
                        crate::indicator::render_line(l, store, out);
                    }
                    ws.color = PLAIN;
                }
                WKind::Plot(_) => ws.color = PLAIN,
                WKind::Map(areas, _) => {
                    ws.areas.resize(areas.len(), ([0; 3], String::new()));
                    for ((_, s), a) in areas.iter().zip(ws.areas.iter_mut()) {
                        let v = store.get(*s);
                        a.0 = zone_color(&w.zones, v);
                        a.1.clear();
                        units::format(v, "", &w.unit, w.decimals, &mut a.1);
                    }
                }
                _ => {
                    let Some(s) = w.signal else { continue };
                    let v = store.get(s);
                    if store.quality(s) == Quality::Failed || !v.is_finite() {
                        ws.text.push_str("----");
                        ws.color = [255, 40, 30];
                        ws.u = 0.0;
                        continue;
                    }
                    ws.u = ((v - w.lo) / (w.hi - w.lo)).clamp(0.0, 1.0) as f32;
                    ws.color = zone_color(&w.zones, v);
                    units::format(v, "", &w.unit, w.decimals, &mut ws.text);
                    if !w.unit_name.is_empty() {
                        ws.text.push(' ');
                        ws.text.push_str(&w.unit_name);
                    }
                }
            }
        }
    }
}

fn page(p: &Page, store: &Store, id: &str, parse_line: &impl Fn(&str) -> Result<Vec<Piece>, String>) -> Result<MfdPage, String> {
    let mut widgets = Vec::new();
    // lines of text alone are one text instrument over the page
    let mut defs = p.elementos.clone();
    if defs.is_empty() && !p.lineas.is_empty() {
        defs.push(WidgetDef { tipo: "texto".into(), lineas: p.lineas.clone(), ocupa: Some(GRID), ..Default::default() });
    }
    // flow on the grid: reading order, each taking its cells
    let mut taken = vec![vec![false; GRID[0] as usize]; GRID[1] as usize];
    for w in &defs {
        let [cw, ch] = w.ocupa.unwrap_or(default_span(&w.tipo));
        let (cw, ch) = (cw.clamp(1, GRID[0]), ch.clamp(1, GRID[1]));
        let spot = (0..=GRID[1] - ch).flat_map(|r| (0..=GRID[0] - cw).map(move |c| (c, r))).find(|&(c, r)| (r..r + ch).all(|y| (c..c + cw).all(|x| !taken[y as usize][x as usize])));
        let Some((c, r)) = spot else { return Err(format!("{id}: la página '{}' no tiene sitio para tanto", p.titulo)) };
        for y in r..r + ch {
            for x in c..c + cw {
                taken[y as usize][x as usize] = true;
            }
        }
        // rows from the top
        let cell = [c as f32 / GRID[0] as f32, 1.0 - (r + ch) as f32 / GRID[1] as f32, cw as f32 / GRID[0] as f32, ch as f32 / GRID[1] as f32];
        widgets.push(widget(w, store, id, cell, parse_line)?);
    }
    Ok(MfdPage { title: p.titulo.clone(), button: p.boton.clone().unwrap_or_else(|| p.titulo.chars().take(5).collect()), widgets })
}

fn default_span(kind: &str) -> [u32; 2] {
    match kind {
        "grafica" | "mapa" => [2, 2],
        "trazas" => [3, 3],
        "deposito" => [1, 2],
        "texto" => [2, 1],
        _ => [1, 1],
    }
}

fn widget(w: &WidgetDef, store: &Store, id: &str, cell: [f32; 4], parse_line: &impl Fn(&str) -> Result<Vec<Piece>, String>) -> Result<Widget, String> {
    let signal = match &w.senal {
        Some(n) => Some(store.find(n).ok_or_else(|| format!("{id}: señal desconocida '{n}'"))?),
        None => None,
    };
    let unit_name = w.unidad.clone().or_else(|| signal.map(|s| store.meta(s).show.to_string())).unwrap_or_default();
    let unit = units::unit(&unit_name).map_err(|e| format!("{id}: {}", e.0))?;
    let (lo, hi) = match &w.escala {
        Some([a, b]) => (a.si().map_err(|e| e.0)?, b.si().map_err(|e| e.0)?),
        None => (0.0, 1.0),
    };
    let kind = match w.tipo.as_str() {
        "valor" => WKind::Value,
        "barra" => WKind::Bar,
        "deposito" => WKind::Tank,
        "reloj" => WKind::Dial,
        "grafica" => WKind::Graph,
        "texto" => WKind::Text(w.lineas.iter().map(|l| parse_line(l)).collect::<Result<_, _>>()?),
        "mapa" => {
            let mut areas = Vec::new();
            let mut rects = Vec::new();
            for a in &w.areas {
                let s = store.find(&a.senal).ok_or_else(|| format!("{id}: señal desconocida '{}'", a.senal))?;
                areas.push((a.rotulo.clone(), s));
                rects.push(a.rect);
            }
            WKind::Map(areas, rects)
        }
        "trazas" => WKind::Plot(w.canal.clone().ok_or_else(|| format!("{id}: un instrumento 'trazas' dice qué enseña ('canal')"))?),
        t => return Err(format!("{id}: instrumento desconocido '{t}' (valor, barra, deposito, reloj, grafica, mapa, texto, trazas)")),
    };
    if signal.is_none() && !matches!(kind, WKind::Text(_) | WKind::Map(..) | WKind::Plot(_)) {
        return Err(format!("{id}: un instrumento '{}' necesita 'senal'", w.tipo));
    }
    Ok(Widget { kind, label: w.rotulo.clone().unwrap_or_default(), signal, unit, unit_name, lo, hi, decimals: w.decimales.unwrap_or(1), zones: zones(&w.zonas)?, cell, window: w.ventana.unwrap_or(120.0) as f32 })
}

/// Every multi-function display of a panel gets its bezel (unless the data gave it one): a
/// `bisel` control over it whose buttons choose its pages (it writes its page signal).
pub fn expand(p: &mut crate::layout::PanelDef) {
    let mut add = Vec::new();
    for d in &p.mandos {
        if d.kind != "mfd" || p.mandos.iter().any(|m| m.kind == "bisel" && m.protege.contains(&d.id)) {
            continue;
        }
        let mut b = ControlDef { id: format!("{}_bisel", d.id), kind: "bisel".into(), protege: vec![d.id.clone()], grupo: d.grupo.clone(), botones: d.botones, ..Default::default() };
        b.posiciones = d.paginas.iter().map(|pg| pg.boton.clone().unwrap_or_else(|| pg.titulo.chars().take(5).collect())).collect();
        b.bind.senal = d.pagina.clone();
        b.nombre = Some(format!("Botones de {}", d.label().to_lowercase()));
        b.ayuda = Some("Cada botón del marco muestra la página que tiene escrita al lado en la pantalla.".into());
        add.push(b);
    }
    p.mandos.extend(add);
}
