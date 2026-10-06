//! What the launcher knows (the game's folder as it was last looked at, the options, the history,
//! the server beside the builds, what it started) and what each thing the player asks for comes
//! to. There is no window in it: it is driven by a test as it is by the window. What takes time
//! is done elsewhere (`jobs`) and taken in by `pump`.
use crate::{
    archive::{self, Plan},
    builds::{self, ARCHIVE, Build, Edition},
    clipboard,
    history::History,
    jobs::{Jobs, Wake},
    launch,
    news::News,
    options::{self, AfterPlay, Options},
    scan::{self, Scan},
    server::{self, Server},
    store::Kept,
    system::{self, Facts},
    tools::{self, Check, Page},
    versions::Version,
    words,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// How long a build must stay up to be taken as started: one that does not understand its command
/// line, or does not find its assets, is gone well before.
const GRACE: Duration = Duration::from_secs(2);
/// The time between looks at the server's folder while it is on show (it may be put there, started
/// or stopped with the launcher open).
const LOOK: Duration = Duration::from_secs(1);
/// How long after the server is started it is looked at again (it takes a moment to open its log).
const SERVER_UP: Duration = Duration::from_millis(1500);

/// The tabs of the plate on the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// The versions there are and what the chosen one brought: the first thing seen.
    Versions,
    Options,
    Multiplayer,
    Tools,
    System,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Versions, Tab::Options, Tab::Multiplayer, Tab::Tools, Tab::System];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Versions => "VERSIONES",
            Tab::Options => "OPCIONES",
            Tab::Multiplayer => "MULTIJUGADOR",
            Tab::Tools => "HERRAMIENTAS",
            Tab::System => "SISTEMA",
        }
    }
}

/// A file or a folder to open with the system's program for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Manual,
    Photos,
    ScriptPhotos,
    Root,
    Docs,
    Out,
    Archive,
    ServerFolder,
    ServerLog,
    /// A document, a log: by its place in the list.
    Doc(usize),
    Log(usize),
}

/// A text to put on the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text {
    /// The address the others join this machine's server at.
    Address,
    /// How the chosen build is started.
    CommandLine,
    Diagnostic,
    /// What the check of the start printed.
    Check,
}

/// What the player asked for in a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Start the chosen build.
    Play,
    /// Run the chosen camera script in the debug build.
    RunScript,
    /// Make the chosen build check its own start.
    CheckBoot,
    Open(Place),
    Copy(Text),
    /// The clipboard's text into the server's address.
    Paste,
    /// Start the server beside the builds, in a console window of its own.
    StartServer,
    /// Look at the game's folder again.
    Rescan,
    /// Put the old versions away: ask first, then (`Archive`) do it, or not (`Cancel`).
    AskArchive,
    Archive,
    Cancel,
    /// Bring a version that was put away back.
    Restore(u32),
    Quit,
}

/// What the last thing done came to, said in a notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub good: bool,
}

/// What an edition's card is for the chosen version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Card {
    /// It is the one that is started.
    Chosen,
    /// It can be chosen.
    Free,
    /// The chosen version has no such build: why, as it is told.
    Missing(String),
}

/// What was started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum What {
    Game {
        version: u32,
        edition: Edition,
    },
    /// A camera script: it ends by itself, its pictures taken.
    Script,
    /// A build checking its own start.
    Check,
}

/// A build that was started, looked after until it ends.
struct Running {
    /// Which of the things started it is (what ends says which it was).
    id: u64,
    /// What it is, as it is told ("V35 · DEMO", "El guion «carga»").
    name: String,
    what: What,
    started: Instant,
    /// When, by the clock (seconds since 1970); what it writes of an error is newer than `since`.
    at: u64,
    since: SystemTime,
    /// What the launcher does once it is seen to be up; none: it has been done, or there is
    /// nothing to do.
    after: Option<AfterPlay>,
}

/// What comes back from the work done elsewhere.
pub enum Message {
    Scanned(Box<Scan>),
    /// What was started (`id`) ended, with that exit code.
    Ended {
        id: u64,
        code: Option<i32>,
    },
    Checked(Box<Check>),
    /// The clipboard's text, or why it could not be had; `typed`: it goes where the keyboard is
    /// (it was asked for with the keys), not into the server's address.
    Pasted {
        text: Result<String, String>,
        typed: bool,
    },
}

/// Whether the launcher's window is to be on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Shown,
    /// Out of the way while the game is played: it comes back when the game ends.
    Hidden,
    /// The launcher is done.
    Gone,
}

pub struct Model {
    /// Where the launcher is: its options are kept there.
    home: PathBuf,
    /// The game's folder: the builds, `LEEME.txt`, `fotos`, `tools/camara`, `out`.
    pub root: PathBuf,
    /// The game's folder as it was last looked at; and whether it has been looked at yet.
    pub found: Scan,
    pub ready: bool,
    pub options: Options,
    pub history: History,
    /// The options and the history as they are in their file.
    saved: Kept,
    /// Whether they are read from their file and kept in it (not in `--prueba`).
    keep: bool,
    pub tab: Tab,
    /// The page of the tools' tab, and the camera script chosen in it.
    pub page: Page,
    pub script: usize,
    pub status: Option<Status>,
    /// The chosen version changed by the keyboard: its row is brought into view.
    pub reveal: bool,
    /// The old versions to put away, while the player is asked whether to.
    pub asking: Option<Plan>,
    /// What was pasted with the keys, for whatever is being typed in to take as its own.
    pub typed: Option<String>,
    /// The versions that came since the launcher last looked, until each is chosen or played.
    pub fresh: Vec<u32>,
    /// The newer launcher left beside this one has been told of.
    told_newer: bool,
    /// What the last check of a build's start came to.
    pub check: Option<Check>,
    /// What the window's side found out: the graphics cards, the screen.
    pub facts: Facts,
    running: Option<Running>,
    started: u64,
    presence: Presence,
    /// The server beside the builds, as it was when last looked at, and whether it is up.
    pub server: Server,
    pub server_up: bool,
    looked: Instant,
    /// When the server is looked at again without anybody asking (it was just started).
    again: Option<Instant>,
    jobs: Jobs<Message>,
}

/// What the game said as it stopped: the first line of the error file it wrote since `since`, if
/// it wrote one (`out/error.log`, or `out/panic.log`).
fn last_words(root: &Path, since: SystemTime) -> Option<String> {
    ["out/error.log", "out/panic.log"].iter().find_map(|file| {
        let path = root.join(file);
        let written = std::fs::metadata(&path).ok()?.modified().ok()?;
        if written < since {
            return None;
        }
        words::first_line(&String::from_utf8_lossy(&std::fs::read(path).ok()?))
    })
}

impl Model {
    /// The launcher's model: its options read from their file (with `keep`), the game's folder
    /// being looked at elsewhere (`pump` takes it in when it is done; `wake` is called then).
    pub fn load(keep: bool, wake: Wake) -> Model {
        let home = builds::home();
        let kept = if keep { Kept::load(&home) } else { Kept::default() };
        let mut model = Model::of(home.clone(), builds::root(&home), Vec::new(), kept, keep, wake);
        model.ready = false;
        model.rescan();
        model
    }

    /// The model of a game's folder with these builds and what is kept, before anything else of
    /// the folder is looked at.
    fn of(home: PathBuf, root: PathBuf, builds: Vec<Build>, kept: Kept, keep: bool, wake: Wake) -> Model {
        let versions = crate::versions::catalog(&builds, |_| crate::versions::Facts::default(), &[]);
        let mut model = Model {
            home,
            server: Server::look(&root),
            server_up: false,
            found: Scan { root: root.clone(), disk: crate::versions::disk(&versions, ARCHIVE), builds, versions, ..Scan::default() },
            root,
            ready: true,
            options: kept.options.clone(),
            history: kept.history.clone(),
            saved: kept,
            keep,
            tab: Tab::Versions,
            page: Page::default(),
            script: 0,
            status: None,
            reveal: true,
            asking: None,
            typed: None,
            fresh: Vec::new(),
            told_newer: false,
            check: None,
            facts: Facts::default(),
            running: None,
            started: 0,
            presence: Presence::Shown,
            looked: Instant::now(),
            again: None,
            jobs: Jobs::new(wake),
        };
        model.server_up = server::up(&model.server.folder);
        model
    }

