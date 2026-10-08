//! Command line: `luna [--ships N] [--npcs N] [--radius M] [--profile NAME] [--bench SECONDS]
//! [--out FILE] [--vsync on|off] [--scenario 10,30,...] [--backend dx12|vulkan] [--shot FILE] [--explode ID]
//! [--look KIND@DIST,AZ,EL]`.
use lunar_core::quality::Preset;
use lunar_render::wgpu;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Options {
    /// Overrides of the scenario's counts and radius.
    pub ships: Option<usize>,
    pub flying: Option<usize>,
    pub npcs: Option<usize>,
    pub radius: Option<f64>,
    pub preset: Option<Preset>,
    pub bench: Option<f64>,
    pub out: PathBuf,
    pub vsync: Option<bool>,
    pub scenarios: Vec<usize>,
    pub backend: Option<wgpu::Backends>,
    pub shot: Option<PathBuf>,
    pub shot_at: f64,
    /// Explosion fired ahead of the camera at the start.
    pub explode: Option<String>,
    /// Camera fixed on the first parked ship of a fleet kind: (kind, distance m, azimuth and
    /// elevation in degrees round the ship, 0 = ahead of its nose).
    pub look: Option<(u8, f64, f64, f64)>,
    pub gpu_test: bool,
    /// A camera script (`script`): steps, pictures, then out.
    pub script: Option<PathBuf>,
    /// No sound.
    pub mute: bool,
    /// Open the sound card, say what it is and leave (nothing is played).
    pub sound_test: bool,
    /// The whole screen (true) or a window (false); as the game was left if none.
    pub fullscreen: Option<bool>,
    /// The window's size (px).
    pub window: Option<(u32, u32)>,
    /// Straight into play: no start menu.
    pub no_menu: bool,
    /// The server to play with others through (`host:port`) and the name to go by there.
    pub server: Option<String>,
    pub name: Option<String>,
    /// Without a server, the game alone as before (`--directo`): no server in the process.
    pub direct: bool,
    /// The game of one's own hosted for others at this UDP port (`--anfitrion`).
    pub host: Option<u16>,
    /// A game of one's own begun anew, whatever is kept (`--nueva`).
    pub fresh: bool,
    /// Pictures of the start-up screen at a few moments of it, into this folder, and out (no
    /// window is shown, nothing is loaded).
    pub splash_test: Option<PathBuf>,
    /// A start as a player's (the game made on its thread behind the start-up screen, then its
    /// window in that one's place) with neither window shown; a few frames of the game, and out.
    pub boot_test: bool,
    /// The window shown even for a script, a bench or a picture (they run with none to be seen
    /// unless this is asked: nothing comes up on the desktop of whoever is at it).
    pub visible: bool,
}

pub const SCENARIOS: [usize; 6] = [10, 30, 60, 100, 300, 1000];

