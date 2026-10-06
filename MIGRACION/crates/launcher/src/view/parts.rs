//! The small things the tabs are made of: a note, a warning, a row of choices, a command line, a
//! line to type in, a fact with its name.
use crate::{
    look::{self, BODY, DIM, TEXT, WARNING},
    options,
};
use egui::{Color32, FontId, Ui, text::LayoutJob, vec2};

pub fn format(size: f32, color: Color32) -> egui::TextFormat {
    egui::TextFormat { font_id: FontId::proportional(size), color, ..Default::default() }
}

/// A few words in that ink, small, folded to the width there is.
pub fn words(ui: &mut Ui, text: &str, color: Color32) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(13.5).color(color)).wrap());
}

/// A few words of explanation, small and dim.
pub fn note(ui: &mut Ui, text: &str) {
    words(ui, text, DIM);
}

/// A few words on what is wrong, small and in the colour of a warning.
pub fn warning(ui: &mut Ui, text: &str) {
    words(ui, text, WARNING);
}

/// A row of things to choose one from, folding to the width there is. Whether another was chosen.
pub fn choices<T: Copy + PartialEq>(ui: &mut Ui, all: &[T], chosen: &mut T, name: impl Fn(T) -> String) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for one in all {
            if look::choice(ui, &name(*one), *chosen == *one, 0.0).clicked() && *chosen != *one {
                *chosen = *one;
                changed = true;
            }
        }
    });
    changed
}

/// A command line as it would be typed, in the fixed-width face.
pub fn command_line(ui: &mut Ui, file: &str, args: &[String]) {
    let mut job = LayoutJob::default();
    job.append(file, 0.0, egui::TextFormat { font_id: FontId::monospace(13.0), color: BODY, ..Default::default() });
    job.append(&format!(" {}", options::as_typed(args)), 0.0, egui::TextFormat { font_id: FontId::monospace(13.0), color: DIM, ..Default::default() });
    ui.add(egui::Label::new(job).wrap());
}

/// What a program printed, in the fixed-width face.
pub fn printed(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: FontId::monospace(12.5), color: BODY, line_height: Some(17.0), ..Default::default() })).wrap());
}

/// The width the widest of these names takes, as the names of a tab's rows are lettered.
pub fn widest(ui: &Ui, names: &[&str]) -> f32 {
    names.iter().map(|name| ui.painter().layout_no_wrap((*name).to_string(), FontId::proportional(14.5), TEXT).size().x).fold(0.0, f32::max)
}

/// A line of text to type in, in a row: its name, `wide` wide, and its field from there to
/// `room` short of the edge (what follows it in the row goes there).
pub fn typed(ui: &mut Ui, wide: f32, name: &str, text: &mut String, most: usize, hint: &str, room: f32) {
    ui.spacing_mut().item_spacing.x = 14.0;
    ui.allocate_ui_with_layout(vec2(wide, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(wide);
        ui.label(egui::RichText::new(name).size(14.5));
    });
    look::field(ui, text, (ui.available_width() - room).max(80.0), most, hint);
}

/// A fact in a row: its name, dim and `wide` wide, and what it is, folded to the width left.
pub fn fact(ui: &mut Ui, wide: f32, name: &str, what: &str, ink: Color32) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        ui.allocate_ui_with_layout(vec2(wide, 20.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.set_min_width(wide);
            ui.label(egui::RichText::new(name).size(13.5).color(DIM));
        });
        ui.add(egui::Label::new(egui::RichText::new(what).size(14.5).color(ink)).wrap());
    });
}
