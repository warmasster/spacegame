//! F4 (debug build): the inside of the ship you are on or nearest to — every signal, every network
//! island, every machine with its provenance, every actuator and joint, the black box.
use crate::ships::Ships;
use glam::DVec3;
use lunar_core::structure::set::Structures;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Signals,
    Nets,
    Machines,
    Actuators,
    Log,
}

#[derive(Default)]
pub struct Inspector {
    pub open: bool,
    tab: Tab,
    filter: String,
    text: String,
}

impl Inspector {
    /// The window, for the ship on structure `on` or else the nearest to `eye`.
    pub fn window(&mut self, ctx: &egui::Context, ships: &Ships, set: &Structures, on: Option<u64>, eye: DVec3) {
        let pick = on.and_then(|id| ships.by_structure(id)).or_else(|| {
            ships
                .list
                .iter()
                .enumerate()
                .filter_map(|(i, sh)| set.get(sh.structure).map(|s| (i, s.to_world(s.center).distance(eye))))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        });
        let mut open = self.open;
        let shown = egui::Window::new("INSPECTOR  ·  F4").frame(crate::hud::frame()).open(&mut open).default_pos([980.0, 40.0]).default_size([520.0, 640.0]).resizable(true).show(ctx, |ui| {
            let Some(n) = pick else {
                ui.label("No hay naves.");
                return;
            };
            let sh = &ships.list[n];
            let k = &sh.kind;
            ui.label(format!("{} ({}) · estructura {} · t = {:.1} s", k.def.nombre, k.id, sh.structure, sh.t));
            ui.horizontal(|ui| {
                for (t, name) in [(Tab::Signals, "Señales"), (Tab::Nets, "Redes"), (Tab::Machines, "Máquinas"), (Tab::Actuators, "Actuadores"), (Tab::Log, "Caja negra")] {
                    if ui.selectable_label(self.tab == t, name).clicked() {
                        self.tab = t;
                    }
                }
            });
            ui.separator();
            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
            match self.tab {
                Tab::Signals => {
                    ui.horizontal(|ui| {
                        ui.label("Filtro:");
                        ui.text_edit_singleline(&mut self.filter);
                    });
                    let q = self.filter.to_lowercase();
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        let mut shown = 0;
                        for id in sh.store.ids() {
                            let name = sh.store.name(id);
                            if !q.is_empty() && !name.to_lowercase().contains(&q) {
                                continue;
                            }
                            self.text.clear();
                            sh.store.format(id, 3, &mut self.text);
                            ui.label(format!("{name:<34} {}", self.text));
                            shown += 1;
                            if shown >= 600 {
                                ui.label("… (afina el filtro)");
                                break;
                            }
                        }
                    });
                }
                Tab::Nets => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for net in &sh.nets {
                            let dead = net.edges.iter().filter(|e| !e.alive).count();
                            let leaking = net.edges.iter().filter(|e| e.leak > 0.0).count();
                            ui.label(format!("{} [{:?}] nodos {} tramos {} (cortados {dead}, fugas {leaking})", net.name, net.medium, net.nodes, net.edges.len()));
                            for (i, is) in net.islands.iter().enumerate() {
                                ui.label(format!(
                                    "  isla {i}: potencial {:.4e}  produce {:.3e}  pide {:.3e}  servido {:.0} %  fuga {:.2e}  almacén {:.3e}",
                                    is.potential,
                                    is.produced,
                                    is.demanded,
                                    is.served * 100.0,
                                    is.leaked,
                                    is.stored
                                ));
                            }
                        }
                    });
                }
                Tab::Machines => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for (i, m) in sh.machines.iter().enumerate() {
                            let p = &m.provenance;
                            let health = sh.store.get(m.health);
                            ui.label(format!(
                                "{:<26} {:<16} salud {:>3.0} % {}  {} {} nº {} ({}{})",
                                k.machines[i].id,
                                m.m.kind(),
                                health * 100.0,
                                if m.working { "funciona" } else { "PARADA  " },
                                p.maker,
                                p.model,
                                p.serial,
                                p.built,
                                if p.night_shift { ", turno de noche" } else { "" }
                            ));
                        }
                    });
                }
                Tab::Actuators => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for a in &sh.actuators {
                            let j = &sh.joints[a.joint];
                            let (q, lo, hi) = if j.hinge { (j.q.to_degrees(), j.lo.to_degrees(), j.hi.to_degrees()) } else { (j.q, j.lo, j.hi) };
                            let u = if j.hinge { "°" } else { "m" };
                            ui.label(format!("{:<20} {:<26} {} {q:8.2} {u} [{lo:.1}..{hi:.1}]  v {:.3}", a.id, a.name, a.joint_name, j.qd));
                        }
                    });
                }
                Tab::Log => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.label(format!("{} entradas en total", sh.blackbox.count));
                        for e in sh.blackbox.entries.iter().rev() {
                            let tag = ["   ", "(!)", "[!]"][usize::from(e.level.min(2))];
                            ui.label(format!("{:8.1} s {tag} {}", e.t, e.text));
                        }
                    });
                }
            }
        });
        if let Some(shown) = shown {
            crate::hud::dress(ctx, shown.response.rect);
        }
        self.open = open;
    }
}