    /// Look at the game's folder again, elsewhere.
    pub fn rescan(&mut self) {
        let (root, known) = (self.root.clone(), self.ready.then(|| self.found.known.clone()).filter(|k| k.windows.is_some()));
        self.jobs.spawn(move || Message::Scanned(Box::new(scan::look(&root, known))));
    }

    /// Look again at the server's folder, if it is a moment since the last look.
    pub fn look_for_server(&mut self) {
        if self.looked.elapsed() >= LOOK {
            self.look_at_server();
        }
    }

    fn look_at_server(&mut self) {
        self.looked = Instant::now();
        self.server = Server::look(&self.root);
        self.server_up = server::up(&self.server.folder);
    }

    /// The addresses the server of this machine is reached at: by the others on its network (none:
    /// this machine's address there is not known) and from this same machine.
    pub fn reach(&self) -> (Vec<String>, String) {
        server::reach(&self.found.known.lan, self.server.port)
    }

    /// Where the server's program is, or should be, as it is told.
    pub fn server_place(&self) -> String {
        self.server.folder.join(self.server.program).display().to_string()
    }

    /// The build that would be started now.
    pub fn chosen(&self) -> Option<&Build> {
        builds::pick(&self.found.builds, self.options.version, self.options.edition)
    }

    /// The version that is chosen, as it is in the list.
    pub fn version(&self) -> Option<&Version> {
        let number = self.chosen()?.version;
        self.found.versions.iter().find(|v| v.number == number)
    }

    /// Tell the versions that are new since the launcher last looked (the ones above the newest
    /// it had seen; none the first time it ever looks), and take the newest there is as seen.
    fn note_new(&mut self) {
        let newest = self.found.versions.iter().map(|v| v.number).max().unwrap_or(0);
        if self.options.seen > 0 {
            let seen = self.options.seen;
            let new: Vec<u32> = self.found.versions.iter().map(|v| v.number).filter(|n| *n > seen && !self.fresh.contains(n)).collect();
            self.fresh.extend(new);
        }
        self.options.seen = self.options.seen.max(newest);
    }

    /// Choose a version. The one the launcher would pick by itself (the newest that has the edition
    /// asked) is not kept by its number: a newer one takes its place when it comes.
    pub fn select(&mut self, version: u32) {
        self.fresh.retain(|n| *n != version);
        let newest = builds::pick(&self.found.builds, None, self.options.edition).map(|b| b.version);
        self.options.version = if newest == Some(version) { None } else { Some(version) };
        self.asking = None;
    }

    /// Choose the version `rows` further down the list (up, if negative), stopping at its ends.
    pub fn step(&mut self, rows: i32) {
        let versions = &self.found.versions;
        let Some(at) = self.version().and_then(|chosen| versions.iter().position(|v| v.number == chosen.number)) else { return };
        let to = (at as i64 + i64::from(rows)).clamp(0, versions.len() as i64 - 1) as usize;
        if to != at {
            let number = versions[to].number;
            self.select(number);
            self.reveal = true;
        }
    }

    /// What an edition's card is for the chosen version.
    pub fn card(&self, edition: Edition) -> Card {
        let Some(build) = self.chosen() else { return Card::Missing("No hay ninguna versión del juego en la carpeta.".to_string()) };
        let version = build.version;
        let has = builds::editions(&self.found.builds, version);
        if build.edition == edition {
            Card::Chosen
        } else if has.contains(&edition) {
            Card::Free
        } else if has == [Edition::Old] {
            Card::Missing(format!("La V{version} es de antes de las ediciones: solo hay una."))
        } else if version < edition.since() {
            Card::Missing(format!("La V{version} no la tiene: existe desde la V{}.", edition.since()))
        } else {
            Card::Missing(format!("Falta {} en la carpeta.", builds::file_name(version, edition)))
        }
    }

    /// Choose an edition, if the chosen version has it. Whether it was chosen.
    pub fn choose(&mut self, edition: Edition) -> bool {
        let free = self.card(edition) == Card::Free;
        if free {
            // (the version stays the one that is on show, whatever "the newest" is for the new edition)
            let version = self.chosen().map(|b| b.version);
            self.options.edition = edition;
            if let Some(version) = version {
                self.select(version);
            }
        }
        free
    }

    /// What the chosen version brought, as the manual tells it.
    pub fn news(&self) -> Option<&News> {
        self.version().and_then(|v| v.section).and_then(|n| self.found.sections.get(n)).map(|s| &s.news)
    }

    /// The chosen build and the command line it would be started with; or why it cannot be
    /// started, as it is told to the player (there is no build; the server's address is wrong).
    pub fn command(&self) -> Result<(&Build, Vec<String>), String> {
        let build = self.chosen().ok_or_else(|| if self.ready { format!("No hay ninguna versión del juego ({}) en {}.", builds::any_name(), self.root.display()) } else { "Todavía se está mirando la carpeta del juego.".to_string() })?;
        Ok((build, options::arguments(&self.options, build)?))
    }

    /// The debug build of the chosen version (whatever the edition chosen to play) and the command
    /// line that runs the chosen script in it; or why not, as it is told to the player.
    pub fn script_command(&self) -> Result<(&Build, Vec<String>), String> {
        let version = self.chosen().ok_or("No hay ninguna versión del juego en la carpeta.")?.version;
        let build = self.found.builds.iter().find(|b| b.version == version && b.edition == Edition::Debug).ok_or(format!("La V{version} no tiene edición DEBUG: los guiones solo se ejecutan en ella."))?;
        let script = self.found.scripts.get(self.script).ok_or(format!("No hay guiones en {}.", builds::SCRIPTS))?;
        let args = options::script_arguments(&self.options, build, &script.name).ok_or(format!("La V{} no ejecuta guiones: hace falta la V{} o una posterior.", build.version, options::SCRIPTS_SINCE))?;
        Ok((build, args))
    }

    /// The chosen build and the command line that makes it check its own start; or why not.
    pub fn boot_command(&self) -> Result<(&Build, Vec<String>), String> {
        let build = self.chosen().ok_or("No hay ninguna versión del juego en la carpeta.")?;
        let args = options::boot_arguments(&self.options, build).ok_or(format!("La V{} no sabe comprobar su arranque: hace falta la V{} o una posterior.", build.version, options::BOOT_SINCE))?;
        Ok((build, args))
    }

    /// What is kept, as it is now.
    fn kept(&self) -> Kept {
        Kept { options: self.options.clone(), history: self.history.clone() }
    }

    /// Keep the options and the history in their file if they have changed. What another launcher
    /// open at the same time has kept there since is not lost: what was played here is added to
    /// its history, and its options stand unless they were changed here too.
    pub fn save(&mut self) {
        if !self.keep || (self.options == self.saved.options && self.history == self.saved.history) {
            return;
        }
        let on_disk = Kept::load(&self.home);
        if on_disk != self.saved {
            self.history = self.history.added_to(&self.saved.history, &on_disk.history);
            if self.options == self.saved.options {
                self.options = on_disk.options;
            }
        }
        let kept = self.kept();
        if let Err(e) = kept.save(&self.home) {
            self.say(false, format!("No se pudieron guardar las opciones ({}): {e}", options::FILE));
        }
        // (kept or not, it is not tried again until they change again)
        self.saved = kept;
    }

    fn say(&mut self, good: bool, text: String) {
        self.status = Some(Status { text, good });
    }

    /// The name of what is running, if something that was started still is.
    pub fn busy(&self) -> Option<&str> {
        self.running.as_ref().map(|run| run.name.as_str())
    }

    /// What is running, in the few words the big button has room for ("V35 · DEMO EN MARCHA").
    pub fn busy_says(&self) -> Option<String> {
        self.running.as_ref().map(|run| match run.what {
            What::Game { .. } => format!("{} EN MARCHA", run.name.to_uppercase()),
            What::Script => "GUION EN MARCHA".to_string(),
            What::Check => "COMPROBANDO EL ARRANQUE".to_string(),
        })
    }

    /// Whether a build's check of its own start is under way.
    pub fn checking(&self) -> bool {
        self.running.as_ref().is_some_and(|run| run.what == What::Check)
    }

    /// One thing at a time: what was started before must have ended. Whether there is nothing in
    /// the way (if there is, it is said).
    fn free(&mut self) -> bool {
        let Some(name) = self.busy().map(str::to_string) else { return true };
        self.say(false, format!("{name} sigue en marcha: espera a que termine antes de iniciar otra cosa."));
        false
    }

