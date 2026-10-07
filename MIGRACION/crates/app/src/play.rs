//! Window, input and the frame loop (play, bench, screenshot).
use crate::{
    aboard::Aboard,
    air::AirFx,
    bench::{self, Bench},
    blasts::Blasts,
    builds::Builds,
    cli::Options,
    content::{Defs, Models},
    editor::Editor,
    gear::Gear,
    hands::Hands,
    hud::{Card, Gauge, Icon, Level, Prompt},
    input::{self, Action},
    inspector::Inspector,
    perf::{self, Perf},
    pilot::{Controls, Input, Pilot},
    rangefinder::Rangefinder,
    script::Script,
    ships::Ships,
    spawner::Spawner,
    ui::{Info, Ui},
    visibility::Visibility,
    world::{Counts, World},
};
use lunar_controls::Mods;
use lunar_core::quality::{Preset, Settings};
use lunar_render::{FrameInput, Renderer};
use std::{error::Error, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Fullscreen, Window, WindowId},
};

/// Right-button zoom: how much narrower the view gets.
const ZOOM: f64 = 3.5;

pub fn run(opts: Options) -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App { opts, state: None, error: None, starting: None, test_frames: 0 };
    event_loop.run_app(&mut app)?;
    match app.error {
        Some(e) => Err(e.into()),
        None => Ok(()),
    }
}

struct App {
    opts: Options,
    state: Option<State>,
    error: Option<String>,
    /// The game starting up: its screen, and the thread making the game.
    starting: Option<Starting>,
    /// `--prueba-arranque`: frames of the game made since it came up.
    test_frames: u32,
}

/// The game starting up. The game is made on a thread of its own (its window not shown yet)
/// while this one shows the start-up screen in a window of its own, with a small GPU device of
/// its own: nothing the player does to it waits on the loading, and nothing of the loading
/// waits on it.
struct Starting {
    window: Arc<Window>,
    gpu: crate::splash_gpu::Gpu,
    ctx: egui::Context,
    splash: crate::splash::Splash,
    boot: crate::boot::Boot,
    last: Instant,
    loader: Option<std::thread::JoinHandle<Result<State, String>>>,
    /// The game made, waiting for the screen to show its end.
    ready: Option<State>,
}

/// Where the times of the last start are kept.
fn boot_times() -> std::path::PathBuf {
    crate::root().join("out/arranque.json")
}

/// The game's window as the options say; `shown`: on the desktop from the start.
fn window_attributes(o: &Options, shown: bool) -> winit::window::WindowAttributes {
    let (w, h) = o.window.unwrap_or((1600, 900));
    Window::default_attributes().with_title(crate::GAME).with_inner_size(winit::dpi::PhysicalSize::new(w, h)).with_visible(shown).with_active(shown)
}

impl Starting {
    /// A frame of the start-up screen.
    fn draw(&mut self) -> Result<(), Box<dyn Error>> {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;
        let size = self.window.inner_size();
        let snap = self.boot.snapshot();
        let (prims, textures) = splash_frame(&self.ctx, &mut self.splash, &snap, dt, (size.width, size.height));
        self.gpu.draw(&crate::splash_gpu::Frame { primitives: &prims, textures: &textures, pixels_per_point: 1.0 })
    }
}

/// A frame of the start-up screen laid out for a window of `size` px.
fn splash_frame(ctx: &egui::Context, splash: &mut crate::splash::Splash, snap: &crate::boot::Snapshot, dt: f32, size: (u32, u32)) -> (Vec<egui::ClippedPrimitive>, egui::TexturesDelta) {
    let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0.max(1) as f32, size.1.max(1) as f32))), ..Default::default() };
    ctx.set_pixels_per_point(1.0);
    let out = ctx.run_ui(input, |ui| {
        let ctx = ui.ctx().clone();
        splash.frame(&ctx, snap, dt, crate::BUILD);
    });
    (ctx.tessellate(out.shapes, out.pixels_per_point), out.textures_delta)
}

/// `--prueba-carga`: the start-up screen at a few moments of a start, as pictures, with no
/// window shown and nothing loaded.
fn splash_pictures(window: Arc<Window>, dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    use crate::boot::{STAGES, Snapshot, Stage};
    let size = window.inner_size();
    let mut gpu = crate::splash_gpu::Gpu::new(window, true)?;
    let ctx = egui::Context::default();
    crate::hud::style(&ctx);
    std::fs::create_dir_all(dir)?;
    // (so many stages done, the next under way; the last picture: everything in)
    for (n, done) in [0usize, 2, 4, STAGES.len()].into_iter().enumerate() {
        let stages: Vec<Stage> = (0..STAGES.len()).map(|k| if k < done { Stage::Done(STAGES[k].secs) } else if k == done { Stage::Running(0.6) } else { Stage::Waiting }).collect();
        let expect: Vec<f32> = STAGES.iter().map(|s| s.secs).collect();
        let secs = STAGES[..done].iter().map(|s| s.secs).sum::<f32>() + 0.6;
        let snap = Snapshot { done: crate::boot::share(&stages, &expect), secs, finished: done == STAGES.len(), stages };
        let mut splash = crate::splash::Splash::default();
        // (what is shown settled; at the end, a moment with everything done)
        let frames = (0..90).map(|_| Snapshot { finished: false, ..snap.clone() }).chain((0..10).filter(|_| snap.finished).map(|_| snap.clone()));
        for snap in frames {
            let (prims, textures) = splash_frame(&ctx, &mut splash, &snap, 1.0 / 60.0, (size.width, size.height));
            gpu.draw(&crate::splash_gpu::Frame { primitives: &prims, textures: &textures, pixels_per_point: 1.0 })?;
        }
        let (w, h, rgb) = gpu.picture()?;
        lunar_render::capture::write_png(&dir.join(format!("carga_{}.png", n + 1)), w, h, &rgb)?;
    }
    Ok(())
}

struct State {
    window: Arc<Window>,
    renderer: Renderer,
    ui: Ui,
    pilot: Pilot,
    defs: Defs,
    models: Models,
    world: World,
    blasts: Blasts,
    builds: Builds,
    ships: Ships,
    aboard: Aboard,
    spawner: Spawner,
    inspector: Inspector,
    /// The keys held, each in the place the table of keys gives it (`input::held`).
    keys: [bool; input::HELD],
    jump: bool,
    captured: bool,
    last: Instant,
    started: Instant,
    title_at: Instant,
    title_frames: u32,
    fps: f32,
    frame_ms: f32,
    bench: Option<Bench>,
    scenarios: Vec<usize>,
    reports: Vec<String>,
    counts: Counts,
    preset_name: String,
    adapter: String,
    /// Right button held: the view narrows (0 none .. 1 full zoom).
    zooming: bool,
    zoom: f64,
    /// A camera script running (`--guion`).
    script: Option<Script>,
    /// The air of the ships: its look, its pull, its shake.
    air: AirFx,
    /// T: distance to what the crosshair is on.
    range: Rangefinder,
    /// What structures are seen from the eye (the rest are not drawn).
    vis: Visibility,
    /// What each part of the frame costs, and its recording when a script asks.
    perf: Perf,
    /// What the suit carries in its hands.
    gear: Gear,
    /// Other players (`--servidor`), and what their bodies are made from.
    multi: Option<crate::multi::Multi>,
    body_source: Option<crate::multi::BodySource>,
    /// The player's own body (if its model is there), and what of it is drawn this frame.
    body: Option<crate::body::Body>,
    figures: lunar_core::anim::BodyScene,
    /// Bare hands on what is loose.
    hands: Hands,
    /// What the body's hands do with no tool in them: the ships' controls, the seat's stick and
    /// throttle, gestures, the wrist computer (`handwork`). And how far the look turns to read
    /// the wrist computer, as last frame's body had it.
    handwork: crate::handwork::Own,
    wrist_look: Option<(f64, f64)>,
    sounds: crate::sound::Sounds,
    dust: crate::dust::Dust,
    /// What touches loose ground leaves its mark there: boots, landing pads, cargo, jets.
    prints: crate::footprints::Footprints,
    /// The exhaust of whatever fires: engines, reaction control jets, the suit's pack.
    plumes: crate::plumes::Plumes,
    /// The helmet's visor: its filter for welding, the breath on it.
    visor: crate::visor::Visor,
    /// The references one finds one's way by, and what the compass shows of them.
    nav: crate::nav::Nav,
    /// What the ships' sensors are told and what their weapons let fly (`tactics`).
    tactics: crate::tactics::Tactics,
    /// A picture asked for with F12: taken with the next frame.
    photo: Option<std::path::PathBuf>,
    /// What a script aims at: (ship, control).
    script_aim: Option<(u64, usize)>,
    /// Fire a script keeps up at the ships: (shot, rounds a second, seconds left, rounds owed, seed).
    barrage: Option<(String, f64, f64, f64, u64)>,
    /// F6: the ship editor; the mouse where it is (px) and the last view (for its rays).
    editor: Editor,
    cursor: (f64, f64),
    last_view: Option<lunar_render::View>,
    /// The player's own view last frame, turned to what the picture's middle was on (the same
    /// as the picture's unless it is taken from outside): what a click acts along.
    last_aim: Option<lunar_render::View>,
    /// V: the view from outside the body.
    chase: crate::chase::Chase,
    /// Alt held: the look turned away from where the body faces.
    look: crate::freelook::FreeLook,
    /// The start menu, while it is up.
    start: Option<crate::start::Start>,
    /// Leave the game (the menu's).
    quit: bool,
}

/// The jet pack's gas (share) under which it warns.
const LOW_GAS: f32 = 0.15;

