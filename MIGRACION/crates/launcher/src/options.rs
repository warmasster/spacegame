//! What the player chooses: kept in `launcher.json` next to the launcher, and turned into the
//! game's command line.
use crate::{
    builds::{Build, Edition, SCRIPTS},
    server::{LOCALHOST, PORT},
};
use serde::{Deserialize, Serialize};

/// The file the options are kept in, next to the launcher.
pub const FILE: &str = "launcher.json";

/// The first version that understands each flag. The game stops at a flag it does not know, so a
/// build is only told what it understands; one older than `FLAGS_SINCE` is told nothing.
pub const FLAGS_SINCE: u32 = 22;
/// `--guion`.
pub const SCRIPTS_SINCE: u32 = 24;
/// `--mudo` (older builds have no sound to silence).
pub const MUTE_SINCE: u32 = 31;
/// `--pantalla`, `--ventana`, `--sin-menu`.
pub const SCREEN_SINCE: u32 = 34;
/// `--prueba-arranque`; and from this one on a script runs with no window to be seen.
pub const BOOT_SINCE: u32 = 35;

/// The most characters of the extra arguments a developer may type for the game.
pub const EXTRA_MOST: usize = 200;
/// The most servers joined that are remembered.
pub const SERVERS_MOST: usize = 5;
/// How many of the newest versions may be kept at hand when the older ones are put away.
pub const KEEP: [usize; 3] = [3, 5, 10];

/// The most characters a player's name has.
pub const NAME_MOST: usize = 24;
/// The name of a player who gives none.
pub const NAME: &str = "Jugador";
/// The most characters a server's address is let to have as it is typed.
pub const ADDRESS_MOST: usize = 80;

/// The sizes of the game's window on offer.
pub const WINDOW_SIZES: [(u32, u32); 4] = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];

/// The game's graphics presets, or what it picks by itself for the card it finds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    #[default]
    Auto,
    Horrible,
    MuyBaja,
    Baja,
    Media,
    Alta,
    MuyAlta,
    Ultra,
    Esplendidos,
}

impl Quality {
    pub const ALL: [Quality; 9] = [Quality::Auto, Quality::Horrible, Quality::MuyBaja, Quality::Baja, Quality::Media, Quality::Alta, Quality::MuyAlta, Quality::Ultra, Quality::Esplendidos];

    /// The preset's id for `--profile`; none: nothing is passed.
    pub fn id(self) -> Option<&'static str> {
        Some(match self {
            Quality::Auto => return None,
            Quality::Horrible => "horrible",
            Quality::MuyBaja => "muybaja",
            Quality::Baja => "baja",
            Quality::Media => "media",
            Quality::Alta => "alta",
            Quality::MuyAlta => "muyalta",
            Quality::Ultra => "ultra",
            Quality::Esplendidos => "esplendidos",
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Quality::Auto => "AUTOMÁTICA",
            Quality::Horrible => "HORRIBLE",
            Quality::MuyBaja => "MUY BAJA",
            Quality::Baja => "BAJA",
            Quality::Media => "MEDIA",
            Quality::Alta => "ALTA",
            Quality::MuyAlta => "MUY ALTA",
            Quality::Ultra => "ULTRA",
            Quality::Esplendidos => "ESPLÉNDIDOS",
        }
    }
}

/// The graphics API the game is made to use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Auto,
    Dx12,
    Vulkan,
}

impl Backend {
    pub const ALL: [Backend; 3] = [Backend::Auto, Backend::Dx12, Backend::Vulkan];

    /// Its id for `--backend`; none: nothing is passed.
    pub fn id(self) -> Option<&'static str> {
        match self {
            Backend::Auto => None,
            Backend::Dx12 => Some("dx12"),
            Backend::Vulkan => Some("vulkan"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Backend::Auto => "AUTOMÁTICA",
            Backend::Dx12 => "DIRECTX 12",
            Backend::Vulkan => "VULKAN",
        }
    }
}

/// The graphics at a stroke: the game's quality and the size of its window, for a machine that
/// is slow, middling, fast or the fastest. The fine options stay there to be set one by one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// The game picks the quality by the card it finds.
    Auto,
    Bajo,
    Medio,
    Alto,
    Ultra,
}

