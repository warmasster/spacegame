//! The left column, the one that is always there: the name, the three editions as cards to choose
//! from (the ones the chosen version lacks are dull and say why), the big button and what it
//! starts, what the last thing done came to, and the small buttons.
use super::{COLUMN, parts};
use crate::{
    builds::{Build, Edition, GAME},
    look::{self, CardState, DIM, GOOD, TEXT, WARNING},
    model::{Action, Card, Model, Place},
    options::{self, Options},
    words,
};
use egui::{Rect, Sense, Ui, UiBuilder, pos2, text::LayoutJob, vec2};

/// The most letters of a server's address told on a card.
const ADDRESS_MOST: usize = 30;

/// What the card of the edition that is started says under its name, and whether it is something
/// wrong: what the edition is and, of the multiplayer one, where it joins and as whom.
pub fn chosen_says(build: &Build, o: &Options) -> (String, bool) {
    if build.edition != Edition::Multiplayer {
        return (build.edition.says().to_string(), false);
    }
    // (started in place of an edition its version lacks, it is told no server: see `options::joins`)
    if !options::joins(o, build) {
        return ("La demo con otros jugadores; ahora sin servidor: juegas solo.".to_string(), false);
    }
    match options::address(&o.server) {
        Ok(Some(server)) => (format!("Te unes a {} como {}.", words::shortened(&server, ADDRESS_MOST), options::name(&o.name)), false),
        Ok(None) => ("Sin dirección de servidor: juegas solo.".to_string(), false),
        Err(_) => ("La dirección del servidor no vale: mira la pestaña MULTIJUGADOR.".to_string(), true),
    }
}

/// What the big button says under its word: the build it starts; or, while something that was
/// started is still up, that (`busy`, as it is told); or why there is nothing to start.
pub fn starts(chosen: Option<&Build>, busy: Option<String>, ready: bool) -> String {
    match (busy, chosen) {
        (Some(busy), _) => busy,
        (None, Some(build)) if build.archived => format!("{} · ARCHIVADA", build.name()),
        (None, Some(build)) => build.name(),
        (None, None) if ready => "NO HAY VERSIONES".to_string(),
        (None, None) => "BUSCANDO VERSIONES…".to_string(),
    }
}

