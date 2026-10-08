//! The game kept in two slots (`<partida>.a.bin`, `<partida>.b.bin`): each save goes over the
//! older of the two, written whole to a file of its own first and only then put in the slot's
//! place, so that a server stopped in the middle of one (or a full disk) leaves the other as it
//! was. At the start, the newest one that is whole is taken up (`lunar_play::save`).
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
    let saves = lunar_play::save::saves(&data)?;
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
            let build = lunar_play::net::BUILD.as_bytes();
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
        assert_eq!((lunar_play::save::saves(&data), at), (Some(2), second.clone()));
        assert_eq!(again.next(), first, "the next goes over the one cut short");
        assert_eq!(again.put_aside().len(), 2);
        assert!(again.newest().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
