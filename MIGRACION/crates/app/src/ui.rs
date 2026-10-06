//! egui: the HUD (`hud`), the menu (Esc) with its tabs — the controls (read off the table of keys,
//! `input`), the graphics (eight presets from HORRIBLE to ESPLÉNDIDOS plus every knob in
//! advanced mode) and, in the debug build, the scene — and the F3 diagnostics.
use crate::{blasts::ShotEdit, hud::Hud, input, pilot::Controls};
use lunar_core::{
    quality::{Preset, Settings},
    system::Sun,
};
use lunar_render::{Stats, UiFrame, timing::PASS_NAMES};
use winit::window::Window;

pub struct Info<'a> {
    pub stats: &'a Stats,
    pub adapter: &'a str,
    pub fps: f32,
    pub frame_ms: f32,
    pub sim_ms: f32,
    pub mode: &'a str,
    pub altitude: f64,
    pub speed: f64,
    /// Flight speed factor of the keys held (Shift, Alt).
    pub speed_factor: f64,
    pub ships: usize,
    pub npcs: usize,
    pub particles: usize,
    pub structures: lunar_core::structure::schedule::SimStats,
    pub missile: String,
}

pub struct Ui {
    pub ctx: egui::Context,
    state: egui_winit::State,
    pub stats: bool,
    pub menu: bool,
    advanced: bool,
    pub edit: Settings,
    pub preset: Option<Preset>,
    pub landed: usize,
    pub flying: usize,
    pub npcs: usize,
    /// The system's sun (edited here, applied with the settings).
    pub sun: Sun,
    pub controls: Controls,
    /// Apply while dragging, not only on release.
    pub realtime: bool,
    /// Set by the menu: apply `edit` / rebuild the scenario.
    pub apply: bool,
    pub regenerate: bool,
    /// Shots the menu changes in play, and whether they changed this frame.
    pub shots: Vec<ShotEdit>,
    pub shots_changed: bool,
    /// What the game shows over the picture this frame.
    pub hud: Hud,
    /// Windows of the game open (catalog, inspector): they take the mouse and keys.
    pub panels: bool,
    tab: Tab,
    /// For the Controls tab: the keys of the seats of the ship at hand (seat, key, what it does)
    /// and of the test shots (debug), as their data says.
    pub seat_keys: Vec<(String, String, String)>,
    pub test_keys: String,
    /// The suit's tools: (key, name and what its buttons do).
    pub tool_keys: Vec<(String, String)>,
    /// The sound card (or why there is no sound), for the Controls tab.
    pub sound: String,
    /// The menu is open over the start menu (not over a game going on).
    pub at_start: bool,
    /// What the menu's head asks of the game.
    pub ask: Option<Ask>,
}

/// What the menu asks of the game besides its settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Back to the start menu.
    Start,
    /// Out of the game.
    Quit,
}

/// The menu's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Controls,
    Graphics,
    Scene,
}

impl Ui {
    #[allow(clippy::too_many_arguments)]
    pub fn new(window: &Window, settings: Settings, preset: Option<Preset>, landed: usize, flying: usize, npcs: usize, sun: Sun, controls: Controls) -> Ui {
        let ctx = egui::Context::default();
        crate::hud::style(&ctx);
        let state = egui_winit::State::new(ctx.clone(), egui::ViewportId::ROOT, window, Some(window.scale_factor() as f32), None, None);
        Ui { ctx, state, stats: false, menu: false, advanced: false, edit: settings, preset, landed, flying, npcs, sun, controls, realtime: false, apply: false, regenerate: false, shots: Vec::new(), shots_changed: false, hud: Hud::default(), panels: false, tab: Tab::Controls, seat_keys: Vec::new(), test_keys: String::new(), tool_keys: Vec::new(), sound: String::new(), at_start: false, ask: None }
    }

    /// Nothing fades in or out: what a picture is taken of is there at once (a script's).
    pub fn instant(&self) {
        self.ctx.all_styles_mut(|s| s.animation_time = 0.0);
    }

    /// The menu open at a tab ("controles", "graficos", "escena"), or shut (None).
    pub fn show_menu(&mut self, tab: Option<&str>) {
        self.menu = tab.is_some();
        self.tab = match tab {
            Some("graficos") => Tab::Graphics,
            Some("escena") if !crate::DEMO => Tab::Scene,
            _ => Tab::Controls,
        };
    }

