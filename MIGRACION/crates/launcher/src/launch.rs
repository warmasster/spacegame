//! Starting things: a build of the game (to play, or with what it prints kept), the server beside
//! the builds, the system's own program for a file or a folder, and the small programs of the
//! system the launcher asks things of (none of them ever shows a console window).
use std::{
    io::{self, Read},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// A console program started with this shows no console of its own.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A command that shows no console window.
fn quiet(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Start a build of the game (`file`: its path from the game's folder) with these arguments, the
/// game's folder being the one it works in. It runs on its own from then on: what is returned
/// only serves to see whether it is still up and how it ended. (The game has its own window; a
/// build that were a console program would get no console from the launcher.)
pub fn start(folder: &Path, file: &str, args: &[String]) -> io::Result<Child> {
    quiet(folder.join(file)).args(args).current_dir(folder).spawn()
}

/// What a program that was run to its end came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Captured {
    /// Its exit code; none if the system gave none (it was stopped).
    pub code: Option<i32>,
    /// What it printed: its output, then its errors.
    pub output: String,
    pub seconds: f32,
    /// It did not end in the time it was given, and was stopped.
    pub cut: bool,
}

/// Everything a pipe gives, read on a thread of its own (a program that fills one pipe while the
/// other is being read would wait for ever).
fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Run a program of `folder` to its end with no window of its own, keeping what it prints; it is
/// stopped if it has not ended after `limit`. (This waits: it is for a thread of its own.)
pub fn captured(folder: &Path, file: &str, args: &[String], limit: Duration) -> io::Result<Captured> {
    let started = Instant::now();
    let mut child = quiet(folder.join(file)).args(args).current_dir(folder).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let (out, err) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let mut cut = false;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None if started.elapsed() >= limit => {
                cut = true;
                let _ = child.kill();
                break child.wait().ok();
            }
            None => std::thread::sleep(Duration::from_millis(40)),
        }
    };
    let seconds = started.elapsed().as_secs_f32();
    let text = |bytes: std::thread::Result<Vec<u8>>| String::from_utf8_lossy(&bytes.unwrap_or_default()).trim().to_string();
    let (out, err) = (text(out.join()), text(err.join()));
    let output = [out, err].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
    Ok(Captured { code: status.and_then(|s| s.code()), output, seconds, cut })
}

/// What a small program of the system prints (`cmd /C ver`), run with no window; none if it
/// could not be run. (This waits for it: it is for a thread of its own.)
pub fn said(program: &str, args: &[&str]) -> Option<String> {
    let output = quiet(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Start a small program of the system with no window and leave it to what it does: nobody waits
/// for it, nor hears what it says.
pub fn quietly(program: &str, args: &[&str]) -> io::Result<()> {
    quiet(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(drop)
}

/// Open a file or a folder with the system's program for it (the notepad, the Explorer):
/// `cmd /C start "" "<path>"`.
#[cfg(windows)]
pub fn open(path: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    // (the path always between quotes: `cmd` would take a `&` or a `^` in it as its own)
    quiet("cmd").args(["/C", "start", ""]).raw_arg(format!("\"{}\"", path.display())).spawn().map(drop)
}

#[cfg(not(windows))]
pub fn open(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Open a folder, making it first if it is not there.
pub fn open_folder(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)?;
    open(path)
}

/// What `cmd` is told to start a console program of `folder` in a console window of its own, that
/// folder being the one it works in (a server finds its config there):
/// `/C start "" /D "<folder>" "<program>"`.
pub fn server_line(folder: &Path, program: &str) -> String {
    format!("/C start \"\" /D \"{}\" \"{program}\"", folder.display())
}

/// The command that starts the server `program` of `folder` in its own console window; it is
/// `cmd`, in that folder too (it is where it looks for the program).
#[cfg(windows)]
fn server_command(folder: &Path, program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = quiet("cmd");
    command.raw_arg(server_line(folder, program)).current_dir(folder);
    command
}

/// Start the server `program` of `folder` in a console window of its own. It runs on its own from
/// then on: closing its window stops it.
#[cfg(windows)]
pub fn start_server(folder: &Path, program: &str) -> io::Result<()> {
    server_command(folder, program).spawn().map(drop)
}

#[cfg(not(windows))]
pub fn start_server(_folder: &Path, _program: &str) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_is_started_in_its_own_window_and_its_own_folder() {
        let folder = Path::new("C:\\Juegos\\LUNA & más\\servidores");
        assert_eq!(server_line(folder, "LunaServidor.exe"), "/C start \"\" /D \"C:\\Juegos\\LUNA & más\\servidores\" \"LunaServidor.exe\"");
    }

    /// (The command is only made and looked at: nothing is started.)
    #[cfg(windows)]
    #[test]
    fn the_server_s_command_is_cmd_in_the_server_s_folder() {
        let folder = Path::new("C:\\Juegos\\LUNA\\servidores");
        let command = server_command(folder, "LunaServidor.exe");
        assert_eq!(command.get_program(), "cmd");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["/C start \"\" /D \"C:\\Juegos\\LUNA\\servidores\" \"LunaServidor.exe\""]);
        assert_eq!(command.get_current_dir(), Some(folder));
    }

    /// The system's `cmd` stands for the game (never the real one): its folder and its file.
    #[cfg(windows)]
    fn shell() -> (std::path::PathBuf, String) {
        let shell = std::path::PathBuf::from(std::env::var("ComSpec").unwrap());
        (shell.parent().unwrap().to_path_buf(), shell.file_name().unwrap().to_str().unwrap().to_string())
    }

    #[cfg(windows)]
    #[test]
    fn what_a_program_prints_and_how_it_ends_are_kept() {
        let (folder, file) = shell();
        let run = |line: &str| captured(&folder, &file, &["/C".to_string(), line.to_string()], Duration::from_secs(30)).unwrap();
        let done = run("echo arranque: bien& echo aviso 1>&2& exit 0");
        assert_eq!((done.code, done.output.as_str(), done.cut), (Some(0), "arranque: bien\naviso", false));
        let failed = run("echo opcion desconocida 1>&2& exit 3");
        assert_eq!((failed.code, failed.output.as_str(), failed.cut), (Some(3), "opcion desconocida", false));
        assert!(captured(&folder, "no_existe.exe", &[], Duration::from_secs(1)).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn a_program_that_does_not_end_is_stopped() {
        let (folder, _) = shell();
        // (a minute of asking this same machine whether it is there: nothing leaves it)
        let started = Instant::now();
        let stuck = captured(&folder, "PING.EXE", &["-n".to_string(), "60".to_string(), "127.0.0.1".to_string()], Duration::from_millis(1500)).unwrap();
        // (what it had printed until then is kept; on a machine too busy for it to print anything, nothing)
        assert!(stuck.cut && (stuck.output.is_empty() || stuck.output.contains("127.0.0.1")), "{stuck:?}");
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[cfg(windows)]
    #[test]
    fn a_small_program_of_the_system_says_its_piece() {
        assert_eq!(said("cmd", &["/C", "echo", "hola"]).as_deref().map(str::trim), Some("hola"));
        assert_eq!(said("no_existe_este_programa", &[]), None);
    }
}
