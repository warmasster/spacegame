//! What the server says: every line goes to the console and, with its date, to the end of
//! `servidor.log`. Times are UTC (the standard library does not know the local time zone) and say so.
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const FILE: &str = "servidor.log";

pub struct Journal {
    file: Option<File>,
}

/// Seconds since 1970 to (year, month, day, hour, minute, second), UTC.
pub fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Days to a date in the Gregorian calendar (Howard Hinnant's algorithm): eras of 400 years from 0000-03-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day, (rest / 3600) as u32, (rest / 60 % 60) as u32, (rest % 60) as u32)
}

impl Journal {
    /// Opens (or starts) the log in `dir`. If it cannot be written, the server only speaks on the console.
    pub fn open(dir: &Path) -> Journal {
        Journal { file: OpenOptions::new().create(true).append(true).open(dir.join(FILE)).ok() }
    }
    pub fn say(&mut self, text: &str) {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let (y, mo, d, h, mi, s) = civil(secs);
        println!("[{h:02}:{mi:02}:{s:02} UTC] {text}");
        if let Some(f) = &mut self.file {
            // A disk that fills up must not stop the server: the line is lost and that is all.
            let _ = writeln!(f, "{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC  {text}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::civil;

    #[test]
    fn seconds_become_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
        assert_eq!(civil(1_791_112_496), (2026, 10, 4, 11, 14, 56));
        assert_eq!(civil(1_798_761_599), (2026, 12, 31, 23, 59, 59));
        assert_eq!(civil(1_798_761_600), (2027, 1, 1, 0, 0, 0));
        assert_eq!(civil(4_107_542_400), (2100, 3, 1, 0, 0, 0));
    }
}
