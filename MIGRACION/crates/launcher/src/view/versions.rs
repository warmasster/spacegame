//! The first tab, the one the launcher opens on: the list of versions (the newest first, each
//! with its date, its size, the editions it has and what it brought) and, beside it, the chosen
//! one told whole: what was played of it and its news as the manual tells them. Putting the old
//! versions away is asked here too, in the place of the news.
use super::parts::{self, note};
use crate::{
    archive,
    builds::{self, ARCHIVE, Edition},
    look::{self, ACCENT, AMBER, BODY, DIM, GOOD, RULE, TEXT, WARNING},
    model::{Action, Model, Place},
    news::Block,
    options,
    versions::Version,
    words,
};
use egui::{Color32, FontId, Rect, Response, Sense, Stroke, Ui, UiBuilder, pos2, text::LayoutJob, vec2};

/// The width of the list: as it is wanted, and the least it takes on a narrow plate.
const LIST: (f32, f32) = (338.0, 310.0);

/// The editions as they are marked on a row, in the order of their cards.
const MARKS: [(Edition, &str); 3] = [(Edition::Demo, "DEMO"), (Edition::Debug, "DEBUG"), (Edition::Multiplayer, "MULTI")];

pub fn draw(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let area = ui.available_rect_before_wrap();
    let list = Rect::from_min_max(area.min, pos2(area.left() + (area.width() * 0.45).clamp(LIST.1, LIST.0), area.bottom()));
    let detail = Rect::from_min_max(pos2(list.right() + 24.0, area.top()), area.max);
    ui.painter().line_segment([pos2(list.right() + 11.0, area.top() + 6.0), pos2(list.right() + 11.0, area.bottom())], Stroke::new(1.0, RULE));
    let picked = versions(&mut ui.new_child(UiBuilder::new().max_rect(list)), m);
    let asked = chosen(&mut ui.new_child(UiBuilder::new().max_rect(detail)), m);
    picked.or(asked)
}