    /// Start a build with its command line and look after it from now on.
    fn start(&mut self, file: &str, args: &[String], name: String, what: What) {
        if !self.free() {
            return;
        }
        // (the file's clock is coarser than ours)
        let since = SystemTime::now() - Duration::from_secs(2);
        match launch::start(&self.root, file, args) {
            Ok(mut child) => {
                self.started += 1;
                let id = self.started;
                let at = words::now();
                let game = matches!(what, What::Game { .. });
                self.say(true, if game { format!("{name} en marcha") } else { format!("{name} está en marcha: sus fotos quedan en {}", tools::SCRIPT_PHOTOS) });
                if let What::Game { version, edition } = what {
                    self.history.started(version, edition, at);
                    self.fresh.retain(|n| *n != version);
                }
                self.running = Some(Running { id, name, what, started: Instant::now(), at, since, after: game.then_some(self.options.after_play).filter(|after| *after != AfterPlay::Stay) });
                self.jobs.spawn(move || Message::Ended { id, code: child.wait().ok().and_then(|status| status.code()) });
            }
            Err(e) => self.say(false, format!("No se pudo iniciar {file}: {e}")),
        }
    }

    /// Whether the window is to be on the screen.
    pub fn presence(&self) -> Presence {
        self.presence
    }

    /// When the model is to be looked at again though nothing has happened: the moment a game
    /// just started is taken as up, or the server just started is looked for.
    pub fn next_wake(&self) -> Option<Instant> {
        let up = self.running.as_ref().filter(|run| run.after.is_some()).map(|run| run.started + GRACE);
        [up, self.again].into_iter().flatten().min()
    }

    /// Take in what the work done elsewhere came to, and do what is due by the clock. Whether
    /// there is something new to show.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        while let Some(message) = self.jobs.take() {
            changed = true;
            match message {
                Message::Scanned(scan) => {
                    self.found = *scan;
                    self.root = self.found.root.clone();
                    self.ready = true;
                    self.script = self.script.min(self.found.scripts.len().saturating_sub(1));
                    self.note_new();
                    if let Some(newer) = self.found.newer.clone().filter(|_| !std::mem::replace(&mut self.told_newer, true)) {
                        self.say(true, format!("Hay un launcher más nuevo junto a este: cierra este y abre {newer}."));
                    }
                }
                Message::Ended { id, code } => self.ended(id, code),
                Message::Checked(check) => {
                    self.say(check.good, format!("{}: {}", check.name, check.verdict));
                    self.check = Some(*check);
                    self.running = None;
                    self.rescan();
                }
                // (what is typed in takes of it what it has room for; the address, what an address may have)
                Message::Pasted { text: Ok(text), typed } => match clipboard::line(&text, if typed { options::EXTRA_MOST } else { options::ADDRESS_MOST }) {
                    line if line.is_empty() => self.say(false, "El portapapeles no tiene texto que pegar.".to_string()),
                    line if typed => self.typed = Some(line),
                    line => self.options.server = line,
                },
                Message::Pasted { text: Err(why), .. } => self.say(false, format!("No se pudo pegar: {why}.")),
            }
        }
        let now = Instant::now();
        // a game that is still up after a moment has started: the launcher gets out of its way
        if let Some(run) = self.running.as_mut().filter(|run| run.after.is_some() && now >= run.started + GRACE) {
            self.presence = if run.after.take() == Some(AfterPlay::Close) { Presence::Gone } else { Presence::Hidden };
            self.status = None;
            changed = true;
        }
        if self.again.is_some_and(|at| now >= at) {
            self.again = None;
            self.look_at_server();
            changed = true;
        }
        changed
    }

    /// What was started has ended: a game is counted and, if it stopped with an error, what it
    /// said is told (a game without a console says nothing by itself); a script has its pictures.
    fn ended(&mut self, id: u64, code: Option<i32>) {
        let Some(run) = self.running.take_if(|run| run.id == id) else { return };
        let (good, seconds) = (code == Some(0), run.started.elapsed().as_secs());
        let said = (!good).then(|| last_words(&self.root, run.since)).flatten();
        let wrong = || format!("{} se cerró con un error: {}", run.name, said.clone().unwrap_or_else(|| format!("código {}; mira out/error.log", code.map_or("desconocido".to_string(), |c| c.to_string()))));
        match run.what {
            What::Game { version, edition } => {
                self.history.ended(version, edition, run.at, seconds, code, said.clone());
                self.status = Some(if good { Status { good, text: format!("{}: {} de partida.", run.name, words::span(seconds)) } } else { Status { good, text: wrong() } });
            }
            What::Script if good => self.say(true, format!("{} ha terminado: sus fotos están en {}", run.name, tools::SCRIPT_PHOTOS)),
            What::Script | What::Check => self.status = Some(Status { good, text: wrong() }),
        }
        // (the window is back, if it was out of the way; and what the game wrote is there to see)
        self.presence = Presence::Shown;
        self.rescan();
    }

    /// Do what was asked.
    pub fn act(&mut self, action: Action) {
        match action {
            Action::Quit => self.presence = Presence::Gone,
            Action::Play => match self.command() {
                Ok((build, args)) => {
                    let (file, name, what) = (build.path(), build.name(), What::Game { version: build.version, edition: build.edition });
                    let joined = options::joins(&self.options, build).then(|| options::address(&self.options.server).ok().flatten()).flatten();
                    self.start(&file, &args, name, what);
                    if let Some(server) = joined.filter(|_| self.running.as_ref().is_some_and(|run| run.what == what)) {
                        self.options.joined(&server);
                    }
                }
                Err(why) => self.say(false, why),
            },
            Action::RunScript => match self.script_command() {
                Ok((build, args)) => {
                    let (file, name) = (build.path(), format!("El guion «{}»", self.found.scripts[self.script].name));
                    self.start(&file, &args, name, What::Script);
                }
                Err(why) => self.say(false, why),
            },
            Action::CheckBoot => match self.boot_command() {
                Ok((build, args)) => {
                    let (file, name) = (build.path(), build.name());
                    if self.free() {
                        self.started += 1;
                        self.running = Some(Running { id: self.started, name: format!("La comprobación del arranque de {name}"), what: What::Check, started: Instant::now(), at: words::now(), since: SystemTime::now(), after: None });
                        self.check = None;
                        self.status = None;
                        let root = self.root.clone();
                        self.jobs.spawn(move || Message::Checked(Box::new(tools::check(&root, &name, &file, &args))));
                    }
                }
                Err(why) => self.say(false, why),
            },
            Action::Open(place) => self.open(place),
            Action::Copy(text) => self.copy(text),
            Action::Paste => self.paste(false),
            Action::StartServer => self.start_server(),
            Action::Rescan => self.rescan(),
            Action::AskArchive => self.asking = Some(archive::plan(&self.found.versions, self.options.keep_versions)),
            Action::Cancel => self.asking = None,
            Action::Archive => {
                // (what is moved is what there is now, not what there was when it was asked)
                let plan = archive::plan(&self.found.versions, self.options.keep_versions);
                let (text, good) = archive::put_away(&self.root, &plan.files).says(&format!("Archivados en {ARCHIVE}"));
                self.asking = None;
                self.moved(text, good);
            }
            Action::Restore(version) => {
                let files = self.found.versions.iter().find(|v| v.number == version).map(archive::away).unwrap_or_default();
                let (text, good) = archive::bring_back(&self.root, &files).says(&format!("La V{version} vuelve a la carpeta del juego"));
                self.moved(text, good);
            }
        }
    }

    /// Ask the system, elsewhere, for the clipboard's text: into the server's address, or
    /// (`typed`) to wherever the keyboard is.
    pub fn paste(&mut self, typed: bool) {
        self.jobs.spawn(move || Message::Pasted { text: clipboard::paste(), typed });
    }

    /// Builds were moved: it is said, and the folder is looked at again at once (the list shows
    /// them where they are now).
    fn moved(&mut self, text: String, good: bool) {
        self.say(good, text);
        self.found = scan::look(&self.root, Some(self.found.known.clone()));
    }

    /// Where a place is, and whether it is a folder to be made if it is not there.
    fn place(&self, place: Place) -> Option<(PathBuf, bool)> {
        let root = &self.root;
        Some(match place {
            Place::Manual => (root.join("LEEME.txt"), false),
            Place::Photos => (root.join("fotos"), true),
            Place::ScriptPhotos => (root.join(tools::SCRIPT_PHOTOS), true),
            Place::Out => (root.join(tools::OUT), true),
            Place::Root => (root.clone(), false),
            Place::Docs => (root.join(tools::DOCS), false),
            Place::Archive => (root.join(ARCHIVE), false),
            // (a folder that is not there is not made: the server is put there whole)
            Place::ServerFolder => (self.server.folder.clone(), false),
            Place::ServerLog => (self.server.folder.join(server::LOG), false),
            Place::Doc(n) => (root.join(tools::DOCS).join(&self.found.docs.get(n)?.file), false),
            Place::Log(n) => (root.join(&self.found.logs.get(n)?.path), false),
        })
    }

    fn open(&mut self, place: Place) {
        let Some((path, make)) = self.place(place) else { return };
        let opened = if make {
            launch::open_folder(&path).map_err(|e| e.to_string())
        } else if path.exists() {
            launch::open(&path).map_err(|e| e.to_string())
        } else {
            Err("no está en la carpeta del juego".to_string())
        };
        if let Err(e) = opened {
            let name = path.strip_prefix(&self.root).unwrap_or(&path).display().to_string();
            self.say(false, format!("No se pudo abrir {name}: {e}"));
        }
    }

    /// The text that is copied, and what it is called; none if there is none.
    pub fn text(&self, text: Text) -> Option<(String, &'static str)> {
        match text {
            Text::Address => self.reach().0.into_iter().next().map(|address| (address, "La dirección de tu servidor")),
            Text::CommandLine => self.command().ok().map(|(build, args)| (format!("{} {}", build.path(), options::as_typed(&args)).trim_end().to_string(), "La línea de órdenes")),
            Text::Diagnostic => Some((system::diagnostic(self), "El diagnóstico")),
            Text::Check => self.check.as_ref().map(|check| (check.text(), "El resultado de la comprobación")),
        }
    }

    fn copy(&mut self, text: Text) {
        let Some((text, name)) = self.text(text) else { return };
        match clipboard::copy(&text) {
            Ok(()) => self.say(true, format!("{name} está en el portapapeles.")),
            Err(e) => self.say(false, format!("No se pudo copiar: {e}")),
        }
    }

    /// Start the server beside the builds, unless it is already up. It is not looked after: it
    /// runs in a console window of its own, and closing that stops it.
    fn start_server(&mut self) {
        self.look_at_server();
        if !self.server.ready {
            let text = format!("No está el servidor: debería estar en {}", self.server_place());
            self.say(false, text);
            return;
        }
        if self.server_up {
            let text = format!("El servidor local ya está en marcha (puerto {}): no se inicia otro.", self.server.port);
            self.say(false, text);
            return;
        }
        match launch::start_server(&self.server.folder, self.server.program) {
            Ok(()) => {
                let text = format!("Servidor local iniciado en su propia ventana (puerto {}): ciérrala para pararlo.", self.server.port);
                self.say(true, text);
                self.again = Some(Instant::now() + SERVER_UP);
            }
            Err(e) => self.say(false, format!("No se pudo iniciar {}: {e}", self.server.program)),
        }
    }
}

