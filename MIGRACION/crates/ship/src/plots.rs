//! Pictures a ship's systems draw for its screens: lines, marks and a few words in a square
//! (−1..1 each way, y up), each under a name. A screen's instrument asks for one by that name
//! (`{ "tipo": "trazas", "canal": "tac.radar" }`, `lunar_controls::mfd`) and draws what is there:
//! the screen knows nothing of radars, and whoever draws knows nothing of screens. A new picture
//! is a name and whoever fills it.
//!
//! Nothing here allocates once warm: a plot is cleared and filled again (its lists keep their
//! room), and its words are bytes in place.
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Dot,
    Square,
    Diamond,
    Circle,
    Triangle,
    Cross,
    /// A chevron pointing out from the middle: something known only by its bearing.
    Caret,
}

#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub color: [u8; 3],
    /// 0 faint .. 1 bold.
    pub weight: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Mark {
    pub at: [f32; 2],
    pub shape: Shape,
    pub color: [u8; 3],
    /// Half its size (the square's units).
    pub size: f32,
    /// Chosen: a box round it.
    pub boxed: bool,
    /// A line from it (where it is going): zero for none.
    pub tail: [f32; 2],
}

/// A few letters, in place.
#[derive(Clone, Copy, Debug)]
pub struct Text {
    bytes: [u8; 24],
    len: u8,
}

impl Default for Text {
    fn default() -> Self {
        Text { bytes: [0; 24], len: 0 }
    }
}

impl Text {
    pub fn as_str(&self) -> &str {
        // (only whole characters are ever written)
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or("")
    }
}

impl Write for Text {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for c in s.chars() {
            let n = c.len_utf8();
            let at = usize::from(self.len);
            if at + n > self.bytes.len() {
                break;
            }
            c.encode_utf8(&mut self.bytes[at..at + n]);
            self.len += n as u8;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Word {
    pub at: [f32; 2],
    /// Height of its letters (the square's units).
    pub size: f32,
    pub color: [u8; 3],
    /// 0 from its left, 0.5 centred, 1 from its right.
    pub align: f32,
    pub text: Text,
}

#[derive(Clone, Debug)]
pub struct Plot {
    pub name: &'static str,
    pub lines: Vec<Line>,
    pub marks: Vec<Mark>,
    pub words: Vec<Word>,
}

impl Plot {
    pub fn new(name: &'static str) -> Plot {
        Plot { name, lines: Vec::new(), marks: Vec::new(), words: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.marks.clear();
        self.words.clear();
    }

    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], color: [u8; 3], weight: f32) {
        self.lines.push(Line { a, b, color, weight });
    }

    /// A ring round `c`, of `n` sides, from `a0` to `a1` (rad, 0 up, clockwise).
    pub fn arc(&mut self, c: [f32; 2], r: f32, a0: f32, a1: f32, n: usize, color: [u8; 3], weight: f32) {
        let at = |a: f32| [c[0] + a.sin() * r, c[1] + a.cos() * r];
        for k in 0..n {
            let (u0, u1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
            self.line(at(a0 + (a1 - a0) * u0), at(a0 + (a1 - a0) * u1), color, weight);
        }
    }

    pub fn mark(&mut self, at: [f32; 2], shape: Shape, color: [u8; 3], size: f32) -> &mut Mark {
        self.marks.push(Mark { at, shape, color, size, boxed: false, tail: [0.0; 2] });
        let k = self.marks.len() - 1;
        &mut self.marks[k]
    }

    pub fn word(&mut self, at: [f32; 2], size: f32, color: [u8; 3], align: f32, args: std::fmt::Arguments) {
        let mut text = Text::default();
        let _ = text.write_fmt(args);
        self.words.push(Word { at, size, color, align, text });
    }
}

/// Every picture of a ship, by name.
#[derive(Clone, Debug, Default)]
pub struct Plots {
    pub list: Vec<Plot>,
}

impl Plots {
    pub fn get(&self, name: &str) -> Option<&Plot> {
        self.list.iter().find(|p| p.name == name)
    }

    /// The picture `name`, made if it is new.
    pub fn open(&mut self, name: &'static str) -> &mut Plot {
        let k = match self.list.iter().position(|p| p.name == name) {
            Some(k) => k,
            None => {
                self.list.push(Plot::new(name));
                self.list.len() - 1
            }
        };
        &mut self.list[k]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_fit_in_place_and_cut_at_whole_letters() {
        let mut p = Plot::new("x");
        p.word([0.0, 0.0], 0.1, [255; 3], 0.5, format_args!("{:.1} km", 12.34));
        assert_eq!(p.words[0].text.as_str(), "12.3 km");
        p.word([0.0, 0.0], 0.1, [255; 3], 0.5, format_args!("ññññññññññññññññññññ"));
        assert_eq!(p.words[1].text.as_str().chars().count(), 12);
        p.clear();
        assert!(p.words.capacity() >= 2 && p.words.is_empty());
    }
}