impl Preset {
    pub const ALL: [Preset; 5] = [Preset::Auto, Preset::Bajo, Preset::Medio, Preset::Alto, Preset::Ultra];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Auto => "AUTOMÁTICO",
            Preset::Bajo => "BAJO",
            Preset::Medio => "MEDIO",
            Preset::Alto => "ALTO",
            Preset::Ultra => "ULTRA",
        }
    }

    /// What it is for.
    pub fn says(self) -> &'static str {
        match self {
            Preset::Auto => "El juego elige la calidad según la tarjeta gráfica que encuentre.",
            Preset::Bajo => "Para gráficas integradas o portátiles: calidad baja; en ventana, 1280 × 720.",
            Preset::Medio => "Para una gráfica modesta: calidad media; en ventana, 1600 × 900.",
            Preset::Alto => "Para una gráfica de juegos: calidad alta; en ventana, 1920 × 1080.",
            Preset::Ultra => "Para las más rápidas: calidad ultra; en ventana, 2560 × 1440.",
        }
    }

    pub fn quality(self) -> Quality {
        match self {
            Preset::Auto => Quality::Auto,
            Preset::Bajo => Quality::Baja,
            Preset::Medio => Quality::Media,
            Preset::Alto => Quality::Alta,
            Preset::Ultra => Quality::Ultra,
        }
    }

    /// The window it asks for; none: the one there is stays.
    fn window(self) -> Option<(u32, u32)> {
        match self {
            Preset::Auto => None,
            Preset::Bajo => Some(WINDOW_SIZES[0]),
            Preset::Medio => Some(WINDOW_SIZES[1]),
            Preset::Alto => Some(WINDOW_SIZES[2]),
            Preset::Ultra => Some(WINDOW_SIZES[3]),
        }
    }

    /// The window it gives on a screen of that size (none: not known): the one it asks for, or the
    /// largest on offer that fits the screen if that one does not.
    fn window_on(self, screen: Option<(u32, u32)>) -> Option<(u32, u32)> {
        let asked = self.window()?;
        let fits = |size: &(u32, u32)| screen.is_none_or(|(w, h)| size.0 <= w && size.1 <= h);
        Some(if fits(&asked) { asked } else { WINDOW_SIZES.iter().rev().find(|size| size.0 <= asked.0 && fits(size)).copied().unwrap_or(WINDOW_SIZES[0]) })
    }

    /// Set the options it stands for.
    pub fn apply(self, o: &mut Options, screen: Option<(u32, u32)>) {
        o.quality = self.quality();
        if let Some(window) = self.window_on(screen) {
            o.window = window;
        }
    }

    /// The preset these options are, if they are one: its quality and, when the game is played in
    /// a window, its window.
    pub fn of(o: &Options, screen: Option<(u32, u32)>) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.quality() == o.quality && (o.fullscreen || p.window_on(screen).is_none_or(|window| window == o.window)))
    }
}

/// What the launcher does once the game it started is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AfterPlay {
    /// Its window goes while the game is played and comes back when the game ends, telling how
    /// it went.
    #[default]
    Hide,
    /// It stays as it is.
    Stay,
    /// It closes: it does not see the game end.
    Close,
}

impl AfterPlay {
    pub const ALL: [AfterPlay; 3] = [AfterPlay::Hide, AfterPlay::Stay, AfterPlay::Close];

    pub fn name(self) -> &'static str {
        match self {
            AfterPlay::Hide => "OCULTARSE Y VOLVER",
            AfterPlay::Stay => "SEGUIR ABIERTO",
            AfterPlay::Close => "CERRARSE",
        }
    }

    pub fn says(self) -> &'static str {
        match self {
            AfterPlay::Hide => "La ventana se quita mientras juegas (sin gastar nada) y vuelve al salir del juego, con lo que duró la partida y cómo acabó.",
            AfterPlay::Stay => "El launcher se queda abierto detrás del juego.",
            AfterPlay::Close => "El launcher se cierra del todo: cuenta la partida, pero no ve cuánto dura ni cómo acaba.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// The version to start; none: the newest there is.
    pub version: Option<u32>,
    /// Demo (to play), multiplayer or debug (to develop).
    pub edition: Edition,
    pub quality: Quality,
    pub fullscreen: bool,
    /// The game's window when it is not full screen.
    pub window: (u32, u32),
    pub vsync: bool,
    pub backend: Backend,
    pub sound: bool,
    /// Straight into the game, without its start menu.
    pub skip_menu: bool,
    /// What the launcher does once the game is up.
    pub after_play: AfterPlay,
    /// The server the multiplayer edition joins, `host:port` as it was typed; empty: none, the
    /// game is played alone.
    pub server: String,
    /// The player's name there, as it was typed.
    pub name: String,
    /// The servers joined before, the last one first.
    pub servers: Vec<String>,
    /// How many of the newest versions stay at hand when the older ones are put away.
    pub keep_versions: usize,
    /// Arguments added to the game's command line as they are typed (a developer's: `--ships 100`).
    pub extra: String,
    /// The newest version there was the last time the launcher looked (0: it never did): the
    /// ones above it are new.
    pub seen: u32,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            version: None,
            edition: Edition::Demo,
            quality: Quality::Auto,
            fullscreen: true,
            window: (1600, 900),
            vsync: true,
            backend: Backend::Auto,
            sound: true,
            skip_menu: false,
            after_play: AfterPlay::Hide,
            server: format!("{LOCALHOST}:{PORT}"),
            // (who the system says is at this machine)
            name: name(&std::env::var("USERNAME").unwrap_or_default()),
            servers: Vec::new(),
            keep_versions: KEEP[1],
            extra: String::new(),
            seen: 0,
        }
    }
}

