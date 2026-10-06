//! The launcher's window, in egui: space behind; on the left, always there, what is played — the
//! three editions as cards and the big button that says what it starts; on the right a plate of
//! glass with its tabs: the versions (the first thing seen: the list, and the chosen one's news),
//! the options, playing with others, the developer's tools, the machine; a foot line with the
//! keys. Each tab is a module of its own. It changes the options in place and returns what the
//! player asked for.
mod column;
mod multiplayer;
mod parts;
mod settings;
mod system;
mod tools;
mod versions;

use crate::{
    builds::Edition,
    look::{self, ACCENT, DIM, RULE},
    model::{Action, Model, Tab},
    space::Backdrop,
    words,
};
use egui::{Key, Rect, Stroke, Ui, UiBuilder, pos2, vec2};

/// The margin round the window, the width of the left column and the height of the foot.
const MARGIN: f32 = 30.0;
const COLUMN: f32 = 318.0;
const FOOT: f32 = 36.0;
/// How many rows a page key moves the chosen version.
const PAGE: i32 = 5;

/// What the window keeps from one frame to the next.
#[derive(Default)]
pub struct View {
    backdrop: Backdrop,
}

/// What the keys pressed in a frame ask for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Keys {
    /// Rows down the list of versions (up, if negative).
    rows: i32,
    /// Tabs to the right (to the left, if negative).
    tabs: i32,
    /// The edition asked for by its number.
    edition: Option<Edition>,
    enter: bool,
    escape: bool,
    refresh: bool,
}

impl Keys {
    fn read(i: &egui::InputState) -> Keys {
        let pressed = |key: Key| i32::from(i.key_pressed(key));
        let far = i32::MAX / 2;
        Keys {
            rows: pressed(Key::ArrowDown) - pressed(Key::ArrowUp) + PAGE * (pressed(Key::PageDown) - pressed(Key::PageUp)) + far * (pressed(Key::End) - pressed(Key::Home)),
            tabs: pressed(Key::ArrowRight) - pressed(Key::ArrowLeft),
            edition: [Key::Num1, Key::Num2, Key::Num3].iter().position(|key| i.key_pressed(*key)).map(|n| Edition::CARDS[n]),
            enter: i.key_pressed(Key::Enter),
            escape: i.key_pressed(Key::Escape),
            refresh: i.key_pressed(Key::F5),
        }
    }

    /// Do what they ask of the model; what is left to do by whoever has the window.
    fn apply(self, m: &mut Model) -> Option<Action> {
        if self.rows != 0 {
            m.step(self.rows);
        }
        if self.tabs != 0 {
            let at = Tab::ALL.iter().position(|tab| *tab == m.tab).unwrap_or(0) as i32;
            m.tab = Tab::ALL[(at + self.tabs).clamp(0, Tab::ALL.len() as i32 - 1) as usize];
        }
        if let Some(edition) = self.edition {
            m.choose(edition);
        }
        // (while the launcher asks whether to put the old versions away, Enter is no answer to
        // that — nothing is moved by a key — and Esc is "no")
        match (m.asking.is_some(), self.escape, self.enter, self.refresh) {
            (true, true, ..) => Some(Action::Cancel),
            (false, true, ..) => Some(Action::Quit),
            (false, _, true, _) => Some(Action::Play),
            (_, _, _, true) => Some(Action::Rescan),
            _ => None,
        }
    }
}

impl View {
    /// The whole window for this frame.
    pub fn draw(&mut self, ui: &mut Ui, m: &mut Model) -> Option<Action> {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        self.backdrop.paint(ui.painter(), screen, ctx.pixels_per_point());
        // the keys choose and play, unless a list is open or something that is typed in has them
        let free = !egui::Popup::is_any_open(&ctx) && ctx.memory(|mem| mem.focused().is_none());
        let action = if free { ctx.input(Keys::read).apply(m) } else { None };
        let body = Rect::from_min_max(screen.min + vec2(MARGIN, 24.0), screen.max - vec2(26.0, FOOT + 12.0));
        let left = Rect::from_min_size(body.min, vec2(COLUMN, body.height()));
        let right = Rect::from_min_max(pos2(left.right() + 24.0, body.top()), body.max);
        let asked = column::draw(&mut ui.new_child(UiBuilder::new().max_rect(left)), m).or(plate(ui, right, m));
        foot(ui, screen, m);
        action.or(asked)
    }
}

