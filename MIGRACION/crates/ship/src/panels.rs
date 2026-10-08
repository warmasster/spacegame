//! The panels of a ship at work: each control's mechanism and state, its signal, the cover that
//! guards it, the switch it may be (breakers measure the real current through their edge), the
//! interlocks that refuse it; each indicator; each panel's power and data. A panel dies with the
//! part it is mounted on: its controls vanish and their signals fail.
//!
//! One mechanism may have a handle in several places (`bind.comun`: a valve's wheel on each side
//! of its bulkhead): the controls that share its signal are one group, a hand on any of them
//! moves them all, and the signal is written by the first of them whose panel still stands.
use crate::{
    blackbox::BlackBox,
    kind::{PanelPlan, ShipKind},
};
use glam::{Affine3A, Vec3};
use lunar_controls::{ControlState, Event, Gate, IndState, Indicator, Intent, Mechanism, Outcome, intent::Blocked, layout::Rect, mech};
use lunar_core::structure::state::Structure;
use lunar_machines::{Net, PortIo};
use lunar_signals::{Eval, Program, Quality, SignalId, Store, Writer, compile_in};

/// How far a hand reaches to work a control and an eye reads an instrument (m): the game's
/// (`aboard`) and the checks' (`diag`).
pub const REACH: f32 = 2.4;
pub const READ: f32 = 3.5;

pub struct ControlRt {
    pub panel: usize,
    /// Its definition: `kind.panels[panel].def.mandos[index]`.
    pub index: usize,
    /// `panel/control`.
    pub id: String,
    pub mech: Box<dyn Mechanism>,
    pub st: ControlState,
    pub sig: SignalId,
    pub sig2: Option<SignalId>,
    /// The cover that guards it.
    pub cover: Option<usize>,
    /// The switch it is (net, edge) and its rating (A), to sense the current through it.
    pub breaker: Option<(u16, usize, f64)>,
    /// Interlocks: (toward, condition, reason index).
    pub interlocks: Vec<(Option<f64>, Program, usize)>,
    /// Seconds the hand has held it down.
    pub held: f32,
    pub last: Option<Event>,
    /// Body rectangle on the panel (mm).
    pub rect: Rect,
    /// The group of controls it is one mechanism with (`Panels::groups`), if it shares its signal.
    pub group: Option<u16>,
}

pub struct IndicatorRt {
    pub panel: usize,
    pub index: usize,
    pub ind: Indicator,
    pub st: IndState,
    pub rect: Rect,
}

pub struct PanelRt {
    pub alive: bool,
    /// Power and data ports (indices into the ship's ports), and what they got last tick.
    pub power: Option<usize>,
    pub data: Option<usize>,
    pub powered: bool,
    pub linked: bool,
    pub draw: f64,
}

pub struct Panels {
    pub panels: Vec<PanelRt>,
    pub controls: Vec<ControlRt>,
    pub indicators: Vec<IndicatorRt>,
    pub reasons: Vec<String>,
    /// The controls that are one mechanism (they share their signal), group by group.
    pub groups: Vec<Vec<usize>>,
    /// What the hand is aimed at now, if anything of these panels: it shows (a faint glow round
    /// it), so there is no doubt what a click will work.
    pub aimed: Option<Aimed>,
    eval: Eval,
    slots: Vec<f64>,
}

/// What is aimed at: a control or an indicator, by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aimed {
    Control(usize),
    Indicator(usize),
}