/// The states `--prueba-caso` can put the launcher in for its picture.
pub const CASES: [&str; 16] = ["v36", "nueva", "historial", "fallo", "archivadas", "guardar", "aviso", "error", "en-marcha", "comprobando", "comprobado", "comprobado-mal", "recientes", "servidor-parado", "servidor-en-marcha", "sin-versiones"];

impl Model {
    /// Put the model in one of the states of `CASES`, for a picture of it (`--prueba-caso`):
    /// nothing is started, moved or written. Whether there is such a state.
    pub fn stage(&mut self, case: &str) -> bool {
        let now = words::now();
        // (the server is not looked at again: it stays as the picture wants it)
        let hold = Instant::now() + Duration::from_secs(3600);
        match case {
            // the next version, as it will be delivered: its three builds under the game's name
            "v36" => {
                let like = self.found.versions.first().cloned().unwrap_or_default();
                let number = like.number.max(35) + 1;
                let names: Vec<String> = Edition::CARDS.iter().map(|edition| builds::file_name(number, *edition)).chain(self.found.builds.iter().filter(|b| !b.archived).map(|b| b.file.clone())).collect();
                let away: Vec<String> = self.found.builds.iter().filter(|b| b.archived).map(|b| b.file.clone()).collect();
                if !self.found.sections.iter().any(|s| s.version == number) {
                    let news = crate::news::read_section(&format!("NOVEDADES V{number}:\nEJEMPLO PARA LA FOTO: el manual todavia no cuenta que trae la V{number}.\nRED: la carga suelta se comparte entre los jugadores.\n"));
                    self.found.sections.insert(0, crate::news::Section { version: number, headline: crate::news::headline(&news), news });
                }
                let old = std::mem::take(&mut self.found.versions);
                let size = like.files.first().map_or(23_000_000, |(_, facts)| facts.size);
                self.restage(builds::discover_both(&names, &away), &old, crate::versions::Facts { size, modified: now - 1800 });
            }
            "archivadas" => {
                let keep: Vec<u32> = self.found.versions.iter().take(6).map(|v| v.number).collect();
                let builds: Vec<Build> = self.found.builds.iter().map(|b| Build { archived: b.archived || !keep.contains(&b.version), ..b.clone() }).collect();
                let old = std::mem::take(&mut self.found.versions);
                self.restage(builds, &old, crate::versions::Facts::default());
            }
            "nueva" => self.fresh = self.found.versions.first().map(|v| v.number).into_iter().collect(),
            "sin-versiones" => {
                self.found.builds.clear();
                self.found.versions.clear();
                self.found.disk = Default::default();
            }
            "historial" => {
                let newest = self.found.versions.first().map_or(35, |v| v.number);
                for (version, edition, ago, seconds, times) in [(newest - 1, Edition::Demo, 3 * 86_400, 4320, 4), (newest - 1, Edition::Debug, 2 * 86_400, 900, 9), (newest, Edition::Debug, 86_400, 1500, 3), (newest, Edition::Demo, 5400, 2520, 2)] {
                    for n in 0..times {
                        self.history.started(version, edition, now - ago - n * 600);
                    }
                    self.history.ended(version, edition, now - ago, seconds, Some(0), None);
                }
            }
            "fallo" => {
                let version = self.version().map_or(34, |v| v.number);
                self.history.started(version, Edition::Demo, now - 7200);
                self.history.ended(version, Edition::Demo, now - 7200, 2, Some(1), Some("assets/defs/components/naves.jsonc: unknown field `cabida`".to_string()));
            }
            "guardar" => self.asking = Some(archive::plan(&self.found.versions, self.options.keep_versions)),
            "aviso" => self.say(true, "V35 · DEMO: 42 min de partida.".to_string()),
            "error" => self.say(false, "V35 · DEBUG se cerró con un error: opción desconocida: --pantalla (esta versión no la entiende; mira out/error.log para ver el resto de lo que dijo el juego al cerrarse)".to_string()),
            "en-marcha" | "comprobando" => {
                let name = self.chosen().map_or("V35 · DEMO".to_string(), Build::name);
                let (name, what) = if case == "comprobando" { (format!("La comprobación del arranque de {name}"), What::Check) } else { (name, What::Game { version: 35, edition: Edition::Demo }) };
                self.running = Some(Running { id: 0, name, what, started: Instant::now(), at: now, since: SystemTime::now(), after: None });
            }
            "comprobado" => {
                self.check = Some(Check {
                    name: self.chosen().map_or("V35 · DEMO".to_string(), Build::name),
                    line: "--prueba-arranque".to_string(),
                    good: true,
                    verdict: "Arranca bien (9,4 s).".to_string(),
                    output: "arranque: el juego hecho en su hilo y 40 fotogramas dibujados; menú de inicio puesto".to_string(),
                })
            }
            "comprobado-mal" => {
                let output = "opción desconocida: --prueba-arranque\n\nSELENE\n  selene                       jugar\n  --ships N --npcs N           naves y NPC (por defecto, los de assets/defs/scenario.jsonc)\n  --profile NOMBRE             horrible|muybaja|baja|media|alta|muyalta|ultra|esplendidos (o low|high)";
                self.check = Some(Check { name: "V34 · DEBUG".to_string(), line: "LunaV34_debug.exe --prueba-arranque".to_string(), good: false, verdict: "Falla al arrancar: código 1 (0,3 s).".to_string(), output: output.to_string() });
            }
            "recientes" => self.options.servers = ["192.168.1.67:47600", "luna.example.org:47611", "127.0.0.1:47600"].map(str::to_string).to_vec(),
            "servidor-parado" | "servidor-en-marcha" => {
                self.server.ready = true;
                self.server_up = case == "servidor-en-marcha";
                self.looked = hold;
            }
            _ => return false,
        }
        true
    }

