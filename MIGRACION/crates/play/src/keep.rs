//! The game kept in two slots (`<partida>.a.bin`, `<partida>.b.bin`): each save goes over the
//! older of the two, written whole to a file of its own first and only then put in the slot's
//! place, so that a game stopped in the middle of one (or a full disk) leaves the other as it was.
//! At the start, the newest one that is whole is taken up (`save`). Whoever runs a game as a
//! server keeps it so: `luna-servidor`, and a game of one's own (`local`).
//!
//! `Keeper` keeps a game as it goes: every so often and when it ends, made into bytes on the
//! game's thread between two steps (a fraction of a millisecond for hundreds of structures) and
//! written to disk on a thread of its own, so no step waits for the disk.
use crate::{
    game::Game,
    host::{Host, HostConfig},
};
use lunar_core::scenario::PlayerDef;
use std::thread::JoinHandle;
use std::time::Instant;
use std::io;
use std::path::{Path, PathBuf};

pub struct Slots {
    paths: [PathBuf; 2],
    /// The slot that holds the newest game kept (the next goes in the other).
    newest: Option<usize>,
}

/// What one slot holds: the bytes and how many times that game had been kept, if it is whole.
fn read(path: &Path) -> Option<(Vec<u8>, u64)> {
    let data = std::fs::read(path).ok()?;
    let saves = crate::save::saves(&data)?;
    Some((data, saves))
}

impl Slots {
    /// The slots of `base` (a path with no extension: `partidas/partida`); its folder is made if
    /// it is not there.
    pub fn new(base: &Path) -> Slots {
        let name = base.file_name().map_or_else(|| "partida".into(), |n| n.to_string_lossy().into_owned());
        let dir = base.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Slots { paths: [dir.join(format!("{name}.a.bin")), dir.join(format!("{name}.b.bin"))], newest: None }
    }

    /// The newest game kept whole, and where it is.
    pub fn newest(&mut self) -> Option<(Vec<u8>, PathBuf)> {
        let a = read(&self.paths[0]);
        let b = read(&self.paths[1]);
        let k = match (&a, &b) {
            (Some(x), Some(y)) => usize::from(y.1 > x.1),
            (Some(_), None) => 0,
            (None, Some(_)) => 1,
            (None, None) => return None,
        };
        self.newest = Some(k);
        let (data, _) = [a, b].into_iter().nth(k).flatten()?;
        Some((data, self.paths[k].clone()))
    }

    /// The slots put aside (renamed `.vieja`): what they hold cannot be taken up by this server,
    /// and is not to be written over.
    pub fn put_aside(&mut self) -> Vec<PathBuf> {
        let mut moved = Vec::new();
        for p in &self.paths {
            if p.exists() {
                let mut old = p.clone().into_os_string();
                old.push(".vieja");
                if std::fs::rename(p, &old).is_ok() {
                    moved.push(PathBuf::from(old));
                }
            }
        }
        self.newest = None;
        moved
    }

    /// Where the next game kept goes (the slot that does not hold the newest), taken.
    pub fn next(&mut self) -> PathBuf {
        let k = self.newest.map_or(0, |k| 1 - k);
        self.newest = Some(k);
        self.paths[k].clone()
    }
}

/// `data` written whole to `path`: to a file beside it first, then put in its place.
pub fn write(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut part = path.as_os_str().to_owned();
    part.push(".parte");
    let part = PathBuf::from(part);
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&part)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&part, path)
}

/// Where and how often a game is kept.
pub struct Keeping {
    pub slots: Slots,
    /// Seconds between two saves (0: only when the game ends).
    pub every: f64,
    /// A new game, whatever is kept (what is kept is gone over by the saves of this one).
    pub fresh: bool,
}

/// The newest game kept in `keeping` (unless it asks for a new one) taken up in a game `make`
/// makes, as a server of `config`: none if there is none; one that cannot be taken up (another
/// version, other data) is put aside, not gone over. What happened is said to `say`.
pub fn take_up(keeping: &mut Keeping, make: impl Fn() -> Result<Game, String>, def: PlayerDef, config: &HostConfig, fingerprint: u32, mut say: impl FnMut(String)) -> Result<Option<Host>, String> {
    if keeping.fresh {
        return Ok(None);
    }
    let Some((bytes, at)) = keeping.slots.newest() else { return Ok(None) };
    let began = Instant::now();
    match Host::load(make()?, def, config.clone(), fingerprint, &bytes) {
        Ok(h) => {
            let waiting = h.waiting().count();
            say(format!(
                "Partida retomada de {} (guardada {}): paso {}, {} estructuras, {} esperando {:.0} s a que vuelvan; en {:.0} ms.",
                at.display(),
                if h.saves == 1 { "1 vez".to_string() } else { format!("{} veces", h.saves) },
                h.game.step,
                h.game.builds.set.list.len(),
                if waiting == 1 { "1 cuerpo".to_string() } else { format!("{waiting} cuerpos") },
                h.config.keep,
                began.elapsed().as_secs_f64() * 1000.0
            ));
            Ok(Some(h))
        }
        Err(e) => {
            let aside = keeping.slots.put_aside();
            say(format!("No se puede retomar la partida guardada en {}: {e}. Se empieza una nueva; la guardada queda aparte en {}.", at.display(), aside.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" y ")));
            Ok(None)
        }
    }
}