    pub fn visible(&self) -> bool {
        self.stats || self.menu || self.panels
    }

    /// True when egui wants the event (menus open).
    pub fn event(&mut self, window: &Window, e: &winit::event::WindowEvent) -> bool {
        self.visible() && self.state.on_window_event(window, e).consumed
    }

    pub fn frame(&mut self, window: &Window, info: &Info, mut extra: impl FnMut(&egui::Context)) -> Option<UiFrame> {
        if !self.visible() && self.hud.is_empty() {
            return None;
        }
        let input = self.state.take_egui_input(window);
        let ctx = self.ctx.clone();
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            self.hud.draw(&ctx);
            extra(&ctx);
            if self.stats {
                stats_window(&ctx, info);
            }
            if self.menu {
                self.menu_window(&ctx);
            }
        });
        self.state.handle_platform_output(window, std::mem::take(&mut out.platform_output));
        let primitives = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        Some(UiFrame { primitives, textures: out.textures_delta, pixels_per_point: out.pixels_per_point })
    }

    /// The menu: the picture dimmed, and over it one plate of the HUD's glass — the name of the
    /// game, its tabs as words with a line under the one open, and what the tab holds scrolling
    /// under them.
    fn menu_window(&mut self, ctx: &egui::Context) {
        use egui::{Color32, Sense, Stroke, pos2, vec2};
        let screen = ctx.content_rect();
        crate::hud::backdrop(ctx, 1.0);
        let size = vec2(820.0_f32.min(screen.width() - 60.0), (screen.height() - 110.0).max(260.0));
        let at = pos2((screen.width() - size.x) * 0.5, (screen.height() - size.y) * 0.5);
        egui::Area::new(egui::Id::new("menu")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            let rect = egui::Rect::from_min_size(at, size);
            crate::hud::plate(ui.painter(), rect, 1.6, Some(crate::hud::ACCENT), 1.0);
            let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(30.0, 22.0))));
            let ui = &mut ui;
            // ---- its head: the name, and how to leave ----
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                crate::hud::emblem(ui.painter(), r.center(), 40.0, ui.input(|i| i.time) as f32);
                ui.ctx().request_repaint();
                let mut job = egui::text::LayoutJob::default();
                job.append(crate::GAME, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(34.0), color: crate::hud::TEXT, extra_letter_spacing: 9.0, ..Default::default() });
                ui.label(job);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add_space(9.0);
                    let mut job = egui::text::LayoutJob::default();
                    job.append(if self.at_start { "OPCIONES Y CONTROLES" } else { "SISTEMAS DEL TRAJE  ·  EN PAUSA" }, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(12.5), color: crate::hud::DIM, extra_letter_spacing: 2.4, ..Default::default() });
                    ui.label(job);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(if self.at_start { "volver" } else { "volver al juego" }).color(crate::hud::DIM));
                    crate::hud::key_label(ui, "Esc");
                    // out of the game, and back to the start menu: words to click
                    let word = |ui: &mut egui::Ui, text: &str| -> bool {
                        ui.add_space(18.0);
                        let mut job = egui::text::LayoutJob::default();
                        job.append(text, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(12.5), color: Color32::PLACEHOLDER, extra_letter_spacing: 2.0, ..Default::default() });
                        let g = ui.painter().layout_job(job);
                        let (r, resp) = ui.allocate_exact_size(g.size() + vec2(0.0, 8.0), Sense::click());
                        let ink = if resp.hovered() { crate::hud::TEXT } else { crate::hud::DIM };
                        ui.painter().galley(r.left_top() + vec2(0.0, 4.0), g, ink);
                        if resp.hovered() {
                            ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, crate::hud::ACCENT));
                        }
                        resp.clicked()
                    };
                    if word(ui, "SALIR DEL JUEGO") {
                        self.ask = Some(Ask::Quit);
                    }
                    if !self.at_start && word(ui, "MENÚ DE INICIO") {
                        self.ask = Some(Ask::Start);
                    }
                });
            });
            ui.add_space(10.0);
            // ---- its tabs ----
            let mut tabs = vec![(Tab::Controls, "CONTROLES"), (Tab::Graphics, "GRÁFICOS")];
            if !crate::DEMO {
                tabs.push((Tab::Scene, "ESCENA Y PRUEBAS"));
            }
            let row = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                for (t, name) in tabs {
                    let on = self.tab == t;
                    let mut job = egui::text::LayoutJob::default();
                    job.append(name, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(16.0), color: Color32::PLACEHOLDER, extra_letter_spacing: 2.6, ..Default::default() });
                    let g = ui.painter().layout_job(job);
                    let (r, resp) = ui.allocate_exact_size(g.size() + vec2(0.0, 14.0), Sense::click());
                    let ink = if on { crate::hud::TEXT } else if resp.hovered() { Color32::from_rgb(200, 226, 236) } else { crate::hud::DIM };
                    ui.painter().galley(r.left_top() + vec2(0.0, 2.0), g, ink);
                    if on {
                        ui.painter().line_segment([r.left_bottom() + vec2(0.0, -1.0), r.right_bottom() + vec2(0.0, -1.0)], Stroke::new(5.0, crate::hud::ACCENT.gamma_multiply(0.18)));
                        ui.painter().line_segment([r.left_bottom() + vec2(0.0, -1.0), r.right_bottom() + vec2(0.0, -1.0)], Stroke::new(2.0, crate::hud::ACCENT));
                    }
                    if resp.clicked() {
                        self.tab = t;
                    }
                }
            });
            let y = row.response.rect.bottom();
            ui.painter().line_segment([pos2(ui.max_rect().left(), y), pos2(ui.max_rect().right(), y)], Stroke::new(1.0, Color32::from_rgb(40, 52, 62)));
            ui.add_space(8.0);
            // ---- its foot: what this is, and how to get about in it ----
            let foot = egui::Rect::from_min_max(pos2(ui.max_rect().left(), ui.max_rect().bottom() - 22.0), ui.max_rect().right_bottom());
            ui.painter().line_segment([foot.left_top(), foot.right_top()], Stroke::new(1.0, Color32::from_rgb(40, 52, 62)));
            let small = |text: &str| {
                let mut job = egui::text::LayoutJob::default();
                job.append(text, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(11.5), color: crate::hud::DIM, extra_letter_spacing: 1.8, ..Default::default() });
                job
            };
            let left = ui.painter().layout_job(small(&format!("{}  ·  {}  ·  {}", crate::GAME, env!("CARGO_PKG_VERSION"), crate::EDITION)));
            let right = ui.painter().layout_job(small("RUEDA: DESPLAZAR  ·  CLIC: ELEGIR  ·  ESC: VOLVER"));
            ui.painter().galley(foot.left_top() + vec2(0.0, 8.0), left, crate::hud::DIM);
            ui.painter().galley(pos2(foot.right() - right.size().x, foot.top() + 8.0), right, crate::hud::DIM);
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 30.0).show(ui, |ui| {
                ui.set_width(ui.available_width() - 10.0);
                match self.tab {
                    Tab::Controls => self.controls_tab(ui),
                    Tab::Graphics => self.graphics_tab(ui),
                    Tab::Scene => self.scene_tab(ui),
                }
            });
        });
    }

    /// The keys, as the table of keys has them (`input::BINDINGS`), the seats' as their ship's
    /// data has them, and the mouse's settings.
    fn controls_tab(&mut self, ui: &mut egui::Ui) {
        crate::hud::section(ui, "Ratón y sonido");
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.controls.mouse, 0.2..=3.0).text("Sensibilidad del ratón"));
            ui.checkbox(&mut self.controls.invert_y, "Invertir el eje vertical");
        });
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.controls.volume, 0.0..=1.5).text("Volumen"));
            ui.label(egui::RichText::new(&self.sound).color(crate::hud::DIM).small());
        });
        let row = |ui: &mut egui::Ui, key: &str, what: &str| {
            crate::hud::key_label(ui, key);
            // (a long line folds: the window keeps its width)
            ui.scope(|ui| {
                ui.set_max_width(500.0);
                ui.add(egui::Label::new(what).wrap());
            });
            ui.end_row();
        };
        let title = |ui: &mut egui::Ui, text: &str| crate::hud::section(ui, text);
        // the keys in one column of one width, whatever the group
        let grid = |id: &str| egui::Grid::new(id).num_columns(2).min_col_width(205.0).spacing([16.0, 6.0]).striped(true);
        for g in input::Group::ALL {
            let rows: Vec<&input::Binding> = input::BINDINGS.iter().filter(|b| b.group == g && !(crate::DEMO && b.debug)).collect();
            if rows.is_empty() {
                continue;
            }
            title(ui, g.title());
            grid(g.title()).show(ui, |ui| {
                for b in rows {
                    row(ui, b.shown, b.what);
                }
                if g == input::Group::Manos {
                    for (key, what) in &self.tool_keys {
                        row(ui, key, what);
                    }
                }
                if g == input::Group::Pruebas && !self.test_keys.is_empty() {
                    row(ui, "Disparos", &self.test_keys);
                }
            });
        }
        // the seats with keys of their own
        let mut seat = "";
        for (name, _, _) in &self.seat_keys {
            if name == seat {
                continue;
            }
            seat = name;
            title(ui, &format!("SENTADO · {}", name.to_uppercase()));
            grid(name).show(ui, |ui| {
                for (_, key, what) in self.seat_keys.iter().filter(|k| &k.0 == name) {
                    row(ui, key, what);
                }
            });
        }
    }

    /// The scene's counts and the test shots (debug build).
    fn scene_tab(&mut self, ui: &mut egui::Ui) {
        ui.add(egui::Slider::new(&mut self.controls.boost_factor, 1.0..=1000.0).logarithmic(true).text("Alt en vuelo libre: multiplicador (sobre Mayús)"));
        for e in &mut self.shots {
            ui.separator();
            ui.label(format!("Proyectil {} (tecla {})", e.name, e.key));
            let before = e.clone();
            let label = format!("Carga: {}", tnt_text(e.tnt));
            ui.add(egui::Slider::new(&mut e.tnt, 0.01..=e.max_tnt).logarithmic(true).text(label));
            ui.add(egui::Slider::new(&mut e.speed, 1.0..=e.max_speed).logarithmic(true).text("Velocidad (m/s)"));
            ui.add(egui::Slider::new(&mut e.size, 0.02..=5.0).logarithmic(true).text("Tamaño en vuelo (m)"));
            if *e != before {
                self.shots_changed = true;
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.landed, 0..=2000).text("Naves aterrizadas"));
            ui.add(egui::Slider::new(&mut self.flying, 0..=2000).text("Naves volando"));
            ui.add(egui::Slider::new(&mut self.npcs, 0..=2000).text("NPC"));
            if ui.button("Regenerar escena").clicked() {
                self.regenerate = true;
            }
        });
    }

    fn graphics_tab(&mut self, ui: &mut egui::Ui) {
        {
            crate::hud::section(ui, "Calidad");
            ui.horizontal_wrapped(|ui| {
                for p in Preset::ALL {
                    if ui.selectable_label(self.preset == Some(p), p.name()).clicked() {
                        let vsync = self.edit.vsync;
                        self.edit = Settings::preset(p);
                        self.edit.vsync = vsync;
                        self.preset = Some(p);
                        self.apply = true;
                    }
                }
            });
            crate::hud::section(ui, "Imagen y sol");
            let before = self.edit.clone();
            let s = &mut self.edit;
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.vsync, "VSync");
                ui.checkbox(&mut s.dynamic_resolution, "Resolución dinámica");
                ui.add(egui::Slider::new(&mut s.target_fps, 30.0..=240.0).text("FPS objetivo"));
            });
            ui.add(egui::Slider::new(&mut s.render_scale, 0.35..=2.0).text("Escala de resolución"));
            let sun_before = self.sun;
            ui.add(egui::Slider::new(&mut self.sun.azimuth, 0.0..=360.0).text("Sol: azimut (°)"));
            ui.add(egui::Slider::new(&mut self.sun.elevation, -10.0..=90.0).text("Sol: elevación (°)"));
            if self.sun != sun_before {
                self.apply = true;
            }
            ui.add(egui::Slider::new(&mut s.shadow_softness, 0.25..=2.5).text("Suavidad de sombras (menos: más nítidas)"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.advanced, "Modo avanzado");
                ui.checkbox(&mut self.realtime, "Tiempo real (aplicar mientras arrastras)");
            });
            if self.advanced {
                ui.collapsing("Imagen", |ui| {
                    ui.checkbox(&mut s.fxaa, "FXAA");
                    ui.checkbox(&mut s.bloom, "Bloom");
                    ui.add(egui::Slider::new(&mut s.bloom_strength, 0.0..=0.3).text("Fuerza del bloom"));
                    ui.add(egui::Slider::new(&mut s.exposure, 0.2..=3.0).text("Exposición"));
                    ui.add(egui::Slider::new(&mut s.anisotropy, 1..=16).text("Anisotropía"));
                });
                ui.collapsing("Sombras", |ui| {
                    ui.add(egui::Slider::new(&mut s.shadow_cascades, 0..=4).text("Cascadas"));
                    pow2(ui, &mut s.shadow_resolution, 512, 4096, "Resolución");
                    ui.add(egui::Slider::new(&mut s.shadow_distance, 30.0..=3000.0).logarithmic(true).text("Distancia (m)"));
                    ui.add(egui::Slider::new(&mut s.shadow_filter, 0..=3).text("Filtro (0 duro - 3 suave)"));
                    ui.add(egui::Slider::new(&mut s.shadow_split, 0.0..=1.0).text("Reparto de cascadas (1: más detalle cerca)"));
                    ui.add(egui::Slider::new(&mut s.shadow_blend, 0.0..=0.5).text("Fundido entre cascadas"));
                    ui.checkbox(&mut s.shadow_cache, "Caché de sombra estática");
                    ui.add(egui::Slider::new(&mut s.far_cascade_every, 1..=8).text("Cascadas lejanas cada N frames"));
                    ui.checkbox(&mut s.baked_terrain_shadows, "Sombras horneadas del relieve");
                });
                ui.collapsing("Terreno", |ui| {
                    ui.add(egui::Slider::new(&mut s.terrain_grid, 8..=128).step_by(8.0).text("Rejilla por nodo"));
                    ui.add(egui::Slider::new(&mut s.terrain_split, 0.8..=4.0).text("Detalle (división)"));
                    ui.add(egui::Slider::new(&mut s.terrain_finest_cell, 0.1..=3.0).logarithmic(true).text("Celda más fina (m)"));
                    ui.add(egui::Slider::new(&mut s.terrain_gen_per_frame, 1..=64).text("Nodos generados por frame"));
                    ui.add(egui::Slider::new(&mut s.terrain_cache_nodes, 128..=4096).text("Caché de nodos"));
                    ui.add(egui::Slider::new(&mut s.terrain_detail, 0..=2).text("Capas de detalle"));
                });
                ui.collapsing("Objetos", |ui| {
                    ui.add(egui::Slider::new(&mut s.lod_bias, 0.1..=4.0).text("Sesgo de LOD"));
                    ui.add(egui::Slider::new(&mut s.shadow_lod_bias, 0.0..=1.0).text("LOD más bajo en sombras"));
                    ui.add(egui::Slider::new(&mut s.draw_distance, 500.0..=50000.0).logarithmic(true).text("Distancia de dibujo (m)"));
                    ui.checkbox(&mut s.impostors, "Impostores");
                    ui.checkbox(&mut s.occlusion_culling, "Oclusión Hi-Z");
                    ui.add(egui::Slider::new(&mut s.rocks_density, 0.0..=3.0).text("Densidad de rocas"));
                    ui.add(egui::Slider::new(&mut s.rocks_radius, 30.0..=800.0).text("Radio de rocas (m)"));
                });
                ui.collapsing("Cielo y efectos", |ui| {
                    ui.add(egui::Slider::new(&mut s.stars, 0..=40000).text("Estrellas"));
                    ui.add(egui::Slider::new(&mut s.particles, 0.0..=3.0).text("Partículas"));
                });
            }
            if *s != before {
                self.preset = Preset::ALL.into_iter().find(|p| {
                    let mut q = Settings::preset(*p);
                    q.vsync = s.vsync;
                    q == *s
                });
                self.apply = true;
            }
        }
    }
}