impl State {
    /// The game, made for `window` (which need not be shown yet: this may run on a thread of its
    /// own). `boot` is told as each stage of it is done.
    fn new(window: Arc<Window>, screen: lunar_render::Screen, o: &Options, boot: &crate::boot::Boot) -> Result<State, Box<dyn Error>> {
        let benching = o.bench.is_some();
        let root = crate::root();
        let lap = |what: &str| boot.done(what);
        let defs = Defs::load(&root.join("assets/defs"))?;
        lap("defs");
        let preset_opt = o.preset;
        let vsync = o.vsync.unwrap_or(!benching);
        let mut chosen = None;
        let mut renderer = Renderer::new(screen, &defs.system, |name, integrated| {
            let p = preset_opt.unwrap_or_else(|| Preset::for_adapter(name, integrated));
            chosen = Some(p);
            let mut s = Settings::preset(p);
            s.vsync = vsync;
            s
        })?;
        let preset = chosen;
        lap("render");
        // (a script's pictures are taken whole, whatever else the GPU is busy with)
        if o.script.is_some() {
            renderer.settings.dynamic_resolution = false;
        }
        let models = Models::register(&mut renderer, &defs, &root.join("assets"))?;
        lap("modelos");
        let sc = &defs.scenario;
        let mut scenarios = o.scenarios.clone();
        let (ships, npcs) = if scenarios.is_empty() { (o.ships.unwrap_or(sc.fleet.count), o.npcs.unwrap_or(sc.crowd.count)) } else { (scenarios[0], scenarios[0]) };
        if !scenarios.is_empty() {
            scenarios.remove(0);
        }
        let share = |n: usize| (n as f64 * sc.fleet.flying_share).floor() as usize;
        let flying = if o.scenarios.is_empty() { o.flying.unwrap_or(share(ships)) } else { share(ships) };
        let counts = Counts { ships, flying, npcs, radius: o.radius.unwrap_or(sc.layout.radius) };
        let world = World::new(&mut renderer, &defs, &models, counts, o.bench.is_none() && scenarios.is_empty())?;
        lap("mundo");
        let mut blasts = Blasts::new(&defs.effects, lunar_core::missiles::Missiles::new(defs.missiles.clone()), renderer.particle_capacity())?;
        blasts.set_guided(lunar_core::guided::Flight::load(&root.join("assets/defs"))?)?;
        if let Some(e) = &o.explode {
            // ID or ID@metres ahead
            let (id, m) = e.split_once('@').map_or((e.as_str(), 40.0), |(id, m)| (id, m.parse().unwrap_or(40.0)));
            blasts.fire_ahead(id, m);
        }
        renderer.set_particle_styles(&blasts.fx.particles.styles);
        let effect_ids: Vec<&str> = defs.effects.explosions.iter().map(|(id, _)| id.as_str()).collect();
        let mut builds = Builds::new(defs.structures.clone(), &defs.scenario, &world.site, &world.bodies, &effect_ids)?;
        let (font, atlas) = &defs.font;
        renderer.set_font_atlas(font.width, font.height, atlas);
        let (size, layers, data) = &defs.finishes;
        renderer.set_finishes(*size, *layers, data);
        let (atlas, pixels) = &defs.decals;
        renderer.set_decal_atlas(atlas.size, pixels);
        let mut gear = Gear::load(&root.join("assets/defs/gear.jsonc"), font.clone())?;
        gear.models(&defs.structures.catalog.models, &mut renderer)?;
        lap("equipo");
        // the player's body: its rig and its model
        let mut rig_def: Option<crate::rig::RigDef> = None;
        let (body, body_source) = {
            let def = root.join("assets/defs/rigs/astronauta.jsonc");
            let made = if def.exists() {
                let def: crate::rig::RigDef = lunar_core::defs::load(&def).map_err(|e| format!("{}: {}", e.file, e.message))?;
                let path = root.join("assets/models").join(format!("{}.glb", def.modelo));
                if path.exists() {
                    let model = crate::rig::Rigged::load(&path)?;
                    let seen = renderer.body_mesh(&model.without(&def.oculto));
                    let whole = renderer.body_mesh(&model.mesh);
                    // (the others' bodies are made from the same, each its own)
                    let rig = crate::rig::Rig::new(def.clone(), &model)?;
                    let source = crate::multi::BodySource { rig: rig.clone(), whole };
                    rig_def = Some(def);
                    Some((crate::body::Body::new(rig, seen, whole), source))
                } else {
                    None
                }
            } else {
                None
            };
            made.unzip()
        };
        // what its hands do with no tool in them (the ships' controls, gestures, the wrist computer)
        let handwork = crate::handwork::Own::load(&root.join("assets/defs"), rig_def.as_ref().filter(|_| body.is_some()), &defs.structures.catalog.models, font.clone(), &mut renderer)?;
        let script = match &o.script {
            Some(p) => Some(Script::load(&if p.is_absolute() { p.clone() } else { root.join(p) })?),
            None => None,
        };
        let wanted = |kind: &str| script.as_ref().and_then(|s| s.only.as_ref()).is_none_or(|only| only.iter().any(|k| k == kind));
        let mut fleet = Ships::new(defs.ships.clone(), font.clone());
        crate::tactics::Tactics::check(&fleet, &blasts)?;
        for a in defs.scenario.ships.iter().filter(|a| wanted(&a.ship)) {
            fleet.spawn(&mut builds, &world.bodies, &a.ship, world.site.body, world.site.at(a.east, a.north), a.yaw.to_radians())?;
        }
        lap("naves");
        // with others: the server asked to let us in (it answers while we play)
        let multi = match &o.server {
            Some(addr) => Some(crate::multi::Multi::connect(addr, o.name.as_deref().unwrap_or("Jugador"), &fleet)?),
            None => None,
        };
        let preset_name = preset.map_or("personalizado".into(), |p| p.name().to_string());
        let player = sc.player;
        let bench = o.bench.map(|secs| Bench::new(secs, format!("{ships}+{npcs}"), &world.crowd.home, world.site, world.bodies.clone(), (player.fov.to_radians(), player.near)));
        let controls = Controls::new(&player);
        let mut ui = Ui::new(&window, renderer.settings.clone(), preset, ships - flying.min(ships), flying, npcs, renderer.sun(), controls);
        ui.shots = blasts.editable();
        if o.script.is_some() {
            ui.instant();
        }
        let renderer_name = renderer.adapter().to_string();
        let pilot = Pilot::new(world.bodies.clone(), &world.site, player);
        let spawner = Spawner::new(&fleet, &builds, crate::DEMO);
        let air = AirFx::new(&blasts.fx);
        let mut dust = crate::dust::Dust::new(&blasts.fx);
        let prints = crate::footprints::Footprints::load(&root.join("assets/defs/huellas.jsonc"), &mut dust, &mut renderer)?;
        let plumes = crate::plumes::Plumes::new(&blasts.fx, &renderer)?;
        let visor = crate::visor::Visor::load(&root.join("assets/defs/visor.jsonc"))?;
        let nav = crate::nav::Nav::load(&root.join("assets/defs/navegacion.jsonc"))?;
        // (pictures and benches are taken in silence)
        let sounds = crate::sound::Sounds::new(&root, o.mute || o.script.is_some() || o.bench.is_some() || o.shot.is_some())?;
        ui.sound = sounds.status.clone();
        lap("sonido");
        // the start menu: what a game opens with (a script, a bench, a picture go straight in)
        let straight = o.no_menu || o.script.is_some() || o.bench.is_some() || o.shot.is_some() || o.look.is_some() || o.explode.is_some();
        let start = (!straight).then(|| {
            let mut st = crate::start::Start::new(&fleet, &builds.set, false);
            if let Some(addr) = &o.server {
                st.server = addr.clone();
            }
            if let Some(name) = &o.name {
                st.name = name.clone();
            }
            st
        });
        let now = Instant::now();
        Ok(State {
            window,
            renderer,
            ui,
            pilot,
            defs,
            models,
            world,
            blasts,
            builds,
            ships: fleet,
            aboard: Aboard::default(),
            spawner,
            inspector: Inspector::default(),
            keys: [false; input::HELD],
            jump: false,
            captured: false,
            last: now,
            started: now,
            title_at: now,
            title_frames: 0,
            fps: 0.0,
            frame_ms: 0.0,
            bench,
            scenarios,
            reports: Vec::new(),
            counts,
            preset_name,
            adapter: renderer_name,
            zooming: false,
            zoom: 0.0,
            script,
            air,
            range: Rangefinder::default(),
            nav,
            vis: Visibility::default(),
            perf: Perf::default(),
            barrage: None,
            gear,
            multi,
            body_source,
            body,
            figures: lunar_core::anim::BodyScene::default(),
            hands: Hands::new(player.manos),
            handwork,
            wrist_look: None,
            sounds,
            dust,
            prints,
            plumes,
            visor,
            tactics: crate::tactics::Tactics::default(),
            photo: None,
            script_aim: None,
            editor: Editor::default(),
            cursor: (0.0, 0.0),
            last_view: None,
            last_aim: None,
            chase: crate::chase::Chase::default(),
            look: crate::freelook::FreeLook::default(),
            start,
            quit: false,
        })
    }

    fn new_world(&mut self, counts: Counts) -> Result<(), Box<dyn Error>> {
        self.counts = counts;
        self.world = World::new(&mut self.renderer, &self.defs, &self.models, counts, self.bench.is_none())?;
        Ok(())
    }

