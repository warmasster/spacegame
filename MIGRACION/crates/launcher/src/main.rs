#![cfg_attr(not(test), windows_subsystem = "windows")]
//! The launcher: picks a build of the game next to it (which edition, which version), its
//! options, and starts it; and keeps what whoever plays or develops the game wants at hand.
//!
//! `luna-launcher` opens its window. It is drawn when something changes and at no other time (a
//! window nobody touches costs nothing); what takes time — looking at the game's folder, waiting
//! for a game to end — is done off its thread (`jobs`), so it is up at once and never waits.
//!
//! `luna-launcher --prueba FILE.png [--prueba-pestana versiones|opciones|multijugador|
//! herramientas|sistema] [--prueba-pagina guiones|comprobar|documentos|registros]
//! [--prueba-edicion demo|debug|multiplayer] [--prueba-version N] [--prueba-caso CASE[,CASE…]]
//! [--prueba-servidor ADDRESS] [--prueba-tamano WxH] [--prueba-reposo SECONDS]
//! [--prueba-ocultar SECONDS] [--prueba-api dx12|vulkan] [--prueba-informe FILE.txt]` is its own
//! test: it draws a few frames in a window that is never shown, leaves in FILE.png a picture of
//! the last one and exits (0: done), without starting the game and without reading or writing the
//! options' file. A CASE is one of `model::CASES`: a state to take the picture of. With
//! `--prueba-reposo` it then stays that long as a window left alone would, counting the frames it
//! draws (none, if all is well). With `--prueba-ocultar` it first goes through what it does when
//! a game is played: a stand-in for the game (the system's `cmd`, waiting that long with no
//! window) is started, the launcher's window and graphics device go, and come back when it ends;
//! the picture is of the window that came back. `--prueba-informe` leaves how long each part of
//! the start took, and those counts, in a text file.
//! There is no console: what goes wrong before there is a window is left in `launcher_error.log`.
mod archive;
mod builds;
mod clipboard;
mod gpu;
mod history;
mod jobs;
mod launch;
mod look;
mod model;
mod news;
mod options;
mod png;
mod scan;
mod server;
mod space;
mod store;
mod system;
mod tools;
mod versions;
mod view;
mod words;

use builds::{Edition, GAME};
use gpu::{Frame, Gpu};
use model::{Model, Presence, Tab};
use std::{
    error::Error,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tools::Page;
use view::View;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{Window, WindowButtons, WindowId},
};

/// The launcher's own version, as it is told (the builds have theirs).
pub const VERSION: &str = "3.0";
/// The window, in points: as it is wanted, and the least it is made on a small screen.
const SIZE: (f64, f64) = (1200.0, 720.0);
const LEAST: (f64, f64) = (1040.0, 640.0);
/// What is left of a screen round the window (its frame, the task bar), in points.
const ROOM: (f64, f64) = (40.0, 110.0);
/// The frames `--prueba` draws, once the game's folder has been looked at, before it takes its
/// picture.
const TEST_FRAMES: u32 = 12;
/// The least time between two looks at the game's folder as the window comes to the front.
const LOOK: Duration = Duration::from_secs(2);