pub const HELP: &str = "SELENE (migración Rust)
  luna                         jugar
  --ships N --npcs N           naves y NPC (por defecto, los de assets/defs/scenario.jsonc)
  --flying N                   cuántas de las naves vuelan (por defecto, la parte del escenario)
  --radius M                   radio de la flota en metros (por defecto, el del escenario)
  --profile NOMBRE             horrible|muybaja|baja|media|alta|muyalta|ultra|esplendidos (o low|high)
  --vsync on|off               sincronía vertical (por defecto: off en bench, on al jugar)
  --bench SEGUNDOS             recorrido fijo y JSON con los tiempos
  --scenario 10,30,60,...      bench de varios tamaños (naves = NPC); 'all' = 10..1000
  --out FICHERO                JSON del bench (out/bench.json)
  --backend dx12|vulkan        forzar API gráfica
  --shot FICHERO.png           guarda una captura y sale (--shot-at SEGUNDOS, por defecto 8)
  --explode ID[@M]             hace estallar la explosión ID (assets/defs/explosions) M metros delante (40) al empezar
  --guion FICHERO.jsonc        guion de cámara: coloca la cámara (en la nave, ante un mando, una pieza),
                               acciona mandos, espera, hace fotos y vuelca señales; sale al acabar
                               (ejemplos en tools/camara)
  --pantalla completa|ventana  a pantalla completa o en ventana
  --ventana ANCHOxALTO         tamaño de la ventana (1600x900)
  --sin-menu                   sin menú de inicio: directo a jugar
  --servidor HOST:PUERTO       jugar con otros a través de ese servidor (edición multijugador; servidores/LunaServidor.exe)
  --nombre NOMBRE              cómo te llamas en el servidor
  --anfitrion PUERTO           alojar la partida propia: otros entran con --servidor ESTA_IP:PUERTO
  --directo                    sin conexión, el juego solo, sin el servidor dentro del proceso (como antes)
  --nueva                      empezar una partida propia nueva aunque haya una guardada (partidas/propia)
  --prueba-carga CARPETA       fotos de la pantalla de carga en varios momentos, sin abrir ventana ni cargar nada
  --prueba-arranque            arranca como para jugar (carga en su hilo tras la pantalla de carga) sin enseñar ventana, hace unos fotogramas y sale
  --visible                    con --guion, --bench o --shot: enseña la ventana (por defecto corren sin ventana a la vista)
  --mudo                       sin sonido (los guiones y el bench van siempre sin él)
  --prueba-sonido              abre la tarjeta de sonido, dice cuál es y sale (no suena nada)
  --look TIPO@D,AZ,EL          cámara fija en la primera nave aparcada del TIPO (0, 1, 2 de la flota) a D m,
                               rumbo AZ y elevación EL en grados (0 = delante del morro); con --shot, para revisarlas";

impl Options {
    /// Whether this run shows no window: a script, a bench or a picture taken, unless asked to
    /// be seen.
    pub fn hidden(&self) -> bool {
        !self.visible && (self.script.is_some() || self.bench.is_some() || self.shot.is_some())
    }

    /// The game of one's own goes through a server in the process (`lunar_play::local`): not with
    /// a server elsewhere, nor for the tools that work the world by hand (a script, a bench, a
    /// picture, a fixed camera, an explosion at the start), nor if asked not to (`--directo`).
    pub fn local(&self) -> bool {
        self.server.is_none() && !self.direct && self.script.is_none() && self.bench.is_none() && self.shot.is_none() && self.look.is_none() && self.explode.is_none() && self.scenarios.is_empty() && !self.gpu_test
    }