/// The list and, under it, what it takes on the disk and the button that puts the old ones away.
fn versions(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    let area = ui.max_rect();
    let foot = Rect::from_min_max(pos2(area.left(), area.bottom() - 34.0), area.max);
    let rows = Rect::from_min_max(area.min, pos2(area.right(), foot.top() - 8.0));
    let chosen = m.version().map(|v| v.number);
    // (the one the launcher picks by itself is told: it is the one a newer version will replace)
    let follows = m.options.version.is_none().then_some(chosen).flatten();
    let (mut picked, mut play) = (None, false);
    let mut list = ui.new_child(UiBuilder::new().max_rect(rows));
    look::section(&mut list, "Elige versión");
    if m.found.versions.is_empty() {
        list.add_space(8.0);
        note(&mut list, &if m.ready { format!("No hay ninguna versión del juego ({}) en {}.", builds::any_name(), m.root.display()) } else { "Buscando versiones…".to_string() });
    }
    egui::ScrollArea::vertical().id_salt("versiones").auto_shrink([false, false]).show(&mut list, |ui| {
        ui.set_width(ui.available_width() - 12.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        for version in &m.found.versions {
            let row = row(ui, version, chosen == Some(version.number), follows == Some(version.number), m.fresh.contains(&version.number), m);
            if row.clicked() {
                picked = Some(version.number);
            }
            play |= row.double_clicked();
            if m.reveal && chosen == Some(version.number) {
                row.scroll_to_me(None);
            }
        }
    });
    if m.ready {
        m.reveal = false;
    }
    if let Some(version) = picked {
        m.select(version);
    }
    if play {
        action = Some(Action::Play);
    }
    // ---- what they take, and putting the old ones away ----
    let (here, away) = m.found.disk;
    let plan = archive::plan(&m.found.versions, m.options.keep_versions);
    let mut ui = ui.new_child(UiBuilder::new().max_rect(foot).layout(egui::Layout::right_to_left(egui::Align::Center)));
    ui.add_enabled_ui(!plan.is_empty() && m.asking.is_none(), |ui| {
        if look::button(ui, "ARCHIVAR…", 0.0).on_hover_text("Apartar las versiones antiguas a una carpeta, sin borrar nada").clicked() {
            action = Some(Action::AskArchive);
        }
    });
    let takes = match (here.versions, away.versions) {
        (0, 0) => String::new(),
        (_, 0) => format!("{} · {}", words::plural(here.versions, "versión", "versiones"), words::size(here.bytes)),
        _ => format!("{} a mano · {} · {}", here.versions, words::size(here.bytes), words::plural(away.versions, "archivada", "archivadas")),
    };
    let room = ui.available_width() - 8.0;
    let said = look::line(ui.painter(), &takes, 12.5, DIM, room);
    ui.painter().galley(pos2(foot.left() + 2.0, foot.center().y - said.size().y * 0.5), said, DIM);
    action
}

/// A row of the list: the version's number and what it is besides (the one the launcher follows,
/// one put away); at its right, the editions it has; under them its date, its size and how many
/// times it was played; and, if the manual tells, what it brought.
fn row(ui: &mut Ui, v: &Version, chosen: bool, follows: bool, fresh: bool, m: &Model) -> Response {
    let width = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(width, if v.headline.is_empty() { 49.0 } else { 67.0 }), Sense::click());
    if !ui.is_rect_visible(r) {
        return resp;
    }
    // (what a row says is put into words only if it is in sight: the list is long)
    let when = if v.modified == 0 { String::new() } else { m.found.clock().moment(v.modified, words::now()) };
    let played = m.history.of_version(v.number);
    let painter = ui.painter();
    look::row(painter, r, chosen, resp.hovered());
    let (ink, soft) = match (chosen, v.archived) {
        (true, _) => (Color32::WHITE, Color32::from_rgb(198, 226, 236)),
        (false, true) => (DIM, DIM.gamma_multiply(0.8)),
        (false, false) => (TEXT, DIM),
    };
    // ---- the editions it has, from the right ----
    let mut right = r.right() - 12.0;
    let mark = |right: &mut f32, word: &str, lit: bool| {
        let colour = if lit { if chosen { Color32::WHITE } else { Color32::from_rgb(178, 222, 236) } } else { Color32::from_rgb(64, 78, 90) };
        let g = look::tracked(painter, word, 10.5, colour, 1.2);
        *right -= g.size().x - 1.2;
        painter.galley(pos2(*right, r.top() + 12.0), g, colour);
        *right -= 9.0;
    };
    if v.old() {
        mark(&mut right, "EDICIÓN ÚNICA", true);
    } else {
        for (edition, word) in MARKS.iter().rev() {
            mark(&mut right, word, v.has(*edition));
        }
    }
    // ---- its number, and what it is besides: each in the longest of its words there is room for ----
    let number = painter.layout_no_wrap(format!("V{}", v.number), FontId::proportional(19.0), ink);
    let mut x = r.left() + 14.0;
    let wide = number.size().x;
    painter.galley(pos2(x, r.top() + 5.0), number, ink);
    x += wide + 10.0;
    let mut tag = |words: &[&str], colour: Color32| {
        if let Some(wide) = words.iter().map(|word| look::tag(painter, pos2(x, r.top() + 10.0), word, colour, right - x)).find(|wide| *wide > 0.0) {
            x += wide + 6.0;
        }
    };
    if fresh {
        tag(&["NUEVA"], GOOD);
    }
    if follows {
        tag(if fresh { &["RECIENTE"] } else { &["LA MÁS RECIENTE", "RECIENTE"] }, ACCENT);
    }
    if v.archived {
        tag(&["ARCHIVADA"], AMBER);
    } else if v.some_archived {
        tag(&["ARCHIVADA EN PARTE", "EN PARTE"], AMBER);
    }
    if played.failed {
        tag(&["FALLÓ"], WARNING);
    }
    // ---- when it was made, what it takes, how much it was played ----
    let facts = [when, words::size(v.size)].into_iter().filter(|f| !f.is_empty() && v.size > 0).collect::<Vec<_>>().join("  ·  ");
    painter.text(pos2(r.left() + 14.0, r.top() + 31.0), egui::Align2::LEFT_TOP, facts, FontId::proportional(12.5), soft);
    if played.starts > 0 {
        let times = if played.starts == 1 { "jugada 1 vez".to_string() } else { format!("jugada {} veces", played.starts) };
        painter.text(pos2(r.right() - 12.0, r.top() + 31.0), egui::Align2::RIGHT_TOP, times, FontId::proportional(12.5), soft);
    }
    // ---- what it brought ----
    if !v.headline.is_empty() {
        let line = look::line(painter, &v.headline, 12.5, soft, width - 26.0);
        painter.galley(pos2(r.left() + 14.0, r.top() + 47.0), line, soft);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The chosen version told whole; or, while it is asked, the putting away of the old ones.
fn chosen(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    if m.asking.is_some() {
        return put_away(ui, m);
    }
    let Some(version) = m.version().cloned() else {
        ui.add_space(14.0);
        note(ui, if m.ready { "Sin versiones no hay novedades que contar." } else { "Mirando la carpeta del juego…" });
        return None;
    };
    let mut action = None;
    let (now, clock) = (words::now(), m.found.clock());
    look::section(ui, &m.news().map_or(format!("Versión {}", version.number), |news| news.title.clone()));
    // ---- its files, and what was played of it ----
    let made = if version.modified == 0 { String::new() } else { format!("{}  ·  ", clock.moment(version.modified, now)) };
    let editions = if version.old() { "edición única".to_string() } else { version.editions.iter().map(|e| e.word()).collect::<Vec<_>>().join(", ") };
    ui.add(egui::Label::new(egui::RichText::new(format!("{made}{}  ·  {editions}", words::size(version.size))).size(14.0).color(BODY)).wrap());
    let played = m.history.of_version(version.number);
    match played.says(clock, now) {
        Some(says) => note(ui, &format!("Jugada {says}.")),
        None => note(ui, "Todavía no la has jugado desde este launcher."),
    }
    if played.failed {
        // (an old build reads the assets of now: one that no longer understands them stops as it starts)
        parts::warning(ui, "La última vez se cerró con un error. Una versión antigua puede no entender ya los datos del juego (assets), que son los de la más reciente.");
    }
    if version.some_archived {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if look::button(ui, "RECUPERAR", 0.0).clicked() {
                action = Some(Action::Restore(version.number));
            }
            ui.add(egui::Label::new(egui::RichText::new(format!("Archivada en {ARCHIVE}: se juega desde ahí igual.")).size(13.5).color(AMBER)).wrap());
        });
    }
    if version.number < options::FLAGS_SINCE {
        note(ui, &format!("Se inicia sin opciones: el juego las entiende desde la V{}.", options::FLAGS_SINCE));
    }
    ui.add_space(4.0);
    let y = ui.cursor().top();
    ui.painter().line_segment([pos2(ui.max_rect().left(), y), pos2(ui.max_rect().right(), y)], Stroke::new(1.0, RULE));
    ui.add_space(2.0);
    // ---- what it brought ----
    egui::ScrollArea::vertical().id_salt(("novedades", version.number)).auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width() - 14.0);
        match m.news() {
            Some(news) => {
                for block in &news.blocks {
                    paragraph(ui, block);
                }
            }
            None => {
                ui.add_space(8.0);
                note(
                    ui,
                    &if m.found.manual {
                        format!("El manual (LEEME.txt) no cuenta qué trajo la V{}: sus novedades empiezan en la V{}.", version.number, m.found.sections.last().map_or(0, |s| s.version))
                    } else {
                        format!("No hay novedades que contar: no se encuentra LEEME.txt en {}.", m.root.display())
                    },
                );
            }
        }
        ui.add_space(8.0);
    });
    action
}