    fn capture_mouse(&mut self, capture: bool) {
        if capture {
            let ok = self.window.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined)).is_ok();
            if !ok {
                return;
            }
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        }
        self.captured = capture;
        self.window.set_cursor_visible(!capture);
    }

    fn mods(&self) -> Mods {
        let held = |a: Action| input::held(a).is_some_and(|i| self.keys[i]);
        Mods { coarse: held(Action::Run), fine: held(Action::Down) }
    }

    /// An action held or let go (a key, or a script in its place).
    fn hold(&mut self, a: Action, on: bool) {
        if let Some(i) = input::held(a) {
            self.keys[i] = on;
        }
    }

    /// What the Controls tab lists besides the table of keys: the tools of the suit, the keys of
    /// the seats of the ship at hand (the one you are on, or the nearest) and the test shots'.
    fn menu_lists(&mut self) {
        let at = self.pilot.position;
        let near = self.pilot.ride.and_then(|r| self.ships.by_structure(r.id)).or_else(|| {
            let d = |n: usize| self.builds.set.get(self.ships.list[n].structure).map(|s| (n, s.to_world(s.center).distance_squared(at)));
            (0..self.ships.list.len()).filter_map(d).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0)
        });
        self.ui.seat_keys = near.map(|n| crate::aboard::seat_keys(&self.ships.list[n].kind)).unwrap_or_default();
        self.ui.tool_keys = self.gear.tools.iter().enumerate().map(|(k, t)| ((k + 1).to_string(), format!("{} — {}", t.nombre, self.gear.help(k)))).collect();
        self.ui.test_keys = if crate::DEMO { String::new() } else { self.blasts.help() };
    }

    /// Out of the start menu and into play, at the place chosen in it.
    fn begin(&mut self) {
        let Some(st) = self.start.take() else { return };
        let place = st.place().unwrap_or(crate::start::Where::Here);
        if place != crate::start::Where::Here && self.pilot.seat.is_some() {
            self.aboard.stand(&mut self.pilot, &mut self.ships, &self.builds.set);
        }
        match place {
            crate::start::Where::Here => {}
            crate::start::Where::Spawn => self.pilot.reset(),
            // on the ground astern of it (where ramps are), looking at it
            crate::start::Where::Beside(id) => {
                // (on the ground under it, where it has any)
                if let Some((s, b)) = self.builds.set.get(id).and_then(|s| Some((s, self.world.bodies.get(self.world.bodies.field(s.to_world(s.center)).ground?)))) {
                    let centre = s.to_world(s.center);
                    let up = b.up(centre);
                    let aft = (s.rot * glam::Vec3::NEG_Z).as_dvec3();
                    let aft = (aft - up * aft.dot(up)).normalize_or(up.any_orthonormal_vector());
                    let feet = b.above_ground(b.up(centre + aft * (f64::from(s.radius) + 2.0)), 0.0);
                    self.pilot.put(feet, b.up(feet));
                    self.pilot.look_at(centre);
                }
            }
            crate::start::Where::Seat(structure, seat) => {
                self.aboard.aim = Some(crate::aboard::Aim { structure, target: crate::aboard::Target::Seat(seat) });
                self.aboard.use_key(&mut self.pilot, &self.ships, &self.builds.set);
            }
        }
        self.keys.fill(false);
        self.jump = false;
        self.ui.menu = false;
        self.capture_mouse(true);
    }

    /// The start menu up over the game as it is (from the menu: the game goes on behind it).
    fn to_start(&mut self) {
        self.ui.menu = false;
        self.start = Some(crate::start::Start::new(&self.ships, &self.builds.set, true));
        self.keys.fill(false);
        self.capture_mouse(false);
    }

    fn key(&mut self, key: KeyCode, pressed: bool, repeat: bool) {
        // the start menu: its own few keys (none while something is being written in it)
        if self.start.is_some() {
            if !pressed || self.ui.ctx.egui_wants_keyboard_input() {
                return;
            }
            match key {
                KeyCode::Escape if !repeat => {
                    self.ui.menu = !self.ui.menu;
                    if self.ui.menu {
                        self.menu_lists();
                    }
                }
                _ if self.ui.menu => {}
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space if !repeat => self.begin(),
                KeyCode::ArrowUp | KeyCode::KeyW => self.start.iter_mut().for_each(|s| s.step(-1)),
                KeyCode::ArrowDown | KeyCode::KeyS => self.start.iter_mut().for_each(|s| s.step(1)),
                KeyCode::F11 if !repeat => self.window.set_fullscreen(if self.window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) }),
                _ => {}
            }
            return;
        }
        // seated, the seat's keys drive its controls
        if !repeat && self.aboard.key(key, pressed, &self.pilot, &mut self.ships, &self.builds.set) {
            return;
        }
        // what a key does is in the table of keys (`input`), the one the Controls tab shows
        let action = input::action(key, crate::DEMO);
        if let Some(a) = action {
            self.hold(a, pressed);
        }
        // (E, held, is also the roll right of whoever floats with the pack: a seat aimed at
        // takes it first, below)
        if action == Some(Action::Use) {
            self.hold(Action::RollRight, pressed);
        }
        if !pressed {
            self.blasts.release(key);
        }
        if !pressed || repeat {
            return;
        }
        if self.pilot.seat.is_some() && action == Some(Action::Jump) {
            self.aboard.stand(&mut self.pilot, &mut self.ships, &self.builds.set);
            self.keys.fill(false);
            return;
        }
        if action == Some(Action::Use) && self.aboard.use_key(&mut self.pilot, &self.ships, &self.builds.set) {
            self.keys.fill(false);
            return;
        }
        if !crate::DEMO && self.blasts.key(key) {
            return;
        }
        let Some(action) = action else { return };
        match action {
            Action::Jump => self.jump = true,
            // (the gestures' wheel open: the numbers choose in it)
            Action::Tool(k) if self.handwork.number(usize::from(k)) => {}
            Action::Wrist => match self.handwork.toggle_wrist() {
                Some(true) => self.ui.hud.notice("muñeca", "Ordenador de muñeca · Clic: pasar página", Level::Normal, 2.5),
                Some(false) => {}
                None => self.ui.hud.notice("muñeca", "Este traje no lleva ordenador de muñeca", Level::Caution, 2.5),
            },
            Action::Tool(k) => {
                if self.gear.pick(usize::from(k)) {
                    let text = self.gear.tool().map_or_else(|| "Manos libres".to_string(), |t| format!("{} en la mano", t.nombre));
                    self.ui.hud.notice("manos", &text, Level::Normal, 2.5);
                }
            }
            Action::Jetpack => {
                let (text, level) = match self.pilot.toggle_pack() {
                    Some(true) => ("Mochila encendida · Espacio: subir · Ctrl: bajar", Level::Good),
                    Some(false) => ("Mochila apagada", Level::Off),
                    None => ("Este traje no lleva mochila", Level::Caution),
                };
                self.ui.hud.notice("mochila", text, level, 3.5);
            }
            Action::Steady => {
                let on = self.pilot.toggle_steady();
                self.ui.hud.notice("mochila", if on { "Estabilizador de la mochila: al soltar las teclas te frena y mantiene tu altura (Ctrl: bajar)" } else { "Estabilizador apagado: sigues con la velocidad que lleves" }, if on { Level::Good } else { Level::Caution }, 3.5);
            }
            Action::Menu if self.spawner.open || self.spawner.placing() => self.spawner.cancel(),
            Action::Menu => {
                self.ui.menu = !self.ui.menu;
                if self.ui.menu {
                    self.menu_lists();
                    self.capture_mouse(false);
                }
            }
            Action::Catalog => {
                self.spawner.toggle();
                if self.spawner.open {
                    self.capture_mouse(false);
                }
            }
            Action::Inspector => {
                self.inspector.open = !self.inspector.open;
                if self.inspector.open {
                    self.capture_mouse(false);
                }
            }
            // free flight is a tool: the demo is played on foot (and with what the suit carries)
            Action::Flight => self.pilot.toggle_flight(),
            Action::Lamp => self.pilot.lamps = !self.pilot.lamps,
            Action::Photo => {
                let dir = crate::root().join("fotos");
                let _ = std::fs::create_dir_all(&dir);
                let n = std::fs::read_dir(&dir).map_or(0, |d| d.filter_map(Result::ok).filter(|e| e.path().extension().is_some_and(|x| x == "png")).count());
                let path = dir.join(format!("luna_{:04}.png", n + 1));
                self.ui.hud.notice("foto", &format!("Foto: fotos/luna_{:04}.png", n + 1), Level::Good, 3.0);
                self.photo = Some(path);
            }
            Action::Rangefinder => self.range.on = !self.range.on,
            Action::View => {
                let on = self.chase.toggle();
                self.ui.hud.notice("vista", if on { "Vista desde fuera · rueda: acercar o alejar · V: volver a tus ojos" } else { "Vista desde tus ojos" }, Level::Normal, 3.5);
            }
            Action::FollowMissile => self.blasts.follow = !self.blasts.follow,
            Action::Reset => self.pilot.reset(),
            Action::Stats => self.ui.stats = !self.ui.stats,
            Action::Editor => {
                let eye = self.pilot.position;
                self.editor.toggle(&self.ships, &self.builds, self.pilot.ride.map(|r| r.id), eye);
                if self.editor.open {
                    self.capture_mouse(false);
                    if !self.pilot.flying {
                        self.pilot.toggle_flight();
                    }
                }
            }
            Action::LodTint => {
                let on = !self.renderer.lod_tint();
                self.renderer.set_lod_tint(on);
                self.ui.hud.notice("detalle", if on { "Niveles de detalle teñidos: 0 normal, 1 verde, 2 cian, 3 amarillo, 4 naranja, 5 rojo" } else { "Niveles de detalle sin teñir" }, Level::Normal, 4.0);
            }
            Action::HandAxes => {
                let on = self.handwork.toggle_axes();
                self.ui.hud.notice("manos", if on { "Ejes de las manos · verde: hacia donde mira la palma · azul: los dedos · rojo: de índice a meñique · gris: como estaría con la muñeca recta · bola ámbar: muñeca en su tope" } else { "Ejes de las manos quitados" }, Level::Normal, 6.0);
            }
            Action::Fullscreen => self.window.set_fullscreen(if self.window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) }),
            // the rest are held, not pressed
            _ => {}
        }
    }

    /// What the HUD shows this frame. Each thing says what it is; where it goes is the HUD's
    /// business (`hud`): the suit bottom left, the tools bottom centre, what is aimed at under the
    /// crosshair and to the right, what the game tells you top right.
    fn fill_hud(&mut self, view: &lunar_render::View) {
        let hud = &mut self.ui.hud;
        self.aboard.hud(&self.pilot, &self.ships, &self.builds.set, !self.gear.owns_click(), hud);
        self.handwork.hud(hud);
        // bare hands: what they hold, or what they could take
        let standing = self.pilot.ride.map(|r| r.id);
        if let Some(s) = self.hands.holding().and_then(|id| self.builds.set.get(id)) {
            hud.hints.push("Suelta el clic para dejarla · Rueda: acercar o alejar".into());
            hud.card = Some(Card { title: s.label(&self.builds.set.lib.catalog), value: format!("{:.0} kg", s.mass), level: Level::Normal, lines: Vec::new(), hint: "Sobre un anclaje abierto: suéltala y haz clic en su palanca".into() });
        } else if hud.prompt.is_none()
            && self.gear.held.is_none()
            && let Some((r, _, _)) = self.hands.reach(&self.builds.set, &self.ships, view, standing)
        {
            hud.prompt = Some(match r.no {
                None => Prompt { key: "Clic".into(), text: format!("Coger (mantener) · {} · {:.0} kg", r.name, r.mass), level: Level::Normal },
                Some(why) => Prompt { key: String::new(), text: format!("{} · {why}", r.name), level: Level::Off },
            });
        }
        let p = &self.pilot;
        // the suit: its jet pack
        if p.has_pack() && !p.flying {
            let fuel = p.fuel as f32;
            let filling = p.ride.is_some() && p.jet == 0.0 && fuel < 0.999;
            let state = if filling {
                "RECARGANDO"
            } else if !p.pack_on {
                "APAGADA"
            } else if fuel <= 0.0 {
                "SIN GAS"
            } else if p.jet > 0.0 {
                "EMPUJE"
            } else if !p.steady {
                "ENCENDIDA · SIN ESTABILIZAR"
            } else {
                "ENCENDIDA"
            };
            let level = if !p.pack_on {
                Level::Off
            } else if fuel < LOW_GAS {
                Level::Warning
            } else if fuel < LOW_GAS * 2.5 {
                Level::Caution
            } else {
                Level::Normal
            };
            hud.gauges.push(Gauge { label: "MOCHILA".into(), key: input::shown(Action::Jetpack).into(), value: fuel, text: format!("{:.0} %", fuel * 100.0), state: state.into(), level, icon: Icon::Pack });
            if p.pack_on && fuel < LOW_GAS && !p.grounded {
                hud.notice("gas", if fuel <= 0.0 { "Mochila sin gas" } else { "Mochila: queda poco gas" }, Level::Warning, 1.5);
            }
            // in the air on the pack: what to fly it by
            if p.pack_on && !p.grounded && p.seat.is_none() {
                // (how high over the ground, where there is ground)
                if p.ground.is_some() {
                    hud.readouts.push(("ALTURA".into(), format!("{:.1} m", p.altitude().max(0.0))));
                }
                // (and how fast one climbs, where there is something to climb from)
                if p.ground.is_some() || p.ride.is_some() {
                    hud.readouts.push(("SUBIDA".into(), format!("{:+.1} m/s", p.vertical_speed())));
                }
                // and against the nearest ship: how far, how fast (what the pack steadies us to
                // is the one we left; flying back to it, this is what to bring to nothing)
                let near = self.builds.set.list.iter().filter(|s| s.owner.is_some() && s.held.is_none()).map(|s| (s.to_world(s.center).distance(p.position) - f64::from(s.radius) * 0.5, s)).filter(|(d, _)| *d < 2000.0).min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((d, s)) = near {
                    let rel = p.velocity_in(&self.builds.set) - s.velocity_at(p.position);
                    hud.readouts.push((s.name.to_uppercase(), format!("{} · {:.1} m/s", crate::rangefinder::metres(d.max(0.0)), rel.length())));
                }
            }
        }
        if p.flying {
            hud.readouts.push(("VUELO LIBRE".into(), format!("{:.0} m/s", p.speed)));
            if p.ground.is_some() {
                hud.readouts.push(("ALTURA".into(), crate::rangefinder::metres(p.altitude().max(0.0))));
            }
        }
        // where one is: what one weighs and by what, the air round the suit; the references one
        // finds one's way by there
        if !p.flying {
            let bodies = &self.world.bodies;
            let set = &self.builds.set;
            // what pulls here: the ship whose own gravity holds where one is; else the body that
            // has most of the place; past every body's reach, nothing
            let aboard = p.ride.map(|r| r.id).or(p.cabin).and_then(|id| set.get(id)).filter(|s| s.gravity.g > 0.0 && s.gravity.on > 0.5 && s.in_rooms(s.to_local(p.position)));
            let mut most = (0.0, None);
            bodies.shares(p.position, |_, b, share, _, _| {
                if share > most.0 {
                    most = (share, Some(b));
                }
            });
            let place = match (aboard, most.1) {
                (Some(s), _) => s.name.to_string(),
                (None, Some(b)) => b.name.clone(),
                (None, None) => "Espacio".to_string(),
            };
            let g = aboard.map_or_else(|| bodies.field(p.position).g(), |s| f64::from(s.gravity.g * s.gravity.on));
            hud.status.push((place, format!("{} m/s²", format!("{g:.2}").replace('.', ",")), if g < 0.005 { Level::Off } else { Level::Normal }));
            let kpa = self.sounds.kpa;
            hud.status.push(if kpa < 1.0 { ("Exterior".into(), "VACÍO".into(), Level::Off) } else if kpa < 40.0 { ("Aire".into(), format!("{kpa:.0} kPa · POBRE"), Level::Caution) } else { ("Aire".into(), format!("{kpa:.0} kPa"), Level::Good) });
            // the suit's clock: how long it has been out
            let t = self.started.elapsed().as_secs();
            hud.status.push(("Misión".into(), format!("T+ {:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60), Level::Normal));
            let who = self.nav.who(p, set, view, self.renderer.sun().direction());
            self.nav.compass(bodies, &who, &mut hud.compass);
            self.visor.hud(hud);
        }
        // the hands: the suit's tools, and what the one in hand does
        self.gear.slots(&mut hud.slots);
        if let Some(k) = self.gear.held {
            hud.hints.push(self.gear.help(k));
        }
        if self.spawner.placing() {
            hud.hints.push("Clic: colocar · Rueda: girar · Mayús + rueda: distancia · Botón derecho o G: soltar".into());
        }
        if self.range.on {
            let r = Rangefinder::read(&self.world.bodies, &self.builds, &self.ships, view, &self.renderer, &self.vis);
            self.range.hud(&r, &mut hud.banner);
        }
        if let Some(m) = self.spawner.message.take() {
            hud.notice("catalogo", &m, Level::Normal, 3.0);
        }
    }

    /// One frame. Returns false when the program should end (bench or screenshot finished).
    fn frame(&mut self, o: &Options) -> Result<bool, Box<dyn Error>> {
        let now = Instant::now();
        let raw_dt = (now - self.last).as_secs_f64();
        let dt = raw_dt.min(0.1);
        self.last = now;
        self.frame_ms = raw_dt as f32 * 1000.0;
        // the frame that just ended, recorded; this one's parts timed from here
        if self.perf.recording() {
            let st = &self.renderer.stats;
            let (_, _, meshed, words) = self.renderer.structure_looks();
            self.perf.sample(perf::Sample {
                ms: self.frame_ms,
                parts: self.perf.now,
                gpu: if st.gpu_timed { st.gpu_ms.iter().sum() } else { 0.0 },
                structures: self.builds.set.list.len() as u32,
                awake: self.builds.set.list.iter().filter(|s| !s.anchored && !s.resting && s.held.is_none()).count() as u32,
                ships: self.ships.list.len() as u32,
                ships_full: self.ships.full as u32,
                draws: st.draws,
                triangles: st.triangles,
                meshed,
                words,
            });
        }
        self.perf.begin();
        let dragging = self.ui.ctx.input(|i| i.pointer.any_down());
        if !self.ui.apply && !dragging && self.renderer.bake_pending() {
            self.renderer.set_sun(self.renderer.sun(), false);
        }
        if self.ui.apply && (self.ui.realtime || !dragging) {
            self.ui.apply = false;
            self.renderer.apply(self.ui.edit.clone());
            self.blasts.fx.particles.set_capacity(self.renderer.particle_capacity());
            self.renderer.set_sun(self.ui.sun, dragging);
            self.preset_name = self.ui.preset.map_or("personalizado".into(), |p| p.name().to_string());
        }
        if self.ui.shots_changed {
            self.ui.shots_changed = false;
            for e in &self.ui.shots {
                self.blasts.set_shot(e);
            }
        }
        if self.ui.regenerate {
            self.ui.regenerate = false;
            let c = Counts { ships: self.ui.landed + self.ui.flying, flying: self.ui.flying, npcs: self.ui.npcs, ..self.counts };
            self.new_world(c)?;
        }
        let k = self.keys;
        let f = |b: bool| f64::from(u8::from(b));
        let h = |a: Action| input::held(a).is_some_and(|i| k[i]);
        // (seated, seen from outside, the look goes all the way round the ship)
        self.pilot.free_look = self.chase.on;
        // Alt held on foot: the mouse turns the head (the camera, from outside), not the body
        let free = h(Action::FreeLook) && !self.pilot.flying && self.pilot.seat.is_none() && self.start.is_none() && self.bench.is_none();
        self.handwork.gesture_key(h(Action::Gesture) && self.start.is_none() && !self.ui.menu);
        // (the wrist computer up: the look goes to it, unless Alt has it)
        let wrist = self.wrist_look.filter(|_| !free && self.start.is_none() && self.bench.is_none());
        self.look.update(dt, free || wrist.is_some());
        if let Some((yaw, pitch)) = wrist {
            // (the same however the second is cut into frames)
            let k = 1.0 - (-dt * 6.0).exp();
            self.look.yaw += (yaw - self.look.yaw) * k;
            self.look.pitch += (pitch - self.look.pitch) * k;
        }
        let mut input = Input {
            forward: f(h(Action::Forward)) - f(h(Action::Back)),
            side: f(h(Action::Right)) - f(h(Action::Left)),
            vertical: f(h(Action::Jump)) - f(h(Action::Down)),
            run: h(Action::Run),
            boost: h(Action::FreeLook),
            jump: self.jump,
            crouch: h(Action::Crouch),
            roll: f(h(Action::RollRight)) - f(h(Action::RollLeft)),
        };
        let view = if let Some(b) = &mut self.bench {
            b.t += dt;
            b.view()
        } else {
            // the air on the move pulls whoever stands in it
            self.pilot.wind = if self.pilot.seat.is_some() || self.pilot.flying {
                glam::DVec3::ZERO
            } else {
                let drag = if self.pilot.crouched() { lunar_ship::atmos::DRAG_CROUCHED } else { lunar_ship::atmos::DRAG_STANDING };
                AirFx::wind(&self.ships, &self.builds.set, self.pilot.position, drag)
            };
            // in the air inside a ship's rooms we go with it; outside them we are on our own.
            // Carried, by where we are in what carries us; on our own, whatever ship's rooms we
            // have come into (it and we are of the same instant)
            // (any structure's rooms: what has an inside says so itself)
            self.pilot.cabin = match self.pilot.ride {
                Some(r) => self.builds.set.get(r.id).filter(|s| s.in_rooms(r.local)).map(|s| s.id),
                None => self.builds.set.rooms_at(self.pilot.position),
            };
            if self.start.is_some() {
                input = Input::default();
            }
            // what the keys ask of the player this frame: the world steps them, with everything
            // else that moves among its structures (`builds.update`)
            self.pilot.begin(input, self.ui.controls);
            self.jump = false;
            self.pilot.view()
        };
        self.world.update(&mut self.renderer, dt);
        self.perf.lap(perf::WORLD);
        let mut looked = o.look.and_then(|l| self.world.look_at(l, view));
        let (mut menu, mut begin) = (None, false);
        if let Some(sc) = &mut self.script {
            let held = self.pilot.view_aboard(&self.builds.set).unwrap_or(view);
            if let Some(v) = sc.update(dt, &mut self.ships, &self.builds, &mut self.blasts, held) {
                looked = Some(v);
            }
            if let Some((en, mira, fov)) = sc.site_camera {
                let (site, b) = (self.world.site, self.world.bodies.get(self.world.site.body));
                let at = |v: [f64; 3]| b.above_ground(site.at(v[0], v[1]), v[2]);
                let (eye, target) = (at(en), at(mira));
                looked = Some(lunar_render::View { eye, forward: (target - eye).normalize_or(view.forward), up: b.up(eye), fov_y: fov.map_or(view.fov_y, f32::to_radians), near: view.near });
            }
            if let Some((en, mira, fov)) = sc.player_camera {
                // by the player: their feet, the way they face, their left
                let (feet, up) = self.pilot.feet();
                let ahead = self.pilot.heading();
                let left = up.cross(ahead);
                let at = |v: [f64; 3]| feet + left * v[0] + up * v[1] + ahead * v[2];
                let (eye, target) = (at(en), at(mira));
                looked = Some(lunar_render::View { eye, forward: (target - eye).normalize_or(view.forward), up, fov_y: fov.map_or(view.fov_y, f32::to_radians), near: view.near });
            }
            for r in std::mem::take(&mut sc.requests) {
                match r {
                    crate::script::Request::LodTint(on) => self.renderer.set_lod_tint(on),
                    crate::script::Request::Walk([ahead, side]) => {
                        for (a, on) in [(Action::Forward, ahead > 0.0), (Action::Back, ahead < 0.0), (Action::Right, side > 0.0), (Action::Left, side < 0.0)] {
                            self.keys[input::held(a).unwrap_or(0)] = on;
                        }
                    }
                    crate::script::Request::Run(on) => self.keys[input::held(Action::Run).unwrap_or(0)] = on,
                    crate::script::Request::Down(on) => self.keys[input::held(Action::Down).unwrap_or(0)] = on,
                    crate::script::Request::Jump => self.jump = true,
                    crate::script::Request::LookAt(id, at) => {
                        if let Some(s) = self.builds.set.get(id) {
                            self.pilot.look_at(s.to_world(glam::Vec3::from(at)));
                        }
                    }
                    // (standing as one weighs there, and still to the ship however it goes: put
                    // aboard one under way)
                    crate::script::Request::Go(id, at) => self.pilot.put_on(&self.builds.set, id, glam::Vec3::from(at)),
                    crate::script::Request::Lamp(on) => self.pilot.lamps = on,
                    crate::script::Request::Crouch(on) => self.keys[input::held(Action::Crouch).unwrap_or(0)] = on,
                    crate::script::Request::Pack(on) => self.pilot.pack_on = on && self.pilot.has_pack(),
                    crate::script::Request::Thrust(on) => self.keys[input::held(Action::Jump).unwrap_or(0)] = on,
                    crate::script::Request::Look([yaw, pitch]) => self.pilot.look_by(yaw.to_radians(), pitch.to_radians()),
                    crate::script::Request::Menu(tab) => menu = Some(tab),
                    crate::script::Request::Chase(m) => {
                        self.chase.set(m.is_some());
                        if let Some(m) = m {
                            self.chase.reach(m);
                        }
                    }
                    crate::script::Request::Head([yaw, pitch]) => {
                        (self.look.yaw, self.look.pitch) = (yaw.to_radians(), pitch.to_radians());
                        self.keys[input::held(Action::FreeLook).unwrap_or(0)] = yaw != 0.0 || pitch != 0.0;
                    }
                    crate::script::Request::Gesture(id) => {
                        if !self.handwork.gesture_named(&id) {
                            sc.note(&format!("gesto: no hay «{id}»"));
                        }
                    }
                    crate::script::Request::Wrist(up) => self.handwork.set_wrist(up),
                    crate::script::Request::HandAxes(on) => self.handwork.axes = on,
                    crate::script::Request::Where(id) => {
                        let (feet, _) = self.pilot.feet();
                        let with = match (self.pilot.ride, self.pilot.beside()) {
                            (Some(_), _) => "la nave lo lleva",
                            (None, Some(_)) => "en el aire, va junto a una nave",
                            _ if self.pilot.grounded => "en el suelo",
                            _ => "en el aire, por su cuenta",
                        };
                        let line = match id.and_then(|id| self.builds.set.get(id)) {
                            Some(s) => {
                                let (at, v) = (s.to_local(feet), self.pilot.velocity_in(&self.builds.set) - s.velocity_at(feet));
                                format!("jugador: pies en ({:.2}, {:.2}, {:.2}) de la nave; {with}; va a {:.2} m/s respecto a ella, que va a {:.1} m/s", at.x, at.y, at.z, v.length(), s.vel.length())
                            }
                            None => format!("jugador: {with}"),
                        };
                        sc.note(&line);
                        sc.note(&format!("  proyectiles: {} en vuelo, {} impactos; ultimo {:?}", self.blasts.rounds.len(), self.blasts.impact_count, self.blasts.last_impact.map(|hit| (hit.at, hit.surface))));
                        if let Some(ship) = id.and_then(|id| self.builds.set.get(id)) {
                            for round in self.blasts.rounds.list.iter().take(4) {
                                sc.note(&format!("  proyectil en {:?} de la nave; velocidad relativa {:?}", ship.to_local(round.pos), ship.dir_to_local(round.vel - ship.velocity_at(round.pos))));
                            }
                        }
                        // what holds where they are: what pulls, what they weigh, which way is
                        // up for them, and what the compass reads
                        let (p, bodies) = (&self.pilot, &self.world.bodies);
                        let here = bodies.field(p.position);
                        let (_, up) = p.feet();
                        let mut shares = String::new();
                        bodies.shares(p.position, |_, b, share, _, r| shares.push_str(&format!(" {} {:.3} (a {:.1} km de altura)", b.name, share, (r - b.radius) / 1000.0)));
                        let deck = id.and_then(|id| self.builds.set.get(id)).map_or(String::new(), |s| format!("; su arriba está a {:.1}° del de la nave (gravedad propia {:.2} m/s² al {:.0} %)", up.angle_between((s.rot * glam::Vec3::Y).as_dvec3()).to_degrees(), s.gravity.g, s.gravity.on * 100.0));
                        sc.note(&format!("  rige:{}; tirón {:.3} m/s²; pesa {:.3} m/s² ({:.3} sobre los pies){deck}", if shares.is_empty() { " ningún cuerpo (espacio libre)" } else { &shares }, here.g(), p.weight().length(), p.weighs()));
                        let mut compass = crate::nav::Compass::default();
                        self.nav.compass(bodies, &self.nav.who(p, &self.builds.set, &held, self.renderer.sun().direction()), &mut compass);
                        let scales: Vec<String> = compass.scales().iter().map(|s| format!("{} {:.0}° ({:.2})", s.name, s.heading, s.alpha)).collect();
                        let marks: Vec<String> = compass.marks().iter().map(|m| format!("{} {:+.0}°{} ({:.2})", m.text, m.bearing, if m.rise.abs() > 4.0 { format!(" ↕{:+.0}°", m.rise) } else { String::new() }, m.alpha)).collect();
                        sc.note(&format!("  brújula: escalas [{}]; marcas [{}]", scales.join(", "), marks.join(", ")));
                    }
                    crate::script::Request::Hands => {
                        let [left, right] = self.handwork.wrists();
                        let tool = self.gear.tool_at().map_or(String::new(), |at| format!("\n  herramienta en ({:.3}, {:.3}, {:.3}) del ojo", at.x, at.y, at.z));
                        sc.note(&format!("manos:\n  izq: {left}\n  der: {right}{tool}"));
                    }
                    crate::script::Request::Start(on) => self.start = on.then(|| crate::start::Start::new(&self.ships, &self.builds.set, false)),
                    crate::script::Request::Begin => begin = true,
                    crate::script::Request::StartPlace(n) => {
                        if let Some(st) = &mut self.start {
                            st.chosen = n.min(st.places.len().saturating_sub(1));
                        }
                    }
                    crate::script::Request::Window(name) => {
                        self.spawner.open = name == "catalogo";
                        self.inspector.open = name == "inspector";
                        self.ui.stats = name == "cifras";
                    }
                    crate::script::Request::Grab(true) => {
                        let v = looked.unwrap_or(view);
                        match self.hands.grab(&self.builds.set, &self.ships, &v, None) {
                            Ok(true) => sc.note(&format!("coger: estructura {:?}", self.hands.holding())),
                            Ok(false) => sc.note("coger: no hay nada suelto en la mira"),
                            Err(why) => sc.note(&format!("coger: {why}")),
                        }
                    }
                    crate::script::Request::Grab(false) => self.hands.release(&mut self.builds),
                    crate::script::Request::Sit(Some((structure, seat))) => {
                        self.aboard.aim = Some(crate::aboard::Aim { structure, target: crate::aboard::Target::Seat(seat) });
                        self.aboard.use_key(&mut self.pilot, &self.ships, &self.builds.set);
                    }
                    crate::script::Request::Sit(None) => self.aboard.stand(&mut self.pilot, &mut self.ships, &self.builds.set),
                    crate::script::Request::Hear => {
                        let text = self.sounds.heard();
                        sc.note(&format!("se oye: {text}; partículas en el aire: {}", self.blasts.fx.particles.len()));
                    }
                    crate::script::Request::Edit(v) => {
                        if !self.editor.open {
                            let eye = sc.ship.and_then(|id| self.builds.set.get(id)).map_or(self.pilot.position, |s| s.to_world(s.center));
                            self.editor.toggle(&self.ships, &self.builds, sc.ship, eye);
                        }
                        match serde_json::from_value::<lunar_editor::Op>(v) {
                            Ok(op) => self.editor.op(op),
                            Err(e) => sc.note(&format!("ERROR editar: {e}")),
                        }
                        sc.note(&format!("editor: {}", self.editor.status()));
                    }
                    crate::script::Request::ApplyEdit => self.editor.request_apply(),
                    crate::script::Request::Fleet(f) => {
                        let site = self.world.site;
                        let (cols, rows) = (f.cuenta[0].max(1), f.cuenta[1].max(1));
                        let mut made = 0;
                        for k in 0..cols * rows {
                            let (i, j) = (f64::from(k % cols) - f64::from(cols - 1) * 0.5, f64::from(k / cols) - f64::from(rows - 1) * 0.5);
                            let dir = site.at(f.este + i * f.paso, f.norte + j * f.paso);
                            let yaw = 0.7 * f64::from(k);
                            let id = if f.altura > 0.0 {
                                let b = self.world.bodies.get(site.body);
                                let pos = b.above_ground(dir, f.altura);
                                let rot = lunar_core::scene::basis(dir, site.north * yaw.cos() + site.east * yaw.sin());
                                self.ships.spawn_free(&mut self.builds, &f.nave, pos, rot)
                            } else {
                                self.ships.spawn(&mut self.builds, &self.world.bodies, &f.nave, site.body, dir, yaw)
                            };
                            match id {
                                Ok(_) => made += 1,
                                Err(e) => {
                                    sc.note(&format!("ERROR flota: {e}"));
                                    break;
                                }
                            }
                        }
                        sc.note(&format!("flota: {made} {} más ({} naves, {} estructuras)", f.nave, self.ships.list.len(), self.builds.set.list.len()));
                    }
                    crate::script::Request::Put(id, p) => {
                        let bodies = &self.world.bodies;
                        match (bodies.find(&p.cuerpo), self.builds.set.index_of(id)) {
                            (Some(body), Some(k)) => {
                                let b = bodies.get(body);
                                let s = &mut self.builds.set.list[k];
                                let dir = p.hacia.map_or_else(|| b.up(s.to_world(s.center)), |d| glam::DVec3::from_array(d).normalize_or(glam::DVec3::Y));
                                let local = lunar_core::scene::Site::new(body, b, dir);
                                let roll = glam::Quat::from_rotation_z(p.volcada.to_radians());
                                s.rot = (lunar_core::scene::basis(dir, local.north) * roll).normalize();
                                let centre = b.center + dir * (b.radius + p.altura + if p.sobre_suelo { b.height(dir) } else { 0.0 });
                                s.pos = centre - (s.rot * s.center).as_dvec3();
                                s.vel = local.east * p.velocidad[0] + local.north * p.velocidad[1] + dir * p.velocidad[2];
                                s.spin = (local.east * f64::from(p.giro[0]) + local.north * f64::from(p.giro[1]) + dir * f64::from(p.giro[2])).as_vec3();
                                (s.resting, s.still, s.acc) = (false, 0.0, glam::DVec3::ZERO);
                                sc.note(&format!("poner: {} a {:.1} km sobre {} ({}), a {:.0} m/s", s.name, p.altura / 1000.0, b.name, p.cuerpo, s.vel.length()));
                                // (whoever it carries goes with it now)
                                self.pilot.moved_with(&self.builds.set);
                            }
                            (None, _) => sc.note(&format!("ERROR poner: no hay cuerpo '{}'", p.cuerpo)),
                            (_, None) => sc.note("ERROR poner: no hay nave"),
                        }
                    }
                    crate::script::Request::Tool(id) => {
                        if !self.gear.pick_named((!id.is_empty()).then_some(id.as_str())) {
                            sc.note(&format!("ERROR equipo: no hay '{id}'"));
                        }
                    }
                    crate::script::Request::Trigger(on) => self.gear.trigger = on,
                    crate::script::Request::Visor(down, effort) => self.visor.set(down, effort),
                    crate::script::Request::Marks => sc.note(&format!("{}\n{}", self.prints.report(&self.renderer), self.visor.report())),
                    crate::script::Request::IntegrityView(on) => self.gear.view = on,
                    crate::script::Request::Damage { part, share } => {
                        let id = sc.ship.or_else(|| self.ships.list.first().map(|s| s.structure));
                        let names = id.and_then(|id| self.ships.by_structure(id)).map(|n| self.ships.list[n].kind.parts.clone()).unwrap_or_default();
                        let hit = lunar_ship::kind::resolve(&names, &part);
                        if let Some(s) = id.and_then(|id| self.builds.set.list.iter_mut().find(|s| s.id == id)) {
                            for &i in &hit {
                                let p = &mut s.parts[i as usize];
                                p.hp = p.max_hp * (1.0 - share).max(0.0);
                                if share >= 1.0 {
                                    p.alive = false;
                                    p.working = false;
                                }
                            }
                            s.refresh();
                        }
                        sc.note(&format!("dañar {part}: {} piezas al {:.0} %", hit.len(), (1.0 - share) * 100.0));
                    }
                    crate::script::Request::Aim(k) => self.script_aim = k.and_then(|k| sc.ship.or_else(|| self.ships.list.first().map(|s| s.structure)).map(|id| (id, k))),
                    crate::script::Request::Fire(f) => self.barrage = Some((f.tiro, f.por_segundo, f.segundos, 0.0, 1)),
                    crate::script::Request::Profile(Some(p)) => {
                        let name = std::path::Path::new(&p).file_stem().map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned());
                        self.perf.start(crate::root().join(&p), &name);
                    }
                    crate::script::Request::Profile(None) => {
                        if let Some(text) = self.perf.stop() {
                            sc.note(&text);
                            // what is still moving as it ends (a standing fleet: nothing should be)
                            let awake: Vec<String> = self.builds.set.list.iter().filter(|s| !s.anchored && !s.resting && s.held.is_none()).take(8).map(|s| format!("{} (v {:.3} m/s, giro {:.3} rad/s, fuerza {:.0} N, par {:.0} N·m)", s.name, s.vel.length(), s.spin.length(), s.force.length(), s.torque.length())).collect();
                            if !awake.is_empty() {
                                sc.note(&format!("  despiertas al acabar: {}", awake.join("; ")));
                            }
                            sc.flush();
                        }
                    }
                    crate::script::Request::Measure => {
                        let v = looked.unwrap_or(view);
                        let rd = Rangefinder::read(&self.world.bodies, &self.builds, &self.ships, &v, &self.renderer, &self.vis);
                        let (all, full) = self.renderer.structure_pool();
                        let mut lines = vec![format!("medida desde ({:.0}, {:.0}, {:.0}):", v.eye.x, v.eye.y, v.eye.z)];
                        if let Some((d, what)) = &rd.hit {
                            lines.push(format!("  mira: {} · {what}", crate::rangefinder::metres(*d)));
                        }
                        if let Some((k, d, lod)) = &rd.ship {
                            lines.push(format!("  nave {k} a {}: {lod}", crate::rangefinder::metres(*d)));
                        }
                        if let Some(t) = &self.world.traffic {
                            let (air, orbit) = t.flying(self.world.time);
                            let (near, _, _) = t.closest();
                            lines.push(format!("  tráfico: {} naves, {air} en vuelo, {orbit} en órbita; {} vuelos acabados; las dos más cercanas a {near:.0} m", t.len(), t.landed));
                        }
                        if let Some(sc) = &self.gear.scan {
                            lines.push(format!("  escáner: {} · {} · {} · integridad {:.0} %", sc.title, sc.detail, if sc.gone { "FALTA" } else { "está" }, sc.integrity * 100.0));
                        }
                        lines.push(format!("  GPU: {all} vértices de estructuras ({full} de vistas completas); ocultas {}, comprobadas {} en {:.2} ms", self.vis.hidden.len(), self.vis.checks, self.vis.ms));
                        sc.note(&lines.join("\n"));
                    }
                }
            }
        }
        if let Some(tab) = menu {
            self.menu_lists();
            self.ui.show_menu(tab.as_deref());
        }
        if begin {
            self.begin();
            // (a script has no mouse to take)
            self.capture_mouse(false);
        }
        // the start menu: the picture is of the place chosen in it, from round it
        if let Some(st) = &mut self.start {
            looked = Some(st.view(dt, &view, &self.builds.set, &self.world.bodies));
        }
        let view = looked.unwrap_or(view);
        // a script's fire: rounds at the ships from all round, each from 60 m off toward one of them
        if let Some((shot, rate, left, owed, seed)) = &mut self.barrage {
            *owed += *rate * dt;
            *left -= dt;
            let n = self.ships.list.len();
            while *owed >= 1.0 && n > 0 {
                *owed -= 1.0;
                *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let r = |k: u32| ((*seed >> (11 + k * 13)) & 0x3ff) as f64 / 1023.0;
                let Some(s) = self.builds.set.get(self.ships.list[(r(0) * n as f64) as usize % n].structure) else { continue };
                let at = s.to_world(s.center + glam::Vec3::new((r(1) as f32 - 0.5) * s.radius, (r(2) as f32 - 0.5) * s.radius * 0.4, (r(3) as f32 - 0.5) * s.radius * 1.6));
                let up = self.world.bodies.get(self.world.bodies.dominant(at)).up(at);
                let side = up.any_orthonormal_vector();
                let a = r(1) * std::f64::consts::TAU;
                let from = at + (side * a.cos() + up.cross(side) * a.sin()) * 60.0 + up * (10.0 + 30.0 * r(2));
                let shot = shot.clone();
                self.blasts.fire_from(&shot, from, (at - from).normalize(), glam::DVec3::ZERO, &self.world.bodies, &mut self.builds);
            }
            if *left <= 0.0 {
                self.barrage = None;
            }
        }
        let view = self.blasts.update(dt, &self.world.bodies, view, self.pilot.motion_in(&self.builds.set), &mut self.builds);
        self.perf.lap(perf::SHOTS);
        let sun = self.renderer.sun().direction();
        self.builds.set.sun = sun;
        // in full: the ship the player rides and the ones a tool is working on
        let awake: Vec<u64> = self.pilot.ride.map(|r| r.id).into_iter().chain(self.script.as_ref().and_then(|sc| sc.ship)).chain(self.editor.ship()).collect();
        let people = self.pilot.body();
        self.tactics.look(&mut self.ships, &self.builds, &self.world.bodies, self.world.traffic.as_ref(), &mut self.blasts);
        self.ships.update(dt, &mut self.builds, &self.world.bodies, &mut self.blasts.fx, sun, view.eye, &awake, &people);
        self.tactics.fire(&mut self.ships, &self.builds, &self.world.bodies, &mut self.blasts);
        // a seat on something that moves takes whoever sits in it along
        if let Some(seat) = &mut self.pilot.seat
            && let Some(n) = self.ships.by_structure(seat.structure)
        {
            seat.eyes = self.ships.list[n].seat_eyes(seat.index);
        }
        // what a ship tells whoever is working it: shown to the one who rides it or stands by it
        for sh in &mut self.ships.list {
            if sh.said.is_empty() {
                continue;
            }
            let said = std::mem::take(&mut sh.said);
            let here = self.pilot.ride.is_some_and(|r| r.id == sh.structure) || self.builds.set.get(sh.structure).is_some_and(|s| s.to_world(s.center).distance(view.eye) < f64::from(s.radius) + 10.0);
            if here {
                for (about, text, level) in said {
                    self.ui.hud.notice(&about, &text, [Level::Normal, Level::Caution, Level::Warning][usize::from(level.min(2))], 5.0);
                }
            }
        }
        self.perf.lap(perf::SHIPS);
        // the world on, and the player with it: stepped among its structures slice by slice
        // (`lunar_core::structure::schedule::Among`), so they are always of the same instant
        let (effects, mut flight) = self.blasts.flight();
        if self.bench.is_none() {
            self.builds.update(dt, &self.world.bodies, effects, view.eye, &mut [&mut self.pilot, &mut flight]);
        } else {
            self.builds.update(dt, &self.world.bodies, effects, view.eye, &mut [&mut flight]);
        }
        self.blasts.land_rounds(&self.world.bodies, &mut self.builds);
        self.perf.lap(perf::PHYSICS);
        let shake = self.air.frame(dt as f32, &self.ships, &self.builds.set, &self.world.bodies, &mut self.blasts.fx, view.eye);
        self.perf.lap(perf::AIR);
        // the picture is taken from where the player is now that the world has moved (seated,
        // with the ship's own turn)
        let view = match (looked, self.bench.is_none() && !self.blasts.follow) {
            (None, true) => self.pilot.view_aboard(&self.builds.set).unwrap_or_else(|| lunar_render::View { eye: self.pilot.eye(), ..view }),
            _ => view,
        };
        // from outside (V): the picture is the camera's; what is aimed at, held and worn stays the
        // body's, turned to what is under the middle of the picture
        let outside = self.chase.on && looked.is_none() && self.bench.is_none() && !self.blasts.follow && !self.pilot.flying && !self.editor.open && self.body.is_some();
        // (looking round with Alt: the picture turns, the body's own look and aim stay)
        let looking = self.look.active() && looked.is_none();
        let (view, aim) = if outside {
            let ship = self.pilot.seat.and_then(|seat| self.builds.set.get(seat.structure)).map(|s| (s.to_world(s.center), f64::from(s.radius)));
            let cam = self.chase.view(dt, &self.look.apply(&view), ship, &self.builds.set, &self.world.bodies);
            (cam, Some(if ship.is_some() || looking { view } else { self.chase.aim(&view, &cam, &self.builds.set) }))
        } else if looking {
            (self.look.apply(&view), Some(view))
        } else {
            (view, None)
        };
        // the air's shake
        let view = if shake > 0.0 {
            let right = view.forward.cross(view.up).normalize_or(glam::DVec3::X);
            let t = self.world.time;
            let jitter = right * (t * 47.0).sin() + view.up * (t * 39.0 + 0.7).sin() * 0.8;
            lunar_render::View { forward: (view.forward + jitter * f64::from(shake) * 0.03).normalize(), ..view }
        } else {
            view
        };
        // right button: zoom in (eased)
        let want = if self.zooming && self.captured { 1.0 } else { 0.0 };
        self.zoom += (want - self.zoom) * (1.0 - (-dt * 14.0).exp());
        let view = lunar_render::View { fov_y: view.fov_y / (1.0 + self.zoom * (ZOOM - 1.0)) as f32, ..view };
        let view = self.blasts.shaken(view);
        // the body and what it holds are the pilot's, wherever the picture is taken from
        let own = match (aim, looked) {
            (Some(aim), _) => aim,
            (None, Some(_)) => self.pilot.view_aboard(&self.builds.set).unwrap_or_else(|| self.pilot.view()),
            (None, None) => view,
        };
        self.last_aim = Some(own);
        let stance = (self.body.is_some() && !self.pilot.flying && self.bench.is_none()).then(|| {
            let (_, up) = self.pilot.feet();
            // (seated, the body is the seat's: up is the ship's, however it flies)
            let seat = self.pilot.seat.and_then(|seat| {
                let s = self.builds.set.get(seat.structure)?;
                Some(((s.rot * glam::Vec3::new(seat.heading.sin(), 0.0, seat.heading.cos())).as_dvec3(), (s.rot * glam::Vec3::Y).as_dvec3()))
            });
            let up = seat.map_or(up, |s| s.1);
            let ahead = seat.map_or_else(|| self.pilot.heading(), |s| s.0);
            crate::body::Stance {
                eye: own.eye,
                up,
                ahead: (ahead - up * ahead.dot(up)).normalize_or(self.pilot.heading()),
                eye_h: if seat.is_some() { 1.2 } else { self.pilot.eye_over_feet() },
                vel: self.pilot.velocity(),
                grounded: self.pilot.grounded,
                g: self.pilot.weighs(),
                ride: self.pilot.ride.map(|r| r.id),
                seated: seat.is_some(),
                inside: self.pilot.cabin.is_some() || seat.is_some(),
                own_eyes: looked.is_none() && !outside,
            }
        });
        let (steps, short) = match (&self.body, &stance) {
            (Some(b), Some(st)) => (
                crate::holding::Steps {
                    phase: b.gait.phase as f32,
                    walk: b.gait.walk as f32,
                    vel: self.pilot.velocity().as_vec3(),
                    // (what of the body carries the tool, as it was last posed)
                    carry: self.gear.rest().and_then(|name| b.carried(name, st)).map_or(glam::Vec3::ZERO, |c| crate::gear::eye_frame(&own).inverse() * c),
                },
                if b.short[1] != glam::Vec3::ZERO { b.short[1] } else { b.short[0] },
            ),
            _ => (crate::holding::Steps::default(), glam::Vec3::ZERO),
        };
        let motion = self.pilot.motion_in(&self.builds.set);
        self.gear.update(dt, &own, motion, &mut self.ships, &mut self.builds, &mut self.blasts, &self.world.bodies, &mut self.renderer, steps, short);
        // the visor the picture is seen through, from one's own eyes only
        let worn = looked.is_none() && !outside && !self.pilot.flying && self.bench.is_none() && self.start.is_none();
        self.visor.frame(dt as f32, &crate::visor::Senses::of(&self.pilot, &input, self.gear.filtering(), &own, worn), &mut self.renderer);
        if let (Some(body), Some(st)) = (&mut self.body, &stance) {
            let grips = self.gear.grips(&own, body, st);
            // the controls worked this frame: a hand goes to each
            // (a script's too: what it works is worked by the body it drives)
            for (structure, k, _) in self.aboard.changed.iter().chain(self.script.iter().flat_map(|sc| sc.changed.iter())) {
                self.handwork.worked(*structure, *k, None);
            }
            let (aim, held) = self.aboard.hand();
            let doing = crate::handwork::Doing { seat: self.pilot.seat.map(|s| (s.structure, s.index)), look: own.forward, aim, held, carrying: self.hands.holding().is_some() };
            let here = crate::handwork::Here {
                eye: own.eye,
                aboard: self.pilot.ride.map(|r| r.id).or(self.pilot.cabin),
                gas: self.pilot.has_pack().then_some(self.pilot.fuel as f32),
                outside: self.sounds.kpa,
                gravity: st.g as f32,
                inside: st.inside,
            };
            self.handwork.drive(dt, body, st, &self.builds.set, &self.world.bodies, &self.ships, grips, &doing, &here);
            // one's own arm in front of what one aims at is seen through
            body.look_along(st, st.own_eyes.then_some(own.forward), dt);
            self.wrist_look = self.handwork.look(body, st, &own);
        }
        self.hands.update(dt, &mut self.builds, &own, motion);
        // the particles and the flashes, from where the picture is taken (from outside that is
        // not where the player's eyes are)
        self.blasts.draw(&mut self.renderer, view.eye, &self.world.bodies);
        // what all that sounds like, and the dust it raises
        if self.bench.is_none() {
            for id in std::mem::take(&mut self.aboard.heard) {
                self.sounds.blow(id, 1.0, 0.0, true);
            }
            if std::mem::take(&mut self.gear.fired) {
                self.sounds.blow("disparo", 1.0, 0.0, true);
            }
            for id in std::mem::take(&mut self.gear.heard) {
                self.sounds.blow(&id, 1.0, 0.0, true);
            }
            let walker = self.body.as_ref().filter(|_| stance.is_some());
            let footfalls = walker.map(|b| b.gait.landed);
            let feet = self.sounds.frame(self.ui.controls.volume, &self.pilot, &self.ships, &self.builds.set, self.gear.working(), self.hands.holding().is_some(), self.ui.menu, footfalls);
            if (feet.step || feet.landed > 0.0) && self.pilot.ride.is_none() {
                let (_, up) = self.pilot.feet();
                let hard = if feet.landed > 0.0 { 1.0 + feet.landed } else { 1.0 };
                match walker {
                    // under the foot that came down
                    Some(b) => {
                        for i in 0..2 {
                            if b.gait.landed & (1 << i) != 0 {
                                self.dust.step(&mut self.blasts.fx, &self.world.bodies, b.gait.feet[i].at, self.pilot.velocity(), hard);
                            }
                        }
                    }
                    None => {
                        let feet_at = self.pilot.position - up * (self.pilot.altitude() + 1.0).min(3.0);
                        self.dust.step(&mut self.blasts.fx, &self.world.bodies, feet_at, self.pilot.velocity(), hard);
                    }
                }
            }
            self.dust.frame(dt as f32, &mut self.blasts.fx, &self.world.bodies, &self.pilot, self.pilot.pack_thrust(), &self.ships, &self.builds.set, view.eye);
            self.plumes.frame(dt as f32, &mut self.blasts.fx, &self.world.bodies, &self.pilot, self.body.as_ref().zip(stance.as_ref()), &self.ships, &self.builds.set, &mut self.renderer, &view, sun);
            // what touched the ground this frame leaves its mark on it
            self.prints.frame(self.world.time, dt, &mut self.dust, &self.world.bodies, walker, &self.builds.set, view.eye, &mut self.renderer);
        }
        let playing = self.captured && !self.ui.menu && self.bench.is_none() && self.start.is_none();
        // (round a ship from outside nothing of it is at hand)
        let afar = outside && self.pilot.seat.is_some();
        if playing && !self.spawner.placing() && afar {
            self.aboard.aim = None;
        } else if playing && !self.spawner.placing() {
            self.aboard.aim(&self.ships, &self.builds.set, own.eye, own.forward, self.pilot.seat.is_some());
        } else {
            // in a script's pictures with the HUD, the control it aims at is the hand's
            let shown = self.script.as_ref().is_some_and(|sc| sc.hud);
            self.aboard.aim = self.script_aim.filter(|_| shown).map(|(structure, k)| crate::aboard::Aim { structure, target: crate::aboard::Target::Control { k, elem: 0 } });
        }
        // what the hand is aimed at shows on its panel
        let aimed = self.aboard.aim.and_then(|a| match a.target {
            crate::aboard::Target::Control { k, .. } => Some((a.structure, lunar_ship::panels::Aimed::Control(k))),
            crate::aboard::Target::Indicator(i) => Some((a.structure, lunar_ship::panels::Aimed::Indicator(i))),
            _ => None,
        });
        let aimed = aimed.or(self.script_aim.map(|(id, k)| (id, lunar_ship::panels::Aimed::Control(k))));
        for sh in &mut self.ships.list {
            sh.panels.aimed = aimed.filter(|(id, _)| *id == sh.structure).map(|(_, a)| a);
        }
        self.aboard.update(dt as f32, &self.pilot, &mut self.ships, &self.builds.set);
        // with others: what our hand changed (or a script's steps) goes to them; without, it is
        // nobody's business
        let scripted = self.script.as_mut().map(|sc| std::mem::take(&mut sc.changed)).unwrap_or_default();
        match &mut self.multi {
            Some(m) => self.aboard.changed.drain(..).chain(scripted).for_each(|(structure, k, value)| m.control(structure, k, value)),
            None => self.aboard.changed.clear(),
        }
        self.spawner.update(&self.world.bodies, &self.builds, view.eye, view.forward, view.up, self.pilot.heading());
        // what is seen: from inside a ship's rooms only what its windows and open doors show
        let inside = self.pilot.ride.and_then(|r| {
            let k = self.ships.by_structure(r.id)?;
            let s = self.builds.set.get(r.id)?;
            lunar_ship::atmos::room_of(&self.ships.list[k].kind, s.to_local(view.eye)).map(|_| r.id)
        });
        self.perf.lap(perf::WORLD);
        self.vis.update(&self.world.bodies, &self.builds.set, &self.builds.set.lib.catalog, view.eye, inside, self.world.time);
        self.perf.lap(perf::VISIBILITY);
        if self.editor.open {
            self.editor.aim(&self.builds, view.eye, view.forward);
            let before = self.editor.ship();
            self.editor.update(&mut self.ships, &mut self.builds, &self.world.bodies);
            // a ship rebuilt is a new structure: a script about it follows it
            if let Some(sc) = &mut self.script
                && sc.ship == before
                && before != self.editor.ship()
            {
                sc.ship = self.editor.ship();
                sc.note(&format!("editor: {}", self.editor.status()));
            }
        }
        self.last_view = Some(view);
        self.renderer.set_hidden_structures(&self.vis.hidden);
        self.builds.show(&mut self.renderer, view.eye);
        let spawner = &self.spawner;
        let lamp = self.pilot.lamp(if outside || looking { &own } else { &view });
        let outline = if self.editor.open { self.editor.highlight(&self.ships, &self.builds) } else { lunar_core::props::PropScene::default() };
        let (gear, in_ship) = (&self.gear, inside.is_some());
        let hands = self.body.as_ref().filter(|_| stance.is_some()).map(|b| &b.hands);
        let handwork = &self.handwork;
        self.ships.show(&mut self.renderer, &self.builds, view.eye, lamp, &self.vis, |scene| {
            spawner.ghost(scene);
            gear.show(scene, &own, in_ship, hands);
            handwork.show(scene);
            // the editor's outline, in frames of its own after the scene's
            let base = scene.frames.len() as u16;
            scene.frames.extend(outline.frames.iter().copied());
            scene.props.extend(outline.props.iter().map(|p| lunar_core::props::Prop { frame: p.frame + base, ..*p }));
        });
        self.figures.clear();
        if let (Some(body), Some(st)) = (&self.body, &stance) {
            body.show(&mut self.figures, st);
        }
        // the others: our player and ships out, theirs in, their bodies drawn
        if let Some(m) = &mut self.multi {
            let tool = self.gear.held.map_or(0, |k| k as u8 + 1);
            let now = lunar_net::now();
            m.frame(now, &self.pilot, self.pilot.eye_over_feet(), [self.look.yaw, self.look.pitch], tool, self.gear.trigger, outside, &mut self.ships, &mut self.builds);
            if let Some(source) = &self.body_source {
                m.bodies(now, dt, source, &self.builds.set, &self.world.bodies, &self.ships, &mut self.figures);
            }
            for (text, level) in m.said.drain(..) {
                self.ui.hud.notice("red", &text, [Level::Normal, Level::Caution, Level::Warning][usize::from(level.min(2))], 5.0);
            }
        }
        self.renderer.set_bodies(&self.figures);
        // the HUD: in play always; in a script's pictures only if it asks for it
        self.ui.hud.begin(dt as f32);
        let shown = self.bench.is_none() && self.script.as_ref().is_none_or(|s| s.hud) && self.start.is_none();
        self.ui.hud.crosshair = shown && (playing || self.script.is_some()) && !afar && !looking;
        if shown {
            self.fill_hud(if outside || looking { &own } else { &view });
        }
        // with others: how the connection is, and each one's name over them in the picture
        if let (true, Some(m)) = (shown, &self.multi) {
            let (text, level) = m.status();
            self.ui.hud.status.push(("Red".into(), text, [Level::Normal, Level::Caution, Level::Warning][usize::from(level.min(2))]));
            let size = self.window.inner_size();
            let aspect = f64::from(size.width) / f64::from(size.height.max(1));
            let tan = (f64::from(view.fov_y) * 0.5).tan();
            let right = view.forward.cross(view.up).normalize_or_zero();
            let up = right.cross(view.forward);
            for (eye, name) in m.names() {
                let rel = eye + view.up * 0.42 - view.eye;
                let depth = rel.dot(view.forward);
                if !(0.5..=120.0).contains(&depth) {
                    continue;
                }
                let (x, y) = (rel.dot(right) / (depth * tan * aspect), rel.dot(up) / (depth * tan));
                if x.abs() < 1.0 && y.abs() < 1.0 {
                    self.ui.hud.tags.push(([(x * 0.5 + 0.5) as f32, (0.5 - y * 0.5) as f32], name.to_string(), (1.0 - (depth - 40.0) / 80.0).clamp(0.15, 1.0) as f32));
                }
            }
        }
        self.ui.panels = self.spawner.open || self.inspector.open || self.editor.open || self.start.is_some();
        self.ui.at_start = self.start.is_some();
        let elapsed = self.started.elapsed().as_secs_f64();
        // (a picture asked for with F12 goes out as a script's would; the HUD's notice of it is of
        // the frame after)
        let photo = self.photo.take();
        let script_shot = self.script.as_mut().and_then(|s| s.shot.take()).or(photo);
        let shot = script_shot.as_ref().or(o.shot.as_ref().filter(|_| elapsed > o.shot_at));
        let mode = if self.bench.is_some() { "BENCH" } else if self.pilot.flying { "VUELO" } else { "A PIE" };
        let ui = {
            let st = self.renderer.stats.clone();
            let info = Info {
            stats: &st,
            adapter: &self.adapter,
            fps: self.fps,
            frame_ms: self.frame_ms,
            sim_ms: self.world.sim_ms,
            mode,
            altitude: self.pilot.altitude(),
            speed: self.pilot.speed,
            speed_factor: if self.pilot.flying { self.ui.controls.factor(&input) } else { 1.0 },
            ships: self.counts.ships,
            npcs: self.counts.npcs,
            particles: self.blasts.fx.particles.len(),
            structures: self.builds.stats,
            missile: self.blasts.status(&self.builds),
            };
            let (spawner, inspector, editor, ships, set) = (&mut self.spawner, &mut self.inspector, &mut self.editor, &self.ships, &self.builds.set);
            let on = self.pilot.ride.map(|r| r.id);
            let eye = view.eye;
            let mut picked = false;
            let (start, menu_open, adapter) = (&mut self.start, self.ui.menu, self.adapter.as_str());
            let net = self.multi.as_ref().map(|m| m.status().0);
            let mut chosen = None;
            let frame = self.ui.frame(&self.window, &info, |ctx| {
                if let Some(st) = start.as_mut().filter(|_| !menu_open) {
                    chosen = st.draw(ctx, crate::BUILD, adapter, net.as_deref());
                }
                if spawner.open {
                    picked = spawner.window(ctx);
                }
                if inspector.open {
                    inspector.window(ctx, ships, set, on, eye);
                }
                editor.window(ctx);
            });
            if picked {
                self.capture_mouse(true);
            }
            match chosen {
                Some(crate::start::Action::Play) => self.begin(),
                Some(crate::start::Action::Options) => {
                    self.menu_lists();
                    self.ui.menu = true;
                }
                Some(crate::start::Action::Quit) => self.quit = true,
                // with others: the server written in the menu is asked to let us in
                Some(crate::start::Action::Connect) => {
                    if let Some((addr, name)) = self.start.as_ref().map(|st| (st.server.trim().to_string(), st.name.trim().to_string())) {
                        match crate::multi::Multi::connect(&addr, if name.is_empty() { "Jugador" } else { &name }, &self.ships) {
                            Ok(m) => self.multi = Some(m),
                            Err(e) => self.ui.hud.notice("red", &format!("No se puede conectar a {addr}: {e}"), Level::Warning, 6.0),
                        }
                    }
                }
                Some(crate::start::Action::Disconnect) => self.multi = None,
                None => {}
            }
            // what the menu's own foot asks: back to the start menu, or out
            match self.ui.ask.take() {
                Some(crate::ui::Ask::Start) => self.to_start(),
                Some(crate::ui::Ask::Quit) => self.quit = true,
                None => {}
            }
            frame
        };
        self.perf.lap(perf::SCENE);
        self.renderer.render(&FrameInput { view, time: self.world.time, ui: ui.as_ref(), capture: shot.map(|p| p.as_path()) })?;
        self.perf.lap(perf::RENDER);
        if script_shot.is_none() && shot.is_some() {
            return Ok(false);
        }
        if self.script.as_ref().is_some_and(|s| s.done) || self.quit {
            return Ok(false);
        }
        self.title_frames += 1;
        let since = (now - self.title_at).as_secs_f32();
        if since > 0.5 {
            self.fps = self.title_frames as f32 / since;
            self.title_frames = 0;
            self.title_at = now;
            self.window.set_title(&format!(
                "{} [{}] {mode} | {:.0} FPS | {} naves + {} NPC | Esc: menú y controles",
                crate::GAME,
                self.preset_name,
                self.fps,
                self.counts.ships + self.world.traffic.as_ref().map_or(0, |t| t.len()),
                self.counts.npcs
            ));
        }
        if let Some(b) = &mut self.bench {
            b.record(self.frame_ms, self.world.sim_ms, &self.renderer.stats);
            if b.done() {
                let report = bench::report(b, &self.adapter, &self.preset_name, self.counts.ships, self.counts.npcs);
                self.reports.push(report);
                if self.scenarios.is_empty() {
                    self.write_reports(o)?;
                    return Ok(false);
                }
                let n = self.scenarios.remove(0);
                let seconds = b.seconds;
                let flying = (n as f64 * self.defs.scenario.fleet.flying_share).floor() as usize;
                self.new_world(Counts { ships: n, flying, npcs: n, ..self.counts })?;
                let p = self.defs.scenario.player;
                self.bench = Some(Bench::new(seconds, format!("{n}+{n}"), &self.world.crowd.home, self.world.site, self.world.bodies.clone(), (p.fov.to_radians(), p.near)));
            }
        }
        Ok(true)
    }

    fn write_reports(&self, o: &Options) -> Result<(), Box<dyn Error>> {
        let path = if o.out.is_absolute() { o.out.clone() } else { crate::root().join(&o.out) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = if self.reports.len() == 1 { self.reports[0].clone() } else { format!("[{}]", self.reports.join(",\n")) };
        std::fs::write(&path, body)?;
        Ok(())
    }
}

impl App {
    /// The start: a script, a bench or a picture make the game here and now, with no window to
    /// be seen; a game to be played is made on a thread of its own behind the start-up screen.
    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let o = &self.opts;
        if let Some(dir) = &o.splash_test {
            let window = Arc::new(event_loop.create_window(window_attributes(o, false))?);
            let dir = if dir.is_absolute() { dir.clone() } else { crate::root().join(dir) };
            splash_pictures(window, &dir)?;
            event_loop.exit();
            return Ok(());
        }
        // (the game's window and its surface are made here, on the window's own thread; the
        // rest of the game, wherever)
        let game = Arc::new(event_loop.create_window(window_attributes(o, false))?);
        let screen = lunar_render::Screen::new(game.clone(), o.backend)?;
        if o.hidden() {
            self.state = Some(State::new(game, screen, o, &crate::boot::Boot::new(None))?);
            return Ok(());
        }
        let full = o.fullscreen.unwrap_or(false);
        let window = Arc::new(event_loop.create_window(window_attributes(o, !o.boot_test).with_title(format!("{} - arrancando", crate::GAME)).with_fullscreen((full && !o.boot_test).then_some(Fullscreen::Borderless(None))))?);
        let gpu = crate::splash_gpu::Gpu::new(window.clone(), false)?;
        let ctx = egui::Context::default();
        crate::hud::style(&ctx);
        let boot = crate::boot::Boot::new(Some(&boot_times()));
        let (o, b) = (o.clone(), boot.clone());
        let loader = std::thread::Builder::new().name("arranque".into()).spawn(move || State::new(game, screen, &o, &b).map_err(|e| e.to_string()))?;
        self.starting = Some(Starting { window, gpu, ctx, splash: crate::splash::Splash::default(), boot, last: Instant::now(), loader: Some(loader), ready: None });
        Ok(())
    }

    /// While starting: the game taken from its thread when it is made; once the start-up screen
    /// has shown its end, the game's window comes up where that one was and that one goes.
    fn started(&mut self, event_loop: &ActiveEventLoop) {
        let Some(st) = &mut self.starting else { return };
        if st.loader.as_ref().is_some_and(|l| l.is_finished()) {
            match st.loader.take().map(|l| l.join()) {
                Some(Ok(Ok(state))) => {
                    st.boot.save(&boot_times());
                    st.ready = Some(state);
                }
                Some(Ok(Err(e))) => {
                    self.error = Some(e);
                    event_loop.exit();
                    return;
                }
                _ => {
                    self.error = Some("el arranque se ha interrumpido (ver out/panic.log)".into());
                    event_loop.exit();
                    return;
                }
            }
        }
        if st.ready.is_some() && st.splash.over() {
            let Some(mut st) = self.starting.take() else { return };
            let Some(mut state) = st.ready.take() else { return };
            if !self.opts.boot_test {
                if self.opts.fullscreen.unwrap_or(false) {
                    state.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                } else if let Ok(at) = st.window.outer_position() {
                    state.window.set_outer_position(at);
                }
                state.window.set_visible(true);
                state.window.focus_window();
            }
            // (the menu takes over from the start-up screen: the same name in the same place)
            if let Some(start) = state.start.as_mut() {
                start.after_splash();
            }
            // (the game's clocks start now, not when it was made)
            state.last = Instant::now();
            state.started = state.last;
            self.state = Some(state);
            return;
        }
        // (a window not shown is never asked to draw itself)
        if self.opts.boot_test {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
            if let Err(e) = st.draw() {
                self.error = Some(e.to_string());
                event_loop.exit();
            }
        } else {
            st.window.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() || self.starting.is_some() {
            return;
        }
        if let Err(e) = self.start(event_loop) {
            self.error = Some(e.to_string());
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // the start-up screen: it is drawn, resized and shut; nothing else done to it does anything
        if let Some(st) = &mut self.starting {
            if id == st.window.id() {
                match event {
                    // (shut while the game is being made: out at once, the loader with it)
                    WindowEvent::CloseRequested => std::process::exit(0),
                    WindowEvent::Resized(size) => st.gpu.resize(size.width, size.height),
                    WindowEvent::RedrawRequested => {
                        if let Err(e) = st.draw() {
                            self.error = Some(e.to_string());
                            event_loop.exit();
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        let Some(s) = &mut self.state else { return };
        if s.ui.event(&s.window, &event) && !matches!(event, WindowEvent::RedrawRequested | WindowEvent::Resized(_)) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => s.renderer.resize(size.width, size.height),
            WindowEvent::Focused(false) => {
                s.blasts.release_all();
                s.capture_mouse(false);
                // (what a script holds is not the keyboard's to let go)
                if s.script.is_none() {
                    s.keys.fill(false);
                }
                s.jump = false;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    s.key(key, event.state == ElementState::Pressed, event.repeat);
                }
            }
            WindowEvent::CursorMoved { position, .. } => s.cursor = (position.x, position.y),
            // the editor: a click picks under the mouse; the right button held looks round
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } if s.editor.open && !s.captured => {
                if let Some(v) = s.last_view {
                    let size = s.window.inner_size();
                    let dir = Editor::mouse_ray(&v, s.cursor.0, s.cursor.1, (size.width, size.height));
                    s.editor.click(&s.ships, &s.builds, v.eye, dir);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } if s.editor.open => s.capture_mouse(state == ElementState::Pressed),
            // (the start menu's clicks are its own)
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } if s.start.is_some() => {}
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } if !s.ui.menu => {
                if !s.captured {
                    s.capture_mouse(true);
                } else if s.spawner.placing() {
                    let made = s.spawner.place(&mut s.ships, &mut s.builds, &s.world.bodies);
                    // (a ship made with others about is made on their copies too)
                    if let (Some(m), Some(id)) = (&mut s.multi, made)
                        && let Some(n) = s.ships.by_structure(id)
                    {
                        let kind = s.ships.list[n].kind.id.clone();
                        m.made(&kind, id, &s.builds.set, &s.ships);
                    }
                } else if s.gear.owns_click() {
                    // a tool whose click is its own (the welder): whatever is under it is its
                    // work, a door and a panel's controls too
                    s.gear.trigger = true;
                } else if !s.aboard.press(&mut s.ships, &s.builds.set) {
                    // nothing of a ship under the hand: the wrist computer if it is up, the tool
                    // in the hand, or bare, what is loose
                    if s.handwork.click() {
                    } else if s.gear.held.is_some() {
                        s.gear.trigger = true;
                    } else if let Some(v) = s.last_aim.or(s.last_view)
                        && let Err(why) = s.hands.grab(&s.builds.set, &s.ships, &v, s.pilot.ride.map(|r| r.id))
                    {
                        s.ui.hud.notice("manos", &format!("No se puede coger: {why}"), Level::Caution, 2.5);
                    }
                }
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                s.gear.trigger = false;
                s.hands.release(&mut s.builds);
                s.aboard.release(&mut s.ships, &s.builds.set);
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } if s.spawner.placing() => s.spawner.toggle(),
            // the right button: the tool's own use if it has one (the welder's view), else a closer look
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } => {
                if state == ElementState::Pressed && s.captured && s.gear.secondary() {
                    s.zooming = false;
                } else {
                    s.zooming = state == ElementState::Pressed;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(p) => p.y * 0.02,
                };
                let m = s.mods();
                if !s.hands.wheel(y) && !s.spawner.wheel(y, m.coarse) && !s.aboard.wheel(&mut s.ships, &s.builds.set, y as f32, m) && !s.chase.wheel(y) {
                    s.pilot.wheel(y);
                }
            }
            WindowEvent::RedrawRequested => match s.frame(&self.opts) {
                Ok(true) => {}
                Ok(false) => event_loop.exit(),
                Err(e) => {
                    self.error = Some(e.to_string());
                    event_loop.exit();
                }
            },
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let Some(s) = &mut self.state
            && s.captured
            && let DeviceEvent::MouseMotion { delta } = event
        {
            let m = s.mods();
            // (the gestures' wheel open: the mouse chooses in it)
            if s.handwork.mouse(delta.0, delta.1) {
            } else if !s.aboard.motion(&mut s.ships, &s.builds.set, delta.0, delta.1, m) {
                // zoomed in, the mouse turns the view as much less
                let c = s.ui.controls;
                let k = c.mouse / (1.0 + s.zoom * (ZOOM - 1.0));
                let (x, y) = (delta.0 * k, delta.1 * k * if c.invert_y { -1.0 } else { 1.0 });
                if s.look.held {
                    // (the head, or the camera from outside: the body stays as it is)
                    let r = s.pilot.mouse();
                    s.look.turn(x * r, y * r, s.chase.on);
                } else {
                    s.pilot.look(x, y);
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.starting.is_some() {
            self.started(event_loop);
            return;
        }
        let Some(s) = &mut self.state else { return };
        let unseen = self.opts.hidden() || self.opts.boot_test;
        if !unseen {
            s.window.request_redraw();
            return;
        }
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        if self.opts.boot_test {
            self.test_frames += 1;
            if self.test_frames == 41 {
                println!("arranque: el juego hecho en su hilo y {} fotogramas dibujados; menú de inicio {}", self.test_frames - 1, if s.start.is_some() { "puesto" } else { "quitado" });
                event_loop.exit();
                return;
            }
        }
        // (a window not shown is never asked to draw itself: its frames are made here)
        match s.frame(&self.opts) {
            Ok(true) => {}
            Ok(false) => event_loop.exit(),
            Err(e) => {
                self.error = Some(e.to_string());
                event_loop.exit();
            }
        }
    }
}