impl Options {
    /// The options a file's value holds: what it does not say is the default, and so is all of it
    /// if it cannot be read. A file from before `after_play` says whether the launcher closes
    /// (`close_on_play`): it is kept to that.
    pub fn of(value: &serde_json::Value) -> Options {
        let mut o: Options = serde_json::from_value(value.clone()).unwrap_or_default();
        if value.get("after_play").is_none()
            && let Some(close) = value.get("close_on_play").and_then(serde_json::Value::as_bool)
        {
            o.after_play = if close { AfterPlay::Close } else { AfterPlay::Stay };
        }
        if o.edition == Edition::Old {
            o.edition = Edition::Demo;
        }
        if !WINDOW_SIZES.contains(&o.window) {
            o.window = Options::default().window;
        }
        if !KEEP.contains(&o.keep_versions) {
            o.keep_versions = Options::default().keep_versions;
        }
        o.servers = o.servers.iter().filter_map(|s| address(s).ok().flatten()).fold(Vec::new(), |mut all, s| {
            if !all.contains(&s) && all.len() < SERVERS_MOST {
                all.push(s);
            }
            all
        });
        o
    }

    /// The options a file's text holds.
    #[cfg(test)]
    pub fn parse(text: &str) -> Options {
        serde_json::from_str::<serde_json::Value>(text).map(|value| Options::of(&value)).unwrap_or_default()
    }

    /// Remember a server that was joined: the first of the list, once.
    pub fn joined(&mut self, server: &str) {
        self.servers.retain(|s| s != server);
        self.servers.insert(0, server.to_string());
        self.servers.truncate(SERVERS_MOST);
    }
}

fn push(args: &mut Vec<String>, parts: &[&str]) {
    args.extend(parts.iter().map(|p| (*p).to_string()));
}

/// The picture's quality and the API: what a game and a camera script are both told.
fn picture(o: &Options, args: &mut Vec<String>) {
    if let Some(id) = o.quality.id() {
        push(args, &["--profile", id]);
    }
    if let Some(id) = o.backend.id() {
        push(args, &["--backend", id]);
    }
}

/// A player's name as the game is given it: without control characters, trimmed, `NAME_MOST`
/// characters at most; `NAME` if nothing is left of it.
pub fn name(typed: &str) -> String {
    let clean: String = typed.chars().filter(|c| !c.is_control()).collect();
    let cut: String = clean.trim().chars().take(NAME_MOST).collect();
    match cut.trim_end() {
        "" => NAME.to_string(),
        name => name.to_string(),
    }
}

/// The most characters of a wrong address that are told back.
const SHOWN_MOST: usize = 32;

/// Whether this names a machine: a name or an IPv4 (letters, digits, `.`, `-`, `_`), or an IPv6
/// between brackets.
fn is_host(host: &str) -> bool {
    if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return v6.parse::<std::net::Ipv6Addr>().is_ok();
    }
    !host.is_empty() && !host.starts_with('-') && host.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// A server's address as it was typed, checked: none if nothing was typed (the game is played
/// alone); the address as the game is given it if it has the shape `host:port`, the port from 1
/// to 65535; else what is wrong with it, as it is told to the player.
pub fn address(typed: &str) -> Result<Option<String>, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Ok(None);
    }
    let wrong = |why: &str| {
        // (what is told stays short enough to be read in a notice, however much was typed)
        let shown: String = if typed.chars().count() > SHOWN_MOST { typed.chars().take(SHOWN_MOST - 1).chain(['…']).collect() } else { typed.to_string() };
        Err(format!("La dirección del servidor «{shown}» no vale: {why}. Se escribe dirección:puerto, por ejemplo {LOCALHOST}:{PORT}."))
    };
    let Some((host, port)) = typed.rsplit_once(':') else { return wrong("le falta el puerto") };
    if host.is_empty() {
        return wrong("le falta la máquina a la que conectarse");
    }
    if !is_host(host) {
        return wrong("la máquina no es un nombre ni una IP");
    }
    if port.is_empty() {
        return wrong("le falta el puerto");
    }
    // (a number and nothing else: no sign before it)
    match port.parse::<u16>() {
        Ok(number) if number > 0 && port.bytes().all(|b| b.is_ascii_digit()) => Ok(Some(format!("{host}:{number}"))),
        _ => wrong("el puerto ha de ser un número de 1 a 65535"),
    }
}

/// What was typed as extra arguments, apart: at the blanks, but for what stands between quotes
/// (which is one argument, without them).
pub fn typed_arguments(typed: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut open: Option<String> = None;
    let mut quoted = false;
    for c in typed.chars().filter(|c| !c.is_control()) {
        match c {
            '"' => {
                quoted = !quoted;
                open.get_or_insert_default();
            }
            c if c.is_whitespace() && !quoted => args.extend(open.take()),
            c => open.get_or_insert_default().push(c),
        }
    }
    args.extend(open);
    args
}

