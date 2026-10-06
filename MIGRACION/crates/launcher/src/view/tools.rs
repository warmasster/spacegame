//! The tools of whoever develops the game, in four pages: the camera scripts (each with what it
//! says it does) run in the debug build with no window; a build's check of its own start and the
//! ship tools' server; the documents; the logs.
use super::parts::{self, note, warning};
use crate::{
    builds,
    look::{self, AMBER, BODY, DIM, GOOD, TEXT, WARNING},
    model::{Action, Model, Place, Text},
    options,
    tools::{self, Page},
    words,
};
use egui::{Color32, FontId, Response, Sense, Ui, pos2, vec2};

pub fn draw(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    ui.add_space(4.0);
    let on = Page::ALL.iter().position(|page| *page == m.page);
    if let Some(n) = look::segments(ui, &Page::ALL.map(Page::name), on, ui.available_width()) {
        m.page = Page::ALL[n];
    }
    match m.page {
        Page::Scripts => scripts(ui, m),
        Page::Check => check(ui, m),
        Page::Docs => docs(ui, m),
        Page::Logs => logs(ui, m),
    }
}

/// A row of a list of two lines: a name and, under it, what it is, each cut to the width there
/// is; at its right, a word.
fn item(ui: &mut Ui, name: &str, says: &str, right: &str, chosen: bool, mono: bool) -> Response {
    let width = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(width, if says.is_empty() { 30.0 } else { 46.0 }), Sense::click());
    if !ui.is_rect_visible(r) {
        return resp;
    }
    let painter = ui.painter();
    look::row(painter, r, chosen, resp.hovered());
    let (ink, soft) = if chosen { (Color32::WHITE, Color32::from_rgb(198, 226, 236)) } else { (TEXT, DIM) };
    let mut room = width - 26.0;
    if !right.is_empty() {
        let word = painter.layout_no_wrap(right.to_string(), FontId::proportional(12.5), soft);
        room -= word.size().x + 14.0;
        painter.galley(pos2(r.right() - 12.0 - word.size().x, r.top() + 8.0), word, soft);
    }
    let font = if mono { FontId::monospace(13.5) } else { FontId::proportional(15.0) };
    let mut job = egui::text::LayoutJob::single_section(name.to_string(), egui::TextFormat { font_id: font, color: ink, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: room, max_rows: 1, break_anywhere: true, ..Default::default() };
    painter.galley(pos2(r.left() + 14.0, r.top() + 5.0), painter.layout_job(job), ink);
    if !says.is_empty() {
        painter.galley(pos2(r.left() + 14.0, r.top() + 25.0), look::line(painter, says, 12.5, soft, width - 26.0), soft);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The camera scripts: what running the chosen one comes to, and the list to choose it from.
fn scripts(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    look::section(ui, "Guiones de cámara");
    note(ui, &format!("Un guion coloca la cámara, acciona mandos y hace fotos él solo, sin abrir ventana; al acabar, el juego se cierra. Están en {} y sus fotos quedan en {}.", builds::SCRIPTS, tools::SCRIPT_PHOTOS));
    ui.add_space(6.0);
    let command = m.script_command().map(|(build, args)| (build.path(), build.version, args));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if ui.add_enabled_ui(command.is_ok() && m.busy().is_none(), |ui| look::button(ui, "EJECUTAR GUION", 170.0)).inner.clicked() {
            action = Some(Action::RunScript);
        }
        if look::button(ui, "FOTOS DE GUION", 0.0).clicked() {
            action = Some(Action::Open(Place::ScriptPhotos));
        }
    });
    ui.add_space(2.0);
    match &command {
        Ok((file, version, args)) => {
            parts::command_line(ui, file, args);
            if *version < options::BOOT_SINCE {
                note(ui, &format!("(La V{version} enseña su ventana mientras corre el guion: sin ventana, desde la V{}.)", options::BOOT_SINCE));
            }
        }
        Err(why) => warning(ui, why),
    }
    ui.add_space(8.0);
    let mut picked = None;
    let mut run = false;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (n, script) in m.found.scripts.iter().enumerate() {
            let row = item(ui, &script.name, &script.says, "", n == m.script, false);
            if row.clicked() {
                picked = Some(n);
            }
            run |= row.double_clicked();
        }
    });
    if let Some(n) = picked {
        m.script = n;
    }
    if run && command.is_ok() && m.busy().is_none() {
        action = Some(Action::RunScript);
    }
    if m.found.scripts.is_empty() {
        note(ui, &if m.ready { format!("No hay guiones (*.jsonc) en {}.", builds::SCRIPTS) } else { "Mirando la carpeta del juego…".to_string() });
    }
    action
}

