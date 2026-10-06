//! What the launcher knows of the machine and of the game's last start, and all of it put into a
//! text to hand to whoever is asked for help: the graphics cards, the screen, the system, how long
//! each stage of the game's start took, how the last game went.
use crate::{builds::GAME, model::Model, options, tools, words};

/// A graphics card, as the graphics API tells of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gpu {
    pub name: String,
    /// "DirectX 12", "Vulkan".
    pub api: String,
    /// "dedicada", "integrada", "por software"…
    pub kind: String,
    pub driver: String,
    /// It is the one the launcher draws with.
    pub used: bool,
}

impl Gpu {
    pub fn says(&self) -> String {
        let driver = if self.driver.is_empty() { String::new() } else { format!(", {}", self.driver) };
        format!("{} ({}{driver})", self.name, self.kind)
    }
}

/// The screen the launcher is on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Screen {
    /// In pixels.
    pub width: u32,
    pub height: u32,
    /// How much the system enlarges what is drawn (1: nothing).
    pub scale: f32,
    pub hertz: Option<f32>,
}

impl Screen {
    pub fn says(&self) -> String {
        let hertz = self.hertz.map_or(String::new(), |hz| format!(" a {hz:.0} Hz"));
        format!("{} × {}{hertz}, escala {:.0} %", self.width, self.height, self.scale * 100.0)
    }
}

/// What the window's side found out as it opened.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// The graphics cards there are; the one in use says so.
    pub gpus: Vec<Gpu>,
    pub screen: Option<Screen>,
    /// How long the launcher took from its start to its first frame (milliseconds).
    pub start_ms: Option<u32>,
}

/// The system, from what `cmd /C ver` says ("Microsoft Windows [Versión 10.0.26200.6584]"):
/// "Windows 11 (10.0.26200.6584)". The numbers are what is read: the words come in the system's
/// own language and code page.
pub fn windows(ver: &str) -> Option<String> {
    let inside = ver.split_once('[')?.1.split_once(']')?.0;
    let number = inside.rsplit(' ').next()?.trim();
    let mut parts = number.split('.').map(str::parse::<u32>);
    let (major, _minor, build) = (parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?);
    let name = match (major, build) {
        (10, 22_000..) => "Windows 11".to_string(),
        (major, _) => format!("Windows {major}"),
    };
    Some(format!("{name} ({number})"))
}

/// How long each stage of the game's last start took (`out/arranque.json`: the stages by name,
/// in seconds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Boot {
    /// The stages, the longest first.
    pub stages: Vec<(String, f32)>,
    pub total: f32,
}

impl Boot {
    pub fn read(text: &str) -> Option<Boot> {
        let map: std::collections::BTreeMap<String, f32> = serde_json::from_str(text).ok()?;
        let mut stages: Vec<(String, f32)> = map.into_iter().filter(|(_, seconds)| seconds.is_finite() && *seconds >= 0.0).collect();
        stages.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        (!stages.is_empty()).then(|| Boot { total: stages.iter().map(|(_, s)| s).sum(), stages })
    }

    /// "8,0 s (defs 4,2 · mundo 2,1 · equipo 0,8 · …)".
    pub fn says(&self) -> String {
        let stages: Vec<String> = self.stages.iter().map(|(name, seconds)| format!("{name} {}", words::decimal(f64::from(*seconds), 1))).collect();
        format!("{} s ({})", words::decimal(f64::from(self.total), 1), stages.join(" · "))
    }
}