/// Whether this build is started to play with others: it is the multiplayer one, and that is the
/// edition asked for. (A multiplayer build started in place of another edition a version lacks is
/// told no server: it plays alone, which is the nearest to what was asked.)
pub fn joins(o: &Options, build: &Build) -> bool {
    o.edition == Edition::Multiplayer && build.edition == Edition::Multiplayer
}

/// The game's command line for these options: only what that build understands. A multiplayer
/// build started as such is told the server to join and the player's name; if the server's address
/// is wrong there is no command line, but what is wrong with it.
pub fn arguments(o: &Options, build: &Build) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    if build.version < FLAGS_SINCE {
        return Ok(args);
    }
    picture(o, &mut args);
    push(&mut args, &["--vsync", if o.vsync { "on" } else { "off" }]);
    if !o.sound && build.version >= MUTE_SINCE {
        push(&mut args, &["--mudo"]);
    }
    if build.version >= SCREEN_SINCE {
        if o.fullscreen {
            push(&mut args, &["--pantalla", "completa"]);
        } else {
            push(&mut args, &["--pantalla", "ventana", "--ventana", &format!("{}x{}", o.window.0, o.window.1)]);
        }
        if o.skip_menu {
            push(&mut args, &["--sin-menu"]);
        }
    }
    if joins(o, build)
        && let Some(server) = address(&o.server)?
    {
        push(&mut args, &["--servidor", &server, "--nombre", &name(&o.name)]);
    }
    args.extend(typed_arguments(&o.extra));
    Ok(args)
}

/// A command line as it would be typed: its arguments apart, the ones with a space in them
/// between quotes.
pub fn as_typed(args: &[String]) -> String {
    let quoted: Vec<String> = args.iter().map(|a| if a.is_empty() || a.contains(char::is_whitespace) { format!("\"{a}\"") } else { a.clone() }).collect();
    quoted.join(" ")
}

/// The command line that makes a build run a camera script (`tools/camara/<script>.jsonc`, its
/// pictures to `out/camara`); none if that build runs no scripts (it is not a debug one, or too old).
pub fn script_arguments(o: &Options, build: &Build, script: &str) -> Option<Vec<String>> {
    if build.edition != Edition::Debug || build.version < SCRIPTS_SINCE {
        return None;
    }
    let mut args = vec!["--guion".to_string(), format!("{SCRIPTS}/{script}.jsonc")];
    picture(o, &mut args);
    Some(args)
}