/// A charge in kg of TNT, in the unit that reads best.
fn tnt_text(kg: f32) -> String {
    if kg >= 1e6 {
        format!("{:.2} kt de TNT", kg / 1e6)
    } else if kg >= 1e3 {
        format!("{:.1} t de TNT", kg / 1e3)
    } else {
        format!("{kg:.2} kg de TNT")
    }
}

fn pow2(ui: &mut egui::Ui, v: &mut u32, lo: u32, hi: u32, text: &str) {
    let mut e = v.trailing_zeros();
    if ui.add(egui::Slider::new(&mut e, lo.trailing_zeros()..=hi.trailing_zeros()).text(format!("{text}: {v}"))).changed() {
        *v = 1 << e;
    }
}

fn stats_window(ctx: &egui::Context, i: &Info) {
    let s = i.stats;
    let shown = egui::Window::new("CIFRAS  ·  F3").frame(crate::hud::frame()).default_pos([8.0, 8.0]).min_width(700.0).resizable(false).show(ctx, |ui| {
        ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
        ui.set_min_width(680.0);
        ui.columns(2, |cols| {
            let c = &mut cols[0];
            c.label(format!("{:.0} FPS  {:.2} ms", i.fps, i.frame_ms));
            c.label(format!("{} en {}  alt {:.1} m", i.mode, s.body, i.altitude));
            c.label(format!("vel {:.0} m/s x{:.0} = {:.0} m/s", i.speed, i.speed_factor, i.speed * i.speed_factor));
            let t = &s.terrain;
            c.label(format!("partículas {}  terreno: {} pend. {} regen. LOD x{:.2}", i.particles, t.pending, t.stale, t.split_scale));
            let st = &i.structures;
            c.label(format!("estructuras: {} activas {} gruesas {} dormidas", st.active, st.coarse, st.dormant));
            if !i.missile.is_empty() {
                c.label(&i.missile);
            }
            c.label("CPU ms");
            c.label(format!(" sim      {:6.3}", i.sim_ms));
            c.label(format!(" prep     {:6.3}", s.cpu_prepare_ms));
            c.label(format!(" terreno  {:6.3}", s.cpu_terrain_ms));
            c.label(format!(" escena   {:6.3}", s.cpu_scene_ms));
            c.label(format!(" codific. {:6.3}", s.cpu_encode_ms));
            c.label(format!(" envío    {:6.3}", s.cpu_submit_ms));
            let cpu = i.sim_ms + s.cpu_prepare_ms + s.cpu_terrain_ms + s.cpu_scene_ms + s.cpu_encode_ms + s.cpu_submit_ms;
            c.label(format!(" total    {cpu:6.3}"));
            c.label(if s.gpu_timed { "GPU ms" } else { "GPU ms (sin timestamps)" });
            for (k, n) in PASS_NAMES.iter().enumerate() {
                c.label(format!(" {n:<11}{:6.3}", s.gpu_ms[k]));
            }
            c.label(format!(" total      {:6.3}", s.gpu_ms.iter().sum::<f32>()));
            let c = &mut cols[1];
            c.label(format!("draws {} (principal {}, sombras {})", s.draws, s.draws_main, s.draws_shadow));
            c.label(format!("multi-draw {} → {} sub-draws", s.scene.multi_draws, s.scene.sub_draws));
            c.label(format!("triángulos {:.2} M", s.triangles as f64 / 1e6));
            c.label(format!("instancias {} ({} naves, {} NPC)", s.scene.instances, i.ships, i.npcs));
            c.label("visibles por LOD 0..5:");
            for (k, name) in s.scene.models.iter().enumerate() {
                let v = s.scene.visible.get(k).copied().unwrap_or_default();
                c.label(format!(" {name:<12}{:>5}{:>5}{:>5}{:>5}{:>5}{:>5}", v[0], v[1], v[2], v[3], v[4], v[5]));
            }
            c.label(format!(" en sombras {}", s.scene.shadow_visible));
            let t = &s.terrain;
            c.label(format!("terreno nodos {} (+{:?} sombra)", t.drawn[0], &t.drawn[1..]));
            c.label(format!(" caché {}/{}  nuevos {}  cola {}", t.cached, t.capacity, t.generated, t.pending));
            c.label(format!(" nivel {} / {}  {} tris/nodo", t.deepest, t.max_level, t.triangles_per_node));
            c.label(format!("memoria GPU {:.0} MB", s.gpu_bytes as f64 / 1048576.0));
            c.label(format!("resolución {}x{} ({:.0} %)", s.render_size.0, s.render_size.1, s.dynamic_res * 100.0));
            c.label(format!("sombras {:?}", s.shadow_actions));
            c.label(i.adapter);
        });
    });
    if let Some(shown) = shown {
        crate::hud::dress(ctx, shown.response.rect);
    }
}
