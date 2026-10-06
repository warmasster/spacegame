//! The ship editor in play (F6, debug build): the ship you are on (or the nearest) as a document
//! (`lunar_editor::Doc`), worked with the same operations the MCP server gives models.
//! - Click a part with the mouse: its component is selected (outlined); a cable or pipe selects its
//!   network's run, for routing it by hand.
//! - The window: move (arrows of 5 cm, 1 cm with Ctrl in the field), turn, duplicate, remove,
//!   wired or not, a component of the catalog placed where you look, a point the selected run
//!   must pass by (at the crosshair), undo and redo, check (builds, tight, worked from every room,
//!   cables), apply (the ship rebuilt in place) and save (a copy of the old file kept).
use crate::{builds::Builds, ships::Ships};
use glam::{DVec3, Quat, Vec3};
use lunar_core::{
    body::BodyRegistry,
    props::{BOX, Prop, PropFrame, PropScene},
};
use lunar_editor::{Doc, Op};
use std::sync::Arc;

#[derive(Default)]
pub struct Editor {
    pub open: bool,
    doc: Option<Doc>,
    /// The structure of the ship being edited.
    ship: Option<u64>,
    selected: Option<String>,
    /// A network's run being routed by hand: (net, run index).
    run: Option<(String, usize)>,
    status: String,
    report: String,
    kinds: Vec<String>,
    kind: usize,
    step: f32,
    /// Where the crosshair is on the ship (ship frame), for placing and routing.
    aim: Option<Vec3>,
    /// Asked by the window, done by `update` (it needs the world).
    apply: bool,
}

fn defs() -> std::path::PathBuf {
    crate::root().join("assets/defs")
}