/// A build's check of its own start, how the game's last start went, and the ship tools' server.
fn check(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    look::section(ui, "Comprobar el arranque");
    note(ui, "La versión elegida arranca como para jugar pero sin enseñar ventana, dibuja unos fotogramas y se cierra. Si algo le falta (un fichero, la gráfica, una opción que no entiende) lo dice aquí.");
    ui.add_space(6.0);
    let command = m.boot_command().map(|(build, args)| (build.path(), args));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if ui.add_enabled_ui(command.is_ok() && m.busy().is_none(), |ui| look::button(ui, "COMPROBAR ARRANQUE", 0.0)).inner.clicked() {
            action = Some(Action::CheckBoot);
        }
        if m.check.is_some() && look::button(ui, "COPIAR", 0.0).clicked() {
            action = Some(Action::Copy(Text::Check));
        }
    });
    ui.add_space(2.0);
    match &command {
        Ok((file, args)) => parts::command_line(ui, file, args),
        Err(why) => warning(ui, why),
    }
    if m.checking() {
        ui.add_space(6.0);
        parts::words(ui, "Comprobando… tarda lo que tarda el juego en arrancar (unos diez segundos). No se abre ninguna ventana.", AMBER);
    }
    if let Some(check) = &m.check {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(format!("{}: {}", check.name, check.verdict)).size(15.0).color(if check.good { GOOD } else { WARNING }));
        if !check.output.is_empty() {
            parts::printed(ui, &check.output);
        }
    }
    look::section(ui, "Último arranque del juego");
    match &m.found.boot {
        Some(boot) => {
            ui.label(egui::RichText::new(format!("{} s en cargar", words::decimal(f64::from(boot.total), 1))).size(15.0).color(TEXT));
            let stages: Vec<String> = boot.stages.iter().map(|(name, seconds)| format!("{name} {} s", words::decimal(f64::from(*seconds), 1))).collect();
            note(ui, &format!("{}. Lo apunta el juego cada vez que arranca ({}/arranque.json).", stages.join("  ·  "), tools::OUT));
        }
        None => note(ui, &format!("El juego todavía no ha apuntado cuánto tarda en arrancar ({}/arranque.json).", tools::OUT)),
    }
    look::section(ui, "Herramientas de naves (MCP)");
    let (says, all) = m.found.mcp.says();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 20.0), Sense::hover());
        look::lamp(ui.painter(), r.center(), all.then_some(GOOD));
        ui.add(egui::Label::new(egui::RichText::new(says).size(14.0).color(if all { BODY } else { AMBER })).wrap());
    });
    action
}

/// The documents, each with its title: a click opens it.
fn docs(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    look::section(ui, "Documentos");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if look::button(ui, "MANUAL (LEEME.TXT)", 0.0).clicked() {
            action = Some(Action::Open(Place::Manual));
        }
        if look::button(ui, "CARPETA DOCS", 0.0).clicked() {
            action = Some(Action::Open(Place::Docs));
        }
    });
    ui.add_space(6.0);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (n, doc) in m.found.docs.iter().enumerate() {
            let title = if doc.title.is_empty() { &doc.file } else { &doc.title };
            if item(ui, title, "", &format!("{}/{}", tools::DOCS, doc.file), false, false).clicked() {
                action = Some(Action::Open(Place::Doc(n)));
            }
        }
    });
    if m.found.docs.is_empty() {
        note(ui, &if m.ready { format!("No hay documentos (*.md) en {}.", tools::DOCS) } else { "Mirando la carpeta del juego…".to_string() });
    }
    action
}

/// The logs, the one written last first, each with its date, its size and what its first line
/// says: a click opens it.
fn logs(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    look::section(ui, "Registros");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if look::button(ui, "CARPETA OUT", 0.0).clicked() {
            action = Some(Action::Open(Place::Out));
        }
        if look::button(ui, "FOTOS DE GUION", 0.0).clicked() {
            action = Some(Action::Open(Place::ScriptPhotos));
        }
        if look::button(ui, "ACTUALIZAR", 0.0).clicked() {
            action = Some(Action::Rescan);
        }
    });
    ui.add_space(6.0);
    let (now, clock) = (words::now(), m.found.clock());
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (n, log) in m.found.logs.iter().enumerate() {
            let facts = format!("{}  ·  {}", clock.ago(log.modified, now), words::size(log.size));
            if item(ui, &log.path, &log.first, &facts, false, true).clicked() {
                action = Some(Action::Open(Place::Log(n)));
            }
        }
    });
    if m.found.logs.is_empty() {
        note(ui, &if m.ready { format!("No hay registros: el juego los deja en {} cuando algo va mal.", tools::OUT) } else { "Mirando la carpeta del juego…".to_string() });
    }
    action
}