impl Panels {
    pub fn new(kind: &ShipKind, store: &mut Store, ports: &mut Vec<PortIo>) -> Result<Panels, String> {
        let mut panels = Vec::new();
        let mut controls: Vec<ControlRt> = Vec::new();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut reasons = Vec::new();
        // first every control (their signals may be read by indicators of any panel)
        for (pi, plan) in kind.panels.iter().enumerate() {
            let power = plan.power.as_ref().map(|p| {
                ports.push(PortIo::on(p.net, p.node));
                ports.len() - 1
            });
            let data = plan.data.as_ref().map(|p| {
                ports.push(PortIo::on(p.net, p.node));
                ports.len() - 1
            });
            let draw: f64 = plan.def.mandos.iter().filter_map(|d| d.requiere.energia.as_ref()).filter_map(|q| q.si().ok()).sum::<f64>() + 5.0;
            panels.push(PanelRt { alive: true, power, data, powered: false, linked: false, draw });
            for (ci, d) in plan.def.mandos.iter().enumerate() {
                let Some(m) = mech::build(d).map_err(|e| format!("panel {}: {e}", plan.id))? else {
                    continue;
                };
                let id = format!("{}/{}", plan.id, d.id);
                let switch = d.bind.union.as_ref().and_then(|u| find_switch(kind, u));
                let name = match (&d.bind.senal, &switch) {
                    (Some(s), _) => s.clone(),
                    (None, Some((_, _, sig))) => sig.clone(),
                    (None, None) => format!("{}.{}", plan.id, d.id),
                };
                let sig = store.define(&name);
                let unit = d.unidad.clone().unwrap_or_default();
                if !unit.is_empty() {
                    let _ = store.define_unit(&name, &unit, 0.0);
                }
                // one mechanism with another handle already built: of its group, and its signal's
                // writer stays the first of them
                let twin = if d.bind.comun { controls.iter().position(|c| c.sig == sig && c.mech.kind() == m.kind() && kind.panels[c.panel].def.mandos[c.index].bind.comun) } else { None };
                let group = match twin {
                    Some(t) => {
                        let g = match controls[t].group {
                            Some(g) => g,
                            None => {
                                groups.push(vec![t]);
                                controls[t].group = Some((groups.len() - 1) as u16);
                                (groups.len() - 1) as u16
                            }
                        };
                        groups[usize::from(g)].push(controls.len());
                        Some(g)
                    }
                    None => {
                        store.claim(sig, Writer::Control(controls.len() as u32)).map_err(|e| format!("panel {}: {e}", plan.id))?;
                        None
                    }
                };
                let sig2 = match &d.bind.senal_y {
                    Some(n) => {
                        let s = store.define(n);
                        store.claim(s, Writer::Control(controls.len() as u32)).map_err(|e| format!("panel {}: {e}", plan.id))?;
                        Some(s)
                    }
                    None => None,
                };
                let st = m.init();
                store.set(sig, m.value(&st));
                let breaker = switch.map(|(net, edge, _)| {
                    let rating = d.nominal.as_ref().and_then(|q| q.si_as("A").ok()).unwrap_or(10.0);
                    (net, edge, rating)
                });
                controls.push(ControlRt { panel: pi, index: ci, id, mech: m, st, sig, sig2, cover: None, breaker, interlocks: Vec::new(), held: 0.0, last: None, rect: plan.layout.controls[ci], group });
            }
        }
        // covers: the control each guards
        for k in 0..controls.len() {
            let plan = &kind.panels[controls[k].panel];
            let d = &plan.def.mandos[controls[k].index];
            if d.kind == "tapa" {
                for g in &d.protege {
                    if let Some(t) = controls.iter().position(|c| c.panel == controls[k].panel && plan.def.mandos[c.index].id == *g) {
                        controls[t].cover = Some(k);
                    }
                }
            }
        }
        Ok(Panels { panels, controls, indicators: Vec::new(), reasons: Vec::new(), groups, aimed: None, eval: Eval::default(), slots: Vec::new() }.with_reasons(&mut reasons))
    }

    fn with_reasons(mut self, r: &mut Vec<String>) -> Panels {
        self.reasons = std::mem::take(r);
        self
    }

    /// Indicators and interlocks, once every signal of the ship exists.
    pub fn finish(&mut self, kind: &ShipKind, store: &Store) -> Result<(), String> {
        for (pi, plan) in kind.panels.iter().enumerate() {
            for (ci, d) in plan.def.mandos.iter().enumerate() {
                if let Some(ind) = Indicator::build(d, store).map_err(|e| format!("panel {}: {e}", plan.id))? {
                    let st = ind.init();
                    self.indicators.push(IndicatorRt { panel: pi, index: ci, ind, st, rect: plan.layout.controls[ci] });
                }
            }
        }
        for il in &kind.def.enclavamientos {
            let k = self.controls.iter().position(|c| c.id == il.mando).ok_or_else(|| format!("enclavamiento: mando desconocido '{}'", il.mando))?;
            let p = compile_in(&il.si, store).map_err(|e| format!("enclavamiento de {}: {}", il.mando, e.0))?;
            self.reasons.push(il.motivo.clone());
            let r = self.reasons.len() - 1;
            self.controls[k].interlocks.push((il.hacia, p, r));
        }
        let slots: usize = self.controls.iter().flat_map(|c| c.interlocks.iter().map(|i| i.1.slots)).sum();
        self.slots = vec![0.0; slots.max(1)];
        Ok(())
    }

