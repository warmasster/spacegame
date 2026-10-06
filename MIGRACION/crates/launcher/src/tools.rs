//! What the debug edition's tools are made of, found in the game's folder: the camera scripts and
//! what each says it does, the documents and their titles, the logs, the ship tools' server (MCP);
//! and a build's check of its own start.
use crate::{builds, launch, words};
use std::path::Path;

/// Where the documents are, from the game's folder.
pub const DOCS: &str = "docs";
/// Where the game leaves what it writes.
pub const OUT: &str = "out";
/// Where a script's pictures are left.
pub const SCRIPT_PHOTOS: &str = "out/camara";
/// The ship tools' server and the file that tells an assistant where it is.
pub const MCP: &str = "LunaMCP.exe";
pub const MCP_CONFIG: &str = ".mcp.json";
/// How long a build is given to check its start before it is stopped.
pub const BOOT_LIMIT: std::time::Duration = std::time::Duration::from_secs(180);

/// The pages of the tools' tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Scripts,
    Check,
    Docs,
    Logs,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Scripts, Page::Check, Page::Docs, Page::Logs];

    pub fn name(self) -> &'static str {
        match self {
            Page::Scripts => "GUIONES DE CÁMARA",
            Page::Check => "COMPROBAR",
            Page::Docs => "DOCUMENTOS",
            Page::Logs => "REGISTROS",
        }
    }
}

/// A camera script: its name (its file's, without the ending) and what it says it does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub name: String,
    pub says: String,
}

/// What a script says it does: the comment its file starts with, its lines put together, up to
/// where it tells how it is run ("Run: luna --guion …").
pub fn describe(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let comment: Vec<&str> = text.lines().map_while(|line| line.trim().strip_prefix("//")).map(|line| line.trim_start_matches('/').trim()).filter(|line| !line.is_empty()).collect();
    let mut said = comment.join(" ");
    for how in ["Run:", "Started with", "Ejecutar:"] {
        if let Some(at) = said.find(how) {
            said.truncate(at);
        }
    }
    said.trim_end().to_string()
}

/// The camera scripts of a folder (`tools/camara`): its `.jsonc` files, in order.
pub fn scripts(dir: &Path) -> Vec<Script> {
    let mut names: Vec<String> = builds::names(dir).into_iter().filter(|n| n.strip_suffix(".jsonc").is_some_and(|stem| !stem.is_empty())).collect();
    names.sort();
    names.dedup();
    names.into_iter().map(|file| Script { says: std::fs::read(dir.join(&file)).map(|bytes| describe(&String::from_utf8_lossy(&bytes))).unwrap_or_default(), name: file.strip_suffix(".jsonc").unwrap_or(&file).to_string() }).collect()
}

/// A document: its file and its title.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doc {
    pub file: String,
    pub title: String,
}

/// A Markdown text's title: its first heading, without its marks.
pub fn title(text: &str) -> Option<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.lines().map(str::trim).find_map(|l| l.strip_prefix('#')).map(|t| t.trim_start_matches('#').trim().to_string()).filter(|t| !t.is_empty())
}

/// The documents of a folder (`docs`): its `.md` files, in order, each with its title.
pub fn docs(dir: &Path) -> Vec<Doc> {
    let mut files: Vec<String> = builds::names(dir).into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".md")).collect();
    files.sort();
    files.into_iter().map(|file| Doc { title: std::fs::read(dir.join(&file)).ok().and_then(|bytes| title(&String::from_utf8_lossy(&bytes))).unwrap_or_default(), file }).collect()
}

/// A log: its file from the game's folder, its size, when it was last written and what its first
/// line says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub path: String,
    pub size: u64,
    pub modified: u64,
    pub first: String,
}

/// The most letters of a log's first line that are kept.
const FIRST_MOST: usize = 160;