/// Putting the old versions away, asked before it is done: how many stay, what would move and
/// where to, and the two answers.
fn put_away(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    look::section(ui, "Archivar versiones antiguas");
    ui.add_space(2.0);
    ui.add(egui::Label::new(egui::RichText::new("Las versiones viejas ocupan sitio y alargan la lista. Se pueden apartar a una carpeta sin perderlas.").size(14.5).color(BODY)).wrap());
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.label(egui::RichText::new("Se quedan a mano las últimas").size(14.5));
        parts::choices(ui, &options::KEEP, &mut m.options.keep_versions, |n| n.to_string());
    });
    let plan = archive::plan(&m.found.versions, m.options.keep_versions);
    ui.add_space(10.0);
    ui.add(egui::Label::new(egui::RichText::new(plan.says()).size(15.0).color(TEXT)).wrap());
    ui.add_space(4.0);
    note(ui, "No se borra nada. Siguen en la lista, marcadas ARCHIVADA; se juegan desde ahí y con RECUPERAR vuelven a su sitio.");
    ui.add_space(14.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
        ui.add_enabled_ui(!plan.is_empty(), |ui| {
            if look::button(ui, &format!("ARCHIVAR {}", words::plural(plan.versions.len(), "VERSIÓN", "VERSIONES")), 0.0).clicked() {
                action = Some(Action::Archive);
            }
        });
        if look::button(ui, "CANCELAR", 0.0).clicked() {
            action = Some(Action::Cancel);
        }
        if m.found.disk.1.versions > 0 && look::button(ui, "ABRIR LA CARPETA", 0.0).clicked() {
            action = Some(Action::Open(Place::Archive));
        }
    });
    action
}

/// One paragraph of the news: a heading over its text, or an item set in behind its mark.
fn paragraph(ui: &mut Ui, b: &Block) {
    let read = egui::TextFormat { line_height: Some(20.0), ..parts::format(14.5, BODY) };
    if b.mark.is_empty() {
        // a heading stands apart from what is above it and close to its own text
        ui.add_space(if b.lead.is_empty() { 0.0 } else { 9.0 });
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            if !b.lead.is_empty() {
                let mut job = LayoutJob::default();
                job.append(&b.lead, 0.0, egui::TextFormat { extra_letter_spacing: 1.5, ..parts::format(13.0, TEXT) });
                ui.add(egui::Label::new(job).wrap());
            }
            if !b.text.is_empty() {
                let text = if b.lead.is_empty() { b.text.clone() } else { words::capitalised(&b.text) };
                ui.add(egui::Label::new(LayoutJob::single_section(text, read)).wrap());
            }
        });
        return;
    }
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (r, _) = ui.allocate_exact_size(vec2(24.0, 20.0), Sense::hover());
        if b.mark == "-" {
            ui.painter().line_segment([r.left_center() + vec2(7.0, 0.0), r.left_center() + vec2(14.0, 0.0)], Stroke::new(1.6, ACCENT));
        } else {
            ui.painter().text(r.left_center() + vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, &b.mark, FontId::proportional(14.5), ACCENT);
        }
        ui.vertical(|ui| {
            let mut job = LayoutJob::default();
            if !b.lead.is_empty() {
                job.append(&format!("{}: ", b.lead), 0.0, egui::TextFormat { line_height: Some(20.0), ..parts::format(14.5, TEXT) });
            }
            job.append(&b.text, 0.0, read);
            ui.add(egui::Label::new(job).wrap());
        });
    });
}