    fn panel_part(kind: &ShipKind, p: usize) -> usize {
        kind.panels[p].part as usize
    }

    /// Controls into their signals (a dead panel's fail and read zero).
    pub fn write(&mut self, kind: &ShipKind, s: &Structure, store: &mut Store, ports: &[PortIo]) {
        for (k, p) in self.panels.iter_mut().enumerate() {
            p.alive = s.parts[Self::panel_part(kind, k)].alive;
            p.powered = p.power.is_none_or(|i| ports[i].fed && ports[i].share >= 0.5);
            p.linked = p.data.is_none_or(|i| ports[i].fed && ports[i].share >= 0.5);
        }
        for (k, c) in self.controls.iter().enumerate() {
            // (of the handles of one mechanism, the first whose panel still stands writes)
            if let Some(g) = c.group
                && self.groups[usize::from(g)].iter().copied().find(|&m| self.panels[self.controls[m].panel].alive).unwrap_or(self.groups[usize::from(g)][0]) != k
            {
                continue;
            }
            let p = &self.panels[c.panel];
            if !p.alive {
                store.set(c.sig, 0.0);
                store.set_quality(c.sig, Quality::Failed);
                continue;
            }
            if !p.linked {
                // no data: its commands do not get through, the last ones stand
                store.set_quality(c.sig, Quality::Stale);
                continue;
            }
            store.set(c.sig, c.mech.value(&c.st));
            if let (Some(s2), Some(v)) = (c.sig2, c.mech.value2(&c.st)) {
                store.set(s2, v);
            }
        }
    }

    /// What the panels ask of their power and data lines.
    pub fn plan(&mut self, _kind: &ShipKind, _s: &Structure, ports: &mut [PortIo]) {
        for p in &self.panels {
            if !p.alive {
                continue;
            }
            if let Some(i) = p.power {
                ports[i].demand = p.draw;
                ports[i].priority = 180;
            }
            if let Some(i) = p.data {
                ports[i].demand = 1.0;
                ports[i].priority = 250;
            }
        }
    }

    /// Indicators follow their signals (when someone may see them: `shown`; nothing else reads
    /// them); controls advance (springs, pulses, breakers on the current through them).
    #[allow(clippy::too_many_arguments)]
    pub fn after(&mut self, _kind: &ShipKind, _s: &Structure, store: &mut Store, _ports: &[PortIo], nets: &[Net], dt: f64, t: f64, bb: &mut BlackBox, shown: bool) {
        if shown {
            for i in &mut self.indicators {
                let p = &self.panels[i.panel];
                i.ind.update(&mut i.st, store, p.alive && p.powered, dt as f32, t, &mut self.eval);
            }
        }
        for c in &mut self.controls {
            let sense = match c.breaker {
                Some((net, edge, rating)) => {
                    let n = &nets[usize::from(net)];
                    let e = &n.edges[edge];
                    let isl = n.island.get(e.a as usize).copied().unwrap_or(u32::MAX);
                    let v = if isl == u32::MAX { 0.0 } else { n.islands[isl as usize].potential };
                    let amps = if v > 1.0 { e.flow / v } else { 0.0 };
                    (amps / rating.max(1e-3)) as f32
                }
                None => 0.0,
            };
            let o = c.mech.advance(&mut c.st, dt as f32, sense);
            if o.event == Some(Event::Trip) {
                bb.log(t, &format!("disyuntor {} saltado", c.id), 1);
            }
            if o.event.is_some() {
                c.last = o.event;
            }
        }
    }