pub fn draw(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    let area = ui.max_rect();
    // ---- the name ----
    let (head, _) = ui.allocate_exact_size(vec2(COLUMN, 56.0), Sense::hover());
    look::emblem(ui.painter(), head.left_center() + vec2(27.0, 0.0), 54.0);
    let name = look::tracked(ui.painter(), GAME, 36.0, TEXT, 9.0);
    ui.painter().galley(head.left_top() + vec2(68.0, -4.0), name, TEXT);
    let under = look::tracked(ui.painter(), "LAUNCHER", 11.5, DIM, 4.2);
    ui.painter().galley(head.left_top() + vec2(70.0, 42.0), under, DIM);
    ui.add_space(4.0);
    // ---- the editions ----
    look::section(ui, "Elige edición");
    ui.add_space(1.0);
    let chosen = m.chosen().cloned();
    let mut picked = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 7.0;
        for (n, edition) in Edition::CARDS.into_iter().enumerate() {
            let (state, says, wrong) = match (m.card(edition), &chosen) {
                (Card::Chosen, Some(build)) => {
                    let (says, wrong) = chosen_says(build, &m.options);
                    (CardState::Chosen, says, wrong)
                }
                (Card::Missing(why), _) => (CardState::Missing, why, false),
                _ => (CardState::Free, edition.says().to_string(), false),
            };
            let card = look::card(ui, COLUMN, &(n + 1).to_string(), edition.name(), &says, state);
            if wrong {
                // (a red corner on the card of an edition that cannot start as it is)
                look::lamp(ui.painter(), pos2(card.rect.right() - 20.0, card.rect.bottom() - 16.0), Some(WARNING));
            }
            if card.clicked() {
                picked = Some(edition);
            }
        }
    });
    if let Some(edition) = picked {
        m.choose(edition);
    }
    ui.add_space(9.0);
    // ---- the big button ----
    let what = starts(chosen.as_ref(), m.busy_says(), m.ready);
    if look::big_button(ui, vec2(COLUMN, 90.0), "JUGAR", &what, chosen.is_some() && m.busy().is_none()).clicked() {
        action = Some(Action::Play);
    }
    let under = ui.cursor().top();
    let row = Rect::from_min_max(pos2(area.left(), area.bottom() - 34.0), area.max);
    // ---- what the last thing done came to: a notice, as the HUD gives them, over the small buttons ----
    if let Some(s) = &m.status {
        // (as many lines as there is room for between the big button and the small ones)
        let room = row.top() - 10.0 - (under + 10.0) - 16.0;
        let mut job = LayoutJob::single_section(s.text.clone(), parts::format(13.5, TEXT));
        job.wrap = egui::text::TextWrapping { max_width: COLUMN - 30.0, max_rows: ((room / 17.0).floor() as usize).clamp(1, 5), ..Default::default() };
        let words = ui.painter().layout_job(job);
        let plate = Rect::from_min_size(pos2(area.left(), row.top() - 26.0 - words.size().y), vec2(COLUMN, words.size().y + 16.0));
        look::plate(ui.painter(), plate, 1.0, Some(if s.good { GOOD } else { WARNING }), 1.0);
        ui.painter().galley(plate.min + vec2(16.0, 8.0), words, TEXT);
    }
    // ---- the small buttons, at the bottom ----
    let mut ui = ui.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
    ui.spacing_mut().item_spacing.x = 8.0;
    let side = 76.0;
    for (name, width, what) in [("MANUAL (LEEME)", COLUMN - 2.0 * (side + 8.0), Action::Open(Place::Manual)), ("FOTOS", side, Action::Open(Place::Photos)), ("SALIR", side, Action::Quit)] {
        if look::button(&mut ui, name, width).clicked() {
            action = Some(what);
        }
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(version: u32, edition: Edition) -> Build {
        Build { version, edition, file: String::new(), archived: false }
    }

    #[test]
    fn the_chosen_card_says_what_is_started() {
        let asked = |edition, server: &str| Options { edition, server: server.to_string(), name: " Fernando ".to_string(), ..Options::default() };
        assert_eq!(chosen_says(&build(35, Edition::Demo), &asked(Edition::Demo, "")), ("El juego tal como se juega.".to_string(), false));
        assert_eq!(chosen_says(&build(35, Edition::Debug), &asked(Edition::Debug, "")), ("Con herramientas de desarrollo: vuelo libre, catálogo, editor…".to_string(), false));
        assert_eq!(chosen_says(&build(21, Edition::Old), &asked(Edition::Demo, "")), ("De antes de que hubiera ediciones.".to_string(), false));
        // the multiplayer edition: where it joins and as whom, or that it plays alone, or that the address is wrong
        let multi = build(35, Edition::Multiplayer);
        assert_eq!(chosen_says(&multi, &asked(Edition::Multiplayer, "127.0.0.1:47600")), ("Te unes a 127.0.0.1:47600 como Fernando.".to_string(), false));
        assert_eq!(chosen_says(&multi, &asked(Edition::Multiplayer, " ")), ("Sin dirección de servidor: juegas solo.".to_string(), false));
        assert_eq!(chosen_says(&multi, &asked(Edition::Multiplayer, "127.0.0.1")), ("La dirección del servidor no vale: mira la pestaña MULTIJUGADOR.".to_string(), true));
        let long = chosen_says(&multi, &asked(Edition::Multiplayer, "un-servidor-con-un-nombre-muy-largo.example.org:47600")).0;
        assert!(long.starts_with("Te unes a un-servid…") && long.ends_with(".org:47600 como Fernando."), "{long}");
        // started in place of an edition its version lacks, it joins nothing, whatever the address
        assert_eq!(chosen_says(&multi, &asked(Edition::Demo, "no vale")), ("La demo con otros jugadores; ahora sin servidor: juegas solo.".to_string(), false));
    }

    #[test]
    fn the_big_button_says_exactly_what_it_starts() {
        assert_eq!(starts(Some(&build(36, Edition::Demo)), None, true), "V36 · DEMO");
        assert_eq!(starts(Some(&build(36, Edition::Debug)), None, true), "V36 · DEBUG");
        assert_eq!(starts(Some(&build(36, Edition::Multiplayer)), None, false), "V36 · MULTIJUGADOR");
        assert_eq!(starts(Some(&build(21, Edition::Old)), None, true), "V21 · ANTIGUA");
        assert_eq!(starts(Some(&Build { archived: true, ..build(30, Edition::Demo) }), None, true), "V30 · DEMO · ARCHIVADA");
        assert_eq!(starts(Some(&build(36, Edition::Demo)), Some("V35 · DEMO EN MARCHA".to_string()), true), "V35 · DEMO EN MARCHA");
        assert_eq!(starts(None, None, true), "NO HAY VERSIONES");
        assert_eq!(starts(None, None, false), "BUSCANDO VERSIONES…");
    }
}
