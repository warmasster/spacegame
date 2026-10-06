//! How things are said: a moment as a date of this machine's clock, a time as a span, a size in
//! megabytes, a count with its noun, a path cut to the room there is. There is nothing behind it
//! (no calendar crate): the date is worked out from the seconds since 1970, and the clock's
//! distance from UTC is asked of the system once.
use std::time::{SystemTime, UNIX_EPOCH};

/// The seconds since 1970 (UTC) of a moment.
pub fn unix(moment: SystemTime) -> u64 {
    moment.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn now() -> u64 {
    unix(SystemTime::now())
}

/// The year, the month (1 to 12) and the day (1 to 31) of a day counted from 1970-01-01.
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let (era, of_era) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let day = (of_year - (153 * shifted + 2) / 5 + 1) as u32;
    let month = (if shifted < 10 { shifted + 3 } else { shifted - 9 }) as u32;
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

const MONTHS: [&str; 12] = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];

/// This machine's clock: how far it is from UTC.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// Seconds east of UTC.
    pub offset: i32,
}

impl Clock {
    /// The day counted from 1970 and the second of that day, on this clock.
    fn local(self, unix: u64) -> (i64, i64) {
        let seconds = unix as i64 + i64::from(self.offset);
        (seconds.div_euclid(86_400), seconds.rem_euclid(86_400))
    }

    /// "18:46".
    pub fn hour(self, unix: u64) -> String {
        let second = self.local(unix).1;
        format!("{}:{:02}", second / 3600, second % 3600 / 60)
    }

    /// "4 oct 18:46"; with its year if it is not this one ("4 oct 2025 18:46").
    pub fn moment(self, unix: u64, now: u64) -> String {
        let ((year, month, day), this) = (civil(self.local(unix).0), civil(self.local(now).0).0);
        let year = if year == this { String::new() } else { format!(" {year}") };
        format!("{day} {}{year} {}", MONTHS[month as usize - 1], self.hour(unix))
    }

    /// How long ago, as it is said: "hoy 18:46", "ayer 21:10", "hace 3 días", and from a week on
    /// its date.
    pub fn ago(self, unix: u64, now: u64) -> String {
        match self.local(now).0 - self.local(unix).0 {
            0 => format!("hoy {}", self.hour(unix)),
            1 => format!("ayer {}", self.hour(unix)),
            days @ 2..=6 => format!("hace {days} días"),
            _ => self.moment(unix, now),
        }
    }
}

/// The hour and the minute a clock's text starts with (`cmd`'s `%TIME%`: " 9:05:03,12"): its
/// first two numbers, whatever stands between them.
pub fn hour_minute(text: &str) -> Option<(u32, u32)> {
    let mut numbers = text.split(|c: char| !c.is_ascii_digit()).filter(|n| !n.is_empty()).map(str::parse::<u32>);
    let (hour, minute) = (numbers.next()?.ok()?, numbers.next()?.ok()?);
    (hour < 24 && minute < 60).then_some((hour, minute))
}

/// How far east of UTC (seconds) a clock that says `local` (hour, minute) is at the moment `unix`:
/// the nearest quarter of an hour, between twelve hours behind and fourteen ahead.
pub fn offset(local: (u32, u32), unix: u64) -> i32 {
    let utc = (unix / 60 % 1440) as i32;
    let apart = (local.0 * 60 + local.1) as i32 - utc;
    let mut quarters = (apart as f32 / 15.0).round() as i32 * 15;
    if quarters > 14 * 60 {
        quarters -= 1440;
    } else if quarters < -12 * 60 {
        quarters += 1440;
    }
    quarters * 60
}

/// This machine's clock, asked of the system (`cmd /C echo %TIME%`, with no window): UTC if it
/// does not say.
pub fn clock() -> Clock {
    let said = crate::launch::said("cmd", &["/C", "echo", "%TIME%"]);
    Clock { offset: said.as_deref().and_then(hour_minute).map_or(0, |local| offset(local, now())) }
}

/// A time as it is said: "45 s", "12 min", "3 h 20 min", "5 h".
pub fn span(seconds: u64) -> String {
    match (seconds / 3600, seconds % 3600 / 60) {
        (0, 0) => format!("{seconds} s"),
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes} min"),
    }
}

/// A decimal number the Spanish way: "22,1".
pub fn decimal(value: f64, places: usize) -> String {
    format!("{value:.places$}").replace('.', ",")
}

/// A size as it is said: "850 kB", "22,1 MB", "1,24 GB".
pub fn size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = bytes as f64 / MB;
    if mb < 1.0 {
        format!("{} kB", bytes.div_ceil(1024))
    } else if mb < 100.0 {
        format!("{} MB", decimal(mb, 1))
    } else if mb < 1000.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{} GB", decimal(mb / 1024.0, 2))
    }
}

/// A count with its noun: "1 versión", "3 versiones".
pub fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A path cut in its middle if it is longer than `most` letters.
pub fn shortened(path: &str, most: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= most.max(5) {
        return path.to_string();
    }
    let keep = most.max(5) - 1;
    let head: String = chars[..keep / 3].iter().collect();
    let tail: String = chars[chars.len() - (keep - keep / 3)..].iter().collect();
    format!("{head}…{tail}")
}