    /// The gate a control works under now.
    pub fn gate(&self, k: usize, s: &Structure, kind: &ShipKind) -> Gate {
        let c = &self.controls[k];
        let p = &self.panels[c.panel];
        let part = &s.parts[Self::panel_part(kind, c.panel)];
        let uncovered = c.cover.is_none_or(|cv| self.controls[cv].mech.value(&self.controls[cv].st) >= 0.5);
        Gate { supply: if p.powered { 1.0 } else { 0.0 }, working: p.alive && part.working, uncovered, ..Gate::default() }
    }

    /// A hand's intent on control `k`. Interlocks are tried on the result and undo it.
    pub fn intent(&mut self, k: usize, i: &Intent, s: &Structure, kind: &ShipKind, store: &Store) -> Outcome {
        let gate = self.gate(k, s, kind);
        let before = self.controls[k].st;
        let v0 = self.controls[k].mech.value(&before);
        let c = &mut self.controls[k];
        let out = c.mech.intent(&mut c.st, i, &gate);
        let v1 = c.mech.value(&c.st);
        if (v1 - v0).abs() > 1e-12 {
            let mut off = 0;
            for (toward, prog, reason) in &c.interlocks {
                let n = prog.slots;
                let hit = toward.is_none_or(|t| (v1 - t).abs() < (v0 - t).abs());
                if hit && self.eval.run(prog, store, &mut self.slots[off..off + n], 0.0, 0.0) >= 0.5 {
                    c.st = before;
                    let o = Outcome::blocked(Blocked::Veto(*reason as u16));
                    c.last = o.event;
                    return o;
                }
                off += n;
            }
        }
        if out.event.is_some() {
            c.last = out.event;
        }
        // the other handles of the same mechanism go with it
        if let Some(g) = c.group {
            let st = c.st;
            for &m in &self.groups[usize::from(g)] {
                self.controls[m].st = st;
            }
        }
        out
    }

    /// Why an intent was blocked, for the HUD.
    pub fn reason(&self, b: Blocked) -> String {
        match b {
            Blocked::Covered => "Tapa cerrada".into(),
            Blocked::NoPower => "Sin energía".into(),
            Blocked::Broken => "Averiado".into(),
            Blocked::NoKey => "Falta la llave".into(),
            Blocked::Pull => "Tira del mando (mantén pulsado)".into(),
            Blocked::Gate => "Compuerta: levanta la palanca (Mayús)".into(),
            Blocked::Veto(r) => self.reasons.get(usize::from(r)).cloned().unwrap_or_else(|| "Enclavamiento".into()),
            Blocked::Stop => "Tope".into(),
            Blocked::Hot => "Disyuntor caliente: espera".into(),
        }
    }

    /// The panel frame now (posed by its part's bone): panel plane → ship frame.
    pub fn frame(_kind: &ShipKind, s: &Structure, plan: &PanelPlan) -> Affine3A {
        let part = &s.parts[plan.part as usize];
        if part.bone > 0 { s.bones[usize::from(part.bone)] * plan.frame } else { plan.frame }
    }