    pub fn parse(args: &[String]) -> Result<Options, String> {
        let mut o = Options {
            ships: None,
            flying: None,
            npcs: None,
            radius: None,
            preset: None,
            bench: None,
            out: PathBuf::from("out/bench.json"),
            vsync: None,
            scenarios: Vec::new(),
            backend: None,
            shot: None,
            shot_at: 8.0,
            explode: None,
            look: None,
            gpu_test: false,
            script: None,
            mute: false,
            sound_test: false,
            fullscreen: None,
            window: None,
            no_menu: false,
            server: None,
            name: None,
            direct: false,
            host: None,
            fresh: false,
            splash_test: None,
            boot_test: false,
            visible: false,
        };
        let mut i = 0;
        while i < args.len() {
            let flag = args[i].as_str();
            let mut value = || -> Result<&String, String> {
                i += 1;
                args.get(i).ok_or(format!("falta el valor de {flag}"))
            };
            match flag {
                "play" => {}
                "--ships" => o.ships = Some(value()?.parse().map_err(|e| format!("--ships: {e}"))?),
                "--flying" => o.flying = Some(value()?.parse().map_err(|e| format!("--flying: {e}"))?),
                "--npcs" => o.npcs = Some(value()?.parse().map_err(|e| format!("--npcs: {e}"))?),
                "--radius" => o.radius = Some(value()?.parse().map_err(|e| format!("--radius: {e}"))?),
                "--profile" | "--preset" => {
                    let v = value()?;
                    o.preset = Some(Preset::parse(v).ok_or(format!("perfil desconocido: {v}"))?);
                }
                "--bench" => o.bench = Some(value()?.parse().map_err(|e| format!("--bench: {e}"))?),
                "--out" => o.out = PathBuf::from(value()?),
                "--vsync" => o.vsync = Some(value()? == "on"),
                "--scenario" => {
                    let v = value()?;
                    o.scenarios = if v == "all" {
                        SCENARIOS.to_vec()
                    } else {
                        v.split(',').map(|s| s.trim().parse().map_err(|e| format!("--scenario: {e}"))).collect::<Result<_, _>>()?
                    };
                }
                "--backend" => {
                    o.backend = Some(match value()?.as_str() {
                        "dx12" => wgpu::Backends::DX12,
                        "vulkan" => wgpu::Backends::VULKAN,
                        v => return Err(format!("backend desconocido: {v}")),
                    })
                }
                "--shot" => o.shot = Some(PathBuf::from(value()?)),
                "--shot-at" => o.shot_at = value()?.parse().map_err(|e| format!("--shot-at: {e}"))?,
                "--explode" => o.explode = Some(value()?.clone()),
                "--look" => {
                    let v = value()?;
                    let bad = || format!("--look: TIPO@D,AZ,EL, no '{v}'");
                    let (kind, rest) = v.split_once('@').ok_or_else(bad)?;
                    let n: Vec<f64> = rest.split(',').map(|x| x.trim().parse().map_err(|_| bad())).collect::<Result<_, _>>()?;
                    let [d, az, el] = n[..] else { return Err(bad()) };
                    o.look = Some((kind.parse().map_err(|_| bad())?, d, az, el));
                }
                "--pantalla" => {
                    o.fullscreen = Some(match value()?.as_str() {
                        "completa" => true,
                        "ventana" => false,
                        v => return Err(format!("--pantalla: completa o ventana, no '{v}'")),
                    })
                }
                "--ventana" => {
                    let v = value()?;
                    let size = v.split_once(['x', 'X']).and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)));
                    o.window = Some(size.filter(|(w, h): &(u32, u32)| *w >= 320 && *h >= 200).ok_or(format!("--ventana: ANCHOxALTO, no '{v}'"))?);
                }
                "--sin-menu" => o.no_menu = true,
                "--servidor" => {
                    if crate::DEMO && !crate::MULTI {
                        return Err("esta edición no es multijugador: usa LunaV…_multiplayer.exe".into());
                    }
                    o.server = Some(value()?.clone());
                }
                "--nombre" => o.name = Some(value()?.clone()),
                "--relevo" => return Err("--relevo ya no existe: todo servidor tiene la partida".into()),
                "--directo" => o.direct = true,
                "--nueva" => o.fresh = true,
                "--anfitrion" => {
                    let v = value()?;
                    o.host = Some(v.parse::<u16>().ok().filter(|p| *p != 0).ok_or(format!("--anfitrion: un puerto (1 a 65535), no '{v}'"))?);
                }
                "--visible" => o.visible = true,
                "--prueba-carga" => o.splash_test = Some(PathBuf::from(value()?)),
                "--prueba-arranque" => o.boot_test = true,
                "--gpu-test" => o.gpu_test = true,
                "--mudo" | "--mute" => o.mute = true,
                "--prueba-sonido" => o.sound_test = true,
                "--guion" | "--script" => o.script = Some(PathBuf::from(value()?)),
                "-h" | "--help" => return Err(HELP.into()),
                _ => return Err(format!("opción desconocida: {flag}\n\n{HELP}")),
            }
            i += 1;
        }
        if !o.scenarios.is_empty() && o.bench.is_none() {
            o.bench = Some(30.0);
        }
        Ok(o)
    }
}