/// The logs there are: those of `out` (`*.log`), the launcher's own and the server's; the one
/// written last, first.
pub fn logs(root: &Path) -> Vec<Log> {
    let out = builds::names(&root.join(OUT)).into_iter().filter(|n| n.ends_with(".log")).map(|n| format!("{OUT}/{n}"));
    let others = ["launcher_error.log".to_string(), format!("{}/{}", crate::server::FOLDER, crate::server::LOG)];
    let mut found: Vec<Log> = out
        .chain(others)
        .filter_map(|path| {
            let meta = std::fs::metadata(root.join(&path)).ok().filter(std::fs::Metadata::is_file)?;
            // (a small one says in its first line what went wrong; a long one is not read)
            let first = if meta.len() <= 65_536 { std::fs::read(root.join(&path)).ok().and_then(|bytes| words::first_line(&String::from_utf8_lossy(&bytes))).unwrap_or_default() } else { String::new() };
            Some(Log { path, size: meta.len(), modified: meta.modified().map_or(0, words::unix), first: first.chars().take(FIRST_MOST).collect() })
        })
        .collect();
    found.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.path.cmp(&b.path)));
    found
}

/// The ship tools' server: whether its program is there, whether the file that tells of it is,
/// and whether that file names the program.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mcp {
    pub program: bool,
    pub config: bool,
    pub wired: bool,
}

impl Mcp {
    pub fn look(root: &Path) -> Mcp {
        let config = std::fs::read(root.join(MCP_CONFIG)).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        Mcp { program: root.join(MCP).is_file(), config: config.is_some(), wired: config.is_some_and(|text| text.contains(MCP)) }
    }

    /// As it is said, and whether it is all there.
    pub fn says(self) -> (String, bool) {
        match (self.program, self.config, self.wired) {
            (true, true, true) => (format!("{MCP} está y {MCP_CONFIG} apunta a él: un asistente puede leer, accionar, validar y fotografiar naves."), true),
            (true, true, false) => (format!("{MCP} está, pero {MCP_CONFIG} no lo nombra."), false),
            (true, false, _) => (format!("{MCP} está, pero falta {MCP_CONFIG} (el fichero que le dice a un asistente dónde encontrarlo)."), false),
            (false, true, _) => (format!("Falta {MCP}; {MCP_CONFIG} sí está."), false),
            (false, false, _) => (format!("No están ni {MCP} ni {MCP_CONFIG}."), false),
        }
    }
}

/// What a build's check of its own start came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Check {
    /// The build, as it is told ("V35 · DEMO"), and its command line as it was typed.
    pub name: String,
    pub line: String,
    /// It started and ended well.
    pub good: bool,
    /// In a line: how it ended and how long it took.
    pub verdict: String,
    /// What it printed.
    pub output: String,
}

/// The most lines of what a build printed that are kept (the last ones).
const OUTPUT_MOST: usize = 40;

impl Check {
    /// The check of the build `name`, run with `line`, from what running it came to.
    pub fn of(name: &str, line: &str, run: std::io::Result<launch::Captured>) -> Check {
        let (good, verdict, output) = match run {
            Err(e) => (false, format!("No se pudo iniciar: {e}"), String::new()),
            Ok(run) => {
                let took = format!("{} s", words::decimal(f64::from(run.seconds), 1));
                let lines: Vec<&str> = run.output.lines().collect();
                let output = lines[lines.len().saturating_sub(OUTPUT_MOST)..].join("\n");
                match (run.cut, run.code) {
                    (true, _) => (false, format!("No terminó en {} y se ha parado.", words::span(BOOT_LIMIT.as_secs())), output),
                    (false, Some(0)) => (true, format!("Arranca bien ({took})."), output),
                    (false, Some(code)) => (false, format!("Falla al arrancar: código {code} ({took})."), output),
                    (false, None) => (false, format!("Se cerró sin código de salida ({took})."), output),
                }
            }
        };
        Check { name: name.to_string(), line: line.to_string(), good, verdict, output }
    }

    /// As it is copied: the build, its command line, the verdict and what it printed.
    pub fn text(&self) -> String {
        format!("{}\n{}\n{}\n{}", self.name, self.line, self.verdict, self.output).trim_end().to_string()
    }
}

