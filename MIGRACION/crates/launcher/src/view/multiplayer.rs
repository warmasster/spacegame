//! Playing with others: the server to join and the player's name, as the multiplayer edition is
//! told them, with the servers joined before; and the server of this machine — whether it is up,
//! starting it (never a second one), and the address to give to friends.
use super::parts::{self, note, warning};
use crate::{
    builds::{self, Edition},
    look::{self, AMBER, BODY, DIM, GOOD, TEXT},
    model::{Action, Model, Place, Text},
    options,
    server::{self, LOCALHOST},
};
use egui::{FontId, Sense, Ui, text::LayoutJob, vec2};

pub fn draw(ui: &mut Ui, m: &mut Model) -> Option<Action> {
    let mut action = None;
    m.look_for_server();
    let chosen = m.chosen().cloned();
    let joins = chosen.as_ref().is_some_and(|b| options::joins(&m.options, b));
    // ---- the server to join, and as whom ----
    look::section(ui, "Conexión");
    let names = ["Servidor (dirección:puerto)", "Tu nombre"];
    let wide = parts::widest(ui, &names);
    ui.horizontal(|ui| {
        parts::typed(ui, wide, names[0], &mut m.options.server, options::ADDRESS_MOST, "vacío: jugar solo", 92.0);
        if look::button(ui, "PEGAR", 78.0).clicked() {
            action = Some(Action::Paste);
        }
    });
    ui.horizontal(|ui| parts::typed(ui, wide, names[1], &mut m.options.name, options::NAME_MOST, options::NAME, 92.0));
    if let Some(b) = chosen.as_ref().filter(|_| !joins) {
        let why = if b.version < builds::MULTIPLAYER_SINCE { format!("la V{} es anterior a ella (la hay desde la V{})", b.version, builds::MULTIPLAYER_SINCE) } else { format!("ahora está elegida la V{} · {}", b.version, b.edition.name()) };
        parts::words(ui, &format!("Esto solo se le dice a la edición MULTIJUGADOR (tecla 3): {why}."), AMBER);
    }
    match options::address(&m.options.server) {
        Err(why) => warning(ui, &why),
        Ok(None) => note(ui, "Sin dirección no hay servidor al que unirse: se juega solo."),
        Ok(Some(server)) if joins => note(ui, &format!("Al jugar te unes a {server} como {}. El nombre es el que ven los demás (hasta {} letras).", options::name(&m.options.name), options::NAME_MOST)),
        Ok(Some(_)) => {}
    }
    if !m.options.servers.is_empty() {
        ui.add_space(4.0);
        let mut picked = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            ui.label(egui::RichText::new("Recientes").size(13.5).color(DIM));
            for server in &m.options.servers {
                if look::choice(ui, server, options::address(&m.options.server).ok().flatten().as_deref() == Some(server), 0.0).clicked() {
                    picked = Some(server.clone());
                }
            }
        });
        if let Some(server) = picked {
            m.options.server = server;
        }
    }
    // ---- the server of this machine ----
    look::section(ui, "Servidor en este PC");
    let port = m.server.port;
    if m.server.ready {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 22.0), Sense::hover());
            look::lamp(ui.painter(), r.center(), m.server_up.then_some(GOOD));
            let (words, ink) = if m.server_up { (format!("EN MARCHA en el puerto {port}"), GOOD) } else { ("PARADO".to_string(), DIM) };
            ui.label(egui::RichText::new(words).size(15.0).color(ink));
            if !m.server.name.is_empty() {
                ui.label(egui::RichText::new(format!("·  «{}»", m.server.name)).size(14.0).color(DIM));
            }
        });
        note(
            ui,
            if m.server_up { "Ya hay uno en marcha: no se inicia otro. Para pararlo, cierra su ventana (o escribe «salir» en ella)." } else { "Un servidor para que los demás se unan a tu partida. Se abre en su propia ventana: al cerrarla se para." },
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
            ui.add_enabled_ui(!m.server_up, |ui| {
                if look::button(ui, "INICIAR SERVIDOR", 0.0).clicked() {
                    action = Some(Action::StartServer);
                }
            });
            let local = format!("{LOCALHOST}:{port}");
            ui.add_enabled_ui(m.options.server.trim() != local, |ui| {
                if look::button(ui, "JUGAR EN ÉL", 0.0).on_hover_text(format!("Pone {local} como servidor al que unirse")).clicked() {
                    m.options.server = local.clone();
                }
            });
            if look::button(ui, "CARPETA", 0.0).clicked() {
                action = Some(Action::Open(Place::ServerFolder));
            }
            if look::button(ui, "REGISTRO", 0.0).clicked() {
                action = Some(Action::Open(Place::ServerLog));
            }
        });
        // ---- where the others find it ----
        look::section(ui, "Para tus amigos");
        let (others, here) = m.reach();
        match others.first() {
            Some(address) => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    ui.label(egui::RichText::new(address).font(FontId::monospace(19.0)).color(TEXT));
                    if look::button(ui, "COPIAR", 0.0).clicked() {
                        action = Some(Action::Copy(Text::Address));
                    }
                });
                let other = others.get(1).map_or(String::new(), |other| format!(" (si no llegan, prueba {other})"));
                note(ui, &format!("Es la dirección de este PC en tu red: quien juegue en ella la escribe como servidor{other}. En este mismo PC vale {here}. Por internet hace falta tu IP pública y abrir en el router el puerto {port} (UDP)."));
            }
            None => note(ui, &format!("No se sabe la dirección de este PC en tu red (¿sin red?). Los demás se conectan a la IP de este PC con el puerto {port} (IP:{port}); en este mismo PC vale {here}.")),
        }
        if port != server::PORT {
            note(ui, &format!("(El puerto {port} es el que dice {}/{}.)", server::FOLDER, server::CONFIG));
        }
    } else {
        let mut job = LayoutJob::default();
        job.append("No está el servidor en la carpeta del juego. Debería estar en ", 0.0, parts::format(13.5, DIM));
        job.append(&m.server_place(), 0.0, parts::format(13.5, BODY));
        ui.add(egui::Label::new(job).wrap());
        if m.server.has_folder {
            ui.add_space(6.0);
            if look::button(ui, "CARPETA", 0.0).clicked() {
                action = Some(Action::Open(Place::ServerFolder));
            }
        }
    }
    // ---- what it all comes to ----
    if chosen.as_ref().is_some_and(|b| b.edition == Edition::Multiplayer) {
        look::section(ui, "Así se inicia");
        match m.command() {
            Ok((build, args)) => parts::command_line(ui, &build.path(), &args),
            Err(_) => warning(ui, "No se inicia mientras la dirección del servidor no valga."),
        }
    }
    action
}