/// The command line that makes a build check its own start (it starts as to be played, with no
/// window to be seen, draws a few frames and ends, saying how it went); none if that build does
/// not know how (it is older than `BOOT_SINCE`).
pub fn boot_arguments(o: &Options, build: &Build) -> Option<Vec<String>> {
    if build.version < BOOT_SINCE {
        return None;
    }
    let mut args = vec!["--prueba-arranque".to_string()];
    picture(o, &mut args);
    Some(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(version: u32, edition: Edition) -> Build {
        Build { version, edition, file: String::new(), archived: false }
    }

    fn line(o: &Options, version: u32) -> String {
        arguments(o, &build(version, Edition::Demo)).unwrap().join(" ")
    }

    /// The command line of the multiplayer build of a version, asked for as such.
    fn net(o: &Options, version: u32) -> Result<String, String> {
        arguments(&Options { edition: Edition::Multiplayer, ..o.clone() }, &build(version, Edition::Multiplayer)).map(|a| a.join(" "))
    }

    fn with(server: &str, name: &str) -> Options {
        Options { server: server.to_string(), name: name.to_string(), ..Options::default() }
    }

    #[test]
    fn the_defaults_ask_for_full_screen_and_vsync_and_nothing_else() {
        let o = Options::default();
        assert_eq!(line(&o, 34), "--vsync on --pantalla completa");
        assert_eq!(line(&o, 33), "--vsync on");
        assert_eq!(line(&o, 22), "--vsync on");
    }

    #[test]
    fn every_option_has_its_flag() {
        let o = Options { quality: Quality::MuyAlta, fullscreen: false, window: (2560, 1440), vsync: false, backend: Backend::Vulkan, sound: false, skip_menu: true, ..Options::default() };
        assert_eq!(line(&o, 34), "--profile muyalta --backend vulkan --vsync off --mudo --pantalla ventana --ventana 2560x1440 --sin-menu");
        assert_eq!(line(&o, 40), line(&o, 34));
        let o = Options { backend: Backend::Dx12, quality: Quality::Esplendidos, ..o };
        assert_eq!(line(&o, 35), "--profile esplendidos --backend dx12 --vsync off --mudo --pantalla ventana --ventana 2560x1440 --sin-menu");
    }

    #[test]
    fn a_build_is_only_told_what_it_understands() {
        let o = Options { quality: Quality::Baja, fullscreen: false, vsync: false, backend: Backend::Dx12, sound: false, skip_menu: true, ..Options::default() };
        assert_eq!(line(&o, 33), "--profile baja --backend dx12 --vsync off --mudo");
        assert_eq!(line(&o, 31), "--profile baja --backend dx12 --vsync off --mudo");
        assert_eq!(line(&o, 30), "--profile baja --backend dx12 --vsync off");
        assert_eq!(line(&o, 22), "--profile baja --backend dx12 --vsync off");
        assert_eq!(line(&o, 21), "");
        assert!(arguments(&o, &build(9, Edition::Old)).unwrap().is_empty());
    }

    #[test]
    fn full_screen_passes_no_window_size() {
        let o = Options { fullscreen: true, window: (1280, 720), ..Options::default() };
        assert!(!arguments(&o, &build(34, Edition::Demo)).unwrap().iter().any(|a| a == "--ventana"));
    }

    #[test]
    fn the_presets_have_the_ids_of_the_game() {
        let ids: Vec<&str> = Quality::ALL.iter().filter_map(|q| q.id()).collect();
        assert_eq!(ids, ["horrible", "muybaja", "baja", "media", "alta", "muyalta", "ultra", "esplendidos"]);
        let names: Vec<&str> = Quality::ALL.iter().skip(1).map(|q| q.name()).collect();
        assert_eq!(names, ["HORRIBLE", "MUY BAJA", "BAJA", "MEDIA", "ALTA", "MUY ALTA", "ULTRA", "ESPLÉNDIDOS"]);
        assert_eq!(Quality::Auto.id(), None);
        assert_eq!(Backend::ALL.map(Backend::id), [None, Some("dx12"), Some("vulkan")]);
    }

    #[test]
    fn a_script_runs_in_a_debug_build_with_its_picture_options() {
        let o = Options { quality: Quality::Ultra, backend: Backend::Dx12, sound: false, skip_menu: true, fullscreen: false, ..Options::default() };
        assert_eq!(script_arguments(&o, &build(33, Edition::Debug), "carga").map(|a| a.join(" ")).as_deref(), Some("--guion tools/camara/carga.jsonc --profile ultra --backend dx12"));
        assert_eq!(script_arguments(&Options::default(), &build(24, Edition::Debug), "hud"), Some(vec!["--guion".to_string(), "tools/camara/hud.jsonc".to_string()]));
        assert_eq!(script_arguments(&o, &build(33, Edition::Demo), "carga"), None);
        assert_eq!(script_arguments(&o, &build(23, Edition::Debug), "carga"), None);
        assert_eq!(script_arguments(&o, &build(21, Edition::Old), "carga"), None);
    }

    #[test]
    fn options_survive_a_round_trip() {
        let o = Options {
            version: Some(31),
            edition: Edition::Debug,
            quality: Quality::MuyBaja,
            fullscreen: false,
            window: (1920, 1080),
            vsync: false,
            backend: Backend::Vulkan,
            sound: false,
            skip_menu: true,
            after_play: AfterPlay::Stay,
            servers: vec!["luna.example:47611".to_string(), "127.0.0.1:47600".to_string()],
            keep_versions: 10,
            extra: "--ships 100 --explode \"bomba grande@90\"".to_string(),
            seen: 36,
            server: "luna.example:47611".to_string(),
            name: "Ñandú del Mar".to_string(),
        };
        let text = serde_json::to_string_pretty(&o).unwrap();
        assert_eq!(Options::parse(&text), o);
        assert!(text.contains("\"quality\": \"muybaja\""), "{text}");
        assert!(text.contains("\"server\": \"luna.example:47611\"") && text.contains("\"name\": \"Ñandú del Mar\""), "{text}");
        let o = Options { edition: Edition::Multiplayer, ..o };
        let text = serde_json::to_string_pretty(&o).unwrap();
        assert_eq!(Options::parse(&text), o);
        assert!(text.contains("\"edition\": \"multiplayer\""), "{text}");
    }

    #[test]
    fn a_missing_or_broken_file_is_the_defaults() {
        assert_eq!(Options::parse(""), Options::default());
        assert_eq!(Options::parse("{ esto no es json"), Options::default());
        assert_eq!(Options::parse("[1, 2, 3]"), Options::default());
        assert_eq!(Options::parse("{\"quality\": \"imposible\"}"), Options::default());
    }

    #[test]
    fn what_a_file_does_not_say_is_the_default_and_nonsense_is_put_right() {
        let o = Options::parse("{\"vsync\": false, \"otra_cosa\": 3}");
        assert_eq!(o, Options { vsync: false, ..Options::default() });
        let o = Options::parse("{\"edition\": \"old\", \"window\": [3, 2]}");
        assert_eq!(o, Options::default());
        // (a file from before the multiplayer edition: its server and its name are the usual ones)
        let o = Options::parse("{\"edition\": \"debug\", \"sound\": false}");
        assert_eq!((o.server.as_str(), o.name.is_empty()), ("127.0.0.1:47600", false));
        assert_eq!(Options::parse("{\"edition\": \"multiplayer\", \"server\": \"\"}"), Options { edition: Edition::Multiplayer, server: String::new(), ..Options::default() });
    }

    #[test]
    fn a_file_from_before_tells_whether_the_launcher_closes() {
        assert_eq!(Options::default().after_play, AfterPlay::Hide);
        assert_eq!(Options::parse("{\"close_on_play\": true}").after_play, AfterPlay::Close);
        assert_eq!(Options::parse("{\"close_on_play\": false, \"vsync\": false}"), Options { after_play: AfterPlay::Stay, vsync: false, ..Options::default() });
        // (what the new key says goes before what the old one did)
        assert_eq!(Options::parse("{\"close_on_play\": true, \"after_play\": \"hide\"}").after_play, AfterPlay::Hide);
        assert_eq!(Options::parse("{\"after_play\": \"close\"}").after_play, AfterPlay::Close);
        let text = serde_json::to_string(&Options::default()).unwrap();
        assert!(text.contains("\"after_play\":\"hide\"") && !text.contains("close_on_play"), "{text}");
        assert_eq!(AfterPlay::ALL.map(AfterPlay::name), ["OCULTARSE Y VOLVER", "SEGUIR ABIERTO", "CERRARSE"]);
    }

    #[test]
    fn the_servers_remembered_are_few_right_and_each_once() {
        let mut o = Options::default();
        assert!(o.servers.is_empty());
        for server in ["a:1", "b:2", "a:1", "c:3", "d:4", "e:5", "f:6"] {
            o.joined(server);
        }
        assert_eq!(o.servers, ["f:6", "e:5", "d:4", "c:3", "a:1"]);
        o.joined("c:3");
        assert_eq!(o.servers, ["c:3", "f:6", "e:5", "d:4", "a:1"]);
        // a file's list is read with the wrong ones and the repeated ones left out
        let o = Options::parse("{\"servers\": [\"a:1\", \"sin puerto\", \" a:1 \", \"\", \"b:02\", \"c:3\", \"d:4\", \"e:5\", \"f:6\"], \"keep_versions\": 4}");
        assert_eq!(o.servers, ["a:1", "b:2", "c:3", "d:4", "e:5"]);
        assert_eq!(o.keep_versions, 5);
        assert_eq!(Options::parse("{\"keep_versions\": 10}").keep_versions, 10);
    }

    #[test]
    fn a_preset_is_a_quality_and_a_window() {
        let mut o = Options::default();
        assert_eq!(Preset::of(&o, None), Some(Preset::Auto));
        for (preset, quality, window) in [(Preset::Bajo, Quality::Baja, (1280, 720)), (Preset::Medio, Quality::Media, (1600, 900)), (Preset::Alto, Quality::Alta, (1920, 1080)), (Preset::Ultra, Quality::Ultra, (2560, 1440))] {
            preset.apply(&mut o, None);
            assert_eq!((o.quality, o.window), (quality, window));
            assert_eq!(Preset::of(&o, None), Some(preset));
            assert!(preset.says().contains(&format!("{} × {}", window.0, window.1)), "{}", preset.says());
        }
        assert_eq!(line(&Options { fullscreen: false, ..o.clone() }, 35), "--profile ultra --vsync on --pantalla ventana --ventana 2560x1440");
        // the automatic one leaves the window as it is, whatever it is
        Preset::Auto.apply(&mut o, None);
        assert_eq!((o.quality, o.window), (Quality::Auto, (2560, 1440)));
        assert_eq!(Preset::of(&Options { fullscreen: false, ..o.clone() }, None), Some(Preset::Auto));
        // a quality of the fine ones is no preset; nor, in a window, is another window
        assert_eq!(Preset::of(&Options { quality: Quality::MuyAlta, ..o.clone() }, None), None);
        let other = Options { quality: Quality::Alta, window: (1280, 720), ..o.clone() };
        assert_eq!(Preset::of(&other, None), Some(Preset::Alto));
        assert_eq!(Preset::of(&Options { fullscreen: false, ..other }, None), None);
        assert_eq!(Preset::ALL.map(Preset::name), ["AUTOMÁTICO", "BAJO", "MEDIO", "ALTO", "ULTRA"]);
    }

    #[test]
    fn a_preset_s_window_fits_the_screen() {
        let mut o = Options { fullscreen: false, ..Options::default() };
        Preset::Ultra.apply(&mut o, Some((1920, 1080)));
        assert_eq!((o.quality, o.window), (Quality::Ultra, (1920, 1080)));
        assert_eq!(Preset::of(&o, Some((1920, 1080))), Some(Preset::Ultra));
        Preset::Alto.apply(&mut o, Some((1366, 768)));
        assert_eq!(o.window, (1280, 720));
        Preset::Bajo.apply(&mut o, Some((1024, 600)));
        assert_eq!(o.window, (1280, 720));
        Preset::Medio.apply(&mut o, Some((3840, 2160)));
        assert_eq!(o.window, (1600, 900));
    }

    #[test]
    fn what_a_developer_types_is_added_as_it_is() {
        assert_eq!(typed_arguments("--ships 100   --npcs 0"), ["--ships", "100", "--npcs", "0"]);
        assert_eq!(typed_arguments("  --explode \"bomba grande@90\" --out \"\" x"), ["--explode", "bomba grande@90", "--out", "", "x"]);
        assert_eq!(typed_arguments("--nombre Ana\" de la \"Luna"), ["--nombre", "Ana de la Luna"]);
        assert_eq!(typed_arguments("--look 0@12,40,10\t\r\n"), ["--look", "0@12,40,10"]);
        assert!(typed_arguments("   ").is_empty() && typed_arguments("").is_empty());
        let o = Options { extra: "--ships 100 --npcs 0".to_string(), ..Options::default() };
        assert_eq!(line(&o, 35), "--vsync on --pantalla completa --ships 100 --npcs 0");
        assert_eq!(net(&Options { extra: "--visible".to_string(), ..with("127.0.0.1:47600", "Ana") }, 35).as_deref(), Ok("--vsync on --pantalla completa --servidor 127.0.0.1:47600 --nombre Ana --visible"));
        // a build from before the options is told nothing, not even that; nor is a script
        assert_eq!(line(&o, 21), "");
        assert_eq!(script_arguments(&o, &build(35, Edition::Debug), "carga").map(|a| a.join(" ")).as_deref(), Some("--guion tools/camara/carga.jsonc"));
        assert_eq!(boot_arguments(&o, &build(35, Edition::Debug)).map(|a| a.join(" ")).as_deref(), Some("--prueba-arranque"));
    }

    #[test]
    fn a_build_checks_its_own_start_from_the_version_that_knows_how() {
        let o = Options { quality: Quality::Alta, backend: Backend::Vulkan, sound: false, fullscreen: false, skip_menu: true, ..Options::default() };
        assert_eq!(boot_arguments(&o, &build(35, Edition::Demo)).map(|a| a.join(" ")).as_deref(), Some("--prueba-arranque --profile alta --backend vulkan"));
        assert_eq!(boot_arguments(&Options::default(), &build(36, Edition::Multiplayer)), Some(vec!["--prueba-arranque".to_string()]));
        assert_eq!(boot_arguments(&o, &build(34, Edition::Debug)), None);
        assert_eq!(boot_arguments(&o, &build(21, Edition::Old)), None);
    }

    #[test]
    fn the_multiplayer_build_is_told_the_server_and_the_name() {
        let o = with("127.0.0.1:47600", "Fernando");
        assert_eq!(net(&o, 35).as_deref(), Ok("--vsync on --pantalla completa --servidor 127.0.0.1:47600 --nombre Fernando"));
        let o = Options { quality: Quality::Alta, fullscreen: false, sound: false, skip_menu: true, ..with("  luna.example:47611 ", "Ana") };
        assert_eq!(net(&o, 36).as_deref(), Ok("--profile alta --vsync on --mudo --pantalla ventana --ventana 1600x900 --sin-menu --servidor luna.example:47611 --nombre Ana"));
        // (each is one argument, whatever spaces the name has)
        let args = arguments(&Options { edition: Edition::Multiplayer, ..with("[::1]:1", "Ana  de la Luna") }, &build(35, Edition::Multiplayer)).unwrap();
        assert_eq!(args[args.len() - 4..], ["--servidor", "[::1]:1", "--nombre", "Ana  de la Luna"]);
        assert_eq!(as_typed(&args), "--vsync on --pantalla completa --servidor [::1]:1 --nombre \"Ana  de la Luna\"");
    }

    #[test]
    fn the_defaults_join_a_server_on_this_machine_under_the_user_s_name() {
        let o = Options::default();
        assert_eq!(o.server, "127.0.0.1:47600");
        assert_eq!(o.name, name(&std::env::var("USERNAME").unwrap_or_default()));
        assert!(!o.name.is_empty() && o.name.chars().count() <= NAME_MOST);
        assert_eq!(net(&o, 35), Ok(format!("--vsync on --pantalla completa --servidor 127.0.0.1:47600 --nombre {}", o.name)));
    }

    #[test]
    fn no_address_is_playing_alone_with_neither_flag() {
        for none in ["", "   ", "\t"] {
            assert_eq!(net(&with(none, "Fernando"), 35).as_deref(), Ok("--vsync on --pantalla completa"), "{none:?}");
        }
    }

    #[test]
    fn the_name_is_given_clean() {
        assert_eq!(name("  Fernando \t"), "Fernando");
        assert_eq!(name("Ana\u{7}\r\n Luz\u{0}"), "Ana Luz");
        assert_eq!(name("Ñandú 🚀"), "Ñandú 🚀");
        // 24 characters at most (characters, not bytes), and no space left at the cut
        assert_eq!(name("ÁBCDEFGHIJKLMNÑOPQRSTUVWXYZ"), "ÁBCDEFGHIJKLMNÑOPQRSTUVW");
        assert_eq!(name("comandante de la nave   Cachalote"), "comandante de la nave");
        for none in ["", "   ", "\u{1b}\n"] {
            assert_eq!(name(none), "Jugador", "{none:?}");
        }
        assert_eq!(net(&with("127.0.0.1:47600", " \u{8}Ana\n"), 35).as_deref(), Ok("--vsync on --pantalla completa --servidor 127.0.0.1:47600 --nombre Ana"));
        assert_eq!(net(&with("127.0.0.1:47600", "  "), 35).as_deref(), Ok("--vsync on --pantalla completa --servidor 127.0.0.1:47600 --nombre Jugador"));
    }

    #[test]
    fn an_address_is_a_machine_and_a_port() {
        for (typed, given) in
            [("127.0.0.1:47600", "127.0.0.1:47600"), (" 192.168.1.37:1 ", "192.168.1.37:1"), ("localhost:65535", "localhost:65535"), ("mi-pc.casa_2:047600", "mi-pc.casa_2:47600"), ("[::1]:47600", "[::1]:47600"), ("[fe80::1c2:3]:80", "[fe80::1c2:3]:80")]
        {
            assert_eq!(address(typed), Ok(Some(given.to_string())), "{typed}");
        }
        assert_eq!(address(""), Ok(None));
        assert_eq!(address("  "), Ok(None));
    }

    #[test]
    fn a_wrong_address_is_refused_and_said() {
        const PORT: &str = "el puerto ha de ser un número de 1 a 65535";
        const HOST: &str = "la máquina no es un nombre ni una IP";
        for (typed, why) in [
            ("127.0.0.1", "le falta el puerto"),
            ("localhost", "le falta el puerto"),
            ("127.0.0.1:", "le falta el puerto"),
            (":47600", "le falta la máquina a la que conectarse"),
            ("127.0.0.1:0", PORT),
            ("127.0.0.1:65536", PORT),
            ("127.0.0.1:+47600", PORT),
            ("127.0.0.1:-1", PORT),
            ("127.0.0.1:puerto", PORT),
            ("127.0.0.1:47 600", PORT),
            ("mi pc:47600", HOST),
            ("http://luna.example:47600", HOST),
            ("::1:47600", HOST),
            ("[::1:47600", HOST),
            ("[no]:47600", HOST),
            ("-x:47600", HOST),
            ("a,b:47600", HOST),
        ] {
            let said = address(typed).unwrap_err();
            assert_eq!(said, format!("La dirección del servidor «{typed}» no vale: {why}. Se escribe dirección:puerto, por ejemplo 127.0.0.1:47600."));
            // and with it there is no command line at all: nothing is started
            assert_eq!(net(&with(typed, "Fernando"), 35), Err(said));
        }
        // (however much was typed, what is told back of it is short)
        let said = address(&"ñ".repeat(80)).unwrap_err();
        assert!(said.starts_with(&format!("La dirección del servidor «{}…» no vale: le falta el puerto.", "ñ".repeat(31))), "{said}");
    }

    #[test]
    fn only_the_multiplayer_edition_is_told_the_server_and_the_name() {
        let told = |args: &[String]| args.iter().any(|a| a == "--servidor" || a == "--nombre");
        for wanted in [Edition::Demo, Edition::Multiplayer, Edition::Debug] {
            let o = Options { edition: wanted, ..with("127.0.0.1:47600", "Fernando") };
            for edition in [Edition::Demo, Edition::Debug, Edition::Old] {
                for version in [9, 21, 22, 34, 35, 40] {
                    assert!(!told(&arguments(&o, &build(version, edition)).unwrap()), "{wanted:?} {edition:?} V{version}");
                }
            }
            // (nor is a wrong address any matter of theirs: they start as ever)
            let wrong = Options { edition: wanted, ..with("sin puerto", "Fernando") };
            assert_eq!(arguments(&wrong, &build(35, Edition::Demo)).unwrap().join(" "), "--vsync on --pantalla completa");
            assert_eq!(arguments(&wrong, &build(35, Edition::Debug)).unwrap().join(" "), "--vsync on --pantalla completa");
            assert_eq!(script_arguments(&o, &build(35, Edition::Debug), "carga").map(|a| told(&a)), Some(false));
            assert_eq!(script_arguments(&o, &build(35, Edition::Multiplayer), "carga"), None);
        }
        // the multiplayer build started in place of an edition its version lacks plays alone
        for wanted in [Edition::Demo, Edition::Debug] {
            let o = Options { edition: wanted, ..with("no vale", "Fernando") };
            assert_eq!(arguments(&o, &build(35, Edition::Multiplayer)).unwrap().join(" "), "--vsync on --pantalla completa");
        }
        assert!(told(&arguments(&Options { edition: Edition::Multiplayer, ..with("127.0.0.1:47600", "Fernando") }, &build(35, Edition::Multiplayer)).unwrap()));
    }
}