/// The first letter in capitals.
pub fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// The first line of a text that says something.
pub fn first_line(text: &str) -> Option<String> {
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_number_is_its_date() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(11_017), (2000, 3, 1));
        assert_eq!(civil(20_730), (2026, 10, 4));
        assert_eq!(civil(20_819), (2027, 1, 1));
    }

    /// 2026-10-04 16:46:00 UTC.
    const THEN: u64 = 20_730 * 86_400 + 16 * 3600 + 46 * 60;

    #[test]
    fn a_moment_is_said_on_this_machine_s_clock() {
        let (utc, madrid) = (Clock::default(), Clock { offset: 7200 });
        assert_eq!(utc.moment(THEN, THEN), "4 oct 16:46");
        assert_eq!(madrid.moment(THEN, THEN), "4 oct 18:46");
        // (past midnight on that clock it is already the next day)
        assert_eq!(madrid.moment(THEN + 6 * 3600, THEN), "5 oct 0:46");
        assert_eq!(Clock { offset: -5 * 3600 }.moment(THEN - 12 * 3600, THEN), "3 oct 23:46");
        // another year's is told with it
        assert_eq!(madrid.moment(THEN - 365 * 86_400, THEN), "4 oct 2025 18:46");
    }

    #[test]
    fn how_long_ago_is_said_by_the_days_of_this_clock() {
        let c = Clock { offset: 7200 };
        assert_eq!(c.ago(THEN, THEN + 3600), "hoy 18:46");
        // (one hour before this clock's midnight, and two hours after it: yesterday)
        assert_eq!(c.ago(THEN + 4 * 3600, THEN + 7 * 3600), "ayer 22:46");
        assert_eq!(c.ago(THEN, THEN + 86_400), "ayer 18:46");
        assert_eq!(c.ago(THEN, THEN + 3 * 86_400), "hace 3 días");
        assert_eq!(c.ago(THEN, THEN + 6 * 86_400), "hace 6 días");
        assert_eq!(c.ago(THEN, THEN + 7 * 86_400), "4 oct 18:46");
        assert_eq!(c.ago(THEN, THEN + 400 * 86_400), "4 oct 2026 18:46");
    }

    #[test]
    fn the_clock_s_distance_from_utc_is_read_from_what_it_says() {
        assert_eq!(hour_minute("18:46:02,31\r\n"), Some((18, 46)));
        assert_eq!(hour_minute(" 9:05:03.12"), Some((9, 5)));
        assert_eq!(hour_minute("23.59.59"), Some((23, 59)));
        assert_eq!(hour_minute("%TIME%"), None);
        assert_eq!(hour_minute("25:00"), None);
        assert_eq!(hour_minute(""), None);
        // 16:46 UTC
        assert_eq!(offset((18, 46), THEN), 7200);
        assert_eq!(offset((16, 46), THEN), 0);
        assert_eq!(offset((11, 46), THEN), -5 * 3600);
        assert_eq!(offset((22, 16), THEN), 5 * 3600 + 1800);
        // (the two clocks read a minute apart, and across midnight)
        assert_eq!(offset((18, 47), THEN + 59), 7200);
        assert_eq!(offset((18, 45), THEN), 7200);
        assert_eq!(offset((1, 46), THEN), 9 * 3600);
        assert_eq!(offset((6, 46), THEN), -10 * 3600);
        assert_eq!(offset((5, 46), THEN), -11 * 3600);
        assert_eq!(offset((16, 0), 3600), -9 * 3600);
        assert_eq!(offset((15, 0), 3600), 14 * 3600);
        assert_eq!(offset((0, 30), 23 * 3600 + 30 * 60), 3600);
    }

    #[test]
    fn spans_sizes_and_counts_are_said_short() {
        assert_eq!([span(0), span(45), span(60), span(12 * 60 + 59), span(3600), span(3 * 3600 + 20 * 60 + 5)], ["0 s", "45 s", "1 min", "12 min", "1 h", "3 h 20 min"]);
        assert_eq!([size(0), size(1), size(870_000), size(23_073_792), size(104_857_600), size(583 * 1024 * 1024), size(1300 * 1024 * 1024)], ["0 kB", "1 kB", "850 kB", "22,0 MB", "100 MB", "583 MB", "1,27 GB"]);
        assert_eq!(decimal(8.04, 1), "8,0");
        assert_eq!(plural(1, "versión", "versiones"), "1 versión");
        assert_eq!(plural(32, "versión", "versiones"), "32 versiones");
    }

    #[test]
    fn a_long_path_is_cut_in_its_middle() {
        assert_eq!(shortened("C:\\juegos\\luna", 40), "C:\\juegos\\luna");
        let cut = shortened("C:\\Users\\Fernando\\Desktop\\spacegame\\MIGRACION", 24);
        assert_eq!(cut.chars().count(), 24);
        assert!(cut.starts_with("C:\\User") && cut.ends_with("MIGRACION") && cut.contains('…'), "{cut}");
        assert_eq!(shortened("ñandúñandúñandú", 7).chars().count(), 7);
    }

    #[test]
    fn a_text_s_first_letter_and_first_line() {
        assert_eq!(capitalised("las patas son"), "Las patas son");
        assert_eq!(capitalised("ñu"), "Ñu");
        assert_eq!(capitalised(""), "");
        assert_eq!(first_line("\n  \nopción desconocida: --pantalla\n\nLUNA (migración Rust)\n").as_deref(), Some("opción desconocida: --pantalla"));
        assert_eq!(first_line("  \n\n"), None);
    }
}
