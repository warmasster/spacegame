//! The options: the graphics at a stroke (the presets) and one by one, the screen, the sound and
//! the start, what the launcher does while the game is played; each as it is passed to the game.
//! What the chosen build does not understand is dimmed and said.
use super::parts::{self, choices, note, warning};
use crate::{
    look,
    model::{Action, Model, Text},
    options::{self, AfterPlay, Backend, Preset, Quality},
};
use egui::Ui;

pub fn draw(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    let version = m.chosen().map_or(u32::MAX, |b| b.version);
    let (any, screen, mute) = (version >= options::FLAGS_SINCE, version >= options::SCREEN_SINCE, version >= options::MUTE_SINCE);
    let monitor = m.facts.screen.map(|s| (s.width, s.height));
    let o = &mut m.options;
    // ---- the graphics at a stroke ----
    ui.add_enabled_ui(any, |ui| {
        look::section(ui, "Gráficos");
        let now = Preset::of(o, monitor);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for preset in Preset::ALL {
                if look::choice(ui, preset.name(), now == Some(preset), 96.0).clicked() {
                    preset.apply(o, monitor);
                }
            }
        });
        note(ui, now.map_or("A medida: lo que hay elegido abajo no es ninguno de los cinco.", Preset::says));
    });
    if !any {
        note(ui, &format!("La V{version} es anterior a las opciones (valen desde la V{}): se inicia sin ninguna.", options::FLAGS_SINCE));
    }
    // ---- and one by one ----
    ui.add_enabled_ui(any, |ui| {
        look::section(ui, "Calidad, ajuste fino");
        choices(ui, &Quality::ALL, &mut o.quality, |q| q.name().to_string());
    });
    look::section(ui, "Pantalla");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.add_enabled_ui(screen, |ui| {
            for (full, name) in [(true, "PANTALLA COMPLETA"), (false, "VENTANA")] {
                if look::choice(ui, name, o.fullscreen == full, 0.0).clicked() {
                    o.fullscreen = full;
                }
            }
        });
        ui.add_space(16.0);
        ui.add_enabled(any, egui::Checkbox::new(&mut o.vsync, "Sincronía vertical"));
    });
    if !o.fullscreen {
        ui.add_enabled_ui(screen, |ui| choices(ui, &options::WINDOW_SIZES, &mut o.window, |(w, h)| format!("{w} × {h}")));
        if let Some((w, h)) = monitor.filter(|(w, h)| o.window.0 > *w || o.window.1 > *h) {
            note(ui, &format!("Esa ventana no cabe en esta pantalla ({w} × {h})."));
        }
    }
    if any && !screen {
        note(ui, &format!("La V{version} no entiende todavía el modo de pantalla, el tamaño de la ventana ni saltar el menú: valen desde la V{}.", options::SCREEN_SINCE));
    }
    ui.add_enabled_ui(any, |ui| {
        look::section(ui, "API gráfica");
        choices(ui, &Backend::ALL, &mut o.backend, |b| b.name().to_string());
    });
    look::section(ui, "Sonido y arranque");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 26.0;
        ui.add_enabled(mute, egui::Checkbox::new(&mut o.sound, "Sonido"));
        ui.add_enabled(screen, egui::Checkbox::new(&mut o.skip_menu, "Saltar el menú de inicio"));
    });
    if any && !mute {
        note(ui, &format!("La V{version} no tiene sonido que apagar (lo hay desde la V{}).", options::MUTE_SINCE));
    }
    // ---- a developer's own ----
    ui.add_enabled_ui(any, |ui| {
        look::section(ui, "Argumentos extra (para desarrollar)");
        look::field(ui, &mut o.extra, ui.available_width(), options::EXTRA_MOST, "por ejemplo: --ships 100 --npcs 0");
        note(ui, "Se añaden tal cual al final de la línea de órdenes. Si la versión no entiende alguno se cierra al arrancar, y el launcher dice cuál.");
    });
    // ---- the launcher itself ----
    look::section(ui, "Al empezar la partida, el launcher");
    choices(ui, &AfterPlay::ALL, &mut o.after_play, |a| a.name().to_string());
    note(ui, o.after_play.says());
    // ---- what it all comes to ----
    if m.chosen().is_some() {
        look::section(ui, "Así se inicia");
        match m.command() {
            Ok((build, args)) => {
                parts::command_line(ui, &build.path(), &args);
                ui.add_space(4.0);
                if look::button(ui, "COPIAR", 0.0).clicked() {
                    action = Some(Action::Copy(Text::CommandLine));
                }
            }
            Err(why) => warning(ui, &why),
        }
    }
    action
}