    /// The control a ship-frame ray hits first within `reach` (m): (control, sub-element, distance).
    pub fn pick(&self, kind: &ShipKind, s: &Structure, from: Vec3, dir: Vec3, reach: f32) -> Option<(usize, u8, f32)> {
        let mut best: Option<(usize, u8, f32)> = None;
        for (pi, plan) in kind.panels.iter().enumerate() {
            if !self.panels[pi].alive {
                continue;
            }
            let f = Self::frame(kind, s, plan);
            let inv = f.inverse();
            let (o, d) = (inv.transform_point3(from), inv.transform_vector3(dir));
            // controls stand up to ~6 cm off the plate: try the plate and a plane above it
            for lift in [0.03f32, 0.0] {
                if d.z.abs() < 1e-6 {
                    continue;
                }
                let t = (lift - o.z) / d.z;
                if t < 0.0 || t > reach || best.is_some_and(|b| t >= b.2) {
                    continue;
                }
                let hit = o + d * t;
                let mm = [hit.x * 1000.0, hit.y * 1000.0];
                for (k, c) in self.controls.iter().enumerate().filter(|(_, c)| c.panel == pi) {
                    let r = c.rect;
                    let pad = 3.0;
                    let within = |r: &lunar_controls::layout::Rect| mm[0] >= r.x - pad && mm[0] <= r.x + r.w + pad && mm[1] >= r.y - pad && mm[1] <= r.y + r.h + pad;
                    // an open cover lies back over the panel above what it guards: the hand finds
                    // it there (to shut it), and what it guards where it was
                    let open_cover = c.mech.kind() == "tapa" && c.mech.value(&c.st) >= 0.5;
                    let hit_here = if open_cover {
                        let lid = lunar_controls::layout::Rect { y: r.y + r.h, h: r.h * 0.97, ..r };
                        within(&lid) || (within(&r) && !self.controls.iter().any(|g| g.cover == Some(k) && within(&g.rect)))
                    } else {
                        within(&r)
                    };
                    if hit_here {
                        // a cover in the way takes the hit while closed... covers sit over what they guard
                        let k = match c.cover {
                            Some(cv) if self.controls[cv].mech.value(&self.controls[cv].st) < 0.5 => cv,
                            _ => k,
                        };
                        let elem = match self.controls[k].mech.kind() {
                            "teclado" => keypad_key(&self.controls[k].rect, mm),
                            "bisel" => {
                                let d = &kind.panels[pi].def.mandos[self.controls[k].index];
                                let scale = lunar_controls::layout::scale_of(d, kind.panels[pi].layout.scale);
                                bezel_key(&self.controls[k].rect, mm, d.botones.unwrap_or(lunar_controls::mfd::PER_SIDE).max(1), lunar_controls::mfd::STRIP * scale)
                            }
                            _ => 0,
                        };
                        if best.is_none_or(|b| t < b.2) {
                            best = Some((k, elem, t));
                        }
                    }
                }
            }
        }
        best
    }

