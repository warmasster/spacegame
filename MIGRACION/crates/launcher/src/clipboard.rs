//! The system's clipboard, through the system's own shell (the launcher has no clipboard crate,
//! and may not call the system itself): PowerShell is told what to put on it, and tells what
//! there is to paste. It shows no window. What goes either way goes as base64 of its UTF-8 —
//! letters and digits only — so that no code page and no quoting comes between the two.
use crate::launch;

/// The letters of base64, in the order of their values.
const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Some bytes as base64.
pub fn to_base64(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let bits = group.iter().fold(0u32, |bits, byte| (bits << 8) | u32::from(*byte)) << (8 * (3 - group.len()));
        for n in 0..4 {
            // (a group of one byte fills two letters, one of two fills three; the rest is padding)
            text.push(if n <= group.len() { char::from(LETTERS[(bits >> (18 - 6 * n)) as usize & 63]) } else { '=' });
        }
    }
    text
}

/// The bytes a base64 text stands for; none if it is not one.
pub fn base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| LETTERS.iter().position(|letter| *letter == c).map(|at| at as u32);
    let letters: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut bytes = Vec::with_capacity(letters.len() * 3 / 4);
    for group in letters.chunks(4) {
        if group.len() == 1 {
            return None;
        }
        let mut bits = 0u32;
        for c in group {
            bits = (bits << 6) | value(*c)?;
        }
        bits <<= 6 * (4 - group.len());
        bytes.extend_from_slice(&bits.to_be_bytes()[1..group.len()]);
    }
    Some(bytes)
}

/// What PowerShell is told to put a text on the clipboard.
pub fn copy_command(text: &str) -> String {
    format!("Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{}')))", to_base64(text.as_bytes()))
}

/// Put a text on the clipboard. (PowerShell is started and left to it: it takes it some tenths
/// of a second, and nobody waits.)
pub fn copy(text: &str) -> std::io::Result<()> {
    launch::quietly("powershell", &["-NoProfile", "-NonInteractive", "-Command", &copy_command(text)])
}

/// What PowerShell is asked: the clipboard's text.
const ASK: &str = "[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string](Get-Clipboard -Raw)))";

/// What is typed into a line from what was pasted: its first line that says something, without
/// control characters, `most` letters at most.
pub fn line(pasted: &str, most: usize) -> String {
    let first = pasted.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    first.chars().filter(|c| !c.is_control()).take(most).collect()
}

/// The clipboard's text; or why it could not be had, as it is told. (This waits for PowerShell,
/// some tenths of a second: it is for a thread of its own.)
pub fn paste() -> Result<String, String> {
    let said = launch::said("powershell", &["-NoProfile", "-NonInteractive", "-Command", ASK]).ok_or("no se pudo preguntar al sistema (PowerShell)")?;
    let bytes = base64(&said).ok_or("el sistema no dijo qué hay en el portapapeles")?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_is_written_and_read_back_into_its_bytes() {
        for (bytes, text) in [(&b""[..], ""), (b"M", "TQ=="), (b"Ma", "TWE="), (b"Man", "TWFu"), (b"192.168.1.67:47600", "MTkyLjE2OC4xLjY3OjQ3NjAw"), ("Ñandú 🚀".as_bytes(), "w5FhbmTDuiDwn5qA"), (&[0xff, 0xfe, 0xfd, 0xfc][..], "//79/A==")] {
            assert_eq!(to_base64(bytes), text);
            assert_eq!(base64(text).as_deref(), Some(bytes));
        }
        assert_eq!(base64("TWFu\r\n").as_deref(), Some(&b"Man"[..]));
        assert_eq!(base64("TQ").as_deref(), Some(&b"M"[..]));
        assert_eq!(base64("no es base64!"), None);
        assert_eq!(base64("TWFuT"), None);
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(base64(&to_base64(&all)), Some(all));
    }

    #[test]
    fn what_is_copied_goes_as_letters_that_need_no_quoting() {
        assert_eq!(copy_command("192.168.1.67:47600"), "Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('MTkyLjE2OC4xLjY3OjQ3NjAw')))");
        // (whatever the text holds — quotes, a dollar, lines — the shell sees letters, digits, + / =)
        let command = copy_command("--nombre \"Ana $x\" 'y'\r\n`z` ñ");
        let inside = command.split('\'').nth(1).unwrap();
        assert!(inside.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=')), "{inside}");
        assert_eq!(command.matches('\'').count(), 2);
        assert_eq!(base64(inside).map(|b| String::from_utf8(b).unwrap()).as_deref(), Some("--nombre \"Ana $x\" 'y'\r\n`z` ñ"));
    }

    #[test]
    fn what_is_pasted_into_a_line_is_one_clean_line() {
        assert_eq!(line("  192.168.1.67:47600 \r\n", 80), "192.168.1.67:47600");
        assert_eq!(line("\r\n\r\n luna.example:47611\r\notra cosa\r\n", 80), "luna.example:47611");
        assert_eq!(line("con\ttabulador\u{7}", 80), "contabulador");
        assert_eq!(line("ñandúñandú", 5), "ñandú");
        assert_eq!(line("", 80), "");
    }

    /// (Run by hand, `--ignored`: it reads the clipboard of whoever sits at the machine — nothing
    /// of it is shown — and changes nothing.)
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn the_system_tells_what_there_is_to_paste() {
        assert!(paste().is_ok());
    }

    /// PowerShell reads back what it is told the way `copy` tells it (written out instead of put
    /// on the clipboard: the clipboard is whoever sits at the machine's, and is not touched).
    #[cfg(windows)]
    #[test]
    fn the_shell_reads_what_it_is_told_as_it_was_written() {
        let text = "SELENE — diagnóstico\nGráfica: «RTX» $x 'y' \"z\" 🚀";
        let command = copy_command(text).replace("Set-Clipboard -Value ", "[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes");
        let said = launch::said("powershell", &["-NoProfile", "-NonInteractive", "-Command", &format!("{command})")]).unwrap();
        assert_eq!(base64(&said).map(|b| String::from_utf8(b).unwrap()).as_deref(), Some(text), "{said}");
    }
}