/// Everything the launcher knows, as a text to paste where help is asked for.
pub fn diagnostic(m: &Model) -> String {
    let now = words::now();
    let (found, clock) = (&m.found, m.found.clock());
    let mut lines = vec![format!("{GAME} — diagnóstico del launcher {} · {}", crate::VERSION, clock.moment(now, now))];
    let mut say = |name: &str, what: String| lines.push(format!("{name}: {what}"));
    say("Sistema", found.known.windows.clone().unwrap_or_else(|| "desconocido".to_string()));
    for gpu in &m.facts.gpus {
        say(if gpu.used { "Gráfica (la del launcher)" } else { "Gráfica" }, format!("{} · {}", gpu.says(), gpu.api));
    }
    if m.facts.gpus.is_empty() {
        say("Gráfica", "desconocida".to_string());
    }
    say("Pantalla", m.facts.screen.map_or("desconocida".to_string(), |s| s.says()));
    say("Carpeta del juego", m.root.display().to_string());
    let (here, away) = found.disk;
    say("Versiones", format!("{} a mano ({}, {}); {} archivadas en {} ({})", here.versions, words::plural(here.files, "ejecutable", "ejecutables"), words::size(here.bytes), away.versions, crate::builds::ARCHIVE, words::size(away.bytes)));
    say("Manual y datos", format!("LEEME.txt {}; assets {}", if found.manual { "está" } else { "FALTA" }, if found.assets { "están" } else { "FALTAN" }));
    match m.command() {
        Ok((build, args)) => say("Se inicia", format!("{} {}", build.path(), options::as_typed(&args)).trim_end().to_string()),
        Err(why) => say("Se inicia", format!("nada: {why}")),
    }
    match &m.history.last {
        Some(last) => say("Última partida", format!("V{} · {}, {}: {}", last.version, last.edition.name(), clock.moment(last.started, now), last.says())),
        None => say("Última partida", "ninguna desde este launcher".to_string()),
    }
    let total = m.history.total();
    say("Partidas", total.says(clock, now).unwrap_or_else(|| "ninguna".to_string()));
    say("Último arranque del juego", found.boot.as_ref().map_or("sin datos (out/arranque.json)".to_string(), Boot::says));
    let (server, up) = (&m.server, m.server_up);
    say("Servidor local", if server.ready { format!("{} en el puerto {}", if up { "en marcha" } else { "parado" }, server.port) } else { "no está en la carpeta".to_string() });
    say("Herramientas de naves (MCP)", m.found.mcp.says().0);
    if let Some(ms) = m.facts.start_ms {
        say("Arranque del launcher", format!("{ms} ms hasta el primer fotograma; carpeta mirada en {} ms", found.took_ms));
    }
    for log in found.logs.iter().filter(|log| !log.first.is_empty() && log.path.starts_with(tools::OUT)).take(3) {
        say(&log.path, format!("{} · {}", clock.moment(log.modified, now), log.first));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_is_named_by_its_numbers() {
        assert_eq!(windows("\r\nMicrosoft Windows [Versi\u{fffd}n 10.0.26200.6584]\r\n").as_deref(), Some("Windows 11 (10.0.26200.6584)"));
        assert_eq!(windows("Microsoft Windows [Version 10.0.19045.5011]").as_deref(), Some("Windows 10 (10.0.19045.5011)"));
        assert_eq!(windows("Microsoft Windows [Version 6.1.7601]").as_deref(), Some("Windows 6 (6.1.7601)"));
        assert_eq!(windows("otra cosa"), None);
        assert_eq!(windows("[sin números]"), None);
    }

    #[test]
    fn the_stages_of_the_game_s_start_are_told_the_longest_first() {
        let boot = Boot::read("{\n  \"defs\": 4.204,\n  \"equipo\": 0.76,\n  \"modelos\": 0.643,\n  \"mundo\": 2.127,\n  \"naves\": 0.044,\n  \"render\": 0.206,\n  \"sonido\": 0.056\n}").unwrap();
        assert_eq!(boot.stages.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["defs", "mundo", "equipo", "modelos", "render", "sonido", "naves"]);
        assert!((boot.total - 8.04).abs() < 1e-3);
        assert_eq!(boot.says(), "8,0 s (defs 4,2 · mundo 2,1 · equipo 0,8 · modelos 0,6 · render 0,2 · sonido 0,1 · naves 0,0)");
        assert_eq!(Boot::read("{}"), None);
        assert_eq!(Boot::read("no es json"), None);
        assert_eq!(Boot::read("{\"defs\": \"mucho\"}"), None);
    }

    #[test]
    fn a_card_and_a_screen_are_said_in_a_line() {
        let gpu = Gpu { name: "NVIDIA GeForce RTX 4070".into(), api: "DirectX 12".into(), kind: "dedicada".into(), driver: "32.0.15.6094".into(), used: true };
        assert_eq!(gpu.says(), "NVIDIA GeForce RTX 4070 (dedicada, 32.0.15.6094)");
        assert_eq!(Gpu { driver: String::new(), ..gpu }.says(), "NVIDIA GeForce RTX 4070 (dedicada)");
        assert_eq!(Screen { width: 2560, height: 1440, scale: 1.25, hertz: Some(143.9) }.says(), "2560 × 1440 a 144 Hz, escala 125 %");
        assert_eq!(Screen { width: 1920, height: 1080, scale: 1.0, hertz: None }.says(), "1920 × 1080, escala 100 %");
    }
}