/// The plate on the right: its tabs as words, and what the open one holds under them.
fn plate(ui: &mut Ui, rect: Rect, m: &mut Model) -> Option<Action> {
    look::plate(ui.painter(), rect, 1.6, Some(ACCENT), 1.0);
    let mut ui = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(26.0, 16.0))));
    let ui = &mut ui;
    // (five tabs take some 690 points as they are lettered: on a narrower plate they come closer)
    let roomy = ui.available_width() >= 700.0;
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = look::tab_lettering(roomy).2;
        for tab in Tab::ALL {
            if look::tab(ui, tab.name(), m.tab == tab, roomy).clicked() {
                m.tab = tab;
            }
        }
    });
    let y = row.response.rect.bottom();
    ui.painter().line_segment([pos2(ui.max_rect().left(), y), pos2(ui.max_rect().right(), y)], Stroke::new(1.0, RULE));
    ui.add_space(4.0);
    // (the versions are two columns, each scrolling by itself; the other tabs scroll whole)
    if m.tab == Tab::Versions {
        return versions::draw(ui, m);
    }
    let mut action = None;
    egui::ScrollArea::vertical().id_salt(m.tab as u8).auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width() - 14.0);
        action = match m.tab {
            Tab::Versions => None,
            Tab::Options => settings::draw(ui, m),
            Tab::Multiplayer => multiplayer::draw(ui, m),
            Tab::Tools => tools::draw(ui, m),
            Tab::System => system::draw(ui, m),
        };
        ui.add_space(8.0);
    });
    action
}