/// Keeping a game as it goes.
pub struct Keeper {
    keeping: Keeping,
    fingerprint: u32,
    writing: Option<JoinHandle<(PathBuf, std::io::Result<()>, f64)>>,
    due: f64,
}

impl Keeper {
    /// Keeping as `keeping` says a game of data `fingerprint`, from `now` (s).
    pub fn new(keeping: Keeping, fingerprint: u32, now: f64) -> Keeper {
        let due = now + if keeping.every > 0.0 { keeping.every } else { f64::INFINITY };
        Keeper { keeping, fingerprint, writing: None, due }
    }

    /// Between two steps: the game kept if it is due; what the last write did, said.
    pub fn tick(&mut self, host: &mut Host, now: f64, say: &mut impl FnMut(String)) {
        self.finish(false, say);
        if now >= self.due {
            self.due = now + self.keeping.every;
            self.save(host, false, say);
        }
    }

    /// The game kept now: made into bytes here, written on a thread of its own (or here and
    /// waited for, `wait`: when the game ends).
    pub fn save(&mut self, host: &mut Host, wait: bool, say: &mut impl FnMut(String)) {
        self.finish(true, say);
        let began = Instant::now();
        let mut bytes = Vec::new();
        host.save(self.fingerprint, &mut bytes);
        let made = began.elapsed().as_secs_f64() * 1000.0;
        let n = host.game.builds.set.list.len();
        let path = self.keeping.slots.next();
        let kb = format!("{:.1} kB, hecha en {made:.1} ms", bytes.len() as f64 / 1000.0).replace('.', ",");
        say(format!("Guardando la partida ({n} estructuras, {kb}) en {}…", path.display()));
        let job = move || {
            let began = Instant::now();
            let r = write(&path, &bytes);
            (path, r, began.elapsed().as_secs_f64() * 1000.0)
        };
        match std::thread::Builder::new().name("guardado".to_string()).spawn(job) {
            Ok(t) => self.writing = Some(t),
            Err(e) => say(format!("No se puede guardar la partida: {e}")),
        }
        if wait {
            self.finish(true, say);
        }
    }

    /// What the write under way did, said, if it is done (or waited for, `wait`).
    fn finish(&mut self, wait: bool, say: &mut impl FnMut(String)) {
        if self.writing.as_ref().is_some_and(|t| wait || t.is_finished())
            && let Some(t) = self.writing.take()
        {
            say(match t.join() {
                Ok((path, Ok(()), ms)) => format!("Partida guardada en {} ({ms:.0} ms en el disco).", path.display()),
                Ok((path, Err(e), _)) => format!("No se pudo guardar la partida en {}: {e}. La anterior sigue en la otra ranura.", path.display()),
                Err(_) => "No se pudo guardar la partida (el hilo del guardado se cayó).".to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_save_goes_over_the_older_and_the_newest_whole_one_is_taken() {
        let dir = std::env::temp_dir().join(format!("luna-ranuras-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut slots = Slots::new(&dir.join("partida"));
        assert!(slots.newest().is_none());
        // (what lunar_play::save writes, by hand: its magic, a count and a checksum)
        let kept = |saves: u64| {
            let mut b = b"LPAR".to_vec();
            b.extend_from_slice(&1u16.to_le_bytes());
            let build = crate::net::BUILD.as_bytes();
            b.extend_from_slice(&(build.len() as u16).to_le_bytes());
            b.extend_from_slice(build);
            b.extend_from_slice(&7u32.to_le_bytes());
            b.extend_from_slice(&saves.to_le_bytes());
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for &x in &b {
                h = (h ^ u64::from(x)).wrapping_mul(0x100_0000_01b3);
            }
            b.extend_from_slice(&h.to_le_bytes());
            b
        };
        let first = slots.next();
        write(&first, &kept(1)).unwrap();
        let second = slots.next();
        assert_ne!(first, second);
        write(&second, &kept(2)).unwrap();
        let third = slots.next();
        assert_eq!(third, first, "over the older one");
        // (one cut short: the other is taken)
        std::fs::write(&third, &kept(3)[..20]).unwrap();
        let mut again = Slots::new(&dir.join("partida"));
        let (data, at) = again.newest().unwrap();
        assert_eq!((crate::save::saves(&data), at), (Some(2), second.clone()));
        assert_eq!(again.next(), first, "the next goes over the one cut short");
        assert_eq!(again.put_aside().len(), 2);
        assert!(again.newest().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
