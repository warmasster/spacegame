//! What the launcher knows of the machine and of the game: the graphics cards, the screen, the
//! game's folder and what its versions take, how the last game went and what has been played;
//! and the button that puts all of it on the clipboard for whoever is asked for help.
use super::parts::{self, fact, note};
use crate::{
    builds::ARCHIVE,
    look::{self, AMBER, BODY, GOOD, WARNING},
    model::{Action, Model, Place, Text},
    options, words,
};
use egui::Ui;

/// The most builds told of in the history.
const MOST: usize = 4;

pub fn draw(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    let (now, clock) = (words::now(), m.found.clock());
    let names = ["Tarjeta gráfica", "Pantalla", "Sistema", "Carpeta", "Versiones", "Última partida", "En total", "Lo más jugado", "Launcher"];
    let wide = parts::widest(ui, &names);
    // ---- the machine ----
    look::section(ui, "Este equipo");
    for gpu in &m.facts.gpus {
        let used = if gpu.used { "  ·  la usa el launcher" } else { "" };
        fact(ui, wide, names[0], &format!("{}  ·  {}{used}", gpu.says(), gpu.api), BODY);
    }
    if m.facts.gpus.is_empty() {
        fact(ui, wide, names[0], "desconocida", BODY);
    }
    fact(ui, wide, names[1], &m.facts.screen.map_or("desconocida".to_string(), |s| s.says()), BODY);
    fact(ui, wide, names[2], m.found.known.windows.as_deref().unwrap_or(if m.ready { "desconocido" } else { "…" }), BODY);
    // ---- the game ----
    look::section(ui, "El juego");
    fact(ui, wide, names[3], &m.root.display().to_string(), BODY);
    let (here, away) = m.found.disk;
    let put_away = if away.versions == 0 { String::new() } else { format!("; {} más archivadas en {ARCHIVE} ({})", away.versions, words::size(away.bytes)) };
    fact(ui, wide, names[4], &format!("{} a mano ({}, {}){put_away}", here.versions, words::plural(here.files, "ejecutable", "ejecutables"), words::size(here.bytes)), BODY);
    if m.ready && !(m.found.manual && m.found.assets) {
        let missing = match (m.found.manual, m.found.assets) {
            (false, false) => "Faltan LEEME.txt y la carpeta assets: esta no parece la carpeta del juego.",
            (false, true) => "Falta LEEME.txt: no hay novedades que contar.",
            _ => "Falta la carpeta assets: el juego no arrancará desde aquí.",
        };
        parts::words(ui, missing, AMBER);
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
        if look::button(ui, "COPIAR DIAGNÓSTICO", 0.0).clicked() {
            action = Some(Action::Copy(Text::Diagnostic));
        }
        if look::button(ui, "ABRIR LA CARPETA", 0.0).clicked() {
            action = Some(Action::Open(Place::Root));
        }
        if look::button(ui, "ACTUALIZAR (F5)", 0.0).clicked() {
            action = Some(Action::Rescan);
        }
    });
    // ---- what was played ----
    look::section(ui, "Partidas");
    match &m.history.last {
        Some(last) => {
            let ink = match last.good() {
                Some(true) => GOOD,
                Some(false) => WARNING,
                None => BODY,
            };
            fact(ui, wide, names[5], &format!("V{} · {}, {}: {}.", last.version, last.edition.name(), clock.ago(last.started, now), last.says()), ink);
        }
        None => fact(ui, wide, names[5], "ninguna desde este launcher", BODY),
    }
    if let Some(total) = m.history.total().says(clock, now) {
        fact(ui, wide, names[6], &total, BODY);
        let most: Vec<String> = m.history.most().into_iter().take(MOST).filter_map(|(version, edition, played)| Some(format!("V{version} · {} ({})", edition.name(), played.says(clock, now)?))).collect();
        fact(ui, wide, names[7], &most.join("\n"), BODY);
    }
    if m.options.after_play == options::AfterPlay::Close {
        note(ui, "Con el launcher cerrándose al empezar la partida se cuentan las partidas, pero no lo que duran (OPCIONES).");
    }
    // ---- the launcher itself ----
    look::section(ui, "Launcher");
    let start = m.facts.start_ms.map_or(String::new(), |ms| format!("  ·  abierto en {ms} ms"));
    let looked = if m.ready { format!("  ·  carpeta mirada en {} ms", m.found.took_ms) } else { String::new() };
    fact(ui, wide, names[8], &format!("{}{start}{looked}  ·  guarda en {}", crate::VERSION, options::FILE), BODY);
    action
}