/// What the command line asks for.
#[derive(Debug, Default, PartialEq)]
struct Args {
    /// `--prueba`: where the picture goes.
    picture: Option<PathBuf>,
    tab: Option<Tab>,
    page: Option<Page>,
    edition: Option<Edition>,
    version: Option<u32>,
    /// `--prueba-caso`: the states to put the launcher in.
    cases: Vec<String>,
    /// `--prueba-servidor`: the server's address as if it had been typed.
    server: Option<String>,
    /// `--prueba-tamano`: the window's size, in points.
    size: Option<(f64, f64)>,
    /// `--prueba-reposo`: how long to stay, left alone, after the picture.
    idle: Option<Duration>,
    /// `--prueba-informe`: where the times and the counts go.
    report: Option<PathBuf>,
    /// `--prueba-api vulkan`: the other graphics API tried first (the one the launcher falls back to).
    vulkan: bool,
    /// `--prueba-ocultar`: how long the stand-in for a game lasts, the launcher out of its way.
    hide: Option<u32>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let mut value = || rest.next().map(String::as_str).ok_or(format!("falta el valor de {flag}"));
        match flag.as_str() {
            "--prueba" => out.picture = Some(PathBuf::from(value()?)),
            "--prueba-pestana" => {
                out.tab = Some(match value()? {
                    // (the news are in the versions' tab: its name of before still asks for it)
                    "versiones" | "novedades" => Tab::Versions,
                    "opciones" => Tab::Options,
                    "multijugador" => Tab::Multiplayer,
                    "herramientas" => Tab::Tools,
                    "sistema" => Tab::System,
                    v => return Err(format!("pestaña desconocida: {v} (versiones, opciones, multijugador, herramientas, sistema)")),
                })
            }
            "--prueba-pagina" => {
                out.page = Some(match value()? {
                    "guiones" => Page::Scripts,
                    "comprobar" => Page::Check,
                    "documentos" => Page::Docs,
                    "registros" => Page::Logs,
                    v => return Err(format!("página desconocida: {v} (guiones, comprobar, documentos, registros)")),
                })
            }
            "--prueba-edicion" => {
                out.edition = Some(match value()? {
                    "demo" => Edition::Demo,
                    "multiplayer" => Edition::Multiplayer,
                    "debug" => Edition::Debug,
                    v => return Err(format!("edición desconocida: {v} (demo, debug, multiplayer)")),
                })
            }
            "--prueba-version" => out.version = Some(value()?.trim_start_matches(['V', 'v']).parse().map_err(|_| "--prueba-version: un número de versión".to_string())?),
            "--prueba-caso" => {
                for case in value()?.split(',').filter(|case| !case.is_empty()) {
                    if !model::CASES.contains(&case) {
                        return Err(format!("caso desconocido: {case} ({})", model::CASES.join(", ")));
                    }
                    out.cases.push(case.to_string());
                }
            }
            "--prueba-servidor" => out.server = Some(value()?.to_string()),
            "--prueba-tamano" => {
                let v = value()?;
                let size = v.split_once(['x', 'X']).and_then(|(w, h)| Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?)));
                out.size = Some(size.filter(|(w, h)| *w >= 640.0 && *h >= 400.0 && *w <= 7680.0 && *h <= 4320.0).ok_or(format!("--prueba-tamano: ANCHOxALTO, no '{v}'"))?);
            }
            "--prueba-reposo" => out.idle = Some(value()?.parse::<f32>().ok().filter(|s| (0.0..=600.0).contains(s)).map(Duration::from_secs_f32).ok_or("--prueba-reposo: segundos, de 0 a 600")?),
            "--prueba-informe" => out.report = Some(PathBuf::from(value()?)),
            "--prueba-ocultar" => out.hide = Some(value()?.parse::<u32>().ok().filter(|s| (1..=30).contains(s)).ok_or("--prueba-ocultar: segundos, de 1 a 30")?),
            "--prueba-api" => {
                out.vulkan = match value()? {
                    "vulkan" => true,
                    "dx12" => false,
                    v => return Err(format!("API desconocida: {v} (dx12, vulkan)")),
                }
            }
            _ => return Err(format!("opción desconocida: {flag}")),
        }
    }
    Ok(out)
}

/// The size of the launcher's window on a screen with that much room (points): as it is wanted,
/// less on a small screen, never less than the least it is laid out for.
fn window_size(screen: Option<(f64, f64)>) -> (f64, f64) {
    let Some((width, height)) = screen else { return SIZE };
    (SIZE.0.min(width - ROOM.0).max(LEAST.0), SIZE.1.min(height - ROOM.1).max(LEAST.1))
}