/// Run a build's check of its own start and tell what it came to. (This waits for the build, some
/// ten seconds: it is for a thread of its own.)
pub fn check(root: &Path, name: &str, file: &str, args: &[String]) -> Check {
    Check::of(name, &format!("{file} {}", crate::options::as_typed(args)), launch::captured(root, file, args, BOOT_LIMIT))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("luna-launcher-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_script_says_what_it_does_in_the_comment_it_starts_with() {
        assert_eq!(
            describe("// The Abejorro, the one-seat cargo tug: outside, from its seat.\n// Run: luna --guion tools/camara/abejorro.jsonc → out/camara/abejorro_*.png\n{\n  \"pasos\": []\n}\n"),
            "The Abejorro, the one-seat cargo tug: outside, from its seat."
        );
        assert_eq!(
            describe("\u{feff}// The hold's cargo and its clamps: each piece held by its clamp\n//   (the rail across the deck).\n//\n// Then a rocket.\n// Run: luna\n{}"),
            "The hold's cargo and its clamps: each piece held by its clamp (the rail across the deck). Then a rocket."
        );
        assert_eq!(describe("// Multiplayer, the first of two players.\n// Started with: luna --servidor 127.0.0.1:47611\n{}"), "Multiplayer, the first of two players.");
        // (how it is run may follow on the same line, and more comment after it)
        assert_eq!(
            describe("// A look round the Alcotán: the hold; then the\n// ramp closing and the air. Run: luna --guion tools/camara/alcotan_revista.jsonc\n// Pictures in out/camara/.\n{}"),
            "A look round the Alcotán: the hold; then the ramp closing and the air."
        );
        assert_eq!(describe("{\n  // un comentario de dentro\n}"), "");
        assert_eq!(describe(""), "");
    }

    #[test]
    fn the_scripts_of_a_folder_are_its_jsonc_files_in_order_each_with_what_it_says() {
        let dir = temp("guiones");
        for (file, text) in [("hud.jsonc", "// The HUD.\n{}"), ("hoja.py", "# no"), ("abordo.jsonc", "{}"), (".jsonc", "// nada"), ("notas.json", "// no"), ("carga.jsonc", "// Cargo.\n// Run: x\n{}")] {
            std::fs::write(dir.join(file), text).unwrap();
        }
        let script = |name: &str, says: &str| Script { name: name.to_string(), says: says.to_string() };
        assert_eq!(scripts(&dir), [script("abordo", ""), script("carga", "Cargo."), script("hud", "The HUD.")]);
        assert!(scripts(&dir.join("no_existe")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_document_s_title_is_its_first_heading() {
        assert_eq!(title("# Naves (prototipo en Rust)\n\n## Dos versiones\n").as_deref(), Some("Naves (prototipo en Rust)"));
        assert_eq!(title("\u{feff}\n\ntexto antes\n  ## Aire: pasarlo  \n").as_deref(), Some("Aire: pasarlo"));
        assert_eq!(title("sin encabezado\n"), None);
        assert_eq!(title("#\n"), None);
        let dir = temp("docs");
        std::fs::write(dir.join("NAVES.md"), "# Naves\n").unwrap();
        std::fs::write(dir.join("AIRE.MD"), "nada\n").unwrap();
        std::fs::write(dir.join("notas.txt"), "# no\n").unwrap();
        assert_eq!(docs(&dir), [Doc { file: "AIRE.MD".into(), title: String::new() }, Doc { file: "NAVES.md".into(), title: "Naves".into() }]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_logs_are_those_of_out_the_launcher_s_and_the_server_s() {
        let root = temp("registros");
        assert!(logs(&root).is_empty());
        std::fs::create_dir_all(root.join("out/camara")).unwrap();
        std::fs::create_dir_all(root.join("servidores")).unwrap();
        std::fs::write(root.join("out/error.log"), "\nopción desconocida: --x\n\nLUNA\n").unwrap();
        std::fs::write(root.join("out/arranque.json"), "{}").unwrap();
        std::fs::write(root.join("out/camara/carga.log"), "no cuenta").unwrap();
        std::fs::write(root.join("launcher_error.log"), "").unwrap();
        std::fs::write(root.join("servidores/servidor.log"), "Servidor en marcha\n").unwrap();
        let mut found = logs(&root);
        found.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(found.iter().map(|l| (l.path.as_str(), l.size, l.first.as_str())).collect::<Vec<_>>(), [("launcher_error.log", 0, ""), ("out/error.log", 32, "opción desconocida: --x"), ("servidores/servidor.log", 19, "Servidor en marcha")]);
        assert!(found.iter().all(|l| l.modified > 1_700_000_000));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_ship_tools_server_is_there_when_its_program_and_its_file_are() {
        let root = temp("mcp");
        assert_eq!(Mcp::look(&root), Mcp::default());
        assert!(!Mcp::look(&root).says().1);
        std::fs::write(root.join(MCP), "").unwrap();
        assert_eq!(Mcp::look(&root), Mcp { program: true, config: false, wired: false });
        std::fs::write(root.join(MCP_CONFIG), "{ \"mcpServers\": {} }").unwrap();
        assert_eq!(Mcp::look(&root), Mcp { program: true, config: true, wired: false });
        assert!(Mcp::look(&root).says().0.contains("no lo nombra"));
        std::fs::write(root.join(MCP_CONFIG), "{ \"mcpServers\": { \"luna-naves\": { \"command\": \"C:/juego/LunaMCP.exe\" } } }").unwrap();
        let all = Mcp::look(&root);
        assert_eq!((all, all.says().1), (Mcp { program: true, config: true, wired: true }, true));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_check_tells_how_the_start_went() {
        let run = |code, output: &str, cut| Ok(launch::Captured { code, output: output.to_string(), seconds: 8.04, cut });
        let good = Check::of("V35 · DEMO", "LunaV35_demo.exe --prueba-arranque", run(Some(0), "arranque: el juego hecho en su hilo y 40 fotogramas dibujados; menú de inicio puesto", false));
        assert_eq!((good.good, good.verdict.as_str()), (true, "Arranca bien (8,0 s)."));
        assert_eq!(good.text(), "V35 · DEMO\nLunaV35_demo.exe --prueba-arranque\nArranca bien (8,0 s).\narranque: el juego hecho en su hilo y 40 fotogramas dibujados; menú de inicio puesto");
        let bad = Check::of("V35 · DEBUG", "x", run(Some(1), "opción desconocida: --x", false));
        assert_eq!((bad.good, bad.verdict.as_str(), bad.output.as_str()), (false, "Falla al arrancar: código 1 (8,0 s).", "opción desconocida: --x"));
        assert_eq!(Check::of("a", "b", run(None, "", true)).verdict, "No terminó en 3 min y se ha parado.");
        assert_eq!(Check::of("a", "b", run(None, "", false)).verdict, "Se cerró sin código de salida (8,0 s).");
        let none = Check::of("a", "b", Err(std::io::Error::other("no está")));
        assert_eq!((none.good, none.verdict.as_str(), none.text().as_str()), (false, "No se pudo iniciar: no está", "a\nb\nNo se pudo iniciar: no está"));
        // (of a long output, the last lines)
        let long: String = (1..=100).map(|n| format!("línea {n}\n")).collect();
        let cut = Check::of("a", "b", run(Some(0), &long, false));
        assert_eq!(cut.output.lines().count(), 40);
        assert!(cut.output.starts_with("línea 61\n") && cut.output.ends_with("línea 100"));
    }

    /// (Run by hand, once, `--ignored --nocapture`: the game of the working tree checks its own
    /// start, with no window; it takes what the game takes to load.)
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn the_game_checks_its_own_start() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let file = "target/release/lunar-app.exe";
        if !root.join(file).is_file() {
            println!("no hay {file}: nada que comprobar");
            return;
        }
        let done = check(&root, "lunar-app", file, &["--prueba-arranque".to_string()]);
        println!("{}", done.text());
        assert!(done.good, "{done:?}");
    }

    /// The system's `cmd` stands for the build (never the real game).
    #[cfg(windows)]
    #[test]
    fn a_check_runs_the_build_and_keeps_what_it_printed() {
        let shell = std::path::PathBuf::from(std::env::var("ComSpec").unwrap());
        let (root, file) = (shell.parent().unwrap(), shell.file_name().unwrap().to_str().unwrap());
        let done = check(root, "V35 · DEMO", file, &["/C".to_string(), "echo arranque: bien".to_string()]);
        assert!(done.good && done.output == "arranque: bien" && done.verdict.starts_with("Arranca bien ("), "{done:?}");
        assert!(done.line.ends_with("/C \"echo arranque: bien\""), "{}", done.line);
        let failed = check(root, "V35 · DEMO", "no_existe.exe", &[]);
        assert!(!failed.good && failed.verdict.starts_with("No se pudo iniciar: "), "{failed:?}");
    }
}