impl Editor {
    /// F6: open on the ship ridden (else the nearest within 40 m), or close.
    pub fn toggle(&mut self, ships: &Ships, builds: &Builds, ride: Option<u64>, eye: DVec3) {
        if self.open {
            self.open = false;
            return;
        }
        let sid = ride.filter(|r| ships.by_structure(*r).is_some()).or_else(|| {
            ships
                .list
                .iter()
                .filter_map(|sh| builds.set.get(sh.structure).map(|s| (s.to_world(s.center).distance(eye) - f64::from(s.radius), sh.structure)))
                .filter(|(d, _)| *d < 40.0)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, id)| id)
        });
        let Some(sid) = sid else {
            self.status = "Ninguna nave cerca para editar".into();
            return;
        };
        let k = ships.by_structure(sid).expect("checked");
        let id = ships.list[k].kind.id.clone();
        if self.doc.as_ref().is_none_or(|d| d.id != id) {
            match Doc::open(&defs(), &id) {
                Ok(d) => self.doc = Some(d),
                Err(e) => {
                    self.status = e;
                    return;
                }
            }
            if let Ok(src) = lunar_ship::Sources::load(&defs()) {
                self.kinds = src.components.keys().cloned().collect();
            }
        }
        self.ship = Some(sid);
        self.step = if self.step <= 0.0 { 0.05 } else { self.step };
        self.open = true;
        self.status = format!("Editando {id}: clic en una pieza para elegirla");
    }

    /// The structure of the ship being edited.
    pub fn ship(&self) -> Option<u64> {
        self.ship
    }

    /// Rebuild the ship from the document on the next update.
    pub fn request_apply(&mut self) {
        self.apply = true;
    }

    /// What the last operation did (for scripts and the HUD).
    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn op(&mut self, op: Op) {
        let Some(d) = &mut self.doc else { return };
        self.status = match d.apply(&op) {
            Ok(m) => format!("{m} (Aplicar para verlo)"),
            Err(e) => format!("No: {e}"),
        };
    }

    /// A click at the mouse ray (world): what it hits on the ship is selected.
    pub fn click(&mut self, ships: &Ships, builds: &Builds, from: DVec3, dir: DVec3) {
        let Some(sid) = self.ship else { return };
        let (Some(s), Some(k)) = (builds.set.get(sid), ships.by_structure(sid)) else { return };
        let kind = &ships.list[k].kind;
        let Some(hit) = s.raycast(s.to_local(from), s.dir_to_local(dir), 60.0) else {
            self.selected = None;
            return;
        };
        let part = &kind.parts[hit.part as usize];
        // a network's run (its conduits are named "net.tramoN.k")
        if let Some((net, rest)) = part.split_once(".tramo")
            && let Some(n) = rest.split('.').next().and_then(|n| n.parse().ok())
        {
            self.run = Some((net.to_string(), n));
            self.status = format!("Tramo {n} de la red {net}: «Punto de ruta» lo hace pasar por donde miras");
            return;
        }
        // the component it belongs to: the longest component id it starts with
        let Some(d) = &self.doc else { return };
        let ids: Vec<String> = d.value["componentes"].as_array().map(|l| l.iter().filter_map(|c| c["id"].as_str().map(str::to_string)).collect()).unwrap_or_default();
        let base = part.split('#').next().unwrap_or(part);
        let found = ids.iter().filter(|id| base == id.as_str() || base.starts_with(&format!("{id}.")) || base.starts_with(&format!("{id}#"))).max_by_key(|id| id.len()).cloned().or_else(|| {
            let m = lunar_ship::components::mirror_name(base);
            ids.iter().filter(|id| m == id.as_str() || m.starts_with(&format!("{id}."))).max_by_key(|id| id.len()).cloned()
        });
        match found {
            Some(id) => {
                self.status = format!("{id} ({part})");
                self.selected = Some(id);
            }
            None => {
                self.status = format!("{part}: no es un componente (casco, cubierta, mamparo, panel)");
                self.selected = None;
            }
        }
    }

    /// The crosshair this frame (world), for placing and routing.
    pub fn aim(&mut self, builds: &Builds, eye: DVec3, fwd: DVec3) {
        self.aim = None;
        let Some(s) = self.ship.and_then(|id| builds.set.get(id)) else { return };
        let (o, d) = (s.to_local(eye), s.dir_to_local(fwd));
        if let Some(h) = s.raycast(o, d, 30.0) {
            // a little off the surface hit, toward the eye
            self.aim = Some(o + d * (h.t - 0.05).max(0.0));
        }
    }

    /// The window (what it asks is done after it is drawn).
    pub fn window(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let Some(d) = &self.doc else { return };
        let (value, title) = (d.value.clone(), format!("{}{}", d.id, if d.dirty { " · cambios sin aplicar o sin guardar" } else { "" }));
        let mut ops: Vec<Op> = Vec::new();
        let (mut check, mut save, mut unselect) = (false, false, false);
        let mut select = None;
        let mut open = true;
        let (aim, kinds, sel, run) = (self.aim, self.kinds.clone(), self.selected.clone(), self.run.clone());
        let mut step = self.step;
        let mut kind = self.kind;
        let mut status = None;
        let shown = egui::Window::new("EDITOR DE NAVES  ·  F6").frame(crate::hud::frame()).open(&mut open).default_pos([60.0, 80.0]).default_width(380.0).show(ctx, |ui| {
            ui.label(&title);
            ui.label("Clic: elegir pieza · botón derecho mantenido: mirar · WASD: moverse");
            ui.separator();
            let comps = value["componentes"].as_array().cloned().unwrap_or_default();
            match &sel {
                Some(id) => {
                    let c = comps.iter().find(|c| c["id"].as_str() == Some(id.as_str())).cloned().unwrap_or_default();
                    ui.label(format!("{id} · {}", c["tipo"].as_str().unwrap_or("forma")));
                    ui.label(format!("en {}   rot {}", c["en"], c.get("rot").cloned().unwrap_or_default()));
                    ui.horizontal(|ui| {
                        ui.label("Paso (m):");
                        ui.add(egui::DragValue::new(&mut step).speed(0.005).range(0.005..=1.0));
                    });
                    ui.horizontal(|ui| {
                        for (label, d) in [("x−", [-step, 0.0, 0.0]), ("x+", [step, 0.0, 0.0]), ("y−", [0.0, -step, 0.0]), ("y+", [0.0, step, 0.0]), ("z−", [0.0, 0.0, -step]), ("z+", [0.0, 0.0, step])] {
                            if ui.button(label).clicked() {
                                ops.push(Op::Desplazar { id: id.clone(), delta: d });
                            }
                        }
                    });
                    let rot = c.get("rot").and_then(|r| serde_json::from_value::<[f32; 3]>(r.clone()).ok()).unwrap_or([0.0; 3]);
                    ui.horizontal(|ui| {
                        for (label, k, a) in [("X+15°", 0, 15.0), ("Y+15°", 1, 15.0), ("Z+15°", 2, 15.0), ("Y−15°", 1, -15.0)] {
                            if ui.button(label).clicked() {
                                let mut r = rot;
                                r[k] += a;
                                ops.push(Op::Girar { id: id.clone(), rot: r });
                            }
                        }
                    });
                    let mut w = c.get("cableado") != Some(&serde_json::json!(false));
                    if ui.checkbox(&mut w, "Con su cableado (sin marcar: conectado sin cables)").changed() {
                        ops.push(Op::Cableado { id: id.clone(), valor: w });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Duplicar").clicked() {
                            let taken = |n: &str| comps.iter().any(|c| c["id"].as_str() == Some(n));
                            let n = (2..100).map(|k| format!("{id}_{k}")).find(|n| !taken(n)).unwrap_or_else(|| format!("{id}_copia"));
                            ops.push(Op::Duplicar { id: id.clone(), nuevo: n.clone(), en: None });
                            select = Some(n);
                        }
                        if ui.button("Quitar").clicked() {
                            ops.push(Op::Quitar { id: id.clone() });
                            unselect = true;
                        }
                    });
                }
                None => {
                    ui.label("Clic en una pieza de la nave para elegir su componente.");
                }
            }
            ui.separator();
            if !kinds.is_empty() {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("tipos").selected_text(kinds[kind.min(kinds.len() - 1)].clone()).show_ui(ui, |ui| {
                        for (i, k) in kinds.iter().enumerate() {
                            ui.selectable_value(&mut kind, i, k);
                        }
                    });
                    if ui.button("Poner donde miro").clicked() {
                        match aim {
                            Some(p) => {
                                let t = kinds[kind].clone();
                                let n = (1..1000).map(|k| format!("{t}_{k}")).find(|n| !comps.iter().any(|c| c["id"].as_str() == Some(n.as_str()))).unwrap_or_else(|| format!("{t}_nuevo"));
                                ops.push(Op::Poner { componente: serde_json::json!({ "id": n, "tipo": t, "en": [p.x, p.y, p.z] }) });
                                select = Some(n);
                            }
                            None => status = Some("Mira a un sitio de la nave".to_string()),
                        }
                    }
                });
            }
            if let Some((net, n)) = &run {
                ui.horizontal(|ui| {
                    ui.label(format!("Tramo {n} de {net}"));
                    if ui.button("Punto de ruta donde miro").clicked() {
                        match aim {
                            Some(p) => ops.push(Op::PuntoRuta { red: net.clone(), tramo: *n, punto: p.to_array() }),
                            None => status = Some("Mira a un sitio de la nave".to_string()),
                        }
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Deshacer").clicked() {
                    ops.push(Op::Deshacer);
                }
                if ui.button("Rehacer").clicked() {
                    ops.push(Op::Rehacer);
                }
                check = ui.button("Comprobar").clicked();
                if ui.button("Aplicar").clicked() {
                    self.apply = true;
                }
                save = ui.button("Guardar").clicked();
            });
            ui.label(&self.status);
            if !self.report.is_empty() {
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    ui.monospace(&self.report);
                });
            }
        });
        if let Some(shown) = shown {
            crate::hud::dress(ctx, shown.response.rect);
        }
        self.step = step;
        self.kind = kind;
        if let Some(s) = status {
            self.status = s;
        }
        for op in ops {
            self.op(op);
        }
        if unselect {
            self.selected = None;
        }
        if select.is_some() {
            self.selected = select;
        }
        if check && let Some(d) = &self.doc {
            self.report = d.check(&defs()).text();
        }
        if save && let Some(d) = &mut self.doc {
            self.status = match d.save(None, &crate::root().join("out/copias")) {
                Ok(p) => format!("Guardada en {}", p.display()),
                Err(e) => format!("No se guardó: {e}"),
            };
        }
        if !open {
            self.open = false;
        }
    }

    /// Apply if asked: the ship rebuilt from the document and put back where it was.
    pub fn update(&mut self, ships: &mut Ships, builds: &mut Builds, bodies: &BodyRegistry) {
        if !std::mem::take(&mut self.apply) {
            return;
        }
        let (Some(d), Some(sid)) = (&self.doc, self.ship) else { return };
        let Some(old) = builds.set.get(sid) else { return };
        let (pos, rot, resting) = (old.pos, old.rot, old.resting);
        let def = match d.def() {
            Ok(x) => x,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let src = match lunar_ship::Sources::load(&defs()) {
            Ok(s) => s,
            Err(e) => {
                self.status = format!("{}: {}", e.file, e.message);
                return;
            }
        };
        // the new kind into the live library (parts appended: the others keep their indices)
        let lib = Arc::make_mut(&mut builds.set.lib);
        let (kind, bp) = match src.build(&d.id, def, &mut lib.catalog) {
            Ok(x) => x,
            Err(e) => {
                self.status = format!("No se arma: {e}");
                return;
            }
        };
        match lib.blueprints.iter_mut().find(|(n, _)| *n == d.id) {
            Some(b) => b.1 = bp,
            None => lib.blueprints.push((d.id.clone(), bp)),
        }
        if let Err(e) = builds.set.library_changed() {
            self.status = format!("No se arma: {e}");
            return;
        }
        let kind = Arc::new(kind);
        match ships.kinds.iter_mut().find(|k| k.id == d.id) {
            Some(k) => *k = kind,
            None => ships.kinds.push(kind),
        }
        builds.set.remove(sid);
        match ships.spawn_free(builds, &d.id, pos, rot) {
            Ok(new) => {
                if resting {
                    builds.set.rest_on_ground(new, bodies);
                }
                self.ship = Some(new);
                self.status = "Aplicada: la nave rehecha en su sitio".into();
            }
            Err(e) => self.status = format!("No se pudo poner: {e}"),
        }
    }

    /// The selected component outlined (its parts' boxes): props in their own scene (frame 0).
    pub fn highlight(&self, ships: &Ships, builds: &Builds) -> PropScene {
        let mut out = PropScene::default();
        let (Some(sid), Some(sel)) = (self.ship, &self.selected) else { return out };
        let (Some(s), Some(k)) = (builds.set.get(sid), ships.by_structure(sid)) else { return out };
        let kind = &ships.list[k].kind;
        let frame = 0;
        out.frames.push(PropFrame { pos: s.pos, rot: s.rot, inside: false });
        for (i, id) in kind.parts.iter().enumerate() {
            if !(id == sel || id.starts_with(&format!("{sel}.")) || id.starts_with(&format!("{sel}#"))) {
                continue;
            }
            let p = &s.parts[i];
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for v in p.shape.verts() {
                let w = p.local.transform_point3(v);
                lo = lo.min(w);
                hi = hi.max(w);
            }
            let (c, h) = ((lo + hi) * 0.5, (hi - lo) * 0.5 + 0.01);
            let t = 0.006;
            let mut edge = |pos: Vec3, size: Vec3| out.props.push(Prop { frame, mesh: BOX, pos, rot: Quat::IDENTITY, size, color: [255, 200, 40], emissive: -2.0, rough: 200, metal: 0 });
            for (a, b) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
                edge(c + Vec3::new(0.0, a * h.y, b * h.z), Vec3::new(h.x * 2.0, t, t));
                edge(c + Vec3::new(a * h.x, 0.0, b * h.z), Vec3::new(t, h.y * 2.0, t));
                edge(c + Vec3::new(a * h.x, b * h.y, 0.0), Vec3::new(t, t, h.z * 2.0));
            }
        }
        out
    }

    /// The ray through the mouse at (x, y) px of a window `size`, for view `v`.
    pub fn mouse_ray(v: &lunar_render::View, x: f64, y: f64, size: (u32, u32)) -> DVec3 {
        let (w, h) = (f64::from(size.0.max(1)), f64::from(size.1.max(1)));
        let t = (f64::from(v.fov_y) * 0.5).tan();
        let right = v.forward.cross(v.up).normalize_or(DVec3::X);
        let up = right.cross(v.forward).normalize_or(DVec3::Y);
        (v.forward + right * ((2.0 * x / w - 1.0) * t * w / h) + up * ((1.0 - 2.0 * y / h) * t)).normalize()
    }
}