/// The window and all that draws on it.
struct State {
    window: Arc<Window>,
    gpu: Gpu,
    ctx: egui::Context,
    input: egui_winit::State,
    view: View,
    /// Something happened that the next frame must show.
    dirty: bool,
    /// When egui asked for another frame (something of its own is moving: a list scrolling).
    again: Option<Instant>,
    frames: u32,
    /// The window is on the screen (it is shown once there is something drawn on it).
    shown: bool,
    modifiers: ModifiersState,
    /// How long each part of opening took (milliseconds): the window, the graphics device, the
    /// lettering; and then the first frame.
    took: Vec<(&'static str, u32)>,
    /// How long the last frame took, all of it, and how long it took to lay it out and turn it
    /// into triangles, before any of it went to the graphics card (microseconds).
    last_us: u32,
    laid_us: u32,
}

impl State {
    fn new(event_loop: &ActiveEventLoop, test: bool, size: Option<(f64, f64)>, vulkan: bool) -> Result<State, Box<dyn Error>> {
        let mut clock = Instant::now();
        let mut took = Vec::new();
        let mut mark = |what: &'static str| {
            took.push((what, clock.elapsed().as_millis() as u32));
            clock = Instant::now();
        };
        let monitor = event_loop.primary_monitor().or_else(|| event_loop.available_monitors().next());
        let room = monitor.as_ref().map(|m| {
            let size = m.size().to_logical::<f64>(m.scale_factor());
            (size.width, size.height)
        });
        let (width, height) = size.unwrap_or_else(|| window_size(room));
        // (a test's window is never seen nor takes the focus: someone may be playing)
        let attributes = Window::default_attributes()
            .with_title(format!("{GAME} — Launcher"))
            .with_inner_size(LogicalSize::new(width, height))
            .with_resizable(false)
            .with_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE)
            .with_visible(false)
            .with_active(!test);
        let window = Arc::new(event_loop.create_window(attributes)?);
        if let Some(monitor) = window.current_monitor().or(monitor) {
            let (screen, at, size) = (monitor.size(), monitor.position(), window.outer_size());
            let middle = |from: i32, room: u32, taken: u32| from + (room as i32 - taken as i32) / 2;
            window.set_outer_position(PhysicalPosition::new(middle(at.x, screen.width, size.width), middle(at.y, screen.height, size.height)));
        }
        mark("ventana");
        let gpu = Gpu::new(window.clone(), test, vulkan)?;
        mark("grafica");
        let ctx = egui::Context::default();
        look::style(&ctx);
        if test {
            // nothing fades in or glides: what the picture is taken of is there at once
            ctx.all_styles_mut(|s| {
                s.animation_time = 0.0;
                s.scroll_animation = egui::style::ScrollAnimation::none();
            });
        }
        let input = egui_winit::State::new(ctx.clone(), egui::ViewportId::ROOT, window.as_ref(), Some(window.scale_factor() as f32), None, Some(gpu.max_texture_side()));
        mark("letras");
        took.extend_from_slice(gpu.took());
        Ok(State { window, gpu, ctx, input, view: View::default(), dirty: true, again: None, frames: 0, shown: false, modifiers: ModifiersState::empty(), took, last_us: 0, laid_us: 0 })
    }

    /// The screen the window is on, as it is told.
    fn screen(&self) -> Option<system::Screen> {
        let monitor = self.window.current_monitor()?;
        Some(system::Screen { width: monitor.size().width, height: monitor.size().height, scale: monitor.scale_factor() as f32, hertz: monitor.refresh_rate_millihertz().map(|mhz| mhz as f32 / 1000.0) })
    }
}

/// What `--prueba` is after, and how far it has got.
struct Test {
    picture: PathBuf,
    version: Option<u32>,
    cases: Vec<String>,
    /// The model has been put in the state the picture is of (once its folder was looked at).
    staged: bool,
    /// The frame the picture was taken at.
    pictured: Option<u32>,
    idle: Option<Duration>,
    /// When the time left alone ends.
    until: Option<Instant>,
    report: Option<PathBuf>,
    /// `--prueba-ocultar`: how long the stand-in for a game lasts.
    hide: Option<u32>,
}