/// The foot line: the launcher, the game's folder and how many builds it has; and the keys.
fn foot(ui: &mut Ui, screen: Rect, m: &Model) {
    let painter = ui.painter();
    let line = Rect::from_min_max(pos2(screen.left() + MARGIN, screen.bottom() - FOOT), pos2(screen.right() - 26.0, screen.bottom()));
    painter.line_segment([line.left_top(), line.right_top()], Stroke::new(1.0, RULE));
    let y = line.top() + 11.0;
    // the keys, from the right
    let mut x = line.right();
    for (keys, what) in [(&["Esc"][..], "SALIR"), (&["Enter"][..], "JUGAR"), (&["3", "2", "1"][..], "EDICIÓN"), (&[][..], "VERSIÓN")] {
        let word = look::tracked(painter, what, 11.5, DIM, 1.8);
        x -= word.size().x;
        painter.galley(pos2(x, y), word, DIM);
        x -= 7.0;
        for key in keys {
            x -= look::key_cap(painter, pos2(x, y - 4.0), key) + 3.0;
        }
        if keys.is_empty() {
            x -= look::arrows_cap(painter, pos2(x, y - 4.0)) + 3.0;
        }
        x -= 15.0;
    }
    let count = match m.found.builds.len() {
        0 if m.ready => "NINGUNA VERSIÓN DEL JUEGO".to_string(),
        0 => "BUSCANDO VERSIONES…".to_string(),
        n => format!("{}  ({})", words::plural(m.found.versions.len(), "VERSIÓN", "VERSIONES"), words::plural(n, "EJECUTABLE", "EJECUTABLES")),
    };
    let text = |folder: &str| format!("LAUNCHER {}   ·   {folder}   ·   {count}", crate::VERSION);
    let folder = m.root.display().to_string();
    // (the folder is cut in its middle until the line fits before the keys)
    let room = x - line.left();
    let mut most = folder.chars().count();
    let mut left = look::tracked(painter, &text(&folder), 11.5, DIM, 1.2);
    while left.size().x > room && most > 12 {
        most -= 4;
        left = look::tracked(painter, &text(&words::shortened(&folder, most)), 11.5, DIM, 1.2);
    }
    painter.galley(pos2(line.left(), y), left, DIM);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::tests::of_nowhere, options::Options};

    const NAMES: [&str; 6] = ["SeleneV36_debug.exe", "LunaV35_demo.exe", "LunaV35_multiplayer.exe", "LunaV35_debug.exe", "LunaV34_demo.exe", "LunaV21.exe"];

    #[test]
    fn the_arrows_choose_the_version_and_the_numbers_the_edition() {
        let mut m = of_nowhere(&NAMES, Options::default());
        let chosen = |m: &Model| m.chosen().map(|b| b.name());
        assert_eq!(chosen(&m).as_deref(), Some("V35 · DEMO"));
        assert_eq!(Keys { rows: 1, ..Keys::default() }.apply(&mut m), None);
        assert_eq!(chosen(&m).as_deref(), Some("V34 · DEMO"));
        assert_eq!(Keys { rows: -1, edition: Some(Edition::Multiplayer), ..Keys::default() }.apply(&mut m), None);
        assert_eq!(chosen(&m).as_deref(), Some("V35 · MULTIJUGADOR"));
        assert_eq!(Keys { edition: Some(Edition::Debug), ..Keys::default() }.apply(&mut m), None);
        assert_eq!(chosen(&m).as_deref(), Some("V35 · DEBUG"));
        // (an edition the chosen version lacks is not chosen)
        Keys { rows: -PAGE, ..Keys::default() }.apply(&mut m);
        assert_eq!(chosen(&m).as_deref(), Some("V36 · DEBUG"));
        Keys { edition: Some(Edition::Demo), ..Keys::default() }.apply(&mut m);
        assert_eq!((chosen(&m).as_deref(), m.options.edition), (Some("V36 · DEBUG"), Edition::Debug));
        // the ends of the list
        Keys { rows: i32::MAX / 2, ..Keys::default() }.apply(&mut m);
        assert_eq!(chosen(&m).as_deref(), Some("V21 · ANTIGUA"));
        Keys { rows: -i32::MAX / 2, ..Keys::default() }.apply(&mut m);
        assert_eq!(chosen(&m).as_deref(), Some("V36 · DEBUG"));
    }

    #[test]
    fn enter_plays_escape_leaves_and_the_side_arrows_change_tab() {
        let mut m = of_nowhere(&NAMES, Options::default());
        assert_eq!(Keys { enter: true, ..Keys::default() }.apply(&mut m), Some(Action::Play));
        assert_eq!(Keys { escape: true, ..Keys::default() }.apply(&mut m), Some(Action::Quit));
        assert_eq!(Keys { escape: true, enter: true, ..Keys::default() }.apply(&mut m), Some(Action::Quit));
        assert_eq!(Keys { refresh: true, ..Keys::default() }.apply(&mut m), Some(Action::Rescan));
        assert_eq!(Keys::default().apply(&mut m), None);
        assert_eq!(m.tab, Tab::Versions);
        Keys { tabs: 1, ..Keys::default() }.apply(&mut m);
        assert_eq!(m.tab, Tab::Options);
        Keys { tabs: -1, ..Keys::default() }.apply(&mut m);
        Keys { tabs: -1, ..Keys::default() }.apply(&mut m);
        assert_eq!(m.tab, Tab::Versions);
        for _ in 0..9 {
            Keys { tabs: 1, ..Keys::default() }.apply(&mut m);
        }
        assert_eq!(m.tab, Tab::System);
    }

    #[test]
    fn while_the_launcher_asks_enter_moves_nothing_and_escape_says_no() {
        let mut m = of_nowhere(&NAMES, Options::default());
        m.act(Action::AskArchive);
        assert!(m.asking.is_some());
        assert_eq!(Keys { enter: true, ..Keys::default() }.apply(&mut m), None);
        assert_eq!(Keys { escape: true, ..Keys::default() }.apply(&mut m), Some(Action::Cancel));
        m.act(Action::Cancel);
        assert_eq!(Keys { escape: true, ..Keys::default() }.apply(&mut m), Some(Action::Quit));
    }

    #[test]
    fn the_tabs_are_five_and_the_first_is_the_versions() {
        assert_eq!(Tab::ALL.map(Tab::name), ["VERSIONES", "OPCIONES", "MULTIJUGADOR", "HERRAMIENTAS", "SISTEMA"]);
        assert_eq!(of_nowhere(&[], Options::default()).tab, Tab::Versions);
    }
}