    /// For `--prueba-ocultar`: a stand-in for a game that lasts some `seconds` and ends well,
    /// started as a game is with the launcher set to get out of its way and come back (the one
    /// thing a test of the window starts: the system's `cmd`, waiting with no window; nothing
    /// leaves the machine). Whether it was started.
    pub fn stage_game(&mut self, seconds: u32) -> bool {
        let Ok(shell) = std::env::var("ComSpec") else { return false };
        self.options.after_play = AfterPlay::Hide;
        self.start(&shell, &["/C".to_string(), format!("ping -n {} 127.0.0.1 >nul", seconds + 1)], "LA PRUEBA".to_string(), What::Game { version: 0, edition: Edition::Demo });
        self.running.is_some()
    }

    /// The versions made again from other builds, for a picture: each file with what was known
    /// of it, or with `new` if it is a new one.
    fn restage(&mut self, builds: Vec<Build>, old: &[Version], new: crate::versions::Facts) {
        let known = |b: &Build| old.iter().flat_map(|v| &v.files).find(|(path, _)| path.ends_with(&b.file)).map_or(new, |(_, facts)| *facts);
        self.found.versions = crate::versions::catalog(&builds, known, &self.found.sections);
        self.found.disk = crate::versions::disk(&self.found.versions, ARCHIVE);
        self.found.builds = builds;
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn every_state_for_a_picture_can_be_staged_and_starts_nothing() {
        let names = ["LunaV35_demo.exe", "LunaV35_multiplayer.exe", "LunaV35_debug.exe", "LunaV34_demo.exe", "LunaV34_debug.exe", "LunaV33_demo.exe", "LunaV32_demo.exe", "LunaV31_demo.exe", "LunaV30_demo.exe", "LunaV21.exe", "LunaV9.exe"];
        for case in CASES {
            let mut m = of_nowhere(&names, Options::default());
            assert!(m.stage(case), "{case}");
            assert_eq!(m.presence(), Presence::Shown, "{case}");
        }
        assert!(!of_nowhere(&names, Options::default()).stage("no-existe"));
        // the next version comes with its three builds, under the game's name, and its news
        let mut m = of_nowhere(&names, Options::default());
        m.stage("v36");
        assert_eq!(m.chosen().map(|b| (b.file.as_str(), b.name())), Some(("SeleneV36_demo.exe", "V36 · DEMO".to_string())));
        assert_eq!(m.found.versions.len(), 9);
        assert_eq!(m.news().map(|news| news.title.as_str()), Some("NOVEDADES V36"));
        assert_eq!(Edition::CARDS.map(|edition| m.card(edition)), [Card::Chosen, Card::Free, Card::Free]);
        // the old ones put away, in the picture only
        m.stage("archivadas");
        assert_eq!(m.found.versions.iter().filter(|v| v.archived).map(|v| v.number).collect::<Vec<_>>(), [30, 21, 9]);
        assert_eq!((m.found.disk.0.versions, m.found.disk.1.versions), (6, 3));
        m.stage("guardar");
        assert_eq!(m.asking.as_ref().map(|plan| plan.versions.clone()), Some(vec![31]));
        m.stage("historial");
        assert_eq!((m.history.of_version(36).starts, m.history.of_version(35).starts, m.history.total().seconds), (5, 13, 9240));
        m.stage("en-marcha");
        assert_eq!((m.busy(), m.busy_says().as_deref()), (Some("V36 · DEMO"), Some("V36 · DEMO EN MARCHA")));
        m.stage("comprobando");
        assert_eq!((m.busy(), m.busy_says().as_deref(), m.checking()), (Some("La comprobación del arranque de V36 · DEMO"), Some("COMPROBANDO EL ARRANQUE"), true));
        m.stage("sin-versiones");
        assert!(m.chosen().is_none());
    }

    #[test]
    fn an_error_file_older_than_the_start_is_not_of_this_run() {
        let dir = std::env::temp_dir().join(format!("luna-launcher-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("out")).unwrap();
        std::fs::write(dir.join("out/panic.log"), "se rompió algo\nmás").unwrap();
        let (before, after) = (SystemTime::now() - Duration::from_secs(60), SystemTime::now() + Duration::from_secs(60));
        assert_eq!(last_words(&dir, before).as_deref(), Some("se rompió algo"));
        assert_eq!(last_words(&dir, after), None);
        std::fs::write(dir.join("out/error.log"), "opción desconocida: --x").unwrap();
        assert_eq!(last_words(&dir, before).as_deref(), Some("opción desconocida: --x"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A model of the folder `root`, with builds of these names, that keeps nothing in any file.
    pub fn at(root: &Path, names: &[&str], options: Options) -> Model {
        Model::of(root.to_path_buf(), root.to_path_buf(), builds::discover(names), Kept { options, history: History::default() }, false, Arc::new(|| {}))
    }

    /// A model of a folder that is not there, with builds of these names: nothing of it can be
    /// started.
    pub fn of_nowhere(names: &[&str], options: Options) -> Model {
        at(Path::new("no/existe/esta/carpeta"), names, options)
    }

    const NAMES: [&str; 9] = ["SeleneV36_debug.exe", "LunaV35_demo.exe", "LunaV35_multiplayer.exe", "LunaV35_debug.exe", "LunaV34_demo.exe", "LunaV34_debug.exe", "LunaV33_demo.exe", "LunaV21.exe", "LunaV9.exe"];

    #[test]
    fn a_wrong_server_address_is_said_instead_of_starting_the_game() {
        let names = ["LunaV35_demo.exe", "LunaV35_multiplayer.exe", "LunaV35_debug.exe", "LunaV34_demo.exe", "LunaV34_debug.exe"];
        let mut m = of_nowhere(&names, Options { edition: Edition::Multiplayer, server: "127.0.0.1".to_string(), name: "Fernando".to_string(), ..Options::default() });
        assert_eq!(m.chosen().map(|b| b.file.as_str()), Some("LunaV35_multiplayer.exe"));
        m.act(Action::Play);
        assert!(m.busy().is_none() && m.presence() == Presence::Shown);
        let said = m.status.take().unwrap();
        assert!(!said.good && said.text.starts_with("La dirección del servidor «127.0.0.1» no vale: le falta el puerto."), "{}", said.text);
        // put right, the multiplayer build is told the server and the name
        m.options.server = "127.0.0.1:47600".to_string();
        let line = |m: &Model| m.command().map(|(build, args)| format!("{} {}", build.file, args.join(" ")));
        assert_eq!(line(&m).as_deref(), Ok("LunaV35_multiplayer.exe --vsync on --pantalla completa --servidor 127.0.0.1:47600 --nombre Fernando"));
        // a version without that edition starts in the one it has, which is told neither
        m.options.version = Some(34);
        assert_eq!(line(&m).as_deref(), Ok("LunaV34_demo.exe --vsync on --pantalla completa"));
        // and the other editions never are, whatever the address
        m.options = Options { version: None, edition: Edition::Demo, server: "no vale".to_string(), ..m.options.clone() };
        assert_eq!(line(&m).as_deref(), Ok("LunaV35_demo.exe --vsync on --pantalla completa"));
        m.options.edition = Edition::Debug;
        assert_eq!(line(&m).as_deref(), Ok("LunaV35_debug.exe --vsync on --pantalla completa"));
    }

    #[test]
    fn a_server_that_is_not_there_is_said_and_nothing_is_started() {
        let mut m = of_nowhere(&[], Options::default());
        assert!(!m.server.ready && !m.server.has_folder && !m.server_up);
        assert_eq!(m.reach(), (Vec::new(), "127.0.0.1:47600".to_string()));
        assert_eq!(m.text(Text::Address), None);
        m.act(Action::StartServer);
        let said = m.status.take().unwrap();
        assert!(!said.good && said.text.starts_with("No está el servidor: debería estar en ") && said.text.ends_with("SeleneServidor.exe") && said.text.contains("servidores"), "{}", said.text);
        m.act(Action::Open(Place::ServerFolder));
        assert_eq!(m.status.as_ref().map(|s| (s.good, s.text.as_str())), Some((false, "No se pudo abrir servidores: no está en la carpeta del juego")));
        m.act(Action::Open(Place::Doc(3)));
        m.act(Action::Open(Place::Manual));
        assert_eq!(m.status.as_ref().map(|s| s.text.as_str()), Some("No se pudo abrir LEEME.txt: no está en la carpeta del juego"));
        // (and with no build at all, the command line is why not)
        assert!(m.command().unwrap_err().starts_with("No hay ninguna versión del juego"));
        assert_eq!(m.text(Text::CommandLine), None);
    }

    #[test]
    fn the_cards_tell_what_the_chosen_version_has() {
        let mut m = of_nowhere(&NAMES, Options::default());
        let cards = |m: &Model| Edition::CARDS.map(|edition| m.card(edition));
        let missing = |why: &str| Card::Missing(why.to_string());
        // the newest demo is the V35: it has the three
        assert_eq!(m.chosen().map(Build::name).as_deref(), Some("V35 · DEMO"));
        assert_eq!(cards(&m), [Card::Chosen, Card::Free, Card::Free]);
        // the V36 has only its debug build yet: the other two say which file is missing
        m.select(36);
        assert_eq!(m.options.version, Some(36));
        assert_eq!(m.chosen().map(Build::name).as_deref(), Some("V36 · DEBUG"));
        assert_eq!(cards(&m), [missing("Falta SeleneV36_demo.exe en la carpeta."), Card::Chosen, missing("Falta SeleneV36_multiplayer.exe en la carpeta.")]);
        // (a version from before the game was renamed misses a file with the name it had)
        assert_eq!(of_nowhere(&["LunaV35_debug.exe"], Options::default()).card(Edition::Demo), missing("Falta LunaV35_demo.exe en la carpeta."));
        assert!(!m.choose(Edition::Demo) && m.options.edition == Edition::Demo);
        // one from before the multiplayer edition, and one from before any edition
        m.select(34);
        assert_eq!(cards(&m), [Card::Chosen, Card::Free, missing("La V34 no la tiene: existe desde la V35.")]);
        m.select(21);
        assert_eq!(m.chosen().map(Build::name).as_deref(), Some("V21 · ANTIGUA"));
        assert_eq!(cards(&m), [missing("La V21 es de antes de las ediciones: solo hay una."), missing("La V21 es de antes de las ediciones: solo hay una."), missing("La V21 es de antes de las ediciones: solo hay una.")]);
        assert_eq!(cards(&of_nowhere(&[], Options::default()))[0], missing("No hay ninguna versión del juego en la carpeta."));
    }

    #[test]
    fn a_script_runs_in_the_debug_build_of_the_chosen_version_whatever_is_played() {
        let mut m = of_nowhere(&NAMES, Options::default());
        m.found.scripts = vec![tools::Script { name: "carga".into(), says: String::new() }, tools::Script { name: "hud".into(), says: String::new() }];
        m.script = 1;
        let line = |m: &Model| m.script_command().map(|(build, args)| format!("{} {}", build.path(), args.join(" ")));
        // (the demo is what is played; the script goes to the debug build of that same version)
        assert_eq!(m.chosen().map(Build::name).as_deref(), Some("V35 · DEMO"));
        assert_eq!(line(&m).as_deref(), Ok("LunaV35_debug.exe --guion tools/camara/hud.jsonc"));
        m.select(36);
        assert_eq!(line(&m).as_deref(), Ok("SeleneV36_debug.exe --guion tools/camara/hud.jsonc"));
        // a version without a debug build, and one too old to run scripts
        m.select(33);
        assert_eq!(line(&m), Err("La V33 no tiene edición DEBUG: los guiones solo se ejecutan en ella.".to_string()));
        m.select(21);
        assert_eq!(line(&m), Err("La V21 no tiene edición DEBUG: los guiones solo se ejecutan en ella.".to_string()));
        m.select(35);
        m.found.scripts.clear();
        assert_eq!(line(&m), Err("No hay guiones en tools/camara.".to_string()));
        // the check of the start is the chosen build's own
        m.select(34);
        assert_eq!(m.boot_command().map(|(build, _)| build.path()), Err("La V34 no sabe comprobar su arranque: hace falta la V35 o una posterior.".to_string()));
        m.select(35);
        assert_eq!(m.boot_command().map(|(build, args)| format!("{} {}", build.path(), args.join(" "))).as_deref(), Ok("LunaV35_demo.exe --prueba-arranque"));
    }

    #[test]
    fn choosing_an_edition_keeps_the_version_on_show() {
        let mut m = of_nowhere(&NAMES, Options::default());
        // (the newest demo is the V35; the newest debug, the V36: the V35 stays, now by its number)
        assert!(m.choose(Edition::Debug));
        assert_eq!((m.options.version, m.chosen().map(Build::name).as_deref()), (Some(35), Some("V35 · DEBUG")));
        assert!(m.choose(Edition::Multiplayer));
        assert_eq!((m.options.version, m.chosen().map(Build::name).as_deref()), (None, Some("V35 · MULTIJUGADOR")));
        assert!(!m.choose(Edition::Multiplayer), "the one chosen is not chosen again");
        // the edition asked is remembered across a version that lacks it
        m.select(34);
        assert_eq!((m.options.edition, m.chosen().map(Build::name).as_deref()), (Edition::Multiplayer, Some("V34 · DEMO")));
        m.select(35);
        assert_eq!((m.options.version, m.chosen().map(Build::name).as_deref()), (None, Some("V35 · MULTIJUGADOR")));
    }

    #[test]
    fn the_arrows_walk_the_list_and_stop_at_its_ends() {
        let mut m = of_nowhere(&NAMES, Options::default());
        assert_eq!(m.found.versions.iter().map(|v| v.number).collect::<Vec<_>>(), [36, 35, 34, 33, 21, 9]);
        m.reveal = false;
        let chosen = |m: &Model| m.version().map(|v| v.number);
        assert_eq!(chosen(&m), Some(35));
        m.step(1);
        assert_eq!((chosen(&m), m.reveal), (Some(34), true));
        m.step(-2);
        assert_eq!((chosen(&m), m.options.version), (Some(36), Some(36)));
        m.reveal = false;
        m.step(-1);
        assert_eq!((chosen(&m), m.reveal), (Some(36), false));
        m.step(100);
        assert_eq!(chosen(&m), Some(9));
        m.step(-4);
        assert_eq!((chosen(&m), m.options.version), (Some(35), None));
        // (with nothing to choose from, nothing happens)
        let mut none = of_nowhere(&[], Options::default());
        none.step(1);
        assert_eq!(none.version().map(|v| v.number), None);
    }

    /// A model of a folder whose "game" is the system's `cmd` (never the real game): what is
    /// started is `cmd /C exit N`, which says nothing, shows nothing and ends at once.
    #[cfg(windows)]
    fn of_the_shell(after_play: AfterPlay) -> (Model, String) {
        let shell = PathBuf::from(std::env::var("ComSpec").unwrap());
        let (root, file) = (shell.parent().unwrap().to_path_buf(), shell.file_name().unwrap().to_str().unwrap().to_string());
        (at(&root, &[], Options { after_play, ..Options::default() }), file)
    }

    /// Take what comes until what was started has ended.
    #[cfg(windows)]
    fn until_it_ends(m: &mut Model) {
        let limit = Instant::now() + Duration::from_secs(30);
        while m.busy().is_some() {
            assert!(Instant::now() < limit, "it never ended");
            if !m.pump() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    #[cfg(windows)]
    const GAME: What = What::Game { version: 1, edition: Edition::Demo };

    /// What `cmd` is told to last some `seconds` and end well, saying nothing (it asks this same
    /// machine whether it is there, once a second: nothing leaves it).
    #[cfg(windows)]
    fn lasting(seconds: u32) -> Vec<String> {
        vec!["/C".to_string(), format!("ping -n {} 127.0.0.1 >nul", seconds + 1)]
    }

    #[cfg(windows)]
    #[test]
    fn what_is_started_is_looked_after_until_it_ends() {
        let (mut m, shell) = of_the_shell(AfterPlay::Stay);
        let exit = |code: u32| vec!["/C".to_string(), format!("exit {code}")];
        // a game that ends well is counted, and how long it was is said
        m.start(&shell, &exit(0), "V1 · DEMO".to_string(), GAME);
        assert_eq!(m.busy(), Some("V1 · DEMO"));
        assert!(m.status.as_ref().is_some_and(|s| s.good && s.text == "V1 · DEMO en marcha"));
        assert_eq!(m.history.of(1, Edition::Demo).starts, 1);
        assert_eq!(m.history.last.as_ref().map(|l| l.seconds), Some(None));
        until_it_ends(&mut m);
        assert_eq!(m.status.as_ref().map(|s| (s.good, s.text.as_str())), Some((true, "V1 · DEMO: 0 s de partida.")));
        assert_eq!(m.history.last.as_ref().map(|l| (l.version, l.edition, l.seconds, l.code, l.good())), Some((1, Edition::Demo, Some(0), Some(0), Some(true))));
        // one that stops with an error is told, with its code if it left nothing written
        m.start(&shell, &exit(3), "V1 · DEMO".to_string(), GAME);
        until_it_ends(&mut m);
        let said = m.status.take().unwrap();
        assert!(!said.good && said.text.starts_with("V1 · DEMO se cerró con un error: código 3"), "{}", said.text);
        assert_eq!(m.history.of(1, Edition::Demo).starts, 2);
        assert_eq!(m.history.last.as_ref().map(|l| (l.code, l.good())), Some((Some(3), Some(false))));
        // a script that ends well has its pictures, and is no game played
        m.start(&shell, &exit(0), "El guion «carga»".to_string(), What::Script);
        assert!(m.status.as_ref().is_some_and(|s| s.text.starts_with("El guion «carga» está en marcha")));
        until_it_ends(&mut m);
        assert!(m.status.as_ref().is_some_and(|s| s.good && s.text.starts_with("El guion «carga» ha terminado")));
        assert_eq!(m.history.total().starts, 2);
        // (the window never left: the launcher was to stay)
        assert_eq!((m.presence(), m.next_wake()), (Presence::Shown, None));
    }

    #[cfg(windows)]
    #[test]
    fn one_thing_at_a_time() {
        let (mut m, shell) = of_the_shell(AfterPlay::Stay);
        m.start(&shell, &lasting(2), "V1 · DEMO".to_string(), GAME);
        assert_eq!(m.busy(), Some("V1 · DEMO"));
        m.start(&shell, &["/C".to_string(), "exit 0".to_string()], "El guion «carga»".to_string(), What::Script);
        assert_eq!(m.status.as_ref().map(|s| (s.good, s.text.as_str())), Some((false, "V1 · DEMO sigue en marcha: espera a que termine antes de iniciar otra cosa.")));
        m.act(Action::CheckBoot);
        assert_eq!(m.history.total().starts, 1);
        until_it_ends(&mut m);
        assert!(m.history.of(1, Edition::Demo).seconds >= 1, "{:?}", m.history);
    }

    #[cfg(windows)]
    #[test]
    fn the_options_and_what_is_played_are_kept_in_the_launcher_s_folder() {
        let home = std::env::temp_dir().join(format!("luna-launcher-modelo-fichero-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let shell = PathBuf::from(std::env::var("ComSpec").unwrap());
        let (root, file) = (shell.parent().unwrap().to_path_buf(), shell.file_name().unwrap().to_str().unwrap().to_string());
        let open = || Model::of(home.clone(), root.clone(), Vec::new(), Kept::load(&home), true, Arc::new(|| {}));
        // nothing changed, nothing written
        let mut m = open();
        m.save();
        assert!(!home.join(options::FILE).exists());
        // an option changed is kept, and read the next time
        m.options.after_play = AfterPlay::Stay;
        m.options.vsync = false;
        m.save();
        assert!(home.join(options::FILE).is_file());
        let mut m = open();
        assert_eq!((m.options.after_play, m.options.vsync), (AfterPlay::Stay, false));
        // a game played is counted there too, when it starts and when it ends
        m.start(&file, &["/C".to_string(), "exit 0".to_string()], "V1 · DEMO".to_string(), GAME);
        m.save();
        assert_eq!(open().history.of(1, Edition::Demo).starts, 1);
        until_it_ends(&mut m);
        m.save();
        let again = open();
        assert_eq!(again.history.last.as_ref().map(|l| (l.version, l.code, l.seconds.is_some())), Some((1, Some(0), true)));
        // what another launcher open at the same time kept meanwhile is not lost when this one's game ends
        let mut other = open();
        other.options.name = "Otra".to_string();
        other.history.started(7, Edition::Debug, 5);
        other.save();
        m.start(&file, &["/C".to_string(), "exit 0".to_string()], "V1 · DEMO".to_string(), GAME);
        until_it_ends(&mut m);
        m.save();
        let last = open();
        assert_eq!((last.history.of(7, Edition::Debug).starts, last.history.of(1, Edition::Demo).starts, last.options.name.as_str()), (1, 2, "Otra"));
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn the_window_gets_out_of_the_way_of_a_game_that_is_up_and_comes_back_when_it_ends() {
        let (mut m, shell) = of_the_shell(AfterPlay::Hide);
        m.start(&shell, &lasting(4), "V1 · DEMO".to_string(), GAME);
        // (it is not taken as up at once: a game that cannot start is gone in a moment)
        assert!(!m.pump() && m.presence() == Presence::Shown);
        let up = m.next_wake().expect("a moment to look again");
        assert!(up > Instant::now() && up <= Instant::now() + GRACE);
        std::thread::sleep(up - Instant::now());
        assert!(m.pump());
        assert_eq!((m.presence(), m.next_wake(), m.busy()), (Presence::Hidden, None, Some("V1 · DEMO")));
        assert!(m.status.is_none());
        until_it_ends(&mut m);
        assert_eq!(m.presence(), Presence::Shown);
        assert!(m.status.as_ref().is_some_and(|s| s.good && s.text.starts_with("V1 · DEMO: ") && s.text.ends_with(" de partida.")), "{:?}", m.status);
        assert!(m.history.of(1, Edition::Demo).seconds >= 3);
    }

    #[cfg(windows)]
    #[test]
    fn a_launcher_that_closes_does_so_once_the_game_is_up_and_not_if_it_fails() {
        let (mut m, shell) = of_the_shell(AfterPlay::Close);
        // one that fails at once: the launcher stays to tell
        m.start(&shell, &["/C".to_string(), "exit 2".to_string()], "V1 · DEMO".to_string(), GAME);
        until_it_ends(&mut m);
        assert_eq!((m.presence(), m.next_wake()), (Presence::Shown, None));
        assert!(m.status.as_ref().is_some_and(|s| !s.good));
        // one that stays up: the launcher is done, the game counted but not timed
        m.start(&shell, &lasting(3), "V1 · DEMO".to_string(), GAME);
        std::thread::sleep(GRACE + Duration::from_millis(50));
        assert!(m.pump());
        assert_eq!(m.presence(), Presence::Gone);
        assert_eq!(m.history.of(1, Edition::Demo).starts, 2);
        assert_eq!(m.history.last.as_ref().map(|l| l.seconds), Some(None));
        m.act(Action::Quit);
        assert_eq!(m.presence(), Presence::Gone);
    }

    #[cfg(windows)]
    #[test]
    fn what_cannot_be_started_is_said_and_nothing_is_looked_after() {
        let (mut m, _) = of_the_shell(AfterPlay::Hide);
        m.start("LunaV0_no_existe.exe", &[], "V0 · DEMO".to_string(), GAME);
        assert!(m.busy().is_none());
        assert!(m.status.as_ref().is_some_and(|s| !s.good && s.text.starts_with("No se pudo iniciar LunaV0_no_existe.exe: ")));
        assert!(!m.pump());
        assert_eq!((m.history.total().starts, m.next_wake()), (0, None));
        // (and with no build chosen, playing says so instead of starting anything)
        m.act(Action::Play);
        assert!(m.status.as_ref().is_some_and(|s| s.text.starts_with("No hay ninguna versión del juego")));
        m.act(Action::RunScript);
        m.act(Action::CheckBoot);
        assert!(m.status.as_ref().is_some_and(|s| s.text == "No hay ninguna versión del juego en la carpeta."));
        assert!(m.busy().is_none() && !m.checking());
    }

    /// A game's folder made for the test, with these builds (empty files: nothing is started).
    fn folder(test: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("luna-launcher-modelo-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for file in files {
            std::fs::write(root.join(file), "").unwrap();
        }
        root
    }

    #[test]
    fn old_versions_are_put_away_only_after_asking_and_can_be_brought_back() {
        let files = ["SeleneV36_demo.exe", "LunaV35_demo.exe", "LunaV34_demo.exe", "LunaV33_demo.exe", "LunaV33_debug.exe", "LunaV21.exe", "LunaLauncher.exe", "SeleneLauncher.exe"];
        let root = folder("guardar", &files);
        let mut m = at(&root, &files, Options { keep_versions: 3, ..Options::default() });
        let here = |m: &Model| m.found.versions.iter().filter(|v| !v.archived).map(|v| v.number).collect::<Vec<_>>();
        // asking moves nothing, and neither does saying no
        m.act(Action::AskArchive);
        assert_eq!(m.asking.as_ref().map(|plan| (plan.versions.clone(), plan.files.len())), Some((vec![33, 21], 3)));
        assert_eq!(here(&m), [36, 35, 34, 33, 21]);
        m.act(Action::Cancel);
        assert!(m.asking.is_none() && root.join("LunaV21.exe").is_file());
        // saying yes moves them: they are still in the list, put away
        m.act(Action::AskArchive);
        m.act(Action::Archive);
        assert!(m.asking.is_none());
        assert_eq!(m.status.as_ref().map(|s| (s.good, s.text.as_str())), Some((true, "Archivados en versiones_antiguas: 3 ejecutables.")));
        assert_eq!(here(&m), [36, 35, 34]);
        assert_eq!(m.found.versions.iter().filter(|v| v.archived).map(|v| v.number).collect::<Vec<_>>(), [33, 21]);
        assert!(root.join("versiones_antiguas/LunaV33_debug.exe").is_file() && !root.join("LunaV33_debug.exe").exists() && root.join("LunaLauncher.exe").is_file() && root.join("SeleneLauncher.exe").is_file());
        // one put away is still played: from where it is, in the game's folder
        m.select(33);
        m.options.edition = Edition::Debug;
        assert_eq!(m.command().map(|(build, _)| build.path()).as_deref(), Ok("versiones_antiguas/LunaV33_debug.exe"));
        // and it comes back whole
        m.act(Action::Restore(33));
        assert_eq!(m.status.as_ref().map(|s| (s.good, s.text.as_str())), Some((true, "La V33 vuelve a la carpeta del juego: 2 ejecutables.")));
        assert_eq!(here(&m), [36, 35, 34, 33]);
        assert_eq!(m.command().map(|(build, _)| build.path()).as_deref(), Ok("LunaV33_debug.exe"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_folder_is_looked_at_elsewhere_and_taken_in_when_it_is_done() {
        let files = ["SeleneV36_demo.exe", "LunaV35_demo.exe"];
        let root = folder("mirar", &files);
        std::fs::write(root.join("LEEME.txt"), "NOVEDADES V36:\nRED: la carga se comparte.\n\nNOVEDADES V35:\nANDAR: un pie tras otro.\n").unwrap();
        let mut m = at(&root, &[], Options::default());
        m.found.known.windows = Some("Windows 11".to_string());
        assert!(m.chosen().is_none() && m.news().is_none());
        m.rescan();
        let limit = Instant::now() + Duration::from_secs(30);
        while !m.pump() {
            assert!(Instant::now() < limit, "the look never ended");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(m.chosen().map(Build::name).as_deref(), Some("V36 · DEMO"));
        assert_eq!(m.news().map(|news| news.title.as_str()), Some("NOVEDADES V36"));
        // an older version shows its own news
        m.select(35);
        assert_eq!(m.news().map(|news| (news.title.as_str(), news.blocks[0].lead.as_str())), Some(("NOVEDADES V35", "ANDAR")));
        assert_eq!(m.found.known.windows.as_deref(), Some("Windows 11"), "what is known of the system is not asked again");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_versions_that_came_since_the_last_look_are_new_until_they_are_chosen() {
        let files = ["LunaV35_demo.exe", "LunaV34_demo.exe"];
        let root = folder("nuevas", &files);
        let look = |m: &mut Model| {
            m.found = scan::look(&m.root, Some(m.found.known.clone()));
            m.note_new();
        };
        // the first time the launcher ever looks, nothing is new: it just takes note
        let mut m = at(&root, &[], Options::default());
        look(&mut m);
        assert_eq!((m.fresh.clone(), m.options.seen), (Vec::new(), 35));
        // two versions come while it is open (or closed: what it had seen is kept with the options)
        std::fs::write(root.join("SeleneV36_demo.exe"), "").unwrap();
        std::fs::write(root.join("SeleneV37_debug.exe"), "").unwrap();
        look(&mut m);
        assert_eq!((m.fresh.clone(), m.options.seen), (vec![37, 36], 37));
        look(&mut m);
        assert_eq!(m.fresh, [37, 36], "told once");
        // one is new until it is chosen
        m.select(36);
        assert_eq!(m.fresh, [37]);
        let mut later = at(&root, &[], Options { seen: 35, ..Options::default() });
        look(&mut later);
        assert_eq!(later.fresh, [37, 36]);
        // (a version taken away makes nothing new, and what was seen stays seen)
        std::fs::remove_file(root.join("SeleneV37_debug.exe")).unwrap();
        look(&mut later);
        assert_eq!((later.fresh.clone(), later.options.seen), (vec![37, 36], 37));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Take what the work done elsewhere came to, once it is there.
    fn until_something_comes(m: &mut Model) {
        let limit = Instant::now() + Duration::from_secs(30);
        while !m.pump() {
            assert!(Instant::now() < limit, "nothing came");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn what_is_pasted_goes_into_the_address_or_to_whatever_is_being_typed_in() {
        let mut m = of_nowhere(&[], Options::default());
        // (the clipboard is not asked: what it would say is handed over as it would come)
        let pasted = |m: &mut Model, text: Result<&str, &str>, typed: bool| {
            let text = text.map(str::to_string).map_err(str::to_string);
            m.jobs.spawn(move || Message::Pasted { text, typed });
            until_something_comes(m);
        };
        pasted(&mut m, Ok("\r\n  192.168.1.67:47600  \r\nmás líneas\r\n"), false);
        assert_eq!((m.options.server.as_str(), m.typed.clone(), m.status.is_none()), ("192.168.1.67:47600", None, true));
        // with the keys, it goes to whatever has the keyboard, and the address stays
        pasted(&mut m, Ok("--ships 100\t--npcs 0\n"), true);
        assert_eq!((m.options.server.as_str(), m.typed.take().as_deref()), ("192.168.1.67:47600", Some("--ships 100--npcs 0")));
        // nothing to paste, and a system that does not say
        pasted(&mut m, Ok(" \r\n"), false);
        assert_eq!(m.status.take().map(|s| (s.good, s.text)), Some((false, "El portapapeles no tiene texto que pegar.".to_string())));
        pasted(&mut m, Err("no se pudo preguntar al sistema (PowerShell)"), true);
        assert_eq!(m.status.take().map(|s| s.text).as_deref(), Some("No se pudo pegar: no se pudo preguntar al sistema (PowerShell)."));
        assert_eq!((m.options.server.as_str(), m.typed.clone()), ("192.168.1.67:47600", None));
    }

    #[test]
    fn what_is_copied_is_what_is_on_show() {
        let mut m = of_nowhere(&["LunaV35_demo.exe", "LunaV35_multiplayer.exe"], Options { edition: Edition::Multiplayer, server: "luna.example:47611".to_string(), name: "Ana de la Luna".to_string(), ..Options::default() });
        assert_eq!(m.text(Text::CommandLine), Some(("LunaV35_multiplayer.exe --vsync on --pantalla completa --servidor luna.example:47611 --nombre \"Ana de la Luna\"".to_string(), "La línea de órdenes")));
        assert_eq!(m.text(Text::Check), None);
        m.check = Some(Check { name: "V35 · DEMO".into(), line: "x".into(), good: true, verdict: "Arranca bien (8,0 s).".into(), output: "arranque: bien".into() });
        assert_eq!(m.text(Text::Check).map(|(text, _)| text).as_deref(), Some("V35 · DEMO\nx\nArranca bien (8,0 s).\narranque: bien"));
        let (diagnostic, name) = m.text(Text::Diagnostic).unwrap();
        assert_eq!(name, "El diagnóstico");
        for part in [
            "SELENE — diagnóstico del launcher 3.",
            "Carpeta del juego: no/existe/esta/carpeta",
            "Versiones: 1 a mano (2 ejecutables",
            "Se inicia: LunaV35_multiplayer.exe --vsync on",
            "Última partida: ninguna desde este launcher",
            "Servidor local: no está en la carpeta",
            "Gráfica: desconocida",
        ] {
            assert!(diagnostic.contains(part), "{part}\n{diagnostic}");
        }
    }
}