struct App {
    model: Model,
    test: Option<Test>,
    /// `--prueba-tamano`.
    size: Option<(f64, f64)>,
    /// `--prueba-api vulkan`.
    vulkan: bool,
    state: Option<State>,
    /// What went wrong, if the launcher stopped for it.
    error: Option<String>,
    /// When the launcher started, and how long it was until its first frame (milliseconds).
    began: Instant,
    start_ms: Option<u32>,
    /// When the game's folder was last looked at because the window came to the front.
    looked: Instant,
    /// How many times the window has been opened (once; and once more each time it comes back
    /// after a game).
    opened: u32,
}

impl App {
    /// The window, opened: when the launcher starts, and when it comes back after a game.
    fn open(&mut self, event_loop: &ActiveEventLoop) {
        match State::new(event_loop, self.test.is_some(), self.size, self.vulkan) {
            Ok(s) => {
                self.opened += 1;
                self.model.facts.gpus = s.gpu.cards().to_vec();
                self.model.facts.screen = s.screen();
                self.state = Some(s);
            }
            Err(e) => {
                self.error = Some(format!("no se pudo abrir la ventana: {e}"));
                event_loop.exit();
            }
        }
    }

    /// One frame: the window laid out and drawn and — but in a test — what the player asked
    /// for done.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(s) = &mut self.state else { return };
        let clock = Instant::now();
        s.dirty = false;
        s.again = None;
        let size = s.window.inner_size();
        if size.width == 0 || size.height == 0 {
            // (minimised: nothing to draw on)
            return;
        }
        s.gpu.resize(size.width, size.height);
        let mut raw = s.input.take_egui_input(&s.window);
        // (what was pasted with the keys comes from the system when it comes: it is typed now)
        if let Some(text) = self.model.typed.take() {
            raw.events.push(egui::Event::Paste(text));
        }
        let (model, view) = (&mut self.model, &mut s.view);
        let mut action = None;
        let mut out = s.ctx.run_ui(raw, |ui| action = view.draw(ui, model));
        let acting = self.test.is_none();
        if acting {
            // (what is copied from a line that is typed in goes to the system's clipboard too)
            for command in &out.platform_output.commands {
                if let egui::OutputCommand::CopyText(text) = command {
                    let _ = clipboard::copy(text);
                }
            }
        }
        s.input.handle_platform_output(&s.window, std::mem::take(&mut out.platform_output));
        let primitives = s.ctx.tessellate(out.shapes, out.pixels_per_point);
        s.laid_us = clock.elapsed().as_micros() as u32;
        if let Err(e) = s.gpu.draw(&Frame { primitives: &primitives, textures: &out.textures_delta, pixels_per_point: out.pixels_per_point }) {
            self.error = Some(e.to_string());
            event_loop.exit();
            return;
        }
        s.frames += 1;
        let now = Instant::now();
        if self.start_ms.is_none() {
            self.start_ms = Some(now.duration_since(self.began).as_millis() as u32);
            self.model.facts.start_ms = self.start_ms;
        }
        if s.frames == 1 {
            s.took.push(("primer_fotograma", clock.elapsed().as_millis() as u32));
        }
        s.last_us = clock.elapsed().as_micros() as u32;
        // (egui asks again when something of its own is moving: a list gliding, a hover's hint)
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.repaint_delay);
        s.again = delay.filter(|d| *d < Duration::from_secs(3600)).map(|d| now + d.max(Duration::from_millis(8)));
        if acting {
            if !s.shown {
                s.window.set_visible(true);
                s.shown = true;
                s.dirty = true;
            }
            if let Some(action) = action {
                self.model.act(action);
                s.dirty = true;
            }
            self.model.save();
        }
    }

    /// The test's part of a turn of the loop: the model put in its state once its folder has been
    /// looked at; the picture, after its frames; the time left alone, and the report. Whether the
    /// test still draws without being asked (it does until its picture is taken).
    fn run_test(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let (Some(test), Some(s)) = (&mut self.test, &mut self.state) else { return false };
        if !self.model.ready {
            return false;
        }
        if !test.staged {
            test.staged = true;
            for case in &test.cases {
                self.model.stage(case);
            }
            if let Some(version) = test.version.filter(|v| self.model.found.versions.iter().any(|known| known.number == *v)) {
                self.model.select(version);
            }
            self.model.reveal = true;
            s.frames = 0;
            s.dirty = true;
            if let Some(seconds) = test.hide {
                if !self.model.stage_game(seconds) {
                    self.error = Some(format!("no se pudo iniciar el sustituto del juego ({seconds} s)"));
                    event_loop.exit();
                    return false;
                }
            }
        }
        // (while a stand-in for a game is up the launcher is on its way out, or out: the picture is
        // of the window that comes back)
        if test.hide.is_some() && (self.model.busy().is_some() || self.opened < 2) {
            return false;
        }
        if test.pictured.is_none() {
            if s.frames < TEST_FRAMES {
                return true;
            }
            match s.gpu.picture().and_then(|(width, height, rgb)| Ok(png::write(&test.picture, width, height, &rgb)?)) {
                Ok(()) => test.pictured = Some(s.frames),
                Err(e) => {
                    self.error = Some(format!("no se pudo guardar {}: {e}", test.picture.display()));
                    event_loop.exit();
                    return false;
                }
            }
            test.until = Some(Instant::now() + test.idle.unwrap_or_default());
        }
        // left alone from here on: whatever frames are drawn now, nobody asked for
        if test.until.is_some_and(|until| Instant::now() >= until) {
            if let Some(report) = &test.report {
                let pictured = test.pictured.unwrap_or(0);
                let parts: String = s.took.iter().map(|(what, ms)| format!("{what}_ms={ms}\n")).collect();
                let text = format!(
                    "inicio_ms={}\n{parts}carpeta_ms={}\nfotogramas={pictured}\nfotograma_us={}\nfotograma_sin_grafica_us={}\nreposo_s={}\nfotogramas_en_reposo={}\nventanas={}\ngrafica={}\n",
                    self.start_ms.unwrap_or(0),
                    self.model.found.took_ms,
                    s.last_us,
                    s.laid_us,
                    test.idle.unwrap_or_default().as_secs_f32(),
                    s.frames - pictured,
                    self.opened,
                    self.model.facts.gpus.iter().find(|gpu| gpu.used).map_or("?".to_string(), |gpu| format!("{} · {}", gpu.name, gpu.api))
                );
                if let Err(e) = std::fs::write(report, text) {
                    self.error = Some(format!("no se pudo guardar {}: {e}", report.display()));
                }
            }
            event_loop.exit();
        }
        false
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() && self.model.presence() == Presence::Shown {
            self.open(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(s) = &mut self.state else { return };
        match &event {
            WindowEvent::ModifiersChanged(modifiers) => s.modifiers = modifiers.state(),
            // Ctrl+V into a line that is typed in: the system is asked for its clipboard (the
            // launcher has none of its own to paste from), and what it says is typed when it comes
            WindowEvent::KeyboardInput { event: key, .. } if key.state == ElementState::Pressed && key.physical_key == PhysicalKey::Code(KeyCode::KeyV) && s.modifiers.control_key() && s.ctx.egui_wants_keyboard_input() => {
                if !key.repeat && self.test.is_none() {
                    self.model.paste(true);
                }
                return;
            }
            _ => {}
        }
        s.dirty |= s.input.on_window_event(&s.window, &event).repaint;
        match event {
            WindowEvent::CloseRequested => {
                self.model.save();
                event_loop.exit();
            }
            WindowEvent::Focused(focused) => {
                s.dirty = true;
                // (what may have changed while the launcher was behind: a new version put in the folder)
                if focused && self.test.is_none() && self.model.ready && self.looked.elapsed() >= LOOK {
                    self.looked = Instant::now();
                    self.model.rescan();
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } | WindowEvent::RedrawRequested => s.dirty = true,
            _ => {}
        }
    }

    /// What is done elsewhere and is ready wakes the loop: it is taken in `about_to_wait`.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {}

    /// Frames are drawn from here, and only when one is due: something changed (a key, the
    /// pointer, the work done elsewhere), or egui asked. The rest of the time the launcher
    /// sleeps: nothing is drawn, nothing is polled.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let changed = self.model.pump();
        match self.model.presence() {
            Presence::Gone => {
                self.model.save();
                event_loop.exit();
                return;
            }
            // (out of the way of the game: no window, no graphics device, nothing to draw)
            Presence::Hidden => {
                if self.state.take().is_some() {
                    self.model.save();
                }
            }
            Presence::Shown => {
                if self.state.is_none() {
                    self.open(event_loop);
                    self.looked = Instant::now();
                }
            }
        }
        let testing = self.run_test(event_loop);
        let Some(s) = &mut self.state else {
            event_loop.set_control_flow(self.model.next_wake().map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
            return;
        };
        s.dirty |= changed;
        let now = Instant::now();
        if testing || s.dirty || s.again.is_some_and(|t| now >= t) {
            self.frame(event_loop);
        }
        let Some(s) = &self.state else { return };
        let until = self.test.as_ref().and_then(|test| test.until);
        let wake = [s.again, self.model.next_wake(), until].into_iter().flatten().min();
        event_loop.set_control_flow(match wake {
            _ if testing || s.dirty => ControlFlow::Poll,
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }
}

/// With no console to say it in, what stopped the launcher is left in a file next to it.
fn report(text: &str) {
    let _ = std::fs::write(builds::home().join("launcher_error.log"), text);
}

fn main() {
    let began = Instant::now();
    std::panic::set_hook(Box::new(|info| report(&info.to_string())));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse(&args) {
        Ok(args) => args,
        Err(e) => {
            report(&e);
            std::process::exit(2);
        }
    };
    let event_loop = match EventLoop::<()>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            report(&e.to_string());
            std::process::exit(1);
        }
    };
    // what is done elsewhere wakes the loop when it is ready (the game's folder is being looked
    // at from here on, while the window and its graphics device are made)
    let proxy = Mutex::new(event_loop.create_proxy());
    let wake = Arc::new(move || {
        if let Ok(proxy) = proxy.lock() {
            let _ = proxy.send_event(());
        }
    });
    let testing = args.picture.is_some();
    let mut model = Model::load(!testing, wake);
    if let Some(edition) = args.edition {
        model.options.edition = edition;
    }
    if let Some(tab) = args.tab {
        model.tab = tab;
    }
    if let Some(page) = args.page {
        model.page = page;
    }
    if let Some(server) = args.server.filter(|_| testing) {
        model.options.server = server;
    }
    let test = args.picture.map(|picture| Test { picture, version: args.version, cases: args.cases, staged: false, pictured: None, idle: args.idle, until: None, report: args.report, hide: args.hide });
    let mut app = App { model, test, size: args.size.filter(|_| testing), vulkan: args.vulkan && testing, state: None, error: None, began, start_ms: None, looked: Instant::now(), opened: 0 };
    let run = event_loop.run_app(&mut app);
    if let Some(e) = app.error.or(run.err().map(|e| e.to_string())) {
        report(&e);
        std::process::exit(1);
    }
    if app.test.is_some_and(|test| test.pictured.is_none()) {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn no_arguments_is_the_launcher_itself() {
        assert_eq!(parse(&[]), Ok(Args::default()));
    }

    #[test]
    fn the_test_takes_its_picture_its_tab_and_its_edition() {
        let a = parse(&args("--prueba out/foto.png --prueba-pestana opciones --prueba-edicion debug")).unwrap();
        assert_eq!(a, Args { picture: Some(PathBuf::from("out/foto.png")), tab: Some(Tab::Options), edition: Some(Edition::Debug), ..Args::default() });
        assert_eq!(parse(&args("--prueba-pestana herramientas --prueba-pagina registros")).unwrap(), Args { tab: Some(Tab::Tools), page: Some(Page::Logs), ..Args::default() });
        assert_eq!(parse(&args("--prueba-pestana novedades --prueba-edicion demo")).unwrap(), Args { tab: Some(Tab::Versions), edition: Some(Edition::Demo), ..Args::default() });
        assert_eq!(parse(&args("--prueba-pestana versiones")).unwrap().tab, Some(Tab::Versions));
        assert_eq!(parse(&args("--prueba-pestana sistema")).unwrap().tab, Some(Tab::System));
        let a = parse(&args("--prueba out/multi.png --prueba-edicion multiplayer --prueba-pestana multijugador --prueba-servidor localhost")).unwrap();
        assert_eq!(a, Args { picture: Some(PathBuf::from("out/multi.png")), tab: Some(Tab::Multiplayer), edition: Some(Edition::Multiplayer), server: Some("localhost".to_string()), ..Args::default() });
    }

    #[test]
    fn the_test_takes_its_version_its_states_its_size_and_its_time_left_alone() {
        let a = parse(&args("--prueba a.png --prueba-version V34 --prueba-caso v36,historial --prueba-caso guardar --prueba-tamano 1040x640 --prueba-reposo 2.5 --prueba-informe out/a.txt --prueba-api vulkan --prueba-ocultar 3")).unwrap();
        assert_eq!(
            a,
            Args {
                picture: Some(PathBuf::from("a.png")),
                version: Some(34),
                cases: vec!["v36".to_string(), "historial".to_string(), "guardar".to_string()],
                size: Some((1040.0, 640.0)),
                idle: Some(Duration::from_millis(2500)),
                report: Some(PathBuf::from("out/a.txt")),
                vulkan: true,
                hide: Some(3),
                ..Args::default()
            }
        );
        assert_eq!(parse(&args("--prueba-version 9")).unwrap().version, Some(9));
        for case in model::CASES {
            assert_eq!(parse(&args(&format!("--prueba-caso {case}"))).unwrap().cases, [case]);
        }
    }

    #[test]
    fn what_is_not_understood_is_said() {
        assert!(parse(&args("--prueba")).unwrap_err().contains("falta el valor"));
        assert!(parse(&args("--prueba-pestana graficos")).unwrap_err().contains("pestaña desconocida"));
        assert!(parse(&args("--prueba-pagina mcp")).unwrap_err().contains("página desconocida"));
        assert!(parse(&args("--prueba-edicion release")).unwrap_err().contains("edición desconocida"));
        assert!(parse(&args("--prueba-version nueva")).unwrap_err().contains("--prueba-version"));
        assert!(parse(&args("--prueba-caso v36,imposible")).unwrap_err().starts_with("caso desconocido: imposible (v36, nueva, historial, fallo, "));
        assert!(parse(&args("--prueba-tamano 10x10")).unwrap_err().contains("ANCHOxALTO"));
        assert!(parse(&args("--prueba-tamano grande")).unwrap_err().contains("ANCHOxALTO"));
        assert!(parse(&args("--prueba-reposo -1")).unwrap_err().contains("--prueba-reposo"));
        assert!(parse(&args("--prueba-ocultar 0")).unwrap_err().contains("--prueba-ocultar"));
        assert!(parse(&args("--prueba-api metal")).unwrap_err().contains("API desconocida"));
        assert!(parse(&args("--jugar")).unwrap_err().contains("opción desconocida"));
    }

    #[test]
    fn the_window_is_as_wanted_or_as_large_as_the_screen_lets_it() {
        assert_eq!(window_size(None), SIZE);
        assert_eq!(window_size(Some((1920.0, 1080.0))), SIZE);
        assert_eq!(window_size(Some((2560.0, 1440.0))), SIZE);
        // (a laptop's screen: what it leaves, down to the least the launcher is laid out for)
        assert_eq!(window_size(Some((1366.0, 768.0))), (1200.0, 658.0));
        assert_eq!(window_size(Some((1280.0, 720.0))), (1200.0, 640.0));
        assert_eq!(window_size(Some((1024.0, 600.0))), LEAST);
    }
}