    /// The indicator a ship-frame ray hits first within `reach` (m): (indicator, distance).
    pub fn pick_indicator(&self, kind: &ShipKind, s: &Structure, from: Vec3, dir: Vec3, reach: f32) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (pi, plan) in kind.panels.iter().enumerate() {
            if !self.panels[pi].alive {
                continue;
            }
            let f = Self::frame(kind, s, plan);
            let inv = f.inverse();
            let (o, d) = (inv.transform_point3(from), inv.transform_vector3(dir));
            if d.z.abs() < 1e-6 {
                continue;
            }
            let t = (0.004 - o.z) / d.z;
            if t < 0.0 || t > reach || best.is_some_and(|b| t >= b.1) {
                continue;
            }
            let hit = o + d * t;
            let mm = [hit.x * 1000.0, hit.y * 1000.0];
            for (i, ind) in self.indicators.iter().enumerate().filter(|(_, x)| x.panel == pi) {
                let r = ind.rect;
                if mm[0] >= r.x && mm[0] <= r.x + r.w && mm[1] >= r.y && mm[1] <= r.y + r.h {
                    best = Some((i, t));
                }
            }
        }
        best
    }

    /// What a hand finds on control `k`: what it is, how it stands, what it does, what keeps it.
    pub fn card_control(&self, kind: &ShipKind, store: &lunar_signals::Store, k: usize) -> Card {
        let c = &self.controls[k];
        let d = &kind.panels[c.panel].def.mandos[c.index];
        let mut value = String::new();
        c.mech.describe(&c.st, &mut value);
        let sig = store.name(c.sig).to_string();
        let unit = store.meta(c.sig).show.to_string();
        if !unit.is_empty() && !matches!(d.kind.as_str(), "interruptor" | "pulsador" | "tapa" | "disyuntor" | "bisel") {
            value.push_str("  (");
            store.format(c.sig, 1, &mut value);
            value.push(')');
        }
        let mut lines = Vec::new();
        match &d.ayuda {
            Some(a) => lines.push(a.clone()),
            None => lines.push(auto_help(d, &sig)),
        }
        if let Some(cv) = c.cover {
            let cd = &kind.panels[self.controls[cv].panel].def.mandos[self.controls[cv].index];
            lines.push(format!("Bajo tapa{}: ábrela primero.", if cd.precinto { " precintada" } else { "" }));
        }
        for (_, _, r) in &c.interlocks {
            lines.push(format!("Enclavamiento: {}", self.reasons.get(*r).map_or("", String::as_str)));
        }
        if !self.panels[c.panel].powered && d.requiere.energia.is_some() {
            lines.push("Sin energía en este panel.".into());
        }
        Card { title: d.label().to_string(), value, level: 0, lines }
    }

    /// What indicator `i` shows: its value in its unit, whether that is normal, its range.
    pub fn card_indicator(&self, kind: &ShipKind, store: &lunar_signals::Store, i: usize) -> Card {
        use lunar_controls::indicator::IndKind;
        let ind = &self.indicators[i];
        let d = &kind.panels[ind.panel].def.mandos[ind.index];
        let sig = d.senal.as_deref().or(d.bind.senal.as_deref()).and_then(|n| store.find(n));
        let mut value = String::new();
        let mut level = 0;
        let mut lines = Vec::new();
        let powered = self.panels[ind.panel].powered || d.requiere.energia.is_none();
        match &ind.ind.kind {
            IndKind::Mfd(m) => {
                let page = ind.st.mfd.page.min(m.pages.len().saturating_sub(1));
                value = format!("página {} de {}: {}", page + 1, m.pages.len(), m.pages.get(page).map_or("", |p| p.title.as_str()));
                lines.push(d.ayuda.clone().unwrap_or_else(|| "Pantalla multifunción: los botones del marco eligen la página escrita a su lado.".into()));
            }
            IndKind::Annunciator { cells, .. } => {
                let on: Vec<&str> = cells.iter().zip(&ind.st.cells).filter(|(_, s)| s.active).map(|(c, _)| c.label.as_str()).collect();
                value = if on.is_empty() { "sin avisos".into() } else { on.join(", ") };
                level = if ind.st.cells.iter().zip(cells).any(|(s, c)| s.active && c.level >= 2) {
                    2
                } else if on.is_empty() {
                    4
                } else {
                    1
                };
                lines.push("Avisos de la nave: ámbar precaución, rojo peligro (parpadea hasta que se reconoce).".into());
            }
            IndKind::Lamp { .. } => {
                value = if ind.st.lit > 0.0 { "encendida".into() } else { "apagada".into() };
                level = match ind.st.color {
                    [255, 40, 30] if ind.st.lit > 0.0 => 2,
                    [255, 170, 20] if ind.st.lit > 0.0 => 1,
                    _ => 0,
                };
                for r in &d.reglas {
                    lines.push(format!("{}: {}", r.color.as_ref().map_or("se enciende".into(), |c| format!("{c:?}").trim_matches('"').to_string()), humanize(&r.si)));
                }
            }
            _ => {
                if let Some(s) = sig {
                    store.format(s, d.decimales.unwrap_or(1), &mut value);
                    let v = store.get(s);
                    if let Some(z) = &d.zonas {
                        let zone = |r: &Option<[lunar_signals::Q; 2]>| r.as_ref().and_then(|[a, b]| Some((a.si().ok()?, b.si().ok()?))).is_some_and(|(a, b)| v >= a.min(b) && v <= a.max(b));
                        level = if zone(&z.roja) {
                            2
                        } else if zone(&z.ambar) {
                            1
                        } else if zone(&z.verde) {
                            4
                        } else {
                            0
                        };
                        let mut zl = Vec::new();
                        for (r, n) in [(&z.verde, "verde"), (&z.ambar, "ámbar"), (&z.roja, "rojo")] {
                            if let Some([a, b]) = r {
                                zl.push(format!("{n} {}–{}", q_text(a), q_text(b)));
                            }
                        }
                        lines.push(format!("Zonas: {}", zl.join(", ")));
                    }
                    if let Some([a, b]) = &d.escala {
                        lines.push(format!("Escala {}–{}", q_text(a), q_text(b)));
                    }
                }
                lines.insert(0, d.ayuda.clone().unwrap_or_else(|| sig.map_or_else(String::new, |s| format!("Indica {}.", humanize(store.name(s))))));
            }
        }
        if !powered {
            value = "apagado (sin energía)".into();
            level = 3;
        }
        let title = d.nombre.clone().or_else(|| d.rotulo.clone()).or_else(|| sig.map(|s| humanize(store.name(s)))).unwrap_or_else(|| d.id.clone());
        Card { title, value, level, lines }
    }

    /// "Nombre — valor" of a control, for the HUD.
    pub fn describe(&self, kind: &ShipKind, k: usize, out: &mut String) {
        let c = &self.controls[k];
        let d = &kind.panels[c.panel].def.mandos[c.index];
        out.push_str(d.label());
        out.push_str(" · ");
        c.mech.describe(&c.st, out);
    }
}

/// What the HUD shows of what the crosshair is on.
#[derive(Clone, Debug, Default)]
pub struct Card {
    pub title: String,
    pub value: String,
    /// 0 plain, 1 caution, 2 warning, 3 off, 4 normal (in its green zone).
    pub level: u8,
    pub lines: Vec<String>,
}

/// A quantity as written ("70 kPa").
fn q_text(q: &lunar_signals::Q) -> String {
    match q {
        lunar_signals::Q::N(v) => format!("{v}"),
        lunar_signals::Q::S(s) => s.clone(),
    }
}

/// A signal name or an expression as words: "cabina.p" → "cabina p", operators in words.
fn humanize(s: &str) -> String {
    s.replace(" && ", " y ").replace(" || ", " o ").replace(".p ", " presión ").replace('_', " ").replace(".", " ")
}

/// What a control does, from its kind, its positions and what it writes (when the data says
/// nothing).
fn auto_help(d: &lunar_controls::ControlDef, sig: &str) -> String {
    let pos = if d.posiciones.is_empty() { String::new() } else { format!(" ({})", d.posiciones.join(" / ")) };
    let what = match d.kind.as_str() {
        "interruptor" => "Interruptor",
        "pulsador" => "Pulsador",
        "selector" => "Selector",
        "rueda" => "Rueda",
        "volante" => "Volante (gíralo con la rueda del ratón)",
        "palanca" => "Palanca",
        "llave" => "Llave",
        "disyuntor" => "Disyuntor (sácalo para cortar; salta solo con sobrecarga)",
        "tapa" => "Tapa de protección",
        "teclado" => "Teclado (teclea y ENTER)",
        "bisel" => "Botones de pantalla",
        k => k,
    };
    format!("{what}{pos}: manda «{}».", humanize(sig))
}

/// The key of a keypad under `mm` (its body: 3 × 4 keys over the lower three quarters, ENTER
/// along the bottom).
fn keypad_key(r: &Rect, mm: [f32; 2]) -> u8 {
    let u = ((mm[0] - r.x) / r.w).clamp(0.0, 0.999);
    let v = ((mm[1] - r.y) / r.h).clamp(0.0, 0.999);
    if v < 0.14 {
        return 12;
    }
    let row = (((0.75 - v) / 0.61) * 4.0).clamp(0.0, 3.0) as u8;
    let col = (u * 3.0) as u8;
    row * 3 + col
}

/// The bezel button under `mm` (`per` a side, strips `strip` mm wide), 255 on the glass.
fn bezel_key(r: &Rect, mm: [f32; 2], per: u32, strip: f32) -> u8 {
    let (x, y) = (mm[0] - r.x, mm[1] - r.y);
    let (gw, gh) = (r.w - 2.0 * strip, r.h - 2.0 * strip);
    let along = |v: f32, len: f32| ((v / len.max(1e-3)) * per as f32).clamp(0.0, per as f32 - 0.001) as u32;
    let k = if x < strip {
        along(gh - (y - strip), gh)
    } else if x > r.w - strip {
        per + along(gh - (y - strip), gh)
    } else if y < strip {
        2 * per + along(x - strip, gw)
    } else if y > r.h - strip {
        3 * per + along(x - strip, gw)
    } else {
        return 255;
    };
    k.min(254) as u8
}

/// The switch called `id` in any network: (net, edge, its signal).
fn find_switch(kind: &ShipKind, id: &str) -> Option<(u16, usize, String)> {
    for (n, net) in kind.nets.iter().enumerate() {
        if let Some(e) = net.edges.iter().position(|e| e.id == id) {
            return Some((n as u16, e, net.edges[e].signal.clone().unwrap_or_else(|| id.to_string())));
        }
    }
    None
}
